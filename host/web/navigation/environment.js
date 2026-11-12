import { beneathApp } from "./type.js";
import { AGENT_ADMITTED, agentParameters } from "./routes.js";
// Read safe-area insets from probe padding and keyboard height from the visual viewport
let probe;
export function environment() {
  if (!probe) {
    probe = document.createElement("div");
    probe.style.cssText = "position:fixed;inset:0;visibility:hidden;pointer-events:none;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)";
    document.body.append(probe);
  }
  const cs = getComputedStyle(probe);
  return {
    "safe-area-inset-top": r2(parseFloat(cs.paddingTop) || 0),
    "safe-area-inset-right": r2(parseFloat(cs.paddingRight) || 0),
    "safe-area-inset-bottom": r2(parseFloat(cs.paddingBottom) || 0),
    "safe-area-inset-left": r2(parseFloat(cs.paddingLeft) || 0),
    "keyboard-inset-height": keyboardInset(),
    ...foldEnv()
  };
}
/** `env(keyboard-inset-height)` (LLP 1039 D9), as the browser reports it: the VirtualKeyboard API's rect where the page overlays the keyboard, else how far the visual viewport falls short of the layout viewport's bottom. Under `interactive-widget=resizes-content` without the VirtualKeyboard API the keys shrink the page itself, which then has nothing over it: 0. */
export function keyboardInset(platform = globalThis) {
  const rect = platform.navigator?.virtualKeyboard?.boundingRect;
  if (rect?.height > 0) {
    return r2(rect.height);
  }
  const vv = platform.visualViewport;
  return vv ? r2(Math.max(0, platform.innerHeight - vv.height - vv.offsetTop)) : 0;
}
/** `changed` when the keyboard moves without a resize: the VirtualKeyboard API's `geometrychange`. */
export function onKeyboard(changed, platform = globalThis) {
  platform.navigator?.virtualKeyboard?.addEventListener?.("geometrychange", changed);
}
// Read CSS when the JS fold interfaces are absent (LLP 1078 D6). Share agent substitutes with agent-navigation.js; they change facts, not CSS env() resolution (D7).
const r2 = (x) => Math.round(x * 100) / 100;
function readFold() {
  const posture = globalThis.navigator?.devicePosture?.type ?? (foldMedia("(device-posture: folded)") ? "folded" : "continuous");
  const segments = globalThis.viewport?.segments;
  if (Array.isArray(segments) && segments.length >= 2) {
    const rects = segments.map((s) => [
      s.x,
      s.y,
      s.width,
      s.height
    ]).sort((a, b) => a[1] - b[1] || a[0] - b[0]);
    return {
      posture,
      cols: new Set(rects.map((r) => r[0])).size,
      rows: new Set(rects.map((r) => r[1])).size,
      rects
    };
  }
  const cols = foldCount("horizontal"), rows = foldCount("vertical");
  return {
    posture,
    cols,
    rows,
    rects: foldRects(cols, rows)
  };
}
const foldQueries = new Map(), foldListeners = new Set();
let foldProbe;
function foldMedia(query) {
  let list = foldQueries.get(query);
  if (!list && typeof globalThis.matchMedia === "function") {
    list = matchMedia(query);
    foldQueries.set(query, list);
    list.addEventListener("change", () => {
      for (const changed of foldListeners) {
        changed();
      }
    });
  }
  return list?.matches === true;
}
function foldCount(axis) {
  let count = 1;
  while (count < 255 && foldMedia(`(${axis}-viewport-segments >= ${count + 1})`)) {
    count++;
  }
  return count;
}
function foldRects(cols, rows) {
  if (cols * rows === 1 || !globalThis.document?.body || typeof globalThis.getComputedStyle !== "function") {
    return [];
  }
  if (!foldProbe) {
    foldProbe = document.createElement("div");
    foldProbe.style.cssText = "position:fixed;width:0;height:0;visibility:hidden;pointer-events:none;contain:strict";
    document.body.append(foldProbe);
  }
  const rects = [], fields = [
    "left",
    "top",
    "width",
    "height"
  ];
  for (let row = 0; row < rows; row++) {
    for (let column = 0; column < cols; column++) {
      for (const field of fields) {
        foldProbe.style.setProperty(`--exact-segment-${field}`, `env(viewport-segment-${field} ${column} ${row}, -1px)`);
      }
      const style = getComputedStyle(foldProbe), rect = fields.map((field) => parseFloat(style.getPropertyValue(`--exact-segment-${field}`)));
      if (!rect.every((value) => Number.isFinite(value) && value >= 0)) {
        return [];
      }
      rects.push(rect);
    }
  }
  return rects;
}
export const fold = () => globalThis.exact?.foldSubstitute ?? readFold();
/** layout.env's names (LLP 1012 section 1). */
export function foldEnv() {
  const f = fold();
  return {
    "device-posture": f.posture,
    "horizontal-viewport-segments": f.cols,
    "vertical-viewport-segments": f.rows,
    "viewport-segments": f.rects.map((r) => r.map(r2))
  };
}
/** LLP 1078 D6: bit 8 is folded, bits 9–16 count columns, 17–24 count rows; bit 25 is reserved. */
export function foldBits() {
  const f = fold();
  return (f.posture === "folded" ? 256 : 0) | (f.cols & 255) << 9 | (f.rows & 255) << 17;
}
export function onFold(changed) {
  // Install media queries even when no resource reads viewport facts.
  readFold();
  foldListeners.add(changed);
  globalThis.navigator?.devicePosture?.addEventListener?.("change", changed);
  addEventListener("resize", changed);
}
/** The grid a host without a fold makes for `prefer segments <cols>x<rows> [gap <points>]`: the viewport split evenly, the gap centred on each divider; refused by name. */
export function evenSegments(width, height, cols, rows, gap = 0) {
  if (!(cols >= 1 && rows >= 1)) {
    throw Object.assign(new Error(`segments ${cols}x${rows}: each count is at least 1`), { malformed: true });
  }
  if (!(Number.isFinite(gap) && gap >= 0)) {
    throw Object.assign(new Error(`segments: gap ${gap} is not a non-negative length`), { malformed: true });
  }
  if (cols * rows === 1) {
    return [];
  }
  const span = (total, n) => {
    const bands = (n - 1) * gap;
    if (bands >= total) {
      throw new Error(`segments ${cols}x${rows} gap ${gap}: the gap is wider than the viewport (${width} × ${height})`);
    }
    return (total - bands) / n;
  };
  const w = span(width, cols), h = span(height, rows), out = [];
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      out.push([
        x * (w + gap),
        y * (h + gap),
        w,
        h
      ]);
    }
  }
  return out;
}
/** The agent's substitute (`prefer`'s `fold` group): `posture`, `cols`, `rows`, `gap`; `null` drops it and the browser's own readings return. */
export function preferFold(request) {
  if (request == null) {
    (globalThis.exact ??= {}).foldSubstitute = null;
    return foldEnv();
  }
  const next = { ...fold() };
  let gap = 0, grid = false;
  for (const [name, raw] of Object.entries(request)) {
    const n = Number(raw);
    switch (name) {
      case "posture":
        if (raw !== "folded" && raw !== "continuous") {
          throw Object.assign(new Error(`prefer: posture: ${raw} is folded or continuous`), { malformed: true });
        }
        next.posture = raw;
        break;
      case "cols":
      case "rows":
        if (!(Number.isInteger(n) && n >= 1)) {
          throw Object.assign(new Error(`prefer: segments: ${raw} ${name === "cols" ? "columns" : "rows"} is not a count`), { malformed: true });
        }
        next[name] = n;
        grid = true;
        break;
      case "gap":
        if (!(Number.isFinite(n) && n >= 0)) {
          throw Object.assign(new Error(`prefer: segments: gap ${raw} is not a length`), { malformed: true });
        }
        gap = n;
        grid = true;
        break;
      default: throw Object.assign(new Error(`prefer: ${name} is not a fold fact this host sets`), { malformed: true });
    }
  }
  if (grid) {
    try {
      next.rects = evenSegments(innerWidth, innerHeight, next.cols, next.rows, gap);
    } catch (e) {
      throw Object.assign(new Error(`prefer: ${e.message}`), { malformed: e.malformed });
    }
  }
  (globalThis.exact ??= {}).foldSubstitute = next;
  return foldEnv();
}
// @ref LLP 1061 D4, LLP 1069.000 D1: media facts ride each boot and resize.
// Bits 0–4: motion, transparency, contrast (more|less), dark; 5–7: coarse|none pointer, no hover.
const DISPLAY_MODES = [
  "standalone",
  "browser",
  "fullscreen",
  "minimal-ui",
  "window-controls-overlay",
  "picture-in-picture",
  "tabbed"
];
let preferenceQueries;
// Bits 8–25 belong to foldBits(); 26–27 gamut, 28 HDR (LLP 1100 D9), 29–31 display-mode (LLP 1039 D7).
const queries = () => preferenceQueries ??= [
  ["(prefers-reduced-motion: reduce)", 1],
  ["(prefers-reduced-transparency: reduce)", 2],
  ["(prefers-contrast: more)", 4],
  ["(prefers-contrast: less)", 8],
  ["(prefers-contrast: custom)", 12],
  ["(prefers-color-scheme: dark)", 16],
  ["(pointer: coarse)", 32],
  ["(pointer: none)", 64],
  ["(hover: none)", 128],
  ["(color-gamut: p3)", 1 << 26],
  ["(color-gamut: rec2020)", 1 << 27],
  ["(dynamic-range: high)", 1 << 28],
  ...DISPLAY_MODES.map((v, i) => [`(display-mode: ${v})`, i << 29])
].map(([q, bits]) => [matchMedia(q), bits]);
export const preferences = () => {
  const bits = queries().reduce((bits, [q, bit]) => bits | (q.matches ? bit : 0), 0), meta = globalThis.document?.querySelector?.("meta[name=\"color-scheme\"]")?.content, pinned = globalThis.exact?.systemScheme ?? (meta === "light" || meta === "dark" ? meta : null);
  return pinned ? bits & ~16 | (pinned === "dark" ? 16 : 0) : bits;
};
// a dev pin's scheme is the system's (LLP 1122 D4; dev.js), else the manifest's fixed `userInterfaceStyle` (the shell's `<meta name="color-scheme">`, look.mjs), as `UIUserInterfaceStyle` is an iOS app's
export const displayMode = () => DISPLAY_MODES[preferences() >>> 29 & 7];
// @ref LLP 1101.003 D6 — Media Queries 4's `update`: 0 `fast` (a screen, and a page that cannot ask), 1 `slow`, 2 `none` (print). The preference bits have no room for it; the wasm host takes it through `exact_set_update`.
let updateQueries;
const updates = () => updateQueries ??= ["(update: slow)", "(update: none)"].map(q => globalThis.matchMedia?.(q));
export const UPDATES = ["fast", "slow", "none"];
export const updateCode = () => updates()[1]?.matches ? 2 : updates()[0]?.matches ? 1 : 0;
/** A phone-sized screen (its short side under 600 CSS px): the GPU tier's class when the adapter says nothing it knows, and never `high` (LLP 1139.003 D2). */ export const handheld = (platform = globalThis) => Math.min(platform.screen?.width ?? Infinity, platform.screen?.height ?? Infinity) < 600;
export const onPreferences = (changed) => {
  queries().forEach(([q]) => q.addEventListener("change", changed));
  updates().forEach(q => q?.addEventListener?.("change", changed));
  // A window can move between displays without changing its logical size.
  let resolution;
  const watch = () => {
    resolution?.removeEventListener?.("change", rescaled);
    resolution = globalThis.matchMedia?.(`(resolution: ${globalThis.devicePixelRatio || 1}dppx)`);
    resolution?.addEventListener("change", rescaled);
  };
  const rescaled = () => { watch(); changed(); };
  watch();
};
// LLP 1069.000 D2/D6: pack page facts; agent preferences replace browser readings except display scale.
// Picture sharing is agent-only; persisted pageshow events count back/forward cache restores.
export function pageReporter(agent, platform = globalThis) {
  const facts = {
    "visibility-state": "visible",
    online: true,
    "can-share": true,
    "can-share-files": true,
    "can-share-pictures": false,
    "can-open-files": true,
    "has-focus": true,
    "root-font-size": 16,
    "gpu-tier": "high",
    "iframe-partition": false
  }, cache = {
    persisted: false,
    restores: 0
  };
  // cache: the drive's page is never put away
  let sharesFiles;
  const canShareFiles = () => sharesFiles ??= (() => {
    try {
      return typeof platform.navigator.canShare === "function" && platform.navigator.canShare({ files: [new platform.File([""], "a.txt", { type: "text/plain" })] }) === true;
    } catch {
      return false;
    }
  })();
  // LLP 1069.000 D3: read the browser root font beneath the app override; the agent supplies its preference.
  const rootFontSize = () => agent ? facts["root-font-size"] : beneathApp(platform, () => parseFloat(platform.getComputedStyle(platform.document.documentElement).fontSize) || 16), displayScale = () => platform.devicePixelRatio > 0 ? platform.devicePixelRatio : 1;
  // @ref LLP 1115.000 D10.3 — the display's scale is the page's own `devicePixelRatio`, read under the agent too: the drive sets it through the browser's emulation (`prefer display-scale`), never by a substitute, and a zoom fires `resize`, which tells it again
  const loaded = agent ? "" : platform.performance?.getEntriesByType?.("navigation")?.[0]?.type;
  let told;
  const gpuTier = () => told ?? (handheld(platform) ? "low" : "high");
  // @ref LLP 1139.003 D2 — the GPU tier: the adapter's, told by gpu-glue.js before the device is made (gpu-tier.js), else the screen's class; the agent's is `high` until `prefer gpu-tier`
  const navigationType = ["reload", "back_forward"].includes(loaded) ? loaded : "navigate";
  const read = () => agent ? {
    ...facts,
    "navigation-type": navigationType,
    ...cache
  } : {
    ...cache,
    "visibility-state": platform.document.visibilityState === "hidden" ? "hidden" : "visible",
    online: platform.navigator.onLine !== false,
    "can-share": typeof platform.navigator.share === "function",
    "can-share-files": canShareFiles(),
    "can-share-pictures": false,
    "can-open-files": typeof platform.showOpenFilePicker === "function",
    "has-focus": typeof platform.document.hasFocus !== "function" || platform.document.hasFocus(),
    "navigation-type": navigationType,
    "gpu-tier": gpuTier(),
    "iframe-partition": false
  };
  // iframe-partition: a browser never honours an iframe's `partition` (LLP 1020.002 Q4)
  const bits = () => {
    const f = read();
    return ((f["visibility-state"] === "hidden" ? 1 : 0) | (f.online ? 0 : 2) | (f["can-share"] ? 4 : 0) | (f["can-open-files"] ? 8 : 0) | (f["has-focus"] ? 0 : 16) | (f["can-share-files"] ? 256 : 0) | (f["can-share-pictures"] ? 512 : 0) | [
      "navigate",
      "reload",
      "back_forward"
    ].indexOf(f["navigation-type"]) + 1 << 5 | (f.persisted ? 1024 : 0) | Math.max(0, [
      "high",
      "mid",
      "low"
    ].indexOf(f["gpu-tier"])) << 11 | (f["iframe-partition"] ? 8192 : 0) | (f.restores & 65535) << 16) >>> 0;
  };
  const prefer = (page) => {
    const next = { ...facts };
    for (const [name, raw] of Object.entries(page ?? {})) {
      const value = String(raw);
      if (name === "visibility-state" && (value === "visible" || value === "hidden") || name === "gpu-tier" && [
        "low",
        "mid",
        "high"
      ].includes(value)) {
        next[name] = value;
      } else if ((name === "online" || name === "can-share" || name === "can-share-files" || name === "can-share-pictures" || name === "can-open-files" || name === "has-focus" || name === "iframe-partition") && (value === "true" || value === "false")) {
        next[name] = value === "true";
      } else if (name === "root-font-size" && Number(value) > 0 && Number.isFinite(Number(value))) {
        next[name] = Number(value);
      } else if (name === "display-scale" && Number(value) > 0 && Number.isFinite(Number(value))) {
        if (Math.abs(displayScale() - Number(value)) > .01) {
          throw new Error(`prefer: display-scale: ${value}: the page's is ${displayScale()}, which the browser sets (its own emulation, or the device)`);
        }
      } else {
        throw Object.assign(new Error(`prefer: ${name}: ${value} is not a page fact this host sets`), name === "keyboard-navigation" || name === "bold-text" ? {} : { malformed: true });
      }
    }
    Object.assign(facts, next);
    if (page?.["root-font-size"] !== undefined) {
      const { style } = platform.document.documentElement;
      style.fontSize = `${facts["root-font-size"]}px`;
      style.setProperty("--exact-host-font-size", style.fontSize);
    }
    // the size an app's percentage is of
  };
  const onChange = (changed) => {
    if (agent) {
      return;
    }
    platform.document.addEventListener("visibilitychange", changed);
    for (const name of [
      "online",
      "offline",
      "focus",
      "blur"
    ]) {
      platform.addEventListener(name, changed);
    }
    for (const name of ["pagehide", "pageshow"]) {
      platform.addEventListener(name, (e) => {
        if (!e.persisted) {
          return;
        }
        cache.persisted = name === "pagehide";
        if (name === "pageshow") {
          cache.restores = cache.restores + 1 & 65535;
        }
        changed();
      });
    }
  };
  return {
    bits,
    read: () => ({
      ...read(),
      "root-font-size": rootFontSize(),
      "display-scale": displayScale()
    }),
    prefer,
    onChange,
    rootFontSize,
    displayScale,
    agent,
    tell: (tier) => {
      if (!agent) {
        told = tier;
      }
    }
  };
}
// The page launch owns its seed, a whole number below 2^32 on every host (LLP 1116.005 D5); a new runner during
// development reuses it. Agent facts are supplied by the drive, never by the browser's environment.
export function placeReporter(params, platform = globalThis) {
  let seed;
  return () => {
    if (AGENT_ADMITTED && params.has("agent")) {
      const value = Number(params.get("seed") ?? 1);
      if (!Number.isSafeInteger(value) || value < 0 || value >= 2 ** 32) {
        throw new Error("seed: a whole number from 0 through 2^32 - 1 (4294967295)");
      }
      return [
        params.get("locale") ?? "en-US",
        params.get("timeZone") ?? "UTC",
        value,
        params.get("formatLocale") ?? ""
      ].join("\0");
    }
    if (seed === undefined) {
      seed = platform.crypto.getRandomValues(new Uint32Array(1))[0];
    }
    return [
      platform.navigator.language,
      platform.Intl.DateTimeFormat().resolvedOptions().timeZone,
      seed,
      platform.Intl.DateTimeFormat().resolvedOptions().locale
    ].join("\0");
    // the last: ECMA-402's default locale, which a browser's formatters use
  };
}
// LLP 1027.000.000 D3: agent time starts at the drive epoch; clock movement updates the date and zone offset.
export function timeReporter(params, platform = globalThis) {
  return (elapsed) => {
    if (!(AGENT_ADMITTED && params.has("agent"))) {
      return [platform.Date.now() - elapsed, -new platform.Date().getTimezoneOffset()];
    }
    const epoch = Number(params.get("epoch") ?? Date.UTC(2026, 0, 1));
    if (!Number.isSafeInteger(epoch) || epoch < 0) {
      throw new Error("epoch: Unix milliseconds at or after 1970");
    }
    const at = {};
    for (const { type, value } of new Intl.DateTimeFormat("en-US", {
      timeZone: params.get("timeZone") ?? "UTC",
      hourCycle: "h23",
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "numeric",
      second: "numeric"
    }).formatToParts(epoch + elapsed)) {
      at[type] = Number(value);
    }
    return [epoch, (Date.UTC(at.year, at.month - 1, at.day, at.hour, at.minute, at.second) - Math.floor((epoch + elapsed) / 1e3) * 1e3) / 6e4];
  };
}
// LLP 1038 D5: launch at the page path, query and fragment, excluding agent-only parameters.
export function launchLocation(platform = globalThis) {
  const { pathname, search, hash = "" } = platform.location, q = new URLSearchParams(search);
  // the fragment is the launch's too (LLP 1153 D5)
  if (!(AGENT_ADMITTED && q.has("agent"))) {
    return pathname + search + hash;
  }
  for (const k of agentParameters) {
    q.delete(k);
  }
  const rest = q.toString();
  return pathname + (rest ? "?" + rest : "") + hash;
}
// Drive facts use the launch URL after route changes too (storage-environment.js `launchHref`).
const launched = () => new URL(globalThis.performance?.getEntriesByType?.("navigation")[0]?.name ?? location.href).searchParams;
let pageTime;
export const reportTime = (elapsed) => (pageTime ??= timeReporter(launched()))(elapsed);
let pagePlace;
export const reportPlace = () => (pagePlace ??= placeReporter(launched()))();
export { launched };
