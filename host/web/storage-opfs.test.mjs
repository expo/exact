// @ref LLP 1027 D10 — database connections hand off the path lock and the OPFS handles together.
import { afterAll, afterEach, beforeAll, expect } from 'bun:test';
import { resolve } from 'node:path';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { webkit } from 'playwright-core';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { engineTests } from '../../scripts/coverage.mjs';
import { hostFixtureFile } from './tests/glue-fixture.mjs';

const engines = ['chromium', 'firefox', 'webkit'], tests = await engineTests(engines);
const sqlite = resolve(import.meta.dir, '../../node_modules/@sqlite.org/sqlite-wasm/dist');
let server;
const held = new Set();
afterEach(() => { for (const finish of held) finish(); });
beforeAll(() => {
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<!doctype html>', { headers: { 'content-type': 'text/html' } });
    if (path === '/hold') return new Promise(resolve => {
      const finish = () => { held.delete(finish); clearTimeout(timer); resolve(new Response('released')); };
      const timer = setTimeout(finish, 10000);
      held.add(finish);
      request.signal.addEventListener('abort', finish, { once: true });
    });
    if (path === '/cut.js') {
      const worker = hostFixtureFile(resolve(import.meta.dir, 'storage-worker.js'), 'utf8');
      return new Response(worker.replace("from './sqlite3.mjs'", "from './observed-sqlite.mjs'"), { headers: { 'content-type': 'text/javascript' } });
    }
    if (path === '/observed-sqlite.mjs') return new Response(observedSqlite, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/favicon.ico') return new Response(null, { status: 404 });
    if (path === '/capability.js') return new Response(`
      onmessage = async () => {
        try {
          const root = await navigator.storage.getDirectory(), name = crypto.randomUUID();
          const file = await root.getFileHandle(name, { create: true });
          const closed = (await file.createSyncAccessHandle()).close();
          await closed;
          await root.removeEntry(name);
          postMessage(!closed?.then);
        } catch { postMessage(false); }
      };`, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/storage-worker.js') {
      const source = hostFixtureFile(resolve(import.meta.dir, 'storage-worker.js'), 'utf8');
      return new Response(source.replace('db.checkRc(c.sqlite3_set_authorizer', `
        if (session.pool) postMessage({ durability: [db.selectValue('PRAGMA journal_mode'), db.selectValue('PRAGMA synchronous')] });
        db.checkRc(c.sqlite3_set_authorizer`), { headers: { 'content-type': 'text/javascript' } });
    }
    if (path === '/quota.js') return new Response(`
      import './storage-worker.js';
      const original = FileSystemDirectoryHandle.prototype.getDirectoryHandle;
      FileSystemDirectoryHandle.prototype.getDirectoryHandle = function(name, options) {
        if (name === 'exact-sqlite-sahpool') {
          FileSystemDirectoryHandle.prototype.getDirectoryHandle = original;
          throw new DOMException('pool quota', 'QuotaExceededError');
        }
        return original.call(this, name, options);
      };`, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/without-opfs.js') return new Response(
      "import './storage-worker.js'; delete globalThis.FileSystemFileHandle?.prototype.createSyncAccessHandle; delete AbortSignal.any;",
      { headers: { 'content-type': 'text/javascript' } });
    const file = path === '/sqlite3.mjs' ? resolve(sqlite, 'index.mjs')
      : path === '/sqlite3.wasm' ? resolve(sqlite, 'sqlite3.wasm') : resolve(import.meta.dir, path.slice(1));
    return new Response(hostFixtureFile(file), { headers: { 'content-type': path.endsWith('.wasm') ? 'application/wasm' : 'text/javascript' } });
  } });
});
afterAll(() => server?.stop(true));

