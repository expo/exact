import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// A declared source on the JS target (fetches.js, LLP 1162.001) asks as whoever asked it: a tool's, a wake's or a
// package event's request to a less public space than the page's is refused before it is sent (LLP 1133.036 D6), as
// a TypeScript source's is. And its request counts as I/O out until it settles, so a render waits for it (held.js).
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = resolve(import.meta.dir, '../..');
// `net.fetch http://127.0.0.1:9`, as the compiler normalizes it (exact-web-js `normalize-grants`).
const GRANT = { entries: [[1, 'net.fetch http://127.0.0.1:9', ['fetch', 'http', '127.0.0.1', 9], null]], error: null, keepsNoAnswers: false, seal: 'e1b17172201a49fa', version: 1 };
const ROW = ['GET', 'http://127.0.0.1:9/forecast?q={q}', 'http://127.0.0.1:9', ['q'], [], '', '', '/current', ['s'], { temperature_2m: 'n' }, GRANT];
let dir, data, sent = [], answers = [];
beforeAll(async () => {
  dir = mkdtempSync(resolve(tmpdir(), 'exact-fetches-'));
  for (const name of ['fetches.js', 'shape.js']) cpSync(resolve(root, 'host/web-js', name), resolve(dir, name));
  mkdirSync(resolve(dir, 'navigation'));
  writeFileSync(resolve(dir, 'navigation/network.js'), hostFixtureFile(resolve(root, 'host/web/navigation/network.js')));
  for (const name of ['grant-admission.js', 'faults.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(root, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission.js'), readFileSync(resolve(root, 'host/web-js/admission.js'), 'utf8').replaceAll("'../web/", "'./"));
  writeFileSync(resolve(dir, 'rt.js'), "export const data = {}; export const Store = { get: () => null }; export const Origin = { now: () => globalThis.asker ?? 'app' };\n");
  writeFileSync(resolve(dir, 'rt-link.js'), "import * as rt from './rt.js'; export { rt };\n");
  // The browser's fetch, which admission binds as it loads: each request recorded, its answer the test's to give.
  globalThis.fetch = (url, init) => { sent.push([String(url), init.redirect]); return new Promise(resolve => answers.push(resolve)); };
  ({ data } = await import(pathToFileURL(resolve(dir, 'rt.js')).href));
  (await import(pathToFileURL(resolve(dir, 'fetches.js')).href)).fetches({ weather: ROW });
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

const settle = () => new Promise(resolve => setTimeout(resolve, 10));

test('the app asks a loopback endpoint; a tool asking it is refused before it is sent', async () => {
  globalThis.asker = 'app';
  const asked = data.reserved.weather(null, ['a b'], 'now', false);
  await settle();
  expect(sent).toEqual([['http://127.0.0.1:9/forecast?q=a%20b', 'follow']]);
  // Out until it settles, as a TypeScript source's fetch is (held.js `answersAtWork`).
  expect(asked.call.plain).toBe(1);
  answers.shift()(Response.json({ current: { temperature_2m: 12.5, time: 'now' } }));
  expect(await asked).toEqual([12.5]);
  expect(asked.call.plain).toBe(0);

  globalThis.asker = 'tool';
  const refused = data.reserved.weather(null, ['a'], 'now', false);
  const error = await refused.catch(e => e);
  expect([error.name, error.kind, String(error.message)]).toEqual(['FetchError', 'Refused', 'NotAllowedError: person-required']);
  expect(sent.length).toBe(1);
  expect(refused.call.plain).toBe(0);
});
