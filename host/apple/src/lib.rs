//! The Apple host.
//!
//! @ref LLP 1008 (Apple host v1)
//! @ref LLP 1001 §5–6 (layout is a host call; text measurement is injected)
//! @ref LLP 1002 D2, §4 (every host but the web runs `exact-motion`)
//!
//! The runner and kernel run natively. After each commit the host lays out
//! the kernel tree, seeks motion to the app's clock and emits changed
//! presentation facts. An opted-in iOS presenter omits layout-only wrappers
//! strictly inside quiet virtualized list rows. Their children attach to
//! the nearest presented ancestor at composed frames (LLP 1171 D4).
//! macOS mirrors these boxes; inline runs and SVG elements belong to their
//! paragraph or scene on both hosts.
//!
//! Style rows cross as a typed dictionary keyed by the row's name, so a
//! row added to `schema.json` reaches the presenter with no change here.
//! The UIKit and AppKit presenters know nothing about the app.
//!
//! - [`style`] — style rows → the typed dictionary.
//! - [`batch`] — the JSON the presenter applies.
//! - [`measure`] — the text-measurement callback as a kernel [`TextMeasurer`].
//! - [`host`] — the runner wrapped for a presenter: receipts → batches, layout,
//!   motion, events, timers.
//! - [`store`] — the app's kept secrets (LLP 1018): `ibex2::host::Secrets`
//!   read into a snapshot before boot, written after each commit (kept
//!   answers by their own thread, `store::flush_kept`).
//! - [`delivery`] — optional delivery callbacks supplied by a higher adapter;
//!   the host itself links no update store (LLP 1030 D4).
//! - [`pan_velocity`] — the pan contact's release velocity (AppKit, the iOS agent).
//! - [`abi`] — the C exports: the web's buffer discipline over `extern "C"`.
//!
//! [`TextMeasurer`]: exact_kernel::TextMeasurer

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod abi;
pub mod app_module;
pub mod baked;
pub mod batch;
mod button;
pub mod calc;
pub mod canvas_text;
pub mod collapse;
pub mod content_region;
pub mod control_text;
pub mod corner;
pub mod decorations;
#[cfg(target_vendor = "apple")]
mod defaults;
pub mod delivery;
mod doc_watch;
pub mod documents;
pub mod executor;
/// The frame-health reducer, shared with the Android host (LLP 1079.000 §5.1).
pub use exact_runner::{frame_health, frame_health_exports};
pub mod geometry;
pub mod gradient;
pub mod host;
mod icon_path;
pub mod launch_preloads;
pub mod link;
mod looks;
pub mod markup;
pub mod material;
pub mod measure;
pub mod pan_velocity;
pub mod picker;
mod plan_bytes;
pub mod presence_secrets;
pub mod raster;
mod raster_exports;
pub mod store;
pub mod style;
pub mod svg_image;
pub mod textflow;
pub mod watch_sync;

pub use host::{Host, HostError};

pub mod time;

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
