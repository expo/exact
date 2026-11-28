// Driver fetch faults on the web (LLP 1103): the session's table, consulted
// where each web target would send a request — the wasm host's `request`
// (http-body.js) and the JS target's `fetchWith` (admission.js) count a
// match and fail it as a refused connection; `fetchEarly` (module-glue.js)
// only declines to start one early, so a request is counted once. The table
// is the page's, on `globalThis`, because the builds copy these modules; it
// is armed by the carrier's startup script and its `faults`
// operation, and a production bake has neither.
const AGENT_ADMITTED = true; // false in a release, as the release gate writes it (agent-gate.mjs, LLP 1069.007 D2)

export function fixtureResponse(response) {
  if (!response || !Number.isInteger(response.status) || response.status < 200 || response.status > 599 || [301, 302, 303, 307, 308].includes(response.status)) throw new Error('answer fetch: status must be 200 through 599, excluding redirects');
  if (typeof response.body !== 'string' || new TextEncoder().encode(response.body).length > 64 * 1024) throw new Error('answer fetch: body must be text of at most 64 KiB');
  if ([204, 205, 304].includes(response.status) && response.body) throw new Error('answer fetch: this status takes no body');
  if (!response.headers || typeof response.headers !== 'object' || Array.isArray(response.headers)) throw new Error('answer fetch: headers must be an object of strings');
  if (Object.keys(response.headers).length > 1024) throw new Error('answer fetch: at most 1024 response headers');
  for (const [name, value] of Object.entries(response.headers)) {
    if (['set-cookie', 'set-cookie2'].includes(name.toLowerCase())) throw new Error('answer fetch: set-cookie and set-cookie2 headers are not exposed on every host');
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name) || typeof value !== 'string' || /[^\t\x20-\x7e\x80-\xff]/.test(value)) throw new Error('answer fetch: invalid header name or value');
  }
  const after = response.afterMs;
  if (after !== undefined && (!Number.isInteger(after) || after < 0 || after > 0xffffffff)) throw new Error('answer fetch: invalid delay in milliseconds');
  return { status: response.status, headers: { ...response.headers }, body: response.body, ...(after ? {afterMs: after} : {}) };
}

/** Once the body is ready, its owner waits on the session clock. Other fetches by the same
 * answer keep their original counts. An abort wakes this fetch even with the clock frozen. */
export function delayFixture(runtime, wait, due, signal) {
  const { clock } = runtime;
  if (!clock) throw new Error('a delayed fetch fixture requires the session clock');
  if (signal?.aborted) return Promise.reject(signal.reason);
  if (clock.now >= due) return Promise.resolve();
  const { call, kind, loose } = wait ?? {};
  const counted = kind && call?.[kind] > 0;
  function timer(delta) {
    if (call) {
      if (counted) call[kind] -= delta;
      call.timers = (call.timers ?? 0) + delta;
    }
    loose?.(-delta);
    call?.recount?.();
  }
  return new Promise((resolve, reject) => {
    const live = () => !call?.settled && (call?.wanted?.() ?? true);
    const entry = { due, once: true, data: true, call, live, action() { finish(); resolve(); return true; } };
    const abort = () => { finish(); reject(signal.reason); };
    function finish() {
      signal?.removeEventListener('abort', abort);
      const index = clock.timers.indexOf(entry);
      if (index >= 0) clock.timers.splice(index, 1);
      timer(-1);
    }
    clock.timers.push(entry);
    timer(1);
    signal?.addEventListener('abort', abort, { once: true });
    if (!clock.agent) runtime.drive?.();
  });
}

/** Read-only `Headers` view matching live Fetch responses (throws `TypeError` on mutation). */
export function immutableHeaders(init) {
  const raw = init instanceof Headers ? init : new Headers(init);
  const headers = new Proxy(raw, { get(target, key) {
    if (['append', 'set', 'delete'].includes(key)) return () => { throw new TypeError('Headers are immutable'); };
    if (key === 'forEach') return (callback, thisArg) => target.forEach((value, name) => callback.call(thisArg, value, name, headers));
    const value = Reflect.get(target, key, target);
    return typeof value === 'function' ? value.bind(target) : value;
  } });
  return headers;
}

