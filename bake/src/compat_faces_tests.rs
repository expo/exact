use super::{compatibility_with_trust, Manifest};

fn app(name: &str) -> crate::TempDir {
    let dir =
        std::env::temp_dir().join(format!("exact-compat-faces-{name}-{}", std::process::id()));
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

/// A tool `add`: `description`, a parameter with `cases`, and a send when
/// `sends`.
struct Tool<'a> {
    description: &'a str,
    cases: &'a str,
    sends: bool,
}

const ADD: Tool = Tool {
    description: "Add.",
    cases: r#""a", "b""#,
    sends: false,
};

/// Platform, target: iOS, macOS, Android, the web, tvOS.
const TARGETS: [(&str, &str); 5] = [
    ("ios", "aarch64-apple-ios"),
    ("macos", "aarch64-apple-darwin"),
    ("android", "aarch64-linux-android"),
    ("web", "wasm32-unknown-unknown"),
    ("ios", "aarch64-apple-tvos"),
];

/// A faced tool's signature is compiled into the binaries with its door
/// (LLP 1133.023 D8): Apple's and Android's, and the Mac's for the mcp door,
/// whose helper carries the catalogue (LLP 1111.000 D1). A change to it
/// moves those ids, never the web's or tvOS's, and an unfaced tool moves
/// none.
#[test]
fn a_faced_tools_signature_moves_the_binaries_with_its_door() {
    let dir = app("identity");
    let manifest = std::fs::read_to_string(dir.join("app.json")).unwrap();
    let ids = |tool: &Tool, doors: Option<&str>| {
        let send = if tool.sends {
            "\n    send logged = add(by)"
        } else {
            ""
        };
        let source = format!(
            "shape Ack\n  ok: bool\n\ncomponent A\n  state n = 0\n  mutation logged as shape Ack\n  tool add writes \"{}\" = add\n  action add(by: number, kind: enum<{}>)\n    n = n + by{send}\n  view\n    text `${{n}}`\n",
            tool.description, tool.cases
        );
        std::fs::write(dir.join("app.contract"), source).unwrap();
        let face = match doors {
            None => "{".into(),
            Some(doors) => {
                format!(r#"{{"intents":{{"tools":{{"add":{{"title":"Add"{doors}}}}}}},"#)
            }
        };
        std::fs::write(dir.join("app.json"), manifest.replacen('{', &face, 1)).unwrap();
        let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
        TARGETS.map(|(platform, target)| {
            let m = Manifest::read(&dir).unwrap();
            let mut c =
                compatibility_with_trust(&dir, platform, target, &m, Some(""), "development")
                    .unwrap();
            super::faces(&mut c, &plan, platform, target);
            c.id
        })
    };
    let [ios, macos, android, web, tvos] = [0, 1, 2, 3, 4];
    let more = Tool {
        description: "Add more.",
        ..ADD
    };
    let bare = ids(&ADD, None);
    assert_eq!(bare, ids(&more, None), "an unfaced tool is plan data");
    let faced = ids(&ADD, Some(""));
    let moved = ids(&more, Some(""));
    assert_ne!(
        bare[ios], faced[ios],
        "a face is compiled into the Apple binary"
    );
    assert_ne!(faced[ios], moved[ios], "so is the tool's description");
    assert_ne!(faced[macos], moved[macos], "on the Mac too");
    assert_ne!(faced[android], moved[android], "and into Android's");
    assert_eq!(faced[web], moved[web], "the web registers it at run time");
    assert_eq!(faced[tvos], moved[tvos], "tvOS compiles no faces");

    let mcp = Some(r#","doors":["mcp"]"#);
    let local = ids(&ADD, mcp);
    assert_ne!(
        bare[macos], local[macos],
        "the Mac's helper carries an mcp face"
    );
    for platform in [ios, android, web, tvos] {
        assert_eq!(
            bare[platform], local[platform],
            "only the Mac has the mcp door"
        );
    }
    let cased = ids(
        &Tool {
            cases: r#""a", "b", "c""#,
            ..ADD
        },
        mcp,
    );
    assert_ne!(
        local[macos], cased[macos],
        "an enum's cases are in the signature"
    );
    let sending = ids(&Tool { sends: true, ..ADD }, mcp);
    assert_ne!(
        local[macos], sending[macos],
        "so is whether the reach sends"
    );
    assert_eq!(local[ios], sending[ios]);
}

/// Android build scripts pass `platform = "linux"` with an `*-linux-android`
/// triple while `exact-bake compat` passes `platform = "android"`. Both must
/// produce identical compatibility JSON, faced tools, Android reach, sign-in
/// audiences, privacy-policy and color refusals, and `android-<target>.*`
/// bake output stems (without Linux's `displayName`).
#[test]
fn android_linux_and_android_platform_spellings_agree() {
    let dir = app("android-parity");
    std::fs::create_dir_all(dir.join("strings")).unwrap();
    std::fs::write(
        dir.join("strings/en.json"),
        r#"{"mic":"Records.","bg":"Recording","health":"Reads steps.","lan":"Finds peers."}"#,
    )
    .unwrap();
    let manifest = r#"{"name":"App","icons":[{"src":"assets/a.png"}],"app":{"id":"com.example.app","name":"App"},"privacy_policy":"https://example.com/privacy","intents":{"tools":{"add":{"title":"Add"}}},"deploy":{"origin":"continuous","bundles":"continuous"}}"#;
    std::fs::write(dir.join("app.json"), manifest).unwrap();
    std::fs::write(
        dir.join("app.contract"),
        "component A\n  state n = 0\n  tool add writes \"Add.\" = add\n  action add(by: number)\n    n = n + by\n  view\n    text `${n}` color=\"#112233\"\n",
    )
    .unwrap();
    let grants = "device.microphone mic\ndevice.microphone-background bg\ndevice.health-read health\ndevice.local-network lan\nnet.dns-sd _demo._tcp\nauth.google client.apps.googleusercontent.com";
    let target = "aarch64-linux-android";
    let m = Manifest::read(&dir).unwrap();
    let from_linux =
        super::compatibility_id(&dir, "linux", target, &m, Some(grants), false).unwrap();
    let from_android =
        super::compatibility_id(&dir, "android", target, &m, Some(grants), false).unwrap();
    assert_eq!(from_linux.to_json(), from_android.to_json());
    assert_eq!(from_linux.inputs["platform"], "android");
    assert!(from_linux.inputs["faces"].is_array());
    assert_eq!(
        from_linux.reach["auth"]["signIn"]["audiences"],
        serde_json::json!({"google": ["client.apps.googleusercontent.com"]})
    );
    let perms = from_linux.reach["android"].as_array().unwrap();
    assert!(perms.iter().any(|p| p == "android.permission.RECORD_AUDIO"));
    assert!(perms
        .iter()
        .any(|p| p == "android.permission.CHANGE_WIFI_MULTICAST_STATE"));
    assert!(from_linux.reach["androidServices"]["capture"].is_object());

    // Without privacy_policy, both spellings refuse Health Connect.
    std::fs::write(
        dir.join("app.json"),
        manifest.replace(r#""privacy_policy":"https://example.com/privacy","#, ""),
    )
    .unwrap();
    let m_no_policy = Manifest::read(&dir).unwrap();
    for platform in ["linux", "android"] {
        let err =
            super::compatibility_id(&dir, platform, target, &m_no_policy, Some(grants), false)
                .unwrap_err();
        assert!(
            err.starts_with("grant-privacy-policy:"),
            "{platform}: {err}"
        );
    }

    // Color and 8-bit canvas refusals apply equally under both spellings.
    std::fs::write(
        dir.join("app.json"),
        r#"{"name":"App","icons":[{"src":"assets/a.png"}],"app":{"id":"com.example.app","name":"App"}}"#,
    )
    .unwrap();
    let m = Manifest::read(&dir).unwrap();
    for (contract, expect) in [
        (
            "component A\n  view\n    box width=4 height=4 background-color=\"color(display-p3 1 0 0)\"\n",
            "outside sRGB",
        ),
        (
            "component A\n  view\n    box width=4 height=4 background-color=\"color(--dci-p3 1 0 0)\"\n",
            "color profile",
        ),
        (
            "component A\n  view\n    canvas surface=dot() width=4 height=4 color-space=\"display-p3\"\n",
            "8-bit sRGB",
        ),
    ] {
        std::fs::write(dir.join("app.contract"), contract).unwrap();
        for platform in ["linux", "android"] {
            let err =
                super::compatibility_id(&dir, platform, target, &m, Some(""), false).unwrap_err();
            assert!(err.contains(expect), "{platform}: {err}");
        }
    }

    // Receipt emission under ("android", target) uses the `android-<target>` stem and omits Linux's displayName.
    std::fs::write(
        dir.join("app.contract"),
        "component A\n  view\n    text \"ok\"\n",
    )
    .unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("app.plan"), plan.encode()).unwrap();
    let mut compat = super::compatibility_id(&dir, "linux", target, &m, Some(""), false).unwrap();
    crate::receipt::emit(
        &mut compat,
        "development",
        &dir,
        ("android", target),
        &m,
        &out,
        &Default::default(),
    )
    .unwrap();
    assert!(compat.embedded.get("displayName").is_none());
}
