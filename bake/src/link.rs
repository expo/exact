//! What a native artifact links (LLP 1047.001 D2, D3; LLP 1047 D8).
//!
//! @ref LLP 1047 D8 (on native the linked set is a compatibility input; the
//! manifest may link capabilities ahead of any plan that uses them)
//! @ref LLP 1047.001 D2 (one composition drives Rust, Swift, admission and
//! compatibility)

use crate::compat::{compatibility_digest, Compat};
use exact_runner::{Capability, Uses};

/// Name what a native artifact links into `compat`'s inputs (`linked`), from
/// the plan its build script baked into `out`, before the id is taken and
/// everything that binds it (the GPU module's card, the receipts) is written.
///
/// Production links what the plan uses, what its grants allow, and the
/// manifest's `link` ahead of use; on Apple, whose boot admits a widget's
/// entry plan against the same set, what `entries` use too. Development and
/// unknown grants link every capability (LLP 1047.001 D6). `EXACT_APPLE_LINK`
/// selects `plan` or `all` explicitly, as Apple's build does. An update
/// whose plan needs a capability this set lacks is a binary change
/// ([`apple_requirement`]).
pub(crate) fn name_into(
    compat: &mut Compat,
    app: &std::path::Path,
    manifest: &contract::Manifest,
    out: &std::path::Path,
    entries: &std::collections::BTreeMap<String, contract::snapshot::Entry>,
) -> Result<(), String> {
    println!("cargo:rerun-if-env-changed=EXACT_APPLE_LINK");
    let by_plan = std::env::var("EXACT_APPLE_LINK").map_or_else(
        |_| std::env::var("EXACT_UPDATE_TRUST").is_ok_and(|v| v == "production"),
        |v| v == "plan",
    );
    let ceiling = compat.inputs["grantCeiling"].as_str();
    let uses = if by_plan && ceiling.is_some() {
        let bytes = std::fs::read(out.join("app.plan")).map_err(|e| {
            format!("the app bake must write OUT_DIR/app.plan before compatibility_id: {e}")
        })?;
        let plan = exact_plan::Plan::decode(&bytes)
            .map_err(|e| format!("the baked plan is invalid: {e:?}"))?;
        let entries = entries.values().map(|entry| &entry.plan);
        composed(
            app,
            &plan,
            entries,
            &compat.inputs,
            manifest,
            compat.host_work,
        )?
    } else {
        Capability::ALL.into_iter().fold(Uses::NONE, Uses::with)
    };
    // The native roster is Apple's: another platform's set, and so its id,
    // never names it, whatever a manifest links ahead.
    let uses = if apple(&compat.inputs) {
        uses
    } else {
        uses.beyond(Uses::NATIVE)
    };
    let names: Vec<&str> = uses.iter().map(Capability::name).collect();
    compat.inputs["linked"] = serde_json::json!(names);
    // The roster `linked` is drawn from. A capability added to it later was in
    // this binary's core, and a binary built after has another id: the
    // classifier requires only what the cohort's roster names (LLP 1047.001
    // D13).
    if apple(&compat.inputs) {
        let roster: Vec<&str> = Capability::ALL.iter().map(|c| c.name()).collect();
        compat.inputs["linkRoster"] = serde_json::json!(roster);
    }
    compat.id = compatibility_digest(&compat.inputs);
    Ok(())
}

/// What a production artifact links: [`production_uses`], the manifest's
/// `link` ahead of use, and what those bring with them ([`dependents`]). Computed
/// once for the bake's linked set and for what an update requires.
pub(crate) fn composed<'a>(
    app: &std::path::Path,
    plan: &'a exact_plan::Plan,
    entries: impl Iterator<Item = &'a exact_plan::Plan>,
    inputs: &serde_json::Value,
    manifest: &contract::Manifest,
    host_work: bool,
) -> Result<Uses, String> {
    let uses = production_uses(app, plan, entries, inputs, host_work);
    let uses = linked_ahead(uses, ahead(manifest)?, inputs);
    Ok(dependents(uses, inputs, host_work))
}

