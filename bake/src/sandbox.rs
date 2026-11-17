//! What the Mac App Store's sandbox asks of this app: each grant line's
//! column from the runner's table, and the host capabilities no grant line
//! names (an iframe's printing, a remote source, a document the person
//! chooses).
//!
//! @ref LLP 1030.005 D2 (`reach.sandboxEntitlements`, each beside what
//! derived it, and `reach.sandboxRefusals`)
//!
//! Every macOS bake reports them; only a sandboxed build reads them (`exact
//! appstore`, `mac --sandbox`), which signs the entitlements and refuses the
//! build on any refusal. A Developer ID build keeps every grant.

use crate::compat::Compat;
use exact_runner::sandbox::{self, Sandbox};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Fill `reach.sandboxEntitlements` (each key with its prefix, sorted),
/// `reach.sandboxSources` (key → what derived it) and
/// `reach.sandboxRefusals` (one sentence per refused line) on a macOS bake.
pub(crate) fn derive(
    app_dir: &Path,
    platform: &str,
    plan: Option<&exact_plan::Plan>,
    compat: &mut Compat,
) {
    if platform != "macos" {
        return;
    }
    let mut sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut add = |key: &str, why: &str| {
        let list = sources
            .entry(format!("{}{key}", sandbox::PREFIX))
            .or_default();
        if !list.iter().any(|w| w == why) {
            list.push(why.to_string());
        }
    };
    add(sandbox::APP, "any app");
    let mut refusals = Vec::new();
    let ceiling = compat.inputs["grantCeiling"].as_str().unwrap_or_default();
    for line in ceiling.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match sandbox::line(line) {
            Sandbox::Keys(keys) => keys.iter().for_each(|key| add(key, line)),
            Sandbox::Profile => {}
            Sandbox::Refused(why) => refusals.push(format!("`{line}`: {why}")),
        }
    }
    if let Some(plan) = plan {
        let makes = |wanted: exact_kernel::NodeType| {
            plan.nodes
                .iter()
                .any(|node| exact_kernel::NodeType::from_wire(node.node_type) == Some(wanted))
        };
        // `FramePrint.swift` runs `NSPrintOperation` for a frame's `window.print()`.
        if makes(exact_kernel::NodeType::WebView) {
            add("print", "an iframe, whose page may print");
        }
        if exact_runner::uses::sources_can_be_remote(plan) {
            add(
                sandbox::CLIENT,
                "an image, media or iframe source that may be remote",
            );
        }
        let uses = exact_runner::uses(plan);
        use exact_runner::uses::Capability;
        if uses.has(Capability::Documents) || uses.has(Capability::Picker) {
            add(
                sandbox::USER_SELECTED,
                "a file picker, save panel, file input or drop",
            );
        }
    }
    let manifest: Value = std::fs::read_to_string(app_dir.join("app.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if manifest["file_handlers"]
        .as_array()
        .is_some_and(|handlers| !handlers.is_empty())
    {
        add(sandbox::USER_SELECTED, "app.json file_handlers");
    }
    if compat.reach.is_null() {
        compat.reach = json!({});
    }
    compat.reach["sandboxEntitlements"] = json!(sources.keys().collect::<Vec<_>>());
    compat.reach["sandboxSources"] = json!(sources);
    compat.reach["sandboxRefusals"] = json!(refusals);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compat(grants: &str) -> Compat {
        Compat {
            id: String::new(),
            inputs: json!({ "grantCeiling": grants }),
            channel: String::new(),
            origin: None,
            activate: "next-launch".into(),
            target: "t".into(),
            embedded: Value::Null,
            reach: Value::Null,
            painter: None,
            game: false,
            check_interval: None,
        }
    }

    fn bake(grants: &str, source: &str, manifest: &str) -> Value {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "exact-sandbox-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.json"), manifest).unwrap();
        let plan = contract::compile(source).unwrap();
        let mut compat = compat(grants);
        derive(&dir, "macos", Some(&plan), &mut compat);
        std::fs::remove_dir_all(&dir).unwrap();
        compat.reach
    }

    const PLAIN: &str = "component App\n  view\n    text \"a\"\n";

    #[test]
    fn every_app_is_sandboxed_and_each_key_names_its_source() {
        let reach = bake(
            "net.fetch https://api.example\nfs.write app:/notes\ndoc.keep\ndevice.camera purpose.camera\ndevice.push",
            PLAIN,
            "{}",
        );
        assert_eq!(
            reach["sandboxEntitlements"],
            json!([
                "com.apple.security.app-sandbox",
                "com.apple.security.device.camera",
                "com.apple.security.files.bookmarks.app-scope",
                "com.apple.security.files.user-selected.read-write",
                "com.apple.security.network.client"
            ])
        );
        assert_eq!(
            reach["sandboxSources"]["com.apple.security.network.client"],
            json!(["net.fetch https://api.example"])
        );
        assert_eq!(
            reach["sandboxSources"]["com.apple.security.app-sandbox"],
            json!(["any app"])
        );
        assert_eq!(reach["sandboxRefusals"], json!([]));
    }

    #[test]
    fn a_refused_line_is_named_with_its_reason() {
        let reach = bake(
            "process.spawn path:t3\nfs.read home:/Documents\nenv.read login:PATH\ndevice.health-read purpose.h",
            PLAIN,
            "{}",
        );
        let refusals: Vec<&str> = reach["sandboxRefusals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        assert_eq!(refusals.len(), 4, "{refusals:?}");
        assert_eq!(
            refusals[0],
            "`process.spawn path:t3`: the Mac App Store sandbox runs no program outside the app's bundle"
        );
        assert!(refusals[1].starts_with("`fs.read home:/Documents`: "));
        assert!(refusals[2].contains("login shell"));
        assert!(refusals[3].contains("HealthKit"));
    }

    #[test]
    fn file_handlers_and_an_iframe_reach_the_column() {
        let reach = bake(
            "",
            "component App\n  view\n    iframe src=\"https://example.com/\" width=10 height=10\n",
            r#"{"file_handlers":[{"action":"/open","accept":{"text/markdown":[".md"]}}]}"#,
        );
        let sources = &reach["sandboxSources"];
        assert_eq!(
            sources["com.apple.security.print"],
            json!(["an iframe, whose page may print"])
        );
        assert_eq!(
            sources["com.apple.security.network.client"],
            json!(["an image, media or iframe source that may be remote"])
        );
        assert_eq!(
            sources["com.apple.security.files.user-selected.read-write"],
            json!(["app.json file_handlers"])
        );
        // A local frame prints and reaches no network.
        let local = bake(
            "",
            "component App\n  view\n    iframe src=\"/deck/index.html\" width=10 height=10\n",
            "{}",
        );
        assert!(local["sandboxSources"]["com.apple.security.print"].is_array());
        assert!(local["sandboxSources"]["com.apple.security.network.client"].is_null());
    }

    #[test]
    fn a_picker_takes_the_user_selected_key_and_a_bundled_image_nothing() {
        let reach = bake(
            "",
            "component App\n  action open\n    showOpenFilePicker(\"opened\")\n  view\n    column\n      button press=open testId=\"opened\"\n        text \"Open\"\n      image \"assets/a.png\" width=1 height=1\n",
            "{}",
        );
        assert_eq!(
            reach["sandboxEntitlements"],
            json!([
                "com.apple.security.app-sandbox",
                "com.apple.security.files.user-selected.read-write"
            ])
        );
        let remote = bake(
            "",
            "component App\n  view\n    image \"https://example.com/a.png\" width=1 height=1\n",
            "{}",
        );
        assert!(remote["sandboxSources"]["com.apple.security.network.client"].is_array());
    }

    #[test]
    fn only_the_mac_reports_the_column() {
        let mut compat = compat("net.fetch https://x");
        derive(Path::new("/nonexistent"), "ios", None, &mut compat);
        assert!(compat.reach.is_null());
    }
}
