// @ref LLP 1123 D9, D17 — each reader's own safe-area observation on the web: how far the unsafe area it lies in
// reaches into its border box from each edge, at most that area's depth, as UIKit gives each view its `safeAreaInsets`,
// written as an inline custom property on the reader so no reader inherits an ancestor's. The area is what layout shows
// obstructing it, measured, never declared, as the kernel's `area` composes it (`area`): the layout viewport's unsafe
// area (`env(safe-area-inset-*)`), a transparent bar over its route, the reservations of the boxes around it, and none
// inside a scroller, which takes it. A reader's input can move it, so D9's rounds run in one task before the browser
// paints, until every input holds or the bound; a round's values start no transition (`data-exact-round` holds them at
// no duration and no delay), and only the settled change reaches transition processing (CSS Transitions 1, "Starting of
// transitions"). A reader is read as laid out, every scroll offset at zero and every sticky box where layout puts it.
// The page's keyboard is the session's (D7, D17), a docked keyboard's depth up from the layout viewport's bottom
// (`keyboardInset`), from which each reader's two keyboard families are its overlap from its bottom edge, a floating
// keyboard taken as docked. Activation (stage 5) runs it at each commit; until then only its tests do.

import { drop, own, register } from "./inputs.js";
import { liveChildren } from "./lists.js";
import { keyboardInset } from "./environment.js";

const EDGES = ["top", "right", "bottom", "left"];
/** Each input, by its property's family, in the order a reader's inputs are kept. */
const FAMILIES = ["safe-area-inset", "keyboard-inset", "keyboard-undocked-inset"];
/** The attribute that holds a sticky box where layout puts it while the rounds read (D9). */
const REST = "data-exact-at-rest";
/** The attribute that marks what a round can restyle, so it starts no transition: a reader, or with `subtree` a reader
 * and its descendants. */
const ROUND = "data-exact-round";
/** Every input's property, by family then edge, in the order a reader's inputs are kept. */
const NAMES = FAMILIES.flatMap((f) => EDGES.map((e) => `--exact-${f}-${e}`));
/** The inherited properties that take a length, through which a reader's observation reaches its descendants' styles. */
const INHERITED = new Set(["font", "font-size", "line-height", "letter-spacing", "word-spacing", "text-indent", "border-spacing",
  "tab-size", "text-underline-offset", "-webkit-text-stroke-width"]);
/** D9's bound on the rounds of one transaction. */
export const ROUNDS = 24;

/** How far the unsafe area `band` (top right bottom left, from the viewport's edges, `width` by `height`) reaches into
 * the border box `r` (viewport coordinates) from each of its edges, at most that area's depth there. */
export function overlap(r, band, width, height) {
  const depth = (value, most) => Math.min(Math.max(0, value), Math.max(0, most));
  return [depth(band[0] - r.top, band[0]), depth(r.right - (width - band[1]), band[1]),
    depth(r.bottom - (height - band[2]), band[2]), depth(band[3] - r.left, band[3])];
}

/** Each of `readers` (elements) given its own observations as its inputs, in rounds until they hold, against the
 * document's unsafe area `band` (top right bottom left; the browser's `env()` unless given) and the docked keyboard's
 * depth `keyboard` (the browser's unless given). Returns how many rounds ran and the readers that had not settled at the
 * bound. A reader out of the document, or with no box, keeps the input it has (D9). */
