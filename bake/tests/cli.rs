//! `exact-bake compat`, the compatibility id on the command line (LLP 1030 D3a).
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("exact-bake-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for App {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn help_succeeds_without_reading_or_changing_files() {
    let app = App::new("help");
    for command in [None, Some("compat")] {
        for flag in ["--help", "-h"] {
            let mut process = Command::new(env!("CARGO_BIN_EXE_exact-bake"));
            process
                .args(command)
                .arg(flag)
                .current_dir(Path::new(&app.0));
            let output = process.output().unwrap();
            assert!(output.status.success(), "{command:?} {flag}: {output:?}");
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.starts_with("usage:"), "{text}");
            assert!(text.contains("exact-bake compat"), "{text}");
        }
    }
    assert_eq!(std::fs::read_dir(&app.0).unwrap().count(), 0);
}

#[test]
fn compat_cli_validates_all_options_before_reading_the_manifest() {
    let app = App::new("compat-args");
    let manifest = r#"{"app":{"id":"com.exact.cli-args","name":"CLI args"},"deploy":{"store":{"ios":"0","macos":"0","linux":"0","web":"0"}}}"#;
    app.write("app.json", manifest);
    std::fs::create_dir_all(app.0.join("gpu/shaders")).unwrap();
    app.write(
        "gpu/shaders/fixture.wgsl",
        "@compute @workgroup_size(1) fn main() {}",
    );
    // The filesystem helper this process checked, named to each child as the bake names it to
    // its own Bun calls: the children are about their options, not about checking the helper
    let helper = exact_bake::bun()
        .get_envs()
        .find(|(name, _)| *name == "EXACT_FILESYSTEM_HELPER")
        .and_then(|(_, value)| value.map(std::ffi::OsStr::to_owned));
    let run = |args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_exact-bake"));
        if let Some(helper) = &helper {
            command.env("EXACT_FILESYSTEM_HELPER", helper);
        }
        command
            .arg("compat")
            .args(args)
            .current_dir(&app.0)
            .env("EXACT_UPDATE_TRUST", "development")
            .env_remove("OUT_DIR")
            .output()
            .unwrap()
    };
    for (platform, target) in [
        ("web", "wasm32-unknown-unknown"),
        ("macos", "aarch64-apple-darwin"),
        ("ios", "aarch64-apple-ios"),
        ("linux", "x86_64-unknown-linux-gnu"),
    ] {
        let result = run(&[".", "--json", "--target", target, "--platform", platform]);
        assert!(result.status.success(), "{result:?}");
        assert!(result.stderr.is_empty());
        let json: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(json["inputs"]["platform"], platform);
        assert_eq!(json["target"], target);
        assert_eq!(json["inputs"]["gpuSurfaces"][0]["name"], "fixture");
        assert!(json["inputs"]["gpuSurfaces"][0]["interface"].is_string());
        let plain = run(&[".", "--platform", platform, "--target", target]);
        assert!(plain.status.success(), "{plain:?}");
        assert_eq!(
            String::from_utf8(plain.stdout).unwrap().trim(),
            json["id"].as_str().unwrap()
        );
    }
    let default = run(&[".", "--platform", "web", "--json"]);
    assert!(default.status.success(), "{default:?}");
    let json: Value = serde_json::from_slice(&default.stdout).unwrap();
    assert_eq!(
        json["target"],
        format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
    );
    assert_eq!(
        std::fs::read_to_string(app.0.join("app.json")).unwrap(),
        manifest
    );
    for (javascript, rust) in [
        ("", ""),
        ("net.fetch https://api.example", "fs.read app:/data"),
    ] {
        let result = run(&[
            ".",
            "--platform",
            "ios",
            "--grants",
            javascript,
            "--rust-grants",
            rust,
            "--json",
        ]);
        assert!(result.status.success(), "{result:?}");
        let json: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(json["inputs"]["javascriptGrants"], javascript);
        assert_eq!(json["inputs"]["rustGrants"], rust);
        for grant in [javascript, rust] {
            assert!(json["inputs"]["grantCeiling"]
                .as_str()
                .unwrap()
                .contains(grant));
        }
    }

    app.write("app.json", "invalid JSON");
    for args in [
        vec![],
        vec![""],
        vec!["."],
        vec![".", "--platform"],
        vec![".", "--platform", "unknown"],
        vec![".", "--platform", "web", "--unknown"],
        vec![".", "--platform", "web", "extra"],
        vec![".", "--platform", "web", "--platform", "ios"],
        vec![".", "--platform", "web", "--target"],
        vec![".", "--platform", "web", "--target", "--json"],
        vec![".", "--platform", "web", "--target", ""],
        vec![".", "--platform", "web", "--target", "a", "--target", "b"],
        vec![".", "--platform", "web", "--json", "--json"],
        vec![".", "--platform", "web", "--grants"],
        vec![".", "--platform", "web", "--grants", "--json"],
        vec![".", "--platform", "web", "--grants", "", "--grants", ""],
        vec![".", "--platform", "web", "--rust-grants", ""],
        vec![".", "--platform", "web", "--grants", "", "--rust-grants"],
        vec![
            ".",
            "--platform",
            "web",
            "--grants",
            "",
            "--rust-grants",
            "--json",
        ],
    ] {
        let result = run(&args);
        assert_eq!(result.status.code(), Some(2), "{args:?}: {result:?}");
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).starts_with("usage: exact-bake compat "));
    }
    assert_eq!(
        std::fs::read_to_string(app.0.join("app.json")).unwrap(),
        "invalid JSON"
    );
    assert_eq!(std::fs::read_dir(&app.0).unwrap().count(), 2);
}

