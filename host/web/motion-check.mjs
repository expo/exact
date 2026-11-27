// What a layout move promises the person watching (LLP 1063 D10), checked on a recorded timeline. A timeline is
//   { nodes: { id: { curve, anchor } },
//     commits: [{ t, laid: { id: [x, y, w, h] } }],
//     samples: [{ k, t, nodes: { id: { shown: [x, y, w, h], moving } } }] }
// in milliseconds and points, each box in its parent's space. `curve` is the box's row throughout, `{ ms, delay, easing }`
// with a CSS easing and `spring: true` for a spring (as the engine writes it, a `linear()` over its settle time), or null
// for a box with no row.
// `anchor` is 'box', or 'centre' for a native button, which takes its new size at once and moves by its centre. `floor`
// is the least size a box is shown at, `[w, h]` (its borders on the web), zero by default. A
// box's sample may hold `content` too, where its content is shown apart from its surface: at the shown corner, at its
// laid-out size. The
// first commit is the first layout. A sample's `k` is how many commits it follows: one at a commit's own time with `k`
// one short is the frame before it. `moving` is whether the host still holds anything of the box's moves.
//
// The check works out, from the commits alone, where each box should be shown at each sample, by the algorithm D10
// promises: a commit that changes a box's laid place (by a hundredth of a point or more in a coordinate it moves by)
// starts a move from that change back to none on its curve, and the box is shown at its laid place plus every move
// under way. A spring's move adds to the moves under way; an easing's ends them and plays what they had left as well.
// So "shown where the
// reference says" holds the promises of a straight path, arrival, interruption, relative motion, the outermost offset
// and the surface's size, and the rest are checked as themselves: no jump, nothing left after the last move's end,
// nothing gone before it, a native button's size.

// A CSS number, or NaN for text that is none
const number = text => /^\s*[+-]?(\d+(\.\d+)?|\.\d+)(e[+-]?\d+)?\s*$/i.test(text) ? Number(text) : NaN;

// CSS's named easings as cubic Béziers (CSS Easing Functions Level 1 §2.2)
const BEZIERS = { ease: [0.25, 0.1, 0.25, 1], 'ease-in': [0.42, 0, 1, 1], 'ease-out': [0, 0, 0.58, 1], 'ease-in-out': [0.42, 0, 0.58, 1] };

/**
 * A CSS easing's output at input progress `p` in [0, 1]: `linear`, a named one, `cubic-bezier()` or `linear()` (CSS
 * Easing Functions Level 2 §2); null for one this does not model, such as `steps()`.
 */
export function eased(easing, p) {
  const [, name, args] = /^([a-z-]+)(?:\((.*)\))?$/.exec(easing.trim()) ?? [];
  if (name === 'linear' && args === undefined) return p;
  if (name === 'linear') return piecewise(args, p);
  const b = name === 'cubic-bezier' ? args?.split(',').map(number) : args === undefined ? BEZIERS[name] : null;
  // (Both x controls in [0, 1], as CSS takes them)
  if (!b || b.length !== 4 || !b.every(Number.isFinite) || [b[0], b[2]].some(x => x < 0 || x > 1)) return null;
  if (p <= 0 || p >= 1) return p <= 0 ? 0 : 1;
  const [x1, y1, x2, y2] = b, along = (a1, a2, s) => 3 * a1 * s * (1 - s) ** 2 + 3 * a2 * s * s * (1 - s) + s ** 3;
  let lo = 0, hi = 1;
  for (let i = 0; i < 60; i++) { const mid = (lo + hi) / 2; if (along(x1, x2, mid) < p) lo = mid; else hi = mid; }
  return along(y1, y2, (lo + hi) / 2);
}

