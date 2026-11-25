//! The terminal host's integration tests: one binary, so one link and one launch.

mod cli;
mod collections;
mod command_line;
mod geometry;
mod input;
mod reading;
mod screen;
mod todo;

use std::path::{Path, PathBuf};

/// A path in a directory of its own under the temp dir, the directory
/// removed when this drops.
pub struct Temp {
    dir: PathBuf,
    path: PathBuf,
}

impl Temp {
    /// The empty directory `name` under the temp dir.
    pub fn dir(name: String) -> Temp {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp {
            path: dir.clone(),
            dir,
        }
    }

    /// `source` as `app.contract` in [`Temp::dir`].
    pub fn entry(name: String, source: &str) -> Temp {
        let mut temp = Temp::dir(name);
        temp.path = temp.dir.join("app.contract");
        std::fs::write(&temp.path, source).unwrap();
        temp
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl std::ops::Deref for Temp {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Temp {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}