/** A synthetic final response still passes the executor's normal body reader. */
export function answeredFetch(fixture, request) {
  const url = new URL(request.url), origin = globalThis.location?.origin;
  url.hash = '';
  const cross = origin !== url.origin;
  if ((cross && request.mode === 'same-origin') || request.cache === 'only-if-cached') throw new TypeError('Failed to fetch');
  const opaque = cross && request.mode === 'no-cors';
  const wrap = response => {
    const headers = immutableHeaders(response.headers);
    Object.defineProperties(response, {
      url: { value: opaque ? '' : url.href },
      type: { value: opaque ? 'opaque' : cross ? 'cors' : 'basic' },
      headers: { value: headers },
      clone: { value: () => wrap(Response.prototype.clone.call(response)), writable: true, configurable: true },
    });
    return response;
  };
  return wrap(opaque ? Response.error() : new Response(request.method === 'HEAD' || [204, 205, 304].includes(fixture.status) ? null : new TextEncoder().encode(fixture.body), fixture));
}

const BASE64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

export function encodeBase64(bytes) {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i], b1 = i + 1 < bytes.length ? bytes[i + 1] : 0, b2 = i + 2 < bytes.length ? bytes[i + 2] : 0;
    const t = (b0 << 16) | (b1 << 8) | b2;
    out += BASE64[(t >> 18) & 63] + BASE64[(t >> 12) & 63]
      + (i + 1 < bytes.length ? BASE64[(t >> 6) & 63] : '=')
      + (i + 2 < bytes.length ? BASE64[t & 63] : '=');
  }
  return out;
}

export function decodeBase64(input) {
  if (typeof input !== 'string' || input.length % 4 !== 0) throw new Error('invalid base64: length must be a multiple of 4');
  const val = ch => {
    const c = ch.charCodeAt(0);
    if (c >= 65 && c <= 90) return c - 65;
    if (c >= 97 && c <= 122) return c - 71;
    if (c >= 48 && c <= 57) return c + 4;
    if (c === 43) return 62;
    if (c === 47) return 63;
    return -1;
  };
  const out = [];
  for (let i = 0; i < input.length; i += 4) {
    const last = i + 4 === input.length;
    const a = val(input[i]), b = val(input[i + 1]);
    if (a < 0 || b < 0) throw new Error('invalid base64: unexpected character');
    if (input[i + 2] === '=') {
      if (!last || input[i + 3] !== '=') throw new Error('invalid base64: misplaced padding');
      if ((b & 0x0f) !== 0) throw new Error('invalid base64: non-zero trailing bits');
      out.push((a << 2) | (b >> 4));
    } else if (input[i + 3] === '=') {
      if (!last) throw new Error('invalid base64: misplaced padding');
      const c = val(input[i + 2]);
      if (c < 0) throw new Error('invalid base64: unexpected character');
      if ((c & 0x03) !== 0) throw new Error('invalid base64: non-zero trailing bits');
      out.push((a << 2) | (b >> 4), ((b << 4) | (c >> 2)) & 0xff);
    } else {
      const c = val(input[i + 2]), d = val(input[i + 3]);
      if (c < 0 || d < 0) throw new Error('invalid base64: unexpected character');
      out.push((a << 2) | (b >> 4), ((b << 4) | (c >> 2)) & 0xff, ((c << 6) | d) & 0xff);
    }
  }
  return new Uint8Array(out);
}

