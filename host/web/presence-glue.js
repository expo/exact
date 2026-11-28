// Exit animation and layout transition on the web (LLP 1063). Fetched after
// paint, when a batch first carries either row (`presenceLoader` in
// navigation.js holds a batch that needs it before it arrives); the browser
// plays both.
//
// An `exit` op names the one view of a removed subtree that leaves (the
// kernel's choice) and the CSS `animation` list it leaves with, its
// `@keyframes` already in the page. This module keeps it, out of flow at its
// last box, inert and unnamed, until those animations end; they follow the
// element's own `animation` list, which keeps playing.
//
// `--exact-layout-transition` is `<ms> <delay ms> [spring] <easing>`: a CSS
// easing, which a host writes for a spring as the engine's curve for it from
// rest (`SpringConfig::easing_over`) after the word `spring`, so this module
// computes no curve. A view that declares it is measured before and after each
// batch; a change of its laid-out place within the box it is placed in plays
// back as an additive `translate`, from the change to none, so it composes with
// the authored one and a moving parent carries it. A spring's move adds to the
// moves under way, and an easing's restarts from where the box is shown
// (`move`, LLP 1063 D6). A size change is the surface's alone, as native frame
// animation shows it: the element keeps its laid-out box, so its content and
// children never scale, and a stand-in behind it (the element's background,
// border, radius, shadow and backdrop filter, which it stops painting for the
// while) grows or shrinks from its top-left corner; an element that clips its
// children clips them to the shown box. A native button, whose chrome the
// browser draws, keeps its laid-out size and moves by its centre.
import { unwrapListItem, viewParent } from './navigation.js';
import { translation } from './motion-glue.js';
const LAYOUT = '--exact-layout-transition';
const PAINT = ['backgroundColor', 'backgroundImage', 'backgroundSize', 'backgroundPosition', 'backgroundRepeat', 'backgroundOrigin', 'backgroundClip',
  'borderTop', 'borderRight', 'borderBottom', 'borderLeft', 'borderRadius', 'boxShadow', 'backdropFilter', 'opacity', 'translate', 'rotate', 'scale', 'transform',
  'transformOrigin'];
const CLEAR = { backgroundColor: 'transparent', backgroundImage: 'none', borderTopColor: 'transparent', borderRightColor: 'transparent',
  borderBottomColor: 'transparent', borderLeftColor: 'transparent', boxShadow: 'none', backdropFilter: 'none' };

// A 2D matrix of an element's own transforms about its origin, from its
// computed style: `translate`, `rotate`, `scale`, then `transform`. Its
// translate is read as a hold reads it (motion-glue.js `translation`): a
// length and a percentage compute as one `calc()`, and a percentage is of
// the element's `transform-box`.
function own(el, cs) {
  const [ox, oy] = origin(el, cs);
  const m = new DOMMatrix().translateSelf(ox, oy);
  if (cs.translate !== 'none') m.translateSelf(...translation(el, cs.translate, cs));
  if (cs.rotate !== 'none') {
    const parts = cs.rotate.split(' '), angle = parseFloat(parts.pop());
    const axis = parts.length === 3 ? parts.map(Number) : { x: [1, 0, 0], y: [0, 1, 0] }[parts[0]] ?? [0, 0, 1];
    m.rotateAxisAngleSelf(...axis, angle);
  }
  if (cs.scale !== 'none') { const [sx, sy = sx] = cs.scale.split(' ').map(Number); m.scaleSelf(sx, sy); }
  if (cs.transform !== 'none') m.multiplySelf(new DOMMatrix(cs.transform));
  return m.translateSelf(-ox, -oy);
}

// An element's transform origin in its border box. The origin is in its `transform-box`, whose used value for a CSS
// box is its content box for `content-box` and `fill-box`, and its border box otherwise (CSS Transforms 1,
// `transform-box`). Of a content box, WebKit and Firefox report a percentage resolved against it, and Chromium against
// the border box, so each axis CSS Typed OM keeps (lengths and percentages, an interpolated `calc()` among them) is
// resolved here against the content box, less the room its scrollbars take (CSS Overflow 3, scrollbar sizing).
function origin(el, cs) {
  const reported = cs.transformOrigin.split(' ').map(parseFloat);
  if (cs.transformBox !== 'content-box' && cs.transformBox !== 'fill-box') return reported.slice(0, 2);
  const n = v => parseFloat(v) || 0, [w, h] = size(cs) ?? [0, 0], [bl, br, bt, bb, pl, pr, pt, pb] = [cs.borderLeftWidth,
    cs.borderRightWidth, cs.borderTopWidth, cs.borderBottomWidth, cs.paddingLeft, cs.paddingRight, cs.paddingTop, cs.paddingBottom].map(n);
  // (A scrollbar's room, in whole pixels, is told from layout's rounding by being two or more)
  const gutter = (outer, inner, a, b) => { const g = outer - inner - a - b; return g >= 2 ? g : 0; };
  const gx = gutter(el.offsetWidth, el.clientWidth, bl, br), gy = gutter(el.offsetHeight, el.clientHeight, bt, bb);
  const inset = [bl + pl + (cs.direction === 'rtl' ? gx : 0), bt + pt], content = [w - bl - br - pl - pr - gx, h - bt - bb - pt - pb - gy];
  const axes = el.computedStyleMap?.().get('transform-origin')?.toString().match(/calc\([^)]*\)|\S+/g) ?? [];
  const at = i => {
    let v = 0, terms = 0;
    for (const [, sign, number, unit] of (axes[i] ?? '').matchAll(/([+-]?)\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?)(px|%)/gi)) {
      v += (sign === '-' ? -1 : 1) * Number(number) * (unit === '%' ? content[i] / 100 : 1);
      terms++;
    }
    return terms ? v : reported[i];
  };
  return [inset[0] + at(0), inset[1] + at(1)];
}

