//! The binary bake's compatibility and embedded receipt (LLP 1030 D3/D3a).
//! The Cargo build script knows its actual target and data-source grants;
//! Consumers read these bytes instead of reconstructing those facts.

use crate::compat::Compat;
use contract::Manifest;
use exact_update::{gpu_code_digest as code_digest, Baked, Envelope};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

mod watch;

/// A development bake's map from plan node to Contract source (LLP 1035.002
/// D6; LLP 1012.001.000 D6), `OUT_DIR/app.plan.map.json`: beside the plan,
/// never in the product, and only when compiling the app's contract again
/// gives the baked plan's own nodes. The digest inside names the plan it
/// describes, so the driver never joins it to another. A map that cannot be
/// made is absent; it never fails a build.
fn source_map(trust: &str, app: &Path, out: &Path, plan: &exact_plan::Plan, plan_bytes: &[u8]) {
    let path = out.join("app.plan.map.json");
    let _ = std::fs::remove_file(&path);
    if trust != "development" {
        return;
    }
    let Ok(source) = app.join("app.contract").canonicalize() else {
        return;
    };
    let Ok((compiled, map)) = contract::compile_path_mapped(&source) else {
        return;
    };
    if compiled.nodes == plan.nodes {
        let _ = std::fs::write(&path, map.json(plan_bytes));
    }
}

