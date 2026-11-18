#!/usr/bin/env bun
/**
 * Hourly verification of origin/main's newest tip, over every commit since the last run.
 * Full workspace build, lint and check always run; affected.mjs selects tests, app drives,
 * XCTests and Apple builds. Every suite runs at least daily. New failures are bisected
 * independently through that range, rerunning only their checks and prerequisite builds.
 * An obstructed probe reports its range without assigning blame. Logs, timings and
 * attribution stay in <worktree>/target/async; known.mjs reads the published git notes.
 *
 *   bun scripts/async.mjs [--once] [--from <rev>] [--full]
 *   bun scripts/async.mjs --tier 2       hourly cross-platform builds in a separate worktree
 *   bun scripts/async.mjs --worktree <dir> --interval <seconds> --branch <ref>
 *   bun scripts/async.mjs --explain [<rev>]   explain semantics fingerprints
 *
 * Both tiers coalesce commits once an hour in their own worktree and target directory.
 */
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, openSync, closeSync, readFileSync, writeFileSync, writeSync } from 'node:fs';
import { availableParallelism, loadavg } from 'node:os';
import { basename, resolve } from 'node:path';
import { Graph, LANE_DISTS, PARITY, Repo, WEB_APPS, commands, gameAppleLeg, parityLeg, select, stepEnv, suites } from './affected.mjs';
import { INPUTS, Inputs, Store, checkEnv, decide, outcomeOf } from './async-inputs.mjs';
import { main as issue } from './issue.mjs';
import { noteOf, publish, stoppedEarly } from './known.mjs';
import { bisectAffected, newFailures, probeOutcomes } from './async-bisect.mjs';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const option = (name, fallback) => process.argv.includes(name) ? process.argv[process.argv.indexOf(name) + 1] : fallback;
const TIER = Number(option('--tier', 1));
if (TIER !== 1 && TIER !== 2) throw new Error(`--tier ${TIER}: the lanes are 1 and 2`);
const WT = resolve(option('--worktree', resolve(ROOT, '..', `${basename(ROOT)}-async${TIER === 2 ? '2' : ''}`)));
const STATE_DIR = resolve(WT, 'target/async');
const STATE = resolve(STATE_DIR, 'state.json');
const BRANCH = option('--branch', 'origin/main');
const git = (args, cwd = ROOT) => {
  const r = spawnSync('git', args, { cwd, encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr.trim()}`);
  return r.stdout.trim();
};

function refuseTrackedEdits(root) {
  const dirty = git(['status', '--porcelain', '--untracked-files=no'], root);
  if (dirty) throw new Error(`${root} has tracked edits; refusing to switch commits. Review them first:\n${dirty}`);
}
export function checkout(sha, root = WT) {
  restoreLaneEdits(root);
  refuseTrackedEdits(root);
  git(['checkout', '--detach', sha], root);
}

function restoreLaneEdits(root) {
  const paths = git(['ls-files', '-z', '--', 'game/app/shells.lock', 'apps/caltrain/app.contract',
    'apps/realworld/app.contract', 'apps/realworld/app.ts'], root).split('\0').filter(Boolean);
  if (paths.length) git(['restore', '--source=HEAD', '--staged', '--worktree', '--', ...paths], root);
}

/** Restore lane-owned inputs after a check; checkout also repairs an interrupted parent. */
export async function withLaneEdits(root, action) {
  refuseTrackedEdits(root);
  try { return await action(); }
  finally { restoreLaneEdits(root); }
}

/** Test names marked for this lane; libtest ORs several filters. */
function laneTests() {
  const r = spawnSync('git', ['grep', '-n', '-A3', '#\\[ignore = "async lane', '--', '*.rs'], { cwd: WT, encoding: 'utf8' });
  return [...new Set([...(r.stdout ?? '').matchAll(/fn (\w+)\s*\(/g)].map(m => m[1]))];
}

const workspace = ['--workspace'];
// The second lane: tvOS uses Caltrain (its TypeScript-free build needs no
// Hermes) and installs it on an Apple TV simulator, which the first lane's
// compile does not; Linux and Windows compile their target-only branches:
// Windows with Windows Desk, on x64 and ARM64 (LLP 1115.004 D0.3), Linux with every app (`*-linux` is the
// host and each `apps/<app>/linux`), Zig compiling Hermes's C++ bindings too. Android
// builds Weatherlight's APK, which compiles the Android target's Rust, the
// reader's Kotlin and a module's (LLP 1133.002 stage 4, Charlie's say-so).
export function tierTwoChecks() {
  return [
    ['tvos', 'env', ['EXACT_JS_ENGINE=stub', 'bun', 'host/apple/build.mjs', '--tvos']],
    ['linux', 'env', ['CRATE_CC_NO_DEFAULTS=1', 'CC_x86_64_unknown_linux_gnu=zig cc -target x86_64-linux-gnu', 'CXX_x86_64_unknown_linux_gnu=zig c++ -target x86_64-linux-gnu', 'cargo', 'check', '--locked', '--keep-going', '--target', 'x86_64-unknown-linux-gnu', '-p', '*-linux']],
    ['windows', 'env', ['CRATE_CC_NO_DEFAULTS=1', 'CC_x86_64_pc_windows_gnu=zig cc -target x86_64-windows-gnu', 'cargo', 'check', '--locked', '--keep-going', '--target', 'x86_64-pc-windows-gnu', '-p', 'exact-windows', '-p', 'windows-desk']],
    // Rust has no GNU MinGW target for ARM64; LLVM's (`gnullvm`) stands in for MSVC here as GNU's does for x64.
    ['windows-arm64', 'env', ['CRATE_CC_NO_DEFAULTS=1', 'CC_aarch64_pc_windows_gnullvm=zig cc -target aarch64-windows-gnu', 'cargo', 'check', '--locked', '--keep-going', '--target', 'aarch64-pc-windows-gnullvm', '-p', 'exact-windows', '-p', 'windows-desk']],
    ['android', 'bun', ['scripts/agent-android.mjs', 'build', 'weatherlight', '--apk']],
  ];
}
export const tvosCompile = ['tvos', 'env', ['EXACT_JS_ENGINE=stub', 'bun', 'host/apple/build.mjs', '--tvos', '--no-install']];
export const watchosCompile = ['watchos', 'bun', ['host/apple/build.mjs', '--watchos', '--no-install']];
export const gameScaffoldCheck = ['game-scaffold', 'bun', ['test', 'game/proof-game.test.mjs', 'game/new.test.mjs', '--test-name-pattern', 'proof script|worldPixels|AGENTS.md lists vocab|new logic/tests file']];
export const gameCheck = ['game', 'cargo', ['test', '--manifest-path', 'game/Cargo.toml', '--workspace', '--locked', '--lib', '--bins', '--tests', '--no-fail-fast']];
export const gameAppleCheck = ['game-apple', 'sh', ['-c', gameAppleLeg]];
// Glue files that edit an app's checked-in sources while they run
const EDITS_APPS = /\/dev-scheduler\.test\.mjs$/;
/** The glue step's worker processes: half the cores the machine's load leaves idle, at least one. Each worker's file
 * drives a browser or a build of its own, so two idle cores a worker keep them from contending (18 cores at load 3:
 * seven workers; at load 15: one) */
export function glueWorkers(cores = availableParallelism(), load = loadavg()[0]) {
  return Math.max(1, Math.floor((cores - load) / 2));
}
// `bundle-size` reads RealWorld's production entry (`EXACT_REALWORLD_DIST`), which no step prepares before the glue.
const LEFT_OUT = /^host\/web-js\/bundle-size\.test\.mjs$/;
/** The glue step's files: every test file of the web hosts, of the modules and of `scripts/`, so a new one runs without
 * being named, less `LEFT_OUT`'s. */
export function glueTests(root) {
  const glob = pattern => [...new Bun.Glob(pattern).scanSync({ cwd: root, onlyFiles: true })];
  const files = [...glob('host/web/**/*.test.{mjs,js}'), ...glob('host/web-js/**/*.test.{mjs,js}'), ...glob('modules/*/tests/*.test.mjs'),
    ...glob('scripts/*.test.mjs')].filter(file => !LEFT_OUT.test(file)).sort().map(file => `./${file}`);
  // The glue lane is macOS: link.test reads Mach-O load commands with Apple tools; xctest-fixtures compiles the
  // XCTests' fixtures with the contract CLI into a directory outside the checkout.
  return [...files, './host/apple/link.test.mjs', './host/apple/cloak.test.mjs', './host/apple/xctest-fixtures.test.mjs'];
}
/**
 * What a commit's checks run (scripts/affected.mjs): the suites whose inputs the files changed since the last commit
 * checked touch, or every suite on a full run. A suite of a step given a dist by variable (`LANE_DISTS`) reads that
 * build when it names the variable. A check whose suites are all left out does not run, and a failure nothing ran to
 * find again is kept (`retainSkippedFailures`). `repo` (tests) stands in for this checkout's.
 */
export function laneSelection(changed, { root = WT, full = false, repo, graphs = new Map() } = {}) {
  if (full) {
    const all = everything(root);
    repo ??= new Repo(root);
    return { ...all, defaultCargo: repo.defaultMembers, extra: extraChecks(suites(repo).map(suite => ({ suite })), all.glue, repo) };
  }
  repo ??= new Repo(root, { also: changed });
  const glue = glueTests(root), dists = new Set([...glue, ...WEB_AX]);
  const graph = step => graphs.get(step) ?? graphs.set(step, new Graph(repo, { env: step ? stepEnv(step) : {} })).get(step);
  const step = suite => suite.kind === 'cargo' ? 'test' : suite.kind === 'bun' && dists.has(`./${suite.file}`) ? 'glue' : null;
  const hits = suites(repo).flatMap(suite => select(graph(step(suite)), changed, [suite]));
  const chosen = hits.map(hit => hit.suite), ids = new Set(chosen.map(s => s.id));
  const ios = hits.find(hit => hit.suite.id === 'xctest:ios'), build = resolve(root, 'host/apple/build.mjs');
  // Run alone the iOS classes a commit's test files reach, unless its build.mjs predates `--only` (#1371)
  const iosOnly = ios?.only && existsSync(build) && readFileSync(build, 'utf8').includes("'--only'") ? ios.only : null;
  const cargo = manifest => chosen.filter(s => s.kind === 'cargo' && s.manifest === manifest).map(s => s.name);
  return {
    full, ids: [...ids].sort(), defaultCargo: repo.defaultMembers,
    cargo: cargo('Cargo.toml'),
    game: cargo('game/Cargo.toml').length > 0 || chosen.some(s => s.kind === 'bun' && s.file.startsWith('game/')),
    glue: glue.filter(file => ids.has(`bun:${file.slice(2)}`)),
    webBuild: ids.has('bun:host/web/tests/document.test.mjs'),
    webAx: WEB_AX.some(file => ids.has(`bun:${file.slice(2)}`)),
    conform: WEB_APPS.filter(app => ids.has(`conform:${app}`)),
    parity: PARITY.filter(smoke => ids.has(`parity:${smoke}`)),
    moduleSeams: ids.has('drive:module-seams'), notificationsWeb: ids.has('drive:notifications-web'),
    extra: extraChecks(hits, glue, repo),
    ios: ids.has('xctest:ios'), iosOnly, touch: ids.has('touch:ios'), tvos: ids.has('compile:tvos'), watchos: ids.has('compile:watchos'),
    gameApple: ids.has('compile:game-apple'),
  };
}
const WEB_AX = ['./host/web/tests/accessibility-tree.test.mjs', './host/web/tests/presence-preload.test.mjs', './host/web/tests/zero-viewport.test.mjs'];
/** Suites outside the established steps still run: other Cargo workspaces, Bun files, macOS XCTests and app tests. */
export function extraChecks(hits, glue, repo) {
  const covered = new Set([...glue, ...WEB_AX, './host/web/tests/document.test.mjs', './game/vocab.test.mjs']);
  const bun = hits.filter(({ suite: s }) => s.kind === 'bun' && !covered.has(`./${s.file}`));
  const out = [], fixture = bun.some(({ suite }) => LEFT_OUT.test(suite.file));
  const realworld = resolve(STATE_DIR, 'fixtures/realworld-production');
  if (fixture) out.push(['bundle-fixture', 'bun', ['host/web-js/build.mjs', 'realworld', '--production', '--out', realworld]]);
  for (const cmd of commands(bun)) out.push(['other-glue', 'env', ['EXACT_REQUIRED=1',
    ...(fixture ? [`EXACT_REALWORLD_DIST=${realworld}`] : []), ...cmd]]);
  for (const hit of hits) {
    const s = hit.suite;
    if (s.kind === 'cargo' && s.manifest !== 'Cargo.toml' && s.manifest !== 'game/Cargo.toml') {
      const [cmd, ...args] = commands([hit])[0];
      out.push([s.id, cmd, args]);
    }
    if (s.kind !== 'app' && !(s.kind === 'xctest' && s.host === 'macos')) continue;
    const named = s.kind === 'app' ? `app:${s.app}:${s.host}` : 'macos';
    const first = out.length;
    if (s.kind === 'app') {
      if (s.host === 'web') out.push([`build-${named}`, 'bun', ['host/web/build.mjs', s.app]]);
      else if (s.host === 'macos' || s.host === 'ios') out.push([`build-${named}`, 'bun', ['host/apple/build.mjs', ...(s.host === 'ios' ? ['--ios'] : []), s.app]]);
      else if (s.host === 'android') out.push([`build-${named}`, 'bun', ['scripts/agent-android.mjs', 'build', s.app]]);
      else if (s.host === 'windows') out.push([`build-${named}`, 'bun', ['host/windows/build.mjs', s.app]]);
      else out.push([`build-${named}`, 'cargo', ['build', '--profile', 'host-dev', '-p', repo.packageAt.get(`${repo.appNames.get(s.app)}/${s.host}`).name]]);
    }
    for (const [i, [cmd, ...args]] of commands([hit]).entries()) out.push([`${named}${s.kind === 'app' ? `:${i}` : ''}`, cmd, args]);
    if (s.kind === 'app' && s.host === 'windows') for (const step of out.slice(first)) step[3] = { platform: 'win32' };
  }
  return out;
}

/** Every suite (a full run), as `laneSelection` gives it; `glue` is computed when it is not given. */
export function everything(root = WT, glue) {
  return { full: true, ids: [], cargo: [], game: true, glue: glue ?? glueTests(root), extra: [], webBuild: true, webAx: true,
    conform: WEB_APPS, parity: PARITY, moduleSeams: true, notificationsWeb: true,
    ios: true, iosOnly: null, touch: true, tvos: true, watchos: true, gameApple: true };
}
/** No suite: what runs whatever changed. */
export const nothing = () => ({ full: false, ids: [], cargo: [], game: false, glue: [], webBuild: false, webAx: false,
  conform: [], parity: [], moduleSeams: false, notificationsWeb: false,
  ios: false, iosOnly: null, touch: false, tvos: false, watchos: false, gameApple: false, lane: [], extra: [] });
/** The checks a selection can leave out: a full run's that no suite's selection runs. */
export const gated = () => {
  const names = selection => new Set(checks('', null, selection).map(([name]) => name));
  const always = names(nothing());
  return new Set([...names({ ...everything(WT, ['./x.test.mjs']), lane: ['x'] })].filter(name => !always.has(name)));
};
/** The suites a check that runs only some of its suites runs (`laneSelection`'s ids), or the iOS XCTest classes
 * (`xctest:ios:<Class>`); null when it runs whole. */
export function covers(name, selection) {
  if (name === 'default-test') return (selection?.defaultCargo ?? []).map(n => `cargo:${n}`);
  if (!selection || selection.full) return null;
  if (name === 'test' || name === 'lane') return selection.cargo.map(n => `cargo:${n}`);
  if (name === 'ios') return selection.iosOnly?.map(c => `xctest:ios:${c}`) ?? null;
  if (name === 'macos') {
    const args = selection.extra?.find(([n]) => n === name)?.[2], at = args?.indexOf('--only') ?? -1;
    return at >= 0 ? args[at + 1].split(',').map(c => `xctest:macos:${c}`) : null;
  }
  if (name === 'other-glue') {
    const args = selection.extra?.find(([n]) => n === name)?.[2];
    return args?.filter(a => a.startsWith('./')).map(a => `bun:${a.slice(2)}`) ?? null;
  }
  if (name === 'glue') return selection.glue.map(f => `bun:${f.slice(2)}`);
  if (name.startsWith('conform')) return selection.conform.map(a => `conform:${a}`);
  return null;
}

/** The checks of `sha`, given what `selection` (`laneSelection`'s; a full run where it says nothing) runs. */
export function checks(sha, since, selection = {}) {
  if (TIER === 2) return tierTwoChecks();
  selection = { ...everything(WT, selection.glue), ...selection };
  const glue = selection.glue;
  const rust = selection.full || selection.cargo.length > 0;
  const lane = rust ? selection.lane ?? laneTests() : [];
  // The workspace's tests: all of them on a full run, else the selected packages', built as a full run builds them by
  // the lane's own runner (a commit checked may predate it; it takes the workspace from its working directory).
  const cargoTests = (...args) => selection.full ? ['cargo', 'test', ...workspace, '--tests', '--no-fail-fast', ...args]
    : ['bun', resolve(ROOT, 'scripts/affected.mjs'), '--cargo-tests', ...selection.cargo, ...args];
  const conform = selection.conform;
  const when = (condition, steps) => condition ? steps : [];
  return [
    ['build', 'cargo', ['build', '--all-targets', '--keep-going']],
    // Workspace feature unification differs from the default-member graph. Qualify both hourly.
    ['workspace-build', 'cargo', ['build', ...workspace, '--all-targets', '--keep-going']],
    ['workspace', 'cargo', ['check', ...workspace, '--all-targets', '--keep-going']],
    // This commit's web builds for the tests that drive one (below): Caltrain wasm with every
    // capability linked (the document, placement, motion, focusout and no-GPU tests) and JS,
    // and Fieldnotes as the dev loop builds it (its Rust drive, in `test`).
    prepareCheck,
    ['default-test', 'env', ['EXACT_REQUIRED=1', ...distsOf('test'), 'cargo', 'test', '--tests', '--no-fail-fast']],
    ...when(rust, [['test', 'env', ['EXACT_REQUIRED=1', ...distsOf('test'), ...cargoTests()]]]),
    ...when(selection.game, [
      ['game-lock', 'bun', ['game/app/shells.mjs', '--update-lock']],
      gameCheck,
      ['game-recipes', 'cargo', ['test', '--manifest-path', 'game/Cargo.toml', '-p', 'exact-game', '--locked', '--doc', '--no-fail-fast']],
      ['game-vocab', 'bun', ['test', 'game/vocab.test.mjs']],
      ['game-pins', 'bun', ['game/app/shells.mjs', '--test']],
      // The scaffold's standing checks: seconds, then the one that builds a game's tests twice.
      gameScaffoldCheck,
      ['game-assets-scaffold', 'bun', ['test', 'game/new.test.mjs', '--timeout', '3600000', '--test-name-pattern', 'assets scaffold']],
    ]),
    ...when(selection.gameApple, [gameAppleCheck]),
    ...(lane.length ? [(([command, ...args]) => ['lane', command, args])(cargoTests('--', '--ignored', ...lane))] : []),
    ['clippy', 'cargo', ['clippy', '--all-targets', '--keep-going', '--', '-D', 'warnings']],
    ['workspace-clippy', 'cargo', ['clippy', ...workspace, '--all-targets', '--keep-going', '--', '-D', 'warnings']],
    ['fmt', 'cargo', ['fmt', '--all', '--', '--check']],
    ['caps', 'bun', ['scripts/caps.mjs']],
    ['boot', 'bun', ['scripts/boot.mjs']],
    ['wasm', 'bun', ['scripts/wasm-check.mjs']],
    // Shared runtime and app byte ceilings fail this step (LLP 1172.011 E1), before metrics --long's native builds.
    ['web-bytes', 'bun', ['scripts/metrics.mjs', '--web-bytes']],
    // Every web host unit test (LLP 1012.001.000 D9; Charlie, 2026-09-30)
    // and the source capture fixtures, in the existing async glue step.
    // Modules' JS tests (`modules/*/tests`) run here too.
    // Run the files in as many worker processes as the machine's idle cores allow when the step starts (`glueWorkers`)
    // A file that edits an app's sources runs after them, alone, since a page another file opens meanwhile would refuse
    // that app's build as stale
    ...when(glue.length, [['glue', 'env', ['EXACT_GLUE_FAST=1', ...required, ...distEnv, 'sh', '-c', [
      'failed=0',
      `workers=$(bun -e "import { glueWorkers } from './scripts/async.mjs'; console.log(glueWorkers())") || { echo 'error: glue: scripts/async.mjs failed to count the idle cores (above); fix it and rerun the step'; exit 1; }`,
      ...when(glue.some(file => !EDITS_APPS.test(file)), [`bun test --parallel="$workers" ${glue.filter(file => !EDITS_APPS.test(file)).join(' ')} || failed=1`]),
      ...glue.filter(file => EDITS_APPS.test(file)).map(file => `bun test ${file} || failed=1`),
      'exit $failed',
    ].join('\n')]]]),
    // This document proof builds Weatherlight's wasm target. Keep it out of
    // glue's seconds loop while retaining it in the asynchronous lane.
    ...when(selection.webBuild, [['web-build-test', 'env', [...required, 'bun', 'test', './host/web/tests/document.test.mjs', '--test-name-pattern', "a TypeScript app's served document"]]]),
    // The web build's JS target against the wasm runner, step by step (LLP
    // 1071 §4; Charlie, 2026-09-28): minutes and a network, so never blocking.
    ...when(conform.length, [['conform', 'bun', ['host/web-js/conform.mjs', ...conform, '--synthetic', '--build', '--linux', '--strict', '--wasm-root', resolve(STATE_DIR, 'conform-wasm'), '--out', resolve(STATE_DIR, 'conform')]]]),
    // These drives build their own external fixtures and launch the pinned Chromium (notifications needs its full
    // build). Missing build tools or browsers fail the step; neither drive skips an unavailable prerequisite.
    ...when(selection.moduleSeams, [['module-seams', 'env', [...required, 'bun', 'contract/cli/tests/bell/seams.mjs']]]),
    ...when(selection.notificationsWeb, [['notifications-web', 'env', [...required, 'bun', 'packages/notifications/test/drive.mjs', 'web']]]),
    // `tree --ax` against Chrome's own tree (LLP 1080.002 §3), and the wasm
    // host's presence preload and zero-viewport recovery, over this commit's
    // Caltrain dists; a missing dist fails it.
    ...when(selection.webAx, [['web-ax-test', 'env', [...required, ...distEnv, 'bun', 'test', ...WEB_AX]]]),
    // The JS target in both other engines, sharing one live Chrome oracle per target.
    // These remain async-only; a missing Playwright browser is a named failure
    // whose log gives the exact outside-the-repo install command.
    ...when(conform.length, [['conform-browsers', 'bun', ['host/web-js/conform.mjs', ...conform, '--synthetic', '--browser', 'firefox,webkit', '--strict', '--wasm-root', resolve(STATE_DIR, 'conform-wasm'), '--js-from', resolve(STATE_DIR, 'conform'), '--out', resolve(STATE_DIR, 'conform-browsers')]]]),
    // The paint walk's pixels against Chrome's (Charlie, 2026-11-11; LLP 1159.000 D6): each gallery on the Linux
    // host, whose walk Windows and Android paint with too. Seconds once the gallery is built; no window, no simulator.
    ...selection.parity.map(smoke => [`parity-${smoke}`, 'sh', ['-c', parityLeg(smoke)]]),
    ...when(selection.ios, [['ios', 'bun', ['host/apple/build.mjs', '--test', '--ios', ...(selection.iosOnly ? ['--only', selection.iosOnly.join(',')] : [])]]]),
    // Real touches through the XCTest runner on a simulator (LLP 1080.000 §6):
    // a runner that does not start fails here, never skips.
    ...when(selection.touch, [['ios-touch', 'bun', ['scripts/smoke-touch.mjs', '--build']]]),
    // tvOS compiles the UIKit presenter's Swift a second time, and an iOS-only
    // name in shared code breaks it (twice on 2026-10-11, seen by hand). Built
    // and signed, installed on nothing (Charlie, 2026-10-12): it needs the tvOS
    // SDK and Rust's tvOS simulator target, and no simulator. Tier 2 installs.
    ...when(selection.tvos, [tvosCompile]),
    // watchOS compiles ExactCore and the snapshot driver's Swift a third time, and a
    // name watchOS lacks breaks it (CACurrentMediaTime in HostPhases, 2026-10-26). It
    // builds Caltrain's watch bundle with the pinned nightly and rust-src
    // (webToolchainEnv names their install), and no simulator.
    ...when(selection.watchos, [watchosCompile]),
    ...(selection.extra ?? []),
    ...tierTwoChecks().map(([name, ...command]) => [name === 'tvos' ? 'tvos-install' : name, ...command]),
    ['difftest-quick', 'cargo', ['run', '-q', '-p', 'contract-difftest', '--', 'quick', '--base', since ?? `${sha}^`]],
    ...semanticsChecks(),
    ['metrics', 'bun', ['scripts/metrics.mjs', '--long']],
  ];
}

// List a rename as both its paths, since what read the old path finds nothing there now
export const changedPaths = (sha, since, diff) => diff(['diff', '-z', '--name-only', '--no-renames', since ?? `${sha}^`, sha], WT).split('\0').filter(Boolean);

// The lane owns the coverage of its checks (scripts/coverage.mjs): a missing
// prerequisite (a browser, a toolchain, a built fixture) fails a test instead
// of skipping it.
const required = ['EXACT_REQUIRED=1'];
const DIST_DIR = resolve(STATE_DIR, 'dists');
// The dists are made read-only once built, and the directory that holds them too. Every step that reads one shares
// it, so a test that writes over one (a build or dev loop given the lane's EXACT_WEB_DIST) fails itself, by name, at
// the build's lock beside the dist, instead of every test that reads the dist after it (2026-11-13: a JS build over
// caltrain-wasm mid-glue failed some 30 tests as a "stale receipt"). A drive's lease beside a dist
// (host/web/generations.mjs) is a reader's write, so each dist's `.leases` directory stays writable.
const DISTS = ['caltrain-wasm', 'caltrain-js', 'fieldnotes'];
export const prepareCheck = ['prepare', 'sh', ['-c', [
  `if [ -d ${JSON.stringify(DIST_DIR)} ]; then chmod -R u+w ${JSON.stringify(DIST_DIR)}; fi`,
  `rm -rf ${JSON.stringify(DIST_DIR)} && mkdir -p ${JSON.stringify(DIST_DIR)}`,
  `EXACT_WEB_LINK=all EXACT_WEB_DIST=${JSON.stringify(resolve(DIST_DIR, 'caltrain-wasm'))} bun host/web/build.mjs caltrain-web --wasm`,
  `EXACT_WEB_DIST=${JSON.stringify(resolve(DIST_DIR, 'caltrain-js'))} bun host/web/build.mjs caltrain-web`,
  // apps/fieldnotes/web/tests/worker.rs drives a wasm build (its app.module.json and bake.json).
  `EXACT_WEB_LINK=all EXACT_WEB_DIST=${JSON.stringify(resolve(DIST_DIR, 'fieldnotes'))} bun host/web/build.mjs fieldnotes-web --wasm`,
  // The bounded feed's data, which host/web-js/lower-equal.test.mjs requires (LLP 1071.001 part B, B5).
  'bench/dioxus/prepare.sh',
  `chmod -R a-w ${DISTS.map(d => JSON.stringify(resolve(DIST_DIR, d))).join(' ')}`,
  `mkdir -p ${DISTS.map(d => JSON.stringify(resolve(DIST_DIR, `${d}.leases`))).join(' ')}`,
  `chmod a-w ${JSON.stringify(DIST_DIR)}`,
].join(' && ')]];
/** The dists a step reads, as its variables (`LANE_DISTS`). */
const distsOf = step => LANE_DISTS.filter(d => d.steps.includes(step)).map(d => `${d.name}=${resolve(DIST_DIR, d.dist)}`);
const distEnv = distsOf('glue');

/** Settings a check inherits from the lane's own environment that change what it tests. */
export const ENV_DROPPED = ['EXACT_UPDATE_TRUST', 'CARGO_TARGET_DIR', 'EXACT_WEB_BROWSER', 'EXACT_JS_ENGINE', 'EXACT_JS_GPU_SURFACES',
  'EXACT_JS_UNREGISTERED_SURFACES', 'EXACT_WEB_LINK', 'EXACT_WEB_DIST', 'EXACT_WEB_JS_DIST', 'EXACT_NO_GPU_WASM_DIST', 'EXACT_NO_GPU_JS_DIST',
  'EXACT_NO_GPU_CHROME', 'EXACT_MOTION_DIST', 'EXACT_GLUE_FAST', 'EXACT_REQUIRED', 'EXACT_BACKEND_ORIGIN'];
export const laneEnv = base => {
  const env = { ...base, HERMES_LEAN_SYS_OFFLINE: '1' };
  for (const k of Object.keys(env)) if (ENV_DROPPED.includes(k) || k.startsWith('EXACT_AGENT')) delete env[k];
  return env;
};


// The Contract semantics (semantics/README.md; Charlie, 2026-10-03), four
// checks, each with its inputs (scripts/async-inputs.mjs). A `sorry` fails
// `lean`: a proof that is not there is not checked. The sweeps' seeds are
// fixed, so a divergence is attributed to the commit that made it. Every part
// of a check runs whatever the one before it found.
const difftest = args => `cargo run -q -p contract-difftest -- ${args} || failed=1`;
const script = lines => ['sh', ['-c', ['failed=0', ...lines, 'exit $failed'].join('\n')]];
/** The semantics' environment (lake on PATH), with `extra`. */
const semanticsEnv = (base, extra) => checkEnv({ ...base, PATH: `${base.HOME}/.elan/bin:${base.PATH}` }, extra);
export function semanticsChecks(base = process.env) {
  const env = semanticsEnv(base);
  return [
    ['lean', ...script([
      'out=$(cd semantics && lake build 2>&1) || failed=1; echo "$out"',
      'if echo "$out" | grep -q "declaration uses .sorry."; then echo "error: semantics: a proof uses sorry"; failed=1; fi',
    ]), { env, inputs: INPUTS.lean }],
    // Exits 77 (UNAVAILABLE: not a pass, not a failure) where Charon or Aeneas is missing.
    ['vm-extract', 'sh', ['semantics/vm-extract/check.sh'], { env, inputs: INPUTS['vm-extract'] }],
    ['difftest', ...script([
      'apps', 'corpus', 'explore contract/corpus apps/*/app.contract', 'random --seed 1 --count 5000', 'lowering-corpus',
      'lowering --seed 1 --count 300', 'numbers --count 200000', 'types --seed 1 --count 100',
      'expansion semantics/corpus --seed 1 --count 200',
    ].map(difftest)), { env: semanticsEnv(base, { DIFFTEST_CACHE: '1' }), inputs: INPUTS.difftest }],
    ['difftest-js', ...script([
      'corpus --js-only', 'explore contract/corpus apps/*/app.contract --js-only', 'random --seed 1 --count 500 --js-only',
    ].map(difftest)), { env, inputs: INPUTS['difftest-js'] }],
  ];
}

/** Replay the full corpus and logged seed; quick would skip a clean baseline. */
export function quickProbe(log, base = process.env) {
  const seed = /quick: random --seed (\d+)/.exec(log)?.[1];
  return ['difftest-quick', ...script(['corpus', ...(seed ? [
    `random --seed ${seed} --count 40`, `lowering --seed ${seed} --count 8 --batch 2`,
  ] : [])].map(difftest)), { env: semanticsEnv(base, { DIFFTEST_CACHE: '1' }) }];
}

/** What the semantics checks read at the worktree's checkout, read once for all four. */
const semanticsInputs = () => new Inputs(WT, semanticsEnv(process.env));

/**
 * Keep each of `previous` (with `owners`, the suites each belongs to) that this run did not find again and nothing ran
 * to: its check (one a selection can leave out, `gated`) did not run, stopped early (not `complete`), or ran without
 * the part that holds it. A check left out, or run without that part, read nothing that changed, so the failure
 * stands; one that stopped early says nothing of it. A full run keeps none.
 */
export function retainSkippedFailures(previous, result, owners = {}, steps = TIER === 1 ? gated() : new Set()) {
  if (result.full) return;
  const found = new Set(result.failures);
  for (const f of previous) {
    const name = f.split(': ', 1)[0], check = result.checks[name], owner = owners[f];
    if (found.has(f) || !(steps.has(name) || name === 'other-glue' || name === 'bundle-fixture' || name === 'macos' || name.startsWith('cargo:') || name.startsWith('app:') || name.startsWith('build-app:'))) continue;
    // What ran it again: its check whole, or the part of it that holds its suites (one it cannot name: any part).
    const ran = check && !check.skipped && check.complete !== false && (!check.covers || (owner?.length && owner.every(o => check.covers.includes(o))));
    if (ran) continue;
    result.failures.push(f);
    found.add(f);
    if (owner) (result.owners ??= {})[f] = owner;
  }
}

/** The suites each failure a check found belongs to, where its log says (scripts/affected.mjs's ids): a Cargo package's
 * (its rerun hint or the crate that did not compile), a glue file's (the file Bun names before its results; one name
 * can be several files'), a conformance app's (after the browser, in a cross-browser step), an iOS XCTest class's
 * (`xctest:ios:<Class>`, which `covers` names). */
export function owners(name, log, found) {
  const out = {};
  const own = (f, id) => { if (!(out[f] ??= []).includes(id)) out[f].push(id); };
  for (const f of found) {
    if (name.startsWith('cargo:')) own(f, name);
    const pkg = /^\S+: -p (\S+) /.exec(f)?.[1] ?? /^\S+: (\S+) (?:\([^)]*\) does not compile|build script failed)$/.exec(f)?.[1];
    if (pkg && ['test', 'lane', 'default-test', 'build', 'workspace-build', 'workspace', 'clippy', 'workspace-clippy', 'linux', 'windows', 'windows-arm64'].includes(name)) own(f, `cargo:${pkg}`);
    const app = name.startsWith('conform') && f.slice(name.length + 2).split(' ').slice(0, 2).find(w => WEB_APPS.includes(w));
    if (app) own(f, `conform:${app}`);
    const errored = /^\S+: (\S+\.test\.\w+): an error between tests$/.exec(f)?.[1];
    if (errored) own(f, `bun:${errored}`);
    const xctest = /^(?:ios|macos): ExactKitTests\.(\w+) \w+ failed$/.exec(f)?.[1];
    if (xctest) own(f, `xctest:${name}:${xctest}`);
  }
  if (!['glue', 'other-glue'].includes(name)) return out;
  let file = null;
  for (const line of log.split('\n')) {
    const header = /^(?:\.\/)?(\S+\.test\.\w+):$/.exec(line);
    if (header) { file = header[1]; continue; }
    const fail = /^\(fail\) (.+?) \[[\d.]+m?s\]$/.exec(line);
    if (fail && file && found.includes(`${name}: ${fail[1]}`)) own(`${name}: ${fail[1]}`, `bun:${file}`);
  }
  return out;
}

/** Every failure a log names, as stable strings (compared across commits). */
export function failures(name, log, status) {
  const found = new Set();
  for (const m of log.matchAll(/error: could not compile `([^`]+)` \(([^)]+)\)/g)) found.add(`${name}: ${m[1]} (${m[2]}) does not compile`);
  for (const m of log.matchAll(/error: failed to run custom build command for `(\S+)/g)) found.add(`${name}: ${m[1]} build script failed`);
  // A failing binary ends with cargo's stable `-p <package> --test <target>`.
  let failed = [];
  for (const line of log.split('\n')) {
    if (/^\s+Running /.test(line)) failed = [];
    const test = /^test (\S+) \.\.\. FAILED$/.exec(line);
    if (test) failed.push(test[1]);
    const rerun = /^error: test failed, to rerun pass `([^`]+)`/.exec(line);
    // Name a binary that failed with no failing test as one that did not finish: it crashed, or was stopped, before
    // the rest ran
    if (rerun && !failed.length) found.add(`${name}: ${rerun[1]} did not finish`);
    if (rerun) for (const t of failed.splice(0)) found.add(`${name}: ${rerun[1]} ${t}`);
  }
  // metrics rows: a VIOLATION, which a byte-budget row names with how far over (async-bisect.mjs `holds`), or a
  // measurement whose build FAILED.
  if (name === 'metrics' || name === 'web-bytes') for (const m of log.matchAll(/^[ \t]+(\S[^\n]*?)[ \t]{2,}(?:FAILED\b|[^\n]*\bVIOLATION\b( by \d+(?:\.\d+)? MB\b)?)/gm)) {
    found.add(`${name}: ${m[1]} ${/\bVIOLATION\b/.test(m[0]) ? `VIOLATION${m[2] ?? ''}` : 'FAILED'}`);
  }
  for (const m of log.matchAll(/Test Case '-\[(\S+) (\S+)\]' failed/g)) found.add(`${name}: ${m[1]} ${m[2]} failed`);
  // xcodebuild's parallel testing names a failure by class and method only. A test it reports failed in 0.000 s did
  // not run: its test process had crashed, and xcodebuild fails each test that process still held (2026-11-12: 377
  // of them for 15 crashes in XCTest's own launch, "bundleProxyForCurrentProcess is nil"). Those are one failure, the
  // crash; its xcresult says what crashed.
  for (const m of log.matchAll(/^Test case '(\w+)\.(\w+)\(\)' failed on [^\n]*?(?:\((\d+\.\d+) seconds\))?$/gm)) {
    found.add(m[3] !== undefined && Number(m[3]) === 0 ? `${name}: a test process crashed` : `${name}: ExactKitTests.${m[1]} ${m[2]} failed`);
  }
  // Swift diagnostics by file and message: line numbers move with every edit.
  for (const m of log.replace(/\x1b\[[0-9;]*m/g, '').matchAll(/(?:^|\/)Sources\/(\S+?\.swift):\d+:\d+: error: (.+)$/gm)) found.add(`${name}: ${m[1]}: ${m[2].trim()}`);
  for (const m of log.matchAll(/^Diff in (\S+?):\d+:/gm)) found.add(`${name}: ${m[1].replace(WT + '/', '')} is not formatted`);
  // conform --strict: a failing step by target and step (the what varies run to run).
  for (const m of log.matchAll(/^FAIL (\S+) ([^:\n]+):/gm)) found.add(`${name}: ${m[1]} ${m[2]}`);
  for (const m of log.matchAll(/^\(fail\) (.+?) \[[\d.]+m?s\]$/gm)) found.add(`${name}: ${m[1]}`);
  // Name a Bun file whose code threw outside every test (an import that failed, a throw as the file or a `describe`
  // body ran) by the file Bun names before the error
  const bun = /^bun test v/m.test(log);
  if (bun) {
    let file = null;
    for (const line of log.split('\n')) {
      const header = /^(?:\.\/)?(\S+\.test\.\w+):$/.exec(line);
      if (header) file = header[1];
      else if (line === '# Unhandled error between tests' && file) found.add(`${name}: ${file}: an error between tests`);
    }
  }
  // A bun step that selected no test passed on nothing (`-t` matching none exits 0).
  if (bun && /matched 0 tests|had no matches|^Ran 0 tests/m.test(log)) found.add(`${name}: ran no tests`);
  // difftest: a case that diverged (on the JS target too), failed an expectation or was refused.
  for (const m of log.matchAll(/^(DIVERGE|DIVERGE-JS|ERROR-JS|EXPECT|EMIT|SCRIPT|REFUSED) (.+?)(?: at line \d+)?:/gm)) found.add(`${name}: ${m[1]} ${m[2]}`);
  // difftest apps: an app embedding that is not what the compiler makes of the app now.
  for (const m of log.matchAll(/^STALE \S*?(semantics\/Apps\/\S+?):/gm)) found.add(`${name}: STALE ${m[1]}`);
  // difftest types: the two checkers disagreed on a program or a mutant.
  for (const m of log.matchAll(/^LEAN (ACCEPTS|REFUSES) (.+?) \(/gm)) found.add(`${name}: LEAN ${m[1]} ${m[2]}`);
  for (const m of log.matchAll(/^error: (\S+\.lean):\d+:\d+: (.*)$/gm)) found.add(`${name}: ${m[1]} ${m[2]}`);
  if (status !== 0 && !found.size) found.add(`${name}: exit ${status} (see log)`);
  return [...found];
}

// A check still running past this is hung (libtest has no per-test timeout;
// a test waiting on accept() held the lane for 15 hours): its process group
// is killed and the tests libtest saw running over 60 s are filed by name.
const HANG_MS = 2 * 60 * 60 * 1000;
/** How often the first lane runs every suite, whatever changed. */
const FULL_EVERY_MS = 24 * 60 * 60 * 1000;
export function run(command, args, env, fd, { platform } = {}) {
  if (platform && platform !== process.platform) {
    const notRun = `needs a ${platform === 'win32' ? 'Windows' : platform} worker`;
    writeSync(fd, `not run: ${notRun}\n`);
    return Promise.resolve({ status: 77, hung: false, notRun });
  }
  return new Promise((done) => {
    const child = spawn(command, args, { cwd: WT, env, stdio: ['ignore', fd, fd], detached: true });
    let hung = false;
    const timer = setTimeout(() => { hung = true; try { process.kill(-child.pid, 'SIGKILL'); } catch {} }, HANG_MS);
    child.on('error', () => { clearTimeout(timer); done({ status: 127, hung }); });
    child.on('exit', (status) => { clearTimeout(timer); done({ status: status ?? 128, hung }); });
  });
}

// The worktree's debug target grows with every commit (a full workspace
// build with debuginfo). Keep it bounded before each check: the incremental
// caches first, the whole debug profile if that is not enough.
const KIB = (path) => Number(spawnSync('du', ['-sk', path], { encoding: 'utf8' }).stdout?.split('\t')[0] || 0);
function prune() {
  const debug = resolve(WT, 'target/debug');
  if (!existsSync(debug) || KIB(debug) < 80 * 1024 * 1024) return;
  spawnSync('rm', ['-rf', resolve(debug, 'incremental')]);
  const left = KIB(debug);
  if (left >= 110 * 1024 * 1024) spawnSync('rm', ['-rf', debug]);
  console.log(`pruned ${WT}/target/debug to ${(KIB(debug) / 1024 / 1024).toFixed(0)} GiB (was over 80 GiB)`);
}

/** Check `sha`: on a full run every suite, else those that the files changed since `base` (the last commit checked)
 * affect. */
async function check(sha, since, { base, full, selection: given, only, probe = false, overrides = {} }) {
  checkout(sha);
  return withLaneEdits(WT, () => checkAt(sha, since, { base, full, selection: given, only, probe, overrides }));
}

async function checkAt(sha, since, { base, full, selection: given, only, probe, overrides }) {
  const dir = resolve(STATE_DIR, sha.slice(0, 12), ...(probe ? ['bisect'] : []));
  mkdirSync(dir, { recursive: true });
  prune();
  const env = laneEnv(process.env);
  const installStart = performance.now(), installLoad = loadavg()[0];
  const installed = spawnSync('bun', ['install', '--frozen-lockfile'], { cwd: WT, env, encoding: 'utf8' });
  const result = { sha, subject: git(['log', '-1', '--format=%s', sha]), checks: {}, failures: [], owners: {} };
  writeFileSync(resolve(dir, 'install.log'), `${installed.stdout ?? ''}${installed.stderr ?? ''}${installed.error?.message ?? ''}`);
  result.checks.install = { command: 'bun install --frozen-lockfile', status: installed.status ?? 127,
    complete: installed.status === 0, seconds: (performance.now() - installStart) / 1000, load: installLoad };
  if (installed.status !== 0) result.failures.push(`install: bun install --frozen-lockfile exit ${installed.status}`);
  let selection = null;
  if (TIER === 1) {
    try { selection = given ?? laneSelection(full ? [] : changedPaths(sha, base, git), { full }); }
    catch (e) {
      console.log(`${sha.slice(0, 8)}: every suite runs, since the selection (scripts/affected.mjs) failed: ${e.message}`);
      result.selectionError = e.message;
      result.failures.push('selection: could not select every suite (see result.json)');
      selection = everything(WT);
    }
    Object.assign(result, { full: selection.full && !result.selectionError, selected: selection.ids });
  }
  const store = new Store(resolve(STATE_DIR, 'fingerprints'));
  let inputs;
  for (const [name, command, args, opts] of checks(sha, since, selection ?? undefined).map(step => overrides[step[0]] ?? step)) {
    if (only && !only.has(name)) continue;
    const ran = { command: [command, ...args].join(' ') };
    let gated = null;
    if (opts?.inputs && !probe) {
      const start = performance.now();
      gated = await decide(store, inputs ??= semanticsInputs(), name, command, args, opts);
      Object.assign(ran, gated.error ? { gateError: gated.error } : { fingerprint: gated.fp.digest, changed: gated.changed },
        { gateSeconds: (performance.now() - start) / 1000 });
      if (gated.passed) {
        result.checks[name] = { ...ran, skipped: `inputs of ${gated.passed.sha?.slice(0, 12)}'s pass` };
        continue;
      }
    }
    const logPath = resolve(dir, `${name.replaceAll(/[^a-zA-Z0-9_.-]/g, '_')}.log`), fd = openSync(logPath, 'w');
    const load = loadavg()[0], start = performance.now();
    const r = await run(command, args, opts?.env ?? env, fd, opts);
    closeSync(fd);
    const seconds = (performance.now() - start) / 1000, log = readFileSync(logPath, 'utf8');
    const part = covers(name, selection);
    result.checks[name] = { ...ran, status: r.status, seconds, load, ...(r.hung ? { hung: true } : {}), ...(r.notRun ? { notRun: r.notRun } : {}), ...(part ? { covers: part } : {}) };
    if (r.notRun) console.log(`${sha.slice(0, 8)} ${name}: not run: ${r.notRun}`);
    const outcome = outcomeOf(r.status, r.hung);
    if (gated?.fp) store.record(name, gated.fp, outcome, { sha });
    // A check that could not run here found nothing.
    if (outcome !== 'unavailable') {
      const found = failures(name, log, r.status);
      result.failures.push(...found);
      for (const [f, ids] of Object.entries(owners(name, log, found))) result.owners[f] = [...new Set([...(result.owners[f] ?? []), ...ids])];
    }
    if (r.hung) {
      const slow = [...new Set([...log.matchAll(/^test (\S+) has been running for over 60 seconds/gm)].map(m => m[1]))];
      result.failures.push(...(slow.length ? slow.map(t => `${name}: ${t} hung`) : [`${name}: hung past ${HANG_MS / 60000} min`]));
    }
    result.checks[name].complete = r.status !== 77 && !stoppedEarly(name, result.failures);
  }
  writeFileSync(resolve(dir, 'result.json'), JSON.stringify(result, null, 2));
  return result;
}

