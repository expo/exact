import { host, root, views, authoredDisabled, motion, focusAutofocus, messageFrames, retiredViews, writeIn, now, moduleReady, messageViews, focus, compositionFlush, composing, heldValues, markupPending, iframeLoading, frameNav, loadAfterPaint, log, documentsGlue, agentMode, picker, inflight, encoder, keyChord } from "../glue.js";
import { guestMessageAuthorized, scrollDrag, sourceField, controlEvent, onSelection, inertAncestor } from "../navigation.js";
import { send } from "./commit.js";
import { writeValue, syncMarkup, imageEvents } from "./views.js";
export function setInputReady(ready) {
  host.inputReady = ready;
  root.setAttribute("aria-busy", String(!ready));
  if (ready) {
    for (const el of views.values()) {
      if (authoredDisabled.has(el)) {
        el.disabled = authoredDisabled.get(el);
        authoredDisabled.delete(el);
      }
    }
  }
  if (ready) {
    motion.commit();
    focusAutofocus();
  }
}
function ensureMessageListener() {
  if (host.messageListening) {
    return;
  }
  host.messageListening = true;
  window.addEventListener("message", (event) => {
    for (const el of messageFrames) {
      if (event.source !== el.contentWindow) {
        continue;
      }
      if (!guestMessageAuthorized(el, event.origin)) {
        return;
      }
      let payload = event.data;
      if (typeof payload !== "string") {
        try {
          payload = JSON.stringify(payload);
        } catch {
          return;
        }
      }
      if (typeof payload !== "string") {
        return;
      }
      const id = Number(el.dataset.view);
      const go = () => {
        if (views.get(id) === el && !retiredViews.has(el) && messageFrames.has(el) && guestMessageAuthorized(el, event.origin)) {
          send(host.wasm.exact_dispatch(id, 9, writeIn(payload), now()));
        }
      };
      if (host.inputReady) {
        go();
      } else {
        moduleReady.then(() => host.inputReady && go());
      }
      return;
    }
  });
}
export function attach(el, id, handlers) {
  el.dataset.view = String(id);
  el.exactHandlers = handlers;
  if (handlers.length) {
    el.dataset.exactOn = handlers.join(" ");
  } else {
    delete el.dataset.exactOn;
  }
  el.exactFlowEvents = [...new Set(handlers.flatMap((k) => ({
    press: ["click"],
    hover: ["pointerenter", "pointerleave"],
    focus: ["focus"],
    blur: ["blur"],
    focusin: ["focusin"],
    focusout: ["focusout"],
    key: ["keydown"],
    keycapture: ["keydown"],
    keyup: ["keyup"]
  })[k] ?? []))];
  if (el.exactMedia) {
    el.exactMedia.handlers = handlers;
  }
  if (handlers.includes("message")) {
    messageViews.add(id);
  }
  const on = (event, handle, capture = false) => {
    const heard = (e) => {
      if ((views.get(id) === el && !retiredViews.has(el) || event === "focusout" && focus.detached(el, e)) && (host.inputReady || event === "load" || event === "error" && el instanceof HTMLIFrameElement) && !(host.page?.restoringFocus && [
        "focus",
        "blur",
        "focusin",
        "focusout"
      ].includes(event))) {
        handle(e);
      }
    };
    el.addEventListener(event, heard, capture);
    return heard;
  }, atBody = (kind, heard) => {
    (el.exactBody ??= {})[kind] = heard;
    host.bodyHeard ??= [
      "keydown",
      "keyup",
      "copy",
      "cut",
      "paste"
    ].map((t) => document.documentElement.addEventListener(t, (e) => {
      const h = root.firstElementChild?.exactBody;
      if (!h || e.target !== document.body && e.target !== document.documentElement) {
        return;
      }
      if (t === "keydown") {
        h.keycapture?.(e);
        h.key?.(e);
      } else {
        h[t]?.(e);
      }
    }));
  };
  // with nothing focused a key or a clipboard event targets `<body>`, outside the app, where the DOM's `window` listeners hear it: the first root's handlers hear any event whose target is `<body>` or `<html>` (never a node a flow forwards a copy to, detached), on `<html>` as it bubbles, so after the shortcuts and before the document's defaults, its `keycapture` before its `key` (rt.js `atBody`)
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    on("compositionstart", () => {
      clearTimeout(compositionFlush.get(el));
      composing.add(el);
    });
    on("compositionend", () => {
      compositionFlush.set(el, setTimeout(() => {
        composing.delete(el);
        if (views.get(id) !== el || retiredViews.has(el)) {
          return;
        }
        if (heldValues.has(el)) {
          writeValue(el, heldValues.get(el));
        }
        markupPending.delete(el);
        syncMarkup(el);
      }, 0));
    });
  }
  if (el.localName === "img" && (handlers.includes("load") || handlers.includes("error"))) {
    imageEvents(el, (failure) => {
      // kind 8, or a media `error` (kind 19) with its message
      const go = () => {
        if (views.get(id) === el && !retiredViews.has(el) && handlers.includes(failure == null ? "load" : "error")) {
          send(failure == null ? host.wasm.exact_dispatch(id, 8, 0, now()) : host.wasm.exact_dispatch(id, 19, writeIn("error\n" + failure), now()));
        }
      };
      if (host.inputReady) {
        go();
      } else {
        moduleReady.then(() => host.inputReady && go());
      }
      // an image that lands before the data executor is heard once it is
    });
  }
  if (el instanceof HTMLIFrameElement) {
    if (!iframeLoading.has(el)) {
      iframeLoading.set(el, true);
    }
    const arrived = (failure) => {
      iframeLoading.set(el, false);
      const source = el.exactAppFrame, go = () => {
        if (views.get(id) === el && !retiredViews.has(el) && el.exactAppFrame === source && handlers.includes(failure == null ? "load" : "error")) {
          send(failure == null ? host.wasm.exact_dispatch(id, 8, 0, now()) : host.wasm.exact_dispatch(id, 19, writeIn("error\n" + failure), now()));
        }
      };
      if (host.inputReady) {
        go();
      } else {
        moduleReady.then(() => host.inputReady && go());
      }
    };
    on("error", (ev) => arrived(ev.detail ?? "iframe did not load"));
    on("load", () => {
      if (el.exactAppFrame && !el.exactAppFrame.url) {
        return;
      }
      arrived(null);
    });
    if (handlers.includes("capture")) {
      on("exactcapture", (ev) => {
        if (views.get(id) === el && !retiredViews.has(el)) {
          send(host.wasm.exact_dispatch(id, 58, writeIn("capture\n" + JSON.stringify(ev.detail)), now()));
        }
      });
    }
    /* a capture's refusal (LLP 1020.002 D7) */
    if (handlers.some((h) => /^(navigate|currententrychange|navigatesuccess|navigateerror)$/.test(h))) {
      frameNav().then((m) => m.watch(el, (name, r) => {
        const go = () => {
          if (views.get(id) === el && !retiredViews.has(el) && handlers.includes(name)) {
            send(host.wasm.exact_dispatch(id, 58, writeIn(name + "\n" + JSON.stringify(r)), now()));
          }
        };
        if (host.inputReady) {
          go();
        } else {
          moduleReady.then(() => host.inputReady && go());
        }
      }));
    }
    // a same-origin guest's navigation records, kind 58 (frame-navigation.js)
    if (handlers.includes("message")) {
      messageFrames.add(el);
      ensureMessageListener();
    }
  }
  // element hears these.
  // A pressable takes the focus too, as natively (chat F14); input-glue.js activates it by key.
  if (handlers.some((k) => [
    "focus",
    "blur",
    "key",
    "keyup",
    "press",
    "copy",
    "cut",
    "paste"
  ].includes(k)) && !el.matches("input, button, select, textarea, a[href], summary") && !el.exactMarkup && !el.hasAttribute("tabindex")) {
    el.tabIndex = 0;
  }
  for (const kind of handlers) {
    if (kind === "press") {
      // A link inside a pressable node is the innermost activation, as a
      // nested press is: the link navigates and the outer press stays out.
      on("click", (e) => {
        const a = e.target.closest?.("a[href]");
        if (a && a !== el && el.contains(a)) {
          return;
        }
        for (let n = e.target; n && n !== el.parentNode; n = n.parentNode) {
          if (n.nodeType === 1 && n.hasAttribute("disabled")) {
            e.stopPropagation();
            return;
          }
        }
        focus.press(e, el, () => send(host.wasm.exact_dispatch(id, 0, writeIn(`${e.clientX},${e.clientY},` + (e.shiftKey ? "Shift+" : "") + (e.ctrlKey ? "Control+" : "") + (e.altKey ? "Alt+" : "") + (e.metaKey ? "Meta+" : "")), now())));
      });
    } else if (kind === "pan") {
      let pan;
      on("pointerdown", (e) => (pan ??= host.inputHandlers?.pan(el, id, on))?.(e));
      el.exactPanRetire = () => pan?.retire?.();
    } else if (kind === "scroll") {
      // The offsets, then the extents an action's `ScrollEvent` reads (chat F4).
      on("scroll", () => {
        const n = writeIn(`${el.scrollLeft},${el.scrollTop},${el.scrollWidth},${el.scrollHeight},${el.clientWidth},${el.clientHeight}`);
        send(host.wasm.exact_dispatch(id, 13, n, now()));
      });
    } else if (kind === "swiperight" || kind === "swipeleft") {
      // one recognizer for either or both ways (motion-glue.js)
      if (!el.exactSwipe) {
        el.exactSwipe = true;
        motion.attachSwipe(el, id, on);
      }
    } else if (kind === "scrolldragstart" || kind === "scrolldragend") {
      if (!el.exactScrollDrag) {
        el.exactScrollDrag = true;
        const wants = (k) => el.exactHandlers.includes(k);
        scrollDrag(el, on, () => wants("scrolldragstart") && send(host.wasm.exact_dispatch(id, 62, 0, now())), (vx, vy) => wants("scrolldragend") && send(host.wasm.exact_dispatch(id, 63, writeIn(`${vx},${vy}`), now())));
      }
    } else if (kind === "heightrelease") {
      motion.attachHeightDrag(el, id, on);
    } else if (kind === "transformrelease") {
      motion.attachTransformDrag(el, id, on);
    } else if (kind === "resize") {
      let landed;
      const first = new Promise((r) => {
        landed = r;
      });
      host.mediaAttaching.add(first);
      host.sizes.add(first);
      first.finally(() => {
        host.mediaAttaching.delete(first);
        host.sizes.delete(first);
      });
      el.exactResize = landed;
      (host.resizes ??= Promise.all([loadAfterPaint("./resize-glue.js", "observeResize"), host.geometryLoading?.catch(() => {})]).then(([o]) => o)).then((observe) => {
        observe(el, (r) => {
          try {
            if (views.get(id) === el && !retiredViews.has(el)) {
              send(host.wasm.exact_dispatch(id, 39, writeIn(`${r.x},${r.y},${r.width},${r.height}`), now()));
            }
          } finally {
            landed();
          }
        });
        if (!el.isConnected || !el.getClientRects().length) {
          landed();
        } else {
          const bound = setTimeout(landed, globalThis.__exactFirstSizeBound ?? 2e3);
          first.then(() => clearTimeout(bound));
        }
      }).catch((e) => {
        log(`resize: ${e.message}`);
        landed();
      });
    } else if (kind === "intersect") {
      let landed;
      const first = new Promise((r) => {
        landed = r;
      });
      host.mediaAttaching.add(first);
      host.sizes.add(first);
      first.finally(() => {
        host.mediaAttaching.delete(first);
        host.sizes.delete(first);
      });
      el.exactIntersect = landed;
      // (a view retired before it is observed lets go at once)
      (host.intersects ??= loadAfterPaint("./intersect-glue.js", "observeIntersect")).then((observe) => {
        if (views.get(id) !== el || retiredViews.has(el)) {
          landed();
          return;
        }
        const stop = observe(el, (r) => {
          try {
            if (views.get(id) === el && !retiredViews.has(el)) {
              send(host.wasm.exact_dispatch(id, 45, writeIn(`${Number(r.isIntersecting)},${r.intersectionRatio}`), now()));
            }
          } finally {
            landed();
          }
        });
        el.exactIntersect = () => {
          stop();
          landed();
        };
        const bound = setTimeout(landed, globalThis.__exactFirstSizeBound ?? 2e3);
        first.then(() => clearTimeout(bound));
      }).catch((e) => {
        log(`intersect: ${e.message}`);
        landed();
      });
    } else if ((kind === "pointerdown" || kind === "pointerup" || kind === "pointermove") && !el.exactPointer) {
      let p;
      el.exactPointer = true;
      const own = () => p ??= host.inputHandlers?.pointer(el, on, (k, r, e) => {
        if (views.get(id) !== el || retiredViews.has(el)) {
          return;
        }
        const outer = [host.pointerEvent, host.pointerDepth];
        host.pointerEvent = k === 31 ? null : e;
        host.pointerDepth = host.sending + 1;
        try {
          send(host.wasm.exact_dispatch(id, k, writeIn(r), now()));
        } finally {
          [host.pointerEvent, host.pointerDepth] = outer;
        }
      });
      el.exactPointerDown = (e, hold) => own()?.(e, hold);
      on("pointerover", () => own());
    } else if (kind === "toggle") {
      on(kind, (e) => send(host.wasm.exact_dispatch(id, 50, writeIn(`${e.oldState},${e.newState}`), now())));
    } else if (kind === "atompress") {
      on(kind, (e) => {
        const [k, source, href, start, end, ...held] = e.detail;
        send(host.wasm.exact_dispatch(id, 59, writeIn(`${start},${end},${[
          "Shift+",
          "Control+",
          "Alt+",
          "Meta+"
        ].filter((_, i) => held[i]).join("")}\n${k}\n${href}\n${source}`), now()));
      });
    } else if (kind === "beforeinput" || kind === "selectionchange" && sourceField(el)) {
      controlEvent(el, kind, on, (k, text, e) => {
        const outer = host.keyEvent;
        host.keyEvent = e;
        try {
          send(host.wasm.exact_dispatch(id, k, writeIn(text), now()));
        } finally {
          host.keyEvent = outer;
        }
      });
    } else if (kind === "selectionchange") {
      // its part of the page's selection (navigation.js `onSelection`): kind 35, offsets, rectangle count and coordinates, then text
      el.exactSelection = onSelection(el, (text, a, b, rects) => {
        if (views.get(id) === el && !retiredViews.has(el)) {
          send(host.wasm.exact_dispatch(id, 35, writeIn([
            a,
            b,
            rects.length,
            ...rects.flatMap((r) => r.slice(0, 4)),
            text
          ].join(",")), now()));
        }
      });
    } else if (kind === "copy" || kind === "cut" || kind === "paste") {
      // the clipboard's events at the focused node, the nearest handler's (spreadsheet F4); a field's own paste proceeds; a paste's files are minted `doc:` handles at once (57: their count, a line each, then the text; documents-glue.js `pasted`), the driver's standing in under the agent
      if (kind === "paste") {
        documentsGlue();
      }
      atBody(kind, on(kind, (e) => {
        e.stopPropagation();
        const outer = host.keyEvent;
        host.keyEvent = e;
        try {
          const text = e.clipboardData?.getData("text/plain") ?? "", driver = agentMode ? globalThis.exact.agentPaste : null;
          globalThis.exact.agentPaste = null;
          const files = kind === "paste" && (e.clipboardData?.files?.length || driver) ? globalThis.exact.documents?.pasted(e.clipboardData, driver) ?? [] : [];
          send(host.wasm.exact_dispatch(id, files.length ? 57 : 32 + [
            "copy",
            "cut",
            "paste"
          ].indexOf(kind), writeIn(files.length ? `${files.length}\n${files.join("\n")}\n${text}` : text), now()));
        } finally {
          host.keyEvent = outer;
        }
      }));
    } else if ([
      "contextmenu",
      "dblclick",
      "wheel",
      "drop",
      "beforeunload",
      "dragenter",
      "dragleave"
    ].includes(kind)) {
      // with their records (input-glue `mouse`; studio diary R22, R3, R19, R17); a running wheel or beforeunload is the event its `preventDefault()` prevents
      const go = (k, e) => host.inputHandlers?.mouse(el, k, e, (n, line) => {
        if (views.get(id) !== el || retiredViews.has(el)) {
          return;
        }
        const outer = host.keyEvent;
        host.keyEvent = e;
        try {
          send(host.wasm.exact_dispatch(id, n, writeIn(line), now()));
        } finally {
          host.keyEvent = outer;
        }
      });
      if (kind === "beforeunload") {
        el.exactUnload = new AbortController();
        addEventListener(kind, (e) => go(kind, e), { signal: el.exactUnload.signal });
      } else if (kind === "dragenter" || kind === "dragleave") {
        documentsGlue().then(() => globalThis.exact.documents.hearDrags(el, kind, () => !el.matches(":disabled") && !inertAncestor(el), (e) => go(kind, e)));
      } else {
        on(kind, (e) => go(kind, e));
      }
      if (kind === "drop") {
        on("dragover", (e) => go("dragover", e));
      }
    } else if ((kind === "change" || kind === "cancel") && el.type === "file") {
      // a picker's files, or its dismissal (LLP 1069.002 D2, D3)
      on(kind, () => {
        const p = picker().then((m) => kind === "change" ? m.change(el, id) : m.cancel(id));
        inflight.add(p);
        p.finally(() => inflight.delete(p));
      });
    } else if (kind === "input" || kind === "change" || kind === "select" && !el.exactMarkup) {
      // navigation.js `controlEvent`: a control's value, a text field's selection
      controlEvent(el, kind, on, (k, text) => send(host.wasm.exact_dispatch(id, k, writeIn(text), now())));
    } else if (kind === "hover") {
      // pointerenter/pointerleave: the element's own, not a bubbling mouseover.
      on("pointerenter", () => send(host.wasm.exact_dispatch(id, 2, 0, now())));
      on("pointerleave", () => send(host.wasm.exact_dispatch(id, 3, 0, now())));
    } else if (kind === "focusin" || kind === "focusout") {
      on(kind, (e) => {
        if (e.exactStopped) {
          return;
        }
        const target = e.target?.exactExitId ?? e.target?.id ?? "", related = e.relatedTarget?.exactExitId ?? e.relatedTarget?.id, outer = host.keyEvent;
        host.keyEvent = e;
        try {
          send(host.wasm.exact_dispatch(id, kind === "focusin" ? 46 : 47, writeIn(`${encoder.encode(target).length}:${target}${related === undefined ? "0" : `1${related}`}`), now()));
        } finally {
          host.keyEvent = outer;
        }
      });
    } else if (kind === "close" || kind === "scrollend") {
      on(kind, () => send(host.wasm.exact_dispatch(id, kind === "close" ? 49 : 51, 0, now())));
    } else if (kind === "focus" || kind === "blur") {
      on(kind, () => {
        const dispatch = () => send(host.wasm.exact_dispatch(id, kind === "focus" ? 4 : 5, 0, now()));
        kind === "blur" ? focus.blur(el, dispatch) : dispatch();
      });
    } else if (kind === "key" || kind === "keycapture" || kind === "keyup") {
      // `keycapture` (kind 44) hears keydown outermost first; `key` (6) and `keyup` (43) bubble with the web's key name (`e.key`).
      atBody(kind, on(kind === "keyup" ? "keyup" : "keydown", (e) => {
        if (e.exactStopped || kind === "keycapture" && e.exactFlowCapture && !e.exactFlowCapture.includes(el)) {
          return;
        }
        const outer = host.keyEvent;
        host.keyEvent = e;
        try {
          const n = writeIn(keyChord(e));
          send(host.wasm.exact_dispatch(id, kind === "keycapture" ? 44 : kind === "key" ? 6 : 43, n, now()));
        } finally {
          host.keyEvent = outer;
        }
      }, kind === "keycapture"));
    }
    if (kind === "submit" && el.tagName !== "TEXTAREA" && !el.exactMarkup) {
      // The web's implicit submission: Enter in a text input submits — here
      // to the node's `submit` handler, no form needed (and no reload). It is Enter's default: after every `key` handler on the path (the window's listener is last), unless one prevented it.
      on("keydown", (e) => {
        if (e.key === "Enter" && !e.isComposing && e.keyCode !== 229 && !e.exactSubmit) {
          e.exactSubmit = true;
          addEventListener("keydown", (w) => {
            if (w !== e || e.defaultPrevented) {
              return;
            }
            const run = () => {
              if (run.done) {
                return;
              }
              run.done = true;
              el.removeEventListener("keydown", run, true);
              el.removeEventListener("beforeinput", run, true);
              if (views.get(id) === el) {
                send(host.wasm.exact_dispatch(id, 7, 0, now()));
              }
            };
            el.addEventListener("keydown", run, true);
            if (e.target.localName !== "textarea" && !e.target.isContentEditable) {
              el.addEventListener("beforeinput", run, true);
            }
            setTimeout(run);
          }, { once: true });
        }
      });
    }
  }
}
