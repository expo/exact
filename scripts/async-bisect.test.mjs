import { expect, test } from 'bun:test';
import { bisectAffected, bisectFailures, probeOutcomes } from './async-bisect.mjs';
import { spawnSync } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

test('an hourly range finds separate breaking commits and shares probes', async () => {
  const calls = [], commits = Array.from({ length: 17 }, (_, i) => String(i));
  const result = await bisectFailures(commits, ['test: a', 'glue: b'], async sha => {
    calls.push(sha);
    return { 'test: a': Number(sha) >= 3 ? 'fail' : 'pass', 'glue: b': Number(sha) >= 13 ? 'fail' : 'pass' };
  });
  expect(result).toEqual({ 'test: a': { commit: '3', parent: '2' }, 'glue: b': { commit: '13', parent: '12' } });
  expect(new Set(calls).size).toBe(calls.length);
  // Repair detection must inspect the failing suffix; no monotonicity assumption is safe.
  expect(calls.length).toBeLessThanOrEqual(commits.length);
});

test('an old failure and an obstructed probe are never blamed on a commit', async () => {
  const result = await bisectFailures(['base', 'middle', 'tip'], ['old', 'blocked'], async sha => ({ old: 'fail', blocked: sha === 'base' ? 'pass' : sha === 'tip' ? 'fail' : 'unknown' }));
  expect(result.old).toEqual({ preexisting: 'base' });
  expect(result.blocked).toEqual({ unresolved: 'probe did not finish at middle', good: 'base', bad: 'tip' });
  expect((await bisectFailures(['base', 'tip'], ['x'], async () => ({ x: 'unknown' }))).x.commit).toBeUndefined();
});

test('only a finished probe can clear a failure; a failure it found still counts when another binary stopped', () => {
  const wanted = ['test: a', 'test: b', 'fmt: c', 'glue: d'];
  expect(probeOutcomes({ failures: ['test: a'], checks: { test: { complete: false }, fmt: { complete: true, status: 0 }, glue: { skipped: 'cached', complete: true } } }, wanted))
    .toEqual({ 'test: a': 'fail', 'test: b': 'unknown', 'fmt: c': 'pass', 'glue: d': 'unknown' });
  expect(probeOutcomes({ failures: [], checks: { test: { complete: true, status: 1 } } }, ['test: absent'])).toEqual({ 'test: absent': 'pass' });
});

import { checkout, withLaneEdits, run as runCheck, failureSelection, nothing, probeSteps, extraChecks, checks, fullCoverage, covers, failureAffected, retainSkippedFailures } from './async.mjs';

test('completed tests can pass at a probe while another test in the same check fails', async () => {
  const result = await bisectFailures(['0', '1', '2', '3', '4'], ['test: a', 'test: b'], async sha => {
    const failures = [...(Number(sha) >= 1 ? ['test: a'] : []), ...(Number(sha) >= 3 ? ['test: b'] : [])];
    return probeOutcomes({ failures, checks: { test: { complete: true, status: failures.length ? 101 : 0 } } }, ['test: a', 'test: b']);
  });
  expect(result).toEqual({ 'test: a': { commit: '1', parent: '0' }, 'test: b': { commit: '3', parent: '2' } });
});

test('attribution finds the failure at the tip after an earlier failure was repaired', async () => {
  const candidates = new Map([['x', ['1', '2', '3', '4']]]);
  const result = await bisectAffected('0', '4', ['x'], candidates,
    async sha => ({ x: ['1', '2', '4'].includes(sha) ? 'fail' : 'pass' }), sha => String(Number(sha) - 1));
  expect(result.x).toEqual({ commit: '4', parent: '3' });
});

test('default-member passes keep workspace test failures even for the same owner', () => {
  const a = 'test: -p one --lib a', b = 'test: -p other --lib b';
  const result = { checks: { 'default-test': { status: 0, complete: true, covers: ['cargo:one'] } }, failures: [], owners: {} };
  retainSkippedFailures([a, b], result, { [a]: ['cargo:one'], [b]: ['cargo:other'] });
  expect(result.failures).toEqual([a, b]);
});

