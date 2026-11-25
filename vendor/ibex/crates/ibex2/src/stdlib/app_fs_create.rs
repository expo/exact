//! Create-only writes into well-known folders (LLP 1173 D4).
use super::*;

pub(super) fn atomic(dir: &File, target: &str, data: &[u8]) -> io::Result<()> {
    atomic_using(dir, dir, target, data, rename)
}

/// [`atomic`], with the temporary file made in `staging`, a directory on
/// the same filesystem as `dir`.
pub(super) fn atomic_in(staging: &File, dir: &File, target: &str, data: &[u8]) -> io::Result<()> {
    atomic_using(staging, dir, target, data, rename)
}

fn atomic_using(
    staging: &File,
    dir: &File,
    target: &str,
    data: &[u8],
    publish: impl FnOnce(&File, &str, &File, &str) -> io::Result<()>,
) -> io::Result<()> {
    let mut bytes = [0; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    let temp = format!(
        ".ibex-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    let mut file = open_at(
        staging,
        &temp,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
    )?;
    let result = (|| {
        file.write_all(data)?;
        file.sync_all()?;
        publish(staging, &temp, dir, target)
    })();
    if result.is_err() {
        let _ = unlink(staging, &temp, false);
    }
    result
}

fn rename_new(from: &File, source: &str, to: &File, target: &str) -> io::Result<()> {
    let source = name(source)?;
    let target = name(target)?;
    // SAFETY: both retained descriptors and terminated names live through the call.
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            from.as_raw_fd(),
            source.as_ptr(),
            to.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            from.as_raw_fd(),
            source.as_ptr(),
            to.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (from, source, to, target);
        return Err(io::Error::from(io::ErrorKind::Unsupported));
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(crate) fn create_below(
    op: FsOp,
    path: Below,
    source: Option<File>,
    data: &[u8],
) -> Result<FsResult, HostError> {
    if op == FsOp::Mkdir {
        let mut dir = path.root;
        for part in &path.parts {
            let encoded = name(part).map_err(error)?;
            // SAFETY: the retained parent and terminated component stay live.
            if unsafe { libc::mkdirat(dir.as_raw_fd(), encoded.as_ptr(), 0o700) } != 0 {
                return Err(error(io::Error::last_os_error()));
            }
            dir = directory(&dir, part).map_err(error)?;
        }
        return Ok(FsResult::Done);
    }
    let (parent, leaf) = path.parent()?;
    if op == FsOp::AtomicWriteFile {
        atomic_using(&parent, &parent, &leaf, data, rename_new).map_err(error)?;
    } else {
        if let Some(source) = &source {
            if !source.metadata().map_err(error)?.is_file() {
                return Err(error("copy needs a regular file"));
            }
        }
        let mut file = open_at(
            &parent,
            &leaf,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )
        .map_err(error)?;
        match source {
            Some(mut source) => {
                io::copy(&mut source, &mut file).map_err(error)?;
            }
            None => file.write_all(data).map_err(error)?,
        }
    }
    Ok(FsResult::Done)
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn atomic_creation_keeps_the_walks_parent_when_its_ancestor_is_replaced() {
        let base = std::env::temp_dir().join(format!("ibex-create-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root/sub")).unwrap();
        std::fs::create_dir_all(base.join("outside/sub")).unwrap();
        let parent = open_directory(&base.join("root/sub")).unwrap();
        atomic_using(
            &parent,
            &parent,
            "shot",
            b"png",
            |from, source, to, target| {
                std::fs::rename(base.join("root"), base.join("retained")).unwrap();
                std::os::unix::fs::symlink(base.join("outside"), base.join("root")).unwrap();
                rename_new(from, source, to, target)
            },
        )
        .unwrap();
        assert_eq!(
            std::fs::read(base.join("retained/sub/shot")).unwrap(),
            b"png"
        );
        assert!(!base.join("outside/sub/shot").exists());
        assert_eq!(
            atomic_using(&parent, &parent, "shot", b"overwrite", rename_new)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            std::fs::read_dir(base.join("retained/sub"))
                .unwrap()
                .count(),
            1
        );
        std::fs::remove_dir_all(base).unwrap();
    }
}
