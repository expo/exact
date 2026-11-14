// `exact new <path> --rust`: an app whose data sources are a Rust crate instead of app.ts (LLP 1153 D1.4). It writes
// the app's own Cargo workspace beside app.json, as an app that needs Rust of its own does by hand: `data/` (the
// DataSource, with the view's shapes generated into it by `contract rust`), and the host crates `apple/` and `web/`,
// shaped as apps/sparkline's are. Paths into this checkout are written `<exact2>/…`, which `update` resolves.
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { patchLines } from './app.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

/** The platforms a Rust app's scaffold has host crates for; another's crate is copied from apps/sparkline. */
export const RUST_PLATFORMS = ['ios', 'macos'];

/** Names the starter's data/src/lib.rs already uses, and Rust's own: a source type of one of these would not compile. */
const TAKEN = new Set(['Self', 'Value', 'DataError', 'DataSource', 'ContractValue', 'Greeting', 'Option', 'Result', 'Some', 'None', 'Ok', 'Err', 'String', 'Vec', 'Box']);

/** `<crate>-data`'s Rust names: the library (`notes_data`) and the source type (`Notes`; `ValueApp` for a name the
 * starter already uses, `APIApp` for one in capitals). */
export function rustNames(crate, title) {
  const type = title.normalize('NFKD').replace(/[^A-Za-z0-9 ]/g, ' ').split(/\s+/).filter(Boolean)
    .map(w => w[0].toUpperCase() + w.slice(1)).join('').replace(/^(?=\d)/, 'App') || 'App';
  // An all-capitals name (`API`) reads as a constant to the web build, which copies only a type (host/web-js/module.mjs).
  return { lib: `${crate.replaceAll('-', '_')}_data`, type: TAKEN.has(type) || /^[A-Z][A-Z0-9_]+$/.test(type) ? `${type}App` : type };
}

/** `writeApp`'s listing for a Rust app: where its data is, without the verbs it has no crate or use for (`pattern`
 * writes app.ts; `linux` and `windows` need a crate copied from apps/sparkline). */
export function rustListing(created) {
  return created.split('\n')
    .filter(line => !/^\s+bun exact\.mjs (pattern|linux|windows)\b/.test(line))
    .map((line, i) => i ? line : `${line} (its data is Rust: data/src/lib.rs, which apple/ and web/ link)`).join('\n');
}

const hostBuild = (lib, type, kind) => `//! Compile and bake \`app.contract\` into \`OUT_DIR/app.plan\` at build time, so the ${kind === 'web' ? 'wasm' : 'library'}
//! carries its plan and the first frame shows what \`${lib}::${type}\` answers.

use contract::DataSource;

fn main() {
    contract::rerun_if_changed(std::path::Path::new("../app.contract"));
    println!("cargo:rerun-if-changed=../app.json");
    println!("cargo:rerun-if-changed=../data");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    let plan = match contract::compile_path(std::path::Path::new("../app.contract")) {
        Ok(p) => p,
        Err(e) => panic!("app.contract:{e}"),
    };
    // The web build copies this source into a module of its own, as the source the app runs: a path from the
    // data crate, or its ::default() or ::new(…) with literal arguments (docs/reference.md, "Rust data sources").
    let baked = match contract::bake(plan, ${lib}::${type}::default()) {
        Ok(baked) => baked,
        Err(e) => panic!("bake: {e:?}"),
    };
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("app.plan"), baked.encode()).unwrap();
    let app_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let target = std::env::var("TARGET").unwrap_or_default();
${kind === 'web' ? '    let platform = "web";' : `    let platform = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("ios" | "tvos" | "watchos") => "ios",
        _ => "macos",
    };`}
    let manifest = contract::Manifest::read(&app_dir).unwrap_or_else(|e| panic!("app.json: {e}"));
    let source = ${lib}::${type}::default();
    let grants = source.grants();
    let compat = exact_bake::compatibility_id(&app_dir, platform, &target, &manifest, Some(grants))
        .unwrap_or_else(|e| panic!("compatibility id: {e}"));
    std::fs::write(out_dir.join("compat.json"), compat.to_json()).unwrap();
${kind === 'web' ? `    let entry = contract::rust_entry(
        "${lib}::${type}",
        "${lib}::${type}::default()",
        contract::web_rust_mode(&compat.inputs),
        &compat.inputs,
    )
    .unwrap();
    std::fs::write(
        out_dir.join("entry.rs"),
        format!(
            "{entry}\\n{}exact_web::host!(AppData, PLAN, COMPAT, app_data);\\n",
            contract::web_linked(&baked, &compat.inputs)
        ),
    )
    .unwrap();` : `    let host = if compat.inputs["store"]["L"] == "0" {
        "exact_apple"
    } else {
        "exact_apple_update"
    };
    // What the archive links, as its compatibility inputs name it (LLP 1047.001 D2).
    let linked = exact_bake::apple_link(&compat, host);
    let entry = contract::rust_entry(
        "${lib}::${type}",
        "${lib}::${type}::default()",
        compat.inputs["rustMode"].as_str().unwrap(),
        &compat.inputs,
    )
    .unwrap();
    std::fs::write(
        out_dir.join("entry.rs"),
        format!("{entry}\\n{linked}{host}::host!(AppData, PLAN, COMPAT; linked = EXACT_LINKED);\\n"),
    )
    .unwrap();`}
}
`;

