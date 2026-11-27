import { afterEach, beforeEach, expect, test } from 'bun:test';
import { implementation } from './index.js';

// The first reader's options were the only ones the browser saw, and its fix answered every later reader: one asking
// for high accuracy and no cached fix got the first reader's five-minute-old coarse one (LLP 1133.002 D2, D5). Each
// set of options now has its own browser registration, which the browser holds to those options.

let registrations, saved, state, statuses;
beforeEach(() => {
  registrations = [];
  state = 'prompt';
  statuses = [];
  saved = globalThis.navigator;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { geolocation: {
    watchPosition(success, error, options) { registrations.push({ success, error, options, cleared: false }); return registrations.length; },
    clearWatch(id) { registrations[id - 1].cleared = true; },
  }, permissions: { async query() { const status = { state, onchange: null }; statuses.push(status); return status; } } } });
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

// The browser ends a watch it refuses, and its registration stayed, so its denial answered every later read even once
// the person allowed location (D5).

/** An implementation whose `changed` asks its reader again, as the runtime does. */
function reading(args) {
  const changes = [], answers = [];
  const impl = implementation({ changed(key) { changes.push(key); if (key === 'geolocation.watchPosition') read(); } });
  const read = () => impl.watchPosition(args).then((v) => answers.push(v.error ?? v.timestamp));
  return { impl, changes, answers, read };
}
const watched = (changes) => changes.filter((key) => key === 'geolocation.watchPosition').length;
/** The permission changes, and every `PermissionStatus` listened to says so. */
const moved = (to) => { state = to; for (const status of [...statuses]) status.onchange?.(); };
const refuse = (registration) => registration.error({ code: 1 });

test('a refused reader is asked again once the person allows location', async () => {
  const { changes, answers, read } = reading(coarse);
  read();
  state = 'denied';
  refuse(registrations[0]);
  await Bun.sleep(1);
  // The refusal answers the reader without announcing, so it is not asked into another.
  expect([answers, watched(changes), registrations.length]).toEqual([['PERMISSION_DENIED'], 0, 1]);
  // Nothing read the status: the refusal itself listens. A second signal of the same grant says nothing more.
  moved('granted');
  moved('granted');
  await Bun.sleep(1);
  expect([watched(changes), registrations.length]).toEqual([1, 2]);
  registrations[1].success(fix(3000, 7));
  await Bun.sleep(1);
  expect(answers).toEqual(['PERMISSION_DENIED', 7, 7]);
  expect(registrations.length).toBe(2);
});

test('a dismissed prompt is shown again by the next read, and not by a focus', async () => {
  const { changes, answers, read } = reading(coarse);
  read();
  refuse(registrations[0]);
  await Bun.sleep(1);
  moved('prompt');
  await Bun.sleep(1);
  expect([watched(changes), registrations.length]).toEqual([0, 1]);
  read();
  expect(registrations.length).toBe(2);
  registrations[1].success(fix(3000, 8));
  await Bun.sleep(1);
  expect(answers).toEqual(['PERMISSION_DENIED', 8, 8]);
});

test('a watch revoked while it runs tells its readers once, and a grant resumes them', async () => {
  const { impl, changes, answers, read } = reading(coarse);
  read();
  registrations[0].success(fix(3000, 1));
  await Bun.sleep(1);
  state = 'denied';
  refuse(registrations[0]);
  expect([watched(changes), registrations.length]).toEqual([2, 2]);
  refuse(registrations[1]);
  await Bun.sleep(1);
  expect(answers).toEqual([1, 1, 'PERMISSION_DENIED']);
  expect([watched(changes), registrations.length]).toEqual([2, 2]);
  moved('granted');
  await Bun.sleep(1);
  expect(registrations.length).toBe(3);
  impl.releaseArgs('watchPosition', coarse);
  expect(registrations.map((r) => r.cleared)).toEqual([true, true, true]);
});