test('Android and Windows app tests use their platform builders', () => {
  const hits = ['android', 'windows'].map(host => ({ suite: { kind: 'app', app: host === 'android' ? 'caltrain' : 'shelf', host, files: ['screen.test.contract'] } }));
  const repo = { packageAt: new Map([['apps/caltrain/android', { name: 'caltrain-android' }], ['apps/shelf/windows', { name: 'shelf-windows' }]]), appNames: new Map([['caltrain', 'apps/caltrain'], ['shelf', 'apps/shelf']]) };
  const steps = extraChecks(hits, [], repo);
  expect(steps.find(s => s[0] === 'build-app:caltrain:android').slice(1, 3)).toEqual(['bun', ['scripts/agent-android.mjs', 'build', 'caltrain']]);
  expect(steps.find(s => s[0] === 'build-app:shelf:windows').slice(1, 3)).toEqual(['bun', ['host/windows/build.mjs', 'shelf']]);
  expect(steps.filter(s => s[0].includes(':shelf:windows')).every(s => s[3]?.platform === 'win32')).toBe(true);
});

test('a Windows drive on another host is unavailable, never a native build or a passing probe', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-lane-platform-')), log = join(root, 'check.log');
  const fd = openSync(log, 'w');
  try {
    const r = await runCheck('/must/not/execute', [], process.env, fd, { platform: process.platform === 'win32' ? 'darwin' : 'win32' });
    expect(r.status).toBe(77);
    expect(r.hung).toBe(false);
    const result = { full: true, checks: { windows: { ...r, complete: false } } };
    expect(fullCoverage(result)).toBe(false);
    expect(fullCoverage({ full: true, checks: { test: { status: 0 }, semantics: { skipped: 'cached pass' } } })).toBe(true);
    expect(fullCoverage({ full: false, checks: { test: { status: 0 } } })).toBe(false);
    expect(r.notRun).toBe(`needs a ${process.platform === 'win32' ? 'darwin' : 'Windows'} worker`);
    expect(readFileSync(log, 'utf8')).toContain(`not run: ${r.notRun}`);
    const failure = 'app:shelf:windows:0: failed';
    expect(probeOutcomes({ failures: [], checks: { 'app:shelf:windows:0': { status: r.status, complete: false } } }, [failure])[failure]).toBe('unknown');
  } finally { closeSync(fd); rmSync(root, { recursive: true, force: true }); }
});

