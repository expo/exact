// The launcher's pinned Bun, and a native verb's platform setup, in an app `exact new` makes.
import { spawnSync } from 'node:child_process';
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { readManifest } from '../scripts/app.mjs';
import { appVerbs } from '../scripts/app-cli.mjs';
import { bringPlatform, ensureShells } from '../scripts/app-shells.mjs';
import { createApp } from './new.mjs';

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
    const missing = await bringPlatform(dir, 'ios', { report: [{ name: 'Xcode', have: 'missing', want: 'Xcode.app', ok: false, fix: 'install Xcode' }], os: 'darwin' });
    assert.match(missing.refused, /^bun exact\.mjs ios: this machine lacks what a ios build needs; nothing was written:\n  Xcode: missing \(SDK Xcode\.app\)\n    install Xcode$/);
    assert.match((await bringPlatform(dir, 'ios', { report: { error: 'no report' }, os: 'darwin' })).refused, /the setup report could not be read, so nothing was written: no report$/);
    assert.match((await bringPlatform(dir, 'macos', { report: [], os: 'linux' })).refused, /^bun exact\.mjs mac: an Apple build needs a Mac/);
    assert.match((await bringPlatform(dir, 'ios', { report: [], os: 'win32' })).refused, /use WSL on Windows/);
    assert.match((await bringPlatform(dir, 'ios', { report: [{ name: 'Apple cross SDK', have: 'missing', want: 'Xcode SDK', ok: false, fix: 'xtool sdk install' }], os: 'linux' })).refused, /Apple cross SDK: missing/);
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
