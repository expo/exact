/**
 * The tools from Rust that exact2's scripts run (LLP 1174): the Contract compiler (`contract`), the web compiler
 * (`exact-web-js`), the filesystem helper the web build's file inventory runs (`exact-filesystem`) and the web's four
 * leaf modules. An app with no Rust of its own needs nothing else from Rust, so a
 * set of them built for one machine triple lets that app be checked and web-built without Rust.
 *
 * Each executable reads exact2's files (the standard modules, the bake's TypeScript analysis) from the checkout
 * `EXACT_ROOT` names, so one kept for the machine, built in another checkout or taken from a set reads this checkout's
 * files (D1).
 *
 * A set is matched by the bytes it was built from (D3): its manifest lists, for each product, every input in the
 * checkout (Cargo's dep-info, each path crate's manifest, the build scripts and what they watch), the recipe a script
 * adds, the product's closure in `Cargo.lock` and the workspace manifest without its member lists. A checkout takes a
 * product when every input's bytes are its own. Sets live in `~/.cache/exact/tools/<triple>/`; `bun scripts/tools.mjs
 * keep` builds one from this checkout's committed sources (D4.1).
 *
 * @ref LLP 1174 D1 D3 D4 D5
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compilerPaths, cargoInputPaths, crateDir, depInfoOf, freshCompiler } from './cargo-dep-info.mjs';
export { freshCompiler } from './cargo-dep-info.mjs';
import { cargoEnvironment, cargoOnPath } from './cargo-check.mjs';
import { builtFilesystemHelper } from './filesystem.mjs';
import { digest, lockClosure, seen, workspaceManifest } from './kept.mjs';
import { EXE, within } from './paths.mjs';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** `env` for a tool run for this checkout: `EXACT_ROOT` names it, over any the caller inherited. */
export const toolEnv = (env = process.env) => ({ ...env, EXACT_ROOT: ROOT });

/** A tool to run: its command and leading arguments, the environment to run it in, and `done()`, for when it ends. */
export const launch = (cmd, pre = [], done = () => {}, env = process.env) => ({ cmd, pre, env: toolEnv(env), done });

/** The packages whose source roots the web build's receipt holds (host/web/build.mjs `webSourceRoots`). */
export const WEB_CRATES = ['exact-web-js', 'exact-web', 'exact-js-bake', 'exact-markdown-editor', 'exact-web-js-motion', 'exact-textflow'];

const TRIPLES = { 'darwin-arm64': 'aarch64-apple-darwin', 'darwin-x64': 'x86_64-apple-darwin', 'linux-x64': 'x86_64-unknown-linux-gnu', 'linux-arm64': 'aarch64-unknown-linux-gnu', 'win32-x64': 'x86_64-pc-windows-msvc', 'win32-arm64': 'aarch64-pc-windows-msvc' };
/** This machine's target triple, as Rust names it, without asking rustc. */
export const hostTriple = () => TRIPLES[`${process.platform}-${process.arch}`] ?? `${process.arch}-${process.platform}`;

const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const home = (env) => env.HOME ?? env.USERPROFILE ?? homedir();
const cacheDir = (env = process.env) => resolve(home(env), '.cache/exact/tools', hostTriple());
const pin = () => Bun.TOML.parse(readFileSync(resolve(ROOT, 'rust-toolchain.toml'), 'utf8')).toolchain.channel;
const short = (commit) => commit?.slice(0, 12) ?? 'an unknown commit';

/** Whether this machine cannot build exact2's Rust: no `cargo`, or not the pinned toolchain (D5). */
export function rustMissing(env = process.env) {
  if (!cargoOnPath({ ...env }, home(env))) return true;
  return !existsSync(resolve(env.RUSTUP_HOME ?? resolve(home(env), '.rustup'), 'toolchains', `${pin()}-${hostTriple()}`));
}

/** Whether a fetched set may run here: only where Rust is missing, or where `EXACT_TOOLS_ORIGIN` names an origin;
 * never under `none` (D5). */
export function fetchedAllowed(env = process.env) {
  const origin = env.EXACT_TOOLS_ORIGIN;
  return origin !== 'none' && (!!origin || rustMissing(env));
}

/** The sets in this machine's cache, newest first, each `{ dir, manifest }`. */
function sets(env = process.env) {
  const dir = cacheDir(env);
  let names;
  try { names = readdirSync(dir).filter(n => !n.includes('.')); } catch { return []; }
  return names.flatMap(name => {
    try { return [{ dir: resolve(dir, name), manifest: JSON.parse(readFileSync(resolve(dir, name, 'manifest.json'), 'utf8')) }]; } catch { return []; }
  }).sort((a, b) => (b.manifest.built ?? 0) - (a.manifest.built ?? 0));
}

