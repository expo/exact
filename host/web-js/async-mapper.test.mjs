import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// A stream's mapper that returns a promise on the web JS target (ts-stream.js `open`; Charlie, 2026-11-10): each
// message answers when its mapper's promise settles, a later message's mapper call never overtakes an earlier one's,
// a rejection answers as a throw does, and the stream's end waits behind a call still pending.
import { afterAll, beforeAll, test, expect } from 'bun:test';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = resolve(import.meta.dir, '../..');
let dir, open;
beforeAll(async () => {
  // The runtime's modules side by side, as stream-abort.test.mjs lays them out. The transport is a stub: each `e`
  // of the URL's query is one message, delivered at once, and the body then ends with a 200. With `together`, the
  // messages arrived together: each but the last says `more` (http-body.js).
  dir = mkdtempSync(resolve(tmpdir(), 'exact-ts-async-mapper-'));
  for (const name of ['ts-stream.js', 'ts-fetch.js', 'ts-body-from.js', 'shape.js', 'uploads.js']) cpSync(resolve(root, 'host/web-js', name), resolve(dir, name));
  for (const name of ['grant-admission.js', 'faults.js', 'address-space.js', 'navigation.js', 'storage-environment.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(root, 'host/web', name)));
  writeFileSync(resolve(dir, 'http-body.js'), `export async function streamed(req, grantSet, message) {
  const q = new URL(req.url).searchParams, all = [...q.getAll('e'), ...Array(Number(q.get('n') ?? 0)).fill('x')];
  const send = (data, i) => message({ event: '', id: '', data, coalesced: 0, ...(q.has('together') && i < all.length - 1 ? { more: true } : {}) });
  if (q.has('pull')) { for (const [i, data] of all.entries()) await send(data, i); }
  else all.forEach(send);
  return Promise.resolve({ status: 200, body: new Uint8Array() });
}\n`);
  writeFileSync(resolve(dir, 'admission.js'), readFileSync(resolve(root, 'host/web-js/admission.js'), 'utf8').replaceAll("'../web/", "'./"));
  writeFileSync(resolve(dir, 'admission-data.js'), "import { createGrantSet } from './admission.js'; export const tsGrantSet = createGrantSet(null);\n");
  writeFileSync(resolve(dir, 'rt.js'), 'export const clock={agent:false,now:0},journal=[],Resources=[],inflight={n:0};export const drive=()=>{},Origin={now:()=>"app"};export const checkpoint=()=>({kept:null});export const commit=f=>f();export const R=()=>{};export const painted=()=>Promise.resolve();\n');
  writeFileSync(resolve(dir, 'rt-link.js'), "import * as rt from './rt.js'; export { rt };\n");
  writeFileSync(resolve(dir, 'names.js'), 'export const sourceTypes = {};\n');
  ({ open } = await import(pathToFileURL(resolve(dir, 'ts-stream.js')).href));
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

// A message `<ms> <label>`: the mapper notes its start, waits that long, and answers with what it has noted; the
// label `reject` rejects instead. The end waits a little too. What each message answered, and the end.
async function run(events, together = false) {
  const seen = [], delivered = [];
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const map = async e => {
    if (e.type !== 'message') { await wait(5); seen.push(e.type); return seen.join(' '); }
    const [ms, label] = e.data.split(' ');
    seen.push('start ' + label);
    await wait(Number(ms));
    if (label === 'reject') throw new Error('rejected');
    seen.push('end ' + label);
    return seen.join(' ');
  };
  const url = 'https://example.test/?' + events.map(e => 'e=' + encodeURIComponent(e)).join('&') + (together ? '&together' : '');
  const counts = [];
  const end = await open({ input: url, init: { exactStream: map } }, v => v, null, o => { counts.push(o.messages); delivered.push(o.error ? 'error ' + o.error : o.v); }, new AbortController());
  return { delivered, counts, end: end.error ? 'error ' + end.error : end.v };
}

test("a later message's mapper call waits for an earlier one's promise, and the end waits behind both", async () => {
  const { delivered, end } = await run(['30 a', '0 b', '5 c']);
  expect(delivered).toEqual(['start a end a', 'start a end a start b end b', 'start a end a start b end b start c end c']);
  expect(end).toBe('start a end a start b end b start c end c error');
});

test('a rejection answers as a throw does, and the next message is still mapped after it', async () => {
  const { delivered } = await run(['10 reject', '0 b']);
  expect(delivered).toEqual(['error rejected', 'start reject start b end b']);
});

test('a mapper that answers at once still answers in its own turn', async () => {
  const delivered = [];
  const map = e => e.type === 'message' ? 'now ' + e.data : 'end';
  const ended = open({ input: 'https://example.test/?e=x&e=y', init: { exactStream: map } }, v => v, null, o => delivered.push(o.v), new AbortController());
  expect(delivered).toEqual(['now x', 'now y']);
  expect((await ended).v).toBe('end');
});

// LLP 1016.000 D4: messages that arrived together each reach the mapper, in order, each call after the last one's
// promise, and only the last value is delivered, once, counting them all; a rejection ends them there.
test('messages that arrive together are each mapped in order and deliver once, as the last', async () => {
  const delivered = [], seen = [];
  const map = e => e.type === 'message' ? (seen.push(e.data), seen.join('')) : 'end';
  const ended = open({ input: 'https://example.test/?e=a&e=b&e=c&together', init: { exactStream: map } }, v => v, null, o => delivered.push([o.v, o.messages]), new AbortController());
  expect(delivered).toEqual([['abc', 3]]);
  expect((await ended).v).toBe('end');
  const awaited = await run(['30 a', '0 b', '5 c'], true);
  expect(awaited.delivered).toEqual(['start a end a start b end b start c end c']);
  expect(awaited.counts).toEqual([3]);
  expect((await run(['10 reject', '0 b'], true)).delivered).toEqual(['error rejected']);
});

// A read of any number of events maps them all with no stack to spare; behind a mapper that awaits at most 4096
// messages wait, as natively, and one more ends the stream.
test('a read of 100,000 events maps every one, and 4097 behind a mapper that awaits end the stream', async () => {
  const delivered = [];
  let seen = 0;
  const map = e => e.type === 'message' ? ++seen : 'end';
  const ended = open({ input: 'https://example.test/?n=100000&together', init: { exactStream: map } }, v => v, null, o => delivered.push([o.v, o.messages]), new AbortController());
  expect(delivered).toEqual([[100000, 100000]]);
  expect((await ended).v).toBe('end');
  const behind = [];
  let release;
  const waits = new Promise(r => { release = r; });
  const awaiting = e => e.type === 'message' && e.data === 'first' ? waits.then(() => 'first') : e.data;
  const done = open({ input: 'https://example.test/?e=first&n=4097', init: { exactStream: awaiting } }, v => v, null, o => behind.push(o.error ?? o.v), new AbortController());
  expect(behind).toEqual(["the stream's mapper fell behind: 4096 messages (4096 bytes) waited for its turns, and 1 more arrived"]);
  release();
  await done;
});

test('a pull-driven byte transport waits for the mapper promise', async () => {
  const { promise, resolve: release } = Promise.withResolvers(), seen = [];
  const map = e => {
    seen.push(e.data);
    return e.data === 'a' ? promise : e.data;
  };
  const done = open({ input: 'https://example.test/?e=a&e=b&pull', init: { exactStream: map } }, v => v, null, () => {}, new AbortController());
  expect(seen).toEqual(['a']);
  release('a');
  await done;
  expect(seen.slice(0, 2)).toEqual(['a', 'b']);
});
