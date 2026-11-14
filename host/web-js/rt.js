export { px, lt, presenceUse } from './presence-bindings.js';
// Bound exits and layout transitions: presence-bindings.js's probes, which the compiler hands over only when a plan
// binds one (LLP 1047.002 D1)
export let Probes = null;
export function usePresenceBindings(b) {
  Probes = b;
}
export const presentArm = (view, ...a) => (Probes ? Probes.arm(view, ...a) : view);
export const presentRows = (row, ...a) => (Probes ? Probes.rows(row, ...a) : row);
import { resourceInspection, retainedAnswer } from './resource-inspection.js';
export { canvasColor } from './said.js';
import { rt as Link } from './rt-link.js';
export { liveTime } from './live-time.js';
import { liveById, installFits } from './navigation.js';
// synced animations (LLP 1055.002, emit.rs `clocks`)
export { animationClocks, launchLocation } from './navigation.js';
export { fontFamily } from './font-family.js';
import { Head, markDocument, projectRoots, openTabs, unopened, tp } from './document.js';
export { Head, unopened, tp };
export { canSetStatus, statusSubject } from './document.js';
import { conforms, eq, equal, Refusal, Failed } from './shape.js';
import { autofocus } from './focus.js';
// The compiler installs `Paint` only when a plan can layer boxes.
export let Paint;
export function usePaint(pass) {
  Paint = pass;
}
export { conforms, eq, equal, Refusal, Failed };
// optimistic writes shown over answers (the runner's writes.rs)
import * as Ov from './overlay.js';
import { heldAsks, heldReplies, Open, closeLetGo } from './held.js';
export { heldAsks, heldReplies };
// the agent's `state.writes`
export const writeRecords = () => Ov.Writes.list.map(w => ({ id: w.id, mutation: w.m.name, landed: !!w.landed }));
// media.js only where a plan has a `video` or `audio`; router.js's `traversal` and the page's navigation.js (the link
// a deferred tree update keeps) only where a plan has routes or a back control (router.js `navigationRoots`): a plan
// without either never links them (LLP 1047.002 D1)
export let Media = null;
export function useMedia(m) {
  Media = m;
}
let Traversal = null,
  Nav = null;
export function useTraversal(f, nav = null) {
  Traversal = f;
  Nav = nav;
}
// links.js only where a plan binds a link's `href`, `rel` or `target` (LLP 1047.002 D1)
export let Links = null;
export function useLinks(l) {
  Links = l;
}
// The runtime's other parts, each its own file, which import what they share from here.
import { clock, time, armedAt, agentMode, every, advance, drive, unpark, fire, pauseTimers } from './clock.js';
import { Resources, Mutations, lateReaders, lateIdle } from './resources.js';
import { Views } from './tree.js';
import { KeyEvent, Propagating } from './events.js';
import { Pres, Present, Sh, Input, pressFeedback } from './pieces.js';
import { Booting, start } from './mount.js';
export * from './clock.js';
export * from './resources.js';
export * from './tree.js';
export * from './events.js';
export * from './pieces.js';
export * from './mount.js';
export * from './strings.js';
// The JS runtime is imported by name, so a bundle carries only what its generated plan uses.
// Semantics kept from the runner (LLP 1005 §6):
// - an action's writes land together at its commit (the VM collects `StoreSlot`s), then one
//   synchronous flush updates the DOM, then its commands run (focus after the tree is there);
// - derives and resources settle by glitch-free, lazy pulls; equal values keep their identity, so
//   nothing downstream reruns;
// - a resource admits its compiled value only while its arguments equal the
//   compiled ones; a source not ready keeps the value and asks again when it
//   is; a later answer lands in its own commit, if its ticket is current;
// - timers fire on a clock the host moves (`advance`), each at its own time.
export let Listener = null,
  Owner = null,
  Queue = [],
  FocusQueue = [],
  Flushing = false,
  Rev = 0,
  // A store write outside a commit (module work, a let-go answer's code) owes a commit of its own (LLP 1146 D2).
  LateStore = false;
const CLEAN = 0,
  CHECK = 1,
  DIRTY = 2;
