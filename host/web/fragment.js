// @ref LLP 1153 D5 — the browser's "scroll to the fragment" (HTML: find a potential indicated element, scroll it into
// view, move the sequential focus navigation starting point to it), made by the runtime once a route's data has
// arrived: an Exact page's content comes after load, so the browser's own step finds nothing. The element is looked
// for in the visit's own route, since a route retained beneath may hold the same `id`; it is held at the start while
// the route's pictures load, as a browser holds its fragment in view while its document loads. Both web runtimes load
// this module at a fragment's first scroll: the JS target's router.js, which decides when, and the wasm host's glue.js,
// which runs the runner's `scrollToFragment`. `arm` comes with the router change that asks for a scroll, `scroll` once
// the data has settled, and `cancel` with the app's own `scrollIntoView`, which wins.

let armed = null, held = null;
// Where each history entry (navigation.js's `exact`) left the scrollers it moved: what Back or Forward between two
// fragments of one visit puts back, as the browser restores a traversed entry's scroll position. Weak references
// let removed routes go; one reference per element lets later scroll events update its place in the entry.
const kept = new Map(), references = new WeakMap(), entry = () => globalThis.history?.state?.exact;
function reference(el) {
  if (!references.has(el)) references.set(el, new WeakRef(el));
  return references.get(el);
}
/** A history push discards the forward entries, including the index it writes (navigation.js). */
export function forget(from) { for (const k of kept.keys()) if (k >= from) kept.delete(k); }
addEventListener("scroll", e => {
  if (held?.restore?.()) return;
  const at = entry(), el = e.target === document ? document.scrollingElement : e.target;
  if (at == null || !(el instanceof Element)) return;
  if (!kept.has(at)) kept.set(at, new Map());
  kept.get(at).set(reference(el), [el.scrollTop, el.scrollLeft]);
}, { capture: true, passive: true });

/** The route of `visit`: the element keyed by it that is not a navigation root, whose key is its top's. */
function route(root, visit) {
  for (const el of root?.querySelectorAll("[navigationKey]") ?? []) {
    if (el.getAttribute("navigationKey") === String(visit) && !el.hasAttribute("navigationBack") && !el.hasAttribute("data-exiting")) return el;
  }
  return null;
}
const scrolls = el => el && (el.scrollTop || el.scrollLeft || el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth);
/** Every element in the route that scrolls or is scrolled. */
const inside = r => [r, ...r.querySelectorAll("*")].filter(scrolls);
/** What a scroll to an element of the route can move: those, the route's ancestors that scroll, and the document's. */
function scrollers(r) {
  const out = [document.scrollingElement];
  for (let el = r.parentElement; el; el = el.parentElement) if (el !== document.scrollingElement && scrolls(el)) out.push(el);
  return [...out, ...inside(r)];
}
function keep(r, at = entry()) {
  if (at != null) kept.set(at, new Map(scrollers(r).map(el => [reference(el), [el.scrollTop, el.scrollLeft]])));
}
/** The fragment of a location: everything after its first `#`. */
const fragmentOf = url => { const at = url?.indexOf("#") ?? -1; return at < 0 ? "" : url.slice(at + 1); };
/** Whether the history entry shown is still `visit` at `fragment` (navigation.js writes `id` and `url`). */
export const shows = (visit, fragment, s = globalThis.history?.state) => !s || (s.id === visit && fragmentOf(s.url) === fragment);
/** The fragment percent-decoded, then UTF-8 decoded without BOM or fail (HTML, "find a potential indicated element"). */
function decoded(fragment) {
  const bytes = [], utf8 = new TextEncoder();
  for (let i = 0; i < fragment.length; i++) {
    if (fragment[i] === "%" && /^[0-9a-f]{2}$/i.test(fragment.slice(i + 1, i + 3))) { bytes.push(parseInt(fragment.slice(i + 1, i + 3), 16)); i += 2; }
    else bytes.push(...utf8.encode(fragment[i]));
  }
  try { return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(new Uint8Array(bytes)); } catch { return null; }
}
const byId = (r, id) => id ? [...r.querySelectorAll("[id]")].find(el => el.id === id && !el.closest("[data-exiting]")) ?? null : null;

/** A router change asked for a scroll to its top's fragment (`op`: its `top`, `fragment` and `activation`; `traversal`
 * when the browser's Back or Forward made it, `pushed` when it pushed a history entry of the visit, navigation.js;
 * `since`, the person's scrolls when it was made). A pushed entry keeps where the one it left had the route; a scroll
 * by the person since (`person.personScrolls`, counted by navigation.js from the page's start) cancels it, and the
 * hold of an earlier one ends. */
export function arm(root, op, person) {
  const r = route(root, op.top), at = entry();
  if (op.pushed && r && at != null) {
    keep(r, at - 1);
  }
  held?.abort();
  armed = { visit: op.top, fragment: op.fragment, activation: op.activation, traversal: !!op.traversal, since: op.since ?? person.personScrolls, person };
}
/** The app's own scroll: a fragment's scroll held gives way, and with `pending` (a `scrollIntoView`) one not yet made. */
export function cancel(pending = true) { held?.abort(); if (armed && pending) armed.authored = true; }

