//! A request body read from an app file (LLP 1108 D6 R2, as LLP
//! 1133.008.000 D4 amends it): `fetch(url, {exactBodyFrom: "app:/tmp/…"})`
//! from TypeScript, `Request::body_from` from Rust. The worker that runs the
//! request opens the file just before it goes out, under the same `fs.read`
//! grant and app-directory handles a source's `readFile` uses. Any refusal
//! (no app files here, a denied path, a missing file, a folder) is the
//! request's failure, and nothing is sent. The bytes never cross the source.
//!
//! Memory: the body is streamed from the opened descriptor, never held
//! whole, whatever its length. Each attempt's reader thread reads it by
//! position in [`CHUNK`]s and hands them over a rendezvous channel, so a
//! body holds at most two chunks and its transport's buffers
//! ([`READ_BOUND`]). Its length is the descriptor's when opened: a file that
//! grows sends that much, one that shrinks fails the exchange, named.
//!
//! Places: a file body holds one of the process's [`MAX_READERS`] reader
//! places from its open until its last reader thread ends, through
//! retries, the request's end, and a read stuck on a stalled disk. So the
//! threads and buffers file bodies hold are bounded, stuck or not. A plain
//! request that finds no place is refused; an upload waits for one.
//!
//! Time: root creation and opening share the request's deadline and abort,
//! as the reads do: the worker watches them while the opener runs, and a
//! reader waiting on the disk never holds the transport past them.
use exact_runner::{FailureKind, Outcome, Request};
use ibex2::stdlib::{abort::AbortSignal, app_fs::AppDirectories, fetch::BodyFile};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex, Weak},
};

/// The app's `app:/data`, `app:/cache` and `app:/tmp` handles, opened once,
/// at the first request whose body is an app file (or why they would not
/// open); `None` on a host or drive that has no app files.
pub(super) type Roots<'a> = Option<&'a Result<AppDirectories, String>>;

/// Open the app's directories, made as storage makes them, so their handles
/// pin the roots from now on: a root renamed or replaced later (by a
/// symlink to another app's tree, say) is not followed, as `readFile`'s
/// handles do not follow it. A root that is a symbolic link when first
/// opened is refused for the same reason.
pub(super) fn open(
    roots: &[PathBuf; 3],
    #[cfg(test)] checked: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
) -> Result<AppDirectories, String> {
    // Reject existing links before creating any root. AppDirectories also
    // refuses links introduced after this check.
    let link = |root: &&PathBuf| std::fs::symlink_metadata(root).is_ok_and(|m| m.is_symlink());
    if let Some(root) = roots.iter().find(link) {
        return Err(format!(
            "app storage: {} is a symbolic link",
            root.display()
        ));
    }
    #[cfg(test)]
    if let Some(checked) = checked {
        checked();
    }
    for root in roots {
        std::fs::create_dir_all(root).map_err(|e| format!("app storage: {e}"))?;
    }
    AppDirectories::new(&roots[0], &roots[1], &roots[2]).map_err(|e| e.to_string())
}

/// Acquire the lazy roots on a bounded reader, so a stalled first open or
/// a wait for another opener cannot hold a request past its deadline or abort.
/// An upload opens them in the reader place it already holds (`place`), so
/// uploads holding every place never wait on one another for a second.
pub(super) fn prepare(
    shared: &Arc<super::Shared>,
    request: &Request,
    stop: &dyn Fn() -> Option<Outcome>,
    place: Option<&Place>,
) -> Result<Option<Result<AppDirectories, String>>, Outcome> {
    if request
        .body_from
        .as_ref()
        .or(request.body_to.as_ref())
        .is_none()
        || shared.root_paths.get().is_none()
    {
        return Ok(None);
    }
    if let Some(roots) = shared.roots.get() {
        return stop().map_or_else(|| Ok(Some(roots.clone())), Err);
    }
    let shared = shared.clone();
    let open = move |_: &AtomicBool, _: &Place| Ok(shared.app_roots().cloned());
    match place {
        Some(place) => acquire_with(place.clone(), open, stop),
        None => acquire(&READERS, open, stop),
    }
}