export function observe(readers, band = documentBand(), keyboard = keyboardInset()) {
  if (!readers.length) return { rounds: 0, unsettled: [] };
  // Each reader's inputs are only those its style reads: kept, written and compared alone
  const reads = new Map(readers.map((el) => [el, consumed(el)]));
  const root = document.documentElement, before = new Map(readers.map((el) => [el, reads.get(el).inputs.map((i) => el.style.getPropertyValue(NAMES[i]))]));
  // The page's own style changes first, so what they start transitions as it would: the rule then holds only the rounds'
  root.getBoundingClientRect();
  // Each reader's scrollers, its containing blocks' chain: a round moves lengths, never what makes a containing block,
  // so the chains hold for the transaction, and readers that share ancestors read each one's style once
  // Only a scroll container has an offset, so only those in the chain are read
  const box = styles(), scroller = document.scrollingElement;
  const chains = new Map(readers.map((el) => [el, chain(el, box).filter((a) => a === scroller || box(a).scrolls)]));
  // The scrollers a round's values can clamp: each reader's own and those that move it
  const scrollers = new Map([...readers.filter((el) => box(el).scrolls), ...[...chains.values()].flat()].map((a) => [a, [a.scrollLeft, a.scrollTop]]));
  const sticky = [...new Set(readers.flatMap(ancestry))].filter((a) => box(a).position === "sticky");
  // Whether a settled change could start a transition: a reader with one, or one whose subtree a round restyles (read
  // before the marks zero them)
  const transitional = readers.some((el) => reads.get(el).subtree || box(el).transitions);
  ensureRoundsRule();
  // Only what a round can restyle is marked, so the marks restyle the readers and the subtrees they hand an observation
  // to, never the whole document
  for (const [el, { subtree }] of reads) el.setAttribute(ROUND, subtree ? "subtree" : "");
  sticky.forEach((a) => a.setAttribute(REST, ""));
  let result, restore = false;
  try {
    // A sticky ancestor held at rest is positioned as it was, so the chains read before the marks still hold
    result = rounds(readers, reads, chains, band, keyboard, box);
    // The last round's values are the settled ones. Where nothing marked can transition, no sticky box was held and no
    // scroller moved (read from the rounds' last layout), they stay as written: putting the provisional values back and
    // writing the settled ones again would lay the page out once more for the same result. Otherwise the provisional
    // values go back while the marks still hold their transitions, and each scroller to where it was
    restore = transitional || sticky.length > 0 || [...scrollers].some(([a, [x, y]]) => a.scrollLeft !== x || a.scrollTop !== y);
    if (restore) {
      for (const [el, was] of before) reads.get(el).inputs.forEach((i, k) => was[k] ? own(el, NAMES[i], was[k]) : drop(el, NAMES[i]));
      sticky.forEach((a) => a.removeAttribute(REST));
      root.getBoundingClientRect();
      for (const [a, [x, y]] of scrollers) if (a.scrollLeft !== x || a.scrollTop !== y) a.scrollTo({ left: x, top: y, behavior: "instant" });
    }
  } finally {
    sticky.forEach((a) => a.removeAttribute(REST));
    for (const el of readers) el.removeAttribute(ROUND);
  }
  // With the marks gone, the settled values in once, so only that change reaches transition processing
  if (restore) {
    getComputedStyle(root).color;
    for (const [el, inputs] of result.written) write(el, reads.get(el).inputs, inputs);
  }
  return { rounds: result.rounds, unsettled: result.unsettled };
}

/** What `el`'s own style reads: the inputs it uses, by their index in `NAMES`, and whether it hands one to its
 * descendants, through a custom property it declares or an inherited length, so a round can restyle them too. Read from
 * its `style` attribute, as `readers` finds it (CSSOM leaves a shorthand's longhands empty where it holds a `var()`); a
 * reader whose inline style reads none is taken to read them all. */
function consumed(el) {
  const inputs = new Set();
  let subtree = false;
  for (const [property, value] of declarations(el.getAttribute("style") ?? "")) {
    if (!value.includes("var(--exact-")) continue;
    NAMES.forEach((name, i) => { if (value.includes(`var(${name}`)) inputs.add(i); });
    subtree ||= property.startsWith("--") || INHERITED.has(property.toLowerCase());
  }
  // One whose inline style shows no read reads through a rule (a look's, a test's): every input, and its subtree marked
  if (!inputs.size) return { inputs: NAMES.map((_, i) => i), subtree: true };
  return { inputs: [...inputs].sort((a, b) => a - b), subtree };
}

/** The declarations of an inline style's text, each `[property, value]`: split at the semicolons outside brackets and
 * quotes, each at its first colon. */