#[test]
fn versions_default_only_when_unset_and_do_not_change_the_cohort() {
    let app = App::new("version");
    app.write(
        "app.json",
        r#"{"app":{"id":"com.exact.version","name":"Version"}}"#,
    );
    let run = |version: Option<&str>, build: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_exact-bake"));
        command
            .args(["compat", ".", "--platform", "web", "--json"])
            .current_dir(&app.0)
            .env("EXACT_UPDATE_TRUST", "development")
            .env_remove("OUT_DIR")
            .env_remove("EXACT_VERSION")
            .env_remove("EXACT_BUILD_NUMBER");
        if let Some(v) = version {
            command.env("EXACT_VERSION", v);
        }
        if let Some(v) = build {
            command.env("EXACT_BUILD_NUMBER", v);
        }
        let result = command.output().unwrap();
        assert!(result.status.success(), "{result:?}");
        serde_json::from_slice::<Value>(&result.stdout).unwrap()
    };
    let default = run(None, None);
    assert_eq!(default["embedded"]["version"], "0.1.0");
    assert_eq!(default["embedded"]["buildNumber"], "1");
    for (version, build) in [("2.3.4", "56"), ("", "")] {
        let changed = run(Some(version), Some(build));
        assert_eq!(changed["embedded"]["version"], version);
        assert_eq!(changed["embedded"]["buildNumber"], build);
        assert_eq!(changed["id"], default["id"]);
        assert_eq!(changed["inputs"], default["inputs"]);
    }
}

