//! The server (LLP 1048.000 D10, D11): `<app>-render --serve <dist>`
//! renders a page per request, on loopback, from the built web app.
//!
//! A route declared `render=build`, `cached` or `request`, and any location
//! the router sends to its not-found route, is rendered with a new
//! runner and module realm (D10), then composed over the
//! built shell ([`crate::page`]). A `client` route gets the shell. Files
//! in the public part of `dist/` are served as they are; `/.exact/health` answers `ok`.
//! Renders run on a fixed set of workers behind a bounded queue: a request
//! that finds the queue full is a 503 at once. A failed render, or one that
//! panics, is a 500 and the server keeps serving. Each render prints one
//! line: location, status, time, answers, pending and bytes.
//! Successful `cached` pages are also kept at the origin for their public
//! lifetime, bounded to 64 locations and 32 MiB. Other routes render fresh.

use crate::encode::{self, Accepts, Variants};
use crate::files::{asset_shaped, percent_decode};
use crate::{page, render_with_at, Ids, Projection, Rendered};
use exact_plan::{Plan, RenderPolicy};
use exact_runner::DataSource;
use exact_web::document::{canonical_location, route_at, Site};
use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::io::{Read, Seek, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[path = "serve_files.rs"]
mod files;

#[path = "deferred.rs"]
pub(crate) mod deferred;
use deferred::{hex, Checkpoints};

/// The largest page the server sends (D10's bound on output bytes).
pub(crate) const MAX_PAGE: usize = 16 << 20;
const MAX_CACHED_PAGES: usize = 64;
const MAX_CACHE_BYTES: usize = 32 << 20;

/// How the server runs.
pub struct Serve {
    /// The built web app: its files, its shell (`shell.html`, or
    /// `index.html` when nothing renders at build) and its plan (`app.plan`).
    pub dist: PathBuf,
    /// The loopback port; 0 picks one.
    pub port: u16,
    /// The app's name: the title of a page whose head sets none.
    pub name: String,
    /// The origin canonical URLs are built from — never a request's `Host`.
    pub origin: Option<String>,
    /// Each render's deadline.
    pub deadline: Duration,
    /// Renders at once.
    pub renders: usize,
    /// Requests that may wait for a render before the server answers 503.
    pub queue: usize,
    /// The page viewport documents render at (D4).
    pub viewport: exact_runner::Viewport,
    /// A rendered route's shared-cache lifetime (`s-maxage`), until routes
    /// declare their own.
    pub lifetime: Duration,
    /// Retained builds for versioned URLs and delta dictionaries
    /// (LLP 1047.000 §9, LLP 1048.000 D10).
    pub generations: Option<PathBuf>,
}

/// A bound server, not yet serving.
pub struct Server {
    listener: TcpListener,
    shared: Shared,
    stop: Arc<AtomicBool>,
}

/// Drains a running server (D10): it stops accepting, answers what it has
/// taken — the renders in flight included — and [`Server::run`] returns.
#[derive(Clone)]
pub struct Stopper(Arc<AtomicBool>, SocketAddr);

/// Moves every armed render deadline to now, for the deadline thread to fire
/// as it fires one that passes (LLP 1048.000 D10): the source call each
/// render is running, or its next, is stopped, and the render ends as one
/// stopped at its deadline does. For a test that ends a render once it has
/// seen what it waits for, not on the wall clock (docs/testing.md); it
/// reaches every render in the process.
#[doc(hidden)]
pub fn expire_renders() {
    let deadlines = crate::Deadlines::get();
    let now = std::time::Instant::now();
    for armed in &mut deadlines.armed.lock().unwrap().1 {
        armed.until = armed.until.min(now);
    }
    deadlines.changed.notify_one();
}

impl Stopper {
    /// Start draining: the flag, then a connection that wakes the blocking
    /// accept to read it.
    pub fn stop(&self) {
        self.0.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.1, Duration::from_millis(200));
    }
}

struct Shared {
    serve: Serve,
    plan: exact_runner::Program,
    delivery: exact_runner::delivery::Delivery,
    shell: String,
    /// A page's CSP header lines, none when the app sends none (D11).
    csp: String,
    /// The header lines a page adds beside its CSP: its `Permissions-Policy`
    /// (LLP 1069.008 D6), and the client hints it asks for and varies by
    /// (LLP 1048.006).
    page_headers: String,
    /// What of the viewport the plan reads (LLP 1048.006).
    reads: crate::viewport::Reads,
    pages: Mutex<VecDeque<CachedPage>>,
    checkpoints: Checkpoints,
    best: Arc<crate::best::Compressor>,
    variants: Variants,
    generations: Option<crate::generations::Generations>,
    /// The entry's `modulepreload`s, for a 103 ([`crate::stream::hint`]).
    hints: Vec<String>,
    /// The shell is the JavaScript runtime's.
    js: bool,
    /// Its places, read once, and its early head ([`page::head_js`]) with the
    /// entry's preloads and without, when the plan alone decides the pages'
    /// `lang` and `dir` (no locale slot, or one table at most): a page may
    /// then go in two parts.
    early: Option<(page::Js, [String; 2])>,
}

struct CachedPage {
    target: String,
    /// The viewport class it was rendered for (LLP 1048.006).
    class: String,
    created: Instant,
    response: Response,
    /// Its bodies at the best compression, made by a thread of their own;
    /// until then a hit is compressed as it is sent, as a render is.
    best: Arc<OnceLock<encode::Best>>,
    /// The SHA-256 of its body, in base64: the name a browser gives it when
    /// it holds the page as a dictionary.
    hash: String,
    /// Its body against each dictionary a browser named, by that dictionary's
    /// hash: made once, when first asked for (none when it didn't shrink).
    pairs: HashMap<String, Option<Vec<u8>>>,
}

impl CachedPage {
    #[cfg(test)]
    fn with_body(mut self, len: usize) -> CachedPage {
        self.response.body = vec![0; len];
        self
    }

