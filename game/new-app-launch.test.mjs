// The launcher's pinned Bun, and a native verb's platform setup, in an app `exact new` makes.
import { spawnSync } from 'node:child_process';
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, relative, resolve, sep } from 'node:path';
import { readManifest } from '../scripts/app.mjs';
import { appVerbs } from '../scripts/app-cli.mjs';
import { bringPlatform, ensureShells } from '../scripts/app-shells.mjs';
import { createApp } from './new.mjs';
import { checkoutOf, slashRelative, within } from '../scripts/paths.mjs';

test('an app command receives the LLVM directory relative to the launcher caller', () => {
  const parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-llvm-launcher-')));
  const caller = resolve(parent, 'caller'), app = resolve(parent, 'app');
  const bin = process.platform === 'win32' ? 'tools' : 'active/../tools';
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('EXACT')));
  try {
    mkdirSync(resolve(caller, 'sdk/version'), { recursive: true });
    if (process.platform !== 'win32') symlinkSync('sdk/version', resolve(caller, 'active'), 'dir');
    mkdirSync(resolve(caller, process.platform === 'win32' ? 'tools' : 'sdk/tools'));
    createApp(app);
    const manifest = resolve(app, 'app.json'), declared = JSON.parse(readFileSync(manifest, 'utf8'));
    declared.commands = { probe: ['bun', 'probe.mjs'] };
    writeFileSync(manifest, JSON.stringify(declared));
    writeFileSync(resolve(app, 'probe.mjs'), 'console.log(JSON.stringify([process.cwd(), process.env.EXACT_APPLE_LLVM_BIN]));');
    const result = spawnSync(process.execPath, [resolve(app, 'exact.mjs'), 'probe'], { cwd: caller, encoding: 'utf8',
      env: { ...env, EXACT_FEEDBACK: '0', EXACT_APPLE_LLVM_BIN: bin } });
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), [app, `${caller}${sep}${bin}`]);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test.each([
  { name: 'plain checkout' },
  { name: 'symlink inside the repository', link: true },
  { name: 'symlink outside the repository', link: true, external: true },
  { name: 'symlink reached from outside', link: true, outside: true },
  { name: 'linked worktree and merge', linked: 'topic' },
  { name: 'nested linked worktree and merge', linked: 'nested/trees/topic' },
  { name: 'relative override', link: true, override: '../other-sdk' },
  { name: 'absolute override', link: true, override: 'absolute' },
  { name: 'override from an unrelated cwd', link: true, override: '../other-sdk', outside: true },
])('a local checkout link bootstraps portable references: $name', row => {
  const root = realpathSync(resolve(import.meta.dir, '..'));
  const parent = realpathSync(mkdtempSync(resolve(root, 'target/launcher-reference-')));
  const main = resolve(parent, 'repo'), repo = row.linked ? resolve(parent, row.linked) : main;
  const app = resolve(repo, 'mobile'), merged = resolve(main, 'mobile');
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('EXACT') && !key.startsWith('GIT_')));
  env.EXACT_FEEDBACK = '0';
  const git = (cwd, ...args) => {
    const r = spawnSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', ...args], { cwd, env, encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
  };
  const run = (at, args, extra = {}) => {
    const cwd = row.outside ? resolve(parent, 'unrelated/work') : at;
    const r = spawnSync(process.execPath, [slashRelative(cwd, resolve(at, 'exact.mjs')), ...args], { cwd, env: { ...env, ...extra }, encoding: 'utf8', timeout: 30_000 });
    assert.equal(r.status, 0, r.stdout + r.stderr);
    return r.stdout;
  };
  const install = (at, frozen = false) => {
    const r = spawnSync(process.execPath, ['install', '--offline', ...(frozen ? ['--frozen-lockfile'] : [])], { cwd: at, env, encoding: 'utf8' });
    assert.equal(r.status, 0, r.stdout + r.stderr);
    return readFileSync(resolve(at, 'bun.lock'));
  };
  const read = (at, file) => readFileSync(resolve(at, file), 'utf8');
  const check = at => {
    assert.equal(checkoutOf(at, env), root);
    assert.equal(JSON.parse(read(at, 'app.json')).$schema, '.exact/checkout/scripts/app.schema.json');
    assert.equal(realpathSync(resolve(at, '.exact/checkout')), root);
    for (const file of ['start-here.md', 'start-here-site.md']) {
      assert.ok(read(at, 'AGENTS.md').includes(`.exact/checkout/docs/${file}`));
      assert.equal(realpathSync(resolve(at, '.exact/checkout/docs', file)), resolve(root, 'docs', file));
    }
    assert.deepEqual(JSON.parse(run(at, ['paths'])), [root, realpathSync(at)]);
    const saved = ['app.json', 'exact.mjs', 'AGENTS.md', 'package.json'].map(file => read(at, file));
    run(at, ['update']);
    assert.deepEqual(['app.json', 'exact.mjs', 'AGENTS.md', 'package.json'].map(file => read(at, file)), saved);
  };
  try {
    mkdirSync(main); mkdirSync(resolve(parent, 'unrelated/work'), { recursive: true });
    git(main, 'init', '-q'); git(main, 'commit', '--allow-empty', '-qm', 'Create the fixture');
    if (row.linked) git(main, 'worktree', 'add', '-qb', 'topic', repo);
    const checkout = row.linked || row.external ? resolve(parent, 'sdk') : row.link ? resolve(repo, 'sdk') : root;
    if (checkout !== root) symlinkSync(root, checkout, 'dir');
    if (row.override) symlinkSync(root, resolve(repo, 'other-sdk'), 'dir');
    const cwd = row.outside ? resolve(parent, 'unrelated/work') : repo;
    const made = spawnSync(process.execPath, [slashRelative(cwd, resolve(checkout, 'scripts/exact.mjs')), 'new', slashRelative(cwd, app)], { cwd, env, encoding: 'utf8' });
    assert.equal(made.status, 0, made.stdout + made.stderr);
    assert.equal(realpathSync(resolve(app, '.exact/checkout')), root);
    const manifest = JSON.parse(read(app, 'app.json'));
    manifest.commands = { paths: ['bun', 'paths.mjs'] };
    // The mounted source uses the same local checkout link as packages and the schema.
    manifest.typescript = { sources: { fixture: '.exact/checkout/packages/device' } };
    writeFileSync(resolve(app, 'app.json'), JSON.stringify(manifest, null, 2) + '\n');
    writeFileSync(resolve(app, 'paths.mjs'), 'console.log(JSON.stringify([process.env.EXACT, process.env.EXACT_APP_DIR]));');
    const extra = row.override ? { EXACT: row.override === 'absolute' ? resolve(repo, 'other-sdk') : row.override } : {};
    // Compare read-only discovery with the dependency-free launcher before it recreates the link.
    rmSync(resolve(app, '.exact/checkout'));
    assert.equal(checkoutOf(app, { ...env, ...extra }), root);
    assert.match(run(app, ['--help'], extra), /update/);
    check(app);
    run(app, ['add', 'device']);
    assert.equal(JSON.parse(read(app, 'package.json')).dependencies['@exact/device'], 'file:.exact/checkout/packages/device');
    const lock = install(app);
    check(app);
    assert.equal(realpathSync(resolve(app, JSON.parse(read(app, 'app.json')).typescript.sources.fixture)), resolve(root, 'packages/device'));
    if (row.linked) {
      git(repo, 'add', '-A'); git(repo, 'commit', '-qm', 'Create the app');
      git(main, 'merge', '-q', '--ff-only', 'topic');
      // No regeneration: the committed launcher must bootstrap a fresh working tree itself.
      assert.equal(checkoutOf(merged, env), root);
      assert.match(run(merged, ['--help']), /update/);
      assert.deepEqual(install(merged, true), lock);
      assert.equal(realpathSync(resolve(merged, JSON.parse(read(merged, 'app.json')).typescript.sources.fixture)), resolve(root, 'packages/device'));
      check(merged);
      assert.equal(spawnSync('git', ['status', '--porcelain'], { cwd: main, env, encoding: 'utf8' }).stdout, '');
    }
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 30_000);

