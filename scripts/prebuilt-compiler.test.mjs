import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { prebuiltCompiler } from './exact.mjs';

// EXACT_CONTRACT_BIN stands in for the compiler a checkout would build, unless it is missing or its stamp names
// another commit (exact-build#172).
test('a prebuilt Contract compiler is used only when it exists and was built from this checkout', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-contract-bin-')), bin = resolve(dir, 'contract');
  const head = spawnSync('git', ['-C', resolve(import.meta.dir, '..'), 'rev-parse', 'HEAD'], { encoding: 'utf8' }).stdout.trim();
  try {
    expect(prebuiltCompiler({})).toBeNull();
    expect(prebuiltCompiler({ EXACT_CONTRACT_BIN: bin })).toBeNull();
    writeFileSync(bin, '');
    expect(prebuiltCompiler({ EXACT_CONTRACT_BIN: bin })).toBe(bin);
    writeFileSync(`${bin}.commit`, `${head}\n`);
    expect(prebuiltCompiler({ EXACT_CONTRACT_BIN: bin })).toBe(bin);
    writeFileSync(`${bin}.commit`, '0000000000000000000000000000000000000000\n');
    expect(prebuiltCompiler({ EXACT_CONTRACT_BIN: bin })).toBeNull();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
