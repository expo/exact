// The host commands the JS target carries beyond rt.js's own, each as the
// wasm web host runs it (host/web/glue.js, navigation.js), so a command the
// compiler admits (contract/types/src/checks.rs `HOST_COMMANDS`) is one this
// runtime carries (files diary F5: `selectText` was refused here).
// Each is an export of its own name, a factory over `say` (the journal) and
// `agentMode` (rt.js's, whether the agent drives the page): the compiled plan
// imports the ones its actions name and hands them to rt.js's `useCommands`
// (emit.rs, LLP 1047.002 D1), so a command no action names is never linked.
// The compiler reads this file's `export const` names as it is built.
import { modalCommand, popoverCommand, appRootFontSize, liveById, declaredScheme, setFieldSelection, setFieldRangeText } from "./navigation.js";
import { Kept, DEVELOPMENT_STORE, Hosts, KeyEvent, Propagating, clock } from './rt.js';
// A launch service can install its handlers before app() links the commands (LLP 1172.011 E1).
export const observe = say => Hosts.observe ?? (name => say(`observe ${name}`));
export const observeAttributes = () => Hosts.observeAttributes ?? (() => {});
export const observeError = say => Hosts.observeError ?? (message => say(`observeError ${message}`));
export const observeFeedback = say => Hosts.observeFeedback ?? (id => say(`observeFeedback ${id}`));
// Commands run before the handler returns; beforeunload also needs Safari's returnValue.
export const preventDefault = () => () => {
  KeyEvent?.preventDefault();
  if (KeyEvent?.type === 'beforeunload') KeyEvent.returnValue = '';
  if (!KeyEvent && Propagating?.type === 'pointerdown') Propagating.preventDefault();
};
export const stopPropagation = () => () => {
  const event = KeyEvent ?? Propagating;
  if (event) event.$stopped = true;
};
// A development page may hold a fact over the app's own (LLP 1122 T7, theme2's F2 ruling: a copy the design tool pins):
// the app's choice kept as asked, with how to apply it, and applied only when the fact is not held.
const asked = (fact, value, apply, deferred = false) => { const x = globalThis.exact ?? (deferred ? (globalThis.exact = {}) : null); if (!x) return apply(); (x.asked ??= {})[fact] = { value, apply }; if (!x.held?.[fact] && !deferred) apply(); else x.onAsked?.(); };
// Bare focus is the window's; under the agent it is journaled (LLP 1133.005 D10).
export const focus = (say, agentMode) => id => id == null
  ? (agentMode() ? say("focus window") : window.focus()) : liveById(id)?.focus();
