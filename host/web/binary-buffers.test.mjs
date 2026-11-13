// js/src/binary.js's BufferSource reads in a realm whose engine has shared, resizable and detachable buffers, as a
// browser's does: the wasm target's module realms run the same prelude, and Hermes has none of these buffers.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { createContext, runInContext } from 'node:vm';

const binary = readFileSync(new URL('../../js/src/binary.js', import.meta.url), 'utf8');
const prelude = readFileSync(new URL('../../js/src/prelude.js', import.meta.url), 'utf8');

function realm() {
  const context = createContext({ Headers, URL, URLSearchParams, TextEncoder, TextDecoder });
  runInContext(binary, context);
  runInContext('globalThis.said = f => { try { return f(); } catch (e) { return `${e.name}: ${e.message}`; } };', context);
  return context;
}

test('web fetches keep byte bodies on the string door as base64 copies', () => {
  const context = realm(), requests = [];
  context.crypto = crypto;
  context.__exact_host = (op, ticket, wire) => {
    if (op === 1) requests.push({ ticket, ...JSON.parse(wire) });
  };
  runInContext(prelude, context);
  runInContext(`
    globalThis.exact = { answer() {
      const bytes = new Uint8Array([9, 0, 255, 128, 9]);
      const bodies = [new ArrayBuffer(0), bytes.subarray(1, 1), bytes.subarray(1, 2), bytes.subarray(1, 3), new DataView(bytes.buffer, 1, 3), bytes.buffer, 'text'];
      const requests = bodies.map(body => fetch('https://upload.test', { method: 'POST', body }));
      bytes.fill(7);
      return Promise.all(requests);
    } };
    __exact_call('upload', '[]', 'resource');
  `, context);
  expect(requests.map(r => r.ticket)).toEqual(['1', '2', '3', '4', '5', '6', '7']);
  expect(requests.map(r => r.body_base64)).toEqual(['', '', 'AA==', 'AP8=', 'AP+A', 'CQD/gAk=', undefined]);
  expect(requests.map(r => r.body)).toEqual(['', '', '', '', '', '', 'text']);
});

test('a body on a detached buffer is no bytes, its view\'s range unread, and the init is still converted', async () => {
  const [order, status, text] = await runInContext(`(async () => {
    const { Response } = __exact_binary.responses(), order = [];
    const buffer = new ArrayBuffer(4), view = new DataView(buffer, 1, 2);
    buffer.transfer();
    const response = new Response(view, { get status() { order.push('status'); return 201; } });
    return [order, response.status, await response.text()];
  })()`, realm());
  expect([order, status, text]).toEqual([['status'], 201, '']);
});

test('a body on a SharedArrayBuffer is refused before the init is read', () => {
  const [refused, order] = runInContext(`(() => {
    const { Response } = __exact_binary.responses(), order = [];
    return [said(() => new Response(new Uint8Array(new SharedArrayBuffer(1)), { get status() { order.push('status'); return 200; } })), order];
  })()`, realm());
  expect([refused, order]).toEqual(['TypeError: new Response(): data on a SharedArrayBuffer is not a BufferSource', []]);
});

test('copyBytes, as crypto.subtle takes its data, refuses a shared or resizable buffer and gives a detached one no bytes', () => {
  const copied = runInContext(`(() => {
    const copy = data => Array.from(new Uint8Array(__exact_binary.copyBytes(data, 'crypto.subtle.digest()')));
    const detached = new ArrayBuffer(2), detachedView = new DataView(detached);
    detached.transfer();
    return [said(() => copy(new Uint8Array(new SharedArrayBuffer(1)))), said(() => copy(new Uint8Array(new ArrayBuffer(2, { maxByteLength: 4 })))),
      said(() => copy(detachedView)), said(() => copy(detached)), said(() => copy(new Uint8Array([1, 2, 3]).subarray(1)))];
  })()`, realm());
  expect(copied).toEqual([
    'TypeError: crypto.subtle.digest(): data on a SharedArrayBuffer is not a BufferSource',
    'TypeError: crypto.subtle.digest(): data on a resizable ArrayBuffer is not a BufferSource',
    [], [], [2, 3],
  ]);
});

test('a browser realm collects Response stream bodies and preserves their read and clone state', async () => {
  const context = createContext({ Headers, URL, URLSearchParams, TextEncoder, TextDecoder, Response, ReadableStream });
  runInContext(binary, context);
  const result = await runInContext(`(async () => {
    const { Response } = __exact_binary.responses();
    const stream = new ReadableStream({ start(c) { c.enqueue(new TextEncoder().encode('hello')); c.close(); } });
    const response = new Response(stream), copy = response.clone();
    const reading = response.text(), used = [response.bodyUsed, copy.bodyUsed];
    const bytes = Array.from(new Uint8Array(await copy.arrayBuffer()));
    const text = await reading;
    const disturbed = new ReadableStream({ start(c) { c.close(); } });
    await disturbed.cancel();
    let refused;
    try { new Response(disturbed); } catch (e) { refused = e.name; }
    return [text, bytes, used, refused, response.headers.get('content-type')];
  })()`, context);
  expect(result).toEqual(['hello', [104, 101, 108, 108, 111], [true, false], 'TypeError', null]);
});
