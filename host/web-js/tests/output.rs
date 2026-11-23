//! The compiler's own process, as `host/web-js/build.mjs` runs it.

/// A parent that left `O_NONBLOCK` on the stderr pipe it shares gets it back
/// blocking: on Linux a nonblocking pipe fails a burst of warnings with
/// `EAGAIN`, and the compiler exits 101 with its message lost (#1337).
#[cfg(unix)]
#[test]
fn a_nonblocking_stderr_pipe_is_made_blocking() {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let mut ends = [0; 2];
    // SAFETY: `pipe` fills both descriptors, which `OwnedFd` then owns.
    let (read, write) = unsafe {
        assert_eq!(libc::pipe(ends.as_mut_ptr()), 0);
        (OwnedFd::from_raw_fd(ends[0]), OwnedFd::from_raw_fd(ends[1]))
    };
    let flags = || unsafe { libc::fcntl(write.as_raw_fd(), libc::F_GETFL) };
    // SAFETY: sets a flag of the descriptor owned above.
    unsafe { libc::fcntl(write.as_raw_fd(), libc::F_SETFL, flags() | libc::O_NONBLOCK) };
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_exact-web-js"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(write.try_clone().unwrap())
        .status()
        .unwrap();
    drop(read);
    assert!(!status.success(), "no arguments is a usage error");
    // The flag belongs to the file description the child shared, so the
    // child's change shows here.
    assert_eq!(
        flags() & libc::O_NONBLOCK,
        0,
        "the compiler left its stderr nonblocking"
    );
}
