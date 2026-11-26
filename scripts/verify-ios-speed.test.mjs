// Unit tests for expo/exact#1850 verify ios speedups and scoped-run honesty.
import { expect, test } from 'bun:test';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { compiledHingeHelper } from './duo/hinge.mjs';
import { beginContentSizeLeg, LARGEST, largestText, report } from './verify.mjs';
import { holdAccessibilityRuntime, withAccessibilityRuntime } from '../host/apple/touches.mjs';

test('compiledHingeHelper compiles and signs once per cache key and separates different toolchains', () => {
  const cacheDir = mkdtempSync(resolve(tmpdir(), 'exact-hinge-cache-'));
  try {
    const calls = [];
    let sdkBuild = '24A100';
    const run = (cmd, args) => {
      calls.push([cmd, ...args]);
      if (cmd === 'xcode-select') return '/Applications/Xcode.app/Contents/Developer';
      if (args.includes('--show-sdk-path')) return '/SDKs/iPhoneSimulator.sdk';
      if (args.includes('--show-sdk-build-version')) return sdkBuild;
      if (args.includes('--version')) return 'Apple clang version 17.0.0\nTarget: arm64';
      const outIdx = args.indexOf('-o');
      if (outIdx >= 0) writeFileSync(args[outIdx + 1], 'fake-binary');
      return '';
    };
    const first = compiledHingeHelper({ cacheDir, run });
    expect(existsSync(first)).toBe(true);
    expect(calls.some((c) => c[0] === 'xcrun' && c.includes('clang') && c.includes('-o'))).toBe(true);
    expect(calls.some((c) => c[0] === 'codesign')).toBe(true);
    const compileCount = calls.filter((c) => c[0] === 'codesign').length;
    const second = compiledHingeHelper({ cacheDir, run });
    expect(second).toBe(first);
    expect(calls.filter((c) => c[0] === 'codesign').length).toBe(compileCount);
    // Changing the SDK build version produces a separate cache entry and recompiles.
    sdkBuild = '24B200';
    const third = compiledHingeHelper({ cacheDir, run });
    expect(third).not.toBe(first);
    expect(calls.filter((c) => c[0] === 'codesign').length).toBe(compileCount + 1);
  } finally { rmSync(cacheDir, { recursive: true, force: true }); }
});

test('beginContentSizeLeg heals a simulator left at LARGEST and largestText fails closed on poll timeout', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-cs-'));
  try {
    const recordPath = resolve(dir, 'cs.json');
    writeFileSync(recordPath, JSON.stringify({ baseline: 'large' }));
    let current = LARGEST;
    const ops = [];
    const io = (_udid, size) => {
      ops.push(size ?? 'read');
      if (size) current = size;
      return { status: 0, stdout: current, stderr: '' };
    };
    const leg = beginContentSizeLeg('SIM-1', { io, recordPath });
    expect(leg.baseline).toBe('large');
    expect(current).toBe('large');
    expect(ops).toEqual(['read', 'large']);
    // Failing to apply largest text size within pollMs fails closed, restores baseline, and prevents a clean verdict.
    const fakeSession = {
      host: 'ios',
      carrier: { udid: 'SIM-1' },
      state: async () => ({}),
      prefer: async () => ({ page: { 'root-font-size': 17 } }),
    };
    const res = await largestText(fakeSession, 'first screen', { baseline: leg.baseline, recordPath, io, pollMs: 120 });
    expect(res.error).toContain('did not apply largest text size');
    expect(res.incomplete).toBe(true);
    expect(current).toBe('large');
    const rep = report({ host: 'ios', built: null, tested: { passed: 1, failed: 0, results: [] }, checks: [], large: [res], shots: [], ms: 2000, cwd: '/' });
    expect(rep.lines.some((l) => l.includes('0 screens: not read'))).toBe(true);
    expect(rep.lines.some((l) => l.includes('nothing spills'))).toBe(false);
    expect(rep.problems).toBeGreaterThan(0);
    expect(rep.exit).toBe(1);
    leg.finish();
    expect(existsSync(recordPath)).toBe(false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('holdAccessibilityRuntime holds AX across overlapping holders, restores only on last release, and surfaces restore failures', async () => {
  const udid = `TEST-HOLD-AX-${process.pid}-${Date.now()}`;
  const calls = [];
  let axOn = false, failDelete = false;
  const run = async (_u, args) => {
    calls.push(args);
    if (args[0] === 'read') return { status: 0, out: axOn ? '{\n    ApplicationAccessibilityEnabled = 1;\n}\n' : '{}\n', err: '' };
    if (args[0] === 'write') { axOn = args.includes('true'); return { status: 0, out: '', err: '' }; }
    if (args[0] === 'delete') {
      if (failDelete) return { status: 1, out: '', err: 'simctl refused' };
      axOn = false;
      return { status: 0, out: '', err: '' };
    }
    return { status: 0, out: '', err: '' };
  };
  const h1 = await holdAccessibilityRuntime(udid, { enabled: 'try', run });
  const h2 = await holdAccessibilityRuntime(udid, { enabled: 'try', run });
  expect(h1.active).toBe(true);
  expect(h2.active).toBe(true);
  expect(axOn).toBe(true);
  const setupSpawns = calls.length;
  // Subsequent launches with ax='try' skip all per-launch defaults spawns while keeping the lock.
  let launched = 0;
  for (let i = 0; i < 4; i++) {
    await withAccessibilityRuntime(udid, 'try', async () => { launched++; return {}; }, { run, check: false, touches: false });
  }
  expect(launched).toBe(4);
  expect(calls.length).toBe(setupSpawns);
  // Releasing one overlapping holder leaves AX enabled for the other.
  await h1.release();
  expect(axOn).toBe(true);
  // Releasing the last holder restores AX; a failed restore rejects rather than resolving silently.
  failDelete = true;
  await expect(h2.release()).rejects.toThrow('simulator accessibility could not be restored');
  expect(axOn).toBe(true);
  // A subsequent holder recovers the preserved baseline and restores cleanly when defaults succeeds.
  failDelete = false;
  const h3 = await holdAccessibilityRuntime(udid, { enabled: 'try', run });
  await h3.release();
  expect(axOn).toBe(false);
  expect(calls.at(-1)).toEqual(['delete', 'com.apple.Accessibility', 'ApplicationAccessibilityEnabled']);
});

test('scoped verify verdicts never emit bare "verify: clean" and AX restore errors prevent clean verdicts', () => {
  const base = { host: 'ios', built: null, tested: { passed: 2, failed: 0, results: [] }, checks: [], shots: [], ms: 2000, cwd: '/' };
  expect(report({ ...base, scopedDuo: true }).lines.at(-1)).toBe('verify (scoped: no iPhone Duo): clean in 2.0 s');
  expect(report({ ...base, partial: { selected: 1, total: 3 } }).lines.at(-1)).toBe('verify (partial: 1/3 test files): clean in 2.0 s');
  expect(report({ ...base, partial: { selected: 1, total: 3 }, scopedDuo: true }).lines.at(-1)).toBe('verify (partial: 1/3 test files, no iPhone Duo): clean in 2.0 s');
  expect(report(base).lines.at(-1)).toBe('verify: clean in 2.0 s');
  const axFail = report({ ...base, axError: 'simulator accessibility could not be restored: busy' });
  expect(axFail.lines.some((l) => l.includes('accessibility: could not restore: simulator accessibility could not be restored: busy'))).toBe(true);
  expect(axFail.problems).toBe(1);
  expect(axFail.exit).toBe(1);
});
