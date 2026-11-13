// Mounting the view: its regions (`when`, `match`, `each`), the adoption of a rendered page, in slices once its budget
// runs out, the boot, and the page's checkpoint.
import { inOrder, rowKey } from './rows.js';
import { reloadOnGone } from './generation-reload.js';
import { viewChildren, listRow, listOwner, unwrapListItem, sheetLeaves } from './navigation.js';
import { head } from './document.js';
import { autofocus } from './focus.js';
import {
  Owner,
  onEnd,
  scope,
  end,
  endAll,
  untracked,
  write,
  writeItem,
  sig,
  effect,
  commit,
  say,
  journal,
  inflight,
  Store,
  After,
  Paint,
  presentArm,
  presentRows,
  Why,
} from './rt.js';
import { clock, agentMode, AGENT_ADMITTED } from './clock.js';
import { at, mark, Mismatch } from './tree.js';
import { Leave, exiting } from './pieces.js';
import { Resources } from './resources.js';
import { conforms } from './shape.js';
// `Booting`: while the boot builds the view (`mount`); `start`: the page's time when it mounted, from which its wall
// clock runs.
export let Booting = false,
  start = 0;
export const booting = () => Booting;
export let Adopt = false,
  // The parents whose rows a keyed list adopted: one holding rows past the list's (a page built ahead, opened at a
  // query it was not built at) is no projection of this plan either (`mount`).
  AdoptedLists = new Set();
/** Build fresh inside an adopted page (a virtualized list's rows). */
export function unadopted(f) {
  const a = Adopt;
  Adopt = false;
  try {
    return f();
  } finally {
    Adopt = a;
  }
}
/** Whether construction is adopting a rendered page. */
export const adopting = () => Adopt;
/** Adopt a rendered row under `w` (a virtualized list's, LLP 1048.001):
 * one that isn't this plan's projection is built afresh, alone. */
export function adoptRow(w, f) {
  const a = Adopt;
  Adopt = true;
  try {
    return f();
  } catch (e) {
    if (!(e instanceof Mismatch)) throw e;
    say(`list: a row built afresh: ${e.message}`);
  } finally {
    Adopt = a;
  }
  w.textContent = '';
  w.$n = undefined;
  return unadopted(f);
}
/** A `head` (LLP 1048.003 D1): its node, as the kernel keeps it, an inert
 * element in the tree, and its fields, the page's while it is the innermost
 * active head (document.js, as runner/src/head.rs). */
export function hd(p, fields) {
  const t = document.createElement('template');
  t.$exactTree = true;
  if (Adopt) p.insertBefore(t, at(p));
  else p.append(t);
  onEnd(head(t, fields, effect, After));
}
function range(p) {
  if (Adopt) return [mark(p), null];
  const a = document.createComment(''),
    b = document.createComment('');
  p.append(a, b);
  return [a, b];
}
function leave(n) {
  const shell = n.nodeType === 1 && n.hasAttribute('data-exact-list-item') ? n : null;
  n = shell ? listRow(shell) : n;
  if (n && !exiting(n)) Leave ? Leave(n) : sheetLeaves(n) || n.remove();
  if (shell?.parentNode) unwrapListItem(shell);
}
function clear(a, b) {
  Paint?.list(a.parentNode);
  for (let n = a.nextSibling; n !== b;) {
    const m = n.nextSibling;
    leave(n);
    n = m;
  }
}
function build(b, f, own) {
  const frag = document.createDocumentFragment();
  frag.$listOwner = listOwner(b.parentNode);
  const s = scope(() => f(frag), own);
  b.before(frag);
  Paint?.list(b.parentNode);
  return s;
}
/** A region's first arm while adopting: built in place, then its end anchor. */
function adoptArm(p, f, own) {
  const s = f ? scope(() => f(p), own) : null;
  return [s, mark(p)];
}
/** `when`: arm 0 while the subject holds, else arm 1 (or nothing). */
export function when(p, subject, a0, a1) {
  let [a, b] = range(p),
    own = Owner;
  let arm = -1,
    s = null;
  effect(() => {
    const want = subject() ? 0 : a1 ? 1 : -1;
    if (want === arm) return;
    arm = want;
    const view = presentArm(want < 0 ? null : want ? a1 : a0, subject, want);
    if (!b)
      return untracked(() => {
        [s, b] = adoptArm(p, view, own);
      });
    untracked(() => {
      if (s) end(s);
      clear(a, b);
      s = view ? build(b, view, own) : null;
    });
  }).$r = () => [a, b]; // the region's range, for shared.js
}
/** `match`: arm 0 with the bound value while the subject is `some`, else arm 1. */
export function match(p, subject, a0, a1) {
  let [a, b] = range(p),
    own = Owner;
  let arm = -1,
    s = null;
  const bound = sig(null);
  effect(() => {
    const v = subject(),
      want = v != null ? 0 : a1 ? 1 : -1;
    if (v != null) write(bound.n, v);
    if (want === arm) return;
    arm = want;
    const view = presentArm(want < 0 ? null : want ? a1 : p2 => a0(p2, bound), subject, want, bound);
    if (!b)
      return untracked(() => {
        [s, b] = adoptArm(p, view, own);
      });
    untracked(() => {
      if (s) end(s);
      clear(a, b);
      s = view ? build(b, view, own) : null;
    });
  }).$r = () => [a, b]; // the region's range, for shared.js
}
/** `each`: rows by key in item order; a kept row keeps its elements, its
 * item and position are signals its bindings read. */
