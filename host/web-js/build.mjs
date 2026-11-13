import { asyncContext, bundleOptions, developmentLocalNetwork, lowered, packageConfig, webImplementations } from '../../js/bake/src/typescript.mjs';
import { writeCrawlerFiles } from '../web/crawl.mjs';
import { bakeWatchEnv, hatchWords, injectedProfiles, moduleDirectory, resolveApp, webAppManifest } from '../../scripts/app.mjs';
import { webIconLinks, webThemeColors } from '../../scripts/app-icons.mjs';
import { webCspFile, webHead } from '../../scripts/web-head.mjs';
import { fileIdentity, resourceIdentities } from '../apple/assets.mjs';
import { launchModules } from '../../scripts/launch-modules.mjs';
import { installedFileDependencies } from '../../scripts/file-dependencies.mjs';
// The web build's JS target: `bun host/web-js/build.mjs <app> [--plan <baked app.plan>] [--out <dir>]`.
//
// 1. `exact-web-js js` compiles the plan (the app's Contract, or a baked
//    `app.plan` from `host/web/build.mjs`, whose resources carry their
//    build-time answers) to `app.js` + `app.css`.
// 2. Rolldown joins it with `rt.js`, tree-shaken and minified: one
//    module, everything needed to be interactive.
// 3. `index.html` carries the web host's own base stylesheet (from
//    `host/web/index.html`), the app's static classes, and the module, with a
//    `modulepreload` in the head for every emitted chunk, so a served page
//    (host/render/src/page.rs `page_js`) fetches its runtime while the document
//    streams, including pieces it may first use offline.
// A data module loaded after first pixel (`rust-data.js`) and the agent
// adapter (`agent.js`, only under `?agent`) are separate files.
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, normalize, posix, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { transformSync, parseSync, Visitor } from 'rolldown/utils';
import { copyGpuCache } from './gpu-cache.mjs';
import { analyze } from './page-style.mjs';
import { bundleHostModule, gateAgent, localImports, stageHostGlue } from './glue.mjs';
import { buildEditor, buildFlow, buildMarkdown, buildModule, buildMotion, fresh, moduleGrants, webCompiler, withInspectHooks, withoutDev, withRuntimeStamp } from './module.mjs';
import { developmentStore } from '../../scripts/agent-launch.mjs';
import { settingLines } from '../../js/bake/src/__exact_env.mjs';
import { colorSchemeMeta, interfaceStyle, withInterfaceStyle, withLook } from '../web/look.mjs';
import { beside as besideDist } from '../web/beside.mjs';
import { copyStaticTreeIfPresent } from '../web/serve.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const args = process.argv.slice(2);
// A path names the app as EXACT_APP_DIR does (scripts/app.mjs resolveApp), so
// what is made from the name (the module crate under web-js-modules, the
// default --out) stays out of the app's own directory.
const given = args[0] && /[\\/]|^\.\.?$/.test(args[0]) ? resolve(args[0]) : null;
if (given) process.env.EXACT_APP_DIR = given;
const app = given ? basename(given) : args[0];
const opt = (name) => { const i = args.indexOf(name); return i < 0 ? null : args[i + 1]; };
if (!app) { console.error('usage: bun host/web-js/build.mjs <app> [--plan <app.plan> | --contract <file>] [--out <dir>] [--inline] [--render rust|js|none] [--check]'); process.exit(2); }
// An app outside this repo is where `EXACT_APP_DIR` says (scripts/app.mjs).
const appDir = process.env.EXACT_APP_DIR ? resolve(process.env.EXACT_APP_DIR) : resolve(root, 'apps', app);
// Before anything reads a package: the dev loop runs this build directly, not through resolveApp (#115).
try { installedFileDependencies(appDir); } catch (error) { console.error(`error: ${error.message}`); process.exit(1); }
const out = resolve(opt('--out') ?? `/tmp/exact-web-js-dist/${app}`);
// Bun's runtime transpiler cache keys a module by its text and keeps the
// imports this build's mount resolver gives it (below), so the same file at
// another place (a second checkout, a moved mount) would resolve back into
// the first. A build through mounts runs without that cache: it starts again
// that way before it touches anything.
if (process.env.BUN_RUNTIME_TRANSPILER_CACHE_PATH !== '0') {
  let mounts = {};
  try { mounts = JSON.parse(readFileSync(resolve(appDir, 'app.json'), 'utf8')).typescript?.sources ?? {}; } catch {}
  if (Object.keys(mounts).length) {
    const again = spawnSync(process.execPath, process.argv.slice(1), { stdio: 'inherit', env: { ...process.env, BUN_RUNTIME_TRANSPILER_CACHE_PATH: '0' } });
    process.exit(again.status ?? 1);
  }
}
// `--production` (delivery, scripts/deploy.mjs): a release, agent mode
// refused. Over a wasm bake's baked plan (`--plan <dist>/app.plan`), the
// bake's origin files (the envelope, the web manifest, install and auth
// pages, sitemap) and head links are carried, so the web root it publishes
// is the wasm root's in everything but the program (LLP 1071 §7, delivery).
// With no bake (an app that names no native platform, LLP 1153 D3.6), it
// builds the app's own Contract, links its web manifest, and has no envelope.
const production = args.includes('--production');
// `--dev` (host/web-js/dev.mjs): typed state checkpoint hooks that ordinary
// and production builds neither emit nor link.
const devReload = args.includes('--dev');
if (production && devReload) { console.error('--production and --dev are mutually exclusive'); process.exit(2); }
// Its app.ts copy loses a development build's only grant lines (`configure`), which reads the trust.
if (production && (process.env.EXACT_UPDATE_TRUST ?? 'production') !== 'production') { console.error(`--production builds under EXACT_UPDATE_TRUST=production, not ${process.env.EXACT_UPDATE_TRUST}`); process.exit(2); }
if (production) process.env.EXACT_UPDATE_TRUST = 'production';
// `--check` (`exact.mjs contract build app.contract`): app.ts's type check alone, saying nothing else.
const checkOnly = args.includes('--check');
// The source revision a development build's install pages name, read while
// it builds: optional, so no git, no repository, or a git held past 5 s (a
// held index lock) leaves it unknown rather than failing or hanging the build.
const git = (...a) => new Promise((done) => {
  let text = '', c;
  try { c = spawn('git', a, { cwd: appDir, stdio: ['ignore', 'pipe', 'ignore'] }); } catch { return done(null); }
  const timer = setTimeout(() => { c.kill('SIGKILL'); done(null); }, 5000);
  c.on('error', () => { clearTimeout(timer); done(null); });
  c.on('close', (code) => { clearTimeout(timer); done(code === 0 ? text.trim() : null); });
  c.stdout?.on('data', (d) => { text += d; });
});
const revision = production ? null : Promise.all([git('rev-parse', '--short=10', 'HEAD'), git('status', '--porcelain')]);
const gen = resolve(out, '.gen');
rmSync(out, { recursive: true, force: true });
mkdirSync(gen, { recursive: true });

