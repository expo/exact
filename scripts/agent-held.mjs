// The agent's waits on real time with the clock frozen, `clock held` (LLP 1016.000 D9, theme2 2026-10-22) and `clock
// until state` (rules/DEFERRED.md, "Agent API", 2026-10-26), over the session's own ops (scripts/agent.mjs), so one
// implementation serves every host.
import { MalformedOperation } from './agent-keys.mjs';

// `clock held [<target>…] [<n>s|<n>ms]`: real time passes, the clock frozen, until each held request in flight now
// (state.pending's `held`, or those named) has had one reply, a server's own timeout answer included, then `clock
// data`; past the bound (20 s) `settled: false, reason: 'held'`, naming the targets with none (LLP 1016.000 D9).
// A reply is the host's count of replies landed for a target's held requests (`state.heldReplies`) rising: a
// request let go, or no longer counted held for a while, is no reply; a reply followed at once by another held
// ask is one.
export async function heldClock(s, words) {
  let bound;
  ({ words, bound } = boundOf('clock held', words));
  // On Apple the driver takes the clock over first (platform timing), as any other clock does, so it stays frozen.
  if (['ios', 'host-ios', 'macos', 'host'].includes(s.carrier.host)) s.now = (await s.op({ op: 'clock', take: true })).clock;
  // The host's `held` op (the held asks and the replies landed for them), lighter than `state`, which a host
  // without it answers instead.
  const look = async () => { const r = await s.op({ op: 'held' }).catch(() => null); return r && Array.isArray(r.held) ? r : (st => ({ held: (st.pending ?? []).filter((p) => p.held), heldReplies: st.heldReplies }))(await s.op({ op: 'state' })); };
  const first = await look();
  const names = [...new Set(first.held.map((p) => p.name))];
  const unknown = words.filter((w) => !names.includes(w));
  if (unknown.length) throw new MalformedOperation(`clock held: ${unknown.join(', ')} ${unknown.length > 1 ? 'are' : 'is'} not a held request in flight (${names.length ? `held: ${names.join(', ')}` : 'none is held'})`);
  const targets = words.length ? [...new Set(words)] : names, before = first.heldReplies ?? {};
  // Looks back off from 25 ms to 200 ms, each wait, and each look itself, cut to what is left of the bound, so the
  // bound stays exact: a look that answers past it counts nothing.
  let left = targets, seen = first;
  const lookBy = (end) => {
    let timer;
    const late = new Promise((ok) => { timer = setTimeout(() => ok(null), Math.max(0, end - performance.now())); });
    return Promise.race([look(), late]).finally(() => clearTimeout(timer));
  };
  for (let end = performance.now() + bound, gap = 25; left.length && performance.now() < end; gap = Math.min(gap * 2, 200)) {
    await new Promise((ok) => setTimeout(ok, Math.max(0, Math.min(gap, end - performance.now()))));
    const got = await lookBy(end);
    if (!got) break;
    seen = got;
    const now = seen.heldReplies ?? {};
    left = left.filter((t) => !((now[t] ?? 0) > (before[t] ?? 0)));
  }
  // Tagged as the host tags a clock reply (its epoch and incarnation), though no host made this one.
  if (left.length) return { ...(await s.op({ op: 'tags' }).catch(() => ({}))), clock: s.now, settled: false, reason: 'held', held: seen.held.length, heldTargets: left, diagnostic: `clock held waited ${bound} ms of real time and ${left.join(', ')} had no reply: the server still holds ${left.length > 1 ? 'them' : 'it'}; give a longer bound (clock held ${Math.ceil(bound / 500)}s) or check the server` };
  const landed = await s.clock('data');
  return targets.length ? landed : { ...landed, diagnostic: 'clock held: no held request was in flight, so it waited for none' };
}

/** A wait's trailing bound, `<n>s` or `<n>ms` (20 s when none), and the words before it. */
function boundOf(what, words) {
  const last = words.at(-1), unit = /^(\d+(?:\.\d+)?)(ms|s)$/.exec(last ?? '');
  const bound = unit ? Number(unit[1]) * (unit[2] === 's' ? 1000 : 1) : 20000;
  if (!Number.isFinite(bound) || bound <= 0 || bound > 3_600_000) throw new MalformedOperation(`${what}: a bound is up to an hour, as 30s or 500ms, not ${last}`);
  return { words: unit ? words.slice(0, -1) : words, bound };
}

