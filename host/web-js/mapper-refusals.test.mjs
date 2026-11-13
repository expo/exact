// What a stream's mapper may call on the web JS target (LLP 1016.000 D7, the amendment of 2026-11-10): `fetch`,
// timers and storage refuse in every mapper call of every stream kind and in its call for the stream's end, before
// its first `await` and after it, with Hermes's words (js/src/fetch.js `request`, storage.js `storageCall`); a socket's mapper
// still sends on its own socket while it answers `open` or a `message`. The mappers are lowered as the bundle lowers
// `async` (js/bake/src/typescript.mjs `asyncContextHelper`), so each step after an `await` runs in the context
// ts-data.js's `__exact_async_context` puts back.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
import { asyncContextHelper } from '../../js/bake/src/typescript.mjs';

const root = resolve(import.meta.dir, '../..');
const GLOBALS = ['location', 'exact', '__exact_async_context', '__exact_ordinary', 'fixture', 'captureCanvas'];
let dir, data, journal, kept;
// What left the page: requests, writes and socket sends.
const fixture = { fetched: [], written: [], sent: [] };
beforeAll(async () => {
  kept = GLOBALS.map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
  globalThis.location = { href: 'https://app.test/' };
  globalThis.fixture = fixture;
  dir = mkdtempSync(resolve(tmpdir(), 'exact-mapper-refusals-'));
  for (const name of ['ts-fetch.js', 'ts-stream.js', 'ts-body-from.js', 'shape.js', 'uploads.js']) cpSync(resolve(root, 'host/web-js', name), resolve(dir, name));
  for (const name of ['grant-admission.js', 'faults.js', 'address-space.js', 'navigation.js', 'storage-environment.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(root, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission-real.js'), readFileSync(resolve(root, 'host/web-js/admission.js'), 'utf8').replaceAll("'../web/", "'./"));
  // Every grant held; a request answers at once, counted.
  writeFileSync(resolve(dir, 'admission.js'), `export * from './admission-real.js';
export const hasGrant = () => true;
export const fetchWith = (set, input) => { globalThis.fixture.fetched.push(String(input)); return Promise.resolve(new Response('asset')); };\n`);
  writeFileSync(resolve(dir, 'admission-data.js'), "import { createGrantSet } from './admission.js'; export const tsGrantSet = createGrantSet(null);\n");
  writeFileSync(resolve(dir, 'storage-fs.js'), 'export const createFileSystem = () => ({ writeFile: async path => { globalThis.fixture.written.push(path); } });\n');
  writeFileSync(resolve(dir, 'rt.js'), 'export const clock={agent:false,now:0},journal=[],Resources=[],inflight={n:0};export const drive=()=>{},time=()=>{},Origin={now:()=>"app"};export const checkpoint=()=>({kept:null});export const commit=f=>f();export const R=()=>{};export const painted=()=>Promise.resolve();export const canvasCommitted=()=>{throw new Error("unexpected capture")};\n');
  writeFileSync(resolve(dir, 'rt-link.js'), "import * as rt from './rt.js'; export { rt };\n");
  writeFileSync(resolve(dir, 'names.js'), 'export const sourceTypes = {};\n');
  // Each kind's transport: one message (an upload's progress; a socket's `open` and a message), then its end.
  writeFileSync(resolve(dir, 'http-body.js'), `const m = (data, event = '') => ({ event, id: '', data, coalesced: 0 });
export function streamed(req, grantSet, message, controller, bodyFile, protocol) {
  if (req.url.endsWith('/rejected')) return Promise.reject(Object.assign(new Error('upload transport failed'), { reason: 'upload-failed' }));
  if (protocol.socket) {
    protocol.socket(Object.freeze({ readyState: 1, bufferedAmount: 0, check() {}, send(data) { globalThis.fixture.sent.push(data); }, close() {} }));
    message(m('', 'open')); message(m('frame'));
    return Promise.resolve({ close: { code: 1000, reason: '', wasClean: true } });
  }
  if (req.upload) { message(m('', 'exact:sending')); message(m(JSON.stringify({ loaded: 1, total: 1 }), 'exact:progress')); return Promise.resolve({ status: 200, body: new TextEncoder().encode('done') }); }
  if (req.longPoll) message(m('{"seq":1}', 'exact:reply'));
  message(m(protocol.bytes && !req.longPoll && /bytes/.test(req.url) ? new ArrayBuffer(1) : 'event'));
  return Promise.resolve({ kind: 2, body: new TextEncoder().encode('the stream ended') });
}\n`);
  writeFileSync(resolve(dir, 'async-context.js'), asyncContextHelper);
  // The issue's mapper, for each kind: what fetch, storage, a timer and a send say before its first await and after it.
  writeFileSync(resolve(dir, 'source.js'), `import resume from './async-context.js';
import { fetch, setTimeout } from './ts-fetch.js';
export const appId = 'test.mapper';
const STREAMS = {
  events: ['https://example.test/events', {}],
  bytes: ['https://example.test/bytes', {}],
  longPoll: ['https://example.test/poll', { exactLongPoll: { cursor: 'since', from: 'seq' } }],
  upload: ['https://example.test/upload', { method: 'POST', body: 'x', exactUploadProgress: true }],
  rejected: ['https://example.test/rejected', { method: 'POST', body: 'x', exactUploadProgress: true }],
  socket: ['wss://example.test/socket', {}],
  requestEvents: [new Request('https://example.test/events', { method: 'POST', body: 'hello' }), {}],
  requestUpload: [new Request('https://example.test/upload', { method: 'POST', body: 'hello' }), { exactUploadProgress: true }],
};
const tried = run => { try { return Promise.resolve(run()).then(() => 'ran', e => e.message); } catch (e) { return Promise.resolve(e.message); } };
export function answer(name, args, store, storage) {
  // An answer of its own, whose steps interleave with a mapper's.
  if (name === 'plain') return resume(function* () {
    yield Promise.resolve();
    const asset = yield fetch('https://example.test/asset').then(r => r.text());
    yield storage.fs.writeFile('app:/data/answer.txt', asset);
    return asset;
  })();
  const [url, init] = STREAMS[args[0]];
  const calls = socket => ({
    fetch: tried(() => fetch('https://example.test/asset').then(r => r.text())),
    capture: tried(() => captureCanvas('ink', 'app:/tmp/ink.png')),
    storage: tried(() => storage.fs.writeFile('app:/data/m.txt', 'kept')),
    timer: tried(() => setTimeout(() => {}, 1)),
    ...(socket ? { send: tried(() => socket.send('reply')) } : {}),
  });
  return fetch(url, { ...init, exactStream: resume(function* (event, socket) {
    const now = calls(socket);
    yield Promise.resolve();
    const later = calls(socket), said = { type: event.type };
    for (const [when, tries] of Object.entries({ now, later })) for (const [what, p] of Object.entries(tries)) said[what + ' ' + when] = yield p;
    return JSON.stringify(said);
  }) });
}\n`);
  writeFileSync(resolve(dir, 'ts-data.js'), readFileSync(resolve(root, 'host/web-js/ts-data.js'), 'utf8')
    .replace('__APP_TS__', './source.js').replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '').replace("'__FETCH_MODULE__'", "'./ts-fetch.js'"));
  const ts = await import(pathToFileURL(resolve(dir, 'ts-data.js')).href);
  ({ journal } = await import(pathToFileURL(resolve(dir, 'rt.js')).href));
  ts.install(data = { q: [] });
});
afterAll(() => {
  for (const [key, descriptor] of kept) descriptor ? Object.defineProperty(globalThis, key, descriptor) : delete globalThis[key];
  rmSync(dir, { recursive: true, force: true });
});

