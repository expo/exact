//! The compatibility id: the cohort a bundle is safe for (LLP 1030 D3a).
//!
//! @ref LLP 1030 D3a (the id and what enters it) / D2 (policy is not
//! identity) / D4 (the two axes, `L` and `E`); LLP 1030.000 stage 3
//!
//! A bundle may depend on things an installed binary cannot replace — the
//! kernel schema, the plan format, the ABI numbers, the executors linked,
//! the native data crate, the surfaces' shaders, the icons and host
//! capabilities a plan may select, the store's shape. Two binaries that
//! agree on all of them can take the same bundle; two that differ cannot,
//! however alike their version strings. The id is the digest of exactly
//! those inputs, computed by the bake per platform and written beside the
//! plan, so an update stream is named by it and a static origin serves each
//! cohort its own manifest with no negotiation.
//!
//! What must not move it (D3a, D2): `[deploy]` — schedules, channels, the
//! release policy — and a host change a bundle cannot observe. The inputs
//! ride along as JSON so two ids that differ can be explained field by
//! field (`dev.mjs`, `exact deploy`), and a field that has no value yet is
//! `null`, so the id is stable until the thing exists.

use crate::path_crates::path_crates;
use contract::Manifest;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The C ABI's header, whose `EXACT_ABI_VERSION` is one of the numbers.
const ABI_HEADER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../host/apple/include/exact.h"
));
/// The GPU module's C ABI (LLP 1009 D2): unnumbered in the module today.
pub const GPU_MODULE_ABI: u32 = 1;
/// The separately linked Rust data-source wire ABI, as the seam declares it:
/// admitted secret names before the store snapshot (LLP 1048.000 D9), two-way
/// sockets (LLP 1016.003), cache-mode tag 12 (LLP 1162 D4), stream
/// protocol names (LLP 1016.000), preload-name tag 13 (LLP 1162.000 D1),
/// prefetch tag 14 (D5) and held-mark tag 15 (LLP 1016.000 D9).
pub(crate) const RUST_ABI: u32 = exact_logic_abi::ABI;
/// The data-module runtime a TypeScript bundle may rely on in the binary's
/// own prelude (`js/src/prelude.js`, compiled into the host): moved when the
/// prelude learns something a bundle cannot do without and an older prelude
/// would silently ignore. 1: `fetch`'s `exactBodyFrom` (LLP 1108 D6 R2),
/// which an older prelude would send as an empty body. 2: `fetch`'s
/// `redirect`, which an older prelude would ignore and follow. 3: `fetch`'s
/// `exactLongPoll` (LLP 1016.000 D8), which an older prelude would ignore and
/// open as an event stream. 4: `fetch`'s `exactBodyTo` (LLP 1108 D6 R2),
/// which an older prelude would ignore, reading the body into memory and
/// writing no file. 5: an aborted `fetch`'s request cancelled on the wire and
/// its answer resumed at the abort (docs/reference.md), which an older
/// prelude would leave running, its reply (a 429 included) dropped; and,
/// sharing it (no binary had shipped 5), an aborted kept write's rejection in
/// a module turn (LLP 1146 D2), which an older prelude would run in the
/// aborting turn, where a refused commit drops its continuation's writes; and,
/// sharing it as well, `fetch`'s `exactUploadProgress` (LLP 1016.000 D10),
/// which an older prelude would ignore, giving the mapper no progress and the
/// reply as a byte stream's messages. 6: `storage.fs`'s `lstat`, `readlink`,
/// `access`, `chmod`, `symlink`, `digest` and `flock`, and `stat`'s `mode`
/// and `isSymbolicLink` (LLP 1133.007 D6a), which an older prelude lacks: a
/// call throws a plain `TypeError` rather than `NotSupportedError`, and the
/// fields read `undefined`. The operations work on `app:/` paths under an
/// existing `fs` grant, so the grant ceiling does not keep them from an
/// older binary. A module that reads only `stat().mode` names nothing below
/// and is not detected. 7: Fetch's `Response`: data code's own (its
/// constructor, `Response.json`, `Response.redirect`), a reply's `clone()`,
/// `statusText` and `bodyUsed`, and a body read once, which an older prelude
/// would make with no status and no body, refuse, or answer `undefined`.
/// 8: `Response` consumes a `ReadableStream` body and `fetch` refuses one,
/// where older preludes stringify the stream.
/// 9: `fetch`'s `exactIdleTimeout` (LLP 1016.000 D11), which an older
/// prelude would ignore, leaving a quiet socket open.
/// 10: `storage.sqlite`'s `transaction(commands, { foreignKeys: "check" })`,
/// whose options an older prelude drops, running a migration's rebuild with
/// enforcement on: the drop deletes a cascading child's rows and the batch
/// commits. The options can come from data (a migration read from JSON) and
/// the method through an alias, so no text in the bundle shows the call: a
/// module whose grants admit `sqlite.open` needs 10 ([`typescript_runtime_of`]).
/// 11: Fetch's `cache` (LLP 1162 D4), which an older prelude ignores.
/// 12: `exactProtocols`, which an older prelude would ignore (LLP 1016.000).
/// 13: `exactPreload` and `exactPrefetch`, which an older prelude ignores
/// (LLP 1162.000 D1, D5).
/// 14: the settings a launch may give (LLP 1153.002), which the bundle's
/// launch module asks the prelude for (`__exact_launch_settings`) before any
/// other module runs: an older prelude has none, and gives the module the
/// build's value whatever the launch set.
/// 15: `exactIdleTimeout` on an event stream or a byte stream (LLP 1016.000
/// D11), which a prelude of 9 to 14 refuses, failing the stream. A bundle
/// names the option alike on a socket, so every use needs 15.
/// 16: per-source turn budgets. Older native hosts give every source 100 ms.
pub(crate) const TYPESCRIPT_RUNTIME: u32 = 16;

/// What each runtime past the first added that a bundle names to use (see
/// [`TYPESCRIPT_RUNTIME`]): a `fetch` option an older prelude would ignore,
/// or, for 5's abort changes, what makes an abort signal. `mode` and
/// `access` are left out of 6's: they would match far more than D6a. 7's are
/// the `Response` itself and what it newly has, however a bundle reaches them
/// (`globalThis.Response`, a `clone` taken apart and called through `.call`).
/// `Response` now needs 8, as do constructors whose readable side can become
/// a request body. `structuredClone` names none of them. 10 is read from the
/// grants, not the bundle.
const RUNTIME_OPTIONS: [(u32, &str); 29] = [
    (16, "turnBudgetsJson"),
    (15, "exactIdleTimeout"),
    (14, "__exact_launch_settings"),
    (13, "exactPrefetch"),
    (13, "exactPreload"),
    (12, "exactProtocols"),
    (11, "cache"),
    (8, "Response"),
    (8, "ReadableStream"),
    (8, "TransformStream"),
    (8, "CompressionStream"),
    (8, "DecompressionStream"),
    (7, "clone"),
    (7, "statusText"),
    (7, "bodyUsed"),
    (6, "isSymbolicLink"),
    (6, "symlink"),
    (6, "readlink"),
    (6, "lstat"),
    (6, "chmod"),
    (6, "digest"),
    (6, "flock"),
    (5, "exactUploadProgress"),
    (5, "AbortController"),
    (5, "AbortSignal"),
    (4, "exactBodyTo"),
    (3, "exactLongPoll"),
    (2, "redirect"),
    (1, "exactBodyFrom"),
];

