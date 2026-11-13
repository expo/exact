//! Concrete app composition selected at bake, never a Cargo feature on a host.
//! @ref LLP 1029.000 — disabling replacement removes its reachable loader code.

/// Generate `AppData` and `app_data()` for a host's source type and constructor.
/// `inputs` are the actual target bake's compatibility inputs.
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
    let constructor = native_storage_constructor(constructor, inputs)?;
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
    let named = app.map(|app| format!(".named({app})")).unwrap_or_default();
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
        launch_parts()?
    ))
}

/// Each `app.json` `launch` module with a `linux/launch.rs` (the app's
/// `modules/` first, then exact2's) becomes a module of the executable,
/// called with its `moduleConfig` before the host starts.
fn launch_parts() -> Result<String, String> {
    let crate_dir = std::env::var_os("CARGO_MANIFEST_DIR").ok_or("not run by cargo")?;
    let app_dir = std::path::Path::new(&crate_dir).join("..");
    let manifest = crate::Manifest::read(&app_dir)?;
    let shared = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../modules");
    let app = serde_json::json!({
        "id": manifest.id,
        "name": manifest.name,
        "version": manifest.json["version"],
    })
    .to_string();
    let (mut mods, mut calls) = (String::new(), String::new());
    for name in manifest.json["launch"].as_array().into_iter().flatten() {
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
        let Some(file) = [app_dir.join("modules"), shared.clone()]
            .iter()
            .map(|base| base.join(name).join("linux/launch.rs"))
            .find(|f| f.exists())
        else {
            continue;
        };
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
            "    {ident}::launch(exact_painted::journal::LaunchContext::new({name:?}, {config:?}, {app:?}, COMPAT));\n"
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
    // shows no tips (LLP 1035.006.000 D4).
    let names: Vec<&str> = uses
        .iter()
        .filter(|c| {
            !matches!(
                c,
                Capability::GroupedLists
                    | Capability::Io
                    | Capability::Transitions
                    | Capability::MediaPicker
                    | Capability::AuthSession
                    | Capability::Numerals
                    | Capability::Dates
                    | Capability::Sqlite
                    | Capability::Tips
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
            | Capability::Geometry
            | Capability::Segments
            | Capability::Dataset
            | Capability::Tabs
            | Capability::GroupedLists
            | Capability::Io
            | Capability::Transitions
            | Capability::MediaPicker
            | Capability::AuthSession
            | Capability::Numerals
            | Capability::Dates
            | Capability::Sqlite
            | Capability::Tips => {}
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
        let markdown = super::apple_linked(Uses::NONE.with(Capability::Markdown), "exact_apple");
        assert!(
            markdown.contains("::exact_apple::markup_exports!();"),
            "{markdown}"
        );
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
