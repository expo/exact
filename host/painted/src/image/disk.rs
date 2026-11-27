//! Remote image responses kept across launches (LLP 1011 §2, "The disk
//! cache"), as Apple's `URLCache` keeps them: a 200 its cache headers let a
//! cache store (RFC 9111, the executor's [`exact_executor::freshness_of`]),
//! shown from here while fresh or within its `stale-while-revalidate` window,
//! and validated with its `ETag` or `Last-Modified` once stale.
//!
//! One file per URL, named by the SHA-256 of the URL: a short text head (the
//! URL, when it was received, how long it took, the body's length and the
//! headers freshness reads), a blank line, then the body. It is written to a
//! temporary file and renamed over the old one, so a process killed midway
//! leaves the previous entry or none. Least recently used first out, to a byte
//! cap; a file's modification time is its last use, so the order outlives the
//! process. Every error here is a miss: the network answers instead.
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Apple's `URLCache` disk capacity for images (`RasterInput.swift`).
pub(super) const DISK_BYTES: u64 = 64 << 20;

/// The headers kept with a body: what freshness and validation read, and
/// nothing else (no `Set-Cookie` reaches the disk).
const KEPT: [&str; 7] = [
    "date",
    "age",
    "cache-control",
    "expires",
    "etag",
    "last-modified",
    "vary",
];

const VERSION: &str = "exact-image-cache 1";
/// The longest head read back; a longer one is never written.
const MAX_HEAD: usize = 16 << 10;

