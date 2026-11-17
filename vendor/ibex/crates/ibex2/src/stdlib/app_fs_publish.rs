//! Move a host-owned response into an admitted app directory without copying its bytes.
use super::*;

impl AppDirectories {
    /// Publish by same-filesystem rename. On recovery an absent source and an
    /// existing destination mean the rename committed before its journal update.
    /// False means neither location has a body. Other errors remain retryable.
    pub fn publish_file(
        &self,
        grants: &GrantSet,
        path: &str,
        source: &Path,
    ) -> Result<bool, HostError> {
        use std::os::unix::fs::OpenOptionsExt;
        self.parse(path)?;
        crate::boundary::admit(grants, &Operation::FsWrite { path: path.into() })?;
        let (parent, leaf) = self.open_parent(path)?;
        let target_exists = match open_at(&parent, &leaf, libc::O_RDONLY | libc::O_NONBLOCK) {
            Ok(file) if file.metadata().map_err(error)?.is_file() => true,
            Ok(_) => return Err(error("write needs a regular file")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(error(e)),
        };
        let from = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(
                source
                    .parent()
                    .ok_or_else(|| error("missing source parent"))?,
            )
            .map_err(error)?;
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| error("invalid source name"))?;
        match open_at(&from, name, libc::O_RDONLY | libc::O_NONBLOCK) {
            Ok(file) => {
                if !file.metadata().map_err(error)?.is_file() {
                    return Err(error("source needs a regular file"));
                }
                file.sync_all().map_err(error)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                parent.sync_all().map_err(error)?;
                from.sync_all().map_err(error)?;
                return Ok(target_exists);
            }
            Err(e) => return Err(error(e)),
        }
        rename(&from, name, &parent, &leaf).map_err(error)?;
        parent.sync_all().map_err(error)?;
        from.sync_all().map_err(error)?;
        Ok(true)
    }
}
