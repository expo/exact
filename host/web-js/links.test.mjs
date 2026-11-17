import { expect, test } from 'bun:test';
import { createDocument } from './dom.js';
import { linkUse } from './links.js';

// LLP 1030.004 D1: a bound `target` reaches the DOM as a literal one does, so the browser opens the window it names.
test('a bound target is written to the link', () => {
  const document = createDocument(), { made, prop } = linkUse();
  const a = document.createElement('a');
  a.setAttribute('href', '/compose');
  made(a, { href: '/compose' });
  prop(a, 'target', 'compose');
  expect(a.getAttribute('target')).toBe('compose');
  expect(a.getAttribute('rel')).toBe(null);
  prop(a, 'target', '_BLANK');
  expect([a.getAttribute('target'), a.getAttribute('rel')]).toEqual(['_BLANK', 'noopener']);
  // A name an author cannot write literally names no window.
  prop(a, 'target', '_inspector');
  expect([a.getAttribute('target'), a.getAttribute('rel')]).toEqual(['_blank', 'noopener']);
  prop(a, 'target', null);
  expect(a.getAttribute('target')).toBe(null);
  // Unset again, a link out of the app opens a new browsing context by default.
  prop(a, 'href', 'https://example.com/');
  expect([a.getAttribute('target'), a.getAttribute('rel')]).toEqual(['_blank', 'external noopener']);
  prop(a, 'target', 'help');
  expect([a.getAttribute('target'), a.getAttribute('rel')]).toEqual(['help', null]);
});
