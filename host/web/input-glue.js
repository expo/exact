import { focusStack, viewChildren, viewParent } from './navigation.js';
import { receive, takes } from './drag-in.js';
// Input-only glue: loaded after the baked first pixel, independently of data readiness.
import "./prompt-glue.js"; // a prompt's Return
import "./alert-message.js"; // a grouped alert's message box
// False in a production bake, which host/web/build.mjs and host/web-js/build.mjs write here (LLP 1069.007 D2): the
// agent's hold, its press readout and its file picker, which only the agent reads, then leave the piece.
const AGENT_ADMITTED = true;
// False in the JS target's runtime, which host/web-js/glue.mjs writes here (LLP 1047.002 D4.1): that runtime hears its
// own pointers and routes its own links, so the wasm host's pointer, mouse and link paths leave the piece.
const WASM_HOST = true;
// @ref LLP 1047.002 D4.2 — the behaviours that act only on what a plan makes: a pressed paint, a state look's edges, a
// pan, a tablist's and a menu's keys, a radio group's, and keyboard shortcuts. The JS target's build writes false each
// one its plan cannot make (input.flag), and the bundler drops its code. The hourly conformance check fails a page
// holding what a left-out behaviour acts on (conform-cut.mjs `LEFT_OUT`).
const PRESS_PAINT = true, STATE_LOOKS = true, PANS = true, ROVING = true, RADIO_GROUPS = true, SHORTCUTS = true;
const hearsPress = el => (el.exactHandlers ?? el.dataset?.exactOn?.split(" "))?.includes("press") === true;
/** An element whose `press` the keyboard reaches only through its tabindex:
 * not one the browser activates itself (the wasm host's handler list, or the
 * JS target's `data-exact-on`). */
const pressesByKey = el => !el.matches("button, a[href], input, select, textarea, summary")
  && hearsPress(el);
