//! EXACT: Metal libraries compiled ahead of time (EXACT-PATCHES.md, 16).
//!
//! `load_shader` writes MSL, which Metal compiles to AIR in
//! `newLibraryWithSource` on every launch whose shader cache misses: on a
//! fresh install, most of a game's first-frame pipeline time. A run may record
//! the MSL it compiled (`record`); a build compiles each recorded source to a
//! `.metallib` named by its key (`host/apple/metal.mjs`) and ships them, and
//! the device loads that library in place of the source when its key matches.
//! The key covers the source and the compile options, so a stale library
//! never matches; one that fails to load falls back to the source.
use alloc::{format, string::String};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSString, NSURL};
use objc2_metal::{MTLDevice, MTLLanguageVersion, MTLLibrary};
use std::{path::PathBuf, sync::OnceLock};

/// Where shipped libraries are read and recorded sources written.
#[derive(Debug, Default)]
struct Places {
    libraries: Option<PathBuf>,
    sources: Option<PathBuf>,
}
static PLACES: OnceLock<Places> = OnceLock::new();

/// Set once per process, before the first pipeline: `libraries` holds
/// `<key>.metallib` files a build compiled; `sources`, when set, receives
/// `<key>.metal` and `<key>.json` for each source compiled at run time.
/// Later calls change nothing.
pub fn set_shader_libraries(libraries: Option<PathBuf>, sources: Option<PathBuf>) {
    let _ = PLACES.set(Places { libraries, sources });
}

/// The key of `source` compiled with these options: FNV-1a 64 over the
/// options and the text, which is fixed across Rust releases and hosts.
pub(super) fn key(source: &str, version: MTLLanguageVersion, invariance: bool) -> String {
    let options = format!(
        "msl {}.{} invariance {invariance}\n",
        version.0 >> 16,
        version.0 & 0xffff
    );
    let hash = options
        .bytes()
        .chain(source.bytes())
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        });
    format!("{hash:016x}")
}

/// The shipped library for `key`, if one exists and Metal loads it.
pub(super) fn shipped(
    device: &ProtocolObject<dyn MTLDevice>,
    key: &str,
) -> Option<Retained<ProtocolObject<dyn MTLLibrary>>> {
    let path = PLACES
        .get()?
        .libraries
        .as_ref()?
        .join(format!("{key}.metallib"));
    if !path.is_file() {
        return None;
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    match device.newLibraryWithURL_error(&url) {
        Ok(library) => Some(library),
        Err(error) => {
            log::warn!("shipped Metal library {key}: {error}");
            None
        }
    }
}

/// Write a source compiled at run time where a build will find it, once
/// per key; the JSON names the options `metal` needs to compile it alike.
pub(super) fn record(key: &str, source: &str, version: MTLLanguageVersion, invariance: bool) {
    let Some(dir) = PLACES.get().and_then(|p| p.sources.as_ref()) else {
        return;
    };
    let metal = dir.join(format!("{key}.metal"));
    if metal.is_file() || std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let options = format!(
        "{{\"msl\":\"{}.{}\",\"preserveInvariance\":{invariance}}}\n",
        version.0 >> 16,
        version.0 & 0xffff
    );
    // The options first: a source without them is not compiled.
    let _ = std::fs::write(dir.join(format!("{key}.json")), options);
    let temporary = dir.join(format!("{key}.{}.tmp", std::process::id()));
    if std::fs::write(&temporary, source).is_ok() {
        let _ = std::fs::rename(&temporary, metal);
    }
    let _ = std::fs::remove_file(temporary);
}
