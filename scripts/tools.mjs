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
import { createHash, createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { parseArgs } from 'node:util';
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compilerPaths, cargoInputPaths, crateDir, depInfoOf, freshCompiler } from './cargo-dep-info.mjs';
export { freshCompiler } from './cargo-dep-info.mjs';
import { cargoEnvironment, cargoOnPath } from './cargo-check.mjs';
import { builtFilesystemHelper, filesystemLock, offlineMiss } from './filesystem.mjs';
import { digest, lockClosure, seen, workspaceManifest } from './kept.mjs';
import { EXE, within } from './paths.mjs';
import { canonicalBytes } from './deploy-signing.mjs';

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
const cacheDir = (env = process.env, triple = hostTriple()) => resolve(home(env), '.cache/exact/tools', triple);
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
function sets(env = process.env, triple = hostTriple()) {
  const dir = cacheDir(env, triple);
  let names;
  try { names = readdirSync(dir).filter(n => !n.includes('.')); } catch { return []; }
  return names.flatMap(name => {
    try { return [{ dir: resolve(dir, name), manifest: JSON.parse(readFileSync(resolve(dir, name, 'manifest.json'), 'utf8')) }]; } catch { return []; }
  }).sort((a, b) => (b.manifest.built ?? 0) - (a.manifest.built ?? 0));
}

/** Where product `name` of a set differs from this checkout: the first input whose bytes are not its own, or the
 * closure or workspace manifest; null when it matches. */
function differs(product, root = ROOT) {
  for (const [file, bytes] of product.inputs) if (digest(resolve(root, file)) !== bytes) return file;
  try {
    if (JSON.stringify(lockClosure(root, product.crate)) !== JSON.stringify(product.closure)) return `Cargo.lock (${product.crate}'s closure)`;
    if (sha(JSON.stringify(workspaceManifest(root))) !== product.workspace) return 'Cargo.toml (the workspace manifest)';
  } catch (error) { return error.message; }
  return null;
}

/** What a checkout saw of a matched product's inputs; while it is unchanged the product is not hashed again (D3). */
const glance = (entry, product) => sha(JSON.stringify([entry, product.inputs.map(([file]) => seen(resolve(ROOT, file))), seen(resolve(ROOT, 'Cargo.lock')), seen(resolve(ROOT, 'Cargo.toml'))]));
const stampOf = (name) => resolve(ROOT, 'target/tools', `${name}.json`);

function productFile(set, product) {
  const path = resolve(set.dir, product.file);
  return statSync(path, { throwIfNoEntry: false })?.isFile() ? path : null;
}

/**
 * Product `name` from the newest set in this machine's cache that `source` made (`built` here by `keep`, or
 * `fetched`) and whose inputs are this checkout's bytes: `{ path, set, product }`, or null.
 */