export function each(p, list, key, row, pure) {
  row = presentRows(row, list, key);
  let [a, b] = range(p),
    own = Owner;
  let rows = new Map(),
    single = false,
    order = null;
  effect(() => {
    const items = list(),
      // rows go where the end anchor is: at an arm's top, `p` is the fragment the arm was built in
      parent = b?.parentNode ?? p;
    // A key that reads more than its item and index (a slot) is read tracked, re-keying the rows as the runner's; a
    // pure one only where needed.
    const keys = pure
        ? null
        : items.map((item, i) =>
            key(
              () => item,
              () => i,
            ),
          ),
      keyAt = i =>
        keys
          ? keys[i]
          : key(
              () => items[i],
              () => i,
            );
    untracked(() => {
      // Conditional arms leave their build fragment.
      if (b) p = a.parentNode;
      // Rows moving or leaving are adopted rows (a row waiting for its slice
      // shows its rendered values until then, and adopts at the current ones).
      if (b && LazyAt < Lazy.length) adoptAll();
      // Keys that didn't move: new items to their rows, nothing else (LLP 1071.000 D2).
      if (pure && order && items.length === order.length) {
        let i = 0;
        for (; i < items.length; i++) {
          const item = items[i],
            r = order[i];
          if (r.item.n.v === item) continue;
          const k = keyAt(i);
          if (rowKey(k) !== r.k) break;
          writeItem(r.item.n, item);
        }
        if (i === items.length) return;
      }
      // A row's place in the last pass is its `at`; repeats count once a key repeats.
      const next = new Map(),
        seen = new Map();
      items.forEach((item, i) => {
        const k = rowKey(keyAt(i), next, seen);
        if (k.startsWith('d')) journal.push(`each: repeated key ${k}`);
        let r = rows.get(k);
        if (r) {
          rows.delete(k);
          r.old = r.at;
          if (!Object.is(r.item.n.v, item)) writeItem(r.item.n, item);
          if (r.index.n.v !== i) write(r.index.n, i);
        } else if (!b) {
          // Adopting: the row's elements are in place, in item order.
          r = { k, item: sig(item), index: sig(i) };
          AdoptedLists.add(p);
          if (single && i && performance.now() > AdoptBy) {
            // Past the adoption's budget, a row of one element waits for a
            // slice (`lazy`); a press on it or any commit adopts it first.
            let e = at(p);
            while (e && e.nodeType !== 1) e = e.nextSibling;
            if (!e) throw new Mismatch(`adoption: expected a row, found nothing`);
            p.$n = e.nextSibling;
            r.start = r.end = e;
            lazy([p, r, row, own]);
          } else {
            const next = at(p),
              last = next ? next.previousSibling : p.lastChild;
            r.s = scope(() => row(p, r.item, r.index, r.k), own);
            // A row of one element is that element, as a fresh build keeps it
            // (a region at its top would have put its own anchors beside it).
            const first = last ? last.nextSibling : p.firstChild;
            if (first && first.nodeType === 1 && first.nextSibling === at(p)) r.start = r.end = first;
            else {
              r.start = document.createComment('');
              p.insertBefore(r.start, first ?? at(p));
              r.end = mark(p);
            }
            if (!i) single = r.start.nodeType === 1;
          }
        } else {
          r = { k, item: sig(item), index: sig(i) };
          const frag = document.createDocumentFragment();
          frag.$listOwner = listOwner(parent);
          r.s = scope(() => row(frag, r.item, r.index, r.k), own);
          // A row of one element is that element (a region at its top would
          // have put its own anchors beside it); any other is bracketed.
          if (frag.childNodes.length === 1 && frag.firstChild.nodeType === 1) r.start = r.end = frag.firstChild;
          else {
            r.start = document.createComment('');
            r.end = document.createComment('');
            frag.prepend(r.start);
            frag.append(r.end);
          }
          r.frag = frag;
        }
        r.k = k;
        next.set(k, r);
      });
      const list = (order = [...next.values()]);
      for (let i = 0; i < list.length; i++) list[i].at = i;
      // Every row goes and the region is all its parent holds: emptied at once, but not where the parent holds a modal
      // route, which may leave (navigation.js `sheetLeaves`).
      if (
        rows.size &&
        b &&
        !Leave &&
        list.every(r => r.frag) &&
        !a.previousSibling &&
        !b.nextSibling &&
        !viewChildren(parent).some(r => r.getAttribute('navigationPresentation') === 'modal')
      ) {
        endAll(rows);
        parent.textContent = '';
        parent.append(a, b);
      } else if (rows.size) {
        endAll(rows);
        for (const r of rows.values()) {
          let n = r.start;
          while (n) {
            const m = n.nextSibling;
            leave(n);
            if (n === r.end) break;
            n = m;
          }
        }
      }
      // Order, from the last row back: kept rows on the longest run already in
      // order stay; any other moves before the row after it; new rows go in
      // one fragment per run.
      if (b) {
        const stay = inOrder(list);
        let anchor = b,
          batch = null,
          first = null;
        const flush = () => {
          if (batch) {
            parent.insertBefore(batch, anchor);
            anchor = first;
            batch = null;
          }
        };
        for (let i = list.length - 1; i >= 0; i--) {
          const r = list[i];
          if (r.frag) {
            if (batch) batch.prepend(r.frag);
            else batch = r.frag;
            first = r.start;
            r.frag = null;
            continue;
          }
          flush();
          if (!stay.has(i)) {
            const f = document.createDocumentFragment();
            let n = r.start;
            while (n) {
              const m = n.nextSibling;
              f.append(n);
              if (n === r.end) break;
              n = m;
            }
            parent.insertBefore(f, anchor);
          }
          anchor = r.start;
        }
        flush();
      }
      rows = next;
      Paint?.list(parent);
      b ??= mark(p);
    });
  }).$r = () => [a, b]; // the region's range, for shared.js
}
// Adoption in slices (LLP 1048.001): a keyed list's rows adopt in the
// adoption's own task until its budget runs out, then in tasks of a few
// milliseconds after it, so no one task holds the page. A press, key or
// edit on a row still waiting adopts that row before the event reaches it;
// a list that changes adopts every row still waiting first. A waiting row
// shows its rendered values, and its bindings take the current ones when it
// adopts (a structure that no longer matches builds that row afresh).
const ADOPT_MS = 16,
  SLICE_MS = 8;
