import { hostFixtureFile } from './tests/glue-fixture.mjs';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import vm from 'node:vm';
import { modulePrelude } from '../web-js/glue.mjs';

const ROOT = resolve(import.meta.dir, '../..');
const GRANTS = {version:1,entries:[[1,'net.fetch https://api.castle.test',['fetch','https','api.castle.test',443],null]],error:null,seal:'825ec7a8503217b3'};

// The prelude replaces AbortSignal.timeout. Give each VM its own constructor
// while accepting the native signals made by the host's AbortController.
export function isolatedAbortSignal() {
  const PlatformAbortSignal = globalThis.AbortSignal;
  class ModuleAbortSignal extends PlatformAbortSignal {}
  Object.defineProperty(ModuleAbortSignal, Symbol.hasInstance, {
    value: signal => signal instanceof PlatformAbortSignal,
  });
  return ModuleAbortSignal;
}

// The real prelude and host turns. Selected lease cases make every storage
// backend promise land in a later task. `done` closes either placement even
// when an answer is still parked.
export async function moduleRealm(script, {
  placement = 'worker', key = 'k', top = '', secrets = false, stored = [],
  controlWorkerMessages = false, taskDelayedStorage = false,
} = {}) {
  const declaration = 'net.fetch https://api.castle.test' + (secrets ? '\nsecret.keep token' : '');
  const normalized = secrets ? { version: 1, entries: [...GRANTS.entries, [2, 'secret.keep token', ['secret-keep', 'token'], null]], error: null, seal: '7c439325539248a1' } : GRANTS;
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-worker-storage-')), captured = {};
  const saved = Object.fromEntries(['fetch', 'document', 'location', 'exact', 'Worker', '__exact_test_storage_event', '__exact_test_release_held_storage'].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  let realm;
  const done = () => {
    realm?.dispose();
    for (const [name, descriptor] of Object.entries(saved)) { if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name]; }
    rmSync(dir, { recursive: true, force: true });
  };
  try {
    for (const name of ['module-glue.js', 'module-worker.js', 'storage.js', 'grant-admission.js', 'faults.js', 'navigation.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
    writeFileSync(resolve(dir, 'storage-environment.js'), `export const directories = Object.freeze({ data: 'app:/data', cache: 'app:/cache', temporary: 'app:/tmp' });
export const storageKey = () => ${JSON.stringify(key)}; export const agentStorageRefusal = 'no store'; export const agentReads = ['readdir']; export const emptyRead = () => []; export const agentSeed = () => null;
export const agentStream = () => null; export const keyStore = () => ({}); export const now = () => 0;
export { refuseCodeFromStrings } from ${JSON.stringify(resolve(ROOT, 'host/web/storage-environment.js'))};`);
    writeFileSync(resolve(dir, 'storage-fs.js'), `const nextTask = () => ${taskDelayedStorage ? `new Promise(resolve => {
  const channel = new MessageChannel();
  channel.port1.onmessage = () => { channel.port1.close(); channel.port2.close(); resolve(); };
  channel.port2.postMessage(null);
})` : 'Promise.resolve()'};
const report = (phase, path) => {
  if (globalThis.__exact_test_storage_event) globalThis.__exact_test_storage_event(phase, path);
  else globalThis.postMessage?.({ op: 'test-storage', phase, path });
};
export const createFileSystem = () => {
  const files = new Map();
  return { readFile: async path => {
    report('start', path);
    await nextTask();
    if (path.endsWith('/held')) await new Promise(resolve => {
      globalThis.__exact_test_release_held_storage = resolve;
      report('held', path);
    });
    if (path.endsWith('/reject')) { report('reject', path); throw new Error('scripted storage rejection'); }
    report('finish', path);
    return files.get(path) ?? new TextEncoder().encode(path);
  }, readdir: async () => { await nextTask(); return []; },
    writeFile: async (path, bytes) => { report('start', path); await nextTask(); files.set(path, bytes); report('finish', path); }, dispose() {} };
};`);
    writeFileSync(resolve(dir, 'storage-sqlite.js'), `export const createSqlite = () => ({
  open: async () => ({ query: async (_sql, params) => params,
    prepare: async () => ({ query: async params => params, close: async () => {} }), close: async () => {} }), dispose() {} });`);
    const prelude = modulePrelude(ROOT);
    const binary = readFileSync(resolve(ROOT, 'js/src/binary.js'), 'utf8');
    const bytes = new TextEncoder().encode(`exact = { abi: 1, appId: 'test.storage', grants: ${JSON.stringify(declaration)}, answer(source, args, store, storage) {
  const read = path => storage.fs.readFile(path).then(bytes => new TextDecoder().decode(bytes));
  ${script}
} };
${top}`);
    const sha = new Bun.CryptoHasher('sha256').update(bytes).digest('hex');
    globalThis.fetch = async url => { const r = new Response(String(url).endsWith('module-prelude.js') ? prelude : String(url).endsWith('module-binary.js') ? binary : 'never'); Object.defineProperty(r, 'url', { value: 'http://127.0.0.1/x' }); return r; };
    globalThis.location = { href: 'http://127.0.0.1/', origin: 'http://127.0.0.1' };
    globalThis.exact = {};
    captured.storageEvents = [];
    captured.storageBarriers = [];
    globalThis.__exact_test_storage_event = (phase, path) => {
      captured.storageEvents.push([phase, path]);
      for (const barrier of captured.storageBarriers) if (barrier.phase === phase && barrier.path === path) barrier.done();
    };
    if (placement === 'worker' && (controlWorkerMessages || taskDelayedStorage)) {
      const NativeWorker = globalThis.Worker;
      const messages = { held: false, queue: [], barriers: [] };
      captured.messages = messages;
      globalThis.Worker = class {
        constructor(...args) {
          this.worker = new NativeWorker(...args);
          messages.dispatch = event => this.onmessage?.(event);
          this.worker.onmessage = event => {
            if (event.data?.op === 'test-storage') {
              globalThis.__exact_test_storage_event(event.data.phase, event.data.path);
              return;
            }
            if (!messages.held) { messages.dispatch(event); return; }
            messages.queue.push(event);
            for (const [count, done] of messages.barriers) if (messages.queue.length >= count) done();
          };
          this.worker.onerror = event => this.onerror?.(event);
          this.worker.onmessageerror = event => this.onmessageerror?.(event);
        }
        postMessage(...args) { return this.worker.postMessage(...args); }
        terminate() { return this.worker.terminate(); }
      };
    }
    if (placement === 'main') {
      const win = vm.createContext({ console: { log() {}, info() {}, warn() {}, error() {}, debug() {} }, TextEncoder, TextDecoder, structuredClone, setTimeout, clearTimeout,
        queueMicrotask, URL, Headers, DOMException, crypto, indexedDB: null, MessageChannel, Uint8Array, ArrayBuffer, AbortController, AbortSignal: isolatedAbortSignal(), addEventListener() {} });
      win.window = win;
      captured.win = win;
      for (const name of ['Promise', 'Error', 'SyntaxError', 'Object', 'Array', 'JSON', 'Symbol', 'Function', 'eval']) win[name] = vm.runInContext(name, win);
      win.document = { createElement: () => ({}), head: { append: el => vm.runInContext(el.textContent, win) } };
      const frame = { contentWindow: win, setAttribute() {}, remove() {} };
      Object.defineProperty(globalThis, 'document', { configurable: true, value: { createElement: () => frame, body: { append() {} } } });
      // The realm's door, kept as module-glue installs it (the prelude takes and deletes it at load).
      Object.defineProperty(win, '__exact_host', { configurable: true, set(door) {
        captured.door = door; Object.defineProperty(win, '__exact_host', { configurable: true, writable: true, value: door });
      } });
    }
    const { createGrantSet } = await import(resolve(dir, 'grant-admission.js'));
    const glue = await import(resolve(dir, 'module-glue.js'));
    const grants = declaration;
    const receipt = new TextEncoder().encode(JSON.stringify({ version: 1, abi: 1, appId: 'test.storage', grants, web: { file: 'app.js', bytes: bytes.length, sha256: sha }, module: { sha256: 'b'.repeat(64) } }));
    realm = await glue.prepare({ receipt, script: bytes }, { appId: 'test.storage', grants, grantSet: createGrantSet(normalized), placement });
    captured.runnerStore = new Map(stored);
    const commit = result => { for (const [name, value] of result.writes ?? []) {
      if (value == null) captured.runnerStore.delete(name); else captured.runnerStore.set(name, value);
    } };
    const request = (op, source, extra = {}) => ({ op, id: realm.id, target: source, source, args: [], store: [['recovery-owner', source], ...captured.runnerStore], grants: secrets ? ['token'] : [], ...extra });
    return {
      done, door: () => captured.door, aborted: () => glue.call({ op: 'aborted', id: realm.id }).tickets,
      ask: source => glue.run(glue.call(request('answer', source)).continuation),
      askWithDeliver: (source, deliver) => glue.run(glue.call(request('answer', source)).continuation, deliver),
      resume: (source, response = { status: 200, headers: [], body: 'fresh' }) => glue.run(glue.call(request('resume', source, { outcome: { response } })).continuation),
      resumeWithDeliver: (source, response, deliver) => glue.run(glue.call(request('resume', source, { outcome: { response: response ?? { status: 200, headers: [], body: 'fresh' } } })).continuation, deliver),
      commit,
      runnerValue: name => captured.runnerStore.get(name) ?? null,
      background: () => glue.call({ op: 'background', id: realm.id }),
      drain: () => glue.run(glue.call({ op: 'background-round', id: realm.id }).token),
      forget: inFlight => glue.call({ op: 'forget', id: realm.id, inFlight: inFlight.map(s => request('answer', s)) }),
      overlay: source => glue.call({ op: 'overlay', id: realm.id, source, args: [], answer: [], writes: [] }),
      holdWorkerMessagesAfter: count => {
        const dispatch = captured.messages.dispatch;
        captured.messages.dispatch = event => {
          dispatch(event);
          if (--count === 0) captured.messages.held = true;
        };
      },
      workerMessages: count => captured.messages.queue.length >= count ? Promise.resolve() : new Promise(done => captured.messages.barriers.push([count, done])),
      workerMessage: index => captured.messages.queue[index]?.data,
      releaseWorkerMessages: () => {
        captured.messages.held = false;
        for (const event of captured.messages.queue.splice(0)) captured.messages.dispatch(event);
      },
      storageEvents: () => captured.storageEvents.slice(),
      releaseHeldStorage: () => globalThis.__exact_test_release_held_storage(),
      callGlobal: name => captured.win[name](),
      storageEvent: (phase, path) => captured.storageEvents.some(event => event[0] === phase && event[1] === path)
        ? Promise.resolve()
        : new Promise(done => captured.storageBarriers.push({ phase, path, done })),
    };
  } catch (error) { done(); throw error; }
}
export const race = p => Promise.race([p.then(v => v, e => `rejected: ${e.message}`), new Promise(r => setTimeout(() => r('STILL WAITING'), 1500))]);

export function secretEnvironment(kept) {
  const saved = Object.fromEntries(['navigator', 'localStorage'].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  const disk = new Map([['exact.secret.token', kept]]), queues = new Map(), held = new Set();
  let writesFail = false, refuseNext = false, requested = 0, granted = 0, entered = 0;
  const requestBarriers = [], grantBarriers = [], enterBarriers = [];
  const grant = () => { granted++; for (const [count, done] of grantBarriers) if (granted >= count) done(); };
  const enter = () => { entered++; for (const [count, done] of enterBarriers) if (entered >= count) done(); };
  const later = () => new Promise(resolve => {
    const channel = new MessageChannel();
    channel.port1.onmessage = () => { channel.port1.close(); channel.port2.close(); resolve(); };
    channel.port2.postMessage(null);
  });
  const pump = name => {
    const queue = queues.get(name);
    if (held.has(name) || !queue?.length) return;
    const next = queue.shift();
    held.add(name);
    void (async () => {
      await later();
      await later();
      if (next.cancelled) { held.delete(name); pump(name); return; }
      next.lockGranted = true;
      grant();
      try {
        const running = next.work({ name, mode: 'exclusive' });
        enter();
        next.resolve(await running);
      }
      catch (error) { next.reject(error); }
      finally { held.delete(name); pump(name); }
    })();
  };
  const locks = { request(name, options, work) {
    if (typeof options === 'function') { work = options; options = {}; }
    requested++;
    for (const [count, done] of requestBarriers) if (requested >= count) done();
    if (options.ifAvailable && held.has(name)) return (async () => { await later(); await later(); return work(null); })();
    return new Promise((resolve, reject) => {
      const next = { work, resolve, reject, lockGranted: false, cancelled: false, refused: refuseNext };
      refuseNext = false;
      const abort = () => {
        if (next.lockGranted || next.cancelled) return;
        next.cancelled = true;
        reject(new Error('aborted'));
      };
      options.signal?.addEventListener('abort', abort, { once: true });
      const finish = next.resolve;
      next.resolve = value => { options.signal?.removeEventListener('abort', abort); finish(value); };
      if (next.refused) {
        void (async () => { await later(); await later(); next.reject(new Error('lock refused')); })();
        return;
      }
      (queues.get(name) ?? queues.set(name, []).get(name)).push(next);
      pump(name);
    });
  } };
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks } });
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: key => disk.get(key) ?? null, setItem: (key, value) => { if (writesFail) throw new Error('quota'); disk.set(key, value); }, removeItem: key => disk.delete(key) } });
  return { disk, requested(count) { return requested >= count ? Promise.resolve() : new Promise(done => requestBarriers.push([count, done])); },
    granted(count) { return granted >= count ? Promise.resolve() : new Promise(done => grantBarriers.push([count, done])); },
    entered(count) { return entered >= count ? Promise.resolve() : new Promise(done => enterBarriers.push([count, done])); },
    checkpoint: later, requestCount: () => requested, grantCount: () => granted,
    failWrites() { writesFail = true; }, refuseNext() { refuseNext = true; }, restore() {
    for (const [name, descriptor] of Object.entries(saved)) { if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name]; }
  } };
}

