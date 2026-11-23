// The web's drag out of the page (LLP 1094.000 D6, D8): what a draggable node's `dragstart` gives the browser, read
// from the admitted `data-` attributes and the event alone, which drags are the node's, a field's down turning the
// node's drag off, and the wasm glue's wait for the listener. The same in the browsers, beside controls, a pan and a
// swipe, is `tests/drag-out.test.mjs`'s.
import { afterEach, test, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { deferred, source } from './glue-fixture.mjs';
import { glueDeclarations } from './tests/glue-fixture.mjs';
import { EXTENSION_TYPES } from '../../scripts/share-target.mjs';
import { carried, listen, start, typeOf, writeDraggable } from './drag-out.js';
import { downForDrag, elementOf, observeDragInput, pressedAt } from './drag-input.js';

const BASE = 'https://example.com/app/';
const node = (attrs) => ({
  parentElement: null,
  getAttribute: (name) => attrs[name] ?? null,
  getBoundingClientRect: () => ({ left: 10, top: 20 }),
});
// `clearData` clears the strings and leaves a file, as Chromium's does; `items.clear` clears both.
const transfer = () => {
  const data = new Map(), files = ['image/png'], calls = [];
  return { data, files, calls, clearData: () => data.clear(), items: { clear: () => { data.clear(); files.length = 0; } },
    setData: (k, v) => data.set(k, v), setDragImage: (...a) => calls.push(a), effectAllowed: 'uninitialized' };
};
/** A `dragstart` the browser began at `target`, at the point of the element the contact went down on (`at`). */
const dragstart = (target, at = target) => {
  target.ownerDocument = { elementFromPoint: (x, y) => (x === 50 && y === 70 ? at : null) };
  return { target, dataTransfer: transfer(), clientX: 50, clientY: 70, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
};

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

// A small element tree: what the listener reads of the page, by the selectors it asks.
class El {
  constructor(localName, attrs = {}, parent = null) { Object.assign(this, { localName, attrs, parent, nodeType: 1, isConnected: true }); }
  getAttribute(name) { return this.attrs[name] ?? null; }
  setAttribute(name, value) { this.attrs[name] = value; }
  removeAttribute(name) { delete this.attrs[name]; }
  get parentElement() { return this.parent; }
  getBoundingClientRect() { return { left: 10, top: 20 }; }
  addEventListener(type, fn) { (this.listeners ??= []).push([type, fn]); }
  removeEventListener(type, fn) { this.listeners = (this.listeners ?? []).filter(([t, f]) => t !== type || f !== fn); }
  fire(type, event) { event.type ??= type; for (const [t, fn] of [...(this.listeners ?? [])]) if (t === type) fn(event); }
  contains(other) { for (let at = other; at; at = at.parentElement) if (at === this) return true; return false; }
  matches(selector) {
    const a = this.attrs;
    return selector.split(',').some((part) => ({
      '[draggable="true" i]': a.draggable?.toLowerCase() === 'true', '[draggable="true"]': a.draggable === 'true',
      'a[href]': this.localName === 'a' && 'href' in a, 'img[data-symbol-source]': this.localName === 'img' && 'data-symbol-source' in a,
      '[data-exact-on~="press"]': (a['data-exact-on'] ?? '').split(' ').includes('press'),
      '[contenteditable]:not([contenteditable="false"])': 'contenteditable' in a && a.contenteditable !== 'false',
      '.exact-markdown-editor': (a.class ?? '').split(' ').includes('exact-markdown-editor'),
    })[part.trim()] ?? part.trim() === this.localName);
  }
  closest(selector) { for (let at = this; at; at = at.parent) if (at.matches(selector)) return at; return null; }
}
/** A node the app made draggable, carrying `attrs`. */
const declared = (localName, attrs = {}, parent = null) => {
  const el = new El(localName, attrs, parent);
  writeDraggable(el, 'true');
  return el;
};
const keys = (event) => [...event.dataTransfer.data.keys()];
/** A touch's down elsewhere: the drag's point is not its. */
const TOUCH = { mouse: false };

test('dragstart fills the transfer, leaves no picture file, pictures the node for a drag begun at an image inside it, and cancels an empty one', () => {
  const card = declared('div', { 'data-datatransferfiles': 'assets/a.png' }), image = new El('img', {}, card);
  const event = dragstart(image);
  event.dataTransfer.setData('text/uri-list', 'https://example.com/app/assets/a.png?from-the-img');
  start(event, BASE);
  expect([event.defaultPrevented, keys(event)]).toEqual([false, ['text/uri-list', 'text/html', 'DownloadURL']]);
  expect(event.dataTransfer.data.get('text/uri-list')).toBe('https://example.com/app/assets/a.png');
  expect([event.dataTransfer.files, event.dataTransfer.effectAllowed, event.dataTransfer.calls]).toEqual([[], 'copy', [[card, 40, 50]]]);
  // The browser's drag of the node itself pictures itself.
  const own = dragstart(card);
  start(own, BASE);
  expect(own.dataTransfer.calls).toEqual([]);
  const empty = dragstart(declared('div'));
  start(empty, BASE);
  expect(empty.defaultPrevented).toBe(true);
  // A drag another listener cancelled, a swipe's, is left as it is.
  const swiped = { ...dragstart(card), defaultPrevented: true };
  start(swiped, BASE);
  expect(keys(swiped)).toEqual([]);
});

test('the drag is the node\'s only when the nearest draggable is one the app made and owns', () => {
  const left = (event) => [event.defaultPrevented, keys(event)];
  const card = declared('div', { 'data-datatransfertext': 'card' });
  // Another script's draggable, in any case, around the app's image or on one of the app's nodes, keeps its drag.
  const wrapper = new El('div', { draggable: 'TRUE' }, card), wrapped = new El('img', {}, wrapper);
  const marked = new El('div', { 'data-datatransfertext': 'marked' }, card);
  marked.setAttribute('draggable', 'true');
  for (const target of [wrapper, wrapped, marked]) {
    const event = dragstart(target);
    start(event, BASE);
    expect(left(event)).toEqual([false, []]);
  }
  // A node the app no longer has draggable, or no longer owns, is not its; nor is a drag from no draggable at all.
  const was = declared('div', { 'data-datatransfertext': 'was' });
  writeDraggable(was, 'false');
  was.setAttribute('draggable', 'true');
  const unowned = dragstart(card), none = dragstart(new El('div')), stale = dragstart(was);
  start(unowned, BASE, () => false);
  start(none, BASE);
  start(stale, BASE);
  expect([left(unowned), left(none), left(stale)]).toEqual([[false, []], [false, []], [false, []]]);
  // An SVG element's `draggable` is not read: one another script marked is not the drag's source, the app's card is;
  // an HTML box the app made draggable in its `foreignObject` is.
  const svg = (name, attrs, parent) => Object.assign(new El(name, attrs, parent), { namespaceURI: 'http://www.w3.org/2000/svg' });
  const glyph = svg('svg', { draggable: 'TRUE' }, card), shape = svg('rect', {}, glyph), foreign = svg('foreignObject', {}, glyph);
  const fromSvg = dragstart(card, shape);
  start(fromSvg, BASE);
  const box = declared('div', { 'data-datatransfertext': 'box' }, foreign), fromBox = dragstart(box);
  start(fromBox, BASE);
  expect([fromSvg.dataTransfer.data.get('text/plain'), fromBox.dataTransfer.data.get('text/plain')]).toEqual(['card', 'box']);
  // A drag Chromium and WebKit begin at the marked SVG element is its script's, a press inside it notwithstanding.
  const pressing = svg('rect', { 'data-exact-on': 'press' }, glyph), atSvg = dragstart(glyph, pressing);
  start(atSvg, BASE);
  expect([atSvg.defaultPrevented, keys(atSvg)]).toEqual([false, []]);
  // A selection's drag begins at its text (Firefox), which is the browser's.
  const text = { nodeType: 3, parentElement: card }, selection = { ...dragstart(card), target: text };
  start(selection, BASE);
  expect(left(selection)).toEqual([false, []]);
});

test('what a mouse went down on keeps the node\'s drag from starting, and a keeper\'s own drag is the browser\'s', () => {
  const card = declared('div', { 'data-datatransfertext': 'card' });
  const press = new El('button', { 'data-exact-on': 'press' }, card), icon = new El('img', {}, press);
  const link = new El('a', { href: '/elsewhere' }, card), field = new El('input', {}, card);
  // The browser begins the card's drag at a press, or at the image in it: the drag is cancelled for a mouse.
  for (const [target, at] of [[card, press], [card, icon], [icon, icon]]) {
    const event = dragstart(target, at);
    start(event, BASE);
    expect(event.defaultPrevented).toBe(true);
  }
  // A touch's drag on a press drags the card.
  const touched = dragstart(card, press);
  start(touched, BASE, undefined, TOUCH);
  expect([touched.defaultPrevented, touched.dataTransfer.data.get('text/plain')]).toEqual([false, 'card']);
  // A link's or a field's own drag inside the card is the browser's (Firefox, Chromium); WebKit begins the card's drag
  // from a field, which is cancelled.
  for (const target of [link, field]) {
    const event = dragstart(target);
    start(event, BASE);
    expect([event.defaultPrevented, keys(event)]).toEqual([false, []]);
  }
  const fromField = dragstart(card, field);
  start(fromField, BASE, undefined, TOUCH);
  expect(fromField.defaultPrevented).toBe(true);
  // A point outside the node, the page having moved under it, reads the browser's own target.
  const elsewhere = dragstart(card, new El('button'));
  start(elsewhere, BASE);
  expect([elsewhere.defaultPrevented, elsewhere.dataTransfer.data.get('text/plain')]).toEqual([false, 'card']);
  // A press whose pressed look moved it from under the point still keeps the drag begun at its down's point, within
  // the half pixel Chromium rounds; a down at another point does not.
  const moved = dragstart(card, card);
  start(moved, BASE, undefined, { target: press, x: 50.5, y: 69.5, mouse: true });
  const other = dragstart(card, card);
  start(other, BASE, undefined, { target: press, x: 90, y: 70, mouse: true });
  expect([moved.defaultPrevented, other.defaultPrevented]).toEqual([true, false]);
});

test('an image the browser begins the drag at is the node\'s, but for one in a link inside it', () => {
  // A plain image in a draggable link the app made drags the link's data, pictured as the link.
  const link = declared('a', { href: '/landmark/1016', 'data-datatransfertext': 'link' }), image = new El('img', {}, link);
  const linked = dragstart(image);
  start(linked, BASE);
  expect([linked.dataTransfer.data.get('text/plain'), linked.dataTransfer.calls.length]).toEqual(['link', 1]);
  // A draggable image the app made inside a plain link carries its own declaration, when the browser begins the drag
  // at the image, or at the link around it (Firefox under the page's stylesheet), then pictured as the image.
  const plain = new El('a', { href: '/landmark/1016' }), picture = declared('img', { 'data-datatransfertext': 'the picture' }, plain);
  for (const [target, pictured] of [[picture, 0], [plain, 1]]) {
    const own = dragstart(target, picture);
    start(own, BASE);
    expect([own.dataTransfer.data.get('text/plain'), own.dataTransfer.calls.length]).toEqual(['the picture', pictured]);
  }
  // Another script's draggable the browser began the drag at keeps its drag over the app's image inside it.
  const theirs = new El('div', { draggable: 'true' }), inside = declared('img', { 'data-datatransfertext': 'inside' }, theirs);
  const over = dragstart(theirs, inside);
  start(over, BASE);
  expect([over.defaultPrevented, keys(over)]).toEqual([false, []]);
  // By touch, an image in a link inside the node is the link's drag.
  const card = declared('div', { 'data-datatransfertext': 'card' }), inner = new El('a', { href: '/x' }, card), thumb = new El('img', {}, inner);
  const touched = dragstart(thumb);
  start(touched, BASE, undefined, TOUCH);
  expect([touched.defaultPrevented, keys(touched)]).toEqual([false, []]);
});

/** An event target double that keeps the listeners `listen` adds, by type and phase. */
function eventTarget() {
  const heard = [];
  return { heard,
    addEventListener(type, fn, options) {
      const entry = [type, fn, !!options?.capture, options?.signal];
      heard.push(entry);
      options?.signal?.addEventListener('abort', () => heard.splice(heard.indexOf(entry), 1), { once: true });
    },
    fire(type, event, capture) {
      event.type ??= type;
      for (const [t, fn, c] of heard) if (t === type && (capture === undefined || c === capture)) fn(event);
    },
  };
}

const page = () => Object.assign(eventTarget(), { nodeType: 9, parentElement: null, baseURI: BASE, defaultView: eventTarget() });

const cleanups = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

function listenOn(root, input) {
  const controller = new AbortController();
  cleanups.push(() => {
    controller.abort();
    expect(root.heard).toEqual([]);
    expect(root.defaultView.heard).toEqual([]);
  });
  listen(root, undefined, { signal: controller.signal, input });
}

test('a primary down in a field turns the drag of every node the app made draggable around it off until the next one', () => {
  const root = page(), controller = new AbortController();
  const outer = declared('div'), card = declared('div', {}, outer), foreign = new El('div', { draggable: 'true' }, card);
  const field = new El('input', {}, foreign), other = new El('div'), unowned = declared('div');
  const range = new El('input', {}, unowned);
  listen(root, (el) => el !== unowned, { signal: controller.signal });
  cleanups.push(() => controller.abort());
  expect(root.heard.map(([type, , capture, signal]) => [type, capture, signal === controller.signal]))
    .toEqual([['pointerdown', true, true], ['mousedown', true, true], ['pointerup', true, true], ['pointercancel', true, true],
      ['mouseup', true, true], ['pointermove', true, true], ['dragstart', true, true], ['dragend', true, true],
      ['dragstart', true, true], ['dragstart', false, true]]);
  expect(root.defaultView.heard.map(([type, , capture, signal]) => [type, capture, signal === controller.signal]))
    .toEqual([['blur', false, true]]);
  const draggable = () => [outer, card, foreign, unowned].map((el) => el.getAttribute('draggable'));
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: range });
  expect(draggable()).toEqual(['true', 'true', 'true', 'true']);
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  expect(draggable()).toEqual(['false', 'false', 'true', 'true']);
  // The mouse's `mousedown` that follows writes nothing again.
  const writes = [];
  card.setAttribute = (name, value) => { writes.push(value); El.prototype.setAttribute.call(card, name, value); };
  root.fire('mousedown', { target: field });
  expect(writes).toEqual([]);
  // What the app writes meanwhile is kept on the node and written at the next primary down; another pointer's down
  // changes nothing.
  writeDraggable(card, 'false');
  writeDraggable(card, 'true');
  writeDraggable(outer, null);
  root.fire('pointerdown', { isPrimary: false, pointerType: 'touch', target: other });
  expect([draggable(), card.exactDraggable, outer.exactDraggable]).toEqual([['false', 'false', 'true', 'true'], 'true', null]);
  // A script's down is no down: it leaves the hold.
  root.fire('pointerdown', { isPrimary: true, isTrusted: false, pointerType: 'mouse', target: other });
  root.fire('mousedown', { isTrusted: false, target: other });
  expect(draggable()).toEqual(['false', 'false', 'true', 'true']);
  // A mouse's `mousedown`, which WebKit sends for a press after a drag without a `pointerdown`, counts.
  root.fire('pointerup', { pointerType: 'mouse', target: field });
  root.fire('mousedown', { target: other });
  expect(draggable()).toEqual([null, 'true', 'true', 'true']);
  // A second field down turns off again a node another script wrote `TRUE` on meanwhile.
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  card.setAttribute('draggable', 'TRUE');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  expect(card.getAttribute('draggable')).toBe('false');
  // A node the app has draggable gets the app's value whatever another script wrote meanwhile; one the app turned
  // undraggable meanwhile keeps a `draggable` another script wrote.
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  card.setAttribute('draggable', 'TRUE');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: other });
  expect(card.getAttribute('draggable')).toBe('true');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  writeDraggable(card, 'false');
  card.setAttribute('draggable', 'TRUE');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: other });
  expect(draggable()).toEqual([null, 'TRUE', 'true', 'true']);
  writeDraggable(card, 'true');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  writeDraggable(card, null);
  card.setAttribute('draggable', 'FALSE');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: other });
  expect(card.getAttribute('draggable')).toBe('FALSE');
  // A script's `FALSE` over the field's `false`, seen at a second field's down, makes the attribute the script's: its
  // `false` written after is not the down's to undo once the app has let the node go.
  writeDraggable(card, 'true');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  card.setAttribute('draggable', 'FALSE');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  writeDraggable(card, null);
  card.setAttribute('draggable', 'false');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: other });
  expect(card.getAttribute('draggable')).toBe('false');
  // A script's `false` written before the field's down is not the down's to undo once the app has let the node go.
  writeDraggable(card, 'true');
  card.setAttribute('draggable', 'false');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  writeDraggable(card, null);
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: other });
  expect(card.getAttribute('draggable')).toBe('false');
  writeDraggable(card, 'true');
  // An abort gives the nodes back their values.
  root.fire('pointerdown', { isPrimary: true, pointerType: 'pen', target: field });
  expect(draggable()).toEqual([null, 'false', 'true', 'true']);
  controller.abort();
  expect(draggable()).toEqual([null, 'true', 'true', 'true']);
});