/// Open `request.body_from` under `grants` (the app's, narrowed by the
/// request's own scope) as the body the transport streams; `None` for a
/// request without one. `stop` says when the request has ended meanwhile
/// (its deadline passed, it was aborted): the open runs on a thread of its
/// own, so the worker settles the request then without waiting for a
/// stalled one. `place` is one an upload already took ([`take_place`]);
/// otherwise one is taken here, or the request refused. `signal` ends a
/// reader waiting on the disk when the request ends.
pub(super) fn resolve(
    roots: Roots<'_>,
    grants: &str,
    request: &mut Request,
    stop: &dyn Fn() -> Option<Outcome>,
    place: Option<Place>,
    signal: AbortSignal,
) -> Result<Option<BodyFile>, Outcome> {
    let Some(path) = request.body_from.clone() else {
        return Ok(None);
    };
    if let Some(why) = request.body_from_refusal() {
        return Err(refused(why.to_string()));
    }
    request.body_from = None;
    let (roots, grants, scope) = (roots.cloned(), grants.to_string(), request.grants.clone());
    let open = move |_: &AtomicBool, place: &Place| {
        let (file, len) = open_file(roots.as_ref(), &grants, scope.as_deref(), &path)?;
        let opened = Opened {
            file: Arc::new(file),
            len,
            path,
            signal,
            place: place.clone(),
        };
        Ok(BodyFile(Arc::new(opened)))
    };
    let body = match place {
        Some(place) => acquire_with(place, open, stop),
        None => acquire(&READERS, open, stop),
    }?;
    Ok(Some(body))
}

/// The file at `path` opened for reading, and its length as that
/// descriptor has it.
fn open_file(
    roots: Roots<'_>,
    grants: &str,
    scope: Option<&str>,
    path: &str,
) -> Result<(std::fs::File, u64), Outcome> {
    size(roots, grants, scope, path)?;
    let (grants, directories) = open_for(roots, grants, scope, path)?;
    let file = directories
        .open_file(&grants, path)
        .map_err(|e| named(path, e))?;
    let meta = file
        .metadata()
        .map_err(|e| refused(format!("exactBodyFrom {path}: filesystem: {e}")))?;
    if !meta.is_file() {
        return Err(refused(format!("exactBodyFrom {path}: not a file")));
    }
    Ok((file, meta.len()))
}

/// What one step of a read takes, and what one reader hands over at a time.
pub(super) const CHUNK: usize = 256 << 10;
/// What a file body holds at most: the chunk its reader fills, the one the
/// transport drains, and Apple's bound stream pair and its copy, 64 KiB each
/// (ibex2's `darwin_http.mm`).
pub(super) const READ_BOUND: usize = 2 * CHUNK + (128 << 10);

/// A file body opened for a request: each attempt's reader reads `file`
/// from its first byte to `len`.
struct Opened {
    file: Arc<std::fs::File>,
    len: u64,
    path: String,
    signal: AbortSignal,
    place: Place,
}
impl ibex2::stdlib::fetch::FileBody for Opened {
    fn len(&self) -> u64 {
        self.len
    }
    fn reader(&self) -> std::io::Result<Box<dyn std::io::Read + Send>> {
        let (give, chunks) = std::sync::mpsc::sync_channel(0);
        let cancel = Arc::new(AtomicBool::new(false));
        let (file, len, path, told, place) = (
            self.file.clone(),
            self.len,
            self.path.clone(),
            cancel.clone(),
            self.place.clone(),
        );
        std::thread::Builder::new()
            .name("exact-body-from".into())
            .spawn(move || {
                let _place = place;
                pump(&file, len, &path, &told, &give);
            })?;
        Ok(Box::new(Pumped {
            chunks,
            chunk: Vec::new(),
            at: 0,
            cancel,
            signal: self.signal.clone(),
            path: self.path.clone(),
        }))
    }
}

/// Read `file` by position into chunks, each handed over once taken, until
/// `len`, `cancel`, or the reader's end; a read that fails, or a file that
/// ends before `len`, is handed over as the reader's error.
fn pump(
    file: &std::fs::File,
    len: u64,
    path: &str,
    cancel: &AtomicBool,
    give: &std::sync::mpsc::SyncSender<std::io::Result<Vec<u8>>>,
) {
    use std::sync::atomic::Ordering;
    let mut at = 0_u64;
    while at < len && !cancel.load(Ordering::Acquire) {
        let mut chunk = vec![0; (len - at).min(CHUNK as u64) as usize];
        let mut filled = 0;
        let read = loop {
            if filled == chunk.len() {
                break Ok(());
            }
            match read_at(file, &mut chunk[filled..], at + filled as u64) {
                Ok(0) => {
                    break Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        format!("exactBodyFrom {path}: the file shrank while it was sent"),
                    ))
                }
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    break Err(std::io::Error::new(
                        e.kind(),
                        format!("exactBodyFrom {path}: filesystem: {e}"),
                    ))
                }
            }
        };
        at += filled as u64;
        let last = read.is_err();
        if give.send(read.map(|()| chunk)).is_err() || last {
            return;
        }
    }
}

