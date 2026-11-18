//! Held launch and prefetch replies (LLP 1162.000 D1, D3–D5).
use super::*;
use ibex2::boundary::HostError;
use ibex2::stdlib::fetch_async::Collected;
#[path = "preload_fetch.rs"]
mod fetch;
use fetch::run;
#[path = "prefetch.rs"]
mod prefetch;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Weak};
use std::time::{Duration, Instant};

pub(super) const DEFINITIONS_WAIT: Duration = Duration::from_secs(1);
pub(super) const HOLD: Duration = Duration::from_secs(10);
const DEFAULT_DEADLINE: Duration = Duration::from_secs(60);
const MAX_PRELOADS: usize = 8;
const MAX_BODY_BYTES: usize = 64 << 10;

#[derive(Clone, Copy)]
pub(super) enum NameWait {
    Idle,
    Definitions(Instant),
    Function(Instant),
    Ended,
}
enum WaitChange {
    Definitions(bool),
    Function(Option<Duration>),
    Expire,
    End,
}
fn change_wait(cache: &mut cache::Cache, change: WaitChange) {
    use NameWait::*;
    let now = (cache.clock)();
    cache.preload_wait = match (cache.preload_wait, change) {
        (Ended, _) | (_, WaitChange::End) => Ended,
        (Function(due), _) if now >= due => Ended,
        (Idle, WaitChange::Definitions(true)) => Definitions(now + DEFINITIONS_WAIT),
        (Idle | Definitions(_), WaitChange::Function(Some(wait))) => Function(now + wait),
        (Definitions(_), WaitChange::Definitions(false)) | (_, WaitChange::Function(None)) => Ended,
        (wait, _) => wait,
    };
}

/// A plain HTTP request with its secrets and launch settings filled in.
#[derive(Clone)]
pub struct Definition {
    /// Stable app-defined name used in the journal.
    pub name: String,
    /// Plain HTTP only; bodies larger than 64 KiB are refused.
    pub request: Request,
    /// Whole-exchange deadline; otherwise the request's, or 60 seconds.
    pub deadline_ms: Option<u32>,
    /// Secret names whose writes invalidate this request and its reply.
    pub secrets: Vec<String>,
}

impl Definition {
    /// Fill the manifest's references before validating or journalling its origin (LLP 1162.000 D2).
    pub fn fill(
        definition: &exact_runner::request::Preload,
        lookup: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, Fact> {
        use ibex2::stdlib::fetch;
        let mut request = definition.fill(lookup, preload_url).map_err(|error| Fact {
            name: definition.name.into(),
            origin: match error {
                exact_runner::request::PreloadFillError::InvalidUrl => String::new(),
                exact_runner::request::PreloadFillError::MissingSecret(_) => definition
                    .fill(|_| Some("redacted".into()), preload_url)
                    .ok()
                    .map(|r| origin(&r))
                    .unwrap_or_default(),
            },
            outcome: match error {
                exact_runner::request::PreloadFillError::MissingSecret(_) => Event::MissingSecret,
                exact_runner::request::PreloadFillError::InvalidUrl => Event::InvalidUrl,
            },
        })?;
        for (_, value) in &mut request.headers {
            *value = fetch::normalize_value(value).to_owned();
        }
        let invalid = if preload_url(&request.url).is_none() {
            Some(Event::InvalidUrl)
        } else if request
            .headers
            .iter()
            .any(|(_, value)| !fetch::is_valid_value(value))
        {
            Some(Event::InvalidHeader)
        } else {
            None
        };
        if let Some(outcome) = invalid {
            return Err(Fact {
                name: definition.name.into(),
                origin: if outcome == Event::InvalidUrl {
                    String::new()
                } else {
                    origin(&request)
                },
                outcome,
            });
        }
        Ok(Self {
            name: definition.name.into(),
            request,
            deadline_ms: definition.timeout_ms,
            secrets: definition.secrets.iter().map(|s| (*s).into()).collect(),
        })
    }
}
/// Validate a preload URL with the sender's parser and return its normalized scheme and authority
/// for the secret-boundary check (LLP 1162.000 D2–D3).
pub fn preload_url(target: &str) -> Option<String> {
    let parsed = ibex2::stdlib::url::parse(target, None).ok()?;
    let userinfo = target.split_once(':').is_some_and(|(_, tail)| {
        tail.trim_start_matches(['/', '\\'])
            .split(['/', '\\', '?', '#'])
            .next()
            .is_some_and(|authority| authority.contains('@'))
    });
    (matches!(parsed.protocol.as_str(), "http:" | "https:")
        && !parsed.hostname.is_empty()
        && !userinfo
        && parsed.username.is_empty()
        && parsed.password.is_empty()
        && !parsed.href.contains('#'))
    .then_some(parsed.origin)
}

fn origin(request: &Request) -> String {
    url::Url::parse(&request.url)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_default()
}

/// An outcome recorded at most once per preload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Handed to the repository fetch client.
    Sent,
    /// The crate finished its process-start function.
    LaunchRan {
        /// Time spent in the entry point, in milliseconds.
        elapsed_ms: u64,
        /// Number of requests handed over, before validation.
        requests: usize,
    },
    /// The crate panicked; no payload is retained.
    LaunchPanicked {
        /// Time until the panic was caught, in milliseconds.
        elapsed_ms: u64,
    },
    /// The crate exceeded its own launch window.
    LaunchDeadline {
        /// Time until the deadline was observed, in milliseconds.
        elapsed_ms: u64,
    },
    /// The device could not supply a named secret.
    MissingSecret,
    /// A filled header is not a Fetch field value.
    InvalidHeader,
    /// The filled URL does not parse.
    InvalidUrl,
    /// The function handed over a request whose name fails the grammar.
    InvalidName,
    /// The first hop or a redirect exceeded the grants.
    RefusedGrant,
    /// The definition was not a bounded plain HTTP request.
    Refused,
    /// The exchange failed without a complete reply.
    Failed,
    /// An equal read used the reply.
    Taken,
    /// An unused reply expired or was evicted.
    NeverTaken,
    /// The deadline or the owner ended the exchange.
    Cancelled,
    /// A write invalidated the request URL.
    DroppedByWrite,
    /// A refresh replaced the request.
    DroppedByRefresh,
    /// A write changed a secret the definition used.
    DroppedBySecret,
    /// The reply exceeded the cache entry limit.
    Oversized,
}