test('bootstrap retargets installed packages, preserves the seed and refuses ambiguous discovery', () => {
  const root = realpathSync(resolve(import.meta.dir, '..'));
  const parent = realpathSync(mkdtempSync(resolve(root, 'target/retarget-checkout-')));
  const app = resolve(parent, 'mobile'), a = resolve(parent, 'sdk-a'), b = resolve(parent, 'sdk-b');
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('EXACT') && !key.startsWith('GIT_')));
  env.EXACT_FEEDBACK = '0';
  try {
    createApp(app);
    const launcher = readFileSync(resolve(app, 'exact.mjs'), 'utf8');
    for (const [sdk, value] of [[a, 'first'], [b, 'second']]) {
      mkdirSync(resolve(sdk, 'scripts'), { recursive: true });
      copyFileSync(resolve(root, 'scripts/paths.mjs'), resolve(sdk, 'scripts/paths.mjs'));
      symlinkSync(resolve(root, 'scripts/file-dependencies.mjs'), resolve(sdk, 'scripts/file-dependencies.mjs'));
      writeFileSync(resolve(sdk, 'scripts/bun.mjs'), 'export function ensureBun() {}');
      writeFileSync(resolve(sdk, 'scripts/app-cli.mjs'), 'export async function run() { console.log(process.env.EXACT_CHECKOUT_SPELLING); return 0; }');
      mkdirSync(resolve(sdk, 'packages/probe'), { recursive: true });
      writeFileSync(resolve(sdk, 'packages/probe/package.json'), '{"name":"probe","version":"1.0.0"}');
      writeFileSync(resolve(sdk, 'packages/probe/value.txt'), value);
      mkdirSync(resolve(sdk, 'snapback4/ts'), { recursive: true });
      writeFileSync(resolve(sdk, 'snapback4/ts/driver.ts'), value);
    }
    const run = extra => spawnSync(process.execPath, [resolve(app, 'exact.mjs'), '--help'], { cwd: root, env: { ...env, ...extra }, encoding: 'utf8' });
    assert.equal(run({ EXACT: '../sdk-a' }).status, 0);
    assert.equal(checkoutOf(app, env), a);
    assert.equal(run({}).stdout.trim(), a);
    const git = (cwd, ...args) => {
      const r = spawnSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', ...args], { cwd, env, encoding: 'utf8' });
      assert.equal(r.status, 0, r.stderr);
    };
    const manifest = JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8'));
    manifest.typescript = { sources: { snapback4: '.exact/checkout/snapback4/ts' } };
    writeFileSync(resolve(app, 'app.json'), JSON.stringify(manifest));
    // Force the main-worktree discovery fallback in a fresh linked tree.
    writeFileSync(resolve(app, 'exact.mjs'), launcher.replace(/^const seed = .*;$/m, 'const seed = "../sdk-a";'));
    git(app, 'init', '-q'); git(app, 'add', '-A'); git(app, 'commit', '-qm', 'Create the app');
    const linked = resolve(parent, 'nested/topic');
    git(app, 'worktree', 'add', '-q', '--detach', linked);
    assert.equal(checkoutOf(linked, env), a);
    const launched = spawnSync(process.execPath, [resolve(linked, 'exact.mjs'), '--help'], { cwd: root, env, encoding: 'utf8' });
    assert.equal(launched.status, 0, launched.stderr);
    for (const at of [app, linked]) assert.equal(readFileSync(resolve(at, manifest.typescript.sources.snapback4, 'driver.ts'), 'utf8'), 'first');
    writeFileSync(resolve(app, 'exact.mjs'), launcher);

    writeFileSync(resolve(app, 'package.json'), '{/* local packages */"dependencies":{"probe":"file:.exact/checkout/packages/probe",}}');
    const installed = spawnSync(process.execPath, ['install', '--offline'], { cwd: app, env, encoding: 'utf8' });
    assert.equal(installed.status, 0, installed.stderr);
    const lock = readFileSync(resolve(app, 'bun.lock'));
    assert.equal(readFileSync(resolve(app, 'node_modules/probe/value.txt'), 'utf8'), 'first');
    symlinkSync(b, resolve(parent, 'named-sdk'), 'dir');
    const retargeted = run({ EXACT: '../named-sdk' });
    assert.equal(retargeted.status, 0, retargeted.stdout + retargeted.stderr);
    assert.equal(retargeted.stdout.trim(), resolve(parent, 'named-sdk'));
    assert.equal(readFileSync(resolve(app, 'node_modules/probe/value.txt'), 'utf8'), 'second');
    assert.equal(readFileSync(resolve(app, '.exact/checkout/snapback4/ts/driver.ts'), 'utf8'), 'second');
    assert.deepEqual(readFileSync(resolve(app, 'bun.lock')), lock);
    assert.equal(readFileSync(resolve(app, 'exact.mjs'), 'utf8'), launcher);
    assert.equal(checkoutOf(app, { ...env, EXACT: '../named-sdk' }), b);
    for (const missing of [false, true]) {
      assert.equal(run({ EXACT: '../sdk-a' }).status, 0);
      assert.equal(readFileSync(resolve(app, 'node_modules/probe/value.txt'), 'utf8'), 'first');
      const moved = resolve(parent, 'moved-sdk-a');
      renameSync(a, moved);
      try {
        if (missing) rmSync(resolve(app, '.exact/checkout'));
        const repaired = run({ EXACT: '../named-sdk' });
        assert.equal(repaired.status, 0, repaired.stdout + repaired.stderr);
        assert.equal(repaired.stdout.trim(), resolve(parent, 'named-sdk'));
        assert.equal(checkoutOf(app, env), b);
        assert.equal(readFileSync(resolve(app, 'node_modules/probe/value.txt'), 'utf8'), 'second');
        assert.deepEqual(readFileSync(resolve(app, 'bun.lock')), lock);
      } finally { renameSync(moved, a); }
    }
    assert.throws(() => checkoutOf(app, { ...env, EXACT: '../missing' }), /no Exact checkout/);
    rmSync(resolve(app, '.exact/checkout'));
    writeFileSync(resolve(app, 'exact.mjs'), launcher.replace(/^const seed = .*;$/m, 'const seed = "../missing";'));
    const refused = run({});
    assert.throws(() => checkoutOf(app, env), /unambiguous.*EXACT=/);
    assert.notEqual(refused.status, 0);
    assert.match(refused.stderr, /unambiguous.*EXACT=/);
    assert.match(refused.stderr, /sdk-a/); assert.match(refused.stderr, /sdk-b|named-sdk/);
    renameSync(b, resolve(parent, 'nested/sdk-b'));
    assert.equal(checkoutOf(app, env), a);
    assert.equal(run({}).stdout.trim(), a);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('app and checkout spellings survive a launcher Bun restart', () => {
  const root = realpathSync(resolve(import.meta.dir, '..'));
  const parent = realpathSync(mkdtempSync(resolve(root, 'target/spelled-launcher-')));
  const physical = resolve(parent, 'physical'), alias = resolve(parent, 'alias');
  const app = resolve(physical, 'mobile'), namedApp = resolve(alias, 'mobile'), checkout = resolve(alias, 'checkout');
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('EXACT') && !key.startsWith('GIT_')));
  env.EXACT_FEEDBACK = '0';
  try {
    mkdirSync(physical); symlinkSync(physical, alias, 'dir');
    symlinkSync(root, resolve(physical, 'checkout'), 'dir');
    env.BUN = resolve(parent, process.platform === 'win32' ? 'bun.exe' : 'bun');
    copyFileSync(process.execPath, env.BUN);
    const run = (entry, args, extra = {}) => {
      const r = spawnSync(process.execPath, [entry, ...args], { cwd: parent, env: { ...env, ...extra }, encoding: 'utf8' });
      assert.equal(r.status, 0, r.stdout + r.stderr);
      return r.stdout;
    };
    run(resolve(checkout, 'scripts/exact.mjs'), ['new', namedApp]);
    const seed = JSON.parse(/^const seed = (.*);$/m.exec(readFileSync(resolve(app, 'exact.mjs'), 'utf8'))[1]);
    assert.ok(seed.endsWith('/checkout'));
    assert.equal(realpathSync(resolve(app, seed)), root);
    const manifest = JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8'));
    manifest.commands = { paths: ['bun', 'paths.mjs'] };
    writeFileSync(resolve(app, 'app.json'), JSON.stringify(manifest));
    writeFileSync(resolve(app, 'paths.mjs'), 'console.log(JSON.stringify([process.env.EXACT_APP_DIR, process.env.EXACT, process.env.EXACT_APP_SPELLING, process.env.EXACT_CHECKOUT_SPELLING]));');
    assert.deepEqual(JSON.parse(run(resolve(namedApp, 'exact.mjs'), ['paths'], { EXACT: checkout })), [app, root, namedApp, checkout]);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('a pattern names the wrapper app when launched from another app', () => {
  const parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-pattern-target-')));
  try {
    const app = resolve(parent, 'probe'), other = resolve(parent, 'scratch');
    createApp(app); createApp(other);
    const read = dir => readFileSync(resolve(dir, 'app.contract'), 'utf8');
    const untouched = read(other);
    const run = (...args) => spawnSync(process.execPath, [resolve(app, 'exact.mjs'), 'pattern', ...args], {
      cwd: other, env: { ...process.env, EXACT: resolve(import.meta.dir, '..'), EXACT_FEEDBACK: '0' }, encoding: 'utf8', timeout: 30_000,
    });
    const help = run('--help');
    assert.equal(help.status, 0, help.stderr);
    assert.match(help.stdout, /wrapper selects its own app/);
    assert.match(help.stdout, /manually emptied app.contract/);
    assert.match(help.stdout, /new ..\/pattern-scratch/);
    const applied = run('tabs');
    assert.equal(applied.status, 0, applied.stderr);
    assert.ok(applied.stdout.includes(`App: ${app}\n`), applied.stdout);
    assert.match(read(app), /tablist/);
    assert.equal(read(other), untouched);
    writeFileSync(resolve(app, 'app.contract'), '');
    const refused = run('one-screen');
    assert.equal(refused.status, 1, refused.stderr);
    assert.match(refused.stderr, /app.contract was edited/);
    assert.equal(read(app), '');
    const forced = run('one-screen', '--force');
    assert.equal(forced.status, 0, forced.stderr);
    assert.ok(forced.stdout.includes(`App: ${app}\n`), forced.stdout);
    assert.ok(read(app).length > 0);
    assert.equal(read(other), untouched);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('launchers and generated own commands find pinned Bun with an old PATH', () => {
  const old = resolve(process.env.HOME, '.bun/bin/bun');
  if (!existsSync(old)) return;
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-bun-'));
  try {
    const dir = resolve(parent, 'pinned'); createApp(dir);
    const manifestPath = resolve(dir, 'app.json'), manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    manifest.commands = {runtime: ['bun', 'runtime.mjs']};
    writeFileSync(manifestPath, JSON.stringify(manifest));
    writeFileSync(resolve(dir, 'runtime.mjs'), 'console.log(JSON.stringify({version:process.versions.bun,bun:process.env.BUN,path:Bun.which("bun")}));');
    for (const explicit of [false, true]) {
      const directEnv = {...process.env, PATH: '/usr/bin:/bin'};
      if (explicit) directEnv.BUN = process.execPath; else delete directEnv.BUN;
      const direct = spawnSync(resolve(dir, 'exact.mjs'), ['runtime'], {env: directEnv, encoding: 'utf8'});
      assert.equal(direct.status, 0, direct.stderr);
      assert.equal(JSON.parse(direct.stdout).version, process.versions.bun);
      const rootDirect = spawnSync(resolve(import.meta.dirname, '../scripts/exact.mjs'), ['list'], {env: directEnv, encoding: 'utf8'});
      assert.equal(rootDirect.status, 0, rootDirect.stderr);
      const env = {...process.env, PATH: `${resolve(process.env.HOME, '.bun/bin')}:/usr/bin:/bin`};
      if (explicit) env.BUN = process.execPath; else delete env.BUN;
      const run = spawnSync(old, [resolve(dir, 'exact.mjs'), 'runtime'], {env, encoding: 'utf8'});
      assert.equal(run.status, 0, run.stderr);
      const observed = JSON.parse(run.stdout);
      assert.equal(observed.version, process.versions.bun);
      assert.equal(realpathSync(observed.bun), realpathSync(process.execPath));
      assert.equal(realpathSync(observed.path), realpathSync(process.execPath));
      const root = spawnSync(old, [resolve(import.meta.dirname, '../scripts/exact.mjs'), 'list'], {env, encoding: 'utf8'});
      assert.equal(root.status, 0, root.stderr);
    }
  } finally {rmSync(parent, {recursive: true, force: true});}
});

test('a launcher whose creating Bun is not on this machine runs the Bun on PATH', () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-bun-'));
  try {
    const dir = resolve(parent, 'moved'); createApp(dir);
    const launcher = resolve(dir, 'exact.mjs'), text = readFileSync(launcher, 'utf8');
    // The app as another account or machine checks it out: the Bun `exact new` ran is not there.
    writeFileSync(launcher, text.replace(`'${process.execPath}'`, `'${resolve(parent, 'gone/bun')}'`), {mode: 0o755});
    const env = {...process.env, HOME: parent, PATH: `${dirname(process.execPath)}:/usr/bin:/bin`};
    delete env.BUN;
    const run = spawnSync(launcher, ['--help'], {env, encoding: 'utf8'});
    assert.equal(run.status, 0, run.stderr);
  } finally {rmSync(parent, {recursive: true, force: true});}
});

test('a script moves to the pinned Bun, and a test file, whose `bun test` cannot move, names the Bun to run it with', () => {
  const parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-bun-')));
  try {
    const pinned = resolve(parent, 'bun'), script = resolve(parent, 'script.mjs'), file = resolve(parent, 'pinned.test.mjs');
    writeFileSync(pinned, '#!/bin/sh\nif [ "$1" = --version ]; then echo 99.0.0; else echo "pinned Bun ran $*"; fi\n', {mode: 0o755});
    const source = `import {ensureBun} from ${JSON.stringify(resolve(import.meta.dir, '../scripts/bun.mjs'))};\nensureBun('1.0.0');\n`;
    writeFileSync(script, source);
    writeFileSync(file, `${source}import {test} from 'bun:test';\ntest('ran', () => {});\n`);
    const env = {...process.env, BUN: pinned};
    const moved = spawnSync(process.execPath, [script], {cwd: parent, env, encoding: 'utf8'});
    assert.deepEqual([moved.status, moved.stdout], [0, `pinned Bun ran ${script}\n`]);
    const tested = spawnSync(process.execPath, ['test', file], {cwd: parent, env, encoding: 'utf8'});
    assert.notEqual(tested.status, 0);
    assert.ok(tested.stderr.includes(`cannot hand \`bun test\` to this checkout's Bun; run ${pinned} test ${file}`), tested.stderr);
    assert.ok(!tested.stdout.includes('pinned Bun ran'), tested.stdout);
  } finally {rmSync(parent, {recursive: true, force: true});}
});

// LLP 1153 D1.2: a native verb names its platform only after that platform's setup rows, and writes nothing when one
// is missing, the report could not be read, or the machine cannot build it at all.
test('a native verb refuses before writing when its platform\'s setup is missing, and names it otherwise', async () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-new-'));
  try {
    const dir = resolve(parent, 'field-log');
    createApp(dir);
    const manifest = () => readFileSync(resolve(dir, 'app.json'), 'utf8'), before = manifest();
    const missing = await bringPlatform(dir, 'ios', { report: [{ name: 'Xcode', required: true, have: 'missing', want: 'Xcode.app', ok: false, fix: 'install Xcode' }], os: 'darwin' });
    assert.match(missing.refused, /^bun exact\.mjs ios: this machine lacks what a ios build needs; nothing was written:\n  Xcode: missing \(SDK Xcode\.app\)\n    install Xcode$/);
    assert.match((await bringPlatform(dir, 'ios', { report: { error: 'no report' }, os: 'darwin' })).refused, /the setup report could not be read, so nothing was written: no report$/);
    assert.match((await bringPlatform(dir, 'macos', { report: [], os: 'linux' })).refused, /^bun exact\.mjs mac: an Apple build needs a Mac/);
    assert.match((await bringPlatform(dir, 'ios', { report: [], os: 'win32' })).refused, /use WSL on Windows/);
    assert.match((await bringPlatform(dir, 'ios', { report: [{ name: 'Apple cross SDK', required: true, have: 'missing', want: 'Xcode SDK', ok: false, fix: 'xtool sdk install' }], os: 'linux' })).refused, /Apple cross SDK: missing/);
    assert.match((await bringPlatform(dir, 'windows', { report: [], os: 'darwin' })).refused, /^bun exact\.mjs windows: a Windows build needs Windows/);
    assert.equal(manifest(), before);
    assert.ok(!existsSync(resolve(dir, '.exact/shells')), 'a refusal generates nothing');
    const brought = await bringPlatform(dir, 'macos', { report: [{ name: 'Xcode', ok: true }], os: 'darwin' });
    assert.match(brought.lines[0], /^app\.json: host\.macos, deploy\.store\.macos added: the app now ships macos \(bun exact\.mjs update --without macos stops it\)$/);
    assert.match(brought.lines[1], /the native workspace \(web, apple\), generated from app\.json$/);
    assert.ok(existsSync(resolve(dir, '.exact/shells/apple/Cargo.toml')));
    // Named already: the report is not read, and a current workspace says nothing.
    assert.deepEqual(await bringPlatform(dir, 'macos', { report: { error: 'not read' }, os: 'darwin' }), { lines: [] });
    const cross = await bringPlatform(dir, 'ios', { report: [{ name: 'Apple cross SDK', ok: true }], os: 'linux' });
    assert.match(cross.lines[0], /host\.ios, deploy\.store\.ios added/);
    assert.ok(JSON.parse(manifest()).host.ios);
    assert.deepEqual(appVerbs('field-log', dir, { os: 'linux' }).ios, ['host/apple/build.mjs', '--ios', 'field-log-apple', '--no-install']);
    assert.deepEqual(appVerbs('field-log', dir, { os: 'darwin' }).ios, ['host/apple/build.mjs', '--ios', 'field-log-apple']);
    // An app without app.json had the defaults readManifest supplies; naming a platform writes them down first.
    rmSync(resolve(dir, 'app.json'));
    const written = await bringPlatform(dir, 'linux', { report: [], os: 'darwin' });
    assert.match(written.lines[0], /^app\.json: written with the defaults the app had without one$/);
    assert.deepEqual(JSON.parse(manifest()).app, { id: 'com.exact.field-log', name: 'Field Log' });
    assert.deepEqual(Object.keys(JSON.parse(manifest()).host), ['web', 'linux']);
    // The stamp: a changed input, or a crate's file gone, regenerates; the same inputs do not.
    const shells = resolve(dir, '.exact/shells');
    assert.equal(ensureShells(dir).wrote, false);
    rmSync(resolve(shells, 'linux/Cargo.toml'));
    assert.equal(ensureShells(dir).wrote, true);
    rmSync(resolve(shells, 'web/build.rs'));
    assert.equal(ensureShells(dir).wrote, true, 'every generated file is checked');
    writeFileSync(resolve(shells, '.stamp.json'), JSON.stringify({ ...JSON.parse(readFileSync(resolve(shells, '.stamp.json'), 'utf8')), inputs: 'stale' }));
    assert.equal(ensureShells(dir).wrote, true);
    assert.equal(ensureShells(dir).wrote, false);
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 60_000);

// LLP 1153 D1.4, D2.1: the app's files say whether it authored a workspace; a workspace above it owns it only when its
// members list the app or one of its crates, as Cargo reads them.