function declarations(text) {
  const out = [];
  let depth = 0, quote = "", start = 0;
  for (let i = 0; i <= text.length; i++) {
    const c = text[i];
    if (quote) { if (c === "\\") i++; else if (c === quote) quote = ""; continue; }
    if (c === "\"" || c === "'") quote = c;
    else if (c === "(" || c === "[") depth++;
    else if (c === ")" || c === "]") depth--;
    else if ((c === ";" && depth === 0) || i === text.length) {
      const decl = text.slice(start, i), colon = decl.indexOf(":");
      if (colon > 0) out.push([decl.slice(0, colon).trim(), decl.slice(colon + 1).trim()]);
      start = i + 1;
    }
  }
  return out;
}

/** The readers in `root`, in document order: each element whose own style uses an observation's input, a family's
 * `var(--exact-…)` (D9). The web's targets write each element's style inline, so its `style` attribute says what it
 * reads; one that only declares an input, as an ancestor's value, reads none. Activation (stage 5) takes them at each
 * commit; tests until then. */
export function readers(root = document) {
  return [...root.querySelectorAll(FAMILIES.map((f) => `[style*="var(--exact-${f}-"]`).join(", "))];
}

/** The readers a running animation or transition can move (D17, "The web: invalidation"): those in the layout
 * dependency closure of an animated element, which is the element itself and its descendants, its ancestors, and
 * everything after it in document order, its following siblings' and its ancestors' following siblings' subtrees; for
 * an item of a flex or grid container, the container's whole subtree. */
