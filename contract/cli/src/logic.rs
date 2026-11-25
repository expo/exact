//! Concrete app composition selected at bake, never a Cargo feature on a host.
//! @ref LLP 1029.000 — disabling replacement removes its reachable loader code.

/// A Rust-only app's source type and constructor as `rust.placement` places them (LLP 1027.002): on `worker`, the
/// source in `exact_data::Placed`, which moves it to an owner of its own at activation; on `main`, unchanged. A
/// mixed app places its own children (`js/bake`'s embedded data) and never comes here. A Rust worker is native
/// only (LLP 1027.002 §7): the web's source stays inline, and the bake says so. A Rust update module (`mode` other
/// than `off`) runs in the swappable's executor, not under `Placed`, so `worker` there is refused, not dropped.
fn placed_source(
    data: &str,
    constructor: &str,
    mode: &str,
    inputs: &serde_json::Value,
) -> Result<(String, String), String> {
    if inputs["rustPlacement"].as_str() != Some("worker") {
        return Ok((data.to_owned(), constructor.to_owned()));
    }
    if inputs["platform"] == "web" {
        println!("cargo:warning=rust.placement worker runs inline on web: a Rust worker is native only (LLP 1027.002 §7)");
        return Ok((data.to_owned(), constructor.to_owned()));
    }
    if mode != "off" {
        return Err(format!(
            "rust.placement worker with Rust updates (rust mode {mode}): an update module runs inline, so its placement would be lost. Set rust.placement to main, or turn Rust updates off"
        ));
    }
    Ok((
        format!("exact_logic::exact_data::Placed<{data}>"),
        format!("exact_logic::exact_data::Placed::new({constructor}, exact_logic::exact_runner::Placement::Worker)"),
    ))
}

/// [`rust_entry`] over a source a mixed app's composer built (`js/bake`'s embedded data), which places its own
/// children: never wrapped again.
/// `app` names an unnamed source as in [`rust_entry_naming`]; `None` takes the manifest's.
pub fn rust_entry_composed(
    data: &str,
    constructor: &str,
    mode: &str,
    inputs: &serde_json::Value,
    app: Option<&str>,
) -> Result<String, String> {
    let manifest = inputs["app"].as_str().map(|app| format!("{app:?}"));
    entry(
        data,
        constructor,
        mode,
        inputs,
        app.or(manifest.as_deref()),
        false,
    )
}

/// Generate `AppData` and `app_data()` for a host's source type and constructor, placed as `rust.placement` says
/// ([`placed_source`]). `inputs` are the actual target bake's compatibility inputs.
/// A data source that names no app id takes the manifest's (`inputs["app"]`),
/// so a native host gives it the app's storage roots (`Swappable::named`).
pub fn rust_entry(
    data: &str,
    constructor: &str,
    mode: &str,
    inputs: &serde_json::Value,
) -> Result<String, String> {
    let app = inputs["app"].as_str().map(|app| format!("{app:?}"));
    rust_entry_naming(data, constructor, mode, inputs, app.as_deref())
}

/// `rust_entry`, where `app` is the Rust expression (a `&str`) that names a
/// data source naming no app id, or `None` to leave it unnamed. A linked
/// Apple bake passes the id the link carries in the executable
/// (`exact_apple::baked::identity`), so the compiled entry is the same for
/// apps that differ only in identity, and an app repacked onto another's
/// executable is named by its own compat.json, not the other's code.
pub fn rust_entry_naming(
    data: &str,
    constructor: &str,
    mode: &str,
    inputs: &serde_json::Value,
    app: Option<&str>,
) -> Result<String, String> {
    entry(data, constructor, mode, inputs, app, true)
}

