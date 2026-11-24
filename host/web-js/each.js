// A resource over a list on the JS target (LLP 1017.000 P4c, amended 2026-11-04; runner/src/runner/each.rs):
// `resource r = source(x) for x in list [key=k] as shape S`. Each element has its own resource (`res`), keyed as a
// row is: an element that joins makes one, which asks, unless the boot makes it from a page that answered it
// (`keptMember`); one that leaves lets only its own ticket go, once the commit that dropped it stands; the others go
// on. `r` reads their values as `S[]` in the list's order, pending or failed when any element's is.
import { res, memo, scope, end, untracked, sig, write, eq, say, stood, restoring, owner, onEnd, Resources, Mutations, Invocation, Refusal, checkpoint, booting, equal, conforms } from "./rt.js";
import { join, forget, drop } from "./overlay.js";

/** The page's answer for an element of list `name` at its arguments `args()`, while the runtime boots (LLP 1048.000
 * D6): an answer at those arguments that still fits. Elements with equal arguments make the same request, so each
 * takes an answer at those arguments, in the page's order. The page wrote the list's answers in its order, as the boot
 * makes the elements, so the answer after the last one taken is looked at first, and a scan finds an element the list
 * moved. With no answer it returns `null`, never `undefined`, which would make `res` look the list's name up as a
 * declared resource's. An instance's is the one the page's instance at `path` gave (`keptOwned`, LLP 1035.005.004 D8),
 * as a sibling at equal arguments may have shown another answer or none. */
function keptMember(name, source, args, type, path) {
  const kept = booting() && checkpoint().kept?.get(name);
  if (kept?.source !== source) {
    return null;
  }
  const at = args();
  for (let k = 0, n = kept.answers.length, i = kept.next % n; k < n; k++, i = (i + 1) % n) {
    if (kept.answers[i][4] === path && equal(kept.answers[i][0], at) && (!type || conforms(kept.answers[i][1], type))) {
      kept.next = i + 1;
      return kept.answers[i];
    }
  }
  return null;
}

/** A row's segment of an instance path from its identity `k` (rows.js `rowKey`), as the runner's owners.rs writes
 * it: its key's text, then `#<n>` for the key's nth repeat. */
export const rowPath = k => {
  const d = /^d(\d+):/.exec(k), key = d ? k.slice(d[0].length) : k;
  return key[0] + key.slice(key.indexOf(':')) + (d ? '#' + d[1] : '');
};

const keyText = v => typeof v === "string" ? "s:" + v : typeof v === "number" ? (Number.isFinite(v) ? "n:" + (v === 0 ? 0 : v) : null) : typeof v === "boolean" ? "b:" + v : null;

/** `list()` the elements, `key(x)` an element's key, `args(x)` its request's arguments; `type` the list's, `elem` an
 * element's, `zero` what an element shows until its answer. */
export function resEach(name, source, list, key, args, type, elem, zero) {
  const members = new Map(), parent = owner(); // key text → { s: its scope, g: its resource, item: its element's slot, left }
  const remove = (k, e) => {
    members.delete(k);
    const r = e.g.r;
    if (r.ticket) {
      Invocation.letGo(r.ticket);
      say(`forget ticket ${r.ticket.id} (${name})`);
      r.ticket = null;
    }
    end(e.s); Resources.splice(Resources.indexOf(r), 1); forget(r);
  };
  let order = [];
  const g = memo(() => {
    const seen = new Set(), next = [];
    for (const x of list()) {
      const k = keyText(key(x));
      if (k === null) throw new Refusal(`ResourceKey { resource: ${JSON.stringify(name)} }`);
      let e = members.get(k);
      if (!e && restoring()) continue; // a refused commit's settle asks nothing
      if (!e) {
        e = untracked(() => { const item = sig(x), e = { item }; e.s = scope(() => { e.g = res(name, source, () => e.left ? r.ticket?.args ?? r.settled : args(item()), undefined, undefined, elem, zero, false, keptMember(name, source, () => args(x), elem)); }, parent); const r = e.g.r; return e; }); // a leaving element asks nothing more
        e.g.r.family = g.r;
        e.g.r.member = true; // diagnostics key it apart from the other elements (resource-inspection.js `resourceKey`)
        join(e.g.r);
        members.set(k, e);
        // A refused join has no checkpoint to restore and no request to keep.
        const joined = () => {};
        joined.refused = () => remove(k, e);
        stood(joined);
      } else if (!seen.has(k) && !eq(e.item.n.v, x)) write(e.item.n, x); // a repeated key reads the first element's answer
      e.left = false;
      seen.add(k); next.push(e);
    }
    for (const [k, e] of members) if (!seen.has(k) && !e.left) {
      e.left = true;
      stood(() => { // let go only once the commit that dropped it stands
        if (!e.left || members.get(k) !== e) return;
        remove(k, e);
      });
    }
    order = next;
    return next.map(e => e.g());
  }, type, name);
  const each = f => { for (const e of members.values()) if (!e.left) f(e.g.r); };
  g.p = () => (g(), order.some(e => e.g.p()));
  g.c = () => (g(), order.every(e => e.g.c())); // `current(x)`: every listed element answers its own arguments (LLP 1161.001)
  g.f = () => (g(), order.some(e => e.g.f()));
  g.e = () => { g(); for (const e of order) { const f = e.g.e(); if (f) return f; } return null; };
  // What a mutation's `refreshes` and the overlay ask of a resource, asked of each element's.
  // `members`: each element's resource in the list's order, once though its key repeats: those a write over the list
  // shows in (overlay.js), and whose answers the page's checkpoint holds (checkpoint.js)
  g.r = { name, source, each: true, members: () => [...new Set(order)].map(e => e.g.r),
    force: (undo, reasking) => each(r => r.force(undo, reasking)), touch: undo => each(r => r.touch(undo)),
    reconcile: undo => each(r => r.reconcile(undo)), revise: undo => each(r => r.revise(undo)) };
  g.n.resource = g.r;
  return g;
}

