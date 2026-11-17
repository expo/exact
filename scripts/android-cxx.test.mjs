import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { androidToolchainEnv } from './agent-android.mjs';
import { PINNED_RUST } from './app.mjs';
import { androidCargo } from './cargo-check.mjs';

// C++ that a data crate's build script compiles with the cc crate (as c++_static, `androidToolchainEnv`) and that
// throws: libc++_static.a leaves the exception runtime to libc++abi.a, and a shared library links with it undefined,
// which the device refuses to load.
let androidCxx;
try { androidCxx = spawnSync('rustup', ['target', 'list', '--installed', '--toolchain', PINNED_RUST], { encoding: 'utf8' }).stdout.split('\n').includes('aarch64-linux-android') && androidToolchainEnv(); } catch {}
test.skipIf(!androidCxx)('an Android library with C++ exceptions resolves the C++ runtime at its link', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-cxx-'));
  try {
    mkdirSync(resolve(dir, 'src'));
    writeFileSync(resolve(dir, 'Cargo.toml'), '[package]\nname = "cxx"\nversion = "0.1.0"\nedition = "2024"\n[lib]\ncrate-type = ["cdylib"]\n[build-dependencies]\ncc = "1"\n[workspace]\n');
    writeFileSync(resolve(dir, 'build.rs'), 'fn main() { cc::Build::new().cpp(true).file("throw.cpp").compile("throw"); }\n');
    writeFileSync(resolve(dir, 'throw.cpp'), '#include <stdexcept>\nextern "C" int throws(int n) { try { if (n) throw std::length_error("n"); } catch (const std::length_error &) { return 1; } return 0; }\n');
    writeFileSync(resolve(dir, 'src/lib.rs'), 'unsafe extern "C" { fn throws(n: i32) -> i32; }\n#[unsafe(no_mangle)]\npub extern "C" fn fixture(n: i32) -> i32 { unsafe { throws(n) } }\n');
    // The workspace's lock, so the build takes the cc it already has, offline.
    writeFileSync(resolve(dir, 'Cargo.lock'), readFileSync(resolve(import.meta.dir, '../Cargo.lock')));
    const env = { ...androidCxx, RUSTUP_TOOLCHAIN: PINNED_RUST, CARGO_TARGET_DIR: resolve(dir, 'target') };
    const built = spawnSync(...androidCargo('cargo', ['build', '--offline', '--target', 'aarch64-linux-android'], { cwd: dir, env, encoding: 'utf8' }));
    expect(built.status, built.stderr).toBe(0);
    const symbols = spawnSync(env.AR_aarch64_linux_android.replace(/llvm-ar(?=[^/\\]*$)/, 'llvm-nm'), ['-D', '--undefined-only', resolve(dir, 'target/aarch64-linux-android/debug/libcxx.so')], { encoding: 'utf8' });
    expect(symbols.status).toBe(0);
    expect(symbols.stdout.split('\n').filter((line) => /_ZTI|_ZTV|_ZNSt|__cxa_(throw|begin)/.test(line))).toEqual([]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120_000);
