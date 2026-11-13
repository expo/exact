import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { prebuiltCompiler } from './exact.mjs';

// EXACT_CONTRACT_BIN stands in for the compiler a checkout would build only when its stamp names this checkout's
// commit and this machine's target, and the checkout has no edits (expo/exact-build#172).
test('a prebuilt Contract compiler is used only when it was built from this checkout for this machine', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-contract-bin-')), bin = resolve(dir, 'contract');
  const git = args => spawnSync('git', ['-C', resolve(import.meta.dir, '..'), ...args], { encoding: 'utf8' });
  const head = git(['rev-parse', 'HEAD']).stdout.trim(), clean = git(['diff', '--quiet', 'HEAD', '--']).status === 0;
  const triple = { 'darwin-arm64': 'aarch64-apple-darwin', 'darwin-x64': 'x86_64-apple-darwin', 'linux-arm64': 'aarch64-unknown-linux-gnu', 'linux-x64': 'x86_64-unknown-linux-gnu' }[`${process.platform}-${process.arch}`];
  const use = () => prebuiltCompiler({ EXACT_CONTRACT_BIN: bin });
  try {
    expect(prebuiltCompiler({})).toEqual({ path: null, why: null });
    expect(use().why).toContain('does not exist');
    writeFileSync(bin, '');
    expect(use().why).toContain('.commit');
    writeFileSync(`${bin}.commit`, `${head} wasm32-unknown-unknown\n`);
    expect(use().why).toContain('built for wasm32-unknown-unknown');
    writeFileSync(`${bin}.commit`, `0000000000000000000000000000000000000000 ${triple}\n`);
    expect(use().why).toContain('git -C');
    writeFileSync(`${bin}.commit`, `${head} ${triple}\n`);
    // This checkout's own state decides the last step: a clean one uses the binary, one with edits sets it aside.
    if (clean) expect(use()).toEqual({ path: bin, why: null });
    else expect(use().why).toContain('edits to tracked files');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
