// @ref LLP 1094.000 D6 — a drag out of the page is the browser's own. A node an author made draggable carries
// HTML's `draggable` attribute (`element.rs`), and as its drag starts this listener gives the drag the node's
// admitted data, read from its `data-` attributes: `text` as `text/plain`, `url` and then each shipped file's
// absolute URL as `text/uri-list`, an image file as `text/html`, and the first file as Chrome's `DownloadURL`,
// which Chrome on macOS and Windows saves as a file where the drop lands in the Finder or Explorer; elsewhere the
// receiver reads the URL. A node that carries nothing begins no drag. The JS target loads it from
// `drag.js`; the wasm target's glue loads it when a node first turns draggable.

/** What makes a node the drag's source: the attribute `element.rs` writes for `draggable=true`. */
export const SOURCE = '[draggable="true"]';

/** A file's type by its extension, as `expect drag` reads it (`scripts/share-target.mjs`); anything else is bytes. */
const TYPES = {
  txt: 'text/plain', text: 'text/plain', md: 'text/markdown', markdown: 'text/markdown', html: 'text/html', htm: 'text/html',
  csv: 'text/csv', tsv: 'text/tab-separated-values', json: 'application/json', pdf: 'application/pdf', zip: 'application/zip',
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml',
  heic: 'image/heic', heif: 'image/heif', mp4: 'video/mp4', mov: 'video/quicktime', m4a: 'audio/mp4', mp3: 'audio/mpeg',
};

export function typeOf(name) {
  const dot = name.lastIndexOf('.'), extension = name.slice(dot + 1).toLowerCase();
  return (dot > 0 && Object.hasOwn(TYPES, extension) && TYPES[extension]) || 'application/octet-stream';
}

/** What keeps a contact from the draggable node around it (LLP 1094.000 D8, rule 3): a control or a text input for
 * every pointer, and for a mouse also a press, a button or a link, as AppKit's mouse chain keeps it. */
const KEEPS = 'input,select,textarea,[contenteditable]:not([contenteditable="false"])';
const KEEPS_A_MOUSE = `${KEEPS},button,a[href],[data-exact-on~="press"]`;

/** The innermost draggable node at or above `target` when it is the app's (`owns`), and what between them keeps a
 * contact begun there. A draggable another script made, such as an extension's, keeps its own drag. */
export function sourceOf(target, mouse, owns = () => true) {
  const found = target?.closest?.(SOURCE), node = found && owns(found) ? found : null;
  const keeper = node && target.closest(mouse ? KEEPS_A_MOUSE : KEEPS);
  return { node, kept: !!keeper && keeper !== node && node.contains(keeper) ? keeper : null };
}

/** What `node`'s drag carries, `{ text, uris, html, download }`, or null when it carries nothing. Its files are
 * shipped `assets/` paths (the page admits nothing else), made absolute against `base`. */
export function carried(node, base) {
  const files = (node.getAttribute('data-datatransferfiles') ?? '').split('\n').filter(Boolean).map((entry) => {
    // A name's trailing spaces encoded, which the URL parser would otherwise strip
    const href = new URL(entry.replace(/ +$/, (spaces) => '%20'.repeat(spaces.length)), base).href;
    const name = entry.slice(entry.lastIndexOf('/') + 1);
    return { href, name, type: typeOf(name) };
  });
  const text = node.getAttribute('data-datatransfertext') || '', url = node.getAttribute('data-datatransferurl') || '';
  if (!text && !url && !files.length) return null;
  const image = files.find((f) => f.type.startsWith('image/'));
  const first = files[0];
  return {
    text,
    uris: [url, ...files.map((f) => f.href)].filter(Boolean),
    html: image ? `<img src="${image.href.replace(/&/g, '&amp;').replace(/"/g, '&quot;')}">` : '',
    download: first ? `${first.type}:${first.name}:${first.href}` : '',
  };
}

/** A `dragstart` during a contact that began at `down` (its target, whether a mouse's, and the `source` it found
 * there), or at the event's target: the innermost draggable node under the contact gives the drag its data, its
 * picture when the browser began the drag at another element (an image inside it, or a link around it), and a copy. A
 * contact something inside the node keeps begins no drag of the node, and leaves the keeper's own drag (a link's,
 * selected text's) to the browser. */
