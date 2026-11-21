// @ref LLP 1094.000 D10 — what a draggable node's drag carries, read from the props the runner admitted, as a
// `tree` reply has them on every host: `expect drag` asserts it. A file's type is its extension's.
import { EXTENSION_TYPES } from './share-target.mjs';

/** A node's drag, `{ text, url, files: [{ path, name, type }] }`, or null when it carries nothing now. */
export function dragOf(node) {
  const p = node?.props ?? {};
  if (p.draggable !== true && p.draggable !== 'true') return null;
  const files = String(p.dataTransferFiles ?? '').split('\n').filter(Boolean).map((path) => {
    const name = path.slice(path.lastIndexOf('/') + 1), dot = name.lastIndexOf('.');
    return { path, name, type: (dot > 0 && EXTENSION_TYPES[name.slice(dot).toLowerCase()]) || 'application/octet-stream' };
  });
  const text = p.dataTransferText || null, url = p.dataTransferUrl || null;
  return text || url || files.length ? { text, url, files } : null;
}

/** What a drag offers, one entry a type: each file, then `url` as `text/uri-list` and `text` as `text/plain`. */
const offered = (drag) => [
  ...(drag?.files ?? []).map((f) => ({ type: f.type, name: f.name, path: f.path })),
  ...(drag?.url ? [{ type: 'text/uri-list', value: drag.url }] : []),
  ...(drag?.text ? [{ type: 'text/plain', value: drag.text }] : []),
];

const describe = (drag) => offered(drag).map((o) => `${o.type} ${o.name ?? JSON.stringify(o.value)}`).join(', ') || 'nothing';

/** `expect drag <target> (none | has <kind> [== <value>] [name <name>] | missing <kind>)` of the `tree` node the
 * target names (agent-test.mjs `targetNode`): null when it holds, else `{ message }`. */
export function dragExpectation(node, step) {
  if (!node) return { message: `drag of "${step.target}": no view carries that testId` };
  const drag = dragOf(node), all = offered(drag), carried = `drag of "${step.target}" carries ${describe(drag)}`;
  if (step.kind == null) return drag ? { message: `${carried}, expected nothing` } : null;
  const ofKind = all.filter((o) => o.type === step.kind);
  if (!step.present) return ofKind.length ? { message: `${carried}, expected no ${step.kind}` } : null;
  const hit = ofKind.find((o) => (step.value == null || o.value === step.value) && (step.name == null || o.name === step.name));
  if (hit) return null;
  const wanted = [step.kind, step.value != null && `== ${JSON.stringify(step.value)}`, step.name != null && `named ${JSON.stringify(step.name)}`].filter(Boolean).join(' ');
  return { message: `${carried}, expected ${wanted}` };
}
