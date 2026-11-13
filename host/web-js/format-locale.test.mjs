// format-locale.js against the runner's oracle rows (runner/tests/it/format_locales.json, Intl's in Bun 1.4.2): each
// formatter in each locale through the record the compiler bakes, which the runner's test holds to the compiler's.
import { test, expect, mock } from 'bun:test';
import { readFileSync } from 'node:fs';

mock.module('./rt.js', () => ({ memo: (f) => f }));
mock.module('./navigation.js', () => ({ reportPlace: () => 'de-DE\0UTC\x001\0de-DE-u-hc-h12' }));
const roster = await import('./format.js');
const { formats, formatSlot, formatTag } = await import('./format-locale.js');
const oracle = JSON.parse(readFileSync(new URL('../../runner/tests/it/format_locales.json', import.meta.url), 'utf8'));
formats(Object.entries(oracle.records));
const arg = (a) => (a === '-0' ? -0 : a);

test('every formatter is Intl\'s in each locale, as the runner\'s', () => {
  const wrong = [];
  for (const [loc, name, args, want] of oracle.rows) {
    const got = roster[`x_${name}`](...args.map(arg), loc);
    if (got !== want) wrong.push(`${loc} ${name}(${JSON.stringify(args)}): ${JSON.stringify(got)}, Intl ${JSON.stringify(want)}`);
  }
  expect(wrong).toEqual([]);
  expect(oracle.rows.length).toBeGreaterThan(5000);
});

test('the format slot is the table with the hour cycle the page forces', () => {
  expect(formatTag('de', 'en-US-u-hc-h23')).toBe('de h23');
  expect(formatTag('de', 'en-US-u-rg-gbzzzz-hc-h12')).toBe('de h12');
  expect(formatTag('de-u-ca-gregory', 'en-US-u-hc-h12')).toBe('de-u-ca-gregory h12');
  expect(formatTag('de-u-hc-h12', 'en-US-u-hc-h23')).toBe('de-u-hc-h12');
  expect(formatTag('de', 'de-DE-u-hc-h11')).toBe('de');
  expect(formatTag('de', 'de-DE')).toBe('de');
  expect(formatSlot(() => 'fr')()).toBe('fr h12');
  // "" formats en-US, with no record.
  expect(roster.x_formatTime(0, 0, 'short', '')).toBe('12:00 AM');
});
