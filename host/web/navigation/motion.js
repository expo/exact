import { queue, sheet, commit, root, log, settled } from "./routes.js";
// LLP 1047 D5: calls to motion-glue.js and collection-glue.js queue in order until their after-paint piece loads. Before use, drop no-op reconciliations and write styles immediately.
// `o.wasm(name, bytes)` returns a wasm reply, or null before boot; `o.replayed()` reconciles after replay.
// Work after a rendering opportunity (two animation frames) or a second, whichever is first, once: a page in a frame scrolled out of view gets no frames (LLP 1122's queue item (3) and its QUEUE follow-up); no timer under the agent, whose clock owns time.
export function afterPaint(fn, agent = false) {
  let ran = false;
  const once = () => {
    if (!ran) {
      ran = true;
      fn();
    }
  };
  requestAnimationFrame(() => requestAnimationFrame(once));
  if (!agent) {
    setTimeout(once, 1e3);
  }
}
export function afterPaintPieces(load, o) {
  let live = null, loading = null;
  const queue = [];
  const start = () => loading ??= Promise.all([
    load("./collection-glue.js", "collectionGlue"),
    load("./motion-glue.js", "motionGlue"),
    load("./group-glue.js", "groupGlue")
  ]).then(([c, m, g]) => {
    const common = {
      views: o.views,
      now: o.now,
      generation: o.generation,
      inert: o.inert,
      applyBatch: o.applyBatch,
      ready: o.ready,
      log: o.log
    };
    const request = (facts) => o.wasm("exact_motion", m.motionBytes(facts)) ?? { accepted: false };
    const collections = c.collectionController({
      root: o.root,
      views: o.views,
      agent: !!o.agent?.(),
      settled: () => arrange.commit(),
      report(bytes) {
        const batch = o.wasm("exact_collection_feedback", bytes);
        return batch ? c.applyCollectionFeedback(batch, o.applyBatch) : false;
      }
    });
    const motion = m.motionController({
      ...common,
      releaseInteraction: (pointer) => collections.releaseInteraction(pointer),
      request
    });
    const arrange = m.arrangeController({
      ...common,
      collections,
      motion,
      request,
      grouped: g.groupController,
      root: o.root
    });
    live = {
      collections,
      motion,
      arrange
    };
    for (const [piece, name, args] of queue.splice(0)) {
      try {
        live[piece][name](...args);
      } catch (error) {
        console.error(`exact: ${piece}.${name} failed`, error);
      }
    }
    o.replayed?.();
  }).catch((error) => {
    loading = null;
    queue.length = 0;
    console.error("exact: after-paint pieces:", error);
  });
  // @ref LLP 1055 D7: CSS animations need no piece — the browser runs them
  // from one `@keyframes` rule per name in a stylesheet the page owns.
  const keyframes = (name, body) => {
    const sheet = (document.getElementById("exact-keyframes") ?? document.head.appendChild(Object.assign(document.createElement("style"), { id: "exact-keyframes" }))).sheet;
    sheet.insertRule(`@keyframes ${CSS.escape(name)}{${body}}`, sheet.cssRules.length);
  };
  // `use`: the call needs its piece; otherwise it only reconciles what uses made.
  const call = (piece, name, use = true) => (...args) => {
    if (live) {
      return live[piece][name](...args);
    }
    if (!use && !loading) {
      return;
    }
    queue.push([
      piece,
      name,
      args
    ]);
    start();
  };
  const motion = {
    style(id, text) {
      if (live) {
        return live.motion.style(id, text);
      }
      const el = o.views.get(id);
      if (el) {
        el.style.cssText = text;
      }
    },
    keyframes
  };
  for (const name of [
    "animate",
    "retire",
    "heightBinding",
    "transformBinding",
    "attachSwipe",
    "attachHeightDrag",
    "attachTransformDrag"
  ]) {
    motion[name] = call("motion", name);
  }
  // A pan's release velocity (LLP 1057 §10.6): only once motion is here.
  motion.pan = {
    sample: (...a) => live?.motion.panSample(...a),
    velocity: (...a) => live?.motion.panVelocity(...a)
  };
  const arrange = {
    binding: call("arrange", "binding"),
    state: call("arrange", "state"),
    group: call("arrange", "group")
  };
  for (const piece of [motion, arrange]) {
    for (const name of [
      "commit",
      "reset",
      "destroy"
    ]) {
      piece[name] = call(piece === motion ? "motion" : "arrange", name, false);
    }
  }
  motion.followTimelines = call("motion", "followTimelines", false);
  // Every first batch commits the (empty) collection set: a use only with items.
  const commit = call("collections", "commit"), reconcile = call("collections", "commit", false);
  const collections = {
    commit: (items) => (items.length ? commit : reconcile)(items),
    scrollPending: () => live?.collections.scrollPending() ?? false
  };
  for (const name of [
    "reset",
    "dataReady",
    "releaseInteraction",
    "settle"
  ]) {
    collections[name] = call("collections", name, false);
  }
  collections.jump = call("collections", "jump");
  collections.restyled = () => live?.collections.restyled();
  collections.fireOwedScrolls = async () => await live?.collections.fireOwedScrolls() ?? {
    fired: false,
    owed: 0
  };
  collections.owedScrolls = () => live?.collections.owedScrolls() ?? 0;
  // nothing to restyle, and no list moved, before the piece
  // @ref LLP 1056 D7 — Canvas 2D's replayer and ResizeObserver: its own piece, injected after a rendering opportunity (`afterPaint`) after the first 2D canvas's op.
  let c2d = null, c2dLoading = null;
  const c2dQueue = [];
  const canvas2d = (op) => {
    if (c2d) {
      return c2d.op(op);
    }
    if (op.forget && !c2dLoading) {
      return;
    }
    c2dQueue.push(op);
    c2dLoading ??= new Promise((r) => afterPaint(r, o.agent())).then(() => load("./canvas2d-glue.js", "canvas2dGlue")).then((make) => {
      c2d = make({
        views: o.views,
        now: o.now,
        applyBatch: o.applyBatch,
        generation: o.generation
      });
      for (const q of c2dQueue.splice(0)) {
        c2d.op(q);
      }
    }).catch((error) => {
      c2dLoading = null;
      c2dQueue.length = 0;
      console.error("exact: canvas2d:", error);
    });
  };
  const c2dPending = () => c2d ? c2d.settled() : c2dLoading?.then(() => c2d?.settled());
  // Preload motion when the plan exports exact_motion, before its first spring.
  return {
    collections,
    motion,
    arrange,
    canvas2d,
    preload: start,
    pending: () => {
      const p = live || !loading ? null : loading, c = c2dPending();
      return p && c ? Promise.all([p, c]) : p ?? c ?? null;
    }
  };
}
// LLP 1063: preload presence-glue.js after paint for declared layout or exit motion; boot and first presses jump until loaded.
// Queue exits and later batches until loading finishes. The agent waits on bounded pending work.
export function presenceLoader(load, root, apply, log) {
  let live = null, loading = null, unavailable = false, scheduled = null;
  const held = [], row = (op) => op.op === "exit" || /(^|;)\s*--exact-(layout-transition|exit-animation)\s*:/.test((op.css ?? "").replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g, "\"\""));
  const release = () => {
    for (const batch of held.splice(0)) {
      apply(batch);
    }
  };
  const start = () => loading ??= load("./presence-glue.js", "presence").then((create) => {
    live = create(root);
    release();
  }).catch((error) => {
    unavailable = true;
    loading = null;
    log(`presence module: unavailable; motion skipped: ${error}`);
    release();
  });
  return {
    get live() {
      return live;
    },
    preload() {
      if (!live && !unavailable && !loading) {
        scheduled ??= new Promise((resolve) => afterPaint(() => {
          scheduled = null;
          resolve(unavailable ? null : start());
        }));
      }
    },
    pending: () => live || unavailable ? null : loading ?? scheduled,
    landed(ms) {
      const p = this.pending();
      let t;
      return !p ? Promise.resolve(true) : Promise.race([p.then(() => true), new Promise((r) => {
        t = setTimeout(r, ms, false);
      })]).finally(() => clearTimeout(t));
    },
    // whether no load is left within `ms`
    resize(batch) {
      return {
        ...batch,
        presenceSnap: true
      };
    },
    hold(batch) {
      if (live || unavailable) {
        return false;
      }
      const ops = batch.ops ?? [];
      if (ops.some(row)) {
        start();
      }
      if (!held.length && !(loading && ops.some((op) => op.op === "exit"))) {
        return false;
      }
      held.push(batch);
      return true;
    }
  };
}
// The agent's browser clock (LLP 1012): author-paused animations keep their
// own time (LLP 1055 D10); every other animation follows the runner's clock.
export function animationClock(now, settled, synced, early = () => undefined) {
  const starts = new WeakMap(), held = new WeakSet(), clocks = animationClocks(document);
  // A scroll-driven animation follows its scroll, not a clock.
  const timed = () => document.getAnimations().filter((a) => !a.timeline || a.timeline instanceof DocumentTimeline);
  return {
    register(t) {
      clocks.commit();
      for (const a of timed()) {
        if (!starts.has(a)) {
          const at = early(a);
          starts.set(a, clocks.start(a, at ?? t) ?? at ?? t);
          if (a.playState === "paused" && at === undefined) held.add(a);
        } else if (held.has(a) && a.playState !== "paused") {
          // A prefetched play resumes at reveal, keeping any time held by its author.
          held.delete(a);
          starts.set(a, clocks.start(a, t) ?? t - (a.currentTime ?? 0));
        }
      }
    },
    runs: a => (!a.timeline || a.timeline instanceof DocumentTimeline) && (starts.has(a) ? !held.has(a) : a.playState !== "paused" || early(a) !== undefined),
    seek(to, sync = true) {
      for (const a of timed()) {
        const timing = a.effect?.getComputedTiming();
        if (!timing) {
          continue;
        }
        const t = to - (starts.get(a) ?? now());
        if (held.has(a)) {
          continue;
        } else if (t >= timing.endTime && timing.endTime !== Infinity) {
          a.finish();
        } else {
          a.pause();
          a.currentTime = t;
        }
      }
      if (sync) synced();
    },
    // A picture holds finite motion just before its end, without firing its end event (LLP 1116.003 D8).
    hold() {
      let n = 0;
      for (const a of timed()) {
        const t = a.effect?.getComputedTiming();
        if (!t || held.has(a) || !(t.activeDuration > 0) || t.activeDuration === Infinity) continue;
        const at = a.currentTime ?? 0, end = t.delay + t.activeDuration;
        if (at >= t.delay && at < end - 0.5) { a.currentTime = end - 0.5; n++; }
      }
      return n;
    },
    // The springs' engine knows its end; browser animations report theirs.
    settle() {
      let to = now();
      const s = settled();
      if (s != null) {
        to = Math.max(to, s);
      }
      for (const a of timed()) {
        const timing = a.effect?.getComputedTiming();
        if (timing && timing.endTime !== Infinity && !held.has(a)) {
          to = Math.max(to, (starts.get(a) ?? now()) + timing.endTime);
        }
      }
      return to;
    }
  };
}
// LLP 1055.002: clock(Name) animations share an origin. Idle clocks start now; joiners use the latest cycle boundary.
// Alternate animations count a pair of iterations as one cycle. sync updates startTime once per commit.
export function animationClocks(root) {
  const origins = new Map(), members = new Map(), paused = new WeakMap(), clocked = new WeakMap();
  const clockOf = (a) => a.animationName === undefined ? "" : a.effect?.target?.style?.getPropertyValue("--exact-animation-clock").trim() ?? "";
  const live = (a) => a.effect?.target?.isConnected && a.playState !== "idle" && a.playState !== "finished";
  // Release departed, ended or retargeted animations so they do not keep clocks busy.
  const commit = () => {
    for (const [c, m] of members) {
      for (const b of m) {
        if (!live(b) || clockOf(b) !== c) {
          m.delete(b);
        }
      }
      if (!m.size) {
        members.delete(c);
      }
    }
  };
  function start(a, now) {
    const c = clockOf(a);
    if (!c) {
      return null;
    }
    let m = members.get(c);
    if (!m) {
      members.set(c, m = new Set());
    }
    // Busy while any member is live, `a` included: a paused or resumed
    // member keeps the origin, so a resume rejoins its phase (D6).
    if (!m.size || !origins.has(c)) {
      origins.set(c, now);
    }
    m.add(a);
    const { duration, direction } = a.effect.getComputedTiming(), period = duration * (/alternate/.test(direction) ? 2 : 1);
    if (!(period > 0 && Number.isFinite(period))) {
      return now;
    }
    // On a boundary in float can read a hair before it: that is on it.
    const into = ((now - origins.get(c)) % period + period) % period;
    return now - (period - into < 1e-6 ? 0 : into);
  }
  return {
    start,
    commit,
    sync(now = document.timeline.currentTime) {
      commit();
      if (!root.querySelector("[style*=\"--exact-animation-clock\"]")) {
        return;
      }
      for (const a of document.getAnimations()) {
        const is = a.playState === "paused", was = paused.get(a), c = clockOf(a), had = clocked.get(a) ?? "";
        // On a clock and not its member: new, moved onto it, or let go
        // while it was off one (its name taken away and given back).
        const member = !c || members.get(c)?.has(a);
        if (was === is && c === had && member) {
          continue;
        }
        paused.set(a, is);
        clocked.set(a, c);
        // An ended play stays ended: a clock does not restart it.
        if (a.playState === "finished" || a.playState === "idle") {
          continue;
        }
        // A pause keeps its membership: paused, it still holds the clock busy. A new one joins (paused, without a start, which would unpause it); a resume rejoins.
        if (is && was !== undefined && c === had && member) {
          continue;
        }
        const s = start(a, now);
        if (s !== null && !is) {
          a.startTime = s;
        }
      }
    }
  };
}
