// `check` on the web (LLP 1116.003 D8): what the presented page shows, as
// observations the driver judges (scripts/agent-check.mjs): each view's box in
// the viewport, a text's own text, ink and type, whether it fits its box or an
// ancestor's clip cuts it, a control and who sized it, what lies over its
// middle at rest, a filled box's fill and edge, and how far a view runs past
// the viewport's sides. Read from the page, never a second model; the
// background a text stands on is the nearest fill painted under it (LLP
// 1116.006 D5), else the driver's, from the pixels.
import { inertAncestor, viewChildren } from './navigation.js';
const CONTROL = 'button, a[href], input:not([type="hidden"]), select, textarea, [role="button"], [role="link"], [role="tab"], [role="switch"], [role="checkbox"], [role="radio"], [role="menuitem"], [data-exact-on~="press"]';
// A user agent's own control at its own size (WCAG 2.5.8's exception), unless the author sized it.
const AGENT_SIZED = 'input[type="checkbox"], input[type="radio"], input[type="range"], input[type="color"], input[type="date"], input[type="time"], input[type="datetime-local"], select, progress';
const SIZE = ['width', 'height', 'min-width', 'min-height', 'max-width', 'max-height', 'padding-top', 'padding-bottom', 'padding-left', 'padding-right', 'flex-basis'];

// Partition a target at the edges where another box, a clip or a hit-slop pseudo-element can
// change the hit. A strip belongs to the control elementFromPoint actually reaches. Joining
// adjacent strips keeps the usual rectangular target one rectangle, however many labels it holds.
function targetReach(el, bounds, edges, curved, owner) {
  const left = Math.max(0, bounds.left), right = Math.min(innerWidth, bounds.right);
  const top = Math.max(0, bounds.top), bottom = Math.min(innerHeight, globalThis.visualViewport?.height ?? innerHeight, bounds.bottom);
  if (right <= left || bottom <= top) return [];
  const nearby = edges.filter(b => b.left < right && left < b.right && b.top < bottom && top < b.bottom);
  const fine = curved.filter(b => b.left < right && left < b.right && b.top < bottom && top < b.bottom);
  const cuts = (lo, hi, low, high, boxes, shapes) => {
    const points = new Set([lo, hi]);
    for (const b of boxes) for (const v of [b[low], b[high]]) if (lo < v && v < hi) points.add(v);
    // A curve or a tilted edge can cross between two box edges. Measure there in CSS pixels.
    for (const b of shapes) for (let v = Math.ceil(Math.max(lo, b[low])); v < Math.min(hi, b[high]); v++) points.add(v);
    return [...points].sort((a, b) => a - b);
  };
  const ys = cuts(top, bottom, 'top', 'bottom', nearby, fine), rects = [];
  let previous = new Map();
  for (let j = 1; j < ys.length; j++) {
    const y = (ys[j - 1] + ys[j]) / 2, row = new Map();
    const atY = b => b.top < y && y < b.bottom;
    const xs = cuts(left, right, 'left', 'right', nearby.filter(atY), fine.filter(atY));
    let start;
    for (let i = 1; i <= xs.length; i++) {
      if (i < xs.length && owner(document.elementFromPoint((xs[i - 1] + xs[i]) / 2, y)) === el) {
        start ??= xs[i - 1];
      } else if (start != null) {
        const end = xs[i - 1], key = `${start}/${end}`;
        const rect = previous.get(key) ?? [start, ys[j - 1], end, ys[j]];
        if (!previous.has(key)) rects.push(rect);
        rect[3] = ys[j]; row.set(key, rect); start = null;
      }
    }
    previous = row;
  }
  return rects;
}