/// `uses` with what the manifest links `ahead`; on Apple, an SVG linked
/// ahead links box effects' engine too, which SVG's filters run on.
fn linked_ahead(uses: Uses, ahead: Vec<Capability>, inputs: &serde_json::Value) -> Uses {
    let uses = ahead.into_iter().fold(uses, Uses::with);
    if apple(inputs) && uses.has(Capability::Svg) {
        uses.with(Capability::Effects)
    } else {
        uses
    }
}

/// What a plan requires of an Apple binary it is delivered to (LLP 1047.001
/// D5, D13): the set [`composed`] names for it, which the classifier
/// (`classifyArtifacts`, `scripts/app.mjs`) compares as a subset of the
/// cohort's `linked`. `None` off Apple.
pub(crate) fn apple_requirement<'a>(
    app: &std::path::Path,
    plan: &'a exact_plan::Plan,
    entries: impl Iterator<Item = &'a exact_plan::Plan>,
    inputs: &serde_json::Value,
    manifest: &contract::Manifest,
    host_work: bool,
) -> Result<Option<serde_json::Value>, String> {
    if !apple(inputs) {
        return Ok(None);
    }
    let uses = composed(app, plan, entries, inputs, manifest, host_work)?;
    Ok(Some(serde_json::json!(uses
        .iter()
        .map(Capability::name)
        .collect::<Vec<_>>())))
}

/// What a production artifact links for `plan`, with what its grants allow
/// (`inputs`' ceiling); on Apple, with what the widget `entries` its boot
/// admits use too, what those bring with them ([`dependents`]), and what
/// only the bake can read in `app`, the app's directory: whether a
/// bundled image is an SVG, and whether the app ships a module artifact,
/// which connects the app-level hatches (LLP 1075.003.000).
pub(crate) fn production_uses<'a>(
    app: &std::path::Path,
    plan: &'a exact_plan::Plan,
    entries: impl Iterator<Item = &'a exact_plan::Plan>,
    inputs: &serde_json::Value,
    host_work: bool,
) -> Uses {
    let grants = Uses::NONE.with_grants(inputs["grantCeiling"].as_str().unwrap_or(""));
    if !apple(inputs) {
        return plan_uses(plan, inputs).iter().fold(grants, Uses::with);
    }
    let plans: Vec<&exact_plan::Plan> = std::iter::once(plan).chain(entries).collect();
    let mut uses = plans.iter().fold(grants, |uses, plan| {
        plan_uses(plan, inputs).iter().fold(uses, Uses::with)
    });
    // SVG's filters run on box effects' engine.
    if plans.iter().any(|plan| bundles_svg(app, plan)) {
        uses = uses.with(Capability::Svg).with(Capability::Effects);
    }
    // The app's own module artifact is loaded by the native module family.
    if ships_module(app, inputs["platform"].as_str().unwrap_or("")) {
        uses = uses.with(Capability::NativeViews);
    }
    dependents(uses, inputs, host_work)
}

/// `uses` with what Apple links for what it holds: [`production_uses`]
/// applies it to the plans and grants, and [`composed`] again once
/// link-ahead is in:
/// - the I/O executor when something hands it work: a source that declares it
///   (`host_work`, LLP 1047.001 D12), one placed on a worker, which answers
///   through continuations (LLP 1027.002 D1), or a native module's calls;
/// - the native module family, which loads a module's Swift implementation,
///   with the runner's module seam (`modules`).
fn dependents(uses: Uses, inputs: &serde_json::Value, host_work: bool) -> Uses {
    if !apple(inputs) {
        return uses;
    }
    let placed = ["rustPlacement", "typescriptPlacement"]
        .iter()
        .any(|key| inputs[key] == "worker");
    let mut uses = uses;
    if host_work || placed || uses.has(Capability::Modules) {
        uses = uses.with(Capability::Io);
    }
    if uses.has(Capability::Modules) {
        uses = uses.with(Capability::NativeViews);
    }
    uses
}