/** Unavailable steps cannot qualify the daily full run (LLP 1172.010 E13). */
export const fullCoverage = result => result.full && !Object.values(result.checks).some(c => c.status === 77);

function file(result, fresh, since, lastFull) {
  const short = result.sha.slice(0, 8);
  const timing = Object.entries(result.checks).map(([n, c]) => c.notRun ? `${n} not run: ${c.notRun}` : c.skipped ? `${n} skipped (${c.skipped})` : `${n} ${c.seconds.toFixed(0)} s (exit ${c.status}, load ${c.load.toFixed(0)})`).join(' · ');
  const what = since ? `while checking ${since.slice(0, 8)}..${short}` : `at baseline ${short}`;
  // A suite left out since the last full run may have broken in any commit since.
  const full = fullCoverage(result) && lastFull ? [`This run checked every suite, as the lane does at least daily: one the selection had left out (scripts/affected.mjs) may have failed since ${lastFull.slice(0, 8)}, the last such run.`, ''] : [];
  const body = [`The tier ${TIER} async lane found ${fresh.length} failure(s) ${what}:`, '',
    ...fresh.map(f => {
      const a = result.attribution?.[f];
      return `- ${f}${a?.commit ? ` — introduced by ${a.commit}` : a?.preexisting ? ` — already present at ${a.preexisting}` : ` — attribution unresolved: ${a?.unresolved ?? 'not probed'} (${a?.good ?? since}..${a?.bad ?? result.sha})`}`;
    }), '', ...full, `Commit: ${short} ${result.subject}`, `Checks: ${timing}`,
    `Logs: ${resolve(STATE_DIR, result.sha.slice(0, 12))}/`].join('\n');
  issue(['new', `Async lane${TIER === 2 ? ' (tier 2)' : ''}: ${fresh.length} new failure(s) at ${short}`, '--systems', 'async lane',
    '--slug', `async${TIER === 2 ? '2' : ''}-${short}`, '--author', 'async lane (scripts/async.mjs)', '--body', body, '--quiet'], ROOT);
}

