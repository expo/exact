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
fn file_handlers_are_linux_and_windows_binary_metadata() {
    let dir = app("desktop-file-handlers");
    let mut manifest = Manifest::read(&dir).unwrap();
    let builds = |manifest: &Manifest| {
        [
            ("linux", "x86_64-unknown-linux-gnu"),
            ("windows", "x86_64-pc-windows-gnu"),
            ("android", "aarch64-linux-android"),
            ("macos", "aarch64-apple-darwin"),
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
    for index in 0..2 {
        assert_eq!(declared[index].inputs["fileHandlers"], handlers);
        assert_ne!(
            bare[index].id, declared[index].id,
            "the binary embeds the handlers"
        );
    }
    for (index, platform) in ["Android", "macOS", "the web"].into_iter().enumerate() {
        assert_eq!(
            bare[index + 2].id,
            declared[index + 2].id,
            "{platform} carries its types elsewhere"
        );
        assert!(declared[index + 2].inputs.get("fileHandlers").is_none());
    }

    manifest.json["file_handlers"] = serde_json::json!([]);
    let empty = builds(&manifest);
    for index in 0..2 {
        assert!(empty[index].inputs.get("fileHandlers").is_none());
        assert_eq!(
            bare[index].id, empty[index].id,
            "an empty list adds no identity"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
