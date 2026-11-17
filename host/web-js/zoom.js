// A zoomable scroller on the JS target (@ref LLP 1057.001.000 §5, "Web"), imported by an app's module only when its
// plan has one (exact-web-js `emit.rs`). The container stays the browser's `overflow: auto` box, and the browser
// scrolls it. A transformed box keeps its untransformed box in its parent's scrollable overflow (CSS Transforms 1 §6),
// so the module owns three boxes in the container: a sizer in flow, the child's margin box at zoom 1 and never more
// than the content box, which keeps the container's own size what the layout gave it; an extent absolutely placed at
// the scroll origin, D3's extent in size with `overflow: clip`, which alone sets the scrollable overflow; and in it a
// layer with the container's content box size and formatting, which lays the child out as the container would and
// carries the zoom (a scale from its top left) and D3's placement. The child's own transform is untouched.
//
// The pinch: `ctrl`+wheel (Chromium's and Firefox's trackpad pinch), Safari's gesture events, and two touches that
// land before the browser began a pan, the container's `touch-action` withholding `pinch-zoom` from the browser. A
// double tap is two touch `pointerup`s within 300 ms (D5). A zoom that moves no offset fires no native `scroll`, so
// the module fires `scroll` and `scrollend` itself.
import { After, Clocked, PropHooks, Adopt, at, clock } from './rt.js';

let Sheet = null;

const Zoomed = new Set(),
  // The `touch-action` values the containers' rules give; `Sheet` holds the rules.
  Owned = new Set(),
  DOUBLE_TAP_MS = 300,
  DOUBLE_TAP_SLOP = 30,
  CONTROL = 'a[href],button,input,select,textarea,summary,[contenteditable=""],[contenteditable="true"],[role="button"],[role="link"],[role="checkbox"],[role="switch"],[role="slider"],[role="tab"]';

/** The container `e`'s layers, made (or adopted from a rendered page); returns the one its children go in. */
export function zoom(e) {
  let sizer = null;
  if (Adopt) {
    const n = at(e);
    if (n?.nodeType === 1 && n.hasAttribute('data-exact-zoom')) sizer = n;
  }
  let extent, layer;
  if (sizer) {
    extent = sizer.nextElementSibling;
    layer = extent.firstElementChild;
    e.$n = extent.nextSibling;
  } else {
    sizer = document.createElement('div');
    extent = document.createElement('div');
    layer = document.createElement('div');
    sizer.setAttribute('data-exact-zoom', '');
    extent.setAttribute('data-exact-zoom-extent', '');
    layer.setAttribute('data-exact-zoom-layer', '');
    // Until the first draw the sizer is out and the extent and layer generate no box, so a page rendered ahead of
    // time, which lays nothing out, shows the child in flow as the container's own at zoom 1.
    sizer.style.cssText = 'display:none;flex:none;max-width:100%;max-height:100%;margin:0;padding:0;border:0;visibility:hidden;pointer-events:none';
    extent.style.cssText = 'display:contents;position:absolute;left:0;top:0;overflow:clip;margin:0;padding:0;border:0';
    // The child's range rows are its container's, not inherited.
    layer.style.cssText = 'display:contents;position:absolute;left:0;top:0;box-sizing:content-box;margin:0;padding:0;border:0;transform-origin:0 0;--exact-min-zoom:initial;--exact-max-zoom:initial';
    extent.append(layer);
    e.append(sizer, extent);
  }
  e.$zoomLayers = { sizer, extent, layer };
  e.$zoom = 1;
  Zoomed.add(e);
  return layer;
}

PropHooks.scrollZoom = (e, v) => {
  if (!e.$zoomLayers) return false;
  const z = v == null ? null : Number(v);
  // A change is a write (D2); an unchanged value written again moves nothing.
  if (z !== e.$zoomWritten) {
    e.$zoomWritten = z;
    if (z != null && Number.isFinite(z) && z > 0) e.$zoomPending = z;
  }
  return true;
};

After.push(() => {
  // A page rendered ahead of time (render.mjs, main-server.js) lays nothing out and has no ResizeObserver: the child
  // stands in flow at zoom 1 until the page boots.
  if (typeof ResizeObserver === 'undefined') return;
  for (const e of Zoomed) {
    if (e.isConnected) settle(e);
    else Zoomed.delete(e);
  }
});

/** The range the container's rows give, its minimum first (D1: either alone is the other's 1). */
function range(e) {
  const s = getComputedStyle(e),
    read = name => {
      const v = s.getPropertyValue(name).trim();
      const n = v.endsWith('%') ? parseFloat(v) / 100 : parseFloat(v);
      return Number.isFinite(n) && n > 0 ? n : 1;
    },
    low = read('--exact-min-zoom');
  return [low, Math.max(low, read('--exact-max-zoom'))];
}

