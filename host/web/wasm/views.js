import { picker, track, iframeLoading, localAssetURL, host, loadAfterPaint, moduleReady, views, writeIn, now, afterNativePaint, log, retiredViews, lists, root, encoder, readOut, imageProbes, inflight, IMAGE_ERROR, CORS_ERROR, heldValues, composing, loadDecorations, markupPending, scrollDocs, followScroll, pendingScrolls, DATA_LIMIT, authoredDisabled, holdWake, holdOrientation } from "../glue.js";
import { pageGivenFiles, commitGuestOrigin, frameURL, refuseURL, valuedControl, renderMarkup, editMenu, inertAncestor, contextAt, liveById, navigates, navigableURL, dataURL, settleValue } from "../navigation.js";
import { send } from "./commit.js";
import { attach } from "./input.js";
export function appSource(el, name, value, then) {
  const p = picker().then((m) => m.appURL(value)).then((url) => {
    if (el[`exactApp-${name}`] !== value) {
      return;
    }
    const media = el instanceof HTMLMediaElement && name === "src", to = url || (media ? value : null);
    if (el.getAttribute(name) !== to) {
      to ? el.setAttribute(name, to) : el.removeAttribute(name);
    }
    if (media) {
      then?.();
      syncMedia(el, { src: to });
    }
  });
  track(p.catch(() => {}));
  return el.getAttribute(name);
}
function appFrameSource(el, value) {
  el.exactSource = value;
  if (el.exactAppFrame?.path === value) {
    return true;
  }
  el.exactAppFrame?.dispose();
  el.exactAppFrame = null;
  if (!value?.startsWith("app:/") && !pageGivenFiles(value ?? "")) {
    return false;
  }
  /* an app file, or a page of the app's origin whose query may name one (LLP 1020.001 D7; picker-glue.js decides) */ const state = el.exactAppFrame = {
    path: value,
    live: true,
    url: null,
    dispose() {
      this.live = false;
      if (this.url) {
        URL.revokeObjectURL(this.url);
      }
    }
  };
  el.removeAttribute("src");
  iframeLoading.set(el, true);
  track(picker().then((m) => m.appFrame(el, state, () => commitGuestOrigin(el), (u) => frameURL(u, el.getAttribute("sandbox")) ? el.setAttribute("src", localAssetURL(u)) : refuseURL(el, "src", u))));
  return true;
}
export function syncMedia(el, set = {}, clear = []) {
  if (!(el instanceof HTMLMediaElement)) {
    return;
  }
  el.exactMedia ??= {
    props: {},
    handlers: []
  };
  Object.assign(el.exactMedia.props, set);
  for (const name of clear) {
    delete el.exactMedia.props[name];
  }
  host.mediaModule ??= new Promise((resolve) => requestAnimationFrame(() => resolve(loadAfterPaint("./media-glue.js", "installMedia"))));
  const attach = Promise.all([host.mediaModule, moduleReady]).then(([install]) => {
    if (el.isConnected) {
      install(el, (payload) => {
        if (views.get(Number(el.dataset.view)) !== el || !host.inputReady) {
          return false;
        }
        send(host.wasm.exact_dispatch(Number(el.dataset.view), 19, writeIn(payload), now()));
      });
      const held = !el.exactMediaHeld && globalThis.exact.mediaHold(el);
      if (held) {
        el.exactMediaHeld = true;
        host.mediaAttaching.add(held.done);
        held.done.finally(() => {
          el.exactMediaHeld = false;
          host.mediaAttaching.delete(held.done);
        });
      }
    }
  }).catch(console.error);
  host.mediaAttaching.add(attach);
  attach.finally(() => host.mediaAttaching.delete(attach));
  // Attached once the data executor settles (a report before then is dropped), reporting what the element has then; `clock settle` waits for it, not `inflight`, which a module replacement drains during activation. A load or a seek in flight then holds a settle too, as on the JS target (media-glue.js `mediaHold`; `removeMedia` lets it go).
}
export function nativeCreate(el, id) {
  const st = el.exactNative = {
    id,
    name: el.localName,
    state: "loading",
    status() {
      return {
        name: this.name,
        state: this.state,
        ...this.error ? { error: this.error } : {}
      };
    },
    destroy() {
      this.destroyed = true;
      host.nativeHost?.then((h) => h.destroy(el));
    }
  };
  (host.nativeHost ??= afterNativePaint().then(() => loadAfterPaint("./native-glue.js", "nativeHost")).then((make) => make({
    log,
    dispatch(el, kind, text) {
      const id = Number(el.dataset.view);
      if (host.inputReady && views.get(id) === el && !retiredViews.has(el)) {
        send(host.wasm.exact_dispatch(id, kind, text == null ? 0 : writeIn(text), now()));
      }
    }
  }))).then((h) => h.attach(el), (e) => {
    host.nativeHost = null;
    st.state = "unavailable";
    st.error = String(e?.message ?? e);
    log(`native ${st.name} #${id}: unavailable: ${st.error}`);
  });
}
// A virtualized list has logical rows; its (`collection`) feedback is navigation.js's.
export function listView(el, id, collection) {
  if (!collection) {
    return;
  }
  lists.set(el, { id });
  host.listSelectionLoading ??= new Promise((resolve) => requestAnimationFrame(() => resolve(loadAfterPaint("./list-selection.js", "installListSelection")))).then((install) => {
    host.listSelection = install({
      root,
      lists,
      index: (el, key) => host.wasm.exact_list_index(Number(el.dataset.view), writeIn(key)),
      text: (el, a, b) => {
        const first = encoder.encode(a?.key ?? "").length, len = writeIn((a?.key ?? "") + (b?.key ?? ""));
        return readOut(host.wasm.exact_list_text(Number(el.dataset.view), first, len, a?.paragraph ?? 0, a?.offset ?? 0, b?.paragraph ?? 0, b?.offset ?? 0));
      }
    });
    host.listSelection.sync();
  }).catch(console.error);
}
export function syncLists() {
  host.listSelection?.sync();
}
export function forgetList(el) {
  host.listSelection?.forget(el);
  lists.delete(el);
}
// A tinted raster's `scale-down` (element.rs `host_css`, LLP 1011 §3): `contain` unless its natural size fits the content box, known once it loads.
function tintFit(el) {
  if (!el.style.getPropertyValue("mask-size").includes("--exact-tint-fit")) {
    return;
  }
  if (!el.complete) {
    el.addEventListener("load", () => tintFit(el), { once: true });
    return;
  }
  const cs = getComputedStyle(el);
  const fits = el.naturalWidth <= el.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight) && el.naturalHeight <= el.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
  el.style.setProperty("--exact-tint-fit", fits ? "auto" : "contain");
}
export function imageEvents(el, fire) {
  const probe = (src) => {
    if (!imageProbes.has(src)) {
      const p = new Promise((ok) => {
        Object.assign(new Image(), {
          crossOrigin: "anonymous",
          onload: () => ok(true),
          onerror: () => ok(false)
        }).src = src;
      });
      inflight.add(p);
      p.finally(() => inflight.delete(p));
      imageProbes.set(src, p);
    }
    return imageProbes.get(src);
  };
  const settle = (failed) => {
    const src = el.currentSrc;
    if (el.hasAttribute("data-symbol-path") || el.exactSettled === src) {
      return;
    }
    (failed || !getComputedStyle(el).maskImage?.includes("url(") || /^(data|blob):/.test(src) || new URL(src, location.href).origin === location.origin ? Promise.resolve(!failed) : probe(src)).then((ok) => {
      if (el.currentSrc !== src || el.exactSettled === src) {
        return;
      }
      el.exactSettled = src;
      fire(ok ? null : failed ? IMAGE_ERROR : CORS_ERROR);
    });
  };
  el.addEventListener("load", () => settle(false));
  el.addEventListener("error", () => settle(true));
  if (el.complete && el.getAttribute("src")) {
    setTimeout(() => el.complete && settle(!el.naturalWidth));
  }
}
export function refreshSymbols() {
  const rasters = [], symbols = [...views.values()].filter((el) => {
    if (!(el instanceof HTMLImageElement)) {
      return false;
    }
    if (el.hasAttribute("data-symbol-path")) {
      return true;
    }
    rasters.push(el);
    return false;
  });
  // Batch reads before writes; computed styles are live, so keep only scalar values.
  const titles = symbols.flatMap((el) => {
    const title = el.parentElement?.matches("button[data-button-style]") && el.parentElement.querySelector(":scope > [data-exact-text]");
    if (!title) {
      return [];
    }
    const own = getComputedStyle(el), font = getComputedStyle(title), style = { color: font.color };
    for (const [axis, prop] of [["size", "fontSize"], ["weight", "fontWeight"]]) {
      if (own.getPropertyValue(`--exact-symbol-${axis}-authored`).trim() !== "1") {
        style[prop] = font[prop];
      }
    }
    return [[el, style]];
  });
  for (const [el, style] of titles) {
    Object.assign(el.style, style);
  }
  const configs = symbols.map((el) => {
    const cs = getComputedStyle(el);
    return {
      el,
      point: parseFloat(cs.fontSize) * (parseFloat(cs.getPropertyValue("--exact-symbol-scale")) || 1),
      weight: Number(cs.fontWeight),
      objectFit: cs.objectFit
    };
  });
  for (const { el, point, weight } of configs) {
    const path = el.getAttribute("data-symbol-path"), filled = el.hasAttribute("data-symbol-fill"), box = el.getAttribute("data-symbol-viewbox") || "0 0 24 24", key = `${path}:${filled}:${point}:${weight}:${box}:${el.hasAttribute("data-symbol-nonzero")}`;
    if (!path && !el.dataset.symbolSource?.startsWith("symbol:sf/") && el.dataset.symbolSource !== "symbol:" && el.symbolRefusal !== el.dataset.symbolSource) {
      log(`image ${el.dataset.symbolSource} refused: unknown symbol role`);
      el.symbolRefusal = el.dataset.symbolSource;
    }
    if (el.symbolKey !== key) {
      el.symbolKey = key;
      const stroke = 1.1 + (Math.max(100, Math.min(900, weight)) - 100) / 400;
      const paint = filled ? `fill="black" fill-rule="${el.hasAttribute("data-symbol-nonzero") ? "nonzero" : "evenodd"}"` : `fill="none" stroke="black" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"`;
      el.symbolMask = `url("data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="${point}" height="${point}" viewBox="${box}">${path.split(";").map((d) => `<path d="${d}" ${paint}/>`).join("")}</svg>`)}")`;
      el.symbolPlaceholder = `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="${point}" height="${point}"/>`)}`;
    }
    if (el.getAttribute("src") !== el.symbolPlaceholder) {
      el.src = el.symbolPlaceholder;
    }
    el.style.setProperty("--exact-symbol-mask", el.symbolMask);
  }
  // Sources can set intrinsic size synchronously; percentage padding must resolve against the resulting layout.
  const fits = configs.map(({ el, point, objectFit }) => {
    const cs = objectFit === "scale-down" ? getComputedStyle(el) : null;
    const fits = cs && point <= el.clientWidth - (parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight)) && point <= el.clientHeight - (parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom));
    // `fill`, CSS's initial `object-fit`, keeps a symbol's aspect, as on the Apple hosts (LLP 1011 §4).
    return [el, objectFit === "none" || objectFit === "scale-down" && fits ? `${point}px ${point}px` : objectFit === "scale-down" || objectFit === "fill" ? "contain" : objectFit];
  });
  for (const [el, fit] of fits) {
    el.style.setProperty("--exact-symbol-fit", fit);
  }
  for (const el of rasters) {
    tintFit(el);
  }
}
export function writeValue(el, value) {
  // An option's value getter falls back to its label, so a bound value equal to the label is written still: the
  // attribute holds it when the label changes.
  if (el.localName === "option") {
    if (el.getAttribute("value") !== value) {
      el.value = value;
    }
    return;
  }
  if (el.exactMarkup) {
    el.exactMarkup.setValue(value);
    return;
  }
  if (el instanceof HTMLTextAreaElement) {
    el.exactSourceValue = String(value);
  }
  const old = el.value;
  if (old === value) {
    heldValues.delete(el);
    return;
  }
  if (composing.has(el)) {
    heldValues.set(el, value);
    return;
  }
  heldValues.delete(el);
  if (typeof el.setRangeText !== "function" || valuedControl(el) || old === "" || document.activeElement !== el) {
    el.value = value;
    return;
  }
  let a = 0, z = 0;
  while (a < old.length && a < value.length && old[a] === value[a]) {
    a++;
  }
  while (z < old.length - a && z < value.length - a && old[old.length - 1 - z] === value[value.length - 1 - z]) {
    z++;
  }
  const end = old.length - z, text = value.slice(a, value.length - z), { selectionStart: s0, selectionEnd: s1 } = el, carry = (p) => p <= a ? p : p >= end ? p + text.length - (end - a) : a + text.length;
  el.setRangeText(text, a, end, "preserve");
  if (el.value !== value) {
    el.value = value;
  } else {
    el.setSelectionRange(carry(s0), Math.max(carry(s0), carry(s1)));
  }
}
export function syncMarkup(el) {
  if (el.exactMarkup) {
    el.exactMarkup.sync();
    return;
  }
  if (el.exactDecorations) {
    el.exactDecorations.sync();
  } else if (el instanceof HTMLTextAreaElement && el.hasAttribute("data-decorations")) {
    loadDecorations(el);
  }
  if (!(el instanceof HTMLTextAreaElement) || el.getAttribute("markup") !== "markdown" || markupPending.has(el)) {
    return;
  }
  markupPending.add(el);
  (host.markupModule ??= moduleReady.then(() => loadAfterPaint("./markup-editor.js", "installMarkupEditor"))).then((install) => {
    const id = Number(el.dataset.view), live = (node) => views.get(id) === node && node.isConnected && !retiredViews.has(node);
    if (composing.has(el)) {
      return;
    }
    // Keep the native marked range until its final input has committed.
    markupPending.delete(el);
    if (!live(el) || el.getAttribute("markup") !== "markdown") {
      return;
    }
    install(el, {
      live,
      replace(node) {
        views.get(id)?.exactUnload?.abort();
        views.get(id)?.exactIntersect?.();
        views.set(id, node);
        attach(node, id, el.exactHandlers);
      },
      // (the old node's first entry lets go: the new one is observed)
      select(node, payload) {
        if (live(node) && host.inputReady && node.exactHandlers.includes("select")) {
          send(host.wasm.exact_dispatch(id, 21, writeIn(payload), now()));
        }
      }
    });
  }).catch((error) => console.error("exact: Markdown editor:", error));
}
export function applyProps(el, set, clear, adopting = false) {
  syncMedia(el, set, clear);
  if (set && "data-scrolldocument" in set) {
    scrollDocs.add(el);
  }
  let sandboxChanged = false;
  for (const name of clear || []) {
    if (name === "tabindex") el.exactAuthoredTabIndex = null;
    if (el instanceof HTMLIFrameElement && name === "src") {
      appFrameSource(el, null);
      iframeLoading.set(el, true);
    }
    if (el instanceof HTMLIFrameElement && name === "sandbox" && el.hasAttribute("sandbox")) {
      sandboxChanged = true;
    }
    if (name === "scrollFollowEnd") {
      followScroll(el, false);
    } else if (name === "scrollTop" || name === "scrollLeft") {
      const pending = pendingScrolls.get(el);
      if (pending) {
        delete pending[name];
      }
    } else if (name === "text") {
      if (el.localName === "time") {
        el.exactTimeText = null;
      }
      if (el.localName === "time" && el.childElementCount) {
        el.exactLiveTime?.text(null);
      } else {
        el.textContent = "";
      }
    } else if (name === "value") {
      el.exactValue = undefined;
      if (el.localName === "option") {
        el.removeAttribute("value");
      } else {
        writeValue(el, "");
      }
    } else if (name === "checked") {
      el.exactChecked = undefined;
      el.checked = false;
    } else if (name === "data-action") {
      el.removeAttribute(name);
      el.style.touchAction = "";
    } else if (name === "autofocus") {
      el.exactAutofocus = false;
      el.removeAttribute(name);
    } else if (name === "inert") {
      el.authoredInert = false;
      el.inert = false;
    } else {
      el.removeAttribute(name);
    }
  }
  for (const [name, value] of Object.entries(set || {})) {
    if (name === "tabindex") el.exactAuthoredTabIndex = value;
    if (el instanceof HTMLIFrameElement && name === "sandbox" && el.getAttribute("sandbox") !== value) {
      sandboxChanged = true;
    }
    if (el instanceof HTMLImageElement && name === "src" && value.startsWith("symbol:")) {
      el.symbolSource = value;
    } else if (name === "scrollFollowEnd") {
      followScroll(el, value === "true");
    } else if (name === "scrollTop" || name === "scrollLeft") {
      const offset = Number(value);
      if (Number.isFinite(offset)) {
        pendingScrolls.set(el, {
          ...pendingScrolls.get(el),
          [name]: offset
        });
      }
    } else if (name === "text") {
      if (el.localName === "time") {
        el.exactTimeText = value;
      }
      if (el.localName === "time" && el.childElementCount) {
        el.exactLiveTime?.text(value);
      } else if (el.childElementCount === 0 && el.textContent !== value) {
        el.textContent = value;
      }
    } else if (name === "markupPieces") {
      if (!renderMarkup(el, value, adopting)) continue;
      if (el.hasAttribute("data-atoms") && !globalThis.exact.atoms) {
        loadAfterPaint("./atoms.js", "atoms").then(() => {
          if (el.isConnected) {
            renderMarkup(el, value);
          }
        });
      }
    } else if (name === "contextpopover") {
      el.setAttribute(name, value);
      if (!el.exactContext) {
        el.exactContext = true;
        el.addEventListener("contextmenu", (e) => {
          if (e.$cp || !el.getAttribute(name) || editMenu(e.target) || el.matches(":disabled") || inertAncestor(el)) {
            return;
          }
          e.$cp = 1;
          e.preventDefault();
          e.stopPropagation();
          contextAt(e, () => {
            if (!el.isConnected || el.matches(":disabled") || inertAncestor(el)) {
              return;
            }
            const p = liveById(el.getAttribute(name) ?? "");
            if (p && !p.matches(":popover-open")) {
              return [p, el];
            }
          });
        });
      }
    } else if (name === "data-action") {
      el.setAttribute(name, value);
      el.style.touchAction = "none";
    } else if (name === "value") {
      writeValue(el, value);
      if (valuedControl(el)) {
        el.exactValue = value;
      }
    } else if (name === "checked") {
      el.exactChecked = value === "true";
      el.checked = el.exactChecked;
    } else if (name === "inert") {
      el.authoredInert = value === "true";
      el.inert = el.authoredInert;
    } else if (name === "autofocus") {
      el.exactAutofocus = value === "true";
      if (!el.exactAutofocus) {
        el.removeAttribute(name);
      }
    } else if (name === "disabled" || name === "readonly" || name === "multiple" || el instanceof HTMLMediaElement && [
      "autoplay",
      "controls",
      "loop",
      "muted",
      "playsinline",
      "disablepictureinpicture",
      "disableremoteplayback"
    ].includes(name)) {
      if (value === "true") {
        el.setAttribute(name, "");
        if (name === "disabled" && el === document.activeElement && el.matches(":disabled")) {
          el.blur();
        }
      } else {
        el.removeAttribute(name);
      }
    } else if (el instanceof HTMLIFrameElement && (name === "src" && appFrameSource(el, value) || /^(partition|appleAllowsBackForwardNavigationGestures|appleAllowsMagnification|appleInactiveSchedulingPolicy)$/i.test(name) && (el["$" + name] = value, true))) {} else {
      const app = (name === "src" || name === "poster") && (el[`exactApp-${name}`] = value.startsWith("app:/") ? value : null), v = app ? appSource(el, name, value) : name === "src" && !(el instanceof HTMLIFrameElement) && value.startsWith("data:") && value.length > DATA_LIMIT ? (log(`image refused: a data: source is over ${DATA_LIMIT} bytes`), "") : name === "src" || name === "href" || name === "poster" ? localAssetURL(value) : value, same = el.getAttribute(name) === v;
      // setting what is there reloads an adopted iframe or video
      if (el instanceof HTMLIFrameElement && name === "src") {
        el.exactSource = value;
        if (!same) {
          iframeLoading.set(el, true);
        }
      }
      // kept: the sandbox decides a `data:` one
      if (navigates(el, name) && !(name === "src" ? frameURL(value, "sandbox" in set ? set.sandbox : el.getAttribute("sandbox")) : navigableURL(value))) {
        refuseURL(el, name, value);
      } else if (!same && v !== null) {
        el.setAttribute(name, v);
      }
    }
  }
  if (!host.inputReady && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLButtonElement)) {
    const disabled = set && "disabled" in set ? set.disabled === "true" : clear?.includes("disabled") ? false : authoredDisabled.get(el) ?? el.disabled;
    authoredDisabled.set(el, disabled);
    el.disabled = true;
  }
  if (el instanceof HTMLIFrameElement && sandboxChanged) {
    iframeLoading.set(el, true);
    const source = dataURL(el.exactSource) ? frameURL(el.exactSource, el.getAttribute("sandbox")) ? el.exactSource : "about:blank" : el.getAttribute("src");
    // the new sandbox decides a `data:` source again
    el.setAttribute("src", source ?? "about:blank");
    if (source === null) {
      el.removeAttribute("src");
    }
  }
  if (el instanceof HTMLIFrameElement) {
    commitGuestOrigin(el);
  }
  if (set && "value" in set || clear?.includes("value")) {
    settleValue(el);
  }
  syncMarkup(el);
  // a valued control shows its committed value when it changes, not another prop (LLP 1069.001 D4, amended)
  if (set && ("viewportFit" in set || "interactiveWidget" in set) || clear?.some((n) => n === "viewportFit" || n === "interactiveWidget")) {
    syncViewportFit();
  }
  if (set && "data-exact-wakelock" in set && !host.wakeLock) {
    holdWake();
  }
  if (set && "data-exact-orientationlock" in set && !host.orientationLock) {
    holdOrientation();
  }
}
export function syncViewportFit() {
  const first = root.firstElementChild;
  const cover = first?.getAttribute("viewportFit") === "cover";
  const widget = first?.getAttribute("interactiveWidget");
  const meta = document.querySelector("meta[name=\"viewport\"]");
  const want = "width=device-width, initial-scale=1" + (cover ? ", viewport-fit=cover" : "") + (widget ? `, interactive-widget=${widget}` : "");
  if (meta && meta.content !== want) {
    meta.content = want;
  }
  const vv = globalThis.visualViewport;
  root.style.height = widget === "resizes-content" && vv && vv.scale === 1 && !document.documentElement.hasAttribute("data-scrolldocument") ? `${Math.min(innerHeight, vv.height)}px` : "";
}
