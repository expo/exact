// The web's drag out of the page (LLP 1094.000 D6): what a draggable node's `dragstart` gives the browser, read
// from the admitted `data-` attributes, and a node that carries nothing beginning no drag. Which contacts the node
// takes, beside controls, a pan and a swipe, is `tests/drag-out.test.mjs`'s, in the browsers.
import { test, expect } from 'bun:test';
import vm from 'node:vm';
import { declaration, deferred } from './glue-fixture.mjs';
import { EXTENSION_TYPES } from '../../scripts/share-target.mjs';
import { carried, listen, sourceOf, start, typeOf, writeDraggable } from './drag-out.js';

const BASE = 'https://example.com/app/';
const node = (attrs) => ({
  parentElement: null,
  getAttribute: (name) => attrs[name] ?? null,
  getBoundingClientRect: () => ({ left: 10, top: 20 }),
  closest: function (selector) { return selector === '[draggable="true"]' && attrs.draggable === 'true' ? this : null; },
});
// `clearData` clears the strings and leaves a file, as Chromium's does; `items.clear` clears both.
const transfer = () => {
  const data = new Map(), files = ['image/png'], calls = [];
  return { data, files, calls, clearData: () => data.clear(), items: { clear: () => { data.clear(); files.length = 0; } },
    setData: (k, v) => data.set(k, v), setDragImage: (...a) => calls.push(a), effectAllowed: 'uninitialized' };
};
const dragstart = (target, dataTransfer = transfer()) =>
  ({ target, dataTransfer, clientX: 50, clientY: 70, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } });

test('a file is typed as `expect drag` types it', () => {
  for (const [ext, type] of Object.entries(EXTENSION_TYPES)) expect(typeOf(`a${ext}`)).toBe(type);
  for (const name of ['a.unknown', 'a.constructor', 'a.__proto__', 'a.toString']) expect(typeOf(name)).toBe('application/octet-stream');
});

test('the drag carries the text, the url then the files as URIs, an image as HTML and the first file to save', () => {
  const card = node({ draggable: 'true', 'data-datatransfertext': 'Mount Fuji', 'data-datatransferurl': 'https://example.com/1016',
    'data-datatransferfiles': 'assets/apple/1016&1017.jpg\nassets/notes "a".pdf' });
  expect(carried(card, BASE)).toEqual({
    text: 'Mount Fuji',
    uris: ['https://example.com/1016', 'https://example.com/app/assets/apple/1016&1017.jpg', 'https://example.com/app/assets/notes%20%22a%22.pdf'],
    html: '<img src="https://example.com/app/assets/apple/1016&amp;1017.jpg">',
    download: 'image/jpeg:1016&1017.jpg:https://example.com/app/assets/apple/1016&1017.jpg',
  });
  expect(carried(node({ draggable: 'true' }), BASE)).toBeNull();
  // A name's trailing space is the file's, which the URL parser would strip.
  expect(carried(node({ draggable: 'true', 'data-datatransferfiles': 'assets/report ' }), BASE).uris).toEqual(['https://example.com/app/assets/report%20']);
});

test('dragstart fills the transfer, leaves no picture file, pictures the node for a drag picked up inside it, and cancels an empty one', () => {
  const card = node({ draggable: 'true', 'data-datatransferfiles': 'assets/a.png' });
  const image = { closest: () => card, parentElement: card, getAttribute: () => null };
  const event = dragstart(image);
  event.dataTransfer.setData('text/uri-list', 'https://example.com/app/assets/a.png?from-the-img');
  start(event, BASE);
  expect(event.defaultPrevented).toBe(false);
  expect([...event.dataTransfer.data.keys()]).toEqual(['text/uri-list', 'text/html', 'DownloadURL']);
  expect(event.dataTransfer.data.get('text/uri-list')).toBe('https://example.com/app/assets/a.png');
  expect(event.dataTransfer.files).toEqual([]);
  expect(event.dataTransfer.effectAllowed).toBe('copy');
  expect(event.dataTransfer.calls).toEqual([[card, 40, 50]]);
  // The browser's drag of the node itself pictures itself.
  const own = dragstart(card);
  start(own, BASE);
  expect(own.dataTransfer.calls).toEqual([]);
  const cancelled = dragstart(node({ draggable: 'true' }));
  start(cancelled, BASE);
  expect(cancelled.defaultPrevented).toBe(true);
  // A drag another listener cancelled, a swipe's, and one outside any draggable node are left as they are.
  const swiped = { ...dragstart(card), defaultPrevented: true };
  start(swiped, BASE);
  expect([...swiped.dataTransfer.data.keys()]).toEqual([]);
  start({ ...dragstart(node({})), preventDefault() { throw new Error('not ours'); } }, BASE);
  // A draggable another script made is not the app's: its drag is its own.
  const foreign = dragstart(node({ draggable: 'true' }));
  start(foreign, BASE, null, () => false);
  expect([foreign.defaultPrevented, [...foreign.dataTransfer.data.keys()]]).toEqual([false, []]);
});

