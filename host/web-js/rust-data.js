import { admitsSecret, createRequestExecutor, fetchHostAsset, sameGrantDeclaration, setAppGrantSet } from './admission.js';
import { rustGrantSet, tsGrantSet } from './admission-data.js';
import { boundedHttpBody, longPollRefusal, streamed } from './http-body.js';
import { rt } from './rt-link.js'; // (rt.js's own, as a lazy chunk takes them)
// A Rust data module on the JS runtime (LLP 1029.000's seam, ABI 9,
// `logic/abi/src/lib.rs`): the app's importless module wasm and its plan's
// declarations (`app.bind.plan`, the plan without the bake's answers),
// fetched after first pixel; once bound and activated, every answer is a
// synchronous call, as the runner's `DataSource::answer` is. Until then the
// runtime keeps each resource's compiled value and asks again at `ready`.
const ABI = 9, utf8 = new TextEncoder(), text = new TextDecoder();

function writer() {
  let buf = new Uint8Array(256), at = 0;
  const room = n => { if (at + n > buf.length) { const b = new Uint8Array((at + n) * 2); b.set(buf); buf = b; } };
  const w = {
    u8(v) { room(1); buf[at++] = v; },
    u16(v) { room(2); new DataView(buf.buffer).setUint16(at, v, true); at += 2; },
    u32(v) { room(4); new DataView(buf.buffer).setUint32(at, v, true); at += 4; },
    f64(v) { room(8); new DataView(buf.buffer).setFloat64(at, v, true); at += 8; },
    u64(v) { room(8); new DataView(buf.buffer).setBigUint64(at, BigInt(v), true); at += 8; },
    bytes(b) { w.u32(b.length); room(b.length); buf.set(b, at); at += b.length; },
    str(s) { w.bytes(utf8.encode(s)); },
    done: () => buf.subarray(0, at),
  };
  return w;
}
// A value by its declared type (`n b s u ?T [T {T…}`): the runtime's arrays
// are records or lists, and `null` is none, only by type.
function encode(w, v, t, i = 0) {
  const c = t[i++];
  if (c === 'n') { w.u8(0); w.f64(v); }
  else if (c === 'b') { w.u8(1); w.u8(v ? 1 : 0); }
  else if (c === 's') { w.u8(2); w.str(v); }
  else if (c === 'u') w.u8(3);
  else if (c === '?') { if (v == null) { w.u8(4); return skip(t, i); } w.u8(5); return encode(w, v, t, i); }
  else if (c === '[') { w.u8(6); w.u32(v.length); let end = skip(t, i); for (const x of v) end = encode(w, x, t, i); return v.length ? end : skip(t, i); }
  else if (c === '{') { w.u8(7); const types = []; while (t[i] !== '}') { types.push(i); i = skip(t, i); } w.u32(types.length); types.forEach((ti, k) => encode(w, v[k], t, ti)); return i + 1; }
  return i;
}
function skip(t, i) {
  const c = t[i++];
  if (c === '?' || c === '[') return skip(t, i);
  if (c === '{') { while (t[i] !== '}') i = skip(t, i); return i + 1; }
  return i;
}
function reader(b) {
  const d = new DataView(b.buffer, b.byteOffset, b.byteLength); let at = 0;
  const r = {
    u8: () => b[at++],
    u16: () => { const v = d.getUint16(at, true); at += 2; return v; },
    u32: () => { const v = d.getUint32(at, true); at += 4; return v; },
    f64: () => { const v = d.getFloat64(at, true); at += 8; return v; },
    u64: () => { const v = Number(d.getBigUint64(at, true)); at += 8; return v; },
    str: () => { const n = r.u32(), s = text.decode(b.subarray(at, at + n)); at += n; return s; },
    bytes: n => { const x = b.slice(at, at + n); at += n; return x; },
    value() {
      const tag = r.u8();
      if (tag === 0) return r.f64();
      if (tag === 1) return r.u8() === 1;
      if (tag === 2) return r.str();
      if (tag === 3 || tag === 4) return null;
      if (tag === 5) return r.value();
      const n = r.u32(), out = new Array(n);
      for (let k = 0; k < n; k++) out[k] = r.value();
      return out;
    },
  };
  return r;
}

