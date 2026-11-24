// Geometry reads for actions (LLP 1051.000 D1, D4, D5): where the viewer
// sees an element, and its border box at `height: auto`, answered synchronously to
// the runner's one import, `exact_geometry.read(op, view, out)`. Loaded after
// first paint when the app's wasm imports it; until then glue.js answers 0
// (unavailable).
//
// `frame` writes nothing: the border box where the viewer sees it,
// `getBoundingClientRect`'s, through every transform and with every scroll
// offset above it applied (LLP 1051.000 D1; the kernel's hosts compose the
// same from their rows and the offsets their presenters note). `measure`
// changes nothing the next rendering step sees: it takes motion off and
// lays the node out at `height: auto` within the call, then puts both back.
import { size } from './presence-glue.js';

// Reply bits: answered, provisional.
const ANSWERED = 1, PROVISIONAL = 2;
// Animated properties that leave layout alone (keyframe keys, kebab-cased):
// among them the paint that Contract's `keyframes` admit, the custom
// properties Exact's own paint and press feedback animate (`css.rs`,
// input-glue.js), and what its layout transition holds.
const PAINT_ONLY = new Set(['transform', 'translate', 'scale', 'rotate', 'opacity', 'filter', 'backdrop-filter',
  'color', 'background-color', 'background-image', 'box-shadow', 'clip-path', 'visibility',
  'border-color', 'border-top-color', 'border-right-color', 'border-bottom-color', 'border-left-color',
  'fill', 'stroke', 'stroke-dashoffset', 'r', 'cx', 'cy', 'x', 'y', 'rx', 'ry',
  '--exact-tint', '--exact-scale', '--exact-scale-y', '--exact-translate-x', '--exact-translate-y', '--exact-translate-z',
  '--exact-press-paint', '--exact-press-opacity', '--exact-press-color', '--exact-press-background-color',
  'offset', 'easing', 'composite', 'computed-offset']);
const kebab = name => name.replace(/[A-Z]/g, c => '-' + c.toLowerCase());

// The properties an animation plays, running or held: a CSS transition names
// its property; a keyframe effect names every property it animates.
function played(animation) {
  if (animation.playState !== 'running' && animation.playState !== 'paused') return [];
  return typeof CSSTransition !== 'undefined' && animation instanceof CSSTransition
    ? [animation.transitionProperty]
    : (animation.effect?.getKeyframes?.() ?? []).flatMap(frame => Object.keys(frame).map(kebab));
}
const movesLayout = animation => played(animation).some(name => !PAINT_ONLY.has(name));
const cssAnimation = animation => typeof CSSAnimation !== 'undefined' && animation instanceof CSSAnimation;
const cssTransition = animation => typeof CSSTransition !== 'undefined' && animation instanceof CSSTransition;

// Boxes a person or a write has scrolled, whose offsets a read must put back:
// those scrolled when this loads, and each a `scroll` event names after.
const scrolled = new Set();
addEventListener('scroll', e => { if (e.target instanceof Element) scrolled.add(e.target); }, true);

// An inline declaration back as it was, or gone.
function restore(style, [name, value, priority]) {
  if (value) style.setProperty(name, value, priority); else style.removeProperty(name);
}

// Fields with text being composed: a range read then answers nothing.
const composing = new WeakSet();
addEventListener('compositionstart', e => composing.add(e.target), true);
addEventListener('compositionend', e => composing.delete(e.target), true);