/** The wasm glue's drag-out declarations in a context of their own, with its page's doubles, and its listeners added
 * as its load adds them. */
function wasmGlue(loadAfterPaint) {
  const logs = [], written = [], timers = [], guards = [], views = new Map(), retiredViews = new Set();
  const document = { addEventListener: (type, fn, capture) => guards.push([type, fn, capture]),
    removeEventListener: (type, fn) => guards.splice(guards.findIndex(([t, f]) => t === type && f === fn), 1) };
  const exact = { writeDraggable: (el, value) => { written.push([el.id, value]); el.attributes.draggable = value; } };
  const context = vm.createContext({ host: {}, document, views, retiredViews, log: (line) => logs.push(line), loadAfterPaint, exact,
    AbortController, setTimeout: (fn, ms) => timers.push([fn, ms]) });
  const names = ['liveView', 'loadDragOut', 'waitingDown', 'waitingEnd', 'forgetDrag', 'waitingMove', 'waitingBlur', 'waitingDragEnd', 'onPath',
    'inWaiting', 'waitingDrag', 'WAITING', 'writeDraggable'];
  vm.runInContext(names.map(declaration).join('\n') + '\nfor (const [type, fn] of WAITING) document.addEventListener(type, fn, true);', context);
  const element = (id, view, parentElement = null, { localName = 'div', explicit = false } = {}) => {
    const el = { id, parentElement, localName, isConnected: true, dataset: view == null ? {} : { view: String(view) }, attributes: {},
      setAttribute(name, v) { this.attributes[name] = v; }, removeAttribute(name) { delete this.attributes[name]; },
      matches: (selector) => explicit && selector === '[draggable="true"]',
      contains(other) { for (let at = other; at; at = at.parentElement) if (at === this) return true; return false; } };
    if (view != null) views.set(view, el);
    return el;
  };
  const listener = (type) => guards.find(([t]) => t === type)?.[1];
  return { context, document, exact, logs, written, timers, guards, views, retiredViews, element, listener };
}
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test('the wasm glue keeps a node made draggable undraggable until the listener listens, and tries a failed load again', async () => {
  const loads = [], heard = [];
  let loading = deferred();
  const { context, document, logs, written, timers, guards, retiredViews, element, listener } = wasmGlue((...args) => { loads.push(args); return loading.promise; });
  // From its load, the glue keeps the contacts under way.
  expect(guards.map(([type, , capture]) => [type, capture])).toEqual([['pointerdown', true], ['pointerup', true], ['pointercancel', true],
    ['pointermove', true], ['dragstart', true], ['dragend', true]]);
  const [card, link, plain, gone] = [element('card', 1), element('link', 2), element('plain', 3), element('gone', 4)];
  const field = element('field', 5, card, { localName: 'input' });
  // A field's contact that went down before any node turned draggable
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: field });
  // A node made draggable stays `false`, so a link or an image drags nothing of its own; `false` and none are written.
  context.writeDraggable(card, 'true');
  context.writeDraggable(gone, 'true');
  context.writeDraggable(link, 'false');
  context.writeDraggable(plain, 'true');
  context.writeDraggable(plain, null);
  expect([card.attributes, link.attributes, plain.attributes]).toEqual([{ draggable: 'false' }, { draggable: 'false' }, {}]);
  expect(loads).toEqual([['./drag-out.js', 'listenDragOut']]);
  // A failed load is tried again on one timer, from a URL of its own, while a live node waits; a node made draggable
  // meanwhile waits for it, and a retired one waits no more.
  loading.reject(new Error('import failed'));
  await settle();
  context.writeDraggable(link, 'true');
  retiredViews.add(gone);
  expect([logs, timers.map(([, ms]) => ms), loads.length]).toEqual([['drag-out module: Error: import failed'], [1000], 1]);
  loading = deferred();
  timers[0][0]();
  expect(loads.at(-1)).toEqual(['./drag-out.js?try=1', 'listenDragOut']);
  // Contacts that ended meanwhile, by an up, a mouse's buttonless move or the end of the drag they began, are not handed
  // on, nor is the contact of a drag under way, whose pointer is the drag's.
  const elsewhere = element('elsewhere', 6), drag = (target) => listener('dragstart')({ target, preventDefault() {} });
  listener('pointerdown')({ isPrimary: true, pointerId: 2, pointerType: 'touch', target: link });
  listener('pointerup')({ pointerId: 2 });
  listener('pointerdown')({ isPrimary: false, pointerId: 3, pointerType: 'touch', target: field });
  listener('pointerdown')({ isPrimary: true, pointerId: 4, pointerType: 'mouse', target: plain });
  listener('pointermove')({ pointerId: 4, pointerType: 'mouse', buttons: 0 });
  listener('pointerdown')({ isPrimary: true, pointerId: 5, pointerType: 'pen', target: plain });
  drag(plain);
  listener('dragend')({ target: elsewhere });
  listener('dragend')({ target: plain });
  listener('pointerdown')({ isPrimary: true, pointerId: 6, pointerType: 'mouse', target: elsewhere });
  drag(elsewhere);
  // The app's values are written first, then the listener takes the contacts under way and holds their fields' nodes.
  loading.resolve((root, owns, downs) => heard.push([root, owns(card), owns(gone), owns({ dataset: { view: '1' } }),
    downs.map((d) => [d.pointerId, d.target.id, d.isPrimary]), written.length]));
  await settle();
  expect([loads.length, heard, written, guards]).toEqual([2, [[document, true, false, false, [[1, 'field', true], [3, 'field', false]], 2]],
    [['card', 'true'], ['link', 'true']], []]);
  // Once it listens, every write is the listener's.
  context.writeDraggable(plain, 'true');
  context.loadDragOut();
  expect([loads.length, written.at(-1)]).toEqual([2, ['plain', 'true']]);
});