fn entry(
    data: &str,
    constructor: &str,
    mode: &str,
    inputs: &serde_json::Value,
    app: Option<&str>,
    place: bool,
) -> Result<String, String> {
    declared_source(data, constructor, inputs)?;
    let constructor = native_storage_constructor(constructor, inputs)?;
    let (data, constructor) = if place {
        placed_source(data, &constructor, mode, inputs)?
    } else {
        (data.to_owned(), constructor)
    };
    let data = data.as_str();
    let constructor = constructor.as_str();
    // The digest of the source this binary compiles `constructor`'s crate
    // from (`scripts/rust.mjs` `nativeSource`): a Rust update built from the
    // same source then runs this native code, not the module. Routing compares
    // source digests, never the wasm module's (LLP 1030 D3a, amended 2026-11-12).
    println!("cargo:rerun-if-env-changed=EXACT_NATIVE_SOURCE");
    let native = match std::env::var("EXACT_NATIVE_SOURCE") {
        Ok(source) if !source.is_empty() => {
            if !source.bytes().all(|b| b.is_ascii_hexdigit()) || source.len() != 64 {
                return Err("EXACT_NATIVE_SOURCE must be a sha256 in hex".into());
            }
            format!(".with_native({source:?}, || {constructor})")
        }
        _ => String::new(),
    };
    // Under a variant a source naming the base app's id takes the variant's
    // (LLP 1030.008 D2); `inputs` carries the base beside the id.
    let named = match (app, inputs["variant"]["base"].as_str()) {
        (Some(app), Some(base)) => format!(".named_variant({app}, {base:?})"),
        (Some(app), None) => format!(".named({app})"),
        (None, _) => String::new(),
    };
    let mut supplemental = if let Some(directory) = std::env::var_os("EXACT_RUST_BUNDLE") {
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::path::Path::new(&directory).join("app.module.json"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let file = receipt["module"]["file"]
            .as_str()
            .ok_or("supplemental Rust receipt names no module")?;
        if !matches!(
            file,
            "app.module.wasm"
                | "app.module.dylib"
                | "app.module.so"
                | "app.module.dll"
                | "app.module.bin"
        ) {
            return Err("invalid supplemental Rust module filename".into());
        }
        format!("{{ use exact_logic::exact_runner::DataSource; exact_logic::Swappable::{mode}({constructor}){native}{named}.replacement(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/app.plan\")), include_str!(concat!(env!(\"OUT_DIR\"), \"/rust/app.module.json\")), include_bytes!(concat!(env!(\"OUT_DIR\"), \"/rust/{file}\")).to_vec()).expect(\"validated baked Rust pair\") }}")
    } else {
        format!("exact_logic::Swappable::{mode}({constructor}){native}{named}")
    };
    let has_preloads = inputs["preload"]
        .as_array()
        .is_some_and(|entries| !entries.is_empty());
    if has_preloads {
        supplemental = format!(
            "({supplemental}).with_preloads({})",
            crate::preload_expression(&inputs["preload"])
        );
    }
    match mode {
        "off" if inputs["platform"] == "web" && !has_preloads => Ok(format!("type AppData = {data};\n#[allow(dead_code)]\nfn app_data() -> AppData {{ {constructor} }}\n")),
        "off" | "native" | "tiered" | "wasm" | "browser" => Ok(format!(
            "exact_logic::configured!(AppData, {data}, || {supplemental});\n#[allow(dead_code)]\nfn app_data() -> AppData {{ Default::default() }}\n"
        )),
        _ => Err(format!("unknown Rust executor {mode:?}")),
    }
}

/// Register SQL storage only from a native shell whose composition links it.
/// Refuse a grant beyond that composition while generating the entry.
pub fn native_storage_constructor(
    constructor: &str,
    inputs: &serde_json::Value,
) -> Result<String, String> {
    if inputs["platform"] == "web" {
        return Ok(constructor.to_owned());
    }
    let linked = inputs["linked"]
        .as_array()
        .is_none_or(|names| names.iter().any(|name| name == "sqlite"));
    let required =
        exact_runner::Uses::NONE.with_grants(inputs["grantCeiling"].as_str().unwrap_or(""));
    if !linked && required.has(exact_runner::Capability::Sqlite) {
        return Err("Unlinked(\"sqlite\")".into());
    }
    if linked {
        if let Some(refused) = shell_without_sqlite() {
            return Err(refused);
        }
    }
    Ok(if linked {
        format!("{{ #[cfg(not(target_arch = \"wasm32\"))] ibex2_sqlite::link(); {constructor} }}")
    } else {
        constructor.to_owned()
    })
}

/// The build script's own crate, when the entry it generates calls `ibex2_sqlite::link()` and the crate depends on a
/// native host but names no `ibex2-sqlite`: an app crate written before native apps selected SQLite (2099b23460)
/// declares only its host. rustc would refuse the entry with E0433 and Cargo's hint to `cargo add` a crates.io crate,
/// where the fix is the SDK's vendored one (expo/exact-build#147). Only from a build script (`CARGO_CFG_TARGET_ARCH`
/// is a build script's) for a native target. A crate without a host (a render crate) writes the entry but never
/// compiles it, so it is left alone. A host is found where `exact.mjs update` looks for one
/// (`game/update-manifests.mjs`, `nativeHosts`), so the update the message names adds the line.
fn shell_without_sqlite() -> Option<String> {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").ok()?;
    if arch == "wasm32" {
        return None;
    }
    let manifest =
        std::path::Path::new(&std::env::var_os("CARGO_MANIFEST_DIR")?).join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).ok()?;
    lacks_sqlite(&text).then(|| sqlite_refusal(&manifest))
}

/// The host crates `exact.mjs update` adds `ibex2-sqlite` beside (`game/update-manifests.mjs`).
const NATIVE_HOSTS: [&str; 6] = [
    "exact-apple",
    "exact-apple-update",
    "exact-linux",
    "exact-painted",
    "exact-linux-update",
    "exact-windows",
];

/// Whether a manifest declares a native host as a dependency and names no `ibex2-sqlite`. A host counts only as a
/// key or `package` in `[dependencies]` or a `[target.….dependencies]` table, the tables `exact.mjs update` edits.
/// SQLite counts anywhere outside a comment, as `ibex2-sqlite` or as the `ibex2_sqlite` the entry names (a renamed or
/// inherited key), so a crate that may have it is left to Cargo. The scan reads one line at a time, so a manifest
/// with a multi-line string, whose lines it cannot tell from declarations, or an escape (`\\`), which may spell a key
/// it does not decode, is left to Cargo too.
fn lacks_sqlite(manifest: &str) -> bool {
    if manifest.contains("\"\"\"") || manifest.contains("'''") || manifest.contains('\\') {
        return false;
    }
    let mut host = false;
    let mut sqlite = false;
    let mut table = None::<bool>; // Some(true): a dependency table; Some(false): one dependency's own table.
    for line in manifest.lines().map(uncommented) {
        sqlite |= line
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .any(|w| w.replace('-', "_") == "ibex2_sqlite");
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_matches(|c| c == '[' || c == ']' || c == ' ');
            let (path, name) = match header.rsplit_once("dependencies") {
                Some((before, after))
                    if before.is_empty()
                        || before.starts_with("target.") && before.ends_with('.') =>
                {
                    (true, after.strip_prefix('.'))
                }
                _ => (false, None),
            };
            table = path.then_some(name.is_none());
            host |= name.is_some_and(is_host);
        } else if let Some((key, value)) = line.split_once('=') {
            let key = key
                .trim()
                .split('.')
                .next()
                .unwrap_or("")
                .trim_matches(|c| c == '"' || c == '\'');
            host |= match table {
                Some(true) => is_host(key) || package(value).is_some_and(is_host),
                Some(false) => {
                    key == "package"
                        && is_host(value.trim().trim_matches(|c| c == '"' || c == '\''))
                }
                None => false,
            };
        }
    }
    host && !sqlite
}

