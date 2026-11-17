// Whether the web compiler a build would reuse is fresh (module.mjs `freshCompiler`): its dep-info's sources, the
// manifests of the crates they are in and the workspace lockfile, found beside a Windows .exe too.
import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { buildScriptDir, crateDir, depInfoOf, fresh, freshCompiler, webCompiler } from './module.mjs';

test('compiler reuse is invalidated by dependency manifests and the workspace lockfile', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-compiler-fresh-'));
  const product = resolve(dir, 'compiler'), crate = resolve(dir, 'dep'), source = resolve(crate, 'src/lib.rs');
  const lock = resolve(import.meta.dir, '../../Cargo.lock'), previous = statSync(lock), fast = process.env.EXACT_WEB_COMPILER;
  try {
    mkdirSync(resolve(crate, 'src'), { recursive: true });
    writeFileSync(source, 'pub fn run() {}'); writeFileSync(resolve(crate, 'Cargo.toml'), '[package]\nname="fixture"\nversion="0.1.0"\n');
    writeFileSync(product, 'compiler fixture'); writeFileSync(`${product}.d`, `${product}: ${source}\n`);
    const built = Date.now() / 1000 + 10;
    utimesSync(product, built, built);
    process.env.EXACT_WEB_COMPILER = product;
    expect(freshCompiler(product)).toBe(true); expect(webCompiler().cmd).toBe(product);
    utimesSync(resolve(crate, 'Cargo.toml'), built + 1, built + 1);
    expect(fresh(product, `${product}.d`)).toBe(true);
    expect(freshCompiler(product)).toBe(false); expect(webCompiler().cmd).not.toBe(product);
    utimesSync(resolve(crate, 'Cargo.toml'), built - 1, built - 1);
    utimesSync(lock, built + 1, built + 1);
    expect(fresh(product, `${product}.d`)).toBe(true);
    expect(freshCompiler(product)).toBe(false); expect(webCompiler().cmd).not.toBe(product);
  } finally {
    utimesSync(lock, previous.atime, previous.mtime);
    if (fast === undefined) delete process.env.EXACT_WEB_COMPILER; else process.env.EXACT_WEB_COMPILER = fast;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a Windows compiler is found by its .exe beside the dep-info Cargo names without it', () => {
  expect(depInfoOf('C:\\t\\debug\\exact-web-js.exe')).toBe('C:\\t\\debug\\exact-web-js.d');
  expect(depInfoOf('/t/debug/exact-web-js')).toBe('/t/debug/exact-web-js.d');
  expect(crateDir('C:\\w\\contract\\src\\lib.rs')).toBe('C:\\w\\contract');
  expect(crateDir('/w/contract/src/lib.rs')).toBe('/w/contract');
  expect(crateDir('C:\\w\\Cargo.lock')).toBe(null);
  expect(buildScriptDir('C:\\t\\debug\\build\\kernel-0a1b\\out\\tables.rs')).toBe('C:\\t\\debug\\build\\kernel-0a1b');
  expect(buildScriptDir('/t/debug/build/kernel-0a1b/out/tables.rs')).toBe('/t/debug/build/kernel-0a1b');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-compiler-exe-'));
  try {
    const product = resolve(dir, 'compiler.exe'), source = resolve(dir, 'dep/src/lib.rs');
    mkdirSync(dirname(source), { recursive: true });
    writeFileSync(source, ''); writeFileSync(resolve(dir, 'dep/Cargo.toml'), '[package]\nname="fixture"\n');
    writeFileSync(product, 'compiler fixture'); writeFileSync(resolve(dir, 'compiler.d'), `${product}: ${source}\n`);
    const built = Date.now() / 1000 + 10;
    utimesSync(product, built, built);
    expect(freshCompiler(product)).toBe(true);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