export const TASK_DELAYED_LOCKS = `{
  const queues = new Map(), held = new Set();
  const later = () => new Promise(resolve => {
    const channel = new MessageChannel();
    channel.port1.onmessage = () => { channel.port1.close(); channel.port2.close(); resolve(); };
    channel.port2.postMessage(null);
  });
  const pump = name => {
    const queue = queues.get(name);
    if (held.has(name) || !queue?.length) return;
    const next = queue.shift();
    held.add(name);
    void (async () => {
      await later();
      await later();
      try { next.resolve(await next.work({name, mode:'exclusive'})); }
      catch (error) { next.reject(error); }
      finally { held.delete(name); pump(name); }
    })();
  };
  const locks = { request(name, options, work) {
    if (typeof options === 'function') { work = options; options = {}; }
    if (options.ifAvailable && held.has(name)) return (async () => { await later(); await later(); return work(null); })();
    return new Promise((resolve, reject) => {
      (queues.get(name) ?? queues.set(name, []).get(name)).push({work, resolve, reject});
      pump(name);
    });
  } };
  const moduleNavigator = typeof navigator === 'object' ? navigator : {};
  if (typeof navigator !== 'object') Object.defineProperty(globalThis, 'navigator', {configurable:true, value:moduleNavigator});
  Object.defineProperty(moduleNavigator, 'locks', {configurable:true, value:locks});
}`;
