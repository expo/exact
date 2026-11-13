import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// A stream's caller's signal on the web JS target (ts-stream.js `open`): the stream ends when the signal aborts, as a
// browser's fetch does, whatever the app's own code does to the signal or to `AbortSignal.any`, and lets go of the
// signal when it ends.
import { afterAll, beforeAll, test, expect } from 'bun:test';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import vm from 'node:vm';
import { webCompiler } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
// A folder of the runtime's modules side by side, as a build lays them out, with the app's grants in
// `admission-data.js`; the rest of the runtime is a stub.
function runtime(prefix) {
  const dir = mkdtempSync(resolve(tmpdir(), prefix));
  for (const name of ['ts-stream.js', 'ts-fetch.js', 'ts-body-from.js', 'shape.js', 'uploads.js']) cpSync(resolve(root, 'host/web-js', name), resolve(dir, name));
  for (const name of ['http-body.js', 'grant-admission.js', 'faults.js', 'address-space.js', 'navigation.js', 'storage-environment.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(root, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission.js'), readFileSync(resolve(root, 'host/web-js/admission.js'), 'utf8').replaceAll("'../web/", "'./"));
  writeFileSync(resolve(dir, 'admission-data.js'), "import { createGrantSet } from './admission.js'; export const tsGrantSet = createGrantSet(null);\n");
  writeFileSync(resolve(dir, 'rt.js'), 'export const clock={agent:false,now:0},journal=[],Resources=[],inflight={n:0};export const drive=()=>{},time=()=>{},Origin={now:()=>"app"};export const checkpoint=()=>({kept:null});export const commit=f=>f();export const R=()=>{};export const painted=()=>Promise.resolve();\n');
  writeFileSync(resolve(dir, 'rt-link.js'), "import * as rt from './rt.js'; export { rt };\n");
  writeFileSync(resolve(dir, 'names.js'), 'export const sourceTypes = {};\n');
  return dir;
}
const at = (dir, name) => pathToFileURL(resolve(dir, name)).href;

