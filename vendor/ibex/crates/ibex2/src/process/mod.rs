//! Child processes for a Rust consumer (exact2 LLP 1133.007 D9): the
//! platform's call, a process group of the child's own (a session for a
//! login shell), only descriptors 0, 1 and 2 inherited, every signal's
//! disposition the default and none blocked; then waiting, signalling the
//! group, and what a crash's cleanup reads at the next launch (a process's
//! start time, the boot, a group's members). Rust's `std::process::Command`
//! does neither of the first two, so this spawns through the platform:
//! `posix_spawn` on Apple platforms, `fork` and `execve` elsewhere, with
//! `PR_SET_PDEATHSIG` on Linux.
//!
//! What a child may run, and what its arguments say, are the caller's: this
//! module admits nothing (the grants are checked above it).

use std::ffi::{CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[cfg(target_vendor = "apple")]
mod darwin;
#[cfg(target_vendor = "apple")]
use darwin as platform;
#[cfg(not(target_vendor = "apple"))]
mod fork;
#[cfg(not(target_vendor = "apple"))]
use fork as platform;
#[cfg(any(not(target_vendor = "apple"), test))]
mod scan;

#[cfg(test)]
mod tests;

/// What one of a child's three descriptors is.
pub enum Stdio {
    /// A pipe whose other end the caller keeps.
    Pipe,
    /// `/dev/null`.
    Null,
    /// An open file (stdin from a file).
    File(std::fs::File),
}

/// A program to start.
pub struct Command<'a> {
    /// The executable, a native path (no `PATH` lookup here: [`find_on_path`]).
    pub program: &'a Path,
    /// `argv[0]`: the program as the caller named it, as Node passes it
    /// (`None`: the native path).
    pub arg0: Option<&'a str>,
    /// Its arguments, after `argv[0]`.
    pub args: &'a [String],
    /// Its whole environment.
    pub env: &'a [(String, String)],
    /// Its working directory.
    pub cwd: &'a Path,
    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
    /// A session of its own, with no controlling terminal (a login shell),
    /// in place of a process group of its own.
    pub session: bool,
}

/// A started child: its pid, which is its process group's id, and the
/// parent's ends of its pipes.
pub struct Child {
    pub pid: i32,
    pub stdin: Option<OwnedFd>,
    pub stdout: Option<OwnedFd>,
    pub stderr: Option<OwnedFd>,
}

/// How a child ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

/// Start `command`. `ENOENT` reads as [`io::ErrorKind::NotFound`].
pub fn spawn(command: Command<'_>) -> io::Result<Child> {
    let c = |s: &OsStr| CString::new(s.as_bytes()).map_err(|_| invalid("a NUL in an argument"));
    let program = c(command.program.as_os_str())?;
    let arg0 = match command.arg0 {
        Some(name) => c(OsStr::new(name))?,
        None => program.clone(),
    };
    let mut argv = vec![arg0];
    for arg in command.args {
        argv.push(c(OsStr::new(arg))?);
    }
    let mut envp = Vec::with_capacity(command.env.len());
    for (k, v) in command.env {
        if k.is_empty() || k.contains('=') {
            return Err(invalid("an environment name that is empty or has `=`"));
        }
        envp.push(c(OsStr::new(&format!("{k}={v}")))?);
    }
    let cwd = c(command.cwd.as_os_str())?;
    // The parent's ends, and what the child gets as 0, 1, 2.
    let mut kept: [Option<OwnedFd>; 3] = [None, None, None];
    let mut given: Vec<Given> = Vec::with_capacity(3);
    for (fd, stdio) in [command.stdin, command.stdout, command.stderr]
        .into_iter()
        .enumerate()
    {
        given.push(match stdio {
            Stdio::Pipe => {
                let (read, write) = pipe()?;
                let (mine, theirs) = if fd == 0 {
                    (write, read)
                } else {
                    (read, write)
                };
                no_sigpipe(&mine);
                kept[fd] = Some(mine);
                Given::Fd(above_stdio(theirs)?)
            }
            Stdio::Null => Given::Null,
            Stdio::File(file) => Given::Fd(above_stdio(file.into())?),
        });
    }
    let pid = platform::spawn(&program, &argv, &envp, &cwd, &given, command.session)?;
    drop(given);
    let [stdin, stdout, stderr] = kept;
    Ok(Child {
        pid,
        stdin,
        stdout,
        stderr,
    })
}

