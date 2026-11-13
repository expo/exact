//! The executor without a thread per request (LLP 1120 D3–D5): each lane has
//! a starter, the only place its work starts, and a finisher, the only place
//! its outcomes are built and its releases applied; one timer keeps every
//! deadline and one disposer destroys work let go. HTTP goes to an
//! asynchronous transport (ibex2's `fetch_async`): on Apple, one
//! `NSURLSession` per lane. The admission rules, the queues, settlement and
//! the fence are `lib.rs`'s, unchanged.
//!
//! Releases, then wakes: a callback, the timer, a reader or a buffer's
//! release only posts to the lane's finisher, which applies what the event
//! makes due under the state lock, then wakes the starter, so a starter
//! never decides on a release not yet applied. Every event names its job's
//! run (ticket and generation): a parked upload's next run ignores the last
//! one's late events.
use super::{
    body, bounded_outcome, failed, fetch_failure, fetch_request, handoff, retained, scoped_grants,
    Completed, Job, Running, Shared, State, StreamHost, BYTES, MAX_HEADERS,
};
use exact_runner::{FailureKind, Outcome, Reply, Request, Response, Work as OwnedWork};
use ibex2::boundary::HostError;
use ibex2::stdlib::abort::{AbortController, AbortRegistration};
use ibex2::stdlib::fetch_async::{
    AsyncTransport, Collected, Exchange, FetchEvents, Flow, Head, HeaderCaps, HopMetrics,
    HopRequest, Sink, Upload,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[path = "timer.rs"]
pub(super) mod timer;
#[cfg(test)]
use super::MAX_BODY;
use timer::Timer;
#[path = "async_body_to.rs"]
mod body_to;
pub(super) use body_to::release_count;
#[path = "async_file.rs"]
mod file;
#[path = "async_http.rs"]
mod http;
#[cfg(test)]
pub(super) use file::ReaderPhase;
#[cfg(test)]
#[path = "async_cache_tests.rs"]
mod cache_tests;
#[cfg(test)]
#[path = "async_read_tests.rs"]
mod read_tests;
#[cfg(test)]
#[path = "redirect_tests.rs"]
mod redirect_tests;
#[cfg(test)]
#[path = "async_retire_tests.rs"]
mod retire_tests;
#[path = "async_retire.rs"]
mod retirement;
#[path = "workers.rs"]
mod workers;
pub(super) use retirement::external_end;

/// Resumed HTTP exchanges on the shared transport, the session's per-host
/// connection limit too (LLP 1120 D2, D5): HTTP/2 multiplexes them.
pub(super) const IN_FLIGHT: usize = super::build_number(option_env!("EXACT_HTTP_IN_FLIGHT"), 32);
/// HTTP/1.1 exchanges the core resumes per origin per lane until one there
/// reports `h2` or `h3` (D3): URLSession's own limit, so it never queues.
const PER_ORIGIN: usize = 6;
/// The base set: two starters, two finishers, the timer, the disposer (D4).
const BASE: usize = 6;
/// The base set and the workers of [`portable_lanes`].
pub(super) const PORTABLE_THREADS: usize = BASE + 1 + super::INDEPENDENT;
/// The threads [`asynch_lanes`] take.
pub(super) const THREADS: usize = if cfg!(any(not(target_vendor = "apple"), test)) {
    PORTABLE_THREADS
} else {
    BASE
};

/// A job's run: its ticket and generation.
type Run = (u64, u64);

/// Who claimed a run's terminal transition, under the state lock when it
/// happened (D3): only that outcome settles it, whatever else is queued.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Claim {
    #[default]
    None,
    /// Its own outcome: a fetch's, a closure's, a handoff's, a refusal.
    Answered,
    /// A file rename has begun; its result wins over a later cancellation.
    Publishing,
    /// Its deadline.
    Timeout,
    /// A forget that cancelled a safe read.
    Cancelled,
}

/// Claim `run`'s transition for `claim`, if nothing has: whether it won.
fn claim(state: &mut State, run: Run, claim: Claim) -> bool {
    match find(&mut state.running, run) {
        Some(r) if !r.terminal && r.claim == Claim::None => {
            r.claim = claim;
            true
        }
        _ => false,
    }
}

/// The async core's bookkeeping, in `State`.
#[derive(Default)]
pub(super) struct Book {
    /// The portable blocking adapter retains one additional upload copy.
    pub(super) portable: [bool; 2],
    pub(super) retiring: bool,
    /// Live HTTP exchanges on the shared transport, and places held for ones about to
    /// start: the in-flight cap's count.
    in_flight: usize,
    /// Live HTTP/1.1 exchanges per origin, per lane, with places held.
    origins: [HashMap<String, usize>; 2],
    /// Origins an exchange reported `h2` or `h3` for, per lane.
    pub(super) multiplexed: [HashSet<String>; 2],
    /// The ordered lane's one blocking job is in flight.
    blocking: bool,
    /// File replies retain their lane place through publish or cleanup (LLP 1162 D1).
    downloads: HashSet<Run>,
    /// Admission places retained until physical cleanup ends.
    held_counts: HashSet<Run>,
    /// Each ended hop's places wait for the destination's physical release.
    download_ends: HashMap<Run, Vec<(usize, String, bool)>>,
    /// Redirects and uploads waiting for admission at their destination origin.
    hops: [VecDeque<Hop>; 2],
    /// Places held from selection until transport preparation consumes them.
    reserved: HashMap<Run, (usize, String)>,
    /// Upload bytes charged per run, released once the lent buffer has
    /// gone and the run has physically ended (D5).
    uploads: HashMap<Run, Charged>,
    /// A file job's request and what its fetch needs, while a reader sizes
    /// or reads its file.
    files: HashMap<Run, FileJob>,
    /// What a sized `exactBodyFrom` job learned: its file's size, kept
    /// while it is parked at the head (D5).
    pub(super) sized: HashMap<u64, u64>,
    /// One sizing decision per lane: successors remain queued, so a park
    /// accounts for every request buffer that could otherwise crowd it out.
    sizing: [Option<Run>; 2],
    /// A sizing reader waiting for the finisher's decision.
    deciders: HashMap<Run, mpsc::Sender<Decision>>,
    readers: HashSet<Run>,
    /// The last run's generation.
    generation: u64,
    /// Work let go under the lock (an exchange whose run settled, with an
    /// upload's bytes): destroyed by the disposer, never under the lock.
    pub(super) trash: Vec<Box<dyn Send>>,
}

