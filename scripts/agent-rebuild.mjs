import { named } from './agent-check.mjs';

/**
 * The screen a test ended on against a fresh build of it (`boxes`, `rebuild`): each shown node's box where the
 * steps left it and where the same state, booted again with every view built new, puts it. A box that differs by a
 * point or more is layout the host kept from what happened before (a fold and back, a swipe let go), which a
 * person sees until something re-renders. The restart keeps the app's state but not a component's own `state`, so
 * the boxes are compared only when the tree it builds is the one the steps left (each node's type, props and focus);
 * otherwise `unkept` names the first node that differs. A screen that does not settle, a plan that does not
 * apply again (a request it must wait for), or a keyboard the fresh build does not raise or lower as the steps left
 * it, is `skipped`, with the reason. Returns `{ differences, unkept, skipped }`,
 * the outermost box first.
 */
export async function freshBuildDifferences(s) {
  const read = async () => {
    if ((await s.clock('settle'))?.settled === false) return null;
    const [{ boxes, error }, tree] = await Promise.all([s.op({ op: 'boxes' }), s.op({ op: 'tree' })]);
    if (error) throw new Error(error);
    if (tree?.error) throw new Error(`tree: ${tree.error}`);
    return placed(tree, boxes);
  };
  // The docked keyboard is the host's, as focus is, and the screen's layout follows it (LLP 1127.000 D9)
  const keys = async () => (await s.op({ op: 'state' }))?.keyboard?.visible === true;
  const before = await read();
  if (!before) return { differences: [], skipped: 'the screen the test ended on did not settle' };
  const keysBefore = await keys();
  const r = await s.op({ op: 'rebuild' });
  if (r?.error) return { differences: [], skipped: r.error };
  // What the fresh build sets off after it settles (the keyboard coming back for the focus it kept) is read
  // again until two reads agree; a box the host kept from before stays where it is
  let after = await read();
  for (let tries = 0; after && tries < 8; tries++) {
    if (treeDifference(before.nodes, after.nodes) || !frameDifferences(before, after).length) break;
    await new Promise((done) => setTimeout(done, 100));
    const next = await read();
    if (next && !treeDifference(after.nodes, next.nodes) && !frameDifferences(after, next).length) break;
    after = next;
  }
  if (!after) return { differences: [], skipped: 'the fresh build did not settle' };
  // A fresh build raises the keyboard again only for a focus it can give back (a bar's search field opens on a tap)
  if (keysBefore !== await keys()) return { differences: [], skipped: `the keyboard ${keysBefore ? 'up' : 'down'} when the steps ended is not ${keysBefore ? 'up' : 'down'} in the fresh build` };
  const unkept = treeDifference(before.nodes, after.nodes);
  return { differences: unkept ? [] : frameDifferences(before, after), unkept };
}

/**
 * Each node's place in the tree (its type and index under its parent, from the root), as a rebuilt tree may number
 * its nodes anew: `nodes`, each place's type, props and focus; `boxes`, each shown node's box by its place; and
 * `byId`, the tree's nodes, which name a node in a line.
 */
export function placed(tree, frames) {
  const nodes = tree.nodes ?? [], keys = new Map(), counts = new Map();
  for (const n of nodes) { // preorder: a parent's key is made before its children's
    const index = counts.get(n.parent) ?? 0;
    counts.set(n.parent, index + 1);
    keys.set(n.id, `${keys.get(n.parent) ?? ''}/${n.type}${index}`);
  }
  return {
    nodes: new Map(nodes.map((n) => [keys.get(n.id), JSON.stringify([n.props ?? {}, n.focused === true])])),
    boxes: new Map(nodes.filter((n) => frames[n.id]).map((n) => [keys.get(n.id), { node: n, box: frames[n.id], parent: keys.get(n.parent) }])),
    byId: new Map(nodes.map((n) => [n.id, n])),
  };
}

/** The first place whose node is missing, added, or holds other props or focus in the second tree, or null when the trees are the same. */
export function treeDifference(before, after) {
  for (const [key, props] of before) if (after.get(key) !== props) return key;
  for (const key of after.keys()) if (!before.has(key)) return key;
  return null;
}

/** The boxes that differ by a point or more; one whose parent's box differs too is left to its parent's line. */
export function frameDifferences(before, after) {
  const differs = new Map();
  for (const [key, was] of before.boxes) {
    const now = after.boxes.get(key);
    if (now && was.box.some((v, i) => Math.abs(v - now.box[i]) >= 1)) differs.set(key, { was, now });
  }
  const box = ([x, y, w, h]) => `(${x}, ${y}) ${w}×${h}`;
  return [...differs.values()].filter(({ was }) => !differs.has(was.parent)).map(({ was, now }) =>
    `${named(was.node.id, before.byId)} is at ${box(was.box)} after the steps and at ${box(now.box)} when built fresh: the host kept layout from what happened before, which a person sees until something re-renders`);
}
