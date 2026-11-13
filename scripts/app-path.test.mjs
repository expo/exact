// A path names an app by its directory, keeping generated artifacts in its target.
import { afterEach, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { authoredTests, resolveApp, webDist } from './app.mjs';
import { runTests } from './agent-test.mjs';
import { traceLocators } from './agent-launch.mjs';

const ROOT = resolve(import.meta.dir, '..');
const before = { EXACT_APP_DIR: process.env.EXACT_APP_DIR, EXACT_WEB_DIST: process.env.EXACT_WEB_DIST };
afterEach(() => {
  for (const [name, value] of Object.entries(before)) {
    if (value === undefined) delete process.env[name]; else process.env[name] = value;
  }
});

test('an absolute or relative path to an app resolves to that app, named by its directory', () => {
  const dir = realpathSync(resolve(ROOT, 'apps/theme-fixture'));
  for (const given of [dir, relative(process.cwd(), dir) || '.', `${dir}/`]) {
    delete process.env.EXACT_APP_DIR;
    const app = resolveApp(given);
    expect([given, app.name, app.dir]).toEqual([given, 'theme-fixture', dir]);
    expect(app.crate('web')).toBe('theme-fixture-web');
    // What the dev loop and the JS build make from the name stays in the target.
    for (const made of [resolve(app.target, 'dev-gpu', app.name), resolve(app.target, 'web-js-gpu', app.name)]) {
      expect([made.startsWith(`${app.target}/`), made.startsWith(`${dir}/`)]).toEqual([true, false]);
    }
    expect(process.env.EXACT_APP_DIR).toBe(undefined);
  }
});

test('resolving a path preserves the environment and later app selections after the fixture is deleted', () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  delete process.env.EXACT_WEB_DIST;
  for (const inherited of [undefined, resolve(ROOT, 'apps/theme-fixture')]) {
    const dir = mkdtempSync(resolve(ROOT, 'target/app-path-test-'));
    if (inherited === undefined) delete process.env.EXACT_APP_DIR;
    else process.env.EXACT_APP_DIR = inherited;
    try {
      writeFileSync(resolve(dir, 'app.contract'), 'component Fixture\n  view\n    text "fixture"\n');
      const app = resolveApp(dir);
      expect(app.dir).toBe(realpathSync(dir));
      expect(webDist(app)).toBe(resolve(app.target, 'web-dist'));
      expect(traceLocators(dir)).toContain(resolve(app.target, 'web-dist/app.plan'));
      expect(process.env.EXACT_APP_DIR).toBe(inherited);
    } finally { rmSync(dir, { recursive: true, force: true }); }
    expect(resolveApp('caltrain-web').dir).toBe(realpathSync(inherited ?? resolve(ROOT, 'apps/caltrain')));
    expect(resolveApp().name).toBe(inherited ? 'theme-fixture' : 'caltrain');
    const app = resolveApp('caltrain');
    expect(webDist(app)).toBe(inherited ? resolve(app.target, 'web-dist') : resolve(ROOT, 'host/web/dist'));
    process.env.EXACT_WEB_DIST = resolve(ROOT, 'target/custom-web-dist');
    expect(webDist(app)).toBe(process.env.EXACT_WEB_DIST);
    delete process.env.EXACT_WEB_DIST;
  }
});

test('a web build and authored test drive use the selected path without changing the environment', async () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(ROOT, 'target/app-path-drive-'));
  delete process.env.EXACT_APP_DIR;
  delete process.env.EXACT_WEB_DIST;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Path', app: { id: 'test.app-path', name: 'Path' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component Fixture\n  view\n    text "fixture" testId="seen"\n');
    const file = resolve(dir, 'app.test.contract');
    writeFileSync(file, 'test "the selected fixture"\n  expect text "seen" == "fixture"\n');
    const built = spawnSync(process.execPath, ['host/web/build.mjs', dir], { cwd: ROOT, env: process.env, encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    expect(existsSync(resolve(webDist(resolveApp(dir)), '.exact-build.json'))).toBe(true);
    const result = await runTests({ host: 'web', app: dir, file });
    expect(result.results[0].failures).toEqual([]);
    expect(process.env.EXACT_APP_DIR).toBe(undefined);
  } finally { rmSync(dir, { recursive: true, force: true }); }
  expect(resolveApp('caltrain').dir).toBe(realpathSync(resolve(ROOT, 'apps/caltrain')));
}, 180_000);