impl Book {
    /// The bytes the parked uploads in `jobs` still need, which admission
    /// counts as taken (LLP 1120 §6 question 8, provisional), wherever in
    /// the queue they wait.
    pub(super) fn parked(&self, lane: usize, jobs: &VecDeque<Job>) -> usize {
        jobs.iter()
            .filter_map(|j| {
                self.sized.get(&j.ticket).map(|n| {
                    j.limit - j.charge
                        + self
                            .upload_bytes(lane, *n as usize)
                            .expect("admitted upload")
                })
            })
            .sum()
    }
    pub(super) fn reserve_inline_copy(
        &self,
        request: &Request,
        (lane, charge, limit): (usize, usize, usize),
    ) -> Result<(usize, usize, usize), &'static str> {
        let extra = if self.portable[lane] {
            request.body.len()
        } else {
            0
        };
        let limit = limit.checked_add(extra).ok_or("request size overflow")?;
        if limit > BYTES[lane] {
            return Err("native response ceiling exceeds the lane's byte budget");
        }
        Ok((lane, charge, limit))
    }
    fn upload_bytes(&self, lane: usize, size: usize) -> Option<usize> {
        size.checked_mul(if self.portable[lane] { 2 } else { 1 })
    }
}

/// An exchange for the starter to start.
struct Hop {
    run: Run,
    origin: String,
    start: Start,
}

/// An upload's charge and whether what holds it still does.
struct Charged {
    lane: usize,
    bytes: usize,
    /// The lent buffer was made, and whether it still exists: every hop
    /// that sends it holds it, the platform's copy included.
    made: bool,
    buffer: bool,
    /// Exchanges on the wire that send it.
    hops: usize,
    /// The run has physically ended.
    ended: bool,
}

impl Charged {
    fn new(lane: usize, bytes: usize) -> Charged {
        Charged {
            lane,
            bytes,
            made: false,
            buffer: false,
            hops: 0,
            ended: false,
        }
    }
}

/// What a file job's fetch needs once its file is read.
struct FileJob {
    request: Request,
    forced: bool,
    limit: usize,
    abort: AbortController,
    retiring: AbortRegistration,
    fetch: ibex2::host::Fetch,
}

/// What the finisher tells a sizing reader.
enum Decision {
    /// The whole charge is taken: read the file.
    Read,
    /// The request is settled, or parked: stop.
    Exit,
}

/// How a selected job starts, off the lock.
type Start = Box<dyn FnOnce(&Arc<Machine>) + Send>;
/// What a finisher runs after an event, off the lock.
type After = Box<dyn FnOnce(&Arc<Machine>)>;
/// A lane's transport and the grants its owner holds.
type Lane = (Arc<dyn AsyncTransport>, ibex2::host::Fetch);

/// What a finisher applies.
enum Event {
    /// A run's outcome: a fetch's, a closure's, a handoff's, the timer's.
    Outcome(Run, Done),
    /// A hop's end names its own transport lane and origin: a later hop may
    /// have changed routes (LLP 1162 D1). Readers and starts have no origin.
    Ended(Run, Option<(usize, String)>, bool, bool),
    ReaderEnded(Run),
    /// A charged upload buffer's last release, including queued disposal.
    Released(Run),
    /// An origin's protocol, from a hop's metrics.
    Protocol(usize, String, String),
    /// A sizing reader's answer.
    Sized(Run, Result<u64, Outcome>),
    /// A reader's file, or why not (`None`: it grew past its charge).
    Read(Run, Result<Vec<u8>, Option<Outcome>>),
    /// A redirect's next hop.
    Hop(Run, String, Box<dyn FnOnce() + Send>),
}

enum Done {
    /// A fetch's result, and its response ceiling.
    Fetch(Result<Collected, HostError>, usize),
    Now(Outcome),
    /// The deadline's, which claimed the transition.
    Timeout(u32),
    /// A forget's, which claimed it by cancelling a safe read.
    Cancelled,
}

/// The machine for one core: its lanes' transports, channels and threads.
pub(super) struct Machine {
    shared: Arc<Shared>,
    worker_budget: Arc<workers::Budget>,
    transports: Mutex<[Option<Lane>; 2]>,
    /// Why a request is refused on a lane with no transport.
    unbound: String,
    finishers: [Mutex<Option<mpsc::Sender<Event>>>; 2],
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    pub(super) timer: Arc<Timer>,
    grants: String,
    /// Retired: a starter starts nothing more.
    retired: AtomicBool,
    #[cfg(test)]
    pub(super) reader_hook: Mutex<Option<file::ReaderHook>>,
    #[cfg(test)]
    pub(super) body_to_hook: Mutex<Option<body_to::Hook>>,
}

