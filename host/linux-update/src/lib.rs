//! Optional Linux delivery composition (LLP 1030 D4).
//! A binary-only app calls `exact_linux::run`; an update-capable app calls this crate.
#![deny(missing_docs)]
pub use exact_painted_update::Updates;
use exact_runner::{DataSource, Delivery};

/// Run the app with its update store attached before boot.
pub fn run<D: DataSource + Default>(baked: &[u8], compat: &str) -> i32 {
    run_composed::<D>(baked, compat, Default::default())
}

/// [`run`] with the app's hatches (LLP 1075.003.000.001 §5), as
/// `exact_linux::run_with_hatches` takes them.
pub fn run_with_hatches<D: DataSource + Default, H: exact_painted::Hatches>(
    baked: &[u8],
    compat: &str,
    words: &'static [&'static str],
) -> i32 {
    let composed = exact_painted::Composed {
        hatches: Some(exact_painted::hatches::install::<H>(words)),
        ..Default::default()
    };
    run_composed::<D>(baked, compat, composed)
}

/// [`run`] with what the app's build composed, as `exact_linux::run_composed`
/// takes it.
pub fn run_composed<D: DataSource + Default>(
    baked: &[u8],
    compat: &str,
    composed: exact_painted::Composed,
) -> i32 {
    run_installed::<D>(baked, compat, composed)
}

fn run_installed<D: DataSource + Default>(
    baked: &[u8],
    compat: &str,
    composed: exact_painted::Composed,
) -> i32 {
    if exact_painted::app::print_baked_receipt(compat) {
        return 0;
    }
    if let Err(why) = exact_runner::delivery::refuse_analysis(compat) {
        eprintln!("exact: {why}");
        return 1;
    }
    if Delivery::default().with_compat(compat).store == '0' {
        eprintln!(
            "exact: this binary-only app must use the core host entry; regenerate the app entry"
        );
        return 1;
    }
    let started = std::time::Instant::now();
    let mut config = exact_painted::app::Config::from_env(baked, compat);
    config.hatches = composed.hatches;
    config.packages = composed.packages;
    let base = exact_painted::picker::xdg_base("XDG_DATA_HOME", ".local/share");
    match Updates::open(compat, baked, &config.assets, base.as_deref()) {
        Ok(updates) => config.use_updates(Box::new(updates), baked),
        Err(error) => eprintln!("exact update: {error}"),
    }
    exact_linux::run_config::<D>(&mut config, started)
}

#[cfg(test)]
mod app_tests;

/// A test's directory, removed when dropped, so a test that fails or panics
/// leaves nothing in the system temp directory.
#[cfg(test)]
pub(crate) struct TempDir(pub(crate) std::path::PathBuf);

#[cfg(test)]
impl std::ops::Deref for TempDir {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(test)]
impl AsRef<std::path::Path> for TempDir {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(test)]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
