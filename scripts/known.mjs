#!/usr/bin/env bun
/**
 * known — which of a local run's failures also fail on origin/main (LLP 1146 D1).
 *
 * The async lane attaches what it found at each commit it checks to that commit as a git note
 * (`refs/notes/async`, tier 2's `refs/notes/async2`; `publish` below), pushed to origin. This runs a
 * test command, prints its output as it comes, names its failures as the lane does (async.mjs
 * `failures`), and sorts each against the nearest first-parent ancestor of HEAD on origin/main whose
 * note ran its suite to the end, in a check of the same kind run whole or in the part that holds it:
 *
 *   known      it failed there too, with how often it failed in the last ten such notes
 *   new        its suite ran there and this test did not fail
 *   unknown    no note there ran its suite to the end, so nothing is known of it
 *
 * The exit status is the command's: a known failure is still a failure.
 *
 *   bun scripts/known.mjs [--no-fetch] -- <command> [args…]
 *     e.g. bun scripts/known.mjs -- cargo test -p exact-runner --lib
 *          bun scripts/known.mjs -- bun host/apple/build.mjs --test --ios
 *   bun scripts/known.mjs [--no-fetch] --log <file> --kind cargo|xctest|bun
 */
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { failures, owners } from './async.mjs';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
export const NOTE_REFS = ['refs/notes/async', 'refs/notes/async2'];
/** How many of a check's latest notes a failure's rate is counted over. */
const RATE_OVER = 10;

const git = (args, cwd = ROOT) => spawnSync('git', args, { cwd, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });

/** A failure without its check's name: `ios: ExactKitTests.A b failed` and `local: ExactKitTests.A b failed` are one test. */
export const identity = (failure) => failure.slice(failure.indexOf(': ') + 2);

/** The kind of test a check's command runs, which a local command of the same kind is compared with. */
export function kindOf(command) {
  if (/\bbuild\.mjs\b.*--test\b/.test(command)) return 'xctest';
  // (The async lane runs a selection's Rust tests through `affected.mjs --cargo-tests`.)
  if (/\bcargo\b.*\btest\b|\baffected\.mjs --cargo-tests\b/.test(command)) return 'cargo';
  if (/\bbun\b.*\btest\b/.test(command)) return 'bun';
  return null;
}

/** Whether check `name` stopped early in a run with these failures: a crate it built did not compile, a build script
 *  it ran failed, a test binary or file did not run to its end, it hung, or it failed with nothing a pattern names, so
 *  it says nothing of the tests it held. */
export const stoppedEarly = (name, failures) => failures.some((f) => f.startsWith(`${name}: `)
  && /: exit \d+ \(see log\)$| does not compile$| build script failed$| did not finish$|: an error between tests$| hung( past .*)?$/.test(f));

/** A lane's note for one commit: what each check ran, the part of it that ran when only a part did (`covers`) and
 *  whether it ran to the end (`complete`), and every failure. */
export function noteOf(result, tier) {
  const checks = {};
  for (const [name, c] of Object.entries(result.checks)) {
    // Use the lane's record, made as the check ended and before it kept earlier runs' failures, where there is one
    const complete = c.complete ?? !stoppedEarly(name, result.failures);
    checks[name] = c.skipped ? { command: c.command, skipped: c.skipped } : { command: c.command, status: c.status, complete, ...(c.covers ? { covers: c.covers } : {}) };
  }
  return { sha: result.sha, tier, at: new Date().toISOString(), checks, failures: result.failures };
}

/** Attach `note` to its commit and push the lane's ref to origin. The lane is the ref's only writer. A failed push is
 *  said and retried with the next commit's. */
export function publish(note, cwd = ROOT) {
  const ref = NOTE_REFS[note.tier === 2 ? 1 : 0];
  const add = git(['notes', `--ref=${ref}`, 'add', '-f', '-m', JSON.stringify(note), note.sha], cwd);
  if (add.status !== 0) return `git notes add: ${add.stderr.trim()}`;
  const push = git(['push', '-q', 'origin', ref], cwd);
  return push.status === 0 ? null : `git push ${ref}: ${push.stderr.trim()}`;
}

/** The lanes' notes on the first parents of origin/main's merge base with HEAD, newest first: `[{ sha, notes }]`. */
export function mainNotes({ fetch = true, depth = 400, cwd = ROOT } = {}) {
  if (fetch) git(['fetch', '-q', 'origin', '+refs/notes/async*:refs/notes/async*'], cwd);
  const base = git(['merge-base', 'HEAD', 'origin/main'], cwd).stdout.trim();
  if (!base) return [];
  const log = git(['log', '--first-parent', `-n${depth}`, ...NOTE_REFS.map((r) => `--notes=${r}`), '--format=%H%n%N%x1e', base], cwd).stdout;
  const out = [];
  for (const entry of log.split('\x1e')) {
    const [sha, ...lines] = entry.trim().split('\n');
    const notes = lines.flatMap((l) => { try { const n = JSON.parse(l); return n && n.checks ? [n] : []; } catch { return []; } });
    if (sha && notes.length) out.push({ sha, notes });
  }
  return out;
}

/** The lane's check that runs tests of each kind, by whose name `owners` reads a failure's suites. */
const LANE_CHECK = { cargo: 'test', bun: 'glue', xctest: 'ios' };