// A held press keeps its field's card off until it ends, and only its own end ends it: a window blur ends every one, a
// mouse's ends at a chord's last `pointerup` whichever button it names, and another input's drag ends none.
test('a held press ends at a blur and at a chord\'s last pointerup, and not at another input\'s drag', () => {
  const root = page(), card = declared('div'), field = new El('input', {}, card), other = new El('div');
  listenOn(root);
  const elsewhere = () => root.fire('pointerdown', { isPrimary: true, pointerType: 'touch', target: other });
  root.fire('pointerdown', { isPrimary: true, pointerType: 'pen', target: field });
  root.defaultView.fire('blur', {});
  elsewhere();
  expect(card.getAttribute('draggable')).toBe('true');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', button: 0, pointerId: 1, target: field });
  root.fire('mousedown', { button: 2, target: field });
  root.fire('mouseup', { button: 0, target: field });
  root.fire('pointerup', { isPrimary: true, pointerType: 'mouse', button: 2, buttons: 0, pointerId: 1, target: field });
  elsewhere();
  expect(card.getAttribute('draggable')).toBe('true');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'pen', target: field });
  root.fire('dragstart', dragstart(other), true);
  root.fire('dragend', { target: other }, true);
  elsewhere();
  expect(card.getAttribute('draggable')).toBe('false');
});