let AdoptBy = Infinity;
const Lazy = [],
  LazyRows = new Map();
let LazyAt = 0,
  LazyTask = null;
const LAZY_EVENTS = [
  'pointerdown',
  'mousedown',
  'touchstart',
  'click',
  'keydown',
  'beforeinput',
  'input',
  'change',
  'focusin',
];
// Passive: a waiting row must never make the page's touches wait for script.
const LAZY_OPTS = { capture: true, passive: true };
function lazy(x) {
  Paint?.wait(x[1].start);
  Lazy.push(x);
  LazyRows.set(x[1].start, x);
}
function adoptLazy(x) {
  const [p, r, row, own] = x;
  LazyRows.delete(r.start);
  // A row whose list left (its region's arm ended) has nothing to adopt.
  if (r.s || own?.gone || !r.start.isConnected) return;
  const save = p.$n,
    a = Adopt;
  p.$n = r.start;
  Adopt = true;
  try {
    r.s = scope(() => row(p, r.item, r.index, r.k), own);
  } catch (e) {
    if (!(e instanceof Mismatch)) throw e;
    say(`each: a row built afresh: ${e.message}`);
    Adopt = false;
    const frag = document.createDocumentFragment(),
      old = r.start;
    frag.$listOwner = listOwner(p);
    r.s = scope(() => row(frag, r.item, r.index, r.k), own);
    if (frag.childNodes.length === 1 && frag.firstChild.nodeType === 1) r.start = r.end = frag.firstChild;
    else {
      r.start = document.createComment('');
      r.end = document.createComment('');
      frag.prepend(r.start);
      frag.append(r.end);
    }
    old.replaceWith(frag);
  } finally {
    Adopt = a;
    p.$n = save;
  }
}
function adoptAll() {
  while (LazyAt < Lazy.length) adoptLazy(Lazy[LazyAt++]);
  lazyDone();
}
function lazyDone() {
  if (LazyAt < Lazy.length || !Lazy.length) return;
  Lazy.length = LazyAt = 0;
  LazyRows.clear();
  inflight.n--;
  const root = document.getElementById('exact-root');
  for (const t of LAZY_EVENTS) root?.removeEventListener(t, onLazy, LAZY_OPTS);
}
function adoptAt(target) {
  for (let n = target; n && n.nodeType === 1; n = n.parentNode) {
    const x = LazyRows.get(n);
    if (x) {
      adoptLazy(x);
      Paint?.flush();
      break;
    }
  }
}
const onLazy = ev => adoptAt(ev.target);
function slice() {
  LazyTask = null;
  const end = performance.now() + SLICE_MS;
  while (LazyAt < Lazy.length && performance.now() < end) adoptLazy(Lazy[LazyAt++]);
  Paint?.flush();
  if (LazyAt < Lazy.length) LazyTask = post(slice);
  else lazyDone();
}
const post = f =>
  globalThis.scheduler?.postTask ? scheduler.postTask(f, { priority: 'user-visible' }) : setTimeout(f);
