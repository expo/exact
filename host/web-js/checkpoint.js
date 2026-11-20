import { viewChildren, viewParent, inertAncestor } from './navigation.js';
import { After, PropHooks, Resources, carryRouter, clock, data, inflight, journal, late, launchLocation, res, settled, sig } from './rt.js';
import { offerAll, restartAutofocus } from './focus.js';

// A render's checkpoint answers (LLP 1048.000 D6), as
// `exact_web::document::checkpoint` writes them: JSON text, one
// `[name, source, [args…], value]` per answer the document used, each value
// typed as `push_value` (host/web/src/page.rs) spells it — a list an array,
// a record `{"r":[…]}`, `none` `{}`, `some(v)` `{"s":v}`, unit `null`.
function encode(v, t) {
  if (t === 'n') return Object.is(v, -0) ? '-0' : Number.isFinite(v) ? JSON.stringify(v) : `{"n":"${v}"}`;
  if (t === 'b') return v ? 'true' : 'false';
  if (t === 's') return JSON.stringify(v);
  if (t === 'u') return 'null';
  if (Array.isArray(t) && t[0] === '?') return v == null ? '{}' : `{"s":${encode(v, t[1])}}`;
  if (Array.isArray(t)) return `[${v.map(x => encode(x, t[1])).join(',')}]`;
  return `{"r":[${Object.keys(t).map((k, i) => encode(v[i], t[k])).join(',')}]}`;
}
/** The page's checkpoint answers as JSON text (`answers`), with the indices in them of those the reader's page shows
 * and asks again (`open`) and of those that read the store (`readers`).
 * `resources` are the plan's declared resources in plan order, as `types` are, and a list writes each element's answer
 * in the list's order. */
export function answers(resources, types, sourceTypes) {
  const out = [], open = [], readers = [], urls = [];
  const write = (r, type) => {
    // Leave out an answer not yet given, and a runner's own source (the date, the viewport), which is the reader's
    // device: the runtime asks it again, as `Runner::document_checkpoint` leaves it out
    if (r.settled === undefined || r.owned) {
      return;
    }
    const params = sourceTypes[r.source]?.[0] ?? [];
    // List in `open`, for the reader's page to ask again, an answer whose work goes on past the render: a ticket kept
    // (an open stream, rt.js `land`, or a request still pending; one not yet sent leaves the page to `revalidate`), or
    // the call that gave it watched a document or was refused storage or visitor credentials (ts-data.js
    // `refuseInRender`): each of a list's elements by its own answer
    if (r.ticket || r.renderCall?.watchedInRender || r.renderCall?.refused) {
      open.push(out.length);
    }
    if (r.store) {
      readers.push(out.length);
    }
    if (!globalThis.__exactProduction) urls.push([...(r.renderUrls ?? [])]);
    out.push(`[${JSON.stringify(r.name)},${JSON.stringify(r.source)},[${r.settled.map((a, k) => encode(a, params[k] ?? 's')).join(',')}],${encode(r.value, type)}]`);
  };
  // a child's resource writes each instance's answer, which the boot's instances take by their arguments (LLP 1035.005.004 D8)
  resources.forEach(({ r }, i) => r.each ? r.members().forEach(element => write(element, types[i][1])) : r.instances ? r.members().forEach(own => write(own, types[i])) : write(r, types[i]));
  return { answers: `[${out.join(',')}]`, open, readers, ...(!globalThis.__exactProduction ? { urls } : {}) };
}

// The JS dev loop's checkpoint handoff. Its separate MIME type keeps rt.js's
// rendered-document checkpoint — including an agent drive's zero clock —
// unchanged. Answers here are validated by source and full result type;
// Slots and view records are development-only fields.
let Dev;
const decode = v => v === null || typeof v !== 'object' ? v : Array.isArray(v) ? v.map(decode) : '$exactNumber' in v ? Number(v.$exactNumber) : Object.fromEntries(Object.entries(v).map(([k, x]) => [k, decode(x)]));
const shape = (a, b) => JSON.stringify(a) === JSON.stringify(b);
function dev() {
  if (Dev !== undefined) return Dev;
  const el = typeof document === 'object' && document.querySelector('script[type="application/vnd.exact.dev-checkpoint"]');
  if (!el) return Dev = null;
  Dev = JSON.parse(el.textContent);
  clock.now = Number.isFinite(Dev.time) ? Dev.time : 0;
  clock.generation = (Dev.generation ?? 0) + 1;
  if (!Number.isSafeInteger(clock.generation)) throw new Error("session generation exhausted");
  clock.epoch = Dev.epoch ?? 0;
  Replacement = { at: clock.now, dropped: [] };
  return Dev;
}