export function moving(readers, animations = document.getAnimations()) {
  // In a flex or grid container an item moves its earlier siblings too, as a centred row's first item moves as the second
  // widens, so the animated item stands for its whole formatting context
  const context = (el) => {
    const parent = el.parentElement;
    return parent && /flex|grid/.test(getComputedStyle(parent).display) ? parent : el;
  };
  const animated = [...new Set(animations.filter((a) => a.playState === "running").map((a) => a.effect?.target).filter(Boolean).map(context))];
  return readers.filter((r) => animated.some((a) => a === r || a.contains(r) || r.contains(a)
    || (a.compareDocumentPosition(r) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0));
}

/** Measure the readers an animation moves each frame while one runs (D17), in rounds as a commit's, until none moves
 * them; returns a function that stops it. Activation (stage 5) starts it as a commit's rounds end; tests until then. */
export function follow(readers, band, keyboard) {
  let frame = 0, last = [];
  const step = () => {
    const now = moving(readers);
    // Once nothing moves them, those it moved last are measured once more, where they came to rest
    const measured = now.length ? now : last;
    if (measured.length) observe(measured, band ?? documentBand(), keyboard ?? keyboardInset());
    last = now;
    frame = now.length ? requestAnimationFrame(step) : 0;
  };
  frame = requestAnimationFrame(step);
  return () => cancelAnimationFrame(frame);
}

/** The rule under which a round's values start no transition, on what the rounds mark: important in the page's first
 * cascade layer, which wins
 * over every important declaration of the page's own sheets, layered after it or unlayered (CSS Cascade 5, "Cascade
 * Layers"). */
function ensureRoundsRule() {
  if (document.getElementById("exact-rounds")) return;
  const style = document.createElement("style");
  style.id = "exact-rounds";
  style.textContent = `@layer exact-rounds; @layer exact-rounds { [${ROUND}], [${ROUND}="subtree"] * { transition-duration: 0s !important; transition-delay: 0s !important }`
    + ` [${REST}] { position: relative !important; inset: auto !important } }`;
  document.head.prepend(style);
}

/** The inputs a reader reads, by their index in `NAMES`, from `inputs` (its safe area's, then its keyboard's twice). */
function write(el, reads, inputs) {
  for (const i of reads) own(el, NAMES[i], `${inputs[i]}px`);
}

/** D9's rounds over `readers`, returning the inputs each settled on, how many rounds ran and the readers still moving.
 * Each round reads every reader's box in one pass and writes every input it moves in one batch. A reader's next input is
 * D9's secant step on its residuals, the measured less the input, of this round and the last (`step`), within half a
 * device pixel of settled; its box is read as laid out, with every scroll offset at zero. */
function rounds(readers, reads, chains, band, keyboard, box) {
  const samples = readers.map(() => []);
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const keys = [0, 0, keyboard, 0], caps = readers.map(() => [...band, ...keys, ...keys]);
  const tolerance = 0.5 / (globalThis.devicePixelRatio || 1);
  // Each reader advances from the input it holds; a new reader with a box starts at 0 (D9)
  const written = new Map();
  readers.forEach((el) => {
    // Only the inputs its style reads, which are all it holds
    const only = reads.get(el).inputs, now = NAMES.map(() => 0);
    let had = false;
    for (const i of only) {
      const v = el.style.getPropertyValue(NAMES[i]);
      if (v.endsWith("px")) { now[i] = parseFloat(v) || 0; had = true; }
    }
    if (had) written.set(el, now);
    else if (el.isConnected && el.getClientRects().length) {
      const zero = NAMES.map(() => 0);
      written.set(el, zero);
      write(el, reads.get(el).inputs, zero);
    }
  });
  const held = over(readers, box);
  // Each round reads every box once, laid out with every scroll offset at zero, and the area each reader observes, which
  // a reader's input can move
  const read = () => {
    const seen = new Map();
    const at = (el) => {
      if (!seen.has(el)) seen.set(el, el.isConnected && el.getClientRects().length ? laid(el, chains.get(el) ?? scrolling(el, box)) : null);
      return seen.get(el);
    };
    return readers.map((el, i) => {
      const r = at(el);
      if (!r) return null;
      const sides = area(held.get(el), band, width, height, at, box, tolerance), mine = edgesOf(r, width, height);
      sides.forEach(([outer, inner], e) => { caps[i][e] = Math.max(0, inner - outer); });
      const k = overlap(r, keys, width, height);
      return [...mine.map((d, e) => depth(sides[e], d)), ...k, ...k];
    });
  };
  let measured = read();
  for (let round = 0; round < ROUNDS; round++) {
    let moved = false;
    readers.forEach((el, i) => {
      // A reader with no box is not measured: it keeps its input until it has a box again (D9). An input held from the
      // last commit holds only where it is the measurement, so a reader at rest takes its exact value
      const input = written.get(el), m = measured[i], only = reads.get(el).inputs;
      if (m == null || settled(input, m, round ? tolerance : 0, only)) return;
      const next = input == null || !round ? m : step(input, m, samples[i], caps[i], tolerance);
      written.set(el, next);
      write(el, only, next);
      moved = true;
    });
    if (!moved) return { written, rounds: round, unsettled: [] };
    measured = read();
  }
  // The bound: whoever the last batch leaves short of its measurement is unsettled
  return { written, rounds: ROUNDS, unsettled: readers.filter((el, i) => measured[i] != null && !settled(written.get(el), measured[i], tolerance, reads.get(el).inputs)) };
}

/** `measure()` with each sticky ancestor of `boxes` held where layout puts it (D9), as the rounds read readers, and
 * nothing a round marks: the reach, extension and inset measurements read the same layout the observations do. */
export function atRest(boxes, box, measure) {
  // Each ancestor once, however many boxes share it
  const seen = new Set();
  for (const el of boxes) for (let a = el; a && !seen.has(a); a = a.parentElement) seen.add(a);
  const sticky = [...seen].filter((a) => box(a).position === "sticky");
  if (!sticky.length) return measure();
  ensureRoundsRule();
  sticky.forEach((a) => a.setAttribute(REST, ""));
  try {
    return measure();
  } finally {
    sticky.forEach((a) => a.removeAttribute(REST));
  }
}

/** A box as laid out with every scroll offset at zero, given the scroll containers that move it (`unscrolled`). */
export function laid(el, scrollers) {
  const b = el.getBoundingClientRect(), [dx, dy] = unscrolled(scrollers);
  return { top: b.top + dy, right: b.right + dx, bottom: b.bottom + dy, left: b.left + dx };
}

/** The scroll containers in `el`'s containing blocks' chain, which move it. */
export const scrolling = (el, box) => chain(el, box).filter((a) => a === document.scrollingElement || box(a).scrolls);

/** Whether each input a reader reads (`only`, indices into `input`) is within `tolerance` of `measured`, a reader with no
 * box matching only another. */
const settled = (input, measured, tolerance, only) =>
  input === measured || (input != null && measured != null && only.every((i) => Math.abs(input[i] - measured[i]) <= tolerance));

/** D9's step on each input, as the kernel's solve takes it: where the residual `measured - input` is within `tolerance`
 * the input holds; with the last round's sample on that input, the secant through both, clamped to 0 and the input's
 * most `caps`, or that range's end where the line's root lies behind the input or the residual repeats while the input
 * moved; else the measurement. `last` keeps each input's sample for the next round. */
function step(input, measured, last, caps, tolerance) {
  return input.map((x, e) => {
    const m = measured[e], r = m - x, prior = last[e];
    last[e] = [x, r];
    if (Math.abs(r) <= tolerance) return x;
    if (!prior || prior[0] === x) return m;
    const [x0, r0] = prior;
    if (r === r0) return r > 0 ? caps[e] : 0;
    const candidate = x - (r * (x - x0)) / (r - r0);
    if (!Number.isFinite(candidate)) return m;
    const ahead = r > 0 ? candidate > x : candidate < x;
    return ahead ? Math.min(Math.max(candidate, 0), caps[e]) : r > 0 ? caps[e] : 0;
  });
}

/** How far a box lies from where it would with every scroll offset at zero, x then y (D9), given its `scrollers`, the
 * scroll containers of its `chain`: the offsets of the scrollers that move it, those in its containing blocks' chain, the
 * document's included unless a fixed box with no containing ancestor ends it. */
/** What shapes the area each reader observes inside its owner (D9, D11, D12, D14), found once per transaction as no round
 * changes which box holds another: outermost first, each box that reserves part of the unsafe area inside it (D11: an
 * additional inset, which may be negative, then safe-area padding, which the emitter writes inline in `RESERVED`'s
 * registered properties) or whose route's transparent bar lies over the reader (`routes.js` `clearBar`); and whether a
 * scroller holds the reader (`cut`), which takes what reaches it on every edge, so the reader observes nothing of it or
 * of what lies outside it (a scroller that ignores the area is D4's). A box's reservation is read again each round, as a
 * reader's input can change it. The boxes inside a route take its bar, as content under a UIKit bar does; the route
 * itself does not, nor a box inside the bar, since the bar is the route's first child in its flow (neutral.css) and
 * either would move it by what it reads. A box observes only what reaches it, not its own reservation, which its
 * descendants observe. */
export function over(readers, box) {
  reserves = register(RESERVED);
  const own = new Map(), held = new Map();
  const step = (a) => {
    if (!own.has(a)) {
      const header = a.matches(TRANSPARENT) ? liveChildren(a).find((c) => c.hasAttribute("data-exact-clear")) ?? null : null;
      own.set(a, { box: a, header, reserving: reserves && a.matches(RESERVING), scroller: a.matches(SCROLLING) && box(a).scrollView });
    }
    return own.get(a);
  };
  for (const el of readers) {
    const steps = [];
    let cut = false;
    for (let a = el.parentElement?.closest(HOLDS); a; a = a.parentElement?.closest(HOLDS)) {
      const found = step(a);
      if (found.scroller) { cut = true; break; }
      const header = found.header?.contains(el) ? null : found.header;
      if (header || found.reserving) steps.unshift(header === found.header ? found : { ...found, header });
    }
    if (steps.length || cut) held.set(el, { steps, cut });
  }
  return held;
}
const TRANSPARENT = '[data-navigationbar="transparent"]';
/** A box whose inline style declares a reservation, as the emitter writes one. */
const RESERVING = '[style*="--exact-additional-safe-area-"], [style*="--exact-safe-area-padding-"]';
/** A box that may scroll: one whose inline style says how it overflows, as the emitter writes it, and the two that
 * neutral.css makes scroll, the coarse pointer's tab bar and an open route popover; its computed overflow decides. */
const SCROLLING = '[style*="overflow"], [role="tablist"][data-exact-tab-bar], [data-exact-route-popover]';
/** The boxes that can shape a reader's area. */
const HOLDS = `${TRANSPARENT}, ${RESERVING}, ${SCROLLING}`;

/** The unsafe area a reader observes (D9) on each edge, as the kernel's `area` gives it: how far in from the viewport's
 * edge its outer and inner sides lie, `[outer, inner]`, top right bottom left. From the viewport's unsafe area `band`,
 * or nothing where a scroller holds the reader (`held.cut`); then, outermost first, each box in `held.steps` on each edge
 * it reserves: what reaches its padding edge, its additional inset added, clamped at 0, its safe-area padding added, from
 * that edge, its area joining the one it lies in where its padding edge meets that one's inner side within D9's
 * `tolerance` and starting apart otherwise; and a route's transparent bar from the route's padding edge to the bar's
 * bottom, on the top edge. `at` reads a box as laid out, once a round, and `box` its style once a transaction. */
export function area(held, band, width, height, at, box, tolerance) {
  const sides = [0, 1, 2, 3].map((e) => [0, held?.cut ? 0 : band[e]]);
  for (const step of held?.steps ?? []) {
    const r = at(step.box);
    if (!r) continue;
    const b = box(step.box).borders, edges = [r.top + b[0], width - r.right + b[1], height - r.bottom + b[2], r.left + b[3]];
    const v = step.reserving ? reservation(getComputedStyle(step.box)) : null;
    const bar = step.header ? at(step.header) : null;
    for (let e = 0; e < 4; e++) {
      const added = v?.added[e] ?? 0, padding = v?.padding[e] ?? 0, barred = e === 0 && !!bar;
      if (!added && !padding && !barred) continue;
      const inside = Math.max(0, depth(sides[e], edges[e]) + added) + padding;
      const inner = barred ? Math.max(edges[e] + inside, bar.bottom) : edges[e] + inside;
      const joins = edges[e] <= sides[e][1] + tolerance;
      sides[e] = [joins ? Math.min(sides[e][0], edges[e]) : edges[e], inner];
    }
  }
  return sides;
}

/** How far an area `[outer, inner]` reaches past an edge `at` in from the viewport's edge: at least 0, at most its depth. */
export const depth = ([outer, inner], at) => Math.min(Math.max(0, inner - at), Math.max(0, inner - outer));

/** A box's edges as distances in from the viewport's edges, top right bottom left. */
export const edgesOf = (r, width, height) => [r.top, width - r.right, height - r.bottom, r.left];

export function unscrolled(scrollers) {
  let dx = 0, dy = 0;
  for (const a of scrollers) {
    dx += a.scrollLeft;
    dy += a.scrollTop;
  }
  return [dx, dy];
}

/** The elements whose scrolling moves `el` (CSS Positioned Layout 3, "Containing Blocks"): each ancestor that is its
 * containing block or one of that block's, the document's scroller last, unless the chain ends at a fixed box that only
 * the viewport contains. An in-flow box's containing block is its parent's box; an absolute one's its nearest positioned
 * ancestor or one that contains fixed boxes; a fixed one's its nearest that contains fixed boxes. */
export function chain(el, box = styles()) {
  const scroller = document.scrollingElement, found = [];
  let position = box(el).position;
  for (let a = el.parentElement; a; a = a.parentElement) {
    if (a === scroller) break;
    const b = box(a);
    // A box that generates no box contains nothing (CSS Display 3 §2.5)
    if (b.display === "contents") continue;
    const contains = position === "fixed" ? b.fixed : position === "absolute" ? b.position !== "static" || b.fixed : true;
    if (!contains) continue;
    found.push(a);
    position = b.position;
  }
  if (scroller && (position !== "fixed" || box(scroller).fixed)) found.push(scroller);
  return found;
}

/** A transaction's reading of each element's computed position, of whether it contains fixed boxes, of whether it is
 * a scroll container (CSS Overflow 3: an axis neither `visible` nor `clip`) and of whether it has a transition that
 * runs, once each: the last three only when asked. */
export function styles() {
  const known = new Map();
  return (el) => {
    let b = known.get(el);
    if (!b) {
      const s = getComputedStyle(el);
      let fixed, scrolls, scrollView, transitions, borders, stops, drawn;
      b = { position: s.position, display: s.display, get fixed() { return fixed ??= fixedContainer(s); },
        get scrollView() { return scrollView ??= [s.overflowX, s.overflowY].some((o) => o === "auto" || o === "scroll"); },
        get borders() { return borders ??= ["Top", "Right", "Bottom", "Left"].map((e) => parseFloat(s[`border${e}Width`]) || 0); },
        get stops() { return stops ??= stopping(s, this.borders); },
        get drawn() { return drawn ??= ["filter", "clipPath", "mask"].some((p) => (s[p] ?? "none") !== "none" && s[p] !== ""); },
        get scrolls() { return scrolls ??= [s.overflowX, s.overflowY].some((o) => o !== "visible" && o !== "clip"); },
        get transitions() { return transitions ??= starts(s); } };
      known.set(el, b);
    }
    return b;
  };
}

/** The registered properties the emitter writes a reserving box's lengths in (D11), each a `<length>` that does not
 * inherit, `0px` unsaid: its additional insets, which may be negative, then its safe-area padding, by edge. */
export const RESERVED = ["additional-safe-area", "safe-area-padding"].flatMap((f) => ["top", "right", "bottom", "left"].map((e) => `--exact-${f}-${e}`));

/** Whether the browser registers `RESERVED`; one that cannot inherits them and gives their text, so no box counts as
 * reserving there. */
let reserves = false;

/** What a box with the computed style `s` reserves (`RESERVED`), or null where it reserves nothing. */
function reservation(s) {
  if (!reserves) return null;
  const v = RESERVED.map((name) => parseFloat(s.getPropertyValue(name)) || 0);
  return v.some((x) => x !== 0) ? { added: v.slice(0, 4), padding: v.slice(4) } : null;
}

/** Whether a box with the computed style `s` has a transition that runs: one whose combined duration, its duration
 * (at least 0) and its delay, is above 0, the lists' items paired by index and the shorter repeated (CSS Transitions 1). */
function starts(s) {
  const times = (list) => list.split(",").map((t) => (parseFloat(t) || 0) * (t.trim().endsWith("ms") ? 1 : 1000));
  const durations = times(s.transitionDuration), delays = times(s.transitionDelay), n = Math.max(durations.length, delays.length);
  for (let i = 0; i < n; i++) if (Math.max(0, durations[i % durations.length]) + delays[i % delays.length] > 0) return true;
  return false;
}

/** On which edges (top right bottom left) a box with the computed style `s` stops a descendant's reach at its padding
 * edge (D13): it clips on that edge's axis, has a border there, or is drawn through a transform, a filter, a clip path or
 * a mask, which leaves the band no place. */
function stopping(s, borders) {
  const drawn = ["transform", "translate", "rotate", "scale", "perspective", "filter", "backdropFilter", "clipPath", "mask"].some((p) => (s[p] ?? "none") !== "none" && s[p] !== "");
  const clips = (o) => o !== "visible";
  const y = drawn || clips(s.overflowY), x = drawn || clips(s.overflowX);
  return [y || borders[0] > 0, x || borders[1] > 0, y || borders[2] > 0, x || borders[3] > 0];
}

/** Whether a box with the computed style `s` is the containing block of its fixed descendants, and so of its absolute
 * ones (CSS Transforms 2, Filter Effects 1, CSS Containment 2, CSS Will Change 1). */
const fixedContainer = (s) => ["transform", "translate", "rotate", "scale", "perspective", "filter", "backdropFilter"].some((p) => (s[p] ?? "none") !== "none")
  || s.transformStyle === "preserve-3d" || s.contentVisibility === "auto" || /\b(layout|paint|strict|content)\b/.test(s.contain ?? "")
  || /\b(transform|translate|rotate|scale|perspective|filter|backdrop-filter)\b/.test(s.willChange ?? "");

/** `el` and its ancestors. */
function ancestry(el) {
  const found = [];
  for (let a = el; a; a = a.parentElement) found.push(a);
  return found;
}

/** The document's unsafe area, top right bottom left, as the browser's `env(safe-area-inset-*)` gives it. */
export function documentBand() {
  const probe = document.createElement("div");
  probe.style.cssText = "position:fixed;visibility:hidden;pointer-events:none;" + EDGES.map((e) => `padding-${e}:env(safe-area-inset-${e},0px)`).join(";");
  document.documentElement.appendChild(probe);
  const s = getComputedStyle(probe), band = EDGES.map((e) => parseFloat(s[`padding-${e}`.replace(/-(\w)/, (_, c) => c.toUpperCase())]) || 0);
  probe.remove();
  return band;
}

