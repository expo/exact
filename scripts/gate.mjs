#!/usr/bin/env bun
/** The only blocking verification command (LLP 1172.010 E13; Charlie's 30 s gate). */
import { spawn } from 'node:child_process';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const formatPart = ['fmt', process.execPath, ['scripts/gate.mjs', '--fmt']];
export const gateParts = [
  formatPart,
  ['caps', process.execPath, ['scripts/caps.mjs']],
  ['boot', process.execPath, ['scripts/boot.mjs']],
  ['seams', process.execPath, ['scripts/seam-ratchets.mjs']],
  ['scripts', process.execPath, ['test', '--isolate', './scripts/gate.test.mjs', './scripts/async-bisect.test.mjs', './scripts/seam-ratchets.test.mjs']],
  ['check', 'cargo', ['check', '--keep-going']],
  ...['exact-num', 'exact-color', 'exact-motion', 'exact-plan', 'contract-syntax', 'contract-types', 'contract-checking']
    .map(pkg => [`unit:${pkg}`, 'cargo', ['test', '--lib', '--no-fail-fast', '-p', pkg]]),
];

export function gateMetric(seconds, budget, failed = []) {
  return ['blocking gate (warm)', Number.isFinite(seconds) ? `${seconds.toFixed(1)} s` : 'n/a',
    `${failed.length ? `VIOLATION: ${failed.join(', ')}; ` : ''}budget ${budget} s${seconds > budget ? '; VIOLATION: over budget' : ''}`];
}

function command(command, args, cwd) {
  return new Promise(done => {
    const child = spawn(command, args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    let log = '', stdout = '';
    child.stdout.on('data', data => { log += data; stdout += data; });
    child.stderr.on('data', data => { log += data; });
    child.on('error', error => done({ status: 127, log: `${log}${error.message}\n` }));
    child.on('close', status => done({ status: status ?? 127, log, stdout }));
  });
}

/** Cargo fmt gives rustfmt every target of an edition in one command, exceeding Windows' 32,767 UTF-16 limit. */
export async function checkFormat({ root = process.cwd(), platform = process.platform, run = command } = {}) {
  if (platform !== 'win32') return run('cargo', ['fmt', '--all', '--', '--check'], root);
  const canonical = path => { try { return realpathSync(path); } catch { return resolve(path); } };
  const visited = new Set(), targets = new Map();
  // Match cargo fmt --all: workspace targets and recursively discovered local path dependencies, including tests and examples.
  const collect = async manifest => {
    manifest = canonical(manifest);
    if (visited.has(manifest)) return;
    visited.add(manifest);
    const args = ['metadata', '--format-version', '1', '--no-deps', '--manifest-path', manifest];
    let result = await run('cargo', [...args, '--offline'], root);
    if (result.status !== 0) result = await run('cargo', args, root);
    if (result.status !== 0) throw new Error(result.log || 'cargo metadata failed');
    const { packages } = JSON.parse(result.stdout);
    for (const pkg of packages) visited.add(canonical(pkg.manifest_path));
    for (const pkg of packages) {
      for (const target of pkg.targets) targets.set(canonical(target.src_path), target.edition);
      for (const dep of pkg.dependencies) {
        if (dep.path && existsSync(resolve(dep.path, 'Cargo.toml'))) await collect(resolve(dep.path, 'Cargo.toml'));
      }
    }
  };
  await collect(resolve(root, 'Cargo.toml'));
  if (!targets.size) return { status: 1, log: 'cargo fmt found no targets' };
  const editions = new Map();
  for (const [path, edition] of [...targets].sort(([a], [b]) => a.localeCompare(b))) {
    if (!editions.has(edition)) editions.set(edition, []);
    editions.get(edition).push(path);
  }
  let status = 0, log = '';
  const flush = async (edition, files) => {
    const result = await run(process.env.RUSTFMT || 'rustfmt', [...files, '--edition', edition, '--check'], root);
    if (result.status !== 0) status ||= result.status;
    log += result.log || '';
  };
  for (const [edition, files] of editions) {
    let batch = [], length = 0;
    for (const file of files) {
      // Allow twice each path's length for Windows argument quoting, plus room for the executable and flags.
      const size = file.length * 2 + 3;
      if (batch.length && length + size > 24000) { await flush(edition, batch); batch = []; length = 0; }
      batch.push(file); length += size;
    }
    if (batch.length) await flush(edition, batch);
  }
  return { status, log };
}

export async function runGate({ root = fileURLToPath(new URL('..', import.meta.url)), parts = gateParts,
  run = command, report = console.log, now = () => performance.now(), budget } = {}) {
  budget ??= Number(/\| Blocking gate suite\s*\|\s*(\d+)s/.exec(readFileSync(`${root}/rules/RULES.md`, 'utf8'))?.[1]);
  if (!(budget > 0)) throw new Error('rules/RULES.md must declare the Blocking gate suite budget in seconds');
  const start = now(), results = [];
  const part = async ([name, cmd, args]) => {
    const begin = now();
    let result;
    try { result = await run(cmd, args, root); }
    catch (error) { result = { status: 127, log: error.message }; }
    const seconds = (now() - begin) / 1000;
    results.push({ name, status: result.status, seconds });
    report(`${name}: ${seconds.toFixed(2)} s, ${result.status === 0 ? 'PASS' : `FAIL (exit ${result.status})`}${result.log?.trim() ? `\n${result.log.trim()}` : ''}`);
  };
  // Static checks share no artifacts. Cargo commands run in order, so none waits on another's lock.
  const cargo = p => p[1] === 'cargo' && p[2][0] !== 'fmt';
  await Promise.all([ ...parts.filter(p => !cargo(p)).map(part),
    (async () => { for (const p of parts.filter(cargo)) await part(p); })(),
  ]);
  const seconds = (now() - start) / 1000, failed = results.filter(r => r.status !== 0).map(r => r.name);
  const over = seconds > budget;
  report(`gate: ${seconds.toFixed(2)} s total / ${budget} s budget; ${failed.length ? `FAIL: ${failed.join(', ')}` : 'all parts passed'}${over ? '; OVER BUDGET (warm the cache; record machine load)' : ''}`);
  return { seconds, budget, failed, over, parts: results, status: failed.length ? 1 : 0 };
}

if (import.meta.main) {
  if (process.argv.includes('--help')) console.log('Usage: bun scripts/gate.mjs [--fmt]\nThe blocking gate. Prints each part and the total; runs every part after failures.\n  --fmt  Check Rust formatting only, batching targets on Windows.');
  else {
    const root = fileURLToPath(new URL('..', import.meta.url));
    const { ensureBun } = await import('./bun.mjs');
    ensureBun(JSON.parse(readFileSync(`${root}/package.json`, 'utf8')).packageManager.replace(/^bun@/, ''));
    if (process.argv.includes('--fmt')) {
      const result = await checkFormat({ root });
      if (result.log) process.stdout.write(result.log);
      process.exitCode = result.status;
    } else process.exitCode = (await runGate()).status;
  }
}
