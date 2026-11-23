// Placement by `portal` on the JS target (LLP 1013.002 D2, D6), imported only by a plan that says it. The element
// stays its declaration's: built there, owned by its scope, ended with it. A comment beside it marks the declaration
// (its anchor), and after each flush (`settled`, rt.js) the same element moves to the element its `portal` names, as
// the runner's `instance/portal.rs` decides: the first with that id in declaration order within the app root, a plain
// box that is neither the mover nor inside it, on no cycle and not inside hidden content. One that has nowhere to go
// waits in a connected, hidden, inert holder. A move is `moveBefore()` where the browser has it, which keeps every
// browser-owned state, else `insertBefore` within the same task, which HTML's media removal check lets a playing video
// survive (https://html.spec.whatwg.org/multipage/media.html#playing-the-media-resource) but which an iframe reloads
// through and focus leaves.
import { onEnd, untracked, say } from './rt.js';
import { usePortals, adopting } from './mount.js';
import { Mismatch } from './tree.js';

const Movers = new Set();
let Holder = null;
// One journal line a mover at a time, so a value typed into `portal` keeps nothing per keystroke.
const note = (m, line) => {
  if (m.said !== line) (m.said = line), say(line);
};
// D3, as portal.rs decides it. The emitter marks a plan's boxes and `foreignObject`s (`data-exact-view`).
const view = e => e?.nodeType === 1 && e.hasAttribute('data-exact-view');
const PLAIN = new Set(['div', 'main', 'section', 'article', 'aside', 'nav', 'footer']);
const PARTS = new Set(['table', 'thead', 'tbody', 'tfoot', 'tr', 'th', 'td', 'caption', 'dl', 'dt', 'dd', 'ul', 'ol', 'li']);
// A route's header projects its children into the bar, a tablist its tabs.
const slots = p =>
  (p.localName === 'header' && !!p.parentElement?.hasAttribute('navigationKey')) || p.getAttribute('role') === 'tablist';
/** Its children are ordinary layout children: a box, not a list, a part's container or a slot holder. */
const layout = p => view(p) && !PARTS.has(p.localName) && !p.hasAttribute('data-exact-list') && !slots(p);
const receives = e =>
  view(e) &&
  PLAIN.has(e.localName) &&
  !e.getAttribute('role') &&
  !e.hasAttribute('popover') &&
  !e.hasAttribute('navigationKey') &&
  (up(e)?.id === 'exact-root' || layout(up(e)));
const PROJECTED = new Set(['dialog', 'alertdialog', 'tab', 'tabpanel', 'tablist']);
const movable = m =>
  layout(m.anchor.parentNode) &&
  !PARTS.has(m.e.localName) &&
  !PROJECTED.has(m.e.getAttribute('role')) &&
  !m.e.hasAttribute('popover') &&
  !m.e.hasAttribute('navigationKey');

/** The declaration parent: a mover's is its anchor's. */
const up = n => (n.$portal ? n.$portal.anchor.parentNode : n.parentNode);
const root = () => document.getElementById('exact-root');
function chain(n, top) {
  const c = [];
  for (; n && n !== top; n = up(n)) c.push(n);
  return c.reverse();
}
/** Whether `a` comes before `b` in declaration preorder: below their nearest common declaration ancestor, the two
 * children's places, a mover's at its anchor. */
function before(a, b, top) {
  const ca = chain(a, top), cb = chain(b, top);
  let k = 0;
  while (k < ca.length && k < cb.length && ca[k] === cb[k]) k++;
  if (k === ca.length) return k < cb.length;
  if (k === cb.length) return false;
  const x = ca[k].$portal?.anchor ?? ca[k], y = cb[k].$portal?.anchor ?? cb[k];
  return !!(x.compareDocumentPosition(y) & Node.DOCUMENT_POSITION_FOLLOWING);
}
const inside = (n, of, top) => chain(n, top).includes(of);

function holder() {
  if (Holder?.isConnected) return Holder;
  Holder = document.createElement('div');
  // Not `hidden`, which lets a browser pause its media: unpainted and out of reach.
  Holder.inert = true;
  Holder.style.display = 'none';
  Holder.dataset.exactPortals = '';
  document.body.append(Holder);
  return Holder;
}
// The focus a move took (a fallback move, or one into the holder), given back once placement settles (D5).
let Refocus = null;
/** Move `n` before `ref` under `parent`, keeping its state where the browser can. */
function move(parent, n, ref) {
  if (n.contains(document.activeElement)) Refocus = document.activeElement;
  if (parent.moveBefore && n.isConnected && parent.isConnected && parent.ownerDocument === n.ownerDocument)
    try {
      return parent.moveBefore(n, ref);
    } catch {}
  parent.insertBefore(n, ref);
}