export function fixtureStream(stream) {
  const bad = msg => { throw new Error(`answer stream: ${msg}`); };
  if (!stream || typeof stream !== 'object' || Array.isArray(stream)) bad('fixture must be an object');
  const status = stream.status;
  if (!Number.isInteger(status) || status < 200 || status > 599) bad('status must be 200-299 (except 204, 205) or 204, 205, 304, 400-599');
  const streaming = status >= 200 && status <= 299 && status !== 204 && status !== 205;
  const buffered = [204, 205, 304].includes(status) || (status >= 400 && status <= 599);
  if (!streaming && !buffered) bad('redirect and 3xx statuses other than 304 are refused');
  if (!stream.headers || typeof stream.headers !== 'object' || Array.isArray(stream.headers)) bad('headers must be an object of strings');
  if (Object.keys(stream.headers).length > 1024) bad('at most 1024 response headers');
  const headers = {};
  let explicitCt = null;
  for (const [name, value] of Object.entries(stream.headers)) {
    if (['set-cookie', 'set-cookie2'].includes(name.toLowerCase()) || !/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name) || typeof value !== 'string' || /[^\t\x20-\x7e\x80-\xff]/.test(value))
      bad('invalid header name or value');
    const lower = name.toLowerCase(), trimmed = value.replace(/^[ \t]+|[ \t]+$/g, '');
    if (lower === 'content-type') explicitCt = trimmed.split(';')[0].trim().toLowerCase();
    headers[lower] = trimmed;
  }
  const parseAfter = v => {
    if (v === undefined || v === null) return 0;
    if (!Number.isInteger(v) || v < 0 || v > 0xffffffff) bad('invalid delay in milliseconds');
    return v;
  };
  if (buffered) {
    if (stream.steps !== undefined || stream.mode !== undefined) bad('buffered status cannot declare a streaming schedule');
    if (typeof stream.body !== 'string' || new TextEncoder().encode(stream.body).length > 64 * 1024) bad('buffered reply requires a string body');
    if ([204, 205, 304].includes(status) && stream.body) bad('invalid buffered body for status');
    const afterMs = parseAfter(stream.afterMs);
    return { status, headers, body: stream.body, ...(afterMs ? { afterMs } : {}) };
  }
  if (stream.body !== undefined || stream.afterMs !== undefined) bad('streaming 2xx schedule takes steps, not inline body or afterMs');
  if (stream.mode !== 'events' && stream.mode !== 'bytes') bad('mode must be "events" or "bytes"');
  if (!Array.isArray(stream.steps)) bad('streaming 2xx fixture requires a steps array');
  let prevAfter = 0, sawTerminal = false, sawEvent = false, sawBytes = false;
  const steps = [];
  for (const raw of stream.steps) {
    if (sawTerminal) bad('no step may follow end or fail');
    if (!raw || typeof raw !== 'object') bad('each step requires a kind');
    const afterMs = parseAfter(raw.afterMs);
    if (afterMs < prevAfter) bad('step afterMs values must be monotonically non-decreasing');
    prevAfter = afterMs;
    if (raw.kind === 'event') {
      if (stream.mode !== 'events' || sawBytes) bad('event steps require mode "events" and cannot mix with bytes');
      if (typeof raw.data !== 'string') bad('event step requires string data');
      if (raw.name !== undefined && raw.name !== null && typeof raw.name !== 'string') bad('event name must be a string');
      if (raw.id !== undefined && raw.id !== null && typeof raw.id !== 'string') bad('event id must be a string');
      sawEvent = true;
      steps.push({ kind: 'event', data: raw.data, ...(raw.name != null ? { name: raw.name } : {}), ...(raw.id != null ? { id: raw.id } : {}), afterMs });
    } else if (raw.kind === 'bytes') {
      if (stream.mode !== 'bytes' || sawEvent) bad('bytes steps require mode "bytes" and cannot mix with events');
      if (typeof raw.base64 !== 'string') bad('bytes step requires base64 payload');
      let decoded;
      try { decoded = decodeBase64(raw.base64); } catch (e) { bad(e.message); }
      sawBytes = true;
      steps.push({ kind: 'bytes', base64: encodeBase64(decoded), afterMs });
    } else if (raw.kind === 'end') {
      sawTerminal = true;
      steps.push({ kind: 'end', afterMs });
    } else if (raw.kind === 'fail') {
      if (typeof raw.message !== 'string') bad('fail step requires a string message');
      sawTerminal = true;
      steps.push({ kind: 'fail', message: raw.message, afterMs });
    } else {
      bad('step kind must be event, bytes, end, or fail');
    }
  }
  if (stream.mode === 'events' && explicitCt !== null && explicitCt !== 'text/event-stream') bad('event schedule requires content-type text/event-stream');
  if (stream.mode === 'bytes' && explicitCt === 'text/event-stream') bad('bytes schedule cannot use content-type text/event-stream');
  if (explicitCt === null) headers['content-type'] = stream.mode === 'events' ? 'text/event-stream' : 'application/octet-stream';
  return { status, headers, mode: stream.mode, steps };
}

const isFailureResponse = response => !response || response.status >= 400;
const isFailureEntry = f => f.stream ? f.stream.body !== undefined && f.stream.status >= 400 : isFailureResponse(f.response);

