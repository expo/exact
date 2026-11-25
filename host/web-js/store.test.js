// The web JS target's store, with its persistence (store.js), as a page with a data module links it. Run alone, `bun test ./host/web-js/store.test.js`: reorder.test.js mocks rt.js for its process.
import { test, expect, mock } from "bun:test";
import { runtimeSource } from '../web/tests/stand-ins.mjs';

// The build copies navigation.js and prompt-glue.js in from host/web and writes names.js.
mock.module("./navigation.js", () => import("../web/navigation.js"));
mock.module("./prompt-glue.js", () => ({}));
mock.module("./alert-message.js", () => ({}));
mock.module("./menu-rows.js", () => ({}));
mock.module("./names.js", () => ({ sourceTypes: {} }));
const { Store } = await import("./rt.js");
await import("./store.js");
const { createSecretFacade } = await import("./admission.js");

test("a render's store refuses writes before changing memory or scheduling readers", () => {
  const saved = ['__exactRender', 'queueMicrotask', 'localStorage'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  const before = Store.save();
  let queued = 0, loaded = 0;
  globalThis.__exactRender = true;
  globalThis.queueMicrotask = () => queued++;
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { get length() { loaded++; return 0; } } });
  Store.map = new Map(); Store.writes = []; Store.dirty = false;
  try {
    for (const value of ['kept', null]) {
      expect(() => Store.set('token', value)).toThrow("a render's store is empty and refuses writes");
      expect(Store.get('token')).toBeUndefined();
      expect([...Store.map]).toEqual([]);
      expect(Store.writes).toEqual([]);
      expect(Store.dirty).toBe(false);
      expect(queued).toBe(0);
    }
    Store.load();
    expect(loaded).toBe(0);
    Store.map.set('token', 'private');
    expect(Store.get('token')).toBeUndefined();
    expect(() => Store.set('token', 'private')).toThrow("a render's store is empty and refuses writes");
    expect([...Store.map]).toEqual([['token', 'private']]);
    expect(queued).toBe(0);
  } finally {
    Store.restore(before);
    for (const [key, descriptor] of saved) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  }
});

// As the runner's `Store::set`.
test("keeping what is already kept is no write", () => {
  Store.map = new Map([["token", "same"]]);
  Store.writes = [];
  Store.dirty = false;
  Store.set("token", "same");
  expect(Store.writes).toEqual([]);
  expect(Store.dirty).toBe(false);
  Store.set("token", "new");
  expect(Store.writes).toEqual([["token", "new"]]);
  Store.set("token", null);
  expect(Store.writes.at(-1)).toEqual(["token", null]);
  expect(Store.get("token")).toBeUndefined();
});

test("a shared-store lock reads another realm's committed value before writing", async () => {
  const { exclusiveSecret } = await import('../web/storage-environment.js');
  const saved = ['navigator', 'localStorage'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  const disk = new Map([['test.token', 'new']]);
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: k => disk.get(k) ?? null, setItem: (k, v) => disk.set(k, v), removeItem: k => disk.delete(k) } });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: { request: async (_name, work) => work() } } });
  const store = { map: new Map([['token', 'stale']]), pending: new Map(), persist() {} };
  try {
    expect(await exclusiveSecret(store, 'test.', 'token', async () => store.map.get('token'))).toBe('new');
    store.pending.set('token', {});
    expect(await exclusiveSecret(store, 'test.', 'token', async () => store.map.get('token'))).toBe('new');
    expect(store.pending.size).toBe(0);
    await expect(exclusiveSecret(store, 'test.', 'token', async () => { store.pending.set('token', {}); })).rejects.toThrow('persisted');
    expect(store.pending.size).toBe(0);
    expect(disk.has('test.token')).toBe(false);
  } finally {
    for (const [key, descriptor] of saved) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  }
});


test("failed secret writes cannot overwrite a later realm's rotation", async () => {
  const { exclusiveSecret } = await import('../web/storage-environment.js');
  const saved = ['navigator', 'localStorage'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  const disk = new Map([['exact.secret.token', 'old']]); let fail = true;
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: k => disk.get(k) ?? null,
    setItem: (k, v) => { if (fail) throw new Error('quota'); disk.set(k, v); }, removeItem: k => disk.delete(k) } });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: { request: async (_name, work) => work() } } });
  Store.map = new Map([['token', 'old']]); Store.writes = []; Store.pending.clear();
  try {
    Store.writes.push(['token', 'ordinary-stale']); Store.persist();
    disk.set('exact.secret.token', 'another-realm'); fail = false; await Store.persist();
    expect(disk.get('exact.secret.token')).toBe('another-realm');
    await expect(exclusiveSecret(Store, 'exact.secret.', 'token', async () => { fail = true; Store.writes.push(['token', 'rotated-A']); })).rejects.toThrow('persisted');
    expect(Store.pending.size).toBe(0);
    disk.set('exact.secret.token', 'rotated-B'); disk.delete('exact.exclusive.exact.secret.token'); fail = false; Store.persist();
    expect(disk.get('exact.secret.token')).toBe('rotated-B');
    Store.writes.push(['token', 'queued-before-lease']);
    expect(await exclusiveSecret(Store, 'exact.secret.', 'token', async () => Store.get('token'))).toBe('rotated-B');
    expect(disk.get('exact.secret.token')).toBe('rotated-B');
  } finally {
    Store.writes = []; Store.pending.clear();
    for (const [key, descriptor] of saved) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  }
});


