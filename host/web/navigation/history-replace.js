// When a commit's new top takes the browser entry of the visit shown rather than pushing one (routes.js `commit`): a
// detail giving way to the next beside its list (LLP 1127), as a web page swaps a split's detail with `replaceState`,
// so history keeps no entry of the visit replaced and ends as the router's stack does. Everything else pushes, as a
// commit did before: what the commit's `visits`, `roots` and `tab` cannot vouch for, a tab switch (a history entry of
// its own on the web, LLP 1038 D12), entries ahead (only a push drops them), and entries or frames this page did not
// write or cannot see.

/** Where `op`'s top stands, which a stamp records: its tab, by name, and the visits beneath it there, none at a root. */
export function placeOf({ top, visits, roots, tab }) {
  const at = visits?.findIndex(([id]) => id === top) ?? -1;
  const root = at < 0 || !roots ? -1 : visits.findLastIndex(([id], i) => i <= at && roots.includes(id));
  return root < 0 ? {} : { tab, beneath: root < at ? visits.slice(root, at).map(([id]) => id).join() : undefined };
}

/** Whether `entry`'s stamp places its visit elsewhere than `op` does: a restored router can mint its id again there. */
export const misplaced = (entry, op) => entry.tab !== placeOf(op).tab || entry.beneath !== placeOf(op).beneath;

/**
 * Whether `op`'s top takes the entry at `cursor` of `written`, this page's mirror of its session history (`nid`, the
 * entry as written: the browser's id for it, or "memory"). `memory`: writes go to memory alone. `traversing`: a
 * traversal the agent asked for has not settled. `canonicalOf`: the router's canonical form of a location.
 */
export function replaces(op, { written, cursor, memory, traversing }, canonicalOf) {
  const place = placeOf(op), shown = written[cursor], beneath = written[cursor - 1];
  // A new visit (none the router held before this change, none an entry holds) directly on its tab's root, as a split's
  // detail stands beside its list, which the route table resolves to that root, at its address and in its tab, and
  // itself (`chain`), so a step back to its address rebuilds the same stack (deeper, an app's own link handler or a
  // nested route can rebuild another); on the root the shown one stood on when its entry was written, whose entry
  // beneath is of that tab too
  if (place.beneath === undefined || place.beneath.includes(",") || op.chain?.length !== 2) return false;
  if (op.held?.has(String(op.top)) !== false || !shown || !beneath) return false;
  if (Object.values(written).some((e) => e.id === op.top)) return false;
  if (shown.tab !== place.tab || beneath.tab !== place.tab || shown.beneath !== place.beneath) return false;
  // The shown visit gone, and the entry beneath its parent's, at that visit's address; the top at an address no visit
  // beneath it in its tab has, which a step back to it would otherwise find there (the router's `go`)
  const visits = op.visits, at = visits.findIndex(([id]) => id === op.top), [parent, address] = visits[at - 1];
  if (visits.some(([id]) => id === shown.id) || parent !== beneath.id) return false;
  if (canonicalOf(beneath.url.split("#")[0]) !== address || op.chain[0][0] !== address || op.chain[0][1] !== place.tab) return false;
  if (visits.slice(at - place.beneath.split(",").length, at).some(([, url]) => url === visits[at][1])) return false;
  // No entry ahead, which only a push drops: this page's, or one the browser lists (a reload left it)
  const index = memory ? null : globalThis.navigation?.currentEntry?.index ?? null;
  const listed = index === null ? null : globalThis.navigation.entries();
  if (Object.keys(written).some((k) => Number(k) > cursor) || listed && index < listed.length - 1) return false;
  // No frame, whose steps share the session history unlisted, and no traversal in flight
  if (globalThis.top !== globalThis || window.length || document.querySelector("iframe")) return false;
  if (globalThis.navigation?.transition || traversing) return false;
  // Only the entries this page wrote, as the browser lists them (another module's would be overwritten)
  if (memory) return shown.nid !== undefined && beneath.nid !== undefined;
  return listed !== null && listed[index]?.id === (shown.nid ?? NaN) && listed[index - 1]?.id === (beneath.nid ?? NaN);
}