export async function install(data, sources, load = p => fetchHostAsset(p).then(r => {
  if (!r.ok) throw new Error(`${p.slice(2)} answered HTTP ${r.status}`);
  return r.arrayBuffer();
}, e => { throw new Error(`${p.slice(2)} could not be fetched: ${e?.message ?? e}`); })) {
  const [wasm, plan] = await Promise.all([load('./rust/wasm/app.module.wasm'), load('./app.bind.plan')]);
  // Bytes, or a module a renderer compiled once for every render.
  const made = await WebAssembly.instantiate(wasm, {}), instance = made.instance ?? made;
  const e = instance.exports;
  if (e.exact_logic_abi() !== ABI) throw new Error(`Rust module ABI ${e.exact_logic_abi()} is not ${ABI}`);
  const session = e.exact_logic_create();
  const call = bytes => {
    const p = e.exact_logic_alloc(bytes.length);
    new Uint8Array(e.memory.buffer, p, bytes.length).set(bytes);
    const rc = e.exact_logic_call(session, p, bytes.length);
    e.exact_logic_dealloc(p, bytes.length);
    if (rc !== 0) throw new Error('Rust module rejected the call');
    const r = reader(new Uint8Array(e.memory.buffer, e.exact_logic_output(session), e.exact_logic_output_len(session)).slice());
    if (r.u32() !== ABI) throw new Error('logic ABI version differs');
    return r;
  };
  // A result (`read_result`): now, or an HTTP request for the host to run.
  const result = r => {
    let tag = r.u8(), cache;
    const prefetch = tag === 14;
    if (prefetch) { tag = r.u8(); if (![1, 12, 13].includes(tag)) throw new Error('invalid prefetch HTTP result'); }
    if (tag === 13) {
      const name = r.str(); tag = r.u8();
      if (!/^[a-z][a-z0-9-]*$/.test(name) || ![1, 6, 9, 10, 11, 12].includes(tag)) throw new Error('invalid preload HTTP result');
    }
    if (tag === 12) {
      cache = r.str(); tag = r.u8();
      if (!['default', 'no-store', 'reload', 'no-cache', 'force-cache', 'only-if-cached'].includes(cache) || ![1, 6, 9, 10].includes(tag)) throw new Error('invalid cached HTTP result');
    }
    if (tag === 0) return { v: r.value() };
    // The source's own refusal refuses the commit, as the runner's does;
    // UnknownSource (2): a mixed app's TypeScript module may answer it.
    // A call the seam could not carry (`call` above) is the resource's
    // failure instead (LLP 1071 §7).
    if (tag >= 2 && tag <= 4) throw Object.assign(new Error(r.str()), { unknown: tag === 2, refuse: true });
    // A stream (9), a long poll (10, LLP 1016.000 D8), a two-way socket (11, LLP 1016.003).
    if (tag === 1 || tag === 6 || (tag >= 9 && tag <= 11)) {
      if (prefetch && tag !== 1) throw new Error('exactPrefetch requires a plain HTTP read');
      const http = tag === 1 ? 'ordered' : `independent:${r.u32()}`;
      const scoped = r.u8() === 1, scope = r.str();
      const method = r.str(), url = r.str(), headers = [];
      for (let n = r.u32(); n--;) headers.push([r.str(), r.str()]);
      const n = r.u32(), body = r.bytes(n);
      const protocols = [];
      if (tag >= 9) for (let n = r.u32(); n--;) protocols.push(r.str());
      // A long poll (tag 10, LLP 1016.000 D8): its terms after the body, checked as the stream opens (http-body.js).
      const longPoll = tag === 10 ? { cursor: r.str(), from: r.str(), sameCursorEvent: [false, true][r.u8()] ?? (() => { throw new Error('invalid long poll terms'); })() } : undefined;
      // (refused as the native seam refuses it: before the turn's writes apply)
      const pollRefusal = longPoll && longPollRefusal({ url, longPoll, stream: true }, method, n ? 'x' : '');
      if (pollRefusal) throw new Error(pollRefusal);
      return { req: { method, url, headers, cache, ...(prefetch ? { prefetch: true } : {}), body: text.decode(body), raw: body, http, maxResponseBytes: http === 'ordered' ? undefined : Number(http.split(':')[1]), scope: scoped ? scope : null, stream: tag >= 9, socket: tag === 11, protocols, longPoll } };
    }
    // Storage work (LLP 1027.001 D2): a portable request the host runs
    // through the web host's own `storage-request.js`, under its scope.
    if (tag === 5) { const payload = r.bytes(r.u32()), scoped = r.u8() === 1, scope = r.str(); return { req: { storage: text.decode(payload), scope: scoped ? scope : null } }; }
    throw new Error(`a Rust answer of kind ${tag} (surface work) is not carried by the JS target`);
  };
  const op = (code, fill) => { const w = writer(); w.u32(ABI); w.u8(code); fill?.(w); return call(w.done()); };
  let r = op(0); r.u8(); const meta = [r.str(), r.str()];
  if (!sameGrantDeclaration(rustGrantSet, meta[1])) throw new Error('Rust module grants differ from the admitted build');
  r = op(1, w => w.bytes(new Uint8Array(plan))); r.u8(); result(r);
  r = op(2); r.u8(); result(r);
  // A Rust source's two-way sockets (LLP 1016.003), each open one's transport under a number for the resource that
  // owns it and its stream's ticket, which the module's handles keep: a handle from an earlier ticket is stale.
  const sockets = new Set(), ids = new WeakMap();
  let nextId = 0;
  const idOf = owner => { if (!ids.has(owner)) ids.set(owner, nextId++); return ids.get(owner); };
  const live = s => sockets.has(s) && s.transport.readyState === 1 && !s.controller.signal.aborted && s.controller.active?.() !== false;
  // What a call staged (`turn`: a mutation's, or `own` socket's mapper), each operation addressed to its socket now,
  // refused whole if one is not open, as natively. The batch goes once the commit stands, checked whole again, as
  // TypeScript's (ts-stream.js `publish`); a refused commit sends none of it, and the journal says so either way.
  const said = (turn, why) => rt.journal.push(`t=${rt.clock.now} ${turn.verb} ${turn.name} failed: ${why}; socket frames were refused`);
  const sending = (ops, turn) => {
    const batch = ops.map(o => {
      const s = [...sockets].find(s => s.id === o.id && live(s));
      if (!s) throw new Error('InvalidStateError: the socket is not open');
      return { ...o, s };
    });
    let held = true;
    for (const o of batch) o.s.unsent += o.size;
    const release = () => { if (held) for (const o of batch) o.s.unsent -= o.size; held = false; };
    return Object.assign(() => {
      release();
      try {
        const totals = new Map();
        for (const o of batch) {
          if (!live(o.s)) throw new DOMException('the socket is not open', 'InvalidStateError');
          totals.set(o.s, (totals.get(o.s) ?? 0) + o.size);
        }
        for (const [s, n] of totals) s.transport.check(n);
        for (const o of batch) {
          if (o.op === 'close') o.s.transport.close(o.code, o.reason); else o.s.transport.send(o.data);
          rt.journal.push(`t=${rt.clock.now} socket ${o.op}: ${o.size} bytes (${o.s.name})`);
        }
      } catch (e) { said(turn, e?.message ?? e); }
    }, { refused: why => { release(); said(turn, why); } });
  };
  // The call's socket turn (`encode_socket_turn`): who may send, and each open socket with its buffered bytes,
  // the transport's and what earlier calls of this commit staged on it.
  const socketTurn = (w, turn) => {
    if (turn?.own) { w.u8(2); w.u32(turn.own.id); w.u64(turn.own.ticket); } else w.u8(turn?.mutation ? 1 : 0);
    const open = [...sockets].filter(live);
    w.u32(open.length);
    for (const s of open) { w.u32(s.id); w.u64(s.ticket); w.u64(s.transport.bufferedAmount + s.unsent); }
  };
  // What the call staged (`encode_socket_ops`), and a mapper's overload.
  const socketOps = r => {
    const ops = [];
    for (let n = r.u32(); n--;) {
      const id = r.u32(), tag = r.u8();
      if (tag === 0) { const data = r.str(); ops.push({ id, op: 'send', data, size: utf8.encode(data).length }); }
      else if (tag === 1) { const data = r.bytes(r.u32()); ops.push({ id, op: 'send', data, size: data.length }); }
      else if (tag === 2) ops.push({ id, op: 'close', code: r.u16(), reason: r.str(), size: 0 });
      else throw new Error('invalid socket operation');
    }
    const over = r.u8(), why = r.str();
    return { ops, overload: over === 1 ? why : null };
  };
  // One call (`call_request`): source, arguments by type, admitted secret names, the store's
  // snapshot, its socket turn; its reply (`call_reply`): a store read, writes, socket operations, the result.
  // Whether the module's last call read the store before it failed: its resource is a store reader (LLP 1146 §2.3).
  let threwRead = false;
  const callWith = (code, source, args, store, outcome, turn) => {
    threwRead = false;
    const r = op(code, w => {
      w.str(source); w.u8(6); w.u32(args.length);
      const t = (sources[source] ?? '').split('|')[0]; let i = 0; for (const a of args) i = encode(w, a, t, i);
      const names = globalThis.__exactRender ? [] : rustGrantSet.entries.filter(([, , grant]) => grant?.[0] === 'secret-keep').map(([, , grant]) => grant[1]);
      w.u32(names.length); for (const name of names) w.str(name);
      const pairs = globalThis.__exactRender ? [] : [...store.map].filter(([k]) => admitsSecret(rustGrantSet, k));
      w.u32(pairs.length); for (const [k, v] of pairs) { w.str(k); w.str(v); }
      if (outcome) {
        if (outcome.storage) { w.u8(5); w.bytes(outcome.storage); }
        // One message of a stream (LLP 1016.000), the ABI's tag 8, or 12 for a socket's binary frame; 11 a socket's close.
        else if (outcome.streamed) {
          const m = outcome.streamed, binary = typeof m.data !== 'string';
          w.u8(binary ? 12 : 8); w.str(m.event); w.str(m.id);
          if (binary) w.bytes(new Uint8Array(m.data)); else w.str(m.data);
          w.u32(m.coalesced); w.str(m.protocol ?? '');
        }
        else if (outcome.closed) { const c = outcome.closed; w.u8(11); w.u16(c.code); w.str(c.reason); w.u8(c.wasClean ? 1 : 0); }
        else if (outcome.failed) { w.u8(outcome.failed); w.str(outcome.message); }
        else { w.u8(0); w.u16(outcome.status); w.u32(outcome.headers.length); for (const [k, v] of outcome.headers) { w.str(k); w.str(v); } w.bytes(outcome.body); }
      }
      socketTurn(w, turn);
    });
    if (r.u8() !== 2) throw new Error('expected a call reply');
    const observed = r.u8() === 1;
    const writes = [];
    for (let n = r.u32(); n--;) { const k = r.str(); const v = r.u8() ? r.str() : null; if (!admitsSecret(rustGrantSet, k)) throw new Error(`secret ${k} is not granted`); writes.push([k, v]); }
    const { ops, overload } = socketOps(r);
    let out;
    try { out = result(r); } catch (error) {
      threwRead = observed; if (error.refuse && !globalThis.__exactRender) for (const [k, v] of writes) store.set(k, v);
      if (ops.length) said(turn, error.message);
      throw error;
    }
    if (!globalThis.__exactRender) for (const [k, v] of writes) store.set(k, v);
    if (observed) out.store = true;
    // A mapper's call past D7's bounds fails its frame, whatever it answered (LLP 1016.003 D4).
    if (overload) { if (ops.length) said(turn, overload); throw new Error(overload); }
    if (!ops.length) return out;
    // A mutation that sends answers now (D4); a mapper's answer that asks again reopens the stream, and sends none.
    if (out.req && turn.mutation) throw new Error('a socket mutation must answer synchronously');
    if (out.req) said(turn, 'its answer reopened the stream');
    else out.effects = [sending(ops, turn)];
    return out;
  };
  const rust = (source, args, store, target, mutation) => callWith(3, source, args, store, null, mutation ? { mutation, verb: 'send', name: target } : null);
  const ts = data.ts, tsThrewRead = data.threwRead;
  data.threwRead = () => { const read = [threwRead, !!tsThrewRead?.()]; threwRead = false; return read[0] || read[1]; };
  data.answer = ts ? (source, args, store, target, mutation, forced) => { try { return rust(source, args, store, target, mutation); } catch (e) { if (e.unknown) return ts(source, args, store, target, mutation, forced); throw e; } } : rust;
  // A stream's messages that arrived together (request-send.js): each parsed in order (LLP 1016.000 D4), the last
  // one's answer theirs; one that answers with a request (the stream asked again) ends them there.
  // A two-way socket's `open` or frame is its own mapper's turn (LLP 1016.003 D4): it may send on that socket.
  data.parse = (source, args, outcome, store) => {
    if (!outcome.streamed) return callWith(4, source, args, store, outcome);
    let out, read = false;
    const effects = [];
    for (const m of outcome.streamed) {
      out = callWith(4, source, args, store, { streamed: m }, m.socket && { own: m.socket, verb: 'mapper', name: m.socket.name });
      read ||= !!out.store;
      effects.push(...(out.effects ?? []));
      if (out.req) break;
    }
    if (read) out.store = true;
    if (effects.length) out.effects = effects;
    return out;
  };
  // An overlay (`overlay_request`): what a resource shows while writes affect it; none from the Rust module asks
  // the TypeScript one's, beside it.
  const types = s => (sources[s] ?? '|').split('|'), tsOverlay = data.overlay;
  data.overlay = (source, args, answer, writes) => {
    const [params, result] = types(source);
    const r = op(5, w => {
      w.str(source); w.u8(6); w.u32(args.length); let i = 0; for (const a of args) i = encode(w, a, params, i);
      encode(w, answer, result); w.u32(writes.length);
      for (const x of writes) {
        const [p, res] = types(x.source); w.u64(x.id); w.str(x.mutation); w.str(x.source);
        w.u8(6); w.u32(x.args.length); let k = 0; for (const a of x.args) k = encode(w, a, p, k);
        if (x.reply === undefined) w.u8(0); else { w.u8(1); encode(w, x.reply, res); }
        w.u8(x.answered ? 1 : 0);
      }
    });
    if (r.u8() !== 3) throw new Error('expected an overlay reply');
    const tag = r.u8();
    if (tag === 2) throw new Error(r.str());
    // A source the Rust module does not answer: the TypeScript module's overlay, beside it.
    if (tag === 3) return tsOverlay?.(source, args, answer, writes);
    if (tag === 0) return undefined;
    const value = r.value(), keep = []; for (let n = r.u32(); n--;) keep.push(r.u64());
    return { value, keep };
  };
  // The host runs the request under this child's own authority. An absent
  // request scope is therefore the Rust child set, not the mixed-app union.
  // A stream (`Answer::stream`, LLP 1016.000) is the web host's own reader, under the same authority;
  // its end is the outcome a single request's would be. A resource's two-way socket (`Request::socket`) takes the
  // reader's D7 path, as a TypeScript one's (ts-stream.js): `open`, every frame, binary too, and its close; each of
  // its messages names its socket for `parse`. A mutation's socket has no mapper to send, and only listens.
  const run = createRequestExecutor(data.appId ?? meta[0], rustGrantSet, boundedHttpBody);
  data.fetch = (req, message, controller) => {
    if (!req.stream) return run(req);
    let socket = null;
    const protocol = req.socket && controller.resource ? { socket: transport => {
      socket = { transport, controller, id: idOf(controller.owner), ticket: controller.ticket, name: controller.streamName, unsent: 0 };
      sockets.add(socket); controller.socket = transport;
    } } : undefined;
    return streamed(req, rustGrantSet, m => message(socket ? { ...m, socket } : m), controller, undefined, protocol).then(o => {
      sockets.delete(socket);
      if (o.close) rt.journal.push(`t=${rt.clock.now} socket close: ${o.close.code} ${o.close.reason} (${controller.streamName})`);
      return o.close ? { closed: o.close } : o.kind ? { failed: o.kind, message: text.decode(o.body) }
        : { status: o.status, headers: o.headers ? o.headers.split('\n').map(l => [l.slice(0, l.indexOf(': ')), l.slice(l.indexOf(': ') + 2)]) : [], body: o.body };
    });
  };
  // What a loaded capability needs of the module (canvas2d.js's draws).
  data.logic = { exports: e, session, writer, reader, encode, ABI };
  data.appId ??= meta[0];
  data.grants = setAppGrantSet(tsGrantSet, rustGrantSet);
  for (const f of data.q.splice(0)) f();
}
