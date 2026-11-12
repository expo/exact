// The runner's reserved sources on the JS target, each answered before the
// app's (`data.reserved`, as the entry answers `exactTime`) by the declared
// fields' names, and re-answered in one commit when the fact changes.
// Imported by an app's module only for the sources its plan declares.
// - `exactViewport` (LLP 1039 D1–D4, 1061 D4; runner/src/viewport.rs): the
//   page's own readings (`navigation.js`), on every resize (glue.js
//   `mediaChanged`).
// - `exactPage` (LLP 1069.000 D2; runner/src/page.rs): visibility, online,
//   share sheet, pickers, focus, from the web host's own `pageReporter`;
//   under the agent the drive's (`prefer page`).
// - `exactDelivery` (LLP 1030 D7; runner/src/delivery.rs): what the build
//   baked; a JS build links no update store, so nothing is ever staged.
// - `exactSurface` (LLP 1047 D3; runner/src/surface_record.rs): a GPU
//   surface's published record, decoded against each reader's shape.
import { data, Resources, commit, R, Refusal, Kept, say } from "./rt.js";
import { environment, keyboardInset, onKeyboard, displayMode, preferences, onPreferences, pageReporter, fold, foldEnv, onFold, preferFold } from "./navigation.js";

const again = (source, why) => () => commit(() => { for (const r of Resources) if (r.source === source) R(r); }, why);
// `readers`: resource name → [its fields, in its shape's order, …].
const byName = (readers, name, v) => readers[name][0].map(n => v[n]);

const CONTRAST = ["no-preference", "more", "less", "custom"];
const GAMUT = ["srgb", "p3", "rec2020", "rec2020"];
/** Every viewport fact, by the runner's names, as the page reads them now. */
export function viewportFacts(geometry = false) {
  const p = preferences(), f = fold();
  const env = geometry && globalThis.document?.body ? environment() : {};
  const [safeAreaTop, safeAreaRight, safeAreaBottom, safeAreaLeft] = ["top", "right", "bottom", "left"].map(edge => env[`safe-area-inset-${edge}`] ?? 0);
  const segments = (f.cols === 1 && f.rows === 1 ? [[0, 0, innerWidth, innerHeight]] : f.rects ?? [])
    .map(([x, y, width, height]) => ({ x, y, width, height }));
  return { width: innerWidth, height: innerHeight, safeAreaTop, safeAreaRight, safeAreaBottom, safeAreaLeft, segments, devicePixelRatio: Number.isFinite(globalThis.devicePixelRatio) && globalThis.devicePixelRatio > 0 ? globalThis.devicePixelRatio : null, prefersReducedMotion: !!(p & 1), prefersReducedTransparency: !!(p & 2),
    displayMode: displayMode(),
    prefersContrast: CONTRAST[(p >> 2) & 3], prefersColorScheme: p & 16 ? "dark" : "light",
    devicePosture: f.posture, horizontalViewportSegments: f.cols, verticalViewportSegments: f.rows, // @ref LLP 1078 D2, D6
    colorGamut: GAMUT[(p >>> 26) & 3], dynamicRange: p & (1 << 28) ? "high" : "standard", // @ref LLP 1100 D9
    pointer: p & 64 ? "none" : p & 32 ? "coarse" : "fine", hover: p & 128 ? "none" : "hover",
    accessibilityTextSize: false, // a browser has no accessibility text sizes (runner/src/runner/root_font.rs)
    keyboardHeight: keyboardInset(), // @ref LLP 1039 D9: `env(keyboard-inset-height)`
    gpuTier: reporter()?.read()["gpu-tier"] ?? "high" }; // @ref LLP 1139.003 D2: the page facts' tier; this target has no GPU module, so the screen's class, or the agent's
}
export function viewport(readers) {
  (data.reserved ??= {}).exactViewport = (_, a, name) => {
    const [names, shape] = readers[name];
    const facts = viewportFacts(names.some(n => n.startsWith("safeArea")));
    return shape ? shaped(shape, facts, "viewport", name) : byName(readers, name, facts);
  };
  if (typeof addEventListener !== "function") return;
  const changed = again("exactViewport", "viewport");
  addEventListener("resize", changed);
  globalThis.visualViewport?.addEventListener("resize", changed);
  onKeyboard(changed); // LLP 1039 D9
  onPreferences(changed);
  onFold(changed);
  viewportChanged = changed;
  shareReporter();
  // The agent's `prefer` fold group (LLP 1078 D7): an empty one re-reads the browser, a filled one is the substitute.
  (globalThis.exact ??= {}).fold = { prefer: (f) => { const env = preferFold(f && Object.keys(f).length ? f : null); changed(); return env; }, env: foldEnv };
}

