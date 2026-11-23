//! Attached agent sessions on Linux (LLP 1012.000 D2): a driver joins the
//! development app a person has open and reads it, on the app's own wall
//! clock, once the person allows it. The listener sits beside the display
//! loop's other descriptors and is serviced on the loop's thread between its
//! turns ([`Attached`]), so a read sees what the presenter holds and moves
//! nothing. The wire is the Apple hosts' (`AgentAttach.swift`): JSON lines over
//! a `0600` Unix socket in a checked `0700` `$XDG_RUNTIME_DIR/exact/`, named by
//! a marker beside it; a handshake with the marker's nonce; then
//! `{"request":n,"op":…}` requests built from checked fields. Input stays
//! unsupported until the display's own input is qualified (LLP 1012.000 piece 8).
//!
//! The person's part, allowing a connection and seeing it attached, is a
//! [`Person`]. No Linux display has one yet (piece 8), so no build turns the
//! `attach` feature on; the display-loop harness supplies one.
use crate::event_loop::Events;
use exact_painted::presenter::Presenter;
use exact_runner::DataSource;
use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::ops::Not;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

const VERSION: u64 = 1;
const REQUEST_LIMIT: usize = 1 << 20;
const REPLY_LIMIT: usize = 32 << 20;
const QUEUE_LIMIT: usize = 64 << 20;
const HANDSHAKE: Duration = Duration::from_secs(10);
const APPROVAL: Duration = Duration::from_secs(60);
const WRITE: Duration = Duration::from_secs(10);
/// After a refusal the driver has this long to read it before the socket closes.
const LINGER: Duration = Duration::from_secs(2);

/// The person at the app: asked about one connection at a time, by its
/// number, so an answer never reaches another; and the window saying an agent
/// is attached while one is.
pub(crate) trait Person {
    /// Connection `id` asks: show the person its prompt.
    fn ask(&mut self, id: u64);
    /// The answer for connection `id`: `Some(true)` allowed, `Some(false)` refused, `None` not yet.
    fn answer(&mut self, id: u64) -> Option<bool>;
    /// Connection `id` ended: its prompt goes, unanswered.
    fn ended(&mut self, id: u64);
    /// The window shows, or stops showing, that an agent is attached.
    fn attached(&mut self, on: bool);
}

/// Whether this binary may listen: development trust baked in, said
/// affirmatively, and a launch the driver does not own. Missing or unknown
/// trust leaves it off (`delivery::production` is false for those too).
pub(crate) fn admitted(compat: &str, agent_carried: bool) -> Result<(), String> {
    let json: serde_json::Value = serde_json::from_str(compat).unwrap_or_default();
    match json.pointer("/embedded/trust").and_then(|t| t.as_str()) {
        _ if agent_carried => Err("a launch the driver owns is never attached to".into()),
        Some("development") => Ok(()),
        Some(other) => Err(format!("the build's trust is {other}, not development")),
        None => Err("the build names no trust; only a development build listens".into()),
    }
}

enum Stage {
    Handshake,
    Asking,
    Attached,
}

/// A connection's replies, written off the display thread: a reader that
/// stops reading costs the connection, never the loop.
struct Writer {
    lines: Sender<(Vec<u8>, bool)>,
    failed: Arc<AtomicBool>,
    queued: Arc<AtomicUsize>,
}

impl Writer {
    fn spawn(stream: UnixStream, wake: UnixStream) -> Self {
        let (lines, rx) = channel::<(Vec<u8>, bool)>();
        let (failed, queued) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicUsize::new(0)),
        );
        let (fail, count) = (failed.clone(), queued.clone());
        std::thread::spawn(move || {
            for (bytes, last) in rx {
                let ok = write_within(&stream, &bytes, WRITE);
                count.fetch_sub(bytes.len(), Ordering::SeqCst);
                if !ok {
                    fail.store(true, Ordering::SeqCst);
                    let _ = (&wake).write(&[1]);
                    let _ = stream.shutdown(Shutdown::Both);
                    return;
                }
                // A refusal is the last line: half-closed, so a driver still connecting reads it, then closed.
                if last {
                    let _ = stream.shutdown(Shutdown::Write);
                    let until = Instant::now() + LINGER;
                    let mut sink = [0u8; 4096];
                    while Instant::now() < until {
                        match (&stream).read(&mut sink) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(20))
                            }
                            Err(_) => break,
                        }
                    }
                    let _ = stream.shutdown(Shutdown::Both);
                    return;
                }
            }
        });
        Self {
            lines,
            failed,
            queued,
        }
    }

    /// Queue one line; false once the queue passes its bound or a write failed.
    fn send(&self, line: &str, last: bool) -> bool {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        if self.failed.load(Ordering::SeqCst)
            || self.queued.fetch_add(bytes.len(), Ordering::SeqCst) + bytes.len() > QUEUE_LIMIT
        {
            return false;
        }
        self.lines.send((bytes, last)).is_ok()
    }
}

