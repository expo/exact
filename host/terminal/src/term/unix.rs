//! Unix raw mode, terminal signals and the interactive event loop.

mod output;

use super::{decode, flush_escape, record, Decoder, Inline, Input};
use crate::host::{After, Host, Mode};
use crate::image::Protocol;
use exact_runner::DataSource;
use output::write as write_output;
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use rustix::termios::{tcgetattr, tcgetwinsize, tcsetattr, OptionalActions, Termios};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use vte::Parser;

/// The terminal's settings before raw mode, for every way out.
static SAVED: Mutex<Option<(Termios, Mode)>> = Mutex::new(None);
static SIGNAL: AtomicI32 = AtomicI32::new(0);
static SIGNAL_FD: AtomicI32 = AtomicI32::new(-1);
static OUTPUT_FLAGS: AtomicI32 = AtomicI32::new(-1);

struct Signals {
    reader: UnixStream,
    _writer: UnixStream,
    previous: Vec<(i32, libc::sigaction)>,
}

#[allow(unsafe_code)]
extern "C" fn received(signal: libc::c_int) {
    if SIGNAL
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |pending| {
            // Termination takes precedence over a pending suspend.
            (pending == 0 || pending == libc::SIGTSTP).then_some(signal)
        })
        .is_ok()
    {
        let fd = SIGNAL_FD.load(Ordering::SeqCst);
        if fd >= 0 {
            let byte = signal as u8;
            unsafe { libc::write(fd, &byte as *const u8 as *const libc::c_void, 1) };
        }
    }
}

impl Signals {
    #[allow(unsafe_code)]
    fn install() -> std::io::Result<Self> {
        let (reader, writer) = UnixStream::pair()?;
        reader.set_nonblocking(true)?;
        writer.set_nonblocking(true)?;
        let mut guard = Self {
            reader,
            _writer: writer,
            previous: Vec::new(),
        };
        SIGNAL.store(0, Ordering::SeqCst);
        SIGNAL_FD.store(guard._writer.as_raw_fd(), Ordering::SeqCst);
        for signal in [
            libc::SIGINT,
            libc::SIGTERM,
            libc::SIGHUP,
            libc::SIGQUIT,
            libc::SIGTSTP,
        ] {
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = received as *const () as libc::sighandler_t;
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
            if unsafe { libc::sigaction(signal, &action, &mut previous) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            guard.previous.push((signal, previous));
        }
        Ok(guard)
    }

    #[allow(unsafe_code)]
    fn termination_handler(&self, handler: libc::sighandler_t) -> std::io::Result<()> {
        for &(signal, _) in &self.previous {
            if signal == libc::SIGTSTP {
                continue;
            }
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = handler;
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    fn stop(&self) -> std::io::Result<()> {
        // The terminal is restored. A termination arriving after the pending
        // check must take effect immediately, rather than wait behind SIGSTOP.
        self.termination_handler(libc::SIG_DFL)?;
        let signal = SIGNAL.swap(0, Ordering::SeqCst);
        if signal != 0 && signal != libc::SIGTSTP {
            terminate_from_signal(signal);
        }
        // SIGSTOP also stops an orphaned process group. Execution resumes here
        // on SIGCONT; the next pass captures the shell's current termios.
        signal_hook::low_level::raise(libc::SIGSTOP)?;
        self.termination_handler(received as *const () as libc::sighandler_t)
    }
}

#[allow(unsafe_code)]
impl Drop for Signals {
    fn drop(&mut self) {
        SIGNAL_FD.store(-1, Ordering::SeqCst);
        for (signal, previous) in self.previous.drain(..).rev() {
            unsafe { libc::sigaction(signal, &previous, std::ptr::null_mut()) };
        }
    }
}

struct OutputFlags(i32);

impl OutputFlags {
    #[allow(unsafe_code)]
    fn nonblocking() -> std::io::Result<Self> {
        let fd = std::io::stdout().as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        OUTPUT_FLAGS.store(flags, Ordering::SeqCst);
        Ok(Self(flags))
    }
}

#[allow(unsafe_code)]
impl Drop for OutputFlags {
    fn drop(&mut self) {
        unsafe { libc::fcntl(std::io::stdout().as_raw_fd(), libc::F_SETFL, self.0) };
        OUTPUT_FLAGS.store(-1, Ordering::SeqCst);
    }
}

fn drain_signals(mut signal_rx: &UnixStream) {
    let mut sink = [0; 64];
    while matches!(signal_rx.read(&mut sink), Ok(n) if n > 0) {}
}

#[allow(unsafe_code)]
fn terminate_from_signal(signal: i32) -> ! {
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
        libc::_exit(128 + signal);
    }
}

/// A resize wakes the same socket as data arrivals. Its signal action owns
/// a separate descriptor and does only an async-signal-safe, nonblocking write.
struct ResizeWake(signal_hook::SigId);

impl ResizeWake {
    fn new(wake: UnixStream) -> std::io::Result<Self> {
        signal_hook::low_level::pipe::register(signal_hook::consts::SIGWINCH, wake).map(Self)
    }
}

impl Drop for ResizeWake {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.0);
    }
}

