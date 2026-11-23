// @ref LLP 1094.000 D6 — a drag out of the page is the browser's own. A node an author made draggable carries
// HTML's `draggable` attribute (`element.rs`), and as its drag starts this listener gives the drag the node's
// admitted data, read from its `data-` attributes: `text` as `text/plain`, `url` and then each shipped file's
// absolute URL as `text/uri-list`, an image file as `text/html`, and the first file as Chrome's `DownloadURL`,
// which Chrome on macOS and Windows saves as a file where the drop lands in the Finder or Explorer; elsewhere the
// receiver reads the URL. A node that carries nothing begins no drag. The JS target loads it from
// `drag.js`; the wasm target's glue loads it when a node first turns draggable.

import { DRAG_FIELDS, downForDrag, lightDomDragPath, observeDragInput, pressedAt } from './drag-input.js';

/** Whether `el` is a drag's source by its own `draggable`, HTML's enumerated attribute, read in any case. Firefox ignores
 * it on an SVG element and begins the drag of the draggable node around it; Chromium and WebKit begin the drag at the
 * SVG element, which the listener leaves to its script as an element inside the node (Measured). */
const explicit = (el) => el.namespaceURI !== 'http://www.w3.org/2000/svg' && el.matches('[draggable="true" i]');

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
 * every pointer, a read-only Markdown field among them (`markup-editor.js`), and for a mouse also a press, a button or a
 * link, as AppKit's mouse chain keeps it. */
const KEEPS_A_MOUSE = `${DRAG_FIELDS},button,a[href],[data-exact-on~="press"]`;

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

/** A `dragstart` as the browser began it: decided from the event and the down it began from, then given its data. */
export function start(event, base = globalThis.document?.baseURI, owns = () => true, down = null) {
  const node = decide(event, owns, down);
  give(event, node, readPayload(event, node, base));
}

/** What the drag the browser began is, read from the event and its best matching `down` ({ target, x, y, mouse }): the
 * node whose data it carries, `CANCEL`, or nothing for a drag that is not the app's. The listener decides and gives the
 * data as the event begins, before the target's own listeners can change what it reads or stop the event.
 *
 * The browser starts a drag at the nearest draggable element around the contact, at an image or a link inside it, or
 * at selected text; Firefox may start it at a link around a draggable image. Each engine gives `dragstart` the point
 * the contact went down at (Measured), so what the contact went down on is the down's target when the drag begins at
 * its point, else the element at the point, either when it is in the browser's choice. The drag is the node's when the
 * nearest draggable around that element is one the app made (`exactDraggable`) and `owns`. Another script's draggable
 * keeps its own drag, and so do a selection, a link inside the node and what keeps the contact (rule 3): a field, and
 * for a mouse's down a press, between the element and the node keeps the node's drag from starting, and leaves the
 * keeper's own drag (a link's, a field's text) to the browser. A detached source or a node that carries nothing
 * begins no drag. */
function decide(event, owns, down, inPage) {
  // A script's `dragstart` is no drag, and a selection's begins at its text, not an element (Firefox). A shadow root's
  // content is its host's, as the document sees it.
  const target = event.isTrusted !== false && event.target?.nodeType === 1 ? event.target : null;
  // Another script's draggable the browser began the drag at is its own, whatever is inside it, an SVG element too where
  // the engine honors its mark.
  if (!target || (target.matches('[draggable="true" i]') && !declared(target))) return;
  const draggable = (el) => (off.has(el) && declared(el)) || explicit(el);
  const image = target.localName === 'img', keepers = down?.mouse !== false ? KEEPS_A_MOUSE : DRAG_FIELDS;
  // An earlier capture listener can take the image out of the page. Its dispatch path still names its card.
  if (image && inPage && !inPage(target)) {
    const ancestors = lightDomDragPath(event);
    if (ancestors.length) {
      const node = ancestors.find(draggable);
      if (!node || !declared(node) || !owns(node)) return;
      const inside = ancestors.slice(0, ancestors.indexOf(node));
      if (inside.some((el) => el.matches(keepers))) return CANCEL;
      return inside.some((el) => el.matches('a[href]')) ? undefined : CANCEL;
    }
  }
  // The down's target outlasts a pressed look that moves it from under the point.
  const point = pressedAt(down, event, target) ?? target.ownerDocument.elementFromPoint?.(event.clientX, event.clientY);
  const at = point && target.contains(point) ? point : target;
  // The nearest draggable, or a node the app has draggable that a field's down turned off, whose field keeps the drag
  let node = at;
  while (node && !draggable(node)) node = node.parentElement;
  if (!node || !declared(node) || !owns(node)) return;
  const keeper = at.closest(keepers);
  if (keeper && keeper !== node && node.contains(keeper)) {
    // A picture in a keeper drags no more than one in any press does (LLP 1115.000 D4.14).
    return !keeper.contains(target) || image ? CANCEL : undefined;
  }
  // The browser began the drag at an element inside the node: an image is the node's drag, but for one in a link
  // there; anything else, a link's, is its own.
  const link = target.closest('a[href]');
  if (target !== node && node.contains(target) && (!image || (link !== node && node.contains(link)))) return;
  const inDocument = inPage ? inPage(node) && inPage(event.composedPath?.()[0] ?? target) : node.isConnected;
  return inDocument ? node : CANCEL;
}

/** A drag that begins nothing. */
const CANCEL = Symbol('cancel');

function readPayload(event, node, base) {
  return node && node !== CANCEL && !event.defaultPrevented ? carried(node, base) : null;
}