let dir, open;
beforeAll(async () => {
  dir = runtime('exact-ts-stream-abort-');
  ({ open } = await import(at(dir, 'ts-stream.js')));
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

// A stream of `signal`, with the controller the runtime ends it through
const opened = signal => {
  const stream = new AbortController();
  return { stream, ended: open({ input: 'https://example.test/feed', init: { exactStream: event => event.data, signal } }, v => v, null, () => {}, stream) };
};

test("a stream ends at its caller's abort past an app's listener that stops it", async () => {
  const caller = new AbortController(), reason = new Error('the app stopped it');
  caller.signal.addEventListener('abort', event => event.stopImmediatePropagation());
  const { stream, ended } = opened(caller.signal);
  caller.abort(reason);
  expect(stream.signal.aborted).toBe(true);
  expect(stream.signal.reason).toBe(reason);
  await ended;
});

test("a stream ends at its caller's abort after the app replaces AbortSignal.any", async () => {
  const any = AbortSignal.any;
  // Each signal the replacement composes stops its own abort event before any other listener hears it
  AbortSignal.any = function (signals) {
    const signal = Reflect.apply(any, this, [signals]);
    signal.addEventListener('abort', event => event.stopImmediatePropagation());
    return signal;
  };
  try {
    const caller = new AbortController(), reason = new Error('aborted');
    const { stream, ended } = opened(caller.signal);
    caller.abort(reason);
    expect(stream.signal.reason).toBe(reason);
    await ended;
  } finally { AbortSignal.any = any; }
});

test('a stream whose signal is no AbortSignal answers so, in Hermes\'s words', async () => {
  for (const signal of [17, 0, {}]) expect(await opened(signal).ended).toEqual({ error: 'fetch: init.signal is not an AbortSignal', code: 'error', reason: '' });
});

test("a stream that has ended no longer hears its caller's signal", async () => {
  const caller = new AbortController(), { stream, ended } = opened(caller.signal);
  await ended;
  caller.abort();
  expect(stream.signal.aborted).toBe(false);
});

test("the runtime composes with the page's AbortSignal.any when the app's first import replaces it", async () => {
  const app = runtime('exact-ts-stream-order-');
  writeFileSync(resolve(app, 'ts-data.js'), readFileSync(resolve(root, 'host/web-js/ts-data.js'), 'utf8')
    .replace("'__APP_TS__'", "'./app.js'").replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '').replace("'__FETCH_MODULE__'", "'./ts-fetch.js'"));
  // The app's first import reaches the page's AbortSignal through a signal, which no binding of the runtime's stands
  // in for, and replaces `any` with one whose signals stop their own abort event
  writeFileSync(resolve(app, 'replace-any.js'), `const Signal = new AbortController().signal.constructor, any = Signal.any;
Signal.any = function (signals) { const signal = Reflect.apply(any, this, [signals]); signal.addEventListener('abort', event => event.stopImmediatePropagation()); return signal; };
export const restore = () => { Signal.any = any; };
`);
  writeFileSync(resolve(app, 'app.js'), "import './replace-any.js';\nimport { fetch } from './ts-fetch.js';\nexport const grants = '';\nexport const answer = () => fetch;\n");
  try {
    await import(at(app, 'ts-data.js'));
    const { ownSignal } = await import(at(app, 'ts-fetch.js')), caller = new AbortController();
    let heard = false;
    ownSignal(caller.signal).addEventListener('abort', () => { heard = true; });
    caller.abort();
    expect(heard).toBe(true);
  } finally {
    (await import(at(app, 'replace-any.js'))).restore();
    rmSync(app, { recursive: true, force: true });
  }
});

for (const name of ['rows', 'optionalRows']) {
  test(`${name} preserve frozen row identity across pages`, async () => withRows(({ setRows, answer }) => {
    const first = frozenRow('first'), second = frozenRow('second');
    setRows([first, second]);
    const initial = answer(name);
    setRows([second, first]);
    const reordered = answer(name);
    expect(reordered[0]).toBe(initial[1]);
    expect(reordered[1]).toBe(initial[0]);
    if (name === 'optionalRows') {
      setRows([null, first]);
      const optional = answer(name);
      expect(optional[0]).toBeNull();
      expect(optional[1]).toBe(initial[0]);
    }
  }));
}

test('frozen containers recheck mutable children and getters', async () => withRows(({ setRows, answer }) => {
  const mutable = Object.freeze({id:'mutable', parts:[{text:'before'}]});
  setRows([mutable]);
  const before = answer();
  mutable.parts[0].text = 'after';
  expect(answer()).toEqual([['mutable', [['after']]]]);
  expect(before).toEqual([['mutable', [['before']]]]);
  mutable.parts[0].extra = true;
  expect(answer).toThrow('field `extra` is not in the shape');
  let text = 'before';
  const accessor = Object.freeze({id:'getter', parts:Object.freeze([Object.freeze({get text() { return text; }})])});
  setRows([accessor]);
  expect(answer()).toEqual([['getter', [['before']]]]);
  text = 'after';
  expect(answer()).toEqual([['getter', [['after']]]]);
}));

test('frozen cycles report shape errors and sparse arrays recheck inherited elements', async () => withRows(({ setRows, answer }) => {
  const cyclic = {id:'cycle', parts:Object.freeze([])};
  cyclic.unexpected = cyclic;
  setRows([Object.freeze(cyclic)]);
  expect(answer).toThrow('field `unexpected` is not in the shape');
  const sparse = Object.freeze({id:'sparse', parts:Object.freeze(Array(1))});
  const inherited = Object.getOwnPropertyDescriptor(Array.prototype, '0');
  try {
    Object.defineProperty(Array.prototype, '0', {value:Object.freeze({text:'before'}), configurable:true, writable:true});
    setRows([sparse]);
    expect(answer()).toEqual([['sparse', [['before']]]]);
    Object.defineProperty(Array.prototype, '0', {value:Object.freeze({text:'after'}), configurable:true, writable:true});
    expect(answer()).toEqual([['sparse', [['after']]]]);
  } finally {
    if (inherited) { Object.defineProperty(Array.prototype, '0', inherited); } else { delete Array.prototype[0]; }
  }
}));

test('array validation follows JSON serialization rather than iteration', async () => withRows(({ setRows, answer }) => {
  setRows([frozenRow('first')]);
  answer();
  const serializer = Object.getOwnPropertyDescriptor(Array.prototype, 'toJSON');
  try {
    Object.defineProperty(Array.prototype, 'toJSON', {value() { return [...this, {text:17}]; }, configurable:true});
    expect(answer).toThrow('expected a string, got a number');
  } finally {
    if (serializer) { Object.defineProperty(Array.prototype, 'toJSON', serializer); } else { delete Array.prototype.toJSON; }
  }
  let iterableText = 17;
  const parts = [Object.freeze({text:'indexed'})];
  Object.defineProperty(parts, Symbol.iterator, {value:function* () { yield {text:iterableText}; }});
  setRows([Object.freeze({id:'iterator', parts:Object.freeze(parts)})]);
  expect(answer()).toEqual([['iterator', [['indexed']]]]);
  iterableText = false;
  expect(answer()).toEqual([['iterator', [['indexed']]]]);
}));

test('frozen proxies cannot bypass serialization or revocation checks', async () => withRows(({ setRows, answer }) => {
  let invalid = false;
  const child = new Proxy(Object.freeze({text:'stable'}), {
    get(target, key, receiver) {
      return key === 'toJSON' && invalid ? () => ({text:17}) : Reflect.get(target, key, receiver);
    },
  });
  setRows([Object.freeze({id:'proxy', parts:Object.freeze([child])})]);
  expect(answer()).toEqual([['proxy', [['stable']]]]);
  invalid = true;
  expect(answer).toThrow('expected a string, got a number');
  const revocable = Proxy.revocable(Object.freeze({text:'revocable'}), {});
  setRows([Object.freeze({id:'revocable', parts:Object.freeze([revocable.proxy])})]);
  answer();
  revocable.revoke();
  expect(answer).toThrow();
}));

async function withRows(check) {
  const directory = runtime('exact-ts-rows-');
  writeFileSync(resolve(directory, 'ts-data.js'), readFileSync(resolve(root, 'host/web-js/ts-data.js'), 'utf8')
    .replace("'__APP_TS__'", "'./app.js'").replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '').replace("'__FETCH_MODULE__'", "'./ts-fetch.js'"));
  writeFileSync(resolve(directory, 'names.js'), "const row = {id:'s', parts:['[', {text:'s'}]}; export const sourceTypes = {rows:[[], ['[', row]], optionalRows:[[], ['[', ['?', row]]]}\n");
  writeFileSync(resolve(directory, 'app.js'), "export const appId='test.rows'; let rows=[]; export const setRows=value=>{rows=value;}; export const answer=()=>rows;\n");
  try {
    const { setRows } = await import(at(directory, 'app.js'));
    const { install } = await import(at(directory, 'ts-data.js'));
    const data = { q:[] };
    install(data);
    await check({ setRows, answer:(name = 'rows') => data.answer(name, [], new Map()).v });
  } finally { rmSync(directory, { recursive:true, force:true }); }
}