// Every mapper call of a stream of `kind`, its end's last: what each call said.
async function mapped(kind) {
  const r = data.answer('feed', [kind], new Map()), calls = [];
  const controller = Object.assign(new AbortController(), { resource: true });
  const end = await r.stream(o => calls.push(o), controller);
  return [...calls, end].map(o => o.error ? { error: o.error } : JSON.parse(o.v));
}
test("a stream refuses a Request's own body rather than send none", async () => {
  for (const kind of ['requestEvents', 'requestUpload']) {
    const answer = data.answer('feed', [kind], new Map());
    const end = await answer.stream(() => {}, Object.assign(new AbortController(), { resource: true }));
    expect(end.error).toBe("fetch: a Request's body is a ReadableStream, which a stream cannot send; pass its bytes as init.body (await request.arrayBuffer())");
  }
});

test('a TypeScript upload transport rejection answers with a failure outcome', async () => {
  const answer = data.answer('feed', ['rejected'], new Map());
  expect(await answer.stream(() => {}, new AbortController()))
    .toEqual({ error: 'upload transport failed', code: 'error', reason: 'upload-failed' });
});

const STREAM = "fetch() is unavailable in a stream's mapper: fetch in the answer that opens the stream";
const SOCKET = "fetch() is unavailable in a socket's mapper: it answers each frame now";
const STORAGE = "storage.writeFile() called in a stream's mapper: it was never run. Start storage inside an answer";
const CAPTURE = "captureCanvas() is unavailable in a stream's mapper: capture in the answer that opens the stream";
const TIMER = "setTimeout() is unavailable here: a timer belongs to the answer that arms it, and none is running (module initialization, a stream's mapper, background work, or an answer that has given its value or was let go). A cache needs no timer to expire: fetch keeps each answer as long as its HTTP headers allow";
const refused = (type, fetchNow = STREAM) => ({ type, 'capture now': CAPTURE, 'capture later': CAPTURE, 'fetch now': fetchNow, 'storage now': STORAGE, 'timer now': TIMER, 'fetch later': STREAM, 'storage later': STORAGE, 'timer later': TIMER });

