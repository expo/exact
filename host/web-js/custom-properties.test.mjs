// LLP 1123 D5 on the JS target: a node's bound custom properties, one row, reach its style one declaration at a time,
// and a name it no longer declares, or one an author may not declare, is not left behind.
import { expect, test } from 'bun:test';
import { declare } from './declarations.js';

test("a node's custom properties are written one declaration at a time, and one no longer declared goes", () => {
  const set = new Map();
  const e = { style: { setProperty: (k, v) => set.set(k, v), removeProperty: (k) => set.delete(k) } };
  declare(e, '--gap: 8px; --pad: calc(2 * var(--gap))');
  expect([...set]).toEqual([['--gap', '8px'], ['--pad', 'calc(2 * var(--gap))']]);
  // A `;` in a comment or a function is no boundary, and the comment stays, so no tokens join
  declare(e, '--pad: calc(1px /*;*/ + 2px); --exact-safe-area-inset-top: 1px; bad: 2px; --a\\?: 1/**/px');
  expect([...set]).toEqual([['--pad', 'calc(1px /*;*/ + 2px)'], ['--a?', '1/**/px']]);
  // A comment may stand around a name, and an escape CSS has no code point for is U+FFFD
  declare(e, '--keep: 2px; /* note */ --b/**/: 4px; --\\110000: 8px');
  expect([...set]).toEqual([['--keep', '2px'], ['--b', '4px'], ['--\ufffd', '8px']]);
  declare(e, null);
  expect(set.size).toBe(0);
});
