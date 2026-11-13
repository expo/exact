import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// Store readers at Rust-module startup and across the adapter's calls (LLP 1027 D4, LLP 1146 §2.3).
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { webCompiler } from './module.mjs';
import { createGrantSet, createRequestExecutor } from './admission.js';
import { appDigest, suiteBrowser } from '../../scripts/agent-launch.mjs';

const ROOT = resolve(import.meta.dir, '../..');

test('a store reader shows its declared placeholder until its Rust source is ready when launch secrets were present', async () => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    const local = resolve(ROOT, '.' + path);
    const file = existsSync(local) ? local : resolve(ROOT, 'host/web', basename(path));
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
  const browser = await suiteBrowser();
  try {
    for (const mode of ['secret', 'empty', 'ungranted', 'removed', 'added', 'ready']) {
      const page = await browser.newPage();
      try {
        await page.goto(server.url.href + '?agent');
        const before = await page.evaluate(async mode => {
          const rt = await import('/host/web-js/rt.js');
          const secret = ['secret', 'removed', 'ready'].includes(mode);
          if (secret) localStorage.setItem(rt.Kept('secret') + 'token', 'private');
          if (mode === 'ungranted') localStorage.setItem(rt.Kept('secret') + 'other', 'private');
          localStorage.setItem(rt.Kept('state') + 'state', 'state');
          localStorage.setItem('exact.kept.old', 'old answer');
          const { install, reader } = await import('/host/web-js/store-reader.js');
          install(['token']);
          const checkpoint = document.createElement('script');
          checkpoint.type = 'application/vnd.exact.checkpoint';
          checkpoint.textContent = JSON.stringify({ answers: [
            ['kept', 'rust', [], 'kept answer'], ['invalid', 'rust', [], 42],
          ], readers: [0, 1], open: [0] });
          document.head.append(checkpoint);
          const values = {}, read = () => Object.fromEntries(Object.entries(values).map(([k, v]) => [k, {
            value: v(), pending: v.p(), reader: v.r.store,
          }]));
          const fresh = () => ({ v: 'fresh answer', store: true });
          if (mode === 'ready') rt.data.answer = fresh;
          rt.mount(root => {
            if (mode === 'removed') rt.Store.set('token', null);
            if (mode === 'added') rt.Store.set('token', 'late secret');
            const preview = rt.res('preview', 'preview', () => [], 'preview', [], 's', undefined, true);
            for (const name of ['literal', 'source', 'bare', 'ordinary', 'kept', 'invalid', 'unavailable']) {
              const res = name === 'ordinary' ? rt.res : reader(rt.res, name !== 'bare');
              values[name] = res(name, 'rust', () => [], 'baked empty store', name === 'ordinary' ? [] : undefined,
                's', name === 'source' ? () => preview() : name === 'unavailable' ? () => undefined : 'waiting');
              const node = root.appendChild(document.createElement('p'));
              node.id = name;
              rt.effect(() => { node.textContent = values[name](); });
            }
          });
          window.fixture = { rt, fresh, read };
          return read();
        }, mode);
        const unseen = ['secret', 'removed'].includes(mode);
        for (const name of ['literal', 'source', 'bare', 'ordinary', 'kept', 'invalid', 'unavailable']) {
          const placeholder = unseen && ['literal', 'source', 'invalid'].includes(name);
          const value = mode === 'ready' ? 'fresh answer' : name === 'kept' ? 'kept answer'
            : placeholder ? name === 'source' ? 'preview' : 'waiting' : 'baked empty store';
          expect(before[name].value, `${mode}/${name}`).toBe(value);
          expect(await page.locator('#' + name).textContent()).toBe(value);
          if (placeholder) expect(before[name].pending).toBe(true);
          expect(before[name].reader).toBe(mode === 'ready' || name !== 'ordinary');
        }
        const after = await page.evaluate(() => {
          fixture.rt.data.answer = fixture.fresh;
          for (const ready of fixture.rt.data.q.splice(0)) ready();
          return fixture.read();
        });
        for (const [name, state] of Object.entries(after)) {
          expect(state).toEqual({ value: 'fresh answer', pending: false, reader: true });
          expect(await page.locator('#' + name).textContent()).toBe('fresh answer');
        }
      } finally { await page.close(); }
    }
  } finally { await browser.close(); server.stop(true); }
});