/// The names in a TypeScript bundle that can leave an answer pending past its
/// turn, for the executor to resume (LLP 1047.001 D12): the prelude's timers,
/// `setTimeout` and `AbortSignal.timeout` (`js/src/time.js`), and
/// `scheduler.yield()`. A native slot's `native.later()` is the bake's to
/// report (it knows the slot); every other wait is on a grant's work (fetch,
/// storage, secrets, listeners), which the grant links.
const HOST_WORK_NAMES: [&str; 3] = ["setTimeout", "AbortSignal", "scheduler"];

/// Whether a TypeScript `bundle` can hand the executor work no grant names: it
/// names one of [`HOST_WORK_NAMES`] anywhere. A name in a string or a comment
/// counts too: such a module links I/O it may not need. A name the module
/// builds at run time (`globalThis["set" + "Timeout"]`) is not seen; the host
/// refuses that work by name.
pub fn typescript_host_work(bundle: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bundle);
    HOST_WORK_NAMES.iter().any(|name| text.contains(name))
}

/// The lowest runtime a TypeScript `bundle` needs: the newest whose option
/// it names anywhere (a property name survives bundling), else none. A push
/// records it, so a module that uses nothing newer still goes to a binary
/// whose prelude is older (LLP 1016.000 D10). A name in a string or a
/// comment counts too: such a module asks for more than it needs, never less.
/// A module whose `grants` admit `sqlite.open` needs 10: only such a module
/// can open a database (the host refuses any other), and none of its code
/// can show whether a transaction's options will ask to check foreign keys.
/// A line's capability is its first word, as `exact-grants` parses it.
pub fn typescript_runtime_of(bundle: &[u8], grants: &str) -> u32 {
    let text = String::from_utf8_lossy(bundle);
    let named = RUNTIME_OPTIONS
        .iter()
        .find(|(_, option)| text.contains(option))
        .map_or(0, |(runtime, _)| *runtime);
    let sqlite = grants.lines().map(str::trim).any(|line| {
        !line.starts_with('#') && line.split_whitespace().next() == Some("sqlite.open")
    });
    if sqlite {
        named.max(10)
    } else {
        named
    }
}
/// The domain separator over the canonical inputs.
const DOMAIN: &str = "exact2 compatibility id v1\n";

/// The id and the inputs it digests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compat {
    /// 16 bytes of the domain-separated SHA-256 over the canonical inputs, lowercase hex.
    pub id: String,
    /// The inputs, one named field each, `null` where a thing does not exist yet.
    pub inputs: serde_json::Value,
    /// The channel this build bakes in and its origin (LLP 1030.000 D4) —
    /// beside the id, not in it: where the binary's update store checks.
    pub channel: String,
    /// The channel's origin URL, if the manifest names one.
    pub origin: Option<String>,
    /// When a staged bundle applies (`deploy.activate`): `next-launch` or
    /// `app-decides` — policy the binary's store follows (LLP 1030.000 D4).
    pub activate: String,
    /// Seconds between foreground checks (`deploy.check.interval`), when the
    /// manifest names it: what a painted host reads (LLP 1030.006 D5); the
    /// Apple hosts read it from Info.plist.
    pub check_interval: Option<serde_json::Number>,
    /// The exact target the binary-producing Cargo bake received.
    pub target: String,
    /// Embedded bundle provenance and its complete static roster, and
    /// `trust`: the build's resolved trust, which every host's production
    /// policy reads. Beside the id, not in it: an updater-free (`L = 0`)
    /// cohort has no trust epoch, so `inputs.trust` is null there and a
    /// production and a development build share the id (LLP 1030 D4).
    pub embedded: serde_json::Value,
    /// What the app can reach and each host's spelling of it (LLP 1069.008
    /// D7): beside the id, not in it; `null` when the grants are unknown.
    pub reach: serde_json::Value,
    /// The binary's painter choice, outside the compatibility hash (LLP 1030 D3a).
    pub painter: Option<&'static str>,
    /// The manifest declares the game engine; outside the compatibility hash (LLP 1149 D12).
    pub game: bool,
    /// The app's sources hand the executor work no grant names (LLP 1047.001
    /// D12); outside the hash, which the linked set it selects is in.
    pub host_work: bool,
}

impl Compat {
    /// Whether this binary admits the development driver, including when its store level is zero.
    pub fn admits_agent(&self) -> bool {
        self.embedded["trust"] == "development"
    }

    /// `{"id":…,"inputs":{…},"delivery":{"activate":…,"channel":…,"origin":…}}`,
    /// canonical (sorted keys within each value), the id first, one line plus
    /// a newline. The runner reads `id`, `store.L`, and `executors` from this
    /// text by hand (`exact_runner::Delivery::with_compat`); a host's update
    /// store reads the rest (`exact_update::Baked::from_compat`).
    pub fn to_json(&self) -> String {
        let mut s = String::from("{\"id\":");
        canonical(&serde_json::Value::String(self.id.clone()), &mut s);
        s.push_str(",\"inputs\":");
        canonical(&self.inputs, &mut s);
        s.push_str(",\"delivery\":");
        canonical(
            &match &self.check_interval {
                Some(interval) => serde_json::json!({
                    "activate": self.activate,
                    "channel": self.channel,
                    "checkInterval": interval,
                    "origin": self.origin.clone().map_or(serde_json::Value::Null, serde_json::Value::String),
                }),
                None => serde_json::json!({
                    "activate": self.activate,
                    "channel": self.channel,
                    "origin": self.origin.clone().map_or(serde_json::Value::Null, serde_json::Value::String),
                }),
            },
            &mut s,
        );
        s.push_str(",\"target\":");
        canonical(&serde_json::json!(self.target), &mut s);
        s.push_str(",\"embedded\":");
        canonical(&self.embedded, &mut s);
        s.push_str(",\"reach\":");
        canonical(&self.reach, &mut s);
        if let Some(painter) = self.painter {
            s.push_str(",\"painter\":");
            canonical(&serde_json::json!(painter), &mut s);
        }
        if self.game {
            s.push_str(",\"game\":true");
        }
        s.push_str("}\n");
        s
    }
}

