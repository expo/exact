// LLP 1021, exact2's menu rule (theme2 2026-10-21 19:47, 22:05): a popover a
// row of a menu (`role="menu"`) opens, other than the menu itself or a
// submenu (itself a menu), is opened as if by the menu's own invoker
// (`showPopover({ source })`): HTML would nest it in the menu, and the menu
// closing as its row is chosen would take it along. The menu's invoker is
// what opened it: the button, or the node a context menu opened on, as the
// menu's `beforetoggle` said (navigation.js `popoverSource`). A popover
// written inside the menu stays nested by where it is (HTML), whatever its
// source. It also closes a popover whose control left the screen, and marks
// the button that opened a popover while the popover is open (below). Loaded
// after the first paint (glue.js); the JS target's page imports it at boot
// when its plan has a dialog, a menu or a popover (host/web-js/build.mjs
// `dialogs.flag`). A page rendered ahead loads either when it activates, so
// a menu may already be open.
import { inertAncestor, popoverSource } from "./navigation.js";

/** HTML's popover commands, and what each does. */
const POPOVER_COMMANDS = new Map([["show-popover", "show"], ["hide-popover", "hide"], ["toggle-popover", "toggle"]]);

/** The popover `row` invokes, as HTML resolves it, with what to do: by `commandfor` when it names an
 * element (a popover command on an HTML element; its `command` lowercased), else by `popovertarget`. None
 * for anything else (a dialog's command, a custom one, a button of a form that isn't `type="button"`),
 * which keeps HTML's own activation. Whether the target is a popover is read when it's acted on. */
function invoked(row) {
  if (row.form && row.getAttribute("type")?.toLowerCase() !== "button") return null;
  const commanded = document.getElementById(row.getAttribute("commandfor") || "");
  if (commanded) {
    const command = (row.getAttribute("command") ?? "").toLowerCase();
    const action = POPOVER_COMMANDS.get(command);
    return action && commanded instanceof HTMLElement ? { target: commanded, command, action } : null;
  }
  const target = document.getElementById(row.getAttribute("popovertarget") || "");
  const action = (row.getAttribute("popovertargetaction") ?? "").toLowerCase();
  return target?.hasAttribute("popover") ? { target, command: null, action: ["show", "hide"].includes(action) ? action : "toggle" } : null;
}

/** What opened `menu`, past any submenu: the button or node the outermost menu came from. */
function opener(menu) {
  let source = popoverSource(menu);
  for (let seen = new Set([menu]), outer; source && (outer = source.closest('[popover][role="menu"]')) && !seen.has(outer); seen.add(outer)) source = popoverSource(outer);
  return source;
}

/** Does what `row` invokes now (a popover's command, as at the click if it names none now), as HTML would,
 * but opens a popover that is no menu from its menu's own invoker. */
function act(row, menu, clicked) {
  if (!row.isConnected || row.matches(":disabled") || row.closest('[popover][role="menu"]') !== menu) return;
  const { target, command, action } = invoked(row) ?? clicked;
  try {
    // A command's event first, which its listeners may cancel (HTML's `command` event); whether the
    // target is a popover, and there, is read after it.
    if (command !== null && typeof CommandEvent === "function" && !target.dispatchEvent(new CommandEvent("command", { command, source: row, cancelable: true }))) return;
    if (!target.isConnected || !target.hasAttribute("popover")) return;
    const open = target.matches(":popover-open");
    if (action === "hide" || action === "toggle" && open) { if (open) target.hidePopover(); return; }
    if (open) return;
    const nested = target === menu || target.getAttribute("role") === "menu"; // a submenu, nested as HTML nests it
    const source = nested ? row : opener(menu);
    target.showPopover({ source: source?.isConnected ? source : undefined });
  } catch {}
}