// WebKit can send `dragend` only to a source a script removed during the drag.
test('a dragend only the drag\'s detached source hears ends the press it began from', () => {
  const root = page(), card = declared('div'), field = new El('div', { contenteditable: 'true' }, card);
  const theirs = new El('img', { draggable: 'true' }, field), other = new El('div');
  listenOn(root);
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', button: 0, pointerId: 1, target: theirs, clientX: 50, clientY: 70 });
  root.fire('dragstart', dragstart(theirs), true);
  theirs.isConnected = false;
  theirs.fire('dragend', {});
  root.fire('pointerdown', { isPrimary: true, pointerType: 'touch', target: other });
  expect(card.getAttribute('draggable')).toBe('true');
});

test('a node the app makes draggable around the last primary down\'s field is off until the next primary down', () => {
  const root = page(), card = declared('div'), field = new El('input', {}, card), other = new El('div');
  writeDraggable(card, 'false');
  listenOn(root);
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  writeDraggable(card, 'true');
  expect(card.getAttribute('draggable')).toBe('false');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: other });
  expect(card.getAttribute('draggable')).toBe('true');
});

test('a field\'s down keeps a held node the source, so an image in its field drags no more than one in any press', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' });
  const editor = new El('div', { contenteditable: '' }, card), picture = new El('img', {}, editor), input = new El('input', {}, card);
  listenOn(root);
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: picture, clientX: 50, clientY: 70 });
  expect(card.getAttribute('draggable')).toBe('false');
  const event = dragstart(picture);
  root.fire('dragstart', event);
  // A read-only Markdown field keeps the drag as an editable one does.
  const markdown = new El('div', { contenteditable: 'false', class: 'exact-markdown-editor' }, card), line = new El('div', {}, markdown);
  root.fire('pointerdown', { isPrimary: true, pointerType: 'touch', target: line, clientX: 50, clientY: 70 });
  const readOnly = dragstart(card, line);
  root.fire('dragstart', readOnly);
  expect([card.getAttribute('draggable'), readOnly.defaultPrevented]).toEqual(['false', true]);
  // The field's own drag of text stays the browser's.
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: input, clientX: 50, clientY: 70 });
  const text = dragstart(input);
  root.fire('dragstart', text);
  expect([event.defaultPrevented, text.defaultPrevented, keys(text)]).toEqual([true, false, []]);
  // An inner node the app turns undraggable meanwhile hands the source to the outer one its field's down turned off.
  const inner = declared('div', { 'data-datatransfertext': 'inner' }, card), deep = new El('div', { contenteditable: '' }, inner);
  const deeper = new El('img', {}, deep);
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: deeper, clientX: 50, clientY: 70 });
  writeDraggable(inner, 'false');
  const nested = dragstart(deeper);
  root.fire('dragstart', nested);
  expect(nested.defaultPrevented).toBe(true);
});

