//! The document pickers in a Linux desktop session (@ref LLP 1069.010 D2,
//! LLP 1069.002 D8, LLP 1069.010.000 D3): `showOpenFilePicker`, `showDirectoryPicker` and
//! `showSaveFilePicker` ask the desktop portal's file chooser,
//! `org.freedesktop.portal.FileChooser` (its `OpenFile` and `SaveFile`:
//! <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html>),
//! over the session bus. The desktop draws its own chooser, so the choice
//! looks as every other app's does.
//!
//! The host links no D-Bus library (it links no system library at all):
//! `wire.rs` is the protocol, `bus.rs` one authenticated connection. A pick
//! runs on its own thread, since the answer is the person's and takes as
//! long as they do; everything before the chooser is shown is bounded by
//! [`TIMEOUT`]. With no session bus, no portal, or a portal without a file
//! chooser, the pick is refused, never left waiting.

use exact_runner::file_pickers::Kind;
#[cfg(unix)]
use exact_runner::file_pickers::Picker;
use std::path::PathBuf;

#[cfg(unix)]
pub(crate) mod wire;

/// How long the bus and the portal have to answer each call before the
/// chooser is up (the bus's own limit for starting a service is 25 s).
pub(crate) const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);

/// What a pick came to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    /// The paths the person chose.
    Chosen(Vec<PathBuf>),
    /// The person closed the chooser.
    Cancelled,
    /// No chooser answered, or the portal ended the request: why.
    Refused(String),
}

/// The portal's picks: those still wanted, and those answered for the
/// presenter to take on its next turn.
pub(crate) type Inbox = std::sync::Arc<std::sync::Mutex<Asks>>;

#[derive(Default)]
pub(crate) struct Asks {
    /// Each pick still wanted, by its number, and the element it is for. A
    /// pick whose element is made again (a renewed list row) leaves it, so
    /// its answer reaches no other element of that id.
    pub(crate) wanted: Vec<(u64, exact_kernel::ViewId)>,
    pub(crate) next: u64,
    /// Answered: the pick's number, the picker, and the outcome.
    pub(crate) answered: Vec<(u64, Kind, Outcome)>,
}

/// The chooser's title, as a browser on Linux titles its own for these
/// three calls (Chromium's).
pub(crate) fn title(kind: Kind) -> &'static str {
    match kind {
        Kind::Open => "Open File",
        Kind::Directory => "Select Folder",
        Kind::Save => "Save File",
    }
}

/// The portal method `picker` calls, and its `options` (`a{sv}`) under
/// `token`. No `filters`: the manifest's `file_handlers` are not in a
/// Linux binary, so the chooser offers every file, as macOS's does for an
/// app that declares no types.
#[cfg(unix)]
pub(crate) fn request(picker: &Picker, token: &str) -> (&'static str, wire::Value) {
    use wire::Value;
    let mut options = vec![("handle_token", Value::str(token))];
    let member = match picker.kind {
        Kind::Open => {
            options.push(("multiple", Value::Bool(picker.multiple)));
            "OpenFile"
        }
        Kind::Directory => {
            options.push(("directory", Value::Bool(true)));
            "OpenFile"
        }
        Kind::Save => {
            options.push(("current_name", Value::str(&picker.suggested_name)));
            "SaveFile"
        }
    };
    (member, Value::dict(options))
}

/// The Request object a call under `token` answers on, from the caller's
/// unique name (`org.freedesktop.portal.Request`: `:1.42` is `1_42`).
pub(crate) fn request_path(unique: &str, token: &str) -> String {
    let sender = unique.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
}

/// The outcome a `Response` signal's body (`u` code, `a{sv}` results)
/// carries: 0 is a choice, 1 the person's cancel, 2 the portal's own end.
#[cfg(unix)]
pub(crate) fn outcome(body: &[wire::Value]) -> Outcome {
    use wire::Value;
    match body {
        [Value::U32(0), results] => {
            let uris = match results.get("uris") {
                Some(Value::Array(_, uris)) => uris.as_slice(),
                _ => &[],
            };
            let paths = uris
                .iter()
                .filter_map(|u| match u {
                    Value::Str(u) => path_of(u),
                    _ => None,
                })
                .collect();
            Outcome::Chosen(paths)
        }
        [Value::U32(1), ..] => Outcome::Cancelled,
        [Value::U32(_), ..] => Outcome::Refused("the desktop portal ended the request".into()),
        _ => Outcome::Refused("the desktop portal's answer did not read".into()),
    }
}

/// A local `file:` URI's path, percent-decoded; None for any other URI (a
/// path must name a file this process can open).
pub(crate) fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    let bytes = unescape(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes).ok().map(PathBuf::from)
    }
}

/// `%xx` escapes decoded (RFC 3986 §2.1); None for a malformed one.
fn unescape(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hex = [bytes.next()?, bytes.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(b);
        }
    }
    Some(out)
}

/// Where a bus address's first `unix:` transport listens: a path, or a
/// name in the abstract namespace (`abstract=`, Linux only). The D-Bus
/// Specification, "Server Addresses".
#[derive(Debug, PartialEq)]
pub(crate) enum Socket {
    Path(PathBuf),
    Abstract(Vec<u8>),
}

pub(crate) fn socket_of(address: &str) -> Option<Socket> {
    address.split(';').find_map(|entry| {
        let keys = entry.strip_prefix("unix:")?;
        let key = |k: &str| {
            keys.split(',')
                .find_map(|kv| kv.strip_prefix(k)?.strip_prefix('='))
                .and_then(unescape)
        };
        #[cfg(unix)]
        if let Some(path) = key("path") {
            use std::os::unix::ffi::OsStringExt;
            return Some(Socket::Path(PathBuf::from(std::ffi::OsString::from_vec(
                path,
            ))));
        }
        key("abstract").map(Socket::Abstract)
    })
}

#[cfg(unix)]
mod bus;
#[cfg(unix)]
pub(crate) use bus::choose;

#[cfg(test)]
#[path = "portal_tests.rs"]
mod tests;
