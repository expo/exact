import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// A Rust source's two-way socket on the web JS target (LLP 1016.003): rust-data.js's half of the module seam, against
// a stand-in for the app's module wasm and a real WebSocket peer. The stand-in plays runner/tests/it/rust_sockets.rs's
// source: its mapper subscribes on `open` and answers each frame, a mutation sends on the handle it kept, and the
// module's own checks (a handle's socket open in the turn, D7's bounds) are the seam's, as `Socket::send` makes them.
// This test plays rt.js: a parse's or an answer's effects run when its commit stands, or are refused.
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { webCompiler } from './module.mjs';

const ROOT = resolve(import.meta.dir, '../..');

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

// The stand-in module: it decodes each call as `exact-logic-abi` lays it out and answers as a Rust source would.
function standIn(url) {
  const memory = new WebAssembly.Memory({ initial: 4 }), out = 1 << 17;
  let output = new Uint8Array();
  const kept = new Map(), turns = [];
  const reply = fill => {
    const bytes = [], u8 = v => bytes.push(v), u16 = v => bytes.push(v & 255, v >>> 8 & 255), u32 = v => bytes.push(v & 255, v >>> 8 & 255, v >>> 16 & 255, v >>> 24 & 255);
    const string = v => { const e = new TextEncoder().encode(v); u32(e.length); bytes.push(...e); };
    u32(9); fill({ u8, u16, u32, string, bytes }); output = Uint8Array.from(bytes); new Uint8Array(memory.buffer, out, output.length).set(output);
  };
  // A call (`call_request`) as a Rust source receives it.
  const decode = at => {
    const view = new DataView(memory.buffer), u8 = () => view.getUint8(at++), u16 = () => { const n = view.getUint16(at, true); at += 2; return n; };
    const u32 = () => { const n = view.getUint32(at, true); at += 4; return n; }, u64 = () => { const n = Number(view.getBigUint64(at, true)); at += 8; return n; };
    const raw = () => { const n = u32(), b = new Uint8Array(memory.buffer, at, n).slice(); at += n; return b; }, string = () => new TextDecoder().decode(raw());
    const code = u8(), source = string();
    u8(); for (let n = u32(); n--;) throw new Error('the stand-in takes no arguments');
    for (let n = u32(); n--;) string();
    for (let n = u32(); n--;) { string(); string(); }
    let outcome = null;
    if (code === 4) {
      const tag = u8();
      if (tag === 8 || tag === 12) outcome = { event: string(), id: string(), data: tag === 8 ? string() : [...raw()], coalesced: u32(), protocol: string() };
      else if (tag === 11) outcome = { closed: u16(), reason: string(), wasClean: u8() === 1 };
      else if ([1, 2, 3, 4, 9, 10].includes(tag)) outcome = { failed: tag, message: string() };
      else throw new Error(`the stand-in reads no outcome ${tag}`);
    }
    const kind = u8(), own = kind === 2 ? { id: u32(), ticket: u64() } : null;
    const open = []; for (let n = u32(); n--;) open.push({ id: u32(), ticket: u64(), buffered: u64() });
    return { code, source, outcome, kind, own, open };
  };
  const exports = {
    memory, exact_logic_abi: () => 9, exact_logic_create: () => 1, exact_logic_alloc: () => 0, exact_logic_dealloc() {},
    exact_logic_output: () => out, exact_logic_output_len: () => output.length,
    exact_logic_call(_session, pointer) {
      const code = new Uint8Array(memory.buffer)[pointer + 4];
      if (code === 0) { reply(w => { w.u8(0); w.string('test.rust'); w.string(`net.websocket ${url}`); }); return 0; }
      if (code === 1 || code === 2) { reply(w => { w.u8(0); w.u8(0); w.u8(3); }); return 0; }
      const call = decode(pointer + 4);
      turns.push(call);
      const ops = [];
      let overload = '', result;
      // `Socket::send`'s checks: the turn may send on it, and it is open in the turn.
      const send = (handle, op) => {
        const mine = call.kind === 1 || call.kind === 2 && call.own.id === handle.id && call.own.ticket === handle.ticket;
        if (!mine) return 'InvalidStateError: a socket sends only from a mutation\'s answer or its own mapper';
        if (!call.open.some(s => s.id === handle.id && s.ticket === handle.ticket)) return 'InvalidStateError: the socket is not open';
        if (ops.length >= 256) { if (call.kind === 2) overload = 'QuotaExceededError: socket mapper call exceeds 256 operations'; return overload; }
        ops.push([handle.id, op]);
        return null;
      };
      const failed = why => ({ error: why });
      if (call.code === 3 && ['book', 'other'].includes(call.source)) result = { socket: `${url}/${call.source}` };
      else if (call.code === 3) {
        // A mutation's sends on the kept handle: `send-*` a text frame, `binary` one of bytes, `close` its close.
        const [verb, arg] = call.source.split(':'), handle = kept.get(arg ?? 'book');
        const op = verb === 'binary' ? [1, [1, 2, 3]] : verb === 'close' ? [2, 3001, 'bye'] : [0, `${verb} ${arg ?? 'book'}`];
        const why = handle ? send(handle, op) : 'InvalidStateError: the socket is not open';
        result = why ? failed(why) : { value: 'sent' };
      } else if (call.outcome?.event === 'open') {
        if (kept.has(call.source)) kept.set(`${call.source}-old`, kept.get(call.source));
        kept.set(call.source, call.own);
        const why = send(call.own, [0, `SUBSCRIBE ${call.source}`]);
        result = why ? failed(why) : { value: 'open' };
      } else if (call.outcome?.closed) result = { value: `closed ${call.outcome.closed} ${call.outcome.reason}` };
      else if (call.outcome?.data === 'echo: flood book') {
        // Past D7's 256 operations: the frame fails, whatever the mapper answers.
        for (let i = 0; i < 257 && !overload; i++) send(call.own, [0, 'x']);
        result = { value: 'flooded' };
      } else if (call.outcome?.event !== undefined) result = { value: Array.isArray(call.outcome.data) ? `bytes ${call.outcome.data.join(',')}` : call.outcome.data };
      else result = { value: 'ended' };
      reply(w => {
        w.u8(2); w.u8(0); w.u32(0);
        w.u32(ops.length);
        for (const [id, op] of ops) {
          w.u32(id); w.u8(op[0]);
          if (op[0] === 0) w.string(op[1]); else if (op[0] === 1) { w.u32(op[1].length); w.bytes.push(...op[1]); } else { w.u16(op[1]); w.string(op[2]); }
        }
        w.u8(overload ? 1 : 0); w.string(overload);
        if (result.socket) { w.u8(11); w.u32(1 << 20); w.u8(0); w.string(''); w.string('GET'); w.string(result.socket); w.u32(0); w.u32(0); w.u32(0); }
        else if (result.error) { w.u8(4); w.string(result.error); }
        else { w.u8(0); w.u8(2); w.string(result.value); }
      });
      return 0;
    },
  };
  return { exports, turns };
}