test('a drag from a node that waits for the listener, or on the path of a contact in one, is cancelled, but another script\'s', () => {
  const { context, element, listener } = wasmGlue(() => new Promise(() => {}));
  const card = element('card', 1), image = element('image', 2, card, { localName: 'img' }), field = element('field', 3, card);
  const foreign = element('foreign', null, card, { explicit: true }), elsewhere = element('elsewhere', 6);
  const link = element('link', 7, null, { localName: 'a' }), linked = element('linked', 8, link, { localName: 'img' });
  const sibling = element('sibling', 9, link);
  const cancelled = (target) => { let prevented = false; listener('dragstart')({ target, preventDefault: () => (prevented = true) }); return prevented; };
  // Nothing waits yet.
  expect(cancelled(card)).toBe(false);
  context.writeDraggable(card, 'true');
  context.writeDraggable(linked, 'true');
  expect([cancelled(card), cancelled(image), cancelled(field), cancelled(foreign), cancelled(elsewhere)]).toEqual([true, true, true, false, false]);
  // The browser begins the drag at the link around a waiting image the mouse went down on, though a finger went down on
  // the link's other child after it.
  expect(cancelled(link)).toBe(false);
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: linked });
  listener('pointerdown')({ isPrimary: true, pointerId: 2, pointerType: 'touch', target: sibling });
  expect(cancelled(link)).toBe(true);
});