async function tab(context, appId, worker) {
  const page = await context.newPage();
  await page.goto(server.url.href);
  await page.evaluate(({ appId, worker }) => {
    const w = new Worker(worker, { type: 'module' }), calls = new Map();
    let sequence = 0;
    globalThis.imports = 0;
    w.onmessage = ({ data }) => {
      if (data.imported) { globalThis.imports++; return; }
      if (data.cut) {
        w.terminate();
        for (const { reject } of calls.values()) reject(new Error('interrupted ' + data.cut));
        return;
      }
      if (data.durability) { globalThis.durability = data.durability; return; }
      const { resolve, reject } = calls.get(data.id);
      calls.delete(data.id);
      if (data.error) reject(Object.assign(new Error(data.error.message), data.error));
      else resolve(data.value);
    };
    w.onerror = e => { for (const { reject } of calls.values()) reject(new Error(e.message)); };
    globalThis.ask = (op, ...args) => new Promise((resolve, reject) => {
      calls.set(++sequence, { resolve, reject });
      w.postMessage({ client: 1, id: sequence, appId, op, args });
    });
  }, { appId, worker });
  return page;
}

for (const engine of engines) tests[engine](`two tabs hand off one committed SQLite file on either store (${engine})`, async () => {
  const browser = await suiteBrowser(engine);
  const profile = engine === 'webkit' ? mkdtempSync(resolve(tmpdir(), 'exact-storage-')) : null;
  try {
    for (const [worker, persistent] of [['/storage-worker.js', false], ['/without-opfs.js', false], ...(profile ? [['/storage-worker.js', true]] : [])]) {
      const context = persistent ? await webkit.launchPersistentContext(profile) : await browser.newContext();
      try {
        const [first, second] = await Promise.all([tab(context, 'tabs', worker), tab(context, 'tabs', worker)]);
        const path = 'app:/data/database';
        await first.evaluate(async path => {
          globalThis.db = await ask('open', path);
          await ask('execute', db, 'create table notes(n integer)', []);
        }, path);
        const waiting = second.evaluate(async path => { globalThis.db = await ask('open', path); }, path);
        await first.waitForFunction(async path => (await navigator.locks.query()).pending.some(lock => lock.name.endsWith(path)), path);
        await first.evaluate(async () => {
          await ask('execute', db, 'insert into notes values(42)', []);
          await ask('close', db);
        });
        await waiting;
        expect(await second.evaluate(async () => {
          const n = (await ask('query', db, 'select n from notes', [])).rows[0][0];
          await ask('close', db);
          return Number(n);
        })).toBe(42);
        const stored = await first.evaluate(async path => {
          const db = await new Promise(resolve => { const r = indexedDB.open('exact-storage:tabs'); r.onsuccess = () => resolve(r.result); });
          try {
            return await new Promise(resolve => {
              const r = db.transaction('files').objectStore('files').get(path);
              r.onsuccess = () => resolve(r.result.pool ? 'opfs' : 'indexeddb');
            });
          } finally { db.close(); }
        }, path);
        const capable = worker !== '/without-opfs.js' && await first.evaluate(() => new Promise(resolve => {
          const probe = new Worker('/capability.js');
          probe.onmessage = e => { probe.terminate(); resolve(e.data); };
          probe.postMessage(null);
        }));
        expect(stored).toBe(capable ? 'opfs' : 'indexeddb');
        if (capable) expect(await first.evaluate(() => globalThis.durability)).toEqual(['delete', 2]);
      } finally { await context.close(); }
    }
  } finally { await browser.close(); if (profile) rmSync(profile, { recursive: true, force: true }); }
}, 60_000);

tests.chromium('a failed pool installation leaves no visible database and a lost pool never opens empty', async () => {
  const browser = await suiteBrowser(), context = await browser.newContext(), path = 'app:/data/notes';
  try {
    const page = await tab(context, 'failure', '/quota.js');
    const first = await page.evaluate(async path => {
      const error = await ask('open', path).then(() => null, e => e.code);
      const { createFileStore } = await import('/storage-fs.js'), files = createFileStore('failure');
      try { return [error, await files.readdir('app:/data')]; }
      finally { files.close(); }
    }, path);
    expect(first).toEqual(['failed', []]);
    await page.evaluate(async path => {
      const db = await ask('open', path);
      await ask('execute', db, 'create table notes(n)', []);
      await ask('close', db);
      const root = await navigator.storage.getDirectory();
      await root.removeEntry('exact-sqlite-sahpool', { recursive: true });
    }, path);
    await page.close();
    const next = await tab(context, 'failure', '/storage-worker.js');
    expect(await next.evaluate(path => ask('open', path).then(() => 'opened', e => e.code), path)).toBe('failed');
  } finally { await context.close(); await browser.close(); }
}, 30_000);