/** The failing suites, found by affected.mjs on the complete hourly range, stay fixed across probes. */
export function failureSelection(selection, wanted, owners) {
  const named = name => wanted.filter(f => f.startsWith(`${name}: `));
  const ids = name => named(name).flatMap(f => owners[f] ?? []);
  const allOwned = name => named(name).length && named(name).every(f => owners[f]?.length);
  const out = { ...selection };
  if (allOwned('test') && !named('lane').length || allOwned('lane') && !named('test').length
    || allOwned('test') && allOwned('lane')) {
    out.full = false;
    out.cargo = [...new Set([...ids('test'), ...ids('lane')].filter(id => id.startsWith('cargo:')).map(id => id.slice(6)))];
  }
  if (allOwned('other-glue')) out.extra = (out.extra ?? []).map(step => step[0] === 'other-glue'
    ? [step[0], step[1], [...step[2].filter(a => !a.startsWith('./')), ...ids('other-glue').map(id => `./${id.slice(4)}`)]] : step);
  if (allOwned('glue')) out.glue = ids('glue').filter(id => id.startsWith('bun:')).map(id => `./${id.slice(4)}`);
  if (allOwned('macos')) out.extra = (out.extra ?? []).map(step => step[0] === 'macos'
    ? ['macos', 'bun', ['host/apple/build.mjs', '--test', '--only', ids('macos').map(id => id.slice('xctest:macos:'.length)).join(',')]] : step);
  if (allOwned('ios')) out.iosOnly = ids('ios').filter(id => id.startsWith('xctest:ios:')).map(id => id.slice(11));
  for (const name of ['conform', 'conform-browsers']) if (allOwned(name)) {
    out.conform = [...new Set([...ids('conform'), ...ids('conform-browsers')].map(id => id.slice('conform:'.length)))];
  }
  return out;
}

