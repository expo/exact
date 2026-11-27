// @ref LLP 1123 D13, D17 — how far each box's reach takes it past its border box on the web, measured in the transaction
// from layout: on each edge where the box's border edge meets the unsafe area's inner side, within D9's tolerance, or
// lies inside the area, to the area's outer side (the viewport's edge for the band, a reserving ancestor's padding edge
// for what it reserves clear of the band), no further than the padding edge of the furthest in of its ancestors that
// stop reach on that edge (one that clips on the edge's axis, has a border there, or is drawn through a transform, a
// filter, a clip path or a mask). A box's own scrollport clip does not stop it. The band is the reader's (`observe.js`):
// the viewport's unsafe area, transparent bars and the boxes around it that reserve. Which boxes reach is the caller's:
// a scroller and a box with a plain background colour under `auto`, the edges a box names otherwise.

import { chain, documentBand, lines, over, styles, unscrolled } from "./observe.js";

/** Each of `boxes` (elements) and how far it reaches past its border box on each edge (top right bottom left), against
 * the viewport's unsafe area `band` (the browser's `env()` unless given). A box out of the document, or with no box,
 * reaches nothing. */
export function reach(boxes, band = documentBand()) {
  const box = styles(), held = over(boxes, box), seen = new Map(), out = new Map();
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const tolerance = 0.5 / (globalThis.devicePixelRatio || 1);
  // A box as laid out with every scroll offset at zero: its scrolled containing blocks' offsets come off, found through its
  // containing blocks only where an ancestor is scrolled, as a box that scrolls nothing has offsets of 0; whether one is,
  // once an ancestor, since boxes share them
  const scrolled = new Map();
  const under = (a) => {
    if (!a) return false;
    let s = scrolled.get(a);
    if (s === undefined) scrolled.set(a, (s = !!(a.scrollTop || a.scrollLeft) || under(a.parentElement)));
    return s;
  };
  const laid = (el) => {
    const b = el.getBoundingClientRect();
    const [dx, dy] = under(el.parentElement) ? unscrolled(chain(el, box).filter((c) => c === document.scrollingElement || box(c).scrolls)) : [0, 0];
    return { top: b.top + dy, right: b.right + dx, bottom: b.bottom + dy, left: b.left + dx };
  };
  const pads = new Map();
  const padding = (a) => {
    let pad = pads.get(a);
    if (!pad) {
      const p = laid(a), s = box(a).borders;
      pads.set(a, (pad = [p.top + s[0], p.right - s[1], p.bottom - s[2], p.left + s[3]]));
    }
    return pad;
  };
  for (const el of boxes) {
    if (!el.isConnected || !el.getClientRects().length) {
      out.set(el, [0, 0, 0, 0]);
      continue;
    }
    const r = laid(el), steps = held.get(el);
    const area = steps ? lines(steps, band, width, height, box, seen) : { band, caps: band, outer: [0, width, height, 0] };
    const inner = [area.band[0], width - area.band[1], height - area.band[2], area.band[3]];
    // Whether the border edge meets the area's inner side or lies inside it, where the area has depth on that edge
    const touches = [r.top <= inner[0] + tolerance, r.right >= inner[1] - tolerance, r.bottom >= inner[2] - tolerance, r.left <= inner[3] + tolerance]
      .map((t, e) => t && area.caps[e] > 0);
    const reached = [r.top - area.outer[0], area.outer[1] - r.right, area.outer[2] - r.bottom, r.left - area.outer[3]]
      .map((d, e) => touches[e] ? Math.max(0, d) : 0);
    // The furthest in of the stopping ancestors' padding edges
    if (reached.some((d) => d > 0)) {
      for (let a = el.parentElement; a && a !== document.documentElement; a = a.parentElement) {
        const stops = box(a).stops;
        if (!stops.some(Boolean)) continue;
        const pad = padding(a);
        const room = [r.top - pad[0], pad[1] - r.right, pad[2] - r.bottom, r.left - pad[3]];
        stops.forEach((stop, e) => { if (stop) reached[e] = Math.min(reached[e], Math.max(0, room[e])); });
      }
    }
    out.set(el, reached);
  }
  return out;
}
