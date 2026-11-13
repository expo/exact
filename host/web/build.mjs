#!/usr/bin/env bun
import { writeCrawlerFiles } from './crawl.mjs';
import { warmGameInputs, warmGameEnvironment, publishWarmGame } from '../web-js/module.mjs';
import { generatedInputFiles } from '../../game/inputs.mjs';
// Build the web app (LLP 1071): the JS target, `host/web-js/build.mjs`, the
// plan compiled to one ES module over a small runtime, into the same dist;
// what it refuses (a capability the runtime lacks, named) fails the build.
// Games and `--wasm` use the wasm target. Shipping modules use `web` plus
// wasm-opt; proofs may select `gpu-dev` without wasm-opt. The host stays `web`.
// Usage: bun host/web/build.mjs [crate=caltrain-web] [--out <dir>] [--render rust|js|none]
// (`--js` is accepted and is the default.) `--wasm` is internal, for what the
// retiring wasm target still serves (LLP 1071 §7, "Retiring the wasm target
// on the web"): conformance's reference, delivery's bake (the streams'
// bundle, whose plan the JS web root compiles), the resident dev loop a
// native client opens, `motionparity` and the smoke's plan fixtures.
// `--render <rust|js|none>` passes to the JS target's build. A JS build's completion marker says
// `target: 'js'`, so the dev loop, the agent and the smoke know which they got. `--out <dir>`
// writes the JS target there instead of the app's target/web-dist, and ends by saying how a
// static host serves it.
// `--check` (internal) checks dependencies and toolchains; the JS compiler selects the plan's artifacts.
// EXACT_WEB_NAMES=1 keeps the wasm's function names, for metrics' byte
// attribution (LLP 1047 D9); EXACT_WEB_LINK=all links every capability, as
// the dev loop does (LLP 1047 D7). Either target draws the neutral look
// (LLP 1115.000 D4 part B; look.mjs composes the sheet).
// Developer builds bake development trust; EXACT_UPDATE_TRUST=production
// requires signing keys, and the deploy verb always selects production.
import { grantOrigins } from './navigation/grant-origins.js';
import { buildHostGlue, gateAgent, modulePrelude } from '../web-js/glue.mjs';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { writeInstallPages } from '../../scripts/install-page.mjs';
import { sealEnvelope } from '../../scripts/deploy-signing.mjs';
import { compilerWarnings, webInputDigests } from '../../scripts/agent-launch.mjs';
import { webBuildFailure } from '../../scripts/app-cli.mjs';
import { builtSettings } from '../../js/bake/src/__exact_env.mjs';
import { authClientMetadata, checkModuleRoster, gpuModules, markWorlds, rustPolicy, webGpuArtifacts, webHostFiles, worldSurfaces } from '../../scripts/app.mjs';
import { webAudioSession, webCspFile, webHead } from '../../scripts/web-head.mjs';
import { buildRust, rustFiles, rustCards, rustPackage } from '../../scripts/rust.mjs';
import { webAppManifest, webDist, copyShaders, bakeOutput, bakeWatchEnv, buildBake, contractLast, readBake, verifyBakeFiles, developmentBuildEnv, resolveApp, webToolchainEnv } from '../../scripts/app.mjs';
import { buildEditor, buildFlow } from '../web-js/module.mjs';
import { closeFilesystemReader } from '../../scripts/filesystem.mjs';
import { guardFlags, reportBuilt } from '../../scripts/help.mjs';
import { BINARYEN_DOWNLOAD, optimizeWasm, splitStages, unsplitReason } from './stages.mjs';
import { interfaceStyle, withInterfaceStyle, withLook } from './look.mjs';
import { webIconLinks, webThemeColors } from '../../scripts/app-icons.mjs';
import { appManifestDigest, buildFileCards, copyStaticTreeIfPresent, listAssets, hostingNotes, planRouteTable, publicFileCards, replaceBuild, webEnvelope, moduleCards, MODULE_FILES } from './serve.mjs';
import { foreignBeside } from './generations.mjs';
import { writeAppleAssociation } from './association.mjs';
import { cargoInputPaths, compilerPaths } from '../../scripts/cargo-dep-info.mjs';

const SDK_ROOT = fileURLToPath(new URL('../..', import.meta.url));
const metadataCache = new Map();
const digest = path => { try { return createHash('sha256').update(readFileSync(path)).digest('hex'); } catch { return null; } };
function webMetadata(workspace) {
  const cached = metadataCache.get(workspace);
  if (cached && cached.inputs.every(([path, bytes]) => digest(path) === bytes)) return cached.metadata;
  const result = spawnSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], {
    cwd: workspace, env: developmentBuildEnv(), encoding: 'utf8', maxBuffer: 128 << 20,
  });
  if (result.error || result.status !== 0) throw new Error(`web build: cargo metadata in ${workspace}: ${result.stderr || result.error?.message}`);
  const metadata = JSON.parse(result.stdout);
  const paths = ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml', '.cargo/config'].map(file => resolve(workspace, file));
  paths.push(...metadata.packages.filter(pkg => !pkg.source).map(pkg => pkg.manifest_path));
  metadataCache.set(workspace, { metadata, inputs: [...new Set(paths)].map(path => [path, digest(path)]) });
  return metadata;
}

