// A `file:` package's sources reach the next build without a `bun install` by hand (#115), in both of Bun's layouts.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileDependencies, installDifference, installedFileDependencies } from './file-dependencies.mjs';

/** An app at `<dir>/app` with `@x/p` from `file:<spec>` (relative to the app), installed by the pinned Bun. */
function installed(spec) {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-file-dep-')));
  const app = resolve(dir, 'app'), source = resolve(app, spec);
  mkdirSync(resolve(source, 'apple'), { recursive: true });
  mkdirSync(app, { recursive: true });
  writeFileSync(resolve(source, 'package.json'), '{"name":"@x/p","version":"1.0.0"}');
  writeFileSync(resolve(source, 'apple/P.swift'), 'let a = 1\n');
  writeFileSync(resolve(source, 'README.md'), 'p\n');
  writeFileSync(resolve(app, 'package.json'), JSON.stringify({ name: 'app', private: true, dependencies: { '@x/p': `file:${spec}` } }));
  const bun = spawnSync(process.execPath, ['install'], { cwd: app, encoding: 'utf8' });
  assert.equal(bun.status, 0, bun.stderr);
  return { dir, app, source, copy: resolve(app, 'node_modules/@x/p') };
}

/** The real install, counted. */
const counted = () => {
  const calls = [];
  return { calls, install: (root) => { calls.push(root); return spawnSync(process.execPath, ['install', '--frozen-lockfile'], { cwd: root, stdio: 'ignore' }).status === 0; } };
};

