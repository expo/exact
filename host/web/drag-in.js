// @ref LLP 1094.001 D1–D3 — a drag into the page, on both web targets: what a node with `drop` takes, asked while the
// browser's `dragover` waits for an answer, and what its drop carries, read while the drop event lasts (HTML keeps a
// drag's data from the page until then). The JS target's files.js and the wasm target's input-glue.js take the drop;
// documents-glue.js tracks the nodes a drag is over.

/** A field that inserts a drop's text itself while it can be edited: a text input, a textarea or an editable element.
 * The agent's Chrome drop asks it too (scripts/agent-drag.mjs). */
export const FIELDS = ':is(input:is(:not([type]), [type=text i], [type=search i], [type=url i], [type=tel i], [type=email i], [type=password i], [type=number i]), textarea, [contenteditable]:not([contenteditable="false" i])):not(:disabled, [readonly])';

/** Whether a drag carries what a node with `drop` takes: files, plain text or a link. */
export const carries = (dataTransfer) => ['Files', 'text/plain', 'text/uri-list'].some((type) => dataTransfer?.types?.includes(type));

/** Whether `node`, which declares `drop`, takes the drag of `event`: one that carries what it takes, unless it is a drag
 * of plain text and no files over a field inside the node, which inserts it (D2), as HTML inserts a drop's `text/plain`.
 * The browser drops it only when its source allows the copy this answers (HTML's drag-and-drop processing model), so
 * the source's `effectAllowed` is not asked. */
export function takes(node, event) {
  const dataTransfer = event.dataTransfer;
  if (!carries(dataTransfer)) return false;
  if (dataTransfer.types.includes('Files') || !dataTransfer.types.includes('text/plain')) return true;
  const field = event.target?.closest?.(FIELDS);
  return !field || field === node || !node.contains(field);
}

/** A drop's plain text and its first link that is not a file (D1): `text/uri-list`'s first line that is neither a
 * comment nor a `file:` URL. A lone UTF-16 surrogate becomes U+FFFD, since the runner reads Unicode text. */
export function words(dataTransfer) {
  const url = (dataTransfer.getData('text/uri-list') || '').split(/\r?\n/).map((line) => line.trim())
    .find((line) => line && !line.startsWith('#') && !/^file:/i.test(line)) ?? '';
  const whole = (text) => text.toWellFormed?.() ?? text;
  return { text: whole(dataTransfer.getData('text/plain')), url: whole(url) };
}

/** Read the drop of `event` while it lasts and hand `deliver(files, text, url)` what arrived: its text and link at once,
 * its files once `glue()` (documents-glue.js) has minted them, a file that cannot be read left out. `deliver` answers
 * false when its node is gone. A drop that delivered nothing is a leave at the nodes the drag was over (`exactLeave`),
 * and a drop that waits for its files holds a newer drag's enters there until it lands (`exactWaits`, `exactLands`),
 * all documents-glue.js's. Returns the pending work, for the agent's `clock settle` to wait on. */
export function receive(event, glue, deliver, say) {
  const dataTransfer = event.dataTransfer, { text, url } = words(dataTransfer);
  // Handles are asked for while the event lasts, as the browser hands them over only then
  const items = [...dataTransfer.items].filter((item) => item.kind === 'file'), files = [...dataTransfer.files];
  const handles = items.map((item) => item.getAsFileSystemHandle?.().catch(() => null) ?? null), entries = items.map((item) => item.webkitGetAsEntry?.() ?? null);
  const settle = (found) => {
    const any = found.length || text || url;
    if (any && deliver(found, text, url) !== false) return;
    event.exactLeave?.();
    say(any ? 'drop: refused: the node it dropped on is gone' : 'drop: refused: nothing it carried arrived');
  };
  if (!items.length) return Promise.resolve(settle([]));
  event.exactWaits?.();
  return Promise.resolve(glue?.()).then(async (m) => m ? m.dropped(await Promise.all(handles), files, entries) : [])
    .catch((e) => { say(`drop: its files could not be read: ${e?.message ?? e}`); return []; })
    .then((found) => { if (!found.length) say('drop: the browser handed over no file'); settle(found); })
    .finally(() => event.exactLands?.());
}