const UNTIL = 'clock until state <path> == <literal> within <ms> (the literal is JSON: a string, a number, true, false, null, or a list of them)';

/** `until state <path> == <literal> within <ms>`, read: the path (a state's name, then fields and list indexes), the
 *  literal as written and as JSON, and the bound. */
function untilSpec(spec) {
  // The path as an authored test writes it too: a space before a dot (`rows.0 .body`, `grid.0 .1`) is dropped.
  const m = /^until\s+state\s+(\S+(?:\s+\.\S+)*)\s*==\s*(\S.*?)\s+within\s+(\d+)$/s.exec(spec.trim());
  if (!m) throw new MalformedOperation(`${UNTIL}, not \`clock ${spec.trim()}\``);
  const path = m[1].replace(/\s+/g, ''), literal = m[2], bound = Number(m[3]);
  let want;
  try { want = JSON.parse(literal); } catch { /* refused below */ }
  const scalar = (v) => v === null || typeof v === 'string' || typeof v === 'boolean' || (typeof v === 'number' && Number.isFinite(v));
  if (want === undefined || !(scalar(want) || (Array.isArray(want) && want.every(scalar)))) throw new MalformedOperation(`${UNTIL}: ${literal} is not one`);
  if (bound < 1 || bound > 3_600_000) throw new MalformedOperation(`${UNTIL}: a bound is 1 ms up to an hour, not ${bound}`);
  const [name, ...fields] = path.split('.');
  return { path, literal, want, bound, name, fields };
}

const has = (o, k) => o !== null && typeof o === 'object' && Object.hasOwn(o, k);
const equal = (value, want) => (Array.isArray(want) ? Array.isArray(value) && JSON.stringify(value) === JSON.stringify(want) : value === want);

/** `name`, then `fields`, in a `state` reply as an authored `expect state` reads it: `{value}`, `{missing}` naming the
 *  first absent field, or `{unknown}` with the names there are. */
function stateAt(record, name, fields, describe) {
  const bag = { ...record.resources, ...record.derives, ...record.slots };
  if (!has(bag, name) && !has(record, name)) return { unknown: Object.keys(bag) };
  let got = has(bag, name) ? bag[name] : record[name], at = name;
  for (const field of fields) {
    if (!has(got, field)) return { missing: `${at} has no field "${field}" (it is ${describe(got)})` };
    got = got[field]; at += `.${field}`;
  }
  return { value: got };
}

