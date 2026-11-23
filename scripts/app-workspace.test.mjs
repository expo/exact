// An app's and a game's workspaces (scripts/app.mjs `resolveApp`, game/app/shells.mjs): shells, locks, Cargo graphs, GPU and
// render ownership, and what a deploy captures (R12–R15).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { basename, delimiter, dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { resolveApp, buildBake, bakeTarget, pendingBuildInputs, PINNED_RUST } from './app.mjs';
import { contractCompiler, snapshotOf, materializeSnapshot, disposeSnapshot } from './deploy.mjs';
import { binaryInputFile } from './deploy-heads.mjs';
import { binaryInputChanges } from './publisher.mjs';
import { receiptChanges } from './agent-launch.mjs';
import { pinned } from './test-workspace.mjs';

// These cases run cargo (the filesystem tool, bakes, locks). A shell whose PATH
// omits rustup's bin directory still finds it there; without cargo, say so.
const cargoBin = resolve(process.env.CARGO_HOME ?? resolve(homedir(), '.cargo'), 'bin');
if (!(process.env.PATH ?? '').split(delimiter).includes(cargoBin)) process.env.PATH = `${process.env.PATH ?? ''}${delimiter}${cargoBin}`;
if (!Bun.which('cargo', { PATH: process.env.PATH })) throw new Error(`these tests need cargo: put it on PATH or in ${cargoBin}`);
// The fixtures name their apps; a caller's EXACT_APP_DIR would redirect every one.
delete process.env.EXACT_APP_DIR;

function notificationServiceIdentity(service) {
  const path = 'NotificationService.swift', bytes = readFileSync(resolve(service, path));
  const size = Buffer.alloc(8); size.writeBigUInt64LE(BigInt(bytes.length));
  return { tree: createHash('sha256').update(path).update(Buffer.from([0])).update(size).update(bytes).digest('hex'),
    files: [{ path, sha256: createHash('sha256').update(bytes).digest('hex') }] };
}

function productionAppleReceiptFixture() {
  const root = pinned(realpathSync(mkdtempSync(resolve(import.meta.dir, '../target/production-apple-receipt-'))));
  const target = bakeTarget('linux'), id = 'com.exact.receipt';
  const write = (name, bytes) => { const path = resolve(root, name); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, bytes); };
  write('package.json', '{}\n');
  write('app.json', JSON.stringify({ name: 'receipt', app: { id, name: 'Receipt' }, host: { ios: {} }, rust: false }));
  write('app.contract', 'component App\n  view\n    text "Receipt"\n');
  write('Cargo.toml', '[workspace]\nmembers=["linux"]\nresolver="2"\n');
  write('linux/Cargo.toml', '[package]\nname="receipt-linux"\nversion="0.1.0"\nedition="2021"\n');
  write('linux/src/main.rs', 'fn main() {}\n');
  write('linux/build.rs', `fn main() {
    println!("cargo:rerun-if-env-changed=FIXTURE_COMPAT");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out.join("compat.json"), std::env::var("FIXTURE_COMPAT").unwrap()).unwrap();
    std::fs::write(out.join("artifacts.json"), r#"{"version":1,"artifacts":[],"sources":{}}"#).unwrap();
    std::fs::write(out.join("app.plan"), b"fixture").unwrap();
  }`);
  for (const args of [['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture']]) {
    const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  }
  const manifest = { name: 'receipt', app: { id, name: 'Receipt' }, host: { ios: {} }, rust: false };
  const app = { dir: root, workspace: root, target: resolve(root, 'target'), name: 'receipt', displayName: 'Receipt', id, manifest,
    crate: kind => `receipt-${kind}` };
  const bake = ({ selected = false } = {}) => {
    const service = resolve(root, 'modules/apple/notification-service');
    const notificationService = selected ? notificationServiceIdentity(service) : null;
    const compat = { target, delivery: {}, inputs: { platform: 'ios', app: id, store: { L: '0' }, keys: {}, notificationService } };
    return buildBake(app, 'ios', target, { kind: 'linux', profile: 'dev', output: resolve(root, 'bakes'),
      env: { CARGO_NET_OFFLINE: 'true', EXACT_UPDATE_TRUST: 'development', EXACT_APPLE_EXTENSIONS: selected ? 'notifications' : '', FIXTURE_COMPAT: JSON.stringify(compat) } });
  };
  const evidence = receipt => JSON.parse(binaryInputFile(receipt).bytes);
  const localChanges = receipt => {
    const path = resolve(app.target, 'fixture-receipt.json'), completed = new Date('2100-01-01T00:00:00Z');
    writeFileSync(path, JSON.stringify({ target: receipt.compat.target, started: completed.toISOString(), build: receipt }));
    utimesSync(path, completed, completed);
    return receiptChanges(path, app);
  };
  const relocatedChanges = async (receipt, change = () => {}) => {
    const snapshot = snapshotOf(app, { dirty: true, compiler: contractCompiler(resolve(import.meta.dir, '..')) }, root);
    const run = mkdtempSync(resolve(root, 'target/deploy-receipt-'));
    try {
      const staged = materializeSnapshot(snapshot, run, app), relocated = staged.app.dir;
      change(relocated);
      const relocatedApp = { ...app, dir: relocated, workspace: relocated, target: staged.app.target };
      return await binaryInputChanges(null, relocatedApp, receipt.compat, undefined, evidence(receipt));
    } finally {
      disposeSnapshot(snapshot);
      rmSync(run, { recursive: true, force: true });
    }
  };
  return { root, app, bake, evidence, localChanges, relocatedChanges, write, dispose: () => rmSync(root, { recursive: true, force: true }) };
}

test('receipts resolve GPU sources and build scripts in their own Cargo workspace', () => {
  const fixture = productionAppleReceiptFixture(), { root, app, write, bake } = fixture;
  try {
    app.gpuWorkspace = resolve(root, 'module');
    app.manifest.gpu = { modules: { sky: ['sky'] } };
    write('module/Cargo.toml', '[workspace]\n[package]\nname="receipt-gpu-sky"\nversion="0.1.0"\nedition="2021"\n[lib]\ncrate-type=["cdylib"]\n');
    write('module/src/lib.rs', '#[no_mangle] pub extern "C" fn color() -> u8 { include!("color.rs") }');
    write('module/src/color.rs', '42');
    write('module/build.rs', 'mod helper; fn main() { helper::watch(); }');
    write('module/helper.rs', 'pub fn watch() { println!("cargo:rerun-if-changed=src/color.rs"); }');
    const locked = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: app.gpuWorkspace, encoding: 'utf8' });
    assert.equal(locked.status, 0, locked.stderr);
    const receipt = bake();
    for (const path of ['src/lib.rs', 'src/color.rs', 'build.rs', 'helper.rs', 'Cargo.toml', 'Cargo.lock']) {
      assert.ok(receipt.binary.inputs.some(input => input.path === resolve(app.gpuWorkspace, path)), path);
    }
    assert.deepEqual(pendingBuildInputs(receipt), []);
    write('module/src/color.rs', '43');
    assert.ok(pendingBuildInputs(receipt).some(name => name.endsWith('/src/color.rs')));
  } finally { fixture.dispose(); }
}, 30000);

test.skipIf(process.platform === 'win32')('production Apple receipts retain main module-root link behavior', async () => {
  const fixture = productionAppleReceiptFixture(), { root, bake, evidence, localChanges, relocatedChanges } = fixture;
  try {
    const moduleRoot = resolve(root, 'modules/apple'), platformRoot = resolve(root, 'ios/modules');
    const clean = () => {
      for (const path of ['modules', 'ios', 'linked-apple', 'alias', 'real-apple', 'target/module-output'])
        rmSync(resolve(root, path), { recursive: true, force: true });
    };
    const moduleLinks = receipt => receipt.binary.links.filter(row => [moduleRoot, platformRoot].includes(row.path));

    clean();
    mkdirSync(resolve(root, 'modules')); mkdirSync(resolve(root, 'linked-apple'));
    writeFileSync(resolve(root, 'linked-apple/Native.swift'), 'struct Native {}\n');
    symlinkSync(resolve(root, 'linked-apple'), moduleRoot, 'dir');
    const linkedReceipt = bake();
    assert.deepEqual(moduleLinks(linkedReceipt), []);
    assert.deepEqual(evidence(linkedReceipt).links.filter(row => row.root === 'app' && row.path === 'modules/apple'), []);
    assert.deepEqual(pendingBuildInputs(linkedReceipt), []);
    assert.deepEqual(localChanges(linkedReceipt), []);
    assert.deepEqual(await relocatedChanges(linkedReceipt, relocated => {
      assert.equal(isAbsolute(readlinkSync(resolve(relocated, 'modules/apple'))), false);
    }), []);

    clean();
    mkdirSync(moduleRoot, { recursive: true }); writeFileSync(resolve(moduleRoot, 'Native.swift'), 'struct Native {}\n');
    const directoryReceipt = bake();
    renameSync(moduleRoot, resolve(root, 'linked-apple')); symlinkSync('../linked-apple', moduleRoot, 'dir');
    assert.deepEqual(pendingBuildInputs(directoryReceipt), []);
    assert.deepEqual(localChanges(directoryReceipt), []);
    assert.deepEqual(await relocatedChanges(directoryReceipt), []);

    clean();
    mkdirSync(resolve(root, 'modules'));
    symlinkSync('../alias', moduleRoot, 'dir'); symlinkSync('modules/apple', resolve(root, 'alias'), 'dir');
    const cyclicReceipt = bake();
    assert.ok(cyclicReceipt.binary.missing.includes(moduleRoot));
    assert.deepEqual(moduleLinks(cyclicReceipt), []);
    assert.deepEqual(pendingBuildInputs(cyclicReceipt), []);
    rmSync(resolve(root, 'alias')); mkdirSync(resolve(root, 'real-apple'));
    writeFileSync(resolve(root, 'real-apple/Native.swift'), 'struct Native {}\n'); symlinkSync('real-apple', resolve(root, 'alias'), 'dir');
    assert.ok(pendingBuildInputs(cyclicReceipt).includes(moduleRoot));
    assert.ok(localChanges(cyclicReceipt).includes(moduleRoot));
    assert.ok((await relocatedChanges(cyclicReceipt)).includes('app/modules/apple'));

    clean();
    mkdirSync(resolve(root, 'modules')); symlinkSync('../missing-a', moduleRoot, 'dir');
    const danglingReceipt = bake();
    assert.ok(danglingReceipt.binary.missing.includes(moduleRoot));
    assert.deepEqual(moduleLinks(danglingReceipt), []);
    rmSync(moduleRoot); symlinkSync('../missing-b', moduleRoot, 'dir');
    assert.deepEqual(pendingBuildInputs(danglingReceipt), []);
    assert.deepEqual(localChanges(danglingReceipt), []);
    assert.deepEqual(await relocatedChanges(danglingReceipt), []);

    clean();
    mkdirSync(resolve(root, 'modules')); mkdirSync(resolve(root, 'target/module-output'), { recursive: true });
    writeFileSync(resolve(root, 'target/module-output/Generated.swift'), 'struct Generated {}\n');
    symlinkSync(resolve(root, 'target/module-output'), moduleRoot, 'dir');
    const outputReceipt = bake();
    assert.deepEqual(outputReceipt.binary.directories.find(row => row.path === moduleRoot)?.names, []);
    assert.deepEqual(moduleLinks(outputReceipt), []);
    assert.equal(evidence(outputReceipt).directories.some(row => row.root === 'app' && row.path === 'modules/apple'), false);
    rmSync(moduleRoot); mkdirSync(moduleRoot); writeFileSync(resolve(moduleRoot, 'Native.swift'), 'struct Native {}\n');
    assert.ok(pendingBuildInputs(outputReceipt).includes(moduleRoot));
    assert.ok(localChanges(outputReceipt).includes(moduleRoot));
    assert.deepEqual(await relocatedChanges(outputReceipt), []);

    clean();
    const absentReceipt = bake();
    const moduleRoots = [platformRoot, moduleRoot];
    assert.deepEqual(absentReceipt.binary.directories.filter(row => moduleRoots.includes(row.path)), []);
    assert.deepEqual(absentReceipt.binary.links.filter(row => moduleRoots.includes(row.path)), []);
    assert.deepEqual(absentReceipt.binary.missing.filter(path => moduleRoots.includes(path)).sort(), moduleRoots.sort());
    assert.deepEqual(fixture.evidence(absentReceipt).absent.filter(row => ['ios/modules', 'modules/apple'].includes(row.path)).map(row => row.path).sort(), ['ios/modules', 'modules/apple']);
    assert.deepEqual(pendingBuildInputs(absentReceipt), []);
    assert.deepEqual(localChanges(absentReceipt), []);
    assert.deepEqual(await relocatedChanges(absentReceipt), []);
    mkdirSync(resolve(root, 'modules/apple/notification-service'), { recursive: true });
    writeFileSync(resolve(root, 'modules/apple/notification-service/NotificationService.swift'), 'class NotificationService {}\n');
    assert.ok(pendingBuildInputs(absentReceipt).includes(resolve(root, 'modules/apple')));
    assert.ok(localChanges(absentReceipt).includes(resolve(root, 'modules/apple')));
    assert.ok((await relocatedChanges(absentReceipt)).includes('app/modules/apple'));
  } finally { fixture.dispose(); }
}, 180_000);

test('production Apple receipts retain compiler dependencies inside the notification service folder', async () => {
  const fixture = productionAppleReceiptFixture(), { root, bake, evidence, localChanges, relocatedChanges, write } = fixture;
  const hidden = resolve(root, 'modules/apple/notification-service/.shared-data'), name = 'app/modules/apple/notification-service/.shared-data';
  try {
    write('modules/apple/notification-service/NotificationService.swift', 'class NotificationService {}\n');
    write('modules/apple/notification-service/.shared-data', 'compiled bytes');
    write('linux/src/main.rs', 'static SERVICE: &[u8] = include_bytes!("../../modules/apple/notification-service/.shared-data");\nfn main() { assert!(!SERVICE.is_empty()); }\n');
    for (const selected of [true, false]) {
      const receipt = bake({ selected }), input = receipt.binary.inputs.find(row => row.path === hidden);
      assert.equal(input?.name, name);
      assert.ok(evidence(receipt).files.some(row => row.root === 'app' && row.path === 'modules/apple/notification-service/.shared-data'));
      assert.deepEqual(pendingBuildInputs(receipt), []);
      assert.deepEqual(localChanges(receipt), []);
      writeFileSync(hidden, 'changed bytes!');
      assert.ok(pendingBuildInputs(receipt).includes(name));
      assert.ok(localChanges(receipt).includes(name));
      writeFileSync(hidden, 'compiled bytes');
      assert.ok((await relocatedChanges(receipt, relocated => writeFileSync(resolve(relocated, 'modules/apple/notification-service/.shared-data'), 'changed bytes!'))).includes(name));
    }
  } finally { fixture.dispose(); }
}, 180_000);