const shortcutKeys = new Set(["Enter", "Tab", "Escape", "Backspace", "Delete", "Insert", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown"]);
/** A shortcut's key when `e.key` is one character outside ASCII and the Latin script (ㅠ on KeyB under Korean 2-Set,
 * Russian, Greek): what its physical key (`code`) types on a US layout, with Shift, as web apps match (the macOS host
 * asks the Mac's ASCII-capable layout instead; the web has only `code`). A Latin character is the key wherever it is
 * (AZERTY, Dvorak, German's ö), so no chord fires twice; so are Option's characters (the macOS host's rule). */
const usCodes = /* @__PURE__ */ "Backquote Digit1 Digit2 Digit3 Digit4 Digit5 Digit6 Digit7 Digit8 Digit9 Digit0 Minus Equal BracketLeft BracketRight Backslash Semicolon Quote Comma Period Slash".split(" ");
const physicalKey = e => {
  if (e.altKey || typeof e.key !== "string" || [...e.key].length !== 1 || !/[^\x00-\x7f]/.test(e.key) || /\p{Script=Latin}/u.test(e.key)) return null;
  if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3).toLowerCase();
  const at = usCodes.indexOf(e.code);
  return at < 0 ? null : (e.shiftKey ? "~!@#$%^&*()_+{}|:\"<>?" : "`1234567890-=[]\\;',./")[at];
};
/** The modifiers an event holds, as a chord prefix (a pointer record's last field; glue.js's press writes the same). */
const modifiers = e => (e.shiftKey ? "Shift+" : "") + (e.ctrlKey ? "Control+" : "") + (e.altKey ? "Alt+" : "") + (e.metaKey ? "Meta+" : "");
/** The `PointerEvent` line of `e` at `el`: the point from its content box in its own CSS px (a scale undone), the button and the buttons, pressure, device, id, the viewport point (LLP 1094 D11) and modifiers. */
function pointerLine(el, e, lifted = false) {
  const r = el.getBoundingClientRect(), cs = getComputedStyle(el);
  const sx = el.offsetWidth ? r.width / el.offsetWidth : 1, sy = el.offsetHeight ? r.height / el.offsetHeight : 1;
  const left = parseFloat(cs.borderLeftWidth) + parseFloat(cs.paddingLeft), top = parseFloat(cs.borderTopWidth) + parseFloat(cs.paddingTop);
  const type = e.pointerType === "pen" || e.pointerType === "touch" ? e.pointerType : "mouse";
  return `${(e.clientX - r.left) / (sx || 1) - left},${(e.clientY - r.top) / (sy || 1) - top},${e.button ?? -1},${lifted ? 0 : e.buttons},${lifted ? 0 : Math.min(1, Math.max(0, e.pressure || 0))},${type},${e.pointerId ?? 1},${e.clientX},${e.clientY},${modifiers(e)}`;
}
/** @ref LLP 1116.004 D5.2 — a `radiogroup` of `radio`s, a value chosen from a few (the segmented
 * control the shell draws), keyed as WAI-ARIA's radio group and a native one: one Tab stop, its
 * checked eligible radio or else its first eligible one on entry, set as Tab goes down, before the browser moves the
 * focus; and the arrows move the choice, to the next or previous radio, wrapping and past
 * disabled ones (Left and Right mirrored right to left), focused and pressed. */
const radiosOf = group => viewChildren(group).filter(el => el.getAttribute?.("role") === "radio");
// Eligible as LLP 1166.000 D1 has it; the box test stands where a browser has no `checkVisibility`.
const radioUsable = el => !el.disabled && el.getAttribute("aria-disabled") !== "true" && !el.closest("[inert]")
  && el.checkVisibility?.({ visibilityProperty: true }) !== false && el.getClientRects().length > 0;
// Both renderers keep authored updates here, even when the DOM already has roving's -1.
const roveAllowed = el => {
  if (el.exactAuthoredTabIndex === undefined) el.exactAuthoredTabIndex = el.getAttribute("tabindex");
  return !(Number(el.exactAuthoredTabIndex) < 0);
};
const rove = (all, stop) => {
  for (const r of all) if (roveAllowed(r)) r.tabIndex = r === stop ? 0 : -1;
};
function radioStops(root) {
  for (const group of root.querySelectorAll('[role="radiogroup"]')) {
    const all = radiosOf(group), live = all.filter(r => radioUsable(r) && roveAllowed(r)), at = document.activeElement;
    rove(all, all.includes(at) ? at : live.find(r => r.getAttribute("aria-checked") === "true") ?? live[0]);
  }
}
/** An arrow on a radio group's radio moves its choice; whether it did. */
function radioArrow(root, event) {
  const el = event.target, group = viewParent(el);
  if (el?.getAttribute?.("role") !== "radio" || group?.getAttribute?.("role") !== "radiogroup" || !root.contains(group)) return false;
  const rtl = getComputedStyle(group).direction === "rtl";
  const step = { ArrowDown: 1, ArrowUp: -1, ArrowRight: rtl ? -1 : 1, ArrowLeft: rtl ? 1 : -1 }[event.key];
  if (!step) return false;
  event.preventDefault();
  const all = radiosOf(group), live = all.filter(r => r === el || radioUsable(r));
  const next = live[(live.indexOf(el) + step + live.length) % live.length];
  if (next !== el) { rove(all, next); next.focus(); next.click(); }
  return true;
}
/** @ref LLP 1115.000 D4.5 — a `tablist`'s tabs and a `menu`'s items (a chooser's too, drawn as a menu) move by the
 * arrow keys, as WAI-ARIA's patterns give the roles and a web developer expects of them: Left and Right along a tablist
 * (Up and Down when it is vertical: its `aria-orientation`, or a root tab bar the shell draws as a sidebar), Up and Down
 * in a menu, Home and End to its ends (into the one on top: Up and Down from outside, all four from a menu beneath),
 * wrapping, past disabled ones. The focus moves and nothing is pressed: a tab's press is the app's action (a fetch, in
 * Shelf's Discover), so Enter or Space chooses, as a `<button>` takes them. A tablist's entry stop is its selected
 * eligible tab, or its first eligible tab. While a member has focus, Tab leaves the group in either direction, including from an
 * authored exclusion or an aria-disabled member. */
const MENUS = '[role="menu"], [data-exact-chooser]', ROVES = `[role="tablist"], ${MENUS}`;
const roversOf = group => [...group.querySelectorAll(group.matches(MENUS) ? 'button, a[href], [role^="menuitem"]' : '[role="tab"]')].filter(el => el.closest(ROVES) === group);
// The open menu on top: the last to open (navigation.js stamps `exactOpened`), which is the top layer's order and not
// the document's (a chooser a menu's row opens stands over the menu, wherever each is written).
const topMenu = root => [...root.querySelectorAll(`:is(${MENUS}):popover-open`)].sort((a, b) => (a.exactOpened ?? 0) - (b.exactOpened ?? 0)).at(-1);
function tabStops(root) {
  for (const list of root.querySelectorAll('[role="tablist"]')) {
    const all = roversOf(list), live = all.filter(t => radioUsable(t) && roveAllowed(t)), at = document.activeElement;
    rove(all, all.includes(at) ? at : live.find(t => t.getAttribute("aria-selected") === "true") ?? live[0]);
  }
}
/** An arrow, Home or End among a tablist's tabs or a menu's items moves the focus; whether it did. */
function roveArrow(root, event) {
  const el = event.target, key = event.key, within = el?.closest?.(ROVES);
  if (!el || !(root.contains(el) || el === document.body)) return false;
  const top = /^(Arrow(Up|Down)|Home|End)$/.test(key) && !el?.matches?.("input, textarea, select, [contenteditable]") ? topMenu(root) : null;
  const open = top && !top.contains(el) && (within ? within.matches(MENUS) : /^Arrow/.test(key)) ? top : null; // from a menu beneath it, or by an arrow from outside
  const group = open ?? within;
  if (!group || !root.contains(group)) return false;
  // An arrow steps from the focused member whatever its state; Home and End go to the first and last usable one.
  const menu = group.matches(MENUS), all = roversOf(group), usable = all.filter(radioUsable), live = all.filter(item => item === el || usable.includes(item));
  if (!open && !all.includes(el)) return false;
  const vertical = menu || group.getAttribute("aria-orientation") === "vertical" || group.hasAttribute("data-exact-sidebar-tabs"), rtl = getComputedStyle(group).direction === "rtl";
  const step = (vertical ? { ArrowDown: 1, ArrowUp: -1 } : { ArrowRight: rtl ? -1 : 1, ArrowLeft: rtl ? 1 : -1 })[key];
  const next = open ? live.at(key === "ArrowDown" || key === "Home" ? 0 : -1) : step ? live[(live.indexOf(el) + step + live.length) % live.length] : key === "Home" ? usable[0] ?? el : key === "End" ? usable.at(-1) ?? el : null;
  if (!next) return false;
  event.preventDefault();
  if (next !== el) { if (!menu) rove(all, next); next.focus(); }
  return true;
}
/** A printable key in a menu moves the focus to the next of its items whose label starts with what was typed in the
 * last half second, as the WAI-ARIA menu pattern's type-ahead and the platform's menus give it: the same letter again
 * goes on to the next item it starts, wrapping, past disabled ones. The items are the menu's `menuitem`s, whether the
 * author or the host (menu-rows.js) gave the role. Space starts no search (it presses), and a field in the menu keeps
 * its keys. Whether it moved the focus. */
let typed = "", typedAt = -Infinity;
function typeAhead(root, event) {
  const el = event.target, menu = el?.closest?.('[role="menu"]'), key = event.key;
  if (!menu || !root.contains(menu) || [...key].length !== 1 || el.matches("input, textarea, select, [contenteditable]") || event.timeStamp - typedAt > 500 && key === " ") return false;
  typed = (event.timeStamp - typedAt > 500 ? "" : typed) + key.toLocaleLowerCase();
  typedAt = event.timeStamp;
  const items = [...menu.querySelectorAll('[role^="menuitem"]')].filter(item => item.closest('[role="menu"]') === menu && (item === el || radioUsable(item)));
  // One letter, or the same one again, steps on from the focused item; more letters refine the match from it.
  const again = [...typed].every(c => c === typed[0]), prefix = again ? typed[0] : typed, at = items.indexOf(el) + (again ? 1 : 0);
  const label = item => (item.getAttribute("aria-label") ?? item.textContent).trim().toLocaleLowerCase();
  const next = [...items.slice(at), ...items.slice(0, at)].find(item => !(again && item === el) && label(item).startsWith(prefix));
  if (!next) return false;
  event.preventDefault();
  next.focus();
  return true;
}
/** The draggable element around `target` whose mark decides a drag, an SVG element's mark skipped as drag-out.js skips
 * it, since an engine may ignore it (Firefox does) */
function draggableAround(target) {
  let source = target?.closest?.('[draggable="true" i]');
  while (source?.namespaceURI === "http://www.w3.org/2000/svg") source = source.parentElement?.closest('[draggable="true" i]');
  return source ?? null;
}

export function createInputHandlers({ root, views, retiredViews, ready, inertAncestor, editMenu, dispatch, release: dispatchRelease = () => {}, velocity = {}, agentMode = false, log = () => {}, track = (p) => p, documents = null, pick = null }) {
  // A press stops bubbling at its nearest handler; otherwise the click reaches the root.
  // Append the default there so the handler's updates and showPicker run first.
  // A successful showPicker consumes the activation, even for another input.
  // The JS executor may hold the press's commit for a view transition (shared.js):
  // the choice waits for it, and the input is checked again.
  let pickerActivations = 0;
  (globalThis.exact ??= {}).pickerOpened = () => pickerActivations++;
  if (AGENT_ADMITTED && agentMode && pick) root.addEventListener("click", event => {
    const el = event.target;
    if (!(el instanceof HTMLInputElement) || el.type !== "file") return;
    const activation = pickerActivations;
    const after = event.composedPath().find(n => n === root || hearsPress(n));
    const usable = () => ready() && el.isConnected && !retiredViews.has(el) && [...views.values()].includes(el)
      && el.type === "file" && !el.matches(":disabled") && !inertAncestor(el) && el.getClientRects().length > 0
      && !["hidden", "collapse"].includes(getComputedStyle(el).visibility);
    after.addEventListener("click", e => {
      if (e !== event || e.defaultPrevented || !usable()) return;
      e.preventDefault();
      (globalThis.exact.afterCommit ?? (run => run()))(() => { if (pickerActivations === activation && usable()) pick(el); });
    }, { once: true });
  }, true);
  // @ref LLP 1005 §Events — a pointer's down goes along the event's own path,
  // innermost first, to each node's `exactPointerDown` (`pointer` below), from
  // this one listener at the root's capture, before any element's: a
  // recognizer that stops the event (a pan taking its contact) takes nothing
  // from the pointer events, as on the native hosts.
  // Every holder is fixed before any down runs, as the native hosts fix the
  // path: a node a handler further in disables or removes still hears the up.
  if (WASM_HOST) root.addEventListener("pointerdown", e => { const path = e.composedPath(); for (const n of path) n.exactPointerDown?.(e, true); for (const n of path) n.exactPointerDown?.(e, false); }, true);
  // @ref LLP 1038 §7 — a plain click on a same-origin link to a declared
  // route stays in this document: a link with its own `press` navigates by
  // it; any other goes to the root's `navigate` handler, as popstate does.
  // With no `navigate` handler it is the runner's own `go` (LLP 1115.000
  // D4.3), as Back is its own `back`: a link to a route never reloads the page.
  // A modified or other-button click, a `target` or `download`, is the
  // browser's alone — a pressing link's press does not also run — and so are
  // other origins and undeclared paths (a file). A link to a fragment is a
  // location too (LLP 1153 D5): `href="#moves"` is the visit shown, read from
  // the attribute, since the shell's `<base href="/">` resolves it to `/`.
  // The page's own `exact` names the route table and the root's handler.
  if (WASM_HOST) document.addEventListener("click", event => {
    const a = event.target.closest?.("a[href]");
    if (!a || !root.contains(a) || event.defaultPrevented || !ready()) return;
    const press = a.exactHandlers?.includes("press");
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey
      || (a.target && a.target !== "_self") || a.hasAttribute("download")) { if (press) event.stopPropagation(); return; }
    const shown = globalThis.history?.state?.url ?? location.pathname + location.search, href = a.getAttribute("href") ?? "";
    const url = new URL(href[0] === "#" ? shown.split("#")[0] + href : a.href, location.href), to = url.pathname + url.search, here = to === location.pathname + location.search || to === shown.split("#")[0];
    // (A JS-target page with no GPU surface writes nothing in: its route table is its own.)
    const { wasm, writeIn = () => 0, navigate, go } = globalThis.exact;
    if (url.origin !== location.origin || wasm.exact_route_match(writeIn(to)) !== 1) return;
    const nav = root.firstElementChild, heard = nav?.hasAttribute("navigationBack") && nav.exactHandlers?.includes("navigate");
    event.preventDefault();
    if (press || (here && !url.hash && !shown.includes("#"))) return;
    // The fragment shown, linked again, moves no router and scrolls again: the runner's `scrollToFragment`.
    if (!(heard ? navigate(to + url.hash) : go(to + url.hash))?.ops?.some(op => op.op === "router" || op.name === "scrollToFragment")) wasm.exact_log(writeIn(`history: link ${JSON.stringify(to + url.hash)} refused`));
  }, true);
  document.addEventListener("keydown", (event) => {
    touched = true; // any key, Tab included: a route's focus shows the ring to a keyboard's user (`arrive`, below)
    if (event.key === "Tab") { // a radio group's one stop, and a tablist's, before the focus moves
      if (RADIO_GROUPS) radioStops(root);
      if (ROVING) tabStops(root);
    }
    if (event.isComposing || event.keyCode === 229 || event.key === "Process" || !ready() || event.defaultPrevented) return; // an input method's key, WebKit's committing Enter included (rt.js `onKey`)
    if (event.key === "Enter" || event.key === " ") note(event, true); // a key's press begins (`arrive`, below)
    // A commit may restore the authored runs: refresh their Tab stops and press origin above.
    // Shortcuts already heard the source event.
    if (event.exactFlowCapture) return;
    if (!SHORTCUTS) return;
    const editing = event.composedPath().some(el => el?.isContentEditable || el?.matches?.("input, textarea, [role=textbox], [role=searchbox], [role=combobox]"));
    const physical = physicalKey(event);
    const matches = (chord) => {
      const parts = chord.split("+");
      let key;
      if (chord === "+") { key = "Plus"; parts.length = 0; }
      else if (parts.length >= 3 && parts.slice(-2).every(p => p === "")) { key = "Plus"; parts.splice(-2); }
      else key = parts.pop();
      if (key === "Plus") key = "+";
      if (key === "Space") key = " ";
      const modifiers = new Set(parts);
      if (editing && key !== "Escape" && !modifiers.has("Meta") && !modifiers.has("Control")) return false;
      return (key?.length === 1 || shortcutKeys.has(key) || /^F([1-9]|[12][0-9]|3[0-5])$/.test(key))
        && [...modifiers].every(m => ["Meta", "Control", "Alt", "Shift"].includes(m))
        && event.metaKey === modifiers.has("Meta") && event.ctrlKey === modifiers.has("Control")
        && event.altKey === modifiers.has("Alt") && event.shiftKey === modifiers.has("Shift")
        && (event.key?.toLowerCase() === key.toLowerCase() || physical === key.toLowerCase());
    };
    // Nothing behind the frontmost modal — a modal `dialog`, or the last
    // shown `aria-modal` view (gallery F22) — and never Enter or Space while
    // the focus is a control they activate (onboarding F27).
    const modal = document.activeElement.closest("dialog:modal") ?? [...root.querySelectorAll('[aria-modal="true"]')].findLast(m => m.getClientRects().length && !inertAncestor(m));
    const focus = document.activeElement, activates = focus?.matches?.("button, a[href], summary, input[type=checkbox], input[type=radio], [data-exact-on~=press]");
    for (const el of root.querySelectorAll("button[aria-keyshortcuts]")) {
      if (modal && !modal.contains(el)) continue;
      if (activates && focus !== el && (event.key === "Enter" || event.key === " ") && !(event.metaKey || event.ctrlKey || event.altKey || event.shiftKey)) continue;
      // A hidden Back control takes its shortcut through the visible bar button, in a header or in content.
      const target = backButton(el) ?? el;
      if (!target.isConnected || !(target.getClientRects().length || target.hasAttribute("data-exact-bar-back")) || inertAncestor(el) || getComputedStyle(target).visibility !== "visible") continue;
      if (!(el.getAttribute("aria-keyshortcuts") ?? "").split(/\s+/).some(matches)) continue;
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!event.repeat && !el.disabled) target.click();
      return;
    }
  }, true);
  // A pressable that is not a button or a link (`tabindex="0"`, written
  // where its handlers are) activates as one, as it does natively: Enter, or
  // Space unless it is a link, after the key's handlers, unless one
  // prevented it — the bubble phase at the document is after them all (chat F14).
  // A radio group's arrows move its choice, held or not, after the key's own handlers too; a tablist's and a menu's
  // move the focus.
  document.addEventListener("keydown", (event) => {
    const el = event.target;
    if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || event.key === "Process" || event.metaKey || event.ctrlKey || event.altKey || !ready()) return;
    if (RADIO_GROUPS && radioArrow(root, event) || ROVING && (roveArrow(root, event) || typeAhead(root, event)) || event.repeat) return;
    // @ref LLP 1115.000 D4.14 — Space holds a `<button>` down until its keyup,
    // when the browser clicks it. Its press shows meanwhile, as under a pointer:
    // the unsaid dim, a native button's highlight, a `-exact-press-scale`, a pressed paint.
    if (event.key === " " && el?.matches?.("button:enabled") && root.contains(el)) return keyed(el);
    if (typeof el?.matches !== "function" || !root.contains(el) || !pressesByKey(el)) return;
    if (el.closest(DISABLED)) return; // a disabled box takes no press by key either (LLP 1121 D8)
    if (event.key !== "Enter" && !(event.key === " " && el.getAttribute("role") !== "link")) return;
    event.preventDefault();
    el.click();
    if (event.key === " " && !surface(el)) keyed(el);
  });
  // @ref LLP 1115.000 D6 — a pressable covering the window is a surface (a
  // scrim that closes a sheet, a viewer that toggles its bars) and shows no
  // press, as on iOS (PressFeedback.swift `platformPressDims`). CSS cannot
  // measure a box against the window, so each press measures the innermost
  // pressable under it and marks it for the shell's rule, before a frame.
  const surface = at => {
    const el = at.closest?.('button, a[href], [data-exact-on~="press"]');
    if (!el || !root.contains(el)) return false;
    const r = el.getBoundingClientRect(), page = document.documentElement;
    return el.toggleAttribute("data-exact-surface", r.width >= page.clientWidth && r.height >= page.clientHeight * .9);
  };
  // Hold keyboard feedback until Space lifts, focus leaves, or the window or page loses the press
  let keyHold = null;
  const keyed = el => {
    const up = (e, now = false) => {
      if (e?.type === "keyup" && e.key !== " ") return;
      // The element can blur before the window does. Cancel that edge without showing a quick tap.
      showPress(el, false, now, e?.type !== "blur");
      removeEventListener("keyup", up, true); el.removeEventListener("blur", up);
      if (keyHold === up) keyHold = null;
    };
    keyHold?.(null, true);
    showPress(el, true);
    up.el = el;
    keyHold = up;
    addEventListener("keyup", up, true); el.addEventListener("blur", up);
  };
  // @ref LLP 1061 D3, D7 — the innermost pressable owns the progress of its press over its own base styles.
  const PRESS = {
    ...(PRESS_PAINT ? {
      "background-color": "--exact-press-background-progress",
      color: "--exact-press-color-progress", opacity: "--exact-press-opacity-progress",
    } : {}),
    translate: "--exact-pressed", scale: "--exact-press-scale-progress", button: "--exact-button-press", dim: "opacity",
  };
  const PAINT_ROWS = {
    "background-color": ["--exact-press-background-color"], color: ["--exact-press-color"],
    opacity: ["--exact-press-opacity", "--exact-press-opacity-to"],
  };
  const presses = new Map();
  const DEFAULT_TIMING = [120, 0, "cubic-bezier(.16,1,.3,1)"]; // LLP 1061 D7
  const DISABLED = ":disabled,[disabled]:not([disabled='false'])";
  const unavailable = el => !el.isConnected || !root.contains(el) || el.closest(`${DISABLED},[data-exiting]`) || inertAncestor(el);
  let held = null;
  const heldPress = el => held?.state === "active" && held.el === el;
  // Whether a live press on `el` is over because the node is gone, disabled, leaving or inert. A held node is not
  // live: its own watcher lets a gone or disabled one go, and it keeps its paint through inertness.
  const lostLive = el => !heldPress(el) && unavailable(el);
  const nativeButton = el => el.matches("button[data-button-style]");
  const refreshPaint = (el, state) => {
    if (!PRESS_PAINT || (!state.painted && !state.rule)) return;
    const declarations = paintRule(el, state), written = new Set(el.getAnimations());
    const key = hold(el, state, declarations);
    if (key) el.setAttribute("data-exact-painting", key);
    else el.removeAttribute("data-exact-painting");
    // Cancel only transitions Firefox started by writing the paint rule
    for (const a of el.getAnimations()) {
      if (!written.has(a) && a.transitionProperty in state.kept) a.cancel();
    }
  };
  const paintProgress = (cs, property) => Number(cs.getPropertyValue(PRESS[property]));
  // Whether the node declares a transition on `property` (by its name, `all`, or a shorthand holding it): the last
  // entry naming it decides, by its combined duration (duration + delay, which may be negative) above 0.
  const HOLDS = { "background-color": ["background"], "background-position": ["background"], "background-position-x": ["background", "background-position"], "background-position-y": ["background", "background-position"], "background-size": ["background"],
    "mask-position": ["mask"], "mask-position-x": ["mask", "mask-position"], "mask-position-y": ["mask", "mask-position"], "mask-size": ["mask"], color: [], opacity: [] };
  const transitioned = (cs, property) => {
    const list = s => s.split(/\s*,\s*/), names = list(cs.transitionProperty), times = list(cs.transitionDuration), delays = list(cs.transitionDelay);
    let on = false;
    names.forEach((name, i) => { if (name === "all" || name === property || HOLDS[property]?.includes(name)) on = Math.max(0, parseFloat(times[i % times.length]) || 0) + (parseFloat(delays[i % delays.length]) || 0) > 0; });
    return on;
  };
  // An authored transition running (or paused, as the agent's clock pauses it) on `property`.
  const PAINTED_KEY = { opacity: "opacity", color: "color", "background-color": "backgroundColor" };
  const running = (el, property) => el.getAnimations().find(a => a.transitionProperty === property && (a.playState === "running" || a.playState === "paused"));
  // A layered list (a background's, a mask's) with the paint's own layer first: each of the node's lists cycled out
  // to its images' count, as CSS matches them, then `first` before it.
  const items = said => { const out = []; let depth = 0, from = 0, quote = ""; for (let i = 0; i < said.length; i++) { const c = said[i]; if (quote) { if (c === "\\") i++; else if (c === quote) quote = ""; } else if (c === '"' || c === "'") quote = c; else if (c === "(") depth++; else if (c === ")") depth--; else if (c === "," && !depth) { out.push(said.slice(from, i).trim()); from = i + 1; } } out.push(said.slice(from).trim()); return out; };
  const layered = (said, count, first) => { const list = items(said); return [first, ...Array.from({ length: count }, (_, i) => list[i % list.length])].join(", "); };
  const LAYER = { background: { position: "0% 0%", size: "auto", repeat: "repeat", origin: "padding-box", clip: "border-box", attachment: "scroll" },
    mask: { position: "0% 0%", size: "auto", repeat: "repeat", origin: "border-box", clip: "no-clip", composite: "intersect", mode: "alpha" } };
  // The paint's layer over a node's own (`kind`: background or mask): its images, then every per-layer list shifted
  // with them. Null where that would move the node's own layers under a transition (a position or size list
  // changing length interpolates): that paint is left out.
  const layer = (cs, kind, mine) => {
    const read = name => cs.getPropertyValue(`${kind}-${name}`), images = read("image"), count = images === "none" ? 0 : items(images).length;
    const out = [`${kind}-image: ${count ? `${mine}, ${images}` : mine} !important`];
    for (const [name, value] of Object.entries(LAYER[kind])) {
      const theirs = read(name);
      // With no images of its own, a node's background clip and origin are its background colour's: the layer
      // takes them (a mask's own no-clip still applies, so a shadow fades with the node).
      if (!count && ((kind === "background" && (name === "clip" || name === "origin")) || items(theirs)[0] === value)) continue;
      if ((name === "position" || name === "size") && [`${kind}-${name}`, ...(name === "position" ? [`${kind}-position-x`, `${kind}-position-y`] : [])].some(n => transitioned(cs, n))) return null;
      out.push(`${kind}-${name}: ${count ? layered(theirs, count, value) : value} !important`);
    }
    return out.join("; ");
  };
  // Mix pressed colours over their bases in premultiplied sRGB (LLP 1061 D7). Write opacity directly where
  // computedStyleMap gives the exact base. Use a uniform mask in Firefox where it can represent the target.
  // WebKit supplies the exact number and clips masks to the box, including shadows.
  // Where the node declares a transition on a property the rule writes, match the underlying value at progress 0,
  // so no CSS transition starts and hides the paint: write only a base that round-trips (an rgb() colour, the
  // engine's opacity), wait for a running transition of that property before writing, then keep the rule for the
  // press, re-read with the paint on, so the paint's turn starts nothing (Firefox's transition styles see the
  // animation).
  // An opaque pressed background colour that can't be written so (over another space's look, or while its
  // transition runs) is for the press a background-image layer over the node's background-color instead, which
  // is the same mix (images change discretely, starting no transition).
  const paintRule = (el, state) => {
    if (!el.closest("#exact-root")) return "";
    // What is kept holds only while the node's own inputs (its inline style: its opacity and colour, its pressed
    // values; its class) are as they were when it was written, and no sheet, scheme or token changed: else the look
    // beneath is read again (a base change, which the node's transition times, D8).
    const inputs = `${el.getAttribute("style")}|${el.getAttribute("class")}`, was = state.kept ?? {}, before = new Set(el.getAnimations());
    if (state.inputs !== inputs || state.stale) state.kept = {};
    state.inputs = inputs; state.stale = false;
    const kept = state.kept ?? {};
    // Re-read with only the kept declarations on, under the same key: the look beneath of what isn't kept, and the
    // node's own images and masks.
    if (state.rule) hold(el, state, Object.values(kept).join("; "), []);
    const cs = getComputedStyle(el), own = name => el.style.getPropertyValue(name) ? cs.getPropertyValue(name).trim() : "";
    // A transition the read itself started (Firefox's transition styles see the animation, so taking a kept
    // declaration off is a change there) toward the very look beneath the kept one was written from: none of the
    // node's, cancelled at once. One toward another look is the base's own move, the node's to time (D8).
    for (const a of el.getAnimations()) if (!before.has(a) && a.transitionProperty in was && String(a.effect.getKeyframes().at(-1)?.[PAINTED_KEY[a.transitionProperty]]) === state.keptBase?.[a.transitionProperty]) a.cancel();
    const out = [], next = {};
    const number = (name, copy) => { if (!own(name)) return null; const v = parseFloat(cs.getPropertyValue(copy)); return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : null; };
    const settle = () => { if (presses.get(el) === state) refreshPaint(el, state); };
    const later = property => { const a = running(el, property); if (a && !state.waits?.has(a)) { (state.waits ??= new Set()).add(a); a.finished.then(settle, settle); } return !!a; };
    // `name` `p` of the way from the look beneath to the pressed colour (`inSrgb`, its `color(srgb …)`),
    // premultiplied. From an rgb() look (b at alpha β; every colour of the app's has an 8-bit alpha, which written
    // as k / 255 computes exactly as it does) to v at alpha a, the alpha is β + (a − β)·p and each channel
    // b + (v − b)·a·p / (β + (a − β)·p): exactly the look at 0 (the divisor held off 0, where both are clear). From
    // another look, color-mix, where the node declares no transition of `name`. Null where neither applies:
    // another space doesn't compute as it reads.
    const mixed = (name, look, pressed, inSrgb) => {
      const p = `var(${PRESS[name]})`;
      const m = /^rgba?\((\d+), (\d+), (\d+)(?:, ([\d.]+))?\)$/.exec(look), x = /^color\(srgb ([-\d.e]+) ([-\d.e]+) ([-\d.e]+)(?: \/ ([\d.]+))?\)$/.exec(cs.getPropertyValue(inSrgb).trim());
      if (m && x) {
        const b = m.slice(1, 4).map(Number), v = x.slice(1, 4).map(Number), a = x[4] == null ? 1 : Number(x[4]);
        const k = m[4] == null ? 255 : Math.round(Number(m[4]) * 255), beneath = k === 255 ? "1" : `${k} / 255`, opaque = k === 255 && a === 1;
        const alpha = `${beneath} + ${a - k / 255} * ${p}`, w = opaque ? p : `${a} * ${p} / max(${alpha}, 0.000001)`;
        return `${name}: rgb(${b.map((bi, i) => `calc(${bi} + ${v[i] * 255 - bi} * ${w})`).join(" ")}${opaque ? "" : ` / calc(${alpha})`}) !important`;
      }
      return transitioned(cs, name) ? null : `${name}: color-mix(in srgb, var(${pressed}) calc(${p} * 100%), ${look}) !important`;
    };
    state.background = false;
    if (kept["background-color"]) next["background-color"] = kept["background-color"];
    else if (own("--exact-press-background-color")) {
      const declared = transitioned(cs, "background-color"), opaque = !cs.getPropertyValue("--exact-press-background-in-srgb").includes("/");
      const d = !state.layered && !(declared && running(el, "background-color")) && mixed("background-color", cs.backgroundColor, "--exact-press-background-color", "--exact-press-background-in-srgb");
      if (d) { if (declared) next["background-color"] = d; else out.push(d); }
      else if (state.layered || opaque) {
        // Once layered, layered for the press: the property written later would start the node's transition.
        state.layered = true;
        const bg = layer(cs, "background", `linear-gradient(color-mix(in srgb, var(--exact-press-background-color) calc(var(${PRESS["background-color"]}) * 100%), transparent) 0 0)`);
        if (bg) out.push(bg);
        state.background = !!bg;
      } else later("background-color");
    }
    const to = number("--exact-press-opacity-to", "--exact-press-opacity-to-number"), share = number("--exact-press-opacity", "--exact-press-opacity-number");
    const held = el.computedStyleMap?.().get("opacity")?.value, base = Number.isFinite(held) ? held : Number(cs.opacity);
    const p = `var(${PRESS.opacity})`;
    state.factor = null;
    if (kept.opacity) next.opacity = kept.opacity;
    else if ((to != null || share != null) && Number.isFinite(base) && !later("opacity")) {
      if (Number.isFinite(held) || (to != null && to > base && !transitioned(cs, "opacity"))) {
        const d = `opacity: ${to != null ? `calc(${held ?? base} + ${to - base} * ${p})` : `calc(${held ?? base} * (1 - ${p} * ${1 - share}))`} !important`;
        if (transitioned(cs, "opacity")) next.opacity = d; else out.push(d);
      } else if (to == null || (base > 0 && to <= base)) {
        const factor = to != null ? `calc((${base} + ${to - base} * ${p}) / ${base})` : `calc(1 - ${p} * ${1 - share})`;
        const mask = layer(cs, "mask", `linear-gradient(rgb(0 0 0 / ${factor}) 0 0)`);
        if (mask) { out.push(mask); state.factor = paint => to != null ? (base + (to - base) * paint) / base : 1 - paint * (1 - share); }
      }
    }
    if (kept.color) next.color = kept.color;
    else if (own("--exact-press-color") && !later("color")) {
      const d = mixed("color", cs.color, "--exact-press-color", "--exact-press-color-in-srgb");
      if (d && transitioned(cs, "color")) next.color = d; else if (d) out.push(d);
    }
    // A boundary below that writes again the colour this node gives it takes the press's colour too, as this node's
    // own colour paints it, on this node's progress, inherited (`hold`).
    const coloured = !!next.color || out.some(d => d.startsWith("color:"));
    state.below = coloured ? pressBelow(el) : [];
    if (state.below.length) out.push(`--x-press-color-progress: var(${PRESS.color})`);
    state.kept = next;
    // The look each kept declaration was written from: carried with it while it is kept (a re-read with it on sees
    // the painted look, not that one), read now only where it was written now.
    const look = { opacity: String(held ?? base), color: cs.color, "background-color": cs.backgroundColor };
    state.keptBase = Object.fromEntries(Object.keys(next).map(name => [name, kept[name] ? state.keptBase?.[name] : look[name]]));
    return [...out, ...Object.values(next)].join("; ");
  };
  // What a painting node shows, where the page can't read it from the node's computed style (a paint on images):
  // its opacity times the mask, its background-color with a layered pressed colour over it (as pixels, 8-bit; its
  // own images and masks aren't in it), its colour. Without OffscreenCanvas, the background-color beneath.
  // The agent's node detail reports it (scripts/agent-inspect.mjs), and the press tests read it.
  if (AGENT_ADMITTED && PRESS_PAINT) (globalThis.exact ??= {}).pressShown = el => {
    const state = presses.get(el);
    if (!state?.painted) return null;
    const cs = getComputedStyle(el), opacity = paintProgress(cs, "opacity");
    const shown = { opacity: String(+(Number(cs.opacity) * (state.factor?.(opacity) ?? 1)).toPrecision(6)), color: cs.color, backgroundColor: cs.backgroundColor };
    if (state.background && typeof OffscreenCanvas === "function") {
      const g = new OffscreenCanvas(1, 1).getContext("2d");
      if (!g) return shown;
      g.fillStyle = cs.backgroundColor; g.fillRect(0, 0, 1, 1);
      g.globalAlpha = Math.min(1, Math.max(0, paintProgress(cs, "background-color"))); g.fillStyle = cs.getPropertyValue("--exact-press-background-in-srgb").trim(); g.fillRect(0, 0, 1, 1);
      const [r, gr, b, a] = g.getImageData(0, 0, 1, 1).data;
      shown.backgroundColor = a === 255 ? `rgb(${r}, ${gr}, ${b})` : `rgba(${r}, ${gr}, ${b}, ${Math.round(a / 255 * 1000) / 1000})`;
    }
    return shown;
  };
  // Each painting node's rule, in one sheet on its root (a document or a shadow root; a frame's own), keyed by
  // the value of its data-exact-painting, an attribute the paint sets anyway, so no observer sees anything new.
  const paintSheets = new WeakMap();
  let paintKeys = 0;
  const sheetOf = el => {
    const root = el.getRootNode?.(), Sheet = (root?.ownerDocument ?? root)?.defaultView?.CSSStyleSheet;
    if (!Sheet || !root || !("adoptedStyleSheets" in root)) return null;
    let sheet = paintSheets.get(root);
    try {
      if (!sheet) paintSheets.set(root, sheet = new Sheet());
      // Adopted again if the page replaced the list since.
      if (!root.adoptedStyleSheets.includes(sheet)) root.adoptedStyleSheets = [...root.adoptedStyleSheets, sheet];
    } catch { return null; }
    return sheet;
  };
  // The boundaries under `el` that write again the colour `el` gives them: the JS target's modes.js finds them; on
  // the wasm target each names the view whose colour it writes again (`data-exact-from`, host/web/src/host/looks.rs),
  // which is `el` or a view above it.
  const pressBelow = el => globalThis.exact?.pressBelow?.(el) ?? [...el.querySelectorAll("[data-exact-from]")].filter(b => {
    const from = views.get(Number(b.getAttribute("data-exact-from")));
    return !!from && (from === el || !el.contains(from));
  });
  const hold = (el, state, declarations, below = state.below) => {
    drop(state);
    const sheet = declarations && sheetOf(el);
    if (!sheet) return "";
    state.key ??= String(++paintKeys);
    const painting = `#exact-root [data-exact-painting="${state.key}"][data-exact-painting]`;
    const insert = text => sheet.cssRules[sheet.insertRule(text, sheet.cssRules.length)];
    state.rule = { sheet, rule: insert(`${painting} { ${declarations} }`), below: [] };
    // Each boundary below it eases from the colour it shows (a look's included; its own rule is off) to the press's,
    // resolved in the boundary's modes: the JS target's is a token's property, which resolves there; the wasm host
    // resolves it on each boundary, by the pressed view (`--exact-press-color-below-<view>`, host/web/src/host/looks.rs).
    const own = el.style.getPropertyValue("--exact-press-color"), view = el.getAttribute("data-view");
    const pressed = own && view ? `var(--exact-press-color-below-${view}, ${own})` : own;
    below.forEach((b, n) => {
      const base = getComputedStyle(b).color, mark = `${state.key}.${n}`;
      if (!base || !pressed) return;
      b.setAttribute("data-x-pr", mark);
      state.rule.below.push([b, mark, insert(`${painting} [data-x-pr="${mark}"] { color: color-mix(in srgb, ${pressed} calc(var(--x-press-color-progress) * 100%), ${base}) !important }`)]);
    });
    return state.key;
  };
  const drop = state => {
    if (!state.rule) return;
    const { sheet, rule, below } = state.rule, rules = [...sheet.cssRules];
    state.rule = null;
    const at = [rule, ...below.map(([, , r]) => r)].map(r => rules.indexOf(r)).filter(i => i >= 0).sort((a, b) => b - a);
    for (const i of at) sheet.deleteRule(i);
    for (const [b, mark] of below) if (b.getAttribute("data-x-pr") === mark) b.removeAttribute("data-x-pr");
  };
  const targets = (el, state) => {
    const own = row => el.style.getPropertyValue(row);
    const authored = PRESS_PAINT ? Object.keys(PAINT_ROWS).filter(p => PAINT_ROWS[p].some(own)) : [];
    state.painted = authored.length > 0;
    if (own("--exact-press")) authored.push("scale");
    if (own("--exact-press-translate")) authored.push("translate");
    if (nativeButton(el)) authored.push("button");
    else if (!authored.length && !el.style.opacity) {
      const cs = getComputedStyle(el);
      if (Number(cs.getPropertyValue("--exact-dims")) && (state.tracks.has("dim") || Number(cs.opacity) === 1)) {
        authored.push("dim");
      }
    }
    return authored;
  };
  const dropTrack = (el, state, property) => {
    state.tracks.get(property)?.animation.cancel();
    state.tracks.delete(property);
    if (property === "button") el.removeAttribute("data-exact-button-press");
  };
  // CSS Transitions 1 §3.1. Keep the logical start and factor for the next reversal.
  const moveTrack = (el, state, property, to, timing) => {
    const old = state.tracks.get(property);
    const sample = old?.animation.effect.getComputedTiming();
    const progress = sample?.progress ?? 0;
    const running = sample?.localTime < sample?.endTime;
    const from = old ? running ? old.from + (old.to - old.from) * progress : old.to : 0;
    const reverses = running && to === old.start;
    const shortening = reverses ? Math.min(1, Math.abs(progress * old.shortening + 1 - old.shortening)) : 1;
    const [duration, delay, easing] = (from === to ? null : timing) ?? [0, 0, "linear"];
    const ms = duration * shortening, wait = delay < 0 ? delay * shortening : delay;
    if (old) dropTrack(el, state, property);
    if (!to && (from === to || ms + wait <= 0)) return;
    const value = p => property === "dim" ? 1 - .35 * p : p; // 65% at full press (LLP 1115.000 D6)
    const name = PRESS[property];
    const animation = el.animate([{ [name]: value(from) }, { [name]: value(to) }], {
      duration: ms, delay: wait, easing: ms + wait > 0 ? easing : "linear", fill: "both",
    });
    state.tracks.set(property, { animation, from, to, start: reverses ? old.to : from, shortening });
    animation.onfinish = () => {
      if (state.tracks.get(property)?.animation !== animation) return;
      if (to) {
        // Land at the pressed endpoint even if linear() ends at another output
        if (animation.effect.getComputedTiming().progress !== 1) animation.effect.updateTiming({ easing: "linear" });
        return;
      }
      dropTrack(el, state, property);
      if (!state.tracks.size) endPress(el, state);
    };
    if (property === "button") el.setAttribute("data-exact-button-press", "");
    if (ms + wait <= 0) animation.finish();
  };
  const endPress = (el, state) => {
    presses.delete(el);
    if (!presses.size) { sheets.disconnect(); gone.disconnect(); }
    state.observer.disconnect();
    cancelAnimationFrame(state.frame);
    for (const property of state.tracks.keys()) dropTrack(el, state, property);
    el.removeAttribute("data-pressed");
    if (PRESS_PAINT && state.rule) {
      el.removeAttribute("data-exact-painting");
      drop(state);
    }
  };
  const repaint = (el, state, stale = false) => {
    state.stale ||= stale;
    if (lostLive(el)) return endPress(el, state);
    let current = targets(el, state);
    for (const property of state.tracks.keys()) {
      if (!current.includes(property)) dropTrack(el, state, property);
    }
    refreshPaint(el, state);
    if (!current.length) current = targets(el, state);
    // Land newly added properties at once. CSS updates an existing track's target at its current progress.
    if (state.down) for (const property of current) {
      if (!state.tracks.has(property)) moveTrack(el, state, property, 1);
    }
    if (!state.down && !state.tracks.size) endPress(el, state);
  };
  const refreshPresses = () => { for (const [el, state] of presses) repaint(el, state, true); };
  const sheets = new MutationObserver(refreshPresses);
  const gone = new MutationObserver(() => {
    if (press && unavailable(press.el)) release(true);
    for (const [el, state] of presses) if (lostLive(el)) endPress(el, state);
  });
  globalThis.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", refreshPresses);
  (globalThis.exact ??= {}).repaintPress = refreshPresses; // The JS target changes its token sheet through the CSSOM

  const showPress = (el, down, now = false, tap = true) => {
    let state = presses.get(el);
    if (now || lostLive(el)) {
      if (state) endPress(el, state);
      return;
    }
    if (!state) {
      if (!down) return;
      const observer = new MutationObserver(() => repaint(el, state));
      state = { observer, tracks: new Map(), down: false, frame: 0, deferred: false };
      observer.observe(el, { attributes: true, attributeFilter: ["style", "class", "data-button-style"] });
      if (!presses.size) {
        sheets.observe(document.head, { childList: true, subtree: true, characterData: true });
        sheets.observe(document.documentElement, { attributes: true });
      }
      gone.observe(el.ownerDocument, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "data-exiting", "inert"] });
      presses.set(el, state);
    }
    if (state.down === down && !state.deferred) return;
    if (!down && tap && state.frame && !state.deferred) {
      // Show the pressed endpoints through the first frame, then release on the next one
      for (const property of [...state.tracks.keys()]) moveTrack(el, state, property, 1);
      state.deferred = true;
      return;
    }
    cancelAnimationFrame(state.frame);
    state.frame = 0;
    state.deferred = false;
    state.down = down;
    el.toggleAttribute("data-pressed", down);
    const current = targets(el, state);
    refreshPaint(el, state);
    const row = el.style.getPropertyValue("--exact-press-timing");
    const timings = row ? JSON.parse(row) : null;
    for (const property of new Set([...state.tracks.keys(), ...current])) {
      const timing = property === "button" || property === "dim" ? DEFAULT_TIMING
        : timings ? timings[property] : nativeButton(el) ? null : DEFAULT_TIMING;
      moveTrack(el, state, property, Number(down && current.includes(property)), agentMode ? null : timing);
    }
    if (down && !agentMode) state.frame = requestAnimationFrame(() => {
      state.frame = state.deferred ? requestAnimationFrame(() => {
        state.frame = 0;
        state.deferred = false;
        showPress(el, false);
      }) : 0;
    });
    if (!down && !state.tracks.size) endPress(el, state);
  };
  // Keep the press offset in the hit box (LLP 1061 D7). Neutralize the scale about the browser's transform origin.
  const unscaledBox = el => {
    const effect = presses.get(el)?.tracks.get("scale")?.animation.effect;
    if (!effect) return el.getBoundingClientRect();
    const frames = effect.getKeyframes();
    effect.setKeyframes(frames.map(frame => ({ ...frame, [PRESS.scale]: 0 })));
    const box = el.getBoundingClientRect();
    effect.setKeyframes(frames);
    return box;
  };
  const boxOf = el => { const r = unscaledBox(el); return [r.left, r.top, r.right, r.bottom]; };
  let press = null;
  // @ref LLP 1122 D8, LLP 1121 stage 8 E — a held state, shown as the real one: `active` the press of the node the
  // press resolves to (its paint and scale, eased as a real press, at once on the agent's clock), `hover` and
  // `focus-visible` that node's look (`data-exact-held`, which the looks rules take beside `:hover` and
  // `:focus-visible`; it snaps, an authored transition or not), handlers never told. One at a time; a disabled node
  // refuses one. Real input on the node, its going or its disabling lets it go (`dropped(reason)`). A real press on
  // the node as it is held ends there: the hold has it (its release unpaints nothing). Window blur and temporary
  // inertness cancel live input; a design hold keeps its paint through both.
  if (AGENT_ADMITTED) {
    // (A leaving view, `data-exiting`, is gone as it starts to leave.)
    const watch = new MutationObserver(() => { if (!held?.el.isConnected || held.el.closest("[data-exiting]")) letGo("gone"); else if (held.el.closest(DISABLED)) letGo("disabled"); });
    // (Held still as a look's edge is, below: `data-exact-snap`, which the press paint's observer doesn't watch.)
    const snap = (el, change) => stillThrough([el], change);
    // `reason` null: let go unsaid (a dev loop's new generation, whose tree goes and whose hold the server replays).
    const letGo = (reason, tap = false) => {
      if (!held) return;
      const h = held;
      held = null;
      watch.disconnect();
      if (h.state === "active") showPress(h.el, false, reason == null || reason === "gone" || reason === "disabled", tap); else snap(h.el, () => h.el.removeAttribute("data-exact-held"));
      if (reason) h.dropped?.(reason);
    };
    // (Announced once it is: a hold sent while the glue loads, after paint, waits for it — `exact-hold`.)
    queueMicrotask(() => globalThis.dispatchEvent?.(new Event("exact-hold")));
    (globalThis.exact ??= {}).hold = (el, state, dropped) => {
      if (state != null && !["active", "hover", "focus-visible"].includes(state)) return { error: "holdState is active, hover, focus-visible or null", malformed: true };
      letGo("replaced", state == null);
      if (state == null) return { held: null };
      // `active`: the innermost node with a press handler or a view's link, as a real press's (theme2's 07:48; LLP 1121 D8);
      // none there is `gone`.
      const node = state === "active" ? el?.closest?.("[data-exact-on~=press],a[data-view][href]") : el;
      if (!node?.isConnected || !root.contains(node) || node.closest("[data-exiting]")) return { reason: "gone", error: state === "active" ? "no press there: the node presses nothing, nor does any around it" : "the node is not on the page" };
      if (node.closest(DISABLED)) return { reason: "disabled", error: "a disabled node holds no state" };
      held = { el: node, asked: el, state, dropped };
      watch.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: ["disabled", "data-exiting"] });
      if (state === "active") {
        if (press?.el === node) press = null;
        if (keyHold?.el === node) { const up = keyHold; keyHold = null; removeEventListener("keyup", up, true); node.removeEventListener("blur", up); }
        showPress(node, true);
      } else snap(node, () => node.setAttribute("data-exact-held", state));
      return { held: state, el: node };
    };
    globalThis.exact.holdQuiet = () => letGo(null);
    // What is held on `el` (or on the press it resolved to, `el` named), for `layout` and inspect (LLP 1122 D8): its
    // state and the rows that state shows, by their
    // wire names: a press's paint rows (`:active` lowered to them), else the state look's rows in the looks sheet
    // (its `[data-exact-held~=…]` rule). The rows' places are the server's to join, from the source map.
    const WIRE = { "--exact-press-translate": "press_translate", "--exact-press": "press_scale", "--exact-press-background-color": "press_background_color", "--exact-press-color": "press_color", "--exact-press-opacity": "press_opacity", "--exact-press-opacity-to": "press_opacity_to" };
    // `ask`: the runner's agent (the wasm page): the rows as `layout` says the press's own (its D7 rows, `:active`
    // lowered to them) or its look's, each with the token it read (LLP 1121 D8, stage 9 E). Without one (the JS
    // target) they are the CSS the page holds, a varying token named where modes.js knows it. Either names the node
    // the state is held on (the press it resolved to, when named on a node inside it).
    globalThis.exact.heldOf = (el, ask = null) => {
      if (!held || (held.el !== el && held.asked !== el)) return null;
      el = held.el;
      // (The wasm page numbers its elements `data-view`; the JS target its own way, a link's `data-view` an empty marker.)
      const view = /^\d+$/.test(el.dataset?.view ?? "") ? Number(el.dataset.view) : globalThis.$exactViewId?.(el) ?? null;
      const node = ask && view != null ? ask({ op: "node", id: view, plan: true }) : null;
      if (node && !node.error) {
        const own = held.state === "active" ? Object.fromEntries(Object.entries(node.style ?? {}).filter(([name]) => Object.values(WIRE).includes(name)))
          : node.looks?.[held.state === "hover" ? "hover" : "focus_visible"] ?? {};
        const rows = Object.entries(own).map(([name, row]) => ({ name, value: row.value, ...(row.token ? { token: row.token } : {}) }));
        return { held: { state: held.state, node: view, site: node.site ?? null, instance: node.instance ?? [], rows } };
      }
      const rows = [];
      if (held.state === "active") for (const [prop, name] of Object.entries(WIRE)) { const value = el.style.getPropertyValue(prop).trim(); if (value) rows.push({ name, value, prop }); }
      else for (const sheet of document.styleSheets) {
        let list; try { list = [...sheet.cssRules]; } catch { continue; }
        for (const rule of list.flatMap(r => r instanceof CSSMediaRule ? [...r.cssRules] : [r])) {
          if (!rule.selectorText?.includes(`[data-exact-held~="${held.state}"]`) && !rule.selectorText?.includes(`[data-exact-held~=${held.state}]`)) continue;
          const own = el.matches(rule.selectorText);
          const card = !own && el.matches(':is([data-liststyle],[data-liststyle]>[data-exact-list-item])>section')
            ? el.querySelector(':scope>div') : null;
          if (!own && !card?.matches(rule.selectorText)) continue;
          // The section owns the row and its source; only the shadow's paint target moves.
          // A custom property is the look's own (`--exact-hover-wash`), not a row.
          for (const prop of rule.style) if (!prop.startsWith("--") && (own || prop === "box-shadow")) rows.push({ name: prop.replace(/^border-(top|right|bottom|left)-color$/, "border_color_$1").replace(/-/g, "_"), value: rule.style.getPropertyValue(prop).trim(), prop, target: own ? el : card });
        }
      }
      // A row a token or a role writes (`var(…)`, `light-dark(…)`) says the value it resolves to here, and the token
      // where modes.js names it; a token reference (`var(--x-…)`) it can't name leaves the answer
      // `provenance: "incomplete"` (a literal pair or a host's own property names none to miss).
      let incomplete = false;
      const cs = getComputedStyle(el);
      for (const row of rows) {
        const prop = row.prop;
        const target = row.target ?? el;
        delete row.prop;
        delete row.target;
        if (target !== el) row.computed = getComputedStyle(target).getPropertyValue(prop).trim();
        if (!/\b(var|light-dark)\(/.test(row.value)) continue;
        row.computed ??= cs.getPropertyValue(prop).trim();
        const token = globalThis.exact.modes?.token?.(el, prop, row.value);
        if (token) row.token = token;
        // Each token reference on its own: one modes.js can't name leaves the answer incomplete, whatever else it named.
        const refs = new Set([...row.value.matchAll(/var\((--x-[\w-]+)/g)].map(m => m[1]));
        if ([...refs].some(ref => !globalThis.exact.modes?.token?.(el, prop, `var(${ref})`))) incomplete = true;
      }
      const site = el.dataset?.site != null ? Number(el.dataset.site) : null;
      // (Its instance path is the inspect table's, where the page has one: `exact.instanceOf`, inspect-dev.js.)
      const instance = site != null ? globalThis.exact.instanceOf?.(el) ?? null : null;
      const answer = { held: { state: held.state, node: view, site, ...(instance ? { instance } : {}), rows, ...(incomplete ? { provenance: "incomplete" } : {}) } };
      // (The element, unsaid in JSON: a host that numbers its views another way names it from this.)
      Object.defineProperty(answer.held, "el", { value: el, enumerable: false });
      return answer;
    };
    // Real input on the held node: a press, a key, the pointer coming over it (a hover's), the focus (a focus's).
    for (const type of ["pointerdown", "keydown"]) root.addEventListener(type, e => { if (held?.el.contains(e.target)) letGo("input"); }, true);
    // The pointer or the focus coming or going: its leaving the node, not moving within it.
    const crosses = (e) => held.el.contains(e.target) !== held.el.contains(e.relatedTarget);
    for (const type of ["pointerover", "pointerout"]) root.addEventListener(type, e => { if (held?.state === "hover" && crosses(e)) letGo("input"); }, true);
    for (const type of ["focusin", "focusout"]) root.addEventListener(type, e => { if (held?.state === "focus-visible" && crosses(e)) letGo("input"); }, true);
  }
  // Paint looks snap; a scale look uses the author's transition and its transformed hit box. (A hold snaps through
  // them too, `stillThrough`.)
  if ((STATE_LOOKS || AGENT_ADMITTED) && !document.getElementById("exact-snap")) document.head.append(Object.assign(document.createElement("style"), { id: "exact-snap", textContent: "[data-exact-snap]{transition:var(--exact-look-transition,none)!important}" }));
  // A JS-target look's states, by its attribute's value, from its rules; read again when a sheet comes or goes.
  const lookStates = new Map();
  if (STATE_LOOKS || AGENT_ADMITTED) new MutationObserver(() => lookStates.clear()).observe(document.head, { childList: true, subtree: true, characterData: true });
  const looks = (el, sheet) => {
    if (el.dataset.view != null) { const css = sheet?.exactRules?.get(Number(el.dataset.view)); if (css) return css; }
    const at = el.getAttribute("data-exact-looks");
    if (at == null) return "";
    if (!lookStates.has(at)) {
      let text = "";
      const sel = `[data-exact-looks="${at}"]`, walk = (rules) => { for (const r of rules) { if (r.selectorText?.includes(sel)) text += r.cssText; if (r.cssRules) walk(r.cssRules); } };
      for (const s of document.styleSheets) { try { walk(s.cssRules); } catch { /* another origin's */ } }
      if (!text) return "";
      lookStates.set(at, text);
    }
    return lookStates.get(at);
  };
  const stillThrough = (els, change) => {
    if (!els.length) return;
    // A grouped section's state shadow paints on its card (looks.rs).
    els = [...new Set(els.flatMap(el => el.matches(':is([data-liststyle],[data-liststyle]>[data-exact-list-item])>section')
      ? [el, ...el.querySelectorAll(':scope>div')] : [el]))];
    const kept = els.map(el => {
      if (!/(?:^|[;{])\s*(?:--exact-)?scale\s*:/.test(looks(el, document.getElementById("exact-looks")))) {
        el.setAttribute("data-exact-snap", "");
        return [el, null, ""];
      }
      const style = getComputedStyle(el), split = text => text.split(/,(?![^(]*\))/).map(x => x.trim());
      const props = split(style.transitionProperty), durations = split(style.transitionDuration);
      const curves = split(style.transitionTimingFunction), delays = split(style.transitionDelay);
      const rows = ["scale", "--exact-scale"].map(prop => {
        const i = props.findLastIndex(p => p === prop || p === "all");
        return i < 0 ? `${prop} 0s` : `${prop} ${durations[i % durations.length]} ${curves[i % curves.length]} ${delays[i % delays.length]}`;
      });
      const old = el.style.getPropertyValue("--exact-look-transition"), priority = el.style.getPropertyPriority("--exact-look-transition");
      el.style.setProperty("--exact-look-transition", rows.join(","));
      el.setAttribute("data-exact-snap", "");
      return [el, old, priority];
    });
    change?.();
    for (const el of els) getComputedStyle(el).opacity;
    for (const [el, old, priority] of kept) {
      el.removeAttribute("data-exact-snap");
      if (old === null) continue;
      if (old) el.style.setProperty("--exact-look-transition", old, priority);
      else el.style.removeProperty("--exact-look-transition");
    }
  };
  // In the page: under `root`, through any shadow root's host.
  const inPage = (el) => { while (el && !root.contains(el)) el = el.getRootNode?.().host; return !!el; };
  // The pointer last used, as `data-exact-input` on the page, which every hover look reads (neutral.css, looks.rs): a
  // finger's tap leaves the browser's `:hover` on what it touched, which no platform shows. Writing the mark restyles
  // the whole page, so a finger's is written as it lifts, before the `:hover` its tap leaves, and not as it lands,
  // where the restyle would hold up the scroll the finger may start. The page starts with its primary pointer's.
  const canHover = STATE_LOOKS && matchMedia("(any-hover: hover)"), html = document.documentElement;
  html.dataset.exactInput ??= matchMedia("(pointer: coarse)").matches ? "touch" : "mouse";
  const touching = () => html.dataset.exactInput === "touch";
  const used = e => { if (html.dataset.exactInput !== e.pointerType) html.dataset.exactInput = e.pointerType; };
  for (const type of ["pointerover", "pointerdown", "pointermove"]) addEventListener(type, e => { if (e.pointerType !== "touch") used(e); }, true);
  for (const type of ["pointerup", "pointercancel"]) addEventListener(type, e => { if (e.pointerType === "touch") used(e); }, true);
  // The focused node's `:focus-visible` as last seen: a focus look's edge is its flipping. (`:hover` can't be read
  // so: a browser moves it for the style it computes next, so an edge is the pointer crossing a node's border.)
  const focusSeen = new WeakMap();
  // The edges pointer events crossed whose compatibility mouse events are still owed (they come after: a pointer's
  // out and over, then the mouse's); a mouse event alone, Firefox's for a scroll under a still pointer, is its own.
  const pointerEdges = [];
  const elementOf = (n) => (n instanceof Element ? n : n?.parentElement ?? null);
  // @ref LLP 1115.000 D6 — under a mouse or a pen a hover wash shows at once and fades out over 0.15 s as the pointer
  // leaves, as a Mac's hover highlight does. Every engine has dropped `:hover` by `pointerout` (Firefox and WebKit
  // have restyled the node by then, so no transition can start there), so the wash's color (`--exact-wash`,
  // neutral.css) is read as the pointer comes over, and the fade is the node's ground easing from it to nothing: the
  // same pixels on a node with no fill of its own, which is the only one it plays on. These snap instead: an author's
  // hover look (LLP 1121 D8), a grouped row's wash (it lies over the row's separator), a node with a fill of its own,
  // and anything under the agent's clock.
  const washes = new WeakMap(), fadesOut = new WeakMap();
  // `<html>` holds the registered property's initial value, which is no wash.
  let noWash;
  const hovered = e => { const el = e.pointerType !== "touch" && elementOf(e.target)?.closest('button, a[data-view][href], [data-exact-on~="press"]'); return el && root.contains(el) ? el : null; };
  addEventListener("pointerover", e => {
    const el = hovered(e);
    if (!el) return;
    fadesOut.get(el)?.cancel();
    const s = getComputedStyle(el), wash = s.getPropertyValue("--exact-wash");
    noWash ??= getComputedStyle(html).getPropertyValue("--exact-wash");
    const shows = s.backgroundImage !== "none" && s.getPropertyValue("--exact-hover-wash").trim() !== "none" && wash !== noWash;
    washes.set(el, shows && !el.matches('[data-groupedrowseparator], [data-exact-said~="background-color"]') ? wash : null);
  }, true);
  addEventListener("pointerout", e => {
    const el = hovered(e), wash = el && washes.get(el);
    if (!wash || el.contains(elementOf(e.relatedTarget))) return;
    washes.delete(el);
    // Pressed, or kept pressed while its popover is open (menu-rows.js): the press paints over any wash.
    if (agentMode || el.matches("[data-pressed], [data-exact-open]") || !el.isConnected) return;
    const ground = getComputedStyle(el).backgroundColor;
    if (ground !== "rgba(0, 0, 0, 0)") return; // a fill of its own
    fadesOut.set(el, el.animate([{ backgroundColor: wash }, { backgroundColor: ground }], { duration: 150, easing: "ease-out" }));
  }, true);
  // In a menu the pointer and the keyboard move one highlight (neutral.css), as NSMenu's: the row under a mouse or a pen
  // takes the focus, which the arrows move on from, and the pointer leaving the rows gives it back to what held it
  // before (the menu's button, where Escape leaves it too), so the arrows enter the menu again from its ends. What
  // held it is noted as the focus comes into the menu, by the pointer or by the arrows.
  if (ROVING) {
    const ROW = 'button, a[href], [role^="menuitem"]';
    let focusBefore = null;
    root.addEventListener("focusin", e => { const menu = e.target.closest?.(MENUS); if (menu && !menu.contains(e.relatedTarget)) focusBefore = e.relatedTarget; }, true);
    root.addEventListener("pointerover", e => {
      const row = e.pointerType !== "touch" && elementOf(e.target)?.closest(ROW), menu = row && row.closest(ROVES);
      if (!menu || menu !== topMenu(root) || !radioUsable(row) || document.activeElement === row) return;
      row.focus({ preventScroll: true, focusVisible: false });
    }, true);
    root.addEventListener("pointerout", e => {
      const row = e.pointerType !== "touch" && elementOf(e.target)?.closest(ROW), menu = row && row.closest(MENUS);
      if (!menu || row !== document.activeElement || elementOf(e.relatedTarget)?.closest(ROW)?.closest(MENUS) === menu) return;
      if (focusBefore?.isConnected && focusBefore !== document.body) focusBefore.focus({ preventScroll: true, focusVisible: false });
      else row.blur();
      focusBefore = null;
    }, true);
  }
  if (STATE_LOOKS) {
    const deepFocus = () => { let el = document.activeElement; while (el?.shadowRoot?.activeElement) el = el.shadowRoot.activeElement; return el; };
    { const el = deepFocus(); if (el instanceof Element) focusSeen.set(el, el.matches(":focus-visible")); }
    const focusLook = (el) => el instanceof Element && inPage(el) && !el.matches(DISABLED) && looks(el, document.getElementById("exact-looks")).includes("focus-visible");
    const lookEdges = (e) => {
      const sheet = document.getElementById("exact-looks"), els = [];
      if (e.type === "focusin" || e.type === "focusout") {
        // `:focus-visible` is the focused node's own (inside a shadow root too). Coming in, it had none.
        const el = e.composedPath()[0];
        if (!(el instanceof Element) || !inPage(el)) return;
        const was = e.type === "focusin" ? false : focusSeen.get(el) ?? false, now = e.type === "focusin" && el.matches(":focus-visible");
        focusSeen.set(el, now);
        if (was !== now && focusLook(el)) els.push(el);
      } else {
        // (A mouse event's target can be a text node: its element's edge.)
        const over = e.type.endsWith("over"), target = elementOf(e.target), related = elementOf(e.relatedTarget);
        if (e.type.startsWith("pointer")) { pointerEdges.push({ over, target, related }); if (pointerEdges.length > 4) pointerEdges.shift(); }
        else {
          // Its pointer event's twin, taken once; any other mouse edge is its own (and no twin is still owed).
          const twin = pointerEdges.findIndex(p => p.over === over && p.target === target && p.related === related);
          if (twin >= 0) { pointerEdges.splice(0, twin + 1); return; }
          pointerEdges.length = 0;
        }
        // A hover look holds only where a pointer can hover and the last one used was no finger (looks.rs), on an
        // enabled node; entering or leaving it, not moving within it.
        if (!canHover.matches || touching()) return;
        for (let el = target; el instanceof Element && root.contains(el); el = el.parentElement) {
          if (el.contains(related)) break;
          if (!el.matches(DISABLED) && looks(el, sheet).includes(":hover")) els.push(el);
        }
      }
      stillThrough(els);
    };
    for (const type of ["pointerover", "pointerout", "mouseover", "mouseout", "focusin", "focusout"]) root.addEventListener(type, lookEdges, true);
    // A key turning `:focus-visible` on for the node already focused (a click, then a key): Blink and WebKit move it
    // before the key comes, others in its default action, so it is looked at now and in the frame's style change (at
    // the window's capture, before a shortcut can take the key).
    const keyTurned = () => {
      const el = deepFocus();
      if (!focusLook(el) || focusSeen.get(el) || !el.matches(":focus-visible")) return false;
      focusSeen.set(el, true);
      stillThrough([el]);
      return true;
    };
    addEventListener("keydown", () => { if (focusLook(deepFocus()) && !keyTurned()) requestAnimationFrame(keyTurned); }, true);
  }
  // Delay touch and pen feedback only where a scroll can move the target. A quick tap flashes on release;
  // a scroll cancels it before it shows (UIKit's `delaysContentTouches`, LLP 1061 D2).
  const TOUCH_DELAY = 150;
  // Only a person's scroll ends a touch's press: one that follows a touch or a pen the browser took (`pointercancel`)
  // or a wheel (a trackpad's momentum is wheels too), in a scroller holding what that input landed on, until its
  // scrolls rest for the delay. Any other scroll is the page moving itself (an app's `scrollTop`, scroll anchoring,
  // `scrollIntoView`), which, like UIKit's `setContentOffset`, cancels no touch: the press goes on, measured where its
  // target now is.
  let person = { until: -Infinity, from: null };
  const personInput = e => { person = { until: performance.now() + TOUCH_DELAY, from: e.target }; };
  // Whether `node` is `el` or inside it, through any shadow root's host.
  const holds = (el, node) => { while (node && node !== el) node = node.parentNode ?? node.host; return !!node; };
  addEventListener("pointercancel", e => { if (e.pointerType === "touch" || e.pointerType === "pen") personInput(e); }, true);
  addEventListener("wheel", personInput, { capture: true, passive: true });
  // It ends with its scroller's `scrollend`, which comes after the glide: what moves that scroller next is the page.
  document.addEventListener("scrollend", e => { if (holds(e.target, person.from)) person.until = -Infinity; }, { capture: true, passive: true });
  const scrolled = new WeakMap();
  const touchScroll = el => {
    const since = performance.now() - TOUCH_DELAY;
    let delay = false, moving = false;
    for (let node = el; node; node = node.parentNode ?? node.host) {
      if (!(node instanceof Element)) {
        continue;
      }
      const style = getComputedStyle(node);
      delay ||= /auto|scroll|overlay/.test(style.overflowY) && node.scrollHeight > node.clientHeight
        || /auto|scroll|overlay/.test(style.overflowX) && node.scrollWidth > node.clientWidth;
      moving ||= scrolled.get(node) > since;
      // Stop at a bar that stays in place while an outer scroller moves
      if (style.position === 'fixed' || style.position === 'sticky') {
        return { delay, moving };
      }
    }
    const page = document.scrollingElement;
    delay ||= page.scrollHeight > page.clientHeight || page.scrollWidth > page.clientWidth;
    moving ||= scrolled.get(document) > since;
    return { delay, moving };
  };
  document.addEventListener("scroll", e => {
    const now = performance.now();
    if (now <= person.until && holds(e.target, person.from)) {
      person.until = now + TOUCH_DELAY;
      scrolled.set(e.target, now);
      if (press?.touch && touchScroll(press.el).moving) return release();
    }
    if (!press || !holds(e.target, press.el)) return;
    press.box = boxOf(press.el);
    if (!press.delay) showPress(press.el, stillUnder(press), false, false);
  }, { capture: true, passive: true });
  const inside = (p, e) => { const [l, t, r, b] = p.box; return e.clientX >= l && e.clientX <= r && e.clientY >= t && e.clientY <= b; };
  // Whether the finger, where it last was, is over the target as the page last moved it.
  const stillUnder = p => inside(p, { clientX: p.at[0], clientY: p.at[1] });
  // Play the haptic when feedback appears, where `navigator.vibrate` is available (LLP 1077 D14)
  const begin = p => {
    // Discard delayed feedback if the target became unavailable before its timer fired
    if (unavailable(p.el)) {
      if (press === p) {
        press = null;
      }
      return false;
    }
    showPress(p.el, true);
    if (p.haptic) {
      navigator.vibrate?.(p.haptic);
    }
    return true;
  };
  // A tap still waiting out the touch delay begins and ends before its first frame
  const release = (now = false, lifted = false) => {
    const p = press;
    press = null;
    if (!p) return;
    if (p.delay) {
      clearTimeout(p.delay);
      if (!lifted || !begin(p)) return;
    }
    showPress(p.el, false, now, lifted);
  };
  root.addEventListener("pointerdown", e => {
    if (e.button !== 0 || !e.isPrimary) return;
    release(); // a press whose release never reached the page
    surface(e.target);
    // The innermost pressable takes the press under every pointer; the shell's dim reads this mark, not `:active`
    // (index.html). A field, or a link that is no view's, in a pressable takes it from it.
    const el = e.target.closest?.("[data-exact-on~=press],button,a[href],input,select,textarea");
    if (!el?.matches("[data-exact-on~=press],button,a[data-view][href]") || !root.contains(el) || el.closest(DISABLED)) return; // a box says `disabled` (LLP 1121 D8)
    const touch = e.pointerType === "touch" || e.pointerType === "pen";
    const scroll = touch && !agentMode ? touchScroll(el) : {};
    if (scroll.moving) return;
    const haptic = el.style.getPropertyValue("--exact-press-haptic").trim();
    const p = press = {
      el, id: e.pointerId, box: boxOf(el), at: [e.clientX, e.clientY], touch,
      haptic: haptic && haptic !== "none" ? (haptic === "selection" ? 5 : 12) : 0,
    };
    // The delay over, the press shows if its target is still under the finger: the page may have moved it away.
    if (scroll.delay) p.delay = setTimeout(() => { if (press !== p) return; p.delay = 0; if (stillUnder(p)) begin(p); }, TOUCH_DELAY);
    else begin(p);
  }, true);
  document.addEventListener("pointermove", e => {
    if (e.pointerId !== press?.id) return;
    // The button came up where this document did not hear it (outside the window).
    if (e.buttons === 0) return release();
    press.at = [e.clientX, e.clientY];
    // Inside the delay the browser decides: a scroll cancels the touch, a tap lifts it.
    if (press.delay) return;
    showPress(press.el, inside(press, e), false, false);
  }, true);
  // A touch belongs to the target it began on, as UIKit hit-tests a touch once: lifted outside that target's box as it
  // stands now (the app may have moved it from under the finger), it presses nothing, though the browser aims its
  // `click` at whatever is under the finger then. Only the click that lift makes is swallowed: a trusted one a pointer
  // made (`detail` above 0, where a key's or assistive technology's activation has 0), within a second, before any
  // other press or key. `exact.strayClick` says so to a listener that may have heard it first (menu-rows.js, which
  // defers a row's action).
  let strayUntil = -Infinity;
  const stray = e => e.isTrusted && e.detail > 0 && performance.now() <= strayUntil;
  (globalThis.exact ??= {}).strayClick = stray;
  addEventListener("click", e => {
    if (!stray(e)) return;
    strayUntil = -Infinity;
    e.preventDefault(); e.stopImmediatePropagation();
  }, true);
  for (const type of ["pointerdown", "keydown"]) addEventListener(type, () => { strayUntil = -Infinity; }, true);
  document.addEventListener("pointerup", e => {
    if (e.pointerId !== press?.id) return;
    press.box = boxOf(press.el);
    const lifted = inside(press, e);
    if (press.touch && !lifted) strayUntil = performance.now() + 1000;
    release(false, lifted);
  }, true);
  document.addEventListener("pointercancel", e => { if (e.pointerId === press?.id) release(); }, true);
  // Leaving the page, the window or the tab may take the release with it, as the pointer dispatch below assumes: the
  // press, a key's too, ends at once. A gesture that claims the contact (motion-glue's swipe) ends the press as it
  // claims it, as a pan cancels a touch.
  const leave = () => {
    release(true);
    keyHold?.(null, true);
    for (const [el, state] of presses) if (!heldPress(el)) endPress(el, state);
  };
  addEventListener("blur", e => { if (e.target === window) leave(); });
  addEventListener("pagehide", leave);
  document.addEventListener("visibilitychange", () => { if (document.hidden) leave(); });
  (globalThis.exact ??= {}).pressClaimed = id => { if (id === press?.id) release(); };
  // @ref LLP 1115.000 D4.2 — a route is a page, so the focus follows it, as a careful web app moves it when it
  // navigates. On a push (or a replace) it goes to the new route's header's heading, else to the route itself; on
  // Back, to the control whose press pushed the route that left (the row, the link, the button that opened a sheet),
  // else as on a push. A focus the commit set is left alone: an `autofocus` field, a `focus()` command, anything
  // now inside the route, or anywhere that is neither the pressed control nor where the focus was as the press began
  // (so a route that arrives with no press never takes the focus from where a person put it), but for one still in a
  // leaving element (`[data-exiting]`: a sheet sliding away or a presence view leaving), which is no one's. Another
  // tab's stack is no arrival (the focus stays on its tab), nor is the first sight of a page. Nothing scrolls, and the
  // ring is the browser's own `:focus-visible`: none after a pointer's press, one after a key's, and none before the
  // person has pressed a key or the pointer, as for a page's first route (`focusVisible` says which, where the engine
  // takes it). The stack is read from the attributes navigation.js projects from.
  let pressed = null, touched = false;
  const shown = new WeakMap(), origins = new WeakMap();
  // The press is noted as it begins (the pointer going down; Enter or Space, in the keys' listener above), before the
  // click any listener navigates by: a link to a route is followed from the document's capture phase (above, and the
  // JS target's router.js), so its commit arrives before a listener here would hear that click. A click with neither
  // before it (a script's, the agent's) is noted as it is heard.
  const note = (e, key) => { if (!root.contains(e.target)) return; touched = true; pressed = { el: e.target?.closest?.("button, a[href], [tabindex]") ?? null, focus: document.activeElement, key, at: performance.now() }; };
  document.addEventListener("pointerdown", e => note(e, false), true);
  document.addEventListener("click", e => { if (!pressed?.el?.contains?.(e.target) || performance.now() - pressed.at > 2000) note(e, e.detail === 0); }, true);
  // The press that moved the focus, if one did: one noted in the last two seconds.
  const recentPress = () => pressed && performance.now() - pressed.at < 2000 ? pressed : null;
  // Whether the person used the keyboard last: the press was a key's, or any key came after it (an Escape that closes
  // a sheet, a shortcut that opens a dialog).
  let keyedAt = -Infinity;
  addEventListener("keydown", () => { keyedAt = performance.now(); }, true);
  const byKey = by => by.key || keyedAt >= by.at;
  // A pointer's press that opens a dialog moves the focus into it with no ring, as it moves the focus to a route
  // (`arrive`) and as Chromium and Firefox decide; WebKit marks that focus `:focus-visible` after a touch. The shell
  // leaves the platform's ring off the marked node (index.html) until a key is pressed or the focus leaves it.
  let unringed = null;
  const ring = () => { unringed?.removeAttribute("data-exact-no-ring"); unringed = null; };
  addEventListener("keydown", ring, true);
  document.addEventListener("focusout", e => { if (e.target === unringed) ring(); }, true);
  document.addEventListener("focusin", e => {
    const into = e.target.closest?.("dialog[open]"), by = recentPress();
    if (!into || !root.contains(into) || into.contains(e.relatedTarget) || !by || byKey(by)) return;
    ring();
    unringed = e.target;
    unringed.setAttribute("data-exact-no-ring", "");
  }, true);
  const lands = el => !!el?.isConnected && !inertAncestor(el) && el.getClientRects().length > 0 && !el.matches(":disabled");
  const titleOf = route => viewChildren(route).find(c => c.localName === "header")?.querySelector("h1, h2, h3, h4, h5, h6, [role=heading]");
  // @ref LLP 1115.000 D4.5 — a modal route (a sheet) is a dialog to assistive technology: `role="dialog"` and
  // `aria-modal`, named by its header's heading, and the focus moves into it and returns on close as it does for any
  // route (`arrive`). The host's marks (`data-exact-dialog`: "named" when the name is its own too) are not the app's:
  // a role or a label the author wrote is left as written, and the agent's tree leaves the marks out (agent.js).
  const dialog = route => {
    const mark = route.getAttribute("data-exact-dialog"), modal = /^(modal|fullscreen)$/.test(route.getAttribute("navigationPresentation") ?? "") && (mark != null || !route.hasAttribute("role"));
    if (!modal) { if (mark != null) for (const name of ["data-exact-dialog", "role", "aria-modal", ...(mark ? ["aria-labelledby"] : [])]) route.removeAttribute(name); return; }
    const title = titleOf(route), names = !!title && !route.hasAttribute("aria-label") && (!!mark || !route.hasAttribute("aria-labelledby"));
    if (names) { title.id ||= `exact-title-${route.getAttribute("navigationKey")}`; if (route.getAttribute("aria-labelledby") !== title.id) route.setAttribute("aria-labelledby", title.id); }
    else if (mark) route.removeAttribute("aria-labelledby");
    if (mark !== (names ? "named" : "")) { route.setAttribute("data-exact-dialog", names ? "named" : ""); route.setAttribute("role", "dialog"); route.setAttribute("aria-modal", "true"); }
  };
  const arrive = () => {
    const navs = root.querySelectorAll("[navigationBack]");
    if (navs.length === 0) return;
    for (const route of root.querySelectorAll(":is([navigationPresentation], [data-exact-dialog]):not([data-exiting])")) {
      if (route.parentElement.closest("[navigationBack]")) dialog(route);
    }
    const active = document.activeElement, by = recentPress();
    let arrival;
    for (const nav of navs) {
      const stack = focusStack(nav), top = stack.at(-1);
      if (!top) { shown.delete(nav); continue; }
      const was = shown.get(nav);
      shown.set(nav, stack);
      if (!arrival && was && was[0] === stack[0] && was.at(-1) !== top && !inertAncestor(nav)) arrival = { top, stack, was };
    }
    if (!arrival) return;
    const { top, stack, was } = arrival;
    const back = was.includes(top);
    if (!back) origins.set(top, was.length === stack.length ? origins.get(was.at(-1)) : by?.el); // a replace keeps where its route came from
    pressed = null;
    if (active && active !== document.body && !active.closest("[data-exiting]") && (top.contains(active) || ![by?.el, by?.focus].includes(active))) return; // a focus a leaving element still holds (in a frame or a shadow root, where its blur does not reach) is no one's
    const origin = back ? origins.get(was[was.indexOf(top) + 1]) : null;
    const to = lands(origin) ? origin : titleOf(top) ?? top;
    if (!to.hasAttribute("tabindex") && to.tabIndex < 0) {
      if (to.exactAuthoredTabIndex === undefined) to.exactAuthoredTabIndex = null;
      to.tabIndex = -1;
      const blurred = e => {
        // A top-layer overlay can restore the previous focus only if it remains focusable.
        const overlay = e.relatedTarget?.closest?.("dialog:modal, :popover-open");
        if (overlay && !overlay.contains(to)) return;
        if (to.exactAuthoredTabIndex == null && to.getAttribute("tabindex") === "-1") to.removeAttribute("tabindex");
        to.removeEventListener("blur", blurred);
      };
      to.addEventListener("blur", blurred);
    }
    const focus = () => to.focus({ preventScroll: true, ...(by ? { focusVisible: byKey(by) } : touched ? {} : { focusVisible: false }) }); focus(); if (document.activeElement?.closest("[data-exiting]")) focus(); // from a frame in a shadow root Firefox lands the focus on the second call
  };
  return {
    arrive, // once the piece is made (the stack as it stands), then after each commit's tree, its focus commands and its `autofocus`
    ...(WASM_HOST && {
      // @ref LLP 1005 §3 — `pointerdown`/`pointerup`, DOM's own: the primary
      // button or a touch going down on the node, then up or cancelled (a
      // cancel is an up), bubbling to every enabled ancestor that hears either
      // (Charlie, 2026-10-15). The up is heard on the document, so it arrives
      // wherever the pointer lifts; a pointer capture would also retarget the
      // click there, a press the platforms do not make. LLP 1056 §3 stage 3 —
      // `pointermove`: a free pointer over the node (no button down), or the
      // held one anywhere, at most once a frame, the latest. Each carries the
      // `PointerEvent` record (`offsetX,offsetY,button,buttons,pressure,pointerType,
      // pointerId,` the modifiers held, from the content box): `fire(29, r)` is down, 30 up, 31 a
      // move. The JS target's pointer.js is the same rule.
      pointer(el, on, fire) {
        let held = null, owner = false, last = null, move = null, frame = 0, spent = false, downOf = null;
        const wants = kind => el.exactHandlers?.includes(kind);
        const record = (e, lifted = false) => pointerLine(el, e, lifted);
        const flush = () => {
          const m = move; move = null;
          if (m && wants("pointermove") && ready()) fire(31, record(m));
        };
        // A move goes out in its own event unless one went out since the last animation frame; then the latest goes
        // at the next frame. Chromium and Firefox deliver moves aligned to frames, so each goes out as it comes.
        const tick = () => { frame = 0; spent = move !== null; flush(); if (spent) frame = requestAnimationFrame(tick); };
        const moved = e => {
          last = e; move = e;
          if (!spent) { spent = true; flush(); }
          if (!frame) frame = requestAnimationFrame(tick);
        };
        // While held: the owner's moves, and every holder's latest point,
        // which a blur's up reports.
        const heldMove = e => { if (e.pointerId !== held) return; last = e; if (owner) moved(e); };
        // The end: an up or cancel anywhere in the document, or the pointer
        // leaving it (out of the window, into a frame) or the window losing
        // focus, which this document never hears the up from.
        const ends = [["pointerup", document], ["pointercancel", document], ["pointerout", document], ["blur", window]];
        const up = e => {
          if (e.type === "blur" ? e.target !== window : e.pointerId !== held || (e.type === "pointerout" && e.relatedTarget && e.relatedTarget.localName !== "iframe")) return;
          held = null; downOf = null;
          for (const [type, target] of ends) target.removeEventListener(type, up, target === document);
          document.removeEventListener("pointermove", heldMove, true);
          flush();
          // Never for a removed node, nor past a handler further in that
          // stopped this up (`exactStopped`, glue.js).
          if (wants("pointerup") && ready() && el.isConnected && !e.exactStopped) fire(30, record(e.type === "blur" ? last : e, true), e);
        };
        const disabled = () => el.matches(":disabled") || el.hasAttribute("disabled") || inertAncestor(el);
        // A free pointer over it: the innermost node hearing moves takes them;
        // one that hears only down or up lets them by to an ancestor that
        // hears them, as pointer.js does.
        on("pointermove", e => {
          if (e.exactPointerMover || !wants("pointermove")) return;
          e.exactPointerMover = el;
          if (held === null && e.buttons === 0 && !disabled()) moved(e);
        });
        // `hold`: the walk's first pass, which fixes the holders; then the down.
        return (e, hold) => {
          // Its down, unless a handler further in removed or disabled it, or
          // stopped this down (`exactStopped`, glue.js): it still holds the up and the moves.
          if (!hold) { if (downOf === e && el.isConnected && !disabled() && wants("pointerdown") && ready() && !e.exactStopped) fire(29, record(e), e); return; }
          // Any button, as the DOM's (studio diary R22); `buttons` says which.
          if (!e.isPrimary || held !== null || !el.isConnected || disabled()) return;
          // The innermost enabled pointer node holds its moves (the walk
          // reaches it first and marks the event); an ancestor holds only its up.
          owner = !e.exactPointerOwner;
          if (!owner && !wants("pointerdown") && !wants("pointerup")) return;
          e.exactPointerOwner = el;
          held = e.pointerId; last = e; downOf = e;
          for (const [type, target] of ends) target.addEventListener(type, up, target === document);
          document.addEventListener("pointermove", heldMove, true);
          flush();
        };
      },
      // Studio diary R22, R3, R19, R17: a `contextmenu` with its point (10),
      // `dblclick` (11), a `wheel` (37, its record; a prevented one does not
      // scroll), what a drag drops (38: its files, each minted a `doc:` handle,
      // documents-glue.js, its text and its link, drag-in.js), a drag's enter and leave (55, 56, documents-glue.js
      // `hearDrags`) and the window's `beforeunload` (36), as the JS target's
      // rt.js and files.js hear them. `fire(kind, line)` dispatches.
      mouse(el, kind, e, fire) {
        if (!ready()) return;
        if (kind === "beforeunload") return el.isConnected && fire(36, "");
        if (el.matches(":disabled") || inertAncestor(el)) return;
        if (kind === "wheel") return e.exactStopped || fire(37, `${pointerLine(el, e).split(",").slice(0, 2)},${e.deltaX},${e.deltaY},${e.deltaMode},${modifiers(e)}`);
        if (kind === "dragover") { if (takes(el, e)) { e.preventDefault(); e.dataTransfer.dropEffect = "copy"; } return; }
        if (kind === "dragenter" || kind === "dragleave") return fire(kind === "dragenter" ? 55 : 56, `${pointerLine(el, e).split(",").slice(0, 2)},${modifiers(e)}`);
        if (kind === "drop") {
          if (!takes(el, e)) return;
          e.preventDefault(); e.stopPropagation(); e.exactTaken = true;
          const at = `${pointerLine(el, e).split(",").slice(0, 2)},${modifiers(e)}`;
          const live = () => el.isConnected && !retiredViews.has(el);
          return track(receive(e, documents, (found, text, url) => live() && (fire(38, `${at}\n${JSON.stringify([found, text, url])}`), true), log));
        }
        // A field keeps the browser's own double-click, and a text field its edit menu (navigation.js `editMenu`); a control's input has no edit menu, so its node's `contextmenu` runs.
        if (kind === "contextmenu" ? editMenu(e.target) : e.target.closest("input,textarea,[contenteditable]")) return;
        e.preventDefault(); e.stopPropagation();
        fire(kind === "contextmenu" ? 10 : 11, kind === "contextmenu" ? pointerLine(el, e) : "");
      },
    }),
    pan(el, id, on) {
      if (!PANS) return () => {}; // (a plan with none asks for none: pieces.js `onPan`)
      // @ref LLP 1043.000 §3 D8: one coalesced action per display frame.
      // LLP 1057.001 §1: an inner swipe that is still deciding (its pointer
      // 'pending' in exact.contacts) goes first; the pan waits for its verdict,
      // and a pan that began cancels the click (rule 4).
      let contact = null, frame = 0, suppressClick = false;
      const contacts = () => (globalThis.exact ??= {}).contacts ??= new Map();
      const live = () => views.get(id) === el && !retiredViews.has(el) && ready() && !inertAncestor(el) && !el.closest(":disabled,[disabled='true']");
      // Under a nested press the pan captures nothing until it begins, so until
      // then the window hears the contact's moves and its end (capture phase),
      // wherever they happen: a release outside this node ends it here too.
      const watched = { pointermove: e => move(e, true), pointerup: e => up(e, true), pointercancel: e => { if (e.pointerId === contact?.pointer) cancelled(e); } };
      const watch = on => { for (const t in watched) (on ? addEventListener : removeEventListener)(t, watched[t], true); };
      // A swipe defers capture, so its waiting ancestor must hear an outside end too.
      const endDeferred = e => { if (contact?.deferred && contact.pointer === e.pointerId) cancel(); };
      const watchEnd = enabled => {
        for (const type of ['pointerup', 'pointercancel'])
          el.ownerDocument[enabled ? 'addEventListener' : 'removeEventListener'](type, endDeferred, true);
      };
      const drop = () => { if (contact?.watching) watch(false); watchEnd(false); contact = null; };
      const flush = () => {
        cancelAnimationFrame(frame); frame = 0;
        if (!contact || !live()) { drop(); return; }
        const [x,y] = contact.to, [px,py] = contact.from;
        // exact_motion::gesture::SLOP; a pan-only plan links no motion export to ask.
        if (!contact.active && Math.max(Math.abs(x-px),Math.abs(y-py)) <= 4) return;
        if (!contact.active) release(); // a pan ends a press, as it cancels a touch
        // The button's tap is over: the drag is the pan's, captured, so this node hears the rest.
        if (contact.watching) { watch(false); contact.watching = false; el.setPointerCapture(contact.pointer); }
        contact.active = true; contact.from = [x,y];
        if (x !== px || y !== py) dispatch(id, `${x-px},${y-py}`);
      };
      const waiting = e => {
        if (!contact.deferred) return false;
        const state = contacts().get(e.pointerId);
        if (state === "pending") return true;
        if (state === "claimed") { drop(); return true; }
        contact.deferred = false; watchEnd(false); el.setPointerCapture(e.pointerId); return false;
      };
      // @ref LLP 1057 §10.6 — a pan that began ends with one `panrelease`:
      // the engine's tracker over the contact's samples, at each event's own
      // timestamp (motion-glue's `pan`); a cancelled contact releases at rest.
      const sample = (e, first = false) => velocity.sample?.(id, e.clientX, e.clientY, e.timeStamp, first);
      const released = (payload) => { if (el.exactHandlers?.includes("panrelease") && live()) dispatchRelease(id, payload); };
      const cancel = () => { cancelAnimationFrame(frame); frame=0; const began = contact?.active; drop(); if (began) released("0,0"); };
      // The browser took the contact (pointercancel): it scrolls or zooms by a
      // finger this node does not claim. Silent, it was a drag that died after
      // two moves (files diary F10); the journal says why and what claims it.
      const cancelled = e => {
        if (contact?.pointer === e.pointerId) log(`pan cancelled: the browser took the ${e.pointerType || "pointer"} contact to scroll or zoom; give the dragged node touch-action="none", or "pan-y" or "pan-x" to leave the browser the other axis`);
        cancel();
      };
      // A watched contact is the window's alone (`watched`); this node's listeners take it once captured.
      const move = (e, outside = false) => {
        if (contact?.pointer !== e.pointerId || !!contact.watching !== outside) return;
        if (e.buttons === 0 && e.pointerType !== "touch") return cancel(); // its button came up where no one heard it
        if (waiting(e)) return;
        sample(e); contact.to=[e.clientX,e.clientY]; if (!frame) frame=requestAnimationFrame(flush);
      };
      const up = (e, outside = false) => {
        if (contact?.pointer === e.pointerId && !!contact.watching !== outside) return;
        if (contact?.pointer !== e.pointerId) return;
        if (contact.deferred) { cancel(); return; }
        sample(e); contact.to=[e.clientX,e.clientY]; flush(); suppressClick = !!contact?.active;
        const began = contact?.active; drop();
        if (began) { const [vx, vy] = velocity.velocity?.(id, e.timeStamp) ?? [0, 0]; released(`${vx},${vy}`); }
      };
      on("pointermove", e => move(e));
      on("pointerup", e => up(e));
      on("pointercancel", cancelled);
      // A child's implicit touch capture, lost when a deferred pan takes it, bubbles here.
      on("lostpointercapture", e => { if (e.target === el) cancel(); });
      el.addEventListener("click", e => { if (suppressClick) { suppressClick = false; e.preventDefault(); e.stopImmediatePropagation(); } }, true);
      // A drag the browser begins at the down's point of the pan's contact is the contact's, unless the most recent
      // trusted primary down the pan heard is another pointer's at that point, as each engine gives `dragstart` the point
      // of the down it began from. By a mouse it is
      // cancelled, since the innermost takes a mouse's drag (LLP 1094.000 D8) and the pan took the contact from any
      // draggable node inside it that drags; a mark on the pan's own element loses to the pan, as on one node. By a pen
      // or a finger it is cancelled only under a nested press with no draggable node around the drag's element. Another
      // script's draggable inside the pan keeps its own drag.
      const at = (point, e) => Math.max(Math.abs(e.clientX - point[0]), Math.abs(e.clientY - point[1])) <= 1;
      // The last trusted primary down the pan heard, its contact's or one it declined
      let latest = null;
      el.addEventListener("dragstart", e => {
        if (e.isTrusted === false || !contact || !at(contact.down, e) || (latest.pointer !== contact.pointer && at(latest.down, e))) return;
        // The mark of the element the browser began the drag at, an SVG element's too, where the engine honored it
        const source = e.target.closest?.('[draggable="true" i]');
        if (source && source !== el && source.exactDraggable !== "true" && el.contains(source)) return;
        if (contact.mouse || (contact.nested && !source)) e.preventDefault();
      });
      const start = e => {
        // A down a script dispatches is no contact: it changes nothing, as the guest frame's scripted tap pans nothing.
        if (e.isTrusted === false) return;
        if (e.isPrimary && e.button === 0) latest = { pointer: e.pointerId, down: [e.clientX, e.clientY] };
        // Only the click right after a pan is suppressed; a drag makes none.
        suppressClick = false;
        if (!live() || !e.isPrimary || e.button !== 0 || contact || e.exactPan || el.matches("input,textarea,[contenteditable]")) return;
        // A control or editor between the contact and this node keeps it (rule 3).
        const inner = e.target.closest("input,textarea,select,[contenteditable]");
        if (inner && inner !== el && el.contains(inner)) return;
        // By a mouse the innermost draggable node that drags takes the contact (LLP 1094.000 D8): another script's
        // draggable, whatever is inside it, or the app's node when it carries something and no press inside it keeps it.
        const source = e.pointerType === "mouse" && draggableAround(e.target);
        const theirs = source && source.exactDraggable !== "true";
        const carries = source && !theirs && ["text", "url", "files"].some(k => source.hasAttribute(`data-datatransfer${k}`));
        const kept = carries && e.target.closest("button,a[href],[data-exact-on~='press']");
        if ((theirs || carries) && source !== el && el.contains(source) && !(kept && kept !== source && source.contains(kept))) return;
        // A press handler between keeps it only within the slop, as a draggable
        // element hears a drag that starts on a button inside it (kanban F6):
        // nothing is captured or prevented until the pan begins, so a tap stays the button's.
        const press = e.target.closest("button,a[href],[data-exact-on~='press']"), nested = !!press && press !== el && el.contains(press);
        const deferred = contacts().get(e.pointerId) === "pending";
        e.exactPan = true; // the innermost pan takes the contact (rule 3)
        if (!nested) { e.preventDefault(); e.stopPropagation(); if (!deferred) el.setPointerCapture(e.pointerId); }
        contact = {pointer:e.pointerId,down:[e.clientX,e.clientY],from:[e.clientX,e.clientY],to:[e.clientX,e.clientY],active:false,deferred,nested,watching:nested,mouse:e.pointerType === "mouse"};
        if (deferred) watchEnd(true);
        if (nested) watch(true);
        velocity.sample?.(id, e.clientX, e.clientY, e.timeStamp, true);
      };
      // A pan that retires mid-contact lets it go at once, its window and document listeners with it, since the contact
      // may end where this window does not hear it. It dispatches nothing: its node is going.
      start.retire = () => {
        cancelAnimationFrame(frame); frame = 0;
        if (contact && el.hasPointerCapture?.(contact.pointer)) el.releasePointerCapture(contact.pointer);
        drop();
      };
      return start;
    },
  };
}

// The visible Back that stands for a hidden route control. The hiding mark belongs only to bar items;
// author-hidden controls keep their styles. Input uses only the selected route; geometry can read a covered route.
export function backButton(el, selectedOnly = true) {
  if (!el?.id || el.checkVisibility?.({ visibilityProperty: true }) !== false) return null;
  const route = el.closest("[navigationkey]"), nav = route?.parentElement?.closest("[navigationback]");
  if (!nav || nav.getAttribute("navigationBack") !== el.id || (selectedOnly && nav.getAttribute("navigationKey") !== route.getAttribute("navigationKey"))
    || route.querySelector(`[id="${CSS.escape(el.id)}"]`) !== el) return null;
  const back = viewChildren(route).find(c => c.localName === "header")?.shadowRoot?.querySelector('button[part="back"]:not([hidden])');
  return back?.checkVisibility({ visibilityProperty: true }) ? back : null;
}
if (globalThis.exact) Object.assign(globalThis.exact, { createInputHandlers, backButton });
