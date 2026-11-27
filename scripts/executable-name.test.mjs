import { expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { APPLE_STATIC_TREES } from '../host/apple/assets.mjs';
import { appleExecutableName, executableName } from './app.mjs';
import { releaseArtifacts } from './exact.mjs';
import { within } from './paths.mjs';

// An iOS app named like one of its bundle root's own entries builds, under an executable that does not collide
// with it; a Mac or Windows executable, which shares no folder with them, keeps the app's name (expo/exact-build#89).
test("only a flat bundle's executable that would name one of the root's entries gets an App suffix", () => {
  const app = (displayName) => ({ displayName, id: 'com.example.x' });
  expect(appleExecutableName(app('Notes'), true)).toBe('Notes');
  for (const name of ['Assets', 'assets', 'Frameworks', 'PlugIns', 'Info.plist', 'PrivacyInfo.xcprivacy', 'ExactResources', 'Watch', 'en.lproj', 'AppIcon60x60@2x.png']) {
    expect(appleExecutableName(app(name), true)).toBe(`${name} App`);
    expect(appleExecutableName(app(name))).toBe(name);
    expect(executableName(app(name))).toBe(name);
  }
  // Every tree the build copies to the bundle root is reserved, whatever its case.
  for (const tree of APPLE_STATIC_TREES) expect(appleExecutableName(app(tree.toUpperCase()), true)).toBe(`${tree.toUpperCase()} App`);
  expect(appleExecutableName(app('Café'), true)).toBe('Café'.normalize('NFD'));
});

test('releaseArtifacts keeps every release artifact inside out when app.name has traversal or slashes', () => {
  const out = resolve('/tmp/exact-release-out/app');
  for (const displayName of ['../../escape', 'A/B Tester', '..']) {
    const paths = releaseArtifacts({ displayName, id: 'com.example.probe' }, out);
    for (const key of ['staged', 'symbols', 'zip', 'dmg']) {
      expect(within(out, paths[key])).toBe(true);
      expect(paths[key].startsWith(`${out}/`)).toBe(true);
    }
  }
});