/// Journal facts contain no request path, query, body or header (D3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// The definition's app-defined name.
    pub name: String,
    /// Scheme, host and port, without credentials, path or query.
    pub origin: String,
    /// Why the journal recorded this fact.
    pub outcome: Event,
}

struct Ready {
    response: Response,
    visited: Vec<String>,
    completed: Instant,
    bytes: usize,
    used: usize,
}
struct Answered {
    at: Instant,
    visited: Vec<String>,
}
enum ReplyState {
    Pending {
        abort: AbortController,
        due: Instant,
    },
    Ready(Ready),
    Gone,
}
pub(super) struct Held {
    generation: usize,
    pub(super) prefetch: bool,
    fixture: Option<ResponseWork>,
    name: String,
    origin: String,
    secrets: Vec<String>,
    pub(super) request: Option<Request>,
    key: Equal,
    reply: ReplyState,
    taken: bool,
    pub(super) store: bool,
    started: Instant,
    answered_at: Option<Arc<Mutex<Option<Answered>>>>,
    observer: Option<Observer>,
}
#[derive(Default, PartialEq, Eq)]
struct Equal {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    redirect: exact_runner::Redirect,
    cache: exact_runner::CacheMode,
}
impl Equal {
    fn of(request: &Request) -> Self {
        let mut headers: Vec<_> = request
            .headers
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
            .collect();
        headers.sort();
        Self {
            method: request.method.clone(),
            url: cache::identity(&request.url),
            headers,
            redirect: request.redirect,
            cache: Self::cache_mode(request.cache),
        }
    }
    fn cache_mode(mode: exact_runner::CacheMode) -> exact_runner::CacheMode {
        use exact_runner::CacheMode;
        match mode {
            CacheMode::Unspecified
            | CacheMode::Default
            | CacheMode::ForceCache
            | CacheMode::OnlyIfCached => CacheMode::Default,
            mode => mode,
        }
    }
    fn same_cache_key(&self, other: &Self) -> bool {
        self.method == other.method && self.url == other.url && self.headers == other.headers
    }
}
impl Held {
    fn new(
        mut definition: Definition,
        now: Instant,
        observer: Option<Observer>,
        generation: usize,
    ) -> Self {
        definition.request.timeout_ms = definition.deadline_ms.or(definition.request.timeout_ms);
        let deadline = definition
            .request
            .timeout_ms
            .map_or(DEFAULT_DEADLINE, |ms| Duration::from_millis(ms.into()));
        let origin = url::Url::parse(&definition.request.url)
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default();
        Self {
            generation,
            prefetch: false,
            fixture: None,
            key: if definition.request.body.len() <= MAX_BODY_BYTES {
                Equal::of(&definition.request)
            } else {
                Equal::default()
            },
            name: definition.name,
            origin,
            secrets: definition.secrets,
            request: Some(definition.request),
            taken: false,
            store: true,
            started: Instant::now(),
            answered_at: Some(Arc::new(Mutex::new(None))),
            observer,
            reply: ReplyState::Pending {
                abort: AbortController::new(),
                due: now + deadline,
            },
        }
    }
    pub(super) fn used(&self) -> Option<usize> {
        match &self.reply {
            ReplyState::Ready(r) => Some(r.used),
            _ => None,
        }
    }
    fn fact(&self, outcome: Event) -> Fact {
        Fact {
            name: self.name.clone(),
            origin: self.origin.clone(),
            outcome,
        }
    }
}

/// Launch measurements, independent of executor adoption (LLP 1162.000 D4).
#[derive(Clone, Copy)]
pub enum Observation {
    /// The request was handed to the fetch client.
    Handed,
    /// A complete reply arrived.
    Complete,
    /// A read took the reply; the host records its successful commit.
    Taken(u64),
}
/// A bounded, nonblocking launch measurement sink.
pub type Observer = Arc<dyn Fn(&str, Observation) + Send + Sync>;

fn notify_observer(observer: &Observer, name: &str, observation: Observation) {
    // Measurement callbacks cannot interrupt settlement or poison its lock (LLP 1162.000 D4).
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(name, observation)));
}

/// `changed`, `threads` and `exited` serve tests, a host's among them, which
/// reach them through [`Control`]: they are compiled into every build.
struct Location {
    state: Mutex<Destination>,
    journal_pending: Arc<AtomicBool>,
    changed: Condvar,
    threads: Mutex<usize>,
    exited: Condvar,
    #[cfg(test)]
    fail_pool: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    before_close: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
}
struct ThreadGuard(Arc<Location>);
impl ThreadGuard {
    fn new(location: &Arc<Location>) -> Self {
        *location.threads.lock().unwrap() += 1;
        Self(location.clone())
    }
}
impl Drop for ThreadGuard {
    fn drop(&mut self) {
        *self.0.threads.lock().unwrap() -= 1;
        self.0.exited.notify_all();
    }
}

enum Destination {
    Staging(Box<cache::Cache>),
    Adopted(Weak<Shared>, Option<Weak<asynch::timer::Timer>>),
    Closed,
}
enum Message {
    Reply(usize, usize, Result<Collected, HostError>, Instant),
    Hop(Box<dyn FnOnce() + Send>),
    Start(Vec<(usize, usize)>, String),
    Barrier(mpsc::Sender<()>),
    Tick,
    Stop,
}