function node(fn, v, effect) {
  const n = { fn, v, effect, s: fn ? DIRTY : CLEAN, src: [], obs: new Set(), kids: null, gone: 0 };
  if (Owner) {
    (Owner.kids ??= []).push(n);
    n.up = Owner;
  }
  return n;
}
export function read(n) {
  if (Listener && !n.obs.has(Listener)) {
    n.obs.add(Listener);
    Listener.src.push(n);
  }
  if (n.fn) fresh(n);
  return n.v;
}
function fresh(n) {
  if (n.s === CHECK)
    for (const s of n.src) {
      if (s.fn) fresh(s);
      if (n.s === DIRTY) break;
    }
  if (n.s === DIRTY) run(n);
  n.s = CLEAN;
}
function drop(n) {
  for (const s of n.src) if (!s.dead) s.obs.delete(n);
  n.src = [];
  if (n.kids) {
    for (const k of n.kids) dispose(k);
    n.kids = null;
  }
}
function dispose(n) {
  n.gone = 1;
  Settle.delete(n);
  drop(n);
  if (n.ends) for (const f of n.ends.splice(0)) f();
}
/** Run `f` when the scope that owns this code ends (a region's arm or row). */
function onEnd(f) {
  if (Owner) (Owner.ends ??= []).push(f);
}
function run(n) {
  if (n.busy) throw new Refusal('a cycle: a derive or resource reads itself');
  drop(n);
  const [l, o] = [Listener, Owner];
  Listener = Owner = n;
  n.busy = 1;
  let v;
  try {
    v = n.fn(n.v);
  } finally {
    Listener = l;
    Owner = o;
    n.busy = 0;
  }
  if (!n.effect && !eq(n.v, v)) {
    n.v = v;
    for (const o of n.obs) o.s = DIRTY;
  }
}
function stale(n, state) {
  if (n.s < state) {
    if (n.s === CLEAN && n.effect) Queue.push(n);
    n.s = state;
    for (const o of n.obs) stale(o, CHECK);
  }
}
function write(n, v) {
  if (eq(n.v, v)) return;
  n.v = v;
  if (!CatchingUp) Rev++;
  for (const o of n.obs) stale(o, DIRTY);
}
/** A row's new item: an effect `fm` marked re-runs only if a field it reads changed (LLP 1071.000 D3). */
export function writeItem(n, v) {
  const o = n.v;
  if (eq(o, v)) return;
  let m = -1;
  if (Array.isArray(o) && Array.isArray(v) && o.length === v.length) {
    m = 0;
    for (let k = 0; k < v.length && k < 31; k++) if (!eq(o[k], v[k])) m |= 1 << k;
  }
  n.v = v;
  if (!CatchingUp) Rev++;
  for (const x of n.obs) if (!x.m || x.m & m) stale(x, DIRTY);
}
/** Writes in `f` publish what an earlier commit already made, to a row or arm whose region's update is still queued
 * (presence-bindings.js): they are no change of this commit's, so `rev` does not count them. */
let CatchingUp = false;
export function catchUp(f) {
  const was = CatchingUp;
  CatchingUp = true;
  try {
    return f();
  } finally {
    CatchingUp = was;
  }
}
let Mask = 0; // effects made in `fm`'s `g` read a row's item only as the fields in `m`
export function fm(m, g) {
  const o = Mask;
  Mask = m;
  try {
    g();
  } finally {
    Mask = o;
  }
}
function flush() {
  if (Flushing) return;
  Flushing = true;
  try {
    for (let i = 0; i < Queue.length; i++) if (!Queue[i].gone) fresh(Queue[i]);
  } finally {
    Queue = [];
    Flushing = false;
  }
  while (FocusQueue.length) FocusQueue.shift()();
}
function untracked(f) {
  const l = Listener;
  Listener = null;
  try {
    return f();
  } finally {
    Listener = l;
  }
}
/** A host fact a roster read depends on (`sharedFiles`' tokens, LLP 1133.024 D3): `get` tracked, `bump` in a commit
 * re-evaluates its readers. */
export function fact() {
  const o = Owner;
  Owner = null;
  const n = node(null, 0);
  Owner = o;
  return {
    get: () => read(n),
    bump() {
      n.v++;
      for (const o of n.obs) stale(o, DIRTY);
    },
  };
}
/** A slot: a getter, `.n` its node; `t` its declared type (writes conform); `name` a string slot's, which a write past
 * MAX_STRING names. An initial value outside `t`: the runner refuses the boot, or the row's creation poisons
 * (SlotType). */
export function sig(v, t, name) {
  if (t && !conforms(v, t)) throw new Error(`a slot's initial value does not conform to its type: ${v}`);
  const n = node(null, v);
  n.t = t;
  n.name = name;
  const g = () => read(n);
  g.n = n;
  return g;
}
/** A string past MAX_STRING's UTF-8 bytes (runner `too_long`): one test, TextEncoder's U+FFFD for a lone surrogate
 * (LLP 1090 D6). */
const long = s =>
  typeof s === 'string' &&
  s.length > 22369621 &&
  (s.length > 67108864 || new TextEncoder().encode(s).length > 67108864);
