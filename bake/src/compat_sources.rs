use super::{compatibility_id_inner, Compat};
use contract::Manifest;
use std::path::Path;

/// The compatibility id of the app at `app_dir` built for `platform`
/// (`ios`, `macos`, `linux`, `windows`, `web`) and `target` (the triple), with the
/// data crate's grants when the caller has a `DataSource` to ask (the bake
/// does; command-line callers can supply the declarations from its receipt),
/// and whether that source hands the executor work no grant names
/// ([`crate::hands_out_work!`], LLP 1047.001 D12).
pub fn compatibility_id(
    app_dir: &Path,
    platform: &str,
    target: &str,
    manifest: &Manifest,
    grants: Option<&str>,
    host_work: bool,
) -> Result<Compat, String> {
    compatibility_id_sources(app_dir, platform, target, manifest, grants, None, host_work)
}

/// Compatibility for separately owned language sources. With `rust_grants`,
/// `grants` describes JavaScript; the host admits their union and hashes each
/// child's exact grant declaration independently. Grants that do not parse
/// refuse the bake, whichever language declared them.
pub fn compatibility_id_sources(
    app_dir: &Path,
    platform: &str,
    target: &str,
    manifest: &Manifest,
    grants: Option<&str>,
    rust_grants: Option<&str>,
    host_work: bool,
) -> Result<Compat, String> {
    compatibility_id_sources_with_entries(
        app_dir,
        platform,
        target,
        manifest,
        grants,
        rust_grants,
        None,
        host_work,
    )
}

/// Reuse entry plans already compiled for source declarations and refresh checks.
/// With `None`, compatibility generation compiles the manifest's entries itself.
#[allow(clippy::too_many_arguments)]
pub fn compatibility_id_sources_with_entries(
    app_dir: &Path,
    platform: &str,
    target: &str,
    manifest: &Manifest,
    grants: Option<&str>,
    rust_grants: Option<&str>,
    entries: Option<&std::collections::BTreeMap<String, contract::snapshot::Entry>>,
    host_work: bool,
) -> Result<Compat, String> {
    compatibility_id_pinned(
        app_dir,
        platform,
        target,
        manifest,
        grants,
        rust_grants,
        entries,
        None,
        &serde_json::Value::Null,
        host_work,
    )
}

/// As [`compatibility_id_sources_with_entries`], with the TypeScript module's
/// receipt and environment. A receipt requires `module.sha256`; with a store,
/// it enters the cohort as `typescriptModule`. This helper does not admit
/// replacement module pairs: delivery checks runtime, identity, grants and
/// module policy. Android also records `hermesBytecode` in the cohort because
/// a device discovers a version mismatch only after download (LLP 1030.006 D4).
/// `host_work` is the Rust source's [`crate::hands_out_work!`]; the module's
/// is its receipt's `typescriptHostWork` (LLP 1047.001 D12).
#[allow(clippy::too_many_arguments)]
pub fn compatibility_id_pinned(
    app_dir: &Path,
    platform: &str,
    target: &str,
    manifest: &Manifest,
    grants: Option<&str>,
    rust_grants: Option<&str>,
    entries: Option<&std::collections::BTreeMap<String, contract::snapshot::Entry>>,
    module: Option<&serde_json::Value>,
    settings: &serde_json::Value,
    host_work: bool,
) -> Result<Compat, String> {
    compatibility_id_inner(
        app_dir,
        (platform, target),
        manifest,
        grants,
        rust_grants,
        host_work,
        Sources {
            entries,
            module,
            settings: Some(settings),
            ..Default::default()
        },
    )
}

#[derive(Default)]
pub(crate) struct Sources<'a> {
    pub(super) entries: Option<&'a std::collections::BTreeMap<String, contract::snapshot::Entry>>,
    pub(super) module: Option<&'a serde_json::Value>,
    pub(super) settings: Option<&'a serde_json::Value>,
    pub(super) compiled: Option<(&'a exact_plan::Plan, Option<&'a contract::SourceMap>)>,
}

impl<'a> Sources<'a> {
    pub(crate) fn compiled(
        plan: &'a exact_plan::Plan,
        map: Option<&'a contract::SourceMap>,
    ) -> Self {
        Self {
            compiled: Some((plan, map)),
            ..Self::default()
        }
    }
}