// `clock until state <path> == <literal> within <ms>`: real time passes, the clock frozen (no timer fires), until
// `state <path>` reads the literal, for at most <ms>; a miss throws, naming the last value read. It is for state a
// stream reaches after its first message, which no settlement waits for (LLP 1016.000 D5). Each look first lands
// what has arrived, as an input's end does (`clock` with `land`: the `then`s of answers in, no timer fired), so a
// protocol's later steps go on meanwhile: Excord's IDENTIFY is sent from a write's `then`, and READY answers it.
export async function untilClock(s, spec) {
  const { path, literal, want, bound, name, fields } = untilSpec(spec);
  // Compared revealed, said masked: a password field's text is never printed (LLP 1156.001 D6). A revealed read says
  // what it misses by kind alone, never by its text.
  const kind = (v) => v === null ? 'null' : Array.isArray(v) ? `a list of ${v.length}` : typeof v === 'object' ? `a record of ${Object.keys(v).join(', ') || 'no fields'}` : `a ${typeof v}`;
  const read = async (reveal = true) => {
    const landed = reveal ? await s.op({ op: 'clock', land: true }) : null;
    if (landed?.error) throw new Error(`clock until state ${path}: ${landed.error}`);
    const record = await s.op({ op: 'state', reveal });
    return { ...stateAt(record, name, fields, reveal ? kind : (v) => JSON.stringify(v)?.slice(0, 200)), tasks: record.tasks ?? {} };
  };
  if (['ios', 'host-ios', 'macos', 'host'].includes(s.carrier.host)) s.now = (await s.op({ op: 'clock', take: true })).clock;
  // Every look races the bound: one that answers past it counts nothing, so a value is never met late.
  const t0 = performance.now(), end = t0 + bound, met = (r) => r != null && 'value' in r && equal(r.value, want);
  const look = () => {
    const r = read();
    r.catch(() => {}); // a look abandoned at the bound may fail later, unheard
    let timer;
    const late = new Promise((ok) => { timer = setTimeout(() => ok(null), Math.max(0, end - performance.now())); });
    return Promise.race([r, late]).finally(() => clearTimeout(timer));
  };
  let seen = await look();
  if (seen?.unknown) throw new MalformedOperation(`clock until state ${path}: no state named "${name}" (${seen.unknown.join(', ') || 'none declared'})`);
  while (!met(seen) && performance.now() < end) {
    await new Promise((ok) => setTimeout(ok, Math.max(0, Math.min(50, end - performance.now()))));
    if (performance.now() >= end) break;
    const got = await look();
    if (!got) break;
    seen = got;
  }
  const real = Math.round(performance.now() - t0);
  // What is said comes from one more read, masked; a value that read cannot give is not said, never taken revealed.
  const said = seen && (seen.missing || typeof seen.value === 'string' || (seen.value !== null && typeof seen.value === 'object')) ? await read(false).catch(() => null) : seen;
  const shown = said != null && 'value' in said;
  if (met(seen)) return { clock: s.now, real, until: 'met', ...(shown && { value: said.value }) };
  const last = seen == null ? 'no look answered within the bound'
    : 'value' in seen ? (shown ? `${path} was ${JSON.stringify(said.value)?.slice(0, 200)}` : `${path} could not be read masked, so its value is not said`)
    : said?.missing ?? seen.missing ?? `no state named "${name}"`;
  // A task due a commit later (`after(1, …)`) is a step the frozen clock never takes (Excord's `record`, which sends IDENTIFY).
  const soon = Object.entries(seen?.tasks ?? {}).filter(([, at]) => Number.isFinite(at) && at > s.now && at <= s.now + 1).map(([task]) => `\`${task}\``);
  const hint = soon.length ? `; ${soon.join(', ')} ${soon.length > 1 ? 'are' : 'is'} due a commit later (an \`after(1, …)\`), which the frozen clock never reaches: \`clock settle\` or \`clock +1\` before this step runs ${soon.length > 1 ? 'them' : 'it'}` : '';
  throw new Error(`clock until state ${path} == ${literal} within ${bound}: ${last} after ${real} ms of real time, the clock frozen at ${s.now} ms${hint}`);
}

/** A missed target's note on what is still out (`state.pending`): requests `clock data` lands, and held asks, whose
 * replies `clock held` waits for (LLP 1016.000 D9). */
export function inFlightNote(pending) {
  const named = (held) => [...new Set(pending.filter((p) => !!p.held === held).map((p) => p.name).filter(Boolean))];
  const waited = named(false), heldBy = named(true);
  return (waited.length ? `; ${waited.length} request${waited.length === 1 ? ' is' : 's are'} still in flight (${waited.join(', ')}): \`clock data\` waits for data, and \`state\` shows what is still pending` : '')
    + (heldBy.length ? `; ${heldBy.join(', ')} ${heldBy.length === 1 ? 'is' : 'are'} held by the server: \`clock held\` waits for ${heldBy.length === 1 ? 'its reply' : 'their replies'}` : '');
}

// An attached session's waits (LLP 1012.000 D3): the app keeps its own wall clock, which the driver never takes, seeks,
// settles or lands, so these watch it go by through passive `state` reads while its timers and work go on. One
// monotonic deadline bounds each wait from before its first read to its report; a report is built only from reads
// answered before it.

/** What the mask a password field's value is shown as reads (runner `MASKED`). */
const MASKED = '•••';
const masks = (v) => v === MASKED || (v !== null && typeof v === 'object' && Object.values(v).some(masks));

const ATTACHED_CLOCK = 'an attached app keeps its own wall clock, which is never taken, seeked, settled or landed; it waits on it with `clock until state <path> == <literal> within <ms>`, `clock held [<target>…] [<n>s|<n>ms]` and `clock replies [<n>s|<n>ms]`';

/** `clock …` on an attached session: `until`, `held` and `replies` wait on the wall clock; anything else is refused
 *  `live-clock`, as the app refuses it. */