test("a new secret owner invalidates a credential left by an interrupted update", async () => {
  const { exclusiveSecret } = await import('../web/storage-environment.js');
  const saved = ['navigator', 'localStorage'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  const disk = new Map([['test.token', 'spent'], ['exact.exclusive.test.token', '1']]);
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: k => disk.get(k) ?? null, setItem: (k, v) => disk.set(k, v), removeItem: k => disk.delete(k) } });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: { request: async (_name, work) => work() } } });
  const store = { map: new Map([['token', 'spent']]), pending: new Map(), persist() {} };
  try {
    expect(await exclusiveSecret(store, 'test.', 'token', async () => store.map.get('token'))).toBeUndefined();
    expect(disk.size).toBe(0);
    disk.set('test.token', 'current');
    await expect(exclusiveSecret(store, 'test.', 'token', async arm => { arm(); throw new Error('interrupted'); })).rejects.toThrow('interrupted');
    expect(disk.size).toBe(0);
  } finally {
    for (const [key, descriptor] of saved) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  }
});

// LLP 1146 D5, as on every host: recovery is armed by a lease's first write or request with effects, and only an
// armed lease that fails loses the secret.
test("a secret lease is armed by its first write, and a failed lease that never armed keeps the secret", async () => {
  const { exclusiveSecret } = await import('../web/storage-environment.js');
  const saved = ['navigator', 'localStorage'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  const disk = new Map([['exact.secret.token', 'old']]), marker = 'exact.exclusive.exact.secret.token';
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: k => disk.get(k) ?? null, setItem: (k, v) => disk.set(k, v), removeItem: k => disk.delete(k) } });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: { request: async (_name, work) => work() } } });
  Store.map = new Map(); Store.writes = []; Store.pending.clear();
  try {
    await expect(exclusiveSecret(Store, 'exact.secret.', 'token', async () => {
      Store.set('token', 'old'); // keeping what it holds is no write
      expect(disk.has(marker)).toBe(false);
      throw new Error('before any write');
    })).rejects.toThrow('before any write');
    expect([...disk]).toEqual([['exact.secret.token', 'old']]);
    await expect(exclusiveSecret(Store, 'exact.secret.', 'absent', async () => {
      Store.set('absent', null); // nor is forgetting what it does not hold
      expect(disk.has('exact.exclusive.exact.secret.absent')).toBe(false);
      throw new Error('forgot nothing');
    })).rejects.toThrow('forgot nothing');
    await expect(exclusiveSecret(Store, 'exact.secret.', 'token', async () => {
      Store.set('token', 'partial');
      expect(disk.get(marker)).toBe('1');
      throw new Error('after a write');
    })).rejects.toThrow('after a write');
    expect([...disk]).toEqual([]);
  } finally {
    Store.map = new Map(); Store.writes = []; Store.pending.clear();
    for (const [key, descriptor] of saved) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  }
});

// One lease at a time in an answer (LLP 1146 D5): a lease asked for inside another is refused, where one Web Lock
// would wait for the other forever, and another answer's waits its turn.
test("an answer's lease inside its own is refused, and another answer's waits for it", async () => {
  const { exclusiveSecret } = await import('../web/storage-environment.js');
  const saved = ['navigator', 'localStorage', '__exact_async_context'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  const disk = new Map([['exact.secret.token', 'old']]), held = new Map();
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: k => disk.get(k) ?? null, setItem: (k, v) => disk.set(k, v), removeItem: k => disk.delete(k) } });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: { request: (name, work) => {
    const run = (held.get(name) ?? Promise.resolve()).then(() => work());
    held.set(name, run.catch(() => {}));
    return run;
  } } } });
  let current = null, reaction = null;
  class Owned extends Promise {}
  globalThis.__exact_async_context = { Promise: Owned, current: () => [current, null, '', null, reaction], swap(to) { const before = [current, null, '', null, reaction]; current = to[0]; reaction = to[4]; return before; } };
  const store = createSecretFacade({ get: k => Store.get(k), set: (k, v) => Store.set(k, v), exclusive: (k, work) => exclusiveSecret(Store, 'exact.secret.', k, work) },
    { version: 1, entries: [[1, "secret.keep token", ["secret-keep", "token"], null]], error: null, seal: "b22a759eedfe6bc2" }, () => null);
  const as = (call, run) => { const before = current; current = call; try { return run(); } finally { current = before; } };
  Store.map = new Map(); Store.writes = []; Store.pending.clear();
  try {
    const a = {}, b = {};
    let inner, other;
    let reacted;
    const first = as(a, () => store.exclusive('token', () => {
      inner = store.exclusive('token', async () => 'inner ran');
      // A promise reaction's answer, as ts-fetch.js's AnswerPromise makes it current
      reacted = (() => { const before = current; current = null; reaction = a; try { return store.exclusive('token', async () => 'reaction ran'); } finally { current = before; reaction = null; } })();
      other = as(b, () => store.exclusive('token', async () => store.get('token')));
      store.set('token', 'first');
      return 'first';
    }));
    // What follows a lease is still its answer's: the lease answers the answer's promise.
    expect(first).toBeInstanceOf(Owned);
    expect(await first).toBe('first');
    expect(await inner.catch(e => e.message)).toBe('nested secret leases are unsupported');
    expect(await reacted.catch(e => e.message)).toBe('nested secret leases are unsupported');
    expect(await other).toBe('first');
    expect(await as(a, () => store.exclusive('token', async () => store.get('token')))).toBe('first');
  } finally {
    Store.map = new Map(); Store.writes = []; Store.pending.clear();
    for (const [key, descriptor] of saved) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  }
});

