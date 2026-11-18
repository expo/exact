//! LLP 1163.000 D2: the alternate icons' names are the iOS binary's
//! identity, in their order; their art and other hosts are not.

use super::{compatibility_with_trust, Manifest};

fn app(name: &str) -> crate::TempDir {
    let dir = std::env::temp_dir().join(format!(
        "exact-compat-alternate-icons-{name}-{}",
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
    crate::TempDir(dir)
}

/// The id of the app with `alternates` (the members of `alternate_icons`,
/// as JSON) on `platform`.
fn id(dir: &std::path::Path, platform: &str, target: &str, alternates: &str) -> String {
    let manifest = format!(
        r#"{{"name":"App","icons":[{{"src":"assets/a.png"}}],"alternate_icons":[{alternates}],"app":{{"id":"com.example.app","name":"App"}},"host":{{"ios":{{"minimumOS":"17.0"}}}}}}"#
    );
    std::fs::write(dir.join("app.json"), manifest).unwrap();
    let m = Manifest::read(dir).unwrap();
    compatibility_with_trust(dir, platform, target, &m, Some(""), "development")
        .unwrap()
        .id
}

#[test]
fn the_ios_roster_moves_the_id_and_its_art_does_not() {
    let dir = app("roster");
    let ios = |alternates: &str| id(&dir, "ios", "aarch64-apple-ios", alternates);
    let paper = r#"{"name":"Paper","icons":[{"src":"assets/paper.png"}]}"#;
    let night = r#"{"name":"Night","icons":[{"src":"assets/night.png"}]}"#;
    let none = ios("");
    let one = ios(paper);
    let two = ios(&format!("{paper},{night}"));
    assert_ne!(none, one, "adding an alternate needs a new binary");
    assert_ne!(one, two, "so does adding another");
    assert_eq!(none, ios(""), "and removing them all returns to the first");
    assert_ne!(
        two,
        ios(&format!("{night},{paper}")),
        "the order is the picker's"
    );
    assert_ne!(
        one,
        ios(&paper.replace("Paper", "Card")),
        "a rename moves it"
    );
    let redrawn = paper.replace("paper.png", "paper-2.png");
    assert_eq!(one, ios(&redrawn), "new art is the binary's alone");
    let project = paper.replace('}', r#","host":{"ios":{"icon":"art/Paper.icon"}}}"#);
    assert_eq!(one, ios(&project), "so is where it comes from");
    for (platform, target) in [
        ("macos", "aarch64-apple-darwin"),
        ("android", "aarch64-linux-android"),
        ("web", "wasm32-unknown-unknown"),
        // tvOS and watchOS bake as iOS.
        ("ios", "aarch64-apple-tvos-sim"),
        ("ios", "aarch64-apple-tvos"),
        ("ios", "aarch64-apple-watchos-sim"),
        ("ios", "aarch64-apple-watchos"),
    ] {
        assert_eq!(
            id(&dir, platform, target, ""),
            id(&dir, platform, target, paper),
            "{target} builds no alternate"
        );
    }
}