    /// What it holds, as the cache's budget counts it: its body and every
    /// variant made of it.
    fn bytes(&self) -> usize {
        self.response.body.len()
            + self.best.get().map_or(0, encode::Best::bytes)
            + self.pairs.values().flatten().map(Vec::len).sum::<usize>()
    }
}

/// Whether `incoming` more bytes would take the pages past the budget.
fn over(pages: &VecDeque<CachedPage>, incoming: usize) -> bool {
    pages.iter().map(CachedPage::bytes).sum::<usize>() + incoming > MAX_CACHE_BYTES
}

impl Server {
    /// Bind `127.0.0.1:<port>` over `serve.dist`, with `plan` (the one the
    /// dist's wasm carries) and `grants` (the app's, for the pages' CSP).
    pub fn bind(serve: Serve, plan: Plan, grants: &str) -> std::io::Result<Server> {
        let shell = ["shell.html", "index.html"]
            .iter()
            .find_map(|name| std::fs::read_to_string(serve.dist.join(name)).ok())
            .ok_or_else(|| std::io::Error::other("the dist has no shell"))?;
        let delivery = crate::delivery::beside(&serve.dist.join("app.plan"))?;
        let listener = TcpListener::bind(("127.0.0.1", serve.port))?;
        let csp = csp::headers(grants, &serve.dist, &shell, serve.origin.as_deref())
            .map_err(std::io::Error::other)?;
        let permissions = exact_runner::device::permissions_policy(grants);
        let reads = crate::viewport::Reads::of(&plan);
        let mut page_headers = reads.headers();
        if !permissions.is_empty() && !invalid_header(&permissions) {
            page_headers.insert_str(0, &format!("Permissions-Policy: {permissions}\r\n"));
        }
        // The transport's first start in a process is slow (Apple's takes
        // seconds); pay it here, not in the first request.
        drop(crate::Executor::start(grants));
        let variants = Variants::default();
        let generations = match &serve.generations {
            Some(dir) => Some(crate::generations::Generations::open(
                dir,
                &std::fs::read(serve.dist.join("app.wasm"))?,
            )?),
            None => None,
        };
        let mut files = encode::files(&serve.dist);
        if let Some(generations) = &generations {
            files.extend(generations.files());
        }
        variants.warm(files);
        let lang = (plan.locale.is_none() || plan.locales.len() <= 1).then(|| {
            plan.locales.first().map_or((String::new(), "ltr"), |row| {
                (
                    plan.str(row.name).to_string(),
                    if row.rtl { "rtl" } else { "ltr" },
                )
            })
        });
        let hints = crate::stream::preload_paths(&shell);
        let js = page::is_js(&shell);
        // The build's analyzed sheet (LLP 1048.007 D4).
        let rules = std::fs::read_to_string(serve.dist.join(".gen/page-style.json")).ok();
        let early = lang.filter(|_| js).and_then(|(lang, dir)| {
            let at = page::Js::of(&shell).ok()?;
            let at = match &rules {
                Some(rules) => at.with_rules(rules),
                None => at,
            };
            let heads =
                [true, false].map(|preload| page::head_js(&shell, &at, &lang, dir, preload, false));
            Some((at, heads))
        });
        Ok(Server {
            listener,
            stop: Arc::new(AtomicBool::new(false)),
            shared: Shared {
                serve,
                plan: plan.into(),
                delivery,
                shell,
                csp,
                page_headers,
                reads,
                pages: Mutex::new(VecDeque::new()),
                checkpoints: Checkpoints::default(),
                best: Arc::default(),
                variants,
                generations,
                hints,
                js,
                early,
            },
        })
    }

    /// Where it listens.
    pub fn addr(&self) -> SocketAddr {
        self.listener.local_addr().expect("a bound listener")
    }

    /// What drains it.
    pub fn stopper(&self) -> Stopper {
        Stopper(self.stop.clone(), self.addr())
    }