// `el`'s paint onto its stand-in `box`, a border box with no padding: its translate as `el` resolves it, and its
// transform origin in its border box, in px, since a percentage of the stand-in's own box (its moving size, and never
// `el`'s `transform-box`) would put them elsewhere.
function paint(box, el, cs) {
  for (const k of PAINT) box.style[k] = cs[k];
  box.style.transformOrigin = origin(el, cs).map(x => `${x}px`).join(' ');
  if (cs.translate === 'none') return;
  const [x, y] = translation(el, cs.translate, cs), z = cs.translate.match(/calc\([^)]*\)|\S+/g)?.[2];
  box.style.translate = `${x}px ${y}px${z ? ` ${z}` : ''}`;
}

// Computed basic shapes keep percentages until the presented border box is known.
// Pixel shapes work on both the final-size content and its moving stand-in.
function shownClip(css, w, h) {
  const match = /^(polygon|inset|circle|ellipse)\((.*)\)$/.exec(css);
  if (!match) return css; // path() already uses CSS pixels.
  const [, kind, body] = match;
  const length = (v, basis) => parseFloat(v) * (v.endsWith('%') ? basis / 100 : 1);
  const words = s => s.trim().split(/\s+/);
  if (kind === 'polygon') {
    const parts = body.split(',').map(s => s.trim());
    const rule = /^(nonzero|evenodd)$/.test(parts[0]) ? parts.shift() + ', ' : '';
    return `path(${rule}"M${parts.map(p => { const [x, y] = words(p); return `${length(x, w)} ${length(y, h)}`; }).join(' L')} Z")`;
  }
  if (kind === 'inset') {
    const [edges, round = '0px'] = body.split(' round ');
    const four = s => { const [a, b = a, c = a, d = b] = words(s); return [a, b, c, d]; };
    let [t, r, b, l] = four(edges).map((v, i) => length(v, i % 2 ? w : h));
    if (t + b > h) { const f = h / (t + b); t *= f; b *= f; }
    if (l + r > w) { const f = w / (l + r); l *= f; r *= f; }
    const iw = Math.max(0, w - l - r), ih = Math.max(0, h - t - b);
    const [rx, ry = rx] = round.split('/');
    const x = four(rx).map(v => length(v, w)), y = four(ry).map(v => length(v, h));
    const f = Math.min(1, ...[x[0] + x[1], x[3] + x[2], y[0] + y[3], y[1] + y[2]].map((sum, i) => sum ? (i < 2 ? iw : ih) / sum : 1));
    for (let i = 0; i < 4; i++) { x[i] *= f; y[i] *= f; }
    const right = l + iw, bottom = t + ih, k = .5522847498307936;
    return `path("M${l + x[0]} ${t} L${right - x[1]} ${t} C${right - x[1] * (1-k)} ${t} ${right} ${t + y[1] * (1-k)} ${right} ${t + y[1]}
      L${right} ${bottom - y[2]} C${right} ${bottom - y[2] * (1-k)} ${right - x[2] * (1-k)} ${bottom} ${right - x[2]} ${bottom}
      L${l + x[3]} ${bottom} C${l + x[3] * (1-k)} ${bottom} ${l} ${bottom - y[3] * (1-k)} ${l} ${bottom - y[3]}
      L${l} ${t + y[0]} C${l} ${t + y[0] * (1-k)} ${l + x[0] * (1-k)} ${t} ${l + x[0]} ${t} Z")`.replace(/\s+/g, ' ');
  }
  const [radii, at = '50% 50%'] = body.split(/\s*at\s+/);
  let [x, y] = words(at);
  if (y === undefined) { if (/^(top|bottom)$/.test(x)) { y = x; x = 'center'; } else y = 'center'; }
  else if (/^(top|bottom)$/.test(x) || /^(left|right)$/.test(y)) [x, y] = [y, x];
  const position = (v, basis) => length(({ left: '0%', top: '0%', right: '100%', bottom: '100%', center: '50%' })[v] ?? v, basis);
  const cx = position(x, w), cy = position(y, h);
  const oval = (rx, ry) => `path("M${cx-rx} ${cy} A${rx} ${ry} 0 1 0 ${cx+rx} ${cy} A${rx} ${ry} 0 1 0 ${cx-rx} ${cy} Z")`;
  const radius = (v, center, extent) => v === 'closest-side' ? Math.min(Math.abs(center), Math.abs(extent-center))
    : v === 'farthest-side' ? Math.max(Math.abs(center), Math.abs(extent-center)) : length(v, extent);
  const [rx = 'closest-side', ry = rx] = radii.trim() ? words(radii) : [];
  if (kind === 'circle') {
    const r = rx.endsWith('side') ? (rx === 'closest-side' ? Math.min : Math.max)(radius(rx, cx, w), radius(rx, cy, h))
      : length(rx, Math.hypot(w, h) / Math.SQRT2);
    return oval(r, r);
  }
  return oval(radius(rx, cx, w), radius(ry, cy, h));
}

// The content clips at the padding edge, inside the shown border and its reduced radii.
function paddingClip(cs, w, h) {
  const edges = ['Top', 'Right', 'Bottom', 'Left'].map(side => parseFloat(cs[`border${side}Width`]));
  const radii = ['TopLeft', 'TopRight', 'BottomRight', 'BottomLeft'].map(corner => {
    const [x, y = x] = cs[`border${corner}Radius`].split(' ');
    return [x, y].map((v, i) => parseFloat(v) * (v.endsWith('%') ? (i ? h : w) / 100 : 1));
  });
  const f = Math.min(1, ...[radii[0][0]+radii[1][0], radii[3][0]+radii[2][0], radii[0][1]+radii[3][1], radii[1][1]+radii[2][1]]
    .map((sum, i) => sum ? (i < 2 ? w : h) / sum : 1));
  const x = radii.map(([r], i) => Math.max(0, r*f - edges[i === 0 || i === 3 ? 3 : 1]));
  const y = radii.map(([, r], i) => Math.max(0, r*f - edges[i < 2 ? 0 : 2]));
  const px = values => values.map(v => `${v}px`).join(' ');
  return shownClip(`inset(${px(edges)} round ${px(x)} / ${px(y)})`, w, h);
}

