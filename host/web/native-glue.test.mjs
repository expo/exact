import { glueDeclarations } from './tests/glue-fixture.mjs';
import { hostFixtureFile as readFileSync } from './tests/glue-fixture.mjs';
import { test, expect } from 'bun:test';

import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('./native-glue.js', import.meta.url), 'utf8');
// A vm context has no module loader: the glue's one `import()` is the
// fixture's, which records each URL and answers when the test says.
const IMPORT = 'import(url.href)';
if (source.split(IMPORT).length !== 2) throw new Error(`native-glue.js no longer loads its artifact with ${IMPORT}`);
function host(table, browser = {}) {
  const imports = [], timers = new Map();
  const exact = {}, context = {
    exact, URL, TextDecoder, TextEncoder, Uint8Array, atob, console, queueMicrotask, performance,
    document: { baseURI: 'https://example.test/' }, ...browser,
    importArtifact: url => new Promise((resolve, reject) => imports.push({ url, resolve, reject })),
    setTimeout: callback => { const id = timers.size + 1; timers.set(id, () => { timers.delete(id); callback(); }); return id; }, clearTimeout: id => timers.delete(id),
  };
  if (table) exact.nativeArtifact = Promise.resolve(table);
  runInNewContext(source.replace(IMPORT, 'importArtifact(url.href)'), context);
  return { exact, imports, timers };
}

test('an announcement-only page module connects without a native.later export', async () => {
  const heard = [];
  const h = host({ connect({ changed }) { changed('meter'); } });
  const page = await h.exact.pageNative(true, { changed: topic => heard.push(topic), ready: () => true, generation: () => 1 });
  page.drain();
  expect(heard).toEqual(['meter']);
  await expect(page.later(btoa('{}'))).rejects.toThrow('exports no later');
});

test('connect receives the agent flag and the page clock (LLP 1067.000 Q7)', async () => {
  let seen = null;
  const h = host({ connect(context) { seen = context; } });
  await h.exact.pageNative(true, { changed() {}, ready: () => true, generation: () => 1, agent: true, now: () => 1234 });
  expect(seen.agent).toBe(true);
  expect(seen.now()).toBe(1234);
  const plain = host({ connect(context) { seen = context; } });
  await plain.exact.pageNative(true, { changed() {}, ready: () => true, generation: () => 1 });
  expect(seen.agent).toBe(false);
  expect(typeof seen.now()).toBe('number');
});

test('a rejected or timed-out artifact loads again from a fresh URL, and a late answer settles nothing', async () => {
  for (const failure of ['error', 'timeout']) {
    const h = host();
    const first = h.exact.pageNative(true, { changed() {}, ready: () => true, generation: () => 1 });
    expect(h.imports.map(i => i.url)).toEqual(['https://example.test/modules/index.js']);
    if (failure === 'error') h.imports[0].reject(new TypeError('Failed to fetch dynamically imported module'));
    else [...h.timers.values()][0]();
    await expect(first).rejects.toThrow(failure === 'error' ? 'did not load: Failed to fetch' : 'did not load in 10 s');
    expect(h.exact.nativeArtifact).toBeNull();
    expect(h.timers.size).toBe(0);
    const second = h.exact.pageNative(true, { changed() {}, ready: () => true, generation: () => 1 });
    expect(h.imports[1].url).toBe('https://example.test/modules/index.js?exact-native-retry=1');
    h.imports[0].resolve({ later: () => ({ stale: true }) });
    h.imports[1].resolve({ later: () => ({ ok: true }) });
    expect(await (await second).later(btoa('{}'))).toBe('{"ok":true}');
    expect(h.timers.size).toBe(0);
  }
});

test('announcements coalesce until the host drains and retired generations cannot re-ask', async () => {
  let announce, ready = false, generation = 1;
  const heard = [];
  const h = host({ connect({ changed }) { announce = changed; } });
  const page = await h.exact.pageNative(true, { changed: topic => heard.push(topic), ready: () => ready, generation: () => generation });
  announce('meter'); announce('meter'); announce('connectivity');
  expect(heard).toEqual([]);
  expect(h.timers.size).toBe(1);
  page.drain();
  expect(heard).toEqual([]);
  ready = true; page.drain();
  expect(heard).toEqual(['meter', 'connectivity']);
  announce('retired'); generation++;
  announce('current'); page.drain();
  expect(heard).toEqual(['meter', 'connectivity', 'current']);
});

test('the page loader clears its rejected cache and activation connects without a request', async () => {
  const glue = readFileSync(new URL('./glue.js', import.meta.url), 'utf8');
  const loaderSource = glueDeclarations(glue, ['loadPageNative']);
  let attempts = 0;
  const native = { drain() {} };
  const load = Function('loadAfterPaint', 'pageNative', 'inputReady', 'incarnation', 'afterNativePaint', 'agentMode', 'now', `const host = { pageNativeModule: null, inputReady, incarnation }; ${loaderSource}; return loadPageNative;`)(
    async () => { if (++attempts === 1) throw Error('transient load failure'); return async () => native; }, true, true, 1, async () => {}, false, () => 0);
  await expect(load()).rejects.toThrow('transient load failure');
  expect(await load()).toBe(native);
  expect(await load()).toBe(native);
  expect(attempts).toBe(2);
  const activationSource = glueDeclarations(glue, ['activateData']);
  const calls = [];
  const activate = Function('wasm', 'readOut', 'page', 'applyBatch', 'setInputReady', 'collections', 'root', 'pageNative', 'loadPageNative', 'pieces', 'log',
    `const host = { wasm, page }; ${activationSource}; return activateData;`)(
      { exact_data_ready: () => 0 }, () => '{}', null, () => {}, () => calls.push('ready'), { dataReady() {} }, { dataset: {} }, true,
      async () => { calls.push('connect'); return { drain: () => calls.push('drain') }; }, { pending: () => null }, () => {});
  await activate();
  expect(calls).toEqual(['ready', 'connect', 'drain']);
});

