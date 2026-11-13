import { test, expect } from 'bun:test';
import { mkdtempSync, mkdirSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { placeMacLibraries } from './bundle.mjs';

// The Mac app and its sample host (LLP 1031 D10) carry the same prebuilt frameworks: the app's library and its
// native modules' dynamic xcframeworks, which module code loads from `@loader_path/../Frameworks`.
test('places and signs the app library and every module library', () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-bundle-'));
  try {
    const lib = (name) => { const placed = join(root, 'built', `${name}.framework`); mkdirSync(placed, { recursive: true }); return { placed, link: name }; };
    const frameworks = join(root, 'Host.app/Contents/Frameworks'), signed = [];
    const placed = placeMacLibraries(frameworks, { library: lib('App'), moduleLibraries: [lib('Vision'), lib('Sync')] }, (p) => signed.push(p));
    expect(placed).toEqual(['App', 'Vision', 'Sync'].map((n) => join(frameworks, `${n}.framework`)));
    expect(signed).toEqual(placed);
    for (const p of placed) expect(existsSync(p)).toBe(true);
    expect(placeMacLibraries(frameworks, { library: null }, () => {})).toEqual([]);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
