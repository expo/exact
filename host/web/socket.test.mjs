import { test, expect } from 'bun:test';
import { readSocket, request } from './http-body.js';

// Drive transport events without a scheduler or network: a burst stays in
// the host queue until close flushes it, so order and overflow are exact.
function fakeSocket(run) {
  const original = globalThis.WebSocket;
  let wire;
  globalThis.WebSocket = class {
    readyState = 0; bufferedAmount = 0; sent = [];
    constructor(url, protocols) { wire = this; this.protocols = protocols; this.protocol = ""; }
    send(data) { this.sent.push(data); }
    close(code, reason) { this.readyState = 2; this.closed = [code, reason]; }
  };
  try { return run(() => wire); } finally { globalThis.WebSocket = original; }
}

test('owned protocol socket sends text and bytes, keeps every frame and close detail', async () => {
  let done, handle;
  const messages = [], controller = new AbortController();
  fakeSocket(wire => {
    done = readSocket('ws://example.test', 1024, m => messages.push(m), controller,
      { socket: h => { handle = h; }, admit: () => true });
    expect(handle).toBeDefined();
    expect(() => handle.send('early')).toThrow('not open');
    wire().readyState = 1; wire().onopen();
    handle.send('subscribe'); handle.send(new Uint8Array([1, 2]));
    expect(wire().sent).toEqual(['subscribe', new Uint8Array([1, 2])]);
    for (const data of ['Ack', 'Ping', 'Pong', new Uint8Array([3]).buffer]) wire().onmessage({ data });
    wire().readyState = 3; wire().onclose({ code: 4001, reason: 'finished', wasClean: true });
    expect(messages.map(m => m.event)).toEqual(['open', '', '', '', '']);
    expect(messages.slice(1).map(m => m.data)).toEqual(['Ack', 'Ping', 'Pong', new Uint8Array([3]).buffer]);
    expect(messages.every(m => m.coalesced === 0)).toBe(true);
    expect(handle.readyState).toBe(3);
    expect(() => handle.send('late')).toThrow('not open');
  });
  expect((await done).close).toEqual({ code: 4001, reason: 'finished', wasClean: true });
});

test('owned socket bounds bytes and queued frames, rechecks grants, and aborts by identity', async () => {
  let done, handle, allowed = true;
  const messages = [], controller = new AbortController();
  fakeSocket(wire => {
    done = readSocket('ws://example.test', 1048576, m => messages.push(m), controller,
      { socket: h => { handle = h; }, admit: () => allowed });
    expect(handle).toBeDefined();
    wire().readyState = 1; wire().onopen();
    wire().bufferedAmount = 1048576;
    expect(() => handle.send('é')).toThrow('buffer');
    expect(wire().sent).toEqual([]);
    wire().bufferedAmount = 0; allowed = false;
    expect(() => handle.send('denied')).toThrow('grants');
    allowed = true;
    expect(() => handle.close(1001, '')).toThrow();
    expect(() => handle.close(4000, 'é'.repeat(62))).toThrow();
    for (let i = 0; i < 257; i++) wire().onmessage({ data: String(i) });
    expect(wire().closed).toBeDefined();
    expect(messages.map(m => m.data)).toEqual(['', ...Array.from({ length: 256 }, (_, i) => String(i))]);
    expect(() => handle.send('after overflow')).toThrow('not open');
  });
  const failed = await done;
  expect(failed.kind).toBe(2);
  expect(new TextDecoder().decode(failed.body)).toContain('queue');
  fakeSocket(wire => {
    const owner = new AbortController();
    readSocket('ws://example.test', 64, () => {}, owner, { socket: h => { handle = h; }, admit: () => true });
    wire().readyState = 1; wire().onopen(); owner.abort();
    expect(handle.readyState).toBe(3);
    expect(() => handle.send('disposed')).toThrow('not open');
  });
});

