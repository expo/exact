import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { authoredWorkspace, claimTargetCrates, declaredWorkspace, linkedCrate, physicalPath, targetRustflags, writtenPath } from './cargo-check.mjs';
import { rustRemapFlags, wasmRemapFlags } from './app.mjs';

// Cargo refuses to merge an array given with --config into a configured string ("expected string, but found array"),
// so a target's added flags follow the form the app's configuration gives that target.
test('added target rustflags join a configured string or array', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-rustflags-'));
  try {
    const env = { CARGO_HOME: resolve(dir, 'home') }, cwd = resolve(dir, 'app');
    mkdirSync(resolve(cwd, '.cargo'), { recursive: true });
    const config = (rustflags) => writeFileSync(resolve(cwd, '.cargo/config.toml'), `[target.wasm32-unknown-unknown]\nrustflags = ${rustflags}\n`);
    assert.deepEqual(targetRustflags('wasm32-unknown-unknown', ['--cfg=a', '--remap-path-prefix=/a b=/x'], { cwd, env }),
      ['--config', 'target.wasm32-unknown-unknown.rustflags=["--cfg=a","--remap-path-prefix=/a b=/x"]']);
    config('["-C", "debuginfo=0"]');
    assert.deepEqual(targetRustflags('wasm32-unknown-unknown', ['--cfg=a'], { cwd, env }), ['--config', 'target.wasm32-unknown-unknown.rustflags=["--cfg=a"]']);
    config('" -C debuginfo=0 "');
    assert.deepEqual(targetRustflags('wasm32-unknown-unknown', ['--cfg=a'], { cwd, env }), ['--config', 'target.wasm32-unknown-unknown.rustflags="-C debuginfo=0 --cfg=a"']);
    // Another target's string is not this one's.
    assert.deepEqual(targetRustflags('aarch64-apple-darwin', ['--cfg=a'], { cwd, env }), ['--config', 'target.aarch64-apple-darwin.rustflags=["--cfg=a"]']);
    // A string cannot carry a flag with a space: refused, naming the file and the array to write.
    assert.throws(() => targetRustflags('wasm32-unknown-unknown', ['--remap-path-prefix=/a b=/x'], { cwd, env }),
      { message: `${resolve(cwd, '.cargo/config.toml')}: [target.wasm32-unknown-unknown] rustflags is a string, which Cargo splits at whitespace, so the build cannot add --remap-path-prefix=/a b=/x to it; write it as an array, rustflags = ["-C","debuginfo=0"]` });
    // The nearest configuration's string is the one Cargo keeps.
    mkdirSync(resolve(cwd, 'inner/.cargo'), { recursive: true });
    writeFileSync(resolve(cwd, 'inner/.cargo/config.toml'), '[target.wasm32-unknown-unknown]\nrustflags = "--cfg inner"\n');
    assert.deepEqual(targetRustflags('wasm32-unknown-unknown', ['--cfg=a'], { cwd: resolve(cwd, 'inner'), env }), ['--config', 'target.wasm32-unknown-unknown.rustflags="--cfg inner --cfg=a"']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A web build's wasm remap is such a flag: an app configuring its wasm rustflags as a string failed in Cargo's merge.
test('a wasm build takes its remap over the app\'s configured wasm rustflags, a string or an array', () => {
  // As the temporary directory is named, which on macOS is a link (/var -> /private/var) Cargo reads through
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-wasm-config-'));
  const workspace = resolve(dir, 'app'), previousRoot = process.env.EXACT_SOURCE_ROOT;
  try {
    process.env.EXACT_SOURCE_ROOT = dir;
    mkdirSync(resolve(workspace, 'src'), { recursive: true });
    mkdirSync(resolve(workspace, '.cargo'), { recursive: true });
    // exact2's toolchain, which a generated workspace carries (app-shells.mjs), not the machine's rustup default
    writeFileSync(resolve(workspace, 'rust-toolchain.toml'), readFileSync(new URL('../rust-toolchain.toml', import.meta.url), 'utf8'));
    mkdirSync(resolve(dir, 'shared/src'), { recursive: true });
    writeFileSync(resolve(dir, 'shared/Cargo.toml'), '[workspace]\n[package]\nname="shared"\nversion="0.1.0"\nedition="2021"\n');
    writeFileSync(resolve(dir, 'shared/src/lib.rs'), 'pub fn path() -> &\'static str { file!() }\n');
    writeFileSync(resolve(workspace, 'Cargo.toml'), '[package]\nname = "configwasm"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\ncrate-type = ["cdylib"]\n\n[workspace]\n[dependencies]\nshared={path="../shared"}\n');
    // The symbol exists only when the configured flags reached rustc; the source path only without the remap.
    writeFileSync(resolve(workspace, 'src/lib.rs'), '#[cfg(configured)]\n#[no_mangle]\npub extern "C" fn exact_configured_marker(at: usize) -> u8 {\n    file!().as_bytes()[at]\n}\n#[no_mangle]\npub extern "C" fn shared_path_byte(at: usize) -> u8 { shared::path().as_bytes()[at] }\n');
    const app = { workspace, target: resolve(dir, 'target'), manifest: {} };
    for (const rustflags of ['"--cfg configured"', '["--cfg", "configured"]']) {
      writeFileSync(resolve(workspace, '.cargo/config.toml'), `[target.wasm32-unknown-unknown]\nrustflags = ${rustflags}\n`);
      const cargo = spawnSync('cargo', ['build', '--offline', ...wasmRemapFlags(app), '--lib', '--target', 'wasm32-unknown-unknown'],
        { cwd: workspace, encoding: 'utf8', env: { ...process.env, CARGO_TARGET_DIR: app.target } });
      assert.equal(cargo.status, 0, `${rustflags}: ${cargo.stderr}`);
      const bytes = readFileSync(resolve(dir, 'target/wasm32-unknown-unknown/debug/configwasm.wasm')).toString('latin1');
      assert.ok(bytes.includes('exact_configured_marker'), `${rustflags}: built with the configured flags`);
      assert.ok(!bytes.includes(dir), `${rustflags}: built with the remap`);
      assert.ok(bytes.includes('source/shared/src/lib.rs'), `${rustflags}: external dependency keeps its relative source path`);
    }
  } finally {
    if (previousRoot === undefined) delete process.env.EXACT_SOURCE_ROOT; else process.env.EXACT_SOURCE_ROOT = previousRoot;
    rmSync(dir, { recursive: true, force: true });
  }
}, 300000);

test('two apps whose crates share names cannot share a target directory while both exist', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-target-crates-')));
  try {
    const target = resolve(dir, 'shared'), one = resolve(dir, 'one/budget'), two = resolve(dir, 'two/budget');
    for (const app of [one, two]) { mkdirSync(app, { recursive: true }); writeFileSync(resolve(app, 'app.contract'), ''); }
    claimTargetCrates(target, 'budget', one);
    claimTargetCrates(target, 'budget', one);
    const alias = resolve(dir, 'alias');
    symlinkSync(one, alias, 'dir');
    claimTargetCrates(target, 'budget', alias);
    claimTargetCrates(target, 'linked', alias);
    assert.equal(readFileSync(resolve(target, '.exact-crates/linked'), 'utf8'), `${one}\n`);
    claimTargetCrates(target, 'other', two);
    assert.throws(() => claimTargetCrates(target, 'budget', two), new RegExp(`holds the crates budget-\\* of the app in ${one}, and the app in ${two} names its crates the same`));
    rmSync(one, { recursive: true });
    claimTargetCrates(target, 'budget', two);
    assert.equal(readFileSync(resolve(target, '.exact-crates/budget'), 'utf8'), `${two}\n`);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('workspace discovery follows a linked app to its physical ancestors', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-linked-workspace-')));
  const repo = resolve(dir, 'repo'), app = resolve(repo, 'apps/mobile'), alias = resolve(dir, 'links/mobile');
  try {
    mkdirSync(app, { recursive: true });
    mkdirSync(resolve(dir, 'links'));
    symlinkSync(app, alias, 'dir');
    writeFileSync(resolve(repo, 'Cargo.toml'), '[workspace]\nmembers=["apps/mobile"]\n');
    writeFileSync(resolve(app, 'Cargo.toml'), '[package]\nname="mobile"\nversion="0.1.0"\n');
    assert.equal(authoredWorkspace(alias), repo);
    writeFileSync(resolve(app, 'Cargo.toml'), '[package]\nworkspace="../.."\nname="mobile"\nversion="0.1.0"\n');
    assert.equal(authoredWorkspace(alias), repo);
    assert.equal(declaredWorkspace(alias, [alias]), repo);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// An app's data crate may be another app's, which its own crates link by path, as Unsaid's is Hello's: given its own
// path, renamed to it, or inherited from the workspace under its name or another, from a host crate and not a
// test's dev-dependency or another crate; none links it; two host crates link it from different directories.
test('the crate an app\'s own crates link by path is found by its package name', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-linked-crate-'));
  try {
    const write = (path, text) => { mkdirSync(resolve(dir, path, '..'), { recursive: true }); writeFileSync(resolve(dir, path), text); };
    write('Cargo.toml', '[workspace]\nmembers = ["apps/a/android"]\n[workspace.dependencies]\nshared-data = { path = "shared/data" }\n');
    write('apps/a/app.json', '{}');
    write('apps/a/android/Cargo.toml', '[package]\nname = "a-android"\n[target.\'cfg(target_os = "android")\'.dependencies]\nhello-data = { path = "../../hello/data" }\n[dependencies]\nlogic = { package = "b-logic", path = "../../b/logic" }\nshared-data = { workspace = true }\n[dev-dependencies]\nfixture = { package = "hello-data", path = "../fixtures/hello" }\n');
    const app = resolve(dir, 'apps/a');
    assert.equal(linkedCrate(app, 'hello-data'), resolve(dir, 'apps/hello/data'));
    assert.equal(linkedCrate(app, 'b-logic'), resolve(dir, 'apps/b/logic'));
    assert.equal(linkedCrate(app, 'shared-data'), resolve(dir, 'shared/data'));
    assert.equal(linkedCrate(app, 'weather-data'), null);
    // A dependency inherited under another name, renamed in the workspace, is the one the host builds.
    write('apps/c/android/Cargo.toml', '[package]\nname = "c-android"\n[dependencies]\nlogic = { workspace = true }\n');
    writeFileSync(resolve(dir, 'Cargo.toml'), '[workspace]\nmembers = ["apps/a/android", "apps/c/android"]\n[workspace.dependencies]\nshared-data = { path = "shared/data" }\nlogic = { package = "c-data", path = "shared/c" }\n');
    assert.equal(linkedCrate(resolve(dir, 'apps/c'), 'c-data'), resolve(dir, 'shared/c'));
    // A host crate that is its own workspace's root inherits from its own manifest, and another joins it by name.
    write('apps/d/apple/Cargo.toml', '[package]\nname = "d-apple"\n[workspace]\n[workspace.dependencies]\nd-data = { path = "../data" }\n[dependencies]\nd-data = { workspace = true }\n');
    assert.equal(linkedCrate(resolve(dir, 'apps/d'), 'd-data'), resolve(dir, 'apps/d/data'));
    write('apps/d/android/Cargo.toml', '[package]\nname = "d-android"\nworkspace = "../apple"\n[dependencies]\nd-data = { workspace = true }\n');
    assert.equal(linkedCrate(resolve(dir, 'apps/d'), 'd-data'), resolve(dir, 'apps/d/data'));
    // A crate that is not a host's, such as a benchmark, links what it likes.
    write('apps/a/bench/Cargo.toml', '[package]\nname = "a-bench"\n[dependencies]\nhello-data = { path = "../old-hello" }\n');
    assert.equal(linkedCrate(app, 'hello-data'), resolve(dir, 'apps/hello/data'));
    write('apps/a/apple/Cargo.toml', '[package]\nname = "a-apple"\n[dependencies]\nhello-data = { path = "../hello-copy" }\n');
    assert.throws(() => linkedCrate(app, 'hello-data'), /link hello-data from .* and .*; link one directory/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The source root through a link: remapped by its physical path, and as written, last; a canonical root once; a spaced
// remap is refused from a string of rustflags and kept in an array. POSIX follows the link before a later `..`, and
// Windows collapses the `..` first.
test('a linked source root is remapped by its physical path and as written', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-source-root-')));
  try {
    const current = resolve(dir, 'opt/project/current'), capture = resolve(dir, 'opt/project/capture'), link = resolve(dir, 'work/link');
    const physical = process.platform === 'win32' ? resolve(dir, 'work/capture') : capture;
    for (const path of [current, capture, resolve(dir, 'work/capture')]) mkdirSync(path, { recursive: true });
    symlinkSync(current, link);
    assert.equal(physicalPath(`${link}/../capture`), physical);
    assert.equal(physicalPath(link), current);
    assert.equal(physicalPath(resolve(dir, 'missing/root')), null);
    // A relative path joins the working directory as written, its `..` kept and a backslash part of a POSIX name
    if (process.platform !== 'win32') assert.equal(writtenPath('cache\\alias/../x'), `${process.cwd()}/cache\\alias/../x`);
    const source = (root) => rustRemapFlags(null, null, { ...process.env, EXACT_SOURCE_ROOT: root }).filter(flag => flag.endsWith('=source'));
    assert.deepEqual(source(`${link}/../capture`), [`--remap-path-prefix=${physical}=source`, `--remap-path-prefix=${link}/../capture=source`]);
    assert.deepEqual(source(capture), [`--remap-path-prefix=${capture}=source`]);

    const env = { CARGO_HOME: resolve(dir, 'home') }, cwd = resolve(dir, 'app'), spaced = resolve(dir, 'Build Cache/capture'), linked = resolve(dir, 'capture');
    mkdirSync(resolve(cwd, '.cargo'), { recursive: true });
    mkdirSync(spaced, { recursive: true });
    symlinkSync(spaced, linked);
    const flags = source(linked);
    assert.deepEqual(flags, [`--remap-path-prefix=${spaced}=source`, `--remap-path-prefix=${linked}=source`]);
    writeFileSync(resolve(cwd, '.cargo/config.toml'), '[target.wasm32-unknown-unknown]\nrustflags = "--cfg a"\n');
    assert.throws(() => targetRustflags('wasm32-unknown-unknown', flags, { cwd, env }), /rustflags is a string.*write it as an array/);
    writeFileSync(resolve(cwd, '.cargo/config.toml'), '[target.wasm32-unknown-unknown]\nrustflags = ["--cfg", "a"]\n');
    assert.deepEqual(targetRustflags('wasm32-unknown-unknown', flags, { cwd, env }), ['--config', `target.wasm32-unknown-unknown.rustflags=${JSON.stringify(flags)}`]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
