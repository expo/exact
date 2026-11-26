//! What a drop over the window receives (LLP 1094.001 D8): the types it
//! reads of an offer, its items as their pipes bring them, and the paths,
//! link and text they hold. None of it needs Wayland, so its tests run on
//! every Unix.
#![allow(unsafe_code)]

use exact_painted::Dropped;
use std::ffi::OsString;
use std::io::{ErrorKind, PipeReader, Read};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The URI list a file manager offers its files as (RFC 2483).
pub(super) const URI_LIST: &str = "text/uri-list";
/// The link a browser offers with its title, as Firefox and Chromium do and
/// as GTK and Qt read it: UTF-16, the URL, a line break and the title.
pub(super) const MOZ_URL: &str = "text/x-moz-url";
/// The plain text types a drag may offer, the first offered taken: UTF-8
/// by name, then X11's name for it, then text in no named charset.
pub(super) const TEXTS: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
/// The document portal's file transfer key, then GTK's older name for it,
/// which a sandboxed receiver redeems for the files.
pub(super) const KEYS: [&str; 2] = [
    "application/vnd.portal.filetransfer",
    "application/vnd.portal.files",
];

/// A worker's answer, once it gives it.
pub(super) type Answer<T> = Arc<Mutex<Option<Vec<T>>>>;

/// Drop `id`'s items arriving through a pipe a type, in [`Wanted`]'s order,
/// each type at most `largest` bytes, and the portal's answer for its key
/// once it is asked: its paths, when it gave some.
pub(super) struct Reading {
    pub(super) id: u32,
    pub(super) pipes: [Option<PipeReader>; 4],
    pub(super) bytes: [Vec<u8>; 4],
    pub(super) largest: usize,
    pub(super) retrieved: Option<Answer<PathBuf>>,
}

/// Where each type is in a [`Reading`]: its URI list, its titled link, its
/// text and the portal's key.
pub(super) const URIS: usize = 0;
pub(super) const LINK: usize = 1;
pub(super) const TEXT: usize = 2;
pub(super) const KEY: usize = 3;

/// The types the window reads from an offer, each spelled as offered: its
/// URI list, its titled link, its text and, in a Flatpak sandbox, the
/// portal's file transfer key.
#[derive(Default, PartialEq, Debug)]
pub(super) struct Wanted {
    pub(super) uris: Option<String>,
    pub(super) link: Option<String>,
    pub(super) text: Option<String>,
    pub(super) key: Option<String>,
}

impl Reading {
    /// Whether every type has arrived or given up, and the portal answered
    /// for the key if it was asked.
    pub(super) fn complete(&self) -> bool {
        self.pipes.iter().all(Option::is_none)
            && self
                .retrieved
                .as_ref()
                .is_none_or(|r| r.lock().unwrap_or_else(|e| e.into_inner()).is_some())
    }

    /// Whether the text or the titled link is still arriving, which a
    /// deadline bounds.
    pub(super) fn timed(&self) -> bool {
        self.pipes[LINK].is_some() || self.pipes[TEXT].is_some()
    }

    /// Stop waiting for the text and the titled link, past their deadline:
    /// what they had not finished sending is not delivered.
    pub(super) fn time_out(&mut self) {
        for k in [LINK, TEXT] {
            if self.pipes[k].take().is_some() {
                self.bytes[k].clear();
            }
        }
    }

    /// Stop waiting for every type and for the portal, another drop having
    /// come: the drop lands with what came, which keeps an answer the portal
    /// already gave, and a key not yet redeemed is not redeemed.
    pub(super) fn cut(&mut self) {
        for k in [URIS, LINK, TEXT, KEY] {
            if self.pipes[k].take().is_some() {
                self.bytes[k].clear();
            }
        }
        self.bytes[KEY].clear();
        self.retrieved = self
            .retrieved
            .take()
            .filter(|r| r.lock().unwrap_or_else(|e| e.into_inner()).is_some());
    }

    /// The key the portal's type sent once it has all come: as GTK writes
    /// it, with a terminal NUL, or as KDE does, without; none when it holds
    /// a NUL inside or is not UTF-8.
    pub(super) fn key(&self) -> Option<&str> {
        if self.pipes[KEY].is_some() {
            return None;
        }
        let bytes = &self.bytes[KEY];
        let bytes = bytes.strip_suffix(b"\0").unwrap_or(bytes);
        std::str::from_utf8(bytes)
            .ok()
            .filter(|key| !key.is_empty() && !key.contains('\0'))
    }