test('a press\'s own `mousedown` keeps its pointer, and another script\'s `false` is the app\'s to undo', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), press = new El('button', {}, card);
  const field = new El('input', {}, card), other = new El('div');
  listenOn(root);
  // A pen's press on a child press: its `mousedown` follows at its point, and the card's drag is not a mouse's.
  root.fire('pointerdown', { isPrimary: true, pointerType: 'pen', target: press, clientX: 50, clientY: 70 });
  root.fire('mousedown', { target: press, clientX: 50, clientY: 70 });
  const pen = dragstart(card, press);
  root.fire('dragstart', pen);
  expect([pen.defaultPrevented, pen.dataTransfer.data.get('text/plain')]).toEqual([false, 'card']);
  // Another script's `FALSE` on a node the app has draggable is written over at the next down after a field's, as the
  // app's node is the app's.
  card.setAttribute('draggable', 'FALSE');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: field });
  expect(card.getAttribute('draggable')).toBe('FALSE');
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: other });
  expect(card.getAttribute('draggable')).toBe('true');
});


test('the target\'s mark is read as the drag began, before its own listener changes it', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), theirs = new El('img', { draggable: 'true' }, card);
  listenOn(root);
  const event = dragstart(theirs);
  event.type = 'dragstart';
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && capture) fn(event);
  // The extension's own `dragstart` gives its payload and clears its mark until its drag ends.
  event.dataTransfer.setData('text/plain', 'theirs');
  theirs.removeAttribute('draggable');
  // The app makes a box draggable during another script's drag from it: the drag stays the script's.
  const box = new El('div', { draggable: 'true', 'data-datatransfertext': 'box' }, card), fromBox = dragstart(box);
  fromBox.type = 'dragstart';
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && capture) fn(fromBox);
  fromBox.dataTransfer.setData('text/plain', 'scripted');
  writeDraggable(box, 'true');
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && !capture) fn(fromBox);
  expect(fromBox.dataTransfer.data.get('text/plain')).toBe('scripted');
  // The app makes another script's draggable link around a plain image its own during that script's drag from the
  // image: the drag was decided as it began, the script's.
  const link = new El('a', { href: '/x', draggable: 'true', 'data-datatransfertext': 'link' }, card), image = new El('img', {}, link);
  const fromImage = dragstart(image);
  fromImage.type = 'dragstart';
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && capture) fn(fromImage);
  fromImage.dataTransfer.setData('text/plain', 'scripted');
  writeDraggable(link, 'true');
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && !capture) fn(fromImage);
  expect(fromImage.dataTransfer.data.get('text/plain')).toBe('scripted');
  // A drag that begins nothing is cancelled as the event begins, though a listener at its node stops it there; a trusted
  // event dispatched again by a script is no drag the second time.
  const press = new El('button', {}, card), kept = dragstart(card, press);
  kept.type = 'dragstart';
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && capture) fn(kept);
  expect(kept.defaultPrevented).toBe(true);
  // The data is given as the event begins, so a listener at the node that stops the event keeps none of it from the drag.
  const stoppedThere = dragstart(card, card);
  stoppedThere.type = 'dragstart';
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && capture) fn(stoppedThere);
  expect(stoppedThere.dataTransfer.data.get('text/plain')).toBe('card');
  // What a listener at the node writes over the node's data is written over again where the event reaches the document.
  const overwritten = dragstart(card, card);
  overwritten.type = 'dragstart';
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && capture) fn(overwritten);
  overwritten.dataTransfer.setData('text/plain', 'theirs');
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && !capture) fn(overwritten);
  expect(overwritten.dataTransfer.data.get('text/plain')).toBe('card');
  const again = dragstart(card, card);
  again.type = 'dragstart';
  for (const [type, fn] of root.heard) if (type === 'dragstart') fn(again);
  expect(again.dataTransfer.data.get('text/plain')).toBe('card');
  again.isTrusted = false;
  again.dataTransfer.data.clear();
  for (const [type, fn] of root.heard) if (type === 'dragstart') fn(again);
  expect([again.defaultPrevented, again.dataTransfer.data.size]).toEqual([false, 0]);
  // A script's own `dragstart`, with no transfer, is no drag.
  const synthetic = { ...dragstart(card), isTrusted: false, dataTransfer: null };
  start(synthetic, BASE);
  expect(synthetic.defaultPrevented).toBe(false);
  // It dispatches a `dragstart` of its own on another element meanwhile, which is read as that event began.
  const other = new El('div'), nested = dragstart(other);
  nested.type = 'dragstart';
  for (const [type, fn] of root.heard) if (type === 'dragstart') fn(nested);
  for (const [type, fn, capture] of root.heard) if (type === 'dragstart' && !capture) fn(event);
  expect([event.defaultPrevented, event.dataTransfer.data.get('text/plain')]).toEqual([false, 'theirs']);
});

test('the primary down says whether the drag it begins is a mouse\'s', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), press = new El('button', {}, card);
  listenOn(root);
  const drag = () => { const event = dragstart(card, press); root.fire('dragstart', event); return event.defaultPrevented; };
  root.fire('pointerdown', { isPrimary: true, pointerType: 'touch', target: press });
  const touch = drag();
  root.fire('pointerdown', { isPrimary: true, pointerType: 'mouse', target: press });
  expect([touch, drag()]).toEqual([false, true]);
});

test('a held mouse keeps its moved press after a touch tap, and release lets a pen drag at the same point', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), press = new El('button', {}, card);
  const other = new El('div');
  listenOn(root);
  const mouse = { target: press, pointerType: 'mouse', clientX: 50, clientY: 70 };
  const touch = { target: other, pointerType: 'touch', clientX: 5, clientY: 5 };
  root.fire('pointerdown', { ...mouse });
  root.fire('mousedown', { ...mouse });
  root.fire('pointerdown', { ...touch });
  root.fire('pointerup', { ...touch });
  root.fire('mousedown', { target: other, clientX: 5, clientY: 5 });
  root.fire('mouseup', { target: other, clientX: 5, clientY: 5 });
  const moved = dragstart(card, card);
  root.fire('dragstart', moved);
  expect(moved.defaultPrevented).toBe(true);

  root.fire('pointerup', { ...mouse });
  const afterRelease = dragstart(card, card);
  root.fire('dragstart', afterRelease);
  expect(afterRelease.dataTransfer.data.get('text/plain')).toBe('card');
  root.fire('pointerdown', { ...mouse, pointerType: 'pen' });
  const pen = dragstart(card, press);
  root.fire('dragstart', pen);
  expect(pen.dataTransfer.data.get('text/plain')).toBe('card');
});

test('a held pen drags from a button after the mouse clicks elsewhere', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), button = new El('button', {}, card);
  listenOn(root);
  const mouse = { pointerType: 'mouse', target: new El('div'), clientX: 5, clientY: 5 };
  root.fire('pointerdown', { pointerType: 'pen', target: button, clientX: 50, clientY: 70 });
  root.fire('pointerdown', { ...mouse });
  root.fire('pointerup', { ...mouse });
  const drag = dragstart(card, button);
  root.fire('dragstart', drag);
  expect(drag.dataTransfer.data.get('text/plain')).toBe('card');
});