// `linear()`: each stop an output and at most two input percentages; an input left out is spaced evenly between the
// stops around it, the first's being 0% and the last's 100%, and none is below an earlier one. Before the first stop and
// after the last, the line through the two nearest goes on (CSS Easing Functions Level 2 §2.1.2).
function piecewise(args, p) {
  const stops = [], list = args.split(',');
  if (list.length < 2) return null;
  for (const stop of list) {
    // (A stop is its output and at most two input percentages together, before or after it)
    const words = stop.trim().split(/\s+/), outputs = words.filter(w => !w.endsWith('%')), inputs = words.filter(w => w.endsWith('%'));
    if (!['n', 'np', 'npp', 'pn', 'ppn'].includes(words.map(w => w.endsWith('%') ? 'p' : 'n').join(''))) return null;
    for (const input of inputs.length ? inputs : [null]) stops.push([input === null ? null : number(input.slice(0, -1)) / 100, number(outputs[0])]);
  }
  if (!stops.every(([input, output]) => (input === null || Number.isFinite(input)) && Number.isFinite(output))) return null;
  stops[0][0] ??= 0;
  stops.at(-1)[0] ??= 1;
  for (let i = 1, last = stops[0][0]; i < stops.length; i++) {
    if (stops[i][0] === null) {
      let j = i;
      while (stops[j][0] === null) j++;
      for (let n = i; n < j; n++) stops[n][0] = last + (stops[j][0] - last) * (n - i + 1) / (j - i + 1);
    }
    last = stops[i][0] = Math.max(stops[i][0], last);
  }
  // (First stops at one input with two outputs give the first before the curve starts and the last of them at it, which
  // a progress alone cannot tell apart)
  const first = stops.findLastIndex(([input]) => input === stops[0][0]);
  if (stops[first][1] !== stops[0][1]) return null;
  const n = stops.length - 1, i = Math.min(Math.max(stops.findLastIndex(([input]) => input <= p), 0), n - 1);
  const [[a, u], [b, w]] = [stops[i], stops[i + 1]];
  if (p >= b && i + 1 === n && b === a) return w;
  return b > a ? u + (w - u) * (p - a) / (b - a) : p < a ? u : w;
}

// The coordinates a move carries: all four, or a native button's centre
const carried = (box, anchor) => anchor === 'centre' ? [box[0] + box[2] / 2, box[1] + box[3] / 2] : box;
const distance = (a, b) => Math.hypot(...a.map((x, i) => x - b[i]));
// Whether a commit moves a box: a change below a hundredth of a point is layout's rounding, which the web's module plays
// no move for (`presence-glue.js` `after`)
const changes = (a, b) => a.some((x, i) => Math.abs(x - b[i]) >= 0.01);
// Whether time `t` has reached `end`, to a clock's rounding: a frame often falls on a move's end, a spring's being on
// the engine's 240 Hz grid
const reached = (t, end) => t >= end - 1e-6;
const round = v => v.map(x => Math.round(x * 100) / 100);
const finite = box => Array.isArray(box) && box.length === 4 && [0, 1, 2, 3].every(i => Number.isFinite(box[i]));

/**
 * A box's moves, from the commits alone: each `{ k, t, d, end, cut }`, started by commit `k` at `t` from `d` (the change
 * of its laid place, in the coordinates it moves by, and for an easing what the moves it ended had left), and ended at
 * `end` or by commit `cut`; `place(i, t)` is where it is shown at `t` after the first `i` commits, and `end(i)` when the
 * last move under way then ends. `modeled` is false for an easing this does not model.
 */
