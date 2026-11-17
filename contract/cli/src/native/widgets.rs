//! What an app's Android library embeds for its home-screen widgets (LLP
//! 1126.003 D9): `widgets.rs`, which defines `exact_widgets!(data)`, and the
//! entry plans it names.
//!
//! A row is an Android widget when it declares a `system*` family (D1).
//! With none, `exact_widgets!` expands to nothing, so the library links no
//! widget code. A TypeScript bake has already written each entry's plan and
//! paired it in `app.module.json`, whose digest names those bytes; any other
//! build has them written here.

use crate::Manifest;
use std::path::Path;

/// Whether `families` makes a home-screen widget on Android (D1).
pub fn on_android(families: &[String]) -> bool {
    families.iter().any(|f| f.starts_with("system"))
}

pub(super) fn write(app_root: &Path, manifest: &Manifest, out_dir: &Path) -> Result<(), String> {
    let widgets = manifest.widgets()?;
    let kinds: Vec<&str> = widgets
        .iter()
        .filter(|w| on_android(&w.families))
        .map(|w| w.kind.as_str())
        .collect();
    let text = if kinds.is_empty() {
        "macro_rules! exact_widgets {\n    ($data:expr) => {};\n}\n".to_owned()
    } else {
        let receipt = std::fs::read_to_string(out_dir.join("app.module.json"))
            .ok()
            .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok());
        let paired = |kind: &str| {
            receipt
                .as_ref()
                .is_some_and(|r| r["plans"][kind].is_object())
        };
        let module = kinds.iter().all(|k| paired(k));
        if !module {
            let entries =
                crate::snapshot::compile_entries(app_root, manifest).map_err(|errors| {
                    errors
                        .iter()
                        .map(|e| e.message.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })?;
            std::fs::create_dir_all(out_dir.join("plans")).map_err(|e| e.to_string())?;
            for kind in &kinds {
                let entry = entries
                    .get(*kind)
                    .ok_or_else(|| format!("widget `{kind}` compiled no entry"))?;
                std::fs::write(
                    out_dir.join(format!("plans/{kind}.plan")),
                    entry.plan.encode(),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        let entries: Vec<String> = kinds
            .iter()
            .map(|k| format!("({k:?}, {:?})", format!("plans/{k}.plan")))
            .collect();
        format!(
            "macro_rules! exact_widgets {{\n    ($data:expr) => {{\n        ::exact_painted::widgets_jni!(data = $data, module = {module}, entries = [{}]);\n    }};\n}}\n",
            entries.join(", ")
        )
    };
    std::fs::write(out_dir.join("widgets.rs"), text).map_err(|e| e.to_string())
}