const hostLib = (title, kind) => `//! ${title} on ${kind === 'web' ? 'the web' : 'Apple platforms'}: the host's exports over the data crate and the baked plan.

/// The baked plan, written by \`build.rs\`.
pub const PLAN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.plan"));

/// The compatibility id and its inputs (LLP 1030 D3a), written beside it.
pub const COMPAT: &str = include_str!(concat!(env!("OUT_DIR"), "/compat.json"));

include!(concat!(env!("OUT_DIR"), "/entry.rs"));
`;

const manifest = (crate, kind, deps) => `[package]
name = "${crate}-${kind}"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false
build = "build.rs"

[lib]
crate-type = [${kind === 'web' ? '"cdylib", "rlib"' : '"rlib"'}]
test = false
bench = false
${kind === 'web' ? `
[[bin]]
name = "${crate}-dev"
path = "src/bin/${crate}-dev.rs"
test = false
bench = false
` : ''}
[dependencies]
${deps.map(([name, path]) => `${name} = { path = "<exact2>/${path}" }`).join('\n')}
${crate}-data = { path = "../data" }

[build-dependencies]
contract = { path = "<exact2>/contract/cli" }
exact-bake = { path = "<exact2>/bake" }
${crate}-data = { path = "../data" }
`.replace(/\n{3,}/g, '\n\n');

/** The files of the Rust scaffold, by path: the workspace, `data/`, `apple/` and `web/`. The starter's one source,
 * `greeting(name)`, is app.contract's (game/new.mjs `appSources`). */
