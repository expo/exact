// The platforms an app outside this repository ships, and the native workspaces it builds them in (LLP 1153 D1,
// LLP 1153.003 D1).
//
// The manifest is the declaration: `host.<platform>` names a platform, with `deploy.store.<platform>` for the three
// that hold an update store. A native verb names its platform the first time it runs (`bringPlatform`), and the crates
// a build needs are generated under the app's `.exact/shells/`, as a game's bake writes `.shells/` (game/app/shells.mjs).
// The app's own crates (a data crate, a GPU module) sit beside app.json and join the generated workspaces as members
// (`appMembers`); exact2 writes every host crate. An app with its own `Cargo.toml` is an authored workspace, which
// `resolveApp` takes instead (scripts/cargo-check.mjs `authoredWorkspace`; LLP 1153.003 D3).
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cargoCheckout, cargoEnvironment, checkoutPathRefusal, lockedMetadata, pathFrom, patchLines, readManifest as readAppManifest } from './app.mjs';
import { dependencyTables, pathWithin } from './paths.mjs';
import { directDependencies, nativePackages } from './package-libraries.mjs';
import { MEMBER_OF, appCrates, authoredWorkspace, gpuShellsDir, notAMember, shellsDir } from './cargo-check.mjs';
import { OFFLINE_MISSING } from './filesystem.mjs';
import { APPLE_BUILD_SCRIPT, appleBuildScript } from './linked-plan.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

/** The platforms a manifest can name, each with the crate that serves it. An Apple TV build has no key of its own: it
 * takes `host.ios` (scripts/app.schema.json). */
export const PLATFORMS = { ios: 'apple', macos: 'apple', linux: 'linux', android: 'android', windows: 'windows' };
/** SDK dependencies whose names app and game crates may not take. */
export const SDK_CRATES = new Set(['ibex2-sqlite', 'exact-logic', 'exact-apple', 'exact-apple-update', 'exact-js', 'exact-js-bake', 'exact-linux', 'exact-painted', 'exact-painted-update', 'exact-web', 'exact-web-capabilities', 'exact-js-web', 'exact-game-bake', 'exact-game-render']);

/** Authored host crates own their identity. Otherwise package.json keeps the scaffold's prefix across directory
 * and display-name changes; a caller's name or the folder supplies it for an app without a package manifest. */
export function appCrateName(dir, fallback = basename(dir), title = null) {
  const kinds = ['web', ...new Set(Object.values(PLATFORMS)), 'ios', 'macos', 'tvos'];
  for (const kind of kinds) {
    const file = resolve(dir, kind, 'Cargo.toml');
    if (!existsSync(file)) continue;
    try {
      const name = Bun.TOML.parse(readFileSync(file, 'utf8')).package?.name;
      if (name?.endsWith(`-${kind}`)) return name.slice(0, -kind.length - 1);
    } catch { /* Cargo reports an invalid authored manifest. */ }
  }
  let name = fallback;
  const pkg = resolve(dir, 'package.json');
  if (existsSync(pkg)) {
    const declared = JSON.parse(readFileSync(pkg, 'utf8')).name;
    if (typeof declared === 'string' && declared.trim()) { name = declared; title = null; }
  }
  const normalize = value => {
    const slug = value.normalize('NFKD').replace(/\p{M}/gu, '').toLowerCase()
      .replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'app';
    return /^[a-z]/.test(slug) ? slug : `app-${slug}`;
  };
  const collides = prefix => kinds.some(kind => SDK_CRATES.has(`${prefix}-${kind}`));
  name = normalize(name);
  if (collides(name) && title !== null) name = normalize(title);
  return collides(name) ? `${name}-app` : name;
}
/** The platforms that hold an update store, so a stream (scripts/deploy.mjs), and Windows, whose host refuses every
 * store level but "0" (it has no update store yet, LLP 1115.004 D15) while an absent key reads as "A". */
const STORED = ['ios', 'macos', 'linux', 'windows'];

/** A verb's platform (`ios`, `mac`, `linux`, `android`, `windows`), or a drive's host (`test ios`); null for the web and
 * every other verb. */
export function platformOf(verb, host = 'web') {
  const word = ['test', 'agent', 'verify'].includes(verb) ? host : verb;
  return { ios: 'ios', share: 'ios', mac: 'macos', macos: 'macos', linux: 'linux', android: 'android', windows: 'windows' }[word] ?? null;
}

/** What `host.<platform>` holds when a verb names the platform: the scaffold's defaults for the two Apple platforms,
 * a desktop window's size for Windows (the host chooses its painter, LLP 1149 D12), nothing
 * for the others (each key may be empty, scripts/app.schema.json). */
const hostDefaults = (platform) => ({
  ios: { minimumOS: '17.0', deviceFamily: ['iphone', 'ipad'] },
  macos: { minimumOS: '14.0', window: { width: 900, height: 700 } },
  windows: { window: { width: 1024, height: 720, minWidth: 360, minHeight: 500 } },
}[platform] ?? {});

const readManifest = (dir) => readAppManifest(dir, basename(dir));
const writeManifest = (dir, manifest) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest, null, 2) + '\n');

/** Whether the manifest names the platform. */
export const namesPlatform = (manifest, platform) => platform in (manifest.host ?? {});

/** Name `platform` in the app's manifest: `host.<platform>`, and its store for one that has a stream. Returns the
 * keys written, none when it was named already. */
export function namePlatform(dir, platform) {
  if (!(platform in PLATFORMS)) throw new Error(`${platform} is not a platform; the platforms are ${Object.keys(PLATFORMS).join(', ')}`);
  const manifest = readManifest(dir), wrote = [];
  manifest.host ??= {};
  if (!(platform in manifest.host)) { manifest.host[platform] = hostDefaults(platform); wrote.push(`host.${platform}`); }
  if (STORED.includes(platform)) {
    manifest.deploy ??= {};
    manifest.deploy.store ??= {};
    if (!(platform in manifest.deploy.store)) { manifest.deploy.store[platform] = '0'; wrote.push(`deploy.store.${platform}`); }
  }
  if (wrote.length) writeManifest(dir, manifest);
  return wrote;
}

/** Stop naming `platform`: its `host`, `deploy.store` and `deploy.binaries` keys go. Returns the keys removed. */
export function withoutPlatformKeys(dir, platform) {
  if (platform === 'web') throw new Error('update --without web: the web host is the dev loop\'s and the agent\'s (`web`, `test web`, `agent web`), so it stays');
  if (!(platform in PLATFORMS)) throw new Error(`update --without ${platform}: the platforms are ${Object.keys(PLATFORMS).join(', ')}`);
  const manifest = readManifest(dir), removed = [];
  for (const [key, table] of [['host', manifest.host], ['deploy.store', manifest.deploy?.store], ['deploy.binaries', manifest.deploy?.binaries]]) {
    if (table && platform in table) { delete table[platform]; removed.push(`${key}.${platform}`); }
  }
  if (removed.length) writeManifest(dir, manifest);
  return removed;
}

/** The crates the shells hold for a manifest: `web/` always (delivery's bake and the resident loop a native window
 * live-reloads from), and one per crate the named platforms share. Android has two: `android/`, the library the
 * Canvas reader's APK loads, and `linux/`, the Linux host built for Android, which `--headless` drives under adb. */
export function shellCrates(manifest) {
  return ['web', ...new Set(Object.keys(manifest.host ?? {}).filter(p => p in PLATFORMS).flatMap(p => p === 'android' ? ['linux', 'android'] : [PLATFORMS[p]]))];
}

/** A constructor's literal arguments, as the web build copies them into a module of its own. */
const LITERAL = /^(?:-?\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d[\d_]*)?(?:[iuf](?:8|16|32|64|128|size))?|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])'|true|false)$/;
const literalArguments = text => {
  const items = [];
  let start = 0, quote = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) { if (c === '\\') i++; else if (c === quote) quote = null; }
    else if (c === '"' || c === "'") quote = c;
    else if (c === ',') { items.push(text.slice(start, i)); start = i + 1; }
  }
  if (quote) return false;
  items.push(text.slice(start));
  return (items.length === 1 && !items[0].trim()) || items.every(item => LITERAL.test(item.trim()));
};

/** app.json's `rust.data` (LLP 1153.003 D2) as `{ krate, ty, lib, source }`, its constructor `<type>::default()` when
 * it names none; null when it names no data crate. Its grammar is the one contract's manifest reader checks
 * (`RustData`): the type itself, its `default()`, or its `new(…)` with literal arguments. */
export function rustDataOf(manifest) {
  const data = manifest.rust?.data;
  if (data === undefined) return null;
  const at = 'app.json rust.data';
  if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error(`${at} is an object: { "crate": "<name>-data", "type": "<name>_data::<Type>" }`);
  const extra = Object.keys(data).find(key => !['crate', 'type', 'source'].includes(key));
  if (extra) throw new Error(`${at} takes crate, type and source, not ${extra}`);
  if (typeof data.crate !== 'string' || !/^[A-Za-z0-9_-]+$/.test(data.crate)) throw new Error(`${at}.crate is the data crate's Cargo package name`);
  if (typeof data.type !== 'string' || !/^[A-Za-z_]\w*(?:::[A-Za-z_]\w*)+$/.test(data.type)) throw new Error(`${at}.type is a path from the data crate's library, as \`pinapp_data::Pins\``);
  const ty = data.type, source = data.source ?? `${ty}::default()`;
  const rest = typeof source === 'string' && source.startsWith(ty) ? source.slice(ty.length) : null;
  const constructs = rest === '' || rest === '::default()' || (rest?.startsWith('::new(') && rest.endsWith(')') && literalArguments(rest.slice(6, -1)));
  if (!constructs) throw new Error(`${at}.source is \`${ty}\`, \`${ty}::default()\` or \`${ty}::new(…)\` with literal arguments, not ${JSON.stringify(source)}; a source built from anything else is a module of its own (rust.module.package)`);
  return { krate: data.crate, ty, lib: ty.split('::')[0], source };
}

/** The refusal of an app with a Rust data crate that app.json's `rust.data` does not name (LLP 1153.003 D2). `found`
 * says how the build knows the app has one; `crates` are the crates that may be it, each `{ name, manifest }`. The
 * line to add names the first, or `<crate>` when there is none yet, and keeps the manifest's other `rust` fields (its
 * string form is the mode). */