const KINDS = [['events', 'an event stream', ['message', 'error']], ['bytes', 'a byte stream', ['message', 'error']],
  ['longPoll', 'a long poll', ['message', 'error']], ['upload', 'an upload', ['progress', 'load']]];
for (const [kind, name, types] of KINDS) {
  test(`${name}'s mapper is refused fetch, storage and timers, before its first await and after it, and at the end`, async () => {
    const before = { fetched: fixture.fetched.length, written: fixture.written.length };
    expect(await mapped(kind)).toEqual(types.map(type => refused(type)));
    expect({ fetched: fixture.fetched.length, written: fixture.written.length }).toEqual(before);
    expect(journal.filter(l => l.includes(STORAGE)).length).toBeGreaterThan(0);
  });
}

test("a socket's mapper sends on its own socket while it answers a frame, and is refused fetch, storage and timers throughout", async () => {
  const before = { fetched: fixture.fetched.length, written: fixture.written.length };
  const notAgain = "socket.send is only available in an answer's own turns (its source, its code after an `await` in an async function, its timer), or its socket's own mapper";
  const frame = type => ({ ...refused(type, SOCKET), 'send now': 'ran', 'send later': notAgain });
  expect(await mapped('socket')).toEqual([frame('open'), frame('message'), { ...refused('close'), 'send now': notAgain, 'send later': notAgain }]);
  expect(fixture.sent).toEqual(['reply', 'reply']);
  expect({ fetched: fixture.fetched.length, written: fixture.written.length }).toEqual(before);
});

test("an answer whose steps interleave with a mapper's still fetches and uses storage after its await", async () => {
  const before = { fetched: fixture.fetched.length, written: fixture.written.length };
  const [calls, answered] = await Promise.all([mapped('events'), data.answer('plain', [], new Map()).promise]);
  expect(calls).toEqual([refused('message'), refused('error')]);
  expect(answered).toBe('asset');
  expect({ fetched: fixture.fetched.length - before.fetched, written: fixture.written.slice(before.written) }).toEqual({ fetched: 1, written: ['app:/data/answer.txt'] });
});