export function rustSources(crate, title) {
  const { lib, type } = rustNames(crate, title);
  return {
    'Cargo.toml': `# The app's Cargo workspace: \`data/\` answers the view's sources in Rust; \`apple/\` and \`web/\` are what each
# host links (a Linux or Android host: copy apps/sparkline's linux/ crate, or the generated one's). Paths into
# exact2 are written \`<exact2>/…\`; \`bun exact.mjs update\` resolves them, and its build scripts pass the profiles.
[workspace]
members = ["data", "web", "apple"]
resolver = "2"

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"

[patch.crates-io]
${patchLines(ROOT, (_, to) => `<exact2>/${to.slice(ROOT.length + 1)}`).join('\n')}
`,
    'data/Cargo.toml': `# The app's data: the DataSource that answers app.contract's sources (docs/reference.md, "Rust data sources").
[package]
name = "${crate}-data"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[lib]
name = "${lib}"
crate-type = ["rlib"]

[dependencies]
exact-runner = { path = "<exact2>/runner" }

# \`build.rs\` writes the view's shapes as Rust structs (\`contract rust\`).
[build-dependencies]
contract = { path = "<exact2>/contract/cli" }
`,
    'data/build.rs': `//! Write the view's shapes into \`OUT_DIR/shapes.rs\` as Rust structs, so a field added to \`app.contract\` is a
//! compile error here rather than a record built one position off.

fn main() {
    let path = std::path::Path::new("../app.contract");
    // The contract and every file it uses.
    contract::rerun_if_changed(path);
    let (plan, calls) = contract::compile_path_for_rust(path)
        .unwrap_or_else(|e| panic!("app.contract does not compile: {e:?}"));
    let rust = contract::rust(&plan, &calls).expect("contract rust");
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("shapes.rs");
    std::fs::write(out, rust).unwrap();
}
`,
    'data/src/lib.rs': `//! The app's data: what app.contract's sources answer. \`query\` answers now, with no I/O, so the build bakes it
//! into the plan for the first frame; \`answer\` and \`parse\` add storage and requests
//! (docs/reference.md, "Rust data sources").

use exact_runner::{DataError, DataSource, Value};

#[allow(dead_code, clippy::all)]
mod shapes {
    include!(concat!(env!("OUT_DIR"), "/shapes.rs"));
}
use shapes::{ContractValue, Greeting};

/// The app's data source.
#[derive(Default)]
pub struct ${type};

impl DataSource for ${type} {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        match source {
            "greeting" => {
                let name = args
                    .first()
                    .and_then(Value::as_str)
                    .ok_or_else(|| DataError::BadArguments(source.into()))?;
                Ok(Greeting {
                    text: format!("Hello from {name}."),
                }
                .to_value())
            }
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }
}
`,
    'apple/Cargo.toml': manifest(crate, 'apple', [['ibex2-sqlite', 'vendor/ibex/crates/ibex2-sqlite'], ['exact-logic', 'logic'], ['exact-apple', 'host/apple'], ['exact-apple-update', 'host/apple-update']]),
    'apple/build.rs': hostBuild(lib, type, 'apple'),
    'apple/src/lib.rs': hostLib(title, 'apple'),
    'web/Cargo.toml': manifest(crate, 'web', [['exact-logic', 'logic'], ['exact-web', 'host/web'], ['exact-web-capabilities', 'host/web-capabilities']]),
    'web/build.rs': hostBuild(lib, type, 'web'),
    'web/src/lib.rs': hostLib(title, 'web'),
    [`web/src/bin/${crate}-dev.rs`]: `//! The resident dev driver: \`${crate}-dev <source> <out>\`, which the web dev loop runs.

fn main() -> std::process::ExitCode {
    exact_web::dev::main::<${lib}::${type}>()
}
`,
  };
}

/** Turn the app `writeApp` made in `dir` into a Rust one: app.ts goes, the workspace comes. `update` resolves its
 * `<exact2>` paths and writes its toolchain; `lock` resolves its Cargo.lock. */
export function writeRustApp(dir, crate, title) {
  rmSync(resolve(dir, 'app.ts'));
  const view = resolve(dir, 'app.contract');
  writeFileSync(view, readFileSync(view, 'utf8').replace('app.ts answers what it asks for.', 'data/src/lib.rs answers what it asks for.'));
  for (const [path, text] of Object.entries(rustSources(crate, title))) {
    mkdirSync(dirname(resolve(dir, path)), { recursive: true });
    writeFileSync(resolve(dir, path), text);
  }
  mkdirSync(resolve(dir, '.cargo'), { recursive: true });
  copyFileSync(resolve(ROOT, '.cargo/config.toml'), resolve(dir, '.cargo/config.toml'));
  // A lock to resolve from: this checkout's versions, which `lock` prunes to what the app's crates reach.
  copyFileSync(resolve(ROOT, 'Cargo.lock'), resolve(dir, 'Cargo.lock'));
}
