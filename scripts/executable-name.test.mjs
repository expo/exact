import { expect, test } from 'bun:test';
import { executableName } from './app.mjs';

// An app named like one of the bundle's own entries builds, under an executable that does not collide with it
// (expo/exact-build#89).
test("an executable that would name one of the bundle's own entries gets an App suffix", () => {
  const named = (displayName) => executableName({ displayName, id: 'com.example.x' });
  expect(named('Notes')).toBe('Notes');
  expect(named('Café')).toBe('Café');
  for (const name of ['Assets', 'assets', 'Frameworks', 'PlugIns', 'Info.plist', 'Watch', 'deck', 'en.lproj', 'AppIcon60x60@2x.png', 'Shaders'])
    expect(named(name)).toBe(`${name} App`);
  // The file-name cleanup still runs first, and an empty name still falls back to the id's last part.
  expect(named('a/b')).toBe('a-b');
  expect(named('..')).toBe('x');
});
