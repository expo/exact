// A tool set is taken by its inputs' bytes, and a fetched one only where it may run (LLP 1174 D3, D5).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash, generateKeyPairSync } from 'node:crypto';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { digest, lockClosure, workspaceManifest } from './kept.mjs';
import { depInfoOf } from './cargo-dep-info.mjs';
import { cargoOnPath } from './cargo-check.mjs';
import { ROOT, RustMissing, WEB_CRATES, builtContract, compilerFrom, fetchToolSet, freshCompiler, fromSet, fromSets, hostTriple, keep, publishToolSet, publishWatchedTip, readToolManifest, rustMissing } from './tools.mjs';
import { DirectoryOrigin, HttpsOrigin, blobPath } from './origin.mjs';
import { offlineMiss } from './filesystem.mjs';
import { EXE } from './paths.mjs';

const pin = Bun.TOML.parse(readFileSync(resolve(ROOT, 'rust-toolchain.toml'), 'utf8')).toolchain.channel;
const sha = (text) => createHash('sha256').update(text).digest('hex');

/** A machine: a HOME whose cache holds the sets `sets` describes, and whether it has Rust (a `cargo` on PATH and the
 * pinned toolchain). */
function machine(sets, { rust, crates = ['contract'] }) {
  const home = mkdtempSync(resolve(tmpdir(), 'exact-tools-')), bin = resolve(home, 'bin');
  const cache = resolve(home, '.cache/exact/tools', hostTriple());
  mkdirSync(bin);
  if (rust) {
    writeFileSync(resolve(bin, process.platform === 'win32' ? 'cargo.exe' : 'cargo'), '', { mode: 0o755 });
    mkdirSync(resolve(home, '.rustup/toolchains', `${pin}-${hostTriple()}`), { recursive: true });
  }
  sets.forEach(({ source, inputs, commit, verifiedBuild = source === 'built' }, i) => {
    const dir = resolve(cache, `set${i}`);
    mkdirSync(dir, { recursive: true });
    const products = Object.fromEntries(crates.map(crate => {
      writeFileSync(resolve(dir, crate), '', { mode: 0o755 });
      return [crate, { file: crate, crate, inputs, closure: lockClosure(ROOT, crate), workspace: sha(JSON.stringify(workspaceManifest(ROOT))) }];
    }));
    writeFileSync(resolve(dir, 'manifest.json'), JSON.stringify({ version: 1, triple: hostTriple(), commit, source, ...(verifiedBuild ? { verifiedBuild } : {}), built: i, products }));
  });
  const env = { HOME: home, PATH: bin, CARGO_HOME: resolve(home, '.cargo'), RUSTUP_HOME: resolve(home, '.rustup'), CARGO_TARGET_DIR: resolve(home, 'target') };
  return { env, cache, bin: (name, i = 0) => resolve(cache, `set${i}`, name), done: () => rmSync(home, { recursive: true, force: true }) };
}

const ours = ['rust-toolchain.toml', 'contract/syntax/src'].map(file => [file, digest(resolve(ROOT, file))]);

function debugCompiler(target, name, source = resolve(target, 'compiler source.rs'), contents = '') {
  const debug = resolve(target, 'debug', name + EXE);
  mkdirSync(dirname(debug), { recursive: true });
  writeFileSync(source, '', { flag: 'a' });
  const escape = path => path.replaceAll(' ', '\\ ');
  writeFileSync(depInfoOf(debug), `${escape(debug)}: ${escape(source)}\n`);
  writeFileSync(debug, contents, { mode: 0o755 });
  const built = Date.now() / 1000 + 60;
  utimesSync(debug, built, built);
  return debug;
}

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
    if (triple === 'aarch64-apple-darwin') {
      const linux = 'x86_64-unknown-linux-gnu';
      const linuxSet = { dir, manifest: { ...set.manifest, triple: linux } };
      const linuxId = await publishToolSet(origin, linuxSet, privateKey);
      const linuxBase = `.exact/tools/${linux}`;
      assert.equal(JSON.parse(await origin.get(`${linuxBase}/commits/${commit}.json`)), linuxId);
      assert.deepEqual(JSON.parse(await origin.get(`${linuxBase}/recent.json`)), [linuxId]);
      const bytes = await origin.get(`${linuxBase}/manifests/${linuxId}.json`);
      assert.equal(readToolManifest(bytes, linuxId, linux, publicKey).triple, linux);
      assert.throws(() => readToolManifest(bytes, linuxId, triple, publicKey), /not a version 1 set/);
    }
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