function frozenRow(id) {
  return Object.freeze({id, parts:Object.freeze([Object.freeze({text:id})])});
}


test("every cache mode reaches the browser from a JS fetch, a stream and a wasm request", async () => {
  const fixture = runtime('exact-fetch-cache-'), browser = globalThis.fetch, pageLocation = globalThis.location, sent = [];
  globalThis.location = new URL('https://example.test/');
  try {
    const { cmd, pre } = webCompiler();
    if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
    const grants = resolve(fixture, 'grants.txt');
    writeFileSync(grants, 'net.fetch https://example.test\nnet.fetch https://another.test');
    const normalized = spawnSync(cmd, [...pre, 'normalize-grants', grants], { cwd: root, encoding: 'utf8' });
    expect(normalized.status).toBe(0);
    writeFileSync(resolve(fixture, 'admission-data.js'), `import { createGrantSet } from './admission.js'; export const tsGrantSet = createGrantSet(${normalized.stdout});`);
    globalThis.fetch = async (input, init) => {
      sent.push(init);
      if (init.cache === 'only-if-cached') {
        expect(init.mode).toBe('same-origin');
        if (new URL(input).origin !== location.origin) throw new TypeError('cross-origin cache-only request');
      }
      return new Response('reply');
    };
    const { tsGrantSet } = await import(at(fixture, 'admission-data.js'));
    const { createRequestExecutor } = await import(at(fixture, 'admission.js'));
    const rustFetch = createRequestExecutor('test.cache', tsGrantSet);
    const { fetch: appFetch } = await import(at(fixture, 'ts-fetch.js'));
    const { open } = await import(at(fixture, 'ts-stream.js'));
    const { request } = await import(at(fixture, 'http-body.js'));
    for (const cache of ['default', 'no-store', 'reload', 'no-cache', 'force-cache', 'only-if-cached']) {
      for (const options of [{ cache }, Object.create({ cache })]) {
        expect(await (await appFetch('https://example.test/cache', options)).text()).toBe('reply');
        expect(sent.at(-1).cache).toBe(cache);
      }
      await open({ input: 'https://example.test/cache', init: { exactStream: e => e.data, cache } }, v => v, tsGrantSet, () => {}, new AbortController());
      expect(sent.at(-1).cache).toBe(cache);
      await request({ method: 'GET', url: 'https://example.test/cache', headers: [], cache }, { grantSet: tsGrantSet, controllers: new Set() });
      expect(sent.at(-1).cache).toBe(cache);
      await rustFetch({ method: 'GET', url: 'https://example.test/cache', headers: [], cache });
      expect(sent.at(-1).cache).toBe(cache);
    }
    expect(sent).toHaveLength(30);
    const { answering } = await import(at(fixture, 'ts-fetch.js'));
    for (const cache of ['force-cache', 'only-if-cached', 'no-store']) {
      const expected = cache === 'no-store' ? cache : 'reload';
      await rustFetch({ method: 'GET', url: 'https://example.test/cache', headers: [], cache, forced: true });
      expect(sent.at(-1).cache).toBe(expected);
      answering.call = { forced: true };
      try { await appFetch('https://example.test/cache', { cache }); }
      finally { answering.call = null; }
      expect(sent.at(-1).cache).toBe(expected);
      const controller = new AbortController(); controller.forced = true;
      await open({ input: 'https://example.test/cache', init: { exactStream: e => e.data, cache } }, v => v, tsGrantSet, () => {}, controller);
      expect(sent.at(-1).cache).toBe(expected);
    }
    for (const cache of [undefined, 'default', 'reload']) {
      const controller = new AbortController();
      globalThis.fetch = async (_input, init) => { sent.push(init); controller.abort(); return new Response(null, { status: 204 }); };
      const poll = { cursor: 'since', from: 'seq', sameCursorEvent: false };
      await request({ method: 'GET', url: 'https://example.test/poll', headers: [], cache, stream: true, longPoll: poll }, { grantSet: tsGrantSet, controllers: new Set(), controller });
      expect(sent.at(-1).cache).toBe(cache ?? 'no-store');
      const next = new AbortController();
      globalThis.fetch = async (_input, init) => { sent.push(init); next.abort(); return new Response(null, { status: 204 }); };
      await open({ input: 'https://example.test/poll', init: { cache, exactStream: e => e.data, exactLongPoll: { cursor: 'since', from: 'seq' } } }, v => v, tsGrantSet, () => {}, next);
      expect(sent.at(-1).cache).toBe(cache ?? 'no-store');
    }
    globalThis.fetch = async (input, init) => { sent.push(init); if (init.cache === 'only-if-cached' && new URL(input).origin !== location.origin) throw new TypeError('cross-origin cache-only request'); return new Response('reply'); };
    writeFileSync(resolve(fixture, 'ts-data.js'), readFileSync(resolve(root, 'host/web-js/ts-data.js'), 'utf8')
      .replace("'__APP_TS__'", "'./app.js'").replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '').replace("'__FETCH_MODULE__'", "'./ts-fetch.js'"));
    writeFileSync(resolve(fixture, 'names.js'), "export const sourceTypes = {cached:[['s'],'s']};\n");
    writeFileSync(resolve(fixture, 'app.js'), "import {fetch} from './ts-fetch.js'; export const appId='test.forced'; export const answer=(_name,[cache])=>fetch('/assets/cache.txt',{cache}).then(r=>r.text());\n");
    const { install } = await import(at(fixture, 'ts-data.js'));
    const sourceData = { q: [] }; install(sourceData);
    for (const cache of ['force-cache', 'no-store']) {
      const answer = sourceData.answer('cached', [cache], new Map(), 'resource', false, true);
      expect(await answer.promise).toBe('reply');
      expect(sent.at(-1).cache).toBe(cache === 'no-store' ? 'no-store' : 'reload');
    }
    const mode = { method: 'GET', url: 'https://example.test/cache', headers: [], mode: 'same-origin' };
    await request(mode, { grantSet: tsGrantSet, controllers: new Set() });
    expect(sent.at(-1).mode).toBe('same-origin');
    const crossSet = tsGrantSet;
    const cross = await request({ ...mode, url: 'https://another.test/cache', cache: 'only-if-cached' }, { grantSet: crossSet, controllers: new Set() });
    expect(cross.kind).toBe(1);
    expect(new TextDecoder().decode(cross.body)).toContain('cross-origin');
  } finally {
    globalThis.fetch = browser;
    globalThis.location = pageLocation;
    rmSync(fixture, { recursive: true, force: true });
  }
});


