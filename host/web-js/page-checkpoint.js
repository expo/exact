// A rendered page's checkpoint and what its reader did before the runtime ran (LLP 1048.000 D6, LLP 1048.001 D5):
// linked by a page whose plan renders a document (build.mjs), as only such a page carries either.
import { checkpointFaults } from '../web/faults.js';
import { documents } from './mount.js';
import { say } from './rt.js';
import { canonical } from './route-pattern.js';
import { clock, agentMode } from './clock.js';

documents({ read, early });

/** The page's checkpoint: the time the render stopped at, and by resource name the `source` and `answers` its document
 * used, each `[args, value, open, reads]`: one, or a list's elements', where a new element looks first at `next`; or
 * `elsewhere`, and nothing kept, for a page rendered at another location. */
function read() {
  const el = typeof document !== 'undefined' && document.querySelector('script[type="application/vnd.exact.checkpoint"]');
  if (!el) return { kept: null };
  const cp = JSON.parse(el.textContent);
  // Render fresh, seeding nothing, where the browser's path is not the page's once both are canonical, the query aside
  // (D6): a page a static host serves at another path, its `404.html` or its fallback to `/`, is another location's
  // document, and its answers are another location's. Both paths keep a mounted build's base. Repeated and trailing
  // slashes make no other page (`canonical_location`, host/web/src/page.rs): put the page's own path in the address
  // bar, as the server redirects to it (D11), since no route matches a path with an empty segment.
  const at = canonical(cp.location).split('?')[0], one = path => path.split('/').filter(Boolean).join('/');
  if (one(canonical(location.pathname)) !== one(at)) {
    say(`document: rendered at ${cp.location}, not ${location.pathname}; rendering fresh`);
    return { kept: null, elsewhere: true };
  }
  // (its origin too: a path that opens with two slashes would name a host)
  if (canonical(location.pathname) !== at) history.replaceState(history.state, '', location.origin + at + location.search + location.hash);
  const open = new Set(cp.open),
    readers = new Set(cp.readers),
    paths = new Map(cp.instances),
    kept = new Map();
  // A drive starts at zero even when its document was rendered on a wall clock.
  const driven = agentMode() && !globalThis.__exactRender;
  if (driven) for (const k of checkpointFaults(cp)) open.add(k);
  cp.answers.forEach(([name, source, args, value], k) => {
    if (!kept.has(name)) kept.set(name, { source, answers: [], next: 0 });
    kept.get(name).answers.push([value_(args), value_(value), open.has(k), readers.has(k), paths.get(k)]);
  });
  return { kept, revalidate: !!cp.revalidate, time: (clock.now = driven ? 0 : cp.time || 0) };
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
/** What the reader did before the runtime ran (the capture script) and what each edited control showed then, taken
 * before the boot's commit writes the state's values over them; the returned replay runs once the boot stands. */
function early(adoptAt) {
  const taps = globalThis.exact?.taps?.() ?? [];
  const shown = taps.filter(t => t.type !== 'click').map(t => [t.target, t.target.value, t.target.checked]);
  // Replayed once, in order, on the same elements (LLP 1048.001 D5), each edited control first showing what the reader
  // left in it (its row, if it waits for a slice, adopted first, so adoption doesn't write over it). One whose element
  // is gone, with another location's document or an abandoned one, is dropped.
  return () => {
    for (const t of taps) adoptAt(t.target);
    for (const [e, v, c] of shown)
      if (e.isConnected) {
        if (e.type === 'checkbox' || e.type === 'radio') e.checked = c;
        else if (e.type !== 'file') e.value = v;
      }
    for (const t of taps)
      if (!t.target.isConnected) say(`document: an early ${t.type} was dropped (its element is gone)`);
      else if (t.type === 'click') t.target.click();
      else t.target.dispatchEvent(new Event(t.type, { bubbles: true }));
  };
}