export function fromSet(name, source, env = process.env) {
  try {
    const last = JSON.parse(readFileSync(stampOf(name), 'utf8'));
    const set = { dir: last.entry, manifest: JSON.parse(readFileSync(resolve(last.entry, 'manifest.json'), 'utf8')) };
    const product = set.manifest.products[name];
    if (within(cacheDir(env), set.dir) && set.manifest.source === source && glance(last.entry, product) === last.glance) {
      const path = productFile(set, product);
      if (path) return { path, set, product };
    }
  } catch {}
  for (const set of sets(env)) {
    const product = set.manifest.products?.[name];
    if (set.manifest.source !== source || !product) continue;
    const path = productFile(set, product);
    if (!path || differs(product)) continue;
    try {
      mkdirSync(dirname(stampOf(name)), { recursive: true });
      writeFileSync(stampOf(name), JSON.stringify({ entry: set.dir, glance: glance(set.dir, product) }));
    } catch {}
    return { path, set, product };
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
      const product = newest.manifest.products[name], built = short(newest.manifest.commit);
      const path = productFile(newest, product), at = path && differs(product);
      if (!path) why = `the tool set built at ${built} is missing ${product.file}`;
      else why = at ? `${at} differs from the newest tool set's, built at ${built}` : `the tool set built at ${built} that matches was fetched, and EXACT_TOOLS_ORIGIN=none runs none`;
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
/** The Contract compiler built already, to `launch`: this checkout's fresh debug build or a matching tool set's. */
export function builtContract(env = process.env) {
  const path = freshCompiler(CONTRACT) ? CONTRACT : fromSets('contract', env)?.path;
  return path ? launch(path, [], undefined, env) : null;
}

export function compilerCargo(verb, env = process.env) {
  const clean = cargoEnvironment(env), channel = pin();
  delete clean.RUSTUP_TOOLCHAIN;
  delete clean.CARGO_TARGET_DIR;
  const args = [`+${channel}`, verb, '-q', '--manifest-path', resolve(ROOT, 'Cargo.toml'), '-p', 'contract'];
  if (verb === 'run') args.push('--');
  return { ...launch('cargo', args, undefined, clean), pin: channel };
}

/** Select a fresh Contract compiler, building with the pinned Cargo when needed (D5).
 * A Rust-less machine receives the diagnostic in the form requested by the caller. */
export function contractTool({ env = process.env, json = false, file = '' } = {}) {
  const built = builtContract(env);
  if (built) return built;
  if (rustMissing(env)) { new RustMissing('contract', env).say(json, file); return null; }
  if (!existsSync(CONTRACT)) console.error('exact contract: building the Contract compiler once (about a minute on a cold checkout; `exact new` starts it in the background)…');
  // Built before and stale, as after a pull: Cargo runs quietly, so say why the wait (LLP 1156.006.000 D11).
  else console.error('exact contract: rebuilding the Contract compiler, since its sources changed (about a minute)…');
  return compilerCargo('run', env);
}

/** Cargo's metadata as `webSourceRoots` reads it, for `WEB_CRATES`' closure: its local packages with their targets,
 * and every reachable package's dependencies. Paths are relative to the checkout, so another checkout reads it as its
 * own (`setMetadata`). */
function recordMetadata() {
  const result = spawnSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], { cwd: ROOT, env: cargoEnvironment(), encoding: 'utf8', maxBuffer: 128 << 20 });
  if (result.status !== 0) throw new Error(`cargo metadata: ${result.stderr}${offlineMiss(result.stderr)}`);
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
export function setMetadata(set, root = ROOT) {
  const recorded = set?.manifest.metadata;
  if (!recorded || recorded.inputs.some(([file, bytes]) => digest(resolve(root, file)) !== bytes)) return null;
  const metadata = JSON.parse(JSON.stringify(recorded.record).replaceAll('<root>', JSON.stringify(root).slice(1, -1)));
  return { ...metadata, workspace_root: root, target_directory: resolve(root, 'target') };
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

const LINUX_TOOLS = 'x86_64-unknown-linux-gnu';
const buildsToolSet = triple => triple === hostTriple() || (hostTriple() === 'aarch64-apple-darwin' && triple === LINUX_TOOLS);

/** Build all seven products for one machine. The Mac producer also builds the Linux
 * executables with Zig. The four wasm modules are identical for both machines. */
async function keep(triple = hostTriple()) {
  if (rustMissing()) throw new Error(`tools keep builds the set, with Rust: install it with rustup (https://rustup.rs) and the pinned toolchain ${pin()}`);
  const host = /^host: (.+)$/m.exec(spawnSync('rustc', ['-vV'], { cwd: ROOT, encoding: 'utf8' }).stdout ?? '')?.[1];
  if (host !== hostTriple()) throw new Error(`rustc builds for ${host}, and this machine's tool sets are ${hostTriple()}'s`);
  const cross = triple !== host;
  if (!buildsToolSet(triple)) throw new Error(`cannot build a ${triple} tool set on ${host}`);
  const commit = spawnSync('git', ['-C', ROOT, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).stdout.trim();
  const target = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : resolve(ROOT, 'target');
  const env = cargoEnvironment();
  delete env.RUSTUP_TOOLCHAIN;
  const recipe = ['rust-toolchain.toml', '.cargo/config.toml'];
  if (cross) {
    recipe.push('scripts/tools.mjs', 'scripts/rustc/exact-linux-cc');
    env.PATH = [resolve(ROOT, 'scripts/rustc'), env.PATH].join(':');
    env.CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = 'exact-linux-cc';
    env.CRATE_CC_NO_DEFAULTS = '1';
    env.CC_x86_64_unknown_linux_gnu = 'exact-linux-cc';
    env.CXX_x86_64_unknown_linux_gnu = 'exact-linux-cc -x c++';
  }
  const products = {}, files = new Map();
  for (const crate of ['contract', 'exact-web-js', 'exact-filesystem']) {
    const nativeHelper = !cross && crate === 'exact-filesystem';
    const profile = nativeHelper ? 'dev' : 'host-dev';
    console.log(`tools: building ${crate} (${triple}, ${profile})`);
    let directory, file, buildDirs, inputs = recipe;
    if (nativeHelper) {
      const helper = builtFilesystemHelper();
      directory = resolve(helper.directory, 'debug');
      file = helper.path;
      // A reused capture must still match the build whose dep-info we read.
      const binary = resolve(directory, crate + EXE);
      if (!existsSync(binary) || sha(readFileSync(binary)) !== sha(readFileSync(file))) throw new Error(`${binary} is not the helper ${file}, so its dep-info cannot name that helper's inputs`);
      buildDirs = [resolve(directory, 'build')];
      inputs = [...recipe, 'scripts/filesystem.mjs', 'scripts/rustc/exact-rustc'];
    } else {
      const args = [cross ? 'rustc' : 'build', '--locked', '-q', '--profile', profile, '-p', crate, '--bin', crate];
      if (cross) args.push('--target', triple, '--', '-C', 'strip=symbols');
      const built = spawnSync('cargo', args, { cwd: ROOT, env, stdio: 'inherit' });
      if (built.status !== 0) throw new Error(`${crate} did not build for ${triple}`);
      directory = resolve(target, cross ? triple : '', profile);
      file = resolve(directory, crate + EXE);
      buildDirs = [resolve(directory, 'build')];
      if (cross) buildDirs.push(resolve(target, profile, 'build'));
    }
    files.set(crate, file);
    products[crate] = { file: crate, crate, profile, toolchain: pin(),
      inputs: inputsOf({ depInfo: resolve(directory, crate + '.d'), buildDirs, skip: [target], recipe: inputs }) };
  }
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
  const dir = cacheDir(process.env, triple), entry = resolve(dir, id), making = `${entry}.${process.pid}`;
  rmSync(making, { recursive: true, force: true });
  mkdirSync(making, { recursive: true });
  for (const [name, file] of files) {
    const kept = resolve(making, name);
    copyFileSync(file, kept);
    // The executables are kept stripped; the profile is not, so its builds stay incremental.
    if (!name.endsWith('.wasm')) {
      chmodSync(kept, 0o755);
      if (!cross && spawnSync('strip', [kept]).status !== 0) console.warn(`tools: ${name} kept unstripped (strip failed)`);
    }
    const bytes = readFileSync(kept);
    products[name].sha256 = sha(bytes);
    products[name].size = bytes.length;
  }
  writeFileSync(resolve(making, 'manifest.json'), `${JSON.stringify(manifest, null, 1)}\n`);
  rmSync(entry, { recursive: true, force: true });
  renameSync(making, entry);
  // The newest four sets of this triple: a set is about 100 MB.
  for (const { dir: old } of sets(process.env, triple).slice(4)) rmSync(old, { recursive: true, force: true });
  console.log(`tools: kept the set for ${short(commit)} in ${entry.replace(homedir(), '~')}`);
  for (const [name, product] of Object.entries(products)) console.log(`  ${name}: ${(product.size / 1e6).toFixed(1)} MB, ${product.inputs.length} inputs`);
  return { dir: entry, manifest };
}

/** Authenticate before using any path or input named by a remote manifest (LLP 1174 D4.2). */
export function readToolManifest(bytes, id, triple, publicKey) {
  if (sha(bytes) !== id) throw new Error('tool manifest digest differs from its lookup');
  const manifest = JSON.parse(bytes.toString('utf8'));
  if (manifest.kind !== 'exact-tool-set' || manifest.version !== 1 || manifest.triple !== triple || manifest.source !== 'fetched')
    throw new Error(`tool manifest is not a version 1 set for ${triple}`);
  const signature = Buffer.from(manifest.signature ?? '', 'base64');
  if (publicKey.asymmetricKeyType !== 'ed25519' || signature.length !== 64 || !verify(null, canonicalBytes(manifest), publicKey, signature))
    throw new Error('tool manifest signature does not match the producer key');
  return manifest;
}

const EXECUTABLES = ['contract', 'exact-web-js', 'exact-filesystem'];

function runProducts({ dir, manifest }, root) {
  const linux = manifest.triple === LINUX_TOOLS && hostTriple() !== LINUX_TOOLS;
  for (const name of ['markdown.wasm', 'motion.wasm', 'markup-editor.wasm', 'textflow.wasm']) {
    const product = manifest.products[name];
    if (!product || product.file !== name) throw new Error(`missing ${name} in the built tool set`);
    new WebAssembly.Module(readFileSync(resolve(dir, name)));
  }
  const digests = EXECUTABLES.map(name => {
    const product = manifest.products[name];
    if (product?.file !== name || !/^[a-f0-9]{64}$/.test(product.sha256)) throw new Error(`invalid product for ${name}`);
    return `${product.sha256}  ${name}`;
  }).join('\n');
  const commit = manifest.commit;
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(commit)) throw new Error('the tools verifier needs a Git commit');

  // Hold the verifier's worktree at the build revision until the three Linux tools finish.
  const unpack = `
test "$(uname -s)/$(uname -m)" = Linux/x86_64
tools="$work/products"
mkdir "$tools"
tar -xzf - -C "$tools"
tree="$HOME/projects/exact-tools-verifier"
if [ ! -d "$tree" ]; then echo "missing Linux verifier worktree: $tree" >&2; exit 1; fi
admin=$(git -C "$tree" rev-parse --absolute-git-dir)
common=$(git -C "$tree" rev-parse --path-format=absolute --git-common-dir)
if [ "$admin" = "$common" ] || [ -n "$(git -C "$tree" rev-parse --show-prefix)" ]; then echo "Linux verification needs a linked worktree" >&2; exit 1; fi
exec 9>"$admin/tools-verify.lock"
flock -x 9
branch=$(git -C "$tree" rev-parse --abbrev-ref HEAD)
changes=$(git -C "$tree" status --porcelain)
if [ "$branch" != HEAD ] || [ -n "$changes" ]; then
  echo "Linux verifier worktree must be clean and detached: $tree" >&2
  exit 1
fi
if ! git -C "$tree" cat-file -e '${commit}^{commit}' 2>/dev/null; then
  GIT_TERMINAL_PROMPT=0 git -C "$tree" fetch --quiet --no-tags origin '${commit}'
fi
git -C "$tree" switch --quiet --detach '${commit}'
export EXACT_ROOT="$tree"`;
  const probe = `set -eu
work=$(mktemp -d /tmp/exact-tools-run.XXXXXXXX)
trap 'rm -rf "$work"' EXIT
cd "$work"
${linux ? unpack : ''}
(cd "$tools" && ${linux ? 'sha256sum' : 'shasum -a 256'} -c - >&2 <<'DIGESTS'
${digests}
DIGESTS
)
printf 'component App\n  view\n    text "Hello"\n' > app.contract
"$tools/contract" build app.contract -o app.plan >&2
"$tools/exact-web-js" js app.plan -o web >&2
printf '{"op":"inventory","root":"%s/web"}\n' "$work" | "$tools/exact-filesystem"`;
  const pipe = `tar -czf - -C "$tools" ${EXECUTABLES.join(' ')} | ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=2 expo-build-1000 "$1"`;
  const command = linux ? ['bash', '-o', 'pipefail', '-c', pipe, 'verify-linux', probe] : ['sh', '-c', probe];
  const ran = spawnSync(command[0], command.slice(1), {
    env: { ...process.env, COPYFILE_DISABLE: '1', tools: dir, EXACT_ROOT: root },
    stdio: ['ignore', 'pipe', 'inherit'], encoding: 'utf8',
  });
  if (ran.status !== 0) throw new Error(`tool verification failed for ${manifest.triple}: ${ran.error?.message ?? ran.signal ?? ran.status}`);
  if (!(JSON.parse(ran.stdout).value?.['app.js']?.bytes > 0)) throw new Error('the compilers and filesystem helper did not produce and inventory app.js');
}

/** Make every product and its signed manifest available before publishing either lookup. */
export async function publishToolSet(origin, set, privateKey, newer = []) {
  const { blobPath, streamPath } = await import('./origin.mjs');
  if (privateKey.asymmetricKeyType !== 'ed25519') throw new Error('the tools producer needs an Ed25519 private key');
  const { manifest, dir } = set;
  const { commit } = manifest;
  if (!/^[0-9a-f]{40}(?:[0-9a-f]{24})?$/.test(commit)) throw new Error('tool lookup needs a full Git commit');
  if (manifest.source !== 'built' || !buildsToolSet(manifest.triple)) throw new Error('only a set built on this producer can be published');
  const stream = { channel: 'tools', compatibilityId: manifest.triple }, base = streamPath(stream);
  const remote = { ...manifest, kind: 'exact-tool-set', source: 'fetched' };
  remote.signature = sign(null, canonicalBytes(remote), privateKey).toString('base64');
  const bytes = Buffer.from(JSON.stringify(remote)), id = sha(bytes);
  for (const [name, product] of Object.entries(manifest.products)) {
    if (basename(product.file) !== product.file || !within(dir, resolve(dir, product.file))) throw new Error(`invalid product file for ${name}`);
    const body = readFileSync(resolve(dir, product.file));
    if (body.length !== product.size || sha(body) !== product.sha256) throw new Error(`${name} differs from the built tool set`);
    await origin.put(blobPath(product.sha256), body, { immutable: true });
  }
  await origin.put(`${base}/manifests/${id}.json`, bytes, { immutable: true });
  await recordCommit(origin, stream, base, commit, id, newer);
  return id;
}

function manifestId(value) {
  if (typeof value !== 'string' || !/^[0-9a-f]{64}$/.test(value)) throw new Error('invalid tool manifest digest in lookup');
  return value;
}

function recentSets(bytes) {
  if (bytes === null) return [];
  const list = JSON.parse(bytes.toString('utf8'));
  if (!Array.isArray(list)) throw new Error('recent tool lookup must be an array');
  return list.map(manifestId);
}

async function recordCommit(origin, stream, base, commit, id, newer) {
  await origin.withLock(stream, async () => {
    const path = `${base}/recent.json`;
    const recent = recentSets(await origin.get(path));
    // Backfill can finish after a newer set. Keep the eight most recent
    // distinct input states in main's order, including a tip that reverts.
    const ordered = new Set();
    for (const revision of newer) {
      const mapped = await origin.get(`${base}/commits/${revision}.json`);
      if (mapped !== null) ordered.add(manifestId(JSON.parse(mapped.toString('utf8'))));
      if (ordered.size === 8) break;
    }
    ordered.add(id);
    for (const previous of recent) ordered.add(previous);
    await origin.put(`${base}/commits/${commit}.json`, Buffer.from(JSON.stringify(id)));
    await origin.put(path, Buffer.from(JSON.stringify([...ordered].slice(0, 8))));
  });
}

function gitAt(tree, ...args) {
  const r = spawnSync('git', ['-C', tree, ...args], { encoding: 'utf8', maxBuffer: 32 << 20 });
  if (r.status !== 0) throw new Error(`git ${args[0]}: ${r.error?.message ?? r.stderr.trim()}`);
  return r.stdout.trim();
}

function cleanRevision(tree) {
  if (gitAt(tree, 'status', '--porcelain', '--untracked-files=normal')) throw new Error(`publish needs a clean producer checkout: ${tree}`);
  return gitAt(tree, 'rev-parse', 'HEAD');
}

function matchesRevision(manifest, tree) {
  return Object.values(manifest.products).every(product => !differs(product, tree)) && !!setMetadata({ manifest }, tree);
}

async function buildIn(tree, commit, triple) {
  if (tree === ROOT) return keep(triple);
  const env = { ...process.env, CARGO_TARGET_DIR: resolve(tree, 'target'), EXACT_ROOT: tree };
  for (const args of [['install', '--frozen-lockfile'], ['scripts/tools.mjs', 'keep', ...(triple === hostTriple() ? [] : ['--target', triple])]]) {
    const run = spawnSync(process.execPath, args, { cwd: tree, stdio: 'inherit', env });
    if (run.status !== 0) throw new Error(`bun ${args.join(' ')} failed for ${short(commit)}`);
  }
  const set = sets(process.env, triple).find(({ manifest }) => manifest.source === 'built' && manifest.commit === commit && manifest.triple === triple && matchesRevision(manifest, tree));
  if (!set) throw new Error(`keep produced no tool set for ${short(commit)}`);
  return set;
}

async function publishRevision(tree, origin, privateKey, triple, commit, newer = []) {
  const { streamPath } = await import('./origin.mjs');
  const stream = { channel: 'tools', compatibilityId: triple }, base = streamPath(stream);
  const checkRevision = () => {
    if (cleanRevision(tree) !== commit) throw new Error('producer checkout changed during publication');
  };
  checkRevision();
  const exact = await origin.get(`${base}/commits/${commit}.json`);
  const recent = recentSets(await origin.get(`${base}/recent.json`));
  const candidates = new Set(exact === null ? recent : [manifestId(JSON.parse(exact.toString('utf8'))), ...recent]);
  const publicKey = createPublicKey(privateKey);
  for (const id of candidates) {
    const bytes = await origin.get(`${base}/manifests/${id}.json`);
    if (!bytes) throw new Error(`missing published tool manifest ${id}`);
    const manifest = readToolManifest(bytes, id, triple, publicKey);
    if (!matchesRevision(manifest, tree)) continue;
    checkRevision();
    await recordCommit(origin, stream, base, commit, id, newer);
    console.log(`tools: ${short(commit)} matches published set ${id}`);
    return;
  }
  const set = await buildIn(tree, commit, triple);
  if (set.manifest.commit !== commit) throw new Error('producer checkout changed during build');
  if (set.manifest.triple !== triple) throw new Error(`candidate set belongs to ${set.manifest.triple}, expected ${triple}`);
  runProducts(set, tree);
  checkRevision();
  const id = await publishToolSet(origin, set, privateKey, newer);
  console.log(`tools: published ${triple} set ${id} for ${short(commit)}`);
}

/** One poll of the private producer worktree. Persist skipped commits before
 * checking one out. A build interrupted after publication can reuse its signed set. */
export async function publishWatchedTip(tree, stateFile, tip, publishOne, retryAt, now = Date.now) {
  let state = existsSync(stateFile) ? JSON.parse(readFileSync(stateFile, 'utf8')) : { tip: null, pending: [] };
  const save = () => {
    writeFileSync(stateFile + '.tmp', JSON.stringify(state));
    renameSync(stateFile + '.tmp', stateFile);
  };
  if (tip !== state.tip) {
    const range = state.tip ? gitAt(tree, 'rev-list', '--first-parent', '--boundary', `${state.tip}..${tip}`).split('\n') : [tip];
    let prior = state.pending;
    if (state.tip && !range.includes('-' + state.tip)) {
      const history = new Set(gitAt(tree, 'rev-list', '--first-parent', tip).split('\n'));
      prior = prior.filter(commit => history.has(commit));
    }
    state = { tip, pending: [...new Set([tip, ...range.filter(c => c && !c.startsWith('-')), ...prior])] };
    save();
  }
  const readyAt = now();
  const commit = state.pending.find(c => (retryAt.get(c) ?? 0) <= readyAt);
  if (!commit) return null;
  if (gitAt(tree, 'rev-parse', '--abbrev-ref', 'HEAD') !== 'HEAD') throw new Error(`tools watch needs a detached worktree: ${tree}`);
  if (cleanRevision(tree) !== commit) gitAt(tree, 'switch', '--detach', commit);
  const newer = commit === tip ? [] : gitAt(tree, 'rev-list', '--first-parent', `${commit}..${tip}`).split('\n');
  try { await publishOne(commit, newer); }
  catch (error) {
    retryAt.set(commit, now() + 60_000);
    return { commit, error };
  }
  retryAt.delete(commit);
  state.pending = state.pending.filter(c => c !== commit);
  save();
  return { commit };
}

async function watchProducer(tree, origin, privateKey, triple) {
  if (existsSync(tree)) {
    if (realpathSync(tree) !== realpathSync(gitAt(tree, 'rev-parse', '--show-toplevel'))) throw new Error(`tools watch needs the root of a worktree: ${tree}`);
    if (gitAt(tree, 'rev-parse', '--path-format=absolute', '--git-common-dir') !== gitAt(ROOT, 'rev-parse', '--path-format=absolute', '--git-common-dir'))
      throw new Error(`${tree} is not a worktree of ${ROOT}`);
  } else {
    gitAt(ROOT, 'worktree', 'add', '--detach', tree, 'HEAD');
  }
  const admin = gitAt(tree, 'rev-parse', '--absolute-git-dir');
  if (admin === gitAt(ROOT, 'rev-parse', '--absolute-git-dir') || admin === gitAt(tree, 'rev-parse', '--path-format=absolute', '--git-common-dir'))
    throw new Error('tools watch needs its own linked worktree');
  if (gitAt(tree, 'rev-parse', '--abbrev-ref', 'HEAD') !== 'HEAD') throw new Error(`tools watch needs a detached worktree: ${tree}`);
  const key = sha(JSON.stringify([origin.dir, triple]));
  const state = resolve(admin, `tools-${key}.json`), retryAt = new Map();
  for (;;) {
    try {
      const attempt = await filesystemLock(admin, 'tools-watch/.lock', async () => {
        gitAt(tree, 'fetch', '--quiet', 'origin', '+refs/heads/main:refs/remotes/origin/main');
        const tip = gitAt(tree, 'rev-parse', 'refs/remotes/origin/main');
        return publishWatchedTip(tree, state, tip, (commit, newer) => publishRevision(tree, origin, privateKey, triple, commit, newer), retryAt);
      });
      if (attempt?.error) console.error(`tools watch: ${short(attempt.commit)}: ${attempt.error.message}`);
      if (attempt) continue;
    } catch (error) { console.error(`tools watch: ${error.message}`); }
    await Bun.sleep(60_000);
  }
}

async function publish(args) {
  const { values } = parseArgs({ args, options: { target: { type: 'string' }, origin: { type: 'string' }, key: { type: 'string' }, watch: { type: 'boolean' }, worktree: { type: 'string' } } });
  if (!values.origin || !values.key) throw new Error('publish needs --origin <directory> and --key <producer.pem>');
  if (values.worktree && !values.watch) throw new Error('--worktree belongs to publish --watch');
  const triple = values.target ?? hostTriple();
  if (hostTriple() !== 'aarch64-apple-darwin' || !buildsToolSet(triple)) throw new Error(`cannot publish a ${triple} tool set from ${hostTriple()}`);
  if (/^[a-z][a-z\d+.-]*:\/\//i.test(values.origin)) throw new Error('--origin needs a local directory');
  const { DirectoryOrigin } = await import('./origin.mjs');
  const origin = new DirectoryOrigin(values.origin), privateKey = createPrivateKey(readFileSync(values.key));
  if (privateKey.asymmetricKeyType !== 'ed25519') throw new Error('the tools producer needs an Ed25519 private key');
  if (values.watch) {
    const tree = resolve(values.worktree ?? resolve(ROOT, '..', basename(ROOT) + '-tools-producer' + (triple === hostTriple() ? '' : '-' + triple)));
    await watchProducer(tree, origin, privateKey, triple);
  } else await publishRevision(ROOT, origin, privateKey, triple, cleanRevision(ROOT));
}

const USAGE = `usage: bun scripts/tools.mjs keep [--target <triple>]
       bun scripts/tools.mjs publish --origin <directory> --key <producer.pem> [--target <triple>] [--watch [--worktree <directory>]]

keep  build this checkout's tool set (the Contract compiler, the web compiler, the
      filesystem helper and the web's four leaf modules) from its committed sources, and keep it in
      ~/.cache/exact/tools/<triple>/, where a checkout with the same input bytes
      takes it in place of building with Rust (LLP 1174). On Apple Silicon,
      --target x86_64-unknown-linux-gnu builds a Linux set with Zig for glibc 2.31 or later.
publish  sign and publish this revision’s tool set, or record its commit against a matching published set.
         --target x86_64-unknown-linux-gnu runs the Linux executables on expo-build-1000
         over SSH before the Mac signs the set. The verifier uses a clean, detached linked
         worktree at ~/projects/exact-tools-verifier on that machine.
         --watch follows origin/main in a dedicated detached worktree, publishing the tip first
         and filling skipped commits newest first. The default worktree is a sibling named
         <checkout>-tools-producer, suffixed with the triple for a cross build. An existing worktree must be clean and detached.`;

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [verb, ...args] = process.argv.slice(2);
  // Not awaited at the top level: this module is in an import cycle with filesystem.mjs, and with a top-level await
  // a dynamic import of a module in that cycle waits for this module's evaluation, so `keep` never finishes.
  if (verb === 'keep' || verb === 'publish') {
    const run = async () => {
      if (verb === 'publish') return publish(args);
      const { values } = parseArgs({ args, options: { target: { type: 'string' } } });
      return keep(values.target);
    };
    run().catch((error) => { console.error(`tools: ${error.message}`); process.exit(1); });
  } else {
    console.log(USAGE);
    process.exit(verb === '--help' || verb === '-h' ? 0 : 2);
  }
}