const tooLong = name => new Refusal(`StringTooLong { name: ${JSON.stringify(name)} }`);
const Settle = new Set();
/** A derive: lazy, cached, equal results keep their object; settled at
 * every commit before the tree; its value conforms to its type. */
export function memo(fn, t, name) {
  // Against its last value, which conformed: an unchanged part is not checked again.
  const n = node(
    t
      ? last => {
          let v;
          try {
            v = fn();
          } catch (e) {
            if (name && e?.typeRefusal) e.message = `derive ${JSON.stringify(name)}: ${e.message}`;
            throw e;
          }
          if (!conforms(v, t, [0], last)) {
            const r = n.resource;
            const error = new Refusal(
              `${r ? `resource ${JSON.stringify(r.name)} (source ${JSON.stringify(r.source)})` : `derive${name ? ` ${JSON.stringify(name)}` : ''}`}: value does not conform to its type${r?.error ? `; source failed: ${r.error}` : ''}`,
            );
            if (!r && name) {
              const seen = new Set(),
                visit = s => {
                  if (seen.has(s)) return;
                  seen.add(s);
                  const upstream = s.resource;
                  if (upstream?.error)
                    error.message += `; read resource ${JSON.stringify(upstream.name)} (source ${JSON.stringify(upstream.source)}) failed: ${upstream.error}`;
                  for (const input of s.src) visit(input);
                };
              for (const input of n.src) visit(input);
            }
            error.typeRefusal = true;
            throw error;
          }
          return v;
        }
      : fn,
  );
  Settle.add(n);
  const g = () => read(n);
  g.n = n;
  return g;
}
/** A binding probe's lazy cache; settlement leaves its errors to the tree update. */
export function bindingMemo(fn) {
  const n = node(fn);
  return () => read(n);
}
export function effect(fn) {
  const n = node(fn, undefined, 1);
  if (Mask) n.m = Mask;
  fresh(n);
  return n;
}
/** A scope whose effects `dispose` ends, owned by `parent` (a region's
 * arms and rows belong to the region's scope, never to its effect, which
 * drops what it owns each time it reruns). */
function scope(f, parent = Owner) {
  const o = Owner;
  Owner = parent;
  const n = node(null);
  Owner = n;
  try {
    f();
  } catch (e) {
    dispose(n);
    throw e;
  } finally {
    Owner = o;
  }
  return n;
}
function end(n) {
  dispose(n);
  const k = n.up?.kids;
  if (k) k.splice(k.indexOf(n), 1);
}
/** Leaving rows' scopes end, owner's kids filtered once; nothing unsubscribes from their own signals. */
export function endAll(rows) {
  let up = null;
  for (const r of rows.values())
    if (r.s) {
      r.item.n.dead = r.index.n.dead = 1;
      dispose(r.s);
      up = r.s.up;
    }
  if (up?.kids) up.kids = up.kids.filter(k => !k.gone);
}
// For loaded pieces (list.js): scopes, untracked reads, writes, the owner in
// force, and a count of what commits changed (an edge's no-op, runner/collection.rs).
export { scope, end, untracked, write, onEnd };
export const owner = () => Owner,
  rev = () => Rev,
  // Run `f` once this commit stands (each.js).
  stood = f => Out.push(f),
  restoring = () => Restoring;
// `Why`: the step the last commit was refused in and the exception's own words, cut (`why`): a boot refusal's
// reason (`mount`); the journal keeps them whole.
export let Writes = null,
  Commands = [],
  Out = [],
  Landed = [],
  Sends = [],
  Refresh = [],
  Finishes = null,
  Poisoned = false,
  Refused = null,
  Why = '',
  Sched = null;
// Cut by code point; a write's value or a source's error may be long, or private.
const why = (step, m, c = [...m]) =>
    `while ${step}: ${c.length > WHY_MAX ? `${c.slice(0, WHY_MAX).join('')}… (${c.length - WHY_MAX} more characters in the journal)` : m}`,
  WHY_MAX = 300;
/** Queued sends and gated tasks (schedule.js, LLP 1092), installed by a plan that declares them; the last commit's
 * refusal. */
export const useSchedule = s => {
    Sched = s;
  },
  refused = () => Refused;
