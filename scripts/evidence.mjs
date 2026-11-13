#!/usr/bin/env bun
/**
 * evidence — what has been run against an app's current sources (LLP 1146 D2).
 *
 * Each drive and test run (`agent.mjs`, `verify`) appends a line to `<app>/.exact/runs.jsonl`: when, on which host and
 * target, on which clock, what ran, what it found, the commit, and the build it ran on, named by the digest of that
 * build's record (the web's `dist/.exact-build.json`, an Apple build's `receipt.json`). The driver refuses a build
 * older than its sources, so a run's build was fresh when it ran. A run is current while that build is still the
 * host's build and still fresh: the same checks the driver makes (`webChanges`, `nativeBuild`) say what made it stale.
 * Linux, Windows, Android, tvOS and phone builds are not identified yet; their runs are listed as unchecked.
 *
 *   bun scripts/evidence.mjs [--app <name>] [--since <rev>] [--draft]
 *   bun scripts/evidence.mjs note <host> "<what was seen>" [--app <name>] [--device "<model, OS>"]
 */
import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, mkdirSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { platformCrateKind, resolveApp, webDist as defaultWebDist } from './app.mjs';
import { nativeBuild, receiptPath } from './agent-build.mjs';
import { webChanges } from './agent-launch.mjs';
import { fileURLToPath } from 'node:url';

const ROOT = realpathSync(fileURLToPath(new URL('..', import.meta.url)));
const git = (args, cwd = ROOT) => {
  const run = Bun.spawnSync(['git', ...args], { cwd, stdout: 'pipe', stderr: 'pipe' });
  if (run.exitCode !== 0) throw new Error(`git ${args.join(' ')}: ${run.stderr.toString()}`);
  return run.stdout.toString();
};
const digest = (path) => createHash('sha256').update(readFileSync(path)).digest('hex').slice(0, 16);
const runsOf = (app) => resolve(app.dir, '.exact', 'runs.jsonl');

/** The record of the build a run on `host` uses: its path, or null where builds are not identified. */
function buildRecord(app, host, { webDist, device } = {}) {
  if (device) return null;
  if (host === 'web') { const marker = resolve(webDist ?? defaultWebDist(app), '.exact-build.json'); return existsSync(marker) ? marker : null; }
  if (host === 'macos' || host === 'ios') return receiptPath(host, app) ?? null;
  return null;
}

/** Append one run to the app's record. Never throws: a run's result does not depend on its record. */
export function recordRun(appName, { host, what, result, timing = 'agent', touch, device, sim, target, webDist, kind = 'run' }) {
  try {
    const app = resolveApp(appName), record = buildRecord(app, host, { webDist, device: device || !!target });
    const line = { at: new Date().toISOString(), kind, host, target: target ?? (device ? 'device' : host === 'ios' ? sim ?? process.env.EXACT_SIM ?? 'simulator' : host === 'web' ? process.env.EXACT_WEB_BROWSER ?? 'chrome' : host),
      timing, ...(touch && touch !== 'agent' ? { touch } : {}), what, result, commit: git(['rev-parse', '--short=10', 'HEAD']).trim(), build: record ? digest(record) : null };
    mkdirSync(resolve(app.dir, '.exact'), { recursive: true });
    appendFileSync(runsOf(app), JSON.stringify(line) + '\n');
  } catch { /* the record is advice */ }
}

/** Every recorded run of `app`, oldest first. */
export function runs(app) {
  if (!existsSync(runsOf(app))) return [];
  return readFileSync(runsOf(app), 'utf8').split('\n').filter(Boolean).flatMap((l) => { try { return [JSON.parse(l)]; } catch { return []; } });
}

/** The host's build now: its id, and what changed since it was built ([] when fresh); null where unidentified. */
function buildNow(app, host) {
  if (host === 'web') {
    const dist = defaultWebDist(app), marker = resolve(dist, '.exact-build.json');
    return existsSync(marker) ? { id: digest(marker), changed: webChanges(dist, app).all } : null;
  }
  if (host === 'macos' || host === 'ios') {
    const state = nativeBuild(host, app);
    return state.missing || !state.receipt ? null : { id: digest(state.receipt), changed: state.changed };
  }
  return null;
}

/** Each run with its standing: `current`, `stale` (and why), or `unchecked`. */
export function standing(app, all = runs(app), now = (host) => buildNow(app, host)) {
  const builds = new Map();
  return all.map((r) => {
    if (!r.build) return { ...r, standing: 'unchecked', why: ['web', 'macos', 'ios'].includes(r.host) ? 'a device build is not identified yet' : `a ${r.host} build is not identified yet` };
    if (!builds.has(r.host)) builds.set(r.host, now(r.host));
    const b = builds.get(r.host);
    if (!b) return { ...r, standing: 'stale', why: `no ${r.host} build is there now` };
    if (b.id !== r.build) return { ...r, standing: 'stale', why: 'a later build replaced the one it ran on' };
    if (b.changed.length) return { ...r, standing: 'stale', why: `${b.changed.slice(0, 3).join(', ')}${b.changed.length > 3 ? ` and ${b.changed.length - 3} more` : ''} changed after` };
    return { ...r, standing: 'current' };
  });
}

