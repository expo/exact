import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// The web Rust adapter against a stand-in for its wasm module (LLP 1146 §2.3).
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { webCompiler } from './module.mjs';
import { createGrantSet, createRequestExecutor } from './admission.js';

const ROOT = resolve(import.meta.dir, '../..');

// `spec` as the web's grant set, by the grammar's own normalizer (`exact-web-js normalize-grants`), the compiler the
// lane built.
function normalized(spec) {
  const { cmd, pre } = webCompiler();
  if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-grants-')), input = resolve(scratch, 'grants.txt');
  writeFileSync(input, spec);
  const run = spawnSync(cmd, [...pre, 'normalize-grants', input], { cwd: ROOT, encoding: 'utf8' });
  rmSync(scratch, { recursive: true });
  if (run.status !== 0) throw new Error(run.stderr || 'the grant normalizer failed');
  return JSON.parse(run.stdout);
}

test('a render refuses a Rust storage request after validating its scope', async () => {
  const run = createRequestExecutor('test.rust', createGrantSet(normalized('fs.read app:/data')));
  const storage = JSON.stringify({ version: 1, op: 'fs.readFile', args: { path: 'app:/data/prefs' } });
  const hadRender = Object.hasOwn(globalThis, '__exactRender'), before = globalThis.__exactRender;
  globalThis.__exactRender = true;
  try {
    expect(await run({ storage, scope: 'fs.write app:/data' })).toEqual({ failed: 2, message: "source scope exceeds the app's admitted grants" });
    expect(await run({ storage })).toEqual({ refused: 'storage request' });
    expect(await run({ storage, scope: '' })).toEqual({ refused: 'storage request' });
  } finally {
    if (hadRender) globalThis.__exactRender = before; else delete globalThis.__exactRender;
  }
});