/// How a lane's transport is made from its owner's bindings.
pub(super) type Lanes = Arc<
    dyn Fn(usize, &ibex2::host::Bindings, Arc<workers::Budget>) -> (Arc<dyn AsyncTransport>, bool)
        + Send
        + Sync,
>;

impl Machine {
    pub(super) fn new(
        shared: Arc<Shared>,
        owners: Vec<Option<ibex2::host::Bindings>>,
        grants: &str,
        host: StreamHost,
        lanes: &Lanes,
    ) -> Arc<Machine> {
        let parsed = || ibex2::grant::GrantSet::parse(&exact_runner::io_grants(grants));
        let unbound = match (&owners.first().cloned().flatten(), parsed()) {
            (None, Err(e)) => format!("the app's grants did not parse: {e}"),
            _ => "the app declares no grants".to_string(),
        };
        let mut owners = owners.into_iter();
        let ordered = owners.next().flatten();
        let independent = owners
            .next()
            .flatten()
            .or_else(|| super::independent_bindings(grants, &host));
        let worker_budget = Arc::new(workers::Budget::default());
        let transport = |lane: usize, b: Option<ibex2::host::Bindings>| {
            b.map(|b| {
                let (transport, portable) = lanes(lane, &b, worker_budget.clone());
                shared.state.lock().unwrap().book.portable[lane] = portable;
                (transport, b.fetch)
            })
        };
        let transports = [transport(0, ordered), transport(1, independent)];
        Arc::new(Machine {
            shared,
            transports: Mutex::new(transports),
            worker_budget,
            unbound,
            finishers: [Mutex::new(None), Mutex::new(None)],
            threads: Mutex::new(Vec::new()),
            timer: Arc::new(Timer::default()),
            grants: grants.to_string(),
            retired: AtomicBool::new(false),
            #[cfg(test)]
            reader_hook: Mutex::new(None),
            #[cfg(test)]
            body_to_hook: Mutex::new(None),
        })
    }

    /// Start the base set, each thread holding one of `slots` until it
    /// exits; false when a thread could not be made (the caller then
    /// retires the core, and the ones made exit).
    pub(super) fn start(self: &Arc<Self>, slots: Vec<super::WorkerSlot>) -> bool {
        let mut slots = slots.into_iter();
        let mut ok = true;
        let mut spawn = |name: String, body: Box<dyn FnOnce() + Send>| {
            let slot = slots.next();
            let disposer = name == "exact-dispose";
            match workers::spawn(
                name,
                Box::new(move || {
                    let _slot = slot;
                    body()
                }),
            ) {
                Ok(thread) if !disposer => self.threads.lock().unwrap().push(thread),
                Ok(_) => {}
                Err(_) => ok = false,
            }
        };
        for lane in 0..2 {
            let (send, receive) = mpsc::channel();
            *self.finishers[lane].lock().unwrap() = Some(send);
            let m = self.clone();
            spawn(
                format!("exact-finish-{lane}"),
                Box::new(move || m.finisher(lane, receive)),
            );
            let m = self.clone();
            spawn(
                format!("exact-start-{lane}"),
                Box::new(move || m.starter(lane)),
            );
        }
        let m = self.clone();
        spawn("exact-timer".into(), Box::new(move || m.timer()));
        let m = self.clone();
        spawn("exact-dispose".into(), Box::new(move || m.disposer()));
        if ok {
            *self.worker_budget.0.lock().unwrap() = slots.collect();
        } else {
            for f in &self.finishers {
                f.lock().unwrap().take();
            }
            self.threads.lock().unwrap().clear();
        }
        ok
    }

    /// `run`'s own outcome, claimed now and published by the finisher.
    fn answer(&self, lane: usize, run: Run, outcome: Outcome) {
        if claim(&mut self.shared.state.lock().unwrap(), run, Claim::Answered) {
            self.post(lane, Event::Outcome(run, Done::Now(outcome)));
        }
    }

    fn post(&self, lane: usize, event: Event) {
        let send = self.finishers[lane].lock().unwrap().clone();
        if let Some(send) = send {
            let _ = send.send(event);
        }
    }

    /// A forgotten safe read's cancellation, which claimed its transition
    /// under the state lock (`Core::forget`): published now, as Aborted.
    pub(super) fn cancelled(&self, lane: usize, run: Run) {
        self.post(lane, Event::Outcome(run, Done::Cancelled));
    }

    // ---- the starter ----

