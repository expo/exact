// `exact new <path> --rust`: an app whose data sources are a Rust crate instead of app.ts (LLP 1153.003 D4). It writes
// `data/`, the DataSource with the view's shapes generated into it by `contract rust`, as a member of the workspace
// exact2 generates (`package.workspace = "../.exact/shells"`), and names it in app.json's `rust.data`, which every
// host crate and the web's module build over (D2).
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

/** Names the starter's data/src/lib.rs already uses, and Rust's own: a source type of one of these would not compile. */
const TAKEN = new Set(['Self', 'Value', 'DataError', 'DataSource', 'ContractValue', 'Greeting', 'Option', 'Result', 'Some', 'None', 'Ok', 'Err', 'String', 'Vec', 'Box']);

/** `<crate>-data`'s Rust names: the library (`notes_data`) and the source type (`Notes`; `ValueApp` for a name the
 * starter already uses, `APIApp` for one in capitals). */
export function rustNames(crate, title) {
  const type = title.normalize('NFKD').replace(/[^A-Za-z0-9 ]/g, ' ').split(/\s+/).filter(Boolean)
    .map(w => w[0].toUpperCase() + w.slice(1)).join('').replace(/^(?=\d)/, 'App') || 'App';
  // An all-capitals type (`API`) fails clippy's default upper_case_acronyms lint.
  return { lib: `${crate.replaceAll('-', '_')}_data`, type: TAKEN.has(type) || /^[A-Z][A-Z0-9_]+$/.test(type) ? `${type}App` : type };
}

/** `writeApp`'s listing for a Rust app: where its data is, without `pattern`, which writes app.ts. */
export function rustListing(created) {
  return created.split('\n')
    .filter(line => !/^\s+bun exact\.mjs pattern\b/.test(line))
    .map((line, i) => i ? line : `${line} (its data is Rust: data/src/lib.rs)`).join('\n');
}

/** The files of the Rust scaffold's `data/` crate, by path. The starter's one source, `greeting(name)`, is
 * app.contract's (game/new.mjs `appSources`). */
export function rustSources(crate, title) {
  const { lib, type } = rustNames(crate, title);
  return {
    'data/Cargo.toml': `# The app's data: the DataSource that answers app.contract's sources (docs/reference.md, "Rust data sources").
# It is a member of the workspace exact2 generates from app.json, which also holds the host crates that link it.
[package]
name = "${crate}-data"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false
workspace = "../.exact/shells"

[lib]
name = "${lib}"
crate-type = ["rlib"]

[dependencies]
exact-runner.workspace = true

# \`build.rs\` writes the view's shapes as Rust structs (\`contract rust\`).
[build-dependencies]
contract.workspace = true
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
  };
}

/** Turn the app `writeApp` made in `dir` into a Rust one: app.ts goes, `data/` and app.json's `rust.data` come. */
export function writeRustApp(dir, crate, title) {
  rmSync(resolve(dir, 'app.ts'));
  const view = resolve(dir, 'app.contract');
  writeFileSync(view, readFileSync(view, 'utf8').replace('app.ts answers what it asks for.', 'data/src/lib.rs answers what it asks for.'));
  const { lib, type } = rustNames(crate, title), manifest = resolve(dir, 'app.json'), json = JSON.parse(readFileSync(manifest, 'utf8'));
  json.rust = { ...json.rust, data: { crate: `${crate}-data`, type: `${lib}::${type}` } };
  writeFileSync(manifest, JSON.stringify(json, null, 2) + '\n');
  for (const [path, text] of Object.entries(rustSources(crate, title))) {
    mkdirSync(dirname(resolve(dir, path)), { recursive: true });
    writeFileSync(resolve(dir, path), text);
  }
}