test("the native cache option encoder distinguishes unsaid from explicit default", () => {
  const realm = vm.createContext({});
  vm.runInContext(readFileSync(resolve(root, 'js/src/binary.js'), 'utf8'), realm);
  for (const init of ['{}', '{cache: undefined}'])
    expect(vm.runInContext(`JSON.stringify({cache: __exact_binary.cacheMode(${init})})`, realm)).toBe('{}');
  expect(vm.runInContext('JSON.stringify({cache: __exact_binary.cacheMode({cache: "default"})})', realm)).toBe('{"cache":"default"}');
  expect(vm.runInContext('JSON.stringify({cache: __exact_binary.cacheMode({cache: "no-store"})})', realm)).toBe('{"cache":"no-store"}');
});


test("a kept upload's request distinguishes cache modes", async () => {
  const { requestKey } = await import('./uploads.js');
  const key = cache => requestKey({ input: 'https://example.test/upload', init: { method: 'POST', exactUploadProgress: true, cache } });
  expect(key(undefined)).toBe(key('default'));
  for (const mode of ['no-store', 'reload', 'no-cache', 'force-cache', 'only-if-cached'])
    expect(key(mode)).not.toBe(key('default'));
});

test('a JS upload with effective only-if-cached fails before sending; a forced upload still sends without cache headers', async () => {
  const fixture = runtime('exact-upload-cache-'), saved = globalThis.XMLHttpRequest, made = [];
  globalThis.XMLHttpRequest = class {
    constructor() { this.upload = {}; this.headers = []; made.push(this); }
    open() {}
    setRequestHeader(...header) { this.headers.push(header); }
    send() { this.status = 201; this.response = new ArrayBuffer(0); this.onload(); }
    getAllResponseHeaders() { return ''; }
  };
  try {
    const { cmd, pre } = webCompiler();
    const grants = resolve(fixture, 'grants.txt');
    writeFileSync(grants, 'net.fetch https://example.test');
    const normalized = spawnSync(cmd, [...pre, 'normalize-grants', grants], { cwd: root, encoding: 'utf8' });
    expect(normalized.status).toBe(0);
    const { createGrantSet } = await import(at(fixture, 'admission.js'));
    const set = createGrantSet(JSON.parse(normalized.stdout));
    const { open } = await import(at(fixture, 'ts-stream.js'));
    const { streamed } = await import(at(fixture, 'http-body.js'));
    for (const forced of [false, true]) {
      const controller = new AbortController(); controller.forced = forced;
      const req = { method: 'POST', url: 'https://example.test/upload', headers: [], stream: true, upload: true, cache: 'only-if-cached', forced, origin: 'app' };
      const messages = [], outcome = await streamed(req, set, m => messages.push(m), controller);
      expect(outcome.kind).toBe(forced ? 0 : 1);
      expect(messages.some(m => m.event === 'exact:sending')).toBe(forced);
      const events = [];
      const ended = await open({ input: req.url, init: { method: 'POST', body: 'bytes', cache: req.cache, exactUploadProgress: true, exactStream: e => (events.push(e), e.type) }, origin: 'app' }, v => v, set, () => {}, controller);
      expect(events.at(-1).type).toBe(forced ? 'load' : 'error');
      expect(ended.v).toBe(forced ? 'load' : 'error');
    }
    expect(made).toHaveLength(2);
    for (const xhr of made) expect(xhr.headers.some(([name]) => name.toLowerCase() === 'cache-control')).toBe(false);
  } finally {
    globalThis.XMLHttpRequest = saved;
    rmSync(fixture, { recursive: true, force: true });
  }
});