/** The child and the container's metrics: the port, its padding, the child's frame in the layer and its end margins. */
function measure(e) {
  const { layer } = e.$zoomLayers,
    child = layer.firstElementChild;
  if (!(child instanceof HTMLElement)) return null;
  const s = getComputedStyle(e),
    c = getComputedStyle(child),
    px = v => parseFloat(v) || 0;
  return {
    child,
    port: [e.clientWidth, e.clientHeight],
    lead: [px(s.paddingLeft), px(s.paddingTop)],
    end: [px(s.paddingRight), px(s.paddingBottom)],
    frame: [child.offsetLeft, child.offsetTop, child.offsetWidth, child.offsetHeight],
    margin: [px(c.marginRight), px(c.marginBottom)],
  };
}

/** D3 at zoom `z`, per axis: the slack's half before the child, and the extent. */
function place(m, z) {
  const shift = [0, 0],
    extent = [0, 0];
  for (const i of [0, 1]) {
    const used = m.lead[i] + m.frame[i] + m.frame[2 + i] * z + m.end[i] + m.margin[i];
    shift[i] = Math.max(0, m.port[i] - used) / 2;
    extent[i] = Math.max(m.port[i], used);
  }
  return { shift, extent };
}

/** The layers at zoom `z`: the sizer, the layer's formatting and size, the extent and the transform. */
function draw(e, z) {
  const { sizer, extent, layer } = e.$zoomLayers,
    s = getComputedStyle(e);
  // The container is the extent's containing block: the compiler makes every scroller `position: relative` unless it
  // names a position (LLP 1074 T1).
  if (e.$zoomTouch === undefined) {
    // The browser keeps the author's pans; the pinch is the module's (§5). The value is a rule on an attribute, as the
    // inspector reports the container's own style as the author's.
    e.$zoomTouch = s.touchAction;
    const pans = e.$zoomTouch.split(/\s+/).filter(t => t.startsWith('pan-'));
    const own = e.$zoomTouch === 'auto' || e.$zoomTouch === 'manipulation' ? 'pan-x pan-y' : pans.join(' ') || 'none';
    e.setAttribute('data-exact-zoom-touch', own);
    if (!Owned.has(own)) {
      Owned.add(own);
      Sheet ??= document.head.appendChild(document.createElement('style')).sheet;
      Sheet.insertRule(`[data-exact-zoom-touch="${own}"]{touch-action:${own} !important}`, Sheet.cssRules.length);
    }
    listen(e);
    sizer.style.display = '';
    extent.style.display = '';
  }
  const display = { 'inline-flex': 'flex', 'inline-grid': 'grid', 'inline-block': 'block', inline: 'block', contents: 'block' }[s.display] ?? s.display;
  const ls = layer.style;
  ls.display = display;
  for (const p of ['flexDirection', 'flexWrap', 'alignItems', 'alignContent', 'justifyItems', 'justifyContent', 'rowGap', 'columnGap', 'gridTemplateColumns', 'gridTemplateRows', 'gridTemplateAreas', 'gridAutoFlow', 'gridAutoColumns', 'gridAutoRows'])
    if (ls[p] !== s[p]) ls[p] = s[p];
  // The sizer moves the container's size only where it is the content's (an auto size): a pass or two settles it.
  let m = null;
  for (let pass = 0; pass < 3; pass++) {
    const pl = parseFloat(s.paddingLeft) || 0,
      pt = parseFloat(s.paddingTop) || 0;
    ls.width = `${Math.max(0, e.clientWidth - pl - (parseFloat(s.paddingRight) || 0))}px`;
    ls.height = `${Math.max(0, e.clientHeight - pt - (parseFloat(s.paddingBottom) || 0))}px`;
    m = measure(e);
    if (!m) return null;
    const box = [`${m.frame[0] + m.frame[2] + m.margin[0]}px`, `${m.frame[1] + m.frame[3] + m.margin[1]}px`];
    if (sizer.style.width === box[0] && sizer.style.height === box[1]) break;
    sizer.style.width = box[0];
    sizer.style.height = box[1];
  }
  const p = place(m, z);
  extent.style.width = `${p.extent[0]}px`;
  extent.style.height = `${p.extent[1]}px`;
  const tx = m.lead[0] + p.shift[0] + m.frame[0] * (1 - z),
    ty = m.lead[1] + p.shift[1] + m.frame[1] * (1 - z);
  ls.transform = `translate(${tx}px, ${ty}px) scale(${z})`;
  return { m, p };
}

