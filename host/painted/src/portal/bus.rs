//! One connection to the session bus, and a pick asked over it.

use super::wire::{self, Message, Value};
use super::{Filter, Outcome, Socket};
use exact_runner::file_pickers::{Kind, Picker};
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
const DBUS: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";

#[cfg(target_os = "linux")]
mod settings;
#[cfg(target_os = "linux")]
pub(crate) use settings::watch_settings;

/// A connection to a message bus, authenticated and named.
pub(crate) struct Bus {
    stream: UnixStream,
    serial: u32,
    /// The name the bus gave this connection (`Hello`).
    pub unique: String,
    /// Signals that arrived while a call waited for its reply, kept for
    /// [`Bus::signal`] unless the connection ignores signals.
    queued: VecDeque<Message>,
    /// Whether signals are kept for [`Bus::signal`].
    signals: bool,
    /// Whether the bus agreed to pass Unix file descriptors.
    passes_fds: bool,
    /// What was read and is not yet a whole line or message, kept when a
    /// wait runs out, so the next read goes on from it and a reply that
    /// comes late is read whole.
    read: Vec<u8>,
    /// How long each wait for the bus may take; none for no limit.
    wait: Option<Duration>,
    /// When every wait ends, whatever `wait` allows.
    deadline: Option<Instant>,
    /// Whether the stream lost its place in the protocol: it ended, failed
    /// otherwise than by a wait running out, a message went out only in
    /// part, or what came cannot be a message. No later call can use it.
    lost: bool,
}

impl Bus {
    /// Connect to the bus at `address`, each answer bounded by `timeout`:
    /// SASL `EXTERNAL` as this process's user, then `Hello`.
    pub(crate) fn connect(address: &str, timeout: Duration) -> Result<Bus, String> {
        Self::open(address, timeout, None, false)
    }

    /// [`Bus::connect`], asking the bus to pass Unix file descriptors
    /// (`NEGOTIATE_UNIX_FD`), as [`Bus::call_passing`] needs, and with
    /// every wait, the connection's own included, ended by `deadline`.
    #[cfg(target_os = "linux")]
    pub(crate) fn connect_passing(
        address: &str,
        timeout: Duration,
        deadline: Option<Instant>,
    ) -> Result<Bus, String> {
        Self::open(address, timeout, deadline, true)
    }

    fn open(
        address: &str,
        timeout: Duration,
        deadline: Option<Instant>,
        fds: bool,
    ) -> Result<Bus, String> {
        let socket = super::socket_of(address).ok_or("its address names no unix socket")?;
        let first = left(Some(timeout), deadline)
            .map_err(|()| "its time ran out")?
            .unwrap_or(timeout);
        let stream = connect(&socket, first).map_err(|e| e.to_string())?;
        let mut bus = Bus {
            stream,
            serial: 0,
            unique: String::new(),
            queued: VecDeque::new(),
            signals: true,
            passes_fds: false,
            read: Vec::new(),
            wait: Some(timeout),
            deadline,
            lost: false,
        };
        // The user id as hex of its decimal digits (the specification's
        // "EXTERNAL" mechanism).
        // SAFETY: getuid(2) takes nothing and always succeeds.
        #[allow(unsafe_code)]
        let uid = unsafe { libc::getuid() };
        let uid: String = uid
            .to_string()
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect();
        bus.write(format!("\0AUTH EXTERNAL {uid}\r\n").as_bytes(), &[])?;
        if !bus.line()?.starts_with(b"OK ") {
            return Err("it refused this user".into());
        }
        if fds {
            bus.write(b"NEGOTIATE_UNIX_FD\r\n", &[])?;
            bus.passes_fds = bus.line()?.starts_with(b"AGREE_UNIX_FD");
        }
        bus.write(b"BEGIN\r\n", &[])?;
        match bus.call(DBUS, DBUS_PATH, DBUS, "Hello", &[])?.as_slice() {
            [Value::Str(name)] => bus.unique = name.clone(),
            _ => return Err("it gave this connection no name".into()),
        }
        Ok(bus)
    }

