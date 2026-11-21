// Shared elements on the web (LLP 1013.000 D7): CSS View Transitions, the
// browser's own, for the commits that hand a `sharedElement` name from one
// element to another. Loaded after first paint with presence-glue.js, by a
// plan with a `-exact-layout-transition` (rt.js `pr`), which a pair needs at one
// end at least; until then a commit cuts.
//
// Before a commit's flush, the named elements inside each region or list
// about to rerun are the possible leavers. If there are any, the commit's
// tree update runs inside `document.startViewTransition`: the leavers get a
// `view-transition-name` before the browser captures the old state; in the
// update, after the flush, each name's new holder gets the same one, its
// group the pair's curve (the arriver's `-exact-layout-transition`, else the
// leaver's; a spring as `linear()`), and is scrolled into view. A leaver
// still there keeps its name (it moves, as the browser moves it). With no
// pair that has a curve the transition is skipped and the update still
// runs. The browser calls the update asynchronously: until it has run, later
// commits' updates wait behind it, in order.
//
// A route's own entrance and exit (LLP 1013.001) share that call. Only the
// route a push adds or a pop removes takes part, under a name it holds on one
// side of the change alone: a popped route whose exit is declared is named
// before the old state is captured, and a pushed route whose entrance is
// declared after the flush, in the new state. The root stays unnamed, so the
// page beneath is the live page; the route beneath a pushed one is held shown,
// and the pushed one inert, until the transition ends (navigation.js
// `holdRoutes`). A push in a commit that needs no transition otherwise starts
// one after its flush: the old state then names nothing, and the browser shows
// nothing of the new until it has run the update.

import { hold, within } from './focus.js';
import { rt as Link } from './rt-link.js';
import { shownStacks, holdRoutes } from './navigation.js';

const NAME = '[data-shared-element]';
let pending = null; // the tails waiting for a transition's update
let running = null; // the transition animating, if any
let style = null;
let serial = 0;

// What waits for the commits a transition holds to have run (input-glue.js: a tapped file input's own picker).
(globalThis.exact ??= {}).afterCommit = run => { if (pending) pending.after.push(run); else run(); };

/** The named elements a flush may remove: those inside each region or
 * list `queue` reruns. */
function leavers(queue) {
  const out = new Map();
  for (const n of queue) {
    if (!n.$r || n.gone) continue;
    const [a, b] = n.$r();
    if (!a || !b || a.parentNode !== b.parentNode) continue;
    for (let e = a.nextSibling; e && e !== b; e = e.nextSibling) {
      if (e.nodeType !== 1) continue;
      if (e.matches(NAME)) out.set(e, e.getAttribute('data-shared-element'));
      for (const d of e.querySelectorAll(NAME)) out.set(d, d.getAttribute('data-shared-element'));
    }
  }
  return out;
}

/** A `-exact-layout-transition` as the group's duration, delay and timing
 * function. The row is carried as the host writes it: times in
 * milliseconds without a unit (`300 0 ease`), or with one as authored, its
 * words in any ASCII case (motion/src/word.rs). */
export function curve(el) {
  const text = el?.style.getPropertyValue('--exact-layout-transition').trim().replace(/[A-Z]+/g, c => c.toLowerCase());
  if (!text || text === 'none') return null;
  const decl = text.split(/,(?![^(]*\))/).pop().trim(); // the last declaration; a spring's commas are inside it
  const times = [];
  let timing = 'ease';
  for (const t of decl.match(/(?:cubic-bezier|steps|linear|spring)\([^)]*\)|\S+/g) ?? []) {
    const ms = /^(-?[\d.]+)(ms|s)?$/.exec(t);
    if (ms) { times.push(+ms[1] * (ms[2] === 's' ? 1000 : 1)); continue; }
    if (t !== 'all') timing = t;
  }
  const delay = times[1] ?? 0;
  const spring = /^spring\(\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+))?\s*\)$/.exec(timing);
  if (spring) return { ...springCurve(+spring[1], +spring[2], +(spring[3] ?? 1)), delay };
  return times[0] > 0 ? { duration: times[0], delay, timing } : null;
}