// Stop the actual worker at each durable boundary. The wait keeps it at the named
// instruction until the page terminates it, including inside a synchronous VFS write.
const observedSqlite = `
import init from './sqlite3.mjs';
const cut = new URL(location.href).searchParams.get('cut');
function checkpoint(name) {
  if (cut !== name) return;
  postMessage({ cut });
  const hold = new XMLHttpRequest(); hold.open('GET', '/hold', false); hold.send();
  throw new Error('test did not terminate the worker');
}
const send = self.postMessage.bind(self);
self.postMessage = message => {
  if ('value' in message) checkpoint('after publication');
  send(message);
};
let associating = false, measuring = false;
const size = FileSystemSyncAccessHandle.prototype.getSize;
FileSystemSyncAccessHandle.prototype.getSize = function() {
  const bytes = size.call(this);
  if (measuring) {
    measuring = false;
    checkpoint('after size read');
    if (cut === 'wrong size') return bytes + 1;
  }
  return bytes;
};
const original = FileSystemSyncAccessHandle.prototype.write;
FileSystemSyncAccessHandle.prototype.write = function(bytes, opts) {
  if (cut === 'hot journal' && opts.at > 4096) {
    const header = new Uint8Array(16);
    this.read(header, { at: 4096 });
    if (new TextDecoder().decode(header).startsWith('SQLite format 3')) {
      original.call(this, bytes, opts);
      this.flush();
      checkpoint('hot journal');
    }
  }
  if (associating && cut === 'short association' && opts.at < 4096) return 0;
  if (associating && cut === 'changed payload' && bytes.byteLength >= 8192) {
    const altered = new Uint8Array(bytes).slice();
    altered[altered.length - 1] ^= 1; // A valid integer field with a different value.
    return original.call(this, altered, opts);
  }
  if (cut === 'partial copy' && bytes.byteLength >= 8192) {
    original.call(this, new Uint8Array(bytes.buffer ?? bytes, bytes.byteOffset ?? 0, 4096), opts);
    checkpoint('partial copy');
  }
  return original.call(this, bytes, opts);
};
let markers = 0;
const put = IDBObjectStore.prototype.put;
IDBObjectStore.prototype.put = function(value, ...args) {
  if (cut === 'metadata quota' && value.kind === 'sqlite-opfs' && ++markers === 3) {
    throw new DOMException('metadata quota', 'QuotaExceededError');
  }
  const request = put.call(this, value, ...args);
  if (value.kind === 'sqlite-opfs') checkpoint('before publication');
  return request;
};
export default async function(options) {
  const sqlite = await init(options), install = sqlite.installOpfsSAHPoolVfs;
  const control = sqlite.capi.sqlite3_file_control;
  sqlite.capi.sqlite3_file_control = (...args) => {
    if (args[2] === sqlite.capi.SQLITE_FCNTL_FILE_POINTER) measuring = true;
    return control(...args);
  };
  sqlite.installOpfsSAHPoolVfs = async (...args) => {
    const pool = await install(...args);
    const copy = pool.importDb.bind(pool);
    pool.importDb = (...args) => {
      checkpoint('before copy');
      associating = true;
      const size = copy(...args);
      associating = false;
      postMessage({ imported: true });
      checkpoint('after copy');
      return size;
    };
    return pool;
  };
  return sqlite;
}`;

