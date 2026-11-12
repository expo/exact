import { test, expect } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { evictArms, keepRoster, keptRoster, rosterRoutes, snapshotSources } from './arms.mjs';

test('a qualified native roster is bound to the exact probe bytes, including when imported from a simulator run', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-roster-'));
  const probe = resolve(dir, 'probe'), receipt = resolve(dir, 'qualification.json');
  const roster = JSON.stringify({ 'native-view': { firstFrame: true }, extensions: {} });
  const bytes = Buffer.from('first compiled probe'), hash = createHash('sha256').update(bytes).digest('hex');
  const env = { EXACT_NATIVE_ROSTERS: receipt };
  try {
    writeFileSync(probe, bytes);
    expect(keptRoster(probe, {})).toBeUndefined();
    keepRoster(probe, roster); expect(keptRoster(probe, {})).toBe(roster);
    writeFileSync(probe, 'changed compiled probe');
    expect(keptRoster(probe, {})).toBeUndefined();
    writeFileSync(receipt, JSON.stringify({ version: 1, probes: [{ probeSha256: hash, roster }] }));
    expect(keptRoster(probe, env)).toBeUndefined();
    writeFileSync(probe, bytes); writeFileSync(`${probe}.roster`, '{}');
    expect(keptRoster(probe, env)).toBe(roster);
    expect(JSON.parse(readFileSync(`${probe}.roster`, 'utf8')).probeSha256).toBe(hash);
    expect(keptRoster(probe, {})).toBe(roster);
    writeFileSync(`${probe}.roster`, '{}');
    writeFileSync(receipt, JSON.stringify({ version: 1, probes: [{ probeSha256: hash, roster }, { probeSha256: hash, roster }] }));
    expect(() => keptRoster(probe, env)).toThrow('duplicate');
    expect(() => keepRoster(probe, '[]')).toThrow('JSON object');
    expect(() => keepRoster(probe, '{')).toThrow();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("a device build's roster routes each take the packages compiled for that route, and a route lacks only a missing xcframework slice", () => {
  const routes = (build) => rosterRoutes({ ...build, packageDir: '/b/packages', missingSlice: () => undefined }).map(r => [r.via, r.lacking, r.packages]);
  assert.deepEqual(routes({ tv: false, device: true }), [
    ['Mac Catalyst', undefined, '/b/packages-maccatalyst'],
    ['iOS Simulator', undefined, '/b/packages-simulator'],
    ['macOS', undefined, '/b/packages-macos'],
  ]);
  // A simulator build's simulator route links the build's own package objects.
  assert.deepEqual(routes({ tv: false, device: false }).map(r => r[2]), ['/b/packages-maccatalyst', '/b/packages', '/b/packages-macos']);
  assert.deepEqual(routes({ tv: true, device: true }), [['macOS', undefined, '/b/packages-macos']]);
  // Only an xcframework without the route's slice makes it lack.
  const lacking = rosterRoutes({ tv: false, device: true, packageDir: '/b/packages', missingSlice: (v) => v === 'maccatalyst' ? '/app/modules/apple/Map.xcframework' : undefined });
  assert.deepEqual(lacking.map(r => r.lacking ?? null), ['/app/modules/apple/Map.xcframework', null, null]);
});
