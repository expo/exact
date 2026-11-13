// App-module bindings, injected by the bundler, never installed as page-wide
// globals. Host modules keep the browser's functions at every load time.
import { socketProtocols, FetchError, fetchWith, inOverlay, overlaying, ownWithout, redirectOf, signalDeadline } from './admission.js';
import { tsGrantSet } from './admission-data.js';
// A stream or an upload takes the redirect mode as `fetch` is called, as Hermes
// does: a bad one rejects before a file is read or a stream registered.
// A mutation does one kind of effect (LLP 1016.000 D7): socket operations
// or a fetch. Whichever comes second refuses, and so does the mutation
// (ts-data.js). In a module that holds a socket, a mutation's fetch is
// captured when called and starts after its source returns, so a refused
// one never starts; a module with none fetches at once, as before.
export const SOCKET_AND_FETCH = 'a mutation cannot both send on a socket and fetch';
// `exactHeld` (LLP 1016.000 D9), with Hermes's words (js/src/fetch.js): the server may hold the reply. Each
// fetch is counted on its answer, held (`call.held`) or not (`call.plain`), or as tied to no answer
// (`answering.loose`); an answer with only held fetches out counts for no clock (held.js `counted`, whose
// hooks the page's entry sets on `answering`, build.mjs).
export function fetch(input, options) {
  // Checked before any path, a mutation's captured fetch included (LLP 1016.000 D8).
  if (options?.exactLongPoll !== undefined && options.exactStream === undefined)
    return Promise.reject(new TypeError('exactLongPoll asks again for an answer that keeps coming: give exactStream too'));
  const upload = options?.exactUploadProgress; // LLP 1016.000 D10
  if (upload !== undefined && upload !== false && options.exactStream === undefined)
    return Promise.reject(new TypeError(upload === true ? "exactUploadProgress reports an upload's progress to its stream: give exactStream too" : 'exactUploadProgress is true or false'));
  const held = options?.exactHeld;
  if (held !== undefined && held !== true && held !== false) return Promise.reject(new TypeError('exactHeld is true or false'));
  if (held && options.exactStream !== undefined) return Promise.reject(new TypeError('exactHeld: a stream is never held; it stops counting at its first message'));
  const idle = options?.exactIdleTimeout; // LLP 1016.000 D11
  if (idle !== undefined) {
    if (!Number.isInteger(idle) || idle < 1 || idle > 3600000) return Promise.reject(new TypeError('exactIdleTimeout must be an integer number of milliseconds from 1 to 3600000'));
    let url; try { url = input instanceof Request ? input.url : String(input); } catch (e) { return Promise.reject(e); }
    if (options.exactStream === undefined || options.exactUploadProgress === true || options.exactLongPoll !== undefined || !/^wss?:/i.test(url))
      return Promise.reject(new TypeError('exactIdleTimeout: only a WebSocket stream takes an idle timeout'));
  }
  let protocols;
  try {
    const offered = options?.exactProtocols;
    if (offered !== undefined)
      protocols = socketProtocols(offered, options.exactStream !== undefined && /^wss?:/i.test(input instanceof Request ? input.url : String(input)));
  } catch (e) { return Promise.reject(e); }
  // A stream counts by its own rule (D5), never here.
  if (options?.exactStream !== undefined) return fetchNow(input, options, protocols);
  // In a render a body from a file or to one is storage, refused there (ts-body-from.js): it is no work to count.
  if (globalThis.__exactRender && (options?.exactBodyFrom !== undefined || options?.exactBodyTo !== undefined)) return fetchNow(input, options);
  // An upload or a download to a file is never held (Request::is_held).
  return counting(held && options.exactBodyFrom === undefined && options.exactBodyTo === undefined ? 'held' : 'plain', () => fetchNow(input, options));
}
/** Work an answer awaits that is no held fetch (a fetch, `crypto.subtle`, `native.later`, a kept key): counted on
 * its answer (`call.plain`), so the answer is never held while it is out (LLP 1016.000 D9). */
