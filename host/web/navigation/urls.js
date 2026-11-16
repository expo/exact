// Use the browser URL parser to normalize schemes, including hidden whitespace, for every navigable URL.
// openURL also permits declared external schemes, excluding schemes that execute or read page content.
export const declaredScheme = (protocol) => !/^(javascript|vbscript|data|file|filesystem|blob|about|view-source|jar|intent|content):$/.test(protocol) && (globalThis.exact?.openSchemes ?? []).includes(protocol.slice(0, -1));
export function navigableURL(href, base = document.baseURI) {
  try {
    const url = new URL(href, base);
    return [
      "http:",
      "https:",
      "mailto:",
      "tel:"
    ].includes(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}
export const navigates = (el, name) => name === "href" || name === "src" && el.localName === "iframe";
// A present sandbox without `allow-same-origin` gives an opaque origin; tokens use ASCII whitespace and ignore case.
export const opaqueSandbox = (sandbox) => sandbox != null && !sandbox.toLowerCase().split(/[\t\n\f\r ]+/).includes("allow-same-origin");
export function dataURL(src, base = document.baseURI) {
  try {
    return src != null && new URL(src, base).protocol === "data:";
  } catch {
    return false;
  }
}
/** Whether an iframe in `sandbox` shows `src`: a `navigableURL`, or a `data:`
* document in an opaque sandbox, whose script cannot reach this page (LLP 1020 §10). */
export const frameURL = (src, sandbox, base = document.baseURI) => navigableURL(src, base) !== null || opaqueSandbox(sandbox) && dataURL(src, base);
export const pageGivenFiles = (src, base = document.baseURI) => {
  try {
    const url = new URL(src, base);
    return url.origin === new URL(base).origin && /[?&][^&=]*=app(:|%3a)/i.test(url.search);
  } catch {
    return false;
  }
};
// a frame source that may give its page app files (LLP 1020.001 D7), read as the URL parser reads it: a page of the app's own origin whose query has a value beginning `app:`; picker-glue.js decides which pages are given which files
/** A refused URL is never written: a link loses its `href`, an iframe shows about:blank. */
export function refuseURL(el, name, value) {
  console.warn(`exact: refused ${name} ${JSON.stringify(String(value).slice(0, 80))}: only http, https, mailto and tel navigate${name === "src" ? ", and data: in a sandbox without allow-same-origin" : ""}`);
  if (name === "src") {
    el.setAttribute("src", "about:blank");
  } else {
    el.removeAttribute(name);
  }
}
// An iframe guest's origin as authored when its `src` or `sandbox` was committed (an opaque
// sandbox posts as "null"), which a `message` from it must match. The authored document owns
// the channel: a later load revokes it until the app commits a new source.
const iframeOrigins = new WeakMap();
export function commitGuestOrigin(el) {
  const src = el.getAttribute("src"), sandbox = el.getAttribute("sandbox"), previous = iframeOrigins.get(el);
  if (previous?.src === src && previous?.sandbox === sandbox) {
    return;
  }
  if (previous) {
    el.removeEventListener("load", previous.loaded);
  }
  const opaque = opaqueSandbox(sandbox);
  let origin = null;
  if (!opaque) {
    try {
      origin = !src || src === "about:blank" ? location.origin : new URL(src, document.baseURI).origin;
    } catch {}
    if (origin === "null") {
      origin = null;
    }
  }
  const state = {
    src,
    sandbox,
    origin,
    opaque,
    committed: false,
    revoked: false
  };
  state.loaded = () => {
    if (iframeOrigins.get(el) !== state) {
      return;
    }
    if (state.committed) {
      state.revoked = true;
    }
    state.committed = true;
  };
  iframeOrigins.set(el, state);
  el.exactFrameChannel = state;
  el.addEventListener("load", state.loaded);
}
export function guestMessageAuthorized(el, eventOrigin) {
  const state = iframeOrigins.get(el);
  return Boolean(state && !state.revoked && (state.opaque ? eventOrigin === "null" : eventOrigin === state.origin));
}