// The runner's ring (JOURNAL_RING): `start` is the oldest line's index.
export const journal = Object.assign([], {
  start: 0,
  push(...l) {
    const over = Array.prototype.push.apply(this, l) - 4096;
    if (over > 0) this.start += this.splice(0, over).length;
    return this.length;
  },
});
export const say = line => journal.push(`t=${clock.now} ${line}`);
/** A write inside an action: collected, applied at commit. */
export function W(s, v) {
  Writes.push([s.n, v]);
  if (s.n === Assigned.n) Assigned.v = v;
}
// The router slot's last value written (router.js `assigned`).
export const Assigned = { n: null, v: undefined };
/** A host command inside an action: run after the commit. */
export function C(name, args) {
  Commands.push([name, args, Origin.now()]);
  Rev++;
}
// Who caused the commit (modules.js, LLP 1133.002 D12), taken as a command is queued, so a deferred tail runs it
// under its own commit's origin.
export const Origin = {
  now: () => Origin.ask ?? 'app',
  run(origin, f) {
    const was = this.ask;
    this.ask = origin;
    try {
      return f();
    } finally {
      this.ask = was;
    }
  },
};
/** A call's id follows its requests, replies, queued sends and `then`s (LLP 1133.023 D7); a person's commit has 0.
 * `run` and `letGo` are calls.js's once the app has tools: until then every commit is a person's. `pending` is what a
 * call waits for besides its requests, each with the call's `id`: a supersession a replacing commit has not published,
 * and a tree update of its own that a view transition defers (shared.js). */
export const Invocation = { id: 0, pending: new Set(), run: (id, f) => f(), letGo() {} };
/** `refresh r`: forced at this commit's settlement (merged, LLP 1054.000.000 D2). */
export function R(r, reasking = false) {
  Refresh.push([r.r ?? r, reasking]);
}
/** Pull every derive and resource in plan order: the settlement pass. */
function settle() {
  for (const n of Settle) if (!n.gone) fresh(n);
}
/** One commit: `f` runs; its writes, sends and refreshes land; settlement
 * runs; a refusal anywhere up to here puts everything back and leaves the
 * tree untouched; then the tree updates, completion callbacks run, requests go out, commands run. */
