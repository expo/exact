import { expect, test } from 'bun:test';
import { parseManifest } from './app-cli.mjs';

test('the former CLI verb refuses with the new command, including help', async () => {
  const { spawnSync } = await import('node:child_process');
  const { fileURLToPath } = await import('node:url');
  const root = fileURLToPath(new URL('..', import.meta.url));
  for (const args of [['tamber'], ['tamber', '--help']]) {
    const wrapper = `const { run } = await import('./scripts/app-cli.mjs'); process.exit(await run({ dir: process.cwd(), name: 'fixture', exact2: process.cwd(), args: ${JSON.stringify(args)} }))`;
    for (const command of [['scripts/exact.mjs', ...args], ['-e', wrapper]]) {
      const result = spawnSync(process.execPath, command, { cwd: root, encoding: 'utf8' });
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('rename the tamber verb to forma');
    }
  }
});

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