    /// One line of the authentication exchange, its CRLF included.
    fn line(&mut self) -> Result<Vec<u8>, String> {
        loop {
            if let Some(end) = self.read.windows(2).position(|w| w == b"\r\n") {
                return Ok(self.read.drain(..end + 2).collect());
            }
            if self.read.len() > 512 {
                self.lost = true;
                return Err("its authentication line is too long".into());
            }
            self.more()?;
        }
    }

    /// Wait without limit: the person's own time at the chooser.
    pub(crate) fn wait_forever(&mut self) {
        self.wait = None;
        self.deadline = None;
    }

    /// Whether the connection lost its place in the protocol, so that no
    /// later call can use it; a call that failed otherwise (an error the bus
    /// or the service answered, a wait that ran out) leaves it usable.
    #[cfg(target_os = "linux")]
    pub(crate) fn lost(&self) -> bool {
        self.lost
    }

    /// A second handle on the connection's stream, to write calls on while
    /// this one reads ([`Bus::next`]).
    #[cfg(target_os = "linux")]
    pub(crate) fn writer(&self) -> std::io::Result<UnixStream> {
        self.stream.try_clone()
    }

    /// The serial of the last call made.
    #[cfg(target_os = "linux")]
    pub(crate) fn serial(&self) -> u32 {
        self.serial
    }

    /// Whether the bus agreed to pass Unix file descriptors.
    #[cfg(target_os = "linux")]
    pub(crate) fn passes_fds(&self) -> bool {
        self.passes_fds
    }

    /// The next message the bus sends, waiting as long as `wait` and
    /// `deadline` allow.
    #[cfg(target_os = "linux")]
    pub(crate) fn next(&mut self) -> Result<Message, String> {
        self.message()
    }

    /// Keep no signal: a connection no one hears signals on.
    #[cfg(target_os = "linux")]
    pub(crate) fn ignore_signals(&mut self) {
        self.signals = false;
        self.queued.clear();
    }

    fn message(&mut self) -> Result<Message, String> {
        while self.read.len() < 16 {
            self.more()?;
        }
        let head: &[u8; 16] = self.read[..16].try_into().expect("16 bytes were read");
        let Some(len) = wire::message_len(head) else {
            self.lost = true;
            return Err("a message did not read".into());
        };
        while self.read.len() < len {
            self.more()?;
        }
        let message = wire::parse(&self.read[..len]);
        self.read.drain(..len);
        message.ok_or_else(|| "a message did not read".into())
    }

