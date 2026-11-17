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
