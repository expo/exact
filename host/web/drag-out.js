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

/** The innermost draggable node at or above `target` when it is the app's (`owns`), or the `given` one, and what
 * between them keeps a contact begun there. A draggable another script made, such as an extension's, keeps its own
 * drag. A node a contact holds off is draggable as its app says. */
export function sourceOf(target, mouse, owns = () => true, given) {
  let found = given ?? null;
  for (let at = target?.closest && given === undefined ? target : null; at && !found; at = at.parentElement) if (authored(at) === 'true') found = at;
  const node = found && owns(found) ? found : null;
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
  // Another script's draggable that the browser began the drag at, such as a link around the app's image, is its own.
  if (owns && event.target?.matches?.(SOURCE) && !owns(event.target)) return;
  // Only a contact on the drag's path: another pointer's down names another source.
  const related = down?.target?.isConnected && (down.target === event.target || event.target?.contains?.(down.target) || down.target.contains(event.target));
  // The source the down found, which a field's hold has since made undraggable, while the app still has it draggable
  const recorded = related ? down.source : null;
  const current = recorded?.node?.isConnected && authored(recorded.node) === 'true' && recorded.node.contains(down.target)
    && (owns?.(recorded.node) ?? true);
  const { node, kept } = current ? recorded : sourceOf(related ? down.target : event.target, related ? down.mouse : true, owns);
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

/** `node`'s `draggable` as its app last wrote it, which a contact's hold keeps. */
function authored(node) {
  return held.get(node)?.value ?? node.getAttribute?.('draggable') ?? null;
}

/** The field each contact under way went down in, by its pointer, and whether a node is the app's, as the listener
 * hears them. */
const fields = new Map();
let owned = () => true;

/** Writes `node`'s `draggable` as the app says, which both targets' renderers do through here: while a contact holds
 * the node's drag off, the value waits for the hold's end, and a node made draggable around a field a contact is in
 * is held for that contact from the write. */
export function writeDraggable(node, value) {
  const holding = held.get(node);
  if (holding) return void (holding.value = value);
  const around = value === 'true' && owned(node) ? [...fields].filter(([, field]) => field !== node && node.contains?.(field)) : [];
  for (const [pointer] of around) hold(node, pointer, value);
  if (around.length) return;
  if (value == null) node.removeAttribute('draggable');
  else node.setAttribute('draggable', value);
}

/** Turns `node`'s drag off for the contact of `pointer`, keeping `value` as the app's. */
function hold(node, pointer, value = authored(node)) {
  const holding = held.get(node) ?? held.set(node, { value, by: new Set() }).get(node);
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
 * app's), and for the down of each contact a drag may begin from, `downs` being those of the contacts already under
 * way; `signal` removes every listener. A control or a text input a contact goes down in turns the drag of every
 * draggable node around it off until the contact ends: Firefox selects no text by a mouse in a field inside a node it
 * may drag, and moves no slider there. */
export function listen(root = globalThis.document, owns = () => true, downs = [], { signal } = {}) {
  // Each primary contact by its pointer: where it went down, whether a mouse's, and the source it found there. Any
  // pointer's contact holds the nodes around its field.
  const contacts = new Map();
  owned = owns;
  // The pointer whose contact the drag under way began from, and the drag's source
  let dragging = null, source = null;
  const end = (pointer) => {
    if (pointer === dragging) dragging = source = null;
    contacts.get(pointer)?.unheard?.();
    contacts.delete(pointer);
    fields.delete(pointer);
    release(pointer);
  };
  // Every pointer the listener knows of: a primary contact's, a field's, or a hold's
  const known = () => new Set([...contacts.keys(), ...fields.keys(), ...pointersHolding()]);
  signal?.addEventListener('abort', () => { for (const pointer of known()) end(pointer); });
  const pointerdown = (event) => {
    // A pointer's new down ends its last contact, whose up went elsewhere
    end(event.pointerId);
    // Every draggable node around a field the contact went down in, and one another contact holds whatever the app has
    // since written
    const field = event.target?.closest?.(KEEPS);
    if (field) fields.set(event.pointerId, field);
    for (let at = field?.parentElement; at; at = at.parentElement) {
      if ((held.has(at) || authored(at) === 'true') && owns(at)) hold(at, event.pointerId);
    }
    if (!event.isPrimary) return;
    // A down handed over from before the listener names the node that was the source then (`source`).
    const mouse = event.pointerType === 'mouse';
    contacts.set(event.pointerId, { target: event.target, mouse, source: sourceOf(event.target, mouse, owns, event.source) });
  };
  const on = (target, type, fn, capture = false) => target?.addEventListener(type, fn, { capture, signal });
  on(root, 'pointerdown', pointerdown, true);
  for (const down of downs) pointerdown(down);
  // A contact ends at its up or its cancellation, which Chromium and Firefox send after a drag's start; at the end of
  // the drag it began, which is all WebKit sends; and, its up gone to another app, when its mouse moves with no button
  // down or the window loses focus.
  on(root, 'pointerup', (event) => end(event.pointerId), true);
  on(root, 'pointercancel', (event) => end(event.pointerId), true);
  on(root, 'pointermove', (event) => { if (event.pointerType === 'mouse' && !event.buttons) end(event.pointerId); }, true);
  on(root.defaultView, 'blur', () => { for (const pointer of known()) end(pointer); });
  on(root, 'dragend', (event) => { if (event.target === source) end(dragging); }, true);
  on(root, 'dragstart', (event) => {
    // A drag an earlier listener cancelled is no drag.
    if (event.defaultPrevented) return;
    // The contact on the drag's path began it. Which of two such contacts did is not known, so the drag is read from
    // the browser's own target, and its end ends neither: each ends at its own up, or a mouse's at its next move or
    // down. WebKit, which sends a drag's contact only its end, gives nothing to tell them apart by, so there a hold of
    // the other one's lasts until then.
    const onPath = [...contacts].filter(([, contact]) => contact.target.isConnected && (contact.target === event.target
      || event.target?.contains?.(contact.target) || contact.target.contains(event.target)));
    const [pointer, contact] = onPath.length === 1 ? onPath[0] : [null, null];
    start(event, undefined, contact, owns);
    [dragging, source] = contact && !event.defaultPrevented ? [pointer, event.target] : [null, null];
    // WebKit sends `dragend` to the drag's source even once it is out of the document, where `root` does not hear it.
    // The listener is the contact's and this drag's: it hears only the source's own end, and goes with the contact or
    // once another listener has cancelled the drag.
    if (!contact || event.defaultPrevented || !event.target.addEventListener) return;
    const from = event.target, heard = (ended) => { if (ended.target === from && contacts.get(pointer) === contact) end(pointer); };
    from.addEventListener('dragend', heard, { signal });
    contact.unheard = () => from.removeEventListener?.('dragend', heard);
    // A drag another listener cancelled has no end to hear.
    setTimeout(() => {
      if (!event.defaultPrevented) return;
      contact.unheard();
      if (source === from) dragging = source = null;
    });
  });
}

/** The pointers that hold a node now. */
function pointersHolding() {
  return new Set([...held.values()].flatMap((holding) => [...holding.by]));
}

// Loaded as a page script by the wasm target's glue (`loadAfterPaint`), which reads what it registers.
if (globalThis.exact) Object.assign(globalThis.exact, { listenDragOut: listen, writeDraggable });