/** Launch lines carry the current table through a reload. */
export const faultSpecOf = faults => (faults ?? []).map(f => [
  f.prefix,
  f.times ?? '-',
  f.left ?? '-',
  f.hits,
  f.armed ? 1 : 0,
  f.stream ? JSON.stringify({ stream: f.stream }) : f.response ? JSON.stringify(f.response) : '',
  f.failureHits ?? (isFailureEntry(f) ? f.hits : 0),
  ...(f.method === undefined ? [] : [f.method]),
].join('\t')).join('\n');

function fixtureMethod(method, verb = 'answer fetch') {
  if (method === undefined) return undefined;
  if (typeof method !== 'string' || !/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(method)) throw new Error(`${verb}: method must be an HTTP token`);
  return method.toUpperCase();
}

function tableFits(entries) {
  // Keep room for hit counts to grow without making a later reload fail.
  const growth = entries.reduce((n, f) => n + 20 - String(f.hits).length - String(f.failureHits ?? 0).length, 0);
  if (entries.some(f => f.response || f.stream) && new TextEncoder().encode(faultSpecOf(entries)).length + growth + 1 > 65536) throw new Error('answer fetch: launch fixtures exceed 64 KiB');
}

/** Launch lines are a prefix and optional count; reload also carries left, hits, armed, response/stream and cumulative failureHits. */
export function parseFaults(spec) {
  const count = field => {
    if (field === undefined || field === '-') return null;
    if (!/^\d+$/.test(field)) throw new Error(`fail fetch: \`${field}\` is not a count`);
    return Number(field);
  };
  const entries = [];
  for (const line of String(spec).split('\n')) {
    if (!line.trim()) continue;
    const [prefix, times, left, hits, armed, payload, failures, method, ...extra] = line.split('\t');
    if (!prefix) throw new Error('fail fetch: each line names a non-empty prefix');
    const n = count(times);
    if (n === 0) throw new Error('fail fetch: `times` is a positive integer');
    const normMethod = fixtureMethod(method);
    if (extra.length || normMethod !== undefined && !payload) throw new Error('answer fetch: invalid method in launch fixture');
    let parsedResponse = null, parsedStream = null;
    if (payload) {
      const obj = JSON.parse(payload);
      if (obj && typeof obj === 'object' && obj.stream !== undefined && obj.status === undefined) {
        parsedStream = fixtureStream(obj.stream);
        if (normMethod === 'HEAD' && parsedStream.steps !== undefined) throw new Error('answer stream: explicit `method HEAD` cannot declare a streaming 2xx schedule');
      } else parsedResponse = fixtureResponse(obj);
    }
    const family = parsedStream ? 'stream' : 'fetch';
    const at = entries.findIndex(f => (f.family ?? 'fetch') === family && f.prefix === prefix && f.method === normMethod);
    if (at >= 0) entries.splice(at, 1);
    const hitCount = count(hits) ?? 0;
    const defaultFailures = parsedStream ? (parsedStream.body !== undefined && parsedStream.status >= 400 ? hitCount : 0) : (isFailureResponse(parsedResponse) ? hitCount : 0);
    entries.push({
      ...(parsedStream ? { family: 'stream' } : {}),
      prefix,
      ...(normMethod === undefined ? {} : { method: normMethod }),
      times: n,
      left: left === undefined ? n : count(left),
      hits: hitCount,
      failureHits: count(failures) ?? defaultFailures,
      armed: armed !== '0',
      ...(parsedResponse ? { response: parsedResponse } : {}),
      ...(parsedStream ? { stream: parsedStream } : {}),
    });
  }
  tableFits(entries);
  return entries;
}

function launchTable() {
  const params = new URL(globalThis.performance?.getEntriesByType?.('navigation')[0]?.name ?? globalThis.location?.href ?? 'http://x/').searchParams;
  if (!(AGENT_ADMITTED && ['', '1'].includes(params.get('agent')))) return [];
  if (!params.has('failFetch')) return [];
  const spec = globalThis.__exactLaunchFaults;
  if (typeof spec !== 'string' || !spec) throw new Error('fetch fixtures are missing from the browser carrier');
  return parseFaults(spec);
}

const state = () => globalThis.__exactFaults ??= { entries: launchTable(), log: null, pending: [] };
const live = f => f.armed && f.left !== 0;
function longest(url, answers = true, method = 'GET', family = 'fetch') {
  const upper = String(method ?? 'GET').toUpperCase();
  let best;
  for (const rule of state().entries) {
    if ((rule.family ?? 'fetch') !== family) continue;
    if (!live(rule) || !answers && rule.response || !url.startsWith(rule.prefix) || rule.method !== undefined && rule.method !== upper) continue;
    if (!best || rule.prefix.length > best.prefix.length || rule.prefix === best.prefix && rule.method !== undefined) best = rule;
  }
  return best;
}