/** Where product `name` of a set differs from this checkout: the first input whose bytes are not its own, or the
 * closure or workspace manifest; null when it matches. */
function differs(product) {
  for (const [file, bytes] of product.inputs) if (digest(resolve(ROOT, file)) !== bytes) return file;
  try {
    if (JSON.stringify(lockClosure(ROOT, product.crate)) !== JSON.stringify(product.closure)) return `Cargo.lock (${product.crate}'s closure)`;
    if (sha(JSON.stringify(workspaceManifest(ROOT))) !== product.workspace) return 'Cargo.toml (the workspace manifest)';
  } catch (error) { return error.message; }
  return null;
}

/** What a checkout saw of a matched product's inputs; while it is unchanged the product is not hashed again (D3). */
const glance = (entry, product) => sha(JSON.stringify([entry, product.inputs.map(([file]) => seen(resolve(ROOT, file))), seen(resolve(ROOT, 'Cargo.lock')), seen(resolve(ROOT, 'Cargo.toml'))]));
const stampOf = (name) => resolve(ROOT, 'target/tools', `${name}.json`);

/**
 * Product `name` from the newest set in this machine's cache that `source` made (`built` here by `keep`, or
 * `fetched`) and whose inputs are this checkout's bytes: `{ path, set, product }`, or null.
 */
export function fromSet(name, source, env = process.env) {
  try {
    const last = JSON.parse(readFileSync(stampOf(name), 'utf8'));
    const set = { dir: last.entry, manifest: JSON.parse(readFileSync(resolve(last.entry, 'manifest.json'), 'utf8')) };
    const product = set.manifest.products[name];
    if (within(cacheDir(env), set.dir) && set.manifest.source === source && glance(last.entry, product) === last.glance && existsSync(resolve(set.dir, product.file)))
      return { path: resolve(set.dir, product.file), set, product };
  } catch {}
  for (const set of sets(env)) {
    const product = set.manifest.products?.[name];
    if (set.manifest.source !== source || !product || differs(product)) continue;
    try {
      mkdirSync(dirname(stampOf(name)), { recursive: true });
      writeFileSync(stampOf(name), JSON.stringify({ entry: set.dir, glance: glance(set.dir, product) }));
    } catch {}
    return { path: resolve(set.dir, product.file), set, product };
  }
  return null;
}

/** The set D5's order takes product `name` from, after this checkout's own fresh build: one built here, else (where a
 * kept product does not answer first) a fetched one where that is allowed. */
export const fromSets = (name, env = process.env) => fromSet(name, 'built', env) ?? (fetchedAllowed(env) ? fromSet(name, 'fetched', env) : null);

/** Thrown where product `name` would need Cargo and Rust is missing: says why no set matched, the newest set's commit
 * (and how to check it out, when the tree is clean), then how to install Rust (D5). Its message is one
 * `error [rust-missing]: …` line, the form `contract build --json` keeps the id of. */
export class RustMissing extends Error {
  constructor(name, env = process.env) {
    const newest = sets(env).find(set => set.manifest.products?.[name]);
    let why = `no tool set on this machine has it (${cacheDir(env).replace(home(env), '~')})`, commit = '';
    if (newest) {
      const at = differs(newest.manifest.products[name]), built = short(newest.manifest.commit);
      why = at ? `${at} differs from the newest tool set's, built at ${built}` : `the tool set built at ${built} that matches was fetched, and EXACT_TOOLS_ORIGIN=none runs none`;
      // Its commit to check out, when that moves nothing of the author's and is not where the checkout is.
      const git = (...args) => spawnSync('git', ['-C', ROOT, ...args], { encoding: 'utf8' });
      const clean = git('status', '--porcelain', '--untracked-files=no');
      if (at && newest.manifest.commit && clean.status === 0 && clean.stdout === '' && git('rev-parse', 'HEAD').stdout.trim() !== newest.manifest.commit)
        commit = `; \`git -C ${ROOT} checkout ${newest.manifest.commit}\` brings this checkout to that set's sources`;
    }
    super(`error [rust-missing]: ${name} builds with Rust, which this machine does not have (cargo, with the pinned toolchain ${pin()}), and ${why}${commit}. Install Rust with rustup (https://rustup.rs), then run bun ${resolve(ROOT, 'scripts/exact.mjs')} setup`);
    this.rustMissing = true;
  }
  /** Says it on stderr, or, for `contract build --json`, as that reply's one diagnostic, at `file`. */
  say(json = false, file = '') {
    if (!json) console.error(this.message);
    else process.stdout.write(`${JSON.stringify([{ id: 'rust-missing', severity: 'error', message: this.message.replace(/^error \[rust-missing\]: /, ''), file, line: 0, col: 0, end_col: 0, related: [] }])}\n`);
  }
}