test('a listener that fails as it starts leaves nothing of itself, and the next try loads anew', async () => {
  let signal;
  const { context, exact, logs, timers, guards, element } = wasmGlue(() => Promise.resolve(exact.listenDragOut));
  exact.listenDragOut = (root, owns, downs, options) => { signal = options.signal; throw new Error('no document'); };
  const card = element('card', 1);
  context.writeDraggable(card, 'true');
  await settle();
  expect([logs, context.host.dragOutModule, context.host.dragOutListening, timers.length, signal.aborted, card.attributes.draggable,
    'listenDragOut' in exact, guards.length]).toEqual([['drag-out module: Error: no document'], null, undefined, 1, true, 'false', false, 6]);
});

// A small element tree: what `start` reads of the page, by the selectors it asks.
class El {
  constructor(localName, attrs = {}, parent = null) { Object.assign(this, { localName, attrs, parent, isConnected: true }); }
  getAttribute(name) { return this.attrs[name] ?? null; }
  setAttribute(name, value) { this.attrs[name] = value; }
  removeAttribute(name) { delete this.attrs[name]; }
  get parentElement() { return this.parent; }
  addEventListener(type, fn, options) { if (options?.once) throw new Error('a once listener ends at any event'); (this.listeners ??= {})[type] = fn; }
  removeEventListener(type, fn) { if (this.listeners?.[type] === fn) delete this.listeners[type]; }
  getBoundingClientRect() { return { left: 0, top: 0 }; }
  contains(other) { for (let at = other; at; at = at.parent) if (at === this) return true; return false; }
  matches(selector) {
    const a = this.attrs;
    return selector.split(',').some((part) => ({
      '[draggable="true"]': a.draggable === 'true', 'a[href]': this.localName === 'a' && 'href' in a,
      '[data-exact-on~="press"]': (a['data-exact-on'] ?? '').split(' ').includes('press'),
      '[contenteditable]:not([contenteditable="false"])': 'contenteditable' in a && a.contenteditable !== 'false',
    })[part.trim()] ?? part.trim() === this.localName);
  }
  closest(selector) { for (let at = this; at; at = at.parent) if (at.matches(selector)) return at; return null; }
}

test('the source is the innermost draggable node under the contact, and a press inside it keeps a mouse\'s', () => {
  const link = new El('a', { href: '/landmark/1016' });
  const picture = new El('img', { draggable: 'true', 'data-datatransfertext': 'the picture' }, link);
  // The browser begins the link's drag; the contact went down on the draggable picture inside it.
  const linked = dragstart(link);
  start(linked, BASE, { target: picture, mouse: true });
  expect([linked.defaultPrevented, linked.dataTransfer.data.get('text/plain'), linked.dataTransfer.calls.length]).toEqual([false, 'the picture', 1]);
  // Another pointer's down names another source: the drag is read from its own target.
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const other = dragstart(card);
  start(other, BASE, { target: picture, mouse: true });
  expect(other.dataTransfer.data.get('text/plain')).toBe('card');
  // A press inside the card keeps a mouse's contact: the card's drag, and a picture's in the press, are cancelled;
  // a touch's is not kept.
  const press = new El('button', { 'data-exact-on': 'press' }, card), icon = new El('img', {}, press);
  for (const target of [card, icon]) {
    const kept = dragstart(target);
    start(kept, BASE, { target: icon, mouse: true });
    expect(kept.defaultPrevented).toBe(true);
  }
  const touched = dragstart(card);
  start(touched, BASE, { target: icon, mouse: false });
  expect([touched.defaultPrevented, touched.dataTransfer.data.get('text/plain')]).toEqual([false, 'card']);
  // A link's own drag inside the card is the browser's.
  const inner = new El('a', { href: '/elsewhere' }, card), own = dragstart(inner);
  start(own, BASE, { target: inner, mouse: true });
  expect([own.defaultPrevented, [...own.dataTransfer.data.keys()]]).toEqual([false, []]);
});

