// @ref LLP 1123 D17, "The web: observers" — what a page's `ResizeObserver` and `IntersectionObserver` deliver after a
// commit carries no Exact generation, so the host keeps the geometry it sampled for each observed target in commit g's
// settled layout and suppresses a delivery only when that sample accounts for it: a resize entry whose box sizes, or an
// intersection entry whose target box and intersection with the viewport, are the sample's within half a device pixel. Any other delivery carries a change
// from outside the commit (an author's style, a font, a scroll), which schedules commit g+1. A target with no sample is
// always outside. Activation (stage 5) samples after each commit's rounds and routes the host's own observers' entries
// through it; until then only its tests do.

/** A new account of commit samples: `sample` records the observed `targets` in the layout as it now stands, for
 * `generation`; `outside` returns the entries of one delivery that no sample accounts for, and the generation the
 * accounted ones came from. */
export function account() {
  const samples = new WeakMap();
  return {
    sample(targets, generation) {
      for (const el of targets) samples.set(el, { generation, ...measure(el) });
    },
    outside(entries) {
      const tolerance = 0.5 / (globalThis.devicePixelRatio || 1);
      return entries.filter((entry) => !accounts(samples.get(entry.target), entry, tolerance));
    },
    generation(target) {
      return samples.get(target)?.generation;
    },
  };
}

/** Whether `sample` accounts for `entry`, each length within `tolerance`. */
function accounts(sample, entry, tolerance) {
  if (!sample) return false;
  const near = (a, b) => Math.abs(a - b) <= tolerance;
  if ("borderBoxSize" in entry) {
    const [border] = entry.borderBoxSize, [content] = entry.contentBoxSize;
    return !!border && !!content && near(border.inlineSize, sample.border[0]) && near(border.blockSize, sample.border[1])
      && near(content.inlineSize, sample.content[0]) && near(content.blockSize, sample.content[1]);
  }
  // An intersection the sample can tell only for the viewport as the root with no margin, as the target box clipped by
  // the boxes that clip it; any other observer's delivery is kept as outside, so no change of its root or clip is lost
  const r = entry.boundingClientRect, s = sample.rect, root = entry.rootBounds, v = sample.visible;
  const box = near(r.x, s.x) && near(r.y, s.y) && near(r.width, s.width) && near(r.height, s.height);
  if (!box || !root || !near(root.x, 0) || !near(root.y, 0) || !near(root.width, sample.viewport[0]) || !near(root.height, sample.viewport[1])) return false;
  const i = entry.intersectionRect;
  return v ? near(i.x, v.x) && near(i.y, v.y) && near(i.width, v.width) && near(i.height, v.height) : i.width === 0 && i.height === 0;
}

/** `el`'s box sizes as a `ResizeObserver` reports them, inline then block, untransformed, its content box less the
 * scrollbars' gutters, and its box and what of it shows in the viewport through the boxes that clip it, as an
 * `IntersectionObserver` with the viewport as its root reports them. */
function measure(el) {
  const s = getComputedStyle(el), px = (v) => parseFloat(v) || 0;
  const vertical = !s.writingMode.startsWith("horizontal");
  const pad = [px(s.paddingLeft) + px(s.paddingRight), px(s.paddingTop) + px(s.paddingBottom)];
  const edge = [px(s.borderLeftWidth) + px(s.borderRightWidth), px(s.borderTopWidth) + px(s.borderBottomWidth)];
  // The used width and height, which `getComputedStyle` resolves for a box that has one
  const used = [px(s.width), px(s.height)];
  const border = s.boxSizing === "border-box" ? used : used.map((v, i) => v + pad[i] + edge[i]);
  // A scrollbar takes its gutter from the content box, which the observer reports without it
  const gutter = [Math.max(0, el.offsetWidth - edge[0] - el.clientWidth), Math.max(0, el.offsetHeight - edge[1] - el.clientHeight)];
  const content = border.map((v, i) => v - pad[i] - edge[i] - gutter[i]);
  const order = (pair) => (vertical ? [pair[1], pair[0]] : pair);
  const rect = el.getBoundingClientRect(), viewport = [document.documentElement.clientWidth, document.documentElement.clientHeight];
  return { border: order(border), content: order(content), rect, visible: shown(el, rect, viewport), viewport };
}

/** What of `rect`, `el`'s box, shows in the viewport through its ancestors that clip their overflow, or null. */
function shown(el, rect, [width, height]) {
  let x0 = Math.max(rect.left, 0), y0 = Math.max(rect.top, 0), x1 = Math.min(rect.right, width), y1 = Math.min(rect.bottom, height);
  for (let a = el.parentElement; a && a !== document.documentElement; a = a.parentElement) {
    const s = getComputedStyle(a);
    if (s.overflowX === "visible" && s.overflowY === "visible") continue;
    const b = a.getBoundingClientRect(), left = b.left + a.clientLeft, top = b.top + a.clientTop;
    if (s.overflowX !== "visible") { x0 = Math.max(x0, left); x1 = Math.min(x1, left + a.clientWidth); }
    if (s.overflowY !== "visible") { y0 = Math.max(y0, top); y1 = Math.min(y1, top + a.clientHeight); }
  }
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, width: x1 - x0, height: y1 - y0 } : null;
}