const CONTRACT = resolve(ROOT, 'target/debug', `contract${EXE}`);
/** The Contract compiler built already, to `launch`: this checkout's debug build, else a tool set's; null when neither is. */
export const builtContract = () => { const path = existsSync(CONTRACT) ? CONTRACT : fromSets('contract')?.path; return path ? launch(path) : null; };

/**
 * `exact contract`'s compiler in D5's order: this checkout's debug build while it is fresh, a tool set's, else `cargo`
 * (Cargo's `launch`-shaped command). Null where only Cargo could build it and Rust is missing, after saying so (on
 * stderr, or as `--json`'s one diagnostic, at `file`).
 */
export async function contractTool(cargo, { env = process.env, json = false, file = '' } = {}) {
  const built = freshCompiler(CONTRACT) ? CONTRACT : fromSets('contract', env)?.path;
  if (built) return launch(built, [], undefined, env);
  if (rustMissing(env)) { new RustMissing('contract', env).say(json, file); return null; }
  if (!existsSync(CONTRACT)) console.error('exact contract: building the Contract compiler once (about a minute on a cold checkout; `exact new` starts it in the background)…');
  return cargo;
}

/** Cargo's metadata as `webSourceRoots` reads it, for `WEB_CRATES`' closure: its local packages with their targets,
 * and every reachable package's dependencies. Paths are relative to the checkout, so another checkout reads it as its
 * own (`setMetadata`). */
function recordMetadata() {
  const result = spawnSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], { cwd: ROOT, env: cargoEnvironment(), encoding: 'utf8', maxBuffer: 128 << 20 });
  if (result.status !== 0) throw new Error(`cargo metadata: ${result.stderr}`);
  const metadata = JSON.parse(result.stdout), packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node])), seenIds = new Set();
  const pending = WEB_CRATES.map(name => metadata.packages.find(pkg => pkg.name === name).id);
  while (pending.length) {
    const id = pending.pop(); if (seenIds.has(id)) continue; seenIds.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) if (dep.dep_kinds.some(kind => kind.kind !== 'dev')) pending.push(dep.pkg);
  }
  const ids = [...seenIds].sort(), local = ids.map(id => packages.get(id)).filter(pkg => !pkg.source);
  const record = {
    packages: ids.map(id => { const { name, source, manifest_path, targets } = packages.get(id); return source ? { id, name, source } : { id, name, source: null, manifest_path, targets: targets.map(({ name, kind, src_path }) => ({ name, kind, src_path })) }; }),
    resolve: { nodes: ids.map(id => ({ id, deps: (nodes.get(id)?.deps ?? []).map(({ pkg, dep_kinds }) => ({ pkg, dep_kinds: dep_kinds.map(({ kind }) => ({ kind })) })) })) },
  };
  const text = JSON.stringify(record).replaceAll(JSON.stringify(ROOT).slice(1, -1), '<root>');
  const files = ['Cargo.toml', 'Cargo.lock', ...local.map(pkg => relative(ROOT, pkg.manifest_path))];
  return { record: JSON.parse(text), inputs: files.map(file => [file, digest(resolve(ROOT, file))]) };
}

/** The Cargo metadata the set the web compiler is taken from recorded for `WEB_CRATES` (null when its inputs differ
 * here), in this checkout's paths: what the web build reads in place of asking Cargo (D5). */
export function setMetadata(set) {
  const recorded = set?.manifest.metadata;
  if (!recorded || recorded.inputs.some(([file, bytes]) => digest(resolve(ROOT, file)) !== bytes)) return null;
  const metadata = JSON.parse(JSON.stringify(recorded.record).replaceAll('<root>', JSON.stringify(ROOT).slice(1, -1)));
  return { ...metadata, workspace_root: ROOT, target_directory: resolve(ROOT, 'target') };
}

/** The nearest directory at or above `file`, inside the checkout, holding a `Cargo.toml`. */
function crateOf(file) {
  for (let dir = dirname(file); within(ROOT, dir) && dir !== ROOT; dir = dirname(dir)) if (existsSync(resolve(dir, 'Cargo.toml'))) return dir;
  return null;
}