    /// Serve until drained ([`Stopper`]). `data` makes each render's source.
    pub fn run<D: DataSource + 'static>(self, data: fn() -> D) -> std::io::Result<()> {
        let shared = Arc::new(self.shared);
        // The connections waiting, and how many workers are free to take one.
        let waiting = Arc::new((
            Mutex::new((VecDeque::<TcpStream>::new(), 0usize)),
            Condvar::new(),
        ));
        // Closing a connection waits on its peer: one thread does that for
        // every connection, and no worker or accept loop does (crate::linger).
        let closing = crate::linger::Linger::start()?;
        let http_cache = exact_executor::SharedHttpCache::new();
        for _ in 0..shared.serve.renders.max(1) {
            let (shared, waiting, stop) = (shared.clone(), waiting.clone(), self.stop.clone());
            let closer = closing.closer();
            let http_cache = http_cache.clone();
            std::thread::spawn(move || {
                crate::executor::set_http_cache(http_cache);
                let (state, ready) = &*waiting;
                let mut answered = None;
                // Its first render's realm, before any request.
                crate::make_realm(data);
                loop {
                    // Free for the next request, and the last one's
                    // connection handed off to close, under one lock: a
                    // client that reads the end of its answer finds the
                    // worker free, and a drain that finds every worker free
                    // finds every answer handed off.
                    {
                        let mut state = state.lock().unwrap();
                        if let Some(stream) = answered.take() {
                            closer.close(stream);
                        }
                        state.1 += 1;
                    }
                    let stream = {
                        let mut state = state.lock().unwrap();
                        loop {
                            if let Some(stream) = state.0.pop_front() {
                                state.1 -= 1;
                                break stream;
                            }
                            // Drained: the worker ends, and its thread's
                            // executors with it (their workers count against
                            // the process's native-worker cap).
                            if stop.load(Ordering::SeqCst) {
                                drop(state);
                                crate::shutdown_render_thread();
                                return;
                            }
                            state = ready.wait(state).unwrap();
                        }
                    };
                    let (mut stream, mut keep) = handle(stream, &shared, data);
                    // A kept connection's next request: on this worker when
                    // no other connection waits, else behind the ones that
                    // do. Serving it at once let as many connections as
                    // there are workers take every render while the rest
                    // waited (RealWorld at concurrency 64, 12 renders: p95
                    // 2-3x the median).
                    answered = loop {
                        // The page is sent: the render's realm is dropped
                        // now, and the next one made while nothing waits.
                        crate::retire_renders();
                        if state.lock().unwrap().0.is_empty() {
                            crate::make_realm(data);
                        }
                        if !keep || !crate::stream::idle(&stream, state, &stop) {
                            break Some(stream);
                        }
                        let mut waiting = state.lock().unwrap();
                        if waiting.0.is_empty() {
                            drop(waiting);
                            (stream, keep) = handle(stream, &shared, data);
                            continue;
                        }
                        waiting.0.push_back(stream);
                        ready.notify_one();
                        break None;
                    };
                }
            });
        }
        // Accepting blocks; a drain wakes it with a connection of its own
        // (`Stopper::stop`). It polled with a 10 ms sleep before, which an
        // idle macOS process's timer coalescing stretched to 60–70 ms before
        // a request was even accepted (measured, 2026-09-28).
        self.listener.set_nonblocking(false)?;
        // What a full queue answers, made once.
        let mut busy = Vec::new();
        Response::text(503, "busy\n")
            .header("Cache-Control", "no-store")
            .header("Retry-After", "1")
            .write(&mut busy, false, &shared.csp, &shared.page_headers, false);
        let closer = closing.closer();
        while !self.stop.load(Ordering::SeqCst) {
            let stream = match self.listener.accept() {
                Ok((stream, _)) => stream,
                Err(_) => continue,
            };
            if self.stop.load(Ordering::SeqCst) {
                break;
            }
            // Headers and body are separate writes: without this, Nagle can
            // hold the body for the client's delayed ACK.
            let _ = stream.set_nodelay(true);
            // An accepted socket inherits the listener's mode on macOS.
            let _ = stream.set_nonblocking(false);
            let (state, ready) = &*waiting;
            let mut state = state.lock().unwrap();
            if state.0.len() >= state.1 + shared.serve.queue {
                drop(state);
                // Answered without reading its request: this thread never
                // waits on a peer, or a slow one would hold every accept.
                closer.refuse(stream, &busy);
                continue;
            }
            state.0.push_back(stream);
            ready.notify_one();
        }
        // Draining: no new connection is taken, and what was taken is
        // answered — within a render's deadline, twice over, and a margin.
        drop(self.listener);
        let renders = shared.serve.renders.max(1);
        let until = Instant::now() + shared.serve.deadline * 2 + Duration::from_secs(10);
        while Instant::now() < until {
            let state = waiting.0.lock().unwrap();
            if state.0.is_empty() && state.1 == renders {
                break;
            }
            drop(state);
            std::thread::sleep(Duration::from_millis(10));
        }
        waiting.1.notify_all();
        // Every answer is handed off: the closing thread takes each to its
        // end, within its hold, before the server returns.
        drop(closing);
        Ok(())
    }
}

/// The request fields that affect a response.
struct Request {
    method: String,
    target: String,
    if_none_match: Option<String>,
    revalidate: bool,
    no_store: bool,
    accepts: Accepts,
    /// A CDN forwarded it (RFC 8586's `CDN-Loop`): its response carries the
    /// surrogate keys.
    cdn: bool,
    /// The SHA-256 (base64) of the dictionary the browser holds for this URL
    /// (`Available-Dictionary`), which a kept page may be compressed against.
    dictionary: Option<String>,
    /// A browser's navigation (`Sec-Fetch-Dest: document`): a rendered page
    /// may go in two parts ([`crate::stream`]).
    navigate: bool,
    /// The client keeps the connection for its next request (HTTP/1.1's
    /// default, unless it said `Connection: close` or sent more than one).
    keep: bool,
    /// What it says of its reader's viewport (LLP 1048.006).
    hints: crate::viewport::Hints,
}

#[derive(Clone)]
struct Response {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    file: Option<(Arc<std::fs::File>, u64)>,
    /// Already sent, as a flushed page is ([`crate::stream`]); what it holds
    /// is what a buffered render would have answered, for the origin's cache.
    streamed: bool,
}

impl Response {
    fn text(status: u16, body: &str) -> Response {
        Response {
            status,
            headers: vec![("Content-Type", "text/plain; charset=utf-8".into())],
            body: body.as_bytes().to_vec(),
            file: None,
            streamed: false,
        }
    }

    fn header(mut self, name: &'static str, value: impl Into<String>) -> Response {
        self.headers.push((name, value.into()));
        self
    }

