// @ref LLP 1094.000 D6 — a drag out of the page is the browser's own. A node an author made draggable carries
// HTML's `draggable` attribute (`element.rs`), and as its drag starts this listener gives the drag the node's
// admitted data, read from its `data-` attributes: `text` as `text/plain`, `url` and then each shipped file's
// absolute URL as `text/uri-list`, an image file as `text/html`, and the first file as Chrome's `DownloadURL`,
// which a drop on the desktop saves. A node that carries nothing begins no drag. The JS target loads it from
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

/** The innermost draggable node at or above `target`, and what between them keeps a contact begun there. */
export function sourceOf(target, mouse) {
  const node = target?.closest?.(SOURCE);
  const keeper = node && target.closest(mouse ? KEEPS_A_MOUSE : KEEPS);
  return { node, kept: !!keeper && keeper !== node && node.contains(keeper) ? keeper : null };
}

/** What `node`'s drag carries, `{ text, uris, html, download }`, or null when it carries nothing. Its files are
 * shipped `assets/` paths (the page admits nothing else), made absolute against `base`. */
export function carried(node, base) {
  const files = (node.getAttribute('data-datatransferfiles') ?? '').split('\n').filter(Boolean).map((entry) => {
    const href = new URL(entry, base).href, name = entry.slice(entry.lastIndexOf('/') + 1);
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

/** A `dragstart` during a contact that began at `down` (its target and whether a mouse's), or at the event's target:
 * the innermost draggable node under the contact gives the drag its data, its picture when the browser began the drag
 * at another element (an image inside it, or a link around it), and a copy. A contact something inside the node keeps
 * begins no drag of the node, and leaves the keeper's own drag (a link's, selected text's) to the browser. */
export function start(event, base = globalThis.document?.baseURI, down = null) {
  // A swipe on the contact took it first (`motion-glue.js`, LLP 1094.000 D8).
  if (event.defaultPrevented) return;
  // Only a contact on the drag's path: another pointer's down names another source.
  const related = down?.target?.isConnected && (down.target === event.target || event.target?.contains?.(down.target) || down.target.contains(event.target));
  const from = related ? down.target : event.target;
  const { node, kept } = sourceOf(from, related ? down.mouse : true);
  if (kept) {
    // The keeper's own drag (a link's, selected text's) is the browser's; a picture in it drags no more than one in any
    // press does (LLP 1115.000 D4.14).
    const own = kept.contains(event.target) && event.target.localName !== 'img';
    if (!own && event.target?.closest?.(SOURCE)) event.preventDefault();
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

/** Listens for every drag in `root`'s document, and for the down of each primary contact a drag may begin from. */
export function listen(root = globalThis.document) {
  let down = null;
  root.addEventListener('pointerdown', (event) => {
    if (event.isPrimary) down = { target: event.target, mouse: event.pointerType === 'mouse' };
  }, true);
  root.addEventListener('pointerup', (event) => { if (event.isPrimary) down = null; }, true);
  root.addEventListener('dragstart', (event) => start(event, undefined, down));
}

// Loaded as a page script by the wasm target's glue (`loadAfterPaint`), which reads what it registers.
if (globalThis.exact) globalThis.exact.listenDragOut = listen;