/** Where an injected failure's journal line goes: each host's own journal; lines from before it was set follow. */
export function setFaultLog(log) { const s = state(); s.log = log; for (const line of s.pending.splice(0)) log(line); }
const say = line => { const s = state(), log = s.log ?? globalThis.__exactFaultLog; if (log) log(line); else s.pending.push(line); };

/** Select without consuming: admission and response reads can still refuse a fixture. */
export const matchFault = (url, answers = false, method = 'GET') => AGENT_ADMITTED && longest(String(url), answers, method, 'fetch');

/** Select a matching stream fixture (`answer stream`) without consuming it. */
export const matchStreamFault = (url, method = 'GET') => AGENT_ADMITTED && longest(String(url), true, method, 'stream');

/** Count the selected entry only when it supplies the request's outcome. */
export function countFault(url, entry) {
  entry.hits += 1;
  if (isFailureResponse(entry.response)) entry.failureHits = (entry.failureHits ?? 0) + 1;
  if (entry.left !== null) entry.left -= 1;
  say(entry.response ? `fetch answered (driver fixture): ${url}` : faultMessage(url));
  return entry.response ?? true;
}

/** Count a stream fixture match on the current live rule for (prefix, method) (buffered >= 400 also increments failureHits). */
export function countStreamFault(url, entry) {
  const method = entry.method === undefined ? undefined : entry.method.toUpperCase();
  const target = state().entries.find(f => f.family === 'stream' && f.prefix === entry.prefix && f.method === method) ?? entry;
  target.hits += 1;
  if (entry.stream?.body !== undefined && entry.stream.status >= 400) target.failureHits = (target.failureHits ?? 0) + 1;
  say(`stream answered (driver fixture): ${url}`);
  return entry.stream;
}

/** Record an arranged mid-stream `fail` or virtual idle timeout so `expect no failures` treats it as arranged. */
export function countStreamFailure(entry, url) {
  if (entry) entry.failureHits = (entry.failureHits ?? 0) + 1;
  say(`stream failed (driver fixture): ${url}`);
}

/** Credit a stream fixture hit and/or arranged failure against the exact `(stream, prefix, method)` ledger entry. */
export function countStreamCreditByPrefix(prefix, method, url, countHit, countFailure) {
  const upper = method == null ? undefined : String(method).toUpperCase();
  const entry = state().entries.find(f => f.family === 'stream' && f.prefix === prefix && f.method === upper);
  if (countHit) {
    if (entry) entry.hits += 1;
    say(`stream answered (driver fixture): ${url ?? prefix}`);
  }
  if (countFailure) {
    if (entry) entry.failureHits = (entry.failureHits ?? 0) + 1;
    if (!countHit) say(`stream failed (driver fixture): ${url ?? prefix}`);
  }
}

/** Credit an arranged mid-stream failure or virtual idle timeout against the exact `(stream, prefix, method)` ledger entry. */
export function countStreamFailureByPrefix(prefix, method, url) {
  countStreamCreditByPrefix(prefix, method, url, false, true);
}

/** Whether a fetch fails, counting the longest live failure prefix. */
export function takeFault(url, answers = false) {
  const entry = matchFault(url, answers);
  return entry ? countFault(url, entry) : false;
}

/** Whether a fetch of `url` would fail, without counting it: `fetchEarly` starts no GET a fault will fail. */
export const faultMatches = url => AGENT_ADMITTED && state().entries.length > 0 && longest(url, true, 'GET', 'fetch') !== undefined;

export const faultMessage = url => `fetch failed (driver fault): ${url}`;

/** The table as `state.faults` shows it. */
export const faultsJson = () => state().entries.map(f => ({ ...f }));

/** Rendered answers affected by this test, including a prefix disarmed before reload. */
export function checkpointFaults(checkpoint) {
  if (!AGENT_ADMITTED) return [];
  const entries = state().entries;
  return (checkpoint.urls ?? []).flatMap((urls, k) => urls.some(url => entries.some(f => url.startsWith(f.prefix))) ? [k] : []);
}

