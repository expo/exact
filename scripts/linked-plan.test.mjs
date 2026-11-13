// Package and linked-bake freshness (scripts/linked-plan.mjs, scripts/cargo-dep-info.mjs and agent-launch.mjs
// `receiptChanges`): what a watched package directory, an optional design and a linked bake's inputs make stale.
import { afterEach, beforeEach, test, expect } from 'bun:test';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { bakeTarget } from './app.mjs';
import { pinned } from './test-workspace.mjs';

const inheritedTarget = process.env.CARGO_TARGET_DIR;
beforeEach(() => { delete process.env.CARGO_TARGET_DIR; });
afterEach(() => { if (inheritedTarget === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = inheritedTarget; });

test('package directory freshness ignores tests and keeps production entries and explicit inputs', async () => {
  const {receiptChanges, watchedNewer} = await import('./agent-launch.mjs');
  const {linkedInputs} = await import('./linked-plan.mjs');
  const scratch = resolve(import.meta.dir, '../target');
  mkdirSync(scratch, {recursive:true});
  const dir = mkdtempSync(resolve(scratch, 'package-freshness-'));
  try {
    const app = resolve(dir, 'app'), pkg = resolve(dir, 'packages/photos');
    const old = new Date(0), built = new Date(Date.now() + 60_000), edited = new Date(+built + 60_000);
    const write = (path, text = '') => { mkdirSync(dirname(path), {recursive:true}); writeFileSync(path, text); utimesSync(path, old, old); };
    const source = resolve(pkg, 'src/index.ts'), manifest = resolve(pkg, 'package.json'), drive = resolve(pkg, 'test/drives.mjs');
    write(source); write(manifest, '{}'); write(drive); write(resolve(app, 'app.json'), '{}');
    const fixture = {dir:app, workspace:dir, target:resolve(app, 'target'), manifest:{}, crate:()=>'native-apple'};
    const changed = () => receiptChanges(receipt, fixture).map(path => resolve(import.meta.dir, '..', path));
    const watched = paths => watchedNewer(+built, paths).map(path => resolve(import.meta.dir, '..', path));
    const info = resolve(fixture.target, 'aarch64-apple-darwin/debug/libnative_apple.d');
    write(info, `archive: ${pkg}\n`);
    const receipt = resolve(fixture.target, 'receipt.json');
    write(receipt, JSON.stringify({target:'aarch64-apple-darwin', started:built.toISOString()}));
    utimesSync(receipt, built, built);
    for (const at of [app, pkg, dirname(pkg), dirname(source), dirname(drive)]) utimesSync(at, old, old);
    expect(changed()).toEqual([]);
    utimesSync(drive, edited, edited);
    expect(changed()).toEqual([]);
    write(resolve(pkg, 'test/another.mjs'));
    utimesSync(dirname(drive), edited, edited);
    expect(changed()).toEqual([]);
    rmSync(resolve(pkg, 'test/another.mjs'));
    utimesSync(dirname(drive), edited, edited);
    expect(changed()).toEqual([]);
    expect(watched([drive])).toEqual([drive]);
    for (const input of [source, manifest]) {
      utimesSync(input, edited, edited);
      expect(changed()).toContain(input);
      utimesSync(input, old, old);
    }
    // An old file moved into production still changes the directory's entries.
    write(resolve(pkg, 'src/added.ts'));
    utimesSync(dirname(source), edited, edited);
    expect(changed()).toContain(dirname(source));
    rmSync(source);
    utimesSync(dirname(source), edited, edited);
    expect(changed()).toContain(dirname(source));
    expect(watched([source])).toEqual([source]);
    utimesSync(dirname(source), old, old);
    const design = resolve(pkg, 'photos.tamber');
    expect(watched([design])).toEqual([design]);
    write(design);
    utimesSync(pkg, edited, edited);
    expect(changed()).toContain(pkg);
    const linked = resolve(fixture.target, 'linked');
    write(resolve(linked, 'inputs.json'), JSON.stringify({paths:[source, manifest, design], missing:[design]}));
    write(info, `archive: ${manifest}\n`);
    write(receipt, JSON.stringify({target:'aarch64-apple-darwin', started:built.toISOString(), build:{linkedPlan:linked, linkedInputs:linkedInputs(linked)}}));
    utimesSync(receipt, built, built);
    rmSync(design);
    // The source was present at build; a missing optional design is recorded separately.
    expect(changed()).toEqual([source]);
    write(source);
    expect(changed()).toEqual([]);
    write(design);
    expect(changed()).toEqual([design]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.each(['create tests', 'remove tests', 'edit tests', 'add test file', 'remove test file', 'move production then create tests', 'remove production then create tests',
  'remove explicit extensionless input', 'replace production', 'edit production at an old mtime']
  .flatMap(mode => ['receipt', 'linked bake'].map(record => [mode, record])))(
  'package freshness inventories detect %s in a %s', async (mode, record) => {
    const {receiptChanges} = await import('./agent-launch.mjs');
    const {packageDirectories} = await import('./cargo-dep-info.mjs');
    const {linkedInputs} = await import('./linked-plan.mjs');
    const dir = mkdtempSync(resolve(import.meta.dir, '../target/package-entries-'));
    try {
      const app = resolve(dir, 'app'), pkg = resolve(dir, 'packages/photos');
      const source = resolve(pkg, 'index.contract'), make = resolve(pkg, 'Makefile');
      const old = new Date(0), built = new Date(Date.now() - 60_000);
      mkdirSync(pkg, {recursive:true}); mkdirSync(app);
      for (const path of [source, make, resolve(pkg, 'package.json'), resolve(app, 'app.json')]) {
        writeFileSync(path, '{}'); utimesSync(path, old, old);
      }
      if (mode === 'remove tests') mkdirSync(resolve(pkg, 'test'));
      if (['edit tests', 'add test file', 'remove test file'].includes(mode)) {
        mkdirSync(resolve(pkg, 'test')); writeFileSync(resolve(pkg, 'test/drives.mjs'), 'test');
      }
      const fixture = {dir:app, workspace:dir, target:resolve(app, 'target'), manifest:{}, crate:()=>'native-apple'};
      const info = resolve(fixture.target, 'aarch64-apple-darwin/debug/libnative_apple.d');
      mkdirSync(dirname(info), {recursive:true}); writeFileSync(info, `archive: ${pkg} ${make}\n`);
      const receipt = resolve(fixture.target, 'receipt.json');
      const watchedDirectories = packageDirectories([pkg]);
      let build = {watchedDirectories};
      if (record === 'linked bake') {
        const linkedPlan = resolve(fixture.target, 'linked'); mkdirSync(linkedPlan);
        writeFileSync(resolve(linkedPlan, 'inputs.json'), JSON.stringify({paths:[pkg], watchedDirectories}));
        build = {linkedPlan, linkedInputs:linkedInputs(linkedPlan)};
      }
      writeFileSync(receipt, JSON.stringify({target:'aarch64-apple-darwin', started:built.toISOString(), build}));
      utimesSync(pkg, old, old);
      const changed = () => receiptChanges(receipt, fixture).map(path => resolve(import.meta.dir, '..', path));
      expect(changed()).toEqual([]);
      if (mode === 'create tests') {
        mkdirSync(resolve(pkg, 'test'));
        expect(changed()).toEqual([]);
      } else if (mode === 'remove tests') {
        rmSync(resolve(pkg, 'test'), {recursive:true});
        expect(changed()).toEqual([]);
      } else if (mode === 'edit tests' || mode === 'add test file') {
        writeFileSync(resolve(pkg, 'test', mode === 'edit tests' ? 'drives.mjs' : 'another.mjs'), 'new test');
        expect(changed()).toEqual([]);
      } else if (mode === 'remove test file') {
        rmSync(resolve(pkg, 'test/drives.mjs'));
        expect(changed()).toEqual([]);
      } else if (mode.endsWith('then create tests')) {
        if (mode.startsWith('move')) {
          const moved = resolve(dir, 'older.contract'); writeFileSync(moved, 'production'); utimesSync(moved, old, old);
          renameSync(moved, resolve(pkg, 'added.contract'));
        } else rmSync(source);
        mkdirSync(resolve(pkg, 'test'));
        expect(changed()).toContain(pkg);
      } else if (mode === 'remove explicit extensionless input') {
        rmSync(make);
        expect(changed()).toContain(make);
      } else {
        const replacement = mode === 'replace production' ? resolve(dir, 'older.contract') : source;
        writeFileSync(replacement, 'new production bytes'); utimesSync(replacement, old, old);
        if (replacement !== source) renameSync(replacement, source);
        expect(changed()).toContain(pkg);
      }
    } finally { rmSync(dir, {recursive:true, force:true}); }
  });

test('a linked build is checked against what the bake it links read, not a later bake\'s record', async () => {
  const {receiptChanges} = await import('./agent-launch.mjs');
  const {packageDirectories} = await import('./cargo-dep-info.mjs');
  const {linkedInputs} = await import('./linked-plan.mjs');
  const dir = mkdtempSync(resolve(import.meta.dir, '../target/linked-receipt-'));
  try {
    const app = resolve(dir, 'app'), pkg = resolve(dir, 'packages/photos'), linked = resolve(app, 'target/linked');
    const source = resolve(pkg, 'index.contract'), design = resolve(pkg, 'photos.tamber');
    const old = new Date(0), built = new Date(Date.now() - 60_000);
    mkdirSync(pkg, {recursive:true}); mkdirSync(linked, {recursive:true});
    for (const path of [source, resolve(pkg, 'package.json'), resolve(app, 'app.json')]) { writeFileSync(path, '{}'); utimesSync(path, old, old); }
    const fixture = {dir:app, workspace:dir, target:resolve(app, 'target'), manifest:{}, crate:()=>'native-apple'};
    const info = resolve(fixture.target, 'aarch64-apple-darwin/debug/libnative_apple.d');
    mkdirSync(dirname(info), {recursive:true}); writeFileSync(info, `archive: ${source}\n`);
    const bake = () => writeFileSync(resolve(linked, 'inputs.json'), JSON.stringify({paths:[pkg, design],
      missing:existsSync(design) ? [] : [design], watchedDirectories:packageDirectories([pkg])}));
    bake();
    const receipt = resolve(fixture.target, 'receipt.json');
    writeFileSync(receipt, JSON.stringify({target:'aarch64-apple-darwin', started:built.toISOString(), build:{linkedPlan:linked, linkedInputs:linkedInputs(linked)}}));
    utimesSync(pkg, old, old);
    const changed = () => receiptChanges(receipt, fixture).map(path => resolve(import.meta.dir, '..', path));
    expect(changed()).toEqual([]);
    // The next build's bake reads a production file moved in and a design made, both at old mtimes; that build then
    // fails before replacing this receipt's bundle.
    const moved = resolve(dir, 'older.contract'); writeFileSync(moved, 'production'); utimesSync(moved, old, old);
    renameSync(moved, resolve(pkg, 'added.contract'));
    writeFileSync(design, 'design'); utimesSync(design, old, old);
    bake(); utimesSync(pkg, old, old);
    expect(changed()).toEqual(expect.arrayContaining([pkg, design]));
    // A receipt without the copy cannot say what its bake read.
    writeFileSync(receipt, JSON.stringify({target:'aarch64-apple-darwin', started:built.toISOString(), build:{linkedPlan:linked}}));
    expect(changed()).toEqual([resolve(linked, 'inputs.json')]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32' || process.getuid?.() === 0)('package inventories tolerate unreadable production directories and track readability', async () => {
  const {watchedNewer} = await import('./agent-launch.mjs');
  const {packageDirectories} = await import('./cargo-dep-info.mjs');
  const dir = mkdtempSync(resolve(import.meta.dir, '../target/package-unreadable-'));
  const blocked = resolve(dir, 'private');
  try {
    writeFileSync(resolve(dir, 'package.json'), '{}'); mkdirSync(blocked);
    writeFileSync(resolve(blocked, 'payload'), 'unused'); chmodSync(blocked, 0);
    expect(() => readdirSync(blocked)).toThrow();
    const snapshot = packageDirectories([dir]);
    const changed = () => watchedNewer(Date.now() + 60_000, [dir], snapshot);
    expect(changed()).toEqual([]);
    chmodSync(blocked, 0o700);
    expect(changed().map(path => resolve(import.meta.dir, '..', path))).toContain(blocked);
  } finally { chmodSync(blocked, 0o700); rmSync(dir, {recursive:true, force:true}); }
});

test('bare dep-info keeps an absent optional directory fresh', async () => {
  const {depInfoNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(import.meta.dir, '../target/optional-directory-'));
  try {
    const info = resolve(dir, 'archive.d');
    writeFileSync(info, `archive: ${resolve(dir, 'assets')}\n`);
    expect(depInfoNewer(Date.now(), null, info, dir)).toEqual([]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

// A new app's first Apple build writes the copy after its start; the driver refused that build as stale.
test("the compiled copy of a linked bake, written during the build, is not newer than the build; the sources are", async () => {
  const {depInfoNewer} = await import('./agent-launch.mjs');
  const {COMPILED_BAKE, compiledBakeDir} = await import('./linked-plan.mjs');
  const scratch = resolve(import.meta.dir, '../target');
  mkdirSync(scratch, {recursive:true});
  const dir = mkdtempSync(resolve(scratch, 'compiled-bake-'));
  try {
    const baked = resolve(dir, 'ios.baked'), source = resolve(dir, 'app.contract');
    mkdirSync(baked);
    for (const name of COMPILED_BAKE) writeFileSync(resolve(baked, name), `// ${name}\n`);
    writeFileSync(source, 'component App\n');
    const started = Date.now() - 60_000;
    utimesSync(source, new Date(started - 60_000), new Date(started - 60_000));
    const copy = compiledBakeDir(baked, resolve(dir, 'target/bake-compiled'));
    const info = resolve(dir, 'archive.d');
    writeFileSync(info, `archive: ${[source, ...COMPILED_BAKE.map(name => resolve(copy, name))].join(' ')}\n`);
    expect(statSync(resolve(copy, COMPILED_BAKE[0])).mtimeMs).toBeGreaterThan(started);
    expect(depInfoNewer(started, null, info, dir)).toEqual([]);
    utimesSync(source, new Date(), new Date());
    expect(depInfoNewer(started, null, info, dir).map(path => resolve(import.meta.dir, '..', path))).toEqual([source]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('optional package inputs keep the bake revision through test edits and change on creation or deletion', async () => {
  const {cargoBakeInputs} = await import('./cargo-dep-info.mjs');
  const scratch = resolve(import.meta.dir, '../target');
  mkdirSync(scratch, {recursive:true});
  const dir = mkdtempSync(resolve(scratch, 'optional-package-'));
  try {
    const out = resolve(dir, 'out'), pkg = resolve(dir, 'packages/photos'), design = resolve(pkg, 'photos.tamber');
    mkdirSync(out); mkdirSync(resolve(pkg, 'test'), {recursive:true});
    const source = resolve(pkg, 'index.contract'), manifest = resolve(pkg, 'package.json'), drive = resolve(pkg, 'test/drives.mjs');
    for (const path of [source, manifest, drive]) writeFileSync(path, 'before');
    writeFileSync(resolve(out, '../output'), `cargo:rerun-if-changed=${source}\ncargo:rerun-if-changed=${manifest}\ncargo:metadata=exact-optional-input=${design}\n`);
    const unit = {id:'fixture', manifest_path:resolve(dir, 'Cargo.toml'), targets:[{kind:['custom-build']}]};
    const env = {CARGO_TARGET_DIR:resolve(dir, 'target')};
    const prepare = () => cargoBakeInputs(unit, 'target', 'dev', env);
    const messages = [{reason:'build-script-executed', package_id:unit.id, out_dir:out}];
    prepare()(messages);
    const first = env.EXACT_BAKE_INPUTS;
    writeFileSync(drive, 'edited'); writeFileSync(resolve(pkg, 'test/new.mjs'), 'new');
    prepare(); expect(env.EXACT_BAKE_INPUTS).toBe(first);
    rmSync(drive); prepare(); expect(env.EXACT_BAKE_INPUTS).toBe(first);
    writeFileSync(design, 'design'); utimesSync(design, new Date(0), new Date(0));
    prepare()(messages); expect(env.EXACT_BAKE_INPUTS).not.toBe(first);
    const created = env.EXACT_BAKE_INPUTS;
    prepare(); expect(env.EXACT_BAKE_INPUTS).toBe(created);
    rmSync(source); prepare(); expect(env.EXACT_BAKE_INPUTS).not.toBe(created);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32')('a linked bake reuses an absent package design and rebakes when it appears at an old mtime', async () => {
  const {bakeOutside, recordRustBake} = await import('./linked-plan.mjs');
  const scratch = resolve(import.meta.dir, '../target');
  mkdirSync(scratch, {recursive:true});
  const dir = mkdtempSync(resolve(scratch, 'linked-package-'));
  try {
    const baked = resolve(dir, 'bakes/macos-target.baked'), executable = resolve(dir, 'baker');
    const design = resolve(dir, 'photos.tamber');
    mkdirSync(baked, {recursive:true}); mkdirSync(resolve(dir, 'data'));
    writeFileSync(resolve(dir, 'data/Cargo.toml'), '');
    writeFileSync(resolve(baked, 'app.plan'), 'plan'); writeFileSync(resolve(baked, 'compat.json'), '{}');
    // The script only reports its reads; no Apple compiler or app is involved.
    writeFileSync(executable, `#!/bin/sh\necho baked >> "$OUT_DIR/runs"\necho 'cargo:metadata=exact-optional-input=${design}'\n`, {mode:0o755});
    const started = Date.now() + 60_000;
    writeFileSync(resolve(baked, 'rust-baker.json'), JSON.stringify({executable, cwd:dir, env:{}, started}));
    writeFileSync(resolve(baked, 'rust-output.txt'), `cargo:metadata=exact-optional-input=${design}\n`);
    const env = {EXACT_BAKED:baked, EXACT_BAKE_OUTPUT:resolve(dir, 'bakes')};
    recordRustBake(env);
    const bake = () => bakeOutside({dir, name:'fixture'}, 'macos', 'target', env);
    bake(); expect(existsSync(resolve(baked, 'runs'))).toBe(false);
    writeFileSync(design, 'design'); utimesSync(design, new Date(0), new Date(0));
    bake(); expect(readFileSync(resolve(baked, 'runs'), 'utf8')).toBe('baked\n');
    bake(); expect(readFileSync(resolve(baked, 'runs'), 'utf8')).toBe('baked\n');
    rmSync(design);
    bake(); expect(readFileSync(resolve(baked, 'runs'), 'utf8')).toBe('baked\nbaked\n');
    bake(); expect(readFileSync(resolve(baked, 'runs'), 'utf8')).toBe('baked\nbaked\n');
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32').each(['file', 'directory', 'file link', 'directory link', 'inner directory link'])('a linked bake compares restored %s contents and reuses unchanged inputs', async kind => {
  const {bakeOutside, recordRustBake} = await import('./linked-plan.mjs');
  const scratch = resolve(import.meta.dir, '../target');
  mkdirSync(scratch, {recursive:true});
  const dir = mkdtempSync(resolve(scratch, 'linked-content-'));
  try {
    const baked = resolve(dir, 'bakes/macos-target.baked'), executable = resolve(dir, 'baker');
    const directory = resolve(dir, 'sources'), source = resolve(directory, 'input');
    mkdirSync(baked, {recursive:true}); mkdirSync(directory); mkdirSync(resolve(dir, 'data'));
    writeFileSync(resolve(dir, 'data/Cargo.toml'), '');
    writeFileSync(source, 'ORIGINAL');
    const old = new Date(0);
    utimesSync(source, old, old);
    let watched = kind === 'file' ? source : directory;
    if (kind.endsWith('link')) {
      watched = resolve(dir, 'watched');
      if (kind === 'inner directory link') {
        mkdirSync(watched);
        symlinkSync(directory, resolve(watched, 'child'), 'dir');
        symlinkSync(watched, resolve(directory, 'back'), 'dir');
      } else symlinkSync(kind === 'file link' ? source : directory, watched);
    }
    writeFileSync(resolve(baked, 'app.plan'), 'ORIGINAL'); writeFileSync(resolve(baked, 'compat.json'), '{}');
    const stdout = `cargo:rerun-if-changed=${watched}\n`;
    writeFileSync(executable, `#!/bin/sh\nset -e\n/bin/cat '${source}' > "$OUT_DIR/app.plan"\necho baked >> "$OUT_DIR/runs"\necho '${stdout.trim()}'\n`, {mode:0o755});
    writeFileSync(resolve(baked, 'rust-baker.json'), JSON.stringify({executable, cwd:dir, env:{}, started:Date.now() + 60_000}));
    writeFileSync(resolve(baked, 'rust-output.txt'), stdout);
    const env = {EXACT_BAKED:baked, EXACT_BAKE_OUTPUT:resolve(dir, 'bakes'),
      CARGO_TARGET_DIR:resolve(dir, 'target'), CARGO_BUILD_BUILD_DIR:resolve(dir, 'build')};
    const outputs = [baked, env.CARGO_TARGET_DIR, env.CARGO_BUILD_BUILD_DIR];
    if (kind.includes('directory')) for (const [index, output] of outputs.entries()) {
      mkdirSync(output, {recursive:true});
      symlinkSync(output, resolve(directory, `output-${index}`), 'dir');
    }
    recordRustBake(env);
    const bake = () => bakeOutside({dir, name:'fixture'}, 'macos', 'target', env);
    const plan = () => readFileSync(resolve(baked, 'app.plan'), 'utf8');
    const runs = () => existsSync(resolve(baked, 'runs')) ? readFileSync(resolve(baked, 'runs'), 'utf8') : '';
    bake(); expect(runs()).toBe('');
    writeFileSync(source, 'RESTORED'); utimesSync(source, old, old);
    bake(); expect(plan()).toBe('RESTORED');
    expect(statSync(source).mtimeMs).toBe(old.getTime());
    expect(runs()).toBe('baked\n');
    bake(); expect(runs()).toBe('baked\n');
    utimesSync(source, new Date(), new Date());
    bake(); expect(runs()).toBe('baked\n');
    if (kind.includes('directory')) {
      for (const output of outputs) writeFileSync(resolve(output, 'product'), 'changed output');
      bake(); expect(runs()).toBe('baked\n');
    }
    if (kind === 'directory') {
      const added = resolve(directory, 'added');
      writeFileSync(added, 'new'); utimesSync(added, old, old); utimesSync(directory, old, old);
      bake(); expect(runs()).toBe('baked\nbaked\n');
      rmSync(added); utimesSync(directory, old, old);
      bake(); expect(runs()).toBe('baked\nbaked\nbaked\n');
      bake(); expect(runs()).toBe('baked\nbaked\nbaked\n');
    }
    const record = resolve(baked, 'inputs.json'), previous = JSON.parse(readFileSync(record, 'utf8'));
    delete previous.digest;
    writeFileSync(record, JSON.stringify({...previous, version:1}));
    const before = runs();
    bake(); expect(runs()).toBe(before + 'baked\n');
    bake(); expect(runs()).toBe(before + 'baked\n');
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32').each(['target', 'build', 'bakes', 'alias'])(
  'a linked bake fingerprints an explicitly watched producer in %s', async kind => {
    const {bakeOutside, recordRustBake} = await import('./linked-plan.mjs');
    const dir = mkdtempSync(resolve(import.meta.dir, '../target/linked-producer-'));
    try {
      const baked = resolve(dir, 'bakes/macos-target.baked'), executable = resolve(dir, 'baker');
      const env = {EXACT_BAKED:baked, EXACT_BAKE_OUTPUT:resolve(dir, 'bakes'),
        CARGO_TARGET_DIR:resolve(dir, 'target'), CARGO_BUILD_BUILD_DIR:resolve(dir, 'build')};
      const product = resolve(dir, kind === 'alias' ? 'target' : kind, 'gpu.dylib');
      mkdirSync(dirname(product), {recursive:true}); mkdirSync(baked, {recursive:true}); mkdirSync(resolve(dir, 'data'));
      writeFileSync(resolve(dir, 'data/Cargo.toml'), ''); writeFileSync(product, 'FIRST');
      const watched = kind === 'alias' ? resolve(dir, 'gpu-link') : product;
      if (kind === 'alias') symlinkSync(product, watched);
      const stdout = `cargo:rerun-if-changed=${watched}\n`;
      writeFileSync(executable, `#!/bin/sh\nset -e\n/bin/cat '${product}' > "$OUT_DIR/app.plan"\necho baked >> "$OUT_DIR/runs"\necho '${stdout.trim()}'\n`, {mode:0o755});
      writeFileSync(resolve(baked, 'app.plan'), 'FIRST'); writeFileSync(resolve(baked, 'compat.json'), '{}');
      writeFileSync(resolve(baked, 'rust-baker.json'), JSON.stringify({executable, cwd:dir, env:{}, started:Date.now()+60_000}));
      writeFileSync(resolve(baked, 'rust-output.txt'), stdout);
      recordRustBake(env);
      const bake = () => bakeOutside({dir, name:'fixture'}, 'macos', 'target', env);
      bake(); expect(existsSync(resolve(baked, 'runs'))).toBe(false);
      writeFileSync(product, 'SECOND'); utimesSync(product, new Date(0), new Date(0));
      bake(); expect(readFileSync(resolve(baked, 'app.plan'), 'utf8')).toBe('SECOND');
      bake(); expect(readFileSync(resolve(baked, 'runs'), 'utf8')).toBe('baked\n');
    } finally { rmSync(dir, {recursive:true, force:true}); }
  });

test.skipIf(process.platform === 'win32').each(['known', 'discovered', 'bootstrap']
  .flatMap(stage => ['file', 'directory', 'link', 'removed'].map(kind => [stage, kind])))(
  'a linked bake does not cache an input edited after its read (%s %s watch)', async (stage, kind) => {
    const {bakeOutside, recordRustBake} = await import('./linked-plan.mjs');
    const dir = mkdtempSync(resolve(import.meta.dir, '../target/linked-race-'));
    try {
      const baked = resolve(dir, 'bakes/macos-target.baked'), executable = resolve(dir, 'baker');
      const sources = resolve(dir, 'sources'), source = resolve(sources, 'input'), trigger = resolve(baked, 'change');
      mkdirSync(baked, {recursive:true}); mkdirSync(resolve(dir, 'data')); mkdirSync(sources);
      writeFileSync(resolve(dir, 'data/Cargo.toml'), ''); writeFileSync(source, 'FIRST');
      writeFileSync(resolve(baked, 'app.plan'), 'FIRST'); writeFileSync(resolve(baked, 'compat.json'), '{}');
      const watched = kind === 'directory' ? sources : kind === 'link' ? resolve(dir, 'watched') : source;
      if (kind === 'link') symlinkSync(source, watched);
      const stdout = `cargo:rerun-if-changed=${watched}\n`;
      const change = kind === 'removed' ? `/bin/rm '${source}'` : `printf RESTORED > '${source}'\n  /usr/bin/touch -t 200001010000 '${source}'`;
      writeFileSync(executable, `#!/bin/sh\nset -e\nif [ -f '${source}' ]; then /bin/cat '${source}'; else printf ABSENT; fi > "$OUT_DIR/app.plan"\necho baked >> "$OUT_DIR/runs"\nif [ -f '${trigger}' ]; then\n  ${change}\n  /bin/rm '${trigger}'\nfi\necho '${stdout.trim()}'\n`, {mode:0o755});
      const registry = resolve(baked, 'rust-baker.json'), output = resolve(baked, 'rust-output.txt');
      writeFileSync(registry, JSON.stringify({executable, cwd:dir, env:{}, started:Date.now()+60_000}));
      writeFileSync(output, stage === 'discovered' ? '' : stdout);
      const env = {EXACT_BAKED:baked, EXACT_BAKE_OUTPUT:resolve(dir, 'bakes')};
      recordRustBake(env);
      const bake = () => bakeOutside({dir, name:'fixture'}, 'macos', 'target', env);
      if (stage === 'bootstrap') {
        // The bootstrap read FIRST, then an editor restored older bytes before Cargo returned.
        writeFileSync(registry, JSON.stringify({executable, cwd:dir, env:{}, started:statSync(source).ctimeMs}));
        if (kind === 'removed') rmSync(source);
        else { writeFileSync(source, 'RESTORED'); utimesSync(source, new Date(0), new Date(0)); }
        rmSync(resolve(baked, 'inputs.json'));
        recordRustBake(env);
      } else {
        writeFileSync(source, 'SECOND'); writeFileSync(trigger, '');
        if (stage === 'discovered') rmSync(resolve(baked, 'inputs.json'));
        bake(); expect(readFileSync(resolve(baked, 'app.plan'), 'utf8')).toBe('SECOND');
      }
      bake(); expect(readFileSync(resolve(baked, 'app.plan'), 'utf8')).toBe(kind === 'removed' ? 'ABSENT' : 'RESTORED');
      const record = readFileSync(resolve(baked, 'inputs.json'), 'utf8');
      bake(); expect(readFileSync(resolve(baked, 'inputs.json'), 'utf8')).toBe(record);
    } finally { rmSync(dir, {recursive:true, force:true}); }
  });

test.skipIf(process.platform === 'win32')('external directory watches record source membership and links, excluding build outputs', async () => {
  const {pendingBuildInputs} = await import('./app.mjs');
  const {depInfoChanges, receiptChanges, watchedNewer} = await import('./agent-launch.mjs');
  const previousOutput = process.env.EXACT_BAKE_OUTPUT;
  const dir = pinned(realpathSync(mkdtempSync(resolve(tmpdir(), 'r14-outside-watch-'))));
  const write = (name, text) => { mkdirSync(dirname(resolve(dir,name)), {recursive:true}); writeFileSync(resolve(dir,name),text); };
  try {
    const target = bakeTarget('linux'), id = 'com.exact.plain', app = resolve(dir,'app');
    // `watcher` is another repository's crate: its `../../.env` is under neither the app, its workspace nor exact2.
    write('vendor/watcher/Cargo.toml', '[package]\nname="watcher"\nversion="0.1.0"\nedition="2021"\n');
    write('vendor/watcher/src/lib.rs', 'pub const VALUE: u32 = 7;\n');
    write('vendor/watcher/package.json', '{}');

    write('app/Cargo.toml', '[workspace]\nmembers=["linux"]\nresolver="2"\n');
    write('app/linux/Cargo.toml', '[package]\nname="plain-linux"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nwatcher={path="../../vendor/watcher"}\n');
    write('app/linux/src/main.rs', `fn main() {
      let _ = watcher::VALUE;
      println!("{{\\\"ready\\\":true}}");
      let mut line = String::new();
      std::io::stdin().read_line(&mut line).unwrap();
    }`);
    write('app/app.contract', 'component App\n  view\n    text "plain"\n');
    write('app/linux/build.rs', `fn main() {
      let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
      std::fs::write(out.join("compat.json"), r#"${JSON.stringify({target,inputs:{platform:'linux',app:id,store:{L:'0'},keys:[]}})}"#).unwrap();
      std::fs::write(out.join("artifacts.json"), r#"{"version":1,"artifacts":[],"sources":{}}"#).unwrap();
      std::fs::write(out.join("app.plan"), b"fixture").unwrap();
    }`);
    const fixture = {dir:app, workspace:app, target:resolve(app,'target'), name:'plain', id, manifest:{app:{id,name:'Plain'},rust:false}};
    write('app/bake.mjs', `import {buildBake} from ${JSON.stringify(resolve(import.meta.dir,'app.mjs'))};
      const receipt = buildBake({...${JSON.stringify(fixture)},crate:kind=>'plain-'+kind},'linux',${JSON.stringify(target)},{profile:'dev',output:${JSON.stringify(resolve(app,'bakes'))},env:{CARGO_BUILD_BUILD_DIR:${JSON.stringify(resolve(dir,'compiler-work'))}}});
      console.log(JSON.stringify(receipt));`);
    const installed = resolve(app, 'node_modules/pkg');
    const versions = ['1', '2'].map(version => resolve(app, `node_modules/.bun/pkg@${version}/node_modules/pkg`));
    for (const version of versions) {
      mkdirSync(version, {recursive:true});
      writeFileSync(resolve(version, 'package.json'), '{}'); writeFileSync(resolve(version, 'input'), 'same source');
      utimesSync(resolve(version, 'input'), new Date(0), new Date(0));
    }
    symlinkSync(versions[0], installed, 'dir');
    const bake = (watched) => {
      write('vendor/watcher/build.rs', `fn main() { println!("cargo:rerun-if-changed=${watched}"); println!("cargo:rerun-if-changed=${installed}"); }\n`);
      return spawnSync(process.execPath,[resolve(app,'bake.mjs')],{cwd:app,encoding:'utf8',env:{...process.env,EXACT_UPDATE_TRUST:'development'}});
    };
    const receipt = (built) => { assert.equal(built.status,0,built.stderr); return JSON.parse(built.stdout.trim().split('\n').at(-1)); };
    // The name has `/` on every host: a receipt's names are the same on every machine.
    const file = receipt(bake('../../.env')).binary;
    assert.ok(file.absent.includes('crate:watcher@0.1.0/../../.env'),JSON.stringify(file.absent));
    assert.ok(file.missing.includes(resolve(dir,'.env')));
    // A directory inside its own package is recorded as before.
    const inside = receipt(bake('src')).binary;
    assert.ok(inside.directories.some(d => d.path === resolve(dir,'vendor/watcher/src')), JSON.stringify(inside.directories.map(d => d.path)));
    const old = new Date(0), since = Date.now() + 60_000;
    for (const name of ['shared/input', 'other/input', '.config/source', 'vendor/watcher/dist/input', 'artifacts/input']) {
      write(name, 'same source'); utimesSync(resolve(dir, name), old, old);
    }
    symlinkSync('shared', resolve(dir, 'source-link'), 'dir');
    symlinkSync('shared/input', resolve(dir, 'file-link'));
    symlinkSync('absent-a', resolve(dir, 'dangling'));
    symlinkSync('..', resolve(dir, 'shared/back'), 'dir');
    symlinkSync('loop-b', resolve(dir, 'loop-a')); symlinkSync('loop-a', resolve(dir, 'loop-b'));
    const outputs = [fixture.target, resolve(app, 'bakes'), resolve(dir, 'compiler-work')];
    process.env.EXACT_BAKE_OUTPUT = resolve(app, 'bakes');
    outputs.forEach((path, i) => symlinkSync(path, resolve(dir, `output-${i}`), 'dir'));
    symlinkSync(resolve(fixture.target, 'future'), resolve(dir, 'future-output'), 'dir');
    const driveReceipt = resolve(app, 'bakes/drive.json');
    const driveApp = {...fixture, crate:()=>'plain-linux'};
    write(`app/target/${target}/debug/libplain_linux.d`, `archive: ${dir}\n`);
    const checks = build => [
      pendingBuildInputs(build),
      watchedNewer(since, [dir, installed], build.watchedDirectories, new Map(), build.binary.watchOutputs),
      receiptChanges(driveReceipt, driveApp),
      depInfoChanges(build.products.find(product => product.path.endsWith('/plain-linux')).path, driveApp),
    ];
    const capture = () => {
      const build = receipt(bake('../..'));
      writeFileSync(driveReceipt, JSON.stringify({target, started:new Date(since).toISOString(), build}));
      utimesSync(driveReceipt, new Date(since), new Date(since));
      for (const changed of checks(build)) assert.deepEqual(changed, []);
      return build;
    };
    const first = capture();
    // Select the fixture's Cargo target/profile; the real carrier and freshness
    // checks open the compiled executable and read its completed bake receipt.
    write('app/bakes/drive.mjs', `import {mock} from 'bun:test';
      import assert from 'node:assert/strict';
      const appPath = ${JSON.stringify(resolve(import.meta.dir, 'app.mjs'))};
      const real = await import(appPath);
      mock.module(appPath, () => ({...real, resolveApp:()=>({...${JSON.stringify(fixture)}, crate:kind=>'plain-'+kind}), linuxBinary:()=>process.argv[2]}));
      const {openStdio} = await import(${JSON.stringify(resolve(import.meta.dir, 'agent-stdio.mjs'))});
      if (process.argv[3] === 'stale') {
        await assert.rejects(openStdio({host:'linux', app:'plain', onProcess:()=>assert.fail('stale build spawned')}), /linux build is stale/);
      } else {
        const session = await openStdio({host:'linux', app:'plain'}); await session.close();
      }`);
    const drive = (build, state) => {
      const bin = build.products.find(product => product.path.endsWith('/plain-linux')).path;
      const result = spawnSync(process.execPath, [resolve(app, 'bakes/drive.mjs'), bin, state], {cwd:app, encoding:'utf8'});
      assert.equal(result.status, 0, result.stderr);
    };
    assert.ok(first.binary.directories.some(d => d.path === dir));
    assert.ok(first.binary.directories.some(d => d.path === installed));
    assert.ok(first.binary.links.some(link => link.path === installed && link.target === versions[0]));
    assert.ok(first.binary.inputs.some(input => input.path === resolve(installed, 'input')));
    for (const name of ['.config/source', 'vendor/watcher/dist/input', 'artifacts/input'])
      assert.ok(first.binary.inputs.some(input => input.path === resolve(dir, name)), `${name} is a source`);
    assert.ok(first.binary.links.some(link => link.path === resolve(dir, 'shared/back') && link.target === '..'));
    for (const path of outputs) writeFileSync(resolve(path, 'new-output'), 'generated');
    for (const name of ['.git/index', 'app/.exact/runs.jsonl', 'app/node_modules/cache', 'app/dist/app.js', 'app/app.contract.d.ts', 'app/target/future/nested']) write(name, 'tool output');
    for (const changed of checks(first)) assert.deepEqual(changed, []);
    drive(first, 'fresh');
    assert.equal(capture().binary.sha256, first.binary.sha256, 'output writes cannot change the binary fingerprint');
    for (const [name, change] of [
      ['source addition', () => { write('shared/added', 'new'); utimesSync(resolve(dir, 'shared/added'), old, old); }],
      ['source removal', () => rmSync(resolve(dir, 'shared/added'))],
      ['empty directory addition', () => mkdirSync(resolve(dir, 'shared/empty'))],
      ['empty directory removal', () => rmSync(resolve(dir, 'shared/empty'), {recursive:true})],
      ['directory link target', () => { rmSync(resolve(dir, 'source-link')); symlinkSync('other', resolve(dir, 'source-link'), 'dir'); }],
      ['directory link to output', () => { rmSync(resolve(dir, 'source-link')); symlinkSync(fixture.target, resolve(dir, 'source-link'), 'dir'); }],
      ['file link target', () => { rmSync(resolve(dir, 'file-link')); symlinkSync('other/input', resolve(dir, 'file-link')); }],
      ['dangling link target', () => { rmSync(resolve(dir, 'dangling')); symlinkSync('absent-b', resolve(dir, 'dangling')); }],
      ['package distribution source', () => { write('vendor/watcher/dist/input', 'compiled source'); utimesSync(resolve(dir, 'vendor/watcher/dist/input'), old, old); }],
      ['source edit at an old mtime', () => { write('other/input', 'new source'); utimesSync(resolve(dir, 'other/input'), old, old); }],
      ['installed source addition', () => { writeFileSync(resolve(installed, 'added'), 'source'); utimesSync(resolve(installed, 'added'), old, old); }],
      ['installed source removal', () => rmSync(resolve(installed, 'added'))],
      ['installed package link target', () => { rmSync(installed); symlinkSync(versions[1], installed, 'dir'); }],
    ]) {
      const build = capture(); change();
      for (const changed of checks(build)) assert.ok(changed.length, `${name} must invalidate the receipt and drive`);
      drive(build, 'stale');
      assert.notEqual(capture().binary.sha256, build.binary.sha256, `${name} must change the next fingerprint`);
    }
  } finally {
    if (previousOutput === undefined) delete process.env.EXACT_BAKE_OUTPUT; else process.env.EXACT_BAKE_OUTPUT = previousOutput;
    rmSync(dir,{recursive:true,force:true});
  }
}, 180000);

test.skipIf(process.platform === 'win32').each(['child', 'root'])('a watched %s link retargeted from source to output invalidates freshness', async kind => {
  const {packageDirectories} = await import('./cargo-dep-info.mjs');
  const {watchedNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-link-output-'));
  try {
    mkdirSync(resolve(dir, 'src')); mkdirSync(resolve(dir, 'target'));
    const link = resolve(dir, 'link'); symlinkSync('src', link, 'dir');
    const paths = [kind === 'root' ? link : dir], directories = packageDirectories(paths);
    expect(watchedNewer(Infinity, paths, directories)).toEqual([]);
    rmSync(link); symlinkSync('target', link, 'dir');
    expect(watchedNewer(Infinity, paths, directories).length).toBeGreaterThan(0);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32')('an explicit Bun package watch records sources and link identity', async () => {
  const {buildInputCollector, packageDirectories, watchedDigest} = await import('./cargo-dep-info.mjs');
  const {pendingBuildInputs} = await import('./app.mjs');
  const {watchedNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-bun-package-'));
  try {
    const link = resolve(dir, 'node_modules/pkg');
    const versions = ['1', '2'].map(version => resolve(dir, `node_modules/.bun/pkg@${version}/node_modules/pkg`));
    for (const version of versions) {
      mkdirSync(resolve(version, 'src'), {recursive:true});
      writeFileSync(resolve(version, 'package.json'), '{}');
      writeFileSync(resolve(version, 'src/index.ts'), 'same source');
      utimesSync(resolve(version, 'src/index.ts'), new Date(0), new Date(0));
      mkdirSync(resolve(version, '.exact'));
    }
    symlinkSync(versions[0], link, 'dir');
    symlinkSync(versions[0], resolve(dir, 'incidental'), 'dir');
    const collector = buildInputCollector({outputs:[], nameOf:path=>path, replaced:new Set(),
      hashes:{of:path=>createHash('sha256').update(readFileSync(path)).digest('hex')}});
    collector.add(dir);
    collector.add(link);
    const build = {binary:{inputs:[...collector.inputs.values()], directories:[...collector.directories.values()],
      links:[...collector.links.values()], missing:[]}};
    const paths = [dir, link], directories = packageDirectories(paths);
    const digest = watchedDigest([link], []);
    expect(build.binary.inputs.map(input => input.path)).toContain(resolve(link, 'src/index.ts'));
    expect(build.binary.links).toContainEqual({name:link, path:link, target:versions[0]});
    expect(packageDirectories([link], [versions[0]])).toEqual([]);
    const fresh = () => {
      expect(pendingBuildInputs(build)).toEqual([]);
      expect(watchedNewer(Infinity, paths, directories)).toEqual([]);
      expect(watchedDigest([link], [])).toBe(digest);
    };
    fresh();
    writeFileSync(resolve(link, '.exact/run.log'), 'output'); fresh();
    writeFileSync(resolve(link, 'src/added.ts'), 'source');
    expect(pendingBuildInputs(build).length).toBeGreaterThan(0);
    expect(watchedNewer(Infinity, [link], directories).length).toBeGreaterThan(0);
    expect(watchedDigest([link], [])).not.toBe(digest);
    rmSync(resolve(link, 'src/added.ts')); fresh();
    rmSync(link); symlinkSync(versions[1], link, 'dir');
    expect(pendingBuildInputs(build)).toContain(link);
    expect(watchedNewer(Infinity, [link], directories)).toContain(link);
    expect(watchedDigest([link], [])).not.toBe(digest);
    // A parent directory watch still omits incidental package-manager stores.
    expect(packageDirectories([dir]).map(snapshot => snapshot.path)).not.toContain(link);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32').each(['bootstrap', 'rebake'])('a %s linked inventory excludes bake and Cargo outputs inside a watch', async stage => {
  const {bakeOutside, linkedInputs, recordRustBake} = await import('./linked-plan.mjs');
  const {watchedNewer, receiptChanges} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-linked-outputs-'));
  try {
    const pkg = resolve(dir, 'pkg'), app = resolve(dir, 'app'), since = Date.now() + 60_000;
    const baked = resolve(pkg, 'bakes/macos-target.baked'), executable = resolve(app, 'baker');
    mkdirSync(baked, {recursive:true}); mkdirSync(resolve(app, 'data'), {recursive:true});
    writeFileSync(resolve(app, 'data/Cargo.toml'), '');
    writeFileSync(resolve(pkg, 'package.json'), '{}'); writeFileSync(resolve(pkg, 'source'), 'source');
    writeFileSync(resolve(app, 'app.json'), '{}');
    const env = {EXACT_BAKED:baked, EXACT_BAKE_OUTPUT:resolve(pkg, 'bakes'),
      CARGO_TARGET_DIR:resolve(pkg, 'cargo-products'), CARGO_BUILD_BUILD_DIR:resolve(pkg, 'compiler-work')};
    const outputs = [env.EXACT_BAKE_OUTPUT, env.CARGO_TARGET_DIR, env.CARGO_BUILD_BUILD_DIR];
    for (const output of outputs) mkdirSync(output, {recursive:true});
    mkdirSync(resolve(pkg, 'shared')); writeFileSync(resolve(pkg, 'shared/input'), 'source');
    symlinkSync('shared', resolve(pkg, 'source-link'), 'dir');
    symlinkSync(env.EXACT_BAKE_OUTPUT, resolve(pkg, 'output-link'), 'dir');
    const stdout = `cargo:rerun-if-changed=${pkg}\n`;
    writeFileSync(executable, `#!/bin/sh\nset -e\nprintf baked > "$OUT_DIR/app.plan"\nprintf '{}' > "$OUT_DIR/compat.json"\necho '${stdout.trim()}'\n`, {mode:0o755});
    writeFileSync(resolve(baked, 'app.plan'), 'initial'); writeFileSync(resolve(baked, 'compat.json'), '{}');
    writeFileSync(resolve(baked, 'rust-baker.json'), JSON.stringify({executable, cwd:app, env:{}, started:since}));
    writeFileSync(resolve(baked, 'rust-output.txt'), stdout);
    if (stage === 'bootstrap') recordRustBake(env);
    else bakeOutside({dir:app, name:'fixture'}, 'macos', 'target', env);
    const record = linkedInputs(baked), fixture = {dir:app, target:env.CARGO_TARGET_DIR, workspace:dir, manifest:{}, crate:()=>'native-apple'};
    const info = resolve(fixture.target, 'aarch64-apple-darwin/debug/libnative_apple.d');
    mkdirSync(dirname(info), {recursive:true}); writeFileSync(info, `archive: ${resolve(pkg, 'source')}\n`);
    const receipt = resolve(app, 'receipt.json');
    writeFileSync(receipt, JSON.stringify({target:'aarch64-apple-darwin', started:new Date(since).toISOString(),
      build:{linkedPlan:baked, linkedInputs:record, binary:{inputs:[], missing:[], directories:[], watchOutputs:outputs}}}));
    const fresh = () => {
      expect(watchedNewer(since, record.paths, record.watchedDirectories, new Map(), outputs)).toEqual([]);
      expect(receiptChanges(receipt, fixture)).toEqual([]);
    };
    fresh();
    for (const output of outputs) writeFileSync(resolve(output, 'generated'), 'output');
    fresh();
    rmSync(resolve(pkg, 'source-link')); symlinkSync(env.EXACT_BAKE_OUTPUT, resolve(pkg, 'source-link'), 'dir');
    expect(watchedNewer(since, record.paths, record.watchedDirectories, new Map(), outputs)).toContain(pkg);
    expect(receiptChanges(receipt, fixture)).toContain(pkg);
    rmSync(resolve(pkg, 'source-link')); symlinkSync('shared', resolve(pkg, 'source-link'), 'dir'); fresh();
    mkdirSync(resolve(pkg, 'other')); rmSync(resolve(pkg, 'source'));
    expect(watchedNewer(since, record.paths, record.watchedDirectories, new Map(), outputs)).toContain(pkg);
    expect(receiptChanges(receipt, fixture)).toContain(pkg);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('replacing a watched root directory with an old file invalidates freshness', async () => {
  const {packageDirectories} = await import('./cargo-dep-info.mjs');
  const {watchedNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-root-type-'));
  try {
    const root = resolve(dir, 'source'); mkdirSync(root);
    const directories = packageDirectories([root]);
    expect(watchedNewer(Infinity, [root], directories)).toEqual([]);
    rmSync(root, {recursive:true}); writeFileSync(root, 'replacement'); utimesSync(root, new Date(0), new Date(0));
    expect(watchedNewer(Infinity, [root], directories)).toContain(root);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('replacing a watched file with an old directory invalidates freshness', async () => {
  const {packageDirectories} = await import('./cargo-dep-info.mjs');
  const {watchedNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-file-type-'));
  try {
    const root = resolve(dir, 'source'); writeFileSync(root, 'source'); utimesSync(root, new Date(0), new Date(0));
    const directories = packageDirectories([root]), since = Date.now() + 60_000;
    expect(watchedNewer(since, [root], directories)).toEqual([]);
    rmSync(root); mkdirSync(root); utimesSync(root, new Date(0), new Date(0));
    expect(watchedNewer(since, [root], directories)).toContain(root);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32').each(['.exact', 'node_modules', '.git', '.build', '.gradle', 'target', 'dist'])(
  'a watch omits aliases into %s descendants but keeps explicitly named files', async tool => {
    const {buildInputCollector, packageDirectories, watchOutput} = await import('./cargo-dep-info.mjs');
    const {pendingBuildInputs} = await import('./app.mjs');
    const {watchedNewer} = await import('./agent-launch.mjs');
    const dir = mkdtempSync(resolve(tmpdir(), 'watch-tool-alias-'));
    try {
      const app = resolve(dir, 'app'), alias = resolve(dir, 'alias'), output = resolve(app, tool, 'state');
      mkdirSync(output, {recursive:true}); writeFileSync(resolve(app, 'app.json'), '{}');
      symlinkSync(output, alias, 'dir'); writeFileSync(resolve(output, 'log'), 'first');
      const collector = buildInputCollector({outputs:[], nameOf:path=>path, replaced:new Set(),
        hashes:{of:path=>createHash('sha256').update(readFileSync(path)).digest('hex')}});
      collector.add(dir);
      const build = {binary:{inputs:[...collector.inputs.values()], directories:[...collector.directories.values()],
        links:[...collector.links.values()], missing:[]}};
      const directories = packageDirectories([dir]);
      expect(watchOutput()(alias, true)).toBe(true);
      expect(build.binary.inputs.some(input => input.path === resolve(alias, 'log'))).toBe(false);
      writeFileSync(resolve(output, 'log'), 'second'); writeFileSync(resolve(output, 'new'), 'generated');
      expect(pendingBuildInputs(build)).toEqual([]);
      expect(watchedNewer(Infinity, [dir], directories)).toEqual([]);
      if (tool !== 'node_modules') {
        writeFileSync(resolve(output, 'package.json'), '{}');
        collector.add(alias);
        expect(collector.directories.has(alias)).toBe(false);
      }
      collector.add(resolve(alias, 'log'));
      expect(collector.inputs.has(resolve(alias, 'log'))).toBe(true);
    } finally { rmSync(dir, {recursive:true, force:true}); }
  });

test('app directory freshness ignores incidental files while external watches keep hidden sources', async () => {
  const {packageDirectories} = await import('./cargo-dep-info.mjs');
  const {watchedNewer, receiptChanges} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-app-notes-'));
  try {
    const app = resolve(dir, 'app'), source = resolve(app, 'src'), target = resolve(app, 'target');
    mkdirSync(source, {recursive:true}); mkdirSync(target);
    writeFileSync(resolve(app, 'app.json'), '{}');
    writeFileSync(resolve(source, 'input.rs'), 'source');
    writeFileSync(resolve(source, 'notes.md'), 'notes');
    const directories = packageDirectories([source]), since = Date.now() + 60_000;
    const info = resolve(target, 'aarch64-apple-darwin/debug/libnative_apple.d');
    mkdirSync(dirname(info), {recursive:true}); writeFileSync(info, `archive: ${source}\n`);
    const receipt = resolve(target, 'receipt.json');
    writeFileSync(receipt, JSON.stringify({target:'aarch64-apple-darwin', started:new Date(since).toISOString(), build:{watchedDirectories:directories}}));
    const fixture = {dir:app, target, workspace:dir, manifest:{}, crate:()=>'native-apple'};
    const fresh = () => {
      expect(watchedNewer(since, [source], directories)).toEqual([]);
      expect(receiptChanges(receipt, fixture)).toEqual([]);
    };
    fresh();
    for (const name of ['notes.md', 'shot.png', 'run.log']) { writeFileSync(resolve(source, name), 'new'); fresh(); }
    for (const name of ['notes.md', 'shot.png', 'run.log']) { rmSync(resolve(source, name)); fresh(); }
    const external = packageDirectories([dir]);
    mkdirSync(resolve(dir, '.config')); writeFileSync(resolve(dir, '.config/input'), 'source');
    expect(watchedNewer(since, [dir], external).length).toBeGreaterThan(0);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('Apple module directory capture uses the receipt output filter', async () => {
  const {directoryInputNames, watchOutput} = await import('./cargo-dep-info.mjs');
  const {pendingBuildInputs} = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-apple-modules-'));
  try {
    const directories = new Map(), absent = new Map();
    for (const name of ['ios/modules', 'modules/apple']) for (const child of ['native', 'node_modules', 'target', '.git', '.exact', '.build'])
      mkdirSync(resolve(dir, name, child), {recursive:true});
    const source = readFileSync(resolve(import.meta.dir, 'app.mjs'), 'utf8');
    const start = source.indexOf('  if (assetCatalogApp) for (const path'), end = source.indexOf('  for (const entry of appleAssetCatalogs)', start);
    assert.ok(start >= 0 && end > start);
    const bindings = {assetCatalogApp:{}, app:{dir}, platform:'ios', directories, absent, nameOf:path=>path,
      existsSync, readdirSync, resolve, directoryInputNames, watchOutput, watchOutputs:[]};
    new Function(...Object.keys(bindings), source.slice(start, end))(...Object.values(bindings));
    const build = {binary:{inputs:[], missing:[...absent.values()], directories:[...directories.values()]}};
    expect(pendingBuildInputs(build)).toEqual([]);
    for (const entry of directories.values()) {
      expect(entry.names).toEqual(['native']);
      writeFileSync(resolve(entry.path, '.exact/run.log'), 'output');
      expect(pendingBuildInputs(build)).toEqual([]);
      mkdirSync(resolve(entry.path, 'added'));
      expect(pendingBuildInputs(build)).toContain(entry.path);
      rmSync(resolve(entry.path, 'added'), {recursive:true});
    }
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32')('a watch of a directory inside a Bun-linked installed package keeps its sources', async () => {
  const {buildInputCollector, packageDirectories} = await import('./cargo-dep-info.mjs');
  const {pendingBuildInputs} = await import('./app.mjs');
  const {watchedNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-bun-subdirectory-'));
  try {
    const store = resolve(dir, 'node_modules/.bun/pkg@1/node_modules/pkg'), link = resolve(dir, 'node_modules/pkg');
    mkdirSync(resolve(store, 'src'), {recursive:true});
    writeFileSync(resolve(store, 'package.json'), '{}'); writeFileSync(resolve(store, 'src/index.ts'), 'source');
    symlinkSync(store, link, 'dir');
    const src = resolve(link, 'src');
    const collector = buildInputCollector({outputs:[], nameOf:path=>path, replaced:new Set(),
      hashes:{of:path=>createHash('sha256').update(readFileSync(path)).digest('hex')}});
    collector.add(src);
    const build = {binary:{inputs:[...collector.inputs.values()], directories:[...collector.directories.values()],
      links:[...collector.links.values()], missing:[]}};
    expect(build.binary.inputs.map(input => input.path)).toEqual([resolve(src, 'index.ts')]);
    const directories = packageDirectories([src]);
    expect(directories.map(snapshot => snapshot.path)).toEqual([src]);
    expect(pendingBuildInputs(build)).toEqual([]);
    expect(watchedNewer(Infinity, [src], directories)).toEqual([]);
    writeFileSync(resolve(src, 'added.ts'), 'source');
    expect(pendingBuildInputs(build)).toContain(src);
    expect(watchedNewer(Infinity, [src], directories)).toContain(src);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('a game\'s generated-product inventory omits tool directories as freshness does', async () => {
  const {inputProducts} = await import('./cargo-dep-info.mjs');
  const {watchedNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-generated-products-'));
  try {
    const app = resolve(dir, 'app'), assets = resolve(app, 'assets'), tables = resolve(assets, 'exact-tables.json');
    mkdirSync(resolve(assets, '.exact'), {recursive:true});
    writeFileSync(resolve(app, 'app.json'), '{}'); writeFileSync(tables, '{}');
    const generated = inputProducts([tables], app, [resolve(app, 'target')]);
    const products = new Map(generated.products.map(product => [resolve(product.path), product.sha256]));
    expect(watchedNewer(Date.now() + 60_000, [assets], generated.watchedDirectories, products, [resolve(app, 'target')])).toEqual([]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.skipIf(process.platform === 'win32')('a linked inventory excludes a Cargo build directory set by configuration', async () => {
  const {recordRustBake, linkedInputs} = await import('./linked-plan.mjs');
  const {watchedNewer} = await import('./agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'watch-linked-config-'));
  try {
    const pkg = resolve(dir, 'pkg'), app = resolve(dir, 'app'), since = Date.now() + 60_000;
    const baked = resolve(dir, 'bakes/macos-target.baked'), work = resolve(pkg, 'compiler-work');
    mkdirSync(baked, {recursive:true}); mkdirSync(app); mkdirSync(work, {recursive:true});
    writeFileSync(resolve(pkg, 'package.json'), '{}'); writeFileSync(resolve(pkg, 'source'), 'source');
    writeFileSync(resolve(work, 'unit'), 'output');
    const executable = resolve(app, 'baker'); writeFileSync(executable, '#!/bin/sh\n', {mode:0o755});
    const stdout = `cargo:rerun-if-changed=${pkg}\n`;
    writeFileSync(resolve(baked, 'app.plan'), 'plan'); writeFileSync(resolve(baked, 'compat.json'), '{}');
    writeFileSync(resolve(baked, 'rust-baker.json'), JSON.stringify({executable, cwd:app, env:{}, started:since}));
    writeFileSync(resolve(baked, 'rust-output.txt'), stdout);
    // Cargo's metadata names the build directory; the environment does not.
    const env = {EXACT_BAKED:baked, EXACT_BAKE_OUTPUT:resolve(dir, 'bakes')}, outputs = [env.EXACT_BAKE_OUTPUT, work];
    recordRustBake(env, outputs);
    const record = linkedInputs(baked);
    expect(watchedNewer(since, record.paths, record.watchedDirectories, new Map(), outputs)).toEqual([]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});
