// The JS target's port of the runner's admission of a drag's data (LLP 1094.000 D2): the same entries kept and the
// same refusals journaled as `contract/cli/tests/it/drag_out.rs` asserts of the runner booted as the web, through
// the hooks `install` registers, which journal a value once while it stands and remove a prop left with nothing.
import { afterEach, test, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { admit, admitUrl, install, shipped } from './drag.js';
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

test('the hooks write what is admitted, remove an empty prop and journal a value once', () => {
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
  expect(journal).toEqual(['drag: refused: app:/data/b.pdf: an app:/ file cannot be dragged on the web', 'drag: refused: app:/data/b.pdf: an app:/ file cannot be dragged on the web']);
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
