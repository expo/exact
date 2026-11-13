import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { claimTargetCrates, targetRustflags } from './cargo-check.mjs';
import { wasmRemapFlags } from './app.mjs';

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
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-wasm-config-'));
  const workspace = resolve(dir, 'app'), previousRoot = process.env.EXACT_SOURCE_ROOT;
  try {
    process.env.EXACT_SOURCE_ROOT = dir;
    mkdirSync(resolve(workspace, 'src'), { recursive: true });
    mkdirSync(resolve(workspace, '.cargo'), { recursive: true });
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
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-target-crates-'));
  try {
    const target = resolve(dir, 'shared'), one = resolve(dir, 'one/budget'), two = resolve(dir, 'two/budget');
    for (const app of [one, two]) { mkdirSync(app, { recursive: true }); writeFileSync(resolve(app, 'app.contract'), ''); }
    claimTargetCrates(target, 'budget', one);
    claimTargetCrates(target, 'budget', one);
    claimTargetCrates(target, 'other', two);
    assert.throws(() => claimTargetCrates(target, 'budget', two), new RegExp(`holds the crates budget-\\* of the app in ${one}, and the app in ${two} names its crates the same`));
    rmSync(one, { recursive: true });
    claimTargetCrates(target, 'budget', two);
    assert.equal(readFileSync(resolve(target, '.exact-crates/budget'), 'utf8'), `${two}\n`);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
