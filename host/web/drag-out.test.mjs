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
  const image = { closest: () => card };
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

test('the wasm glue keeps a node made draggable undraggable until the listener listens, and tries a failed load again', async () => {
  const loads = [], logs = [], heard = [], written = [], timers = [], document = {}, views = new Map(), retiredViews = new Set();
  let loading = deferred();
  const context = vm.createContext({ host: {}, document, views, retiredViews, log: (line) => logs.push(line),
    setTimeout: (fn, ms) => timers.push([fn, ms]), exact: { writeDraggable: (el, value) => written.push([el.id, value]) },
    loadAfterPaint: (...args) => { loads.push(args); return loading.promise; } });
  vm.runInContext(declaration('liveView') + declaration('loadDragOut') + declaration('writeDraggable'), context);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  const element = (id, view) => {
    const el = { id, dataset: { view: String(view) }, attributes: {}, setAttribute(name, v) { this.attributes[name] = v; },
      removeAttribute(name) { delete this.attributes[name]; } };
    views.set(view, el);
    return el;
  };
  const [card, link, plain, gone] = [element('card', 1), element('link', 2), element('plain', 3), element('gone', 4)];
  // A node made draggable stays `false`, so a link or an image drags nothing of its own; `false` and none are written.
  context.writeDraggable(card, 'true');
  context.writeDraggable(gone, 'true');
  context.writeDraggable(link, 'false');
  context.writeDraggable(plain, 'true');
  context.writeDraggable(plain, null);
  expect([card.attributes, link.attributes, plain.attributes]).toEqual([{ draggable: 'false' }, { draggable: 'false' }, {}]);
  expect(loads).toEqual([['./drag-out.js', 'listenDragOut']]);
  // A failed load is tried again later while a live node waits; a retired one waits no more.
  retiredViews.add(gone);
  loading.reject(new Error('import failed'));
  await settle();
  expect([logs, timers.map(([, ms]) => ms)]).toEqual([['drag-out module: Error: import failed'], [1000]]);
  loading = deferred();
  timers[0][0]();
  loading.resolve((root, owns) => heard.push([root, owns(card), owns(gone), owns({ dataset: { view: '1' } })]));
  await settle();
  expect([loads.length, heard, written]).toEqual([2, [[document, true, false, false]], [['card', 'true']]]);
  // Once it listens, every write is the listener's.
  context.writeDraggable(link, 'true');
  context.loadDragOut();
  expect([loads.length, written.at(-1)]).toEqual([2, ['link', 'true']]);
});

test('a listener that throws as it starts is a failed load', async () => {
  const logs = [], timers = [];
  const context = vm.createContext({ host: {}, document: {}, views: new Map(), retiredViews: new Set(), log: (line) => logs.push(line),
    setTimeout: (fn, ms) => timers.push(ms), exact: {}, loadAfterPaint: () => Promise.resolve(() => { throw new Error('no document'); }) });
  vm.runInContext(declaration('liveView') + declaration('loadDragOut') + declaration('writeDraggable'), context);
  context.writeDraggable({ dataset: { view: '1' }, setAttribute() {} }, 'true');
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect([logs, context.host.dragOutModule, context.host.dragOutListening]).toEqual([['drag-out module: Error: no document'], null, undefined]);
});

// A small element tree: what `start` reads of the page, by the selectors it asks.
class El {
  constructor(localName, attrs = {}, parent = null) { Object.assign(this, { localName, attrs, parent, isConnected: true }); }
  getAttribute(name) { return this.attrs[name] ?? null; }
  setAttribute(name, value) { this.attrs[name] = value; }
  removeAttribute(name) { delete this.attrs[name]; }
  get parentElement() { return this.parent; }
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

test('a drag during a field\'s hold is read from the source its down found', () => {
  const card = new El('div', { draggable: 'true', 'data-datatransfertext': 'card' });
  const editor = new El('div', { contenteditable: '' }, card), image = new El('img', {}, editor);
  const source = sourceOf(image, true);
  card.attrs.draggable = 'false';
  // The image's own drag inside the held card is cancelled, as any picture's in a keeper is.
  const event = dragstart(image);
  start(event, BASE, { target: image, mouse: true, source });
  expect(event.defaultPrevented).toBe(true);
  const recomputed = dragstart(image);
  start(recomputed, BASE, { target: image, mouse: true });
  expect(recomputed.defaultPrevented).toBe(false);
});