    /// `csp`: a page's CSP header lines (`csp::headers`); `page_headers`: the
    /// others it carries.
    fn write(
        &self,
        stream: &mut impl Write,
        head: bool,
        csp: &str,
        page_headers: &str,
        keep: bool,
    ) {
        let connection = if keep { "keep-alive" } else { "close" };
        // Header values can contain app data; refuse the entire response before
        // writing anything, including on the 304 path.
        if self.headers.iter().any(|(_, value)| invalid_header(value)) {
            Response::text(500, "invalid response header\n").write(stream, head, "", "", keep);
            return;
        }
        let reason = match self.status {
            200 => "OK",
            301 => "Moved Permanently",
            304 => "Not Modified",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            410 => "Gone",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "",
        };
        let mut out = format!("HTTP/1.1 {} {reason}\r\n", self.status);
        let page = self
            .headers
            .iter()
            .any(|(name, value)| *name == "Content-Type" && value.starts_with("text/html"));
        // A 304 carries what updates the copy the client holds (RFC 9110
        // §15.4.5), nothing of the representation's own: its CSP, surrogate
        // keys and length are the stored response's already. Its `Vary` is
        // every one its 200 sends, a page's viewport `Vary` among them.
        if self.status == 304 {
            for (name, value) in &self.headers {
                if matches!(*name, "ETag" | "Cache-Control" | "Vary" | "Expires" | "Age") {
                    let _ = write!(out, "{name}: {value}\r\n");
                }
            }
            if page {
                for line in page_headers.split_inclusive("\r\n") {
                    if line.starts_with("Vary:") {
                        out.push_str(line);
                    }
                }
            }
            let _ = write!(out, "Connection: {connection}\r\n\r\n");
            let _ = stream.write_all(out.as_bytes());
            let _ = stream.flush();
            return;
        }
        for (name, value) in &self.headers {
            let _ = write!(out, "{name}: {value}\r\n");
        }
        // A page's policy. On a script's response a CSP would govern only a
        // worker made from it, and a module worker evaluates the module text
        // it verified against the receipt (LLP 1027.002 D2).
        if page {
            out.push_str(csp);
            // A page's own code never reaches a device the app was not
            // granted: the same promise the CSP makes for `connect-src`.
            out.push_str(page_headers);
        }
        let _ = write!(
            out,
            "X-Content-Type-Options: nosniff\r\nReferrer-Policy: strict-origin-when-cross-origin\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n",
            self.file.as_ref().map_or(self.body.len() as u64, |(_, size)| *size)
        );
        let _ = stream.write_all(out.as_bytes());
        if !head {
            if let Some((file, size)) = &self.file {
                let _ = std::io::copy(&mut file.as_ref().take(*size), stream);
            } else {
                let _ = stream.write_all(&self.body);
            }
        }
        let _ = stream.flush();
    }
}

fn invalid_header(value: &str) -> bool {
    value
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
}

/// How long a request's head may take to arrive, from its worker's first
/// read of it.
const HEAD: Duration = Duration::from_secs(5);

/// Answer one request on a connection: the stream, and whether it stays
/// open for the client's next request ([`idle`]); the worker closes it.
fn handle<D: DataSource + 'static>(
    mut stream: TcpStream,
    shared: &Shared,
    data: fn() -> D,
) -> (TcpStream, bool) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let Ok(request) = read_request(&mut stream, HEAD) else {
        Response::text(400, "bad request\n").write(
            &mut stream,
            false,
            &shared.csp,
            &shared.page_headers,
            false,
        );
        return (stream, false);
    };
    let head = request.method == "HEAD";
    let response = if request.method != "GET" && !head {
        Response::text(405, "GET or HEAD\n").header("Allow", "GET, HEAD")
    } else {
        finish(respond(&request, shared, data, Some(&mut stream)), &request)
    };
    if !response.streamed {
        response.write(
            &mut stream,
            head,
            &shared.csp,
            &shared.page_headers,
            request.keep,
        );
    }
    (stream, request.keep)
}

/// A response as the client accepts it (D11): a page, the sitemap or any
/// other body made for this request, compressed now — brotli, else gzip —
/// with the encoding in its ETag, and a 304 when the client holds that
/// representation. A dist file carries its own `Vary` and made variant.
/// The surrogate keys go only to a CDN, which purges by them; a browser
/// never reads them.
fn finish(mut response: Response, request: &Request) -> Response {
    if response.streamed {
        return response;
    }
    if !request.cdn {
        response
            .headers
            .retain(|(name, _)| !matches!(*name, "Surrogate-Key" | "Cache-Tag"));
    }
    let kind = response
        .headers
        .iter()
        .find(|(name, _)| *name == "Content-Type")
        .map_or("", |(_, value)| value.as_str());
    let made = response.headers.iter().any(|(name, _)| *name == "Vary");
    if response.status == 304 || response.file.is_some() || made || !encode::compressible(kind) {
        return response;
    }
    response.headers.push(("Vary", "Accept-Encoding".into()));
    if let Some((encoding, body)) = encode::now(&response.body, request.accepts) {
        encoded(&mut response, encoding.name(), body);
    }
    if response.status == 200
        && response.headers.iter().any(|(name, value)| {
            *name == "ETag" && encode::none_match(request.if_none_match.as_deref(), value)
        })
    {
        response.status = 304;
    }
    response
}

/// `body` as `response`'s content in `encoding`, which its ETag names. A page
/// compressed as it was sent and one made at the best share their tag: they
/// decode to the same bytes, and the server sends no ranges.
fn encoded(response: &mut Response, encoding: &'static str, body: Vec<u8>) {
    response.body = body;
    response.headers.push(("Content-Encoding", encoding.into()));
    for (name, value) in &mut response.headers {
        if *name == "ETag" {
            *value = format!("{}-{encoding}\"", value.trim_end_matches('"'));
        }
    }
}

/// Of `locations`, the ones a crawler may index, as the build's sitemap
/// keeps them: each is answered as a request is (a cached page from the
/// origin's cache), and a page that is gone, failed, or says `noindex` is
/// left out. A page at its deadline stays: it exists, and asks to be read
/// again. Renders run `--renders` at a time.
fn indexed<D: DataSource + 'static>(
    shared: &Shared,
    data: fn() -> D,
    locations: Vec<String>,
) -> Vec<String> {
    let keep = |location: &String| {
        let request = Request {
            method: "GET".into(),
            target: location.clone(),
            if_none_match: None,
            revalidate: false,
            no_store: false,
            accepts: Accepts::default(),
            cdn: false,
            dictionary: None,
            navigate: false,
            keep: false,
            hints: Default::default(),
        };
        let response = respond(&request, shared, data, None);
        let noindex = response.headers.iter().any(|(name, value)| {
            *name == "X-Robots-Tag" && value.to_ascii_lowercase().contains("noindex")
        });
        matches!(response.status, 200 | 503) && !noindex
    };
    let mut kept = Vec::with_capacity(locations.len());
    for batch in locations.chunks(shared.serve.renders.max(1)) {
        let verdicts: Vec<bool> = std::thread::scope(|scope| {
            let running: Vec<_> = batch
                .iter()
                .map(|location| scope.spawn(move || keep(location)))
                .collect();
            running
                .into_iter()
                .map(|handle| handle.join().unwrap_or(false))
                .collect()
        });
        kept.extend(
            batch
                .iter()
                .zip(verdicts)
                .filter(|(_, keep)| *keep)
                .map(|(location, _)| location.clone()),
        );
    }
    kept
}

