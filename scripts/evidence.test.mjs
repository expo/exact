// What has been run against an app's current sources (LLP 1146 D2).
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, renameSync, rmSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { resolveApp } from './app.mjs';
import { changedPaths, hostsTouched, standing } from './evidence.mjs';
import { appleArtifacts, appleRunBinary } from '../host/apple/build.mjs';

const ROOT = resolve(import.meta.dir, '..');
const allHosts = ['web', 'macos', 'ios', 'tvos', 'linux', 'windows', 'android'];
const appAt = (path) => {
  const previous = process.env.EXACT_APP_DIR;
  try { return resolveApp(resolve(ROOT, path)); }
  finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR;
    else process.env.EXACT_APP_DIR = previous;
  }
};

test('a run is current while its build is the host\'s and fresh', () => {
  const runs = [
    { host: 'web', build: 'b1', what: 'test a' },
    { host: 'macos', build: 'm1', what: 'test a' },
    { host: 'ios', build: 'i0', what: 'test a' },
    { host: 'linux', build: null, what: 'drive tree' },
  ];
  const now = { web: { id: 'b1', changed: [] }, macos: { id: 'm1', changed: ['apps/x/app.contract'] }, ios: { id: 'i1', changed: [] } };
  expect(standing(null, runs, (h) => now[h] ?? null).map((r) => [r.host, r.standing, r.why])).toEqual([
    ['web', 'current', undefined],
    ['macos', 'stale', 'apps/x/app.contract changed after'],
    ['ios', 'stale', 'a later build replaced the one it ran on'],
    ['linux', 'unchecked', 'a linux build is not identified yet'],
  ]);
});

test('evidence lists native runs whose receipts predate backend metadata', () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(ROOT, 'target/evidence-backend-'));
  const put = (path, text = '') => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, text); };
  try {
    put(resolve(dir, 'app.contract'), 'component App\n  text "hello"\n');
    put(resolve(dir, 'app.json'), JSON.stringify({ name: 'evidence-backend', app: { id: 'com.exact.evidence-backend', name: 'Evidence Backend' } }));
    put(resolve(dir, 'app.ts'), "const origin = 'http://127.0.0.1:4400';\n");
    const app = resolveApp(dir), all = [];
    for (const host of ['macos', 'ios']) {
      const paths = appleArtifacts(app, { destination: host === 'ios' ? 'ios-simulator' : 'macos' });
      const receipt = resolve(host === 'ios' ? paths.bundle : paths.products, 'receipt.json');
      const text = JSON.stringify({ target: paths.target, build: { trust: 'development' } });
      put(receipt, text);
      if (host === 'macos') {
        put(appleRunBinary(app));
        put(resolve(paths.bundle, 'Contents/Resources/receipt.json'), text);
      }
      const future = new Date(Date.now() + 60_000);
      utimesSync(receipt, future, future);
      all.push({ host, build: createHash('sha256').update(text).digest('hex').slice(0, 16), what: 'test fixture' });
    }
    expect(standing(app, all).map(r => r.standing)).toEqual(['current', 'current']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a change touches the hosts whose code it is', () => {
  expect(hostsTouched(['host/apple/Sources/ExactKit/Mac/PresenterMac.swift'])).toEqual(['macos', 'ios', 'tvos']);
  expect(hostsTouched(['host/web-js/rt.js', 'host/windows/src/main.rs'])).toEqual(['web', 'windows']);
  expect(hostsTouched(['runner/src/perf.rs'])).toHaveLength(7);
  expect(hostsTouched(['docs/testing.md', 'llp/1146-x.rfc.md'])).toEqual([]);
});

test.each(['runner', 'canvas', 'route', 'color', 'logic', 'markdown', 'textflow', 'update', 'grants',
  'data', 'tamber', 'bake', 'gpu', 'game/engine', 'a-new-shared-crate'])(
  '%s touches every host', (dir) => {
    expect(hostsTouched([`${dir}/src/lib.rs`])).toEqual(allHosts);
  },
);