export function probeSteps(wanted, selection) {
  const names = new Set(wanted.map(f => f.split(': ', 1)[0]));
  if (['test', 'default-test', 'lane', 'glue', 'web-ax-test'].some(n => names.has(n))) names.add('prepare');
  if (names.has('conform-browsers')) names.add('conform');
  if (names.has('other-glue')) names.add('bundle-fixture');
  if ([...names].some(name => name === 'game' || name.startsWith('game-'))
    || names.has('other-glue') && selection?.extra?.some(([name, , args]) => name === 'other-glue' && args.some(a => a.startsWith('./game/')))) names.add('game-lock');
  for (const name of [...names]) if (name.startsWith('app:')) names.add(`build-${name.slice(0, name.lastIndexOf(':'))}`);
  return names;
}

/** A known suite must read something this commit changed; unknown coverage cannot exclude a commit. */
export function failureAffected(selection, ownerIds, catalogue, name) {
  if (!ownerIds.length) return name === 'install' || checks('', null, { ...selection, lane: [] }).some(([step]) => step === name);
  return ownerIds.some(id => {
    const suite = id.startsWith('xctest:') ? id.split(':').slice(0, 2).join(':') : id;
    if (!catalogue.has(suite)) return true;
    if (!selection.ids.includes(suite)) return false;
    if (suite === id) return true;
    const ran = covers(suite.slice('xctest:'.length), selection);
    return !ran || ran.includes(id);
  });
}