    fn starter(self: Arc<Self>, lane: usize) {
        loop {
            let starts = {
                let mut state = self.shared.state.lock().unwrap();
                loop {
                    if state.retired {
                        return;
                    }
                    let starts = self.select(&mut state, lane);
                    if !starts.is_empty() {
                        break starts;
                    }
                    state = self.shared.ready.wait(state).unwrap();
                }
            };
            for (run, start) in starts {
                let _ended = Ended(self.clone(), lane, run);
                // Started only if its run is still open: one settled meanwhile
                // (or retirement) lets it go unstarted. Either way the start's
                // own hold on the run ends with it.
                let open = !self.retired.load(Ordering::Acquire)
                    && find(&mut self.shared.state.lock().unwrap().running, run)
                        .is_some_and(|r| !r.terminal && r.claim == Claim::None);
                if open {
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| start(&self)))
                        .is_err()
                    {
                        self.answer(
                            lane,
                            run,
                            failed(FailureKind::Aborted, "native start panicked"),
                        );
                    }
                } else {
                    drop(start);
                }
            }
        }
    }

    /// What `lane` starts now, decided under the lock (D3, D5). Every
    /// exchange chosen holds its in-flight and origin places from here.
    fn select(&self, state: &mut State, lane: usize) -> Vec<(Run, Start)> {
        let mut starts: Vec<(Run, Start)> = Vec::new();
        // Waiting exchanges first: their requests already run.
        let mut at = 0;
        while at < state.book.hops[lane].len() {
            let run = state.book.hops[lane][at].run;
            if !state
                .running
                .iter()
                .any(|r| (r.ticket, r.generation) == run && !r.terminal && r.claim == Claim::None)
            {
                // Its run settled meanwhile: nothing to start.
                let hop = state.book.hops[lane].remove(at).expect("index");
                state.book.trash.push(Box::new(hop.start));
                continue;
            }
            let r = find(&mut state.running, run).expect("open");
            let previous_lane = r.transport_lane;
            let overlap = r.overlap;
            let hop = &state.book.hops[lane][at];
            // LLP 1162 D1: every shared hop can fall back when held independent
            // exchanges occupy its adapter. An ordered route keeps its slot.
            let transport_lane = if lane == 0 && overlap && previous_lane == 1 {
                shared_route(state, &hop.origin)
            } else {
                previous_lane
            };
            if (transport_lane == 0 && previous_lane != 0 && state.book.blocking)
                || (transport_lane == 1 && state.book.in_flight >= IN_FLIGHT)
                || at_limit(state, transport_lane, &hop.origin)
            {
                at += 1;
                continue;
            }
            let hop = state.book.hops[lane].remove(at).expect("index");
            hold(&mut state.book, transport_lane, hop.run, hop.origin);
            // The start holds its run until it has run (`starter`).
            let r = find(&mut state.running, hop.run).expect("open");
            r.transport_lane = transport_lane;
            r.live += 1;
            state.book.blocking |= lane == 0 && transport_lane == 0;
            starts.push((hop.run, hop.start));
        }
        let mut index = 0;
        while index < state.jobs[lane].len() {
            if lane == 0 && !super::preload::reserve_front(state) {
                break;
            }
            if super::forget::refuse_crowded(&self.shared, state, lane, index) {
                continue;
            }
            let job = &state.jobs[lane][index];
            let kind = Kind::of(job);
            if lane == 0 && super::preload::waiting(state, job) {
                index += 1;
                continue;
            }
            // Only a read passes one that waits for a launch reply (LLP 1162.000 D1).
            if lane == 0
                && ((index > 0 && job.class != exact_runner::RequestClass::Read)
                    || !super::reads::can_start(state, job))
            {
                break;
            }
            let exchange = kind == Kind::Http && job.request.body_from.is_none();
            if kind == Kind::Http
                && job.request.body_from.is_some()
                && !state.book.sized.contains_key(&job.ticket)
                && state.book.sizing[lane].is_some()
            {
                break;
            }
            let transport_lane = if lane == 0 && job.overlap {
                shared_route(state, &origin(&job.request.url))
            } else {
                lane
            };
            if transport_lane == 0 && state.book.blocking {
                break;
            }
            if exchange && transport_lane == 1 && state.book.in_flight >= IN_FLIGHT {
                break;
            }
            // A head short of bytes waits and is not passed over.
            if !fits(state, lane, job, kind) {
                break;
            }
            if exchange && at_limit(state, transport_lane, &origin(&job.request.url)) {
                // The independent lane passes over a head at its origin's
                // HTTP/1.1 limit; the ordered one waits for it (D3).
                if lane == 0 {
                    break;
                }
                index += 1;
                continue;
            }
            let start = self.take(state, lane, transport_lane, index, kind);
            starts.push(start);
            // From the head again: a head passed over may no longer fit.
            index = 0;
        }
        starts
    }

    /// Move job `index`, which fits, into `running`, charged, and say how
    /// it starts.
    fn take(
        &self,
        state: &mut State,
        lane: usize,
        transport_lane: usize,
        index: usize,
        kind: Kind,
    ) -> (Run, Start) {
        let job = state.jobs[lane].remove(index).expect("index");
        let mut job = job;
        let (more, upload) = charge(&state.book, lane, &job, kind);
        job.charge += more;
        state.bytes[lane] += more + upload;
        state.book.generation += 1;
        let run = (job.ticket, state.book.generation);
        if job.request.body_to.is_some() {
            state.book.downloads.insert(run);
        }
        let sized = state.book.sized.remove(&job.ticket).is_some();
        if kind == Kind::Http && job.request.body_from.is_some() && !sized {
            state.book.sizing[lane] = Some(run);
        }
        if sized {
            state.book.uploads.insert(run, Charged::new(lane, upload));
        }
        if kind == Kind::Http && job.request.body_from.is_none() {
            hold(
                &mut state.book,
                transport_lane,
                run,
                origin(&job.request.url),
            );
        }
        let blocking = lane == 0;
        state.book.blocking |= blocking && transport_lane == 0;
        let abort = AbortController::new();
        state.running.push(Running {
            started: Instant::now(),
            ticket: job.ticket,
            lane,
            transport_lane,
            charge: job.charge,
            limit: job.limit,
            // Only an exchange is cancelled by a forget (a safe read's).
            abort: (kind == Kind::Http && super::reads::safe_to_forget(&job.request, job.class))
                .then(|| abort.clone()),
            exchange: (kind == Kind::Http).then(|| abort.clone()),
            forgotten: job.forgotten,
            stream: false,
            waiting: false,
            read: lane == 0 && super::reads::safe_read(&job.request, job.work.is_some(), job.class),
            overlap: job.overlap,
            class: job.class,
            terminal: false,
            // The start holds its run until it has run (`starter`).
            live: 1,
            drained: false,
            blocking,
            generation: run.1,
            claim: Claim::None,
            cached: false,
            cache_exchange: None,
            cache_replacing: super::cache::replacing(
                &job.request,
                job.class,
                job.forced,
                job.work.is_some(),
            ),
            cache_barrier: super::cache::barrier(&job.request, job.class, job.work.is_some()),
            invalidates: super::cache::invalidates(&job.request, job.work.is_some()),
        });
        let start: Start =
            Box::new(move |m: &Arc<Machine>| m.run(lane, run, kind, job, sized, abort));
        (run, start)
    }

    /// Start one job, off the lock.
    fn run(
        self: &Arc<Self>,
        lane: usize,
        run: Run,
        kind: Kind,
        job: Job,
        sized: bool,
        abort: AbortController,
    ) {
        let Job {
            mut request,
            forced,
            work,
            queued_deadline,
            class,
            ..
        } = job;
        let refuse = |outcome| self.answer(lane, run, outcome);
        if let Some(deadline) = queued_deadline {
            if !deadline.remaining(&mut request) {
                return refuse(failed(
                    FailureKind::Timeout,
                    "the request timed out while queued",
                ));
            }
        }
        // A source's narrower scope, refused before any work runs, as on
        // the workers.
        let scoped = match scoped_grants(&self.grants, request.grants.as_deref()) {
            Some(Err(message)) => return refuse(failed(FailureKind::Refused, message)),
            Some(Ok(scope)) => Some(scope),
            None => None,
        };
        // Retirement aborts every job; forgetting aborts a safe read.
        let retiring = {
            let abort = abort.clone();
            self.shared.abort.signal().register(move || abort.abort())
        };
        match kind {
            Kind::Handoff => {
                let Some(OwnedWork::Later(hand)) = work else {
                    unreachable!("a handoff has its hand")
                };
                let m = self.clone();
                let reply = Reply::new(move |outcome| {
                    m.answer(lane, run, outcome);
                    drop(retiring);
                });
                // The module owner completes asynchronously; the starter stays
                // free. Reply's drop publishes Aborted if the handoff panics.
                // The call is physical work until it returns, even when its
                // reply comes inside it: the start's hold covers it.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hand(reply)));
                let mut state = self.shared.state.lock().unwrap();
                if let Some(r) = find(&mut state.running, run).filter(|r| r.blocking) {
                    r.blocking = false;
                    state.book.blocking = false;
                }
                self.shared.ready.notify_all();
            }
            Kind::Closure => {
                // Run here (a driver fault's, a continuation, storage): the
                // lane's starts wait for it (a declared change, D3).
                let work = match work {
                    Some(OwnedWork::Now(work)) => Some(work),
                    _ => None,
                };
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    closure(request, work, &abort)
                }))
                .unwrap_or_else(|_| failed(FailureKind::Aborted, "native work panicked"));
                refuse(outcome);
            }
            Kind::Http => {
                if let Some(outcome) =
                    super::cache::prepare(&self.shared, run.0, &mut request, class, forced)
                        .or_else(|| super::cache::missing(&request, forced))
                {
                    return refuse(outcome);
                }
                self.http(lane, run, request, forced, sized, abort, retiring, scoped)
            }
        }
    }

    // ---- the finisher ----

    fn finisher(self: Arc<Self>, lane: usize, events: mpsc::Receiver<Event>) {
        while let Ok(event) = events.recv() {
            let after = {
                let mut state = self.shared.state.lock().unwrap();
                if state.retired {
                    self.apply_retired(&mut state, lane, event)
                } else {
                    self.apply(&mut state, lane, event)
                }
            };
            if let Some(after) = after {
                after(&self);
            }
            // What it settled or ended may have freed the independent lane
            // room an upload waits for (LLP 1016.000 D10).
            if lane == 1 && !self.retired.load(Ordering::Acquire) {
                super::stream::start_uploads(&self.shared);
            }
            #[cfg(test)]
            self.shared
                .worker_notifications
                .fetch_add(1, Ordering::Relaxed);
            self.shared.ready.notify_all();
        }
    }

    /// Apply one event under the lock; anything to run after it, unlocked.
    fn apply(self: &Arc<Self>, state: &mut State, lane: usize, event: Event) -> Option<After> {
        match event {
            Event::Outcome(run, done) => {
                // Only the claimant settles: a fetch's outcome claims when it
                // is posted, as the timer and a forget do.
                let claimant = match &done {
                    Done::Timeout(_) => Claim::Timeout,
                    Done::Cancelled => Claim::Cancelled,
                    Done::Fetch(..) | Done::Now(_) => Claim::Answered,
                };
                if !find(&mut state.running, run)
                    .is_some_and(|r| !r.terminal && r.claim == claimant)
                {
                    state.book.trash.push(Box::new(done));
                    return None;
                }
                let outcome = match done {
                    Done::Now(outcome) => outcome,
                    Done::Timeout(ms) => failed(
                        FailureKind::Timeout,
                        format!("the request timed out after {ms} ms"),
                    ),
                    Done::Cancelled => failed(FailureKind::Aborted, "native request aborted"),
                    Done::Fetch(result, limit) => {
                        let redirected = result.as_ref().is_ok_and(|r| r.redirected);
                        let mut outcome =
                            classify(result, limit, self.shared.abort.signal().aborted());
                        if let Outcome::Response(response) = &mut outcome {
                            super::cache::response(state, run.0, response, redirected);
                        }
                        outcome
                    }
                };
                Some(self.settled(state, run, outcome))
            }
            Event::Ended(run, origin, sent, final_hop) => {
                if let Some((transport_lane, origin)) = origin {
                    let deferred = state.book.downloads.contains(&run)
                        && (final_hop
                            || find(&mut state.running, run)
                                .is_some_and(|r| r.claim != Claim::None));
                    if deferred {
                        state.book.download_ends.entry(run).or_default().push((
                            transport_lane,
                            origin,
                            sent,
                        ));
                    } else {
                        unreserve(&mut state.book, transport_lane, &origin);
                        if let Some(u) = state.book.uploads.get_mut(&run).filter(|_| sent) {
                            u.hops -= 1;
                        }
                    }
                }
                if let Some(r) = find(&mut state.running, run) {
                    r.live -= 1;
                }
                physical(state, run);
                None
            }
            Event::ReaderEnded(run) => {
                state.book.readers.remove(&run);
                self.apply(state, lane, Event::Ended(run, None, false, false))
            }
            Event::Released(run) => {
                if let Some(u) = state.book.uploads.get_mut(&run) {
                    u.buffer = false;
                }
                release_upload(state, run);
                None
            }
            Event::Protocol(transport_lane, origin, protocol) => {
                if matches!(protocol.as_str(), "h2" | "h3") {
                    state.book.multiplexed[transport_lane].insert(origin);
                } else {
                    state.book.multiplexed[transport_lane].remove(&origin);
                }
                None
            }
            Event::Sized(run, size) => self.sized(state, lane, run, size),
            Event::Read(run, read) => self.read(state, lane, run, read),
            Event::Hop(run, url, start) => {
                let open = find(&mut state.running, run)
                    .is_some_and(|r| !r.terminal && r.claim == Claim::None);
                if !open {
                    state.book.trash.push(Box::new(start));
                    return None;
                }
                state.book.hops[lane].push_back(Hop {
                    run,
                    origin: origin(&url),
                    start: Box::new(move |_| start()),
                });
                None
            }
        }
    }

    /// Settle `run`, its claimant's `outcome`; its deadline goes after the
    /// lock.
    fn settled(&self, state: &mut State, run: Run, outcome: Outcome) -> After {
        settle(state, run, outcome);
        if let Some(decider) = state.book.deciders.remove(&run) {
            let _ = decider.send(Decision::Exit);
        }
        Box::new(move |m: &Arc<Machine>| m.timer.remove(run))
    }

    /// A decision the finisher makes itself (a sizing or reading failure,
    /// a refusal): it claims the transition, if nothing has, and settles it.
    fn settle(&self, state: &mut State, run: Run, outcome: Outcome) -> After {
        if claim(state, run, Claim::Answered) {
            settle(state, run, outcome);
        } else {
            state.book.trash.push(Box::new(outcome));
        }
        if let Some(decider) = state.book.deciders.remove(&run) {
            let _ = decider.send(Decision::Exit);
        }
        Box::new(move |m: &Arc<Machine>| m.timer.remove(run))
    }

    // ---- the timer and the disposer ----

    fn timer(self: Arc<Self>) {
        while let Some(entry) = self.timer.next() {
            // The claim is made under the lock: from here only the timer's
            // outcome settles the run, whatever is already queued.
            let claimed = {
                let mut state = self.shared.state.lock().unwrap();
                if entry.run.1 == 0 {
                    if !state.retired {
                        super::forget::expire_queued(
                            &self.shared,
                            &mut state,
                            entry.lane,
                            entry.run.0,
                            entry.ms,
                        );
                    }
                    false
                } else {
                    claim(&mut state, entry.run, Claim::Timeout)
                }
            };
            if !claimed {
                continue;
            }
            self.post(
                entry.lane,
                Event::Outcome(entry.run, Done::Timeout(entry.ms)),
            );
            entry.abort.abort();
        }
    }
}