// The grant set is the Rust parser's normalized `secret.keep token`.
test("a render's source facade reads no secret and refuses writes before calling its backing store", async () => {
  const saved = Object.getOwnPropertyDescriptor(globalThis, '__exactRender');
  let reads = 0, writes = 0, keys = 0;
  const store = createSecretFacade({ get() { reads++; return 'private'; }, set() { writes++; } },
    { version: 1, entries: [[1, "secret.keep token", ["secret-keep", "token"], null]], error: null, seal: "b22a759eedfe6bc2" }, () => { keys++; return Promise.resolve({ put() {} }); });
  globalThis.__exactRender = true;
  try {
    expect(store.get('token')).toBeNull();
    expect(store.has('token')).toBe(false);
    expect(await store.unlock('token')).toBeNull();
    expect(await store.key('token')).toBeNull();
    for (const write of [() => store.set('token', 'kept'), () => store.forget('token')]) {
      expect(write).toThrow("a render's store is empty and refuses writes");
    }
    for (const write of [() => store.exclusive('token', () => { throw new Error('work ran'); }), () => store.keepKey('token', {})]) {
      expect(await write().catch(e => e.message)).toBe("a render's store is empty and refuses writes");
    }
    expect({ reads, writes, keys, read: store.read }).toEqual({ reads: 0, writes: 0, keys: 0, read: true });
  } finally {
    if (saved) Object.defineProperty(globalThis, '__exactRender', saved); else delete globalThis.__exactRender;
  }
});

// The Rust parser's normalized `secret.keep token user-presence optional` and `secret.keep key user-presence`.
const presence = { entries: [[1, "secret.keep token user-presence optional", ["secret-keep", "token"], null], [2, "secret.keep key user-presence", ["secret-keep", "key"], null]], error: null, keepsNoAnswers: false, seal: "b130c25d0bbbe8c6", version: 1 };

test("every write to a name that must be locked is refused before either backing store changes (LLP 1133.010 D15)", async () => {
  const backing = Object.assign(new Map([['key', 'older']]), { exclusive: (_name, work) => work() }), puts = [];
  const keys = () => Promise.resolve({ put: (handle, pair) => { puts.push([handle, pair]); }, get: () => null });
  const store = createSecretFacade(backing, presence, keys);
  const pair = { privateKey: {}, publicKey: {} }, refused = { name: 'NotSupportedError' };
  expect(() => store.set('key', 's')).toThrow(expect.objectContaining(refused));
  await expect(store.exclusive('key', () => 'ran')).rejects.toMatchObject(refused);
  await expect(store.keepKey('key', pair)).rejects.toMatchObject(refused);
  await expect(store.protect('key')).rejects.toMatchObject(refused);
  expect([[...backing], puts]).toEqual([[['key', 'older']], []]);
  // An `optional` name stays plain: each write goes through.
  store.set('token', 'open');
  expect(await store.exclusive('token', () => 'ran')).toBe('ran');
  await store.keepKey('token', pair);
  expect(backing.get('token')).toMatch(/^exact\.key:/);
  expect(puts).toEqual([[backing.get('token'), pair]]);
});

test("a kept key's name holds its new handle only once the pair is kept, and the newest write of the name wins", async () => {
  const granted = { version: 1, entries: [[1, "secret.keep token", ["secret-keep", "token"], null]], error: null, seal: "b22a759eedfe6bc2" };
  const backing = new Map(), kept = new Map(), writes = [];
  let opening = null;
  // Each put waits until the test settles it: `writes.at(i).ok()` or `.fail(error)`.
  const service = {
    put: (handle, pair) => new Promise((resolve, reject) => writes.push({ ok: () => { kept.set(handle, pair); resolve(); }, fail: reject })),
    get: handle => kept.get(handle) ?? null,
  };
  const store = createSecretFacade(backing, granted, () => opening ?? Promise.resolve(service));
  const settle = () => new Promise(r => setTimeout(r, 0));
  const first = { privateKey: {}, publicKey: {} }, second = { privateKey: {}, publicKey: {} };

  const keeping = store.keepKey('token', first);
  await settle();
  expect(backing.has('token')).toBe(false);
  writes.at(-1).ok();
  await keeping;
  const held = backing.get('token');
  expect(await store.key('token')).toBe(first);

  // A refused write, and a key service that does not open, leave the key the name held.
  const quota = Object.assign(new Error('quota'), { name: 'QuotaExceededError' });
  const refused = store.keepKey('token', second);
  await settle();
  writes.at(-1).fail(quota);
  await expect(refused).rejects.toBe(quota);
  opening = Promise.reject(new Error('the database did not open'));
  await expect(store.keepKey('token', second)).rejects.toThrow('the database did not open');
  opening = null;
  expect(backing.get('token')).toBe(held);
  expect(await store.key('token')).toBe(first);

  // A forget while the pair is written is not undone when it lands.
  const forgotten = store.keepKey('token', second);
  await settle();
  store.forget('token');
  writes.at(-1).ok();
  await forgotten;
  expect(backing.get('token')).toBeNull();

  // Two replacements: the later call's pair is the name's, whichever lands first.
  const older = store.keepKey('token', first);
  await settle();
  const newer = store.keepKey('token', second);
  await settle();
  writes.at(-1).ok();
  await newer;
  writes.at(-2).ok();
  await older;
  expect(await store.key('token')).toBe(second);
});

test('secret names and values use the native string conversion', () => {
  const granted = { version: 1, entries: [[1, 'secret.keep token', ['secret-keep', 'token'], null]], error: null, seal: 'b22a759eedfe6bc2' };
  const backing = new Map(), store = createSecretFacade(backing, granted, () => Promise.resolve({}));
  const name = { toString: () => 'token' };
  store.set(name, 42);
  expect(backing.get('token')).toBe('42');
  expect(store.get(name)).toBe('42');
  store.forget(name);
  expect(store.get('token')).toBeNull();
});