/// Whether a bundled image source of `plan` (an asset or a `data:` URL) is
/// an SVG, which the host tells by the bytes; one the bake cannot read counts
/// as one.
fn bundles_svg(app: &std::path::Path, plan: &exact_plan::Plan) -> bool {
    exact_runner::uses::image_sources(plan)
        .iter()
        .filter_map(|source| source.bundled())
        .filter(|name| !name.is_empty())
        .any(|name| {
            let bytes = if let Some(url) = name.strip_prefix("data:") {
                exact_runner::uses::data_url(url)
            } else {
                std::fs::read(app.join(name.trim_start_matches('/'))).ok()
            };
            bytes.is_none_or(|bytes| exact_svg_file::sniff(&bytes))
        })
}

/// Whether the app ships a module artifact on Apple: Swift or a framework
/// in its module directory (`scripts/app.mjs`, `moduleDirectory`).
fn ships_module(app: &std::path::Path, platform: &str) -> bool {
    let own = app.join(platform).join("modules");
    let folder = if own.exists() {
        own
    } else {
        app.join("modules").join("apple")
    };
    std::fs::read_dir(folder).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".swift") || name.ends_with(".xcframework")
        })
    })
}

/// What `plan` uses on the platform `inputs` name: on Apple, which links
/// grid layout by use and the native roster, [`exact_runner::uses::apple_uses`]
/// (LLP 1047.001 D10).
pub(crate) fn plan_uses(plan: &exact_plan::Plan, inputs: &serde_json::Value) -> Uses {
    if apple(inputs) {
        exact_runner::uses::apple_uses(plan)
    } else {
        exact_runner::uses(plan)
    }
}

fn apple(inputs: &serde_json::Value) -> bool {
    matches!(inputs["platform"].as_str(), Some("ios" | "macos"))
}

/// The Apple entry's text for what `compat` says the archive links
/// ([`contract::apple_linked`]); every capability where it says nothing.
pub fn apple_link(compat: &Compat, host: &str) -> String {
    let uses = match compat.inputs["linked"].as_array() {
        Some(names) => names
            .iter()
            .filter_map(|n| n.as_str().and_then(Capability::from_name))
            .fold(Uses::NONE, Uses::with),
        None => Capability::ALL.into_iter().fold(Uses::NONE, Uses::with),
    };
    contract::apple_linked(uses, host)
}

/// The manifest's `link`: capabilities by name, linked before any plan uses
/// them, so a later bundle may (LLP 1047 D8).
pub fn ahead(manifest: &contract::Manifest) -> Result<Vec<Capability>, String> {
    let Some(link) = manifest.json.get("link") else {
        return Ok(Vec::new());
    };
    let names = link
        .as_array()
        .ok_or("app.json: `link` is a list of capability names")?;
    names
        .iter()
        .map(|name| {
            name.as_str()
                .and_then(Capability::from_name)
                .ok_or_else(|| {
                    let known: Vec<&str> = Capability::ALL.iter().map(|c| c.name()).collect();
                    format!("app.json: `link` names {name}, which is not a capability; capabilities: {}", known.join(", "))
                })
        })
        .collect()
}