tests.chromium('an interrupted import keeps one complete source until SQLite verifies its replacement', async () => {
  const browser = await suiteBrowser();
  try {
    for (const cut of ['before copy', 'partial copy', 'after copy', 'after size read', 'before publication', 'after publication', 'wrong size', 'short association', 'changed payload', 'corrupt source']) {
      const context = await browser.newContext(), path = 'app:/data/db';
      try {
        const seed = await tab(context, 'import', '/without-opfs.js');
        await seed.evaluate(async ({ path, corrupt }) => {
          const id = await ask('open', path);
          await ask('execute', id, 'create table notes(n)', []);
          await ask('execute', id, 'insert into notes values(123)', []);
          await ask('close', id);
          if (corrupt) {
            const { createFileStore } = await import('/storage-fs.js');
            const files = createFileStore('import'), bytes = new Uint8Array(await files.readFile(path));
            bytes[100] = 255; // Page one's b-tree header follows the 100-byte database header.
            await files.writeFile(path, bytes);
            files.close();
          }
        }, { path, corrupt: cut === 'corrupt source' });
        const attempt = await tab(context, 'import', '/cut.js?cut=' + encodeURIComponent(cut));
        const error = await attempt.evaluate(path => ask('open', path).then(() => '', e => e.message), path);
        if (cut === 'corrupt source') expect(error).toMatch(/malformed|integrity check/i);
        else if (cut === 'short association') expect(error).toMatch(/open|I.O/i);
        else if (cut === 'changed payload') expect(error).toContain('contents mismatch');
        else if (cut === 'wrong size') expect(error).toContain('size mismatch');
        else expect(error).toBe('interrupted ' + cut);
        await attempt.close();
        const image = await seed.evaluate(async path => {
          const { createFileStore } = await import('/storage-fs.js');
          const files = createFileStore('import');
          try { return (await files.databaseEntry(path, false)).entry.contents?.byteLength ?? 0; }
          finally { files.close(); }
        }, path);
        expect(image > 0).toBe(cut !== 'after publication');
        if (cut === 'corrupt source') continue;
        if (cut === 'after copy') await seed.evaluate(async path => {
          const db = await ask('open', path);
          await ask('execute', db, 'update notes set n=456', []);
          await ask('close', db);
        }, path);
        const peers = await Promise.all([tab(context, 'import', '/cut.js'), tab(context, 'import', '/cut.js')]);
        const reopened = await Promise.all(peers.map(page => page.evaluate(async path => {
          const id = await ask('open', path);
          const n = (await ask('query', id, 'select n from notes', [])).rows[0][0];
          await ask('close', id);
          return [Number(n), globalThis.imports];
        }, path)));
        expect(reopened.map(r => r[0])).toEqual(cut === 'after copy' ? [456, 456] : [123, 123]);
        expect(reopened.reduce((n, r) => n + r[1], 0)).toBe(cut === 'after publication' ? 0 : 1);
      } finally { await context.close(); }
    }
  } finally { await browser.close(); }
}, 90_000);