fn is_host(name: &str) -> bool {
    NATIVE_HOSTS.contains(&name)
}

/// The `package = "…"` an inline dependency table renames.
fn package(value: &str) -> Option<&str> {
    let (_, rest) = value.split_once("package")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    rest[1..].split(quote).next()
}

/// A manifest line without its comment: from the first `#` outside a quoted string.
fn uncommented(line: &str) -> &str {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '#') => return line[..i].trim(),
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            _ => {}
        }
    }
    line.trim()
}

fn sqlite_refusal(manifest: &std::path::Path) -> String {
    let vendored = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/ibex/crates/ibex2-sqlite");
    let vendored = vendored.canonicalize().unwrap_or(vendored);
    format!(
        "{}: this native app crate's entry links SQLite storage (ibex2_sqlite::link) but the crate does not depend on ibex2-sqlite. \
         Run `bun exact.mjs update` in the app, which adds it beside the host crate, or add under [dependencies]: \
         ibex2-sqlite = {{ path = '{}' }}. Not `cargo add`: the crate is Exact's vendored copy.",
        manifest.display(),
        vendored.display()
    )
}

/// Refuse an entry whose source is not the one app.json's `rust.data` names (LLP 1153.003 D2), as the compatibility
/// inputs carry it (`rustData`): the web build runs the manifest's, so a build script that passed another would run
/// one source natively and another on the web. A source the host storage wraps (`exact_data_host::Storage`) is read
/// inside it. An entry over a source the crate composes itself (`ExactEmbeddedData`, `ExactEmptyData`) names no
/// constructor of the data crate's; the mixed bake checks that one's type.
fn declared_source(
    data: &str,
    constructor: &str,
    inputs: &serde_json::Value,
) -> Result<(), String> {
    let declared = &inputs["rustData"];
    let (Some(ty), Some(source)) = (declared["type"].as_str(), declared["source"].as_str()) else {
        return Ok(());
    };
    if matches!(data, "ExactEmbeddedData" | "ExactEmptyData") {
        return Ok(());
    }
    let compact = |text: &str| text.split_whitespace().collect::<String>();
    let (data, constructor) = (compact(data), compact(constructor));
    let inner = |text: &str, open: &str, close: char| {
        text.strip_prefix(open)
            .and_then(|rest| rest.strip_suffix(close))
            .map(str::to_owned)
    };
    let data = inner(&data, "exact_data_host::Storage<", '>').unwrap_or(data);
    let constructor =
        inner(&constructor, "exact_data_host::Storage::new(", ')').unwrap_or(constructor);
    if data == compact(ty) && constructor == compact(source) {
        return Ok(());
    }
    // A native host crate the app wrote may run another source of the same crate (markdown-stress's Apple host runs
    // its native document lane); the web's entry runs the module the manifest names, so it must name the same.
    let library = |path: &str| path.split("::").next().map(str::to_owned);
    if inputs["platform"] != "web" && library(&data) == library(ty) {
        return Ok(());
    }
    Err(format!(
        "app.json's rust.data names the source `{source}` (`{ty}`), and this build passes `{constructor}` (`{data}`): pass the source the manifest names, which the web build also runs"
    ))
}

/// A Linux app's whole `entry.rs`: [`rust_entry`], its `launch_parts()`, its
/// `hatches` and a `main` that runs the parts before `host` (`exact_linux` or
/// `exact_linux_update`) starts with `run` (both from
/// `native::rust_hatch_entry`). Run from the app's Linux build script.
pub fn linux_entry(
    data: &str,
    constructor: &str,
    inputs: &serde_json::Value,
    host: &str,
    hatches: &str,
    run: &str,
) -> Result<String, String> {
    Ok(format!(
        "{}\n{}\n{hatches}fn main() {{ launch_parts(); std::process::exit({host}::{run}); }}\n",
        rust_entry(
            data,
            constructor,
            inputs["rustMode"].as_str().ok_or("missing Rust policy")?,
            inputs
        )?,
        {
            let crate_dir = std::env::var_os("CARGO_MANIFEST_DIR").ok_or("not run by cargo")?;
            let app_dir = std::path::Path::new(&crate_dir).join("..");
            let manifest = crate::Manifest::read(&app_dir)?;
            painted_launch_parts(&app_dir, &manifest, "linux")?
        }
    ))
}