struct Startup {
    receive: mpsc::Receiver<Message>,
    lane: Arc<Lane>,
}

/// Owns the launch work until adoption. Dropping it cancels every exchange;
/// the launch thread never needs to join a transport (D4).
pub struct Preloads {
    startup: Arc<Mutex<Option<Startup>>>,
    destination: Arc<Location>,
    send: mpsc::Sender<Message>,
    /// The transport its exchanges run on, which the executor that adopts
    /// it takes for its independent lane (`lane.rs`).
    lane: Arc<Lane>,
}
impl Preloads {
    /// Start the first eight definitions off the caller's thread, with the
    /// same host factory as `Executor::start_on` (LLP 1162.000 D4).
    pub fn start(
        definitions: Vec<Definition>,
        grants: &str,
        host: impl Fn() -> ibex2::host::Host + Send + Sync + 'static,
    ) -> Self {
        Self::start_clock(
            definitions,
            grants,
            Lane::new(Arc::new(host)),
            Arc::new(Instant::now),
        )
    }
    fn start_clock(
        definitions: Vec<Definition>,
        grants: &str,
        lane: Arc<Lane>,
        clock: Arc<dyn Fn() -> Instant + Send + Sync>,
    ) -> Self {
        let mut cache = cache::Cache::new(grants.into());
        cache.clock = clock;
        let now = (cache.clock)();
        for definition in definitions.into_iter().take(MAX_PRELOADS) {
            let generation = cache.touch();
            cache
                .preloads
                .push(Held::new(definition, now, None, generation));
        }
        let indices = cache
            .preloads
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.generation))
            .collect::<Vec<_>>();
        let destination = Arc::new(Location {
            journal_pending: cache.journal_pending.clone(),
            state: Mutex::new(Destination::Staging(Box::new(cache))),
            changed: Condvar::new(),
            threads: Mutex::new(0),
            exited: Condvar::new(),
            #[cfg(test)]
            fail_pool: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            before_close: Mutex::new(None),
        });
        let (send, receive) = mpsc::channel();
        let set = Self {
            startup: Arc::new(Mutex::new(Some(Startup {
                receive,
                lane: lane.clone(),
            }))),
            destination,
            send,
            lane,
        };
        if !indices.is_empty() {
            update(&set.destination, |cache| {
                set.control().start(cache, indices, grants)
            });
        }
        set
    }
    /// Cancel uses of a changed secret before returning (LLP 1162.000 D3).
    pub fn drop_secret(&self, name: &str) {
        update(&self.destination, |cache| drop_secret(cache, name));
        let _ = self.send.send(Message::Tick);
    }
    /// Drain the bounded launch journal; the host chooses where to write it.
    pub fn take_journal(&self) -> Vec<Fact> {
        take_journal(&self.destination)
    }
}
/// A launch's control remains usable after the executor adopts its owner.
#[derive(Clone)]
pub struct Control {
    startup: Arc<Mutex<Option<Startup>>>,
    destination: Arc<Location>,
    send: mpsc::Sender<Message>,
}
impl Preloads {
    /// Keep cancellation, settlement and late foreground starts across adoption.
    pub fn control(&self) -> Control {
        Control {
            startup: self.startup.clone(),
            destination: self.destination.clone(),
            send: self.send.clone(),
        }
    }
}
impl Control {
    /// Selected updates may use only full equality (LLP 1162.000 D1, D2).
    pub fn select_update(&self) {
        update(&self.destination, |cache| cache.preload_names = false);
    }
    /// Wait only on the launch thread until earlier definitions have reached the client.
    pub fn handed(&self) {
        if self.startup.lock().unwrap().is_some() {
            return;
        }
        let (send, receive) = mpsc::channel();
        if self.send.send(Message::Barrier(send)).is_ok() {
            let _ = receive.recv();
        }
    }
    fn start(&self, cache: &mut cache::Cache, indices: Vec<(usize, usize)>, grants: &str) -> bool {
        if !cache.preloads_open {
            return false;
        }
        if !self.start_worker(cache)
            || self
                .send
                .send(Message::Start(indices, grants.into()))
                .is_err()
        {
            close(cache);
            return false;
        }
        true
    }
    fn start_worker(&self, cache: &cache::Cache) -> bool {
        if !cache.preloads_open {
            return false;
        }
        let Some(Startup { receive, lane }) = self.startup.lock().unwrap().take() else {
            return true;
        };
        let destination = self.destination.clone();
        let send = self.send.clone();
        let thread_guard = ThreadGuard::new(&destination);
        let spawned = std::thread::Builder::new()
            .name("exact-preload".into())
            .spawn(move || {
                let _guard = thread_guard;
                run(&destination, &send, receive, &lane)
            });
        spawned.is_ok()
    }
    /// A boot can dispatch before its launch thread has read the definitions;
    /// hold those reads for at most one second without delaying its first batch
    /// (LLP 1162.000 D4).
    pub fn definitions_pending(&self, pending: bool) {
        update(&self.destination, |cache| {
            change_wait(cache, WaitChange::Definitions(pending));
        });
        let _ = self.send.send(Message::Tick);
    }
    /// Hold reads naming unknown preloads while the Rust launch function runs,
    /// bounded by its remaining window (LLP 1162.000 D2).
    pub fn launch_pending(&self, deadline: Option<Duration>) {
        let delay = update(&self.destination, |cache| {
            change_wait(cache, WaitChange::Function(deadline));
            pending_due(cache).map(|due| due.saturating_duration_since((cache.clock)()))
        });
        if let Some(Some(delay)) = delay {
            if let Destination::Adopted(_, Some(timer)) = &*self.destination.state.lock().unwrap() {
                if let Some(timer) = timer.upgrade() {
                    arm_expiry(&self.destination, &timer, delay);
                }
            }
        }
        let _ = self.send.send(Message::Tick);
    }
    /// A test's clock in place of the monotonic one.
    pub fn test_clock(&self, clock: Arc<dyn Fn() -> Instant + Send + Sync>) {
        update(&self.destination, |cache| cache.clock = clock);
    }
    /// Expire what is due now and report whether definitions are still pending: a test's.
    pub fn test_tick(&self) -> bool {
        update(&self.destination, |cache| {
            expire(cache);
            pending_due(cache).is_some()
        })
        .unwrap_or(false)
    }
    /// The launch threads alive: a test's.
    pub fn thread_count(&self) -> usize {
        *self.destination.threads.lock().unwrap()
    }
    /// Wait up to ten seconds for the launch definitions to be read, and
    /// panic if they are not: a test's.
    pub fn wait_definitions(&self) {
        let state = self.destination.state.lock().unwrap();
        let (state, timeout) = self
            .destination
            .changed
            .wait_timeout_while(
                state,
                Duration::from_secs(10),
                |s| matches!(s, Destination::Staging(cache) if matches!(cache.preload_wait, NameWait::Definitions(_) | NameWait::Function(_))),
            )
            .unwrap();
        drop(state);
        assert!(!timeout.timed_out(), "launch definitions did not finish");
    }
    /// Wait up to ten seconds for `count` complete launch replies, and
    /// panic if they do not come: a test's.
    pub fn wait_ready(&self, count: usize) {
        let state = self.destination.state.lock().unwrap();
        let (state, timeout) = self.destination.changed.wait_timeout_while(state, Duration::from_secs(10), |s| {
            matches!(s, Destination::Staging(cache) if cache.preloads.iter().filter(|p| matches!(p.reply, ReplyState::Ready(_))).count() < count)
        }).unwrap();
        drop(state);
        assert!(!timeout.timed_out(), "launch replies did not finish");
    }