/// Read a request's head, `within` that long of now however its bytes are
/// paced: a timeout on each read alone lets a peer that sends a byte at a
/// time hold a worker for hours, and no render's deadline has begun.
fn read_request(stream: &mut TcpStream, within: Duration) -> Result<Request, ()> {
    let until = Instant::now() + within;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(());
        }
        let _ = stream.set_read_timeout(Some(left));
        let n = stream.read(&mut chunk).map_err(|_| ())?;
        if n == 0 || bytes.len() + n > 16 << 10 {
            return Err(());
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    // Bytes past the head are a second request sent before this one's
    // answer (pipelining): answered by closing, as before keep-alive.
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").ok_or(())? + 4;
    let pipelined = bytes.len() > end;
    let text = String::from_utf8(bytes).map_err(|_| ())?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().ok_or(())?.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (first.next(), first.next(), first.next(), first.next())
    else {
        return Err(());
    };
    if !version.starts_with("HTTP/1.")
        || !target.starts_with('/')
        || target.chars().any(char::is_control)
    {
        return Err(());
    }
    let headers: Vec<_> = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .collect();
    let if_none_match = headers
        .iter()
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("if-none-match"))
        .map(|(_, value)| value.trim().to_string());
    let revalidate = headers.iter().any(|(name, value)| {
        name.trim().eq_ignore_ascii_case("cache-control")
            && value.split(',').any(|part| {
                matches!(
                    part.trim().to_ascii_lowercase().as_str(),
                    "no-cache" | "no-store" | "max-age=0"
                )
            })
    });
    let no_store = headers.iter().any(|(name, value)| {
        name.trim().eq_ignore_ascii_case("cache-control")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("no-store"))
    });
    let accepts = headers
        .iter()
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("accept-encoding"))
        .map_or_else(Accepts::default, |(_, value)| Accepts::parse(value));
    let cdn = headers
        .iter()
        .any(|(name, _)| name.trim().eq_ignore_ascii_case("cdn-loop"));
    // A structured field's byte sequence: `:<base64>:`.
    let dictionary = headers
        .iter()
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("available-dictionary"))
        .and_then(|(_, value)| value.trim().strip_prefix(':')?.strip_suffix(':'))
        .map(str::to_string);
    let navigate = headers.iter().any(|(name, value)| {
        name.trim().eq_ignore_ascii_case("sec-fetch-dest")
            && value.trim().eq_ignore_ascii_case("document")
    });
    let close = headers.iter().any(|(name, value)| {
        name.trim().eq_ignore_ascii_case("connection")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("close"))
    });
    let keep = version == "HTTP/1.1" && !close && !pipelined && matches!(method, "GET" | "HEAD");
    Ok(Request {
        keep,
        hints: crate::viewport::Hints::parse(&headers),
        method: method.to_string(),
        target: target.to_string(),
        if_none_match,
        revalidate,
        no_store,
        accepts,
        cdn,
        dictionary,
        navigate: navigate && version == "HTTP/1.1",
    })
}

fn respond<D: DataSource + 'static>(
    request: &Request,
    shared: &Shared,
    data: fn() -> D,
    early: Option<&mut TcpStream>,
) -> Response {
    let path = request
        .target
        .split_once('?')
        .map_or(request.target.as_str(), |(path, _)| path);
    // Do not reflect a URL spelling that browsers can reinterpret as an origin
    // into Location. Dot segments follow the canonicalizer; check escapes too.
    let safe = percent_decode(path, true)
        .is_some_and(|decoded| !decoded.contains('\\') && !decoded.chars().any(char::is_control));
    if !safe {
        return Response::text(400, "bad path\n");
    }
    if path == "/.exact/health" {
        return Response::text(200, "ok\n").header("Cache-Control", "no-store");
    }
    if let Some(response) = deferred::response(path, &shared.checkpoints) {
        return response;
    }
    // An authored route at a crawler file's path is the app's page, not the generated file.
    let crawler = matches!(path, "/robots.txt" | "/sitemap.xml")
        && !shared
            .plan
            .routes
            .iter()
            .any(|r| !r.notfound && canonical_location(shared.plan.str(r.pattern)) == path);
    if crawler && path == "/sitemap.xml" {
        return sitemap::sitemap(shared, data);
    }
    if crawler && path == "/robots.txt" {
        // Neutral until hosting decides a crawler policy (LLP 1048 §9.9).
        let sitemap = shared
            .serve
            .origin
            .as_deref()
            .map_or(String::new(), |origin| {
                format!("Sitemap: {}/sitemap.xml\n", origin.trim_end_matches('/'))
            });
        let lifetime = shared.serve.lifetime.as_secs();
        return Response::text(200, &format!("User-agent: *\nAllow: /\n{sitemap}")).header(
            "Cache-Control",
            format!("public, max-age=0, s-maxage={lifetime}"),
        );
    }
    if let Some(response) = files::response(request, shared, path) {
        return response;
    }
    // One URL per page: any other spelling redirects to the canonical one.
    let canonical = canonical_location(&request.target);
    if canonical != request.target {
        return Response::text(301, "moved\n").header("Location", canonical);
    }
    let route = route_at(&shared.plan, &request.target);
    let policy = route.map(|r| r.render);
    let notfound = route.is_some_and(|r| r.notfound);
    // A file the dist doesn't have (a browser's `/favicon.ico`, an old
    // script) is a plain 404: the not-found document is for readers, and
    // rendering it cost 22 ms and 36 KB a visit (Interview's measurement).
    if notfound && asset_shaped(path) {
        return Response::text(404, "not found\n")
            .header("Cache-Control", "public, max-age=0, s-maxage=60");
    }
    if !notfound && matches!(policy, None | Some(RenderPolicy::Client)) {
        return Response {
            status: 200,
            headers: vec![
                ("Content-Type", "text/html; charset=utf-8".into()),
                ("Cache-Control", "no-cache".into()),
            ],
            body: shared.shell.clone().into_bytes(),
            file: None,
            streamed: false,
        };
    }
    if policy != Some(RenderPolicy::Cached) || shared.serve.lifetime.is_zero() || request.no_store {
        return document(request, policy, notfound, shared, data, early);
    }
    // A dictionary the browser holds and names itself (D11). A CDN's request
    // never gets one: the CDN would keep the body for browsers without it.
    let wanted = request
        .dictionary
        .as_deref()
        .filter(|_| !request.cdn && request.accepts.dcb());
    let class = shared.reads.class(&request.hints, shared.serve.viewport);
    let against;
    {
        let mut pages = shared.pages.lock().unwrap();
        pages.retain(|page| page.created.elapsed() < shared.serve.lifetime);
        let page = if request.revalidate {
            None
        } else {
            pages
                .iter()
                .find(|page| page.target == request.target && page.class == class)
        };
        let Some(page) = page else {
            drop(pages);
            return render_kept(request, policy, notfound, wanted, shared, data, early);
        };
        let response = page
            .response
            .clone()
            .header("Age", page.created.elapsed().as_secs().to_string());
        let dictionary = wanted.and_then(|hash| match page.pairs.get(hash) {
            Some(made) => Some(Err(made.clone())),
            None => pages
                .iter()
                .find(|kept| kept.hash == hash)
                .map(|kept| Ok(kept.response.body.clone())),
        });
        match dictionary {
            Some(Err(Some(body))) => return dictionary_compressed(response, body, request),
            Some(Ok(dictionary)) => against = (response, page.hash.clone(), dictionary),
            _ => return plain(response, page.best.get(), request),
        }
    }
    let (response, hash, dictionary) = against;
    pair(response, &hash, &dictionary, request, shared)
}