export function reference(timeline, id) {
  const { curve, anchor = 'box', floor = [0, 0] } = timeline.nodes[id], { commits } = timeline;
  const modeled = !curve || Number.isFinite(eased(curve.easing, 0.5)), moves = [];
  // The latest a move could end after each commit, from each change of the laid place alone, for a curve this does not
  // model
  const latest = [-Infinity, -Infinity];
  // How far a move is along its curve at `t`, its start's share in its delay and all of it from its end, and how much of
  // it is left
  const progress = (m, t) => reached(t, m.end) ? 1 : eased(curve.easing, Math.min(1, Math.max(0, (t - m.t - curve.delay) / curve.ms))) ?? 1;
  const left = (m, t) => 1 - progress(m, t);
  const under = (i, t) => moves.filter(m => m.k <= i && !(m.cut <= i) && !reached(t, m.end));
  for (let k = 2; curve && k <= commits.length; k++) {
    const was = carried(commits[k - 2].laid[id], anchor), next = carried(commits[k - 1].laid[id], anchor), t = commits[k - 1].t;
    latest[k] = latest[k - 1];
    if (!changes(was, next)) continue;
    latest[k] = Math.max(latest[k], t + curve.delay + curve.ms);
    // An easing ends the moves under way and plays what they had left, summed as the web's module sums it
    const whole = was.map((x, j) => x - next[j]), shown = whole.map(() => 0);
    if (!curve.spring) for (const m of under(k - 1, t)) {
      const p = progress(m, t);
      m.d.forEach((x, j) => { whole[j] += x; shown[j] += x * p; });
      m.cut = k;
    }
    const rounding = (2 * under(k - 1, t).length + 2) * Number.EPSILON;
    const d = whole.map((x, j) => { const r = x - shown[j]; return Math.abs(r) <= rounding * (Math.abs(x) + Math.abs(shown[j])) ? 0 : r; });
    // (An easing retargeted to where the box is shown has nothing to play)
    if (curve.ms + curve.delay > 0 && d.some(x => x !== 0)) moves.push({ k, t, d, end: t + curve.delay + curve.ms });
  }
  const place = (i, t) => {
    const at = under(i, t).reduce((at, m) => { const share = left(m, t); return at.map((x, j) => x + m.d[j] * share); }, carried(commits[i - 1].laid[id], anchor));
    // A size the moves would take below its floor is shown at it
    return anchor === 'centre' ? at : [at[0], at[1], Math.max(floor[0], at[2]), Math.max(floor[1], at[3])];
  };
  const end = i => Math.max(-Infinity, ...moves.filter(m => m.k <= i && !(m.cut <= i)).map(m => m.end));
  return { moves, place, end, modeled, latest: i => latest[i] ?? -Infinity };
}

/**
 * Every promise the timeline breaks, as sentences, the first instance of each in each stretch between commits; none
 * for one that keeps them all. `tolerance` is the distance in points within which two boxes agree.
 */