let Replacement = null;
const Slots = new Map();
/** The root-slot report; unbuilt tab panels remain carried rather than dropped. */
export function replacement() {
  if (!Replacement) return null;
  Replacement.dropped = (dev()?.slots ?? []).filter(([name]) => !name.includes('@')).flatMap(([name, type]) => {
    const found = Slots.get(name), why = !found ? 'no place' : !found.router && !shape(type, found.type) ? 'type changed' : null;
    return why ? [{ what: 'state', name, why }] : [];
  });
  return Replacement;
}

/** A root slot carried by authored name and full declared shape. */
export function devSignal(name, initial, type, declared, router = false) {
  // The launch locale must reach newly added initializers before language().
  Slots.set(name, { type: declared, router });
  const cp = dev(), kept = name === '#locale' ? null : cp?.slots?.find(s => s[0] === name);
  const value = kept && (router || shape(kept[1], declared)) ? (router ? carryRouter(decode(kept[2]), launchLocation()) : decode(kept[2])) : initial;
  const out = sig(value, type, type === "s" ? name : undefined); out.n.devName = name; out.n.devType = declared;
  if (name.includes('@')) { Panels.live.add(out); Panels.used.add(name); }
  return out;
}
/** A child's state used outside every region (rt.js `late`): what the last reload carried, else its initializer's
 * value as the boot starts. */
export function devLate(name, init, type, declared) {
  const kept = dev()?.slots?.find(s => s[0] === name);
  if (kept && shape(kept[1], declared)) return devSignal(name, decode(kept[2]), type, declared);
  Slots.set(name, { type: declared, router: false });
  const out = late(init, type, type === "s" ? name : undefined); out.n.devName = name; out.n.devType = declared;
  return out;
}
/** A tab panel's state, `slot@panel`, made when the panel is built (emit's `owned`; LLP 1075.003 §9.14): the capture
 * (dev.mjs) takes each live one, and what the last reload carried for a panel not built since. */
const Panels = globalThis.__exactDevPanels = { live: new Set(), used: new Set(), left: () => (dev()?.slots ?? []).filter(s => s[0].includes('@') && !Panels.used.has(s[0])) };

/** A settled resource carried only for the same source, arguments and type. */
export function devResource(name, source, args, initial, initialArgs, type, placeholder, declared, elseRow = false) {
  const cp = dev(), kept = cp?.carryAnswers === false ? null : cp?.answers?.find(a => a[0] === name && a[1] === source && shape(a[4], declared));
  const out = res(name, source, args, kept ? decode(kept[3]) : initial, kept ? decode(kept[2]) : initialArgs, type, placeholder, !!kept || elseRow);
  out.r.devType = declared; if (kept?.[5]) out.r.store = true; return out;
}

// Until places reach the JS emitter, records use the checkpoint's logical tree path
// (LLP 1029.000.002 §6, piece 7), including virtual row wrappers.
const children = e => viewChildren(e).filter(x => x.hasAttribute('data-carry-type') || x.hasAttribute('data-listitemkey'));
const followers = new WeakMap();
function place(el) {
  const path = [], type = el?.getAttribute('data-carry-type');
  for (let p; el && el.id !== 'exact-root'; el = p) {
    p = viewParent(el);
    path.unshift(children(p).indexOf(el));
  }
  return el && !path.includes(-1) ? { path, type } : null;
}
function find(record) {
  if (!Array.isArray(record.path)) return null;
  let el = document.getElementById('exact-root');
  for (const at of record.path) el = children(el)[at];
  return el?.getAttribute('data-carry-type') === record.type ? el : null;
}
const port = el => ({ left: 0, top: 0,
  right: el === document.scrollingElement ? innerWidth : el.clientWidth,
  bottom: el === document.scrollingElement ? innerHeight : el.clientHeight });
