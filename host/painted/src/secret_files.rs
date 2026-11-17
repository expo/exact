//! Android ordinary secrets use only the encrypted file format.
use exact_data::secrets::Backend;
use ibex2::secrets::{FileStore, SecretStore};
use std::path::{Path, PathBuf};

pub(crate) type Crypt = fn(u32, &str, &str) -> Result<String, String>;

pub(crate) struct EncryptedFiles {
    root: PathBuf,
    crypt: Crypt,
    opened: Result<(), String>,
}
impl EncryptedFiles {
    pub fn new(root: PathBuf, crypt: Crypt) -> Self {
        let mut store = Self {
            root,
            crypt,
            opened: Ok(()),
        };
        store.opened = store.locked(|_| Ok(()));
        store
    }

    fn locked<T>(&self, run: impl FnOnce(&FileStore) -> Result<T, String>) -> Result<T, String> {
        self.opened.clone()?;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.root)
            .map_err(|e| e.to_string())?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.root.join(".secret-encryption.lock"))
            .map_err(|e| e.to_string())?;
        if !lock.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("the secret encryption lock is not a regular file".into());
        }
        lock.lock().map_err(|e| e.to_string())?;
        // Remove plaintext secrets from disk for security; no values are read or migrated.
        let plain = self.root.join("secrets");
        match std::fs::symlink_metadata(&plain) {
            Ok(meta) => {
                if meta.is_dir() {
                    std::fs::remove_dir_all(&plain)
                } else {
                    std::fs::remove_file(&plain)
                }
                .map_err(|e| e.to_string())?;
                exact_data::secrets::sync_directory_chain(&self.root)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        run(&FileStore::new(self.root.join("secrets-encrypted")))
    }

    fn crypt(&self, op: u32, name: &str, value: &str) -> Result<String, String> {
        let context = self.root.join(name);
        (self.crypt)(
            op,
            context.to_str().ok_or("the secret root is not UTF-8")?,
            value,
        )
    }

    fn seal(&self, sealed: &FileStore, name: &str, value: &str) -> Result<(), String> {
        let value = self.crypt(1, name, value)?;
        sealed.set(name, &value).map_err(|e| e.to_string())?;
        exact_data::secrets::sync_directory_chain(&self.root.join("secrets-encrypted"))
    }
}
impl Backend for EncryptedFiles {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        self.locked(|sealed| {
            sealed
                .get(name)
                .map_err(|e| e.to_string())?
                .map(|value| self.crypt(2, name, &value))
                .transpose()
        })
    }

    fn write(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        self.locked(|sealed| {
            if let Some(value) = value {
                self.seal(sealed, name, value)
            } else {
                sealed.forget(name).map_err(|e| e.to_string())?;
                let dir = self.root.join("secrets-encrypted");
                if dir.exists() {
                    exact_data::secrets::sync_directory_chain(&dir)?;
                }
                Ok(())
            }
        })
    }
}

pub(super) fn names(root: &Path) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(root.join("secrets-encrypted")) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if ibex2::secrets::is_valid_name(&name) {
                    names.insert(name);
                }
            }
        }
    }
    names.into_iter().collect()
}

#[cfg(test)]
#[path = "secret_files_tests.rs"]
mod tests;
