//! Editor declarations from the one-shot native producer.
use std::process::Command;

#[test]
fn one_shot_with_unset_trust_refreshes_editor_source_signatures() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let root = std::env::temp_dir().join(format!("exact-declarations-{}", std::process::id()));
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::write(
        app.join("app.ts"),
        r#"
export const appId = 'test.exact.declarations';
export const grants = '';
export const answer = () => 'hello';
"#,
    )
    .unwrap();
    let editor = app.join("app.contract.d.ts");
    for name in ["message", "greeting"] {
        std::fs::write(
            app.join("app.contract"),
            format!(
            "component App\n  resource value = {name}() as shape string\n  view\n    text value\n"
        ),
        )
        .unwrap();
        std::fs::write(&editor, "stale declarations").unwrap();
        let out = root.join(name);
        let result = Command::new(env!("CARGO_BIN_EXE_exact-js-bake"))
            .arg(&app)
            .arg("--out")
            .arg(&out)
            .env_remove("EXACT_UPDATE_TRUST")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let declarations = std::fs::read_to_string(&editor).unwrap();
        assert!(declarations.contains(name), "{declarations}");
    }
}
