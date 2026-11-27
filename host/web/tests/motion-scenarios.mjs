// Seeded scenarios of layout moves for `motion-check.mjs`, so that a failure is found again by its seed: up to three
// positioned boxes, one maybe inside another, and maybe a box in a column that moves when the box above it grows, with
// commits that move and resize them, some while a move still runs.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// The JS target's copy of the engine's spring lowering (`SpringConfig::easing_over`), and the extent a layout spring's
// curve settles for (`exact_motion::LAYOUT_EXTENT`)
const SPRINGS = (0, eval)(readFileSync(resolve(import.meta.dirname, '../../web-js/transition-spring.js'), 'utf8')), EXTENT = 1000;

/** A layout spring's row as the hosts write it: `spring` and its curve from rest, over the time a move of the extent settles in. */
export function springCurve(config) {
  // (Its duration as the row's text carries it)
  const [seconds, easing] = SPRINGS.sample(SPRINGS.coefficients(config), EXTENT);
  return { ms: Number(SPRINGS.cssNumber(seconds * 1000)), easing, spring: true };
}

/** A seeded generator of numbers in [0, 1) (mulberry32). */
function random(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * Boxes on springs with and without bounce (as the hosts write a spring: its curve from rest), on CSS's easings, or
 * with no row, some after a delay; some native buttons, some turned or scaled (which only move, since their surface
 * would grow about a moving centre), some with a `translate` of their own or borders; at quarter points. `flow` is the column: a
 * box with no row whose height commits change, above a box that moves.
 */
export function scenario(seed) {
  const r = random(seed), pick = list => list[Math.floor(r() * list.length)], between = (a, b) => a + r() * (b - a);
  // (A curve that overshoots, for a box whose size it takes below its borders)
  const quarter = (a, b) => Math.round(between(a, b) * 4) / 4, curve = (kind, overshoots = false) => {
    if (kind === 'none') return null;
    const delay = r() < 0.25 ? Math.round(between(10, 80)) : 0, over = 'cubic-bezier(0.3, -0.4, 0.6, 1.5)';
    if (kind === 'easing') return { ms: Math.round(between(150, 700)), delay, easing: overshoots ? over : pick(['linear', 'ease', 'ease-in', 'ease-out', 'ease-in-out', over]) };
    const k = Math.round(between(150, 700)), ratio = overshoots ? 0.6 : pick([0.6, 1, 1, 1.4]);
    return { ...springCurve([k, Math.round(ratio * 2 * Math.sqrt(k) * 100) / 100, 1]), delay };
  };
  const boxes = [];
  for (let i = 0, count = 1 + Math.floor(r() * 3); i < count; i++) {
    const kind = pick(['spring', 'spring', 'easing', 'button', 'none']), turned = kind !== 'button' && r() < 0.25;
    // (Only inside a plain box: one inside a turned or scaled box is in coordinates the test does not undo)
    const nests = i > 0 && !boxes[0].button && !boxes[0].rotate && boxes[0].scale === 1 && r() < 0.3;
    const border = kind !== 'button' && !turned && r() < 0.25 ? 6 : 0;
    boxes.push({ id: String(i + 1), button: kind === 'button', curve: curve(kind, border > 0), rotate: turned ? pick([30, 90]) : 0, scale: turned ? pick([0.5, 2]) : 1, border,
      translate: r() < 0.25 ? [quarter(-20, 20), quarter(-20, 20)] : [0, 0], parent: nests ? '1' : null,
      box: [quarter(0, 300), quarter(0, 200), quarter(30, 120), quarter(20, 80)] });
  }
  const flow = r() < 0.4 ? { curve: curve(pick(['spring', 'easing'])), width: 60, height: 30, above: [quarter(10, 120)] } : null;
  const commits = [], last = boxes.map(b => b.box);
  for (let c = 0, n = 1 + Math.floor(r() * 3), t = 50; c < n; c++, t += between(40, 700)) {
    const changes = {};
    for (const [i, b] of boxes.entries()) {
      if (r() < 0.35) continue;
      // (Some moves keep the size, so that one that changes it may come while they run, and some the place)
      const [x, y, w, h] = last[i], still = b.rotate || b.scale !== 1 || r() < 0.3, placed = !still && r() < 0.2;
      // (A bordered box often shrinks to about its borders, so that a curve that overshoots takes it below them)
      const small = b.border && r() < 0.5, side = (a, z) => small ? quarter(2 * b.border, 2 * b.border + 8) : quarter(a, z);
      last[i] = [placed ? x : quarter(0, 300), placed ? y : quarter(0, 200), still ? w : side(30, 120), still ? h : side(20, 80)];
      changes[b.id] = last[i];
    }
    if (flow) flow.above.push(r() < 0.5 ? quarter(10, 120) : flow.above.at(-1));
    commits.push({ t: Math.round(t), changes });
  }
  // (Some in a parent written vertically, where a stand-in must still keep a bordered box's left edge)
  return { seed, boxes, flow, commits, vertical: r() < 0.25 };
}

/**
 * Where each box is laid out after each commit, in its parent's space, from the scenario alone: its box, carried by
 * its own `translate` and turned and scaled about its centre; the column's box under the one above it.
 */
export function layouts({ boxes, flow, commits }) {
  const place = (b, [x, y, w, h]) => {
    const a = b.rotate * Math.PI / 180, cos = Math.abs(Math.cos(a)), sin = Math.abs(Math.sin(a));
    const bw = b.scale * (w * cos + h * sin), bh = b.scale * (w * sin + h * cos), cx = x + w / 2 + b.translate[0], cy = y + h / 2 + b.translate[1];
    return [cx - bw / 2, cy - bh / 2, bw, bh];
  };
  const now = Object.fromEntries(boxes.map(b => [b.id, b.box])), out = [];
  for (const [i, c] of [{ t: 0, changes: {} }, ...commits].entries()) {
    Object.assign(now, c.changes);
    const laid = Object.fromEntries(boxes.map(b => [b.id, place(b, now[b.id])]));
    if (flow) laid.flow = [0, flow.above[i], flow.width, flow.height];
    out.push({ t: c.t, laid });
  }
  return out;
}
