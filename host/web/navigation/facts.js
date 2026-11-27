// @ref LLP 1123 D6 — what the web answers a geometry fact from, on both targets (web-js `facts.js`, the wasm target's
// `geometry-glue.js`): an element's border box as the browser draws it, transforms and scroll offsets above it counted.
/** `el`'s drawn border box, its size and its frame in `space`: the document's (`"owner"`, outside every owner, which
 * every box is until the web's owners exist), the viewport's (`"window"`) or the border box of `node` (`"node"`), the
 * frame `null` there where `node` has no box or it or an ancestor of it is transformed, since a bounding rectangle
 * does not recover its own coordinates; `null` where `el` is gone or has no box. */
export function drawnBox(el, space, node) {
  if (!el?.getClientRects().length) return null;
  const r = el.getBoundingClientRect();
  const origin = space === "owner" ? [-(globalThis.scrollX ?? 0), -(globalThis.scrollY ?? 0)] : space === "window" ? [0, 0]
    : node?.getClientRects().length && !transformed(node) ? [node.getBoundingClientRect().left, node.getBoundingClientRect().top] : null;
  return { size: [r.width, r.height], frame: origin && [r.left - origin[0], r.top - origin[1], r.width, r.height] };
}

/** Whether `el` or an ancestor of it is transformed, which a bounding rectangle cannot undo. */
function transformed(el) {
  for (let e = el; e?.nodeType === 1; e = e.parentElement) {
    const s = getComputedStyle(e);
    if ((s.transform && s.transform !== "none") || (s.scale && s.scale !== "none") || (s.rotate && s.rotate !== "none") || (s.translate && s.translate !== "none")) return true;
  }
  return false;
}
