// @ref LLP 1123 D17, "The web: observers" — what a page's `ResizeObserver` and `IntersectionObserver` deliver after a
// commit carries no Exact generation, so the host keeps the geometry it sampled for each observed target in commit g's
// settled layout and suppresses a delivery only when that sample accounts for it: a resize entry whose box sizes, or an
// intersection entry whose target box, are the sample's within half a device pixel. Any other delivery carries a change
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
  const r = entry.boundingClientRect, s = sample.rect;
  return near(r.x, s.x) && near(r.y, s.y) && near(r.width, s.width) && near(r.height, s.height);
}

/** `el`'s box sizes as a `ResizeObserver` reports them, inline then block, untransformed, and its box as an
 * `IntersectionObserver` reports it. */
function measure(el) {
  const s = getComputedStyle(el), px = (v) => parseFloat(v) || 0;
  const vertical = !s.writingMode.startsWith("horizontal");
  const pad = [px(s.paddingLeft) + px(s.paddingRight), px(s.paddingTop) + px(s.paddingBottom)];
  const edge = [px(s.borderLeftWidth) + px(s.borderRightWidth), px(s.borderTopWidth) + px(s.borderBottomWidth)];
  // The used width and height, which `getComputedStyle` resolves for a box that has one
  const used = [px(s.width), px(s.height)];
  const content = s.boxSizing === "border-box" ? used.map((v, i) => v - pad[i] - edge[i]) : used;
  const border = content.map((v, i) => v + pad[i] + edge[i]);
  const order = (pair) => (vertical ? [pair[1], pair[0]] : pair);
  return { border: order(border), content: order(content), rect: el.getBoundingClientRect() };
}