    /// Mark a name whose definition is known, including a refused or foreground request.
    pub fn defined(&self, name: &str) {
        update(&self.destination, |cache| {
            if !cache.preloads.iter().any(|p| p.name == name) {
                cache.preload_defined.insert(name.into());
            }
        });
    }
    /// Add launch definitions on their launch thread, including after adoption.
    pub fn extend(&self, definitions: Vec<Definition>, grants: &str, observer: Observer) {
        if let Destination::Staging(cache) = &mut *self.destination.state.lock().unwrap() {
            cache.grants = grants.into();
        }
        update_state(&self.destination, |cache, state| {
            if !cache.preloads_open {
                return;
            }
            let first = cache.preloads.len();
            let count = cache.preloads.iter().filter(|p| !p.prefetch).count();
            for definition in definitions.into_iter().take(MAX_PRELOADS - count) {
                cache.preload_defined.remove(&definition.name);
                let now = (cache.clock)();
                let generation = cache.touch();
                cache.preloads.push(Held::new(
                    definition,
                    now,
                    Some(observer.clone()),
                    generation,
                ));
                let i = cache.preloads.len() - 1;
                let held = &cache.preloads[i];
                if state.is_some_and(|state| {
                    refresh_underway(
                        state,
                        cache.preload_names,
                        held.request.as_ref().expect("new definition"),
                        &held.name,
                    )
                }) {
                    drop_held(cache, i, Event::DroppedByRefresh);
                }
            }
            let indices: Vec<_> = (first..cache.preloads.len())
                .filter(|i| matches!(cache.preloads[*i].reply, ReplyState::Pending { .. }))
                .map(|i| (i, cache.preloads[i].generation))
                .collect();
            if !indices.is_empty() {
                self.start(cache, indices, grants);
            }
        });
    }
    /// Cancel uses of a changed secret before returning.
    pub fn drop_secret(&self, name: &str) {
        update(&self.destination, |cache| drop_secret(cache, name));
        let _ = self.send.send(Message::Tick);
    }
    /// Cancel the launch function's accepted requests if it panics (LLP 1162.000 D2).
    pub fn cancel_names(&self, names: &[String]) {
        update(&self.destination, |cache| {
            for i in 0..cache.preloads.len() {
                if names.contains(&cache.preloads[i].name) {
                    drop_held(cache, i, Event::Cancelled);
                    cache::drop_preload_copy(cache, i);
                }
            }
        });
        let _ = self.send.send(Message::Tick);
    }
    /// Drain facts before or after adoption.
    pub fn take_journal(&self) -> Vec<Fact> {
        take_journal(&self.destination)
    }
    /// Settle a complete reply without an executor or a wait.
    pub fn settle(&self, request: &RequestOut) -> Option<(Outcome, CompletionTiming)> {
        let mut destination = self.destination.state.lock().unwrap();
        match &mut *destination {
            Destination::Staging(cache) => settle(cache, request),
            _ => None,
        }
    }
}
impl Drop for Preloads {
    fn drop(&mut self) {
        update(&self.destination, close);
        *self.destination.state.lock().unwrap() = Destination::Closed;
        let _ = self.send.send(Message::Stop);
    }
}

// Adoption moves the cache under the executor's lock. A callback releases
// the destination lock before taking that lock, in the same order as boot.
fn update<T>(location: &Location, f: impl FnOnce(&mut cache::Cache) -> T) -> Option<T> {
    update_state(location, |cache, _| f(cache))
}
fn update_state<T>(
    location: &Location,
    f: impl FnOnce(&mut cache::Cache, Option<&State>) -> T,
) -> Option<T> {
    let mut destination = location.state.lock().unwrap();
    match &mut *destination {
        Destination::Staging(cache) => {
            let result = f(cache, None);
            location.changed.notify_all();
            Some(result)
        }
        Destination::Adopted(shared, _) => {
            let shared = shared.upgrade()?;
            drop(destination);
            let mut state = shared.state.lock().unwrap();
            if state.retired {
                return None;
            }
            let mut cache = state.cache.take()?;
            let result = f(&mut cache, Some(&state));
            state.cache = Some(cache);
            cache::serve_ready(&mut state);
            rouse(&shared, &state);
            #[cfg(test)]
            shared.worker_notifications.fetch_add(1, Ordering::Relaxed);
            shared.ready.notify_all();
            Some(result)
        }
        Destination::Closed => None,
    }
}

