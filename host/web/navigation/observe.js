// @ref LLP 1123 D9, D17 — each reader's own safe-area observation on the web. A box that reads
// `var(--exact-safe-area-inset-*)` reads how far its owner's unsafe area reaches into its own border box from each edge,
// at most that area's depth, as UIKit gives each view its `safeAreaInsets`. The host writes that input as an inline
// custom property on the reader's element, `initial` (the guaranteed-invalid value) where it has none, so a reader
// never inherits an ancestor's value. A reader's input can move it, and so the readers' inputs: the host runs D9's
// rounds in one task before the browser paints, each round reading every reader's box in one pass and writing every
// changed input in one batch, until the inputs hold or D9's bound of 24 rounds; a reader still moving then is
// unsettled. A round's values are provisional: under `exact-rounds` (a zero transition duration and delay on the
// document) none starts a CSS transition or delays what a round reads, and once the rounds end each reader's inputs go
// back to what they were before the task, the class goes, and the settled inputs are written once, so only that change
// reaches transition processing (CSS Transitions 1, "Starting of transitions"). A transition already running elsewhere
// keeps running; one running on a property a reader's input moves restarts from its previous end value. The document is the one owner until the web's owners exist: its unsafe area is the layout viewport's edges
// that `env(safe-area-inset-*)` gives. Activation (stage 5) runs it at each commit; until then only its tests do.

const EDGES = ["top", "right", "bottom", "left"];
/** D9's bound on the rounds of one transaction. */
export const ROUNDS = 24;

/** How far the unsafe area `band` (top right bottom left, from the viewport's edges, `width` by `height`) reaches into
 * the border box `r` (viewport coordinates) from each of its edges, at most that area's depth there. */
export function overlap(r, band, width, height) {
  const depth = (value, most) => Math.min(Math.max(0, value), Math.max(0, most));
  return [depth(band[0] - r.top, band[0]), depth(r.right - (width - band[1]), band[1]),
    depth(r.bottom - (height - band[2]), band[2]), depth(band[3] - r.left, band[3])];
}

/** Each of `readers` (elements) given its own observation as its input, in rounds until they hold, against the document's
 * unsafe area `band` (top right bottom left; the browser's `env()` unless given). Returns how many rounds ran and the
 * readers that had not settled at the bound. A reader out of the document, or with no box, takes `initial`. */
export function observe(readers, band = documentBand()) {
  const root = document.documentElement, before = new Map(readers.map((el) => [el, EDGES.map((e) => el.style.getPropertyValue(`--exact-safe-area-inset-${e}`))]));
  root.classList.add("exact-rounds");
  ensureRoundsRule();
  try {
    const result = rounds(readers, band);
    // The provisional values out, then the settled ones in once the rounds' rule is gone
    for (const [el, was] of before) EDGES.forEach((edge, e) => was[e] ? el.style.setProperty(`--exact-safe-area-inset-${edge}`, was[e]) : el.style.removeProperty(`--exact-safe-area-inset-${edge}`));
    root.getBoundingClientRect();
    root.classList.remove("exact-rounds");
    getComputedStyle(root).color;
    for (const [el, inputs] of result.written) EDGES.forEach((edge, e) => el.style.setProperty(`--exact-safe-area-inset-${edge}`, inputs ? `${inputs[e]}px` : "initial"));
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

/** D9's rounds over `readers`, returning the inputs each settled on, how many rounds ran and the readers still moving. */
function rounds(readers, band) {
  const written = new Map();
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const read = () => readers.map((el) => el.isConnected && el.getClientRects().length ? overlap(el.getBoundingClientRect(), band, width, height) : null);
  for (let round = 0; round < ROUNDS; round++) {
    // One read pass, each reader's box as the last batch laid it out, then one write batch of the inputs that changed
    const next = read();
    if (readers.every((el, i) => written.has(el) && same(written.get(el), next[i]))) return { written, rounds: round, unsettled: [] };
    readers.forEach((el, i) => {
      if (written.has(el) && same(written.get(el), next[i])) return;
      written.set(el, next[i]);
      EDGES.forEach((edge, e) => el.style.setProperty(`--exact-safe-area-inset-${edge}`, next[i] ? `${next[i][e]}px` : "initial"));
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