// A release admits no agent mode (LLP 1069.007 D2): its build writes this false.
const AGENT_ADMITTED = true;
// One reporter per session for both sources (`exactViewport().gpuTier` is a
// page fact), and the agent's `prefer` page group (LLP 1069.000 D6) sets it,
// whichever of them the plan reads.
let pageFacts, viewportChanged = () => {}, pageChanged = () => {};
const reporter = () => pageFacts !== undefined ? pageFacts : (pageFacts = typeof document === "object" && document.createElement
  ? pageReporter(AGENT_ADMITTED && typeof location === "object" && new URLSearchParams(location.search).has("agent")) : null);
function shareReporter() {
  const facts = reporter();
  if (!facts) return;
  (globalThis.exact ??= {}).page = { prefer: p => { facts.prefer(p); pageChanged(); if (p?.["gpu-tier"]) viewportChanged(); return facts.read(); }, read: () => facts.read() };
}
export function page(readers) {
  const facts = reporter();
  (data.reserved ??= {}).exactPage = (_, a, name) => {
    // A render has no page: the bake's answer (runner/src/page.rs `Page::default`).
    const f = facts ? facts.read() : { "visibility-state": "visible", online: true, "can-share": false, "can-share-files": false, "can-share-pictures": false, "can-open-files": false, "has-focus": true, "navigation-type": "", persisted: false, restores: 0, "iframe-partition": false };
    return byName(readers, name, { visibilityState: f["visibility-state"], onLine: f.online, canShare: f["can-share"], canShareFiles: f["can-share-files"], canSharePictures: f["can-share-pictures"], canOpenFiles: f["can-open-files"], hasFocus: f["has-focus"], navigationType: f["navigation-type"], persisted: f.persisted, restores: f.restores, iframePartition: f["iframe-partition"] === true });
  };
  if (!facts) return;
  pageChanged = again("exactPage", "page");
  facts.onChange(pageChanged);
  shareReporter();
}

// `exactTime` (runner/src/time.rs) is answered by the entry (build.mjs), the reader's device on a page and the
// unknown date in a render, each reader by its own fields: readers of one source may declare different shapes.
let timeReaders = {};
export function time(readers) { timeReaders = readers; }
/** `facts`: every `exactTime` field, by the runner's names; the fields resource `name` declares, in its order. */
export const timeFields = (name, facts) => byName(timeReaders, name, facts);

/** Each reader's second entry: the value the build baked, if it baked one
 * (a plan built from source did not): the embedded answer. */
export function activities(readers) {
  (data.reserved ??= {}).exactActivities = (_, a, name) => byName(readers, name, { activities: [], frequentPushesEnabled: false, linked: false });
}

export function delivery(readers) {
  const embedded = { stream: "embedded", seq: 0, embeddedSeq: 0, staged: false, sunset: "", interpreted: [], compatibilityId: "" };
  (data.reserved ??= {}).exactDelivery = (_, a, name) => readers[name][1] ?? byName(readers, name, embedded);
}

/** `readers`: resource name → [surface name, the shape by field name
 * (names.js `types`)]. A record arrives from the GPU module (`gpu-glue.js`
 * calls the wasm host's `exact_surface_record`); the host's bytes are
 * `name` alone (disposed) or `name\0json`. */
export function canvasPerformance(readers) {
  const names = new Set(Object.values(readers).map(r => r[0]));
  (globalThis.exact ??= {}).canvasPerformanceRequested = name => names.has(name);
  surfaces(readers, "exactCanvasPerformance", "canvasPerformanceRecord", 16 * 1024);
}