export function undeclaredDataCrate(found, consequence, crates, manifest) {
  const crate = crates[0] ?? { name: '<crate>' };
  const lib = (() => { try { return Bun.TOML.parse(readFileSync(crate.manifest, 'utf8')).lib?.name; } catch { return undefined; } })() ?? (crates.length ? crate.name.replaceAll('-', '_') : '<library>');
  const mode = typeof manifest.rust === 'string' ? manifest.rust : manifest.rust?.mode;
  const among = crates.length > 1 ? ` (the data crate: one of ${crates.map(c => c.name).join(', ')})` : '';
  return `${found}, and app.json names no rust.data: ${consequence}. Name the data crate and the source it answers with in app.json${among}, as apps/forma-host/app.json does:\n`
    + `  "rust": { ${mode ? `"mode": ${JSON.stringify(mode)}, ` : ''}"data": { "crate": "${crate.name}", "type": "${lib}::<Type>", "source": "${lib}::<Type>::new(…)" } }\n`
    + `"source" is the type itself, its ::default() (when "source" is left out) or its ::new(…) with literal arguments; a source `
    + `built from anything else is a module of its own, named in rust.module.package (LLP 1153.003 D2).`;
}

/** A Rust-data app's host crate (LLP 1153.003 D2): the data crate the manifest names, linked, and the shared bake
 * (`exact_bake::host::app`) as its build script's one line; no exact-js, so no Hermes. `data.dependency` is the data
 * crate's dependency from the crate's directory; `updates` links the Android library's update store, as for app.ts. */
function rustCrateFiles(kind, { name, title, dep, app, data, updates = false }) {
  const target = { web: `[lib]\ncrate-type = ["cdylib", "rlib"]`, apple: '[lib]\ncrate-type = ["rlib"]', android: '[lib]\ncrate-type = ["cdylib"]\ntest = false\nbench = false' }[kind]
    ?? `[[bin]]\nname = "${name}-${kind}"\npath = "src/main.rs"${kind === 'windows' ? '\ntest = false\nbench = false' : ''}`;
  // The web dev loop's resident driver bakes with the source's `default()` (`exact_web::dev::main`).
  const dev = kind === 'web' && data.source === `${data.ty}::default()`;
  const sdk = { web: ['logic', 'host/web', 'host/web-capabilities'], apple: ['vendor/ibex/crates/ibex2-sqlite', 'logic', 'host/apple', 'host/apple-update'],
    linux: ['vendor/ibex/crates/ibex2-sqlite', 'logic', 'host/linux', 'host/painted'], android: ['vendor/ibex/crates/ibex2-sqlite', 'logic', 'host/painted', ...updates ? ['host/painted-update'] : []],
    windows: ['vendor/ibex/crates/ibex2-sqlite', 'logic', 'host/windows', 'host/painted'] }[kind];
  const crateOf = path => ({ 'vendor/ibex/crates/ibex2-sqlite': 'ibex2-sqlite', logic: 'exact-logic', 'host/web-capabilities': 'exact-web-capabilities', 'host/apple-update': 'exact-apple-update' })[path] ?? `exact-${path.split('/').pop()}`;
  const dependencies = [...sdk.map(path => `${crateOf(path)} = ${dep(path)}`), `exact-data-host = ${dep('data/host')}`, `${data.krate} = ${data.dependency}`].join('\n');
  const on = { web: 'the web', apple: 'Apple platforms', linux: 'Linux', android: 'Android', windows: 'Windows' }[kind];
  return {
    'Cargo.toml': `[package]
name = "${name}-${kind}"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false
build = "build.rs"

${target}
${dev ? `\n[[bin]]\nname = "${name}-dev"\npath = "src/bin/${name}-dev.rs"\ntest = false\nbench = false\n` : ''}
${kind === 'android' ? `[target.'cfg(target_os = "android")'.dependencies]` : '[dependencies]'}
${dependencies}

[build-dependencies]
exact-bake = ${dep('bake')}
${data.krate} = ${data.dependency}
`,
    'build.rs': `fn main() {\n    let source = ${data.source};\n    let host_work = exact_bake::hands_out_work!(source);\n    exact_bake::host::app(${JSON.stringify(kind)}, ${JSON.stringify(app)}, source, host_work);\n}\n`,
    [['linux', 'windows'].includes(kind) ? 'src/main.rs' : 'src/lib.rs']: `//! ${title} on ${on}: the host over the Rust data crate (\`${data.ty}\`) and the baked plan.
${kind === 'windows' ? '#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]\n' : ''}
include!(concat!(env!("OUT_DIR"), "/entry.rs"));
`,
    ...(dev ? { [`src/bin/${name}-dev.rs`]: `//! The resident dev driver: \`${name}-dev <source> <out>\`, which the web dev loop runs.

fn main() -> std::process::ExitCode {
    exact_web::dev::main::<${data.ty}>()
}
` } : {}),
  };
}

/** Both generated and authored host crates link the app's declared sources and packages. */
function hostConfiguration(dir, manifest, name, members, generated) {
  const data = rustDataOf(manifest), dataCrate = data && members.find(crate => crate.name === data.krate);
  if (data && !dataCrate) {
    const joins = generated ? ` that joins the app's workspace (package.workspace = "${MEMBER_OF.host}")` : '';
    throw new Error(`app.json's rust.data.crate is ${data.krate}, which is not a crate beside app.json${joins}`);
  }
  return {
    name, title: manifest.app?.name ?? manifest.name ?? titleOf(name),
    data, dataCrate, typescript: existsSync(resolve(dir, 'app.ts')),
    native: nativePackages(dir).map(pkg => pkg.name), preload: Object.keys(manifest.preload ?? {}).length > 0,
    updates: manifest.deploy?.store?.android === 'A',
  };
}

function hostCrateFiles(kind, config, app, dep) {
  const data = config.data && { ...config.data, dependency: dep(config.dataCrate.dir) };
  const files = data && !config.typescript ? rustCrateFiles : crateFiles;
  return files(kind, { ...config, native: config.native.length > 0, data, app, dep });
}

/** A host crate's files, keyed by path under the crate's directory. `dep(path)` is a dependency's path from there and
 * `app` the app directory's, as Rust strings' content; `native` attaches the factory the bake writes for a package
 * that fills the native slot; `updates` links the Android library's update store (`deploy.store.android` "A",
 * LLP 1030.006 D3), which a library at "0" does not carry. */
function crateFiles(kind, { name, title, dep, app, native = false, preload = false, data = null, updates = false }) {
  const header = (target) => `[package]
name = "${name}-${kind}"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false
build = "build.rs"

${target}

[dependencies]
`;
  const bake = (platform) => data ? beside(platform) : `fn main() {
    exact_js_bake::build(std::path::Path::new(${JSON.stringify(app)}), ${JSON.stringify(platform)}).unwrap_or_else(|error| {
        // What the bake found (tsc's and the compiler's diagnostics), as it is: no panic around it.
        eprintln!("{error}");
        std::process::exit(1);
    });
}
`;
  // Beside the data crate (LLP 1153.003 D2): Rust answers what the crate's RUST_SOURCES names, app.ts the rest.
  const beside = (platform) => `fn main() {
${platform === 'apple' ? `    let platform = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("ios" | "tvos" | "watchos") => "ios",
        _ => "macos",
    };