export function checkMoves(timeline, { tolerance = 0.1 } = {}) {
  const errors = [], { nodes = {}, commits = [], samples = [] } = timeline;
  if (!commits.length || !samples.length) return [`the timeline has ${commits.length} commits and ${samples.length} samples: nothing to check`];
  for (const [i, c] of commits.entries()) if (!Number.isFinite(c.t) || (i && c.t < commits[i - 1].t)) errors.push(`commit ${i + 1} is at ${c.t} ms, out of order`);
  for (const s of samples) {
    if (!Number.isFinite(s.t) || !Number.isInteger(s.k) || !(s.k >= 1 && s.k <= commits.length) || s.t < commits[s.k - 1].t || s.t > (commits[s.k]?.t ?? Infinity)) errors.push(`the sample at ${s.t} ms is outside the stretch after commit ${s.k}`);
  }
  for (let i = 2; i <= commits.length; i++) {
    const t = commits[i - 1].t;
    if (!samples.some(s => s.k === i - 1 && s.t === t) || !samples.some(s => s.k === i && s.t === t)) errors.push(`no frame on both sides of the commit at ${t} ms`);
  }
  if (errors.length) return errors;
  // Every box the timeline names anywhere is checked, each apart: one it does not declare, or declares unreadably, is named
  const ids = new Set([...Object.keys(nodes), ...commits.flatMap(c => Object.keys(c.laid ?? {})), ...samples.flatMap(s => Object.keys(s.nodes ?? {}))]);
  for (const id of ids) {
    if (!nodes[id]) { errors.push(`${id}: the timeline records it, and declares no curve for it`); continue; }
    const { curve, anchor = 'box', floor = [0, 0] } = nodes[id];
    if (curve !== null && !(typeof curve?.easing === 'string' && Number.isFinite(curve.ms) && Number.isFinite(curve.delay))) { errors.push(`${id}: its curve is neither null nor { ms, delay, easing }`); continue; }
    if (!(Array.isArray(floor) && floor.length === 2 && [0, 1].every(i => Number.isFinite(floor[i]) && floor[i] >= 0))) { errors.push(`${id}: its floor is not two sizes`); continue; }
    const evidence = s => finite(s.nodes?.[id]?.shown) && typeof s.nodes[id].moving === 'boolean' && (s.nodes[id].content === undefined || finite(s.nodes[id].content));
    if (!commits.every(c => finite(c.laid?.[id])) || !samples.every(evidence)) { errors.push(`${id}: a commit or a sample has no box, lifetime or content it can read for it`); continue; }
    const { place, end, modeled, latest } = reference(timeline, id), shown = s => s.nodes[id].shown;
    // A curve this does not model leaves an easing's restart unknown, so only what holds from the latest end its moves
    // could have is checked, with its content and a native button's size
    if (!modeled) errors.push(`${id}: its curve ${curve.easing} is not one the check models, so only its end is checked`);
    for (let i = 1; i <= commits.length; i++) {
      const own = samples.filter(s => s.k === i), laid = commits[i - 1].laid[id], t = commits[i - 1].t, last = modeled ? end(i) : latest(i);
      const said = new Set(), say = (rule, text) => { if (!said.has(rule)) { said.add(rule); errors.push(`${id}: ${text}`); } };
      // No jump: the frame after a commit shows the box where the frame before did, wherever the new move shows nothing
      // of itself at the commit, as the reference has it
      if (modeled && i > 1 && distance(place(i, t), place(i - 1, t)) <= tolerance) {
        const was = carried(shown(samples.find(s => s.k === i - 1 && s.t === t)), anchor), now = carried(shown(own.find(s => s.t === t)), anchor);
        if (distance(was, now) > tolerance) say('jump', `jumps at the commit at ${t} ms, from ${round(was)} to ${round(now)}`);
      }
      for (const s of own) {
        // Shown where the reference says: on its curves, on a straight path, from where it was, in its parent, outermost
        if (modeled) {
          const want = place(i, s.t), got = carried(shown(s), anchor);
          if (distance(got, want) > tolerance) say('place', `at ${s.t} ms is shown at ${round(got)}, where its ${curve ? 'moves put' : 'layout puts'} it at ${round(want)}`);
        }
        // It arrives with nothing left, and nothing ends early
        const ended = reached(s.t, last);
        if (ended && (distance(shown(s), laid) > tolerance || s.nodes[id].moving)) say('end', `at ${s.t} ms, ${last > -Infinity ? `past its last move's end at ${Math.round(last)} ms` : 'at rest'}, is shown at ${round(shown(s))} laid at ${round(laid)}${s.nodes[id].moving ? ', and something of a move is left' : ''}`);
        if (modeled && !ended && !s.nodes[id].moving) say('early', `at ${s.t} ms rests before its last move ends at ${Math.round(last)} ms`);
        // Its content, apart from its surface, is at its shown corner at its laid-out size
        const content = s.nodes[id].content, corner = [...shown(s).slice(0, 2), ...laid.slice(2)];
        if (content && distance(content, corner) > tolerance) say('content', `at ${s.t} ms shows its content at ${round(content)}, not at its shown corner at its laid size ${round(corner)}`);
        // A native button takes its new size at once
        if (anchor === 'centre' && distance(shown(s).slice(2), laid.slice(2)) > tolerance) say('size', `at ${s.t} ms is ${round(shown(s).slice(2))}, not its laid size ${round(laid.slice(2))}`);
      }
      // The recording follows the last move to its end
      if (i === commits.length && !reached(own.at(-1)?.t, last)) say('short', `the recording ends at ${own.at(-1)?.t} ms, before its last move ends at ${Math.round(last)} ms`);
    }
  }
  return errors;
}