test('a field holds the drag of every draggable node around it off for its own contact alone', () => {
  const heard = {}, root = { addEventListener: (type, fn) => (heard[type] = fn), defaultView: { addEventListener: (type, fn) => (heard[`window ${type}`] = fn) } };
  listen(root);
  const outer = new El('div', { draggable: 'true', 'data-datatransfertext': 'outer' });
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }, outer), field = new El('input', {}, card);
  const elsewhere = new El('div', {}), foreign = new El('div', { draggable: 'true' });
  const pointer = (type, pointerId, pointerType, target, more = {}) => heard[type]({ type, pointerId, pointerType, target, isPrimary: true, ...more });
  const states = () => [outer.attrs.draggable, card.attrs.draggable];
  pointer('pointerdown', 1, 'mouse', field);
  expect(states()).toEqual(['false', 'false']);
  // A touch's contact is its own: its up ends no hold of the mouse's.
  pointer('pointerdown', 2, 'touch', elsewhere);
  pointer('pointerup', 2, 'touch', elsewhere);
  // What the app writes meanwhile is the node's once the hold ends.
  writeDraggable(card, 'false');
  writeDraggable(card, 'true');
  expect(states()).toEqual(['false', 'false']);
  // A drag another element began, and its end, are not the contact's.
  heard.dragstart({ target: foreign, defaultPrevented: false, preventDefault() {} });
  heard.dragend({});
  expect(states()).toEqual(['false', 'false']);
  // The mouse's up went elsewhere: it moves with no button down.
  pointer('pointermove', 1, 'mouse', elsewhere, { buttons: 0 });
  expect(states()).toEqual(['true', 'true']);
  pointer('pointerdown', 1, 'mouse', field);
  heard['window blur']();
  expect(states()).toEqual(['true', 'true']);
});

/** `listen` on a stand-in document: its listeners by type, and a pointer event sent to one. */
function listening(owns) {
  const heard = {}, root = { addEventListener: (type, fn) => (heard[type] = fn), defaultView: { addEventListener: (type, fn) => (heard[`window ${type}`] = fn) } };
  listen(root, owns);
  const pointer = (type, pointerId, pointerType, target, more = {}) => heard[type]({ type, pointerId, pointerType, target, isPrimary: true, ...more });
  return { heard, pointer, drag: (target) => { const event = dragstart(target); heard.dragstart(event); return event; } };
}

test('a drag during a field\'s hold is read from the source its down found, while the app has it draggable', () => {
  const { pointer, drag } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const editor = new El('div', { contenteditable: '' }, card), image = new El('img', {}, editor);
  pointer('pointerdown', 1, 'mouse', image);
  expect(card.attrs.draggable).toBe('false');
  // The image's own drag inside the held card is cancelled, as any picture's in a keeper is.
  expect(drag(image).defaultPrevented).toBe(true);
  pointer('pointerup', 1, 'mouse', image);
  // Once the app turns the card's drag off, the image's drag is the browser's.
  pointer('pointerdown', 1, 'mouse', image);
  writeDraggable(card, 'false');
  const own = drag(image);
  expect([own.defaultPrevented, [...own.dataTransfer.data.keys()]]).toEqual([false, []]);
  pointer('pointerup', 1, 'mouse', image);
  expect(card.attrs.draggable).toBe('false');
});

test('a second contact joins a hold, and a drag two contacts could have begun ends neither', () => {
  const { heard, pointer, drag } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const first = new El('input', {}, card), second = new El('input', {}, card);
  pointer('pointerdown', 1, 'mouse', first);
  pointer('pointerdown', 2, 'touch', second);
  pointer('pointerup', 1, 'mouse', first);
  expect(card.attrs.draggable).toBe('false');
  pointer('pointerup', 2, 'touch', second);
  expect(card.attrs.draggable).toBe('true');
  const editor = new El('div', { contenteditable: '' }, card), link = new El('a', { href: '/1016' }, editor);
  pointer('pointerdown', 1, 'mouse', link);
  pointer('pointerdown', 2, 'touch', link);
  drag(link);
  heard.dragend({});
  expect(card.attrs.draggable).toBe('false');
  pointer('pointerup', 1, 'mouse', link);
  pointer('pointerup', 2, 'touch', link);
  expect(card.attrs.draggable).toBe('true');
});