async function attribute(result, fresh, base, pending) {
  if (!base) return {};
  const changes = changedPaths(result.sha, base, git);
  const repo = TIER === 1 ? new Repo(WT, { also: changes }) : null;
  const catalogue = repo && new Set(suites(repo).map(s => s.id));
  // Read the tip's graph before probes move the worktree. Renames include both paths.
  const graphs = new Map();
  const selections = repo && new Map(pending.map(sha => [sha, laneSelection(changedPaths(sha, null, git), { repo, graphs })]));
  const candidates = new Map(fresh.map(f => [f, pending.filter(sha => !repo
    || failureAffected(selections.get(sha), result.owners[f] ?? [], catalogue, f.split(': ', 1)[0]))]));
  const eligible = fresh.filter(f => candidates.get(f).length);
  const selection = repo ? laneSelection(changes, { full: result.full, repo, graphs }) : undefined;
  const selected = selection && failureSelection(selection, eligible, result.owners);
  const only = probeSteps(eligible, selected);
  const overrides = eligible.some(f => f.startsWith('difftest-quick: '))
    ? { 'difftest-quick': quickProbe(readFileSync(resolve(STATE_DIR, result.sha.slice(0, 12), 'difftest-quick.log'), 'utf8')) } : {};
  return bisectAffected(base, result.sha, fresh, candidates, async (sha, wanted) => {
    console.log(`bisect ${sha.slice(0, 8)}: ${[...only].join(', ')}`);
    const found = await check(sha, null, { base, full: false, selection: selected, only, probe: true, overrides });
    return probeOutcomes(found, wanted);
  }, sha => git(['rev-parse', `${sha}^`]));
}