export function attachedClock(s, spec) {
  const words = String(spec).trim().split(/\s+/);
  if (words[0] === 'until') return attachedUntil(s, String(spec));
  if (words[0] === 'held') return attachedHeld(s, words.slice(1));
  if (words[0] === 'replies') return attachedReplies(s, words.slice(1));
  const error = `live-clock: ${ATTACHED_CLOCK}`;
  return Promise.reject(Object.assign(new Error(`clock: ${error}`), { reply: { error, code: 'live-clock' } }));
}

/** A wait's deadline, `bound` ms from now on the monotonic clock. `read(req)` is one passive read answered before
 *  the deadline, or null: past the deadline, or once the connection has failed (`ended` says how). A read still out
 *  at the deadline, or answered only after it, retires the connection (`carrier.abandon`), so its reply, which would
 *  come in order, never answers a later request. */
function deadline(s, bound) {
  const t0 = performance.now(), end = t0 + bound, left = () => end - performance.now();
  const w = {
    left,
    ended: null,
    waited: () => Math.round(performance.now() - t0),
    pause: (ms) => new Promise((ok) => setTimeout(ok, Math.max(0, Math.min(ms, left())))),
    async read(req) {
      if (s.carrier.failure) { w.ended ??= s.carrier.failure.message; return null; }
      if (left() <= 0) return null;
      const r = s.op(req);
      let timer;
      const out = await Promise.race([r.then((value) => ({ value }), (error) => ({ error })), new Promise((ok) => { timer = setTimeout(() => ok(null), left()); })]).finally(() => clearTimeout(timer));
      if (out?.error) { w.ended = out.error.message; return null; }
      // A reply the event loop reached only after the deadline is not counted, as one still out is not.
      if (!out || left() < 0) {
        r.catch(() => {});
        s.carrier.abandon?.(`a ${req.op} read was not answered before the wait's ${bound} ms deadline, so the connection was closed`);
        return null;
      }
      return out.value;
    },
  };
  return w;
}

/** The app's clock when `state` was read: the wall clock the observation took, else the runner's. */
const clockOf = (st) => st.observation?.clock ?? st.clock;
/** `state.pending`, or a refusal where the app's state has none: what is in flight cannot be seen. */
function pendingOf(st, what) {
  if (Array.isArray(st.pending)) return st.pending;
  throw Object.assign(new Error(`${what}: unsupported: the app's state has no \`pending\`, so its requests in flight cannot be seen`), { reply: { code: 'unsupported' } });
}

/** `clock until state <path> == <literal> within <ms>` against the masked state, every 50 ms. A value the app masks
 *  is never compared: the literal cannot match a password field's text the session never sees. A miss says the last
 *  value read, masked as read, or that no read answered. */
async function attachedUntil(s, spec) {
  const { path, literal, want, bound, name, fields } = untilSpec(spec);
  const refuseMasked = () => { throw Object.assign(new MalformedOperation(`clock until state ${path}: masked: it holds a password field's value, which an attached session reads only as ${MASKED} and cannot compare`), { reply: { code: 'masked' } }); };
  if (masks(want)) refuseMasked();
  const w = deadline(s, bound), describe = (v) => JSON.stringify(v)?.slice(0, 200);
  const look = async () => { const record = await w.read({ op: 'state' }); return record && { ...stateAt(record, name, fields, describe), clock: clockOf(record) }; };
  let seen = await look();
  if (seen?.unknown) throw new MalformedOperation(`clock until state ${path}: no state named "${name}" (${seen.unknown.join(', ') || 'none declared'})`);
  for (;;) {
    if (seen && 'value' in seen && masks(seen.value)) refuseMasked();
    if (seen && 'value' in seen && equal(seen.value, want)) return { clock: seen.clock, real: w.waited(), until: 'met', value: seen.value };
    if (w.left() <= 0) break;
    await w.pause(50);
    const got = await look();
    if (!got) break;
    seen = got;
  }
  const last = seen == null ? 'no read answered within the bound' : 'value' in seen ? `${path} was ${describe(seen.value)}` : seen.missing ?? `no state named "${name}"`;
  throw new Error(`clock until state ${path} == ${literal} within ${bound}: ${last} after ${w.waited()} ms of the app's own wall clock${w.ended ? `; it stopped there: ${w.ended}` : ''}`);
}

