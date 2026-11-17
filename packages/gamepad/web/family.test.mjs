import { afterAll, beforeAll, expect, test } from 'bun:test';
import { implementation } from './index.js';

// A pad's family comes from its vendor id when the engine's id carries one, else from its name. Sony's pads name
// themselves "Wireless Controller", and that match ran before "xbox": "Xbox Wireless Controller" read as PlayStation
// and its buttons as Cross and Square.

let pads = [], saved;
beforeAll(() => {
  saved = globalThis.navigator;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { getGamepads: () => pads } });
});
afterAll(() => Object.defineProperty(globalThis, 'navigator', { configurable: true, value: saved }));

const pad = (id) => ({ index: 0, id, mapping: 'standard', connected: true, buttons: [], axes: [] });

for (const [id, family, faces] of [
  ['Xbox Wireless Controller', 'xbox', ['A', 'B', 'X', 'Y']],
  ['Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)', 'xbox', ['A', 'B', 'X', 'Y']],
  ['Wireless Controller', 'playstation', ['Cross', 'Circle', 'Square', 'Triangle']],
  ['DualSense Wireless Controller', 'playstation', ['Cross', 'Circle', 'Square', 'Triangle']],
  ['Pro Controller', 'nintendo', ['B', 'A', 'Y', 'X']],
  ['Generic USB Joystick', 'generic', ['A', 'B', 'X', 'Y']],
]) {
  test(`"${id}" is ${family}, with its labels`, () => {
    pads = [pad(id)];
    const impl = implementation({ changed() {}, emit() {}, foreground: () => true });
    const [read] = impl.getGamepads({}).gamepads;
    expect([read.family, read.labels.slice(0, 4)]).toEqual([family, faces]);
  });
}
