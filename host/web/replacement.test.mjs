import { expect, test } from 'bun:test';
import vm from 'node:vm';
import { declaration, deferred, fixture } from './glue-fixture.mjs';

// Run the public reload paths and their boundary over the host fixture,
// with input delivery and task turns controlled by the test.
function running() {
  const f = fixture(), listeners = new Map();
  f.launchVisits = () => [];
  f.assetNamespace = () => new Map();
  f.setInputReady = () => {};
  f.addEventListener = (type, fn) => listeners.set(type, [...listeners.get(type) ?? [], fn]);
  f.MutationObserver = class { observe() {} disconnect() {} };
  f.document.documentElement.hasAttribute = () => false;
  f.MessageChannel = class {
    port1 = {};
    port2 = { postMessage: () => queueMicrotask(() => this.port1.onmessage()) };
  };
  vm.runInContext(declaration('engagement') + declaration('replacements') + '\nhost.replacements = replacements;', f);
  // An event as the page's tracker reads it: its innermost target hears its ending too (engagement.js).
  const emit = (type, event = {}) => {
    const e = { target: { isConnected: true }, ...event };
    e.target.addEventListener ??= () => {};
    e.composedPath = () => [e.target];
    listeners.get(type)?.forEach(hear => hear(e));
  };
  return { f, emit };
}
const turn = () => new Promise(ok => setImmediate(ok));

test.each(['composition', 'pointer'])('a direct wasm reload waits for %s', async interaction => {
  const { f, emit } = running();
  await f.boot(null);
  const target = { isConnected: true };
  const event = { target, pointerId: 1 };
  emit(interaction === 'composition' ? 'compositionstart' : 'pointerdown', event);
  let done = false;
  const reload = f.exact.reload(new Uint8Array([1])).then(value => { done = true; return value; });
  await new Promise(ok => setImmediate(ok));
  expect(f.incarnation).toBe(1);
  expect(done).toBe(false);
  emit(interaction === 'composition' ? 'compositionend' : 'pointerup', event);
  expect(await reload).not.toBeNull();
  expect(f.incarnation).toBe(2);
});

// The press a replacement waits out may navigate: the location, the visits beneath it and the launch's date and
// place go in as they are when it applies, not as they were when it began to wait.
test('a held wasm reload launches at the location the held press navigated to', async () => {
  const { f, emit } = running();
  await f.boot(null);
  let at = '/';
  const launched = [], dated = [];
  f.launchLocation = () => { launched.push(at); return at; };
  f.launchVisits = () => at === '/' ? [] : ['/'];
  f.wasm.exact_set_launch = () => dated.push(at);
  const staged = deferred(), release = deferred();
  f.loadStage = () => { staged.resolve(); return release.promise; };
  const reload = f.exact.reload(new Uint8Array([1]));
  await staged.promise; // preparing, past the first wait
  emit('pointerdown', { pointerId: 1 });
  release.resolve();
  await turn();
  expect(f.replacements.pending).toBe('pointer');
  expect(f.incarnation).toBe(1);
  at = '/next'; // the press's handler navigates
  emit('pointerup', { pointerId: 1 });
  expect(await reload).not.toBeNull();
  expect(f.incarnation).toBe(2);
  expect(launched).toEqual(['/next']);
  expect(dated).toEqual(['/next']);
});

test('an older wasm preparation cannot displace the newest held generation', async () => {
  const { f, emit } = running();
  await f.boot(null);
  // Expose the second-wait race independently of the missing boot guard.
  f.booted = true;
  const entered = deferred(), release = deferred();
  f.logicInfo = {};
  f.moduleLoader = { prepare: () => { entered.resolve(); return release.promise; } };
  f.wasm.exact_boot_module = () => '{"ops":[]}';
  const module = { receipt: new Uint8Array(), script: new Uint8Array() };
  let current = 1;
  const first = f.exact.reloadGeneration(new Uint8Array([1]), [], () => current === 1, module);
  await entered.promise;
  emit('pointerdown', { pointerId: 1 });
  current = 2;
  let ended = false;
  const second = f.exact.reloadGeneration(new Uint8Array([2]), [], () => current === 2, module).then(out => { ended = true; return out; });
  await new Promise(ok => setImmediate(ok));
  release.resolve({ id: {}, dispose() {} });
  await new Promise(ok => setImmediate(ok));
  expect(ended).toBe(false);
  expect(f.replacements.pending).toBe('pointer');
  emit('pointerup', { pointerId: 1 });
  expect(await first).toBe(false);
  expect(await second).toBe(true);
  expect(f.incarnation).toBe(2);
});

test('a wasm generation rechecks interactions after asynchronous preparation', async () => {
  const { f, emit } = running();
  await f.boot(null);
  const entered = deferred(), release = deferred();
  f.prepareFonts = () => { entered.resolve(); return release.promise; };
  let ended = false;
  const reload = f.exact.reloadGeneration(new Uint8Array([1]), [], () => true).then(out => { ended = true; return out; });
  await entered.promise;
  const target = { isConnected: true };
  emit('compositionstart', { target });
  release.resolve([]);
  await new Promise(ok => setImmediate(ok));
  expect(ended).toBe(false);
  expect(f.incarnation).toBe(1);
  emit('compositionend', { target });
  expect(await reload).toBe(true);
  expect(f.incarnation).toBe(2);
});