/** The wasm checkpoint's digest carries driver boot facts separately from the rendered bytes. */
export function drivenCheckpoint(text) {
  const split = text.indexOf('\n');
  const affected = checkpointFaults(JSON.parse(text.slice(split + 1)));
  return `${text.slice(0, split)} driven:${affected.join(',')}${text.slice(split)}`;
}

/** The agent's `faults` operation: `{fail, times?}` / `{answer}` / `{answerStream}` arms, `{pass}` / `{passStream}` disarms; the reply is the table. */
export function faultOp(req) {
  const { entries } = state();
  if (req.answerStream !== undefined) {
    if (typeof req.answerStream !== 'string' || !req.answerStream || /[\t\r\n]/.test(req.answerStream)) return { malformed: true, error: 'answer stream needs a URL prefix without tabs or newlines' };
    let stream, method;
    try {
      stream = fixtureStream(req.stream);
      method = fixtureMethod(req.method, 'answer stream');
      if (method === 'HEAD' && stream.steps !== undefined) throw new Error('answer stream: explicit `method HEAD` cannot declare a streaming 2xx schedule');
    } catch (e) { return { malformed: true, error: e.message }; }
    const same = f => f.family === 'stream' && f.prefix === req.answerStream && f.method === method;
    const next = entries.filter(f => !same(f));
    next.push({ family: 'stream', prefix: req.answerStream, ...(method === undefined ? {} : { method }), times: null, left: null, hits: 0, failureHits: entries.find(same)?.failureHits ?? 0, armed: true, stream });
    try { tableFits(next); } catch (e) { return { malformed: true, error: e.message }; }
    entries.splice(0, entries.length, ...next);
  } else if (req.passStream !== undefined) {
    const matching = entries.filter(f => f.family === 'stream' && f.prefix === req.passStream);
    if (!matching.length) return { error: `pass stream "${req.passStream}": no stream fixture was armed for it` };
    for (const rule of matching) rule.armed = false;
  } else if (req.answer !== undefined) {
    if (typeof req.answer !== 'string' || !req.answer || /[\t\r\n]/.test(req.answer)) return { malformed: true, error: 'answer fetch needs a URL prefix without tabs or newlines' };
    let response, method;
    try { response = fixtureResponse(req.response); method = fixtureMethod(req.method); } catch (e) { return { malformed: true, error: e.message }; }
    const same = f => (f.family ?? 'fetch') === 'fetch' && f.prefix === req.answer && f.method === method;
    const next = entries.filter(f => !same(f));
    next.push({ prefix: req.answer, ...(method === undefined ? {} : { method }), times: null, left: null, hits: 0, failureHits: entries.find(same)?.failureHits ?? 0, armed: true, response });
    try { tableFits(next); } catch (e) { return { malformed: true, error: e.message }; }
    entries.splice(0, entries.length, ...next);
  } else if (req.fail !== undefined) {
    if (typeof req.fail !== 'string' || !req.fail) return { malformed: true, error: 'fail fetch needs a URL prefix' };
    if (req.times !== undefined && !(Number.isInteger(req.times) && req.times >= 1 && req.times <= 0xffffffff)) return { malformed: true, error: 'fail fetch: `times` is a positive integer' };
    const same = f => (f.family ?? 'fetch') === 'fetch' && f.prefix === req.fail && f.method === undefined;
    const next = entries.filter(f => !same(f)), times = req.times ?? null;
    next.push({ prefix: req.fail, times, left: times, hits: 0, failureHits: entries.find(same)?.failureHits ?? 0, armed: true });
    try { tableFits(next); } catch (e) { return { malformed: true, error: e.message }; }
    entries.splice(0, entries.length, ...next);
  } else if (req.pass !== undefined) {
    const matching = entries.filter(f => (f.family ?? 'fetch') === 'fetch' && f.prefix === req.pass);
    if (!matching.length) return { error: `pass fetch "${req.pass}": no fault was armed for it` };
    for (const rule of matching) rule.armed = false;
  }
  return { faults: faultsJson() };
}

// The wasm host's glue loads this file after paint for the agent's `faults` (glue.js `loadAfterPaint`).
if (AGENT_ADMITTED && globalThis.exact) {
  globalThis.exact.faults = { faultOp, countStreamFailureByPrefix, countStreamCreditByPrefix };
  globalThis.exact.drivenCheckpoint = drivenCheckpoint;
}