/** The suites (async.mjs `owners`: `cargo:<package>`, `bun:<file>`, `xctest:ios:<Class>`) each of `local`'s failures of
 *  `kind` belongs to, where `log` says; none for a failure it does not say of. */
export function suitesOf(local, kind, log) {
  const name = LANE_CHECK[kind], named = local.map((f) => `${name}: ${identity(f)}`), own = owners(name, log, named);
  return Object.fromEntries(local.map((f, i) => [f, own[named[i]] ?? []]));
}

/** Each local failure sorted against `history` (`mainNotes`) by the notes that ran its suites (`suitesOf`'s) to the
 *  end: a check of `kind` whole, or the part of one that holds every suite of the failure. A part says nothing of a
 *  failure whose suites are not known, nor of one it left out, so neither does what it kept of that failure from an
 *  earlier run (async.mjs `retainSkippedFailures`). Each row names the note it was sorted against (`sha`); `at` is the
 *  nearest of them. */
export function classify(local, kind, history, suites = {}) {
  const notes = history.flatMap(({ sha, notes }) => notes.map((note) => ({ sha, note })));
  const ran = (note, ids) => Object.values(note.checks).some((c) => !c.skipped && c.complete && kindOf(c.command) === kind
    && (!c.covers || (ids.length > 0 && ids.every((id) => c.covers.includes(id)))));
  let at = null;
  const rows = local.map((failure) => {
    const id = identity(failure), seen = notes.filter(({ note }) => ran(note, suites[failure] ?? []));
    if (!seen.length) return { failure, verdict: 'unknown' };
    const recent = seen.slice(0, RATE_OVER), failedIn = recent.filter(({ note }) => note.failures.some((f) => identity(f) === id)).length;
    if (!at || notes.indexOf(seen[0]) < notes.indexOf(at)) at = seen[0];
    const row = { failure, sha: seen[0].sha };
    return seen[0].note.failures.some((f) => identity(f) === id)
      ? { ...row, verdict: 'known', rate: `${failedIn} of its last ${recent.length} lane runs` }
      : { ...row, verdict: 'new', ...(failedIn ? { rate: `${failedIn} of the last ${recent.length} lane runs` } : {}) };
  });
  return { at: at && { sha: at.sha, when: at.note.at, tier: at.note.tier }, rows };
}

/** One line for the run, then each failure, with the note it was sorted against when that is not `at`. */
export function report({ at, rows }) {
  if (!rows.length) return [];
  const count = (v) => rows.filter((r) => r.verdict === v).length;
  if (!at) return [`${rows.length} failed: unknown on origin/main (no lane note there ran their suites to the end; \`git fetch origin 'refs/notes/async*:refs/notes/async*'\` fetches them)`];
  const lines = [`${rows.length} failed: ${count('known')} known on origin/main at ${at.sha.slice(0, 10)} (lane${at.tier === 2 ? ' tier 2' : ''}, ${at.when?.slice(0, 16).replace('T', ' ') ?? 'undated'}), ${count('new')} new${count('unknown') ? `, ${count('unknown')} unknown` : ''}`];
  for (const r of rows) {
    const said = [r.rate && `failed ${r.rate}`, r.sha && r.sha !== at.sha && `at ${r.sha.slice(0, 10)}`].filter(Boolean);
    lines.push(`  ${r.verdict.padEnd(5)} ${identity(r.failure)}${said.length ? ` (${said.join('; ')})` : ''}`);
  }
  return lines;
}

if (import.meta.main) {
  // Its own flags lead; the command is what follows them, after a `--` or not (Bun keeps no `--` for a script).
  const argv = process.argv.slice(2), own = { '--no-fetch': 0, '--kind': 1, '--log': 1 }, flags = {};
  let i = 0;
  while (i < argv.length && argv[i] in own) { flags[argv[i]] = own[argv[i]] ? argv[i + 1] : true; i += 1 + own[argv[i]]; }
  if (argv[i] === '--') i++;
  const fetch = !flags['--no-fetch'], opt = (name) => flags[name];
  let log, status, kind;
  if (i < argv.length) {
    const [command, ...args] = argv.slice(i);
    kind = opt('--kind') ?? kindOf([command, ...args].join(' '));
    if (!kind) throw new Error(`known: ${command} ${args.join(' ')} is no kind the lane runs (cargo test, bun test, build.mjs --test); pass --kind`);
    ({ log, status } = await new Promise((done) => {
      const child = spawn(command, args, { stdio: ['inherit', 'pipe', 'pipe'] });
      let out = '';
      child.stdout.on('data', (d) => { out += d; process.stdout.write(d); });
      child.stderr.on('data', (d) => { out += d; process.stderr.write(d); });
      child.on('close', (code) => done({ log: out, status: code ?? 1 }));
    }));
  } else if (opt('--log')) {
    log = readFileSync(opt('--log'), 'utf8'); status = 1; kind = opt('--kind');
    if (!['cargo', 'xctest', 'bun'].includes(kind)) throw new Error('known --log <file>: name its --kind (cargo, xctest or bun)');
  } else {
    console.log('Usage: bun scripts/known.mjs [--no-fetch] -- <command> [args…] | --log <file> --kind cargo|xctest|bun');
    process.exit(2);
  }
  const local = status === 0 ? [] : failures('local', log, status);
  for (const line of report(classify(local, kind, mainNotes({ fetch }), suitesOf(local, kind, log)))) console.error(line);
  process.exit(status);
}