/// The compatibility id of the app at `app_dir` built for `platform`
/// (`ios`, `macos`, `linux`, `web`) and `target` (the triple), with the
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

/// `deploy.store.<platform>`, but `"0"` for a Mac App Store build (`EXACT_APPSTORE`,
/// which `host/apple/build.mjs --appstore` sets): App Review Guideline 2.4.5(vii)
/// allows updates only through the store, so it links no update store and app.json
/// keeps its value for the Developer ID build (LLP 1030.005 D3).
fn store_level(manifest: &Manifest, platform: &str) -> String {
    let appstore = std::env::var_os("EXACT_APPSTORE").is_some_and(|v| v == "1");
    store_level_of(manifest, platform, appstore)
}

/// [`store_level`] with the Mac App Store build's switch given.
pub(crate) fn store_level_of(manifest: &Manifest, platform: &str, appstore: bool) -> String {
    if appstore && platform == "macos" {
        return "0".into();
    }
    manifest.store(platform)
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
    // A receipt that does not say links the executor: it cannot be shown unneeded.
    let host_work =
        host_work || module.is_some_and(|m| m["typescriptHostWork"].as_bool() != Some(false));
    // Cargo must rebake even when only the explicit trust selection changes.
    // External apps call this same entrypoint from their own build scripts.
    if std::env::var_os("OUT_DIR").is_some() {
        println!("cargo:rerun-if-env-changed=EXACT_UPDATE_TRUST");
        println!("cargo:rerun-if-env-changed=EXACT_APPSTORE");
        println!("cargo:rerun-if-env-changed=EXACT_VERSION");
        println!("cargo:rerun-if-env-changed=EXACT_BUILD_NUMBER");
        println!("cargo:rerun-if-env-changed=EXACT_TYPESCRIPT_PLACEMENT");
        println!("cargo:rerun-if-env-changed=EXACT_RUST_PLACEMENT");
    }
    let trust = match std::env::var("EXACT_UPDATE_TRUST") {
        Ok(value) => value,
        // Unset is development: such a binary checks only an origin the
        // developer names (EXACT_UPDATE_ORIGIN), never the manifest's.
        Err(std::env::VarError::NotPresent) => "development".into(),
        Err(_) => return Err("EXACT_UPDATE_TRUST is not UTF-8".into()),
    };
    crate::reach::parsed(app_dir, grants, rust_grants, trust == "production")?;
    let ceiling = rust_grants.map(|rust| grant_union(grants.unwrap_or(""), rust));
    // Android uses the Linux bake; its launch parts read the build fallback (LLP 1153.002 D2).
    let preloads = crate::preload::checked(
        app_dir,
        manifest,
        ceiling.as_deref().or(grants),
        settings,
        platform != "web",
    )?;
    // @ref LLP 1100 D1 — a colour this platform can't show is refused here.
    // The app's plan, compiled once here: its colours checked, and (stage
    // 14 C2) its icons' fill rules read below.
    let plan = crate::colors::compiled(app_dir)?;
    if let Some(plan) = &plan {
        crate::colors::check_plan(app_dir, plan, platform)?;
    }
    let mut compat = compatibility_with_trust(
        app_dir,
        platform,
        target,
        manifest,
        ceiling.as_deref().or(grants),
        &trust,
    )?;
    compat.host_work = host_work;
    if preloads
        .as_array()
        .is_some_and(|entries| !entries.is_empty())
    {
        compat.inputs["preload"] = preloads;
        compat.id = compatibility_digest(&compat.inputs);
    }
    if let Some(rust) = rust_grants {
        source_scopes(&mut compat, grants.unwrap_or(""), rust);
    }
    if let Some(meta) = module {
        let sha = meta["module"]["sha256"]
            .as_str()
            .ok_or("missing module hash")?;
        if compat.inputs["store"]["L"] != "0" {
            compat.inputs["typescriptModule"] = serde_json::Value::String(sha.into());
        }
        if target.contains("android") {
            compat.inputs["hermesBytecode"] = meta["bytecodeVersion"].clone();
        }
        compat.id = compatibility_digest(&compat.inputs);
    }
    // The origins its declared sources carry are the binary's grants too
    // (LLP 1162.001 D3): an update that declares another is a new cohort.
    if let (Some(plan), Some(ceiling)) = (&plan, compat.inputs["grantCeiling"].as_str()) {
        let ceiling = exact_runner::grants::with_declared(ceiling, plan);
        if compat.inputs["grantCeiling"] != ceiling.as_str() {
            compat.inputs["grantCeiling"] = serde_json::Value::String(ceiling);
            compat.id = compatibility_digest(&compat.inputs);
        }
    }
    let sync = manifest.watch()?.map(|w| w.sync).unwrap_or_default();
    contract::watch_sync::grants(&sync, compat.inputs["grantCeiling"].as_str().unwrap_or(""))?;
    compat.inputs["watchSync"] = contract::watch_sync::identity(&sync, plan.as_ref())?;
    compat.id = compatibility_digest(&compat.inputs);
    crate::reach::derive(app_dir, platform, plan.as_ref(), &mut compat)?;
    localizations(platform, &mut compat);
    crate::sandbox::derive(app_dir, platform, plan.as_ref(), &mut compat);
    if target.contains("android") {
        crate::channels::derive(app_dir, &manifest.json, &mut compat)?;
    }
    // The plans that ship, each resolved once (the receipt writes the entry
    // plans): an even-odd icon in any moves the id (stage 14 C2,
    // icon_rules.rs), and one that binds `atomFaces` names its record's
    // fields (atom_faces.rs).
    let mut owned = None;
    let entries = crate::icon_rules::entries(app_dir, entries, &mut owned)?;
    let out = std::env::var_os("OUT_DIR").map(std::path::PathBuf::from);
    let other = crate::icon_rules::shipped(app_dir, plan.as_ref(), out.as_deref())?;
    let ships = other.as_ref().or(plan.as_ref());
    // From the plan that ships, which the APK's build reads: a TypeScript
    // bake's marks the resources its module answers, which link the host's
    // I/O, and the `app.contract` plan alone does not.
    if let (true, Some(ships)) = (target.contains("android"), ships) {
        compat.inputs["androidLinked"] = android_linked(
            ships,
            compat.inputs["grantCeiling"].as_str(),
            &contract::native::app_text(app_dir),
        );
        compat.id = compatibility_digest(&compat.inputs);
    }
    crate::icon_rules::apply(&mut compat, ships, entries)?;
    crate::atom_faces::apply(&mut compat, ships, entries);
    if !target.contains("watchos") {
        crate::icon_rules::raster_inputs(
            app_dir,
            &mut compat.inputs,
            ships
                .into_iter()
                .chain(entries.values().map(|entry| &entry.plan)),
        )?;
        compat.id = compatibility_digest(&compat.inputs);
    }
    if let Some(plan) = &plan {
        faces(&mut compat, plan, platform, target);
        crate::controls::derive(&mut compat, plan, manifest, platform, target)?;
    }
    if let Some(out) = std::env::var_os("OUT_DIR") {
        // A native artifact's composition is part of its cohort (LLP
        // 1047.001 D2), named before anything binds the id.
        if platform != "web" {
            crate::link::name_into(&mut compat, app_dir, manifest, Path::new(&out), entries)?;
        }
        crate::receipt::emit(
            &mut compat,
            &trust,
            app_dir,
            (platform, target),
            manifest,
            Path::new(&out),
            entries,
        )?;
    }
    Ok(compat)
}