test('fetch authenticates the whole set before the cache can run it and falls back from HEAD to matching recent inputs', async () => {
  const m = machine([], { rust: false }), { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const triple = hostTriple(), base = `.exact/tools/${triple}`, dir = resolve(m.env.HOME, 'origin');
  const cache = resolve(m.env.HOME, '.cache/exact/tools', triple);
  mkdirSync(dir);
  const origin = new DirectoryOrigin(dir), requests = [];
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
    const path = new URL(request.url).pathname.slice(1);
    requests.push(path);
    const bytes = await origin.get(path);
    return bytes === null ? new Response(null, { status: 404 }) : new Response(bytes);
  } });
  try {
    const head = execFileSync('git', ['-C', ROOT, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
    const env = { ...m.env, PATH: process.env.PATH, EXACT_TOOLS_ORIGIN: server.url.href };
    const mirror = new HttpsOrigin(server.url.href);
    const tool = Buffer.from('contract executable'), wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
    const product = { file: 'contract', inputs: ours, crate: 'contract', closure: lockClosure(ROOT, 'contract'),
      workspace: sha(JSON.stringify(workspaceManifest(ROOT))), size: tool.length, sha256: sha(tool) };
    const products = { contract: product, 'motion.wasm': { ...product, file: 'motion.wasm', size: wasm.length, sha256: sha(wasm) } };
    writeFileSync(resolve(dir, 'contract'), tool);
    writeFileSync(resolve(dir, 'motion.wasm'), wasm);
    const publish = (commit, rows) => publishToolSet(origin, { dir, manifest: { version: 1, triple, source: 'built', commit, products: rows } }, privateKey);
    const good = await publish(head, products);
    const fetchSet = (keys = [publicKey]) => fetchToolSet(mirror, 'contract', keys, env);
    await assert.rejects(fetchSet([generateKeyPairSync('ed25519').publicKey]), /signature/);
    assert.equal(requests.some(path => path.includes('/blobs/')), false);
    writeFileSync(origin.path(blobPath(sha(wasm))), Buffer.alloc(wasm.length));
    await assert.rejects(fetchSet(), /motion.wasm differs/);
    assert.deepEqual(readdirSync(cache), []);
    assert.equal(requests.includes(`${base}/recent.json`), false);
    writeFileSync(origin.path(blobPath(sha(wasm))), wasm);
    const stale = await publish(head, { ...products, contract: { ...product, inputs: [[ours[0][0], '0'.repeat(64)]] } });
    await origin.put(`${base}/recent.json`, Buffer.from(JSON.stringify([stale])));
    await assert.rejects(fetchSet(), new RegExp(`rust-toolchain.toml differs from the tool set built at ${head.slice(0, 12)}`));
    await origin.put(`${base}/recent.json`, Buffer.from(JSON.stringify([stale, good])));
    requests.length = 0;
    assert.deepEqual(await Promise.all([fetchSet(), fetchSet()]), [true, true]);
    assert.deepEqual(readdirSync(cache), [good]);
    const distinct = [...new Set(requests)];
    assert.deepEqual(distinct.slice(0, 4), [`${base}/commits/${head}.json`, `${base}/manifests/${stale}.json`, `${base}/recent.json`, `${base}/manifests/${good}.json`]);
    assert.deepEqual(readFileSync(fromSets('contract', env).path), tool);
    assert.deepEqual(readFileSync(fromSets('motion.wasm', env).path), wasm);
    rmSync(resolve(cache, good, 'contract'));
    assert.equal(await fetchSet(), true);
    server.stop(true);
    assert.deepEqual(readFileSync(fromSets('contract', env).path), tool);
  } finally { server.stop(true); m.done(); }
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

test('a set built here is taken over a fresh debug compiler, which answers once an input differs from the set\'s', () => {
  const changed = [ours[0], ['contract/syntax/src', 'f'.repeat(64)]];
  for (const [inputs, taken] of [[ours, 'set'], [changed, 'debug']]) {
    const m = machine([{ source: 'built', inputs, commit: 'a'.repeat(40) }], { rust: true });
    try {
      const debug = debugCompiler(m.env.HOME, 'contract'), set = m.bin('contract');
      assert.equal(freshCompiler(debug), true);
      const chosen = compilerFrom('contract', debug, m.env);
      assert.equal(chosen?.path, taken === 'set' ? set : debug);
      assert.equal(chosen?.set?.dir ?? null, taken === 'set' ? dirname(set) : null);
    } finally { m.done(); }
  }
}, 60000);

test('a fresh debug compiler takes precedence for dev flags and over an allowed fetched set', () => {
  const built = machine([{ source: 'built', inputs: ours }], { rust: true });
  const fetched = machine([{ source: 'fetched', inputs: ours }], { rust: false });
  try {
    const debug = debugCompiler(built.env.HOME, 'contract');
    for (const flags of [{ RUSTFLAGS: '' }, { CARGO_ENCODED_RUSTFLAGS: '-g' }, { CARGO_PROFILE_DEV_DEBUG: 'true' }]) {
      const env = { ...built.env, ...flags };
      assert.deepEqual(compilerFrom('contract', debug, env), { path: debug, set: null }, JSON.stringify(flags));
      assert.equal(compilerFrom('contract', `${debug}-absent`, env)?.path, built.bin('contract'));
    }
    assert.deepEqual(compilerFrom('contract', debug, fetched.env), { path: debug, set: null });
    const kept = resolve(fetched.env.HOME, 'kept-contract');
    assert.deepEqual(compilerFrom('contract', `${debug}-absent`, fetched.env, { kept: () => kept }), { path: kept, set: null });
    assert.equal(compilerFrom('contract', `${debug}-absent`, fetched.env)?.path, fetched.bin('contract'));
  } finally { built.done(); fetched.done(); }
});

test('a set kept without build verification answers only in the absence of fresh debug', () => {
  const m = machine([{ source: 'built', inputs: ours, verifiedBuild: false }], { rust: true });
  try {
    const debug = debugCompiler(m.env.HOME, 'contract');
    assert.equal(compilerFrom('contract', `${debug}-absent`, m.env)?.path, m.bin('contract'));
    assert.equal(compilerFrom('contract', debug, m.env)?.path, debug);
  } finally { m.done(); }
});

test.skipIf(process.platform === 'win32')('exact contract forma serve asks for the fresh compiler with unwinding', () => {
  const m = machine([{ source: 'built', inputs: ours }], { rust: true });
  try {
    const debug = debugCompiler(m.env.HOME, 'contract', undefined, '#!/bin/sh\nexit 37\n');
    writeFileSync(m.bin('contract'), '#!/bin/sh\nexit 38\n');
    const result = spawnSync(process.execPath, ['-e', `
      import { contract } from './scripts/exact.mjs';
      const debug = ${JSON.stringify(debug)};
      console.log(JSON.stringify([await contract(['forma', 'serve', '--stdio'], process.env, { debug }), await contract(['forma', 'check'], process.env, { debug })]));
    `], { cwd: ROOT, env: m.env, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), [37, 38]);
  } finally { m.done(); }
});

test('a set match is rehashed before it can displace a fresh debug compiler', () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(ROOT, 'target', 'exact-input-'));
  const input = resolve(dir, 'source.rs'), file = `target/${basename(dir)}/source.rs`;
  writeFileSync(input, 'old');
  utimesSync(input, 1700000000, 1700000000);
  const m = machine([{ source: 'built', inputs: [[file, digest(input)]] }], { rust: true });
  try {
    const debug = debugCompiler(m.env.HOME, 'contract', input), before = statSync(input);
    assert.equal(fromSet('contract', 'built', m.env)?.path, m.bin('contract'));
    const stamp = JSON.parse(readFileSync(resolve(m.env.CARGO_TARGET_DIR, 'tools/contract.json'), 'utf8'));
    assert.equal(stamp.entry, dirname(m.bin('contract')));
    writeFileSync(input, 'new');
    utimesSync(input, before.atime, before.mtime);
    assert.equal(statSync(input).mtimeMs, before.mtimeMs);
    assert.equal(statSync(input).size, before.size);
    assert.equal(compilerFrom('contract', `${debug}-absent`, m.env)?.path, m.bin('contract'));
    assert.deepEqual(compilerFrom('contract', debug, m.env), { path: debug, set: null });
    assert.equal(compilerFrom('contract', `${debug}-absent`, m.env, { rehash: true }), null);
  } finally { m.done(); rmSync(dir, { recursive: true, force: true }); }
});

