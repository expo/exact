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
  return { status: response.status, headers: { ...response.headers }, body: response.body };
}

/** A synthetic final response still passes the executor's normal body reader. */
export function answeredFetch(fixture, request) {
  const url = new URL(request.url), origin = globalThis.location?.origin;
  url.hash = '';
  const cross = origin !== url.origin;
  if ((cross && request.mode === 'same-origin') || request.cache === 'only-if-cached') throw new TypeError('Failed to fetch');
  const opaque = cross && request.mode === 'no-cors';
  const wrap = response => {
    const headers = new Proxy(response.headers, { get(target, key) {
      if (['append', 'set', 'delete'].includes(key)) return () => { throw new TypeError('Headers are immutable'); };
      if (key === 'forEach') return (callback, thisArg) => target.forEach((value, name) => callback.call(thisArg, value, name, headers));
      const value = Reflect.get(target, key, target);
      return typeof value === 'function' ? value.bind(target) : value;
    } });
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

/** Launch lines carry the current table through a reload. */
export const faultSpecOf = faults => (faults ?? []).map(f => [f.prefix, f.times ?? '-', f.left ?? '-', f.hits, f.armed ? 1 : 0, f.response ? JSON.stringify(f.response) : '', f.failureHits ?? (f.response ? 0 : f.hits)].join('\t')).join('\n');

function tableFits(entries) {
  // Keep room for hit counts to grow without making a later reload fail.
  const growth = entries.reduce((n, f) => n + 20 - String(f.hits).length - String(f.failureHits ?? 0).length, 0);
  if (entries.some(f => f.response) && new TextEncoder().encode(faultSpecOf(entries)).length + growth + 1 > 65536) throw new Error('answer fetch: launch fixtures exceed 64 KiB');
}

/** Launch lines are a prefix and optional count; reload also carries left, hits, armed, response and cumulative failureHits. */
export function parseFaults(spec) {
  const count = field => {
    if (field === undefined || field === '-') return null;
    if (!/^\d+$/.test(field)) throw new Error(`fail fetch: \`${field}\` is not a count`);
    return Number(field);
  };
  const entries = [];
  for (const line of String(spec).split('\n')) {
    if (!line.trim()) continue;
    const [prefix, times, left, hits, armed, response, failures] = line.split('\t');
    if (!prefix) throw new Error('fail fetch: each line names a non-empty prefix');
    const n = count(times);
    if (n === 0) throw new Error('fail fetch: `times` is a positive integer');
    const at = entries.findIndex(f => f.prefix === prefix);
    if (at >= 0) entries.splice(at, 1);
    entries.push({ prefix, times: n, left: left === undefined ? n : count(left), hits: count(hits) ?? 0, failureHits: count(failures) ?? (response ? 0 : count(hits) ?? 0), armed: armed !== '0', ...(response ? { response: fixtureResponse(JSON.parse(response)) } : {}) });
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
const longest = (url, answers = true) => state().entries.filter(f => live(f) && (answers || !f.response) && String(url).startsWith(f.prefix)).sort((a, b) => b.prefix.length - a.prefix.length)[0];

/** Where an injected failure's journal line goes: each host's own journal; lines from before it was set follow. */
export function setFaultLog(log) { const s = state(); s.log = log; for (const line of s.pending.splice(0)) log(line); }
const say = line => { const s = state(), log = s.log ?? globalThis.__exactFaultLog; if (log) log(line); else s.pending.push(line); };

/** Select without consuming: admission and response reads can still refuse a fixture. */
export const matchFault = (url, answers = false) => AGENT_ADMITTED && longest(url, answers);

/** Count the selected entry only when it supplies the request's outcome. */
export function countFault(url, entry) {
  entry.hits += 1;
  if (!entry.response) entry.failureHits = (entry.failureHits ?? 0) + 1;
  if (entry.left !== null) entry.left -= 1;
  say(entry.response ? `fetch answered (driver fixture): ${url}` : faultMessage(url));
  return entry.response ?? true;
}

/** Whether a fetch fails, counting the longest live failure prefix. */
export function takeFault(url, answers = false) {
  const entry = matchFault(url, answers);
  return entry ? countFault(url, entry) : false;
}

/** Whether a fetch of `url` would fail, without counting it: `fetchEarly` starts no GET a fault will fail. */
export const faultMatches = url => AGENT_ADMITTED && state().entries.length > 0 && longest(url) !== undefined;

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

/** The agent's `faults` operation: `{fail, times?}` arms, `{pass}` stops failing; the reply is the table. */
export function faultOp(req) {
  const { entries } = state();
  if (req.answer !== undefined) {
    if (typeof req.answer !== 'string' || !req.answer || /[\t\r\n]/.test(req.answer)) return { malformed: true, error: 'answer fetch needs a URL prefix without tabs or newlines' };
    let response;
    try { response = fixtureResponse(req.response); } catch (e) { return { malformed: true, error: e.message }; }
    const next = entries.filter(f => f.prefix !== req.answer);
    next.push({ prefix: req.answer, times: null, left: null, hits: 0, failureHits: entries.find(f => f.prefix === req.answer)?.failureHits ?? 0, armed: true, response });
    try { tableFits(next); } catch (e) { return { malformed: true, error: e.message }; }
    entries.splice(0, entries.length, ...next);
  } else if (req.fail !== undefined) {
    if (typeof req.fail !== 'string' || !req.fail) return { malformed: true, error: 'fail fetch needs a URL prefix' };
    if (req.times !== undefined && !(Number.isInteger(req.times) && req.times >= 1 && req.times <= 0xffffffff)) return { malformed: true, error: 'fail fetch: `times` is a positive integer' };
    const next = entries.filter(f => f.prefix !== req.fail), times = req.times ?? null;
    next.push({ prefix: req.fail, times, left: times, hits: 0, failureHits: entries.find(f => f.prefix === req.fail)?.failureHits ?? 0, armed: true });
    try { tableFits(next); } catch (e) { return { malformed: true, error: e.message }; }
    entries.splice(0, entries.length, ...next);
  } else if (req.pass !== undefined) {
    const f = entries.find(f => f.prefix === req.pass);
    if (!f) return { error: `pass fetch "${req.pass}": no fault was armed for it` };
    f.armed = false;
  }
  return { faults: faultsJson() };
}

// The wasm host's glue loads this file after paint for the agent's `faults` (glue.js `loadAfterPaint`).
if (AGENT_ADMITTED && globalThis.exact) {
  globalThis.exact.faults = { faultOp };
  globalThis.exact.drivenCheckpoint = drivenCheckpoint;
}