/// A miss on a kept route: render, keep the page, and send it as the other
/// kept pages go.
fn render_kept<D: DataSource + 'static>(
    request: &Request,
    policy: Option<RenderPolicy>,
    notfound: bool,
    wanted: Option<&str>,
    shared: &Shared,
    data: fn() -> D,
    early: Option<&mut TcpStream>,
) -> Response {
    // Render without holding the cache lock. A concurrent miss may render too;
    // it never delays an unrelated route or holds a module realm in the cache.
    let unconditional = Request {
        method: request.method.clone(),
        target: request.target.clone(),
        if_none_match: None,
        revalidate: true,
        no_store: false,
        accepts: request.accepts,
        cdn: request.cdn,
        dictionary: None,
        navigate: request.navigate && request.if_none_match.is_none(),
        keep: request.keep,
        hints: request.hints,
    };
    let mut response = document(&unconditional, policy, notfound, shared, data, early);
    // Keep no page sent `no-store`, such as one whose render drew randomness
    let unkept = response
        .headers
        .iter()
        .any(|(name, value)| *name == "Cache-Control" && value == "no-store");
    if response.status != 200 || response.body.len() > MAX_CACHE_BYTES || unkept {
        if response.status == 200 && identity(&response, request) {
            response.status = 304;
        }
        return response;
    }
    let hash = {
        use sha2::{Digest, Sha256};
        exact_data::envelope::base64(&Sha256::digest(&response.body))
    };
    let dictionary = {
        let mut pages = shared.pages.lock().unwrap();
        let class = shared.reads.class(&request.hints, shared.serve.viewport);
        pages.retain(|page| page.target != request.target || page.class != class);
        while pages.len() >= MAX_CACHED_PAGES || over(&pages, response.body.len()) {
            pages.pop_front();
        }
        // This response goes out compressed as it is sent; later hits get the
        // best, made here once, off every request's path.
        let best = Arc::new(OnceLock::new());
        shared.best.make(&best, response.body.clone());
        let dictionary = wanted.and_then(|wanted| {
            (wanted == hash).then(|| response.body.clone()).or_else(|| {
                pages
                    .iter()
                    .find(|kept| kept.hash == wanted)
                    .map(|kept| kept.response.body.clone())
            })
        });
        pages.push_back(CachedPage {
            target: request.target.clone(),
            class,
            created: Instant::now(),
            response: Response {
                streamed: false,
                ..response.clone()
            },
            best,
            hash: hash.clone(),
            pairs: HashMap::new(),
        });
        dictionary
    };
    if response.streamed {
        return response;
    }
    match dictionary {
        Some(dictionary) => pair(response, &hash, &dictionary, request, shared),
        None => plain(response, None, request),
    }
}

/// Say that a kept page may be a dictionary for the site's next documents
/// (Compression Dictionary Transport). A CDN's request doesn't hear it.
fn as_dictionary(response: &mut Response, request: &Request) {
    if !request.cdn {
        response.headers.push((
            "Use-As-Dictionary",
            "match=\"/*\", match-dest=(\"document\")".into(),
        ));
    }
}

/// A kept page as `finish` sends it, or as its best variant once that is
/// made, whose 304 is decided here: `finish` leaves a response with its
/// `Vary` as it is.
fn plain(mut response: Response, best: Option<&encode::Best>, request: &Request) -> Response {
    if let Some(tag) = held_tag(&response, request) {
        return not_modified(response, tag);
    }
    as_dictionary(&mut response, request);
    if let Some((encoding, body)) = best.and_then(|best| best.pick(request.accepts)) {
        response.headers.push(("Vary", "Accept-Encoding".into()));
        encoded(&mut response, encoding.name(), body.to_vec());
    }
    response
}

/// Of a kept page's encodings' tags, the one the request holds. They all
/// decode to the same bytes, so a client holding any has the page: a
/// browser back with a dictionary still gets its 304.
fn held_tag(response: &Response, request: &Request) -> Option<String> {
    let (_, tag) = response.headers.iter().find(|(name, _)| *name == "ETag")?;
    let tag = tag.trim_end_matches('"');
    ["", "-br", "-gzip", "-dcb"]
        .into_iter()
        .map(|coding| format!("{tag}{coding}\""))
        .find(|held| encode::none_match(request.if_none_match.as_deref(), held))
}