function dimensions(node) {
  const css = getComputedStyle(node);
  return ['width', 'height'].map((name, i) => {
    let value = parseFloat(css[name]);
    if (!Number.isFinite(value)) return node[i ? 'offsetHeight' : 'offsetWidth'];
    if (css.boxSizing !== 'border-box') {
      for (const side of i ? ['Top', 'Bottom'] : ['Left', 'Right']) value += (parseFloat(css[`padding${side}`]) || 0) + (parseFloat(css[`border${side}Width`]) || 0);
    }
    return value;
  });
}
const transformed = n => {
  const css = getComputedStyle(n);
  return ['transform', 'translate', 'rotate', 'scale', 'perspective'].some(name => css[name] !== 'none');
};
function layoutPosition(node) {
  if (!(node instanceof HTMLElement)) return null;
  let left = 0, top = 0;
  for (let n = node; n; n = n.offsetParent) {
    left += n.offsetLeft + (n.offsetParent?.clientLeft ?? 0);
    top += n.offsetTop + (n.offsetParent?.clientTop ?? 0);
  }
  return { left, top };
}
// Scroll Anchoring §2.2 measures within the scrolling box, before transforms
// on that box or its ancestors. Client rects retain subpixels and descendant
// transforms only when no outer transform projects them into screen space.
function box(el, node) {
  const page = el === document.scrollingElement;
  let outerTransform = false;
  for (let n = el; n; n = n.parentElement) outerTransform ||= transformed(n);
  let result;
  if (!outerTransform) {
    const r = node.getBoundingClientRect(), p = page ? { left: 0, top: 0 } : el.getBoundingClientRect();
    const left = r.left - p.left - (page ? 0 : el.clientLeft), top = r.top - p.top - (page ? 0 : el.clientTop);
    result = { left, top, right: left + r.width, bottom: top + r.height };
  } else {
    // SVG has no offset box. An unmeasurable candidate falls back to another
    // viable anchor, its recorded ancestor, or the numeric scroll offset.
    const at = layoutPosition(node), origin = layoutPosition(el);
    if (!at || !origin || !el.clientWidth || !el.clientHeight) return null;
    let left = at.left - origin.left - (page ? 0 : el.clientLeft), top = at.top - origin.top - (page ? 0 : el.clientTop);
    for (let n = node.parentElement; n; n = n.parentElement) {
      left -= n.scrollLeft; top -= n.scrollTop;
      if (n === el) break;
    }
    const [width, height] = dimensions(node);
    result = { left, top, right: left + width, bottom: top + height };
  }
  return Object.values(result).every(Number.isFinite) ? result : null;
}
const scrollContainer = n => {
  const s = getComputedStyle(n);
  return [s.overflowX, s.overflowY].some(v => ['auto', 'scroll', 'hidden'].includes(v));
};
// CSS Scroll Anchoring §2.1: prefer a viable focused editor, otherwise examine
// children in order, descending into partially visible boxes. Excluded subtrees
// cannot supply an anchor. A nested scroller supplies its box, not its content.
function anchor(el) {
  const p = port(el), css = getComputedStyle(el);
  p.top += parseFloat(css.scrollPaddingTop) || 0;
  p.bottom -= parseFloat(css.scrollPaddingBottom) || 0;
  const excluded = n => {
    const s = getComputedStyle(n);
    return s.display === 'none' || s.overflowAnchor === 'none' || ['fixed', 'sticky'].includes(s.position)
      || s.position === 'absolute' && n.offsetParent && n.offsetParent !== el && n.offsetParent.contains(el);
  };
  const examine = n => {
    if (excluded(n)) return null;
    const s = getComputedStyle(n);
    if (s.display === 'contents') return [...n.children].map(examine).find(Boolean);
    const r = box(el, n);
    if (!r) return null;
    if (s.overflowY === 'visible') r.bottom = Math.max(r.bottom, r.top + n.clientTop + n.scrollHeight);
    if (s.overflowX === 'visible') r.right = Math.max(r.right, r.left + n.clientLeft + n.scrollWidth);
    if (r.bottom <= p.top || r.top >= p.bottom || r.right <= p.left || r.left >= p.right) return null;
    const candidate = s.display !== 'inline' && place(n) ? n : null;
    if (candidate && r.top >= p.top && r.bottom <= p.bottom && r.left >= p.left && r.right <= p.right) return candidate;
    if (scrollContainer(n) || n instanceof SVGElement) return candidate;
    for (const child of n.children) { const found = examine(child); if (found) return found; }
    return candidate;
  };
  const focused = document.activeElement;
  if (focused?.matches('input, textarea, [contenteditable]') && el.contains(focused)) {
    let allowed = true;
    for (let n = focused; n && n !== el; n = n.parentElement) {
      if (excluded(n) || n !== focused && scrollContainer(n)) allowed = false;
    }
    if (allowed) { const found = examine(focused); if (found) return found; }
  }
  for (const child of el.children) { const found = examine(child); if (found) return found; }
  return null;
}
function records() {
  const active = document.activeElement?.closest('[data-carry-type]');
  const focus = active && place(active);
  if (focus && typeof active.selectionStart === 'number') Object.assign(focus, {
    text: active.value, start: active.selectionStart, end: active.selectionEnd, direction: active.selectionDirection,
  });
  const root = document.getElementById('exact-root'), scrolls = [];
  for (const el of new Set([document.scrollingElement, root, ...root.querySelectorAll('[data-carry-type]')])) {
    if (!el || !(el.scrollTop || el.scrollLeft || el.$list || followers.get(el))) continue;
    const where = el === document.scrollingElement ? { page: true } : place(el);
    if (!where) continue;
    const p = port(el), anchors = [];
    for (let n = !el.scrollTop || el.$list || getComputedStyle(el).overflowAnchor === 'none' ? null : anchor(el); n && n !== el; n = viewParent(n)) {
      const at = place(n), r = box(el, n);
      if (at && r) anchors.push({ ...at, x: r.left - p.left, y: r.top - p.top });
    }
    const c = el.$list, a = c?.anchor();
    const list = a && { ...a, order: c.mounted.map(m => m.key), row: a.row === null ? null : c.mounted.findIndex(m => m.key === a.order[a.row]) };
    const follows = followers.get(el) && el.scrollHeight - el.clientHeight - el.scrollTop <= 1;
    const axes = {
      left: { target: el.scrollLeft, ...(anchors.length ? { anchor: { nodes: anchors, coordinate: 'x' } } : {}) },
      top: { target: el.scrollTop, ...(anchors.length ? { anchor: { nodes: anchors, coordinate: 'y' } } : {}) },
    };
    if (follows) axes.top.anchor = { end: true };
    if (list) axes[c.axis === 'x' ? 'left' : 'top'].anchor = { list };
    scrolls.push({ ...where, axes });
  }
  return { focus: focus || null, scrolls };
}

