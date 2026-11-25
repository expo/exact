//! Optional app-supplied delivery composition (LLP 1030 D4).
//! The host owns sessions and assets; the higher adapter owns store policy.
use crate::image::AssetResolver;
use exact_runner::Delivery;
use std::sync::Arc;

/// A pinned plan with its complete asset resolver.
pub struct Selection {
    /// The selected entry, absent for the embedded plan.
    pub entry: Option<String>,
    /// The sequence belonging to this exact entry.
    pub seq: u64,
    /// Validated plan bytes.
    pub plan: Arc<[u8]>,
    /// Complete selected assets; missing names never fall through to embedded files.
    pub assets: AssetResolver,
    /// Every name the signed asset index lists.
    pub names: Vec<String>,
    /// The directory a reader opens files from (LLP 1030.006 D1).
    /// Absent where the resolver has no directory.
    pub dir: Option<std::path::PathBuf>,
    /// Verify every file against its signed card before committing activation
    /// or exposing the directory to a reader. Inspection leaves assets lazy.
    pub verify_assets: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
}

/// The running generation's asset directory, for a reader that opens
/// sounds, videos and pages by path: a selected update's verified
/// directory, or none for the embedded assets (LLP 1030.006 D1).
static READER_ROOT: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// The reader's asset directory is now `dir` (none: the embedded assets).
pub fn set_reader_root(dir: Option<std::path::PathBuf>) {
    *READER_ROOT.lock().unwrap_or_else(|e| e.into_inner()) = dir;
}

