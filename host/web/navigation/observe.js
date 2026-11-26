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
// reaches transition processing (CSS Transitions 1, "Starting of transitions"). A transition already running elsewhere
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
  const names = FAMILIES.flatMap((f) => EDGES.map((e) => `--exact-${f}-${e}`));
  const root = document.documentElement, before = new Map(readers.map((el) => [el, names.map((n) => el.style.getPropertyValue(n))]));
  root.classList.add("exact-rounds");
  ensureRoundsRule();
  try {
    const result = rounds(readers, band, keyboard);
    // The provisional values out, then the settled ones in once the rounds' rule is gone
    for (const [el, was] of before) names.forEach((n, i) => was[i] ? el.style.setProperty(n, was[i]) : el.style.removeProperty(n));
    root.getBoundingClientRect();
    root.classList.remove("exact-rounds");
    getComputedStyle(root).color;
    for (const [el, inputs] of result.written) write(el, inputs);
    return { rounds: result.rounds, unsettled: result.unsettled };
  } finally {
    root.classList.remove("exact-rounds");
  }
}

/** The rule under which a round's values start no transition. */
function ensureRoundsRule() {
  if (document.getElementById("exact-rounds")) return;
  const style = document.createElement("style");
  style.id = "exact-rounds";
  style.textContent = ".exact-rounds, .exact-rounds * { transition-duration: 0s !important; transition-delay: 0s !important }";
  document.head.appendChild(style);
}

/** Each of a reader's inputs, its safe area's, then its keyboard's twice, or `initial` for a reader with no box. */
function write(el, inputs) {
  FAMILIES.forEach((f, k) => EDGES.forEach((edge, e) => {
    el.style.setProperty(`--exact-${f}-${edge}`, inputs ? `${inputs[k * 4 + e]}px` : "initial");
  }));
}

/** D9's rounds over `readers`, returning the inputs each settled on, how many rounds ran and the readers still moving. */
function rounds(readers, band, keyboard) {
  const written = new Map();
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const keys = [0, 0, keyboard, 0];
  const read = () => readers.map((el) => {
    if (!el.isConnected || !el.getClientRects().length) return null;
    const r = el.getBoundingClientRect(), k = overlap(r, keys, width, height);
    return [...overlap(r, band, width, height), ...k, ...k];
  });
  for (let round = 0; round < ROUNDS; round++) {
    // One read pass, each reader's box as the last batch laid it out, then one write batch of the inputs that changed
    const next = read();
    if (readers.every((el, i) => written.has(el) && same(written.get(el), next[i]))) return { written, rounds: round, unsettled: [] };
    readers.forEach((el, i) => {
      if (written.has(el) && same(written.get(el), next[i])) return;
      written.set(el, next[i]);
      write(el, next[i]);
    });
  }
  // The bound: whoever the last batch still moves is unsettled
  const last = read();
  return { written, rounds: ROUNDS, unsettled: readers.filter((el, i) => !same(written.get(el), last[i])) };
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

const same = (a, b) => a === b || (a != null && b != null && a.every((v, i) => Math.abs(v - b[i]) < 0.01));
