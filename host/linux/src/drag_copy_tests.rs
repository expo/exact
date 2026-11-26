use super::*;
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;

/// `path` opened as a path alone, as the app opens an `app:/` file.
fn path_only(path: &std::path::Path) -> File {
    File::options()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .unwrap()
}

#[test]
fn a_path_only_descriptor_is_reopened_for_reading_and_copied() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("notes.txt");
    std::fs::write(&source, "Mount Fuji, 3776 m").unwrap();
    let reopened = reopen(path_only(&source).as_fd()).unwrap();
    let target = File::create(folder.path().join("copy.txt")).unwrap();
    clone_or_copy(&reopened, &target).unwrap();
    assert_eq!(
        std::fs::read_to_string(folder.path().join("copy.txt")).unwrap(),
        "Mount Fuji, 3776 m"
    );
    assert!(
        reopen(path_only(folder.path()).as_fd()).is_err(),
        "a folder is not a file to copy"
    );
}

/// The helper takes the file as a path alone on its stdin and writes the copy
/// to its stdout, here played by `sh`; what it says on failure is the reason.
#[test]
fn the_helper_copies_from_its_stdin_to_its_stdout_and_its_failure_is_said() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("notes.txt");
    std::fs::write(&source, "Etna").unwrap();
    let copy = folder.path().join("copy.txt");
    let mut helper = Command::new("/bin/sh");
    helper.args(["-c", "cat /proc/self/fd/0"]);
    copy_through(
        &mut helper,
        path_only(&source).into(),
        File::create(&copy).unwrap(),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "Etna");
    let mut failing = Command::new("/bin/sh");
    failing.args(["-c", "echo 'No space left on device' >&2; exit 1"]);
    assert_eq!(
        copy_through(
            &mut failing,
            path_only(&source).into(),
            File::create(&copy).unwrap()
        ),
        Err("No space left on device".to_owned())
    );
}

/// Whether another process sees a write lock on `path` (`F_GETLK`, asked by a
/// child, so that no descriptor of the file opens or closes here).
fn locked_elsewhere(path: &std::ffi::CStr) -> bool {
    // SAFETY: the child makes only async-signal-safe calls (open, fcntl, _exit).
    unsafe {
        let pid = libc::fork();
        if pid == 0 {
            let fd = libc::open(path.as_ptr(), libc::O_RDONLY);
            let mut lock: libc::flock = std::mem::zeroed();
            lock.l_type = libc::F_WRLCK as _;
            lock.l_whence = libc::SEEK_SET as _;
            let held = fd >= 0
                && libc::fcntl(fd, libc::F_GETLK, &mut lock) == 0
                && lock.l_type != libc::F_UNLCK as libc::c_short;
            libc::_exit(held as i32);
        }
        let mut status = 0;
        libc::waitpid(pid, &mut status, 0);
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 1
    }
}

/// The app opens a file it drags only as a path alone, so a POSIX lock it
/// holds on it, as SQLite holds one on a database, survives the copy;
/// closing a readable descriptor of the file in the app would drop it.
#[test]
fn a_copy_through_a_path_alone_keeps_the_apps_lock_on_the_file() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("notes.db");
    std::fs::write(&source, "SQLite format 3").unwrap();
    let named = std::ffi::CString::new(source.as_os_str().as_encoded_bytes()).unwrap();
    let held = File::options()
        .read(true)
        .write(true)
        .open(&source)
        .unwrap();
    // SAFETY: an all-zero flock is a valid one; `held` is open for the call.
    let locked = unsafe {
        let mut lock: libc::flock = std::mem::zeroed();
        lock.l_type = libc::F_WRLCK as _;
        lock.l_whence = libc::SEEK_SET as _;
        libc::fcntl(held.as_raw_fd(), libc::F_SETLK, &lock)
    };
    assert_eq!(locked, 0);
    assert!(locked_elsewhere(&named));
    let copy = folder.path().join("copy.db");
    let mut helper = Command::new("/bin/sh");
    helper.args(["-c", "cat /proc/self/fd/0"]);
    copy_through(
        &mut helper,
        path_only(&source).into(),
        File::create(&copy).unwrap(),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "SQLite format 3");
    assert!(locked_elsewhere(&named), "the copy kept the lock");
    drop(File::open(&source).unwrap());
    assert!(
        !locked_elsewhere(&named),
        "closing a readable descriptor here drops it, as the copy did not"
    );
}

/// Where the kernel has no `close_range` or a sandbox refuses it, each
/// descriptor `/proc/self/fd` lists but the first three is marked
/// close-on-exec instead: a descriptor a child holds without the mark is
/// gone after its exec, while one it does not mark stays.
#[test]
fn without_close_range_each_listed_descriptor_closes_on_exec() {
    let after_exec = |mark: bool| {
        let mut child = Command::new("/bin/sh");
        child.args(["-c", "[ -e /proc/self/fd/50 ] && echo open || echo closed"]);
        // SAFETY: dup2(2) and close_on_exec_listed make system calls only, as a closure between fork and exec may.
        unsafe {
            child.pre_exec(move || {
                // A copy of stderr at 50, without the close-on-exec mark, as a C library may leave one
                if libc::dup2(2, 50) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if mark {
                    close_on_exec_listed()?;
                }
                Ok(())
            });
        }
        let output = child.output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(
        after_exec(false),
        "open",
        "unmarked, it stays across the exec"
    );
    assert_eq!(after_exec(true), "closed");
}

/// A listing record that does not read is an error, so the copy is
/// declined rather than run with descriptors left unmarked.
#[test]
fn a_listing_that_does_not_read_declines_the_copy() {
    let mut record = [0u8; 24];
    record[16..18].copy_from_slice(&24u16.to_ne_bytes());
    record[19..21].copy_from_slice(b"7x");
    assert!(
        mark_listed(&record, -1).is_err(),
        "a name that is no number"
    );
    assert!(
        mark_listed(&record[..17], -1).is_err(),
        "a record cut short"
    );
    let mut dot = [0u8; 24];
    dot[16..18].copy_from_slice(&24u16.to_ne_bytes());
    dot[19] = b'.';
    assert!(mark_listed(&dot, -1).is_ok(), "`.` names no descriptor");
}
