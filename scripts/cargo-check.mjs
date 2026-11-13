import { existsSync, mkdirSync, readFileSync, realpathSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { spawn, spawnSync } from '../host/apple/toolchain.mjs';
import { homedir } from 'node:os';
import { createHash } from 'node:crypto';
import { withHostDeployment } from './filesystem.mjs';
import { delimiter, dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

/** Add rustup's tools when a non-interactive shell omitted them from PATH. */
export function cargoOnPath(env = process.env, home = homedir()) {
  const dirs = (env.PATH ?? '').split(delimiter).filter(Boolean);
  if (dirs.some(dir => existsSync(resolve(dir, 'cargo')))) return true;
  const rustup = resolve(env.CARGO_HOME ?? resolve(home, '.cargo'), 'bin');
  if (!existsSync(resolve(rustup, 'cargo'))) return false;
  env.PATH = [rustup, ...dirs].join(delimiter);
  return true;
}

/** Every Cargo process Exact starts refuses implicit Hermes acquisition. */
export function cargoEnvironment(env = process.env) { return withHostDeployment({ ...env, HERMES_LEAN_SYS_OFFLINE: '1' }); }

/** The web artifacts' toolchain (LLP 1047 §6, §10): a pinned nightly builds
 * std for size, and a panic aborts without its message or location. The web
 * shows neither: wasm32-unknown-unknown has no panic output and the glue
 * reads no trap. Everything else builds with rust-toolchain.toml's stable. */
export const WEB_TOOLCHAIN = 'nightly-2026-08-21';
export const WEB_STD = ['-Zbuild-std=std,panic_abort', '-Zbuild-std-features=optimize_for_size'];
// Deprecation is stable's to judge (the six checks' clippy): the nightly
// deprecates names, like `fetch_update`, before stable has their successors.
export const WEB_RUSTFLAGS = ['-Zunstable-options', '-Cpanic=immediate-abort', '-Zlocation-detail=none', '-Adeprecated'];
export const WEB_SIZE = [...WEB_STD, ...WEB_RUSTFLAGS].join(' ');
/** The environment for a web build, refusing a machine without the toolchain
 * or its std sources (Cargo builds offline, so std's dependencies too). */
export function webToolchainEnv(env) {
  const sysroot = spawnSync('rustc', ['--print', 'sysroot'], { env: { ...env, RUSTUP_TOOLCHAIN: WEB_TOOLCHAIN }, encoding: 'utf8' });
  if (sysroot.status !== 0 || !existsSync(resolve(sysroot.stdout.trim(), 'lib/rustlib/src/rust/library/Cargo.lock'))) throw new Error(
    `the web artifact builds with ${WEB_TOOLCHAIN} and its std sources (LLP 1047):\n` +
    `  rustup toolchain install ${WEB_TOOLCHAIN} --profile minimal --component rust-src --component llvm-tools\n` +
    `  cargo +${WEB_TOOLCHAIN} fetch --manifest-path "$(rustc +${WEB_TOOLCHAIN} --print sysroot)/lib/rustlib/src/rust/library/Cargo.toml"`);
  // `-Zbuild-std` builds std offline, so std's own locked sources must be in
  // Cargo's cache. A successful preparation belongs to that cache and sysroot.
  const library = resolve(sysroot.stdout.trim(), 'lib/rustlib/src/rust/library');
  const home = (process.platform === 'win32' ? env.USERPROFILE : env.HOME) || homedir();
  const cargoHome = resolve(env.CARGO_HOME || resolve(home, '.cargo'));
  const marker = resolve(cargoHome, '.exact', 'web-std', createHash('sha256').update(library).digest('hex'));
  if (!existsSync(marker)) {
    const offline = spawnSync('cargo', ['metadata', '--offline', '--locked', '--format-version', '1', '--manifest-path', resolve(library, 'Cargo.toml')], { env: cargoEnvironment({ ...env, RUSTUP_TOOLCHAIN: WEB_TOOLCHAIN }), encoding: 'utf8', maxBuffer: 1 << 26 });
    let ready = offline.status === 0;
    if (!ready) {
      console.error(`${WEB_TOOLCHAIN}: std's sources are not fetched; fetching them once (cargo fetch --manifest-path …/library/Cargo.toml)`);
      ready = spawnSync('cargo', ['fetch', '--locked', '--manifest-path', resolve(library, 'Cargo.toml')], { env: cargoEnvironment({ ...env, RUSTUP_TOOLCHAIN: WEB_TOOLCHAIN }), stdio: ['ignore', 'inherit', 'inherit'] }).status === 0;
    }
    if (ready) {
      try {
        mkdirSync(dirname(marker), { recursive: true });
        writeFileSync(marker, '');
      } catch { /* a read-only Cargo home checks again next time */ }
    }
  }
  return cargoEnvironment({ ...env, RUSTUP_TOOLCHAIN: WEB_TOOLCHAIN, EXACT_WEB_SIZE: WEB_SIZE });
}

/** Cargo's unused-patch diagnostics for the patches every outside workspace must carry (LLP 1036.001 D2).
 * Match both package and canonical path; an author's patch with the same name still needs attention.
 * Cargo shares one help block across consecutive unused patches, so keep it if any of them is the author's. */
export function cargoDiagnosticFilter(write) {
  const real = path => { try { return realpathSync(path); } catch { return resolve(path); } };
  const required = new Set(Object.entries(Bun.TOML.parse(readFileSync(resolve(ROOT, 'Cargo.toml'), 'utf8')).patch?.['crates-io'] ?? {})
    .map(([name, spec]) => `${name} ${real(resolve(ROOT, spec.path))}`));
  const hint = [
    'help: Check that the patched package version and available features are compatible',
    '      with the dependency requirements. If the patch has a different version from',
    '      what is locked in the Cargo.lock file, run `cargo update` to use the new',
    '      version. This may also occur with an optional dependency that is not enabled.',
  ];
  let pending = '', patches = false, kept = false, help = 0;
  const line = text => {
    const plain = text.replace(/\x1b\[[0-9;]*m/g, '').replace(/\r?\n$/, '');
    const unused = /^warning: patch `(\S+) v\S+ \((.+)\)` was not used in the crate graph$/.exec(plain);
    if (unused) {
      patches = true; help = 0;
      if (required.has(`${unused[1]} ${real(unused[2])}`)) return;
      kept = true;
    } else if (patches && plain === hint[help]) {
      const keep = kept;
      if (++help === hint.length) { patches = false; kept = false; help = 0; }
      if (!keep) return;
    } else { patches = false; kept = false; help = 0; }
    write(text);
  };
  return {
    write(text) {
      pending += text;
      let end;
      while ((end = pending.indexOf('\n')) >= 0) {
        line(pending.slice(0, end + 1)); pending = pending.slice(end + 1);
      }
    },
    end() { if (pending) line(pending); pending = ''; },
  };
}

/** A child drains stderr while the synchronous bake waits, forwarding progress immediately. Stdout stays Cargo's
 * JSON. The relay inherits the build environment without loading another workspace's settings from .env. */
export function cargoDiagnosticsCommand(command, args, options) {
  return command === 'cargo' && options.stdio?.[2] === 'inherit'
    ? [process.execPath, ['--no-env-file', fileURLToPath(import.meta.url), ...args], options] : [command, args, options];
}

async function streamCargo(args) {
  const child = spawn('cargo', args, { stdio: ['ignore', 'inherit', 'pipe'] });
  const ended = new Promise((ok, fail) => { child.once('error', fail); child.once('close', (code, signal) => ok({ code, signal })); });
  const handlers = ['SIGINT', 'SIGTERM', 'SIGHUP'].map(signal => {
    const handler = () => child.kill(signal);
    process.on(signal, handler); return [signal, handler];
  });
  const filter = cargoDiagnosticFilter(text => process.stderr.write(text));
  try {
    child.stderr.setEncoding('utf8');
    for await (const text of child.stderr) filter.write(text);
    filter.end();
    const { code, signal } = await ended;
    for (const [signal, handler] of handlers) process.off(signal, handler);
    if (signal) process.kill(process.pid, signal); else process.exitCode = code;
  } finally { for (const [signal, handler] of handlers) process.off(signal, handler); }
}

if (import.meta.main) await streamCargo(process.argv.slice(2));

/** What `resolveApp` asked Cargo of an outside app's authored workspace (`cargo metadata --locked`, ~0.1 s), asked when a
 * build first reads `app.workspace` (LLP 1153 D2.1: a web verb never does), kept while the files Cargo reads for it are
 * as they were (`cargoInputs`): in this process, and for the next in the app's `.exact/cargo-check.json`. The workspace
 * itself is found from the files (`authoredWorkspace`), never by `cargo locate-project`. */
const outsideChecks = new Map();
/** The files whose size and time decide the answer: the app's crate manifests (`manifests`), a `Cargo.toml` above the
 * app, where a workspace could start; the workspace's manifest, lock and toolchain pin, and each manifest its `members`
 * name, a glob's too; Cargo's configuration, the workspace's, its parents' and `CARGO_HOME`'s; exact2's manifest and pin
 * (`outsideWorkspaceProblems` holds the workspace's to them); and each path package's manifest `cargo metadata` listed,
 * with each `Cargo.toml` above it, where the workspace it inherits from is. */
function cargoInputs(dir, manifests, workspace, packages) {
  const above = [], configs = [];
  for (let at = dirname(dir); at !== dirname(at); at = dirname(at)) above.push(resolve(at, 'Cargo.toml'));
  for (let at = workspace; ; at = dirname(at)) { configs.push(resolve(at, '.cargo/config.toml'), resolve(at, '.cargo/config')); if (at === dirname(at)) break; }
  const home = process.env.CARGO_HOME ?? resolve(homedir(), '.cargo');
  let members = [];
  try {
    members = (Bun.TOML.parse(readFileSync(resolve(workspace, 'Cargo.toml'), 'utf8')).workspace?.members ?? [])
      .flatMap(pattern => [...new Bun.Glob(`${pattern}/Cargo.toml`).scanSync({ cwd: workspace, absolute: true, onlyFiles: true })]).sort();
  } catch { /* an unreadable manifest is the signature's own '-' */ }
  const inherited = new Set();
  for (const manifest of packages) for (let at = dirname(dirname(manifest)); at !== dirname(at); at = dirname(at)) inherited.add(resolve(at, 'Cargo.toml'));
  return [...manifests, ...above, ...['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'].map(file => resolve(workspace, file)), `members ${members.join(' ')}`, ...members,
    ...configs, resolve(home, 'config.toml'), resolve(home, 'config'), resolve(ROOT, 'Cargo.toml'), resolve(ROOT, 'rust-toolchain.toml'), ...packages, ...[...inherited].sort()];
}
/** Each file's size and time, or that it is missing; a line that names no file (`members …`) as it is. */
const fileSignature = (paths) => paths.map(path => { if (path.startsWith('members ')) return path; try { const s = statSync(path); return `${s.size}:${s.mtimeMs}`; } catch { return '-'; } }).join('\n');
export function knownWorkspace(dir, outside, manifests) {
  const key = `${dir}\0${outside}`;
  try {
    const known = outsideChecks.get(key) ?? JSON.parse(readFileSync(resolve(dir, '.exact/cargo-check.json'), 'utf8'));
    if (known.key !== key || typeof known.workspace !== 'string' || !Array.isArray(known.packages)) return null;
    if (known.signature !== fileSignature(cargoInputs(dir, manifests, known.workspace, known.packages))) return null;
    outsideChecks.set(key, known);
    return known.workspace;
  } catch { return null; }
}
/** Remembers what `check` found for `workspace` (the path packages' manifests), unless a file it read may have changed
 * while it ran, when Cargo may have read either version: the inputs it was known to read (with the path packages the
 * last check found) differ before and after it, a path package's manifest is gone, or any input was written since it
 * started (a second's slack for the clock). */
export function rememberWorkspace(dir, outside, manifests, workspace, check) {
  const key = `${dir}\0${outside}`;
  let earlier = outsideChecks.get(key)?.packages;
  try { earlier ??= JSON.parse(readFileSync(resolve(dir, '.exact/cargo-check.json'), 'utf8')).packages; } catch { /* none yet */ }
  if (!Array.isArray(earlier)) earlier = [];
  const started = Date.now(), before = fileSignature(cargoInputs(dir, manifests, workspace, earlier)), packages = check();
  if (fileSignature(cargoInputs(dir, manifests, workspace, earlier)) !== before || !packages.every(path => existsSync(path))) return;
  const inputs = cargoInputs(dir, manifests, workspace, packages);
  if (inputs.some(path => { try { return statSync(path).mtimeMs >= started - 1000; } catch { return false; } })) return;
  const known = { key, workspace, packages, signature: fileSignature(inputs) };
  outsideChecks.set(key, known);
  try { mkdirSync(resolve(dir, '.exact'), { recursive: true }); const at = resolve(dir, `.exact/cargo-check.json.${process.pid}`); writeFileSync(at, JSON.stringify(known)); renameSync(at, resolve(dir, '.exact/cargo-check.json')); } catch { /* asked again next time */ }
}

/** The generated native workspace of an app that authored none (LLP 1153 D1): under its `.exact/`, which is ignored. */
export const shellsDir = (dir) => resolve(dir, '.exact/shells');
const readToml = (path) => { try { return Bun.TOML.parse(readFileSync(path, 'utf8')); } catch { return null; } };
/** The host crates an app writes beside app.json: each one only its host's build reads. */
export const HOST_CRATES = ['apple', 'ios', 'macos', 'linux', 'web', 'windows', 'android'];
/** The crates an app writes beside app.json. */
export const APP_CRATES = [...HOST_CRATES, 'data', 'gpu'];
/** How an app with Rust of its own gets the workspace it needs (LLP 1153 D1.4: the generated one, copied up). */
export const NO_WORKSPACE = 'An app with Rust of its own (a gpu/ or data/ crate) copies the generated workspace up from .exact/shells/, which a native verb writes only while no such crate is there: `bun exact.mjs contract vocab gpu-module` says how.';
/** Whether the workspace at `root` owns the package at `path`, as Cargo reads it: a `members` glob names it, or a
 * member depends on it by a path under the root (Cargo's automatic membership), and `exclude` (a prefix or a glob)
 * does not name it. Read from the files alone. */
function owns(root, workspace, path) {
  const under = (p) => { const rel = relative(root, p).split(sep).join('/'); return rel && !rel.startsWith('..') ? rel : null; };
  const excluded = (rel) => (workspace.exclude ?? []).some(pattern => { const prefix = pattern.replace(/\/+$/, ''); return rel === prefix || rel.startsWith(prefix + '/') || new Bun.Glob(prefix).match(rel); });
  const rel = under(path);
  if (!rel || excluded(rel)) return false;
  if ((workspace.members ?? []).some(pattern => new Bun.Glob(pattern.replace(/\/+$/, '')).match(rel))) return true;
  // The members' path dependencies under the root, transitively, each a member unless excluded.
  const seen = new Set(), queue = (workspace.members ?? []).flatMap(pattern => [...new Bun.Glob(`${pattern.replace(/\/+$/, '')}/Cargo.toml`).scanSync({ cwd: root, absolute: true, onlyFiles: true })].map(dirname));
  while (queue.length) {
    const member = queue.shift();
    if (seen.has(member)) continue;
    seen.add(member);
    const manifest = readToml(resolve(member, 'Cargo.toml'));
    // Its three dependency tables, each target's three, and a dependency inherited from the workspace's table.
    const tables = [manifest, ...Object.values(manifest?.target ?? {})].flatMap(t => ['dependencies', 'dev-dependencies', 'build-dependencies'].map(k => t?.[k] ?? {}));
    for (const table of tables) {
      for (const [name, spec] of Object.entries(table)) {
        const dep = spec?.workspace === true ? { ...workspace.dependencies?.[name], from: root } : { ...spec, from: member };
        if (!dep?.path) continue;
        const at = resolve(dep.from, dep.path), depRel = under(at);
        if (!depRel || excluded(depRel)) continue;
        if (at === path) return true;
        queue.push(at);
      }
    }
  }
  return false;
}
/** The authored Cargo workspace an app builds in, read from the files alone (no `cargo locate-project`: a web verb runs
 * no Cargo, LLP 1153 D2.1), as Cargo would find it: the app's own `Cargo.toml` when it declares `[workspace]`; the
 * workspace a package names (`package.workspace`, the app's or a crate's beside app.json); else the nearest enclosing
 * workspace whose `members` own the app or one of its crates; else the app itself, a root of its own package or
 * crates. `null` when the app authored no Cargo manifest at all: its workspace is generated (scripts/app-shells.mjs).
 * An enclosing workspace that does not list the app (another repository the app sits in) is not the app's. */
export function authoredWorkspace(dir) {
  const own = readToml(resolve(dir, 'Cargo.toml'));
  if (own?.workspace !== undefined) return dir;
  if (own?.package?.workspace) return resolve(dir, own.package.workspace);
  const crates = APP_CRATES.map(kind => resolve(dir, kind)).filter(crate => existsSync(resolve(crate, 'Cargo.toml')));
  for (const crate of crates) { const declared = readToml(resolve(crate, 'Cargo.toml'))?.package?.workspace; if (declared) return resolve(crate, declared); }
  if (!own && !crates.length) return null;
  for (let at = dirname(dir); at !== dirname(at); at = dirname(at)) {
    const workspace = readToml(resolve(at, 'Cargo.toml'))?.workspace;
    if (workspace && [dir, ...crates].some(path => owns(at, workspace, path))) return at;
  }
  return dir;
}

/** The Cargo configuration files a build in `cwd` reads, lowest precedence first: `$CARGO_HOME`'s, then each `.cargo/`
 * from the root down to `cwd`, `config` before `config.toml` in each
 * (https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure). Each `{ path, configuration }`;
 * an invalid one is refused, saying it could not `purpose`. */
function cargoConfigurations(cwd, env, purpose) {
  const directories = [];
  for (let at = resolve(cwd); ; at = dirname(at)) {
    directories.push(at);
    if (at === dirname(at)) break;
  }
  const home = env.CARGO_HOME ?? resolve(homedir(), '.cargo');
  return [...new Set([home, ...directories.reverse().map(at => resolve(at, '.cargo'))])].flatMap(directory => {
    const path = ['config', 'config.toml'].map(name => resolve(directory, name)).find(existsSync);
    if (!path) return [];
    try { return [{ path, configuration: Bun.TOML.parse(readFileSync(path, 'utf8')) }]; }
    catch { throw new Error(`Cannot ${purpose} because Cargo configuration ${path} is unreadable or invalid. Fix its TOML before building.`); }
  });
}

/** The `--config` that gives `target` the rustflags `flags` beside those its Cargo configuration gives it. A TOML array
 * keeps an argument with a space whole, and Cargo appends it to a configured array. Cargo refuses to merge an array
 * into a configured string ("expected string, but found array"), so over a string the flags are one string, the
 * configured flags first, which replaces it; a flag with a space cannot join one. */
export function targetRustflags(target, flags, { cwd = process.cwd(), env = process.env } = {}) {
  const configured = cargoConfigurations(cwd, env, `pass ${target}'s rustflags`).findLast(({ configuration }) => configuration.target?.[target]?.rustflags !== undefined);
  const value = configured?.configuration.target[target].rustflags;
  if (typeof value !== 'string') return ['--config', `target.${target}.rustflags=${JSON.stringify(flags)}`];
  const spaced = flags.find(flag => /\s/.test(flag));
  if (spaced) throw new Error(`${configured.path}: [target.${target}] rustflags is a string, which Cargo splits at whitespace, so the build cannot add ${spaced} to it; write it as an array, rustflags = ${JSON.stringify(value.trim().split(/\s+/).filter(Boolean))}`);
  return ['--config', `target.${target}.rustflags=${JSON.stringify([value.trim(), ...flags].filter(Boolean).join(' '))}`];
}

/** Make Apple's deployment environment part of every target unit's Cargo hash. Add to each existing flags source
 * so Cargo keeps choosing and merging flags itself; introducing a target table would hide `build.rustflags`. */
export function appleCargo(command, arguments_, options = {}) {
  const targets = arguments_.filter((value, index) => arguments_[index - 1] === '--target' || value.startsWith('--target='));
  const apple = command === 'cargo' && (targets.some(value => value.includes('-apple-')) ||
    !targets.length && process.platform === 'darwin' && ['build', 'rustc', 'check', 'test', 'clippy'].includes(arguments_[0]));
  if (!apple) return [command, arguments_, options];
  const environment = { ...(options.env ?? process.env) };
  // No SDKROOT, whatever the caller inherited: rustc and cc-rs ask xcrun for each target's own SDK. One left here
  // reaches the build scripts' C and C++ compiles for the Mac too, where cc-rs passes no -isysroot and clang takes
  // SDKROOT: an iOS SDK there fails `-Werror` (aws-lc-sys's probes) on -Wincompatible-sysroot.
  delete environment.SDKROOT;
  const sources = new Map(), configuredEnvironment = new Map();
  for (const { path, configuration } of cargoConfigurations(options.cwd ?? process.cwd(), environment, 'track Apple deployment targets')) {
    if (configuration.build?.rustflags !== undefined) sources.set('build.rustflags', configuration.build.rustflags);
    for (const [selector, target] of Object.entries(configuration.target ?? {})) {
      if (target.rustflags !== undefined) sources.set(`target.${JSON.stringify(selector)}.rustflags`, target.rustflags);
    }
    for (const [name, value] of Object.entries(configuration.env ?? {})) {
      const previous = configuredEnvironment.get(name);
      configuredEnvironment.set(name, typeof value === 'string' ? { value, path, table: false } : { ...previous, ...value, path, table: true });
    }
  }
  const minimums = ['MACOSX_DEPLOYMENT_TARGET', 'IPHONEOS_DEPLOYMENT_TARGET', 'TVOS_DEPLOYMENT_TARGET', 'WATCHOS_DEPLOYMENT_TARGET'];
  const values = minimums.map(name => {
    const setting = configuredEnvironment.get(name);
    if (!setting || environment[name] !== undefined && !setting.force) return environment[name] ?? '';
    return setting.relative ? resolve(dirname(dirname(setting.path)), setting.value) : setting.value;
  });
  const prefix = '--cfg=exact_apple_deployment_targets=';
  const flag = prefix + JSON.stringify(values.join(':'));
  const flagVariable = name => ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS'].includes(name) || /^CARGO_TARGET_.+_RUSTFLAGS$/.test(name);
  const append = (value, name) => {
    const separator = name === 'CARGO_ENCODED_RUSTFLAGS' ? '\x1f' : ' ';
    const flags = (separator === ' ' ? value.trim().split(/\s+/) : value.split(separator)).filter(value => value && !value.startsWith(prefix));
    return [...flags, flag].join(separator);
  };
  const configurationArgs = [];
  const set = (key, value) => configurationArgs.push('--config', `${key}=${JSON.stringify(value)}`);
  for (const name of Object.keys(environment).filter(flagVariable)) environment[name] = append(environment[name], name);
  for (const [name, setting] of configuredEnvironment) {
    if (flagVariable(name)) set(`env.${name}${setting.table ? '.value' : ''}`, append(setting.value, name));
  }
  if (!sources.has('build.rustflags')) sources.set('build.rustflags', []);
  for (const [key, flags] of sources) {
    const selector = key.startsWith('target.') ? JSON.parse(key.slice(7, -10)) : null;
    const variable = selector === null ? 'CARGO_BUILD_RUSTFLAGS' : /^[\w-]+$/.test(selector) ? `CARGO_TARGET_${selector.toUpperCase().replaceAll('-', '_')}_RUSTFLAGS` : null;
    // Keep the config source for variables in [env]: Cargo exports them after resolving its own flags
    if (variable && environment[variable] !== undefined) continue;
    if (typeof flags === 'string') set(key, append(flags, 'RUSTFLAGS'));
    else if (Array.isArray(flags) && flags.every(value => typeof value === 'string')) set(key, [flag]);
    else throw new Error(`Cannot track Apple deployment targets because Cargo ${key} is not a string or string array. Fix its configuration before building.`);
  }
  const separator = arguments_.indexOf('--'), at = separator < 0 ? arguments_.length : separator;
  return [command, [...arguments_.slice(0, at), ...configurationArgs, ...arguments_.slice(at)], { ...options, env: environment }];
}

/** The link argument that aligns an Android library's LOAD segments to 16 KB, which a device with 16 KB pages needs
 * to run it outside page-size compatibility mode (https://developer.android.com/guide/practices/page-sizes); NDKs
 * before r28 align to 4 KB. */
export const ANDROID_PAGE_SIZE = '-Clink-arg=-Wl,-z,max-page-size=16384';

/** A Cargo run for `aarch64-linux-android` with ANDROID_PAGE_SIZE added to every rustflags source it may use, and none
 * made: CARGO_ENCODED_RUSTFLAGS or RUSTFLAGS when set, the target's own variable when set, and `build.rustflags`
 * through `--config`, which Cargo joins with the configuration's array and CARGO_BUILD_RUSTFLAGS. A new target source
 * would displace `build.rustflags` (a game's `-fp-contract=off`), and a variable would reach the host Cargo runs that
 * build scripts start, whose linker refuses the argument. Other runs pass through. */
export function androidCargo(command, arguments_, options = {}) {
  const android = command === 'cargo' && arguments_.some((value, index) => value === '--target=aarch64-linux-android' || value === 'aarch64-linux-android' && arguments_[index - 1] === '--target');
  if (!android) return [command, arguments_, options];
  const environment = { ...(options.env ?? process.env) };
  const add = (value, separator) => value.split(separator).includes(ANDROID_PAGE_SIZE) ? value : [value, ANDROID_PAGE_SIZE].filter(Boolean).join(separator);
  if (environment.CARGO_ENCODED_RUSTFLAGS !== undefined) environment.CARGO_ENCODED_RUSTFLAGS = add(environment.CARGO_ENCODED_RUSTFLAGS, '\x1f');
  else if (environment.RUSTFLAGS !== undefined) environment.RUSTFLAGS = add(environment.RUSTFLAGS.trim(), ' ');
  const own = 'CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS';
  if (environment[own] !== undefined) environment[own] = add(environment[own].trim(), ' ');
  const separator = arguments_.indexOf('--'), at = separator < 0 ? arguments_.length : separator;
  return [command, [...arguments_.slice(0, at), '--config', `build.rustflags=${JSON.stringify([ANDROID_PAGE_SIZE])}`, ...arguments_.slice(at)], { ...options, env: environment }];
}
