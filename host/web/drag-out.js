// @ref LLP 1094.000 D6 — a drag out of the page is the browser's own. A node an author made draggable carries
// HTML's `draggable` attribute (`element.rs`), and as its drag starts this listener gives the drag the node's
// admitted data, read from its `data-` attributes: `text` as `text/plain`, `url` and then each shipped file's
// absolute URL as `text/uri-list`, an image file as `text/html`, and the first file as Chrome's `DownloadURL`,
// which Chrome on macOS and Windows saves as a file where the drop lands in the Finder or Explorer; elsewhere the
// receiver reads the URL. A node that carries nothing begins no drag. The JS target loads it from
// `drag.js`; the wasm target's glue loads it when a node first turns draggable.

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
const KEEPS = 'input,select,textarea,[contenteditable]:not([contenteditable="false"]),.exact-markdown-editor';
const KEEPS_A_MOUSE = `${KEEPS},button,a[href],[data-exact-on~="press"]`;

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

/** A `dragstart` as the browser began it: decided from the event and the last primary `down`, then done. */
export function start(event, base = globalThis.document?.baseURI, owns = () => true, down = null) {
  give(event, decide(event, owns, down), base);
}

/** What the drag the browser began is, read from the event and the last primary `down` ({ target, x, y, mouse }): the
 * node whose data it carries, `CANCEL`, or nothing for a drag that is not the app's. The listener decides as the event
 * begins, before the target's own listeners can change what it reads, and gives the data once they have run.
 *
 * The browser starts a drag at the nearest draggable element around the contact, at an image or a link inside it, or
 * at selected text; Firefox may start it at a link around a draggable image. Each engine gives `dragstart` the point
 * the contact went down at (Measured), so what the contact went down on is the down's target when the drag begins at
 * its point, else the element at the point, either when it is in the browser's choice. The drag is the node's when the
 * nearest draggable around that element is one the app made (`exactDraggable`) and `owns`. Another script's draggable
 * keeps its own drag, and so do a selection, a link inside the node and what keeps the contact (rule 3): a field, and
 * for a mouse's down a press, between the element and the node keeps the node's drag from starting, and leaves the
 * keeper's own drag (a link's, a field's text) to the browser. A node that carries nothing begins no drag. */
function decide(event, owns, down) {
  // A script's `dragstart` is no drag, and a selection's begins at its text, not an element (Firefox).
  const target = event.isTrusted !== false && event.target?.nodeType === 1 ? event.target : null;
  // Another script's draggable the browser began the drag at is its own, whatever is inside it, an SVG element too where
  // the engine honors its mark.
  if (!target || (target.matches('[draggable="true" i]') && !declared(target))) return;
  // The down's target outlasts a pressed look that moves it from under the point.
  const pressed = atPoint(down, event) && target.contains(down.target);
  const point = pressed ? down.target : target.ownerDocument.elementFromPoint?.(event.clientX, event.clientY);
  const at = point && target.contains(point) ? point : target;
  // The nearest draggable, or a node the app has draggable that a field's down turned off, whose field keeps the drag
  let node = at;
  while (node && !(off.has(node) && declared(node)) && !explicit(node)) node = node.parentElement;
  if (!node || !declared(node) || !owns(node)) return;
  const keeper = at.closest(down?.mouse === false ? KEEPS : KEEPS_A_MOUSE);
  if (keeper && keeper !== node && node.contains(keeper)) {
    // A picture in a keeper drags no more than one in any press does (LLP 1115.000 D4.14).
    return !keeper.contains(target) || target.localName === 'img' ? CANCEL : undefined;
  }
  // The browser began the drag at an element inside the node: an image is the node's drag, but for one in a link
  // there; anything else, a link's, is its own.
  const link = target.closest('a[href]');
  if (target !== node && node.contains(target) && (target.localName !== 'img' || (link !== node && node.contains(link)))) return;
  return node;
}

/** A drag that begins nothing. */
const CANCEL = Symbol('cancel');

/** Does what `decide` said: gives the drag the node's data, pictured as the node when the browser began it at another
 * element, as a copy, or cancels it. A drag another listener cancelled, a swipe's on the contact (`motion-glue.js`, LLP
 * 1094.000 D8), is left alone. */
