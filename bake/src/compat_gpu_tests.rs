use super::super::{compatibility_with_trust, Manifest};
use serde_json::json;

#[test]
fn package_gpu_ownership_uses_the_apple_destination() {
    let dir = super::app("gpu-targets");
    let package = dir.join("node_modules/@fixture/sky");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"dependencies":{"@fixture/sky":"1.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"@fixture/sky","exports":"./exact.json"}"#,
    )
    .unwrap();
    let manifest = Manifest::read(&dir).unwrap();
    // No `targets` means every host that draws GPU surfaces, which the watch is not.
    for supported in [Some("ios"), Some("tvos"), None] {
        let mut gpu = json!({
            "crate": "rust", "exactGpu": "0.4",
            "surfaces": {"sky": {"arity": 1, "factory": "sky_surface::surface"}}
        });
        if let Some(platform) = supported {
            gpu["targets"] = json!([platform]);
        }
        std::fs::write(
            package.join("exact.json"),
            json!({ "gpu": gpu }).to_string(),
        )
        .unwrap();
        for destination in ["ios", "tvos", "watchos"] {
            let compat = compatibility_with_trust(
                &dir,
                "ios",
                &format!("aarch64-apple-{destination}-sim"),
                &manifest,
                Some(""),
                "development",
            )
            .unwrap();
            let expected = if destination != "watchos" && supported.is_none_or(|s| s == destination)
            {
                json!({"fixture-sky": ["sky"]})
            } else {
                serde_json::Value::Null
            };
            assert_eq!(compat.inputs["gpuModules"], expected);
        }
    }
}
