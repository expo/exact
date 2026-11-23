import { pop, liveById, root } from "./routes.js";
/** A context menu's popover, opened for the `contextmenu` event `e` (LLP 1021 §5.1): `find` answers `[popover, source]` when there is still one to open. It opens once the press that asked is over: a Mac's `contextmenu` comes with the button still down, and Firefox and WebKit take the release that follows, outside a popover just opened, for its light dismiss, so the menu never showed there. And it opens at the pointer (LLP 1115.000 D4.7): where the press was is left on the popover (`--exact-context-x`, `-y`) until it closes, for the page's sheet to place it by (neutral.css; the shell's own reads neither). A menu key has no point, and its menu opens at its row. */
export function contextAt(e, find) {
  const open = () => {
    const [p, source] = find() ?? [];
    if (!p) {
      return;
    }
    if (e.clientX || e.clientY) {
      p.style.setProperty("--exact-context-x", `${e.clientX}px`);
      p.style.setProperty("--exact-context-y", `${e.clientY}px`);
      p.exactAt ??= (p.addEventListener("toggle", (t) => {
        if (t.newState === "closed") {
          for (const axis of "xy") {
            p.style.removeProperty(`--exact-context-${axis}`);
          }
        }
      }), true);
    }
    try {
      p.showPopover({ source });
    } catch {}
  }, ends = [
    "pointerup",
    "pointercancel",
    "mouseup"
  ], end = () => {
    for (const type of ends) {
      removeEventListener(type, end, true);
    }
    setTimeout(open);
  };
  if (!e.buttons) {
    return void setTimeout(open);
  }
  for (const type of ends) {
    addEventListener(type, end, true);
  }
}
const closingDialogs = new WeakMap();
const closingMark = "data-exact-dialog-closing";

function clearDialogExit(dialog) {
  closingDialogs.delete(dialog);
  dialog.removeAttribute(closingMark);
}

function leaveDialog(dialog, returnValue) {
  if (closingDialogs.has(dialog)) return true;
  const exit = dialog.style.getPropertyValue("--exact-exit-animation").trim();
  if (!dialog.open || !exit || exit === "none" || matchMedia("(prefers-reduced-motion: reduce)").matches) return false;
  // Stop the entrance before starting the exit, even when both use the same named keyframes.
  dialog.setAttribute(closingMark, "start");
  dialog.getAnimations();
  dialog.setAttribute(closingMark, "play");
  const animations = dialog.getAnimations().filter(a => a.animationName !== undefined && a.effect?.target === dialog);
  if (!animations.length) { clearDialogExit(dialog); return false; }
  closingDialogs.set(dialog, animations);
  Promise.allSettled(animations.map(a => a.finished)).then(() => {
    if (closingDialogs.get(dialog) !== animations) return;
    clearDialogExit(dialog);
    if (dialog.open && dialog.isConnected && !dialog.closest("[data-exiting]")) dialog.close(returnValue);
  });
  return true;
}

function requestDialogExit(event) {
  if (event.cancelable && !event.defaultPrevented && leaveDialog(event.currentTarget)) event.preventDefault();
}

function dialogCommand(event) {
  if (event.defaultPrevented || !event.cancelable) return;
  if (event.command === "close" && leaveDialog(event.currentTarget, event.source?.getAttribute("value") ?? undefined)) event.preventDefault();
  if (event.command === "show-modal" && closingDialogs.has(event.currentTarget)) clearDialogExit(event.currentTarget);
}

// A dialog stays in the top layer during its authored exit. Its native close then returns focus and fires the event.
export function modalCommand(element, id, method, say) {
  if (element instanceof HTMLDialogElement) {
    if (method === "close") {
      if (!leaveDialog(element)) element.close();
    } else if (closingDialogs.has(element)) clearDialogExit(element);
    else if (!element.open) element.showModal();
    else say(`showModal "${id}" ignored: dialog is already open`);
  } else if (element?.popover != null && element.getAttribute("role") === "alertdialog") {
    if (method === "close") element.hidePopover();
    else if (!element.matches(":popover-open")) element.showPopover();
  } else if (method === "showModal") say(`showModal "${id}" refused: no dialog or alertdialog popover has that id`);
}

