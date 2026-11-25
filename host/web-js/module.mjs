// An app's Rust data source as a standalone logic module (LLP 1029.000,
// ABI 12) for the JS runtime, when the app declares none of its own: a
// generated crate exporting the source app.json's `rust.data` names (LLP
// 1153.003 D2), which the native hosts bake with too.
// usage: bun host/web-js/module.mjs <app> [--out <dir>] → <dir>/app.module.wasm
//        bun host/web-js/module.mjs --prebuild: what every app's web build
//        shares (the compiler, the leaf modules), compiled and kept for the
//        machine, so no app's first build compiles them.
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { gzipSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';
// Also for its PATH: the pinned binaryen (and Cargo) as every web build has
// them, so a prebuild optimizes and keeps what an app's build would.
import { awaitBuildOutput, cargoCheckout, refuseStdLockfile, WEB_STD, WEB_SIZE, WEB_TOOLCHAIN, injectedProfiles, resolveApp, wasmRemapFlags, webToolchainEnv } from '../../scripts/app.mjs';
import { rustDataOf, undeclaredDataCrate } from '../../scripts/app-shells.mjs';
import { appCrates, linkedCrate } from '../../scripts/cargo-check.mjs';
import { SETUP_FETCHES, withHostDeployment } from '../../scripts/filesystem.mjs';
import { withoutBase } from './route-pattern.js';
import { buildTreeFile, replaceBuild, webContentType } from '../web/serve.mjs';
import { webPackageBindingChanged } from '../../js/bake/src/package-config.mjs';
import { proofInputs } from '../../game/inputs.mjs';
import { keptProducts } from '../../scripts/kept.mjs';
import { RustMissing, compilerFrom, fromSet, fromSets, fetchedTool, launch, rustMissing, freshCompiler } from '../../scripts/tools.mjs';
import { EXE, slashRelative, within } from '../../scripts/paths.mjs';
// Most tables use only ECMAScript at import time. Detect a host read even under
// `typeof` or a caught exception, so an answer for the build VM never qualifies a browser-only branch.
async function scriptModuleTags(dist, base, factoryTags) {
  const vm = await import('node:vm');
  const options = { strings: false, wasm: false };
  const properties = vm.runInNewContext('Object.getOwnPropertyDescriptors(globalThis)', {}, { contextCodeGeneration: options });
  delete properties.globalThis;
  let hostRead = false;
  const context = vm.createContext(new Proxy(Object.defineProperties(Object.create(null), properties), {
    get(target, key) {
      if (typeof key === 'string' && !Object.hasOwn(target, key)) {
        hostRead = true;
        throw new ReferenceError(key);
      }
      return Reflect.get(target, key);
    },
  }), { codeGeneration: options });
  const origin = 'https://exact.invalid', modules = new Map();
  function load(url) {
    if (modules.has(url.href)) return modules.get(url.href);
    const path = url.origin === origin && withoutBase(url.pathname, base);
    const file = path && buildTreeFile(dist, path);
    if (!file) throw new Error(url.href);
    const module = new vm.SourceTextModule(readFileSync(file.path, 'utf8'), {
      context, identifier: url.href, initializeImportMeta(meta) { meta.url = url.href; },
    });
    modules.set(url.href, module);
    return module;
  }
  let timer;
  try {
    const module = load(new URL(base + 'modules/index.js', origin));
    await module.link((specifier, parent) => {
      if (!/^(?:\.{1,2}\/|\/)/.test(specifier)) throw new Error(specifier);
      return load(new URL(specifier, parent.identifier));
    });
    await Promise.race([module.evaluate({ timeout: 2000 }), new Promise((_, reject) => { timer = setTimeout(reject, 2000); })]);
    const tags = factoryTags(module.namespace);
    return hostRead ? null : tags;
  } catch { return null; } finally { clearTimeout(timer); }
}

/** Read the table in the staged page when its initialization needs a browser. */
export async function webModuleTags(dist, base, required, release) {
  const entry = resolve(dist, 'modules/index.js');
  if (!existsSync(entry)) return [];
  let view, server, timer;
  try {
    const { factoryTags } = await import('../../scripts/launch-modules.mjs');
    const tags = await scriptModuleTags(dist, base, factoryTags);
    if (tags) return tags;
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const url = new URL(request.url);
      const path = url.origin === server.url.origin && withoutBase(url.pathname, base);
      const file = path && buildTreeFile(dist, path, request.headers.get('Accept') ?? '');
      if (!file) return new Response(null, { status: 404 });
      return new Response(Bun.file(file.path), { headers: {
        'Content-Type': webContentType(file.route),
        'Content-Security-Policy': "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; style-src 'self' 'unsafe-inline'",
      } });
    } });
    let backend = 'webkit';
    if (process.platform !== 'darwin') {
      const { chromium } = await import('../../scripts/agent-launch.mjs');
      const chrome = chromium();
      if (chrome.unavailable) throw new Error(chrome.unavailable);
      backend = { type: 'chrome', path: chrome.executable };
    }
    view = new Bun.WebView({ backend });
    const read = async () => {
      await view.navigate(new URL(base, server.url).href);
      return view.evaluate(`(async () => (${factoryTags})(await import(new URL('./modules/index.js', document.baseURI))))()`);
    };
    return await Promise.race([read(), new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('module initialization timed out')), 15000);
    })]);
  } catch (cause) {
    const message = `web: cannot read the factory roster for ${required.join(', ')}: ${cause.message}`;
    if (release) throw new Error(message, { cause });
    console.warn(`warning: ${message}`);
    return null;
  } finally { clearTimeout(timer); view?.close(); server?.stop(true); }
}

/** The development URL for frames/<name>.contract. */
export const frameURL = name => '/__dev/frames/' + encodeURIComponent(name).replaceAll("'", '%27');

export const frameFixturePath = frame => frame.slice(0, -'.contract'.length) + '.fixture.mts';

/** Only the selected frame's fixture enters its TypeScript capture. */
export function frameFixture(app, frame) {
  if (!frame) return null;
  const path = frameFixturePath(frame), stat = lstatSync(path, { throwIfNoEntry: false });
  if (!stat) return null;
  const root = realpathSync(app), physical = realpathSync(path);
  if (!stat.isFile() || !within(root, physical)) throw new Error(`${path}: a frame fixture must be a regular file inside the app`);
  return slashRelative(root, physical);
}

/** The preview's dispatcher accepts the app's TypeScript sources and calls introduced by the frame.
 * The app retains its grants, hooks and original handler for every unanswered call. */
export function frameAnswer(file, types) {
  return `import * as app from './app';
${types}
import fixture from ${JSON.stringify('./' + file)};
import type { SourceMap as AppSources, TypeScriptSources } from './app.contract.d.ts';
export * from './app';
type Providers = { [S in TypeScriptSource]?: (args: Args<S>, store: Store, storage: Storage, native?: NativeModule | null) => Result<S> | Promise<Result<S>> };
type Allowed = keyof TypeScriptSources | Exclude<TypeScriptSource, keyof AppSources>;
const checked: Providers & Partial<TypeScriptSources> = fixture satisfies Record<Exclude<keyof typeof fixture, Allowed>, never>;
const overrides: Providers = checked;
const live: TypeScriptAnswer = app.answer;
export const answer: TypeScriptAnswer = (source, args, store, storage, native) => {
  const provide = Object.hasOwn(overrides, source) ? overrides[source] : undefined;
  return provide ? provide(args, store, storage, native) : live(source, args, store, storage, native);
};`;
}

export { EXE };
import { buildScriptDir, crateDir, depInfoOf, fresh as cargoFresh, refreshCargoRustBindings } from '../../scripts/cargo-dep-info.mjs';
export { buildScriptDir, crateDir, depInfoOf, freshCompiler };
export const fresh = (product, dep, also = []) => cargoFresh(product, dep, also);
import { BINARYEN_DOWNLOAD } from '../web/stages.mjs';