/// Each launch module's part for `platform`, called with its configuration
/// before the painted host starts. An app-local module shadows the SDK's whole module.
pub(crate) fn painted_launch_parts(
    app_dir: &std::path::Path,
    manifest: &crate::Manifest,
    platform: &str,
) -> Result<String, String> {
    let shared = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../modules");
    let app = serde_json::json!({
        "id": manifest.id,
        "name": manifest.name,
        "version": manifest.json["version"],
    })
    .to_string();
    let (mut mods, mut calls) = (String::new(), String::new());
    let compat = if platform == "android" {
        "exact_compat()"
    } else {
        "COMPAT"
    };
    let names = match manifest.json.get("launch") {
        Some(value) => value
            .as_array()
            .ok_or("app.json launch: expected a list of module names")?
            .as_slice(),
        None => &[],
    };
    for name in names {
        let name = name
            .as_str()
            .ok_or("app.json launch: a module name is a string")?;
        // As `scripts/app.schema.json`: `^[a-z][a-z0-9]*$`.
        if !name.starts_with(|c: char| c.is_ascii_lowercase())
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("app.json launch: invalid module name {name:?}"));
        }
        let own = app_dir.join("modules").join(name);
        let base = if own.exists() { own } else { shared.join(name) };
        if !base.exists() {
            return Err(format!(
                "app.json launch: no module {name} (neither the app nor Exact provides it)"
            ));
        }
        let file = base.join(platform).join("launch.rs");
        if !file.is_file() {
            return Err(format!(
                "app.json launch: {name} has no {platform} launch part ({}); provide it or remove {name} from launch",
                file.display()
            ));
        }
        println!("cargo:rerun-if-changed={}", file.display());
        let config = manifest.json["moduleConfig"]
            .get(name)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}))
            .to_string();
        let ident = format!("launch_{name}");
        mods.push_str(&format!(
            "#[allow(missing_docs, dead_code)]\nmod {ident} {{ include!({:?}); }}\n",
            file.canonicalize().map_err(|e| e.to_string())?
        ));
        calls.push_str(&format!(
            "    {ident}::launch(exact_painted::journal::LaunchContext::new({name:?}, {config:?}, {app:?}, {compat}));\n"
        ));
    }
    if calls.is_empty() {
        return Ok("fn launch_parts() {}\n".into());
    }
    Ok(format!(
        "{mods}fn launch_parts() {{\n    let started = std::time::Instant::now();\n{calls}    exact_painted::journal::launch_parts_ran(started.elapsed());\n}}\n"
    ))
}

/// The Rust executor a web entry links (LLP 1047 D3): the compatibility
/// inputs' `rustMode`, or `off` when the manifest names no `rust.module`. A
/// browser then has no Rust module to swap in (the dev loop builds one only
/// for a declared module), so it links no executor.
pub fn web_rust_mode(inputs: &serde_json::Value) -> &str {
    let module = inputs["rustModule"]
        .as_str()
        .is_some_and(|module| !module.is_empty());
    match inputs["rustMode"].as_str() {
        Some(mode) if module => mode,
        _ => "off",
    }
}

/// The wide colour functions (LLP 1056 §8.2).
const WIDE_COLORS: [&str; 5] = ["lab(", "lch(", "oklab(", "oklch(", "color("];

/// Whether the app's Rust data crate (`../data` beside the web crate that
/// builds this) names a wide colour function in its source: Canvas 2D's
/// `lab()`, `lch()`, `oklab()`, `oklch()` or `color()` (LLP 1056 §8.2).
/// TypeScript draws parse colours in their own recorder, not the wasm.
fn names_wide_colors() -> bool {
    let Some(dir) = std::env::var_os("CARGO_MANIFEST_DIR") else {
        return false;
    };
    let data = std::path::Path::new(&dir).join("../data");
    // A missing path would rerun the build script on every build (Cargo
    // counts it as changed); an app without a data crate names none.
    if !data.exists() {
        return false;
    }
    println!("cargo:rerun-if-changed={}", data.display());
    fn scan(dir: &std::path::Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|e| {
            let p = e.path();
            if p.is_dir() {
                return scan(&p);
            }
            p.extension().is_some_and(|x| x == "rs")
                && std::fs::read_to_string(&p).is_ok_and(|s| {
                    // Inside string literals only: the odd pieces between
                    // quotes on a line.
                    s.lines().any(|line| {
                        line.split('"')
                            .skip(1)
                            .step_by(2)
                            .any(|lit| WIDE_COLORS.iter().any(|f| lit.contains(f)))
                    })
                })
        })
    }
    scan(&data)
}

/// Whether the bake's grant ceiling (every source's grants, LLP 1027.001)
/// names `capability`: a device capability its sources may use (LLP 1069),
/// linked by that grant, since no plan row says so.
fn grants(inputs: &serde_json::Value, capability: &str) -> bool {
    inputs["grantCeiling"].as_str().is_some_and(|ceiling| {
        ceiling
            .lines()
            .any(|line| line.split_whitespace().next() == Some(capability))
    })
}