async function fixture(body) {
  const scratch = resolve(import.meta.dir, '../target');
  mkdirSync(scratch, {recursive:true});
  const root = mkdtempSync(resolve(scratch, 'shell-repair-'));
  const previous = process.env.EXACT_APP_DIR;
  const previousTarget = process.env.CARGO_TARGET_DIR;
  const write = (path, bytes) => { mkdirSync(dirname(resolve(root, path)), {recursive:true}); writeFileSync(resolve(root, path), bytes); };
  const run = (cmd, args) => {
    const r = spawnSync(cmd, args, {cwd:root, encoding:'utf8', timeout:60000});
    assert.equal(r.status, 0, r.stderr); return r.stdout;
  };
  const pkg = (path, name, source = '') => {
    write(`${path}/Cargo.toml`, `[package]\nname="${name}"\nversion="0.1.0"\nedition="2021"\n`);
    write(`${path}/src/lib.rs`, source);
  };
  const game = (name, crate = `${name}-logic`) => {
    const dir = `game/games/${name}`;
    write(`${dir}/app.contract`, 'component App\n  view\n');
    write(`${dir}/app.json`, JSON.stringify({name, app:{id:`com.exact.${name}`,name}, game:{crate,type:'SmallGame'}}));
    pkg(`${dir}/logic`, crate, 'pub struct SmallGame;');
    write(`${dir}/logic/Cargo.toml`, readFileSync(resolve(root, dir, 'logic/Cargo.toml'), 'utf8').replace('[package]', '[package]\nworkspace="../.shells"'));
    if (name !== 'foo' && existsSync(resolve(root,'game/games/foo/Cargo.lock'))) write(`${dir}/Cargo.lock`,readFileSync(resolve(root,'game/games/foo/Cargo.lock'),'utf8').replaceAll('foo',name));
    return resolve(root, dir);
  };
  try {
    // These fixtures assert the workspaces' default target paths.
    delete process.env.CARGO_TARGET_DIR;
    // The checkout files the scripts under test read, plus every module they import.
    const copied = new Set(), copy = path => {
      if (copied.has(path)) return;
      copied.add(path);
      const bytes = readFileSync(resolve(import.meta.dir, '..', path));
      write(path, bytes);
      if (!/\.m?js$/.test(path)) return;
      for (const [, spec] of bytes.toString('utf8').matchAll(/(?:\bfrom|\bimport\(?)\s*['"](\.{1,2}\/[^'"]+)['"]/g)) {
        const target = relative(resolve(import.meta.dir, '..'), resolve(dirname(resolve(import.meta.dir, '..', path)), spec));
        if (existsSync(resolve(import.meta.dir, '..', target))) copy(target);
      }
    };
    for (const path of ['scripts/app.mjs','scripts/variants.mjs','scripts/links.mjs','scripts/help.mjs','scripts/forma-design-path.mjs','scripts/app-icons.mjs','scripts/notification-service.mjs','scripts/png.mjs','host/web/beside.mjs','scripts/bun.mjs','js/bake/src/__exact_env.mjs','scripts/contract-diagnosis.mjs','scripts/filesystem.mjs','scripts/tools.mjs','scripts/kept.mjs','scripts/paths.mjs','scripts/file-dependencies.mjs','scripts/cargo-check.mjs','scripts/app-shells.mjs','scripts/document-utis.mjs','scripts/document-utis.json','scripts/web-head.mjs','scripts/share-target.mjs','scripts/rust.mjs','scripts/origin.mjs','scripts/install-page.mjs','scripts/sweep.mjs','scripts/app.schema.json','host/web/stages.mjs','host/web/build-lock.mjs','host/web/look.mjs','host/web-js/glue.mjs','host/web-js/route-pattern.js','host/apple/assets.mjs','host/apple/buildinfo.mjs','host/apple/shader-libraries.mjs','host/apple/svg-filter.mjs','host/apple/toolchain.mjs','host/apple/plist.mjs','scripts/grant-advice.mjs','host/apple/link.mjs','host/apple/Package.swift','host/apple/privacy.mjs','game/app/shells.mjs','game/inputs.mjs','game/app/crates.mjs','game/.cargo/config.toml','scripts/cargo-dep-info.mjs','scripts/package-libraries.mjs','scripts/linked-plan.mjs','scripts/app-cli.mjs','scripts/rustc/exact-rustc','js/bake/src/__exact_portable.mjs','js/bake/src/__exact_binding.mjs','js/bake/src/__exact_launch_setting.mjs','js/bake/src/typescript.mjs','host/apple/image.mjs','host/apple/metal.mjs','scripts/deploy-signing.mjs','js/bake/src/__exact_facts.mjs']) copy(path);
    // The rustc wrapper the scripts' cargo calls run on a Mac (`withHostDeployment`), found beside the copied scripts.
    chmodSync(resolve(root, 'scripts/rustc/exact-rustc'), 0o755);
    const { resolveApp: localResolveApp, cargoReproducibilityFlags: flags } = await import(resolve(root,'scripts/app.mjs'));
    const {prepareGame} = await import(resolve(root,'game/app/shells.mjs'));
    write('rust-toolchain.toml', readFileSync(resolve(import.meta.dir,'../rust-toolchain.toml')));
    const deps = ['exact-game','exact-game-render','exact-game-app','exact-game-apple','exact-game-bake','exact-runner','exact-web','exact-web-capabilities','exact-apple','exact-linux','exact-painted','exact-windows','wasm-bindgen','wasm-bindgen-futures','web-sys'];
    write('Cargo.toml', '[workspace]\nmembers=["stub","contract","logic","vendor/ibex/crates/ibex2-sqlite"]\nresolver="2"\n'); pkg('stub','root-stub');
    pkg('vendor/ibex/crates/ibex2-sqlite', 'ibex2-sqlite', 'pub fn link() {}');
    pkg('logic', 'exact-logic');
    pkg('contract','contract'); // This fixture has no Contract package imports.
    write('contract/src/main.rs', 'fn main() { println!("{}", r#"{\"packages\":[],\"sources\":[],\"consulted\":[]}"#); }');
    write('game/Cargo.toml', '[workspace]\nmembers=["deps/*","ordinary/*"]\nexclude=["games"]\nresolver="2"\n[workspace.package]\nversion="0.1.0"\nedition="2021"\nlicense="MIT"\n[workspace.dependencies]\n' + deps.map(n=>`${n}={path="deps/${n}"}`).join('\n'));
    for (const dep of deps) pkg(`game/deps/${dep}`, dep);
    write('game/bake/src/files.rs', 'pub fn bake_game_level<G>(_: impl AsRef<std::path::Path>) -> Result<(), String> { Ok(()) }\npub fn bake_game_tables<G>(_: impl AsRef<std::path::Path>) -> Result<(), String> { Ok(()) }\npub fn bake_game_sounds<G>(_: impl AsRef<std::path::Path>) -> Result<(), String> { Ok(()) }\n');
    write('game/games/.gitignore', '*/.shells/\n');
    // Cargo permits an empty glob when its containing directory exists.
    pkg('game/ordinary/stub','ordinary-stub');
    const dir = game('foo'); process.env.EXACT_APP_DIR = dir;
    prepareGame(dir, JSON.parse(readFileSync(resolve(dir,'app.json'))).game, resolve(root,'game'), {updateLock:true});
    rmSync(resolve(dir,'.shells'),{recursive:true});
    body({root, dir, write, run, pkg, game, flags, update:()=>prepareGame(dir, JSON.parse(readFileSync(resolve(dir,'app.json'))).game, resolve(root,'game'), {updateLock:true}), app:(name='foo')=>localResolveApp(name)});
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    if (previousTarget === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = previousTarget;
    rmSync(root,{recursive:true,force:true});
  }
}

test('shell fixture includes Apple build info', () => fixture(({root}) => {
  assert.ok(existsSync(resolve(root, 'host/apple/buildinfo.mjs')));
}), 60_000);

test('the Hermes installer owns its workspace in ordinary and nested checkouts', () => {
  const root = resolve(import.meta.dir, '..'), scratch = resolve(root, 'target');
  mkdirSync(scratch, {recursive:true});
  const outer = realpathSync(mkdtempSync(resolve(scratch, 'hermes-workspace-')));
  const installer = 'vendor/ibex/crates/hermes-lean-sys-installer';
  try {
    const nested = resolve(outer, '.claude/worktrees/lane');
    mkdirSync(resolve(nested, installer, 'src'), {recursive:true});
    writeFileSync(resolve(outer, 'Cargo.toml'), '[workspace]\nmembers=[]\n');
    writeFileSync(resolve(nested, 'Cargo.toml'), '[workspace]\nmembers=[]\nexclude=["vendor"]\n');
    for (const file of ['Cargo.toml', 'Cargo.lock', 'src/main.rs']) {
      writeFileSync(resolve(nested, installer, file), readFileSync(resolve(root, installer, file)));
    }
    for (const checkout of [root, nested]) {
      const manifest = resolve(checkout, installer, 'Cargo.toml');
      const result = spawnSync('cargo', ['metadata', '--offline', '--locked', '--no-deps',
        '--format-version', '1', '--manifest-path', manifest], {cwd:checkout, encoding:'utf8', timeout:60000});
      assert.equal(result.status, 0, result.stderr);
      const metadata = JSON.parse(result.stdout);
      assert.equal(metadata.workspace_root, dirname(manifest));
      assert.deepEqual(metadata.packages.map(p => p.manifest_path), [manifest]);
      assert.deepEqual(metadata.workspace_members, [metadata.packages[0].id]);
    }
    // Its own workspace takes none of the root's profiles, so it optimizes its dependencies itself, as the root's dev
    // profile does: setup's `--check` hashes each bundle with sha2, nine seconds a target unoptimized.
    const own = Bun.TOML.parse(readFileSync(resolve(root, installer, 'Cargo.toml'), 'utf8'));
    const rootProfile = Bun.TOML.parse(readFileSync(resolve(root, 'Cargo.toml'), 'utf8')).profile.dev.package['*'];
    assert.deepEqual(own.profile?.dev?.package?.['*'], rootProfile);
  } finally { rmSync(outer, {recursive:true, force:true}); }
}, 60_000);

test('shell repair replaces half-written members before metadata', () => fixture(({app}) => {
  const before = app(), path = before.cargoPackage('gpu').manifest_path;
  rmSync(resolve(dirname(path),'src'),{recursive:true});
  assert.ok(app().cargoPackage('gpu'));
  assert.ok(existsSync(resolve(dirname(path),'src/lib.rs')));
}), 60_000);

test('in-repo app members can own a separate locked workspace without EXACT_APP_DIR', () => fixture(({app, root, pkg, write, run}) => {
  delete process.env.EXACT_APP_DIR;
  write('apps/separate/app.contract', 'component App\n  view\n');
  write('apps/separate/app.json', JSON.stringify({name:'Separate',app:{id:'com.exact.separate',name:'Separate'}}));
  pkg('apps/separate/web', 'separate-web');
  write('apps/separate/web/Cargo.toml', '[package]\nworkspace="../../../optional"\nname="separate-web"\nversion="0.1.0"\nedition="2021"\n');
  write('optional/Cargo.toml', '[workspace]\nmembers=["../apps/separate/web"]\nresolver="2"\n');
  run('cargo', ['generate-lockfile', '--offline', '--manifest-path', resolve(root,'optional/Cargo.toml')]);
  const resolved = app('separate');
  assert.equal(resolved.workspace, resolve(root,'optional'));
  assert.equal(resolved.target, resolve(root,'optional/target'));
  assert.equal(resolved.cargoPackage('web').name, 'separate-web');
  pkg('apps/separate/linux', 'separate-linux');
  write('apps/separate/linux/Cargo.toml', '[package]\nworkspace="../../.."\nname="separate-linux"\nversion="0.1.0"\nedition="2021"\n');
  assert.throws(() => app('separate'), /different Cargo workspaces/);
}), 60_000);

test('in-repo declarative apps select generated native hosts without eagerly writing a Cargo workspace', () => fixture(({ app, root, write }) => {
  delete process.env.EXACT_APP_DIR;
  write('apps/plain/app.contract', 'component App\n  view\n');
  write('apps/plain/app.json', JSON.stringify({ name: 'Plain', app: { id: 'com.exact.plain', name: 'Plain' }, host: { ios: {} } }));
  const resolved = app('plain');
  assert.equal(resolved.generated, true);
  assert.equal(resolved.crate('ios'), 'plain-apple');
  assert.equal(resolved.target, resolve(root, 'apps/plain/target'));
  assert.equal(existsSync(resolve(root, 'apps/plain/.exact/shells/Cargo.toml')), false);
}), 60_000);

test('copied app identities keep separate Cargo graphs and generated hosts', () => fixture(({app, dir, game, write}) => {
  const first = app();
  const before = ['gpu','web','apple','linux'].map(kind => first.cargoPackage(kind).manifest_path);
  const copy = game('copy');
  const manifest = JSON.parse(readFileSync(resolve(copy, 'app.json'), 'utf8'));
  manifest.app.id = first.manifest.app.id;
  write('game/games/copy/app.json', JSON.stringify(manifest));
  process.env.EXACT_APP_DIR = copy;
  const second = app('copy');
  for (const kind of ['gpu','web','apple','linux']) {
    const pkg = second.cargoPackage(kind);
    assert.equal(pkg.name, `copy-${kind}`);
    assert.ok(pkg.manifest_path.startsWith(resolve(copy, '.shells') + sep));
  }
  process.env.EXACT_APP_DIR = dir;
  const reopened = app();
  assert.deepEqual(['gpu','web','apple','linux'].map(kind => reopened.cargoPackage(kind).manifest_path), before);
  for (const path of before) assert.ok(existsSync(path));
}), 60_000);

test('art adds its baker on demand and retains it until generated outputs are pruned', () => fixture(({app, dir, write, update}) => {
  const baked = () => {
    update();
    const gpu = app().cargoPackage('gpu');
    const dependency = gpu.dependencies.some(d=>d.name==='exact-game-bake');
    assert.ok(gpu.targets.some(t=>t.kind.includes('custom-build')));
    assert.equal(readFileSync(resolve(dirname(gpu.manifest_path),'build.rs'),'utf8').includes('exact_game_bake::bake_art'), dependency);
    return dependency;
  };
  assert.equal(baked(), false);
  write('game/games/foo/art/example.png', 'fixture');
  assert.equal(baked(), true);
  write('game/games/foo/.baked-assets.json', '{}');
  rmSync(resolve(dir, 'art'), {recursive:true});
  assert.equal(baked(), true, 'cleanup still needs the baker');
  rmSync(resolve(dir, '.baked-assets.json'));
  assert.equal(baked(), false);
}), 60_000);

test('host paths survive app identity and logic crate renames', () => fixture(({app, dir, write, run, update}) => {
  const before = app().cargoPackage('gpu').manifest_path;
  const manifest = JSON.parse(readFileSync(resolve(dir,'app.json'),'utf8'));
  manifest.app.id = 'org.example.renamed';
  manifest.game.crate = 'renamed-logic';
  write('game/games/foo/app.json',JSON.stringify(manifest));
  write('game/games/foo/logic/Cargo.toml','[package]\nworkspace="../.shells"\nname="renamed-logic"\nversion="0.1.0"\nedition="2021"\n');
  update();
  const after = app();
  assert.equal(after.cargoPackage('gpu').manifest_path, before);
  assert.equal(after.name, 'renamed');
  const metadata = JSON.parse(run('cargo',['metadata','--manifest-path','game/games/foo/.shells/Cargo.toml','--no-deps','--offline','--format-version','1']));
  assert.ok(!metadata.packages.some(p=>p.name==='foo-gpu'));
}), 60_000);

test('resolving a surviving game prunes a deleted game shell', () => fixture(({app, game}) => {
  const gone = game('gone');
  process.env.EXACT_APP_DIR = gone;
  const shell = dirname(app('gone').cargoPackage('gpu').manifest_path);
  rmSync(gone,{recursive:true});
  process.env.EXACT_APP_DIR = resolve(dirname(gone),'foo');
  assert.ok(app().hasGpu);
  assert.ok(!existsSync(shell));
}), 60_000);

test('ordinary GPU ownership requires its own manifest and matching metadata directory', () => fixture(({app, dir, pkg, write, run}) => {
  const manifest = JSON.parse(readFileSync(resolve(dir,'app.json'),'utf8')); delete manifest.game;
  // An ordinary app outside games/ sharing a workspace with an unrelated foo-gpu.
  process.env.EXACT_APP_DIR = resolve(dirname(dirname(dir)), 'ordinary/foo');
  pkg('game/ordinary/foo','foo-web');
  write('game/ordinary/foo/app.json',JSON.stringify(manifest)); write('game/ordinary/foo/app.contract','component App\n  view\n');
  pkg('game/ordinary/unrelated','foo-gpu');
  run('cargo',['metadata','--offline','--format-version','1','--manifest-path','game/Cargo.toml']);
  assert.equal(app().hasGpu,false);
  // A gpu/ crate of another name is not skipped in silence: the build names the package it must be.
  write('game/ordinary/foo/gpu/Cargo.toml','[package]\nname="some-other-gpu"\nversion="0.1.0"\n');
  assert.throws(() => app().hasGpu, /gpu\/Cargo\.toml names the package some-other-gpu; the app's GPU module must be named foo-gpu/);
}), 60_000);

test('missing game entry refuses by key at resolve time', () => fixture(({app, dir, write}) => {
  const manifest = JSON.parse(readFileSync(resolve(dir,'app.json'),'utf8'));
  write('game/games/foo/app.json',JSON.stringify({...manifest,game:undefined}));
  assert.throws(app, /app.json.*game|game.*required/);
}), 60_000);

test('game.type exports are checked by Rust compilation, not app resolution', () => fixture(({app, dir, write}) => {
  const manifest = JSON.parse(readFileSync(resolve(dir,'app.json'),'utf8'));
  write('game/games/foo/app.json',JSON.stringify({...manifest,game:{...manifest.game,type:'SmallGmae'}}));
  assert.ok(app().hasGpu);
}), 60_000);

test('game.type module paths resolve before Rust checks the referenced export', () => fixture(({app, dir, write}) => {
  const manifest = JSON.parse(readFileSync(resolve(dir,'app.json'),'utf8'));
  write('game/games/foo/app.json',JSON.stringify({...manifest,game:{...manifest.game,type:'play::SmallGame'}}));
  assert.ok(app().hasGpu);
  write('game/games/foo/logic/src/lib.rs', 'pub mod play;');
  write('game/games/foo/logic/src/play.rs', 'pub struct SmallGame;');
  assert.ok(app().hasGpu);
}), 60_000);

test('build graph refuses a requested GPU surface that Cargo cannot find', () => fixture(({app, pkg, root, run}) => {
  pkg('game/ordinary/foo','ordinary-web');
  const resolved = app();
  run('cargo',['generate-lockfile','--offline','--manifest-path','game/Cargo.toml']);
  const fake = {...resolved, workspace:resolve(root,'game'), hasGpu:true, crate:kind=>`ordinary-${kind}`};
  assert.throws(()=>buildBake(fake,'web','wasm32-unknown-unknown'), /GPU.*ordinary-gpu|ordinary-gpu.*surface/);
}), 60_000);

test('deploy excludes generated shells and regenerates them from captured game source', () => fixture(({app, root, write, run, game, dir, update}) => {
  const manifest = JSON.parse(readFileSync(resolve(dir,'app.json'),'utf8'));
  manifest.game.render = {crate:'foo-render',hooks:'Hooks'};
  write('game/games/foo/app.json', JSON.stringify(manifest));
  write('game/games/foo/render/Cargo.toml','[package]\nname="foo-render"\nversion="0.1.0"\nedition="2021"\nworkspace="../.shells"\n');
  write('game/games/foo/render/src/lib.rs','pub struct Hooks;');
  write('game/games/foo/render/shaders/fog.wgsl','// captured shader');
  update();
  const resolved = app();
  resolved.cargoPackage('gpu');
  run('cargo',['generate-lockfile','--offline','--manifest-path','game/games/foo/.shells/Cargo.toml']);
  run('cargo',['generate-lockfile','--offline']);
  run('cargo',['generate-lockfile','--offline','--manifest-path','game/Cargo.toml']);
  run('git',['init','-q']); run('git',['add','.']);
  run('git',['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture']);
  // A generated extra byte must never enter the captured source or dirty it.
  write('game/games/foo/.shells/generated-note','not source');
  write('game/target/cached-build','not source');
  write('game/render/target/cached-build','not source');
  const snapshot = snapshotOf(resolved,{},root);
  try {
    assert.equal(snapshot.dirty,false);
    const staged = materializeSnapshot(snapshot,resolve(root,'target/run'),resolved);
    assert.ok(!existsSync(resolve(staged.app.workspace,'generated-note')));
    const metadata = JSON.parse(spawnSync('cargo',['metadata','--no-deps','--offline','--format-version','1'],{cwd:staged.app.workspace,encoding:'utf8'}).stdout);
    const gpu = metadata.packages.find(p=>p.name==='foo-gpu');
    assert.ok(gpu,'materialized source must regenerate the GPU shell');
    assert.ok(gpu.manifest_path.startsWith(staged.sourceRoot));
    assert.ok(readFileSync(resolve(dirname(gpu.manifest_path),'src/lib.rs'),'utf8').includes('SmallGame'));
    assert.ok(readFileSync(resolve(dirname(gpu.manifest_path),'src/lib.rs'),'utf8').includes('game_render::Hooks'));
    const hooks = metadata.packages.find(p=>p.name==='foo-render');
    assert.ok(hooks.manifest_path.startsWith(staged.sourceRoot));
    assert.equal(readFileSync(resolve(dirname(hooks.manifest_path),'shaders/fog.wgsl'),'utf8'),'// captured shader');
  } finally { disposeSnapshot(snapshot); }
}), 60000); // Three lockfiles, a commit, a capture and Cargo metadata; Windows filesystem cost is higher.

test('R12 resolution is lazy and does not create a workspace', () => fixture(({app, dir}) => {
  const started = performance.now();
  const path=process.env.PATH;
  let resolved;
  try { process.env.PATH=resolve(dir,'no-executables'); resolved=app(); }
  finally { process.env.PATH=path; }
  assert.equal(resolved.name, 'foo');
  assert.equal(resolved.hasGpu, true);
  assert.ok(!existsSync(resolve(dir, '.shells')));
  assert.ok(performance.now() - started < 200);
}), 60_000);

test('R12 the captured source lock replaces a corrupted generated lock', () => fixture(({app, dir}) => {
  app().cargoPackage('gpu');
  const lock = resolve(dir, 'Cargo.lock');
  assert.ok(existsSync(lock), 'source lock must be outside ignored shells');
  const captured = readFileSync(lock, 'utf8');
  writeFileSync(resolve(dir, '.shells/Cargo.lock'), 'invalid generated cache');
  app().cargoPackage('gpu');
  assert.equal(readFileSync(resolve(dir, '.shells/Cargo.lock'), 'utf8'), captured);
}), 60_000);

test('R12 authored logic belongs only to its app workspace and locked edits refuse', () => fixture(({app, dir, run, write}) => {
  const resolved=app();resolved.cargoPackage('gpu');
  const metadata=JSON.parse(run('cargo',['metadata','--locked','--offline','--format-version','1','--manifest-path',resolve(dir,'.shells/Cargo.toml')]));
  const logic=metadata.packages.find(p=>p.name==='foo-logic');
  assert.ok(metadata.workspace_members.includes(logic.id));
  assert.equal(metadata.workspace_root,resolve(dir,'.shells'));
  const captured=readFileSync(resolve(dir,'Cargo.lock'),'utf8');
  write('game/games/foo/logic/Cargo.toml',readFileSync(resolve(dir,'logic/Cargo.toml'),'utf8').replace('version="0.1.0"','version="0.2.0"'));
  assert.throws(()=>app().cargoPackage('gpu'),/lock|locked/);
  assert.equal(readFileSync(resolve(dir,'Cargo.lock'),'utf8'),captured);
}), 60_000);

test('render crate isolation checks renamed transitive normal, build and inactive-target Cargo edges', () => fixture(({app, dir, root, run, write, update, pkg}) => {
  const previousDeclaration = app();
  const manifest = JSON.parse(readFileSync(resolve(dir,'app.json'),'utf8'));
  manifest.game.render = {crate:'foo-render',hooks:'Hooks'};
  write('game/games/foo/app.json', JSON.stringify(manifest));
  pkg('game/games/foo/render','foo-render','pub struct Hooks;');
  const renderManifest = 'game/games/foo/render/Cargo.toml';
  write(renderManifest, readFileSync(resolve(root,renderManifest),'utf8') + '\nworkspace="../.shells"\n');
  update();
  const graph = app().prepare();
  const hooks = graph.packages.find(p=>p.name==='foo-render');
  assert.ok(graph.workspace_members.includes(hooks.id));
  const original = readFileSync(resolve(dir,'logic/Cargo.toml'),'utf8');
  pkg('game/deps/bridge','bridge');
  write('game/deps/bridge/Cargo.toml', readFileSync(resolve(root,'game/deps/bridge/Cargo.toml'),'utf8') + '\n[dependencies]\nrenamed-hook={package="foo-render",path="../../games/foo/render"}\n');
  for (const table of ['dependencies','build-dependencies', 'target.\'cfg(target_os = "haiku")\'.dependencies']) {
    write('game/games/foo/logic/Cargo.toml', `${original}\n[${table}]\nbridge-alias={package="bridge",path="../../../deps/bridge"}\n`);
    assert.throws(update, /GPU-only: foo-logic -> bridge -> foo-render/);
  }
  // Capture the otherwise valid lock as an author could, then ask for a Windows
  // bake. Filtering out Haiku before checking would incorrectly admit this graph.
  write('game/games/foo/Cargo.lock', readFileSync(resolve(dir,'.shells/Cargo.lock'),'utf8'));
  assert.throws(() => app().prepare(true,{target:'x86_64-pc-windows-msvc'}), /GPU-only: foo-logic -> bridge -> foo-render/);
  assert.throws(() => previousDeclaration.prepare(true,{target:'x86_64-pc-windows-msvc'}), /GPU-only: foo-logic -> bridge -> foo-render/);
  write('game/games/foo/logic/Cargo.toml', original);
  update();
  // Presentation is permitted to read logic resource types in the opposite direction.
  write(renderManifest, readFileSync(resolve(root,renderManifest),'utf8') + '\n[dependencies]\ngame-logic={package="foo-logic",path="../logic"}\n');
  update();
  assert.ok(app().prepare().packages.some(p=>p.id===hooks.id));
}), 60000);

test('game profiles drop redundant dependency overrides and keep authored optimization', () => fixture(({app, dir, root, run, write, update}) => {
  write('game/Cargo.toml', readFileSync(resolve(root,'game/Cargo.toml'),'utf8') + `
[profile.gpu-dev]
inherits="dev"
opt-level=1
[profile.gpu-dev.package."*"]
opt-level=3
[profile.gpu-dev.package.exact-game]
opt-level=3
[profile.gpu-dev.package.absent-audio]
opt-level=3
[profile.gpu-dev.package.exact-game-render]
opt-level=2
[profile.gpu-dev.package."exact-runner@0.1.0"]
opt-level=3
[profile.gpu-dev.package.foo-logic]
opt-level=3
[profile.gpu-dev.package.foo-gpu]
opt-level=3
`);
  write('game/games/foo/logic/Cargo.toml', readFileSync(resolve(dir,'logic/Cargo.toml'),'utf8') + '\n[dependencies]\nexact-game.workspace=true\nexact-game-render.workspace=true\nexact-runner.workspace=true\n');
  update();
  app().cargoPackage('gpu');
  const packages = Bun.TOML.parse(readFileSync(resolve(dir,'.shells/Cargo.toml'),'utf8')).profile['gpu-dev'].package;
  assert.ok(!('exact-game' in packages));
  assert.ok(!('absent-audio' in packages));
  assert.equal(packages['foo-gpu']['opt-level'],3);
  assert.equal(packages['exact-runner@0.1.0']['opt-level'],3);
  const messages = run('cargo',['build','--offline','--locked','--manifest-path',resolve(dir,'.shells/Cargo.toml'),'-p','foo-logic','--profile','gpu-dev','--message-format=json']).trim().split('\n').map(JSON.parse);
  const units = new Map(messages.filter(m=>m.reason==='compiler-artifact').map(m=>[m.target.name,m.profile.opt_level]));
  assert.deepEqual(Object.fromEntries(units), {exact_game:'3',exact_game_render:'2',exact_runner:'3',foo_logic:'3'});
}), 180000);

test('game bakes resolve one fresh Cargo graph for the actual target and environment', () => fixture(({app, dir, root, run, write, pkg, update}) => {
  write('game/Cargo.toml', readFileSync(resolve(root,'game/Cargo.toml'),'utf8') + '\n[profile.gpu-dev]\ninherits="dev"\n');
  write('game/deps/exact-game/src/lib.rs', `
    pub enum Value { Number(f64), Bool(bool), Text(Box<str>), Other }
    impl Value {
      pub fn is_str(&self) -> bool { matches!(self, Self::Text(_)) }
      pub fn text(&self) -> &str { if let Self::Text(s) = self { s } else { "" } }
    }
    pub trait Args: Default { const FIELDS: &'static [(&'static str, ())]; fn values(&self) -> Vec<Value>; }
    impl Args for () {
      const FIELDS: &'static [(&'static str, ())] = &[("seed", ()), ("paused", ()), ("label", ())];
      fn values(&self) -> Vec<Value> { vec![Value::Number(7.0), Value::Bool(false), Value::Text("say \\"hi\\"\\n雪".into())] }
    }
    pub struct Actions;
    impl Actions { pub fn keys(&self) -> Vec<(&'static str, &'static str)> { Vec::new() } }
    pub trait Game { const NAME: &'static str; type Args: Args; fn actions() -> Actions { Actions } }
  `);
  write('game/deps/exact-game-render/src/lib.rs', '#[macro_export] macro_rules! module { ($game:ty) => { #[no_mangle] pub extern "C" fn answer() -> u32 { game_logic::ANSWER } }; }');
  write('game/games/foo/logic/src/lib.rs', `pub struct SmallGame; impl SmallGame { pub const NAME: &'static str = "inherent"; } pub const ANSWER: u32 = 42; impl exact_game::Game for SmallGame { const NAME: &'static str = "world"; type Args = (); }`);
  pkg('game/deps/wasm-only', 'wasm-only');
  write('game/games/foo/logic/Cargo.toml', readFileSync(resolve(dir,'logic/Cargo.toml'),'utf8') + '\n[dependencies]\nexact-game.workspace=true\n[target.\'cfg(target_arch = "wasm32")\'.dependencies]\nwasm-only={path="../../../deps/wasm-only"}\n');
  update();
  const info = app(), target = bakeTarget('linux'), cargo = run('rustup',['which','cargo']).trim();
  info.cargoPackage('gpu'); // A package lookup must not make a later build graph stale.
  for (const kind of ['gpu','web','apple']) assert.deepEqual(
    info.cargoPackage(kind).targets.find(t=>!t.kind.includes('custom-build')).crate_types,
    [kind === 'apple' ? 'staticlib' : 'cdylib']);
  const prepare = info.prepare;
  let graph;
  info.prepare = (...args) => graph = prepare(...args);
  const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
  const trace = resolve(root,'cargo-calls'), bin = resolve(root,'bin'), previous = process.env.PATH;
  // Calling cargo's binary directly skips rustup's proxy, so name its toolchain:
  // otherwise each rustc proxy picks one by directory and a build mixes two.
  const toolchain = /\/toolchains\/([^/]+)\/bin\/cargo$/.exec(cargo)?.[1];
  write('bin/cargo', `#!/bin/sh\nif [ "$1" = metadata ]; then printf '%s|%s\\n' "$*" "$CARGO_TARGET_DIR" >> ${quote(trace)}; fi\n${toolchain ? `RUSTUP_TOOLCHAIN=${quote(toolchain)} ` : ''}exec ${quote(cargo)} "$@"\n`);
  chmodSync(resolve(bin,'cargo'), 0o755);
  process.env.PATH = `${bin}:${previous}`;
  const bake = () => {
    writeFileSync(trace, '');
    const result = buildBake(info, 'linux', target, {profile:'gpu-dev', part:'gpu', env:{EXACT_UPDATE_TRUST:'development'}});
    const calls = readFileSync(trace,'utf8').trim().split('\n');
    assert.equal(calls.length, 1, calls.join('\n'));
    assert.ok(calls[0].includes(`--filter-platform ${target}`));
    assert.ok(calls[0].endsWith(`|${info.target}`));
    assert.equal(graph.target_directory, info.target);
    assert.equal(graph.build_directory, info.target);
    assert.ok(!graph.resolve.nodes.some(node => graph.packages.find(p=>p.id===node.id)?.name === 'wasm-only'));
    assert.ok(result.products.some(path=>/\.(so|dylib)$/.test(path)));
    assert.ok(result.products.every(path=>!path.endsWith('.rlib')));
    assert.ok(!existsSync(resolve(info.target,target,'gpu-dev/libfoo_gpu.rlib')));
  };
  try {
    bake();
    const declaration = resolve(dir, '.shells/surfaces.json');
    assert.deepEqual(JSON.parse(readFileSync(declaration, 'utf8')), {world:[
      {name:'seed', default:7}, {name:'paused', default:false}, {name:'label', default:'say "hi"\n雪'},
    ]});
    const timestamp = new Date(1234000);
    utimesSync(declaration, timestamp, timestamp);
    const web = info.prepare(true, {target:'wasm32-unknown-unknown', env:{...process.env, CARGO_TARGET_DIR:info.target}});
    assert.ok(web.resolve.nodes.some(node => web.packages.find(p=>p.id===node.id)?.name === 'wasm-only'));
    const privateBuild = resolve(dir,'private-build');
    assert.equal(info.prepare(true, {target, env:{...process.env, CARGO_BUILD_BUILD_DIR:privateBuild}}).build_directory, privateBuild);
    pkg('game/deps/new-dependency','new-dependency');
    write('game/games/foo/logic/Cargo.toml', readFileSync(resolve(dir,'logic/Cargo.toml'),'utf8').replace('[dependencies]', '[dependencies]\nnew-dependency={path="../../../deps/new-dependency"}'));
    assert.throws(bake, /locked|lock file/);
    update();
    bake();
    assert.equal(statSync(declaration).mtimeMs, timestamp.getTime(), 'a rebuilt GPU retains an unchanged declaration');
    assert.ok(graph.resolve.nodes.some(node => graph.packages.find(p=>p.id===node.id)?.name === 'new-dependency'));
  } finally { process.env.PATH = previous; }
}), 180000);

test('R12 named in-tree game resolves without EXACT_APP_DIR', () => fixture(({app, dir}) => {
  delete process.env.EXACT_APP_DIR;
  assert.equal(app().dir,dir);
}), 60_000);

test('R12 deploy captures initialized dependency submodules without building discovery in the snapshot', () => fixture(({app,root,write,run})=>{
  const resolved=app();resolved.cargoPackage('gpu');
  write('contract/src/main.rs', `fn main() { let target=std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into()); std::fs::write(std::path::Path::new(&target).join("source-discovery-target"), b"compiled").unwrap(); println!("{}", r#"{"packages":[],"sources":[],"consulted":[]}"#); }`);
  run('cargo',['generate-lockfile','--offline']);run('cargo',['generate-lockfile','--offline','--manifest-path','game/Cargo.toml']);
  write('vendor/fixture-source/data.txt','captured submodule');
  run('git',['-C','vendor/fixture-source','init','-q']);
  run('git',['-C','vendor/fixture-source','add','data.txt']);
  run('git',['-C','vendor/fixture-source','-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','source']);
  run('git',['init','-q']);run('git',['add','.']);
  const submoduleCommit=run('git',['-C','vendor/fixture-source','rev-parse','HEAD']).trim();
  mkdirSync(resolve(root,'vendor/absent-source'));
  run('git',['update-index','--add','--cacheinfo',`160000,${submoduleCommit},vendor/absent-source`]);
  run('git',['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture']);
  assert.throws(()=>snapshotOf(resolved,{},root), /vendor\/absent-source.*git submodule update --init vendor\/absent-source/);
  run('git',['rm','--cached','vendor/absent-source']);
  run('git',['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','remove absent fixture']);
  const snapshot=snapshotOf(resolved,{},root), previousPath=process.env.PATH, cargo=Bun.which('cargo'), trace=resolve(root,'target/snapshot-cargo');
  write('target/probe/cargo', `#!/bin/sh\ncase "$1" in build|run) printf '%s\\n' "$PWD" >> '${trace}';; esac\nexec '${cargo}' "$@"\n`);chmodSync(resolve(root,'target/probe/cargo'),0o755);process.env.PATH=`${root}/target/probe:${previousPath}`;
  try {
    assert.ok(snapshot.sources.some(source=>source.roles.includes('submodule')));
    const staged=materializeSnapshot(snapshot,resolve(root,'target/run'),resolved,resolve(root,'target/source-cache'));
    assert.equal(readFileSync(resolve(staged.exactRoot,'vendor/fixture-source/data.txt'),'utf8'),'captured submodule');
    assert.equal(existsSync(trace),false,'source discovery spawned Cargo in the snapshot');
    assert.equal(readFileSync(resolve(root,'target/source-discovery-target'),'utf8'),'compiled');
    assert.equal(existsSync(resolve(staged.app.target,'source-discovery-target')),false);
  } finally {process.env.PATH=previousPath;disposeSnapshot(snapshot);}
}), 30000); // two lockfiles, a submodule, three commits and two captures: 2.6 s at load 35, past five seconds on a loaded Mac

test('R13 capture refuses tracked files under inferred game output roots',()=>fixture(({app,root,write,run})=>{
  const resolved=app();resolved.cargoPackage('gpu');
  run('cargo',['generate-lockfile','--offline']);run('cargo',['generate-lockfile','--offline','--manifest-path','game/Cargo.toml']);
  run('git',['init','-q']);run('git',['add','.']);run('git',['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture']);
  for(const output of ['target','.shells','dist','dist.previous','artifacts']) {
    const path=`game/games/foo/${output}/source.rs`;write(path,'source');run('git',['add','-f',path]);
    assert.throws(()=>snapshotOf(resolved,{dirty:true},root),error=>error.message.includes(path)&&/tracked/.test(error.message));
    run('git',['rm','--cached',path]);rmSync(resolve(root,path));
  }
}),30000); // five captures, each spawning cargo and git: more than the default five seconds on a loaded Mac

test('without its own lock a game resolves only against the SDK lock, never a leftover shell lock',()=>fixture(({app,dir,write})=>{
  app().cargoPackage('gpu');const captured=readFileSync(resolve(dir,'Cargo.lock'),'utf8');rmSync(resolve(dir,'Cargo.lock'));
  assert.throws(()=>app().cargoPackage('gpu'),/no captured lock and no SDK lock/);
  write('game/app/shells.lock',captured);
  assert.ok(app().cargoPackage('gpu'));
  assert.equal(existsSync(resolve(dir,'Cargo.lock')),false,'the SDK lock writes no lock into the game');
}), 60_000);

test('ordinary resolution refuses missing and stale locks without writing them',()=>fixture(({app,root,pkg,write,run})=>{
  process.env.EXACT_APP_DIR=resolve(root,'game/ordinary/plain');
  pkg('game/ordinary/plain','plain-web');
  write('game/ordinary/plain/app.json',JSON.stringify({name:'Plain',app:{id:'com.exact.plain',name:'Plain'}}));
  write('game/ordinary/plain/app.contract','component App\n  view\n');
  rmSync(resolve(root,'game/Cargo.lock'),{force:true});
  assert.throws(()=>app('plain').cargoPackage('web'),/update the lock explicitly.*bun exact.mjs lock.*cargo metadata --offline/s);
  assert.equal(existsSync(resolve(root,'game/Cargo.lock')),false);
  run('cargo',['metadata','--offline','--format-version','1','--manifest-path','game/Cargo.toml']);
  const before=readFileSync(resolve(root,'game/Cargo.lock'),'utf8');
  assert.equal(app('plain').cargoPackage('web').name,'plain-web');
  const path='game/ordinary/plain/Cargo.toml';
  write(path,readFileSync(resolve(root,path),'utf8').replace('version="0.1.0"','version="0.2.0"'));
  assert.throws(()=>app('plain').cargoPackage('web'),/Cargo\.lock does not match its Cargo\.toml files.*`bun exact.mjs lock`.*`cargo metadata --offline --format-version 1`.*\nerror: .*--locked/s);
  assert.equal(readFileSync(resolve(root,'game/Cargo.lock'),'utf8'),before);
}), 60_000);

test('R13 explicit update-lock accepts a deliberate dependency change',()=>fixture(({app,dir,write,update})=>{
  app().cargoPackage('gpu');const before=readFileSync(resolve(dir,'Cargo.lock'),'utf8');
  write('game/deps/exact-game-render/Cargo.toml',readFileSync(resolve(dir,'../../deps/exact-game-render/Cargo.toml'),'utf8').replace('version="0.1.0"','version="0.2.0"'));
  assert.throws(()=>app().cargoPackage('gpu'),/locked|lock file/);
  update();assert.ok(app().cargoPackage('gpu'));assert.notEqual(readFileSync(resolve(dir,'Cargo.lock'),'utf8'),before);
}), 60_000);

test('R14 external game capture refuses tracked output roots',()=>fixture(({app,root,write,run})=>{
  const external = resolve(root, 'outside/foreign');
  mkdirSync(external,{recursive:true});
  for (const path of ['app.json','app.contract','Cargo.lock','logic/Cargo.toml','logic/src/lib.rs']) {
    write(`outside/foreign/${path}`,readFileSync(resolve(root,'game/games/foo',path)));
  }
  // Keep the fixture dependencies pointing at its engine from this different depth.
  const manifest = resolve(external,'logic/Cargo.toml');
  writeFileSync(manifest,readFileSync(manifest,'utf8').replaceAll('../../../deps/','../../../game/deps/'));
  write('outside/foreign/.gitignore','/.shells/\n/target/\n/dist/\n/dist.previous/\n/artifacts/\n');
  process.env.EXACT_APP_DIR=external;
  const resolved=app();resolved.cargoPackage('gpu');
  run('cargo',['generate-lockfile','--offline']);run('cargo',['generate-lockfile','--offline','--manifest-path','game/Cargo.toml']);
  run('git',['init','-q']);run('git',['add','.']);run('git',['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture']);
  for(const output of ['target','.shells','dist','dist.previous','artifacts']) {
    const path=`outside/foreign/${output}/source.rs`;write(path,'source');run('git',['add','-f',path]);
    assert.throws(()=>snapshotOf(resolved,{dirty:true},root),error=>error.message.includes(path)&&/tracked/.test(error.message));
    run('git',['rm','--cached',path]);rmSync(resolve(root,path));
  }
}), 60000);

test('R15 reproducibility flags follow workspace locks and always lock game shells', async () => {
  const {cargoReproducibilityFlags} = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'r15-locks-'));
  try {
    const ordinary = {workspace:dir,manifest:{}};
    assert.deepEqual(cargoReproducibilityFlags(ordinary),[]);
    writeFileSync(resolve(dir,'Cargo.lock'), '# stray lock');
    assert.deepEqual(cargoReproducibilityFlags(ordinary),[]);
    writeFileSync(resolve(dir,'Cargo.toml'), '[workspace]\nmembers=[]\n');
    assert.deepEqual(cargoReproducibilityFlags(ordinary),['--locked','--offline']);
    const game = {workspace:resolve(dir,'.shells'),manifest:{game:{}}};
    assert.deepEqual(cargoReproducibilityFlags(game),['--locked','--offline']);
    assert.deepEqual(cargoReproducibilityFlags(game,dir),['--locked','--offline']);
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('R15 the root Caltrain workspace refuses a missing lock and accepts its restored lock',()=>fixture(({app,root,pkg,write,run,flags})=>{
  pkg('apps/caltrain/web','caltrain-web');
  write('Cargo.toml','[workspace]\nmembers=["apps/caltrain/web"]\nresolver="2"\n');
  write('apps/caltrain/app.contract','component App\n  view\n');
  write('apps/caltrain/app.json',JSON.stringify({name:'Caltrain',app:{id:'com.exact.caltrain',name:'Caltrain'}}));
  process.env.EXACT_APP_DIR=resolve(root,'apps/caltrain');
  const metadata=()=>spawnSync('cargo',['metadata',...flags(app('caltrain')),'--format-version','1'],{cwd:root,encoding:'utf8'});
  assert.match(metadata().stderr,/locked|lock file/);
  run('cargo',['generate-lockfile','--offline']);
  assert.equal(metadata().status,0);
}), 60_000);

test('a worktree whose target resolves into another checkout is refused', async () => {
  const { mkdtempSync, mkdirSync, symlinkSync, rmSync, writeFileSync } = await import('node:fs');
  const { resolve } = await import('node:path');
  const { tmpdir } = await import('node:os');
  const { spawnSync } = await import('node:child_process');
  const { assertOwnTarget } = await import('./app.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-target-')), main = resolve(root, 'main'), lane = resolve(root, 'lane');
  const previousTarget = process.env.CARGO_TARGET_DIR;
  try {
    // The compiler must inspect lane/target, where this test puts the symlink.
    delete process.env.CARGO_TARGET_DIR;
    const git = (...args) => assert.equal(spawnSync('git', args, { cwd: main }).status, 0, args.join(' '));
    mkdirSync(main); git('init', '-q'); writeFileSync(resolve(main, 'a'), 'a'); git('add', 'a');
    git('-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-qm', 'a'); git('worktree', 'add', '-q', lane);
    mkdirSync(resolve(main, 'target')); symlinkSync(resolve(main, 'target'), resolve(lane, 'target'));
    assertOwnTarget(resolve(main, 'target'), main);
    assert.throws(() => assertOwnTarget(resolve(lane, 'target'), lane), /another checkout of this repository/);
    assert.throws(() => contractCompiler(lane), /another checkout of this repository/);
    mkdirSync(resolve(root, 'private'));
    assertOwnTarget(resolve(root, 'private'), lane);
  } finally {
    if (previousTarget === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = previousTarget;
    rmSync(root, { recursive: true, force: true });
  }
}, 60_000);

test('locked metadata fetches a missing git checkout without rewriting the lock', async () => {
  const { lockedMetadata } = await import('./app.mjs'), dir = pinned(mkdtempSync(resolve(tmpdir(), 'exact-metadata-fetch-')));
  const git = resolve(dir, 'source'), app = resolve(dir, 'app');
  const run = (cwd, cmd, args, env = process.env) => {
    const r = spawnSync(cmd, args, {cwd, env, encoding:'utf8'});
    assert.equal(r.status, 0, r.stderr); return r.stdout;
  };
  try {
    for (const path of [git, app]) mkdirSync(resolve(path, 'src'), {recursive:true});
    writeFileSync(resolve(git, 'Cargo.toml'), '[package]\nname="local-source"\nversion="0.1.0"\nedition="2021"\n');
    writeFileSync(resolve(git, 'src/lib.rs'), 'pub fn value() {}');
    run(git, 'git', ['init', '-q']); run(git, 'git', ['add', '.']);
    run(git, 'git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.test', 'commit', '-qm', 'source']);
    writeFileSync(resolve(app, 'Cargo.toml'), `[package]\nname="metadata-app"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nlocal-source={git="file://${git}"}\n`);
    writeFileSync(resolve(app, 'src/lib.rs'), '');
    run(app, 'cargo', ['generate-lockfile'], {...process.env, CARGO_HOME:resolve(dir, 'warm')});
    const lock = readFileSync(resolve(app, 'Cargo.lock'), 'utf8');
    const env = {...process.env, CARGO_HOME:resolve(dir, 'cold')};
    const missing = spawnSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], {cwd:app, env, encoding:'utf8'});
    assert.notEqual(missing.status, 0);
    const result = lockedMetadata(app, false, env);
    assert.equal(result.status, 0, result.stderr);
    assert.ok(JSON.parse(result.stdout).packages.some(p => p.name === 'local-source'));
    assert.equal(readFileSync(resolve(app, 'Cargo.lock'), 'utf8'), lock);
  } finally { rmSync(dir, {recursive:true, force:true}); }
}, 60_000);

test.each(['notes', 'planner'])('OS-specific folders resolve for %s without app-specific rules', async name => {
  const { moduleDirectory } = await import('./app.mjs');
  const dir = (await import('node:fs')).realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-platform-folders-')));
  const previous = process.env.EXACT_APP_DIR;
  const write = (p, text = '') => { mkdirSync(dirname(resolve(dir, p)), {recursive:true}); writeFileSync(resolve(dir, p), text); };
  try {
    process.env.EXACT_APP_DIR = dir;
    write('src/app.contract', 'component App\n  view\n    text "test"\n');
    assert.throws(() => resolveApp(name), /no app.contract/);
    write('app.contract', 'use App from "./src/app.contract"\n');
    write('app.json', JSON.stringify({name,app:{id:`com.exact.${name}`,name}}));
    for (const p of ['ios','macos','web']) write(`${p}/Cargo.toml`, `[package]\nname="${name}-${p}"\nversion="0.1.0"\n`);
    // Host crates the app writes are its authored workspace's (LLP 1153.003 D3).
    const { patchLines } = await import('./app.mjs');
    write('Cargo.toml', `[workspace]\nmembers = ["ios", "macos", "web"]\nresolver = "2"\n\n[patch.crates-io]\n${patchLines(dir).join('\n')}\n`);
    write('rust-toolchain.toml', readFileSync(resolve(import.meta.dir, '../rust-toolchain.toml'), 'utf8'));
    write('ios/modules/Input.swift'); write('ios/modules/notification-service/NotificationService.swift'); write('macos/modules/Input.swift'); write('web/modules/index.js');
    const app = resolveApp(`${name}-ios`);
    assert.equal(app.crate('ios'), `${name}-ios`); assert.equal(app.crate('macos'), `${name}-macos`);
    assert.deepEqual(app.modulesFor('ios').apple, [resolve(dir, 'ios/modules/Input.swift')]);
    assert.deepEqual(app.modulesFor('macos').apple, [resolve(dir, 'macos/modules/Input.swift')]);
    assert.equal(app.modules.web, resolve(dir, 'web/modules/index.js'));
    const { receiptChanges, refuseStale } = await import('./agent-launch.mjs');
    const { utimesSync } = await import('node:fs');
    const receipt = resolve(dir, 'target/receipt.json');
    const built = new Date(Date.now() + 10000), edited = new Date(+built + 10000);
    for (const platform of ['ios', 'macos']) {
      write('target/receipt.json', JSON.stringify({target: platform === 'ios' ? 'aarch64-apple-ios-sim' : 'aarch64-apple-darwin'}));
      utimesSync(receipt, built, built);
      assert.deepEqual(receiptChanges(receipt, app), []);
      const source = resolve(dir, `${platform}/modules/Input.swift`);
      utimesSync(source, edited, edited);
      const changed = receiptChanges(receipt, app);
      assert.deepEqual(changed, [source]);
      assert.throws(() => refuseStale(platform, receipt, changed, 'rebuild'), /build is stale:.*Input.swift/);
      utimesSync(source, built, built);
    }
    rmSync(resolve(dir, 'ios'), {recursive:true}); rmSync(resolve(dir, 'macos'), {recursive:true});
    write('modules/apple/Input.swift'); write('modules/apple/notification-service/NotificationService.swift');
    const shared = resolveApp(name);
    assert.equal(shared.crate('ios'), `${name}-apple`); assert.equal(shared.crate('macos'), `${name}-apple`);
    assert.equal(moduleDirectory(dir, 'ios'), resolve(dir, 'modules/apple'));
    assert.deepEqual(shared.modulesFor('macos').apple, [resolve(dir, 'modules/apple/Input.swift')]);
    assert.deepEqual(shared.modules.apple, [resolve(dir, 'modules/apple/Input.swift')]);
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    rmSync(dir, {recursive:true,force:true});
  }
});

test('native resource snapshot preserves ignored scoped packages, executable bits and links', () => fixture(({app,root,dir,write,run})=>{
  const resolved=app();resolved.cargoPackage('gpu');
  run('cargo',['generate-lockfile','--offline']);
  run('cargo',['generate-lockfile','--offline','--manifest-path','game/Cargo.toml']);
  write('.gitignore','**/node_modules/\n');
  write('game/games/foo/server/helper','#!/bin/sh\necho helper ran\n');
  chmodSync(resolve(dir,'server/helper'),0o755);
  const {symlinkSync,readlinkSync}=require('node:fs');
  symlinkSync('helper',resolve(dir,'server/link'));
  run('git',['init','-q']);run('git',['add','.']);
  run('git',['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture']);
  write('game/games/foo/server/node_modules/@scope/pkg/index.js','captured ignored dependency');
  const snapshot=snapshotOf(resolved,{dirty:true},root);
  try {
    const staged=materializeSnapshot(snapshot,resolve(root,'target/run'),resolved);
    assert.equal(readFileSync(resolve(staged.app.dir,'server/node_modules/@scope/pkg/index.js'),'utf8'),'captured ignored dependency');
    // Executable as the process's umask lets a new file be (an agent's is
    // often 077: the owner's bit alone), as git's own checkout makes it.
    assert.equal(statSync(resolve(staged.app.dir,'server/helper')).mode&0o111,0o111&~process.umask());
    assert.equal(readlinkSync(resolve(staged.app.dir,'server/link')),'helper');
  } finally {disposeSnapshot(snapshot);}
}),60000);


test('app update reports lock drift without changing it, and the app lock verb repairs it', async () => {
  const { createApp } = await import('../game/new.mjs');
  const { appVerbs } = await import('./app-cli.mjs');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-app-lock-'));
  const dir = resolve(scratch, 'app'); mkdirSync(dir); pinned(dir);
  const write = (file, text) => { mkdirSync(dirname(resolve(dir, file)), {recursive:true}); writeFileSync(resolve(dir, file), text); };
  const run = args => spawnSync(process.execPath, ['exact.mjs', ...args], { cwd: dir, encoding: 'utf8' });
  try {
    write('Cargo.toml', '[workspace]\nmembers=["web"]\nresolver="2"\n');
    write('app.contract', 'component App\n  view\n');
    write('web/Cargo.toml', '[package]\nname="lock-app-web"\nversion="0.1.0"\nedition="2021"\n');
    write('web/src/lib.rs', '');
    const initial = createApp(dir, {update:true});
    assert.match(initial, /Cargo.lock: none yet; run `bun exact.mjs lock`/);
    assert.deepEqual(appVerbs('lock-app', dir).lock, ['scripts/app-cli.mjs', '--lock', dir]);
    let locked = run(['lock']);
    assert.equal(locked.status, 0, locked.stderr);
    const before = readFileSync(resolve(dir, 'Cargo.lock'), 'utf8');
    write('web/Cargo.toml', '[package]\nname="lock-app-web"\nversion="0.2.0"\nedition="2021"\n');
    const updated = createApp(dir, {update:true});
    assert.match(updated, /Cargo.lock: does not match the app manifests; run `bun exact.mjs lock`/);
    assert.equal(readFileSync(resolve(dir, 'Cargo.lock'), 'utf8'), before);
    locked = run(['lock']);
    assert.equal(locked.status, 0, locked.stderr);
    assert.match(locked.stdout, /lock-app-web 0.1.0.*0.2.0/);
    assert.notEqual(readFileSync(resolve(dir, 'Cargo.lock'), 'utf8'), before);
    assert.doesNotMatch(createApp(dir, {update:true}), /does not match the app manifests/);
    const again = run(['lock']);
    assert.equal(again.status, 0, again.stderr);
    assert.match(again.stdout, /current; no changes/);
  } finally { rmSync(scratch, {recursive:true,force:true}); }
}, 60_000);

test('the wasm-bindgen the CLI processes is the one the GPU modules reach by normal dependencies', async () => {
  const { gpuBindgen } = await import('./app-cli.mjs');
  const pkg = (name, version, deps = [], source = null) => ({ id: `${name} ${version}`, name, version, source, manifest_path: `/w/${name}/Cargo.toml`,
    dependencies: deps.map(([n, req, kind = null]) => ({ name: n, req, kind })) });
  const node = (id, deps) => ({ id, deps: deps.map(([pkg, kind = null]) => ({ pkg, dep_kinds: [{ kind, target: null }] })) });
  const registry = 'registry+https://github.com/rust-lang/crates.io-index';
  const metadata = {
    packages: [
      pkg('app-gpu', '0.1.0', [['wasm-bindgen', '^0.2'], ['wasm-bindgen', '=0.2.128', 'dev'], ['helper', '^1', 'build']]),
      pkg('app-gpu-tools', '0.1.0', [['wasm-bindgen', '=0.2.130']]), pkg('app-gpu-sky', '0.1.0', [['wasm-bindgen', '^0.2']]),
      pkg('wasm-bindgen', '0.2.127', [], registry), pkg('wasm-bindgen', '0.2.128', [], registry), pkg('wasm-bindgen', '0.2.130', [], registry), pkg('helper', '1.0.0', [['wasm-bindgen', '=0.2.130']], registry),
    ],
    workspace_members: ['app-gpu 0.1.0', 'app-gpu-tools 0.1.0', 'app-gpu-sky 0.1.0'],
    resolve: { nodes: [
      node('app-gpu 0.1.0', [['wasm-bindgen 0.2.127'], ['wasm-bindgen 0.2.128', 'dev'], ['helper 1.0.0', 'build']]),
      node('helper 1.0.0', [['wasm-bindgen 0.2.130']]),
      node('app-gpu-tools 0.1.0', [['wasm-bindgen 0.2.130']]), node('app-gpu-sky 0.1.0', [['wasm-bindgen 0.2.127']]),
    ] },
  };
  // A dev or build dependency, and a member whose name only contains -gpu, are not what the CLI processes.
  assert.deepEqual(gpuBindgen(metadata, ['app-gpu']), { held: ['0.2.127'], requiring: [{ name: 'app-gpu', req: '^0.2', manifest: '/w/app-gpu/Cargo.toml' }] });
  // A declared module is, by its exact name.
  assert.deepEqual(gpuBindgen(metadata, ['app-gpu', 'app-gpu-sky']).held, ['0.2.127']);
  assert.deepEqual(gpuBindgen(metadata, ['app-gpu', 'app-gpu-tools']).held.sort(), ['0.2.127', '0.2.130']);
});

test('lock says when the GPU module links a wasm-bindgen the CLI refuses, and what to write, and changes nothing for it', async () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact app bindgen \'$x\' ')));
  const dir = resolve(scratch, 'my app'), lock = resolve(dir, 'Cargo.lock');
  const write = (file, text) => { mkdirSync(dirname(resolve(dir, file)), { recursive: true }); writeFileSync(resolve(dir, file), text); };
  const run = () => spawnSync(process.execPath, [resolve(import.meta.dir, 'app-cli.mjs'), '--lock', dir], { encoding: 'utf8' });
  const held = () => Bun.TOML.parse(readFileSync(lock, 'utf8')).package.filter((p) => p.name === 'wasm-bindgen').map((p) => p.version);
  const pin = Bun.TOML.parse(readFileSync(resolve(import.meta.dir, '../game/Cargo.toml'), 'utf8')).workspace.dependencies['wasm-bindgen'].replace(/^=/, '');
  const crate = (name, deps = '') => `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n\n[target.'cfg(target_arch = "wasm32")'.dependencies]\n${deps}`;
  try {
    mkdirSync(dir); pinned(dir);
    write('gpu/src/lib.rs', ''); write('other/src/lib.rs', '');
    // A wasm-bindgen of the app's own, 0.2.999, which a [patch] puts in every graph that names it.
    write('fake/Cargo.toml', '[package]\nname = "wasm-bindgen"\nversion = "0.2.999"\nedition = "2021"\n'); write('fake/src/lib.rs', '');
    write('Cargo.toml', '[workspace]\nmembers = ["gpu", "other"]\nresolver = "2"\n\n[patch.crates-io]\nwasm-bindgen = { path = "fake" }\n');
    write('gpu/Cargo.toml', crate('my-app-gpu', 'wasm-bindgen = "0.2"\n')); write('other/Cargo.toml', crate('my-app-other'));
    let locked = run();
    assert.equal(locked.status, 0, locked.stderr);
    assert.ok(locked.stdout.includes(`The GPU module's wasm build links wasm-bindgen 0.2.999 (my-app-gpu requires ^0.2), and the web build's wasm-bindgen CLI is ${pin}, which refuses any other: the web build will fail. Require wasm-bindgen = "=${pin}" in gpu/Cargo.toml's wasm32 dependencies, then run bun exact.mjs lock again`), locked.stdout);
    assert.deepEqual(held(), ['0.2.999'], 'lock reports; it rewrites nothing');
    // A requirement that excludes the pin is named as what to change.
    write('gpu/Cargo.toml', crate('my-app-gpu', 'wasm-bindgen = "=0.2.999"\n'));
    locked = run();
    assert.equal(locked.status, 0, locked.stderr);
    assert.ok(locked.stdout.includes(`Change my-app-gpu's requirement (gpu/Cargo.toml) to wasm-bindgen = "=${pin}", then run bun exact.mjs lock again.`), locked.stdout);
    // Another crate's wasm-bindgen is not the CLI's.
    write('gpu/Cargo.toml', crate('my-app-gpu')); write('other/Cargo.toml', crate('my-app-other', 'wasm-bindgen = "=0.2.999"\n'));
    locked = run();
    assert.equal(locked.status, 0, locked.stderr);
    assert.doesNotMatch(locked.stdout, /wasm-bindgen CLI/);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 120_000);