// The queue holds 1 MiB beyond the per-message ceiling, counted in UTF-8:
// 2 MiB + 1 under a 1 MiB ceiling (1.5 MiB + 1 in UTF-16 units); a frame
// between 1 MiB and a 4 MiB ceiling, then exactly 5 MiB, then one byte more.
test.each([
  [1048576, ['é'.repeat(524288), 'x'.repeat(1048576), 'z'], 'queue'],
  [4194304, ['x'.repeat(3145728), 'y'.repeat(2097152), 'z'], 'queue'],
  [3, ['one', new Uint8Array([2]).buffer, new Uint8Array(4).buffer], 'ceiling'],
])('owned receive queue with limit %i flushes accepted frames before refusing overflow', async (limit, frames, reason) => {
  let done;
  const messages = [];
  fakeSocket(wire => {
    done = readSocket('ws://example.test', limit, m => messages.push(m), new AbortController(), { socket() {}, admit: () => true });
    wire().readyState = 1; wire().onopen();
    for (const data of frames) wire().onmessage({ data });
  });
  const outcome = await done;
  expect(messages.slice(1).map(m => m.data)).toEqual(frames.slice(0, -1));
  expect(messages.every(m => m.coalesced === 0)).toBe(true);
  expect(outcome.kind).toBe(2);
  expect(new TextDecoder().decode(outcome.body)).toContain(reason);
});

test('a socket offers protocols in order and reports the selection', async () => {
  let done;
  const messages = [];
  fakeSocket(wire => {
    done = readSocket('ws://example.test', 1024, m => messages.push(m), new AbortController(),
      { socket() {}, admit: () => true }, undefined, ['chat.v2', 'chat.v1']);
    expect(wire().protocols).toEqual(['chat.v2', 'chat.v1']);
    wire().protocol = 'chat.v1'; wire().readyState = 1; wire().onopen();
    wire().onmessage({ data: 'hello' });
    wire().onclose({ code: 1000, reason: '', wasClean: true });
    expect(messages.map(m => m.protocol)).toEqual(['chat.v1', 'chat.v1']);
  });
  await done;
});

test('the host WebSocket constructor accepts case variants and refuses exact duplicates', async () => {
  const source = await Bun.file(new URL('../../vendor/ibex/crates/ibex2/src/bindings/websocket.js', import.meta.url)).text();
  const globals = Object.assign(Object.create(globalThis), { __ibex2_fire_trusted_event() {} });
  const factory = new Function('globalThis', 'return ' + source.slice(source.indexOf('(function')))(globals);
  const offered = [];
  const Socket = factory({ supported: true, open(_socket, _receive, _url, names) { offered.push(names); return 1; } }, () => {});
  expect(() => new Socket('ws://example.test', ['events.v1', 'Events.v1'])).not.toThrow();
  expect(offered).toEqual(['events.v1\nEvents.v1']);
  expect(() => new Socket('ws://example.test', ['events.v1', 'events.v1'])).toThrow('duplicates');
});

test('protocols on a non-socket request are refused before fetching', async () => {
  // The normalized grant also used by module-storage.test.mjs.
  const grantSet = {version:1,entries:[[1,'net.fetch https://api.castle.test',['fetch','https','api.castle.test',443],null]],error:null,seal:'825ec7a8503217b3'};
  const original = globalThis.fetch;
  let hits = 0;
  globalThis.fetch = async () => { hits++; return new Response('discarded protocols'); };
  try {
    for (const stream of [false, true]) {
      const outcome = await request({ method: 'GET', url: 'https://api.castle.test/events', headers: {}, stream, protocols: ['events.v1'] }, { grantSet, controllers: new Set() });
      expect([outcome.kind, new TextDecoder().decode(outcome.body)]).toEqual([2, 'exactProtocols: only a WebSocket stream takes protocols']);
    }
    expect(hits).toBe(0);
  } finally { globalThis.fetch = original; }
});