export function commit(f, what = 'commit', finished) {
  if (Writes) {
    if (finished) Finishes.push(finished);
    return f();
  }
  if (Poisoned) return say(`refused ${what}: the runner is poisoned; reload`);
  Probes?.capture();
  Writes = [];
  Commands = [];
  Out = [];
  Landed = [];
  Sends = [];
  Refresh = [];
  Finishes = finished ? [finished] : [];
  Refused = null;
  Why = '';
  time();
  const was = Now.v,
    undo = [],
    saved = Resources.map(r => r.save()),
    held = Mutations.map(m => [m.ticket, m.tickets, m.concluded]),
    store = Store.save(),
    queued = Sched?.save(),
    writes = Ov.save();
  let ok = true,
    // where a refusal stops it (`Why`)
    step = Booting ? 'building the view' : `running the ${what}`;
  try {
    untracked(f);
    step = 'landing its writes and sends';
    tick();
    for (const [n, v] of Writes) {
      if (n.t && !conforms(v, n.t))
        throw new Refusal(`a write does not conform to its slot's type: ${JSON.stringify(v)}`);
      if (n.name && long(v)) throw tooLong(n.name);
      undo.push([n, n.v]);
      write(n, v);
      if (n.m && !n.landing) n.m.forget(undo);
    }
    for (const [m, source, args, own] of Sends) m.send(source, args, undo, own);
    // an assignment beside a send wins
    for (const [n] of Writes) if (n.m && !n.landing && Sends.some(x => x[0] === n.m)) n.m.forget(undo);
    for (const [r, reasking] of Refresh) r.force(undo, reasking);
    // runner/src/runner/router.rs `change`
    if (!routerValid()) throw new Refusal('invalid router value');
    step = 'settling its derives and resources';
    settle();
    // A store write re-asks the resources that read the store, until quiet.
    for (let k = 0; Store.dirty && k < 4; k++) {
      Store.dirty = false;
      for (const r of Resources) if (r.store) r.revise(undo);
      settle();
    }
    // gated tasks and sequences over the settled state, inside the rollback (LLP 1092 D8, LLP 1125 D2)
    step = 'arming its tasks and sequences';
    Sched?.gates();
    Sounds.gate?.();
  } catch (e) {
    ok = false;
    for (const [n, v] of undo.reverse()) write(n, v);
    // nor its time: the clock's readers read as they did
    if (Now.v !== was) {
      Now.v = was;
      for (const o of Now.obs) stale(o, DIRTY);
    }
    // new list members end through their refusal callbacks (each.js)
    Resources.forEach((r, k) => k < saved.length && r.restore(saved[k]));
    Mutations.forEach((m, k) => {
      [m.ticket, m.tickets, m.concluded] = held[k];
    });
    Sched?.restore(queued);
    Ov.restore(writes);
    Store.restore(store);
    // what would have gone says so (ts-stream.js, ts-data.js)
    for (const go of Out) go.refused?.(e?.message ?? e);
    Out = [];
    Commands = [];
    Landed = [];
    Refused = e;
    say(`refused ${what}: ${e.message}`);
    // `String`: a TypeError's name too
    Why = why(step, e instanceof Refusal ? e.message : String(e));
    if (!(e instanceof Refusal)) console.error(e);
    Restoring = true;
    try {
      settle();
    } catch {
    } finally {
      Restoring = false;
    }
  }
  const [out, cmds, landed, finishes] = [Out, Commands, Landed, Finishes],
    changed = Sched?.changed(queued), invocation = Invocation.id;
  // A key handler's preventDefault/stopPropagation act on its event before the dispatch ends, a tree update a view
  // transition defers too (review C3).
  if (KeyEvent || Propagating)
    for (const c of cmds.filter(c => c[0] === 'preventDefault' || c[0] === 'stopPropagation')) {
      cmds.splice(cmds.indexOf(c), 1);
      command(...c);
    }
  Writes = Finishes = null;
  unpark();
  // What the commit would have sent and did not is refused, whatever stopped its tree update (a hook that threw, a
  // poisoned tree, a tail a view transition never ran): an answer's batches that wait for it too (ts-stream.js).
  let sent = 0;
  const unrun = why => {
    for (const go of out.splice(sent)) go.refused?.(why);
  };
  const link = Nav?.holdLink();
  const tail = Object.assign(
    () => {
      try {
        return Invocation.run(invocation, () => link ? Nav.withLink(link, update) : update());
      } finally {
        unrun('its tree update failed');
      }
    },
    { unrun: () => unrun('its tree update never ran') },
  );
  // the tree update; inside a view transition when it may hand on a shared element's name (LLP 1013.000 D7)
  const update = () => {
    for (const f of Before) f();
    // presence measures what it tracks, and what may gain the row, before the tree changes (LLP 1063)
    Pres?.before({ ops: Probes?.gains() ?? [] }, Views);
    try {
      flush();
      openTabs();
    } catch (e) {
      Poisoned = true;
      Why = why('updating the tree', String(e));
      // a trap as the runner's InstanceError (LLP 1090 D6)
      say(`poisoned: ${e.pc != null ? `Instance(${e.message})` : e.message}`);
      console.error(e);
      Sched?.forget();
      for (const f of finishes) f(false);
      return false;
    }
    for (const f of finishes) f(ok);
    settled(true);
    if (!ok) return (Sched?.scan(false), false);
    clock.epoch++;
    Store.persist();
    for (const r of Resources) retainedAnswer(r, say);
    while (sent < out.length) out[sent++]();
    if (Open.size) closeLetGo(inflight, say);
    for (const c of cmds) command(...c);
    Sounds.apply?.(cmds);
    Sounds.midiApply?.(cmds);
    Sounds.stood?.();
    // after its focus commands, as the wasm host (focus.js); then the focus follows the route (input-glue.js;
    // LLP 1115.000 D4.2)
    if (!Booting) {
      autofocus();
      Input?.arrive();
    }
    // An answer's `then` is armed, due now, once however many land: the next advance runs it as its own commit
    // (LLP 1016.001 D3).
    for (const m of landed)
      if (m.then) {
        m.due = armedAt();
        m.thenInvocation = invocation;
        if (!clock.agent) drive();
      }
    Sched?.scan(true, changed); // a free queue's waiting send is due (LLP 1092 D3)
    return true;
  };
  // a Back or Forward is mirrored to the history first, and one the browser animated cuts (router.js `traversal`)
  return Sh && ok && !Traversal?.() ? Sh.commit(tail, Queue, inflight, After) : tail();
}
/** A commit made again after a write ended or the source became ready (the runner's `commit_again`). Refused while
 * reads are owed (a reconciling ask), they fail, keeping what they show, before a commit publishes it; a refusal of
 * that leaves nothing owed. */
export function recommit(f, what) {
  const done = commit(f, what),
    owed = done === false ? Resources.filter(r => r.reconciling) : [],
    why = Refused?.message;
  if (owed.length) {
    for (const r of owed) r.giveUp(`the commit that asked it again was refused: ${why}`);
    commit(() => {}, 'the reads a refused commit owed');
  }
  return done;
}
/** What runs after each commit's tree update (a loaded piece's publication), before it (the text flow piece puts
 * flowed paragraphs back), and as the clock moves: before each timer or `then` fires, and where an advance lands. */
