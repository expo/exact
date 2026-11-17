//! Language-independent application data capabilities and composition.
//! @ref LLP 1027.001 — the same operation behind either language's data seam.

pub mod auth;
pub mod crypto;
pub mod documents;
pub mod envelope;
pub mod image;
pub mod known_folders;
mod mixed;
pub mod modules;
pub mod placed;
pub mod presence;
pub mod process;
#[cfg(not(target_arch = "wasm32"))]
pub mod secrets;
pub mod storage;
pub use mixed::Mixed;
pub use placed::Placed;

use exact_plan::Value;
use exact_runner::{Answer, DataError, DataSource, Outcome, Store, Target};

/// `answer`, for `target` when the caller named one: a composer records a
/// call with its target and hands the target on (LLP 1027 D1a).
fn answer<D: DataSource + ?Sized>(
    source: &mut D,
    target: Option<Target>,
    store: &mut Store,
    name: &str,
    args: &[Value],
) -> Result<Answer, DataError> {
    match target {
        Some(target) => source.answer_for(target, store, name, args),
        None => source.answer(store, name, args),
    }
}

/// `parse`, for `target` when the caller named one; see [`answer`].
fn parse<D: DataSource + ?Sized>(
    source: &mut D,
    target: Option<Target>,
    store: &mut Store,
    name: &str,
    args: &[Value],
    outcome: Outcome,
) -> Result<Answer, DataError> {
    match target {
        Some(target) => source.parse_for(target, store, name, args, outcome),
        None => source.parse(store, name, args, outcome),
    }
}

/// [`std::fs::File::try_lock`], which std implements with `flock(2)` only on
/// the targets its `cfg` lists: on Android it answers `Unsupported`, so on
/// every Unix this calls `flock` itself.
pub(crate) fn try_lock(file: &std::fs::File) -> Result<(), std::fs::TryLockError> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: the descriptor is this open file's, valid for the call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            return Err(std::fs::TryLockError::WouldBlock);
        }
        Err(std::fs::TryLockError::Error(error))
    }
    #[cfg(not(unix))]
    file.try_lock()
}

#[cfg(test)]
mod mixed_tests;
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

#[cfg(test)]
mod try_lock_tests {
    /// The lock excludes another open file description until its holder
    /// closes, and agrees with std's where std has one.
    #[test]
    #[cfg(unix)]
    fn try_lock_excludes_another_description_until_released() {
        let dir = crate::TempDir(
            std::env::temp_dir().join(format!("exact-data-lock-{}", std::process::id())),
        );
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.lock");
        let open = || {
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
                .unwrap()
        };
        let held = open();
        super::try_lock(&held).unwrap();
        let other = open();
        assert!(matches!(
            super::try_lock(&other),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        #[cfg(not(target_os = "android"))]
        assert!(matches!(
            other.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(held);
        super::try_lock(&other).unwrap();
        drop(other);
        let _ = std::fs::remove_dir_all(dir);
    }
}