test.each([
  ['host/apple-update/src/lib.rs', ['macos', 'ios', 'tvos']],
  ['host/apple-painted/src/lib.rs', ['macos']],
  ['host/web-capabilities/src/lib.rs', ['web']],
  ['host/web-geometry/src/lib.rs', ['web']],
  ['host/linux-update/src/lib.rs', ['linux']],
  ['host/linux/src/presenter.rs', ['linux', 'windows', 'android']],
  ['host/ink/src/lib.rs', ['linux', 'windows', 'android']],
  ['host/listen/src/lib.rs', ['macos', 'linux', 'android']],
  ['host/raster/src/lib.rs', ['macos', 'ios', 'tvos', 'linux', 'windows', 'android']],
  ['host/render/src/lib.rs', ['web']],
  ['host/svg-raster/src/lib.rs', ['macos', 'ios', 'tvos', 'linux', 'windows', 'android']],
])('%s touches its host family', (path, hosts) => {
  expect(hostsTouched([path])).toEqual(hosts);
});

test('Mac coverage follows the selected app\'s dependencies', () => {
  const heavy = appAt('bench/heavy-list/exact-heavylist'), caltrain = appAt('apps/caltrain');
  expect(heavy.manifest.host.macos.presenter).toBeUndefined();
  for (const path of ['host/ink/src/device/apple.rs', 'host/linux/src/presenter.rs']) {
    expect(hostsTouched([path], heavy)).toEqual(['macos', 'linux', 'windows', 'android']);
    expect(hostsTouched([path], caltrain)).toEqual(['linux', 'windows', 'android']);
    expect(hostsTouched([path], { ...caltrain, manifest: { host: { macos: { presenter: 'painted' } } } }))
      .toEqual(['macos', 'linux', 'windows', 'android']);
  }
});

