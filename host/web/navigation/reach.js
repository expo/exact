// @ref LLP 1123 D13, D14, D17 — how far each box's reach takes it past its border box on the web, how far its colour
// extends and how deep the unsafe area occupies a scroller, measured in the transaction from layout, as laid out with
// every scroll offset at zero and every sticky box where layout puts it. The area is the one a box there observes, as
// `observe.js` `area` gives it after the kernel's: the viewport's unsafe area, transparent bars and reservations, or none
// inside a scroller, which takes it. A box touches an edge where its border edge meets the area's inner side within D9's
// tolerance or lies inside it, and reaches there to the area's outer side, no further than the padding edge of the
// furthest in of its ancestors that stop reach on that edge (one that clips on the edge's axis, has a border there, or
// is drawn through a transform, a filter, a clip path or a mask). A box's own scrollport clip does not stop it. Which
// boxes reach, and on which edges, is the caller's: a scroller and a box with a plain background colour under `auto`, the
// edges a box names otherwise.

import { own, register } from "./inputs.js";
import { area, atRest, depth, documentBand, edgesOf, framing, laid, over, scrolling, shaping, styles } from "./observe.js";
import { keyboardInset, keyboardPolicy } from "./environment.js";

const EDGES = ["top", "right", "bottom", "left"], ALL = [true, true, true, true], NONE = [0, 0, 0, 0];

/** Each of `boxes` (elements) and how far it reaches past its border box on each edge (top right bottom left), against
 * the viewport's unsafe area `band` (the browser's `env()` unless given), on the edges `reaches(box)` allows (every edge
 * unless given). A box out of the document, or with no box, reaches nothing. */
export function reach(boxes, band = documentBand(), { reaches } = {}) {
  return new Map([...measure(boxes, band, styles(), { reaches })].map(([el, m]) => [el, m.reach]));
}

/** Each of `boxes` with, on each edge, its reach (`reach`), its laid-out box (`rect`), the area it lies in (`sides`) and
 * whether a scroller above it takes the keyboard (`taken`, under `policy`). Reads only. */
function measure(boxes, band, box, { reaches = () => ALL, policy } = {}) {
  const held = over(boxes, box, policy), out = new Map();
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const tolerance = 0.5 / (globalThis.devicePixelRatio || 1);
  return atRest([...boxes, ...shaping(held)], box, () => {
    // A box is unscrolled through its containing blocks only where an ancestor is scrolled, as one that scrolls nothing
    // has offsets of 0; whether one is, once an ancestor, since boxes share them
    const scrolled = new Map(), seen = new Map(), pads = new Map(), framed = framing();
    const under = (a) => {
      if (!a) return false;
      let s = scrolled.get(a);
      if (s === undefined) scrolled.set(a, (s = !!(a.scrollTop || a.scrollLeft) || under(a.parentElement)));
      return s;
    };
    const at = (el) => {
      if (!seen.has(el)) seen.set(el, el.isConnected && el.getClientRects().length ? laid(el, under(el.parentElement) ? scrolling(el, box) : [], box) : null);
      return seen.get(el);
    };
    const padding = (a) => {
      let pad = pads.get(a);
      if (!pad) {
        const p = at(a), s = box(a).borders;
        pads.set(a, (pad = [p.top + s[0], p.right - s[1], p.bottom - s[2], p.left + s[3]]));
      }
      return pad;
    };
    for (const el of boxes) {
      const r = at(el);
      if (!r) {
        out.set(el, { reach: NONE, rect: null, sides: null });
        continue;
      }
      const sides = area(held.get(el), band, width, height, at, framed, tolerance), may = reaches(el);
      const reached = edgesOf(r, width, height).map((d, e) => may[e] && d <= sides[e][1] + tolerance && sides[e][1] > sides[e][0] ? Math.max(0, d - sides[e][0]) : 0);
      // The furthest in of the stopping ancestors' padding edges, up to the top layer, whose box is the last that stops it;
      // a box that generates none (`display: contents`) has no edge to stop at
      const room = [Infinity, Infinity, Infinity, Infinity];
      if (reached.some((d) => d > 0)) {
        for (let a = box(el).top ? null : el.parentElement; a && a !== document.documentElement; a = box(a).top ? null : a.parentElement) {
          const stops = box(a).display === "contents" ? NONE : box(a).stops;
          if (!stops.some(Boolean)) continue;
          const pad = padding(a), left = [r.top - pad[0], pad[1] - r.right, pad[2] - r.bottom, r.left - pad[3]];
          stops.forEach((stop, e) => { if (stop) room[e] = Math.min(room[e], Math.max(0, left[e])); });
        }
      }
      out.set(el, { reach: reached.map((d, e) => Math.min(d, room[e])), rect: r, sides, taken: !!held.get(el)?.keys });
    }
    return out;
  });
}

/** The registered lengths the executor writes each reaching box's reach in, edge by edge, which the emitter's surface
 * reads: the box's paint and touches carried past its border box (D13), its layout and observations left alone. */
export const REACH = EDGES.map((e) => `--exact-reach-${e}`);

