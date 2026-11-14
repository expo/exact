import { expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createGrantSet, scopedGrantSet } from './grant-admission.js';
import { hostFixtureFile } from './tests/glue-fixture.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from '../web-js/module.mjs';
import { renderedPages } from '../web-js/rendered-pages.mjs';

const ROOT = resolve(import.meta.dir, '../..');
function normalized(spec) {
  const dir = mkdtempSync(resolve(ROOT, 'target/grant-parity-'));
  try {
    const path = resolve(dir, 'grants');
    writeFileSync(path, spec);
    const { cmd, pre } = webCompiler();
    if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
    const result = spawnSync(cmd, [...pre, 'normalize-grants', path], { encoding: 'utf8' });
    expect(result.status, result.stderr).toBe(0);
    return JSON.parse(result.stdout);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('module-glue prepare accepts normalized formatting and exact-only grants', async () => {
  const set = createGrantSet(structuredClone(normalized('net.fetch https://api.example\nauth.session https://login.example')));
  const formatted = '\u0085net.fetch https://api.example\n\n\u0085auth.session https://login.example\u0085';
  const win = { Error, SyntaxError, Function, Promise, structuredClone, crypto: { subtle: {} }, indexedDB: null, addEventListener() {},
    exact: { abi: 1, appId: 'test.module', grants: formatted, answer() {} }, __exact_install_storage() {}, __exact_has_overlay: () => '0',
    document: { createElement: () => ({}), head: { append() {} } } };
  const frame = { hidden: false, contentWindow: win, setAttribute() {}, remove() {} };
  const descriptors = Object.fromEntries(['fetch', 'document', 'location'].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  Object.defineProperties(globalThis, {
    fetch: { configurable: true, writable: true, value: async () => { const response = new Response(''); Object.defineProperty(response, 'url', { value: 'http://127.0.0.1/module-prelude.js' }); return response; } },
    document: { configurable: true, value: { createElement: () => frame, body: { append() {} } } },
    location: { configurable: true, value: { href: 'http://127.0.0.1/module', origin: 'http://127.0.0.1' } },
  });
  globalThis.exact = { moduleDigest: async () => 'a'.repeat(64) };
  const script = new TextEncoder().encode('module');
  const receipt = new TextEncoder().encode(JSON.stringify({ version: 1, abi: 1, appId: 'test.module', grants: formatted,
    web: { file: 'app.js', bytes: script.length, sha256: 'a'.repeat(64) }, module: { sha256: 'b'.repeat(64) } }));
  try {
    const { call, prepare } = await import(`./module-glue.js?prepare=${Date.now()}`);
    const realm = await prepare({ receipt, script }, { appId: 'test.module', grants: 'net.fetch https://api.example\nauth.session https://login.example', grantSet: set, placement: 'main' });
    expect(realm.meta.grants).toBe(formatted);
    expect(call({ op: 'activate', id: realm.id, appId: 'test.module', grants: 'net.fetch https://api.example\nauth.session https://login.example', revision: 'b'.repeat(64) })).toEqual({ ok: true });
    realm.dispose();
  } finally {
    for (const [name, descriptor] of Object.entries(descriptors)) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name];
    }
    delete globalThis.exact;
  }
});

test("glue.js surface admission uses a valid raw child line even when another I/O line is malformed", () => {
  const source = hostFixtureFile(new URL('./glue.js', import.meta.url), 'utf8');
  const declaration = source.match(/function surfaceGranted\(op\) \{[^]*?\n\}/)[0];
  const admitted = (grants, op) => new Function('host', 'op', 'scopedGrantSet', `${declaration};return surfaceGranted(op);`)({ grants, grantSet: normalized(grants.join('\n')) }, op, scopedGrantSet);
  const grants = ['secret.keep camelCase', 'surface.read world'];
  expect(admitted(grants, { mode: 'capture', name: 'world', scope: 'surface.read world' })).toBe(true);
  expect(admitted(grants, { mode: 'capture', name: 'world', scope: null })).toBe(true);
  expect(admitted(grants, { mode: 'capture', name: 'world', scope: 'surface.read elsewhere' })).toBe(false);
  expect(admitted(grants, { mode: 'capture', name: 'world', scope: '\u0085surface.read world\u0085' })).toBe(true);
  expect(admitted(grants, { mode: 'capture', name: 'world', scope: '\ufeffsurface.read world' })).toBe(false);
});

test('opaque no-cors responses are refused on both web targets', async () => {
  let hits = 0;
  const destination = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch() { hits++; return new Response('hidden'); } });
  const origin = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/redirect') return Response.redirect(destination.url.href, 307);
    if (path === '/') return new Response('<div></div>', { headers: { 'content-type': 'text/html' } });
    return new Response(Bun.file(resolve(ROOT, '.' + path)));
  } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.goto(origin.url.href);
    const result = await page.evaluate(async ({ set, url }) => {
      const { fetchWith, createRequestExecutor } = await import('/host/web-js/admission.js');
      const { request } = await import('/host/web/http-body.js');
      const raw = await fetch(url, { mode: 'no-cors' });
      const direct = await fetchWith(set, url, { mode: 'no-cors' }).then(() => 'accepted', e => e.kind);
      const js = await createRequestExecutor('test.opaque', set)({ method: 'GET', url, mode: 'no-cors', headers: [], origin: 'app' });
      const wasm = await request({ method: 'GET', url, mode: 'no-cors', headers: [], origin: 'app' }, { grantSet: set, controllers: new Set() });
      const stream = await request({ method: 'GET', url, mode: 'no-cors', headers: [], stream: true, origin: 'app' }, { grantSet: set, controllers: new Set(), message() {} });
      const poll = await request({ method: 'GET', url, mode: 'no-cors', headers: [], stream: true, longPoll: { cursor: 'cursor', from: 'seq', sameCursorEvent: false }, origin: 'app' }, { grantSet: set, controllers: new Set(), message() {} });
      return { type: raw.type, url: raw.url, direct, js: js.failed, wasm: wasm.kind, stream: stream.kind, poll: poll.kind };
    }, { set: normalized(`net.fetch ${origin.url.origin}`), url: new URL('/redirect', origin.url).href });
    expect(result).toEqual({ type: 'opaque', url: '', direct: 'Refused', js: 2, wasm: 2, stream: 2, poll: 2 });
    expect(hits).toBe(6);
  } finally { await browser.close(); origin.stop(true); destination.stop(true); }
});