/// Alternate screen, SGR mouse with drags, bracketed paste, kitty keyboard
/// disambiguation (LLP 1101 D6).
const ENTER_FULL: &str = "\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[>1u";
/// The normal screen: no mouse capture, so selection stays the terminal's.
const ENTER_INLINE: &str = "\x1b[?25l\x1b[?2004h\x1b[>1u";
/// Each mode undone, in reverse.
const LEAVE_FULL: &str =
    "\x1b]8;;\x1b\\\x1b[?2026l\x1b[<u\x1b[?2004l\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[0m\x1b[0 q\x1b[?25h\x1b[?1049l";
const LEAVE_INLINE: &str =
    "\x1b]8;;\x1b\\\x1b[?2026l\x1b[<u\x1b[?2004l\x1b[?1006l\x1b[?1000l\x1b[0m\x1b[0 q\x1b[?25h";

#[allow(unsafe_code)]
fn restore_output_flags() {
    let flags = OUTPUT_FLAGS.swap(-1, Ordering::SeqCst);
    if flags >= 0 {
        unsafe { libc::fcntl(std::io::stdout().as_raw_fd(), libc::F_SETFL, flags) };
    }
}

fn restore(signal_rx: Option<&UnixStream>) -> std::io::Result<()> {
    let saved = SAVED.lock().ok().and_then(|mut s| s.take());
    let result = if let Some((termios, mode)) = saved {
        let leave = if mode == Mode::Fullscreen {
            LEAVE_FULL
        } else {
            LEAVE_INLINE
        };
        output::restore(leave.as_bytes(), signal_rx, || {
            tcsetattr(rustix::stdio::stdin(), OptionalActions::Now, &termios).map_err(Into::into)
        })
    } else {
        Ok(())
    };
    restore_output_flags();
    result
}

#[cfg(test)]
mod restoration_tests {
    use super::*;

    #[test]
    fn leave_sequences_close_frame_modes() {
        for leave in [LEAVE_INLINE, LEAVE_FULL] {
            assert!(leave.contains("\x1b]8;;\x1b\\"));
            assert!(leave.contains("\x1b[?2026l"));
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn restore_resets_stdout_flags_for_abort_hook() {
        if std::env::var_os("EXACT_TERMINAL_FLAG_CHILD").is_some() {
            let fd = std::io::stdout().as_raw_fd();
            let before = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert!(before >= 0);
            let flags = OutputFlags::nonblocking().unwrap();
            assert_ne!(
                unsafe { libc::fcntl(fd, libc::F_GETFL) } & libc::O_NONBLOCK,
                0
            );
            restore(None).unwrap();
            assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFL) }, before);
            std::mem::forget(flags);
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "term::unix::restoration_tests::restore_resets_stdout_flags_for_abort_hook",
                "--nocapture",
            ])
            .env("EXACT_TERMINAL_FLAG_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success(), "{status}");
    }
}

/// The terminal's size in cells, if it reports one.
pub fn size() -> Option<(usize, usize)> {
    let w = tcgetwinsize(rustix::stdio::stdout()).ok()?;
    (w.ws_col > 0 && w.ws_row > 0).then_some((w.ws_col as usize, w.ws_row as usize))
}