test('the wasm host identifies a package view before loading the app artifact', async () => {
  const glue = readFileSync(new URL('./wasm/views.js', import.meta.url), 'utf8');
  const state = { packageViewOwners: { 'swatch-box': '@fixture/swatch' } };
  const attached = [];
  const native = { attach: element => attached.push(element.localName) };
  const create = Function('host', 'pageNative', 'HAS_PACKAGE_VIEWS', 'log', 'afterNativePaint', 'loadAfterPaint',
    glueDeclarations(glue, ['nativeCreate']) + '; return nativeCreate;')(
      state, false, false, () => {}, async () => {}, async () => () => native);
  const swatch = { localName: 'swatch-box' };
  create(swatch, 1);
  expect(swatch.exactNative.status()).toEqual({
    name: 'swatch-box', state: 'error',
    error: 'the web wasm target cannot load the view factory for <swatch-box> from @fixture/swatch',
  });
  expect(state.nativeHost).toBeUndefined();
  create({ localName: 'app-box' }, 2);
  await state.nativeHost;
  expect(attached).toEqual(['app-box']);
});

test('a wasm game can still supply a package tag in the app artifact', async () => {
  const h = host({
    abi: 1, roster: { 'swatch-box': {} }, create: tag => tag, setProps() {}, destroy() {},
  }, { customElements: { get: () => true }, MutationObserver: class { observe() {} } });
  const state = { packageViewOwners: { 'swatch-box': '@fixture/swatch', 'words-panel': '@fixture/words' } };
  const glue = readFileSync(new URL('./wasm/views.js', import.meta.url), 'utf8');
  const create = Function('host', 'pageNative', 'HAS_PACKAGE_VIEWS', 'log', 'afterNativePaint', 'loadAfterPaint',
    glueDeclarations(glue, ['nativeCreate']) + '; return nativeCreate;')(
      state, true, false, () => {}, async () => {}, async () => h.exact.nativeHost);
  const place = (localName, id) => {
    const el = { localName, getAttribute: () => null };
    create(el, id);
    return el.exactNative;
  };
  const provided = place('swatch-box', 1), absent = place('words-panel', 2), app = place('app-absent', 3);
  await (await state.nativeHost).loaded;
  expect(provided.handle).toBe('swatch-box');
  expect(provided.state).toBe('ready');
  expect(absent.error).toBe('the web wasm target cannot load the view factory for <words-panel> from @fixture/words');
  expect(app.error).toBe('the module artifact has no factory for app-absent');
});

test('a wasm package view loads the session’s factory table after paint without an app artifact', async () => {
  const state = { packageViewOwners: { 'swatch-box': '@fixture/swatch' } };
  const loaded = [], attached = [];
  const create = Function('host', 'pageNative', 'HAS_PACKAGE_VIEWS', 'PACKAGE_VIEW_OWNERS', 'agentMode', 'now', 'afterNativePaint', 'loadAfterPaint', 'log',
    glueDeclarations(readFileSync(new URL('./wasm/views.js', import.meta.url), 'utf8'), ['nativeCreate']) + '; return nativeCreate;')(
    state, false, true, state.packageViewOwners, false, () => 0, async () => loaded.push('paint'), async file => {
      loaded.push(file);
      return () => ({ attach: el => attached.push(el.localName) });
    }, () => {});
  create({ localName: 'swatch-box' }, 7);
  await state.packageNative;
  expect(loaded).toEqual(['paint', './packages/views.js']);
  expect(attached).toEqual(['swatch-box']);
});

test('a loading package view catches up with props and retires a late handle', async () => {
  const h = host(null, { customElements: { get: () => true }, MutationObserver: class { observe() {} disconnect() {} } });
  const waiting = [], updates = [], destroyed = [], logs = [];
  const table = h.exact.nativeHost({ log: line => logs.push(line), packages: {
    serves: () => true,
    make: (_, { view }) => new Promise(resolve => waiting.push(() => resolve({
      setProps: p => updates.push(p.caption), destroy: () => destroyed.push(view),
    }))),
  } });
  let props = '{"caption":"old"}';
  const nodes = [1, 2].map(id => ({ exactNative: { name: 'swatch-box', id }, getAttribute: () => props }));
  const pending = nodes.map(node => table.attach(node));
  await Promise.resolve();
  props = '{"caption":"new"}';
  nodes[1].exactNative.destroyed = true;
  table.destroy(nodes[1]);
  for (const release of waiting) release();
  await Promise.all(pending);
  expect(updates).toEqual(['new']);
  expect(destroyed).toEqual([2]);
  expect(logs.some(line => line.includes('destroy threw'))).toBe(false);
});