test('denied secret writes throw ordinary errors', () => {
  const granted = { version: 1, entries: [[1, 'secret.keep token', ['secret-keep', 'token'], null]], error: null, seal: 'b22a759eedfe6bc2' };
  const store = createSecretFacade(new Map(), granted, () => Promise.resolve({}));
  const denied = (() => { try { store.set('missing', 'x'); } catch (error) { return error; } })();
  expect(denied).toBeInstanceOf(Error);
  expect(denied.refuse).toBeUndefined();
});

// LLP 1030.004 D2: another tab's write or removal of a key is taken, unless this tab owes its own write of it.
test("another tab's store write is taken unless this tab's own is owed", () => {
  Store.map = new Map([["token", "old"], ["gone", "x"]]); Store.writes = []; Store.pending.clear();
  Store.peer("token", "theirs");
  expect(Store.get("token")).toBe("theirs");
  Store.peer("gone", null);
  expect(Store.get("gone")).toBeUndefined();
  Store.set("token", "mine");
  Store.peer("token", "later");
  expect(Store.get("token")).toBe("mine");
  Store.writes = [];
});

// A request with effects arms its answer's lease before it goes (LLP 1146 D5), as a native host's park does; a GET
// or HEAD does not. ts-fetch.js is copied beside an admission whose fetch answers at once.
test("a lease's request with effects arms its recovery, and a safe one does not", async () => {
  const { mkdtempSync, cpSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { resolve } = await import('node:path');
  const { pathToFileURL } = await import('node:url');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-lease-fetch-'));
  try {
    writeFileSync(resolve(dir, 'ts-fetch.js'), runtimeSource('ts-fetch.js'));
    writeFileSync(resolve(dir, 'admission.js'), `export { fetchTarget, ownWithout, redirectOf, socketProtocols } from ${JSON.stringify(pathToFileURL(resolve(import.meta.dir, 'admission.js')).href)};
export class FetchError extends Error {} export const overlaying = { on: false }; export const inOverlay = what => new Error(what);
export const signalDeadline = { arm: null }; export const fetchWith = async () => new Response('sent');\n`);
    writeFileSync(resolve(dir, 'admission-data.js'), 'export const tsGrantSet = {};\n');
    const tsf = await import(pathToFileURL(resolve(dir, 'ts-fetch.js')).href);
    let arms = 0;
    const call = { lease: { arm: () => { arms++; } } };
    const sent = [];
    // The last one from a promise reaction of the answer's (ts-fetch.js `AnswerPromise`)
    for (const [input, init, field = 'call'] of [['https://api.test/a'], ['https://api.test/a', { method: 'head' }],
      [new Request('https://api.test/a')], ['https://api.test/a', { method: 'post' }], [new Request('https://api.test/a', { method: 'DELETE' })],
      ['https://api.test/a', { method: 'PUT' }, 'promiseCall']]) {
      tsf.answering[field] = call;
      const response = tsf.fetch(input, init);
      tsf.answering[field] = null;
      await response;
      sent.push(arms);
    }
    expect(sent).toEqual([0, 0, 0, 1, 2, 3]);
    // A request refused before it goes arms nothing.
    tsf.answering.call = { ...call, mutation: true, turn: true, socketOps: [{}] };
    const refused = tsf.fetch('https://api.test/a', { method: 'POST' });
    tsf.answering.call = null;
    expect(await refused.catch(e => e.message)).toBe(tsf.SOCKET_AND_FETCH);
    tsf.answering.call = call;
    const invalid = tsf.fetch('https://api.test/a', { method: 'POST', redirect: 'bad' });
    tsf.answering.call = null;
    expect(await invalid.catch(e => e.message)).toBe('redirect must be follow, manual or error');
    expect(arms).toBe(3);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('the render fetch boundary defers effective credentials before the cache or network', async () => {
  const { renderer } = await import('./render.mjs');
  const saved = globalThis.fetch, sent = [];
  globalThis.fetch = async (input, init) => {
    expect(init.credentials).toBe('omit');
    expect(new Headers(input.headers).has('cookie')).toBe(false);
    const url = typeof input === 'string' ? input : input.url;
    sent.push(url);
    return Object.defineProperty(new Response('public', { headers: { 'cache-control': 'public, max-age=600' } }), 'url', { value: url });
  };
  const files = {
    'index.html': '<html><head><title>Test</title><meta name="viewport" content="width=device-width">\n</head><body><div id="exact-root"></div><script type="module" data-entry src="app.js"></script></body></html>',
    '.gen/server.js': `globalThis.__exactRenderOrigin = 'https://site.test';
      globalThis.__start = async () => {
        const cases = [
          ['https://api.test/current', { credentials: 'include' }],
          [new Request('https://api.test/current', { credentials: 'include' })],
          ['https://site.test/current'],
          ['https://site.test/current', Object.create({ credentials: 'same-origin' })],
          ['https://site.test/public', { credentials: 'omit' }],
          [new Request('https://site.test/public', { credentials: 'omit' }).clone()],
          [new Request('https://api.test/current', { credentials: 'include' }), { credentials: 'omit' }],
          ['https://other.test/public'],
        ];
        const deferred = [];
        for (const args of cases) {
          try { await fetch(...args); deferred.push(false); }
          catch (error) { if (!error.renderDeferred) throw error; deferred.push(true); }
        }
        // The public response is already cached when the credentialed request asks for the same URL.
        try { await fetch('https://site.test/public'); deferred.push(false); }
        catch (error) { if (!error.renderDeferred) throw error; deferred.push(true); }
        return { activate: 'eager', deferred };
      };`,
  };
  let page;
  try {
    const render = renderer('', { read: path => {
      if (!(path in files)) throw Object.assign(new Error(path), { code: 'ENOENT' });
      return Buffer.from(files[path]);
    } });
    page = await render.begin('/');
    expect(page.deferred).toEqual([true, true, true, true, false, false, false, false, true]);
    expect(sent).toEqual(['https://site.test/public', 'https://api.test/current', 'https://other.test/public']);
  } finally { page?.dispose(); globalThis.fetch = saved; }
});

test('the web-session recipe selects cookies or native storage at run time', async () => {
  const vm = await import('node:vm');
  const { readFileSync } = await import('node:fs');
  const { resolve } = await import('node:path');
  const {transformSync} = await import('rolldown/utils');
  const recipe = readFileSync(resolve(import.meta.dir, '../../docs/recipes/web-sessions.md'), 'utf8');
  expect(recipe).toMatch(/JS target[\s\S]*preserves\s+`credentials`/);
  expect(recipe).toMatch(/wasm target, including workers,[\s\S]*same-origin endpoints/);
  const source = recipe.match(/```ts\n([\s\S]*?)\n```/)[1].replace(/^import type .*;\n/, '');
  const code = transformSync('session.ts', source, {}).code;
  for (const host of ['web', 'native']) {
    const kept = new Map();
    const store = {get: name => kept.get(name), set: (name, value) => kept.set(name, value), forget: name => kept.delete(name)};
    const context = vm.createContext({exactHost: () => host, Headers});
    vm.runInContext(code, context);
    const transport = context.sessionTransport(store);
    expect(transport.path).toBe(`/session/${host}`);
    transport.remember(host === 'web' ? {} : {token: 'rotated'});
    const options = transport.options({headers: {'x-csrf-token': 'proof'}});
    expect(options.credentials).toBe(host === 'web' ? 'include' : 'omit');
    expect(options.headers.get('x-csrf-token')).toBe('proof');
    expect(options.headers.get('authorization')).toBe(host === 'web' ? null : 'Bearer rotated');
    expect([...kept]).toEqual(host === 'web' ? [] : [['session', 'rotated']]);
    transport.forget();
    expect(kept.size).toBe(0);
  }
});

test('the web-session recipe uses an HttpOnly cookie through sign-in, rotation and logout', async () => {
  const { mkdtempSync, readFileSync, rmSync, writeFileSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { resolve } = await import('node:path');
  const { spawnSync } = await import('node:child_process');
  const { suiteBrowser } = await import('../../scripts/agent-launch.mjs');
  const root = resolve(import.meta.dir, '../..'), app = mkdtempSync(resolve(tmpdir(), 'exact-web-session-'));
  const out = resolve(app, 'dist'), cookies = [];
  let browser, pageServer, api, token = '';
  try {
    api = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const origin = pageServer?.url.origin;
      const headers = { 'access-control-allow-origin': origin ?? 'http://unbound.invalid',
        'access-control-allow-credentials': 'true', 'vary': 'Origin' };
      if (request.headers.get('origin') !== origin) return new Response(null, { status: 403 });
      if (request.method === 'OPTIONS') return new Response(null, { status: 204, headers: { ...headers,
        'access-control-allow-methods': 'GET, POST', 'access-control-allow-headers': 'x-csrf-token' } });
      const path = new URL(request.url).pathname;
      const cookie = request.headers.get('cookie') ?? '';
      cookies.push([path, cookie]);
      if (request.headers.get('x-csrf-token') !== 'fixture-proof') return new Response(null, { status: 403, headers });
      if (path !== '/session/web/sign-in' && (!token || cookie !== `__Host-session=${token}`)) return new Response(null, { status: 401, headers });
      if (path === '/session/web/sign-in' || path === '/session/web/refresh') {
        token = path.endsWith('/sign-in') ? 'first-private-token' : 'rotated-private-token';
        headers['set-cookie'] = `__Host-session=${token}; HttpOnly; Secure; SameSite=Strict; Path=/`;
      } else if (path === '/session/web/logout') {
        token = '';
        headers['set-cookie'] = '__Host-session=; Max-Age=0; HttpOnly; Secure; SameSite=Strict; Path=/';
      }
      return Response.json({ ok: true }, { headers });
    } });
    const recipe = readFileSync(resolve(root, 'docs/recipes/web-sessions.md'), 'utf8');
    const adapter = recipe.match(/```ts\n([\s\S]*?)\n```/)[1];
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({name:'Session', app:{id:'test.web-session', name:'Session'}, host:{web:{}}}));
    writeFileSync(resolve(app, 'app.contract'), 'component App\n  resource result = probe() as shape string\n  view\n    text result testId="result"\n');
    writeFileSync(resolve(app, 'app.ts'), adapter + `
import type { Answer } from './app.contract.d.ts';
export const grants = 'net.fetch ${api.url.origin}\\nsecret.keep session';
export const answer: Answer = async (_source, _args, store) => {
  const session = sessionTransport(store);
  const call = (path: string, method = 'POST') => fetch('${api.url.origin}' + path,
    session.options({method, headers:{'x-csrf-token':'fixture-proof'}}));
  for (const operation of ['/sign-in', '/refresh']) {
    const response = await call(session.path + operation);
    if (!response.ok || response.headers.get('set-cookie')) throw new Error('session exchange failed');
    session.remember(await response.json());
  }
  if (!(await call('/items', 'GET')).ok) throw new Error('cookie was not sent');
  if (!(await call(session.path + '/logout')).ok) throw new Error('logout failed');
  session.forget();
  if ((await call('/items', 'GET')).status !== 401) throw new Error('logout left a session');
  return exactHost() + '/' + globalThis.exactHost();
};
`);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'session', '--out', out, '--production', '--render', 'none'],
      {cwd:root, env:{...process.env, EXACT_APP_DIR:app}, encoding:'utf8'});
    expect(built.status, built.stdout + built.stderr).toBe(0);
    pageServer = Bun.serve({hostname:'127.0.0.1', port:0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(out, path === '/' ? 'index.html' : path.slice(1))));
    }});
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(pageServer.url.href);
    await page.getByTestId('result').filter({hasText:'web/web'}).waitFor();
    expect(errors).toEqual([]);
    expect(cookies).toEqual([
      ['/session/web/sign-in', ''], ['/session/web/refresh', '__Host-session=first-private-token'],
      ['/items', '__Host-session=rotated-private-token'], ['/session/web/logout', '__Host-session=rotated-private-token'], ['/items', ''],
    ]);
    expect(await page.evaluate(() => ({cookie:document.cookie, secrets:Object.keys(localStorage).filter(k => k.startsWith('exact.secret.'))})))
      .toEqual({cookie:'', secrets:[]});
  } finally {
    await browser?.close(); pageServer?.stop(true); api?.stop(true); rmSync(app, {recursive:true, force:true});
  }
}, 120000);