export function start(event, base = globalThis.document?.baseURI, down = null, owns) {
  // A swipe on the contact took it first (`motion-glue.js`, LLP 1094.000 D8).
  if (event.defaultPrevented) return;
  // Only a contact on the drag's path: another pointer's down names another source.
  const related = down?.target?.isConnected && (down.target === event.target || event.target?.contains?.(down.target) || down.target.contains(event.target));
  // The source the down found, which a field's hold has since made undraggable
  const { node, kept } = related ? down.source ?? sourceOf(down.target, down.mouse, owns) : sourceOf(event.target, true, owns);
  if (kept) {
    // The keeper's own drag (a link's, selected text's) is the browser's; a picture in it drags no more than one in any
    // press does (LLP 1115.000 D4.14).
    const own = kept.contains(event.target) && event.target.localName !== 'img';
    if (!own && node.contains(event.target)) event.preventDefault();
    return;
  }
  if (!node) return;
  const data = carried(node, base);
  if (!data) {
    event.preventDefault();
    return;
  }
  const transfer = event.dataTransfer;
  // `clearData` leaves a file item, the picture Chromium adds for a drag begun on an image.
  transfer.items.clear();
  if (data.text) transfer.setData('text/plain', data.text);
  if (data.uris.length) transfer.setData('text/uri-list', data.uris.join('\r\n'));
  if (data.html) transfer.setData('text/html', data.html);
  if (data.download) transfer.setData('DownloadURL', data.download);
  transfer.effectAllowed = 'copy';
  if (event.target !== node) {
    const box = node.getBoundingClientRect();
    transfer.setDragImage(node, event.clientX - box.left, event.clientY - box.top);
  }
}

/** The nodes whose drag a contact holds off, each with the `draggable` its app last wrote (null: none) and the
 * pointers of the contacts holding it. */
const held = new Map();

/** Writes `node`'s `draggable` as the app says, which both targets' renderers do through here: while a contact holds
 * the node's drag off, the value waits for the hold's end. */
export function writeDraggable(node, value) {
  const hold = held.get(node);
  if (hold) hold.value = value;
  else if (value == null) node.removeAttribute('draggable');
  else node.setAttribute('draggable', value);
}

/** Turns `node`'s drag off for the contact of `pointer`. */
function hold(node, pointer) {
  const holding = held.get(node) ?? held.set(node, { value: node.getAttribute('draggable'), by: new Set() }).get(node);
  holding.by.add(pointer);
  node.setAttribute('draggable', 'false');
}

/** Ends the holds of `pointer`'s contact: a node no contact holds now has the `draggable` its app last wrote. */
function release(pointer) {
  for (const [node, holding] of [...held]) {
    if (!holding.by.delete(pointer) || holding.by.size) continue;
    held.delete(node);
    writeDraggable(node, holding.value);
  }
}

/** Listens for every drag in `root`'s document from a node the app made draggable (`owns`: whether an element is the
 * app's), and for the down of each primary contact a drag may begin from. A control or a text input that keeps a
 * contact turns the drag of every draggable node around it off until the contact ends: Firefox selects no text by a
 * mouse in a field inside a node it may drag, and moves no slider there. */
export function listen(root = globalThis.document, owns = () => true) {
  // Each primary contact by its pointer: where it went down, whether a mouse's, and the source it found there
  const contacts = new Map();
  // The pointer whose contact the drag under way began from
  let dragging = null;
  const end = (pointer) => {
    if (pointer === dragging) dragging = null;
    if (contacts.delete(pointer)) release(pointer);
  };
  root.addEventListener('pointerdown', (event) => {
    if (!event.isPrimary) return;
    // A pointer's new down ends its last contact, whose up went elsewhere
    end(event.pointerId);
    const mouse = event.pointerType === 'mouse', source = sourceOf(event.target, mouse, owns);
    contacts.set(event.pointerId, { target: event.target, mouse, source });
    if (!source.kept?.matches(KEEPS)) return;
    for (let at = source.kept.parentElement; at; at = at.parentElement) {
      if (at.matches(SOURCE) && owns(at)) hold(at, event.pointerId);
    }
  }, true);
  // A contact ends at its up or its cancellation, which Chromium and Firefox send after a drag's start; at the end of
  // the drag it began, which is all WebKit sends; and, its up gone to another app, when its mouse moves with no button
  // down or the window loses focus.
  root.addEventListener('pointerup', (event) => end(event.pointerId), true);
  root.addEventListener('pointercancel', (event) => end(event.pointerId), true);
  root.addEventListener('pointermove', (event) => { if (event.pointerType === 'mouse' && !event.buttons) end(event.pointerId); }, true);
  root.defaultView?.addEventListener('blur', () => { for (const pointer of [...contacts.keys()]) end(pointer); });
  root.addEventListener('dragend', () => end(dragging), true);
  root.addEventListener('dragstart', (event) => {
    // The newest contact on the drag's path began it
    const begun = [...contacts].reverse().find(([, contact]) => contact.target.isConnected && (contact.target === event.target
      || event.target?.contains?.(contact.target) || contact.target.contains(event.target)));
    dragging = begun?.[0] ?? null;
    start(event, undefined, begun?.[1] ?? null, owns);
  });
}

// Loaded as a page script by the wasm target's glue (`loadAfterPaint`), which reads what it registers.
if (globalThis.exact) Object.assign(globalThis.exact, { listenDragOut: listen, writeDraggable });
