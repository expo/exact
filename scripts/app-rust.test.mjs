// The Rust data crate named by app.json's rust.data and the hosts generated for it.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createApp } from '../game/new.mjs';
import { ensureShells, rustDataOf } from './app-shells.mjs';
import { authoredWorkspace } from './cargo-check.mjs';
import { buildModule } from '../host/web-js/module.mjs';
import { hostTriple } from './tools.mjs';
import { refreshCargoRustBindings } from './cargo-dep-info.mjs';
import { resolveApp } from './app.mjs';
import { webSourceRoots } from '../host/web/build.mjs';

test('a generated Rust Linux app opens its default update store', () => {
  const root = resolve(import.meta.dir, '..'), recipe = resolve(root, 'docs/recipes/progressive-tabs');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-linux-store-')), app = resolve(scratch, 'linux-store-fixture');
  try {
    mkdirSync(app);
    for (const name of ['app.contract', 'data']) cpSync(resolve(recipe, name), resolve(app, name), { recursive: true });
    const manifest = JSON.parse(readFileSync(resolve(recipe, 'app.json'), 'utf8'));
    delete manifest.deploy.store.linux;
    writeFileSync(resolve(app, 'app.json'), JSON.stringify(manifest));
    const store = resolve(app, 'updates');
    const env = { ...process.env, EXACT_APP_DIR: app, CARGO_TARGET_DIR: resolve(scratch, 'target'),
      CARGO_BUILD_BUILD_DIR: process.env.CARGO_BUILD_BUILD_DIR ?? process.env.CARGO_TARGET_DIR ?? resolve(root, 'target'),
      EXACT_UPDATE_TRUST: 'development', EXACT_UPDATE_ORIGIN: '', EXACT_UPDATE_DIR: store, EXACT_JSON_FD: '' };
    const built = spawnSync(process.execPath, ['scripts/build-linux.mjs', app, '--json'], { cwd: root, env, encoding: 'utf8', timeout: 900_000 });
    if (process.platform === 'win32') {
      assert.notEqual(built.status, 0);
      assert.match(built.stderr, /set deploy\.store\.linux to "0"/);
      return;
    }
    assert.equal(built.status, 0, built.stderr || built.error?.message);
    const { path } = JSON.parse(built.stdout.trimEnd().split('\n').at(-1));
    const launch = spawnSync(path, [], { cwd: app, env: { ...env, EXACT_AGENT: '1' }, input: '{"op":"state"}\n',
      encoding: 'utf8', timeout: 30_000 });
    assert.equal(launch.status, 0, launch.stderr || launch.error?.message);
    assert.equal(JSON.parse(launch.stdout.trimEnd().split('\n').at(-1)).delivery.L, 'A');
    assert.ok(existsSync(resolve(store, 'owner.lock')), 'the delivery adapter opened the store');
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 940_000);

test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST)('Contract presentation edits keep a Rust module; shape edits rebuild it', () => {
  const root = resolve(import.meta.dir, '..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const scratch = mkdtempSync(resolve(root, 'target/rust-shapes-')), dir = resolve(scratch, 'shapes-fixture');
  const previous = process.env.CARGO_TARGET_DIR, target = resolve(root, 'target/rust-shapes-tests');
  process.env.CARGO_TARGET_DIR = target;
  try {
    createApp(dir, { rust: true, title: 'Shapes Fixture', with: ['linux'] });
    const { workspace } = ensureShells(dir);
    const env = { ...process.env, EXACT_RUST_BINDINGS: '1' };
    const cargo = args => {
      const r = spawnSync('cargo', args, { cwd: workspace, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
      assert.equal(r.status, 0, r.stderr);
      return r.stdout.split('\n').filter(Boolean).map(JSON.parse);
    };
    const packages = cargo(['metadata', '--offline', '--format-version', '1'])[0].packages;
    const native = () => {
      refreshCargoRustBindings(target, hostTriple(), 'dev', packages, env);
      return cargo(['build', '--offline', '--lib', '-p', 'shapes-fixture-data', '--target', hostTriple(), '--message-format=json'])
        .find(m => m.reason === 'compiler-artifact' && m.target.name === 'shapes_fixture_data').fresh;
    };
    const path = resolve(dir, 'app.contract'), original = readFileSync(path, 'utf8');
    let stage = 0;
    const build = () => buildModule('shapes-fixture', resolve(scratch, `stage-${stage++}`), false, dir);
    const stamp = () => statSync(resolve(target, 'wasm32-unknown-unknown/web/shapes_fixture_module.wasm'), { bigint: true }).mtimeNs;
    const optimized = () => statSync(resolve(target, 'wasm32-unknown-unknown/web/shapes_fixture_module.optimized.wasm'), { bigint: true }).mtimeNs;
    build();
    // Cargo establishes a newly generated file's watch on the following build.
    build();
    assert.equal(native(), false);
    native();
    const first = stamp(), optimizedFirst = optimized();
    writeFileSync(path, original.replace('text "Shapes Fixture" role=', 'text "Edited title" role='));
    build();
    assert.equal(stamp(), first, 'a presentation edit must not rebuild the Rust module');
    assert.equal(optimized(), optimizedFirst, 'a presentation edit must not rerun wasm-opt');
    assert.equal(native(), true, 'a presentation edit must leave the native data crate fresh');
    writeFileSync(path, original.replace('shape Greeting', 'shape Extra\n  value: number\n\nshape Greeting'));
    build();
    assert.notEqual(stamp(), first, 'a shape edit must rebuild the Rust module');
    assert.equal(native(), false, 'a shape edit must rebuild the native data crate');
    const shaped = stamp(), lib = resolve(dir, 'data/src/lib.rs');
    writeFileSync(lib, readFileSync(lib, 'utf8').replace('Hello from', 'Greetings from'));
    build();
    assert.notEqual(stamp(), shaped, 'a Rust edit must rebuild the module');
    delete env.EXACT_RUST_BINDINGS;
    native();
    writeFileSync(path, original);
    assert.equal(native(), false, 'direct Cargo must still detect a shape edit');
  } finally {
    if (previous === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = previous;
    rmSync(scratch, { recursive: true, force: true });
  }
}, 300_000);

// Each case uses the same warm dependencies, with app sources and Cargo units of its own.
function moduleFixture(run) {
  const root = resolve(import.meta.dir, '..'), target = resolve(root, 'target/rust-shapes-tests');
  const scratch = mkdtempSync(resolve(root, 'target/rust-module-')), dir = resolve(scratch, 'shapes-fixture');
  const previous = process.env.CARGO_TARGET_DIR;
  process.env.CARGO_TARGET_DIR = target;
  try {
    createApp(dir, { rust: true, title: 'Shapes Fixture', with: ['linux'] });
    ensureShells(dir);
    const raw = resolve(target, 'wasm32-unknown-unknown/web/shapes_fixture_module.wasm');
    let packages, stage = 0;
    const env = { ...process.env, EXACT_RUST_BINDINGS: '1' }, workspace = resolve(dir, '.exact/shells');
    const native = () => {
      packages ??= JSON.parse(spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'],
        { cwd: workspace, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }).stdout).packages;
      refreshCargoRustBindings(target, hostTriple(), 'dev', packages, env);
      const result = spawnSync('cargo', ['build', '--offline', '--lib', '-p', 'shapes-fixture-data', '--target', hostTriple(), '--message-format=json'],
        { cwd: workspace, env, encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      return result.stdout.split('\n').filter(Boolean).map(JSON.parse)
        .find(m => m.reason === 'compiler-artifact' && m.target.name === 'shapes_fixture_data').fresh;
    };
    run({ dir, raw, native, build: () => readFileSync(buildModule('shapes-fixture', resolve(scratch, `stage-${stage++}`), false, dir)) });
  } finally {
    if (previous === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = previous;
    rmSync(scratch, { recursive: true, force: true });
  }
}

function featureFixture(run) {
  moduleFixture(fixture => {
    const manifest = resolve(fixture.dir, 'data/Cargo.toml'), lib = resolve(fixture.dir, 'data/src/lib.rs');
    writeFileSync(lib, readFileSync(lib, 'utf8').replace('text: format!("Hello from {name}."),',
      'text: format!("Hello from {name}: {}.", cfg!(feature = "alternate")),'));
    const original = readFileSync(manifest, 'utf8');
    const feature = enabled => writeFileSync(manifest, `${original}\n[features]\ndefault = [${enabled ? '"alternate"' : ''}]\nalternate = []\n`);
    feature(false);
    run({ ...fixture, feature });
  });
}

const moduleTest = test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST);
moduleTest('presentation edits keep module bindings and module calls rebuild them', () => moduleFixture(({ dir, native }) => {
  const data = resolve(dir, 'data'), manifest = resolve(data, 'Cargo.toml');
  const exactData = resolve(import.meta.dir, '../data');
  writeFileSync(manifest, readFileSync(manifest, 'utf8').replace('[dependencies]', `[dependencies]\nexact-data = { path = ${JSON.stringify(exactData)} }`));
  writeFileSync(resolve(data, 'build.rs'), `fn main() {
    contract::build_module_rust(std::path::Path::new("../app.contract")).unwrap();
}`);
  writeFileSync(resolve(data, 'src/lib.rs'), 'include!(concat!(env!("OUT_DIR"), "/modules.rs"));\n');
  const path = resolve(dir, 'app.contract');
  writeFileSync(path, 'component App\n  view\n    text "Before"\n');
  assert.equal(native(), false);
  native();
  writeFileSync(path, 'component App\n  view\n    text "After"\n');
  assert.equal(native(), true, 'a view edit must leave the data crate fresh');
  writeFileSync(path, `use geolocationStatus, GeolocationStatus from "@exact/geolocation"

component App
  resource status = geolocationStatus() as shape GeolocationStatus else empty()
  view
    text status.state
`);
  assert.equal(native(), false, 'a new module call must rebuild the data crate');
  assert.equal(native(), true);
}), 300_000);

moduleTest('a dependency default-feature edit invalidates a module in a new stage', () => featureFixture(({ build, feature }) => {
  const before = build();
  feature(true);
  assert.notDeepEqual(build(), before, 'dependency manifests must invalidate module reuse');
}), 300_000);

moduleTest('Cargo rebuilds a module when RUSTFLAGS change', () => moduleFixture(({ dir, build }) => {
  const lib = resolve(dir, 'data/src/lib.rs'), previous = process.env.RUSTFLAGS;
  writeFileSync(lib, readFileSync(lib, 'utf8').replace('text: format!("Hello from {name}."),',
    'text: format!("Hello from {name}: {}.", cfg!(alternate)),'));
  try {
    const before = build();
    process.env.RUSTFLAGS = `${previous ?? ''} --cfg alternate`.trim();
    assert.notDeepEqual(build(), before, 'Cargo must evaluate compiler flags on every build');
  } finally {
    if (previous === undefined) delete process.env.RUSTFLAGS; else process.env.RUSTFLAGS = previous;
  }
}), 300_000);

moduleTest('Cargo selects a feature-dependent shape entry after the previous entry is removed', () => moduleFixture(({ dir, build, native }) => {
  const manifest = resolve(dir, 'data/Cargo.toml'), original = readFileSync(manifest, 'utf8');
  const feature = enabled => writeFileSync(manifest, `${original}\n[features]\ndefault = [${enabled ? '"new-entry"' : ''}]\nnew-entry = []\n`);
  writeFileSync(resolve(dir, 'data/build.rs'), `fn main() {
    let path = if cfg!(feature = "new-entry") { "../new.contract" } else { "../old.contract" };
    contract::build_rust(std::path::Path::new(path)).unwrap();
}\n`);
  feature(false);
  const old = resolve(dir, 'old.contract'), next = resolve(dir, 'new.contract');
  writeFileSync(old, readFileSync(resolve(dir, 'app.contract')));
  const lib = resolve(dir, 'data/src/lib.rs');
  writeFileSync(lib, readFileSync(lib, 'utf8').replace('text: format!("Hello from {name}."),',
    'text: format!("Hello from {name}."), ..Default::default()'));
  const before = build();
  native();
  renameSync(old, next);
  writeFileSync(next, readFileSync(next, 'utf8').replace('shape Greeting\n', 'shape Greeting\n  count: number\n'));
  feature(true);
  assert.notDeepEqual(build(), before, 'the selected shapes must reach the wasm module');
  assert.equal(native(), false, 'the selected shapes must rebuild the native artifact');
  rmSync(next);
  assert.throws(build, /did not build/, 'a missing selected entry must fail the wasm build');
  assert.throws(native, 'a missing selected entry must fail the native build');
}), 300_000);

moduleTest('retargeting a Contract entry symlink refreshes its generated shapes', () => moduleFixture(({ dir, build, native }) => {
  const entry = resolve(dir, 'app.contract'), first = resolve(dir, 'first.contract'), next = resolve(dir, 'second.contract');
  renameSync(entry, first);
  writeFileSync(next, readFileSync(first, 'utf8').replace('shape Greeting\n', 'shape Greeting\n  count: number\n'));
  symlinkSync('first.contract', entry);
  const lib = resolve(dir, 'data/src/lib.rs');
  writeFileSync(lib, readFileSync(lib, 'utf8').replace('text: format!("Hello from {name}."),',
    'text: format!("Hello from {name}."), ..Default::default()'));
  const before = build();
  build();
  native();
  native();
  rmSync(entry);
  symlinkSync('second.contract', entry);
  assert.notDeepEqual(build(), before, 'the authored entry must follow its current symlink target');
  assert.equal(native(), false, 'retargeting the entry must also rebuild the native artifact');
}), 300_000);

moduleTest('the web watcher retains a shape generator\'s separate Contract package graph', () => moduleFixture(({ dir, build, raw }) => {
  const entry = resolve(dir, 'data.contract'), pkg = resolve(dir, 'node_modules/@fixture/shapes');
  mkdirSync(pkg, { recursive: true });
  writeFileSync(resolve(pkg, 'package.json'), '{"name":"@fixture/shapes","version":"1.0.0","exports":"./index.contract"}');
  writeFileSync(resolve(dir, 'package.json'), '{"dependencies":{"@fixture/shapes":"1.0.0"}}');
  const shape = name => `shape ${name}\n  value: number\ncomponent DataShapes\n  state payload = ${name}(value=1)\n  view\n    text \`\${payload.value}\`\n`;
  writeFileSync(resolve(pkg, 'index.contract'), 'use DataShapes from "./first.contract"\n');
  writeFileSync(resolve(pkg, 'first.contract'), shape('Payload'));
  writeFileSync(resolve(pkg, 'second.contract'), shape('Payload'));
  writeFileSync(entry, 'use DataShapes from "@fixture/shapes"\n' + readFileSync(resolve(dir, 'app.contract'), 'utf8') + '            DataShapes()\n');
  const script = resolve(dir, 'data/build.rs');
  writeFileSync(script, readFileSync(script, 'utf8').replace('../app.contract', '../data.contract'));
  build();
  build();
  const files = () => webSourceRoots(resolveApp(dir), true, 'none').filter(root => root.file).map(root => root.file);
  const watched = files();
  for (const path of [entry, resolve(pkg, 'package.json'), resolve(pkg, 'index.contract'), resolve(pkg, 'first.contract')]) {
    assert.ok(watched.includes(path), `the watcher must retain ${path}`);
  }
  const before = statSync(raw, { bigint: true }).mtimeNs;
  writeFileSync(resolve(pkg, 'index.contract'), 'use DataShapes from "./second.contract"\n');
  build();
  assert.equal(statSync(raw, { bigint: true }).mtimeNs, before, 'a changed import with identical shapes must keep the module');
  assert.ok(files().includes(resolve(pkg, 'second.contract')), 'the watcher must refresh the graph even when Cargo does not rebuild');
  writeFileSync(resolve(pkg, 'second.contract'), shape('ChangedPayload'));
  build();
  assert.notEqual(statSync(raw, { bigint: true }).mtimeNs, before, 'an imported shape edit must rebuild the module');
}), 300_000);

moduleTest('optimized wasm follows Cargo when it restores an older feature artifact', () => featureFixture(({ build, feature, raw }) => {
  const first = build(), rawFirst = readFileSync(raw);
  feature(true);
  rmSync(raw.replace(/\.wasm$/, '.d'));
  assert.notDeepEqual(build(), first);
  feature(false);
  rmSync(raw.replace(/\.wasm$/, '.d'));
  const restored = build();
  assert.deepEqual(readFileSync(raw), rawFirst, 'Cargo must restore configuration A');
  assert.deepEqual(restored, first, 'wasm-opt must consume the restored bytes');
}), 300_000);

moduleTest('shape generation survives missing module dep-info', () => moduleFixture(({ dir, raw, build }) => {
  build();
  rmSync(raw.replace(/\.wasm$/, '.d'));
  const path = resolve(dir, 'app.contract'), lib = resolve(dir, 'data/src/lib.rs');
  writeFileSync(path, readFileSync(path, 'utf8').replace('shape Greeting\n', 'shape Greeting\n  count: number\n'));
  writeFileSync(lib, readFileSync(lib, 'utf8').replace('Ok(Greeting {', 'Ok(Greeting { count: 1.0,'));
  build();
}), 300_000);

moduleTest('an edited build script replaces its obsolete shape entry path', () => moduleFixture(({ dir, build, native }) => {
  const script = resolve(dir, 'data/build.rs'), original = readFileSync(script, 'utf8');
  writeFileSync(resolve(dir, 'data-shapes.contract'), readFileSync(resolve(dir, 'app.contract')));
  writeFileSync(script, original.replace('../app.contract', '../data-shapes.contract'));
  build();
  native();
  renameSync(resolve(dir, 'data-shapes.contract'), resolve(dir, 'new-shapes.contract'));
  writeFileSync(script, original.replace('../app.contract', '../new-shapes.contract'));
  build();
  native();
}), 300_000);

moduleTest('an existing shape generator still rebuilds after a Contract edit', () => moduleFixture(({ dir, raw, build }) => {
  writeFileSync(resolve(dir, 'data/build.rs'), `fn main() {
    let path = std::path::Path::new("../app.contract");
    contract::rerun_if_changed(path);
    let (plan, calls) = contract::compile_path_for_rust(path).unwrap();
    let rust = contract::rust(&plan, &calls).unwrap();
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("shapes.rs"), rust).unwrap();
}\n`);
  build();
  const first = statSync(raw, { bigint: true }).mtimeNs, path = resolve(dir, 'app.contract');
  writeFileSync(path, readFileSync(path, 'utf8').replace('shape Greeting', 'shape Extra\n  value: number\n\nshape Greeting'));
  build();
  assert.notEqual(statSync(raw, { bigint: true }).mtimeNs, first);
}), 300_000);

test('exact new --rust writes a data crate that joins the generated workspace, and no host crate', () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-new-rust-'))), dir = resolve(scratch, 'field-log');
  try {
    createApp(dir, { rust: true, title: 'Field Log', with: ['linux'] });
    assert.equal(existsSync(resolve(dir, 'app.ts')), false);
    for (const file of ['data/Cargo.toml', 'data/build.rs', 'data/src/lib.rs']) assert.ok(existsSync(resolve(dir, file)), file);
    for (const file of ['Cargo.toml', 'Cargo.lock', '.cargo', 'apple', 'web', 'linux']) assert.equal(existsSync(resolve(dir, file)), false, file);
    assert.equal(authoredWorkspace(dir), null);
    assert.match(readFileSync(resolve(dir, 'data/Cargo.toml'), 'utf8'), /^workspace = "\.\.\/\.exact\/shells"$/m);
    // The generated workspace takes it as a member, with the host crates that link it.
    const { workspace } = ensureShells(dir);
    const loads = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: resolve(dir, 'data'), encoding: 'utf8' });
    assert.equal(loads.status, 0, loads.stderr);
    const { packages, workspace_root } = JSON.parse(loads.stdout);
    assert.equal(workspace_root, workspace);
    for (const name of ['field-log-data', 'field-log-web', 'field-log-linux']) assert.ok(packages.some(p => p.name === name), name);
    // The source answers the starter's one source, under the type app.json names.
    assert.match(readFileSync(resolve(dir, 'data/src/lib.rs'), 'utf8'), /pub struct FieldLog;[^]*"greeting" =>/);
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    assert.deepEqual(rustDataOf(manifest), { krate: 'field-log-data', ty: 'field_log_data::FieldLog', lib: 'field_log_data', source: 'field_log_data::FieldLog::default()' });
    // What the app tells an agent names its Rust data module, not app.ts or a pattern.
    const notes = readFileSync(resolve(dir, 'AGENTS.md'), 'utf8');
    assert.match(notes, /`data\/src\/lib\.rs` the data module \(Rust\)/);
    assert.doesNotMatch(notes, /app\.ts|pattern <name>|apple\/|web\//);
    assert.match(readFileSync(resolve(dir, 'app.contract'), 'utf8'), /^\/\/ Field Log: the view\. data\/src\/lib\.rs answers/);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 300_000);

test('exact new --rust refuses, before writing, a Snapback starter', () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-new-rust-refusal-'))), dir = resolve(scratch, 'notes');
  try {
    assert.throws(() => createApp(dir, { rust: true, snapback: {} }), /--rust and --snapback are separate starters/);
    assert.equal(existsSync(dir), false);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
});

test('a Rust app\'s source type never takes a name its starter or Rust already uses', async () => {
  const { rustNames, rustListing } = await import('./app-rust.mjs');
  for (const [title, type] of [['Greeting', 'GreetingApp'], ['Value', 'ValueApp'], ['Data Source', 'DataSourceApp'], ['Self', 'SelfApp'], ['Field Log', 'FieldLog'], ['3D', 'App3D'], ['API', 'APIApp'], ['TV', 'TVApp'], ['Q1', 'Q1App']])
  {
    assert.equal(rustNames('x', title).type, type, title);
    // app.json's rust.data names it: every name the scaffold gives it must be one the manifest admits.
    assert.ok(rustDataOf({ rust: { data: { crate: 'x-data', type: `x_data::${type}` } } }), type);
  }
  const listing = rustListing('Created /a\n  bun exact.mjs pattern      start\n  bun exact.mjs web          the web dev loop\n  bun exact.mjs linux        build\n  bun exact.mjs windows --run  on');
  assert.equal(listing, 'Created /a (its data is Rust: data/src/lib.rs)\n  bun exact.mjs web          the web dev loop\n  bun exact.mjs linux        build\n  bun exact.mjs windows --run  on');
});