test("a Rust source's socket on the JS target subscribes on open, sends from a mutation, and keeps D7's rules", async () => {
  const heard = new Map(), closes = [], peers = new Map();
  const peer = Bun.serve({ hostname: '127.0.0.1', port: 0,
    fetch(req, server) { return server.upgrade(req, { data: { path: new URL(req.url).pathname } }) ? undefined : new Response('websocket only'); },
    websocket: {
      open(ws) { peers.set(ws.data.path, ws); },
      // Each frame is echoed; a text frame is kept by the path it came on.
      message(ws, data) {
        const said = typeof data === 'string' ? data : `bytes ${[...data].join(',')}`;
        heard.set(ws.data.path, [...heard.get(ws.data.path) ?? [], said]);
        ws.send(typeof data === 'string' ? `echo: ${data}` : data);
      },
      close(ws, code, reason) { closes.push([ws.data.path, code, reason]); },
    },
  });
  const url = `ws://127.0.0.1:${peer.port}`;
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-rust-socket-'));
  const instantiate = WebAssembly.instantiate;
  try {
    for (const name of ['rust-data.js', 'admission.js']) {
      const source = readFileSync(resolve(ROOT, 'host/web-js', name), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'");
      writeFileSync(resolve(dir, name), source);
    }
    for (const name of ['http-body.js', 'grant-admission.js', 'faults.js', 'navigation.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
    writeFileSync(resolve(dir, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const rustGrantSet=createGrantSet(${JSON.stringify(normalized(`net.websocket ${url}`))}),tsGrantSet=createGrantSet(${JSON.stringify(normalized(''))});\n`);
    writeFileSync(resolve(dir, 'rt-link.js'), 'export const rt = { journal: [], clock: { now: 0 } };\n');
    const { exports, turns } = standIn(url);
    WebAssembly.instantiate = async () => ({ instance: { exports } });
    const { rt } = await import(pathToFileURL(resolve(dir, 'rt-link.js')).href);
    const module = await import(pathToFileURL(resolve(dir, 'rust-data.js')).href);
    const data = { q: [] }, store = { map: new Map(), set() {} };
    await module.install(data, {}, async () => new ArrayBuffer(0));

    // rt.js's part: a resource's ticket, each message parsed in a commit that stands (or is refused), its effects
    // after; what the resource shows.
    const shown = {};
    const until = (what, ok, deadline = Date.now() + 5000) => new Promise((done, fail) => {
      const look = () => ok() ? done() : Date.now() > deadline
        ? fail(new Error(`timed out waiting for ${what}: ${JSON.stringify({ shown, heard: [...heard], journal: rt.journal })}`)) : setTimeout(look, 2);
      look();
    });
    let tickets = 0;
    const ask = (name, owner = {}) => {
      const a = data.answer(name, [], store, name, false);
      expect(a.req).toMatchObject({ socket: true, stream: true, url: `${url}/${name}` });
      const ctl = Object.assign(new AbortController(), { resource: true, owner, ticket: ++tickets, streamName: name, active: () => true });
      // (rt.js sends it as the asking commit's origin, resources.js)
      data.fetch({ ...a.req, origin: 'app' }, m => {
        try { const p = data.parse(name, [], { streamed: [m] }, store); shown[name] = p.v; for (const go of p.effects ?? []) go(); }
        catch (e) { shown[name] = `failed: ${e.message}`; }
      }, ctl).then(o => { shown[name] = data.parse(name, [], o, store).v; });
      return { ctl, owner };
    };
    const send = (source, stands = true) => {
      const a = data.answer(source, [], store, source, true);
      for (const go of a.effects ?? []) stands ? go() : go.refused('the commit was refused');
      return a.v;
    };
    const said = path => heard.get(path) ?? [];

    // Subscribe on `open`: the mapper's reply goes once its frame's commit stands, and the echo answers.
    const book = ask('book');
    await until('the subscription echoed', () => shown.book === 'echo: SUBSCRIBE book');
    const open = turns.find(t => t.outcome?.event === 'open');
    expect(open).toMatchObject({ code: 4, kind: 2, own: { id: 0, ticket: 1 }, open: [{ id: 0, ticket: 1, buffered: 0 }] });
    expect(rt.journal).toContain('t=0 socket send: 14 bytes (book)');
    // A mutation sends on the handle it kept; a refused commit sends nothing, and says so.
    expect(send('hello')).toBe('sent');
    await until('the mutation echoed', () => shown.book === 'echo: hello book');
    expect(send('refused', false)).toBe('sent');
    expect(rt.journal.at(-1)).toBe('t=0 send refused failed: the commit was refused; socket frames were refused');
    // A binary frame each way.
    send('binary');
    await until('the bytes echoed', () => shown.book === 'bytes 1,2,3');
    // Two resources at one URL stay apart: each subscribes on its own socket.
    ask('other');
    await until('the other subscription echoed', () => shown.other === 'echo: SUBSCRIBE other');
    send('hello:other');
    await until('the other mutation echoed', () => shown.other === 'echo: hello other');
    expect(said('/book')).toEqual(['SUBSCRIBE book', 'hello book', 'bytes 1,2,3']);
    expect(said('/other')).toEqual(['SUBSCRIBE other', 'hello other']);
    // Past D7's bounds a mapper's frame fails, whatever it answered, and its frames are refused.
    send('flood');
    await until('the flooded frame failed', () => shown.book?.startsWith?.('failed:'));
    expect(shown.book).toBe('failed: QuotaExceededError: socket mapper call exceeds 256 operations');
    expect(said('/book')).toEqual(['SUBSCRIBE book', 'hello book', 'bytes 1,2,3', 'flood book']);
    // The resource asked again: its old handle is stale, never sent on the newer socket.
    book.ctl.abort();
    ask('book', book.owner);
    await until('the new subscription', () => said('/book').length === 5);
    expect(turns.findLast(t => t.outcome?.event === 'open').own).toEqual({ id: 0, ticket: 3 });
    expect(() => send('hello:book-old')).toThrow('InvalidStateError: the socket is not open');
    expect(turns.at(-1)).toMatchObject({ kind: 1, own: null });
    expect(turns.at(-1).open.map(s => [s.id, s.ticket]).sort()).toEqual([[0, 3], [1, 2]]);
    // A close the mutation staged ends the stream with its code, which the mapper answers.
    send('close');
    await until('the close', () => shown.book === 'closed 3001 bye');
    await until('the peer heard the close', () => closes.length === 2);
    expect(closes).toEqual([['/book', 1000, ''], ['/book', 3001, 'bye']]);
    expect(() => send('hello')).toThrow('InvalidStateError: the socket is not open');
  } finally {
    WebAssembly.instantiate = instantiate;
    peer.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 30_000);