/// The reader's asset directory, when an update's is running.
pub fn reader_root() -> Option<std::path::PathBuf> {
    READER_ROOT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Operations the presenter needs from a linked delivery adapter.
/// A binary-only app supplies none and creates no store or check thread.
pub trait Store {
    /// Pin a candidate and its asset resolver for this boot.
    fn prepare_selected(&mut self) -> Option<Selection>;
    /// Count a selected boot.
    fn boot_started(&mut self);
    /// Refuse the selected plan.
    fn entry_refused(&mut self, entry: &str, why: &str);
    /// Refuse assets belonging to the last prepared selection.
    fn selection_corrupt(&mut self, why: &str);
    /// Mark the selected boot successful.
    fn boot_succeeded(&mut self);
    /// Take a boot diagnostic.
    fn take_note(&mut self) -> Option<String>;
    /// The completion wake in the display's poll set.
    #[cfg(unix)]
    fn fd(&self) -> std::os::unix::io::RawFd;
    /// Start a check. When its result is ready, signal the completion fd
    /// and call the window's wake. Return whether the check started.
    fn check(&self, wake: fn()) -> bool;
    /// Take the latest asynchronous outcome.
    fn take_line(&mut self) -> Option<String>;
    /// Prepare a complete staged generation without changing live state.
    fn prepare_activation(&self) -> Result<Option<Selection>, String>;
    /// Commit the exact entry and sequence accepted by the host.
    fn commit_activation(&mut self, entry: Option<String>, seq: u64) -> Result<(), String>;
    /// Candidate stream facts, before the live store changes.
    fn staged_stream_into(&self, delivery: &mut Delivery);
    /// Copy the latest delivery facts into the runner.
    fn status_into(&self, delivery: &mut Delivery);
}

/// Opens a binary's update store from its `compat.json`, baked plan, embedded
/// asset root and store base, as `exact_painted_update::open` does: what an
/// Android library at store `"A"` hands `canvas_jni!` (LLP 1030.006 D3).
pub type Opener = fn(
    compat: &str,
    plan: &[u8],
    assets: &std::path::Path,
    base: Option<&std::path::Path>,
) -> Result<Box<dyn Store>, String>;

/// The files a Rust pair's receipt may name.
const RUST_FILES: [&str; 5] = [
    "app.module.wasm",
    "app.module.dylib",
    "app.module.so",
    "app.module.dll",
    "app.module.bin",
];

/// One inseparable module replacement, already authenticated by the asset
/// resolver: a Rust pair or a TypeScript pair.
#[derive(Clone, Debug)]
pub struct Module {
    /// UTF-8 pairing receipt.
    pub receipt: String,
    /// Opaque wasm, native library or Hermes bytecode bytes; never executed
    /// during preparation.
    pub bytes: Vec<u8>,
}

/// A validated roster and receipt; executable bytes stay unread while requests run.
pub(crate) struct ModulePair {
    pub receipt: String,
    path: String,
}

impl ModulePair {
    pub(crate) fn read(self, assets: &AssetResolver) -> Result<Module, String> {
        let bytes = assets(&self.path)?.ok_or("missing module bytes")?;
        if bytes.len() > 32 << 20 {
            return Err("module exceeds 32 MiB".into());
        }
        Ok(Module {
            receipt: self.receipt,
            bytes: bytes.to_vec(),
        })
    }
}

impl Module {
    /// The generation's module pair, both halves read before either is
    /// admitted into a data source (LLP 1030.006 D4, the Apple loader's
    /// checks): a `rust/` pair or a `typescript/` pair, never both, each
    /// exactly its receipt and one module. A TypeScript pair needs a binary
    /// that runs Hermes, a Rust pair one whose Rust policy replaces its
    /// module (`compat`'s `executors` and `rustMode`).
    pub fn resolve(
        assets: &AssetResolver,
        names: &[String],
        compat: &str,
    ) -> Result<Option<Self>, String> {
        Self::prepare(assets, names, compat)?
            .map(|pair| pair.read(assets))
            .transpose()
    }

    /// Validate the signed pair and read its bounded receipt without loading code.
    pub(crate) fn prepare(
        assets: &AssetResolver,
        names: &[String],
        compat: &str,
    ) -> Result<Option<ModulePair>, String> {
        let Some(dir) = Self::directory(names, compat)? else {
            return Ok(None);
        };
        let receipt =
            assets(&format!("{dir}/app.module.json"))?.ok_or("missing module pairing receipt")?;
        if receipt.len() > 1 << 20 {
            return Err("module receipt exceeds 1 MiB".into());
        }
        let receipt = String::from_utf8(receipt.to_vec()).map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(&receipt).map_err(|e| e.to_string())?;
        let file = if dir == "typescript" {
            "app.hbc".to_string()
        } else {
            json["module"]["file"].as_str().unwrap_or("").to_string()
        };
        let path = format!("{dir}/{file}");
        if !names.contains(&path) || (dir == "rust" && !RUST_FILES.contains(&file.as_str())) {
            return Err(format!("invalid signed {dir} module pair"));
        }
        Ok(Some(ModulePair { receipt, path }))
    }

    /// Validate the signed roster and binary policy without reading module bytes.
    fn directory(names: &[String], compat: &str) -> Result<Option<&'static str>, String> {
        let in_dir = |dir: &str| names.iter().filter(|n| n.starts_with(dir)).count();
        let (rust, typescript) = (in_dir("rust/"), in_dir("typescript/"));
        let dir = match (rust, typescript) {
            (0, 0) => return Ok(None),
            (_, 0) => "rust",
            (0, _) => "typescript",
            _ => return Err("a generation carries one module pair, not two".into()),
        };
        let compat: serde_json::Value =
            serde_json::from_str(compat).map_err(|e| format!("the binary's compat.json: {e}"))?;
        let inputs = &compat["inputs"];
        if dir == "typescript"
            && !inputs["executors"]
                .as_array()
                .is_some_and(|e| e.iter().any(|e| e == "hermes"))
        {
            return Err("this binary runs no TypeScript module".into());
        }
        if dir == "rust"
            && !matches!(
                inputs["rustMode"].as_str(),
                Some("wasm" | "native" | "tiered")
            )
        {
            return Err("Rust replacement is disabled in this binary".into());
        }
        if !names.contains(&format!("{dir}/app.module.json")) {
            return Err("missing module pairing receipt".into());
        }
        let has_module = if dir == "typescript" {
            names.contains(&"typescript/app.hbc".to_string())
        } else {
            RUST_FILES
                .iter()
                .any(|file| names.contains(&format!("rust/{file}")))
        };
        if rust + typescript != 2 || !has_module {
            return Err(format!("invalid signed {dir} module pair"));
        }
        Ok(Some(dir))
    }

    /// Read the producer's selected local development executor directory.
    pub fn local(plan: &std::path::Path, compat: &str) -> Result<Option<Self>, String> {
        let root = plan.parent().unwrap_or_else(|| std::path::Path::new("."));
        let rust = root.join("rust");
        if !rust.exists() {
            return Ok(None);
        }
        let json: serde_json::Value = serde_json::from_str(compat).map_err(|e| e.to_string())?;
        let mode = json["inputs"]["rustMode"].as_str().unwrap_or("off");
        let prefix = match mode {
            "wasm" => "rust/wasm".to_string(),
            "native" | "tiered" => format!("rust/{mode}/{}", json["target"].as_str().unwrap_or("")),
            _ => return Err("Rust replacement is disabled in this binary".into()),
        };
        let root = root.to_path_buf();
        let read: AssetResolver = Arc::new(move |name| {
            let path = root.join(name);
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.len() > 32 << 20 => {
                    return Err("Rust replacement exceeds 32 MiB".into())
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.to_string()),
                _ => {}
            }
            match std::fs::read(path) {
                Ok(bytes) if bytes.len() <= 32 << 20 => Ok(Some(bytes.into())),
                Ok(_) => Err("Rust replacement exceeds 32 MiB".into()),
                Err(e) => Err(e.to_string()),
            }
        });
        Self::resolve_at(&read, &prefix)?.map_or_else(
            || Err("no Rust replacement matching this binary's executor and target".into()),
            |module| Ok(Some(module)),
        )
    }

    fn resolve_at(assets: &AssetResolver, prefix: &str) -> Result<Option<Self>, String> {
        let Some(receipt) = assets(&format!("{prefix}/app.module.json"))? else {
            for file in RUST_FILES {
                if assets(&format!("{prefix}/{file}"))?.is_some() {
                    return Err("Rust replacement is missing its pairing receipt".into());
                }
            }
            return Ok(None);
        };
        if receipt.len() > 1 << 20 {
            return Err("Rust receipt exceeds 1 MiB".into());
        }
        let receipt = String::from_utf8(receipt.to_vec()).map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(&receipt).map_err(|e| e.to_string())?;
        let file = json["module"]["file"].as_str().unwrap_or("");
        if !RUST_FILES.contains(&file) {
            return Err("invalid Rust module filename".into());
        }
        let bytes =
            assets(&format!("{prefix}/{file}"))?.ok_or("Rust replacement is missing its module")?;
        if bytes.len() > 32 << 20 {
            return Err("Rust replacement exceeds 32 MiB".into());
        }
        Ok(Some(Self {
            receipt,
            bytes: bytes.to_vec(),
        }))
    }

    /// Pair against the candidate plan and this binary's baked authority.
    pub fn replacement<D: exact_runner::DataSource>(
        &self,
        plan: &[u8],
        admitted: &D,
    ) -> Result<D, String> {
        admitted
            .replacement(plan, &self.receipt, self.bytes.clone())
            .map_err(|e| format!("module replacement: {e:?}"))
    }
}