test('authored tests use the configured compiler target and preserve compiler and launch failures', () => {
  const m = machine([], { rust: true }), dir = realpathSync(m.env.HOME);
  const cargo = resolve(m.env.PATH, 'cargo' + EXE), source = resolve(dir, 'compiler-fixture.js');
  const file = resolve(dir, 'suite.test.contract'), trace = resolve(dir, 'compiler.json');
  const runner = resolve(dir, 'run.mjs');
  const env = { ...process.env, ...m.env, COMPILER_TRACE: trace, EXACT_TOOLS_ORIGIN: 'none' };
  const run = overrides => spawnSync(process.execPath, [runner], { cwd: dir, env: { ...env, ...overrides }, encoding: 'utf8' });
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const relativeTarget = mkdtempSync(resolve(ROOT, 'target/relative compiler '));
  try {
    // One host executable serves as Cargo and the compiler it installs, including on Windows.
    writeFileSync(source, `import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
const args = process.argv.slice(2);
if (args.includes('run') || args.includes('build')) {
  const bin = resolve(process.env.CARGO_TARGET_DIR, 'debug', 'contract' + (process.platform === 'win32' ? '.exe' : ''));
  mkdirSync(dirname(bin), { recursive: true });
  copyFileSync(process.execPath, bin);
  if (args.includes('build')) process.exit(0);
  process.exit(spawnSync(bin, args.slice(args.indexOf('--') + 1), { stdio: 'inherit' }).status ?? 1);
}
writeFileSync(process.env.COMPILER_TRACE, JSON.stringify([process.execPath, ...args]));
if (readFileSync(args[1], 'utf8') === 'refuse') { console.error('fixture.contract:7:3: invalid test step'); process.exit(2); }
console.log('{"tests":[],"properties":[]}');`);
    execFileSync(process.execPath, ['build', '--compile', source, '--outfile', cargo]);
    writeFileSync(runner, `import { runTests } from ${JSON.stringify(new URL('./agent.mjs', import.meta.url).href)};
try { console.log(JSON.stringify(await runTests({ host: 'linux', file: 'suite.test.contract' }))); }
catch (error) { console.error(error.message); process.exitCode = 1; }`);
    for (const target of [resolve(dir, 'absolute target'), relative(ROOT, relativeTarget)]) {
      const bin = resolve(ROOT, target, 'debug', 'contract' + EXE);
      writeFileSync(file, 'accept');
      const good = run({ CARGO_TARGET_DIR: target });
      assert.equal(good.status, 0, good.stderr);
      assert.deepEqual(JSON.parse(good.stdout), { passed: 0, failed: 0, skipped: 0, results: [] });
      const [launched, ...args] = JSON.parse(readFileSync(trace));
      assert.equal(relative(realpathSync(bin), realpathSync(launched)), '');
      assert.deepEqual(args, ['test', file]);
      writeFileSync(file, 'refuse');
      const cargoRefusal = run({ CARGO_TARGET_DIR: target });
      assert.equal(cargoRefusal.status, 1);
      assert.match(cargoRefusal.stderr, /fixture\.contract:7:3: invalid test step/);
      const escape = path => path.replaceAll(' ', '\\ ');
      writeFileSync(depInfoOf(bin), `${escape(bin)}: ${escape(source)}\n`);
      // With Cargo unavailable the runner must find the fresh compiler in this same target.
      const noCargo = { CARGO_TARGET_DIR: target, PATH: resolve(dir, 'absent') };
      const bad = run(noCargo);
      assert.equal(bad.status, 1);
      assert.match(bad.stderr, /fixture\.contract:7:3: invalid test step/);
      writeFileSync(file, 'accept');
      const reused = run(noCargo);
      assert.equal(reused.status, 0, reused.stderr);
      assert.deepEqual(JSON.parse(reused.stdout), JSON.parse(good.stdout));
    }
    for (const absent of [{ PATH: resolve(dir, 'absent') }, { RUSTUP_HOME: resolve(dir, 'absent') }]) {
      const missing = run({ ...absent, CARGO_TARGET_DIR: resolve(dir, 'empty target') });
      assert.equal(missing.status, 1);
      assert.match(missing.stderr, /rust-missing.*cargo/);
      assert.doesNotMatch(missing.stderr, /TypeError/);
    }
    const target = resolve(dir, 'broken target'), broken = debugCompiler(target, 'contract', source, 'invalid executable');
    if (process.platform !== 'win32') chmodSync(broken, 0o644);
    const refusal = spawnSync(broken, ['test', file], { cwd: ROOT, env: { ...env, CARGO_TARGET_DIR: target }, encoding: 'utf8' });
    assert.notEqual(refusal.status, 0);
    const failed = run({ CARGO_TARGET_DIR: target });
    assert.equal(failed.status, 1);
    assert.equal(failed.stderr.trim(), refusal.stderr?.trim() || refusal.error?.message || 'contract test compiler failed');
  } finally { rmSync(relativeTarget, { recursive: true, force: true }); m.done(); }
}, 60_000);

