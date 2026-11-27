// The JS target's port of the runner's admission of a drag's data (LLP 1094.000 D2, D14): the same entries kept, the
// same names offered and the same refusals journaled as `contract/cli/tests/it/drag_out.rs`, `drag_names.rs` and
// `plan/src/drag_name.rs` assert, through the hooks `install` registers, which journal a refusal once while it stands
// and remove a prop left with nothing.
import { afterEach, test, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { admit, admitEntries, admitUrl, decodeEntries, install, nameRefusal, offeredName, shipped } from './drag.js';
import { createDocument } from './dom.js';
import { listOwner, viewChildren } from '../web/navigation/lists.js';
import { glueDeclarations } from '../web/tests/glue-fixture.mjs';
import { dragExpectation } from '../../scripts/agent-drag-out.mjs';

const declarations = (file, names) => glueDeclarations(readFileSync(new URL(file, import.meta.url), 'utf8'), names);

const controllers = [];
afterEach(() => {
  for (const controller of controllers.splice(0)) controller.abort();
});

function installOn(document, PropHooks = {}) {
  const controller = new AbortController(), listeners = [];
  controllers.push(controller);
  document.addEventListener = (type, handler, { capture, signal } = {}) => {
    const entry = { type, handler, capture: !!capture };
    listeners.push(entry);
    signal?.addEventListener('abort', () => listeners.splice(listeners.indexOf(entry), 1), { once: true });
  };
  install({ PropHooks, say() {}, document, signal: controller.signal });
  const fire = (type, target, props) => {
    const event = { type, target, ...props };
    for (const listener of listeners) if (listener.type === type) listener.handler(event);
  };
  return { PropHooks, listeners, fire, controller };
}

test('the rules keep shipped files and absolute URLs, and refuse the rest in the runner words', () => {
  for (const ok of ['assets/1016.jpg', 'assets/apple/1016-thumb.jpg', `assets/${'a'.repeat(128)}`]) expect(shipped(ok)).toBe(true);
  for (const no of ['assets/', 'assets/../x.jpg', 'assets/./x.jpg', 'assets//x.jpg', 'assets/a?.jpg', 'assets/é.jpg', `assets/${'a'.repeat(129)}`, 'img/x.jpg', 'app:/data/x.pdf']) expect(shipped(no)).toBe(false);
  const said = [];
  expect(admit('assets/1016.jpg\napp:/data/export.pdf\n\nhttps://example.com/a.jpg', (r) => said.push(r))).toBe('assets/1016.jpg');
  expect(admitUrl('/landmark/1016', (r) => said.push(r))).toBe('');
  expect(said).toEqual([
    'app:/data/export.pdf: an app:/ file cannot be dragged on the web',
    'https://example.com/a.jpg is neither a shipped assets/ file nor an app:/ path',
    "url is not an absolute http: or https: URL: another app cannot open this app's path",
  ]);
});

test('the hooks write what is admitted, remove an empty prop and journal a refusal once while it stands', () => {
  const PropHooks = {}, journal = [];
  install({ PropHooks, say: (line) => journal.push(line) });
  const attrs = new Map(), el = { setAttribute: (k, v) => attrs.set(k, v), removeAttribute: (k) => attrs.delete(k) };
  const files = PropHooks['data-datatransferfiles'], url = PropHooks['data-datatransferurl'];
  expect(files(el, 'assets/a.jpg\napp:/data/b.pdf')).toBe(true);
  expect(files(el, 'assets/a.jpg\napp:/data/b.pdf')).toBe(true);
  expect(attrs.get('data-datatransferfiles')).toBe('assets/a.jpg');
  files(el, 'app:/data/b.pdf');
  expect(attrs.has('data-datatransferfiles')).toBe(false);
  url(el, 'https://example.com/1016');
  expect(attrs.get('data-datatransferurl')).toBe('https://example.com/1016');
  expect(journal).toEqual(['drag: refused: app:/data/b.pdf: an app:/ file cannot be dragged on the web']);
  // A refusal is known by what was refused: a second of the same file, or another URL refused for the same reason, is journaled
  files(el, 'app:/data/b.pdf\napp:/data/b.pdf');
  url(el, '/one');
  url(el, '/two');
  expect(journal.slice(1)).toEqual([
    'drag: refused: app:/data/b.pdf: an app:/ file cannot be dragged on the web',
    "drag: refused: url is not an absolute http: or https: URL: another app cannot open this app's path",
    "drag: refused: url is not an absolute http: or https: URL: another app cannot open this app's path",
  ]);
});

test('a name is a portable basename and keeps the file’s suffix, as the plan’s rule has it', () => {
  for (const ok of ['Mount Fuji.jpg', 'St. Paul', 'Fuji', '.profile', 'a'.repeat(255), 'COM0.txt', 'CONSOLE']) expect(nameRefusal(ok)).toBe(null);
  for (const [bad, why] of [
    ['', 'a name is 1 to 255 bytes of UTF-8'], ['é'.repeat(128), 'a name is 1 to 255 bytes of UTF-8'],
    ['a\nb', 'a name holds no control character'], ['a\u007f', 'a name holds no control character'],
    ['photos/1016.jpg', 'a name holds none of < > : " / \\ | ? *'], ['a:b', 'a name holds none of < > : " / \\ | ? *'],
    [' a', 'a name neither begins nor ends with a space'], ['a ', 'a name neither begins nor ends with a space'],
    ['Fuji.', 'a name does not end with a dot'], ['.', 'a name does not end with a dot'], ['..', 'a name does not end with a dot'],
    ['con', 'Windows keeps that name for a device'], ['Nul.txt', 'Windows keeps that name for a device'],
    ['COM9.tar.gz', 'Windows keeps that name for a device'], ['lpt³', 'Windows keeps that name for a device'],
  ]) expect([bad, nameRefusal(bad)]).toEqual([bad, why]);
  for (const [file, name, offered] of [
    ['assets/apple/1016.jpg', 'Mount Fuji.jpg', 'Mount Fuji.jpg'], ['assets/apple/1016.jpg', 'Fuji.JPG', 'Fuji.JPG'],
    ['assets/apple/1016.jpg', 'Fuji.jpeg', 'Fuji.jpeg'], ['app:/data/notes.md', 'Notes.markdown', 'Notes.markdown'],
    ['assets/apple/1016.jpg', 'Mount Fuji', 'Mount Fuji.jpg'], ['assets/apple/1016.jpg', 'St. Paul', 'St. Paul.jpg'],
    ['assets/apple/1016.jpg', 'Fuji.png', 'Fuji.png.jpg'], ['assets/apple/1016.jpg', '.profile', '.profile.jpg'],
    ['assets/README', 'Read me.txt', 'Read me.txt'], ['assets/.profile', 'Profile', 'Profile'],
    ['assets/a.md', 'Notes.mar\u212adown', 'Notes.mar\u212adown.md'],
  ]) expect(offeredName(file, name)).toBe(offered);
  expect(decodeEntries('')).toEqual({ files: [], names: undefined });
  expect(decodeEntries('a\n\u001e')).toEqual({ files: ['a'], names: [] });
  expect(decodeEntries('a\n\u001e\n')).toEqual({ files: ['a'], names: [''] });
  expect(decodeEntries('assets%2Fa.jpg\nassets%2Fb.jpg\n\u001e\nB%20b\n')).toEqual({ files: ['assets/a.jpg', 'assets/b.jpg'], names: ['', 'B b'] });
  for (const broken of ['a', 'a\n\u001eb', '%4', '%zz\n', '%FF\n']) expect(decodeEntries(broken)).toBe(null);
});

test('the entries hook admits each file with its name, a refused file taking its name with it', () => {
  const PropHooks = {}, journal = [];
  install({ PropHooks, say: (line) => journal.push(line), document: null });
  const attrs = new Map(), el = { setAttribute: (k, v) => attrs.set(k, v), removeAttribute: (k) => attrs.delete(k) };
  const entries = (files, names) => files.map((f) => `${encodeURIComponent(f)}\n`).join('')
    + (names ? `\u001e${names.map((n) => `${encodeURIComponent(n)}\n`).join('')}` : '');
  const hook = PropHooks['data-datatransferentries'];
  hook(el, entries(['app:/data/a.jpg', 'assets/a\nb.jpg', 'assets/b.jpg', 'assets/c.jpg', 'assets/d.jpg', 'assets/e.jpg'],
    ['Gone', 'Split', '', 'Mount Fuji', 'Fuji.', 'Photo.jpeg', 'Extra']));
  expect([attrs.get('data-datatransferfiles'), attrs.get('data-datatransfernames')])
    .toEqual(['assets/b.jpg\nassets/c.jpg\nassets/d.jpg\nassets/e.jpg', '\nMount Fuji.jpg\n\nPhoto.jpeg']);
  expect(journal).toEqual([
    'drag: refused: app:/data/a.jpg: an app:/ file cannot be dragged on the web',
    'drag: refused: "assets/a\\nb.jpg": a file with a line break cannot be carried',
    'drag: refused: name "Fuji.": a name does not end with a dot',
    'drag: refused: name "Extra": there is no file in files= for it',
  ]);
  // A name changed beside a standing refusal journals only what is new
  hook(el, entries(['app:/data/a.jpg', 'assets/b.jpg'], ['', 'Second']));
  expect([attrs.get('data-datatransferfiles'), attrs.get('data-datatransfernames')]).toEqual(['assets/b.jpg', 'Second.jpg']);
  expect(journal.length).toBe(4);
  // All names empty carry none, and a cleared value removes both
  hook(el, entries(['assets/b.jpg'], ['']));
  expect(attrs.has('data-datatransfernames')).toBe(false);
  hook(el, null);
  expect([attrs.has('data-datatransferfiles'), attrs.has('data-datatransfernames')]).toEqual([false, false]);
  // A plan compiled before D14 binds its files alone, read as before, whatever they look like
  journal.length = 0;
  PropHooks['data-datatransferfiles'](el, entries(['assets/b.jpg'], ['Named']));
  expect(attrs.has('data-datatransferfiles')).toBe(false);
  expect(journal.length).toBe(2);
});

test('a render on the server admits the data and hears no drag', () => {
  const PropHooks = {}, journal = [];
  install({ PropHooks, say: (line) => journal.push(line), document: null });
  const attrs = new Map(), el = { setAttribute: (k, v) => attrs.set(k, v), removeAttribute: (k) => attrs.delete(k) };
  PropHooks['data-datatransferurl'](el, 'https://:443/');
  expect(attrs.has('data-datatransferurl')).toBe(false);
  expect(journal).toEqual(['drag: refused: url is not an absolute http: or https: URL: it names no host; write the host after `https://`']);
  expect(PropHooks.draggable).toBeUndefined();
});

test('install owns only marked drags and restores the card’s draggable attribute on abort', () => {
  const document = createDocument(), card = document.createElement('div'), foreign = document.createElement('div');
  const field = document.createElement('input');
  const { PropHooks, listeners, fire, controller } = installOn(document);
  expect(listeners.map(({ type, capture }) => [type, capture])).toEqual([
    ['pointerdown', true], ['mousedown', true], ['pointerup', true], ['pointercancel', true], ['mouseup', true],
    ['pointermove', true], ['dragstart', true], ['dragend', true], ['dragstart', true], ['dragstart', false],
  ]);
  document.root.append(card, foreign);
  card.append(field);
  card.$exactTree = true;
  field.closest = () => field;
  for (const element of [card, foreign]) {
    element.ownerDocument = document;
    element.getRootNode = () => document;
    element.matches = (selector) => selector === '[draggable="true" i]' && element.getAttribute('draggable') === 'true';
    element.closest = () => null;
    element.setAttribute('data-datatransfertext', 'card');
    expect(PropHooks.draggable(element, 'true')).toBe(true);
    expect([element.getAttribute('draggable'), element.exactDraggable]).toEqual(['true', 'true']);
    const data = new Map([['text/plain', 'browser']]);
    fire('dragstart', element, { dataTransfer: { items: { clear: () => data.clear() }, setData: (k, v) => data.set(k, v) } });
    expect(data.get('text/plain')).toBe(element === card ? 'card' : 'browser');
  }
  fire('pointerdown', field, { pointerType: 'mouse' });
  expect(card.getAttribute('draggable')).toBe('false');
  controller.abort();
  expect(card.getAttribute('draggable')).toBe('true');
  expect(listeners).toEqual([]);
});

test.each(['created', 'adopted'])('a literal draggable node goes through the prop hook when %s', (mode) => {
  const document = createDocument(), Adopt = mode === 'adopted', root = document.root;
  const attrs = { draggable: 'true' }, serverNode = document.createElement('div');
  if (Adopt) {
    serverNode.setAttribute('draggable', 'true');
    root.append(serverNode);
  }
  const ctx = vm.createContext({ document, Adopt, listOwner, Paint: null, Links: null, rel: (_, value) => value });
  vm.runInContext(declarations('./tree.js', ['h', 'at', 'adopt', 'NOT_DOM', 'PropHooks']) + '\nthis.PropHooks = PropHooks;', ctx);
  installOn(document, ctx.PropHooks);
  const card = ctx.h(root, 'div', 0, attrs, 0);
  expect(card).toBe(root.firstChild);
  if (Adopt) expect(card).toBe(serverNode);
  expect([card.getAttribute('draggable'), card.exactDraggable]).toEqual(['true', 'true']);
});

test('the JS agent reports only the app’s draggable declaration after a field release', async () => {
  const document = createDocument(), root = document.root;
  const { PropHooks, fire } = installOn(document);
  const card = document.createElement('div'), field = document.createElement('input'), foreign = document.createElement('img');
  field.closest = (selector) => field.matches(selector) ? field : null;
  root.append(card, foreign);
  card.append(field);
  card.$exactTree = true;
  card.setAttribute('data-datatransfertext', 'card');
  foreign.setAttribute('draggable', 'true');
  PropHooks.draggable(card, 'true');
  const views = new Map([[1, card], [2, field], [3, foreign]]);
  const exact = { views, viewId: (node) => [...views].find(([, el]) => el === node)[0], After: [], clock: { now: 0 } };
  const ctx = vm.createContext({ document, setFaultLog() {}, viewChildren, routeFlags: () => () => 0,
    unopened: () => false, HTMLIFrameElement: class {}, animationClock: () => ({ register() {}, seek() {} }), pieces() {}, watchGlides() {} });
  vm.runInContext(declarations('./agent.js', ['PROPS', 'SEMANTIC_TAGS', 'TYPES', 'gate', 'targetOf', 'install']), ctx);
  ctx.install(exact);
  for (const type of ['pointerdown', 'pointerup']) fire(type, field, { pointerType: 'mouse' });
  expect(card.getAttribute('draggable')).toBe('false');
  const tree = () => exact.agentSettled({ op: 'tree' });
  const { nodes } = await tree();
  expect(dragExpectation(nodes[0], { target: 'card', kind: 'text/plain', present: true, value: 'card' })).toBeNull();
  expect(nodes[2].props).not.toHaveProperty('draggable');
  PropHooks.draggable(card, null);
  expect(card.getAttribute('draggable')).toBe('false');
  expect((await tree()).nodes[0].props).not.toHaveProperty('draggable');
});

// A plan baked before Contract refused a drag's carrier words as an app's own (`data-datatransferurl`) writes them
// through the app's dataset, which leaves them to the page, whose hooks write only what they admit.
test("an app's own data-datatransfer words are not written as attributes", () => {
  const ctx = vm.createContext({});
  vm.runInContext(declarations('./dataset.js', ['CARRIERS', 'ds']) + '\nthis.ds = ds;', ctx);
  const attributes = new Map();
  const element = {
    getAttribute: (name) => attributes.get(name) ?? null,
    setAttribute: (name, value) => attributes.set(name, value),
    removeAttribute: (name) => attributes.delete(name),
  };
  ctx.ds(element, JSON.stringify({ datatransferurl: 'https://:443/', datatransferfiles: 'app:/data/x.pdf', datatransfertext: 'x', 'datatransfer-label': 'Card' }));
  expect([...attributes]).toEqual([['data-datatransfer-label', 'Card']]);
});
