import { expect, test } from 'bun:test';
import { parseManifest } from './app-cli.mjs';

// A UTF-8 byte-order mark, which some Windows editors write, is named with its fix (expo/exact-build#190).
test('app.json with a byte-order mark is refused by name', () => {
  expect(parseManifest('{"app":{"id":"com.example.a"}}')).toEqual({ app: { id: 'com.example.a' } });
  expect(() => parseManifest('﻿{"app":{}}')).toThrow('byte-order mark (U+FEFF), which JSON does not allow; save it as UTF-8 without a BOM');
  expect(() => parseManifest('{')).toThrow();
});

// scripts/app.mjs keeps the same check for readManifest and gameDefaults.
test('the SDK side names a byte-order mark the same way', async () => {
  const { parseManifest: sdk } = await import('./app.mjs');
  expect(() => sdk('﻿{}')).toThrow('byte-order mark (U+FEFF)');
});