/// What an APK is composed from beyond the compatibility inputs' other
/// fields (LLP 1030.006 D4a): the plan's capabilities (`uses.txt`, which
/// decides what the reader links), the Android permissions its grant lines
/// and the source's use of module names derive (`permissions.txt`), and the
/// package extensions it binds a Kotlin implementation for. A generation
/// that needs more than this is a new APK; the hatch words are `hatches`.
pub(crate) fn android_linked(
    plan: &exact_plan::Plan,
    ceiling: Option<&str>,
    source: &str,
) -> serde_json::Value {
    serde_json::json!({
        "uses": android_uses(plan),
        "permissions": contract::native::android_permissions(plan, ceiling, source),
        "extensions": plan_extensions(plan),
    })
}

/// What the APK links for the plan, by name, as its build reads it
/// (`contract::native::android_reach`).
pub(crate) fn android_uses(plan: &exact_plan::Plan) -> Vec<&'static str> {
    contract::native::android_uses(plan)
}

/// The package extensions the plan's modules, views and events name, sorted.
pub(crate) fn plan_extensions(plan: &exact_plan::Plan) -> Vec<String> {
    let names = plan
        .modules
        .iter()
        .map(|m| m.extension)
        .chain(plan.views.iter().map(|v| v.extension))
        .chain(plan.module_events.iter().map(|e| e.extension));
    let names: std::collections::BTreeSet<String> =
        names.map(|name| plan.str(name).to_string()).collect();
    names.into_iter().collect()
}

/// The faced tools this platform's binary compiles a door for (LLP 1133.023
/// D8), and on the Mac those its tools helper lists (LLP 1111.000 D1): each
/// one's signature and face join the id, so a plan that changes one is a
/// binary change. tvOS (an `ios` bake for a tvOS target) and the web
/// compile none.
fn faces(compat: &mut Compat, plan: &exact_plan::Plan, platform: &str, target: &str) {
    let doors: &[&str] = match platform {
        _ if target.contains("tvos") => return,
        "ios" => &["apple"],
        "macos" => &["apple", "mcp"],
        "android" => &["android"],
        _ => return,
    };
    let faced: Vec<serde_json::Value> = (0..plan.tools.len())
        .filter_map(|i| {
            let face: serde_json::Value = serde_json::from_str(plan.str(plan.tools[i].face)).ok()?;
            let named = face["doors"].as_array()?;
            named.iter().any(|d| doors.iter().any(|door| d == door)).then(|| serde_json::json!({ "signature": contract::faces::tool_signature(plan, i), "face": face }))
        })
        .collect();
    if !faced.is_empty() {
        compat.inputs["faces"] = serde_json::json!(faced);
        compat.id = compatibility_digest(&compat.inputs);
    }
}

fn grant_union(javascript: &str, rust: &str) -> String {
    let union = [javascript, rust]
        .into_iter()
        .flat_map(str::lines)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n");
    // Match Mixed::grants: retain the existing receipt spelling when Rust adds
    // no capability; otherwise the combined ceiling is deterministic.
    if union
        .lines()
        .all(|line| javascript.lines().map(str::trim).any(|grant| grant == line))
    {
        javascript.into()
    } else {
        union
    }
}

fn source_scopes(compat: &mut Compat, javascript: &str, rust: &str) {
    compat.inputs["grantCeiling"] = serde_json::Value::String(grant_union(javascript, rust));
    compat.inputs["javascriptGrants"] = serde_json::Value::String(javascript.into());
    compat.inputs["rustGrants"] = serde_json::Value::String(rust.into());
    compat.id = compatibility_digest(&compat.inputs);
}

/// The Apple bundle lists the strings tables as its localizations
/// (`localizationKeys` in `host/apple/build.mjs`), and the system and
/// `SessionFacts.viewerLocale` choose the app's table from that list, so the
/// tables' names are in the id there (LLP 1030 D3a); their text is not. An
/// app with one table records nothing, so its id did not move when this was
/// added.
fn localizations(platform: &str, compat: &mut Compat) {
    let tables = compat.reach["locales"].as_array().map_or(0, Vec::len);
    if tables > 1 && matches!(platform, "ios" | "macos") {
        compat.inputs["localizations"] =
            serde_json::json!({"base": compat.reach["base"], "locales": compat.reach["locales"]});
        compat.id = compatibility_digest(&compat.inputs);
    }
}