// The web Rust adapter carries a store read its call made to rt.js (LLP 1146 §2.3): on the answer when the call
// answers, and through `data.threwRead` when it fails, an answer's or a parse's, so the resource is a store reader
// either way, as natively.
test("a Rust source's store read reaches rt.js whether its call answers or fails", async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-rust-readers-'));
  const set = normalized('secret.keep k');
  for (const name of ['rust-data.js', 'admission.js']) {
    const source = readFileSync(resolve(ROOT, 'host/web-js', name), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'");
    writeFileSync(resolve(dir, name), source);
  }
  for (const name of ['http-body.js', 'grant-admission.js', 'faults.js', 'navigation.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const rustGrantSet=createGrantSet(${JSON.stringify(set)}),tsGrantSet=createGrantSet(${JSON.stringify(normalized(''))});\n`);
  writeFileSync(resolve(dir, 'rt-link.js'), 'export const rt = {};\n');
  const memory = new WebAssembly.Memory({ initial: 1 }), out = 32768;
  let output = new Uint8Array(), read = false, fails = false, unknown = false, socket = false, writes = false, cache, preload, prefetch = false, abi = 9;
  const scopes = [];
  const response = fill => {
    const bytes = [], u8 = value => bytes.push(value), u32 = value => bytes.push(value & 255, value >>> 8 & 255, value >>> 16 & 255, value >>> 24 & 255);
    const string = value => { const encoded = new TextEncoder().encode(value); u32(encoded.length); bytes.push(...encoded); };
    u32(9); fill({ u8, u32, string }); output = Uint8Array.from(bytes); new Uint8Array(memory.buffer, out, output.length).set(output);
  };
  const exports = {
    memory, exact_logic_abi: () => abi, exact_logic_create: () => 1, exact_logic_alloc: () => 0, exact_logic_dealloc() {},
    exact_logic_output: () => out, exact_logic_output_len: () => output.length,
    exact_logic_call(_session, pointer) {
      const code = new Uint8Array(memory.buffer)[pointer + 4];
      if (code === 0) response(w => { w.u8(0); w.string('test.rust'); w.string('secret.keep k'); });
      else if (code === 1 || code === 2) response(w => { w.u8(0); w.u8(0); w.u8(3); });
      // A call's reply: whether it read the store, writes, no socket operations, then a source it lacks (tag 2), a
      // failure (tag 3) or a request (tag 1).
      else if (code === 3 || code === 4) {
        const view = new DataView(memory.buffer); let at = pointer + 5;
        const u32 = () => { const n = view.getUint32(at, true); at += 4; return n; };
        const string = () => { const n = u32(), s = new TextDecoder().decode(new Uint8Array(memory.buffer, at, n)); at += n; return s; };
        string(); at += 5; // source and empty arguments
        const names = Array.from({ length: u32() }, string), pairs = Array.from({ length: u32() }, () => [string(), string()]);
        scopes.push({ names, pairs });
        response(w => {
          w.u8(2); w.u8(read ? 1 : 0); w.u32(writes ? 1 : 0);
          if (writes) { w.string('k'); w.u8(1); w.string('private'); }
          w.u32(0); w.u8(0); w.string('');
          if (prefetch) w.u8(14);
          if (preload !== undefined) { w.u8(13); w.string(preload); }
          if (socket) { if (cache !== undefined) { w.u8(12); w.string(cache); } w.u8(11); w.u32(1 << 20); w.u8(0); w.string(''); w.string('GET'); w.string('wss://api.example/book'); w.u32(0); w.u32(0); w.u32(0); } else if (unknown) { w.u8(2); w.string('reader'); } else if (fails) { w.u8(3); w.string('nothing kept'); } else { if (cache !== undefined) { w.u8(12); w.string(cache); } w.u8(1); w.u8(0); w.string(''); w.string('GET'); w.string('https://api.example/me'); w.u32(0); w.u32(0); }
        });
      }
      else throw new Error(`unexpected logic call ${code}`);
      return 0;
    },
  };
  const instantiate = WebAssembly.instantiate;
  const render = Object.getOwnPropertyDescriptor(globalThis, '__exactRender');
  WebAssembly.instantiate = async () => ({ instance: { exports } });
  try {
    const module = await import(`${pathToFileURL(resolve(dir, 'rust-data.js')).href}?readers=${Date.now()}`);
    const kept = [], data = { q: [] }, store = { map: new Map(), set(...write) { kept.push(write); } }, failed = { failed: 3, message: 'refused' };
    abi = 8;
    await expect(module.install(data, { reader: '' }, async () => new ArrayBuffer(0))).rejects.toThrow('Rust module ABI 8 is not 9');
    abi = 9;
    await module.install(data, { reader: '' }, async () => new ArrayBuffer(0));
    for (const mode of ['default', 'no-store', 'reload', 'no-cache', 'force-cache', 'only-if-cached']) {
      cache = mode;
      expect(data.answer('reader', [], store).req.cache).toBe(mode);
    }
    for (const mode of [undefined, 'force-cache']) {
      preload = 'feed-2'; cache = mode;
      const req = data.answer('reader', [], store).req;
      expect([req.url, req.cache, req.preload]).toEqual(['https://api.example/me', mode, undefined]);
    }
    prefetch = true;
    for (const mode of [undefined, 'force-cache']) {
      for (const name of [undefined, 'feed-2']) {
        preload = name; cache = mode;
        const req = data.answer('reader', [], store).req;
        expect([req.url, req.cache, req.prefetch]).toEqual(['https://api.example/me', mode, true]);
      }
    }
    prefetch = false;
    preload = 'Bad';
    expect(() => data.answer('reader', [], store)).toThrow('invalid preload HTTP result');
    preload = undefined;
    cache = undefined;
    [read, fails] = [true, false];
    expect(data.answer('reader', [], store).store).toBe(true);
    expect(data.parse('reader', [], failed, store).store).toBe(true);
    expect(data.threwRead()).toBe(false);
    [read, fails] = [true, true];
    expect(() => data.answer('reader', [], store)).toThrow('nothing kept');
    expect([data.threwRead(), data.threwRead()]).toEqual([true, false]);
    expect(() => data.parse('reader', [], failed, store)).toThrow('nothing kept');
    expect(data.threwRead()).toBe(true);
    // A Rust source's two-way socket (LLP 1016.003) is a stream that says so (rust-socket.test.mjs).
    [read, fails, socket] = [false, false, true];
    expect(data.answer('reader', [], store).req).toMatchObject({ stream: true, socket: true, url: 'wss://api.example/book' });
    cache = 'reload';
    expect(() => data.answer('reader', [], store)).toThrow('invalid cached HTTP result');
    cache = undefined; socket = false;
    [read, fails] = [false, true];
    expect(() => data.answer('reader', [], store)).toThrow('nothing kept');
    expect(data.threwRead()).toBe(false);
    // A mixed app: the Rust module reads and names a source it lacks, and the TypeScript one, asked in its place,
    // reads and throws. One failure: its read is told once, and nothing of it is left for the next.
    let tsRead = false, tsForced = false;
    const mixed = { q: [], ts: (_source, _args, _store, _target, _mutation, forced) => { tsForced = forced; tsRead = true; throw new Error('the TypeScript source threw'); }, threwRead: () => [tsRead, tsRead = false][0] };
    await module.install(mixed, { reader: '' }, async () => new ArrayBuffer(0));
    [read, fails, unknown] = [true, false, true];
    expect(() => mixed.answer('reader', [], store, 'resource', false, true)).toThrow('the TypeScript source threw');
    expect(tsForced).toBe(true);
    expect([mixed.threwRead(), mixed.threwRead()]).toEqual([true, false]);
    [read, fails, unknown] = [false, true, false];
    expect(() => mixed.answer('reader', [], store)).toThrow('nothing kept');
    expect(mixed.threwRead()).toBe(false);
    // Even a module that returns writes cannot replace its result with a render store error.
    globalThis.__exactRender = true;
    store.map.set('k', 'hidden'); writes = true;
    for (const failsNow of [false, true]) {
      fails = failsNow;
      for (const call of [() => data.answer('reader', [], store), () => data.parse('reader', [], failed, store)]) {
        if (fails) expect(call).toThrow('nothing kept'); else expect(call().req.method).toBe('GET');
        expect(scopes.at(-1)).toEqual({ names: [], pairs: [] });
      }
    }
    expect(kept).toEqual([]);
    expect([...store.map]).toEqual([['k', 'hidden']]);
    globalThis.__exactRender = false; fails = false;
    data.answer('reader', [], store);
    expect(scopes.at(-1)).toEqual({ names: ['k'], pairs: [['k', 'hidden']] });
    expect(kept).toEqual([['k', 'private']]);
  } finally {
    if (render) Object.defineProperty(globalThis, '__exactRender', render); else delete globalThis.__exactRender;
    WebAssembly.instantiate = instantiate;
    rmSync(dir, { recursive: true, force: true });
  }
});


test('a forced Rust resource keeps forcing parser-created requests and tickets after a stream message', async () => {
  const { stageHostGlue, bundleHostModule, localImports } = await import('./glue.mjs');
  const { createDocument } = await import('./dom.js');
  const vm = await import('node:vm');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-forced-chain-'));
  try {
    for (const name of readdirSync(resolve(ROOT, 'host/web-js')).filter(n => n.endsWith('.js')))
      cpSync(resolve(ROOT, 'host/web-js', name), resolve(dir, basename(name)));
    stageHostGlue(ROOT, dir);
    for (const name of ['navigation.js', 'grant-admission.js'])
      writeFileSync(resolve(dir, name), await bundleHostModule(ROOT, name));
    writeFileSync(resolve(dir, 'admission.js'), localImports(readFileSync(resolve(dir, 'admission.js'), 'utf8')));
    writeFileSync(resolve(dir, 'names.js'), 'export const sourceTypes = {};');
    writeFileSync(resolve(dir, 'draw.js'), 'export const drawer = null;');
    writeFileSync(resolve(dir, 'path2d.js'), 'export const browserPath2D = undefined;');
    writeFileSync(resolve(dir, 'entry.js'), "import * as rt from './rt.js'; globalThis.rt = rt;");
    const built = await Bun.build({ entrypoints: [resolve(dir, 'entry.js')], format: 'iife', target: 'browser', define: { 'import.meta.url': '"https://example.test/app.js"' } });
    expect(built.success, String(built.logs)).toBe(true);
    const code = await built.outputs[0].text();
    for (const stream of [false, true]) {
      const ctx = vm.createContext({
        document: createDocument(), location: new URL('https://example.test/'), navigator: {},
        __exactRender: true, console, performance, setTimeout, clearTimeout, queueMicrotask,
        addEventListener() {}, removeEventListener() {}, requestAnimationFrame: () => 0,
        getComputedStyle: () => ({ fontSize: '16px' }),
        TextEncoder, TextDecoder, URL, URLSearchParams, AbortController,
        stream,
      });
      ctx.globalThis = ctx; ctx.self = ctx;
      vm.runInContext(code, ctx);
      const result = await vm.runInContext(`(async () => {
        const sent = [];
        let value, finish;
        const done = new Promise(resolve => { finish = resolve; });
        rt.data.answer = () => ({ req: { method: 'GET', url: '/first', cache: 'force-cache', stream } });
        rt.data.parse = (_source, _args, o) => o.streamed || o.status === 200
          ? { req: { method: 'GET', url: '/next', cache: 'force-cache' } }
          : { v: 'done' };
        rt.data.fetch = (req, message) => {
          sent.push({ url: req.url, forced: req.forced });
          if (req.stream) { message({ data: 'next' }); return new Promise(() => {}); }
          if (req.url === '/next') queueMicrotask(finish);
          return Promise.resolve({ status: req.url === '/first' ? 200 : 201 });
        };
        rt.mount(() => {
          value = rt.res('value', 'rust', () => [], 'initial', [], 's', '', true);
          rt.effect(() => value());
        });
        rt.clock.agent = true;
        rt.commit(() => rt.R(value));
        await done;
        return JSON.stringify(sent);
      })()`, ctx);
      expect(JSON.parse(result)).toEqual([{ url: '/first', forced: true }, { url: '/next', forced: true }]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('web dispatch refuses a Rust prefetch on a send and sends a resource prefetch normally', async () => {
  const { sendRequest } = await import('./request-send.js');
  for (const resource of [false, true]) {
    const out = [], inflight = { n: 0 }, requests = [];
    let land;
    const landed = new Promise(resolve => { land = resolve; });
    const ticket = { id: 1, req: { method: 'GET', url: 'https://api.example/me', prefetch: true }, r: resource ? {} : undefined, held: () => true };
    const data = { fetch: async req => { requests.push(req); return { status: 200, headers: [], body: new Uint8Array() }; } };
    sendRequest(ticket, land, out, data, inflight, () => {});
    out[0]();
    const outcome = await landed;
    expect(requests.length).toBe(resource ? 1 : 0);
    expect(resource ? outcome.status : outcome.failed).toBe(resource ? 200 : 2);
    expect(inflight.n).toBe(0);
  }
});