/// `bytes` written whole by `within`, on a socket the reader keeps nonblocking.
fn write_within(stream: &UnixStream, bytes: &[u8], within: Duration) -> bool {
    let (until, mut offset) = (Instant::now() + within, 0);
    while offset < bytes.len() {
        match (&*stream).write(&bytes[offset..]) {
            Ok(0) => return false,
            Ok(n) => offset += n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {
                if Instant::now() > until {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(_) => return false,
        }
    }
    true
}

struct Connection {
    id: u64,
    stream: UnixStream,
    writer: Writer,
    pending: Vec<u8>,
    stage: Stage,
    since: Instant,
    eof: bool,
}

/// The socket and the marker that names it, for one launch, and the one
/// connection: asking, or attached.
pub(crate) struct Listener {
    listener: UnixListener,
    socket: PathBuf,
    marker: PathBuf,
    nonce: String,
    /// Written to wake the display loop: at a deadline, for a line still to answer, or by a writer that failed.
    wake: (UnixStream, UnixStream),
    connection: Option<Connection>,
    next: u64,
    accept_paused: Option<Instant>,
    frame: Option<Arc<Pixmap>>,
}

/// `path` as a directory this user owns that no one else can enter, and no link.
fn private(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    if meta.uid() != uid() {
        return Err(format!("{} is owned by another user", path.display()));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(format!("{} is open to other users", path.display()));
    }
    Ok(())
}

#[allow(unsafe_code)]
fn uid() -> u32 {
    // SAFETY: getuid cannot fail and touches no memory.
    unsafe { libc::getuid() }
}

/// The connecting process's user, from the kernel: `SO_PEERCRED` on Linux,
/// `getpeereid` elsewhere.
#[allow(unsafe_code)]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let fd = stream.as_raw_fd();
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: `cred` and `len` are valid for the call and sized for SO_PEERCRED's struct.
        let r = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut len,
            )
        };
        (r == 0).then_some(cred.uid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: both out-pointers are valid for the call.
        let r = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
        (r == 0).then_some(uid)
    }
}

/// When this process started, seconds since 1970, which a driver checks against the process it finds: a recycled
/// PID started later. Linux's `/proc/self/stat` and `/proc/stat` say it; elsewhere it is not said.
fn started() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let ticks: f64 = stat
            .rsplit(')')
            .next()?
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()?;
        let boot: f64 = std::fs::read_to_string("/proc/stat")
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("btime "))?
            .trim()
            .parse()
            .ok()?;
        Some(boot + ticks / 100.)
    }
    #[cfg(not(target_os = "linux"))]
    None
}

fn nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn text(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| {
        format!(
            "{} is not UTF-8, which the marker's JSON cannot name",
            path.display()
        )
    })
}