/** A spring from rest to its target as `linear()`, over its settle time:
 * a damped harmonic oscillator, settling as the native flight's does (its
 * progress runs 0 to 1000; at rest under a thousandth in displacement and
 * speed, on a 240 Hz grid, at most ten seconds: motion/src/spring.rs). */
function springCurve(k, c, m) {
  const w0 = Math.sqrt(k / m), z = c / (2 * Math.sqrt(k * m));
  const x = t => {
    if (z < 1) { const wd = w0 * Math.sqrt(1 - z * z); return Math.exp(-z * w0 * t) * (Math.cos(wd * t) + (z * w0 / wd) * Math.sin(wd * t)); }
    if (z === 1) return Math.exp(-w0 * t) * (1 + w0 * t);
    const r1 = -w0 * (z - Math.sqrt(z * z - 1)), r2 = -w0 * (z + Math.sqrt(z * z - 1));
    return (r2 * Math.exp(r1 * t) - r1 * Math.exp(r2 * t)) / (r2 - r1);
  };
  const at = n => { const t = n / 240, h = 1e-6; return Math.abs(1000 * x(t)) < 1e-3 && Math.abs(1000 * (x(t + h) - x(Math.max(0, t - h))) / (t > h ? 2 * h : h)) < 1e-3; };
  let n = 1;
  while (n < 2400 && !at(n)) n++;
  const end = n / 240;
  const points = [];
  for (let i = 0; i <= 48; i++) points.push(+(1 - x(end * i / 48)).toFixed(4));
  points[48] = 1;
  return { duration: Math.round(end * 1000), timing: `linear(${points.join(', ')})` };
}

function ident() { return `exact-se-${++serial}`; }

const PARTS = { new: '--exact-view-transition-new', old: '--exact-view-transition-old' };
/** A route's entrance or exit (LLP 1013.001 D1): its `animation` as CSS, `none` (the author's cut), or '' when it
 * declares none. */
const part = (route, which) => route.style.getPropertyValue(PARTS[which]).trim();
const plays = css => css !== '' && css !== 'none';

/** Whether `el` is inside a region or list `queue` reruns, which may remove it. */
function rerun(queue, el) {
  for (const n of queue) {
    if (!n.$r || n.gone) continue;
    const [a, b] = n.$r();
    if (!a || !b || a.parentNode !== b.parentNode) continue;
    for (let e = a.nextSibling; e && e !== b; e = e.nextSibling) if (e === el || (e.nodeType === 1 && e.contains(el))) return true;
  }
  return false;
}

/** The eligible route changes between two `shownStacks` (LLP 1013.001 D2): at each root, one route pushed onto the
 * selected stack or popped off it, never a sheet's or a zoom's. A tab switch or a swap changes the stack otherwise. */
function routeChanges(before, after) {
  const out = [];
  for (const [nav, was] of before) {
    const now = after.get(nav) ?? [];
    let change = null;
    if (was.length && now.length === was.length + 1 && was.every((r, i) => now[i] === r)) change = { owner: now.at(-1), beneath: was.at(-1), which: 'new' };
    else if (now.length && was.length === now.length + 1 && now.every((r, i) => was[i] === r)) change = { owner: was.at(-1), beneath: now.at(-1), which: 'old' };
    if (!change || change.owner.getAttribute('navigationPresentation') === 'modal' || change.owner.getAttribute('navigationSource')) continue;
    out.push({ ...change, css: part(change.owner, change.which) });
  }
  return out;
}

/** After a commit's flush that needed no transition: a push whose route declares its entrance plays it, in a
 * transition started now. */
function entrance(before, root, inflight, after) {
  const change = routeChanges(before, shownStacks(root)).find(c => c.which === 'new' && plays(c.css));
  if (!change) return;
  if (running) { running.skipTransition(); running = null; }
  const id = ident(), { owner } = change;
  const release = holdRoutes(change.beneath, owner);
  style ??= document.head.appendChild(document.createElement('style'));
  style.textContent = `:root{view-transition-name:none}::view-transition-new(${id}){animation:${change.css}}`;
  pending = Object.assign([], { after: [] });
  inflight.n++;
  const t = document.startViewTransition(() => {
    owner.style.viewTransitionName = id;
    const tails = pending;
    pending = null;
    try { for (const f of tails) f(); } finally { for (const f of tails) if (!f.ran) f.release(); for (const f of tails.after) f(); }
  });
  running = t;
  const clean = () => {
    if (running === t) running = null;
    if (owner.style.viewTransitionName === id) owner.style.viewTransitionName = '';
    release();
  };
  const ready = t.ready.then(() => { for (const f of after ?? []) f(); }, () => {}).finally(() => inflight.n--);
  (globalThis.exact ??= {}).viewTransition = () => ready;
  t.finished.then(clean, clean);
}