/** Does what `decide` said: gives the drag the node's data, pictured as the node when the browser began it at another
 * element, as a copy, or cancels it. A drag an earlier listener cancelled is left alone. */
function give(event, decision, data) {
  if (!decision || event.defaultPrevented) return;
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
  if (event.target !== decision) {
    const box = decision.getBoundingClientRect();
    transfer.setDragImage(decision, event.clientX - box.left, event.clientY - box.top);
  }
}

/** Whether the app made `node` draggable: both targets write its `draggable` through `writeDraggable`, which keeps the
 * value on the node. Another script's `draggable`, on its own element or on one of the app's, is not the app's. */
const declared = (node) => node.exactDraggable === 'true';

/** The nodes whose drag a field turned off, each with whether Exact wrote its `false`; the fields that keep them off
 * (`listen`); and whether a node is still the app's (`listen`'s `owns`). */
let off = new Map(), fields = new Set(), owned = () => true;

/** Writes `node`'s `draggable` as the app says, which both targets' renderers do through here, and keeps the value on
 * the node. A node the app makes draggable around a protected field stays off until the field's protection ends. */
export function writeDraggable(node, value) {
  node.exactDraggable = value;
  if (off.has(node)) return;
  if (value === 'true' && owned(node) && [...fields].some((field) => node !== field && node.contains?.(field))) {
    write(node, 'false');
    off.set(node, true);
    return;
  }
  write(node, value);
}

const write = (node, value) => (value == null ? node.removeAttribute('draggable') : node.setAttribute('draggable', value));

/** Turns off the drag of the nodes `now`, writing `false` where it reads otherwise, and gives every other node it turned
 * off its app's `draggable`: a node the app still has draggable gets it whatever another script wrote meanwhile, and one
 * the app has since turned undraggable only while it reads a `false` written here. */
function turnOff(now, waiting) {
  const next = new Map();
  for (const node of now) {
    const writes = node.getAttribute('draggable')?.toLowerCase() !== 'false';
    if (writes) node.setAttribute('draggable', 'false');
    // A `false` the wasm wait wrote (views.js) is Exact's, as one written here is.
    const wrote = off.get(node) ?? waiting?.has(node) ?? false;
    next.set(node, writes || (wrote && node.getAttribute('draggable') === 'false'));
  }
  for (const [node, wrote] of off) {
    if (!next.has(node) && (declared(node) || (wrote && node.getAttribute('draggable') === 'false'))) write(node, node.exactDraggable);
  }
  off = next;
}

/** Listens for every drag in `root`'s document from a node the app made draggable (`owns`: whether it is still the
 * app's), and for each primary press; `signal` removes the listeners. A control or a text input that a contact holds,
 * and the one the last primary down went down in until the next, turns off the drag of every draggable node around it,
 * since Firefox selects no text by a mouse in a field inside a node it may drag. `input` and `waiting` are the wasm
 * wait's (views.js): the presses it followed, and the nodes it wrote `false`. */
export function listen(root = globalThis.document, owns = () => true, { signal, input, waiting } = {}) {
  owned = owns;
  signal?.addEventListener('abort', () => {
    fields.clear();
    turnOff(new Set());
    owned = () => true;
  });
  const protect = ({ field, held }, waiting) => {
    fields = new Set([...held.values()].map((press) => press.field).concat(field).filter(Boolean));
    const now = new Set();
    for (const field of fields) {
      for (let at = field.parentElement; at; at = at.parentElement) if (declared(at) && owns(at)) now.add(at);
    }
    turnOff(now, waiting);
  };
  const presses = observeDragInput(root, signal, input, protect);
  if (input) protect(input, waiting);
  // Capture gives the drag its data before the target's listeners run, while the transfer is writable. A later
  // listener may still cancel it, such as a swipe's listener on the contact.
  // A form control named `getRootNode` shadows the form's method.
  const getRootNode = (root.defaultView?.Node ?? globalThis.Node)?.prototype.getRootNode;
  const inPage = (node) => node != null && (getRootNode ?? node.getRootNode).call(node, { composed: true }) === root;
  const given = new WeakMap();
  root.addEventListener('dragstart', (event) => {
    if (event.isTrusted === false) return;
    const node = decide(event, owns, downForDrag(presses, event), inPage);
    const data = readPayload(event, node, root.baseURI ?? globalThis.document?.baseURI);
    give(event, node, data);
    if (data) given.set(event, { node, data });
  }, { capture: true, signal });
  // Given again where the event reaches the document, over what a listener at the node wrote: the drag carries the
  // data the node declared as it began. A listener that took the node or the browser's source out of the page, into
  // another document too (a same-origin frame's, where it still reads as connected), or the node from the app,
  // meanwhile cancels the drag, which already holds the node's data. The source is read inside an open shadow root,
  // whose host the event's target is; a closed one's is its host.
  root.addEventListener('dragstart', (event) => {
    if (event.isTrusted === false) return;
    const payload = given.get(event);
    given.delete(event);
    if (!payload) return;
    const source = event.composedPath?.()[0] ?? event.target;
    if (inPage(payload.node) && inPage(source) && owns(payload.node)) give(event, payload.node, payload.data);
    else event.preventDefault();
  }, { signal });
}

// Loaded as a page script by the wasm target's glue (`loadAfterPaint`), which reads what it registers.
if (globalThis.exact) Object.assign(globalThis.exact, { listenDragOut: listen, writeDraggable });
