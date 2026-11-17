// How a test that cannot run says so (scripts/coverage.mjs), and the async
// lane's side of it: required coverage, this commit's fixtures, no test run.
import { expect, spyOn, test } from 'bun:test';
import { drivable, engineTests, need, notApplicable, required } from './coverage.mjs';
import { checks, failures, laneEnv } from './async.mjs';

/** A stand-in for bun's `test` that records what was registered. */
function recorder() {
  const seen = [];
  const t = (name, fn) => seen.push({ name, fn, how: 'run' });
  t.skip = (name, fn) => seen.push({ name, fn, how: 'skip' });
  t.each = table => (name, fn) => seen.push({ name, fn, how: 'run', table });
  t.skip.each = table => (name, fn) => seen.push({ name, fn, how: 'skip', table });
  t.serial = t;
  return { t, seen };
}

test('nothing missing is the test itself', () => {
  const { t } = recorder();
  expect(need('chrome', null, { test: t, env: {} })).toBe(t);
  expect(need('chrome', null, { test: t, env: { EXACT_REQUIRED: '1' } })).toBe(t);
  expect(notApplicable(null, { test: t })).toBe(t);
});

test('a missing prerequisite skips, saying why once and then naming each test, rows too', () => {
  const { t, seen } = recorder();
  const warn = spyOn(console, 'warn').mockImplementation(() => {});
  let said;
  try {
    const chromeTest = need('chrome', 'Chromium is missing', { test: t, env: {} });
    chromeTest('presses');
    chromeTest.each([['taps', 1], ['drags', 2]])('%s with %d finger');
    said = warn.mock.calls.map(c => c[0]);
  } finally { warn.mockRestore(); }
  expect(seen.map(s => [s.name, s.how])).toEqual([['presses — missing chrome', 'skip'], ['%s with %d finger — missing chrome', 'skip']]);
  expect(said).toEqual([
    'SKIP: missing chrome, so the tests that need it are skipped, not failed. This run lacks it; the code under test is not at fault. Chromium is missing',
    'SKIP: presses — missing chrome', 'SKIP: taps with 1 finger — missing chrome', 'SKIP: drags with 2 finger — missing chrome',
  ]);
});

test('a missing browser engine skips its own tests, and in a test of several engines only its part', async () => {
  const { t, seen } = recorder();
  const missing = async engine => engine === 'firefox' ? 'Firefox is missing' : null;
  const warn = spyOn(console, 'warn').mockImplementation(() => {});
  let said;
  try {
    const tests = await engineTests(['chromium', 'firefox', 'webkit'], { test: t, env: {}, missing });
    expect([tests.chromium, tests.chrome, tests.webkit]).toEqual([t, t, t]);
    tests.firefox('scrolls');
    for (const _ of [1, 2]) expect(await drivable(['chrome', 'firefox', 'webkit'], { env: {}, missing })).toEqual(['chrome', 'webkit']);
    said = warn.mock.calls.map(c => c[0]);
  } finally { warn.mockRestore(); }
  expect(seen).toEqual([{ name: 'scrolls — missing firefox', fn: undefined, how: 'skip' }]);
  expect(said).toEqual([expect.stringMatching(/^SKIP: missing firefox, so .* Firefox is missing$/), 'SKIP: scrolls — missing firefox',
    expect.stringMatching(/^SKIP: the firefox part of the test at scripts\/coverage\.test\.mjs:\d+ — missing firefox$/)]);
  await expect(drivable(['webkit', 'firefox'], { env: { EXACT_REQUIRED: '1' }, missing })).rejects.toThrow('missing firefox: Firefox is missing (EXACT_REQUIRED=1)');
});

test('under EXACT_REQUIRED a missing prerequisite fails rather than skips', () => {
  const { t, seen } = recorder();
  const check = need('fixture', 'no wasm dist', { test: t, env: { EXACT_REQUIRED: '1' } });
  check('adopts the document');
  check.serial('serially too');
  check.each([1, 2])('row %d');
  expect(seen.map(s => [s.name, s.how])).toEqual([
    ['adopts the document — missing fixture: no wasm dist', 'run'],
    ['serially too — missing fixture: no wasm dist', 'run'],
    ['row %d — missing fixture: no wasm dist', 'run'],
  ]);
  expect(seen[2].fn).toThrow('missing fixture: no wasm dist (EXACT_REQUIRED=1)');
  expect(() => seen[0].fn()).toThrow('missing fixture: no wasm dist (EXACT_REQUIRED=1)');
});