test('webCompiler and compilerSet select the same compiler', () => {
  const m = machine([{ source: 'built', inputs: ours }], { rust: true, crates: ['exact-web-js'] });
  const target = resolve(m.env.HOME, 'target');
  try {
    const debug = debugCompiler(target, 'exact-web-js'), built = m.bin('exact-web-js');
    const selected = (env = {}) => {
      const result = spawnSync(process.execPath, ['-e', `
        import { compilerSet, webCompiler, webCompilerFrom } from './host/web-js/module.mjs';
        console.log(JSON.stringify([webCompiler().cmd, compilerSet()?.dir ?? null, webCompilerFrom()?.path]));
      `], { cwd: ROOT, env: { ...m.env, CARGO_TARGET_DIR: target, ...env }, encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      return JSON.parse(result.stdout);
    };
    assert.deepEqual(selected(), [built, dirname(built), built]);
    assert.deepEqual(selected({ CARGO_PROFILE_DEV_OPT_LEVEL: '1' }), [debug, null, debug]);
    if (process.platform !== 'win32') {
      chmodSync(built, 0o644);
      assert.deepEqual(selected(), [debug, null, debug]);
    }
  } finally { m.done(); }
});

test.skipIf(process.platform === 'win32')('builtContract passes over a set with a non-executable compiler', () => {
  const m = machine([{ source: 'built', inputs: ours }, { source: 'built', inputs: ours }], { rust: true });
  try {
    const newest = m.bin('contract', 1);
    assert.equal(builtContract(m.env)?.cmd, newest);
    chmodSync(newest, 0o644);
    assert.equal(builtContract(m.env)?.cmd, m.bin('contract'));
  } finally { m.done(); }
});

function keepFixture() {
  const m = machine([], { rust: false });
  const root = resolve(m.env.HOME, 'checkout'), file = 'sources/input.rs', product = 'contract';
  const git = (...args) => execFileSync('git', ['-C', root, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  const commit = message => {
    git('add', '--all');
    git('-c', 'user.name=Exact', '-c', 'user.email=exact@example.invalid', '-c', 'commit.gpgSign=false', 'commit', '-qm', message);
    return git('rev-parse', 'HEAD');
  };
  const artifact = resolve(m.env.HOME, product);
  const run = ({ duringBuild, inputs = ['sources'], env = {}, beforeDigest } = {}) => keep(hostTriple(), {
    root, env: { ...m.env, ...env }, readMetadata: () => ({ dirs: ['sources'], metadata: { inputs: [] } }),
    build() {
      copyFileSync(process.platform === 'win32' ? process.execPath : '/usr/bin/true', artifact);
      duringBuild?.();
      const output = { file: product, get crate() { beforeDigest?.(); return product; }, inputs };
      return { products: { [product]: output }, files: new Map([[product, artifact]]) };
    },
  });
  try {
    mkdirSync(resolve(root, 'sources'), { recursive: true });
    git('init', '-q');
    writeFileSync(resolve(root, 'Cargo.toml'), '[workspace]\n');
    writeFileSync(resolve(root, 'Cargo.lock'), '[[package]]\nname = "contract"\nversion = "0.1.0"\n');
    writeFileSync(resolve(root, '.gitignore'), '.DS_Store\nignored-link\n');
    writeFileSync(resolve(root, file), 'first');
    return { ...m, root, file, product, git, commit, run };
  } catch (error) { m.done(); throw error; }
}

test('keep verifies committed tool inputs before and after building', async () => {
  const m = keepFixture();
  const { root, file, product, git, commit, run } = m;
  try {
    mkdirSync(resolve(root, 'apps/unrelated'), { recursive: true });
    const app = resolve(root, 'apps/unrelated/app.contract');
    writeFileSync(app, 'first');
    const first = commit('first');
    writeFileSync(app, 'other');
    const second = commit('second');
    git('checkout', '-q', '--detach', first);
    const prior = await run();
    assert.equal(prior.manifest.verifiedBuild, true);
    assert.equal(JSON.parse(readFileSync(resolve(prior.dir, 'manifest.json'), 'utf8')).verifiedBuild, true);
    assert.equal(spawnSync(resolve(prior.dir, product), process.platform === 'win32' ? ['--version'] : []).status, 0);
    writeFileSync(resolve(root, 'sources/.DS_Store'), 'ignored');
    const withIgnored = await run();
    assert.deepEqual(withIgnored.manifest.products[product].inputs, [['sources', digest(resolve(root, 'sources'))]]);
    writeFileSync(resolve(root, file), 'dirty before the build');
    await assert.rejects(run({ duringBuild: () => git('restore', file) }), /keeps committed sources only:[\s\S]*sources\/input\.rs/);
    assert.equal(readFileSync(resolve(root, file), 'utf8'), 'dirty before the build');
    git('restore', file);
    mkdirSync(resolve(root, 'ud'));
    writeFileSync(resolve(root, 'ud/notes'), 'untracked');
    assert.equal((await run()).dir, withIgnored.dir);
    writeFileSync(app, 'uncommitted app edit');
    assert.equal((await run()).dir, withIgnored.dir);
    git('restore', 'apps/unrelated/app.contract');
    rmSync(resolve(root, 'ud'), { recursive: true });
    await assert.rejects(run({ duringBuild: () => git('checkout', '-q', '--detach', second) }), /HEAD changed while building the tool set\. No set was kept\./);
    assert.equal(git('status', '--porcelain'), '');
    const untracked = 'sources/untracked.rs';
    git('config', 'status.showUntrackedFiles', 'no');
    await assert.rejects(run({ duringBuild: () => writeFileSync(resolve(root, untracked), 'untracked'), inputs: [untracked] }),
      /keeps committed sources only:[\s\S]*sources\/untracked\.rs/);
    rmSync(resolve(root, untracked));
    if (process.platform !== 'win32') {
      symlinkSync('ignored-link', resolve(root, 'apps/unrelated/ignored-link'));
      assert.equal((await run()).dir, withIgnored.dir);
    }
    assert.deepEqual(readdirSync(m.cache).sort(), [basename(prior.dir), basename(withIgnored.dir)].sort());
  } finally { m.done(); }
});

test.each([
  ['CARGO_PROFILE_HOST_DEV_OVERFLOW_CHECKS', 'true'],
  ['RUSTFLAGS', '-Cdebug-assertions=on'],
  ['CARGO_ENCODED_RUSTFLAGS', ''],
  ['CARGO_BUILD_RUSTFLAGS', '-Cdebug-assertions=on'],
  ['CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS', '-Cdebug-assertions=on'],
  ['CARGO_BUILD_DEP_INFO_BASEDIR', 'sources'],
  ['RustFlags', '-Cdebug-assertions=on'],
])('keep refuses the build override %s before building', async (name, value) => {
  const { cache, commit, run, done } = keepFixture();
  try {
    commit('first');
    let built = false;
    await assert.rejects(run({ env: { [name]: value }, duringBuild: () => { built = true; } }), new RegExp(name));
    assert.equal(built, false);
    assert.throws(() => readdirSync(cache), { code: 'ENOENT' });
  } finally { done(); }
});

test('keep rejects a restored input that was dirty outside the pre-build scope', async () => {
  const { root, cache, git, commit, run, done } = keepFixture();
  try {
    const file = 'docs/contract grammar.md';
    mkdirSync(resolve(root, 'docs'));
    writeFileSync(resolve(root, file), 'first');
    commit('first');
    writeFileSync(resolve(root, file), 'dirty before the build');
    await assert.rejects(run({ inputs: [file], duringBuild: () => git('restore', file) }), /docs\/contract grammar\.md was dirty before the build/);
    assert.equal(readFileSync(resolve(root, file), 'utf8'), 'first');

    const untracked = resolve(root, 'untracked');
    mkdirSync(untracked);
    writeFileSync(resolve(untracked, 'input.rs'), 'untracked before the build');
    await assert.rejects(run({ inputs: ['untracked/input.rs'], duringBuild: () => rmSync(untracked, { recursive: true }) }),
      /untracked\/ was dirty before the build/);
    assert.equal(git('status', '--porcelain'), '');
    assert.throws(() => readdirSync(cache), { code: 'ENOENT' });
  } finally { done(); }
});

test('keep checks Git status after reading the inputs for their digests', async () => {
  const { root, file, cache, commit, run, done } = keepFixture();
  try {
    commit('first');
    // The crate is read to compute the lockfile closure just before hashing the input files.
    const beforeDigest = () => writeFileSync(resolve(root, file), 'edited after the build');
    await assert.rejects(run({ inputs: [file], beforeDigest }), /keeps committed sources only:[\s\S]*sources\/input\.rs/);
    assert.equal(readFileSync(resolve(root, file), 'utf8'), 'edited after the build');
    assert.throws(() => readdirSync(cache), { code: 'ENOENT' });
  } finally { done(); }
});

test('keep takes the native executable and dep-info despite an inherited cross target', () => {
  const m = machine([], { rust: true }), triple = hostTriple();
  const target = m.env.CARGO_TARGET_DIR;
  const inherited = triple.startsWith('aarch64-') ? triple.replace('aarch64-', 'x86_64-') : triple.replace('x86_64-', 'aarch64-');
  try {
    const result = spawnSync(process.execPath, ['-e', `
      import { mock } from 'bun:test';
      import * as children from 'node:child_process';
      import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
      import { dirname, resolve } from 'node:path';
      import { EXE } from './scripts/paths.mjs';
      import { depInfoOf } from './scripts/cargo-dep-info.mjs';
      import * as filesystem from './scripts/filesystem.mjs';
      const { CARGO_TARGET_DIR: target, CARGO_BUILD_TARGET: inherited } = process.env;
      const host = ${JSON.stringify(triple)};
      const make = (path, body, input) => {
        mkdirSync(dirname(path), { recursive: true });
        writeFileSync(path, body);
        const escape = path => path.replaceAll(' ', '\\\\ ');
        writeFileSync(depInfoOf(path), escape(path) + ': ' + escape(resolve(input)) + '\\n');
        return path;
      };
      const helper = make(resolve(target, 'debug', 'exact-filesystem' + EXE), 'helper', 'README.md');
      mock.module('./scripts/filesystem.mjs', () => ({ ...filesystem, builtFilesystemHelper: () => ({ directory: target, path: helper }) }));
      mock.module('./host/web-js/module.mjs', () => ({ leafRecipes: async () => [] }));
      const packages = ${JSON.stringify([...WEB_CRATES, 'contract', 'exact-filesystem', 'exact-web-capabilities'])}.map(name => ({ id: name, name, source: 'registry:fixture' }));
      const output = stdout => ({ status: 0, stdout, stderr: '' });
      mock.module('node:child_process', () => ({ ...children, spawnSync(command, args) {
        if (command === 'git') return output(args.includes('rev-parse') ? 'a'.repeat(40) : '');
        if (command === 'rustc') return output('host: ' + host);
        if (command === 'strip') return output('');
        if (args[0] === 'metadata') return output(JSON.stringify({ packages, resolve: { nodes: [] } }));
        const name = args[args.indexOf('--bin') + 1];
        const triple = args.includes('--target') ? args[args.indexOf('--target') + 1] : inherited;
        make(resolve(target, 'host-dev', name + EXE), 'stale', 'README.md');
        const path = make(resolve(target, triple, 'host-dev', name + EXE), 'built ' + name + ' for ' + triple, 'AGENTS.md');
        const artifact = { reason: 'compiler-artifact', target: { name, kind: ['bin'] }, filenames: [path],
          executable: name === 'contract' ? path : null };
        return output(JSON.stringify(artifact) + '\\n');
      }}));
      const { keep } = await import('./scripts/tools.mjs');
      const { dir, manifest } = await keep();
      for (const name of ['contract', 'exact-web-js']) {
        const kept = readFileSync(resolve(dir, name), 'utf8');
        if (kept !== 'built ' + name + ' for ' + host) throw new Error('kept ' + kept + ', expected ' + host);
        if (!manifest.products[name].inputs.some(([path]) => path === 'AGENTS.md')) throw new Error('used the wrong dep-info for ' + name);
      }
    `], { cwd: ROOT, env: { ...m.env, CARGO_TARGET_DIR: target, CARGO_BUILD_TARGET: inherited }, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  } finally { m.done(); }
});

test.skipIf(process.platform === 'win32')('the gallery selects compilers against the checkout target before falling back to Cargo', async () => {
  const { galleryLoop } = await import('../host/web-js/dev-gallery.mjs');
  const m = machine([{ source: 'built', inputs: ours }], { rust: true, crates: ['contract', 'exact-web-js'] });
  const root = m.env.HOME, target = resolve(root, 'target'), app = resolve(root, 'app');
  const build = resolve(root, 'host/web-js/build.mjs');
  const generator = text => `#!/bin/sh\n/bin/mkdir -p "$4"\nprintf '%s' '${text}' > "$4/app.contract"\nprintf '{}' > "$4/app.json"\n`;
  try {
    mkdirSync(dirname(build), { recursive: true });
    mkdirSync(app);
    debugCompiler(target, 'contract', undefined, generator('debug'));
    writeFileSync(m.bin('contract'), generator('set'));
    const debug = debugCompiler(target, 'exact-web-js');
    const override = debugCompiler(resolve(root, 'override'), 'exact-web-js');
    writeFileSync(build, `
      import { readFileSync } from 'node:fs';
      import { webCompiler } from ${JSON.stringify(pathToFileURL(resolve(ROOT, 'host/web-js/module.mjs')).href)};
      console.log(JSON.stringify([readFileSync(process.argv[2] + '/app.contract', 'utf8'), process.env.EXACT_WEB_COMPILER ?? null, webCompiler().cmd, process.env.CARGO_TARGET_DIR]));
      process.exitCode = 1;
    `);
    const preview = async (env = {}, compiler) => {
      const { promise, resolve: reported } = Promise.withResolvers();
      const loop = galleryLoop({ root, app: { dir: app, id: 'selection' }, appDist: resolve(root, 'dist'), client: () => '', log: () => {},
        compiler, env: { ...m.env, EXACT_TOOLS_ORIGIN: 'none', CARGO_TARGET_DIR: target, ...env } });
      try {
        await loop.handle({ on() {} }, { on() {}, writeHead() {}, write(text) {
          if (text.startsWith('data: ')) {
            const { error } = JSON.parse(text.slice(6));
            if (error) reported(error);
          }
        } }, new URL('http://example.test/__dev/gallery/stream'));
        return { result: JSON.parse(await promise), cargoTarget: resolve(loop.cache, 'cargo') };
      } finally { await loop.stop(); }
    };
    assert.deepEqual((await preview({ EXACT_WEB_COMPILER: override }, () => debug)).result, ['set', override, override, target]);
    assert.deepEqual((await preview({ EXACT_WEB_COMPILER: '' }, () => debug)).result, ['set', debug, debug, target]);
    assert.deepEqual((await preview()).result, ['set', null, m.bin('exact-web-js'), target]);
    assert.deepEqual((await preview({ RUSTFLAGS: '-g' })).result, ['debug', null, debug, target]);
    const receipt = resolve(dirname(m.bin('exact-web-js')), 'manifest.json');
    const manifest = JSON.parse(readFileSync(receipt, 'utf8'));
    manifest.products['exact-web-js'].inputs = [['rust-toolchain.toml', 'different']];
    writeFileSync(receipt, JSON.stringify(manifest));
    assert.deepEqual((await preview()).result, ['set', null, debug, target]);
    rmSync(depInfoOf(debug));
    const { result, cargoTarget } = await preview();
    assert.deepEqual(result, ['set', null, 'cargo', cargoTarget]);
  } finally { m.done(); }
});

test('when a cached compiler disappears, the next matching set supplies it', () => {
  const m = machine(['a', 'b'].map(c => ({ source: 'built', inputs: ours, commit: c.repeat(40) })), { rust: false });
  try {
    const bin = i => resolve(m.env.HOME, '.cache/exact/tools', hostTriple(), `set${i}`, 'contract');
    assert.equal(fromSets('contract', m.env)?.path, bin(1));
    rmSync(bin(1));
    assert.equal(fromSets('contract', m.env)?.path, bin(0));
    rmSync(bin(0));
    assert.equal(fromSets('contract', m.env), null);
    assert.match(new RustMissing('contract', m.env).message, /tool set built at bbbbbbbbbbbb is missing contract/);
  } finally { m.done(); }
});

test('web setup accepts a complete matching tool set for a Contract app and keeps the source requirements for a Rust app', () => {
  const m = machine([{ source: 'built', inputs: ours, commit: 'a'.repeat(40) }], { rust: false });
  try {
    const dir = resolve(m.env.HOME, '.cache/exact/tools', hostTriple(), 'set0'), receipt = resolve(dir, 'manifest.json');
    const manifest = JSON.parse(readFileSync(receipt, 'utf8'));
    for (const file of ['exact-web-js', 'exact-filesystem', 'markdown.wasm', 'motion.wasm', 'markup-editor.wasm', 'textflow.wasm']) {
      writeFileSync(resolve(dir, file), '', { mode: 0o755 });
      manifest.products[file] = { ...manifest.products.contract, file };
    }
    manifest.metadata = { record: { packages: [], resolve: { nodes: [] } }, inputs: ours };
    writeFileSync(receipt, JSON.stringify(manifest));
    const app = resolve(m.env.HOME, 'starter'), script = resolve(app, 'web/build.rs');
    mkdirSync(resolve(app, 'web'), { recursive: true });
    writeFileSync(resolve(app, 'app.contract'), '');
    const check = (chrome = process.execPath) => spawnSync(process.execPath, [resolve(ROOT, 'scripts/exact.mjs'), 'setup', '--check', '--platform', 'web'],
      { cwd: app, encoding: 'utf8', env: { ...m.env, CHROME: chrome, EXACT_TOOLS_ORIGIN: 'none' } });
    const needsRust = () => {
      const result = check();
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stdout, /MISSING rustup/);
      assert.doesNotMatch(result.stdout, /Prebuilt web tools/);
    };
    writeFileSync(script, 'fn main() { exact_js_bake::build(); }');
    const ready = check();
    assert.equal(ready.status, 0, ready.stderr);
    assert.match(ready.stdout, /Prebuilt web tools/);
    assert.doesNotMatch(ready.stdout, /rustup|Cargo crates/);
    const noBrowser = check(resolve(app, 'absent-chrome'));
    assert.equal(noBrowser.status, 1);
    assert.match(noBrowser.stdout, /MISSING Chrome/);
    writeFileSync(script, 'fn main() { println!("cargo:rerun-if-changed=data.json"); }');
    needsRust();
    rmSync(script);
    rmSync(resolve(dir, 'motion.wasm'));
    needsRust();
    writeFileSync(resolve(dir, 'motion.wasm'), '');
    manifest.metadata.inputs = [[ours[0][0], '0'.repeat(64)]];
    writeFileSync(receipt, JSON.stringify(manifest));
    needsRust();
  } finally { m.done(); }
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

test('the rust-missing diagnostic includes the configured origin failure', () => {
  const m = machine([], { rust: false });
  try {
    const env = { ...m.env, EXACT_TOOLS_ORIGIN: 'invalid:' };
    assert.equal(fromSets('contract', env), null);
    assert.match(new RustMissing('contract', env).message, /rust-missing.*tools origin needs an HTTP or HTTPS URL/);
  } finally { m.done(); }
});

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