` : ''}    exact_js_bake::build_beside(
        std::path::Path::new(${JSON.stringify(app)}),
        ${platform === 'apple' ? 'platform' : JSON.stringify(platform)},
        &${data.source},
        exact_js_bake::hands_out_work!(${data.source}),
        ${data.lib}::RUST_SOURCES,
        |javascript, typescript| {
            let app = exact_runner::DataSource::app_id(&javascript).to_owned();
            Box::new(
                exact_data::Mixed::new(
                    javascript,
                    exact_data::Placed::new(${data.source}, exact_runner::Placement::Main).named(&app),
                    typescript,
                    ${data.lib}::RUST_SOURCES,
                )
                .expect("app.ts and the data crate answer different sources, for one app"),
            )
        },
    )
    .unwrap_or_else(|error| {
        // What the bake found (tsc's and the compiler's diagnostics), as it is: no panic around it.
        eprintln!("{error}");
        std::process::exit(1);
    });
}
`;
  // The TypeScript module and the data crate's source as one, in host storage: the composition the bake made. A
  // data crate that names no app takes the module's identity, which is app.json's.
  const mixed = (web, javascript) => `type ExactEmbeddedData = exact_data_host::Storage<
    exact_data::Mixed<${web ? 'exact_js_web::Module' : 'exact_js::Placed<exact_js::Module>'}, exact_data::Placed<${data.ty}>>,
>;
fn embedded_data() -> ExactEmbeddedData {
    let javascript =
${javascript.replace(/^/gm, '    ').trimEnd()};
    let app = exact_runner::DataSource::app_id(&javascript).to_owned();
    exact_data_host::Storage::new(
        exact_data::Mixed::new(
            javascript,
            exact_data::Placed::new(${data.source}, RUST_PLACEMENT).named(&app),
            TYPESCRIPT_SOURCES,
            ${data.lib}::RUST_SOURCES,
        )
        .expect("app.ts and the data crate answer different sources, for one app")
        .with_embedded_rust(|placed| {
            let app = exact_runner::DataSource::app_id(placed);
            Ok(exact_data::Placed::new(${data.source}, placed.given_placement()).named(app))
        }),
    )
}
`;
  const module = (web) => data ? mixed(web, web
    ? `    exact_js_web::Module::new(APP, GRANTS, REVISION)
        .with_canvas_surfaces(CANVAS_SURFACES)
        .placed(TYPESCRIPT_PLACEMENT)`
    : `    exact_js::Module::new(APP_BYTECODE, APP, GRANTS)
${preload ? '        .with_preloads(PRELOAD)\n' : ''}        .with_canvas_surfaces(CANVAS_SURFACES)
${native ? '        .with_native(exact_native)\n' : ''}        .placed(TYPESCRIPT_PLACEMENT)`) : web
    ? `type ExactEmbeddedData = exact_js_web::Module;
fn embedded_data() -> ExactEmbeddedData {
    exact_js_web::Module::new(APP, GRANTS, REVISION)
        .with_canvas_surfaces(CANVAS_SURFACES)
        .placed(TYPESCRIPT_PLACEMENT)
}
`
    : `type ExactEmbeddedData = exact_js::Placed<exact_js::Module>;
fn embedded_data() -> ExactEmbeddedData {
    exact_js::Module::new(APP_BYTECODE, APP, GRANTS)
${preload ? '        .with_preloads(PRELOAD)\n' : ''}        .with_canvas_surfaces(CANVAS_SURFACES)
${native ? '        .with_native(exact_native)\n' : ''}        .placed(TYPESCRIPT_PLACEMENT)
}
`;
  const includes = (plan) => `include!(concat!(env!("OUT_DIR"), "/module.rs"));
${plan ? `const PLAN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.plan"));
const COMPAT: &str = include_str!(concat!(env!("OUT_DIR"), "/compat.json"));
` : ''}`;
  const build = `[build-dependencies]
exact-js-bake = ${dep('js/bake')}
${data ? `exact-data = ${dep('data')}\nexact-runner = ${dep('runner')}\n${data.krate} = ${data.dependency}\n` : ''}`;
  // What the composition links beside the TypeScript module.
  const linked = data ? `exact-data = ${dep('data')}\nexact-data-host = ${dep('data/host')}\nexact-runner = ${dep('runner')}\n${data.krate} = ${data.dependency}\n` : '';
  if (kind === 'web') return {
    'Cargo.toml': `${header('[lib]\ncrate-type = ["cdylib", "rlib"]')}exact-logic = ${dep('logic')}
exact-web = ${dep('host/web')}
exact-web-capabilities = ${dep('host/web-capabilities')}
exact-js-web = ${dep('js/web')}
${linked}
${build}`,
    'build.rs': bake('web'),
    'src/lib.rs': `//! ${title} on the web: the Contract UI and the TypeScript data module.

${includes(true)}
${module(true)}include!(concat!(env!("OUT_DIR"), "/logic.rs"));
exact_web::host!(
    AppData,
    PLAN,
    COMPAT,
    app_data,
    [
        include_bytes!(concat!(env!("OUT_DIR"), "/app.module.json")) as &[u8],
        include_bytes!(concat!(env!("OUT_DIR"), "/app.js")) as &[u8]
    ]
);
`,
  };
  if (kind === 'apple') return {
    'Cargo.toml': `${header('[lib]\ncrate-type = ["rlib"]')}ibex2-sqlite = ${dep('vendor/ibex/crates/ibex2-sqlite')}
exact-logic = ${dep('logic')}
exact-apple = ${dep('host/apple')}
exact-apple-update = ${dep('host/apple-update')}
exact-js = ${dep('js')}
${linked}
${build}`,
    'build.rs': data ? beside('apple') : appleBuildScript(app),
    // Names no app: a change to only its identity leaves the compiled archive fresh (linked-plan.mjs compiledBakeDir).
    'src/lib.rs': `//! The app on Apple: the Contract UI and the TypeScript data module.

${includes(false)}
${module(false)}include!(concat!(env!("OUT_DIR"), "/logic.rs"));
include!(concat!(env!("OUT_DIR"), "/linked.rs"));
// The host app.json's deploy.store names (with an update store, signed updates), and the plan it boots.
include!(concat!(env!("OUT_DIR"), "/host.rs"));
`,
  };
  if (kind === 'linux') return {
    'Cargo.toml': `${header(`[[bin]]\nname = "${name}-linux"\npath = "src/main.rs"`)}ibex2-sqlite = ${dep('vendor/ibex/crates/ibex2-sqlite')}
exact-logic = ${dep('logic')}
exact-linux = ${dep('host/linux')}
exact-painted = ${dep('host/painted')}
exact-js = ${dep('js')}
${linked}
${build}`,
    'build.rs': bake('linux'),
    'src/main.rs': `//! ${title} on Linux (and Android): the Contract UI and the TypeScript data module.

${includes(true)}
${module(false)}include!(concat!(env!("OUT_DIR"), "/logic.rs"));
// The packages' Linux implementations the build composes (exact_packages()), and the app's Rust hatches
// (exact_hatches(), from modules/linux).
include!(concat!(env!("OUT_DIR"), "/packages.rs"));
include!(concat!(env!("OUT_DIR"), "/hatches.rs"));
fn main() {
    let composed = exact_painted::Composed { hatches: exact_hatches(), packages: exact_packages() };
    std::process::exit(exact_linux::run_composed::<AppData>(PLAN, COMPAT, composed));
}
`,
  };
  // The library the Canvas reader loads (LLP 1107, host/android/README.md), in Shelf's shape: empty off Android.
  if (kind === 'android') return {
    'Cargo.toml': `${header('[lib]\ncrate-type = ["cdylib"]\ntest = false\nbench = false').replace(/\n\[dependencies\]\n$/, '')}
[target.'cfg(target_os = "android")'.dependencies]
ibex2-sqlite = ${dep('vendor/ibex/crates/ibex2-sqlite')}
exact-logic = ${dep('logic')}
exact-painted = ${dep('host/painted')}
${updates ? `exact-painted-update = ${dep('host/painted-update')}\n` : ''}exact-js = ${dep('js')}
${linked}
${build}`,
    'build.rs': data ? beside('android') : `fn main() {
    exact_js_bake::build_android(std::path::Path::new(${JSON.stringify(app)})).unwrap_or_else(|error| {
        // What the bake found (tsc's and the compiler's diagnostics), as it is: no panic around it.
        eprintln!("{error}");
        std::process::exit(1);
    });
}
`,
    'src/lib.rs': `//! ${title} on Android: the Contract UI and the TypeScript data module, in the library
//! exact2's Canvas reader loads (\`bun exact.mjs android\` builds the APK).

#[cfg(target_os = "android")]
mod android {
    #[allow(dead_code)]
    mod baked {
        include!(concat!(env!("OUT_DIR"), "/module.rs"));
    }
    use baked::*;

    ${module(false).replace(/\n(?=.)/g, '\n    ')}    include!(concat!(env!("OUT_DIR"), "/logic.rs"));
    include!(concat!(env!("OUT_DIR"), "/hatches.rs"));
    // The home-screen widgets' JNI, when app.json declares any (LLP 1126.003 D9).
    include!(concat!(env!("OUT_DIR"), "/widgets.rs"));
    exact_widgets!(app_data());

    exact_painted::canvas_jni!(super::AppData, hatches = crate::android::exact_hatches()${updates ? ', updates = exact_painted_update::open' : ''});
}
`,
  };
  if (kind === 'windows') return {
    'Cargo.toml': `${header(`[[bin]]\nname = "${name}-windows"\npath = "src/main.rs"\ntest = false\nbench = false`)}ibex2-sqlite = ${dep('vendor/ibex/crates/ibex2-sqlite')}
exact-logic = ${dep('logic')}
exact-windows = ${dep('host/windows')}
# The packages' Windows implementations name \`exact_painted::package\` (LLP 1133.022 D10 item 5).
exact-painted = ${dep('host/painted')}
exact-js = ${dep('js')}
${linked}
${build}`,
    'build.rs': bake('windows'),
    // A GUI executable in every profile, as Excord's: opened from Explorer or the desktop, a development build shows
    // no console window beside its own; a parent's handles still carry its stderr.
    'src/main.rs': `//! ${title} on Windows: the host's window over the app's TypeScript data module and its baked plan.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

${includes(true)}
${module(false)}include!(concat!(env!("OUT_DIR"), "/logic.rs"));
// The packages' Windows implementations the build composes (exact_packages()), and the app's Rust hatches
// (exact_hatches(), from modules/windows or else modules/linux).
include!(concat!(env!("OUT_DIR"), "/packages.rs"));
include!(concat!(env!("OUT_DIR"), "/hatches.rs"));
fn main() {
    let composed = exact_painted::Composed { hatches: exact_hatches(), packages: exact_packages() };
    std::process::exit(exact_windows::run_baked::<AppData, ADMIT_AGENT>(${JSON.stringify(title)}, window(), PLAN, COMPAT, composed, SERVICES));
}
`,
  };
  throw new Error(`no host crate of kind ${kind}`);
}

export function authoredCrates(dir, manifest, name, kinds) {
  const config = hostConfiguration(dir, manifest, name, appCrates(dir), false);
  return Object.fromEntries(kinds.map(kind => {
    const dep = path => `{ path = ${JSON.stringify(pathFrom(resolve(dir, kind), resolve(ROOT, path)))} }`;
    return [kind, hostCrateFiles(kind, config, '..', dep)];
  }));
}

/** For each host crate, its build script as `.exact/shells/` writes it and as it reads beside app.json: a crate copied up
 * (LLP 1153 D1.4) whose script is still the first gets the second from `update` (game/new.mjs). */
export const copiedBuildScripts = () => Object.fromEntries(Object.values(PLATFORMS).concat('web').filter((kind, i, all) => all.indexOf(kind) === i)
  .map(kind => [kind, ['../../..', '..'].map(app => crateFiles(kind, { name: 'app', title: 'App', app, dep: () => '' })['build.rs'])]));

/** The Canvas entry a generated TypeScript crate can bring to an authored workspace. */
export const copiedAndroidEntries = () => [false, true].flatMap(updates => [false, true].flatMap(preload =>
  [false, true].map(native => crateFiles('android', {title: 'App', dep: () => '', updates, preload, native})['src/lib.rs'])));

const titleOf = name => name.split('-').map(w => w[0].toUpperCase() + w.slice(1)).join(' ');
const stampOf = (shells) => { try { return JSON.parse(readFileSync(resolve(shells, '.stamp.json'), 'utf8')); } catch { return null; } };

/** The files under `dir`, by path relative to it, without Cargo's `target/`. */
const listFiles = (dir, at = '') => readdirSync(resolve(dir, at), { withFileTypes: true }).flatMap(entry =>
  entry.isDirectory() ? (entry.name === 'target' ? [] : listFiles(dir, `${at}${entry.name}/`)) : [`${at}${entry.name}`]);

/** Write a file only when its bytes differ, so Cargo's times stay. */
function writeChanged(path, text) {
  if (existsSync(path) && readFileSync(path, 'utf8') === text) return false;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
  return true;
}

/** The refusal of a crate whose `package` (`declared`) does not name `workspace`, so the app owns `owns`, or null.
 * Cargo requires a member outside the workspace root to name the root
 * (https://doc.rust-lang.org/cargo/reference/manifest.html#the-workspace-field); a game's crates name `.shells/`, an
 * app's the generated workspaces (LLP 1153.003 D1.1). */
export function memberRefusal(manifest, declared, workspace, owns) {
  return declared?.workspace?.replace(/\/+$/, '') === workspace ? null : `${manifest}: set package.workspace = "${workspace}" so the app owns ${owns}`;
}

/** The app's own crates (LLP 1153.003 D1.1), as `appCrates` lists them, by the generated workspace each joins:
 * `{ host, gpu }`. A crate that joins neither is refused with the line to add, and one that takes the name of a
 * crate exact2 generates for the app with the escape for an app that writes its host crates (D3). */