test('Mac coverage respects target conditions and inherited path dependencies without a lock', () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(ROOT, 'target/evidence-test-'));
  try {
    for (const child of ['apple', 'data']) mkdirSync(resolve(dir, child));
    writeFileSync(resolve(dir, 'Cargo.toml'), `[workspace]
members = ["apple", "data"]
[workspace.dependencies]
probe = { package = "exact-ink", path = ${JSON.stringify(resolve(ROOT, 'host/ink'))} }
`);
    writeFileSync(resolve(dir, 'apple/Cargo.toml'), `[package]
name = "app-apple"
[dependencies]
feed = { path = "../data" }
[target.'cfg(all(unix, any(target_os = "linux", target_os = "windows")))'.dependencies]
exact-linux = { path = ${JSON.stringify(resolve(ROOT, 'host/linux'))} }
`);
    const data = (target) => `[package]
name = "app-data"
[target.'${target}'.dependencies]
probe.workspace = true
`;
    const app = { dir };
    for (const target of ['cfg(all(target_vendor = "apple", not(target_os = "ios")))', 'x86_64-apple-darwin']) {
      writeFileSync(resolve(dir, 'data/Cargo.toml'), data(target));
      expect(hostsTouched(['host/ink/src/lib.rs'], app)).toEqual(['macos', 'linux', 'windows', 'android']);
      expect(hostsTouched(['host/linux/src/presenter.rs'], app)).toEqual(['linux', 'windows', 'android']);
    }
    writeFileSync(resolve(dir, 'data/Cargo.toml'), data('cfg(not(unix))'));
    expect(hostsTouched(['host/ink/src/lib.rs'], app)).toEqual(['linux', 'windows', 'android']);
    expect(existsSync(resolve(dir, 'Cargo.lock'))).toBe(false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.each(['host/executor/src/lib.rs', 'host/executor/src/async.rs'])(
  '%s touches native apps and web rendering', (path) => {
    expect(hostsTouched([path])).toEqual(allHosts);
  },
);

test.each([
  ['host/executor/src/process.rs', ['web', 'macos', 'linux']],
  ['host/executor/src/process_supervise.rs', ['web', 'macos', 'linux']],
  ['host/executor/src/process_off.rs', ['web', 'ios', 'tvos', 'windows', 'android']],
  ['host/apple/src/executor_order_tests.rs', ['macos', 'ios', 'tvos']],
])('%s touches only the hosts that compile it', (path, hosts) => {
  expect(hostsTouched([path])).toEqual(hosts);
  expect(hostsTouched([path], appAt('apps/realworld'))).toEqual(hosts);
});

test('only the selected app touches its hosts', () => {
  const app = { dir: resolve(ROOT, 'apps/demo') };
  expect(hostsTouched(['apps/demo/app.contract', 'apps/demo/data/src/lib.rs'], app)).toEqual(allHosts);
  for (const [dir, hosts] of [
    ['apple', ['macos', 'ios', 'tvos']], ['web', ['web']], ['linux', ['linux']],
    ['windows', ['windows']], ['android', ['android']],
  ]) expect(hostsTouched([`apps/demo/${dir}/src/lib.rs`], app)).toEqual(hosts);
  expect(hostsTouched(['apps/else/apple/src/lib.rs', 'apps/demo-other/app.contract'], app)).toEqual([]);
  expect(hostsTouched(['apps/demo/app.contract'], { dir: resolve(ROOT, '../outside') })).toEqual([]);
  expect(hostsTouched(['apps/demo/app.contract'], { dir: ROOT })).toEqual([]);
  expect(hostsTouched(['docs/testing.md', 'llp/1146-x.rfc.md', 'issues/open.md', 'rules/RULES.md', 'README.md'], app)).toEqual([]);
});

test('an app\'s Linux render entry and package files also touch its web pages', () => {
  const app = appAt('apps/caltrain');
  expect(hostsTouched(['apps/caltrain/linux/src/bin/caltrain-render.rs'], app)).toEqual(['web']);
  expect(hostsTouched(['apps/caltrain/linux/build.rs'], app)).toEqual(['web', 'linux']);
  expect(hostsTouched(['apps/caltrain/linux/Cargo.toml'], app)).toEqual(['web', 'linux']);
  expect(hostsTouched(['apps/caltrain/linux/src/main.rs'], app)).toEqual(['linux']);
  expect(hostsTouched(['apps/caltrain/linux/src/bin/caltrain-render.rs'], appAt('apps/realworld'))).toEqual([]);
});

test.each(['game/games/forest', 'game/games/rivals', 'game/bench/cubes', 'bench/list/exact-listbench',
  'bench/heavy-list/exact-heavylist'])(
  '%s affects only the selected app', (dir) => {
    const path = `${dir}/app.contract`;
    expect(hostsTouched([path], { dir: resolve(ROOT, 'apps/caltrain') })).toEqual([]);
    const app = { dir: resolve(ROOT, dir) };
    expect(hostsTouched([path], app)).toEqual(allHosts);
    expect(hostsTouched([`${dir}/web/src/lib.rs`], app)).toEqual(['web']);
  },
);

test('a symlinked checkout recognizes the selected app', () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const root = mkdtempSync(resolve(ROOT, 'target/evidence-test-'));
  try {
    const link = resolve(root, 'checkout');
    symlinkSync(ROOT, link, 'junction');
    const code = `import { hostsTouched } from ${JSON.stringify(resolve(link, 'scripts/evidence.mjs'))};
      import { resolveApp } from ${JSON.stringify(resolve(link, 'scripts/app.mjs'))};
      console.log(JSON.stringify(hostsTouched(['apps/caltrain/app.contract'], resolveApp('caltrain'))));`;
    const run = Bun.spawnSync([process.execPath, '--preserve-symlinks', '--eval', code], {
      cwd: ROOT, env: { ...process.env, EXACT_APP_DIR: '', CARGO_TARGET_DIR: resolve(root, 'build') },
    });
    expect(run.exitCode, run.stderr.toString()).toBe(0);
    expect(JSON.parse(run.stdout.toString())).toEqual(allHosts);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('changed paths preserve unstaged edits, filenames and both sides of a rename', () => {
  const target = resolve(ROOT, 'target');
  mkdirSync(target, { recursive: true });
  const root = mkdtempSync(resolve(target, 'evidence-test-'));
  const git = (...args) => {
    const run = spawnSync('git', ['-c', 'user.name=Evidence test', '-c', 'user.email=evidence@example.invalid',
      '-c', 'commit.gpgsign=false', ...args], { cwd: root, encoding: 'utf8' });
    expect(run.status, run.stderr).toBe(0);
    return run.stdout.trim();
  };
  const write = (path, text) => {
    mkdirSync(dirname(resolve(root, path)), { recursive: true });
    writeFileSync(resolve(root, path), text);
  };
  try {
    git('init', '-q');
    write('runner/src/lib.rs', 'runner\n');
    write('host/apple/old.swift', 'apple\n');
    write('host/linux/src/lib.rs', 'linux\n');
    git('add', '.');
    git('commit', '-qm', 'Fixture');
    const base = git('rev-parse', 'HEAD');
    write('runner/src/lib.rs', 'runner changed\n');
    expect(changedPaths(base, root)).toEqual(['runner/src/lib.rs']);
    expect(hostsTouched(changedPaths(base, root))).toEqual(allHosts);

    write('host/linux/src/lib.rs', 'linux staged\n');
    git('add', 'host/linux/src/lib.rs');
    write('host/linux/src/lib.rs', 'linux\n');
    expect(changedPaths(base, root).sort()).toEqual(['host/linux/src/lib.rs', 'runner/src/lib.rs']);

    git('add', '.');
    git('commit', '-qm', 'Changed runner');
    write('runner/src/lib.rs', 'runner\n');
    write('host/web/new file with spaces.js', 'web\n');
    renameSync(resolve(root, 'host/apple/old.swift'), resolve(root, 'host/web/renamed.swift'));
    git('add', '.');
    write('canvas/src/naïve.rs', 'canvas\n');
    write('.gitignore', 'ignored.rs\n');
    write('ignored.rs', 'ignored\n');
    const paths = changedPaths(base, root);
    expect(paths.sort()).toEqual([
      '.gitignore', 'canvas/src/naïve.rs', 'host/apple/old.swift', 'host/web/new file with spaces.js',
      'host/web/renamed.swift', 'runner/src/lib.rs',
    ]);
    if (process.platform !== 'win32') {
      const newline = 'host/web/new file\nwith spaces.js';
      write(newline, 'web\n');
      git('add', newline);
      expect(changedPaths(base, root).sort()).toEqual([...paths, newline].sort());
    }

    // Past 1 MiB of names, in paths that stay well under macOS's PATH_MAX (1,024 bytes) once the fixture's root is joined.
    const prefix = Array.from({ length: 3 }, (_, i) => `${i}-${'x'.repeat(180)}`).join('/');
    const many = Array.from({ length: 2000 }, (_, i) => `canvas/${prefix}/${i}.rs`);
    expect(Buffer.byteLength(many.join('\0'))).toBeGreaterThan(1024 * 1024);
    for (const path of many) write(path, 'canvas\n');
    const before = paths.concat(process.platform === 'win32' ? [] : ['host/web/new file\nwith spaces.js']);
    expect(changedPaths(base, root).sort()).toEqual([...before, ...many].sort());
    git('add', '.');
    git('commit', '-qm', 'Many changed paths');
    expect(changedPaths(base, root).sort()).toEqual([...before.filter(path => path !== 'runner/src/lib.rs'), ...many].sort());
  } finally { rmSync(root, { recursive: true, force: true }); }
});
