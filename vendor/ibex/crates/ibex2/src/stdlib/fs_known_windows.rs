//! Create-only known-folder operations under retained handles (LLP 1173 D4).
use super::{
    fs::KnownFolder,
    windows_directory::{self, Directory},
    windows_fs::token,
};
use crate::stdlib::fs::{FsOp, FsResult};
use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(super) fn create(
    root: KnownFolder,
    parts: &[String],
    op: FsOp,
    source: Option<File>,
    data: &[u8],
) -> io::Result<FsResult> {
    for part in parts {
        super::windows_path::validate_name(part)?;
    }
    let top = Directory::known(&root.path, root.directory)?;
    if op == FsOp::Mkdir {
        return mkdir(top, parts, Directory::create_child).map(|()| FsResult::Done);
    }
    let mut parents = vec![top];
    let (leaf, ancestors) = parts
        .split_last()
        .map_or((None, &[][..]), |(leaf, rest)| (Some(leaf), rest));
    for part in ancestors {
        parents.push(parents.last().unwrap().child(part, false)?);
    }
    let parent = parents.last().unwrap();
    if op == FsOp::Realpath {
        let _entry = leaf.map(|leaf| parent.entry(leaf)).transpose()?;
        return Ok(FsResult::Text(
            parts
                .iter()
                .fold(root.path, |at, part| at.join(part))
                .display()
                .to_string(),
        ));
    }
    let leaf = leaf.ok_or_else(|| io::Error::new(io::ErrorKind::IsADirectory, "EISDIR"))?;
    if op == FsOp::AtomicWriteFile {
        return atomic(parent, leaf, data, || Ok(())).map(|()| FsResult::Done);
    }
    if let Some(source) = &source {
        if !source.metadata()?.is_file() {
            return Err(io::Error::other("copy needs a regular file"));
        }
    }
    // FILE_CREATE is CREATE_NEW's NT disposition: no existing entry is opened.
    let mut file = parent.file(leaf, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
    match source {
        Some(mut source) => {
            io::copy(&mut source, &mut file)?;
        }
        None => file.write_all(data)?,
    }
    Ok(FsResult::Done)
}

/// Create every component exclusively. A failure removes what this call
/// made, deepest first, each closed before its parent's removal.
fn mkdir(
    root: Directory,
    parts: &[String],
    create: impl Fn(&Directory, &str) -> io::Result<Directory>,
) -> io::Result<()> {
    let mut parents = vec![root];
    for (index, part) in parts.iter().enumerate() {
        match create(parents.last().unwrap(), part) {
            Ok(directory) => parents.push(directory),
            Err(error) => {
                let mut kept = Vec::new();
                for (directory, part) in parents.drain(1..).rev().zip(parts[..index].iter().rev()) {
                    if let Err(left) = directory.delete() {
                        kept.push(format!("{part}: {left}"));
                    }
                }
                if kept.is_empty() {
                    return Err(error);
                }
                return Err(io::Error::new(
                    error.kind(),
                    format!("left behind {}; {error}", kept.join(", ")),
                ));
            }
        }
    }
    Ok(())
}

fn atomic(
    parent: &Directory,
    leaf: &str,
    data: &[u8],
    before_commit: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let temp = format!(".ibex-{}", token()?);
    let mut file = parent.file(&temp, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
    let result = (|| {
        file.write_all(data)?;
        file.sync_all()?;
        before_commit()?;
        parent.rename_new(&file, leaf)
    })();
    if result.is_err() {
        // Through the handle, so that another entry given the name since is kept.
        let _ = windows_directory::delete(&file);
    }
    result
}

/// A drive's home pinned before app code runs, never the person's known folders.
pub struct WindowsAgentFolders {
    home: Directory,
    path: PathBuf,
}

impl WindowsAgentFolders {
    /// Retain the drive's home; every role is opened relative to that handle.
    pub fn new(home: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(home)?;
        let path = std::fs::canonicalize(home)?;
        let directory = Directory::root(
            path.to_str()
                .ok_or_else(|| io::Error::other("non-Unicode home"))?,
            false,
        )?;
        Ok(Self {
            home: directory,
            path,
        })
    }

    /// Create a role's directory without following links below the drive's home.
    pub fn resolve(&self, name: &str) -> io::Result<KnownFolder> {
        let relative = match name {
            "desktop" => "Desktop",
            "documents" => "Documents",
            "downloads" => "Downloads",
            "music" => "Music",
            "pictures" => "Pictures",
            "videos" => "Videos",
            "screenshots" => "Pictures/Screenshots",
            _ => return Err(io::Error::from_raw_os_error(2)),
        };
        let mut parents = vec![self.home.try_clone()?];
        for part in relative.split('/') {
            parents.push(parents.last().unwrap().child(part, true)?);
        }
        let directory = parents.pop().unwrap().0;
        Ok((self.path.join(relative), directory).into())
    }
}

#[cfg(test)]
#[path = "fs_known_windows_tests.rs"]
mod tests;
