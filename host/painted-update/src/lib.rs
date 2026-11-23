//! The update store a painted host composes (LLP 1030 D4; LLP 1030.006
//! D3): [`Updates`], `exact_update::Client` behind the host's lock, check
//! thread and wake, as the presenter's [`Store`]. The Linux host's update
//! entry (`exact-linux-update`) links it, and an Android library whose bake
//! records store `"A"`; a binary-only app links neither.
#![deny(missing_docs)]
mod store;
use exact_painted::delivery::{Selection, Store};
use exact_runner::Delivery;
use std::sync::Arc;
pub use store::Updates;

/// [`Updates::open`] as the host's [`Opener`](exact_painted::delivery::Opener):
/// what an Android library at store `"A"` hands `canvas_jni!`.
pub fn open(
    compat: &str,
    plan: &[u8],
    assets: &std::path::Path,
    base: Option<&std::path::Path>,
) -> Result<Box<dyn Store>, String> {
    Ok(Box::new(Updates::open(compat, plan, assets, base)?))
}

impl Store for Updates {
    fn prepare_selected(&mut self) -> Option<Selection> {
        let prepared = Updates::prepare_selected(self)?;
        // A reader opens some assets by path, past the resolver's check: the
        // whole generation is checked before it boots (LLP 1030.006 D1).
        if let Err(why) = prepared.assets.verify() {
            self.selection_corrupt(&prepared.generation, &why);
            return None;
        }
        self.pin(prepared.generation.clone());
        Some(selection(prepared))
    }
    fn boot_started(&mut self) {
        Updates::boot_started(self);
    }
    fn entry_refused(&mut self, entry: &str, why: &str) {
        Updates::entry_refused(self, entry, why);
    }
    fn selection_corrupt(&mut self, why: &str) {
        self.refuse_pinned(why);
    }
    fn boot_succeeded(&mut self) {
        Updates::boot_succeeded(self);
    }
    fn take_note(&mut self) -> Option<String> {
        Updates::take_note(self)
    }
    fn fd(&self) -> std::os::unix::io::RawFd {
        Updates::fd(self)
    }
    fn check(&self, wake: fn()) -> bool {
        self.check_and_wake(wake)
    }
    fn take_line(&mut self) -> Option<String> {
        Updates::take_line(self)
    }
    fn prepare_activation(&self) -> Result<Option<Selection>, String> {
        let Some(prepared) = Updates::prepare_activation(self)? else {
            return Ok(None);
        };
        Ok(Some(selection(prepared)))
    }
    fn commit_activation(&mut self, entry: Option<String>, seq: u64) -> Result<(), String> {
        Updates::commit_activation(self, &exact_update::Generation { entry, seq })
    }
    fn staged_stream_into(&self, delivery: &mut Delivery) {
        Updates::staged_stream_into(self, delivery);
    }
    fn status_into(&self, delivery: &mut Delivery) {
        Updates::status_into(self, delivery);
    }
}

/// A pinned generation with lazy reads and a full check for path-based readers.
fn selection(prepared: exact_update::PreparedSelection) -> Selection {
    let assets = prepared.assets.clone();
    Selection {
        entry: prepared.generation.entry,
        seq: prepared.generation.seq,
        plan: prepared.plan,
        names: prepared.assets.names(),
        dir: Some(prepared.assets.root().to_path_buf()),
        verify_assets: Box::new(move || assets.verify()),
        assets: Arc::new(move |name| prepared.assets.resolve(name)),
    }
}

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