/** A retained visit traversed to the very URL already shown: the router need not emit a change. */
export function traverse(root, op, person, log) {
  const fragment = fragmentOf(op.url);
  arm(root, { ...op, fragment, traversal: true, pushed: false, since: person.personScrolls }, person);
  scroll(root, op.top, fragment, log);
}

function holding() {
  held?.abort();
  const h = held = new AbortController();
  h.signal.addEventListener("abort", () => { if (held === h) held = null; }, { once: true });
  return h;
}

/** Hold `target` at the start of its scrollers while `r`'s pictures load, until the person scrolls, it leaves, or the
 * entry shown is no longer `visit` at `fragment`. */
function hold(r, target, visit, fragment, person) {
  const loading = () => [...r.querySelectorAll("img")].some(img => !img.complete);
  held?.abort();
  if (!loading()) return;
  const h = holding(), since = person?.personScrolls, again = () => {
    if (!target.isConnected || person?.personScrolls !== since || !shows(visit, fragment)) return h.abort();
    target.scrollIntoView({ block: "start", inline: "nearest", behavior: "instant" });
    keep(r);
    if (!loading()) h.abort();
  };
  for (const type of ["load", "error"]) r.addEventListener(type, again, { capture: true, signal: h.signal });
}

/** Scroll `visit`'s route to `fragment` for the router change armed with `activation`: the element with that id, else
 * its decoded id, at the start of every scroller above it and the starting point of the next Tab; `top`, or no such
 * element, the route's top. A traversal puts back where its history entry left the route, or leaves it where it is. */
export function scroll(root, visit, fragment, log, activation) {
  const was = armed && (activation != null ? armed.activation === activation : armed.visit === visit && armed.fragment === fragment) ? armed : null;
  if (was) armed = null;
  if (was?.authored) return log(`fragment #${fragment}: the app's own scrollIntoView took its place`);
  if (was && was.person.personScrolls !== was.since) return log(`fragment #${fragment}: the person scrolled first; the route stays where it is`);
  const r = route(root, visit);
  if (!r) return log(`fragment #${fragment}: no route of visit ${visit} is on the page`);
  if (was?.traversal) {
    const at = entry(), place = new Map(kept.get(at)), since = was.person?.personScrolls, h = holding();
    const restore = h.restore = () => {
      if (h.signal.aborted) return false;
      if (!r.isConnected || entry() !== at || !shows(visit, fragment) || was.person?.personScrolls !== since) { h.abort(); return false; }
      for (const [ref, [top, left]] of place) {
        const el = ref.deref();
        if (el?.isConnected && (el === document.scrollingElement || r.contains(el) || el.contains(r))) el.scrollTo({ top, left, behavior: "instant" });
      }
      return true;
    };
    if (!place.size) { h.abort(); return; }
    // Chrome may keep aligning its own fragment until the document finishes loading. Those scrolls must neither
    // replace the saved place nor outlive this activation's cancellation by navigation or authored/user scrolling.
    restore();
    const settled = () => requestAnimationFrame(() => requestAnimationFrame(() => { if (restore() && document.readyState === "complete") h.abort(); }));
    addEventListener("hashchange", restore, { signal: h.signal });
    addEventListener("load", settled, { once: true, signal: h.signal });
    settled();
    return;
  }
  const id = fragment && decoded(fragment), target = fragment ? byId(r, fragment) ?? byId(r, id) : null;
  if (target) {
    target.scrollIntoView({ block: "start", inline: "nearest" });
    keep(r);
    hold(r, target, visit, fragment, was?.person);
    // The starting point: the target takes the focus, as the next Tab and a screen reader start from it, with no focus
    // ring, as a browser's own scroll to a fragment draws none. A field the person is typing in keeps it.
    if (document.activeElement?.matches?.("input, textarea, select, [contenteditable]")) return;
    if (!target.matches("a[href], button, input, select, textarea, [tabindex], [contenteditable]")) {
      if (target.exactAuthoredTabIndex === undefined) target.exactAuthoredTabIndex = null;
      target.setAttribute("tabindex", "-1");
      target.addEventListener("blur", () => target.removeAttribute("tabindex"), { once: true });
    }
    target.focus({ preventScroll: true, focusVisible: false });
    return;
  }
  if (fragment && id?.toLowerCase() !== "top") log(`fragment #${fragment}: no element in the route of visit ${visit} has that id; the route goes to its top`);
  for (const el of inside(r)) el.scrollTo({ top: 0, left: 0, behavior: "instant" });
  r.scrollIntoView({ block: "start", inline: "nearest", behavior: "instant" });
  keep(r);
}

// The wasm host loads this module by a script tag (glue.js `loadAfterPaint`), which finds it here.
(globalThis.exact ??= {}).fragment = { arm, scroll, cancel, shows, forget, traverse };
