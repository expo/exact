import { test, expect } from 'bun:test';
import { closeSync, fstatSync, mkdtempSync, mkdirSync, existsSync, openSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { placeMacDylibs, placeMacLibraries } from './bundle.mjs';

test('replacing package and service dylibs preserves the old open inode', () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-dylibs-')), descriptors = [];
  try {
    const built = join(root, 'built.dylib');
    const libraries = ['libPackage.dylib', 'libService.dylib'].map(load => ({ built, load }));
    writeFileSync(built, 'first build');
    placeMacDylibs(root, libraries);
    for (const library of libraries) descriptors.push(openSync(join(root, library.load), 'r'));
    writeFileSync(built, 'second build');
    placeMacDylibs(root, libraries);
    for (const [i, library] of libraries.entries()) {
      const destination = join(root, library.load);
      expect(statSync(destination).ino).not.toBe(fstatSync(descriptors[i]).ino);
      expect(readFileSync(descriptors[i], 'utf8')).toBe('first build');
      expect(readFileSync(destination, 'utf8')).toBe('second build');
    }
  } finally {
    for (const descriptor of descriptors) closeSync(descriptor);
    rmSync(root, { recursive: true, force: true });
  }
});

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