export function surfaces(readers, source = "exactSurface", report = "surfaceRecord", limit = MAX_RECORD) {
  const records = new Map(), refusals = new Map(), names = new Set(Object.values(readers).map(r => r[0]));
  const kept = source === "exactSurface" ? surfaceKeeps(readers) : null;
  let why = null;
  (data.reserved ??= {})[source] = (_, args, name) => {
    const [surface, shape] = readers[name];
    const text = records.get(surface);
    if (text === undefined) { const seed = kept?.seed(name); if (seed !== undefined) return seed; }
    let json;
    try {
      if (text != null) {
        // surface_record.rs MAX_BYTES (UTF-8 bytes; a third of it in UTF-16 units cannot exceed it).
        const bytes = text.length > limit / 3 ? new TextEncoder().encode(text).length : 0;
        if (bytes > limit) throw new Refusal(`${name}: record is ${bytes} bytes, over the ${limit}-byte limit`);
        try { json = JSON.parse(text); } catch (e) { throw new Refusal(`${name}: ${e.message}`); }
        if (!json || typeof json !== "object" || Array.isArray(json)) throw new Refusal(`${name}: record: expected JSON object`);
      }
      return shaped(shape, json, "record", name);
    } catch (e) { why = String(e?.message ?? e); throw e; }
  };
  const x = globalThis.exact ??= {};
  x[report] = (surface, json) => {
    if (!names.has(surface)) return;
    const was = records.get(surface) ?? null;
    if (was === (json ?? null)) return;
    const put = v => v == null ? records.delete(surface) : records.set(surface, v);
    put(json);
    // A record a reader refuses leaves the last one standing; `state.surfaceRefusals` says why.
    why = null;
    if (!commit(() => { for (const r of Resources) if (r.source === source && readers[r.name]?.[0] === surface) R(r); }, `surface ${surface}`)) {
      put(was); refusals.set(surface, why ?? "refused");
    } else {
      refusals.delete(surface);
      kept?.published(surface, json, name => data.reserved[source](null, null, name));
    }
  };
  x[report === "surfaceRecord" ? "surfaceRefusals" : "canvasPerformanceRefusals"] = () => Object.fromEntries(refusals);
}
const MAX_RECORD = 16 << 20;
// surface_record.rs `shaped`: absent fields are their type's zero, extra keys
// are ignored, anything else of the wrong kind is refused by its path.
function shaped(t, v, path, name) {
  const fail = () => { throw new Refusal(`${name}: ${path}: expected ${Array.isArray(t) ? (t[0] === "?" ? "Option" : "List") : typeof t === "string" ? KIND[t] : "Record"}`); };
  if (typeof t === "string") {
    if (v === undefined) return t === "n" ? 0 : t === "b" ? false : t === "s" ? "" : null;
    if (t === "u") return v === null ? null : fail();
    return (t === "n" && typeof v === "number" && Number.isFinite(v)) || (t === "b" && typeof v === "boolean") || (t === "s" && typeof v === "string") ? v : fail();
  }
  if (Array.isArray(t) && t[0] === "e") return v === undefined ? t[1][0] : typeof v === "string" ? v : fail();
  if (t[0] === "?") return v == null ? null : shaped(t[1], v, path, name);
  if (Array.isArray(t)) {
    if (v === undefined) return [];
    return Array.isArray(v) ? v.map((x, i) => shaped(t[1], x, `${path}[${i}]`, name)) : fail();
  }
  if (v !== undefined && (v === null || typeof v !== "object" || Array.isArray(v))) fail();
  return Object.keys(t).map(k => shaped(t[k], v?.[k], `${path}.${k}`, name));
}
const KIND = { n: "Number", b: "Bool", s: "String", u: "Unit" };