// The pending bit is cleared under the same lock as every writer. Adoption
// moves that lock from Location to Shared without moving the bit (LLP 1171 D1).
fn take_journal(location: &Location) -> Vec<Fact> {
    if !location.journal_pending.load(Ordering::Acquire) {
        return Vec::new();
    }
    let mut destination = location.state.lock().unwrap();
    match &mut *destination {
        Destination::Staging(cache) => cache.take_preload_journal(),
        Destination::Adopted(shared, _) => {
            let Some(shared) = shared.upgrade() else {
                location.journal_pending.store(false, Ordering::Release);
                return Vec::new();
            };
            drop(destination);
            let mut state = shared.state.lock().unwrap();
            match state.cache.as_mut() {
                Some(cache) => cache.take_preload_journal(),
                // A retired executor discarded its cache and the facts in it.
                None => {
                    location.journal_pending.store(false, Ordering::Release);
                    Vec::new()
                }
            }
        }
        Destination::Closed => {
            location.journal_pending.store(false, Ordering::Release);
            Vec::new()
        }
    }
}

fn arm_expiry(location: &Arc<Location>, timer: &asynch::timer::Timer, delay: Duration) {
    let destination = Arc::downgrade(location);
    timer.arm_callback(
        delay,
        Box::new(move || {
            if let Some(destination) = destination.upgrade() {
                update(&destination, expire);
            }
        }),
    );
}

impl Core {
    /// Adopt launch work before dispatching any request (LLP 1162.000 D4).
    pub fn adopt_preloads(&self, set: Preloads) -> Result<(), Preloads> {
        let mut owned = self.preloads.lock().unwrap();
        let mut state = self.shared.state.lock().unwrap();
        if state.dispatched || state.retired || owned.is_some() {
            return Err(set);
        }
        let mut destination = set.destination.state.lock().unwrap();
        let Destination::Staging(mut cache) =
            std::mem::replace(&mut *destination, Destination::Closed)
        else {
            drop(destination);
            return Err(set);
        };
        cache.grants = self.grants.clone();
        if let (Some(machine), Some(due)) = (&self.machine, pending_due(&cache)) {
            arm_expiry(
                &set.destination,
                &machine.timer,
                due.saturating_duration_since((cache.clock)()),
            );
        }
        state.cache = Some(*cache);
        *destination = Destination::Adopted(
            Arc::downgrade(&self.shared),
            self.machine.as_ref().map(|m| Arc::downgrade(&m.timer)),
        );
        drop(destination);
        // Its exchanges' connections serve the lane's reads (`lane.rs`):
        // no worker has started, so none holds another transport. The
        // machine made its transports when the core was made, and keeps them.
        if self.machine.is_none() {
            *self.lane.lock().unwrap() = set.lane.clone();
        }
        *owned = Some(set);
        Ok(())
    }
    /// Cancel and discard preloads that used a changed secret before returning.
    pub fn drop_preload_secret(&self, name: &str) {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(cache) = state.cache.as_mut() {
            drop_secret(cache, name);
        }
        cache::serve_ready(&mut state);
        rouse(&self.shared, &state);
    }
    /// Facts are returned to the host for its journal (LLP 1162.000 D3).
    pub fn take_preload_journal(&self) -> Vec<Fact> {
        self.shared
            .state
            .lock()
            .unwrap()
            .cache
            .as_mut()
            .map(cache::Cache::take_preload_journal)
            .unwrap_or_default()
    }
    /// Boot may fulfil complete replies from its initial requests, in ticket
    /// order, until the first unresolved request (LLP 1162.000 D1, D4).
    pub fn settle_preload(&self, r: &RequestOut) -> Option<(Outcome, CompletionTiming)> {
        let mut state = self.shared.state.lock().unwrap();
        if state.retired || state.dispatched || state.counts != [0, 0] || state.ordered_barrier {
            return None;
        }
        settle(state.cache.as_mut()?, r)
    }
}

fn settle(cache: &mut cache::Cache, r: &RequestOut) -> Option<(Outcome, CompletionTiming)> {
    if !eligible(&r.request, r.class, r.forced, false) || r.request.timeout_refusal().is_some() {
        return None;
    }
    expire(cache);
    let i = find(cache, &r.request)?;
    if !matches!(cache.preloads[i].reply, ReplyState::Ready(_)) {
        return None;
    }
    if !can_take(cache, &r.request, i) {
        return None;
    }
    let (_, _, limit) = reads::reservation(&r.request, true).ok()?;
    let outcome = match cache::granted(&cache.grants, &r.request) {
        Err(outcome) => outcome,
        Ok(()) => {
            let ReplyState::Ready(ready) = &cache.preloads[i].reply else {
                unreachable!()
            };
            if response_bytes(&ready.response) + std::mem::size_of::<Completed>() > limit {
                return None;
            }
            let response = ready.response.clone();
            taken(cache, i, r.ticket);
            Outcome::Response(response)
        }
    };
    let cached = matches!(outcome, Outcome::Response(_));
    Some((
        outcome,
        CompletionTiming {
            cached,
            elapsed_ms: 0,
            delivery_wait_ms: 0,
        },
    ))
}

