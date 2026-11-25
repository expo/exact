/**
 * Apple builds bake replaceable artifacts before Cargo and link them into
 * __TEXT. Cargo sees only generated Rust inputs, so a plan edit leaves the
 * archive untouched. TypeScript apps use exact-js-bake; Rust apps register
 * their own compiled build-script executable through exact_bake::take_linked.
 *
 * Android development APKs reuse the registered baker and link the data
 * into a separate ELF image. A bake runs again when a watched path,
 * variable or tool changes.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bakeTarget, cargoEnvironment, developmentBuildEnv } from './app.mjs';
import { oneScheme, webOnly } from './app-cli.mjs';
import { copiedAndroidEntries, copiedBuildScripts } from './app-shells.mjs';
import { appleCargo, cargoBuildEnv } from './cargo-check.mjs';
import { cargoInputPaths, cargoLibraryTarget, watchedDigest } from './cargo-dep-info.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const TOOL_TARGET = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : resolve(ROOT, 'target');

/** The bake command (`exact-js-bake`) a plan override runs (scripts/agent-build.mjs), built once in this
 * checkout's target and shared by every app built from it: the command that builds it (a no-op when built), and
 * where it is. An Apple build bakes with `appleBakeTool`. */
export const bakeTool = { command: 'cargo', args: ['build', '-q', '--profile', 'host-dev', '-p', 'exact-js-bake'], built: resolve(TOOL_TARGET, 'host-dev/exact-js-bake'),
  // Built with a build's own Mac deployment target when one is named, as its host side shares the target directory.
  options: (build = {}) => ({ cwd: ROOT, env: { ...developmentBuildEnv(), ...(build.MACOSX_DEPLOYMENT_TARGET ? { MACOSX_DEPLOYMENT_TARGET: build.MACOSX_DEPLOYMENT_TARGET } : {}), EXACT_UPDATE_TRUST: 'development', CARGO_TARGET_DIR: TOOL_TARGET } }) };

/** The Cargo arguments that build `appleBakeTool` on `host` with the app's profile arguments. */
export const appleBakeToolArgs = (host, cargoArgs = ['--profile', 'host-dev']) => ['build', ...cargoArgs, '-p', 'exact-js-bake-tool', '--target', host];

/** The bake tool an Apple build runs before Cargo (`bakeOutside`): `exact-js-bake-tool`'s build script, which runs
 * the `exact-js-bake` command. A build script's dependencies compile as the app crate's build dependency on
 * exact-js-bake does (the profile's build override, the build side's features). Built with the app's profile
 * arguments, its environment and an explicit target (Cargo then leaves RUSTFLAGS off the host's units, as in the
 * app's build), every unit of the tool is one the app's build then finds built. Built as a binary, most were others,
 * and a Rust change under the bake compiled twice for the host. The environment must be the app build's: a shared
 * build script runs again when a variable it reads differs (cc's `CXXFLAGS`, the deployment targets). The
 * executable. */
export function appleBakeTool(env, cargoArgs) {
  const host = bakeTarget('macos');
  const args = [...appleBakeToolArgs(host, cargoArgs), '-q', '--message-format=json-render-diagnostics'];
  const built = spawnSync(...appleCargo('cargo', args, { cwd: ROOT, env: { ...cargoEnvironment(env), CARGO_TARGET_DIR: TOOL_TARGET }, stdio: ['ignore', 'pipe', 'inherit'], encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }));
  if (built.status !== 0) throw new Error(`the bake tool did not build (cargo ${args.join(' ')})`);
  const manifest = resolve(ROOT, 'js/bake/tool/Cargo.toml');
  const script = (built.stdout ?? '').split('\n').filter(line => line.startsWith('{')).map(line => JSON.parse(line))
    .find(m => m.reason === 'compiler-artifact' && m.manifest_path === manifest && m.target.kind.includes('custom-build'));
  if (!script?.filenames?.[0]) throw new Error(`cargo ${args.join(' ')} named no build script for exact-js-bake-tool`);
  return script.filenames[0];
}

