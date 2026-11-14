// Answers whose fetch the server holds (`exactHeld`, LLP 1016.000 D9): in flight, but no clock waits on them.
// A ticket (rt.js `send`) counts in `inflight` while it waits, unless its answer has only held fetches out
// (`call.held`, none of its others, `call.plain`, ts-fetch.js) and no fetch tied to no answer is out (one made
// in a plain `.then`, which the async context does not reach): then it is held, and counts again once that
// changes, for the answer's work after its held reply. A ticket its resource or mutation has let go (or whose
// resource left the screen) is no longer named, and its late reply is no reply (`heldReplies`).
// The invariant this rests on: every host facility a data source can await is counted on its answer, as a held
// fetch (`call.held`) or as ordinary work (`call.plain`, ts-fetch.js `ordinary`: fetches, crypto.subtle, native.later,
// generated module calls, kept keys, storage, openAuthSession), or in `inflight` itself (the storage queue). The
// runner hosts get this for free: an answer with more than one operation out is a continuation, never held.
const HeldAsks = new Set(), loose = { n: 0 }, Replies = new Map();
// A render's answers (rt.js `send`), while their promises are out.
const Answers = new Set();
/** In a render, whether an answer is still at work: it has I/O out (`call.plain`, `call.held`; ts-fetch.js counts
 * them) or a timer of its own while its ticket is current, or one firing. A render waits only for work, as the render
 * host does (host/render/src/lib.rs `settle`): an answer with none waits on nothing the render runs (a promise nothing
 * settles, another answer's, a storage call the render refused), and stays pending without holding the page. With
 * `discarded`, an answer whose ticket was let go counts too: its work can still write the store, and so a status. */
export function answersAtWork(discarded = false) {
  for (const t of Answers) {
    const c = t.call, live = current(t);
    if (!live && !discarded) continue;
    // A ticket with no call is a promise nothing here counts (a module answer forwarded to its implementation): still
    // in flight, so the render waits for it.
    if (!c || c.plain || c.held || c.firing || (c.timers > 0 && live)) return true;
  }
  return false;
}
/** A fetch tied to no answer is out (one in a plain `.then`): a render waits for it, as for an answer's. */
export const looseOut = () => loose.n > 0;
export function trackAnswer(ticket) {
  Answers.add(ticket);
  const ended = () => Answers.delete(ticket);
  ticket.promise.then(ended, ended);
}
// Every ticket that waits on an answer's call is in `Waiting`. Whether it counts reads its call (I/O, timers), which
// recounts it on each change (ts-fetch.js), and what no call holds, a fetch tied to no answer (`loose`), each start
// and end of which recounts every ticket in `Waiting`. A ticket with no call (a stream, a Rust request in a render)
// counts while it waits; a Rust request's call is rust-data.js's, its `held` the request's mark.
const Waiting = new Set();
/** The tickets `inflight` counts now: a wake owns those that were not counted at its delivery (wakes.js). */
export const Counting = new Set();
const current = t => !!t && t.owner?.ticket === t && !t.owner.gone;
export const heldAsks = () => [...HeldAsks].filter(current).map(t => ({ name: t.name, ticket: t.id }));
/** The held tickets themselves (wakes.js tells a stream's from an answer's). */
export const heldTickets = () => [...HeldAsks].filter(current);
/** Replies landed for held fetches, by target: `clock held`'s evidence (the runner's `held_replies`). */
export const heldReplies = () => Object.fromEntries(Replies);
export function heldReply(call) { if (current(call.ticket)) Replies.set(call.ticket.name, (Replies.get(call.ticket.name) ?? 0) + 1); }
/** A long poll's reply (LLP 1016.000 D8): from its first the stream is held and named, and each is a reply. */
export function longPollReplied(t) { if (current(t)) { HeldAsks.add(t); Replies.set(t.name, (Replies.get(t.name) ?? 0) + 1); } }
/** A long poll's end, or its letting go: no longer held. */
export function longPollEnded(t) { HeldAsks.delete(t); }
/** Open streams: one whose ticket its resource or mutation let go (new arguments, `refresh`, a failure,
 * its region ended) is closed once the commit that let it go stands, as the runner's forget path (D2). */
export const Open = new Set();
export function closeLetGo(inflight, say) {
  for (const t of Open) if (!t.held() || t.r?.gone) {
    Open.delete(t); longPollEnded(t); t.closed = true; t.ctl.abort();
    if (t.waiting) { t.waiting = false; counted(t, inflight); }
    say(`close stream ${t.id}: its ticket was let go`);
  }
}
export function counted(t, inflight) {
  if (t.call) { t.call.recount ??= () => counted(t, inflight); t.call.ticket ??= t; } // a fetch's start and end (ts-fetch.js)
  const waits = !!t.waiting && !t.closed, candidate = waits && t.call?.held > 0;
  // An answer with only its own timers out waits on the clock, not on I/O (ts-fetch.js `arm`).
  const held = candidate && !t.call.plain && !loose.n, timed = waits && !held && t.call?.timers > 0 && !t.call.plain && !t.call.firing && !loose.n;
  // In a render an answer's ticket counts nothing (`answersAtWork` says what holds the page), and nor does a stream's
  // whose body file the render refused as it opened (ts-data.js `refuseInRender`): it started no request.
  const counts = waits && !held && !timed && !(globalThis.__exactRender && (t.call || t.ctl?.refused));
  if (counts !== !!t.counted) { t.counted = counts; inflight.n += counts ? 1 : -1; }
  if (counts) Counting.add(t); else Counting.delete(t);
  if (held) HeldAsks.add(t); else HeldAsks.delete(t);
  if (waits && t.call) Waiting.add(t); else Waiting.delete(t);
}
/** A fetch tied to no answer starts (+1) or ends (-1): every waiting answer is counted again. */
export function looseFetch(delta) {
  loose.n += delta;
  for (const t of [...Waiting]) t.call.recount();
}