function createGeometry(root) {
  for (const a of root.querySelectorAll('*')) if (a.scrollTop || a.scrollLeft) scrolled.add(a);
  // Where the viewer sees `el`'s border box: its client rect.
  const frame = el => {
    const r = el.getBoundingClientRect();
    return [r.x, r.y, r.width, r.height];
  };

  // Its border box at `height: auto`, every other style kept, with motion
  // in flight removed, as the kernel measures with presented heights removed
  // (D4). Each transition and script animation that may move layout has its
  // keyframes taken off for the read and put back, keeping its clock and
  // its events: a transition reads at its end, anything else at the style
  // under it. (WebKit stops applying a transition whose effect's target is
  // cleared and set again.) One that plays `display` stays: taken off, its
  // subtree could be `display: none` for the read, which cancels every
  // animation in it. A CSS animation's keyframes are the stylesheet's, and
  // setting them would cut it off from the sheet, so one that moves layout
  // in the box or above it refuses the read; Contract's keyframes move
  // layout only through a font's weight, stretch or variation settings.
  //
  // The height is an inline `!important` write, which outranks a sheet's
  // `!important` detent height, with the transition timing zero so it
  // starts no transition and cancels none. A box whose own height is in a
  // CSS transition takes a script animation instead, since a style change
  // would cancel that transition. Every scroller the pass can move is put
  // back: an ancestor of a changed box, which anchoring may shift, and every
  // scrolled box, whose offset is clamped when it shrinks.
  const auto = el => {
    const above = [];
    for (let a = el.parentElement; a; a = a.parentElement) above.push(a);
    if ([...el.getAnimations({ subtree: true }), ...above.flatMap(a => a.getAnimations())].some(a => cssAnimation(a) && movesLayout(a))) return null;
    const outside = [];
    for (let a = root.parentElement; a; a = a.parentElement) outside.push(...a.getAnimations());
    const off = [...root.getAnimations({ subtree: true }), ...outside].flatMap(a => {
      const names = cssAnimation(a) ? [] : played(a);
      return names.some(name => !PAINT_ONLY.has(name)) && !names.includes('display') ? [[a, a.effect, a.effect.getKeyframes(), names]] : [];
    });
    const scrollers = new Map();
    const hold = a => {
      if (scrollers.has(a)) return;
      scrollers.set(a, [a.scrollLeft, a.scrollTop, ['overflow-anchor', a.style.getPropertyValue('overflow-anchor'), a.style.getPropertyPriority('overflow-anchor')]]);
      a.style.setProperty('overflow-anchor', 'none', 'important');
    };
    for (const moved of new Set([el, ...off.map(([, effect]) => effect.target)])) {
      if (moved.scrollTop || moved.scrollLeft) hold(moved);
      for (let a = moved.parentElement; a; a = a.parentElement) {
        if (a.scrollHeight > a.clientHeight || a.scrollWidth > a.clientWidth) hold(a);
      }
    }
    for (const a of scrolled) {
      if (!a.isConnected) scrolled.delete(a);
      else if (a.scrollTop || a.scrollLeft) hold(a);
    }
    const ownHeight = off.some(([a, effect, , names]) => cssTransition(a) && effect.target === el && !effect.pseudoElement
      && names.some(name => name === 'height' || name === 'block-size'));
    const style = el.style, names = ownHeight ? [] : ['height', 'transition-duration', 'transition-delay'];
    const saved = names.map(name => [name, style.getPropertyValue(name), style.getPropertyPriority(name)]);
    let what;
    try {
      for (const [, effect] of off) effect.setKeyframes([]);
      if (ownHeight) what = el.animate([{ height: 'auto' }, { height: 'auto' }], Infinity);
      else {
        style.setProperty('transition-duration', '0s', 'important');
        style.setProperty('transition-delay', '0s', 'important');
        style.setProperty('height', 'auto', 'important');
      }
      return size(getComputedStyle(el));
    } finally {
      what?.cancel();
      if (saved.length) {
        restore(style, saved[0]);
        void getComputedStyle(el).height; // the height is back before the timing is
        restore(style, saved[1]);
        restore(style, saved[2]);
      }
      for (const [, effect, frames] of off) effect.setKeyframes(frames);
      // Offsets land at once, since `scroll-behavior: smooth` animates an
      // assignment (CSSOM View §7), and only where one moved, so no snap
      // restarts and no pan is cancelled.
      for (const [a, [left, top]] of scrollers) {
        if (a.scrollLeft !== left || a.scrollTop !== top) a.scrollTo({ left, top, behavior: 'instant' });
      }
      for (const [a, [, , anchor]] of scrollers) restore(a.style, anchor);
    }
  };

  // An image or video inside that waits for its natural size, or fonts still
  // loading: the box may change (D5).
  const provisional = el => {
    const media = [el, ...el.querySelectorAll('img, video')];
    return media.some(m => (m instanceof HTMLImageElement && (!m.complete || m.naturalWidth === 0))
      || (m instanceof HTMLVideoElement && m.readyState < 1))
      || (document.fonts && document.fonts.status !== 'loaded');
  };

  // `elementFromPoint(x, y)` (LLP 1094 D10): the page's own hit test, DOM's
  // `elementsFromPoint`, front to back — through every transform, as `frame`
  // reads boxes (LLP 1051.000 D1, changed 2026-10-04), an overflow clip
  // cutting descendants, `pointer-events: none` and a hidden `visibility`
  // passed over — less what is not the app's own: a leaving node, the host's
  // drag ghost and anything else outside the root. The runner's kernel walk
  // answers the same natively.
  const points = (x, y) => document.elementsFromPoint(x, y)
    .filter(el => el !== root && root.contains(el) && !el.closest('[data-exiting]'));
  const point = (x, y) => points(x, y)[0] ?? null;
  const namedPoints = (x, y) => {
    const hits = points(x, y), seen = new Set();
    return hits.flatMap(el => {
      let named = el;
      if (!named.id) {
        named = el.closest('[id]:not([id=""])');
        if (!named || hits.includes(named)) return [];
        const box = named.getBoundingClientRect();
        if (x >= box.left && x < box.right && y >= box.top && y < box.bottom) return [];
      }
      if (named === root || seen.has(named)) return [];
      seen.add(named);
      return [named.id];
    });
  };

  // Whether `offset` is on a line with no text (an empty field's too): a
  // textarea's value breaks at LF (CR and CR LF become it) and at the other
  // forced breaks a line keeps (CSS Text's BK and NL).
  const breaks = '\n\u000b\u000c\u0085\u2028\u2029';
  const emptyLine = (value, offset) =>
    (offset === 0 || breaks.includes(value[offset - 1])) && (offset === value.length || breaks.includes(value[offset]));
  // Where a textarea paints its caret at `offset`: a twin of the field,
  // every computed style copied (its font, wrapping, writing mode, box,
  // scrollbars, transform and zoom), standing where the field stands among
  // the same ancestors and scrolled as it is, holding its whole value with
  // a zero-width space at the offset; that space's box is the caret's
  // (the text's height there), read in the viewport, then the twin goes.
  const caretProbe = (el, offset) => {
    const parent = el.parentNode;
    if (!parent) return null;
    const style = getComputedStyle(el), twin = document.createElement('exact-caret-twin');
    // Its `left` and `top` are in its own zoomed px; the offsets are not.
    const zoom = parseFloat(style.zoom) || 1;
    for (const p of style) twin.style.setProperty(p, style.getPropertyValue(p));
    Object.assign(twin.style, {
      position: style.position === 'fixed' ? 'fixed' : 'absolute', left: `${el.offsetLeft / zoom}px`, top: `${el.offsetTop / zoom}px`,
      right: 'auto', bottom: 'auto', margin: '0', visibility: 'hidden', pointerEvents: 'none', display: 'block', float: 'none',
      // It moves, and scrolls, at once.
      transition: 'none', animation: 'none', scrollBehavior: 'auto',
      // The field's border box: a computed content width is less its own
      // scrollbar, which the twin lays out again.
      boxSizing: 'border-box', width: `${el.offsetWidth}px`, height: `${el.offsetHeight}px`,
      minWidth: '0', maxWidth: 'none', minHeight: '0', maxHeight: 'none',
      minInlineSize: '0', maxInlineSize: 'none', minBlockSize: '0', maxBlockSize: 'none',
    });
    // A textarea's editor keeps every space and break whatever its
    // `white-space` (Blink makes `normal` `pre-wrap` and `nowrap` `pre`;
    // `break-spaces` it leaves).
    if (style.whiteSpaceCollapse !== 'break-spaces') twin.style.whiteSpaceCollapse = 'preserve';
    // Never read, reached or focused while it stands (inert takes the
    // focus and the pointer from it and its text).
    twin.setAttribute('aria-hidden', 'true');
    twin.inert = true;
    const mark = document.createElement('exact-caret-mark');
    mark.style.cssText = 'all: unset; display: inline';
    mark.textContent = '\u200b';
    // A last empty line is a line in the field (its editor's placeholder
    // break), as it isn't after a bare break in a box: keep it.
    const last = offset < el.value.length && breaks.includes(el.value.at(-1)) ? '\u200b' : '';
    twin.append(el.value.slice(0, offset), mark, el.value.slice(offset) + last);
    parent.insertBefore(twin, el.nextSibling);
    // Its containing block may not be the field's offset parent (a
    // transformed ancestor, zoom): find where its box moves per px of
    // `left` and `top` in the viewport (a box's bounds move with it under
    // any transform), then move it onto the field's.
    const at = () => { const r = twin.getBoundingClientRect(); return [r.x, r.y]; };
    const [left, top] = [el.offsetLeft / zoom, el.offsetTop / zoom], r0 = at();
    twin.style.left = `${left + 100}px`;
    const r1 = at();
    twin.style.left = `${left}px`;
    twin.style.top = `${top + 100}px`;
    const r2 = at(), e = el.getBoundingClientRect();
    const [ax, ay, bx, by] = [(r1[0] - r0[0]) / 100, (r1[1] - r0[1]) / 100, (r2[0] - r0[0]) / 100, (r2[1] - r0[1]) / 100];
    const [dx, dy, det] = [e.x - r0[0], e.y - r0[1], ax * by - bx * ay];
    if (!det) { twin.remove(); return null; }
    twin.style.left = `${left + (dx * by - bx * dy) / det}px`;
    twin.style.top = `${top + (ax * dy - dx * ay) / det}px`;
    twin.scrollTop = el.scrollTop;
    twin.scrollLeft = el.scrollLeft;
    const r = mark.getBoundingClientRect();
    twin.remove();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  };

  return {
    point, points, namedPoints,
    // The painted caret as `caretProbe` finds it, for the tests that hold it
    // to Chrome's own carets on lines with text.
    caretAt(el, offset) { return caretProbe(el, offset); },
    scrollMetrics(el) {
      if (!el?.isConnected || !root.contains(el) || (el.localName !== 'textarea' && !el.exactMarkup)
        || !el.getClientRects().length || getComputedStyle(el).visibility === 'hidden') return null;
      return [el.scrollLeft, el.scrollTop, el.scrollWidth, el.scrollHeight, el.clientWidth, el.clientHeight];
    },
    // `rangeFrame` (LLP 1114.007): a plain textarea's range through
    // `createValueRange` (OpaqueRange), the Markdown editor's through its own
    // source mapping; a collapsed range is the caret's rectangle. Offsets are
    // truncated toward zero; a bad range, one past the text, a hidden field
    // and composition answer nothing.
    range(el, start, end, out) {
      if (!el?.isConnected || !root.contains(el) || !el.getClientRects().length || getComputedStyle(el).visibility === 'hidden') return 0;
      [start, end] = [Math.trunc(start), Math.trunc(end)];
      // A Markdown reader answers from its atoms' pills (LLP 1045.000 D7, atoms.js), its offsets the source's.
      const reader = !el.exactMarkup && !('value' in el) && el.getAttribute('markup') === 'markdown';
      if (!(start >= 0 && start <= end && end <= (reader ? el.$source?.length ?? Infinity : el.value?.length ?? 0)) || composing.has(el) || el.exactComposing) return 0;
      let box = null;
      if (reader) box = globalThis.exact?.atoms?.rangeFrame(el, start, end) ?? null;
      else if (el.exactMarkup) box = el.exactMarkup.rangeFrame?.(start, end) ?? null;
      else if (el instanceof HTMLTextAreaElement && typeof el.createValueRange === 'function') {
        // Chrome puts a caret on an empty line at the end of the text before
        // it, and an empty field's at the viewport's origin, where it paints
        // the caret on that line: the grammar's answer is the painted caret.
        if (start === end && emptyLine(el.value, start)) box = caretProbe(el, start);
        else try { const range = el.createValueRange(start, end); box = range.getBoundingClientRect(); range.disconnect?.(); } catch { return 0; }
      }
      if (!box) return 0;
      out.set([box.x, box.y, box.width, box.height]);
      return ANSWERED | (provisional(el) ? PROVISIONAL : 0);
    },
    read(op, el, out) {
      if (!el?.isConnected || !root.contains(el)) return 0;
      const box = frame(el);
      if (!box) return 0;
      if (op === 1) {
        const measured = auto(el);
        if (!measured) return 0;
        [box[2], box[3]] = measured;
      }
      out.set(box);
      return ANSWERED | (provisional(el) ? PROVISIONAL : 0);
    },
  };
}

if (globalThis.exact) globalThis.exact.geometry = createGeometry;
