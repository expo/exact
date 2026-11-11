import { expect, test } from 'bun:test';
import { project, unproject } from '../web/index.js';

// A pan unprojects the new centre once per pointer move, so the longitude has to come back as it went in.
test('unproject returns the longitude project was given', () => {
  for (const lon of [-179.5, -122.42, -74.006, -0.5, 0, 0.5, 13.4, 100, 151.2, 179.5]) {
    for (const z of [0, 4, 12]) {
      const [x, y] = project(40, lon, z);
      expect(unproject(x, y, z)[1]).toBeCloseTo(lon, 6);
    }
  }
});

test('a point past the antimeridian wraps back into [-180, 180)', () => {
  const s = 256 * 2 ** 3, [, y] = project(0, 0, 3);
  expect(unproject(s + s / 4, y, 3)[1]).toBeCloseTo(-90, 6);
  expect(unproject(-s / 4, y, 3)[1]).toBeCloseTo(90, 6);
});
