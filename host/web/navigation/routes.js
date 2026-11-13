import { viewChildren, unwrapListItem, viewParent } from "./lists.js";
import { fold, launched, onFold, launchLocation } from "./environment.js";
import { inertAncestor } from "./layers.js";
import { popoverObservation } from "./popover.js";
// @ref LLP 1038 D6/D7/D11 — host projection and the last committed URL.
const AGENT_ADMITTED = true;
// false in a production bake: host/web/build.mjs rewrites this line (LLP 1069.007 D2)
let last = null, refused = new WeakMap();
// The tabpanels each root last hid or showed (one it stops naming shows again); the keys of the stack it last showed.
let managed = new WeakMap(), shown = new WeakMap(), placed = new WeakMap();
let written = [], gone = new Set(), cursor = 0, first = null, originIndex = null, echo = null, pop = null, draining = false;
const queue = [], waiters = new Set();
let root, navigate, log, hostBack, hostGo, routeMatches = () => false;
const routesIn = (node) => viewChildren(node).filter((r) => r.hasAttribute("navigationKey") && !r.hasAttribute("data-exiting"));
const setInert = (el, on) => {
  if (el.inert !== on) {
    el.inert = on;
  }
};
export const liveById = (id, root = document) => {
  const el = document.getElementById(id), live = (n) => !n.closest("[data-exiting]");
  return !el ? null : live(el) && root.contains(el) ? el : [...root.querySelectorAll("[id]")].find((n) => n.id === id && live(n)) ?? null;
};
// a command by id means the live element: one leaving keeps its ids for its own references
const leavingSheets = new Set(), shownRoutes = new WeakMap();
export function sheetLeaves(el) {
  if (typeof matchMedia !== "function" || typeof HTMLTextAreaElement !== "function" || el.nodeType !== 1 || el.getAttribute("navigationPresentation") !== "modal" || el.hasAttribute("data-exiting") || !el.isConnected || el.querySelector("audio, video, dialog[open], :popover-open") || "createValueRange" in HTMLTextAreaElement.prototype && el.querySelector("textarea[data-decorations]") || getComputedStyle(el).position !== "fixed" || matchMedia("(prefers-reduced-motion: reduce)").matches) {
    return false;
  }
  if (el.parentElement?.hasAttribute("data-exact-list-item")) {
    unwrapListItem(el.parentElement);
  }
  if (el.contains(document.activeElement)) {
    document.activeElement.blur();
  }
  el.setAttribute("navigationKey", "");
  el.setAttribute("data-exiting", "");
  el.setAttribute("aria-hidden", "true");
  el.inert = true;
  const now = getComputedStyle(el), from = now.translate === "none" ? "0 0" : now.translate, shadow = now.boxShadow;
  Object.assign(el.style, {
    transition: "none",
    pointerEvents: "none"
  });
  const slide = el.animate([{
    translate: from,
    boxShadow: shadow
  }, {
    translate: "0 100vh",
    boxShadow: shadow.replace(/rgba?\([^)]*\)|color\([^)]*\)|#[\da-f]+\b/gi, "transparent")
  }], {
    duration: 400,
    easing: "cubic-bezier(.32, .72, 0, 1)"
  }), done = () => {
    leavingSheets.delete(slide);
    el.remove();
  };
  leavingSheets.add(slide);
  slide.finished.then(done, done);
  return true;
}
// @ref LLP 1115.000 D4.8 — a sheet the look draws (fixed to the window) slides back down as it closes, as it slid up (neutral.css), unless the person asks for less motion. A runtime asks this before it removes a modal route (glue.js's `destroy`; rt.js's `clear`, `each` and presence's `Leave`); a sheet that leaves stays whole until it is off the window, from where it stands, out of input, accessibility and the routes as presence-glue.js's leaving views are (LLP 1063), the focus leaving it as a removed one's would (input-glue.js's `arrive` takes one a blur does not reach). One holding media or a field whose decorations the browser paints closes at once, as its runtime empties those with the commit, and so does one holding an open dialog or popover, which the top layer would leave frozen on the window; a native view slides, showing what its module left in it. It keeps its ids for what it draws, though a command by id passes over it (`liveById`), and its stack's projection ends it on showing a sheet that was not beneath it, which may share them (compared as elements: a replace keeps its key).
// @ref LLP 1075.003 §3.7 — tabs name their kept panels with aria-controls; each panel holds a route stack.
// Attribute and parent walks also work in the server renderer's DOM, which has no selector engine.
const tabsOf = (nav) => {
  const all = [...nav?.getElementsByTagName("*") ?? []];
  const owns = (el) => {
    for (let p = el.parentElement; p; p = p.parentElement) {
      if (p.hasAttribute("navigationKey")) {
        return p === nav;
      }
    }
    return false;
  };
  for (const list of all.filter((el) => el.getAttribute("role") === "tablist" && owns(el))) {
    const panels = viewChildren(list).filter((tab) => tab.getAttribute("role") === "tab" && tab.hasAttribute("aria-controls")).map((tab) => all.find((panel) => panel.getAttribute("id") === tab.getAttribute("aria-controls"))).filter((panel) => panel?.getAttribute("role") === "tabpanel" && owns(panel));
    if (panels.length) {
      return [list, panels];
    }
  }
  return [null, []];
};
export const panelsOf = (nav) => tabsOf(nav)[1];
const stacksOf = (nav) => {
  const panels = panelsOf(nav);
  return panels.length ? panels.map(routesIn) : [nav ? routesIn(nav) : []];
};
// @ref LLP 1115.002 D4, LLP 1116.004 D1.2 — WIDE changes presentation; the list/outer-press rule stays shared.
const WIDE_WIDTH = 900, WIDE = `(min-width: ${WIDE_WIDTH}px)`, wide = () => {
  if (!globalThis.matchMedia?.(WIDE).matches) {
    return false;
  }
  const { cols, rects } = fold();
  return cols === 1 || rects.length > 0 && rects.every(([, , width]) => width >= WIDE_WIDTH);
};
// Base wide navigation on the space within each horizontal segment
let splitLists = new WeakMap(), rowOf = new WeakMap(), actionRows = new WeakMap(), visits = new Map();
const activationRequired = new WeakSet(), activated = new WeakSet();
const sheet = (route) => ["modal", "fullscreen"].includes(route?.getAttribute("navigationPresentation"));
// A sheet's frame (`sheetFrame`, below), which the projection writes once a host hands it over: the wasm host's glue
// for every plan, the JS target's compiler only for a plan that can present a sheet (LLP 1047.002 D1). A plan that
// cannot has no route in a sheet, so there is nothing to write.
let framing = null;
export const useSheets = (frame) => {
  framing = frame;
};
// @ref LLP 1115.000 D4.8 (amended 2026-11-12) — a narrow sheet opens at the height iOS opens it at: the detent
// `navigationInitialDetent` names, else the first, among the detents UIKit keeps (ModalIOS.swift `keptDetents`: a
// word it cannot read, and one out of order, are dropped). neutral.css sizes it from `data-exact-detent` and
// `--exact-sheet-detent`: `medium` is half the tallest sheet and a percentage that share, as UIKit's are, a point
// height that many pixels, `fit-content` the route's content; `large`, or no detent, is the tallest. The detent is
// chosen as the sheet is presented and when its detents change, as UIKit selects one, so a change to the initial
// detent alone does not move an open sheet; it is written again at every projection, since the wasm target's
// style op replaces a route's whole inline style.
const sheetDetents = new WeakMap(), TALLEST = "(100% - env(safe-area-inset-top, 0px) - 12px - env(safe-area-inset-bottom, 0px))";
function keptDetents(said) {
  const kept = [];
  for (const word of said.split(" ").filter(Boolean)) {
    const share = word.endsWith("%") ? Number(word.slice(0, -1)) : NaN, points = Number(word);
    const [kind, height] = word === "large" ? ["large", 100] : word === "medium" ? ["share", 50] : word === "fit-content" ? ["content", 0]
      : word.endsWith("%") ? share > 0 && share <= 100 ? share === 100 ? ["large", 100] : ["share", share] : []
      : Number.isFinite(points) && points > 0 ? ["points", points] : [];
    const last = kept.at(-1);
    if (kind && !(last && (last.kind === "large" || kind !== "large" && (last.kind !== kind || height <= last.height)))) {
      kept.push({ word, kind, height });
    }
  }
  return kept;
}
function selectedDetent(route) {
  const said = route.getAttribute("data-navigationdetent") ?? "";
  let chosen = sheetDetents.get(route);
  if (chosen?.said !== said) {
    const kept = keptDetents(said), initial = route.getAttribute("data-navigationinitialdetent");
    const { kind = "large", height = 100, word } = kept.find((d) => d.word === initial) ?? kept[0] ?? {};
    const css = kind === "share" ? `calc(${TALLEST} * ${word === "medium" ? ".5" : height / 100})` : kind === "points" ? `${height}px` : "";
    sheetDetents.set(route, chosen = { said, kind: kind === "content" ? "fit-content" : css ? "height" : null, css });
  }
  return chosen;
}
/** The frame of `routes[index]`: its own detent, or its sheet's when it was pushed inside one; written only where it
 * changed. `shownIn` is the sheet each route shows in (`sheetAt`), `inUse` the sheets something shown is in. */