// ---------------------------------------------------------------- boot
/** Build the view into `#exact-root` and start the clock. */
export function mount(f) {
  const root = document.getElementById('exact-root');
  // A press on a `retainFocus` node keeps the focus where it is (an
  // editor's), as the web host's glue does: the browser's focus move on
  // pointerdown is prevented, unless the contact is on an editor.
  root?.addEventListener('pointerdown', ev => {
    const t = ev.target;
    if (!(t.closest?.('input, textarea, select') || t.isContentEditable) && t.closest?.('[retainFocus="true"]'))
      ev.preventDefault();
  });
  // Under the agent, and in a render, the clock is the driver's: no timer runs by itself.
  clock.agent = agentMode();
  // (a build kept no more: the page again, once)
  if (!clock.agent && typeof window !== 'undefined') reloadOnGone(window);
  Store.load();
  // Ask again each page answer that read the store when the device's store holds the app's own entries, which the
  // render never had (LLP 1048.000 D6); `res` does so for one made later. A dev reload's carried answers have no page
  // checkpoint, so this rule does not apply to them, as in the runner's carried boot.
  if (Store.map.size && checkpoint().kept) {
    for (const r of Resources) {
      if (r.store) {
        r.baked = true;
      }
    }
  }
  let built = false;
  Adopt = !!(checkpoint().kept && root.firstElementChild);
  // Elapsed time continues from where a render's clock stopped.
  start = clock.start = performance.now() - clock.now;
  const adopting = Adopt;
  // What the reader did before the runtime ran (the capture script), and
  // what each edited control showed then: the commit writes the state's
  // values, which the reader had changed.
  const early = globalThis.exact?.taps?.() ?? [];
  const shown = early.filter(t => t.type !== 'click').map(t => [t.target, t.target.value, t.target.checked]);
  AdoptBy = performance.now() + ADOPT_MS;
  Booting = true;
  let booted = commit(
    () => {
      scope(() => f(root));
      for (const p of AdoptedLists) {
        let n = at(p);
        while (n && n.nodeType !== 1) n = n.nextSibling;
        if (n) throw new Mismatch("adoption: a list's document holds more rows than its items");
      }
      built = true;
    },
    adopting ? 'adopt' : 'boot',
  );
  AdoptedLists.clear();
  Adopt = false;
  AdoptBy = Infinity;
  if (!built) {
    Lazy.length = LazyAt = 0;
    LazyRows.clear();
  }
  const adopted = adopting && built;
  if (!built && adopting) {
    // The document isn't this plan's projection: build afresh (and say so).
    say(`adoption abandoned ${Why}`);
    root.textContent = '';
    booted = commit(() => {
      scope(() => f(root));
      built = true;
    }, 'boot');
  }
  // A boot whose settlement refused (a derive outside its type) is refused whole, as `Runner::boot` fails: nothing
  // shows.
  Booting = false;
  if (!built || booted === false) {
    root.textContent = '';
    throw new Error(`boot refused ${Why}`);
  }
  // the runner's journal line (LLP 1012 logs)
  say(`boot: ${root.getElementsByTagName('*').length} nodes, epoch ${clock.epoch}`);
  if (adopted) say('adopted the document');
  autofocus(root); // the document's autofocus (LLP 1035.000 D9), then each commit's mounts (`commit`, focus.js)
  // Rows waiting are in flight, for the agent's `clock settle`.
  if (Lazy.length) {
    inflight.n++;
    for (const t of LAZY_EVENTS) root.addEventListener(t, onLazy, LAZY_OPTS);
    LazyTask = post(slice);
  }
  root.dataset.bootMs = String(Math.round(performance.now()));
  // Replayed once, in order, on the same elements (LLP 1048.001 D5), each
  // edited control first showing what the reader left in it (its row, if it
  // waits for a slice, adopted first, so adoption doesn't write over it).
  for (const t of early) adoptAt(t.target);
  for (const [e, v, c] of shown)
    if (e.isConnected) {
      if (e.type === 'checkbox' || e.type === 'radio') e.checked = c;
      else if (e.type !== 'file') e.value = v;
    }
  for (const t of early)
    if (t.target.isConnected)
      t.type === 'click' ? t.target.click() : t.target.dispatchEvent(new Event(t.type, { bubbles: true }));
}
/** The page's checkpoint (LLP 1048.000 D6): the time the render stopped at, and by resource name the `source` and
 * `answers` its document used, each `[args, value, open, reads]`: one, or a list's elements', where a new element
 * looks first at `next`. */
