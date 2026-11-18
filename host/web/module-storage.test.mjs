import { hostFixtureFile } from './tests/glue-fixture.mjs';
// The page's module realm (module-glue.js) over the real prelude, in a vm
// context standing in for its iframe, and its worker placement on a Worker,
// each with a scripted file store.
import { expect, test } from 'bun:test';
import { closeSync, cpSync, fsyncSync, mkdtempSync, openSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import vm from 'node:vm';
import { modulePrelude } from '../web-js/glue.mjs';
import { isolatedAbortSignal, moduleRealm, race, secretEnvironment, TASK_DELAYED_LOCKS } from './module-storage-harness.mjs';

const ROOT = resolve(import.meta.dir, '../..');
// The runner's normalized set for `net.fetch https://api.castle.test` (as tests/it/request.rs's batch carries it).
const GRANTS = {version:1,entries:[[1,'net.fetch https://api.castle.test',['fetch','https','api.castle.test',443],null]],error:null,seal:'825ec7a8503217b3'};

test('the image waiter cannot expose a raw database behind the storage queue', async () => {
  const realm = vm.createContext({ URL, TextEncoder, TextDecoder, AbortController, AbortSignal: isolatedAbortSignal(), crypto,
    __exact_host: () => '', console: { error() {} } });
  vm.runInContext(readFileSync(resolve(ROOT, 'js/src/binary.js'), 'utf8'), realm);
  vm.runInContext(modulePrelude(ROOT), realm);
  vm.runInContext(`
    var executed = [], release, reached, probe;
    var held = new Promise(r => release = r), entered = new Promise(r => reached = r);
    __exact_storage = {
      sqlite: { open: () => Promise.resolve({ execute: sql => { executed.push(sql); return Promise.resolve(); } }) },
      fs: { atomicWriteFile: () => { reached(); return held; } }
    };
    __exact_install_storage();
    exact = { answer(source, args, store, storage) {
      var opened = storage.sqlite.open('app:/data/notes.db');
      var raw = globalThis.__exact_storage_promise?.();
      return globalThis.result = opened.then(db => {
        var first = storage.fs.atomicWriteFile('app:/data/value', new Uint8Array([1]));
        var next = db.execute('queued');
        probe = raw ? raw.then(db => db.execute('bypass')) : Promise.resolve();
        return Promise.all([first, next, probe]);
      });
    } };
  `, realm);
  realm.__exact_call('work', '[]', 'answer');
  try {
    await realm.entered;
    await realm.probe;
    expect(Array.from(realm.executed)).toEqual([]);
  } finally {
    realm.release();
    await realm.result;
  }
  expect(Array.from(realm.executed)).toEqual(['queued']);
});

for (const phase of ['sync', 'rename']) test(`a timed-out atomic write held at ${phase} keeps its queue slot`, async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-storage-timeout-'));
  const path = resolve(dir, 'value'), temporary = text => resolve(dir, 'staged-' + text);
  const realm = vm.createContext({ URL, TextEncoder, TextDecoder, AbortController, AbortSignal: isolatedAbortSignal(), crypto,
    __exact_host: () => '', console: { error() {} } });
  vm.runInContext(readFileSync(resolve(ROOT, 'js/src/binary.js'), 'utf8'), realm);
  vm.runInContext(modulePrelude(ROOT), realm);
  realm.__exact_main_thread();
  realm.phase = phase;
  realm.disk = {
    stage: (text, bytes) => writeFileSync(temporary(text), bytes),
    sync: text => {
      const fd = openSync(temporary(text), 'r+');
      try { fsyncSync(fd); } finally { closeSync(fd); }
    },
    rename: text => renameSync(temporary(text), path),
  };
  try {
    vm.runInContext(`
      var started = [], rejected = [], abandoned = [], reserved = 0, release, reached;
      __exact_reserve_storage = () => ++reserved;
      __exact_abandon_storage = token => abandoned.push(token);
      var held = new Promise(r => release = r), entered = new Promise(r => reached = r);
      __exact_storage = { fs: { atomicWriteFile: async function(path, bytes) {
        var text = new TextDecoder().decode(bytes);
        started.push(text);
        disk.stage(text, bytes);
        if (text === 'first' && phase === 'sync') { reached(); await held; }
        disk.sync(text);
        if (text === 'first' && phase === 'rename') { reached(); await held; }
        disk.rename(text);
      } } };
      __exact_install_storage();
      exact = { grants: 'fs.read app:/data', answer(source, args, store, storage) {
        var write = text => storage.fs.atomicWriteFile('app:/data/value', new TextEncoder().encode(text));
        if (source === 'failed') {
          globalThis.failed = write('first').catch(e => {
            rejected.push('first');
            return write('reaction').catch(e => rejected.push('reaction'));
          });
          globalThis.queued = write('orphan').catch(e => rejected.push('orphan'));
          return Promise.all([failed, queued]);
        }
        return globalThis.successor = write('successor');
      } };
    `, realm);
    const first = JSON.parse(realm.__exact_call('failed', '[]', 'answer'));
    await realm.entered;
    const next = JSON.parse(realm.__exact_call('next', '[]', 'answer'));
    realm.__exact_storage_failed(String(first.call), JSON.stringify({ failed: { kind: 'Aborted', message: 'storage continuation timed out' } }));
    await Promise.all([realm.failed, realm.queued]);
    expect(Array.from(realm.started)).toEqual(['first']);
    expect(Array.from(realm.abandoned)).toEqual([2]);
    expect(Array.from(realm.rejected).sort()).toEqual(['first', 'orphan', 'reaction']);
    expect(JSON.parse(realm.__exact_settle(String(first.call))).tag).toBe(2);
    expect(JSON.parse(realm.__exact_settle(String(next.call))).waiting).toBe(true);
    expect(JSON.parse(realm.__exact_background())).toMatchObject({ head: true, queued: 1, inFlight: 1 });
    realm.release();
    await realm.successor;
    expect(Array.from(realm.started)).toEqual(['first', 'successor']);
    expect(readFileSync(path, 'utf8')).toBe('successor');
    expect(JSON.parse(realm.__exact_settle(String(next.call))).tag).toBe(0);
    expect(JSON.parse(realm.__exact_background())).toMatchObject({ head: false, queued: 0, inFlight: 0, done: 1, failed: 2, writes: 2 });
  } finally {
    realm.release?.();
    await realm.successor?.catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
});

// A realm whose `atomicWriteFile` holds the write of `first` until `release()`; `order` records what the store did.
function heldWriteRealm(answer) {
  const realm = vm.createContext({ URL, TextEncoder, TextDecoder, AbortController, AbortSignal: isolatedAbortSignal(), DOMException, crypto,
    __exact_host: () => '', console: { error() {} } });
  vm.runInContext(readFileSync(resolve(ROOT, 'js/src/binary.js'), 'utf8'), realm);
  vm.runInContext(modulePrelude(ROOT), realm);
  realm.__exact_main_thread();
  vm.runInContext(`
    var order = [], rejected = [], release, reached;
    var held = new Promise(r => release = r), entered = new Promise(r => reached = r);
    __exact_storage = { fs: {
      flock: () => Promise.resolve(7),
      unlock: h => { order.push('unlock ' + h); return Promise.resolve(); },
      atomicWriteFile: function (path, bytes) {
        var text = new TextDecoder().decode(bytes);
        order.push('write ' + text);
        if (text !== 'first') return Promise.resolve();
        reached();
        return held.then(() => { order.push('written first'); });
      },
    } };
    __exact_install_storage();
    var write = (storage, text) => storage.fs.atomicWriteFile('app:/data/' + text, new TextEncoder().encode(text));
    exact = { grants: '', answer: ${answer} };
  `, realm);
  return realm;
}
const TIMED_OUT = JSON.stringify({ failed: { kind: 'Aborted', message: 'storage continuation timed out' } });

test('a timed-out answer\'s lock release waits behind its write and still unlocks', async () => {
  const realm = heldWriteRealm(`function (source, args, store, storage) {
    return storage.fs.flock('app:/data/lock').then(lock =>
      write(storage, 'first').finally(() => {
        globalThis.unlocked = lock.unlock().then(() => 'released', e => 'refused: ' + e.message);
      }));
  }`);
  const first = JSON.parse(realm.__exact_call('locked', '[]', 'answer'));
  await realm.entered;
  realm.__exact_storage_failed(String(first.call), TIMED_OUT);
  await new Promise(r => setTimeout(r, 0));
  expect(JSON.parse(realm.__exact_settle(String(first.call))).tag).toBe(2);
  expect(Array.from(realm.order)).toEqual(['write first']);
  realm.release();
  expect(await realm.unlocked).toBe('released');
  expect(Array.from(realm.order)).toEqual(['write first', 'written first', 'unlock 7']);
  expect(JSON.parse(realm.__exact_background())).toMatchObject({ head: false, queued: 0, inFlight: 0 });
});

test('a let-go storage timeout rejects only the timed-out answer\'s queued work', async () => {
  const realm = heldWriteRealm(`function (source, args, store, storage) {
    var settled = text => p => p.then(() => {}, () => rejected.push(text));
    if (source === 'a') return Promise.all([settled('first')(write(storage, 'first')), settled('a2')(write(storage, 'a2'))]);
    return globalThis.second = settled('b')(write(storage, 'b'));
  }`);
  const a = JSON.parse(realm.__exact_call('a', '[]', 'answer'));
  await realm.entered;
  const b = JSON.parse(realm.__exact_call('b', '[]', 'answer'));
  expect(realm.__exact_forget(String(a.call))).toBe('storage');
  expect(realm.__exact_forget(String(b.call))).toBe('storage');
  expect(realm.__exact_let_go('failed', 'storage continuation timed out')).toBe('settled');
  await new Promise(r => setTimeout(r, 0));
  expect(Array.from(realm.rejected).sort()).toEqual(['a2', 'first']);
  // The timed-out write keeps its place as the module's; `b` starts once it completes.
  expect(realm.__exact_let_go('', '')).toBe('queued');
  expect(JSON.parse(realm.__exact_background())).toMatchObject({ head: true, queued: 1, inFlight: 1 });
  realm.release();
  await realm.second;
  expect(Array.from(realm.order)).toEqual(['write first', 'written first', 'write b']);
  expect(Array.from(realm.rejected).sort()).toEqual(['a2', 'first']);
  expect(realm.__exact_let_go('', '')).toBe('');
});

test('image writes notify only for a successful widget-readable destination', async () => {
  const realm = vm.createContext({ URL, TextEncoder, TextDecoder, AbortController, AbortSignal: isolatedAbortSignal(), crypto,
    __exact_host: () => '', console: { error() {} } });
  vm.runInContext(readFileSync(resolve(ROOT, 'js/src/binary.js'), 'utf8'), realm);
  vm.runInContext(modulePrelude(ROOT), realm);
  vm.runInContext(`
    exact = { grants: 'fs.read app:/data/output.jpg', answer(source, args, store, storage) {
      return globalThis.result = storage.fs[args[0]](args[1], args[2], {
        maxDimension: 32, maxBytes: 10000, rotate: 90
      });
    } };
    function image(from, to, transform) {
      if (from === 'app:/data/missing.jpg') throw new Error('missing image');
      const path = transform ? to + '.jpg' : to;
      return nativeReply
        ? (transform ? '32\\t32\\t100\\t' + path + '\\t{"type":"image/jpeg"}' : '32\\t32\\t100')
        : { path, type: 'image/jpeg', width: 32, height: 32, size: 100 };
    }
    __exact_storage = { fs: {
      compressImage: (from, to) => Promise.resolve(image(from, to, false)),
      transformImage: (from, to) => Promise.resolve(image(from, to, true))
    } };
    __exact_install_storage();
  `, realm);
  let writes = 0;
  for (const nativeReply of [false, true]) {
    realm.nativeReply = nativeReply;
    for (const method of ['compressImage', 'transformImage']) {
      const destination = method === 'transformImage' ? 'app:/data/output' : 'app:/data/output.jpg';
      for (const [from, to, changed, ok] of [
        ['app:/data/input.jpg', destination, true, true],
        ['app:/data/output.jpg', 'app:/cache/hidden', false, true],
        ['app:/data/missing.jpg', destination, false, false],
      ]) {
        const call = JSON.parse(realm.__exact_call('image', JSON.stringify([method, from, to]), 'answer'));
        await realm.result.catch(() => {});
        const reply = JSON.parse(realm.__exact_settle(String(call.call)));
        expect(reply.tag).toBe(ok ? 0 : 2);
        if (ok) expect(reply.value.path).toBe(method === 'transformImage' ? to + '.jpg' : to);
        if (changed) writes++;
        expect(JSON.parse(realm.__exact_background()).writes).toBe(writes);
      }
    }
  }
});

// An answer's storage operation queued behind another answer's parks; when
// that answer delivers its own (after its fetch came back), the parked one's
// operation is in flight and it is asked again (prelude.js `settle`, LLP 1097
// D3). Before, only a background delivery asked parked answers again, and the
// Bluesky clone's device settings never answered on the web.
test('a parked answer is asked again after another answer delivers its storage', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-module-storage-'));
  const saved = Object.fromEntries(['fetch', 'document', 'location'].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  try {
    for (const name of ['module-glue.js', 'storage.js', 'grant-admission.js', 'faults.js', 'navigation.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
    writeFileSync(resolve(dir, 'storage-environment.js'), `export const directories = Object.freeze({ data: 'app:/data', cache: 'app:/cache', temporary: 'app:/tmp' });
export const storageKey = () => 'k'; export const agentStorageRefusal = 'no store'; export const agentReads = []; export const emptyRead = () => []; export const agentSeed = () => null;
export const agentStream = () => null; export const keyStore = () => ({}); export const now = () => 0;
export { refuseCodeFromStrings } from ${JSON.stringify(resolve(ROOT, 'host/web/storage-environment.js'))};`);
    // Each read lands when the test says, in the order issued.
    writeFileSync(resolve(dir, 'storage-fs.js'), `export const createFileSystem = () => ({
  readFile: path => new Promise(ok => globalThis.reads.push(() => ok(new TextEncoder().encode(path)))), dispose() {} });`);
    const prelude = modulePrelude(ROOT);
    const binary = readFileSync(resolve(ROOT, 'js/src/binary.js'), 'utf8');
    const script = new TextEncoder().encode(`exact = { abi: 1, appId: 'test.storage', grants: 'net.fetch https://api.castle.test', answer(source, args, store, storage) {
  const read = path => storage.fs.readFile(path).then(bytes => new TextDecoder().decode(bytes));
  if (source === 'feed') { const file = read('app:/data/feed'); return fetch('https://api.castle.test/feed').then(r => r.text()).then(async text => text + ' ' + await file); }
  return read('app:/data/device');
} };`);
    const sha = new Bun.CryptoHasher('sha256').update(script).digest('hex');
    globalThis.reads = [];
    const win = vm.createContext({ console: { log() {}, info() {}, warn() {}, error() {}, debug() {} }, TextEncoder, TextDecoder, structuredClone, setTimeout, clearTimeout,
      queueMicrotask, URL, Headers, DOMException, crypto, indexedDB: null, MessageChannel, Uint8Array, ArrayBuffer, AbortController, AbortSignal: isolatedAbortSignal(), addEventListener() {} });
    win.window = win;
    for (const name of ['Promise', 'Error', 'SyntaxError', 'Object', 'Array', 'JSON', 'Symbol', 'Function', 'eval']) win[name] = vm.runInContext(name, win); // what storage.js builds in the realm
    win.document = { createElement: () => ({}), head: { append: el => vm.runInContext(el.textContent, win) } };
    const frame = { contentWindow: win, setAttribute() {}, remove() {} };
    Object.defineProperties(globalThis, {
      fetch: { configurable: true, writable: true, value: async url => { const r = new Response(String(url).endsWith('module-prelude.js') ? prelude : String(url).endsWith('module-binary.js') ? binary : 'never'); Object.defineProperty(r, 'url', { value: 'http://127.0.0.1/x' }); return r; } },
      document: { configurable: true, value: { createElement: () => frame, body: { append() {} } } },
      location: { configurable: true, value: { href: 'http://127.0.0.1/', origin: 'http://127.0.0.1' } },
    });
    globalThis.exact = {};
    const { createGrantSet } = await import(resolve(dir, 'grant-admission.js'));
    const glue = await import(resolve(dir, 'module-glue.js'));
    const grants = 'net.fetch https://api.castle.test';
    const receipt = new TextEncoder().encode(JSON.stringify({ version: 1, abi: 1, appId: 'test.storage', grants, web: { file: 'app.js', bytes: script.length, sha256: sha }, module: { sha256: 'b'.repeat(64) } }));
    const realm = await glue.prepare({ receipt, script }, { appId: 'test.storage', grants, grantSet: createGrantSet(GRANTS), placement: 'main' });
    const ask = (source, extra = {}) => glue.run(glue.call({ op: 'answer', id: realm.id, target: source, source, args: [], store: [], grants: [], ...extra }).continuation);
    const feed = ask('feed');
    const fetched = await feed; // the fetch: the turn goes, its read in flight
    expect(fetched.tag).toBe(1);
    const device = ask('device');
    await new Promise(r => setTimeout(r, 20)); // its read queued behind feed's: it parks
    const resumed = glue.run(glue.call({ op: 'resume', id: realm.id, target: 'feed', source: 'feed', args: [], store: [], grants: [], outcome: { response: { status: 200, headers: [], body: 'fresh' } } }).continuation);
    globalThis.reads.shift()();
    expect((await resumed).value).toBe('fresh app:/data/feed');
    await new Promise(r => setTimeout(r, 20));
    globalThis.reads.shift()?.();
    const answered = await Promise.race([device, new Promise(r => setTimeout(() => r('still parked'), 500))]);
    expect(answered.value).toBe('app:/data/device');
    realm.dispose();
  } finally {
    for (const [name, descriptor] of Object.entries(saved)) { if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name]; }
    delete globalThis.exact; delete globalThis.reads;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a module realm does not replace the test process AbortSignal.timeout', async () => {
  const timeout = Object.getOwnPropertyDescriptor(AbortSignal, 'timeout');
  const w = await moduleRealm(`return 'ready';`, { placement: 'main' });
  try {
    expect(Object.getOwnPropertyDescriptor(AbortSignal, 'timeout')).toEqual(timeout);
    expect((await w.ask('probe')).value).toBe('ready');
  } finally { w.done(); }
  expect(Object.getOwnPropertyDescriptor(AbortSignal, 'timeout')).toEqual(timeout);
});


// #128: the module's realm, the iframe's (module-glue.js `prepare`) and the worker's (module-worker.js `init`), runs no
// code from a string, as Hermes runs none, and keeps what a function needs of `Function` (storage-environment.js
// `refuseCodeFromStrings`). The worker still evaluates the prelude and the module with its own `eval`.
test.each(['main', 'worker'])('code from a string throws in the %s module realm as on Hermes', async (placement) => {
  const w = await moduleRealm(`const thrown = f => { try { f(); return 'ran'; } catch (e) { return e.name + ': ' + e.message; } };
  for (const name of ['Function', 'eval']) globalThis[name] = globalThis[name];
  const F = Function;
  const key = 'constructor';
  try { delete Function[key]; } catch {}
  return JSON.stringify([thrown(() => Object.getOwnPropertyDescriptor(globalThis, 'Function').value('return 1')), thrown(() => Function[key]('return 1')),thrown(() => Function('return 1')), thrown(() => new F('return 1')), thrown(() => globalThis.Function('return 1')),
    thrown(() => eval('1')), thrown(() => (0, eval)('1')), thrown(() => globalThis.eval('1')), thrown(() => eval.constructor('return 1')), thrown(() => Function.constructor('return 1')),
    Function instanceof Function, eval instanceof Function, typeof Function, ((a, b) => a + b).call(null, 1, 2), Function.prototype.bind.call((a) => a * 2, null)(3)]);`, { placement });
  try {
    const answered = await race(w.ask('code'));
    const refused = 'SyntaxError: Parsing source code unsupported: a data source runs no code from a string (Function, eval), as on Hermes';
    expect(JSON.parse(answered.value)).toEqual([...Array(10).fill(refused), true, true, 'function', 3, 6]);
  } finally { w.done(); }
});

test('background storage counters survive a turn after an answer abandons its read', async () => {
  const w = await moduleRealm(`if (source === 'abandoned') {
    void read('app:/data/abandoned'); throw new Error('abandoned');
  }
  return read('app:/data/current');`, { placement: 'main' });
  try {
    expect((await w.ask('abandoned')).tag).toBe(2);
    expect(w.background().head).toBe(true);
    const current = w.ask('current');
    const drained = await w.drain();
    expect(drained.writes).toBe(0);
    expect(drained.failed).toBe(0);
    expect(w.background().head).toBe(false);
    expect((await current).value).toBe('app:/data/current');
  } finally { w.done(); }
}, 20000);

// The sequence above, on the worker. An answer waiting there held the
// worker's turn on an operation only the other answer's turn could deliver,
// and both waited forever; and an operation issued in another answer's turn
// was charged to that answer.
test('a worker-placed answer queued behind another answer\'s storage parks and answers', async () => {
  const w = await moduleRealm(`if (source === 'feed') { const file = read('app:/data/feed'); return fetch('https://api.castle.test/feed').then(r => r.text()).then(async text => text + ' ' + await file); }
  return read('app:/data/' + source);`);
  try {
    expect((await w.ask('feed')).tag).toBe(1);
    const device = w.ask('device');
    await Bun.sleep(50);
    expect((await race(w.resume('feed'))).value).toBe('fresh app:/data/feed');
    expect((await race(device)).value).toBe('app:/data/device');
  } finally { w.done(); }
}, 20000);

// The runner lets go of a parked answer: its turn is refused, and storage
// goes on for the rest; and a let-go chain a parked answer's delivery made
// the head is finished, so the answers after it are not stuck behind it.
test('a worker-placed parked answer the runner lets go is dropped, and a let-go chain finishes', async () => {
  const w = await moduleRealm(`if (source === 'feed') { const file = read('app:/data/feed').then(() => read('app:/data/feed2')); return fetch('https://api.castle.test/feed').then(r => r.text()).then(async text => text + ' ' + await file); }
  return read('app:/data/' + source);`);
  try {
    // A parked answer let go: refused; the feed resumed answers, and one after.
    expect((await w.ask('feed')).tag).toBe(1);
    const device = w.ask('device');
    await Bun.sleep(50);
    w.forget(['feed']);
    expect(String(await race(device))).toContain('rejected');
    expect((await race(w.resume('feed'))).value).toBe('fresh app:/data/feed2');
    expect((await race(w.ask('after'))).value).toBe('app:/data/after');
    // The feed let go between its reads, with an answer parked behind them:
    // its chain is finished, and the parked answer answers.
    expect((await w.ask('feed')).tag).toBe(1);
    const parked = w.ask('parked');
    await Bun.sleep(50);
    w.forget(['parked']);
    expect((await race(parked)).value).toBe('app:/data/parked');
    expect((await race(w.ask('last'))).value).toBe('app:/data/last');
  } finally { w.done(); }
}, 20000);

const setupDatabase = `if (source === 'setup') return storage.sqlite.open('app:/data/test.db').then(async db => {
  globalThis.db = db; globalThis.statement = await db.prepare('SELECT ?'); return 'ready';
});`;

for (const placement of ['main', 'worker']) {
  for (const [name, operation, key] of [
    ['file clone', "storage.fs.writeFile('app:/data/device', () => {})", 'k'],
    ['database clone', "db.query('SELECT ?', [() => {}])", 'k'],
    ['statement clone', 'statement.query([() => {}])', 'k'],
    ['missing store', "storage.fs.writeFile('app:/data/device', 'bytes')", null],
  ]) test(`${placement}: ${name} failure recovers in the answer that reserved it`, async () => {
    const realm = await moduleRealm(`${setupDatabase}
  if (source === 'barrier') return 'ready';
  if (source === 'feed') {
    const file = storage.fs.readdir('app:/data');
    return fetch('https://api.castle.test/feed').then(r => r.text()).then(async text => text + JSON.stringify(await file));
  }
  return ${operation}.then(() => 'unexpected success', error => {
    const owner = store.get('recovery-owner');
    return storage.fs.readdir('app:/data').then(files => ({ owner, files, refused: !!error.code }));
  });`, { placement, key });
    try {
      if (name.includes('database') || name.includes('statement')) expect((await race(realm.ask('setup'))).value).toBe('ready');
      expect((await race(realm.ask('feed'))).tag).toBe(1);
      const device = race(realm.ask('device'));
      // This turn follows device's parked turn, so the failed call is queued
      // before feed resumes, without a timer deciding when it was accepted.
      expect((await race(realm.ask('barrier'))).value).toBe('ready');
      const feed = await race(realm.resume('feed'));
      expect(feed.value).toBe('fresh[]');
      // The store read observes the same host context.owner that reserves
      // the recovery's storage cell. It must belong to device, never feed.
      expect(feed.reads).toEqual([]);
      const recovered = await device;
      expect(recovered.reads).toEqual(['recovery-owner']);
      expect(recovered.value).toEqual({ owner: 'device', files: [], refused: true });
    } finally { realm.done(); }
  });

  test(`${placement}: file and SQLite arguments are snapshotted when issued`, async () => {
    const realm = await moduleRealm(`${setupDatabase}
  const bytes = new Uint8Array([1, 2]);
  if (source === 'file') {
    const written = storage.fs.writeFile('app:/data/snapshot', bytes);
    bytes[0] = 9;
    return written.then(() => storage.fs.readFile('app:/data/snapshot')).then(value => Array.from(value));
  }
  const pending = source === 'database' ? db.query('SELECT ?', [bytes]) : statement.query([bytes]);
  bytes[0] = 9;
  return pending.then(value => Array.from(value[0]));`, { placement });
    try {
      expect((await race(realm.ask('setup'))).value).toBe('ready');
      for (const source of ['file', 'database', 'statement']) expect((await race(realm.ask(source))).value).toEqual([1, 2]);
    } finally { realm.done(); }
  });
}

// `scheduler.yield()` (LLP 1141.001 D10): the realm resolves an answer's
// yields in a task of their own, and the answer goes on to its value.
for (const placement of ['main', 'worker']) test(`${placement}: an answer that yields goes on to its value`, async () => {
  const realm = await moduleRealm(`return (async () => { let n = 0; for (let i = 0; i < 5; i++) { await scheduler.yield(); n++; } return 'yielded ' + n; })();`, { placement });
  try {
    expect((await race(realm.ask('walk'))).value).toBe('yielded 5');
  } finally { realm.done(); }
});

// The prelude asks at load whether the host has a local listener (LLP 1133.005), outside any answer: the page's
// realm answers that ask, no (''), the one call newly let through; what it refused outside an answer it still
// refuses (a journal line, 13, and the clock, 21, were already answered there).
test('outside an answer the realm newly answers only the load-time listen ask', async () => {
  const w = await moduleRealm(`return 'loaded';`, { placement: 'main' });
  try {
    expect((await race(w.ask('any'))).value).toBe('loaded');
    const door = w.door();
    expect(door(12, 'listen', '')).toBe('');
    expect(() => door(12, 'callback', '')).toThrow('host call outside an answer');
    expect(() => door(2, 'name', '')).toThrow('host call outside an answer');
    expect(() => door(1, '1', '{}')).toThrow('host call outside an answer');
    // An abort made outside a turn (a background round, a mapper) waits in the realm until the runner asks,
    // and wakes the host to ask, since no turn's batch follows it.
    let wakes = 0;
    globalThis.exact.abortWake = () => wakes++;
    expect(door(23, '9', '')).toBeUndefined();
    expect([wakes, w.aborted(), w.aborted()]).toEqual([1, [9], []]);
  } finally { w.done(); }
}, 20000);

test('main: an auth window is prepared at the press before auth glue loads', async () => {
  const priorWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const priorNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const opened = [];
  Object.defineProperties(globalThis, {
    window: { configurable: true, value: { open: url => {
      const popup = { closed: false, url, close() { this.closed = true; } };
      opened.push(popup);
      return popup;
    } } },
    navigator: { configurable: true, value: { userActivation: { isActive: true } } },
  });
  let w;
  try {
    w = await moduleRealm(`return prepareAuthSession();`, { placement: 'main' });
    const prepared = (await race(w.ask('prepare'))).value;
    expect(typeof prepared).toBe('string');
    expect(opened.map(popup => popup.url)).toEqual(['about:blank']);
  } finally {
    w?.done();
    if (priorWindow) Object.defineProperty(globalThis, 'window', priorWindow); else delete globalThis.window;
    if (priorNavigator) Object.defineProperty(globalThis, 'navigator', priorNavigator); else delete globalThis.navigator;
  }
}, 20000);

// A fetch the app aborts (the abort runtime gap): another answer's turn aborts an awaited POST, and that
// turn's result names its ticket (door 23) for the runner to answer aborted and the page to cancel; the
// awaiting answer, resumed, has the rejection. A fetch aborted before it is awaited is withdrawn (16).
for (const placement of ['main', 'worker']) {
  test(`an aborted fetch is reported with the turn that aborted it and withdrawn when unsent (${placement})`, async () => {
    const w = await moduleRealm(`if (source === 'held') { globalThis.held = new AbortController();
    return fetch('https://api.castle.test/held', { method: 'POST', body: 'x', signal: globalThis.held.signal }).then(() => 'fetched', e => 'aborted: ' + e.message); }
  if (source === 'halt') { globalThis.held.abort(new Error('by the app')); return 'halted'; }
  if (source === 'pair') { const c = new AbortController(), first = fetch('https://api.castle.test/first'), second = fetch('https://api.castle.test/second', { signal: c.signal });
    c.abort(); return second.then(() => 'sent', e => e.name).then(async why => (await (await first).text()) + ' ' + why); }
  return read('app:/data/' + source);`, { placement });
    try {
      const held = await race(w.ask('held'));
      expect([held.tag, held.request?.method]).toEqual([1, 'POST']);
      let wakes = 0;
      globalThis.exact.abortWake = () => wakes++;
      const halt = await race(w.ask('halt'));
      // The realm keeps the abort until the runner asks, the worker's posted to the page as it was made. Each
      // wakes the host, since a let-go's turn (its storage delivery included) has no batch to follow it.
      expect([halt.value, halt.aborted, w.aborted(), w.aborted()]).toEqual(['halted', undefined, [held.ticket], []]);
      expect(wakes).toBe(1);
      expect((await race(w.resume('held'))).value).toBe('aborted: by the app');
      const pair = await race(w.ask('pair'));
      expect([pair.tag, pair.request?.url, w.aborted()]).toEqual([1, 'https://api.castle.test/first', []]);
      expect((await race(w.resume('pair'))).value).toBe('fresh AbortError');
    } finally { w.done(); }
  }, 20000);
}

// A getter on `exact.overlay` runs inside the overlay's guard (prelude.js `__exact_overlay`, LLP 1054): on the
// page realm an answer's call stays current between its tasks, so before the first read moved under the guard a
// getter could `store.set` through a store it kept into that outstanding answer. Both reads refuse it now.
test('a writing getter on exact.overlay is refused on both of the overlay\'s reads (main)', async () => {
  const w = await moduleRealm(`if (source === 'arm') { globalThis.kept = store; return fetch('https://api.castle.test/feed').then(r => r.text()); }
  return globalThis.tries.join('\\n');`, { placement: 'main', top: `globalThis.tries = [];
Object.defineProperty(exact, 'overlay', { get() {
  if (globalThis.kept) { try { globalThis.kept.set('x', 'y'); globalThis.tries.push('ran'); } catch (e) { globalThis.tries.push('throws ' + e.message); } }
  return (source, args, answer) => answer.concat(['overlaid']);
} });` });
  try {
    expect((await w.ask('arm')).tag).toBe(1); // its fetch out: the answer is outstanding, its store kept
    expect(w.overlay('items')).toMatchObject({ tag: 0, value: ['overlaid'] });
    const tries = (await race(w.ask('tries'))).value.split('\n');
    expect(tries).toHaveLength(2);
    for (const line of tries) expect(line).toStartWith('throws store.set() is unavailable in an overlay');
  } finally { w.done(); }
}, 20000);


for (const placement of ['main', 'worker']) test(`${placement}: module read flags remain boolean across secret waits`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'plain') return source;
    if (source === 'entropy') crypto.randomUUID();
    if (source === 'external') store.has('token');
    return store.exclusive('token', () => {
      if (source === 'after') { crypto.randomUUID(); store.has('token'); }
      return source;
    });`, { placement, secrets: true });
  try {
    for (const [source, entropy, externalRead] of [
      ['plain', false, false], ['locked', false, false], ['entropy', true, false],
      ['external', false, true], ['after', true, true], ['plain', false, false],
    ]) {
      const result = await w.ask(source);
      expect(result.value).toBe(source);
      expect(result.entropy).toBe(entropy);
      expect(result.externalRead).toBe(externalRead);
    }
  } finally { w.done(); environment.restore(); }
}, 20000);

// These are the real OAuth package, browser prelude, module turns and worker protocol. Only the
// provider's HTTP replies and the browser's lock/storage backends are supplied by the test.
for (const pair of [['main', 'main'], ['main', 'worker'], ['worker', 'worker'], ['main'], ['worker']]) {
  test(`rotating OAuth session is spent once across ${pair.join('/')} realms`, async () => {
    const kept = JSON.stringify({ accessToken: 'old', refreshToken: 'rotate-0', tokenEndpoint: 'https://api.castle.test/token', scopes: [] });
    const environment = secretEnvironment(kept), { disk } = environment;
    const code = new Bun.Transpiler({ loader: 'ts' }).transformSync(readFileSync(resolve(ROOT, 'packages/oauth/index.ts'), 'utf8')).replace(/^export /gm, '');
    const top = `${code}
const provider = oauth({clientId:'fixture',scopes:[],key:'token',endpoints:{authorization:'https://api.castle.test/authorize',token:'https://api.castle.test/token'},apis:['https://api.castle.test']});`;
    const modules = [];
    try {
      for (const placement of pair) modules.push(await moduleRealm(`return provider.fetch(store, 'https://api.castle.test/me').then(r => r.status);`, { placement, secrets: true, stored: [['token', kept]], top }));
      const first = modules[0], second = modules[1] ?? first;
      expect((await first.ask('a')).request.method).toBe('GET');
      expect((await second.ask('b')).request.method).toBe('GET');
      const refreshing = await first.resume('a', { status: 401, headers: [], body: '' });
      expect(refreshing.request.method).toBe('POST');
      expect(refreshing.request.body).toContain('refresh_token=rotate-0');
      const waiting = second.resume('b', { status: 401, headers: [], body: '' });
      const response = { status: 200, headers: [], body: JSON.stringify({ access_token: 'fresh', refresh_token: 'rotate-1', token_type: 'Bearer', expires_in: 3600 }) };
      const refreshed = await first.resume('a', response);
      expect(refreshed.request.method).toBe('GET');
      expect(refreshed.request.headers).toContainEqual(['authorization', 'Bearer fresh']);
      expect((await first.resume('a')).value).toBe(200);
      const reused = await race(waiting);
      expect(reused.request.method).toBe('GET');
      expect(reused.request.headers).toContainEqual(['authorization', 'Bearer fresh']);
      expect((await second.resume('b')).value).toBe(200);
      expect(JSON.parse(disk.get('exact.secret.token')).refreshToken).toBe('rotate-1');
    } finally {
      for (const module of modules.reverse()) module.done();
      environment.restore();
    }
  }, 20000);
}

for (const placement of ['main', 'worker']) test(`${placement}: OAuth sign-out tolerates task-delayed Web Locks`, async () => {
  const kept = JSON.stringify({ accessToken: 'access-alice', refreshToken: 'refresh-alice', login: 'alice', revocationEndpoint: 'https://api.castle.test/revoke' });
  const environment = secretEnvironment(kept);
  const code = new Bun.Transpiler({ loader: 'ts' }).transformSync(readFileSync(resolve(ROOT, 'packages/oauth/index.ts'), 'utf8')).replace(/^export /gm, '');
  const top = `${TASK_DELAYED_LOCKS}
${code}
const provider = oauth({clientId:'fixture',scopes:[],key:'token',endpoints:{authorization:'https://api.castle.test/authorize',token:'https://api.castle.test/token',revocation:'https://api.castle.test/revoke'},apis:[]});`;
  const w = await moduleRealm(`if (source === 'probe') return store.exclusive('token', async () => store.get('token'));
return provider.signOut(store);`, { placement, secrets: true, stored: [['token', kept]], top });
  try {
    const first = await race(w.ask('logout'));
    expect(first.request.url).toBe('https://api.castle.test/revoke');
    expect(first.request.method).toBe('POST');
    expect(first.request.body).toContain('refresh-alice');
    expect(environment.disk.get('exact.secret.token') ?? '').not.toContain('access-alice');
    const competing = w.ask('probe');
    await environment.requested(2);
    expect(environment.grantCount()).toBe(1);
    expect((await w.resume('logout')).value).toMatchObject({ ok: true });
    await environment.granted(2);
    expect((await competing).value).toBeNull();
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: OAuth revocation retries only in the same runtime`, async () => {
  const kept = JSON.stringify({ accessToken: 'access-alice', refreshToken: 'refresh-alice', login: 'alice', revocationEndpoint: 'https://api.castle.test/revoke' });
  const environment = secretEnvironment(kept);
  const code = new Bun.Transpiler({ loader: 'ts' }).transformSync(readFileSync(resolve(ROOT, 'packages/oauth/index.ts'), 'utf8')).replace(/^export /gm, '');
  const top = `${code}
const provider = oauth({clientId:'fixture',scopes:[],key:'token',endpoints:{authorization:'https://api.castle.test/authorize',token:'https://api.castle.test/token',revocation:'https://api.castle.test/revoke'},apis:[]});`;
  let w = await moduleRealm(`return provider.signOut(store);`, { placement, secrets: true, stored: [['token', kept]], top });
  try {
    const first = await w.ask('first');
    expect(first.request.body).toContain('refresh-alice');
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    expect((await w.resume('first', { status: 503, headers: [], body: '' })).value).toMatchObject({ ok: true });
    const retry = await w.ask('retry');
    expect(retry.request.body).toContain('refresh-alice');
    expect((await w.resume('retry')).value).toMatchObject({ ok: true });

    environment.disk.set('exact.secret.token', JSON.stringify({ accessToken: 'access-bob', refreshToken: 'refresh-bob', login: 'bob', revocationEndpoint: 'https://api.castle.test/revoke' }));
    const departing = await w.ask('departing');
    expect(departing.request.body).toContain('refresh-bob');
    expect((await w.resume('departing', { status: 503, headers: [], body: '' })).value).toMatchObject({ ok: true });
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    w.done();
    w = await moduleRealm(`return provider.signOut(store);`, { placement, secrets: true, stored: [['token', kept]], top });
    const afterCrash = await w.ask('after-crash');
    expect(afterCrash.request).toBeUndefined();
    expect(afterCrash.value).toMatchObject({ ok: true });
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: an answer cannot wait on another answer's fetch`, async () => {
  const w = await moduleRealm(`if (source === 'owner') {
    globalThis.shared = fetch('https://api.castle.test/first').then(response => response.text());
    return globalThis.shared;
  }
  return globalThis.shared.then(() => fetch('https://api.castle.test/second')).then(response => response.text());`, { placement });
  try {
    expect((await w.ask('owner')).request.url).toBe('https://api.castle.test/first');
    const refused = await w.ask('waiter');
    expect(refused.tag).toBe(2);
    expect(refused.message).toContain('awaits a promise another answer started');
  } finally { w.done(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: secret locks retire and failed writes cannot retain a spent token`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`return store.exclusive(source === 'denied' ? 'other' : 'token', async () => {
    if (source === 'hold') await fetch('https://api.castle.test/wait');
    if (source === 'write') store.set('token', 'rotated');
    return store.get('token');
  });`, { placement, secrets: true, stored: [['token', 'stale']] });
  try {
    expect((await w.ask('hold')).tag).toBe(1);
    const waiter = w.ask('waiter');
    const cancelled = race(waiter);
    await environment.requested(2);
    w.forget(['hold']);
    expect(await cancelled).toContain('let go');
    // The cancelled waiter did not end the holder; its continuation still owns the key.
    expect((await w.resume('hold')).value).toBe('old');
    expect((await w.ask('read')).value).toBe('old');
    expect((await w.ask('denied')).message).toContain('not granted');
    environment.failWrites();
    const failed = await w.ask('write').catch(error => error);
    expect(failed.message).toContain('quota');
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    expect((await w.ask('read')).value).toBe(null);
    expect((await w.ask('hold')).tag).toBe(1);
    w.forget([]);
    expect((await race(w.ask('read'))).value).toBe(null);
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a second lease stays held through its request and writes`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'chain') return store.exclusive('token', async () => {
    store.set('token', 'first');
  }).then(() => store.exclusive('token', async () => {
    const response = await fetch('https://api.castle.test/token', {method:'POST', body:'first'});
    store.set('token', await response.text());
    return store.get('token');
  }));
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'stale']] });
  try {
    const second = await w.ask('chain');
    expect(second.request.method).toBe('POST');
    const competing = w.ask('competing');
    await environment.requested(2);
    const grantsWhilePending = environment.grantCount();
    expect((await w.resume('chain', { status: 200, headers: [], body: 'second' })).value).toBe('second');
    await environment.granted(2);
    const observed = await competing;
    expect(grantsWhilePending).toBe(1);
    expect(observed.value).toBe('second');
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a completed lease survives yield and ends before later browser work`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'logout') return store.exclusive('token', async () => {
    store.forget('token');
    await scheduler.yield();
  }).then(async () => {
    await scheduler.yield();
    return fetch('https://api.castle.test/logout');
  });
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'stale']] });
  try {
    expect((await w.ask('logout')).request.url).toBe('https://api.castle.test/logout');
    const competing = w.ask('competing');
    await environment.requested(2);
    await environment.granted(2);
    const grantsWhilePending = environment.grantCount();
    await w.resume('logout');
    const observed = await competing;
    expect(grantsWhilePending).toBe(2);
    expect(observed.value).toBeNull();
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a held lease parked behind another answer's storage does not deadlock`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'lease-holder') return store.exclusive('token', async () => {
    store.set('token', 'half-written');
    await fetch('https://api.castle.test/lease', { method: 'POST', body: 'login' });
    await read('app:/data/queued-behind-owner');
    store.set('token', 'complete');
    return 'lease-finished';
  });
  if (source === 'storage-owner') {
    const ownStorage = read('app:/data/owner');
    return store.exclusive('token', async () => {
      await ownStorage;
      return store.get('token');
    });
  }
  return 'ready';`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('lease-holder')).request.url).toBe('https://api.castle.test/lease');
    const storageOwner = w.ask('storage-owner');
    await w.storageEvent('finish', 'app:/data/owner');
    await environment.requested(2);
    const grantsBeforeHolderFinishes = environment.grantCount();
    const holder = w.resume('lease-holder');
    const settled = await race(Promise.allSettled([holder, storageOwner]));
    expect(settled).not.toBe('STILL WAITING');
    expect(settled).toMatchObject([{ status: 'fulfilled', value: { value: 'lease-finished' } }, { status: 'fulfilled', value: { value: 'complete' } }]);
    expect(grantsBeforeHolderFinishes).toBe(1);
    expect(environment.disk.get('exact.secret.token')).toBe('complete');
    expect(environment.disk.has('exact.exclusive.exact.secret.token')).toBe(false);
  } finally { w.done(); environment.restore(); }
}, 20000);
for (const placement of ['main', 'worker']) test(`${placement}: an answer drains storage before waiting for a secret`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`globalThis.leaseBodies ??= 0;
  if (source === 'holder') return store.exclusive('token', async () => {
    await fetch('https://api.castle.test/holder', { method: 'POST', body: 'login' });
    await read('app:/data/holder-storage');
    store.set('token', 'complete');
    return 'holder-finished';
  });
  if (source === 'head') {
    const file = read('app:/data/head');
    return fetch('https://api.castle.test/head').then(async () => (await file, 'head-finished'));
  }
  if (source === 'waiter') {
    const seen = [];
    const own = label => value => { seen.push(label + ':' + store.get('recovery-owner')); return value; };
    const one = read('app:/data/waiter-1').then(own('one'));
    const rejected = read('app:/data/reject').catch(() => {
      seen.push('reject:' + store.get('recovery-owner'));
      return read('app:/data/waiter-recovery').then(own('recovery'));
    });
    const two = read('app:/data/waiter-2').then(own('two'));
    const storage = Promise.all([one, rejected, two]);
    return store.exclusive('token', async () => {
      globalThis.leaseBodies++;
      await storage;
      return { seen, token: store.get('token') };
    });
  }
  return globalThis.leaseBodies;`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('holder')).request.url).toBe('https://api.castle.test/holder');
    expect((await w.ask('head')).request.url).toBe('https://api.castle.test/head');
    const waiter = w.ask('waiter');
    expect((await w.ask('barrier')).value).toBe(0);
    const lockRequestsBeforeDrain = environment.requestCount();
    const holder = w.resume('holder');
    const head = w.resume('head');
    const settled = await race(Promise.allSettled([holder, head, waiter]));
    expect(settled).not.toBe('STILL WAITING');
    expect(lockRequestsBeforeDrain).toBe(1);
    expect(settled).toMatchObject([{ status: 'fulfilled', value: { value: 'holder-finished' } },
      { status: 'fulfilled', value: { value: 'head-finished' } }, { status: 'fulfilled', value: { value: { token: 'complete' } } }]);
    expect(settled[2].value.value.seen).toEqual(['one:waiter', 'reject:waiter', 'two:waiter', 'recovery:waiter']);
    expect(w.storageEvents().filter(event => event[0] === 'start').map(event => event[1])).toEqual([
      'app:/data/head', 'app:/data/waiter-1', 'app:/data/reject', 'app:/data/waiter-2',
      'app:/data/holder-storage', 'app:/data/waiter-recovery',
    ]);
    expect(environment.requestCount()).toBe(2);
  } finally { w.done(); environment.restore(); }
}, 30000);
for (const placement of ['main', 'worker']) test(`${placement}: an answer forgotten while draining storage never starts its lease body`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`globalThis.leaseBodies ??= 0;
  if (source === 'head') {
    const file = read('app:/data/head');
    return fetch('https://api.castle.test/head').then(async () => (await file, 'head-finished')); }
  if (source === 'forgotten') {
    const file = read('app:/data/forgotten');
    return store.exclusive('token', async () => {
      globalThis.leaseBodies++;
      await file;
      return 'lease-ran';
    });
  }
  return globalThis.leaseBodies;`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('head')).request.url).toBe('https://api.castle.test/head');
    await w.storageEvent('start', 'app:/data/head');
    const forgotten = race(w.ask('forgotten'));
    expect((await w.ask('barrier')).value).toBe(0);
    w.forget(['head']);
    const head = w.resume('head');
    expect((await race(head)).value).toBe('head-finished');
    expect(String(await forgotten)).toContain('rejected');
    expect((await w.ask('inspect')).value).toBe(0);
    expect(environment.requestCount()).toBe(0);
  } finally { w.done(); environment.restore(); }
}, 20000);
for (const placement of ['main', 'worker']) test(`${placement}: storage issued for an answer that waits for a lease is refused by name`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'holder') return store.exclusive('token', async () => {
    await fetch('https://api.castle.test/holder', { method: 'POST', body: 'login' });
    return 'holder-finished';
  });
  if (source === 'waiter') {
    globalThis.waitingCall = __exact_async_context.current();
    return store.exclusive('token', async () => 'waiter-finished');
  }
  if (source === 'inject') {
    const before = __exact_async_context.swap(globalThis.waitingCall);
    const refused = read('app:/data/too-late').then(() => 'ran', error => error.message);
    __exact_async_context.swap(before);
    return refused;
  }
  return 'ready';`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('holder')).request.url).toBe('https://api.castle.test/holder');
    const waiter = w.ask('waiter');
    await environment.requested(2);
    const refused = await w.ask('inject');
    expect(refused.value).toBe('storage.readFile() called while its answer waits for store.exclusive(): it was never run');
    expect(w.storageEvents().some(event => event[1] === 'app:/data/too-late')).toBe(false);
    expect((await w.resume('holder')).value).toBe('holder-finished');
    expect((await waiter).value).toBe('waiter-finished');
  } finally { w.done(); environment.restore(); }
}, 20000);
for (const placement of ['main', 'worker']) test(`${placement}: a completed lease whose answer then waits on storage does not deadlock a lease waiter with storage in flight`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'holder') return store.exclusive('token', async () => {
    await fetch('https://api.castle.test/holder', { method: 'POST', body: 'login' });
    store.set('token', 'completed');
  }).then(() => read('app:/data/after-complete')).then(() => 'holder-finished');
  if (source === 'waiter') {
    const ownStorage = read('app:/data/waiter');
    return store.exclusive('token', async () => {
      await ownStorage;
      return store.get('token');
    });
  }
  return 'ready';`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('holder')).request.url).toBe('https://api.castle.test/holder');
    const waiter = w.ask('waiter');
    await w.storageEvent('start', 'app:/data/waiter');
    await environment.checkpoint();
    const holder = w.resume('holder');
    const settled = await race(Promise.allSettled([holder, waiter]));
    expect(settled).not.toBe('STILL WAITING');
    expect(settled).toMatchObject([{ status: 'fulfilled', value: { value: 'holder-finished' } },
      { status: 'fulfilled', value: { value: 'completed' } }]);
    expect(environment.disk.get('exact.secret.token')).toBe('completed');
  } finally { w.done(); environment.restore(); }
}, 20000);
for (const placement of ['main', 'worker']) test(`${placement}: a lease's storage catch and finally remain in its own turn`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'holder') return store.exclusive('token', async () => {
    await fetch('https://api.castle.test/holder', { method: 'POST', body: 'login' });
    try {
      await read('app:/data/reject');
    } catch (_) {
      store.set('token', 'caught');
      await Promise.resolve();
      await fetch('https://api.castle.test/cleanup', { method: 'POST', body: store.get('recovery-owner') });
    } finally {
      store.set('token', 'finally');
      await new Promise(resolve => setTimeout(resolve, 0));
    }
    return 'holder-finished';
  });
  if (source === 'head') {
    const file = read('app:/data/head');
    return fetch('https://api.castle.test/head').then(async () => (await file, 'head-finished'));
  }
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('holder')).request.url).toBe('https://api.castle.test/holder');
    expect((await w.ask('head')).request.url).toBe('https://api.castle.test/head');
    const holder = w.resume('holder');
    const head = await w.resume('head');
    const cleanup = await holder;
    expect(head).toMatchObject({ value: 'head-finished', writes: [] });
    expect(cleanup.request).toMatchObject({ url: 'https://api.castle.test/cleanup', body: 'holder' });
    expect(cleanup.writes).toContainEqual(['token', 'caught', true]);
    const following = w.ask('following');
    await environment.requested(2);
    expect(environment.grantCount()).toBe(1);
    const timer = await w.resume('holder');
    expect(timer.timer).toBeDefined();
    expect(timer.writes.at(-1)).toEqual(['token', 'finally', true]);
    expect(environment.grantCount()).toBe(1);
    expect((await w.resume('holder')).value).toBe('holder-finished');
    expect((await following).value).toBe('finally');
    expect(environment.disk.get('exact.secret.token')).toBe('finally');
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a completed lease cannot resurrect a later sign-out`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'holder') return store.exclusive('token', async () => {
    store.set('token', 'completed-login');
  }).then(() => fetch('https://api.castle.test/later')).then(() => store.exclusive('token', async () => store.get('token')));
  if (source === 'sign-out') return store.exclusive('token', async () => store.forget('token'));
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'login']], taskDelayedStorage: true });
  try {
    const later = await w.askWithDeliver('holder', w.commit);
    expect(later.request.url).toBe('https://api.castle.test/later');
    expect(environment.disk.get('exact.secret.token')).toBe('completed-login');
    expect(w.runnerValue('token')).toBe('completed-login');
    const signedOut = await w.askWithDeliver('sign-out', w.commit);
    expect(signedOut.value).toBeNull();
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    expect(w.runnerValue('token')).toBeNull();
    const resumed = await w.resumeWithDeliver('holder', undefined, w.commit);
    expect(resumed.value).toBeNull();
    expect(resumed.writes).toEqual([]);
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    expect(w.runnerValue('token')).toBeNull();
    expect((await w.ask('following')).value).toBeNull();
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) for (const caught of [false, true]) test(`${placement}: today's rejected lease with queued storage ${caught ? 'publishes its partial credential after an outer catch' : 'recovers by deleting the credential'}`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'head') {
    const file = read('app:/data/head');
    return fetch('https://api.castle.test/head').then(async () => (await file, 'head-finished')); }
  if (source === 'failed') {
    const failed = store.exclusive('token', async () => {
      await fetch('https://api.castle.test/token', { method: 'POST', body: 'old' });
      store.set('token', 'partial');
      void read('app:/data/queued').catch(() => {});
      throw new Error('lease body failed');
    });
    return ${caught ? "failed.catch(error => 'caught: ' + error.message)" : 'failed'};
  }
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    expect((await w.ask('head')).request.url).toBe('https://api.castle.test/head');
    expect((await w.ask('failed')).request.url).toBe('https://api.castle.test/token');
    expect(environment.disk.get('exact.exclusive.exact.secret.token')).toBe('1');
    const failed = w.resumeWithDeliver('failed', undefined, result => { if (result.tag === 0) w.commit(result); });
    const head = w.resume('head');
    expect((await race(head)).value).toBe('head-finished');
    if (placement === 'main' && w.background().head) await w.drain();
    const failedResult = await race(failed);
    if (caught) expect(failedResult.value).toBe('caught: lease body failed');
    else expect(failedResult.tag).toBe(2);
    expect(failedResult.writes.at(-1)).toEqual(['token', 'partial', true]);
    const following = await w.askWithDeliver('following', w.commit);
    expect(following.value).toBe(caught ? 'partial' : null);
    expect(environment.disk.get('exact.secret.token') ?? null).toBe(caught ? 'partial' : null);
    expect(environment.disk.has('exact.exclusive.exact.secret.token')).toBe(false);
    expect(w.runnerValue('token')).toBe(caught ? 'partial' : null);
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a timer resumed during lease publication starts outside the completed turn`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'timer') return (async () => {
    await store.exclusive('token', async () => store.set('token', 'published'));
    await new Promise(resolve => setTimeout(resolve, 0));
    return 'timer-finished';
  })();
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'stale']] });
  try {
    let resumed;
    const timer = await w.askWithDeliver('timer', result => {
      resumed = Promise.resolve().then(() => w.resume('timer'));
    });
    expect(timer.secretComplete).toBe(true);
    expect(timer.timer).toBeDefined();
    expect(await race(resumed)).toEqual({ tag: 0, value: 'timer-finished', reads: [], writes: [], externalRead: false, entropy: false, topics: [] });
    const following = w.ask('following');
    await environment.requested(2);
    await environment.granted(2);
    expect((await following).value).toBe('published');
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a lease completed before a wait behind another answer's storage ends before later browser work`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'holder') {
    const file = read('app:/data/holder');
    return fetch('https://api.castle.test/holder').then(async response => (await file, response.text()));
  }
  if (source === 'parked') return store.exclusive('token', async () => {
    store.set('token', 'parked-complete');
  }).then(() => read('app:/data/parked')).then(() => fetch('https://api.castle.test/later')).then(response => response.text());
  if (source === 'following') return store.exclusive('token', async () => store.get('token'));
  return 'ready';`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('holder')).request.url).toBe('https://api.castle.test/holder');
    const parked = w.ask('parked');
    await environment.granted(1);
    expect((await w.ask('barrier')).value).toBe('ready');
    expect(environment.disk.get('exact.secret.token')).toBe('login');
    const following = w.ask('following');
    await environment.requested(2);
    await environment.checkpoint();
    await environment.checkpoint();
    expect(environment.grantCount()).toBe(1);
    const holder = w.resume('holder');
    const later = await parked;
    expect(later.request.url).toBe('https://api.castle.test/later');
    expect((await holder).value).toBe('fresh');
    expect(environment.disk.get('exact.secret.token')).toBe('parked-complete');
    await environment.granted(2);
    const grantsWhileLaterWorkWaits = environment.grantCount();
    expect((await following).value).toBe('parked-complete');
    expect(grantsWhileLaterWorkWaits).toBe(2);
    expect((await w.resume('parked')).value).toBe('fresh');
  } finally { w.done(); environment.restore(); }
}, 20000);

test(`worker: forgetting a parked answer after a completed lease releases the lock and publishes nothing`, async () => {
  const placement = 'worker';
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'holder') {
    const file = read('app:/data/held');
    return fetch('https://api.castle.test/holder').then(async response => (await file, response.text()));
  }
  if (source === 'forgotten') return store.exclusive('token', async () => {
    store.set('token', 'forgotten-complete');
  }).then(() => read('app:/data/forgotten'));
  if (source === 'following') return store.exclusive('token', async () => store.get('token'));
  if (source === 'release') { globalThis.__exact_test_release_held_storage(); return 'released'; }
  return source === 'barrier' ? 'ready' : read('app:/data/' + source);`, { placement, secrets: true, stored: [['token', 'stale']], taskDelayedStorage: true });
  try {
    expect((await w.ask('holder')).request.url).toBe('https://api.castle.test/holder');
    const forgotten = w.ask('forgotten');
    await environment.entered(1);
    expect((await w.ask('barrier')).value).toBe('ready');
    w.forget(['holder']);
    expect(await race(forgotten)).not.toBe('STILL WAITING');
    expect(environment.disk.get('exact.secret.token')).toBe('login');
    const following = w.ask('following');
    await environment.requested(2);
    expect(await race(environment.granted(2))).toBeUndefined();
    expect((await following).value).toBe('login');
    expect((await w.ask('release')).value).toBe('released');
    expect((await w.resume('holder')).value).toBe('fresh');
    expect((await race(w.ask('after'))).value).toBe('app:/data/after');
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a lease after a park is forgotten with its request out`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'holder') {
    const file = read('app:/data/holder');
    return fetch('https://api.castle.test/holder').then(async response => (await file, response.text()));
  }
  if (source === 'chain') return store.exclusive('token', async () => {
    store.set('token', 'first-complete');
  }).then(() => read('app:/data/chain')).then(() => store.exclusive('token', async () => {
    await fetch('https://api.castle.test/token', { method: 'POST', body: 'first-complete' });
    store.set('token', 'second-complete');
  }));
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'stale']] });
  try {
    expect((await w.ask('holder')).request.url).toBe('https://api.castle.test/holder');
    const chain = w.ask('chain');
    await environment.granted(1);
    await environment.checkpoint();
    await environment.checkpoint();
    const holder = w.resume('holder');
    expect((await chain).request.method).toBe('POST');
    expect((await holder).value).toBe('fresh');
    expect(environment.disk.get('exact.exclusive.exact.secret.token')).toBe('1');
    w.forget([]);
    const competing = w.ask('competing');
    await environment.requested(2);
    expect(await race(environment.granted(2))).toBeUndefined();
    expect((await competing).value).toBeNull();
    expect(environment.disk.has('exact.secret.token')).toBe(false);
  } finally { w.done(); environment.restore(); }
}, 20000);

test(`worker: a successful result forgotten before delivery cannot write after its lock is released`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`return store.exclusive('token', async () => {
    store.set('token', 'forgotten-complete');
    return 'worker-complete';
  });`, { placement: 'worker', secrets: true, stored: [['token', 'stale']], controlWorkerMessages: true });
  try {
    w.holdWorkerMessagesAfter(1);
    const forgotten = w.askWithDeliver('forgotten', result => {
      for (const [name, value, durable] of result.writes ?? []) if (!durable) {
        if (value == null) localStorage.removeItem(`exact.secret.${name}`);
        else localStorage.setItem(`exact.secret.${name}`, value);
      }
    });
    await environment.granted(1);
    await w.workerMessages(1);
    expect(w.workerMessage(0)).toMatchObject({ result: { tag: 0, value: 'worker-complete', writes: [['token', 'forgotten-complete']] } });
    w.forget([]);
    const replacement = navigator.locks.request('exact.secret.token', () => localStorage.setItem('exact.secret.token', 'replacement'));
    expect(await race(replacement)).toBeUndefined();
    expect(environment.disk.get('exact.secret.token')).toBe('replacement');
    w.releaseWorkerMessages();
    const forgottenResult = await race(forgotten);
    expect(environment.disk.get('exact.secret.token')).toBe('replacement');
    expect(String(forgottenResult)).toContain('rejected');
  } finally { w.done(); environment.restore(); }
}, 20000);

test(`worker: forgetting after committed delivery does not undo the secret publication`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'publish') return store.exclusive('token', async () => {
    store.set('token', 'committed'); return 'published'; });
  return store.exclusive('token', async () => store.get('token'));`, { placement: 'worker', secrets: true, stored: [['token', 'login']] });
  try {
    expect((await w.askWithDeliver('publish', w.commit)).value).toBe('published');
    expect(environment.disk.get('exact.secret.token')).toBe('committed');
    expect(w.runnerValue('token')).toBe('committed');
    w.forget([]);
    expect(environment.disk.get('exact.secret.token')).toBe('committed');
    expect((await w.ask('following')).value).toBe('committed');
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) test(`${placement}: a lease ends before later browser work and a forgotten call stays settled`, async () => {
  const environment = secretEnvironment('login');
  const w = await moduleRealm(`if (source === 'logout') return store.exclusive('token', async () => {
    store.forget('token');
  }).then(() => fetch('https://api.castle.test/logout'));
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'stale']] });
  try {
    expect((await w.ask('logout')).request.url).toBe('https://api.castle.test/logout');
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    const firstRequests = environment.requestCount(), firstGrants = environment.grantCount();
    const read = w.ask('read');
    await environment.requested(firstRequests + 1);
    await environment.granted(firstGrants + 1);
    const grantsWhileBrowserWaits = environment.grantCount();
    await w.resume('logout');
    const readResult = await read;
    expect(grantsWhileBrowserWaits).toBe(firstGrants + 1);
    expect(readResult.value).toBeNull();
    environment.disk.set('exact.secret.token', 'next-login');
    let delivered, release;
    const delivering = new Promise(resolve => { delivered = resolve; });
    const paused = new Promise(resolve => { release = resolve; });
    const forgotten = w.askWithDeliver('logout', async () => { delivered(); await paused; });
    await delivering;
    w.forget([]);
    const requestsBefore = environment.requestCount();
    const grantsBefore = environment.grantCount();
    const after = w.ask('after-forget');
    let settled = false;
    after.finally(() => { settled = true; });
    await environment.requested(requestsBefore + 1);
    expect(environment.grantCount()).toBe(grantsBefore);
    expect(settled).toBe(false);
    release();
    await forgotten;
    await environment.granted(grantsBefore + 1);
    expect((await race(after)).value).toBeNull();
  } finally { w.done(); environment.restore(); }
}, 20000);


for (const placement of ['main', 'worker']) test(`${placement}: an interrupted secret lease publishes no half-written credential and recovers`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`return store.exclusive('token', async () => {
    if (source === 'refresh') {
      store.set('token', 'half-written');
      await fetch('https://api.castle.test/token', {method:'POST', body:'old'});
      store.set('token', 'complete');
    }
    return store.get('token');
  });`, { placement, secrets: true, stored: [['token', 'stale']] });
  try {
    expect((await w.ask('refresh')).request.method).toBe('POST');
    expect(environment.disk.get('exact.secret.token')).toBe('half-written');
    expect(environment.disk.get('exact.exclusive.exact.secret.token')).toBe('1');
    w.forget([]);
    expect((await w.ask('read')).value).toBeNull();
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    // A page that died without cleanup leaves the same marker for its successor.
    environment.disk.set('exact.secret.token', 'spent-before-page-exit');
    environment.disk.set('exact.exclusive.exact.secret.token', '1');
    expect((await w.ask('successor')).value).toBeNull();
    expect(environment.disk.has('exact.exclusive.exact.secret.token')).toBe(false);
  } finally { w.done(); environment.restore(); }
}, 20000);

for (const placement of ['main', 'worker']) for (const wait of ['GET', 'timer']) test(`${placement}: an interrupted secret lease after a ${wait} exposes today's half-written credential`, async () => {
  const environment = secretEnvironment('old');
  const body = wait === 'GET' ? "await fetch('https://api.castle.test/token');" : 'await new Promise(resolve => setTimeout(resolve, 0));';
  const w = await moduleRealm(`if (source === 'interrupted') return store.exclusive('token', async () => {
    store.set('token', 'half-written');
    ${body}
    store.set('token', 'complete');
  });
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'old']] });
  try {
    const interrupted = await w.ask('interrupted');
    if (wait === 'GET') expect(interrupted.request.method).toBe('GET');
    else expect(interrupted.timer).toBeDefined();
    expect(environment.disk.get('exact.secret.token')).toBe('half-written');
    expect(environment.disk.has('exact.exclusive.exact.secret.token')).toBe(false);
    w.forget([]);
    expect((await w.ask('successor')).value).toBe('half-written');
    expect(environment.disk.get('exact.secret.token')).toBe('half-written');
  } finally { w.done(); environment.restore(); }
}, 20000);

test('storage preserves access denial codes and the known-folder absence name', () => {
  const source = readFileSync(resolve(ROOT, 'js/src/storage.js'), 'utf8');
  for (const [windows, errno] of [[false, 13], [true, 5]]) {
    const realm = vm.createContext({ __exact_windows_storage: windows });
    vm.runInContext(source, realm);
    const calls = realm.__exact_storage_calls({});
    expect(calls.storageError(`filesystem: access denied (os error ${errno})`).code).toBe('EACCES');
    const absent = calls.storageError('NotSupportedError: well-known folders are unavailable on this host');
    expect([absent.name, absent.kind, absent.code]).toEqual(['NotSupportedError', 'Unavailable', 'failed']);
  }
});