test.each(['TypeScript', 'Rust'])('a rendered %s page leaves session reads to the browser with its existing cookie', async language => {
  const { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { resolve } = await import('node:path');
  const { suiteBrowser } = await import('../../scripts/agent-launch.mjs');
  const root = resolve(import.meta.dir, '../..'), app = mkdtempSync(resolve(tmpdir(), 'exact-render-session-'));
  const out = resolve(app, 'dist'), requests = [];
  let browser, site;
  try {
    site = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/current' || path === '/declared') {
        const cookie = request.headers.get('cookie') ?? '';
        requests.push(cookie);
        const state = cookie === '__Host-session=existing-private-token' ? 'signed in' : 'signed out';
        return path === '/declared' ? Response.json(state) : new Response(state);
      }
      if (path === '/public') return new Response('public');
      if (path === '/existing-session') return new Response('signed in', { headers: {
        'set-cookie': '__Host-session=existing-private-token; HttpOnly; Secure; SameSite=Strict; Path=/',
      } });
      return new Response(Bun.file(resolve(out, path === '/' ? 'index.html' : path.slice(1))));
    } });
    const recipe = readFileSync(resolve(root, 'docs/recipes/web-sessions.md'), 'utf8');
    const adapter = recipe.match(/```ts\n([\s\S]*?)\n```/)[1];
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Session',
      app: { id: 'test.render-session', name: 'Session', origin: site.url.origin + '/' }, host: { web: {} },
      ...(language === 'Rust' ? { rust: { data: { crate: 'render-session-data', type: 'render_session_data::Session' } } } : {}) }));
    writeFileSync(resolve(app, 'app.contract'), `routes nav
  home "/" render=build

source declared() as shape string
  url "${site.url.origin}/declared"

component App
  resource declared = declared() as shape string
  resource current = current() as shape string
  resource sameOrigin = sameOrigin() as shape string
  resource public = public() as shape string
  view
    column
      text current testId="current"
      text sameOrigin testId="sameOrigin"
      text declared testId="declared"
      text public testId="public"
      text (pending(current) ? "pending" : "answered") testId="state"
`);
    if (language === 'TypeScript') writeFileSync(resolve(app, 'app.ts'), adapter + `
import type { Answer } from './app.contract.d.ts';
export const grants = 'net.fetch ${site.url.origin}\\nsecret.keep session';
export const answer: Answer = async (source, _args, store) => {
  await Promise.resolve();
  const options = source === 'current' ? sessionTransport(store).options()
    : source === 'public' ? { credentials: 'omit' as const } : {};
  return (await fetch('${site.url.origin}/' + (source === 'public' ? 'public' : 'current'), options)).text();
};
`);
    else {
      mkdirSync(resolve(app, 'data/src'), { recursive: true });
      writeFileSync(resolve(app, 'data/Cargo.toml'), `[package]
name = "render-session-data"
version.workspace = true
edition.workspace = true
workspace = "../.exact/shells"
[dependencies]
exact-runner.workspace = true
`);
      writeFileSync(resolve(app, 'data/src/lib.rs'), `use exact_runner::{Answer, DataError, DataSource, Outcome, Request, Store, Value};
#[derive(Default)]
pub struct Session;
impl DataSource for Session {
    fn grants(&self) -> &str { "net.fetch ${site.url.origin}" }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> { Ok(Value::str("")) }
    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        if source == "public" { return Ok(Answer::Now(Value::str("public"))); }
        Ok(Answer::Later(Request::get("${site.url.origin}/current")))
    }
    fn parse(&mut self, _: &mut Store, _: &str, _: &[Value], outcome: Outcome) -> Result<Answer, DataError> {
        let Outcome::Response(response) = outcome else { return Err(DataError::Unavailable(format!("{outcome:?}"))); };
        Ok(Answer::Now(Value::str(&String::from_utf8(response.body).unwrap())))
    }
}
`);
    }
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'session', '--out', out, '--production', '--render', 'js'],
      { cwd: root, env: { ...process.env, EXACT_APP_DIR: app, CARGO_TARGET_DIR: resolve(root, 'target') }, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    const html = readFileSync(resolve(out, 'index.html'), 'utf8');
    const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(html)[1]);
    expect(checkpoint.pending).toEqual(['declared', 'current', 'sameOrigin']);
    expect(checkpoint.answers.map(a => a[0])).toEqual(['public']);
    expect(html).toMatch(/data-testid="state"[^>]*>pending</);
    expect(requests).toEqual([]);
    browser = await suiteBrowser();
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto(new URL('/existing-session', site.url).href);
    expect((await context.cookies())[0]).toMatchObject({ name: '__Host-session', httpOnly: true, secure: true, sameSite: 'Strict' });
    await page.goto(site.url.href);
    await page.getByTestId('current').filter({ hasText: 'signed in' }).waitFor();
    await page.getByTestId('sameOrigin').filter({ hasText: 'signed in' }).waitFor();
    await page.getByTestId('declared').filter({ hasText: 'signed in' }).waitFor();
    expect(requests).toEqual(Array(3).fill('__Host-session=existing-private-token'));
    expect(await page.evaluate(() => document.cookie)).toBe('');
    expect(await page.evaluate(() => globalThis.exact.journal)).toContainEqual(expect.stringMatching(/adopted the document$/));
  } finally {
    await browser?.close(); site?.stop(true); rmSync(app, { recursive: true, force: true });
  }
}, 120000);