export const After = [],
  Before = [],
  Clocked = [];
/** `-exact-fit` (LLP 1116.001 D1.4): fitted texts are scaled after each tree update (navigation.js `fitText`). */
export const fitTexts = () => installFits(After);
/** Authored scroll offsets (`scrollTop`, `scrollLeft`), set once the commit's tree is in place, as the web host's
 * `pendingScrolls` (a virtualized list builds the rows there first: `$jump`, list.js); and a select's committed
 * `$value` once written or its options change, as glue.js `settleValue` (a reader's pick stands until its action). */
export const Scrolls = new Map(),
  Selects = new Set();
/** What a commit does once its tree is in place: authored scrolls, then the
 * loaded pieces' publications (also after a list's report, list.js). */
export function settled(inCommit) {
  drain();
  markDocument();
  Paint?.flush();
  Present?.();
  for (const f of After) f();
  // a list's own mounts (list.js); a commit's scan follows its commands
  if (!inCommit && !Booting) autofocus();
}
// `Restoring`: a refused commit's settle over what it put back, which asks nothing. The boot's own offsets (`Booting`,
// mount.js) are no reader's scroll (the web host hears none: its input opens after them): `scroll` skips one
export let Restoring = false;
function drain() {
  for (const [e, o] of Scrolls)
    for (const name in o) {
      const at = o[name];
      if (e[name] !== at) globalThis.exact?.fragment?.cancel(false);
      if (e.$jump) e.$jump(name, at);
      else if (e[name] !== at) {
        if (
          Booting &&
          (at < 0 ||
            Math.min(at, name === 'scrollTop' ? e.scrollHeight - e.clientHeight : e.scrollWidth - e.clientWidth) !==
              e[name])
        )
          // only an offset that moves fires
          e.$bootScroll = e.$bootScrollEnd = true;
        if (clock.agent && e.style.scrollBehavior === 'smooth')
          e.scrollTo({ [name === 'scrollTop' ? 'top' : 'left']: at, behavior: 'instant' });
        else e[name] = at;
      }
    }
  // Options compare as nodes and values: a branch that replaces them with equal values still resets the pick.
  // Reapply an equal value too: the render DOM must mark the first option selected explicitly.
  Scrolls.clear();
  for (const e of Selects)
    if (!e.isConnected) Selects.delete(e);
    else {
      const o = [...e.options],
        v = o.map(x => x.value);
      if (e.$set || o.length !== e.$options?.length || o.some((x, i) => x !== e.$options[i] || v[i] !== e.$values[i])) {
        e.$set = false;
        e.$options = o;
        e.$values = v;
        e.value = e.$value;
      }
    }
}
/** An action: each call is one commit. Its arguments conform to its parameters' types (`types`, after `skip` leading
 * arguments: a row action's row), each then within MAX_STRING when `names` names it a string's, or it is refused before
 * its body runs, as the runner's ArgumentType and StringTooLong. `.t(f)` is a handler whose arguments `f` makes inside
 * the commit (a trap refuses it, LLP 1090 D1), then the event's; on a poisoned runner `f` still runs first, as the
 * runner evaluates them before its poisoned check. */