export function appMembers(dir, name) {
  const crates = appCrates(dir), loose = crates.find(crate => !crate.joins);
  if (loose) throw new Error(notAMember(loose));
  const generated = new Set(['web', 'module', ...Object.values(PLATFORMS)].map(kind => `${name}-${kind}`));
  const taken = crates.find(crate => generated.has(crate.name));
  if (taken) throw new Error(`${taken.manifest}: ${taken.name} is a host crate's name, and exact2 writes the app's host crates under .exact/shells/. Rename the crate, or, for an app that writes its own host crates, write their workspace (docs/reference.md, "An authored workspace").`);
  // A GPU module links nothing of the host's, so it resolves apart from it (D5.1).
  const gpu = crates.find(crate => crate.joins === 'host' && (crate.name === `${name}-gpu` || crate.name?.startsWith(`${name}-gpu-`)));
  if (gpu) throw new Error(`${gpu.manifest}: ${gpu.name} is a GPU module, which resolves apart from the host's crates: set package.workspace = "${MEMBER_OF.gpu}"`);
  return { host: crates.filter(crate => crate.joins === 'host'), gpu: crates.filter(crate => crate.joins === 'gpu') };
}

/** Generated beside app.json for an app with crates of its own (LLP 1153.003 D1.5): rustup and Cargo find these by
 * searching up from the working directory, so `cargo test` in `data/` uses exact2's compiler and the app's `target/`.
 * Each is rewritten only while it is still the generated one (its first line), and listed in the app's .gitignore. */
const BESIDE = '# Generated by exact2 for the app\'s own crates (LLP 1153.003 D1.5); rewritten with .exact/shells/, never edited.\n';
function besideApp(dir, wanted) {
  const files = { 'rust-toolchain.toml': () => BESIDE + readFileSync(resolve(ROOT, 'rust-toolchain.toml'), 'utf8'),
    '.cargo/config.toml': () => `${BESIDE}${readFileSync(resolve(ROOT, '.cargo/config.toml'), 'utf8').trimEnd()}\n\n[build]\ntarget-dir = "target"\n` };
  for (const [file, text] of Object.entries(files)) {
    const path = resolve(dir, file), generated = !existsSync(path) || readFileSync(path, 'utf8').startsWith(BESIDE);
    if (!generated) {
      if (wanted) throw new Error(`${path}: the app's crates build with exact2's ${file === 'rust-toolchain.toml' ? 'toolchain' : 'Cargo configuration'}, which exact2 writes there; move this one aside (LLP 1153.003 D1.5)`);
      continue;
    }
    if (wanted) writeChanged(path, text());
    else rmSync(path, { force: true });
  }
  if (!wanted) return;
  const ignore = resolve(dir, '.gitignore'), had = existsSync(ignore) ? readFileSync(ignore, 'utf8') : '';
  const missing = Object.keys(files).map(file => `/${file}`).filter(line => !had.split('\n').includes(line));
  if (missing.length) writeFileSync(ignore, `${had}${had && !had.endsWith('\n') ? '\n' : ''}${missing.join('\n')}\n`);
}

let exact2CrateDirs;
/** The checkout's crates by name, `name` -> directory: its workspace's packages and the SQLite provider it vendors. */
export function exact2Crates() {
  if (exact2CrateDirs) return exact2CrateDirs;
  const listed = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: ROOT, env: cargoEnvironment(), encoding: 'utf8', maxBuffer: 1 << 26 });
  if (listed.status !== 0) throw new Error(`cargo metadata in ${ROOT}:\n${listed.stderr || listed.error?.message}`);
  exact2CrateDirs = new Map(JSON.parse(listed.stdout).packages.map(p => [p.name, dirname(p.manifest_path)]));
  return exact2CrateDirs.set('ibex2-sqlite', resolve(ROOT, 'vendor/ibex/crates/ibex2-sqlite'));
}

/** What the members `manifests` of a generated workspace take from exact2 by `<name>.workspace = true` beyond
 * `offered`: `{ stamp, entries() }`, each entry `{ name, dir, defaults }` for its `[workspace.dependencies]`
 * (`defaults` false when a member turns them off, which Cargo reads only from the workspace's entry). A member that
 * names a crate of exact2 through the app's link is refused: the workspace names the checkout by its real path, and a
 * second spelling of it is a second package (`cargoCheckout`, scripts/paths.mjs). */
export function takenFromExact2(manifests, offered) {
  const refused = checkoutPathRefusal(manifests);
  if (refused) throw new Error(refused);
  const names = new Set(), off = new Set();
  for (const manifest of manifests) {
    for (const [, table] of dependencyTables(Bun.TOML.parse(readFileSync(manifest, 'utf8')))) {
      for (const [name, dep] of Object.entries(table)) {
        if (dep?.workspace !== true || offered.includes(name)) continue;
        names.add(name);
        if (dep['default-features'] === false || dep.default_features === false) off.add(name);
      }
    }
  }
  const sorted = [...names].sort();
  // A name exact2 has no crate of is the member's own mistake, which Cargo names.
  const entries = () => sorted.filter(name => exact2Crates().has(name)).map(name => ({ name, dir: exact2Crates().get(name), defaults: !off.has(name) }));
  return { stamp: sorted.map(name => off.has(name) ? `${name} without defaults` : name), entries };
}
/** A `[workspace.dependencies]` line for `takenFromExact2`'s entry, its path relative to the workspace at `from`. */
const takenLine = (from, { name, dir, defaults }) => `${name} = { path = ${JSON.stringify(pathWithin(from, dir))}${defaults ? '' : ', default-features = false'} }`;

/** The generated workspace for the app at `dir`, current with its inputs: the manifest's platforms and name, the app's
 * own crates, the checkout's path and the files the workspace copies from it, and this module. Returns the workspace
 * directory and whether anything was written. The crates are `<name>-<kind>`, with the prefix `appCrateName` reads.
 * Its lock is the app's captured `Cargo.lock` when it has one (LLP 1153.003 D1.4), copied in; otherwise exact2's,
 * re-resolved offline for these members, and one this machine's cache cannot resolve is left as the copy for the
 * build, which `settleLock` resolves. */