async function once(state) {
  git(['fetch', '-q', 'origin']);
  const tip = git(['rev-parse', BRANCH]);
  const pending = state.last
    ? git(['rev-list', '--first-parent', '--reverse', `${state.last}..${tip}`]).split('\n').filter(Boolean)
    : [tip];
  const full = TIER === 1 && (!state.last || process.argv.includes('--full') || !(Date.now() - Date.parse(state.fullAt ?? 0) < FULL_EVERY_MS));
  if (!pending.length && !full) return;
  const since = state.last, result = await check(tip, since, { base: since, full });
  retainSkippedFailures(state.failures ?? [], result, state.owners ?? {});
  const baseline = state.failures === undefined, fresh = newFailures(state.failures ?? [], result.failures);
  console.log(`${tip.slice(0, 8)} ${result.subject}: ${result.failures.length} failure(s), ${baseline ? 'baseline' : `${fresh.length} new`}`);
  if (fresh.length && !baseline) {
    try { result.attribution = await attribute(result, fresh, since, pending); }
    catch (e) { result.attribution = Object.fromEntries(fresh.map(f => [f, { unresolved: e.message, good: since, bad: tip }])); }
    writeFileSync(resolve(STATE_DIR, tip.slice(0, 12), 'result.json'), JSON.stringify(result, null, 2));
    file(result, fresh, since, state.fullSha);
  }
  // Bisection leaves the lane's own worktree at a probe. Restore the checked tip before the next run.
  checkout(tip);
  const unpublished = publish(noteOf(result, TIER));
  if (unpublished) console.log(`${tip.slice(0, 8)}: the note was not published: ${unpublished}`);
  Object.assign(state, { last: tip, failures: result.failures, owners: result.owners },
    fullCoverage(result) ? { fullAt: new Date().toISOString(), fullSha: tip } : {});
  writeFileSync(STATE, JSON.stringify(state, null, 2));
}