fn eligible(request: &Request, class: RequestClass, forced: bool, work: bool) -> bool {
    class == RequestClass::Read
        && request.reach.origin == exact_runner::CallOrigin::App
        && request.is_plain_http()
        && request.preload_refusal().is_none()
        && !forced
        && !work
        && !matches!(
            request.cache,
            exact_runner::CacheMode::Reload | exact_runner::CacheMode::NoCache
        )
}
fn find(cache: &cache::Cache, request: &Request) -> Option<usize> {
    if waiting_for_name(cache, request) {
        return None;
    }
    if cache.preload_names {
        if let Some(name) = &request.preload {
            let url = cache::identity(&request.url);
            let now = (cache.clock)();
            if let Some(i) = cache.preloads.iter().position(|p| {
                p.name == name.as_ref()
                    && p.key.method == request.method
                    && p.key.url == url
                    && p.key.redirect == request.redirect
                    && p.key.cache == Equal::cache_mode(request.cache)
                    && live_at(p, now)
            }) {
                return Some(i);
            }
        }
    }
    matching(cache, request, true)
}
// A preload's HTTP-cache copy cannot erase D1's body comparison.
pub(super) fn different_body(cache: &cache::Cache, request: &Request) -> bool {
    matching(cache, request, false).is_some()
}
fn matching(cache: &cache::Cache, request: &Request, equal_body: bool) -> Option<usize> {
    if cache.preloads.is_empty() {
        return None;
    }
    let key = Equal::of(request);
    let now = (cache.clock)();
    cache.preloads.iter().position(|p| {
        (if equal_body {
            p.key == key
        } else {
            p.key.same_cache_key(&key)
        }) && p
            .request
            .as_ref()
            .is_some_and(|r| (r.body == request.body) == equal_body)
            && live_at(p, now)
    })
}
fn live_at(held: &Held, now: Instant) -> bool {
    match &held.reply {
        ReplyState::Pending { due, .. } => now < *due,
        ReplyState::Ready(r) => now.saturating_duration_since(r.completed) < HOLD,
        ReplyState::Gone => false,
    }
}
fn can_take(cache: &cache::Cache, request: &Request, i: usize) -> bool {
    match &cache.preloads[i].reply {
        ReplyState::Ready(ready) => ready
            .visited
            .iter()
            .all(|url| cache::granted_url(&cache.grants, request, url).is_ok()),
        ReplyState::Pending { .. } => true,
        ReplyState::Gone => false,
    }
}
pub(super) fn pending_due(cache: &cache::Cache) -> Option<Instant> {
    match cache.preload_wait {
        NameWait::Definitions(due) => Some(due),
        NameWait::Function(due) if cache.preload_names => Some(due),
        _ => None,
    }
    .filter(|due| (cache.clock)() < *due)
}
fn waiting_for_name(cache: &cache::Cache, request: &Request) -> bool {
    cache.preload_names
        && matches!(
            cache.preload_wait,
            NameWait::Definitions(_) | NameWait::Function(_)
        )
        && request.preload.as_ref().is_some_and(|name| {
            !cache.preloads.iter().any(|p| p.name == name.as_ref())
                && !cache.preload_defined.contains(name.as_ref())
        })
}
pub(super) fn waiting(state: &State, job: &Job) -> bool {
    eligible(
        &job.request,
        job.class,
        job.forced,
        super::reads::opaque(&job.work),
    ) && state.cache.as_ref().is_some_and(|c| {
        let now = (c.clock)();
        matches!(c.preload_wait, NameWait::Definitions(due) if now < due)
            || waiting_for_name(c, &job.request)
            || find(c, &job.request).is_some_and(|i| can_take(c, &job.request, i))
    })
}
// The earliest queued request keeps room to finish before later replies
// can fill its lane. A preload failure still needs that room (D1).
pub(super) fn front_reservation(state: &State) -> usize {
    if !state.jobs[0]
        .iter()
        .any(|job| job.waited_for_preload || waiting(state, job))
    {
        return 0;
    }
    let job = state.jobs[0].front().expect("a preload waiter");
    job.limit - job.charge
}
pub(super) fn reserve_front(state: &mut State) -> bool {
    for index in 0..state.jobs[0].len() {
        let waiting = waiting(state, &state.jobs[0][index]);
        state.jobs[0][index].waited_for_preload |= waiting;
    }
    let more = front_reservation(state);
    if !reads::has_room(state, 0, more) {
        return false;
    }
    if more != 0 {
        state.jobs[0].front_mut().unwrap().charge += more;
        state.bytes[0] += more;
    }
    true
}
pub(super) fn effect_count(state: &State) -> usize {
    let effects = state.counts[0] - state.light;
    if state
        .cache
        .as_ref()
        .is_none_or(|c| pending_due(c).is_none() && c.preloads.is_empty())
    {
        return effects;
    }
    effects
        - state.jobs[0]
            .iter()
            .filter(|job| {
                waiting(state, job)
                    && reads::safe_read(&job.request, super::reads::opaque(&job.work), job.class)
            })
            .count()
}

