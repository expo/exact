use super::super::{compatibility_with_trust, Manifest};
use super::{app, id};
use std::path::Path;

/// The iOS compatibility inputs of the app at `dir`.
fn inputs(dir: &Path) -> serde_json::Value {
    let m = Manifest::read(dir).unwrap();
    compatibility_with_trust(dir, "ios", "aarch64-apple-ios", &m, Some(""), "development")
        .unwrap()
        .inputs
}

/// Writes a crate named `name` at `dir` whose manifest ends with `deps`.
fn krate(dir: &Path, name: &str, deps: &str) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub const A: u8 = 1;\n").unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{deps}"),
    )
    .unwrap();
}

/// The binary links the path crates its data crate depends on: an edit to
/// one moves the id, as an edit to `data/` does; a dev-dependency's does not.
/// An optional one counts though no default feature enables it: the build
/// that links the crate can (`--features app-data/native`).
#[test]
fn the_data_crates_path_dependencies_are_identity() {
    let dir = app("path-deps");
    assert!(inputs(&dir)["dataCrate"].get("crates").is_none());
    let workspace = "[workspace]\nmembers = [\"data\"]\nresolver = \"2\"\n";
    std::fs::write(dir.join("Cargo.toml"), workspace).unwrap();
    krate(
        &dir.join("data"),
        "app-data",
        "[dependencies]\nparse = { path = \"../parse\" }\n\
         helper = { path = \"../helper\", optional = true }\n\
         [dev-dependencies]\ntool = { path = \"../tool\" }\n\
         [features]\nnative = [\"dep:helper\"]\n",
    );
    krate(&dir.join("helper"), "helper", "");
    let leaf = "[build-dependencies]\nleaf = { path = \"../leaf\" }\n";
    krate(&dir.join("parse"), "parse", leaf);
    krate(&dir.join("leaf"), "leaf", "");
    krate(&dir.join("tool"), "tool", "");
    let crates = inputs(&dir)["dataCrate"]["crates"].clone();
    let names: Vec<&String> = crates.as_object().unwrap().keys().collect();
    assert_eq!(
        names,
        ["helper", "leaf", "parse"],
        "the closure, less dev-dependencies"
    );
    let first = id(&dir, "ios");
    std::fs::write(dir.join("tool/src/lib.rs"), "pub const A: u8 = 2;\n").unwrap();
    assert_eq!(first, id(&dir, "ios"), "a dev-dependency is not linked");
    std::fs::write(dir.join("parse/src/lib.rs"), "pub const A: u8 = 2;\n").unwrap();
    let parse = id(&dir, "ios");
    assert_ne!(first, parse, "a path dependency's edit moves it");
    std::fs::write(dir.join("leaf/src/lib.rs"), "pub const A: u8 = 2;\n").unwrap();
    let transitive = id(&dir, "ios");
    assert_ne!(parse, transitive, "so does a transitive one's");
    std::fs::write(dir.join("helper/src/lib.rs"), "pub const A: u8 = 2;\n").unwrap();
    assert_ne!(transitive, id(&dir, "ios"), "and an optional one's");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The data crate is the one `rust.data` names, wherever it sits beside
/// app.json, and a generated workspace's lock is its lockfile: an edit to
/// either moves the id (LLP 1030.007 D3).
#[test]
fn the_named_data_crate_and_the_generated_lock_are_identity() {
    let dir = app("named-data");
    std::fs::remove_dir_all(dir.join("data")).unwrap();
    let json = std::fs::read_to_string(dir.join("app.json")).unwrap();
    let named = json.replacen(
        '{',
        r#"{"rust":{"data":{"crate":"records","type":"records::Records"}},"#,
        1,
    );
    std::fs::write(dir.join("app.json"), named).unwrap();
    krate(&dir.join("records"), "records", "[workspace]\n");
    assert_eq!(
        inputs(&dir)["dataCrate"]["files"],
        3,
        "the named crate and its lockfile, not data/"
    );
    let first = id(&dir, "ios");
    std::fs::write(dir.join("records/src/lib.rs"), "pub const A: u8 = 2;\n").unwrap();
    let edited = id(&dir, "ios");
    assert_ne!(first, edited, "an edit to the named crate moves it");
    std::fs::create_dir_all(dir.join(".exact/shells")).unwrap();
    std::fs::write(dir.join(".exact/shells/Cargo.lock"), "version = 4\n").unwrap();
    let locked = id(&dir, "ios");
    assert_ne!(
        edited, locked,
        "the generated workspace's lock is the lockfile"
    );
    std::fs::write(
        dir.join(".exact/shells/Cargo.lock"),
        "version = 4\n# moved\n",
    )
    .unwrap();
    assert_ne!(locked, id(&dir, "ios"), "and its edit moves it");
}

/// The named data crate is found by its package name as Cargo reads it: a
/// comment after the `[package]` header or the name is TOML's, not part of
/// either, and a header may hold whitespace.
#[test]
fn a_comment_beside_the_package_name_still_names_the_data_crate() {
    for manifest in [
        "[package] # the app's records\nname = \"records\" # read by rust.data",
        "[ package ]\nname = 'records'",
    ] {
        let name = super::super::package_name(manifest);
        assert_eq!(name.as_deref(), Some("records"), "{manifest}");
    }
}

/// A path crate outside the app's workspace is no member of it, so the
/// closure is read from Cargo's full graph. An optional dependency there
/// that Cargo did not enable (`unused`) counts with what it declares
/// (`deep`), read without resolving its registry dependencies; one with no
/// manifest (`gone`) cannot be built. Such a crate can share its package
/// name with one Cargo enabled (`other`, `enabled`): both trees count.
#[test]
fn a_path_dependency_outside_the_workspace_is_identity() {
    let root = app("outside-deps");
    let dir = root.join("app");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::rename(root.join("app.json"), dir.join("app.json")).unwrap();
    std::fs::remove_dir_all(root.join("data")).unwrap();
    let workspace = "[workspace]\nmembers = [\"data\"]\nresolver = \"2\"\n";
    std::fs::write(dir.join("Cargo.toml"), workspace).unwrap();
    let shared = "[dependencies]\nshared = { path = \"../../shared\" }\n";
    krate(&dir.join("data"), "app-data", shared);
    krate(
        &root.join("shared"),
        "shared",
        "[dependencies]\nunused = { path = \"../unused\", optional = true }\n\
         renamed = { package = \"enabled\", path = \"../enabled\", optional = true }\n\
         gone = { path = \"../gone\", optional = true }\n\
         other = { package = \"enabled\", path = \"../other\", optional = true }\n\
         [features]\ndefault = [\"dep:renamed\"]\n",
    );
    krate(&root.join("other"), "enabled", "");
    let deep = "[dependencies]\ndeep = { path = \"../deep\" }\nin-no-registry = \"1\"\n";
    krate(&root.join("unused"), "unused", deep);
    krate(&root.join("deep"), "deep", "");
    krate(&root.join("enabled"), "enabled", "");
    let names: Vec<_> = super::super::path_crates(&dir.join("data"))
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["deep", "enabled", "enabled", "shared", "unused"]);
    assert!(!root.join("unused/Cargo.lock").exists());
    let first = id(&dir, "ios");
    assert_eq!(first, id(&dir, "ios"), "stable across computations");
    std::fs::write(root.join("shared/src/lib.rs"), "pub const A: u8 = 2;\n").unwrap();
    let shared = id(&dir, "ios");
    assert_ne!(first, shared);
    std::fs::write(root.join("deep/src/lib.rs"), "pub const A: u8 = 2;\n").unwrap();
    let mut last = id(&dir, "ios");
    assert_ne!(shared, last);
    for same_name in ["enabled", "other"] {
        let source = root.join(same_name).join("src/lib.rs");
        std::fs::write(source, "pub const A: u8 = 2;\n").unwrap();
        let edited = id(&dir, "ios");
        assert_ne!(last, edited, "an edit to {same_name} moves it");
        last = edited;
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// `tree_identity` joins path components with `/` and sorts by UTF-8 bytes
/// (matching `notification_service_identity`), so nested crates produce the
/// same `dataCrate.tree` digest on POSIX and Windows regardless of separator
/// or `PathBuf` component-wise sort order.
#[test]
fn tree_identity_uses_slash_joined_utf8_byte_order() {
    use sha2::{Digest, Sha256};

    let dir = app("tree-identity-canonical");
    let data = dir.join("data");
    std::fs::create_dir_all(data.join("src/a-b")).unwrap();
    std::fs::create_dir_all(data.join("src/a")).unwrap();
    std::fs::create_dir_all(data.join("src/mod")).unwrap();
    std::fs::write(data.join("src/a-b/file.rs"), "pub const AB: u8 = 1;\n").unwrap();
    std::fs::write(data.join("src/a/file.rs"), "pub const A: u8 = 2;\n").unwrap();
    std::fs::write(data.join("src/mod.rs"), "pub const MOD: u8 = 3;\n").unwrap();
    std::fs::write(data.join("src/mod/sub.rs"), "pub const SUB: u8 = 4;\n").unwrap();

    let compat_tree = inputs(&dir)["dataCrate"]["tree"].clone();
    let (digest, files) = super::super::tree_identity(&data).unwrap();
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    // `-` (0x2d) and `.` (0x2e) precede `/` (0x2f) in UTF-8 bytes, reversing
    // component-wise `PathBuf` order for `a-b` vs `a/` and `mod.rs` vs `mod/`.
    assert_eq!(
        names,
        [
            "Cargo.lock",
            "Cargo.toml",
            "src/a-b/file.rs",
            "src/a/file.rs",
            "src/lib.rs",
            "src/mod.rs",
            "src/mod/sub.rs",
        ]
    );
    let mut expected = Sha256::new();
    for name in &names {
        let bytes = std::fs::read(data.join(name)).unwrap();
        expected.update(name.as_bytes());
        expected.update([0]);
        expected.update((bytes.len() as u64).to_le_bytes());
        expected.update(&bytes);
    }
    let expected = super::super::hex(&expected.finalize());
    assert_eq!(digest, expected);
    assert_eq!(compat_tree, expected);
    let _ = std::fs::remove_dir_all(&dir);
}
