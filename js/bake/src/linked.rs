//! A bake made outside Cargo for an Apple link (`exact-js-bake --cargo`,
//! scripts/linked-plan.mjs): the archive's build script takes from it only
//! what Rust compiles, and the link carries the plan, `compat.json` and the
//! module's bytecode in the executable's `__TEXT` (`exact_apple::baked`).

use crate::replace_changed;
use std::path::Path;

/// The files of a bake the archive compiles: all but the plan, the module's
/// bytecode and their receipts, which the link carries.
pub(crate) const COMPILED: [&str; 4] = ["module.rs", "logic.rs", "linked.rs", "host.rs"];

/// The module's bytecode from the section the link added
/// (`exact_apple::baked::bytecode`), for `exact_js::Module::new`, which
/// takes it through `Into<exact_js::Bytecode>`.
pub(crate) const LINKED_BYTECODE: &str = r#"#[allow(dead_code)]
pub const APP_BYTECODE: ExactLinkedBytecode = ExactLinkedBytecode;
#[doc(hidden)]
pub struct ExactLinkedBytecode;
impl From<ExactLinkedBytecode> for ::exact_js::Bytecode {
    fn from(_: ExactLinkedBytecode) -> Self {
        ::exact_js::Bytecode::Linked(::exact_apple::baked::bytecode())
    }
}
"#;

pub(crate) const ANDROID_BYTECODE: &str = r#"#[allow(dead_code)]
pub const APP_BYTECODE: ExactLinkedBytecode = ExactLinkedBytecode;
#[doc(hidden)]
pub struct ExactLinkedBytecode;
impl From<ExactLinkedBytecode> for ::exact_js::Bytecode {
    fn from(_: ExactLinkedBytecode) -> Self {
        ::exact_js::Bytecode::Linked(::exact_painted::android_baked!("exact_app_hbc"))
    }
}
"#;

pub(crate) const ANDROID_GRANTS: &str = r#"#[allow(dead_code)]
pub const GRANTS: ExactLinkedGrants = ExactLinkedGrants;
#[doc(hidden)]
pub struct ExactLinkedGrants;
impl std::ops::Deref for ExactLinkedGrants {
    type Target = str;
    fn deref(&self) -> &str {
        ::exact_painted::android::baked_grants(::exact_painted::android_baked!("exact_app_module_json"))
    }
}
impl From<ExactLinkedGrants> for String {
    fn from(grants: ExactLinkedGrants) -> Self {
        (*grants).to_owned()
    }
}
"#;

const ANDROID_SURFACES: &str = r#"#[allow(dead_code)]
pub const CANVAS_SURFACES: ExactLinkedSurfaces = ExactLinkedSurfaces;
#[doc(hidden)]
pub struct ExactLinkedSurfaces;
impl AsRef<[(&'static str, usize)]> for ExactLinkedSurfaces {
    fn as_ref(&self) -> &[(&'static str, usize)] {
        ::exact_painted::android::baked_canvas_surfaces(::exact_painted::android_baked!("exact_app_module_json"))
    }
}
"#;

/// The app's identity and grants from the compat.json the link added
/// (`exact_apple::baked::identity`): `APP` and `GRANTS` read as `&str`
/// (`Deref`, `Display`) and convert `Into<String>` as the constants did.
pub(crate) const LINKED_IDENTITY: &str = r#"#[allow(dead_code)]
pub const APP: ::exact_apple::baked::Linked = ::exact_apple::baked::Linked::App;
#[allow(dead_code)]
pub const GRANTS: ::exact_apple::baked::Linked = ::exact_apple::baked::Linked::Grants;
"#;

/// The app id the entry names an unnamed data source by
/// (`contract::rust_entry_naming`), from the same compat.json.
pub(crate) const LINKED_APP: &str = "::exact_apple::baked::identity().app";

/// The module's Canvas 2D roster from the same compat.json; it is
/// `AsRef<[(&str, usize)]>`, as `with_canvas_surfaces` takes it.
const LINKED_SURFACES: &str = r#"#[allow(dead_code)]
pub const CANVAS_SURFACES: ::exact_apple::baked::LinkedSurfaces = ::exact_apple::baked::LinkedSurfaces;
"#;

/// The Canvas 2D roster (LLP 1056 D1), available before the module loads.
pub(crate) fn canvas_surfaces(meta: &serde_json::Value, apple: bool, android: bool) -> String {
    if android {
        return ANDROID_SURFACES.to_owned();
    }
    if apple {
        return LINKED_SURFACES.to_owned();
    }
    let mut source =
        String::from("#[allow(dead_code)]\npub const CANVAS_SURFACES: &[(&str, usize)] = &[");
    for (name, arity) in meta["surfaces"].as_object().into_iter().flatten() {
        source.push_str(&format!("({name:?}, {}), ", arity.as_u64().unwrap_or(0)));
    }
    source.push_str("];\n");
    source
}

/// The entry's plan and compatibility document, embedded from OUT_DIR.
pub(crate) const EMBEDDED: &str = r#"#[allow(dead_code)]
fn exact_plan() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/app.plan"))
}
#[allow(dead_code)]
fn exact_compat() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/compat.json"))
}
"#;