pub(super) fn serve_ready(state: &mut State) {
    if let Some(cache) = state.cache.as_mut() {
        expire(cache);
    }
    if state.retired
        || state.cache.is_none()
        || state
            .running
            .iter()
            .any(|r| r.lane == 0 && r.blocking && r.cache_barrier)
    {
        return;
    }
    let mut index = 0;
    while index < state.jobs[0].len() {
        if !reserve_front(state) {
            break;
        }
        let job = &state.jobs[0][index];
        if job.forgotten && reads::safe_to_forget(&job.request, job.class) && waiting(state, job) {
            let job = state.jobs[0].remove(index).expect("forgotten preload read");
            state.counts[0] -= 1;
            state.bytes[0] -= job.charge;
            continue;
        }
        if job.class != RequestClass::Read
            || !job.request.is_plain_http()
            || super::reads::opaque(&job.work)
        {
            break;
        }
        if !eligible(&job.request, job.class, job.forced, false) {
            index += 1;
            continue;
        }
        let cache = state.cache.as_mut().expect("enabled");
        expire(cache);
        let Some(i) = find(cache, &job.request) else {
            index += 1;
            continue;
        };
        if !can_take(cache, &job.request, i) {
            index += 1;
            continue;
        }
        if job.forgotten && matches!(cache.preloads[i].reply, ReplyState::Ready(_)) {
            let job = state.jobs[0].remove(index).expect("forgotten preload read");
            state.counts[0] -= 1;
            state.bytes[0] -= job.charge;
            if state.counts[0] == 0 || !state.fenced.is_empty() {
                wake(state);
            }
            continue;
        }
        let outcome = match cache::granted(&cache.grants, &job.request) {
            Err(outcome) => outcome,
            Ok(()) => {
                let ReplyState::Ready(ready) = &cache.preloads[i].reply else {
                    index += 1;
                    continue;
                };
                let bytes = response_bytes(&ready.response) + std::mem::size_of::<Completed>();
                if bytes > BYTES[0].saturating_sub(state.bytes[0] - job.charge) {
                    break;
                }
                let response = ready.response.clone();
                taken(cache, i, job.ticket);
                Outcome::Response(response)
            }
        };
        let cached = matches!(outcome, Outcome::Response(_));
        let bytes = retained(&outcome) + std::mem::size_of::<Completed>();
        if bytes > BYTES[0].saturating_sub(state.bytes[0] - job.charge) {
            break;
        }
        let job = state.jobs[0].remove(index).expect("preload read");
        state.bytes[0] = state.bytes[0] - job.charge + bytes;
        let light = reads::safe_read(&job.request, super::reads::opaque(&job.work), job.class);
        state.light += usize::from(light);
        state.completed[0].push_back(Completed {
            cached,
            elapsed_ms: 0,
            finished_at: Instant::now(),
            ticket: job.ticket,
            outcome,
            bytes,
            message: false,
            stream: false,
            light,
            again: false,
            reply: false,
        });
    }
    if has_ready(state) {
        wake(state);
    }
}
fn response_bytes(response: &Response) -> usize {
    response.body.len()
        + response.headers.len() * std::mem::size_of::<(String, String)>()
        + response
            .headers
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
}
fn taken(cache: &mut cache::Cache, i: usize, ticket: u64) {
    if let Some(observer) = &cache.preloads[i].observer {
        notify_observer(
            observer,
            &cache.preloads[i].name,
            Observation::Taken(ticket),
        );
    }
    let used = cache.touch();
    let held = &mut cache.preloads[i];
    if let ReplyState::Ready(r) = &mut held.reply {
        r.used = used;
    }
    if !held.taken && !held.prefetch {
        let fact = held.fact(Event::Taken);
        held.taken = true;
        cache.record_preload_fact(fact);
    }
}
pub(super) fn drop_held(cache: &mut cache::Cache, i: usize, event: Event) {
    let held = &mut cache.preloads[i];
    let reply = std::mem::replace(&mut held.reply, ReplyState::Gone);
    let invalidates = matches!(reply, ReplyState::Pending { .. })
        .then(|| {
            held.request
                .as_ref()
                .and_then(|r| cache::invalidates(r, false))
        })
        .flatten();
    let bytes = match reply {
        ReplyState::Pending { abort, .. } => {
            abort.abort();
            0
        }
        ReplyState::Ready(r) => r.bytes,
        ReplyState::Gone => return,
    };
    let fact =
        (!held.prefetch && (event != Event::NeverTaken || !held.taken)).then(|| held.fact(event));
    held.request = None;
    held.key = Equal::default();
    held.answered_at = None;
    if let Some(fact) = fact {
        cache.record_preload_fact(fact);
    }
    cache.release(bytes);
    // Cancellation settles before the transport can report its failure (LLP 1162 D4).
    if let Some((url, _)) = invalidates {
        cache::drop_entries(cache, &[url]);
    }
}
// Completion wins over a later timer observation (LLP 1162.000 D1).
pub(super) fn expire(cache: &mut cache::Cache) {
    let now = (cache.clock)();
    change_wait(cache, WaitChange::Expire);
    for i in 0..cache.preloads.len() {
        match &cache.preloads[i].reply {
            ReplyState::Pending { due, .. }
                if now >= *due
                    && cache.preloads[i]
                        .answered_at
                        .as_ref()
                        .expect("pending exchange")
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_none_or(|answer| answer.at >= *due) =>
            {
                drop_held(cache, i, Event::Cancelled)
            }
            ReplyState::Ready(r) if now.saturating_duration_since(r.completed) >= HOLD => {
                drop_held(cache, i, Event::NeverTaken)
            }
            _ => {}
        }
    }
}
pub(super) fn drop_urls(cache: &mut cache::Cache, urls: &[String]) {
    for i in 0..cache.preloads.len() {
        if urls.contains(&cache.preloads[i].key.url) {
            drop_held(cache, i, Event::DroppedByWrite);
        }
    }
}
pub(super) fn withhold_store(cache: &mut cache::Cache, request: &Request) {
    let key = Equal::of(request);
    for held in &mut cache.preloads {
        if held.key.same_cache_key(&key) && matches!(held.reply, ReplyState::Pending { .. }) {
            // Asking withholds the HTTP copy; admission drops the hold (LLP 1162 D4).
            held.store = false;
        }
    }
}
pub(super) fn refresh(cache: &mut cache::Cache, r: &RequestOut) {
    let Some(refresh) = Refresh::of(&r.request, r.class, r.forced) else {
        return;
    };
    for i in 0..cache.preloads.len() {
        let held = &cache.preloads[i];
        if held
            .request
            .as_ref()
            .is_some_and(|request| refresh.drops(cache.preload_names, request, &held.name))
        {
            cache::drop_preload_copy(cache, i);
            drop_held(cache, i, Event::DroppedByRefresh);
        }
    }
}

