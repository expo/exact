// A tool set is taken by its inputs' bytes, and a fetched one only where it may run (LLP 1174 D3, D5).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash, generateKeyPairSync } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { digest, lockClosure, workspaceManifest } from './kept.mjs';
import { cargoOnPath } from './cargo-check.mjs';
import { ROOT, RustMissing, fromSet, fromSets, hostTriple, publishToolSet, publishWatchedTip, readToolManifest, rustMissing } from './tools.mjs';
import { DirectoryOrigin, blobPath } from './origin.mjs';
import { offlineMiss } from './filesystem.mjs';

const pin = Bun.TOML.parse(readFileSync(resolve(ROOT, 'rust-toolchain.toml'), 'utf8')).toolchain.channel;
const sha = (text) => createHash('sha256').update(text).digest('hex');

/** A machine: a HOME whose cache holds the sets `sets` describes, and whether it has Rust (a `cargo` on PATH and the
 * pinned toolchain). */
function machine(sets, { rust }) {
  const home = mkdtempSync(resolve(tmpdir(), 'exact-tools-')), bin = resolve(home, 'bin');
  mkdirSync(bin);
  if (rust) {
    writeFileSync(resolve(bin, process.platform === 'win32' ? 'cargo.exe' : 'cargo'), '', { mode: 0o755 });
    mkdirSync(resolve(home, '.rustup/toolchains', `${pin}-${hostTriple()}`), { recursive: true });
  }
  sets.forEach(({ source, inputs, commit }, i) => {
    const dir = resolve(home, '.cache/exact/tools', hostTriple(), `set${i}`);
    mkdirSync(dir, { recursive: true });
    writeFileSync(resolve(dir, 'contract'), '');
    const product = { file: 'contract', crate: 'contract', inputs, closure: lockClosure(ROOT, 'contract'), workspace: sha(JSON.stringify(workspaceManifest(ROOT))) };
    writeFileSync(resolve(dir, 'manifest.json'), JSON.stringify({ version: 1, triple: hostTriple(), commit, source, built: i, products: { contract: product } }));
  });
  const env = { HOME: home, PATH: bin, CARGO_HOME: resolve(home, '.cargo'), RUSTUP_HOME: resolve(home, '.rustup') };
  return { env, done: () => rmSync(home, { recursive: true, force: true }) };
}

const ours = ['rust-toolchain.toml', 'contract/syntax/src'].map(file => [file, digest(resolve(ROOT, file))]);