test('not applicable here is a skip even when coverage is required', () => {
  const { t, seen } = recorder();
  notApplicable('macOS only', { test: t })('placement');
  expect(seen).toEqual([{ name: 'placement — n/a: macOS only', fn: undefined, how: 'skip' }]);
  expect(required({ EXACT_REQUIRED: '1' })).toBe(true);
  expect(required({})).toBe(false);
});

test('the lane requires coverage and gives its web tests this commit\'s builds', () => {
  const steps = Object.fromEntries(checks('new', 'old', { lane: [], glue: ['./x.test.mjs'] }).map(s => [s[0], s]));
  const names = Object.keys(steps);
  expect(names.indexOf('prepare')).toBeLessThan(names.indexOf('test'));
  expect(names.indexOf('prepare')).toBeLessThan(names.indexOf('glue'));
  for (const name of ['test', 'glue', 'web-build-test', 'web-ax-test', 'module-seams', 'notifications-web']) expect(steps[name][2]).toContain('EXACT_REQUIRED=1');
  const glue = steps.glue[2].join(' ');
  for (const v of ['EXACT_WEB_DIST=', 'EXACT_WEB_JS_DIST=', 'EXACT_NO_GPU_WASM_DIST=', 'EXACT_NO_GPU_JS_DIST=', 'EXACT_MOTION_DIST=']) expect(glue).toContain(v);
  expect(steps.prepare[2][1]).toContain('EXACT_WEB_LINK=all');
});

test('the lane drops settings that change what a check tests', () => {
  const env = laneEnv({ PATH: '/p', EXACT_JS_ENGINE: 'stub', EXACT_AGENT: '1', EXACT_AGENT_FAIL_FETCH: '1', EXACT_WEB_DIST: '/old', EXACT_REQUIRED: '0', CARGO_TARGET_DIR: '/t' });
  expect(env).toEqual({ PATH: '/p', HERMES_LEAN_SYS_OFFLINE: '1' });
});

test('a bun step that ran no test is a failure', () => {
  expect(failures('web-build-test', 'bun test v1.4.2\nerror: regex "x" matched 0 tests. Searched 1 file\n', 0)).toEqual(['web-build-test: ran no tests']);
  expect(failures('glue', 'bun test v1.4.2\n 3 pass\n 0 fail\nRan 3 tests across 1 file.\n', 0)).toEqual([]);
});

test("the tests a crashed XCTest process took down are one failure, and a test that failed is named", () => {
  const log = [
    "Test case 'PushedTitleIOSTests.testAPush()' failed on 'Clone 1 of iPhone 18 Pro - xctest (83259)' (0.000 seconds)",
    "Test case 'NavigationCollapseIOSTests.testADrag()' failed on 'Clone 1 of iPhone 18 Pro - xctest (83440)' (0.000 seconds)",
    "Test case 'NavigationCollapseIOSTests.testAnInset()' failed on 'Clone 1 of iPhone 18 Pro - xctest (52457)' (5.465 seconds)",
    "Test case 'PushedTitleIOSTests.testASwap()' failed on 'Clone 1 of 2026-11-13 05:03:35.751 xcodebuild[47825:30582401] [MT] elapsed",
  ].join('\n');
  expect(failures('ios', log, 1).sort()).toEqual([
    'ios: ExactKitTests.NavigationCollapseIOSTests testAnInset failed',
    'ios: ExactKitTests.PushedTitleIOSTests testASwap failed',
    'ios: a test process crashed',
  ]);
});

test('a gate budget failure in the hourly metrics is attributed as a violation', () => {
  expect(failures('metrics', '  blocking gate (warm)                 31.0 s   VIOLATION: gate; budget 30s\n', 0))
    .toEqual(['metrics: blocking gate (warm) VIOLATION']);
});
