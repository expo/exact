import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { checkoutCrates, checkoutPackages, keptCrates } from '../host/apple/crates.mjs';
import { pinned } from './test-workspace.mjs';

// LLP 1168.000 §4 D3 (a): apps naming one checkout share its compiled path crates, per checkout, development only.
const profile = 'kept-proof';
const git = (cwd, ...args) => {
  const ran = spawnSync('git', args, { cwd, encoding: 'utf8', env: { ...process.env, GIT_AUTHOR_NAME: 'kept', GIT_AUTHOR_EMAIL: 'kept@example.com', GIT_COMMITTER_NAME: 'kept', GIT_COMMITTER_EMAIL: 'kept@example.com' } });
  assert.equal(ran.status, 0, ran.stderr);
  return ran.stdout.trim();
};
/** A checkout of two path crates, one with a build script whose output the other compiles in, under git. */
function checkout(dir) {
  mkdirSync(resolve(dir, 'base/src'), { recursive: true });
  mkdirSync(resolve(dir, 'kit/src'), { recursive: true });
  pinned(dir);
  writeFileSync(resolve(dir, 'Cargo.toml'), '[workspace]\nmembers=["base","kit"]\nresolver="2"\n');
  writeFileSync(resolve(dir, 'base/Cargo.toml'), '[package]\nname="exact-proof-base"\nversion="0.1.0"\nedition="2021"\n[dependencies]\ncfg-if="=1.0.4"\n');
  // Its build script's instructions name its OUT_DIR by path (an env a source reads, a link search), as ibex2's do.
  writeFileSync(resolve(dir, 'base/build.rs'), 'fn main() { let out = std::env::var("OUT_DIR").unwrap(); std::fs::write(format!("{out}/n.rs"), "pub const N: u32 = 7;").unwrap(); std::fs::write(format!("{out}/data.bin"), [1u8, 2, 3]).unwrap(); println!("cargo:rustc-env=BASE_DATA={out}/data.bin"); println!("cargo:rustc-link-search=native={out}"); println!("cargo:rerun-if-changed=build.rs"); }\n');
  writeFileSync(resolve(dir, 'base/src/lib.rs'), 'include!(concat!(env!("OUT_DIR"), "/n.rs"));\npub static DATA: &[u8] = include_bytes!(env!("BASE_DATA"));\ncfg_if::cfg_if! { if #[cfg(unix)] { pub fn n() -> u32 { N } } else { pub fn n() -> u32 { 0 } } }\n');
  writeFileSync(resolve(dir, 'kit/Cargo.toml'), '[package]\nname="exact-proof-kit"\nversion="0.1.0"\nedition="2021"\n[features]\nextra=[]\n[dependencies]\nexact-proof-base={path="../base"}\n');
  writeFileSync(resolve(dir, 'kit/src/lib.rs'), 'pub fn kit() -> u32 { exact_proof_base::n() + 1 }\n');
  const lock = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: dir, encoding: 'utf8' });
  assert.equal(lock.status, 0, lock.stderr);
  git(dir, 'init', '-q', '-b', 'main');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'checkout');
  return dir;
}
/** An app outside the checkout, its crate named `name`, naming the checkout's kit. */
function appOf(dir, root, name = 'chat', features = []) {
  mkdirSync(resolve(dir, 'src'), { recursive: true });
  pinned(dir);
  writeFileSync(resolve(dir, 'Cargo.toml'), `[workspace]\n[package]\nname="${name}"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nexact-proof-kit={path=${JSON.stringify(resolve(root, 'kit'))},features=${JSON.stringify(features)}}\n[profile.${profile}]\ninherits="dev"\ndebug=false\nincremental=false\n`);
  writeFileSync(resolve(dir, 'src/lib.rs'), 'pub fn app() -> u32 { exact_proof_kit::kit() }\n');
  const lock = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: dir, encoding: 'utf8' });
  assert.equal(lock.status, 0, lock.stderr);
  return { dir, workspace: dir, target: resolve(dir, 'target') };
}
/** Cargo's freshness of each compiled unit, by crate name. */
function build(app) {
  const result = spawnSync('cargo', ['build', '--offline', '--profile', profile, '--lib', '--message-format=json-render-diagnostics'], {
    cwd: app.dir, env: { ...process.env, CARGO_TARGET_DIR: app.target }, encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  const fresh = {};
  for (const row of result.stdout.trim().split('\n').map(line => JSON.parse(line))) if (row.reason === 'compiler-artifact' && row.target.kind.includes('lib')) fresh[row.target.name] = row.fresh;
  return fresh;
}
const keeper = (home, root, app, more = {}) => keptCrates({ root, profile, app, env: { HOME: home, RUSTUP_TOOLCHAIN: 'fixture', ...more } });
const scopes = (home) => { const kind = resolve(home, '.cache/exact/apple-crates', `fixture-${profile}`, 'app'); return readdirSync(kind).filter(name => name.startsWith('checkout-')).map(name => resolve(kind, name)); };
const units = (scope) => { const gen = readdirSync(scope).filter(name => name.startsWith('gen-')).sort().at(-1); return readdirSync(resolve(scope, gen, profile, '.fingerprint')); };

test('a second app of a checkout starts with the checkout\'s crates and keeps none of its own', () => {
  const run = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-kept-checkout-'))), home = resolve(run, 'home');
  try {
    const root = checkout(resolve(run, 'checkout'));
    const first = appOf(resolve(run, 'first'), root), second = appOf(resolve(run, 'second'), root);
    assert.equal(build(first).exact_proof_kit, false);
    keeper(home, root, first).keep(first.target, 'app', resolve(first.dir, 'Cargo.lock'));
    const [scope] = scopes(home);
    const kept = units(scope);
    assert.ok(kept.some(name => name.startsWith('exact-proof-base-')) && kept.some(name => name.startsWith('exact-proof-kit-')) && kept.some(name => name.startsWith('cfg-if-')), kept.join(' '));
    assert.ok(!kept.some(name => name.startsWith('chat-')), 'an app\'s own crate is never kept');
    const record = JSON.parse(readFileSync(resolve(scope, 'checkout.json'), 'utf8'));
    assert.equal(record.root, root);
    assert.equal(record.made.at(-1).commit, git(root, 'rev-parse', 'HEAD'), 'the commit is recorded');
    // The maker's directory gone: the build script's output travels with the generation.
    rmSync(first.target, { recursive: true, force: true });
    keeper(home, root, second).take(second.target, 'app');
    const fresh = build(second);
    assert.deepEqual(fresh, { cfg_if: true, exact_proof_base: true, exact_proof_kit: true, chat: false }, 'the second app compiles only its own crate');
    // A source edit compiles base again under the build script it took, which does not run again: its `output` still
    // names the maker's OUT_DIR, gone, and Cargo reads it against the directory's own (`root-output`), as for a moved target.
    writeFileSync(resolve(root, 'base/src/lib.rs'), readFileSync(resolve(root, 'base/src/lib.rs'), 'utf8') + 'pub fn again() {}\n');
    const output = readdirSync(resolve(second.target, profile, 'build')).map(unit => resolve(second.target, profile, 'build', unit, 'output')).filter(existsSync);
    assert.ok(output.some(file => readFileSync(file, 'utf8').includes(first.target)), 'the instructions are the maker\'s');
    assert.equal(build(second).exact_proof_base, false, 'and base compiles, its include_bytes! read from this target');
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 120_000);

test('an edit, a branch switch, another feature or another toolchain compiles the checkout\'s crates again', () => {
  const run = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-kept-checkout-'))), home = resolve(run, 'home');
  try {
    const root = checkout(resolve(run, 'checkout'));
    const first = appOf(resolve(run, 'first'), root);
    build(first);
    keeper(home, root, first).keep(first.target, 'app', resolve(first.dir, 'Cargo.lock'));
    // A branch whose kit differs, switched to: kit and what is above it compile, base does not.
    git(root, 'checkout', '-q', '-b', 'other');
    writeFileSync(resolve(root, 'kit/src/lib.rs'), 'pub fn kit() -> u32 { exact_proof_base::n() + 2 }\n');
    git(root, 'commit', '-q', '-am', 'kit');
    const switched = appOf(resolve(run, 'switched'), root);
    keeper(home, root, switched).take(switched.target, 'app');
    assert.deepEqual(build(switched), { cfg_if: true, exact_proof_base: true, exact_proof_kit: false, chat: false });
    keeper(home, root, switched).keep(switched.target, 'app', resolve(switched.dir, 'Cargo.lock'));
    // A path crate compiled again keeps its unit's name; the next generation still carries it.
    const after = appOf(resolve(run, 'after'), root);
    keeper(home, root, after).take(after.target, 'app');
    assert.deepEqual(build(after), { cfg_if: true, exact_proof_base: true, exact_proof_kit: true, chat: false });
    // An edit not committed: base and kit compile.
    writeFileSync(resolve(root, 'base/src/lib.rs'), readFileSync(resolve(root, 'base/src/lib.rs'), 'utf8') + 'pub fn edited() {}\n');
    const edited = appOf(resolve(run, 'edited'), root);
    keeper(home, root, edited).take(edited.target, 'app');
    assert.deepEqual(build(edited), { cfg_if: true, exact_proof_base: false, exact_proof_kit: false, chat: false });
    keeper(home, root, edited).keep(edited.target, 'app', resolve(edited.dir, 'Cargo.lock'));
    assert.equal(JSON.parse(readFileSync(resolve(scopes(home)[0], 'checkout.json'), 'utf8')).made.at(-1).dirty, true);
    // Another feature of kit: kit compiles, base is taken.
    const featured = appOf(resolve(run, 'featured'), root, 'chat', ['extra']);
    keeper(home, root, featured).take(featured.target, 'app');
    assert.deepEqual(build(featured), { cfg_if: true, exact_proof_base: true, exact_proof_kit: false, chat: false });
    // Another toolchain is another store: nothing is taken.
    const other = appOf(resolve(run, 'other'), root);
    keeper(home, root, other, { RUSTUP_TOOLCHAIN: 'another' }).take(other.target, 'app');
    assert.equal(existsSync(resolve(other.target, profile)), false);
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 180_000);

test('a rebuild in the same target after an edit makes the next generation, and a target inside the checkout keeps none', () => {
  const run = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-kept-checkout-'))), home = resolve(run, 'home');
  try {
    const root = checkout(resolve(run, 'checkout'));
    const first = appOf(resolve(run, 'first'), root);
    build(first);
    keeper(home, root, first).keep(first.target, 'app', resolve(first.dir, 'Cargo.lock'));
    writeFileSync(resolve(root, 'kit/src/lib.rs'), 'pub fn kit() -> u32 { exact_proof_base::n() + 3 }\n');
    assert.equal(build(first).exact_proof_kit, false);
    keeper(home, root, first).keep(first.target, 'app', resolve(first.dir, 'Cargo.lock'));
    const next = appOf(resolve(run, 'next'), root);
    keeper(home, root, next).take(next.target, 'app');
    assert.deepEqual(build(next), { cfg_if: true, exact_proof_base: true, exact_proof_kit: true, chat: false });
    // An app whose target is the checkout's (the repository's apps share it), or a CARGO_TARGET_DIR other apps may
    // share: no checkout scope.
    for (const [label, target, named] of [['inside', resolve(root, 'target')], ['shared', resolve(run, 'shared-target')], ['named', resolve(run, 'named/target'), true]]) {
      const home_ = resolve(run, `${label}-home`), shared = { ...appOf(resolve(run, label), root), target };
      build(shared);
      keeper(home_, root, shared, named ? { CARGO_TARGET_DIR: target } : {}).keep(shared.target, 'app', resolve(shared.dir, 'Cargo.lock'));
      assert.equal(existsSync(resolve(home_, '.cache/exact/apple-crates', `fixture-${profile}`, 'app')) ? scopes(home_).length : 0, 0, label);
    }
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 180_000);

test('another checkout takes only the registry\'s crates, and the switch turns the checkout\'s scope off', () => {
  const run = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-kept-checkout-'))), home = resolve(run, 'home');
  try {
    const root = checkout(resolve(run, 'checkout'));
    const first = appOf(resolve(run, 'first'), root);
    build(first);
    keeper(home, root, first).keep(first.target, 'app', resolve(first.dir, 'Cargo.lock'));
    const copy = resolve(run, 'copy');
    cpSync(root, copy, { recursive: true, preserveTimestamps: true });
    const elsewhere = appOf(resolve(run, 'elsewhere'), copy);
    keeper(home, copy, elsewhere).take(elsewhere.target, 'app');
    assert.equal(JSON.parse(readFileSync(resolve(elsewhere.target, `kept-crates-${profile}.json`), 'utf8')).scope, 'registry');
    assert.deepEqual(build(elsewhere), { cfg_if: true, exact_proof_base: false, exact_proof_kit: false, chat: false });
    const off = resolve(run, 'off-home'), plain = appOf(resolve(run, 'plain'), root);
    build(plain);
    keeper(off, root, plain, { EXACT_APPLE_KEEP_CHECKOUT: '0' }).keep(plain.target, 'app', resolve(plain.dir, 'Cargo.lock'));
    assert.deepEqual(scopes(off), [], 'switched off, no checkout scope is made');
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 120_000);

test('an app\'s own packages are not the checkout\'s, whatever their names', () => {
  const run = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-kept-checkout-')));
  try {
    const root = checkout(resolve(run, 'checkout'));
    // The repository's own app (no app.json, an app.contract), and a checkout crate an outside app vendors a copy of.
    mkdirSync(resolve(root, 'apps/shelf/src'), { recursive: true });
    writeFileSync(resolve(root, 'apps/shelf/app.contract'), 'component App\n');
    writeFileSync(resolve(root, 'apps/shelf/Cargo.toml'), '[package]\nname="shelf-apple"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nexact-proof-kit={path="../../kit"}\n');
    writeFileSync(resolve(root, 'apps/shelf/src/lib.rs'), '');
    mkdirSync(resolve(root, 'extra/src'), { recursive: true });
    writeFileSync(resolve(root, 'extra/Cargo.toml'), '[package]\nname="exact-proof-extra"\nversion="0.1.0"\nedition="2021"\n');
    writeFileSync(resolve(root, 'extra/src/lib.rs'), '');
    writeFileSync(resolve(root, 'Cargo.toml'), '[workspace]\nmembers=["base","kit","extra","apps/shelf"]\nresolver="2"\n');
    assert.equal(spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: root, encoding: 'utf8' }).status, 0);
    const app = appOf(resolve(run, 'app'), root);
    mkdirSync(resolve(app.dir, 'vendor/extra/src'), { recursive: true });
    writeFileSync(resolve(app.dir, 'vendor/extra/Cargo.toml'), '[package]\nname="exact-proof-extra"\nversion="0.1.0"\nedition="2021"\n');
    writeFileSync(resolve(app.dir, 'vendor/extra/src/lib.rs'), '');
    writeFileSync(resolve(app.dir, 'Cargo.toml'), readFileSync(resolve(app.dir, 'Cargo.toml'), 'utf8').replace('[dependencies]\n', '[dependencies]\nexact-proof-extra={path="vendor/extra"}\n'));
    const lock = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: app.dir, encoding: 'utf8' });
    assert.equal(lock.status, 0, lock.stderr);
    const checkout_ = checkoutCrates(root, app);
    assert.deepEqual([...checkout_].sort(), ['exact-proof-base', 'exact-proof-kit']);
    assert.deepEqual([...checkoutPackages(resolve(app.dir, 'Cargo.lock'), checkout_)].sort(), ['cfg-if', 'exact-proof-base', 'exact-proof-kit']);
    // The repository's app, from its own workspace: its crate is never the checkout's.
    assert.deepEqual([...checkoutCrates(root, { dir: resolve(root, 'apps/shelf'), workspace: root })].sort(), ['exact-proof-base', 'exact-proof-extra', 'exact-proof-kit']);
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 120_000);

test('a checkout scope whose checkout is gone, or unused for 14 days, is removed', () => {
  const run = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-kept-checkout-'))), home = resolve(run, 'home');
  try {
    const root = checkout(resolve(run, 'checkout'));
    const first = appOf(resolve(run, 'first'), root);
    build(first);
    const kind = resolve(home, '.cache/exact/apple-crates', `fixture-${profile}`, 'app');
    const old = Date.now() / 1000 - 15 * 24 * 3600;
    for (const [name, record] of [['checkout-gone', { root: resolve(run, 'gone'), used: Date.now() }], ['checkout-unused', { root, used: old * 1000 }]]) {
      mkdirSync(resolve(kind, name, 'gen-1'), { recursive: true });
      writeFileSync(resolve(kind, name, 'checkout.json'), JSON.stringify(record));
      utimesSync(resolve(kind, name, 'checkout.json'), old, old);
      utimesSync(resolve(kind, name), old, old);
    }
    keeper(home, root, first).keep(first.target, 'app', resolve(first.dir, 'Cargo.lock'));
    // A build of the checkout, keeping nothing new, is still a use.
    const [live] = scopes(home).filter(scope => /checkout-[0-9a-f]{16}$/.test(scope));
    writeFileSync(resolve(live, 'checkout.json'), JSON.stringify({ ...JSON.parse(readFileSync(resolve(live, 'checkout.json'), 'utf8')), used: 0 }));
    keeper(home, root, first).keep(first.target, 'app', resolve(first.dir, 'Cargo.lock'));
    assert.ok(Date.now() - JSON.parse(readFileSync(resolve(live, 'checkout.json'), 'utf8')).used < 60_000);
    assert.deepEqual(scopes(home).map(scope => scope.split('/').at(-1)).filter(name => !/^checkout-[0-9a-f]{16}$/.test(name)), []);
    assert.equal(scopes(home).length, 1, 'the live checkout keeps its scope');
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 120_000);