function axisTarget(el, axis, waiting, listPending) {
  const a = axis.anchor;
  if (!a) return { target: axis.target };
  if (a.end) return { target: el.scrollHeight - el.clientHeight };
  if (a.list) {
    const c = el.$list, saved = a.list;
    if (!c?.geometry) return { pending: true };
    if (waiting && !saved.follows && saved.row !== null && saved.fixed == null && !c.index.pos.has(saved.order[saved.row])) return { pending: true };
    if (!axis.started) {
      axis.started = true;
      c.atEnd = false;
      if (saved.follows || saved.row === null || saved.fixed != null) c.restore(saved);
      else {
        const order = saved.order, keys = [order[saved.row], ...order.slice(saved.row + 1), ...order.slice(0, saved.row).reverse()];
        const key = keys.find(k => c.index.pos.has(k));
        if (key === undefined) c.restore(saved);
        else c.intoView(key, 'start', saved.within, false);
      }
      settled();
    }
    if (c.target || c.correction) return { pending: true };
    if (saved.follows) return { target: c.axis === 'x' ? el.scrollWidth - el.clientWidth : el.scrollHeight - el.clientHeight };
    if (saved.row === null && saved.fixed == null) return { target: axis.target };
    const at = c.index.anchorPosition(saved, c.geometry.port_main) ?? 0;
    return { target: el[c.axis === 'x' ? 'scrollLeft' : 'scrollTop'] + at - c.geometry.offset };
  }
  const target = a.nodes.map(n => [n, find(n)]).find(([, n]) => n?.getClientRects().length);
  // Wait for the original anchor before falling back to an ancestor or an offset.
  if (waiting && (!target || target[0] !== a.nodes[0])) return { pending: true };
  if (!target) return { target: axis.target };
  if (listPending(target[0])) return { pending: true };
  const coordinate = a.coordinate === 'x' ? 'left' : 'top', offset = a.coordinate === 'x' ? 'scrollLeft' : 'scrollTop';
  const measured = box(el, target[1]), saved = target[0][a.coordinate];
  return { target: measured && Number.isFinite(saved) ? el[offset] + measured[coordinate] - saved : axis.target };
}

/** Suppress autofocus while records find their targets. A later
 * focus or scroll takes precedence over a record (LLP 1029.000.002 D4–D6). */
