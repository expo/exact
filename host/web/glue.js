import { listItem, syncListItemRoles, sheetLeaves, sheetFrame, useSheets, liveById, sourceField, popoverCommand, contextAt, grantOrigins, createGrantSet, grantError, rawGrantText, scopedGrantSet, deferredFulfill, refusal, commitGuestOrigin, guestMessageAuthorized, pageGivenFiles, focusController, runFocusCommands, controlEvent, environment, onKeyboard, preferences, updateCode, onPreferences, inertAncestor, modalLayers, editMenu, navigation, afterPaintPieces, afterPaint, presenceLoader, animationClock, scrollFollowers, renderMarkup, navigableURL, navigates, refuseURL, frameURL, dataURL, releaseAssets, devFirst, reportPlace, reportTime, launchLocation, pageReporter, appRootFontSize, valuedControl, typedControl, settleValue, foldBits, foldEnv, onFold, preferFold, fold, animationClocks, onSelection, noteFit, refitTexts, pageTitle, pageStore, routeFlags, scrollDrag } from "./navigation.js";
import { send, applyBatch } from "./wasm/commit.js";
import { positionContexts, contextPanel } from "./wasm/context.js";
import { syncViewportFit } from "./wasm/views.js";
import { boot, loadRust, main, reportColors } from "./wasm/boot.js";
import { agentInputReady, agent, agentSettled } from "./wasm/agent.js";
// @ref LLP 1007 §3: the wasm DOM host and its live session state.
export const host = {};
host.guestOutline = undefined;
host.guestTap = undefined;
host.guestType = undefined;
host.reveal = undefined;
host.backButton = undefined;
host.viewBox = undefined;
host.layoutBoxes = undefined;
host.scrollRange = undefined;
host.selectAll = undefined;
host.typeControl = undefined;
host.glidesSettled = undefined;
host.scrollsSeen = undefined;
host.agentInputLoading = undefined;
export const AGENT_ADMITTED = true;
// false in a production bake: host/web/build.mjs rewrites this line (LLP 1069.007 D2)
host.httpModule = undefined;
host.pickerModule = undefined;
host.documentsModule = undefined;
host.frameModule = undefined;
host.frameNavigation = undefined;
export const frameNav = () => host.frameNavigation ??= loadAfterPaint("./frame-navigation.js", "frameNavigation");
// frameNavigation: an iframe's navigation records and commands (LLP 1020.002); the file picker (LLP 1069.002) and documents (LLP 1069.010), loaded on first use
export const picker = (at = host.incarnation) => (host.pickerModule ??= loadAfterPaint("./picker-glue.js", "picker").then((install) => install({
  appId: globalThis.exact.compat?.inputs?.app,
  generation: () => host.incarnation,
  dispatch: (id, kind, payload) => {
    if (views.has(id)) {
      send(host.wasm.exact_dispatch(id, kind, writeIn(payload), now()));
    }
  },
  pickedPath: (name) => loadStage("inspection").then(() => ask({
    op: "pickedPath",
    name
  }).path),
  log
}))).then((m) => m.at(at));
// its operations answer generation `at`, the one that asked
// An `app:/` source (D7): the app's own file, as the picker glue's object URL once it resolves; the old picture stays meanwhile, as for any `src`, and an element with none gets none (an empty media `src` is a failed load). A media element plays it (podcast F19): the glue hears the URL as its source, and a path with no file is handed to it as written, which it refuses (`src-not-supported`) as HTML refuses a source it cannot fetch. `then` runs once it lands (a `load` command's). A `data:` source past its bound shows nothing, as on every host (LLP 1011 §2; exact_raster::MAX_DATA_URL_BYTES).
export const DATA_LIMIT = 1024 * 1024;
export const documentsGlue = (at = host.incarnation) => (host.documentsModule ??= loadAfterPaint("./documents-glue.js", "documents").then((d) => (d.keeping(globalThis.exact.compat?.inputs?.app, host.grantSet), d).install({
  generation: () => host.incarnation,
  dispatch: (id, kind, payload) => {
    if (views.has(id)) {
      send(host.wasm.exact_dispatch(id, kind, writeIn(payload), now()));
    }
  },
  log,
  openFile: (value) => {
    const id = [...views].find(([, el]) => el.dataset?.testid === "open-file")?.[0];
    if (id == null) {
      return false;
    }
    send(host.wasm.exact_dispatch(id, 1, writeIn(value), now()));
    return true;
  }
}))).then((m) => m.at(at));
export function httpHelpers() {
  return host.httpModule ??= moduleReady.then(() => loadAfterPaint("./http-body.js", "httpHelpers"));
}
export const root = document.getElementById("exact-root");
export const views = new Map();
// view id -> element
// Springs, holds, drags and virtualized collections: after-paint pieces, fetched on first use (LLP 1047 D5).
export const pieces = afterPaintPieces(loadAfterPaint, {
  root,
  views,
  applyBatch,
  agent: () => agentMode,
  inert: inertAncestor,
  now: () => now(),
  generation: () => host.incarnation,
  ready: () => host.inputReady,
  log: (line) => log(line),
  replayed() {
    motion.commit();
    arrange.commit();
    if (agentMode) {
      register(host.agentClock);
      seek(host.agentClock);
    } else {
      motion.followTimelines();
    }
  },
  wasm(name, bytes) {
    if (!host.wasm) {
      return null;
    }
    new Uint8Array(host.memory.buffer, host.wasm.exact_in(bytes.length), bytes.length).set(bytes);
    return JSON.parse(readOut(host.wasm[name](bytes.length)));
  }
});
export const { collections, motion, arrange } = pieces, retiredViews = new WeakSet();
// committed removals must not dispatch teardown events
export const presence = presenceLoader(loadAfterPaint, root, (batch) => applyBatch(batch), log);
/* -exact-exit-animation and -exact-layout-transition, after paint at first use (LLP 1063) */
afterPaint(() => loadAfterPaint("./menu-rows.js", "menuRows").then((install) => install(), console.error));
// a menu row's popover opens from the menu's invoker (LLP 1021)
host.mediaModule = undefined;
host.mediaAttaching = new Set();
host.sizes = new Set();
host.soundModule = undefined;
host.soundOut = undefined;
host.midiModule = undefined;
host.midiOut = undefined;
host.imageHold = undefined;
host.geometry = null;
host.geometryLoading = null;
host.resizes = null;
host.intersects = null;
// the voice table's output (sound-glue.js, LLP 1096 D7); animated images held to the agent's clock (image-glue.js, LLP 1011.000); geometry reads (geometry-glue.js, LLP 1051.000 D4); the element resize event (resize-glue.js)
host.wakeLock = null;
export const holdWake = () => (host.wakeLock ??= loadAfterPaint("./wakelock-glue.js", "wakeLockGlue")).then((w) => w.hold(w.holders(root).length > 0, log));
host.orientationLock = null;
export const holdOrientation = () => (host.orientationLock ??= loadAfterPaint("./orientationlock-glue.js", "orientationLockGlue")).then((o) => o.hold(o.session(root), log, (e) => {
  for (const [i, v] of views) {
    if (v === e) {
      return i;
    }
  }
  return null;
}));
// LLP 1125 D11: loaded at the first `wakelock`, then asked after each batch; LLP 1133.029 D7: `orientationlock` the same
export const iframeLoading = new WeakMap();
// iframe -> true until its latest src load
export const messageViews = new Set(), messageFrames = new Set();
// the latter: iframes whose node handles `message`
export const keyChord = (e) => (e.shiftKey ? "Shift+" : "") + (e.ctrlKey ? "Control+" : "") + (e.altKey ? "Alt+" : "") + (e.metaKey ? "Meta+" : "") + e.key + "\n" + (/^[A-Za-z0-9]*$/.test(e.code ?? "") ? e.code ?? "" : "") + "\n" + !!e.repeat + "\n" + !!(e.isComposing || e.keyCode === 229 || e.key === "Process");
/* a keydown or keyup as kind 6's or 43's payload: the chord `Event::key` reads, then its code, repeat and isComposing, which a keyCode-229 keydown is too (rt.js `onKey`; `KeyboardEvent::parse`, #140) */
host.messageListening = false;
host.bodyHeard = undefined;
host.keyEvent = null;
host.pointerEvent = null;
host.pointerDepth = 0;
// keyEvent: the keydown (or wheel, beforeunload, clipboard event) a handler is running for, which its `preventDefault()` command prevents; pointerEvent: the pointer's down or up, which only its `stopPropagation()` reaches (its default is the browser's), from its own batch alone (`pointerDepth`, `send`)
host.wasm = null;
host.memory = null;
host.inputReady = false;
host.inputHandlers = undefined;
host.structuralModalLayers = undefined;
// Native modules (LLP 1024 D3): a module node is its custom element, empty until the adapter and the app's module load after first paint (the browser's paint entry; two frames and a beat where it records none).
host.nativePaint = null;
export function afterNativePaint() {
  return host.nativePaint ??= new Promise((resolve) => {
    let observer;
    const done = () => {
      observer?.disconnect();
      resolve();
    };
    try {
      observer = new PerformanceObserver(() => requestAnimationFrame(done));
      observer.observe({
        type: "paint",
        buffered: true
      });
    } catch {}
    requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 250)));
  });
}
host.nativeHost = null;
export const authoredDisabled = new WeakMap();
host.logicInfo = null;
host.moduleLoader = null;
host.activeModule = null;
host.moduleResponse = new Uint8Array();
export const pageNative = Boolean(document.querySelector("meta[name=\"exact-native\"]"));
host.pageNativeModule = null;
export const loadPageNative = () => host.pageNativeModule ??= afterNativePaint().then(() => loadAfterPaint("./native-glue.js", "pageNative")).then((load) => load(pageNative, {
  ready: () => host.inputReady,
  generation: () => host.incarnation,
  agent: agentMode,
  now,
  changed: (topic) => {
    if (host.wasm.exact_changed) {
      applyBatch(JSON.parse(readOut(host.wasm.exact_changed(writeIn(topic)))));
    }
  }
})).catch((error) => {
  host.pageNativeModule = null;
  throw error;
});
host.rustLoader = null;
host.rustLoading = null;
export const rustImports = Object.fromEntries([
  "load",
  "call",
  "read",
  "drop"
].map((name) => [name, (...args) => {
  if (!host.rustLoader) {
    throw new Error("Rust module loader is not ready");
  }
  return host.rustLoader[name](...args);
}]));
// A Rust source's entropy and, under the agent, the seed of its repeatable
// stream (LLP 1069.005 D5, D2b; `exact_data::crypto`). The seed is
// storage-environment.js's `agentSeed`: a loopback page only; -1 is none.
export const dataImports = {
  random: (ptr, len) => {
    crypto.getRandomValues(new Uint8Array(host.wasm.memory.buffer, ptr, len));
  },
  agent_seed: () => {
    const url = new URL(location.href), seed = Number(url.searchParams.get("seed") ?? 1);
    if (!url.searchParams.has("agent") || ![
      "localhost",
      "127.0.0.1",
      "[::1]"
    ].includes(url.hostname)) {
      return -1;
    }
    return Number.isSafeInteger(seed) && seed >= 0 && seed < 2 ** 32 ? seed : 1;
  }
};
export function loadAfterPaint(file, exported) {
  return globalThis.exact?.[exported] === undefined ? new Promise((resolve, reject) => {
    const script = Object.assign(document.createElement("script"), { type: "module" });
    script.src = new URL(file, import.meta.url).href;
    script.onload = () => resolve(globalThis.exact[exported]);
    script.onerror = () => reject(new Error("host module loader failed: " + file));
    document.head.append(script);
  }) : Promise.resolve(globalThis.exact[exported]);
}
export const pageFile = (name) => (path, grants) => (globalThis.exact[name] ? Promise.resolve(globalThis.exact[name]) : loadAfterPaint("./storage-request.js", name)).then((f) => f(globalThis.exact.compat.inputs.app, grants, path));
// @ref LLP 1043.000 §3 D7/D8 — optional executor, absent from ordinary boots.
host.textflow = null;
host.flowLoading = null;
host.flowContexts = [];
host.flowDue = null;
host.flowLanding = false;
host.flowFrames = false;
host.flowWaits = [];
host.flowTolerance = 0;
host.present = (timestamp) => send(host.flowWaits.length ? host.wasm.exact_advance(timestamp - t0, 8) : host.wasm.exact_frame(timestamp - t0));
// an animation frame (LLP 1073 D5)
host.resolveModuleReady = undefined;
host.headGlue = null;
host.fragmentGlue = null;
host.fragmentTurn = 0;
host.page = document.querySelector("script[type=\"application/vnd.exact.checkpoint\"]") ? {
  holding: true,
  early: globalThis.exact?.taps?.() ?? []
} : null;
// a built document (LLP 1048.000 D6), held until the runtime settles; the presses its capture script took before now come first
export const moduleReady = new Promise((resolve) => {
  host.resolveModuleReady = resolve;
});
export const focus = focusController({
  ready: () => host.inputReady,
  elements: () => views.values(),
  inert: inertAncestor,
  removed: (el) => retiredViews.has(el)
});
export const focusAutofocus = focus.autofocus;
for (const kind of [
  "click",
  "beforeinput",
  "submit"
]) {
  root.addEventListener(kind, (event) => {
    if (!host.inputReady && !host.page?.holding) {
      event.preventDefault();
      event.stopImmediatePropagation();
    } else if (event.type === "click") {
      host.page?.early?.push(event);
    }
  }, true);
}
export function moduleCall(op, ptr, len) {
  if (op === 1) {
    if (len !== host.moduleResponse.length) {
      throw new Error("module response buffer mismatch");
    }
    new Uint8Array(host.memory.buffer, ptr, len).set(host.moduleResponse);
    return len;
  }
  try {
    const request = JSON.parse(new TextDecoder().decode(new Uint8Array(host.memory.buffer, ptr, len)));
    host.moduleResponse = new TextEncoder().encode(JSON.stringify(host.moduleLoader?.call(request) ?? { error: "browser module not loaded" }));
  } catch (error) {
    host.moduleResponse = new TextEncoder().encode(JSON.stringify({ error: String(error) }));
  }
  return host.moduleResponse.length;
}
export const encoder = new TextEncoder();
export const decoder = new TextDecoder();
export const t0 = performance.now();
export const agentMode = AGENT_ADMITTED && new URL(location.href).searchParams.has("agent"), agentKeepsStore = !agentMode || new URL(location.href).searchParams.has("storage"), pendingStore = pageStore(loadAfterPaint);
// `--storage` on the page (platformer R10); a nameless drive stays in memory
// The driver's fetch faults (LLP 1103, faults.js, loaded with the first request): an injected failure's journal line is the runner's.
if (agentMode) {
  globalThis.__exactFaultLog = (line) => log(line);
}
host.agentClock = agentMode ? 0 : null;
host.followOnSeek = true;
// A seek moves drag timelines' sources too (LLP 1057.003 D2): their consumers follow in it.
export const clocks = animationClocks(root), { register, seek: seekAnimations, settle: settleCandidate } = animationClock(() => host.agentClock, () => ask({ op: "settle" }).settle, () => {
  if (host.followOnSeek) {
    motion.followTimelines();
  }
  presence.live?.sync();
});
export const seek = (to) => {
  (host.imageHold ??= loadAfterPaint("./image-glue.js", "holdImages").then((f) => f({
    root,
    now: () => host.agentClock
  }))).then((h) => h.seek());
  seekAnimations(to);
};
const POST_BOUND = 64;
// posts held per surface before its canvas is live; gpu-glue.js's exact.postBound, Linux POST_BOUND, Apple Canvases.postBound
export const now = () => host.agentClock ?? performance.now() - t0;
export function postSurface(name, text, at) {
  if ([...views.values()].some((e) => e instanceof HTMLIFrameElement && e.isConnected && !retiredViews.has(e) && e.id === name)) {
    const refused = `postMessage: canvas name collides with an iframe id "${name}"`;
    log(refused);
    return refused;
  }
  if (globalThis.exact.gpu) {
    return globalThis.exact.gpu.post(name, text, at);
  }
  const held = globalThis.exact.pendingPosts ??= [];
  if (held.filter((p) => p.name === name).length < POST_BOUND) {
    return held.push({
      name,
      text,
      at,
      generation: host.incarnation
    }), null;
  }
  const refused = `postMessage: dropped: ${POST_BOUND} posts already wait for surface "${name}"`;
  log(refused);
  return refused;
}
// `postMessage(text, surface)` and the driver's `post` (LLP 1139.004 D1): held per surface name until one of its canvases is live, at most POST_BOUND; the refusal past it, or null
host.frameSampler = null;
// a development page's frame sampler (frames.js, LLP 1079 D3)
host.timelinesMoved = false;
// a batch's `timelines` op: its consumers are sought once it is applied
host.bootAttempt = 0;
host.devAssets = null;
host.installedFonts = [];
export function readOut(len) {
  const ptr = host.wasm.exact_out();
  return decoder.decode(new Uint8Array(host.memory.buffer, ptr, len));
}
export function writeIn(text) {
  const bytes = encoder.encode(text);
  const ptr = host.wasm.exact_in(bytes.length);
  new Uint8Array(host.memory.buffer, ptr, bytes.length).set(bytes);
  return bytes.length;
}
export function localAssetURL(source, assets = host.devAssets) {
  let url;
  try {
    url = new URL(source, document.baseURI);
  } catch {
    return source;
  }
  let name;
  try {
    name = decodeURIComponent(url.pathname.replace(/^\//, ""));
  } catch {
    return source;
  }
  if (assets !== null && url.origin === location.origin && /^(assets|deck|shaders)\//.test(name)) {
    const card = assets.get(name);
    if (!card) {
      return `/__dev/absent/${name.split("/").map(encodeURIComponent).join("/")}`;
    }
    if (card.objectURL) {
      return card.objectURL;
    }
    const resolved = new URL(card.url);
    resolved.search = url.search;
    resolved.hash = url.hash;
    return resolved.href;
  }
  if (/^\/\.exact\/root\/web\/releases\/[0-9a-f]{64}\/$/.test(new URL(document.baseURI).pathname) && /^\/(assets|deck|shaders)\//.test(source)) {
    return new URL("." + source, document.baseURI).href;
  }
  return source;
}
export function assetNamespace(cards) {
  const assets = new Map();
  const types = {
    mp4: "video/mp4",
    webm: "video/webm",
    vtt: "text/vtt",
    wav: "audio/wav",
    mp3: "audio/mpeg",
    m4a: "audio/mp4",
    aac: "audio/aac",
    ogg: "audio/ogg",
    oga: "audio/ogg",
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    svg: "image/svg+xml",
    gif: "image/gif",
    webp: "image/webp",
    woff2: "font/woff2",
    woff: "font/woff",
    ttf: "font/ttf",
    otf: "font/otf"
  };
  for (const [name, card] of cards) {
    const type = types[name.split(".").pop().toLowerCase()];
    assets.set(name, {
      ...card,
      objectURL: type ? URL.createObjectURL(new Blob([card.bytes], { type })) : null
    });
  }
  return assets;
}
export const lists = new Map();
host.listSelection = undefined;
host.listSelectionLoading = undefined;
export const pendingScrolls = new Map();
export const { followedScrolls, followScroll, settleFollow, rememberScroll, own: ownFollowed, release: releaseFollowed } = scrollFollowers(positionContexts);
root.addEventListener("pointerdown", (event) => {
  const target = event.target;
  const editor = target.closest?.("input, textarea, select") || target.isContentEditable;
  if (!editor && target.closest?.("[retainFocus=\"true\"]")) {
    event.preventDefault();
    return;
  }
  const button = target.closest?.("button");
  if (!button) {
    return;
  }
  for (const preview of root.querySelectorAll("[contextTarget]")) {
    if (contextPanel(preview)?.contains(button)) {
      event.preventDefault();
      return;
    }
  }
});
export const contextTransforms = new Set();
export const contextAnchors = new Map();
// preview view id -> mounted source and entry box
// @ref LLP 1039 D2, LLP 1061 D4 — viewport facts and display preferences, on every change, without debounce.
export const pageFacts = pageReporter(agentMode), pageChanged = () => {
  if (host.wasm?.exact_set_page && host.incarnation) {
    applyBatch(JSON.parse(readOut(host.wasm.exact_set_page(pageFacts.bits()))));
    if (host.wasm.exact_set_root_font_size) {
      applyBatch(JSON.parse(readOut(host.wasm.exact_set_root_font_size(pageFacts.rootFontSize()))));
    }
    if (host.wasm.exact_set_display_scale) {
      applyBatch(JSON.parse(readOut(host.wasm.exact_set_display_scale(pageFacts.displayScale()))));
    }
    // A size the runner has not been told goes with the geometry, in one
    // commit (LLP 1039 D9): the visual viewport's `resize` can come first.
    if (host.toldSize !== `${innerWidth}x${innerHeight}`) {
      resizeRunner();
    } else if (host.wasm.exact_set_viewport_geometry && document.body) {
      applyBatch(JSON.parse(readOut(host.wasm.exact_set_viewport_geometry(writeIn(pageGeometry())))));
    }
  }
};
pageFacts.onChange(pageChanged);
// @ref LLP 1039 D9 — the page's geometry as the browser resolves it: the safe-area insets, the segments and the keyboard's overlap
export const pageGeometry = () => {
  const env = environment();
  return JSON.stringify([["top", "right", "bottom", "left"].map(edge => env[`safe-area-inset-${edge}`]), fold().rects, env["keyboard-inset-height"]]);
};
// @ref LLP 1069.000 D2, D3 — the size, the preferences and, riding them, the page's geometry: a rotation's insets and size are one commit (LLP 1039 D9)
const resizeRunner = () => {
  if (host.wasm && host.incarnation && innerWidth > 0 && innerHeight > 0) {
    const told = host.wasm.exact_set_viewport_geometry && document.body ? writeIn(pageGeometry()) : 0;
    host.wasm.exact_set_update?.(updateCode());
    const batch = JSON.parse(readOut(host.wasm.exact_resize(innerWidth, innerHeight, now(), preferences() | foldBits(), told)));
    // Only a size the runner took is told: a refused one is asked again, never followed by geometry alone.
    if (!batch.error) {
      host.toldSize = `${innerWidth}x${innerHeight}`;
    }
    applyBatch(presence.resize(batch));
  }
};
// The window's `resize` comes here alone: the page's facts follow the size, never before it.
const mediaChanged = (event) => {
  if (event?.media?.startsWith("(prefers-contrast:")) reportColors();
  resizeRunner();
  pageChanged();
  requestAnimationFrame(positionContexts);
};
addEventListener("resize", mediaChanged);
onPreferences(mediaChanged);
export const foldChanged = mediaChanged;
onFold(foldChanged);
// @ref LLP 1078 D6 — the posture and the segment counts ride the facts word beside the preferences; the browser resolves the `env(viewport-segment-*)` lengths itself
visualViewport?.addEventListener("resize", () => { pageChanged(); requestAnimationFrame(positionContexts); });
onKeyboard(pageChanged); // LLP 1039 D9: the VirtualKeyboard API moves the keys without a resize
const symbolStyle = document.createElement("style");
document.head.append(symbolStyle);
// @ref LLP 1081 stage 2 — a symbol draws in its `color`: a zero-specificity default, so a native face's or a bar's own ink wins.
symbolStyle.textContent = "@property --exact-tint{syntax:\"<color>\";inherits:false;initial-value:#000}:where(img[data-symbol-path]){--exact-tint:currentcolor}img[data-symbol-path]{background-color:var(--exact-tint)!important;mask-image:var(--exact-symbol-mask);mask-repeat:no-repeat;mask-position:center;mask-size:var(--exact-symbol-fit,contain);mask-origin:content-box;mask-clip:content-box}";
// An image's `load` and `error` (LLP 1011 §2) as HTML `<img>` fires them, once per source, the error with its message (rt.js `imageEvent`); a symbol fires neither. A tinted raster paints
// through its CSS mask (element.rs `host_css`), a CORS fetch: from an origin that sends no CORS headers it paints nothing, so a CORS probe of the source decides. An adopted page's image may have settled before this attached.
export const IMAGE_ERROR = "the image did not load", CORS_ERROR = "a tinted image from another origin needs CORS (Access-Control-Allow-Origin)", imageProbes = new Map();
// The app's `value` into an editor as a person types: the changed middle only (`setRangeText` keeps the selection where a whole assignment throws the caret to the end), never mid-composition — held, applied at compositionend. LLP 1045 D5.
export const composing = new WeakSet(), heldValues = new WeakMap(), compositionFlush = new WeakMap();
for (const [type, on] of [["compositionstart", true], ["compositionend", false]]) {
  addEventListener(type, (e) => {
    e.target.exactComposing = on;
  }, true);
}
// which field composes, for a piece loaded later (LLP 1114.007)
host.markupModule = undefined;
host.decorationsModule = undefined;
export const markupPending = new WeakSet();
host.liveTimeModule = undefined;
export const loadLiveTime = (el) => (host.liveTimeModule ??= moduleReady.then(() => loadAfterPaint("./live-time-glue.js", "installLiveTime"))).then((install) => {
  if (el.isConnected && !retiredViews.has(el)) {
    install(el);
  }
}).catch((error) => console.error("exact: live time:", error));
export const loadDecorations = (el) => (host.decorationsModule ??= moduleReady.then(() => loadAfterPaint("./decorations.js", "installDecorations"))).then((install) => {
  if (el.isConnected && !retiredViews.has(el) && el.hasAttribute("data-decorations")) {
    install(el, log);
  }
}).catch((error) => console.error("exact: decorations:", error));
// a plain textarea's paint-only decorations (LLP 1114.007), after first paint; they follow their attributes and typing themselves
// LLP 1048.003 D4: `<html data-scrolldocument>` while an element is the page's scroller, and `data-exact-frame` on its ancestors while it is the one, which the shell's rule reads (never a `:has()` over the tree), as host/web-js/document.js `markDocument`.
host.pagePath = [];
export const scrollDocs = new Set(), markScrollDocument = () => {
  const live = [...scrollDocs].filter((el) => el.isConnected ? el.getAttribute("data-scrolldocument") === "true" : !scrollDocs.delete(el)), path = [];
  document.documentElement.toggleAttribute("data-scrolldocument", live.length > 0);
  for (let a = live.length === 1 ? live[0].parentElement : null; a && a.id !== "exact-root"; a = a.parentElement) {
    path.push(a);
  }
  for (const a of host.pagePath) {
    if (!path.includes(a)) {
      a.removeAttribute("data-exact-frame");
    }
  }
  for (const a of path) {
    if (!a.hasAttribute("data-exact-frame")) {
      a.setAttribute("data-exact-frame", "");
    }
  }
  host.pagePath = path;
};
visualViewport?.addEventListener("resize", syncViewportFit);
host.sending = 0;
// batches being applied: a dispatch run while another's applies (a focus it moved) is one deeper
// LLP 1016: grants come from one whole-set parse; `unparsed` names why an
// invalid full set admits nothing while retaining raw lines for child scopes.
// The agent's settle waits on active fetches; replies return to the wasm.
host.grants = [];
host.grantSet = null;
host.unparsed = "";
host.storageRequests = null;
export const inflight = new Set(), heldAsks = new Map();
// `heldAsks`: requests the server holds (LLP 1016.000 D9), ticket to target
export const controllers = new Set(), forgettable = new Map(), byTicket = new Map();
// `byTicket`: each request's controller, for the app's abort
host.incarnation = 0;
host.authHost = null;
// auth-glue.js once an auth.session grant asks for it (LLP 1069.006 D4)
// A ticket's work, in flight while the runner holds the ticket (LLP 1016 D5):
// a superseded or forgotten one never holds `clock settle`. Work without a
// ticket counts until it ends.
export function track(p, ticket) {
  if (ticket != null) {
    p.ticket = ticket;
    p.owner = host.incarnation;
  }
  inflight.add(p);
  p.finally(() => inflight.delete(p));
}
export const holds = (ticket, owner = host.incarnation) => owner === host.incarnation && host.wasm?.exact_request_active(ticket) === 1;
export const waiting = () => [...inflight].filter((p) => p.ticket == null || holds(p.ticket, p.owner));
export const heldNamed = (r) => {
  const h = [...heldAsks].filter(([ticket, e]) => holds(ticket, e.owner));
  return h.length && r && !r.error ? {
    ...r,
    held: h.length,
    heldTargets: h.map(([, e]) => e.target)
  } : r;
};
// a clock reply names the held asks the runner still holds
// After each commit: abort the reads whose tickets the runner let go of.
export function letGo() {
  for (const [controller, ticket] of forgettable) {
    if (!holds(ticket)) {
      forgettable.delete(controller);
      controller.abort();
    }
  }
}
export const HOST_WORK_BYTES = 16 * 1024 * 1024, HOST_WORK_BASE64 = 4 * Math.ceil(HOST_WORK_BYTES / 3);
export function surfaceGranted(op) {
  const admitted = host.grants.map((g) => g.trim()).filter(Boolean), scoped = op.scope == null ? admitted : op.scope.split("\n").map((g) => g.trim()).filter(Boolean);
  const need = `surface.${op.mode === "capture" ? "read" : "write"} ${op.name}`;
  return scoped.every((g) => admitted.includes(g)) && scoped.includes(need);
}
function fulfill(requestIncarnation, ticket, kind, status, headersText, body, elapsedMs) {
  // Boot reuses ticket IDs; an old incarnation's completion must not land.
  if (!host.wasm || requestIncarnation !== host.incarnation) {
    return;
  }
  if (Number.isFinite(elapsedMs)) {
    log(`reply ${ticket}: wall ${Math.max(0, Math.round(elapsedMs))} ms`);
  }
  // first: its `writeIn` reuses the buffer the reply fills
  const h = encoder.encode(headersText);
  const ptr = host.wasm.exact_in(h.length + body.length);
  const mem = new Uint8Array(host.memory.buffer, ptr, h.length + body.length);
  mem.set(h);
  mem.set(body, h.length);
  send(host.wasm.exact_fulfill(ticket, kind, status, h.length, body.length, now()));
}
export function safelyFulfill(...args) {
  try {
    fulfill(...args);
  } catch (e) {
    console.error("exact: request fulfillment failed", e);
  }
}
export const deferFulfill = deferredFulfill(safelyFulfill, inflight);
// The agent API's page half (LLP 1012). `tree`, `state`, `logs`, and
// `settle` go to the wasm (`exact_agent`); `layout` reads the browser's
// boxes — the only layout the web host has; `clock` moves both clocks to
// one instant: the runner's (`exact_advance`, each timer at its own time)
// and every animation the browser holds (`Animation.currentTime`, LLP 1002
// D3), and the GPU module's picture. `tap`, `type`, and `screenshot` are
// the driver's, over CDP: real input, real pixels.
export function ask(request) {
  // entering an unloaded stage traps mid-call; callers load it first (LLP 1047.000 §9)
  if (!stageLoaded("inspection")) {
    throw new Error("inspection is a stage that has not loaded (LLP 1047.000): load it before asking");
  }
  return JSON.parse(readOut(host.wasm.exact_agent(writeIn(JSON.stringify(request)))));
}
// Staged capabilities (the core's `exact.stages`; none unsplit): modules over its memory and tables, named by digest (LLP 1047.000 §9).
host.stages = {};
export const stageLoads = new Map(), stageLoaded = (name) => !host.stages[name] || stageLoads.get(name)?.loaded === true;
export function loadStage(name) {
  if (!host.stages[name] || !host.wasm) {
    return Promise.resolve();
  }
  let load = stageLoads.get(name);
  if (load) {
    return load.promise;
  }
  load = { loaded: false };
  stageLoads.set(name, load);
  return load.promise = WebAssembly.instantiateStreaming(fetch(new URL(host.stages[name], import.meta.url)), { primary: host.wasm }).then(() => {
    load.loaded = true;
  }, (error) => {
    stageLoads.delete(name);
    throw new Error(`the ${name} stage (${host.stages[name]}) did not load: ${error}`);
  });
}
// A line for the runner's journal (LLP 1012 §3): what the page refused, and why.
export function log(line) {
  if (host.wasm) {
    host.wasm.exact_log(writeIn(line));
  }
}
// `layout <node>` (LLP 1035.002 D1): the runner's rows and their sources
// for one node (`node`, answered in the wasm), then what the page knows —
// the box in the viewport and relative to its parent, the scroll and clip
// chains above it, whether it is hidden, inert, in the viewport or clipped
// away, the element that carries it — and the browser's own computed value
// of every inherited row: the oracle printed beside the kernel's answer.
// Spaces the page cannot observe (a window, a screen) are absent.
export const INHERITED_CSS = {
  text_color: "color",
  font_family: "font-family",
  font_size: "font-size",
  font_weight: "font-weight",
  font_style: "font-style",
  line_height: "line-height",
  letter_spacing: "letter-spacing",
  font_variant_numeric: "font-variant-numeric",
  font_variant_ligatures: "font-variant-ligatures",
  direction: "direction",
  white_space: "white-space",
  overflow_wrap: "overflow-wrap",
  text_align: "text-align",
  widows: "widows",
  orphans: "orphans"
};
// A field's value as agent output shows it: a password's is the runner's fixed mark, whatever its length (#134).
export const shownValue = (el) => el.type === "password" && el.value ? "•••" : el.value;
export const SETTLE_DEADLINE_MS = 2e4, REPLACE_SETTLE_MS = 1e3, GLIDE_BOUND_MS = 4e3;
// a glide: as a native host waits for a scroll view a finger left moving (agent.mjs reason `scroll`)
export const mediaAttached = (deadline, set = host.mediaAttaching) => Promise.race([Promise.all(set).then(() => true), new Promise((resolve) => setTimeout(() => resolve(false), Math.max(0, deadline - performance.now())))]);
// the media glue's attaches (syncMedia), bounded as requests are
host.ticker = null;
host.timerFactory = null;
host.toldOffset = null;
// The zone's offset follows the clock (habits F6): told again before an advance whose instant finds it changed (a DST
// change, a new zone), and after the agent's `clock`; an unchanged one is not told.
export const followOffset = (at) => {
  const [epoch, offset] = reportTime(at);
  if (host.wasm.exact_set_time && offset !== host.toldOffset) {
    host.toldOffset = offset;
    applyBatch(JSON.parse(readOut(host.wasm.exact_set_time(epoch, offset))));
  }
};
// Boot the app: the baked plan, or `bytes` carrying compatible state (the dev loop's restart, LLP 1007 §6).
host.mutation = Promise.resolve();
export function mutate(work) {
  const next = host.mutation.then(work);
  host.mutation = next.catch(() => {});
  return next;
}
// `agent`, `agentSettled` and `now` exist only in agent mode; a normal page uses the browser's clock.
host.ready = undefined;
globalThis.exact = {
  ...globalThis.exact,
  discardSecretWrite: (name) => pendingStore.discard(name),
  mutate,
  devFirst: () => devFirst(),
  pageFacts: Object.assign(pageFacts, { changed: () => pageChanged() }),
  abortWake: function wake() {
    if (wake.due || !host.wasm?.exact_collect_aborts) {
      return;
    }
    wake.due = true;
    setTimeout(() => {
      wake.due = false;
      send(host.wasm.exact_collect_aborts());
    });
  },
  keptChanged: () => {
    if (host.wasm?.exact_changed) {
      applyBatch(JSON.parse(readOut(host.wasm.exact_changed(writeIn("doc-watch:doc:/")))));
    }
  },
  // the GPU tier's reader and teller (gpu-tier.js); a fetch a realm aborted between batches (module-glue.js door 23); the kept documents' list changed (documents-glue.js, LLP 1069.010.000 D5)
  // @ref LLP 1038 D8/D11 — synchronous for the serialized popstate caller.
  navigate: (location) => {
    const nav = root.firstElementChild;
    if (!host.inputReady || !nav?.hasAttribute("navigationBack")) {
      return {
        ops: [],
        error: "no navigation root"
      };
    }
    const batch = JSON.parse(readOut(host.wasm.exact_dispatch(Number(nav.dataset.view), 14, writeIn(location), now())));
    applyBatch(batch);
    return batch;
  },
  // Queue a dev-plan event behind initial boot when it arrives while wasm is fetching,
  // so a reload is never acknowledged before it happens.
  reload: async (bytes, fresh = false) => {
    await host.ready;
    await moduleReady;
    if (host.logicInfo || host.activeModule) {
      throw new Error("module reload requires a paired generation");
    }
    return boot(bytes, host.devAssets, () => true, null, fresh);
  },
  reloadGeneration: async (bytes, cards, current, module = null, rust = null, letGo = false) => {
    // letGo: the dev loop's, which swaps with requests in flight
    await host.ready;
    await moduleReady;
    if (module && !host.logicInfo) {
      throw new Error("this web client has binary-bound logic; rebuild with the browser module executor");
    }
    if (!module && !rust && (host.logicInfo || host.activeModule)) {
      throw new Error("a module client requires a paired plan/module generation");
    }
    if (rust && globalThis.exact.compat?.inputs?.rustMode !== "browser") {
      throw new Error("Rust replacement is disabled in this client; rebuild it");
    }
    const assets = assetNamespace(cards);
    let candidate = null;
    try {
      if (module) {
        candidate = {
          ...module,
          realm: await host.moduleLoader.prepare(module, host.logicInfo)
        };
      }
      if (rust) {
        await loadRust();
        if (candidate) {
          const js = new TextEncoder().encode(JSON.stringify(candidate.realm.id));
          const payload = new Uint8Array(js.length + rust.module.length);
          payload.set(js);
          payload.set(rust.module, js.length);
          const decode = (bytes) => JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
          candidate = {
            ...candidate,
            receipt: new TextEncoder().encode(JSON.stringify({
              version: 1,
              kind: "mixed",
              javascript: decode(module.receipt),
              rust: decode(rust.receipt),
              javascriptBytes: js.length
            })),
            rust: payload
          };
        } else {
          candidate = {
            receipt: rust.receipt,
            rust: rust.module
          };
        }
      }
      if (candidate) {
        candidate.letGo = letGo;
      }
      return await boot(bytes, assets, current, candidate) !== null;
    } finally {
      if (host.devAssets !== assets) {
        releaseAssets(assets);
      }
      if (candidate && host.activeModule !== candidate) {
        candidate.realm?.dispose();
      }
    }
  },
  message: (el, text) => {
    const id = Number(el?.dataset.view);
    if (host.inputReady && el && views.get(id) === el && messageViews.has(id)) {
      send(host.wasm.exact_dispatch(id, 9, writeIn(text), now()));
    }
  },
  get devAssets() {
    return host.devAssets;
  },
  get ready() {
    return host.ready.then(async () => {
      await moduleReady;
      if (!host.inputReady) {
        throw new Error(root.dataset.error || "data executor not ready");
      }
      if (agentMode) {
        await agentInputReady();
      }
    });
  },
  ...agentMode ? {
    agent,
    agentSettled,
    now,
    worldCarry: globalThis.exactWorldCarry
  } : {},
  get wasm() {
    return host.wasm;
  },
  assetURL: localAssetURL,
  stages: () => Object.fromEntries(Object.keys(host.stages).map((name) => [name, stageLoaded(name) ? "loaded" : "staged"])),
  writeIn,
  readOut,
  send,
  views,
  root,
  generation: 0,
  pendingSurfaces: [],
  postBound: POST_BOUND
};
// The GPU module, on demand: a script element after a rendering opportunity
// (two animation-frame callbacks), never an eager import, and only when a
// canvas is on the page. Until one is, an agent reply has nothing to settle.
host.gpuLoading = null;
export const gpuInPlay = () => Boolean(host.gpuLoading || globalThis.exact.gpu || globalThis.exact.pendingSurfaces?.length);
export function dropSurface(id) {
  const queued = globalThis.exact.pendingSurfaces;
  if (queued) {
    globalThis.exact.pendingSurfaces = queued.filter((s) => s.id !== id);
  }
}
// a surface gone before its module loads asks none
export function loadGpuIfNeeded() {
  if (host.gpuLoading || !(globalThis.exact.pendingSurfaces ?? []).length) {
    return;
  }
  host.gpuLoading = loadAfterPaint(globalThis.exact.compat?.inputs?.gpuModules ? "./gpu-modules.js" : "./gpu-glue.js", "gpu").catch((error) => console.error("exact gpu:", error));
  // @ref LLP 1009 D6
}
host.ready = main();
host.ready.catch((e) => {
  console.error(e);
  root.dataset.error = String(e);
});
function navigate(location) {
  const batch = globalThis.exact.navigate(location);
  return /^NoHandler|no navigation root/.test(batch?.error ?? "") ? false : batch;
}
// @ref LLP 1038 D7/D8/D11 — the mirror observes the handler's synchronous commit. false: unheard, so Back is the runner's own (LLP 1115 D5)
const hostGo = (location) => {
  if (!host.inputReady) {
    return null;
  }
  const batch = JSON.parse(readOut(host.wasm.exact_host_go(writeIn(location), now())));
  applyBatch(batch);
  return batch;
};
// Any plan the wasm host runs may present a sheet (navigation/routes.js `sheetFrame`).
useSheets(sheetFrame);
// the runner's own `go` (LLP 1115.000 D4.3): Forward, and a link to a route (input-glue.js)
navigation.connect(root, navigate, log, (id) => {
  if (host.inputReady) {
    applyBatch(JSON.parse(readOut(host.wasm.exact_host_back(id, now()))));
  }
}, globalThis.exact.go = hostGo, (href) => !!host.wasm && host.wasm.exact_route_match(writeIn(href)) === 1, true,
  (location) => host.wasm ? readOut(host.wasm.exact_location_of(writeIn(location))) : location);