test('render origins normalize paths, default ports and host case', async () => {
  const { renderedPages } = await import('./rendered-pages.mjs');
  for (const origin of ['https://site.test/', 'https://site.test:443', 'https://SITE.TEST', 'https://site.test/app', 'https://site.test:8443/']) {
    const target = new URL(origin).origin;
    await renderedPages(`routes nav
  home "/" render=build
source current() as shape string
  url "${target}/current"
component App
  resource current = current() as shape string
  view
    text current testId="current"
`, (_head, html) => {
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(html(''))[1]);
      expect(checkpoint.pending, origin).toEqual(['current']);
      expect(checkpoint.answers, origin).toEqual([]);
    }, {}, { app: { id: 'test.origin', name: 'Origin', origin } });
  }
}, 120000);

test('a render resolves data against its own URL and holds only its named network grants', async () => {
  const { renderer, pageServer } = await import('./render.mjs');
  const { renderedPages } = await import('./rendered-pages.mjs');
  const hits = [];
  const servers = Array.from({length: 2}, () => Bun.serve({port: 0, async fetch(request) {
    hits.push(request.url);
    const path = new URL(request.url).pathname;
    if (path.endsWith('/post303') || path.endsWith('/post307')) return new Response(null, {status: Number(path.slice(-3)), headers: {Location: 'echo'}});
    if (path.endsWith('/echo')) return new Response(request.method + ':' + await request.text());
    if (path.endsWith('/empty')) return new Response(null, {status: 302, headers: {Location: ''}});
    if (path.endsWith('/redirect')) return new Response(null, {status: 302, headers: {Location: 'final.json'}});
    if (path.endsWith('/ungranted')) return new Response(null, {status: 302, headers: {Location: 'https://ungranted.invalid/'}});
    if (path.endsWith('/events')) return new Response('data: from-stream\n\n', {headers: {'content-type': 'text/event-stream'}});
    return Response.json(request.url, { headers: { 'cache-control': 'public, max-age=60' } });
  }}));
  const [one, two] = servers.map(s => s.url.origin);
  try {
    await renderedPages(`routes nav
  home "/" render=request
  item "/articles/:id" render=request
component App
  derive id = match stack(nav).at(-1) { case some(e) => e.params.id, case none => "" }
  resource label = label(id) as shape string
  view
    text label
`, async (_, __, dist) => {
      const render = renderer(dist);
      await expect(render('about:blank')).rejects.toThrow('absolute HTTP(S) URL');
      const locations = [one, two, one].map(origin => origin + '/articles/entry');
      const pages = await Promise.all(locations.map(location => render(location)));
      for (const [i, page] of pages.entries()) expect(page.html).toContain(new URL('data.json', locations[i]).href);
      expect(new Set(hits)).toEqual(new Set([one + '/articles/data.json', two + '/articles/data.json']));
      const before = hits.length;
      expect((await render(locations[0])).html).toContain(one + '/articles/data.json');
      expect(hits).toHaveLength(before);
      expect((await render(one + '/articles/redirect')).html).toContain(one + '/articles/final.json cloned redirect');
      for (const [id, text] of [['post303', 'GET:'], ['post307', 'POST:sample'], ['empty', 'status 302']]) {
        expect((await render(one + '/articles/' + id)).html).toContain(text);
      }
      const denied = await render(one + '/articles/ungranted');
      expect(denied.refused).toEqual([]);
      expect(denied.html).toContain('redirected to https://ungranted.invalid');
      const stream = await render(two + '/articles/stream');
      expect(stream.html).toContain('from-stream');
      expect(stream.html).toContain(JSON.stringify([[two + '/articles/events']]));
      expect(hits).toContain(two + '/articles/events');
      const header = name => ({ host: 'unused.example', 'x-forwarded-host': 'unused.example' })[name];
      const missing = await pageServer(dist, render, () => {})('/articles/entry', header);
      expect(missing.body).toContain('relative fetch has no page origin');
      expect(missing.body).toContain('set app.origin');
      const pending = await render('http://example.invalid/articles/entry');
      expect(pending.refused).toEqual([['label', "fetch() outside the render server's grants"]]);
      const readerStream = await render('http://example.invalid/articles/stream');
      expect(readerStream.refused).toEqual([['label', "fetch() outside the render server's grants"]]);
    }, { 'app.ts': `export const grants = ${JSON.stringify('net.fetch http:\nnet.fetch ' + one + '\nnet.fetch ' + two)};
export const answer = async (_: string, [id]: string[]) => {
  if (id === 'stream') return fetch('events', {credentials: 'omit', exactStream: e => e.data || 'from-stream'});
  try {
    if (id === 'empty') return 'status ' + (await fetch(id, { credentials: 'omit' })).status;
    if (id === 'post303' || id === 'post307') return await (await fetch(id, {method: 'POST', body: 'sample', mode: 'same-origin', credentials: 'omit'})).text();
    const response = await fetch(new Request(id === 'redirect' || id === 'ungranted' ? id : 'data.json', { credentials: 'omit' }));
    if (id === 'redirect') {
      const copy = response.clone();
      return await copy.json() + (response.redirected && copy.redirected ? ' cloned redirect' : '');
    }
    return await response.json();
  }
  catch (e) { return String(e); }
};` });
  } finally { for (const server of servers) server.stop(true); }
}, 60000);

