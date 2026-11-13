// The runtime's clock (LLP 1005 §6): timers fire on a clock the host moves (`advance`), each at its own time; on
// the wall clock `drive` and `paint` move it, under the agent the driver does.
import { rt as Link } from './rt-link.js';
import { beatAfter } from './beat.js';
import { commit, journal, say, Poisoned, Rev, Sched, Clocked, Writes, Listener, Now, read } from './rt.js';
import { Mutations, Ticket } from './resources.js';
import { start } from './mount.js';
export const clock = { now: 0, timers: [], agent: false, epoch: 0 };
let Timing = false,
  Instant = null;
// Where a `then` or queue `next` armed now is due: on the wall clock, a callback's own place (the timers RFC, D2).
export const armedAt = () => Instant ?? clock.now;
// A commit takes its time first (`time`); its clock readers see it once the body has run (`tick`): the body reads the
// pre-state.
export function time() {
  if (!clock.agent && !Timing && start) clock.now = Math.max(clock.now, performance.now() - start);
}
// A release build never enters agent mode (LLP 1069.007 D2): its build writes this false, as the wasm host's files
// are gated.
export const AGENT_ADMITTED = true;
/** Whether the agent drives this page (or a render does): the clock is then the driver's. */
export const agentMode = () =>
  !!globalThis.__exactRender || (AGENT_ADMITTED && new URLSearchParams(location.search).has('agent'));
let Order = 0;
export const order = () => Order++,
  // every task, for the agent's `state.tasks`
  Tasks = [];
/** A timer: `every(ms, action, once)`, due from mount; `i` its plan order, which breaks ties (schedule.js inserts by
 * it). */
export function every(ms, action, once, name, wait = 0, max = 0, tolerance = 0) {
  clock.timers.push({ due: clock.now + ms, ms, action, once, name, wait, max, tolerance, i: Order++ });
  Tasks.push(clock.timers.at(-1));
  if (!clock.agent) drive();
}
// A frame task (LLP 1073): once per presented frame, never caught up; on the
// agent's seekable clock, a virtual frame every 1000/60 ms after it last fired.
// The kth virtual frame after `base`, the product first, as the runner's
// `virtual_frame`: sixty frames are exactly a second.
const vf = (base, k) => base + (k * 1000) / 60;
/** `every(frame, action)`. */
export function frames(action, name) {
  clock.timers.push({ due: vf(clock.now, 1), base: clock.now, k: 1, frame: true, action, name, i: Order++ });
  Tasks.push(clock.timers.at(-1));
  if (!clock.agent) paint();
}
/** Move the clock to `to`, firing each due timer and armed `then` at its own time, in order; a seek fires frame tasks'
 * virtual frames too, the wall clock's (`wall`) none. `stop()`, asked after each, ends it there (the agent's: one that
 * sent a request): true. A change receipt boundary also yields true; resume toward the same `to`. A refusal or 4096
 * commits stops there; a non-finite `to` (NonFiniteClock) leaves the clock where it was: its journal line, as the
 * runner's error. Under the agent the journal gets the runner's line for an advance that fired. `timers` false runs
 * only armed `then`s and change actions, at `to` = now, as an agent's input ends (Runner::land_then; trivia F3). */
export function advance(to, wall, stop, timers = true) {
  if (!Number.isFinite(to)) return (say(`refused advance: NonFiniteClock (${to})`), journal.at(-1));
  if (Poisoned) return (say('refused advance: Poisoned'), journal.at(-1));
  to = Link.timerWait?.cutoff(to, wall) ?? to;
  let fired = 0,
    stopped = false;
  for (;;) {
    let next = null,
      then = null,
      head = null;
    const execution = t => (t.change ? Math.max(t.due, clock.now) : (Link.timerWait?.execution(t, wall) ?? t.due)),
      priority = t => (t.hatch || t.data ? 2 : t.wait === 4 ? 1 : 0);
    // Run authored tasks before hatch and data callbacks due at the same time
    if (!clock.stopped)
      for (const t of clock.timers)
        if (
          (timers || t.change) &&
          (!t.change || t.ready) &&
          execution(t) <= to &&
          !(wall && t.frame) &&
          (!next ||
            execution(t) < execution(next) ||
            (execution(t) === execution(next) &&
              (priority(t) < priority(next) || (priority(t) === priority(next) && t.due < next.due))))
        )
          next = t;
    // An answer's `then` goes before a timer due at the same time: the answer landed first; a queue's `next` between
    // them (LLP 1092 D3).
    for (const m of Mutations)
      if (m.due <= to && (!then || m.due < then.due) && (!next || m.due <= execution(next))) then = m;
    for (const m of Mutations)
      if (
        m.next <= to &&
        (!head || m.next < head.next) &&
        (!next || m.next <= execution(next)) &&
        (!then || m.next < then.due)
      )
        head = m;
    if (head) then = null;
    if (!next && !then && !head) break;
    if (next?.change && !then && !head && fired) return true;
    if (fired === 4096) return (say('refused advance: 4096 commits in one advance (TIMER_FIRE_LIMIT)'), journal.at(-1));
    // On the wall clock (the timers RFC, D2, D3) a repeat that missed beats fires once, its firing consumed first; each
    // callback observes the clock as it runs and as it ends, and what it arms takes its place; a repeat's next beat is
    // the first of its phase after the end, unless its callback dropped it, or re-armed it (a gate's step) from there.
    const at = head ? head.next : then ? then.due : next.due,
      repeat = (wall || next?.wait) && !head && !then && !next.once && !next.frame && !next.hatch,
      gen = next?.gen;
    if (head) clock.now = Math.max(clock.now, head.next);
    else if (then) {
      clock.now = Math.max(clock.now, then.due);
      then.due = Infinity;
    } else {
      clock.now = wall ? Math.max(clock.now, execution(next)) : execution(next);
      if (next.once) clock.timers.splice(clock.timers.indexOf(next), 1);
      else if (!next.hatch)
        next.due = next.frame
          ? vf(next.base, ++next.k)
          : repeat
            ? beatAfter(at, next.ms, Math.max(clock.now, wall ? to : clock.now))
            : next.due + next.ms;
    }
    if (!head && !then && !next.hatch && !next.data) {
      const counts = (clock.timerFirings ??= Array(7).fill(0));
      counts[next.frame ? 6 : wall && next.wait ? 5 : (next.wait ?? 0)]++;
    }
    const due = next?.due,
      sample = () => {
        if (wall) clock.now = Math.max(clock.now, performance.now() - start);
      };
    sample();
    Instant = wall ? (next?.wait || next?.change ? clock.now : at) : null;
    const ok = fire(head ? () => Sched.next(head) : then ? () => commit(then.then, `${then.name} then`) : next.action);
    Instant = null;
    sample();
    if (repeat && clock.timers.includes(next))
      next.due =
        next.gen === gen && next.due === due
          ? beatAfter(at, next.ms, Math.max(clock.now, wall ? to : clock.now))
          : clock.now + next.ms;
    if (ok !== true) return journal.at(-1);
    // Hatch and data callbacks have their own limits
    if (head || then || (!next.hatch && !next.data)) {
      fired++;
    }
    if (!head && !then && next.change) return true;
    if (stop?.()) {
      stopped = true;
      break;
    }
  }
  if (!stopped) clock.now = Math.max(clock.now, to);
  for (const f of Clocked) f();
  // Under the agent only: this journal is not a ring, and a page's own clock would add a line a tick.
  if (fired && clock.agent) say(`advance → ${fired} timer${fired === 1 ? '' : 's'} fired, epoch ${clock.epoch}`);
  return stopped;
}
export function fire(f) {
  for (const c of Clocked) c();
  Timing = true;
  try {
    return f();
  } finally {
    Timing = false;
  }
}
let driving = 0,
  painting = 0;
