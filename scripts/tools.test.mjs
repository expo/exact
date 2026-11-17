// A tool set is taken by its inputs' bytes, and a fetched one only where it may run (LLP 1174 D3, D5).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { digest, lockClosure, workspaceManifest } from './kept.mjs';
import { cargoOnPath } from './cargo-check.mjs';
import { ROOT, RustMissing, fromSet, fromSets, hostTriple, rustMissing } from './tools.mjs';

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
