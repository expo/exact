import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { posix, resolve, win32 } from 'node:path';
import { keptInput, keptProducts } from './kept.mjs';

test('a kept product reads its dep-info by the platform separator: a target is skipped, outside keeps nothing', () => {
  const root = 'C:\\exact', skip = ['C:\\exact\\target'];
  assert.equal(keptInput('C:\\exact\\kernel\\src\\lib.rs', root, skip, win32), 'kernel\\src\\lib.rs');
  assert.equal(keptInput('C:\\exact\\target\\debug\\build\\k-0a\\out\\t.rs', root, skip, win32), undefined);
  assert.equal(keptInput('C:\\Users\\me\\.cargo\\registry\\src\\x\\lib.rs', root, skip, win32), null);
  assert.equal(keptInput('C:\\exactly\\lib.rs', root, skip, win32), null);
  assert.equal(keptInput('/exact/kernel/src/lib.rs', '/exact', ['/exact/target'], posix), 'kernel/src/lib.rs');
  assert.equal(keptInput('/exact/target/debug/out/t.rs', '/exact', ['/exact/target'], posix), undefined);
  assert.equal(keptInput('/home/me/.cargo/x.rs', '/exact', ['/exact/target'], posix), null);
});

test('keptProducts records rerun-if-env-changed from both nightly two-level run/stdout and stable output build dirs', () => {
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-kept-nightly-'));
  const git = (cwd, ...args) => { const r = spawnSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', '-C', cwd, ...args], { encoding: 'utf8' }); if (r.status !== 0) throw new Error(r.stderr); };
  try {
    const home = resolve(scratch, 'home'), repo = resolve(scratch, 'repo');
    mkdirSync(resolve(repo, 'src'), { recursive: true });
    writeFileSync(resolve(repo, 'Cargo.toml'), '[package]\nname = "probe"\nversion = "0.1.0"\nedition = "2021"\n');
    writeFileSync(resolve(repo, 'Cargo.lock'), 'version = 3\n\n[[package]]\nname = "probe"\nversion = "0.1.0"\n');
    writeFileSync(resolve(repo, 'src/lib.rs'), 'pub fn x() {}\n');
    git(repo, 'init', '-q'); git(repo, 'add', '.'); git(repo, 'commit', '-qm', 'init');
    const old = new Date(Date.now() - 10_000);
    for (const f of ['Cargo.toml', 'Cargo.lock', 'src/lib.rs']) utimesSync(resolve(repo, f), old, old);
    const buildDir = resolve(repo, 'target/build'), product = resolve(repo, 'target/probe.wasm'), depInfo = resolve(repo, 'target/probe.d');
    mkdirSync(resolve(buildDir, 'icu_properties_data/09c45dcdb6c411ee/run'), { recursive: true });
    writeFileSync(resolve(buildDir, 'icu_properties_data/09c45dcdb6c411ee/run/stdout'), 'cargo:rerun-if-env-changed=TEST_NIGHTLY_VAR\ncargo::rerun-if-env-changed=TEST_NIGHTLY_COLON\n');
    mkdirSync(resolve(buildDir, 'stable_pkg-0123abcd'), { recursive: true });
    writeFileSync(resolve(buildDir, 'stable_pkg-0123abcd/output'), 'cargo:rerun-if-env-changed=TEST_STABLE_VAR\n');
    writeFileSync(depInfo, `${product}: ${resolve(repo, 'src/lib.rs')}\n`);
    const started = Date.now() - 1000;
    writeFileSync(product, 'wasm-bytes');
    const baseEnv = { ...process.env, HOME: home, TEST_NIGHTLY_VAR: 'one', TEST_NIGHTLY_COLON: 'alpha', TEST_STABLE_VAR: 'stable-1' };
    const spec = { crate: 'probe', product, depInfo, buildDirs: [buildDir], stamp: resolve(repo, 'target/stamp.json') };
    const kept = keptProducts({ root: repo, cache: 'test-kept', env: baseEnv, label: 'test' })(spec);
    assert.equal(kept.keep(started), true);
    const groupDir = readdirSync(resolve(home, '.cache/exact/test-kept'))[0];
    const entryDir = readdirSync(resolve(home, '.cache/exact/test-kept', groupDir))[0];
    const recorded = JSON.parse(readFileSync(resolve(home, '.cache/exact/test-kept', groupDir, entryDir, 'inputs.json'), 'utf8'));
    assert.deepEqual(recorded.env, [['TEST_NIGHTLY_COLON', 'alpha'], ['TEST_NIGHTLY_VAR', 'one'], ['TEST_STABLE_VAR', 'stable-1']]);
    rmSync(product);
    assert.ok(existsSync(keptProducts({ root: repo, cache: 'test-kept', env: baseEnv, label: 'test' })(spec).find()));
    for (const changed of [{ TEST_NIGHTLY_VAR: 'two' }, { TEST_NIGHTLY_COLON: 'beta' }, { TEST_STABLE_VAR: 'stable-2' }]) {
      rmSync(spec.stamp, { force: true });
      assert.equal(keptProducts({ root: repo, cache: 'test-kept', env: { ...baseEnv, ...changed }, label: 'test' })(spec).find(), null);
    }
  } finally { rmSync(scratch, { recursive: true, force: true }); }
});
