import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// A TypeScript answer on the web JS target is what a native host makes of it: the text `JSON.stringify` writes
// (js/src/binary.js), decoded by the shape (js/value's `decode_tree`). So each answer here must convert, and be
// refused, exactly as its own JSON round trip does, and what it converts to must pass the runtime's shape check
// (rt.js `take`), which sees no `Date`, `NaN` or `-0`.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = resolve(import.meta.dir, '../..');
// Each source's plan type and its type code (shape.js `conforms`).
const TYPES = {
  row: [{ at: 's', n: ['?', 'n'] }, '{s?n}'],
  rows: [['[', { at: 's', n: ['?', 'n'] }], '[{s?n}'],
  numbers: [['[', ['?', 'n']], '[?n'],
  strings: [['[', 's'], '[s'],
};
let dir, data, conforms;
beforeAll(async () => {
  // ts-data.js beside the runtime's modules, as request-refusal.test.mjs lays it out; the app answers
  // `globalThis.answering`, now, through a promise, or as one mutable stream's successive snapshots.
  dir = mkdtempSync(resolve(tmpdir(), 'exact-ts-answer-json-'));
  const app = resolve(dir, 'source.js');
  writeFileSync(app, `import { fetch as streamFetch } from './ts-fetch.js';
export const appId = 'test.answer.json', grants = '';
export function answer(name) {
  if (name === 'rows-stream') return streamFetch('https://example.test/', {
    exactStream(event) {
      if (event.type === 'message') globalThis.answering[0].n = Number(event.data);
      return globalThis.answering;
    },
  });
  return name.endsWith('later') ? Promise.resolve(globalThis.answering) : globalThis.answering;
}
`);
  writeFileSync(resolve(dir, 'ts-data.js'), readFileSync(resolve(root, 'host/web-js/ts-data.js'), 'utf8')
    .replace('__APP_TS__', pathToFileURL(app).href).replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '').replace("'__FETCH_MODULE__'", "'./ts-fetch.js'"));
  for (const name of ['ts-fetch.js', 'ts-stream.js', 'ts-body-from.js', 'shape.js', 'uploads.js']) cpSync(resolve(root, 'host/web-js', name), resolve(dir, name));
  for (const name of ['grant-admission.js', 'faults.js', 'address-space.js', 'navigation.js', 'storage-environment.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(root, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission.js'), readFileSync(resolve(root, 'host/web-js/admission.js'), 'utf8').replaceAll("'../web/", "'./"));
  writeFileSync(resolve(dir, 'admission-data.js'), "import { createGrantSet } from './admission.js'; export const tsGrantSet = createGrantSet(null);\n");
  writeFileSync(resolve(dir, 'rt.js'), 'export const clock={agent:false,now:0},journal=[],Resources=[],inflight={n:0};export const drive=()=>{},time=()=>{},Origin={now:()=>"app"};export const checkpoint=()=>({kept:null});export const commit=f=>f();export const R=()=>{};export const painted=()=>Promise.resolve();export const canvasCommitted=()=>Promise.resolve();\n');
  writeFileSync(resolve(dir, 'rt-link.js'), "import * as rt from './rt.js'; export { rt };\n");
  writeFileSync(resolve(dir, 'http-body.js'), `export async function streamed(req, grants, message) {
  for (const data of ['1', '2', '3']) message({ event: '', id: '', data, coalesced: 0 });
  return { status: 200, body: new Uint8Array() };
}
`);
  const types = Object.fromEntries(Object.entries(TYPES).flatMap(([name, [t]]) => [[name, [[], t]], [`${name}-later`, [[], t]]]));
  types['rows-stream'] = [[], TYPES.rows[0]];
  writeFileSync(resolve(dir, 'names.js'), `export const sourceTypes = ${JSON.stringify(types)};\n`);
  const ts = await import(pathToFileURL(resolve(dir, 'ts-data.js')).href);
  ({ conforms } = await import(pathToFileURL(resolve(dir, 'shape.js')).href));
  data = { q: [] };
  ts.install(data);
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

// What `source` answering `value` converts to, or the refusal's message; each target is its own, so no answer is
// converted against another's.
let asked = 0;
function web(source, value) {
  globalThis.answering = value;
  try { return { v: data.answer(source, [], new Map(), `${source} ${++asked}`).v }; } catch (e) { return { refused: e.message }; }
}
async function webLater(source, value) {
  globalThis.answering = value;
  try { return { v: await data.answer(`${source}-later`, [], new Map(), `${source} ${++asked}`).promise }; }
  catch (e) { return { refused: e.message.replace(`\`${source}-later\``, `\`${source}\``) }; }
}
// The native answer: the same source answering the text JSON writes, read back.
const native = (source, value) => web(source, JSON.parse(JSON.stringify(value)));

// Holds `value` to the native result, now and through a promise, and returns that result.
async function asNative(source, value) {
  const now = web(source, value), expected = native(source, value);
  expect(now).toEqual(expected);
  expect(await webLater(source, value)).toEqual(expected);
  if (now.v !== undefined) expect(conforms(now.v, TYPES[source][1])).toBe(true);
  return now;
}

test('a Date where the shape says string is its ISO text', async () => {
  expect(await asNative('row', { at: new Date(0), n: 1 })).toEqual({ v: ['1970-01-01T00:00:00.000Z', 1] });
  expect((await asNative('rows', [{ at: new Date(0), n: null }, { at: 'b', n: 2 }])).v).toEqual([['1970-01-01T00:00:00.000Z', null], ['b', 2]]);
});

test('a non-finite number in an option is none, and -0 is 0', async () => {
  expect(await asNative('row', { at: 'a', n: NaN })).toEqual({ v: ['a', null] });
  const { v } = await asNative('numbers', [1, Infinity, -Infinity, NaN, -0]);
  expect(v).toEqual([1, null, null, null, 0]);
  expect(Object.is(v[4], 0)).toBe(true);
  expect(Object.is(web('row', { at: 'a', n: -0 }).v[1], 0)).toBe(true);
});

test('a toJSON hook is called with its key, and its value is what the shape checks', async () => {
  class Money { constructor(cents) { this.cents = cents; } toJSON() { return this.cents / 100; } }
  expect(await asNative('row', { at: 'a', n: new Money(250) })).toEqual({ v: ['a', 2.5] });
  const keyed = { toJSON: key => `key ${key}` };
  expect(await asNative('row', { at: keyed, n: null })).toEqual({ v: ['key at', null] });
  expect(await asNative('strings', [keyed, keyed])).toEqual({ v: ['key 0', 'key 1'] });
  // The whole answer's hook, with the key '' JSON gives it.
  expect(await asNative('row', { toJSON: key => ({ at: `root ${JSON.stringify(key)}`, n: 1 }) })).toEqual({ v: ['root ""', 1] });
  // Boxed primitives are their values, as JSON writes them.
  expect(await asNative('row', { at: new String('s'), n: new Number(3) })).toEqual({ v: ['s', 3] });
});

test('what JSON leaves out is not there, and a value outside the shape is refused as natively', async () => {
  expect(await asNative('row', { at: 'a', n: 1, later: undefined, f() {}, [Symbol('s')]: 1 })).toEqual({ v: ['a', 1] });
  expect(await asNative('numbers', [undefined, () => 1, 2])).toEqual({ v: [null, null, 2] });
  const missing = '`row` answered outside its shape: field `n` is missing';
  expect(await asNative('row', { at: 'a', n: undefined })).toEqual({ refused: missing });
  expect(await asNative('row', { at: 'a', n: { toJSON: () => undefined } })).toEqual({ refused: missing });
  expect(await asNative('row', { at: 'a', n: new Date(0) })).toEqual({ refused: '`row` answered outside its shape: field `n`: expected a number, got a string' });
  expect(await asNative('row', { at: 'a', n: 1, extra: new Date(0) })).toEqual({ refused: '`row` answered outside its shape: field `extra` is not in the shape' });
  expect(await asNative('rows', [{ at: 'a', n: 1 }, { at: 1, n: 1 }])).toEqual({ refused: '`rows` answered outside its shape: field `at`: expected a string, got a number' });
});
test('a stream may return the same mutable list and rows for successive snapshots', async () => {
  const rows = [{ at: 'a', n: 0 }, { at: 'b', n: 10 }];
  globalThis.answering = rows;
  const values = [], answer = data.answer('rows-stream', [], new Map(), 'mutable stream');
  const ended = await answer.stream(o => values.push(o.v), new AbortController());
  expect(values).toEqual([[['a', 1], ['b', 10]], [['a', 2], ['b', 10]], [['a', 3], ['b', 10]]]);
  expect(ended.v).toEqual([['a', 3], ['b', 10]]);
  expect(values[0][1] === values[1][1] && values[1][1] === values[2][1]).toBe(true);
  rows[0].n = 99;
  expect(values[2][0][1]).toBe(3);
});
