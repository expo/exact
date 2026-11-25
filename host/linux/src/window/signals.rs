//! The first termination signal wakes winit. A second wakes the signal
//! thread even after winit has returned and the UI thread is draining.
#![allow(unsafe_code)]

use rustix::thread::futex::{self, Flags};
use std::io;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::thread;

const INACTIVE: u32 = 0;
const STOPPING: u32 = 1;
const RUNNING: u32 = 2;
const QUIT: u32 = 3;
// A forced exit is the shell's 128 plus the signal that interrupted the drain.
static STATUS: AtomicU32 = AtomicU32::new(INACTIVE);

extern "C" fn record(signal: libc::c_int) {
    if STATUS
        .try_update(SeqCst, SeqCst, |status| match status {
            RUNNING => Some(QUIT),
            QUIT => Some(128 + signal as u32),
            _ => None,
        })
        .is_ok()
    {
        // SAFETY: preserve the interrupted thread's errno across the system call.
        unsafe {
            let errno = *libc::__errno_location();
            let _ = futex::wake(&STATUS, Flags::PRIVATE, 1);
            *libc::__errno_location() = errno;
        }
    }
}

fn listen(quit: impl FnOnce()) {
    let mut quit = Some(quit);
    loop {
        let status = STATUS.load(SeqCst);
        match status {
            STOPPING | INACTIVE => return,
            RUNNING => {}
            QUIT => {
                if let Some(quit) = quit.take() {
                    quit();
                }
            }
            // SAFETY: a repeated termination request cuts short even a blocked drain.
            code => unsafe { libc::_exit(code as i32) },
        }
        // The futex checks the word and sleeps atomically. A signal between
        // the load and the wait changes the word, so this wait returns at once.
        let _ = futex::wait(&STATUS, Flags::PRIVATE, status, None);
    }
}

pub(super) struct Signals {
    previous: Vec<(libc::c_int, libc::sigaction)>,
    listener: Option<thread::JoinHandle<()>>,
}

impl Signals {
    pub(super) fn install(quit: impl FnOnce() + Send + 'static) -> io::Result<Self> {
        STATUS
            .compare_exchange(INACTIVE, RUNNING, SeqCst, SeqCst)
            .map_err(|_| io::Error::other("a Wayland signal listener is already running"))?;
        let mut signals = Self {
            previous: Vec::new(),
            listener: None,
        };
        signals.listener = Some(
            thread::Builder::new()
                .name("exact signals".into())
                .spawn(move || listen(quit))?,
        );
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            // SAFETY: sigaction fills old. The new handler records the signal on STATUS.
            unsafe {
                let mut old = std::mem::zeroed();
                if libc::sigaction(signal, std::ptr::null(), &mut old) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if old.sa_sigaction == libc::SIG_IGN {
                    continue; // Preserve nohup and signals ignored by the parent shell.
                }
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = record as *const () as libc::sighandler_t;
                action.sa_flags = libc::SA_RESTART;
                libc::sigemptyset(&mut action.sa_mask);
                if libc::sigaction(signal, &action, &mut old) != 0 {
                    return Err(io::Error::last_os_error());
                }
                signals.previous.push((signal, old));
            }
        }
        Ok(signals)
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        for (signal, old) in &self.previous {
            // SAFETY: old is the disposition replaced for this signal.
            unsafe { libc::sigaction(*signal, old, std::ptr::null_mut()) };
        }
        // A forced exit already recorded during the drain still belongs to the listener.
        let _ = STATUS.try_update(SeqCst, SeqCst, |status| {
            matches!(status, RUNNING | QUIT).then_some(STOPPING)
        });
        let _ = futex::wake(&STATUS, Flags::PRIVATE, 1);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
        STATUS.store(INACTIVE, SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Command, sync::mpsc, time::Duration};

    #[test]
    fn returning_from_the_window_restores_its_signal_dispositions() {
        const CHILD: &str = "EXACT_WAYLAND_RESTORE_SIGNAL_TEST";
        if std::env::var_os(CHILD).is_some() {
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_DFL);
                libc::alarm(5);
            }
            let (wake, woke) = mpsc::channel();
            let signals = Signals::install(move || wake.send(()).unwrap()).unwrap();
            assert_eq!(unsafe { libc::raise(libc::SIGTERM) }, 0);
            woke.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(signals);
            let restored = unsafe { libc::signal(libc::SIGTERM, libc::SIG_DFL) };
            assert_eq!(restored, libc::SIG_DFL);
            return;
        }
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", thread::current().name().unwrap()])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success(), "{status}");
    }

    #[test]
    fn termination_wakes_the_loop_and_a_second_signal_interrupts_the_drain() {
        const CASE: &str = "EXACT_WAYLAND_SIGNAL_TEST";
        let signals = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];
        if let Ok(pair) = std::env::var(CASE) {
            let (first, second) = pair.split_once(',').unwrap();
            let first: i32 = first.parse().unwrap();
            let second: i32 = second.parse().unwrap();
            // Bound the child even if a failed assertion hangs while dropping the listener.
            unsafe { libc::alarm(10) };
            // The test runner can itself have inherited ignored signals.
            for signal in signals {
                unsafe { libc::signal(signal, libc::SIG_DFL) };
            }
            let ignored = signals
                .into_iter()
                .find(|&s| s != first && s != second)
                .unwrap();
            unsafe { libc::signal(ignored, libc::SIG_IGN) };
            let (wake, woke) = mpsc::channel();
            let _signals = Signals::install(move || wake.send(()).unwrap()).unwrap();
            assert_eq!(unsafe { libc::raise(ignored) }, 0);
            assert_eq!(unsafe { libc::raise(first) }, 0);
            woke.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(unsafe { libc::raise(second) }, 0);
            thread::sleep(Duration::from_secs(5));
            panic!("the second signal left the process running");
        }
        for (first, second) in [
            (signals[0], signals[0]),
            (signals[1], signals[2]),
            (signals[2], signals[1]),
        ] {
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", thread::current().name().unwrap()])
                .env(CASE, format!("{first},{second}"))
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(128 + second));
        }
    }
}