if (import.meta.main) {
  if (process.argv.includes('--help')) {
    console.log('Usage: bun scripts/async.mjs [--once] [--from <rev>] [--worktree <dir>] [--interval <s>] [--branch <ref>] [--tier 1|2] [--full] [--explain [<rev>]]');
    process.exit(0);
  }
  if (!existsSync(WT)) git(['worktree', 'add', '--detach', WT, BRANCH]);
  mkdirSync(STATE_DIR, { recursive: true });
  const state = existsSync(STATE) ? JSON.parse(readFileSync(STATE, 'utf8')) : {};
  if (option('--from')) state.last = git(['rev-parse', option('--from')]);
  if (process.argv.includes('--explain')) {
    const at = process.argv[process.argv.indexOf('--explain') + 1];
    git(['fetch', '-q', 'origin']);
    const sha = git(['rev-parse', at && !at.startsWith('--') ? at : BRANCH]);
    checkout(sha);
    const store = new Store(resolve(STATE_DIR, 'fingerprints')), inputs = semanticsInputs();
    for (const [name, command, args, opts] of semanticsChecks()) {
      const { error, passed, changed } = await decide(store, inputs, name, command, args, opts);
      console.log(`${name}: ${error ? `run (the gate failed: ${error})` : passed ? 'skip' : `run (changed: ${changed.join(', ')})`}`);
    }
    process.exit(0);
  }
  const interval = Number(option('--interval', 3600)) * 1000;
  if (!Number.isFinite(interval) || interval <= 0) throw new Error('--interval must be a positive number of seconds');
  for (;;) {
    const started = Date.now();
    try { await once(state); }
    catch (error) {
      console.error(`hourly lane: ${error.message}`);
      if (process.argv.includes('--once')) process.exitCode = 1;
    }
    if (process.argv.includes('--once')) break;
    await new Promise(done => setTimeout(done, Math.max(0, interval - (Date.now() - started))));
  }
}