test('build-time data takes the manifest origin and a missing origin names the fix', async () => {
  const { renderedPages } = await import('./rendered-pages.mjs');
  const hits = [];
  const site = Bun.serve({port: 0, fetch(req) {
    hits.push(req.url);
    return new Response('published');
  }});
  try {
    for (const [configured, grant] of [[false, site.url.origin], [true, site.url.origin], [true, 'self']]) {
      hits.length = 0;
      await renderedPages(`routes nav
  home "/" render=request
  guide "/guides/one" render=build
component App
  resource label = read() as shape string
  view
    text label
`, async (_, html) => {
        expect(html('guides/one')).toContain(configured ? 'published' : 'relative fetch has no page origin: set app.origin');
        expect(hits).toEqual(configured ? [site.url.origin + '/guides/data.json'] : []);
      }, { 'app.ts': `export const grants = 'net.fetch ${grant}';
export const answer = async () => {
  try { return await (await fetch('data.json', {credentials: 'omit'})).text(); }
  catch (e) { return String(e); }
};` }, { app: {id: 'test.build', name: 'Build', ...(configured ? { origin: site.url.origin } : {})} });
    }
  } finally { site.stop(true); }
}, 60000);

test('one build with net.fetch self resolves data against each trusted render URL', async () => {
  const { renderer } = await import('./render.mjs');
  const { renderedPages } = await import('./rendered-pages.mjs');
  const hits = [];
  const sites = Array.from({ length: 3 }, () => Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    hits.push(request.url);
    if (new URL(request.url).pathname === '/guides/redirect') return Response.redirect(sites[2].url.origin + '/api');
    return new Response(request.url);
  }}));
  const [named, tenant, api] = sites.map(site => site.url.origin);
  try {
    for (const grants of ['net.fetch self', ['net.fetch self', 'net.fetch http:', 'net.fetch ' + named, 'net.fetch ' + api].join('\n')]) {
      hits.length = 0;
      await renderedPages(`routes nav
  home "/" render=request
  page "/guides/:id" render=request
component App
  derive id = match stack(nav).at(-1) { case some(e) => e.params.id, case none => "" }
  resource label = read(id) as shape string
  view
    text label
`, async (_, __, dist) => {
        const render = renderer(dist);
        for (const origin of [named, tenant]) {
          const page = await render(origin + '/guides/one');
          expect(page.html).toContain(origin + '/guides/data.json');
        }
        if (grants === 'net.fetch self') {
          expect((await render(tenant + '/guides/redirect')).html).toContain("outside the app's grants");
          expect((await render('/guides/one')).html).toContain('needs a trusted HTTP(S) page URL');
          expect(hits).toEqual([named + '/guides/data.json', tenant + '/guides/data.json', tenant + '/guides/redirect']);
          return;
        }
        expect((await render(named + '/guides/redirect')).html).toContain(api + '/api');
        expect((await render(named + '/guides/same')).html).toContain('same-origin fetch redirected');
        const deferred = await render(tenant + '/guides/redirect');
        expect(deferred.refused).toEqual([['label', "fetch() redirected outside the render server's grants"]]);
        expect((await render(tenant + '/guides/same')).html).toContain('same-origin fetch redirected');
        expect((await render(tenant + '/guides/authority')).html).toContain('host predicate is private');
        const withoutPage = await render('/guides/one');
        expect(withoutPage.refused).toEqual([['label', "fetch() outside the render server's grants"]]);
        expect(hits).toEqual([named + '/guides/data.json', tenant + '/guides/data.json', named + '/guides/redirect', api + '/api', named + '/guides/redirect', tenant + '/guides/redirect', tenant + '/guides/redirect']);
      }, { 'app.ts': `export const grants = ${JSON.stringify(grants)};
export const answer = async (_: string, [id]: string[]) => {
  try {
    if (id === 'authority') return [globalThis, fetch, import.meta].every(object =>
      !('__exactPageAdmits' in object || 'admitsPageOrigin' in object || 'exactPageAdmits' in object)) ? 'host predicate is private' : 'exposed';
    if (id === 'same') return await (await fetch('redirect', {credentials: 'omit', mode: 'same-origin'})).text();
    return await (await fetch(id === 'redirect' ? id : 'data.json', {credentials: 'omit'})).text();
  }
  catch (e) { return String(e); }
};` }, { app: { id: 'test.self', name: 'Self', origin: tenant } });
    }
  } finally { for (const site of sites) site.stop(true); }
}, 60000);
