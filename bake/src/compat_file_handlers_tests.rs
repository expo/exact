use super::{compatibility_with_trust, Manifest};

fn app(name: &str) -> crate::TempDir {
    let dir = std::env::temp_dir().join(format!(
        "exact-compat-file-handlers-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data/src")).unwrap();
    std::fs::write(dir.join("data/src/lib.rs"), "pub struct App;\n").unwrap();
    std::fs::write(
        dir.join("data/Cargo.toml"),
        "[package]\nname = \"app-data\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.json"),
        r#"{"name":"App","icons":[{"src":"assets/a.png"}],"app":{"id":"com.example.app","name":"App"},"host":{"ios":{"minimumOS":"17.0","backgroundModes":["fetch"]}},"deploy":{"origin":"continuous","bundles":"continuous"}}"#,
    )
    .unwrap();
    crate::TempDir(dir)
}

#[test]
fn file_handlers_are_linux_binary_metadata() {
    let dir = app("linux-file-handlers");
    let mut manifest = Manifest::read(&dir).unwrap();
    let builds = |manifest: &Manifest| {
        [
            ("linux", "x86_64-unknown-linux-gnu"),
            ("android", "aarch64-linux-android"),
            ("macos", "aarch64-apple-darwin"),
            ("windows", "x86_64-pc-windows-gnu"),
            ("web", "wasm32-unknown-unknown"),
        ]
        .map(|(platform, target)| {
            compatibility_with_trust(&dir, platform, target, manifest, Some(""), "development")
                .unwrap()
        })
    };
    let bare = builds(&manifest);
    let handlers = serde_json::json!([
        {
            "action": "/",
            "name": "Markdown",
            "accept": {"text/markdown": [".md", ".markdown"]}
        },
        {
            "action": "/",
            "name": "Folder",
            "accept": {"inode/directory": []}
        }
    ]);
    manifest.json["file_handlers"] = handlers.clone();
    let declared = builds(&manifest);
    assert_eq!(declared[0].inputs["fileHandlers"], handlers);
    assert_ne!(bare[0].id, declared[0].id, "Linux embeds the handlers");
    for (index, platform) in ["Android", "macOS", "Windows", "the web"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            bare[index + 1].id,
            declared[index + 1].id,
            "{platform} does not embed Linux portal filters"
        );
        assert!(declared[index + 1].inputs.get("fileHandlers").is_none());
    }

    manifest.json["file_handlers"] = serde_json::json!([]);
    let empty = builds(&manifest);
    assert!(empty[0].inputs.get("fileHandlers").is_none());
    assert_eq!(bare[0].id, empty[0].id, "an empty list adds no identity");
    std::fs::remove_dir_all(dir).unwrap();
}