/** After a commit: the range, a write held or applied about the port's centre, and the layers. */
function settle(e) {
  const [low, high] = range(e);
  e.$zoomRange = [low, high];
  const held = !!e.$zoomGesture;
  let z = Math.min(Math.max(e.$zoom, low), high),
    animate = false;
  if (!held && e.$zoomPending != null) {
    const asked = e.$zoomPending;
    e.$zoomPending = null;
    // A zoom the scroll reported since the gesture began is the app mirroring it back (D2). Only a write the gesture
    // held can echo it: a later one is the app's, whatever its value.
    const echo = (e.$zoomReported ?? []).includes(asked);
    e.$zoomReported = [];
    if (!echo) {
      animate = !clock.agent && getComputedStyle(e).scrollBehavior === 'smooth';
      const to = Math.min(Math.max(asked, low), high);
      if (to !== e.$zoom) return void move(e, to, [e.clientWidth / 2, e.clientHeight / 2], animate, true);
    }
  }
  if (z !== e.$zoom) return void move(e, z, [e.clientWidth / 2, e.clientHeight / 2], false, true);
  draw(e, e.$zoom);
}

/** To zoom `z`, keeping the child's point under `focal` (in the port, from its top left) where it is. */
function move(e, z, focal, animate, end) {
  if (animate) {
    const from = e.$zoom,
      t0 = performance.now(),
      step = now => {
        const t = Math.min(1, (now - t0) / 300),
          k = 1 - (1 - t) ** 3;
        move(e, from * (z / from) ** k, focal, false, t === 1);
        if (t < 1) e.$zoomFrame = requestAnimationFrame(step);
      };
    cancelAnimationFrame(e.$zoomFrame);
    e.$zoomFrame = requestAnimationFrame(step);
    return;
  }
  const before = measure(e);
  if (!before) return;
  const was = place(before, e.$zoom),
    u = [0, 1].map(i => (e[i ? 'scrollTop' : 'scrollLeft'] + focal[i] - before.lead[i] - was.shift[i] - before.frame[i]) / e.$zoom);
  const at = [e.scrollLeft, e.scrollTop];
  e.$zoom = z;
  const drawn = draw(e, z);
  if (!drawn) return;
  const { m, p } = drawn;
  offsetTo(e, [0, 1].map(i => m.lead[i] + p.shift[i] + m.frame[i] + u[i] * z - focal[i]), p.extent, m.port, at);
  report(e, end);
}

/** The offsets within the extent. The browser's own `scrollend` for them is the module's to fire (`report`). */
function offsetTo(e, to, extent, port, was) {
  const [x, y] = [0, 1].map(i => Math.min(Math.max(0, to[i]), extent[i] - port[i]));
  // At once, under `scroll-behavior: smooth` too: the zoom's own animation is the motion.
  e.scrollTo({ left: x, top: y, behavior: 'instant' });
  if (e.scrollLeft !== was[0] || e.scrollTop !== was[1]) e.$zoomMoved = true;
}

/** The scroll the zoom made: `scroll` with the zoom (events.js reads `$zoom`), then `scrollend` if it rests. */
function report(e, end) {
  (e.$zoomReported ??= []).push(e.$zoom);
  queueMicrotask(() => {
    e.dispatchEvent(new Event('scroll'));
    if (end) e.dispatchEvent(Object.assign(new Event('scrollend'), { $zoom: true }));
  });
}