test.each(['\u0085auth.session https://login.example\u0085', 'auth.google\tclient-id'])('the JS build installs auth and storage for %j', async auth => {
  const dir = mkdtempSync(resolve(ROOT, 'target/grant-build-')), dist = resolve(dir, 'dist');
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Grants', app: { id: 'test.grants', name: 'Grants' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), 'shape Status\n  text: string\ncomponent App\n  resource status = status() as shape Status\n  view\n    text status.text testId="status"\n');
    const grants = `${auth}\nauth.callback https://app.example/cb\n\u0085fs.read app:/data\n# \ufeff`;
    expect(normalized(grants).error).toBeNull();
    writeFileSync(resolve(dir, 'app.ts'), `import type { Answer } from './app.contract.d.ts';
export const grants = ${JSON.stringify(grants)};
export const answer: Answer = () => ({ text: 'ready' });\n`);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'grants', '--out', dist, '--render', 'none'], { cwd: ROOT, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href + '?agent');
    await page.waitForFunction(() => document.querySelector('[data-testid="status"]')?.textContent === 'ready');
    expect(await page.evaluate(() => typeof openAuthSession)).toBe('function');
    if (auth.startsWith('auth.google')) expect(await page.evaluate(async () => {
      const pending = exact.authSession.open({ url: 'https://accounts.google.com/authorize?client_id=client-id', callback: authCallback(), state: 's' });
      const holds = exact.auth.holds();
      for (const hold of holds) exact.auth.answer({ ticket: hold.ticket, op: 'tap', choice: 'cancel' });
      return { held: holds.length, status: await pending.then(() => 200, e => e.status) };
    })).toEqual({ held: 1, status: 499 });
    expect(readFileSync(resolve(dist, 'storage-fs.js'), 'utf8')).toContain('createFileSystem');
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
});

test('a JS render installs Rust grants before MIDI readers', async () => {
  await renderedPages(`routes nav
  home "/" render=build

shape MIDI
  state: string

component App
  resource midi = exactMIDI() as shape MIDI
  view
    head title=midi.state
`, page => {
    expect(page('')[0]).toBe('prompt');
  }, {
    'strings/en.json': '{"purpose.midi":"Plays the pattern on your instruments."}',
    'web/build.rs': 'fn main() { contract::bake(plan, render_grants_data::Reader); }',
    'data/Cargo.toml': `[package]
name = "render-grants-data"
version = "0.1.0"
edition = "2021"
[dependencies]
exact-runner = { path = "${ROOT}/runner" }
[workspace]
`,
    'data/src/lib.rs': `use exact_runner::{DataError, DataSource, Value};
pub struct Reader;
impl DataSource for Reader {
    fn app_id(&self) -> &str { "test.heads" }
    fn grants(&self) -> &str { "device.midi purpose.midi" }
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}
`,
  });
}, 180_000);