export function prepareDev() {
  globalThis.__exactDevRecords = records;
  const cp = dev();
  let scrolls = [...(cp?.scrolls ?? [])].sort((a, b) => (b.path?.length ?? -1) - (a.path?.length ?? -1));
  const scroller = s => s.page ? document.scrollingElement : find(s);
  // A record holding a scroller's top axis turns the author's follower off until `keep` gives it back.
  const holds = (s, el) => !s.page && s.axes.top && find(s) === el;
  const follow = PropHooks.scrollFollowEnd;
  if (follow) PropHooks.scrollFollowEnd = (el, value) => {
    followers.set(el, value === 'true');
    return follow(el, scrolls.some(s => holds(s, el)) ? 'false' : value);
  };
  if (!cp) return () => {};
  const scrollRestoration = history.scrollRestoration;
  if (scrolls.length) history.scrollRestoration = 'manual';
  restartAutofocus();
  const focus = HTMLElement.prototype.focus;
  HTMLElement.prototype.focus = function () {};
  return () => {
    HTMLElement.prototype.focus = focus;
    const report = replacement();
    journal.unshift(`t=${clock.now} replacement generation ${clock.generation}: ${JSON.stringify(report)}`);
    let pendingFocus = cp.focus, frame = null, done = false;
    const until = cp.restoreUntil ?? Date.now() + 10000;
    const reader = new Map(), armed = new Map(), positions = new WeakMap();
    let inputFrame = null, held = false;
    const listPending = record => scrolls.some(s => Object.values(s.axes).some(a => a.anchor?.list
      && (!a.started || scroller(s)?.$list?.target || scroller(s)?.$list?.correction))
      && s.path?.length < record.path?.length && s.path.every((at, i) => at === record.path[i]));
    const schedule = () => { if (!done) frame ??= requestAnimationFrame(restore); };
    const observer = new MutationObserver(schedule);
    const focused = () => { pendingFocus = null; schedule(); };
    const releaseFollow = el => {
      if (!followers.get(el)) return;
      follow(el, 'true');
      // An authored scroll makes the follower remember this position before following resumes. An unrendered
      // scroller has no position: the follower starts at the end and settles there once it is shown.
      if (el.getClientRects().length) PropHooks.scrollTop(el, String(el.scrollTop));
      settled();
    };
    // A record leaves `scrolls`, or loses its top axis, only through here, which gives its scroller back to the
    // author's follower once no remaining record holds it.
    const keep = test => {
      const held = scrolls.filter(s => !s.page && s.axes.top).map(scroller);
      scrolls = scrolls.filter(test);
      for (const el of new Set(held)) if (el && !scrolls.some(s => holds(s, el))) releaseFollow(el);
    };
    // Input arms every pending scroller; only a subsequent movement cancels an
    // axis. Control activation and navigation need no scroll-key classification.
    const inputs = ['wheel', 'keydown', 'touchstart', 'touchmove', 'touchend', 'touchcancel', 'pointerdown', 'pointerup', 'pointercancel'];
    const offsets = { left: 'scrollLeft', top: 'scrollTop' };
    const positionOf = el => Object.fromEntries(Object.entries(offsets).map(([name, property]) => [name, el[property]]));
    const releaseInput = () => {
      if (inputFrame !== null) cancelAnimationFrame(inputFrame);
      inputFrame = null;
      if (held) return;
      // Compositor scrolling can follow the input's first rendering opportunity.
      inputFrame = requestAnimationFrame(() => {
        inputFrame = requestAnimationFrame(() => { inputFrame = null; armed.clear(); });
      });
    };
    const input = e => {
      if (e.type === 'pointerdown' || e.type === 'touchstart') held = true;
      // A middle-click can keep autoscrolling after the button is released.
      if ((e.type === 'pointerup' && e.button !== 1) || ['pointercancel', 'touchend', 'touchcancel'].includes(e.type)) held = false;
      for (const s of scrolls) {
        const el = scroller(s);
        if (el && !armed.has(el)) armed.set(el, positions.get(el) ?? positionOf(el));
      }
      releaseInput();
    };
    const scrolled = e => {
      const el = e.target === document ? document.scrollingElement : e.target;
      const at = armed.get(el);
      if (!at) return;
      for (const [name, property] of Object.entries(offsets)) {
        if (el[property] === at[name]) continue;
        if (!reader.has(el)) reader.set(el, new Set());
        reader.get(el).add(name);
        at[name] = el[property];
        schedule();
      }
      releaseInput();
    };
    const cancelRead = s => {
      const el = scroller(s), moved = reader.get(el);
      if (moved) {
        for (const name of moved) delete s.axes[name];
        reader.delete(el);
      }
      return Object.keys(s.axes).length > 0;
    };
    globalThis.__exactDevRecords = () => {
      if (Date.now() >= until) end();
      keep(cancelRead);
      const current = records();
      if (done) return current;
      // Another save may arrive before a resource. Keep its unclaimed records,
      // without carrying a list controller's progress into the next document.
      for (const s of scrolls) {
        const found = current.scrolls.find(r => r.page === s.page && r.type === s.type && shape(r.path, s.path));
        const axes = Object.fromEntries(Object.entries(s.axes).map(([name, a]) => [name, { target: a.target, ...(a.anchor ? { anchor: a.anchor } : {}) }]));
        if (found) Object.assign(found.axes, axes);
        else current.scrolls.push({ ...s, axes });
      }
      return { ...current, focus: pendingFocus ?? current.focus, restoreUntil: until };
    };
    function end() {
      if (done) return;
      done = true;
      pendingFocus = null;
      keep(() => false);
      reader.clear();
      armed.clear();
      if (inputFrame !== null) cancelAnimationFrame(inputFrame);
      observer.disconnect();
      window.removeEventListener('pagehide', end);
      document.removeEventListener('focusin', focused, true);
      document.removeEventListener('scroll', scrolled, true);
      for (const name of inputs) document.removeEventListener(name, input, true);
      const at = After.indexOf(schedule); if (at >= 0) After.splice(at, 1);
      if (frame !== null) { cancelAnimationFrame(frame); frame = null; }
      clearTimeout(deadline);
      history.scrollRestoration = scrollRestoration;
      offerAll();
    }
    function restoreScroll(s, el, waiting) {
      const targets = Object.entries(s.axes).map(([name, axis]) => [name, axisTarget(el, axis, waiting, listPending)]);
      reader.delete(el);
      el.scrollTo({ ...Object.fromEntries(targets.filter(([, a]) => a.target !== undefined).map(([name, a]) => [name, a.target])), behavior: 'instant' });
      // Include clamping in the baseline so restoration never cancels itself.
      const at = positionOf(el);
      positions.set(el, at);
      if (armed.has(el)) armed.set(el, at);
      for (const [name, a] of targets) s.axes[name].pending = !!(a.pending || waiting && Math.abs(el[offsets[name]] - a.target) > 1);
      return Object.values(s.axes).some(a => a.pending);
    }
    function restore() {
      frame = null;
      const waiting = inflight.n || Resources.some(r => r.waiting || r.pending() && r.ticket && !r.ticket.messages && !r.ticket.closed && !r.ticket.ctl?.refused);
      const remaining = scrolls.length;
      keep(cancelRead);
      keep(s => {
        const el = scroller(s);
        // A saved path through a virtualized window names its row only after that window settles.
        if (listPending(s)) return true;
        if (!el?.getClientRects().length) return !!waiting;
        return restoreScroll(s, el, waiting);
      });
      if (scrolls.length && scrolls.length < remaining) schedule();
      // A path through a virtualized window names a temporary row until its lists finish restoring.
      if (pendingFocus && !listPending(pendingFocus)) {
        const el = find(pendingFocus);
        if (el?.getClientRects().length && !el.matches(':disabled') && !inertAncestor(el) && getComputedStyle(el).visibility === 'visible') {
          const kept = pendingFocus;
          pendingFocus = null;
          focus.call(el, { preventScroll: true });
          if (kept.text === el.value && typeof kept.start === 'number' && typeof el.selectionStart === 'number') el.setSelectionRange(kept.start, kept.end, kept.direction);
        }
      }
      if (!pendingFocus && !scrolls.length) end();
    }
    After.push(schedule);
    observer.observe(document.getElementById('exact-root'), { childList: true, subtree: true });
    window.addEventListener('pagehide', end);
    document.addEventListener('focusin', focused, true);
    document.addEventListener('scroll', scrolled, true);
    for (const name of inputs) document.addEventListener(name, input, { capture: true, passive: true });
    // A missing target or a request that never answers cannot hold browser behavior indefinitely.
    const deadline = setTimeout(end, Math.max(0, until - Date.now()));
    if (Date.now() >= until) end();
    else restore();
  };
}