test('lane edits are restored after success, an error and a child killed before its cleanup', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-lane-edits-'));
  const paths = ['game/app/shells.lock', 'apps/caltrain/app.contract', 'apps/realworld/app.contract', 'apps/realworld/app.ts'];
  const git = (...args) => {
    const r = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
    if (r.status !== 0) throw new Error(r.stderr);
    return r.stdout.trim();
  };
  const write = (path, value) => { mkdirSync(dirname(join(root, path)), { recursive: true }); writeFileSync(join(root, path), value); };
  try {
    git('init', '-q');
    for (const path of [...paths, 'unrelated']) write(path, 'original');
    git('add', '.');
    git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture');
    const tip = git('rev-parse', 'HEAD');
    for (const outcome of ['success', 'error', 'killed']) {
      const run = withLaneEdits(root, async () => {
        for (const path of paths) write(path, 'lane');
        git('add', paths[0]);
        if (outcome === 'error') throw new Error('check failed');
        if (outcome === 'killed') {
          const child = spawnSync(process.execPath, ['-e', `
            const fs = require('node:fs');
            fs.writeFileSync('apps/caltrain/app.contract', 'child edit');
            process.on('exit', () => fs.writeFileSync('exit-cleanup', 'ran'));
            process.kill(process.pid, 'SIGKILL');
          `], { cwd: root });
          expect(child.error).toBeUndefined();
          expect(process.platform === 'win32' ? child.status : child.signal).toBe(process.platform === 'win32' ? 1 : 'SIGKILL');
          expect(readFileSync(join(root, paths[1]), 'utf8')).toBe('child edit');
          expect(existsSync(join(root, 'exit-cleanup'))).toBe(false);
        }
      });
      if (outcome === 'error') await expect(run).rejects.toThrow('check failed');
      else await run;
      expect(git('status', '--porcelain')).toBe('');
      checkout(tip, root);
      for (const path of paths) expect(readFileSync(join(root, path), 'utf8')).toBe('original');
    }
    write(paths[0], 'preexisting');
    let called = false;
    await expect(withLaneEdits(root, () => { called = true; })).rejects.toThrow('has tracked edits');
    expect(called).toBe(false);
    expect(readFileSync(join(root, paths[0]), 'utf8')).toBe('preexisting');
    git('restore', '--', paths[0]);
    const killed = spawnSync(process.execPath, ['-e', `
      const fs = require('node:fs');
      for (const path of ${JSON.stringify(paths)}) fs.writeFileSync(path, 'interrupted lane');
      require('node:child_process').execFileSync('git', ['add', ${JSON.stringify(paths[0])}]);
      process.on('exit', () => fs.writeFileSync('exit-cleanup', 'ran'));
      process.kill(process.pid, 'SIGKILL');
    `], { cwd: root });
    expect(killed.error).toBeUndefined();
    expect(process.platform === 'win32' ? killed.status : killed.signal).toBe(process.platform === 'win32' ? 1 : 'SIGKILL');
    for (const path of paths) expect(readFileSync(join(root, path), 'utf8')).toBe('interrupted lane');
    expect(existsSync(join(root, 'exit-cleanup'))).toBe(false);
    checkout(tip, root);
    expect(git('status', '--porcelain')).toBe('');
    for (const path of paths) expect(readFileSync(join(root, path), 'utf8')).toBe('original');
    await withLaneEdits(root, () => { write(paths[0], 'lane'); write('unrelated', 'keep'); });
    write(paths[0], 'interrupted lane');
    git('add', paths[0], 'unrelated');
    expect(() => checkout(tip, root)).toThrow('has tracked edits');
    expect(readFileSync(join(root, paths[0]), 'utf8')).toBe('original');
    expect(readFileSync(join(root, 'unrelated'), 'utf8')).toBe('keep');
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 30000);

test('a preexisting failure repaired in the range is attributed to its new introduction', async () => {
  const result = await bisectFailures(['base', 'pass', 'tip'], ['x'], async sha => ({ x: sha === 'pass' ? 'pass' : 'fail' }));
  expect(result.x).toEqual({ commit: 'tip', parent: 'pass' });
});

test('an obstructed later probe cannot confirm an earlier introduction', async () => {
  const result = await bisectFailures(['base', 'fail', 'unknown', 'tip'], ['x'], async sha => ({ x: sha === 'base' ? 'pass' : sha === 'unknown' ? 'unknown' : 'fail' }));
  expect(result.x.commit).toBeUndefined();
  expect(result.x.unresolved).toContain('unknown');
});

test('an unavailable or incomplete default-member run keeps earlier failures', () => {
  const failure = 'test: -p one --lib a';
  for (const defaults of [{ status: 77, complete: false }, { status: 0, complete: true, skipped: 'cached' }, { status: 101, complete: false }]) {
    const result = { checks: { 'default-test': { ...defaults, covers: ['cargo:one'] } }, failures: [] };
    retainSkippedFailures([failure], result, { [failure]: ['cargo:one'] });
    expect(result.failures).toEqual([failure]);
  }
});
test('bisection runs the failing affected suites and their prerequisite builds', () => {
  const selection = { ...nothing(), full: true, cargo: ['one', 'two'], glue: ['./a.test.mjs', './b.test.mjs'], ios: true };
  const wanted = ['test: -p one --lib fails', 'glue: bad', 'ios: ExactKitTests.ATests bad failed'];
  const owners = { [wanted[0]]: ['cargo:one'], [wanted[1]]: ['bun:a.test.mjs'], [wanted[2]]: ['xctest:ios:ATests'] };
  const narrowed = failureSelection(selection, wanted, owners);
  expect(narrowed.cargo).toEqual(['one']);
  expect(narrowed.glue).toEqual(['./a.test.mjs']);
  expect(narrowed.iosOnly).toEqual(['ATests']);
  expect([...probeSteps(wanted)].sort()).toEqual(['glue', 'ios', 'prepare', 'test']);
});

test('affected suites outside glue, including macOS XCTest and app tests, have hourly commands', () => {
  const steps = extraChecks([
    { suite: { kind: 'bun', file: 'scripts/in-glue.test.mjs' } },
    { suite: { kind: 'bun', file: 'packages/p/test/other.test.mjs' } },
    { suite: { kind: 'xctest', host: 'macos' }, only: ['A'] },
    { suite: { kind: 'app', app: 'fixture', host: 'web', files: ['apps/fixture/screen.test.contract'] } },
  ], ['./scripts/in-glue.test.mjs']);
  expect(steps.map(s => s[0])).toEqual(['other-glue', 'macos', 'build-app:fixture:web', 'app:fixture:web:0']);
  expect(steps[0][2]).toEqual(['EXACT_REQUIRED=1', 'bun', 'test', '--isolate', './packages/p/test/other.test.mjs']);
  expect(steps[1][2]).toEqual(['host/apple/build.mjs', '--test', '--only', 'A']);
});

test('a full hourly run never claims default-member tests covered other workspace packages', () => {
  expect(covers('default-test', { full: true, defaultCargo: ['one'] })).toEqual(['cargo:one']);
});

test('attribution excludes unrelated commits and preserves unknown coverage', () => {
  const known = new Set(['cargo:one', 'xctest:ios']), untouched = { ...nothing(), ids: [] };
  expect(failureAffected(untouched, ['cargo:one'], known, 'default-test')).toBe(false);
  expect(failureAffected(untouched, ['cargo:unknown'], known, 'default-test')).toBe(true);
  expect(failureAffected(untouched, [], known, 'ios')).toBe(false);
  expect(failureAffected({ ...untouched, ids: ['cargo:one'] }, ['cargo:one'], known, 'default-test')).toBe(true);
  const classes = { ...untouched, ids: ['xctest:ios'], ios: true, iosOnly: ['BTests'] };
  expect(failureAffected(classes, ['xctest:ios:ATests'], known, 'ios')).toBe(false);
  expect(failureAffected(classes, ['xctest:ios:BTests'], known, 'ios')).toBe(true);
});

test('a pruned range confirms the real parent, and refuses a missed input or an unrelated failure', async () => {
  const commits = ['0', '1', '2', '3', '4'], calls = [];
  const candidates = new Map([['a', ['2']], ['missed', ['3']], ['unrelated', []]]);
  const result = await bisectAffected('0', '4', [...candidates.keys()], candidates, async sha => {
    calls.push(sha);
    return { a: Number(sha) >= 2 ? 'fail' : 'pass', missed: Number(sha) >= 2 ? 'fail' : 'pass' };
  }, sha => commits[Number(sha) - 1]);
  expect(result.a).toEqual({ commit: '2', parent: '1' });
  expect(result.missed.commit).toBeUndefined();
  expect(result.missed.unresolved).toContain('actual parent');
  expect(result.unrelated.commit).toBeUndefined();
  expect(new Set(calls).size).toBe(calls.length);
});

test('game probes refresh the shell lock before lock-consuming steps', () => {
  for (const name of ['game', 'game-recipes', 'game-pins', 'game-scaffold', 'game-assets-scaffold']) {
    const steps = probeSteps([`${name}: failed`]);
    expect(steps.has('game-lock')).toBe(true);
    const ordered = checks('', null, { ...nothing(), game: true }).filter(([n]) => steps.has(n)).map(([n]) => n);
    expect(ordered[0]).toBe('game-lock');
    expect(ordered).toContain(name);
  }
});

test('remaining game Bun suites run hourly, including partially covered scaffold files', () => {
  const files = ['game/proof-inputs.test.mjs', 'game/proof-game.test.mjs', 'game/new.test.mjs', 'game/vocab.test.mjs'];
  const hits = files.map(file => ({ suite: { kind: 'bun', file } }));
  const steps = extraChecks(hits, []);
  const args = steps.filter(([name]) => name === 'other-glue').flatMap(step => step[2]);
  for (const file of files.slice(0, -1)) expect(args).toContain(`./${file}`);
  expect(args).not.toContain('./game/vocab.test.mjs');
});

test('probes of remaining game Bun suites refresh the lock without adding game work to unrelated probes', () => {
  const extra = extraChecks([{ suite: { kind: 'bun', file: 'game/proof-inputs.test.mjs' } }], []);
  expect(probeSteps(['other-glue: failed'], { extra }).has('game-lock')).toBe(true);
  expect(probeSteps(['other-glue: failed'], { extra: [] }).has('game-lock')).toBe(false);
  expect(probeSteps(['test: failed']).has('game-lock')).toBe(false);
});

test('only a workspace test check covering the owner clears its earlier failure', () => {
  const failure = 'test: -p one --lib a';
  for (const owner of ['one', 'other']) {
    const result = { checks: { test: { status: 0, complete: true, covers: [`cargo:${owner}`] } }, failures: [] };
    retainSkippedFailures([failure], result, { [failure]: ['cargo:one'] });
    expect(result.failures).toEqual(owner === 'one' ? [] : [failure]);
  }
});