// ECMA-426 §6 and §9.2. Only the generated column and source index are needed to charge a span to its module.
function vlq(segment, at) {
  const digits = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  let value = 0, place = 1, digit;
  do {
    digit = digits.indexOf(segment[at++]);
    if (digit < 0) throw new Error('invalid source-map VLQ');
    value += (digit & 31) * place;
    place *= 32;
  } while (digit & 32);
  return [value % 2 ? -Math.floor(value / 2) : value / 2, at];
}

/** Module shares of the minified launch script. A line ending, a one-field segment or text before a line's first
 * mapping has no source. Columns count UTF-16 units. A character split by a mapping belongs to its starting span. */
export function entrySizeReport(chunk, out) {
  const map = chunk.map, mappings = map.mappings.split(';'), counts = new Map();
  const lines = chunk.code.split(/(\r\n|[\r\n\u2028\u2029])/);
  const add = (source, bytes) => { if (bytes) counts.set(source, (counts.get(source) ?? 0) + bytes); };
  let source = 0;
  for (let i = 0; i < lines.length; i += 2) {
    const line = lines[i];
    let column = 0, start = 0, owner = null;
    for (const segment of (mappings[i / 2] || '').split(',')) {
      if (!segment) continue;
      const [advance, at] = vlq(segment, 0);
      column += advance;
      const low = line.charCodeAt(column), high = line.charCodeAt(column - 1);
      const end = low >= 0xdc00 && low <= 0xdfff && high >= 0xd800 && high <= 0xdbff ? column + 1 : column;
      add(owner, Buffer.byteLength(line.slice(start, end)));
      start = end;
      if (at < segment.length) { source += vlq(segment, at)[0]; owner = map.sources[source]; }
      else owner = null;
    }
    add(owner, Buffer.byteLength(line.slice(start)));
    if (lines[i + 1]) add(null, Buffer.byteLength(lines[i + 1]));
  }
  const gen = realpathSync(resolve(out, '.gen'));
  const moduleName = source => {
    const path = resolve(out, source);
    return relative(gen, existsSync(path) ? realpathSync(path) : path);
  };
  const bytes = Buffer.from(chunk.code), rows = [...counts].sort((a, b) => b[1] - a[1]);
  return [`Launch script ${chunk.fileName}, ${bytes.length} B (${gzipSync(bytes).length} B gzip)`, 'Minified bytes by source module',
    ...rows.map(([source, n]) => `  ${n} B  ${source == null ? '(unmapped)' : moduleName(source)}`)].join('\n');
}

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
// The web toolchain's size build (scripts/app.mjs WEB_TOOLCHAIN): std built
// for size, panics abort without text, and no path of this machine or
// checkout in the bytes (`wasmRemapFlags`, as every web-profile wasm build).
// Asked of rustc once, when a module is built.
const WEB = WEB_TOOLCHAIN;
let wasmFlags;
const WASM_FLAGS = () => (wasmFlags ??= [...WEB_STD, ...wasmRemapFlags(null, WEB)]);
// A wasm link's flags: the toolchain is checked once first, so a machine that
// cannot link is refused with its install command (scripts/cargo-check.mjs).
let linkChecked = false;
const LINK_FLAGS = () => { if (!linkChecked) { webToolchainEnv(process.env, { linksWasm: true }); linkChecked = true; } return WASM_FLAGS(); };
// Each directory a WEB_STD build runs Cargo in, under each lockfile variable, once Cargo's configuration there names
// no lockfile (`refuseStdLockfile`).
const stdChecked = new Set();
const checkStd = (cwd, what) => {
  const key = `${cwd}\0${process.env.CARGO_RESOLVER_LOCKFILE_PATH ?? ''}`;
  if (!stdChecked.has(key)) { refuseStdLockfile(process.env, cwd, what); stdChecked.add(key); }
};
// What those flags say apart from this checkout's path, for a kept module's group.
const WASM_FLAGS_KEY = () => WASM_FLAGS().map(f => f.replaceAll(root, '.'));
// Under this checkout's own target/ (AGENTS.md: never another checkout's):
// a module built from one worktree's sources is never another's.
const TARGET_DIR = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : resolve(root, 'target');
const MODULES = resolve(TARGET_DIR, 'web-js-modules');
const TARGET = resolve(MODULES, 'target');
// A leaf module's own target directory: the leaves build at once, and one
// Cargo holds a target directory's lock for the whole of its build.
const leafTarget = (name) => resolve(MODULES, `target-${name}`);

// A leaf module, and the compiler, hold nothing of an app: one some checkout
// of this machine compiled from these bytes is taken (scripts/kept.mjs,
// `~/.cache/exact/web-modules`), and one compiled here is kept. A production
// build takes one too: a leaf is built at the same release profile either way,
// and an entry is taken only for the same bytes of every input.
const kept = keptProducts({ root, cache: 'web-modules', env: process.env, skip: [TARGET_DIR], label: 'web-js' });
// A leaf is kept as `wasm-opt` left it: which one ran is part of what it is.
let wasmOpt;
const WASM_OPT = () => (wasmOpt ??= spawnSync('wasm-opt', ['--version'], { encoding: 'utf8' }).stdout ?? null);
// What a prebuild left in the cache, by product: whether it is kept for these sources.
const held = new Map();