pub(super) struct Refresh {
    key: Equal,
    body: Option<Vec<u8>>,
    name: Option<Box<str>>,
}
impl Refresh {
    pub(super) fn of(request: &Request, class: RequestClass, forced: bool) -> Option<Self> {
        (class == RequestClass::Read
            && request.is_plain_http()
            && (forced
                || matches!(
                    request.cache,
                    exact_runner::CacheMode::Reload | exact_runner::CacheMode::NoCache
                )))
        .then(|| Self {
            key: Equal::of(request),
            // A larger body can match a hold only by name (LLP 1162.000 D1).
            body: (request.body.len() <= MAX_BODY_BYTES).then(|| request.body.clone()),
            name: request.preload.clone(),
        })
    }
    fn drops(&self, names: bool, request: &Request, name: &str) -> bool {
        let key = Equal::of(request);
        let named = names
            && self.name.as_deref() == Some(name)
            && self.key.method == key.method
            && self.key.url == key.url;
        // Refresh equality ignores cache mode, including no-store (LLP 1162.000 D1).
        named
            || (self.key.same_cache_key(&key)
                && self.key.redirect == key.redirect
                && self.body.as_deref() == Some(request.body.as_slice()))
    }
}

// Cache lookup asks only about earlier tickets; a hold must exclude every
// unfinished refresh, including one behind it (LLP 1162.000 D1, D5).
fn refresh_underway(state: &State, names: bool, request: &Request, name: &str) -> bool {
    // Independent requests are never refreshes: Refresh::of requires is_plain_http (LLP 1162.000 D1).
    reads::pending(state).any(|pending| match pending {
        reads::Pending::Running(r) => r
            .refresh
            .as_ref()
            .is_some_and(|r| r.drops(names, request, name)),
        reads::Pending::Queued(j) => Refresh::of(&j.request, j.class, j.forced)
            .is_some_and(|r| r.drops(names, request, name)),
    }) || state.fenced.iter().any(|f| match f {
        Fenced::Held(r, ..) => Refresh::of(&r.request, r.class, r.forced)
            .is_some_and(|r| r.drops(names, request, name)),
        _ => false,
    })
}
fn drop_secret(cache: &mut cache::Cache, name: &str) {
    for i in 0..cache.preloads.len() {
        if cache.preloads[i].secrets.iter().any(|s| s == name) {
            drop_held(cache, i, Event::DroppedBySecret);
            // A preload's HTTP-cache copy must not outlive its credential (D3).
            cache::drop_preload_copy(cache, i);
        }
    }
}
pub(super) fn close(cache: &mut cache::Cache) {
    cache.preloads_open = false;
    change_wait(cache, WaitChange::End);
    for i in 0..cache.preloads.len() {
        drop_held(cache, i, Event::Cancelled);
    }
}

/// Runtime checks for requests supplied by Rust, including filled manifest references.
pub fn valid_request(request: &Request) -> bool {
    use ibex2::stdlib::fetch::{is_valid_name, is_valid_value, normalize_value, Headers};
    if preload_url(&request.url).is_none()
        || !is_valid_name(&request.method)
        || !request.is_plain_http()
        || request.body.len() > MAX_BODY_BYTES
        || (matches!(request.method.to_ascii_uppercase().as_str(), "GET" | "HEAD")
            && !request.body.is_empty())
        || request.preload_refusal().is_some()
        || request.timeout_refusal().is_some()
    {
        return false;
    }
    let mut names = std::collections::BTreeSet::new();
    for (name, value) in &request.headers {
        if !is_valid_name(name)
            || !is_valid_value(value)
            || normalize_value(value) != value
            || !names.insert(name.to_ascii_lowercase())
        {
            return false;
        }
        let mut headers = Headers::new();
        headers.set(name, value);
        if headers.for_request().entries().is_empty() {
            return false;
        }
    }
    true
}

/// A launch-context fetch bypasses the executor's cache in both directions (LLP 1162.000 D2).
pub fn launch_fetch(
    request: Request,
    grants: &str,
    host: ibex2::host::Host,
    launch_signal: &ibex2::stdlib::abort::AbortSignal,
) -> Result<Response, String> {
    if !valid_request(&request) {
        return Err("invalid launch fetch".into());
    }
    cache::granted(grants, &request).map_err(|_| "launch fetch refused".to_owned())?;
    let scope = exact_data::storage::scope(grants, request.grants.as_deref())?;
    let grants = ibex2::grant::GrantSet::parse(&exact_runner::io_grants(scope))
        .map_err(|_| "invalid launch grants")?;
    let abort = AbortController::new();
    let cancel = abort.clone();
    let _launch = launch_signal.register(move || cancel.abort());
    let deadline = super::outcome::Deadline::arm(
        request
            .timeout_ms
            .unwrap_or(DEFAULT_DEADLINE.as_millis() as u32),
        &abort,
    )?;
    let fetch = reach::gate_fetch(host.endow(grants).fetch, request.reach);
    let mut req = fetch_request(request, false);
    req.max_body = Some(cache::MAX_ENTRY);
    req.collected = true;
    let result = fetch
        .stream(req, &abort.signal())
        .and_then(check_headers)
        .and_then(|r| r.collect());
    if deadline.passed() {
        return Err("launch fetch deadline".into());
    }
    result
        .map(|r| Response {
            status: r.status,
            headers: r.headers.entries().to_vec(),
            body: r.body,
        })
        .map_err(|_| "launch fetch failed".into())
}

#[cfg(test)]
#[path = "preload_tests.rs"]
mod tests;