/** `views`: the runtime's id → element map; `inactive(el)`: under a route or tab its navigation root has not selected. */
export function observe(views, inactive = () => false) {
  const ids = new Map();
  for (const [k, v] of views) ids.set(v, k);
  const viewOf = (el) => { for (let e = el; e; e = e.parentElement) if (ids.has(e)) return ids.get(e); return null; };
  // Every colour as sRGB bytes and an alpha, however the page wrote it (a canvas converts).
  const ctx = Object.assign(document.createElement('canvas'), { width: 1, height: 1 }).getContext('2d', { willReadFrequently: true });
  const rgba = (css) => {
    if (!css || css === 'transparent' || css === 'none') return null;
    ctx.clearRect(0, 0, 1, 1); ctx.fillStyle = '#0000'; ctx.fillStyle = css; ctx.fillRect(0, 0, 1, 1);
    const d = ctx.getImageData(0, 0, 1, 1).data;
    return d[3] ? [d[0], d[1], d[2], Math.round(d[3] / 255 * 1000) / 1000] : null;
  };
  // What the author wrote on an element's classes: its platform colour role, and whether it set a size. The last class
  // first: a lowered node's arm class (lower.js) stands over its static class, as its rule does.
  const rules = new Map();
  const gather = (list) => { for (const r of list) { const m = r.selectorText && /\.([\w-]+)$/.exec(r.selectorText.trim()); if (m) { if (!rules.has(m[1])) rules.set(m[1], []); rules.get(m[1]).push(r.style); } if (r.cssRules) gather(r.cssRules); } };
  for (const sheet of document.styleSheets) { try { gather(sheet.cssRules); } catch { /* another origin's sheet */ } }
  const authored = (el, prop) => { for (const c of [...(el.classList ?? [])].reverse()) for (const st of rules.get(c) ?? []) { const v = st.getPropertyValue(prop); if (v) return v; } return el.style?.getPropertyValue(prop) ?? ''; };
  const role = (el, prop) => /^var\(--exact-([\w-]+)/.exec(authored(el, prop))?.[1];
  const W = innerWidth, H = innerHeight, r2 = (x) => Math.round(x * 100) / 100;
  const modal = document.querySelector('dialog:modal');
  // Folded text has line boxes but no element box. Its nearest box controls rendering; its own visibility can
  // override that box's, as an ordinary descendant's can.
  const box = el => {
    if (!el.hasAttribute('data-exact-text') || getComputedStyle(el).display !== 'contents') return el.getBoundingClientRect();
    const words = document.createRange(); words.selectNodeContents(el);
    return words.getBoundingClientRect();
  };
  const shown = (el, b, allowEmptyAxis = false) => {
    let rendered = el;
    if (getComputedStyle(el).display === 'contents') {
      if (getComputedStyle(el).visibility !== 'visible') return false;
      do { rendered = rendered.parentElement; } while (rendered && getComputedStyle(rendered).display === 'contents');
      if (!rendered || getComputedStyle(rendered).contentVisibility === 'hidden') return false;
    }
    return (allowEmptyAxis ? b.width > 0 || b.height > 0 : b.width > 0 && b.height > 0)
      && (!rendered.checkVisibility || rendered.checkVisibility({ opacityProperty: true, visibilityProperty: rendered === el }))
      && !inertAncestor(el) && !el.closest('[data-exiting]') && (!modal || modal.contains(el)) && !inactive(el);
  };
  const ownOpacity = el => { const s = getComputedStyle(el); return s.display === 'contents' ? 1 : Number(s.opacity); };
  const opacity = (el) => { let o = 1; for (let e = el; e && e !== document.documentElement; e = e.parentElement) o *= ownOpacity(e); return o; };
  // What paints that a computed colour does not say: a picture, a native widget, a gradient.
  const opaque = (e, s) => /^(img|svg|canvas|video|iframe|picture|object|embed)$/.test(e.localName) || s.backgroundImage !== 'none'
    || (/^(button|input|select|textarea|progress|meter)$/.test(e.localName) && (s.appearance ?? s.webkitAppearance ?? 'none') !== 'none');
  const paints = (e) => { const s = getComputedStyle(e); return opaque(e, s) || (s.backgroundClip !== 'text' && !!rgba(s.backgroundColor)); };
  // LLP 1116.006 D5: what a text stands on, at its ink's middle: the nearest painted fill — its own box's, a bubble's, a
  // card's, the page's — composited over what lies below where it is translucent, and whose it is (the page's: null).
  // Unknown where something the styles do not say paints there (a picture, a native widget, a box laid between that is not
  // its ancestor): the capture's pixels answer then.
  let canvasColor;
  const backing = (el, at) => {
    const cx = at.left + at.width / 2, cy = at.top + at.height / 2, chain = [];
    for (let e = el; e; e = e.parentElement) chain.push(e);
    const alpha = []; // each box's opacity times its ancestors'
    for (let i = chain.length - 1, o = 1; i >= 0; i--) alpha[i] = o *= ownOpacity(chain[i]);
    const layers = [];
    let by, accent = false, top = null;
    for (let i = 0; i < chain.length; i++) {
      const e = chain[i], s = getComputedStyle(e), canvas = e === document.documentElement || e === document.body;
      if (!canvas) { const r = e.getBoundingClientRect(); if (cx < r.left || cx > r.right || cy < r.top || cy > r.bottom) continue; }
      if (opaque(e, s)) return null;
      const c = s.backgroundClip === 'text' ? null : rgba(s.backgroundColor);
      if (!c) continue;
      c[3] *= alpha[i];
      if (!layers.length) {
        by = canvas ? null : viewOf(e);
        // The accent (`AccentColor`, which Contract writes as the accent's variable over it).
        accent = !canvas && /^var\(--exact-accent\b|^AccentColor$/.test(authored(e, 'background-color'));
      }
      layers.push(c);
      if (c[3] >= 0.999) { top = e; break; }
    }
    // Below every box, the canvas: the root's used `Canvas` colour where neither the root nor the body paints one.
    if (!top) {
      if (canvasColor === undefined) {
        const p = document.createElement('span'); p.style.backgroundColor = 'Canvas'; document.documentElement.append(p);
        canvasColor = rgba(getComputedStyle(p).backgroundColor); p.remove();
      }
      if (!canvasColor || canvasColor[3] < 0.999) return null;
      if (!layers.length) by = null;
      layers.push(canvasColor); top = document.documentElement;
    }
    // A box painted between the text and that fill, not one of its ancestors: an image under a title, a stacked sibling.
    const stack = document.elementsFromPoint(Math.min(Math.max(cx, 0), W - 1), Math.min(Math.max(cy, 0), H - 1));
    // A folded text, or a box `pointer-events: none` takes out of hit testing, has no entry in the stack: scan from its
    // nearest ancestor that has one, which may be the opaque fill. A fill inside that ancestor paints above everything
    // the stack holds below it, so nothing there lies between.
    const anchor = chain.find(e => stack.includes(e));
    if (anchor && !anchor.contains(top)) for (const e of stack.slice(stack.indexOf(anchor))) {
      if (e === top) break;
      if (!e.contains(el) && !el.contains(e) && paints(e)) return null;
    }
    // `solid`: the nearest fill under it is a view's own and opaque (it hides what lies beneath that view).
    const solid = by != null && layers[0][3] >= 0.999;
    let bg = layers.pop().slice(0, 3);
    while (layers.length) bg = mix(layers.pop(), bg);
    return { bg: bg.map((v) => Math.round(v)), by, accent, solid };
  };
  const mix = (fg, bg) => [0, 1, 2].map((i) => fg[i] * fg[3] + bg[i] * (1 - fg[3]));
  // Whether something outside `el` that paints lies over it at (x, y). Unknown, so false, where `el` takes no hit test.
  const drawnOver = (el, x, y) => {
    const stack = document.elementsFromPoint(x, y), at = stack.indexOf(el);
    return at > 0 && stack.slice(0, at).some((e) => !el.contains(e) && paints(e));
  };
  // The part of box `b` (el's) that is presented: each ancestor that clips or scrolls its overflow cuts it at its padding
  // box, up to a fixed box, which escapes them (LLP 1116.006 D5: a row its list cuts overlaps nothing past the list).
  const visible = (el, b) => {
    let l = b.left, t = b.top, r = b.right, btm = b.bottom;
    for (let e = el; e && e !== document.documentElement; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (e !== el) {
        const a = e.getBoundingClientRect(), x0 = a.left + e.clientLeft, y0 = a.top + e.clientTop;
        if (s.overflowX !== 'visible') { l = Math.max(l, x0); r = Math.min(r, x0 + e.clientWidth); }
        if (s.overflowY !== 'visible') { t = Math.max(t, y0); btm = Math.min(btm, y0 + e.clientHeight); }
      }
      if (s.position === 'fixed') break;
    }
    return { x: r2(l), y: r2(t), w: r2(Math.max(0, r - l)), h: r2(Math.max(0, btm - t)) };
  };
  // Whether a box holds what a person reads or uses: a control, or a text whose words show. A bare text is
  // `display: contents`, with no box of its own: its words' box is its.
  const holds = (el) => [...el.querySelectorAll(`[data-exact-text], ${CONTROL}`)].some(e => shown(e, box(e)));
  const fixed = (el, upTo) => { for (let e = el; e && e !== upTo; e = e.parentElement) if (/^(fixed|sticky)$/.test(getComputedStyle(e).position)) return true; return false; };
  const scrolls = (e) => /^(auto|scroll)$/.test(getComputedStyle(e).overflowY) && e.scrollHeight > e.clientHeight + 1;
  // What keeps a box's sideways overflow its own: a box that clips on x, or one that scrolls sideways on purpose (an authored
  // `overflow-x`, or the shorthand's first value). A vertical scroller's x is `auto` only because CSS computes `visible`
  // beside `auto` so (CSS Overflow 3 §3.1), and a native host neither scrolls nor clips there as a person would scroll to it:
  // UIKit's vertical scroll view cuts it at the screen's edge (LLP 1116.020 D4; round 18's focus-timer and trivia-quiz).
  const holdsX = (e, s = getComputedStyle(e)) => /^(hidden|clip)$/.test(s.overflowX)
    || (/^(auto|scroll)$/.test(s.overflowX) && (/^(auto|scroll)$/.test(authored(e, 'overflow-x').trim()) || /^(auto|scroll)(\s|$)/.test(authored(e, 'overflow').trim())));
  // A box whose written percentage width fits its parent, and whose own padding and border (content-box sizing) push it past
  // the parent's content edge (LLP 1116.009 D5; focus-timer and recipe-finder, round 11: `width="100%"` with padding clipped
  // the phone's right side while `check` said nothing, the scroller around it holding the overflow). The cause is judged, not
  // where it lands: only a written percentage, only content-box, only a parent that takes the percentage of its own content
  // (a block, or a column: a row's items shrink to fit, a grid's resolve against a cell) and does not scroll sideways on purpose
  // (an authored `overflow-x`). A wider percentage, a fixed width, a bar's or sheet's own chrome (border-box) are none of it.
  const widerThanParent = (el, cs) => {
    const parent = el.parentElement;
    if (!parent || parent.id === 'exact-root' || cs.boxSizing !== 'content-box' || /^(absolute|fixed)$/.test(cs.position) || cs.transform !== 'none') return null;
    const written = authored(el, 'width').trim();
    if (!/^\d+(\.\d+)?%$/.test(written)) return null;
    const ps = getComputedStyle(parent);
    if (!(/^(block|flow-root|list-item)$/.test(ps.display) || (/^(inline-)?flex$/.test(ps.display) && ps.flexDirection.startsWith('column')))) return null;
    if (/^(auto|scroll)$/.test(authored(parent, 'overflow-x').trim()) || /^(auto|scroll)(\s|$)/.test(authored(parent, 'overflow').trim())) return null;
    const content = parseFloat(cs.width), pad = ['Left', 'Right'].reduce((n, side) => n + parseFloat(cs[`padding${side}`]) + parseFloat(cs[`border${side}Width`]), 0);
    const room = parent.clientWidth - parseFloat(ps.paddingLeft) - parseFloat(ps.paddingRight), past = content + pad - room;
    return pad > 0.5 && past > 1.5 && content <= room + 1.5 ? { by: viewOf(parent), dx: r2(past), pad: r2(pad), width: written } : null;
  };
  const fixedHeightBetween = (el, sheet) => {
    for (let e = el.parentElement; e && e !== sheet; e = e.parentElement) {
      if (/^[\d.]+(?:px|rem|em)$/.test(authored(e, 'height'))) return true;
    }
    return false;
  };
  // A zero-length port still has a position. It may lie on its parent's edge, provided the parent itself is visible.
  const observeScroll = (el, cs, b, item) => {
    const list = el.$list;
    if (list && (!list.index.len || list.portUnknown)) return;
    const scrollable = value => value === 'auto' || value === 'scroll';
    const horizontal = scrollable(cs.overflowX) && scrollable(authored(el, 'overflow-x'));
    for (const axis of ['y', 'x']) {
      const y = axis === 'y', size = y ? 'height' : 'width', client = y ? el.clientHeight : el.clientWidth;
      const content = y ? el.scrollHeight : el.scrollWidth;
      if (list ? list.axis !== axis : !(y ? scrollable(cs.overflowY) : horizontal)) continue;
      if (client > 44 || content <= client) continue;
      // CSSOM rounds client sizes. Restore the fractional part of the padding box without counting a scrollbar in the port.
      const sides = y ? ['Top', 'Bottom'] : ['Left', 'Right'];
      const paddingBox = sides.reduce((n, side) => n + (cs.boxSizing === 'border-box' ? -parseFloat(cs['border' + side + 'Width']) : parseFloat(cs['padding' + side])), parseFloat(cs[size]));
      const port = Math.max(0, r2(client + paddingBox - Math.round(paddingBox)));
      const implicitY = y && horizontal && !scrollable(authored(el, 'overflow-y'));
      if (port >= 44 || (implicitY && content - port < 44)) continue;
      const clip = visible(el, { left: 0, top: 0, right: W, bottom: H });
      const overlaps = (lo, hi, start, length, allowZero) => length > 0 && (allowZero && hi === lo
        ? lo >= start && lo <= start + length : Math.min(hi, start + length) > Math.max(lo, start));
      if (!overlaps(b.left, b.right, clip.x, clip.w, !y) || !overlaps(b.top, b.bottom, clip.y, clip.h, y)) continue;
      const sheet = el.closest('[navigationPresentation="modal"], [data-exact-in-sheet]');
      if (y && sheet && W < 900 && H >= 480 && (sheet.getAnimations().some(a => a.playState === 'running') ||
          sheet.$sheetCanGrow && sheet.getBoundingClientRect().height < H - 12 && !fixedHeightBetween(el, sheet))) continue;
      // Only a measured row supports a claim about a positive port. A zero port hides even an unmounted row.
      let first;
      if (list && !port) first = 44;
      else {
        let children = viewChildren(el);
        if (!el.hasAttribute('data-exact-list') && children.length === 1 && children[0].childElementCount > 1) children = viewChildren(children[0]);
        for (const child of children) {
          if (list && !child.hasAttribute('data-listitemkey')) continue;
          const bounds = box(child);
          if (shown(child, bounds)) { first = bounds[size]; break; }
        }
      }
      if (!(port < Math.min(44, first))) continue;
      const parent = getComputedStyle(el.parentElement);
      const alongFlex = parent.display.includes('flex') && parent.flexDirection.startsWith('column') === y;
      const props = [size, 'max-' + size];
      if (alongFlex && cs.flexGrow === '0' && cs.flexShrink === '0') props.push('flex-basis');
      const sized = props.some(prop => {
        const said = authored(el, prop);
        if (!said) return false;
        const value = el.computedStyleMap?.().get(prop)?.toString() ?? said;
        if (['auto', 'none', 'initial', 'unset'].includes(value)) return false;
        return !value.endsWith('px') || parseFloat(value) <= parseFloat(cs[size]) + 0.5;
      });
      if (!sized) items.push({ ...item, squeezed: { axis, port, content } });
    }
  };
  const page = document.scrollingElement;
  // The box that covers `el` where `hit` is: the subtree beside `el` that `hit` is in. A subtree drawn around `el`, as a web
  // sheet's header is (one grid over the whole sheet, its title above the content and its buttons at its foot), covers only
  // from the top of its first child that starts at or above `my` and ends below `el`'s top, down to its own bottom: a row
  // under the buttons is cleared by scrolling up past them, not past the whole sheet.
  const coverBox = (el, hit, my) => {
    let c = hit; while (c.parentElement && !c.parentElement.contains(el)) c = c.parentElement;
    const r = c.getBoundingClientRect(), b = el.getBoundingClientRect();
    if (r.top > b.top || r.bottom < b.bottom) return r;
    const band = [...c.children].map((k) => k.getBoundingClientRect()).filter((k) => k.height > 0 && k.bottom > b.top && k.top <= my);
    return band.length ? { top: Math.min(...band.map((k) => k.top)), bottom: r.bottom } : r;
  };
  // Whether a scroller holding `el` could move it out from under `hit` (or, with none, into view): one `hit` does not
  // move with. The cover is `coverBox` (the bar, the header, the sheet), and the middle `my`
  // clears it by landing above it (the content moves up: room toward the end of at least `my - top`) or below it (the
  // content moves down: room back of at least `bottom - my`), within what the scroller shows of the screen: a bar along
  // its bottom edge is cleared only upward, a header along its top only downward, and a last row that scrolls only a few
  // points does not clear a bar taller than that (LLP 1116.012 D3; contacts, round 14: a fold row under an in-flow tab bar,
  // with the list's whole range to scroll it up by). With no `hit` (`my` unused), `above` says the element lies above
  // the screen: scrolling back reveals.
  const reach = (el, hit, above, my) => {
    const cover = hit ? coverBox(el, hit, my) : null;
    for (let s = el.parentElement; s; s = s.parentElement) {
      const scroller = s === page || s === document.body ? page : scrolls(s) ? s : null;
      if (!scroller || (hit && ((scroller !== page && scroller.contains(hit) && !fixed(hit, scroller)) || (scroller === page && !fixed(hit))))) continue;
      const back = scroller.scrollTop, end = scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop;
      if (!cover) { if (above ? back > 0.5 : end > 0.5) return true; continue; }
      const shown = scroller === page ? { top: 0, bottom: H } : scroller.getBoundingClientRect(), top = Math.max(0, shown.top), bottom = Math.min(H, shown.bottom);
      if ((cover.top > top + 1 && end > my - cover.top + 0.5) || (cover.bottom < bottom - 1 && back > cover.bottom - my + 0.5)) return true;
    }
    return false;
  };
  // Whether a box that scrolls sideways holds `el` clipped at rest, its middle `mx` past the box's edge, with room to scroll that
  // middle in: a swipe row's action beside a pane in a wide window, where what a finger meets at the action's place is the pane.
  const slides = (el, mx) => {
    for (let s = el.parentElement; s; s = s.parentElement) {
      if (!/^(auto|scroll)$/.test(getComputedStyle(s).overflowX) || s.scrollWidth <= s.clientWidth + 1) continue;
      const left = s.getBoundingClientRect().left + s.clientLeft, right = left + s.clientWidth;
      if (mx >= right) return s.scrollWidth - s.clientWidth - s.scrollLeft > mx - right + 0.5;
      if (mx < left) return s.scrollLeft > left - mx + 0.5;
    }
    return false;
  };
  const items = [];
  const out = new Set();
  // Outermost first, so a view past the viewport's side is named and not each of its children.
  const all = [...document.querySelectorAll('#exact-root *')];
  for (const el of all) {
    if (!ids.has(el) || el.namespaceURI !== 'http://www.w3.org/1999/xhtml') continue;
    const b = box(el), scroller = el.dataset.scroll === 'true';
    if (!shown(el, b, scroller)) continue;
    const cs = getComputedStyle(el), item = { id: ids.get(el), x: r2(b.x), y: r2(b.y), w: r2(b.width), h: r2(b.height) };
    if (scroller) {
      observeScroll(el, cs, b, item);
      if (!b.width || !b.height) continue;
    }
    // A run of a paragraph that wraps is a box per line (CSS Inline 3 §2): its box bounds them all, and takes in the ends of
    // lines it shares with the runs before and after it. The driver judges such a run by its lines.
    const lines = cs.display === 'inline' ? [...el.getClientRects()].filter((r) => r.width > 0 && r.height > 0) : [];
    if (lines.length > 1) item.lines = lines.map((r) => ({ x: r2(r.left), y: r2(r.top), w: r2(r.width), h: r2(r.height) }));
    // A padded percentage width wider than its parent is a finding wherever the box lies, below the fold too.
    const padded = widerThanParent(el, cs);
    if (padded) item.padded = padded;
    // Off the screen: only a control out of reach (past its top or bottom, and nothing scrolls to it) is anything.
    if (!(b.right > 0 && b.bottom > 0 && b.left < W && b.top < H)) {
      if (b.right > 0 && b.left < W && el.matches(CONTROL) && !el.parentElement?.closest('button, a[href]') && !reach(el, null, b.bottom <= 0)) {
        items.push({ ...item, control: el.localName, sized: 'platform', cover: { kind: 'offscreen', name: b.bottom <= 0 ? 'the top of the screen' : 'the bottom of the screen' } });
      } else if (padded) items.push(item);
      continue;
    }
    const o = opacity(el);
    // Past the viewport's sides, nothing scrolling it back sideways nor clipping it: the outermost such view only. Where it
    // stands with a vertical scroller back at its sideways start, as a native host never moves it (a reveal moves it in Chrome).
    let held = false, sx = 0;
    for (let e = el.parentElement; e && e.id !== 'exact-root'; e = e.parentElement) {
      if (out.has(e) || holdsX(e)) { held = true; break; }
      sx += e.scrollLeft;
    }
    const past = Math.max(b.right + sx - W, -(b.left + sx));
    if (!held && past > 1 && !fixed(el)) { item.outX = r2(past); out.add(el); }
    // A flex or grid item past its container's sides, within the viewport (LLP 1116.007 D3: tip-splitter's fifth radio at 320 px,
    // an option wider than its row, poking out of its card): the container neither scrolling nor clipping it, nor any ancestor
    // it lies beyond, nor transformed or placed out of flow. Past the viewport it is the finding above.
    const parent = el.parentElement, ps = parent && parent.id !== 'exact-root' ? getComputedStyle(parent) : null;
    if (!item.outX && ps && /^(inline-)?(flex|grid)$/.test(ps.display) && !holdsX(parent, ps) && !/^(absolute|fixed)$/.test(cs.position) && cs.transform === 'none') {
      const pb = parent.getBoundingClientRect(), left = pb.left + parent.clientLeft, spill = Math.max(b.right - (left + parent.clientWidth), left - b.left);
      let beyond = false;
      for (let e = parent.parentElement; e && e.id !== 'exact-root' && !beyond; e = e.parentElement) {
        if (out.has(e)) beyond = true;
        else if (holdsX(e)) { const a = e.getBoundingClientRect(), l = a.left + e.clientLeft; beyond = b.right > l + e.clientWidth + 0.5 || b.left < l - 0.5; }
      }
      if (spill > 1.5 && !beyond && !fixed(el)) { item.spill = { by: viewOf(parent), dx: r2(spill) }; out.add(el); }
    }
    const text = el.hasAttribute('data-exact-text') ? (el.textContent ?? '').replace(/\s+/g, ' ').trim() : '';
    if (text) {
      item.text = text.length > 40 ? text.slice(0, 39) + '…' : text;
      const fg = rgba(cs.webkitTextFillColor && cs.webkitTextFillColor !== cs.color ? cs.webkitTextFillColor : cs.color);
      if (fg) { fg[3] = r2(fg[3] * o); item.fg = fg; }
      item.size = parseFloat(cs.fontSize); item.weight = parseInt(cs.fontWeight, 10) || 400;
      const fgRole = role(el, 'color');
      if (fgRole) item.fgRole = fgRole;
      // A control's own label on the control's own fill (D3's ink rule): the nearest control within three levels. Ink no
      // one authored on a host-drawn control (a tab bar's item) is the platform's, as UIKit's own labels are.
      for (let e = el.parentElement, k = 0; e && k < 3; e = e.parentElement, k++) if (e.matches(CONTROL)) {
        item.of = viewOf(e);
        // A disabled control's label is an inactive component's text, which WCAG 1.4.3 does not judge.
        if (e.matches(':disabled') || e.getAttribute('aria-disabled') === 'true') item.inactive = true;
        if (rgba(getComputedStyle(e).backgroundColor)) item.label = true;
        let inked = false;
        for (let a = el; a && !inked; a = a === e ? null : a.parentElement) inked = !!authored(a, 'color');
        if (!inked && e.hasAttribute('data-native')) item.platformInk = true;
        // The tinted style's label in the button's accent, over a pale fill of it, as iOS's observer says (`tinted`).
        if (!inked && e.getAttribute('data-button-style') === 'tinted') item.tinted = true;
        break;
      }
      const ellipsis = cs.textOverflow === 'ellipsis' && cs.overflowX !== 'visible';
      if (ellipsis || cs.webkitLineClamp && cs.webkitLineClamp !== 'none') item.clamp = true;
      if (ellipsis) item.ellipsis = true;
      let ink = b;
      if (cs.overflowX !== 'visible' || cs.overflowY !== 'visible') {
        const dx = el.scrollWidth - el.clientWidth, dy = el.scrollHeight - el.clientHeight;
        if (dx > 1 || dy > 1) { item.over = [Math.max(0, dx), Math.max(0, dy)]; item.clips = true; }
      } else {
        const range = document.createRange(); range.selectNodeContents(el);
        const rr = range.getBoundingClientRect();
        if (rr.width && rr.height) {
          ink = rr;
          const dx = Math.max(rr.right - b.right, b.left - rr.left), dy = Math.max(rr.bottom - b.bottom, b.top - rr.top);
          if (dx > 1 || dy > 1) item.over = [r2(Math.max(0, dx)), r2(Math.max(0, dy))];
          // Where its lines are drawn, where that is not its box: the driver judges two texts running into each other on it.
          if (Math.abs(rr.left - b.left) > 0.5 || Math.abs(rr.right - b.right) > 0.5 || Math.abs(rr.top - b.top) > 0.5 || Math.abs(rr.bottom - b.bottom) > 0.5) {
            item.ink = { x: r2(rr.left), y: r2(rr.top), w: r2(rr.width), h: r2(rr.height) };
          }
        }
      }
      const under = backing(el, ink);
      if (under) { item.bg = under.bg; item.on = under.by === item.id ? 'own' : under.by ?? 'page'; if (under.accent) item.onAccentFill = true; if (under.solid) item.solid = true; }
      else {
        // The driver then reads the ground at the corners of the presented box (agent-check.mjs `backdrop`, in its order);
        // a corner another view is drawn over shows that view (a card laid over this text's card), not the ground.
        const v = visible(el, b), vh = globalThis.visualViewport?.height ?? H, h = vh < H ? Math.min(v.h, Math.max(0, vh - v.y)) : v.h;
        const i = Math.min(1.5, v.w / 4, h / 4);
        const over = [[v.x + i, v.y + i], [v.x + v.w - i, v.y + i], [v.x + i, v.y + h - i], [v.x + v.w - i, v.y + h - i]]
          .flatMap(([x, y], k) => (drawnOver(el, x, y) ? [k] : []));
        if (over.length) item.drawnOver = over;
      }
      // An ancestor that clips (never one that scrolls: a person scrolls to it) cutting part of the text away.
      // One that hides it all is a collapsed disclosure, on purpose. What lies past a scroller's edge is reached by
      // scrolling, so an ancestor above that scroller is judged on the part of the text inside the scroller's box:
      // a row straddling a scroll view's end is not cut by the screen that holds the scroll view.
      let seen = { left: ink.left, top: ink.top, right: ink.right, bottom: ink.bottom };
      for (let e = el.parentElement; e && e.id !== 'exact-root'; e = e.parentElement) {
        const s = getComputedStyle(e), hx = /^(hidden|clip)$/.test(s.overflowX), hy = /^(hidden|clip)$/.test(s.overflowY);
        const a = e.getBoundingClientRect(), left = a.left + e.clientLeft, top = a.top + e.clientTop, right = left + e.clientWidth, bottom = top + e.clientHeight;
        const sx = /^(auto|scroll)$/.test(s.overflowX), sy = /^(auto|scroll)$/.test(s.overflowY);
        if (sx || sy) {
          seen = { left: sx ? Math.max(seen.left, left) : seen.left, top: sy ? Math.max(seen.top, top) : seen.top, right: sx ? Math.min(seen.right, right) : seen.right, bottom: sy ? Math.min(seen.bottom, bottom) : seen.bottom };
          if (seen.right <= seen.left || seen.bottom <= seen.top) break;
        }
        if (!hx && !hy) continue;
        if (seen.right <= left || seen.left >= right || seen.bottom <= top || seen.top >= bottom) break;
        const dx = hx ? Math.max(0, left - seen.left) + Math.max(0, seen.right - right) : 0, dy = hy ? Math.max(0, top - seen.top) + Math.max(0, seen.bottom - bottom) : 0;
        if (dx >= 2 || dy >= 2) { item.cut = { by: viewOf(e), dx: r2(dx), dy: r2(dy) }; break; }
      }
    }
    if (el.matches(CONTROL) && !el.parentElement?.closest('button, a[href]')) {
      item.control = el.localName === 'a' ? 'link' : el.localName === 'input' ? (el.type || 'text') : el.getAttribute('role') ?? (el.localName === 'button' || el.localName === 'select' || el.localName === 'textarea' ? el.localName : 'press');
      const own = SIZE.some((p) => authored(el, p));
      item.sized = !own && (el.hasAttribute('data-native') || el.matches(AGENT_SIZED)) ? 'platform' : 'author';
      // A target in a sentence, its size the line's (WCAG 2.5.8's inline exception): a link or a pressable run in a
      // paragraph that says more than it does.
      if (cs.display === 'inline') {
        let p = el.parentElement;
        while (p && getComputedStyle(p).display === 'inline') p = p.parentElement;
        if (p && p.textContent.trim() !== el.textContent.trim()) item.inline = true;
      }
      if (el.matches(':disabled') || el.getAttribute('aria-disabled') === 'true') item.disabled = true;
    }
    // What lets taps through (`pointer-events: none`) is no target, and a hit test cannot say what lies over it. In an
    // announcement laid over the page (a toast: a live region out of flow), it lies over what is beneath by design.
    const through = cs.pointerEvents === 'none';
    if (through) {
      let live = false, out = false;
      for (let e = el; e && e.id !== 'exact-root' && !live; e = e.parentElement) {
        live = /^(polite|assertive)$/.test(e.getAttribute('aria-live') ?? '') || /^(status|alert)$/.test(e.getAttribute('role') ?? '');
        out ||= /^(absolute|fixed)$/.test(getComputedStyle(e).position);
      }
      if (live && out) item.overlay = true;
    }
    // What a finger at its middle reaches; covered only where no scroller could move it clear. Past the screen's top
    // or bottom where nothing scrolls it in (a column taller than the screen) is out of reach: a control's finding.
    if ((item.control || text) && !through) {
      // A wrapped run's middle may be another run's: a finger aims at its longest line.
      const at = lines.length > 1 ? lines.reduce((p, q) => (q.width * q.height > p.width * p.height ? q : p)) : b;
      const my = at.top + at.height / 2;
      if (my >= H || my < 0) { if (item.control && !reach(el, null, my < 0)) item.cover = { kind: 'offscreen', name: my < 0 ? 'the top of the screen' : 'the bottom of the screen' }; }
      else {
        const cx = Math.min(Math.max(at.left + at.width / 2, 1), W - 1), hit = document.elementFromPoint(cx, my);
        if (hit && hit !== el && !el.contains(hit) && !hit.contains(el)) {
          // What is drawn over its middle at rest (the subtree beside it the hit is in), scrolled clear or not.
          let c = hit; while (c.parentElement && !c.parentElement.contains(el)) c = c.parentElement;
          const cb = c.getBoundingClientRect();
          item.beneath = { by: viewOf(c), x: r2(cb.left), y: r2(cb.top), w: r2(cb.width), h: r2(cb.height) };
          if (!reach(el, hit, false, my) && !slides(el, at.left + at.width / 2)) item.cover = { by: viewOf(hit), name: hit.localName, kind: modal && !modal.contains(hit) ? 'sheet' : 'node' };
        }
      }
    }
    // Past the screen's bottom where nothing scrolls to it: geometry, whether or not it takes taps.
    if (text && !item.cut && b.bottom - H >= 2 && b.top < H && !reach(el, null, false)) item.cut = { edge: 'bottom', dx: 0, dy: r2(b.bottom - H) };
    // A card: a filled box, not itself text or a control, that holds a text or a control, its fill what sets that group
    // apart from the page. None is a box with square corners that reaches a side of the viewport (the page itself, or a
    // band across it); a list, whose ground is the page under its rows (a grouped list's grey, its sections the cards);
    // or a shape that holds no text or control (an illustration's, a map's), whether it shows or not
    // (AgentCheckIOS.swift's is iOS's). A symbol's fill is its ink through a mask, and a picture is not a fill.
    // `faded`: an ancestor's opacity composites it with what lies behind that ancestor, which the capture cannot separate.
    const picture = /^(img|svg|canvas|video|iframe)$/.test(el.localName) || el.hasAttribute('data-symbol-source') || cs.backgroundImage !== 'none' || (cs.maskImage ?? cs.webkitMaskImage ?? 'none') !== 'none' || (cs.webkitMaskImage ?? 'none') !== 'none';
    const fill = !text && !item.control && !picture && !el.hasAttribute('data-exact-list') ? rgba(cs.backgroundColor) : null;
    if (fill && b.width >= 16 && b.height >= 16) {
      const radius = parseFloat(cs.borderTopLeftRadius) || 0, inset = b.left >= 2 && b.right <= W - 2;
      if ((radius > 0 || inset) && holds(el)) {
        fill[3] = r2(fill[3] * o);
        item.fill = fill; item.radius = radius;
        if (o < Number(cs.opacity) * 0.999) item.faded = true;
        const fillRole = role(el, 'background-color');
        if (fillRole) item.fillRole = fillRole;
        const border = ['Top', 'Right', 'Bottom', 'Left'].some((side) => parseFloat(cs[`border${side}Width`]) > 0 && rgba(cs[`border${side}Color`]));
        if (border || cs.boxShadow !== 'none' || (cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) > 0)) item.edge = true;
      }
    }
    if (item.text || item.control || item.fill) {
      const v = visible(el, b);
      if (Math.abs(v.x - item.x) > 0.5 || Math.abs(v.y - item.y) > 0.5 || Math.abs(v.w - item.w) > 0.5 || Math.abs(v.h - item.h) > 0.5) item.vis = v;
    }
    if (item.text || item.control || item.fill || item.outX || item.spill || item.padded) items.push(item);
  }
  // The browser lays the slop pseudo-element out from its containing block's padding edge.
  // A box with its used insets gives its bounds through transforms and scroll offsets too.
  const targets = new Map(items.filter(it => it.control).map(it => [views.get(it.id), it]));
  const owner = hit => {
    for (let e = hit; e; e = e.parentElement) {
      if (targets.has(e)) return e;
      if (e.matches(CONTROL)) return null;
    }
    return null;
  };
  if (targets.size) {
    const edges = [], curved = [], bounds = new Map(), ports = new Map(), transformed = new Set(), tilted = new Set();
    const include = (target, r) => {
      if (!target) return;
      const a = bounds.get(target);
      bounds.set(target, !a ? r : { left: Math.min(a.left, r.left), right: Math.max(a.right, r.right),
        top: Math.min(a.top, r.top), bottom: Math.max(a.bottom, r.bottom) });
    };
    // A pointer-transparent decoration contributes no boundaries unless a descendant takes input.
    const participating = new Set();
    for (let i = all.length - 1; i >= 0; i--) {
      const e = all[i];
      if (getComputedStyle(e).pointerEvents !== 'none' || participating.has(e)) {
        participating.add(e); participating.add(e.parentElement);
      }
    }
    for (const e of all) {
      if (!participating.has(e)) continue;
      const b = e.getBoundingClientRect(), cs = getComputedStyle(e);
      if (transformed.has(e.parentElement) || cs.transform !== 'none' || cs.rotate !== 'none' ||
          cs.scale !== 'none' || cs.perspective !== 'none' || Number(cs.zoom) !== 1) transformed.add(e);
      if (tilted.has(e.parentElement) || cs.rotate !== 'none' || cs.perspective !== 'none' || /(?:^| )-/.test(cs.scale) ||
          /^matrix\(-|^matrix\([^,]+, 0, 0, -/.test(cs.transform) ||
          cs.transform !== 'none' && !/^matrix\([^,]+, 0, 0, [^,]+, [^,]+, [^)]+\)$/.test(cs.transform)) tilted.add(e);
      const target = owner(e);
      for (const line of e.getClientRects()) { edges.push(line); include(target, line); }
      let squareCorners = false;
      if (e.getAttribute('style')?.includes('--exact-hit-slop-')) for (const pseudo of ['::before', '::after']) {
        const p = getComputedStyle(e, pseudo);
        if (p.content === 'none' || p.position !== 'absolute') continue;
        let slop;
        if (!transformed.has(e) && cs.position !== 'static' && !e.scrollTop && !e.scrollLeft) {
          slop = { left: b.left + parseFloat(cs.borderLeftWidth) + parseFloat(p.left),
            right: b.right - parseFloat(cs.borderRightWidth) - parseFloat(p.right),
            top: b.top + parseFloat(cs.borderTopWidth) + parseFloat(p.top),
            bottom: b.bottom - parseFloat(cs.borderBottomWidth) - parseFloat(p.bottom) };
          if (!Object.values(slop).every(Number.isFinite)) continue;
        } else {
          const probe = document.createElement('span');
          probe.style.cssText = `all:initial!important;position:absolute!important;inset:${p.inset}!important;visibility:hidden!important;pointer-events:none!important`;
          e.append(probe); slop = probe.getBoundingClientRect(); probe.remove();
        }
        edges.push(slop); include(target, slop);
        if (tilted.has(e)) curved.push(slop);
        // With visible overflow the rectangular pseudo-element itself takes the rounded corners.
        squareCorners ||= p.pointerEvents !== 'none' && cs.overflowX === 'visible' && cs.overflowY === 'visible' &&
          slop.left <= b.left && slop.right >= b.right && slop.top <= b.top && slop.bottom >= b.bottom;
      }
      const sx = transformed.has(e) && e.offsetWidth ? b.width / e.offsetWidth : 1;
      const sy = transformed.has(e) && e.offsetHeight ? b.height / e.offsetHeight : 1;
      const inset = /^inset\(([-+.%\dpx ]+)\)$/.exec(cs.clipPath);
      if (tilted.has(e) || cs.clipPath !== 'none' && !inset) curved.push(b);
      else if (inset) {
        const [t, r = t, d = t, l = r] = inset[1].trim().split(/\s+/);
        const offset = (v, extent, scale) => parseFloat(v) * (v.endsWith('%') ? extent / 100 : scale);
        edges.push({ left: b.left + offset(l, b.width, sx), right: b.right - offset(r, b.width, sx),
          top: b.top + offset(t, b.height, sy), bottom: b.bottom - offset(d, b.height, sy) });
      }
      if (!squareCorners && !tilted.has(e)) for (const [corner, left, top] of [['TopLeft', true, true], ['TopRight', false, true],
        ['BottomLeft', true, false], ['BottomRight', false, false]]) {
        const [x, y = x] = cs[`border${corner}Radius`].split(' ');
        const length = (value, size) => Math.min(size, parseFloat(value) * (value.endsWith('%') ? size / 100 : 1));
        const rx = length(x, b.width), ry = length(y, b.height);
        if (rx && ry) curved.push({ left: left ? b.left : b.right - rx, right: left ? b.left + rx : b.right,
          top: top ? b.top : b.bottom - ry, bottom: top ? b.top + ry : b.bottom });
      }
      if (cs.overflowX !== 'visible' || cs.overflowY !== 'visible') {
        const port = { left: b.left + sx * e.clientLeft, right: b.left + sx * (e.clientLeft + e.clientWidth),
          top: b.top + sy * e.clientTop, bottom: b.top + sy * (e.clientTop + e.clientHeight), sx, sy,
        };
        edges.push({ left: cs.overflowX === 'visible' ? 0 : port.left, right: cs.overflowX === 'visible' ? W : port.right,
          top: cs.overflowY === 'visible' ? 0 : port.top, bottom: cs.overflowY === 'visible' ? H : port.bottom });
        ports.set(e, port);
      }
    }
    const min = matchMedia('(any-pointer: coarse)').matches ? 44 : 24;
    const measured = new Map(), squares = new Set();
    const intersects = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
    for (const [el, it] of targets) {
      const b = bounds.get(el) ?? el.getBoundingClientRect();
      const left = Math.max(b.left, 0), right = Math.min(b.right, W), top = Math.max(b.top, 0), bottom = Math.min(b.bottom, H);
      if (right - left >= 2 * min && bottom - top >= 2 * min && curved.some(c => intersects(b, c))) {
        for (const [fx, fy] of [[0.5, 0.5], [0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]) {
          const x = left + fx * (right - left - min), y = top + fy * (bottom - top - min);
          if (owner(document.elementFromPoint(x + min / 2, y + min / 2)) !== el) continue;
          const square = { left: x, top: y, right: x + min, bottom: y + min };
          const witness = targetReach(el, square, edges, curved, owner);
          if (witness.length === 1 && witness[0][0] === x && witness[0][1] === y && witness[0][2] === x + min && witness[0][3] === y + min) {
            squares.add(el); measured.set(el, witness); break;
          }
        }
      }
      if (!measured.has(el)) measured.set(el, targetReach(el, b, edges, curved, owner));
      // A scrollport can cut an otherwise reachable row. Only extend a single hit rectangle
      // at the port edge, up to the drawn box. Slop and overflowing children need a measured hit.
      const hits = measured.get(el);
      if (hits.length !== 1) continue;
      const seen = hits[0], box = el.getBoundingClientRect();
      let dx = 0, dy = 0, reveal = { left: box.left, top: box.top, right: box.right, bottom: box.bottom };
      for (let e = el; e; e = e.parentElement) {
        const cs = getComputedStyle(e);
        const port = e === page ? { left: 0, top: 0, right: W, bottom: H, sx: 1, sy: 1 } : ports.get(e);
        if (!port) { if (cs.position === 'fixed') break; continue; }
        if (/^(hidden|clip)$/.test(cs.overflowX)) { reveal.left = Math.max(reveal.left, port.left); reveal.right = Math.min(reveal.right, port.right); }
        if (/^(hidden|clip)$/.test(cs.overflowY)) { reveal.top = Math.max(reveal.top, port.top); reveal.bottom = Math.min(reveal.bottom, port.bottom); }
        const gain = (l, r, lo, hi, back, end) => Math.max(0,
          Math.min(r - l, hi - lo, r + back - lo, hi - (l - end)) - Math.max(0, Math.min(r, hi) - Math.max(l, lo)));
        if ((e === page || /^(auto|scroll)$/.test(cs.overflowX)) && (Math.abs(seen[0] - port.left) < 0.01 || Math.abs(seen[2] - port.right) < 0.01)) dx = Math.max(dx,
          gain(reveal.left, reveal.right, port.left, port.right, port.sx * e.scrollLeft, port.sx * (e.scrollWidth - e.clientWidth - e.scrollLeft)));
        if ((e === page || /^(auto|scroll)$/.test(cs.overflowY)) && (Math.abs(seen[1] - port.top) < 0.01 || Math.abs(seen[3] - port.bottom) < 0.01)) dy = Math.max(dy,
          gain(reveal.top, reveal.bottom, port.top, port.bottom, port.sy * e.scrollTop, port.sy * (e.scrollHeight - e.clientHeight - e.scrollTop)));
        if (cs.position === 'fixed') break;
      }
      if (dx || dy) it.scrollGain = [r2(dx), r2(dy)];
    }
    // A proven square settles size. Outside it, only the spacing circles of potentially
    // undersized neighbors need the large control's hit region.
    for (const [peer, rs] of measured) {
      if (!squares.size || squares.has(peer) || !rs.length || rs.some(([l, t, r, b]) => Math.min(r - l, b - t) >= min)) continue;
      const x = (Math.min(...rs.map(r => r[0])) + Math.max(...rs.map(r => r[2]))) / 2;
      const y = (Math.min(...rs.map(r => r[1])) + Math.max(...rs.map(r => r[3]))) / 2;
      const circleBox = { left: x - min / 2, right: x + min / 2, top: y - min / 2, bottom: y + min / 2 };
      for (const el of squares) if (intersects(bounds.get(el), circleBox)) {
        measured.get(el).push(...targetReach(el, circleBox, edges, curved, owner));
      }
    }
    for (const [el, it] of targets) {
      const reachRects = measured.get(el).map(rect => rect.map(r2));
      const box = [it.x, it.y, it.x + it.w, it.y + it.h];
      if (reachRects.length !== 1 || reachRects[0].some((v, i) => Math.abs(v - box[i]) > 0.01)) it.reach = reachRects;
    }
  }
  // LLP 1116.004 D1.3: a phone's layout at a desktop width, as advice (never a finding): a root tab bar stretched along
  // the window's bottom. The advice for a list beside its detail is LLP 1127 D1's, held until its qualification (§3).
  const advice = [];
  if (W >= 900) for (const nav of document.querySelectorAll('#exact-root [navigationBack]')) {
    const bar = [...nav.querySelectorAll('[role="tablist"]')].find((l) => l.parentElement?.closest('[navigationKey]') === nav && l.querySelector(':scope > [role="tab"][aria-controls]'));
    const bb = bar?.getBoundingClientRect();
    if (bb && !nav.hasAttribute('data-exact-sidebar') && bb.width >= W * 0.75 && bb.bottom >= H - 2 && bb.height * 4 < bb.width) advice.push({ kind: 'tabbar', id: viewOf(bar), w: r2(bb.width) });
  }
  // The scheme the page draws in: a light-dark() probe in the root, whatever the system says.
  const root = document.getElementById('exact-root'), probe = document.createElement('span');
  probe.style.color = 'light-dark(rgb(0, 0, 1), rgb(0, 0, 2))'; root.append(probe);
  const used = getComputedStyle(probe).color.includes('2)') ? 'dark' : 'light';
  // What the page draws each of Exact's colour roles in, light and dark (host/web/look.mjs names them): the driver knows an ink
  // or a ground by it.
  const names = getComputedStyle(document.documentElement).getPropertyValue('--exact-apple-roles').trim(), palette = {};
  if (names) for (const scheme of ['light', 'dark']) {
    probe.style.colorScheme = scheme; palette[scheme] = {};
    for (const name of JSON.parse(names).split(' ')) { probe.style.color = `var(--exact-${name})`; palette[scheme][name] = rgba(getComputedStyle(probe).color); }
  }
  probe.remove();
  const coarse = matchMedia('(any-pointer: coarse)').matches, overlap = Math.max(0, H - (globalThis.visualViewport?.height ?? H));
  return { viewport: { w: W, h: H, scrollW: page.scrollWidth }, scheme: used, ...(names ? { palette } : {}), target: coarse ? 44 : 24, ...(overlap ? { keyboard: r2(H - overlap) } : {}), items, ...(advice.length ? { advice } : {}) };
}