/// Whether `Core::start_switched` (Apple's executor) and a test core run
/// the asynchronous machine (LLP 1120): off until the gate (§5) decides;
/// `EXACT_HTTP_ASYNC=1` at build or run turns it on. Linux and the render
/// host keep their workers until stage 2.
pub(super) fn asynchronous() -> bool {
    option_env!("EXACT_HTTP_ASYNC") == Some("1")
        || std::env::var("EXACT_HTTP_ASYNC").is_ok_and(|v| v == "1")
}

/// Each lane's transport: Apple's session per lane, or, in tests, the
/// owner's blocking transport on bounded, reserved workers.
pub(super) fn asynch_lanes() -> Lanes {
    #[cfg(all(target_vendor = "apple", not(test)))]
    {
        Arc::new(|lane, _, _| {
            let per_host = if lane == 1 { IN_FLIGHT } else { 0 };
            (
                Arc::new(ibex2::transport::darwin_async::DarwinLane::new(per_host)),
                false,
            )
        })
    }
    #[cfg(any(not(target_vendor = "apple"), test))]
    {
        portable_lanes()
    }
}

/// Each lane's transport the owner's blocking one, on bounded, reserved
/// workers.
pub(super) fn portable_lanes() -> Lanes {
    Arc::new(|lane, bindings: &ibex2::host::Bindings, budget| {
        let threads = if lane == 0 { 1 } else { super::INDEPENDENT };
        let workers = Arc::new(workers::Workers::new(threads, budget));
        (Arc::new(bindings.fetch.blocking_adapter(workers)), true)
    })
}