/** Source roots of the tools and artifacts this build consumes. A JS-only app needs no Cargo workspace (LLP 1153 D2.1). */
export function webSourceRoots(app, js, render = 'rust') {
  // Preparation writes a clean game's workspace before even the shared tools' Cargo query.
  const prepared = app.manifest.game && app.prepare ? app.prepare() : null;
  const roots = new Set([resolve(SDK_ROOT, 'scripts')]), files = new Set();
  const workspaces = [];
  const closure = (metadata, names) => {
    const packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
    const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
    const pending = names.map(name => {
      const pkg = metadata.packages.find(pkg => pkg.name === name);
      if (!pkg) throw new Error(`web build: Cargo has no ${name} package`);
      return pkg.id;
    }), seen = new Set();
    const local = [];
    while (pending.length) {
      const id = pending.pop(); if (seen.has(id)) continue; seen.add(id);
      const pkg = packages.get(id);
      if (!pkg.source) local.push(pkg);
      if (!pkg.source && !dirname(pkg.manifest_path).startsWith(resolve(app.dir, '.shells') + sep)) roots.add(dirname(pkg.manifest_path));
      for (const dep of nodes.get(id)?.deps ?? []) if (dep.dep_kinds.some(kind => kind.kind !== 'dev')) pending.push(dep.pkg);
    }
    for (const name of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml', '.cargo/config']) files.add(resolve(metadata.workspace_root, name));
    workspaces.push({ metadata, packages: local });
  };
  closure(webMetadata(SDK_ROOT), ['exact-web-js', 'exact-web', 'exact-js-bake', 'exact-markdown-editor', 'exact-web-js-motion', 'exact-textflow']);
  const names = [], build = resolve(app.dir, 'web/build.rs');
  const authored = existsSync(build) && !/^\s*fn main\(\)\s*\{\s*exact_js_bake::build\w*\(/m.test(readFileSync(build, 'utf8'));
  if (app.crate) {
    if (!js || authored) names.push(app.crate('web'));
    if (app.manifest.rust?.module?.package) names.push(app.manifest.rust.module.package);
    if (app.manifest.game || existsSync(resolve(app.dir, 'gpu/Cargo.toml'))) names.push(app.crate('gpu'));
    names.push(...gpuModules(app.manifest).map(({ name }) => app.crate(`gpu-${name}`)));
    const at = (!js || render === 'rust') && ['linux', 'web'].find(dir => existsSync(resolve(app.dir, dir, 'src/bin', `${app.name ?? app.crate('web').replace(/-web$/, '')}-render.rs`)));
    if (at) names.push(app.crate(at));
  }
  if (names.length) {
    closure(prepared ?? webMetadata(app.workspace), names);
  }
  // Package scans find new sources; Cargo's records also name includes and watches outside the package.
  const targets = [...new Set([app.target, process.env.CARGO_TARGET_DIR, process.env.CARGO_BUILD_BUILD_DIR,
    ...workspaces.flatMap(({ metadata }) => [metadata.target_directory, metadata.build_directory]), resolve(SDK_ROOT, 'target')].filter(Boolean).map(path => resolve(path)))];
  const sourceBases = [app.dir, ...workspaces.map(({ metadata }) => metadata.workspace_root)];
  const under = (dir, path) => path === dir || path.startsWith(dir + sep);
  const generated = path => targets.some(target => under(target, path) && !sourceBases.some(base => base.length > target.length && under(base, path)))
    || path.startsWith(resolve(app.dir, '.shells') + sep);
  const add = (path, watch = false) => {
    if (generated(path)) return;
    const info = statSync(path, { throwIfNoEntry: false }), directory = info?.isDirectory();
    if (directory || (watch && !info)) roots.add(path);
    if (!directory) files.add(path);
  };
  const targetsOf = workspaces.flatMap(({ packages }) => packages.flatMap(pkg => pkg.targets ?? []));
  const sources = new Set(targetsOf.map(target => resolve(target.src_path)));
  // rustc names a target's dep-info after its crate (`<crate>-<hash>.d`, `lib<crate>.d`, `<crate>.d`), so the
  // rest of a large target directory is never read.
  const crates = new Set(targetsOf.map(target => target.name.replace(/-/g, '_')));
  const ours = file => { const stem = file.slice(0, -2).replace(/-[0-9a-f]{16}$/, ''); return crates.has(stem) || (stem.startsWith('lib') && crates.has(stem.slice(3))); };
  const scan = dir => {
    for (const entry of existsSync(dir) ? readdirSync(dir, { withFileTypes: true }) : []) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) {
        if (!/^(\..*|incremental|node_modules|artifacts)$/.test(entry.name)) scan(path);
      } else if (entry.name.endsWith('.d')) {
        if (!ours(entry.name)) continue;
        const text = readFileSync(path, 'utf8');
        if (!text.includes(': ')) continue;
        for (const { metadata } of workspaces) {
          const paths = compilerPaths(text, metadata.workspace_root);
          if (!sources.has(paths[0])) continue;
          // Registry inputs belong to their locked packages, not this checkout's source receipt.
          for (const input of paths) if (!/[/\\](registry|git)[/\\](src|checkouts)[/\\]/.test(input)) add(input);
          break;
        }
      } else if ((entry.name === 'output' && basename(dirname(dir)) === 'build')
        || (entry.name === 'stdout' && basename(dir) === 'run' && basename(dirname(dirname(dirname(dir)))) === 'build')) {
        const name = entry.name === 'output' ? basename(dir).replace(/-[a-f0-9]+$/, '') : basename(dirname(dirname(dir)));
        for (const { packages } of workspaces) {
          const pkg = packages.find(pkg => pkg.name === name);
          if (pkg) for (const input of cargoInputPaths(readFileSync(path, 'utf8'), dirname(pkg.manifest_path))) add(input, true);
        }
      } else if (entry.name.endsWith('.json') && basename(dir) === 'kept') {
        const kept = JSON.parse(readFileSync(path, 'utf8'));
        if (kept.entry && existsSync(resolve(kept.entry, 'inputs.json'))) {
          const record = JSON.parse(readFileSync(resolve(kept.entry, 'inputs.json'), 'utf8'));
          for (const [input] of record.files) add(resolve(SDK_ROOT, input), true);
        }
      }
    }
  };
  for (const target of targets) scan(target);
  return [...roots, ...[...files].map(file => ({ file }))];
}

function completeWebInputs(app, js, render, inputRoots, inputs, inputsAt) {
  const next = webSourceRoots(app, js, render);
  inputRoots.splice(0, inputRoots.length, ...next);
  for (const [path, value] of Object.entries(webInputDigests(app, js, next))) {
    if (Object.hasOwn(inputs, path)) continue;
    // An include first discovered by Cargo cannot certify bytes edited during compilation.
    const info = statSync(resolve(SDK_ROOT, path), { throwIfNoEntry: false });
    inputs[path] = info && info.ctimeMs >= inputsAt ? null : value;
  }
}

async function main() {
guardFlags('web-build', process.argv.slice(2));
const target = ['--js', '--wasm'].find((flag) => process.argv.includes(flag));
// `--bake` (internal, delivery's): the web crate's bake without its wasm —
// `cargo check` runs the build script (the baked plan, its receipt, a
// TypeScript module's artifacts), and the origin files are written as the
// wasm build writes them, with no `app.wasm`, glue or leaf wasm (LLP 1071
// §7, "Retiring the wasm target on the web", step c).
const bakeOnly = process.argv.includes('--bake');
const checkOnly = process.argv.includes('--check');
process.argv = process.argv.filter((a) => !['--js', '--wasm', '--bake', '--check'].includes(a));
// `--render <rust|js|none>`: the JS target's pages at build (host/web-js/build.mjs).
const renderFlag = process.argv.indexOf('--render');
const render = renderFlag < 0 ? [] : process.argv.splice(renderFlag, 2);
// `--out <dir>`: the JS target's build written there, a site to host (LLP 1153 D3), in place of target/web-dist.
const outFlag = process.argv.indexOf('--out');
const outDir = outFlag < 0 ? null : process.argv.splice(outFlag, 2)[1];
if (outFlag >= 0 && (!outDir || outDir.startsWith('--'))) { console.error('web-build: --out names a directory'); process.exit(2); }
const app = resolveApp(process.argv[2]);
const outRoot = outDir && outPath(outDir);
/** `--out`'s directory, its links resolved, once it is one the build may replace whole: never the app, this checkout or
 * a directory holding either; inside the app, `dist` or a path under `dist/` or `target/`, which no source scan reads,
 * nor the builds kept beside it (`dist.gen`), and the scaffold's .gitignore lists; and, when it is there already, a
 * web build. */
function outPath(given) {
  let at = resolve(given);
  const rest = [];
  while (!existsSync(at)) { rest.unshift(basename(at)); at = dirname(at); }
  const path = resolve(realpathSync(at), ...rest), within = (dir, p) => p === dir || p.startsWith(dir.endsWith(sep) ? dir : dir + sep);
  const refuse = (why) => { console.error(`web-build: --out ${given}: ${why}`); process.exit(2); };
  const appDir = realpathSync(app.dir), checkout = realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
  for (const [dir, what] of [[appDir, 'the app'], [checkout, 'this exact2 checkout']]) if (within(path, dir)) refuse(`it is or holds ${what} (${dir}), which the build would replace`);
  const first = within(appDir, path) ? relative(appDir, path).split(sep)[0] : null;
  if (first !== null && !['dist', 'target'].includes(first)) refuse('inside the app, the build goes in dist (or under dist/ or target/), which no source scan of the app reads');
  if (existsSync(path) && !statSync(path).isDirectory()) refuse('it is not a directory');
  if (existsSync(path) && readdirSync(path).length && !existsSync(resolve(path, '.exact-build.json'))) refuse('it holds files that are not a web build, which the build would replace: name a new or empty directory');
  // And what a build there keeps beside it, which it would replace or reap: absent, or a build's own (generations.mjs).
  const foreign = foreignBeside(path);
  if (foreign) refuse(foreign);
  return path;
}
// After resolveApp, which names `bun install` when the packages are missing.
const [{ rolldown }, { minifySync }] = await Promise.all([import('rolldown'), import('rolldown/experimental')]);
// A game's web build is the wasm target (Charlie, 2026-09-29: "Game runtime
// is fine to be on wasm"; LLP 1071 §8). For an app the JS target is the web
// build, and what it refuses is an error; `--wasm` is internal (below).
const game = app.manifest.game !== undefined;
const jsTarget = target !== '--wasm' && !game && !bakeOnly;
const inputRoots = webSourceRoots(app, jsTarget, render[1] ?? 'rust');
if (checkOnly) {
  if (!jsTarget) {
    webToolchainEnv(developmentBuildEnv());
    if (!bakeOnly) webGpuArtifacts(app, null, { check: true });
  }
  else {
    mkdirSync(app.target, { recursive: true });
    const stage = mkdtempSync(resolve(app.target, 'web-check-'));
    try {
      const checked = spawnSync(process.execPath, [resolve(SDK_ROOT, 'host/web-js/build.mjs'), app.name, '--prerequisites', '--out', stage, ...render],
        { stdio: 'inherit', env: { ...process.env, EXACT_APP_DIR: app.dir } });
      if (checked.status !== 0) process.exitCode = checked.status ?? 1;
    } finally { rmSync(stage, { recursive: true, force: true }); }
    if (process.exitCode) return;
  }
  console.log(`${app.name}: web build prerequisites ready`);
  return;
}
if (outDir && (target === '--wasm' || game || bakeOnly)) { console.error('web-build: --out writes the JS target, an app\'s web build; a game\'s wasm build is target/web-dist'); process.exit(2); }
if (target !== '--wasm' && !game && !bakeOnly) {
  // An app outside apps/ reaches it through EXACT_APP_DIR, as here. It builds in a stage of its own beside the
  // output, which replaces the output only when complete (`replaceBuild`), so builds run at once never mix, and the
  // build replaced is kept by its generation while a page `serve` sent may still ask for its files.
  const dist = outRoot || webDist(app);
  mkdirSync(dirname(dist), { recursive: true });
  let stage = mkdtempSync(`${dist}.stage-`);
  process.on('exit', () => { if (stage) rmSync(stage, { recursive: true, force: true }); });
  // What this build reads, and when, as it starts: the next check (webChanges) compares every input
  // changed since then, so a source changed or gone while it compiles, or a build published over it
  // meanwhile, is a change, never taken as built. A site to host is no drive's, and its marker names no source.
  const inputsAt = Date.now(), inputs = outDir ? null : webInputDigests(app, true, inputRoots);
  // A site to host is a release, as delivery builds one (`--production`): agent mode refused, the app's storage its id
  // alone, its install pages a release's.
  const release = outDir ? { flags: ['--production'], env: { EXACT_UPDATE_TRUST: 'production' } } : { flags: [], env: {} };
  const js = spawnSync(process.execPath, [fileURLToPath(new URL('../web-js/build.mjs', import.meta.url)), app.name, '--out', stage, ...render, ...release.flags], { stdio: ['ignore', 'pipe', 'pipe'], encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: app.dir, ...release.env } });
  // What it says names the output, not the stage.
  const said = (text) => (text ?? '').replaceAll(stage, dist);
  js.stderr = said(js.stderr);
  process.stdout.write(said(js.stdout));
  // The compiler's warnings, as `contract build` says them, its file from here: the count names them (LLP 1116.004 D7).
  const compiler = compilerWarnings(js.stderr);
  if (js.status === 0) {
    if (inputs) completeWebInputs(app, true, render[1] ?? 'rust', inputRoots, inputs, inputsAt);
    // What the JS target warned of, in its own words: the count it prints names nothing (workout F3, kanban F30).
    // The bundler's notes on the generated glue are not the app's to act on. The types app.ts imports, when
    // the build rewrote them, are one line (LLP 1116.001 D4.5).
    for (const line of compiler) console.error(line);
    for (const line of (js.stderr ?? '').split('\n')) if (/^(warning|app\.contract\.d\.ts): /.test(line)) console.error(line);
    // The origin's association file, from the manifest's claims (LLP 1030 D1). Its `webcredentials` comes from the
    // bake's grants, which only the wasm build below runs.
    writeAppleAssociation(stage, app);
    // The receipt first, since what it records may come from the build's intermediates. `release`: a site to host
    // (`--out`), built again (to the same directory, as a release) when what it was built from changes.
    // `env`: each setting app.ts read, as inlined, and its source (LLP 1153 D4), for the drive's logs and `changedSettings`.
    writeFileSync(resolve(stage, '.exact-build.json'), JSON.stringify({ exactBuild: 1, target: 'js', app: { id: app.id, name: app.displayName },
      manifestSha256: appManifestDigest(app), files: buildFileCards(stage), ...(inputs ? { inputs, inputsAt, inputRoots } : {}), ...builtSettings(stage), ...(outDir ? { release: true } : {}) }) + '\n');
    // A site to host holds the site, not the build's intermediates (`.gen`, the app's captured sources among them;
    // the receipt's file cards name no dot path), but for what `serve` renders a `render=request` or `cached` route's
    // page with, when it has such a route: the server bundle (LLP 1153 D3.4) and the analyzed sheet a page selects
    // its rules from (LLP 1048.007 D4); and, for a render server, the app's part of its CSP (LLP 1048.000 D11).
    if (outDir) {
      const routes = JSON.parse(readFileSync(resolve(stage, 'routes.json'), 'utf8')).routes;
      const kept = [...routes.some((r) => r.render === 'request' || r.render === 'cached') ? ['server.js', 'page-style.json'] : [], 'csp.json']
        .filter((name) => existsSync(resolve(stage, '.gen', name))).map((name) => [name, readFileSync(resolve(stage, '.gen', name))]);
      rmSync(resolve(stage, '.gen'), { recursive: true, force: true });
      if (kept.length) mkdirSync(resolve(stage, '.gen'));
      for (const [name, bytes] of kept) writeFileSync(resolve(stage, '.gen', name), bytes);
    }
    replaceBuild(stage, dist);
    stage = null;
    if (outDir) console.log(hostingNotes(dist, existsSync(resolve(app.dir, 'exact.mjs')) ? 'bun exact.mjs serve' : 'bun host/web/serve.mjs').join('\n'));
    // A release is `--out`'s alone: without it the JS target builds for development, whatever EXACT_UPDATE_TRUST says.
    if (process.argv.includes('--json')) reportBuilt({ path: dist, target: 'web', trust: outDir ? 'production' : 'development' });
    process.exit(0);
  }
  console.error(`${webBuildFailure(js.stderr).join('\n')}\n${app.name}: the web build (the JS target) failed; the wasm target is internal (--wasm)`);
  process.exit(1);
}
const crate = app.crate('web');
const kib = (n) => `${(n / 1024).toFixed(0)} KiB`;
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
// `EXACT_WEB_DIST` names another output directory: `exact deploy` bakes into a
// run-specific one and never publishes from the dev server's shared dist/.
const dist = webDist(app);
const previous = `${dist}.previous`;
// A hard stop can land after dist moved aside but before the completed stage
// took its place. Restore the prior complete build before doing slow work;
// serve.mjs/dev.mjs also fall back to it during the live rename window.
if (!existsSync(dist) && existsSync(previous)) renameSync(previous, dist);
else if (existsSync(dist) && existsSync(previous)) rmSync(previous, { recursive: true, force: true });
const buildEnv = developmentBuildEnv();
buildEnv.EXACT_BAKE_OUTPUT = bakeOutput(app, buildEnv);
// Proofs select the requested game profile under either trust mode; ordinary builds ship web.
const proofProfile = game ? process.env.EXACT_GAME_PROOF_PROFILE : undefined;
if (proofProfile && !['gpu-dev', 'release'].includes(proofProfile)) throw new Error('EXACT_GAME_PROOF_PROFILE must be gpu-dev or release');
const gpuProfile = proofProfile === 'gpu-dev' ? 'gpu-dev' : 'web';
if (proofProfile === 'gpu-dev') Object.assign(buildEnv, warmGameEnvironment(buildEnv));
// `EXACT_WEB_NAMES=1` keeps the wasm's name section for attribution (LLP 1047
// D9, metrics.mjs --long): rustc strips only DWARF, which would stop binaryen's
// duplicate-function elimination, and wasm-opt keeps names. The code is the
// shipped code's; the file is not for shipping.
const keepNames = process.env.EXACT_WEB_NAMES === '1';
// A production artifact is split into a core and its staged capabilities
// (LLP 1047.000 §9), which needs the names too; they are stripped after the
// split. The dev loop's `EXACT_WEB_LINK=all` build and the names build stay
// whole, as does a machine whose binaryen isn't the pinned one.
const unsplit = keepNames || buildEnv.EXACT_WEB_LINK === 'all' ? 'a development or names build' : unsplitReason();
if (keepNames || !unsplit) buildEnv.CARGO_PROFILE_WEB_STRIP = 'debuginfo';
// Capture before the bake so edits made during compilation or packaging stay detectable.
const inputsAt = Date.now(), inputs = webInputDigests(app, false, inputRoots);
const generated = game ? generatedInputFiles(app.dir) : [];
const warmInputs = !bakeOnly ? warmGameInputs(app, {...buildEnv, EXACT_GAME_PROOF_PROFILE:proofProfile}) : null;
const buildReceipt = contractLast(() => buildBake(app, 'web', 'wasm32-unknown-unknown', {env:buildEnv, check:bakeOnly, gpuProfile}));
// Game bakes create, replace and prune assets. Capture those outputs before packaging;
// authored files keep their initial digests, including handwritten assets beside generated ones.
if (game) {
  const outputs = new Set(generatedInputFiles(app.dir));
  for (const path of new Set([...generated, ...outputs])) {
    if (!existsSync(path)) inputs[path] = null;
    else if (outputs.has(path)) inputs[path] = createHash('sha1').update(readFileSync(path)).digest('hex');
  }
}
const built = resolve(app.target, 'wasm32-unknown-unknown/web', crate.replace(/-/g, '_') + '.wasm');
// Build one app into its own staging directory. Only a complete build replaces
// dist, so a server sees the previous app or the next one, never a mixture;
// replacing the directory also drops every stale optional/private artifact.
// Stages live under ignored target/, so even a SIGKILL leaves no source dirt.
const stages = resolve(app.target, 'web-dist-stages');
mkdirSync(stages, { recursive: true });
// A worktree may share target/ through a symlink. This directory is ours;
// retain its physical name before the strict filesystem reader inventories it.
let stage = realpathSync(mkdtempSync(resolve(stages, `${app.name.replace(/[^a-zA-Z0-9_-]/g, '_')}-`)));
process.on('exit', () => { if (stage) rmSync(stage, { recursive: true, force: true }); });
const out = resolve(stage, 'app.wasm');

// Keep small single-caller functions inline, but bound large expansions: -Oz's
// unlimited default shrinks raw bytes while increasing both Brotli and gzip.
// Bound 20 and passes to convergence ship the fewest compressed bytes
// (2026-09-24: 3 KB less Brotli per app than 50 alone, for ~0.1% more raw).
// Functions of up to 6 instructions are inlined everywhere: V8 compiles a wasm
// function lazily, on the main thread, at its first call, and a tiny one costs
// about what a large one does to set up. RealWorld's boot compiles 15% fewer
// (1,314 -> 1,123), for 511 B less Brotli (+7 KB raw); 10 or 16 add Brotli.
// The feature flags match what rustc's wasm32 target emits.
// `--low-memory-unused` lets binaryen fold a constant under 1024 added to a
// pointer into the access's offset, which differs only when the addition
// wraps below zero. Rust's pointers never wrap, and nothing lives there:
// the stack is first in memory and grows down from 1 MiB, so it reaches
// below 1024 only in its last KiB (2026-09-28: 0.7–1.4 KiB brotli).
let optNote = 'none (--bake)';
if (!bakeOnly) {
const named = resolve(stage, 'app.named.wasm');
const opt = optimizeWasm(built, unsplit ? out : named, ['-Oz', '--one-caller-inline-max-function-size', '20', '--always-inline-max-function-size', '6', '--converge', '--low-memory-unused', '--enable-bulk-memory', '--enable-nontrapping-float-to-int', '--enable-sign-ext', '--enable-mutable-globals', keepNames || !unsplit ? '-g' : '--strip-debug', '--strip-producers'], resolve(app.target, 'web-opt', crate));
if (opt.error?.code === 'ENOENT') { copyFileSync(built, out); optNote = `wasm-opt not on PATH (binaryen: brew install binaryen, or ${BINARYEN_DOWNLOAD}): shipped unoptimized`; }
else if (opt.status !== 0) process.exit(opt.status ?? 1);
else if (unsplit) optNote = `wasm-opt -Oz${opt.cached ? ' (cached)' : ''}; unsplit: ${unsplit}`;
else {
  const split = splitStages(named);
  writeFileSync(out, split.core);
  for (const [path, bytes] of split.files) { mkdirSync(resolve(stage, path, '..'), { recursive: true }); writeFileSync(resolve(stage, path), bytes); }
  rmSync(named);
  optNote = `wasm-opt -Oz${opt.cached ? ' (cached)' : ''}; the core, with stages: ${split.report.join('; ')}`;
}
}

// The app's static files ride beside the page: `assets/…` images and an
// optional `deck/` iframe guest (@ref LLP 1020 M1). Replaced whole, so a
// deleted file does not linger in dist.
const assets = resolve(app.dir, 'assets');
copyStaticTreeIfPresent(assets, resolve(stage, 'assets'));
const deck = resolve(app.dir, 'deck');
copyStaticTreeIfPresent(deck, resolve(stage, 'deck'));
// The GPU crate's shaders (LLP 1030 D8): `shaders/<name>.wgsl` beside the
// page, fetched and registered by the GPU glue before a surface is created
// — never a string in the wasm.
copyShaders(app, resolve(stage, 'shaders'));
// The neutral sheet follows the browser resets (LLP 1115.000 D4 part B; look.mjs).
writeFileSync(resolve(stage, 'index.html'), withInterfaceStyle(withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')), interfaceStyle(app.manifest)));
const production = buildEnv.EXACT_UPDATE_TRUST === 'production';
const hostDir = resolve(stage, '.host');
function copyHostFiles(group) {
  for (const [name, source] of Object.entries(webHostFiles(group))) {
    if (!name.endsWith('.js')) { copyFileSync(resolve(root, source), resolve(stage, name)); continue; }
    // The module realm's prelude is joined from its scripts here: the module group is copied without the host glue's build.
    const code = name === 'module-prelude.js' ? modulePrelude(root)
      : readFileSync(source.startsWith('target/web-host/') ? resolve(hostDir, name) : resolve(root, source), 'utf8');
    // Production ships executable code, without source comments and long
    // local names. Keep exports and property names intact across modules.
    const result = minifySync(name, production ? gateAgent(code, name) : code, { module: !['module-prelude.js', 'module-binary.js', 'audio-session.js'].includes(name) });
    if (result.errors.length) throw new Error(`${name}: ${JSON.stringify(result.errors)}`);
    writeFileSync(resolve(stage, name), result.code);
  }
}
if (!bakeOnly) {
  await buildHostGlue(root, { production, out: hostDir });
  copyHostFiles('base');
  rmSync(hostDir, { recursive: true });
}
if (!bakeOnly && !production) copyHostFiles('development');
// These app-independent modules use the same input-checked products as the JS
// target, so a gameplay edit does not rerun Cargo and wasm-opt for either leaf.
if (!bakeOnly) await Promise.all([
  [buildEditor, 'markup-editor.wasm'], [buildFlow, 'textflow.wasm'],
].map(async ([build, name]) => copyFileSync(await build({keepFresh:true}), resolve(stage, name))));

// The plan and its pointer card (LLP 1023 D1/D2): extract the exact bytes
// baked into the produced, optimized wasm. Compiling app.contract a second
// time here could pair app.wasm with a later source revision. A native client
// GETs the page URL, follows index.html's link to exact.json, and fetches this
// app.plan; a browser never notices. Header offsets are the generated
// encoder's fixed little-endian layout.
// `--bake` reads the same bytes from the bake's own output.
const planOut = resolve(stage, 'app.plan');
let wasm = null, exports = {}, planBytes;
if (bakeOnly) planBytes = readFileSync(resolve(buildEnv.EXACT_BAKE_OUTPUT, 'web-wasm32-unknown-unknown.plan'));
else {
wasm = readFileSync(out);
const unbooted = () => { throw new Error('app logic ran while extracting baked bytes'); };
const { instance } = await WebAssembly.instantiate(wasm, { exact_grants: grantOrigins(() => instance.exports.memory), exact_js: { call: unbooted }, exact_rust: { load: unbooted, call: unbooted, read: unbooted, drop: unbooted }, exact_data: { random: unbooted, agent_seed: unbooted }, exact_clock: { now: unbooted }, exact_geometry: { range: unbooted, read: unbooted, point: unbooted, points: unbooted, shown: unbooted } });
exports = instance.exports;
if (typeof exports.exact_plan !== 'function' || typeof exports.exact_out !== 'function' || !(exports.memory instanceof WebAssembly.Memory)) {
  throw new Error('the web wasm does not export exact_plan, exact_out, and memory');
}
const planLen = exports.exact_plan();
const planPtr = exports.exact_out();
planBytes = Buffer.from(new Uint8Array(exports.memory.buffer, planPtr, planLen));
}
if (planBytes.length < 36 || planBytes.subarray(0, 4).toString() !== 'EXPL') throw new Error('the web wasm returned an invalid baked plan');
writeFileSync(planOut, planBytes);
// A development build's source map for that plan, for the agent driver only
// (LLP 1012.001.000 D6), as the JS target writes it: the bake's, and only
// when it maps these bytes; never in a production build, never a stale one.
const planMap = resolve(buildEnv.EXACT_BAKE_OUTPUT, 'web-wasm32-unknown-unknown.plan.map.json');
if (!production && existsSync(planMap) && JSON.parse(readFileSync(planMap, 'utf8')).digest === createHash('sha256').update(planBytes).digest('hex')) copyFileSync(planMap, resolve(stage, 'app.plan.map.json'));
let pairedModule = null;
// A module client's build script emits paired artifacts beside its receipt.
// Extract the exact embedded receipt/JS rather than rebaking moving sources.
// Native bytecode is a separate download: it never executes in the browser.
// `--bake` reads them from the build script's output, which the wasm embeds.
const moduleInput = buildReceipt.binary.inputs.find(input => input.name === `generated:${crate}:wasm32-unknown-unknown/app.module.json`);
if (bakeOnly ? !!moduleInput : typeof exports.exact_module_artifact === 'function') {
  const files = new Map([['app.plan', planBytes]]);
  for (const [index, name] of ['app.module.json', 'app.js'].entries()) {
    // The length first: the call can grow memory, detaching an earlier view of it.
    const len = bakeOnly ? 0 : exports.exact_module_artifact(index);
    const body = bakeOnly ? readFileSync(resolve(moduleInput.path, '..', name)) : Buffer.from(new Uint8Array(exports.memory.buffer, exports.exact_out(), len));
    files.set(name, body); writeFileSync(resolve(stage, name), body);
  }
  if (!moduleInput) throw new Error('the build receipt does not name the paired module output');
  const bytecode = readFileSync(resolve(moduleInput.path, '..', 'app.hbc'));
  files.set('app.hbc', bytecode);
  // moduleCards checks every byte against the receipt embedded in this wasm,
  // including the native digest/version. A concurrent rebake cannot mix pairs.
  pairedModule = Object.fromEntries(Object.entries(moduleCards(files, app.id)).map(([key, card]) => [key, { ...card, url: './' + MODULE_FILES[key] }]));
  writeFileSync(resolve(stage, 'app.hbc'), bytecode);
  copyHostFiles('module');
  // Remove the module-glue → storage → fs/sqlite request chain's middle
  // step. Keep the stateful adapters as shared modules: Rust requests also
  // import them, and must share the same filesystem mutation queues, and
  // the pickers' handles live in the page's one documents-glue.js.
  const bundle = await rolldown({ input: resolve(root, 'host/web/module-glue.js'), platform: 'browser',
    external: ['./storage-fs.js', './storage-sqlite.js', './documents-glue.js'], plugins: [{ name: 'agent-gate', transform: (code, id) => ({ code: gateAgent(code, id) }) }] });
  try { await bundle.write({ file: resolve(stage, 'module-glue.js'), format: 'es', minify: true }); }
  finally { await bundle.close(); }
}
const bakedReceipt = readBake(app, 'web', 'wasm32-unknown-unknown', buildEnv.EXACT_BAKE_OUTPUT);
writeFileSync(resolve(stage, 'app.version.json'), JSON.stringify({ version: bakedReceipt.embedded.version, buildNumber: bakedReceipt.embedded.buildNumber }) + '\n');
if (!bakeOnly) {
  if (typeof exports.exact_compat !== 'function') throw new Error('the web wasm exposes no baked receipt');
  const compatLen = exports.exact_compat();
  const embeddedCompat = Buffer.from(new Uint8Array(exports.memory.buffer, exports.exact_out(), compatLen)).toString('utf8');
  if (JSON.stringify(JSON.parse(embeddedCompat)) !== JSON.stringify(bakedReceipt)) throw new Error('the emitted receipt differs from the wasm receipt');
}
// Storage is an app capability, including apps whose only logic is Rust.
if (pairedModule || /^\s*(?:fs\.|sqlite\.)/m.test(bakedReceipt.inputs.grantCeiling ?? '')) {
  copyHostFiles('storage');
}
copyStaticTreeIfPresent(resolve(buildEnv.EXACT_BAKE_OUTPUT, 'web-wasm32-unknown-unknown.entries/plans'), resolve(stage, 'plans'));
copyStaticTreeIfPresent(resolve(buildEnv.EXACT_BAKE_OUTPUT, 'web-wasm32-unknown-unknown.entries/activities'), resolve(stage, 'activities'));
const copiedAssets = listAssets(stage);
verifyBakeFiles(bakedReceipt, planBytes, copiedAssets);
let pairedRust = null;
if (rustPackage(app) && rustPolicy(app.manifest, 'web', buildEnv.EXACT_UPDATE_TRUST === 'production' ? 'prod' : 'dev') !== 'off') {
  const built = await buildRust(app, { compat: bakedReceipt, env: buildEnv, plan: planBytes, profile: buildEnv.EXACT_UPDATE_TRUST === 'production' ? 'release' : 'logic-dev' });
  const files = rustFiles(built);
  for (const [name, bytes] of files) { mkdirSync(resolve(stage, name, '..'), {recursive:true}); writeFileSync(resolve(stage, name), bytes); }
  pairedRust = rustCards(files);
  copyHostFiles('rust');
}
writeFileSync(resolve(stage, 'bake.json'), JSON.stringify(buildReceipt) + '\n');
const sealed = sealEnvelope({ ...webEnvelope(app, planBytes, copiedAssets), ...(pairedModule ? { module: pairedModule } : {}), ...(pairedRust ? { rust: pairedRust } : {}) });
if (sealed.index) writeFileSync(resolve(stage, 'assets.json'), sealed.index);
writeFileSync(resolve(stage, 'exact.json'), sealed.bytes);
// The web app manifest (LLP 1030 D2/D10; 1030.000 D7): the W3C keys of
// `app.json`, copied out as `manifest.json`; the page links it, takes its
// name as the title, and its favicons and Apple touch icon from its icons
// (LLP 1163 D7). An installed PWA's icon and name are the browser's cached
// copies of these — the origin's carrier, at its real strength.
const manifestNotes = [];
const webManifest = webAppManifest(app.manifest, app.displayName, manifestNotes, app.dir);
for (const note of manifestNotes) console.log(note);
writeFileSync(resolve(stage, 'manifest.json'), JSON.stringify(webManifest, null, 2) + '\n');
const sourceRevision = spawnSync('git', ['rev-parse', '--short=10', 'HEAD'], {cwd:app.dir,encoding:'utf8'});
const sourceChanges = spawnSync('git', ['status', '--porcelain'], {cwd:app.dir,encoding:'utf8'});
writeInstallPages(stage, app.manifest, {id:buildReceipt.binary.sha256, source:sourceRevision.status === 0 ? sourceRevision.stdout.trim() : null, dirty:sourceChanges.status === 0 && !!sourceChanges.stdout.trim(), builtAt:new Date().toISOString(), mode:buildEnv.EXACT_UPDATE_TRUST === 'production' ? 'Release build' : 'Development build', reach:bakedReceipt.reach?.rows ?? []});
const escapeHtml = (t) => String(t).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');
// The shell's <style> ships without its comments and indentation: they are
// for readers of host/web/index.html, and every served document's first bytes
// carry the shell's head (LLP 1048.000 D3). Strings stay whole; whitespace
// goes only where CSS never reads it (never before a `:`, which in a selector
// is a descendant's pseudo-class).
function minifyCss(css) {
  let out = '', space = false;
  const put = (text) => {
    if (space && out && !'{};,:'.includes(out.at(-1)) && !'{};,!'.includes(text[0])) out += ' ';
    space = false;
    if (text === '}' && out.endsWith(';')) out = out.slice(0, -1);
    out += text;
  };
  for (let i = 0; i < css.length;) {
    if (css.startsWith('/*', i)) { const end = css.indexOf('*/', i + 2); i = end < 0 ? css.length : end + 2; space = true; continue; }
    const c = css[i];
    if (/\s/.test(c)) { space = true; i++; continue; }
    if (c === '"' || c === "'") { let j = i + 1; while (j < css.length && css[j] !== c) j += css[j] === '\\' ? 2 : 1; put(css.slice(i, j + 1)); i = j + 1; continue; }
    put(c); i++;
  }
  return out;
}
// The app's module artifact answers `native.later` on the page too (LLP
// 1067 D5): the glue finds it by the meta tag below.
const pageNative = app.modules.web;
// The page in the app's first-frame background from its first paint (the
// manifest's background colours, as the iOS launch screen), so nothing lighter or
// darker shows before the first frame. Only until then: from the first frame
// the app paints its own, and the canvas beyond it is the browser's for the
// page's colour scheme, as on the JS target (a launch white kept under an
// app that chose dark showed beside its root; Markdown's conformance).
// A fixed `userInterfaceStyle` takes its side alone, as the iOS launch screen does.
const hex = (value) => /^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(value ?? '') ? value : null;
const launchLight = app.manifest.background_color, launchDark = interfaceStyle(app.manifest) === 'light' ? null : hex(app.manifest.color_scheme_dark?.background_color);
const launchHtml = 'html:has(#exact-root:empty)';
const launchCss = !hex(launchLight) ? '' : interfaceStyle(app.manifest) === 'dark' ? `${launchHtml}{background-color:${launchDark ?? launchLight}}`
  : `${launchHtml}{background-color:${launchLight}}${launchDark ? `@media (prefers-color-scheme:dark){${launchHtml}{background-color:${launchDark}}}` : ''}`;
writeFileSync(resolve(stage, 'index.html'), readFileSync(resolve(stage, 'index.html'), 'utf8')
  .replace(/<style>([\s\S]*?)<\/style>/, (_, css) => `<style>${minifyCss(css)}${launchCss}</style>`)
  .replace('<html lang="en">', `<html lang="${escapeHtml(webManifest.lang)}">`)
  .replace('<title>Exact</title>', `<title>${escapeHtml(webManifest.name)}</title>`)
  // The manifest's favicons and Apple touch icon, else the shell's default favicon (LLP 1163 D7).
  .replace(/<link rel="icon"[^>]*>/, (link) => webIconLinks(app.manifest, app.dir, link))
  // `app.json`'s `audio_session` (LLP 1096 D7), applied by a separate page entry before the runtime, and
  // its `host.web.head` before the root, after the sheet, as the JS target writes it (LLP 1048.000 D3).
  .replace('<div id="exact-root"></div>', `${webHead(app.manifest)}${app.manifest.audio_session ? `<div id="exact-root" data-audio-session="${escapeHtml(app.manifest.audio_session)}"></div>` : '<div id="exact-root"></div>'}${webAudioSession(app.manifest)}`)
  .replace(
    '<script type="module" src="./glue.js"></script>',
    `<link rel="alternate" type="application/vnd.exact.envelope+json" href="./exact.json">\n<link rel="manifest" href="./manifest.json">\n${webThemeColors(app.manifest).map((meta) => `${meta}\n`).join('')}${pageNative ? '<meta name="exact-native" content="./modules/index.js">\n' : ''}<script type="module" src="./glue.js"></script>`,
  ));
// The app's part of the render server's CSP (LLP 1048.000 D11), which it reads at start.
const cspFile = webCspFile(app.manifest);
if (cspFile) {
  mkdirSync(resolve(stage, '.gen'), { recursive: true });
  writeFileSync(resolve(stage, '.gen', 'csp.json'), cspFile);
}
// app.wasm's URL names its build (LLP 1047.000 §9): `./app.wasm?v=` and
// the first 16 hex digits of its SHA-256, in the shell's preload, which the
// capture script and the glue fetch (and a document's checkpoint, when it
// drops the preload). A URL that names its content caches for good, so a
// browser keeps the build as the dictionary the render server sends the next
// one against (`--generations`). The file keeps its name, and the glue stays
// the same across builds: a deploy that changes only the wasm leaves it cached.
if (!bakeOnly) {
  const wasmUrl = `./app.wasm?v=${createHash('sha256').update(readFileSync(out)).digest('hex').slice(0, 16)}`;
  const shellText = readFileSync(resolve(stage, 'index.html'), 'utf8');
  if (!shellText.includes('href="./app.wasm"')) throw new Error('index.html no longer preloads ./app.wasm');
  writeFileSync(resolve(stage, 'index.html'), shellText.replace('href="./app.wasm"', `href="${wasmUrl}"`));
}
// Documents (LLP 1048.000 D3, D7, D9): every route the plan declares
// `render=build`, rendered by the app's native render entry (`<app>-render`,
// exact_render::main; looked up in its Linux crate beside its native data
// source, then in its web crate)
// from the plan the wasm carries, each a whole page composed over this
// shell (exact_render::page): the renderer's <head>, the document in
// #exact-root, its checkpoint; an idle page preloads its wasm and
// navigation.js with the document, an interaction page nothing. Then
// 404.html, sitemap.xml (absolute, against the manifest's origin),
// robots.txt, and the shell itself as shell.html — what a client route is
// served, and what the render server composes documents over.
const renderBin = crate.replace(/-web$/, '-render');
const renderAt = [['linux', app.crate('linux')], ['web', crate]]
  .find(([dir]) => existsSync(resolve(app.dir, dir, 'src/bin', `${renderBin}.rs`)));
const renderCrate = renderAt?.[1];
// Pay for what you use: an app that renders nothing at build builds and runs
// no render entry (the wasm says which locations it renders, from its plan).
// `--bake` has no wasm to ask: the entry, where there is one, renders what the plan declares.
const locationsLen = typeof exports.exact_build_locations === 'function' ? exports.exact_build_locations() : null;
const buildLocations = bakeOnly ? (renderAt ? ['the plan\'s'] : []) : locationsLen === null ? [] : JSON.parse(Buffer.from(new Uint8Array(exports.memory.buffer, exports.exact_out(), locationsLen)).toString('utf8'));
if (buildLocations.error) throw new Error(`the plan's render=build routes: ${buildLocations.error}`);
let documentNote = buildLocations.length ? `${buildLocations.length} declared, but no ${renderBin} entry in ${app.crate('linux')} or ${crate}` : 'none declared';
if (buildLocations.length && renderAt) {
  // A build-time tool, never shipped: it renders the plan it is handed, so
  // its own Linux bake takes development trust (a production bake would
  // demand a publisher receipt for an updater nothing publishes).
  // With what its bake watches named (`bakeWatchEnv`), as every baking build: unset, the dev loop's every rebuild
  // reran its build script and relinked it.
  const renderEnv = { ...buildEnv, ...bakeWatchEnv(app), CARGO_TARGET_DIR: app.target, EXACT_UPDATE_TRUST: 'development' };
  delete renderEnv.EXACT_BAKE_OUTPUT;
  const rendered = spawnSync('cargo', ['run', '-q', '-p', renderCrate, '--bin', renderBin, '--', '--plan', planOut,
    '--name', webManifest.name, ...(app.origin ? ['--origin', app.origin] : []), '--shell', resolve(stage, 'index.html'), '--build'],
  { cwd: app.dir, env: renderEnv, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
  if (rendered.status !== 0) throw new Error(`${renderBin}: ${rendered.stderr}${rendered.stdout}`);
  const shell = readFileSync(resolve(stage, 'index.html'), 'utf8');
  const pages = rendered.stdout.split('\n').filter(Boolean).map((line) => JSON.parse(line));
  if (pages.length) writeFileSync(resolve(stage, 'shell.html'), shell);
  for (const doc of pages) {
    if (doc.error) throw new Error(`${renderBin} ${doc.location}: ${doc.error}`);
    const html = doc.page;
    const file = doc.notfound ? '404.html' : `${decodeURIComponent(doc.location).replace(/^\/|\/$/g, '')}/index.html`.replace(/^\//, '');
    if (!resolve(stage, file).startsWith(stage + '/')) throw new Error(`${renderBin}: location ${doc.location} leaves dist`);
    mkdirSync(resolve(stage, file, '..'), { recursive: true });
    writeFileSync(resolve(stage, file), html);
  }
  if (pages.length) writeCrawlerFiles(stage, pages, app.origin, planRouteTable(planBytes));
  // A render that reached its deadline still ships: its placeholders show
  // until the runtime asks what was pending (LLP 1048.000 D9).
  const late = pages.filter((doc) => doc.settled === false).map((doc) => doc.location);
  documentNote = `${pages.length} document${pages.length === 1 ? '' : 's'}${pages.length && !app.origin ? ' (no origin: no sitemap)' : ''}${late.length ? `; at the deadline, with placeholders: ${late.join(', ')}` : ''}`;
}
// The deep-link association file (LLP 1030 D1; 1030.000 D2): generated from
// the manifest when the iOS host claims the domain and names its team; a
// static origin file Apple's CDN fetches, never a dev-server claim.
const ios = app.manifest.host?.ios ?? {};
// @ref LLP 1069.006 D2 — a claimed https auth callback on Apple needs this
// origin's `webcredentials` entry too (not `applinks:`, which stays routing).
const authReach = bakedReceipt.reach?.auth ?? {};
// @ref LLP 1133.011 D6 — an `auth.passkey` RP ID that is the origin's host: this origin serves both platforms' files.
const originHost = app.origin ? new URL(app.origin).hostname : null;
const passkeyHere = (authReach.passkeys ?? []).includes(originHost);
const webcredentials = passkeyHere || (authReach.callbacks ?? []).some((c) => app.origin && c.startsWith(new URL(app.origin).origin + '/'));
writeAppleAssociation(stage, app, { webcredentials });
// Android's statement (LLP 1133.011 D6): the package may use this domain's passkeys, for each certificate the app
// lists. Every other RP ID's two entries are printed, for the domain that serves it.
const certificates = app.manifest.host?.android?.certificates ?? [];
const androidStatement = { relation: ['delegate_permission/common.get_login_creds'], target: { namespace: 'android_app', package_name: app.id, sha256_cert_fingerprints: certificates } };
if (passkeyHere && certificates.length) {
  mkdirSync(resolve(stage, '.well-known'), { recursive: true });
  writeFileSync(resolve(stage, '.well-known/assetlinks.json'), JSON.stringify([androidStatement], null, 2) + '\n');
}
const elsewhere = (authReach.passkeys ?? []).filter((rp) => rp !== originHost && rp !== 'localhost');
const passkeyNote = (authReach.passkeys ?? []).length ? `; passkeys: ${[
  passkeyHere ? `${originHost}'s association files${ios.team ? '' : ' (no host.ios.team: no apple-app-site-association)'}${certificates.length ? '' : ' (no host.android.certificates: no assetlinks.json)'}` : null,
  ...elsewhere.map((rp) => `${rp} must serve /.well-known/apple-app-site-association {"webcredentials":{"apps":["${ios.team ?? '<team>'}.${app.id}"]}} and /.well-known/assetlinks.json ${JSON.stringify([androidStatement])}`),
].filter(Boolean).join('; ')}` : '';

// The web's auth callback page and the two client-metadata documents
// (LLP 1069.006 D4; after review, item 3: one client id per
// `application_type`, since the AT Protocol's metadata names one type and
// a `web` client's redirect URIs are https only).
let authNote = '';
if (authReach.sessions) {
  mkdirSync(resolve(stage, '.exact/auth'), { recursive: true });
  copyFileSync(resolve(root, 'host/web/auth-callback.html'), resolve(stage, '.exact/auth/callback'));
  copyFileSync(resolve(root, 'host/web/auth-callback.js'), resolve(stage, '.exact/auth/callback.js'));
  const docs = authClientMetadata(app, authReach.callbacks ?? []);
  for (const [name, doc] of Object.entries(docs)) writeFileSync(resolve(stage, `.exact/auth/${name}.json`), JSON.stringify(doc, null, 2) + '\n');
  authNote = `; auth: callback page${Object.keys(docs).length ? `, client metadata (${Object.keys(docs).join(', ')})` : ' (no origin: no client metadata)'}`;
}

// The app's GPU module (LLP 1009 D2): a second wasm the page fetches on
// demand, built with wasm-bindgen's glue (its exports are the module's ABI on
// the web) and wasm-opt. Only when the app has a GPU crate.
// Each declared GPU module (LLP 1009 D6) is its own wasm under gpu/, fetched
// the first time a canvas of one of its surfaces mounts.
const gpu = bakeOnly ? { note: 'none (--bake)', built: false } : webGpuArtifacts(app, stage, {profile:gpuProfile});
let gpuNote = gpu.note;
if (gpu.built) {
  copyHostFiles('gpu');
  if (gpuModules(app.manifest).length) copyHostFiles('gpuModules');
  gpuNote += ', on demand';
  // A game's world surfaces, by name, as its GPU build declared them (`.shells/surfaces.json`): a browser without
  // WebGPU shows the refusal page over these and no other canvas (gpu-refusal.js, LLP 1139.003 D1).
  if (game) writeFileSync(resolve(stage, 'index.html'), markWorlds(readFileSync(resolve(stage, 'index.html'), 'utf8'), worldSurfaces(app.dir)));
}
// @ref LLP 1024 D3/D5 — the app's module table, beside the page only when the
// app has modules (the GPU gate): its web executor under `modules/`, fetched
// after first paint by the host's adapter.
let moduleNote = 'no native modules';
if (app.modules.tags.length || app.modules.web) {
  const release = buildEnv.EXACT_UPDATE_TRUST === 'production';
  let provided = [];
  if (app.modules.web) {
    copyStaticTreeIfPresent(dirname(app.modules.web), resolve(stage, 'modules'));
    provided = Object.keys((await import(app.modules.web)).roster ?? {});
  }
  checkModuleRoster(app, provided, 'web', release);
  copyHostFiles('native');
  moduleNote = `modules/ (${provided.join(', ') || 'none'}), on demand`;
}
// Written last inside the private stage. Dev startup trusts a dist only when
// this marker and the public plan card agree, so a partial/corrupt directory
// can never be mistaken for a completed build of the requested app.
completeWebInputs(app, false, render[1] ?? 'rust', inputRoots, inputs, inputsAt);
writeFileSync(resolve(stage, '.exact-build.json'), JSON.stringify({
  exactBuild: 1, app: { id: app.id, name: app.displayName }, manifestSha256: appManifestDigest(app),
  files: await publicFileCards(stage).finally(closeFilesystemReader),
  // Content digests of what this build read: a staleness check compares bytes, not times.
  inputs, inputsAt, inputRoots,
  // Each setting its module inlined (LLP 1153 D4), from the paired module's receipt, as a JS build's marker says them.
  ...builtSettings(stage),
}) + '\n');
// In place in one step where the system has one (serve.mjs `replaceBuild`).
publishWarmGame(app, stage, dist, warmInputs, buildReceipt, {...buildEnv, EXACT_GAME_PROOF_PROFILE:proofProfile});
stage = null;
if (bakeOnly && process.argv.includes('--json')) reportBuilt({ path: dist, target: 'web', trust: buildEnv.EXACT_UPDATE_TRUST });
if (bakeOnly) { console.log(`${relative(process.cwd(), dist) || "."}: the bake (no wasm): app.plan ${kib(planBytes.length)}, ${sealed.report}, index.html; documents: ${documentNote}${authNote}${passkeyNote}; modules: ${moduleNote}`); process.exit(0); }
const textFlowWasm = readFileSync(resolve(dist, 'textflow.wasm'));
const markdownEditor = readFileSync(resolve(dist, 'markup-editor.wasm'));
console.log(`${relative(process.cwd(), dist) || "."}: app.wasm ${kib(wasm.length)} (${kib(gzipSync(wasm, { level: 9 }).length)} gzip; ${optNote}), index.html, glue.js, app.plan ${kib(planBytes.length)}, ${sealed.report}; documents: ${documentNote}${authNote}${passkeyNote}; GPU: ${gpuNote}; modules: ${moduleNote}; markup-editor.wasm ${kib(markdownEditor.length)} (${kib(gzipSync(markdownEditor, { level: 9 }).length)} gzip), on demand; textflow.wasm ${kib(textFlowWasm.length)} (${kib(gzipSync(textFlowWasm, { level: 9 }).length)} gzip), on demand`);
if (process.argv.includes('--json')) reportBuilt({ path: dist, target: 'web', trust: buildEnv.EXACT_UPDATE_TRUST });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
