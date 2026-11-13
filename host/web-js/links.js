// A link's `rel` and `target` as its bound `href`, `rel` or `target` change (element.rs `link_rel`, `leaves_app`;
// LLP 1141.002 D1), which rt.js reaches through `useLinks`. The compiler installs it only for a plan with such a
// binding: a static link's the template wrote whole (element.rs `props_of`), and nothing here would change them
// (LLP 1047.002 D1).
const relTokens = v => (v ?? "").split(/[ \t\n\f\r]+/).filter(Boolean);

/** A link made or adopted from its template: its literal `rel` and `target` are the author's; `external` stays as the
 * page was rendered until a bound `href` says again. */
function made(e, attrs, rendered) {
  e.$rel = relTokens(attrs.rel).filter(t => !/^(external|noopener)$/i.test(t)).join(" "); e.$target = "target" in attrs && !relTokens(attrs.rel).includes("external");
  e.$external ??= relTokens(rendered ?? attrs.rel).includes("external"); linkRel(e);
}

/** A bound `rel`, `target` or `href`: a link to an absolute URL leaves the app in a new browsing context unless its
 * `target` is authored (element.rs `leaves_app`, `props_of`; chat F11). */
function prop(e, name, v) {
  if (name === "rel") e.$rel = v; else if (name === "target") { e.$target = v != null; e.$external = false; }
  else if (!e.$target) { const out = v != null && /^\s*(https?:)?\/\//i.test(v); if (out) e.setAttribute("target", "_blank"); else e.removeAttribute("target"); e.$external = out; }
  linkRel(e);
}

/** A link's `rel` (element.rs `link_rel`): the author's tokens, then those its target needs that they lack, compared
 * ASCII case-insensitively: `noopener` for a new browsing context, and `external` before it for the one an absolute
 * URL opens by default. The author's are what `rel` last wrote, and the template's (`made`) before that. */
function linkRel(e) {
  const own = relTokens(e.$rel), need = e.getAttribute("target") !== "_blank" ? [] : e.$external ? ["external", "noopener"] : ["noopener"];
  const all = own.concat(need.filter(t => !own.some(o => o.toLowerCase() === t))).join(" ");
  if (all) { if (e.getAttribute("rel") !== all) e.setAttribute("rel", all); } else e.removeAttribute("rel");
}

export const linkUse = () => ({ made, prop });