export const blur = () => id => {
  const a = document.activeElement;
  if (a && a !== document.body && (id == null || a.id === id)) a.blur();
};
// The scheme the app chose is kept beside persisted state under "#scheme", as runner/src/runner/held.rs keeps it
// ("system" forgets it), and asked again as this command is installed, before the plan's first render.
const keptScheme = () => Kept("state") + "#scheme";
const scheme = s => () => { document.documentElement.style.colorScheme = s === "system" ? "" : s; };
export const setScheme = say => {
  if (!globalThis.__exactRender) try {
    const s = JSON.parse(localStorage.getItem(keptScheme()));
    if (s === "light" || s === "dark") asked("scheme", s, scheme(s), !!DEVELOPMENT_STORE && self !== parent && /^exact-copy-\d{1,2}$/.test(self.name) && !document.getElementById("exact-dev-pin"));
  } catch {}
  return s => {
    if (!globalThis.__exactRender) try {
      if (s === "light" || s === "dark") localStorage.setItem(keptScheme(), JSON.stringify(s));
      else localStorage.removeItem(keptScheme());
    } catch (e) { say(`store #scheme failed: ${e}`); }
    asked("scheme", s, scheme(s));
  };
};
// Never the person's clipboard under the agent (LLP 1133.006 §7).
export const copyText = (_say, agentMode) => t => {
  if (agentMode()) (clock.clipboard ??= {}).item = { "text/plain": t }; else navigator.clipboard?.writeText(t);
};
// LLP 1077 D14: vibration where the browser has it.
export const haptic = () => k => navigator.vibrate?.(k === "selection" ? 5 : 12);
// The field's own method, by its id (x2apps codeedit #2); commands run
// once the commit's tree is in place, so a value set beside it is there.
export const removeAllRanges = () => () => getSelection()?.removeAllRanges();
export const setSelectionRange = say => (...args) => setFieldSelection(liveById(args[0]), args, say);
export const setRangeText = say => (...args) => setFieldRangeText(liveById(args[0]), args, say);
export const cancelReorder = () => id => globalThis.exact?.cancelReorder?.(id);
// Focus, then the field's whole text selected, as `select()` does
// (navigation.js `runFocusCommands`).
export const selectText = say => id => {
  const e = liveById(id);
  if (typeof e?.select !== "function") return say(`selectText "${id}" refused: ${e ? "not a text editor" : "no live node with that id"}`);
  e.focus();
  if (document.activeElement === e) e.select();
};
// An absolute http, https, mailto or tel URL in a new browsing context,
// the one scheme allowlist every navigation takes (navigation.js `navigableURL`),
// or one of a scheme app.json's `open_url.schemes` declares (`exact.openSchemes`),
// which the browser asks about before it hands it to the app registered for it.
// `change` at `id` hears `opened` for a URL handed to the browser, which says
// nothing of what then opened it (a `noopener` open returns null), and
// `failed` for one refused here; after the commands, as an event.
export const openURL = (say, agentMode) => (u, id) => {
  let to = null, declared = false;
  try { const url = new URL(u); declared = declaredScheme(url.protocol); if (declared || ["http:", "https:", "mailto:", "tel:"].includes(url.protocol)) to = url.href; } catch {}
  if (to && declared && agentMode()) say(`openURL ${to.slice(0, to.indexOf(":"))}: ${to.slice(0, 200)} (journaled, not opened under the agent)`);
  else if (to) open(to, "_blank", "noopener,noreferrer");
  // The app's own settings page (LLP 1133.002 D11): a browser has none to open.
  else if (u === "app-settings:") say("openURL refused: app-settings: has no page on the web; a site's permissions are the browser's own (its address bar, its settings)");
  else say(`openURL refused: ${JSON.stringify(String(u).slice(0, 80))} is not an absolute http, https, mailto or tel URL, nor of a scheme app.json's open_url.schemes declares`);
  if (id != null) queueMicrotask(() => liveById(String(id))?.dispatchEvent(new CustomEvent("change", { detail: to ? "opened" : "failed" })));
};
// The inverse of `message=`: text into the named surface, every post in
// order, held until the GPU glue loads and drains `pendingPosts`
// (gpu-glue.js); 64 a surface, as every host bounds them.
export const postMessage = say => (text, name, kind) => {
  const live = [...document.querySelectorAll('#exact-root *')].filter(e => !e.closest('[data-exiting]'));
  if (kind === "iframe") { const captured = live.filter(e => e.localName === 'iframe' && e.id === name).map(e => [e, e.exactFrameChannel]); import('./frame-messages.js').then(m => m.postFrameMessage(text, name, null, say, captured)); return; }
  if (live.some(e => e.localName === 'iframe' && e.id === name)) return say(`postMessage: canvas name collides with an iframe id "${name}"`);
  const x = globalThis.exact ??= {}, at = x.now?.() ?? performance.now();
  if (x.gpu) x.gpu.post(name, text, at);
  else if ((x.pendingPosts ??= []).filter(p => p.name === name).length >= 64) say(`postMessage: dropped: 64 posts already wait for surface "${name}"`);
  else x.pendingPosts.push({ name, text, at, generation: 0 });
};
// `:root { font-size }` (LLP 1069.000 D3): the stylesheet's `rem` rows
// follow it; checked here, where no runner checked it first.
// (A size the app may not set is no choice: the last one stands, and the call refuses as ever.)
export const setRootFontSize = say => v => v === "medium" || (typeof v === "number" && Math.fround(v) > 0 && Number.isFinite(Math.fround(v))) || (typeof v === "string" && /^[+-]?(\d+|\d*\.\d+)(e[+-]?\d+)?%$/i.test(v) && ((h) => Math.fround(parseFloat(v) / 100 * h) > 0 && Number.isFinite(Math.fround(parseFloat(v) / 100 * h)))(parseFloat(getComputedStyle(document.documentElement).fontSize) || 16)) ? asked('rootFontSize', v, () => appRootFontSize(v, say)) : appRootFontSize(v, say);
// The document's own full screen ends, as `document.exitFullscreen()`; a page cannot fade its window (LLP 1069.010).
export const exitFullscreen = say => () => { if (document.fullscreenElement) document.exitFullscreen().catch(e => say(`exitFullscreen: refused: ${e.name}`)); };
export const panel = say => () => say("panel unsupported: route panels are available on macOS only");
export const setBounds = say => () => say("setBounds refused: a browser page cannot set its outer frame atomically");
export const setAspectRatio = say => () => say("setAspectRatio refused: a browser page cannot constrain its window's aspect ratio");
export const setWindowAlwaysOnTop = say => () => say("setWindowAlwaysOnTop refused: a browser page cannot set its window level");
export const setWindowIgnoreMouseEvents = say => () => say("setWindowIgnoreMouseEvents refused: a browser page cannot pass mouse events through its window");
export const setWindowOpacity = say => () => say("setWindowOpacity refused: a page cannot change its window's opacity");
// A page cannot put its window away and keep running (LLP 1133.025 D4).
export const hideWindow = say => () => say("hideWindow refused: a page cannot hide its window");
// The commit that names it already ran its tree update inside the browser's view transition (rt.js, shared.js).
export const startViewTransition = () => () => {};
// A page cannot set the browser's zoom, which is the person's (LLP 1021 §5.3.1).
export const setPageZoom = say => () => say("setPageZoom refused: the browser's zoom is the person's; a page cannot set it");
// A recording of frames is the Apple hosts'; the browser's performance panel records the page.
export const startTrace_EXPERIMENTAL = say => () => say("startTrace_EXPERIMENTAL refused: the web host does not record traces; use the browser's performance panel");
export const stopTrace_EXPERIMENTAL = say => () => say("stopTrace_EXPERIMENTAL refused: the web host does not record traces; use the browser's performance panel");
// The window's app modes (LLP 1121 D5, D7): modes.js's, which an app with
// axes loads; any other refuses it, as the runner does.
export const setMode = say => (...args) => globalThis.exact?.modes ? globalThis.exact.modes.command(args) : say(`setMode(${args.map(x => JSON.stringify(x)).join(", ")}) refused: an axis of the app's design and one of its values`);
// The page's own reload: every web page has one, where a native host
// needs its dev menu's. `reload(id, ignoreCache)` is a frame's.
export const reload = say => (...args) => args.length && args[0] != null ? frame(say, "reload", args) : location.reload();
// A frame's navigation commands (LLP 1020.002 D3): frame-navigation.js's,
// the subset a page has over its frame.
export const navigate = say => (...args) => frame(say, "navigate", args);
export const traverseHistory = say => (...args) => frame(say, "traverseHistory", args);
export const stopLoading = say => (...args) => frame(say, "stopLoading", args);
// A picture of a frame's guest (LLP 1020.002 D7): refused on the web, answered on its `capture=`.
export const captureScreenshot = say => (...args) => frame(say, "captureScreenshot", args);
// A frame's inspector (LLP 1020.002 D11): a page has no API that opens the browser's developer tools.
export const openDevTools = say => id => say(`openDevTools("${id}") refused: a page cannot open the browser's developer tools; open them from the browser's menu`);
export const relaunch = () => () => location.reload();
// `window.close()` (studio diary R17): a browser closes only a window a
// script opened, and says so in its console otherwise.
// `close(id)` is a dialog's (LLP 1101.001 P5): HTMLDialogElement's.
export const close = say => id => id == null ? window.close() : modalCommand(liveById(id), id, "close", say);
// `showModal(id)` from an action (LLP 1115 D6): the dialog's own, or an
// alertdialog popover's showPopover, as the native hosts present either.
export const showModal = say => id => modalCommand(liveById(id), id, "showModal", say);
export const showPopover = say => (id, source) => popoverCommand("showPopover", id, source, say);
export const hidePopover = say => id => popoverCommand("hidePopover", id, null, say);
export const togglePopover = say => (id, source) => popoverCommand("togglePopover", id, source, say);
// A JS build links no update store (facts.js `exactDelivery`): the page
// loaded the newest root, and nothing is ever staged.
export const deliveryCheck = say => () => say("delivery: no update store on the web; the page loaded the newest root");
export const deliveryActivate = say => () => say("delivery: nothing is staged");
// A media element's, by HTML's method names (podcast F8, F18): media.js
// queues them for the web host's media glue (media-glue.js `run`).
export const fastSeek = say => (id, seconds) => media(say, id, "fastSeek", seconds);
export const load = say => id => media(say, id, "load");
// An image's pictures in the browser's floating window (picture-in-picture.js, LLP 1042 §9).
export const requestPictureInPicture = say => (id, presentation, title, width, height) => { const e = liveById(String(id ?? "")); import("./picture-in-picture.js").then(m => m.float(e, id, say, { presentation, title, width, height })); };
export const exitPictureInPicture = say => () => import("./picture-in-picture.js").then(m => m.exit(say));

function frame(say, name, args) {
  import("./frame-navigation.js").then(m => m.command(name, args, say));
}

function media(say, id, name, seconds) {
  const e = liveById(id);
  if (!e?.$media) return say(`${name} "${id}" refused: ${e ? "not a video or audio" : "no live node with that id"}`);
  e.$media.command(name, seconds);
}