/// Complete the compatibility document from the plan already baked by the
/// app's build script. Production sequence provenance is a signed publisher
/// receipt; an unreceipted stream requires an explicit genesis bake.
pub(crate) fn emit(
    compat: &mut Compat,
    trust: &str,
    app: &Path,
    destination: (&str, &str),
    manifest: &Manifest,
    out: &Path,
    entries: &std::collections::BTreeMap<String, contract::snapshot::Entry>,
) -> Result<(), String> {
    let (platform, target) = destination;
    crate::widgets::check_watch_sources(app, out, target, entries)?;
    for name in [
        "EXACT_UPDATE_RECEIPT",
        "EXACT_UPDATE_GENESIS",
        "EXACT_BAKE_OUTPUT",
        "EXACT_BAKE_INPUTS",
        "EXACT_BAKE_ANALYSIS",
        "EXACT_RUST_BUNDLE",
        "EXACT_GPU_PRODUCT",
        "EXACT_GPU_MODULES",
        "EXACT_GPU_DEVELOPMENT",
        "EXACT_GPU_WEB_PRODUCTS",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let mut plan_bytes = std::fs::read(out.join("app.plan")).map_err(|e| {
        format!("the app bake must write OUT_DIR/app.plan before compatibility_id: {e}")
    })?;
    let mut plan = exact_plan::Plan::decode(&plan_bytes)
        .map_err(|e| format!("the baked plan is invalid: {e:?}"))?;
    if plan.app_id.is_empty() {
        plan.app_id.clone_from(&manifest.id);
        plan_bytes = plan.encode();
        std::fs::write(out.join("app.plan"), &plan_bytes).map_err(|e| e.to_string())?;
    } else if plan.app_id != manifest.id {
        return Err(format!(
            "the baked plan names app {}, but app.json names {}",
            plan.app_id, manifest.id
        ));
    }
    source_map(trust, app, out, &plan, &plan_bytes);
    let plan_card = json!({"sha256": hash(&plan_bytes), "bytes": plan_bytes.len()});
    let (mut assets, thinned) = asset_cards(app, out, manifest, platform)?;
    // A binary whose roster holds one texture family has a renderer that
    // chooses among what its bundle holds; an older one falls back to files
    // a thinned update leaves out. So thinning is a new cohort, and an update
    // is thinned only for a cohort that says it was (`scripts/push.mjs`).
    if let Some(family) = thinned {
        compat.inputs["textureFamily"] = Value::String(family);
        compat.id = crate::compat::compatibility_digest(&compat.inputs);
    }
    let pictures = asset_islands(app, &assets);
    let (widget_assets, plans) = crate::widgets::emit(
        out,
        matches!(platform, "ios" | "macos") && app.join("app.ts").is_file(),
        entries,
    )?;
    assets.extend(widget_assets);
    let rust_assets = if let Some(directory) = std::env::var_os("EXACT_RUST_BUNDLE") {
        stage_rust_bundle(
            Path::new(&directory),
            out,
            &plan_bytes,
            &compat.inputs,
            target,
        )?
    } else {
        // OUT_DIR survives incremental builds; a prior supplied pair must not
        // become part of a later build that deliberately supplies none.
        let stale = out.join("rust");
        if stale.exists() {
            std::fs::remove_dir_all(stale).map_err(|e| e.to_string())?;
        }
        Vec::new()
    };
    assets.extend(rust_assets);
    assets.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let mut graph = artifact_graph(&plan, &compat.inputs, &assets, pictures, target)?;
    graph["contractSources"] = json!(contract::source_graph(&app.join("app.contract"))
        .sources
        .into_iter()
        .map(|s| s.path)
        .filter(|p| p.is_absolute() && p.extension().is_some_and(|ext| ext == "contract"))
        .collect::<Vec<_>>());
    std::fs::write(out.join("artifacts.json"), graph.to_string()).map_err(|e| e.to_string())?;
    compat.target = target.into();
    compat.embedded = json!({"seq":0,"plan":plan_card,"assets":assets,"entryDigest":null,
        "genesis":true,"trust":trust,
        "version":compat.embedded["version"],"buildNumber":compat.embedded["buildNumber"]});
    if plans.as_object().is_some_and(|p| !p.is_empty()) {
        compat.embedded["plans"] = plans;
    }
    // The GPU product is built (and on Apple, signed) before the host. Its
    // exact bytes belong to this app/cohort; a sibling filename is not identity.
    if platform != "web" {
        let mut declared = Declared::new();
        let product = std::env::var_os("EXACT_GPU_PRODUCT")
            .map(|path| gpu_product(Path::new(&path), &mut declared))
            .transpose()?;
        let development = std::env::var("EXACT_GPU_DEVELOPMENT").ok();
        if let Some(card) = gpu_card(&manifest.id, &compat.id, trust, development, product) {
            compat.embedded["gpu"] = card;
        }
        // One card per module (LLP 1009 D6), the same shape as the primary's
        // product card: a module always binds its signed digest.
        if let Ok(modules) = std::env::var("EXACT_GPU_MODULES") {
            let products: serde_json::Map<String, Value> =
                serde_json::from_str(&modules).map_err(|e| format!("EXACT_GPU_MODULES: {e}"))?;
            let mut cards = serde_json::Map::new();
            for (name, path) in products {
                let path = path.as_str().ok_or("EXACT_GPU_MODULES: a product path")?;
                let product = Some(gpu_product(Path::new(path), &mut declared)?);
                if let Some(card) = gpu_card(&manifest.id, &compat.id, trust, None, product) {
                    cards.insert(name, card);
                }
            }
            compat.embedded["gpuModules"] = Value::Object(cards);
        }
        surface_arguments(&plan, &declared)?;
    } else if let Ok(products) = std::env::var("EXACT_GPU_WEB_PRODUCTS") {
        // The web binds no GPU digest (its wasm and its files are one build),
        // but its GPU products compile before this bake too.
        let paths: Vec<PathBuf> =
            serde_json::from_str(&products).map_err(|e| format!("EXACT_GPU_WEB_PRODUCTS: {e}"))?;
        let mut declared = Declared::new();
        for path in paths {
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes =
                std::fs::read(&path).map_err(|e| format!("GPU product {}: {e}", path.display()))?;
            declare(&path, &bytes, &mut declared)?;
        }
        surface_arguments(&plan, &declared)?;
    }
    // A binary without an updater has no stream or rollback floor.
    // Native L=A artifacts still require authenticated sequence provenance.
    let development = compat.inputs["trust"] == "development";
    if development || compat.inputs["store"]["L"] == "0" {
        // Explicit development artifacts and updater-free hosts have a
        // local genesis; neither inherits a production stream receipt.
    } else if std::env::var("EXACT_BAKE_ANALYSIS").as_deref() == Ok("1") {
        // Classification compiles the real target before publication has a
        // sequence. This artifact cannot launch: Baked requires a u64 seq.
        // Only a later authenticated receipt bake may produce a release.
        compat.embedded["analysis"] = json!(true);
        compat.embedded["seq"] = Value::Null;
        compat.embedded["genesis"] = json!(false);
    } else if let Some(path) = std::env::var_os("EXACT_UPDATE_RECEIPT") {
        let path = PathBuf::from(path);
        println!("cargo:rerun-if-changed={}", path.display());
        let raw = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        apply_release(compat, &raw, &plan_bytes)?;
    } else if compat.inputs["store"]["L"] != "0"
        && std::env::var("EXACT_UPDATE_GENESIS").as_deref() != Ok("1")
    {
        return Err("production embedding requires EXACT_UPDATE_RECEIPT naming its authenticated publisher receipt; a new stream must explicitly set EXACT_UPDATE_GENESIS=1 (developer builds use EXACT_UPDATE_TRUST=development)".into());
    }
    if let Some(destination) = std::env::var_os("EXACT_BAKE_OUTPUT") {
        let destination = PathBuf::from(destination);
        std::fs::create_dir_all(&destination)
            .map_err(|e| format!("{}: {e}", destination.display()))?;
        let stem = format!("{platform}-{target}");
        crate::widgets::copy(out, &destination.join(format!("{stem}.entries")), &assets)?;
        // Never watched: the builder rewrites these from OUT_DIR after every
        // Cargo run, so a watch here would rerun this bake on every next build.
        std::fs::write(destination.join(format!("{stem}.json")), compat.to_json())
            .map_err(|e| e.to_string())?;
        std::fs::write(destination.join(format!("{stem}.plan")), &plan_bytes)
            .map_err(|e| e.to_string())?;
        std::fs::write(
            destination.join(format!("{stem}.artifacts.json")),
            graph.to_string(),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The GPU module's baked identity. `trust` is the build's resolved trust
/// (unset is development, as `embedded.trust` records it), never the cohort
/// input: a no-delivery (store L=0) cohort omits trust.
/// gpu-dev authenticates the independently completed module receipt at load;
/// production always embeds the exact GPU digest.
fn gpu_card(
    app: &str,
    cohort: &str,
    trust: &str,
    development: Option<String>,
    product: Option<(String, String)>,
) -> Option<Value> {
    match (development.filter(|_| trust == "development"), product) {
        (Some(name), _) => Some(json!({"app":app,"cohort":cohort,
            "name":name,"trust":"development","receipt":true})),
        (None, Some((name, sha256))) => Some(json!({"name":name,"sha256":sha256,
            "app":app,"cohort":cohort,"trust":trust})),
        (None, None) => None,
    }
}

// The same directory-owned gate used by copying, including direct Cargo
// bakes. This tooling process does not link Unix filesystem code into a wasm
// consumer of `contract`; its Cargo cache is distinct from the calling build.
// A native platform's roster leaves out the texture families its devices do
// not read (`scripts/texture-families.mjs`), and its bundle holds the roster;
// the family it kept, when it left any file out.
fn asset_cards(
    app: &Path,
    out: &Path,
    manifest: &Manifest,
    platform: &str,
) -> Result<(Vec<Value>, Option<String>), String> {
    let gate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/filesystem.mjs");
    println!("cargo:rerun-if-changed={}", gate.display());
    println!(
        "cargo:rerun-if-changed={}",
        gate.with_file_name("texture-families.mjs").display()
    );
    let code = r#"
        import {pathToFileURL} from 'node:url';
        import {resolve} from 'node:path';
        import {createHash} from 'node:crypto';
        const {filesystem} = await import(pathToFileURL(process.argv[1]));
        const {shaderFiles,shaderRoots,shaderPreludeFiles} = await import(pathToFileURL(resolve(process.argv[1],'../app.mjs')));
        const {bundledFamily,unbundledTextures} = await import(pathToFileURL(resolve(process.argv[1],'../texture-families.mjs')));
        const app={dir:process.argv[2],manifest:JSON.parse(process.argv[3])};
        let cards=[];
        for (const [source,prefix] of [['assets','assets'],['deck','deck']]) {
            const tree=filesystem({op:'inventory',root:resolve(process.argv[2],source),optionalRoot:true});
            for (const [name,card] of Object.entries(tree??{})) cards.push({name:`${prefix}/${name}`,...card});
        }
        const held=bundledFamily(process.argv[4]), unbundled=unbundledTextures(cards.map(c=>c.name),held);
        cards=cards.filter(c=>!unbundled.has(c.name));
        for (const [name,bytes] of shaderFiles(app)) cards.push({name:`shaders/${name}`,sha256:createHash('sha256').update(bytes).digest('hex'),bytes:bytes.length});
        process.stdout.write(JSON.stringify({cards:cards.sort((a,b)=>a.name<b.name?-1:a.name>b.name?1:0),roots:[...shaderRoots(app),...shaderPreludeFiles(app)],
            family:unbundled.size?held:null}));
    "#;
    let output = crate::bun()
        .args(["--input-type=module", "-e", code])
        .arg(gate)
        .arg(app)
        .arg(manifest.json.to_string())
        .arg(platform)
        .output()
        .map_err(|e| format!("bake asset gate: {}", crate::bun_error(e)))?;
    if !output.status.success() {
        return Err(format!(
            "bake asset gate: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let inventory: Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("bake asset inventory: {e}"))?;
    let cards = inventory["cards"]
        .as_array()
        .ok_or("missing bake cards")?
        .clone();
    // Every client must stage these as distinct files, APFS included; the
    // publisher and every client parse refuse the same names.
    exact_update::check_asset_names(cards.iter().filter_map(|c| c["name"].as_str()))
        .map_err(|e| format!("bake asset gate: {e}"))?;
    // The helper has its own target directory. Other known bake destinations
    // may also be beneath an external app: never broaden a watch over them.
    let mut outputs = vec![
        out.to_path_buf(),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"),
    ];
    for name in [
        "EXACT_BAKE_OUTPUT",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_BUILD_DIR",
    ] {
        if let Some(path) = std::env::var_os(name) {
            outputs.push(PathBuf::from(path));
        }
    }
    println!("cargo:rerun-if-env-changed=EXACT_ASSET_ROOTS");
    let declared = std::env::var("EXACT_ASSET_ROOTS").ok();
    let roots = ["assets", "deck"]
        .into_iter()
        .filter_map(|root| watch::asset_tree(app, root, &outputs, declared.as_deref()))
        .chain(
            inventory["roots"]
                .as_array()
                .ok_or("missing shader roots")?
                .iter()
                .filter_map(|v| {
                    watch::shader_tree(
                        Path::new(v.as_str().unwrap()),
                        &outputs,
                        declared.as_deref(),
                    )
                }),
        );
    let paths = roots.collect::<std::collections::BTreeSet<_>>();
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let family = inventory["family"].as_str().map(str::to_owned);
    Ok((cards, family))
}

/// Whether an asset of `cards` is an SVG whose picture can show an island
/// (`exact_svg_file::Loaded::islands`): the host draws an SVG image's masks
/// and filters through the island module, as it draws an `svg`'s. Each file
/// is sniffed as the host's loader sniffs it, so the name decides nothing.
fn asset_islands(app: &Path, cards: &[Value]) -> bool {
    use std::io::Read;
    cards
        .iter()
        .filter_map(|card| card["name"].as_str())
        .any(|name| {
            let path = app.join(name);
            let mut prefix = Vec::new();
            std::fs::File::open(&path)
                .and_then(|file| file.take(4096).read_to_end(&mut prefix))
                .is_ok_and(|_| exact_svg_file::sniff(&prefix))
                && std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| exact_svg_file::load(&text).ok())
                    .is_some_and(|picture| picture.islands())
        })
}

// Supply bytes, never just signed names. A binary's entry-zero asset roster is
// used by the updater to decide Current, so claiming absent Rust files would
// prevent their download and leave the embedded implementation answering them.
fn stage_rust_bundle(
    directory: &Path,
    out: &Path,
    plan: &[u8],
    inputs: &Value,
    target: &str,
) -> Result<Vec<Value>, String> {
    let mode = inputs["rustMode"]
        .as_str()
        .ok_or("Rust bundle requires a baked executor policy")?;
    let (executor, module_target, filename) = match mode {
        "wasm" | "browser" => ("wasm", "wasm32-unknown-unknown", "app.module.wasm"),
        "tiered" if !target.contains("-apple-ios") && !target.starts_with("wasm") => {
            ("tiered", target, "app.module.bin")
        }
        "native" if !target.contains("-apple-ios") && !target.starts_with("wasm") => {
            let name = if target.contains("-apple-darwin") {
                "app.module.dylib"
            } else if target.contains("-windows-") {
                "app.module.dll"
            } else {
                "app.module.so"
            };
            ("native", target, name)
        }
        _ => return Err("supplied Rust bundle is disabled by this host's executor policy".into()),
    };
    if !std::fs::symlink_metadata(directory)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_dir()
    {
        return Err("Rust bundle must be an ordinary directory, not a symlink".into());
    }
    let receipt_path = directory.join("app.module.json");
    let module_path = directory.join(filename);
    for path in [&receipt_path, &module_path] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let receipt = read_rust_artifact(&receipt_path, 1 << 20)?;
    let module = read_rust_artifact(&module_path, 32 << 20)?;
    let meta: Value =
        serde_json::from_slice(&receipt).map_err(|e| format!("Rust bundle receipt: {e}"))?;
    if meta["version"].as_u64() != Some(1)
        || meta["kind"] != "rust"
        || meta["abi"].as_u64() != Some(crate::compat::RUST_ABI.into())
    {
        return Err("Rust bundle has an unsupported receipt or ABI".into());
    }
    let expected_app = inputs["app"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Rust bundle needs an admitted app identity")?;
    let expected_grants = inputs["rustGrants"]
        .as_str()
        .ok_or("Rust bundle needs its baked source grants")?;
    let ceiling = inputs["grantCeiling"]
        .as_str()
        .ok_or("Rust bundle needs the baked grant ceiling")?;
    let admitted: Vec<_> = ceiling
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if expected_grants
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .any(|line| !admitted.contains(&line))
    {
        return Err("Rust source grants exceed the app ceiling".into());
    }
    if meta["appId"].as_str() != Some(expected_app)
        || meta["grants"].as_str().map(str::trim) != Some(expected_grants.trim())
        || meta["executor"].as_str() != Some(executor)
        || meta["target"].as_str() != Some(module_target)
    {
        return Err("Rust bundle changes the baked app, grants, executor or target".into());
    }
    for (field, name, bytes) in [
        ("plan", "app.plan", plan),
        ("module", filename, module.as_slice()),
    ] {
        let card = &meta[field];
        if card["file"].as_str() != Some(name)
            || card["bytes"].as_u64() != Some(bytes.len() as u64)
            || card["sha256"].as_str() != Some(hash(bytes).as_str())
        {
            return Err(format!(
                "Rust bundle {field} does not match its exact baked bytes"
            ));
        }
    }
    if executor == "wasm" && !module.starts_with(b"\0asm\x01\0\0\0") {
        return Err("Rust bundle module is not wasm version 1".into());
    }
    let destination = out.join("rust");
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let mut cards = Vec::new();
    for (name, bytes) in [
        ("app.module.json", receipt.as_slice()),
        (filename, module.as_slice()),
    ] {
        std::fs::write(destination.join(name), bytes).map_err(|e| e.to_string())?;
        cards.push(json!({"name":format!("rust/{name}"),"sha256":hash(bytes),"bytes":bytes.len()}));
    }
    Ok(cards)
}

fn read_rust_artifact(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > limit as u64 {
        return Err(format!(
            "{} must be an ordinary bounded Rust artifact",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Rust bundle artifact exceeds its byte limit".into());
    }
    Ok(bytes)
}

// Shapes use semantic names and field order, never plan-local row numbers.
// The same source signature survives unrelated declarations being inserted.
fn shape(plan: &exact_plan::Plan, ty: exact_plan::TypesId, depth: usize) -> Result<Value, String> {
    use exact_plan::TypeKind;
    if depth > 128 {
        return Err("artifact source shape exceeds 128 levels".into());
    }
    let row = plan.type_(ty);
    Ok(match row.kind {
        TypeKind::Option | TypeKind::List => {
            json!({"kind":format!("{:?}", row.kind),"item":shape(plan, row.elem.ok_or("container type has no item")?, depth + 1)?})
        }
        TypeKind::Record => {
            let fields = row.fields.iter().map(|id| {
                let field = plan.field(id);
                Ok(json!({"name":plan.str(field.name),"type":shape(plan, field.ty, depth + 1)?}))
            }).collect::<Result<Vec<Value>, String>>()?;
            json!({"kind":"Record","fields":fields})
        }
        // An enum is a string row whose field names are its members. An
        // installed source may answer only those, so a changed vocabulary is
        // a different shape; the members are sorted, as their order means
        // nothing. A receipt from before this recorded a bare "String" for
        // an enum, which differs from this, so it is refused, not trusted.
        TypeKind::String if !row.fields.is_empty() => {
            let mut members: Vec<&str> = row
                .fields
                .iter()
                .map(|id| plan.str(plan.field(id).name))
                .collect();
            members.sort_unstable();
            json!({"kind":"String","members":members})
        }
        _ => json!(format!("{:?}", row.kind)),
    })
}

/// The plan's requirement graph against a cohort's compatibility inputs
/// (`exact-bake graph`): what a publisher that starts from a signed stream
/// head, not a native bake, checks a new plan against.
pub fn plan_graph(plan_bytes: &[u8], inputs: &Value, target: &str) -> Result<Value, String> {
    let plan = exact_plan::Plan::decode(plan_bytes).map_err(|e| format!("the plan: {e:?}"))?;
    let mut graph = artifact_graph(&plan, inputs, &[], false, target)?;
    let node = &mut graph["artifacts"][0];
    node["sha256"] = json!(hash(plan_bytes));
    node["bytes"] = json!(plan_bytes.len());
    // The plan's own format and kernel schema, not the cohort's: this tool
    // decoded it, so it carries this format, and a cohort built for another
    // is then refused by the comparison of requirements, not given a plan
    // its binary cannot read.
    let requires = &mut node["requires"];
    requires["formatVersion"] = json!(exact_plan::FORMAT_VERSION);
    requires["formatDigest"] = json!(format!("{:016x}", exact_plan::FORMAT_DIGEST));
    requires["kernelSchema"] = json!(format!("{:016x}", plan.kernel_schema_digest));
    Ok(graph)
}

/// `pictures`: whether an SVG image the app bundles can show an island
/// (`asset_islands`).
fn artifact_graph(
    plan: &exact_plan::Plan,
    inputs: &Value,
    assets: &[Value],
    pictures: bool,
    target: &str,
) -> Result<Value, String> {
    let mut sources = serde_json::Map::new();
    for row in &plan.sources {
        let params = row
            .params
            .iter()
            .map(|id| shape(plan, plan.source_param(id).ty, 0))
            .collect::<Result<Vec<_>, _>>()?;
        sources.insert(
            plan.str(row.name).into(),
            json!({"params":params,"result":shape(plan, row.ty, 0)?}),
        );
    }
    // Every surface row includes deferred templates and branches. This is
    // prior plan demand, not a claim to enumerate the native module's exports.
    let mut calls = serde_json::Map::new();
    for row in &plan.surfaces {
        let name = plan.str(row.name);
        let arities = calls.entry(name).or_insert_with(|| json!([]));
        let arity = json!(row.args.len);
        let values = arities
            .as_array_mut()
            .ok_or("surface arities are not an array")?;
        if !values.contains(&arity) {
            values.push(arity);
            values.sort_by_key(|v| v.as_u64());
        }
    }
    let mut requires = serde_json::Map::new();
    requires.insert("surfaceCalls".into(), Value::Object(calls.clone()));
    for key in ["app", "kernelSchema", "formatVersion", "formatDigest"] {
        requires.insert(key.into(), inputs[key].clone());
    }
    // The plan's source table covers resource calls and sends, including
    // branches not mounted during bake. Installed cohorts retain their own
    // implementations of matching names/shapes (1030 D3's stated caveat).
    requires.insert("sources".into(), Value::Object(sources.clone()));
    if sources.keys().any(|name| {
        name != exact_runner::delivery::SOURCE
            && name != exact_runner::viewport::SOURCE
            && name != exact_runner::page::SOURCE
    }) {
        requires.insert("executors".into(), inputs["executors"].clone());
        requires.insert("grantCeiling".into(), inputs["grantCeiling"].clone());
    }
    // @ref LLP 1075.003.000.001 §4.3 — the hatch words the plan marks that
    // this platform's module handles: the installed module must handle each.
    // A word this platform does not handle is no requirement here: its nodes
    // are shown and never called.
    let handled = inputs["hatches"].as_array().cloned().unwrap_or_default();
    let mut marked: Vec<&str> = plan
        .bindings
        .iter()
        .filter(|b| {
            b.kind == exact_plan::BindingKind::Prop
                && exact_kernel::PropId::from_wire(b.id) == Some(exact_kernel::PropId::Hatch)
        })
        .filter_map(|b| crate::colors::literal(plan, &b))
        .filter(|word| handled.iter().any(|h| h == word))
        .collect();
    marked.sort_unstable();
    marked.dedup();
    if !marked.is_empty() {
        requires.insert("hatches".into(), json!(marked));
    }
    // A purpose's text is in the installed binary's Info.plist: a bundle
    // whose purposes differ is a binary release (LLP 1069.008 D5).
    // What a TypeScript module may ask of the binary's prelude (compat.rs
    // `TYPESCRIPT_RUNTIME`): an older cohort's prelude would ignore it
    // silently (`exactBodyFrom` sent as an empty body), so it takes no bundle.
    if inputs.get("typescriptRuntime").is_some() {
        requires.insert(
            "typescriptRuntime".into(),
            inputs["typescriptRuntime"].clone(),
        );
    }
    if inputs.get("grantPurposes").is_some() {
        requires.insert("grantPurposes".into(), inputs["grantPurposes"].clone());
    }
    let rust = assets
        .iter()
        .any(|asset| asset["name"] == "rust/app.module.json");
    let rust_requirements = if rust {
        let mode = inputs["rustMode"]
            .as_str()
            .ok_or("Rust artifact lacks executor policy")?;
        let target = if mode == "wasm" || mode == "browser" {
            "wasm32-unknown-unknown".to_string()
        } else {
            target.to_string()
        };
        json!({"rustMode":mode,"rustAbi":crate::compat::RUST_ABI,"rustTarget":target,"grantCeiling":inputs["grantCeiling"],"rustGrants":inputs["rustGrants"]})
    } else {
        json!({})
    };
    if rust {
        requires.remove("sources");
        requires.extend(rust_requirements.as_object().unwrap().clone());
    }
    let plan_bytes = plan.encode();
    let mut artifacts = vec![
        json!({"name":"app.plan","kind":"bundle","sha256":hash(&plan_bytes),"bytes":plan_bytes.len(),"requires":requires}),
    ];
    for asset in assets {
        let mut requires = serde_json::Map::new();
        if asset["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("rust/"))
        {
            requires.extend(rust_requirements.as_object().unwrap().clone());
        }
        // The module pair runs only on a binary whose TypeScript source holds
        // exactly its grants (js/src/paired.rs refuses any other) and has its
        // prelude's runtime.
        if asset["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("typescript/"))
        {
            requires.insert(
                "javascriptGrants".into(),
                inputs["javascriptGrants"].clone(),
            );
            if inputs.get("typescriptRuntime").is_some() {
                requires.insert(
                    "typescriptRuntime".into(),
                    inputs["typescriptRuntime"].clone(),
                );
            }
        }
        if let Some(stem) = asset["name"]
            .as_str()
            .and_then(|n| n.strip_prefix("shaders/"))
            .and_then(|n| n.strip_suffix(".wgsl"))
        {
            let interface = inputs["gpuSurfaces"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["name"] == stem))
                .ok_or_else(|| format!("shader {stem} has no baked surface interface"))?;
            requires.insert("gpuSurfaces".into(), json!([interface]));
        }
        artifacts.push(json!({"name":asset["name"],"kind":"bundle","sha256":asset["sha256"],"bytes":asset["bytes"],"requires":requires}));
    }
    // The host's loaded modules the plan can reach (LLP 1047 D1's loaded
    // tier), deferred templates and branches included, each by the rule its
    // host loads it by: the Canvas 2D GPU module for a `Canvas`, the SVG
    // island module where `svg_islands` holds or a bundled SVG image can
    // show an island, the video arm for a `Video` or an image's picture in
    // picture (LLP 1042 §9), the web arm for a
    // `WebView`, and the sound arm for a declared sound (LLP 1096 D8: a
    // `sound` declaration is not a node). A build whose plan is fixed
    // leaves out the rest.
    let makes = |wanted: exact_kernel::NodeType| {
        plan.nodes
            .iter()
            .any(|node| exact_kernel::NodeType::from_wire(node.node_type) == Some(wanted))
    };
    let loads: Vec<&str> = [
        ("canvas", makes(exact_kernel::NodeType::Canvas)),
        ("svg", pictures || exact_runner::svg_islands(plan)),
        (
            "video",
            makes(exact_kernel::NodeType::Video)
                || exact_runner::uses::uses_picture_in_picture(plan),
        ),
        ("web", makes(exact_kernel::NodeType::WebView)),
        ("sound", !plan.sounds.is_empty()),
        // LLP 1125 D6: the MIDI arm for a plan that asks for ports or hears an input.
        ("midi", exact_runner::uses::uses_midi(plan)),
        // LLP 1133.005 D13: the listener for an app whose grants can listen.
        ("listen", listens(inputs)),
    ]
    .into_iter()
    .filter_map(|(module, reached)| reached.then_some(module))
    .collect();
    // LLP 1098 D8: a node that binds `mediaTitle` (`metadata=`) claims the
    // media session, which iOS shows only for a playback session.
    let media_session = plan.nodes.iter().any(|node| {
        node.bindings.iter().any(|b| {
            let b = plan.binding(b);
            b.kind == exact_plan::BindingKind::Prop
                && b.id == exact_kernel::PropId::MediaTitle as u16
        })
    });
    // The linked tier the plan uses (LLP 1047 D2), by name: a native build
    // whose plan is fixed links these and no other (LLP 1047.001 D2).
    let uses: Vec<&str> = exact_runner::uses(plan)
        .with_grants(inputs["grantCeiling"].as_str().unwrap_or(""))
        .iter()
        .map(|c| c.name())
        .collect();
    let mut extensions = std::collections::BTreeMap::new();
    for (extension, package) in plan
        .modules
        .iter()
        .map(|m| (m.extension, m.package))
        .chain(plan.views.iter().map(|v| (v.extension, v.package)))
        .chain(plan.module_events.iter().map(|e| (e.extension, e.package)))
    {
        let (extension, package) = (plan.str(extension), plan.str(package));
        if let Some(previous) = extensions
            .insert(extension, package)
            .filter(|previous| *previous != package)
        {
            return Err(format!(
                "bake-extension-ambiguous: extension '{extension}' names both '{previous}' and '{package}'; host rosters require one package per extension"
            ));
        }
    }
    Ok(
        json!({"version":1,"sources":sources,"extensions":extensions,"surfaceCalls":calls,"loads":loads,"uses":uses,"mediaSession":media_session,"launchBackground":launch_background(plan),"artifacts":artifacts}),
    )
}

/// Whether the app's grant ceiling has a `net.listen` line or a loopback
/// `auth.callback` (D9's receiver is the listener's): without either,
/// everything the listener would do is refused, so the build carries none.
fn listens(inputs: &Value) -> bool {
    (inputs["grantCeiling"].as_str().unwrap_or("").lines()).any(|line| {
        let line = line.trim();
        exact_runner::listen::grant_prefix(line).is_some()
            || line
                .strip_prefix("auth.callback ")
                .is_some_and(|url| exact_runner::auth::loopback_callback(url).is_some())
            || [
                "auth.callback http://127.0.0.1/",
                "auth.callback http://[::1]/",
            ]
            .iter()
            .any(|p| line.starts_with(p))
    })
}

/// LLP 1116.011 D2.3: the ground the app's first route stands on, which the
/// iOS launch screen shows until its first frame: `"grouped"` where the first
/// route's `background-color` is the metric the compiler gives a route that
/// scrolls a grouped list (`-exact-grouped-page-ground`, LLP 1116.007 D5),
/// iOS's grouped grey, or that role written (`-exact-grouped-background`),
/// `"system"` for any other (the platform's page, `systemBackground`). A route
/// is a node with a `navigationKey` and no `navigationBack` (the navigation
/// root has both); the first in the plan's order, past the page the compiler
/// writes for an unknown address (its test id `not-found`), is the first one
/// the author declared. A route whose background is written or computed is
/// the author's: the manifest's `background_color` names its launch colour.
fn launch_background(plan: &exact_plan::Plan) -> &'static str {
    use exact_kernel::PropId;
    use exact_plan::BindingKind::{Prop, Style};
    let literal = |node: &exact_plan::NodesRow, kind, id: u16| {
        node.bindings.iter().find_map(|b| {
            let b = plan.binding(b);
            (b.kind == kind && b.id == id).then(|| crate::colors::literal(plan, &b))
        })
    };
    let route = plan.nodes.iter().find(|node| {
        let has = |id: PropId| literal(node, Prop, id as u16).is_some();
        has(PropId::NavigationKey)
            && !has(PropId::NavigationBack)
            && literal(node, Prop, PropId::TestId as u16) != Some(Some("not-found"))
    });
    let ground = route.and_then(|node| {
        literal(&node, Style, exact_kernel::StyleId::BackgroundColor as u16).flatten()
    });
    if matches!(
        ground,
        Some("-exact-grouped-page-ground" | "-exact-grouped-background")
    ) {
        "grouped"
    } else {
        "system"
    }
}

/// Refresh the resident compiler's candidate graph beside its plan. The
/// actual binary's compatibility, provided sources and input receipt stay
/// frozen until the binary-producing build runs again.
pub fn write_development_artifacts(plan: &exact_plan::Plan) -> Result<(), String> {
    let Some(path) = std::env::var_os("EXACT_DEV_BAKE") else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let mut receipt: Value =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if receipt["trust"] != "development"
        || receipt["compat"]["inputs"]["app"] != plan.app_id
        || receipt["compat"]["inputs"]["platform"] != "web"
    {
        return Err("resident compiler needs its app's actual development web bake receipt".into());
    }
    let graph = artifact_graph(
        plan,
        &receipt["compat"]["inputs"],
        &[],
        false,
        "wasm32-unknown-unknown",
    )?;
    let rows = receipt["graph"]["artifacts"]
        .as_array_mut()
        .ok_or("bake receipt has no artifacts")?;
    let node = rows
        .iter_mut()
        .find(|row| row["name"] == "app.plan")
        .ok_or("bake receipt has no plan node")?;
    *node = graph["artifacts"][0].clone();
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, receipt.to_string())
        .and_then(|_| std::fs::rename(&temporary, &path))
        .map_err(|e| e.to_string())
}

fn apply_release(compat: &mut Compat, bytes: &[u8], plan: &[u8]) -> Result<(), String> {
    let receipt: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("publisher receipt is not JSON: {e}"))?;
    let mut envelope =
        Envelope::parse(&serde_json::to_vec(&receipt["envelope"]).map_err(|e| e.to_string())?)?;
    if envelope.index.is_some() {
        // The record carries the asset index as JSON; its blob is that
        // value's canonical bytes, which the signed head names by digest.
        let index = exact_update::canonical_bytes(&receipt["assets"].to_string())
            .map_err(|e| format!("publisher receipt's asset index: {e}"))?;
        envelope.read_index(&index)?;
    }
    let baked = Baked::from_compat(&compat.to_json(), plan)?;
    envelope.verify(&baked.embedded.verification_keys)?;
    if envelope.app_id != baked.embedded.app_id
        || envelope.stream.channel != baked.embedded.channel
        || envelope.stream.compatibility_id != compat.id
        || envelope.stream.seq == 0
    {
        return Err(format!(
            "publisher receipt names {} {} {} seq {}; this binary is {} {} {}",
            envelope.app_id,
            envelope.stream.channel,
            envelope.stream.compatibility_id,
            envelope.stream.seq,
            baked.embedded.app_id,
            baked.embedded.channel,
            compat.id
        ));
    }
    if envelope.plan.sha256 != compat.embedded["plan"]["sha256"]
        || envelope.plan.bytes != plan.len() as u64
    {
        return Err("publisher receipt does not name the exact embedded plan bytes".into());
    }
    let mut named = envelope
        .assets()?
        .iter()
        .map(|a| json!({"name":a.name,"sha256":a.sha256,"bytes":a.bytes}))
        .collect::<Vec<_>>();
    named.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    if Value::Array(named) != compat.embedded["assets"] {
        return Err("publisher receipt does not name the complete embedded asset roster".into());
    }
    compat.embedded["seq"] = json!(envelope.stream.seq);
    compat.embedded["entryDigest"] = json!(envelope.digest);
    compat.embedded["genesis"] = Value::Bool(false);
    Ok(())
}

/// Each surface a GPU product declares the named arguments of, with them and
/// the product's file name.
type Declared = std::collections::BTreeMap<String, (Vec<String>, String)>;

/// A signed GPU product's file name and digest, watched by the bake: its code
/// belongs to this app and cohort, which the loader checks before opening it.
/// The surfaces it declares the arguments of go into `declared`.
fn gpu_product(path: &Path, declared: &mut Declared) -> Result<(String, String), String> {
    println!("cargo:rerun-if-changed={}", path.display());
    let bytes = std::fs::read(path).map_err(|e| format!("GPU product {}: {e}", path.display()))?;
    let name = declare(path, &bytes, declared)?;
    Ok((name, code_digest(&bytes)))
}

/// The product's file name, after adding the surfaces whose arguments its
/// `bytes` declare to `declared`.
fn declare(path: &Path, bytes: &[u8], declared: &mut Declared) -> Result<String, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("GPU product filename is not UTF-8")?
        .to_string();
    let surfaces = exact_plan::surface_arguments::declared(bytes)
        .map_err(|e| format!("GPU product {}: {e}", path.display()))?;
    for (surface, arguments) in surfaces {
        declared.insert(surface, (arguments, name.clone()));
    }
    Ok(name)
}

/// Refuse a plan whose canvas passes a named argument that its surface's GPU
/// product, bound beside the plan, does not declare, or more values by position
/// than it has arguments (expo/exact#176). The product
/// compiles before this bake reads the Contract, so the two disagree only
/// when a source changed in between; at run time the canvas would fail to
/// bind.
fn surface_arguments(plan: &exact_plan::Plan, declared: &Declared) -> Result<(), String> {
    let arguments = declared
        .iter()
        .map(|(surface, (names, _))| (surface.clone(), names.clone()))
        .collect();
    let missing = exact_plan::surface_arguments::undeclared(plan, &arguments);
    if missing.is_empty() {
        return Ok(());
    }
    let each = missing.iter().map(|found| {
        use exact_plan::surface_arguments::Undeclared;
        let surface = found.surface();
        let (names, product) = &declared[surface];
        let has = if names.is_empty() { "none".to_owned() } else { names.join(", ") };
        match found {
            Undeclared::Name { argument, .. } => format!("canvas surface `{surface}` passes `{argument}`, which the GPU product {product} does not declare (it declares {has})"),
            Undeclared::Arity { passed, .. } => format!("canvas surface `{surface}` passes {passed} values by position, and the GPU product {product} declares {} argument{} ({has})", names.len(), if names.len() == 1 { "" } else { "s" }),
        }
    });
    Err(format!(
        "{}: a source changed after the GPU product compiled and before the Contract bake read it (an edit while the build ran); build again",
        each.collect::<Vec<_>>().join("; ")
    ))
}

fn hash(bytes: &[u8]) -> String {
    exact_update::sha256_hex(bytes)
}

#[cfg(test)]
mod tests;
