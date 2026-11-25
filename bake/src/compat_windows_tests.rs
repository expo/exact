use super::{compatibility_with_trust, Manifest};

fn app(name: &str, manifest: &str) -> crate::TempDir {
    let dir = std::env::temp_dir().join(format!(
        "exact-compat-windows-{name}-{}",
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
    std::fs::write(dir.join("app.json"), manifest).unwrap();
    crate::TempDir(dir)
}

#[test]
fn windows_store_levels_are_refused_at_bake_time() {
    let base = |store: Option<&str>| {
        let mut deploy = serde_json::json!({"origin": "continuous", "bundles": "continuous"});
        if let Some(level) = store {
            deploy["store"] = serde_json::json!({"windows": level});
        }
        serde_json::json!({
            "name": "App",
            "icons": [{"src": "assets/a.png"}],
            "app": {"id": "com.example.app", "name": "App"},
            "host": {"windows": {}},
            "deploy": deploy,
        })
        .to_string()
    };

    for (name, store, expected) in [
        ("absent", None, "deploy.store.windows to \"0\""),
        ("level-a", Some("A"), "deploy.store.windows to \"0\""),
        (
            "invalid",
            Some("B"),
            "deploy.store.windows must be \"0\" or \"A\"",
        ),
    ] {
        let dir = app(name, &base(store));
        let manifest = Manifest::read(&dir).unwrap();
        let err = compatibility_with_trust(
            &dir,
            "windows",
            "x86_64-pc-windows-gnu",
            &manifest,
            Some(""),
            "development",
        )
        .unwrap_err();
        assert!(err.contains(expected), "{name}: {err}");
    }

    let dir = app("level-0", &base(Some("0")));
    let manifest = Manifest::read(&dir).unwrap();
    let compat = compatibility_with_trust(
        &dir,
        "windows",
        "x86_64-pc-windows-gnu",
        &manifest,
        Some(""),
        "development",
    )
    .unwrap();
    assert_eq!(compat.inputs["store"]["L"], "0");
}

#[test]
fn a_web_mount_is_a_native_delivery_input() {
    let dir = app(
        "web-mount",
        r#"{"name":"App","icons":[{"src":"assets/a.png"}],"app":{"id":"com.example.app","name":"App","origin":"https://site.invalid"},"host":{"ios":{"minimumOS":"17.0","backgroundModes":["fetch"]},"web":{"base":"/app/"}},"deploy":{"origin":"continuous","bundles":"continuous","store":{"windows":"0"}}}"#,
    );
    let manifest = Manifest::read(&dir).unwrap();
    for (platform, target) in [
        ("macos", "aarch64-apple-darwin"),
        ("ios", "aarch64-apple-ios"),
        ("linux", "aarch64-unknown-linux-gnu"),
        ("windows", "x86_64-pc-windows-gnu"),
        ("linux", "aarch64-linux-android"),
    ] {
        let compat =
            compatibility_with_trust(&dir, platform, target, &manifest, Some(""), "development")
                .unwrap();
        assert_eq!(
            compat.inputs["webMount"],
            serde_json::json!({"origin": "https://site.invalid", "base": "/app/"}),
            "{platform} {target}"
        );
    }
}