/** `startViewTransition()` (LLP 1013.003): the commit's tree update `run` inside a transition of the whole page, the
 * root's own cross-fade as the browser plays it; its pushes and pops play nothing of their own. */
function crossFade(run, inflight, after) {
  if (running) { running.skipTransition(); running = null; }
  // the root takes part again: the other transitions here leave it unnamed
  if (style) style.textContent = '';
  pending = Object.assign([run], { after: [] });
  inflight.n++;
  const t = document.startViewTransition(() => {
    const tails = pending;
    pending = null;
    try { for (const f of tails) f(); } finally { for (const f of tails) if (!f.ran) f.release(); for (const f of tails.after) f(); }
  });
  running = t;
  const ready = t.ready.then(() => { for (const f of after ?? []) f(); }, () => {}).finally(() => inflight.n--);
  (globalThis.exact ??= {}).viewTransition = () => ready;
  const clean = () => { if (running === t) running = null; };
  t.finished.then(clean, clean);
  return true;
}

/** Run a commit's tree update `tail`, inside a view transition when its
 * flush may hand a name on. Returns what `tail` returns, or true when it
 * waits for the browser. */
export function commit(tail, queue, inflight, after, fade) {
  // A deferred tree update keeps the press that caused it (focus.js), so a field it mounts may still
  // take the focus from the pressed control.
  // A tool call waits for its commit's deferred update, whose commands are the call's (calls.js).
  const deferred = () => {
    const held = hold(), call = { id: Link.Invocation.id }, run = () => { run.ran = true; Link.Invocation.pending.delete(call); return within(held, tail); };
    Link.Invocation.pending.add(call);
    run.release = () => { Link.Invocation.pending.delete(call); held?.release(); tail.unrun?.(); };
    return run;
  };
  if (pending) { pending.push(deferred()); return true; }
  // Under reduce motion every change cuts (LLP 1013.000 D7, LLP 1013.001 D6).
  if (typeof document.startViewTransition !== 'function' || matchMedia('(prefers-reduced-motion: reduce)').matches) return tail();
  if (fade) return crossFade(deferred(), inflight, after);
  const root = document.getElementById('exact-root');
  const routes = root ? shownStacks(root) : new Map();
  const old = leavers(queue);
  // The top route of each root, when its exit is declared and the flush may pop it: captured under a name of its own.
  const exits = new Map();
  for (const was of routes.values()) {
    const top = was.at(-1);
    if (top && plays(part(top, 'old')) && rerun(queue, top)) exits.set(top, ident());
  }
  if (!old.size && !exits.size) {
    const result = tail();
    if (root) entrance(routes, root, inflight, after);
    return result;
  }
  const before = new Set(document.querySelectorAll(NAME)); // an arriver is new
  // A second change ends the first at its end state (LLP 1013.001 D6).
  if (running) { running.skipTransition(); running = null; }
  // Every candidate is captured under its own name; which ones left, and
  // whether a name left once and arrived once, is known after the flush.
  const ids = new Map(), curves = new Map(); // element → ident, its curve
  for (const el of old.keys()) {
    const id = ident();
    ids.set(el, id);
    curves.set(el, curve(el));
    el.style.viewTransitionName = id;
  }
  for (const [el, id] of exits) el.style.viewTransitionName = id;
  // Its own names only: a transition skipped by the next must not clear the
  // names the next just gave.
  const own = new Set([...ids.values(), ...exits.values()]);
  // Only the pairs move: the root and unpaired leavers are not shown.
  style ??= document.head.appendChild(document.createElement('style'));
  style.textContent = ':root{view-transition-name:none}';
  pending = Object.assign([deferred()], { after: [] });
  inflight.n++;
  let paired = false, routed = false, result = true;
  const rules = [], named = [...old.keys(), ...exits.keys()], releases = [];
  const t = document.startViewTransition(() => {
    const tails = pending;
    pending = null;
    // A tail that throws skips the rest; their presses are let go all the same (focus.js).
    try { result = tails.map(f => f())[0]; } finally { for (const f of tails) if (!f.ran) f.release(); for (const f of tails.after) f(); }
    // A leaver is gone, or kept only as an exit ghost (its own or an
    // ancestor's): it has left. One that stayed keeps its name and moves.
    const gone = new Map(); // name → the leavers that left
    for (const [el, name] of old) {
      const ghost = el.isConnected ? el.closest('[data-exiting]') : null;
      // Stayed: shown as it is, not moved on the browser's own curve (its
      // own layout transition, if any, moves it).
      if (el.isConnected && !ghost && el.getAttribute('data-shared-element') === name) { rules.push(`::view-transition-group(${ids.get(el)}),::view-transition-old(${ids.get(el)}),::view-transition-new(${ids.get(el)}){animation:none}`); continue; }
      gone.set(name, [...(gone.get(name) ?? []), [el, ghost]]);
    }
    for (const [name, left] of gone) {
      const arrivers = [...document.querySelectorAll(`[data-shared-element="${CSS.escape(name)}"]`)].filter(e => !before.has(e));
      const to = left.length === 1 && arrivers.length === 1 ? arrivers[0] : null;
      const [el, ghost] = left[0];
      const c = to && (curve(to) ?? curves.get(el));
      if (!c) { for (const [e] of left) rules.push(`::view-transition-group(${ids.get(e)}){display:none}`); continue; }
      // The transition is its exit (D7): a ghost of its own goes; inside an
      // ancestor's, it is hidden and the rest leaves as it would.
      if (ghost === el) el.remove(); else if (ghost) el.style.visibility = 'hidden';
      const id = ids.get(el);
      to.style.viewTransitionName = id;
      named.push(to);
      to.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'instant' });
      rules.push(`::view-transition-group(${id}),::view-transition-old(${id}),::view-transition-new(${id}){animation-duration:${c.duration}ms;animation-delay:${c.delay}ms;animation-timing-function:${c.timing}}`);
      paired = true;
    }
    // The route change, if any: its owner alone takes part (LLP 1013.001 D1).
    const changes = root ? routeChanges(routes, shownStacks(root)) : [];
    for (const [el, id] of exits) {
      // A popped route is in the old state alone, and plays its exit there.
      const popped = changes.find(c => c.which === 'old' && c.owner === el);
      if (el.isConnected) el.style.viewTransitionName = '';
      if (popped && plays(popped.css)) { rules.push(`::view-transition-old(${id}){animation:${popped.css}}`); routed = true; }
      else rules.push(`::view-transition-group(${id}){display:none}`);
    }
    for (const c of changes.filter(c => c.which === 'new' && plays(c.css))) {
      const id = ident();
      c.owner.style.viewTransitionName = id;
      named.push(c.owner);
      own.add(id);
      releases.push(holdRoutes(c.beneath, c.owner));
      rules.push(`::view-transition-new(${id}){animation:${c.css}}`);
      routed = true;
    }
    style.textContent = `:root{view-transition-name:none}${rules.join('')}`;
  });
  running = t;
  const clean = () => {
    if (running === t) running = null;
    for (const el of named) if (own.has(el.style.viewTransitionName)) el.style.viewTransitionName = '';
    for (const release of releases) release();
  };
  t.updateCallbackDone.then(() => { if (!paired && !routed) t.skipTransition(); }, () => {});
  // Its animations exist once it is ready: what runs after a commit's tree
  // (the agent's clock registers and seeks them) runs again then. The agent
  // waits for that before it moves the clock (agent.js).
  const ready = t.ready.then(() => { for (const f of after ?? []) f(); }, () => {}).finally(() => inflight.n--);
  (globalThis.exact ??= {}).viewTransition = () => ready;
  t.finished.then(clean, clean);
  return result;
}