export const ordinary = start => counting('plain', start);
globalThis.__exact_ordinary = ordinary; // for host modules ts-fetch.js does not reach (auth.js)
// A render waits for I/O even after its source returns a value or its answer Promise settles.
let RenderIO = 0;
export const unfinishedIO = () => RenderIO > 0;
function counting(kind, start) {
  if (globalThis.__exactRender) RenderIO++;
  const call = answering.call;
  // The hook that counted the start counts the end, so the two always pair.
  const loose = answering.loose;
  const end = () => { if (globalThis.__exactRender) RenderIO--; if (call) { call[kind]--; if (kind === 'held') answering.heldReply(call); call.recount?.(); } else loose(-1); };
  if (call) { call[kind] = (call[kind] ?? 0) + 1; call.recount?.(); } else loose(1);
  let out;
  try { out = start(); } catch (e) { end(); throw e; }
  return Promise.resolve(out).finally(end);
}
function fetchNow(input, options, protocols) {
  if (overlaying.on) return Promise.reject(inOverlay('fetch()'));
  // A mapper makes no fetch (D7), in any turn of its call.
  if (answering.mapper || answering.mapperCall) return Promise.reject(new Error(answering.mapper ? "fetch() is unavailable in a socket's mapper: it answers each frame now" : "fetch() is unavailable in a stream's mapper: fetch in the answer that opens the stream"));
  const call = answering.call;
  if (call?.mutation) {
    // An answer in flight may fetch in a later turn though its source sent, as natively (LLP 1016.000 D7, amended
    // 2026-11-12). The fetch waits for the commit that asked the answer: its refusal takes the frames back, and this too.
    const later = !call.turn && call.flight && !call.socketConflict && call.socketOps?.length > 0;
    // A stream's is refused, as it was (a mutation that sent opens none), and refuses nothing the answer goes on to do.
    if (later && options?.exactStream !== undefined) return Promise.reject(new Error(SOCKET_AND_FETCH));
    // An answer let go since (while it waited too: this runs again as it is released) fetches nothing, as it sends nothing.
    if (later && !answers(call)) return Promise.reject(new FetchError('Aborted', 'the answer was let go before this fetch'));
    if (later && call.stood === false) return Promise.reject(new Error('fetch(): the commit that asked this answer was refused'));
    if (later && !call.stood) {
      // The request is the one `fetch` was given: what it reads of its options is read now, as a mutation's always is.
      let target, init;
      try {
        init = options?.exactBodyFrom === undefined && options?.exactBodyTo === undefined ? captured(options) : held(options);
        // A `Request` is copied: its headers can change and its body be read meanwhile. The signal stays the one
        // given, since a copy's is another, whose `AbortSignal.timeout` deadline `signalDeadline` does not know.
        // One whose body was read or is held by a reader cannot be copied. It is a request only when the options
        // give another body, and is then kept, with its headers as they are now; otherwise it is refused now, as
        // `fetch` refuses it, whatever its body becomes while this waits. Left unequal to a fetch that did not
        // wait (LLP 1016.000, the amendment of 2026-11-12): with `exactBodyFrom` or `exactBodyTo`, that fetch
        // looks at a reader's hold only after its file work, and takes no `body` its options inherit.
        if (input instanceof Request) {
          init = Object.assign(init ?? {}, init?.signal === undefined ? { signal: input.signal } : null);
          if (!input.bodyUsed && !input.body?.locked) target = input.clone();
          else if (init.body == null) throw new FetchError('Network', 'fetch(): the Request\'s body was read or is locked');
          else { target = input; if (init.headers === undefined) init.headers = new Headers(input.headers); }
        } else target = String(input);
      } catch (e) { return Promise.reject(e); }
      return new Promise((go, no) => (call.waiting ??= []).push(Object.assign(go, { refused: why => no(new Error(`fetch(): the commit that asked this answer was refused: ${why}`)) })))
        .then(() => asCall(call, () => fetchNow(target, init, protocols)));
    }
    if (!later && (call.socketConflict || call.socketOps?.length)) { call.socketConflict = true; return Promise.reject(new Error(SOCKET_AND_FETCH)); }
    call.fetched = true;
    // A stream only registers with its answer, which ts-data.js then refuses;
    // a body from a file is read after its source returns, and checks then.
    if (answering.sockets && options?.exactStream === undefined && options?.exactBodyFrom === undefined && options?.exactBodyTo === undefined) {
      let target, init;
      try { target = input instanceof Request ? input : String(input); init = captured(options); } catch (e) { return Promise.reject(e); }
      return Promise.resolve().then(() => {
        if (call.socketConflict) throw new Error(SOCKET_AND_FETCH);
        // As the mutation's own, so a deadline its signal gives aborts in its turn
        return asCall(call, () => fetchWith(tsGrantSet, target, init, null, undefined, undefined, call.origin));
      });
    }
    return fetchOf(input, options, call, protocols);
  }
  return fetchOf(input, options, call, protocols);
}
// What `fetch` reads of its options, read now (inherited ones too), with
// the headers and a body that can change copied.
const OPTIONS = ['method', 'headers', 'body', 'redirect', 'signal', 'mode', 'credentials', 'cache', 'referrer',
  'referrerPolicy', 'integrity', 'keepalive', 'priority', 'exactTimeout', 'exactIndependentHttp'];