/// Hand the URLs the app asked to open (`openURL`, a clicked link) to the
/// system's opener, detached: the terminal stays the app's.
fn open_outbound<D: DataSource>(host: &mut Host<D>) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    for url in host.outbound.drain(..) {
        let _ = std::process::Command::new(opener)
            .arg(&url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Run the host in this terminal until the app or the user quits.
pub fn run<D: DataSource>(host: &mut Host<D>) -> std::io::Result<()> {
    host.set_colors(host.colors.unwrap_or_else(crate::color::ColorDepth::detect));
    let signals = Signals::install()?;
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore(None);
        hook(info);
    }));
    let result = run_raw(host, &signals);
    drop(signals);
    let signal = SIGNAL.swap(0, Ordering::SeqCst);
    if signal != 0 {
        terminate_from_signal(signal);
    }
    result
}

fn run_raw<D: DataSource>(host: &mut Host<D>, signals: &Signals) -> std::io::Result<()> {
    let signal_rx = &signals.reader;
    let mode = host.mode;
    // The data module's threads wake the loop through a socket.
    let (mut wake_rx, wake_tx) = UnixStream::pair()?;
    wake_rx.set_nonblocking(true)?;
    wake_tx.set_nonblocking(true)?;
    let _resize_wake = ResizeWake::new(wake_tx.try_clone()?)?;
    let wake_tx = Arc::new(Mutex::new(wake_tx));
    host.listen(Arc::new(move || {
        if let Ok(mut w) = wake_tx.lock() {
            let _ = w.write(&[1]);
        }
    }));
    host.announced();
    let start = Instant::now();
    host.live_from(start);
    let now = || start.elapsed().as_secs_f64() * 1000.0;
    let mut parser = Parser::new();
    let mut decoder = Decoder::default();
    let mut shown: Option<crate::grid::Grid> = None;
    let mut inline = Inline::new(Protocol::detect());
    let mut buf = [0u8; 8192];
    host.publish(&record(mode, inline.protocol, host.colors(), 0, ""));
    loop {
        let stdin = rustix::stdio::stdin();
        let saved = tcgetattr(stdin)?;
        let output_flags = OutputFlags::nonblocking()?;
        let mut raw = saved.clone();
        raw.make_raw();
        *SAVED.lock().expect("saved") = Some((saved, mode));
        let mut result = (|| {
            tcsetattr(stdin, OptionalActions::Now, &raw)?;
            write_output(
                if mode == Mode::Fullscreen {
                    ENTER_FULL
                } else {
                    ENTER_INLINE
                }
                .as_bytes(),
                signal_rx,
            )?;
            loop {
                if SIGNAL.load(Ordering::SeqCst) != 0 {
                    return Ok(());
                }
                if let Some((cols, rows)) = size() {
                    host.resize(cols, rows);
                }
                host.tick(now());
                if SIGNAL.load(Ordering::SeqCst) != 0 {
                    return Ok(());
                }
                let mut out = String::new();
                match mode {
                    Mode::Fullscreen => {
                        let grid = &host.frame().grid;
                        if shown.as_ref() != Some(grid) {
                            out = grid.diff(shown.as_ref());
                            shown = Some(grid.clone());
                        }
                    }
                    Mode::Inline => inline.frame(host, &mut out),
                }
                if host.quitting() {
                    if mode == Mode::Inline {
                        inline.finish(&mut out);
                    }
                    write_output(out.as_bytes(), signal_rx)?;
                    return Ok(());
                }
                if !out.is_empty() {
                    write_output(out.as_bytes(), signal_rx)?;
                    host.frames += 1;
                }
                // Tell the app what is in scrollback now that the bytes are out,
                // so it can let go (LLP 1101.001 P1; 1101.002 §0 P5).
                if mode == Mode::Inline {
                    if let Some(r) = inline.printed(mode, host.colors()) {
                        host.publish(&r);
                    }
                }
                // Admit idle work only after checking for input already queued
                let idle_stdin = rustix::stdio::stdin();
                let mut input = [PollFd::new(&idle_stdin, PollFlags::IN)];
                let zero = Timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                };
                if poll(&mut input, Some(&zero)).is_ok_and(|ready| ready == 0) {
                    host.idle_timer(now());
                }
                // Before sleeping: a timer, an announcement or idle work may
                // have asked for a URL, and no input may come to follow it.
                open_outbound(host);
                // Sleep until input, a wake, the next timer, a held ESC's time, or
                // a size check.
                let wait = host
                    .timer_due_ms()
                    .map(|due| (due - now()).clamp(0.0, 250.0))
                    .unwrap_or(250.0);
                let wait = match decoder.escape_due() {
                    Some(due) => wait.min((due - now()).max(0.0)),
                    None => wait,
                };
                let timeout = Timespec {
                    tv_sec: 0,
                    tv_nsec: (wait * 1_000_000.0) as i64,
                };
                let stdin = rustix::stdio::stdin();
                let mut fds = [
                    PollFd::new(&stdin, PollFlags::IN),
                    PollFd::new(&wake_rx, PollFlags::IN),
                    PollFd::new(signal_rx, PollFlags::IN),
                ];
                let interrupted = match poll(&mut fds, Some(&timeout)) {
                    Ok(_) => false,
                    Err(rustix::io::Errno::INTR) => true,
                    Err(e) => return Err(e.into()),
                };
                if !fds[2].revents().is_empty() {
                    drain_signals(signal_rx);
                }
                if SIGNAL.load(Ordering::SeqCst) != 0 {
                    return Ok(());
                }
                let input_ready = !interrupted && !fds[0].revents().is_empty();
                let woken = !interrupted && !fds[1].revents().is_empty();
                // However the poll ended (a timer, a wake, a signal), a held ESC
                // with nothing after it is the Escape key once its time is up. With
                // input waiting, the input is read first: it may be the rest of
                // the sequence.
                if !input_ready {
                    for input in flush_escape(&mut decoder, now()) {
                        if let Input::Key(k) = input {
                            if host.key(k) == After::Quit {
                                return Ok(());
                            }
                        }
                    }
                }
                if woken {
                    let mut sink = [0u8; 256];
                    while matches!(wake_rx.read(&mut sink), Ok(n) if n > 0) {}
                    host.announced();
                }
                if !input_ready {
                    continue;
                }
                let n = match rustix::io::read(stdin, &mut buf) {
                    Ok(n) => n,
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(error) => return Err(error.into()),
                };
                if n == 0 {
                    return Ok(());
                }
                // These keys were typed against what the terminal showed by now.
                host.read_at = host.frames;
                for input in decode(&mut parser, &mut decoder, &buf[..n], now()) {
                    match input {
                        Input::Key(k) => {
                            let after = host.key(k);
                            if after == After::Suspend {
                                received(libc::SIGTSTP);
                                return Ok(());
                            }
                            if after == After::Quit {
                                let mut out = String::new();
                                if mode == Mode::Inline {
                                    inline.frame(host, &mut out);
                                    inline.finish(&mut out);
                                }
                                write_output(out.as_bytes(), signal_rx)?;
                                return Ok(());
                            }
                        }
                        Input::Click(x, y) => host.click(x, y),
                        Input::CursorAt(row, _) => inline.answered(host, row),
                        Input::Wheel(x, y, rows) => host.wheel(x, y, rows),
                        Input::Paste(text) => host.paste(&text),
                    }
                }
            }
        })();
        // What the turn that quit or suspended asked to open.
        open_outbound(host);
        let suspend = SIGNAL
            .compare_exchange(libc::SIGTSTP, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
        if suspend && mode == Mode::Inline {
            let mut out = String::new();
            inline.finish(&mut out);
            result = result.and_then(|()| write_output(out.as_bytes(), signal_rx));
        }
        let restored = restore(Some(signal_rx));
        drop(output_flags);
        if !suspend {
            return result.and(restored);
        }
        result.and(restored)?;
        drain_signals(signal_rx);
        signals.stop()?;
        shown = None;
        inline.resume();
        parser = Parser::new();
        decoder = Decoder::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_registration_wakes_and_releases_only_its_socket_writer() {
        let (mut rx, mut tx) = UnixStream::pair().unwrap();
        rx.set_nonblocking(true).unwrap();
        let registration = ResizeWake::new(tx.try_clone().unwrap()).unwrap();
        let mut bytes = [0; 8];
        signal_hook::low_level::raise(signal_hook::consts::SIGWINCH).unwrap();
        assert_eq!(rx.read(&mut bytes).unwrap(), 1);
        drop(registration);
        signal_hook::low_level::raise(signal_hook::consts::SIGWINCH).unwrap();
        assert_eq!(
            rx.read(&mut bytes).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        // Unregistering a resize must not close the data module's writer.
        tx.write_all(b"data").unwrap();
        assert_eq!(rx.read(&mut bytes).unwrap(), 4);
        assert_eq!(&bytes[..4], b"data");
    }
}
