//! The bake of a host crate over a linked Rust data source (LLP 1153.003 D2): what a generated host crate's build
//! script calls in one line, for an app (`scripts/app-shells.mjs`) and a game (`game/app`, `exact-game-app`) alike.
//! It compiles the app's Contract, bakes the first frame through the source, and writes the plan, the compatibility
//! id and the entry (`entry.rs`) the crate includes.
use contract::DataSource;
use std::{env, fs, path::Path, path::PathBuf};

/// What differs between the two generators' host crates.
pub struct Shape<'a> {
    /// The directory, relative to the app, whose app.json the bake reads: the app's own (`.`), or the copy a game's
    /// bake resolves into `.shells/`.
    pub manifest: &'a str,
    /// Whether the entry takes the manifest's Rust replacement mode (an app). A game's generated hosts require it off.
    pub replacement: bool,
    /// Whether the desktop hosts compose the app's packages and Rust hatches from `packages.rs` and `hatches.rs`,
    /// as a TypeScript app's generated crates do (the crate's `main` is its own). Otherwise the entry carries `main`,
    /// with the packages and no hatches, as a game's does.
    pub composed: bool,
    /// Whether the Apple entry names what it links and takes the update host when the binary has a store, as an
    /// app's does; a game's links every capability into the plain host.
    pub apple_linked: bool,
    /// Whether the bake says a button shortcut on a key the game's `Actions` bind (LLP 1132 D11).
    pub shortcuts: bool,
}

impl Shape<'_> {
    /// An app's host crate (LLP 1153.003 D2).
    pub const APP: Shape<'static> = Shape {
        manifest: ".",
        replacement: true,
        composed: true,
        apple_linked: true,
        shortcuts: false,
    };
}

/// Take the generated Rust for a development APK whose bake runs before Cargo.
/// `files` names extra Rust sources the app's entry includes.
pub fn take_android_linked(files: &[&str]) -> Result<bool, String> {
    let mut sources = vec![
        "baked.rs",
        "hatches.rs",
        "launch.rs",
        "transfers.rs",
        "widgets.rs",
    ];
    sources.extend_from_slice(files);
    crate::take_linked(&sources)
}

/// The entry and its pieces, before `entry.rs` is written: what a game's shared Apple shell compares.
pub struct Entry<'a> {
    /// The platform baked: `apple` resolved to `macos` or `ios`.
    pub platform: &'a str,
    /// The constructor the entry names.
    pub constructor: &'a str,
    /// The compatibility inputs.
    pub inputs: &'a serde_json::Value,
}