export function ensureShells(dir, manifest = readManifest(dir), name = null) {
  dir = realpathSync(dir);
  name = appCrateName(dir, name ?? basename(dir));
  const shells = shellsDir(dir), crates = shellCrates(manifest), members = appMembers(dir, name);
  const config = hostConfiguration(dir, manifest, name, members.host, true);
  const { title, native, data, dataCrate, typescript, preload, updates } = config;
  const taken = takenFromExact2(members.host.map(member => member.manifest), ['exact-runner', 'contract']), checkout = cargoCheckout(dir, true);
  // The paths written are relative (`pathWithin`), which reads real directories.
  for (const kind of crates) mkdirSync(resolve(shells, kind), { recursive: true });
  const copied = ['rust-toolchain.toml', '.cargo/config.toml'].map(file => [file, readFileSync(resolve(ROOT, file), 'utf8')]);
  const patches = patchLines(shells, pathWithin);
  const own = members.host.map(member => pathWithin(shells, member.dir));
  // A crate the host links is the data crate or a helper of it (D1.1). Without rust.data no host crate links one, so
  // unless a module of the app's own answers (rust.module.package), nothing answers the app's requests.
  if (!data && !manifest.rust?.module?.package && members.host.length) {
    const named = members.host.map(member => member.name).join(', ');
    throw new Error(undeclaredDataCrate(`${dir}: ${named} beside app.json ${members.host.length > 1 ? 'join' : 'joins'} the app's generated workspace`,
      'its host crates link a crate of the app\'s only as the data crate rust.data names, so they would answer none of the app\'s requests', members.host, manifest));
  }
  // The inputs whose change rewrites the workspace: the app's path, the checkout's and its commit, the templates (this
  // module), the files copied from the checkout, its patch table and its lock (a dependency the SDK moved), the
  // manifest's platforms, the app's name, its own crates and the package that fills the native slot.
  const commit = spawnSync('git', ['-C', ROOT, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).stdout?.trim() ?? '';
  const stamp = {
    exact2: ROOT, app: dir, commit, name, title, crates, native, members: own, taken: taken.stamp, data, typescript,
    ...(preload ? { preload } : {}),
    ...(updates ? { updates } : {}),
    inputs: createHash('sha256').update([readFileSync(fileURLToPath(import.meta.url), 'utf8'), APPLE_BUILD_SCRIPT, ...copied.map(([, text]) => text), ...patches, readFileSync(resolve(ROOT, 'Cargo.lock'), 'utf8')].join('\0')).digest('hex'),
  };
  besideApp(dir, members.host.length + members.gpu.length > 0);
  const captured = resolve(dir, 'Cargo.lock'), before = stampOf(shells);
  const expected = ['Cargo.toml', 'Cargo.lock', ...copied.map(([file]) => file), ...(data ? ['module/Cargo.toml', 'module/src/lib.rs'] : []), ...crates.flatMap(kind => Object.keys(hostCrateFiles(kind, config, '', () => '')).map(file => `${kind}/${file}`))];
  const complete = expected.every(file => existsSync(resolve(shells, file)));
  if (before && JSON.stringify(before) === JSON.stringify(stamp) && complete) {
    // The captured lock is the workspace's whenever it changed (`bun exact.mjs lock`, a pull).
    if (existsSync(captured)) writeChanged(resolve(shells, 'Cargo.lock'), readFileSync(captured, 'utf8'));
    return { workspace: shells, wrote: false, crates };
  }
  const dependency = path => `{ path = ${JSON.stringify(pathWithin(shells, resolve(checkout, path)))} }`;
  let wrote = writeChanged(resolve(shells, 'Cargo.toml'), `# ${title}'s native workspace, generated by exact2 from app.json and the app's own crates (LLP 1153 D1,
# LLP 1153.003 D1): never edited, rewritten when the manifest's platforms, the app's crates or the checkout change. A
# crate beside app.json joins it with package.workspace = "${MEMBER_OF.host}".
[workspace]
members = [${[...crates, ...(data ? ['module'] : []), ...own].map(c => JSON.stringify(c)).join(', ')}]
resolver = "2"

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"

# What the app's own crates may take from exact2 (\`exact-runner.workspace = true\`).
[workspace.dependencies]
exact-runner = ${dependency('runner')}
contract = ${dependency('contract/cli')}
${taken.entries().map(entry => takenLine(shells, entry) + '\n').join('')}
[patch.crates-io]
${patches.join('\n')}
`);
  for (const [file, text] of copied) wrote = writeChanged(resolve(shells, file), text) || wrote;
  for (const kind of crates) {
    const files = hostCrateFiles(kind, config, '../../..', path => `{ path = ${JSON.stringify(pathWithin(resolve(shells, kind), resolve(checkout, path)))} }`);
    for (const [file, text] of Object.entries(files)) wrote = writeChanged(resolve(shells, kind, file), text) || wrote;
    // A file an earlier shape wrote (app.ts added or removed, the dev driver) goes.
    const keep = new Set(Object.keys(files));
    for (const file of listFiles(resolve(shells, kind))) if (!keep.has(file)) { rmSync(resolve(shells, kind, file)); wrote = true; }
  }
  // The web's Rust module (LLP 1153.003 D2): the source app.json's rust.data names, exported for the JS runtime.
  // host/web-js/module.mjs writes its src/lib.rs, which exports the Canvas 2D draw only for a plan that draws.
  const module = resolve(shells, 'module');
  if (data) {
    mkdirSync(module, { recursive: true });
    wrote = writeChanged(resolve(module, 'Cargo.toml'), `[package]
name = "${name}-module"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[lib]
crate-type = ["cdylib"]
test = false
bench = false

[dependencies]
${data.krate} = { path = ${JSON.stringify(pathWithin(module, dataCrate.dir))} }
exact-logic-abi = { path = ${JSON.stringify(pathWithin(module, resolve(checkout, 'logic/abi')))} }
`) || wrote;
    if (!existsSync(resolve(module, 'src/lib.rs'))) wrote = writeChanged(resolve(module, 'src/lib.rs'), `exact_logic_abi::export!(${data.ty}, ${data.source});\n`) || wrote;
  } else if (existsSync(module)) { rmSync(module, { recursive: true, force: true }); wrote = true; }
  // A crate for a platform no longer named goes with its platform.
  for (const entry of readdirSync(shells, { withFileTypes: true })) {
    if (entry.isDirectory() && Object.values(PLATFORMS).concat('web').includes(entry.name) && !crates.includes(entry.name)) { rmSync(resolve(shells, entry.name), { recursive: true, force: true }); wrote = true; }
  }
  // The lock follows every change. A captured one is the workspace's and is never replaced by exact2's (D1.4).
  if (existsSync(captured)) writeChanged(resolve(shells, 'Cargo.lock'), readFileSync(captured, 'utf8'));
  else {
    writeFileSync(resolve(shells, 'Cargo.lock'), readFileSync(resolve(ROOT, 'Cargo.lock')));
    const lock = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], { cwd: shells, env: cargoEnvironment(), stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' });
    if (lock.error?.code === 'ENOENT') throw new Error('cargo was not found on PATH or in ~/.cargo/bin; install Rust with rustup (https://rustup.rs)');
    // Cargo may have rewritten the lock before failing to download; `settleLock` recognises exact2's bytes.
    if (lock.status !== 0) writeFileSync(resolve(shells, 'Cargo.lock'), readFileSync(resolve(ROOT, 'Cargo.lock')));
  }
  writeFileSync(resolve(shells, '.stamp.json'), JSON.stringify(stamp) + '\n');
  return { workspace: shells, wrote: true, crates };
}

/** The lock an app captures for its GPU crates (LLP 1153.003 D5.1), beside the host's `Cargo.lock`. */
export const GPU_LOCK = 'Cargo.gpu.lock';

/** The generated workspace of the app's GPU crates (LLP 1153.003 D5.1), `.exact/gpu/`, current with them: a GPU module
 * links nothing of the host's, so its dependencies resolve without the host's graph. It carries exact2's
 * `[patch.crates-io]` (exact-gpu's build script refuses a graph without the wgpu-hal patch) and, as workspace
 * dependencies, `exact-gpu` and the web dependencies `module!` exports through at the versions exact2 pins (the
 * wasm-bindgen CLI the web build runs refuses any other). Its lock follows the host's rule: the app's captured
 * `Cargo.gpu.lock` when it has one, else exact2's, which `settleLock` resolves for these members. Returns the
 * workspace, or null when the app has no GPU crate (and removes a workspace written before). */
export function ensureGpuShells(dir, name = null) {
  dir = realpathSync(dir);
  name = appCrateName(dir, name ?? basename(dir));
  const gpu = gpuShellsDir(dir), { gpu: members } = appMembers(dir, name);
  if (!members.length) {
    if (existsSync(gpu)) rmSync(gpu, { recursive: true, force: true });
    return null;
  }
  const taken = takenFromExact2(members.map(member => member.manifest), ['exact-gpu', 'wasm-bindgen', 'wasm-bindgen-futures', 'web-sys']);
  mkdirSync(resolve(gpu, '.cargo'), { recursive: true });
  const pinned = Bun.TOML.parse(readFileSync(resolve(ROOT, 'game/Cargo.toml'), 'utf8')).workspace.dependencies;
  const toml = value => typeof value === 'string' ? JSON.stringify(value)
    : `{ ${Object.entries(value).map(([key, v]) => `${key} = ${Array.isArray(v) ? `[${v.map(x => JSON.stringify(x)).join(', ')}]` : JSON.stringify(v)}`).join(', ')} }`;
  writeChanged(resolve(gpu, 'Cargo.toml'), `# The app's GPU modules' workspace, generated by exact2 from the crates beside app.json that name it (LLP 1153.003
# D5.1): never edited, rewritten when they change. A GPU crate joins it with package.workspace = "${MEMBER_OF.gpu}".
[workspace]
members = [${members.map(member => JSON.stringify(pathWithin(gpu, member.dir))).join(', ')}]
resolver = "2"

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"

# What a GPU crate takes from exact2 (\`exact-gpu.workspace = true\`), and the web dependencies at exact2's pins.
[workspace.dependencies]
exact-gpu = { path = ${JSON.stringify(pathWithin(gpu, resolve(cargoCheckout(dir, true), 'gpu')))} }
${['wasm-bindgen', 'wasm-bindgen-futures', 'web-sys'].map(dep => `${dep} = ${toml(pinned[dep])}`).join('\n')}
${taken.entries().map(entry => takenLine(gpu, entry) + '\n').join('')}
[patch.crates-io]
${patchLines(gpu, pathWithin).join('\n')}
`);
  for (const file of ['rust-toolchain.toml', '.cargo/config.toml']) writeChanged(resolve(gpu, file), readFileSync(resolve(ROOT, file), 'utf8'));
  const captured = resolve(dir, GPU_LOCK), lock = resolve(gpu, 'Cargo.lock');
  if (existsSync(captured)) writeChanged(lock, readFileSync(captured, 'utf8'));
  else if (!existsSync(lock)) writeFileSync(lock, readFileSync(resolve(ROOT, 'Cargo.lock')));
  return gpu;
}

/** Each package a lock holds from a registry or git that `sdk` (exact2's lock) does not hold at that version and
 * checksum: what an app's own crates added (LLP 1153.003 D1.4). Path packages are exact2's or the app's own. */
export function outsideExact2(lock, sdk) {
  const paths = new Set((Bun.TOML.parse(lock).package ?? []).filter(pkg => !pkg.source).map(pkg => pkg.name));
  return outsideSdkLock(lock, sdk, paths);
}
const listed = (items, most = 6) => items.length > most ? `${items.slice(0, most).join(', ')}, … (${items.length} in all)` : items.join(', ');

/** Writes the app's generated workspaces and settles their locks (LLP 1153.003 D1.4, D5.1); returns the GPU
 * workspace, or null for an app without GPU crates. */
export function settleShells(dir, manifest, name) {
  settleLock(dir, ensureShells(dir, manifest, name).workspace);
  const gpu = ensureGpuShells(dir, name);
  if (gpu) settleLock(dir, gpu, resolve(dir, GPU_LOCK));
  return gpu;
}

/** Whether `gpu/` beside app.json joins the generated GPU workspace; one there not named `crate` is refused. */
export function joinsGpu(dir, crate) {
  const member = appCrates(dir).find(c => c.dir === resolve(dir, 'gpu'));
  if (member?.joins !== 'gpu') return false;
  if (member.name !== crate) throw new Error(`${member.manifest} names the package ${member.name ?? 'nothing'}; the app's GPU module must be named ${crate}`);
  return true;
}

/** `app` as its GPU crates build: in their own workspace when the app's is generated (LLP 1153.003 D5.1). */
export const gpuView = (app, module = null) => {
  const at = module?.package ? ensurePackageGpu(app, module) : app.gpuWorkspace;
  return at && (module?.package || at !== app.workspace) ? Object.create(app, { workspace: { value: at } }) : app;
};

/** Cargo's workspace inputs and the checkout's injected profiles are absent from rustc's dep-info. */
export function gpuWorkspaceInputs(app) {
  const result = lockedMetadata(app.workspace);
  if (result.status !== 0) throw new Error(`GPU workspace ${app.workspace}: ${result.stderr || result.error?.message}`);
  const metadata = JSON.parse(result.stdout);
  return [resolve(ROOT, 'Cargo.toml'),
    ...['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml'].map(file => resolve(app.workspace, file)).filter(existsSync),
    ...metadata.packages.filter(p => p.source === null).map(p => p.manifest_path)];
}

const packageModules = Symbol('package GPU modules'), packageDeclarations = Symbol('package exact.json files');
/** What package GPU declarations make every host's build read, whichever hosts they name: each direct dependency's main
 * exact.json that `packageGpuManifest` read, which says whether and for which hosts it declares GPU surfaces, and each
 * package GPU crate's lock. */
export const packageGpuInputs = manifest => [...manifest[packageDeclarations] ?? [],
  ...(manifest[packageModules] ?? []).map(m => resolve(m.root, m.crate, 'Cargo.lock'))];
/** The hosts that draw GPU surfaces (LLP 1153.003 §3): what a package's `targets` may name, and its default. The
 * watch draws none, so no GPU module is built for it. */
export const GPU_PLATFORMS = ['web', 'macos', 'ios', 'tvos', 'android', 'linux', 'windows'];
export function gpuModules(manifest, platform = null) {
  if (platform && !GPU_PLATFORMS.includes(platform)) return [];
  return [...Object.entries(manifest.gpu?.modules ?? {}).map(([name, surfaces]) => ({ name, surfaces })),
    ...(manifest[packageModules] ?? [])].filter(m => !platform || !m.targets || m.targets.includes(platform));
}
/** The surfaces of package modules whose `targets` omit `platform`: no artifact draws them there (LLP 1153.003 D5.8). */
export const unregisteredSurfaces = (manifest, platform) =>
  (manifest[packageModules] ?? []).filter(m => m.targets && !m.targets.includes(platform)).flatMap(m => m.surfaces);
/** `gpuModules` with package declarations read now, not when `app` was resolved: a long-lived process (a dev loop)
 * otherwise acts on the declarations it started with. */
export const currentGpuModules = (app, platform = null) => gpuModules(packageGpuManifest({ gpu: app.manifest.gpu }, app.dir), platform);
/** The stock web bake reads the Contract directly. An authored build script must run through Cargo. */
export function authoredWebBake(dir) {
  const script = resolve(dir, 'build.rs');
  return existsSync(script) && !/^\s*fn main\(\)\s*\{\s*exact_js_bake::build\w*\(/m.test(readFileSync(script, 'utf8'));
}

export const webRendererHost = (dir, binary) => ['linux', 'web'].find(host => existsSync(resolve(dir, host, 'src/bin', binary + '.rs')));

/** Whether a normal web build compiles anything belonging to the app as well as the SDK tools. */
export function webCompilesRust(app) {
  return !!(app.manifest.game || rustDataOf(app.manifest) || app.manifest.rust?.module?.package ||
    existsSync(resolve(app.dir, 'gpu/Cargo.toml')) || currentGpuModules(app, 'web').length ||
    authoredWebBake(resolve(app.dir, 'web')) || webRendererHost(app.dir, app.name + '-render'));
}

/** Where `ensurePackageGpu` generates a package module's workspace. */
export const packageGpuDir = (app, module) => resolve(app.dir, '.exact/package-gpu', module.name);

/** As package_views.rs does, read only direct dependencies whose main export is an exact.json. */
export function packageGpuManifest(manifest, dir) {
  const modules = [], declarations = [], owners = new Map(), slugs = new Map();
  for (const [name, surfaces] of Object.entries(manifest.gpu?.modules ?? {})) {
    slugs.set(name, `gpu.modules.${name}`);
    for (const surface of surfaces) owners.set(surface, `gpu.modules.${name}`);
  }
  for (const { name: pkg, root } of directDependencies(dir)) {
    const exports = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')).exports;
    const main = typeof exports === 'string' ? exports : exports?.['.'];
    const file = typeof main === 'string' ? main : main?.contract ?? main?.default;
    if (typeof file !== 'string' || !file.endsWith('exact.json')) continue;
    const declaration = resolve(root, file), gpu = JSON.parse(readFileSync(declaration, 'utf8')).gpu;
    declarations.push(declaration);
    if (gpu === undefined) continue;
    const fail = why => { throw new Error(`${pkg}: exact.json gpu ${why}`); };
    if (!gpu || typeof gpu !== 'object') fail('must be an object');
    if (typeof gpu.crate !== 'string' || !gpu.crate.split(/[\\/]/).every(p => p && p !== '.' && p !== '..') || gpu.crate.startsWith('/')) fail('crate must be a directory inside the package');
    if (typeof gpu.exactGpu !== 'string' || !gpu.exactGpu) fail('requires an exactGpu version requirement');
    const name = pkg.replace(/^@/, '').replace(/[\/_.]/g, '-');
    if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name)) fail('package name must form a lowercase module slug');
    if (slugs.has(name)) throw new Error(`GPU module ${name} is claimed by both ${slugs.get(name)} and ${pkg}`);
    slugs.set(name, pkg);
    if (!gpu.surfaces || Array.isArray(gpu.surfaces) || typeof gpu.surfaces !== 'object' || !Object.keys(gpu.surfaces).length) fail('surfaces must declare at least one surface');
    for (const [surface, spec] of Object.entries(gpu.surfaces)) {
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(surface)) fail('surface names must be identifiers');
      if (!Number.isInteger(spec?.arity) || spec.arity < 0 || spec.arity > 0xffffffff) fail('each surface needs a nonnegative integer arity');
      if (typeof spec.factory !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)+$/.test(spec.factory)) fail('each surface needs a Rust factory path, crate::function');
      if (owners.has(surface)) throw new Error(`GPU surface ${surface} is claimed by both ${owners.get(surface)} and ${pkg}`);
      owners.set(surface, pkg);
    }
    if (gpu.targets !== undefined && (!Array.isArray(gpu.targets) || gpu.targets.some(t => !GPU_PLATFORMS.includes(t)))) fail(`targets may name only ${GPU_PLATFORMS.join(', ')}`);
    modules.push({ name, surfaces: Object.keys(gpu.surfaces).sort(), package: pkg, root: realpathSync(root), declaration,
      crate: gpu.crate, exactGpu: gpu.exactGpu, declarations: gpu.surfaces, targets: gpu.targets, limits: gpu.limits, features: gpu.features });
  }
  manifest[packageModules] = modules; manifest[packageDeclarations] = declarations;
  return manifest;
}