impl Listener {
    /// Admit the build, then bind the socket in `$XDG_RUNTIME_DIR/exact/`, both directories checked private, and
    /// publish its marker naming `app`.
    pub(crate) fn open(
        runtime: Option<&Path>,
        compat: &str,
        agent_carried: bool,
        app: &str,
    ) -> Result<Self, String> {
        admitted(compat, agent_carried)?;
        let runtime =
            runtime.ok_or("XDG_RUNTIME_DIR is unset: no private directory to listen in")?;
        private(runtime)?;
        let dir = runtime.join("exact");
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        }
        private(&dir)?;
        let pid = std::process::id();
        let socket = dir.join(format!("a{pid}"));
        if socket.as_os_str().len() > 103 {
            return Err(format!(
                "{} is longer than a Unix socket path can be",
                socket.display()
            ));
        }
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "v": VERSION, "pid": pid, "started": started(), "bundle": app, "socket": text(&socket)?,
            "executable": text(&executable)?, "nonce": nonce()?,
        });
        let _ = std::fs::remove_file(&socket);
        // No other user can enter `dir`, so the path still names this socket when its mode is set.
        let listener =
            UnixListener::bind(&socket).map_err(|e| format!("bind {}: {e}", socket.display()))?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let marker = dir.join(format!("exact-attach-{pid}.json"));
        let staging = dir.join(format!("exact-attach-{pid}.json.part"));
        let _ = std::fs::remove_file(&staging);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&staging)
            .and_then(|mut f| f.write_all(body.to_string().as_bytes()))
            .and_then(|()| std::fs::rename(&staging, &marker))
            .map_err(|e| {
                let _ = std::fs::remove_file(&staging);
                let _ = std::fs::remove_file(&socket);
                format!("marker {}: {e}", marker.display())
            })?;
        let wake = UnixStream::pair().map_err(|e| e.to_string())?;
        wake.0.set_nonblocking(true).map_err(|e| e.to_string())?;
        let nonce = body["nonce"].as_str().unwrap_or_default().to_owned();
        Ok(Self {
            listener,
            socket,
            marker,
            nonce,
            wake,
            connection: None,
            next: 0,
            accept_paused: None,
            frame: None,
        })
    }

    pub(crate) fn fds(&self) -> Vec<i32> {
        let mut fds = vec![self.wake.0.as_raw_fd()];
        if self.accept_paused.is_none() {
            fds.push(self.listener.as_raw_fd());
        }
        fds.extend(
            self.connection
                .as_ref()
                .filter(|c| !c.eof)
                .map(|c| c.stream.as_raw_fd()),
        );
        fds
    }

    /// The frame the display last presented: a screenshot's pixels, never painted for it.
    pub(crate) fn presented(&mut self, frame: Arc<Pixmap>) {
        self.frame = Some(frame);
    }

    /// Wake the display loop at `at`, from a thread that only sleeps.
    fn wake_at(&self, at: Instant) {
        let Ok(wake) = self.wake.1.try_clone() else {
            return;
        };
        std::thread::spawn(move || {
            std::thread::sleep(at.saturating_duration_since(Instant::now()));
            let _ = (&wake).write(&[1]);
        });
    }

    /// Accept, read and answer what has arrived, on the display loop's thread: at most a few connections and one
    /// request a turn, so the display paints between them. `wall` is the loop's own clock, in ms.
    pub(crate) fn service<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        person: &mut dyn Person,
        wall: f64,
    ) {
        let mut drained = [0u8; 64];
        while matches!((&self.wake.0).read(&mut drained), Ok(n) if n > 0) {}
        if self
            .accept_paused
            .is_some_and(|until| Instant::now() >= until)
        {
            self.accept_paused = None;
        }
        for _ in 0..4 {
            if self.accept_paused.is_some() {
                break;
            }
            match self.listener.accept() {
                Ok((stream, _)) => self.accepted(stream),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // Out of descriptors, say: the queued connection keeps the listener readable, so it rests a second.
                Err(_) => {
                    let until = Instant::now() + Duration::from_secs(1);
                    self.accept_paused = Some(until);
                    self.wake_at(until);
                }
            }
        }
        if let Some(why) = self.turn(p, person, wall) {
            self.end(person, why);
        }
    }

    fn accepted(&mut self, stream: UnixStream) {
        // Only a process running as this user; the nonce then names this listener.
        if peer_uid(&stream) != Some(uid()) || stream.set_nonblocking(true).is_err() {
            return;
        }
        let Ok(wake) = self.wake.1.try_clone() else {
            return;
        };
        let Ok(write) = stream.try_clone() else {
            return;
        };
        let writer = Writer::spawn(write, wake);
        if self.connection.is_some() {
            writer.send(
                &refusal(None, "busy", "another agent is attached to this app"),
                true,
            );
            return;
        }
        self.next += 1;
        self.connection = Some(Connection {
            id: self.next,
            stream,
            writer,
            pending: Vec::new(),
            stage: Stage::Handshake,
            since: Instant::now(),
            eof: false,
        });
        self.wake_at(Instant::now() + HANDSHAKE);
    }

    /// One connection's progress, or why it ends; a refusal is its last line.
    fn turn<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        person: &mut dyn Person,
        wall: f64,
    ) -> Option<String> {
        let nonce = self.nonce.clone();
        let c = self.connection.as_mut()?;
        if c.writer.failed.load(Ordering::SeqCst) {
            return Some("the agent did not read its replies".into());
        }
        let mut chunk = [0u8; 65536];
        while !c.eof {
            match c.stream.read(&mut chunk) {
                Ok(0) => c.eof = true,
                Ok(n) => c.pending.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Some(format!("read: {e}")),
            }
        }
        let line_end = c.pending.iter().position(|&b| b == b'\n');
        if line_end.unwrap_or(c.pending.len()) > REQUEST_LIMIT {
            c.writer
                .send(&refusal(None, "malformed", "a request over 1 MiB"), true);
            return Some("a request over 1 MiB".into());
        }
        let mut take = || {
            line_end.map(|end| {
                String::from_utf8_lossy(&c.pending.drain(..=end).take(end).collect::<Vec<u8>>())
                    .into_owned()
            })
        };
        match c.stage {
            Stage::Handshake => {
                // The deadline first: a handshake complete only after it is late, whenever the loop reads it.
                if c.since.elapsed() > HANDSHAKE {
                    c.writer.send(
                        &refusal(None, "handshake", "none arrived within 10 s"),
                        true,
                    );
                    return Some("no handshake within 10 s".into());
                }
                let Some(line) = take() else {
                    return c.eof.then(|| "the agent hung up".into());
                };
                if let Err(why) = handshake(&line, &nonce) {
                    c.writer.send(&refusal(None, "handshake", &why), true);
                    return Some(why);
                }
                c.stage = Stage::Asking;
                c.since = Instant::now();
                person.ask(c.id);
                let waiting = serde_json::json!({"attach": VERSION, "waiting": "approval", "pid": std::process::id()});
                c.writer.send(&waiting.to_string(), false);
                let (at, again) = (c.since + APPROVAL, c.pending.contains(&b'\n'));
                self.wake_at(at);
                if again {
                    self.wake_at(Instant::now());
                }
                None
            }
            Stage::Asking => {
                if line_end.is_some() {
                    return Some("a request before the person allowed reads".into());
                }
                // The deadline first, so an answer read late is no answer.
                if c.since.elapsed() > APPROVAL {
                    c.writer.send(
                        &refusal(None, "declined", "not allowed in the app within 60 s"),
                        true,
                    );
                    return Some("not allowed within 60 s".into());
                }
                match person.answer(c.id) {
                    Some(true) => {
                        c.stage = Stage::Attached;
                        person.attached(true);
                        let attached = serde_json::json!({
                            "attached": true, "pid": std::process::id(), "session": "main", "window": "main",
                            "grants": {"read": true, "input": false}, "build": {"commit": null}, "planDigest": null,
                        });
                        c.writer
                            .send(&attached.to_string(), false)
                            .not()
                            .then(|| "the agent did not read its replies".into())
                    }
                    Some(false) => {
                        c.writer.send(
                            &refusal(None, "declined", "the person did not allow it"),
                            true,
                        );
                        Some("declined".into())
                    }
                    None => c.eof.then(|| "the agent hung up".into()),
                }
            }
            Stage::Attached => {
                let Some(line) = take() else {
                    return c.eof.then(|| "the agent hung up".into());
                };
                let mut reply = answer(p, &line, self.frame.as_deref(), wall);
                if reply.len() > REPLY_LIMIT {
                    reply = refusal(
                        request_number(&line),
                        "reply-size",
                        "the reply is over 32 MiB",
                    );
                }
                if !c.writer.send(&reply, false) {
                    return Some("the agent's unread replies passed 64 MiB".into());
                }
                // One request a turn: another already here is answered on the next.
                if c.pending.contains(&b'\n') {
                    let _ = (&self.wake.1).write(&[1]);
                }
                None
            }
        }
    }

    /// The connection ends: its prompt or its indicator goes. A refusal already queued is still written, then the
    /// writer closes the socket; otherwise it closes now.
    fn end(&mut self, person: &mut dyn Person, why: String) {
        if let Some(c) = self.connection.take() {
            person.ended(c.id);
            if let Stage::Attached = c.stage {
                person.attached(false);
            }
            if c.writer.queued.load(Ordering::SeqCst) == 0 {
                let _ = c.stream.shutdown(Shutdown::Both);
            }
            eprintln!("exact: agent attach ended: {why}");
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.marker);
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn request_number(line: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("request")
        .and_then(count)
}

fn refusal(request: Option<u64>, code: &str, why: &str) -> String {
    serde_json::json!({"request": request, "error": format!("{code}: {why}"), "refused": code})
        .to_string()
}

fn handshake(line: &str, nonce: &str) -> Result<(), String> {
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|_| "handshake: not JSON".to_string())?;
    let o = v.as_object().ok_or("handshake: not an object")?;
    if !distinct_keys(line)
        || o.keys()
            .any(|k| !["attach", "nonce", "session"].contains(&k.as_str()))
        || o.get("attach") != Some(&VERSION.into())
    {
        return Err(format!(
            "handshake: expected {{\"attach\":{VERSION},\"nonce\":…}} as the first line"
        ));
    }
    let offered = o.get("nonce").and_then(|n| n.as_str()).unwrap_or_default();
    let same = offered.len() == nonce.len()
        && offered
            .bytes()
            .zip(nonce.bytes())
            .fold(0, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !same {
        return Err("handshake: the nonce names another listener".into());
    }
    match o.get("session") {
        None => Ok(()),
        Some(s) if s == "main" => Ok(()),
        Some(s) => Err(format!("attach: no window shows session {s}")),
    }
}

/// Whether the object's top-level keys are distinct, compared as decoded: serde keeps one of a repeated key.
fn distinct_keys(line: &str) -> bool {
    let mut keys = std::collections::HashSet::new();
    let (bytes, mut depth, mut key_next, mut i) = (line.as_bytes(), 0i32, false, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'"' {
                    j += if bytes[j] == b'\\' { 2 } else { 1 };
                }
                if depth == 1 && key_next {
                    let Ok(key) = serde_json::from_str::<String>(&line[i..(j + 1).min(line.len())])
                    else {
                        return false;
                    };
                    if !keys.insert(key) {
                        return false;
                    }
                }
                key_next = false;
                i = j;
            }
            b'{' | b'[' => {
                depth += 1;
                key_next = depth == 1;
            }
            b'}' | b']' => depth -= 1,
            b',' if depth == 1 => key_next = true,
            _ => {}
        }
        i += 1;
    }
    true
}