/// Refuse a delivered generation that needs more than the binary was
/// composed from (LLP 1030.006 D4a): an Android binary's `androidLinked`
/// names the capabilities, package extensions and permissions its APK
/// links; the plan's own are recomputed from it, and the permissions only
/// the module's source shows are in its signed receipt. A binary that
/// records no composition refuses nothing here.
pub fn within_linked(plan: &[u8], receipt: Option<&str>, compat: &str) -> Result<(), String> {
    let compat: serde_json::Value = serde_json::from_str(compat).unwrap_or_default();
    let linked = &compat["inputs"]["androidLinked"];
    if linked.is_null() {
        return Ok(());
    }
    let plan = exact_plan::Plan::decode(plan).map_err(|e| format!("{e:?}"))?;
    let uses: Vec<String> = exact_runner::uses(&plan)
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    let extensions: Vec<String> = plan
        .modules
        .iter()
        .map(|m| m.extension)
        .chain(plan.views.iter().map(|v| v.extension))
        .chain(plan.module_events.iter().map(|e| e.extension))
        .map(|name| plan.str(name).to_string())
        .collect();
    let permissions: Vec<String> = receipt
        .and_then(|r| serde_json::from_str::<serde_json::Value>(r).ok())
        .and_then(|r| r["androidPermissions"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|p| p.as_str().map(str::to_string))
        .collect();
    for (key, needs) in [
        ("uses", uses),
        ("extensions", extensions),
        ("permissions", permissions),
    ] {
        let have = linked[key].as_array();
        if let Some(missing) = needs
            .iter()
            .find(|need| !have.is_some_and(|have| have.iter().any(|h| h == need.as_str())))
        {
            return Err(format!(
                "the generation needs {missing}, which this binary does not link"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPESCRIPT: &str = r#"{"inputs":{"executors":["hermes","native"],"rustMode":"off"}}"#;
    const RUST: &str = r#"{"inputs":{"executors":["native","wasmi"],"rustMode":"wasm"}}"#;

    fn roster(files: &[(&str, &[u8])]) -> (AssetResolver, Vec<String>) {
        let files: std::collections::BTreeMap<String, Arc<[u8]>> = files
            .iter()
            .map(|(name, bytes)| (name.to_string(), Arc::from(*bytes)))
            .collect();
        let names = files.keys().cloned().collect();
        (Arc::new(move |name| Ok(files.get(name).cloned())), names)
    }

    fn resolve(files: &[(&str, &[u8])], compat: &str) -> Result<Option<Module>, String> {
        let (assets, names) = roster(files);
        Module::resolve(&assets, &names, compat)
    }

    #[test]
    fn either_pair_resolves_whole_and_none_is_no_module() {
        let typescript = resolve(
            &[
                ("typescript/app.module.json", b"{\"appId\":\"a\"}"),
                ("typescript/app.hbc", b"bytecode"),
                ("assets/mark.png", b"png"),
            ],
            TYPESCRIPT,
        )
        .unwrap()
        .unwrap();
        assert_eq!(typescript.receipt, "{\"appId\":\"a\"}");
        assert_eq!(typescript.bytes, b"bytecode");
        let rust = resolve(
            &[
                (
                    "rust/app.module.json",
                    br#"{"module":{"file":"app.module.wasm"}}"#,
                ),
                ("rust/app.module.wasm", b"wasm"),
            ],
            RUST,
        )
        .unwrap()
        .unwrap();
        assert_eq!(rust.bytes, b"wasm");
        assert!(resolve(&[("assets/mark.png", b"png")], "")
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_pair_is_refused_beside_another_beyond_its_two_names_or_missing_a_half() {
        let both = resolve(
            &[
                ("typescript/app.module.json", b"{}"),
                ("typescript/app.hbc", b"bytecode"),
                (
                    "rust/app.module.json",
                    br#"{"module":{"file":"app.module.wasm"}}"#,
                ),
                ("rust/app.module.wasm", b"wasm"),
            ],
            r#"{"inputs":{"executors":["hermes","native"],"rustMode":"wasm"}}"#,
        );
        assert_eq!(
            both.unwrap_err(),
            "a generation carries one module pair, not two"
        );
        let third = resolve(
            &[
                ("typescript/app.module.json", b"{}"),
                ("typescript/app.hbc", b"bytecode"),
                ("typescript/app.js", b"source"),
            ],
            TYPESCRIPT,
        );
        assert_eq!(third.unwrap_err(), "invalid signed typescript module pair");
        let receipt_only = resolve(&[("typescript/app.module.json", b"{}")], TYPESCRIPT);
        assert_eq!(
            receipt_only.unwrap_err(),
            "invalid signed typescript module pair"
        );
        let module_only = resolve(&[("typescript/app.hbc", b"bytecode")], TYPESCRIPT);
        assert_eq!(module_only.unwrap_err(), "missing module pairing receipt");
        let renamed = resolve(
            &[
                ("rust/app.module.json", br#"{"module":{"file":"../x.so"}}"#),
                ("rust/app.module.wasm", b"wasm"),
            ],
            RUST,
        );
        assert_eq!(renamed.unwrap_err(), "invalid signed rust module pair");
    }

    #[test]
    fn sizes_are_bounded() {
        let big_receipt = vec![b' '; (1 << 20) + 1];
        let refused = resolve(
            &[
                ("typescript/app.module.json", &big_receipt),
                ("typescript/app.hbc", b"bytecode"),
            ],
            TYPESCRIPT,
        );
        assert_eq!(refused.unwrap_err(), "module receipt exceeds 1 MiB");
        let big_module = vec![0u8; (32 << 20) + 1];
        let refused = resolve(
            &[
                ("typescript/app.module.json", b"{}"),
                ("typescript/app.hbc", &big_module),
            ],
            TYPESCRIPT,
        );
        assert_eq!(refused.unwrap_err(), "module exceeds 32 MiB");
    }

    /// A Rust binary refuses a TypeScript pair, and a binary that replaces no
    /// Rust refuses a Rust pair, before reading either.
    #[test]
    fn a_pair_needs_its_executor_in_the_binary() {
        let typescript = [
            ("typescript/app.module.json", b"{}".as_slice()),
            ("typescript/app.hbc", b"bytecode".as_slice()),
        ];
        assert_eq!(
            resolve(&typescript, RUST).unwrap_err(),
            "this binary runs no TypeScript module"
        );
        let rust = [
            (
                "rust/app.module.json",
                br#"{"module":{"file":"app.module.wasm"}}"#.as_slice(),
            ),
            ("rust/app.module.wasm", b"wasm".as_slice()),
        ];
        assert_eq!(
            resolve(&rust, TYPESCRIPT).unwrap_err(),
            "Rust replacement is disabled in this binary"
        );
    }

    /// A generation needing a capability, an extension or a permission beyond
    /// what the APK linked is refused; one needing less is not.
    #[test]
    fn a_generation_beyond_the_linked_composition_is_refused() {
        let plan = crate::test_plan(
            "component App\n  state day = \"2026-09-14\"\n  action setDay(value: string)\n    day = value\n  view\n    column gap=8\n      input type=\"date\" value=day change=setDay testId=\"day\"\n",
        );
        let decoded = exact_plan::Plan::decode(&plan).unwrap();
        let uses: Vec<&str> = exact_runner::uses(&decoded)
            .iter()
            .map(|c| c.name())
            .collect();
        assert!(!uses.is_empty(), "the fixture uses a capability");
        let compat = |uses: &[&str], permissions: &[&str]| {
            serde_json::json!({"inputs": {"androidLinked": {
                "uses": uses, "extensions": [], "permissions": permissions
            }}})
            .to_string()
        };
        let module = |permissions: &[&str]| Module {
            receipt: serde_json::json!({"androidPermissions": permissions}).to_string(),
            bytes: Vec::new(),
        };
        let calendar = "android.permission.WRITE_CALENDAR";
        assert_eq!(within_linked(&plan, None, &compat(&uses, &[])), Ok(()));
        assert_eq!(
            within_linked(
                &plan,
                Some(&module(&[calendar]).receipt),
                &compat(&uses, &[calendar])
            ),
            Ok(())
        );
        let error = within_linked(&plan, None, &compat(&uses[1..], &[])).unwrap_err();
        assert!(error.contains(uses[0]), "{error}");
        let error = within_linked(
            &plan,
            Some(&module(&[calendar]).receipt),
            &compat(&uses, &[]),
        )
        .unwrap_err();
        assert!(error.contains("WRITE_CALENDAR"), "{error}");
        // A binary that records no composition refuses nothing here.
        assert_eq!(
            within_linked(&plan, Some(&module(&[calendar]).receipt), "{}"),
            Ok(())
        );
    }
}
