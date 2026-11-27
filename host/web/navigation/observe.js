// @ref LLP 1123 D9, D17 — each reader's own safe-area observation on the web. A box that reads
// `var(--exact-safe-area-inset-*)` reads how far its owner's unsafe area reaches into its own border box from each
// edge, at most that area's depth, as UIKit gives each view its `safeAreaInsets`. The host writes that input as an
// inline custom property on the reader's element, `initial` (the guaranteed-invalid value) where it has none, so a
// reader never inherits an ancestor's value. A reader's input can move it, and so the readers' inputs: the host runs
// D9's rounds in one task before the browser paints, each round reading every reader's box in one pass and writing
// every changed input in one batch, until the inputs hold or D9's bound of 24 rounds; a reader still moving then is
// unsettled. A round's values are provisional: the elements a round can restyle take a zero transition duration and
// delay while the rounds run (`data-exact-round`: each reader, and the subtree of one that hands an observation down), so
// none starts a CSS transition or delays what a round reads, and once the rounds end each reader's inputs go back to
// what they were before the task, the marks go, and the settled inputs are written once, so only that change reaches
// transition processing (CSS Transitions 1, "Starting of transitions"), and each scroller a round's values
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
 * bound. A reader out of the document, or with no box, takes `initial`. */
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
  ensureRoundsRule();
  // Only what a round can restyle is marked, so the marks restyle the readers and the subtrees they hand an observation
  // to, never the whole document
  for (const [el, { subtree }] of reads) el.setAttribute(ROUND, subtree ? "subtree" : "");
  sticky.forEach((a) => a.setAttribute(REST, ""));
  try {
    // A sticky ancestor held at rest is positioned as it was, so the chains read before the marks still hold
    const result = rounds(readers, reads, chains, band, keyboard);
    // The provisional values out, and each scroller where it was, then the settled ones in once the marks are gone
    for (const [el, was] of before) reads.get(el).inputs.forEach((i, k) => was[k] ? el.style.setProperty(NAMES[i], was[k]) : el.style.removeProperty(NAMES[i]));
    sticky.forEach((a) => a.removeAttribute(REST));
    root.getBoundingClientRect();
    for (const [a, [x, y]] of scrollers) if (a.scrollLeft !== x || a.scrollTop !== y) a.scrollTo({ left: x, top: y, behavior: "instant" });
    for (const el of readers) el.removeAttribute(ROUND);
    getComputedStyle(root).color;
    for (const [el, inputs] of result.written) write(el, reads.get(el).inputs, inputs);
    return { rounds: result.rounds, unsettled: result.unsettled };
  } finally {
    sticky.forEach((a) => a.removeAttribute(REST));
    for (const el of readers) el.removeAttribute(ROUND);
  }
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

/** The inputs a reader reads, by their index in `NAMES`, from `inputs` (its safe area's, then its keyboard's twice), or
 * `initial` for a reader with no box. */
function write(el, reads, inputs) {
  for (const i of reads) el.style.setProperty(NAMES[i], inputs ? `${inputs[i]}px` : "initial");
}

/** D9's rounds over `readers`, returning the inputs each settled on, how many rounds ran and the readers still moving.
 * Each round reads every reader's box in one pass and writes every input it moves in one batch. A reader's next input is
 * D9's secant step on its residuals, the measured less the input, of this round and the last (`step`), within half a
 * device pixel of settled; its box is read as laid out, with every scroll offset at zero. */
function rounds(readers, reads, chains, band, keyboard) {
  const written = new Map(), samples = readers.map(() => []);
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const keys = [0, 0, keyboard, 0], caps = [...band, ...keys, ...keys];
  const tolerance = 0.5 / (globalThis.devicePixelRatio || 1);
  // Each round's offsets, which its values can clamp
  const read = () => readers.map((el) => {
    if (!el.isConnected || !el.getClientRects().length) return null;
    const b = el.getBoundingClientRect(), [dx, dy] = unscrolled(chains.get(el));
    const r = { top: b.top + dy, right: b.right + dx, bottom: b.bottom + dy, left: b.left + dx };
    const k = overlap(r, keys, width, height);
    return [...overlap(r, band, width, height), ...k, ...k];
  });
  let measured = read();
  for (let round = 0; round < ROUNDS; round++) {
    let moved = false;
    readers.forEach((el, i) => {
      const input = written.get(el), m = measured[i], only = reads.get(el).inputs;
      if (written.has(el) && settled(input, m, tolerance, only)) return;
      const next = input == null || m == null ? m : step(input, m, samples[i], caps, tolerance);
      written.set(el, next);
      write(el, only, next);
      moved = true;
    });
    if (!moved) return { written, rounds: round, unsettled: [] };
    measured = read();
  }
  // The bound: whoever the last batch leaves short of its measurement is unsettled
  return { written, rounds: ROUNDS, unsettled: readers.filter((el, i) => !settled(written.get(el), measured[i], tolerance, reads.get(el).inputs)) };
}

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
function unscrolled(scrollers) {
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
function chain(el, box = styles()) {
  const scroller = document.scrollingElement, found = [];
  let position = box(el).position;
  for (let a = el.parentElement; a; a = a.parentElement) {
    if (a === scroller) break;
    const b = box(a);
    const contains = position === "fixed" ? b.fixed : position === "absolute" ? b.position !== "static" || b.fixed : true;
    if (!contains) continue;
    found.push(a);
    position = b.position;
  }
  if (scroller && (position !== "fixed" || box(scroller).fixed)) found.push(scroller);
  return found;
}

/** A transaction's reading of each element's computed position, of whether it contains fixed boxes, and of whether it
 * is a scroll container (CSS Overflow 3: an axis neither `visible` nor `clip`), once each: the last two only when
 * asked. */
function styles() {
  const known = new Map();
  return (el) => {
    let b = known.get(el);
    if (!b) {
      const s = getComputedStyle(el);
      let fixed, scrolls;
      b = { position: s.position, get fixed() { return fixed ??= fixedContainer(s); },
        get scrolls() { return scrolls ??= [s.overflowX, s.overflowY].some((o) => o !== "visible" && o !== "clip"); } };
      known.set(el, b);
    }
    return b;
  };
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

