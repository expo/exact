//! The checkout the compiler reads exact2's files from is the one it runs
//! for, never the one that compiled it (LLP 1174 D1).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const APP: &str =
    "use probe from \"@exact/root-probe\"\n\ncomponent App\n  view\n    text \"hi\"\n";

struct Dir(PathBuf);
impl Dir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("exact-sdk-root-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `contract build` of `app` with neither of the variables that name a
/// checkout, then whatever `env` sets.
fn build(binary: &Path, app: &Path, env: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(binary);
    command
        .args(["build", "app.contract", "-o", "app.plan"])
        .current_dir(app)
        .env_remove("CARGO_MANIFEST_DIR")
        .env_remove("EXACT_ROOT");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

#[test]
fn under_exact_root_a_standard_module_is_that_checkouts() {
    let app = Dir::new("app");
    app.write("app.contract", APP);
    let root = Dir::new("checkout");
    root.write("contract/cli/Cargo.toml", "");
    root.write(
        "packages/root-probe/exact.json",
        "{\"kind\": \"interface\"}",
    );
    let binary = Path::new(env!("CARGO_BIN_EXE_contract"));

    let theirs = build(binary, &app.0, &[("EXACT_ROOT", &root.0)]);
    let said = String::from_utf8_lossy(&theirs.stderr);
    let module = root.0.join("packages/root-probe");
    assert!(
        said.contains("exact2's standard module")
            && said.contains(&format!("file:{}", module.display())),
        "{said}"
    );

    // Its own checkout, found from the binary's directory, has no such module.
    let own = build(binary, &app.0, &[]);
    let said = String::from_utf8_lossy(&own.stderr);
    assert!(
        said.contains("no package `@exact/root-probe`") && !said.contains("standard module"),
        "{said}"
    );
}

#[test]
fn a_binary_outside_a_checkout_needs_exact_root() {
    let app = Dir::new("outside");
    app.write("app.contract", APP);
    // As the machine's cache holds a kept binary: a copy, under no checkout.
    let binary = app.0.join("bin/contract");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    if std::fs::hard_link(env!("CARGO_BIN_EXE_contract"), &binary).is_err() {
        std::fs::copy(env!("CARGO_BIN_EXE_contract"), &binary).unwrap();
    }

    let refused = build(&binary, &app.0, &[]);
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("set EXACT_ROOT"),
        "{refused:?}"
    );

    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let named = build(&binary, &app.0, &[("EXACT_ROOT", &checkout)]);
    let said = String::from_utf8_lossy(&named.stderr);
    assert!(said.contains("no package `@exact/root-probe`"), "{said}");
}