function captured(options) {
  if (options == null) return options;
  const init = {};
  for (const key of OPTIONS) { const value = options[key]; if (value !== undefined) init[key] = value; }
  if (init.headers !== undefined) init.headers = new Headers(init.headers);
  const body = init.body;
  if (body instanceof ArrayBuffer) init.body = body.slice(0);
  else if (ArrayBuffer.isView(body)) init.body = new Uint8Array(body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength));
  else if (body instanceof URLSearchParams) init.body = new URLSearchParams(body);
  else if (typeof FormData === 'function' && body instanceof FormData) { init.body = new FormData(); for (const [k, v] of body) init.body.append(k, v); }
  return init;
}
// What a file's fetch reads of its options when it is called (`fetchOf`), read now: its own fields stay own and
// the inherited ones inherited, as the check of a file's request tells them apart.
const INHERITED = ['exactBodyFrom', 'exactBodyTo', 'exactIndependentHttp', 'method', 'body', 'exactStream', 'signal', 'exactTimeout', 'cache'];
function held(options) {
  const own = ownWithout(options, []), inherited = {};
  for (const key of [...INHERITED, 'redirect']) if (!Object.hasOwn(own, key)) inherited[key] = options[key];
  return Object.assign(Object.create(inherited), own);
}
// The origin a fetch carries (LLP 1133.036 D6): its answer's, or, after the answer's first await, where no call is
// traced, the least privileged of the answers in flight (LLP 1133.002 D12).
export const activeCall = () => answering.call ?? [...answering.untraced].find(c => !c.settled);
export const fetchOrigin = () => activeCall()?.origin ?? 'app';
function fetchOf(input, options, call, protocols) {
  if (options?.exactStream === undefined && options?.exactBodyFrom === undefined && options?.exactBodyTo === undefined) return fetchWith(tsGrantSet, input, options, null, undefined, undefined, fetchOrigin(), false, !!activeCall()?.forced);
  if (options.exactStream !== undefined && options.exactBodyTo !== undefined) return Promise.reject(new TypeError("exactBodyTo: a stream's body is its messages, not a file"));
  let redirect;
  try { redirect = redirectOf(input, options); } catch (e) { return Promise.reject(e); }
  if (options.exactStream === undefined) {
    let init, ms, signal;
    try {
      // Keep inherited validation fields inherited, but take their values now.
      const own = ownWithout(options, ['redirect']);
      const inherited = {};
      for (const key of INHERITED) if (!Object.hasOwn(own, key)) inherited[key] = options[key];
      init = Object.assign(Object.create(inherited), own);
      ms = init.exactTimeout;
      if (ms !== undefined && (!Number.isInteger(ms) || ms < 1 || ms > 3600000)) throw new TypeError('exactTimeout must be an integer number of milliseconds from 1 to 3600000');
      signal = init.signal !== undefined ? init.signal : typeof Request === 'function' && input instanceof Request ? input.signal : null;
    } catch (e) { return Promise.reject(e); }
    const started = globalThis.performance.now();
    if (signal?.aborted) return Promise.reject(signal.reason ?? new FetchError('Aborted', 'the fetch was aborted'));
    // A render has no storage: the file is refused (ts-body-from.js `refusedFile`) after the device's checks, with no
    // deadline or later abort armed around a refusal that never answers.
    if (globalThis.__exactRender) return import('./ts-body-from.js').then(m => m.refusedFile(input, init, call));
    // The signal's deadline covers the file work too, as natively, replacing an `exactTimeout` no earlier
    const armed = signalDeadline.arm(signal, ms);
    if (armed) init.exactTimeout = ms = undefined;
    const deadline = ms === undefined ? null : AbortSignal.timeout(ms);
    let stop = () => {};
    const ended = new Promise((_, reject) => {
      const timedOut = () => reject(new FetchError('Timeout', `the request timed out after ${ms} ms`));
      const aborted = () => reject(signal.reason ?? new FetchError('Aborted', 'the fetch was aborted'));
      const heard = ownSignal(signal);
      heard?.addEventListener?.('abort', aborted, { once: true });
      deadline?.addEventListener('abort', timedOut, { once: true });
      stop = () => { heard?.removeEventListener?.('abort', aborted); deadline?.removeEventListener('abort', timedOut); };
    });
    const origin = fetchOrigin(), forced = !!activeCall()?.forced;
    return Promise.race([import('./ts-body-from.js'), ended]).finally(stop).then(m => {
      if (call?.socketConflict) throw new Error(SOCKET_AND_FETCH);
      return m.fromFile(input, init, redirect, started, origin, armed, forced);
    }).finally(() => armed?.cancel());
  }
  return options.exactTimeout !== undefined && options.exactUploadProgress !== true ? Promise.reject(new TypeError('exactTimeout: a stream has no timeout')) : stream(input, options, redirect, protocols);
}

export const files = { appId: null };