// Cargo treats a bare version as a caret requirement; commas are conjunctions.
// Build metadata (`0.38.0+1.3.281`) takes no part in either, as in SemVer 2.0.0 §10.
export function gpuVersionMatches(version, requirement) {
  if (requirement.includes('||')) return false;
  const bare = v => v.replace(/\+[0-9A-Za-z.-]*$/, '');
  const range = requirement.split(',').map(r => bare(r.trim())).map(r => /^\d+(?:\.\d+){0,2}$/.test(r) ? `^${r}` : r).join(' ');
  try { return Bun.semver.satisfies(bare(version), range); } catch { return false; }
}

/** Whole-graph starting points (LLP 1153.003 D5.4). Merged prefers newer compatible shared blocks;
 * package-first keeps every package block except exact-gpu and competing exact pins, adding only the checkout's
 * exact-gpu and the pinned closure. SDK-first keeps the SDK's block for each name, series and source it holds. */
export function packageGpuLock(sdk, own, attempt = 'merged', pins = {}) {
  const graph = text => {
    const blocks = lockBlocks(text), packages = Bun.TOML.parse(text).package ?? [];
    const edge = dep => { const [name, version, ...source] = dep.split(' '); return packages.findIndex(p => p.name === name && (!version || p.version === version) && (!source.length || p.source === source.join(' ').slice(1, -1))); };
    const reached = (roots, skipGpu = false) => {
      const seen = new Set(), pending = [...roots];
      while (pending.length) {
        const i = pending.pop();
        if (i < 0 || seen.has(i) || skipGpu && packages[i].name === 'exact-gpu') continue;
        seen.add(i);
        pending.push(...(packages[i].dependencies ?? []).map(edge));
      }
      return seen;
    };
    return { blocks, packages, reached };
  };
  const s = graph(sdk), o = graph(own);
  const gpu = o.reached(o.packages.flatMap((p, i) => p.name === 'exact-gpu' ? [i] : []));
  const shared = o.reached(o.packages.flatMap((_, i) => !gpu.has(i) ? [i] : []), true);
  const keep = o.blocks.filter((_, i) => attempt === 'package-first' ? o.packages[i].name !== 'exact-gpu' : !gpu.has(i) || shared.has(i))
    .map(b => b.replace(/^ "exact-gpu[^"\n]*",\n/gm, ''));
  const sdkGpu = s.reached(s.packages.flatMap((p, i) => ['exact-gpu', 'wasm-bindgen', 'wasm-bindgen-futures', 'web-sys'].includes(p.name) ? [i] : []));
  const lock = blocks => `version = 4\n\n${blocks.join('\n\n')}\n`;
  const pinned = s.packages.flatMap((p, i) => pins[p.name]?.some(req => gpuVersionMatches(p.version, req)) ? [i] : []);
  if (attempt === 'package-first') {
    const needed = s.reached(pinned);
    const added = s.blocks.filter((_, i) => s.packages[i].name === 'exact-gpu' || needed.has(i));
    const pinnedSeries = new Set(pinned.map(i => lockSeriesSource(s.blocks[i])));
    const ownBlocks = keep.filter(b => !pinnedSeries.has(lockSeriesSource(b)));
    // Keep the package's dependency blocks, including multiple versions in one series.
    const identities = new Set(ownBlocks.map(lockBlockId));
    return lock([...ownBlocks, ...added.filter(b => !identities.has(lockBlockId(b)))]);
  }
  return mergeLockBlocks(lock(s.blocks.filter((_, i) => sdkGpu.has(i) || pinned.includes(i))), keep,
    attempt === 'merged' ? new Set(pinned.map(i => lockSeriesSource(s.blocks[i]))) : null);
}

/** Resolve a generated package GPU workspace against the two shipped locks. */
export function settlePackageGpuLock(dir, sdk, own, label) {
  const options = { cwd: dir, env: cargoEnvironment(), encoding: 'utf8', maxBuffer: 1 << 26 };
  const failures = [];
  const allowed = `${sdk}\n${lockBlocks(own).join('\n\n')}`;
  // The generated manifest carries the dependencies copied from game/Cargo.toml, including CLI pins.
  const manifest = Bun.TOML.parse(readFileSync(resolve(dir, 'Cargo.toml'), 'utf8'));
  const tables = [manifest.dependencies, manifest['build-dependencies'],
    ...Object.values(manifest.target ?? {}).flatMap(t => [t.dependencies, t['build-dependencies']])];
  const pins = {};
  for (const table of tables.filter(Boolean)) for (const [alias, spec] of Object.entries(table)) {
    const requirement = typeof spec === 'string' ? spec : spec.version;
    if (requirement?.split(',').some(r => r.trim().startsWith('='))) (pins[spec.package ?? alias] ??= []).push(requirement);
  }
  for (const attempt of ['merged', 'package-first', 'sdk-first']) {
    writeChanged(resolve(dir, 'Cargo.lock'), packageGpuLock(sdk, own, attempt, pins));
    let resolved = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], options);
    if (resolved.status !== 0 && OFFLINE_MISSING.test(resolved.stderr ?? '')) {
      // Fetch may resolve missing sources; validation below binds that resolution to the input locks.
      const fetched = spawnSync('cargo', ['fetch'], options);
      resolved = fetched.status === 0 ? spawnSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], options) : fetched;
    }
    const outside = resolved.status === 0 ? repairOutside(dir, options, allowed) : [];
    if (resolved.status === 0 && !outside.length) return;
    const reason = outside.length ? `requires versions or checksums neither lock holds: ${outside.join(', ')}`
      : resolved.stderr || resolved.error?.message;
    failures.push(`${attempt}: ${reason}`);
  }
  throw new Error(`${label}: its locked dependencies have no accepted resolution:\n${failures.join('\n')}`);
}

/** A crate neither lock holds is one Cargo picked from its cache for an edge no lock entry fixed. Each is replaced
 * by the newest block of its name, series and source that a lock holds and that Cargo then accepts as resolved
 * (a lock edit rather than `cargo update --precise`, which a directory source ignores); what remains is returned. */
