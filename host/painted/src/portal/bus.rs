//! One connection to the session bus, and a pick asked over it.

use super::wire::{self, Message, Value};
use super::{Outcome, Socket};
use exact_runner::file_pickers::{Kind, Picker};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
const DBUS: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";

/// A connection to a message bus, authenticated and named.
pub(crate) struct Bus {
    stream: UnixStream,
    serial: u32,
    /// The name the bus gave this connection (`Hello`).
    pub unique: String,
    /// Signals that arrived while a call waited for its reply.
    queued: VecDeque<Message>,
}

impl Bus {
    /// Connect to the bus at `address`, each answer bounded by `timeout`:
    /// SASL `EXTERNAL` as this process's user, then `Hello`.
    pub(crate) fn connect(address: &str, timeout: Duration) -> Result<Bus, String> {
        let socket = super::socket_of(address).ok_or("its address names no unix socket")?;
        let stream = match socket {
            Socket::Path(path) => UnixStream::connect(path),
            #[cfg(target_os = "linux")]
            Socket::Abstract(name) => {
                use std::os::linux::net::SocketAddrExt;
                std::os::unix::net::SocketAddr::from_abstract_name(name)
                    .and_then(|a| UnixStream::connect_addr(&a))
            }
            #[cfg(not(target_os = "linux"))]
            Socket::Abstract(_) => {
                return Err("this host reaches an abstract socket only on Linux".into())
            }
        }
        .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        let mut bus = Bus {
            stream,
            serial: 0,
            unique: String::new(),
            queued: VecDeque::new(),
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
        let io = |e: std::io::Error| e.to_string();
        bus.stream
            .write_all(format!("\0AUTH EXTERNAL {uid}\r\n").as_bytes())
            .map_err(io)?;
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            if line.len() > 512 {
                return Err("its authentication line is too long".into());
            }
            bus.stream.read_exact(&mut byte).map_err(io)?;
            line.push(byte[0]);
        }
        if !line.starts_with(b"OK ") {
            return Err("it refused this user".into());
        }
        bus.stream.write_all(b"BEGIN\r\n").map_err(io)?;
        match bus.call(DBUS, DBUS_PATH, DBUS, "Hello", &[])?.as_slice() {
            [Value::Str(name)] => bus.unique = name.clone(),
            _ => return Err("it gave this connection no name".into()),
        }
        Ok(bus)
    }

    /// Wait without limit: the person's own time at the chooser.
    pub(crate) fn wait_forever(&mut self) {
        let _ = self.stream.set_read_timeout(None);
    }

    fn message(&mut self) -> Result<Message, String> {
        let mut head = [0u8; 16];
        self.stream
            .read_exact(&mut head)
            .map_err(|e| e.to_string())?;
        let len = wire::message_len(&head).ok_or("a message did not read")?;
        let mut buf = head.to_vec();
        buf.resize(len, 0);
        self.stream
            .read_exact(&mut buf[16..])
            .map_err(|e| e.to_string())?;
        wire::parse(&buf).ok_or_else(|| "a message did not read".into())
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
        );
        self.stream.write_all(&call).map_err(|e| e.to_string())?;
        loop {
            let m = self.message()?;
            match m.kind {
                wire::SIGNAL => self.queued.push_back(m),
                wire::METHOD_RETURN if m.reply_serial == Some(serial) => return Ok(m.body),
                wire::ERROR if m.reply_serial == Some(serial) => {
                    return Err(match m.body.first() {
                        Some(Value::Str(text)) => format!("{}: {text}", m.error_name),
                        _ => m.error_name,
                    })
                }
                _ => {}
            }
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
        }
    }
}

/// Ask the portal at the session bus `address` for `picker`'s choice, and
/// wait for it: the body of a pick's thread. `None`: no session bus.
pub(crate) fn choose(picker: &Picker, address: Option<&str>, timeout: Duration) -> Outcome {
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
    match ask(&mut bus, picker) {
        Ok(outcome) => outcome,
        Err(e) => Outcome::Refused(e),
    }
}

static TOKENS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn ask(bus: &mut Bus, picker: &Picker) -> Result<Outcome, String> {
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
    let (member, options) = super::request(picker, &token);
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