/** Installs the rule on the document, once. */
function menuRows() {
  if (globalThis.exactMenuRows) return;
  globalThis.exactMenuRows = true;
  addEventListener("click", e => {
    if (globalThis.exact?.strayClick?.(e)) return; // the input glue swallows it: a lift off the target the touch began on
    const row = e.target.closest?.("button[popovertarget], button[commandfor]");
    const menu = row?.closest('[popover][role="menu"]');
    // Only a row that opens a popover at the click: every other keeps HTML's own activation (a press
    // retargeting a row between a popover and anything else acts on what it named at the click).
    // And one that would close its popover (a hide, a toggle of an open one) nests nothing: HTML's too.
    const clicked = menu ? invoked(row) : null;
    if (!clicked || clicked.action === "hide" || clicked.action === "toggle" && clicked.target.matches(":popover-open")) return;
    // HTML would act on what the row names before its own press; this acts after it, on the popover it names then.
    e.preventDefault();
    setTimeout(() => act(row, menu, clicked));
  }, true);
  // An open popover belongs to the screen of the control that opened it (its `beforetoggle` source, navigation.js
  // `popoverSource`), as UIKit dismisses a menu with the screen that presented it: when that control leaves the page
  // or is no longer shown (its route popped, or switched away from), the popover closes. HTML ties a popover's life to
  // its own element only, so this is the one place that says it; while any is open, each frame looks. A popover
  // removed while open is dismissed there too, since HTML's removing steps hide it with no `toggle`.
  const opened = new Map(); // popover -> the control that opened it
  const focus = new WeakMap();
  let focused = document.activeElement;
  addEventListener("focusin", e => { focused = e.target; }, true);
  const usable = el => el?.isConnected && !el.matches(':disabled, [aria-disabled="true"]')
    && !inertAncestor(el) && el.checkVisibility({ visibilityProperty: true });
  addEventListener("beforetoggle", e => {
    const menu = e.target;
    if (!menu.matches('[popover][role="menu"]')) return;
    if (e.newState === "open") focus.set(menu, { before: document.activeElement, source: e.source });
    else {
      const state = focus.get(menu);
      if (state) state.restore = menu.contains(document.activeElement);
    }
  }, true);
  const menuFocus = (menu, open) => {
    const state = focus.get(menu);
    if (!state) return;
    const at = document.activeElement;
    if (open) {
      // An authored autofocus or focus handler owns the focus it set while the menu opened.
      if (menu.contains(at) || ![document.body, state.before, state.source].includes(at)) return;
      const first = [...menu.querySelectorAll('button, a[href], [role^="menuitem"]')]
        .find(el => el.closest('[role="menu"]') === menu && usable(el));
      if (first) first.focus({ preventScroll: true });
      else {
        state.tabindex = menu.getAttribute("tabindex");
        if (state.tabindex === null) menu.tabIndex = -1;
        menu.focus({ preventScroll: true });
      }
    } else {
      focus.delete(menu);
      if (state.tabindex === null) menu.removeAttribute("tabindex");
      // Light dismissal can already have focused another control; leave that focus where it landed.
      if (!state.restore || at !== document.body && !menu.contains(at)) return;
      for (const target of [state.source, state.before]) {
        if (!usable(target)) continue;
        target.focus({ preventScroll: true });
        // A context menu's source may be visible without being focusable.
        if (document.activeElement !== at) break;
      }
    }
  };
  // While its popover is open a button stays pressed (neutral.css reads the mark as it reads a press); not for a
  // context menu, which the Mac opens on a button without highlighting it. A popover hidden and shown again from
  // another button in one task fires a single `toggle`, open to open, which moves the mark; a button two open popovers
  // marked keeps it until both close.
  const marks = (pop, source) => source.matches("button") && !pop.matches('[style*="--exact-context-x"]');
  const forget = pop => {
    const source = opened.get(pop);
    opened.delete(pop);
    if (source && ![...opened].some(([p, s]) => s === source && marks(p, s))) source.removeAttribute("data-exact-open");
  };
  // A route switched away from is `visibility: hidden` (navigation.js), so visibility counts as not shown.
  let sweeping = false;
  const sweep = () => {
    for (const [pop, source] of opened) {
      if (!pop.isConnected || !pop.matches(":popover-open")) {
        const state = focus.get(pop);
        // Removal resets activeElement to body before the sweep can read it.
        if (state) state.restore ??= pop.contains(focused);
        menuFocus(pop, false);
        forget(pop);
      } else if (source && !source.checkVisibility({ visibilityProperty: true })) pop.hidePopover(); // its `toggle` forgets it
    }
    sweeping = opened.size > 0;
    if (sweeping) requestAnimationFrame(sweep);
  };
  const toggled = (pop, open, source = popoverSource(pop)) => {
    menuFocus(pop, open);
    forget(pop);
    if (!open || !source && !focus.has(pop)) return;
    if (source && marks(pop, source)) source.setAttribute("data-exact-open", "");
    opened.set(pop, source);
    if (!sweeping) { sweeping = true; requestAnimationFrame(sweep); }
  };
  addEventListener("toggle", e => toggled(e.target, e.newState === "open"), true);
  // A page rendered ahead opens a popover before its runtime, and so this, has loaded (opening a menu starts the
  // runtime: capture.js, document-glue.js): its events went unheard. Its source is the focused control that names it,
  // and a menu takes the focus now if it is still where the menu opened.
  for (const pop of document.querySelectorAll("[popover]:popover-open")) {
    const at = document.activeElement ?? document.body;
    const source = popoverSource(pop) ?? (pop.id && [at.getAttribute("popovertarget"), at.getAttribute("commandfor")].includes(pop.id) ? at : null);
    if (pop.matches('[role="menu"]') && (at === document.body || at === source)) focus.set(pop, { before: at, source });
    toggled(pop, true, source);
  }
}

(globalThis.exact ??= {}).menuRows = menuRows;