test('a built store reader replaces its placeholder when the delayed Rust module loads', async () => {
  const { patchLines } = await import('../../scripts/app.mjs');
  const dir = mkdtempSync(resolve(ROOT, 'target/rust-reader-startup-')), dist = resolve(dir, 'dist');
  const write = (path, content) => writeFileSync(resolve(dir, path), content);
  let browser, server, release;
  try {
    for (const path of ['data/src', 'bake/src', 'web']) mkdirSync(resolve(dir, path), { recursive: true });
    write('app.json', JSON.stringify({ name: 'Reader', app: { id: 'test.reader', name: 'Reader' }, host: { web: {} } }));
    write('app.contract', 'component App\n  resource session = session() as shape string else preview()\n  action export\n    saveFile("export", "app:/data/note.txt", "note.txt")\n  view\n    column\n      text session testId="session"\n      button "Export" id="export" testId="export" press=export\n');
    write('web/build.rs', 'fn main() { contract::bake(plan, startup_data::Reader); }');
    write('Cargo.toml', `[workspace]\nmembers = ["data", "bake"]\nresolver = "2"\n[patch.crates-io]\n${patchLines(dir).join('\n')}\n`);
    cpSync(resolve(ROOT, 'Cargo.lock'), resolve(dir, 'Cargo.lock'));
    write('data/Cargo.toml', `[package]\nname = "startup-data"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\nexact-runner = { path = "${ROOT}/runner" }\n`);
    write('data/src/lib.rs', `use exact_runner::{Answer, DataError, DataSource, Store, Value};
pub struct Reader;
impl DataSource for Reader {
    fn app_id(&self) -> &str { "test.reader" }
    fn grants(&self) -> &str { "secret.keep token\\nfs.read app:/data" }
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn answer(&mut self, store: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::str(match source {
            "session" => store.get("token").unwrap_or("signed out"),
            "preview" => "waiting",
            _ => return Err(DataError::UnknownSource(source.into())),
        })))
    }
}
`);
    write('bake/Cargo.toml', `[package]\nname = "startup-bake"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\nstartup-data = { path = "../data" }\ncontract = { path = "${ROOT}/contract/cli" }\n`);
    write('bake/src/main.rs', `fn main() {
    let plan = contract::compile(&std::fs::read_to_string("app.contract").unwrap()).unwrap();
    let plan = contract::bake(plan, startup_data::Reader).unwrap();
    assert!(plan.resources.iter().any(|r| r.reader));
    std::fs::write("app.plan", plan.encode()).unwrap();
}
`);
    const baked = spawnSync('cargo', ['run', '--offline', '--manifest-path', resolve(dir, 'Cargo.toml'), '-p', 'startup-bake'], {
      cwd: dir, env: { ...process.env, CARGO_TARGET_DIR: resolve(ROOT, 'target') }, encoding: 'utf8', timeout: 120_000,
    });
    expect(baked.status, baked.stdout + baked.stderr).toBe(0);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'rust-reader-startup', '--plan', resolve(dir, 'app.plan'), '--out', dist, '--render', 'none'], {
      cwd: ROOT, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8', timeout: 180_000,
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const gate = Promise.withResolvers();
    release = gate.resolve;
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/rust/wasm/app.module.wasm') await gate.promise;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(key => localStorage.setItem(key, 'signed in'), `exact.secret.@${appDigest(dir)}/token`);
    const requested = page.waitForRequest(request => new URL(request.url()).pathname === '/rust/wasm/app.module.wasm');
    await page.goto(server.url.href + '?agent');
    await requested;
    expect(await page.getByTestId('session').textContent()).toBe('waiting');
    expect(await page.evaluate(() => exact.resources.find(r => r.name === 'session').pending())).toBe(true);
    expect(await page.evaluate(() => exact.data.appId)).toBe('test.reader');
    await page.getByTestId('export').click();
    expect(await page.evaluate(() => exact.files.holds().some(h => h.device?.capability === 'export'))).toBe(true);
    release();
    await page.waitForFunction(() => document.querySelector('[data-testid="session"]')?.textContent === 'signed in');
    expect(await page.evaluate(() => exact.resources.find(r => r.name === 'session').pending())).toBe(false);
    expect(errors).toEqual([]);
  } finally { release?.(); await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 360_000);

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
  for (const name of ['rust-data.js', 'admission.js', 'held.js', 'rt-link.js']) {
    const source = readFileSync(resolve(ROOT, 'host/web-js', name), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'");
    writeFileSync(resolve(dir, name), source);
  }
  for (const name of ['http-body.js', 'grant-admission.js', 'faults.js', 'navigation.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const rustGrantSet=createGrantSet(${JSON.stringify(set)}),tsGrantSet=createGrantSet(${JSON.stringify(normalized(''))});\n`);
  writeFileSync(resolve(dir, 'rt-link.js'), 'export const rt = {};\n');
  const memory = new WebAssembly.Memory({ initial: 1 }), out = 32768;
  let capture = false;
  let output = new Uint8Array(), read = false, fails = false, unknown = false, socket = false, writes = false, cache, preload, prefetch = false, method = 'GET', abi = 11;
  let held = false, heldTag;
  const scopes = [];
  const response = fill => {
    const bytes = [], u8 = value => bytes.push(value), u32 = value => bytes.push(value & 255, value >>> 8 & 255, value >>> 16 & 255, value >>> 24 & 255);
    const string = value => { const encoded = new TextEncoder().encode(value); u32(encoded.length); bytes.push(...encoded); };
    u32(11); fill({ u8, u32, string }); output = Uint8Array.from(bytes); new Uint8Array(memory.buffer, out, output.length).set(output);
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
          w.u8(2); w.u8(read ? 1 : 0); w.u32(0); w.u32(0); w.u32(writes ? 1 : 0);
          if (writes) { w.string('k'); w.u8(1); w.string('private'); }
          w.u32(0); w.u8(0); w.string('');
          if (held) w.u8(15);
          if (heldTag !== undefined) { w.u8(heldTag); return; }
          if (prefetch) w.u8(14);
          if (preload !== undefined) { w.u8(13); w.string(preload); }
          if (capture) { w.u8(16); w.string('ink'); w.string('app:/tmp/ink.png'); w.u8(1); w.string(''); }
          else if (socket) { if (cache !== undefined) { w.u8(12); w.string(cache); } w.u8(11); w.u32(1 << 20); w.u8(0); w.string(''); w.string('GET'); w.string('wss://api.example/book'); w.u32(0); w.u32(0); w.u32(0); } else if (unknown) { w.u8(2); w.string('reader'); } else if (fails) { w.u8(3); w.string('nothing kept'); } else { if (cache !== undefined) { w.u8(12); w.string(cache); } w.u8(1); w.u8(0); w.string(''); w.string(method); w.string('https://api.example/me'); w.u32(0); w.u32(0); }
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
    abi = 9;
    await expect(module.install(data, { reader: '' }, async () => new ArrayBuffer(0))).rejects.toThrow('Rust module ABI 9 is not 11');
    abi = 11;
    await module.install(data, { reader: '' }, async () => new ArrayBuffer(0));
    for (const mode of ['default', 'no-store', 'reload', 'no-cache', 'force-cache', 'only-if-cached']) {
      cache = mode;
      expect(data.answer('reader', [], store).req.cache).toBe(mode);
      held = true;
      expect(data.answer('reader', [], store).req).toMatchObject({ held: true, cache: mode });
      held = false;
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
        held = true;
        expect(data.answer('reader', [], store).req).toMatchObject({ held: true, prefetch: true, cache: mode });
        held = false;
      }
    }
    prefetch = false;
    // A read is one the commit that lets its ticket go aborts (request-send.js); a write runs to its end.
    preload = undefined; cache = undefined;
    for (const [verb, cancel] of [['GET', true], ['head', true], ['POST', false], ['DELETE', false]]) {
      method = verb;
      expect(data.answer('reader', [], store).req.cancel).toBe(cancel);
    }
    method = 'GET';
    preload = 'Bad';
    expect(() => data.answer('reader', [], store)).toThrow('invalid preload HTTP result');
    preload = undefined;
    cache = undefined;
    held = true;
    expect(data.answer('reader', [], store).req).toMatchObject({ held: true, method: 'GET' });
    writes = true;
    for (const tag of [0, 2, 3, 4, 5, 7, 8, 15, 255]) {
      heldTag = tag;
      expect(() => data.answer('reader', [], store)).toThrow('invalid held HTTP result');
      expect(kept).toEqual([]);
    }
    heldTag = undefined; writes = false; held = false;
    // On the page each request carries a call whose `held` held.js counts, and `http` tells a held request from the
    // same one unheld (resources.js `sameReq`); a render makes no call. A held request's reply (a refusal here: no
    // grant covers its origin) is a held reply, and a request its parse asks for takes the same call.
    const heldJs = await import(pathToFileURL(resolve(dir, 'held.js')).href);
    held = true;
    const asked = data.answer('reader', [], store);
    held = false;
    const unheld = data.answer('reader', [], store);
    expect([asked.call, unheld.call, asked.req.http, unheld.req.http]).toEqual([{ held: 1 }, { held: 0 }, 'ordered held', 'ordered']);
    globalThis.__exactRender = true; held = true;
    expect(data.answer('reader', [], store)).not.toHaveProperty('call');
    globalThis.__exactRender = false;
    const ticket = { name: 'reader' };
    ticket.owner = { ticket };
    asked.call.ticket = ticket;
    const outcome = await data.fetch({ ...asked.req });
    expect([outcome.failed, asked.call.held, heldJs.heldReplies()]).toEqual([2, 0, { reader: 1 }]);
    expect(data.parse('reader', [], outcome, store).req.held).toBe(true);
    expect(asked.call.held).toBe(1);
    held = false;
    expect(data.parse('reader', [], outcome, store).req.held).toBe(false);
    expect(asked.call.held).toBe(0);
    capture = true;
    expect(data.answer('reader', [], store).req).toEqual({ canvas: { surface: 'ink', path: 'app:/tmp/ink.png' }, scope: '' });
    capture = false;
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
    expect(data.answer('reader', [], store).req).toMatchObject({ stream: true, socket: true, cancel: false, url: 'wss://api.example/book' });
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


// A Rust long poll's reply that is no event (LLP 1016.000 D8): the reader counts it, and rust-data.js lands nothing of it.
test("a Rust long poll's reply that is no event reaches no message", async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-rust-poll-'));
  for (const name of ['rust-data.js', 'admission.js', 'held.js']) {
    const source = readFileSync(resolve(ROOT, 'host/web-js', name), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'");
    writeFileSync(resolve(dir, name), source);
  }
  for (const name of ['grant-admission.js', 'faults.js', 'navigation.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
  writeFileSync(resolve(dir, 'http-body.js'), `export const boundedHttpBody = r => r.arrayBuffer(), longPollRefusal = () => null;
export function streamed(req, _grants, message) {
  const m = (event, data) => ({ event, id: '', data, coalesced: 0 });
  message(m('exact:reply', '{"seq":1}'));
  message(m('', 'event'));
  return Promise.resolve({ kind: 2, body: new TextEncoder().encode('ended') });
}\n`);
  writeFileSync(resolve(dir, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const rustGrantSet=createGrantSet(${JSON.stringify(normalized(''))}),tsGrantSet=rustGrantSet;\n`);
  writeFileSync(resolve(dir, 'rt-link.js'), 'export const rt = {};\n');
  const memory = new WebAssembly.Memory({ initial: 1 }), out = 32768;
  let output = new Uint8Array();
  const exports = {
    memory, exact_logic_abi: () => 11, exact_logic_create: () => 1, exact_logic_alloc: () => 0, exact_logic_dealloc() {},
    exact_logic_output: () => out, exact_logic_output_len: () => output.length,
    exact_logic_call(_session, pointer) {
      const code = new Uint8Array(memory.buffer)[pointer + 4], bytes = [];
      const u32 = v => bytes.push(v & 255, v >>> 8 & 255, v >>> 16 & 255, v >>> 24 & 255), string = v => { const e = new TextEncoder().encode(v); u32(e.length); bytes.push(...e); };
      u32(11);
      if (code === 0) { bytes.push(0); string('test.rust'); string(''); } else bytes.push(0, 0, 3);
      output = Uint8Array.from(bytes); new Uint8Array(memory.buffer, out, output.length).set(output);
      return 0;
    },
  };
  const instantiate = WebAssembly.instantiate;
  WebAssembly.instantiate = async () => ({ instance: { exports } });
  try {
    const module = await import(pathToFileURL(resolve(dir, 'rust-data.js')).href);
    const data = { q: [] };
    await module.install(data, {}, async () => new ArrayBuffer(0));
    const heard = {};
    for (const longPoll of [undefined, { cursor: 'since', from: 'seq', sameCursorEvent: false }]) {
      const messages = [];
      await data.fetch({ method: 'GET', url: 'https://api.example/poll', stream: true, longPoll }, m => messages.push(m.event), new AbortController());
      heard[longPoll ? 'longPoll' : 'stream'] = messages;
    }
    expect(heard).toEqual({ stream: ['exact:reply', ''], longPoll: [''] });
  } finally {
    WebAssembly.instantiate = instantiate;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a forced Rust resource keeps forcing parser-created requests and tickets after a stream message, and preserves held changes and store invalidation', async () => {
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
    for (const kind of ['request', 'stream', 'held']) {
      const ctx = vm.createContext({
        document: createDocument(), location: new URL('https://example.test/'), navigator: {},
        __exactRender: true, console, performance, setTimeout, clearTimeout, queueMicrotask,
        addEventListener() {}, removeEventListener() {}, requestAnimationFrame: () => 0,
        getComputedStyle: () => ({ fontSize: '16px' }),
        TextEncoder, TextDecoder, URL, URLSearchParams, AbortController,
        stream: kind === 'stream', heldCase: kind === 'held',
      });
      ctx.globalThis = ctx; ctx.self = ctx;
      vm.runInContext(code, ctx);
      const result = await vm.runInContext(`(async () => {
        const sent = [];
        if (heldCase) {
          // What rust-data.js answers on the page (rust-readers' first test): a held request's \`http\` says it is held,
          // and its call counts it out.
          globalThis.__exactRender = false;
          const marked = rt.sig(true);
          rt.data.answer = (_source, args) => ({ store: true, call: { held: args[0] ? 1 : 0 }, req: { method: 'GET', url: '/same', held: args[0], http: args[0] ? 'ordered held' : 'ordered' } });
          rt.data.fetch = req => { sent.push(req.held); return new Promise(() => {}); };
          rt.mount(() => {
            const value = rt.res('value', 'rust', () => [marked()], undefined, undefined, 's', '');
            rt.effect(() => value());
          });
          rt.commit(() => rt.W(marked, false));
          rt.commit(() => rt.W(marked, true));
          rt.Store.set('token', 'renewed');
          await new Promise(queueMicrotask);
          return JSON.stringify(sent);
        }
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
      expect(JSON.parse(result)).toEqual(kind === 'held'
        ? [true, false, true, true]
        : [{ url: '/first', forced: true }, { url: '/next', forced: true }]);
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

// A Rust call that fails may have changed the module's own state before it failed: refused after its answer ran, or
// trapped. The next call runs in a fresh instance, activated again, as natively (logic/src/lib.rs `run`).
test('a Rust call after a refused or trapped one runs in a fresh instance', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-rust-fresh-'));
  for (const name of ['rust-data.js', 'admission.js', 'held.js']) {
    const source = readFileSync(resolve(ROOT, 'host/web-js', name), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'");
    writeFileSync(resolve(dir, name), source);
  }
  for (const name of ['http-body.js', 'grant-admission.js', 'faults.js', 'navigation.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const rustGrantSet=createGrantSet(${JSON.stringify(normalized(''))}),tsGrantSet=createGrantSet(${JSON.stringify(normalized(''))});\n`);
  writeFileSync(resolve(dir, 'rt-link.js'), 'export const rt = {};\n');
  // Each instance counts the calls named `bump` it ran; `count` answers that count.
  let instances = 0;
  const instance = () => {
    instances++;
    const memory = new WebAssembly.Memory({ initial: 1 }), out = 32768;
    let output = new Uint8Array(), bumps = 0;
    const response = fill => {
      const bytes = [], u8 = value => bytes.push(value), u32 = value => bytes.push(value & 255, value >>> 8 & 255, value >>> 16 & 255, value >>> 24 & 255);
      const string = value => { const encoded = new TextEncoder().encode(value); u32(encoded.length); bytes.push(...encoded); };
      u32(11); fill({ u8, u32, string }); output = Uint8Array.from(bytes); new Uint8Array(memory.buffer, out, output.length).set(output);
    };
    return { exports: {
      memory, exact_logic_abi: () => 11, exact_logic_create: () => 1, exact_logic_alloc: () => 0, exact_logic_dealloc() {},
      exact_logic_output: () => out, exact_logic_output_len: () => output.length,
      exact_logic_call(_session, pointer) {
        const code = new Uint8Array(memory.buffer)[pointer + 4];
        if (code === 0) response(w => { w.u8(0); w.string('test.rust'); w.string(''); });
        else if (code === 1 || code === 2) response(w => { w.u8(0); w.u8(0); w.u8(3); });
        else {
          const view = new DataView(memory.buffer), n = view.getUint32(pointer + 5, true);
          const source = new TextDecoder().decode(new Uint8Array(memory.buffer, pointer + 9, n));
          if (source === 'refused') { bumps++; return 1; }
          if (source === 'trapped') { bumps++; throw new WebAssembly.RuntimeError('unreachable'); }
          response(w => { w.u8(2); w.u8(0); w.u32(0); w.u32(0); w.u32(0); w.u32(0); w.u8(0); w.string(''); w.u8(0); w.u8(2); w.string(String(bumps)); });
        }
        return 0;
      },
    } };
  };
  const [instantiate, Instance] = [WebAssembly.instantiate, WebAssembly.Instance];
  WebAssembly.instantiate = async () => ({ instance: instance(), module: 'compiled' });
  WebAssembly.Instance = function (module) { expect(module).toBe('compiled'); return instance(); };
  try {
    const module = await import(pathToFileURL(resolve(dir, 'rust-data.js')).href);
    const data = { q: [] }, store = { map: new Map(), set() {} };
    await module.install(data, { count: '', refused: '', trapped: '' }, async () => new ArrayBuffer(0));
    expect(data.answer('count', [], store).v).toBe('0');
    expect(() => data.answer('refused', [], store)).toThrow('Rust module rejected the call');
    expect(data.answer('count', [], store).v).toBe('0');
    expect(() => data.answer('trapped', [], store)).toThrow('unreachable');
    expect(data.answer('count', [], store).v).toBe('0');
    expect(instances).toBe(3);
    expect(data.logic.exports.exact_logic_abi()).toBe(11);
  } finally {
    [WebAssembly.instantiate, WebAssembly.Instance] = [instantiate, Instance];
    rmSync(dir, { recursive: true, force: true });
  }
});
