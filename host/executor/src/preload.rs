//! Replies fetched before the executor exists (LLP 1162.000 D1, D3, D4).
use super::*;
use ibex2::boundary::HostError;
use ibex2::stdlib::fetch_async::{Collected, FetchEvents, HeaderCaps};
use std::sync::{mpsc, Weak};
use std::time::{Duration, Instant};

pub(super) const DEFINITIONS_WAIT: Duration = Duration::from_secs(1);
pub(super) const HOLD: Duration = Duration::from_secs(10);
const DEFAULT_DEADLINE: Duration = Duration::from_secs(60);
const MAX_PRELOADS: usize = 8;
const MAX_BODY_BYTES: usize = 64 << 10;

/// A plain HTTP request prepared by the host, with its present secrets filled in.
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

/// An outcome recorded at most once per preload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Handed to the repository fetch client.
    Sent,
    /// The device could not supply a named secret.
    MissingSecret,
    /// A filled header is not a Fetch field value.
    InvalidHeader,
    /// The filled URL does not parse.
    InvalidUrl,
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
    name: String,
    origin: String,
    secrets: Vec<String>,
    request: Option<Request>,
    key: Equal,
    reply: ReplyState,
    taken: bool,
    started: Instant,
    answered_at: Option<Arc<Mutex<Option<Answered>>>>,
    observer: Option<Observer>,
}
#[derive(Default, PartialEq, Eq)]
struct Equal {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
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
        }
    }
}
impl Held {
    fn new(definition: Definition, now: Instant, observer: Option<Observer>) -> Self {
        let deadline = definition
            .deadline_ms
            .or(definition.request.timeout_ms)
            .map_or(DEFAULT_DEADLINE, |ms| Duration::from_millis(ms.into()));
        let origin = url::Url::parse(&definition.request.url)
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default();
        Self {
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

/// `changed`, `threads` and `exited` serve tests, a host's among them, which
/// reach them through [`Control`]: they are compiled into every build.
struct Location {
    state: Mutex<Destination>,
    changed: Condvar,
    threads: Mutex<usize>,
    exited: Condvar,
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
    Adopted(Weak<Shared>),
    Closed,
}
enum Message {
    Reply(usize, Result<Collected, HostError>, Instant),
    Hop(Box<dyn FnOnce() + Send>),
    Start(std::ops::Range<usize>, String),
    Tick,
    Stop,
}

struct Startup {
    receive: mpsc::Receiver<Message>,
    host: StreamHost,
}

/// Owns the launch work until adoption. Dropping it cancels every exchange;
/// the launch thread never needs to join a transport (D4).
pub struct Preloads {
    startup: Arc<Mutex<Option<Startup>>>,
    destination: Arc<Location>,
    send: mpsc::Sender<Message>,
}
impl Preloads {
    /// Start the first eight definitions off the caller's thread, with the
    /// same host factory as `Executor::start_on` (LLP 1162.000 D4).
    pub fn start(
        definitions: Vec<Definition>,
        grants: &str,
        host: impl Fn() -> ibex2::host::Host + Send + Sync + 'static,
    ) -> Self {
        Self::start_clock(definitions, grants, Arc::new(host), Arc::new(Instant::now))
    }
    fn start_clock(
        definitions: Vec<Definition>,
        grants: &str,
        host: StreamHost,
        clock: Arc<dyn Fn() -> Instant + Send + Sync>,
    ) -> Self {
        let mut cache = cache::Cache::new(grants.into());
        cache.clock = clock;
        let now = (cache.clock)();
        cache.preloads = definitions
            .into_iter()
            .take(MAX_PRELOADS)
            .map(|definition| Held::new(definition, now, None))
            .collect();
        let count = cache.preloads.len();
        let destination = Arc::new(Location {
            state: Mutex::new(Destination::Staging(Box::new(cache))),
            changed: Condvar::new(),
            threads: Mutex::new(0),
            exited: Condvar::new(),
        });
        let (send, receive) = mpsc::channel();
        let set = Self {
            startup: Arc::new(Mutex::new(Some(Startup { receive, host }))),
            destination,
            send,
        };
        if count > 0 {
            set.control().start_worker();
            let _ = set.send.send(Message::Start(0..count, grants.into()));
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
        update(&self.destination, |cache| {
            std::mem::take(&mut cache.journal)
        })
        .unwrap_or_default()
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
    fn start_worker(&self) {
        let Some(Startup { receive, host }) = self.startup.lock().unwrap().take() else {
            return;
        };
        let destination = self.destination.clone();
        let send = self.send.clone();
        let thread_guard = ThreadGuard::new(&destination);
        let spawned = std::thread::Builder::new()
            .name("exact-preload".into())
            .spawn(move || {
                let _guard = thread_guard;
                run(&destination, &send, receive, &host)
            });
        if spawned.is_err() {
            update(&self.destination, cancel_all);
        }
    }
    /// A boot can dispatch before its launch thread has read the definitions;
    /// hold those reads for at most one second without delaying its first batch
    /// (LLP 1162.000 D4).
    pub fn definitions_pending(&self, pending: bool) {
        update(&self.destination, |cache| {
            cache.preload_starting = pending;
            cache.preload_definitions_due = pending.then(|| (cache.clock)() + DEFINITIONS_WAIT);
        });
        let _ = self.send.send(Message::Tick);
    }
    /// A test's clock in place of the monotonic one.
    pub fn test_clock(&self, clock: Arc<dyn Fn() -> Instant + Send + Sync>) {
        update(&self.destination, |cache| cache.clock = clock);
    }
    /// Expire what is due now, as the launch thread's tick does: a test's.
    pub fn test_tick(&self) {
        update(&self.destination, expire);
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
                |s| matches!(s, Destination::Staging(cache) if cache.preload_starting),
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

    /// Add launch definitions on their launch thread, including after adoption.
    pub fn extend(&self, definitions: Vec<Definition>, grants: &str, observer: Observer) {
        if let Destination::Staging(cache) = &mut *self.destination.state.lock().unwrap() {
            cache.grants = grants.into();
        }
        let range = update(&self.destination, |cache| {
            let first = cache.preloads.len();
            for definition in definitions.into_iter().take(MAX_PRELOADS - first) {
                let now = (cache.clock)();
                cache
                    .preloads
                    .push(Held::new(definition, now, Some(observer.clone())));
            }
            first..cache.preloads.len()
        });
        if let Some(range) = range.filter(|range| !range.is_empty()) {
            self.start_worker();
            if self
                .send
                .send(Message::Start(range, grants.into()))
                .is_err()
            {
                update(&self.destination, |cache| {
                    cache.preload_starting = false;
                    cancel_all(cache);
                });
            }
        }
    }
    /// Cancel uses of a changed secret before returning.
    pub fn drop_secret(&self, name: &str) {
        update(&self.destination, |cache| drop_secret(cache, name));
        let _ = self.send.send(Message::Tick);
    }
    /// Drain facts before or after adoption.
    pub fn take_journal(&self) -> Vec<Fact> {
        update(&self.destination, |cache| {
            std::mem::take(&mut cache.journal)
        })
        .unwrap_or_default()
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
        update(&self.destination, cancel_all);
        *self.destination.state.lock().unwrap() = Destination::Closed;
        let _ = self.send.send(Message::Stop);
    }
}

// Adoption moves the cache under the executor's lock. A callback releases
// the destination lock before taking that lock, in the same order as boot.
fn update<T>(location: &Location, f: impl FnOnce(&mut cache::Cache) -> T) -> Option<T> {
    let mut destination = location.state.lock().unwrap();
    match &mut *destination {
        Destination::Staging(cache) => {
            let result = f(cache);
            location.changed.notify_all();
            Some(result)
        }
        Destination::Adopted(shared) => {
            let shared = shared.upgrade()?;
            drop(destination);
            let mut state = shared.state.lock().unwrap();
            if state.retired {
                return None;
            }
            let result = f(state.cache.as_mut()?);
            cache::serve_ready(&mut state);
            rouse(&shared, &state);
            shared.ready.notify_all();
            Some(result)
        }
        Destination::Closed => None,
    }
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
        if let (Some(machine), Some(due)) = (&self.machine, cache.preload_definitions_due) {
            let destination = Arc::downgrade(&set.destination);
            machine.timer.arm_callback(
                due.saturating_duration_since((cache.clock)()),
                Box::new(move || {
                    if let Some(destination) = destination.upgrade() {
                        update(&destination, expire);
                    }
                }),
            );
        }
        state.cache = Some(*cache);
        *destination = Destination::Adopted(Arc::downgrade(&self.shared));
        drop(destination);
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
            .map(|cache| std::mem::take(&mut cache.journal))
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
        && !forced
        && !work
        && !matches!(
            request.cache,
            exact_runner::CacheMode::Reload | exact_runner::CacheMode::NoCache
        )
}
fn find(cache: &cache::Cache, request: &Request) -> Option<usize> {
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
        p.key == key
            && p.request
                .as_ref()
                .is_some_and(|r| (r.body == request.body) == equal_body)
            && match &p.reply {
                ReplyState::Pending { due, .. } => now < *due,
                ReplyState::Ready(r) => now.saturating_duration_since(r.completed) < HOLD,
                ReplyState::Gone => false,
            }
    })
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
pub(super) fn waiting(state: &State, job: &Job) -> bool {
    eligible(&job.request, job.class, job.forced, job.work.is_some())
        && state.cache.as_ref().is_some_and(|c| {
            c.preload_starting
                || find(c, &job.request).is_some_and(|i| can_take(c, &job.request, i))
        })
}
// The earliest queued request keeps room to finish before later replies
// can fill its lane. A preload failure still needs that room (D1).
pub(super) fn reserve_front(state: &mut State) -> bool {
    if !state.jobs[0].iter().any(|job| waiting(state, job)) {
        return true;
    }
    let job = state.jobs[0].front().expect("a preload waiter");
    let more = if handed_off(job) {
        0
    } else {
        job.limit - job.charge
    };
    if more > BYTES[0].saturating_sub(state.bytes[0]) {
        return false;
    }
    state.jobs[0].front_mut().unwrap().charge += more;
    state.bytes[0] += more;
    true
}
pub(super) fn effect_count(state: &State) -> usize {
    let effects = state.counts[0] - state.light;
    if state
        .cache
        .as_ref()
        .is_none_or(|c| !c.preload_starting && c.preloads.is_empty())
    {
        return effects;
    }
    effects
        - state.jobs[0]
            .iter()
            .filter(|job| {
                waiting(state, job) && reads::safe_read(&job.request, job.work.is_some(), job.class)
            })
            .count()
}

pub(super) fn serve_ready(state: &mut State) {
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
        if job.forgotten && waiting(state, job) {
            let job = state.jobs[0].remove(index).expect("forgotten preload read");
            state.counts[0] -= 1;
            state.bytes[0] -= job.charge;
            continue;
        }
        if job.class != RequestClass::Read || !job.request.is_plain_http() || job.work.is_some() {
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
        state.light += 1;
        state.completed[0].push_back(Completed {
            cached,
            elapsed_ms: 0,
            finished_at: Instant::now(),
            ticket: job.ticket,
            outcome,
            bytes,
            message: false,
            stream: false,
            light: true,
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
        observer(&cache.preloads[i].name, Observation::Taken(ticket));
    }
    let used = cache.touch();
    let held = &mut cache.preloads[i];
    if let ReplyState::Ready(r) = &mut held.reply {
        r.used = used;
    }
    if !held.taken {
        cache.journal.push(held.fact(Event::Taken));
        held.taken = true;
    }
}
pub(super) fn drop_held(cache: &mut cache::Cache, i: usize, event: Event) {
    let held = &mut cache.preloads[i];
    let reply = std::mem::replace(&mut held.reply, ReplyState::Gone);
    let bytes = match reply {
        ReplyState::Pending { abort, .. } => {
            abort.abort();
            0
        }
        ReplyState::Ready(r) => r.bytes,
        ReplyState::Gone => return,
    };
    if event != Event::NeverTaken || !held.taken {
        cache.journal.push(held.fact(event));
    }
    held.request = None;
    held.key = Equal::default();
    held.answered_at = None;
    cache.release(bytes);
}
// Completion wins over a later timer observation (LLP 1162.000 D1).
pub(super) fn expire(cache: &mut cache::Cache) {
    let now = (cache.clock)();
    if cache.preload_definitions_due.is_some_and(|due| now >= due) {
        cache.preload_starting = false;
        cache.preload_definitions_due = None;
    }
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
pub(super) fn refresh(cache: &mut cache::Cache, r: &RequestOut) {
    if r.class != RequestClass::Read
        || !r.request.is_plain_http()
        || !(r.forced
            || matches!(
                r.request.cache,
                exact_runner::CacheMode::Reload | exact_runner::CacheMode::NoCache
            ))
    {
        return;
    }
    let key = Equal::of(&r.request);
    for i in 0..cache.preloads.len() {
        if cache.preloads[i].key == key
            && cache.preloads[i]
                .request
                .as_ref()
                .is_some_and(|request| request.body == r.request.body)
        {
            drop_held(cache, i, Event::DroppedByRefresh);
        }
    }
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
pub(super) fn cancel_all(cache: &mut cache::Cache) {
    for i in 0..cache.preloads.len() {
        drop_held(cache, i, Event::Cancelled);
    }
}

struct Events {
    send: mpsc::Sender<Message>,
    index: usize,
    clock: Arc<dyn Fn() -> Instant + Send + Sync>,
    answered_at: Arc<Mutex<Option<Answered>>>,
    observed: Option<(String, Observer)>,
    visited: Mutex<Vec<String>>,
}
impl FetchEvents for Events {
    fn outcome(&self, result: Result<Collected, HostError>) {
        let at = (self.clock)();
        *self.answered_at.lock().unwrap() = Some(Answered {
            at,
            visited: std::mem::take(&mut *self.visited.lock().unwrap()),
        });
        if result.is_ok() {
            if let Some((name, observer)) = &self.observed {
                observer(name, Observation::Complete);
            }
        }
        let _ = self.send.send(Message::Reply(self.index, result, at));
    }
    fn next_hop(&self, url: &str, start: Box<dyn FnOnce() + Send>) {
        self.visited.lock().unwrap().push(url.into());
        let _ = self.send.send(Message::Hop(start));
    }
}

fn finish(
    cache: &mut cache::Cache,
    index: usize,
    result: Result<Collected, HostError>,
    completed: Instant,
) {
    let held = &cache.preloads[index];
    let ReplyState::Pending { due, .. } = &held.reply else {
        return;
    };
    if completed >= *due {
        drop_held(cache, index, Event::Cancelled);
        return;
    }
    let reply = match result {
        Ok(reply) => reply,
        Err(e) => {
            let event = match e {
                HostError::Denied { .. } | HostError::DeniedRedirect { .. } => Event::RefusedGrant,
                _ => Event::Failed,
            };
            drop_held(cache, index, event);
            return;
        }
    };
    let visited = std::mem::take(
        &mut held
            .answered_at
            .as_ref()
            .expect("pending exchange")
            .lock()
            .unwrap()
            .as_mut()
            .expect("completed exchange")
            .visited,
    );
    let started = held.started;
    let request = held.request.as_ref().expect("pending request");
    let mut response = Response {
        status: reply.status,
        headers: reply.headers.entries().to_vec(),
        body: reply.body,
    };
    let bytes = response_bytes(&response)
        + visited.capacity() * std::mem::size_of::<String>()
        + visited.iter().map(String::capacity).sum::<usize>()
        + std::mem::size_of::<Held>()
        + held.name.capacity()
        + held.origin.capacity()
        + held.secrets.capacity() * std::mem::size_of::<String>()
        + held.secrets.iter().map(String::capacity).sum::<usize>()
        + held.key.method.capacity()
        + held.key.url.capacity()
        + cache::header_bytes(&held.key.headers)
        + request.method.capacity()
        + request.url.capacity()
        + request.body.capacity()
        + request.grants.as_ref().map_or(0, String::capacity)
        + request.protocols.capacity() * std::mem::size_of::<String>()
        + request
            .protocols
            .iter()
            .map(String::capacity)
            .sum::<usize>()
        + cache::header_bytes(&request.headers)
        + std::mem::size_of::<Mutex<Option<Answered>>>()
        + 2 * std::mem::size_of::<usize>();
    let request = request.clone();
    if bytes > cache::MAX_ENTRY || !cache.room(bytes) {
        drop_held(cache, index, Event::Oversized);
        return;
    }
    let used = cache.touch();
    cache.preloads[index].reply = ReplyState::Ready(Ready {
        response: response.clone(),
        visited,
        completed,
        bytes,
        used,
    });
    cache.charge(bytes);
    cache::store_preload(
        cache,
        index,
        &request,
        &mut response,
        started,
        reply.redirected,
    );
}

fn run(
    destination: &Arc<Location>,
    send: &mpsc::Sender<Message>,
    receive: mpsc::Receiver<Message>,
    host: &StreamHost,
) {
    // Adapt the binding itself so every host honours its supplied transport (D4).
    let guards: Vec<_> = (0..MAX_PRELOADS)
        .map(|_| ThreadGuard::new(destination))
        .collect();
    let workers = match ibex2::stdlib::fetch_async::BoundedWorkerPool::with_worker_guards(
        guards,
        MAX_PRELOADS,
    ) {
        Ok(workers) => Arc::new(workers),
        Err(_) => {
            update(destination, cancel_all);
            return;
        }
    };
    loop {
        let delay = update(destination, |cache| {
            expire(cache);
            let now = (cache.clock)();
            cache
                .preloads
                .iter()
                .filter_map(|p| match &p.reply {
                    ReplyState::Pending { due, .. } => Some(*due),
                    ReplyState::Ready(r) => Some(r.completed + HOLD),
                    ReplyState::Gone => None,
                })
                .min()
                .map(|at| at.saturating_duration_since(now))
        });
        let Some(delay) = delay else {
            return;
        };
        let message = match delay {
            Some(delay) => receive.recv_timeout(delay),
            None => receive
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        match message {
            Ok(Message::Start(indices, grants)) => {
                start_indices(destination, send, host, &grants, &workers, indices)
            }
            Ok(Message::Reply(i, result, at)) => {
                update(destination, |c| finish(c, i, result, at));
            }
            Ok(Message::Hop(start)) => start(),
            Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Ok(Message::Tick) | Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn start_indices(
    destination: &Arc<Location>,
    send: &mpsc::Sender<Message>,
    host: &StreamHost,
    launch_grants: &str,
    workers: &Arc<ibex2::stdlib::fetch_async::BoundedWorkerPool>,
    indices: std::ops::Range<usize>,
) {
    for i in indices {
        let start = update(destination, |cache| {
            expire(cache);
            let held = &cache.preloads[i];
            let ReplyState::Pending { abort, .. } = &held.reply else {
                return None;
            };
            let request = held.request.as_ref().expect("pending request");
            if request.body.len() > MAX_BODY_BYTES || !request.is_plain_http() {
                drop_held(cache, i, Event::Refused);
                return None;
            }
            if cache::granted(launch_grants, request).is_err() {
                drop_held(cache, i, Event::RefusedGrant);
                return None;
            }
            let start = (
                request.clone(),
                abort.clone(),
                launch_grants.to_owned(),
                cache.clock.clone(),
                held.answered_at.as_ref().expect("pending exchange").clone(),
                held.observer.clone().map(|o| (held.name.clone(), o)),
            );
            cache.journal.push(held.fact(Event::Sent));
            Some(start)
        })
        .flatten();
        let Some((request, abort, grants, clock, answered_at, observed)) = start else {
            continue;
        };
        let grants = exact_data::storage::scope(&grants, request.grants.as_deref())
            .and_then(|s| ibex2::grant::GrantSet::parse(&exact_runner::io_grants(s)))
            .expect("checked grants");
        let fetch = reach::gate_fetch(host().endow(grants).fetch, request.reach);
        let transport = Arc::new(fetch.blocking_adapter(workers.clone()));
        let visited = Mutex::new(vec![request.url.clone()]);
        let mut req = fetch_request(request, false);
        req.max_body = Some(cache::MAX_ENTRY);
        // A preload is asked for by nobody yet: Low Data Mode refuses it (LLP 1162.000 D4).
        req.speculative = true;
        if let Some((name, observer)) = &observed {
            observer(name, Observation::Handed);
        }
        fetch.start_async(
            transport.clone(),
            req,
            None,
            abort.signal(),
            HeaderCaps {
                bytes: MAX_HEADERS,
                entries: 1024,
            },
            Arc::new(Events {
                send: send.clone(),
                index: i,
                clock,
                answered_at,
                observed,
                visited,
            }),
        );
    }
}

#[cfg(test)]
#[path = "preload_tests.rs"]
mod tests;
