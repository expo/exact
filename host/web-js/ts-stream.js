// A TypeScript resource's stream on the JS target (LLP 1016.000 D7): the
// shared web reader owns admission and lifetime; the mapper produces a
// snapshot for every protocol frame. Loaded only for a streaming source.
import { streamed } from './http-body.js';
import { answering, answers, isSignal, letGo, ownSignal, SOCKET_AND_FETCH } from './ts-fetch.js';
import { rt } from './rt-link.js'; // (rt.js's own, as a lazy chunk takes them: rt-link.js)
const { clock, journal } = rt;
import { failureCode, failureReason } from './shape.js';
import { bodyFromRefusal, methodOf, readBodyFile } from './ts-body-from.js';
import { refuseStreamBody } from './uploads.js';

const KINDS = ['Response', 'Network', 'Refused', 'Unsupported', 'Aborted', , , , , , 'Timeout'], said = new TextDecoder(), utf8 = new TextEncoder();
// Hermes's checks and words on `init` (js/src/fetch.js `request`): a
// refusal ends the stream with that message, which fails the answer.
function request(input, init, redirect, protocols) {
  if (typeof init.exactStream !== 'function') throw new TypeError('exactStream maps each event to the answer: (event) => value');
  const independent = init.exactIndependentHttp?.maxResponseBytes;
  if (independent !== undefined && (!Number.isInteger(independent) || independent <= 0 || independent > 67108864))
    throw new TypeError('exactIndependentHttp.maxResponseBytes must be an integer from 1 to 67108864');
  // `exactBodyFrom` (LLP 1108 D6 R2): read as the stream opens (http-body.js).
  const bodyFrom = init.exactBodyFrom === undefined ? undefined : init.exactBodyFrom, refusal = bodyFrom === undefined ? null : bodyFromRefusal(input, init);
  if (refusal) throw new TypeError(refusal);
  // The transport takes a complete body, as Hermes's does.
  const body = init.body;
  refuseStreamBody(body);
  const raw = body == null ? undefined : body instanceof ArrayBuffer ? new Uint8Array(body.slice(0))
    : ArrayBuffer.isView(body) ? new Uint8Array(body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength)) : new TextEncoder().encode(String(body));
  const request = typeof Request === 'function' && input instanceof Request ? input : null;
  // The caller's signal, as Fetch picks it (`init.signal` over the Request's)
  const signal = init.signal !== undefined ? init.signal : request ? request.signal : null;
  if (signal != null && !isSignal(signal)) throw new TypeError('fetch: init.signal is not an AbortSignal');
  const method = methodOf(input, init), url = request ? request.url : String(input);
  // (a Request's own body comes along when `init` names none: a long poll has none)
  if (init.exactLongPoll !== undefined && request?.body != null && init.body == null)
    throw new TypeError('exactLongPoll: a long poll is a GET with no body');
  const upload = uploadOf(init, method, url, redirect), timeoutMs = upload ? init.exactTimeout : undefined;
  if (timeoutMs !== undefined && (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 3600000))
    throw new TypeError('exactTimeout must be an integer number of milliseconds from 1 to 3600000');
  // Read again as the stream opens: what `fetch` checked may have changed since (LLP 1016.000 D11).
  const idle = init.exactIdleTimeout;
  if (idle !== undefined && (!Number.isInteger(idle) || idle < 1 || idle > 3600000)) throw new TypeError('exactIdleTimeout must be an integer number of milliseconds from 1 to 3600000');
  if (idle !== undefined && (upload || init.exactLongPoll !== undefined || !/^wss?:/i.test(url))) throw new TypeError('exactIdleTimeout: only a WebSocket stream takes an idle timeout');
  return { method, url, signal, headers: [...new Headers(init.headers ?? request?.headers ?? [])], raw, bodyFrom, cache: init.cache ?? request?.cache, mode: init.mode ?? request?.mode, redirect, maxResponseBytes: independent ?? 1048576, longPoll: longPollOf(init.exactLongPoll, method, url), upload, timeoutMs, idleTimeoutMs: idle, protocols };
}
// `exactUploadProgress` (LLP 1016.000 D10), as js/src/binary.js `uploadOf` checks it.
function uploadOf(init, method, url, redirect) {
  const upload = init.exactUploadProgress;
  if (upload === undefined || upload === false) return undefined;
  if (upload !== true) throw new TypeError('exactUploadProgress is true or false');
  if (/^wss?:/i.test(url)) throw new TypeError('exactUploadProgress: a WebSocket sends no body');
  if (/^[a-z][a-z0-9+.-]*:/i.test(url) && !/^https?:/i.test(url)) throw new TypeError('exactUploadProgress: an upload is an http: or https: request');
  if (init.exactLongPoll !== undefined) throw new TypeError('exactUploadProgress: a long poll sends no body');
  if (method === 'GET' || method === 'HEAD') throw new TypeError('exactUploadProgress: a GET or HEAD request sends no body');
  if (redirect !== undefined && redirect !== 'follow') throw new TypeError('exactUploadProgress: an upload follows its redirects');
  return true;
}
// `exactLongPoll` (LLP 1016.000 D8), as js/src/binary.js `longPollOf` checks it.
function longPollOf(poll, method, url) {
  if (poll === undefined) return undefined;
  if (/^wss?:/i.test(url)) throw new TypeError('exactLongPoll: a long poll is HTTP, not a WebSocket');
  if (method !== 'GET') throw new TypeError('exactLongPoll: a long poll asks with GET');
  if (poll === null || typeof poll !== 'object') throw new TypeError('exactLongPoll is { cursor, from }');
  // (characters as the runner counts them, code points; `from` a member, or two joined by one `.`)
  const name = k => typeof poll[k] === 'string' && [...poll[k]].length > 0 && [...poll[k]].length <= 256;
  if (!name('cursor')) throw new TypeError('exactLongPoll.cursor is the query parameter the cursor goes in, a name');
  if (!name('from') || poll.from.split('.').length > 2 || poll.from.split('.').some(p => !p))
    throw new TypeError("exactLongPoll.from is the reply's field the cursor comes from, a name, or two joined by one `.`");
  if (poll.sameCursor !== undefined && poll.sameCursor !== 'heartbeat' && poll.sameCursor !== 'event')
    throw new TypeError('exactLongPoll.sameCursor is "heartbeat" (the default) or "event"');
  return { cursor: poll.cursor, from: poll.from, sameCursorEvent: poll.sameCursor === 'event' };
}
// A turn's operations, sent once its commit stands. The whole batch is
// checked again before any frame: an earlier batch in the same commit may
// have filled the browser's buffer or closed the socket.
const publish = (operations, call) => () => {
  // Sent or refused, they are no longer the call's to count (`bufferedAmount`, the buffer's bound).
  if (call?.unsent) call.unsent = call.unsent.filter(x => !operations.includes(x));
  const totals = new Map();
  for (const x of operations) {
    if (x.owner.signal.aborted || x.owner.active?.() === false) throw new DOMException('the socket is not open', 'InvalidStateError');
    totals.set(x.transport, (totals.get(x.transport) ?? 0) + x.size);
  }
  for (const [socket, count] of totals) socket.check(count);
  for (const x of operations) {
    x.transport[x.op](x.data, x.reason);
    journal.push(`t=${clock.now} socket ${x.op}: ${x.size} bytes (${x.owner.streamName})`);
  }
};
// What an answer in flight sent in a later turn (its code after an `await` in an async function, or its
// own timer): one batch for the turn, sent when the turn ends (a timer's as its callback returns, before the
// next timer of the advance; an `await`'s step in the microtask after it), behind what its source sent,
// which waits for the commit that asked it (LLP 1016.000 D7, the amendment of 2026-11-10 on later turns). A
// refused commit takes the batches that waited for it back, and so does the answer's being let go while one
// waits; the journal says which.
function later(call) {
  const batch = call.laterOps;
  if (!batch) return; // the turn's end already took it
  call.laterOps = null;
  const go = publish(batch, call);
  const said = why => journal.push(`t=${clock.now} answer ${call.name} failed: ${why}; socket frames were refused`);
  const refused = why => { call.unsent = call.unsent.filter(x => !batch.includes(x)); said(why); };
  // Let go since it staged these, it sends nothing more. Its ticket is its owner's until its reply lands, which
  // rt.js wires after the commit that asked it stands, so an answer that gave its value still sends what waited.
  const run = Object.assign(() => { if (letGo(call)) return refused('the answer was let go'); try { go(); } catch (e) { said(e?.message ?? e); } }, { refused });
  if (call.stood) run(); else if (call.stood === false) refused('the commit that asked it was refused'); else (call.waiting ??= []).push(run);
}
answering.later = later;
// The module never sees the browser socket. Operations belong to the current
// mutation's answer, to the socket's own mapper while it answers `open` or a
// `message` (`answering.mapper`), or to an answer in flight: while its source
// runs, and in each later turn the bundle's async helper or its own timer puts
// it back for (ts-data.js `__exact_async_context`, ts-fetch.js `runAs`; LLP
// 1016.000 D7, the amendments of 2026-10-18 and 2026-11-10). rt.js publishes
// a source's only after the commit that settles that turn, or that ran that
// source, stands.
function handle(transport, owner) {
  const encoder = new TextEncoder();
  answering.sockets = true;
  const turn = () => answering.mapper?.owner === owner ? answering.mapper : answering.call;
  const stage = (op, data, reason, size = 0) => {
    const call = turn(), mapper = !!call && call === answering.mapper, answer = !!call && !mapper && !call.mutation;
    // Code no answer is put back for (a plain `.then` callback outside an async function) has no turn here.
    if (!mapper && !call) throw new Error(`socket.${op} is only available in an answer's own turns (its source, its code after an \`await\` in an async function, its timer), or its socket's own mapper`);
    // A later turn: only of an answer still in flight. One that gave its value, failed or was let go has none.
    const resumed = !mapper && !call.turn;
    if (resumed && (!call.flight || !answers(call) || call.stood === false)) throw new DOMException(`socket.${op}: the answer has given its value or was let go`, 'InvalidStateError');
    if (owner.signal.aborted || owner.active?.() === false || (answer || resumed) && !owner.resource) throw new DOMException('the socket is not open', 'InvalidStateError');
    // A mutation's synchronous source may not both send and fetch; an answer in flight's later turns may.
    if (!mapper && !answer && !resumed && call.fetched) { call.socketConflict = true; throw new Error(SOCKET_AND_FETCH); }
    // Overload fails a mapper's snapshot, caught or not.
    const over = e => { if (mapper && e.name === 'QuotaExceededError') call.overload = e; return e; };
    const kind = mapper ? 'mapper call' : answer || resumed ? 'answer' : 'mutation';
    // The call's bounds are over all its turns; its buffer's, over what it has staged and not yet sent.
    const all = call.socketAll ??= [], unsent = call.unsent ??= [];
    if (all.length >= 256) throw over(new DOMException(`socket ${kind} exceeds 256 operations`, 'QuotaExceededError'));
    if (call.closed?.has(transport)) throw new DOMException('the socket is not open', 'InvalidStateError');
    if (all.reduce((n, x) => n + x.size, 0) + size > 1048576) throw over(new DOMException(`socket ${kind} exceeds 1 MiB`, 'QuotaExceededError'));
    try { transport.check(unsent.reduce((n, x) => x.transport === transport ? n + x.size : n, 0) + size); } catch (e) { throw over(e); }
    const operations = resumed ? (call.laterOps ??= []) : (call.socketOps ??= []);
    if (resumed) { if (!operations.length) queueMicrotask(() => later(call)); }
    else if (!mapper && !operations.length) (call.effects ??= []).push(publish(operations, call));
    const staged = { transport, owner, op, data, reason, size };
    operations.push(staged); all.push({ size }); unsent.push(staged);
    if (op === 'close') (call.closed ??= new Set()).add(transport);
  };
  return Object.freeze({
    get readyState() { return owner.signal.aborted ? 3 : transport.readyState; },
    // The browser's count (WHATWG: as of the task's start, plus its sends)
    // and what this turn has staged on the socket.
    get bufferedAmount() {
      const staged = turn()?.unsent ?? [];
      return transport.bufferedAmount + staged.reduce((n, x) => x.transport === transport ? n + x.size : n, 0);
    },
    send(data) {
      let copy, size;
      if (typeof data === 'string') { copy = data; size = encoder.encode(data).length; }
      else if (data instanceof ArrayBuffer || ArrayBuffer.isView(data)) {
        const bytes = data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
        copy = bytes.slice(); size = copy.byteLength;
      } else throw new TypeError('socket.send expects a string, ArrayBuffer or ArrayBufferView');
      stage('send', copy, undefined, size);
    },
    close(code = 1000, reason = '') {
      if (code !== 1000 && (!Number.isInteger(code) || code < 3000 || code > 4999)) throw new DOMException('invalid socket close code', 'InvalidAccessError');
      if (typeof reason !== 'string' || encoder.encode(reason).length > 123) throw new SyntaxError('socket close reason exceeds 123 UTF-8 bytes');
      stage('close', code, reason);
    },
  });
}
// LLP 1133.003 §9's gate, under the agent only: agent.js installs
// `exact.streamGate`, which times each message from its receipt
// (http-body.js `received`) through the mapper to the commit, and the stub
// decode the mapper calls. A page with no agent has none of it.
const gate = () => globalThis.exact?.streamGate;
export function open({ input, init, redirect, protocols }, conv, grantSet, deliver, controller) {
  let req;
  try { req = request(input, init, redirect, protocols); req.forced = !!controller.forced; } catch (e) { return Promise.resolve({ error: String(e?.message ?? e), code: failureCode(e), reason: failureReason(e) }); }
  // The caller's signal ends the stream too, before a body is read from a file
  // or after. The stream hears it, while it lasts, through a signal of its own
  // (ts-fetch.js `ownSignal`), whose abort event the app's listeners on the
  // caller's signal do not reach where the platform composes signals.
  const own = req.signal, heard = ownSignal(own), abort = () => controller.abort(own.reason);
  if (own?.aborted) controller.abort(own.reason);
  else heard?.addEventListener('abort', abort, { once: true });
  const map = init.exactStream;
  controller.upload = !!req.upload; // in flight until its reply, for every clock (rt.js)
  let socket;
  // An HTTP body that is not events is a byte stream (http-body.js `readBytes`).
  const protocol = /^wss?:/i.test(req.url) ? { socket: transport => { socket = handle(transport, controller); controller.socket = transport; } } : { bytes: true };
  // A mapper call's replies are refused with the frame: the journal says so.
  const refused = why => journal.push(`t=${clock.now} mapper ${controller.streamName} failed: ${why}; socket frames were refused`);
  // `frame`: the socket's `open` or a `message`, which its mapper may answer
  // with replies on it (D7). They ride the frame's answer as effects
  // (rt.js `reply`), sent once its commit stands. A mapper that returns a
  // promise answers when it settles (Charlie, 2026-11-10); the replies it
  // staged before its first `await` go now, as natively they go with the
  // frame's first commit, and after it, it cannot send.
  const failed = (e, turn) => {
    if (turn?.socketOps?.length) refused(e?.message ?? e);
    return { error: String(e?.message ?? e), code: failureCode(e), reason: failureReason(e) };
  };
  const value = (event, frame) => {
    if (controller.active?.() === false) return { error: 'the stream was let go', code: 'error' };
    const turn = socket && frame ? answering.mapper = { owner: controller, socketOps: null, overload: null } : null;
    // The call is the mapper's in every turn, and no answer's (ts-fetch.js `answering.mapperCall`).
    const outer = { call: answering.call, mapperCall: answering.mapperCall };
    answering.call = null;
    // Settled once it has answered or its stream was let go: entropy is then refused (ts-fetch.js `answerOnly`).
    let ended = false;
    const call = answering.mapperCall = { get settled() { return ended || controller.active?.() === false; } };
    try {
      const v = map(event, socket);
      if (turn?.overload) throw turn.overload;
      // A socket no resource owns (a mutation's stream) takes none, as natively.
      if (turn?.socketOps?.length && !controller.resource) throw new DOMException('the socket is not open', 'InvalidStateError');
      const ops = turn?.socketOps, effects = ops?.length ? [Object.assign(() => { try { publish(ops)(); } catch (e) { refused(e?.message ?? e); } }, { refused })] : null;
      if (!v || typeof v.then !== 'function') { ended = true; return effects ? { v: conv(v), effects } : { v: conv(v) }; }
      effects?.[0]();
      return Promise.resolve(v).then(v => controller.active?.() === false ? { error: 'the stream was let go', code: 'error' } : { v: conv(v) }).catch(e => failed(e)).finally(() => { ended = true; });
    } catch (e) { ended = true; return failed(e, turn); } finally { if (turn) answering.mapper = null; Object.assign(answering, outer); }
  };
  // A later frame's mapper call waits for an earlier one's promise, and so
  // does the end: `behind` is the last call still settling.
  let behind = null;
  const inOrder = run => {
    const r = behind ? behind.then(run) : run();
    if (r && typeof r.then === 'function') { const done = () => { if (behind === at) behind = null; }, at = behind = r.then(done, done); }
    return r;
  };
  const settled = (r, then) => r && typeof r.then === 'function' ? r.then(then) : then(r);
  // A long poll's reply that is no event (`exact:reply`, LLP 1016.000 D8) reaches no mapper, nor an upload's start
  // (`exact:sending`), which, as any bytes sent, says it reached the network (rt.js); its bytes sent are a `progress`
  // event, as XMLHttpRequest's.
  const event = m => req.upload && m.event === 'exact:progress' ? { type: 'progress', ...JSON.parse(m.data), lengthComputable: true, data: '', lastEventId: '', coalesced: m.coalesced } : { type: m.event || 'message', data: m.data, lastEventId: m.id, coalesced: m.coalesced, protocol: m.protocol };
  // Messages that arrive together (`more`, http-body.js) are mapped one by one, each after the last one's promise,
  // and their last value commits once (LLP 1016.000 D4); a failure ends them there. A mapper that answers now is
  // called in a loop, so a read of any number of events takes no stack.
  let batch = [];
  const each = (ms, i = 0) => {
    for (;; i++) {
      const r = value(event(ms[i]), true), at = i, last = r => r.error !== undefined || at === ms.length - 1;
      const done = r => last(r) ? deliver({ ...r, coalesced: ms[at].coalesced, messages: at + 1 }) : each(ms, at + 1);
      if (r && typeof r.then === 'function') return r.then(done);
      if (last(r)) return done(r);
    }
  };
  // What waits behind a mapper that awaits, as natively (mapper_turns.rs): at most 4096 messages, or 64 MiB of
  // their data; one more ends the stream (D4: never queue without limit).
  let held = 0, heldBytes = 0, fellBehind = false;
  const bytesOf = m => typeof m.data === 'string' ? utf8.encode(m.data).length : m.data.byteLength;
  const message = m => {
    controller.sent ||= !!req.upload;
    if (fellBehind || req.longPoll && m.event === 'exact:reply' || req.upload && m.event === 'exact:sending') return;
    batch.push(m);
    if (m.more) return;
    const ms = batch; batch = [];
    if (!behind) { inOrder(() => each(ms)); return behind; }
    const bytes = ms.reduce((n, m) => n + bytesOf(m), 0);
    if (held + ms.length > 4096 || heldBytes + bytes > 64 << 20) {
      fellBehind = true;
      return void deliver({ error: `the stream's mapper fell behind: ${held} messages (${heldBytes} bytes) waited for its turns, and ${ms.length} more arrived`, code: 'error' });
    }
    held += ms.length; heldBytes += bytes;
    inOrder(() => { held -= ms.length; heldBytes -= bytes; return each(ms); });
    return behind;
  };
  const timed = m => {
    const g = gate();
    if (!g) return message(m);
    g.enter(controller.streamName, m.received);
    try { return message(m); } finally { g.leave(); }
  };
  return streamed(req, grantSet, timed, controller, (path, set) => readBodyFile(path, set, controller), protocol)
    .then(o => {
      gate()?.ended(controller.streamName);
      if (o.close) journal.push(`t=${clock.now} socket close: ${o.close.code} ${o.close.reason} (${controller.streamName})`);
      return inOrder(() => value(o.close ? { type: 'close', data: '', lastEventId: '', coalesced: 0, ...o.close } : o.kind
      ? { type: 'error', data: '', lastEventId: '', coalesced: 0, kind: KINDS[o.kind], message: said.decode(o.body), status: 0 }
      : req.upload ? { type: 'load', status: o.status, data: said.decode(o.body), lastEventId: '', coalesced: 0 }
      : { type: 'error', data: said.decode(o.body), lastEventId: '', coalesced: 0, kind: 'Response', message: `HTTP ${o.status}`, status: o.status }));
    })
    .finally(() => heard?.removeEventListener('abort', abort));
}