tests.chromium('empty files and Blob images import, and removing a staged source reclaims its pool', async () => {
  const browser = await suiteBrowser(), context = await browser.newContext();
  try {
    const fallback = await tab(context, 'legacy', '/without-opfs.js');
    const pooled = await tab(context, 'legacy', '/storage-worker.js');
    await fallback.evaluate(async () => {
      const empty = await ask('open', 'app:/data/empty');
      await ask('close', empty);
      const full = await ask('open', 'app:/data/full');
      await ask('execute', full, 'create table notes(n)', []);
      await ask('close', full);
      const { createFileStore } = await import('/storage-fs.js'), files = createFileStore('legacy');
      try {
        const blob = new Blob([await files.readFile('app:/data/full')]);
        await files.putBlob('app:/data/blob', blob);
      } finally { files.close(); }
    });
    expect(await pooled.evaluate(async () => {
      const counts = [];
      for (const path of ['empty', 'blob']) {
        const db = await ask('open', 'app:/data/' + path);
        counts.push(Number((await ask('query', db, "select count(*) from sqlite_schema where name='notes'", [])).rows[0][0]));
        await ask('close', db);
      }
      return counts;
    })).toEqual([0, 1]);
    const attempt = await tab(context, 'legacy', '/cut.js?cut=after%20copy');
    expect(await attempt.evaluate(() => ask('open', 'app:/data/full').catch(e => e.message))).toBe('interrupted after copy');
    await attempt.close();
    const remaining = await fallback.evaluate(async () => {
      const root = await (await navigator.storage.getDirectory()).getDirectoryHandle('exact-sqlite-sahpool');
      const pools = async () => { const names = []; for await (const name of root.keys()) names.push(name); return names.length; };
      const before = await pools();
      const { createFileStore } = await import('/storage-fs.js'), files = createFileStore('legacy');
      try {
        await files.rename('app:/data/full', 'app:/data/moved');
        const db = await ask('open', 'app:/data/moved');
        const count = Number((await ask('query', db, "select count(*) from sqlite_schema where name='notes'", [])).rows[0][0]);
        await ask('close', db);
        return [before - await pools(), count];
      } finally { files.close(); }
    });
    expect(remaining).toEqual([1, 1]);
  } finally { await context.close(); await browser.close(); }
}, 30_000);

const fileGrants = {
  entries: [[1, 'fs.read app:/data', ['fs-read', 'app:', 'data'], null],
    [2, 'fs.write app:/data', ['fs-write', 'app:', 'data'], null]],
  error: null, keepsNoAnswers: false, seal: '855d78ac7052db66', version: 1,
};

// The filesystem in a second page uses only its file grants, including for a live snapshot.
for (const engine of engines) tests[engine](`open and closed database paths are files (${engine})`, async () => {
  const browser = await suiteBrowser(engine);
  const profile = engine === 'webkit' ? mkdtempSync(resolve(tmpdir(), 'exact-db-file-')) : null;
  try {
    for (const worker of ['/storage-worker.js', '/without-opfs.js']) {
      const context = profile && worker === '/storage-worker.js'
        ? await webkit.launchPersistentContext(profile) : await browser.newContext();
      try {
        const [writer, reader] = await Promise.all([tab(context, 'files', worker), tab(context, 'files', worker)]);
        await reader.evaluate(async grants => {
          const { createFileSystem } = await import('/storage-fs.js');
          globalThis.fs = createFileSystem('files', grants);
          await fs.mkdir('app:/data/notes');
        }, fileGrants);
        const path = 'app:/data/notes/live', copy = 'app:/data/snapshot';
        await writer.evaluate(async path => {
          globalThis.db = await ask('open', path);
          await ask('execute', db, 'create table notes(n)', []);
          await ask('execute', db, 'insert into notes values(42)', []);
          await ask('transaction', db, [{ sql: 'update notes set n=99' }, { sql: 'insert into missing values(1)' }], false)
            .then(() => { throw new Error('invalid transaction committed'); }, () => {});
        }, path);
        const live = await reader.evaluate(async ({ path, copy }) => {
          const image = new Uint8Array(await fs.readFile(path));
          const stat = await fs.stat(path);
          await fs.copyFile(path, copy);
          const refused = async fn => fn.then(() => 'succeeded', e => e.code);
          const codes = await Promise.all([fs.writeFile(path, new Uint8Array()), fs.appendFile(path, image),
            fs.rm(path), fs.rename(path, copy), fs.copyFile(copy, path), fs.rm('app:/data/notes')].map(refused));
          return [new TextDecoder().decode(image.subarray(0, 15)), stat.isFile && stat.size === image.length, codes];
        }, { path, copy });
        expect(live).toEqual(['SQLite format 3', true, Array(6).fill('EBUSY')]);
        await writer.evaluate(async () => {
          await ask('execute', db, 'update notes set n=43', []);
          await ask('close', db);
        });
        const closed = await reader.evaluate(async ({ path, copy }) => {
          const read = async path => {
            const db = await ask('open', path);
            try { return Number((await ask('query', db, 'select n from notes', [])).rows[0][0]); }
            finally { await ask('close', db); }
          };
          const snapshot = await read(copy);
          await fs.rename('app:/data/notes', 'app:/data/moved');
          const moved = 'app:/data/moved/live';
          const latest = await read(moved);
          await fs.copyFile(copy, moved);
          const restored = await read(moved);
          await fs.rm(copy);
          await fs.rename(moved, copy);
          const before = (await fs.stat(copy)).size;
          await fs.appendFile(copy, new Uint8Array([65]));
          const appended = new Uint8Array(await fs.readFile(copy));
          await fs.atomicWriteFile(copy, new Uint8Array([8, 9]));
          const replaced = Array.from(new Uint8Array(await fs.readFile(copy)));
          await fs.rm('app:/data/moved');
          await fs.rm(copy);
          return [snapshot, latest, restored, appended.length === before + 1 && appended.at(-1) === 65,
            replaced, await fs.readdir('app:/data')];
        }, { path, copy });
        expect(closed).toEqual([42, 43, 42, true, [8, 9], []]);
      } finally { await context.close(); }
    }
  } finally { await browser.close(); if (profile) rmSync(profile, { recursive: true, force: true }); }
}, 90_000);

