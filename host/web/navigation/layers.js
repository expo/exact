const opening = [];
// an open dialog's or popover's `exactOpened` is when it opened, so the greatest is on top (input-glue.js's menu)
let order = 0, stamping = false;
/** Stamp each dialog and popover as it opens, from now on: a page that opens none installs nothing. The inner opening is
 * stamped, then its still-dispatching callers, innermost first; an outer target already open keeps its stamp. Where
 * two bundles each install one, only the first to hear an opening stamps it (`exactStamped`). */
export function stampOpenings() {
  if (stamping || !globalThis.addEventListener) return;
  stamping = true;
  addEventListener("beforetoggle", (e) => {
    for (let i = opening.length - 1; i >= 0; i--) {
      if (opening[i].eventPhase === Event.NONE) {
        opening.splice(i, 1);
      }
    }
    if (e.exactStamped || e.newState !== "open" || e.target.localName !== "dialog" && !e.target.hasAttribute("popover")) {
      return;
    }
    e.exactStamped = true;
    e.target.exactOpened = ++order;
    for (let i = opening.length - 1; i >= 0; i--) {
      const target = opening[i].target;
      if (!target.matches("dialog:modal,:popover-open")) {
        target.exactOpened = ++order;
      }
    }
    opening.push(e);
  }, true);
}
// Structural reads retain the front modal and presentations opened after it.
export function modalLayers() {
  const top = [...document.querySelectorAll("dialog:modal,:popover-open")];
  const modals = top.filter((el) => el.matches("dialog:modal"));
  const front = Math.max(0, ...modals.map((el) => el.exactOpened ?? 0));
  return modals.length ? top.filter((el) => (el.exactOpened ?? 0) >= front) : null;
}
// The nearest inert ancestor (a modal dialog ends the search: its subtree is live).
export function inertAncestor(el) {
  for (let node = el; node; node = node.parentElement) {
    if (node.hasAttribute("inert")) {
      return node;
    }
    if (node.localName === "dialog" && node.matches(":modal")) {
      return null;
    }
  }
  return null;
}
