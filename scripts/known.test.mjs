// What fails on main, said beside a local run (LLP 1146 D1): the lane's note and its reading.
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { classify, kindOf, mainNotes, noteOf, publish, report, stoppedEarly, suitesOf } from './known.mjs';
import { checks, covers, glueTests, nothing } from './async.mjs';

const result = (sha, failures, checks = { ios: { command: 'bun host/apple/build.mjs --test --ios', status: 1 } }) => ({ sha, checks, failures });

test('a note says which checks ran to the end', () => {
  const note = noteOf(result('a', ['ios: ExactKitTests.A b failed', 'test: exact-x (lib) does not compile'], {
    ios: { command: 'bun host/apple/build.mjs --test --ios', status: 1 },
    test: { command: 'cargo test --workspace', status: 101 },
    difftest: { command: 'cargo run -p contract-difftest', skipped: "inputs of abc's pass" },
  }), 1);
  expect(note.checks.ios).toEqual({ command: 'bun host/apple/build.mjs --test --ios', status: 1, complete: true });
  expect(note.checks.test.complete).toBe(false);
  expect(note.checks.difftest.skipped).toBeDefined();
});

test('a failure is known, new or unknown against the nearest note that ran its kind of check', () => {
  const history = [
    { sha: 'c3', notes: [noteOf(result('c3', [], { lint: { command: 'cargo clippy', status: 0 } }), 1)] },
    { sha: 'c2', notes: [noteOf(result('c2', ['ios: ExactKitTests.Iframe pinch failed']), 1)] },
    { sha: 'c1', notes: [noteOf(result('c1', ['ios: ExactKitTests.Iframe pinch failed', 'ios: ExactKitTests.Nav pop failed']), 1)] },
  ];
  const sorted = classify(['local: ExactKitTests.Iframe pinch failed', 'local: ExactKitTests.Nav pop failed', 'local: ExactKitTests.Mine failed'], 'xctest', history);
  expect(sorted.at.sha).toBe('c2');
  expect(sorted.rows.map((r) => [r.verdict, r.rate])).toEqual([['known', '2 of its last 2 lane runs'], ['new', '1 of the last 2 lane runs'], ['new', undefined]]);
  expect(report(sorted)[0]).toMatch(/^3 failed: 1 known on origin\/main at c2 \(lane, .*\), 2 new$/);
  expect(classify(['local: x'], 'cargo', history).rows[0].verdict).toBe('unknown');
  expect(report(classify(['local: x'], 'cargo', history))[0]).toContain('unknown on origin/main');
  // A note keeps the part a check ran when it ran only a part
  const part = { ios: { command: 'bun host/apple/build.mjs --test --ios --only Iframe', status: 0, covers: ['xctest:ios:Iframe'] } };
  expect(noteOf(result('c4', [], part), 1).checks.ios.covers).toEqual(['xctest:ios:Iframe']);
});

test('a failure is sorted only against notes that ran its suite to the end, and its rate counts only those', () => {
  const ios = (only, status = 0, complete = true) => ({ ios: { command: `bun host/apple/build.mjs --test --ios${only ? ` --only ${only}` : ''}`, status, complete, ...(only ? { covers: [`xctest:ios:${only}`] } : {}) } });
  const history = [
    // A part that ran another class, then one that ran Nav but stopped early: neither says anything of Nav.
    { sha: 'c4', notes: [{ sha: 'c4', tier: 1, at: '2026-11-12T04:00', checks: ios('Iframe'), failures: ['ios: ExactKitTests.Nav pop failed'] }] },
    { sha: 'c3', notes: [{ sha: 'c3', tier: 1, at: '2026-11-12T03:00', checks: ios('Nav', 1, false), failures: [] }] },
    { sha: 'c2', notes: [{ sha: 'c2', tier: 1, at: '2026-11-12T02:00', checks: ios(null, 0), failures: [] }] },
    { sha: 'c1', notes: [{ sha: 'c1', tier: 1, at: '2026-11-12T01:00', checks: ios(null, 1), failures: ['ios: ExactKitTests.Nav pop failed'] }] },
  ];
  const local = ['local: ExactKitTests.Nav pop failed', 'local: ExactKitTests.Iframe pinch failed', 'local: ExactKitTests.Odd one failed'];
  const suites = suitesOf(local, 'xctest', '');
  expect(suites[local[0]]).toEqual(['xctest:ios:Nav']);
  const sorted = classify(local, 'xctest', history, { ...suites, [local[2]]: [] });
  // Nav is new at c2, the nearest note that ran it, and failed in one of the two that did; Iframe is new at c4; a
  // failure whose suite is not known is sorted against full runs only.
  expect(sorted.rows.map((r) => [r.verdict, r.sha, r.rate])).toEqual([['new', 'c2', '1 of the last 2 lane runs'], ['new', 'c4', undefined], ['new', 'c2', undefined]]);
  expect(sorted.at.sha).toBe('c4');
  expect(report(sorted)[1]).toBe('  new   ExactKitTests.Nav pop failed (failed 1 of the last 2 lane runs; at c2)');
  // Only partial runs of other suites: unknown. Another Cargo package's partial run says nothing of this one's test.
  expect(classify(local.slice(0, 1), 'xctest', history.slice(0, 1), suites).rows[0].verdict).toBe('unknown');
  const cargo = [{ sha: 'r1', notes: [{ sha: 'r1', tier: 1, checks: { test: { command: 'bun scripts/affected.mjs --cargo-tests a', status: 0, complete: true, covers: ['cargo:a'] } }, failures: [] }] }];
  const mine = ['local: -p b --lib x::y'], own = suitesOf(mine, 'cargo', '');
  expect(own[mine[0]]).toEqual(['cargo:b']);
  expect(classify(mine, 'cargo', cargo, own).rows[0].verdict).toBe('unknown');
  expect(classify(['local: -p a --lib x::y'], 'cargo', cargo, suitesOf(['local: -p a --lib x::y'], 'cargo', '')).rows[0].verdict).toBe('new');
});