/** Write how far each of `boxes` reaches on each edge (`reach`) on the box itself (`REACH`), every box read before any is
 * written. */
export function reached(boxes, band = documentBand(), { reaches } = {}) {
  register(REACH);
  for (const [el, d] of reach(boxes, band, { reaches })) if (el.isConnected) d.forEach((x, e) => own(el, REACH[e], `${x}px`));
}

/** The registered properties the emitter writes a box's extension bounds in (`-exact-safe-area-extend-bounds`), on each
 * edge it extends on (`-exact-safe-area-extend`): `auto` or a length; `none`, unsaid, on an edge it does not. */
export const BOUNDS = EDGES.map((e) => `--exact-safe-area-extend-bounds-${e}`);
/** The registered lengths the executor writes how far each box's colour extends past its border box in, edge by edge,
 * which the emitter's strip reads. */
export const EXTENT = EDGES.map((e) => `--exact-extend-${e}`);

/** How far each of `boxes` extends its colour past its border box (D13, "The extension's bounds"), written on the box
 * (`EXTENT`): on an edge with an `auto` bound, the distance its reach would have there, whatever its own reach; with a
 * length, that length, as the kernel takes it, which a clipping ancestor cuts as the browser paints it; else 0. No strip
 * shows, so every edge is 0, where the box clips its overflow on either axis, where it or an ancestor below the top layer
 * is drawn through a filter, a clip path or a mask, and while a transition or animation of its background colour runs,
 * which one colour cannot follow (D13, "Without an extension view"). Every box is read before any is written. */
export function extension(boxes, band = documentBand()) {
  register(BOUNDS, "none | auto | <length>", "none");
  register(EXTENT);
  const box = styles(), live = boxes.filter((el) => el.isConnected), through = new Map();
  // Whether a box or an ancestor is drawn through a filter, a clip path or a mask, whose picture the strip cannot mirror;
  // each ancestor asked once
  const drawn = (a) => {
    if (!a || a === document.documentElement) return false;
    let d = through.get(a);
    if (d === undefined) through.set(a, (d = box(a).drawn || (!box(a).top && drawn(a.parentElement))));
    return d;
  };
  const said = new Map(live.map((el) => {
    const s = getComputedStyle(el), bounds = BOUNDS.map((name) => s.getPropertyValue(name).trim());
    const clips = s.overflowX !== "visible" || s.overflowY !== "visible";
    const playing = el.getAnimations().some((a) => a.playState === "running"
      && (a.transitionProperty === "background-color" || (a.effect?.getKeyframes?.() ?? []).some((k) => "backgroundColor" in k)));
    return [el, { bounds, shows: !clips && !playing && !drawn(el) }];
  }));
  const measured = measure(live, band, box);
  const writes = live.map((el) => {
    const { bounds, shows } = said.get(el), { reach: d } = measured.get(el);
    return [el, bounds.map((bound, e) => !shows || !bound || bound === "none" ? 0 : bound === "auto" ? d[e] : Math.max(0, parseFloat(bound) || 0))];
  });
  for (const [el, extents] of writes) extents.forEach((x, e) => own(el, EXTENT[e], `${x}px`));
}

/** The registered lengths the executor writes each scroller's occupied depth in, edge by edge (D14), from which the
 * emitter composes its content inset. */
export const OCCUPIED = EDGES.map((e) => `--exact-scroll-occupied-${e}`);

/** Write on each of `scrollers` how deep the unsafe area occupies it on each edge (D14's occupied terms): how far the
 * area a box there observes reaches past the scroller's edge as its reach carries it (`reaches`, as `reach` takes it),
 * at most that area's depth; and on the bottom edge of a vertical scroller that no other vertical scroller holds, under
 * `resizes-visual` (`policy`, D7), how far the docked keyboard reaches past that edge (`keyboard`, its depth up from the
 * viewport's bottom), at most its depth, where that is deeper, as the kernel's `taken`. The web has no platform
 * adjustment, so the whole occupied depth is the scroller's (D14's P is 0). Every scroller is read before any is
 * written. */
export function insets(scrollers, band = documentBand(), keyboard = keyboardInset(), { reaches, policy = keyboardPolicy() } = {}) {
  register(OCCUPIED);
  const width = document.documentElement.clientWidth, height = document.documentElement.clientHeight;
  const writes = [], box = styles();
  for (const [el, { reach: d, rect, sides, taken }] of measure(scrollers, band, box, { reaches, policy })) {
    if (!el.isConnected) continue;
    if (!rect) {
      writes.push([el, NONE]);
      continue;
    }
    const edges = edgesOf(rect, width, height), occupied = edges.map((x, e) => depth(sides[e], x - d[e]));
    // Only a vertical scroller takes the keyboard, and none under another vertical one, which takes it already (D7)
    if (policy === "resizes-visual" && box(el).scrollsY && !taken) occupied[2] = Math.max(occupied[2], depth([0, keyboard], edges[2] - d[2]));
    writes.push([el, occupied]);
  }
  for (const [el, occupied] of writes) occupied.forEach((x, e) => own(el, OCCUPIED[e], `${x}px`));
}