function repairOutside(dir, options, allowed) {
  const lockPath = resolve(dir, 'Cargo.lock'), held = lockBlocks(allowed), tried = new Set();
  const field = (block, key) => new RegExp(`^${key} = "([^"]*)"`, 'm').exec(block)?.[1] ?? '';
  for (;;) {
    const outside = outsideExact2(readFileSync(lockPath, 'utf8'), allowed);
    const id = outside.find(o => !tried.has(o));
    if (!id) return outside;
    tried.add(id);
    const [name, version, source = ''] = id.split(' ');
    const before = readFileSync(lockPath, 'utf8');
    const current = lockBlocks(before).find(b => field(b, 'name') === name && field(b, 'version') === version);
    const candidates = held.filter(b => field(b, 'name') === name && field(b, 'source') === source && series(field(b, 'version')) === series(version))
      .sort((a, b) => Bun.semver.order(field(b, 'version'), field(a, 'version')));
    for (const block of candidates) {
      const precise = field(block, 'version');
      writeFileSync(lockPath, before.replace(current, block).replaceAll(`"${name} ${version}`, `"${name} ${precise}`));
      let moved = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], options);
      // An allowed version absent from the local cache is fetched, as the attempt's own resolution is.
      if (moved.status !== 0 && OFFLINE_MISSING.test(moved.stderr ?? '')
        && spawnSync('cargo', ['fetch'], options).status === 0) {
        moved = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], options);
      }
      const after = readFileSync(lockPath, 'utf8');
      // Cargo rewrites the block's dependency list; its identity and checksum are what must hold.
      const kept = lockBlocks(after).some(b => field(b, 'name') === name && field(b, 'version') === precise
        && field(b, 'checksum') === field(block, 'checksum'));
      if (moved.status === 0 && kept && !outsideExact2(after, allowed).includes(id)) break;
      writeFileSync(lockPath, before);
    }
  }
}

function gpuRequirementsSource(modules) {
  const pairs = values => Object.entries(values ?? {}).map(([name, value]) => `(${JSON.stringify(name)}, ${value})`).join(', ');
  const strings = values => (values ?? []).map(value => JSON.stringify(value)).join(', ');
  return '&[' + modules.map(m => {
    const targets = m.targets?.map(t => t === 'web' ? 'target_arch = "wasm32"' : `target_os = "${t}"`);
    return `${targets ? `#[cfg(any(${targets.join(', ')}))] ` : ''}exact_gpu::grants::Requirements {
    package: ${JSON.stringify(m.package)}, surfaces: &[${strings(m.surfaces)}],
    required_limits: &[${pairs(m.limits?.required)}], optional_limits: &[${pairs(m.limits?.optional)}],
    required_features: &[${strings(m.features?.required)}], optional_features: &[${strings(m.features?.optional)}],
}`;
  }).join(', ') + ']';
}

/** One generated cdylib per package, resolved without any host or other package's graph (LLP 1153.003 D5). */
export function ensurePackageGpu(app, module) {
  const dir = packageGpuDir(app, module), source = resolve(module.root, module.crate);
  const lockPath = resolve(source, 'Cargo.lock'), manifestPath = resolve(source, 'Cargo.toml');
  if (!existsSync(lockPath)) throw new Error(`${module.package}: its GPU crate must ship Cargo.lock (${lockPath})`);
  const manifest = Bun.TOML.parse(readFileSync(manifestPath, 'utf8'));
  const version = Bun.TOML.parse(readFileSync(resolve(ROOT, 'gpu/Cargo.toml'), 'utf8')).package.version;
  if (!gpuVersionMatches(version, module.exactGpu)) throw new Error(`${module.package} requires exact-gpu ${module.exactGpu}, but this checkout provides ${version}`);
  const tables = [manifest.dependencies, manifest['build-dependencies'], ...Object.values(manifest.target ?? {}).flatMap(t => [t.dependencies, t['build-dependencies']])].filter(Boolean);
  let usesGpu = false;
  for (const table of tables) for (const [key, spec] of Object.entries(table)) {
    const dep = spec.package ?? key;
    if (dep === 'exact-gpu') {
      const req = typeof spec === 'string' ? spec : spec.version;
      if (!req || spec.path || spec.git || spec.workspace || !gpuVersionMatches(version, req)) throw new Error(`${module.package}: exact-gpu must require the checkout's ${version} from crates.io, without a path, git or workspace dependency`);
      usesGpu = true;
    } else if (dep.startsWith('exact-') || dep === 'contract' || dep.startsWith('contract-') || SDK_CRATES.has(dep)) {
      const path = typeof spec === 'object' && spec.path && realpathSync(resolve(source, spec.path));
      const own = path && (path === module.root || path.startsWith(`${module.root}/`)) && !SDK_CRATES.has(dep);
      if (!own) throw new Error(`${module.package}: a GPU package may depend on exact-gpu, not ${dep}`);
    }
  }
  if (!usesGpu) throw new Error(`${module.package}: its GPU crate must depend on exact-gpu`);
  if (manifest.package?.workspace || Object.keys(manifest.workspace ?? {}).length) throw new Error(`${module.package}: its GPU crate must have a standalone Cargo.toml`);
  const crateName = manifest.package?.name;
  if (!crateName) throw new Error(`${manifestPath}: missing package.name`);
  const alias = crateName.replaceAll('-', '_');
  for (const spec of Object.values(module.declarations)) if (!spec.factory.startsWith(`${alias}::`)) throw new Error(`${module.package}: factory ${spec.factory} must start with ${alias}::`);
  mkdirSync(resolve(dir, 'src'), { recursive: true });
  const pins = Bun.TOML.parse(readFileSync(resolve(ROOT, 'game/Cargo.toml'), 'utf8')).workspace.dependencies;
  const toml = v => typeof v === 'string' ? JSON.stringify(v) : `{ ${Object.entries(v).map(([k, v]) => `${k} = ${JSON.stringify(v)}`).join(', ')} }`;
  writeChanged(resolve(dir, 'Cargo.toml'), `[workspace]\nresolver = "2"\n\n[package]\nname = "${app.crate(`gpu-${module.name}`)}"\nversion = "0.1.0"\nedition = "2021"\npublish = false\n\n[lib]\ncrate-type = ["cdylib"]\n\n[dependencies]\nexact-gpu = { path = ${JSON.stringify(resolve(ROOT, 'gpu'))} }\n${alias} = { package = ${JSON.stringify(crateName)}, path = ${JSON.stringify(source)} }\n\n[target.'cfg(target_arch = "wasm32")'.dependencies]\n${['wasm-bindgen', 'wasm-bindgen-futures', 'web-sys'].map(d => `${d} = ${toml(pins[d])}`).join('\n')}\n\n[patch.crates-io]\nexact-gpu = { path = ${JSON.stringify(resolve(ROOT, 'gpu'))} }\n${patchLines(dir, pathWithin).join('\n')}\n`);
  writeChanged(resolve(dir, 'src/lib.rs'), `use exact_gpu::Registry;\npub static REGISTRY: Registry = Registry {\n    surfaces: &[${module.surfaces.map(s => `(${JSON.stringify(s)}, ${module.declarations[s].arity}, ${module.declarations[s].factory})`).join(', ')}],\n    shaders: &[],\n};\nexact_gpu::module!(REGISTRY, ${gpuRequirementsSource([module])});\n`);
  for (const file of ['rust-toolchain.toml', '.cargo/config.toml']) writeChanged(resolve(dir, file), readFileSync(resolve(ROOT, file), 'utf8'));
  const sdk = readFileSync(resolve(ROOT, 'Cargo.lock'), 'utf8'), own = readFileSync(lockPath, 'utf8');
  const fingerprint = createHash('sha256').update(sdk).update(own).update(readFileSync(manifestPath))
    .update(readFileSync(resolve(dir, 'Cargo.toml'))).update(readFileSync(fileURLToPath(import.meta.url))).digest('hex');
  const stamp = resolve(dir, '.lock-inputs');
  let metadata = existsSync(stamp) && readFileSync(stamp, 'utf8') === fingerprint ? lockedMetadata(dir) : null;
  if (metadata?.status !== 0) {
    settlePackageGpuLock(dir, sdk, own, `${module.package} with exact-gpu ${version}`);
    metadata = lockedMetadata(dir);
  }
  if (metadata.status !== 0) throw new Error(`${module.package}: ${metadata.stderr || metadata.error?.message}`);
  const packages = JSON.parse(metadata.stdout).packages, gpu = packages.filter(p => p.name === 'exact-gpu');
  if (gpu.length !== 1 || gpu[0].source !== null || realpathSync(gpu[0].manifest_path) !== resolve(ROOT, 'gpu/Cargo.toml')) throw new Error(`${module.package}: exact-gpu must resolve only from this checkout, never from a registry or another source`);
  writeChanged(stamp, fingerprint);
  return dir;
}
/** Two workspaces' metadata as one graph: the host's, and its GPU crates' resolved apart from it. A package both
 * hold is one node, with the dependencies either resolved for it. */
export function joinMetadata(host, gpu) {
  const packages = new Map([...host.packages, ...gpu.packages].map(p => [p.id, p])), nodes = new Map();
  for (const node of [...host.resolve.nodes, ...gpu.resolve.nodes]) {
    const was = nodes.get(node.id);
    nodes.set(node.id, was ? { ...was, deps: [...was.deps, ...node.deps.filter(d => !was.deps.some(w => w.pkg === d.pkg))] } : node);
  }
  return { ...host, packages: [...packages.values()], workspace_members: [...host.workspace_members, ...gpu.workspace_members], resolve: { ...host.resolve, nodes: [...nodes.values()] } };
}

/** The generated workspace's lock settled for a build (LLP 1153.003 D1.4), and its locked metadata. The app's captured
 * lock (`captured`, beside app.json) must match its crates: a build never changes it. Without one, the lock derived
 * from exact2's stands while it is current and holds nothing exact2's lacks; otherwise it is derived again, fetching
 * only what this machine's cache lacks, and a derivation that needs a version exact2's lock does not hold is refused,
 * since only `bun exact.mjs lock` captures a lock. */
export function settleLock(dir, workspace, captured = resolve(dir, 'Cargo.lock')) {
  const shell = resolve(workspace, 'Cargo.lock'), sdk = readFileSync(resolve(ROOT, 'Cargo.lock'), 'utf8');
  const relock = `\`bun exact.mjs lock\` in ${dir}`;
  if (existsSync(captured)) {
    writeChanged(shell, readFileSync(captured, 'utf8'));
    const result = lockedMetadata(workspace);
    if (result.status === 0) return JSON.parse(result.stdout);
    throw new Error(staleLock(result.stderr)
      ? `${captured} does not match the app's crates (a dependency or member changed since it was captured), and a build never changes it: ${relock} resolves it again.\n${result.stderr.trim()}`
      : `cargo metadata --locked --offline in ${workspace}:\n${result.stderr || result.error?.message}`);
  }
  const derived = existsSync(shell) ? readFileSync(shell, 'utf8') : sdk;
  if (!outsideExact2(derived, sdk).length) {
    const result = lockedMetadata(workspace);
    if (result.status === 0) return JSON.parse(result.stdout);
  }
  writeFileSync(shell, sdk);
  const options = { cwd: workspace, env: cargoEnvironment(), stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' };
  let resolved = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], options);
  if (resolved.status !== 0) {
    console.error(`${workspace}: resolving its lock from exact2's, fetching what Cargo's cache lacks (cargo metadata)`);
    resolved = spawnSync('cargo', ['metadata', '--format-version', '1'], options);
  }
  const outside = resolved.status === 0 ? outsideExact2(readFileSync(shell, 'utf8'), sdk) : [];
  if (resolved.status !== 0 || outside.length) writeFileSync(shell, sdk);
  if (resolved.status !== 0) throw new Error(`cargo metadata in ${workspace}:\n${resolved.stderr || resolved.error?.message}`);
  if (outside.length) throw new Error(`${dir}: the app's crates need versions exact2's lock does not hold (${listed(outside)}). An app whose crates add dependencies commits a lock of its own: ${relock} captures it.`);
  const result = lockedMetadata(workspace);
  if (result.status !== 0) throw new Error(`cargo metadata --locked --offline in ${workspace}:\n${result.stderr || result.error?.message}`);
  return JSON.parse(result.stdout);
}