/// Take the Rust sources of an Apple bake made before Cargo. The build
/// script calls this before baking and returns when it answers true.
/// `files` names generated Rust inputs; a bake must preserve their times
/// when their bytes do not change. Without EXACT_BAKED, bake normally.
///
/// The first Cargo run registers its compiled build-script executable and
/// bootstraps the bake in a child. Later builds run that executable before
/// Cargo, so a Contract edit changes only the artifacts the linker reads.
pub fn take_linked(files: &[&str]) -> Result<bool, String> {
    use sha2::{Digest, Sha256};
    use std::{env, fs, path::PathBuf, process::Command};
    println!("cargo:rerun-if-env-changed=EXACT_BAKED");
    let Some(baked) = env::var_os("EXACT_BAKED").map(PathBuf::from) else {
        return Ok(false);
    };
    if env::var_os("EXACT_RUST_BAKE").is_some() {
        return Ok(false);
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("build requires OUT_DIR")?);
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let variables: std::collections::BTreeMap<_, _> = env::vars()
        .filter(|(key, _)| {
            key.starts_with("CARGO_CFG_")
                || key.starts_with("CARGO_FEATURE_")
                || matches!(
                    key.as_str(),
                    "TARGET" | "HOST" | "PROFILE" | "OPT_LEVEL" | "DEBUG" | "CARGO_MANIFEST_DIR"
                )
        })
        .collect();
    let mut registration = serde_json::json!({
        "executable": executable, "cwd": env::current_dir().map_err(|e| e.to_string())?,
        "env": variables, "sha256": format!("{:x}", Sha256::digest(fs::read(&executable).map_err(|e| e.to_string())?)),
    });
    fs::create_dir_all(&baked).map_err(|e| e.to_string())?;
    let registry = baked.join("rust-baker.json");
    let mut previous: serde_json::Value = fs::read(&registry)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    if let Some(object) = previous.as_object_mut() {
        object.remove("started");
    }
    if previous != registration
        || !baked.join("compat.json").exists()
        || files.iter().any(|file| !baked.join(file).exists())
    {
        registration["started"] = serde_json::json!(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis());
        let result = Command::new(&executable)
            .env("EXACT_RUST_BAKE", "1")
            .env("OUT_DIR", &baked)
            .output()
            .map_err(|e| e.to_string())?;
        eprint!("{}", String::from_utf8_lossy(&result.stderr));
        if !result.status.success() {
            return Err(format!("Rust bake failed: {}", result.status));
        }
        fs::write(baked.join("rust-output.txt"), &result.stdout).map_err(|e| e.to_string())?;
        fs::write(&registry, registration.to_string()).map_err(|e| e.to_string())?;
    }
    for line in fs::read_to_string(baked.join("rust-output.txt"))
        .map_err(|e| e.to_string())?
        .lines()
    {
        if line.starts_with("cargo:warning=") || line.starts_with("cargo::warning=") {
            println!("{line}");
        }
    }
    for file in files {
        let from = baked.join(file);
        println!("cargo:rerun-if-changed={}", from.display());
        let bytes = fs::read(from).map_err(|e| e.to_string())?;
        let to = out.join(file);
        if fs::read(&to).ok().as_deref() != Some(&bytes) {
            fs::write(to, bytes).map_err(|e| e.to_string())?;
        }
    }
    // A previous embedded bake must not be mistaken for this linked bake.
    for file in [
        "app.plan",
        "compat.json",
        "artifacts.json",
        "app.plan.map.json",
        "app.hbc",
    ] {
        match fs::remove_file(out.join(file)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::ahead;
    use exact_runner::Capability;

    fn manifest(json: serde_json::Value) -> contract::Manifest {
        contract::Manifest {
            json,
            id: "com.example.link".into(),
            name: "Link".into(),
            declared: true,
        }
    }

    #[test]
    fn link_names_capabilities_and_refuses_another_word() {
        assert!(ahead(&manifest(serde_json::json!({}))).unwrap().is_empty());
        assert_eq!(
            ahead(&manifest(
                serde_json::json!({"link": ["markdown", "grouped_lists", "presence"]})
            ))
            .unwrap(),
            vec![
                Capability::Markdown,
                Capability::GroupedLists,
                Capability::Presence
            ]
        );
        let refused = ahead(&manifest(serde_json::json!({"link": ["markdwon"]}))).unwrap_err();
        assert!(
            refused.contains("markdwon") && refused.contains("markdown"),
            "{refused}"
        );
    }

    /// A widget's entry plan is admitted against its app's set on Apple, so
    /// the set holds what the entry uses: here a table, laid out as a grid.
    #[test]
    fn an_apple_composition_links_what_a_widget_entry_uses() {
        let app = contract::compile("component A\n  view\n    text \"a\"\n").unwrap();
        let entry = contract::compile(
            "component W\n  view\n    table\n      tr\n        td\n          text \"a\"\n",
        )
        .unwrap();
        for (platform, grid) in [("ios", true), ("macos", true), ("linux", false)] {
            let inputs = serde_json::json!({ "platform": platform, "grantCeiling": "" });
            let uses = super::production_uses(
                std::path::Path::new(""),
                &app,
                [&entry].into_iter(),
                &inputs,
                false,
            );
            assert_eq!(uses.has(Capability::Grid), grid, "{platform}");
            assert!(!super::production_uses(
                std::path::Path::new(""),
                &app,
                std::iter::empty(),
                &inputs,
                false,
            )
            .has(Capability::Grid));
        }
    }

    /// Production hello, the size row's app, links nothing beyond the core
    /// (LLP 1047.001 D10): every roster entry is absent from its binary.
    #[test]
    fn production_hello_links_no_capability() {
        let hello = contract::compile(include_str!("../../apps/hello/app.contract")).unwrap();
        let hello_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../apps/hello");
        for platform in ["ios", "macos"] {
            let inputs = serde_json::json!({ "platform": platform, "grantCeiling": "" });
            let uses =
                super::production_uses(&hello_dir, &hello, std::iter::empty(), &inputs, false);
            assert_eq!(uses, exact_runner::Uses::NONE, "{platform}: {uses}");
        }
    }

    /// What an update's plan requires of an Apple cohort is what a bake of
    /// it would link: a table needs grid, the manifest's `link` and a
    /// source's declared host work count, and no other platform is asked.
    #[test]
    fn an_update_requires_what_its_bake_would_link_on_apple() {
        let hello = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../apps/hello");
        let manifest = contract::Manifest::read(&hello).unwrap();
        let table = contract::compile(
            "component W\n  view\n    table\n      tr\n        td\n          text \"a\"\n",
        )
        .unwrap();
        let required = |platform: &str, host_work| {
            let inputs = serde_json::json!({ "platform": platform, "grantCeiling": "" });
            super::apple_requirement(
                &hello,
                &table,
                std::iter::empty(),
                &inputs,
                &manifest,
                host_work,
            )
            .unwrap()
        };
        assert_eq!(required("ios", false), Some(serde_json::json!(["grid"])));
        assert_eq!(
            required("macos", true),
            Some(serde_json::json!(["grid", "io"]))
        );
        assert_eq!(required("linux", true), None);
        let mut linking = manifest.clone();
        linking.json["link"] = serde_json::json!(["svg"]);
        let inputs = serde_json::json!({ "platform": "ios", "grantCeiling": "" });
        let ahead =
            super::apple_requirement(&hello, &table, std::iter::empty(), &inputs, &linking, false);
        // SVG's filters run on box effects' engine, so SVG linked ahead or
        // drawn requires both, as the device admits it.
        assert_eq!(
            ahead.unwrap(),
            Some(serde_json::json!(["effects", "grid", "svg"]))
        );
        let svg = contract::compile(
            "component S\n  view\n    svg viewBox=\"0 0 10 10\"\n      circle cx=5 cy=5 r=4\n",
        )
        .unwrap();
        let drawn =
            super::apple_requirement(&hello, &svg, std::iter::empty(), &inputs, &manifest, false);
        assert_eq!(drawn.unwrap(), Some(serde_json::json!(["effects", "svg"])));
    }

    /// A source that declares host work (LLP 1047.001 D12) and one placed on
    /// a worker both answer through the executor, so an Apple composition
    /// links I/O for them.
    #[test]
    fn declared_host_work_and_a_worker_placement_link_io_on_apple() {
        let app = contract::compile("component A\n  view\n    text \"a\"\n").unwrap();
        let none = std::path::Path::new("");
        for (platform, io) in [("ios", true), ("macos", true), ("linux", false)] {
            let inputs = serde_json::json!({ "platform": platform });
            for (declared, linked) in [(true, io), (false, false)] {
                let uses =
                    super::production_uses(none, &app, std::iter::empty(), &inputs, declared);
                assert_eq!(uses.has(Capability::Io), linked, "{platform} {declared}");
            }
        }
        for language in ["rustPlacement", "typescriptPlacement"] {
            for (platform, placed, io) in [
                ("ios", "worker", true),
                ("macos", "worker", true),
                ("ios", "main", false),
                ("linux", "worker", false),
            ] {
                let inputs = serde_json::json!({ "platform": platform, language: placed });
                let uses = super::production_uses(none, &app, std::iter::empty(), &inputs, false);
                assert_eq!(
                    uses.has(Capability::Io),
                    io,
                    "{language} {platform} {placed}"
                );
            }
        }
        // Modules from wherever they come (an entry, link-ahead) bring the
        // executor that runs their calls and the family that loads their
        // Swift.
        let modules = exact_runner::Uses::NONE.with(Capability::Modules);
        let ios = serde_json::json!({ "platform": "ios" });
        let brought = super::dependents(modules, &ios, false);
        assert!(brought.has(Capability::Io) && brought.has(Capability::NativeViews));
        let linux = serde_json::json!({ "platform": "linux" });
        assert_eq!(super::dependents(modules, &linux, false), modules);
    }

    /// A TypeScript bundle reports host work for each route the prelude
    /// parks an answer on, and none for an answer that settles in its turn.
    #[test]
    fn a_typescript_bundle_reports_the_timers_it_can_reach() {
        use crate::compat::typescript_host_work as work;
        assert!(work(b"setTimeout(resolve, 10)"));
        assert!(work(
            b"AbortSignal.timeout(10).addEventListener('abort', resolve)"
        ));
        assert!(work(b"yield scheduler.yield()"));
        assert!(!work(
            b"function answer(a) { return Promise.resolve(a + 1); }"
        ));
    }

    /// On Apple the bake reads what the plan cannot say: a bundled image's
    /// bytes (an SVG links SVG decoding; one it cannot read counts as one),
    /// and an app module artifact, which connects the app-level hatches.
    #[test]
    fn the_bake_reads_bundled_images_and_the_module_artifact() {
        let dir = std::env::temp_dir().join(format!("exact-link-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets/a.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        std::fs::write(dir.join("assets/b.png"), b"<?xml version=\"1.0\"?>\n<svg/>").unwrap();
        let ios = serde_json::json!({ "platform": "ios", "grantCeiling": "" });
        let linked = |source: &str| {
            let plan = contract::compile(&format!("component A\n  view\n    image \"{source}\"\n"))
                .unwrap();
            super::production_uses(&dir, &plan, std::iter::empty(), &ios, false)
        };
        for (source, svg) in [
            ("assets/a.png", false),
            ("assets/b.png", true),
            ("/assets/a.png", false),
            ("assets/missing.png", true),
            ("data:image/png;base64,iVBORw0KGgo=", false),
            ("data:image/svg+xml;base64,PHN2Zy8+", true),
            ("data:image/svg+xml,%3Csvg%2F%3E", true),
            ("https://example.test/a.png", true),
        ] {
            let uses = linked(source);
            assert!(uses.has(Capability::Images), "{source}");
            assert_eq!(uses.has(Capability::Svg), svg, "{source}");
            assert!(!uses.has(Capability::NativeViews), "{source}");
        }
        std::fs::create_dir_all(dir.join("modules/apple")).unwrap();
        std::fs::write(dir.join("modules/apple/Hatches.swift"), "").unwrap();
        assert!(linked("assets/a.png").has(Capability::NativeViews));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The manifest schema's `link` names are the roster's, in its order.
    #[test]
    fn the_schema_names_every_capability() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../scripts/app.schema.json")).unwrap();
        let names: Vec<&str> = schema["properties"]["link"]["items"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let roster: Vec<&str> = Capability::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names, roster);
    }
}