test('publish exposes a verified manifest and complete blobs before updating either lookup', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-tools-publish-'));
  try {
    const publication = resolve(dir, 'origin');
    mkdirSync(publication);
    const origin = new DirectoryOrigin(publication);
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const triple = hostTriple(), base = `.exact/tools/${triple}`, commit = 'b'.repeat(40);
    const body = Buffer.from('a compiled tool'), digest = sha(body);
    writeFileSync(resolve(dir, 'contract'), body);
    const set = { dir, manifest: { version: 1, source: 'built', commit, triple, products: {
      contract: { file: 'contract', size: body.length, sha256: digest },
    } } };
    const publish = (revision, newer = []) => publishToolSet(origin, { dir, manifest: { ...set.manifest, commit: revision } }, privateKey, newer);
    const id = await publish(commit);
    assert.deepEqual(await origin.get(blobPath(digest)), body);
    const bytes = await origin.get(`${base}/manifests/${id}.json`);
    const manifest = readToolManifest(bytes, id, triple, publicKey);
    assert.deepEqual(manifest.products, set.manifest.products);
    assert.equal(manifest.source, 'fetched');
    assert.throws(() => readToolManifest(bytes, id, triple, generateKeyPairSync('ed25519').publicKey), /signature/);
    const altered = Buffer.from(JSON.stringify({ ...manifest, commit: 'a'.repeat(40) }));
    assert.throws(() => readToolManifest(altered, id, triple, publicKey), /digest/);
    assert.throws(() => readToolManifest(altered, sha(altered), triple, publicKey), /signature/);
    assert.equal(await publish(commit), id);
    assert.equal(JSON.parse(await origin.get(`${base}/commits/${commit}.json`)), id);
    assert.deepEqual(JSON.parse(await origin.get(`${base}/recent.json`)), [id]);
    writeFileSync(resolve(dir, 'contract'), 'changed');
    const next = 'c'.repeat(40);
    await assert.rejects(publish(next), /contract differs/);
    assert.equal(await origin.get(`${base}/commits/${next}.json`), null);
    assert.deepEqual(JSON.parse(await origin.get(`${base}/recent.json`)), [id]);

    writeFileSync(resolve(dir, 'contract'), body);
    const commits = Array.from({ length: 10 }, (_, i) => i.toString(16).padStart(40, '0'));
    const ids = [];
    for (const i of [0, 4, 6, 9, 8, 7, 5, 3, 2, 1]) {
      ids[i] = await publish(commits[i], commits.slice(i + 1).reverse());
    }
    assert.deepEqual(JSON.parse(await origin.get(`${base}/recent.json`)), ids.slice(-8).reverse());
    assert.equal(JSON.parse(await origin.get(`${base}/commits/${commits[1]}.json`)), ids[1]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60000);

test('the publisher prefers each new tip and resumes skipped commits after failure, restart or branch replacement', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-tools-watch-'));
  const repo = resolve(dir, 'repo'), tree = resolve(dir, 'worker'), state = resolve(dir, 'state.json');
  mkdirSync(repo);
  const git = (cwd, ...args) => execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  const commit = message => {
    git(repo, '-c', 'user.name=Exact', '-c', 'user.email=exact@example.invalid', '-c', 'commit.gpgSign=false', 'commit', '--allow-empty', '-qm', message);
    return git(repo, 'rev-parse', 'HEAD');
  };
  let retry = new Map(), fail;
  const seen = [];
  const poll = tip => publishWatchedTip(tree, state, tip, (revision, newer) => {
    assert.equal(git(tree, 'rev-parse', 'HEAD'), revision);
    if (revision === fail) throw new Error('build failed');
    seen.push([revision, newer]);
  }, retry, () => 1000);
  try {
    git(repo, 'init', '-q', '-b', 'main');
    const a = commit('a');
    git(repo, 'worktree', 'add', '--detach', tree, a);
    await poll(a);
    assert.equal(await poll(a), null);
    const b = commit('b'), c = commit('c'), d = commit('d');
    await poll(d);
    const e = commit('e');
    await poll(e);
    fail = c;
    assert.match((await poll(e)).error.message, /build failed/);
    await poll(e);
    assert.equal(await poll(e), null);
    retry = new Map();
    fail = null;
    await poll(e);
    assert.deepEqual(seen, [[a, []], [d, []], [e, []], [b, [e, d, c]], [c, [e, d]]]);
    assert.equal(await poll(e), null);

    commit('discarded');
    await poll(commit('old tip'));
    git(repo, 'switch', '-qc', 'replacement', e);
    const replacement = commit('replacement');
    writeFileSync(resolve(tree, 'unfinished'), 'keep');
    await assert.rejects(poll(replacement), /clean producer checkout/);
    assert.equal(readFileSync(resolve(tree, 'unfinished'), 'utf8'), 'keep');
    rmSync(resolve(tree, 'unfinished'));
    git(tree, 'switch', '-qc', 'work-in-progress');
    await assert.rejects(poll(replacement), /detached worktree/);
    git(tree, 'switch', '--detach', 'HEAD');
    assert.deepEqual(await poll(replacement), { commit: replacement });
    assert.equal(await poll(replacement), null);
    assert.equal(git(repo, 'rev-parse', '--abbrev-ref', 'HEAD'), 'replacement');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60000);

test('a set is taken while its inputs are these bytes, and a miss names the first that differs', () => {
  const changed = [ours[0], ['contract/syntax/src', 'f'.repeat(64)]];
  const m = machine([{ source: 'built', inputs: changed, commit: 'a'.repeat(40) }, { source: 'built', inputs: ours, commit: 'b'.repeat(40) }], { rust: false });
  try {
    assert.equal(fromSet('contract', 'built', m.env)?.path, resolve(m.env.HOME, '.cache/exact/tools', hostTriple(), 'set1', 'contract'));
  } finally { m.done(); }
  const miss = machine([{ source: 'built', inputs: changed, commit: 'c'.repeat(40) }], { rust: false });
  try {
    assert.equal(fromSet('contract', 'built', miss.env), null);
    assert.match(new RustMissing('contract', miss.env).message, /^error \[rust-missing\]: contract builds with Rust, .* contract\/syntax\/src differs from the newest tool set's, built at cccccccccccc.*https:\/\/rustup\.rs/);
  } finally { miss.done(); }
}, 60000);

test('a fetched set runs only without Rust or when an origin is named, and never under EXACT_TOOLS_ORIGIN=none', () => {
  for (const [rust, origin, taken] of [[true, undefined, false], [true, 'https://tools.example', true], [false, undefined, true], [false, 'none', false]]) {
    const m = machine([{ source: 'fetched', inputs: ours, commit: 'd'.repeat(40) }], { rust });
    try {
      const env = origin ? { ...m.env, EXACT_TOOLS_ORIGIN: origin } : m.env;
      assert.equal(!!fromSets('contract', env), taken, `rust ${rust}, origin ${origin}`);
    } finally { m.done(); }
  }
}, 60000);

function onWindows(arch, run) {
  const platform = Object.getOwnPropertyDescriptor(process, 'platform');
  const cpu = Object.getOwnPropertyDescriptor(process, 'arch');
  Object.defineProperty(process, 'platform', { value: 'win32' });
  Object.defineProperty(process, 'arch', { value: arch });
  try { run(); } finally {
    Object.defineProperty(process, 'platform', platform);
    Object.defineProperty(process, 'arch', cpu);
  }
}

for (const [arch, triple] of [['x64', 'x86_64-pc-windows-msvc'], ['arm64', 'aarch64-pc-windows-msvc']]) {
  test(`Windows ${arch} uses rustup's host triple`, () => onWindows(arch, () => {
    assert.equal(hostTriple(), triple);
  }));

  test(`Windows ${arch} finds Cargo on Path and in CARGO_HOME`, () => onWindows(arch, () => {
    const home = mkdtempSync(resolve(tmpdir(), 'exact-windows-cargo-'));
    const first = resolve(home, 'empty'), bin = resolve(home, '.cargo/bin');
    mkdirSync(first); mkdirSync(bin, { recursive: true });
    writeFileSync(resolve(bin, 'cargo.exe'), '', { mode: 0o755 });
    try {
      const env = { Path: `${first};${bin}`, CARGO_HOME: resolve(home, 'absent') };
      assert.equal(cargoOnPath(env, home), true);
      assert.equal(env.Path, `${first};${bin}`);
      const fallback = { PATH: first, CARGO_HOME: resolve(home, '.cargo') };
      assert.equal(cargoOnPath(fallback, home), true);
      assert.equal(fallback.PATH, `${bin};${first}`);
      rmSync(resolve(bin, 'cargo.exe'));
      assert.equal(cargoOnPath({ PATH: bin }, home), false);
    } finally { rmSync(home, { recursive: true, force: true }); }
  }));

  test(`Windows ${arch} recognizes installed Rust and uses its tool-set cache`, () => onWindows(arch, () => {
    const home = mkdtempSync(resolve(tmpdir(), 'exact-windows-rust-'));
    const bin = resolve(home, '.cargo/bin'), toolchain = resolve(home, '.rustup/toolchains', `${pin}-${triple}`);
    mkdirSync(bin, { recursive: true }); mkdirSync(toolchain, { recursive: true });
    writeFileSync(resolve(bin, 'cargo.exe'), '', { mode: 0o755 });
    const env = { USERPROFILE: home, Path: bin };
    try {
      assert.equal(rustMissing(env), false);
      const dir = resolve(home, '.cache/exact/tools', triple, 'windows');
      mkdirSync(dir, { recursive: true });
      const product = { file: 'contract.exe', crate: 'contract', inputs: ours, closure: lockClosure(ROOT, 'contract'), workspace: sha(JSON.stringify(workspaceManifest(ROOT))) };
      writeFileSync(resolve(dir, 'contract.exe'), '');
      writeFileSync(resolve(dir, 'manifest.json'), JSON.stringify({ source: 'built', products: { contract: product } }));
      assert.equal(fromSet('contract', 'built', env)?.path, resolve(dir, 'contract.exe'));
      rmSync(toolchain, { recursive: true });
      assert.equal(rustMissing(env), true);
      rmSync(dir, { recursive: true });
      assert.ok(new RustMissing('exact-filesystem', env).message.includes(triple));
    } finally { rmSync(home, { recursive: true, force: true }); }
  }));
}

// LLP 1156.006.000 D11: the three offline Cargo sites with no fetch-and-retry name the checkout's own setup, by path.
test('an offline Cargo miss names this checkout\'s setup, and another failure gains nothing', () => {
  assert.equal(offlineMiss('error: failed to download `js-sys v0.3.77`\n  attempting to make an HTTP request, but --offline was specified'),
    `\n\`bun ${resolve(ROOT, 'scripts/exact.mjs')} setup\` fetches the crates this checkout locks`);
  assert.equal(offlineMiss('error: could not compile `exact-web-js`'), '');
  assert.equal(offlineMiss(undefined), '');
});
