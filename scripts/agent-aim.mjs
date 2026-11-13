// A tap addressed by id presses what it names (LLP 1012 §1 `tap`), the web's
// half of `host/apple/Sources/ExactKit/AgentAddressedTap.swift`: a plain
// `tap <target>` aims at the target's middle, and a click there reaches the
// innermost element with a press of its own — a link card or a Like button
// inside a post row (the Bluesky clone, 2026-10-09). A target with a press
// of its own is clicked where a click reaches it (its middle, else the point
// of its box nearest the middle that does), or refused; a target without one
// never presses a control inside it in its name. `tap <target> at <x> <y>` is
// a point and keeps a point's semantics.

/** In the page: a wrapped inline's box centre can fall between its fragments. */
export function pagePoint({ id }) {
  const el = globalThis.exact.views.get(id), box = el?.getBoundingClientRect();
  const on = (x, y) => {
    const hit = document.elementFromPoint(x, y);
    return !!hit && (el === hit || el.contains(hit));
  };
  if (!el || on(box.x + box.width / 2, box.y + box.height / 2)) return null;
  for (const rect of el.getClientRects()) {
    const x = rect.x + rect.width / 2, y = rect.y + rect.height / 2;
    if (rect.width && rect.height && on(x, y)) return [x, y];
  }
  return null;
}

/** In the page (self-contained: Chrome evaluates its source, Playwright passes it): where a plain tap on view `id`, aimed
 *  at (`x`, `y`) in the viewport, must land. Null: there. `{at, avoided}`: at that point instead, its middle reaching
 *  `avoided`. `{error, pressing}`: refused. A press is an element's own when it has a `press` handler, is a link, takes
 *  a canvas's pointer, is a surface's action button, or is a button invoking a command or a popover; a form control
 *  takes the click itself; a disabled form control or an inert element stops it. */
export function pageAim({ id, x, y }) {
  const el = globalThis.exact?.views?.get(id);
  if (!el) return null;
  // A surface's action button over its canvas takes the pointer itself (gpu-glue.js `control`).
  const takes = (e) => String(e.dataset?.exactOn ?? '').split(' ').includes('press') || e.matches('a[href], [data-gpu-input], button[data-action]')
    || (e.localName === 'button' && (e.hasAttribute('commandfor') || e.hasAttribute('popovertarget')));
  const control = (e) => e.matches('input:not([type="hidden"]), select, textarea, [contenteditable=""], [contenteditable="true"]');
  // What a click at (px, py) reaches, or null where it would not land on the target.
  const reach = (px, py) => {
    const hit = document.elementFromPoint(px, py);
    if (!hit || !(el === hit || el.contains(hit))) return null;
    for (let e = hit; e && e !== document.documentElement; e = e.parentElement) {
      // A disabled form control gets no click, nor does anything inert; `disabled` on a box means nothing on the web.
      if (e.matches(':disabled') || e.matches('[inert]')) return { el: e, blocked: true };
      if (takes(e) || control(e)) return { el: e, control: control(e) && !takes(e) };
    }
    return { el: null };
  };
  // A node's id, by the element that is its view or holds it.
  const ids = new Map();
  for (const [key, view] of globalThis.exact.views) ids.set(view, key);
  const pressing = (r) => { for (let e = r.el; e; e = e.parentElement) if (ids.has(e)) return Number(ids.get(e)); return null; };
  const name = (r) => { const v = pressing(r); return r.control ? `the form control of #${v}` : r.blocked ? `disabled #${v}` : `#${v}`; };
  // An ancestor under the middle (the target passes its presses through) or nothing: the caller's checks decide.
  const middle = reach(x, y);
  if (!middle || middle.el === el) return null;
  const a = `#${id}`, d = name(middle), how = `tap ${pressing(middle) != null ? `#${pressing(middle)}` : d}, or tap ${a} at <x> <y> (a point in its box) for whatever a finger there reaches`;
  if (takes(el)) {
    const b = el.getBoundingClientRect();
    const left = Math.max(0, b.left), top = Math.max(0, b.top), right = Math.min(innerWidth, b.right), bottom = Math.min(innerHeight, b.bottom);
    if (right > left && bottom > top) {
      // The middles of a grid of cells of about 12 px (at most 32 a side), then of about 3 px (at most 96 a side) with
      // points along its edges 1 px in, every 2 px; each nearest the middle first, ties in reading order. A strip
      // narrower than the finer grid away from the edges can still be missed.
      const w = right - left, h = bottom - top;
      const grid = (cell, most) => {
        const cols = Math.min(most, Math.max(3, Math.ceil(w / cell))), rows = Math.min(most, Math.max(3, Math.ceil(h / cell))), out = [];
        for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) out.push([left + (c + 0.5) * w / cols, top + (r + 0.5) * h / rows]);
        return out;
      };
      const near = (points) => points.map((p, i) => [p[0], p[1], Math.hypot(p[0] - x, p[1] - y), i]).sort((p, q) => p[2] - q[2] || p[3] - q[3]);
      const fine = grid(3, 96);
      if (w > 2 && h > 2) {
        const xs = Math.min(400, Math.floor(w / 2)), ys = Math.min(400, Math.floor(h / 2));
        for (let i = 0; i <= xs; i++) { const px = left + 1 + (w - 2) * i / Math.max(xs, 1); fine.push([px, top + 1], [px, bottom - 1]); }
        for (let i = 0; i <= ys; i++) { const py = top + 1 + (h - 2) * i / Math.max(ys, 1); fine.push([left + 1, py], [right - 1, py]); }
      }
      for (const [px, py] of [...near(grid(12, 32)), ...near(fine)]) if (reach(px, py)?.el === el) return { at: [px, py], avoided: { pressing: pressing(middle), what: d } };
    }
    return { error: `tap ${a} would press ${d} inside it, at its middle; no point of ${a} a finger can reach presses ${a} itself; ${how}`, pressing: pressing(middle), addressed: id };
  }
  if (middle.el && !middle.blocked && el.contains(middle.el)) {
    return { error: `tap ${a} would press ${d} inside it; ${a} has no press of its own, and a tap that names it never presses a control inside it; ${how}`, pressing: pressing(middle), addressed: id };
  }
  return null;
}