function cfgMatches(expression, facts) {
  const tokens = expression.match(/\w+|"(?:\\.|[^"\\])*"|[(),=]/g);
  let at = 2; // cfg(
  const predicate = () => {
    const name = tokens[at++];
    if (!name) throw new Error(`invalid cfg expression: ${expression}`);
    if (tokens[at] === '(') {
      at++;
      const args = [];
      while (at < tokens.length && tokens[at] !== ')') {
        args.push(predicate());
        if (tokens[at] === ',') at++;
      }
      if (tokens[at] !== ')') throw new Error(`invalid cfg expression: ${expression}`);
      at++;
      if (name === 'all') return args.every(Boolean);
      if (name === 'any') return args.some(Boolean);
      if (name === 'not') return !args[0];
      throw new Error(`unknown cfg predicate: ${name}`);
    }
    if (tokens[at] !== '=') return facts.has(name);
    at++;
    return facts.has(`${name}=${tokens[at++]}`);
  };
  return predicate();
}

// Follow local dependencies without resolving a lock: evidence is also useful before an app's first build.
function macDependencies(app) {
  const names = new Set();
  if (!app) return names;
  let targets;
  const matches = (target) => {
    const triples = ['aarch64-apple-darwin', 'x86_64-apple-darwin'];
    if (!/^cfg\s*\(/.test(target)) return triples.includes(target);
    targets ??= triples.map(triple => {
      const run = Bun.spawnSync(['rustc', '--print', 'cfg', '--target', triple], { stdout: 'pipe', stderr: 'pipe' });
      if (run.exitCode !== 0) throw new Error(`rustc --print cfg: ${run.stderr.toString()}`);
      return new Set(run.stdout.toString().trim().split('\n'));
    });
    return targets.some(facts => cfgMatches(target, facts));
  };
  const manifests = new Map();
  const read = (dir) => {
    if (!manifests.has(dir)) {
      const path = resolve(dir, 'Cargo.toml');
      manifests.set(dir, existsSync(path) ? Bun.TOML.parse(readFileSync(path, 'utf8')) : {});
    }
    return manifests.get(dir);
  };
  const pending = [resolve(app.dir, platformCrateKind(app.dir, 'macos'))], seen = new Set();
  while (pending.length) {
    const dir = pending.pop();
    if (seen.has(dir)) continue;
    seen.add(dir);
    const manifest = read(dir);
    if (manifest.package) names.add(manifest.package.name);
    const conditional = Object.entries(manifest.target ?? {})
      .filter(([target]) => matches(target)).map(([, table]) => table.dependencies);
    const dependencies = [manifest.dependencies, ...conditional];
    for (const [name, spec] of dependencies.flatMap(table => Object.entries(table ?? {}))) {
      let from = dir, dependency = spec;
      if (spec.workspace) {
        from = resolve(dir, manifest.package?.workspace ?? '.');
        while (!read(from).workspace && dirname(from) !== from) from = dirname(from);
        dependency = read(from).workspace?.dependencies?.[name];
      }
      if (dependency?.path) pending.push(resolve(from, dependency.path));
    }
  }
  return names;
}

/** The hosts whose code a set of changed paths touches: shared layers touch every host. */
export function hostsTouched(paths, app) {
  const all = ['web', 'macos', 'ios', 'tvos', 'linux', 'windows', 'android'], out = new Set();
  const apple = ['macos', 'ios', 'tvos'], linux = ['linux', 'windows', 'android'];
  const painted = app?.manifest?.host?.macos?.presenter === 'painted';
  let mac;
  const macUses = (crate) => painted || (mac ??= macDependencies(app)).has(crate);
  const dir = app && relative(ROOT, resolve(app.dir));
  const own = dir && dir !== '..' && !dir.startsWith(`..${sep}`) && !isAbsolute(dir) ? dir.split(sep).join('/') + '/' : null;
  for (const p of paths) {
    if (/^(docs|llp|issues|rules)\//.test(p) || /^[^/]+\.md$/.test(p)) continue;
    const local = own && p.startsWith(own) ? p.slice(own.length) : null;
    if (/^(apps|examples|bench|game\/(games|bench))\//.test(p) && local === null) continue;
    const directory = local !== null ? local.split('/')[0] : /^host\/([^/]+)/.exec(p)?.[1];
    const platform = local !== null ? directory
      : /^(apple|ios|macos|tvos|watchos|web|linux|windows|android|terminal)(?:-|$)/.exec(directory)?.[1];
    let hosts = all;
    // The executor serves every native host and the web's native renderer.
    // Its process transport is selected by the compilation target.
    if (local === null && directory === 'executor') {
      if (p === 'host/executor/src/process_off.rs') hosts = ['web', 'ios', 'tvos', 'windows', 'android'];
      else if (/^host\/executor\/src\/process(?:_|\.)/.test(p)) hosts = ['web', 'macos', 'linux'];
    } else if (local === null && ['raster', 'svg-raster'].includes(directory)) hosts = [...apple, ...linux];
    else if (local === null && ['linux', 'linux-update'].includes(directory)) hosts = ['linux'];
    else if (local === null && ['painted', 'ink'].includes(directory)) hosts = macUses(`exact-${directory}`) ? ['macos', ...linux] : linux;
    else if (local === null && directory === 'listen') hosts = ['macos', 'linux', 'android'];
    else if (local === null && directory === 'render') hosts = ['web'];
    else if (local === null && directory === 'apple-painted') hosts = ['macos'];
    else if (['apple', 'ios', 'macos', 'tvos', 'watchos'].includes(platform)) hosts = apple;
    else if (platform === 'linux') {
      hosts = local === null ? linux : ['linux'];
      const render = `linux/src/bin/${app?.name}-render.rs`;
      // The page renderer is a binary of the Linux package that only the web build runs.
      if (local === render) hosts = ['web'];
      else if (['linux/build.rs', 'linux/Cargo.toml'].includes(local) && existsSync(resolve(app.dir, render))) {
        hosts = ['web', 'linux'];
      }
    } else if (['web', 'windows', 'android'].includes(platform)) hosts = [platform];
    else if (platform === 'terminal') hosts = [];
    hosts.forEach((h) => out.add(h));
  }
  return all.filter((h) => out.has(h));
}

/** The branch's changed paths and its staged, unstaged and untracked paths. */
export function changedPaths(since, root = ROOT) {
  const paths = new Set(git(['diff', '--name-only', '--no-renames', '-z', `${since}...HEAD`], root).split('\0').filter(Boolean));
  const status = git(['status', '--porcelain', '-z', '--untracked-files=all'], root).split('\0');
  for (let i = 0; i < status.length - 1; i++) {
    const entry = status[i];
    paths.add(entry.slice(3));
    // A rename or copy has a second NUL-delimited path without a status prefix.
    if (/[RC]/.test(entry.slice(0, 2))) paths.add(status[++i]);
  }
  return [...paths];
}

const said = (r) => {
  const t = r.result;
  const found = r.kind === 'note' ? r.what : typeof t === 'string' ? `${r.what}: ${t}` : t?.tests ? `${r.what}: ${t.passed} passed, ${t.failed} failed${t.skipped ? `, ${t.skipped} skipped` : ''}` : `${r.what}: ${t?.ok ? 'ok' : `failed${t?.error ? ` (${t.error})` : ''}`}`;
  return `${r.host}${r.target && r.target !== r.host ? `, ${r.target}` : ''}: ${found}${r.kind === 'note' ? ' (seen by hand)' : ` (${r.timing} clock${r.touch ? `, ${r.touch} touch` : ''})`}`;
};

if (import.meta.main) {
  const argv = process.argv.slice(2), opt = (name) => { const i = argv.indexOf(name); return i < 0 ? undefined : argv[i + 1]; };
  const app = resolveApp(opt('--app'));
  if (argv[0] === 'note') {
    const [, host, what] = argv;
    if (!host || !what) throw new Error('evidence note <host> "<what was seen>" [--device "<model, OS>"]');
    // Seen on a named device, the note stands for that device, whose build is not identified; else for the host's build now.
    recordRun(app.selector, { host, what, result: null, kind: 'note', target: opt('--device'), timing: 'real' });
    console.log(`noted on ${host}: ${what}`);
    process.exit(0);
  }
  const since = opt('--since') ?? 'origin/main';
  const changed = changedPaths(since);
  const all = standing(app);
  // The latest run of each kind of thing on each host and target is what stands; older ones repeat it.
  const latest = [...new Map(all.map((r) => [`${r.host}\n${r.target}\n${r.kind}\n${r.what}`, r])).values()];
  const lines = [];
  for (const r of latest) lines.push(`${said(r)}${r.standing === 'current' ? ', current' : `; ${r.standing}${r.why ? ` (${r.why})` : ''}`}`);
  const covered = new Set(latest.filter((r) => r.standing === 'current').map((r) => r.host));
  const owed = hostsTouched(changed, app).filter((h) => !covered.has(h));
  if (argv.includes('--draft')) {
    for (const l of lines.filter((l) => l.endsWith(', current'))) console.log(`- ${l.slice(0, -', current'.length)}`);
    for (const h of owed) console.log(`- ${h}: not run against these sources`);
  } else {
    console.log(`${app.name}: ${latest.length ? `${latest.length} run${latest.length === 1 ? '' : 's'} recorded` : 'no run recorded'} (${runsOf(app)})`);
    for (const l of lines) console.log(`  ${l}`);
    if (owed.length) console.log(`changed since ${since}, and no current run: ${owed.join(', ')}`);
  }
}