test('a wasm generation reports module and request holds until each clears', async () => {
  const { f } = running();
  await f.boot(null);
  const loading = deferred(), loaded = deferred();
  f.logicInfo = {};
  f.moduleLoader = { prepare: () => { loading.resolve(); return loaded.promise; } };
  f.wasm.exact_boot_module = () => '{"ops":[]}';
  f.requestsPending = 1;
  const reload = f.exact.reloadGeneration(new Uint8Array([1]), [], () => true, { receipt: new Uint8Array() });
  await loading.promise;
  expect(f.replacements.pending).toBe('module');
  loaded.resolve({ id: {}, dispose() {} });
  await turn();
  expect(f.replacements.pending).toBe('requests');
  f.requestsPending = 0;
  f.replacements.changed();
  expect(await reload).toBe(true);
  expect(f.replacements.pending).toBeNull();
});

// One wait asks the interaction and the requests together (LLP 1029.000.002 D9): a request the interaction
// started holds the candidate that waited the interaction out, where the runner would refuse it
// (host/web/src/abi.rs `boot_module`).
test('a wasm module replacement keeps its candidate when its interaction starts a request', async () => {
  const { f, emit } = running();
  await f.boot(null);
  const staged = deferred(), release = deferred();
  f.logicInfo = {};
  f.moduleLoader = { prepare: async () => ({ id: {}, dispose() {} }) };
  f.loadStage = () => { staged.resolve(); return release.promise; };
  f.wasm.exact_boot_module = () => f.requestsPending ? '{"error":"module replacement waits for in-flight requests to settle"}' : '{"ops":[]}';
  let ended;
  const reload = f.exact.reloadGeneration(new Uint8Array([1]), [], () => true, { receipt: new Uint8Array() })
    .then(out => { ended = out; }, error => { ended = error; });
  await staged.promise;
  emit('pointerdown', { pointerId: 1 });
  release.resolve();
  await turn();
  expect(f.replacements.pending).toBe('pointer');
  f.requestsPending = 1; // the press sends one
  emit('pointerup', { pointerId: 1 });
  await turn();
  expect(ended).toBeUndefined();
  expect(f.replacements.pending).toBe('requests');
  f.requestsPending = 0;
  f.replacements.changed(); // its reply commits (commit.js `applyBatch`)
  await reload;
  expect(ended).toBe(true);
  expect(f.incarnation).toBe(2);
});

// An app may always have a request out, here a read and a write that never end. An update waits for them with no
// deadline; the dev loop lets them go after a second and names the write (LLP 1007 §6).
test.each(['update', 'dev', 'superseded'])('a wasm module replacement waits for requests in flight (%s)', async kind => {
  const { f } = running();
  await f.boot(null);
  const lines = [], timers = [], letGo = kind === 'dev';
  let flag = null, current = true, ended;
  f.log = line => lines.push(line);
  f.setTimeout = (fn, ms) => timers.push([fn, ms]);
  f.wasm.exact_boot_module = (...args) => { flag = args[3]; return '{"ops":[]}'; };
  const stuck = () => Object.assign(new Promise(() => {}), { stuck: true });
  f.inflight.add(stuck()).add(Object.assign(stuck(), { write: 'POST https://api.test/like' }));
  const candidate = { realm: { id: 1 }, receipt: new Uint8Array([1]), letGo };
  const swap = f.boot(new Uint8Array([1]), null, () => current, candidate).then(out => { ended = out; }, error => { ended = error; });
  await turn();
  expect(ended).toBeUndefined();
  expect(flag).toBeNull();
  expect(f.replacements.pending).toBe('requests');
  expect(timers.map(([, ms]) => ms)).toEqual(letGo ? [1000] : []);
  if (letGo) timers[0][0]();
  else {
    if (kind === 'superseded') current = false;
    f.inflight.clear();
    f.replacements.changed();
  }
  await swap;
  expect(lines.filter(line => line.startsWith('module replacement'))).toEqual(letGo
    ? ['module replacement let go of 2 requests still in flight after 1000 ms; aborted writes the server may or may not have seen: POST https://api.test/like'] : []);
  if (kind === 'superseded') {
    expect(ended).toBeNull();
    expect(flag).toBeNull();
  } else {
    expect(ended).not.toBeNull();
    expect(flag).toBe(letGo ? 1 : 0);
    expect(f.activeModule).toBe(candidate);
  }
  expect(f.replacements.pending).toBeNull();
});

test('a generation that goes stale while it waits leaves no hold behind', async () => {
  const { f, emit } = running();
  await f.boot(null);
  emit('pointerdown', { pointerId: 1 });
  let current = true;
  const reload = f.exact.reloadGeneration(new Uint8Array([1]), [], () => current);
  await new Promise(ok => setImmediate(ok));
  expect(f.replacements.pending).toBe('pointer');
  // A newer generation is on its way, and the pointer lifts before it begins.
  current = false;
  emit('pointerup', { pointerId: 1 });
  expect(await reload).toBe(false);
  emit('compositionstart', { target: { isConnected: true } });
  expect(f.replacements.pending).toBeNull();
  expect(f.incarnation).toBe(1);
});

// One ticket owns the candidate that waits, whichever call brought it (LLP 1029.000.002 D9).
test('the newest wasm candidate wins whichever call brought it', async () => {
  const { f, emit } = running();
  await f.boot(null);
  emit('pointerdown', { pointerId: 1 });
  const reload = f.exact.reload(new Uint8Array([1]));
  await new Promise(ok => setImmediate(ok));
  const generation = f.exact.reloadGeneration(new Uint8Array([2]), [], () => true);
  await new Promise(ok => setImmediate(ok));
  expect(f.incarnation).toBe(1);
  emit('pointerup', { pointerId: 1 });
  expect(await reload).toBeNull();
  expect(await generation).toBe(true);
  expect(f.incarnation).toBe(2);
});
