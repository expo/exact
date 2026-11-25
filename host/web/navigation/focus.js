import { root, log, liveById } from "./routes.js";
import { inertAncestor } from "./layers.js";
import { setFieldSelection, setFieldRangeText } from "./controls.js";
// A keyboard shortcut or a captured press dispatches exactly one click with its input provenance.
const inputClicks = new WeakSet();
export const isInputClick = event => inputClicks.has(event);
export function clickFromInput(target, trusted) {
  if (!trusted || target.matches(':disabled')) return target.click();
  const click = new PointerEvent('click', { bubbles: true, cancelable: true, composed: true, pointerId: -1 });
  inputClicks.add(click);
  try { target.dispatchEvent(click); } finally { inputClicks.delete(click); }
}

export function focusController({ ready, elements, inert, removed = (el) => !el.isConnected }) {
  const processed = new WeakSet(), outgoing = new WeakMap();
  let pointerTarget = null, restarting = false;
  const autofocus = () => {
    if (restarting || !ready()) {
      return;
    }
    for (const el of elements()) {
      if (processed.has(el) || !el.exactAutofocus || !el.getClientRects().length || inert(el) || el.matches(":disabled") || getComputedStyle(el).visibility !== "visible") {
        continue;
      }
      processed.add(el);
      // Once per mount, including a refused autofocus.
      const active = document.activeElement;
      if (active && active !== document.body && active !== pointerTarget && !(active.matches("[data-gpu-input]") && active.contains(el))) {
        return;
      }
      el.setAttribute("autofocus", "");
      el.focus();
      return;
    }
  };
  // A carried restart (dev reload, delivered update) keeps focus at its place in the runner's tree — index among siblings at each level, and type — and autofocuses nothing it rebuilt.
  const keep = (tree, id) => {
    const nodes = new Map(tree.nodes?.map((n) => [n.id, n])), path = [];
    for (let n = nodes.get(id); n; n = nodes.get(n.parent)) {
      path.unshift((n.parent == null ? tree.roots : nodes.get(n.parent)?.children ?? []).indexOf(n.id));
    }
    return nodes.has(id) && !path.includes(-1) ? {
      path,
      type: nodes.get(id).type
    } : null;
  };
  const restart = (kept, apply, tree, view) => {
    if (kept === undefined) {
      return apply();
    }
    // a fresh boot autofocuses (LLP 1035.000 D9)
    try {
      restarting = true;
      apply();
    } finally {
      restarting = false;
      for (const el of elements()) {
        if (el.exactAutofocus) {
          processed.add(el);
        }
      }
    }
    const t = kept && tree(), nodes = new Map(t?.nodes?.map((n) => [n.id, n]));
    const [, id] = kept?.path.reduce(([ids], i) => [nodes.get(ids?.[i])?.children, ids?.[i]], [t.roots]) ?? [];
    const el = id != null && nodes.get(id)?.type === kept.type ? view(id) : null;
    if (el?.isConnected && el.getClientRects().length && !inert(el) && !el.matches(":disabled")) {
      el.focus({ preventScroll: true });
    }
  };
  return {
    autofocus,
    keep,
    restart,
    detached(el, event) {
      const pending = outgoing.get(event.target);
      if (!pending || pending.event && pending.event !== event) {
        return false;
      }
      pending.event = event;
      const held = pending.path.delete(el);
      if (!pending.path.size) {
        outgoing.delete(event.target);
      }
      return held;
    },
    blur(el, dispatch) {
      const path = [];
      for (let at = el; at; at = at.parentElement) {
        path.push(at);
      }
      dispatch();
      // Only the subtree detached by this blur hears the following DOM focusout.
      const pending = new Set();
      for (const at of path) {
        if (!removed(at)) {
          break;
        }
        if (at.exactHandlers?.includes("focusout")) {
          pending.add(at);
        }
      }
      if (pending.size) {
        outgoing.set(el, {
          path: pending,
          event: null
        });
      }
    },
    press(event, el, dispatch) {
      event.stopPropagation();
      const canvas = el.closest("[data-gpu-input]");
      const previous = pointerTarget;
      pointerTarget = event.detail > 0 ? el : null;
      try {
        dispatch();
        // Blur handlers must not retire the press target before dispatch.
        const removed = !el.isConnected && document.activeElement === document.body;
        if (el instanceof HTMLButtonElement && (pointerTarget && document.activeElement === el || removed) && el.getAttribute("role") !== "slider" && !el.hasAttribute("data-action")) {
          if (canvas?.isConnected) {
            event.preventDefault();
            canvas.focus({ preventScroll: true });
            pointerTarget = canvas;
          }
        }
        autofocus();
      } finally {
        pointerTarget = previous;
      }
    }
  };
}
// Field and focus commands run after the batch's nodes and values commit; their handlers may dispatch an action.
export function runFocusCommands(commands, { root, ready, inertAncestor, log }) {
  for (const { name, args } of commands) {
    if (name === "scrollIntoView") {
      // `Element.scrollIntoView()` by the element's id, after the batch's layout (minesweeper F3)
      const el = liveById(args?.[0], root);
      if (el) {
        el.scrollIntoView({
          block: args[1] ?? "start",
          inline: args[2] ?? "nearest",
          behavior: args[3] ?? "auto"
        });
      } else {
        log(`scrollIntoView "${args?.[0]}" refused: no live node with that id`);
      }
      continue;
    }
    if (name === "blur") {
      // `blur()` drops whatever holds focus; `blur(id)` only when that node holds it.
      const active = document.activeElement;
      if (ready && active && active !== document.body && (!args?.length || active.id === args[0])) {
        active.blur();
      }
      continue;
    }
    if (name === "removeAllRanges") {
      getSelection()?.removeAllRanges();
      continue;
    }
    if (name === "setSelectionRange") {
      setFieldSelection(liveById(args?.[0], root), args, log);
      continue;
    }
    if (name === "setRangeText") {
      setFieldRangeText(liveById(args?.[0], root), args, log);
      continue;
    }
    const selectText = name === "selectText";
    if (args?.length !== 1 || typeof args[0] !== "string" || !ready) {
      continue;
    }
    const el = liveById(args[0], root);
    const reason = !el ? "no live node with that id" : !el.isConnected ? "not mounted" : el.matches(":disabled,[disabled]") ? "disabled" : inertAncestor(el) ? "inert ancestor" : !el.getClientRects().length ? "zero size" : getComputedStyle(el).visibility !== "visible" ? "hidden ancestor" : null;
    if (reason) {
      log(`focus "${args[0]}" refused: ${reason}`);
      continue;
    }
    if (selectText && typeof el.select !== "function") {
      log(`selectText "${args[0]}" refused: not a text editor`);
      continue;
    }
    el.focus();
    if (selectText && document.activeElement === el) {
      el.select();
    }
  }
}