test('update refuses, before writing, a root the crates it adds cannot load in', async () => {
  const { createApp } = await import('../game/new.mjs');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-app-root-'));
  const dir = resolve(scratch, 'app'), root = resolve(dir, 'Cargo.toml');
  const keys = '[workspace.package]\nedition = "2021"\nversion = "0.1.0"\nlicense = "MIT"\n';
  try {
    createApp(dir, { with: ['android'] });
    pinned(dir);
    // The linux/ and android/ crates update adds inherit edition, version and license, as the copied-up workspace has them.
    for (const [text, said] of [
      ['[workspace]\nmembers = ["web"\n', /Cargo\.toml does not parse: /],
      ['[package]\nname = "app"\nversion = "0.1.0"\n', /Cargo\.toml has no \[workspace\] table/],
      ['[workspace]\nmembers = []\n[workspace.package]\nedition = "2021"\n', /Cargo\.toml has no version, license in \[workspace\.package\]/],
    ]) {
      writeFileSync(root, text);
      assert.throws(() => createApp(dir, { update: true }), (e) => said.test(e.message) && /"An authored workspace"/.test(e.message), text);
      assert.deepEqual([readFileSync(root, 'utf8'), existsSync(resolve(dir, 'linux')), existsSync(resolve(dir, 'exact.mjs.tmp'))], [text, false, false], 'nothing written');
    }
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 60_000);

test('update fails, with Cargo\'s words, when the workspace does not load', async () => {
  const { createApp } = await import('../game/new.mjs');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-app-load-'));
  const dir = resolve(scratch, 'app'), root = resolve(dir, 'Cargo.toml');
  const write = (file, text) => { mkdirSync(dirname(resolve(dir, file)), { recursive: true }); writeFileSync(resolve(dir, file), text); };
  const crate = (name, more = '') => `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n${more}`;
  try {
    createApp(dir);
    pinned(dir);
    for (const member of ['a', 'b']) write(`${member}/src/lib.rs`, '');
    for (const [what, files, said] of [
      ['two packages of one name', { 'a/Cargo.toml': crate('same'), 'b/Cargo.toml': crate('same') }, /two packages named `same`/],
      ['a key the root does not define', { 'a/Cargo.toml': '[package]\nname = "a"\nversion.workspace = true\nedition = "2021"\n', 'b/Cargo.toml': crate('b') }, /workspace\.package\.version` was not defined/],
      ['a member that is not there', { 'a/Cargo.toml': crate('a'), 'b/Cargo.toml': null }, /failed to load manifest for workspace member `[^`]*b`/],
      ['a member that does not parse', { 'a/Cargo.toml': crate('a', 'unknown = [\n'), 'b/Cargo.toml': crate('b') }, /a\/Cargo\.toml:5/],
    ]) {
      write('Cargo.toml', '[workspace]\nmembers = ["a", "b"]\nresolver = "2"\n');
      for (const [file, text] of Object.entries(files)) if (text === null) rmSync(resolve(dir, file), { force: true }); else write(file, text);
      assert.throws(() => createApp(dir, { update: true }), (e) => /^Updated [^]*\nbut Cargo cannot load the workspace /.test(e.message) && said.test(e.message), what);
    }
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 60_000);

// The issue's manifest (#163): the GPU module recipe's `<exact2>/contract/cli`, which names no exact-* crate.
test('update resolves every <exact2>/ path an app\'s manifests write, and leaves the app\'s own crates and comments', async () => {
  const { createApp } = await import('../game/new.mjs');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-app-paths-'));
  const dir = resolve(scratch, 'app'), exact2 = resolve(import.meta.dir, '..');
  const write = (file, text, root = dir) => { mkdirSync(dirname(resolve(root, file)), { recursive: true }); writeFileSync(resolve(root, file), text); };
  const crate = (name, deps = '') => `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n${deps}`;
  try {
    write('app.contract', 'component App\n  view\n');
    pinned(dir);
    write('Cargo.toml', '[workspace]\nmembers = ["web", "data"]\nresolver = "2"\n');
    const web = crate('paths-app-web', `
[dependencies]
exact-logic = { path = "<exact2>/logic" }
paths-app-data = { path = "../data" }
types = { package = "contract-types", path = '<exact2>/contract/types' }
# lower = { package = "contract-lower", path = "<exact2>/contract/lower" }

[build-dependencies.contract]
path = "<exact2>/contract/cli"
`);
    write('web/Cargo.toml', web);
    // The app's own crate named contract, at a real path.
    write('data/Cargo.toml', crate('paths-app-data', '\n[dependencies]\ncontract = { path = "../../shared/contract" }\n'));
    for (const member of ['web', 'data']) write(`${member}/src/lib.rs`, '');
    write('shared/contract/Cargo.toml', crate('contract'), scratch);
    write('shared/contract/src/lib.rs', '', scratch);
    createApp(dir, { update: true });
    const loads = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: dir, encoding: 'utf8' });
    assert.equal(loads.status, 0, loads.stderr);
    const updated = readFileSync(resolve(dir, 'web/Cargo.toml'), 'utf8'), parsed = Bun.TOML.parse(updated);
    const at = (path) => realpathSync(resolve(dir, 'web', path)), ours = (path) => realpathSync(resolve(exact2, path));
    assert.deepEqual([parsed.dependencies['exact-logic'], parsed.dependencies.types, parsed['build-dependencies'].contract].map(d => at(d.path)), ['logic', 'contract/types', 'contract/cli'].map(ours));
    assert.match(updated, /^types = \{ package = "contract-types", path = '[^']+' \}$/m);
    assert.match(updated, /^# lower = \{ package = "contract-lower", path = "<exact2>\/contract\/lower" \}$/m);
    assert.equal(Bun.TOML.parse(readFileSync(resolve(dir, 'data/Cargo.toml'), 'utf8')).dependencies.contract.path, '../../shared/contract');
    // A form it does not resolve is named, not left in silence.
    write('web/Cargo.toml', crate('paths-app-web', '\n[build-dependencies]\ncontract = { path = "<exact2>\\\\contract\\\\cli" }\n'));
    assert.throws(() => createApp(dir, { update: true }), /<exact2> left unresolved in web\/Cargo\.toml/);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 60_000);

// What the placeholder rewrite reads as TOML: strings in all four forms, and comments outside them.
test('update rewrites exactly the strings whose value begins <exact2>/, and names one written another way', async () => {
  const { exact2Placeholders } = await import('../game/new.mjs');
  const from = resolve(import.meta.dir, '../apps/caltrain/web'), to = (path) => `../../../${path}`;
  const cases = [
    // Text that only mentions the token: inside a string of the other quote, or later in a string.
    [`description = 'Compiler example: "<exact2>/contract/cli"'`, `description = 'Compiler example: "<exact2>/contract/cli"'`, false],
    [`description = "sample is <exact2>/apps/sparkline"`, `description = "sample is <exact2>/apps/sparkline"`, false],
    // A # inside an earlier string is no comment; one outside every string is.
    [`contract = { git = "ssh://host/exact.git#main", path = "<exact2>/contract/cli" }`, `contract = { git = "ssh://host/exact.git#main", path = "${to('contract/cli')}" }`, false],
    [`# contract = { path = "<exact2>/contract/cli" }`, `# contract = { path = "<exact2>/contract/cli" }`, false],
    // An escaped quote before the token ends no string.
    [String.raw`contract = { description = "a \"quoted\" word", path = "<exact2>/contract/cli" }`, String.raw`contract = { description = "a \"quoted\" word", path = "` + to('contract/cli') + '" }', false],
    // Both multiline forms, and a literal string, keep their quotes.
    [`runner = { path = """<exact2>/runner""" }`, `runner = { path = """${to('runner')}""" }`, false],
    [`plan = { path = '''<exact2>/plan''' }`, `plan = { path = '''${to('plan')}''' }`, false],
    [`types = { path = '<exact2>/contract/types' }`, `types = { path = '${to('contract/types')}' }`, false],
    // The token written another way is not rewritten, and said.
    [String.raw`contract = { path = "<exact2>\\contract\\cli" }`, String.raw`contract = { path = "<exact2>\\contract\\cli" }`, true],
    // A multiline basic string's line-ending backslash: its value is the token's, which is not decoded here, so it is said.
    ['contract = { path = """\\\n<exact2>/contract/cli""" }', 'contract = { path = """\\\n<exact2>/contract/cli""" }', true],
  ];
  for (const [text, expected, unresolved] of cases) assert.deepEqual(exact2Placeholders(text, from), { text: expected, unresolved }, text);
});

test('update notes a lock it could not check offline, and does not call the workspace unloadable', async () => {
  const { createApp } = await import('../game/new.mjs');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-app-offline-'));
  const dir = resolve(scratch, 'app'), source = resolve(scratch, 'source'), home = process.env.CARGO_HOME;
  const run = (cwd, cmd, args, env = process.env) => { const r = spawnSync(cmd, args, { cwd, env, encoding: 'utf8' }); assert.equal(r.status, 0, r.stderr); };
  try {
    // A git dependency whose source is gone, under a Cargo home that never fetched it: what a cold cache offline meets.
    mkdirSync(resolve(source, 'src'), { recursive: true });
    writeFileSync(resolve(source, 'Cargo.toml'), '[package]\nname = "local-source"\nversion = "0.1.0"\nedition = "2021"\n');
    writeFileSync(resolve(source, 'src/lib.rs'), '');
    run(source, 'git', ['init', '-q']); run(source, 'git', ['add', '.']);
    run(source, 'git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.test', 'commit', '-qm', 'source']);
    mkdirSync(resolve(dir, 'web/src'), { recursive: true }); pinned(dir);
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n');
    writeFileSync(resolve(dir, 'Cargo.toml'), '[workspace]\nmembers = ["web"]\nresolver = "2"\n');
    writeFileSync(resolve(dir, 'web/Cargo.toml'), `[package]\nname = "offline-app-web"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nlocal-source = { git = "file://${source}" }\n`);
    writeFileSync(resolve(dir, 'web/src/lib.rs'), '');
    run(dir, 'cargo', ['generate-lockfile'], { ...process.env, CARGO_HOME: resolve(scratch, 'warm') });
    rmSync(source, { recursive: true, force: true });
    process.env.CARGO_HOME = resolve(scratch, 'cold');
    assert.match(createApp(dir, { update: true }), /^Updated [^]*\n\S*Cargo\.lock: could not check lock: [^]*failed to load source/);
  } finally {
    if (home === undefined) delete process.env.CARGO_HOME; else process.env.CARGO_HOME = home;
    rmSync(scratch, { recursive: true, force: true });
  }
}, 60_000);

test('a gpu/ crate that is not the app\'s GPU module is named, not skipped', async () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-app-gpu-name-'))), dir = resolve(scratch, 'field-log'), was = process.env.EXACT_APP_DIR;
  try {
    for (const crate of ['gpu', 'data']) mkdirSync(resolve(dir, crate, 'src'), { recursive: true });
    pinned(dir);
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Field Log', app: { id: 'test.field-log', name: 'Field Log' } }));
    for (const crate of ['gpu', 'data']) writeFileSync(resolve(dir, crate, 'src/lib.rs'), '');
    writeFileSync(resolve(dir, 'data/Cargo.toml'), '[package]\nname = "field-log-data"\nversion = "0.1.0"\nedition = "2021"\n');
    const { patchLines } = await import('./app.mjs');
    const app = (name, members) => {
      writeFileSync(resolve(dir, 'Cargo.toml'), `[workspace]\nmembers = ${JSON.stringify(members)}\nresolver = "2"\n\n[patch.crates-io]\n${patchLines(dir).join('\n')}\n`);
      writeFileSync(resolve(dir, 'gpu/Cargo.toml'), `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n`);
      rmSync(resolve(dir, 'Cargo.lock'), { force: true }); rmSync(resolve(dir, '.exact'), { recursive: true, force: true });
      spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: dir });
      process.env.EXACT_APP_DIR = dir;
      return resolveApp();
    };
    assert.equal(app('field-log-gpu', ['data', 'gpu']).hasGpu, true);
    assert.throws(() => app('my-app-gpu', ['data', 'gpu']).hasGpu, /gpu\/Cargo\.toml names the package my-app-gpu; the app's GPU module must be named field-log-gpu$/);
    assert.throws(() => app('field-log-gpu', ['data']).hasGpu, /gpu\/Cargo\.toml \(field-log-gpu\) is not a member of the app's Cargo workspace: add "gpu" to its members$/);
  } finally {
    if (was === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = was;
    rmSync(scratch, { recursive: true, force: true });
  }
}, 60_000);

// LLP 1133.001 D9: what is installed is what bun.lock says. A registry package installed at another
// version, or missing, is named; a workspace's, a platform's own, and a nested install are not compared.
test('a node_modules that differs from bun.lock is named by package', async () => {
  const { lockMismatches } = await import('./filesystem.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-lock-'));
  try {
    writeFileSync(resolve(dir, 'bun.lock'), `{
  "lockfileVersion": 1,
  "packages": {
    "@exact/reading": ["@exact/reading@workspace:packages/reading"],
    "left": ["left@1.2.3", "", {}, "sha512-x"],
    "gone": ["gone@2.0.0", "", {}, "sha512-y"],
    "same": ["same@1.0.0", "", {}, "sha512-z"],
    "tool-linux-x64": ["tool-linux-x64@1.0.0", "", { "os": "linux", "cpu": "x64" }, "sha512-w"],
    "same/nested": ["nested@9.9.9", "", {}, "sha512-v"],
  },
}
`);
    for (const [name, version] of [['left', '1.0.0'], ['same', '1.0.0']]) {
      mkdirSync(resolve(dir, 'node_modules', name), { recursive: true });
      writeFileSync(resolve(dir, 'node_modules', name, 'package.json'), JSON.stringify({ name, version }));
    }
    assert.deepEqual(lockMismatches(dir), ['left 1.0.0, bun.lock 1.2.3', 'gone missing, bun.lock 2.0.0']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// `rustup-init --default-toolchain none`, and a command started in an app's directory, which pins nothing.
test('the host target is the pinned toolchain\'s when rustup has no default', () => {
  const root = resolve(import.meta.dir, '..');
  const sysroot = spawnSync('rustc', ['--print', 'sysroot'], { cwd: root, encoding: 'utf8' }).stdout.trim();
  if (basename(dirname(sysroot)) !== 'toolchains') return; // not rustup's
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-no-default-')));
  try {
    mkdirSync(resolve(dir, 'rustup/toolchains'), { recursive: true });
    symlinkSync(sysroot, resolve(dir, 'rustup/toolchains', basename(sysroot)));
    const env = { ...process.env, RUSTUP_HOME: resolve(dir, 'rustup'), RUSTUP_AUTO_INSTALL: '0' };
    delete env.RUSTUP_TOOLCHAIN;
    assert.notEqual(spawnSync('rustc', ['-vV'], { cwd: dir, env }).status, 0, 'rustup has no default here');
    const code = `console.log((await import(${JSON.stringify(resolve(root, 'scripts/app.mjs'))})).bakeTarget('linux'))`;
    const run = spawnSync(process.execPath, ['-e', code], { cwd: dir, env, encoding: 'utf8' });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stdout.trim(), bakeTarget('linux'));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('the build says why and how to resume when rustc names no host target', () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'exact-rustc-'));
  const fail = script => {
    writeFileSync(resolve(directory, 'rustc'), `#!/bin/sh\n${script}\n`, { mode: 0o755 });
    const code = `const { bakeTarget } = await import(${JSON.stringify(resolve(import.meta.dir, 'app.mjs'))});\nprocess.env.PATH = ${JSON.stringify(directory)};\nbakeTarget('linux');`;
    const run = spawnSync(process.execPath, ['-e', code], { encoding: 'utf8' });
    assert.equal(run.status, 1, run.stderr);
    return run.stderr;
  };
  try {
    const silent = fail('exit 9');
    assert.match(silent, /`rustc -vV` failed \(exit 9\)/);
    assert.ok(silent.includes(PINNED_RUST ? `run \`rustup toolchain install ${PINNED_RUST}\`` : 'install a Rust toolchain'), silent);
    assert.match(fail('echo "rustc 1.0.0"'), /`rustc -vV` printed no `host:` line.*the `rustc` on PATH is rustup's/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

// LLP 1153.003 D1: an app's own crates join the workspace exact2 generates, which keeps writing every host crate.
test('a crate beside app.json joins the generated workspace by naming it, and one that names none is refused with the line to add', async () => {
  const { createApp } = await import('../game/new.mjs');
  const { ensureShells } = await import('./app-shells.mjs');
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-members-'))), dir = resolve(scratch, 'field-log'), was = process.env.EXACT_APP_DIR, target = process.env.CARGO_TARGET_DIR;
  const crate = (path, name, workspace, dependency = 'exact-runner') => {
    mkdirSync(resolve(dir, path, 'src'), { recursive: true });
    writeFileSync(resolve(dir, path, 'Cargo.toml'), `[package]\nname = "${name}"\nversion.workspace = true\nedition.workspace = true\n${workspace ? `workspace = "${workspace}"\n` : ''}publish = false\n\n[dependencies]\n${dependency}.workspace = true\n`);
    writeFileSync(resolve(dir, path, 'src/lib.rs'), '');
  };
  try {
    createApp(dir, { with: ['ios'] });
    delete process.env.CARGO_TARGET_DIR;
    process.env.EXACT_APP_DIR = dir;
    crate('data', 'field-log-data', null);
    assert.throws(() => resolveApp(), /data\/Cargo\.toml: a crate beside app\.json joins the app's generated workspace\. Add, under \[package\], workspace = "\.\.\/\.exact\/shells" \(a GPU crate: workspace = "\.\.\/\.exact\/gpu"\)/);
    assert.throws(() => ensureShells(dir), /a crate beside app\.json joins the app's generated workspace/);
    crate('data', 'field-log-data', '../.exact/shells');
    // A host crate links an app's crate only as the data crate rust.data names (LLP 1153.003 D2), so a member with
    // none named is refused with the line to add.
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    assert.throws(() => ensureShells(dir), /field-log-data beside app\.json joins the app's generated workspace, and app\.json names no rust\.data: [^]*\n  "rust": \{ "data": \{ "crate": "field-log-data", "type": "field_log_data::<Type>", "source": "field_log_data::<Type>::new\(…\)" \} \}\n/);
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...manifest, rust: { data: { crate: 'field-log-data', type: 'field_log_data::FieldLog' } } }));
    const { workspace } = ensureShells(dir);
    const shell = readFileSync(resolve(workspace, 'Cargo.toml'), 'utf8');
    assert.match(shell, /^members = \["web", "apple", "module", "\.\.\/\.\.\/data"\]$/m);
    assert.match(shell, /^exact-runner = \{ path = "[^"]*\/runner" \}$/m);
    const loads = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: workspace, encoding: 'utf8' });
    assert.equal(loads.status, 0, loads.stderr);
    assert.deepEqual(JSON.parse(loads.stdout).packages.map(p => p.name).sort(), ['field-log-apple', 'field-log-data', 'field-log-module', 'field-log-web']);
    const app = resolveApp();
    assert.deepEqual([app.generated, app.workspace, app.target], [true, workspace, resolve(dir, 'target')]);
    // Cargo run in the member finds exact2's toolchain and the app's target directory, written beside app.json (D1.5).
    const toolchain = spawnSync('rustup', ['show', 'active-toolchain'], { cwd: resolve(dir, 'data'), encoding: 'utf8' });
    if (toolchain.status === 0) assert.match(toolchain.stdout, new RegExp(`^${readFileSync(resolve(import.meta.dir, '../rust-toolchain.toml'), 'utf8').match(/channel = "([^"]+)"/)[1].replaceAll('.', '\\.')}`));
    const inMember = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: resolve(dir, 'data'), encoding: 'utf8' });
    assert.equal(inMember.status, 0, inMember.stderr);
    assert.equal(JSON.parse(inMember.stdout).target_directory, resolve(dir, 'target'));
    assert.match(readFileSync(resolve(dir, '.gitignore'), 'utf8'), /^\/rust-toolchain\.toml\n\/\.cargo\/config\.toml$/m);
    // A crate that takes a generated host crate's name is the authored workspace's (D3).
    crate('helper', 'field-log-web', '../.exact/shells');
    assert.throws(() => ensureShells(dir), /field-log-web is a host crate's name, and exact2 writes the app's host crates under \.exact\/shells\//);
    rmSync(resolve(dir, 'helper'), { recursive: true });
    // A GPU crate alone joins the GPU workspace, and the host crates stay generated (D1.2).
    rmSync(resolve(dir, 'data'), { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
    crate('gpu', 'field-log-gpu', '../.exact/gpu', 'exact-gpu');
    assert.deepEqual([resolveApp().generated, resolveApp().workspace], [true, workspace]);
    assert.match(readFileSync(resolve(ensureShells(dir).workspace, 'Cargo.toml'), 'utf8'), /^members = \["web", "apple"\]$/m);
    // A crate naming another workspace beside one that joins a generated one is refused.
    crate('data', 'field-log-data', '../elsewhere');
    mkdirSync(resolve(dir, 'elsewhere'));
    assert.throws(() => resolveApp(), /app crates declare different Cargo workspaces/);
  } finally {
    if (was === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = was;
    if (target !== undefined) process.env.CARGO_TARGET_DIR = target;
    rmSync(scratch, { recursive: true, force: true });
  }
}, 120_000);

test('host data and GPU lock refreshes preserve compatible captured versions', async () => {
  const { GPU_LOCK, lockShells, staleLock } = await import('./app-shells.mjs');
  const dir = pinned(realpathSync(mkdtempSync(resolve(import.meta.dir, '../target/lock-refresh-'))));
  try {
    for (const [kind, file] of [['data', 'Cargo.lock'], ['gpu', GPU_LOCK]]) {
      const workspace = resolve(dir, kind), member = resolve(workspace, 'member'), captured = resolve(dir, file);
      mkdirSync(resolve(member, 'src'), { recursive: true });
      writeFileSync(resolve(member, 'src/lib.rs'), '');
      writeFileSync(resolve(workspace, 'Cargo.toml'), '[workspace]\nmembers = ["member"]\nresolver = "2"\n');
      const manifest = (json, width) => writeFileSync(resolve(member, 'Cargo.toml'), `[package]\nname = "fixture-${kind}"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nserde_json = "${json}"\nunicode-width = "${width}"\n`);
      manifest('=1.0.154', '=0.2.2');
      lockShells(dir, workspace, captured);
      // Keep a captured lock even when the SDK eventually holds these versions too.
      writeFileSync(captured, readFileSync(resolve(workspace, 'Cargo.lock')));
      const before = readFileSync(captured, 'utf8');
      assert.match(lockShells(dir, workspace, captured), /current; no changes$/);
      // Both the SDK's serde_json 1.0.151 and unicode-width 0.1.14 fit these ranges.
      manifest('1', '>=0.1.14, <=0.2.2');
      assert.match(lockShells(dir, workspace, captured), /current; no changes$/);
      assert.equal(readFileSync(captured, 'utf8'), before);
      assert.equal(readFileSync(resolve(workspace, 'Cargo.lock'), 'utf8'), before);
      assert.match(lockShells(dir, workspace, captured), /current; no changes$/);
      // A changed requirement still lets Cargo update a captured version.
      manifest('=1.0.151', '=0.1.14');
      assert.match(lockShells(dir, workspace, captured), /resolved\n/);
      const packages = Bun.TOML.parse(readFileSync(captured, 'utf8')).package;
      for (const [name, version] of [['serde_json', '1.0.151'], ['unicode-width', '0.1.14']])
        assert.deepEqual(packages.filter(p => p.name === name).map(p => p.version), [version]);
      const older = readFileSync(captured, 'utf8');
      manifest('1', '>=0.1.14, <=0.2.2');
      assert.match(lockShells(dir, workspace, captured), /current; no changes$/);
      assert.equal(readFileSync(captured, 'utf8'), older);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
  assert.ok(staleLock('error: cannot update the lock file /app/Cargo.lock because --locked was passed to prevent this'));
  assert.ok(staleLock('error: the lock file /app/Cargo.lock needs to be updated but --locked was passed to prevent this'));
  assert.ok(!staleLock('error: failed to download `rustix v1.1.5`\n\nCaused by:\n  attempting to make an HTTP request, but --offline was specified'));
}, 120_000);

// LLP 1153.003 D1.4, a game's rule (LLP 1046.011 D2): an app captures a lock only when its crates leave exact2's.
test('a generated workspace captures no lock until a dependency leaves exact2\'s, and keeps a captured one across a rewrite', async () => {
  const { createApp } = await import('../game/new.mjs');
  const { ensureShells, settleLock, lockShells } = await import('./app-shells.mjs');
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-member-lock-'))), dir = resolve(scratch, 'field-log');
  const sdk = readFileSync(resolve(import.meta.dir, '../Cargo.lock'), 'utf8');
  // A version this machine's Cargo cache holds and exact2's lock does not: itoa beside exact2's.
  const held = /name = "itoa"\nversion = "([^"]+)"/.exec(sdk)[1];
  const cache = resolve(process.env.CARGO_HOME ?? resolve(homedir(), '.cargo'), 'registry/cache');
  const other = (existsSync(cache) ? readdirSync(cache).flatMap(index => readdirSync(resolve(cache, index))) : [])
    .map(file => /^itoa-(1\.\d+\.\d+)\.crate$/.exec(file)?.[1]).find(version => version && version !== held);
  const data = (deps = '') => writeFileSync(resolve(dir, 'data/Cargo.toml'), `[package]\nname = "field-log-data"\nversion.workspace = true\nedition.workspace = true\nworkspace = "../.exact/shells"\npublish = false\n\n[dependencies]\nexact-runner.workspace = true\n${deps}`);
  try {
    createApp(dir);
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...manifest, rust: { data: { crate: 'field-log-data', type: 'field_log_data::FieldLog' } } }));
    mkdirSync(resolve(dir, 'data/src'), { recursive: true });
    writeFileSync(resolve(dir, 'data/src/lib.rs'), '');
    data();
    const { workspace } = ensureShells(dir);
    settleLock(dir, workspace);
    assert.match(lockShells(dir, workspace), /resolves against exact2's lock; the app's crates there add nothing, so it commits no Cargo\.lock of its own/);
    assert.equal(existsSync(resolve(dir, 'Cargo.lock')), false);
    if (other) {
      data(`itoa = "=${other}"\n`);
      assert.throws(() => settleLock(dir, workspace), new RegExp(`the app's crates need versions exact2's lock does not hold \\(itoa ${other.replaceAll('.', '\\.')} [^)]*\\)\\. An app whose crates add dependencies commits a lock of its own: \`bun exact\\.mjs lock\``));
      assert.match(lockShells(dir, workspace), /Cargo\.lock: captured \(\d+ packages\); commit it/);
      const captured = readFileSync(resolve(dir, 'Cargo.lock'), 'utf8');
      assert.match(captured, new RegExp(`name = "itoa"\nversion = "${other.replaceAll('.', '\\.')}"`));
      settleLock(dir, workspace);
      // A rewrite of the shell (here, its stamp as a moved checkout leaves it) keeps the captured lock.
      writeFileSync(resolve(workspace, '.stamp.json'), '{}\n');
      assert.equal(ensureShells(dir).wrote, true);
      assert.equal(readFileSync(resolve(workspace, 'Cargo.lock'), 'utf8'), captured);
      // A dependency changed since the capture is the lock's to settle, never a build's.
      data(`itoa = "=${held}"\n`);
      assert.throws(() => settleLock(dir, workspace), /Cargo\.lock does not match the app's crates [^]*`bun exact\.mjs lock` in /);
      assert.match(lockShells(dir, workspace), new RegExp(`Cargo\\.lock: resolved\nitoa ${other.replaceAll('.', '\\.')} → ${held.replaceAll('.', '\\.')}`));
    }
    data('rustls-platform-verifier = "=0.7.1"\n');
    assert.match(lockShells(dir, workspace), /Cargo\.lock: (?:resolved|captured)|resolves against exact2's lock/);
    const locked = Bun.TOML.parse(readFileSync(resolve(workspace, 'Cargo.lock'), 'utf8')).package;
    assert.deepEqual(locked.filter(p => p.name === 'rustls-platform-verifier').map(p => p.version), ['0.7.1']);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 180_000);

// LLP 1153.003 D2: a data crate the manifest names is linked by generated host crates whose build script is one line,
// with no exact-js (no Hermes) without app.ts, and composed with the TypeScript module beside it.
test('rust.data generates host crates over the data crate, composed with app.ts when there is one', async () => {
  const { createApp } = await import('../game/new.mjs');
  const { ensureShells } = await import('./app-shells.mjs');
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-rust-data-'))), dir = resolve(scratch, 'pins');
  try {
    createApp(dir, { with: ['ios', 'linux', 'android'] });
    mkdirSync(resolve(dir, 'data/src'), { recursive: true });
    writeFileSync(resolve(dir, 'data/Cargo.toml'), '[package]\nname = "pins-data"\nversion.workspace = true\nedition.workspace = true\nworkspace = "../.exact/shells"\npublish = false\n\n[dependencies]\nexact-runner.workspace = true\n');
    writeFileSync(resolve(dir, 'data/src/lib.rs'), '');
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    const declare = data => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...manifest, rust: { data } }, null, 2));
    declare({ crate: 'other-data', type: 'other_data::Pins' });
    assert.throws(() => ensureShells(dir), /rust\.data\.crate is other-data, which is not a crate beside app\.json that joins the app's workspace/);
    declare({ crate: 'pins-data', type: 'pins_data::Pins', source: 'pins_data::Pins::new(3)' });
    rmSync(resolve(dir, 'app.ts'));
    const { workspace } = ensureShells(dir);
    const read = file => readFileSync(resolve(workspace, file), 'utf8');
    assert.match(read('Cargo.toml'), /^members = \["web", "apple", "linux", "android", "module", "\.\.\/\.\.\/data"\]$/m);
    for (const kind of ['web', 'apple', 'linux', 'android']) {
      assert.equal(read(`${kind}/build.rs`), `fn main() {\n    let source = pins_data::Pins::new(3);\n    let host_work = exact_bake::hands_out_work!(source);\n    exact_bake::host::app("${kind}", "../../..", source, host_work);\n}\n`);
      assert.match(read(`${kind}/Cargo.toml`), /^pins-data = \{ path = "\.\.\/\.\.\/\.\.\/data" \}$/m);
      assert.doesNotMatch(read(`${kind}/Cargo.toml`), /exact-js/);
    }
    // The web's module exports the same source; the dev driver bakes with `default()`, so a source given is none.
    assert.match(read('module/src/lib.rs'), /^exact_logic_abi::export!\(pins_data::Pins, pins_data::Pins::new\(3\)\);$/m);
    assert.equal(existsSync(resolve(workspace, 'web/src/bin/pins-dev.rs')), false);
    // The Android library links the update store only at deploy.store.android "A" (LLP 1030.006 D3).
    assert.doesNotMatch(read('android/Cargo.toml'), /exact-painted-update/);
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')), deploy: { ...manifest.deploy, store: { ...manifest.deploy?.store, android: 'A' } } }, null, 2));
    ensureShells(dir);
    assert.match(read('android/Cargo.toml'), /^exact-painted-update = \{ path = "[^"]*host\/painted-update" \}$/m);
    assert.doesNotMatch(read('linux/Cargo.toml'), /exact-painted-update/);
    declare({ crate: 'pins-data', type: 'pins_data::Pins', source: 'pins_data::Pins::new(3)' });
    ensureShells(dir);
    const loads =spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: workspace, encoding: 'utf8' });
    assert.equal(loads.status, 0, loads.stderr);
    // With app.ts beside it, the crates compose the two: Rust answers its RUST_SOURCES, app.ts the rest.
    writeFileSync(resolve(dir, 'app.ts'), 'export const grants = "";\n');
    assert.equal(ensureShells(dir).wrote, true);
    assert.match(read('web/build.rs'), /exact_js_bake::build_beside\([^]*&pins_data::Pins::new\(3\),\n\s*exact_js_bake::hands_out_work!\(pins_data::Pins::new\(3\)\),\n\s*pins_data::RUST_SOURCES,/);
    assert.match(read('apple/src/lib.rs'), /exact_data::Mixed::new\([^]*TYPESCRIPT_SOURCES,\n\s*pins_data::RUST_SOURCES,/);
    // A data crate that names no app takes the module's identity, which `Mixed` requires both sides to share.
    for (const file of ['web/build.rs', 'apple/src/lib.rs']) assert.match(read(file), /Placed::new\(pins_data::Pins::new\(3\), [^)]+\)\.named\(&app\)/);
    assert.match(read('linux/Cargo.toml'), /^exact-js = /m);
    // Without rust.data, the module member goes with it.
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest, null, 2));
    rmSync(resolve(dir, 'data'), { recursive: true });
    ensureShells(dir);
    assert.equal(existsSync(resolve(workspace, 'module')), false);
    assert.match(read('web/build.rs'), /exact_js_bake::build\(/);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 180_000);

// LLP 1153.003 D5.1: a GPU module links nothing of the host's, so its crates resolve in a generated workspace of their
// own, with a lock of their own captured by the host's rule.
test('a GPU crate builds in .exact/gpu, apart from the host\'s graph, and captures Cargo.gpu.lock only when it leaves exact2\'s', async () => {
  const { createApp } = await import('../game/new.mjs');
  const { GPU_LOCK, ensureGpuShells, ensureShells, lockShells } = await import('./app-shells.mjs');
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-gpu-workspace-'))), dir = resolve(scratch, 'field-log'), was = process.env.EXACT_APP_DIR, target = process.env.CARGO_TARGET_DIR;
  const sdk = readFileSync(resolve(import.meta.dir, '../Cargo.lock'), 'utf8');
  const held = /name = "itoa"\nversion = "([^"]+)"/.exec(sdk)[1];
  const cache = resolve(process.env.CARGO_HOME ?? resolve(homedir(), '.cargo'), 'registry/cache');
  const other = (existsSync(cache) ? readdirSync(cache).flatMap(index => readdirSync(resolve(cache, index))) : [])
    .map(file => /^itoa-(1\.\d+\.\d+)\.crate$/.exec(file)?.[1]).find(version => version && version !== held);
  // The recipe's manifest (docs/recipes/gpu-module.md).
  const gpu = (deps = '', workspace = '../.exact/gpu') => writeFileSync(resolve(dir, 'gpu/Cargo.toml'), `[package]\nname = "field-log-gpu"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\npublish = false\nworkspace = "${workspace}"\n\n[lib]\ncrate-type = ["cdylib", "rlib"]\n\n[dependencies]\nexact-gpu.workspace = true\n${deps}\n[target.'cfg(target_arch = "wasm32")'.dependencies]\nwasm-bindgen.workspace = true\nwasm-bindgen-futures.workspace = true\nweb-sys.workspace = true\n`);
  try {
    createApp(dir, { with: ['macos'] });
    delete process.env.CARGO_TARGET_DIR;
    process.env.EXACT_APP_DIR = dir;
    mkdirSync(resolve(dir, 'gpu/src'), { recursive: true });
    writeFileSync(resolve(dir, 'gpu/src/lib.rs'), '');
    // A GPU crate that names the host's workspace is pointed at its own.
    gpu('', '../.exact/shells');
    assert.throws(() => ensureShells(dir), /gpu\/Cargo\.toml: field-log-gpu is a GPU module, which resolves apart from the host's crates: set package\.workspace = "\.\.\/\.exact\/gpu"/);
    gpu();
    const app = resolveApp(), workspace = app.workspace, own = resolve(dir, '.exact/gpu');
    assert.equal(app.gpuWorkspace, own);
    assert.equal(app.hasGpu, true);
    const manifest = readFileSync(resolve(own, 'Cargo.toml'), 'utf8');
    assert.match(manifest, /^members = \["\.\.\/\.\.\/gpu"\]$/m);
    assert.match(manifest, /^wgpu-hal = \{ path = "[^"]*\/vendor\/wgpu-hal" \}$/m);
    assert.match(manifest, /^wasm-bindgen = "=\d+\.\d+\.\d+"$/m);
    const names = at => JSON.parse(spawnSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], { cwd: at, encoding: 'utf8', maxBuffer: 1 << 28 }).stdout).packages.map(p => p.name);
    assert.ok(names(own).includes('field-log-gpu'));
    assert.ok(names(own).includes('exact-gpu'));
    // The host's graph holds neither the GPU crate nor wgpu.
    assert.equal(names(workspace).some(name => name === 'field-log-gpu' || name === 'exact-gpu' || name === 'wgpu'), false);
    assert.match(lockShells(dir, own, resolve(dir, GPU_LOCK)), /resolves against exact2's lock; the app's crates there add nothing, so it commits no Cargo\.gpu\.lock of its own/);
    assert.equal(existsSync(resolve(dir, GPU_LOCK)), false);
    // mln-rs 0.0.11 needs these versions in the same GPU graph (#426).
    gpu('rustls-platform-verifier = "=0.7.1"\nwasm-bindgen = "=0.2.130"\n');
    assert.match(lockShells(dir, own, resolve(dir, GPU_LOCK)), /resolves against exact2's lock|Cargo\.gpu\.lock: captured/);
    const locked = Bun.TOML.parse(readFileSync(resolve(own, 'Cargo.lock'), 'utf8')).package;
    for (const [name, version] of [['rustls-platform-verifier', '0.7.1'], ['wasm-bindgen', '0.2.130']]) {
      assert.deepEqual(locked.filter(p => p.name === name).map(p => p.version), [version]);
    }
    if (other) {
      gpu(`itoa = "=${other}"\n`);
      assert.match(lockShells(dir, own, resolve(dir, GPU_LOCK)), /Cargo\.gpu\.lock: captured \(\d+ packages\); commit it/);
      assert.match(readFileSync(resolve(dir, GPU_LOCK), 'utf8'), new RegExp(`name = "itoa"\nversion = "${other.replaceAll('.', '\\.')}"`));
      // The host's lock is untouched: it captures nothing of the GPU crate's.
      assert.equal(existsSync(resolve(dir, 'Cargo.lock')), false);
      assert.equal(readFileSync(resolve(workspace, 'Cargo.lock'), 'utf8').includes(`name = "itoa"\nversion = "${other}"`), false);
    }
    // Without a GPU crate the workspace goes.
    rmSync(resolve(dir, 'gpu'), { recursive: true });
    assert.equal(ensureGpuShells(dir), null);
    assert.equal(existsSync(own), false);
  } finally {
    if (was === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = was;
    if (target !== undefined) process.env.CARGO_TARGET_DIR = target;
    rmSync(scratch, { recursive: true, force: true });
  }
}, 300_000);
