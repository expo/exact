// @ref LLP 1094.000 D10, D14 — what a draggable node's drag carries, read from the props the runner admitted, as a
// `tree` reply has them on every host: `expect drag` asserts it. A file's type is its extension's, and its name the one
// `dataTransferNames` gives it, else its own.
import { EXTENSION_TYPES } from './share-target.mjs';

/** A node's drag, `{ text, url, files: [{ path, name, type }] }`, or null when it carries nothing now. */
export function dragOf(node) {
  const p = node?.props ?? {};
  if (p.draggable !== true && p.draggable !== 'true') return null;
  const names = String(p.dataTransferNames ?? '').split('\n');
  const files = String(p.dataTransferFiles ?? '').split('\n').filter(Boolean).map((path, at) => {
    const own = path.slice(path.lastIndexOf('/') + 1), dot = own.lastIndexOf('.');
    const type = (dot > 0 && EXTENSION_TYPES[own.slice(dot).toLowerCase()]) || 'application/octet-stream';
    return { path, name: names[at] || own, type, named: !!names[at] };
  });
  const text = p.dataTransferText || null, url = p.dataTransferUrl || null;
  return text || url || files.length ? { text, url, files } : null;
}

/** What a drag offers, one entry a type: each file, then `url` as `text/uri-list` and `text` as `text/plain`. */
const offered = (drag) => [
  ...(drag?.files ?? []).map((f) => ({ type: f.type, name: f.name, path: f.path, named: f.named })),
  ...(drag?.url ? [{ type: 'text/uri-list', value: drag.url }] : []),
  ...(drag?.text ? [{ type: 'text/plain', value: drag.text }] : []),
];

/** `s` quoted with JSON's escapes, and DEL's too, which JSON leaves as an invisible character. */
const quoted = (s) => JSON.stringify(s).replace(/\u007f/g, '\\u007f');

/** A path as written, or quoted with its escapes when it holds a control character, which a terminal would act on. */
const shown = (path) => (/[\u0000-\u001f\u007f]/.test(path) ? quoted(path) : path);

/** Each offer as `tree` reads it: a file by its path, and the name it is offered under when it has one of its own. */
export const describe = (drag) => offered(drag).map((o) => {
  if (o.path == null) return `${o.type} ${quoted(o.value)}`;
  return `${o.type} ${shown(o.path)}${o.named ? ` as ${quoted(o.name)}` : ''}`;
}).join(', ') || 'nothing';

/** A `tree` row's note of what its node's drag carries, empty when it carries nothing now (D10, D14). */
export const dragNote = (node) => {
  const drag = dragOf(node);
  return drag ? ` drag{${describe(drag)}}` : '';
};

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
