// The generated binding of @exact/clipboard (js/bake/src/__exact_binding.mjs),
// run as data code runs it, its calls answered here: what it refuses before a
// call is sent, call order, and `supports` by target (LLP 1133.006 D2, D7,
// D8 item 1).
import { afterAll, afterEach, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { moduleBinding } from '../../../js/bake/src/__exact_binding.mjs';

const pkg = resolve(import.meta.dir, '..');
const declaration = JSON.parse(readFileSync(join(pkg, 'exact.json'), 'utf8'));
const implementations = ['android', 'apple', 'web', 'windows'].map((p) => JSON.parse(readFileSync(join(pkg, p, 'exact.json'), 'utf8')));
// Each target's binding, written before any is imported (Bun lists a directory once).
const dir = mkdtempSync(join(tmpdir(), 'exact-clipboard-binding-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
for (const target of [null, 'macos', 'ios', 'tvos', 'android', 'web', 'windows']) {
  writeFileSync(join(dir, `clipboard-${target}.ts`), moduleBinding('@exact/clipboard', declaration, implementations, () => [], {}, target));
}
const load = (target) => import(join(dir, `clipboard-${target}.ts`));
const sent = [];
globalThis.__exact_module = (body) => { sent.push(body); return Promise.resolve({}); };
afterEach(() => { sent.length = 0; });
const outcome = (p) => p.then(() => 'ok', (e) => e.name);

test('supports answers the declared types where an implementation is, and none elsewhere', async () => {
  // (Bun runs the binding untyped; js/bake/tests/producer.rs bakes it through tsc for tvOS and iOS.)
  expect((await load('windows')).ClipboardItem.supports('image/png')).toBe(true);
  expect((await load('ios')).ClipboardItem.supports('image/png')).toBe(true);
  expect((await load('web')).ClipboardItem.supports('text/plain')).toBe(true);
  for (const target of [null, 'macos', 'ios', 'web', 'windows', 'tvos', 'android']) {
    expect((await load(target)).ClipboardItem.supports('text/html')).toBe(['macos', 'ios', 'web'].includes(target));
  }
  expect((await load('tvos')).ClipboardItem.supports('text/plain')).toBe(false);
  for (const type of ['text/plain', 'image/png']) expect((await load('android')).ClipboardItem.supports(type)).toBe(false);
});

test('text over 1 MiB refuses before it is sent, as a key and as writeText', async () => {
  const { write, writeText, ClipboardItem } = await load('web');
  expect(await outcome(write([new ClipboardItem({ 'text/plain': 'x'.repeat((1 << 20) + 1) })]))).toBe('DataError');
  expect(await outcome(write([new ClipboardItem({ 'text/plain': Promise.resolve('x'.repeat((1 << 20) + 1)) })]))).toBe('DataError');
  expect(await outcome(writeText('x'.repeat((1 << 20) + 1)))).toBe('DataError');
  expect(await outcome(write([new ClipboardItem({ 'text/html': Promise.resolve('é'.repeat((1 << 19) + 1)) })]))).toBe('DataError');
  expect(sent).toEqual([]);
});

test('a call over the 4 MiB request bound refuses DataError before it is sent', async () => {
  const { write, writeText, ClipboardItem } = await load('web');
  // Each image within its 2 MiB, the two together over the request's 4 MiB as base64.
  const item = () => new ClipboardItem({ 'image/png': new Uint8Array(2 * 1024 * 1024) });
  expect(await outcome(write([item(), item()]))).toBe('DataError');
  // JSON escapes count as the host measures them: 1 MiB of newlines is 2 MiB on the wire beside a 2 MiB image,
  // and 1 MiB of U+0001 is 6 MiB; each is within its own bound before escaping.
  const MiB = 1024 * 1024;
  expect(await outcome(write([new ClipboardItem({ 'text/plain': '\n'.repeat(MiB), 'image/png': new Uint8Array(2 * MiB) })]))).toBe('DataError');
  expect(await outcome(writeText('\u0001'.repeat(MiB)))).toBe('DataError');
  expect(sent).toEqual([]);
});

test('a call refused at once supersedes no pending call; a later call sent first does', async () => {
  const { write, ClipboardItem } = await load('web');
  let resolveA;
  const a = outcome(write([new ClipboardItem({ 'text/plain': new Promise((r) => { resolveA = r; }) })]));
  const refused = outcome(write([new ClipboardItem({ 'image/png': new Uint8Array(2 * 1024 * 1024 + 1) })]));
  resolveA('A');
  expect([await a, await refused]).toEqual(['ok', 'DataError']);
  expect(sent.map((b) => b.args.data[0]['text/plain'])).toEqual(['A']);
  sent.length = 0;
  let resolveC;
  const c = outcome(write([new ClipboardItem({ 'text/plain': new Promise((r) => { resolveC = r; }) })]));
  const d = outcome(write([new ClipboardItem({ 'text/plain': 'D' })]));
  expect(await d).toBe('ok');
  resolveC('C');
  expect(await c).toBe('AbortError');
  expect(sent.map((b) => b.args.data[0]['text/plain'])).toEqual(['D']);
});

test('write takes its platform options after the items and snapshots them at the call', async () => {
  const { write, ClipboardItem } = await load('ios');
  const apple = { localOnly: true, expirationDate: 1800000000000 };
  const pending = write([new ClipboardItem({ 'text/plain': Promise.resolve('note') })], { apple });
  apple.localOnly = false;
  await pending;
  expect(sent[0].args).toEqual({ data: [{ 'text/plain': 'note' }], apple: { localOnly: true, expirationDate: 1800000000000 } });
});

test('Windows sends the path and text as one item through the generated binding', async () => {
  const { write, ClipboardItem } = await load('windows');
  expect(await outcome(write([new ClipboardItem({ 'image/png': 'app:/tmp/capture.png', 'text/plain': 'caption' })]))).toBe('ok');
  expect(sent).toHaveLength(1);
  expect(sent[0].args.data).toEqual([{ 'image/png': 'app:/tmp/capture.png', 'text/plain': 'caption' }]);
});

test('read returns ClipboardItems, omits absent representations and rejects a missing type', async () => {
  const { read, ClipboardItem } = await load('web');
  const before = globalThis.__exact_module;
  globalThis.__exact_module = () => Promise.resolve([{ 'image/png': null, 'text/plain': 'pasted' }]);
  try {
    const [item] = await read();
    expect(item).toBeInstanceOf(ClipboardItem);
    expect(item.types).toEqual(['text/plain']);
    expect(await item.getType('text/plain')).toBe('pasted');
    expect(await outcome(item.getType('image/png'))).toBe('NotFoundError');
  } finally { globalThis.__exact_module = before; }
});

test('native read responses carry a full MiB of escaped plain text', async () => {
  const { read, readText } = await load('ios');
  const text = '\u0001'.repeat(1 << 20);
  const via = globalThis.__exact_module, fetch = globalThis.fetch;
  delete globalThis.__exact_module;
  globalThis.fetch = async (_url, init) => {
    const list = JSON.parse(init.body).send === 'read';
    const json = JSON.stringify(list ? [{ 'text/plain': text }] : text);
    expect(json.length).toBeGreaterThan(6 << 20);
    expect(json.length).toBeLessThanOrEqual(init.exactIndependentHttp.maxResponseBytes);
    return new Response(json);
  };
  try {
    expect(await readText()).toBe(text);
    expect(await (await read())[0].getType('text/plain')).toBe(text);
  } finally { globalThis.fetch = fetch; globalThis.__exact_module = via; }
});
