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

test('formatDate labels strict day keys without shifting their calendar day', () => {
  for (const key of ['2026-10-16', '2024-02-29', '2000-02-29', '2400-02-29', '0001-01-01', '0099-12-31', '9999-12-31']) {
    const ms = Date.parse(`${key}T00:00:00Z`);
    for (const loc of ['', 'de', 'fr', 'ar-EG']) {
      for (const style of ['medium', 'month-year', 'iso']) {
        const want = roster.x_formatDate(ms, 0, style, loc);
        for (const offset of [-1080, -720, -420, 0, 345.5, 840, 1080]) {
          expect(roster.x_formatDate(key, offset, style, loc)).toBe(want);
        }
      }
    }
  }
  for (const key of ['', '2026-02-29', '1900-02-29', '2100-02-29', '2024-04-31',
    '2024-00-01', '2024-13-01', '2024-01-00', '2024-01-32', '0000-01-01',
    '10000-01-01', '2024-2-09', '24-02-09', '2024/02/09', ' 2024-02-09',
    '2024-02-09 ', '2024-02-09\n', '2024-02-09T00:00:00Z', '1707436800000',
    '２０２４-02-09', '2024-٠٢-09']) {
    for (const loc of ['', 'de']) {
      for (const style of ['medium', 'month-year', 'iso']) {
        expect(roster.x_formatDate(key, 0, style, loc)).toBe('');
      }
    }
  }
  for (const offset of [NaN, Infinity, -1081, 1080.1]) {
    expect(roster.x_formatDate('2026-10-16', offset, 'medium', '')).toBe('');
  }
});