/// A 304 for a kept page the client holds as `tag`, which it keeps.
fn not_modified(mut response: Response, tag: String) -> Response {
    let vary = if tag.ends_with("-dcb\"") {
        "Accept-Encoding, Available-Dictionary"
    } else {
        "Accept-Encoding"
    };
    for (name, value) in &mut response.headers {
        if *name == "ETag" {
            value.clone_from(&tag);
        }
    }
    response.headers.push(("Vary", vary.into()));
    response.status = 304;
    response
}

/// Whether the request's `If-None-Match` is the page's own ETag, as a client
/// that took it uncompressed holds it.
fn identity(response: &Response, request: &Request) -> bool {
    response
        .headers
        .iter()
        .any(|(name, value)| *name == "ETag" && request.if_none_match.as_ref() == Some(value))
}

/// A kept page as `dcb` against the dictionary its request named, with its
/// own ETag and a `Vary` that names the dictionary.
fn dictionary_compressed(mut response: Response, body: Vec<u8>, request: &Request) -> Response {
    if let Some(tag) = held_tag(&response, request) {
        return not_modified(response, tag);
    }
    response
        .headers
        .push(("Vary", "Accept-Encoding, Available-Dictionary".into()));
    encoded(&mut response, "dcb", body);
    as_dictionary(&mut response, request);
    response
}

/// `response`, a kept page whose hash is `page`, against `dictionary`, which
/// the request named: compressed now, as a page is as it is sent, and kept
/// beside the page's other variants, inside the cache's budget.
fn pair(
    response: Response,
    page: &str,
    dictionary: &[u8],
    request: &Request,
    shared: &Shared,
) -> Response {
    use sha2::{Digest, Sha256};
    let made = encode::against(
        &response.body,
        dictionary,
        &Sha256::digest(dictionary).into(),
    );
    {
        let mut pages = shared.pages.lock().unwrap();
        let named = request.dictionary.clone().unwrap_or_default();
        if let Some(kept) = pages
            .iter_mut()
            .find(|kept| kept.target == request.target && kept.hash == page)
        {
            kept.pairs.insert(named, made.clone());
        }
        while over(&pages, 0) {
            pages.pop_front();
        }
    }
    match made {
        Some(body) => dictionary_compressed(response, body, request),
        None => plain(response, None, request),
    }
}

/// Large assets bypass compression and memory caches. Hash in a fixed buffer
/// for a content validator, rewind, then copy to the socket in a fixed buffer.
/// As with every response, slow clients occupy one bounded worker until timeout.
fn stream_file(path: &Path, kind: &str, request: &Request) -> Option<Response> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut remaining = size;
    while remaining > 0 {
        let capacity = remaining.min(buffer.len() as u64) as usize;
        let count = file.read(&mut buffer[..capacity]).ok()?;
        if count == 0 {
            return None;
        }
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    file.rewind().ok()?;
    let etag = format!("\"{:x}\"", hash.finalize());
    let fresh = encode::none_match(request.if_none_match.as_deref(), &etag);
    Some(Response {
        status: if fresh { 304 } else { 200 },
        headers: vec![
            ("Content-Type", kind.into()),
            ("Cache-Control", "no-cache".into()),
            ("ETag", etag),
        ],
        body: Vec::new(),
        file: Some((Arc::new(file), size)),
        streamed: false,
    })
}