export let Checkpoint = null,
  // A document's answer from the reader's own device was the render's: `res` asks it again, as a bake's
  // (LLP 1048.007 D2).
  READER = new Set(['exactViewport', 'exactPage']);
export function checkpoint() {
  if (Checkpoint) return Checkpoint;
  Checkpoint = { kept: null };
  const el =
    typeof document !== 'undefined' && document.querySelector('script[type="application/vnd.exact.checkpoint"]');
  if (!el) return Checkpoint;
  const cp = JSON.parse(el.textContent),
    open = new Set(cp.open),
    readers = new Set(cp.readers);
  Checkpoint.revalidate = !!cp.revalidate;
  Checkpoint.kept = new Map();
  cp.answers.forEach(([name, source, args, value], k) => {
    if (!Checkpoint.kept.has(name)) {
      Checkpoint.kept.set(name, { source, answers: [], next: 0 });
    }
    Checkpoint.kept.get(name).answers.push([value_(args), value_(value), open.has(k), readers.has(k)]);
  });
  // A drive starts at zero even when its document was rendered on a wall clock.
  const driven =
    AGENT_ADMITTED &&
    typeof location !== 'undefined' &&
    new URLSearchParams(location.search).has('agent') &&
    !globalThis.__exactRender;
  Checkpoint.time = clock.now = driven ? 0 : cp.time || 0;
  return Checkpoint;
}
/** A declared resource's page answer when it names `source` and fits `type` (a list's element's: each.js
 * `keptMember`). */
export function keptAnswer(name, source, type) {
  const kept = checkpoint().kept?.get(name),
    answer = kept?.source === source ? kept.answers[0] : undefined;
  return answer && (!type || conforms(answer[1], type)) ? answer : undefined;
}
/** A checkpoint value (`push_value`, host/web/src/page.rs) as a runtime value: lists and records are arrays, unit and
 * `none` null, `some(v)` v. */
const value_ = v =>
  v === null || typeof v !== 'object'
    ? v
    : Array.isArray(v)
      ? v.map(value_)
      : 'r' in v
        ? v.r.map(value_)
        : 's' in v
          ? value_(v.s)
          : 'n' in v
            ? Number(v.n)
            : null;
