//! How a command-line run ends (LLP 1101.003 D2, D8): one termination
//! record, shared by the print loop, the signal handler and the two
//! watchdogs, holds every cause and when it was recorded, and every exit,
//! cooperative or forced, takes its status from it by one precedence.
//!
//! The loop records a cause between passes and shuts down cooperatively.
//! A watchdog records the cause it enforces when it fires, then forces the
//! exit with `_exit`, which runs no flush, handler or destructor: the
//! timeout's at the deadline plus 2 s, a signal's at its receipt plus 2 s,
//! so a write blocked on a full pipe or a stuck source call cannot keep
//! the process alive. A second SIGINT or SIGTERM forces the exit at once.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The grace a watchdog gives the loop to shut down cooperatively.
const GRACE: Duration = Duration::from_secs(2);

/// Why the run ended: `exit.reason` in `--json`'s object (D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    Settled,
    Closed,
    Timeout,
    Signal,
    Epipe,
    Usage,
    Error,
}

impl Reason {
    pub fn word(self) -> &'static str {
        match self {
            Reason::Settled => "settled",
            Reason::Closed => "closed",
            Reason::Timeout => "timeout",
            Reason::Signal => "signal",
            Reason::Epipe => "epipe",
            Reason::Usage => "usage",
            Reason::Error => "error",
        }
    }
}

/// The outcome a run captured: its status, the cause that decided it, the
/// active head's `status` and a message for usage, error and signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub status: u8,
    pub reason: Reason,
    pub head: Option<u16>,
    pub message: Option<String>,
}

impl Outcome {
    /// `{"status":…,"reason":…,"head":…[,"message":…]}`.
    pub fn json(&self) -> String {
        let head = self.head.map_or("null".into(), |h| h.to_string());
        let mut out = format!(
            "{{\"status\":{},\"reason\":\"{}\",\"head\":{head}",
            self.status,
            self.reason.word()
        );
        if let Some(message) = &self.message {
            out.push_str(",\"message\":");
            exact_runner::agent::quote(message, &mut out);
        }
        out.push('}');
        out
    }
}

/// The causes no app answer can outrank, recorded where any thread (and
/// the signal handler) can read them.
struct Record {
    start: OnceLock<Instant>,
    /// The first SIGINT or SIGTERM, and when it arrived, in nanoseconds
    /// after `start`.
    signal: AtomicI32,
    signal_at: AtomicU64,
    timeout: AtomicBool,
    /// Before boot a forced exit writes the `exit` object or a line.
    before_boot: AtomicBool,
    json: AtomicBool,
}

static RECORD: Record = Record {
    start: OnceLock::new(),
    signal: AtomicI32::new(0),
    signal_at: AtomicU64::new(0),
    timeout: AtomicBool::new(false),
    before_boot: AtomicBool::new(true),
    json: AtomicBool::new(false),
};

/// The loop's own causes, and those the record holds, by D2's precedence.
#[derive(Debug, Default)]
pub(crate) struct Causes {
    pub epipe: bool,
    pub error: Option<String>,
    pub closed: bool,
}

/// Start the record for a print-mode run that began at `start`: the signal
/// handler and both watchdogs. `timeout` is `--timeout`'s seconds, zero for
/// none.
pub(crate) fn arm(start: Instant, timeout: f64, json: bool) {
    let _ = RECORD.start.set(start);
    RECORD.json.store(json, Ordering::SeqCst);
    signals::install();
    std::thread::spawn(signals::watch);
    if let Some(end) = (timeout > 0.0)
        .then(|| forced_end(start, timeout))
        .flatten()
    {
        std::thread::spawn(move || {
            sleep_until(end);
            RECORD.timeout.store(true, Ordering::SeqCst);
            force_exit(outcome(&Causes::default(), None).status);
        });
    }
}

/// When the timeout's watchdog forces the exit: `timeout` seconds after
/// `start`, plus the grace. `None` when no `Instant` is that late.
pub(crate) fn forced_end(start: Instant, timeout: f64) -> Option<Instant> {
    start
        .checked_add(Duration::try_from_secs_f64(timeout).ok()?)?
        .checked_add(GRACE)
}

/// Boot is done: a forced exit no longer writes the pre-boot `exit` object.
pub(crate) fn booted() {
    RECORD.before_boot.store(false, Ordering::SeqCst);
}

/// The loop's check of the deadline: past it, the timeout is recorded.
pub(crate) fn check_deadline(timeout: f64) {
    if let Some(start) = RECORD.start.get() {
        if timeout > 0.0 && start.elapsed().as_secs_f64() >= timeout {
            RECORD.timeout.store(true, Ordering::SeqCst);
        }
    }
}

/// Whether a signal or the timeout is recorded: the loop shuts down.
pub(crate) fn cut_short() -> bool {
    mark() != Mark::default()
}