export function act(fn, types, skip = 0, names) {
  const go = a => {
    for (let i = 0; types && i < types.length; i++) {
      if (!conforms(a[i + skip], types[i]))
        throw new Refusal(`argument ${i + 1} does not conform to its parameter's type`);
      if (names?.[i] && long(a[i + skip])) throw tooLong(names[i]);
    }
    fn(...a);
  };
  const a = (...x) => commit(() => go(x), 'action');
  a.t =
    f =>
    (...v) => {
      if (Poisoned)
        try {
          f();
        } catch (e) {
          return say(`refused action: ${e.message}`);
        }
      return commit(() => go([...f(), ...v]), 'action');
    };
  return a;
}
/** The host commands, by name; a loaded piece adds its own (list.js `scrollIntoView`). */
export const Hosts = {
  // media.js's where a plan has media; without any, the same refusal
  requestFullscreen: id =>
    Media ? Media.requestFullscreen(id) : say(`requestFullscreen: refused: no video with id "${id}"`),
  // an element's, by id (minesweeper F3); list.js takes a row's
  scrollIntoView: (id, block, inline, behavior) => {
    const e = liveById(id);
    if (e) e.scrollIntoView({ block: block ?? 'start', inline: inline ?? 'nearest', behavior: behavior ?? 'auto' });
    else say(`scrollIntoView "${id}" refused: no live node with that id`);
  },
  // LLP 1101.004 R3
  scrollBy: (id, x, y) => {
    const e = liveById(id);
    if (e) e.scrollBy(x, y);
    else say(`scrollBy "${id}" refused: no live node with that id`);
  },
  // journal lines only; marks.js replaces these when the app has launch modules
  observe: n => say(`observe ${n}`),
  observeAttributes: () => {},
  observeError: m => say(`observeError ${m}`),
};
// commands.js's, each by name as the compiled plan imported it for an action that names it (emit.rs, LLP 1047.002 D1):
// a factory over the journal and whether the agent drives the page; one no action names is never linked
export function useCommands(table) {
  for (const name in table) Hosts[name] = table[name](say, () => agentMode());
}
// A handler's `preventDefault()` and `stopPropagation()` act on its event (events.js `KeyEvent`, `Propagating`):
// commands run before its commit returns; a stopped key reaches no ancestor's `key` handler, its default still does
// (files diary F8); a prevented beforeunload is the browser's "Leave site?" (Safari reads `returnValue`)
Hosts.preventDefault = () => {
  KeyEvent?.preventDefault();
  if (KeyEvent?.type === 'beforeunload') KeyEvent.returnValue = '';
  // A pointerdown handler's: the browser keeps the focus, places no caret, starts no selection; the click still fires
  // (LLP 1005 §3).
  if (!KeyEvent && Propagating?.type === 'pointerdown') Propagating.preventDefault();
};
Hosts.stopPropagation = () => {
  const e = KeyEvent ?? Propagating;
  if (e) e.$stopped = true;
};
// The voice table's and the MIDI queue's commands (LLP 1096 D5, LLP 1125 D7): the runtime's own, never a host's;
// sounds.js and midi.js apply a commit's once it stood, and transport.js reads the sequences in the gate step and plays
// them once it stood (LLP 1125 D2).
export const Sounds = {
  apply: null,
  own: new Set(['playSound', 'playSounds', 'stopSounds', 'setSounds', 'sendMIDI', 'requestMIDIAccess']),
  gate: null,
  stood: null,
  seq: null,
  midi: null,
  midiApply: null,
  beatAt: null,
};
function command(name, args, origin) {
  const f = Hosts[name];
  say(`command ${name}`);
  if (Sounds.own.has(name)) return;
  if (Origin.refused?.(name, origin)) return;
  if (f) f.origin ? f(origin, ...args) : f(...args);
  else say(`refused: ${name} is not a command this runtime carries`);
}
// `performanceNow()` is elapsed time: the driver's clock under the agent and in a render, else the
// page's since it started; a timer commits at its due time. A reader of `performanceNow()` is
// re-evaluated at each commit made at a later time, as the runner marks the clock read dirty
// (instance/deps.rs), and never by the clock moving alone. Not a write: no commit counts it as a change.
export const Now = node(null, 0);
export function tick() {
  if (Now.v !== clock.now) {
    Now.v = clock.now;
    for (const o of Now.obs) stale(o, DIRTY);
  }
}
// The runtime's objects for lazy chunks (rt-link.js).
Object.assign(Link, {
  clock,
  every,
  commit,
  journal,
  advance,
  drive,
  wall: () => performance.now() - start,
  nextMutation: () => Mutations.reduce((due, m) => Math.min(due, m.due ?? Infinity, m.next ?? Infinity), Infinity),
  fireTimer: t => fire(t.action),
  busy: () => !!Writes,
  poisoned: () => Poisoned,
  pauseTimers,
  Invocation,
});
/** The app's data sources (LLP 1016). `answer(source, args, store)` gives `{v}` now, `{req}` (an HTTP request the host
 * runs, then `parse`s), a Promise (an executor-local continuation: TypeScript), or `null` while the source is not
 * ready; `ready(f)` calls `f` once it is. `uploads`: the uploads asked, none until ts-data.js installs uploads.js's
 * `ledger` (LLP 1016.000 D10). */
export const data = {
  answer: () => null,
  parse: null,
  q: [],
  ready: f => data.q.push(f),
  uploads: { at() {}, ends: (r, t, ...c) => c, kept: r => r.value },
};
// A development build's app directory, as storage-environment.js's (host/web-js/build.mjs writes both).
export const DEVELOPMENT_STORE = null;
/** The page's `localStorage` keys of a kind: "exact.<kind>.<name>", or in a development build
 * "exact.<kind>.@<digest>/<name>", its app's own on the origin the dev server's apps share (LLP 1116.004 D8). Persisted
 * state's kind is "state" (persist.js); the durable store's (LLP 1018, `Store`: name → text, kept after a commit
 * stands) is "secret". */