// An answer that keeps coming (LLP 1016.000), with Hermes's words
// (js/src/fetch.js): the stream is the answer's, so its fetch is made in one
// of the answer's turns (`answering`, set by ts-data.js), which opens it. Its
// promise never settles: the answer is what `exactStream` maps each event to.
// `mapper`: a socket's mapper answering a frame, in its call's synchronous part, where it may send on that socket.
// `mapperCall`: the stream whose mapper call is running, in every turn of that call: ts-stream.js sets it for the
// synchronous part, and the bundle's async helper puts it back at each step of an async function the call made
// (ts-data.js `__exact_async_context`), as Hermes's prelude carries a mapper's `currentCall`.
export const answering = { call: null, untraced: new Set(), sockets: false, mapper: null, mapperCall: null, loose: () => {}, heldReply: () => {}, ordinary: start => ordinary(start), page: null, entropy: null, later: null }; // the hooks: held.js's, set by the page's entry; `ordinary` for ts-data.js; `page`, the runtime's clock, journal, `drive` and `time`, set by ts-data.js
function stream(input, init, redirect, protocols) {
  const call = answering.call;
  // The answer's own turns, as Hermes's: its source, or a later turn of an answer still in flight (its async
  // function's steps after an `await`, its timer), whose ticket the stream then takes (ts-data.js `late`; LLP
  // 1016.000, the amendment of 2026-11-12).
  const later = !!call?.ticket && !call.turn && !!call.flight && answers(call);
  if (!call?.turn && !later) return Promise.reject(new Error('fetch() with exactStream outside an answer: a stream starts in its answer\'s own turns (its source, its code after an `await` in an async function, its timer), before the answer gives its value or is let go'));
  if (call.stream) return Promise.reject(new Error('an answer streams one request'));
  // Checked and opened by ts-stream.js, which only a streaming module loads.
  call.stream = { input, init, redirect, protocols };
  if (later) try { call.late(); } catch (e) { call.stream = null; return Promise.reject(e); }
  return new Promise(() => {});
}

// Time and seeds are source arguments (LLP 1027.000 D3): the app's modules see
// these in place of the page's Date, Math, Intl, timers and performance, each
// refusing what Hermes refuses with Hermes's words (js/src/time.js, crypto.js), so an
// app that reads the clock fails on the web as it would on a device. An
// explicit date, UTC arithmetic and an explicit timestamp's formatting work.
const refuse = api => { throw new Error(`${api} is unavailable in data sources; pass time or a random seed as an argument`); };
const noTimers = api => () => { throw new Error(`${api} is unavailable in data sources: an answer waits with setTimeout; pass time as an argument`); };
// An answer's own timers (LLP 1027.000 D1 as amended 2026-10-28), with Hermes's rules (js/src/time.js `timers`).
// Each is an entry of the page's clock (the driver's under the agent, whose `clock +N` and `clock settle` fire it)
// that goes after the tasks due with it and is no commit (rt.js `advance`). It belongs to the answer that armed it:
// one that has answered, or whose ticket was let go, never runs it. An answer with only timers out counts in no
// clock's wait (held.js `counted`), as a native host keeps its continuation back until it is due.
const Armed = new Map();
let NextTimer = 1;
function arm(api, run, ms, args) {
  if (overlaying.on) throw inOverlay(api);
  const call = answering.call;
  if (answering.mapper || answering.mapperCall || !call || !answers(call)) throw new Error(`${api} is unavailable here: a timer belongs to the answer that arms it, and none is running (module initialization, a stream's mapper, background work, or an answer that has given its value or was let go). A cache needs no timer to expire: fetch keeps each answer as long as its HTTP headers allow`);
  const { clock, drive, time } = answering.page, id = NextTimer++;
  // An async continuation may run long after the page's last commit or timer.
  time();
  // The action answers true, as every clock entry's does (rt.js `advance`: anything else stops the advance).
  // `live`: its answer has not answered nor been let go (the agent's `clock settle` goes only to those).
  const live = () => answers(call);
  const entry = { due: clock.now + ms, once: true, data: true, call, live, action() {
    disarm(id);
    if (!live()) { dropTimers(call, true); return true; }
    runAs(call, () => run(...args));
    return true;
  } };
  Armed.set(id, { call, entry }); clock.timers.push(entry);
  call.timers = (call.timers ?? 0) + 1; call.recount?.();
  if (!clock.agent) drive();
  return id;
}
// Run `run` as `call`'s turn, as its timers run: in flight until the page's next task, so the agent's jump lands what it
// started (its microtasks, the answer's continuation) before the next timer fires, as a browser runs them between tasks.
function runAs(call, run) {
  const { clock, journal } = answering.page;
  call.firing = (call.firing ?? 0) + 1; call.recount?.();
  globalThis.setTimeout(() => { call.firing--; call.recount?.(); });
  asCall(call, () => { try { run(); } catch (e) { journal.push(`t=${clock.now} data: a timer's callback threw: ${e?.message ?? e}`); } });
  // What the callback sent on a socket goes now, before the next timer of the same advance (ts-stream.js `later`).
  answering.later?.(call);
}
// `run`'s answer, with `call` the answer running while it runs
function asCall(call, run) {
  const before = answering.call;
  answering.call = call;
  try { return run(); } finally { answering.call = before; }
}
/** Whether `call`'s ticket was let go (new arguments, a refresh, a newer send, its view's end). rt.js ties the two as
 *  it makes the ticket; a `parallel` mutation holds one for each send in flight (rt.js `tickets`). */