#[test]
fn compat_uses_recorded_settings_instead_of_its_own_environment() {
    let app = App::new("preload-settings");
    app.write("app.json", r#"{"app":{"id":"test.preload-settings","name":"Settings"},"deploy":{"store":{"ios":"0","linux":"0","web":"0"}},"env":[{"name":"EONE_API","launch":true},"EONE_NOTE"],"preload":{"feed":{"url":"{env:EONE_API}/feed","headers":{"X-Note":"{env:EONE_NOTE}"}}}}"#);
    let settings = serde_json::json!({
        "EONE_API":{"value":"https://built.example","source":"default","launch":true},
        "EONE_NOTE":{"value":"built","source":"environment"}
    })
    .to_string();
    for platform in ["ios", "linux", "web"] {
        let output = Command::new(env!("CARGO_BIN_EXE_exact-bake"))
            .args([
                "compat",
                ".",
                "--platform",
                platform,
                "--grants",
                "net.fetch https:",
                "--settings",
                &settings,
                "--json",
            ])
            .current_dir(&app.0)
            .env("EXACT_UPDATE_TRUST", "development")
            .env("EONE_API", "https://unrelated.example")
            .env_remove("OUT_DIR")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let compat: Value = serde_json::from_slice(&output.stdout).unwrap();
        let preload = &compat["inputs"]["preload"][0];
        assert_eq!(
            preload["headers"][0][1],
            serde_json::json!([["literal", "built"]])
        );
        assert_eq!(
            preload["url"],
            if platform == "web" {
                serde_json::json!([["literal", "https://built.example/feed"]])
            } else {
                serde_json::json!([
                    ["env", "EONE_API", "https://built.example"],
                    ["literal", "/feed"]
                ])
            }
        );
    }
}

#[test]
fn android_compat_uses_the_merged_permissions_after_package_resolution() {
    let app = App::new("merged-permissions");
    let network = "android.permission.ACCESS_NETWORK_STATE";
    let capture = "android.permission.DETECT_SCREEN_CAPTURE";
    let installed = [network, capture];
    let device = app.0.join("node_modules/@exact/device");
    std::fs::create_dir_all(&device).unwrap();
    std::fs::write(device.join("exact.json"), r#"{"kind":"interface","name":"@exact/device","version":"1.0.0","grants":[],"shapes":{"BatteryManager":{"level":"number"}},"names":[{"source":"getBattery","live":true,"answers":"BatteryManager","substitute":{"default":{"level":1}}}],"views":[],"hatches":[]}"#).unwrap();
    std::fs::write(device.join("package.json"), r#"{"name":"@exact/device","exports":{"contract":"./exact.json","default":"./exact.json"}}"#).unwrap();
    let provider = app.0.join("node_modules/@fixture/provider/android");
    std::fs::create_dir_all(&provider).unwrap();
    std::fs::write(
        provider.join("../package.json"),
        r#"{"name":"@fixture/provider","exports":{"./android":"./android/exact.json"}}"#,
    )
    .unwrap();
    std::fs::write(provider.join("exact.json"), serde_json::json!({"kind":"implementation","implements":"@exact/device@^1.0","platform":"android","targets":["android"],"sources":{"kotlin":["."]},"class":"capture.Module","manifest":{"permissions":installed}}).to_string()).unwrap();
    app.write(
        "package.json",
        r#"{"dependencies":{"@exact/device":"1.0.0"}}"#,
    );
    let bake = |imports: bool, declared: &[&str], merged: &[&str]| {
        app.write("app.json", &serde_json::json!({"app":{"id":"com.example.capture","name":"Capture"},"implementations":{"@exact/device":{"android":"@fixture/provider/android"}},"host":{"android":{"permissions":declared}}}).to_string());
        let view = if imports {
            "use getBattery, BatteryManager from \"@exact/device\"\ncomponent App\n  resource battery = getBattery() as shape BatteryManager else empty(level=-1)\n  view\n    text \"battery\"\n"
        } else {
            "component App\n  view\n    text \"battery\"\n"
        };
        app.write("app.contract", view);
        let result = Command::new(env!("CARGO_BIN_EXE_exact-bake"))
            .arg("compat")
            .arg(&app.0)
            .args([
                "--target",
                "aarch64-linux-android",
                "--platform",
                "android",
                "--json",
            ])
            .env(
                "EXACT_ANDROID_MANIFEST_PERMISSIONS",
                serde_json::to_string(merged).unwrap(),
            )
            .env_remove("OUT_DIR")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<Value>(&result.stdout).unwrap()
    };
    let implicit = bake(true, &[], &installed);
    let repeated = bake(true, &installed, &installed);
    std::fs::write(
        device.join("package.json"),
        r#"{"name":"@exact/device","exports":{".":"./exact.json"}}"#,
    )
    .unwrap();
    let bound = bake(true, &installed, &installed);
    let dropped = bake(false, &installed, &installed);
    assert_eq!(
        bound["inputs"]["capabilities"],
        dropped["inputs"]["capabilities"]
    );
    let removed = bake(false, &[network], &[network]);
    assert_ne!(removed["id"], dropped["id"]);
    assert_eq!(
        removed["inputs"]["androidLinked"]["permissions"],
        serde_json::json!([network])
    );
    assert_eq!(implicit["id"], repeated["id"]);
    for compat in [&implicit, &repeated, &dropped] {
        assert_eq!(
            compat["inputs"]["androidLinked"]["permissions"],
            serde_json::json!(installed)
        );
        assert_eq!(
            compat["inputs"]["capabilities"],
            implicit["inputs"]["capabilities"]
        );
    }
}