enum Field {
    Flag,
    Count,
    Text,
    Target,
}

/// Each read and the fields it takes (LLP 1012.000 D3).
fn form(op: &str) -> Option<&'static [(&'static str, Field)]> {
    Some(match op {
        "tree" => &[
            ("target", Field::Target),
            ("shallow", Field::Flag),
            ("plan", Field::Flag),
        ],
        "state" => &[("id", Field::Count), ("under", Field::Text)],
        "logs" => &[("since", Field::Count), ("all", Field::Flag)],
        "layout" => &[("id", Field::Count), ("plan", Field::Flag)],
        "perf" => &[("target", Field::Count)],
        "screenshot" | "tags" | "grants" => &[],
        _ => return None,
    })
}

/// A whole number within a view id's range, not a Boolean.
fn count(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().filter(|&n| n <= u64::from(u32::MAX))
}

/// One attached request: refused by name, or built from its checked fields and answered from what the presenter holds.
fn answer<D: DataSource>(
    p: &mut Presenter<D>,
    line: &str,
    frame: Option<&Pixmap>,
    wall: f64,
) -> String {
    let v: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
    let Some(o) = v.as_object() else {
        return refusal(None, "malformed", "a request is one JSON object");
    };
    let id = o.get("request").and_then(count).filter(|&n| n > 0);
    let refuse = |code: &str, why: &str| refusal(id, code, why);
    if !distinct_keys(line) {
        return refuse("malformed", "a field is named twice");
    }
    let Some(id) = id else {
        return refusal(
            None,
            "malformed",
            "a request needs a positive integer request number",
        );
    };
    let Some(op) = o.get("op").and_then(|op| op.as_str()) else {
        return refuse("malformed", "a request names its op");
    };
    if o.contains_key("session") {
        return refuse(
            "pinned",
            "this connection reads the window the person approved; a request names no session",
        );
    }
    if op == "clock" {
        return refuse("live-clock", "an attached app keeps its own wall clock: it is never taken, seeked, settled or landed");
    }
    if op == "perf" && o.contains_key("frames") {
        return refuse("unsupported", "perf frames reads a presented-frame history no Linux display keeps for an attached session yet");
    }
    if op == "state" && o.contains_key("reveal") {
        return refuse("masked", "an attached session reads state masked");
    }
    let Some(fields) = form(op) else {
        return refuse("unsupported", &format!("{op} is not offered to an attached session on Linux, whose input is not qualified yet"));
    };
    let mut request = serde_json::Map::new();
    request.insert("op".into(), op.into());
    for (key, value) in o
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "request" | "op"))
    {
        let Some((_, field)) = fields.iter().find(|(name, _)| name == key) else {
            return refuse("malformed", &format!("{op} takes no field {key}"));
        };
        let checked = match field {
            Field::Flag => value.as_bool().map(Into::into),
            Field::Count => count(value).map(Into::into),
            Field::Text => value.as_str().filter(|t| t.len() <= 256).map(Into::into),
            Field::Target => count(value).map(Into::into).or_else(|| {
                value
                    .as_str()
                    .filter(|t| !t.is_empty() && t.len() <= 256)
                    .map(Into::into)
            }),
        };
        let Some(checked) = checked else {
            return refuse("malformed", &format!("{key} is not a {op} field's kind"));
        };
        request.insert(key.clone(), checked);
    }
    let reply = match op {
        "grants" => serde_json::json!({"read": true, "input": false}).to_string(),
        "screenshot" => match frame.and_then(|f| f.encode_png().ok().map(|png| (f, png))) {
            Some((f, png)) => serde_json::json!({
                "data": exact_runner::agent::base64(&png), "bytes": png.len(), "width": f.width(), "height": f.height(),
                "captureSpace": "window", "capture": "last-presented-frame",
            })
            .to_string(),
            None => return refuse("unsupported", "the display has presented no frame to read"),
        },
        "state" => {
            // The observation the Apple hosts add: read attached, on the wall clock, which nothing here moves.
            let mut s = exact_painted::agent::attached::passive(p, &serde_json::Value::Object(request).to_string());
            if s.ends_with('}') && !s.starts_with("{\"error\"") {
                s.pop();
                let clock = p.host().runner().now_ms();
                s.push_str(&format!(",\"observation\":{{\"attached\":true,\"wall\":{wall},\"clock\":{clock},\"clockMode\":\"real\"}}}}"));
            }
            s
        }
        _ => exact_painted::agent::attached::passive(p, &serde_json::Value::Object(request).to_string()),
    };
    // The request's number first, then the reply's own fields.
    match reply.strip_prefix('{') {
        Some(rest) if rest.starts_with('}') => format!("{{\"request\":{id}}}"),
        Some(rest) => format!("{{\"request\":{id},{rest}"),
        None => refuse("unsupported", "an unreadable reply"),
    }
}

