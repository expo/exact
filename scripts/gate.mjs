#!/usr/bin/env bun
/** The only blocking verification command (LLP 1172.010 E13; Charlie's 30 s gate). */
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const gateParts = [
  ['fmt', 'cargo', ['fmt', '--all', '--', '--check']],
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
    let log = '';
    child.stdout.on('data', data => { log += data; });
    child.stderr.on('data', data => { log += data; });
    child.on('error', error => done({ status: 127, log: `${log}${error.message}\n` }));
    child.on('close', status => done({ status: status ?? 127, log }));
  });
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
  if (process.argv.includes('--help')) console.log('Usage: bun scripts/gate.mjs\nThe blocking gate. Prints each part and the total; runs every part after failures.');
  else {
    const root = fileURLToPath(new URL('..', import.meta.url));
    const { ensureBun } = await import('./bun.mjs');
    ensureBun(JSON.parse(readFileSync(`${root}/package.json`, 'utf8')).packageManager.replace(/^bun@/, ''));
    process.exitCode = (await runGate()).status;
  }
}
