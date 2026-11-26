// @ref LLP 1123 D9, D17 — each reader's own safe-area observation on the web. A box that reads
// `var(--exact-safe-area-inset-*)` reads how far its owner's unsafe area reaches into its own border box from each
// edge, at most that area's depth, as UIKit gives each view its `safeAreaInsets`. The host writes that input as an
// inline custom property on the reader's element, `initial` (the guaranteed-invalid value) where it has none, so a
// reader never inherits an ancestor's value. A reader's input can move it, and so the readers' inputs: the host runs
// D9's rounds in one task before the browser paints, each round reading every reader's box in one pass and writing
// every changed input in one batch, until the inputs hold or D9's bound of 24 rounds; a reader still moving then is
// unsettled. A round's values are provisional: under `exact-rounds` (a zero transition duration and delay on the
// document) none starts a CSS transition or delays what a round reads, and once the rounds end each reader's inputs go
// back to what they were before the task, the class goes, and the settled inputs are written once, so only that change
// reaches transition processing (CSS Transitions 1, "Starting of transitions"), and each scroller a round's values
// clamped, a reader's own or one that moves it, is scrolled back to where it was, so only that change is clamped. A reader is read as laid out, with every
// scroll offset at zero: the offsets of the scrollers in its containing blocks' chain come off, and a sticky box in its
// ancestry is read where layout puts it, without the offset its scrollers give it, as the kernel lays it out. A transition already running elsewhere
// keeps running; one running on a property a reader's input moves restarts from its previous end value. The document is
// the one owner until the web's owners exist: its unsafe area is the layout viewport's edges that
// `env(safe-area-inset-*)` gives. The page's keyboard is the session's (D7, D17), a docked keyboard's depth up from the
// layout viewport's bottom (`keyboardInset`: the Virtual Keyboard API's rectangle, else the visual viewport's
// shortfall), from which each reader's `--exact-keyboard-inset-*` and `--exact-keyboard-undocked-inset-*` are its
// overlap from its bottom edge, not limited by its size, as the kernel derives them for the native hosts, and 0 while
// no keyboard shows. `keyboardInset` gives only that depth, so the two families read alike, a floating keyboard taken
// as docked. Activation (stage 5) runs it at each commit; until then only its tests do.
import { keyboardInset } from "./environment.js";

const EDGES = ["top", "right", "bottom", "left"];
/** Each input, by its property's family, in the order a reader's inputs are kept. */
const FAMILIES = ["safe-area-inset", "keyboard-inset", "keyboard-undocked-inset"];
/** The attribute that holds a sticky box where layout puts it while the rounds read (D9). */
const REST = "data-exact-at-rest";
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
 * bound. A reader out of the document, or with no box, takes `initial`. */
