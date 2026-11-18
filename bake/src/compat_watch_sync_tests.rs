use super::{compatibility_id_pinned, Manifest};
use serde_json::json;

/// The compatibility inputs of an app that replicates `liked` to its watch.
fn sync(name: &str, liked: &str, other: &str) -> serde_json::Value {
    let dir = std::env::temp_dir().join(format!(
        "exact-compat-watch-sync-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data/src")).unwrap();
    let dir = crate::TempDir(dir);
    std::fs::write(dir.join("data/src/lib.rs"), "pub struct App;\n").unwrap();
    std::fs::write(
        dir.join("data/Cargo.toml"),
        "[package]\nname = \"app-data\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.json"),
        r#"{"name":"App","icons":[{"src":"assets/a.png"}],"app":{"id":"com.example.app","name":"App"},"watch":{"source":"watch.contract","component":"AppWatch","sync":{"state":["liked"]}},"deploy":{"origin":"continuous","bundles":"continuous"}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("app.contract"),
        format!("component A\n  state liked = {liked} persist\n  state other = {other} persist\n  view\n    text \"hello\"\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("watch.contract"),
        format!(
            "component AppWatch\n  state liked = {liked} persist\n  view\n    text \"watch\"\n"
        ),
    )
    .unwrap();
    let manifest = Manifest::read(&dir).unwrap();
    compatibility_id_pinned(
        &dir,
        "ios",
        "aarch64-apple-ios",
        &manifest,
        Some(""),
        None,
        None,
        None,
        &serde_json::Value::Null,
        false,
    )
    .unwrap()
    .inputs["watchSync"]
        .clone()
}

/// The installed watch plan refuses a snapshot whose selected state is of
/// another type, so a plan that changes the type cannot reach that binary.
#[test]
fn a_selected_states_type_requires_a_new_binary() {
    let boolean = sync("boolean", "false", "false");
    assert_eq!(boolean["state"], json!(["liked"]));
    assert!(boolean["types"]["liked"].is_string(), "{boolean}");
    assert_ne!(boolean, sync("number", "0", "false"));
    assert_eq!(boolean, sync("unselected", "true", "0"));
}