test('a drag\'s end at its source out of the document ends its contact, as WebKit sends it', () => {
  const { pointer, drag } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }), editor = new El('div', { contenteditable: '' }, card);
  pointer('pointerdown', 1, 'mouse', editor);
  // The editor's own drag, its selected text's, goes; the app then replaces the editor.
  expect(drag(editor).defaultPrevented).toBe(false);
  Object.assign(editor, { parent: null, isConnected: false });
  // Another drag's end passing through the editor is not its own.
  editor.listeners.dragend({ target: new El('img', {}, editor) });
  expect(card.attrs.draggable).toBe('false');
  editor.listeners.dragend({ target: editor });
  expect(card.attrs.draggable).toBe('true');
});

test('another script\'s draggable the browser begins a drag at keeps its drag over the app\'s image inside it', () => {
  const link = new El('a', { href: '/1016', draggable: 'true' }), image = new El('img', { draggable: 'true', 'data-datatransfertext': 'the picture' }, link);
  const { pointer, drag } = listening((el) => el !== link);
  pointer('pointerdown', 1, 'mouse', image);
  const event = drag(link);
  expect([event.defaultPrevented, [...event.dataTransfer.data.keys()]]).toEqual([false, []]);
});

test('a field\'s contact joins a hold another keeps, whatever the app wrote meanwhile', () => {
  const { pointer } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const first = new El('input', {}, card), second = new El('input', {}, card);
  pointer('pointerdown', 1, 'mouse', first);
  writeDraggable(card, 'false');
  pointer('pointerdown', 2, 'touch', second);
  writeDraggable(card, 'true');
  pointer('pointerup', 1, 'mouse', first);
  expect(card.attrs.draggable).toBe('false');
  pointer('pointerup', 2, 'touch', second);
  expect(card.attrs.draggable).toBe('true');
});

test('a drag two contacts could have begun is read from the browser\'s target', () => {
  const { pointer, drag } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const background = new El('div', {}, card), image = new El('img', { draggable: 'true', 'data-datatransfertext': 'image' }, card);
  pointer('pointerdown', 1, 'mouse', background);
  pointer('pointerdown', 2, 'touch', image);
  expect(drag(card).dataTransfer.data.get('text/plain')).toBe('card');
});

test('a recorded source the app no longer owns gives a drag nothing', () => {
  let ours = true;
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }), label = new El('span', {}, card);
  const { pointer, drag } = listening(() => ours);
  pointer('pointerdown', 1, 'mouse', label);
  ours = false;
  const event = drag(label);
  expect([event.defaultPrevented, [...event.dataTransfer.data.keys()]]).toEqual([false, []]);
});

test('the drag\'s source stops hearing its end once the contact is over', () => {
  const { pointer, drag } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }), editor = new El('div', { contenteditable: '' }, card);
  pointer('pointerdown', 1, 'mouse', editor);
  drag(editor);
  expect(typeof editor.listeners.dragend).toBe('function');
  // Another listener cancelled the drag, so no `dragend` came; the contact's up ends it, and the listener with it.
  pointer('pointerup', 1, 'mouse', editor);
  expect(editor.listeners.dragend).toBeUndefined();
});

test('a field holds its node for every pointer, primary or not', () => {
  const { pointer } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const first = new El('input', {}, card), second = new El('input', {}, card);
  pointer('pointerdown', 1, 'touch', first);
  writeDraggable(card, 'false');
  pointer('pointerdown', 2, 'touch', second, { isPrimary: false });
  writeDraggable(card, 'true');
  pointer('pointerup', 1, 'touch', first);
  expect(card.attrs.draggable).toBe('false');
  pointer('pointerup', 2, 'touch', second, { isPrimary: false });
  expect(card.attrs.draggable).toBe('true');
});