// The launch cache contains only typed values from successful publications.
function surfaceKeeps(readers) {
  const prefix = Kept('kept'), seeds = new Map(), live = new Set(), wanted = new Set(Object.entries(readers).filter(([, r]) => r[2]).map(([name]) => name));
  const limit = 256 * 1024, totalLimit = 2 * (limit + 5), bytes = text => new TextEncoder().encode(text).length;
  const entry = (name, text) => {
    if (typeof text !== 'string' || bytes(text) > 6 * (limit + 5)) throw Error('too large');
    const [surface, signature, value] = JSON.parse(text), row = readers[name];
    if (!row || !row[2] || surface !== row[0] || signature !== row[2] || !keptFits(row[1], value)) throw Error('shape changed');
    const size = keptSize(row[1], value) + 5 + bytes(surface) + signature.length / 2;
    if (size > limit) throw Error('too large');
    return {value, size:size + 5};
  };
  let cleared = false;
  const available = () => {
    try { if (globalThis.__exactRender || typeof localStorage === 'undefined') return false; } catch { return false; }
    if (data.keepSurfaceAnswers !== false) return true;
    if (!cleared) { cleared = true; try { for (const key of entries()) localStorage.removeItem(key); } catch {} }
    return false;
  };
  const entries = () => Array.from({length: localStorage.length}, (_, i) => localStorage.key(i)).filter(k => k?.startsWith(prefix));
  const remove = name => { try { if (available()) localStorage.removeItem(prefix + name); } catch {} };
  try { if (available()) for (const key of entries()) if (!wanted.has(key.slice(prefix.length))) localStorage.removeItem(key); } catch {}
  return {
    seed(name) {
      if (!wanted.has(name) || live.has(readers[name][0]) || !available()) return undefined;
      if (!seeds.has(name)) {
        let value;
        try {
          const text = localStorage.getItem(prefix + name);
          if (text !== null) {
            value = entry(name, text).value;
          }
        } catch { remove(name); }
        seeds.set(name, value);
      }
      return seeds.get(name);
    },
    published(surface, json, read) {
      live.add(surface);
      for (const [name, row] of Object.entries(readers)) {
        if (row[0] !== surface || !wanted.has(name)) continue;
        seeds.delete(name);
        if (json == null || !available()) continue;
        const value = withoutHostFacts(row[1], read(name)), text = `[${JSON.stringify(surface)},${JSON.stringify(row[2])},${keptEncode(row[1], value)}]`;
        let size;
        try { size = entry(name, text).size; } catch { remove(name); say(`too big to keep: resource ${name}`); continue; }
        try {
          const key = prefix + name, others = entries().filter(k => k !== key).flatMap(k => {
            try { return [[k, entry(k.slice(prefix.length), localStorage.getItem(k)).size]]; } catch { localStorage.removeItem(k); return []; }
          }).sort((a, b) => b[1] - a[1]);
          let total = size + others.reduce((n, [, size]) => n + size, 0);
          for (const [other, size] of others) { if (total <= totalLimit) break; localStorage.removeItem(other); total -= size; }
          if (localStorage.getItem(key) !== text) localStorage.setItem(key, text);
        } catch (e) { say(`kept surface ${name}: ${e}`); }
      }
    }
  };
}
// The members a host adds to a surface's record (a GPU canvas's `presented`, LLP 1009 D9) are this
// launch's: a kept record holds each as absent, as the runner's `HOST_FIELDS`.
function withoutHostFacts(t, v) {
  if (typeof t !== 'object' || Array.isArray(t) || !Object.hasOwn(t, 'presented')) return v;
  const at = Object.keys(t).indexOf('presented'), absent = t.presented === 'b' ? false : null;
  return v.map((x, i) => i === at ? absent : x);
}
function keptFits(t, v) {
  if (typeof t === 'string') return t === 'n' ? typeof v === 'number' && Number.isFinite(v) : t === 's' ? typeof v === 'string' : t === 'b' ? typeof v === 'boolean' : v === null;
  if (t[0] === '?') return v === null || keptFits(t[1], v);
  if (t[0] === 'e') return typeof v === 'string' && t[1].includes(v);
  if (Array.isArray(t)) return Array.isArray(v) && v.every(x => keptFits(t[1], x));
  const fields = Object.values(t); return Array.isArray(v) && v.length === fields.length && fields.every((f, i) => keptFits(f, v[i]));
}
function keptSize(t, v) {
  if (t === 'n') return 9;
  if (t === 'b') return 2;
  if (t === 's' || t[0] === 'e') return 5 + new TextEncoder().encode(v).length;
  if (t === 'u') return 1;
  if (t[0] === '?') return 1 + (v === null ? 0 : keptSize(t[1], v));
  if (Array.isArray(t)) return 5 + v.reduce((n, x) => n + keptSize(t[1], x), 0);
  return 5 + Object.values(t).reduce((n, f, i) => n + keptSize(f, v[i]), 0);
}

function keptEncode(t, v) {
  if (t === 'n') return Object.is(v, -0) ? '-0' : JSON.stringify(v);
  if (typeof t === 'string' || t[0] === 'e') return JSON.stringify(v);
  if (t[0] === '?') return v === null ? 'null' : keptEncode(t[1], v);
  if (Array.isArray(t)) return `[${v.map(x => keptEncode(t[1], x)).join(',')}]`;
  return `[${Object.values(t).map((f, i) => keptEncode(f, v[i])).join(',')}]`;
}