test('a released touch at the same point cannot take the drag from a held mouse', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), button = new El('button', {}, card);
  listenOn(root);
  const point = { target: button, clientX: 50, clientY: 70 };
  root.fire('pointerdown', { ...point, pointerType: 'mouse' });
  root.fire('pointerdown', { ...point, pointerType: 'touch' });
  root.fire('pointerup', { ...point, pointerType: 'touch' });
  const drag = dragstart(card, card);
  root.fire('dragstart', drag);
  expect(drag.defaultPrevented).toBe(true);
});

test('the newest held press at the drag point wins when the last down was elsewhere', () => {
  const root = page(), input = observeDragInput(root), card = new El('div'), button = new El('button', {}, card);
  const point = { clientX: 50, clientY: 70 };
  root.fire('pointerdown', { ...point, target: card, pointerType: 'mouse' });
  root.fire('pointerdown', { ...point, target: button, pointerType: 'pen' });
  root.fire('pointerdown', { target: new El('div'), pointerType: 'touch', clientX: 5, clientY: 5 });
  expect(downForDrag(input, { ...point, target: card }).target).toBe(button);
});

test.each(['pen', 'touch'])('compatibility events from a %s that went down first leave a held mouse alone', (pointerType) => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  listenOn(root);
  const other = { target: new El('div'), clientX: 5, clientY: 5 };
  const mouse = { target: field, pointerType: 'mouse', pointerId: 1, clientX: 50, clientY: 70 };
  root.fire('pointerdown', { ...other, pointerType });
  if (pointerType === 'pen') root.fire('mousedown', { ...other });
  root.fire('pointerdown', { ...mouse });
  root.fire('mousedown', { ...mouse });
  // A delayed touch down comes after its pointerup. A pen can release at a different point from its down.
  const release = pointerType === 'pen' ? { target: new El('div'), clientX: 15, clientY: 15 } : other;
  root.fire('pointerup', { ...release, pointerType });
  if (pointerType === 'touch') root.fire('mousedown', { ...other });
  root.fire('mouseup', { ...release });
  root.fire('pointerdown', { ...other, pointerType });
  expect(card.getAttribute('draggable')).toBe('false');
  root.fire('mouseup', { ...mouse });
  root.fire('pointerup', { ...mouse, pointerId: 2 });
  expect(card.getAttribute('draggable')).toBe('false');
  root.fire('pointerup', { ...mouse });
  expect(card.getAttribute('draggable')).toBe('true');
});

test('a pen compatibility mouseup at its release point leaves a standalone mouse held', () => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  listenOn(root);
  const pen = { target: new El('div'), pointerType: 'pen', clientX: 5, clientY: 5 };
  const mouse = { target: field, clientX: 50, clientY: 70 };
  root.fire('pointerdown', { ...pen });
  root.fire('mousedown', { ...pen });
  root.fire('mousedown', { ...mouse });
  const release = { ...pen, target: new El('div'), clientX: 15, clientY: 15 };
  root.fire('pointerup', { ...release });
  root.fire('mouseup', { ...release });
  root.fire('pointerdown', { ...pen, pointerType: 'touch' });
  expect(card.getAttribute('draggable')).toBe('false');
  root.fire('mouseup', { ...mouse });
  expect(card.getAttribute('draggable')).toBe('true');
});

test('a mouseup over a held pen ends the standalone mouse press', () => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  const pen = { target: new El('div'), clientX: 5, clientY: 5 };
  listenOn(root);
  root.fire('mousedown', { target: field, clientX: 50, clientY: 70, timeStamp: 10 });
  root.fire('pointerdown', { ...pen, pointerType: 'pen', timeStamp: 20 });
  root.fire('mousedown', { ...pen, timeStamp: 21 });
  expect(card.attrs.draggable).toBe('false');
  root.fire('mouseup', { ...pen, timeStamp: 30 });
  expect(card.attrs.draggable).toBe('true');
});

test('a targetless mouseup at another point ends the standalone mouse press after a pen release', () => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  const pen = { target: new El('div'), clientX: 5, clientY: 5 };
  listenOn(root);
  root.fire('mousedown', { target: field, clientX: 50, clientY: 70, timeStamp: 10 });
  root.fire('pointerdown', { ...pen, pointerType: 'pen', timeStamp: 20 });
  root.fire('mousedown', { ...pen, timeStamp: 21 });
  root.fire('pointerup', { target: root, pointerType: 'pen', clientX: 15, clientY: 15, timeStamp: 30 });
  expect(card.attrs.draggable).toBe('false');
  root.fire('mouseup', { target: root, clientX: 50, clientY: 70, timeStamp: 40 });
  expect(card.attrs.draggable).toBe('true');
});

test('a cancelled dragstart keeps the pen’s mouseup paired and pointerup clears the drag press', () => {
  const root = page(), input = { held: new Map() }, card = declared('div'), empty = declared('div');
  const mouse = { target: new El('input', {}, card), clientX: 5, clientY: 5 };
  const pen = { target: empty, clientX: 50, clientY: 70 };
  listenOn(root, input);
  root.fire('mousedown', { ...mouse, timeStamp: 10 });
  root.fire('pointerdown', { ...pen, pointerType: 'pen', timeStamp: 20 });
  root.fire('mousedown', { ...pen, timeStamp: 21 });
  const drag = dragstart(empty);
  root.fire('dragstart', drag);
  expect(drag.defaultPrevented).toBe(true);
  expect(input.dragPress?.target).toBe(empty);
  root.fire('pointerup', { ...pen, pointerType: 'pen', timeStamp: 30 });
  expect(input.dragPress).toBeNull();
  root.fire('mouseup', { ...pen, timeStamp: 31 });
  expect(card.attrs.draggable).toBe('false');
  root.fire('mouseup', { ...mouse, timeStamp: 40 });
  expect(card.attrs.draggable).toBe('true');
});

test.each(['pointercancel', 'dragend'])('a pen’s %s discards its pending compatibility mousedown', (type) => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  listenOn(root);
  const point = { target: field, clientX: 50, clientY: 70 };
  root.fire('pointerdown', { ...point, pointerType: 'pen' });
  if (type === 'dragend') root.fire('dragstart', dragstart(field));
  root.fire(type, { ...point, pointerType: 'pen' });
  root.fire('mousedown', { ...point });
  root.fire('pointerdown', { target: card, pointerType: 'touch' });
  expect(card.attrs.draggable).toBe('false');
});

test('a touch’s delayed mouse events stay paired after a second primary touch goes down', () => {
  const root = page(), input = { held: new Map() }, firstCard = declared('div'), secondCard = declared('div');
  const first = { target: new El('input', {}, firstCard), clientX: 5, clientY: 5 };
  const second = { target: new El('input', {}, secondCard), clientX: 50, clientY: 70 };
  listenOn(root, input);
  root.fire('pointerdown', { ...first, pointerType: 'touch', isPrimary: true, pointerId: 1, timeStamp: 10 });
  root.fire('pointerup', { ...first, pointerType: 'touch', isPrimary: true, pointerId: 1, timeStamp: 20 });
  root.fire('pointerdown', { ...second, pointerType: 'touch', isPrimary: true, pointerId: 2, timeStamp: 30 });
  root.fire('mousedown', { ...first, timeStamp: 40 });
  expect(input.held.has('mouse')).toBe(false);
  root.fire('mouseup', { ...first, timeStamp: 41 });
  expect([firstCard.attrs.draggable, secondCard.attrs.draggable]).toEqual(['true', 'false']);
});