/// The display loop's events with an attach listener beside them: its
/// descriptors wake the loop, and each turn answers what arrived. The last
/// presented frame is kept for a screenshot, which is never painted for it:
/// the display mirrors its frames, a readback on a GPU painter, only while a
/// listener is open, which the person's Enable Agent Attach opens (piece 8).
pub(crate) struct Attached<E, P> {
    pub(crate) inner: E,
    pub(crate) listener: Listener,
    pub(crate) person: P,
}

impl<D: DataSource, E: Events<D>, P: Person> Events<D> for Attached<E, P> {
    fn fds(&self) -> Vec<i32> {
        let mut fds = self.inner.fds();
        fds.extend(self.listener.fds());
        fds
    }
    fn idle(&self) -> bool {
        self.inner.idle()
    }
    fn mirror(&self) -> bool {
        true
    }
    fn publish(&mut self, pixels: Arc<Pixmap>) {
        self.listener.presented(pixels.clone());
        if self.inner.mirror() {
            self.inner.publish(pixels);
        }
    }
    fn turn(&mut self, p: &mut Presenter<D>, wall: impl Fn() -> f64) {
        let now = wall();
        self.inner.turn(p, wall);
        self.listener.service(p, &mut self.person, now);
    }
    fn date_jumped(&self) -> bool {
        self.inner.date_jumped()
    }
    fn trace_requested(&self) -> bool {
        self.inner.trace_requested()
    }
}