/// The external causes recorded so far: a write stops when they change
/// under it, not for those its outcome already counted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mark {
    signal: i32,
    timeout: bool,
}

pub(crate) fn mark() -> Mark {
    Mark {
        signal: RECORD.signal.load(Ordering::SeqCst),
        timeout: RECORD.timeout.load(Ordering::SeqCst),
    }
}

/// The status and reason D2's precedence gives the causes recorded so far,
/// with the active head's `status` (`head`) when the app answered.
pub(crate) fn outcome(causes: &Causes, head: Option<u16>) -> Outcome {
    let signal = RECORD.signal.load(Ordering::SeqCst);
    let (status, reason, message) = if signal != 0 {
        let name = if signal == SIGINT {
            "SIGINT"
        } else {
            "SIGTERM"
        };
        (128 + signal as u8, Reason::Signal, Some(name.to_string()))
    } else if RECORD.timeout.load(Ordering::SeqCst) {
        (124, Reason::Timeout, None)
    } else if causes.epipe {
        (141, Reason::Epipe, None)
    } else if let Some(error) = &causes.error {
        (1, Reason::Error, Some(error.clone()))
    } else {
        let reason = match causes.closed {
            true => Reason::Closed,
            false => Reason::Settled,
        };
        (u8::from(head.is_some()), reason, None)
    };
    Outcome {
        status,
        reason,
        head,
        message,
    }
}

fn sleep_until(at: Instant) {
    let now = Instant::now();
    if at > now {
        std::thread::sleep(at - now);
    }
}

#[cfg(unix)]
const SIGINT: i32 = libc::SIGINT;
#[cfg(not(unix))]
const SIGINT: i32 = 2;

/// Exit now, with no flush, handler or destructor (LLP 1101.003 D2): not
/// `std::process::exit`, which flushes stdout and could block on it. Before
/// boot it first writes what a pre-boot exit says.
pub(crate) fn force_exit(status: u8) -> ! {
    if RECORD.before_boot.load(Ordering::SeqCst) {
        let outcome = outcome(&Causes::default(), None);
        let (fd, text) = match RECORD.json.load(Ordering::SeqCst) {
            true => (1, format!("{{\"exit\":{}}}\n", outcome.json())),
            false => (2, format!("{}: {}\n", app(), outcome.reason.word())),
        };
        raw_write(fd, text.as_bytes());
    }
    exit_now(status)
}

/// The program's name, for the lines it writes to stderr.
pub(crate) fn app() -> String {
    std::env::args()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "exact-terminal".into())
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn raw_write(fd: i32, bytes: &[u8]) {
    // SAFETY: a write of a live buffer to a descriptor; a failure is ignored.
    unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
}

#[cfg(not(unix))]
fn raw_write(fd: i32, bytes: &[u8]) {
    use std::io::Write;
    let _ = match fd {
        1 => std::io::stdout().write_all(bytes),
        _ => std::io::stderr().write_all(bytes),
    };
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn exit_now(status: u8) -> ! {
    // SAFETY: `_exit` ends the process; nothing after it runs.
    unsafe { libc::_exit(i32::from(status)) }
}

#[cfg(not(unix))]
fn exit_now(status: u8) -> ! {
    std::process::exit(i32::from(status))
}

#[cfg(unix)]
mod signals {
    use super::{exit_now, sleep_until, RECORD};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    #[allow(unsafe_code)]
    extern "C" fn received(signal: libc::c_int) {
        let start = RECORD.start.get().copied();
        match RECORD
            .signal
            .compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => {
                let at = start.map_or(0, |s| s.elapsed().as_nanos() as u64);
                RECORD.signal_at.store(at.max(1), Ordering::SeqCst);
            }
            // A second one: at once, with the first one's status.
            Err(first) => exit_now(128 + first as u8),
        }
    }

    /// SIGINT and SIGTERM into the record.
    #[allow(unsafe_code)]
    pub(super) fn install() {
        for signal in [libc::SIGINT, libc::SIGTERM] {
            // SAFETY: the handler only touches atomics and calls `_exit`,
            // both async-signal-safe.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = received as *const () as libc::sighandler_t;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(signal, &action, std::ptr::null_mut());
            }
        }
    }

    /// The signal watchdog: the exit is forced at the first signal's
    /// receipt plus the grace, whatever `--timeout` says.
    pub(super) fn watch() {
        loop {
            let at = RECORD.signal_at.load(Ordering::SeqCst);
            if at != 0 {
                let start = RECORD.start.get().copied().unwrap_or_else(Instant::now);
                sleep_until(start + Duration::from_nanos(at) + super::GRACE);
                let outcome = super::outcome(&super::Causes::default(), None);
                super::force_exit(outcome.status);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(not(unix))]
mod signals {
    pub(super) fn install() {}
    pub(super) fn watch() {}
}