/** Element `e`, just built at its declaration, is shown where `value()` names. */
export function portal(e, value) {
  // A page rendered ahead leaves the element out, and its reader's page builds afresh: adopting a placed element is
  // LLP 1013.002 D7's, not yet built.
  if (globalThis.__exactRender) return void e.remove();
  if (adopting()) throw new Mismatch('a rendered page with a `portal` is built afresh');
  const anchor = document.createComment('portal');
  e.parentNode.insertBefore(anchor, e);
  const m = { e, anchor, value };
  e.$portal = m;
  Movers.add(m);
  // Its declaration ended: the element goes from wherever it is shown, after any other mover shown inside it is
  // rescued. One shown at its declaration leaves with the region around it (an exit animation included).
  onEnd(() => {
    Movers.delete(m);
    if (e.previousSibling !== anchor) {
      rescue(e);
      e.remove();
    }
  });
}

// D1: an event in a moved element bubbles through its declaration. Where a handler waits on either path, the event
// stops at the element and a copy goes on from its declaration parent; the browser's default action is the original's.
// A pointer's down walks `path` instead (pointer.js).
// Focus events are not copied: focus stays a fact of the shown element.
const BUBBLING = ['click', 'dblclick', 'contextmenu', 'keydown', 'keyup', 'beforeinput', 'input', 'change', 'wheel', 'copy',
  'cut', 'paste'];
/** An event's path from its target through declarations. */
function path(ev) {
  const out = [];
  for (let n = ev.target; n; n = up(n)) out.push(n);
  return out;
}
const heard = (n, top) => {
  for (; n && n !== top; n = up(n)) if (n.nodeType === 1 && n.hasAttribute('data-exact-on')) return true;
  return false;
};
// Listened for after the element's own handlers (the first placement pass), so one that stops the event ends it here.
function redirect(m) {
  m.heard = true;
  for (const type of BUBBLING)
    m.e.addEventListener(type, ev => {
      const from = m.anchor.parentNode, top = root();
      if (ev.cancelBubble || ev.$stopped || !from || m.e.parentNode === from) return;
      let shown = false;
      for (let n = m.e.parentNode; n && n !== top; n = n.parentNode)
        if (n.nodeType === 1 && n.hasAttribute('data-exact-on')) shown = true;
      if (!shown && !heard(from, top)) return;
      ev.stopPropagation();
      const copy = new ev.constructor(ev.type, ev);
      if (ev.defaultPrevented) copy.preventDefault();
      if (!from.dispatchEvent(copy)) ev.preventDefault();
    });
}

/** Before `n` leaves the document, each live mover shown inside it waits in the holder, so the removal does not take
 * it along (mount.js `leave`, `each`). */
function rescue(n) {
  for (const m of Movers) if (n.contains(m.e)) move(holder(), m.e, null);
}

/** A row a keyed reorder moves: node by node where it shows a mover, which a detached fragment would disconnect. */
function moveRange(parent, start, end, ref) {
  const nodes = [];
  for (let n = start; n; n = n.nextSibling) {
    nodes.push(n);
    if (n === end) break;
  }
  if (![...Movers].some(m => nodes.some(n => n.contains(m.e)))) {
    const f = document.createDocumentFragment();
    f.append(...nodes);
    return parent.insertBefore(f, ref);
  }
  for (const n of nodes) move(parent, n, ref);
}

const WHY = {
  missing: 'no element in its app root has that id',
  inside: 'that element is this one or inside it',
  refuses: 'that element cannot receive content (a plain box with no role, navigationKey or popover, not a slot of its parent, can)',
  cycle: 'the placements make a cycle',
  unreachable: 'that element is itself hidden',
};

