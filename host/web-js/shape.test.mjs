// A lone UTF-16 surrogate is no string on the web, as on native hosts (LLP 1146 D6).
import { expect, test } from 'bun:test';
import { conforms, failureCode, loneSurrogate, roleIndex } from './shape.js';

test('a string with a lone surrogate does not conform, and is said', () => {
  const cut = '😀 reply'.slice(0, 1); // half an emoji, as `.slice(0, n)` through one leaves it
  expect(conforms('a 😀', 's')).toBe(true);
  expect(conforms(cut, 's')).toBe(false);
  expect(conforms(['ok', cut], '[s')).toBe(false);
  expect(conforms([cut], '{s}')).toBe(false);
  expect(loneSurrogate([['ok'], ['x', cut]])).toBe(true);
  expect(loneSurrogate(['a 😀', 3, null])).toBe(false);
});

// A fetch failure's kind is the module's text: one that names an inherited member is no kind of failure.
test('a fetch failure whose kind names an inherited member is a plain error', () => {
  expect(['Network', 'constructor', 'toString', '__proto__'].map(kind => failureCode({ name: 'FetchError', kind }))).toEqual(['offline', 'error', 'error', 'error']);
});

test('an accent fallback names a flat role and refuses nested tints without recursing', () => {
  const roles = [['AccentColor', '', true, true, 'AccentColor']];
  expect(roleIndex('var(--exact-accent, AccentColor)', roles)).toBe(0);
  expect(roleIndex('var(--exact-accent, var(--exact-accent, AccentColor))', roles)).toBe(-1);
  const nested = 'var(--exact-accent,'.repeat(50_000) + 'AccentColor' + ')'.repeat(50_000);
  expect(roleIndex(nested, roles)).toBe(-1);
});
