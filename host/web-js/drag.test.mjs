// The JS target's port of the runner's admission of a drag's data (LLP 1094.000 D2): the same entries kept and the
// same refusals journaled as `contract/cli/tests/it/drag_out.rs` asserts of the runner booted as the web, through
// the hooks `install` registers, which journal a value once while it stands and remove a prop left with nothing.
import { test, expect } from 'bun:test';
import { admit, admitUrl, install, shipped } from './drag.js';

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

test('install hears the contacts and the drags on the page\'s document', () => {
  const heard = [], PropHooks = {};
  install({ PropHooks, say() {}, document: { addEventListener: (type, _, options) => heard.push([type, !!options?.capture]),
    defaultView: { addEventListener: (type) => heard.push([`window ${type}`, false]) } } });
  expect(heard).toEqual([['pointerdown', true], ['pointerup', true], ['pointercancel', true], ['pointermove', true],
    ['window blur', false], ['dragend', true], ['dragstart', false]]);
  // The page writes `draggable` through the listener, which holds a node's drag off for a field's contact.
  const el = { setAttribute: (name, value) => heard.push([name, value]) };
  expect(PropHooks.draggable(el, 'true')).toBe(true);
  expect(heard.at(-1)).toEqual(['draggable', 'true']);
});