/** The placement pass, after each flush. */
function place() {
  const top = root();
  if (Movers.size && top) placeAll(top);
  // D5: the focus a move took returns once, to an element still shown.
  const f = Refocus;
  Refocus = null;
  if (f?.isConnected && top?.contains(f) && document.activeElement !== f) f.focus({ preventScroll: true });
}
function placeAll(top) {
  const list = [...Movers].filter(m => m.anchor.parentNode);
  list.sort((a, b) => (a === b ? 0 : before(a.anchor, b.anchor, top) ? -1 : 1));
  const first = new Map();
  const dest = (m, id) => {
    if (first.has(id)) return first.get(id);
    const all = [...document.querySelectorAll(`#${CSS.escape(id)}`)].filter(
      e => (top.contains(e) || Holder?.contains(e)) && !e.closest('[data-exiting]'),
    );
    all.sort((a, b) => (a === b ? 0 : before(a, b, top) ? -1 : 1));
    if (all.length > 1) note(m, `id "${id}" names more than one element; \`portal\` takes the first in declaration order`);
    first.set(id, all[0] ?? null);
    return all[0] ?? null;
  };
  // D2.2: each mover's edge, or why it is hidden.
  const edge = new Map();
  for (const m of list) {
    if (!m.heard) redirect(m);
    const v = untracked(m.value);
    m.v = typeof v === 'string' ? v : '';
    if (!m.v) continue;
    if (!movable(m)) {
      note(m, `portal: ignored: this element is a slot of its parent, a route, a dialog or a popover (LLP 1013.002 D3)`);
      continue;
    }
    const d = dest(m, m.v);
    edge.set(m, !d ? 'missing' : inside(d, m.e, top) ? 'inside' : !receives(d) ? 'refuses' : d);
  }
  // The shown parent under the edges kept so far; null in hidden content.
  const shownParent = n => {
    const m = n.$portal;
    if (m && edge.has(m)) {
      const to = edge.get(m);
      return typeof to === 'string' ? null : to;
    }
    if (m) return m.anchor.parentNode;
    return n.parentNode === Holder ? null : n.parentNode;
  };
  // Where the walk up from `n` ends: the root, null in hidden content, or the mover it meets again (a cycle), with the
  // movers whose edges it crossed.
  const climb = (n, from) => {
    const seen = new Set(), crossed = [];
    for (let at = n; at; at = shownParent(at)) {
      if (at === top || at === from) return { end: at, crossed };
      if (seen.has(at)) return { end: at, crossed };
      seen.add(at);
      if (at.$portal && typeof edge.get(at.$portal) === 'object') crossed.push(at.$portal);
    }
    return { end: null, crossed };
  };
  // Drop every edge on a cycle, then each into hidden content, until none changes.
  for (;;) {
    const dropped = new Map();
    for (const [m, to] of edge) {
      if (typeof to === 'string') continue;
      const { end, crossed } = climb(to, m.e);
      if (end === m.e) for (const p of [m, ...crossed]) dropped.set(p, 'cycle');
    }
    if (!dropped.size)
      for (const [m, to] of edge) if (typeof to !== 'string' && climb(to).end !== top) dropped.set(m, 'unreachable');
    if (!dropped.size) break;
    for (const [m, why] of dropped) edge.set(m, why);
  }
  // Where each goes: home after its anchor, after its destination's own children in declaration order, or the holder.
  const into = new Map();
  for (const m of list) {
    const to = edge.get(m);
    if (typeof to === 'string') note(m, `portal="${m.v}" hides it: ${WHY[to]} (LLP 1013.002 D2)`);
    const p = to === undefined ? m.anchor.parentNode : typeof to === 'string' ? holder() : to;
    m.to = p;
    if (to !== undefined) {
      if (!into.has(p)) into.set(p, []);
      into.get(p).push(m.e);
    }
  }
  // A mover whose destination is shown inside it lets go of the mover between them first (the final placement has no
  // cycle, so that one leaves); otherwise each moves straight to its place.
  for (const m of list)
    while (m.e !== m.to && m.e.contains(m.to)) {
      let x = m.to;
      for (let n = m.to; n !== m.e; n = n.parentNode) if (n.$portal) x = n;
      if (!x.$portal) break;
      move(holder(), x, null);
    }
  for (const m of list)
    if (edge.get(m) === undefined && m.e.previousSibling !== m.anchor) move(m.to, m.e, m.anchor.nextSibling);
  for (const [p, es] of into) {
    let ref = null;
    for (let i = es.length - 1; i >= 0; i--) {
      if (es[i].parentNode !== p || es[i].nextSibling !== ref) move(p, es[i], ref);
      ref = es[i];
    }
  }
}

usePortals({ place, rescue, moveRange, path });
