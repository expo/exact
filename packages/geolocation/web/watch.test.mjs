import { afterEach, beforeEach, expect, test } from 'bun:test';
import { implementation } from './index.js';

// The first reader's options were the only ones the browser saw, and its fix answered every later reader: one asking
// for high accuracy and no cached fix got the first reader's five-minute-old coarse one (LLP 1133.002 D2, D5). Each
// set of options now has its own browser registration, which the browser holds to those options.

let registrations, saved;
beforeEach(() => {
  registrations = [];
  saved = globalThis.navigator;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { geolocation: {
    watchPosition(success, error, options) { registrations.push({ success, error, options, cleared: false }); return registrations.length; },
    clearWatch(id) { registrations[id - 1].cleared = true; },
  } } });
});
afterEach(() => Object.defineProperty(globalThis, 'navigator', { configurable: true, value: saved }));

const fix = (accuracy, timestamp) => ({ coords: { latitude: 1, longitude: 2, accuracy }, timestamp });
const coarse = { enableHighAccuracy: false, maximumAge: 600000 }, precise = { enableHighAccuracy: true, maximumAge: 0, timeout: 0 };
const settled = (promise) => Promise.race([promise, Bun.sleep(5).then(() => 'pending')]);

test('readers with different options each have the browser hold them to their own', async () => {
  const impl = implementation({ changed() {} });
  const first = impl.watchPosition(coarse);
  registrations[0].success(fix(3000, 1));
  expect((await first).coords.accuracy).toBe(3000);
  const second = impl.watchPosition(precise);
  expect(registrations.map((r) => r.options)).toEqual([coarse, precise]);
  expect(await settled(second)).toBe('pending');
  // timeout=0 with no eligible cached fix: the browser fails this registration, and only it.
  registrations[1].error({ code: 3 });
  expect(await second).toEqual({ error: 'TIMEOUT' });
  expect((await impl.watchPosition(coarse)).coords.accuracy).toBe(3000);
  expect(await impl.watchPosition(precise)).toEqual({ error: 'TIMEOUT' });
  registrations[1].success(fix(5, 2));
  expect((await impl.watchPosition(precise)).coords.accuracy).toBe(5);
  expect(registrations.length).toBe(2);
});

test('a reader whose options change is registered anew, and the old options released', async () => {
  const impl = implementation({ changed() {} });
  impl.watchPosition(coarse);
  impl.watchPosition(precise);
  impl.releaseArgs('watchPosition', { ...coarse, enableHighAccuracy: false, timeout: null });
  expect(registrations.map((r) => r.cleared)).toEqual([true, false]);
  // The stronger reader's release leaves the other's registration running.
  impl.watchPosition(coarse);
  impl.releaseArgs('watchPosition', precise);
  expect(registrations.map((r) => r.cleared)).toEqual([true, true, false]);
  impl.watchPosition(precise);
  impl.release('watchPosition');
  expect(registrations.map((r) => r.cleared)).toEqual([true, true, true, true]);
});