/** In the page (self-contained, as `pageAim`): why a tap on view `id` is refused when no element is at its middle. Under
 *  the agent the page's animations run only on the drive's clock (rules/DEFERRED.md), so a target that a presentation
 *  or a transition is moving (a sheet sliding up, on it or a box that holds it) stays where that motion starts until the
 *  clock moves. The animations on the target and the boxes that hold it that the clock will run to an end (finite, and
 *  run by the clock by its own account, `exact.clockRuns`: not held by their author, not scroll-driven) are put at their
 *  ends for a moment and the target's middle measured there: when that brings it into the viewport, the refusal names
 *  the last of them to end and the time it has left. Otherwise, or where the clock says nothing of what it runs (the
 *  wasm target), the advice is to scroll. A pseudo-element's animation (an indeterminate progress's spinner) moves no
 *  box. Only paused animations are measured, as every one the clock runs is between its moves: so measuring fires no
 *  event. */
export function pageOutside({ id }) {
  const view = globalThis.exact?.views?.get(id), el = globalThis.exact?.backButton?.(view) ?? view;
  const scroll = 'its middle is outside the viewport; scroll it into view first', runs = globalThis.exact?.clockRuns;
  if (!el || !runs) return scroll;
  const ids = new Map();
  for (const [key, v] of globalThis.exact.views) ids.set(v, key);
  const named = (e) => { for (let n = e; n; n = n.parentElement) if (ids.has(n)) return `node #${ids.get(n)}`; return e.localName; };
  const moving = [];
  for (let e = el; e; e = e.parentElement ?? e.getRootNode?.()?.host) {
    for (const a of e.getAnimations?.() ?? []) {
      const t = a.effect?.getComputedTiming?.(), left = t ? Math.ceil(t.endTime - (t.localTime ?? 0)) : NaN;
      if (a.playState === 'paused' && Number.isFinite(left) && left > 0 && !a.effect.pseudoElement && runs(a)) moving.push({ a, e, left, at: a.currentTime });
    }
  }
  const middle = () => { const r = el.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; };
  // Where the middle is with `ended` at their ends, the rest as they are.
  const at = (ended) => { for (const m of ended) m.a.currentTime = m.a.effect.getComputedTiming().endTime; const p = middle(); for (const m of ended) m.a.currentTime = m.at; return p; };
  const [x, y] = at(moving);
  if (!moving.length || !(x >= 0 && y >= 0 && x < innerWidth && y < innerHeight)) return scroll;
  // Named: the last to end of those that move the middle by themselves (a sheet's slide, not its shadow's fade).
  const [x0, y0] = middle(), movers = moving.filter((m) => { const [mx, my] = at([m]); return Math.hypot(mx - x0, my - y0) > 0.5; });
  const { a, e, left } = (movers.length ? movers : moving).reduce((p, q) => (q.left > p.left ? q : p));
  const what = a.transitionProperty ? `a ${a.transitionProperty} transition` : `the animation ${a.animationName ?? a.id ?? ''}`.trim();
  return `its middle is outside the viewport while ${what} on ${e === el ? 'it' : `${named(e)}, which holds it,`} has ${left} ms to run, and the agent's clock runs it: advance the clock first (\`clock +${left}\` or \`clock settle\`), not a scroll`;
}

/** A refusal's node ids as a driver names them: `#12` becomes its testId (quoted when it has a space), else stays. */
export function namedRefusal(message, nodes) {
  if (!/^tap #\d+ would press /.test(message ?? '')) return message;
  return message.replace(/#(\d+)/g, (m, id) => {
    const testId = nodes.find((n) => n.id === Number(id))?.props?.testId;
    return testId ? (/\s/.test(testId) ? JSON.stringify(testId) : testId) : m;
  });
}