/** A child's resource (LLP 1035.005.004 D1): the declared row, standing for every instance's own. `members`: the live
 * instances' resources, whose answers a page's checkpoint holds (checkpoint.js); `retired`: the asks of instances gone,
 * which `perf` still counts under the declared name, as the runner counts by row. */
export function owned(name) {
  const g = () => { throw new Refusal(`resource ${JSON.stringify(name)} is an instance's, read on its instance`); };
  const live = new Set();
  g.r = { name, declared: name.split("#")[0], instances: true, live, retired: 0, members: () => [...live] };
  return g;
}

/** Row `g`'s resource for the instance being built, `make(path)` in its scope: asked now, after the instance's state,
 * and let go with the instance, its ticket forgotten so a late reply changes nothing (D2, D4). `path` names the instance
 * (the agent's `instances`). */
export function own(g, make, path) {
  const m = make(path), r = m.r;
  r.declared = g.r.declared;
  r.path = path;
  g.r.live.add(r);
  onEnd(() => {
    g.r.live.delete(r);
    g.r.retired += r.asks ?? 0;
    if (r.ticket) {
      Invocation.letGo(r.ticket);
      say(`forget ticket ${r.ticket.id} (${r.name})`);
      r.ticket = null;
    }
    const k = Resources.indexOf(r);
    if (k >= 0) Resources.splice(k, 1);
  });
  untracked(m);
  return m;
}

/** A child's mutation row (LLP 1035.005.004 D6): each instance of its arm sends on its own (`ownMut`), under the name
 * its component declares. */
export function ownedMut(name) {
  return { declared: name.split("#")[0], live: new Set() };
}

/** Row `g`'s mutation for the instance being built: `make(name)` in its scope, after the instance's state and
 * requests, its `then` the instance's (`then`, run in the instance's frames). The instance's end lets it go: its
 * tickets are forgotten (a send already made stays made, and its reply changes nothing), its waiting sends and their
 * writes are dropped, and its `then` does not run (D4). */
export function ownMut(g, make, then) {
  const m = make(g.declared);
  if (m.queue) {
    m.wait = [];
    m.stall = null;
  }
  m.then = then || null;
  g.live.add(m);
  onEnd(() => {
    g.live.delete(m);
    for (const t of m.ticket ? [m.ticket] : m.tickets) {
      Invocation.letGo(t, true);
      say(`forget ticket ${t.id} (${m.name})`);
    }
    const n = m.wait?.length ?? 0;
    if (n) say(`forgot ${n} waiting send${n === 1 ? "" : "s"} (${m.name})`);
    Object.assign(m, { ticket: null, tickets: [], wait: m.queue ? [] : m.wait, stall: null, then: null, due: Infinity, next: Infinity });
    drop(m);
    const k = Mutations.indexOf(m);
    if (k >= 0) Mutations.splice(k, 1);
    // the agent's `mutationRequests` counts every send made, as the runner's does
    Mutations.retired = (Mutations.retired ?? 0) + m.requests;
  });
  return m;
}

export { keptMember as keptOwned };