for (const [layout, spec] of [['a copy (inside the app)', './packages/p'], ['links (outside the app)', '../lib/p']]) {
  test(`a file: package installed as ${layout} is installed again when its source has an edit, a new file or a removed one`, () => {
    const { dir, app, source, copy } = installed(spec);
    try {
      assert.deepEqual(fileDependencies(app).map(d => [d.name, d.source, d.installed]), [['@x/p', source, copy]]);
      assert.equal(installDifference(fileDependencies(app)[0]), null);
      const run = counted();
      installedFileDependencies(app, run);
      assert.equal(run.calls.length, 0, 'an install that matches its sources is left alone');

      appendFileSync(resolve(source, 'apple/P.swift'), 'let b = 2\n');
      // A link is the source file itself; a copy is stale until installed again.
      const copied = spec.startsWith('./');
      assert.equal(installDifference(fileDependencies(app)[0]), copied ? 'apple/P.swift differs' : null);
      writeFileSync(resolve(source, 'apple/Q.swift'), 'let c = 3\n');
      assert.equal(installDifference(fileDependencies(app)[0]), copied ? 'apple/P.swift differs' : 'apple/Q.swift is not installed');
      // An edit older than a build that read the old copy: the new copy is dated now, so a build that judges by time
      // sees it; a link is the source itself, whose time is left alone.
      const old = new Date(1_700_000_000_000), started = Date.now() - 1000;
      utimesSync(resolve(source, 'apple/P.swift'), old, old);
      installedFileDependencies(app, run);
      assert.deepEqual(run.calls, [app]);
      assert.equal(readFileSync(resolve(copy, 'apple/P.swift'), 'utf8'), 'let a = 1\nlet b = 2\n');
      assert.equal(readFileSync(resolve(copy, 'apple/Q.swift'), 'utf8'), 'let c = 3\n');
      if (copied) assert.ok(statSync(resolve(copy, 'apple/P.swift')).mtimeMs >= started);
      assert.equal(statSync(resolve(source, 'apple/P.swift')).mtimeMs, old.getTime());

      rmSync(resolve(source, 'README.md'));
      assert.equal(installDifference(fileDependencies(app)[0]), 'README.md is installed but no longer in the source');
      installedFileDependencies(app, run);
      assert.equal(run.calls.length, 2);
      assert.ok(!readdirSync(copy).includes('README.md'));
      installedFileDependencies(app, run);
      assert.equal(run.calls.length, 2, 'once installed, nothing differs');
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 30_000);
}

test('a copy differing only in its time is the same; one of the same size and time is taken as the same', () => {
  const { dir, app, source, copy } = installed('./packages/p');
  try {
    const later = new Date(Date.now() + 60_000);
    utimesSync(resolve(source, 'apple/P.swift'), later, later);
    assert.equal(installDifference(fileDependencies(app)[0]), null, 'the bytes are compared when the times differ');
    // An edit of the same length is caught by its time, as Bun's copy keeps the source's.
    writeFileSync(resolve(source, 'apple/P.swift'), 'let z = 1\n');
    assert.equal(installDifference(fileDependencies(app)[0]), 'apple/P.swift differs');
    // The package's own installs and repository are not its sources.
    mkdirSync(resolve(source, 'node_modules/q'), { recursive: true });
    mkdirSync(resolve(source, '.git'));
    writeFileSync(resolve(source, '.git/HEAD'), 'ref\n');
    writeFileSync(resolve(copy, 'apple/P.swift'), 'let z = 1\n');
    const t = new Date(1_700_000_000_000);
    utimesSync(resolve(source, 'apple/P.swift'), t, t); utimesSync(resolve(copy, 'apple/P.swift'), t, t);
    assert.equal(installDifference(fileDependencies(app)[0]), null);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

test('an install that leaves a package differing is said; one that fails refuses the build with the remedy', () => {
  const { dir, app, source, copy } = installed('./packages/p');
  try {
    appendFileSync(resolve(source, 'apple/P.swift'), 'let b = 2\n');
    // An install whose result still differs (here, one that adds a file the source lacks): the build reads it, said.
    const odd = (root) => { const ok = counted().install(root); writeFileSync(resolve(copy, 'extra.txt'), 'x'); return ok; };
    const said = [], error = console.error;
    console.error = (line) => said.push(line);
    try { installedFileDependencies(app, { install: odd }); } finally { console.error = error; }
    assert.match(said.at(-1), /still differs .*the build reads what is installed \(@x\/p \(file:\.\/packages\/p\): extra\.txt is installed but no longer in the source\)/);
    appendFileSync(resolve(source, 'apple/P.swift'), 'let c = 3\n');
    assert.throws(() => installedFileDependencies(app, { install: () => false }), /bun install --frozen-lockfile failed .*run bun install there/);
    // The stale install was set aside for Bun to make again, so what is left is no package: the resolver's to report.
    assert.deepEqual(fileDependencies(app), []);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

test('a declared directory that is gone refuses the build, though its last install is still in node_modules', () => {
  const { dir, app, source, copy } = installed('./packages/p');
  try {
    rmSync(source, { recursive: true });
    const run = counted();
    assert.throws(() => installedFileDependencies(app, run), /@x\/p is file:\.\/packages\/p in .*package\.json, and .*packages\/p is not a directory, so the build would read the copy .* restore the directory/);
    assert.equal(run.calls.length, 0);
    assert.equal(readFileSync(resolve(copy, 'apple/P.swift'), 'utf8'), 'let a = 1\n', 'nothing was removed');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

// The package suites' fixtures install packages packed as a registry carries them; every build of one was refused.
test('a file: tarball is no directory to keep installed: the build goes on, before and after the tarball is removed', () => {
  const { dir, app, source } = installed('../lib/p');
  try {
    const packed = resolve(dir, 'packed');
    mkdirSync(packed);
    const pack = spawnSync(process.execPath, ['pm', 'pack', '--destination', packed, '--quiet'], { cwd: source, encoding: 'utf8' });
    assert.equal(pack.status, 0, pack.stderr);
    const tarball = resolve(packed, readdirSync(packed).find(f => f.endsWith('.tgz')));
    writeFileSync(resolve(app, 'package.json'), JSON.stringify({ name: 'app', private: true, dependencies: { '@x/p': `file:${tarball}` } }));
    const bun = spawnSync(process.execPath, ['install'], { cwd: app, encoding: 'utf8' });
    assert.equal(bun.status, 0, bun.stderr);
    assert.deepEqual(fileDependencies(app), []);
    const run = counted();
    installedFileDependencies(app, run);
    rmSync(packed, { recursive: true });
    installedFileDependencies(app, run);
    assert.equal(run.calls.length, 0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

// Two installs outside the app's own node_modules directory: a Bun workspace member's package, which Bun 1.4.2 links
// from the member's node_modules into the workspace's store (`node_modules/.bun`, its default isolated linker), and
// a package installed only in an ancestor's node_modules, which the resolver finds walking up as Node does.
test('a file: package installed in a workspace store or an ancestor node_modules is found as the resolver finds it, and installed again there', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-file-dep-ancestor-')));
  try {
    for (const layout of ['workspace', 'ancestor']) {
      const top = resolve(dir, layout), app = resolve(top, 'app'), source = resolve(app, 'packages/p');
      mkdirSync(resolve(source, 'apple'), { recursive: true });
      writeFileSync(resolve(source, 'package.json'), '{"name":"@x/p","version":"1.0.0"}');
      writeFileSync(resolve(source, 'apple/P.swift'), 'let a = 1\n');
      writeFileSync(resolve(app, 'package.json'), JSON.stringify({ name: 'app', private: true, dependencies: { '@x/p': 'file:./packages/p' } }));
      writeFileSync(resolve(top, 'package.json'), JSON.stringify(layout === 'workspace'
        ? { name: 'top', private: true, workspaces: ['app'] }
        : { name: 'top', private: true, dependencies: { '@x/p': 'file:./app/packages/p' } }));
      const bun = spawnSync(process.execPath, ['install'], { cwd: top, encoding: 'utf8' });
      assert.equal(bun.status, 0, bun.stderr);
      const [dependency] = fileDependencies(app);
      const installedAt = layout === 'workspace' ? resolve(app, 'node_modules/@x/p') : resolve(top, 'node_modules/@x/p');
      assert.equal(dependency.installed, installedAt, layout);
      if (layout === 'workspace') assert.ok(realpathSync(installedAt).includes('/node_modules/.bun/'), 'the member links into the store');
      writeFileSync(resolve(source, 'apple/Q.swift'), 'let c = 3\n');
      rmSync(resolve(source, 'apple/P.swift'));
      assert.equal(installDifference(fileDependencies(app)[0]), 'apple/Q.swift is not installed', layout);
      const run = counted();
      installedFileDependencies(app, run);
      // The workspace member's install from the member, Bun finding its workspace; the ancestor's where it is.
      assert.deepEqual(run.calls, [layout === 'workspace' ? app : top], layout);
      assert.deepEqual(readdirSync(resolve(installedAt, 'apple')), ['Q.swift'], layout);
      assert.equal(installDifference(fileDependencies(app)[0]), null, layout);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);
