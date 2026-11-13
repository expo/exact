// `exact install` and `exact uninstall` replace or remove only this app's
// bundle and shim. Every path is in a temporary directory the test made.
import { afterEach, expect, test } from 'bun:test';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { place, remove } from './exact-install.mjs';

const app = { name: 'notes', displayName: 'Notes', id: 'com.exact.notes', manifest: {} };
let dir;
afterEach(() => rmSync(dir, { recursive: true, force: true }));

/** A bundle with `id` and one file, `marker`, holding `text`. */
function bundle(path, id, text) {
  mkdirSync(resolve(path, 'Contents'), { recursive: true });
  writeFileSync(resolve(path, 'Contents/Info.plist'), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>${id}</string></dict></plist>
`);
  writeFileSync(resolve(path, 'Contents/marker'), text);
  return path;
}
/** A fresh directory: the build's bundle, `Applications/Notes.app` and `bin/notes`. */
function paths() {
  dir = mkdtempSync(resolve(tmpdir(), 'exact-install-'));
  mkdirSync(resolve(dir, 'Applications'));
  mkdirSync(resolve(dir, 'bin'));
  return { built: bundle(resolve(dir, 'build/Notes.app'), app.id, 'new'), target: resolve(dir, 'Applications/Notes.app'), shim: resolve(dir, 'bin/notes') };
}
const marker = (target) => readFileSync(resolve(target, 'Contents/marker'), 'utf8');

test.skipIf(process.platform !== 'darwin')('an install refuses another app, another command or a link, and changes nothing', () => {
  const cases = {
    'another app': ({ target }) => bundle(target, 'com.example.notes', 'theirs'),
    'a link to a bundle': ({ target }) => symlinkSync(bundle(resolve(dir, 'Theirs.app'), app.id, 'theirs'), target),
    'another command': ({ shim }) => writeFileSync(shim, '#!/bin/sh\necho theirs\n'),
    'a link to a command': ({ shim }) => { writeFileSync(resolve(dir, 'theirs'), 'theirs\n'); symlinkSync(resolve(dir, 'theirs'), shim); },
  };
  for (const [name, occupy] of Object.entries(cases)) {
    const { built, target, shim } = paths();
    occupy({ target, shim });
    const before = [target, shim].map((p) => existsSync(p) && lstatSync(p).isSymbolicLink());
    expect(() => place(app, built, target, shim), name).toThrow('nothing was installed');
    expect([target, shim].map((p) => existsSync(p) && lstatSync(p).isSymbolicLink()), name).toEqual(before);
    if (name === 'another app') expect(marker(target)).toBe('theirs');
    if (name === 'a link to a bundle') expect(marker(resolve(dir, 'Theirs.app'))).toBe('theirs');
    if (name === 'another command') expect(readFileSync(shim, 'utf8')).toContain('echo theirs');
    if (name === 'a link to a command') expect(readFileSync(resolve(dir, 'theirs'), 'utf8')).toBe('theirs\n');
    if (name.endsWith('command')) expect(existsSync(target), name).toBe(false);
    rmSync(dir, { recursive: true, force: true });
  }
});

test.skipIf(process.platform !== 'darwin')('an install replaces its own, and a failed copy leaves the one there', () => {
  const { built, target, shim } = paths();
  place(app, built, target, shim);
  expect(marker(target)).toBe('new');
  expect(readFileSync(shim, 'utf8')).toContain(`exec /usr/bin/open -a ${JSON.stringify(target)}`);
  expect(lstatSync(shim).mode & 0o777).toBe(0o755);
  writeFileSync(resolve(built, 'Contents/marker'), 'newer');
  place(app, built, target, shim);
  expect(marker(target)).toBe('newer');
  const shimBefore = readFileSync(shim, 'utf8');
  expect(() => place(app, resolve(dir, 'build/Missing.app'), target, shim)).toThrow('copying');
  expect(marker(target)).toBe('newer');
  expect(readFileSync(shim, 'utf8')).toBe(shimBefore);
  // A build that is another app's is never put in place.
  bundle(resolve(dir, 'build/Other.app'), 'com.example.other', 'other');
  expect(() => place(app, resolve(dir, 'build/Other.app'), target, shim)).toThrow('is not com.exact.notes');
  expect(marker(target)).toBe('newer');
  expect(existsSync(resolve(dir, 'Applications/.Notes.app.installing-' + process.pid))).toBe(false);
});

test.skipIf(process.platform !== 'darwin')('an uninstall removes only its own bundle and shim', () => {
  const { built, target, shim } = paths();
  bundle(target, 'com.example.notes', 'theirs');
  writeFileSync(resolve(dir, 'theirs'), 'theirs\n');
  symlinkSync(resolve(dir, 'theirs'), shim);
  expect(remove(app, target, shim).map((line) => line.split(' ')[0])).toEqual(['left', 'left']);
  expect(marker(target)).toBe('theirs');
  expect(readFileSync(shim, 'utf8')).toBe('theirs\n');
  rmSync(target, { recursive: true });
  rmSync(shim);
  place(app, built, target, shim);
  expect(remove(app, target, shim)).toEqual([`removed ${target}`, `removed ${shim}`]);
  expect([existsSync(target), existsSync(shim)]).toEqual([false, false]);
  expect(remove(app, target, shim)).toEqual([`not installed: ${target}`, `not installed: ${shim}`]);
});