fn document<D: DataSource + 'static>(
    request: &Request,
    policy: Option<RenderPolicy>,
    notfound: bool,
    shared: &Shared,
    data: fn() -> D,
    early: Option<&mut TcpStream>,
) -> Response {
    let serve = &shared.serve;
    let started = Instant::now();
    // The reader's viewport, when the plan reads one (LLP 1048.006).
    let viewport = shared.reads.viewport(&request.hints, serve.viewport);
    let site = Site {
        name: &serve.name,
        origin: serve.origin.as_deref(),
    };
    let location = request.target.as_str();
    let checkpoints = matches!(policy, Some(RenderPolicy::Request | RenderPolicy::Cached))
        .then_some(&shared.checkpoints);
    // @ref LLP 1071 D6 — the early flush (crate::stream): a browser's
    // navigation gets the page's head now; its rest follows the render.
    let js = shared.js && !notfound && request.method == "GET";
    let preload = crate::page::preloads_early(&shared.plan, location);
    let mut early = early.filter(|_| js);
    if let Some(out) = early.as_deref_mut().filter(|_| request.cdn && preload) {
        crate::stream::hint(out, &shared.hints);
    }
    let head = shared
        .early
        .as_ref()
        .filter(|_| request.navigate && !request.cdn && request.if_none_match.is_none())
        .zip(early)
        .map(|((at, heads), out)| (at, heads[usize::from(!preload)].clone(), out));
    let mut flush = head.map(|(at, head, out)| {
        if preload {
            crate::stream::hint(out, &shared.hints);
        }
        let cache = if policy == Some(RenderPolicy::Request) {
            "no-store"
        } else {
            "private, no-cache"
        };
        let flush = crate::stream::Flush::open(
            out,
            &head,
            cache,
            request.accepts,
            &shared.csp,
            &shared.page_headers,
            request.keep,
        );
        (flush, head, at)
    });
    // A flushed JavaScript page whose plan allows it is written from its
    // runner's instance tree and sent as it is written (LLP 1048.004).
    let mut streamed_body = false;
    let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut projection = Projection::Auto;
        if let Some((out, head, at)) = flush.as_mut() {
            let mut send = |bytes: &[u8]| out.send(bytes);
            let late = |activate: exact_plan::ActivatePolicy| {
                !preload && crate::page::activate_js(activate) != "interaction"
            };
            if let Some(page) = crate::direct::render_js(
                &shared.plan,
                &data,
                viewport,
                location,
                &site,
                serve.deadline,
                &shared.delivery,
                &shared.shell,
                at,
                late,
                MAX_PAGE.saturating_sub(head.len()),
                checkpoints,
                &mut send,
            )? {
                streamed_body = true;
                return Ok((page.rendered, head.clone() + &page.body));
            }
            // The direct writer requires a kernel; do not settle without one again.
            projection = Projection::Kernel;
        }
        // A JavaScript page drops the document's view ids (page::for_runtime).
        let ids = if shared.js { Ids::Any } else { Ids::Runtime };
        render_with_at(
            &shared.plan,
            &data,
            viewport,
            location,
            &site,
            serve.deadline,
            ids,
            projection,
            crate::render_time(),
            &shared.delivery,
        )
        .and_then(|rendered| {
            let html = match &flush {
                Some((_, head, at)) => {
                    let late =
                        !preload && crate::page::activate_js(rendered.activate) != "interaction";
                    head.clone()
                        + &crate::page::body_js(
                            &shared.shell,
                            at,
                            &rendered,
                            late,
                            false,
                            checkpoints,
                        )
                }
                None => match &shared.early {
                    Some((at, _)) => {
                        crate::page::page_with(&shared.shell, at, &rendered, checkpoints)
                    }
                    None => page::page_at(&shared.shell, &rendered, checkpoints)?,
                },
            };
            if html.len() > MAX_PAGE {
                return Err(format!("the page is {} bytes", html.len()));
            }
            Ok((rendered, html))
        })
    }))
    .unwrap_or_else(|_| Err("the render panicked".into()));
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let (rendered, html) = match rendered {
        Ok(done) => done,
        Err(error) => {
            crate::log(format_args!(
                "render {location} 500 {ms:.1}ms error={error:?}"
            ));
            if let Some((mut flush, _, _)) = flush {
                flush.send(UNAVAILABLE_AFTER_HEAD.as_bytes());
                flush.end();
                return Response {
                    streamed: true,
                    ..Response::text(500, "")
                };
            }
            return Response {
                status: 500,
                headers: vec![
                    ("Content-Type", "text/html; charset=utf-8".into()),
                    ("Cache-Control", "no-store".into()),
                ],
                file: None,
                streamed: false,
                body: b"<!doctype html>\n<title>Unavailable</title>\n<p>This page couldn't be rendered.</p>\n".to_vec(),
            };
        }
    };
    let status = rendered.status(notfound);
    crate::log(format_args!(
        "render {location} {status} {ms:.1}ms answers={} pending={} bytes={}{}",
        rendered.state.answers.len(),
        rendered.state.pending.len(),
        html.len(),
        match (flush.is_some(), streamed_body) {
            (true, true) => " flushed streamed",
            (true, false) => " flushed",
            _ => "",
        }
    ));
    let streamed = flush.is_some();
    if let Some((mut flush, head, _)) = flush.take() {
        if !streamed_body {
            flush.send(&html.as_bytes()[head.len()..]);
        }
        flush.end();
    }
    let mut response = Response {
        status,
        headers: vec![("Content-Type", "text/html; charset=utf-8".into())],
        body: html.into_bytes(),
        file: None,
        streamed: false,
    };
    let lifetime = serve.lifetime.as_secs();
    match status {
        503 => {
            response.headers.push(("Cache-Control", "no-store".into()));
            response.headers.push(("Retry-After", "1".into()));
        }
        // A page whose render drew randomness is the reader's alone, whatever
        // its status, as a draw is the device's (LLP 1069.005 D2)
        _ if rendered.drew => {
            response.headers.push(("Cache-Control", "no-store".into()));
        }
        404 | 410 => {
            response
                .headers
                .push(("Cache-Control", "public, max-age=0, s-maxage=60".into()));
        }
        _ if policy == Some(RenderPolicy::Request) => {
            response.headers.push(("Cache-Control", "no-store".into()));
        }
        _ => response.headers.push((
            "Cache-Control",
            format!("public, max-age=0, s-maxage={lifetime}, stale-while-revalidate={lifetime}"),
        )),
    }
    if let Some(robots) = &rendered.document.head.robots {
        response.headers.push(("X-Robots-Tag", robots.clone()));
    }
    response.streamed = streamed;
    // A flushed page's head went out before the render: its headers are
    // sent, so what follows reaches only the origin's cache, which keeps a
    // flushed 200 of a cached route. A page nothing keeps skips it (the
    // page's SHA-256 was ~3% of RealWorld's CPU per page).
    let kept = policy == Some(RenderPolicy::Cached)
        && !serve.lifetime.is_zero()
        && !request.no_store
        && !rendered.drew;
    if streamed && !kept {
        return response;
    }
    let keys = keys(&rendered);
    if !keys.is_empty() {
        response.headers.push(("Surrogate-Key", keys.join(" ")));
        response.headers.push(("Cache-Tag", keys.join(",")));
    }
    if status != 503 {
        let tag = format!("\"{}\"", &hex(&response.body)[..32]);
        if request.if_none_match.as_deref() == Some(tag.as_str()) {
            response.status = 304;
        }
        response.headers.push(("ETag", tag));
    }
    response
}

/// What a flushed page's rest is when its render fails: the 500's text,
/// after the head ([`crate::stream`]).
const UNAVAILABLE_AFTER_HEAD: &str =
    "<title>Unavailable</title>\n<p>This page couldn't be rendered.</p>\n";

/// The surrogate keys (D11): each answer's source, and its source with a
/// digest of its arguments — `post` purges every post page, `post:<hex>`
/// the pages that showed that one.
fn keys(rendered: &Rendered) -> Vec<String> {
    let mut keys = Vec::new();
    for (_, source, args, _) in &rendered.state.answers {
        let bytes = exact_plan::Value::list(args.clone()).to_bytes();
        for key in [source.clone(), format!("{source}:{}", &hex(&bytes)[..16])] {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

#[path = "csp.rs"]
mod csp;
#[cfg(test)]
use csp::csp;

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;

#[path = "sitemap.rs"]
mod sitemap;
