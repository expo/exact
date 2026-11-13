// Builds and bakes (scripts/app.mjs `buildBake`, `bakeTarget`): artifacts and their evidence, receipts, GPU modules and shader
// packs, the build environment and what makes a build stale.
import { test } from 'bun:test';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import { cargoBakeInputs } from './cargo-dep-info.mjs';
import { cargoDiagnosticFilter, cargoDiagnosticsCommand } from './cargo-check.mjs';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { basename, delimiter, dirname, relative, resolve } from 'node:path';
import { classifyArtifacts, buildBake, bakeOutput, bakeTarget, pendingBuildInputs } from './app.mjs';
import { pinned } from './test-workspace.mjs';
import { fileURLToPath } from 'node:url';
import { copyStaticTreeIfPresent, syncStaticTree } from '../host/web/serve.mjs';

test.each([
  ['host/web-js/build.mjs', "// Launch modules' web services", '\nif (devReload)', '', ['assets', 'deck'], false],
  ['host/windows/build.mjs', "  for (const name of ['assets','deck'])", '\n  copyShaders', '', ['assets', 'deck'], true],
  ['host/apple/watch.mjs', '    for (const entry of entries) copyFileSync', '\n    const appInfo', '', ['assets', 'deck'], false],
  ['host/apple-painted/build.mjs', "  writeFileSync(resolve(bundle, 'Contents/Info.plist')", "\n  run('codesign'", 'Contents/Resources', ['assets'], true],
  ['scripts/agent-android.mjs', "  copyStaticTreeIfPresent(resolve(app.dir, 'assets')", "\n  // The modules' shader text", 'assets/app', ['assets'], false],
])('%s packages static files without Finder metadata', async (file, start, end, prefix, folders, reused) => {
  const root = mkdtempSync(resolve(import.meta.dir, '../target/static-package-'));
  const appDir = resolve(root, 'app'), output = resolve(root, 'output');
  try {
    mkdirSync(resolve(output, 'Contents/Resources'), { recursive: true });
    for (const folder of folders) {
      const tree = resolve(appDir, folder);
      mkdirSync(resolve(tree, 'nested'), { recursive: true });
      for (const name of ['.DS_Store', 'nested/.DS_Store', 'nested/.config', 'image.png']) writeFileSync(resolve(tree, name), name);
      if (reused) {
        mkdirSync(resolve(output, prefix, folder, 'nested'), { recursive: true });
        for (const name of ['.DS_Store', 'nested/.DS_Store']) writeFileSync(resolve(output, prefix, folder, name), 'old metadata');
      }
    }
    // Run the packaging step itself; the native toolchains are outside this test.
    const source = readFileSync(resolve(import.meta.dir, '..', file), 'utf8');
    const from = source.indexOf(start), to = source.indexOf(end, from);
    assert.ok(from >= 0 && to > from, `${file}: packaging step`);
    const bindings = { cpSync, copyFileSync, existsSync, mkdirSync, writeFileSync, resolve, dirname, copyStaticTreeIfPresent, syncStaticTree,
      app: { dir: appDir }, appDir, output, out: output, bundle: output, staged: output,
      launchWeb: [], entries: [], live: false, gpu: null, ts: false, macInfoPlist: () => '' };
    await new (async function () {}).constructor(...Object.keys(bindings), source.slice(from, to))(...Object.values(bindings));
    for (const folder of folders) {
      const tree = resolve(output, prefix, folder);
      assert.equal(existsSync(resolve(tree, '.DS_Store')), false);
      assert.equal(existsSync(resolve(tree, 'nested/.DS_Store')), false);
      for (const name of ['nested/.config', 'image.png']) assert.equal(readFileSync(resolve(tree, name), 'utf8'), name);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test.each(['source additions', 'package watches', 'build-script helpers', 'new Rust sources'])('Windows bake receipts track %s by content', async mode => {
  const { packagedBuildChanges } = await import('./agent-launch.mjs');
  const { windowsBuild } = await import('./agent-build.mjs');
  mkdirSync(resolve(import.meta.dir, '../target'), { recursive: true });
  const dir = pinned(realpathSync(mkdtempSync(resolve(import.meta.dir, '../target/windows-inputs-'))));
  const write = (path, bytes) => { mkdirSync(dirname(resolve(dir, path)), { recursive: true }); writeFileSync(resolve(dir, path), bytes); };
  const old = new Date('2000-01-01T00:00:00Z'), target = bakeTarget('windows'), id = 'com.exact.inputs';
  let blocked;
  try {
    write('Cargo.toml', '[workspace]\nmembers=["windows","data"]\nresolver="2"\n');
    write('windows/Cargo.toml', '[package]\nname="inputs-windows"\nversion="0.1.0"\nedition="2021"\n[dependencies]\ninputs-data={path="../data"}\n');
    write('data/Cargo.toml', '[package]\nname="inputs-data"\nversion="0.1.0"\nedition="2021"\n');
    write('data/src/lib.rs', 'pub fn value() -> u8 { option_env!("WINDOWS_INPUTS_VALUE").unwrap_or("1").parse().unwrap() }\n');
    write('windows/src/main.rs', 'fn main() { println!("{}", inputs_data::value()); }\n');
    const watched = ['app.contract', 'app.ts', 'imported.ts', 'value/index.ts', 'child.contract', 'optional.txt'];
    write('app.contract', 'use Child from "./child.contract"\ncomponent App\n  view\n    Child()\n    text "before"\n');
    write('child.contract', 'component Child\n  text "before"\n');
    write('app.ts', 'export { value } from "./imported";\nexport { other } from "./value";\n');
    write('imported.ts', 'export const value = 1;\n');
    write('value/index.ts', 'export const other = 1;\n');
    if (mode === 'package watches') write('assets/icon.txt', 'icon');
    if (mode === 'source additions' && process.platform !== 'win32') {
      symlinkSync('.cycle-b.md', resolve(dir, '.cycle-a.md'));
      symlinkSync('.cycle-a.md', resolve(dir, '.cycle-b.md'));
    }
    if (mode === 'package watches') {
      const pkg = resolve(dir, 'node_modules/@fixture/ui');
      write('node_modules/@fixture/ui/package.json', '{"name":"@fixture/ui","exports":"./child.contract"}');
      write('node_modules/@fixture/ui/child.contract', 'component Child\n  text "package"\n');
      watched.push('node_modules/@fixture/ui');
      if (process.platform !== 'win32') {
        blocked = resolve(pkg, '.cache/0-private');
        write('node_modules/@fixture/ui/.cache/0-private', 'unused');
        chmodSync(blocked, 0);
        if (process.getuid?.() !== 0) assert.throws(() => readFileSync(blocked), {code:'EACCES'});
        symlinkSync(pkg, resolve(pkg, '.cache/cycle'), 'dir');
        mkdirSync(resolve(dir, 'target'), {recursive:true});
        symlinkSync(resolve(dir, 'target'), resolve(pkg, '.cache/output'), 'dir');
      }
    }
    write('windows/bake_helper.rs', 'pub fn value() -> u8 { 1 }\n');
    utimesSync(resolve(dir, 'windows/bake_helper.rs'), old, old);
    write('windows/build.rs', `mod bake_helper;
    fn main() {
      assert_eq!(bake_helper::value(), 1);
      println!("cargo:rerun-if-env-changed=EXACT_BAKE_INPUTS");
      ${watched.map(path => `println!("cargo:rerun-if-changed=../${path}");`).join('\n      ')}
      let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
      std::fs::write(out.join("compat.json"), r#"${JSON.stringify({target,inputs:{platform:'windows',app:id,store:{L:'A'},keys:[]}})}"#).unwrap();
      std::fs::write(out.join("artifacts.json"), r#"{"version":1,"artifacts":[],"sources":{}}"#).unwrap();
      let mut plan = std::fs::read("../app.contract").unwrap();
      plan.extend(std::fs::read("../value.ts").or_else(|_| std::fs::read("../value/index.ts")).unwrap());
      std::fs::write(out.join("app.plan"), plan).unwrap();
    }`);
    const paths = [...watched.filter(path => !['node_modules/@fixture/ui', 'optional.txt'].includes(path)), 'data/src/lib.rs', 'windows/src/main.rs', ...(mode === 'package watches' ? ['assets/icon.txt'] : [])];
    for (const path of paths) utimesSync(resolve(dir, path), old, old);
    const app = {dir, workspace:dir, target:resolve(dir, 'target'), name:'inputs', displayName:'Inputs', id,
      manifest:{app:{id,name:'Inputs'},rust:false}, crate:kind=>`inputs-${kind}`};
    // Compile a native fixture on this machine; packaging checks read bytes, so its PE suffix can be supplied here.
    const bake = () => buildBake(app, 'windows', target, {profile:'dev', env:{CARGO_NET_OFFLINE:'true',EXACT_UPDATE_TRUST:'development'}});
    const build = bake();
    const original = build.products.find(product => basename(product.path).startsWith('inputs-windows'));
    assert.ok(original);
    const binary = resolve(dir, 'dist-windows/Inputs.exe'), dll = resolve(dir, 'dist-windows/module.dll');
    write('dist-windows/Inputs.exe', readFileSync(original.path));
    write('dist-windows/module.dll', 'module');
    const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
    build.products = [{...original,path:resolve(dirname(original.path),'inputs-windows.exe')}, {path:dll,sha256:digest(dll)}];
    const receipt = resolve(bakeOutput(app), `windows-${target}.build.json`);
    writeFileSync(receipt, JSON.stringify(build));
    const fresh = () => {
      assert.deepEqual(packagedBuildChanges(receipt, dirname(binary), app), []);
      assert.deepEqual(windowsBuild(app), {bin:binary,changed:[]});
    };
    fresh();
    if (mode === 'build-script helpers') {
      const helper = resolve(dir, 'windows/bake_helper.rs'), before = readFileSync(helper);
      writeFileSync(helper, 'pub fn value() -> u8 { 2 }\n'); utimesSync(helper, old, old);
      assert.ok(packagedBuildChanges(receipt, dirname(binary), app).length, 'the agent must detect a build-script helper edit');
      assert.ok(windowsBuild(app).changed.length, 'build-first must detect a build-script helper edit');
      assert.ok(build.binary.inputs.some(file => file.path === helper), 'compiler dependencies are binary inputs');
      writeFileSync(helper, before); fresh();
      return;
    }
    if (mode === 'new Rust sources') {
      for (const path of ['data/build.rs', 'data/src/main.rs', 'data/src/bin/tool.rs']) {
        write(path, 'fn main() {}\n'); utimesSync(resolve(dir, path), old, old);
        assert.ok(packagedBuildChanges(receipt, dirname(binary), app).length, `${path}: the agent must detect a new Cargo target`);
        assert.ok(windowsBuild(app).changed.length, `${path}: build-first must detect a new Cargo target`);
        rmSync(resolve(dir, path)); fresh();
      }
      write('data/build.rs', 'fn main() { println!("cargo:rustc-env=WINDOWS_INPUTS_VALUE=2"); }\n');
      utimesSync(resolve(dir, 'data/build.rs'), old, old);
      const rebuilt = bake();
      assert.ok(rebuilt.binary.inputs.some(file => file.path === resolve(dir, 'data/build.rs')), 'the rebuilt receipt records the discovered build script');
      assert.deepEqual(pendingBuildInputs(rebuilt, {includeSources:true}), []);
      const run = spawnSync(original.path, [], {encoding:'utf8'});
      assert.equal(run.status, 0, run.stderr);
      assert.equal(run.stdout.trim(), '2');
      return;
    }
    for (const path of paths) {
      const file = resolve(dir, path), before = readFileSync(file);
      const edited = Buffer.from(before); edited[edited.length - 2] ^= 1;
      writeFileSync(file, edited); utimesSync(file, old, old);
      assert.ok(packagedBuildChanges(receipt, dirname(binary), app).length, `${path}: the agent must reject equal-mtime edits`);
      assert.ok(windowsBuild(app).changed.length, `${path}: build-first must reject equal-mtime edits`);
      writeFileSync(file, before);
      fresh(); // Identical bytes with a newer mtime are still fresh.
      utimesSync(file, old, old);
    }
    for (const path of ['value.ts', 'optional.txt', 'assets/new.txt', 'deck/new.txt', 'gpu/shaders/new.wgsl']) {
      const root = resolve(dir, path.split('/')[0]), hadRoot = existsSync(root);
      write(path, 'new');
      utimesSync(resolve(dir, path), old, old);
      assert.ok(packagedBuildChanges(receipt, dirname(binary), app).length, `${path}: new source input`);
      assert.ok(windowsBuild(app).changed.length, `${path}: build-first must detect a new source input`);
      rmSync(resolve(dir, path));
      if (path.includes('/new.') && !hadRoot) rmSync(root, {recursive:true});
      fresh();
    }
    rmSync(resolve(dir, 'child.contract'));
    assert.ok(packagedBuildChanges(receipt, dirname(binary), app).length, 'a missing imported Contract is stale');
    write('child.contract', 'component Child\n  text "before"\n'); fresh();
    write('notes.md', 'unused'); fresh();
    write('target/build-output.json', 'output'); fresh();
    if (blocked) {
      chmodSync(blocked, 0o600);
      if (process.getuid?.() !== 0) assert.ok(packagedBuildChanges(receipt, dirname(binary), app).length, 'a newly readable watch changes its identity');
      chmodSync(blocked, 0);
      // Permission changes alter the guarded identity; restore the receipt for the remaining checks.
      Object.assign(build, bake(), {products:build.products});
      writeFileSync(receipt, JSON.stringify(build)); fresh();
    }
    for (const path of [binary, dll]) {
      const before = readFileSync(path);
      writeFileSync(path, 'changed');
      assert.ok(packagedBuildChanges(receipt, dirname(binary), app).includes(path));
      assert.ok(windowsBuild(app).changed.includes(path));
      writeFileSync(path, before); fresh();
    }
    assert.ok(!build.binary.inputs.some(file => file.path === resolve(dir, 'app.contract')));
    const cohort = {binary:build.binary.sha256,compat:build.compat,sources:build.graph.sources};
    write('app.contract', readFileSync(resolve(dir, 'app.contract'), 'utf8').replace('before', 'after!'));
    utimesSync(resolve(dir, 'app.contract'), old, old);
    assert.equal(classifyArtifacts(build, cohort).binary, false, 'replaceable source edits do not change the binary identity');
    assert.deepEqual(pendingBuildInputs(build), [], 'binary classification considers only binary inputs');
    const rebaked = bake();
    assert.equal(rebaked.binary.sha256, build.binary.sha256, 'rebaking a plan edit keeps the binary fingerprint');
    assert.equal(classifyArtifacts(rebaked, cohort).binary, false);
    assert.deepEqual(pendingBuildInputs(rebaked, {includeSources:true}), [], 'rebaked equal-mtime sources are fresh');
    write('optional.txt', 'present');
    const stable = bake();
    const planPath = resolve(stable.sourceInputs.outputs.find(path => basename(path) === 'out'), 'app.plan');
    const before = statSync(planPath, {bigint:true}).mtimeNs;
    if (mode === 'source additions') {
      bake();
      assert.equal(statSync(planPath, {bigint:true}).mtimeNs, before, 'unchanged sources must not rerun the bake');
    }
    write('value.ts', 'export const other = 2;\n');
    utimesSync(resolve(dir, 'value.ts'), old, old);
    const added = bake();
    assert.ok(readFileSync(resolve(bakeOutput(app), `windows-${target}.plan`), 'utf8').includes('other = 2'), 'the rebuild must bake the new import candidate');
    assert.deepEqual(pendingBuildInputs(added, {includeSources:true}), [], 'the rebuilt inventory is fresh');
  } finally { if (blocked) chmodSync(blocked, 0o600); rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('kept registry units rebuild foreign dep-info while retaining valid units', async () => {
  const { keptCrates } = await import('../host/apple/crates.mjs');
  const { unitDepInfo } = await import('./cargo-dep-info.mjs');
  const root = pinned(realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-crate-proof-'))));
  const profile = 'cache-proof', first = resolve(root, 'first'), second = resolve(root, 'second');
  const metadata = target => ({ target_directory: target, build_directory: target });
  const build = target => {
    const result = spawnSync('cargo', ['build', '--offline', '--profile', profile, '--lib', '--message-format=json-render-diagnostics'], {
      cwd: root, env: { ...process.env, CARGO_TARGET_DIR: target }, encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim().split('\n').map(line => JSON.parse(line)).filter(row => row.reason === 'compiler-artifact');
  };
  try {
    mkdirSync(resolve(root, 'src'));
    writeFileSync(resolve(root, 'Cargo.toml'), `[workspace]\n[package]\nname="cache-proof"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nzeroize="=1.9.0"\ncfg-if="=1.0.4"\n[profile.${profile}]\ninherits="dev"\ndebug=false\n`);
    writeFileSync(resolve(root, 'src/lib.rs'), 'pub fn clear(bytes: &mut [u8]) { zeroize::Zeroize::zeroize(bytes); }\n');
    const original = build(first).find(row => row.target.name === 'zeroize');
    const dep = unitDepInfo(original, root, metadata(first)), before = statSync(dep);
    const contents = readFileSync(dep, 'utf8');
    assert.ok(contents.startsWith(dep + ': '));
    writeFileSync(dep, resolve(root, 'foreign', basename(dep)) + contents.slice(dep.length));
    utimesSync(dep, before.atime, before.mtime);
    assert.throws(() => unitDepInfo(original, root, metadata(first)), /no matching rustc unit dep-info for zeroize/);
    const home = resolve(root, 'cache-home');
    const cache = keptCrates({ root, profile, env: { HOME: home, RUSTUP_TOOLCHAIN: 'fixture' } });
    cache.keep(first, 'app', resolve(root, 'Cargo.lock'));
    const store = resolve(home, '.cache/exact/apple-crates', `fixture-${profile}`, 'app');
    const generation = resolve(store, readdirSync(store).find(name => name.startsWith('gen-')));
    const snapshot = () => readdirSync(generation, { recursive: true }).sort().map(file => {
      const path = resolve(generation, file), stat = statSync(path, { bigint: true });
      return [file, stat.mtimeNs, stat.isFile() ? createHash('sha256').update(readFileSync(path)).digest('hex') : null];
    });
    const kept = snapshot();
    cache.take(second, 'app');
    const rebuilt = build(second), zeroize = rebuilt.find(row => row.target.name === 'zeroize');
    assert.ok(unitDepInfo(zeroize, root, metadata(second)).startsWith(second));
    assert.equal(zeroize.fresh, false, 'invalid evidence forces Cargo to regenerate the unit');
    assert.equal(rebuilt.find(row => row.target.name === 'cfg_if').fresh, true, 'valid registry units stay reusable');
    assert.deepEqual(snapshot(), kept, 'importing and rebuilding must leave the source generation unchanged');
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 60_000);

// These cases run cargo (the filesystem tool, bakes, locks). A shell whose PATH
// omits rustup's bin directory still finds it there; without cargo, say so.
const cargoBin = resolve(process.env.CARGO_HOME ?? resolve(homedir(), '.cargo'), 'bin');
if (!(process.env.PATH ?? '').split(delimiter).includes(cargoBin)) process.env.PATH = `${process.env.PATH ?? ''}${delimiter}${cargoBin}`;
if (!Bun.which('cargo', { PATH: process.env.PATH })) throw new Error(`these tests need cargo: put it on PATH or in ${cargoBin}`);
// The fixtures name their apps; a caller's EXACT_APP_DIR would redirect every one.
delete process.env.EXACT_APP_DIR;

test('runner-owned sources never warn that native app code is retained', () => {
  for (const name of ['exactSurface', 'exactViewport', 'exactDelivery', 'appData']) {
    const sources = { [name]: { params: [], result: {} } };
    const candidate = { binary: { sha256: 'new' }, graph: { artifacts: [], sources }, compat: { inputs: { dataCrate: 'new' } } };
    const cohort = { binary: 'old', sources, compat: { id: 'old', inputs: { dataCrate: 'old' } } };
    assert.equal(classifyArtifacts(candidate, cohort).warnings.length, name === 'appData' ? 1 : 0, name);
  }
});

// Opt in because this exercises three real Cargo bakes, not a mocked driver.
test.skipIf(!process.env.EXACT_ASSET_BAKE_TEST)('creating optional asset roots rebakes once and then stays fresh', async () => {
  const { mkdtempSync, mkdirSync, writeFileSync, statSync, rmSync, readFileSync, readdirSync } = await import('node:fs');
  const { resolve } = await import('node:path');
  const { tmpdir } = await import('node:os');
  const { spawnSync } = await import('node:child_process');
  const { buildBake, readManifest } = await import('./app.mjs');
  const root = resolve(import.meta.dir, '..'), dir = mkdtempSync(resolve(tmpdir(), 'exact-asset-roots-'));
  try {
    mkdirSync(resolve(dir, 'web/src'), { recursive: true });
    writeFileSync(resolve(dir, 'rust-toolchain.toml'), readFileSync(resolve(root, 'rust-toolchain.toml')));
    writeFileSync(resolve(dir, 'Cargo.toml'), `[workspace]\nmembers=["web"]\nresolver="2"\n[patch.crates-io]\ntaffy={path=${JSON.stringify(resolve(root, 'vendor/taffy'))}}\n`);
    writeFileSync(resolve(dir, 'web/Cargo.toml'), `[package]\nname="asset-roots-web"\nversion="0.1.0"\nedition="2021"\n[lib]\ncrate-type=["cdylib"]\n[dependencies]\nexact-runner={path=${JSON.stringify(resolve(root, 'runner'))}}\nexact-web={path=${JSON.stringify(resolve(root, 'host/web'))}}\n[build-dependencies]\nexact-game-app={path=${JSON.stringify(resolve(root, 'game/app'))}}\n`);
    writeFileSync(resolve(dir, 'web/build.rs'), 'fn main() { exact_game_app::bake("web", ".."); }');
    writeFileSync(resolve(dir, 'web/src/lib.rs'), 'include!(concat!(env!("OUT_DIR"), "/entry.rs"));');
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "assets"\n');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Asset roots', app: { id: 'com.exact.assetroots', name: 'Asset roots' }, rust: false, deploy: { store: { web: '0' } } }));
    const lock = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: dir, encoding: 'utf8' });
    assert.equal(lock.status, 0, lock.stderr);
    const manifest = readManifest(dir, 'asset-roots');
    const app = { dir, workspace: dir, target: resolve(root, 'target'), name: 'asset-roots', id: manifest.app.id, manifest, crate: kind => `asset-roots-${kind}` };
    const output = resolve(dir, 'bakes'), target = 'wasm32-unknown-unknown';
    const bake = () => buildBake(app, 'web', target, { output, profile: 'dev', env: { EXACT_UPDATE_TRUST: 'development' } });
    const receiptPath = resolve(output, `web-${target}.json`);
    bake();
    // Packaging recopies receipts even for a cache hit; measure Cargo's actual OUT_DIR.
    const builds = resolve(app.target, target, 'debug/build');
    const unit = readdirSync(builds).find(name => name.startsWith('asset-roots-web-') && (() => {
      try { return readFileSync(resolve(builds, name, 'output'), 'utf8').includes(dir); } catch { return false; }
    })());
    assert.ok(unit);
    const bakedPath = resolve(builds, unit, 'out/compat.json');
    let before = statSync(bakedPath, { bigint: true }).mtimeNs;
    for (const [directory, prefix] of [['assets', 'assets'], ['deck', 'deck'], ['gpu/shaders', 'shaders']]) {
      mkdirSync(resolve(dir, directory), { recursive: true });
      const name=prefix==='shaders'?'x.wgsl':'x.png';
      writeFileSync(resolve(dir,directory,name),prefix==='shaders'?'@compute @workgroup_size(1) fn cs() {}':Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=','base64'));
      bake();
      const changed = statSync(bakedPath, { bigint: true }).mtimeNs;
      assert.notEqual(changed, before, `${directory}: bake did not rerun`);
      const receipt = JSON.parse(readFileSync(receiptPath, 'utf8'));
      assert.ok(receipt.embedded.assets.some(a => a.name === `${prefix}/${name}`));
      bake();
      assert.equal(statSync(bakedPath, { bigint: true }).mtimeNs, changed, `${directory}: unchanged third build reran`);
      before = changed;
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300000);

function watchFixture(use) {
  const dir=mkdtempSync(resolve(tmpdir(),'watch-'));
  try {
    const out=resolve(dir,'out'), watched=resolve(dir,'watched'), target=resolve(dir,'target');
    for(const path of [out,watched,target])mkdirSync(path);
    const pkg={id:'watch',manifest_path:resolve(dir,'Cargo.toml'),targets:[{kind:['custom-build']}]};
    const env={CARGO_TARGET_DIR:target,EXACT_BAKE_OUTPUT:resolve(dir,'bake')};
    writeFileSync(resolve(out,'../output'),`cargo:rerun-if-changed=${watched}\n`);
    const messages=[{reason:'build-script-executed',package_id:pkg.id,out_dir:out}];
    const prepare=()=>cargoBakeInputs(pkg,'target','dev',env);
    use({dir,out,watched,env,messages,prepare,pkg});
  }finally{rmSync(dir,{recursive:true,force:true});}
}

test.skipIf(process.platform === 'win32')('a watched directory follows inner directory links without cycles or output aliases',()=>watchFixture(({dir,out,watched,env,messages,prepare})=>{
  const real=resolve(dir,'real');mkdirSync(real);writeFileSync(resolve(real,'input'),'ORIGINAL');
  symlinkSync(real,resolve(watched,'link'),'dir');
  symlinkSync(watched,resolve(real,'back'),'dir');
  symlinkSync('loop-b',resolve(watched,'loop-a'));symlinkSync('loop-a',resolve(watched,'loop-b'));
  mkdirSync(env.EXACT_BAKE_OUTPUT);
  env.CARGO_BUILD_BUILD_DIR=resolve(dir,'build');mkdirSync(env.CARGO_BUILD_BUILD_DIR);
  const outputs=[env.CARGO_TARGET_DIR,env.EXACT_BAKE_OUTPUT,env.CARGO_BUILD_BUILD_DIR,out];
  for(const [i,path] of outputs.entries())symlinkSync(path,resolve(watched,`output-${i}`),'dir');
  prepare()(messages);const first=env.EXACT_BAKE_INPUTS;
  writeFileSync(resolve(real,'input'),'RESTORED');utimesSync(resolve(real,'input'),new Date(0),new Date(0));
  const record=prepare();assert.notEqual(env.EXACT_BAKE_INPUTS,first,'bytes behind the inner directory link must change the revision');
  record(messages);const changed=env.EXACT_BAKE_INPUTS;
  for(const path of outputs)writeFileSync(resolve(path,'product'),'new output');
  prepare();assert.equal(env.EXACT_BAKE_INPUTS,changed,'output aliases must not change the next revision');
}));

test('disappearance after metadata records a missing input and a stable revision',()=>watchFixture(({out,watched,env,messages,prepare,pkg})=>{
  const path=resolve(watched,'vanish');
  // A directly watched missing path must contribute a row, even without a parent watch.
  writeFileSync(resolve(out,'../output'),`cargo:rerun-if-changed=${path}\n`);
  writeFileSync(path,'content');prepare()(messages);
  const script=`import {mock} from 'bun:test'; import * as fs from 'node:fs';
    const read=fs.readFileSync, remove=fs.rmSync;
    mock.module('node:fs',()=>({...fs,readFileSync:(path,...args)=>{if(path===${JSON.stringify(path)})remove(path);return read(path,...args);}}));
    const {cargoBakeInputs}=await import(${JSON.stringify(resolve(import.meta.dir,'cargo-dep-info.mjs'))});
    const env=${JSON.stringify(env)};
    cargoBakeInputs(${JSON.stringify(pkg)},'target','dev',env)(${JSON.stringify(messages)});
    console.log(env.EXACT_BAKE_INPUTS);
  `;
  const result=spawnSync(process.execPath,['-e',script],{encoding:'utf8'});
  assert.equal(result.status,0,result.stderr);
  assert.equal(existsSync(path),false,'the read must have raced with removal');
  const records=resolve(env.CARGO_TARGET_DIR,'bake-inputs');
  const record=JSON.parse(readFileSync(resolve(records,readdirSync(records)[0]),'utf8'));
  assert.deepEqual(record.paths,[path]);
  assert.notEqual(record.digest,createHash('sha256').digest('hex'),'a missing watch must not be silently omitted');
  prepare();
  assert.equal(env.EXACT_BAKE_INPUTS,result.stdout.trim(),'a second scan with the file gone must keep the recorded revision');
}));

// Real Cargo units include named game logic for the lint and absent-art receipt.
test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST)('native bakes stay fresh and retain unit source and environment evidence', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact native cache-')));
  const write = (path, bytes) => { mkdirSync(dirname(resolve(dir, path)), {recursive:true}); writeFileSync(resolve(dir, path), bytes); };
  try {
    write('rust-toolchain.toml', readFileSync(resolve(import.meta.dir, '../rust-toolchain.toml')));
    write('Cargo.toml', '[workspace]\nmembers=["gpu","linux"]\nresolver="2"\n');
    write('gpu/Cargo.toml', '[package]\nname="cache-gpu"\nversion="0.1.0"\nedition="2021"\n[lib]\nname="cache_module"\ncrate-type=["cdylib","staticlib","rlib"]\n');
    write('gpu/src/lib.rs', '#[no_mangle]\npub extern "C" fn fixture() -> usize { env!("CACHE_FIXTURE_VALUE").len() + include_bytes!("../payload").len() }\n');
    write('gpu/payload', 'first');
    write('linux/Cargo.toml', '[package]\nname="cache-linux"\nversion="0.1.0"\nedition="2021"\n[[bin]]\nname="cache-runner"\npath="src/main.rs"\n');
    write('linux/src/main.rs', 'fn main() { println!("{}", env!("CACHE_EXEC_VALUE").len() + include_bytes!("../payload").len()); }\n');
    write('linux/payload', 'first');
    const target = bakeTarget('linux'), id = 'com.exact.cachefixture';
    const compat = {target, inputs:{platform:'linux', app:id, store:{L:'0'}, keys:[]}};
    write('linux/build.rs', `fn main() {
      println!("cargo:rerun-if-changed=build.rs");
      let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
      std::fs::write(out.join("compat.json"), r#"${JSON.stringify(compat)}"#).unwrap();
      std::fs::write(out.join("artifacts.json"), r#"{"version":1,"artifacts":[],"sources":{}}"#).unwrap();
      std::fs::write(out.join("app.plan"), b"fixture").unwrap();
    }`);
    write('.shells/Cargo.toml', '[workspace]\n[package]\nname="cache-logic"\nversion="0.1.0"\nedition="2021"\n'); write('.shells/src/lib.rs', 'pub fn fixture() -> bool { true }\n');
    for (const cwd of [dir, resolve(dir,'.shells')]) { const lock = spawnSync('cargo', ['generate-lockfile', '--offline'], {cwd, encoding:'utf8'}); assert.equal(lock.status, 0, lock.stderr); }
    const app = {dir, workspace:dir, target:resolve(dir,'target'), id, hasGpu:true,
      manifest:{app:{id, name:'Cache fixture'}, game:{crate:'cache-logic'}, rust:false}, crate:kind=>`cache-${kind}`};
    const bake = (value, executable = value) => buildBake(app, 'linux', target, {profile:'dev', output:resolve(dir,'bakes'),
      env:{EXACT_UPDATE_TRUST:'development', CACHE_FIXTURE_VALUE:value, CACHE_EXEC_VALUE:executable, EXACT_RUST_BUNDLE:'', EXACT_UPDATE_RECEIPT:''}});
    const modified = receipt => receipt.products.filter(p=>/libcache_module\.|\/cache-runner$/.test(p.path))
      .map(p=>[p.path, statSync(p.path, {bigint:true}).mtimeNs]);
    const environment = receipt => receipt.binary.configuration.units.find(u=>u.target==='cache_module').environment;
    const first = bake('left'), stamp = modified(first);
    assert.ok(stamp.length >= 2, 'dynamic and static libraries must both be exercised');
    assert.ok(first.binary.inputs.some(f=>f.path===resolve(dir,'gpu/src/lib.rs')));
    assert.ok(first.binary.inputs.some(f=>f.path===resolve(dir,'gpu/payload')));
    assert.ok(environment(first).some(([key])=>key==='CACHE_FIXTURE_VALUE'), 'summary .d is not unit evidence');
    assert.ok(first.binary.configuration.units.find(u=>u.target==='cache-runner').environment
      .some(([key])=>key==='CACHE_EXEC_VALUE'), 'the executable retains compile-time environment evidence');
    assert.ok(first.binary.inputs.some(f=>f.path===resolve(dir,'linux/payload')));
    assert.ok(first.binary.missing.includes(resolve(dir,'art')), 'dev must observe the first art directory');
    mkdirSync(resolve(dir, 'art'));
    assert.ok(pendingBuildInputs(first).includes(resolve(dir, 'art')));
    rmSync(resolve(dir, 'art'), {recursive:true});
    const unchanged = bake('left');
    assert.deepEqual(modified(unchanged), stamp, 'unchanged libraries and executables must not relink');
    assert.equal(unchanged.binary.sha256, first.binary.sha256);
    const changed = bake('rght'); // Same length and compiled result; the environment still differs.
    assert.notDeepEqual(environment(changed), environment(first));
    assert.notEqual(changed.binary.sha256, first.binary.sha256);
    const changedStamp = modified(changed), stable = bake('rght');
    assert.deepEqual(modified(stable), changedStamp);
    assert.equal(stable.binary.sha256, changed.binary.sha256);
    write('gpu/payload', 'second');
    const edited = bake('rght');
    assert.notEqual(edited.binary.sha256, stable.binary.sha256);
    assert.notEqual(edited.binary.inputs.find(f=>f.path.endsWith('/gpu/payload')).sha256,
      stable.binary.inputs.find(f=>f.path.endsWith('/gpu/payload')).sha256);
    const executable = bake('rght', 'next');
    assert.notEqual(executable.binary.sha256, edited.binary.sha256);
    assert.notDeepEqual(executable.binary.configuration.units.find(u=>u.target==='cache-runner').environment,
      edited.binary.configuration.units.find(u=>u.target==='cache-runner').environment);
    write('linux/payload', 'second');
    const binaryEdit = bake('rght', 'next');
    assert.notEqual(binaryEdit.binary.inputs.find(f=>f.path.endsWith('/linux/payload')).sha256,
      executable.binary.inputs.find(f=>f.path.endsWith('/linux/payload')).sha256);
  } finally { rmSync(dir, {recursive:true, force:true}); }
}, 180000);

test.each(['rlib', 'staticlib', 'executable', 'windows-executable', 'build-script', 'windows-build-script'].flatMap(kind => [null, 'intermediate', 'output/intermediate', '.'].map(split => [kind, split])))('copied %s roots with build directory %s require unique compiler dep-info', async (kind, split) => {
  const { unitDepInfo } = await import('./app.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-unit-dep-'));
  try {
    const metadata = {target_directory:resolve(root, 'output'), build_directory:resolve(root, split ?? 'output')};
    const custom = kind.endsWith('build-script'), folder = custom ? 'release/build/script-a11ce' : 'release';
    const dir = resolve(custom ? metadata.build_directory : metadata.target_directory, folder);
    const deps = custom ? dir : resolve(metadata.build_directory, folder, 'deps'), src = resolve(root, custom ? 'build.rs' : 'src/main.rs');
    mkdirSync(dir, {recursive:true});
    mkdirSync(deps, {recursive:true});
    const executable = custom || kind.endsWith('executable'), target = custom ? 'build-script-build' : executable ? 'game-native' : 'game_apple';
    const suffix = kind.startsWith('windows-') ? '.exe' : '';
    const extension = kind === 'staticlib' ? '.a' : '.rlib';
    const artifact = resolve(dir, executable ? target + suffix : `lib${target}${extension}`);
    writeFileSync(artifact, 'selected unit');
    const message = {filenames:[artifact],
      executable:executable && !custom ? artifact : null, target:{name:target, src_path:src,kind:custom ? ['custom-build'] : []}};
    writeFileSync(resolve(dir, `${target}.d`), `${artifact}: ${src}\n`);
    const unit = (hash, bytes, source = src) => {
      const name = `${target.replaceAll('-', '_')}-${hash}`, dep = resolve(deps, `${name}.d`);
      writeFileSync(resolve(deps, executable ? name + suffix : `lib${name}${extension}`), bytes);
      writeFileSync(dep, `${dep}: ${source}\n\n# env-dep:EXACT_UPDATE_TRUST=development\n`);
      return dep;
    };
    unit('deadbeef', 'another unit');
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
    const expected = unit('a11ce', 'selected unit', resolve(root, 'old/main.rs'));
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
    writeFileSync(expected, `${resolve(root, 'copied.d')}: ${src}\n`);
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
    unit('a11ce', 'selected unit');
    assert.equal(unitDepInfo(message, root, metadata), expected);
    assert.match(readFileSync(unitDepInfo(message, root, metadata), 'utf8'), /env-dep:EXACT_UPDATE_TRUST=development/);
    unit('aabbcc', 'selected unit');
    assert.throws(() => unitDepInfo(message, root, metadata), /ambiguous rustc unit/);
  } finally { rmSync(root, {recursive:true, force:true}); }
});

test.each([null, 'intermediate', 'output/intermediate'])('copied wasm with build directory %s requires unique compiler evidence', async split => {
  const { unitDepInfo } = await import('./app.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-wasm-dep-'));
  try {
    const metadata = {target_directory:resolve(root, 'output'), build_directory:resolve(root, split ?? 'output'),
      packages:[{id:'game-id', name:'game-gpu'}]};
    const dir = resolve(metadata.target_directory, 'wasm32-unknown-unknown/web'), src = resolve(root, 'gpu/src/lib.rs');
    mkdirSync(dir, {recursive:true});
    const artifact = resolve(dir, 'game_gpu.wasm');
    writeFileSync(artifact, 'selected wasm');
    const message = {package_id:'game-id', filenames:[artifact], target:{name:'game_gpu', src_path:src}};
    const unit = (hash, bytes, source = src) => {
      const out = resolve(metadata.build_directory, 'wasm32-unknown-unknown/web/build/game-gpu', hash, 'out');
      mkdirSync(out, {recursive:true});
      writeFileSync(resolve(out, 'game_gpu.wasm'), bytes);
      const dep = resolve(out, 'game_gpu.d');
      writeFileSync(dep, `${dep}: ${source}\n# env-dep:EXACT_GAME_PARANOID=0\n`);
      return dep;
    };
    unit('deadbeef', 'another wasm');
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
    const expected = unit('a11ce', 'selected wasm', resolve(root, 'old/lib.rs'));
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
    unit('a11ce', 'selected wasm');
    writeFileSync(expected, `${artifact}: ${src}\n`);
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
    unit('a11ce', 'selected wasm');
    const deps = resolve(metadata.build_directory, 'wasm32-unknown-unknown/web/deps'), stale = resolve(deps, 'game_gpu.d'); // another toolchain's unit: the JS target's stable GPU build
    mkdirSync(deps, {recursive:true}); writeFileSync(resolve(deps, 'game_gpu.wasm'), 'stable wasm'); writeFileSync(stale, `${stale}: ${src}\n`);
    assert.equal(unitDepInfo(message, root, metadata), expected);
    assert.match(readFileSync(expected, 'utf8'), /env-dep:EXACT_GAME_PARANOID=0/);
    unit('aabbcc', 'selected wasm');
    assert.throws(() => unitDepInfo(message, root, metadata), /ambiguous rustc unit/);
    writeFileSync(resolve(deps, 'game_gpu.wasm'), 'selected wasm'); assert.equal(unitDepInfo(message, root, metadata), stale, 'the unit Cargo copied out of deps/');
  } finally { rmSync(root, {recursive:true, force:true}); }
});

test('watch archives from nightly require matching bytes and the exact compiler unit', async () => {
  const { unitDepInfo } = await import('./app.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-watch-dep-'));
  try {
    const directory = resolve(root, 'aarch64-apple-watchos-sim/host-dev');
    const out = resolve(directory, 'build/caltrain-apple/a11ce/out');
    mkdirSync(out, { recursive: true });
    const file = resolve(directory, 'libcaltrain_apple.a'), unit = resolve(out, 'libcaltrain_apple-a11ce.a');
    const dep = resolve(out, 'caltrain_apple-a11ce.d'), src = resolve(root, 'lib.rs');
    const metadata = { target_directory: root, build_directory: root, packages: [{ id: 'caltrain', name: 'caltrain-apple' }] };
    const message = { package_id: 'caltrain', filenames: [file], target: { name: 'caltrain_apple', src_path: src } };
    writeFileSync(file, 'selected archive'); writeFileSync(unit, 'different bytes');
    writeFileSync(dep, `${dep}: ${src}\n`);
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
    writeFileSync(unit, 'selected archive');
    assert.equal(unitDepInfo(message, root, metadata), dep);
    writeFileSync(dep, `${dep}: ${root}/wrong.rs\n`);
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a cdylib copied out of deps finds its unit dep-info by its bytes', async () => {
  const { unitDepInfo } = await import('./app.mjs');
  const root = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-cdylib-dep-')));
  try {
    // A game's GPU crate: Cargo reports target/<profile>/libgame_gpu.dylib, a copy of deps/libgame_gpu-<hash>.dylib,
    // whose unit dep-info is deps/game_gpu-<hash>.d (and the summary beside the copy is libgame_gpu.d, not game_gpu.d).
    const out = resolve(root, 'target/aarch64-apple-ios/host-dev'), deps = resolve(out, 'deps'), src = resolve(root, 'gpu/src/lib.rs');
    mkdirSync(deps, { recursive: true });
    const artifact = resolve(out, 'libgame_gpu.dylib');
    writeFileSync(artifact, 'dylib'); writeFileSync(resolve(deps, 'libgame_gpu-68d7406e.dylib'), 'dylib');
    const dep = resolve(deps, 'game_gpu-68d7406e.d');
    writeFileSync(dep, `${dep}: ${src}\n${resolve(deps, 'libgame_gpu-68d7406e.dylib')}: ${src}\n`);
    writeFileSync(resolve(out, 'libgame_gpu.d'), `${artifact}: ${src}\n`);
    const metadata = { target_directory: resolve(root, 'target'), build_directory: resolve(root, 'target') };
    const message = { filenames: [artifact], executable: null, target: { name: 'game-gpu', src_path: src } };
    assert.equal(unitDepInfo(message, root, metadata), dep);
    // A unit with other bytes is another build's, never this product's.
    writeFileSync(resolve(deps, 'libgame_gpu-68d7406e.dylib'), 'other');
    assert.throws(() => unitDepInfo(message, root, metadata), /no matching rustc unit dep-info for game-gpu/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('rustc unit dep-info written under another spelling of the same directory still matches', async () => {
  const { unitDepInfo } = await import('./app.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-spelled-dep-')), alias = root + '-alias';
  try {
    // Cargo wrote the unit through one spelling (macOS's /private/tmp); the build asks through another (/tmp).
    symlinkSync(root, alias);
    const out = resolve(root, 'out/release'), deps = resolve(out, 'deps');
    mkdirSync(deps, { recursive: true });
    const src = resolve(root, 'src/lib.rs'), artifact = resolve(out, 'libleaf.rlib');
    writeFileSync(artifact, 'unit'); writeFileSync(resolve(deps, 'libleaf-a11ce.rlib'), 'unit');
    const dep = resolve(deps, 'leaf-a11ce.d');
    writeFileSync(dep, `${dep}: ${src}\n`);
    const metadata = { target_directory: resolve(alias, 'out'), build_directory: resolve(alias, 'out') };
    const message = { filenames: [resolve(alias, 'out/release/libleaf.rlib')], executable: null, target: { name: 'leaf', src_path: resolve(alias, 'src/lib.rs') } };
    assert.equal(realpathSync(unitDepInfo(message, alias, metadata)), realpathSync(dep));
    writeFileSync(dep, `${dep}: ${resolve(root, 'src/other.rs')}\n`);
    assert.throws(() => unitDepInfo(message, alias, metadata), /no matching rustc unit dep-info for leaf; searched target directory /);
  } finally { rmSync(alias, { force: true }); rmSync(root, { recursive: true, force: true }); }
});

test('a target directory copied from another checkout names where it was built', async () => {
  const { unitDepInfo } = await import('./app.mjs');
  const root = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-moved-dep-')));
  try {
    const first = resolve(root, 'first/target'), second = resolve(root, 'second/target'), src = resolve(root, 'registry/leaf/src/lib.rs');
    const deps = resolve(second, 'debug/deps'), artifact = resolve(deps, 'libleaf-a11ce.rlib'), dep = resolve(deps, 'leaf-a11ce.d');
    mkdirSync(deps, { recursive: true });
    writeFileSync(artifact, 'unit');
    const metadata = { target_directory: second, build_directory: second };
    const message = { filenames: [artifact], executable: null, target: { name: 'leaf', src_path: src } };
    writeFileSync(dep, `${resolve(root, 'other.d')}: ${src}\n`);
    assert.throws(() => unitDepInfo(message, resolve(root, 'second'), metadata), /rebuild the stale Cargo unit/);
    writeFileSync(dep, `${resolve(first, 'debug/deps/leaf-a11ce.d')}: ${src}\n`);
    assert.throws(() => unitDepInfo(message, resolve(root, 'second'), metadata),
      { message: `no matching rustc unit dep-info for leaf; searched target directory ${second}; ${second} was built at ${first}: delete it and build again` });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test.each([null, 'intermediate', 'output/intermediate'])('rustc unit dep-info accepts spaces and build directory %s', async split => {
  const { unitDepInfo } = await import('./app.mjs');
  const { spawnSync } = await import('node:child_process');
  const root = mkdtempSync(resolve(tmpdir(), 'exact unit dep '));
  try {
    const metadata = {target_directory:resolve(root, 'output'), build_directory:resolve(root, split ?? 'output')};
    mkdirSync(metadata.build_directory, {recursive:true});
    const source = resolve(root, 'lib.rs'), artifact = resolve(metadata.build_directory, 'libspace_unit.rlib');
    writeFileSync(source, 'pub fn value() -> u32 { 1 }');
    const result = spawnSync('rustc', ['--crate-name', 'space_unit', '--crate-type', 'lib',
      '--emit=dep-info,link', source, '--out-dir', metadata.build_directory], {encoding:'utf8'});
    assert.equal(result.status, 0, result.stderr);
    const message = {filenames:[artifact], target:{name:'space_unit', src_path:source}};
    assert.equal(unitDepInfo(message, root, metadata), resolve(metadata.build_directory, 'space_unit.d'));
  } finally { rmSync(root, {recursive:true, force:true}); }
}, 60_000);

test('Cargo diagnostics retain unrelated text across chunks and shared patch help blocks', () => {
  const patch = path => `warning: patch \`wgpu-hal v30.0.1 (${path})\` was not used in the crate graph`;
  const exact = patch(realpathSync(resolve(import.meta.dir, '../vendor/wgpu-hal')));
  const author = patch('/app/patches/wgpu-hal');
  const help = [
    'help: Check that the patched package version and available features are compatible',
    '      with the dependency requirements. If the patch has a different version from',
    '      what is locked in the Cargo.lock file, run `cargo update` to use the new',
    '      version. This may also occur with an optional dependency that is not enabled.',
  ].join('\r\n');
  const kept = `warning: app warning\r\nerror: café failed\r\n      its context\r\npartial output`;
  const output = `${exact}\r\n${author}\r\n${help}\r\n\x1b[33m${exact}\x1b[0m\r\n${help}\r\n${kept}`;
  for (const size of [1, 7, output.length]) {
    let said = '';
    const filter = cargoDiagnosticFilter(text => { said += text; });
    for (let i = 0; i < output.length; i += size) filter.write(output.slice(i, i + size));
    filter.end();
    assert.equal(said, `${author}\r\n${help}\r\n${kept}`);
  }
});

test('the Cargo diagnostic relay preserves exit status, stdout and the build environment', () => {
  mkdirSync(resolve(import.meta.dir, '../target'), {recursive:true});
  const dir = pinned(realpathSync(mkdtempSync(resolve(import.meta.dir, '../target/cargo-relay-'))));
  const env = { ...process.env, RUSTUP_AUTO_INSTALL: '0' };
  delete env.RUSTUP_TOOLCHAIN;
  try {
    writeFileSync(resolve(dir, '.env'), 'RUSTUP_TOOLCHAIN=exact-relay-must-not-load\n');
    const options = { cwd: dir, env, stdio: ['ignore', 'pipe', 'inherit'], encoding: 'utf8' };
    const args = ['--version'];
    assert.deepEqual(cargoDiagnosticsCommand('rustc', args, options), ['rustc', args, options]);
    const piped = { ...options, stdio: ['ignore', 'pipe', 'pipe'] };
    assert.deepEqual(cargoDiagnosticsCommand('cargo', args, piped), ['cargo', args, piped]);
    const run = args => spawnSync(...cargoDiagnosticsCommand('cargo', args, options));
    const version = run(args);
    assert.equal(version.status, 0);
    assert.match(version.stdout, /^cargo \d+\./);
    const refused = run(['--exact-invalid-option']);
    assert.equal(refused.status, 1);
    assert.equal(refused.stdout, '');
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test.each([false, true])('ordinary buildBake with split directories=%s streams progress and retains product receipts', async split => {
  mkdirSync(resolve(import.meta.dir, '../target'), {recursive:true});
  const dir = pinned(realpathSync(mkdtempSync(resolve(import.meta.dir, '../target/r14-no-lock-'))));
  const write = (name, text) => { mkdirSync(dirname(resolve(dir,name)), {recursive:true}); writeFileSync(resolve(dir,name),text); };
  try {
    const target = bakeTarget('linux'), id = 'com.exact.plain';
    write('Cargo.toml', `[workspace]\nmembers=["linux"]\nresolver="2"\n[patch.crates-io]\nwgpu-hal={path=${JSON.stringify(resolve(import.meta.dir, '../vendor/wgpu-hal'))}}\n${split ? 'parley={path="patches/parley"}\n' : ''}`);
    write('patches/parley/Cargo.toml', '[package]\nname="parley"\nversion="0.11.1"\nedition="2021"\n');
    write('patches/parley/src/lib.rs', '');
    write('linux/Cargo.toml', '[package]\nname="plain-linux"\nversion="0.1.0"\nedition="2021"\n');
    write('linux/src/main.rs', 'fn main() { let unused = 1; }');
    write('app.contract', 'component App\n  view\n    text "plain"\n');
    write('linux/build.rs', `fn main() {
      println!("cargo:warning=application build warning");
      let release = std::path::Path::new(${JSON.stringify(resolve(dir,'progress-received'))});
      let started = std::time::Instant::now();
      while !release.exists() {
        assert!(started.elapsed().as_secs() < 30, "Cargo progress was buffered until the build finished");
        std::thread::sleep(std::time::Duration::from_millis(10));
      }
      let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
      std::fs::write(out.join("compat.json"), r#"${JSON.stringify({target,inputs:{platform:'linux',app:id,store:{L:'0'},keys:[]}})}"#).unwrap();
      std::fs::write(out.join("artifacts.json"), r#"{"version":1,"artifacts":[],"sources":{}}"#).unwrap();
      std::fs::write(out.join("app.plan"), b"fixture").unwrap();
    }`);
    const app = {dir, workspace:dir, target:resolve(dir,'target'), name:'plain', id,
      manifest:{app:{id,name:'Plain'},rust:false}, crate:kind=>`plain-${kind}`};
    write('bake.mjs', `import {buildBake} from ${JSON.stringify(resolve(import.meta.dir,'app.mjs'))};
      const app = {...${JSON.stringify(app)},crate:kind=>'plain-'+kind};
      const receipt = buildBake(app,'linux',${JSON.stringify(target)},{profile:'dev',output:${JSON.stringify(resolve(dir,'bakes'))}});
      console.log(JSON.stringify(receipt));`);
    const bake = () => new Promise((ok, fail) => {
      const child = spawn(process.execPath,[resolve(dir,'bake.mjs')],{cwd:dir,env:{...process.env,EXACT_UPDATE_TRUST:'development',...(split ? {CARGO_BUILD_BUILD_DIR:resolve(dir,'intermediate')} : {})},stdio:['ignore','pipe','pipe']});
      let stdout='',stderr='';
      child.stdout.on('data', bytes=>{stdout+=bytes;});
      child.stderr.on('data', bytes=>{
        stderr+=bytes;
        if (stderr.includes('Compiling plain-linux')) write('progress-received','seen before completion');
      });
      child.on('error',fail);
      child.on('close',(code,signal)=>ok({code,signal,stdout,stderr}));
    });
    assert.equal(existsSync(resolve(dir,'Cargo.lock')),false);
    const built = await bake();
    assert.equal(built.code,0,built.stderr);
    assert.ok(existsSync(resolve(dir,'progress-received')));
    assert.equal((built.stderr.match(/warning: unused variable/g)??[]).length,1,built.stderr);
    const diagnostics = stderr => {
      assert.doesNotMatch(stderr, /warning: patch `wgpu-hal .*was not used in the crate graph/);
      if (split) assert.match(stderr, /warning: patch `parley .*patches[/\\]parley.*was not used in the crate graph/);
      else assert.doesNotMatch(stderr, /warning: patch /);
      assert.equal((stderr.match(/Check that the patched package version/g) ?? []).length, split ? 1 : 0, stderr);
      assert.match(stderr, /application build warning/);
      assert.match(stderr, /warning: unused variable/);
      assert.match(stderr, /Finished `dev` profile/);
    };
    diagnostics(built.stderr);
    const receipt = JSON.parse(built.stdout);
    assert.ok(receipt.products.some(p=>basename(p.path) === `plain-linux${process.platform === 'win32' ? '.exe' : ''}`));
    assert.ok(existsSync(resolve(dir,'Cargo.lock')));
    const warm = await bake();
    assert.equal(warm.code, 0, warm.stderr);
    diagnostics(warm.stderr);
    assert.deepEqual(JSON.parse(warm.stdout).products, receipt.products);
    const receiptPath=resolve(dir,'bakes',`linux-${target}.build.json`), before=readFileSync(receiptPath,'utf8');
    write('linux/src/main.rs','fn main() { let broken: u32 = "wrong type"; }');
    const refused = await bake();
    assert.notEqual(refused.code,0);
    assert.match(refused.stderr,/mismatched types/);
    assert.match(refused.stderr,/failed: exit 101/);
    assert.equal(readFileSync(receiptPath,'utf8'),before,'failed builds cannot replace the completed receipt');
  } finally { rmSync(dir,{recursive:true,force:true}); }
}, 180000);

test('a path dependency\'s build script: a watched file outside its package is named from it, a directory outside is refused', () => {
  const dir = pinned(realpathSync(mkdtempSync(resolve(tmpdir(), 'r14-outside-watch-'))));
  const write = (name, text) => { mkdirSync(dirname(resolve(dir,name)), {recursive:true}); writeFileSync(resolve(dir,name),text); };
  try {
    const target = bakeTarget('linux'), id = 'com.exact.plain', app = resolve(dir,'app');
    // `watcher` is another repository's crate: its `../../.env` is under neither the app, its workspace nor exact2.
    write('vendor/watcher/Cargo.toml', '[package]\nname="watcher"\nversion="0.1.0"\nedition="2021"\n');
    write('vendor/watcher/src/lib.rs', 'pub const VALUE: u32 = 7;\n');

    write('app/Cargo.toml', '[workspace]\nmembers=["linux"]\nresolver="2"\n');
    write('app/linux/Cargo.toml', '[package]\nname="plain-linux"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nwatcher={path="../../vendor/watcher"}\n');
    write('app/linux/src/main.rs', 'fn main() { let _ = watcher::VALUE; }');
    write('app/app.contract', 'component App\n  view\n    text "plain"\n');
    write('app/linux/build.rs', `fn main() {
      let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
      std::fs::write(out.join("compat.json"), r#"${JSON.stringify({target,inputs:{platform:'linux',app:id,store:{L:'0'},keys:[]}})}"#).unwrap();
      std::fs::write(out.join("artifacts.json"), r#"{"version":1,"artifacts":[],"sources":{}}"#).unwrap();
      std::fs::write(out.join("app.plan"), b"fixture").unwrap();
    }`);
    const fixture = {dir:app, workspace:app, target:resolve(app,'target'), name:'plain', id, manifest:{app:{id,name:'Plain'},rust:false}};
    write('app/bake.mjs', `import {buildBake} from ${JSON.stringify(resolve(import.meta.dir,'app.mjs'))};
      const receipt = buildBake({...${JSON.stringify(fixture)},crate:kind=>'plain-'+kind},'linux',${JSON.stringify(target)},{profile:'dev',output:${JSON.stringify(resolve(app,'bakes'))}});
      console.log(JSON.stringify(receipt));`);
    const bake = (watched) => {
      write('vendor/watcher/build.rs', `fn main() { println!("cargo:rerun-if-changed=${watched}"); }\n`);
      return spawnSync(process.execPath,[resolve(app,'bake.mjs')],{cwd:app,encoding:'utf8',env:{...process.env,EXACT_UPDATE_TRUST:'development'}});
    };
    const receipt = (built) => { assert.equal(built.status,0,built.stderr); return JSON.parse(built.stdout.trim().split('\n').at(-1)).binary; };
    // The name has `/` on every host: a receipt's names are the same on every machine.
    const file = receipt(bake('../../.env'));
    assert.ok(file.absent.includes('crate:watcher@0.1.0/../../.env'),JSON.stringify(file.absent));
    assert.ok(file.missing.includes(resolve(dir,'.env')));
    // A directory inside its own package is recorded as before.
    const inside = receipt(bake('src'));
    assert.ok(inside.directories.some(d => d.path === resolve(dir,'vendor/watcher/src')), JSON.stringify(inside.directories.map(d => d.path)));
    // A directory outside it is refused, saying which package watches what and what to do.
    const refused = bake('../..');
    assert.notEqual(refused.status, 0);
    assert.ok(refused.stderr.includes(`watcher 0.1.0's build script watches ${dir}, a directory outside its package`), refused.stderr);
    assert.match(refused.stderr, /may watch files outside its package and directories inside it: have it print `cargo:rerun-if-changed=<file>` for each file it reads there/);
  } finally { rmSync(dir,{recursive:true,force:true}); }
}, 180000);

test('ordinary bake metadata follows intermediate directory A, B and the unset default', () => {
  const dir=pinned(realpathSync(mkdtempSync(resolve(tmpdir(),'exact metadata directories-'))));
  const previous=process.env.CARGO_BUILD_BUILD_DIR;
  delete process.env.CARGO_BUILD_BUILD_DIR;
  const write=(name,bytes)=>{mkdirSync(dirname(resolve(dir,name)),{recursive:true});writeFileSync(resolve(dir,name),bytes);};
  try {
    const platform=process.platform==='win32'?'windows':'linux', target=bakeTarget(platform), id='com.exact.metadatadirs';
    write('rust-toolchain.toml',readFileSync(resolve(import.meta.dir,'../rust-toolchain.toml')));
    write('Cargo.toml',`[workspace]\nmembers=["${platform}"]\nresolver="2"\n`);
    write(`${platform}/Cargo.toml`,`[package]\nname="metadata-${platform}"\nversion="0.1.0"\nedition="2021"\n`);
    write('app.contract','component App\n  view\n    text "Metadata"\n');
    write(`${platform}/build.rs`,`fn main(){
      println!("cargo:rerun-if-changed=build.rs");
      let out=std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
      std::fs::write(out.join("compat.json"),r#"${JSON.stringify({target,inputs:{platform,app:id,store:{L:'0'},keys:[]}})}"#).unwrap();
      std::fs::write(out.join("artifacts.json"),r#"{"version":1,"artifacts":[],"sources":{}}"#).unwrap();
      std::fs::write(out.join("app.plan"),b"fixture").unwrap();
    }`);
    write(`${platform}/src/main.rs`,'fn main(){}');
    const lock=spawnSync('cargo',['generate-lockfile','--offline'],{cwd:dir,encoding:'utf8'});
    assert.equal(lock.status,0,lock.stderr);
    const app={dir,workspace:dir,target:resolve(dir,'target'),name:'metadata',id,
      manifest:{app:{id,name:'Metadata'},rust:false},crate:kind=>`metadata-${kind}`};
    const sourcePaths=[];
    for(const lane of ['a','b',null]) {
      const input=`${platform}/${lane??'default'}.txt`, text=`input-${lane??'default'}`;
      write(input,text);
      write(`${platform}/src/main.rs`,`fn main(){println!("{}",include_str!("../${lane??'default'}.txt"));}`);
      const receipt=buildBake(app,platform,target,{profile:'dev',output:resolve(dir,'bakes'),
        env:{EXACT_UPDATE_TRUST:'development',...(lane?{CARGO_BUILD_BUILD_DIR:resolve(dir,`intermediate-${lane}`)}:{})}});
      const executable=receipt.products.find(p=>basename(p.path)===`metadata-${platform}${process.platform==='win32'?'.exe':''}`);
      assert.ok(executable,'completed receipt identifies the actual native executable');
      const ran=spawnSync(executable.path,[],{encoding:'utf8'});
      assert.equal(ran.status,0,ran.stderr); assert.equal(ran.stdout.trim(),text);
      const paths=receipt.binary.inputs.map(f=>f.path);
      assert.ok(paths.includes(resolve(dir,input)),'new compiler include is in the completed receipt');
      assert.ok(sourcePaths.every(path=>!paths.includes(path)),'prior intermediate dep-info is not reused');
      sourcePaths.push(resolve(dir,input));
      write(input,`${text}-edited-after-bake`);
      const named=receipt.binary.inputs.find(f=>f.path===resolve(dir,input)).name;
      assert.ok(pendingBuildInputs(receipt).includes(named),'new included input stays freshness-tracked');
    }
  } finally {
    if(previous===undefined)delete process.env.CARGO_BUILD_BUILD_DIR;else process.env.CARGO_BUILD_BUILD_DIR=previous;
    rmSync(dir,{recursive:true,force:true});
  }
},180000);

test('E11 partial bakes select only their graph and production retains GPU binding',async()=>{
  const {bakeSelection,bindGpuProduct}=await import('./app.mjs');
  const graph={root:{id:'host'},surface:{id:'gpu'}};
  assert.deepEqual(bakeSelection(graph,'gpu'),[graph.surface]);
  assert.deepEqual(bakeSelection(graph,'host'),[graph.root]);
  assert.deepEqual(bakeSelection(graph),[graph.surface,graph.root]);
  assert.equal(bindGpuProduct('gpu-dev','development'),false);
  assert.equal(bindGpuProduct('gpu-dev','production'),true);
  assert.equal(bindGpuProduct('release','development'),true);
});

test('production native bakes for Apple and Android link with fat LTO; developer builds and other hosts keep the profile', async () => {
  const {wholeProgram} = await import('./app.mjs');
  const fat = ['--config', 'profile.release.lto="fat"'];
  for (const kind of ['apple', 'ios', 'macos', 'tvos', 'android']) assert.deepEqual(wholeProgram(kind, 'release'), fat, kind);
  for (const [kind, profile] of [['android', 'host-dev'], ['ios', 'host-dev'], ['android', 'size'], ['linux', 'release'], ['windows', 'release'], ['web', 'web']])
    assert.deepEqual(wholeProgram(kind, profile), [], `${kind} at ${profile}`);
});

test('D6 GPU modules bake beside the primary and each surface has one owner', async () => {
  const {bakeSelection, gpuModules, readManifest} = await import('./app.mjs');
  const {mkdtempSync, writeFileSync, rmSync} = await import('node:fs');
  const {resolve} = await import('node:path');
  const {tmpdir} = await import('node:os');
  const graph = {root:{id:'host'}, surface:{id:'gpu'}, modules:[{id:'world'}]};
  assert.deepEqual(bakeSelection(graph, 'gpu'), [graph.surface, graph.modules[0]]);
  assert.deepEqual(bakeSelection(graph), [graph.surface, graph.modules[0], graph.root]);
  assert.deepEqual(bakeSelection({...graph, surface:undefined}), [graph.modules[0], graph.root], 'a module needs no primary');
  assert.deepEqual(gpuModules({gpu:{modules:{world:['world', 'arena']}}}), [{name:'world', surfaces:['world', 'arena']}]);
  assert.deepEqual(gpuModules({}), []);
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-gpu-modules-'));
  const manifest = gpu => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify({name:'M', app:{id:'com.exact.m', name:'M'}, gpu})); return () => readManifest(dir, 'm'); };
  try {
    assert.equal(manifest({modules:{world:['world']}})().gpu.modules.world[0], 'world');
    assert.throws(manifest({modules:{a:['world'], b:['world']}}), /surface world is claimed by both a and b/);
    assert.throws(manifest({modules:{World:['world']}}), /gpu\.modules\.World: a module name/);
    assert.throws(manifest({modules:{world:[]}}), /gpu\.modules\.world: names no surface/);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('declared shader packs merge, reject duplicates and links, and preserve a rejected live candidate', async () => {
  const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, symlinkSync } = await import('node:fs');
  const { resolve } = await import('node:path');
  const { tmpdir } = await import('node:os');
  const { shaderFiles, copyShaders } = await import('./app.mjs');
  const { applyShaderTreeChange } = await import('../host/web/serve.mjs');
  const dir=mkdtempSync(resolve(tmpdir(),'exact-shader-packs-'));
  const app={dir,manifest:{gpu:{shaderRoots:['pack']}}}, target=resolve(dir,'dist/shaders');
  try {
    mkdirSync(resolve(dir,'gpu/shaders'),{recursive:true}); mkdirSync(resolve(dir,'pack'));
    writeFileSync(resolve(dir,'gpu/shaders/a.wgsl'),'a'); writeFileSync(resolve(dir,'pack/b.wgsl'),'b');
    copyShaders(app,target); writeFileSync(resolve(target,'stale.wgsl'),'old');
    copyShaders(app,target,{replace:true}); assert.ok(!existsSync(resolve(target,'stale.wgsl'))); assert.deepEqual([...shaderFiles(app).keys()].sort(),['a.wgsl','b.wgsl']);
    writeFileSync(resolve(dir,'shared.wgsl'),'shared');
    app.manifest.gpu.shaderPreludes={b:['shared.wgsl']};
    assert.equal(shaderFiles(app).get('b.wgsl').toString(),'shared\nb');
    writeFileSync(resolve(dir,'shared.wgsl'),'updated');
    applyShaderTreeChange(app,target);
    assert.equal(readFileSync(resolve(target,'b.wgsl'),'utf8'),'updated\nb');
    delete app.manifest.gpu.shaderPreludes;
    writeFileSync(resolve(dir,'pack/a.wgsl'),'collision');
    assert.throws(()=>applyShaderTreeChange(app,target),/duplicate shader/);
    assert.equal(readFileSync(resolve(target,'a.wgsl'),'utf8'),'a');
    rmSync(resolve(dir,'pack/a.wgsl')); rmSync(resolve(dir,'pack/b.wgsl'));
    const changed=applyShaderTreeChange(app,target);
    assert.ok(changed.files.some(f=>f.name==='b.wgsl'&&f.removed));
    assert.equal(readFileSync(resolve(target,'a.wgsl'),'utf8'),'a');
    // Windows directory junctions are unprivileged reparse points; file
    // symlinks require Developer Mode or an elevated process. Both must refuse.
    symlinkSync(resolve(dir,process.platform==='win32'?'gpu/shaders':'gpu/shaders/a.wgsl'),resolve(dir,'pack/b.wgsl'),process.platform==='win32'?'junction':'file');
    assert.throws(()=>shaderFiles(app));
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('a build env keeps the pinned toolchain and the checked Bun ahead of ambient ones', async () => {
  const { developmentBuildEnv } = await import('./app.mjs');
  const { readFileSync } = await import('node:fs');
  const { delimiter, dirname, resolve } = await import('node:path');
  const pinned = /^channel\s*=\s*"([^"]+)"/m.exec(readFileSync(resolve(import.meta.dir, '../rust-toolchain.toml'), 'utf8'))[1];
  const previous = process.env.RUSTUP_TOOLCHAIN;
  try {
    assert.equal(process.env.HERMES_LEAN_SYS_OFFLINE, '1');
    process.env.RUSTUP_TOOLCHAIN = 'stable'; // What `mise exec` exports.
    assert.equal(developmentBuildEnv().RUSTUP_TOOLCHAIN, undefined);
    for (const same of [pinned, `${pinned}-aarch64-apple-darwin`]) {
      process.env.RUSTUP_TOOLCHAIN = same;
      assert.equal(developmentBuildEnv().RUSTUP_TOOLCHAIN, same);
    }
    delete process.env.RUSTUP_TOOLCHAIN;
    const env = developmentBuildEnv();
    assert.equal(env.RUSTUP_TOOLCHAIN, undefined);
    assert.equal(env.PATH.split(delimiter)[0], dirname(process.execPath));
    assert.ok(Bun.which('cargo', {PATH:env.PATH}), 'the build inherits Cargo after spreading Windows Path');
    assert.equal(Object.keys(env).filter(key => key.toLowerCase() === 'path').length, 1);
    assert.equal(env.EXACT_UPDATE_TRUST, process.env.EXACT_UPDATE_TRUST ?? 'development');
    assert.equal(env.HERMES_LEAN_SYS_OFFLINE, '1');
  } finally { if (previous === undefined) delete process.env.RUSTUP_TOOLCHAIN; else process.env.RUSTUP_TOOLCHAIN = previous; }
});

test.each(['success', 'failure'])('web std preparation isolates Cargo homes after fetch %s', (outcome) => {
  const failFirst = outcome === 'failure';
  mkdirSync(resolve(import.meta.dir, '../target'), { recursive: true });
  const dir = mkdtempSync(resolve(import.meta.dir, '../target/web-std-'));
  try {
    // A child keeps the process mock out of other build tests. Both caches share one sysroot.
    const script = resolve(dir, 'check.mjs');
    writeFileSync(script, `
      import { mock } from 'bun:test';
      import * as processes from 'node:child_process';
      import assert from 'node:assert/strict';
      import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
      import { resolve } from 'node:path';
      const dir = ${JSON.stringify(dir)}, sysroot = resolve(dir, 'sysroot');
      const library = resolve(sysroot, 'lib/rustlib/src/rust/library');
      const first = resolve(dir, 'cargo-first'), second = resolve(dir, 'cargo-second');
      mkdirSync(library, { recursive: true });
      writeFileSync(resolve(library, 'Cargo.toml'), '[workspace]');
      writeFileSync(resolve(library, 'Cargo.lock'), '');
      const prepared = home => resolve(home, 'std-dependencies');
      if (${failFirst}) {
        mkdirSync(first, { recursive: true });
        writeFileSync(prepared(first), '');
      }
      const calls = [];
      let failures = ${failFirst ? 1 : 0};
      mock.module('node:child_process', () => ({ ...processes, spawnSync(command, args, { env }) {
        assert.equal(env.RUSTUP_TOOLCHAIN, 'nightly-2026-08-21');
        if (command === 'rustc') {
          assert.deepEqual(args, ['--print', 'sysroot', '--print', 'host-tuple']);
          return { status: 0, stdout: sysroot + '\\naarch64-apple-darwin\\n' };
        }
        assert.equal(command, 'cargo');
        assert.equal(env.HERMES_LEAN_SYS_OFFLINE, '1');
        assert.equal(args[args.indexOf('--manifest-path') + 1], resolve(library, 'Cargo.toml'));
        assert.ok(args.includes('--locked'));
        const home = env.CARGO_HOME;
        assert.ok(home === first || home === second);
        calls.push([home, args[0]]);
        if (args[0] === 'metadata') {
          assert.ok(args.includes('--offline'));
          return { status: existsSync(prepared(home)) ? 0 : 1 };
        }
        assert.equal(args[0], 'fetch');
        if (home === second && failures-- > 0) return { status: 1 };
        mkdirSync(home, { recursive: true });
        writeFileSync(prepared(home), '');
        return { status: 0 };
      }}));
      const { webToolchainEnv, WEB_TOOLCHAIN, WEB_SIZE } = await import(${JSON.stringify(resolve(import.meta.dir, 'app.mjs'))});
      const prepare = home => {
        const env = webToolchainEnv({ ...process.env, CARGO_HOME: home });
        assert.equal(env.CARGO_HOME, home);
        assert.equal(env.RUSTUP_TOOLCHAIN, WEB_TOOLCHAIN);
        assert.equal(env.EXACT_WEB_SIZE, WEB_SIZE);
      };
      prepare(first);
      assert.ok(existsSync(prepared(first)));
      assert.deepEqual(calls, ${failFirst ? "[[first, 'metadata']]" : "[[first, 'metadata'], [first, 'fetch']]"});
      writeFileSync(resolve(library, '.exact-fetched'), '');
      prepare(second);
      assert.deepEqual(calls.filter(([home]) => home === second), [[second, 'metadata'], [second, 'fetch']]);
      if (${failFirst}) {
        assert.equal(existsSync(prepared(second)), false);
        prepare(second);
        assert.deepEqual(calls.filter(([home]) => home === second),
          [[second, 'metadata'], [second, 'fetch'], [second, 'metadata'], [second, 'fetch']]);
      }
      assert.ok(existsSync(prepared(second)), 'the second Cargo home has its own std dependencies');
      const fetches = calls.filter(([, verb]) => verb === 'fetch').length;
      prepare(first);
      prepare(second);
      assert.equal(calls.filter(([, verb]) => verb === 'fetch').length, fetches);
    `);
    const result = spawnSync(process.execPath, ['--no-env-file', script], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('web remapping preserves game floating-point determinism on both web toolchains', async () => {
  const {wasmRemapFlags, WEB_TOOLCHAIN} = await import('./app.mjs');
  const workspace = fileURLToPath(new URL('../game/', import.meta.url));
  for (const toolchain of [null, WEB_TOOLCHAIN]) {
    const flags = manifest => JSON.parse(wasmRemapFlags({workspace,target:workspace+'target',manifest},toolchain)[1].replace(/^[^=]*=/,''));
    const game = flags({game:{}});
    assert.equal(game.filter(flag=>flag==='llvm-args=-fp-contract=off').length,1);
    assert.equal(game[game.indexOf('llvm-args=-fp-contract=off')-1],'-C');
    assert.ok(!flags({}).includes('llvm-args=-fp-contract=off'));
  }
}, 60_000);

test('setup accepts both official Binaryen release tags and package-manager version output', async () => {
  const {binaryenVersion} = await import('./exact.mjs');
  assert.equal(binaryenVersion('wasm-opt version 132 (version_132)\n'), 'version 132');
  assert.equal(binaryenVersion('wasm-opt version 132\n'), 'version 132');
  assert.notEqual(binaryenVersion('wasm-opt version 1320 (version_1320)'), 'version 132');
  assert.notEqual(binaryenVersion('missing'), 'version 132');
});

// Grow a Garden: a `git checkout` that rewrote unchanged files made the next
// drive refuse (`web build is stale`) though the proof's receipt (by content)
// was fresh. The staleness check now compares content digests.
test('rewriting an input with its own bytes does not make a web build stale', async () => {
  const { webChanges, webInputDigests } = await import('./agent-launch.mjs');
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync, utimesSync } = await import('node:fs');
  const { resolve } = await import('node:path');
  const { tmpdir } = await import('node:os');
  const dir = mkdtempSync(resolve(tmpdir(), 'web-freshness-'));
  try {
    const app = { dir: resolve(dir, 'app'), manifest: { game: {} } };
    const dist = resolve(dir, 'dist');
    mkdirSync(resolve(app.dir, 'logic/src'), { recursive: true });
    mkdirSync(dist);
    const source = resolve(app.dir, 'logic/src/lib.rs');
    writeFileSync(source, 'fn tick() {}\n');
    const marker = resolve(dist, '.exact-build.json');
    writeFileSync(marker, JSON.stringify({ exactBuild: 1, inputs: webInputDigests(app, false) }));
    const past = new Date(Date.now() - 60_000);
    utimesSync(marker, past, past);
    writeFileSync(source, 'fn tick() {}\n');
    assert.deepEqual(webChanges(dist, app).app, []);
    writeFileSync(source, 'fn tick() { edited(); }\n');
    assert.deepEqual(webChanges(dist, app).app, [source]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60000);

for (const assets of ['none', 'create', 'update', 'prune', 'create level', 'update level', 'prune level', 'release level']) test(`a wasm build keeps authored inputs (${assets === 'none' ? 'ordinary app' : assets + ' game assets'})`, async () => {
  const { resolveApp } = await import('./app.mjs');
  const { refuseStale, webChanges } = await import('./agent-launch.mjs');
  const target = resolve(import.meta.dir, '../target');
  mkdirSync(target, { recursive: true });
  const dir = realpathSync(mkdtempSync(resolve(target, 'web-build-inputs-')));
  const source = resolve(dir, 'app.contract'), dist = resolve(dir, 'target/web-dist');
  const operation = assets.split(' ')[0], level = assets.endsWith(' level');
  const game = assets !== 'none', edit = operation !== 'create';
  const asset = level ? 'levels/start.level.json' : 'strip.tex';
  const baked = level ? '{"seed":2}' : 'completed bake';
  const before = 'component App\n  view\n    text "MarketsFirst"\n';
  const after = edit ? before.replace('MarketsFirst', 'MarketsSecond') : before;
  const ceiling = process.env.GIT_CEILING_DIRECTORIES;
  // Keep the fixture in this worktree while giving its app walk the no-Git case.
  process.env.GIT_CEILING_DIRECTORIES = target;
  try {
    if (game) assert.notEqual(spawnSync('git', ['rev-parse', '--show-toplevel'], { cwd: dir }).status, 0);
    writeFileSync(source, before);
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Build inputs', app: { id: 'com.exact.build-inputs', name: 'Build inputs' }, rust: false,
      ...(game ? { game: { crate: 'build-inputs-logic', type: 'BuildInputs' } } : {}) }));
    if (game) {
      mkdirSync(resolve(dir, 'assets'), { recursive: true });
      writeFileSync(resolve(dir, 'assets/authored.txt'), 'authored before');
      writeFileSync(resolve(dir, 'config.level.json'), '{"seed":1}');
      writeFileSync(resolve(dir, 'assets/config.level.json'), '{"seed":1}');
      if (level) {
        mkdirSync(resolve(dir, 'levels'), { recursive: true });
        writeFileSync(resolve(dir, asset), baked);
        mkdirSync(resolve(dir, 'logic/src'), { recursive: true });
        writeFileSync(resolve(dir, 'logic/src/lib.rs'), `use exact_game::*;
#[derive(Default, Data)] pub struct Settings { pub seed: u64 }
pub struct BuildInputs;
impl Game for BuildInputs {
    const ID: &'static str = "build-inputs";
    ${operation === 'release' ? '' : `const LEVEL: Option<asset::Level> = Some(asset::Level::of::<Settings>("${asset}"));`}
    type Args = ();
    fn setup(_: &mut World, _: &()) {}
    fn tick(_: &mut World, _: &Input, _: &()) {}
}
`);
      } else if (operation !== 'prune') {
        mkdirSync(resolve(dir, 'art'), { recursive: true });
        writeFileSync(resolve(dir, 'art/strip.png'), 'art to bake');
      }
      if (['update', 'prune', 'release'].includes(operation)) {
        mkdirSync(dirname(resolve(dir, 'assets', asset)), { recursive: true });
        writeFileSync(resolve(dir, 'assets', asset), level ? '{"seed":1}' : 'previous bake');
        if (!level) writeFileSync(resolve(dir, '.baked-assets.json'), JSON.stringify({ 'strip.tex': createHash('sha256').update('previous bake').digest('hex') }));
        else {
          mkdirSync(resolve(dir, '.shells'), { recursive: true });
          writeFileSync(resolve(dir, '.shells/level.json'), JSON.stringify(asset));
        }
      }
    }
    // The child stubs compilation; --bake packages the receipt and writes the real marker.
    // Authored inputs change during baking or packaging; generated assets change after copying.
    const script = `
      import { mock } from 'bun:test';
      import { createHash } from 'node:crypto';
      import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
      import { resolve } from 'node:path';
      const appPath = ${JSON.stringify(resolve(import.meta.dir, 'app.mjs'))};
      const actual = await import(appPath);
      const servePath = ${JSON.stringify(resolve(import.meta.dir, '../host/web/serve.mjs'))};
      const serve = await import(servePath), cards = serve.publicFileCards;
      mock.module(servePath, () => ({ ...serve, publicFileCards(stage) {
        if (${game && edit}) writeFileSync(${JSON.stringify(resolve(dir, 'assets/authored.txt'))}, 'authored after');
        if (${operation === 'update'}) writeFileSync(${JSON.stringify(resolve(dir, 'assets', asset))}, 'edited during packaging');
        return cards(stage);
      } }));
      mock.module(appPath, () => ({ ...actual, buildBake(app, platform, target, { env }) {
        const started = Date.now();
        const source = resolve(app.dir, 'app.contract');
        const header = Buffer.alloc(36);
        header.write('EXPL'); header.writeUInt32LE(Buffer.byteLength(app.id), 32);
        const plan = Buffer.concat([header, Buffer.from(app.id), readFileSync(source)]);
        const inputs = { app: app.id, platform };
        const id = createHash('sha256').update('exact2 compatibility id v1\\n').update(JSON.stringify(inputs)).digest('hex').slice(0, 32);
        const receipt = { id, inputs, target, embedded: { assets: [], plan: {
          sha256: createHash('sha256').update(plan).digest('hex'), bytes: plan.length,
        } } };
        mkdirSync(env.EXACT_BAKE_OUTPUT, { recursive: true });
        writeFileSync(resolve(env.EXACT_BAKE_OUTPUT, platform + '-' + target + '.plan'), plan);
        writeFileSync(source, ${JSON.stringify(after)});
        if (${game}) {
          if (${edit}) writeFileSync(resolve(app.dir, 'assets/config.level.json'), '{"seed":2}');
          const asset = resolve(app.dir, 'assets', ${JSON.stringify(asset)});
          if (${operation === 'prune' || operation === 'release'}) {
            if (${operation === 'prune'}) rmSync(asset);
            else writeFileSync(asset, '{"seed":3}');
            if (!${level}) rmSync(resolve(app.dir, '.baked-assets.json'));
            else rmSync(resolve(app.dir, '.shells/level.json'));
          } else {
            mkdirSync(resolve(asset, '..'), { recursive: true });
            writeFileSync(asset, ${level ? `readFileSync(resolve(app.dir, ${JSON.stringify(asset)}))` : JSON.stringify(baked)});
            if (!${level}) writeFileSync(resolve(app.dir, '.baked-assets.json'), JSON.stringify({ 'strip.tex': createHash('sha256').update('completed bake').digest('hex') }));
            else {
              mkdirSync(resolve(app.dir, '.shells'), { recursive: true });
              writeFileSync(resolve(app.dir, '.shells/level.json'), ${JSON.stringify(JSON.stringify(asset))});
            }
          }
          if (${level && edit}) writeFileSync(resolve(app.dir, ${JSON.stringify(asset)}), '{"seed":3}');
          writeFileSync(resolve(app.dir, 'assets/exact-tables.json'), '{}');
        }
        receipt.embedded.assets = serve.listAssets(app.dir);
        writeFileSync(resolve(env.EXACT_BAKE_OUTPUT, platform + '-' + target + '.json'), JSON.stringify(receipt));
        return { started, binary: { sha256: 'fixture', inputs: [] } };
      } }));
      process.argv = [process.execPath, ${JSON.stringify(resolve(import.meta.dir, '../host/web/build.mjs'))}, ${JSON.stringify(dir)}, '--bake'];
      await import(${JSON.stringify(resolve(import.meta.dir, '../host/web/build.mjs'))});
    `;
    const built = spawnSync(process.execPath, ['-e', script], {
      env: { ...process.env, EXACT_APP_DIR: dir, EXACT_WEB_DIST: dist, CARGO_TARGET_DIR: resolve(dir, 'target') },
      encoding: 'utf8', timeout: 30_000,
    });
    assert.equal(built.status, 0, built.stderr);
    assert.ok(readFileSync(resolve(dist, 'app.plan')).includes(Buffer.from('MarketsFirst')));
    assert.equal(readFileSync(source, 'utf8'), after);
    const app = resolveApp(dir), markerPath = resolve(dist, '.exact-build.json');
    // A long packaging step makes the marker newer than the edit; no elapsed-time wait is needed.
    const finished = new Date(Date.now() + 10_000);
    utimesSync(markerPath, finished, finished);
    const shown = game ? source : relative(resolve(import.meta.dir, '..'), source);
    const changed = !edit ? [] : game ? [
      source, resolve(dir, 'assets/authored.txt'), resolve(dir, 'assets/config.level.json'),
      ...(['update', 'release'].includes(operation) ? [resolve(dir, 'assets', asset)] : []),
      ...(level ? [resolve(dir, asset)] : []),
    ] : [shown];
    assert.deepEqual(webChanges(dist, app).app, changed);
    if (edit) assert.throws(() => refuseStale('web', markerPath, webChanges(dist, app).all, 'bun host/web/build.mjs --wasm'), /web build is stale/);
    const marker = JSON.parse(readFileSync(markerPath, 'utf8'));
    assert.equal(marker.inputs[shown], createHash('sha1').update(before).digest('hex'));
    if (game) assert.equal(marker.inputs[resolve(dir, 'assets/config.level.json')], createHash('sha1').update('{"seed":1}').digest('hex'));
    if (level) assert.equal(marker.inputs[resolve(dir, asset)], createHash('sha1').update(baked).digest('hex'));
    if (operation === 'prune') assert.equal(marker.inputs[resolve(dir, 'assets', asset)], null);
    if (operation === 'release') assert.equal(marker.inputs[resolve(dir, 'assets', asset)], createHash('sha1').update('{"seed":1}').digest('hex'));
    assert.ok(marker.inputsAt <= JSON.parse(readFileSync(resolve(dist, 'bake.json'), 'utf8')).started);
    writeFileSync(source, before);
    if (game) writeFileSync(resolve(dir, 'assets/authored.txt'), 'authored before');
    if (game) writeFileSync(resolve(dir, 'assets/config.level.json'), '{"seed":1}');
    if (level) writeFileSync(resolve(dir, asset), baked);
    if (operation === 'update') writeFileSync(resolve(dir, 'assets', asset), baked);
    if (operation === 'release') writeFileSync(resolve(dir, 'assets', asset), '{"seed":1}');
    assert.deepEqual(webChanges(dist, app).all, []);
    if (game && operation !== 'prune') {
      assert.equal(readFileSync(resolve(dist, 'assets', asset), 'utf8'), operation === 'release' ? '{"seed":3}' : baked);
      writeFileSync(resolve(dir, 'assets', asset), 'edited after build');
      assert.deepEqual(webChanges(dist, app).app, [resolve(dir, 'assets', asset)]);
    }
  } finally {
    delete process.env.EXACT_APP_DIR;
    if (ceiling === undefined) delete process.env.GIT_CEILING_DIRECTORIES;
    else process.env.GIT_CEILING_DIRECTORIES = ceiling;
    rmSync(dir, { recursive: true, force: true });
  }
}, 60_000);

// LLP 1012.001.000 D1: a drive's check compares every receipt input's bytes with its digest, so a file replaced with
// other bytes of the same length, its mtime put back (`cp -p`, `rsync -a`), is still found.
test('a receipt input is found changed by its bytes, whatever its size and mtime', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-pending-'));
  try {
    const path = resolve(dir, 'a.swift');
    writeFileSync(path, 'let a = 1\n');
    const at = statSync(path), build = { binary: { inputs: [{ name: 'a.swift', path, sha256: createHash('sha256').update('let a = 1\n').digest('hex') }], missing: [], directories: [] } };
    assert.deepEqual(pendingBuildInputs(build), []);
    writeFileSync(path, 'let b = 2\n');
    utimesSync(path, at.atime, at.mtime);
    assert.deepEqual(pendingBuildInputs(build), ['a.swift']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('bake asset comparison preserves diagnostics without repeated inventory scans', async () => {
  const { verifyBakeFiles } = await import('./app.mjs');
  const plan = Buffer.from('plan'), count = 8000;
  let reads = 0;
  const cards = () => Array.from({ length: count }, (_, i) => ({
    get name() { reads++; return `assets/${String(i).padStart(6, '0')}`; }, sha256: 'digest', bytes: 1,
  }));
  const assets = cards(), receipt = { embedded: { plan: { sha256: createHash('sha256').update(plan).digest('hex'), bytes: plan.length }, assets: cards() } };
  verifyBakeFiles(receipt, plan, assets);
  assert.ok(reads < count * 20, `${reads} asset-name reads for ${count} matching assets`);
  const damaged = [...assets];
  damaged[1] = { name: assets[1].name, sha256: 'changed', bytes: 1 };
  damaged[2] = { name: assets[2].name, sha256: 'digest', bytes: 2 };
  damaged.splice(3, 1);
  damaged.push({ name: 'extra', sha256: 'digest', bytes: 1 });
  assert.throws(() => verifyBakeFiles(receipt, plan, damaged), e => e.rebuild && e.message.includes('static files (assets/000001, assets/000002, extra, 1 more)'));
  assert.throws(() => verifyBakeFiles(receipt, Buffer.from('edit'), assets), e => e.rebuild && e.message.includes('plan differs'));
});

test('build output claims exclude contenders and recover after an owner is killed', async () => {
  const { createInterface } = await import('node:readline');
  const { claimBuildOutput } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-output-claim-'));
  const path = resolve(dir, 'build.lock'), app = { id: 'fixture', dir };
  const children = [];
  const contender = async () => {
    const source = `import {claimBuildOutput} from ${JSON.stringify(import.meta.resolve('./app.mjs'))};
      import {createInterface} from 'node:readline';
      let release; console.log('ready');
      for await (const command of createInterface({input:process.stdin})) {
        if(command==='claim') { try { release=claimBuildOutput(${JSON.stringify(app)},${JSON.stringify(path)}); console.log('held'); } catch(e) { if(!e.message.startsWith('Apple build busy')) throw e; console.log('busy'); } }
        if(command==='release') { release?.(); console.log('released'); }
      }`;
    const child = spawn(process.execPath, ['--eval', source], { stdio: ['pipe', 'pipe', 'pipe'] });
    children.push(child);
    let errors = ''; child.stderr.on('data', chunk => errors += chunk);
    const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    const read = async () => { const line = await lines.next(); assert.equal(line.done, false, errors); return line.value; };
    const ended = new Promise(resolve => child.once('close', resolve));
    assert.equal(await read(), 'ready');
    return { child, read, ended, send(command) { child.stdin.write(command + '\n'); } };
  };
  try {
    writeFileSync(path, JSON.stringify({ pid: 2 ** 22 + 12345, started: '2000-01-01T00:00:00Z' }));
    const [a, b] = await Promise.all([contender(), contender()]);
    a.send('claim'); b.send('claim');
    const answers = await Promise.all([a.read(), b.read()]);
    assert.deepEqual([...answers].sort(), ['busy', 'held']);
    const [owner, waiter] = answers[0] === 'held' ? [a, b] : [b, a];
    const inode = statSync(`${path}.flock`).ino;
    waiter.send('release'); assert.equal(await waiter.read(), 'released');
    assert.throws(() => claimBuildOutput(app, path), /Apple build busy/);
    owner.child.kill('SIGKILL'); await owner.ended;
    waiter.send('claim'); assert.equal(await waiter.read(), 'held');
    assert.throws(() => claimBuildOutput(app, path), /Apple build busy/);
    waiter.send('release'); assert.equal(await waiter.read(), 'released');
    const release = claimBuildOutput(app, path);
    waiter.send('release'); assert.equal(await waiter.read(), 'released');
    assert.throws(() => claimBuildOutput(app, path), /Apple build busy/);
    release(); release();
    assert.equal(existsSync(path), false);
    assert.equal(statSync(`${path}.flock`).ino, inode);
    // A killed owner may leave an incomplete diagnostic record; the OS lock still governs ownership.
    writeFileSync(path, '{'); claimBuildOutput(app, path)();
  } finally {
    await Promise.all(children.map(child => child.exitCode !== null || child.signalCode !== null ? null : new Promise(resolve => { child.once('close', resolve); child.kill('SIGKILL'); })));
    rmSync(dir, { recursive: true, force: true });
  }
}, 15000);

// An unreadable claim or process start time cannot establish that the holder has exited or its PID was reused.
test('a claim is live unless its holder is shown exited or reused', async () => {
  const { claimLive } = await import('./app.mjs');
  const claim = (pid, started) => JSON.stringify({ pid, started });
  assert.equal(claimLive(claim(process.pid, new Date().toISOString())), true, 'this process, claimed now');
  assert.equal(claimLive(claim(process.pid, '2000-01-01T00:00:00Z')), false, 'a pid started after its claim is reused');
  assert.equal(claimLive(claim(2 ** 22 + 12345, new Date().toISOString())), false, 'no such process');
  assert.equal(claimLive('not json'), true, 'unreadable: never guess');
  assert.equal(claimLive(claim(process.pid, 'not a date')), true, 'an undated claim of a running pid');
});

// Run after a gallery build: exercise the completed receipt and the real CLI refusal.
test.skipIf(!process.env.EXACT_WIDGET_GALLERY_TEST)('a snapshot driver edit makes a gallery build stale', async () => {
  const { resolveApp, readBuilds } = await import('./app.mjs');
  const app = resolveApp('caltrain'), source = resolve(import.meta.dir, '../host/apple/Sources/ExactSnapshot/SnapshotDriver.swift');
  const receipt = readBuilds(app).find(build => build.compat.inputs.platform === 'macos' && build.binary.inputs.some(input => input.path === source));
  assert.ok(receipt, 'build Caltrain with --widget-gallery first');
  const before = readFileSync(source);
  try {
    writeFileSync(source, Buffer.concat([before, Buffer.from('\n// freshness test\n')]));
    const name = receipt.binary.inputs.find(input => input.path === source).name;
    assert.ok(pendingBuildInputs(receipt).includes(name));
    const refused = spawnSync(process.execPath, [resolve(import.meta.dir, 'agent.mjs'), 'macos', 'widget', 'next-trains', 'tree'], { encoding: 'utf8' });
    assert.equal(refused.status, 3, refused.stderr);
    assert.match(refused.stderr, /SnapshotDriver.swift/);
  } finally { writeFileSync(source, before); }
});

test('game web, release and unwired gpu-dev bakes never open registry generations', () => {
  const dir=mkdtempSync(resolve('/tmp','exact-unwired-cache-'));
  try {
    // The child isolates module mocks. Only process execution and receipt
    // collection are stubbed; every selected GPU and host build must finish.
    const script=resolve(dir,'check.mjs');
    writeFileSync(script, `import {mock} from 'bun:test';
      import assert from 'node:assert/strict';
      import {readFileSync} from 'node:fs';
      let calls=0, lints=0, receipts=0;
      const builds=[];
      mock.module(${JSON.stringify(resolve(import.meta.dir,'../game/app/crates.mjs'))},()=>({keptGameCrates(){calls++;return {take(){calls++;},keep(){calls++;}};}}));
      mock.module(${JSON.stringify(resolve(import.meta.dir,'../game/app/shells.mjs'))},()=>({gameDefaults(){},prepareGame(){},lintGame(){lints++;}}));
      const appPath=${JSON.stringify(resolve(import.meta.dir,'app.mjs'))};
      globalThis.stubBuild=(command,args)=>{
        assert.equal(command,'cargo');
        const pkg=args[args.indexOf('-p')+1]; builds.push(pkg);
        return {stdout:JSON.stringify({reason:'compiler-artifact',package_id:pkg,filenames:[dir+'/'+pkg+'.so']})};
      };
      globalThis.stubReceipt=(app,platform,target,graph,messages,roots)=>{
        assert.deepEqual(roots.map(r=>r.package),[graph.surface.id,graph.root.id]);
        assert.equal(messages.length,2); receipts++;
        return {products:messages.flatMap(m=>m.filenames)};
      };
      Bun.plugin({name:'stub-bake-execution',setup(builder){
        builder.onLoad({filter:/[\\/]scripts[\\/]app\\.mjs$/},({path})=>{
          assert.equal(path,appPath);
          const source=readFileSync(path,'utf8')
            .replace(/(function buildCommand\\([^\\n]+\\{)/,'$1 return globalThis.stubBuild(...arguments);')
            .replace(/(function completeBuild\\([^\\n]+\\{)/,'$1 return globalThis.stubReceipt(...arguments);');
          return {contents:source,loader:'js'};
        });
      }});
      const {buildBake}=await import(appPath);
      const dir=${JSON.stringify(dir)};
      for(const [platform,profile,target] of [['web','web','wasm32-unknown-unknown'],['linux','release',${JSON.stringify(bakeTarget('linux'))}],['linux','gpu-dev',${JSON.stringify(bakeTarget('linux'))}]]) {
        const name='fixture-'+platform;
        const host={id:name,name,targets:[{kind:[platform==='web'?'cdylib':'bin'],name:'fixture'}]};
        const gpu={id:'fixture-gpu',name:'fixture-gpu',targets:[{kind:['cdylib'],name:'surface'}]};
        const metadata={workspace_root:dir,packages:[host,gpu],resolve:{nodes:[{id:host.id,deps:[]},{id:gpu.id,deps:[]}]}};
        const app={dir,workspace:dir,target:dir+'/target',hasGpu:true,manifest:{app:{id:'fixture'},rust:false,game:{}},crate:kind=>'fixture-'+kind,prepare:()=>metadata};
        const before=builds.length;
        const receipt=buildBake(app,platform,target,{kind:platform,profile,output:dir+'/bakes',env:{EXACT_UPDATE_TRUST:'development'}});
        assert.deepEqual(builds.slice(before),[gpu.name,host.name]);
        assert.equal(receipt.products.length,2); assert.equal(calls,0);
      }
      assert.equal(receipts,3); assert.equal(lints,2); assert.equal(calls,0);
    `);
    const result=spawnSync(process.execPath,[script],{cwd:dir,encoding:'utf8'});
    assert.equal(result.status,0,result.stderr);
  } finally {rmSync(dir,{recursive:true,force:true});}
});

// A setting's source is a bake input (LLP 1153 D4): the same value from the environment once and from Bun's loading
// of `.env.development`, which is no source, the next time is another bake.
test('a linked bake follows a setting whose value stays but whose source moved', async () => {
  const { bakeOutside, linkedBakeDir } = await import('./linked-plan.mjs');
  const { createApp } = await import('../game/new.mjs');
  const root = mkdtempSync(resolve(import.meta.dir, '../target/linked-settings-')), dir = resolve(root, 'app');
  try {
    createApp(dir);
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...manifest, env: ['API_BASE_URL'] }));
    writeFileSync(resolve(dir, 'app.ts'), readFileSync(resolve(dir, 'app.ts'), 'utf8').replace("export const grants = '';", "const API = import.meta.env.API_BASE_URL ?? 'http://b.example';\nexport const grants = `net.fetch ${API}`;"));
    writeFileSync(resolve(dir, '.env.development'), 'API_BASE_URL=http://a.example\n');
    const target = spawnSync('rustc', ['-vV'], { encoding: 'utf8' }).stdout.match(/^host: (.*)$/m)[1];
    // With what the bake watches named, as every builder names it (`bakeWatchEnv`): what is absent is not watched.
    const { bakeWatchEnv } = await import('./app.mjs');
    const env = { ...process.env, ...bakeWatchEnv({ dir, manifest: { ...manifest, env: ['API_BASE_URL'] } }), EXACT_BAKE_OUTPUT: resolve(root, 'bakes'), API_BASE_URL: 'http://a.example', EXACT_BUN_LOADED: '' };
    for (const name of ['EXACT_APP_DIR', 'EXACT_BACKEND_ORIGIN', 'CARGO_TARGET_DIR']) delete env[name];
    const app = { dir, name: 'app' }, baked = linkedBakeDir(env, 'macos', target);
    const inlined = () => JSON.parse(readFileSync(resolve(baked, 'app.module.json'), 'utf8')).env.API_BASE_URL;
    const bakedAt = () => JSON.parse(readFileSync(resolve(baked, 'inputs.json'), 'utf8')).at;
    bakeOutside(app, 'macos', target, env);
    assert.deepEqual(inlined(), { value: 'http://a.example', source: 'environment' });
    const first = bakedAt();
    bakeOutside(app, 'macos', target, env);
    assert.equal(bakedAt(), first, 'nothing changed: the bake is reused');
    // The command was not given it this time: Bun loaded the same value, and the note says so.
    env.EXACT_BUN_LOADED = 'API_BASE_URL';
    bakeOutside(app, 'macos', target, env);
    assert.deepEqual(inlined(), { value: 'http://b.example', source: 'default' });
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 600_000);

// The cache boundary needs real Cargo: a script rerun recompiles its crate,
// even when the script leaves the generated Rust file unchanged.
test.skipIf(!process.env.EXACT_RUST_BAKE_TEST)('a linked Rust bake changes its plan without compiling its archive', async () => {
  const { bakeOutside, linkedBakeDir, linkable, recordRustBake } = await import('./linked-plan.mjs');
  const root = resolve(import.meta.dir, '..'), dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-linked-rust-')));
  const write = (name, bytes) => { mkdirSync(dirname(resolve(dir, name)), {recursive:true}); writeFileSync(resolve(dir, name), bytes); };
  try {
    write('rust-toolchain.toml', readFileSync(resolve(root, 'rust-toolchain.toml')));
    write('Cargo.lock', readFileSync(resolve(root, 'Cargo.lock')));
    write('Cargo.toml', `[package]\nname="linked-rust-fixture"\nversion="0.1.0"\nedition="2024"\n[workspace]\n[patch.crates-io]\ntaffy={path=${JSON.stringify(resolve(root, 'vendor/taffy'))}}\n[build-dependencies]\nexact-bake={path=${JSON.stringify(resolve(root, 'bake'))}}\n`);
    write('data/Cargo.toml', '');
    write('src/lib.rs', 'include!(concat!(env!("OUT_DIR"), "/entry.rs"));');
    write('app.contract', 'first');
    write('app.json', 'nextTrains');
    write('data/src/lib.rs', `pub fn next_trains() -> &'static str { "nextTrains" }`);
    write('build.rs', `#[path="data/src/lib.rs"] mod data;
    fn main() {
      if exact_bake::take_linked(&["entry.rs"]).unwrap() { return; }
      println!("cargo:rerun-if-changed=app.contract");
      println!("cargo:rerun-if-changed=app.json");
      assert_eq!(std::fs::read_to_string("app.json").unwrap(), data::next_trains(), "unknown widget refresh");
      println!("cargo:rerun-if-env-changed=FIXTURE_BINARY");
      let out=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
      std::fs::write(out.join("app.plan"), std::fs::read("app.contract").unwrap()).unwrap();
      std::fs::write(out.join("compat.json"), "{}").unwrap();
      std::fs::write(out.join("app.hbc"), []).unwrap();
      let entry=format!("pub const VALUE: &str = {:?};", std::env::var("FIXTURE_BINARY").unwrap());
      if std::fs::read(out.join("entry.rs")).ok().as_deref()!=Some(entry.as_bytes()) {
        std::fs::write(out.join("entry.rs"), entry).unwrap();
      }
    }`);
    const app = { dir, target: resolve(root, 'target'), name: 'linked-rust-fixture' };
    const pkg = { targets: [{kind:['custom-build'],src_path:resolve(dir,'build.rs')},{kind:['lib'],src_path:resolve(dir,'src/lib.rs')}] };
    assert.equal(linkable(app, pkg), true);
    const target = spawnSync('rustc',['-vV'],{encoding:'utf8'}).stdout.match(/^host: (.*)$/m)[1];
    const env = {...process.env, EXACT_BAKE_OUTPUT:resolve(dir,'bakes'), FIXTURE_BINARY:'one'};
    env.EXACT_BAKED = linkedBakeDir(env,'macos',target);
    const cargo = (archive = false) => {
      const result=spawnSync('cargo',[...(archive ? ['rustc','--crate-type','staticlib'] : ['build']),'--offline','--manifest-path',resolve(dir,'Cargo.toml'),'--target-dir',app.target,'--message-format=json','-vv'],{cwd:dir,env,encoding:'utf8',maxBuffer:64*1024*1024});
      assert.equal(result.status,0,result.stderr);
      return {...result.stdout.split('\n').filter(line=>line.startsWith('{')).map(line=>JSON.parse(line)).find(row=>row.reason==='compiler-artifact'&&row.target.name==='linked_rust_fixture'), diagnostic: result.stderr};
    };
    const bake = () => bakeOutside(app,'macos',target,env,() => cargo());
    cargo(); // Compile and run the baker through the rlib first.
    const first=cargo(true), library=first.filenames.find(name=>name.endsWith('.a'));
    recordRustBake(env);
    const watches=JSON.parse(readFileSync(resolve(env.EXACT_BAKED,'inputs.json'),'utf8'));
    assert.ok(watches.paths.includes(resolve(dir,'app.contract')),'the first build is ready for the driver freshness check');
    assert.ok(Number.isFinite(watches.at));
    const initial=statSync(library,{bigint:true}).mtimeNs;
    write('app.contract','second');
    bake();
    assert.equal(readFileSync(resolve(env.EXACT_BAKED,'app.plan'),'utf8'),'second');
    const unchanged=cargo(true);
    assert.equal(unchanged.fresh,true,unchanged.diagnostic);
    assert.equal(statSync(library,{bigint:true}).mtimeNs,initial);
    write('app.contract','first');
    const restored = new Date(0), source = resolve(dir, 'app.contract');
    utimesSync(source, restored, restored);
    bake();
    assert.equal(readFileSync(resolve(env.EXACT_BAKED,'app.plan'),'utf8'),'first');
    assert.equal(statSync(source).mtimeMs,restored.getTime(),'rebaking must preserve the restored source mtime');
    assert.equal(cargo(true).fresh,true,'restoring a plan must keep the compiled archive');
    const inputs = readFileSync(resolve(env.EXACT_BAKED,'inputs.json'),'utf8');
    bake();
    assert.equal(readFileSync(resolve(env.EXACT_BAKED,'inputs.json'),'utf8'),inputs,'an unchanged linked bake must keep its record');
    env.FIXTURE_BINARY='two';
    bake();
    assert.equal(cargo(true).fresh,false,'a generated Rust change must still compile');
    assert.match(readFileSync(resolve(env.EXACT_BAKED,'entry.rs'),'utf8'),/two/);
    write('app.json','upcomingTrains');
    write('data/src/lib.rs', `pub fn next_trains() -> &'static str { "upcomingTrains" }`);
    bake();
    assert.equal(cargo(true).fresh,false,'the coordinated Rust edit compiles');
    recordRustBake(env);
    write('app.json','unknownRefresh');
    assert.throws(bake,/unknown widget refresh/,'a refresh absent from the current Rust implementation is rejected');
  } finally { rmSync(dir,{recursive:true,force:true}); }
},300000);