test.each(['expired', 'prevented'])('a compatibility down that is %s cannot swallow a standalone mousedown', (reason) => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  listenOn(root);
  const point = { target: field, clientX: 50, clientY: 70 };
  const touch = { ...point, pointerType: 'touch', timeStamp: 10, defaultPrevented: false };
  root.fire('pointerdown', touch);
  touch.defaultPrevented = reason === 'prevented';
  root.fire('pointerup', { ...touch, timeStamp: 20 });
  root.fire('mousedown', { ...point, timeStamp: reason === 'expired' ? 600 : 30 });
  root.fire('pointerdown', { target: card, pointerType: 'pen' });
  expect(card.getAttribute('draggable')).toBe('false');
  root.fire('mouseup', { ...point });
  expect(card.getAttribute('draggable')).toBe('true');
});

test.each(['blur', 'pointermove'])('a missed release ends at %s and the last field stays protected until a new down', (type) => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  listenOn(root);
  root.fire('pointerdown', { target: field, pointerType: 'mouse' });
  if (type === 'blur') root.defaultView.fire('blur', { isTrusted: true });
  else {
    root.fire('pointermove', { target: card, pointerType: 'mouse', buttons: 0, isTrusted: false });
    root.fire('pointermove', { target: card, pointerType: 'pen', buttons: 0, isTrusted: true });
    root.fire('pointermove', { target: card, pointerType: 'mouse', buttons: 1, isTrusted: true });
    root.fire('pointerdown', { target: card, pointerType: 'pen' });
    expect(card.getAttribute('draggable')).toBe('false');
    root.fire('pointerdown', { target: field, pointerType: 'mouse' });
    root.fire('pointermove', { target: card, pointerType: 'mouse', buttons: 0, isTrusted: true });
  }
  expect(card.getAttribute('draggable')).toBe('false');
  root.fire('pointerdown', { target: card, pointerType: 'touch' });
  expect(card.getAttribute('draggable')).toBe('true');
});

test('a synthetic blur leaves the mouse held in a field', () => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  listenOn(root);
  root.fire('pointerdown', { target: field, pointerType: 'mouse' });
  root.defaultView.fire('blur', { isTrusted: false });
  root.fire('pointerdown', { target: new El('div'), pointerType: 'pen' });
  expect(card.attrs.draggable).toBe('false');
  root.fire('pointerup', { target: field, pointerType: 'mouse' });
  expect(card.attrs.draggable).toBe('true');
});

test('releasing the left mouse button while the right stays down frees the field’s card for the next press', () => {
  const root = page(), input = { held: new Map() }, card = declared('div'), field = new El('input', {}, card);
  const mouse = { target: field, pointerType: 'mouse', isTrusted: true };
  listenOn(root, input);
  root.fire('pointerdown', { ...mouse, button: 0, buttons: 1 });
  root.fire('mousedown', { ...mouse, button: 0, buttons: 1 });
  root.fire('pointermove', { ...mouse, button: 2, buttons: 3 });
  root.fire('mousedown', { ...mouse, button: 2, buttons: 3 });
  expect(input.held.has('mouse')).toBe(true);
  root.fire('pointermove', { ...mouse, button: 0, buttons: 2 });
  root.fire('mouseup', { ...mouse, button: 0, buttons: 2 });
  root.fire('pointerdown', { target: new El('div'), pointerType: 'pen' });
  expect(card.attrs.draggable).toBe('true');
});

test.each(['pen', 'touch'])('dragend releases the %s that began the drag and keeps the held mouse', (pointerType) => {
  const root = page(), card = declared('div'), other = declared('div');
  const field = new El('input', {}, card), mouseField = new El('input', {}, other);
  listenOn(root);
  root.fire('pointerdown', { target: field, pointerType, clientX: 50, clientY: 70 });
  root.fire('dragstart', dragstart(field));
  root.fire('pointerdown', { target: mouseField, pointerType: 'mouse' });
  root.fire('dragend', { target: field });
  expect(card.getAttribute('draggable')).toBe('true');
  root.fire('pointerdown', { target: other, pointerType });
  expect(other.getAttribute('draggable')).toBe('false');
  root.fire('pointerup', { target: mouseField, pointerType: 'mouse' });
  // If pointerup already ended the dragging press, dragend leaves a newer press of that type held.
  const press = { target: field, pointerType, clientX: 50, clientY: 70 };
  root.fire('pointerdown', { ...press });
  root.fire('dragstart', dragstart(field));
  root.fire('pointerup', { ...press });
  root.fire('pointerdown', { ...press, target: mouseField });
  root.fire('dragend', { target: field });
  root.fire('pointerdown', { target: other, pointerType: 'mouse' });
  expect([card.getAttribute('draggable'), other.getAttribute('draggable')]).toEqual(['true', 'false']);
});

test.each(['pointerup', 'mouseup', 'dragend'])('%s forgets the mouse down target and point', (end) => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' });
  const link = new El('a', { href: '/x' }, card);
  listenOn(root);
  root.fire(end === 'mouseup' ? 'mousedown' : 'pointerdown', { target: link, pointerType: 'mouse', clientX: 50, clientY: 70 });
  if (end === 'dragend') root.fire('dragstart', dragstart(link));
  root.fire(end, { target: link, pointerType: 'mouse' });
  const drag = dragstart(card, card);
  root.fire('dragstart', drag);
  expect(drag.dataTransfer.data.get('text/plain')).toBe('card');
});

test('fields held by overlapping contacts stay protected until their contacts release', () => {
  const root = page(), mouseCard = declared('div'), touchCard = declared('div');
  const mouse = { target: new El('input', {}, mouseCard), pointerType: 'mouse' };
  const touch = { target: new El('input', {}, touchCard), pointerType: 'touch' };
  listenOn(root);
  root.fire('pointerdown', { ...mouse });
  root.fire('pointerdown', { ...touch });
  expect([mouseCard.attrs.draggable, touchCard.attrs.draggable]).toEqual(['false', 'false']);
  root.fire('pointerup', { ...mouse });
  expect([mouseCard.attrs.draggable, touchCard.attrs.draggable]).toEqual(['true', 'false']);
  root.fire('pointerup', { ...touch });
  expect(touchCard.attrs.draggable).toBe('false');
  root.fire('pointerdown', { target: new El('div'), pointerType: 'mouse' });
  expect(touchCard.attrs.draggable).toBe('true');
});

test('a secondary mouse button does not replace the last primary field down', () => {
  const root = page(), card = declared('div'), field = new El('input', {}, card);
  listenOn(root);
  root.fire('mousedown', { target: field, button: 0 });
  root.fire('mouseup', { target: field, button: 0 });
  root.fire('mousedown', { target: new El('div'), button: 2 });
  expect(card.attrs.draggable).toBe('false');
});

test('a down on a text node uses its parent element as the keeper', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), button = new El('button', {}, card);
  listenOn(root);
  root.fire('pointerdown', { target: { nodeType: 3, parentElement: button }, pointerType: 'mouse', clientX: 50, clientY: 70 });
  const drag = dragstart(card, card);
  root.fire('dragstart', drag);
  expect(drag.defaultPrevented).toBe(true);
});