/**
 * A product's inputs in the checkout, relative to it: every file Cargo's dep-info lists outside the target
 * directories `skip` (a generated source is not one: what generates it is listed instead), each path crate's
 * manifest, each of those crates' build script with what it includes and watches (from the build directories
 * `buildDirs`), and `recipe`, the files a script adds to Cargo's inputs. Sources outside the checkout (the registry's,
 * the toolchain's) are pinned by the closure and the toolchain the manifest declares.
 */
function inputsOf({ depInfo, buildDirs, skip, recipe }) {
  const files = new Set(recipe.filter(file => existsSync(resolve(ROOT, file)))), crates = new Set();
  const take = (path) => {
    if (skip.some(dir => within(dir, path)) || !within(ROOT, path)) return;
    files.add(relative(ROOT, path));
    const crate = crateOf(path);
    if (crate) crates.add(crate);
  };
  for (const path of compilerPaths(readFileSync(depInfo, 'utf8'), ROOT)) take(path);
  for (const crate of [...crates]) {
    files.add(relative(ROOT, resolve(crate, 'Cargo.toml')));
    const manifest = Bun.TOML.parse(readFileSync(resolve(crate, 'Cargo.toml'), 'utf8'));
    const script = manifest.package?.build ?? (existsSync(resolve(crate, 'build.rs')) ? 'build.rs' : null);
    if (!script) continue;
    take(resolve(crate, script));
    const name = manifest.package.name, list = (dir) => { try { return readdirSync(dir); } catch { return []; } };
    // A unit is `<package>-<hash>` (stable Cargo's build directory) or `<package>/<hash>` (the web nightly's).
    const units = buildDirs.flatMap(builds => list(builds).flatMap(dir => dir === name ? list(resolve(builds, dir)).map(hash => resolve(builds, dir, hash))
      : dir.replace(/-[0-9a-f]+$/, '') === name ? [resolve(builds, dir)] : []));
    for (const unit of units) {
      for (const dir of [unit, resolve(unit, 'out')]) for (const file of list(dir).filter(f => /^build_script_build(-[0-9a-f]+)?\.d$/.test(f))) {
        for (const path of compilerPaths(readFileSync(resolve(dir, file), 'utf8'), ROOT)) take(path);
      }
      for (const output of [resolve(unit, 'output'), resolve(unit, 'run/stdout')].filter(existsSync)) {
        for (const path of cargoInputPaths(readFileSync(output, 'utf8'), crate)) take(path);
      }
    }
  }
  return [...files].sort();
}