#[cfg(unix)]
pub(super) fn read_at(file: &std::fs::File, out: &mut [u8], at: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, out, at)
}
#[cfg(windows)]
pub(super) fn read_at(file: &std::fs::File, out: &mut [u8], at: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, out, at)
}

/// One attempt's body as the transport reads it: the chunks its reader
/// hands over, waited for with the request's abort signal, which its
/// deadline also trips. Dropped, it tells the reader to stop.
/// How long a body's transport waits on the disk for its next chunk: a
/// stalled disk fails the request even where it names no deadline, as the
/// network's 60 s without a byte written does (LLP 1133.008.000 D4).
const IDLE_READ: std::time::Duration = std::time::Duration::from_secs(60);

struct Pumped {
    chunks: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
    chunk: Vec<u8>,
    at: usize,
    cancel: Arc<AtomicBool>,
    signal: AbortSignal,
    path: String,
}
impl std::io::Read for Pumped {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        use std::sync::mpsc::RecvTimeoutError;
        let asked = std::time::Instant::now();
        while self.at == self.chunk.len() {
            match self
                .chunks
                .recv_timeout(std::time::Duration::from_millis(5))
            {
                Ok(chunk) => (self.chunk, self.at) = (chunk?, 0),
                Err(RecvTimeoutError::Timeout) if self.signal.aborted() => {
                    return Err(std::io::Error::other(format!(
                        "exactBodyFrom {}: the request ended",
                        self.path
                    )))
                }
                Err(RecvTimeoutError::Timeout) if asked.elapsed() >= IDLE_READ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!(
                            "exactBodyFrom {}: the file gave no bytes for {} s",
                            self.path,
                            IDLE_READ.as_secs()
                        ),
                    ))
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = out.len().min(self.chunk.len() - self.at);
        out[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}
impl Drop for Pumped {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

struct ReadersState {
    running: usize,
    /// Executors with an upload waiting for a place, started when one is
    /// given back.
    waiting: Vec<Weak<super::Shared>>,
}

/// The process's reader places in use: by a file body from its open until
/// its last reader thread ends (whether or not its request is still
/// waiting), by a root's first opening, and by the asynchronous executor's
/// reads (LLP 1120 D4). Past [`MAX_READERS`] (a stalled disk, or file
/// bodies going out) a new file body is refused, or an upload waits.
pub(super) struct Readers {
    state: Mutex<ReadersState>,
    #[cfg(test)]
    pub(super) replied: std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}
impl Readers {
    pub(super) const fn new() -> Readers {
        Readers {
            state: Mutex::new(ReadersState {
                running: 0,
                waiting: Vec::new(),
            }),
            #[cfg(test)]
            replied: std::sync::Mutex::new(None),
        }
    }
    #[cfg(test)]
    pub(super) fn running(&self) -> usize {
        self.state.lock().unwrap().running
    }
    /// A place, if one is free.
    fn take(&'static self) -> Option<Place> {
        let mut state = self.state.lock().unwrap();
        if state.running >= MAX_READERS {
            return None;
        }
        state.running += 1;
        Some(Place(Arc::new(Reader(self))))
    }
    /// A place if one is free, or record `shared` to start when one is given
    /// back. Both check and registration share `self.state`, preventing a
    /// lost wakeup against [`Reader::drop`].
    fn take_or_wait(&'static self, shared: &Arc<super::Shared>) -> Option<Place> {
        let mut state = self.state.lock().unwrap();
        if state.running < MAX_READERS {
            state.running += 1;
            return Some(Place(Arc::new(Reader(self))));
        }
        state.waiting.retain(|w| w.strong_count() > 0);
        if !state
            .waiting
            .iter()
            .any(|w| w.as_ptr() == Arc::as_ptr(shared))
        {
            state.waiting.push(Arc::downgrade(shared));
        }
        None
    }
}
pub(super) static READERS: Readers = Readers::new();
/// Waits until the places of earlier tests' bodies, whose last holders may
/// still be ending on their own threads, are all given back.
#[cfg(test)]
pub(super) fn settle_readers() {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while READERS.running() > 0 && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(READERS.running(), 0, "a reader place was never given back");
}
/// The most reader places at once, across this process's executors.
pub(super) const MAX_READERS: usize = 4;

/// A reader place, shared by a file body and its reader threads, given back
/// when the last of them lets it go.
#[derive(Clone)]
pub(super) struct Place(Arc<Reader>);

/// A free place for an upload `shared` is about to start, or `None`, in
/// which case `shared` is started again when a place is given back.
pub(super) fn take_place(shared: &Arc<super::Shared>) -> Option<Place> {
    READERS.take_or_wait(shared)
}

/// Releases a place when its last holder ends, and gives the executors
/// whose uploads wait for one their chance at it.
struct Reader(&'static Readers);
impl Drop for Reader {
    fn drop(&mut self) {
        let waiting = {
            let mut state = self.0.state.lock().unwrap();
            state.running -= 1;
            std::mem::take(&mut state.waiting)
        };
        for shared in waiting.iter().filter_map(Weak::upgrade) {
            super::stream::upload::start(&shared);
        }
    }
}

/// Run `read` off this thread with a place of its own, and wait for it, or
/// for `stop` to end the request first. A read that loses is told
/// (`cancel`, which it checks between chunks), and its result dropped; the
/// place stays taken until its I/O ends, or longer if `read` keeps it.
pub(super) fn acquire<T: Send + 'static>(
    readers: &'static Readers,
    read: impl FnOnce(&AtomicBool, &Place) -> Result<T, Outcome> + Send + 'static,
    stop: &dyn Fn() -> Option<Outcome>,
) -> Result<T, Outcome> {
    if let Some(ended) = stop() {
        return Err(ended);
    }
    let Some(place) = readers.take() else {
        return Err(refused(format!(
            "exactBodyFrom: {MAX_READERS} file reads are still running (a stalled disk?); nothing was sent"
        )));
    };
    read_in(place, read, stop)
}

/// [`acquire`], with a place already taken.
pub(super) fn acquire_with<T: Send + 'static>(
    place: Place,
    read: impl FnOnce(&AtomicBool, &Place) -> Result<T, Outcome> + Send + 'static,
    stop: &dyn Fn() -> Option<Outcome>,
) -> Result<T, Outcome> {
    if let Some(ended) = stop() {
        return Err(ended);
    }
    read_in(place, read, stop)
}

fn read_in<T: Send + 'static>(
    place: Place,
    read: impl FnOnce(&AtomicBool, &Place) -> Result<T, Outcome> + Send + 'static,
    stop: &dyn Fn() -> Option<Outcome>,
) -> Result<T, Outcome> {
    use std::sync::atomic::Ordering;
    let readers = place.0 .0;
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let told = cancel.clone();
    let (done, result) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("exact-body-from".into())
        .spawn(move || {
            let result = read(&told, &place);
            drop(place);
            let _ = done.send(result);
            #[cfg(test)]
            {
                let hook = readers.replied.lock().unwrap().clone();
                if let Some(hook) = hook {
                    hook();
                }
            }
            #[cfg(not(test))]
            let _ = readers;
        })
        .map_err(|e| refused(format!("exactBodyFrom: the read could not start: {e}")))?;
    let ended = |outcome| {
        cancel.store(true, Ordering::Release);
        Err(outcome)
    };
    loop {
        match result.recv_timeout(std::time::Duration::from_millis(5)) {
            Ok(read) => return stop().map_or(read, ended),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Some(outcome) = stop() {
                    return ended(outcome);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(refused("exactBodyFrom: the read failed".into()));
            }
        }
    }
}