export function letGo(call) { const t = call.ticket, o = t?.owner; return !!t && (!(o.parallel ? o.tickets.includes(t) : o.ticket === t) || o.gone); }
/** Whether `call` still answers: it has not answered, and its ticket was not let go. */
export function answers(call) { return !call.settled && !letGo(call); }
function disarm(id) {
  const armed = Armed.get(id);
  if (!armed) return;
  Armed.delete(id);
  const timers = answering.page.clock.timers, i = timers.indexOf(armed.entry);
  if (i >= 0) timers.splice(i, 1);
  armed.call.timers--; armed.call.recount?.();
}
/** Clear `call`'s timers, unrun: it has answered, or (`gone`) its ticket was let go, which ends its bookkeeping. */
export function dropTimers(call, gone) { for (const [id, armed] of Armed) if (armed.call === call) disarm(id); if (gone) call.abandon?.(); }
// A signal of the request's own that the platform aborts with `signal`, so an abort listener of the app's that stops
// the event cannot keep the request from ending, as none can keep a browser's fetch from ending (DOM, "signal abort":
// a dependent signal's event is its own); `signal` where the platform composes none
export function ownSignal(signal) {
  return signal && composeSignals ? Reflect.apply(composeSignals, PageAbortSignal, [[signal]]) : signal;
}
// A request carrying an `AbortSignal.timeout`, or a signal composed of one, takes the time the signal has left on the
// page's clock as a real-time deadline (the browser's `setTimeout`, which `clock +N` does not move), as natively
// (time.js `timers`: `left`, `expire`): the signal's own timer goes with its answer, but a POST let go runs on (LLP 1146
// D2), and another answer may carry the signal. An earlier `exactTimeout` (`before`) stays the deadline instead. A
// `Request`'s own signal gives none: it is the browser's, following the one the Request was given.
signalDeadline.arm = (signal, before) => {
  const t = signal && !signal.aborted ? Deadlines.get(signal) : undefined;
  if (t) answering.page.time();
  const left = t && Math.max(1, t.due - answering.page.clock.now);
  if (left === undefined || before < left) return undefined;
  // The signal aborts in the turn of the answer that fetched, as natively, while that answer still answers
  const call = answering.call;
  return after(left, () => {
    disarm(t.id);
    if (call && answers(call)) runAs(call, t.abort); else t.abort();
  });
};
// `run` once `ms` of real time have passed, by the page's monotonic clock, waited in steps of at most the browser
// timers' longest delay, 2^31 - 1 ms (HTML, "timer initialization steps"); answers `cancel`, after which it never
// runs, and `overdue`, which runs it at once when it is due though its timer has not fired yet
function after(ms, run) {
  const due = globalThis.performance.now() + ms;
  let id, done = false;
  const fire = () => { done = true; run(); };
  const step = () => {
    const left = due - globalThis.performance.now();
    if (left <= 0) fire();
    else id = globalThis.setTimeout(step, Math.min(left, 2147483647));
  };
  id = globalThis.setTimeout(step, Math.min(ms, 2147483647));
  return {
    cancel: () => { done = true; globalThis.clearTimeout(id); },
    overdue: () => { if (!done && globalThis.performance.now() >= due) { globalThis.clearTimeout(id); fire(); } },
  };
}
// Each `AbortSignal.timeout`'s deadline by its signal, and a composed signal's: the earliest of its sources', the one
// that aborts it first
const Deadlines = new WeakMap();
// `AbortSignal.any`'s argument as Web IDL converts a sequence, as the platform's own `any` does, so that it sees
// the same signals as an array (https://webidl.spec.whatwg.org/#create-sequence-from-iterable): the iterator method and
// `next` read once and called directly, never through a `call` of their own, each `value` read once, and each value
// refused as it is yielded unless it is a signal (the `aborted` getter's own check).
function sequence(values) {
  const method = Object(values) === values ? values[Symbol.iterator] : undefined;
  if (typeof method !== 'function') throw new TypeError('AbortSignal.any(): the signals are not iterable');
  const iterator = Reflect.apply(method, values, []), list = [];
  if (Object(iterator) !== iterator) throw new TypeError("AbortSignal.any(): the signals' iterator is not an object");
  const next = iterator.next;
  for (;;) {
    const step = Reflect.apply(next, iterator, []);
    if (Object(step) !== step) throw new TypeError('AbortSignal.any(): an iterator result is not an object');
    if (step.done) return list;
    const value = step.value;
    if (!isSignal(value)) throw new TypeError('AbortSignal.any(): a value is not an AbortSignal');
    list.push(value);
  }
}
// `AbortSignal.timeout` is an answer's timer too, and `AbortSignal.any` keeps the earliest deadline it is composed of,
// as the platform keeps its sources; the page's AbortSignal otherwise.
// The page's own, taken as this module loads: ts-data.js imports it before an app's module, so no app code has run
// to replace `AbortSignal.any`.
const PageAbortSignal = globalThis.AbortSignal, abortedOf = PageAbortSignal && Object.getOwnPropertyDescriptor(PageAbortSignal.prototype, 'aborted').get;
const composeSignals = typeof PageAbortSignal?.any === 'function' ? PageAbortSignal.any : null;
// Whether `value` is an AbortSignal, by the `aborted` getter's own check
export function isSignal(value) {
  try { Reflect.apply(abortedOf, value, []); return true; } catch { return false; }
}
const GuardedAbortSignal = PageAbortSignal && new Proxy(PageAbortSignal, { get(target, name, receiver) {
  if (name === 'any' && typeof target.any === 'function') return function any(signals) {
    const list = sequence(signals), signal = target.any(list);
    let earliest;
    for (const source of list) {
      const t = Deadlines.get(source);
      if (t && !(earliest?.due <= t.due)) earliest = t;
    }
    if (earliest) Deadlines.set(signal, earliest);
    return signal;
  };
  if (name !== 'timeout') return Reflect.get(target, name, receiver);
  return function timeout(ms) {
    const n = Number(ms);
    if (!(n >= 0 && n <= Number.MAX_SAFE_INTEGER)) throw new TypeError('AbortSignal.timeout(): the delay is a number of milliseconds from 0');
    const controller = new AbortController(), abort = () => controller.abort(new DOMException('signal timed out', 'TimeoutError'));
    const id = arm('AbortSignal.timeout()', abort, Math.floor(n), []);
    Deadlines.set(controller.signal, { id, due: Armed.get(id).entry.due, abort });
    return controller.signal;
  };
} });
// The engine's own, read through the prototypes a page's fake clock keeps
// (Playwright's `clock.install`, sinon: a Date and DateTimeFormat whose
// prototype is or inherits the real one, with no `format` accessor of its own):
// a guard over the fake would find no getter and the page would not start.
const NativeDate = globalThis.Date.prototype.constructor, NativeMath = globalThis.Math, NativeIntl = globalThis.Intl, construct = Reflect.construct;
// A function, not a Proxy of the page's Date: its instances' prototype says
// `constructor` is this one, so `new (new Date(0).constructor)()` refuses too.
function GuardedDate(...args) {
  if (!new.target) return refuse('Date()');
  if (!args.length) return refuse('new Date()');
  return construct(NativeDate, args, new.target);
}
GuardedDate.prototype = Object.create(NativeDate.prototype, { constructor: { value: GuardedDate, writable: true, configurable: true } });
Object.defineProperties(GuardedDate, {
  now: { value: () => refuse('Date.now()') }, UTC: { value: NativeDate.UTC }, parse: { value: NativeDate.parse },
  name: { value: 'Date' }, [Symbol.hasInstance]: { value: v => v instanceof NativeDate },
});
const GuardedMath = Object.freeze(Object.create(Object.getPrototypeOf(NativeMath), Object.fromEntries(Reflect.ownKeys(NativeMath).map(k => [k,
  k === 'random' ? { value: () => { throw new Error('Math.random() is unavailable in data sources; seed from crypto.getRandomValues in a source (docs/recipes/random-and-shuffle.md), or pass a seed as an argument; a roll in the view needs no source (docs/recipes/random-rolls.md)'); } }
    : Object.getOwnPropertyDescriptor(NativeMath, k)]))));
