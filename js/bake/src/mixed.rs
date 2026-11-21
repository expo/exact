//! The bakes of an app with a Rust data source beside app.ts (LLP 1027.002): each source keeps its own identity and
//! grants, and Rust answers the sources it owns.

use crate::{android, build_sources, library, BakeError, Composer};
use contract::DataSource;
use exact_js::Module;
use std::path::Path;

/// Whether a Rust source's type hands out host work (LLP 1047.001 D12), for
/// the mixed bakes' `host_work`.
pub use exact_runner::hands_out_work;

/// Bake a mixed app using each source's own identity and grants. Metadata reads
/// never invoke or activate the Rust implementation; first-frame values bake
/// through TypeScript alone. `host_work`: the Rust source's
/// `exact_bake::hands_out_work!` (LLP 1047.001 D12).
pub fn build_mixed(
    app: &Path,
    platform: &str,
    rust: &dyn DataSource,
    host_work: bool,
) -> Result<(), BakeError> {
    library::link(app, platform, false).map_err(BakeError)?;
    exact_bake::link_windows_resources();
    build_sources(app, platform, Some((rust, host_work)), None, false).map_err(BakeError)
}

/// Bake a mixed app through its own composer (LLP 1027.002 §5 step 0): the
/// bake's TypeScript module is composed with the app's Rust source by
/// `compose`, so a resource only Rust owns bakes its first-frame value
/// through Rust, with no TypeScript placeholder. `rust_sources` names what
/// Rust owns, for the TypeScript provider types and the receipt the
/// development producer seeds from. `host_work`: the Rust source's
/// `exact_bake::hands_out_work!` (LLP 1047.001 D12).
///
/// The source's type is the one app.json's `rust.data` names, when it names one (LLP 1153.003 D2): the web build
/// runs that one.
pub fn build_mixed_with<D: DataSource>(
    app: &Path,
    platform: &str,
    rust: &D,
    host_work: bool,
    rust_sources: &[&str],
    compose: impl Fn(Module) -> Box<dyn DataSource>,
) -> Result<(), BakeError> {
    let manifest = contract::Manifest::read(app).map_err(BakeError)?;
    if let Some(declared) = manifest.rust_data().map_err(BakeError)? {
        let passed = std::any::type_name::<D>();
        if passed != declared.ty {
            return Err(BakeError(format!(
                "app.json's rust.data names the source type `{}`, and this build passes a `{passed}`: pass the source the manifest names, which the web build also runs",
                declared.ty
            )));
        }
    }
    library::link(app, platform, false).map_err(BakeError)?;
    exact_bake::link_windows_resources();
    let composer = Composer {
        rust_sources: rust_sources.iter().map(|s| s.to_string()).collect(),
        compose: Box::new(compose),
    };
    build_sources(
        app,
        platform,
        Some((rust, host_work)),
        Some(composer),
        false,
    )
    .map_err(BakeError)
}

/// A generated host crate's bake for an app with app.ts beside its Rust data crate (LLP 1153.003 D2): `rust`
/// answers `rust_sources` and app.ts the rest. `compose` is given the TypeScript module and the names of the sources
/// it answers, as the crate composes them at run time (`TYPESCRIPT_SOURCES`, which the bake writes into
/// `module.rs`). `host_work`: `rust`'s [`hands_out_work!`]. On `android`, the Canvas host's bake, as
/// `build_android` makes it.
pub fn build_beside<D: DataSource>(
    app: &Path,
    platform: &str,
    rust: &D,
    host_work: bool,
    rust_sources: &[&str],
    compose: impl Fn(Module, &[&str]) -> Box<dyn DataSource>,
) -> Result<(), BakeError> {
    let bake = |app: &Path, platform: &str| {
        let owned: Vec<String> = rust_sources.iter().map(|s| s.to_string()).collect();
        // A plan that does not compile is the bake's to report, with its excerpts.
        let typescript = contract::compile_path(&app.join("app.contract"))
            .map(|plan| contract::typescript_sources(&plan, &owned))
            .unwrap_or_default();
        let typescript: Vec<&str> = typescript.iter().map(String::as_str).collect();
        build_mixed_with(app, platform, rust, host_work, rust_sources, |javascript| {
            compose(javascript, &typescript)
        })
    };
    if platform == "android" {
        return android::build_android_with(app, |app| bake(app, "linux"));
    }
    bake(app, platform)
}
