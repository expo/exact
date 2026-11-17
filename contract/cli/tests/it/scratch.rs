//! A test's directory or file under the temp dir, removed when the guard drops, so a
//! failing or panicking test leaves nothing behind either.

use std::ffi::OsStr;
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// Derefs to `path`, a path inside `root`; dropping it removes `root`, a
/// directory or a file.
pub struct Scratch {
    root: PathBuf,
    path: PathBuf,
}

impl Scratch {
    /// `temp_dir()/<name>`, emptied and created.
    pub fn new(name: impl AsRef<Path>) -> Self {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self::adopt(dir)
    }

    /// Removes `root` on drop, touching nothing before.
    pub fn adopt(root: PathBuf) -> Self {
        Self {
            path: root.clone(),
            root,
        }
    }

    /// The same guard, dereferencing to `rel` inside the directory.
    pub fn at(mut self, rel: impl AsRef<Path>) -> Self {
        self.path = self.root.join(rel);
        self
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for Scratch {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.root).is_err() {
            let _ = std::fs::remove_file(&self.root);
        }
    }
}