/// What a job is to the starter.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A module's handoff (`native.later`, a continuation): called, returns.
    Handoff,
    /// Work the starter runs: a closure, storage, a continuation's own.
    Closure,
    /// An HTTP exchange on the lane's transport.
    Http,
}

impl Kind {
    fn of(job: &Job) -> Kind {
        if handoff(&job.request) && matches!(job.work, Some(OwnedWork::Later(_))) {
            Kind::Handoff
        } else if job.work.is_some()
            || job.request.storage.is_some()
            || job.request.continuation.is_some()
        {
            Kind::Closure
        } else {
            Kind::Http
        }
    }
}

/// `scheme://host:port`, the per-origin limit's key.
fn origin(url: &str) -> String {
    ibex2::stdlib::fetch::origin_of(url).map_or_else(
        |_| url.to_string(),
        |o| format!("{}://{}:{}", o.scheme, o.host, o.port),
    )
}

/// `origin` is at its HTTP/1.1 limit on `lane` (D3).
fn at_limit(state: &State, lane: usize, origin: &str) -> bool {
    !state.book.multiplexed[lane].contains(origin)
        && state.book.origins[lane].get(origin).copied().unwrap_or(0) >= PER_ORIGIN
}

/// What starting `job` charges more: response bytes, and a sized upload's
/// file. A handoff charges no response bytes, a job sizing its file only
/// the request buffers it holds (D5).
fn charge(book: &Book, lane: usize, job: &Job, kind: Kind) -> (usize, usize) {
    let sized = book.sized.get(&job.ticket).map(|n| *n as usize);
    let more = match kind {
        Kind::Handoff => 0,
        Kind::Http if job.request.body_from.is_some() && sized.is_none() => 0,
        _ => job.limit - job.charge,
    };
    (
        more,
        book.upload_bytes(lane, sized.unwrap_or(0))
            .expect("admitted upload"),
    )
}

