// `bun exact.mjs verify`'s `contract verify` (LLP 1151 D8): started beside the device run, its report kept in a file,
// and the part of it the console shows.
import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

// `contract verify` (LLP 1151 D8) reads only the app's Contract and its tests, so it runs beside the device run rather
// than after it. Its report and receipt (D7) go whole to .exact/verify/properties.txt. The console keeps failure
// details and folds the result counts and file path into the final verdict.
export function contractVerify(app, json) {
  let output = '', errors = '';
  // Its own process group, so that stopping it stops the tester `cargo run` starts under it too.
  const child = spawn('cargo', ['run', '-q', '-p', 'contract', '--', 'verify', resolve(app.dir, 'app.contract')], { cwd: ROOT, detached: true, stdio: ['ignore', 'pipe', json ? 'inherit' : 'pipe'] });
  const pass = signal => { try { process.kill(-child.pid, signal); } catch { /* It has exited. */ } }, signals = ['SIGINT', 'SIGTERM', 'SIGHUP'];
  // A listener takes the place of a signal's default, ending the process: after stopping the child, verify ends with
  // the signal's code (as `whenEnded` does), unless another listener (`whenEnded` itself) is there to end it.
  const codes = { SIGINT: 130, SIGTERM: 143, SIGHUP: 129 };
  const ended = signal => { pass(signal); if (process.listenerCount(signal) === 1) process.exit(codes[signal]); };
  for (const signal of signals) process.on(signal, ended);
  child.stdout.on('data', data => { output += data; });
  child.stderr?.on('data', data => { errors += data; });
  const done = new Promise(resolveResult => {
    const finish = exit => { for (const signal of signals) process.off(signal, ended); resolveResult({ exit, output, errors }); };
    child.on('error', e => { errors += e.message; finish(1); });
    child.on('exit', exit => finish(exit ?? 1));
  });
  return { done, stop: () => pass('SIGTERM') };
}

// Lines of `contract verify`'s report that are routine: one per delivered or outside script, what the oracle cannot
// deliver, and the counts the receipt repeats.
const ROUTINE = /^(WHOLE|PART|NOTE|OUTSIDE) |^(undeliverable parser step kinds|delivered forms|runner|properties|commit kinds the trace leaves out|world): /;

/** The console's part of a `contract verify` report kept whole in `file`. */
export function semanticsSummary({ exit, output, errors }, file) {
  const lines = output.split('\n');
  const receiptAt = lines.findIndex(l => l.startsWith('verify receipt'));
  const observedAt = lines.findIndex(l => l.startsWith('Observed over'));
  const suggested = observedAt >= 0 ? lines.slice(observedAt, receiptAt >= 0 ? receiptAt : undefined).filter(l => /^(always|after|never) /.test(l)).length : 0;
  const findings = output.match(/^runner: .*?; (\d+ findings?)\b/m)?.[1];
  const properties = lines.find(l => l.startsWith('properties: '))?.split(';')[0];
  const head = [`contract verify: ${exit ? `failed (exit ${exit})` : 'done'}`, findings, properties,
    observedAt >= 0 && `${plural(suggested, 'suggested property', 'suggested properties')} (observed, not proved: keep the ones the app means)`, `report and receipt: ${file}`].filter(Boolean).join('; ');
  if (!exit) return [head];
  const said = [...lines.slice(0, observedAt >= 0 ? observedAt : receiptAt >= 0 ? receiptAt : undefined), ...errors.split('\n')].filter(l => l.trim() && !ROUTINE.test(l));
  const shown = said.slice(0, 40);
  if (said.length > shown.length) shown.push(`  … and ${said.length - shown.length} more lines (${file})`);
  return [head, ...shown];
}