/** The container's own pinch, wheel and double tap. */
function listen(e) {
  const clamp = z => Math.min(Math.max(z, e.$zoomRange[0]), e.$zoomRange[1]),
    admits = () => e.$zoomTouch === 'auto' || e.$zoomTouch === 'manipulation' || e.$zoomTouch.includes('pinch-zoom'),
    portPoint = (x, y) => {
      const r = e.getBoundingClientRect();
      return [x - r.left - e.clientLeft, y - r.top - e.clientTop];
    },
    begin = () => {
      e.$zoomGesture = true;
      e.$zoomReported = [];
      cancelAnimationFrame(e.$zoomFrame);
    },
    finish = () => {
      e.$zoomGesture = false;
      report(e, true);
      // A write the gesture held, now at rest.
      if (e.$zoomPending != null) settle(e);
    };
  let wheelEnd = 0;
  e.addEventListener(
    'wheel',
    ev => {
      if (!ev.ctrlKey || !admits()) return;
      ev.preventDefault();
      if (!e.$zoomGesture) begin();
      const z = clamp(e.$zoom * Math.exp(-ev.deltaY * (ev.deltaMode === 1 ? 0.05 : 0.01)));
      if (z !== e.$zoom) move(e, z, portPoint(ev.clientX, ev.clientY), false, false);
      clearTimeout(wheelEnd);
      // A wheel has no phase: the agent's one wheel rests at once, a hand's after a pause.
      if (clock.agent) finish();
      else wheelEnd = setTimeout(finish, 150);
    },
    { passive: false },
  );
  // Safari's trackpad pinch.
  let gestureFrom = 1;
  e.addEventListener('gesturestart', ev => {
    if (!admits()) return;
    ev.preventDefault();
    begin();
    gestureFrom = e.$zoom;
  });
  e.addEventListener('gesturechange', ev => {
    if (!e.$zoomGesture) return;
    ev.preventDefault();
    const z = clamp(gestureFrom * ev.scale);
    if (z !== e.$zoom) move(e, z, portPoint(ev.clientX, ev.clientY), false, false);
  });
  e.addEventListener('gestureend', ev => {
    if (!e.$zoomGesture) return;
    ev.preventDefault();
    finish();
  });
  // Two touches that land before the browser began a pan: their move is still cancelable.
  let pinch = null;
  const pair = ev => {
    const [a, b] = ev.touches;
    return { d: Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY), at: portPoint((a.clientX + b.clientX) / 2, (a.clientY + b.clientY) / 2) };
  };
  e.addEventListener(
    'touchmove',
    ev => {
      if (ev.touches.length !== 2 || !admits()) return;
      if (!pinch) {
        if (!ev.cancelable) return;
        const m = measure(e);
        if (!m) return;
        begin();
        const p = pair(ev),
          was = place(m, e.$zoom);
        pinch = { d: Math.max(1, p.d), z: e.$zoom, u: [0, 1].map(i => (e[i ? 'scrollTop' : 'scrollLeft'] + p.at[i] - m.lead[i] - was.shift[i] - m.frame[i]) / e.$zoom) };
      }
      ev.preventDefault();
      const p = pair(ev),
        z = clamp((pinch.z * p.d) / pinch.d);
      // The child's point under the fingers stays under them as they move apart and together.
      const at = [e.scrollLeft, e.scrollTop];
      e.$zoom = z;
      const drawn = draw(e, z);
      if (!drawn) return;
      const { m, p: placed } = drawn;
      offsetTo(e, [0, 1].map(i => m.lead[i] + placed.shift[i] + m.frame[i] + pinch.u[i] * z - p.at[i]), placed.extent, m.port, at);
      report(e, false);
    },
    { passive: false },
  );
  const lift = ev => {
    if (pinch && ev.touches.length < 2) {
      pinch = null;
      finish();
    }
  };
  e.addEventListener('touchend', lift);
  e.addEventListener('touchcancel', lift);
  // The browser's `scrollend` for offsets the zoom wrote: the zoom's own ends the scroll once.
  e.addEventListener(
    'scrollend',
    ev => {
      if (ev.$zoom || !(e.$zoomGesture || e.$zoomMoved)) return;
      e.$zoomMoved = false;
      ev.stopImmediatePropagation();
    },
    true,
  );
  // D5's double tap, where `touch-action` is `auto` and no control or `dblclick` takes it. A touch's press on content
  // waits until a second tap cannot come, as a browser that zooms on a double tap waits (`click`, below).
  const doubles = t => e.$zoomTouch === 'auto' && t instanceof Element && !t.closest(CONTROL) && !t.closest('[data-exact-on~="dblclick"]');
  let last = null,
    touched = false,
    held = null,
    swallow = false;
  e.addEventListener(
    'click',
    ev => {
      if (ev.$zoomHeld || !touched || !doubles(ev.target)) return;
      touched = false;
      ev.stopImmediatePropagation();
      ev.preventDefault();
      if (swallow) return void (swallow = false);
      const { target, clientX, clientY, screenX, screenY, shiftKey, ctrlKey, altKey, metaKey } = ev,
        press = () => {
          held = null;
          if (target.isConnected)
            target.dispatchEvent(Object.assign(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1, clientX, clientY, screenX, screenY, shiftKey, ctrlKey, altKey, metaKey }), { $zoomHeld: true }));
        };
      clearTimeout(held?.timer);
      // On the agent's clock, the wait is the driver's to move (`clock +300`).
      held = clock.agent ? { due: clock.now + DOUBLE_TAP_MS, press } : { timer: setTimeout(press, DOUBLE_TAP_MS) };
    },
    true,
  );
  Clocked.push(() => {
    if (held?.due !== undefined && clock.now >= held.due) queueMicrotask(held.press);
  });
  e.addEventListener('pointerup', ev => {
    touched = ev.pointerType === 'touch';
    if (!touched || pinch || !doubles(ev.target)) return void (last = null);
    const tap = { t: ev.timeStamp, x: ev.clientX, y: ev.clientY };
    if (!last || tap.t - last.t > DOUBLE_TAP_MS || Math.hypot(tap.x - last.x, tap.y - last.y) > DOUBLE_TAP_SLOP) return void (last = tap);
    last = null;
    // The double tap: neither tap presses.
    clearTimeout(held?.timer);
    held = null;
    swallow = true;
    const [low, high] = e.$zoomRange,
      z = e.$zoom > low ? low : Math.min(high, 2 * low);
    if (z === e.$zoom) return;
    e.$zoomReported = [];
    move(e, z, portPoint(tap.x, tap.y), !clock.agent, true);
  });
}