/// The web entry's `EXACT_LINKED` (LLP 1047 D3): the capabilities `plan`
/// uses, each registered from `exact-web-capabilities`, so the linker drops
/// the rest. A development build links every capability (D7): the dev loop
/// restarts from new plans without rebuilding the wasm, so `host/web/dev.mjs`
/// sets `EXACT_WEB_LINK=all`.
///
/// Beside it, `EXACT_REPLACEMENT`: whether a running page can take a new data
/// module. The dev loop's can, and so can a production client whose
/// compatibility `inputs` name a Rust module replaced in the browser (LLP
/// 1029.000); every other build refuses `exact_boot_module` by name.
pub fn web_linked(plan: &exact_plan::Plan, inputs: &serde_json::Value) -> String {
    use exact_runner::{Capability, Uses};
    println!("cargo:rerun-if-env-changed=EXACT_WEB_LINK");
    let all = std::env::var_os("EXACT_WEB_LINK").is_some_and(|v| v == "all");
    let uses = if all {
        Capability::ALL.into_iter().fold(Uses::NONE, Uses::with)
    } else {
        exact_runner::uses(plan)
    };
    let replacement = all
        || (inputs["rustMode"] == "browser"
            && inputs["rustModule"]
                .as_str()
                .is_some_and(|module| !module.is_empty()));
    // Inspection is linked by policy, not by use: in production too, so the
    // smoked artifact is the shipped one (LLP 1047 §10, Q3).
    // A grouped list is its authored nodes on the web, and the browser does
    // its own I/O and plays CSS's transitions (LLP 1047.001 D2); its file
    // input is the media picker, `auth` below is the sign-in sheet, and its
    // numerals roll in the runtime's text (LLP 1077 D15), its date and
    // time controls are the browser's own (LLP 1115.001.000 D2), and it
    // shows no tips (LLP 1035.006.000 D4); a drag out of the page is the
    // browser's own, its data in the page's glue (LLP 1094.000 D6). The
    // native roster's entries are never the web's.
    let names: Vec<&str> = uses
        .iter()
        .filter(|c| !Uses::NATIVE.has(*c))
        .filter(|c| {
            !matches!(
                c,
                Capability::GroupedLists
                    | Capability::Io
                    | Capability::Transitions
                    | Capability::MediaPicker
                    | Capability::AuthSession
                    | Capability::Presence
                    | Capability::Numerals
                    | Capability::Dates
                    | Capability::Sqlite
                    | Capability::Tips
                    | Capability::DragOut
            )
        })
        .map(|c| c.name())
        .chain(["inspection"])
        // A colour row's text, literal or a template's piece, names one in
        // the plan's strings; a drawing names one in the data crate.
        .chain(
            (all || plan
                .strings
                .iter()
                .any(|s| WIDE_COLORS.iter().any(|f| s.contains(f)))
                || names_wide_colors())
            .then_some("wide_colors"),
        )
        .chain((all || grants(inputs, "auth.session")).then_some("auth"))
        .collect();
    let mut entry = format!("/// What this artifact links beyond the core (LLP 1047 D3).\nconst EXACT_LINKED: ::exact_web::Linked = ::exact_web_capabilities::linked!({});\n", names.join(", "));
    entry.push_str(&format!("/// Whether a running page can take a new data module (LLP 1029.000).\nconst EXACT_REPLACEMENT: bool = {replacement};\n"));
    // A capability's export group, where it has one.
    for capability in uses.iter() {
        match capability {
            Capability::Motion => entry.push_str("::exact_web::motion_exports!();\n"),
            Capability::Collections => entry.push_str("::exact_web::list_exports!();\n"),
            Capability::Surfaces => entry.push_str("::exact_web::surface_exports!();\n"),
            // Drag's input rides motion's export; the others have none.
            Capability::Markdown
            | Capability::Drag
            | Capability::Router
            | Capability::Format
            | Capability::Materials
            | Capability::Backdrop
            | Capability::Share
            | Capability::Documents
            | Capability::Picker
            | Capability::Timelines
            | Capability::TextTransform
            | Capability::Effects
            | Capability::Animations
            | Capability::Gradients
            | Capability::Grid
            | Capability::Shadows
            | Capability::Corners
            | Capability::Rotate
            | Capability::TextStroke
            | Capability::SymbolPalette
            | Capability::Geometry
            | Capability::Segments
            | Capability::Dataset
            | Capability::Tabs
            | Capability::GroupedLists
            | Capability::Io
            | Capability::Transitions
            | Capability::MediaPicker
            | Capability::AuthSession
            | Capability::Presence
            | Capability::Numerals
            | Capability::Dates
            | Capability::Sqlite
            | Capability::Tips
            | Capability::Modules
            | Capability::Tools
            | Capability::Svg
            | Capability::Images
            | Capability::NativeViews
            | Capability::ContentRegions
            | Capability::Traces
            | Capability::Controls
            | Capability::Flights
            | Capability::Sticky
            | Capability::Notifications
            | Capability::HostEvents
            | Capability::Activities
            | Capability::Wakes
            | Capability::WakeLock
            | Capability::Video
            | Capability::WebViews
            | Capability::Sound
            | Capability::Midi
            | Capability::Navigation
            | Capability::TextSelection
            | Capability::Widgets
            | Capability::ContentIndex
            | Capability::Dialogs
            | Capability::DragOut
            | Capability::DragIn => {}
        }
    }
    entry
}