/// Whether `job` fits the lane's free bytes now.
fn fits(state: &State, lane: usize, job: &Job, kind: Kind) -> bool {
    let (more, upload) = charge(&state.book, lane, job, kind);
    more + upload <= BYTES[lane].saturating_sub(state.bytes[lane])
}

/// The same shared admission for a read's first hop and its redirects (LLP 1162 D1).
fn shared_route(state: &State, origin: &str) -> usize {
    let capacity = if state.book.portable[1] {
        IN_FLIGHT.min(super::INDEPENDENT)
    } else {
        IN_FLIGHT
    };
    usize::from(state.book.in_flight < capacity && !at_limit(state, 1, origin))
}

/// Hold the exchange's shared and origin places before its start can run.
fn hold(book: &mut Book, lane: usize, run: Run, origin: String) {
    *book.origins[lane].entry(origin.clone()).or_default() += 1;
    if lane == 1 {
        book.in_flight += 1;
    }
    book.reserved.insert(run, (lane, origin));
}

/// Give back one exchange's shared and origin places on its actual transport.
fn unreserve(book: &mut Book, lane: usize, origin: &str) {
    if let Some(n) = book.origins[lane].get_mut(origin) {
        *n -= 1;
        if *n == 0 {
            book.origins[lane].remove(origin);
        }
    }
    if lane == 1 {
        book.in_flight -= 1;
    }
}

fn find(running: &mut [Running], run: Run) -> Option<&mut Running> {
    running
        .iter_mut()
        .find(|r| r.ticket == run.0 && r.generation == run.1)
}

/// What a closure job answers: `execute`'s non-HTTP arms.
fn closure(request: Request, work: Option<super::Work>, abort: &AbortController) -> Outcome {
    if abort.signal().aborted() {
        return failed(FailureKind::Aborted, "native request aborted");
    }
    if request.storage.is_some() {
        return failed(
            FailureKind::Unsupported,
            "storage requires an app storage adapter",
        );
    }
    match work {
        Some(work) => work(),
        None => failed(
            FailureKind::Unsupported,
            "missing or consumed native continuation",
        ),
    }
}

/// A fetch's own result as the runner's outcome, as `execute` words it.
/// Only a claimed transition is a timeout or a forget's Aborted; a fetch
/// aborted otherwise was retired.
fn classify(result: Result<Collected, HostError>, limit: usize, retired: bool) -> Outcome {
    match result {
        Ok(r) => Outcome::Response(Response {
            status: r.status,
            headers: r.headers.entries().to_vec(),
            body: r.body,
        }),
        // @ref LLP 1109 D3 — over its size limit is the host refusing it.
        Err(e) if !retired && e == ibex2::stdlib::fetch::over_limit(limit) => {
            failed(FailureKind::Refused, "HTTP response exceeds limit")
        }
        Err(e) => {
            let abort = AbortController::new();
            if retired || e == ibex2::stdlib::fetch_async::aborted() {
                abort.abort();
            }
            fetch_failure(e, &abort)
        }
    }
}

