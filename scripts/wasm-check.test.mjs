// The hourly wasm graph, compilation commands and complete diagnostics.
import { afterAll, test, expect } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { report, wasmCheck, wasmCrates } from './wasm-check.mjs';

const ROOT = resolve(import.meta.dir, '..');
const TARGET = mkdtempSync(join(tmpdir(), 'wasm-check-')); // a cold target directory, never compiled into
afterAll(() => rmSync(TARGET, { recursive: true, force: true }));
const FOUND = { crates: ['contract', 'exact-kernel', 'exact-web'], bins: [['exact-textflow', 'textflow-web']], dirs: ['contract/cli/', 'kernel/', 'host/web/', 'textflow/'], target: TARGET };

test('the crates checked are the libraries the web apps link for wasm32, the apps and vendored crates aside', () => {
  const { crates, bins, roots } = wasmCrates(ROOT);
  expect(bins).toEqual([['exact-textflow', 'textflow-web']]);
  expect(roots).toContain('apps/caltrain/web');
  // contract: G2's cfg(unix)-only name (5540abbfc0) broke it there; exact-web: the host every web app links
  for (const c of ['contract', 'exact-web', 'exact-runner', 'exact-kernel', 'exact-markdown-editor', 'exact-web-js-motion']) expect(crates).toContain(c);
  expect(crates.some((c) => c.endsWith('-web') && c !== 'exact-web' && c !== 'exact-js-web')).toBe(false);
  expect(crates).not.toContain('taffy');
}, 30_000);

test('the hourly check always runs every library and binary, even after a failed compile', () => {
  const calls = [];
  const result = wasmCheck(ROOT, { found: { ...FOUND }, execute: (root, cmd, args) => {
    calls.push([root, cmd, args]);
    return { status: calls.length === 1 ? 101 : 0, stderr: 'contract/src/lib.rs:1:1: error[E0425]: missing name' };
  } });
  expect(calls.length).toBe(2);
  expect(calls[0][2]).toEqual(expect.arrayContaining(['--lib', '--keep-going', '-p', 'contract', '-p', 'exact-kernel', '-p', 'exact-web']));
  expect(calls[1][2]).toEqual(expect.arrayContaining(['-p', 'exact-textflow', '--bin', 'textflow-web']));
  expect(result.problems.length).toBe(1);
});

test('a failure says every diagnostic, a build script\'s cause beneath it, and a rerun for each check that failed', () => {
  const lib = ['--lib', '--keep-going', '-p', 'contract', '-p', 'exact-kernel', '-p', 'exact-web'], bin = ['-p', 'exact-textflow', '--bin', 'textflow-web'];
  const { problems, counted } = report([
    { args: lib, status: 101, stderr: [
      'contract/cli/src/gallery.rs:444:5: error[E0425]: cannot find value `made` in this scope: not found in this scope',
      'error: could not compile `contract` (lib) due to 1 previous error',
      'error: failed to run custom build command for `exact-kernel v0.1.0 (/x/kernel)`', '', 'Caused by:',
      '  process didn\'t exit successfully: `/x/build-script-build` (exit status: 101)', '  --- stderr',
      "  thread 'main' panicked at kernel/build.rs:12:5:", '  schema.json: expected value at line 3 column 1',
      'warning: build failed, waiting for other jobs to finish...'].join('\n') },
    { args: bin, status: 101, stderr: 'textflow/src/web.rs:9:1: error[E0433]: failed to resolve: use of undeclared crate `libc`\nerror: could not compile `exact-textflow` (bin "textflow-web") due to 1 previous error' },
  ], FOUND, 'Cargo decides what changed');
  expect(counted).toBe('wasm32: 3 crates and 1 binary the web apps link checked (Cargo decides what changed), 3 errors');
  expect(problems.length).toBe(1);
  const [{ message, detail }] = problems;
  expect(message).toContain('3 errors in contract, exact-kernel, exact-textflow.');
  expect(message).toContain('needs an arm for neither');
  expect(detail).toContain('gallery.rs:444:5: error[E0425]');
  expect(detail).toContain('schema.json: expected value at line 3 column 1');
  expect(detail).toContain('textflow/src/web.rs:9:1: error[E0433]');
  expect(detail).toContain('Rerun: cargo check --target wasm32-unknown-unknown --lib -p contract -p exact-kernel; cargo check --target wasm32-unknown-unknown -p exact-textflow --bin textflow-web');
  // One that could not run is said beside the others, not in their place
  const partial = report([{ args: lib, status: 0, stderr: '' }, { args: bin, thrown: 'cargo check exited null: killed' }], FOUND, 'x');
  expect(partial.problems[0].detail).toContain('could not run: cargo check exited null: killed');
});