test('a drag another listener cancels after this one leaves its source hearing nothing', async () => {
  const { pointer, drag } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }), editor = new El('div', { contenteditable: '' }, card);
  pointer('pointerdown', 1, 'mouse', editor);
  const event = drag(editor);
  event.preventDefault();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(editor.listeners.dragend).toBeUndefined();
  pointer('pointerup', 1, 'mouse', editor);
});

test('the end of a drag two contacts could have begun ends neither kept contact', () => {
  const { element, listener, context } = wasmGlue(() => new Promise(() => {}));
  const wrapper = element('wrapper', null, null, { explicit: true }), handle = element('handle', 2, wrapper), field = element('field', 3, wrapper);
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: handle });
  listener('pointerdown')({ isPrimary: true, pointerId: 2, pointerType: 'touch', target: field });
  listener('dragstart')({ target: wrapper, preventDefault() {} });
  listener('dragend')({ target: wrapper });
  expect([...context.host.dragOutDowns.keys()]).toEqual([1, 2]);
});

test('a node the app makes draggable around a field a contact is in is held for that contact', () => {
  const { pointer } = listening();
  const card = new El('div', { 'data-datatransfertext': 'card' }), field = new El('input', {}, card);
  pointer('pointerdown', 1, 'mouse', field);
  writeDraggable(card, 'true');
  expect(card.attrs.draggable).toBe('false');
  pointer('pointerup', 1, 'mouse', field);
  expect(card.attrs.draggable).toBe('true');
});

test('only the drag\'s own source ends its contact at the document', () => {
  const { heard, pointer, drag } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }), editor = new El('div', { contenteditable: '' }, card);
  pointer('pointerdown', 1, 'mouse', editor);
  drag(editor);
  heard.dragend({ target: new El('span', {}, editor) });
  expect(card.attrs.draggable).toBe('false');
  heard.dragend({ target: editor });
  expect(card.attrs.draggable).toBe('true');
});

test('a listener aborted gives back the holds it made', () => {
  const controller = new AbortController(), heard = {};
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }), field = new El('input', {}, card);
  listen({ addEventListener: (type, fn) => (heard[type] = fn) }, () => true, [{ pointerId: 1, pointerType: 'mouse', target: field, isPrimary: true }],
    { signal: controller.signal });
  expect(card.attrs.draggable).toBe('false');
  controller.abort();
  expect(card.attrs.draggable).toBe('true');
});

test('a drag recorded while the glue waits is its own down\'s, cancelled or not, and its end at a detached source counts', async () => {
  let loading = deferred();
  const heard = [];
  const { context, element, listener } = wasmGlue(() => loading.promise);
  const card = element('card', 1), field = element('field', 2, card), source = element('source', 3);
  const added = [];
  source.addEventListener = (type, fn) => added.push([type, fn]);
  source.removeEventListener = (type, fn) => added.splice(added.findIndex(([t, f]) => t === type && f === fn), 1);
  context.writeDraggable(card, 'true');
  // A mouse drag from elsewhere goes, its source is removed, and its end comes to the source alone.
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: source });
  listener('dragstart')({ target: source, preventDefault() {} });
  source.isConnected = false;
  added[0][1]({ target: source });
  // The same mouse then goes down in a waiting card's field: that contact is handed to the listener.
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: field });
  // A drag a later listener cancels is no drag, though the load lands before anything else runs.
  source.isConnected = true;
  listener('pointerdown')({ isPrimary: true, pointerId: 2, pointerType: 'touch', target: source });
  const cancelled = { target: source, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
  listener('dragstart')(cancelled);
  cancelled.preventDefault();
  loading.resolve((root, owns, downs) => heard.push(downs.map((d) => [d.pointerId, d.target.id])));
  await settle();
  expect([heard, added]).toEqual([[[[1, 'field'], [2, 'source']]], []]);
});

test('a non-primary field pointer the window\'s blur ends holds nothing later', () => {
  const { heard, pointer } = listening();
  const card = new El('div', { 'data-datatransfertext': 'card' }), field = new El('input', {}, card);
  pointer('pointerdown', 2, 'touch', field, { isPrimary: false });
  heard['window blur']();
  writeDraggable(card, 'true');
  expect(card.attrs.draggable).toBe('true');
});