/// A stored response's head: its kept headers, when it was received (since
/// the epoch) and how long after it was asked for.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Head {
    pub headers: Vec<(String, String)>,
    pub received: Duration,
    pub delay: Duration,
}
impl Head {
    pub fn of(headers: &[(String, String)], received: SystemTime, delay: Duration) -> Self {
        Self {
            headers: (headers.iter())
                .filter(|(name, _)| KEPT.iter().any(|k| name.eq_ignore_ascii_case(k)))
                .map(|(name, value)| (name.to_ascii_lowercase(), value.clone()))
                .collect(),
            received: received.duration_since(UNIX_EPOCH).unwrap_or_default(),
            delay,
        }
    }
    /// This head freshened by a 304's headers (RFC 9111 §4.3.4): each kept
    /// header it carries replaces the stored ones. The stored `Date` and `Age`
    /// go either way, as they describe the old exchange's timing.
    pub fn refreshed(
        &self,
        headers: &[(String, String)],
        received: SystemTime,
        delay: Duration,
    ) -> Self {
        let fresh = Head::of(headers, received, delay);
        let mut kept: Vec<_> = (self.headers.iter())
            .filter(|(name, _)| {
                !matches!(name.as_str(), "date" | "age")
                    && !fresh.headers.iter().any(|(n, _)| n == name)
            })
            .cloned()
            .collect();
        kept.extend(fresh.headers);
        Self {
            headers: kept,
            ..fresh
        }
    }
    fn get(&self, name: &str) -> Option<&str> {
        (self.headers.iter())
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    /// The conditional request's headers.
    pub fn validators(&self) -> Vec<(&'static str, &str)> {
        let etag = self.get("etag").map(|v| ("if-none-match", v));
        let date = self.get("last-modified").map(|v| ("if-modified-since", v));
        etag.into_iter().chain(date).collect()
    }
    /// What a load may do with it at `now`.
    pub fn usable(&self, now: SystemTime) -> Use {
        let Some(f) = exact_executor::freshness_of(&self.headers, self.delay, self.received) else {
            return Use::Refetch;
        };
        let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let age = f.age.saturating_add(since.saturating_sub(self.received));
        if !f.validate && age < f.lifetime {
            Use::Fresh
        } else if !f.validate && age < f.lifetime.saturating_add(f.stale_while_revalidate) {
            Use::StaleWhileRevalidate
        } else if self.validators().is_empty() {
            Use::Refetch
        } else {
            Use::Validate
        }
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum Use {
    Fresh,
    /// Shown now and refreshed behind (RFC 5861).
    StaleWhileRevalidate,
    /// Asked for with its validators first.
    Validate,
    Refetch,
}

/// A stored entry, its file open at the body.
pub(super) struct Entry {
    pub head: Head,
    pub len: u64,
    file: File,
}
impl Entry {
    /// Its body copied to `out`.
    pub fn copy_to(&mut self, out: &mut File) -> std::io::Result<()> {
        let copied = std::io::copy(&mut (&mut self.file).take(self.len), out)?;
        if copied != self.len {
            return Err(std::io::Error::other("a short entry"));
        }
        Ok(())
    }
}

/// The file an entry is kept in, by its URL.
fn key(url: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(super) struct Disk {
    dir: PathBuf,
    cap: u64,
    /// The entries' sizes and last uses, read from the directory at the first
    /// store: no lookup waits for it.
    index: Mutex<Option<Index>>,
}
#[derive(Default)]
struct Index {
    entries: HashMap<String, (u64, SystemTime)>,
    bytes: u64,
}
impl Disk {
    /// Touches nothing on disk until it is first used.
    pub fn new(dir: PathBuf, cap: u64) -> Self {
        Self {
            dir,
            cap,
            index: Mutex::new(None),
        }
    }
    /// `url`'s entry, if one is stored and reads whole; marked as used.
    pub fn get(&self, url: &str) -> Option<Entry> {
        let name = key(url);
        let path = self.dir.join(&name);
        // Written too, for its modification time (Windows asks for write access).
        let mut file = File::options().read(true).write(true).open(&path).ok()?;
        let entry = read(&mut file, url).and_then(|(head, at, len)| {
            let whole = file.metadata().ok()?.len() == at + len;
            file.seek(SeekFrom::Start(at)).ok()?;
            whole.then_some((head, len))
        });
        let Some((head, len)) = entry else {
            drop(file);
            self.remove(url);
            return None;
        };
        let now = SystemTime::now();
        let _ = file.set_modified(now);
        if let Some(index) = self.index.lock().unwrap().as_mut() {
            if let Some((_, used)) = index.entries.get_mut(&name) {
                *used = now;
            }
        }
        Some(Entry { head, len, file })
    }
    /// Keep `body` (`len` bytes at the start of the file) as `url`'s answer,
    /// if its head lets it be stored and it is at most 1/20 of the cap, as
    /// `URLCache` keeps none larger (else the old answer goes); then the least
    /// recently used go to the cap.
    pub fn put(&self, url: &str, head: &Head, body: &Path, len: u64) {
        let storable = exact_executor::freshness_of(&head.headers, head.delay, head.received);
        if storable.is_none() || len > self.cap / 20 || url.contains(['\n', '\r']) {
            self.remove(url);
            return;
        }
        let mut text = format!(
            "{VERSION}\n{url}\n{} {} {len}\n",
            head.received.as_secs(),
            head.delay.as_millis()
        );
        for (name, value) in &head.headers {
            text.push_str(&format!("{name}: {value}\n"));
        }
        text.push('\n');
        if text.len() > MAX_HEAD {
            return;
        }
        let name = key(url);
        let Ok(tmp) = super::spool_name().map(|n| self.dir.join(format!("{name}.{n}.tmp"))) else {
            return;
        };
        let written = std::fs::create_dir_all(&self.dir).and_then(|()| {
            let mut out = super::spool_file(&tmp)?;
            out.write_all(text.as_bytes())?;
            let copied = std::io::copy(&mut File::open(body)?.take(len), &mut out)?;
            if copied != len {
                return Err(std::io::Error::other("a short body"));
            }
            drop(out);
            std::fs::rename(&tmp, self.dir.join(&name))
        });
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }
        let mut index = self.index.lock().unwrap();
        let index = index.get_or_insert_with(|| scan(&self.dir));
        let size = text.len() as u64 + len;
        if let Some((old, _)) = index.entries.insert(name, (size, SystemTime::now())) {
            index.bytes -= old;
        }
        index.bytes += size;
        while index.bytes > self.cap {
            let oldest = (index.entries.iter())
                .min_by_key(|(_, (_, used))| *used)
                .map(|(name, _)| name.clone())
                .expect("over the cap with no entry");
            let (size, _) = index.entries.remove(&oldest).expect("listed");
            index.bytes -= size;
            let _ = std::fs::remove_file(self.dir.join(oldest));
        }
    }
    /// Forget `url`'s entry: its body is not one this host decodes.
    pub fn remove(&self, url: &str) {
        let name = key(url);
        let _ = std::fs::remove_file(self.dir.join(&name));
        if let Some(index) = self.index.lock().unwrap().as_mut() {
            if let Some((size, _)) = index.entries.remove(&name) {
                index.bytes -= size;
            }
        }
    }
}

/// An entry's head, where its body starts and its length, if it is `url`'s.
fn read(file: &mut File, url: &str) -> Option<(Head, u64, u64)> {
    let mut start = Vec::new();
    file.take(MAX_HEAD as u64).read_to_end(&mut start).ok()?;
    let at = start.windows(2).position(|w| w == b"\n\n")? + 2;
    let text = std::str::from_utf8(&start[..at - 1]).ok()?;
    let mut lines = text.lines();
    if lines.next()? != VERSION || lines.next()? != url {
        return None;
    }
    let mut numbers = lines.next()?.split(' ').map(|n| n.parse::<u64>().ok());
    let (received, delay, len) = (numbers.next()??, numbers.next()??, numbers.next()??);
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(": ")?;
            Some((name.to_owned(), value.to_owned()))
        })
        .collect::<Option<_>>()?;
    let head = Head {
        headers,
        received: Duration::from_secs(received),
        delay: Duration::from_millis(delay),
    };
    Some((head, at as u64, len))
}

/// The entries in `dir`, and the temporary files a killed process left
/// removed (one younger than a minute may be another process's, writing).
fn scan(dir: &Path) -> Index {
    let mut index = Index::default();
    let stale = SystemTime::now() - Duration::from_secs(60);
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let (Ok(meta), Some(name)) = (entry.metadata(), entry.file_name().into_string().ok())
        else {
            continue;
        };
        let used = meta.modified().unwrap_or(UNIX_EPOCH);
        if name.ends_with(".tmp") {
            if used < stale {
                let _ = std::fs::remove_file(entry.path());
            }
        } else if name.len() == 64 && meta.is_file() {
            index.bytes += meta.len();
            index.entries.insert(name, (meta.len(), used));
        }
    }
    index
}

#[cfg(test)]
#[path = "disk_tests.rs"]
mod tests;