// Intl.DateTimeFormat defaults an omitted date to the clock: its `format` and
// `formatToParts` refuse one, through any alias the app can reach.
let formatProto = NativeIntl.DateTimeFormat.prototype;
while (!Object.getOwnPropertyDescriptor(formatProto, 'format')?.get) formatProto = Object.getPrototypeOf(formatProto);
const NativeFormat = formatProto.constructor, nativeFormat = Object.getOwnPropertyDescriptor(formatProto, 'format').get;
const nativeParts = NativeFormat.prototype.formatToParts, formats = new WeakMap();
function DateTimeFormat(locales, options) { return construct(NativeFormat, [locales, options], new.target ?? DateTimeFormat); }
DateTimeFormat.prototype = Object.create(NativeFormat.prototype, {
  constructor: { value: DateTimeFormat, writable: true, configurable: true },
  format: { configurable: true, get() {
    const f = nativeFormat.call(this);
    if (!formats.has(f)) formats.set(f, (...args) => args[0] === undefined ? refuse('Intl.DateTimeFormat.format()') : f(...args));
    return formats.get(f);
  } },
  formatToParts: { configurable: true, writable: true, value(...args) { return args[0] === undefined ? refuse('Intl.DateTimeFormat.formatToParts()') : nativeParts.apply(this, args); } },
});
Object.setPrototypeOf(DateTimeFormat, NativeFormat);
// A data module does no I/O of its own (LLP 1016.000 D3, D7: its socket
// belongs to a resource opened by `fetch` with `exactStream`): the page's
// XMLHttpRequest, WebSocket and EventSource refuse, as the wasm target's realm
// refuses them (host/web/module-glue.js), so no independent socket or origin
// is reached past the grants.
const noIo = api => function () { throw new Error(`${api} is unavailable in data sources`); };
// `scheduler.yield()` (LLP 1141.001 D10): the browser's where it has one, else
// a task posted on a MessageChannel, which no timer clamp delays. The page has
// no turn budget; a yield here lets it paint and answer input mid-walk.
const pageYield = globalThis.scheduler?.yield?.bind(globalThis.scheduler) ?? (typeof MessageChannel === 'function'
  ? () => new Promise(resolve => { const { port1, port2 } = new MessageChannel(); port1.onmessage = () => { port1.close(); resolve(); }; port2.postMessage(null); })
  : () => Promise.resolve());