/** `bun exact.mjs lock` for a generated workspace (LLP 1153.003 D1.4): resolve from the captured lock when present,
 * otherwise from exact2's, fetching what the graph lacks. Capture the result when it leaves exact2's lock or a
 * lock was captured before. What it did, as a line. */
export function lockShells(dir, workspace, captured = resolve(dir, 'Cargo.lock')) {
  const shell = resolve(workspace, 'Cargo.lock'), sdk = readFileSync(resolve(ROOT, 'Cargo.lock'), 'utf8');
  const before = existsSync(captured) ? readFileSync(captured, 'utf8') : null;
  writeFileSync(shell, before ?? sdk);
  const options = { cwd: workspace, env: cargoEnvironment(), stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' };
  let result = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], options);
  if (result.status !== 0) {
    console.error(`${workspace}: fetching what Cargo's cache lacks to resolve the app's lock`);
    result = spawnSync('cargo', ['metadata', '--format-version', '1'], options);
  }
  if (result.status !== 0) {
    if (before !== null) writeFileSync(shell, before);
    throw new Error(`could not resolve ${captured}: ${result.stderr || result.error?.message}`);
  }
  const after = readFileSync(shell, 'utf8');
  if (before === null && !outsideExact2(after, sdk).length) return `${workspace} resolves against exact2's lock; the app's crates there add nothing, so it commits no ${basename(captured)} of its own`;
  writeChanged(captured, after);
  const changes = lockChanges(before, after);
  return before === null ? `${captured}: captured (${(Bun.TOML.parse(after).package ?? []).length} packages); commit it`
    : `${captured}: ${changes.length ? `resolved\n${changes.join('\n')}` : 'current; no changes'}`;
}

/** What a native verb does before its build (LLP 1153 D1.2): when the manifest does not name `platform`, check that
 * platform's setup rows (`report`: the rows, or `{ error }` when they could not be read) and stop on a missing one,
 * nothing written; then name it, writing an app.json the app lacked. Then keep the generated workspace current unless
 * the app authored one (`ensureShells`). Returns the lines to say, or `{ refused }`. */
export async function bringPlatform(dir, platform, { report = null, os = process.platform, name = null, verb = platform === 'macos' ? 'mac' : platform } = {}) {
  const lines = [];
  verb = `bun exact.mjs ${verb}`;
  const manifest = existsSync(resolve(dir, 'app.json')) ? readManifest(dir) : null;
  if (!namesPlatform(manifest ?? {}, platform)) {
    if (platform === 'ios' && !['darwin', 'linux'].includes(os)) return { refused: `${verb}: iOS builds need macOS or Linux (use WSL on Windows); nothing was written` };
    if (platform === 'macos' && os !== 'darwin') return { refused: `${verb}: an Apple build needs a Mac with Xcode; nothing was written` };
    if (platform === 'windows' && os !== 'win32') return { refused: `${verb}: a Windows build needs Windows with the MSVC Rust target; nothing was written` };
    if (report?.error) return { refused: `${verb}: the setup report could not be read, so nothing was written: ${report.error}` };
    const missing = (report ?? []).filter(row => row.required && !row.ok);
    if (missing.length) return { refused: `${verb}: this machine lacks what a ${platform} build needs; nothing was written:\n${missing.map(row => `  ${row.name}: ${row.have} (SDK ${row.want})\n    ${row.fix}`).join('\n')}` };
    if (!manifest) {
      // An app without app.json had the defaults scripts/app.mjs `readManifest` supplies; naming a platform writes them.
      const app = name ?? basename(dir), title = titleOf(app);
      writeManifest(dir, { name: title, app: { id: `com.exact.${app}`, name: title }, host: { web: {} }, deploy: { store: { web: '0' } } });
      lines.push('app.json: written with the defaults the app had without one');
    }
    const wrote = namePlatform(dir, platform);
    if (wrote.length) lines.push(`app.json: ${wrote.join(', ')} added: the app now ships ${platform} (bun exact.mjs update --without ${platform} stops it)`);
  }
  if (!authoredWorkspace(dir)) {
    let shells;
    // Generating the workspace resolves it with Cargo: without Rust, say so as the checks above do.
    try { shells = ensureShells(dir, undefined, name); } catch (error) { return { refused: `${verb}: ${error.message}` }; }
    const { workspace, wrote, crates } = shells;
    if (wrote) lines.push(`${workspace}: the native workspace (${crates.join(', ')}), generated from app.json`);
  }
  return { lines };
}

// Cargo's compatible semver series: 1.x, 0.37.x, 0.0.3.
export const series = version => { const [major, minor, patch] = String(version).split('.'); return major !== '0' ? major : minor !== '0' ? `0.${minor}` : `0.0.${patch}`; };
export const seriesId = block => `${/^name = "([^"]+)"/m.exec(block)?.[1]} ${series(/^version = "([^"]+)"/m.exec(block)?.[1])}`;
export const lockBlocks = text => text.split(/\n(?=\[\[package\]\]\n)/).slice(1).map(block => block.trimEnd());

// Git revisions belong to one source, as Cargo's update of a locked git dependency does.
const lockSeriesSource = block => `${seriesId(block)} ${/^source = "([^"#]*)/m.exec(block)?.[1] ?? ''}`;
const lockBlockId = block => `${/^name = "([^"]+)"/m.exec(block)[1]} ${/^version = "([^"]+)"/m.exec(block)[1]} ${/^source = "([^"]*)"/m.exec(block)?.[1] ?? ''}`;

/** Merge by compatible series and source, preserving exact pins; null keeps every SDK choice. */
export function mergeLockBlocks(sdk, incoming, pinned = new Set()) {
  const held = new Map(lockBlocks(sdk).map(block => [lockSeriesSource(block), block]));
  const version = block => /^version = "([^"]+)"/m.exec(block)[1];
  for (const block of incoming) {
    const id = lockSeriesSource(block), previous = held.get(id);
    if (!previous || pinned && !pinned.has(id) && Bun.semver.order(version(block), version(previous)) > 0
      && gpuVersionMatches(version(block), version(previous))) held.set(id, block);
  }
  return `version = 4\n\n${[...held.values()].join('\n\n')}\n`;
}

// A lock's packages by identity. Dependency edges follow from the versions
// and the activated features, so a workspace's subset keeps every version but may
// drop edges; the versions and checksums are what the SDK lock decides.
const lockIds = text => new Map((Bun.TOML.parse(text).package ?? []).map(pkg => [`${pkg.name} ${pkg.version} ${pkg.source ?? ''}`.trim(), pkg.checksum ?? null]));
/** Packages of `derived` (members excepted) whose version the SDK lock does not hold.
 * A root path crate new since the SDK lock is held through `withRootPins`. */
export function outsideSdkLock(derived, sdk, members) {
  const known = lockIds(sdk);
  return [...lockIds(derived)].filter(([id, checksum]) => !members.has(id.split(' ')[0]) && (!known.has(id) || known.get(id) !== checksum)).map(([id]) => id);
}

/** What a lock refresh changed, by package name, for `lock` to print: the
 * versions (or a git or path package's source) that moved, then what was
 * added, then what was removed; with none of those, the dependencies a package
 * gained or lost (Cargo rewrites them when a manifest gains or drops a package the lock already holds). */
export function lockChanges(before, after) {
  // As an author reads a package: its version, and where it comes from unless crates.io.
  const shown = pkg => `${pkg.version}${!pkg.source ? ' (path)' : pkg.source === CRATES_IO ? '' : ` (${pkg.source})`}`;
  const read = text => (text ? Bun.TOML.parse(text).package ?? [] : []).reduce((map, pkg) => map.set(pkg.name, [...(map.get(pkg.name) ?? []), shown(pkg)].sort()), new Map());
  const was = read(before), now = read(after), moved = [], added = [];
  for (const [name, versions] of now) {
    const old = was.get(name);
    if (!old) added.push(`added ${name} ${versions.join(', ')}`);
    else if (old.join() !== versions.join()) moved.push(`${name} ${old.join(', ')} → ${versions.join(', ')}`);
  }
  const changes = [...moved, ...added, ...[...was].filter(([name]) => !now.has(name)).map(([name, versions]) => `removed ${name} ${versions.join(', ')}`)];
  if (changes.length) return changes;
  const lists = text => new Map((text ? Bun.TOML.parse(text).package ?? [] : []).map(pkg => [`${pkg.name} ${shown(pkg)}`, pkg.dependencies ?? []]));
  const had = lists(before);
  return [...lists(after)].flatMap(([pkg, deps]) => [...deps.filter(dep => !had.get(pkg)?.includes(dep)).map(dep => `${pkg} now depends on ${dep}`),
    ...(had.get(pkg) ?? []).filter(dep => !deps.includes(dep)).map(dep => `${pkg} no longer depends on ${dep}`)]);
}
export const CRATES_IO = 'registry+https://github.com/rust-lang/crates.io-index';

/** Whether Cargo refused a locked graph because the lock no longer matches the
 * manifests, a lock to refresh, rather than for a crate missing from its cache. */
export const staleLock = stderr => /cannot update the lock file|lock file \S+ needs to be updated/.test(stderr ?? '');

// Only explicitly developmental gpu-dev hosts may be reused across GPU edits.
export const bindGpuProduct = (profile, trust) => profile !== 'gpu-dev' || trust !== 'development';
export function bakeSelection(graph, part) {
  if (part && !['gpu','host'].includes(part)) throw new Error(`unknown bake part: ${part}`);
  const gpu = [graph.surface, ...(graph.modules ?? [])];
  return (part === 'gpu' ? gpu : part === 'host' ? [graph.root] : [...gpu,graph.root]).filter(Boolean);
}