fn refused(message: String) -> Outcome {
    Outcome::Failed {
        kind: FailureKind::Refused,
        message,
    }
}

/// The admitted `grants` and the opened directories for a read of `path`.
fn open_for<'a>(
    roots: Roots<'a>,
    grants: &str,
    scope: Option<&str>,
    path: &str,
) -> Result<(ibex2::grant::GrantSet, &'a AppDirectories), Outcome> {
    open_as("exactBodyFrom", roots, grants, scope, path)
}

/// [`open_for`], its refusals named by the option `what` (`exactBodyTo`'s too).
pub(super) fn open_as<'a>(
    what: &str,
    roots: Roots<'a>,
    grants: &str,
    scope: Option<&str>,
    path: &str,
) -> Result<(ibex2::grant::GrantSet, &'a AppDirectories), Outcome> {
    let Some(directories) = roots else {
        return Err(Outcome::Failed {
            kind: FailureKind::Unsupported,
            message: format!("{what} {path}: this host has no app files (no storage here)"),
        });
    };
    let admitted = exact_data::storage::scope(grants, scope)
        .map_err(|e| refused(format!("{what} {path}: {e}")))?;
    let grants = ibex2::grant::GrantSet::parse(&exact_runner::io_grants(admitted))
        .map_err(|e| refused(format!("the app's grants did not parse: {e}")))?;
    let directories = directories.as_ref().map_err(|e| {
        refused(format!(
            "{what} {path}: the app's files would not open: {e}"
        ))
    })?;
    Ok((grants, directories))
}

