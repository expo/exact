//! The helper a dragged `app:/` file is copied by (LLP 1094.000 D13): the
//! app's own executable started again, holding only the file as a path
//! alone (`O_PATH`) on its stdin and the copy on its stdout. The reopen,
//! read and close are that process's, so the app's SQLite keeps its POSIX
//! locks on a database dragged out, which closing any readable descriptor
//! of the file in the app's own process would drop (fcntl(2)).
#![allow(unsafe_code)]

use std::fs::File;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

/// The argument that starts the app's executable as the helper.
const HELPER: &str = "--exact-drag-copy";

/// Copy as the helper when this process was started as one: the exit code.
pub fn helper() -> Option<i32> {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new(HELPER)) {
        return None;
    }
    // SAFETY: the parent gave the helper these two descriptors and nothing else uses them.
    let (source, target) = unsafe { (BorrowedFd::borrow_raw(0), File::from_raw_fd(1)) };
    Some(
        match reopen(source).and_then(|source| clone_or_copy(&source, &target)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
    )
}

/// Copy `source`, an `app:/` file opened as a path alone, into `target`
/// through the helper; why it failed, as the helper said.
pub(crate) fn copy_app_file(source: OwnedFd, target: File) -> Result<(), String> {
    let mut helper = Command::new("/proc/self/exe");
    helper.arg(HELPER);
    // SAFETY: close_on_exec_beyond_stdio makes only system calls, which a closure run between fork and exec may.
    unsafe {
        helper.pre_exec(close_on_exec_beyond_stdio);
    }
    copy_through(&mut helper, source, target)
}

/// Mark every descriptor but stdin, stdout and stderr close-on-exec, so the
/// helper keeps those three alone: `close_range`'s `CLOSE_RANGE_CLOEXEC`
/// (Linux 5.11), else, where the kernel lacks it or a sandbox refuses it,
/// each descriptor `/proc/self/fd` lists. An error, which declines the
/// copy, when neither can. Async-signal-safe, as what runs between `fork`
/// and `exec` must be: system calls only, into a buffer on the stack.
fn close_on_exec_beyond_stdio() -> std::io::Result<()> {
    // SAFETY: close_range(2) takes no pointers.
    if unsafe {
        libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    } == 0
    {
        return Ok(());
    }
    close_on_exec_listed()
}

/// Mark each descriptor `/proc/self/fd` lists but stdin, stdout and stderr
/// close-on-exec; async-signal-safe, as [`close_on_exec_beyond_stdio`]. An
/// error when a record of the listing does not read or a descriptor is not
/// marked.
fn close_on_exec_listed() -> std::io::Result<()> {
    // SAFETY: the path is a terminated string.
    let listing = unsafe {
        libc::open(
            c"/proc/self/fd".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if listing < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut entries = [0u8; 4096];
    let marked = loop {
        // SAFETY: getdents64(2) writes at most `entries.len()` bytes into the buffer.
        let read = unsafe {
            libc::syscall(
                libc::SYS_getdents64,
                listing,
                entries.as_mut_ptr(),
                entries.len(),
            )
        };
        if read < 0 {
            break Err(std::io::Error::last_os_error());
        }
        if read == 0 {
            break Ok(());
        }
        if let Err(e) = mark_listed(&entries[..read as usize], listing) {
            break Err(e);
        }
    };
    // SAFETY: `listing` is the descriptor opened above.
    unsafe { libc::close(listing) };
    marked
}

/// Mark close-on-exec each descriptor `records` (getdents64(2)'s) names but
/// stdin, stdout, stderr and `listing`; an error for a record that does not
/// read or a descriptor not marked.
fn mark_listed(records: &[u8], listing: i32) -> std::io::Result<()> {
    let malformed = || std::io::Error::from(std::io::ErrorKind::InvalidData);
    // Each record: its inode (8 bytes), offset (8), length (2), type (1), then its name, ended by a NUL
    let mut at = 0;
    while at < records.len() {
        let length = match records.get(at + 16..at + 18) {
            Some(&[low, high]) => u16::from_ne_bytes([low, high]) as usize,
            _ => return Err(malformed()),
        };
        let name = records.get(at + 19..at + length).ok_or_else(malformed)?;
        let digits = &name[..name.iter().position(|&b| b == 0).ok_or_else(malformed)?];
        at += length.max(20);
        if digits == b"." || digits == b".." {
            continue;
        }
        let fd = digits
            .iter()
            .try_fold(0i32, |n, &b| {
                b.is_ascii_digit()
                    .then(|| n.checked_mul(10)?.checked_add(i32::from(b - b'0')))
                    .flatten()
            })
            .ok_or_else(malformed)?;
        // SAFETY: fcntl(2) on a descriptor number takes no pointers.
        if fd > 2
            && fd != listing
            && unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Run `helper` with `source` as its stdin and `target` as its stdout, and
/// wait for it.
pub(crate) fn copy_through(
    helper: &mut Command,
    source: OwnedFd,
    target: File,
) -> Result<(), String> {
    let output = helper
        .stdin(Stdio::from(source))
        .stdout(Stdio::from(target))
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|child| child.wait_with_output())
        .map_err(|e| format!("the copy helper did not start ({e})"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(if said.is_empty() {
        format!("the copy helper failed ({})", output.status)
    } else {
        said
    })
}

/// The regular file `path_only` names, opened for reading through
/// `/proc/self/fd`, as `O_PATH` descriptors are reopened (open(2)).
pub(crate) fn reopen(path_only: BorrowedFd<'_>) -> std::io::Result<File> {
    let file = File::open(format!("/proc/self/fd/{}", path_only.as_raw_fd()))?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("it is not a regular file"));
    }
    Ok(file)
}

/// Make `target` a copy of `source`: a clone sharing its blocks (`FICLONE`,
/// ioctl_ficlone(2)) where the file system makes one, whole at once, else
/// a streamed copy (`copy_file_range` where the kernel takes the two files,
/// else reads and writes, as `std::io::copy` does between files).
pub(crate) fn clone_or_copy(source: &File, target: &File) -> std::io::Result<()> {
    // SAFETY: both descriptors are open for the call.
    if unsafe { libc::ioctl(target.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) } == 0 {
        return Ok(());
    }
    std::io::copy(&mut &*source, &mut &*target).map(drop)
}

#[cfg(test)]
#[path = "drag_copy_tests.rs"]
mod tests;