test('metrics passes its path-selected app to web rebuilds and Apple builds', () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(ROOT, 'target/app-path-metrics-'));
  writeFileSync(resolve(dir, 'app.contract'), 'component Fixture\n  view\n    text "fixture"\n');
  // Run the build orchestration with measurements stubbed; each build's child environment resolves a real app.
  const script = `
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { EventEmitter } from 'node:events';
    import * as processes from 'node:child_process';
    import * as fs from 'node:fs';
    import * as serving from './host/web/serve.mjs';
    import * as launch from './scripts/agent-launch.mjs';
    const dir = ${JSON.stringify(dir)}, root = ${JSON.stringify(ROOT)}, builds = [], measurements = [];
    const spawnSync = processes.spawnSync, read = fs.readFileSync, mkdir = fs.mkdirSync, done = new Error('builds checked');
    delete process.env.EXACT_APP_DIR;
    process.env.EXACT_DIAGNOSTIC_ROOT = root;
    process.env.EXACT_DIAGNOSTIC_SOURCE = JSON.stringify({ started: Date.now(), sources: [{ roles: ['exact2'], commit: 'test' }] });
    process.argv = [process.execPath, root + '/scripts/metrics.mjs', '--app', dir, '--rebuild', '--long'];
    process.kill = () => { throw new Error('no measurement processes running'); };
    mock.module('node:fs', () => ({ ...fs,
      utimesSync() {},
      mkdirSync(path, options) { if (!String(path).includes('/exact-chrome-profile')) return mkdir(path, options); },
      readFileSync(path, ...args) { return String(path) === root + '/host/web/dist/app.js' ? Buffer.from('export {};') : read(path, ...args); },
    }));
    mock.module('./host/web/serve.mjs', () => ({ ...serving, jsTargetBuild: () => true, buildFileCards: () => [] }));
    mock.module('./scripts/agent-launch.mjs', () => ({ ...launch, chromium: () => ({ unavailable: 'measurement stub' }) }));
    mock.module('node:child_process', () => ({ ...processes,
      spawnSync(command, args, options) {
        if (command === 'rustc') return spawnSync(command, args, options);
        if (command === 'cargo' && args.includes('--bin')) {
          const bin = args[args.indexOf('--bin') + 1];
          assert.ok(['metrics', 'plan-metrics'].includes(bin));
          assert.ok(args.includes('--release'));
          measurements.push(bin);
        }
        if (args[0]?.endsWith('/host/web/build.mjs') || args[0]?.endsWith('/host/apple/build.mjs')) {
          const kind = args[0].includes('/web/') ? 'web' : 'apple';
          const probe = 'import { resolveApp } from "./scripts/app.mjs"; console.log(resolveApp('
            + JSON.stringify(args[1]) + ').dir)';
          const child = spawnSync(command, ['-e', probe], { cwd: root, env: options.env, encoding: 'utf8' });
          assert.equal(child.status, 0, child.stderr);
          assert.equal(child.stdout.trim(), dir);
          builds.push([kind, args.includes('--host')]);
          assert.equal(process.env.EXACT_APP_DIR, undefined);
          if (args.includes('--host')) throw done;
          return { status: 0, stdout: '', stderr: '' };
        }
        return { status: 0, stdout: '{}', stderr: '' };
      },
      spawn(command, args) {
        assert.ok(args[0].endsWith('/host/web/dev.mjs'));
        const child = new EventEmitter();
        child.stdout = new EventEmitter(); child.stderr = new EventEmitter();
        queueMicrotask(() => child.emit('exit', 0));
        return child;
      },
    }));
    try { await import('./scripts/metrics.mjs'); assert.fail('did not reach all build calls'); }
    catch (error) { assert.equal(error, done); }
    assert.deepEqual(builds, [['web', false], ['web', false], ['apple', false], ['apple', false], ['apple', true]]);
    assert.deepEqual(measurements, ['metrics', 'plan-metrics']);
  `;
  try {
    const r = spawnSync(process.execPath, ['-e', script], { cwd: ROOT, encoding: 'utf8', timeout: 20_000 });
    expect(r.status, r.stdout + r.stderr + (r.error ?? '')).toBe(0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

test('an app\'s selector selects it again by name and directory, when its name is not its directory\'s', () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-app-selector-')), dir = join(root, 'copy'), link = join(root, 'link');
  mkdirSync(dir);
  writeFileSync(join(dir, 'app.contract'), 'component Fixture\n  view\n    text "fixture"\n');
  symlinkSync(dir, link);
  try {
    const again = (app) => { const a = resolveApp(app.selector); return [a.name, a.dir, a.crate('gpu')]; };
    process.env.EXACT_APP_DIR = dir;
    const named = resolveApp('original-web');
    expect(again(named)).toEqual(['original', realpathSync(dir), 'original-gpu']);
    delete process.env.EXACT_APP_DIR;
    // A path through a link keeps the link's name, as the path itself did.
    expect(again(resolveApp(link))).toEqual(['link', realpathSync(dir), 'link-gpu']);
    expect(again(resolveApp('theme-fixture'))).toEqual(['theme-fixture', realpathSync(resolve(ROOT, 'apps/theme-fixture')), 'theme-fixture-gpu']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a name still resolves under apps/, as before', () => {
  delete process.env.EXACT_APP_DIR;
  const app = resolveApp('theme-fixture');
  expect([app.name, app.dir]).toEqual(['theme-fixture', realpathSync(resolve(ROOT, 'apps/theme-fixture'))]);
  expect(process.env.EXACT_APP_DIR).toBe(undefined);
  expect(resolveApp('theme-fixture-web').name).toBe('theme-fixture');
});

test('the JS build given a path writes its module crate under the target, none in the app', () => {
  const dir = realpathSync(resolve(ROOT, 'apps/theme-fixture'));
  const out = mkdtempSync(join(tmpdir(), 'exact-app-path-'));
  try {
    const env = { ...process.env };
    delete env.EXACT_APP_DIR;
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', dir, '--out', out, '--render', 'none'], { cwd: ROOT, env, encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    expect(existsSync(resolve(dir, 'crate'))).toBe(false);
    expect(existsSync(resolve(resolveApp('theme-fixture').target, 'web-js-modules/theme-fixture/crate'))).toBe(true);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 600_000);

test('an app\'s authored tests are app.test.contract, then each other *.test.contract beside it by name', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-authored-tests-'));
  try {
    expect(authoredTests(dir)).toEqual([]);
    mkdirSync(join(dir, 'nested'));
    for (const f of ['b.test.contract', 'app.contract', 'nested/c.test.contract', 'a.test.contract']) writeFileSync(join(dir, f), '');
    expect(authoredTests(dir)).toEqual([join(dir, 'a.test.contract'), join(dir, 'b.test.contract')]);
    writeFileSync(join(dir, 'app.test.contract'), '');
    expect(authoredTests(dir)).toEqual(['app', 'a', 'b'].map((n) => join(dir, `${n}.test.contract`)));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