function parkSnapshot(page) {
  return page.evaluate(() => {
    const post = Worker.prototype.postMessage;
    globalThis.captured = new Promise(captured => {
      Worker.prototype.postMessage = function(data, ...rest) {
        if (data.op === 'image') {
          Worker.prototype.postMessage = post;
          const deliver = this.onmessage;
          this.onmessage = async event => {
            if (event.data.client === data.client && event.data.id === data.id) {
              this.onmessage = deliver;
              await new Promise(resume => { globalThis.resume = resume; captured(); });
            }
            deliver.call(this, event);
          };
        }
        return post.call(this, data, ...rest);
      };
    });
  });
}

tests.chromium('opposite database copies publish snapshots in a serial order', async () => {
  const browser = await suiteBrowser(), context = await browser.newContext();
  try {
    const pages = await Promise.all([tab(context, 'copies', '/storage-worker.js'), tab(context, 'copies', '/storage-worker.js')]);
    const paths = ['app:/data/a', 'app:/data/b'];
    await Promise.all(pages.map((page, i) => page.evaluate(async ({ grants, path, n }) => {
      globalThis.fs = (await import('/storage-fs.js')).createFileSystem('copies', grants);
      const db = await ask('open', path);
      await ask('execute', db, 'create table notes(n)', []);
      await ask('execute', db, 'insert into notes values(?)', [n]);
      await ask('close', db);
    }, { grants: fileGrants, path: paths[i], n: i + 1 })));
    await Promise.all(pages.map(parkSnapshot));
    const copies = pages.map((page, i) => page.evaluate(([from, to]) => fs.copyFile(from, to), [paths[i], paths[1 - i]]));
    await Promise.all(pages.map(page => page.evaluate(() => captured)));
    await Promise.all(pages.map(page => page.evaluate(() => resume())));
    await Promise.all(copies);
    const values = await Promise.all(pages.map((page, i) => page.evaluate(async path => {
      const db = await ask('open', path);
      try { return Number((await ask('query', db, 'select n from notes', [])).rows[0][0]); }
      finally { await ask('close', db); }
    }, paths[i])));
    expect([1, 2]).toContain(values[0]);
    expect(values[1]).toBe(values[0]);
  } finally { await context.close(); await browser.close(); }
}, 60_000);

tests.chromium('a refused file metadata write leaves the SQL update uncommitted', async () => {
  const browser = await suiteBrowser(), context = await browser.newContext();
  try {
    const page = await tab(context, 'metadata', '/cut.js?cut=metadata%20quota');
    expect(await page.evaluate(async () => {
      const path = 'app:/data/notes', db = await ask('open', path);
      await ask('execute', db, 'create table notes(n)', []);
      const result = await ask('execute', db, 'insert into notes values(7)', []).then(() => 'ok', e => e.message);
      await ask('close', db);
      const reopened = await ask('open', path);
      try { return [result, Number((await ask('query', reopened, 'select count(*) from notes', [])).rows[0][0])]; }
      finally { await ask('close', reopened); }
    })).toEqual(['metadata quota', 0]);
  } finally { await context.close(); await browser.close(); }
}, 30_000);