// Code from a string runs on no host a data module runs on: Hermes compiles none, and throws `SyntaxError: Parsing
// source code unsupported` at the call. `Function` and `eval` throw that here too, so a package's guarded probe (zod's
// `allowsEval`) takes its fallback as it does on Hermes; `x instanceof Function`, `Function.prototype` and its `call`,
// `apply` and `bind` are the page's. The constructor a function's own `constructor` names is still the page's, as
// LLP 1027 D5 records. As host/web/storage-environment.js `refuseCodeFromStrings`, the wasm route's.
const codeFromString = () => { throw new SyntaxError('Parsing source code unsupported: a data source runs no code from a string (Function, eval), as on Hermes'); };
// Each has its own `constructor`, the stand-in, where a function inherits the page's real one from `Function.prototype`;
// what reaches `Function.prototype.constructor` reaches the real one (LLP 1027 D5).
const NoFunction = function Function(body) { return codeFromString(body); };
// A method, since no function may be named `eval` in strict code; as `eval`, not a constructor.
const noEval = { eval(source) { return codeFromString(source); } }.eval;
Object.defineProperty(NoFunction, 'prototype', { value: globalThis.Function.prototype });
// Fixed, so no `delete` or write uncovers the inherited one.
for (const f of [NoFunction, noEval]) Object.defineProperty(f, 'constructor', { value: NoFunction, writable: false, configurable: false });
const guarded = {
  Date: GuardedDate, Math: GuardedMath, Function: NoFunction, eval: noEval,
  XMLHttpRequest: noIo('XMLHttpRequest'), WebSocket: noIo('WebSocket'), EventSource: noIo('EventSource'),
  Intl: Object.freeze(Object.create(NativeIntl, { DateTimeFormat: { value: DateTimeFormat } })),
  setTimeout(callback, delay, ...args) {
    if (typeof callback !== 'function') throw new TypeError('setTimeout(): the callback must be a function; a string of code is never run');
    return arm('setTimeout()', callback, Math.max(Number(delay) | 0, 0), args); // `long`, as WebIDL converts it
  },
  clearTimeout: disarm, setInterval: noTimers('setInterval()'), AbortSignal: GuardedAbortSignal,
  requestAnimationFrame: noTimers('requestAnimationFrame()'), requestIdleCallback: noTimers('requestIdleCallback()'),
  clearInterval() {}, cancelAnimationFrame() {}, cancelIdleCallback() {},
  performance: Object.freeze({ now: () => refuse('performance.now()') }),
  scheduler: Object.freeze({
    yield: () => overlaying.on ? Promise.reject(inOverlay('scheduler.yield()')) : pageYield(),
    postTask() { throw new Error('scheduler.postTask() is unavailable in data sources; split long work with await scheduler.yield()'); },
  }),
  // The page's own, but no draw while an overlay runs.
  crypto: Object.freeze({ subtle: globalThis.crypto?.subtle && new Proxy(globalThis.crypto.subtle, { get(target, name) {
      const v = target[name];
      return typeof v !== 'function' ? v : (...a) => overlaying.on ? Promise.reject(inOverlay(`crypto.subtle.${String(name)}()`)) : ordinary(() => v.apply(target, a));
    } }),
    getRandomValues: a => {
      if (overlaying.on) throw inOverlay('crypto.getRandomValues()');
      // The browser's call checks the view and its quota, as it always does.
      const filled = globalThis.crypto.getRandomValues(a), bytes = seeded(filled.byteLength);
      if (bytes) new Uint8Array(filled.buffer, filled.byteOffset, filled.byteLength).set(bytes);
      return filled;
    },
    randomUUID: () => {
      if (overlaying.on) throw inOverlay('crypto.randomUUID()');
      const b = seeded(16);
      if (!b) return globalThis.crypto.randomUUID();
      // The v4 bits and text of js/src/crypto.js `formatUuid`: one seed draws one id on every host.
      b[6] = b[6] & 0x0f | 0x40; b[8] = b[8] & 0x3f | 0x80;
      return Array.from(b, (x, i) => (i === 4 || i === 6 || i === 8 || i === 10 ? '-' : '') + x.toString(16).padStart(2, '0')).join('');
    } }),
};
// Under the driver, the launch seed's stream (LLP 1069.005 D2b), which the page's entry sets before the first
// answer (`answering.entropy`); null outside it. The journal says so once, at the first draw of any bytes, as a
// native module does (js/src/door.rs, op 11).
function seeded(n) {
  const stream = answering.entropy;
  if (!stream) return null;
  if (n > 0 && !stream.said) {
    stream.said = true;
    answering.page?.journal.push(`crypto: the driver seeds this launch's randomness (seed ${stream.seed}), so each launch at that seed draws the same bytes; --seed <n> draws others (LLP 1069.005 D2b)`);
  }
  return stream(n);
}
export const { Date, Math, Intl, setTimeout, setInterval, requestAnimationFrame, requestIdleCallback, clearTimeout, clearInterval,
  cancelAnimationFrame, cancelIdleCallback, performance, scheduler, XMLHttpRequest, WebSocket, EventSource, crypto } = guarded;