// `wasm-opt -Oz` from `input` to `output`, or, when binaryen is missing or
// fails, `input` copied there unoptimized and said so, as host/web/build.mjs
// does: a missing tool is named, never left for a later ENOENT to report.
const unoptimized = new Set();
const OPT = ['-Oz', '--enable-bulk-memory', '--enable-nontrapping-float-to-int', '--enable-sign-ext', '--enable-mutable-globals'];
function optimized(opt, input, output, label) {
  if (opt.status === 0) return output;
  // Not optimized: a leaf is not kept for other checkouts (`buildLeaf`).
  unoptimized.add(output);
  if (input !== output) cpSync(input, output);
  // `warning: ` so the web build's filter of this piped child's lines passes it on (LLP 1156.006.000 D10).
  process.stderr.write(`warning: ${label}: ${opt.error?.code === 'ENOENT' ? `wasm-opt not on PATH (binaryen: brew install binaryen, or ${BINARYEN_DOWNLOAD})` : `wasm-opt failed: ${(opt.stderr ?? '').trim().split('\n').at(-1)}`}; shipped unoptimized\n`);
  return output;
}
function optimize(input, output, label) {
  return optimized(spawnSync('wasm-opt', [...OPT, input, '-o', output], { stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' }), input, output, label);
}
/** `spawnSync`'s answer, without blocking: `{ status, error, stderr }`. */
function run(cmd, args, options, capture = false) {
  return new Promise(done => {
    const child = spawn(cmd, args, { ...options, stdio: capture ? ['ignore', 'ignore', 'pipe'] : ['ignore', 'inherit', 'inherit'] });
    let stderr = '';
    child.stderr?.on('data', d => { stderr += d; });
    child.on('error', error => done({ status: null, error, stderr }));
    child.on('close', status => done({ status, stderr }));
  });
}
const optimizeAsync = async (input, output, label) => optimized(await run('wasm-opt', [...OPT, input, '-o', output], {}, true), input, output, label);
// Every build of this checkout that uses a module copies it from here, and
// builds run at once (conformance's --jobs), while a module taken from the
// machine's cache is copied here again at each build. So a module is written
// beside its place by `write` and renamed onto it: a reader copies the old
// bytes or the new, never a file still being written (a page got an empty
// motion.wasm, had no motion engine, and its drags did nothing).
function replace(path, write) {
  const temp = `${path}.${process.pid}.tmp`, clean = () => rmSync(temp, { force: true });
  const land = (value) => { try { renameSync(temp, path); return value; } finally { clean(); } };
  let written;
  try { written = write(temp); } catch (error) { clean(); throw error; }
  return written instanceof Promise ? written.then(land, (error) => { clean(); throw error; }) : land(written);
}
const cargoWasmArgs = (manifest, profile = ['--release']) => [`+${WEB}`, 'build', ...profile, '--target', 'wasm32-unknown-unknown', '--manifest-path', manifest, ...LINK_FLAGS()];
function cargoWasm(manifest, args, profile) {
  return spawnSync('cargo', [...cargoWasmArgs(manifest, profile), ...args], { stdio: ['ignore', 'inherit', 'inherit'], env: { ...process.env, CARGO_TARGET_DIR: TARGET, EXACT_RUST_BINDINGS: '1' } });
}
// The dev loop's app module: the profile a build ships (`inherits`) without whole-module LTO, in parallel units and
// incremental, so an edit recompiles the data crate and relinks. It is given on Cargo's command line, so no manifest,
// and no other build's arguments or cache keys, carry it. The page takes it without `wasm-opt`, marked so that no
// production build ships it (`moduleGrants`).
export const DEV_PROFILE = 'web-dev';
const devProfile = inherits => [['inherits', inherits], ['opt-level', 1], ['lto', false], ['codegen-units', 16], ['incremental', true]]
  .flatMap(([key, value]) => ['--config', `profile.${DEV_PROFILE}.${key}=${JSON.stringify(value)}`]).concat('--profile', DEV_PROFILE);
const DEV_MARK = 'exact.dev-module';
/** A custom section named DEV_MARK (WebAssembly core spec 5.5.3), appended to a module. */
function devMarked(bytes) {
  const leb = n => { const out = []; do { out.push((n & 0x7f) | (n > 0x7f ? 0x80 : 0)); n >>>= 7; } while (n); return out; };
  const name = [...new TextEncoder().encode(DEV_MARK)], body = [...leb(name.length), ...name];
  return Buffer.concat([bytes, Buffer.from([0, ...leb(body.length), ...body])]);
}
/** The dep-info and generated wrapper of each module, by its staged path. */
const moduleDepInfo = new Map();
/** The module's package directories for the dev watcher (LLP 1153.003 D1.6).
 * The generated wrapper is a build output. */
export function moduleSourceDirs(built) {
  try {
    const { depInfo, wrapper } = moduleDepInfo.get(built);
    const text = readFileSync(depInfo, 'utf8');
    const files = text.slice(text.indexOf(': ') + 2).trim().split(/(?<!\\)\s+/).map(f => f.replace(/\\ /g, ' '));
    const dirs = [...new Set(files.map(crateDir).filter(Boolean))];
    if (!wrapper) return dirs;
    return dirs.filter(dir => {
      try { return realpathSync(dir) !== wrapper; } catch { return true; }
    });
  } catch { return []; }
}
/** An unchanged generated crate keeps its mtime for Cargo. */
const keep = (path, text) => { if (!existsSync(path) || readFileSync(path, 'utf8') !== text) writeFileSync(path, text); };
/** A generated crate's `Cargo.lock`: the workspace's, again whenever the
 * workspace's changes, so it builds the versions the workspace pins (and
 * that a kept module's group names). */
function lockFrom(crate) {
  const lock = readFileSync(resolve(root, 'Cargo.lock')), from = resolve(crate, 'Cargo.lock.from'), id = createHash('sha256').update(lock).digest('hex');
  if (existsSync(resolve(crate, 'Cargo.lock')) && existsSync(from) && readFileSync(from, 'utf8') === id) return;
  writeFileSync(resolve(crate, 'Cargo.lock'), lock);
  writeFileSync(from, id);
}

const LOCKFILE_KEY = 'resolver.lockfile-path';

// Cargo prunes unused pins. Start each resolution with Exact's full lockfile
// and preserve the build lockfile's mtime when the result is identical.
function resolveDevLock(lock, readMetadata) {
  const dir = mkdtempSync(resolve(dirname(lock), 'resolve-'));
  try {
    const seed = resolve(dir, 'Cargo.lock');
    writeFileSync(seed, readFileSync(resolve(root, 'Cargo.lock')));
    const metadata = readMetadata(['--config', `${LOCKFILE_KEY}=${JSON.stringify(seed)}`]);
    const resolved = readFileSync(seed);
    if (!existsSync(lock) || !resolved.equals(readFileSync(lock))) replace(lock, temp => writeFileSync(temp, resolved));
    return metadata;
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

/** Resolve the module's graph, refreshing a dev wrapper's default lockfile from Exact's pins. */
export function moduleMetadata(manifest, env, dev = false) {
  const readMetadata = (args = []) => {
    const result = spawnSync('cargo', [`+${WEB}`, 'metadata', '--offline', '--format-version', '1', '--manifest-path', manifest, ...args],
      { env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    if (result.status !== 0) throw new Error(`module metadata: ${result.stderr || result.error?.message}`);
    return JSON.parse(result.stdout);
  };
  return { metadata: dev ? resolveDevLock(resolve(dirname(manifest), 'Cargo.lock'), readMetadata) : readMetadata(), locked: dev };
}

/** A built `app.js` past this is an order of magnitude over what the apps here build, and near what a host refuses
 * (a 25 MiB limit in one report): a guess, for a warning that fails nothing. */
export const APP_JS_WARN_BYTES = 8 << 20;
/** What a build says of a built `app.js` of `bytes` past that size, with the compiler's line of plan code by component
 * (`costs`) when it gave one; '' below it. Two apps reached 30 MB with no word (LLP 1156.006.000 D4). */
export const sizeWarning = (name, bytes, costs = '') => bytes <= APP_JS_WARN_BYTES ? ''
  : `warning: ${name} is ${(bytes / 2 ** 20).toFixed(1)} MiB. The plan expands each component use. Different mount bodies can add launch code. Add --sizes to the web build for bytes by source module.${costs ? ` ${costs}` : ''}`;
/** What separates two directories of one app name (two worktrees of an app) in the names made from it: twelve hex
 * of the directory's real path, as `host/web/dev.mjs` keys `dev-generations` (LLP 1156.006.000 D13). Every app's
 * module builds into one Cargo target, where one crate name from two directories is one product and one claim. */
export const dirKey = (dir) => createHash('sha256').update(realpathSync(dir)).digest('hex').slice(0, 12);

// A path as a TOML basic string. JSON's escapes are TOML's, so a Windows path's
// backslashes (`C:\Users`) are not read as escapes.
const tomlString = (path) => JSON.stringify(path);

// The workspace's `[patch.crates-io]` table, its vendor paths made absolute, under `checkout`, for a crate outside the workspace.
const workspacePatch = (ws, checkout = root) => ws.slice(ws.indexOf('[patch.crates-io]')).split('\n[')[0]
  .replace(/path = "(vendor[^"]*)"/g, (_, path) => `path = ${tomlString(resolve(checkout, path))}`);

/** Cargo's module at `wasm`, optimized to `<out>/app.module.wasm`, or for the dev loop (`dev`) as Cargo linked it. */
function module(manifest, name, out, label, writeInputs, dev = false) {
  const wasm = resolve(TARGET, 'wasm32-unknown-unknown', dev ? DEV_PROFILE : 'release', `${name}.wasm`), done = resolve(out, 'app.module.wasm');
  mkdirSync(out, { recursive: true });
  const crate = dirname(manifest);
  const release = awaitBuildOutput({ id: label, dir: crate }, resolve(TARGET, '.claims', `${name}.lock`));
  try {
    writeInputs?.();
    prepareModule(wasm, done, { manifest, targetDir: TARGET, profile: dev ? DEV_PROFILE : 'release', wrapper: dev && !!writeInputs, dev }, locked => {
      const args = locked ? ['--locked'] : [];
      if (cargoWasm(manifest, args, dev ? devProfile('release') : undefined).status !== 0) throw new Error(`${label} did not build`);
    });
    moduleDepInfo.set(done, { depInfo: wasm.replace(/\.wasm$/, '.d'), wrapper: writeInputs ? realpathSync(crate) : undefined });
    return done;
  } finally { release(); }
}

// A dev build's staging directory is replaced each time. Keep the optimized
// module beside Cargo's artifact so a new stage need not optimize it again.
// `wrapper`: a dev wrapper, whose lock `moduleMetadata` resolves; `dev`: the
// dev loop's module, served as Cargo linked it.
function prepareModule(wasm, done, { manifest, targetDir, profile, wrapper = false, dev = false }, build) {
  const cached = wasm.replace(/\.wasm$/, '.optimized.wasm'), from = `${cached}.from`;
  const env = { ...process.env, CARGO_TARGET_DIR: targetDir, EXACT_RUST_BINDINGS: '1' };
  const { metadata, locked } = moduleMetadata(manifest, env, wrapper);
  refreshCargoRustBindings(targetDir, 'wasm32-unknown-unknown', profile, metadata.packages, env, metadata.build_directory);
  build(locked);
  if (dev) return void replace(done, temp => writeFileSync(temp, devMarked(readFileSync(wasm))));
  // Cargo may restore an older artifact when features change back.
  const id = createHash('sha256').update(readFileSync(wasm)).update(JSON.stringify([WASM_OPT(), OPT])).digest('hex');
  if (!existsSync(cached) || !existsSync(from) || readFileSync(from, 'utf8') !== id) {
    replace(cached, temp => optimize(wasm, temp, 'module'));
    replace(from, temp => writeFileSync(temp, id));
  }
  replace(done, temp => cpSync(cached, temp, { preserveTimestamps: true }));
}

export function buildModule(app, out, draw = false, dir = resolve(root, 'apps', app), { dev = false } = {}) {
  checkStd(process.cwd(), app);
  const key = dirKey(dir);
  out ??= resolve(MODULES, `${app}-${key}`);
  const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
  if (manifest.rust?.module?.package) return ownModule(app, out, dir, dev);
  // The source app.json's rust.data names (LLP 1153.003 D2), exported from a crate of its own whose one dependency is
  // the data crate.
  const data = rustDataOf(manifest);
  if (!data) return ownModule(app, out, dir, dev);
  // Canvas 2D draws only for a plan that has a 2D surface (logic/abi/src/draw.rs). Beside app.ts, the module carries
  // the sources Rust answers (logic/abi/src/sources.rs).
  const lib = `exact_logic_abi::export!(${data.ty}, ${data.source});\n${draw ? `exact_logic_abi::export_draw!(${data.ty});\n` : ''}${existsSync(resolve(dir, 'app.ts')) ? `exact_logic_abi::export_sources!(${data.lib}::RUST_SOURCES);\n` : ''}`;
  const resolved = resolveApp(dir);
  // An app whose host crates exact2 generates has the module as a member of that workspace (`.exact/shells/module`),
  // built --locked against the app's resolution, so a data crate's dependencies resolve once.
  if (resolved.generated) {
    const workspace = resolved.rust(), crate = resolve(workspace, 'module');
    const manifestPath = resolve(crate, 'Cargo.toml'), name = Bun.TOML.parse(readFileSync(manifestPath, 'utf8')).package.name;
    const wasm = resolve(resolved.target, 'wasm32-unknown-unknown', dev ? DEV_PROFILE : 'web', `${name.replaceAll('-', '_')}.wasm`), done = resolve(out, 'app.module.wasm');
    mkdirSync(out, { recursive: true });
    moduleDepInfo.set(done, { depInfo: wasm.replace(/\.wasm$/, '.d') });
    const release = awaitBuildOutput(resolved, resolve(resolved.target, '.claims', `${name}.lock`));
    try {
      keep(resolve(crate, 'src/lib.rs'), lib);
      prepareModule(wasm, done, { manifest: manifestPath, targetDir: resolved.target, profile: dev ? DEV_PROFILE : 'web', dev }, () => {
        const args = [`+${WEB}`, 'build', '--locked', '--offline', ...dev ? devProfile('web') : ['--profile', 'web'], '--target', 'wasm32-unknown-unknown', '--manifest-path', manifestPath, ...injectedProfiles({ manifest }, workspace), ...LINK_FLAGS()];
        if (spawnSync('cargo', args, { stdio: ['ignore', 'inherit', 'inherit'], env: { ...process.env, CARGO_TARGET_DIR: resolved.target, EXACT_RUST_BINDINGS: '1' } }).status !== 0) throw new Error(`${app}: the module did not build; if Cargo says above that it could not download a crate offline, ${SETUP_FETCHES}`);
      });
      return done;
    } finally { release(); }
  }

  // The one the app's own host crates link by path, which they build, else the one beside app.json
  const dataCrate = linkedCrate(resolved.dir, data.krate) ?? appCrates(resolved.dir).find(crate => crate.name === data.krate)?.dir;
  if (!dataCrate) throw new Error(`${app}: app.json's rust.data.crate is ${data.krate}, which neither its host crates link by path nor is a crate beside app.json; name the data crate its host crates link, or link it by path from one of them`);
  const packageName = `exact-js-module-${app}-${key}${dev ? '-dev' : ''}`;
  // A stable path lets Cargo keep the dev wrapper fresh across stages. Its
  // own package name keeps Cargo from reusing a stage-local wrapper's artifact.
  const crate = dev ? resolve(MODULES, 'wrappers', packageName) : resolve(out, 'crate');
  mkdirSync(resolve(crate, 'src'), { recursive: true });
  // The data crate's workspace was authored, so the wrapper names the checkout as it does (scripts/paths.mjs).
  const checkout = cargoCheckout(resolved.dir, false);
  const writeInputs = () => {
    const ws = readFileSync(resolve(root, 'Cargo.toml'), 'utf8');
    const patch = workspacePatch(ws, checkout);
    keep(resolve(crate, 'Cargo.toml'), `[package]
name = "${packageName}"
version = "0.1.0"
edition = "2021"
publish = false
[lib]
crate-type = ["cdylib"]
[dependencies]
${data.krate} = { path = ${tomlString(dataCrate)} }
exact-logic-abi = { path = ${tomlString(resolve(checkout, 'logic/abi'))} }
[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
[workspace]
${ws.slice(ws.indexOf('[workspace.dependencies]')).split('\n[workspace.package]')[0]}
${patch}
`);
    keep(resolve(crate, 'src/lib.rs'), lib);
    if (!dev) lockFrom(crate);
  };
  return module(resolve(crate, 'Cargo.toml'), packageName.replaceAll('-', '_'), out, `${app}: the module`, writeInputs, dev);
}

/** The refusal of an app at `dir` whose web crate bakes a Rust source while `manifest` names neither `rust.data` nor
 * `rust.module.package`, or null. The JS target runs only the module the manifest names (LLP 1153.003 D2), so the
 * page would ship none and no request the source owns would be answered. The web crate bakes one when
 * `web/Cargo.toml` links a crate beside app.json, or when `web/build.rs` composes one with app.ts, which a source
 * defined in the build script itself does without linking any. */
export function undeclaredRustData(app, dir, manifest) {
  if (manifest.rust?.module?.package || manifest.rust?.data !== undefined) return null;
  let web;
  try { web = Bun.TOML.parse(readFileSync(resolve(dir, 'web/Cargo.toml'), 'utf8')); } catch { return null; }
  const tables = [web.dependencies, ...Object.values(web.target ?? {}).map(target => target?.dependencies)].filter(Boolean);
  const linked = new Set(tables.flatMap(Object.values).map(dep => dep?.path).filter(path => typeof path === 'string').map(path => resolve(dir, 'web', path)));
  const crates = appCrates(dir), crate = crates.find(crate => crate.name && linked.has(crate.dir));
  const consequence = 'the web build runs only the Rust source rust.data names, so this page would answer none of the app\'s requests';
  if (crate) return undeclaredDataCrate(`${app}: web/Cargo.toml links ${crate.name}, a crate beside app.json`, consequence, [crate], manifest);
  let script = '';
  try { script = readFileSync(resolve(dir, 'web/build.rs'), 'utf8'); } catch {}
  const composer = /\bexact_js_bake::(build_mixed(?:_with)?|build_beside)\s*\(/.exec(script)?.[1];
  return composer ? undeclaredDataCrate(`${app}: web/build.rs composes a Rust source with app.ts (exact_js_bake::${composer})`,
    `${consequence} the Rust source owns. A source that is not in a crate of its own beside app.json cannot be named: move it into one`,
    crates.filter(crate => crate.name && crate.dir !== resolve(dir, 'web')), manifest) : null;
}

/** The app's explicit logic module (`rust.module.package`, LLP 1029.000):
 * its crate already exports the ABI, built as the generated one is. */
function ownModule(app, out, dir, dev) {
  const pkg = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).rust?.module?.package;
  if (!pkg) throw new Error(`${app}: its web build bakes with no Rust data source, and app.json names no rust.module.package`);
  const manifest = readdirSync(dir).map(d => resolve(dir, d, 'Cargo.toml')).find(f => existsSync(f) && new RegExp(`^name\\s*=\\s*"${pkg}"`, 'm').test(readFileSync(f, 'utf8')));
  if (!manifest) throw new Error(`${app}: no crate in ${dir} is ${pkg}`);
  return module(manifest, pkg.replaceAll('-', '_'), out, `${app}: ${pkg}`, undefined, dev);
}

/** A logic module's `meta`, the seam's first call: the app id its data names and its grants, or null if it fails. */
function moduleMeta(instance, path) {
  const e = instance.exports, abi = e.exact_logic_abi();
  if (abi !== 12) throw new Error(`${path}: Rust module ABI ${abi} is not 12; rebuild it with this checkout`);
  const session = e.exact_logic_create(), request = new Uint8Array(5), p = e.exact_logic_alloc(request.length);
  new DataView(request.buffer).setUint32(0, abi, true);
  new Uint8Array(e.memory.buffer, p, request.length).set(request);
  if (e.exact_logic_call(session, p, request.length) !== 0) return null;
  const out = new Uint8Array(e.memory.buffer, e.exact_logic_output(session), e.exact_logic_output_len(session)).slice(), d = new DataView(out.buffer);
  // The reply's header: the ABI, then the kind of reply, 0 for `meta` (logic/abi/src/lib.rs `dispatch`).
  if (out.length < 5 || d.getUint32(0, true) !== abi || out[4] !== 0) return null;
  let at = 5;
  // The id exactly as Rust wrote it, a leading U+FEFF included, since the composition compares it byte for byte.
  const str = (ignoreBOM = false) => { const n = d.getUint32(at, true), t = new TextDecoder('utf-8', { ignoreBOM }).decode(out.subarray(at + 4, at + 4 + n)); at += 4 + n; return t; };
  return { appId: str(true), grants: str() };
}

/** A logic module's grants: what its storage requests may name, which decides whether the page ships the storage
 * adapters. A `production` build refuses a dev loop's module. */
export async function moduleGrants(path, { production = false } = {}) {
  const { module, instance } = await WebAssembly.instantiate(readFileSync(path), {});
  if (production && WebAssembly.Module.customSections(module, DEV_MARK).length) throw new Error(`this production build was given a dev loop's Rust module (built at ${DEV_PROFILE}, not optimized): give --data or --plan a build made without --dev`);
  return moduleMeta(instance, path)?.grants ?? '';
}

/** The sources a module beside app.ts answers, as its `exact_logic_abi::export_sources!` carries them, or null for a
 * module that carries none. The module is refused what a native host's composition refuses (`exact_data::Mixed::new`
 * over `Placed::named`): a source named twice or empty, and data whose app id is neither empty nor app.ts's, which is
 * `manifest.app.id` (the selected variant's) as the page runs it. */
export async function moduleSources(path, manifest) {
  const { module, instance } = await WebAssembly.instantiate(readFileSync(path), {});
  const [section] = WebAssembly.Module.customSections(module, 'exact.rust_sources');
  if (!section) return null;
  const names = JSON.parse(new TextDecoder().decode(section)), bad = names.find((name, i) => !name || names.indexOf(name) !== i);
  if (bad !== undefined) throw new Error(`RUST_SOURCES names ${bad ? `\`${bad}\` twice` : 'an empty source'}: name each source once`);
  const meta = moduleMeta(instance, path), id = manifest.app.id;
  if (!meta) throw new Error('its Rust module did not answer `meta`, the seam\'s first call');
  if (meta.appId && meta.appId !== id) throw new Error(`its Rust data's app_id is ${JSON.stringify(meta.appId)}, but app.ts's is ${JSON.stringify(id)}${manifest.variant ? ` (variant ${manifest.variant.name})` : ''}: the two halves of a mixed app declare one identity, so return the same id or an empty one`);
  return names;
}

/** Markdown pieces (LLP 1045 D3) as a loaded capability for the runtime:
 * the web host's own `exact_web_capabilities::markdown::pieces`, exported
 * alone from a wasm module the page fetches on its first Markdown node. */
export function buildMarkdown({ own = false, out = resolve(MODULES, 'markdown'), keepFresh = false } = {}) {
  const ws = readFileSync(resolve(root, 'Cargo.toml'), 'utf8');
  const patch = workspacePatch(ws);
  return buildLeaf('markdown', 'exact-web-capabilities', 'host/web-capabilities', { out, keepFresh, own, patch, lib: `//! Generated: Markdown pieces for the JS runtime.
static mut OUT: Vec<u8> = Vec::new();
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 { let mut v = Vec::<u8>::with_capacity(len); let p = v.as_mut_ptr(); std::mem::forget(v); p }
/// # Safety: \`ptr\` holds \`len\` UTF-8 bytes from \`alloc\`: the source, then
/// \`atoms\` bytes of its \`atoms\` JSON and \`faces\` of its \`atomFaces\`.
#[no_mangle]
pub unsafe extern "C" fn pieces(ptr: *mut u8, len: usize, atoms: usize, faces: usize) -> usize {
    let all = std::slice::from_raw_parts(ptr, len);
    let (source, rest) = all.split_at(len - atoms - faces);
    let (atoms, faces) = rest.split_at(atoms);
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let (source, atoms, faces) = (text(source), text(atoms), text(faces));
    drop(Vec::from_raw_parts(ptr, len, len));
    OUT = exact_web_capabilities::markdown::pieces(&source, &atoms, &faces).into_bytes();
    #[allow(static_mut_refs)]
    OUT.len()
}
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn output() -> *const u8 { unsafe { OUT.as_ptr() } }
` });
}

/** The motion capability (LLP 1071 §7): `exact-web-js-motion`'s exports,
 * the one `exact_motion` Engine, as a wasm module the page fetches after
 * first paint when its plan uses motion. */
export function buildMotion({ own = false, out = resolve(MODULES, 'motion'), keepFresh = false } = {}) {
  return buildLeaf('motion', 'exact-web-js-motion', 'host/web-js/motion', { out, keepFresh, own });
}

/** The Markdown editor's rules (LLP 1045 D5), the crate the wasm host's
 * build makes its `markup-editor.wasm` from. */
export function buildEditor({ own = false, out = resolve(MODULES, 'editor'), keepFresh = false } = {}) {
  return buildLeaf('markup-editor', 'exact-markdown-editor', 'markdown/editor', { out, keepFresh, own });
}

/** The exclusions walker (LLP 1043.000 §3 D7): `exact-textflow`'s
 * `textflow-web` binary, as the wasm build builds its `textflow.wasm`. */
export async function buildFlow({ out = resolve(MODULES, 'flow'), keepFresh = false, own = false } = {}) {
  mkdirSync(out, { recursive: true });
  const { product, depInfo } = FLOW, wasm = resolve(out, 'textflow.wasm'), target = leafTarget('flow');
  // Its group asks rustc and wasm-opt for their versions: only when it is needed.
  const at = () => kept({ name: 'textflow.wasm', crate: 'exact-textflow', product, depInfo, file: wasm, toolchain: WEB, group: ['textflow-web', 'web', WASM_FLAGS_KEY(), OPT, WASM_OPT()],
    buildDirs: [resolve(target, 'wasm32-unknown-unknown/web/build'), resolve(target, 'web/build')], stamp: resolve(MODULES, 'kept', 'textflow.json') });
  // Built again only when something it was built from changed (`fresh`). A
  // prebuild keeps one built earlier on its receipt, or builds it again.
  // `own` (a tool set's): Cargo's build, whose dep-info the set records, never a copy that is merely newer.
  if (!own && fresh(wasm, depInfo)) {
    if (!keepFresh) return wasm;
    const again = at().keepAgain();
    if (again !== null) { held.set('textflow.wasm', again); return wasm; }
    rmSync(product, { force: true });
  }
  const leaf = own ? null : taken('textflow.wasm', at);
  if (leaf?.path) { replace(wasm, (temp) => cpSync(leaf.path, temp)); held.set('textflow.wasm', leaf.kept); return wasm; }
  const started = Date.now();
  checkStd(root, 'textflow.wasm');
  const r = await run('cargo', [`+${WEB}`, 'build', '--profile', 'web', '--target', 'wasm32-unknown-unknown', '-p', 'exact-textflow', '--bin', 'textflow-web', ...LINK_FLAGS()],
    { cwd: root, env: { ...process.env, CARGO_TARGET_DIR: target } });
  if (r.status !== 0) throw new Error('textflow.wasm did not build');
  const optimizedFlow = await replace(wasm, async (temp) => !unoptimized.has(await optimizeAsync(product, temp, 'textflow.wasm')));
  held.set('textflow.wasm', optimizedFlow && at().keep(started));
  return wasm;
}
const FLOW = { product: resolve(leafTarget('flow'), 'wasm32-unknown-unknown/web/textflow-web.wasm'), depInfo: resolve(leafTarget('flow'), 'wasm32-unknown-unknown/web/textflow-web.d') };

/** A leaf from where D5's order takes it, short of Cargo (LLP 1174): a tool set built on this machine, a module kept
 * for the machine (`at()`, which only a Rust toolchain can match), or a fetched set where one may run, as
 * `{ path, kept }` (whether the machine's cache holds it); null where Cargo builds it, which throws `RustMissing`
 * when Rust is missing. */
function taken(name, at) {
  const built = fromSet(name, 'built');
  if (built) return { path: built.path, kept: true };
  const missing = rustMissing();
  const kept = missing ? null : at().find();
  if (kept) return { path: kept, kept: true };
  const fetched = fetchedTool(name);
  if (fetched) return { path: fetched.path, kept: true };
  if (missing) throw new RustMissing(name);
  return null;
}
/** Whether the leaf `flag` names (host/web-js/build.mjs's `<flag>.flag`) is built and fresh here, or a tool set
 * holds it: then the web build needs no web toolchain for it. */
export function leafTaken(flag) {
  const [name, wasm, depInfo] = { markdown: ['markdown.wasm', ...leafAt('markdown')], motion: ['motion.wasm', ...leafAt('motion')], editor: ['markup-editor.wasm', ...leafAt('markup-editor')],
    flow: ['textflow.wasm', resolve(MODULES, 'flow/textflow.wasm'), FLOW.depInfo] }[flag];
  return fresh(wasm, depInfo) || !!fromSets(name);
}
/** A `buildLeaf` module's output and the dep-info Cargo writes for it. */
function leafAt(name) {
  const dir = name === 'markup-editor' ? 'editor' : name;
  return [resolve(MODULES, dir, `${name}.wasm`), resolve(leafTarget(name), 'wasm32-unknown-unknown/release', `exact_js_${name.replaceAll('-', '_')}.d`)];
}

/** Each leaf built here by Cargo, never taken (`tools.mjs keep`), with what its tool set's manifest records: where
 * its dep-info and build scripts are, and what else decides its bytes. */
export async function leafRecipes() {
  const flags = WASM_FLAGS_KEY(), wasmOpt = WASM_OPT()?.trim() ?? null;
  const leaves = [['markdown.wasm', 'exact-web-capabilities', buildMarkdown, 'markdown'], ['motion.wasm', 'exact-web-js-motion', buildMotion, 'motion'], ['markup-editor.wasm', 'exact-markdown-editor', buildEditor, 'markup-editor']];
  const recipes = await Promise.all(leaves.map(async ([name, crate, build, dir]) => {
    const wasm = await build({ own: true });
    if (unoptimized.has(wasm)) throw new Error(`${name} is not optimized (wasm-opt), and a tool set's leaves are`);
    const release = resolve(leafTarget(dir), 'wasm32-unknown-unknown/release');
    return { name, crate, wasm, profile: 'release', toolchain: WEB, flags, wasmOpt, depInfo: leafAt(dir)[1], buildDirs: [resolve(release, 'build'), resolve(leafTarget(dir), 'release/build')] };
  }));
  const wasm = await buildFlow({ own: true }), target = leafTarget('flow');
  if (unoptimized.has(wasm)) throw new Error('textflow.wasm is not optimized (wasm-opt), and a tool set\'s leaves are');
  return [...recipes, { name: 'textflow.wasm', crate: 'exact-textflow', wasm, profile: 'web', toolchain: WEB, flags, wasmOpt, depInfo: FLOW.depInfo,
    buildDirs: [resolve(target, 'wasm32-unknown-unknown/web/build'), resolve(target, 'web/build')] }];
}

// A workspace crate's exports as a size-built wasm module: `<name>.wasm`,
// in its own target directory, so the leaves an app uses build at once.
async function buildLeaf(name, pkg, dir, { out, keepFresh, own = false, patch = '', lib }) {
  const crate = resolve(out, 'crate');
  mkdirSync(resolve(crate, 'src'), { recursive: true });
  const ws = readFileSync(resolve(root, 'Cargo.toml'), 'utf8');
  const toml = `[package]
name = "exact-js-${name}"
version = "0.1.0"
edition = "2021"
publish = false
[lib]
crate-type = ["cdylib"]
[dependencies]
${pkg} = { path = ${tomlString(resolve(root, dir))} }
[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
[workspace]
${ws.slice(ws.indexOf('[workspace.dependencies]')).split('\n[workspace.package]')[0]}
${patch}`;
  const source = lib ?? `//! Generated: ${name}.wasm for the JS runtime.\npub use ${pkg.replaceAll('-', '_')}::*;\n`;
  keep(resolve(crate, 'Cargo.toml'), toml);
  keep(resolve(crate, 'src/lib.rs'), source);
  lockFrom(crate);
  const target = leafTarget(name), release = resolve(target, 'wasm32-unknown-unknown/release');
  const product = resolve(release, `exact_js_${name.replaceAll('-', '_')}.wasm`), depInfo = product.replace(/\.wasm$/, '.d'), wasm = resolve(out, `${name}.wasm`);
  // The generated crate names this checkout's paths; what it says otherwise is the group's.
  const at = () => kept({ name: `${name}.wasm`, crate: pkg, product, depInfo, file: wasm, toolchain: WEB, group: ['leaf', name, toml.replaceAll(root, '.'), source, WASM_FLAGS_KEY(), OPT, WASM_OPT()],
    buildDirs: [resolve(release, 'build'), resolve(target, 'release/build')], stamp: resolve(MODULES, 'kept', `${name}.json`) });
  // Built again only when something it was built from changed (`fresh`). A
  // prebuild keeps one built earlier on its receipt, or builds it again.
  if (!own && fresh(wasm, depInfo, [resolve(crate, 'Cargo.toml')])) {
    if (!keepFresh) return wasm;
    const again = at().keepAgain();
    if (again !== null) { held.set(`${name}.wasm`, again); return wasm; }
    rmSync(product, { force: true });
  }
  const leaf = own ? null : taken(`${name}.wasm`, at);
  if (leaf?.path) { replace(wasm, (temp) => cpSync(leaf.path, temp)); held.set(`${name}.wasm`, leaf.kept); return wasm; }
  const started = Date.now();
  checkStd(process.cwd(), `${name}.wasm`);
  const r = await run('cargo', cargoWasmArgs(resolve(crate, 'Cargo.toml')), { env: { ...process.env, CARGO_TARGET_DIR: target } });
  if (r.status !== 0) throw new Error(`${name}.wasm did not build`);
  const optimizedLeaf = await replace(wasm, async (temp) => !unoptimized.has(await optimizeAsync(product, temp, `${name}.wasm`)));
  held.set(`${name}.wasm`, optimizedLeaf && at().keep(started));
  return wasm;
}

/** `text` without its `// dev-only {` … `// dev-only }` regions: what a
 * build that is not the dev loop's copies of a runtime file (modes.js's
 * live table, LLP 1121 stage 4), so no release page carries it. */
/** The runtime files a development build copies with inspect mode's hooks
 * (LLP 1122 T2b; inspect-dev.js), so the files themselves, and every other
 * build's copy, have none: an element's view id (`$exactViewId`, tree.js),
 * and every `each` region with its rows and key function (`$exactEaches`,
 * mount.js), which a development build's emitter names (regions.rs passes
 * the region id) and which leaves with its scope. */
const INSPECT_HOOKS = {
  'tree.js': [/^export function viewId\(e\) \{\n[\s\S]*?^\}\n/m, (fn) => `${fn}globalThis.$exactViewId = viewId;\n`],
  'mount.js': [/^  let rows = new Map\(\),\n    single = false,\n    order = null;\n/m, (line) => `${line}  if (arguments[5] !== undefined) { const e = { region: arguments[5], rows: () => order ?? [], key, live: () => a.isConnected }; (globalThis.$exactEaches ??= new Set()).add(e); onEnd(() => globalThis.$exactEaches.delete(e)); }\n`],
};
export const inspectHooked = Object.keys(INSPECT_HOOKS);
/** `text`, runtime file `file`'s source, with its inspect hook. Throws when
 * the file no longer has the lines the hook hangs on. */
export function withInspectHooks(file, text) {
  const [at, hook] = INSPECT_HOOKS[file];
  if (!at.test(text)) throw new Error(`${file}: inspect mode's hook has no line to hang on (${at})`);
  return text.replace(at, hook);
}
/** B5 (lower.js `RUNTIME`): the arms sheet's host stamp, a digest of the runtime files the build copies first (`files`:
 * `{ path, text }`, lower.js's unstamped source among them), so a runtime whose helpers or rule builder changed under a
 * kept document starts its sheet over, where the arm names (the recipe, its literals, its constants) did not change. */
export const withRuntimeStamp = (text, files) => text.replace('"exact-runtime-unstamped"', JSON.stringify(createHash('sha256').update(files.map(f => `${f.path}\0${f.text}`).join('\0')).digest('hex').slice(0, 24)));
export const withoutDev = (text) => text.replace(/^[ \t]*\/\/ dev-only \{.*\n[\s\S]*?^[ \t]*\/\/ dev-only \}.*\n/gm, '');

const COMPILER = resolve(TARGET_DIR, `debug/exact-web-js${EXE}`);
const compilerKept = () => kept({ crate: 'exact-web-js', product: COMPILER, depInfo: depInfoOf(COMPILER), group: ['debug'], buildDirs: [resolve(TARGET_DIR, 'debug/build')], stamp: resolve(MODULES, 'kept', 'exact-web-js.json') });
export function webCompilerFrom(env = process.env, { rehash = false } = {}) {
  // The dev loop can lend a fresh copy of its optimized design reader to this build.
  const fast = env.EXACT_WEB_COMPILER;
  if (fast && freshCompiler(fast)) return { path: fast, set: null };
  return compilerFrom('exact-web-js', COMPILER, env, { rehash, kept: () => rustMissing(env) ? null : compilerKept().find() });
}

/** Launch the compiler selected by LLP 1174 D5, or build it with Cargo when none answers. */
export function webCompiler(options) {
  const selected = webCompilerFrom(process.env, options);
  if (selected) return launch(selected.path);
  if (rustMissing()) throw new RustMissing('exact-web-js');
  const started = Date.now();
  return launch('cargo', ['run', '-q', '-p', 'exact-web-js', '--'], () => compilerKept().keep(started));
}
/** The selected compiler's tool set supplies the web build's Cargo metadata (LLP 1174 D5). */
export function compilerSet() {
  return webCompilerFrom()?.set ?? null;
}

/** Everything an app's web build shares, compiled where it is not yet and
 * kept for the machine: after it, no app's first web build on this machine
 * compiles them at this checkout's sources. A product already built is kept
 * on the receipt its compile left (`keepAgain`); one without is built again. */
async function prebuild() {
  if (spawnSync('git', ['--no-optional-locks', '-C', root, 'status', '--porcelain'], { encoding: 'utf8' }).stdout) console.warn('web-js: this checkout has uncommitted changes; what they touch is built but not kept');
  const compiler = compilerKept();
  const compiling = (async () => {
    const again = freshCompiler(COMPILER) ? compiler.keepAgain() : null;
    if (again !== null) return held.set('exact-web-js', again);
    rmSync(COMPILER, { force: true });
    // Already kept from these sources (this checkout took it): nothing to do.
    if (compiler.find()) return held.set('exact-web-js', true);
    const started = Date.now();
    if ((await run('cargo', ['build', '-q', '-p', 'exact-web-js'], { cwd: root })).status !== 0) throw new Error('exact-web-js did not build');
    held.set('exact-web-js', compiler.keep(started));
  })();
  await Promise.all([buildMarkdown, buildMotion, buildEditor, buildFlow].map(build => build({ keepFresh: true })));
  await compiling;
  const names = (kept) => [...held].filter(([, k]) => k === kept).map(([name]) => name).join(', ');
  console.log(`web-js: kept for this machine (~/.cache/exact/web-modules): ${names(true) || 'nothing'}`);
  if (names(false)) console.warn(`web-js: built, not kept (uncommitted or changed while compiling): ${names(false)}`);
}

// A game's resident producer and its proof share completed development web
// products locally. The snapshot supplies its own inputs; the live world is never read.
export function warmGameEnvironment(env) {
  // The bake's own environment, which every cargo call's host deployment target is in (scripts/filesystem.mjs).
  return {...withHostDeployment(env), EXACT_UPDATE_TRUST:env.EXACT_UPDATE_TRUST ?? 'development',
    EXACT_GAME_PARANOID:env.EXACT_GAME_PARANOID ?? '0', EXACT_WEB_LINK:env.EXACT_WEB_LINK ?? 'all',
    RUSTUP_TOOLCHAIN:WEB_TOOLCHAIN, EXACT_WEB_SIZE:WEB_SIZE};
}
// Cargo-provided paths and package metadata follow the captured sources. Other
// compiler env-deps must match the caller or a recorded build-script output;
// the script's own watched environment must match the caller in either case.
function warmConfigurationMatches(app, configuration, environment) {
  if (!configuration || configuration.target !== 'wasm32-unknown-unknown'
      || !Array.isArray(configuration.units) || !Array.isArray(configuration.builders)
      || !configuration.units.length || !configuration.rustc || !configuration.flags) return false;
  const env = warmGameEnvironment(environment);
  const hash = value => value == null ? null : createHash('sha256').update(value).digest('hex');
  for (const [key,value] of Object.entries(configuration.flags)) if ((env[key] ?? null) !== value) return false;
  for (const builder of configuration.builders) {
    if (!Array.isArray(builder.environment) || !Array.isArray(builder.env)) return false;
    for (const [key,value] of builder.environment) if (hash(env[key]) !== value) return false;
  }
  for (const unit of configuration.units) {
    if (!Array.isArray(unit.environment)) return false;
    for (const [key,value] of unit.environment) {
      if (key === 'OUT_DIR' && value === `generated:${unit.package}:${unit.role}/`) continue;
      if (key === 'CARGO_MANIFEST_DIR' && /^crate:[^/]+\/$/.test(value ?? '')) continue;
      if (/^CARGO_PKG_(VERSION(?:_MAJOR|_MINOR|_PATCH|_PRE)?|NAME|AUTHORS|DESCRIPTION|HOMEPAGE|REPOSITORY|LICENSE|LICENSE_FILE|RUST_VERSION|README)$/.test(key)) continue;
      const generated = configuration.builders.filter(b => b.package === unit.package && b.role === unit.role)
        .flatMap(b => b.env).filter(([name]) => name === key);
      if (generated.length) { if (generated.some(([,v]) => v !== value)) return false; }
      else if (hash(env[key]) !== value) return false;
    }
  }
  const compiler = spawnSync('rustc', ['-vV'], {cwd:app.workspace ?? root, env, encoding:'utf8'});
  return compiler.status === 0 && compiler.stdout === configuration.rustc;
}

// Cache the owned stage before the directory swap can expose another build's dist.
export function publishWarmGame(app, stage, dist, before, build, env = process.env, publish = replaceBuild) {
  keepWarmGame(app, stage, before, env, build);
  publish(stage, dist);
}
export function warmGameInputs(app, env = process.env) {
  if (!app.manifest.game || env.EXACT_GAME_PROOF_PROFILE !== 'gpu-dev'
      || (env.EXACT_UPDATE_TRUST ?? 'development') !== 'development') return null;
  env = warmGameEnvironment(env);
  const inputs = proofInputs(root, app.dir, resolve(app.target, 'proof-web-inputs.json'));
  const flags = Object.entries(env).filter(([key]) => /^(RUST|CARGO_PROFILE_|CARGO_ENCODED_|CARGO_TARGET_.*_(RUSTFLAGS|LINKER)$|EXACT_GAME_PARANOID_COMPARE$)/.test(key)).sort();
  const configuration = JSON.stringify([app.id, app.manifest, env.EXACT_GAME_PARANOID ?? '0', env.EXACT_UPDATE_ORIGIN ?? null, env.EXACT_BACKEND_ORIGIN ?? null, env.EXACT_WEB_LINK, env.EXACT_WEB_NAMES ?? null, flags]);
  return {...inputs, configuration, key:createHash('sha256').update(inputs.all + configuration).digest('hex')};
}
function warmFiles(directory) {
  const files = [];
  const walk = (dir, prefix = '') => {
    for (const entry of readdirSync(dir, {withFileTypes:true}).sort((a,b) => a.name.localeCompare(b.name))) {
      const name = prefix + entry.name, path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path, name + '/');
      else if (entry.isFile()) files.push([name, createHash('sha256').update(readFileSync(path)).digest('hex')]);
      else throw new Error('warm web product contains a non-file');
    }
  };
  walk(directory);
  return files;
}
export function keepWarmGame(app, dist, before, env = process.env, build) {
  if (!before || !warmConfigurationMatches(app, build?.binary?.configuration, env)) return false;
  let stage;
  try {
    const after = warmGameInputs(app, env);
    // A bake may create assets, but may not take credit for moving authored sources.
    if (!after || before.authored !== after.authored || before.authoredState !== after.authoredState
        || before.configuration !== after.configuration) return false;
    const cache = resolve(app.target, 'proof-web'); mkdirSync(cache, {recursive:true});
    stage = mkdtempSync(resolve(cache, 'build-'));
    const copied = resolve(stage, 'dist');
    cpSync(dist, copied, {recursive:true});
    const current = warmGameInputs(app, env);
    if (current?.key !== after.key || current.authoredState !== after.authoredState || current.assetState !== after.assetState) return false;
    const receipt = {key:after.key, directory:stage, files:warmFiles(copied), configuration:build.binary.configuration};
    const path = resolve(cache, `${after.key}.json`);
    let previous;
    try { previous = JSON.parse(readFileSync(path, 'utf8')); } catch {}
    replace(path, temp => writeFileSync(temp, JSON.stringify(receipt)));
    stage = null;
    if (typeof previous?.directory === 'string' && previous.directory !== receipt.directory && dirname(previous.directory) === cache)
      rmSync(previous.directory, {recursive:true,force:true});
    // Evict by receipt publication mtime; this never enters simulation state.
    const entries = readdirSync(cache).filter(name => /^[a-f0-9]{64}\.json$/.test(name)).sort((a,b) => statSync(resolve(cache,b)).mtimeMs - statSync(resolve(cache,a)).mtimeMs);
    for (const name of entries.slice(6)) {
      const old = JSON.parse(readFileSync(resolve(cache,name),'utf8'));
      if (dirname(old.directory) === cache) rmSync(old.directory,{recursive:true,force:true});
      rmSync(resolve(cache,name),{force:true});
    }
    return copied;
  } catch (error) { console.warn(`web: warm game product not kept (${error.message})`); return false; }
  finally { if (stage) rmSync(stage, {recursive:true,force:true}); }
}
export function takeWarmGame(app, dist, env = process.env) {
  const inputs = warmGameInputs(app, env);
  if (!inputs) return false;
  let stage;
  try {
    const cache = resolve(app.target, 'proof-web');
    const receipt = JSON.parse(readFileSync(resolve(cache, `${inputs.key}.json`), 'utf8'));
    if (receipt.key !== inputs.key || dirname(receipt.directory) !== cache
        || !warmConfigurationMatches(app, receipt.configuration, env)) return false;
    stage = mkdtempSync(resolve(app.target, 'proof-web-copy-'));
    cpSync(resolve(receipt.directory, 'dist'), stage, {recursive:true});
    if (JSON.stringify(warmFiles(stage)) !== JSON.stringify(receipt.files)) return false;
    // The game proof's source inventory excludes installed packages. Verify what the bundler read and what it bound.
    const markerPath = resolve(stage, '.exact-build.json');
    const marker = existsSync(markerPath) ? JSON.parse(readFileSync(markerPath, 'utf8')) : {};
    if (webPackageBindingChanged(app.dir, marker.packageViews)) return false;
    // Driver staleness paths are local to the captured source tree. Relocate only
    // after the complete portable input identity and copied products agree.
    // The caller refreshes the driver's path-based marker after this copy.
    rmSync(dist, {recursive:true,force:true});
    renameSync(stage, dist); stage = null;
    console.log('BUILD warm web producer: matching input identity');
    return true;
  } catch { return false; }
  finally { if (stage) rmSync(stage, {recursive:true,force:true}); }
}

if (import.meta.main) {
  if (process.argv.includes('--prebuild')) await prebuild();
  else {
    const [app] = process.argv.slice(2);
    const o = process.argv.indexOf('--out');
    console.log(buildModule(app, o < 0 ? undefined : process.argv[o + 1]));
  }
}
