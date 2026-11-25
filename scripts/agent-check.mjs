// `check` (LLP 1116.003 D8): what is wrong with the presented screen, in text, so a builder need not read a
// screenshot back to find it — text clipped against its box or squeezed to no width, a scroller squeezed to less than
// a row, controls overlapping or covered by a bar, the keyboard or a sheet, a control or text outside the safe area or across
// a fold (check-regions.mjs), targets under the platform's minimum,
// horizontal overflow, text below a contrast floor, and a card whose fill is what lies behind it. The host observes
// (host/web-js/check.js, AgentCheckIOS.swift: its presented boxes, ink, fills, clips and hit tests, and the fill painted
// under each text, LLP 1116.006 D5); where no fill is known, the background a text stands on is read from a capture's
// pixels; this file only judges. `check dark` (or `light`) runs it under that `prefers-color-scheme`, `check both` under each.
import { MalformedOperation } from './agent-keys.mjs';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { encodePng, readSrgbPng, shrink } from './png.mjs';
import { clears, meet, regionFindings, regionHint, regionLine } from './check-regions.mjs';

const SHOWN = 12;
const OFFSCREEN = 'a column taller than the screen runs past it: let it scroll (`overflow-y: auto`), or give it `flex: 1` and `min-height: 0` under a header instead of `height: 100%`';
/** The remedy for a scroller squeezed along its axis, by that axis: keep the text's size, give the scroller the room. */
const SQUEEZED_SCROLL = {
  y: 'let what sits above it scroll with it (inside the scroll, or in the list\'s first section), or give it a `min-height` in rem',
  x: 'let what sits beside it give way (`flex-shrink`, or `flex-wrap="wrap"` on the row), or give it a `min-width` in rem',
};
const COLORS = JSON.parse(readFileSync(fileURLToPath(new URL('../kernel/tables/schema.json', import.meta.url)), 'utf8')).colors ?? [];
/** A platform colour's row name by every name a host may report (kernel/tables/schema.json `colors`, LLP 1115.000 D3.2). */
const ROLES = new Map();
for (const [name, webkit, ios, mac] of COLORS) for (const n of [name, webkit, ios, mac, ios?.replace(/Color$/, '')]) if (n && !ROLES.has(n)) ROLES.set(n, name);
/** Each role's colour by scheme, `[r, g, b, a]`: what the page says it draws (`palette`, the web's check), else Apple's
 *  pair (an Apple host says none). */
const APPLE = Object.fromEntries(['light', 'dark'].map((scheme, i) => [scheme, Object.fromEntries(COLORS.filter((row) => String(row[4 + i]).startsWith('#'))
  .map(([name, , , , ...pair]) => [name, [...parseHex(pair[i]), pair[i].length === 9 ? parseInt(pair[i].slice(7), 16) / 255 : 1]]))]));