test('a drag an earlier listener cancelled is no drag, and a dragend at its target ends no hold', () => {
  const { heard, pointer } = listening();
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' }), editor = new El('div', { contenteditable: '' }, card);
  pointer('pointerdown', 1, 'mouse', editor);
  heard.dragstart({ ...dragstart(editor), defaultPrevented: true });
  heard.dragend({ target: editor });
  expect([card.attrs.draggable, editor.listeners?.dragend]).toEqual(['false', undefined]);
  pointer('pointerup', 1, 'mouse', editor);
});

test('a contact handed over from the wait keeps the source the app had draggable at its down', async () => {
  let loading = deferred();
  const handed = [];
  const { context, element, listener } = wasmGlue(() => loading.promise);
  const card = element('card', 1), image = element('image', 2, card, { localName: 'img' });
  context.writeDraggable(card, 'true');
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: image });
  context.writeDraggable(image, 'true');
  loading.resolve((root, owns, downs) => handed.push(...downs));
  await settle();
  expect(handed.map((down) => [down.target.id, down.source?.id])).toEqual([['image', 'card']]);
  // The listener reads the drag from that source.
  const heard = {};
  const cardEl = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const imageEl = new El('img', { draggable: 'true', 'data-datatransfertext': 'image' }, cardEl);
  listen({ addEventListener: (type, fn) => (heard[type] = fn) }, () => true, [{ pointerId: 1, pointerType: 'mouse', target: imageEl, isPrimary: true, source: cardEl }]);
  const event = dragstart(imageEl);
  heard.dragstart(event);
  expect(event.dataTransfer.data.get('text/plain')).toBe('card');
});

test('the wait forgets a drag that a later listener cancels, or whose contact ends', () => {
  const { context, element, listener, timers } = wasmGlue(() => new Promise(() => {}));
  const source = element('source', 1), added = [];
  source.addEventListener = (type, fn) => added.push(fn);
  source.removeEventListener = (type, fn) => added.splice(added.indexOf(fn), 1);
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: source });
  const event = { target: source, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
  listener('dragstart')(event);
  event.preventDefault();
  timers.at(-1)[0]();
  expect([context.host.dragOutDrag, added.length]).toEqual([null, 0]);
  listener('dragstart')({ target: source, defaultPrevented: false, preventDefault() {} });
  listener('pointerup')({ pointerId: 1 });
  expect([context.host.dragOutDrag, added.length]).toEqual([null, 0]);
  // A drag already cancelled is not recorded at all.
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: source });
  listener('dragstart')({ target: source, defaultPrevented: true, preventDefault() {} });
  expect([context.host.dragOutDrag, added.length]).toEqual([null, 0]);
});

test('a down in another script\'s draggable inside a waiting card has no source, and a blur or the pointer\'s next down forgets its drag', () => {
  const { context, element, listener } = wasmGlue(() => new Promise(() => {}));
  const card = element('card', 1), wrapper = element('wrapper', null, card, { explicit: true }), image = element('image', 3, wrapper, { localName: 'img' });
  wrapper.getAttribute = (name) => (name === 'draggable' ? 'true' : null);
  context.writeDraggable(card, 'true');
  listener('pointerdown')({ isPrimary: true, pointerId: 1, pointerType: 'mouse', target: image });
  expect(context.host.dragOutDowns.get(1).source).toBe(null);
  const elsewhere = element('elsewhere', 4);
  listener('pointerdown')({ isPrimary: true, pointerId: 2, pointerType: 'mouse', target: elsewhere });
  listener('dragstart')({ target: elsewhere, defaultPrevented: false, preventDefault() {} });
  expect(context.host.dragOutDrag?.source).toBe(elsewhere);
  listener('pointerdown')({ isPrimary: true, pointerId: 2, pointerType: 'mouse', target: elsewhere });
  expect(context.host.dragOutDrag).toBe(null);
  listener('dragstart')({ target: elsewhere, defaultPrevented: false, preventDefault() {} });
  context.waitingBlur();
  expect(context.host.dragOutDrag).toBe(null);
});