/// `fd` moved above 2, close-on-exec: a parent whose own stdio is closed
/// gets pipe ends at 0, 1 or 2, and putting one end in place as the
/// child's 0, 1 or 2 would then overwrite another before it is placed.
fn above_stdio(fd: OwnedFd) -> io::Result<OwnedFd> {
    if fd.as_raw_fd() > 2 {
        return Ok(fd);
    }
    // SAFETY: plain fcntl on a descriptor the caller owns.
    let moved = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    if moved < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `moved` was just opened and is owned here alone.
    Ok(unsafe { OwnedFd::from_raw_fd(moved) })
}

/// What the child gets as one of its descriptors.
pub(crate) enum Given {
    Fd(OwnedFd),
    Null,
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, why.to_string())
}

/// A pipe whose ends are close-on-exec: another spawn inherits neither. On
/// Linux both are made so at once (`pipe2`), so a fork on another thread
/// between the two calls cannot inherit them.
fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as RawFd; 2];
    #[cfg(target_os = "linux")]
    // SAFETY: `fds` has room for the two descriptors pipe2 writes.
    let r = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    #[cfg(not(target_os = "linux"))]
    // SAFETY: `fds` has room for the two descriptors pipe writes.
    let r = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both were just opened and are owned here alone.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    #[cfg(not(target_os = "linux"))]
    for fd in [&read, &write] {
        // SAFETY: a valid descriptor.
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok((read, write))
}

/// A write to the parent's end of a pipe whose child has gone fails with
/// `EPIPE` rather than raising `SIGPIPE`, which ends a process whose `main`
/// did not ignore it (a Swift app's: Rust's runtime ignores it for Rust's
/// own `main`). Apple's `F_SETNOSIGPIPE` says so per descriptor; elsewhere
/// the host's `main` is Rust's.
/// `<sys/fcntl.h>` on Apple platforms; libc 0.2.190 lacks them.
#[cfg(target_vendor = "apple")]
const F_SETNOSIGPIPE: libc::c_int = 73;
#[cfg(all(test, target_vendor = "apple"))]
const F_GETNOSIGPIPE: libc::c_int = 74;

fn no_sigpipe(fd: &OwnedFd) {
    #[cfg(target_vendor = "apple")]
    // SAFETY: a valid descriptor; the flag affects it alone.
    unsafe {
        libc::fcntl(fd.as_raw_fd(), F_SETNOSIGPIPE, 1);
    }
    #[cfg(not(target_vendor = "apple"))]
    let _ = fd;
}

/// Whether `fd` writes raise no `SIGPIPE` ([`no_sigpipe`]), for a test.
#[cfg(all(test, target_vendor = "apple"))]
fn sigpipe_off(fd: RawFd) -> bool {
    // SAFETY: plain fcntl.
    unsafe { libc::fcntl(fd, F_GETNOSIGPIPE) == 1 }
}

/// Wait for child `pid` to end without reaping it. The retained zombie
/// reserves its pid until [`wait`], so its group cannot acquire a new leader
/// while the caller can still signal it.
pub fn observe_exit(pid: i32) -> io::Result<Exit> {
    observe(pid, 0).map(|exit| exit.expect("a blocking wait observes an exit"))
}

/// Observe an already exited child without reaping it or waiting for a
/// running child. `None` means the child has not exited.
pub fn try_observe_exit(pid: i32) -> io::Result<Option<Exit>> {
    observe(pid, libc::WNOHANG)
}