/** `bun scripts/tools.mjs keep`: build this checkout's tool set and keep it in the machine's cache (D4.1). */
async function keep() {
  if (rustMissing()) throw new Error(`tools keep builds the set, with Rust: install it with rustup (https://rustup.rs) and the pinned toolchain ${pin()}`);
  const triple = /^host: (.+)$/m.exec(spawnSync('rustc', ['-vV'], { cwd: ROOT, encoding: 'utf8' }).stdout ?? '')?.[1];
  if (triple !== hostTriple()) throw new Error(`rustc builds for ${triple}, and this machine's tool sets are ${hostTriple()}'s`);
  const commit = spawnSync('git', ['-C', ROOT, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).stdout.trim();
  const target = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : resolve(ROOT, 'target');
  const env = cargoEnvironment();
  delete env.RUSTUP_TOOLCHAIN;
  const products = {}, files = new Map();
  // The two executables at host-dev, each built alone, as the dev loop builds its design reader.
  for (const crate of ['contract', 'exact-web-js']) {
    console.log(`tools: building ${crate} (host-dev)`);
    const built = spawnSync('cargo', ['build', '-q', '--profile', 'host-dev', '-p', crate, '--bin', crate], { cwd: ROOT, env, stdio: 'inherit' });
    if (built.status !== 0) throw new Error(`${crate} did not build`);
    const product = resolve(target, 'host-dev', `${crate}${EXE}`);
    files.set(crate, product);
    products[crate] = { file: crate, crate, profile: 'host-dev', toolchain: pin(),
      inputs: inputsOf({ depInfo: resolve(target, 'host-dev', `${crate}.d`), buildDirs: [resolve(target, 'host-dev/build')], skip: [target], recipe: ['rust-toolchain.toml', '.cargo/config.toml'] }) };
  }
  // The filesystem helper, which the web build's file inventory runs, built as scripts/filesystem.mjs builds it.
  console.log('tools: building exact-filesystem');
  const helper = builtFilesystemHelper(), helperBuilt = resolve(helper.directory, `debug/exact-filesystem${EXE}`);
  // A capture it took without building is checked by its inputs; the dep-info read here must be that build's.
  if (!existsSync(helperBuilt) || sha(readFileSync(helperBuilt)) !== sha(readFileSync(helper.path))) throw new Error(`${helperBuilt} is not the helper ${helper.path}, so its dep-info cannot name that helper's inputs`);
  files.set('exact-filesystem', helper.path);
  products['exact-filesystem'] = { file: 'exact-filesystem', crate: 'exact-filesystem', profile: 'dev', toolchain: pin(),
    inputs: inputsOf({ depInfo: resolve(helper.directory, 'debug/exact-filesystem.d'), buildDirs: [resolve(helper.directory, 'debug/build')], skip: [target],
      recipe: ['rust-toolchain.toml', '.cargo/config.toml', 'scripts/filesystem.mjs', 'scripts/rustc/exact-rustc'] }) };
  const { leafRecipes } = await import('../host/web-js/module.mjs');
  for (const leaf of await leafRecipes()) {
    files.set(leaf.name, leaf.wasm);
    products[leaf.name] = { file: leaf.name, crate: leaf.crate, profile: leaf.profile, toolchain: leaf.toolchain, flags: leaf.flags, wasmOpt: leaf.wasmOpt,
      inputs: inputsOf({ depInfo: leaf.depInfo, buildDirs: leaf.buildDirs, skip: [target], recipe: ['host/web-js/module.mjs', 'scripts/cargo-check.mjs', '.cargo/config.toml'] }) };
  }
  // Only committed sources: a set serves every checkout with these bytes, so it is never an edit in progress.
  const all = [...new Set(Object.values(products).flatMap(product => product.inputs))];
  const dirty = spawnSync('git', ['-C', ROOT, 'status', '--porcelain', '--', ...all], { encoding: 'utf8' }).stdout.trim();
  if (dirty) throw new Error(`tools keep keeps committed sources only, and these inputs are not:\n${dirty}`);
  for (const product of Object.values(products)) {
    product.inputs = product.inputs.map(file => [file, digest(resolve(ROOT, file))]);
    product.closure = lockClosure(ROOT, product.crate);
    product.workspace = sha(JSON.stringify(workspaceManifest(ROOT)));
    product.target = triple;
  }
  const manifest = { version: 1, triple, commit, source: 'built', built: Date.now(), products, metadata: recordMetadata() };
  const id = sha(JSON.stringify(Object.entries(products).map(([name, product]) => [name, product.inputs, product.closure, product.workspace]))).slice(0, 24);
  const dir = cacheDir(), entry = resolve(dir, id), making = `${entry}.${process.pid}`;
  rmSync(making, { recursive: true, force: true });
  mkdirSync(making, { recursive: true });
  for (const [name, file] of files) {
    const kept = resolve(making, name);
    copyFileSync(file, kept);
    // The executables are kept stripped; the profile is not, so its builds stay incremental.
    if (!name.endsWith('.wasm')) {
      chmodSync(kept, 0o755);
      if (spawnSync('strip', [kept]).status !== 0) console.warn(`tools: ${name} kept unstripped (strip failed)`);
    }
    const bytes = readFileSync(kept);
    products[name].sha256 = sha(bytes);
    products[name].size = bytes.length;
  }
  writeFileSync(resolve(making, 'manifest.json'), `${JSON.stringify(manifest, null, 1)}\n`);
  rmSync(entry, { recursive: true, force: true });
  renameSync(making, entry);
  // The newest four sets of this triple: a set is about 100 MB.
  for (const { dir: old } of sets().slice(4)) rmSync(old, { recursive: true, force: true });
  console.log(`tools: kept the set for ${short(commit)} in ${entry.replace(homedir(), '~')}`);
  for (const [name, product] of Object.entries(products)) console.log(`  ${name}: ${(product.size / 1e6).toFixed(1)} MB, ${product.inputs.length} inputs`);
}

const USAGE = `usage: bun scripts/tools.mjs keep

keep  build this checkout's tool set (the Contract compiler, the web compiler, the
      filesystem helper and the web's four leaf modules) from its committed sources, and keep it in
      ~/.cache/exact/tools/${hostTriple()}/, where a checkout with the same input bytes
      takes it in place of building with Rust (LLP 1174).`;

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [verb] = process.argv.slice(2);
  // Not awaited at the top level: this module is in an import cycle with filesystem.mjs, and with a top-level await
  // a dynamic import of a module in that cycle waits for this module's evaluation, so `keep` never finishes.
  if (verb === 'keep') {
    keep().catch((error) => { console.error(`tools: ${error.message}`); process.exit(1); });
  } else {
    console.log(USAGE);
    process.exit(verb === '--help' || verb === '-h' ? 0 : 2);
  }
}