/// The Apple entry's `EXACT_LINKED` (LLP 1047.001 D2, D3), which the entry
/// passes as `host!(…; linked = EXACT_LINKED)`, and the export groups of what
/// it names. `host` is the crate whose `host!` the entry calls
/// (`exact_apple`, or `exact_apple_update` above it). Which capabilities an
/// archive links is `exact_bake::apple_link`'s to say.
pub fn apple_linked(uses: exact_runner::Uses, host: &str) -> String {
    use exact_runner::Capability;
    let apple = if host == "exact_apple" {
        "::exact_apple".to_owned()
    } else {
        format!("::{host}::exact_apple")
    };
    let link = format!("{apple}::link");
    let mut set = format!("{link}::Uses::NONE");
    for capability in uses.iter() {
        set.push_str(&format!(".with({link}::Capability::{capability:?})"));
    }
    let mut entry = format!("/// What this archive links beyond the core (LLP 1047.001 D2).\nconst EXACT_LINKED: {link}::Uses = {set};\n");
    // A capability's export group, where it has one on Apple.
    for (capability, group) in [
        (Capability::GroupedLists, "grouped_list_exports"),
        (Capability::Markdown, "markup_exports"),
        (Capability::Presence, "presence_exports"),
        (Capability::Svg, "svg_exports"),
        (Capability::ContentRegions, "content_region_exports"),
        (Capability::Surfaces, "canvas_exports"),
    ] {
        if uses.has(capability) {
            entry.push_str(&format!("{apple}::{group}!();\n"));
        }
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::web_rust_mode;

    #[test]
    fn native_entries_link_sqlite_by_composition_and_refuse_missing_sql_storage() {
        let mut inputs = serde_json::json!({
            "rustMode": "off", "linked": ["io"],
            "grantCeiling": "net.fetch https://example.test"
        });
        let entry = super::rust_entry("Data", "data()", "off", &inputs).unwrap();
        assert!(!entry.contains("sqlite"), "{entry}");
        inputs["grantCeiling"] = "sqlite.open app:/data/notes.db".into();
        let refused = super::rust_entry("Data", "data()", "off", &inputs).unwrap_err();
        assert_eq!(refused, "Unlinked(\"sqlite\")");
        eprintln!("entry generation: {refused}");
        inputs["linked"] = serde_json::json!(["io", "sqlite"]);
        let entry = super::rust_entry("Data", "data()", "off", &inputs).unwrap();
        assert!(entry.contains("ibex2_sqlite::link();"), "{entry}");
        inputs.as_object_mut().unwrap().remove("linked");
        assert!(super::rust_entry("Data", "data()", "off", &inputs)
            .unwrap()
            .contains("ibex2_sqlite::link();"));
    }

    /// LLP 1027.002: a Rust-only app's entry places its source as `rust.placement` says, natively; `main`, a mixed
    /// app's entry (`rust_entry`) and the web are today's entry.
    #[test]
    fn a_rust_only_entry_places_its_source_as_the_manifest_says() {
        let inputs = |platform: &str, placement: &str| serde_json::json!({ "platform": platform, "rustMode": "off", "rustPlacement": placement, "app": "com.example.placed" });
        let worker = super::rust_entry("Data", "data()", "off", &inputs("ios", "worker")).unwrap();
        assert!(
            worker.contains(
                "exact_logic::configured!(AppData, exact_logic::exact_data::Placed<Data>,"
            ),
            "{worker}"
        );
        // The constructor the entry writes (here with its native storage link) is the one placed.
        assert!(
            worker.contains("exact_logic::exact_data::Placed::new({ #[cfg(not(target_arch = \"wasm32\"))] ibex2_sqlite::link(); data() }, exact_logic::exact_runner::Placement::Worker)"),
            "{worker}"
        );
        for (platform, placement) in [("ios", "main"), ("macos", "main"), ("web", "worker")] {
            let inputs = inputs(platform, placement);
            assert_eq!(
                super::rust_entry("Data", "data()", "off", &inputs).unwrap(),
                super::rust_entry_composed("Data", "data()", "off", &inputs, None).unwrap(),
                "{platform} {placement}"
            );
        }
        // Rust updates run their module inline: worker is refused there, not dropped.
        let tiered =
            super::rust_entry("Data", "data()", "tiered", &inputs("ios", "worker")).unwrap_err();
        assert!(
            tiered.contains("rust.placement worker with Rust updates"),
            "{tiered}"
        );
        // A mixed app's composer places its own children: its entry is never wrapped.
        assert!(!super::rust_entry_composed(
            "Data",
            "data()",
            "off",
            &inputs("ios", "worker"),
            None
        )
        .unwrap()
        .contains("Placed"));
    }

    /// LLP 1153.003 D2: a build script passes the source app.json's `rust.data` names, which the web build runs.
    #[test]
    fn an_entry_over_another_source_than_the_manifests_is_refused() {
        let inputs = serde_json::json!({
            "platform": "web", "rustMode": "off",
            "rustData": {"crate": "pins-data", "type": "pins_data::Pins", "source": "pins_data::Pins::new(3)"}
        });
        let entry = |data, constructor| super::rust_entry(data, constructor, "off", &inputs);
        assert!(entry("pins_data::Pins", "pins_data::Pins::new(3)").is_ok());
        assert!(entry(
            "exact_data_host::Storage<pins_data::Pins>",
            "exact_data_host::Storage::new(pins_data::Pins::new( 3 ))"
        )
        .is_ok());
        let refused = entry("pins_data::Pins", "pins_data::Pins::default()").unwrap_err();
        assert!(
            refused.contains("rust.data names the source `pins_data::Pins::new(3)`"),
            "{refused}"
        );
        assert!(entry("other_data::Other", "pins_data::Pins::new(3)").is_err());
        // A source the crate composes itself is the mixed bake's to check.
        assert!(entry("ExactEmbeddedData", "embedded_data()").is_ok());
        // A native host may run another source of the manifest's crate, never another crate's.
        let native = serde_json::json!({ "platform": "macos", "rustMode": "off", "rustData": inputs["rustData"] });
        let entry = |data, constructor| super::rust_entry(data, constructor, "off", &native);
        assert!(entry("pins_data::NativePins", "pins_data::NativePins::default()").is_ok());
        assert!(entry("other_data::Other", "other_data::Other::default()").is_err());
    }

    #[test]
    fn a_native_crate_without_ibex2_sqlite_is_named_with_the_fix() {
        // expo/exact-build#147: an outside app's apple crate from before SQLite was selected.
        let before = "[package]\nname = \"notes-apple\"\n\n[dependencies]\n# ibex2-sqlite comes later\nexact-apple = { path = \"../exact2/host/apple\" }\n";
        assert!(super::lacks_sqlite(before));
        // A render crate writes a native entry it never compiles (apps/expose/linux), and a renamed host still counts.
        let render = "[dependencies]\nexact-js = { path = \"../../../js\" }\nexact-render = { path = \"../../../host/render\" }\n";
        assert!(!super::lacks_sqlite(render));
        assert!(super::lacks_sqlite(
            "[dependencies]\nhost = { package = \"exact-linux\", path = \"x\" }\n"
        ));
        assert!(!super::lacks_sqlite(
            "[dependencies]\n# exact-apple = { path = \"x\" }\n"
        ));
        // A host only in a comment, another field or a table `exact.mjs update` does not edit is no host.
        for not_host in [
            "[package]\ndescription = \"beside exact-apple\"\n[dependencies]\nexact-render = { path = \"x\" } # with exact-apple\n",
            "[dev-dependencies]\nexact-apple = { path = \"x\" }\n",
            "[build-dependencies]\nexact-linux = { path = \"x\" }\n",
            "[package.metadata]\nexample = \"\"\"\n[dependencies.exact-apple]\n\"\"\"\n",
            // TOML decodes this key to `ibex2-sqlite`.
            "[dependencies]\nexact-apple = { path = \"x\" }\n\"ibex2\\u002dsqlite\".workspace = true\n",
        ] {
            assert!(!super::lacks_sqlite(not_host), "{not_host}");
        }
        for host in [
            "[dependencies.exact-apple]\npath = \"x\"\n",
            "[target.'cfg(target_os = \"ios\")'.dependencies]\nexact-apple.workspace = true\n",
            "[dependencies.host]\npackage = \"exact-windows\"\npath = \"x\"\n",
        ] {
            assert!(super::lacks_sqlite(host), "{host}");
        }
        for named in [
            "ibex2-sqlite = { path = \"../exact2/vendor/ibex/crates/ibex2-sqlite\" }",
            "ibex2-sqlite.workspace = true",
            "[dependencies.ibex2-sqlite]",
            "sql = { package = \"ibex2-sqlite\", path = \"x\" }",
            // The workspace root renames it to the name the entry uses.
            "ibex2_sqlite.workspace = true",
        ] {
            assert!(
                !super::lacks_sqlite(&format!("{before}{named}\n")),
                "{named}"
            );
        }
        let refused = super::sqlite_refusal(std::path::Path::new("/app/apple/Cargo.toml"));
        assert!(refused.starts_with("/app/apple/Cargo.toml: "), "{refused}");
        assert!(
            refused.contains("bun exact.mjs update")
                && refused.contains("vendor/ibex/crates/ibex2-sqlite'"),
            "{refused}"
        );
        // A test is no build script: the entry is unchanged there.
        assert!(
            super::shell_without_sqlite().is_none()
                || std::env::var_os("CARGO_CFG_TARGET_ARCH").is_some()
        );
    }

    /// The entry gives a data source the manifest's app id, which a source
    /// that names none takes (`Swappable::named`), in every native mode.
    #[test]
    fn an_entry_names_the_manifests_app() {
        let inputs = serde_json::json!({"app": "com.example.notes", "linked": []});
        for mode in ["off", "native", "tiered", "wasm"] {
            let entry = super::rust_entry("Data", "data()", mode, &inputs).unwrap();
            let named = format!("Swappable::{mode}(data()).named(\"com.example.notes\")");
            assert!(entry.contains(&named), "{entry}");
            let linked =
                super::rust_entry_naming("Data", "data()", mode, &inputs, Some("linked_app()"))
                    .unwrap();
            assert!(
                linked.contains(&format!("Swappable::{mode}(data()).named(linked_app())")),
                "{linked}"
            );
            assert!(!linked.contains("com.example.notes"), "{linked}");
            let unnamed = super::rust_entry_naming("Data", "data()", mode, &inputs, None).unwrap();
            assert!(!unnamed.contains(".named("), "{unnamed}");
        }
        // Under a variant (LLP 1030.008 D2) a source naming the base app takes the variant's id.
        let variant = serde_json::json!({"app": "com.example.notes.dev", "linked": [], "variant": {"name": "dev", "base": "com.example.notes"}});
        let entry = super::rust_entry("Data", "data()", "off", &variant).unwrap();
        assert!(
            entry.contains(".named_variant(\"com.example.notes.dev\", \"com.example.notes\")"),
            "{entry}"
        );
    }

    /// LLP 1047.001 D3: the entry names its set and invokes the export
    /// groups of what it names, through whichever host crate it calls.
    #[test]
    fn an_apple_entry_names_its_set_and_its_groups() {
        use exact_runner::{Capability, Uses};
        let grouped = Uses::NONE.with(Capability::GroupedLists);
        let entry = super::apple_linked(grouped, "exact_apple");
        assert!(
            entry.contains("::exact_apple::link::Capability::GroupedLists"),
            "{entry}"
        );
        assert!(
            entry.contains("::exact_apple::grouped_list_exports!();"),
            "{entry}"
        );
        let update = super::apple_linked(grouped, "exact_apple_update");
        assert!(
            update.contains("::exact_apple_update::exact_apple::grouped_list_exports!();"),
            "{update}"
        );
        assert!(!super::apple_linked(Uses::NONE, "exact_apple").contains("exports!"));
        let svg = super::apple_linked(Uses::NONE.with(Capability::Svg), "exact_apple");
        assert!(svg.contains("::exact_apple::svg_exports!();"), "{svg}");
        let regions =
            super::apple_linked(Uses::NONE.with(Capability::ContentRegions), "exact_apple");
        assert!(
            regions.contains("::exact_apple::content_region_exports!();"),
            "{regions}"
        );
        let canvas = super::apple_linked(Uses::NONE.with(Capability::Surfaces), "exact_apple");
        assert!(
            canvas.contains("::exact_apple::canvas_exports!();"),
            "{canvas}"
        );
        let markdown = super::apple_linked(Uses::NONE.with(Capability::Markdown), "exact_apple");
        assert!(
            markdown.contains("::exact_apple::markup_exports!();"),
            "{markdown}"
        );
        let presence = Uses::NONE.with_grants("secret.keep token user-presence optional");
        for host in ["exact_apple", "exact_apple_update"] {
            let entry = super::apple_linked(presence, host);
            assert!(entry.contains("::presence_exports!();"), "{entry}");
            assert!(entry.contains("::Capability::Presence"), "{entry}");
        }
    }

    #[test]
    fn a_web_entry_links_a_rust_executor_only_for_a_declared_module() {
        let module = serde_json::json!({"rustMode": "browser", "rustModule": "app-logic"});
        assert_eq!(web_rust_mode(&module), "browser");
        let none = serde_json::json!({"rustMode": "browser", "rustModule": null});
        assert_eq!(web_rust_mode(&none), "off");
        let off = serde_json::json!({"rustMode": "off", "rustModule": "app-logic"});
        assert_eq!(web_rust_mode(&off), "off");
    }
}

#[cfg(test)]
mod launch_tests {
    use crate as contract;
    use serde_json::json;
    use std::path::PathBuf;

    struct App(PathBuf);

    impl App {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("launch-parts-{}-{n}", std::process::id()));
            std::fs::create_dir_all(dir.join("out")).unwrap();
            Self(dir)
        }

        fn generate(&self, launch: serde_json::Value) -> Result<String, String> {
            let manifest = contract::Manifest {
                json: json!({"launch": launch, "moduleConfig": {"observe": {"projectId": "local"}}}),
                id: "test.launch".into(),
                name: "Launch".into(),
                declared: true,
            };
            let plan = contract::compile("component App\n  view\n    text \"Launch\"\n").unwrap();
            let out = self.0.join("out");
            contract::native::android_entry(&self.0, &manifest, &plan, &out)?;
            std::fs::read_to_string(out.join("launch.rs")).map_err(|e| e.to_string())
        }
    }

    impl Drop for App {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn android_embeds_the_launch_list_and_refuses_missing_parts() {
        let app = App::new();
        let entry = app.generate(json!(["observe"])).unwrap();
        assert!(entry.contains("/observe/android/launch.rs"), "{entry}");
        assert!(entry.contains("launch_observe::launch("), "{entry}");
        assert!(entry.contains(", exact_compat()));"), "{entry}");
        assert!(entry.contains("projectId\\\":\\\"local"), "{entry}");
        assert!(entry.contains("test.launch"), "{entry}");
        let second = app.0.join("modules/second/android");
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(second.join("launch.rs"), "pub fn launch() {}").unwrap();
        let entry = app.generate(json!(["second", "observe"])).unwrap();
        assert!(
            entry.find("launch_second::launch(").unwrap()
                < entry.find("launch_observe::launch(").unwrap()
        );
        let own = app.0.join("modules/observe/linux");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("launch.rs"), "pub fn launch() {}").unwrap();
        assert!(app
            .generate(json!(["observe"]))
            .unwrap_err()
            .contains("observe has no android launch part"));
        assert!(app
            .generate(json!(["missing"]))
            .unwrap_err()
            .contains("no module missing"));
        assert!(app
            .generate(json!([]))
            .unwrap()
            .contains("fn launch_parts() {}"));
        assert!(app
            .generate(json!(["../observe"]))
            .unwrap_err()
            .contains("invalid module name"));
        assert!(app
            .generate(json!("observe"))
            .unwrap_err()
            .contains("expected a list"));
        let manifest = contract::Manifest {
            json: json!({"launch": ["observe"]}),
            id: "test.launch".into(),
            name: "Launch".into(),
            declared: true,
        };
        let linux = super::painted_launch_parts(&app.0, &manifest, "linux").unwrap();
        assert!(linux.contains(", COMPAT));"), "{linux}");
        assert!(linux.contains("/modules/observe/linux/launch.rs"));
        std::fs::remove_dir_all(own).unwrap();
        assert!(super::painted_launch_parts(&app.0, &manifest, "linux")
            .unwrap_err()
            .contains("observe has no linux launch part"));
    }
}
