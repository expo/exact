use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn missing_prerequisite(reason: &str) {
    assert_ne!(
        std::env::var("EXACT_REQUIRED").as_deref(),
        Ok("1"),
        "{reason}"
    );
    writeln!(io::stderr(), "SKIP: {reason}").expect("report missing prerequisite");
}

#[test]
fn cargo_reruns_simulator_preparation_when_the_llvm_override_changes() {
    const TARGET: &str = "aarch64-apple-ios-sim";
    let target_libdir = Command::new("rustc")
        .args(["--print", "target-libdir", "--target", TARGET])
        .output()
        .expect("run rustc");
    assert!(target_libdir.status.success(), "{target_libdir:?}");
    let target_libdir = String::from_utf8(target_libdir.stdout).expect("Rust target directory");
    if !Path::new(target_libdir.trim()).is_dir() {
        missing_prerequisite(&format!(
            "install Rust target {TARGET} to check Cargo's Simulator rebuild"
        ));
        return;
    }

    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let package = root.join("crates/hermes-lean-sys");
    fs::create_dir_all(package.join("src")).expect("temporary crate");
    for file in [
        "Cargo.toml",
        "build.rs",
        "build_support.rs",
        "receipt_schema.rs",
        "src/lib.rs",
    ] {
        fs::copy(source.join(file), package.join(file)).expect("copy crate inputs");
    }
    let install = root.join("install");
    let libraries = install.join("macos-static");
    fs::create_dir_all(&libraries).expect("Hermes libraries");
    fs::create_dir(install.join("hermes-headers")).expect("Hermes headers");
    for name in ["libhermesvm_a.a", "libjsi.a", "libboost_context.a"] {
        fs::write(libraries.join(name), b"!<arch>\n").expect("fixture archive");
    }
    let log = root.join("lipo.log");
    for name in ["first", "second"] {
        let bin = root.join(name);
        fs::create_dir(&bin).expect("LLVM bin directory");
        let tool = bin.join("llvm-lipo");
        fs::write(
            &tool,
            format!("#!/bin/sh\n[ \"$1\" = -archs ] || exit 99\nprintf '%s\\n' {name} >> \"$LIPO_LOG\"\nprintf 'arm64\\n'\n"),
        )
        .expect("fake llvm-lipo");
        fs::set_permissions(tool, fs::Permissions::from_mode(0o755)).expect("executable llvm-lipo");
    }

    let mut cargo = Command::new(env!("CARGO"));
    cargo
        .current_dir(&package)
        .args([
            "check",
            "--lib",
            "--offline",
            "--features",
            "link",
            "--target",
            TARGET,
            "--target-dir",
        ])
        .arg(root.join("cargo-target"))
        .env_remove("CARGO_BUILD_BUILD_DIR")
        .env_remove("EXACT_JS_ENGINE")
        .env("HERMES_LEAN_SYS_OFFLINE", "1")
        .env("HERMES_LEAN_SYS_DIR_aarch64_apple_ios_sim", install)
        .env("LIPO_LOG", &log);
    let lock = source.join("Cargo.lock");
    if lock.try_exists().expect("locate crate lockfile") {
        fs::copy(lock, package.join("Cargo.lock")).expect("copy crate lockfile");
        cargo.arg("--locked");
    }
    for (index, (bin, expected)) in [
        ("first", "first\n".repeat(3)),
        ("first", "first\n".repeat(3)),
        ("second", "first\n".repeat(3) + &"second\n".repeat(3)),
    ]
    .into_iter()
    .enumerate()
    {
        let output = cargo
            .env("EXACT_APPLE_LLVM_BIN", root.join(bin))
            .output()
            .expect("run Cargo");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if index == 0 && !output.status.success() {
            if let Some(reason) = stderr.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("Hermes engine resolution failed: Hermes bundle ")
                    .filter(|message| message.contains(" is not cached at "))
            }) {
                let detail = reason.split_once(';').map_or(reason, |(detail, _)| detail);
                missing_prerequisite(&format!("missing pinned host Hermes bundle {detail}"));
                return;
            }
        }
        assert!(output.status.success(), "{stderr}");
        assert_eq!(fs::read_to_string(&log).expect("llvm-lipo calls"), expected);
    }
}
