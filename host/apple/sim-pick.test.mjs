import { test, expect } from 'bun:test';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { compareRuntimeVersionDesc, matchSimulatorQuery, parseRuntimeVersion, pickSimulatorInteractive, resolveHumanSimulator, sortSimulatorCandidates } from './sim-pick.mjs';

const SIMS = [
  { udid: 'IOS-17-9', name: 'iPhone 16 Pro', runtime: 'com.apple.CoreSimulator.SimRuntime.iOS-17-9', state: 'Shutdown', type: 'iPhone-16-Pro' },
  { udid: 'IOS-17-10', name: 'iPhone 16 Pro', runtime: 'com.apple.CoreSimulator.SimRuntime.iOS-17-10', state: 'Shutdown', type: 'iPhone-16-Pro' },
  { udid: 'IOS-27-PRO', name: 'iPhone 17 Pro', runtime: 'com.apple.CoreSimulator.SimRuntime.iOS-27-0', state: 'Booted', type: 'iPhone-17-Pro' },
  { udid: 'IOS-27-AIR', name: 'iPhone 17 Air', runtime: 'com.apple.CoreSimulator.SimRuntime.iOS-27-0', state: 'Shutdown', type: 'iPhone-17-Air' },
  { udid: 'TV-27', name: 'Apple TV 4K', runtime: 'com.apple.CoreSimulator.SimRuntime.tvOS-27-0', state: 'Booted', type: 'Apple-TV-4K-3rd-generation' },
];

test('runtime versions sort as integer tuples (17.10 ahead of 17.9)', () => {
  expect(parseRuntimeVersion('com.apple.CoreSimulator.SimRuntime.iOS-17-10')).toEqual([17, 10, 0]);
  expect(compareRuntimeVersionDesc('com.apple.CoreSimulator.SimRuntime.iOS-17-10', 'com.apple.CoreSimulator.SimRuntime.iOS-17-9')).toBeLessThan(0);
  const ordered = sortSimulatorCandidates([SIMS[0], SIMS[1], SIMS[2]], 'IOS-17-9');
  expect(ordered.map((d) => d.udid)).toEqual(['IOS-27-PRO', 'IOS-17-9', 'IOS-17-10']);
});

test('matchSimulatorQuery supports exact match, platform-scoped substring, and ambiguous refusal', () => {
  // Exact UDID wins.
  expect(matchSimulatorQuery(SIMS, 'IOS-17-9')).toEqual({ dev: SIMS[0], matchedBySubstring: false });
  // Case-insensitive unique substring.
  expect(matchSimulatorQuery(SIMS, '17 air')).toEqual({ dev: SIMS[3], matchedBySubstring: true });
  // Duplicate name across OS versions picks newest tuple when both shut down.
  expect(matchSimulatorQuery(SIMS, '16 pro')).toEqual({ dev: SIMS[1], matchedBySubstring: true });
  // Ambiguous distinct names refuse listing candidates.
  expect(() => matchSimulatorQuery(SIMS, '17')).toThrow(/multiple iOS simulators match "17".*iPhone 17 Pro.*iPhone 17 Air/);
  // Platform-scoped: iOS query never falls back to tvOS.
  expect(() => matchSimulatorQuery(SIMS, 'Apple TV', { tv: false })).toThrow(/no iOS simulator matching "Apple TV"/);
  expect(matchSimulatorQuery(SIMS, 'Apple TV', { tv: true }).dev.udid).toBe('TV-27');
});

test('pickSimulatorInteractive selects by number, persists choice, and supports cancel', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-sim-pick-'));
  try {
    const mkStdin = () => Object.assign(new EventEmitter(), { isTTY: true, isRaw: false, setRawMode() {}, setEncoding() {}, resume() {}, pause() {} });
    const out = [];
    const stdin1 = mkStdin();
    const p1 = pickSimulatorInteractive([SIMS[0], SIMS[2]], { lastUsed: 'IOS-17-9', stdin: stdin1, stdout: { write: (s) => out.push(s) } });
    stdin1.emit('data', '2\n');
    expect((await p1).udid).toBe('IOS-17-9');

    const stdin2 = mkStdin();
    const p2 = pickSimulatorInteractive([SIMS[0], SIMS[2]], { stdin: stdin2, stdout: { write() {} } });
    stdin2.emit('data', 'q\n');
    expect(await p2).toBeNull();

    // forcePick refuses on non-TTY and persists on interactive TTY.
    await expect(resolveHumanSimulator(undefined, { dir, forcePick: true, interactive: false })).rejects.toThrow(/--pick requires an interactive terminal/);
    const chosen = await resolveHumanSimulator(undefined, {
      dir, forcePick: true, interactive: true,
      listSimulators: () => SIMS,
      pickInteractive: async (candidates) => {
        expect(candidates.every((d) => d.runtime.includes('SimRuntime.iOS'))).toBe(true);
        return candidates.find((d) => d.udid === 'IOS-27-AIR');
      },
    });
    expect(chosen.udid).toBe('IOS-27-AIR');
    expect(readFileSync(resolve(dir, '.exact/simulator'), 'utf8').trim()).toBe('IOS-27-AIR');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('pickSimulatorInteractive leaves a stdin it found flowing still flowing, so the dev loop keeps its keys', async () => {
  const stdin = Object.assign(new PassThrough(), { isTTY: true, isRaw: false, setRawMode() {} });
  stdin.on('data', () => {});
  const picked = pickSimulatorInteractive([SIMS[2]], { stdin, stdout: { write() {} } });
  stdin.write('1\n');
  expect((await picked).udid).toBe('IOS-27-PRO');
  expect(stdin.readableFlowing).toBe(true);
});