// Not a binding of this module's own: its `fetch` keeps the page's `AbortSignal.timeout` for `exactTimeout`.
export { GuardedAbortSignal as AbortSignal };
// By name, since no binding of this module may be named `Function` or `eval` here; a module imports `eval` under
// another name, since no import may bind that one (host/web-js/build.mjs `scopedModule`).
export { NoFunction as Function, noEval as eval };

// The usual browser global spellings share this app-local view. Computed
// access, aliases and destructuring therefore get the same scoped fetch.
const methods = new WeakMap();
// What Hermes's realm lacks and the page has: the global object's other
// names, the DOM and Node's globals, absent from the module here too, so a
// test of one (`typeof document`) answers as it does natively
// (js/bake/src/typescript.mjs `packageRefusals`). This keeps a module
// portable; it is no security boundary (LLP 1027 D5): the module runs in the
// page's realm, and its code is the app author's.
const ABSENT = new Set(['window', 'self', 'frames', 'parent', 'top', 'opener', 'document', 'navigator', 'location', 'history', 'localStorage', 'sessionStorage',
  'indexedDB', 'HTMLElement', 'Element', 'getComputedStyle', 'customElements', 'alert', 'process', 'Buffer', 'global']);
export const absent = undefined;
// Over a target of its own, not the page's global object, whose
// non-configurable properties (`window`, `document`, `top`) a proxy over it
// could not report absent.
const page = globalThis;
// The `fetch` a source's `globalThis.fetch` is: this module's, or, for sources with no network grant, the asset path and the
// refusal alone (ts-fetch-refused.js); the data glue sets it (ts-data.js), so the request paths link only where they can be
// reached (LLP 1047.002 D3).
let Fetch = null;
// The module's own: `fetch` and `guarded`'s names (`Date`, `Function`, `eval`, …) are data properties of the proxy's
// target, so a read, write, definition, descriptor or deletion of one behaves as on any object, and never reaches the
// page, whose own code keeps its `Date` and `Function` (`globalThis.Function = globalThis.Function` would otherwise
// install the stand-in as the page's). Deleting one defines the module's original again, so a read never falls through
// to the page's; one the module made non-configurable cannot be deleted, as on any object.
const target = Object.create(null), mine = new Set(['fetch', ...Object.keys(guarded)]);
const original = name => Reflect.defineProperty(target, name, { value: name === 'fetch' ? Fetch : guarded[name], writable: true, enumerable: false, configurable: true });
for (const name of mine) original(name);
export const useFetch = f => { Fetch = f; original('fetch'); };
export const appGlobal = new Proxy(target, {
  has(own, name) { return mine.has(name) ? Reflect.has(own, name) : !ABSENT.has(name) && Reflect.has(page, name); },
  getOwnPropertyDescriptor(own, name) {
    if (mine.has(name)) return Reflect.getOwnPropertyDescriptor(own, name);
    const found = ABSENT.has(name) ? undefined : Reflect.getOwnPropertyDescriptor(page, name);
    return found && { ...found, configurable: true };
  },
  ownKeys(own) { return [...new Set([...Reflect.ownKeys(own), ...Reflect.ownKeys(page).filter(name => !ABSENT.has(name))])]; },
  getPrototypeOf() { return Object.prototype; },
  set(own, name, value, receiver) { return mine.has(name) ? Reflect.set(own, name, value, receiver) : Reflect.set(page, name, value); },
  defineProperty(own, name, descriptor) { return Reflect.defineProperty(mine.has(name) ? own : page, name, descriptor); },
  deleteProperty(own, name) {
    if (!mine.has(name)) return Reflect.deleteProperty(page, name);
    return Reflect.deleteProperty(own, name) && original(name);
  },
  get(own, name, receiver) {
    if (mine.has(name)) return Reflect.get(own, name, receiver);
    if (ABSENT.has(name)) return undefined;
    if (name === 'globalThis') return appGlobal;
    const value = Reflect.get(page, name, page);
    if (typeof value !== 'function') return value;
    if (!methods.has(value)) methods.set(value, new Proxy(value, { apply(fn, receiver, args) { return Reflect.apply(fn, receiver === appGlobal ? page : receiver, args); } }));
    return methods.get(value);
  },
});