function give(event, decision, base = globalThis.document?.baseURI) {
  if (!decision || event.defaultPrevented) return;
  const data = decision === CANCEL ? null : carried(decision, base);
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

/** Whether `event` is at the point of `down`, within the half pixel Chromium rounds a down's. */
const atPoint = (down, event) => Math.abs(down?.x - event.clientX) <= 1 && Math.abs(down?.y - event.clientY) <= 1;

/** Whether the app made `node` draggable: both targets write its `draggable` through `writeDraggable`, which keeps the
 * value on the node. Another script's `draggable`, on its own element or on one of the app's, is not the app's. */
const declared = (node) => node.exactDraggable === 'true';

/** The nodes whose drag a field's down turned off, until the next primary down, each with whether the down wrote its
 * `false`. */
let off = new Map();

/** Writes `node`'s `draggable` as the app says, which both targets' renderers do through here, and keeps the value on
 * the node: a node a field's contact turned off gets it at the next primary down. */
export function writeDraggable(node, value) {
  node.exactDraggable = value;
  if (!off.has(node)) write(node, value);
}

const write = (node, value) => (value == null ? node.removeAttribute('draggable') : node.setAttribute('draggable', value));

/** Turns off the drag of the nodes `now`, writing `false` where it reads otherwise, and gives every other node it turned
 * off its app's `draggable`: a node the app still has draggable gets it whatever another script wrote meanwhile, and one
 * the app has since turned undraggable only while it reads a `false` written here. */
function turnOff(now) {
  const next = new Map();
  for (const node of now) {
    const writes = node.getAttribute('draggable')?.toLowerCase() !== 'false';
    if (writes) node.setAttribute('draggable', 'false');
    // The `false` is still the one written here only while it reads exactly that
    next.set(node, writes || ((off.get(node) ?? false) && node.getAttribute('draggable') === 'false'));
  }
  for (const [node, wrote] of off) {
    if (!next.has(node) && (declared(node) || (wrote && node.getAttribute('draggable') === 'false'))) write(node, node.exactDraggable);
  }
  off = next;
}

/** Listens for every drag in `root`'s document from a node the app made draggable (`owns`: whether it is still the
 * app's), and for each primary down; `signal` removes the listeners. A primary down in a control or a text input turns
 * the drag of every draggable node around it off until the next primary down, since Firefox selects no text by a mouse
 * in a field inside a node it may drag. The down is kept for the drag it may begin. */
export function listen(root = globalThis.document, owns = () => true, { signal } = {}) {
  // The last primary down, and the target of a `pointerdown` whose own `mousedown` is still to come
  let last = null, paired = null;
  signal?.addEventListener('abort', () => turnOff(new Set()));
  const down = (event) => {
    if (event.isPrimary === false || event.isTrusted === false) return;
    // A press's `mousedown` after its own `pointerdown` adds nothing: a pen's or a finger's would read as a mouse's.
    const own = event.type === 'mousedown' && paired === event.target;
    paired = event.type === 'mousedown' ? null : event.target;
    if (own) return;
    last = { target: event.target, x: event.clientX, y: event.clientY, mouse: (event.pointerType ?? 'mouse') === 'mouse' };
    const now = new Set(), field = event.target?.closest?.(KEEPS);
    for (let at = field?.parentElement; at; at = at.parentElement) if (declared(at) && owns(at)) now.add(at);
    turnOff(now);
  };
  // A `mousedown` alone too: WebKit sends a press after a drag no `pointerdown` (Measured under Playwright).
  for (const type of ['pointerdown', 'mousedown']) root.addEventListener(type, down, { capture: true, signal });
  // Heard first, before any listener can stop the drag's event or change what it reads: a press's own `mousedown` is
  // past by its drag, if another listener kept it from the document, and the drag is decided as it began, by event, so a
  // `dragstart` another listener dispatches meanwhile has its own; its data is given once the event has run.
  const decided = new WeakMap();
  root.addEventListener('dragstart', (event) => {
    if (event.isTrusted === false) return;
    paired = null;
    decided.set(event, decide(event, owns, last));
  }, { capture: true, signal });
  root.addEventListener('dragstart', (event) => give(event, decided.get(event)), { signal });
}

// Loaded as a page script by the wasm target's glue (`loadAfterPaint`), which reads what it registers.
if (globalThis.exact) Object.assign(globalThis.exact, { listenDragOut: listen, writeDraggable });