    /// The drop's number and items, and whether its files are the portal's:
    /// a type still arriving gives nothing, and the portal's paths for the
    /// key, when it gave some, are the files.
    pub(super) fn items(self) -> (u32, Dropped, bool) {
        let Reading {
            id,
            pipes,
            mut bytes,
            retrieved,
            ..
        } = self;
        for (pipe, bytes) in pipes.iter().zip(&mut bytes) {
            if pipe.is_some() {
                bytes.clear();
            }
        }
        let [uris, link, text, _] = bytes;
        let mut dropped = items(&uris, &link, &text);
        let given = retrieved.and_then(|r| r.lock().unwrap_or_else(|e| e.into_inner()).take());
        let given = given.filter(|paths| !paths.is_empty());
        let portal = given.is_some();
        if let Some(paths) = given {
            dropped.paths = paths;
        }
        (id, dropped, portal)
    }

    /// Read some of what type `k`'s pipe holds, a bounded batch so other
    /// pipes and the display are heard between: its end closes the pipe, and
    /// an error or more than `largest` bytes closes it with nothing, as does
    /// an end of the text or the titled link after `deadline`.
    pub(super) fn read(&mut self, k: usize, deadline: Instant) {
        let Some(pipe) = self.pipes[k].as_mut() else {
            return;
        };
        let mut chunk = [0; 1 << 16];
        for _ in 0..16 {
            match pipe.read(&mut chunk) {
                Ok(0) => {
                    self.pipes[k] = None;
                    if [LINK, TEXT].contains(&k) && Instant::now() >= deadline {
                        self.bytes[k].clear();
                    }
                    return;
                }
                Ok(n) if self.bytes[k].len() + n <= self.largest => {
                    self.bytes[k].extend_from_slice(&chunk[..n])
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                _ => {
                    self.pipes[k] = None;
                    self.bytes[k].clear();
                    return;
                }
            }
        }
    }
}

/// A `poll` entry waiting on `fd` for `events`.
pub(super) fn poll_for(fd: i32, events: libc::c_short) -> libc::pollfd {
    libc::pollfd {
        fd,
        events,
        revents: 0,
    }
}

/// Make `pipe`'s end not block (`O_NONBLOCK`); whether it took.
pub(super) fn nonblocking(pipe: &impl AsRawFd) -> bool {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0
}

impl Wanted {
    /// What an offer of `types` gives the window, each type as offered:
    /// its URI list, its titled link, the first of its text types and, in a
    /// sandbox, the first of the portal's key types; none when it offers none
    /// of them.
    pub(super) fn of(types: &[String], sandboxed: bool) -> Option<Self> {
        let find = |t: &str| types.iter().find(|offered| same_type(offered, t)).cloned();
        let wanted = Wanted {
            uris: find(URI_LIST),
            link: find(MOZ_URL),
            text: TEXTS.into_iter().find_map(find),
            key: KEYS.into_iter().find_map(find).filter(|_| sandboxed),
        };
        (wanted != Wanted::default()).then_some(wanted)
    }

    /// The type the window accepts while a node under the drag takes the
    /// drop.
    pub(super) fn first(&self) -> Option<&str> {
        [&self.uris, &self.link, &self.text, &self.key]
            .into_iter()
            .flatten()
            .next()
            .map(String::as_str)
    }
}

/// Whether MIME type `offered` is `wanted`, as the forms the window reads
/// are spelled: the type, subtype and each parameter's name and value alike
/// but for case, with or without spaces around a `;` and an `=`, and a value
/// quoted or not, so `text/plain; charset="UTF-8"` is
/// `text/plain;charset=utf-8`.
fn same_type(offered: &str, wanted: &str) -> bool {
    fn parts(mime: &str) -> impl Iterator<Item = (&str, Option<&str>)> {
        mime.split(';').map(|part| match part.split_once('=') {
            Some((name, value)) => {
                let value = value.trim();
                let value = value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .unwrap_or(value);
                (name.trim(), Some(value))
            }
            None => (part.trim(), None),
        })
    }
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let alike = |(a, x): (&str, Option<&str>), (b, y): (&str, Option<&str>)| {
        same(a, b)
            && match (x, y) {
                (Some(x), Some(y)) => same(x, y),
                (x, y) => x.is_none() && y.is_none(),
            }
    };
    let (mut offered, mut wanted) = (parts(offered), parts(wanted));
    loop {
        match (offered.next(), wanted.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) if alike(a, b) => {}
            _ => return false,
        }
    }
}

