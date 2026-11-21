// The web's drag out of the page (LLP 1094.000 D6): what a draggable node's `dragstart` gives the browser, read
// from the admitted `data-` attributes, and a node that carries nothing beginning no drag. Which contacts the node
// takes, beside controls, a pan and a swipe, is `tests/drag-out.test.mjs`'s, in the browsers.
import { test, expect } from 'bun:test';
import vm from 'node:vm';
import { declaration, deferred } from './glue-fixture.mjs';
import { EXTENSION_TYPES } from '../../scripts/share-target.mjs';
import { carried, start, typeOf } from './drag-out.js';

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
});

test('the wasm glue loads the listener once, for the first node that turns draggable, and again after a failed load', async () => {
  const loads = [], logs = [], heard = [], document = {};
  let loading = deferred();
  const context = vm.createContext({ host: {}, document, log: (line) => logs.push(line),
    loadAfterPaint: (...args) => { loads.push(args); return loading.promise; } });
  vm.runInContext(declaration('loadDragOut'), context);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  context.loadDragOut();
  context.loadDragOut();
  expect(loads).toEqual([['./drag-out.js', 'listenDragOut']]);
  loading.reject(new Error('import failed'));
  await settle();
  expect(logs).toEqual(['drag-out module: Error: import failed']);
  loading = deferred();
  context.loadDragOut();
  loading.resolve((root) => heard.push(root));
  await settle();
  context.loadDragOut();
  expect([loads.length, heard]).toEqual([2, [document]]);
});

// A small element tree: what `start` reads of the page, by the selectors it asks.
class El {
  constructor(localName, attrs = {}, parent = null) { Object.assign(this, { localName, attrs, parent, isConnected: true }); }
  getAttribute(name) { return this.attrs[name] ?? null; }
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
