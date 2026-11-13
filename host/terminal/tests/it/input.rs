//! Keys as a terminal sends them (bytes written to the host in a
//! pseudo-terminal) and as a headless drive names them, and the keys the
//! app hears.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod signals;

use std::path::PathBuf;
use std::process::Command;
#[cfg(unix)]
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    os::unix::process::CommandExt,
    process::Child,
    time::{Duration, Instant},
};

/// An entry file holding `source`.
fn entry(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "exact-terminal-input-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.contract");
    std::fs::write(&path, source).unwrap();
    path
}

/// The app on a pseudo-terminal, its output read into a terminal emulator.
#[cfg(unix)]
struct Pty {
    master: File,
    slave: File,
    output: Vec<u8>,
    child: Child,
    screen: vt100::Parser,
}

#[cfg(unix)]
impl Pty {
    fn spawn(name: &str, source: &str) -> Pty {
        Self::spawn_mode(name, source, "--fullscreen")
    }

    fn spawn_mode(name: &str, source: &str, mode: &str) -> Pty {
        Self::spawn_with(name, source, mode, |_| {})
    }

    /// The app, its command first passed to `configure`.
    fn spawn_with(
        name: &str,
        source: &str,
        mode: &str,
        configure: impl FnOnce(&mut Command),
    ) -> Pty {
        let path = entry(name, source);
        let (mut master_fd, mut slave_fd) = (-1, -1);
        let mut size = libc::winsize {
            ws_row: 6,
            ws_col: 40,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master_fd,
                    &mut slave_fd,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &raw mut size,
                )
            },
            0
        );
        let master = unsafe { File::from_raw_fd(master_fd) };
        let slave = unsafe { File::from_raw_fd(slave_fd) };
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            0
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_exact-terminal"));
        configure(&mut command);
        let child = command
            .arg(path)
            .arg(mode)
            .process_group(0)
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave.try_clone().unwrap())
            .spawn()
            .unwrap();
        Pty {
            master,
            slave,
            output: Vec::new(),
            child,
            screen: vt100::Parser::new(6, 40, 0),
        }
    }

    fn text(&mut self) -> String {
        let mut chunk = [0; 4096];
        while let Ok(n) = self.master.read(&mut chunk) {
            if n == 0 {
                break;
            }
            self.output.extend_from_slice(&chunk[..n]);
            self.screen.process(&chunk[..n]);
        }
        self.screen.screen().contents()
    }

    /// Wait up to ten seconds for the screen to show `what`.
    fn until(&mut self, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.text().contains(what) {
            assert!(
                Instant::now() < deadline,
                "the screen never showed {what:?}:\n{}",
                self.text()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn termios(&self) -> libc::termios {
        let mut settings = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(self.slave.as_raw_fd(), settings.as_mut_ptr()) },
            0
        );
        unsafe { settings.assume_init() }
    }

    fn signal(&self, signal: i32) {
        assert_eq!(unsafe { libc::kill(self.child.id() as i32, signal) }, 0);
    }

    fn stopped(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.text();
            let mut status = 0;
            let pid = unsafe {
                libc::waitpid(
                    self.child.id() as i32,
                    &mut status,
                    libc::WUNTRACED | libc::WNOHANG,
                )
            };
            if pid > 0 {
                assert!(libc::WIFSTOPPED(status), "child exited: {status}");
                return;
            }
            assert_eq!(pid, 0);
            assert!(
                Instant::now() < deadline,
                "child did not suspend: {}",
                self.text()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn exited(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "{status}");
                return;
            }
            self.text();
            assert!(Instant::now() < deadline, "child did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn until_output(&mut self, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.text();
            if String::from_utf8_lossy(&self.output).contains(what) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "no output {what:?}: {:?}",
                self.output
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }
}

#[cfg(unix)]
impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A screen that lists the keys its root hears, under a timer that fires
/// every 2 ms.
#[cfg(unix)]
const HEARD: &str = r#"component App
  state heard = ""
  state ticks = 0
  action keyed(k: string)
    heard = `${heard}${k};`
  action step
    ticks = ticks + 1
  task spin mount
    every(2, step)
  view
    column key=keyed
      text `heard [${heard}]`
"#;

/// A lone ESC waits 25 ms for the rest of a sequence however often the
/// loop wakes: here a timer wakes it every 2 ms. The bound is a lower one,
/// so a slow machine cannot fail it.
#[cfg(unix)]
#[test]
fn a_held_escape_waits_its_time_whatever_wakes_the_loop() {
    let mut pty = Pty::spawn("escape-wait", HEARD);
    pty.until("heard []");
    let sent = Instant::now();
    pty.write(b"\x1b");
    pty.until("heard [Escape;]");
    let waited = sent.elapsed();
    assert!(
        waited >= Duration::from_millis(25),
        "Escape after {waited:?}"
    );
}

/// A URL a timer asks for reaches the system's opener with no input to
/// follow it: here the opener on `PATH` records what it was given.
#[cfg(unix)]
#[test]
fn a_timer_opens_its_url_without_waiting_for_input() {
    use std::os::unix::fs::PermissionsExt;
    let bin = std::env::temp_dir().join(format!(
        "exact-terminal-input-{}-opener",
        std::process::id()
    ));
    std::fs::create_dir_all(&bin).unwrap();
    let opened = bin.join("opened");
    let _ = std::fs::remove_file(&opened);
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\n",
        opened.display()
    );
    for opener in ["open", "xdg-open"] {
        let path = bin.join(opener);
        std::fs::write(&path, &script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let opens = |name: &str, then: &str| {
        let _ = std::fs::remove_file(&opened);
        let source = format!(
            "component App\n  action opened\n    openURL(\"https://example.invalid/{name}\")\n{then}  task later mount\n    after(50, opened)\n  view\n    text \"Waiting\"\n"
        );
        let mut pty = Pty::spawn_with(name, &source, "--fullscreen", |c| {
            c.env("PATH", &path);
        });
        pty.until("Waiting");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let text = std::fs::read_to_string(&opened).unwrap_or_default();
            if text == format!("https://example.invalid/{name}\n") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{name}: the opener was given {text:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    opens("timer", "");
    // The action that asks also closes the app.
    opens("timer-close", "    close()\n");
}

#[cfg(unix)]
const SUSPEND_APP: &str = r#"component App
  state value = ""
  action typed(v: string)
    value = v
  view
    column
      column role="log"
        text "Settled transcript"
      text "Ready"
      input value=value input=typed autofocus=true
"#;

#[cfg(unix)]
fn suspend_and_resume(name: &str, signal: bool) {
    for mode in ["--fullscreen", "--inline"] {
        let mut pty = Pty::spawn_mode(&format!("{name}-{mode}"), SUSPEND_APP, mode);
        pty.until("Ready");
        let cooked_flags = libc::ICANON | libc::ECHO | libc::ISIG;
        assert_eq!(pty.termios().c_lflag & cooked_flags, 0);
        // The shell's `ECHOE` as it was last changed while the host was stopped.
        let mut echoe = None;
        for cycle in 1..=2 {
            pty.output.clear();
            if signal {
                pty.signal(libc::SIGTSTP);
            } else {
                pty.write(if cycle == 1 { b"\x1a" } else { b"\x1b[122;5u" });
            }
            pty.stopped();
            assert_eq!(pty.termios().c_lflag & cooked_flags, cooked_flags);
            if let Some(echoe) = echoe {
                assert_eq!(pty.termios().c_lflag & libc::ECHOE, echoe);
            }
            pty.text();
            let output = String::from_utf8_lossy(&pty.output);
            for leave in ["\x1b[?25h", "\x1b[?2004l", "\x1b[<u", "\x1b[?1000l"] {
                assert!(output.contains(leave), "missing {leave:?}: {output:?}");
            }
            if mode == "--fullscreen" {
                assert!(output.contains("\x1b[?1049l"));
            }
            assert_eq!(
                unsafe { libc::fcntl(pty.slave.as_raw_fd(), libc::F_GETFL) } & libc::O_NONBLOCK,
                0,
            );
            // `fg` restores job settings; the resumed host must enter raw mode
            // again and give the shell's changed cooked settings back when it
            // next leaves. Raw mode itself clears `ECHOE` on macOS (`cfmakeraw`),
            // so the change is read where cooked mode returns.
            let mut settings = pty.termios();
            settings.c_lflag ^= libc::ECHOE;
            echoe = Some(settings.c_lflag & libc::ECHOE);
            assert_eq!(
                unsafe { libc::tcsetattr(pty.slave.as_raw_fd(), libc::TCSANOW, &settings) },
                0
            );
            pty.slave.write_all(b"\r\nshell prompt\r\n").unwrap();
            pty.text();
            pty.output.clear();
            pty.signal(libc::SIGCONT);
            pty.until_output("Ready");
            assert_eq!(pty.termios().c_lflag & cooked_flags, 0);
            let output = String::from_utf8_lossy(&pty.output);
            assert!(output.contains("\x1b[?2004h") && output.contains("\x1b[>1u"));
            if mode == "--fullscreen" {
                assert!(output.contains("\x1b[?1049h") && output.contains("\x1b[?1000h"));
            } else {
                assert!(
                    !output.contains("Settled transcript"),
                    "printed history repeated"
                );
                assert!(pty.text().contains("shell prompt"));
            }
            pty.write(b"x");
            pty.until(&"x".repeat(cycle));
        }
        pty.write(b"\x03");
        pty.exited();
        assert_eq!(pty.termios().c_lflag & cooked_flags, cooked_flags);
        assert_eq!(Some(pty.termios().c_lflag & libc::ECHOE), echoe);
    }
}

#[cfg(unix)]
#[test]
fn control_z_suspends_and_resumes_the_terminal() {
    suspend_and_resume("control-z", false);
}

#[cfg(unix)]
#[test]
fn sigtstp_suspends_and_resumes_the_terminal() {
    suspend_and_resume("sigtstp", true);
}

#[cfg(unix)]
#[test]
fn suspend_waits_for_room_to_restore_the_screen() {
    for mode in ["--fullscreen", "--inline"] {
        let mut pty = Pty::spawn_mode(&format!("suspend-leave-{mode}"), SUSPEND_APP, mode);
        pty.until("Ready");
        pty.until_output("\x1b[?2026l");
        pty.output.clear();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let trace = signals::Trace::attach(&pty);
        pty.signal(libc::SIGTSTP);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        trace.blocked_leave(&pty);
        assert_eq!(
            unsafe { libc::tcflow(pty.slave.as_raw_fd(), libc::TCOON) },
            0
        );
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        trace.detach();
        pty.stopped();
        pty.text();
        let output = String::from_utf8_lossy(&pty.output);
        for leave in ["\x1b[?25h", "\x1b[?2004l", "\x1b[<u", "\x1b[?1000l"] {
            assert!(output.contains(leave), "missing {leave:?}: {output:?}");
        }
        if mode == "--fullscreen" {
            assert!(output.contains("\x1b[?1049l"));
        }
        let cooked_flags = libc::ICANON | libc::ECHO | libc::ISIG;
        assert_eq!(pty.termios().c_lflag & cooked_flags, cooked_flags);
        pty.signal(libc::SIGCONT);
        pty.until_output("Ready");
        pty.write(b"\x03");
        pty.exited();
    }
}

#[cfg(unix)]
#[test]
fn normal_exit_waits_for_room_to_restore_screen() {
    for mode in ["--fullscreen", "--inline"] {
        let mut pty = Pty::spawn_mode(&format!("normal-leave-{mode}"), SUSPEND_APP, mode);
        pty.until("Ready");
        pty.until_output("\x1b[?2026l");
        pty.output.clear();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let trace = signals::Trace::attach(&pty);
        pty.write(b"\x03");
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        trace.blocked_leave(&pty);
        assert_eq!(
            unsafe { libc::tcflow(pty.slave.as_raw_fd(), libc::TCOON) },
            0
        );
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        trace.detach();
        pty.exited();
        pty.text();
        let output = String::from_utf8_lossy(&pty.output);
        assert!(output.contains("\x1b[?25h"));
        if mode == "--fullscreen" {
            assert!(output.contains("\x1b[?1049l"));
        }
        let cooked = libc::ICANON | libc::ECHO | libc::ISIG;
        assert_eq!(pty.termios().c_lflag & cooked, cooked);
    }
}

#[cfg(unix)]
fn partial_transcript(name: &str) -> (Pty, Vec<String>) {
    let entries: Vec<_> = (0..2048).map(|i| format!("entry {i:04} settled")).collect();
    let source = format!(
        r#"component App
  state shown = false
  action show
    shown = true
  view
    column
      column role="log"
        if shown
          for line in {entries:?} key=line
            text line id=line
      text "Ready"
      button press=show aria-keyshortcuts="p"
        text "Print"
"#
    );
    let mut pty = Pty::spawn_mode(name, &source, "--inline");
    pty.until("Ready");
    pty.output.clear();
    pty.write(b"p");
    // Read only the beginning: the frame exceeds the PTY's output queue,
    // so the writer cannot finish until this test drains the rest.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut chunk = [0; 4096];
        if let Ok(n) = pty.master.read(&mut chunk) {
            pty.output.extend_from_slice(&chunk[..n]);
            pty.screen.process(&chunk[..n]);
            if String::from_utf8_lossy(&pty.output).contains(&entries[0]) {
                break;
            }
        }
        assert!(Instant::now() < deadline, "transcript did not start");
        std::thread::sleep(Duration::from_millis(50));
    }
    (pty, entries)
}

#[cfg(unix)]
#[test]
fn suspend_during_output_preserves_the_whole_transcript() {
    let (mut pty, entries) = partial_transcript("suspend-output");
    pty.signal(libc::SIGTSTP);
    pty.stopped();
    pty.text();
    let mut output = std::mem::take(&mut pty.output);
    pty.signal(libc::SIGCONT);
    pty.until_output("\x1b[?2004h");
    pty.until_output("Ready");
    pty.write(b"\x03");
    pty.exited();
    pty.text();
    assert!(
        !String::from_utf8_lossy(&pty.output).contains("entry "),
        "resume repeated printed history"
    );
    output.extend_from_slice(&pty.output);
    let output = String::from_utf8_lossy(&output);
    for entry in entries {
        assert_eq!(
            output.matches(&entry).count(),
            1,
            "lost or repeated {entry}"
        );
    }
}

#[cfg(unix)]
#[test]
fn termination_interrupts_a_suspend_waiting_for_output() {
    use std::os::unix::process::ExitStatusExt;

    let (mut pty, _) = partial_transcript("terminate-suspend-output");
    assert_eq!(
        unsafe { libc::tcflow(pty.slave.as_raw_fd(), libc::TCOOFF) },
        0
    );
    pty.signal(libc::SIGTSTP);
    pty.signal(libc::SIGTERM);
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = pty.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "termination did not interrupt output"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status.signal(), Some(libc::SIGTERM));
    let cooked_flags = libc::ICANON | libc::ECHO | libc::ISIG;
    assert_eq!(pty.termios().c_lflag & cooked_flags, cooked_flags);
}

#[test]
fn control_z_is_a_preventable_default() {
    use exact_terminal::host::{After, Host, Key, Mode};
    for (handler, shortcut, expected) in [
        ("", "", After::Suspend),
        ("preventDefault()", "", After::Continue),
        ("", "aria-keyshortcuts=\"Control+z\"", After::Continue),
    ] {
        let source = format!(
            "component App\n  state n = 0\n  action keyed(key: string)\n    n = n + 1\n    {handler}\n  action hit\n    n = n + 1\n  view\n    column key=keyed\n      text `keys ${{n}}`\n      button press=hit {shortcut}\n        text \"hit\"\n"
        );
        let mut host = Host::boot(
            contract::compile_terminal(&source).unwrap(),
            (),
            Mode::Fullscreen,
            30,
            4,
        )
        .unwrap();
        host.frame();
        host.arm = false;
        assert_eq!(host.key(Key::Ctrl('z')), expected);
        let count = if shortcut.is_empty() { 1 } else { 2 };
        assert!(host.frame().grid.text().contains(&format!("keys {count}")));
    }
}

/// Ctrl-\ leaves whatever the app does with keys (LLP 1101 D6): a focused
/// field's handler or the root's that prevents the default, or a button
/// that names it as its shortcut.
#[test]
fn control_backslash_always_leaves() {
    use exact_terminal::host::{After, Host, Key, Mode};
    use exact_terminal::vt::Vt;
    for (case, view) in [
        (
            "a focused field's handler",
            "    column\n      text `keys ${n}`\n      input value=\"\" key=onKey autofocus=true\n",
        ),
        ("the root's handler", "    column key=onKey\n      text `keys ${n}`\n"),
        (
            "a shortcut",
            "    column\n      text `keys ${n}`\n      button press=hit aria-keyshortcuts=\"Control+\\\\\"\n        text \"hit\"\n",
        ),
    ] {
        let source = format!(
            "component App\n  state n = 0\n  action onKey(key: string)\n    n = n + 1\n    preventDefault()\n  action hit\n    n = n + 1\n  view\n{view}"
        );
        let plan = contract::compile_terminal(&source).expect("compiles");
        let mut host = Host::boot(plan, (), Mode::Fullscreen, 30, 4).expect("boots");
        let mut vt = Vt::new(&mut host);
        vt.render(&mut host);
        host.read_at = host.frames;
        assert_eq!(host.key(Key::Ctrl('\\')), After::Quit, "{case}");
        vt.render(&mut host);
        assert!(
            vt.text(false).contains("keys 0"),
            "{case}: {}",
            vt.text(false)
        );
    }
}

/// A drive's `key` presses the key the terminal's decoder makes of the same
/// chord, and refuses a name that is no key.
#[test]
fn a_driven_key_is_the_chord_it_names() {
    // Each key heard as the modifiers held (Control, Alt, Shift, Meta) and
    // the key.
    let path = entry(
        "driven-keys",
        r#"component App
  state heard = ""
  action keyed(k: string, e: KeyboardEvent)
    heard = `${heard}${e.ctrlKey ? "C" : ""}${e.altKey ? "A" : ""}${e.shiftKey ? "S" : ""}${e.metaKey ? "M" : ""} ${k};`
  view
    column key=keyed
      text `heard [${heard}]`
"#,
    );
    let drive = |keys: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_exact-terminal"))
            .arg(&path)
            .args(["--size", "120x4", "--drive"])
            .args(keys.iter().flat_map(|k| ["key", k]))
            .arg("print")
            .output()
            .expect("runs");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (ok, out, err) = drive(&[
        "Control+ArrowUp",
        "Control+Enter",
        "Shift+Control+Enter",
        "Control+x",
        "Alt+b",
        "Shift+Tab",
        "Meta+s",
        "Space",
        "Shift+n",
        "/",
    ]);
    assert!(ok, "{err}");
    assert!(
        out.contains("heard [C ArrowUp;C Enter;CS Enter;C x;A b;S Tab;M s; Space;S N; /;]"),
        "{out}"
    );
    for name in ["F99", "Control+Nope", "Super+a"] {
        let (ok, _, err) = drive(&[name]);
        assert!(!ok && err.contains(name), "{name}: {err}");
    }
}