/// The items a drop's URI list (RFC 2483), titled link and text give: each
/// local `file:` URI a path, the first other URI the link, else the titled
/// link's URL, and the text as it came.
pub(super) fn items(uris: &[u8], link: &[u8], text: &[u8]) -> Dropped {
    let mut dropped = Dropped {
        text: String::from_utf8_lossy(text).into_owned(),
        ..Default::default()
    };
    let lines = uris.split(|&b| b == b'\n').map(<[u8]>::trim_ascii);
    for line in lines.filter(|l| !l.is_empty() && !l.starts_with(b"#")) {
        if line.len() >= 5 && line[..5].eq_ignore_ascii_case(b"file:") {
            dropped.paths.extend(file_path(&line[5..]));
        } else if dropped.url.is_empty() {
            if let Some(url) = std::str::from_utf8(line).ok().and_then(web_link) {
                dropped.url = url.to_owned();
            }
        }
    }
    if dropped.url.is_empty() {
        let titled = utf16_or_utf8(link).unwrap_or_default();
        if let Some(url) = titled.lines().next().and_then(web_link) {
            dropped.url = url.to_owned();
        }
    }
    dropped
}

/// `text`, its ASCII spaces and controls trimmed, as a link that is not a
/// file: a line with a URI scheme (RFC 3986: a letter, then letters, digits,
/// `+`, `-` or `.`) and no ASCII space or control inside, the bytes D1's URL
/// test refuses, so a URL D1 admits arrives whole. The app checks the URL
/// before following it (D1), as GTK's URI-list reader and Android's leave
/// its parts.
pub(super) fn web_link(text: &str) -> Option<&str> {
    let blank = |c: char| c <= ' ' || c == '\u{7f}';
    let text = text.trim_matches(blank);
    let (scheme, _) = text.split_once(':')?;
    let mut letters = scheme.chars();
    let valid = letters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        && !text.chars().any(blank);
    (valid && !scheme.eq_ignore_ascii_case("file")).then_some(text)
}

/// Text a browser wrote as UTF-16, as Mozilla's types are: by its byte
/// order mark, else by where an ASCII character's zero byte falls; else
/// UTF-8, without the NUL a C string ends with. None for UTF-16 that ends
/// partway through a code unit, which its source cut short.
pub(super) fn utf16_or_utf8(bytes: &[u8]) -> Option<String> {
    let (big, units) = match bytes {
        [0xFE, 0xFF, rest @ ..] => (true, rest),
        [0xFF, 0xFE, rest @ ..] => (false, rest),
        [first, 0, ..] if *first != 0 => (false, bytes),
        [0, second, ..] if *second != 0 => (true, bytes),
        _ => {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            return Some(String::from_utf8_lossy(&bytes[..end]).into_owned());
        }
    };
    let (units, []) = units.as_chunks::<2>() else {
        return None;
    };
    let units = units.iter().map(|&pair| {
        if big {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    let text = char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .take_while(|&c| c != '\0')
        .collect();
    Some(text)
}

/// The local path a `file:` URI names (RFC 8089), after its scheme: none
/// for another machine's file, or for a URI with a query, a fragment, a `%`
/// not followed by two hex digits, or an escaped `/` or NUL, which name no
/// file's name. Its percent-decoded bytes are kept as they are, UTF-8 or
/// not.
pub(super) fn file_path(rest: &[u8]) -> Option<PathBuf> {
    let path = match rest.strip_prefix(b"//") {
        Some(authority) => {
            let at = authority.iter().position(|&b| b == b'/')?;
            let host = &authority[..at];
            (host.is_empty() || host.eq_ignore_ascii_case(b"localhost"))
                .then_some(&authority[at..])?
        }
        None => rest.starts_with(b"/").then_some(rest)?,
    };
    if path.iter().any(|b| matches!(b, b'?' | b'#')) {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let mut at = path.iter();
    while let Some(&b) = at.next() {
        if b == b'%' {
            let hex = [*at.next()?, *at.next()?];
            let digit = |h: u8| (h as char).to_digit(16);
            let byte = (digit(hex[0])? * 16 + digit(hex[1])?) as u8;
            // An escaped separator or NUL names no file, as GLib's `g_filename_from_uri` refuses them
            if matches!(byte, b'/' | 0) {
                return None;
            }
            bytes.push(byte);
        } else {
            bytes.push(b);
        }
    }
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(test)]
#[path = "drop_received_tests.rs"]
pub(super) mod tests;