const paletteOf = (obs) => Object.fromEntries(['light', 'dark'].map((scheme) => [scheme, { ...APPLE[scheme], ...obs.palette?.[scheme] }]));
const role = (raw) => (raw && ROLES.get(String(raw).replace(/^-exact-/, ''))) ?? null;
const hex = (c) => '#' + c.slice(0, 3).map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
const lum = (c) => { const [r, g, b] = c.slice(0, 3).map((v) => { v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
export const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
const over = (fg, bg) => { const a = fg[3] ?? 1; return [0, 1, 2].map((i) => fg[i] * a + bg[i] * (1 - a)); };
const near = (a, b, d) => a && b && [0, 1, 2].every((i) => Math.abs(a[i] - b[i]) <= d);
const r0 = (n) => Math.round(n);

/** Samples a capture in the observer's coordinates. */
function sampler(png, { x = 0, y = 0, w, h }) {
  const sx = png.width / w, sy = h > 0 ? png.height / h : sx;
  return (px, py) => {
    const col = Math.floor((px + x) * sx), row = Math.floor((py + y) * sy);
    if (col < 0 || row < 0 || col >= png.width || row >= png.height) return null;
    const i = (row * png.width + col) * 4;
    return [png.data[i], png.data[i + 1], png.data[i + 2]];
  };
}

/** What a text stands on: the commonest of its box's four corners (a glyph rarely reaches one), or null where they disagree (an image).
 *  A corner the host says another view is drawn over (`drawnOver`), or that lies under a control in `above`, is not read. */
function backdrop(pixel, it, above = []) {
  const inset = Math.min(1.5, it.w / 4, it.h / 4);
  const corners = [[it.x + inset, it.y + inset], [it.x + it.w - inset, it.y + inset], [it.x + inset, it.y + it.h - inset], [it.x + it.w - inset, it.y + it.h - inset]]
    .map(([x, y], k) => (it.drawnOver?.includes(k) || above.some((c) => x >= c.x && x <= c.x + c.w && y >= c.y && y <= c.y + c.h) ? null : pixel(x, y))).filter(Boolean);
  let best = null, count = 0;
  for (const c of corners) { const n = corners.filter((d) => near(c, d, 12)).length; if (n > count || (n === count && best && ratio(c, it.fg) > ratio(best, it.fg))) { best = c; count = n; } }
  return count >= 2 ? best : null;
}

/** The contrast a text needs (WCAG 1.4.3, and LLP 1115's platform answers): 4.5:1; 3:1 for large text (24 px, or 18.66 px bold), for a
 *  control's label on its own fill (D3's ink rule), for ink the platform chose (a tab bar's item) and for text in a platform colour (the
 *  secondary label, system red: the platform's palette, as a hand-built app draws it) or on a fill in a system hue (the same pair
 *  turned over: iOS's badge, white on its red), and for text on an accent fill (LLP 1116.020 D7: a sent
 *  bubble's white is a filled button's label; a pale accent still fails). The platform's tertiary and quaternary labels and its placeholders are its own and not judged,
 *  nor is a disabled control's label (WCAG's inactive component), nor a tinted button's label in a system hue: UIKit draws the hue on
 *  a pale fill of itself (the Clock's green Start, 1.9:1 in light; `RoundButton`'s, LLP 1116.019 D3), and Increase Contrast darkens it. */
function floor(it, scheme, palette) {
  if (it.inactive) return null;
  const r = role(it.fgRole) ?? drawnRole(it.fg, scheme, palette);
  if (r && /^(tertiary|quaternary)-label$|^placeholder$|^GrayText$/.test(r)) return null;
  if (it.tinted && r && /^system-/.test(r)) return null;
  if (it.size >= 24 || (it.size >= 18.66 && it.weight >= 700) || it.label || it.platformInk || it.onAccentFill || (r && /^secondary-label$|^system-|^link$|^LinkText$|^AccentColor$|^ButtonText$/.test(r))) return 3;
  if (/^system-/.test(drawnRole(it.bg, scheme, palette) ?? '')) return 3;
  return 4.5;
}
/** The platform role an ink is, by its value in this scheme as its host draws it (`paletteOf`): UIKit's own labels carry
 *  no name, nor does an inherited colour (a section footer's secondary label on the web). Never `AccentColorText`, an
 *  ink only on the accent (the host says where, `platformInk`): by value it may be black, and an author's black is not
 *  the platform's. */
function drawnRole(fg, scheme = 'light', palette = APPLE) {
  if (!fg) return null;
  for (const [name, c] of Object.entries(palette[scheme] ?? {})) {
    if (name !== 'AccentColorText' && c && near(fg, c, 2) && Math.abs((fg[3] ?? 1) - (c[3] ?? 1)) < 0.03) return name;
  }
  return null;
}

/** What is presented of an item's box: the host's `vis` (a clipping view, or a bar drawn over it, cuts it), less what the
 * keyboard (its top, `kb`) covers; and whether none is (LLP 1116.007 D3). */
const shown = (it, kb) => { const v = it.vis ? { ...it, ...it.vis } : it; return kb != null && v.y + v.h > kb ? { ...v, h: Math.max(0, kb - v.y) } : v; };
const covers = (it, kb) => { const v = shown(it, kb); return v.w <= 0 || v.h <= 0; };
const holds = (a, b) => a.x <= b.x + 0.5 && a.y <= b.y + 0.5 && a.x + a.w >= b.x + b.w - 0.5 && a.y + a.h >= b.y + b.h - 0.5;

// A front sibling can leave an L or two disjoint strips. Find the rectangle that fits the
// largest square, breaking ties by area. The histogram heights are uninterrupted tap space
// above this edge. Its columns follow the host's edges, independent of the screen's resolution.
function tapTarget(it) {
  const rects = it.reach ?? [[it.x, it.y, it.x + it.w, it.y + it.h]];
  if (!rects.length) return { rects, w: 0, h: 0 };
  const xs = [...new Set(rects.flatMap(r => [r[0], r[2]]))].sort((a, b) => a - b);
  const ys = [...new Set(rects.flatMap(r => [r[1], r[3]]))].sort((a, b) => a - b);
  const cx = (xs[0] + xs.at(-1)) / 2, cy = (ys[0] + ys.at(-1)) / 2;
  if (rects.length === 1) return { rects, cx, cy, w: xs.at(-1) - xs[0], h: ys.at(-1) - ys[0] };
  const heights = new Array(xs.length - 1).fill(0);
  let w = 0, h = 0;
  for (let j = 1; j < ys.length; j++) {
    const row = rects.filter(r => r[1] < ys[j] && ys[j] <= r[3]).sort((a, b) => a[0] - b[0]);
    const stack = [];
    for (let i = 0, run = 0; i < xs.length; i++) {
      while (run < row.length && row[run][2] <= xs[i]) run++;
      const hit = run < row.length && row[run][0] <= xs[i];
      const height = hit ? heights[i] + ys[j] - ys[j - 1] : 0;
      heights[i] = height;
      let left = xs[i];
      while (stack.length && stack.at(-1).height > height) {
        const bar = stack.pop(); left = bar.left;
        const width = xs[i] - left, side = Math.min(width, bar.height), best = Math.min(w, h);
        if (side > best || (side === best && width * bar.height > w * h)) { w = width; h = bar.height; }
      }
      if (height > (stack.at(-1)?.height ?? 0)) stack.push({ height, left });
    }
  }
  return { rects, cx, cy, w, h };
}

// WCAG 2.5.8, https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html#spacing
// The circle is centered on the target's bounding box. It must clear every other target
// and the circle of each other undersized target. Two edges that just touch have clearance.
function spacedTarget(target, targets, min) {
  for (const [item, other] of targets) {
    if (other === target || !other.rects.length) continue;
    const [dx, dy] = item.scrollGain ?? [0, 0];
    if (Math.min(other.w + dx, other.h + dy) < min - 0.5 && Math.hypot(target.cx - other.cx, target.cy - other.cy) < min) return false;
    if (other.rects.some(([l, t, r, b]) => Math.hypot(Math.max(l - target.cx, 0, target.cx - r),
      Math.max(t - target.cy, 0, target.cy - b)) < min / 2)) return false;
  }
  return true;
}

/** The findings for one observation (`obs`, the host's reply) and its capture (`png`, decoded; null skips what needs pixels),
 *  with node types from the tree shown in the same scheme (`nodes`, by id). */
export function judge(obs, png, { host = 'web', nodes = new Map(), capture = obs.capture ?? obs.viewport } = {}) {
  const vp = obs.viewport, items = obs.items ?? [], found = [], palette = paletteOf(obs);
  const pixel = png && capture ? sampler(png, capture) : null;
  const add = (kind, it, detail, hint) => found.push({ kind, id: it.id ?? null, ...(it.of != null ? { of: it.of } : {}), name: it.name, text: it.text, box: [r0(it.x), r0(it.y), r0(it.w), r0(it.h)], detail, ...(hint ? { hint } : {}) });
  const unit = host === 'web' ? 'px' : 'pt';
  const targets = new Map(items.filter(it => it.control && !it.disabled).map(it => [it, tapTarget(it)]));
  const controls = items.filter((it) => it.control && !it.overlay);
  // The controls that stay over what is scrolled under them, whose pixels are theirs
  const staying = (it) => controls.filter((c) => c !== it && clears(obs, it, c));
  for (const it of items) {
    if (it.squeezed?.axis) {
      const { axis, port, content } = it.squeezed;
      add('squeezed', it, { axis, port, content, unit }, SQUEEZED_SCROLL[axis]);
      continue;
    }
    if (it.squeezed) { add('squeezed', it, {}, 'let it keep its words (drop `min-width: 0` or a fixed width beside it), or let the row wrap (`flex-wrap="wrap"`)'); continue; }
    // Covered: something else is over its middle at rest, and no scroller moves it clear.
    if (it.cover && !it.disabled) {
      const what = it.cover.kind === 'keyboard' ? 'the software keyboard' : it.cover.kind === 'sheet' ? 'a sheet' : it.cover.by != null ? { ref: it.cover.by } : it.cover.name;
      add('covered', it, { under: what, bar: it.cover.kind === 'bar' ? it.cover.name : undefined, offscreen: it.cover.kind === 'offscreen' },
        it.cover.kind === 'offscreen' ? OFFSCREEN : it.cover.kind === 'keyboard' ? 'scroll it above the keyboard, or move it before the field' : it.text && !it.control ? 'start the content below what covers it (padding or a divider), or let the bar take its place' : 'move it clear, or give the scroller room at that end');
    }
    // Text that does not fit its box, or an ancestor's clip cutting it. A clamp the author asked for answers the lines
    // past it, and only `text-overflow: ellipsis` (`ellipsis`) a line wider than the box (a word longer than its line:
    // "January" into the next button at the largest text size, expense-tracker, round 22).
    const need = it.text && it.over ? (it.clamp ? [it.ellipsis ? 0 : it.over[0], 0] : it.over) : null;
    if (need && (need[0] >= 2 || need[1] >= 2)) {
      add('clipped', it, { need: need.map(r0), clips: !!it.clips, unit }, it.clips ? 'let it wrap, give its box room, or ask for an ellipsis (`text-overflow: ellipsis` or `line-clamp`)' : 'its box is smaller than its text, which spills over what follows: drop the fixed size, or ask for `line-clamp`');
    } else if (it.text && it.cut) add('clipped', it, { cut: [r0(it.cut.dx), r0(it.cut.dy)], by: it.cut.by, edge: it.cut.edge, unit }, it.cut.edge ? OFFSCREEN : 'the clipping box is smaller than its content: let it grow, or scroll');
    // LLP 1116.009 D5: a written percentage width and the padding it does not include, past what its parent gives it: the cause, named.
    if (it.padded) add('overflow', it, { past: r0(it.padded.dx), of: it.padded.by, pad: r0(it.padded.pad), written: it.padded.width, unit }, 'drop the width (a block or a stretched flex item fills its parent) or say `box-sizing="border-box"`');
    else if (it.outX) add('overflow', it, { past: r0(it.outX), width: vp.w, unit }, ['Svg', 'SvgViewport', 'Image'].includes(nodes.get(it.id)?.type)
      ? 'fit the graphic (`width="100%"`), or clip it in its containing box (`overflow="hidden"`)'
      : 'let it wrap (`flex-wrap: wrap` with a `flex-basis` in rem, `min-width: 0`, `max-width: 100%`), or scroll it inside its own box');
    else if (it.spill) add('overflow', it, { past: r0(it.spill.dx), of: it.spill.by, unit }, 'a flex item\'s width stops at its content: let it shrink (`min-width: 0`, a smaller `padding`), or let the row wrap (`flex-wrap: wrap`) or scroll');
    // A target the author sized under the platform's minimum (on the web 44 px on a touch device, `any-pointer: coarse`, and otherwise WCAG 2.5.8's 24 px, with its spacing exception; 44 pt on iOS).
    // One passing its scroller's end, under a bar or not, which scrolling shows whole (`passing`), is judged by its box
    const tap = targets.get(it);
    const [dx, dy] = it.scrollGain ?? [0, 0];
    if (tap && !it.inline && it.sized === 'author' && (it.reach?.length || !covers(it, obs.keyboard)) &&
        Math.min(tap.w + dx, tap.h + dy) < obs.target - 0.5 && !(it.passing && Math.min(it.w, it.h) >= obs.target - 0.5) &&
        (host !== 'web' || !tap.rects.length || !spacedTarget(tap, targets, obs.target))) {
      const keyboardCuts = obs.keyboard != null && it.y < obs.keyboard && it.y + it.h > obs.keyboard && it.w >= obs.target && it.h >= obs.target;
      add('target', it, { min: obs.target, unit, tap: [r0(tap.w), r0(tap.h)] },
        keyboardCuts ? 'scroll the control above the software keyboard so a finger can reach its full height' : `give it a ${obs.target}×${obs.target} tap area with padding, \`min-height\`/\`min-width\`, or \`-exact-hit-slop\` to keep the drawn size`);
    }
    // Contrast against what it stands on: the fill the host found painted under it (LLP 1116.006 D5), else what the
    // pixels show behind it, read where it is presented (a bar's glass over it is not what it stands on, LLP 1116.007 D3.8).
    if (it.text && it.fg && (it.bg || pixel) && !covers(it, obs.keyboard)) {
      const need = floor(it, obs.scheme, palette), bg = need && (it.bg ?? backdrop(pixel, shown(it, obs.keyboard), staying(it)));
      if (bg) {
        const ink = over(it.fg, bg), got = ratio(ink, bg);
        if (got < need - 0.005) add('contrast', it, { got: Math.floor(got * 10) / 10, need, fg: hex(ink), bg: hex(bg), ...(it.bg ? { on: it.on ?? 'page' } : {}) },
          it.onAccent || it.onAccentFill ? 'the platform draws a filled button\'s label white on the accent: leave the accent unsaid (the platform\'s blue; a brand is an `app.forma`)'
            : it.label ? 'name a `color` for the label, or a fill it reads on in this scheme' : role(it.fgRole) ? `-exact-${role(it.fgRole)} is too light for text on this: use -exact-label or -exact-secondary-label, or keep the colour for a fill or a symbol` : 'use a role (`-exact-label`, `-exact-secondary-label`) or a colour that changes with the scheme');
      }
    }
    // A card (a filled box holding a text or a control; the hosts report no other fill) whose fill is what lies behind
    // it, with no border or shadow: invisible. Each side is read across the card's edge, a pixel inside against one
    // outside. Where the two differ, the edge shows and nothing is found. Where they are alike, the edge does not show
    // only if the inside one is the card's fill over the outside one, and two such sides are needed: where a view is
    // drawn over the edge (a panel, a nearer shape, a scrim) or the box is not the shape (a rotated view's box bounds
    // it), both pixels are something else's (mapapp's park under its panel, photos' hills). A card the host says is
    // `faded` is not judged: an ancestor's opacity mixes it with what lies behind that ancestor, so no colour can be
    // expected at its edge.
    // Only where it is presented: under the keyboard the capture shows the keyboard, not the card's edge (unit-converter, round 9).
    if (it.fill && !it.edge && !it.faded && pixel && !covers(it, obs.keyboard)) {
      const v = shown(it, obs.keyboard), on = (x, y) => x >= v.x - 2.5 && x <= v.x + v.w + 2.5 && y >= v.y - 2.5 && y <= v.y + v.h + 2.5;
      const sides = [[it.x + 2, it.y + it.h / 2, it.x - 2, it.y + it.h / 2], [it.x + it.w - 2, it.y + it.h / 2, it.x + it.w + 2, it.y + it.h / 2],
        [it.x + it.w / 2, it.y + 2, it.x + it.w / 2, it.y - 2], [it.x + it.w / 2, it.y + it.h - 2, it.x + it.w / 2, it.y + it.h + 2]]
        .filter(([ix, iy, ox, oy]) => on(ix, iy) && on(ox, oy))
        .map(([ix, iy, ox, oy]) => [pixel(ix, iy), pixel(ox, oy)]).filter(([a, b]) => a && b);
      const filled = sides.filter(([a, b]) => near(a, over(it.fill, b), 4));
      if (filled.length >= 2 && sides.every(([a, b]) => near(a, b, 4))) {
        const fill = role(it.fillRole), behind = filled[0][1], scheme = obs.scheme ?? 'light';
        const plain = near(behind, palette[scheme].background, 3);
        add('card', it, { fill: fill ? `-exact-${fill}` : hex(it.fill), behind: hex(behind) },
          fill === 'secondary-grouped-background' && plain ? 'on a plain page a card is -exact-secondary-background (-exact-secondary-grouped-background is for a -exact-grouped-background page)'
            : 'give it a fill that differs from the page (-exact-secondary-background on a plain page), a border or a shadow');
      }
    }
  }
  // Where a text's lines are drawn (`ink`, where the host says it is not the box, already cut to a box that clips it),
  // within what a scroller presents of its box (`vis`) and above the keyboard: a rectangle, or one a line for a run of a
  // paragraph that wraps (`lines`), whose box takes in the ends of lines it shares with its neighbours.
  const drawn = (it) => {
    if (it.lines) { const v = shown(it, obs.keyboard); return it.lines.map((l) => meet(l, v)).filter((r) => r.w > 0 && r.h > 0); }
    const k = it.ink ?? it;
    return [meet(k, shown(it.vis ? it : { ...it, ...k }, obs.keyboard))];
  };
  // Where two items' rectangles meet most.
  const meeting = (as, bs) => {
    let best = { w: 0, h: 0, x: 0, y: 0 };
    for (const p of as) for (const q of bs) {
      const i = meet(p, q);
      if (i.w > 0 && i.h > 0 && i.w * i.h > best.w * best.h) best = i;
    }
    return best;
  };
  // Controls overlapping each other, or a text where it is drawn (a word wider than its box runs into the button beside
  // it): neither holds the other and they share more than a sliver of what is presented of each (`vis`: a row its list
  // or scroller cuts overlaps nothing past that edge, LLP 1116.006 D5). A pressable run of a paragraph that wraps meets
  // others only where its lines are.
  const where = (it) => (it.control && !it.lines ? [shown(it, obs.keyboard)] : drawn(it));
  for (const { it, kind, detail } of regionFindings(obs)) add(kind, it, { ...detail, unit }, regionHint(kind));
  // A list's section header (`pinned`) lies over the rows passing under it, as UIKit pins it, and an announcement laid
  // over the page that lets taps through (`overlay`, a toast) over what it passes: no overlap.
  for (const a of controls) for (const b of items) {
    if (a === b || !(b.control || b.text) || b.pinned || b.overlay || (b.control && b.id < a.id)) continue;
    if (!b.control && (b.of === a.id || controls.some((c) => c !== a && holds(c, b)))) continue; // its own label, or another control's
    if (clears(obs, b, a) || (b.control && clears(obs, a, b))) continue; // what scrolls passes under a control that stays (a composer, a floating button)
    const i = meeting(where(a), where(b));
    if (i.w >= 2 && i.h >= 2 && !holds(a, b) && !holds(b, a)) add('overlap', a, { with: b.id, by: [r0(i.w), r0(i.h)] }, 'give them room (a gap, or let the row wrap)');
  }
  // Two texts running into each other: where their lines are drawn (`ink`, where the host says it is not the box) share
  // any width at all. Glyphs a pixel or two into a neighbour's read as one word ("GoingMaybe", event-rsvp at the largest
  // text size, round 21), and two texts set flush on purpose ("$" then "12") only touch.
  const texts = items.filter((it) => it.text && !it.control && !it.squeezed);
  // A text drawn on the opaque fill of the view laid over another's middle (a bar over its list's row) hides that
  // one where they meet, if that view's box holds all of the meeting: they do not run together there (the host's
  // `beneath`, `on` and `solid`; a label running past its bar's edge still meets the row outside it).
  // A control's own label goes with it over what scrolls under it
  const labels = (a, b) => controls.some((c) => c.id === b.of && clears(obs, a, c));
  const hides = (top, below, i) => below.beneath != null && top.on === below.beneath.by && top.solid && holds(below.beneath, i);
  for (const [k, a] of texts.entries()) for (const b of texts.slice(k + 1)) {
    if (holds(a, b) || holds(b, a) || a.pinned || b.pinned || a.overlay || b.overlay || labels(a, b) || labels(b, a)) continue;
    const i = meeting(drawn(a), drawn(b));
    if (hides(a, b, i) || hides(b, a, i)) continue;
    if (i.w > 0.5 && i.h >= 2) add('overlap', a, { with: b.id, by: [Math.round(i.w * 10) / 10, r0(i.h)] }, 'give them room (a gap, or let the row wrap)');
  }
  // A control's own text covered with it is one finding: the control's.
  const coveredControls = found.filter((f) => f.kind === 'covered' && items.find((it) => it.id === f.id)?.control);
  const kept = found.filter((f) => !(f.kind === 'covered' && f.text != null && coveredControls.some((c) => c !== f && holds({ x: c.box[0], y: c.box[1], w: c.box[2], h: c.box[3] }, { x: f.box[0], y: f.box[1], w: f.box[2], h: f.box[3] }))));
  const order = ['covered', 'edge', 'fold', 'squeezed', 'clipped', 'overflow', 'overlap', 'contrast', 'card', 'target'];
  return kept.sort((p, q) => order.indexOf(p.kind) - order.indexOf(q.kind) || p.box[1] - q.box[1] || p.box[0] - q.box[0]);
}
function parseHex(h) { const n = parseInt(String(h).slice(1, 7), 16); return [n >> 16 & 255, n >> 8 & 255, n & 255]; }

/** A node as a builder names it: its testId, else its type and an excerpt of its text, else its id (`tree`'s nodes by id). */
function named(id, nodes, fallback) {
  const n = id == null ? null : nodes.get(id);
  if (!n) return fallback ?? (id == null ? 'a view' : `#${id}`);
  if (n.props?.testId != null) return /\s/.test(n.props.testId) ? JSON.stringify(n.props.testId) : n.props.testId;
  const text = n.props?.text ?? n.props?.accessibilityLabel;
  return `${n.type}${text ? ' ' + JSON.stringify(text.length > 24 ? text.slice(0, 23) + '…' : text) : ''}#${id}`;
}

/** Who a finding is about: a control's label by its control, a native view by its kind and text. */
const who = (f, nodes) => f.of != null && f.text != null ? `${named(f.of, nodes)}'s label ${JSON.stringify(f.text)}` : named(f.id, nodes, f.name ? f.name + (f.text ? ` ${JSON.stringify(f.text)}` : '') : null);

/** The entry of app.json's `verify.accept` that takes a finding as the app's design, or undefined: its kind, and the
 *  testId of its node or of the control whose label it is (`node`; a non-empty prefix and one trailing `*` match a prefix:
 *  a bare `*`, which the schema refuses too, matches nothing, so no entry accepts a kind everywhere). */
export function accepts(entries, f, nodes) {
  const ids = [f.id, f.of].map((id) => (id == null ? null : nodes.get(id)?.props?.testId)).filter((id) => id != null).map(String);
  const takes = (node, id) => (/^[^*]+\*$/.test(node) ? id.startsWith(node.slice(0, -1)) : !node.includes('*') && id === node);
  return entries.find((e) => e.finding === f.kind && ids.some((id) => takes(String(e.node), id)));
}

/** One line a finding. */
export function line(f, nodes) {
  const box = `[${f.box[0]},${f.box[1]} ${f.box[2]}×${f.box[3]}]`, d = f.detail;
  const under = (u) => typeof u === 'object' && u ? named(u.ref, nodes) : u;
  const what = {
    covered: () => d.offscreen ? `its middle is past ${d.under}, and nothing scrolls to it` : `its middle is under ${under(d.under)} at rest${d.bar ? ` (${d.bar})` : ''}, and nothing scrolls it clear`,
    edge: () => regionLine('edge', d, d.unit),
    fold: () => regionLine('fold', d, d.unit),
    clipped: () => d.need ? `its text is ${[d.need[0] ? `${d.need[0]} ${d.unit} wider` : '', d.need[1] ? `${d.need[1]} ${d.unit} taller` : ''].filter(Boolean).join(' and ')} than its box${d.clips ? ', which clips it' : ', and spills'}` : `${d.edge ? `the ${d.edge} of the screen` : named(d.by, nodes)} cuts ${[d.cut[0] ? `${d.cut[0]} ${d.unit} of its width` : '', d.cut[1] ? `${d.cut[1]} ${d.unit} of its height` : ''].filter(Boolean).join(' and ')}`,
    overflow: () => d.pad != null ? `runs ${d.past} ${d.unit} past ${named(d.of, nodes)}'s side: ${d.written != null ? `its width is ${d.written} of its parent's` : 'its content is as wide as its parent\'s'}, and its ${d.pad} ${d.unit} of padding${d.written != null ? ' and border' : ''} add to that (content-box sizing)` : d.of != null ? `runs ${d.past} ${d.unit} past ${named(d.of, nodes)}'s side, and nothing there scrolls or clips it` : `runs ${d.past} ${d.unit} past the ${d.width}-wide viewport's side, and nothing scrolls it`,
    target: () => `${d.tap.join('×')} ${d.unit} tap area (box ${f.box[2]}×${f.box[3]}), under the ${d.min}×${d.min} minimum`,
    contrast: () => `${d.got}:1 (${d.fg} on ${d.bg}${d.on == null ? '' : d.on === 'own' ? ', its own fill' : d.on === 'page' ? ', the page' : `, ${named(d.on, nodes)}'s fill`}), needs ${d.need}:1`,
    card: () => `its fill ${d.fill} is what lies behind it (${d.behind}): it does not show`,
    overlap: () => `overlaps ${named(d.with, nodes)} by ${d.by[0]}×${d.by[1]}`,
    squeezed: () => d.axis ? `its port is squeezed to ${d.port} ${d.unit} ${d.axis === 'y' ? 'tall' : 'wide'} of its ${d.content} ${d.unit} of content, so ${d.port > 0 ? `less than ${d.axis === 'y' ? 'a row' : 'an item'} of it shows` : 'none of it shows'}` : 'its box is squeezed to no width, so none of its text shows',
  }[f.kind]();
  return `  ${f.kind.padEnd(8)} ${who(f, nodes)} ${box}: ${what}${f.hint ? `; ${f.hint}` : ''}`;
}

/** An advisory line, never a finding: a phone's layout at a desktop width, which a composed page may mean (LLP 1116.004 D1.3), or an
 *  open menu the platform presents and the driver paints (`menu`, iOS). */
function advise(a, width, nodes) {
  const who = named(a.id, nodes);
  if (a.kind === 'menu') return `  advice   ${who}: a person sees this open menu as UIKit's ${a.context ? 'context menu' : 'pull-down'}, which the system places, sizes and scrolls to fit the screen; the driver paints it in the page to drive it, so its rows are not judged (\`--touch platform\` presents UIKit's)`;
  return `  advice   ${who}: a bottom tab bar ${r0(a.w)} px wide along a ${width}-px window; placed as docs/recipes/tabs.md places it (the last child of the root's flex column, unpositioned) a wide window draws it as a sidebar, or lay out a rail of your own when the window is wide`;
}

/** The text a check prints: per scheme, its findings (at most SHOWN, then a count), or one line saying there are none; then its advice. */
export function report(runs, trees, host) {
  const out = [];
  for (const [i, run] of runs.entries()) {
    const nodes = trees[i];
    const head = `check ${host} · ${run.scheme}${run.asked && run.asked !== run.scheme ? ` (asked ${run.asked}: the app keeps its own scheme)` : ''}${run.accepted?.length ? ` (${run.accepted.length} accepted)` : ''}`;
    if (run.incomplete) out.push(`${head}: incomplete (${run.incomplete})`);
    else if (!run.findings.length) out.push(`${head}: no findings (text fits, nothing covered or overlapping, targets ≥ ${run.target}, no horizontal overflow${run.regions ? ', edges and folds clear' : ''}, contrast ≥ 4.5:1 or 3:1 large, cards show)`);
    else {
      out.push(`${head}: ${run.findings.length} finding${run.findings.length === 1 ? '' : 's'}`);
      for (const f of run.findings.slice(0, SHOWN)) out.push(line(f, nodes));
      if (run.findings.length > SHOWN) out.push(`  … and ${run.findings.length - SHOWN} more (--json lists them all)`);
    }
    for (const a of run.advice ?? []) out.push(advise(a, run.viewport?.w, nodes));
  }
  return out.join('\n');
}

/** `check [light|dark|both]` on a session: the host's observation and a capture under each scheme asked, judged; the scheme is put back.
 *  `shots(viewport, scheme)` names a file to keep each capture in (verify's), else they go with the check. A finding an
 *  entry of `accept` (app.json `verify.accept`) takes is the run's `accepted`, each with its line and the entry, not a finding. */
export async function checkOp(s, args = [], { shots, accept = [] } = {}) {
  const word = args[0] ?? 'current';
  if (!['current', 'light', 'dark', 'both'].includes(word) || args.length > 1) throw new MalformedOperation('check [light|dark|both]: the presented screen, judged in text; `check dark` under prefers-color-scheme dark, `check both` under each');
  if (!['web', 'ios', 'host-ios'].includes(s.host)) throw new Error(`check: the web and iOS hosts carry it, not ${s.host}`);
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-check-'));
  // The web's wasm target answers the runner's op table, which has no `check`: name that, not the wire's reply.
  const first = await s.op({ op: 'check' }).catch((e) => {
    if (e?.reply?.error !== 'unknown op: check') throw e;
    throw new Error('check: this web build is the wasm target (a game, or `--wasm`), which carries no check; the JS web target and iOS do, so run `agent ios check`');
  });
  const asked = word === 'both' ? ['light', 'dark'] : word === 'current' ? [first.scheme] : [word];
  const runs = [], trees = [];
  let preferred = null;
  try {
    for (const scheme of asked) {
      let obs = first;
      if (scheme !== first.scheme || preferred) { await s.prefer({ 'prefers-color-scheme': scheme }); preferred = scheme; obs = await s.op({ op: 'check' }); }
      const pathFor = viewport => shots?.(viewport, scheme) ?? resolve(dir, `${scheme}.png`);
      let picture = await checkPicture(s, obs, pathFor);
      if (picture.incomplete) {
        const { incomplete, obs: previous } = picture;
        picture = await checkPicture(s, previous, pathFor).catch(() => ({ obs: previous, incomplete }));
      }
      const { path, taken, display, capture, incomplete } = picture;
      obs = picture.obs;
      const run = { scheme: obs.scheme, asked: scheme, target: obs.target, viewport: obs.viewport, ...(obs.owners ? { regions: true } : {}) };
      if (incomplete) {
        trees.push(new Map());
        runs.push({ ...run, findings: [], incomplete });
        break;
      }
      const png = readSrgbPng(path);
      // The tree as this scheme shows it: a view only one scheme shows is named, and accepted, by its own testId.
      const nodes = new Map(((await s.op({ op: 'tree' })).nodes ?? []).map((n) => [n.id, n]));
      trees.push(nodes);
      runs.push({ ...run, findings: judge(obs, png, { host: s.host === 'web' ? 'web' : 'ios', nodes, capture }), ...(obs.advice ? { advice: obs.advice } : {}) });
      // A capture kept for a person or an agent to look at is a pixel a point once judged, as `agent`'s are: a third of an iPhone's pixels each way.
      if (shots && taken?.scale > 1) {
        const w = display ? taken.w : png.width / taken.scale, h = display ? taken.h : png.height / taken.scale;
        writeFileSync(path, encodePng(shrink(png, Math.round(w), Math.round(h))));
      }
    }
  } finally {
    if (preferred && preferred !== first.scheme) await s.prefer({ 'prefers-color-scheme': first.scheme }).catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
  const result = settle(runs, trees, accept, s.host);
  return runs.some(run => run.incomplete) ? result : s.tagged(result);
}

async function checkPicture(s, obs, pathFor) {
  // Keep UIKit's menu substitutes hidden through capture and release them before a retry.
  const held = s.host !== 'web';
  let path, taken, releaseFailed = false;
  try {
    if (held) obs = await s.op({ op: 'check', capture: true });
    path = pathFor(obs.viewport);
    taken = await s.carrier.screenshot(path, obs.capture?.window === true);
  } finally {
    if (held) await s.op({ op: 'check', release: true }).catch(error => {
      if (taken?.captureSpace !== 'screen') throw error;
      releaseFailed = true;
    });
  }
  const display = taken?.captureSpace === 'screen';
  if (!display) return { obs, path, taken, display, capture: obs.capture };
  if (taken.front !== true || releaseFailed) return { obs, incomplete: 'app focus could not be confirmed during display capture' };
  const screen = obs.capture?.screen;
  if (!screen || screen.w !== taken.w || screen.h !== taken.h)
    return { obs, incomplete: 'screen geometry changed between observation and capture' };
  return { obs, path, taken, display, capture: screen };
}

/** The runs judged, each against the tree its own scheme showed (`trees[i]`): what `accept` takes is the run's `accepted`, each
 *  with its line and the entry; every finding is named, with its line (the text shows at most SHOWN). `{check, text}`, as `check` replies. */
export function settle(runs, trees, accept, host) {
  for (const [n, r] of runs.entries()) {
    const nodes = trees[n], taken = r.findings.map((f) => accepts(accept, f, nodes));
    r.accepted = r.findings.flatMap((f, i) => (taken[i] ? [{ ...f, line: `${line({ ...f, hint: undefined }, nodes)}; accepted: ${taken[i].why}`, accept: taken[i] }] : []));
    r.findings = r.findings.filter((_, i) => !taken[i]);
  }
  const named = (fs, nodes) => fs.map((f) => ({ line: line(f, nodes), ...f, node: who(f, nodes) }));
  return { check: runs.map((r, i) => ({ ...r, findings: named(r.findings, trees[i]), accepted: named(r.accepted, trees[i]) })), text: report(runs, trees, host) };
}