/** `clock held [<target>…] [<n>s|<n>ms]`: one reply for each held request in flight at the first read (or each one
 *  named), seen as its target's count in `state.heldReplies` rising. A target that stops being held without one has
 *  had no reply. Past the bound (20 s) `settled: false`, naming the targets with none. */
async function attachedHeld(s, words) {
  let bound;
  ({ words, bound } = boundOf('clock held', words));
  const w = deadline(s, bound);
  const look = async () => { const st = await w.read({ op: 'state' }); return st && { held: pendingOf(st, 'clock held').filter((p) => p.held), replies: st.heldReplies ?? {}, clock: clockOf(st) }; };
  const first = await look();
  if (!first) return { clock: null, settled: false, reason: 'held', waitedMs: w.waited(), diagnostic: `clock held: no read answered${w.ended ? `: ${w.ended}` : ' within the bound'}` };
  const names = [...new Set(first.held.map((p) => p.name))];
  const unknown = words.filter((t) => !names.includes(t));
  if (unknown.length) throw new MalformedOperation(`clock held: ${unknown.join(', ')} ${unknown.length > 1 ? 'are' : 'is'} not a held request in flight (${names.length ? `held: ${names.join(', ')}` : 'none is held'})`);
  const targets = words.length ? [...new Set(words)] : names;
  // Own counts only: a target named `constructor` starts at 0, not at what an object inherits.
  const count = (replies, t) => (Object.hasOwn(replies, t) ? replies[t] : 0);
  let left = targets, seen = first;
  for (let gap = 25; left.length && w.left() > 0; gap = Math.min(gap * 2, 200)) {
    await w.pause(gap);
    const got = await look();
    if (!got) break;
    seen = got;
    left = left.filter((t) => !(count(seen.replies, t) > count(first.replies, t)));
  }
  if (left.length) {
    const still = left.filter((t) => seen.held.some((p) => p.name === t)), gone = left.filter((t) => !still.includes(t));
    const why = [still.length && `the server still holds ${still.join(', ')}; give a longer bound (clock held ${Math.ceil(bound / 500)}s) or check the server`,
      gone.length && `${gone.join(', ')} ${gone.length > 1 ? 'are' : 'is'} no longer held, with no reply counted`, w.ended && `the wait stopped: ${w.ended}`].filter(Boolean).join('; ');
    return { clock: seen.clock, settled: false, reason: 'held', held: seen.held.length, heldTargets: left, waitedMs: w.waited(), diagnostic: `clock held waited ${w.waited()} ms of the app's wall clock and ${left.join(', ')} had no reply: ${why}` };
  }
  return { clock: seen.clock, settled: true, held: seen.held.length, waitedMs: w.waited(), ...(targets.length ? {} : { diagnostic: 'clock held: no held request was in flight, so it waited for none' }) };
}

/** `clock replies [<n>s|<n>ms]`: the first read with no data request in flight, `{clock, settled, pending, waitedMs}`.
 *  The server's held requests, answers waiting for their own timers and held device requests are not in flight for
 *  it (`state.pending`'s `held`, `timer` and `device`). Past the bound (20 s) `settled: false` with the last read's
 *  requests and clock, both null when no read answered. */
async function attachedReplies(s, words) {
  const { words: rest, bound } = boundOf('clock replies', words);
  if (rest.length) throw new MalformedOperation(`clock replies [<n>s|<n>ms]: it waits for every data request, and names none (${rest.join(' ')})`);
  const w = deadline(s, bound);
  let last = null;
  for (;;) {
    const st = await w.read({ op: 'state' });
    if (!st) break;
    last = { clock: clockOf(st), pending: pendingOf(st, 'clock replies').filter((p) => !p.held && p.timer === undefined && p.device === undefined) };
    if (!last.pending.length) return { clock: last.clock, settled: true, pending: [], waitedMs: w.waited() };
    if (w.left() <= 0) break;
    await w.pause(50);
  }
  return { clock: last?.clock ?? null, settled: false, pending: last?.pending ?? null, waitedMs: w.waited(), ...(w.ended ? { ended: w.ended } : {}) };
}