test('the lane publishes a note to origin and another checkout reads it', () => {
  const dir = mkdtempSync(join(tmpdir(), 'known-'));
  const run = (cwd, ...args) => { const r = spawnSync('git', args, { cwd, encoding: 'utf8' }); if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr}`); return r.stdout.trim(); };
  try {
    const origin = join(dir, 'origin.git'), lane = join(dir, 'lane'), mine = join(dir, 'mine');
    run(dir, 'init', '-q', '--bare', '-b', 'main', origin);
    run(dir, 'clone', '-q', origin, lane);
    for (const k of ['user.email', 'user.name']) run(lane, 'config', k, 'lane');
    writeFileSync(join(lane, 'a'), '1'); run(lane, 'add', 'a'); run(lane, 'commit', '-qm', 'one');
    run(lane, 'push', '-q', 'origin', 'HEAD:main');
    const sha = run(lane, 'rev-parse', 'HEAD');
    expect(publish(noteOf(result(sha, ['ios: ExactKitTests.Iframe pinch failed']), 1), lane)).toBeNull();
    run(dir, 'clone', '-q', origin, mine);
    const history = mainNotes({ cwd: mine });
    expect(history.map((h) => h.sha)).toEqual([sha]);
    expect(classify(['local: ExactKitTests.Iframe pinch failed'], 'xctest', history).rows[0].verdict).toBe('known');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('each stop the lane names marks its check stopped early, and a note reads what the lane recorded', () => {
  for (const f of ['test: x (lib test) does not compile', 'test: x build script failed', 'test: -p x --lib did not finish', 'glue: a.test.mjs: an error between tests', 'glue: exit 1 (see log)', 'test: t hung'])
    expect(stoppedEarly(f.split(': ', 1)[0], [f]), f).toBe(true);
  expect(stoppedEarly('test', ['test: -p x --lib a::b'])).toBe(false);
  // Put a stop kept from an earlier run in the list the note reads, while the check's own record says it ran to the end
  const result = { sha: 'x', checks: { test: { command: 'bun scripts/affected.mjs --cargo-tests a', status: 0, complete: true } }, failures: ['test: y (lib test) does not compile'] };
  expect(noteOf(result, 1).checks.test.complete).toBe(true);
});

test('kindOf classifies glue as bun, splits macos and ios xctest, and ignores narrow non-test commands', () => {
  const fullSteps = Object.fromEntries(checks('tip', 'base', { glue: glueTests(process.cwd()), lane: ['x'] }).map(([name, cmd, args]) => [name, [cmd, ...args].join(' ')]));
  expect(kindOf(fullSteps.glue)).toBe('bun');
  expect(kindOf(fullSteps.test)).toBe('cargo');
  expect(kindOf(fullSteps.ios)).toBe('xctest-ios');
  expect(kindOf('bun host/apple/build.mjs --test')).toBe('xctest-macos');
  expect(kindOf('bun host/apple/build.mjs --test --ios --only Nav')).toBe('xctest-ios');
  expect(kindOf('bun test scripts/cargo-checkout.test.mjs')).toBe('bun');
  expect(kindOf(fullSteps['game-pins'])).toBeNull();
  expect(kindOf(fullSteps['notifications-web'])).toBeNull();
  expect(kindOf('bun scripts/agent.mjs web --app shelf --test apps/shelf/app.test.contract')).toBeNull();
});

test('narrow checks, lane, full glue, and macos vs ios xctest do not misclassify unrelated failures', () => {
  const glueCmd = checks('tip', 'base', { ...nothing(), glue: glueTests(process.cwd()) }).find(([n]) => n === 'glue');
  const fullGlueCmd = [glueCmd[1], ...glueCmd[2]].join(' ');
  // Full glue + incomplete test + passing lane: cargo failure is unknown, bun failure is new.
  const mixed = [{
    sha: 'm1',
    notes: [{
      sha: 'm1', tier: 1, at: '2026-11-25T01:00',
      checks: {
        test: { command: 'cargo test --workspace', status: 101, complete: false },
        lane: { command: 'cargo test --workspace -- --ignored slow', status: 0, complete: true, covers: ['cargo:exact-linux'] },
        glue: { command: fullGlueCmd, status: 0, complete: true },
      },
      failures: [],
    }],
  }];
  const cargoFailure = ['local: -p exact-linux --lib foo::bar'];
  const bunFailure = ['local: fails'];
  expect(classify(cargoFailure, 'cargo', mixed, suitesOf(cargoFailure, 'cargo', '')).rows[0].verdict).toBe('unknown');
  expect(classify(bunFailure, 'bun', mixed, { 'local: fails': ['bun:scripts/foo.test.mjs'] }).rows[0].verdict).toBe('new');

  // Partial game/vocab + narrow checks without whole coverage never satisfy unrelated cargo or bun failures.
  const narrow = [{
    sha: 'n1',
    notes: [{
      sha: 'n1', tier: 1, at: '2026-11-25T02:00',
      checks: {
        game: { command: 'cargo test --manifest-path game/Cargo.toml --workspace', status: 0, complete: true },
        'game-vocab': { command: 'bun test game/vocab.test.mjs', status: 0, complete: true, covers: covers('game-vocab', { full: true }) },
        'web-ax-test': { command: 'env bun test ./host/web/tests/zero-viewport.test.mjs', status: 0, complete: true, covers: covers('web-ax-test', { full: true }) },
        'other-glue': { command: 'env bun test --isolate ./packages/p/a.test.mjs', status: 0, complete: true, covers: covers('other-glue', { full: true, extra: [['other-glue', 'env', ['bun', 'test', './packages/p/a.test.mjs']]] }) },
        'app:shelf:web:0': { command: 'bun scripts/agent.mjs web --app shelf --test apps/shelf/app.test.contract', status: 0, complete: true },
      },
      failures: [],
    }],
  }];
  expect(classify(cargoFailure, 'cargo', narrow, suitesOf(cargoFailure, 'cargo', '')).rows[0].verdict).toBe('unknown');
  expect(classify(bunFailure, 'bun', narrow, { 'local: fails': ['bun:apps/agents/app.test.ts'] }).rows[0].verdict).toBe('unknown');
  expect(classify(bunFailure, 'bun', narrow, { 'local: fails': ['bun:game/vocab.test.mjs'] }).rows[0].verdict).toBe('new');
  expect(classify(bunFailure, 'bun', narrow, { 'local: fails': ['bun:packages/p/a.test.mjs'] }).rows[0].verdict).toBe('new');

  // macOS and iOS XCTests never cross-match each other, and partial macOS suites match xctest:macos:<Class>.
  const macFailure = ['local: ExactKitTests.Nav pop failed'];
  const macSuites = suitesOf(macFailure, 'xctest-macos', '');
  expect(macSuites[macFailure[0]]).toEqual(['xctest:macos:Nav']);
  const macOnly = [{
    sha: 'x1',
    notes: [{
      sha: 'x1', tier: 1, at: '2026-11-25T03:00',
      checks: { macos: { command: 'bun host/apple/build.mjs --test', status: 0, complete: true } },
      failures: ['ios: ExactKitTests.Nav pop failed'],
    }],
  }];
  expect(classify(macFailure, 'xctest-ios', macOnly, suitesOf(macFailure, 'xctest-ios', '')).rows[0].verdict).toBe('unknown');
  expect(classify(macFailure, 'xctest-macos', macOnly, macSuites).rows[0].verdict).toBe('new');
  const macPart = [{
    sha: 'x2',
    notes: [{
      sha: 'x2', tier: 1, at: '2026-11-25T04:00',
      checks: { macos: { command: 'bun host/apple/build.mjs --test --only Nav', status: 1, complete: true, covers: ['xctest:macos:Nav'] } },
      failures: ['macos: ExactKitTests.Nav pop failed'],
    }],
  }];
  expect(classify(macFailure, 'xctest-macos', macPart, macSuites).rows[0].verdict).toBe('known');
  expect(classify(macFailure, 'xctest-ios', macPart, suitesOf(macFailure, 'xctest-ios', '')).rows[0].verdict).toBe('unknown');
});