fn observe(pid: i32, flags: libc::c_int) -> io::Result<Option<Exit>> {
    loop {
        // SAFETY: zeroed siginfo_t is valid; waitid fills it for this child.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let r = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT | flags,
            )
        };
        if r == 0 {
            // SAFETY: waitid leaves si_pid zero when WNOHANG finds no exit.
            if unsafe { info.si_pid() } == 0 {
                return Ok(None);
            }
            // SAFETY: WEXITED filled the child-exit variant of siginfo_t.
            let status = unsafe { info.si_status() };
            return Ok(Some(if info.si_code == libc::CLD_EXITED {
                Exit::Code(status)
            } else {
                Exit::Signal(status)
            }));
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

/// Wait for child `pid` to end, reaping it. After this call its numeric pid
/// and group id may be reused; finish signalling its group first.
pub fn wait(pid: i32) -> io::Result<Exit> {
    let mut status = 0;
    loop {
        // SAFETY: `status` is a valid out pointer.
        let r = unsafe { libc::waitpid(pid, &mut status, 0) };
        if r == pid {
            return Ok(if libc::WIFSIGNALED(status) {
                Exit::Signal(libc::WTERMSIG(status))
            } else {
                Exit::Code(libc::WEXITSTATUS(status))
            });
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

/// `SIGKILL` and `SIGTERM`, for a caller that names no libc.
pub const SIGKILL: i32 = libc::SIGKILL;
pub const SIGTERM: i32 = libc::SIGTERM;

/// One signal to process `pid` alone, if `same(pid)` holds for the process
/// the signal reaches: `Ok(false)` when it does not. On Linux the process is
/// held by a pidfd from before the test, so a pid reused after the test is
/// not signalled; elsewhere nothing can hold a process that is not a child,
/// and the signal follows the test at once.
pub fn signal_process_if(
    pid: i32,
    signal: i32,
    same: impl FnOnce(i32) -> bool,
) -> io::Result<bool> {
    if pid <= 1 {
        return Err(invalid("not a child's pid"));
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: plain syscall; a new descriptor or -1.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if fd >= 0 {
            // SAFETY: pidfd_open returned a descriptor this call now owns.
            let fd = unsafe { OwnedFd::from_raw_fd(fd as RawFd) };
            if !same(pid) {
                return Ok(false);
            }
            // SAFETY: a pidfd, no siginfo, no flags.
            let r = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    signal,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            };
            return if r == 0 {
                Ok(true)
            } else {
                Err(io::Error::last_os_error())
            };
        }
        let e = io::Error::last_os_error();
        // Before Linux 5.3, or under a seccomp filter that refuses it.
        if !matches!(e.raw_os_error(), Some(libc::ENOSYS | libc::EPERM)) {
            return Err(e);
        }
    }
    if !same(pid) {
        return Ok(false);
    }
    // SAFETY: plain syscall.
    if unsafe { libc::kill(pid, signal) } == 0 {
        Ok(true)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The process group `pid` is in.
pub fn group_of(pid: i32) -> Option<i32> {
    getpgid(pid).ok()
}

fn getpgid(pid: i32) -> io::Result<i32> {
    // SAFETY: plain syscall.
    match unsafe { libc::getpgid(pid) } {
        -1 => Err(io::Error::last_os_error()),
        pgid => Ok(pgid),
    }
}

/// Whether `fd` is readable (or at its end) within `timeout_ms`.
pub fn readable(fd: RawFd, timeout_ms: i32) -> bool {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one valid pollfd.
    unsafe { libc::poll(&mut poll, 1, timeout_ms) > 0 }
}

/// One signal to process group `pgid`.
pub fn signal_group(pgid: i32, signal: i32) -> io::Result<()> {
    if pgid <= 1 {
        return Err(invalid("not a child's process group"));
    }
    // SAFETY: plain syscall; a negative pid names the group.
    if unsafe { libc::kill(-pgid, signal) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Whether process group `pgid` has a member with live threads, treating
/// failed inspection as alive. A process with no live threads needs
/// neither signalling nor a grace period.
/// Keep the leader unreaped until the last group signal: a liveness check
/// alone cannot protect the id from reuse between the check and the signal.
pub fn group_alive(pgid: i32) -> bool {
    pgid > 1
        // SAFETY: signal 0 only checks whether the group exists.
        && (unsafe { libc::kill(-pgid, 0) } == 0
            || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
        && platform::group_members(pgid).map_or(true, |members| !members.is_empty())
}

/// The signals a child may be sent, by their names.
const SIGNALS: [(&str, i32); 9] = [
    ("SIGTERM", libc::SIGTERM),
    ("SIGINT", libc::SIGINT),
    ("SIGHUP", libc::SIGHUP),
    ("SIGQUIT", libc::SIGQUIT),
    ("SIGUSR1", libc::SIGUSR1),
    ("SIGUSR2", libc::SIGUSR2),
    ("SIGSTOP", libc::SIGSTOP),
    ("SIGCONT", libc::SIGCONT),
    ("SIGKILL", libc::SIGKILL),
];

/// The number of a signal named here.
pub fn signal_number(name: &str) -> Option<i32> {
    SIGNALS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// A signal's name, for the ones a child is commonly ended by.
pub fn signal_name(signal: i32) -> String {
    let more = [
        ("SIGABRT", libc::SIGABRT),
        ("SIGSEGV", libc::SIGSEGV),
        ("SIGBUS", libc::SIGBUS),
        ("SIGPIPE", libc::SIGPIPE),
        ("SIGALRM", libc::SIGALRM),
        ("SIGILL", libc::SIGILL),
        ("SIGFPE", libc::SIGFPE),
        ("SIGTRAP", libc::SIGTRAP),
    ];
    (SIGNALS.iter().chain(more.iter()))
        .find(|(_, s)| *s == signal)
        .map_or_else(|| format!("SIG{signal}"), |(n, _)| n.to_string())
}

/// When process `pid` started, in microseconds since the epoch on Apple
/// platforms and clock ticks since boot on Linux: comparable within one boot.
pub fn start_time(pid: i32) -> Option<u64> {
    platform::start_time(pid)
}

/// This boot's identity.
pub fn boot_id() -> Option<String> {
    platform::boot_id()
}

/// The live members of process group `pgid`, or an empty list if inspection
/// fails. An empty list alone does not establish that a group has ended;
/// use [`group_alive`] for liveness, which treats inspection failure as alive.
pub fn group_members(pgid: i32) -> Vec<i32> {
    platform::group_members(pgid).unwrap_or_default()
}

/// The session process `pid` belongs to: what a crash's cleanup checks a
/// group's members by, since a group id reused after the group ended
/// belongs to another session (a GUI app's children are in the app's).
pub fn session_of(pid: i32) -> Option<i32> {
    // SAFETY: plain syscall.
    let sid = unsafe { libc::getsid(pid) };
    (pid > 0 && sid > 0).then_some(sid)
}
/// The user's password-database entry: its home directory and login shell.
fn passwd() -> Option<(PathBuf, PathBuf)> {
    let mut buf = vec![0u8; 16 << 10];
    // SAFETY: zeroed is a valid passwd to be filled.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: valid pointers and buffer length.
    let r = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut found,
        )
    };
    if r != 0 || found.is_null() || entry.pw_dir.is_null() {
        return None;
    }
    // SAFETY: getpwuid_r filled NUL-terminated strings in `buf`.
    let text = |p: *const libc::c_char| unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes().to_vec();
    let dir = PathBuf::from(OsStr::from_bytes(&text(entry.pw_dir)));
    let shell = if entry.pw_shell.is_null() {
        PathBuf::new()
    } else {
        PathBuf::from(OsStr::from_bytes(&text(entry.pw_shell)))
    };
    Some((dir, shell))
}

/// Run `wait` when the process exits (`atexit`), once however often it is
/// asked: a host whose quit ends its children off the main thread has
/// `exit` wait for them. Only in the process that asked: a child forked
/// later inherits the `atexit` list, and its `exit` runs none of them, nor
/// waits on a lock another thread held at the fork.
pub fn wait_at_exit(wait: fn()) {
    static WAITS: std::sync::Mutex<Vec<fn()>> = std::sync::Mutex::new(Vec::new());
    static REGISTER: std::sync::Once = std::sync::Once::new();
    static OWNER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    extern "C" fn run() {
        if OWNER.load(std::sync::atomic::Ordering::Relaxed) != std::process::id() {
            return;
        }
        let waits = WAITS.try_lock().map(|w| w.clone()).unwrap_or_default();
        for wait in waits {
            wait();
        }
    }
    OWNER.store(std::process::id(), std::sync::atomic::Ordering::Relaxed);
    let mut waits = WAITS.lock().unwrap_or_else(|e| e.into_inner());
    if !waits.contains(&wait) {
        waits.push(wait);
    }
    drop(waits);
    // SAFETY: `run` is a plain function that lives as long as the process.
    REGISTER.call_once(|| unsafe {
        libc::atexit(run);
    });
}

/// The person's home directory, from the password database, not `$HOME`,
/// which a launcher may set (D6).
pub fn home_dir() -> Option<PathBuf> {
    passwd().map(|(dir, _)| dir)
}

/// The person's login shell: `$SHELL`, else the password database's.
pub fn login_shell() -> Option<PathBuf> {
    std::env::var_os("SHELL")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            passwd()
                .map(|(_, shell)| shell)
                .filter(|s| !s.as_os_str().is_empty())
        })
}

/// Whether this user may execute `path`, a file: `faccessat` with
/// `AT_EACCESS` counts ownership, ACLs and a `noexec` mount, as mode bits
/// cannot.
pub fn executable(path: &Path) -> bool {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    path.metadata().is_ok_and(|m| m.is_file())
        // SAFETY: a NUL-terminated path.
        && unsafe { libc::faccessat(libc::AT_FDCWD, c.as_ptr(), libc::X_OK, crate::AT_EACCESS) } == 0
}

/// The first executable named `name` on `path` (a `PATH` value), as a
/// shell's lookup finds it, but only in absolute directories: a relative
/// one names the app's working directory, not the child's, which changes
/// it before `exec` (as Go's `exec.LookPath` refuses them).
pub fn find_on_path(name: &str, path: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains('/') {
        return None;
    }
    (path.split(':'))
        .filter(|dir| Path::new(dir).is_absolute())
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| executable(candidate))
}

/// Make `fd` non-blocking.
pub fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: plain fcntl on a descriptor the caller owns.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