/// Bake the app at `app_dir` (relative to the crate) for `platform` (`web`, `apple`, `linux`, `android`, `windows`)
/// through `data`, the source the entry constructs as `constructor` (a Rust expression of type `data_type`).
/// `replace` may return an entry that takes the generated one's place (a game's shared Apple shell). `host_work`:
/// whether `data`'s type hands out host work ([`crate::hands_out_work!`], LLP 1047.001 D12). Panics with the
/// bake's error, as a build script reports it.
#[allow(clippy::too_many_arguments)]
pub fn bake<D: DataSource>(
    platform: &str,
    app_dir: &str,
    shape: &Shape,
    data: D,
    host_work: bool,
    data_type: &str,
    constructor: &str,
    replace: impl FnOnce(&Entry) -> Option<String>,
) {
    if platform == "windows" {
        crate::link_windows_resources();
    }
    let platform = if platform == "apple" {
        if matches!(
            env::var("CARGO_CFG_TARGET_OS").as_deref(),
            Ok("ios" | "tvos" | "watchos")
        ) {
            "ios"
        } else {
            "macos"
        }
    } else {
        platform
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    // The Android shell is a cdylib only on Android (`exact_painted::canvas_jni!`); off it the library is empty.
    if platform == "android" && env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        fs::write(out.join("entry.rs"), "").unwrap();
        return;
    }
    if platform == "android"
        && take_android_linked(&["entry.rs"]).unwrap_or_else(|e| panic!("Android bake: {e}"))
    {
        return;
    }
    let app = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join(app_dir);
    let manifest_dir = app.join(shape.manifest);
    println!("cargo:rerun-if-env-changed=EXACT_ASSET_ROOTS");
    for path in ["app.json", "assets", "deck", "gpu/shaders"]
        .iter()
        .map(|path| app.join(path))
        .chain([manifest_dir.join("app.json")])
    {
        // A nonexistent watch path makes Cargo rebuild every invocation.
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    // The plan reads the whole Contract source graph, not just its root. Keep
    // Cargo's build-script fingerprint aligned with the compiler's imports so
    // an edit to a component file reruns this bake.
    contract::rerun_if_changed(&app.join("app.contract"));
    let manifest =
        contract::Manifest::read(&manifest_dir).unwrap_or_else(|e| panic!("app.json: {e}"));
    let mut plan = contract::compile_path(&app.join("app.contract"))
        .unwrap_or_else(|e| panic!("app.contract: {e}"));
    if shape.shortcuts {
        for warning in contract::shortcut_findings(&app.join("app.contract")).unwrap_or_default() {
            println!("cargo:warning={}", warning.to_string().replace('\n', " "));
        }
    }
    if plan.app_id.is_empty() {
        plan.app_id = manifest.id.clone();
    }
    let grants = data.grants().to_string();
    let baked = contract::bake(plan, data).unwrap_or_else(|e| panic!("bake: {e}"));
    fs::write(out.join("app.plan"), baked.encode()).unwrap();
    let target = env::var("TARGET").unwrap();
    // Android's Canvas host is the Linux host's presenter under Android's own Canvas, so its
    // cohort is Linux's for this target (as apps/caltrain/android/build.rs computes it).
    let cohort = if platform == "android" {
        "linux"
    } else {
        platform
    };
    let compat =
        crate::compatibility_id(&app, cohort, &target, &manifest, Some(&grants), host_work)
            .unwrap_or_else(|e| panic!("compatibility id: {e}"));
    let mode = compat.inputs["rustMode"].as_str().unwrap_or("off");
    assert!(
        shape.replacement || mode == "off",
        "generated game hosts require app.json rust: false"
    );
    fs::write(out.join("compat.json"), compat.to_json()).unwrap();
    // What the APK's build reads beside the plan: its capabilities and derived permissions, and for an app the
    // hatches' install (`hatches.rs`), as a TypeScript app's Android crate writes it.
    if platform == "android" {
        contract::native::android_entry(&app, &manifest, &baked, &out)
            .unwrap_or_else(|e| panic!("android: {e}"));
    }
    let entry = Entry {
        platform,
        constructor,
        inputs: &compat.inputs,
    };
    if let Some(replaced) = replace(&entry) {
        fs::write(out.join("entry.rs"), replaced).unwrap();
        return;
    }
    let mode = match platform {
        // The web links no Rust executor without a module to swap in (LLP 1047 D3).
        "web" => contract::web_rust_mode(&compat.inputs),
        _ => mode,
    };
    let source = runtime_entry(data_type, constructor, mode, &compat.inputs).unwrap();
    let linked = if platform == "web" {
        contract::web_linked(&baked, &compat.inputs)
    } else {
        String::new()
    };
    let host = host(
        platform, shape, &app, &baked, &target, &compat, &manifest, &out,
    );
    let embedded = if platform == "android" {
        ""
    } else {
        r#"const PLAN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.plan"));
const COMPAT: &str = include_str!(concat!(env!("OUT_DIR"), "/compat.json"));"#
    };
    let entry_path = out.join("entry.rs");
    let entry = format!("\n{embedded}\n{source}\n{linked}\n{host}\n");
    if fs::read(&entry_path).ok().as_deref() != Some(entry.as_bytes()) {
        fs::write(entry_path, entry).unwrap();
    }
}

/// A generated host crate's bake (LLP 1153.003 D2), its build script's one line: the app one level up from
/// `.exact/shells/<kind>` at `app_dir`, baked for `platform` through `data`, the source app.json's `rust.data` names,
/// which the entry holds in host storage, and whether its type hands out host work ([`crate::hands_out_work!`]).
pub fn app<D: DataSource>(platform: &str, app_dir: &str, data: D, host_work: bool) {
    let app = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join(app_dir);
    let declared = contract::Manifest::read(&app)
        .and_then(|manifest| manifest.rust_data())
        .unwrap_or_else(|e| panic!("app.json: {e}"))
        .unwrap_or_else(|| panic!("app.json names no rust.data"));
    assert_eq!(
        std::any::type_name::<D>(),
        declared.ty,
        "the source is the one app.json's rust.data names"
    );
    let data_type = format!("exact_data_host::Storage<{}>", declared.ty);
    let constructor = format!("exact_data_host::Storage::new({})", declared.source);
    bake(
        platform,
        app_dir,
        &Shape::APP,
        data,
        host_work,
        &data_type,
        &constructor,
        |_| None,
    );
}

/// The entry's host: its exports over `AppData`, and the desktop hosts' `main` with the packages they compose.
#[allow(clippy::too_many_arguments)]
fn host(
    platform: &str,
    shape: &Shape,
    app: &Path,
    baked: &exact_plan::Plan,
    target: &str,
    compat: &crate::Compat,
    manifest: &contract::Manifest,
    out: &Path,
) -> String {
    let composed = if shape.composed {
        // The packages' Linux or Windows implementations (`exact_packages()`) and the app's Rust hatches
        // (`exact_hatches()`, from modules/<platform>), as a TypeScript app's crates compose them.
        contract::native::write_desktop_packages(platform, app, &baked.encode(), target, out)
            .and_then(|()| contract::native::write_desktop_hatches(platform, app, target, out))
            .unwrap_or_else(|e| panic!("packages: {e}"));
        "exact_painted::Composed { packages: exact_packages(), hatches: exact_hatches() }"
    } else {
        "exact_painted::Composed { packages: exact_packages(), ..Default::default() }"
    };
    let packages = if !matches!(platform, "linux" | "windows") {
        String::new()
    } else if shape.composed {
        r#"include!(concat!(env!("OUT_DIR"), "/packages.rs"));
include!(concat!(env!("OUT_DIR"), "/hatches.rs"));"#
            .to_string()
    } else {
        // The packages' Linux or Windows implementations (LLP 1133.022 D10 item 5).
        contract::native::plan_packages(app, baked, target)
            .unwrap_or_else(|e| panic!("packages: {e}"))
    };
    let main = match platform {
        "linux" => format!(
            "fn main() {{ std::process::exit(exact_linux::run_composed::<AppData>(PLAN, COMPAT, {composed})); }}"
        ),
        "windows" => format!(
            "fn main() {{ std::process::exit(exact_windows::run_composed::<AppData>({:?}, {}, PLAN, COMPAT, {composed})); }}",
            manifest.name,
            manifest.windows_window().unwrap_or_else(|e| panic!("app.json: {e}"))
        ),
        "web" => "exact_web::host!(AppData, PLAN, COMPAT, app_data);".into(),
        "macos" | "ios" if shape.apple_linked => {
            let host = if compat.inputs["store"]["L"] == "0" {
                "exact_apple"
            } else {
                "exact_apple_update"
            };
            // What the archive links, as its compatibility inputs name it (LLP 1047.001 D2).
            format!(
                "{}{host}::host!(AppData, PLAN, COMPAT; linked = EXACT_LINKED);",
                crate::apple_link(compat, host)
            )
        }
        "macos" | "ios" => apple_host("PLAN", "COMPAT"),
        // An app's library with a store takes the update
        // store's opener, which its crate links at deploy.store.android "A" (LLP 1030.006 D3).
        "android" if shape.composed => {
            let updates = if compat.inputs["store"]["L"] == "0" {
                ""
            } else {
                ", updates = exact_painted_update::open"
            };
            format!("include!(concat!(env!(\"OUT_DIR\"), \"/hatches.rs\"));\nexact_painted::canvas_jni!(crate::AppData, hatches = crate::exact_hatches(){updates});")
        }
        "android" => "exact_painted::canvas_jni!(crate::AppData);".into(),
        _ => panic!("unsupported host platform: {platform}"),
    };
    format!("{packages}\n{main}")
}

/// The Apple host's exports over `AppData`, with the plan and compat.json named by the Rust expressions `plan` and
/// `compat`, linking every capability into the host without an update store.
pub fn apple_host(plan: &str, compat: &str) -> String {
    format!("exact_apple::host!(AppData, {plan}, {compat}, None, std::ptr::null(), app_data);")
}

/// The entry over `data_type`, constructed as `constructor`; a source of `()` is the empty one, named by the app's id.
pub fn runtime_entry(
    data_type: &str,
    constructor: &str,
    mode: &str,
    inputs: &serde_json::Value,
) -> Result<String, String> {
    if data_type != "()" {
        return contract::rust_entry(data_type, constructor, mode, inputs);
    }
    // Native storage asks the source's identity before the runner binds the plan.
    let app = inputs["app"]
        .as_str()
        .ok_or("game runtime needs an app id")?;
    empty_entry(constructor, inputs, &format!("{app:?}"))
}

/// The entry over an empty data source, which names the app by `app`, a Rust expression of type `&str`.
pub fn empty_entry(
    constructor: &str,
    inputs: &serde_json::Value,
    app: &str,
) -> Result<String, String> {
    // An empty source has nothing to place: the shared Apple game shell's entry is the same for every placement.
    let entry =
        contract::rust_entry_composed("ExactEmptyData", constructor, "off", inputs, Some(app))?;
    Ok(format!(
        r#"#[derive(Default)]
struct ExactEmptyData;
impl exact_runner::DataSource for ExactEmptyData {{
    fn app_id(&self) -> &str {{ {app} }}
    fn query(&mut self, source: &str, args: &[exact_runner::Value])
        -> Result<exact_runner::Value, exact_runner::DataError>
    {{
        exact_runner::DataSource::query(&mut (), source, args)
    }}
}}
{entry}"#
    ))
}
