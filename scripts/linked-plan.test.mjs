// Package and linked-bake freshness (scripts/linked-plan.mjs, scripts/cargo-dep-info.mjs and agent-launch.mjs
// `receiptChanges`): what a watched package directory, an optional design and a linked bake's inputs make stale.
import { afterEach, beforeEach, test, expect } from 'bun:test';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';

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