// SVG clip paths intersect the authored outline with overflow without changing
// the content's layout or losing clip-path's pointer hit testing.
let clipId = 0;
function contentClip(sf) {
  const svg = name => document.createElementNS('http://www.w3.org/2000/svg', name);
  const defs = svg('svg'), clip = svg('clipPath'), padding = svg('clipPath');
  defs.style.cssText = 'position:absolute;width:0;height:0';
  clip.id = `exact-presence-clip-${++clipId}`; padding.id = `${clip.id}-padding`;
  const shape = svg('path'), edge = svg('path');
  shape.setAttribute('clip-path', `url(#${padding.id})`);
  clip.append(shape); padding.append(edge); defs.append(clip, padding); sf.stand.append(defs);
  const set = (path, css) => {
    const [, rule, d] = /^path\((?:(evenodd|nonzero),\s*)?["'](.*)["']\)$/.exec(css);
    path.setAttribute('d', d); path.setAttribute('clip-rule', rule ?? 'nonzero');
  };
  return (css, cs, w, h) => {
    const inner = paddingClip(cs, w, h);
    set(shape, css === 'none' ? inner : css); set(edge, inner);
    return `url(#${clip.id})`;
  };
}

// A box that clips its children clips them to the shown box: it stops clipping
// while its size moves, keeping the layout clipping gave it. A box that
// `hidden` makes a scroll container keeps that container's automatic minimum
// size of 0 as a flex or grid item (CSS Flexbox §4.5, CSS Grid §6.6) and, as
// a block, its own formatting context, held by `display: flow-root`, which
// Firefox does not animate (there it keeps clipping at its laid-out box). A
// box keeps clipping too where clipping places more than that: a scroller or
// a box scrolled, which would lose its offset and its scrollbar's room; an
// inline-level box, whose baseline is its bottom edge only while it clips
// (CSS 2 §10.8.1); and one that `hidden` makes a scroll container while it
// holds a shown sticky box, which might stick to it and would then stick to an
// outer one: any counts, one in a scroller inside it too, which keeps it clipping
// early rather than late (`clip` makes no scroll container). What it holds is
// read from its own style with the hold off, again after each batch, so a
// restyle mid-move is its own.
function unclip(hold, el) {
  hold.effect.setKeyframes([]);
  const cs = getComputedStyle(el), axes = [cs.overflowX, cs.overflowY];
  if (cs.display.startsWith('inline') || axes.some(v => v === 'auto' || v === 'scroll') || el.scrollTop || el.scrollLeft) return;
  if (axes.includes('hidden') && [...el.querySelectorAll('*')].some(d => getComputedStyle(d).position === 'sticky' && d.getClientRects().length)) return;
  const held = { overflow: 'visible' };
  if (axes.includes('hidden')) {
    if (cs.minWidth === 'auto') held.minWidth = '0px';
    if (cs.minHeight === 'auto') held.minHeight = '0px';
    if (cs.display === 'block') {
      // Alone, `flow-root` leaves a clipping block's layout as it is.
      held.display = 'flow-root';
      hold.effect.setKeyframes([{ display: 'flow-root' }, { display: 'flow-root' }]);
      const animates = getComputedStyle(el).display === 'flow-root';
      hold.effect.setKeyframes([]);
      if (!animates) return;
    }
  }
  hold.effect.setKeyframes([held, held]);
}

// The linear part (rotation, scale, skew) of a matrix.
const linear = m => new DOMMatrix([m.a, m.b, m.c, m.d, 0, 0]);

// An element's border-box size from its computed style, sub-pixel and free
// of transforms; null for a box CSS gives no single size (an inline box).
function size(cs) {
  const edges = (a, b) => ['padding', 'border'].reduce((n, e) => n + parseFloat(cs[`${e}${a}${e === 'border' ? 'Width' : ''}`]) + parseFloat(cs[`${e}${b}${e === 'border' ? 'Width' : ''}`]), 0);
  let w = parseFloat(cs.width), h = parseFloat(cs.height);
  if (!(w >= 0 && h >= 0)) return null;
  if (cs.boxSizing !== 'border-box') { w += edges('Left', 'Right'); h += edges('Top', 'Bottom'); }
  return [w, h];
}

// One measure of the page: every element's computed style, size and own
// transform, and the linear map its ancestors draw it with, each read once.
function measure() {
  const known = new Map();
  const of = el => {
    let m = known.get(el);
    if (m) return m;
    const cs = getComputedStyle(el), [w, h] = size(cs) ?? [0, 0], parent = el.parentElement;
    const above = parent && parent !== document.documentElement ? of(parent) : null;
    // (A box `display: contents` makes none of paints no transform of its own)
    m = { cs, w, h, own: cs.display === 'contents' ? new DOMMatrix() : own(el, cs), above: above ? above.above.multiply(linear(above.own)) : new DOMMatrix() };
    // Where its untransformed top-left lands on the page: its bounding rect
    // is the box of its corners through its own and its ancestors' maps.
    const r = el.getBoundingClientRect(), map = m.above.multiply(m.own);
    const corners = [[0, 0], [w, 0], [0, h], [w, h]].map(([x, y]) => map.transformPoint({ x, y }));
    m.at = { x: r.left - Math.min(...corners.map(c => c.x)), y: r.top - Math.min(...corners.map(c => c.y)) };
    m.map = map;
    known.set(el, m);
    return m;
  };
  // Where layout put `el` in `parent`, `[x, y, w, h]` in the parent's own
  // points: sub-pixel and free of ancestral transforms. Row wrappers only
  // position virtual rows (`data-listitemkey`) or supply semantics without
  // a box (`data-exact-list-item`), so measure a row in the list content.
  const measured = (el, parent = measuredIn(el)) => {
    if (!parent || !el.isConnected || !size(getComputedStyle(el))) return null;
    const e = of(el), p = of(parent);
    // The page point of its top-left is the parent's local point through
    // the parent's map: undo that map, then the parent's scroll.
    const d = p.map.inverse().transformPoint({ x: e.at.x - p.at.x, y: e.at.y - p.at.y });
    return [d.x + parent.scrollLeft, d.y + parent.scrollTop, e.w, e.h];
  };
  // The linear map from `parent`'s points to the points a translate on `el` moves it by: the maps of the boxes between
  // them, which a box moved under a scaled one within `parent` has
  measured.linear = (el, parent) => {
    const e = of(el), p = of(parent);
    return e.above.inverse().multiply(p.above.multiply(linear(p.own)));
  };
  // A displacement `[dx, dy]` in `parent`'s points, in the points a translate on `el` moves it by
  measured.carry = (el, parent, [dx, dy]) => {
    const v = measured.linear(el, parent).transformPoint({ x: dx, y: dy });
    return Number.isFinite(v.x) && Number.isFinite(v.y) ? [v.x, v.y] : [dx, dy];
  };
  return measured;
}

// The box an element is measured in: its parent, but for a row (`measure`), past the boxes `display: contents` makes
// none of, whose children lie in their parent's
function measuredIn(el) {
  let p = el.parentElement?.matches('[data-listitemkey], [data-exact-list-item]') ? el.parentElement.parentElement : el.parentElement;
  while (p && p !== document.documentElement && getComputedStyle(p).display === 'contents') p = p.parentElement;
  return p;
}

// What a layout transition moves: an element's box, or a native button's centre with no size, as the kernel's
// `layout_box` gives it. The browser draws its chrome at its laid-out size, so only its place moves (LLP 1069.011 D6).
const moving = (el, at) => at && el.matches('button[data-button-style]') ? [at[0] + at[2] / 2, at[1] + at[3] / 2, 0, 0] : at;

// Whether a batch changes no tree: the JS target says so (`still`, rt.js `commit`); the wasm host's holds only its clock
// stamp (`at`).
const changesNoTree = batch => batch.still ?? !batch.ops?.some(op => op.op !== 'at');

function createPresence(root) {
  // Every view that declared the row before this module arrived.
  const tracked = new Set([...root.querySelectorAll('[style*="--exact-layout-transition"]')]);
  // element -> { base, moves, surface }: each moving element's zero `translate`, its moves under way (each its
  // animation and the parts that show it on the surface), and the surface's stand-in.
  const flips = new Map();
  const surfaces = new Set();
  // element -> its row's text and what `row` read of it
  const rows = new WeakMap();
  // A ghost cannot escape its parent's turn (LLP 1083.000 D4). Keep the
  // parent's current row beneath this temporary write, including changes
  // made by a later batch, and restore it when the last ghost ends.
  const ghostParents = new Map();
  const restoreIsolation = (el, held) => {
    if (held.value) el.style.setProperty('isolation', held.value, held.priority);
    else el.style.removeProperty('isolation');
  };
  let frame = null;
  // What a leaving view's ghost keeps (LLP 1063 D4), read as it leaves: the `left` and `top` that keep its border box
  // where it is (its offsets less its margins), its border-box size, and whether its `z-index` applies (a positioned
  // box's or a flex or grid item's)
  const kept = el => {
    const cs = getComputedStyle(el), parent = viewParent(el);
    return [[el.offsetLeft - parseFloat(cs.marginLeft), el.offsetTop - parseFloat(cs.marginTop), el.offsetWidth, el.offsetHeight],
      cs.position !== 'static' || /flex|grid/.test(getComputedStyle(parent).display)];
  };
  // Motion lets go of a view that leaves and of every view inside it, before the ghost is read and written: a lift
  // ends, and a hold or lift that ends later writes nothing to the ghost (motion-glue.js `letGo`)
  const release = leaving => {
    for (const [el, letGo] of globalThis.exact?.motionHolds ?? []) if (leaving.contains(el)) letGo();
    for (const fn of globalThis.exact?.timelineReleasers ?? []) fn(leaving);
  };
  let first = new Map();
  // Whether the batch `before` last read changes no tree, which `after` reads for the same batch; a batch `before` did
  // not read counts as changing one
  let stillBatch = false;
  const exitBoxes = new Map();

  // Read the element's presented paint without our transparent surface or
  // layout offset. Paint animations keep their clocks: only their painted
  // values are hidden while the stand-in draws them. This includes CSS
  // transitions, which outrank the transparent Web Animation in the cascade.
  function reveal(f) {
    const sf = f.surface;
    if (!sf) return;
    sf.hide.effect.target = null;
    if (sf.shape) sf.shape.effect.target = null;
    for (const [a, frames] of sf.hidden) a.effect.setKeyframes(frames);
    sf.hidden.clear();
  }

  // A move that shows nothing more without having finished (paused and sought past its end, or taken off its element)
  // ends
  function retire() {
    for (const [el, f] of flips) {
      if (!el.isConnected) { stop(el); continue; }
      // (A stand-in its element left behind, moved within the box it is measured in, goes beside it again, which `sync`
      // places; one it left that box for ended with its moves, `after`)
      if (f.surface && f.surface.stand.nextElementSibling !== el) el.before(f.surface.stand);
      for (const m of [...f.moves]) if (m.animation.effect?.target !== el || m.animation.effect.getComputedTiming().progress === null) end(el, m);
    }
  }

  function sync(placed = false) {
    retire();
    for (const el of surfaces) {
      const f = flips.get(el), sf = f.surface;
      reveal(f);
      for (const m of f.moves) m.animation.effect.target = null;
      const cs = getComputedStyle(el);
      paint(sf.box, el, cs);
      if (placed) cover(sf, el);
      if (sf.shape) {
        const [w, h] = size(getComputedStyle(sf.box));
        const clipPath = shownClip(cs.clipPath, w, h);
        sf.box.style.clipPath = clipPath;
        const content = sf.contentClip ? sf.contentClip(clipPath, cs, w, h) : clipPath;
        sf.shape.effect.setKeyframes([{ clipPath: content }, { clipPath: content }]);
        sf.shape.effect.target = el;
      }
      for (const a of el.getAnimations()) {
        const frames = a.effect.getKeyframes();
        if (!frames.some(frame => Object.keys(CLEAR).some(k => k in frame))) continue;
        sf.hidden.set(a, frames);
        a.effect.setKeyframes(frames.map(frame => Object.fromEntries(Object.entries(frame).map(([k, v]) => [k, CLEAR[k] ?? v]))));
      }
      for (const m of f.moves) m.animation.effect.target = el;
      sf.hide.effect.target = el;
    }
    if (flips.size && frame === null) frame = requestAnimationFrame(() => { frame = null; sync(); });
  }

  function stop(el) {
    const f = flips.get(el);
    if (!f) return;
    flips.delete(el);
    unsurface(el, f);
    for (const a of [f.base, ...f.moves.flatMap(parts)]) a.cancel();
    if (!flips.size && frame !== null) { cancelAnimationFrame(frame); frame = null; }
  }

  // A move that ends takes its parts with it; the element's last takes its base and surface too
  function end(el, m) {
    const f = flips.get(el);
    if (!f?.moves.includes(m)) return;
    f.moves.splice(f.moves.indexOf(m), 1);
    for (const a of parts(m)) a.cancel();
    if (!f.moves.length) { stop(el); return; }
    // With the last move that changes the size, the surface goes: the element paints itself again, in its own turn,
    // and the moves left wait on it, keeping their clocks
    if (m.box && f.surface && !f.moves.some(o => o.box)) {
      unsurface(el, f);
      for (const o of f.moves) { o.stand.effect.target = el; o.stand.effect.setKeyframes([]); }
    }
  }

  // A move's animations: the element's, and its stand-in's place and size
  const parts = m => [m.animation, m.stand, ...m.box ? [m.box] : []];

  // The element's surface taken away, its moves' parts on it with it unless a new surface takes them
  function unsurface(el, f) {
    const sf = f.surface;
    if (!sf) return;
    reveal(f);
    for (const a of sf.parts) a.cancel();
    sf.stand.remove();
    surfaces.delete(el);
    f.surface = null;
  }

  // The element's surface, standing in for it behind it from a move that
  // changes its size until the last such move ends: a zero-size anchor before it
  // (so the size never moves the anchor, whatever its parent's alignment),
  // moved by each move, and in it a box with the element's background,
  // border, radius, shadow, backdrop filter and own transforms, over the
  // element's laid-out box, to whose size each move adds. Behind its
  // content in the parent's stacking context, isolated structurally by the
  // kernel. Its parts (the element's paint hidden, the shown clip, the clip
  // held off) are held, as the element's base is.
  function surface(el, measured) {
    const cs = getComputedStyle(el), stand = document.createElement('div'), box = document.createElement('div');
    stand.setAttribute('data-exiting', ''); stand.setAttribute('aria-hidden', 'true'); stand.inert = true;
    // (Its moves add to a zero `translate`, which WebKit paints where it places them)
    // (and horizontal, left to right, so that a box its borders hold wider than its moves would keeps its left edge)
    stand.style.cssText = 'position:absolute;width:0;height:0;margin:0;padding:0;border:0;z-index:-1;pointer-events:none;translate:0px 0px;direction:ltr;writing-mode:horizontal-tb';
    el.before(stand);
    box.style.cssText = 'position:absolute;box-sizing:border-box;margin:0';
    paint(box, el, cs);
    stand.append(box);
    const sf = { stand, box, parts: [], hidden: new Map(), clips: cs.overflowX !== 'visible' || cs.overflowY !== 'visible', clipPath: cs.clipPath };
    cover(sf, el, measured);
    const held = frames => { const a = hold(el.animate(frames, { duration: 1 })); sf.parts.push(a); return a; };
    sf.hide = held([CLEAR, CLEAR]);
    // (`sync` gives the clip its shape at the box's shown size)
    if (sf.clipPath !== 'none' || sf.clips) {
      if (sf.clips) sf.contentClip = contentClip(sf);
      sf.shape = held([]);
    }
    if (sf.clips) {
      sf.unclip = held([]);
      unclip(sf.unclip, el);
    }
    surfaces.add(el);
    return sf;
  }

  // The surface's box over the element's laid-out box, spanning it by its four offsets, so that each move adds its size
  // to the box's right and bottom margins, which (unlike a width) may be negative in every engine
  function cover(sf, el, measured = measure()) {
    const [x, y, w, h] = measured(el, viewParent(el)), [ax, ay] = measured(sf.stand, viewParent(el)), [left, top] = [x - ax, y - ay];
    Object.assign(sf.box.style, { left: `${left}px`, top: `${top}px`, right: `${-left - w}px`, bottom: `${-top - h}px` });
  }

  // A row's timing and whether it is a spring's (`spring`), or null for no row or one the browser refuses (such as a
  // curve whose numbers overflow its floats), which ends the moves under way as no row does
  function row(el) {
    const text = el.style.getPropertyValue(LAYOUT).trim();
    if (!text) return null;
    const known = rows.get(el);
    if (known?.text === text) return known.row;
    const [ms, delay, ...words] = text.split(' '), spring = words[0] === 'spring';
    const timing = { duration: Number(ms), delay: Number(delay), easing: words.slice(spring ? 1 : 0).join(' '), fill: 'backwards' };
    let read = null;
    try { new KeyframeEffect(null, null, timing); read = { timing, spring }; } catch {}
    rows.set(el, { text, row: read });
    return read;
  }

  // Play a change of `el`'s laid-out box back from `d`, `[dx, dy, dw, dh]` from where it is now laid out, to none, on
  // its row's curve. A spring's move adds to the moves under way: on the same spring, with no delay, each earlier move
  // past its own, the box's place then follows the engine's spring released where it is shown (its size before the
  // floor), with its velocity, each release to its own stops (LLP 1063 D6). An easing's restarts from where the box is
  // shown, as a CSS transition does (CSS Transitions 1 §3.1, at its full duration even when it reverses): the moves
  // under way end, and it plays what they had left as well, before the size's floor, and nothing when that and the
  // change cancel.
  function move(el, d, measured) {
    const r = row(el);
    if (!r) { stop(el); return; }
    const { timing, spring } = r;
    let f = flips.get(el);
    if (f && !spring) {
      // What the moves under way had left, `o.d · (1 − progress)` each, summed as `Σ o.d − Σ o.d · progress` so that no
      // `1 − progress` rounds it; what cancels to within the rounding of those sums' terms cancels
      const whole = d.slice(), shown = d.map(() => 0);
      for (const o of f.moves) {
        const p = o.animation.effect.getComputedTiming().progress ?? 1;
        o.d.forEach((x, i) => { whole[i] += x; shown[i] += x * p; });
      }
      const rounding = (2 * f.moves.length + 2) * Number.EPSILON;
      d = whole.map((x, i) => { const r = x - shown[i]; return Math.abs(r) <= rounding * (Math.abs(x) + Math.abs(shown[i])) ? 0 : r; });
      stop(el);
      f = null;
    }
    if (!(timing.duration + timing.delay > 0) || d.every(x => x === 0)) return;
    if (!f) {
      // A zero translate under the element's moves gives them a base other than `none` and adds no displacement:
      // WebKit paints a translate added to `none` farther from the box than it places it, and one added to `0px 0px`
      // where it places it
      const base = hold(el.animate([{ translate: '0px 0px' }, { translate: '0px 0px' }], { duration: 1, composite: 'add' }));
      base.id = 'exact-layout-base';
      f = { base, moves: [], surface: null };
      flips.set(el, f);
    }
    const m = { d, timing, frames: [{ translate: `${d[0]}px ${d[1]}px` }, { translate: '0px 0px' }] };
    if (d[2] || d[3]) resurface(el, f, measured);
    const sf = f.surface, added = { ...timing, composite: 'add' };
    m.animation = el.animate(m.frames, added);
    m.animation.id = 'exact-layout-move';
    // Its stand-in's place, on the element and doing nothing until a surface takes it
    m.stand = sf ? sf.stand.animate(m.frames, added) : el.animate([], added);
    m.stand.id = 'exact-layout-stand';
    if (d[2] || d[3]) m.box = sf.box.animate([{ marginRight: `${-d[2]}px`, marginBottom: `${-d[3]}px` }, { marginRight: '0px', marginBottom: '0px' }], added);
    f.moves.push(m);
    extend([f.base, ...sf?.parts ?? []], timing);
    const ended = () => end(el, m);
    m.animation.finished.then(ended, ended);
  }

  // Rewrite the moves under way of `el` through the change of the map from the box it is measured in to its translate
  // points, `was` before a batch and `now` after it, so that each goes on showing the box where it did in that box: a
  // batch that moves the box under a scaled or turned one, or scales or turns one between, changes it
  function remap(el, was, now) {
    const f = flips.get(el), back = was.inverse();
    // (A map with no inverse, under a box scaled to nothing, says nothing of where the moves showed the box)
    if (!f || ![back.a, back.b, back.c, back.d, now.a, now.b, now.c, now.d].every(Number.isFinite)) return;
    const change = now.multiply(back);
    if ([change.a - 1, change.b, change.c, change.d - 1].every(x => Math.abs(x) < 1e-9)) return;
    for (const o of f.moves) {
      const v = change.transformPoint({ x: o.d[0], y: o.d[1] });
      o.d = [v.x, v.y, o.d[2], o.d[3]];
      o.frames = [{ translate: `${v.x}px ${v.y}px` }, { translate: '0px 0px' }];
      o.animation.effect.setKeyframes(o.frames);
      // (A stand-in part plays nothing until a surface takes it, `move`)
      if (f.surface) o.stand.effect.setKeyframes(o.frames);
    }
  }

  // The element's surface made anew, from its style now, the moves under way moving their stand-in parts onto it and
  // keeping their clocks: at a move that changes its size, and when what it clips changes
  function resurface(el, f, measured) {
    unsurface(el, f);
    f.surface = surface(el, measured);
    for (const o of f.moves) {
      o.stand.effect.target = f.surface.stand;
      o.stand.effect.setKeyframes(o.frames);
      if (o.box) o.box.effect.target = f.surface.box;
      extend(f.surface.parts, o.timing);
    }
  }

  // Held animations last as long as every move they hold for, one after another, so that something that plays or seeks
  // them with the moves still finds them: the moves under way are never apart, so while they play their lengths
  // together bound the time since the hold began
  function extend(held, timing) {
    for (const a of held) a.effect.updateTiming({ duration: a.effect.getTiming().duration + Math.max(0, timing.delay + timing.duration) });
  }

  // An animation held at its first frame until the module cancels it: the agent's clock leaves a paused one where it
  // is (navigation/motion.js `animationClock`), and it fills neither way, so even played or finished by something else
  // it never takes the place of an older animation that fills (Web Animations 1, "Replacing animations")
  function hold(a) {
    a.pause();
    return a;
  }

  return {
    sync,
    // Whether a view declares a layout transition: an agent's jump then
    // applies each due time's commits as their own batch (glue.js `clock`).
    tracking() { return tracked.size > 0; },
    // Agent state reads the painted surface, including unnamed exit ghosts.
    // The ordinary layout box still describes the content's final geometry.
    observation() {
      return [...root.querySelectorAll('[data-view]')].filter(el =>
        el.style.getPropertyValue(LAYOUT) || el.style.getPropertyValue('--exact-exit-animation') || el.hasAttribute('data-exiting')
      ).map(el => {
        const shown = flips.get(el)?.surface?.box ?? el, r = shown.getBoundingClientRect();
        return { id: Number(el.dataset.view), x: r.x, y: r.y, w: r.width, h: r.height,
          opacity: Number(getComputedStyle(el).opacity), exiting: el.hasAttribute('data-exiting') };
      });
    },
    // Before a batch's ops: the place of every view that declares the row or
    // gains it here, with what it has still to go (a view that gains the row
    // moves from where it was, as a CSS transition gained with its change
    // runs), then the leaving views, before any op moves them. A batch that
    // changes no tree moves no view, so none is measured: the JS target
    // says which (`still`, rt.js `commit`), and the wasm host's then holds
    // only its clock stamp (`at`). A view the page no longer holds is
    // tracked no longer, whether or not the batch measures.
    before(batch, views, departures = []) {
      first = new Map();
      const place = measure();
      for (const el of surfaces) reveal(flips.get(el));
      for (const el of tracked) if (!el.isConnected) tracked.delete(el);
      const still = stillBatch = changesNoTree(batch);
      if (batch.presenceSnap) { for (const el of tracked) stop(el); }
      else if (!still) {
        // Before any box is read, so that none is read in a box a move that shows nothing more still contains it in
        retire();
        const gains = (batch.ops ?? []).filter(op => op.op === 'style' && op.css?.includes(LAYOUT)).map(op => views.get(op.id));
        for (const el of new Set([...tracked, ...gains])) {
          // (A hidden element has no box to move from, though its computed size stays what was written)
          const parent = el && measuredIn(el), at = el && el.getClientRects().length ? moving(el, place(el, parent)) : null;
          // (and, while it moves, the map its moves are in)
          if (at) first.set(el, [at, parent, flips.has(el) ? place.linear(el, parent) : null]);
        }
      }
      // The batch's leaving views, each read before any exit takes one out of flow
      const exits = (batch.ops ?? []).filter(op => op.op === 'exit').map(op => [views.get(op.id), op.css]).filter(([el]) => el?.isConnected);
      exitBoxes.clear();
      // Only a confirmed exit releases holds. A JS region may rescue portal content as it leaves.
      for (const [el] of exits) release(el);
      for (const el of new Set([...departures, ...exits.map(([el]) => el)])) {
        exitBoxes.set(el, { kept: kept(el), block: el.offsetParent, local: place(el, root) });
      }
      // Let this commit update the underlying row before reapplying the
      // ghost policy in after(). Both run in the same task, before paint.
      for (const [parent, held] of ghostParents) {
        restoreIsolation(parent, held);
        held.released = true;
      }
      for (const [el, css] of exits) this.exit(el, css);
    },
    exit(el, css) {
      if (!el?.isConnected || !css) return;
      stop(el);
      release(el);
      // Read existing animations (inline or class) and their timelines before
      // `data-exiting` or writing the inline `animation` shorthand overrides them.
      const splitList = t => {
        const out = [];
        let d = 0, start = 0;
        for (let i = 0; i < t.length; i++) {
          const c = t[i];
          if (c === '(') d++;
          else if (c === ')') d--;
          else if (c === ',' && !d) { out.push(t.slice(start, i).trim()); start = i + 1; }
        }
        out.push(t.slice(start).trim());
        return out;
      };
      const cs = getComputedStyle(el);
      const existingNames = cs.animationName && cs.animationName !== 'none' ? splitList(cs.animationName) : [];
      const dur = existingNames.length ? splitList(cs.animationDuration) : [];
      const tf = existingNames.length ? splitList(cs.animationTimingFunction) : [];
      const del = existingNames.length ? splitList(cs.animationDelay) : [];
      const iter = existingNames.length ? splitList(cs.animationIterationCount) : [];
      const dir = existingNames.length ? splitList(cs.animationDirection) : [];
      const fill = existingNames.length ? splitList(cs.animationFillMode) : [];
      const ps = existingNames.length ? splitList(cs.animationPlayState) : [];
      const tl = cs.animationTimeline ? splitList(cs.animationTimeline) : [];
      const rs = cs.animationRangeStart ? splitList(cs.animationRangeStart) : [];
      const re = cs.animationRangeEnd ? splitList(cs.animationRangeEnd) : [];
      const prevCss = new Set(el.getAnimations().filter(a => a.animationName !== undefined));
      const [box, ranked] = exitBoxes.get(el)?.kept ?? kept(el), s = el.style;
      if (el.parentElement.hasAttribute('data-exact-list-item')) unwrapListItem(el.parentElement);
      const parent = el.parentElement;
      // No input, no accessibility, no focus, no ids: a view re-created with
      // the same key while this one leaves is the only one with its names.
      el.setAttribute('data-exiting', '');
      el.setAttribute('aria-hidden', 'true');
      el.inert = true;
      // The outgoing transition keeps its IDs and ends at the logically removed root.
      for (const n of [el, ...el.querySelectorAll('[id]')]) { n.exactExitId = n.id; n.removeAttribute('id'); }
      el.addEventListener('focusout', event => event.stopPropagation());
      tracked.delete(el);
      // Its authored transitions do not animate it out of flow: the ghost
      // takes its box at once (and a transition still running ends there).
      s.transition = 'none';
      s.scrollTimelineName = 'none'; s.viewTimelineName = 'none';
      for (const n of el.querySelectorAll('[style*="--exact-scroll-timeline"],[style*="--exact-view-timeline"]')) { n.style.scrollTimelineName = 'none'; n.style.viewTimelineName = 'none'; }
      s.position = 'absolute'; s.left = `${box[0]}px`; s.top = `${box[1]}px`;
      s.width = `${box[2]}px`; s.height = `${box[3]}px`; s.boxSizing = 'border-box'; s.pointerEvents = 'none';
      // It keeps its paint order among its siblings (LLP 1083.000 D4): its own `z-index`, or level 0 in tree
      // order, as a positioned box. A `z-index` that had no effect before it was positioned still has none.
      if (!ranked) s.zIndex = 'auto';
      // And it paints its subtree in its turn, as a ghost does on Apple: a sticky or fixed box was a stacking
      // context by its position alone, which an absolute one with no `z-index` is not
      s.isolation = 'isolate';
      let held = ghostParents.get(parent);
      if (!held) {
        held = { count: 0, value: parent.style.getPropertyValue('isolation'), priority: parent.style.getPropertyPriority('isolation'), released: false };
        ghostParents.set(parent, held);
        parent.$ghostIsolation = held; // paint.js updates the underlying decision during a ghost.
      }
      held.count++;
      if (!held.released) parent.style.setProperty('isolation', 'isolate');
      // After its own animations, which keep playing on their timelines: a name
      // it already plays is a second entry, which starts now on the clock.
      const prev = existingNames.map((n, i) => [
        dur[i % dur.length], tf[i % tf.length], del[i % del.length],
        iter[i % iter.length], dir[i % dir.length], fill[i % fill.length],
        ps[i % ps.length], n,
      ].join(' '));
      const exitCount = splitList(css).length;
      s.animation = prev.length ? `${prev.join(', ')}, ${css}` : css;
      if (tl.some(v => v !== 'auto')) {
        s.animationTimeline = [
          ...existingNames.map((_, i) => tl[i % tl.length]),
          ...Array(exitCount).fill('auto'),
        ].join(', ');
      }
      if (rs.some(v => v !== 'normal') || re.some(v => v !== 'normal')) {
        s.animationRangeStart = [
          ...existingNames.map((_, i) => rs[i % rs.length]),
          ...Array(exitCount).fill('normal'),
        ].join(', ');
        s.animationRangeEnd = [
          ...existingNames.map((_, i) => re[i % re.length]),
          ...Array(exitCount).fill('normal'),
        ].join(', ');
      }
      // CSS Animations 2 §4.3 orders CSSAnimation objects by `animation-name`
      // list order, whereas duplicate names reuse from the end; restart any
      // reused finished animation dropped by no-fill timing, then take trailing exits.
      const allCss0 = el.getAnimations().filter(a => a.animationName !== undefined);
      const reused = [...prevCss].filter(a => a.playState === 'finished' && !allCss0.includes(a));
      for (const a of reused) { try { a.currentTime = 0; a.play(); } catch {} }
      const allCss = reused.length ? el.getAnimations().filter(a => a.animationName !== undefined) : allCss0;
      const take = Math.min(exitCount, Math.max(0, allCss.length - prevCss.size));
      const leaving = take ? allCss.slice(allCss.length - take) : [];
      for (const a of leaving) if (prevCss.has(a)) { try { a.currentTime = 0; a.play(); } catch {} }
      const done = () => {
        // The moves inside it go with it, which no later batch would see
        for (const [child] of [...flips]) if (el.contains(child)) stop(child);
        el.remove();
        if (--held.count === 0) {
          restoreIsolation(parent, held);
          ghostParents.delete(parent);
          delete parent.$ghostIsolation;
        }
      };
      Promise.all(leaving.map(a => a.finished)).then(done, done);
    },
    // Whether a destroyed view stays in the page: a leaving one and its subtree.
    keeps(el) { return el.closest('[data-exiting]') !== null; },
    // After a batch's ops: which views declare the row now, and every one
    // whose box moved plays back from where it was. All are measured before
    // any starts, so no move reads another's first frame.
    after(batch, views) {
      // A restyle can establish or drop the containing block the saved offsets were relative to.
      // Express the old box in the new block before measuring surviving layout transitions.
      const locate = measure(), corrections = [];
      for (const [el, { block, local }] of exitBoxes) {
        if (!local || !el.hasAttribute('data-exiting') || block === el.offsetParent) continue;
        const at = locate(el, root);
        // Carry the difference from the root's points into the points `left` and `top` move the ghost by
        const [dx, dy] = at ? locate.carry(el, root, [local[0] - at[0], local[1] - at[1]]) : [];
        if (at) corrections.push([el.style, parseFloat(el.style.left) + dx, parseFloat(el.style.top) + dy]);
      }
      for (const [style, x, y] of corrections) { style.left = `${x}px`; style.top = `${y}px`; }
      exitBoxes.clear();
      const restyled = new Set((batch.ops ?? []).filter(op => op.op === 'style').map(op => views.get(op.id)));
      for (const [parent, held] of ghostParents) {
        if (held.released || restyled.has(parent)) {
          held.value = parent.style.getPropertyValue('isolation');
          held.priority = parent.style.getPropertyPriority('isolation');
          held.released = false;
        }
        parent.style.setProperty('isolation', 'isolate');
      }
      for (const op of batch.ops ?? []) {
        if (op.op !== 'create' && op.op !== 'style') continue;
        const el = views.get(op.id);
        if (!el) continue;
        // (One the browser refuses is still measured, so that it moves once its row turns valid)
        if (el.style.getPropertyValue(LAYOUT)) tracked.add(el);
        else tracked.delete(el);
        if (!row(el)) stop(el);
      }
      // A move whose box is gone (a restyle to `display: none`) ends with it, a native button's too, which moves with no
      // stand-in. A batch that changes no tree, as `before` read it (the JS target's batch here holds only what it
      // synthesizes), hides none, so a move with no surface reads no box (GitHub #747), though a surface's size and hold
      // are read again after each batch; a removed element is let go unmeasured.
      const still = stillBatch, running = [...flips], reclip = new Map();
      stillBatch = false;
      // Every hold off first, so one an ancestor's surface holds (`flow-root`) gives no box to what it hides
      for (const [, { surface: sf }] of running) sf?.unclip?.effect.setKeyframes([]);
      for (const [el, { surface: sf }] of running) {
        if (!el.isConnected) { stop(el); continue; }
        // (Its computed size stays what was written, so the box is what tells; a surface needs a size to place)
        if (!still && !el.getClientRects().length || sf && !size(getComputedStyle(el))) { stop(el); continue; }
        // A surface whose element now clips otherwise than when it was made is made anew (its hold is off here), once
        // the batch's moves are known, unless one that changes the size makes it anew first
        const cs = sf && getComputedStyle(el), clips = cs && (cs.overflowX !== 'visible' || cs.overflowY !== 'visible');
        if (sf && (clips !== sf.clips || (clips || cs.clipPath !== 'none') !== !!sf.shape)) reclip.set(el, sf);
        else if (sf?.unclip) unclip(sf.unclip, el);
      }
      const place = measure(), moves = [], remaps = [];
      for (const [el, [was, parent, map]] of first) {
        if (!tracked.has(el)) continue;
        // Measured again in the box it was measured in; one the batch takes out of that box, or that box out of being one,
        // is in other coordinates, and its moves end there
        if (!parent?.contains(el) || getComputedStyle(parent).display === 'contents') { stop(el); continue; }
        const at = el.isConnected && el.getClientRects().length ? moving(el, place(el, parent)) : null;
        if (at && map) remaps.push([el, map, place.linear(el, parent)]);
        if (at && at.some((x, i) => Math.abs(x - was[i]) >= 0.01)) {
          const [dx, dy, dw, dh] = at.map((x, i) => was[i] - x);
          moves.push([el, [...place.carry(el, parent, [dx, dy]), dw, dh]]);
        }
      }
      // The moves under way rewritten once every box is measured, so that none is read through a rewritten one, and
      // before the new moves, an easing's restart reading what they had left
      for (const [el, was, now] of remaps) remap(el, was, now);
      for (const [el, d] of moves) move(el, d, measure());
      for (const [el, sf] of reclip) { const f = flips.get(el); if (f?.surface === sf) resurface(el, f, measure()); }
      first = new Map();
      sync(true);
    },
  };
}

if (globalThis.exact) globalThis.exact.presence = createPresence;
// A box's border-box size from its computed style, shared with geometry
// reads' `measure` (geometry-glue.js, LLP 1051.000 D4).
export { own, size, shownClip };
