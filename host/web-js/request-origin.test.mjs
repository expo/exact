import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// LLP 1133.036 D6 through the JS target's request adapters: a Rust source's request (`createRequestExecutor`), a
// stream (`streamed`, a Rust or TypeScript source's, and a WebSocket open) go out as the origin of the commit that
// asked them. From a public page, a tool's, a wake's or a device event's reach to the local network does not start
// while its permission is unanswered or denied, and its redirects are not followed; the app's always may. A request
// that carries no origin is checked as a non-app one.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
const PRIVATE = 'http://192.168.1.40/json', PUBLIC = 'https://public.example/json', SOCKET = 'ws://192.168.1.40/socket';

function normalized(spec) {
  const { cmd, pre } = webCompiler();
  if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-grants-')), input = resolve(scratch, 'grants.txt');
  writeFileSync(input, spec);
  const run = spawnSync(cmd, [...pre, 'normalize-grants', input], { cwd: root, encoding: 'utf8' });
  rmSync(scratch, { recursive: true });
  if (run.status !== 0) throw new Error(run.stderr || 'the grant normalizer failed');
  return JSON.parse(run.stdout);
}

let dir, kept, sent = [], opened = [];
let spaces, admission, body, tsData, tsFetch;
beforeAll(async () => {
  dir = mkdtempSync(resolve(tmpdir(), 'exact-request-origin-'));
  kept = ['fetch', 'WebSocket', 'location', 'exact', '__exact_module', '__exact_async_context', '__exact_ordinary', 'asker'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  globalThis.location = { hostname: 'public.example', href: 'https://public.example/', origin: 'https://public.example' };
  // The browser's transports, each use recorded; a request is answered at once, a socket never opens.
  globalThis.fetch = async (url, init) => {
    sent.push([String(url), init?.redirect]);
    const response = new Response('sent');
    Object.defineProperty(response, 'url', { value: String(url) });
    return response;
  };
  globalThis.WebSocket = class { constructor(url) { opened.push(String(url)); throw new Error('no socket here'); } };
  const fromWeb = name => readFileSync(resolve(root, 'host/web-js', name), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'");
  for (const name of ['http-body.js', 'grant-admission.js', 'faults.js', 'navigation.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(root, 'host/web', name)));
  for (const name of ['admission.js', 'ts-stream.js']) writeFileSync(resolve(dir, name), fromWeb(name));
  writeFileSync(resolve(dir, 'ts-data.js'), fromWeb('ts-data.js').replace('__APP_TS__', './source.js').replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '').replace("'__FETCH_MODULE__'", "'./ts-fetch.js'"));
  for (const name of ['ts-fetch.js', 'ts-body-from.js', 'shape.js', 'uploads.js']) cpSync(resolve(root, 'host/web-js', name), resolve(dir, name));
  // A TypeScript source that streams: asked by whichever origin `asker` names.
  writeFileSync(resolve(dir, 'source.js'), `import {fetch} from './ts-fetch.js';
export const answer = (_name, [url]) => fetch(url, { exactStream: e => e.type === 'error' ? e.message : e.type });`);
  writeFileSync(resolve(dir, 'rt.js'), 'export const clock={agent:false,now:0},journal=[],Resources=[],inflight={n:0};export const drive=()=>{},time=()=>{},Origin={now:()=>globalThis.asker};export const checkpoint=()=>({kept:null});export const commit=f=>f();export const R=()=>{};export const painted=()=>Promise.resolve();export {painted as canvasCommitted};');
  writeFileSync(resolve(dir, 'rt-link.js'), 'export const rt = { journal: [], clock: { now: 0 } };\n');
  writeFileSync(resolve(dir, 'names.js'), 'export const sourceTypes={};');
  const grants = normalized(`net.fetch http://192.168.1.40\nnet.fetch https://public.example\nnet.websocket ws://192.168.1.40`);
  writeFileSync(resolve(dir, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const tsGrantSet=createGrantSet(${JSON.stringify(grants)}),rustGrantSet=tsGrantSet;\n`);
  const load = name => import(pathToFileURL(resolve(dir, name)).href);
  [spaces, admission, body, tsData, tsFetch] = await Promise.all(['address-space.js', 'admission.js', 'http-body.js', 'ts-data.js', 'ts-fetch.js'].map(load));
});
afterAll(() => {
  for (const [k, d] of kept) if (d) Object.defineProperty(globalThis, k, d); else delete globalThis[k];
  rmSync(dir, { recursive: true, force: true });
});

const permission = state => { spaces.setSpaceState('local', state); spaces.setSpaceState('loopback', state); };
const text = o => new TextDecoder().decode(o.body);
// Each adapter asked for each destination as `origin`: what it answered, and what reached the browser.
async function ask(origin) {
  const set = (await import(pathToFileURL(resolve(dir, 'admission-data.js')).href)).rustGrantSet;
  const run = admission.createRequestExecutor('test.origin', set, r => r.arrayBuffer());
  const stream = url => body.streamed({ method: 'GET', url, headers: [], maxResponseBytes: 1024, origin }, set, () => {}, new AbortController());
  sent = []; opened = [];
  const rust = await run({ method: 'GET', url: PRIVATE, headers: [], origin });
  const streamedPrivate = await stream(PRIVATE), socket = await stream(SOCKET);
  const said = [rust.failed ? rust.message : 'sent', streamedPrivate.kind === 2 ? text(streamedPrivate) : 'sent', socket.kind === 2 ? text(socket) : 'sent'];
  const reached = [sent.map(([url]) => url), opened];
  // A public destination is sent; whether its redirects are followed is the gate's.
  sent = [];
  await run({ method: 'GET', url: PUBLIC, headers: [], origin });
  await stream(PUBLIC);
  return { said, reached, redirects: sent.map(([, redirect]) => redirect) };
}

test("a Rust request, a stream and a socket from a public page go out as their origin's", async () => {
  for (const origin of ['tool', 'wake', 'device', undefined]) {
    for (const state of ['prompt', 'unknown', 'denied']) {
      permission(state);
      const why = state === 'denied' ? 'NotAllowedError: local network access is denied' : 'NotAllowedError: person-required';
      expect(await ask(origin), `${origin} ${state}`).toEqual({ said: [why, why, why], reached: [[], []], redirects: ['error', 'error'] });
    }
    permission('granted');
    expect(await ask(origin), `${origin} granted`).toEqual({ said: ['sent', 'sent', 'sent'], reached: [[PRIVATE, PRIVATE], [SOCKET]], redirects: ['follow', 'follow'] });
  }
  permission('prompt');
  expect(await ask('app')).toEqual({ said: ['sent', 'sent', 'sent'], reached: [[PRIVATE, PRIVATE], [SOCKET]], redirects: ['follow', 'follow'] });
});

test("a TypeScript source's stream opens as the origin that asked it", async () => {
  const data = { q: [] };
  tsData.install(data);
  permission('prompt');
  const watch = async (asker, url) => {
    globalThis.asker = asker;
    const answer = data.answer('watch', [url], new Map(), 'watch', false, false);
    globalThis.asker = 'app';
    // The answer's stream opens after its turn, as rt.js opens it.
    expect(tsFetch.answering.call).toBe(null);
    sent = []; opened = [];
    const end = await answer.stream(() => {}, Object.assign(new AbortController(), { active: () => true }));
    return [end.v, sent.map(([u, redirect]) => [u, redirect]), opened];
  };
  expect(await watch('tool', PRIVATE)).toEqual(['NotAllowedError: person-required', [], []]);
  expect(await watch('device', SOCKET)).toEqual(['NotAllowedError: person-required', [], []]);
  expect((await watch('wake', PUBLIC))[1]).toEqual([[PUBLIC, 'error']]);
  expect((await watch('app', PRIVATE))[1]).toEqual([[PRIVATE, 'follow']]);
});

// rt.js's part: a Rust source's request, a resource's or a mutation's, carries the origin of the commit that asked
// it, and a request its reply asks for carries the same.
test("the runtime sends a Rust source's requests as the origin that asked them", async () => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') return new Response(null, { status: 204 });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    const local = resolve(root, '.' + path), file = existsSync(local) ? local : resolve(root, 'host/web', path.split('/').pop());
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const sent = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      const sent = [], asker = { now: 'tool' };
      rt.Origin.now = () => asker.now;
      rt.clock.agent = true;
      rt.data.fetch = req => { sent.push([req.url, req.origin]); return Promise.resolve({ status: 200, headers: [], body: new Uint8Array() }); };
      // A reply to `/first` asks for `/then`, whose reply is the answer.
      rt.data.parse = () => sent.at(-1)[0] === '/first' ? { req: { method: 'GET', url: '/then', headers: [] } } : { v: 'reply' };
      rt.data.answer = source => ({ req: { method: 'GET', url: source === 'send' ? '/first' : '/read', headers: [] } });
      const slot = rt.sig(null);
      let read, mutation;
      rt.mount(root => {
        read = rt.res('read', 'read', () => [], undefined, undefined, 's', '');
        mutation = rt.mut('send', slot, [], 's');
        rt.hd(root, { headTitle: () => read() });
      });
      const settle = async () => { while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0)); };
      await settle();
      rt.commit(() => rt.M(mutation, 'send', []));
      await settle();
      asker.now = 'app';
      rt.commit(() => rt.M(mutation, 'send', []));
      await settle();
      return sent;
    });
    expect(sent).toEqual([['/read', 'tool'], ['/first', 'tool'], ['/then', 'tool'], ['/first', 'app'], ['/then', 'app']]);
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);