for (const protocols of [false, true]) test(`private-field option getters keep their receiver ${protocols ? 'with' : 'without'} protocols`, async () => {
  const { fetch: appFetch, answering } = await import(at(dir, 'ts-fetch.js'));
  const caller = new AbortController();
  class Options {
    #mapper = event => event.data;
    #signal = caller.signal;
    #redirect = 'manual';
    exactProtocols = ['events.v1'];
    get exactStream() { return this.#mapper; }
    get signal() { return this.#signal; }
    get redirect() { return this.#redirect; }
  }
  const options = new Options();
  if (!protocols) delete options.exactProtocols;
  answering.call = { turn: true };
  try {
    void appFetch('wss://example.test/events', options).catch(() => {});
    const registered = answering.call.stream;
    expect(registered).toBeDefined();
    expect(registered.init).toBe(options);
    expect(registered.init.exactStream).toBe(options.exactStream);
    expect(registered.init.signal).toBe(caller.signal);
    expect(registered.redirect).toBe('manual');
    const stream = new AbortController();
    const ended = open(registered, v => v, null, () => {}, stream);
    const reason = new Error('cancelled'); caller.abort(reason);
    expect(stream.signal.reason).toBe(reason);
    await ended;
  } finally { answering.call = null; }
});

for (const inherited of [true, false]) test(`protocols preserve ${inherited ? 'inherited' : 'non-enumerable'} stream options and cancellation`, async () => {
  const { fetch: appFetch, answering } = await import(at(dir, 'ts-fetch.js'));
  const caller = new AbortController(), mapper = e => e.data;
  const members = { exactStream: mapper, signal: caller.signal, redirect: 'manual' };
  const options = inherited ? Object.create(members) : Object.defineProperties({}, Object.fromEntries(Object.entries(members).map(([key, value]) => [key, { value }])));
  const protocols = ['events.v1']; options.exactProtocols = protocols;
  answering.call = { turn: true };
  try {
    void appFetch('ws://example.test/events', options).catch(() => {});
    const registered = answering.call.stream;
    expect(registered).toBeDefined();
    expect(registered.init.exactStream).toBe(mapper);
    expect(registered.init.signal).toBe(caller.signal);
    expect(registered.redirect).toBe('manual');
    protocols.push('later');
    expect(registered.protocols).toEqual(['events.v1']);
    const stream = new AbortController();
    const ended = open(registered, v => v, null, () => {}, stream);
    const reason = new Error('cancelled'); caller.abort(reason);
    expect(stream.signal.reason).toBe(reason);
    await ended;
  } finally { answering.call = null; }
});