test('a pen down still pairs with its mousedown when the target changes at that point', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' }), button = new El('button', {}, card);
  const child = new El('span', {}, button);
  listenOn(root);
  root.fire('pointerdown', { target: child, pointerType: 'pen', clientX: 50, clientY: 70 });
  child.parent = null;
  root.fire('mousedown', { target: button, clientX: 50, clientY: 70 });
  const drag = dragstart(card, button);
  root.fire('dragstart', drag);
  expect(drag.dataTransfer.data.get('text/plain')).toBe('card');
});

test('a detached card cancels its own drag and its plain image’s, leaving native link and field drags alone', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' });
  const image = new El('img', {}, card), theirs = new El('img');
  const link = new El('a', { href: '/elsewhere' }, card), field = new El('input', {}, card);
  listenOn(root);
  for (const [target, cancelled] of [[card, true], [image, true], [link, false], [field, false], [theirs, false]]) {
    target.isConnected = false;
    const drag = dragstart(target);
    root.fire('dragstart', drag, true);
    expect([drag.defaultPrevented, keys(drag)]).toEqual([cancelled, []]);
  }
});

// The drag already holds the card's data from capture, so a source a listener took out of the page cancels it: the
// card itself, or a native image in it, which held the card's place on the drag's path.
test('a source detached between capture and bubble cancels the drag', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' });
  const image = new El('img', {}, card);
  listenOn(root);
  for (const [source, removed] of [[card, card], [image, image]]) {
    source.isConnected = card.isConnected = true;
    const drag = dragstart(source);
    root.fire('dragstart', drag, true);
    expect(drag.dataTransfer.data.get('text/plain')).toBe('card');
    removed.isConnected = false;
    root.fire('dragstart', drag, false);
    expect(drag.defaultPrevented).toBe(true);
  }
});

test('a synthetic dragstart inside a trusted dragstart leaves its restoration pending', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' });
  listenOn(root);
  const drag = dragstart(card);
  root.fire('dragstart', drag, true);
  drag.dataTransfer.setData('text/plain', 'theirs');
  root.fire('dragstart', { ...dragstart(new El('div')), isTrusted: false });
  root.fire('dragstart', drag, false);
  expect(drag.dataTransfer.data.get('text/plain')).toBe('card');
});

test('bubble restores the capture payload even if a target listener changes the declaration', () => {
  const root = page(), card = declared('div', { 'data-datatransfertext': 'card' });
  listenOn(root);
  const drag = dragstart(card);
  root.fire('dragstart', drag, true);
  drag.dataTransfer.setData('text/plain', 'theirs');
  card.setAttribute('data-datatransfertext', 'changed');
  root.fire('dragstart', drag, false);
  expect(drag.dataTransfer.data.get('text/plain')).toBe('card');
});

test('navigation spares a marked HTML drag in a press and ignores an SVG draggable mark', () => {
  const document = page(), root = new El('main'), button = new El('button', {}, root);
  const ctx = vm.createContext({ document, addEventListener() {}, captureLink() {}, TABS: false, SPLITS: false, SHEETS: false,
    activationRequired: new WeakSet(), activated: new WeakSet() });
  const routes = readFileSync(new URL('./navigation/routes.js', import.meta.url), 'utf8');
  vm.runInContext(glueDeclarations(routes, ['AGENT_ADMITTED', 'navigation']) + '\nthis.navigation = navigation;', ctx);
  ctx.navigation.connect(root, () => {}, () => {});
  const html = new El('img', { 'data-symbol-source': 'symbol:star', draggable: 'TRUE' }, button);
  const svg = Object.assign(new El('svg', { draggable: 'true' }, button), { namespaceURI: 'http://www.w3.org/2000/svg' });
  const image = new El('img', { 'data-symbol-source': 'symbol:star' }, svg);
  const drags = [dragstart(html), dragstart(image)];
  for (const drag of drags) document.fire('dragstart', drag);
  expect(drags.map((drag) => drag.defaultPrevented)).toEqual([false, true]);
});

const DECLARATIONS = glueDeclarations(source, ['liveView', 'dragOutWait', 'waitingInput', 'waitingView',
  'loadDragOut', 'waits', 'waitingDrag', 'writeDraggable']);
/** The wasm glue's drag-out declarations in a context of their own, with its page's doubles. */
function wasmGlue(loadAfterPaint) {
  const logs = [], timers = [], views = new Map(), retiredViews = new Set();
  let point = null;
  const document = Object.assign(page(), { elementFromPoint: () => point }), exact = { listenDragOut: listen, writeDraggable };
  class Controller extends AbortController {
    constructor() { super(); cleanups.push(() => this.abort()); }
  }
  const context = vm.createContext({ host: {}, document, views, retiredViews, log: (line) => logs.push(line), loadAfterPaint, exact,
    downForDrag, elementOf, observeDragInput, pressedAt, AbortController: Controller, setTimeout: (fn, ms) => timers.push([fn, ms]) });
  vm.runInContext(DECLARATIONS, context);
  const element = (id, view, parent = null, localName = 'div', attrs = {}) => {
    const el = Object.assign(new El(localName, attrs, parent), { id, dataset: view == null ? {} : { view: String(view) } });
    if (view != null) views.set(view, el);
    return el;
  };
  const cancelled = (target, at = null) => {
    point = at;
    const event = { target, clientX: 50, clientY: 70, preventDefault() { this.defaultPrevented = true; } };
    document.fire('dragstart', event);
    return !!event.defaultPrevented;
  };
  return { context, document, exact, logs, timers, guards: document.heard, retiredViews, element, cancelled };
}

test('the wasm glue keeps a node made draggable waiting, `false`, until the listener listens, and tries a failed load again', async () => {
  const loads = [];
  let loading = deferred();
  const { context, document, exact, logs, timers, guards, retiredViews, element } = wasmGlue((...args) => { loads.push(args); return loading.promise; });
  const [card, link, plain, gone] = [element('card', 1), element('link', 2), element('plain', 3), element('gone', 4)];
  const field = element('field', null, card, 'input');
  document.fire('pointerdown', { target: field, pointerType: 'touch' });
  // A node made draggable waits `false`, so a link or an image drags nothing of its own, with the app's value on it;
  // `false` and none are written as they are.
  context.writeDraggable(card, 'true');
  context.writeDraggable(gone, 'true');
  context.writeDraggable(link, 'false');
  context.writeDraggable(plain, 'true');
  context.writeDraggable(plain, null);
  expect([card.attrs, link.attrs, plain.attrs, card.exactDraggable, plain.exactDraggable]).toEqual([{ draggable: 'false' }, { draggable: 'false' }, {}, 'true', null]);
  expect(loads).toEqual([['./drag-out.js', 'listenDragOut']]);
  // A failed load is tried again on one timer, from a URL of its own, while a live node waits; a node made draggable
  // meanwhile waits for it, and a retired one waits no more.
  loading.reject(new Error('import failed'));
  await context.host.dragOutModule;
  context.writeDraggable(link, 'true');
  retiredViews.add(gone);
  expect([logs, timers.map(([, ms]) => ms), loads.length]).toEqual([['drag-out module: Error: import failed'], [1000], 1]);
  loading = deferred();
  timers[0][0]();
  expect(loads.at(-1)).toEqual(['./drag-out.js?try=1', 'listenDragOut']);
  document.fire('pointerdown', { target: plain, pointerType: 'touch', isPrimary: false });
  document.fire('pointerdown', { target: link, pointerType: 'touch', isPrimary: false });
  document.fire('mousedown', { target: plain, isTrusted: false });
  const waitSignal = guards[0][3];
  loading.resolve(exact.listenDragOut);
  await context.host.dragOutModule;
  expect([card.attrs.draggable, link.attrs.draggable, gone.attrs.draggable]).toEqual(['false', 'true', 'false']);
  expect([waitSignal.aborted, guards.some(([, , , signal]) => signal === waitSignal)]).toEqual([true, false]);
  context.writeDraggable(card, null);
  document.fire('pointerup', { target: field, pointerType: 'touch' });
  document.fire('pointerdown', { target: plain, pointerType: 'mouse' });
  expect(card.getAttribute('draggable')).toBeNull();
  context.writeDraggable(plain, 'true');
  context.loadDragOut();
  expect([loads.length, plain.attrs.draggable]).toEqual([2, 'true']);
});