export function drive() {
  clearTimeout(driving);
  if (clock.agent || Poisoned) return;
  let next = Infinity;
  // no timer while a wake runs unseen (wakes.js)
  for (const t of [...(clock.stopped ? [] : clock.timers), ...Mutations])
    if (!t.frame && (!t.change || t.ready))
      next = Math.min(next, Link.timerWait?.wake(t) ?? t.due + (t.tolerance ?? 0));
  for (const m of Mutations) if (m.next < next) next = m.next;
  if (!isFinite(next)) return;
  const token = Link.timerWait?.generation();
  driving = setTimeout(
    () => {
      if (clock.agent || token !== Link.timerWait?.generation()) return;
      advance(performance.now() - start, true);
      Link.timerWait?.check();
      drive();
    },
    Math.min(2147483647, Math.max(0, next - (performance.now() - start))),
  );
}
// Presented frames: before each paint, timers due by the frame's time, then
// every frame task once at it (Runner::frame).
export function paint() {
  if (painting || typeof requestAnimationFrame !== 'function') return;
  const token = Link.timerWait?.generation();
  painting = requestAnimationFrame(function frame(ts) {
    if (Poisoned || token !== Link.timerWait?.generation()) return;
    painting = 0;
    if (clock.agent || !clock.timers.some(t => t.frame)) return;
    painting = requestAnimationFrame(frame);
    advance(Math.max(clock.now, ts - start), true);
    Link.timerWait?.check(true);
    const at = clock.now,
      rev = Rev,
      ticket = Ticket;
    NowRead = false;
    // A frame task's commit may arm or drop one (a gate, LLP 1092 D10): each armed at the frame's start fires once.
    for (const t of clock.timers.filter(t => t.frame).sort((a, b) => !!a.hatch - !!b.hatch))
      if (clock.timers.includes(t)) {
        t.base = at;
        t.k = 1;
        t.due = vf(at, 1);
        (clock.timerFirings ??= Array(7).fill(0))[6]++;
        fire(t.action);
      }
    // Frames whose tasks changed nothing and read no clock would change
    // nothing again until state does: the loop parks until a commit writes
    // (skipping a frame that would commit nothing is unobservable).
    if (Rev === rev && Ticket === ticket && !NowRead && !clock.timers.some(t => t.hatch && t.frame)) {
      cancelAnimationFrame(painting);
      painting = 0;
      Parked = Rev;
    }
    drive();
  });
}
let NowRead = false,
  Parked = -1;
/** After a commit that wrote: a parked frame loop runs again. */
export function unpark() {
  if (Parked >= 0 && Rev !== Parked) {
    Parked = -1;
    paint();
  }
}
/** The timers stop until the next `drive` or `paint` (rt-link.js `pauseTimers`). */
export function pauseTimers() {
  clearTimeout(driving);
  globalThis.cancelAnimationFrame?.(painting);
  driving = painting = 0;
}
// Read untracked (an action's body, a handler's curried argument evaluated as the event arrives) it is the clock now:
// the commit's time, or outside one the time the runner would evaluate the arguments at. A derive, resource or the
// tree reads it tracked, as of the last commit: an advance that fired nothing committed no new time.
export const x_performanceNow = () => {
  NowRead = true;
  if (Listener) return read(Now);
  if (!Writes) time();
  return clock.now;
};