export const Kept = kind => `exact.${kind}.${DEVELOPMENT_STORE ? `@${DEVELOPMENT_STORE}/` : ''}`;
export const Store = {
  map: new Map(),
  writes: [],
  pending: new Map(),
  dirty: false,
  get(k) {
    return globalThis.__exactRender ? undefined : this.map.get(k);
  },
  exclusive(k, work) {
    return import('./storage-environment.js').then(m => m.exclusiveSecret(this, Kept('secret'), k, work));
  },
  // Outside a render, keeping what is already kept is no write, as the runner's `Store::set`.
  set(k, v) {
    if (globalThis.__exactRender) throw new Error("a render's store is empty and refuses writes"); // LLP 1048.000 D9
    if (v != null && this.map.get(k) === v) return;
    if (v == null) this.map.delete(k);
    else this.map.set(k, v);
    this.writes.push([k, v]);
    if (Writes) this.dirty = true;
    Rev++;
    if (!Writes && !LateStore) {
      LateStore = true;
      queueMicrotask(() => {
        LateStore = false;
        Store.persist();
        lateReaders();
        recommit(lateIdle, 'a store write outside a commit');
      });
    }
  }, // kept now, whatever that commit's settlement does; the commit asks the store's readers again
  save() {
    return [new Map(this.map), this.writes.length];
  },
  restore([m, n]) {
    this.map = m;
    this.writes.length = n;
    this.dirty = false;
  },
  persist() {
    const p = Kept('secret'),
      jobs = [];
    for (const [k, v] of this.writes.splice(0)) this.pending.set(k, { v, warned: false });
    for (const [k, entry] of this.pending) {
      const live = () => this.pending.get(k) === entry,
        failed = e => {
          if (!live()) return;
          if (!Object.hasOwn(entry, 'before')) this.pending.delete(k);
          if (!entry.warned) {
            entry.warned = true;
            say(`store ${k} failed: ${e}`);
          }
        },
        write = () => {
          try {
            const before = localStorage.getItem(p + k);
            if (Object.hasOwn(entry, 'before') && entry.before !== before) {
              this.pending.delete(k);
              return;
            }
            entry.before = before;
            entry.v == null ? localStorage.removeItem(p + k) : localStorage.setItem(p + k, entry.v);
            this.pending.delete(k);
          } catch (e) {
            failed(e);
          }
        };
      if (Object.hasOwn(entry, 'before') && !this.leased?.has(k)) {
        entry.retry ??= import('./storage-environment.js')
          .then(m => m.retrySecretWrite(p + k, live, write))
          .catch(failed)
          .finally(() => {
            delete entry.retry;
          });
        jobs.push(entry.retry);
      } else write();
    }
    return Promise.all(jobs);
  },
  load() {
    if (globalThis.__exactRender) return;
    const p = Kept('secret');
    try {
      for (let i = 0; i < localStorage.length; i++) {
        const k = localStorage.key(i);
        if (k.startsWith(p)) this.map.set(k.slice(p.length), localStorage.getItem(k));
      }
    } catch {}
  },
};
// A release build's DEVELOPMENT_STORE is null: it inspects nothing, and its bundle drops the inspection's code.
export const Inspections = DEVELOPMENT_STORE
  ? resourceInspection(true, () => clock.now)
  : { track() {}, begin() {}, placeholder() {}, take() {}, fail() {} };
// Requests in flight, for the agent's `clock settle`.
export const inflight = { n: 0 };
// A development build's only: the dev page's hold asks for it so, as a bundled page serves no rt.js; not on `exact`,
// which says the runtime is up.
if (DEVELOPMENT_STORE) globalThis.__exactPressFeedback = pressFeedback;
// The elements the app marks `aria-busy="true"` (still loading), by test id else id.
export const busy = () =>
  [...document.querySelectorAll('[aria-busy="true"]')].map(el => el.getAttribute('data-testid') ?? el.id);
export { gridValue } from './grid.js';
export { Pc } from './lower.js';
export { svgTransform, svgGeometry } from './svg-transform.js';
export { ds, x_querySelectorAll } from './dataset.js';
export { ht } from './hatches.js';
export { pf } from './perf.js';
export { onResize, onIntersect } from './observers.js';
export { decorationRecord, atomFacesRecord } from './shape.js';
// ---------------------------------------------------------------- the roster (runner/src/stdlib.rs)
export * from './roster.js'; // the roster's pure entries
// the router (LLP 1038), its own file
export * from './router.js';
import { routerValid } from './router.js';
export { queues, gated } from './schedule.js'; // queued sends and gated tasks (LLP 1092), their own file
export { timerWaits } from './timer-waits.js';
export { resEach } from './each.js';