/// The terminal transition of `run` (LLP 1120 D3): its outcome is
/// published, the ordered slot released; its charge stays until its
/// physical end too (D5's later-of rule).
fn settle(state: &mut State, run: Run, outcome: Outcome) {
    let Some(r) = find(&mut state.running, run) else {
        state.book.trash.push(Box::new(outcome));
        return;
    };
    if r.terminal {
        state.book.trash.push(Box::new(outcome));
        return;
    }
    r.terminal = true;
    let (lane, limit, read, forgotten, cached) = (r.lane, r.limit, r.read, r.forgotten, r.cached);
    let invalidates = r.invalidates.take();
    let finished_at = std::time::Instant::now();
    let elapsed = finished_at
        .duration_since(r.started)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let slot = r.blocking && r.transport_lane == 0 && !state.book.downloads.contains(&run);
    if !state.book.downloads.contains(&run) {
        r.blocking = false;
    }
    if slot {
        state.book.blocking = false;
    }
    let outcome = bounded_outcome(outcome, limit);
    super::cache::invalidate(state, invalidates.as_ref(), &outcome);
    if forgotten {
        state.book.trash.push(Box::new(outcome));
        if release_count(
            &mut state.book,
            find(&mut state.running, run).expect("found"),
        ) {
            state.counts[lane] -= 1;
        }
    } else {
        state.light += usize::from(read);
        state.completed[lane].push_back(Completed {
            cached,
            finished_at,
            elapsed_ms: elapsed,
            ticket: run.0,
            outcome,
            // Charged to the running job until its physical end.
            bytes: 0,
            message: false,
            stream: false,
            light: read,
            again: false,
            reply: false,
        });
    }
    physical(state, run);
    super::cache::serve_ready(state);
    if super::has_ready(state)
        || (lane == 0 && state.counts[0] == 0)
        || (read && !state.ordered_barrier && !state.fenced.is_empty())
        || (forgotten && lane == 0 && !state.fenced.is_empty())
    {
        super::wake(state);
    }
}

/// If `run` is terminal and nothing physical runs, its charge moves to its
/// undrained outcome (independent: the running charge, as `complete`;
/// ordered: what it retains), or goes if the outcome is gone (D5).
fn physical(state: &mut State, run: Run) {
    let Some(at) = state
        .running
        .iter()
        .position(|r| (r.ticket, r.generation) == run)
    else {
        return;
    };
    let r = &state.running[at];
    if !r.terminal || r.live > 0 {
        return;
    }
    let r = state.running.swap_remove(at);
    state.book.downloads.remove(&run);
    if r.blocking && r.transport_lane == 0 {
        state.book.blocking = false;
    }
    let lane = r.lane;
    if state.book.held_counts.remove(&run) {
        state.counts[lane] -= 1;
        if super::has_ready(state)
            || (lane == 0 && (state.counts[0] == 0 || !state.fenced.is_empty()))
        {
            super::wake(state);
        }
    }
    if let Some(ends) = state.book.download_ends.remove(&run) {
        for (transport_lane, origin, sent) in ends {
            unreserve(&mut state.book, transport_lane, &origin);
            if let Some(u) = state.book.uploads.get_mut(&run).filter(|_| sent) {
                u.hops -= 1;
            }
        }
    }
    if state.book.sizing[lane] == Some(run) {
        state.book.sizing[lane] = None;
    }
    // An exchange chosen that never started gives back its places.
    if let Some((transport_lane, origin)) = state.book.reserved.remove(&run) {
        unreserve(&mut state.book, transport_lane, &origin);
    }
    let (gone, kept): (VecDeque<Hop>, VecDeque<Hop>) = std::mem::take(&mut state.book.hops[lane])
        .into_iter()
        .partition(|h| h.run == run);
    state.book.hops[lane] = kept;
    state
        .book
        .trash
        .extend(gone.into_iter().map(|h| Box::new(h.start) as Box<dyn Send>));
    if let Some(file) = state.book.files.remove(&run) {
        state.book.trash.push(Box::new(file));
    }
    let kept = if r.drained {
        0
    } else {
        let done = state.completed[lane]
            .iter_mut()
            .find(|d| d.ticket == run.0 && !d.message);
        match done {
            Some(done) => {
                done.bytes = if lane == 0 {
                    retained(&done.outcome) + std::mem::size_of::<Completed>()
                } else {
                    r.charge
                };
                done.bytes
            }
            None => 0,
        }
    };
    state.bytes[lane] = state.bytes[lane] + kept - r.charge;
    if let Some(u) = state.book.uploads.get_mut(&run) {
        u.ended = true;
    }
    release_upload(state, run);
}

/// A run's upload charge goes once its buffer is gone and no exchange
/// sending it is still on the wire: the later of its last release and the
/// last sending hop's physical end (D5). A buffer never made (a failed
/// read, a timeout first) goes with the run's physical end.
fn release_upload(state: &mut State, run: Run) {
    if state.book.uploads.get(&run).is_some_and(|u| {
        if u.made {
            !u.buffer && u.hops == 0
        } else {
            u.ended
        }
    }) {
        let u = state.book.uploads.remove(&run).expect("checked");
        state.bytes[u.lane] -= u.bytes;
    }
}

fn body_refused(message: String) -> Outcome {
    failed(FailureKind::Refused, message)
}

/// A drained outcome whose job still runs physically: its bytes go with it.
pub(super) fn drained(state: &mut State, ticket: u64) -> bool {
    state
        .running
        .iter_mut()
        .find(|r| r.ticket == ticket)
        .is_none_or(|run| release_count(&mut state.book, run))
}

/// Posts a reader's end when it leaves, however it leaves.
struct Ended(Arc<Machine>, usize, Run);
impl Drop for Ended {
    fn drop(&mut self) {
        self.0
            .post(self.1, Event::Ended(self.2, None, false, false));
    }
}

/// A forget cancels `run` if it is an exchange still open (a safe read's,
/// or any method's the app aborted, `by_app`; never a stream's, nor an
/// upload's waiting for its start, which its queue ends): its terminal
/// transition is claimed now (D3).
pub(super) fn cancel(run: &mut Running, by_app: bool) -> bool {
    let open = !run.terminal && run.claim == Claim::None;
    let exchange = run.abort.is_some() || (by_app && run.exchange.is_some());
    let cancels = open && exchange && !run.stream && !run.waiting;
    if cancels {
        run.claim = Claim::Cancelled;
    }
    cancels
}