fn named(path: &str, e: ibex2::boundary::HostError) -> Outcome {
    named_as("exactBodyFrom", path, e)
}

/// A filesystem refusal of `path`, named by the option `what`.
pub(super) fn named_as(what: &str, path: &str, e: ibex2::boundary::HostError) -> Outcome {
    match e {
        // As `readFile`'s denial reads (js/src/storage.js `deniedPath`).
        ibex2::boundary::HostError::Denied { capability } => refused(format!(
            "{what} {path}: denied: {capability} {path}: no grant covers it; grant `{capability} {path}`"
        )),
        e => refused(format!("{what} {path}: {e}")),
    }
}

/// The file's size: `stat` admits the read and names a missing file or a
/// folder before anything is read.
pub(super) fn size(
    roots: Roots<'_>,
    grants: &str,
    scope: Option<&str>,
    path: &str,
) -> Result<u64, Outcome> {
    use ibex2::stdlib::fs::{FsOp, FsResult};
    let (grants, directories) = open_for(roots, grants, scope, path)?;
    match ibex2::stdlib::fs::run(&grants, Some(directories), FsOp::Stat, path, None, None)
        .map_err(|e| named(path, e))?
    {
        FsResult::Stat(stat) if !stat.is_file => {
            Err(refused(format!("exactBodyFrom {path}: not a file")))
        }
        FsResult::Stat(stat) => Ok(stat.size),
        _ => Err(refused(format!("exactBodyFrom {path}: not a file"))),
    }
}

/// At most `cap` bytes of the file, in chunks through the opened
/// descriptor, stopping between chunks once the request has ended. `Err(None)`
/// when the file holds more than `cap` (it grew, or was replaced). `exact`:
/// `cap` is the stat's size, which the async core charged (LLP 1120 D5), and
/// the buffer is allocated once at that size and one byte, never more.
pub(super) fn read_at_most(
    roots: Roots<'_>,
    grants: &str,
    scope: Option<&str>,
    path: &str,
    cap: u64,
    exact: bool,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, Option<Outcome>> {
    let (grants, directories) = open_for(roots, grants, scope, path).map_err(Some)?;
    let bytes = {
        use std::io::Read;
        let mut file = directories
            .open_file(&grants, path)
            .map_err(|e| Some(named(path, e)))?
            .take(cap + 1);
        let mut bytes = if exact {
            Vec::with_capacity(cap as usize + 1)
        } else {
            Vec::new()
        };
        loop {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err(Some(refused(format!(
                    "exactBodyFrom {path}: the request ended"
                ))));
            }
            let at = bytes.len();
            let end = if exact {
                (at + CHUNK).min(cap as usize + 1)
            } else {
                at + CHUNK
            };
            bytes.resize(end, 0);
            let n = loop {
                match file.read(&mut bytes[at..]) {
                    Ok(n) => break n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        return Err(Some(refused(format!(
                            "exactBodyFrom {path}: filesystem: {e}"
                        ))))
                    }
                }
            };
            bytes.truncate(at + n);
            if n == 0 {
                break bytes;
            }
        }
    };
    if bytes.len() as u64 > cap {
        return Err(None);
    }
    Ok(bytes)
}

/// Run `job` on a reader thread of its own, holding one of the process's
/// [`MAX_READERS`] places until it ends; refused, as today's fifth reader
/// is, when none is free or no thread can be made (LLP 1120 D4).
pub(super) fn reader(
    job: impl FnOnce() + Send + 'static,
) -> Result<std::thread::JoinHandle<()>, Outcome> {
    let Some(place) = READERS.take() else {
        return Err(refused(format!(
            "exactBodyFrom: {MAX_READERS} file reads are still running (a stalled disk?); nothing was sent"
        )));
    };
    std::thread::Builder::new()
        .name("exact-body-from".into())
        .spawn(move || {
            let _place = place;
            job();
        })
        .map_err(|e| refused(format!("exactBodyFrom: the read could not start: {e}")))
}
