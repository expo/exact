import { afterEach, beforeEach, expect, test } from 'bun:test';
import { implementation } from './index.js';

// A battery read waits on `navigator.getBattery()`. Its last reader may be released meanwhile (LLP 1133.031 D5), and
// the page runtime then has no reader left to release the subscription that read would make: it answers, but
// subscribes nothing.

let saved, pending;
const manager = () => {
  const listeners = new Map();
  return { level: 0.5, charging: false, listeners, addEventListener: (e, f) => listeners.set(e, f), removeEventListener: (e) => listeners.delete(e) };
};
beforeEach(() => {
  saved = globalThis.navigator;
  pending = [];
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {
    getBattery: () => new Promise((resolve) => pending.push(resolve)),
  } });
});
afterEach(() => Object.defineProperty(globalThis, 'navigator', { configurable: true, value: saved }));

test('a read released before the browser answers subscribes nothing', async () => {
  const changed = [], device = implementation({ changed: (t) => changed.push(t) }), m = manager();
  const read = device.getBattery();
  device.release('getBattery');
  pending[0](m);
  expect(await read).toEqual({ charging: false, level: 0.5 });
  expect([...m.listeners.keys()]).toEqual([]);
});

test('a read answered, then released, subscribes and then stops', async () => {
  const changed = [], device = implementation({ changed: (t) => changed.push(t) }), m = manager();
  const read = device.getBattery();
  pending[0](m);
  await read;
  expect([...m.listeners.keys()]).toEqual(['levelchange', 'chargingchange']);
  m.level = 0.7;
  m.listeners.get('levelchange')();
  expect(changed).toEqual(['device.getBattery']);
  device.release('getBattery');
  expect([...m.listeners.keys()]).toEqual([]);
});

test('a release, then a new read, before the first answers: only the new read subscribes', async () => {
  const device = implementation({ changed() {} }), old = manager(), current = manager();
  const first = device.getBattery();
  device.release('getBattery');
  const second = device.getBattery();
  pending[0](old);
  await first;
  expect([...old.listeners.keys()]).toEqual([]);
  pending[1](current);
  await second;
  expect([...current.listeners.keys()]).toEqual(['levelchange', 'chargingchange']);
});