pub(crate) fn compatibility_digest(inputs: &serde_json::Value) -> String {
    let mut canon = String::new();
    canonical(inputs, &mut canon);
    let mut h = Sha256::new();
    h.update(DOMAIN.as_bytes());
    h.update(canon.as_bytes());
    let digest = h.finalize();
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

fn commands_digest() -> String {
    let mut h = Sha256::new();
    for name in contract_syntax::HOST_COMMANDS {
        let shape = match *name {
            "share" => "3",
            "stopSounds" => "1",
            "setSounds" => "5",
            "playSounds" => "1:4|5|6",
            "playSound" => "6",
            "sendMIDI" => "3",
            "scrollIntoView" => "4|7",
            "saveFile" => "3|4",
            _ => "positional",
        };
        h.update(format!("{name}:{shape}\n"));
    }
    h.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn compatibility_with_trust(
    app_dir: &Path,
    platform: &str,
    target: &str,
    manifest: &Manifest,
    grants: Option<&str>,
    trust: &str,
) -> Result<Compat, String> {
    use serde_json::{json, Value};
    if !matches!(trust, "production" | "development") {
        return Err("EXACT_UPDATE_TRUST must be production or development".into());
    }
    if platform == "web"
        && manifest
            .json
            .pointer("/deploy/store/web")
            .is_some_and(|value| value.as_str() != Some("0"))
    {
        return Err("the web host links no updater; deploy.store.web must be \"0\"".into());
    }
    // An Android library bakes through the Linux host's code generation (it
    // is that host's presenter under the reader's Canvas), so its callers
    // name `linux`; its record names `android`, whose host settings, hatch
    // words and store level are its own (LLP 1030.006 D2). An absent
    // `deploy.store.android` is "0", not the native default of "A": Android
    // apps shipped binary-only, and "A" asks a production build for keys and
    // a stream.
    let android = target.contains("android");
    let platform = if android { "android" } else { platform };
    let store = if android {
        match manifest.json.pointer("/deploy/store/android") {
            None => "0".to_string(),
            Some(value) => value.as_str().unwrap_or("").to_string(),
        }
    } else {
        store_level(manifest, platform)
    };
    if !matches!(store.as_str(), "0" | "A") {
        return Err(format!("deploy.store.{platform} must be \"0\" or \"A\""));
    }
    let binary_only = store == "0";
    let keys = if binary_only {
        Value::Null
    } else {
        manifest.keys()?
    };
    let (channel, origin) = if binary_only {
        (String::new(), None)
    } else {
        manifest.channel()?
    };
    if trust == "production"
        && (store != "0" || origin.is_some())
        && !binary_only
        && keys.as_object().is_none_or(|keys| keys.is_empty())
    {
        return Err("production updater requires deploy.signing.keys with at least one verification key; use EXACT_UPDATE_TRUST=development only for a development artifact".into());
    }
    let host = manifest.host(platform);
    let rust_module = manifest
        .json
        .pointer("/rust/module/package")
        .and_then(Value::as_str);
    // An executor runs a replacement of the module the manifest names; an
    // app that names none has nothing to replace, so it links none
    // (LLP 1047.001 D7, as `web_rust_mode` already does on the web). The
    // policy is still validated.
    // The APK links no Rust executor (`rules/DEFERRED.md` keeps Android below
    // the replaceable Rust module), so its record admits no Rust pair whatever
    // the manifest's policy (LLP 1030.006 D1).
    let rust_mode = match manifest.rust_mode(platform, trust == "development")? {
        _ if rust_module.is_none() || android => "off",
        mode => mode,
    };
    let mut executors = executors(app_dir, platform);
    let rust_target = match rust_mode {
        "tiered" => {
            executors.push("native-dylib".into());
            executors.push("wasmi".into());
            Some(target)
        }
        "native" => {
            executors.push("native-dylib".into());
            Some(target)
        }
        "wasm" => {
            executors.push("wasmi".into());
            Some("wasm32-unknown-unknown")
        }
        "browser" => {
            executors.push("browser-wasm".into());
            Some("wasm32-unknown-unknown")
        }
        _ => None,
    };
    executors.sort();
    executors.dedup();
    let hermes = executors.iter().any(|e| e == "hermes");
    let wasmi = executors.iter().any(|e| e == "wasmi");
    let mut kinds = vec!["plan", "assets"];
    if hermes {
        kinds.push("bytecode");
    }
    if wasmi {
        kinds.push("wasm");
    }
    let list = |key: &str| -> Value {
        match host.get(key).and_then(|v| v.as_array()) {
            Some(items) => {
                let mut v: Vec<String> = items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect();
                v.sort();
                json!(v)
            }
            None => Value::Null,
        }
    };
    let mut icons: Vec<String> = manifest
        .json
        .get("icons")
        .and_then(|i| i.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.get("src").and_then(|s| s.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    icons.sort();
    let mut inputs = json!({
        "kernelSchema": format!("{:016x}", exact_kernel::SCHEMA_DIGEST),
        "formatVersion": exact_plan::FORMAT_VERSION,
        "formatDigest": format!("{:016x}", exact_plan::FORMAT_DIGEST),
        // @ref LLP 1114.001 D1 — an OTA plan with a changed fixed command
        // shape must not run in a binary that reads its old arguments.
        "commands": commands_digest(),
        "abi": { "c": abi_version()?, "gpuModule": GPU_MODULE_ABI, "storeCodec": if binary_only { Value::Null } else { json!(exact_update::STORE_CODEC) } },
        "executors": executors,
        "rustMode": rust_mode,
        "rustAbi": if rust_mode == "off" { Value::Null } else { json!(RUST_ABI) },
        "rustTarget": rust_target,
        "rustModule": rust_module,
        // Where each language's module runs (LLP 1027.002 §6): a host
        // without the executor refuses the build; a change is a new cohort.
        "typescriptPlacement": manifest.placement("typescript", platform)?,
        "rustPlacement": manifest.placement("rust", platform)?,
        // A binary that replaces its Rust module binds the module's ABI,
        // target and grants above, never the source it was first built
        // from: an edit there is an update, not a new cohort (LLP 1029.000 §7).
        "dataCrate": if rust_mode == "off" {
            let named = manifest.rust_data()?.map(|data| data.krate);
            data_crate(app_dir, named.as_deref())?
        } else {
            Value::Null
        },
        // Each shader's reflected interface digest (LLP 1030 D8) — entry
        // points, bindings, layouts, inputs, outputs, overrides, never the
        // text — so a colour edit does not move the id and a binding edit
        // does. Shaders are assets (1030.000 stage 1); the surface's Rust
        // binds this interface.
        "gpuSurfaces": gpu_surfaces(app_dir, manifest)?,
        // @ref LLP 1024 D3/D4 — the roster and the table's ABI: a new tag
        // is a binary change, never a bundle (LLP 1047 D8).
        "nativeModules": match contract::native::roster(manifest)? {
            tags if tags.is_empty() => Value::Null,
            tags => json!({ "appleAbi": 3, "webAbi": 1, "tags": tags }),
        },
        // The service extension is app-authored native code in the installed
        // cohort, not a replaceable module. Its complete tree therefore moves
        // compatibility when an ordinary build replaces its executable.
        "notificationService": notification_service(app_dir, platform, target)?,
        // @ref LLP 1075.003.000.001 §4.3 — the hatch words this platform's
        // module is built to handle, from app.json: the cohort's capability.
        "hatches": match contract::native::hatches_on(manifest, platform)? {
            words if words.is_empty() => Value::Null,
            words => json!(words),
        },
        "icons": icons,
        "capabilities": {
            "backgroundModes": list("backgroundModes"),
            "urlSchemes": list("urlSchemes"),
            "associatedDomains": host.get("associatedDomains").filter(|v| v.is_array()).cloned().unwrap_or(Value::Null),
            // @ref LLP 1096 D8 — the Apple audio session, baked into the binary.
            "audioSession": manifest.json.get("audio_session").cloned().unwrap_or(Value::Null),
        },
        // The verification keys the binary carries (LLP 1026 D11), by id: a
        // rotation is a new cohort (1030 D3a).
        "keys": keys,
        "trust": if binary_only { Value::Null } else { json!(trust) },
        "grantCeiling": grants.map_or(Value::Null, |g| Value::String(g.to_string())),
        "javascriptGrants": Value::Null,
        "rustGrants": grants.map_or(Value::Null, |g| Value::String(g.to_string())),
        "platform": platform,
        "arch": target.split('-').next().unwrap_or(target),
        "minimumOS": host.get("minimumOS").and_then(|v| v.as_str()).map_or(Value::Null, |s| Value::String(s.to_string())),
        "store": { "L": store, "acceptedKinds": if binary_only { vec![] } else { kinds } },
        "app": manifest.id,
    });
    // A variant's base identity, which a Rust source naming its own id names
    // (LLP 1030.008 D2); absent otherwise, so no other app's id moves.
    if let Some(variant) = manifest.json.get("variant") {
        inputs["variant"] = variant.clone();
    }
    if matches!(platform, "macos" | "ios") {
        // A share target's claim is `ExactSharing`'s (LLP 1133.024), which
        // the composition links for it; an app without one has no key.
        if manifest
            .json
            .get("share_target")
            .is_some_and(|t| !t.is_null())
        {
            inputs["shareTarget"] = Value::Bool(true);
        }
        if let Some(handlers) = manifest.json.get("protocol_handlers") {
            if !handlers.as_array().is_some_and(Vec::is_empty) {
                inputs["protocolHandlers"] = handlers.clone();
            }
        }
    }
    // @ref LLP 1163.000 D2 — the alternate icons' names, in their order, are
    // the iOS binary's: a plan never reaches a binary with another roster.
    // Their art is the binary's alone, so it is not an input here.
    // tvOS and watchOS bake as iOS but build no alternate (§7).
    if platform == "ios" && !target.contains("-tvos") && !target.contains("-watchos") {
        let names: Vec<&str> = (manifest.json.get("alternate_icons"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(|i| i["name"].as_str()).collect())
            .unwrap_or_default();
        if !names.is_empty() {
            inputs["alternateIcons"] = json!(names);
        }
    }
    if platform != "web" {
        let base = manifest
            .json
            .pointer("/host/web/base")
            .and_then(Value::as_str)
            .unwrap_or("/");
        if base != "/" {
            if let Some(origin) = manifest.json.pointer("/app/origin").and_then(Value::as_str) {
                inputs["webMount"] = serde_json::json!({"origin": origin, "base": base});
            }
        }
    }
    if let Some(links) = crate::links::claims(&manifest.json, platform) {
        inputs["links"] = links;
    }
    // Linux has no bundle beside the executable. Its portal picker reads the
    // same declared file types from the compatibility metadata the binary
    // already carries (LLP 1030 D2; LLP 1069.010 D2).
    if platform == "linux" && !target.contains("android") {
        if let Some(handlers) = manifest.json.get("file_handlers") {
            if !handlers.as_array().is_some_and(Vec::is_empty) {
                inputs["fileHandlers"] = handlers.clone();
            }
        }
    }
    // The icon's menu each native binary carries (LLP 1133.023 D3): iOS's
    // Info.plist, Android's `shortcuts.xml`, the Mac's Dock menu, Windows'
    // jump list and Linux's desktop entry read it from the build.
    if platform != "web" {
        if let Some(shortcuts) =
            (manifest.json.get("shortcuts")).filter(|s| s.as_array().is_some_and(|s| !s.is_empty()))
        {
            inputs["shortcuts"] = shortcuts.clone();
        }
    }
    let widgets = manifest.widgets()?;
    if !widgets.is_empty() {
        inputs["widgets"] = json!(widgets
            .iter()
            .map(|w| json!({"kind":w.kind,"families":w.families,"watchFamilies":w.watch_families,"requiresSecrets":w.requires_secrets,"title":w.title,"description":w.description,"configuration":w.configuration.as_ref().map(contract::Configuration::json)}))
            .collect::<Vec<_>>());
    }
    let activities = manifest.live_activities()?;
    if !activities.is_empty() {
        inputs["liveActivities"] = json!(activities.iter().map(|a| json!({
            "kind":a.kind, "title":a.title, "description":a.description, "push":a.push, "frequent":a.frequent
        })).collect::<Vec<_>>());
    }
    // Which GPU artifact owns which surface (LLP 1009 D6): the manifest's
    // modules, bound into the cohort — a routing change is a new binary. An
    // app with one artifact carries no key, so its id and bytes are as before.
    // The TV and the watch bake as "ios"; a package's `targets` names them apart.
    let gpu_platform = ["tvos", "watchos"]
        .into_iter()
        .find(|p| target.contains(&format!("-{p}")))
        .unwrap_or(platform);
    let modules = contract::package_gpu::modules(app_dir, &manifest.json, gpu_platform)?;
    if modules.as_object().is_some_and(|m| !m.is_empty()) {
        inputs["gpuModules"] = modules;
    }
    // A native TypeScript module runs on the binary's prelude: what it may
    // ask of it is identity. Only apps with one carry the key, so no other
    // app's id moves.
    if hermes {
        inputs["typescriptRuntime"] = json!(TYPESCRIPT_RUNTIME);
    }
    // The data crate the manifest names (LLP 1153.003 D2), which an entry's
    // source must be (`contract::rust_entry`). Only apps that name one carry
    // the key, so no other app's id moves.
    if let Some(data) = manifest.rust_data()? {
        inputs["rustData"] = json!({ "crate": data.krate, "type": data.ty, "source": data.source });
    }
    let id = compatibility_digest(&inputs);
    let version = |name: &str, default: &str| match std::env::var(name) {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok(default.to_string()),
        Err(_) => Err(format!("{name} is not UTF-8")),
    };
    Ok(Compat {
        id,
        inputs,
        channel,
        origin,
        activate: manifest.activate(),
        check_interval: manifest
            .json
            .pointer("/deploy/check/interval")
            .and_then(Value::as_number)
            .filter(|interval| interval.as_f64().is_some_and(|i| i.is_finite() && i > 0.0))
            .cloned(),
        target: target.into(),
        embedded: json!({ "trust": trust,
            "version": version("EXACT_VERSION", "0.1.0")?,
            "buildNumber": version("EXACT_BUILD_NUMBER", "1")? }),
        reach: serde_json::Value::Null,
        painter: manifest.painter(platform)?,
        game: manifest
            .json
            .get("game")
            .is_some_and(serde_json::Value::is_object),
        host_work: false,
    })
}

/// `EXACT_ABI_VERSION` from the C header.
fn abi_version() -> Result<u32, String> {
    ABI_HEADER
        .lines()
        .find_map(|l| l.trim().strip_prefix("#define EXACT_ABI_VERSION"))
        .and_then(|rest| rest.trim().parse().ok())
        .ok_or_else(|| "exact.h declares no EXACT_ABI_VERSION".to_string())
}

/// The executors the app's composition links (LLP 1029 D2): the native
/// crate always; an `app.ts` uses the browser on web, Hermes on native;
/// wasmi iff the native host crate names `Swappable`.
fn executors(app_dir: &Path, platform: &str) -> Vec<String> {
    let mut out = vec!["native".to_string()];
    if app_dir.join("app.ts").exists() {
        out.push(
            if platform == "web" {
                "browser"
            } else {
                "hermes"
            }
            .into(),
        );
    }
    let platform_host = app_dir.join(platform).join("src/lib.rs");
    let host = if platform_host.is_file() {
        platform_host
    } else {
        app_dir.join("apple/src/lib.rs")
    };
    if platform != "web" && std::fs::read_to_string(host).is_ok_and(|s| s.contains("Swappable")) {
        out.push("wasmi".into());
    }
    out.sort();
    out
}

/// The convention-defined iOS notification service target and its complete
/// source identity. Other Apple hosts neither advertise nor package it.
fn notification_service(
    app_dir: &Path,
    platform: &str,
    target: &str,
) -> Result<serde_json::Value, String> {
    let selected = std::env::var("EXACT_APPLE_EXTENSIONS").ok();
    if std::env::var_os("OUT_DIR").is_some() {
        println!("cargo:rerun-if-env-changed=EXACT_APPLE_EXTENSIONS");
    }
    if platform != "ios" || target.contains("-tvos") || target.contains("-watchos") {
        return Ok(serde_json::Value::Null);
    }
    if selected
        .as_deref()
        .is_some_and(|kinds| !kinds.split(',').any(|kind| kind == "notifications"))
    {
        return Ok(serde_json::Value::Null);
    }
    let dir = app_dir.join("modules/apple/notification-service");
    let directory_exists = match std::fs::metadata(&dir) {
        Ok(metadata) => metadata.is_dir(),
        Err(error) if notification_service_is_absent(&error) => false,
        Err(error) => return Err(format!("modules/apple/notification-service: {error}")),
    };
    if let Some(out) = std::env::var_os("OUT_DIR") {
        if let Some(path) = crate::receipt::watch::notification_service_tree(
            &dir,
            &[PathBuf::from(out)],
            selected.as_deref(),
            platform,
            target,
        ) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    if !directory_exists {
        return Ok(serde_json::Value::Null);
    }
    for name in [
        "modules",
        "modules/apple",
        "modules/apple/notification-service",
    ] {
        match std::fs::symlink_metadata(app_dir.join(name)) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "{name}: is a symbolic link; notification service sources cannot be symbolic links"
                ));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Ok(serde_json::Value::Null),
            Err(error) => return Err(format!("{name}: {error}")),
        }
    }
    let (tree, files) = notification_service_identity(&dir)?;
    Ok(serde_json::json!({
        "bundleSuffix": "notifications",
        "bundleName": "ExactNotificationService",
        "principalClass": "ExactNotificationService.NotificationService",
        "tree": tree,
        "files": files,
    }))
}

fn notification_service_is_absent(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    ) {
        return true;
    }
    #[cfg(target_vendor = "apple")]
    if error.raw_os_error() == Some(62) {
        return true;
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if error.raw_os_error() == Some(40) {
        return true;
    }
    false
}

/// Swift sources anywhere below the convention root and its one top-level
/// privacy manifest, ordered by the UTF-8 bytes of their `/`-joined names.
/// Links are refused instead of acquiring source bytes from outside the app.
fn notification_service_identity(dir: &Path) -> Result<(String, Vec<serde_json::Value>), String> {
    let mut paths = Vec::new();
    walk_notification_service(dir, dir, &mut paths)?;
    paths.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut tree = Sha256::new();
    let mut files = Vec::with_capacity(paths.len());
    for (name, path) in paths {
        let bytes = std::fs::read(&path).map_err(|e| format!("{name}: {e}"))?;
        tree.update(name.as_bytes());
        tree.update([0]);
        tree.update((bytes.len() as u64).to_le_bytes());
        tree.update(&bytes);
        files.push(serde_json::json!({
            "path": name,
            "sha256": hex(&Sha256::digest(&bytes)),
        }));
    }
    Ok((hex(&tree.finalize()), files))
}

fn walk_notification_service(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name().into_string().map_err(|name| {
            format!(
                "{} contains a non-UTF-8 source name",
                name.to_string_lossy()
            )
        })?;
        if name.starts_with('.') || name == "target" {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("a read_dir child stays below its root")
            .components()
            .map(|part| part.as_os_str().to_str())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("{} contains a non-UTF-8 source name", path.display()))?
            .join("/");
        let kind = entry.file_type().map_err(|e| format!("{relative}: {e}"))?;
        if kind.is_symlink() {
            return Err(format!(
                "{relative} is a symbolic link; notification service sources cannot be symbolic links"
            ));
        }
        if kind.is_dir() {
            walk_notification_service(root, &path, out)?;
        } else if kind.is_file() {
            if !relative.ends_with(".swift") && relative != "PrivacyInfo.xcprivacy" {
                return Err(format!(
                    "{relative} is an unsupported notification service resource; use Swift sources and a top-level PrivacyInfo.xcprivacy"
                ));
            }
            out.push((relative, path));
        } else {
            return Err(format!(
                "{relative} is not a regular notification service source file"
            ));
        }
    }
    Ok(())
}

/// The data crate's source-input digest (LLP 1030 D3a): its tree, the trees
/// of the path crates it is built from, the workspace lockfile, the
/// toolchain — never its compiled output, which a `wasm32` build can keep
/// while the native build moved. It reads every arm of a `cfg(target_*)`
/// branch, so the bake need not refuse one (LLP 1030 D3a, amended
/// 2026-11-12). The crate is the one beside app.json that `rust.data`
/// names, else `data/` (LLP 1030.007 D3). A `data/` without a `Cargo.toml` is
/// no crate: TypeScript modules there are the bundle's, and an edit to them is
/// an update, not a new cohort.
fn data_crate(app_dir: &Path, named: Option<&str>) -> Result<serde_json::Value, String> {
    let dir = named
        .and_then(|name| crate_beside(app_dir, name))
        .unwrap_or_else(|| app_dir.join("data"));
    if !dir.join("Cargo.toml").is_file() {
        return Ok(serde_json::Value::Null);
    }
    // Cargo may create a lockfile that the source digest must already include.
    let dependencies = path_crates(&dir)?;
    let (tree, files) = tree_digest(&dir)?;
    let watch = std::env::var_os("OUT_DIR").is_some();
    if watch {
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    let mut trees = std::collections::BTreeMap::<String, Vec<String>>::new();
    for (name, path) in dependencies {
        if watch {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        trees.entry(name).or_default().push(tree_digest(&path)?.0);
    }
    // Two path crates can share a package name (two versions; one Cargo
    // left out of the resolve). Each tree counts, in digest order, so the id
    // does not depend on where the crates are.
    let crates: serde_json::Map<_, _> = trees
        .into_iter()
        .map(|(name, mut trees)| {
            trees.sort();
            let value = match trees.as_slice() {
                [tree] => serde_json::Value::from(tree.as_str()),
                _ => serde_json::Value::from(trees),
            };
            (name, value)
        })
        .collect();
    let lockfile = lockfile(app_dir).map(|p| {
        std::fs::read(&p)
            .map(|b| hex(&Sha256::digest(&b)))
            .map_err(|e| format!("{}: {e}", p.display()))
    });
    let lockfile = match lockfile {
        Some(r) => serde_json::Value::String(r?),
        None => serde_json::Value::Null,
    };
    let toolchain = toolchain().map_or(serde_json::Value::Null, serde_json::Value::String);
    let mut identity = serde_json::json!({ "tree": tree, "files": files, "lockfile": lockfile, "toolchain": toolchain });
    // Only a crate with path dependencies carries the key, so the ids of the
    // others are as they were.
    if !crates.is_empty() {
        identity["crates"] = serde_json::Value::Object(crates);
    }
    Ok(identity)
}

/// The digest of every file under `dir` and how many there are.
fn tree_digest(dir: &Path) -> Result<(String, usize), String> {
    let (tree, files) = tree_identity(dir)?;
    Ok((tree, files.len()))
}

fn tree_identity(dir: &Path) -> Result<(String, Vec<PathBuf>), String> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut h = Sha256::new();
    for rel in &files {
        let bytes = std::fs::read(dir.join(rel)).map_err(|e| format!("{}: {e}", rel.display()))?;
        h.update(rel.to_string_lossy().as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    Ok((hex(&h.finalize()), files))
}

/// The toolchain's version line (`$RUSTC --version`), asked once per process and per
/// `RUSTC`: a process computes many ids, and a compiler does not change under it.
fn toolchain() -> Option<String> {
    static ASKED: std::sync::Mutex<Vec<(String, Option<String>)>> =
        std::sync::Mutex::new(Vec::new());
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, version)) = asked.iter().find(|(name, _)| *name == rustc) {
        return version.clone();
    }
    let version = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    asked.push((rustc, version.clone()));
    version
}

/// Every file under `root`, as paths relative to it, skipping a `target`
/// directory (a crate built in place) and dot files.
fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

/// The directory beside app.json whose `Cargo.toml` names the package `name`
/// (the app's own crates, LLP 1153.003 D1.1).
fn crate_beside(app_dir: &Path, name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(app_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs.into_iter().find(|dir| {
        std::fs::read_to_string(dir.join("Cargo.toml"))
            .is_ok_and(|text| package_name(&text).as_deref() == Some(name))
    })
}

/// `[package] name` of a Cargo manifest, read without a TOML parser.
fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package {
            let value = line
                .strip_prefix("name")
                .and_then(|rest| rest.trim_start().strip_prefix('='));
            if let Some(value) = value {
                return Some(
                    value
                        .trim()
                        .trim_matches(|c| c == '"' || c == '\'')
                        .to_string(),
                );
            }
        }
    }
    None
}

/// The lock the data crate is built against: the app's own workspace's,
/// else the generated workspace's (`.exact/shells/Cargo.lock`, which an app
/// outside exact2 builds against), else the nearest one above the app.
fn lockfile(app_dir: &Path) -> Option<PathBuf> {
    let generated = app_dir.join(".exact/shells/Cargo.lock");
    if !app_dir.join("Cargo.lock").is_file() && generated.is_file() {
        return Some(generated);
    }
    let mut dir = app_dir.canonicalize().ok()?;
    loop {
        let candidate = dir.join("Cargo.lock");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The declared shader inventory, by stem and interface digest;
/// `null` when the app has no shaders.
fn gpu_surfaces(app_dir: &Path, manifest: &Manifest) -> Result<serde_json::Value, String> {
    // The bake, hosts and dev server share the declared-root inventory.
    // @ref llp/1046.006.000-render-hooks.rfc.md#d5-shaders-that-live-with-the-game
    let gate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/app.mjs");
    let cargo = std::env::var_os("OUT_DIR").is_some();
    if cargo {
        println!("cargo:rerun-if-changed={}", gate.display());
    }
    let code = r#"
        import {pathToFileURL} from 'node:url';
        const {shaderFiles,shaderRoots,shaderPreludeFiles} = await import(pathToFileURL(process.argv[1]));
        const app={dir:process.argv[2],manifest:JSON.parse(process.argv[3])};
        process.stdout.write(JSON.stringify({roots:[...shaderRoots(app),...shaderPreludeFiles(app)],files:[...shaderFiles(app)].map(([name,bytes])=>[name,bytes.toString('utf8')])}));
    "#;
    let result = crate::bun()
        .args(["--input-type=module", "-e", code])
        .arg(gate)
        .arg(app_dir)
        .arg(manifest.json.to_string())
        .output()
        .map_err(|e| format!("shader inventory: {}", crate::bun_error(e)))?;
    if !result.status.success() {
        return Err(format!(
            "shader inventory: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let inventory: serde_json::Value =
        serde_json::from_slice(&result.stdout).map_err(|e| e.to_string())?;
    for root in inventory["roots"].as_array().unwrap() {
        let path = Path::new(root.as_str().unwrap());
        if cargo && path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    let mut shaders = Vec::new();
    for pair in inventory["files"].as_array().unwrap() {
        let name = pair[0].as_str().unwrap();
        let digest = exact_gpu_reflect::interface_digest(pair[1].as_str().unwrap())
            .map_err(|e| format!("shader {name}: {e}"))?;
        shaders.push((
            name.trim_end_matches(".wgsl").to_string(),
            format!("{digest:016x}"),
        ));
    }
    if shaders.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    shaders.sort();
    Ok(serde_json::json!(shaders
        .into_iter()
        .map(|(name, interface)| serde_json::json!({ "name": name, "interface": interface }))
        .collect::<Vec<_>>()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// JSON with sorted keys and no whitespace: the same bytes for the same
/// inputs, whatever built the value.
pub(crate) fn canonical(v: &serde_json::Value, out: &mut String) {
    use serde_json::Value;
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap_or_default()),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                canonical(&map[*k], out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
#[path = "compat_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "compat_faces_tests.rs"]
mod faces_tests;

#[cfg(test)]
#[path = "compat_notification_service_tests.rs"]
mod notification_service_tests;

#[cfg(test)]
#[path = "compat_alternate_icons_tests.rs"]
mod alternate_icons_tests;
#[cfg(test)]
#[path = "compat_file_handlers_tests.rs"]
mod file_handlers_tests;
#[cfg(test)]
#[path = "compat_watch_sync_tests.rs"]
mod watch_sync_tests;
