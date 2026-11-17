//! The exact2 checkout whose files the compiler reads while it runs: the
//! standard modules under `packages/`, the bake's TypeScript analysis, the
//! semantics (LLP 1174 D1).
//!
//! @ref LLP 1174 D1 (the executables find exact2's files at run time)
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// The checkout this process reads exact2's files from: the one an
/// executable found as it started ([`find_for_executable`]), else the one
/// this library was compiled in. The second is right for an app's build
/// script, which Cargo compiles in the checkout that runs it.
pub fn sdk_root() -> &'static Path {
    ROOT.get()
        .map(PathBuf::as_path)
        .unwrap_or(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")))
}

/// For an executable, first thing in `main`: the checkout is `EXACT_ROOT`
/// when it is set; else the nearest ancestor holding `contract/cli` of
/// `CARGO_MANIFEST_DIR` (which `cargo run` and `cargo test` set), then of the
/// executable's own directory (a `target/<profile>/` binary run directly).
/// A binary kept for the machine, or copied from another one, so never reads
/// the checkout that compiled it. Errs when none is found.
pub fn find_for_executable() -> Result<(), String> {
    let found = match std::env::var_os("EXACT_ROOT") {
        Some(named) => {
            let named = std::path::absolute(&named).unwrap_or_else(|_| named.into());
            if !is_checkout(&named) {
                return Err(format!(
                    "EXACT_ROOT is {}, which is no exact2 checkout (it has no contract/cli/Cargo.toml)",
                    named.display()
                ));
            }
            named
        }
        None => {
            let manifest = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
            let exe = std::env::current_exe()
                .and_then(std::fs::canonicalize)
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf));
            let checkout = [manifest, exe].into_iter().flatten().find_map(|from| {
                from.ancestors()
                    .find(|dir| is_checkout(dir))
                    .map(Path::to_path_buf)
            });
            checkout.ok_or(
                "this binary is outside an exact2 checkout: set EXACT_ROOT to the checkout whose files it reads (exact2's scripts set it)",
            )?
        }
    };
    let _ = ROOT.set(found);
    Ok(())
}

fn is_checkout(dir: &Path) -> bool {
    dir.join("contract/cli/Cargo.toml").is_file()
}