test('a drag that begins in a waiting node, or at an element around one, is cancelled, but another script\'s', () => {
  const { context, guards, element, cancelled } = wasmGlue(() => new Promise(() => {}));
  const card = element('card', 1), image = element('image', 2, card, 'img'), field = element('field', 3, card, 'input');
  const elsewhere = element('elsewhere', 6), link = element('link', 7, null, 'a', { href: '/x' }), linked = element('linked', 8, link, 'img');
  // Downs are already observed even though no drag starts in a waiting view yet.
  expect(guards.some(([type]) => type === 'pointerdown')).toBe(true);
  context.writeDraggable(card, 'true');
  context.writeDraggable(linked, 'true');
  // A drag begun in a waiting node, or at an element around one where its point is in the node, as the link around a
  // waiting image pressed on the image, is cancelled; pressed elsewhere on the link, the drag is the link's.
  expect([cancelled(card), cancelled(image), cancelled(field), cancelled(elsewhere), cancelled(link, linked), cancelled(link)])
    .toEqual([true, true, true, false, true, false]);
  // Another script's draggable, in any case, inside the waiting card keeps its drag, and so does an image in it.
  const wrapper = element('wrapper', null, card, 'div', { draggable: 'TRUE' }), wrapped = element('wrapped', 10, wrapper, 'img');
  expect([cancelled(wrapper), cancelled(wrapped)]).toEqual([false, false]);
  // A plain image in a `foreignObject` of an SVG element another script marked, inside the waiting card: Firefox ignores
  // the mark and begins the image's drag, which is cancelled; Chromium and WebKit begin the SVG element's, its script's.
  const NS = 'http://www.w3.org/2000/svg', mark = Object.assign(element('mark', null, card, 'svg', { draggable: 'true' }), { namespaceURI: NS });
  const object = Object.assign(element('object', null, mark, 'foreignObject'), { namespaceURI: NS }), plain = element('plain', 12, object, 'img');
  expect([cancelled(plain), cancelled(mark)]).toEqual([true, false]);
  // A script's `dragstart` around a waiting image is no drag.
  let scripted = false;
  for (const [type, fn] of guards) if (type === 'dragstart') fn({ target: link, isTrusted: false, clientX: 1, clientY: 2, preventDefault: () => (scripted = true) });
  expect(scripted).toBe(false);
  // A waiting node another script marked is still the app's: its drag is cancelled.
  linked.setAttribute('draggable', 'TRUE');
  expect(cancelled(linked)).toBe(true);
  // A waiting image of the app's inside another script's wrapper: the browser begins the wrapper's drag, which is its.
  const theirs = element('theirs', null, null, 'div', { draggable: 'TRUE' }), mine = element('mine', 11, theirs, 'img');
  context.writeDraggable(mine, 'true');
  expect(cancelled(theirs)).toBe(false);
});

test('the wasm wait prunes retired entries and respects a retired wrapper reclaimed by a script', () => {
  const { context, element, retiredViews, cancelled } = wasmGlue(() => new Promise(() => {}));
  const card = element('card', 1), retired = element('retired', 2, card);
  const wrapper = element('wrapper', 3), image = element('image', 4, wrapper, 'img');
  for (const el of [card, retired, wrapper, image]) context.writeDraggable(el, 'true');
  retiredViews.add(retired);
  retiredViews.add(wrapper);
  wrapper.setAttribute('draggable', 'TRUE');
  expect(cancelled(retired)).toBe(true);
  expect(cancelled(wrapper, image)).toBe(false);
  expect([...context.host.dragOutWaiting]).toEqual([card, image]);
});

test('the wasm wait uses the down target when a pressed image moves off the link label', () => {
  const { context, document, element, cancelled } = wasmGlue(() => new Promise(() => {}));
  const link = element('link', 1, null, 'a', { href: '/x' });
  const image = element('image', 2, link, 'img'), label = element('label', 3, link, 'span');
  context.writeDraggable(image, 'true');
  const down = (target) => document.fire('pointerdown', { target, pointerType: 'mouse', clientX: 50, clientY: 70 });
  down(image);
  expect(cancelled(link, label)).toBe(true);
  down(label);
  expect(cancelled(link, label)).toBe(false);
});

test('the wasm handoff keeps a held mouse and a pen down awaiting its compatibility mousedown', async () => {
  const loading = deferred();
  const { context, document, exact, element } = wasmGlue(() => loading.promise);
  const card = element('card', 1, null, 'div', { 'data-datatransfertext': 'card' });
  const button = element('button', 2, card, 'button'), other = element('other', 3, card, 'button');
  context.writeDraggable(card, 'true');
  const mouse = { target: button, pointerType: 'mouse', clientX: 50, clientY: 70 };
  const pen = { target: other, pointerType: 'pen', clientX: 5, clientY: 5 };
  document.fire('pointerdown', { ...mouse });
  document.fire('mousedown', { ...mouse });
  document.fire('pointerdown', { ...pen });
  document.fire('pointerdown', { target: card, pointerType: 'touch', isPrimary: false });
  loading.resolve(exact.listenDragOut);
  await context.host.dragOutModule;
  document.fire('mousedown', { target: other, clientX: 5, clientY: 5 });
  const penDrag = dragstart(card, other);
  penDrag.clientX = penDrag.clientY = 5;
  card.ownerDocument.elementFromPoint = () => other;
  document.fire('dragstart', penDrag);
  expect(penDrag.dataTransfer.data.get('text/plain')).toBe('card');
  const mouseDrag = dragstart(card, card);
  document.fire('dragstart', mouseDrag);
  expect(mouseDrag.defaultPrevented).toBe(true);
});

test('a listener that fails as it starts leaves nothing of itself, and the next try loads anew', async () => {
  let signal;
  const { context, exact, logs, timers, guards, element } = wasmGlue(() => Promise.resolve(exact.listenDragOut));
  exact.listenDragOut = (root, owns, options) => { signal = options.signal; };
  exact.writeDraggable = () => { throw new Error('no document'); };
  const card = element('card', 1);
  context.writeDraggable(card, 'true');
  await context.host.dragOutModule;
  expect([logs, context.host.dragOutModule, context.host.dragOutListening, timers.length, signal.aborted, card.attrs.draggable,
    'listenDragOut' in exact, guards.some(([type]) => type === 'pointerdown')])
    .toEqual([['drag-out module: Error: no document'], null, undefined, 1, true, 'false', false, true]);
});