/** The Apple build script `exact new` writes: it names the platform and
 * bakes, and does nothing else a bake made before Cargo would miss. */
export const APPLE_BUILD_SCRIPT = `fn main() {
    let platform = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        // tvOS and watchOS reuse the manifest's iOS policy, including interpreted Rust updates.
        Ok("ios" | "tvos" | "watchos") => "ios",
        _ => "macos",
    };
    exact_js_bake::build(std::path::Path::new(".."), platform).unwrap_or_else(|error| {
        // What the bake found (tsc's and the compiler's diagnostics), as it is: no panic around it.
        eprintln!("{error}");
        std::process::exit(1);
    });
}
`;

/** APPLE_BUILD_SCRIPT with the app at `app` from the crate: `..` for an authored crate beside app.json, `../../..` for
 * one generated under `.exact/shells/` (scripts/app-shells.mjs, LLP 1153 D1). */
export const appleBuildScript = (app = '..') => APPLE_BUILD_SCRIPT.replace('std::path::Path::new("..")', `std::path::Path::new(${JSON.stringify(app)})`);

/** The one `exact new` wrote before, which mapped iOS alone; `update`
 * replaces it with APPLE_BUILD_SCRIPT. */
export const EARLIER_APPLE_BUILD_SCRIPT = `fn main() {
    let platform = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("ios") => "ios",
        _ => "macos",
    };
    exact_js_bake::build(std::path::Path::new(".."), platform).unwrap_or_else(|error| {
        // What the bake found (tsc's and the compiler's diagnostics), as it is: no panic around it.
        eprintln!("{error}");
        std::process::exit(1);
    });
}
`;

/** Whether this Apple entry can read a linked plan. Standard TypeScript
 * entries bake with exact-js-bake; Rust build scripts opt in with
 * exact_bake::take_linked and register their own compiled executable.
 * Other scripts and entries declaring their own PLAN/COMPAT still embed.
 * Embedding archives keep their plans, since their caller supplies no
 * linked sections. Widget builds link the plan into their shared framework. */
export function linkable(app, pkg) {
  const read = (target) => { try { return readFileSync(target.src_path, 'utf8'); } catch { return null; } };
  const script = pkg.targets.find(t => t.kind.includes('custom-build')), entry = pkg.targets.find(t => t.kind.some(k => ['lib', 'rlib', 'staticlib'].includes(k)));
  if (!script || !entry) return false;
  if (read(script)?.includes('exact_bake::take_linked(')) return !/^(?:pub )?const (?:PLAN|COMPAT):/m.test(read(entry) ?? '');
  if (!existsSync(resolve(app.dir, 'app.ts')) || existsSync(resolve(app.dir, 'data/Cargo.toml'))) return false;
  if (![APPLE_BUILD_SCRIPT, EARLIER_APPLE_BUILD_SCRIPT, appleBuildScript('../../..')].includes(read(script))) return false;
  const source = read(entry) ?? '';
  if (!/^const PLAN:/m.test(source) || !/^const COMPAT:/m.test(source)) return true;
  console.error(`host/apple: ${entry.src_path} declares PLAN and COMPAT, so the plan is compiled into the archive and a Contract edit compiles it again; \`bun exact.mjs update\` drops them, and the link then carries the plan`);
  return false;
}

/** A stock authored script needs the matching library entry. Custom scripts
 * opt in at entry so that a handoff can return before their remaining work. */