tests.chromium('a filesystem copy after a worker crash rolls back the hot journal', async () => {
  const browser = await suiteBrowser(), context = await browser.newContext(), path = 'app:/data/notes';
  try {
    const reader = await tab(context, 'journal', '/storage-worker.js');
    await reader.evaluate(async path => {
      const db = await ask('open', path);
      await ask('execute', db, 'create table notes(n)', []);
      await ask('execute', db, 'insert into notes values(42)', []);
      await ask('close', db);
    }, path);
    const writer = await tab(context, 'journal', '/cut.js?cut=hot%20journal');
    expect(await writer.evaluate(async path => {
      const db = await ask('open', path);
      return ask('execute', db, 'update notes set n=99', []).catch(e => e.message);
    }, path)).toBe('interrupted hot journal');
    expect(await reader.evaluate(async ({ path, grants }) => {
      const fs = (await import('/storage-fs.js')).createFileSystem('journal', grants);
      const to = 'app:/data/snapshot';
      await fs.copyFile(path, to);
      const db = await ask('open', to);
      try { return Number((await ask('query', db, 'select n from notes', [])).rows[0][0]); }
      finally { await ask('close', db); fs.dispose(); }
    }, { path, grants: fileGrants })).toBe(42);
  } finally { await context.close(); await browser.close(); }
}, 30_000);

tests.chromium('copy publishes before a competing SQL update and a rejected statement preserves mtime', async () => {
  const browser = await suiteBrowser(), context = await browser.newContext();
  const from = 'app:/data/from', to = 'app:/data/to';
  try {
    const [writer, reader] = await Promise.all([tab(context, 'order', '/storage-worker.js'), tab(context, 'order', '/storage-worker.js')]);
    await writer.evaluate(async path => {
      globalThis.db = await ask('open', path);
      await ask('execute', db, 'create table notes(n)', []);
      await ask('execute', db, 'insert into notes values(1)', []);
    }, from);
    await reader.evaluate(async ({ grants, to }) => {
      globalThis.fs = (await import('/storage-fs.js')).createFileSystem('order', grants);
      await fs.writeFile(to, new Uint8Array([0]));
    }, { grants: fileGrants, to });
    await parkSnapshot(reader);
    const copy = reader.evaluate(([from, to]) => fs.copyFile(from, to), [from, to]);
    await reader.evaluate(() => captured);
    const update = writer.evaluate(() => ask('execute', db, 'update notes set n=2', []));
    await reader.waitForFunction(async () => (await navigator.locks.query()).pending.some(
      lock => lock.name.startsWith('exact-sqlite-mutation:') && lock.mode === 'exclusive'));
    expect(await reader.evaluate(async to => Array.from(new Uint8Array(await fs.readFile(to))), to)).toEqual([0]);
    await reader.evaluate(() => resume());
    await Promise.all([copy, update]);
    const modified = await reader.evaluate(async path => (await fs.stat(path)).modifiedMs, from);
    await writer.evaluate(() => ask('execute', db, 'invalid sql', []).then(() => { throw new Error('invalid SQL accepted'); }, () => {}));
    expect(await reader.evaluate(async path => (await fs.stat(path)).modifiedMs, from)).toBe(modified);
    expect(await writer.evaluate(async to => {
      const copy = await ask('open', to);
      try {
        return await Promise.all([copy, db].map(async id => Number((await ask('query', id, 'select n from notes', [])).rows[0][0])));
      } finally { await ask('close', copy); await ask('close', db); }
    }, to)).toEqual([1, 2]);
  } finally { await context.close(); await browser.close(); }
}, 30_000);