/// The entry's plan and compatibility document, from the sections the link added.
pub(crate) const LINKED: &str = r#"#[allow(dead_code)]
fn exact_plan() -> &'static [u8] {
    ::exact_apple::baked::plan()
}
#[allow(dead_code)]
fn exact_compat() -> &'static str {
    ::exact_apple::baked::compat()
}
"#;

/// The pair over an entry that declares `const PLAN` and `const COMPAT`
/// itself, as `exact new` wrote it before the plan was linked: such an app
/// builds as it did, its plan in the archive, until `bun exact.mjs update`
/// drops the two (`names_its_plan`).
pub(crate) const NAMED: &str = r#"#[allow(dead_code)]
fn exact_plan() -> &'static [u8] {
    PLAN
}
#[allow(dead_code)]
fn exact_compat() -> &'static str {
    COMPAT
}
"#;

/// Whether the crate's `src/lib.rs` declares the plan and compatibility
/// constants itself (see `NAMED`); watched, so `update` dropping them is
/// noticed. Outside a build script, or in a crate with no `src/lib.rs`, no.
pub(crate) fn names_its_plan() -> bool {
    let Some(dir) = std::env::var_os("CARGO_MANIFEST_DIR") else {
        return false;
    };
    let entry = Path::new(&dir).join("src/lib.rs");
    let Ok(source) = std::fs::read_to_string(&entry) else {
        return false;
    };
    println!("cargo:rerun-if-changed={}", entry.display());
    let declares = |name: &str| {
        source
            .lines()
            .any(|line| line.starts_with(&format!("const {name}:")))
    };
    declares("PLAN") && declares("COMPAT")
}

/// `build` over a bake made outside Cargo into `baked`, for OUT_DIR `out`.
pub(crate) fn take_baked(baked: &Path, out: &Path) -> Result<(), String> {
    if names_its_plan() {
        return Err("src/lib.rs declares `const PLAN` and `const COMPAT`, which read a plan this build links instead: run `bun exact.mjs update`, which drops them".into());
    }
    for name in COMPILED {
        let from = baked.join(name);
        println!("cargo:rerun-if-changed={}", from.display());
        let bytes = std::fs::read(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        replace_changed(&out.join(name), &bytes)?;
    }
    replace_changed(&out.join("baked.rs"), LINKED.as_bytes())?;
    // What an earlier bake in Cargo left here would be stale.
    for name in [
        "app.plan",
        "app.plan.map.json",
        "compat.json",
        "artifacts.json",
        "app.module.json",
        "app.js",
        "app.hbc",
    ] {
        match std::fs::remove_file(out.join(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(())
}

impl crate::Baked {
    /// Publish the complete result to a *new* directory. Existing output is
    /// never replaced; a dev server selects an accepted generation separately.
    pub fn write_new(&self, output: &Path) -> Result<(), String> {
        let output = std::path::absolute(output).map_err(|e| e.to_string())?;
        if output.exists() {
            return Err(format!("output already exists: {}", output.display()));
        }
        // Reserve the destination before writing, so another producer cannot
        // be overwritten by a later rename. The receipt is written last; this
        // directory is not a candidate until that complete receipt exists.
        std::fs::create_dir(&output).map_err(|e| e.to_string())?;
        let result = (|| {
            contract::snapshot::write_entries(&self.entries, &output)?;
            if let Some(map) = &self.source_map {
                std::fs::write(output.join("app.plan.map.json"), map).map_err(|e| e.to_string())?;
            }
            for (name, bytes) in [
                ("app.plan", &self.plan[..]),
                ("app.js", &self.script),
                ("app.hbc", &self.bytecode),
                ("app.contract.d.ts", self.declarations.as_bytes()),
                ("app.sources.json", &self.source_inputs),
                ("app.module.json", self.receipt.as_bytes()),
            ] {
                std::fs::write(output.join(name), bytes).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&output);
        }
        result
    }
}