// HTML's methods and source, shared by both web targets (LLP 1021 §4).
export function popoverCommand(method, id, source, say) {
  const pop = liveById(id), anchor = source == null ? null : liveById(source);
  if (!pop?.hasAttribute("popover")) {
    return say(`${method} "${id}" refused: no popover has that id`);
  }
  if (source != null && !anchor) {
    return say(`${method} "${id}" refused: no source has id "${source}"`);
  }
  try {
    pop[method](method === "hidePopover" || !anchor ? undefined : { source: anchor });
  } catch (e) {
    say(`${method} "${id}" refused: ${e.name}`);
  }
}
/** LLP 1021 D2: a popover whose author wrote `left` or `top`, and no `position-area` (which CSS lets win, the insets then offsetting it in its cell), opens at that point, not at its invoker. Its insets are left on it as `--exact-inset-x`, `-y` while it is open (`auto` for an axis with none), and again as they change; written away while it is open, or overruled by a `position-area`, `right` or `bottom`, they go, and its invoker places it again. They are for the page's sheet to place it by and to flip it the other way from the point where it does not fit, as a context menu at the pointer (neutral.css). Only where that sheet's `inset: auto` holds its other sides is a `left` or `top` the author's: the user agent's `inset: 0` writes all four, as does an author's `right` or `bottom`. */
function insetAt(p, open) {
  const set = (axis, v) => {
    if (p.style.getPropertyValue(`--exact-inset-${axis}`) !== v) {
      v ? p.style.setProperty(`--exact-inset-${axis}`, v) : p.style.removeProperty(`--exact-inset-${axis}`);
    }
  }, has = (v) => !!v && v !== "auto", area = p.style.getPropertyValue("position-area");
  /* open, an anchored box's computed insets are where it was placed, so its author's are read from its own style; closed, the computed ones are what was written, the sheet's `inset: auto` among them */
  if (open) {
    if (!p.exactInsetOn) {
      return;
    }
    const { left, top, right, bottom } = p.style, on = !has(right) && !has(bottom) && (has(left) || has(top)) && !(area && area !== "none");
    set("x", on ? left || "auto" : "");
    set("y", on ? top || "auto" : "");
    return;
  }
  const { left, top, right, bottom } = getComputedStyle(p);
  p.exactInsetOn = !(right !== "auto" || bottom !== "auto" || left === "auto" && top === "auto" || area && area !== "none");
  set("x", p.exactInsetOn ? left : "");
  set("y", p.exactInsetOn ? top : "");
  p.exactInset ??= (p.addEventListener("toggle", (t) => {
    if (t.newState === "closed") {
      p.exactInsetOn = false;
      set("x", "");
      set("y", "");
    }
  }), new MutationObserver(() => {
    if (p.matches(":popover-open")) {
      insetAt(p, true);
    }
  }).observe(p, {
    attributes: true,
    attributeFilter: ["style"]
  }), true);
}
const popoverSources = new WeakMap();
/** What opened popover `pop` last (its `beforetoggle`'s source): a menu's invoker, for menu-rows.js. */ export const popoverSource = (pop) => popoverSources.get(pop) ?? null;
/** Keeps each popover's source as it opens, marks a chooser and places a popover opened at a point. The wasm host's
 * glue.js calls it, and a JS target page whose plan has a dialog, a menu or a popover, or which has a page module
 * (host/web-js/build.mjs): another page has no popover in its root, and its bundle drops this (LLP 1047.002 D1). Once
 * per page: a second call adds nothing. */
let placing = false;
export function placePopovers() {
  if (placing) return;
  placing = true;
  globalThis.document?.addEventListener?.("beforetoggle", (e) => {
    if (e.target.localName === "dialog") {
      if (e.newState === "open") {
        e.target.addEventListener("cancel", requestDialogExit);
        e.target.addEventListener("command", dialogCommand);
      } else if (closingDialogs.has(e.target)) clearDialogExit(e.target);
    }
    if (e.target.hasAttribute("popover") && e.newState === "open") {
      popoverSources.set(e.target, e.source ?? null);
      e.target.toggleAttribute("data-exact-chooser", !!e.source?.checkVisibility() && e.target.getAttribute("role") === "alertdialog" && !e.target.querySelector(":scope > [data-exact-text], input"));
      insetAt(e.target);
    }
    // a chooser (LLP 1021) its invoker opened, the invoker still shown: neutral.css draws it there, as a menu
  }, true);
}
function popoverObservation(root) {
  const modal = [...root.querySelectorAll("dialog:modal")].sort((a, b) => (a.exactOpened ?? 0) - (b.exactOpened ?? 0)).at(-1);
  const alert = modal?.getAttribute("role") === "alertdialog";
  const pop = alert ? modal : [...root.querySelectorAll("[popover]:popover-open")].at(-1), source = pop && popoverSources.get(pop);
  const id = (el) => globalThis.exact?.viewId?.(el) ?? Number(el.dataset.view);
  return pop ? {
    ...(alert ? { kind: "confirmation" } : {}),
    phase: "open",
    popover: id(pop),
    source: source ? id(source) : null
  } : null;
}
export { popoverObservation };
