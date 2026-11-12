import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { targetRustflags } from './cargo-check.mjs';
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
  try {
    mkdirSync(resolve(dir, 'src'), { recursive: true });
    mkdirSync(resolve(dir, '.cargo'), { recursive: true });
    writeFileSync(resolve(dir, 'Cargo.toml'), '[package]\nname = "configwasm"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\ncrate-type = ["cdylib"]\n\n[workspace]\n');
    // The symbol exists only when the configured flags reached rustc; the source path only without the remap.
    writeFileSync(resolve(dir, 'src/lib.rs'), '#[cfg(configured)]\n#[no_mangle]\npub extern "C" fn exact_configured_marker(at: usize) -> u8 {\n    file!().as_bytes()[at]\n}\n');
    const app = { workspace: dir, target: resolve(dir, 'target'), manifest: {} };
    for (const rustflags of ['"--cfg configured"', '["--cfg", "configured"]']) {
      writeFileSync(resolve(dir, '.cargo/config.toml'), `[target.wasm32-unknown-unknown]\nrustflags = ${rustflags}\n`);
      const cargo = spawnSync('cargo', ['build', '--offline', ...wasmRemapFlags(app), '--lib', '--target', 'wasm32-unknown-unknown'],
        { cwd: dir, encoding: 'utf8', env: { ...process.env, CARGO_TARGET_DIR: resolve(dir, 'target') } });
      assert.equal(cargo.status, 0, `${rustflags}: ${cargo.stderr}`);
      const bytes = readFileSync(resolve(dir, 'target/wasm32-unknown-unknown/debug/configwasm.wasm')).toString('latin1');
      assert.ok(bytes.includes('exact_configured_marker'), `${rustflags}: built with the configured flags`);
      assert.ok(!bytes.includes(resolve(dir, 'src/lib.rs')), `${rustflags}: built with the remap`);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300000);