export function androidLinkable(pkg, generated = false) {
  const script = pkg.targets.find(t => t.kind.includes('custom-build'));
  if (!script || generated) return !!script;
  const read = path => readFileSync(path, 'utf8').replaceAll('\r\n', '\n');
  const source = read(script.src_path);
  if (/^\s*if (?:exact_bake::host|exact_js_bake)::take_android_linked\(/m.test(source)) return true;
  const entry = cargoLibraryTarget(pkg);
  const body = text => text.replace(/^(?:\/\/![^\n]*\n|\n)*/, '');
  const short = `fn main() {
    exact_js_bake::build_android(std::path::Path::new(".."))
        .unwrap_or_else(|e| panic!("Android bake: {e}"));
}`;
  if (!entry || !(copiedBuildScripts().android.includes(source) || body(source).trim() === short)) return false;
  const library = body(read(entry.src_path));
  return copiedAndroidEntries().some(stock => body(stock) === library);
}

/** The files of a linked bake the archive compiles (js/bake/src/linked.rs `COMPILED`). */
export const COMPILED_BAKE = ['module.rs', 'logic.rs', 'linked.rs', 'host.rs'];

/** A content-addressed copy of the files Cargo compiles from the linked bake in `dir`, under `root`; what Cargo's
 * EXACT_BAKED names. The app's identity, grants and Canvas roster are in compat.json, which the link carries
 * (`exact_apple::baked::identity`), so bakes whose generated Rust is the same get the same path and bytes: the
 * archive's build script and crate stay fresh when only those change. Different apps share the compiled archive only
 * when Cargo sees one package in one target directory (a service building many apps under one name). The copy never changes once
 * written; a concurrent build of another app writes its own. Null when the bake lacks one of the files. */
export function compiledBakeDir(dir, root) {
  const files = [];
  for (const name of COMPILED_BAKE) {
    try { files.push([name, readFileSync(resolve(dir, name))]); } catch { return null; }
  }
  const hash = createHash('sha256');
  for (const [name, bytes] of files) hash.update(`${name}\0${bytes.length}\0`).update(bytes);
  const out = resolve(root, hash.digest('hex').slice(0, 32));
  if (existsSync(resolve(out, '.complete'))) return out;
  const making = `${out}.${process.pid}.tmp`;
  rmSync(making, { recursive: true, force: true });
  mkdirSync(making, { recursive: true });
  for (const [name, bytes] of files) writeFileSync(resolve(making, name), bytes);
  writeFileSync(resolve(making, '.complete'), '');
  try { renameSync(making, out); } catch (error) {
    // Another build wrote the same bytes first.
    rmSync(making, { recursive: true, force: true });
    if (!existsSync(resolve(out, '.complete'))) throw error;
  }
  return out;
}

/** Where an Apple build for `target` keeps the bake it links. */
export const linkedBakeDir = (env, platform, target) => resolve(env.EXACT_BAKE_OUTPUT, `${platform}-${target}.baked`);

/** What the bake in `dir` read (its watched paths, those absent and its package directories' entries), or null
 * without a readable record. The build that links the bake keeps a copy: the next bake rewrites `inputs.json`
 * before its own build has replaced this one. */
export function linkedInputs(dir) {
  try {
    const {paths, digest, missing = [], watchedDirectories = []} = JSON.parse(readFileSync(resolve(dir, 'inputs.json'), 'utf8'));
    return [paths, missing, watchedDirectories].every(Array.isArray) ? {paths, missing, watchedDirectories, ...(digest === null ? {unstable:resolve(dir, 'inputs.json')} : {})} : null;
  } catch { return null; }
}

/** The linker arguments that put a linked bake's plan, compat.json and module bytecode in the executable; the
 * bytecode 16-byte aligned, which Hermes needs to read it in place (js/src/shim.cc). */
export const linkedPlanArgs = (dir) => !dir ? [] : [...[['__exact_plan', 'app.plan'], ['__exact_compat', 'compat.json'], ['__exact_hbc', 'app.hbc']]
  .flatMap(([section, file]) => ['-Xlinker', '-sectcreate', '-Xlinker', '__TEXT', '-Xlinker', section, '-Xlinker', resolve(dir, file)]),
  '-Xlinker', '-sectalign', '-Xlinker', '__TEXT', '-Xlinker', '__exact_hbc', '-Xlinker', '0x10'];

/** `outputs` are the build's (app.mjs `buildOutputs`), Cargo's configured build directory among them. */
const watchOutputs = (dir, env, outputs = []) => [dir, env.CARGO_TARGET_DIR, env.CARGO_BUILD_BUILD_DIR, env.EXACT_BAKE_OUTPUT, ...outputs].filter(Boolean).map(path => resolve(path));

/** Explicit producer files (a signed GPU library) remain inputs under output roots.
 * The bake's own directory is always excluded, including through aliases. */
function inputDigests(paths, dir, env, {since = Infinity, known = new Set(), outputs: built = [], snapshots} = {}) {
  const outputs = watchOutputs(dir, env, built);
  return new Map(paths.map(path => {
    let file = false; try { file = statSync(path).isFile(); } catch { /* missing watch */ }
    return [path, watchedDigest([path], file ? [dir] : outputs, {since:known.has(path) ? Infinity : since, snapshots})];
  }));
}
const digestOf = inputs => [...inputs.values()].includes(null) ? null : createHash('sha256').update(JSON.stringify([...inputs])).digest('hex');
const inputDigest = (paths, dir, env, options) => digestOf(inputDigests(paths, dir, env, options));

/** The payload files required by the Android bake's compatibility and module receipts. */
export function androidBakeBlobs(dir) {
  const compat = JSON.parse(readFileSync(resolve(dir, 'compat.json'), 'utf8'));
  const blobs = ['app.plan', 'compat.json'];
  if (compat.inputs.hermesBytecode != null) {
    const {module} = JSON.parse(readFileSync(resolve(dir, 'app.module.json'), 'utf8'));
    const hbc = readFileSync(resolve(dir, 'app.hbc'));
    if (module?.file !== 'app.hbc' || module.bytes !== hbc.length ||
        module.sha256 !== createHash('sha256').update(hbc).digest('hex')) {
      throw new Error(`${dir}: app.hbc differs from its module receipt`);
    }
    blobs.push('app.hbc', 'app.module.json');
  }
  return blobs;
}

/** The digest of the bake's linked files, or null for incomplete output. */
function pairOf(dir, android) {
  try {
    const files = android ? androidBakeBlobs(dir) : ['app.plan', 'compat.json'];
    const hash = createHash('sha256');
    for (const file of files) hash.update(readFileSync(resolve(dir, file)));
    return hash.digest('hex');
  } catch { return null; }
}

/** The file system's time now, as change times read it: a stamp's, written in `dir`. File times come from a coarser
 * clock than Date.now() and can trail it by a tick, so an input the bake changes would otherwise seem to precede it. */
function fileClock(dir) {
  const stamp = resolve(dir, `.started.${process.pid}`);
  mkdirSync(dir, { recursive: true });
  writeFileSync(stamp, '');
  try { return statSync(stamp).ctimeMs; } finally { rmSync(stamp, { force: true }); }
}

/** Whether the linked output and its inputs still match the bake's record. */
function fresh(record, dir, env, tool, outputs, android, snapshots) {
  if (record?.version !== 2 || record.tool !== tool || !record.pair || record.pair !== pairOf(dir, android)) return false;
  if (Object.entries(record.env).some(([name, value]) => (env[name] ?? null) !== value)) return false;
  return record.digest != null && record.digest === inputDigest(record.paths, dir, env, {outputs, snapshots});
}

function writeInputs(at, record) {
  writeFileSync(`${at}.${process.pid}.tmp`, JSON.stringify(record));
  renameSync(`${at}.${process.pid}.tmp`, at);
}

/** Bakes `app` for `platform` and `target` into its linked bake directory,
 * unless the bake there is fresh. A bake that fails throws
 * what it said, as the build script's failure ends a build. `prepareRust`
 * refreshes the compiled baker through Cargo before it reads new inputs;
 * `cargoArgs`, the app's profile arguments, build a TypeScript app's tool
 * (`appleBakeTool`).
 * The caller holds the app's build claim as the directory's sole writer. */
export function bakeOutside(app, platform, target, env, prepareRust, { cargoArgs, outputs = [], toolBuilt = false, workspace, dir = linkedBakeDir(env, platform, target) } = {}) {
  const at = resolve(dir, 'inputs.json');
  let executable, args = [app.dir, '--cargo', platform, target, dir], cwd = app.dir, runEnv = cargoEnvironment(env);
  const android = platform === 'android';
  const rust = android || existsSync(resolve(app.dir, 'data/Cargo.toml'));
  if (rust) {
    prepareRust?.(dir);
    // Cargo registers the app's own compiled build script on its first run.
    // It also refreshes that registration and bake when Rust inputs change.
    const registry = resolve(dir, 'rust-baker.json');
    if (!existsSync(registry)) return dir;
    const baker = JSON.parse(readFileSync(registry, 'utf8'));
    if (!existsSync(baker.executable)) return dir;
    executable = baker.executable; args = []; cwd = baker.cwd;
    if (android && workspace) runEnv = cargoBuildEnv(workspace, runEnv);
    runEnv = { ...runEnv, ...baker.env, OUT_DIR: dir, EXACT_BAKED: dir, EXACT_RUST_BAKE: '1' };
  } else {
    // `toolBuilt`: the caller knows this checkout's tool is built (a read-only compiled SDK), so Cargo's check is skipped.
    executable = toolBuilt && existsSync(bakeTool.built) ? bakeTool.built : appleBakeTool(env, cargoArgs);
  }
  const tool = `${executable} ${statSync(executable).mtimeMs}`;
  // Its warnings are said on every build, as Cargo replays a build script's: the SF Symbols only the web draws blank
  // one line where the first was, as `contract build` says them (18 of them every euxy build).
  const warn = (warnings) => { for (const line of bakeWarnings(app.name, warnings ?? [])) console.error(line); };
  let record = null; try { record = JSON.parse(readFileSync(at, 'utf8')); } catch { /* none yet */ }
  const snapshots = new Map();
  if (fresh(record, dir, runEnv, tool, outputs, android, snapshots)) {
    const watchedDirectories = [...snapshots.values()];
    if (JSON.stringify(watchedDirectories) !== JSON.stringify(record.watchedDirectories)) writeInputs(at, {...record, watchedDirectories});
    warn(record.warnings);
    return dir;
  }
  snapshots.clear();
  const cargoOutput = resolve(dir, 'rust-output.txt');
  const previousOutput = rust && !record?.paths ? readFileSync(cargoOutput, 'utf8') : '';
  const watched = record?.paths ?? cargoInputPaths(previousOutput, cwd);
  const before = inputDigests(watched, dir, runEnv, {outputs});
  rmSync(at, { force: true });
  const started = Date.now(), since = fileClock(dir);
  const r = spawnSync(executable, args, { cwd, env: runEnv, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  const lines = (r.stdout ?? '').split('\n'), said = (key) => lines.map(line => new RegExp(`^cargo::?${key}=(.*)$`).exec(line)?.[1]).filter(value => value !== undefined);
  warn(said('warning'));
  if (r.status !== 0) {
    const error = new Error(`${app.name}: the bake failed:\n${(r.stderr || r.error?.message || `exit ${r.status}`).trim()}`);
    error.stack = error.message; error.contract = true;
    throw error;
  }
  if (rust) writeFileSync(cargoOutput, r.stdout ?? '');
  const paths = cargoInputPaths(r.stdout ?? '', cwd);
  // New watches have no pre-bake digest. Their change times must precede the read;
  // known watches compare bytes so touching an unchanged input does not invalidate it.
  const after = inputDigests(paths, dir, runEnv, {since, known:new Set(watched), outputs, snapshots});
  const stable = [...after].every(([path, digest]) => !before.has(path) || before.get(path) === digest);
  record = { version: 2, at: started, tool, pair: pairOf(dir, android), warnings: said('warning'), env: Object.fromEntries(said('rerun-if-env-changed').map(name => [name, runEnv[name] ?? null])), paths, digest:stable ? digestOf(after) : null, missing: paths.filter(path => !existsSync(path)), watchedDirectories:[...snapshots.values()] };
  mkdirSync(dir, { recursive: true });
  writeInputs(at, record);
  return dir;
}

const androidRegistry = dir => resolve(dir, 'rust-baker.json');
const androidUnitDir = (root, out) => resolve(root, createHash('sha256').update(out).digest('hex'));

function androidLinkDigest(output, cwd) {
  const directives = output.split(/\r?\n/).flatMap(line => {
    const match = /^cargo::?([^:=]+)=(.*)$/.exec(line);
    if (!match || ['rerun-if-changed', 'rerun-if-env-changed', 'warning'].includes(match[1])) return [];
    const [, name, value] = match;
    return [[name === 'rustc-cdylib-link-arg' ? 'rustc-link-arg-cdylib' : name, value]];
  });
  const args = directives.filter(([name]) => name.startsWith('rustc-link-arg'))
    .flatMap(([name, value]) => {
      const arg = name === 'rustc-link-arg-bin' ? value.slice(value.indexOf('=') + 1) : value;
      return arg.startsWith('-Wl,') ? arg.slice(4).split(',') : [arg];
    });
  const dirs = directives.filter(([name, value]) => name === 'rustc-link-search' && (!value.includes('=') || /^(native|all)=/.test(value)))
    .map(([, value]) => value.replace(/^(native|all)=/, ''));
  const libraries = directives.filter(([name]) => name === 'rustc-link-lib').flatMap(([, value]) => {
    const equals = value.indexOf('=');
    const name = value.slice(equals + 1).split(':')[0];
    const modifiers = equals < 0 ? undefined : value.slice(0, equals).split(':')[1];
    if (modifiers?.split(',').includes('+verbatim')) return [name];
    return [`lib${name}.a`, `lib${name}.so`];
  });
  const paths = [
    ...args.filter(arg => !arg.startsWith('-') && /\.(o|a|so|obj|lib)$/.test(arg)).map(arg => resolve(cwd, arg)),
    ...dirs.flatMap(dir => libraries.map(name => resolve(cwd, dir, name))),
  ];
  return createHash('sha256').update(JSON.stringify([directives, watchedDigest([...new Set(paths)].sort(), [])])).digest('hex');
}

function refreshAndroidUnit(app, dir, target, env, workspace, outputs, watch) {
  const {out, files, executable, cwd} = JSON.parse(readFileSync(androidRegistry(dir), 'utf8'));
  if (!existsSync(executable) || !existsSync(out)) return null;
  const output = resolve(dir, 'rust-output.txt'), inputs = resolve(dir, 'inputs.json');
  let previous;
  try { previous = JSON.parse(readFileSync(inputs, 'utf8')).link; }
  catch { /* without a saved digest, force a relink */ }
  bakeOutside(app, 'android', target, env, undefined, {workspace, outputs, dir});
  const record = JSON.parse(readFileSync(inputs, 'utf8'));
  const link = androidLinkDigest(readFileSync(output, 'utf8'), cwd);
  let changed = previous !== link;
  if (changed) {
    // Cargo must relink changed objects and replay changed directives.
    writeFileSync(watch, '');
  }
  if (record.link !== link) {
    writeFileSync(`${inputs}.${process.pid}.tmp`, JSON.stringify({...record, link}));
    renameSync(`${inputs}.${process.pid}.tmp`, inputs);
  }
  // Cargo tracks these include! inputs through rustc. A build-script watch
  // would dirty the next build after the script itself wrote them.
  for (const file of files) {
    const bytes = readFileSync(resolve(dir, file)), dest = resolve(out, file);
    if (existsSync(dest) && bytes.equals(readFileSync(dest))) continue;
    const next = `${dest}.${process.pid}.tmp`;
    writeFileSync(next, bytes);
    renameSync(next, dest);
    changed = true;
  }
  return changed;
}

/** Only the unit Cargo reports may supply the APK's bake. With one cached
 * unit, prepare it before Cargo so changed Rust needs just one invocation. */
export function buildAndroidBake(app, pkg, target, env, workspace, outputs, library, build) {
  const effective = cargoBuildEnv(workspace, cargoEnvironment(env));
  if (effective.EXACT_RUST_BUNDLE || effective.EXACT_UPDATE_TRUST === 'production' ||
      !androidLinkable(pkg, app.generated || !!app.manifest.game)) return {result:build(env)};
  const root = linkedBakeDir(env, 'android', target), watch = resolve(root, '.cargo-watch');
  mkdirSync(root, {recursive:true});
  // Cargo watches a file created before its run, so deleting the bake invalidates
  // the script without making a bake's own writes trigger another compile.
  if (!existsSync(watch)) writeFileSync(watch, '');
  const cached = new Map(readdirSync(root).map(name => resolve(root, name)).filter(dir => existsSync(androidRegistry(dir)))
    .map(dir => [dir, readFileSync(androidRegistry(dir), 'utf8')]));
  const candidate = cached.size === 1 ? cached.keys().next().value : null;
  const cargoEnv = {...env, EXACT_BAKED:root, EXACT_ANDROID_BAKE_LIBRARY:library};
  let prepared, rejected;
  const refresh = dir => {
    const changed = refreshAndroidUnit(app, dir, target, cargoEnv, workspace, outputs, watch);
    if (changed !== null) prepared = dir;
    return changed;
  };
  if (candidate) try { refresh(candidate); } catch (error) {
    if (!error.contract) throw error;
    // Cargo can replace a baker after its Rust inputs change.
    rejected = error;
  }
  const unitDir = result => {
    const unit = (result.stdout ?? '').split('\n').filter(line => line.startsWith('{')).map(JSON.parse)
      .find(message => message.reason === 'build-script-executed' && message.package_id === pkg.id);
    return unit && androidUnitDir(root, unit.out_dir);
  };
  const run = recover => {
    const result = build(cargoEnv, recover), dir = unitDir(result);
    if (!dir) throw new Error('Cargo did not report the Android bake unit');
    return {result, dir};
  };
  // Cargo can encounter a stale include! in a unit we could not refresh
  // before the build. The failed run still names the unit that needs a bake.
  let built = run(result => {
    const dir = unitDir(result);
    return dir && dir !== prepared && (dir !== candidate || !rejected) && cached.has(dir) &&
      readFileSync(androidRegistry(dir), 'utf8') === cached.get(dir) && refresh(dir);
  });
  if (built.result.status !== 0) built = run();
  if (!existsSync(androidRegistry(built.dir))) {
    // Cargo has an output for this unit but its separate registration was deleted.
    writeFileSync(watch, '');
    built = run();
  }
  let registration = readFileSync(androidRegistry(built.dir), 'utf8');
  if (registration === cached.get(built.dir)) {
    if (built.dir === candidate && rejected) throw rejected;
    if (built.dir !== prepared && refresh(built.dir)) {
      const compiled = run();
      if (compiled.dir !== built.dir) throw new Error('Cargo changed the Android bake unit during compilation');
      built = compiled;
      registration = readFileSync(androidRegistry(built.dir), 'utf8');
    }
  }
  // A changed registration identifies a bake Cargo ran with this build's environment.
  if (registration !== cached.get(built.dir)) {
    recordRustBake({...cargoEnv, EXACT_BAKED:built.dir}, outputs, workspace);
  }
  return built;
}

/** Publish the watches of a Rust bake bootstrapped or refreshed by Cargo.
 * A first build must have this record before its driver checks freshness. */
export function recordRustBake(env, outputs = [], workspace) {
  const dir = env.EXACT_BAKED;
  if (!dir) return;
  if (!existsSync(resolve(dir, 'rust-baker.json'))) {
    if (workspace) throw new Error(`${dir}: Cargo did not register the Android baker`);
    return;
  }
  const baker = JSON.parse(readFileSync(resolve(dir, 'rust-baker.json'), 'utf8'));
  const at = resolve(dir, 'inputs.json'), tool = `${baker.executable} ${statSync(baker.executable).mtimeMs}`, pair = pairOf(dir, !!workspace);
  let previous; try { previous = JSON.parse(readFileSync(at, 'utf8')); } catch { /* bootstrap */ }
  if (previous?.version === 2 && previous.tool === tool && previous.pair === pair && previous.at >= baker.started) return;
  const cargoEnv = cargoEnvironment(env);
  const runEnv = { ...(workspace ? cargoBuildEnv(workspace, cargoEnv) : cargoEnv), ...baker.env, OUT_DIR: dir, EXACT_BAKED: dir, EXACT_RUST_BAKE: '1' };
  const output = readFileSync(resolve(dir, 'rust-output.txt'), 'utf8'), lines = output.split('\n');
  const said = key => lines.map(line => new RegExp(`^cargo::?${key}=(.*)$`).exec(line)?.[1]).filter(value => value !== undefined);
  const paths = cargoInputPaths(output, baker.cwd);
  const snapshots = new Map();
  const record = { version: 2, at: baker.started, tool, pair, warnings: said('warning'),
    ...(workspace ? {link:androidLinkDigest(output, baker.cwd)} : {}),
    env: Object.fromEntries(said('rerun-if-env-changed').map(name => [name, runEnv[name] ?? null])),
    // Cargo's bootstrap predates this call: never certify inputs changed since its read.
    paths, digest:inputDigest(paths, dir, runEnv, {since:baker.since ?? baker.started, outputs, snapshots}), missing: paths.filter(path => !existsSync(path)), watchedDirectories:[...snapshots.values()] };
  writeInputs(at, record);
}

/** An Apple bake's warning lines for app `name`, in the compiler's order: each, but the SF Symbols only the web draws
 * blank (`webOnly`), which are one line counting them at the first one's place, and the literal colours that stay one
 * in dark mode, one line past one (`oneScheme`), unless a drive reads them (`EXACT_WARNINGS_WHOLE`), which lists each in
 * its file and says the one line itself. */
export function bakeWarnings(name, warnings) {
  const web = warnings.filter(webOnly), one = web.length === 1, lines = [];
  for (const warning of process.env.EXACT_WARNINGS_WHOLE ? warnings : oneScheme(warnings)) {
    if (!webOnly(warning)) lines.push(warning);
    else if (warning === web[0]) lines.push(`${warning.split(' warning [')[0]} warning [lower-symbol-platform] ${web.length} SF Symbol use${one ? '' : 's'} draw${one ? 's' : ''} blank on the web, here first: an icon token or a role draws on every host (\`bun exact.mjs contract build --json\` says each, and how)`);
  }
  return lines.map((line) => `warning: ${name}: ${line}`);
}

/** Places what the link carries, `plan`, the `compat` text naming it (`id`) and the module's `bytecode`, in `dir`,
 * where one app's links alone read it (`linkedPlanArgs`): each file replaced whole and only when its bytes change.
 * Its directory and digest, which `<product>.linked` records. */
export function placeLinkedPlan(dir, plan, compat, id, bytecode) {
  if (JSON.parse(compat).id !== id) throw new Error(`host/apple: the linked compat.json is not the bake's (${id})`);
  mkdirSync(dir, { recursive: true });
  for (const [file, bytes] of [['app.plan', plan], ['compat.json', compat], ['app.hbc', bytecode]]) {
    const path = resolve(dir, file);
    if (existsSync(path) && readFileSync(path).equals(bytes)) continue;
    writeFileSync(`${path}.${process.pid}.tmp`, bytes);
    renameSync(`${path}.${process.pid}.tmp`, path);
  }
  return { dir, digest: createHash('sha256').update(plan).update(compat).update(bytecode).digest('hex') };
}