export function sheetFrame(routes, index, shownIn, inUse) {
  const route = routes[index], at = shownIn[index], pushed = at >= 0 && at !== index;
  // A sheet nothing shown is in forgets its detent, so its next presentation chooses it again.
  if (at === index && !inUse.has(index)) {
    sheetDetents.delete(route);
  }
  const chosen = inUse.has(at) ? selectedDetent(routes[at]) : null;
  if (route.hasAttribute("data-exact-in-sheet") !== pushed) {
    route.toggleAttribute("data-exact-in-sheet", pushed);
  }
  if ((route.getAttribute("data-exact-detent") ?? null) !== (chosen?.kind ?? null)) {
    chosen?.kind ? route.setAttribute("data-exact-detent", chosen.kind) : route.removeAttribute("data-exact-detent");
  }
  if (route.style.getPropertyValue("--exact-sheet-detent") !== (chosen?.css ?? "")) {
    route.style.setProperty("--exact-sheet-detent", chosen?.css ?? "");
  }
}
// The route a route shows in as a sheet: itself, or the sheet it was pushed over, since a push inside a sheet stays in
// it at its height, as a UINavigationController's in a presented sheet does; -1 for none (a full-screen route or the
// stack's root ends the search).
const sheetAt = (routes, i) => {
  for (let j = i; j > 0; j--) {
    const p = routes[j].getAttribute("navigationPresentation");
    if (p === "modal" || p === "fullscreen") {
      return p === "modal" ? j : -1;
    }
  }
  return -1;
};
export const overPage = (route) => {
  const p = route?.getAttribute("navigationPresentation");
  if (p !== "fullscreen") {
    return p === "modal";
  }
  const { backgroundColor: c = "rgba(1, 2, 3, 0)", backgroundImage: i = "none" } = getComputedStyle(route);
  return c !== "rgba(1, 2, 3, 0)" || i !== "none";
};
// A route that shows the page it covers: a sheet, or a full-screen route whose author wrote its background, which composites over that page as CSS has it (Charlie, 2026-11-10). neutral.css gives a full-screen route a clear colour no author writes, so its computed colour tells whether the author wrote one, `transparent` included; a server render computes none (render.mjs). One that paints nothing hides the page, on the root's ground.
let pressedRow = null, pendingRow = null;
const excluded = (el) => el.hasAttribute("popover") || el.hasAttribute("data-exiting") || getComputedStyle(el).display === "none";
const rowPress = (el) => el.getAttribute("data-exact-router-press");
const rowHref = (el) => el.getAttribute("href") || null;
const linked = (href) => href?.startsWith("/") && !href.startsWith("//") && routeMatches(href);
/** Outermost press owners; unlike candidate discovery, this walk crosses a navigationKey. */
function listRows(list) {
  const rows = [], walk = (parent) => {
    for (const el of parent.children) {
      if (excluded(el)) {
        continue;
      }
      const press = rowPress(el), href = rowHref(el);
      if (press !== null || href) {
        rows.push({
          el,
          press,
          href
        });
      } else {
        walk(el);
      }
    }
  };
  walk(list);
  return rows;
}
/** The root route's pushing list: the first `data-exact-list` in document order reachable from the route (no `navigationKey`,
* popover, leaving or undisplayed element on the way, itself included), not in a list taken (a sideways list is none) nor sideways,
* whose rows push; else `retained` with no rows. From the lists (`querySelectorAll`, the light DOM `children` walked), each predicate read
* once an element a search: the walk's answer for a DOM the search leaves alone (`listRows`, `linked` don't change it); one taken out meanwhile is skipped. (Exported for its test.) */
export function pushingList(route, retained = splitLists.get(route)) {
  const open = new Map(), across = new Map(), memo = (m, el, f) => {
    let v = m.get(el);
    if (v === undefined) {
      m.set(el, v = f(el));
    }
    return v;
  };
  // this search's reads only
  const passable = (el) => memo(open, el, (e) => !e.hasAttribute("navigationKey") && !excluded(e));
  const sideways = (el) => memo(across, el, (e) => {
    const s = getComputedStyle(e);
    return /flex/.test(s.display) && s.flexDirection.startsWith("row") || Number(e.getAttribute("data-exact-estimated-item-width")) > 0;
  });
  let empty = null;
  candidates: for (const el of route.querySelectorAll("[data-exact-list]")) {
    if (!passable(el)) {
      continue;
    }
    for (let up = el.parentElement; up !== route; up = up.parentElement) {
      if (!up || !passable(up) || up.hasAttribute("data-exact-list") && !sideways(up)) {
        continue candidates;
      }
    }
    // (!up: taken out; an admitted list owns its row scan, nested lists included)
    if (sideways(el)) {
      continue;
    }
    const rows = listRows(el), found = {
      list: el,
      rows
    };
    if (rows.some((r) => r.press === "true" || r.press === null && linked(r.href))) {
      return found;
    }
    if (el === retained && !rows.length) {
      empty = found;
    }
  }
  return empty;
}
/** Recheck authored admission even while narrow. Projection's visibility/inert never opt a list out. */
export function splitOf([route]) {
  // A rendered document has route and handler facts but no browser layout.
  if (typeof globalThis.getComputedStyle !== "function" || typeof globalThis.matchMedia !== "function") {
    if (route) {
      splitLists.delete(route);
    }
    return {
      width: 0,
      list: null,
      rows: []
    };
  }
  const said = route?.getAttribute("navigationSplit"), explicit = Number(said);
  const found = route && said !== "none" && !sheet(route) ? pushingList(route) : null;
  const width = route && said !== "none" && !sheet(route) && wide() && /^(absolute|fixed)$/.test(getComputedStyle(route).position) ? Number.isFinite(explicit) && explicit > 0 ? explicit : found ? 320 : 0 : 0;
  if (route) {
    if (found && (width || splitLists.has(route))) {
      splitLists.set(route, found.list);
    } else {
      splitLists.delete(route);
    }
  }
  return {
    width,
    list: found?.list,
    rows: found?.rows ?? []
  };
}
function captureRow(event) {
  pressedRow = null;
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
    return;
  }
  const route = event.target.closest?.("[data-exact-split=\"list\"]"), list = route && splitLists.get(route);
  if (!list || !root.contains(route)) {
    return;
  }
  let owner = event.target;
  while (owner && owner !== list && rowPress(owner) === null && !rowHref(owner)) {
    owner = owner.parentElement;
  }
  if (owner?.matches("a[href]") && (owner.target && owner.target !== "_self" || owner.hasAttribute("download"))) {
    return;
  }
  if (listRows(list).some((r) => r.el === owner && r.press === "true")) {
    const candidate = pressedRow = {
      row: owner,
      list,
      route,
      from: last?.url,
      event
    };
    // Qualification below reads the dispatch itself. This timer only releases the
    // retained Event after the dispatch; it does not extend row ownership.
    setTimeout(() => {
      if (pressedRow === candidate) {
        pressedRow = null;
      }
    }, 0);
  }
}
/** Href rows restore from retained visits. Action-only rows remember a router change made inside a normal click dispatch. */
function detailRows(routes, found) {
  for (const route of routes.slice(1)) {
    const id = route.getAttribute("navigationKey"), url = visits.get(id);
    let action = actionRows.get(route);
    if (pendingRow?.id === id && pendingRow.url === url) {
      action = pendingRow;
    }
    const valid = action && action.route === routes[0] && action.list === found.list && action.url === url && found.rows.some((r) => r.el === action.row && r.press === "true");
    if (valid) {
      actionRows.set(route, action);
    } else {
      actionRows.delete(route);
    }
    const link = found.rows.find((r) => r.press === null && linked(r.href) && r.href === url)?.el;
    const row = link ?? (valid ? action.row : null);
    if (row) {
      rowOf.set(route, row);
    } else {
      rowOf.delete(route);
    }
  }
}
// @ref LLP 1116.005 D3 — a route `replace` gives another path (a split's detail, another row's) keeps its id, so its node: it opens at its top, as a pushed one does; a new query keeps its place.
const paths = new WeakMap(), freshPath = (route) => {
  const path = last?.url?.split(/[?#]/)[0], was = paths.get(route);
  paths.set(route, path);
  if (was !== undefined && was !== path) {
    for (const el of [route, ...route.querySelectorAll("*")]) {
      if (el.scrollTop || el.scrollLeft) {
        el.scrollTo(0, 0);
      }
    }
    if (route.matches("[data-scrolldocument=\"true\"]:not([data-exact-offpage])") || route.querySelector("[data-scrolldocument=\"true\"]:not([data-exact-offpage])")) {
      scrollTo(0, 0);
    }
  }
};
/** Whether a root's tablist is a bar whose placement the app left unsaid, read while the root is not yet a sidebar. */
const unsaidBar = (nav, list) => viewParent(list) === nav && getComputedStyle(list).appearance !== "none" && list.getAttribute("aria-orientation") !== "vertical" && (([n, l]) => /flex/.test(n.display) && n.flexDirection.startsWith("column") && /^(static|relative)$/.test(l.position) && l.flexDirection.startsWith("row"))([getComputedStyle(nav), getComputedStyle(list)]);
/** Routes and panels excluded from input or document fields. Only input keeps a visible split list live. */
function unselectedWithin(nav, keepList) {
  const key = nav.getAttribute("navigationKey"), panels = panelsOf(nav), stacks = panels.length ? panels.map(routesIn) : [routesIn(nav)];
  const at = stacks.findIndex((routes) => routes.some((r) => r.getAttribute("navigationKey") === key)), routes = stacks[at] ?? [], top = routes.find((r) => r.getAttribute("navigationKey") === key);
  const beside = keepList && routes[0]?.getAttribute("data-exact-split") === "list" && !sheet(top) ? routes[0] : top;
  return at < 0 ? [] : [...panels.filter((_, i) => i !== at), ...routes.filter((r) => r !== top && r !== beside)];
}
export function unselected(nav) {
  return unselectedWithin(nav, true);
}
export function unselectedForHead(nav) {
  return unselectedWithin(nav, false);
}
export function routeFlags(root) {
  const off = new Set([...root.querySelectorAll("[navigationBack]")].flatMap((nav) => unselected(nav))), seen = new Map();
  const at = (el) => {
    if (!el || el === root) {
      return 0;
    }
    let v = seen.get(el);
    if (v === undefined) {
      seen.set(el, v = Math.max(off.has(el) ? el.style.visibility === "hidden" ? 2 : 1 : 0, at(el.parentElement)));
    }
    return v;
  };
  return at;
}
// LLP 1153 D8 — both runtimes' agent `tree` flags an element as the projection left the page, for one read: 1 (`inactive`) under a route or panel `unselected` names, which a split's list beside its detail is not; 2 (`hidden` too) where the projection hid that route, every one but a page a sheet covers
const routesOf = (nav) => stacksOf(nav).find((routes) => routes.some((r) => r.getAttribute("navigationKey") === nav.getAttribute("navigationKey"))) ?? [];
const selectedRoute = (nav) => routesOf(nav).find((r) => r.getAttribute("navigationKey") === nav.getAttribute("navigationKey"));
const browserIndex = () => globalThis.navigation?.currentEntry?.index ?? null;
// Agent launch facts belong to the carrier, not the router's typed URL. Keep
// them on every History entry so a browser reload retains its agent adapter.
const agentParameters = [
  "agent",
  "seed",
  "locale",
  "formatLocale",
  "timeZone",
  "epoch",
  "storage",
  "failFetch"
];
export function historyURL(path) {
  // (exported for its test) the document's URL with the route's path, query and fragment: History refuses a URL whose userinfo differs (HTML, "can have its URL rewritten")
  // "/." keeps a leading "//" a path, as the router has it (route/src/location.rs): the URL parser drops the dot segment, where alone it would read "//b" as a host
  const facts = AGENT_ADMITTED ? launched() : null, url = new URL(location.href), route = new URL(path.startsWith("/") ? "/." + path : path, url);
  url.pathname = route.pathname;
  url.search = route.search;
  url.hash = route.hash;
  if (facts?.has("agent")) {
    for (const key of agentParameters) {
      if (facts.has(key)) {
        url.searchParams.set(key, facts.get(key));
      }
    }
  }
  return url.href;
}
/** Whether `e` scrolls the page by the person: a wheel, a touch's move, a key that scrolls, or a press on a scrollbar. A
* `scroll` event cannot say: the page's own scrolls (a fragment's, scroll anchoring, a clamp) are trusted events too. */
const byPerson = (e, el = e.target) => e.type === "keydown" ? /^(Arrow(Up|Down|Left|Right)|Page(Up|Down)|Home|End| )$/.test(e.key) && !el.closest?.("input, textarea, select, [contenteditable]") : e.type !== "pointerdown" || el instanceof Element && ((b) => e.clientX - b.left >= el.clientLeft + el.clientWidth || e.clientY - b.top >= el.clientTop + el.clientHeight)(el.getBoundingClientRect());
const stamp = (index, op) => ({
  exact: index,
  id: op.top,
  url: op.url
});
/** A route's Back control, the selected route's unless named (1035.001 D1: the `id` the root's `navigationBack` names), pressable or not: the route's own, else one in no route at all (a header the routes share), as iOS resolves it (NavigationBack.control); another route's is that route's. */
const backControl = (nav, route = selectedRoute(nav), name = nav.getAttribute("navigationBack")) => route && name !== null ? route.querySelector(`[id="${CSS.escape(name)}"]`) ?? [...nav.ownerDocument.querySelectorAll(`[id="${CSS.escape(name)}"]`)].find((control) => [null, nav].includes(control.closest("[navigationKey]"))) ?? null : undefined;
// LLP 1116.000 D4: Back belongs to the header shadow root; bar items proxy the authored controls.
const BACK = `<style>button{all:unset;display:inline-flex;align-items:center;gap:6px;flex:none;vertical-align:middle;margin-inline:-6px 6px;padding:4px 8px 4px 4px;border-radius:8px;cursor:pointer;white-space:nowrap;-webkit-user-select:none;user-select:none}button[hidden],span[hidden]{display:none}button:focus-visible{outline:2px solid var(--exact-accent,AccentColor);outline-offset:1px}button:active>*{opacity:var(--exact-back-press,.5)}svg{width:1em;height:1em;fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}button:dir(rtl) svg{scale:-1 1}</style><button part="back" type="button"><svg viewBox="4 4 16 16" aria-hidden="true"><path d="M19 12H5m6-6-6 6 6 6"/></svg><span></span></button><slot></slot>`;
const headerOf = (route) => viewChildren(route).find((child) => child.localName === "header");
const authored = "[data-exact-said~=\"appearance\"]";
// a header or a node in one whose author wrote `appearance: none` (element.rs)
// @ref LLP 1153 D8 — a route's header holding a `nav` anywhere (a popover's too), or more than one link outside a `popover` or `dialog` in it (a menu's links are its button's, as an app bar's are), is a site's, marked `data-exact-site`: drawn as written (index.html, neutral.css), as one whose author wrote `appearance: none` is, it is no app bar and holds no Back. document_bars.rs `site_header` is the Rust renderer's rule.
const siteHeader = (header, links = 0) => {
  const holds = (el, menu) => [...el.children].some((child) => {
    const inMenu = menu || child.matches("[popover], dialog");
    return child.localName === "nav" || !inMenu && child.matches("a[href], [role=link]") && ++links > 1 || holds(child, inMenu);
  });
  return holds(header, false);
}, markSites = (route) => {
  for (const child of viewChildren(route)) {
    if (child.localName === "header") {
      child.toggleAttribute("data-exact-site", siteHeader(child));
    }
  }
}, asWritten = (header) => header.matches(`${authored}, [data-exact-site]`);
// The items of the bar iOS makes of a shown `header`, or null where it makes none: kernel/src/bars.rs `header_bar`,
// ported to the elements both runtimes write alike, since the JS target keeps no kernel to ask. `walk` is `Walk::walk`:
// a bar's header holds one heading, and an item is a pressable it reaches, not the title's, or a disabled button. A header drawn as written is no bar.
function barItems(route, header) {
  if (route.getAttribute("data-navigationbar") === "none" || viewChildren(route)[0] !== header || asWritten(header)) {
    return null;
  }
  const heading = "[data-exact-text][role=heading]", press = "[data-exact-on~=\"press\"]", layer = (el) => el.matches("[popover], dialog");
  const holds = (el) => [...el.children].some((child) => !layer(child) && (child.matches(heading) || holds(child)));
  const disabled = (el) => el.matches("button[disabled]") && !el.hasAttribute("role") && root?.getAttribute("aria-busy") !== "true";
  // `fact::DISABLED`, dimmed whether or not it presses; a `button` element carries no `role` unless it has another one (element.rs); until input is ready, the glue disables every control itself (glue.js)
  let headings = 0, tap = null;
  const items = [], walk = (node) => {
    for (const child of node.children) {
      if (layer(child) || child.matches(`[role=tablist], input[type="search"], ${authored}`)) {
        continue;
      }
      // a layer, the bar's segments, its search field, or what its author wrote `appearance: none` on (iOS projects none of it): no item
      if (child.matches(heading)) {
        headings++;
      } else if (child.matches(press) || opensPopover(child) || disabled(child) && !holds(child)) {
        if (!tap && child.matches(press) && holds(child)) {
          walk(tap = child);
        } else {
          items.push(child);
        }
      } else if (!child.matches("[role=status]") || holds(child)) {
        walk(child);
      }
    }
  };
  walk(header);
  return headings === 1 ? items : null;
}
/** Mark every route's bar and site header on a page rendered ahead of time (host/web-js/render.mjs), which runs no projection, as document.rs marks them. */
export function markBars(node) {
  for (const child of node.children) {
    if (child.hasAttribute("navigationKey")) {
      markSites(child);
    }
    const header = child.hasAttribute("navigationKey") && !child.hasAttribute("navigationBack") && headerOf(child);
    if (header) {
      header.toggleAttribute("data-exact-bar", !!barItems(child, header));
    }
    markBars(child);
  }
}
// Whether a button shows or toggles a popover, a header's menu (kernel/src/invocation.rs `opens_popover`).
function opensPopover(el) {
  if (el.localName !== "button") {
    return false;
  }
  const commanded = document.getElementById(el.getAttribute("commandfor") || "");
  if (commanded) {
    return commanded.hasAttribute("popover") && /^(show|toggle)-popover$/i.test(el.getAttribute("command") ?? "");
  }
  return !!document.getElementById(el.getAttribute("popovertarget") || "")?.hasAttribute("popover") && !/^hide$/i.test(el.getAttribute("popovertargetaction") ?? "");
}
// LLP 1115.000 D4.1: title precedence is authored head, visible route heading, then app name.
const titleOf = (route) => headerOf(route)?.querySelector("h1, h2, h3, h4, h5, h6, [role=heading]")?.textContent.trim() ?? "";
let saidTitle = null, appName = null;
export function pageTitle(said = saidTitle) {
  saidTitle = said;
  appName ??= document.querySelector("meta[property=\"og:site_name\"]")?.content ?? document.title;
  const nav = root?.querySelector("[navigationBack]"), title = said ?? (nav && titleOf(selectedRoute(nav)) || appName);
  if (document.title !== title) {
    document.title = title;
  }
}
function appBarBack(nav, routes, index, split, shown) {
  const route = routes[index], header = headerOf(route);
  if (!header) {
    return;
  }
  const headerShown = getComputedStyle(header).display !== "none";
  // In a split, a detail its list's row pushed has none: the list beside it is its way back (LLP 1116.004 D1). Nor has a route whose shown header is drawn as written, wherever it stands in the route (LLP 1153 D8): its author wrote `appearance: none`, keeping the bar they drew (HeaderRule.swift `keepsAuthoredHeader`), or it is a site's.
  const pushed = index > 0 && !sheet(route) && route.getAttribute("data-navigationbar") !== "none" && !(split && (index === 1 || rowOf.has(route))) && !(asWritten(header) && headerShown);
  // A header iOS makes no bar of draws Back when it does not show a visible Back control (iOS draws it either way).
  // Discount the bar's own hiding mark. In a header not shown, the Back stays hidden with it.
  const control = pushed ? backControl(nav, route) : null, inShownHeader = headerShown && !!control && header.contains(control) && !header.contains(control.closest("[popover], dialog"));
  const items = headerShown && (inShownHeader || shown) ? barItems(route, header) : null;
  const held = inShownHeader && !!items?.includes(control);
  for (const el of route.querySelectorAll("[data-exact-bar-back]")) {
    if (el !== control || !held) {
      el.removeAttribute("data-exact-bar-back");
    }
  }
  const drawn = pushed && !(inShownHeader && header.contains(control.closest(authored))) && (!inShownHeader || !!items || !control.checkVisibility({ visibilityProperty: true }));
  // a control its author wrote `appearance: none` on, or an item holding it, is their Back, with no bar's beside it (iOS's `authoredBack`)
  if (shown) {
    header.toggleAttribute("data-exact-bar", !!items);
  }
  // a shown bar, whose symbols are the bar's size (neutral.css)
  let button = header.shadowRoot?.querySelector("button[part=\"back\"]");
  if (drawn && !button && !header.shadowRoot && header.attachShadow) {
    // not a shadow root of someone else's
    header.attachShadow({ mode: "open" }).innerHTML = BACK;
    button = header.shadowRoot.querySelector("button");
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      appBack(viewParent(header));
    });
  }
  for (const el of [...route.querySelectorAll("[data-exact-bar-back]"), control]) {
    el?.toggleAttribute("data-exact-bar-back", el === control && held && !!button);
  }
  // A control the author disabled, or made inert in its route, leaves no Back, as `pressBack` refuses it. The glue's own
  // disabling until the page is interactive (`aria-busy`) and the inert it gives a route a sheet covers are not theirs.
  if (button) {
    button.hidden = !drawn || control?.matches(":disabled") && !nav.closest("[aria-busy=\"true\"]") || route.contains(inertAncestor(control)?.parentElement ?? null);
  }
  if (!button || button.hidden) {
    return;
  }
  const words = control ? control.textContent.trim() : titleOf(routes[index - 1]), english = /^en\b/i.test(document.documentElement.lang || "en");
  const label = control ? words.length <= 14 ? words : english ? "Back" : "" : english ? "Back" : "", name = control ? control.getAttribute("aria-label") || words || "Back" : !english ? words || "Back" : words ? `Back to ${words}` : "Back";
  button.lastChild.hidden = !label;
  if (button.lastChild.textContent !== label) {
    button.lastChild.textContent = label;
  }
  if (button.getAttribute("aria-label") !== name) {
    button.setAttribute("aria-label", name);
  }
  if (control?.hasAttribute("aria-keyshortcuts") && !control.checkVisibility({ visibilityProperty: true })) {
    button.setAttribute("aria-keyshortcuts", control.getAttribute("aria-keyshortcuts"));
  } else {
    button.removeAttribute("aria-keyshortcuts");
  }
}
/** The app bar's Back on `route`, pressed: the browser's Back from its route (`popped`), applied at once: the route's control
* pressed, else the root's `navigate` with the location beneath, else the runner's own `back` (LLP 1115 D5). The
* history follows the commit as it follows any Back the app takes (`commit`). */
function appBack(route) {
  const nav = route?.parentElement?.closest("[navigationBack]");
  if (!nav || route !== selectedRoute(nav)) {
    return;
  }
  const routes = routesOf(nav), below = routes[routes.indexOf(route) - 1], id = Number(route.getAttribute("navigationKey"));
  if (!below) {
    return;
  }
  if (backControl(nav)) {
    const why = pressBack(nav);
    if (why) {
      log(`app bar: Back refused: ${why}`);
    }
    return;
  }
  const entry = written[cursor - 1], known = cursor > first && written[cursor]?.id === id && entry?.id === Number(below.getAttribute("navigationKey"));
  if (!known || navigate(entry.url.split("#")[0]) === false) {
    hostBack?.(id);
  }
  // without its fragment, which would scroll it again (LLP 1153 D5)
}
const closedNone = (route) => ["modal", "fullscreen"].includes(route?.getAttribute("navigationPresentation")) && route.getAttribute("closedby") === "none";
/** Press the Back control wherever it is, shown or not, as UIKit's Back presses it (NavigationBack.swift `control`),
* unless it is disabled, or inert (the web's choice: iOS's Back reads no inert); else why it was not pressed. */
function pressBack(nav) {
  const route = selectedRoute(nav), control = backControl(nav);
  if (!route) {
    return "no route is selected";
  }
  if (closedNone(route)) {
    return `route ${route.getAttribute("navigationKey")} is closedby="none"`;
  }
  if (!control) {
    return `route ${route.getAttribute("navigationKey")} has no id="${nav.getAttribute("navigationBack")}" control`;
  }
  if (control.matches(":disabled") || inertAncestor(control)) {
    return `its id="${control.id}" control is disabled or inert`;
  }
  control.click();
}
function go(to, from, finish = () => {}) {
  const index = browserIndex();
  const current = index !== null && originIndex !== null ? index - originIndex : from;
  if (to === current) {
    finish();
    return;
  }
  echo = {
    index: to,
    finish
  };
  history.go(to - current);
}
function commit(op) {
  for (const id of op.removed) {
    gone.add(id);
  }
  if (first === null) {
    first = 0;
    originIndex = browserIndex();
    written[0] = stamp(0, op);
    history.replaceState(written[0], "", historyURL(op.url));
  } else if (written[cursor]?.id === op.top && !(pop === null && written[cursor].url !== op.url && written[cursor].url.split("#")[0] === op.url.split("#")[0])) {
    // another fragment on the visit shown, or none, is one more browser entry of it, as a same-document link's is (LLP 1153 D5)
    if (written[cursor].url !== op.url) {
      written[cursor] = stamp(cursor, op);
      history.replaceState(written[cursor], "", historyURL(op.url));
    }
  } else {
    let j = cursor;
    while (j > first && gone.has(written[j]?.id)) {
      j--;
      if (written[j]?.id === op.top) {
        const from = cursor;
        cursor = j;
        // The entry gone back to takes the top's url when that changed in the same commit (`replace(back(r), u)`).
        return go(j, from, () => written[j].url === op.url || history.replaceState(written[j] = stamp(j, op), "", historyURL(op.url)));
      }
    }
    for (const index of Object.keys(written)) {
      if (Number(index) > cursor) {
        delete written[index];
      }
    }
    written.length = Math.max(0, cursor + 1);
    op.pushed = written[cursor]?.id === op.top;
    written[++cursor] = stamp(cursor, op);
    // `pushed`: an entry of the visit shown, whose place fragment.js keeps
    history.pushState(written[cursor], "", historyURL(op.url));
    globalThis.exact?.fragment?.forget(cursor);
  }
}
function popped({ j, state, url, ua }) {
  const entry = written[j];
  const owned = entry && state?.exact === j && state.id === entry.id && state.url === entry.url;
  const target = owned ? entry.url : url;
  const nav = root.querySelector("[navigationBack]");
  const routes = routesOf(nav), selected = routes.indexOf(selectedRoute(nav));
  // A completed pop presses the selected route's Back control; a route with none still goes back by the root's `navigate`
  // with the entry's URL. Unheard, a Back one entry down is the platform's own, as iOS's (NavigationBack.way): the route's
  // control pressed (disabled, or closedby="none", refuses), else the runner's `back` from the visit the entry left (LLP 1115 D5), however routes are keyed. Any other entry unheard (Forward, or more than one away) is the runner's own `go` there (LLP 1115.000 D4.3). An entry given to `navigate` at the URL already shown, which this page did not write or which holds the visit shown, is accepted though nothing changed: a `replace` can give the only entry a reload wrote the URL of the one beneath it, and refusing would hold Back there. An earlier visit at that URL is still refused: accepting it would leave the visits after it on the stack.
  const step = owned && j === cursor - 1 && entry.id !== written[cursor]?.id, beneath = step && selected > 0 && routes[selected - 1].getAttribute("navigationKey") === String(entry.id);
  let back = beneath && !!backControl(nav), own = false, why = null;
  pop = { ua };
  try {
    if (back) {
      why = pressBack(nav);
    } else if (navigate(target) === false) {
      own = true;
      back = step && (!!backControl(nav) || closedNone(selectedRoute(nav)));
      if (back) {
        why = pressBack(nav);
      } else if (step) {
        hostBack?.(written[cursor].id);
      } else {
        hostGo?.(target);
      }
    }
    const accepted = back ? last?.top === entry.id : (pop.op ?? (!owned || entry.id === last?.top ? last : null))?.url === target;
    if (accepted) {
      cursor = j ?? cursor;
      first = Math.min(first, cursor);
      written[cursor] = stamp(cursor, last);
      go(cursor, j ?? cursor, () => history.replaceState(written[cursor], "", historyURL(written[cursor].url)));
    } else if (pop.op) {
      const op = pop.op;
      if (j !== null && j !== cursor) {
        go(cursor, j, () => commit(op));
      } else {
        history.replaceState(written[cursor], "", historyURL(written[cursor].url));
        commit(op);
      }
    } else {
      if (back) {
        log(`history: Back refused: ${why ?? `pressing the Back control did not select entry ${entry.id}`}; restoring the entry`);
      } else {
        log(`history: ${own ? `${step ? "Back to" : "navigate"} ${JSON.stringify(target)} refused: the navigation root has no navigate handler (navigate=…), and the runner's own ${step ? `back from visit ${written[cursor].id}` : "go there"} committed no router change` : `${beneath ? `Back to ${JSON.stringify(target)} refused: route ${nav.getAttribute("navigationKey")} has no id="${nav.getAttribute("navigationBack")}" control, and` : `navigate ${JSON.stringify(target)} refused:`} the navigation root's navigate handler (navigate=…) committed no router change, or the root has none`}; restoring the entry`);
      }
      if (j !== null && j !== cursor) {
        go(cursor, j);
      } else {
        history.replaceState(written[cursor], "", historyURL(written[cursor].url));
      }
    }
  } finally {
    pop = null;
  }
}
function drain() {
  if (draining || echo !== null) {
    return;
  }
  draining = true;
  try {
    for (let item; queue.length && echo === null;) {
      item = queue.shift();
      if (item.op) {
        commit(item.op);
      } else {
        popped(item);
      }
    }
  } finally {
    draining = false;
  }
}
const settled = () => {
  if (echo === null && !queue.length) {
    for (const finish of [...waiters]) {
      finish();
    }
  }
};
export const navigation = {
  personScrolls: 0,
  // the person's scrolls (`byPerson`) from the page's start: one after a navigation asked a fragment's scroll cancels it (fragment.js, LLP 1153 D5)
  /** The Back or Forward being dispatched (`popped`), else null: `ua` when the browser animated it (`hasUAVisualTransition`).
  * A commit made then mirrors its router at once, before a view transition may defer its tree update (router.js `traversal`). */
  get traversing() {
    return pop;
  },
  /** `dispatch(location)` is false when no `navigate` handler heard it; `back(id)` is the runner's own `back` from visit `id`, `goTo(location)` its own `go`. */
  connect(hostRoot, dispatch, journal, back = null, goTo = null, matchesRoute = () => false, waitForActivation = false) {
    root = hostRoot;
    navigate = dispatch;
    log = journal;
    hostBack = back;
    hostGo = goTo;
    routeMatches = matchesRoute;
    for (const type of [
      "wheel",
      "touchmove",
      "keydown",
      "pointerdown"
    ]) {
      addEventListener(type, (e) => {
        if (byPerson(e)) {
          navigation.personScrolls++;
        }
      }, {
        capture: true,
        passive: true
      });
    }
    if (waitForActivation) {
      activationRequired.add(hostRoot);
      activated.delete(hostRoot);
    } else {
      activationRequired.delete(hostRoot);
      activated.add(hostRoot);
    }
    // Capture before either target's link listener commits. Early replay before connect has no action-row association.
    document.addEventListener("click", captureRow, true);
    document.addEventListener("dragstart", (event) => {
      if (root.contains(event.target) && event.target.matches?.("img[data-symbol-source], :is(button, a[href], [data-exact-on~=\"press\"]) img")) {
        event.preventDefault();
      }
    });
    // @ref LLP 1115.000 D4.14 — symbols and images inside pressables do not start picture drags.
    globalThis.matchMedia?.(WIDE).addEventListener?.("change", () => navigation.project(root, log));
    onFold(() => navigation.project(root, log));
    addEventListener("popstate", (event) => {
      if (!last) {
        return;
      }
      const index = browserIndex();
      const j = index !== null && originIndex !== null ? index - originIndex : Number.isInteger(event.state?.exact) ? event.state.exact : null;
      if (echo !== null && j === echo.index) {
        const finish = echo.finish;
        echo = null;
        finish();
        drain();
        settled();
        return;
      }
      queue.push({
        j,
        state: event.state,
        url: launchLocation(),
        ua: event.hasUAVisualTransition
      });
      if (echo !== null) {
        const pending = echo;
        echo = null;
        go(pending.index, j ?? cursor, pending.finish);
      }
      drain();
      settled();
    });
    document.addEventListener("keydown", (event) => {
      // An input method's Escape cancels its composition.
      if (event.key !== "Escape" || event.defaultPrevented || event.isComposing || event.keyCode === 229) {
        return;
      }
      if (!root.contains(event.target) && event.target !== document.body && event.target !== document.documentElement) {
        return;
      }
      if (document.querySelector("dialog:modal") || [...document.querySelectorAll(":popover-open")].some((p) => p.popover === "auto" || p.popover === "hint")) {
        return;
      }
      // The innermost shown sheet's Back: its named control, else the app bar's way, as iOS's swipe down (`goBack`).
      // A route pushed inside a sheet goes Back to the route before it in the sheet, as Android's Back does there.
      for (const nav of [...root.querySelectorAll("[navigationBack]")].reverse()) {
        const route = selectedRoute(nav), routes = routesOf(nav), inSheet = route && sheetAt(routes, routes.indexOf(route)) >= 0;
        if (!(inSheet || ["modal", "fullscreen"].includes(route?.getAttribute("navigationPresentation"))) || inertAncestor(route) || route.checkVisibility?.({ visibilityProperty: true }) === false) {
          continue;
        }
        event.preventDefault();
        if (backControl(nav) || closedNone(route)) {
          pressBack(nav);
        } else {
          appBack(route);
        }
        return;
      }
    });
  },
  /** Enable projection after a target has adopted its served DOM, then make the first projection. */
  activate(hostRoot = root, journal = log) {
    activated.add(hostRoot);
    navigation.project(hostRoot, journal);
  },
  reset(op = null) {
    refused = new WeakMap();
    splitLists = new WeakMap();
    rowOf = new WeakMap();
    actionRows = new WeakMap();
    pressedRow = pendingRow = null;
    visits = new Map((op?.visits ?? []).map(([id, url]) => [String(id), url]));
    if (op && written[cursor]?.id === op.top) {
      return;
    }
    last = null;
    written = [];
    gone = new Set();
    cursor = 0;
    first = null;
    originIndex = null;
    echo = null;
    pop = null;
    queue.length = 0;
    for (const finish of [...waiters]) {
      finish();
    }
  },
  apply(op) {
    visits = new Map((op.visits ?? []).map(([id, url]) => [String(id), url]));
    pendingRow = pressedRow && pressedRow.event.eventPhase !== 0 && op.url !== pressedRow.from ? {
      row: pressedRow.row,
      list: pressedRow.list,
      route: pressedRow.route,
      id: String(op.top),
      url: op.url
    } : null;
    pressedRow = null;
    last = op;
    if (pop) {
      pop.op = op;
      op.traversal = true;
      for (const id of op.removed) {
        gone.add(id);
      }
    } else {
      // a traversal's scroll puts back the visit's place (fragment.js)
      queue.push({ op });
      drain();
    }
  },
  // @ref LLP 1116.006 D4 — the agent's `back` from a route: the browser's Back where the entry beneath is the route beneath, else (a reload left none) the app bar's.
  back(hostRoot) {
    const nav = hostRoot?.querySelector("[navigationBack]"), routes = nav ? routesOf(nav) : [], route = nav && selectedRoute(nav), at = routes.indexOf(route), way = sheet(route) ? "sheet" : "route", key = nav?.getAttribute("navigationKey"), kept = () => nav.getAttribute("navigationKey") === key ? { refused: "the app kept the route: its Back control is disabled, its closedby is \"none\", or its navigate changed nothing (logs say which)" } : {};
    if (at < 1) {
      return { error: "back: nothing to go back from: no dialog is open, and no route is beneath the selected one" };
    }
    if (cursor > first && written[cursor]?.id === Number(route.getAttribute("navigationKey")) && written[cursor - 1]?.id === Number(routes[at - 1].getAttribute("navigationKey"))) {
      return navigation.travel(nav, -1).then((r) => ({
        ...r,
        back: way,
        ...kept()
      }));
    }
    appBack(route);
    return {
      back: way,
      delivery: "app-bar",
      ...kept()
    };
  },
  // @ref LLP 1038 D11 — the agent uses the real browser traversal.
  travel(nav, delta) {
    if (nav !== root.querySelector("[navigationBack]")) {
      return { error: "history target is not the navigation root" };
    }
    if (!Number.isInteger(delta) || delta === 0) {
      return {
        malformed: true,
        error: "history must be a nonzero integer"
      };
    }
    return new Promise((resolve) => {
      const timer = setTimeout(finish, 1e3);
      // Once the traversal's tree update has run, which a view transition defers (shared.js `afterCommit`).
      function finish() {
        clearTimeout(timer);
        waiters.delete(finish);
        (globalThis.exact?.afterCommit ?? ((run) => run()))(() => resolve({
          history: delta,
          delivery: "platform"
        }));
      }
      waiters.add(finish);
      history.go(delta);
    });
  },
  project(root, log) {
    if (activationRequired.has(root) && !activated.has(root)) {
      return;
    }
    for (const route of root.querySelectorAll("[navigationKey]")) {
      markSites(route);
    }
    // site headers, under a root with no `navigationBack` too
    for (const nav of root.querySelectorAll("[navigationBack]")) {
      let key = nav.getAttribute("navigationKey"), [list, panels] = tabsOf(nav);
      // The authored placement is read with the sidebar's own rules lifted, which restyles the page (an embedded frame re-centres), so only when what places the bar may have changed: the wide breakpoint, the bar's parent or orientation, or the root's or the bar's class or inline display, flex direction, position or appearance (their class rules and inline style are what place them).
      const isWide = wide(), said = [
        isWide,
        viewParent(list) === nav && list.getAttribute("aria-orientation"),
        ...[nav, list].map((el) => el && [
          el.className,
          el.style.display,
          el.style.flexDirection,
          el.style.position,
          el.style.appearance
        ].join("|"))
      ].join("/");
      if (placed.get(nav) !== said) {
        nav.removeAttribute("data-exact-sidebar");
        nav.toggleAttribute("data-exact-sidebar", isWide && unsaidBar(nav, list));
        placed.set(nav, said);
      }
      for (const panel of managed.get(nav) ?? []) {
        if (!panels.includes(panel)) {
          panel.style.visibility = "";
          setInert(panel, !!panel.authoredInert);
        }
      }
      managed.set(nav, panels);
      const stacks = panels.length ? panels.map(routesIn) : [routesIn(nav)];
      const declared = (k) => stacks.some((routes) => routes.some((route) => route.getAttribute("navigationKey") === k));
      if (!declared(key)) {
        if (refused.get(nav) !== key) {
          refused.set(nav, key);
          log(`navigationKey "${key}" matches no route among the root's children or those of the tabpanels its tablist names; the stack is unchanged`);
        }
        // @ref LLP 1035.001 D1 — left alone, less the routes that left the tree: the deepest of the stack last shown
        // still declared is selected, so a closed sheet or a popped screen never leaves the route beneath it inert.
        key = (shown.get(nav) ?? []).findLast(declared);
        if (key === undefined) {
          continue;
        }
      } else {
        refused.delete(nav);
      }
      const at = stacks.findIndex((routes) => routes.some((route) => route.getAttribute("navigationKey") === key));
      // Every tab but the selected one stays mounted, hidden and inert.
      for (const [index, panel] of panels.entries()) {
        const active = index === at;
        if (!active && panel.contains(document.activeElement)) {
          document.activeElement.blur();
        }
        panel.style.visibility = active ? "" : "hidden";
        setInert(panel, !active || !!panel.authoredInert);
      }
      for (const [stack, routes] of stacks.entries()) {
        // The selected stack shows the route the root names; another keeps its top laid out.
        const selected = stack === at ? routes.findIndex((route) => route.getAttribute("navigationKey") === key) : routes.length - 1;
        if (stack === at) {
          const before = shownRoutes.get(nav) ?? [], showing = routes.slice(0, selected + 1);
          shownRoutes.set(nav, showing);
          shown.set(nav, showing.map((route) => route.getAttribute("navigationKey")));
          if (sheet(routes[selected]) && !before.includes(routes[selected])) {
            for (const slide of leavingSheets) {
              if (viewParent(slide.effect.target) === viewParent(routes[selected])) {
                slide.finish();
                slide.effect.target.remove();
              }
            }
          }
        }
        // A route pushed inside a sheet is presented as that sheet is: what the sheet covers shows beneath it.
        const host = sheetAt(routes, selected), presented = host >= 0 ? host : selected;
        const found = splitOf(routes), split = found.width, live = split && !sheet(routes[presented]);
        const modal = overPage(routes[presented]), beneath = routes.findLastIndex((r, i) => i < presented && !overPage(r));
        detailRows(routes, found);
        if (stack === at && list) {
          setInert(list, sheet(routes[presented]) || !!list.authoredInert);
        }
        // a sheet is modal: the root's tab bar is inert beneath it, as the route it covers is (LLP 1115.000 D4.5)
        // A split's detail shown is the top, or the route a sheet covers; its row is the one whose press pushed it, or pushed a route beneath it in its column, else (a detail the app opened itself, at launch or after Back) the row the app says is current.
        const detail = split ? routes.findLastIndex((r, i) => i > 0 && i <= selected && !sheet(r) && sheetAt(routes, i) < 0) : -1;
        // The sheet each route shows in, and the sheets something shown is in.
        const shownIn = routes.map((r, i) => sheetAt(routes, i));
        const inUse = framing && new Set(shownIn.filter((s, i) => s >= 0 && (i === selected || live && i === 0 || modal && (i >= beneath && i < presented || split > 0 && i === 0))));
        for (const [index, route] of routes.entries()) {
          const active = index === selected || live && index === 0;
          if (!active && route.contains(document.activeElement)) {
            document.activeElement.blur();
          }
          const covered = modal && (index >= beneath && index < presented || split > 0 && index === 0);
          // a sheet shows what it sits over: the page (in the strip above a narrow sheet) and any sheets between, which only the top one dims
          route.style.visibility = active || covered ? "" : "hidden";
          route.toggleAttribute("data-exact-covered", covered);
          setInert(route, !active || !!route.authoredInert);
          const column = !split ? null : index === 0 ? "list" : sheet(route) || shownIn[index] >= 0 ? null : "detail";
          if (route.getAttribute("data-exact-split") !== column) {
            column ? route.setAttribute("data-exact-split", column) : route.removeAttribute("data-exact-split");
          }
          route.style.setProperty("--exact-split", column ? `${split}px` : "");
          framing?.(routes, index, shownIn, inUse);
          appBarBack(nav, routes, index, split > 0, active || covered);
          if (stack === at && index === selected) {
            freshPath(route);
          }
        }
        let row = null;
        for (let i = detail; i > 0 && !row; i--) {
          row = rowOf.get(routes[i])?.isConnected ? rowOf.get(routes[i]) : null;
        }
        if (!row && detail > 0) {
          row = found.rows.find((r) => ![
            "false",
            "",
            null
          ].includes(r.el.getAttribute("aria-current")))?.el ?? null;
        }
        for (const el of routes[0]?.querySelectorAll("[data-exact-split-row]") ?? []) {
          if (el !== row) {
            el.removeAttribute("data-exact-split-row");
          }
        }
        row?.toggleAttribute("data-exact-split-row", true);
        // @ref LLP 1116.006 D2 — a split showing no detail fills the detail pane, as an iPad's split view does: the platform's placeholder (the shell draws `data-exact-split-empty`), or the author's, a box of the root route marked `navigationSplit="placeholder"` (which the lowerer hides everywhere else), at the pane's width.
        const empty = split > 0 && detail < 0, own = routes[0]?.querySelector("[navigationSplit=\"placeholder\"]"), box = viewParent(routes[0]);
        own?.toggleAttribute("data-exact-split-placeholder", empty);
        box?.toggleAttribute("data-exact-split-empty", empty && !own);
        if (empty) {
          box.style.setProperty("--exact-split", `${split}px`);
          own && routes[0].style.setProperty("--exact-split-pane", `${(routes[0].offsetParent ?? box).clientWidth - split}px`);
        }
      }
    }
    pendingRow = null;
    // The page module's container hatches, when it has them (LLP 1075.003.000 §3.7); the wake lock, which follows what shows (LLP 1125 D11).
    globalThis.exact?.onProject?.(root);
    globalThis.exact?.wakeLockGlue?.projected(root);
    globalThis.exact?.orientationLockGlue?.projected(root);
    pageTitle();
  },
  observation(root) {
    const nav = root.querySelector("[navigationBack]");
    const routes = routesOf(nav);
    const key = nav?.getAttribute("navigationKey") ?? null;
    const index = routes.findIndex((r) => r.getAttribute("navigationKey") === key);
    const selected = index >= 0 ? routes[index] : null;
    return {
      url: last?.url ?? null,
      route: key,
      stack: index >= 0 ? routes.slice(0, index + 1).map((r) => r.getAttribute("navigationKey")) : [],
      presentation: ["modal", "fullscreen"].includes(selected?.getAttribute("navigationPresentation")) ? selected.getAttribute("navigationPresentation") : null,
      source: selected?.getAttribute("navigationSource") ?? null,
      closedby: selected?.getAttribute("closedby") ?? null,
      transition: {
        interactive: false,
        phase: "idle"
      },
      popover: popoverObservation(root)
    };
  }
};
export { queue, sheet, commit, root, log, settled, AGENT_ADMITTED, agentParameters, pop, last };