export function observe(readers, band = documentBand(), keyboard = keyboardInset()) {
  if (!readers.length) return { rounds: 0, unsettled: [] };
  const names = FAMILIES.flatMap((f) => EDGES.map((e) => `--exact-${f}-${e}`));
  const root = document.documentElement, before = new Map(readers.map((el) => [el, names.map((n) => el.style.getPropertyValue(n))]));
  // The page's own style changes first, so what they start transitions as it would: the rule then holds only the rounds'
  root.getBoundingClientRect();
  // The scrollers a round's values can clamp: each reader's own and those that move it
  const scrollers = new Map([...readers, ...readers.flatMap(chain)].map((a) => [a, [a.scrollLeft, a.scrollTop]]));
  const sticky = [...new Set(readers.flatMap(ancestry))].filter((a) => getComputedStyle(a).position === "sticky");
  root.classList.add("exact-rounds");
  ensureRoundsRule();
  sticky.forEach((a) => a.setAttribute(REST, ""));
  try {
    const result = rounds(readers, band, keyboard);
    // The provisional values out, and each scroller where it was, then the settled ones in once the rounds' rule is gone
    for (const [el, was] of before) names.forEach((n, i) => was[i] ? el.style.setProperty(n, was[i]) : el.style.removeProperty(n));
    sticky.forEach((a) => a.removeAttribute(REST));
    root.getBoundingClientRect();
    for (const [a, [x, y]] of scrollers) if (a.scrollLeft !== x || a.scrollTop !== y) a.scrollTo({ left: x, top: y, behavior: "instant" });
    root.classList.remove("exact-rounds");
    getComputedStyle(root).color;
    for (const [el, inputs] of result.written) write(el, inputs);
    return { rounds: result.rounds, unsettled: result.unsettled };
  } finally {
    sticky.forEach((a) => a.removeAttribute(REST));
    root.classList.remove("exact-rounds");
  }
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
 * everything after it in document order, its following siblings' and its ancestors' following siblings' subtrees. */
export function moving(readers, animations = document.getAnimations()) {
  const animated = [...new Set(animations.filter((a) => a.playState === "running").map((a) => a.effect?.target).filter(Boolean))];
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

/** The rule under which a round's values start no transition: important in the page's first cascade layer, which wins
 * over every important declaration of the page's own sheets, layered after it or unlayered (CSS Cascade 5, "Cascade
 * Layers"). */
function ensureRoundsRule() {
  if (document.getElementById("exact-rounds")) return;
  const style = document.createElement("style");
  style.id = "exact-rounds";
  style.textContent = "@layer exact-rounds; @layer exact-rounds { .exact-rounds, .exact-rounds * { transition-duration: 0s !important; transition-delay: 0s !important }"
    + ` .exact-rounds [${REST}] { position: relative !important; inset: auto !important } }`;
  document.head.prepend(style);
}

/** Each of a reader's inputs, its safe area's, then its keyboard's twice, or `initial` for a reader with no box. */
function write(el, inputs) {
  FAMILIES.forEach((f, k) => EDGES.forEach((edge, e) => {
    el.style.setProperty(`--exact-${f}-${edge}`, inputs ? `${inputs[k * 4 + e]}px` : "initial");
  }));
}

/** D9's rounds over `readers`, returning the inputs each settled on, how many rounds ran and the readers still moving.
 * Each round reads every reader's box in one pass and writes every input it moves in one batch. A reader's next input is
 * D9's secant step on its residuals, the measured less the input, of this round and the last (`step`), within half a
 * device pixel of settled; its box is read as laid out, with every scroll offset at zero. */
function rounds(readers, band, keyboard) {
  const written = new Map(), samples = readers.map(() => []);
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const keys = [0, 0, keyboard, 0], caps = [...band, ...keys, ...keys];
  const tolerance = 0.5 / (globalThis.devicePixelRatio || 1);
  // Each round's offsets, which its values can clamp
  const read = () => readers.map((el) => {
    if (!el.isConnected || !el.getClientRects().length) return null;
    const b = el.getBoundingClientRect(), [dx, dy] = unscrolled(el);
    const r = { top: b.top + dy, right: b.right + dx, bottom: b.bottom + dy, left: b.left + dx };
    const k = overlap(r, keys, width, height);
    return [...overlap(r, band, width, height), ...k, ...k];
  });
  let measured = read();
  for (let round = 0; round < ROUNDS; round++) {
    let moved = false;
    readers.forEach((el, i) => {
      const input = written.get(el), m = measured[i];
      if (written.has(el) && settled(input, m, tolerance)) return;
      const next = input == null || m == null ? m : step(input, m, samples[i], caps, tolerance);
      written.set(el, next);
      write(el, next);
      moved = true;
    });
    if (!moved) return { written, rounds: round, unsettled: [] };
    measured = read();
  }
  // The bound: whoever the last batch leaves short of its measurement is unsettled
  return { written, rounds: ROUNDS, unsettled: readers.filter((el, i) => !settled(written.get(el), measured[i], tolerance)) };
}

/** Whether each of `input` is within `tolerance` of `measured`, a reader with no box matching only another. */
const settled = (input, measured, tolerance) =>
  input === measured || (input != null && measured != null && input.every((v, i) => Math.abs(v - measured[i]) <= tolerance));

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

/** How far `el`'s box lies from where it would with every scroll offset at zero, x then y (D9): the offsets of the
 * scrollers that move it, those in its containing blocks' chain, the document's included unless a fixed box with no
 * containing ancestor ends it. */
function unscrolled(el) {
  let dx = 0, dy = 0;
  for (const a of chain(el)) {
    dx += a.scrollLeft;
    dy += a.scrollTop;
  }
  return [dx, dy];
}

/** The elements whose scrolling moves `el` (CSS Positioned Layout 3, "Containing Blocks"): each ancestor that is its
 * containing block or one of that block's, the document's scroller last, unless the chain ends at a fixed box that only
 * the viewport contains. An in-flow box's containing block is its parent's box; an absolute one's its nearest positioned
 * ancestor or one that contains fixed boxes; a fixed one's its nearest that contains fixed boxes. */
function chain(el) {
  const scroller = document.scrollingElement, found = [];
  let position = getComputedStyle(el).position;
  for (let a = el.parentElement; a; a = a.parentElement) {
    if (a === scroller) break;
    const s = getComputedStyle(a);
    const contains = position === "fixed" ? fixedContainer(s) : position === "absolute" ? s.position !== "static" || fixedContainer(s) : true;
    if (!contains) continue;
    found.push(a);
    position = s.position;
  }
  if (scroller && (position !== "fixed" || fixedContainer(getComputedStyle(scroller)))) found.push(scroller);
  return found;
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