const input = opt('--plan') ?? opt('--contract') ?? resolve(appDir, 'app.contract');
const manifest = JSON.parse(readFileSync(resolve(appDir, 'app.json'), 'utf8'));
// documents-glue reads the app's web manifest lazily. The JS path exits from
// host/web/build.mjs after this builder succeeds, so it owns the same artifact.
const manifestNotes = [];
const webManifest = webAppManifest(manifest, manifest.app?.name ?? manifest.name, manifestNotes, appDir);
for (const note of manifestNotes) console.log(note);
writeFileSync(resolve(out, 'manifest.json'), JSON.stringify(webManifest, null, 2) + '\n');
// Rust data: the app's own module (`rust.module`), or a module generated
// from the DataSource its web build bakes with (host/web-js/module.mjs).
const bakes = existsSync(resolve(appDir, 'web/build.rs')) && /contract::bake\(\s*plan,/.test(readFileSync(resolve(appDir, 'web/build.rs'), 'utf8'));
const rust = !!manifest.rust?.module || bakes;
// A TypeScript source (`app.ts`) runs in the page, bundled with it; beside a
// Rust one (LLP 1027.002), the Rust module answers what it owns and the
// TypeScript module the rest (ts-data.js, rust-data.js). A synthetic plan
// over another app's sources (`--data`) asks the Rust module only.
const ts = existsSync(resolve(appDir, 'app.ts')) && !(rust && opt('--data'));
const mixed = rust && ts;
const appTs = resolve(appDir, 'app.ts');
// The manifest's `typescript.sources` mounts (js/bake/src/lib.rs `mounts`):
// `./<name>/…` from the app's own modules resolves into that directory in
// every load this build makes (the module read below, the server and page
// bundles), as the type check's capture lays it out.
// A mount is relative to the app's directory, where app.json is; one that is not there is named with the directory it
// was resolved from, and this checkout's Snapback driver offered when that is what it names (app farm wave 1: five
// builds miscounted the `../`, and one built the device's wasm outside the app).
function mountDir(name, path) {
  try { return realpathSync(resolve(appDir, path)); } catch (e) {
    const driver = /(^|\/)snapback4\/ts\/?$/.test(path) ? `; this checkout's Snapback driver is "${relative(appDir, resolve(root, 'snapback4/ts'))}" from there` : '';
    console.error(`error: app.json typescript.sources.${name}: "${path}" is resolved from the app's directory, ${appDir}, to ${resolve(appDir, path)}, which ${e.code === 'ENOENT' ? 'does not exist' : `cannot be read (${e.code ?? e.message})`}${driver}`);
    process.exit(1);
  }
}
const tsMounts = Object.entries(manifest.typescript?.sources ?? {}).map(([name, path]) => [name, mountDir(name, path)]);
// A checkout without the Snapback client has no driver for an app to mount.
const snapbackDriver = existsSync(resolve(root, 'snapback4/ts')) ? realpathSync(resolve(root, 'snapback4/ts')) : null;
if (tsMounts.some(([, dir]) => dir === snapbackDriver) && !existsSync(resolve(appDir, 'assets/snapback4.wasm'))) {
  console.error(`warning: the app mounts Snapback's driver, and assets/snapback4.wasm, the device it loads by default, is missing (the page will fail with HTTP 404 for /assets/snapback4.wasm). From the app's directory: node ${resolve(root, 'snapback4/web/build.mjs')} assets/snapback4.wasm`);
}
// As the bake refuses them (js/bake/src/lib.rs `mounts`): a file reachable
// through two mounts would have two places in the layout.
for (const [i, [a, aDir]] of tsMounts.entries()) for (const [b, bDir] of tsMounts.slice(i + 1)) {
  if (aDir === bDir || aDir.startsWith(bDir + sep) || bDir.startsWith(aDir + sep)) {
    console.error(`error: typescript.sources.${a} and typescript.sources.${b} overlap; mount directories that do not contain each other`);
    process.exit(1);
  }
}
const appRoots = [...new Set([resolve(appDir), realpathSync(appDir)])];
const within = (path, dir) => path === dir || path.startsWith(dir + sep);
// Where a file sits in the captured layout (the app at the root, each mount
// under its name), and back: an import resolves there, from any importer.
const logical = path => {
  const mount = tsMounts.find(([, dir]) => within(path, dir));
  if (mount) return join(mount[0], relative(mount[1], path));
  const root = appRoots.find(dir => within(path, dir));
  return root === undefined ? null : relative(root, path);
};
const physical = path => {
  const [first, ...rest] = path.split(sep);
  const mount = tsMounts.find(([name]) => name === first);
  return mount ? resolve(mount[1], ...rest) : resolve(appRoots[0], path);
};
function mounted(spec, importer) {
  if (!tsMounts.length || !importer || !/^\.\.?\//.test(spec)) return null;
  const from = logical(dirname(importer));
  if (from === null) return null;
  const target = normalize(join(from, spec));
  if (target === '..' || target.startsWith('..' + sep) || isAbsolute(target)) return null;
  const base = physical(target);
  return [base, `${base}.ts`, `${base}.js`, resolve(base, 'index.ts')].find(path => statSync(path, { throwIfNoEntry: false })?.isFile()) ?? null;
}
const devLogic = [];
// Native modules (LLP 1024): the app's module artifact, `modules/web/` beside
// the page as `modules/`, with the web host's adapter (native.js).
const pageModules = existsSync(resolve(moduleDirectory(appDir, 'web'), 'index.js'));
// The page module's container, app and window hatches (LLP 1075.003.000 §3.7, 1075.003.000.001 §2.1): their glue loads
// only for a page module that exports one. Its exports are read by Bun's
// parser, never run: a browser module may touch the DOM as it loads. An
// `export *` may export one.
const pageSource = pageModules ? readFileSync(resolve(moduleDirectory(appDir, 'web'), 'index.js'), 'utf8') : '';
const pageExports = pageModules ? new Bun.Transpiler({ loader: 'js' }).scan(pageSource) : { exports: [], imports: [] };
const containerHatches = pageExports.exports.some(n => ['navigation', 'route', 'routeEnded', 'tabs', 'app', 'appEnded', 'window', 'windowEnded'].includes(n))
  || /\bexport\s*\*\s*from\b/.test(pageSource);
// The app's GPU modules, built (or taken from the cache) before the compiler
// runs: the surfaces the primary draws are what its compiled registry says
// (`gpu_surfaces`, exact_gpu::module!), as a native host looks a surface up
// in its registry. The compiler sends a canvas naming one of them to it, and
// any other to Canvas 2D, which a data source draws. A module declared in
// app.json (`gpu.modules`) is built too, but this runtime creates every GPU
// surface in the primary: a declared module's canvases need the modules
// router (host/web/gpu-modules.js), which only the wasm target loads, so the
// build refuses a page whose canvas names one of their surfaces (below). A
// check (`--check`) builds no GPU module: the routing changes none of the
// compiler's diagnostics, and the primary's surfaces stay unknown (null).
const gpuLib = resolve(appDir, 'gpu/src/lib.rs');
let gpuSurfaces = existsSync(gpuLib) ? null : [], declaredGpu = [];
if (existsSync(gpuLib) && !checkOnly) {
  const { gpuModules, resolveApp, webGpuArtifacts, copyShaders, shaderWatchRoots } = await import('../../scripts/app.mjs');
  const target = resolveApp(app), cache = resolve(target.target, 'web-js-gpu', target.name);
  declaredGpu = gpuModules(target.manifest);
  const crates = [['gpu', target.crate('gpu')], ...declaredGpu.map(({ name }) => [`gpu/${name}`, target.crate(`gpu-${name}`)])];
  const shaders = shaderWatchRoots(target).filter(existsSync);
  // Shared by every loop of the app on this target: built in a stage and renamed over the cache (gpu-cache.mjs).
  try {
    copyGpuCache(cache, out, {
      fresh: dir => crates.every(([stem, crate]) => fresh(resolve(dir, `${stem}_bg.wasm`), resolve(target.target, 'wasm32-unknown-unknown/web', `${crate.replace(/-/g, '_')}.d`), shaders)),
      build: stage => {
        const gpu = webGpuArtifacts(target, stage, { cargo: true });
        if (!gpu.built) throw new Error(gpu.note);
        copyShaders(target, resolve(stage, 'shaders'));
      },
    });
    // The primary module's registry, instantiated here only to read it: it asks for no device until `gpu_load`.
    const glue = await import(pathToFileURL(resolve(out, 'gpu.js')).href);
    glue.initSync({ module: readFileSync(resolve(out, 'gpu_bg.wasm')) });
    if (typeof glue.gpu_surfaces !== 'function') throw new Error(`${crates[0][1]}: its module exports no gpu_surfaces (exact_gpu::module!)`);
    gpuSurfaces = JSON.parse(glue.gpu_surfaces());
  } catch (error) { console.error(`${app}: ${error.message}`); process.exit(1); }
}
// The compiler, run as its built binary when nothing it was built from
// changed (module.mjs `fresh`; `cargo run`'s own check costs ~0.4 s an edit),
// or as the one another checkout of this machine built from these sources.
const compiler = webCompiler();
const cargo = spawnSync(compiler.cmd, [...compiler.pre, 'js', input, '-o', gen, '--app', appDir, ...(production ? [] : ['--sites']), ...(devReload ? ['--dev-reload'] : [])], { cwd: root, stdio: checkOnly ? ['ignore', 'ignore', 'pipe'] : 'inherit', encoding: 'utf8', env: { ...process.env, EXACT_JS_GPU_SURFACES: (gpuSurfaces ?? []).join(',') } });
if (cargo.status !== 0) { if (checkOnly) process.stderr.write(cargo.stderr ?? ''); process.exit(cargo.status ?? 1); }
compiler.done();
const runtime = ['rt.js', 'request-send.js', 'presence-bindings.js', 'rows.js', 'resource-inspection.js', 'rt-link.js', 'generation-reload.js', 'elements.js', 'editor.js', 'live-time.js', 'font-family.js', 'observers.js', 'grid.js', 'overlay.js', 'roster.js', 'router.js', 'route-pattern.js', 'schedule.js', 'each.js', 'timer-waits.js', 'timer-observations.js', 'budget.js', 'shape.js', 'pointer.js', 'document.js', 'media.js', 'links.js', 'commands.js', 'focus.js', 'backdrop.js', 'persist.js', 'chosen.js', 'modes.js', 'follow.js', 'beat.js', 'regions.js', 'said.js', 'held.js', 'uploads.js', 'lower.js'];
for (const f of runtime) cpSync(resolve(here, f === 'timer-observations.js' ? '../web/' + f : f), resolve(gen, f));
// The live table's runtime is the dev loop's build's alone.
if (!devReload) writeFileSync(resolve(gen, 'modes.js'), withoutDev(readFileSync(resolve(here, 'modes.js'), 'utf8')));
// Inspect mode's hooks in the runtime are the dev loop's build's alone (module.mjs `withInspectHooks`).
if (devReload) writeFileSync(resolve(gen, 'rt.js'), withInspectHooks(readFileSync(resolve(here, 'rt.js'), 'utf8')));
// The arms sheet's host stamp (lower.js, LLP 1071.001 part B): a digest of the runtime array above as copied (the mode
// and inspect transforms included), lower.js's own unstamped source among it; not the files copied after this.
{ const lower = readFileSync(resolve(here, 'lower.js'), 'utf8'); writeFileSync(resolve(gen, 'lower.js'), withRuntimeStamp(lower, runtime.map(f => ({ path: f, text: f === 'lower.js' ? lower : readFileSync(resolve(gen, f), 'utf8') })))); }
// Canvas 2D surfaces (a loaded chunk: this runtime's engine over the web
// host's own replayer) are drawn by the Rust data module, or by a
// TypeScript source's `draw` in the page (ts-draw.js, in the same chunk).
const canvas2d = existsSync(resolve(gen, 'canvas2d.flag'));
{
  const named = canvas2d ? readFileSync(resolve(gen, 'canvas2d.flag'), 'utf8').split('\n') : [];
  const owned = declaredGpu.flatMap(({ name, surfaces }) => surfaces.filter((s) => named.includes(s)).map((s) => `${s} (gpu.modules.${name})`));
  if (owned.length) { console.error(`${app}: the web's JS target does not run a declared GPU module's surfaces yet, only the gpu crate's, and a canvas names ${owned.join(', ')}`); process.exit(1); }
}
cpSync(resolve(here, 'canvas2d.js'), resolve(gen, 'canvas2d.js'));
// An image's pictures in the browser's floating window, loaded on first use (commands.js, LLP 1042 §9).
cpSync(resolve(here, 'picture-in-picture.js'), resolve(gen, 'picture-in-picture.js'));
// The checked capture of app.ts and what it imports (typecheck, below): the
// page and its server render bundle it, never the app's own directory.
const tsStage = resolve(realpathSync(gen), 'typescript'), pageAppTs = resolve(tsStage, 'app.ts');
// `exactTime`: the runner's reserved source (runner/src/time.rs), answered
// before the app's, as the web host tells the wasm runner (navigation.js).
// The whole map: its type strings hold braces, so a source sorted before it
// (`advance`, a nested shape) once hid `exactTime` and its app could not boot.
const sources = JSON.parse(readFileSync(resolve(gen, 'app.js'), 'utf8').match(/export const sources=(\{.*?\});export const wait=/)?.[1] ?? '{}');
const time = Object.hasOwn(sources, 'exactTime');
// A native module's names and extensions (modules.js, LLP 1133.002 D13): each
// extension's web implementation is a chunk of its own, `ext-<name>.js`, loaded
// outside the agent on first use; an extension with none answers by name.
const moduleFlag = existsSync(resolve(gen, 'modules.flag')) ? JSON.parse(readFileSync(resolve(gen, 'modules.flag'), 'utf8')) : { extensions: {}, sources: [] };
if (Object.keys(moduleFlag.extensions).length) {
  const bound = webImplementations(appDir, moduleFlag.extensions);
  // `app.json`'s package configuration (LLP 1133.021 §9 item 7): refused here when a required key is absent.
  let configs;
  try { configs = packageConfig(appDir, 'web', { extensions: moduleFlag.extensions }); } catch (e) { console.error(`${app}: ${e.message}`); process.exit(1); }
  for (const extension of Object.keys(moduleFlag.extensions)) {
    const at = bound.get(extension);
    // `roster`: the views it serves (views.js), where it exports one.
    const views = at && /\bexport\s+(const|let|var|function)\s+roster\b|\bexport\s*\{[^}]*\broster\b/.test(readFileSync(at, 'utf8'));
    const config = `export const config = ${JSON.stringify(configs.get(extension) ?? {})};\n`;
    writeFileSync(resolve(gen, `ext-${extension}.js`), at ? `export { implementation${views ? ', roster' : ''} } from ${JSON.stringify(at)};\n${config}` : 'export const implementation = null;\n');
  }
}
// A source a data module answers: not one the runtime reserves (`exact…`), nor a native module's.
const asks = Object.keys(sources).some(name => !/^exact[A-Z]/.test(name) && !moduleFlag.sources.includes(name));
// A file input, `saveFile` or `share` (files.js), registered before any press.
const files = existsSync(resolve(gen, 'files.flag'));
// A declared sound or a sound command (sounds.js, LLP 1096 D5): the flag is the plan's table.
const soundTable = existsSync(resolve(gen, 'sounds.flag')) ? readFileSync(resolve(gen, 'sounds.flag'), 'utf8') : null;
// `wakelock` on any element (wakelock.js, LLP 1125 D11).
const wakelock = existsSync(resolve(gen, 'wakelock.flag'));
// `orientationlock` on any element (orientationlock.js, LLP 1133.029 D7).
const orientationlock = existsSync(resolve(gen, 'orientationlock.flag'));
// A dialog, an alertdialog, a menu or a popover (prompt-glue.js, alert-message.js, menu-rows.js; LLP 1047.002 D1).
const dialogs = existsSync(resolve(gen, 'dialogs.flag'));
// App generation may create files imported by app.ts. Run it before reading
// the declaration, as the wasm build does.
const webScript = resolve(appDir, 'web/build.rs');
if (ts && existsSync(webScript) && !/^\s*fn main\(\)\s*\{\s*exact_js_bake::build\w*\(/m.test(readFileSync(webScript, 'utf8'))) {
  // With what its bake watches named, as every baking build is (`bakeWatchEnv`): unset, a check reruns it every time.
  // The workspace's lock is checked first, as every build's is, and `--locked` keeps the check from rewriting it (LLP 1153 D2.1).
  resolveApp(appDir).rust();
  const r = spawnSync('cargo', ['check', '-q', '--locked', '--manifest-path', resolve(appDir, 'web/Cargo.toml')], { cwd: root, stdio: 'inherit', env: { ...process.env, ...bakeWatchEnv({ dir: appDir, manifest }) } });
  if (r.status !== 0) { console.error(`${app}: its web build script failed`); process.exit(1); }
}
// `app.ts` is captured and type-checked as the native bake does it: the
// same configuration and check (js/bake/src/typescript.mjs) over the same
// capture and entry, against this plan's declarations, so an app.ts the web
// builds is one every host builds, refused with the same diagnostics (calc
// F2, calendar F9/F11). The type check runs while the page bundles; the
// build waits for it at the end. The capture is the module's one graph: its
// syntax answers appId and grants (`staticFacts`), its bundle the exports, and the page
// and server bundles replay its resolutions (`replayCaptured`): no app or
// package code runs in this process. Under `--data` (a conformance fixture's synthetic plan over
// another app's sources) it is captured but not checked: against the
// fixture's declarations every source that app answers is unknown.
// The check's errors, then its warnings: what `contract build` and a drive's summary read, warnings after refusals.
const sayWarnings = (warnings) => { for (const warning of warnings ?? []) console.warn(warning); };
const captured = ts ? await typecheck(!opt('--data')).catch((error) => {
  console.error(error.code === 'backend-selection' ? `error [backend-selection]: ${error.message}`
    : `${error.message ?? error}\nerror: ${app}: app.ts failed the type check every host's build runs`);
  sayWarnings(error.warnings);
  process.exit(1);
}) : null;
if (checkOnly) {
  const typeError = await captured?.typed;
  if (typeError) console.error(`${String(typeError.message ?? typeError).trimEnd()}\nerror: ${app}: app.ts failed the type check every host's build runs`);
  sayWarnings(captured?.warnings);
  process.exit(typeError ? 1 : 0);
}
// Each setting app.ts read, as inlined, and its source (`configure`; LLP 1153 D4).
if (ts) for (const line of settingLines(JSON.parse(readFileSync(resolve(tsStage, '__exact_env.json'), 'utf8')))) console.log(line);
async function typecheck(checking) {
  const { configure, check, capturedSources, staticFacts, valueExports, CODE_WARNINGS } = await import(resolve(root, 'js/bake/src/typescript.mjs'));
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const declarations = readFileSync(resolve(gen, 'app.contract.d.ts'), 'utf8');
  // Beside app.ts too, for an editor, as the native development bake writes
  // it: never a captured source, written only when it changes, and said in
  // one line, as `exact.mjs contract build` says it (LLP 1116.001 D4.5).
  const beside = resolve(appDir, 'app.contract.d.ts');
  const before = existsSync(beside) ? readFileSync(beside, 'utf8') : null;
  if (checking && !production && before !== declarations) {
    writeFileSync(beside, declarations);
    console.error(`app.contract.d.ts: ${before === null ? 'written' : 'regenerated, since a source\'s signature changed'} (the types app.ts imports)`);
  }
  // The bake's capture (js/bake/src/lib.rs `sources`): the app's TypeScript,
  // JavaScript and JSON, and each `typescript.sources` mount under its name.
  const stage = resolve(gen, 'typescript');
  // This build's own output may sit inside the app (`--out <app>/web-out`):
  // it is no source, and the stage is inside it (review r4a 2).
  const output = realpathSync(out);
  const mounts = Object.entries(manifest.typescript?.sources ?? {}).map(([name, path]) => [name, mountDir(name, path)]);
  // A declared resource tree is no source, known by identity (js/bake's `sources`, by the same rule).
  const resources = resourceIdentities(appDir, manifest);
  const capture = (from, to, top) => {
    for (const entry of readdirSync(from, { withFileTypes: true })) {
      const name = entry.name, path = resolve(from, name);
      if (resources.size && resources.has(fileIdentity(path))) continue;
      if (['.git', 'node_modules', 'target', 'dist'].includes(name) || name.startsWith('.exact-js-bake-') || (top && name === 'app.contract.d.ts')) continue;
      // The app's dot directories (`.exact/`: an agent's evidence, logs, runtime files) are no source, as in js/bake's capture.
      if (top && name.startsWith('.') && entry.isDirectory()) continue;
      // Nor is what a build in `dist` keeps beside it (`web-build --out dist`: serve.mjs `replaceBuild`, generations.mjs).
      if (top && besideDist(name)) continue;
      if (top && mounts.some(([mount]) => mount === name)) continue;
      // Native link inputs, fingerprinted by the Apple build, as in js/bake's capture.
      if (entry.isDirectory() && name.endsWith('.xcframework') && /^(modules|(?:ios|macos|tvos)\/modules)\//.test(relative(appDir, path).split(sep).join('/'))) continue;
      // Links are refused, except a document link outside the static trees
      // (CLAUDE.md → AGENTS.md), as js/bake's capture: no build reads one.
      if (entry.isSymbolicLink()) {
        const document = /\.(md|txt)$/.test(name) && !/^(assets|deck|shaders|gpu)$/.test(relative(appDir, from).split(/[\\/]/)[0]);
        if (!document || statSync(path, { throwIfNoEntry: false })?.isDirectory()) throw new Error(`source links are not captured: ${path}`);
        continue;
      }
      if (entry.isDirectory()) { if (realpathSync(path) !== output) capture(path, resolve(to, name), false); }
      else if (/\.(ts|[cm]?js|json)$/.test(name)) { mkdirSync(to, { recursive: true }); cpSync(path, resolve(to, name)); }
    }
  };
  capture(appDir, stage, true);
  for (const [name, dir] of mounts) capture(dir, resolve(stage, name), false);
  writeFileSync(resolve(stage, 'app.contract.d.ts'), declarations);
  // The bake's optional data exports (js/bake/src/lib.rs `OPTIONAL_ENTRY`).
  // The web host supplies its own Canvas 2D seam.
  const optional = "import type { Overlay } from './app.contract.d.ts';\ntype Optional<K extends string> = K extends keyof typeof app ? (typeof app)[K] : undefined;\nexport const overlay: Overlay | undefined = (app as { overlay?: unknown }).overlay as Optional<'overlay'>;\nexport const tools = (app as { tools?: unknown }).tools as Optional<'tools'>;\n";
  // The app's id is app.json's; app.ts need not name it, and one it names is checked against it below.
  const id = (await import('../../scripts/app.mjs')).readManifest(appDir, app).app.id;
  writeFileSync(resolve(stage, '__exact_entry.ts'), `import * as app from './app';\nimport type { Answer } from './app.contract.d.ts';\nexport const appId: string = (app as unknown as { appId?: string }).appId ?? ${JSON.stringify(id)};\nexport const grants: string = app.grants;\nexport const answer: Answer = app.answer;\n` + optional
    + "export const draw: ((surface: string, args: any, ctx: any, frame: any) => unknown) | undefined = (app as { draw?: unknown }).draw as Optional<'draw'>;\nexport const surfaces = (app as { surfaces?: unknown }).surfaces as Optional<'surfaces'>;\n");
  writeFileSync(resolve(stage, '__exact_paths.json'), JSON.stringify({ app: realpathSync(appDir), mounts, target: 'web' }));
  writeFileSync(resolve(stage,'__exact_backend_source.json'), JSON.stringify(readFileSync(resolve(stage,'app.ts'),'utf8')));
  const real = realpathSync(stage);
  if (real !== tsStage) throw new Error(`the TypeScript stage moved: ${real}`);
  configure(real, false, parseSync);
  const backendWarning=JSON.parse(readFileSync(resolve(real,'__exact_backend.json'),'utf8')).warning;
  if (backendWarning) console.warn(`warning: ${backendWarning}`);
  // The packages whose declarations the check reads (typescript.mjs
  // `stagePackages`) are inputs the dev loop watches too, as it watches the
  // Contract packages the compile read: a changed `.d.ts` checks app.ts again.
  if (devReload) {
    // Each package's own tree, and the entry it is installed as (a link
    // retargeted, or a reinstall, is an edit there).
    const read = JSON.parse(readFileSync(resolve(real, '__exact_declarations.json'), 'utf8')).packages;
    const listed = resolve(gen, 'dev-sources.json');
    const sources = existsSync(listed) ? JSON.parse(readFileSync(listed, 'utf8')) : {};
    sources.packages = [...new Set([...(sources.packages ?? []), ...read.map(([, , dir]) => dir)])];
    sources.shallow = [...(sources.shallow ?? []), ...read.map(([, installed]) => [dirname(installed), basename(installed)])];
    writeFileSync(listed, JSON.stringify(sources));
  }
  // The clock, randomness and timers, refused at build in the modules
  // app.ts reaches, as the native bake's bundler refuses them
  // (js/bake/src/typescript.mjs `ambientRefusals`). A graph that does not
  // bundle from the capture is refused, as the native bake refuses it.
  // Both run, and every diagnostic is reported, as the resident compiler joins them.
  const why = [];
  const { rolldown } = await import('rolldown');
  const typed = checking ? check(real, resolve(libraries, 'tsc'), libraries).then(() => null, error => error) : Promise.resolve(null);
  let failed;
  rmSync(resolve(real, CODE_WARNINGS), { force: true });
  try {
    const bundle = await rolldown({ ...bundleOptions, cwd: real, input: resolve(real, '__exact_entry.ts'), tsconfig: resolve(real, '__exact_tsconfig.json'),
      logLevel: 'silent', plugins: [capturedSources(real, why)] });
    try { await bundle.generate({ format: 'esm' }); } finally { await bundle.close(); }
  } catch (error) { failed = error; }
  // Each use of `Function` or `eval`, whose code from a string throws on every host (js/bake/src/__exact_portable.mjs):
  // a diagnostic in the compiler's form, as `contract build` and a drive's summary read it, said after any error.
  let warnings = [];
  try { warnings = JSON.parse(readFileSync(resolve(real, CODE_WARNINGS), 'utf8')); } catch { /* a bundle that failed may have written none */ }
  if (failed || why.length) {
    const errors = [failed, ...why.map(line => new Error(line)), await typed].filter(Boolean);
    throw Object.assign(new Error(errors.map(e => e.message ?? String(e)).join('\n')), { warnings });
  }
  const exported = await valueExports(real, libraries);
  // The packages whose code it captured are inputs the dev loop watches, as
  // their declarations are: each package's tree, and each place it is
  // installed (a link retargeted, or a reinstall, is an edit there).
  if (devReload) {
    const listed = resolve(gen, 'dev-sources.json');
    const sources = existsSync(listed) ? JSON.parse(readFileSync(listed, 'utf8')) : {};
    const packages = JSON.parse(readFileSync(resolve(real, '__exact_packages.json'), 'utf8')).packages;
    sources.packages = [...new Set([...(sources.packages ?? []), ...packages.map(([, dir]) => dir)])];
    sources.shallow = [...(sources.shallow ?? []), ...packages.flatMap(([, , installed]) => installed.map(path => [dirname(path), basename(path)]))];
    writeFileSync(listed, JSON.stringify(sources));
  }
  return { facts: { ...staticFacts(readFileSync(resolve(real, 'app.ts'), 'utf8'), parseSync), exported }, typed, warnings };
}
const tsDraws = canvas2d && captured?.facts.exported.includes('draw');
// A Rust module says whether it draws only when it loads (canvas2d.js says so then); with none, a surface no GPU module
// registers and app.ts does not draw would stay blank.
if (canvas2d && !rust && !tsDraws && gpuSurfaces) {
  const names = readFileSync(resolve(gen, 'canvas2d.flag'), 'utf8').split('\n').filter(Boolean);
  console.error(`${app}: nothing draws the canvas surface${names.length === 1 ? '' : 's'} ${names.join(', ')}: ${existsSync(gpuLib) ? `the GPU module registers ${gpuSurfaces.join(', ') || 'no surface'}` : 'the app has no GPU module'}, and ${ts ? 'app.ts exports no `draw`' : 'it has no data source to draw on Canvas 2D'}`);
  process.exit(1);
}
if (tsDraws) writeFileSync(resolve(gen, 'ts-draw.js'), readFileSync(resolve(here, 'ts-draw.js'), 'utf8').replace('__APP_TS__', pageAppTs).replace('__RECORDER__', resolve(root, 'canvas/recorder.js')));
if (tsDraws) writeFileSync(resolve(gen, 'path2d.js'), 'export const browserPath2D = globalThis.Path2D;\n');
writeFileSync(resolve(gen, 'draw.js'), tsDraws ? "export { drawer } from './ts-draw.js';\n" : 'export const drawer = null;\n');
const normalizeGrants = (label, spec, stem) => {
  const file = resolve(gen, `${stem}.grants`);
  writeFileSync(file, spec);
  const result = spawnSync(compiler.cmd, [...compiler.pre, 'normalize-grants', file], { cwd: root, encoding: 'utf8' });
  if (result.status !== 0) { console.error(result.stderr); process.exit(result.status ?? 1); }
  const set = JSON.parse(result.stdout);
  if (set.error) throw new Error(`grant-parse: ${label}: ${set.error}`);
  return set;
};
const tsModule = captured?.facts ?? null;
const grants = ts ? developmentLocalNetwork(String(tsModule.grants ?? '')) : '';
// What the native bake refuses of the module (js/bake/src/lib.rs `bake_in`,
// bake/src/receipt.rs), refused here as well, so the web loop fails where a
// native build would (files diary F13).
if (ts) {
  const expected = (await import('../../scripts/app.mjs')).readManifest(appDir, app).app.id;
  const problems = [
    ...['__exact_backend_source.json', '__exact_backend.json', '__exact_code_warnings.json', '__exact_packages.json', '__exact_entry.ts', '__exact_tsconfig.json', '__exact_config.mjs', '__exact_binding.mjs', '__exact_env.mjs', '__exact_portable.mjs', '__exact_env.json', '__exact_paths.json', '__exact_declarations.json', '__exact_canvas.js', '__exact_canvas.d.ts', 'app.js']
      .filter(name => existsSync(resolve(appDir, name))).map(name => `${name} is reserved for the producer`),
    ...tsModule.exported.includes('draw') !== tsModule.exported.includes('surfaces') ? ['app.ts exports `draw` and `surfaces` together, or neither (LLP 1056 D1)'] : [],
    ...tsModule.appId !== undefined && tsModule.appId !== expected ? [`app.ts's appId is ${tsModule.appId}, but app.json's "app": { "id" } is ${expected} (its top-level "id" is the web manifest's, another field): make the two the same`] : [],
  ];
  if (problems.length) { console.error(problems.map(p => `error: ${p}`).join('\n')); process.exit(1); }
}
const tsGrantSet = normalizeGrants(ts ? 'app.ts' : 'TypeScript', grants, 'typescript');
let moduleStorage = false, rustGrants = '', rustGrantSet = normalizeGrants('Rust module', '', 'rust');
if (rust) {
  mkdirSync(resolve(out, 'rust/wasm'), { recursive: true });
  const from = opt('--data') ?? (opt('--plan') && dirname(resolve(opt('--plan'))));
  const built = from && existsSync(resolve(from, 'rust/wasm/app.module.wasm')) ? resolve(from, 'rust/wasm/app.module.wasm') : buildModule(app, undefined, canvas2d, appDir);
  cpSync(built, resolve(out, 'rust/wasm/app.module.wasm'));
  if (devReload) devLogic.push(['rust', createHash('sha256').update(readFileSync(resolve(out, 'rust/wasm/app.module.wasm'))).digest('hex')]);
  rustGrants = await moduleGrants(built);
  rustGrantSet = normalizeGrants(ts ? 'Rust module' : 'app', rustGrants, 'rust');
  moduleStorage = /^\s*(?:fs|sqlite)\./m.test(rustGrants);
  // The module binds with the plan's declarations, not the bake's answers.
  cpSync(resolve(gen, 'app.bind.plan'), resolve(out, 'app.bind.plan'));
}
writeFileSync(resolve(gen, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const tsGrantSet=createGrantSet(${JSON.stringify(tsGrantSet)}),rustGrantSet=createGrantSet(${JSON.stringify(rustGrantSet)});\n`);
// The app's launch modules that have a web service.
const launchWeb = launchModules(appDir, manifest).filter(m => m.web);
if (launchWeb.length) cpSync(resolve(here, 'marks.js'), resolve(gen, 'marks.js'));
// Held fetches and fetches tied to no answer (LLP 1016.000 D9): ts-fetch.js counts them through held.js, which it
// does not import. Each entry imports this first, so a fetch an app module makes as it loads is counted too.
if (ts) writeFileSync(resolve(gen, 'held-hooks.js'), "import { answering } from './ts-fetch.js'; import { heldReply, looseFetch } from './held.js'; Object.assign(answering, { loose: looseFetch, heldReply });\n");
writeFileSync(resolve(gen, 'main.js'), [
  ...(ts ? ["import './held-hooks.js';"] : []),
  "import app, { sources, wait } from './app.js';",
  ...(devReload ? ["import { prepareDev } from './checkpoint.js';", "const finishDev = prepareDev();"] : []),
  ...(files ? ["import './files.js';"] : []),
  ...(soundTable ? [`import { install as sounds } from './sounds.js'; sounds(${soundTable});`] : []),
  ...(wakelock ? ["import { install as wakelock } from './wakelock.js'; wakelock();"] : []),
  ...(orientationlock ? ["import { install as orientationlock } from './orientationlock.js'; orientationlock();"] : []),
  // A prompt's Return, a grouped alert's message box and a menu row's popover (host/web): glue a plan with a dialog, an
  // alert, a menu or a popover needs on the page; until LLP 1047.002 commands.js carried them for every page.
  ...(dialogs ? ["import './prompt-glue.js'; import './alert-message.js'; import './menu-rows.js'; globalThis.exact?.menuRows?.();"] : []),
  // What this entry names from the runtime is what the bundle keeps (LLP 1047.002 D4). A production page names what
  // its own lines below use and what production glue reads from `globalThis.exact` (`journal`, `clock`: arrange.js;
  // `views`: media-glue.js; `viewId`: navigation.js; `After`: motion.js); the agent adapter, inspect and the dev
  // loop read the rest, so a development page names them too.
  `import { data, journal, clock, commit, inflight, Views, viewId, After, Clocked, R, resolvedLocale, Resources, agentMode${production ? '' : ', advance, heldAsks, heldReplies, Mutations, writeRecords, goTo'} } from './rt.js';`,
  `data.keepSurfaceAnswers = ${!tsGrantSet.keepsNoAnswers && !rustGrantSet.keepsNoAnswers};`,
  ...(production ? [] : ["import { develop } from './perf.js';"]),
  // A data module's answers, watched from before the app asks (seam.js).
  ...(production || !asks ? [] : ["import { seam } from './perf.js';", 'seam();']),
  ...(time ? [
    "import { timeFields } from './facts.js';",
    "import { reportTime, reportPlace } from './navigation.js';",
    "data.reserved = { exactTime: (_, a, name) => {",
    "  const [epochAtZero, utcOffset] = reportTime(clock.now), [locale, timeZone, seed, formatLocale] = reportPlace().split('\\0');",
    "  return timeFields(name, { epochAtZero, utcOffset, locale, timeZone, seed: Number(seed), formatLocale: formatLocale || locale, resolvedLocale: resolvedLocale() });",
    "} };",
    // The zone's offset follows the clock (habits F6): a timer, a `then` or the agent's `clock` that finds the offset
    // at its instant changed (a DST change, a new zone) answers `exactTime` again first (LLP 1027.000.000 D2).
    "let told = reportTime(clock.now)[1];",
    "Clocked.push(() => { const o = reportTime(clock.now)[1]; if (o === told) return; told = o; commit(() => { for (const r of Resources) if (r.source === 'exactTime') R(r); }, 'time'); });",
  ] : []),
  ...(ts ? [`import { install as ts${production ? '' : ', watchState'} } from './ts-data.js';`, `ts(data, ${mixed}${pageModules ? ", () => import('./native.js')" : ''});`, "import { answering } from './ts-fetch.js';"] : []),
  ...(ts && !production ? ['globalThis.exact = Object.assign(globalThis.exact ?? {}, { watchState });'] : []),
  // The schemes `openURL` may hand to the browser beyond its own (app.json `open_url`, navigation.js `declaredScheme`).
  ...(manifest.open_url?.schemes?.length ? [`globalThis.exact = Object.assign(globalThis.exact ?? {}, { openSchemes: ${JSON.stringify(manifest.open_url.schemes)} });`] : []),
  // Before the first render, whose hatched nodes read them (hatches.js `ht`, `page`): the hatch words the web handles,
  // when app.json gives words their platforms (LLP 1075.003.000.001 §4.3, §5: a word it leaves out is shown and never
  // called), and whether the app has a page module at all (none: no module artifact is asked for).
  `globalThis.exact = Object.assign(globalThis.exact ?? {}, { pageModule: ${pageModules}${manifest.hatches && !Array.isArray(manifest.hatches) ? `, hatchWords: ${JSON.stringify(hatchWords(manifest, 'web'))}` : ''} });`,
  // Launch marks install before the app boots; marks.js loads the services after startup.
  ...(launchWeb.length ? ["import { install as launchMarks } from './marks.js';", `const marks = launchMarks(${JSON.stringify(launchWeb.map(m => ({ name: m.name, url: `./launch/${m.name}/service.js`, config: { ...(manifest.moduleConfig?.[m.name] ?? {}), 'app.CFBundleIdentifier': manifest.app?.id, 'app.CFBundleName': manifest.app?.name, 'fact.development': !production } })))});`] : []),
  "const start = () => {",
  "  const state = app();",
  ...(launchWeb.length ? ["  marks.mounted();"] : []),
  ...(devReload ? ["  finishDev();"] : []),
  `  globalThis.exact = Object.assign(globalThis.exact ?? {}, { ready: true, journal, clock, views: Views, viewId, After${production ? '' : ', advance, commit, data, state, inflight, heldAsks, heldReplies, resources: Resources, mutations: Mutations, writes: writeRecords'} });`,
  // A development page counts its work and samples its frames (LLP 1079); the agent adapter, only when the agent drives it.
  // The served plan's digest, which a development page's `perf` names (LLP 1079 D2).
  ...(production ? [] : [`  globalThis.exact.plan = ${JSON.stringify(createHash('sha256').update(readFileSync(opt('--plan') ? resolve(opt('--plan')) : resolve(gen, 'app.plan'))).digest('hex'))};`, "  globalThis.exact.goTo = goTo;", "  develop(globalThis.exact).catch(console.error);", "  if (clock.agent) globalThis.exact.ready = import('./agent.js').then(m => m.install(globalThis.exact));"]),
  "  booted?.();",
  "};",
  // The declared fonts are the stylesheet's `font-display: optional` faces (src/faces.rs): a face first needed while it
  // still loads is never used on the page, so with a slow or uncached font file the page kept the fallback for good,
  // where the wasm host waits up to 100 ms for its faces before its first render (glue.js prepareFonts). The first
  // render here waits the same way: each face loads, for at most 100 ms, and one loaded by then is used.
  // Only the plan's own faces (`ExactPlanStack<n>`, exact_web::host::fonts), never another stylesheet's. While it
  // waits, `exact.ready` is a promise the boot settles, so a reader that finds the TypeScript data's early
  // `globalThis.exact` (the dev client, inspect) waits for the runtime rather than reading a half-made one.
  "const faces = typeof document === 'object' && document.fonts ? [...document.fonts].filter(f => /^\"?ExactPlanStack\\d+\"?$/.test(f.family) && f.status !== 'loaded') : [];",
  "const fonts = faces.length ? Promise.race([Promise.all(faces.map(f => f.load().catch(() => {}))), new Promise(r => setTimeout(r, 100))]) : null;",
  "let booted; if (fonts) (globalThis.exact ??= {}).ready = new Promise(r => { booted = r; });",
  // What armed itself at module load (sounds' output, a native page module's connection) reads `clock.agent` in a
  // callback that a font wait lets run before the first render, where `mount` sets it: set it now.
  "if (fonts) clock.agent = agentMode();",
  // A rendered page's sheet holds only what its document can match (LLP 1048.007 D4): the whole sheet comes first,
  // then the page's goes, before the first render (one that fails to load leaves the page's, and the render goes on).
  "const page = typeof document === 'object' && document.querySelector('style[data-exact-page]');",
  "const sheet = page && new Promise(r => { const l = Object.assign(document.createElement('link'), { rel: 'stylesheet', href: './style.css' }); l.onload = () => { page.remove(); r(); }; l.onerror = r; page.before(l); });",
  // A waiting page's answers past 1 KB are named, not held (LLP 1048.007 D10): the capture script fetched them as it
  // imported this; the first render waits for them. Ones no longer kept are none, and the boot builds afresh.
  "const named = typeof document === 'object' && document.querySelector('script[type=\"application/vnd.exact.checkpoint\"][data-src]');",
  "const answers = named && (globalThis.exact?.answers ?? fetch(named.dataset.src).then(r => r.ok ? r.text() : null, () => null)).then(t => { named.textContent = t ?? '{\"time\":0,\"answers\":[]}'; });",
  // Under the driver a TypeScript source draws the launch seed's stream (LLP 1069.005 D2b), from its first answer:
  // the first render waits for the stream's module, which no other page loads before it needs storage.
  ...(ts && !production ? ["const seeded = agentMode() && !globalThis.__exactRender && import('./storage-environment.js').then(env => { const seed = env.agentSeed(); if (seed !== null) answering.entropy = Object.assign(env.agentStream(seed, 'typescript'), { seed }); });"] : ['const seeded = null;']),
  "const waits = [fonts, sheet, answers, seeded].filter(Boolean);",
  "if ((sheet || answers || seeded) && !fonts) { (globalThis.exact ??= {}).ready = new Promise(r => { booted = r; }); clock.agent = agentMode(); }",
  "const go = f => waits.length ? Promise.all(waits).then(f) : f();",
  // Under the agent, an animation the first renders start (an indeterminate progress's turn) is held at the clock it
  // began at after each commit, before a frame shows it, until agent.js, which loads after the first render, takes it
  // (its `register`): never left on the browser's clock, nor turned on the compositor first (LLP 1069.001: a spinner
  // follows the agent's clock), as the wasm host holds each commit's at once.
  "if (agentMode()) { const early = (globalThis.exact ??= {}).early = new WeakMap(); After.push(globalThis.exact.holdEarly = () => { for (const a of document.getAnimations()) if (!early.has(a) && a.playState === 'running' && (!a.timeline || a.timeline instanceof DocumentTimeline)) { early.set(a, clock.now); a.pause(); a.currentTime = 0; } }); }",
  ...(rust ? [
    // Rust data: loaded after first pixel, asked synchronously once ready;
    // a plan with a resource that has no compiled value waits for it.
    // Counted in flight, so `clock settle` waits for the source to be ready.
    // One that did not load (its chunk, its fetch, its binding) is said once in the journal, as LLP 1027.007 §5 has
    // a failed load reported once. Once the app has started, with what the module owns not ready (its compiled
    // value or placeholder shown), each such source fails when asked, as one whose call failed (LLP 1071 §7): the
    // resource keeps what it shows and `failed` is true. A mixed app starts anyway, its TypeScript module answering;
    // what that module throws may be a source the Rust module owns, so its error names the failed load too.
    "let unloaded = null;",
    "const load = () => import('./rust-data.js').then(m => m.install(data, sources)).catch(e => { unloaded = `the Rust module did not load: ${e?.message ?? e}`; journal.push(`t=${clock.now} ${unloaded}`); }).finally(() => inflight.n--);",
    "const also = e => { if (!(e instanceof Error)) return new Error(`${e}; ${unloaded}`); if (!e.message.endsWith(unloaded)) e.message += `; ${unloaded}`; return e; };",
    "const fail = () => { if (!unloaded) return; const ts = data.ts; data.answer = ts ? (...a) => { try { return ts(...a); } catch (e) { throw also(e); } } : () => { throw new Error(unloaded); }; for (const f of data.q.splice(0)) f(); };",
    "inflight.n++;",
    `if (wait || ${mixed}) load().then(() => go(() => { start(); fail(); })); else go(() => { start(); requestAnimationFrame(() => setTimeout(() => load().then(fail))); });`,
  ] : ['go(start);']),
  // After the first render, which a font wait may defer (`containers()` reads the runtime's `globalThis.exact`).
  ...(containerHatches ? ["go(() => requestAnimationFrame(() => requestAnimationFrame(() => import('./hatches.js').then(m => m.containers()))));"] : []),
].join('\n'));
for (const f of ['agent.js', 'check.js', 'perf.js', 'seam.js', 'rust-data.js', 'list.js', 'facts.js', 'fetches.js', 'modules.js', 'sheets.js', 'samples.js', 'wakes.js', 'reach.js', 'devices.js', 'members.js', 'arrival.js', 'calls.js', 'update.js', 'spans.js', 'views.js', 'pictures.js', 'markdown.js', 'symbols.js', 'motion.js', 'drag-target.js', 'transform.js', 'svg-transform.js', 'dataset.js', 'format.js', 'format-locale.js', 'hatches.js', 'arrange.js', 'reorder.js', 'flow.js', 'native.js', 'shared.js']) cpSync(resolve(here, f), resolve(gen, f));
writeFileSync(resolve(gen, 'modules.js'), localImports(readFileSync(resolve(here, 'modules.js'), 'utf8')));
// The web host's own pieces the runtime loads (glue.mjs).
stageHostGlue(root, gen);
// Keep navigation one runtime module and admission a separate lazy module on the JS target.
writeFileSync(resolve(gen, 'navigation.js'), await bundleHostModule(root, 'navigation.js', { grants: true, production }));
const grantNames = Object.keys(await import('../web/grant-admission.js'))
  .filter(name => !['isCloudMetadata', 'isLocalNetwork'].includes(name));
writeFileSync(resolve(gen, 'grant-network.js'), await bundleHostModule(root, 'navigation/network.js'));
// A production page whose every grant set is empty (no TypeScript grant, no Rust module, no native module, no page
// module, no declared source) admits nothing, and says so without the parser (LLP 1047.002 D3): each export of admission answers as it
// does for the empty set the compiler normalized (`admission-data.js` holds the same one), and the parser is not
// linked. A name admission exports that this table lacks fails the build, so a new check is answered here before a
// page ships without it. Development pages keep the parser (LLP 1047 D7).
// A source declared in Contract carries its origin as its grant (fetches.js, LLP 1162.001 D3).
const declared = readFileSync(resolve(gen, 'app.js'), 'utf8').includes('from"./fetches.js"');
const admitsNothing = production && !tsGrantSet.error && tsGrantSet.entries.length === 0 && !rust && !Object.keys(moduleFlag.extensions).length && !pageModules && !declared;
// Whether any source holds a network grant (the kinds grant-admission.js's `networkGrants`, `schemeGrants` and `localGrants` name).
const netGranted = [tsGrantSet, rustGrantSet].some(set => (set.entries ?? []).some(([, , grant]) => /^(fetch|websocket)(-|$)/.test(grant?.[0] ?? '')));
const fetchRefused = production && ts && !netGranted;
const emptyGrantAdmission = () => {
  const answers = {
    createGrantSet: '() => EMPTY', scopedGrantSet: '() => EMPTY', unionGrantSets: '() => EMPTY',
    grantError: '() => null', rawGrantText: "() => ''", hasGrant: '() => false',
    admittingGrant: '() => null', admitsNetwork: '() => false', admitsSecret: '() => false', coversPath: '() => false',
    // Scoped to any source name, the empty set declares what it declares whole: nothing.
    sameGrantDeclaration: "(set, source) => typeof source === 'string'",
    // No grant admits a WebSocket stream, so no protocol is ever offered to one.
    socketProtocols: "(value) => { if (value === undefined) return []; throw new TypeError('exactProtocols: no grant admits a WebSocket stream'); }",
  };
  const missing = grantNames.filter(name => !(name in answers));
  if (missing.length) throw new Error(`host/web/grant-admission.js: admission exports ${missing.join(', ')}, which build.mjs's empty-set admission does not answer`);
  return ['// Generated: the grant admission of a page that admits nothing (LLP 1047.002 D3). Every set is the empty set the compiler normalized.',
    `const EMPTY = Object.freeze({ ...${JSON.stringify(tsGrantSet)}, entries: Object.freeze([]) });`,
    ...grantNames.map(name => `export const ${name} = ${answers[name]};`),
    "export { isCloudMetadata, isLocalNetwork } from './grant-network.js';"].join('\n');
};
// The agent adapter reads its own copies of the modules it shares with the
// entry: a module lives in one chunk, so what only the agent reads from
// navigation.js (the environment and page observations) or names.js
// (every slot's type) would otherwise ride in every page's entry module.
cpSync(resolve(root, 'host/web/frame-messages.js'), resolve(gen, 'frame-messages.js'));
for (const f of ['navigation.js', 'names.js']) cpSync(resolve(gen, f), resolve(gen, 'agent-' + f));
writeFileSync(resolve(gen, 'agent.js'), localImports(readFileSync(resolve(gen, 'agent.js'), 'utf8').replace("from './names.js'", "from './agent-names.js'").replace("from './navigation.js'", "from './agent-navigation.js'")));
// A source granted `auth.session` signs in through the system browser (auth.js, LLP 1069.006), as
// `@exact/sign-in` does for one granted `auth.apple` or `auth.google` (LLP 1133.014 D8).
const auth = /^\s*auth\.(session|apple|google)\s/m.test(grants);
// An upload is a granted fetch's stream (`exactUploadProgress`, LLP 1016.000 D10), so sources whose `fetch` is
// ts-fetch-refused.js's never make one: rt.js keeps its empty ledger and uploads.js is not linked (LLP 1047.002 D3).
const uploadsImport = "import { ledger, requestKey } from './uploads.js';";
const tsData = readFileSync(resolve(here, 'ts-data.js'), 'utf8');
if (!tsData.includes(uploadsImport)) throw new Error(`host/web-js/ts-data.js: build.mjs replaces a line it no longer has: ${uploadsImport}`);
if (ts) writeFileSync(resolve(gen, 'ts-data.js'), tsData.replace("'__APP_TS__'", JSON.stringify(pageAppTs))
  .replace(uploadsImport, fetchRefused ? "import { data as runtime } from './rt.js'; const ledger = runtime.uploads, requestKey = () => undefined;" : uploadsImport)
  .replace('const appId = source.appId;', `const appId = ${JSON.stringify((await import('../../scripts/app.mjs')).readManifest(appDir, app).app.id)};`)
  .replace('__AUTH_IMPORT__', auth ? "import { install as signIn } from './auth.js';" : '')
  .replace('__AUTH_INSTALL__', auth ? `signIn(${JSON.stringify(grants)}, () => asking);` : '')
  .replace("'__FETCH_MODULE__'", JSON.stringify(fetchRefused ? './ts-fetch-refused.js' : './ts-fetch.js')));
writeFileSync(resolve(gen, 'grant-admission.js'), admitsNothing ? emptyGrantAdmission() : await bundleHostModule(root, 'grant-admission.js'));
// With nothing admitted no request leaves the page's origin, so the driver's faults (LLP 1103), which fail a request
// that would have gone out, have nothing to fail: `fetchWith` keeps its asset path and loses faults.js.
// Likewise the local network's reach and redirect gates (address-space.js, LLP 1133.036 D6), which check a tool's, a
// wake's or the device's request and pass the app's: with no module there is no origin but the app.
const admission = readFileSync(resolve(here, 'admission.js'), 'utf8');
const stubbed = [
  ["import { faultMessage, takeFault } from '../web/faults.js';", "const takeFault = () => false, faultMessage = () => '';"],
  ["import { redirectFor, reachRefusal } from '../web/address-space.js';", 'const redirectFor = (origin, redirect) => redirect, reachRefusal = () => null;'],
].reduce((text, [line, stub]) => {
  if (!admission.includes(line)) throw new Error(`host/web-js/admission.js: build.mjs replaces a line it no longer has: ${line}`);
  return admitsNothing ? text.replace(line, stub) : text;
}, admission);
writeFileSync(resolve(gen, 'admission.js'), localImports(stubbed));
// A production page whose sources hold no network grant binds their `fetch` to the app's own assets and the refusal by
// name (LLP 1047.002 D3): `fetchWith` admits an asset GET and refuses the rest for the empty network set, counted on its
// answer as any fetch is (`ordinary`), so ts-fetch.js's request paths (options, streams, uploads, bodies from files) are
// never linked. The realm it binds (the clock, the timers, the refused globals) stays with the module.
if (fetchRefused) writeFileSync(resolve(gen, 'ts-fetch-refused.js'), [
  '// Generated: `fetch` for sources with no network grant (LLP 1047.002 D3): the app\'s own assets, else the refusal by name.',
  "import { fetchWith } from './admission.js'; import { ordinary } from './ts-fetch.js'; import { tsGrantSet } from './admission-data.js';",
  'export const fetch = (input, init) => ordinary(() => fetchWith(tsGrantSet, input, init));', ''].join('\n'));
cpSync(resolve(here, 'ts-fetch.js'), resolve(gen, 'ts-fetch.js'));
cpSync(resolve(here, 'ts-body-from.js'), resolve(gen, 'ts-body-from.js'));
cpSync(resolve(here, 'ts-stream.js'), resolve(gen, 'ts-stream.js'));
cpSync(resolve(here, 'auth.js'), resolve(gen, 'auth.js'));
cpSync(resolve(here, 'files.js'), resolve(gen, 'files.js'));
cpSync(resolve(here, 'sounds.js'), resolve(gen, 'sounds.js'));
cpSync(resolve(here, 'wakelock.js'), resolve(gen, 'wakelock.js'));
cpSync(resolve(here, 'orientationlock.js'), resolve(gen, 'orientationlock.js'));
cpSync(resolve(here, 'transport.js'), resolve(gen, 'transport.js'));
cpSync(resolve(here, 'clockin.js'), resolve(gen, 'clockin.js'));
cpSync(resolve(here, 'midi.js'), resolve(gen, 'midi.js'));
// The server bundle a JavaScript render runs (render.mjs), one script per VM context.
writeFileSync(resolve(gen, 'main-server.js'), [
  ...(ts ? ["import './held-hooks.js';"] : []),
  `import app${rust ? ', { sources }' : ''} from './app.js';`,
  ...(soundTable ? [`import { install as sounds } from './sounds.js'; sounds(${soundTable});`] : []),
  "import { data, clock, inflight, Resources, Mutations, routeAt, Head, canSetStatus, resolvedLanguage } from './rt.js'; import { answersAtWork, looseOut } from './held.js';",
  "import { types, sourceTypes, pages, viewportMedia, activation, markdownPaths } from './names.js';",
  ...(existsSync(resolve(gen, 'markdown.flag')) ? ["import { compileReaders } from './markdown.js';", 'globalThis.__markdown = () => compileReaders(markdownPaths, planResources.map(g => g.r));'] : []),
  "import { viewportFacts } from './facts.js';",
  "import { answers } from './checkpoint.js';",
  "import { markBars, routeTitle } from './navigation.js';",
  "import { markRoutes, markDocument } from './document.js';",
  ...(ts ? ["import { install as ts } from './ts-data.js';", "import { unfinishedIO } from './ts-fetch.js';"] : []),
  ...(rust ? ["import { install } from './rust-data.js';"] : []),
  // `exactTime` is a fact of the reader's device, which a render has not got: it answers as the runner does before a
  // host says the date (runner/src/time.rs, as the Rust renderer does), the unknown date in UTC, never the app's
  // source (LLP 1039 D1). The runtime answers the reader's when it adopts the page.
  ...(time ? [
    "import { resolvedLocale } from './rt.js'; import { timeFields } from './facts.js';",
    "data.reserved = { exactTime: (_, a, name) => timeFields(name, { epochAtZero: 0, utcOffset: 0, locale: 'en-US', timeZone: 'UTC', seed: 0, formatLocale: 'en-US', resolvedLocale: resolvedLocale() }) };",
  ] : []),
  // Two steps, so a server can send the page's head between them
  // (render.mjs): the app starts and names its route, then settles.
  'let t0, route, planResources;',
  'globalThis.__start = async () => {',
  '  t0 = performance.now();',
  ...(ts ? [`  ts(data, ${mixed});`] : []),
  ...(rust ? ['  await install(data, sources, async p => __files(p));'] : []),
  // Types and Markdown paths index the declared resources. Resources holds list members instead of their aggregate.
  '  planResources = app()[2];',
  '  route = routeAt(location.pathname + location.search);',
  '  const [render, activate] = pages[route] ?? ["build", "inferred"];',
  // `language`: the resolved table's `lang` and `dir`, which the page's head carries (render.mjs `headOf`).
  '  return { activate: ["idle", "interaction", "never"].includes(activate) ? activate : "inferred", policy: render, notfound: !!pages[route]?.[2], readsViewport: activation.viewport, canSetStatus: canSetStatus(), language: resolvedLanguage() };',
  '};',
  // A render waits for work in flight, as the render host settles (host/render/src/lib.rs `settle`, LLP 1048.000 D9):
  // requests and streams before their first message (`inflight`), a source not ready yet, an answer's I/O and timers
  // (`answersAtWork`), a fetch tied to no answer. An answer waiting on nothing the render runs stays pending. A page
  // that can declare a status also waits for work a let-go answer or one past its answer still has out.
  'globalThis.__render = async (deadline, waitStatus = true) => {',
  '  const end = t0 + deadline;',
  `  const unfinished = () => answersAtWork(true)${ts ? ' || unfinishedIO()' : ''};`,
  // A stream or long poll holds the page until its first message, as on the render host (runner.rs `in_flight`): a long
  // poll's reply that carries none (a 204, a 503 to wait out) takes it out of `inflight`, for clocks, but not here.
  // A stream whose body file the render refused as it opened started no request, and holds nothing (ts-data.js).
  '  const unmessaged = t => !!t && (!!t.stream || !!t.req?.stream) && !t.messages && !t.closed && !t.ctl?.refused;',
  '  const outstanding = () => inflight.n > 0 || Resources.some(r => r.waiting || (r.pending() && unmessaged(r.ticket))) || answersAtWork() || looseOut() || (waitStatus && unfinished());',
  '  do await new Promise(r => setTimeout(r, 1)); while (outstanding() && performance.now() < end);',
  '  markBars(document.getElementById("exact-root")); // the bars the glue marks, so their symbols are the size they will be',
  '  markRoutes(document.getElementById("exact-root")); markDocument(); // a hidden route\'s page scroller is off the page',
  '  return { root: document.rootHTML(), title: Head.headTitle ?? routeTitle(document.getElementById("exact-root")), description: Head.headDescription, image: Head.headImage, canonical: Head.headCanonical, robots: Head.headRobots,',
  // The manifest's origin, against which a crawler's URLs are made absolute (page.rs `page_head`).
  `    origin: ${JSON.stringify(manifest.app?.origin ?? null)}, status: Head.headStatus ?? null, time: clock.now, ...answers(planResources, types[2], sourceTypes),`,
  // Each resource whose source failed, with its message: the server's log names it (render.mjs), as the page cannot.
  '    failed: Resources.filter(r => r.failed).map(r => `${r.name}: ${r.error}`),',
  // Each pending resource refused storage, by its request (admission.js) or call (ts-data.js), for the log.
  '    refused: Resources.filter(r => r.pending() && (r.ticket?.refused || (r.ticket?.call ?? r.ticket?.ctl)?.refused)).map(r => [r.name, r.ticket.refused ?? (r.ticket.call ?? r.ticket.ctl).refused]),',
  // `continuing`: work that outlives the render, whatever it waited for (a page with no status does not wait for an
  // answered source's I/O): the client asks every answer again (render.mjs `revalidate`). The answers the client asks
  // again one at a time are in `answers`' `open` (checkpoint.js).
  '    pending: Resources.filter(r => r.pending()).map(r => r.name), outstanding: outstanding(), continuing: outstanding() || unfinished(), scrollDocument: document.documentElement.hasAttribute("data-scrolldocument"), render: performance.now() - t0,',
  // What an inferred activation reads (LLP 1048.007 D1): what is still due, the sources read, the viewport's comparisons as the render saw them.
  '    due: clock.timers.length > 0 || Mutations.some(m => m.due < Infinity || m.next < Infinity), activation, zoned: !!globalThis.__exactZone,',
  // Only a TypeScript source is held to no clock and no Math.random (ts-fetch.js): a Rust module's page is never kept.
  `    rust: ${!!rust},`,
  '    wake: viewportMedia.map(([query, holds]) => [query, holds(viewportFacts())]) };',
  '};',
].join('\n'));
cpSync(resolve(here, 'checkpoint.js'), resolve(gen, 'checkpoint.js'));
// A release admits no agent mode (LLP 1069.007 D2): every file that reads
// `?agent` declares AGENT_ADMITTED, written false here, as host/web/build.mjs
// gates the wasm host's. Lazy host modules pass through the same gate.
if (production) for (const f of readdirSync(gen).filter(f => f.endsWith('.js'))) {
  const code = readFileSync(resolve(gen, f), 'utf8');
  writeFileSync(resolve(gen, f), gateAgent(code, f));
}
// A development page's storage is its app's (LLP 1116.004 D8): every app the dev server serves
// shares its origin, so the app directory's digest (agent-launch.mjs `appDigest`) keys its files,
// databases and kept keys (storage-environment.js) and its `localStorage` (rt.js `Kept`). A release keeps the id alone.
if (!production) for (const f of readdirSync(gen).filter(f => f.endsWith('.js'))) {
  const code = readFileSync(resolve(gen, f), 'utf8'), scoped = developmentStore(code, realpathSync(appDir));
  if (scoped !== code) writeFileSync(resolve(gen, f), scoped);
}
// A build that renders nothing (the dev loop's) makes no server bundle. Pages render
// with the app's native render entry where it has one (`<app>-render`, in an authored
// crate), else under Bun (LLP 1153 D3.5).
const renderBin = `${app}-render`;
const renderAt = [['linux', `${app}-linux`], ['web', `${app}-web`]].find(([dir]) => existsSync(resolve(appDir, dir, 'src/bin', `${renderBin}.rs`)));
const how = opt('--render') ?? (renderAt ? 'rust' : 'js');
// Inject only into the app's module graph (the capture, under gen), never
// the copied host runtime. Oxc resolves lexical bindings, so an authored
// local `fetch` stays local.
const generatedRoots = [gen, realpathSync(gen)];
const inside = (path, root) => { const child = relative(root, path); return child === '' || !isAbsolute(child) && child !== '..' && !child.startsWith('..' + sep); };
// Both bundles lower `async` so an answer's context survives its awaits
// (`asyncContext`, LLP 1133.002 D14). Rolldown supplies every helper the lowering emits.
// A native module's web implementation is the page's, as an app's own page module is.
const implementationRoots = [...webImplementations(appDir, moduleFlag.extensions).values()].map(at => dirname(realpathSync(at)));
const scopedModule = (code, id) => {
  if (!ts || !isAbsolute(id) || !/\.[cm]?[jt]sx?$/.test(id)) return null;
  if (!inside(id, tsStage) && generatedRoots.some(root => inside(id, root))) return null;
  if (implementationRoots.some(root => inside(id, root) || inside(realpathSync(id), root))) return null;
  // And ts-fetch.js's bindings: the clock and Math.random refused by name
  // (LLP 1027.000 D3), an answer's own timers (D1) with every other timer
  // refused, the scheduler's yield (LLP 1141.001 D10), the browser's own
  // I/O refused, as the wasm target's realm refuses it, and code from a
  // string, which throws as on Hermes (`eval`, which no import may bind, is imported under a fresh name below).
  const bound = ['fetch', 'Date', 'Math', 'Intl', 'setTimeout', 'setInterval', 'requestAnimationFrame', 'requestIdleCallback',
    'clearTimeout', 'clearInterval', 'cancelAnimationFrame', 'cancelIdleCallback', 'performance', 'scheduler', 'XMLHttpRequest', 'WebSocket', 'EventSource', 'crypto', 'AbortSignal',
    'Function'];
  // The DOM and Node's globals are absent, as on Hermes (js/bake/src/typescript.mjs `packageRefusals`).
  const absent = ['window', 'self', 'frames', 'parent', 'top', 'opener', 'document', 'navigator', 'location', 'history', 'localStorage', 'sessionStorage',
    'indexedDB', 'HTMLElement', 'Element', 'getComputedStyle', 'customElements', 'alert', 'process', 'Buffer', 'global'];
  // The stage's normalized configuration, never one found beside the file: a
  // mount's own `tsconfig.json` may extend a file outside the mount.
  const transformed = transformSync(id, code, { tsconfig: resolve(tsStage, '__exact_tsconfig.json'),
    ...lowered, helpers: { mode: 'Runtime' } });
  if (transformed.errors.length) throw new Error(transformed.errors.map(e => e.message).join('\n'));
  // Choose the alias after JSX lowering, which turns tags and pragma references into identifiers.
  const names = new Set();
  new Visitor({ Identifier: node => names.add(node.name) }).visit(parseSync(id, transformed.code).program);
  let evalName = '__exact_eval';
  for (let suffix = 1; names.has(evalName); suffix++) evalName = `__exact_eval${suffix}`;
  // `define` makes a free `eval` (`eval(…)`, `(0, eval)(…)`) this fresh name, which inject imports as ts-fetch.js's
  // stand-in: an import, so a parameter or local named `globalThis` cannot stand between. Rolldown's type comment says
  // define runs after inject; in one transform as here (rolldown 1.2.9), inject still imports the name define wrote.
  // Pinned by host/web-js/build.test.mjs "a page rendered at build runs no code from a string…".
  const result = transformSync(id, transformed.code, { tsconfig: false, define: { eval: evalName }, inject: { ...Object.fromEntries(bound.map(name => [name, [resolve(gen, name === 'fetch' && fetchRefused ? 'ts-fetch-refused.js' : 'ts-fetch.js'), name]])),
    ...Object.fromEntries(absent.map(name => [name, [resolve(gen, 'ts-fetch.js'), 'absent']])),
    [evalName]: [resolve(gen, 'ts-fetch.js'), 'eval'], globalThis: [resolve(gen, 'ts-fetch.js'), 'appGlobal'] } });
  if (result.errors.length) throw new Error(result.errors.map(e => e.message).join('\n'));
  return result.code;
};
// Both bundles take app.ts and its packages from the checked capture alone.
const replay = ts ? (await import(resolve(root, 'js/bake/src/typescript.mjs'))).replayCaptured(tsStage,
  generatedRoots.flatMap(root => ['ts-fetch.js', ...(fetchRefused ? ['ts-fetch-refused.js'] : [])].map(name => resolve(root, name)))) : null;
// These hooks are optional. Keep their static accesses for tree-shaking,
// but suppress missing-export warnings only at the generated glue's path.
const optionalHookWarning = (id, message) => generatedRoots.some(root => id === resolve(root, 'ts-data.js'))
  && /Import ["`](?:kept|overlay|tools)["`] will always be undefined because there is no matching export/.test(message);
const { rolldown } = await import('rolldown');
const scopedPlugins = () => [...(replay ? [replay.rolldown] : []),
  { name: 'exact-mounts', resolveId: (source, importer) => mounted(source, importer) },
  { name: 'source-grants', transform: scopedModule }, asyncContext()];
const onLog = (level, log) => {
  if (level === 'warn' && log.code === 'IMPORT_IS_UNDEFINED' && optionalHookWarning(log.id, log.message)) return;
  console.error(log.message);
};
// The renderer uses vm.Script, which cannot parse import.meta. Inline dynamic imports into its single script.
const markdownReaders = existsSync(resolve(gen, 'markdown.flag'));
const version = { version: process.env.EXACT_VERSION ?? '', buildNumber: process.env.EXACT_BUILD_NUMBER ?? '' };
const versionDefines = { 'globalThis.__exactVersion': JSON.stringify(version.version), 'globalThis.__exactBuildNumber': JSON.stringify(version.buildNumber) };
if (how !== 'none' || markdownReaders) {
  const b = await rolldown({ input: resolve(gen, 'main-server.js'), ...(ts ? { tsconfig: resolve(tsStage, '__exact_tsconfig.json') } : {}),
    transform: { ...bundleOptions.transform, define: { ...bundleOptions.transform.define, ...versionDefines, 'globalThis.__exactRender': 'true', 'import.meta.url': JSON.stringify(pathToFileURL(resolve(out, 'app.js')).href) } },
    plugins: scopedPlugins(), logLevel: 'warn', onLog });
  try { await b.write({ file: resolve(gen, 'server.js'), format: 'iife', codeSplitting: false }); }
  finally { await b.close(); }
}
// The page's module and its chunks: Rolldown, the repo's app bundler, whose
// minifier leaves the entry 6–10% smaller than Bun's after brotli (RealWorld
// 25.8 → 23.6 KB, the feed bench's 30.9 → 27.9, the grid's 19.3 → 18.1),
// bytes a page downloads before its runtime is up.
const preloadModules = [];
{
  const b = await rolldown({ input: resolve(gen, 'main.js'), ...(ts ? { tsconfig: resolve(tsStage, '__exact_tsconfig.json') } : {}), transform: { ...bundleOptions.transform, define: { ...bundleOptions.transform.define, ...versionDefines, 'globalThis.__exactRender': 'false' } },
    plugins: scopedPlugins(), logLevel: 'warn', onLog });
  // In a development reload build, put the TypeScript data module and every
  // helper it imports in one chunk the page really loads. Its emitted bytes,
  // rather than watcher filenames or source mtimes, are the logic revision.
  const fromData = (id, getModuleInfo, seen = new Set()) => {
    if (id === pageAppTs) return true;
    if (seen.has(id)) return false;
    seen.add(id);
    return (getModuleInfo(id)?.importers ?? []).some(parent => fromData(parent, getModuleInfo, seen));
  };
  // The entry leads with V8's explicit compile hint (LLP 1071 §7, generated
  // code at scale): Chrome compiles every function in it eagerly, and a
  // module it fetches (not `--inline`'s) as it streams in, on a background
  // thread, not each on the main thread at its first call, as a large
  // app's mount calls most of them (the Bluesky
  // clone's release page: runtime up at 1.7–1.9 s, 0.5 s with it; ~23 MB
  // more heap). Other browsers read a comment.
  const built = await b.write({ dir: out, format: 'esm', minify: true, comments: false, entryFileNames: 'app.js', chunkFileNames: '[name]-[hash].js',
    postBanner: (chunk) => chunk.isEntry ? '//# allFunctionsCalledOnLoad' : undefined,
    ...(devReload && ts ? { manualChunks: (id, { getModuleInfo }) => fromData(id, getModuleInfo) ? 'dev-data' : undefined } : {}) });
  preloadModules.push(...built.output.filter(file => file.type === 'chunk').map(file => file.fileName));
  if (devReload && ts) for (const chunk of built.output.filter(file => file.type === 'chunk' && Object.keys(file.modules).includes(pageAppTs))) {
    devLogic.push(['typescript', createHash('sha256').update(readFileSync(resolve(out, chunk.fileName))).digest('hex')]);
  }
  const entry = built.output.find(file => file.type === 'chunk' && file.isEntry);
  // Interaction-activated documents omit the head's preloads. Starting their
  // runtime prepares the same chunks without executing them.
  const deferred = preloadModules.filter(name => name !== entry.fileName);
  if (deferred.length) writeFileSync(resolve(out, entry.fileName), entry.code + `
{
  const prepared = new Set([...document.querySelectorAll('link[rel="modulepreload"]')].map(link => link.href));
  for (const name of ${JSON.stringify(deferred)}) {
    const href = new URL(name, ${args.includes('--inline') ? 'document.baseURI' : 'import.meta.url'}).href;
    if (!prepared.has(href)) document.head.append(Object.assign(document.createElement('link'), { rel: 'modulepreload', href }));
  }
}
`);
  // The entry's bytes by source module, as Rolldown rendered them before minification (shares, not a ledger), for
  // `scripts/metrics.mjs --web-bytes` (LLP 1047.002 D5): what of the runtime an app's plan reached.
  const moduleName = (id) => { const name = relative(gen, id); return name.startsWith('..') ? basename(id) : name; };
  writeFileSync(resolve(gen, 'modules.json'), JSON.stringify(Object.fromEntries(Object.entries(entry.modules).map(([id, m]) => [moduleName(id), m.renderedLength]).filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1]))));
  await b.close();
}

// The web host's base stylesheet, as its build writes it (comments out).
// Whitespace before a `:` stays: in a selector it is a descendant's pseudo-class.
// In the build's look (LLP 1115.000 D4 part B; host/web/look.mjs): the shell's own sheet, or the neutral one after it.
// A fixed `userInterfaceStyle` is the root's `color-scheme` there, and the head's meta below.
const shell = withInterfaceStyle(withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')), interfaceStyle(manifest));
const base = shell.match(/<style>([\s\S]*?)<\/style>/)[1]
  .replace(/\/\*[\s\S]*?\*\//g, '').replace(/\s*\n\s*/g, '').replace(/\s*([{};,>])\s*/g, '$1').replace(/:\s+/g, ':').replace(/;}/g, '}');
const attr = (t) => String(t).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');
// The manifest's favicons and Apple touch icon, else the shell's default favicon (LLP 1163 D7).
const icon = webIconLinks(manifest, appDir, shell.match(/<link rel="icon"[^>]*>/)[0]);
const css = readFileSync(resolve(gen, 'app.css'), 'utf8');
const viewport = existsSync(resolve(gen, 'viewport.txt')) ? readFileSync(resolve(gen, 'viewport.txt'), 'utf8') : 'width=device-width, initial-scale=1';
// Declared fonts: a preload each, from the head (LLP 1019; the faces are in app.css).
const fonts = existsSync(resolve(gen, 'preloads.html')) ? readFileSync(resolve(gen, 'preloads.html'), 'utf8') : '';
// modulepreload fills the document's module map without evaluating the pieces
// that import() activates later, even when HTTP caching is disabled.
const preloads = fonts + preloadModules.filter(f => !args.includes('--inline') || f !== 'app.js')
  .map(f => `<link rel="modulepreload" href="./${f}">\n`).join('');
// A rendered page carries only the rules its document can match (page-style.mjs, LLP 1048.007 D4); the runtime adds
// the whole sheet before its first render (main.js), from style.css.
writeFileSync(resolve(out, 'style.css'), base + css);
writeFileSync(resolve(gen, 'page-style.json'), JSON.stringify(analyze(base + css)));
// The app's part of the render server's CSP (LLP 1048.000 D11), which it reads at start.
const cspFile = webCspFile(manifest);
if (cspFile) writeFileSync(resolve(gen, 'csp.json'), cspFile);
// app.json's `host.web.head` goes after the sheet, where no search for the shell's places (its sheet, preloads and
// title) can find a tag in an entry's text; every page composed over the shell keeps it (LLP 1048.000 D3).
writeFileSync(resolve(out, 'index.html'), `<!doctype html>
<html lang="${attr(webManifest.lang)}">
<meta charset="utf-8">
<base href="/">
<title>${attr(manifest.name)}</title>
<meta name="viewport" content="${viewport}">
${colorSchemeMeta(interfaceStyle(manifest))}${icon}
${preloads}<style>${base}${css}</style>
${webHead(manifest)}<div id="exact-root"${manifest.audio_session ? ` data-audio-session="${manifest.audio_session}"` : ''}></div>
${args.includes('--inline') ? `<script type="module">${readFileSync(resolve(out, 'app.js'), 'utf8').replaceAll('</script', '<\\/script')}</script>` : '<script type="module" src="./app.js"></script>'}
`);
if (production && !opt('--plan')) {
  // No bake, so no envelope to link: the web manifest written above, as a bake's page links it.
  const links = ['<link rel="manifest" href="./manifest.json">', ...webThemeColors(manifest)];
  writeFileSync(resolve(out, 'index.html'), readFileSync(resolve(out, 'index.html'), 'utf8').replace(/(<meta name="viewport"[^>]*>\n)/, `$1${links.map(l => l + '\n').join('')}`));
} else if (production) {
  const bake = dirname(resolve(opt('--plan')));
  const links = readFileSync(resolve(bake, 'index.html'), 'utf8').match(/^<(?:link rel="(?:alternate|manifest)"|meta name="theme-color")[^>]*>$/gm) ?? [];
  writeFileSync(resolve(out, 'index.html'), readFileSync(resolve(out, 'index.html'), 'utf8').replace(/(<meta name="viewport"[^>]*>\n)/, `$1${links.map(l => l + '\n').join('')}`)); // its `<html lang>` is already the manifest's, as the bake's is
  for (const f of ['exact.json', 'assets.json', 'manifest.json', 'robots.txt', 'sitemap.xml', '.well-known', '.exact', 'rust', 'plans', 'activities']) if (existsSync(resolve(bake, f))) cpSync(resolve(bake, f), resolve(out, f), { recursive: true });
  // What the envelope names beside the plan and assets, for a native client
  // following the page's link (LLP 1023 D1): the Rust module's files (above,
  // the bytes this page loads) and a TypeScript module's, under `module/`,
  // since this root's `app.js` is its runtime.
  const envelope = existsSync(resolve(bake, 'exact.json')) && JSON.parse(readFileSync(resolve(bake, 'exact.json'), 'utf8'));
  if (envelope?.module) {
    mkdirSync(resolve(out, 'module'), { recursive: true });
    for (const card of Object.values(envelope.module)) {
      const name = card.url.replace(/^\.\//, '');
      cpSync(resolve(bake, name), resolve(out, 'module', name));
      card.url = `./module/${name}`;
    }
    writeFileSync(resolve(out, 'exact.json'), JSON.stringify(envelope) + '\n');
  }
}
// The web's auth callback page (host/web/build.mjs does the same for the wasm target).
if (auth) {
  mkdirSync(resolve(out, '.exact/auth'), { recursive: true });
  cpSync(resolve(root, 'host/web/auth-callback.html'), resolve(out, '.exact/auth/callback'));
  cpSync(resolve(root, 'host/web/auth-callback.js'), resolve(out, '.exact/auth/callback.js'));
  const { authClientMetadata } = await import('../../scripts/app.mjs');
  const callbacks = grants.split('\n').map(l => l.trim()).filter(l => l.startsWith('auth.callback ')).map(l => l.slice(14).trim());
  const docs = authClientMetadata({ origin: manifest.app?.origin ?? null, displayName: manifest.app?.name ?? manifest.name, manifest }, callbacks);
  for (const [name, doc] of Object.entries(docs)) writeFileSync(resolve(out, `.exact/auth/${name}.json`), JSON.stringify(doc, null, 2) + '\n');
}
// The leaf modules the plan uses, built at once: Markdown's pieces, the
// motion engine (host/web-js/motion), the Markdown editor's rules
// (exact-markdown-editor) and the exclusions walker (exact-textflow's
// `textflow-web`), each beside its chunk.
await Promise.all([['markdown', buildMarkdown, 'markdown.wasm'], ['motion', buildMotion, 'motion.wasm'], ['editor', buildEditor, 'markup-editor.wasm'], ['flow', buildFlow, 'textflow.wasm']]
  .filter(([flag]) => existsSync(resolve(gen, `${flag}.flag`))).map(async ([, build, name]) => cpSync(await build(), resolve(out, name))));
// The app's GPU module (LLP 1009 D2), built here as the wasm target's build
// makes it, with the web host's glue: a loaded capability.
// Built again only when something a module was built from changed (Cargo's
// dep-info, module.mjs `fresh`): the bindings and wasm-opt with it.
const literalModuleURLs = code => [...code.matchAll(/new\s+URL\(\s*['"]([^'":/#][^'"]*)['"]\s*,\s*import\.meta\.url\s*\)/g)].map(match => match[1]);
const moduleImports = code => new Bun.Transpiler({ loader: 'js' }).scanImports(code).map(entry => entry.path).filter(path => path.startsWith('.'));
const publicDependency = (from, specifier) => posix.resolve('/', posix.dirname(from), specifier).slice(1);
async function copyLazyModules(roots) {
  const { webHostFiles } = await import('../../scripts/app.mjs');
  // These modules were built for this JS runtime and have already passed its
  // production gate. The wasm inventory belongs to a different build.
  const inventory = { ...webHostFiles(),
    'navigation.js': resolve(gen, 'navigation.js'),
    'grant-admission.js': resolve(gen, 'grant-admission.js'),
    'grant-network.js': resolve(gen, 'grant-network.js'),
  }, queue = roots.map(name => ({ name, source: inventory[name] })), copied = new Set();
  while (queue.length) {
    const { name, source } = queue.shift();
    if (copied.has(name)) continue;
    copied.add(name);
    if (!source) {
      if (!existsSync(resolve(out, name))) throw new Error(`lazy module dependency ${name} has no build source`);
      continue;
    }
    const from = resolve(root, source), code = name.endsWith('.js') || name.endsWith('.mjs') ? readFileSync(from, 'utf8') : null;
    mkdirSync(dirname(resolve(out, name)), { recursive: true });
    // The adapters' own storage-environment.js names the app's development store, as the page's chunk does (above).
    const scoped = code == null || from.startsWith(gen + '/') ? code
      : production ? gateAgent(code, name) : developmentStore(code, realpathSync(appDir));
    if (scoped !== code) writeFileSync(resolve(out, name), scoped);
    else cpSync(from, resolve(out, name));
    if (code != null) for (const specifier of [...moduleImports(code), ...literalModuleURLs(code)]) {
      const dependency = publicDependency(name, specifier);
      if (dependency.startsWith('..')) throw new Error(`${name}: lazy dependency leaves the build root: ${specifier}`);
      const sibling = resolve(dirname(from), specifier);
      queue.push({ name: dependency, source: inventory[dependency] ?? (existsSync(sibling) ? sibling : null) });
    }
  }
}

if (existsSync(gpuLib)) {
  await copyLazyModules(['gpu-glue.js']);
}
if (pageModules) cpSync(moduleDirectory(appDir, 'web'), resolve(out, 'modules'), { recursive: true });
// Launch modules' web services, which marks.js imports after startup.
for (const m of launchWeb) cpSync(m.web, resolve(out, 'launch', m.name, 'service.js'));
copyStaticTreeIfPresent(resolve(appDir, 'assets'), resolve(out, 'assets'));
// Copy each imported package's declared web assets from the capture, to /assets/<package>/<path> (LLP 1154 D2;
// js/bake/src/typescript.mjs `capturedSources`).
if (ts) for (const { name, path, file } of (await import(resolve(root, 'js/bake/src/typescript.mjs'))).capturedAssets(tsStage)) {
  mkdirSync(dirname(resolve(out, 'assets', name, path)), { recursive: true });
  cpSync(file, resolve(out, 'assets', name, path));
}
copyStaticTreeIfPresent(resolve(appDir, 'deck'), resolve(out, 'deck'));
if (devReload) writeFileSync(resolve(out, '.exact-dev-logic.json'), JSON.stringify({ version: 1, modules: devLogic.sort(([a], [b]) => a.localeCompare(b)) }) + '\n');
// The web host's own picker and storage adapters beside the page, fetched on
// first use (files.js; a source's `storage`, ts-data.js and rust-data.js; an
// `app:/` image's file, symbols.js, or media element's, media.js; a module
// implementation's files, modules.js; a module view's `app:/` file, native.js):
// what host/web/build.mjs ships.
const extensionFiles = Object.keys(moduleFlag.extensions).length > 0;
if (files || moduleStorage || extensionFiles || pageModules || /^\s*(?:fs|sqlite)\./m.test(grants)) {
  const storageGrants = /^\s*(?:fs|sqlite)\./m.test(grants);
  const seeds = [resolve(gen, 'symbols.js'), files && resolve(gen, 'files.js'), (moduleStorage || storageGrants) && resolve(gen, 'admission.js'), storageGrants && resolve(gen, 'ts-data.js'),
    extensionFiles && resolve(gen, 'modules.js'), (pageModules || extensionFiles) && resolve(gen, 'native.js')].filter(Boolean);
  const roots = seeds.flatMap(seed => literalModuleURLs(readFileSync(seed, 'utf8'))).map(specifier => specifier.replace(/^\.\//, ''));
  await copyLazyModules(roots);
}
// The plan beside the pages: a render server (either renderer) reads it.
cpSync(opt('--plan') ? resolve(opt('--plan')) : resolve(gen, 'app.plan'), resolve(out, 'app.plan'));
writeFileSync(resolve(out, 'app.version.json'), JSON.stringify(version) + '\n');
// A development build's source map for that plan, for the agent driver only
// (LLP 1012.001.000 D6): never in a production build, never a stale one.
rmSync(resolve(out, 'app.plan.map.json'), { force: true });
if (!production && !opt('--plan') && existsSync(resolve(gen, 'app.plan.map.json'))) cpSync(resolve(gen, 'app.plan.map.json'), resolve(out, 'app.plan.map.json'));
// The install pages and their data (LLP 1030.003 D6a), as the wasm build
// writes them (a release over a bake carries its bake's, above). The build is
// named by the program's bytes: the plan and the page's module.
if (!production || !opt('--plan')) {
  const [{ readManifest }, { writeInstallPages }] = await Promise.all([import('../../scripts/app.mjs'), import('../../scripts/install-page.mjs')]);
  // A release is `--production`'s, so otherwise this is a development build
  // whatever the shell's EXACT_UPDATE_TRUST; with no bake receipt its reach is unknown.
  const [source, changes] = await revision ?? [null, null];
  const id = createHash('sha256').update(readFileSync(resolve(out, 'app.plan'))).update(readFileSync(resolve(out, 'app.js'))).digest('hex');
  writeInstallPages(out, readManifest(appDir, app), { id, source, dirty: changes === null ? null : !!changes, builtAt: new Date().toISOString(),
    mode: production ? 'Release build' : 'Development build' });
}
// The type check, which ran while the page bundled: its diagnostics are the
// native bake's (host/web/build.mjs keeps them in its summary).
const typeError = await captured?.typed;
if (typeError) console.error(`${typeError.message ?? typeError}\nerror: ${app}: app.ts failed the type check every host's build runs`);
sayWarnings(captured?.warnings);
if (typeError) process.exit(1);
// Pages at build (LLP 1048.000): `--render rust` (the default where the app
// has one) runs the app's native render entry (`<app>-render`, exact_render)
// over this shell; `--render js` runs this runtime under Bun (render.mjs).
// Either page adopts. `--render none` renders nothing: the dev loop's, where
// every route is the shell and the page builds itself (a render entry
// rebuilds on every Contract edit, since its crate bakes the plan).
const pages = JSON.parse(readFileSync(resolve(gen, 'pages.json'), 'utf8'));
// The shell, before a rendered home page replaces index.html: what a host
// serves for a route with no page of its own, and what a page is composed
// over (a render server's, too). And the route table a host serves by
// (LLP 1153 D3: host/web/serve.mjs `buildTreeFile` applies it).
cpSync(resolve(out, 'index.html'), resolve(out, 'shell.html'));
const { planRouteTable } = await import('../web/serve.mjs');
const routes = planRouteTable(readFileSync(resolve(out, 'app.plan'))).map(({ name, pattern, render, notfound }) => ({ name, pattern, render, notfound }));
writeFileSync(resolve(out, 'routes.json'), JSON.stringify({ routes }, null, 2) + '\n');
// Compile known reader text even when the development build emits only a shell.
if (markdownReaders) {
  const { markdownAt, renderer } = await import('./render.mjs');
  const prepared = await renderer(out, { closed: true }).markdown();
  if (prepared.length) {
    const data = JSON.stringify(prepared).replace(/</g, '\\u003c');
    const tag = `<script type="application/vnd.exact.markdown">${data}</script>`;
    // Before the shell's own entry, not the first module script: a head entry may be one, or a script's text spell one.
    for (const file of ['index.html', 'shell.html']) {
      const path = resolve(out, file), html = readFileSync(path, 'utf8'), at = markdownAt(html);
      if (at >= 0) writeFileSync(path, html.slice(0, at) + tag + html.slice(at));
    }
    console.log(`${app}: ${prepared.length} Markdown sources compiled`);
  }
}
if (pages.length && !opt('--render') && !renderAt) console.log(`${app}: no ${renderBin} entry, so its pages render under Bun (--render js)`);
if (pages.length && how === 'rust') {
  const bin = renderBin, at = renderAt;
  if (!at) { console.error(`--render rust: ${app} has no ${bin} entry; use --render js`); process.exit(1); }
  const plan = resolve(out, 'app.plan');
  // The render entry runs here as a build tool, in the app's own workspace and target (an authored app outside the
  // repository has its own, LLP 1153 D1.4): the development profile, which the Linux host an agent drives shares.
  const renderApp = resolveApp(appDir);
  const r = spawnSync('cargo', ['run', ...injectedProfiles(renderApp), '--profile', 'host-dev', '-q', '-p', at[1], '--bin', bin, '--', '--plan', plan, '--name', manifest.name, ...(manifest.app?.origin ? ['--origin', manifest.app.origin] : []), '--shell', resolve(out, 'index.html'), '--build'],
    { cwd: renderApp.workspace, encoding: 'utf8', maxBuffer: 256 << 20, env: { ...process.env, CARGO_TARGET_DIR: renderApp.target, EXACT_UPDATE_TRUST: 'development' } });
  if (r.status !== 0) { console.error(r.stderr); process.exit(1); }
  const docs = r.stdout.split('\n').filter(Boolean).map(l => JSON.parse(l));
  for (const doc of docs) {
    if (doc.error) { console.error(`${bin} ${doc.location}: ${doc.error}`); process.exit(1); }
    const file = doc.notfound ? '404.html' : `${decodeURIComponent(doc.location).replace(/^\/|\/$/g, '')}/index.html`.replace(/^\//, '');
    mkdirSync(dirname(resolve(out, file)), { recursive: true });
    writeFileSync(resolve(out, file), doc.page);
  }
  writeCrawlerFiles(out, docs, manifest.app?.origin, routes);
  console.log(`${out}: ${pages.length} pages rendered by ${bin}`);
} else if (pages.length && how === 'js') {
  const r = spawnSync('bun', [resolve(here, 'render.mjs'), out, '--build'], { cwd: root, stdio: 'inherit' });
  if (r.status !== 0) process.exit(1);
} else if (how !== 'none' && routes.length) {
  writeCrawlerFiles(out, [], null, routes); // robots.txt alone: no page made at build to list
}
console.log(`${out}: built`);