    /// Read what the bus sent next into `read`, waiting as long as `wait`
    /// and `deadline` allow.
    fn more(&mut self) -> Result<(), String> {
        let mut chunk = [0u8; 4096];
        loop {
            self.bound(Reading)?;
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    self.lost = true;
                    return Err("the bus closed the connection".into());
                }
                Ok(n) => {
                    self.read.extend_from_slice(&chunk[..n]);
                    return Ok(());
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err("the bus did not answer in time".into());
                }
                Err(e) => {
                    self.lost = true;
                    return Err(e.to_string());
                }
            }
        }
    }

    /// Write `bytes`, with `fds` beside the first of them (`SCM_RIGHTS`),
    /// each wait bounded as [`Bus::more`]'s. A message written only in part
    /// loses the stream its place.
    fn write(&mut self, bytes: &[u8], fds: &[std::os::fd::BorrowedFd<'_>]) -> Result<(), String> {
        let mut at = 0;
        while at < bytes.len() {
            if let Err(e) = self.bound(Writing) {
                self.lost |= at > 0;
                return Err(e);
            }
            let wrote = match (at, fds) {
                #[cfg(target_os = "linux")]
                (0, [_, ..]) => send_passing(&self.stream, bytes, fds),
                #[cfg(not(target_os = "linux"))]
                (0, [_, ..]) => return Err("this host passes no file descriptors".into()),
                _ => (&self.stream).write(&bytes[at..]),
            };
            match wrote {
                Ok(0) => {
                    self.lost = true;
                    return Err("the bus took no more".into());
                }
                Ok(n) => at += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => {
                    // A wait that ran out with nothing written leaves the stream where it was
                    let waited = matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut);
                    self.lost |= at > 0 || !waited;
                    return Err(match waited {
                        true => "the bus did not take the call in time".into(),
                        false => e.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Set the stream's timeout for `way` to what `wait` and `deadline`
    /// leave for the next wait; an error once nothing is left, and one that
    /// loses the stream when it cannot be set. Each way sets its own, so a
    /// thread reading and another writing the same socket never set the
    /// other's.
    fn bound(&mut self, way: Way) -> Result<(), String> {
        let left = left(self.wait, self.deadline).map_err(|()| "its time ran out")?;
        let set = match way {
            Reading => self.stream.set_read_timeout(left),
            Writing => self.stream.set_write_timeout(left),
        };
        set.map_err(|e| {
            self.lost = true;
            e.to_string()
        })
    }

    /// End the connection, both ways, whoever else holds its stream.
    #[cfg(target_os = "linux")]
    pub(crate) fn shutdown(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    /// Call `member` with `body`; its reply's values, or the error's name
    /// and message.
    pub(crate) fn call(
        &mut self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: &[Value],
    ) -> Result<Vec<Value>, String> {
        self.call_passing(destination, path, interface, member, body, &[])
    }

    /// [`Bus::call`], sending `fds` beside it, which `body`'s
    /// [`Value::Fd`]s index (`SCM_RIGHTS`).
    pub(crate) fn call_passing(
        &mut self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: &[Value],
        fds: &[std::os::fd::BorrowedFd<'_>],
    ) -> Result<Vec<Value>, String> {
        if !fds.is_empty() && !self.passes_fds {
            return Err("the session bus passes no file descriptors".into());
        }
        self.serial += 1;
        let serial = self.serial;
        let call = wire::message(
            wire::METHOD_CALL,
            serial,
            None,
            destination,
            path,
            interface,
            member,
            body,
            fds.len() as u32,
        );
        self.write(&call, fds)?;
        self.reply(serial)
    }

    /// The reply to call `serial`: its values, or the error's name and
    /// message. Other messages meanwhile go where [`Bus::route`] puts them.
    fn reply(&mut self, serial: u32) -> Result<Vec<Value>, String> {
        loop {
            let m = self.message()?;
            if m.reply_serial != Some(serial) {
                self.route(m);
                continue;
            }
            match m.kind {
                wire::METHOD_RETURN => return Ok(m.body),
                wire::ERROR => {
                    return Err(match m.body.first() {
                        Some(Value::Str(text)) => format!("{}: {text}", m.error_name),
                        _ => m.error_name,
                    })
                }
                _ => {}
            }
        }
    }

    /// Keep a message no call waits for when it is a signal, for
    /// [`Bus::signal`], unless signals are ignored; drop anything else.
    fn route(&mut self, m: Message) {
        if m.kind == wire::SIGNAL && self.signals {
            self.queued.push_back(m);
        }
    }

    /// Hear the signals `rule` matches (`AddMatch`).
    pub(crate) fn subscribe(&mut self, rule: &str) -> Result<(), String> {
        self.call(DBUS, DBUS_PATH, DBUS, "AddMatch", &[Value::str(rule)])
            .map(drop)
    }

    /// The next signal, or why none will come.
    pub(crate) fn signal(&mut self) -> Result<Message, String> {
        if let Some(m) = self.queued.pop_front() {
            return Ok(m);
        }
        loop {
            let m = self.message()?;
            if m.kind == wire::SIGNAL {
                return Ok(m);
            }
            self.route(m);
        }
    }
}

/// Which way a wait on the bus goes.
enum Way {
    Reading,
    Writing,
}
use Way::{Reading, Writing};

/// What `wait` and `deadline` leave for one wait: none for no limit, an
/// error once nothing is left.
fn left(wait: Option<Duration>, deadline: Option<Instant>) -> Result<Option<Duration>, ()> {
    let until = deadline.map(|d| d.saturating_duration_since(Instant::now()));
    let left = match (wait, until) {
        (Some(wait), Some(until)) => Some(wait.min(until)),
        (wait, until) => wait.or(until),
    };
    match left {
        Some(left) if left.is_zero() => Err(()),
        left => Ok(left),
    }
}

/// A stream connected to `socket`, the connection's own wait bounded by
/// `timeout`: while the bus's queue of connections is full, a connect waits
/// as long as the socket's send timeout allows (unix(7)).
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn connect(socket: &Socket, timeout: Duration) -> std::io::Result<UnixStream> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    let (name, path) = match socket {
        Socket::Path(path) => (path.as_os_str().as_bytes().to_vec(), true),
        Socket::Abstract(name) => ([&[0][..], name].concat(), false),
    };
    // SAFETY: an all-zero sockaddr_un is a valid empty one.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    // A path keeps room for its terminating NUL; an abstract name is counted by its length
    if name.len() + usize::from(path) > address.sun_path.len() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidInput,
            "the bus's socket name is too long",
        ));
    }
    for (to, &b) in address.sun_path.iter_mut().zip(&name) {
        *to = b as libc::c_char;
    }
    let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + name.len() + usize::from(path);
    // SAFETY: socket(2) takes no pointers.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `fd` is a new descriptor that nothing else owns.
    let stream = UnixStream::from(unsafe { OwnedFd::from_raw_fd(fd) });
    stream.set_write_timeout(Some(timeout))?;
    // SAFETY: `address` is a filled sockaddr_un of `length` bytes, live for the call.
    let connected = unsafe {
        libc::connect(
            stream.as_raw_fd(),
            (&raw const address).cast(),
            length as libc::socklen_t,
        )
    };
    if connected < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(stream)
}

/// A stream connected to `socket`; elsewhere than Linux the connection's
/// own wait is not bounded.
#[cfg(not(target_os = "linux"))]
fn connect(socket: &Socket, _: Duration) -> std::io::Result<UnixStream> {
    match socket {
        Socket::Path(path) => UnixStream::connect(path),
        Socket::Abstract(_) => Err(std::io::Error::new(
            ErrorKind::Unsupported,
            "this host reaches an abstract socket only on Linux",
        )),
    }
}

/// Write the start of `bytes` to `stream` in one `sendmsg`, with `fds`
/// beside its first byte (`SCM_RIGHTS`, unix(7)): how many bytes went.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub(crate) fn send_passing(
    stream: &UnixStream,
    bytes: &[u8],
    fds: &[std::os::fd::BorrowedFd<'_>],
) -> std::io::Result<usize> {
    use std::os::fd::AsRawFd;
    let raw: Vec<i32> = fds.iter().map(AsRawFd::as_raw_fd).collect();
    let data = std::mem::size_of_val(raw.as_slice()) as u32;
    // SAFETY: CMSG_SPACE and CMSG_LEN compute sizes only.
    let (space, len) = unsafe { (libc::CMSG_SPACE(data) as usize, libc::CMSG_LEN(data)) };
    // A u64 buffer keeps the control message aligned as cmsghdr needs
    let mut control = vec![0u64; space.div_ceil(8)];
    let mut iov = libc::iovec {
        iov_base: bytes.as_ptr() as *mut libc::c_void,
        iov_len: bytes.len(),
    };
    // SAFETY: an all-zero msghdr is a valid empty one.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = space as _;
    // SAFETY: the control buffer holds one header and `data` bytes, aligned, and outlives the call.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = len as _;
        std::ptr::copy_nonoverlapping(raw.as_ptr(), libc::CMSG_DATA(cmsg).cast(), raw.len());
    }
    // SAFETY: msg points at live buffers for the call; the descriptors stay open while borrowed.
    let n = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(n as usize)
}

/// Ask the portal at the session bus `address` for `picker`'s choice, and
/// wait for it: the body of a pick's thread. `None`: no session bus.
pub(crate) fn choose(
    picker: &Picker,
    filters: &[Filter],
    folder: Option<&std::path::Path>,
    address: Option<&str>,
    timeout: Duration,
) -> Outcome {
    let Some(address) = address.filter(|a| !a.is_empty()) else {
        return Outcome::Refused("no desktop portal: there is no session bus".into());
    };
    let mut bus = match Bus::connect(address, timeout) {
        Ok(bus) => bus,
        Err(e) => {
            return Outcome::Refused(format!(
                "no desktop portal: the session bus did not answer ({e})"
            ))
        }
    };
    match ask(&mut bus, picker, filters, folder) {
        Ok(outcome) => outcome,
        Err(e) => Outcome::Refused(e),
    }
}

static TOKENS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn ask(
    bus: &mut Bus,
    picker: &Picker,
    filters: &[Filter],
    folder: Option<&std::path::Path>,
) -> Result<Outcome, String> {
    // Whether a file chooser is there at all, and which version: a folder
    // needs `directory`, version 3's.
    let version = bus
        .call(
            PORTAL,
            PORTAL_PATH,
            "org.freedesktop.DBus.Properties",
            "Get",
            &[Value::str(FILE_CHOOSER), Value::str("version")],
        )
        .map_err(|e| format!("no desktop portal file chooser ({e})"))?;
    let version = match version.as_slice() {
        [Value::Variant(v)] => match **v {
            Value::U32(n) => n,
            _ => 0,
        },
        _ => 0,
    };
    if picker.kind == Kind::Directory && version < 3 {
        return Err(format!(
            "the desktop portal's file chooser is version {version}, and choosing a folder needs 3"
        ));
    }
    let token = format!(
        "exact{}_{}",
        std::process::id(),
        TOKENS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    // Listen before asking, so an answer that comes at once is heard
    // (`org.freedesktop.portal.Request`).
    let expected = super::request_path(&bus.unique, &token);
    let response = |path: &str| {
        format!("type='signal',sender='{PORTAL}',interface='org.freedesktop.portal.Request',member='Response',path='{path}'")
    };
    bus.subscribe(&response(&expected))?;
    bus.subscribe(&format!(
        "type='signal',sender='{DBUS}',interface='{DBUS}',member='NameOwnerChanged',arg0='{PORTAL}'"
    ))?;
    let (member, options) = super::request(picker, &token, filters, folder);
    let title = super::title(picker.kind);
    let reply = bus
        .call(
            PORTAL,
            PORTAL_PATH,
            FILE_CHOOSER,
            member,
            &[Value::str(""), Value::str(title), options],
        )
        .map_err(|e| format!("the desktop portal refused the chooser ({e})"))?;
    // A portal older than `handle_token` answers on a path of its own.
    let handle = match reply.as_slice() {
        [Value::Path(handle)] => handle.clone(),
        _ => return Err("the desktop portal gave no request".into()),
    };
    if handle != expected {
        bus.subscribe(&response(&handle))?;
    }
    bus.wait_forever();
    loop {
        let m = bus
            .signal()
            .map_err(|e| format!("the session bus closed before the chooser answered ({e})"))?;
        match m.member.as_str() {
            "Response" if m.path == handle => return Ok(super::outcome(&m.body)),
            "NameOwnerChanged" => {
                if let [Value::Str(name), _, Value::Str(owner)] = m.body.as_slice() {
                    if name == PORTAL && owner.is_empty() {
                        return Err("the desktop portal quit before the chooser answered".into());
                    }
                }
            }
            _ => {}
        }
    }
}
