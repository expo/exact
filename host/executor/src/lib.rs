//! The native request executor the Apple, Linux and render hosts share
//! (LLP 1041 D1–D4): grant-checked transport, the ordered and independent
//! lanes, streams, sockets, uploads and child processes. Reads and parallel
//! sends overlap on shared workers, retaining their place in the ordered
//! lane (LLP 1162 D1). Each worker owns its transport.
//!
//! A host wraps [`Core`] in an executor of its own, which gives it a wake
//! and transports and decides which [`WorkerBudget`] its workers count against.
#![deny(unsafe_code)]
#![deny(missing_docs)]
use exact_runner::{
    CompletionTiming, FailureKind, HttpScheduling, Outcome, Reply, Request, RequestClass,
    RequestOut, Response, Work as OwnedWork,
};
use ibex2::stdlib::abort::AbortController;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};

/// Shared HTTP workers, each with its own transport: a browser's six
/// connections a host, so a source's `Promise.all` of reads overlaps.
const INDEPENDENT: usize = build_number(option_env!("EXACT_HTTP_WORKERS"), 6);
/// The ordered owner and the independent ones.
const WORKERS: usize = 1 + INDEPENDENT;
/// Sixteen cores' workers; includes retired workers until they actually exit.
const MAX_WORKERS: usize = 16 * WORKERS;
const COUNTS: [usize; 2] = [16, 128];
const ORDERED_READS: usize = 128;
/// Re-ask markers in the ordered sequence at once (LLP 1041 §8.4, amended
/// 2026-10-09): their own window beside the 128 real tickets, so that
/// answers waiting on shared work never take the room that work needs.
const AGAINS: usize = 128;
/// What an `Again` marker retains until drained: its record, an empty body.
const AGAIN_BYTES: usize = std::mem::size_of::<Completed>();
const ORDERED_WAITING_BYTES: usize = 64 << 20;
/// Open streams, bounded apart from the independent lane's count: a stream
/// holds a transport lease and a reader thread for as long as it is open,
/// and must neither starve nor be starved by a lane of held replies
/// (LLP 1016.000 D4; LLP 1069.004 As built). Its bytes stay on the lane.
const STREAMS: usize = 16;
/// The independent lane holds the largest job it admits: a 64 MiB reply
/// beside the most request buffers, alone.
const BYTES: [usize; 2] = [512 << 20, MAX_REQUEST + MAX_BODY + MAX_HEADERS * 2];
const MAX_REQUEST: usize = 4 << 20;
const MAX_BODY: usize = 64 << 20;
const MAX_HEADERS: usize = 64 << 10;

type Work = Box<dyn FnOnce() -> Outcome + Send>;
type Wake = Box<dyn Fn() + Send + Sync>;
struct Job {
    ticket: u64,
    request: Request,
    forced: bool,
    work: Option<OwnedWork>,
    /// Charged now: the request's own buffers while an ordered job waits,
    /// the whole ceiling once it runs or while independent work is admitted.
    charge: usize,
    /// Request buffers in the backlog, excluding a preload waiter's reply reservation.
    waiting_charge: usize,
    /// The ceiling: request buffers plus the response allowance.
    limit: usize,
    /// The runner no longer wants its reply (`Core::forget`).
    forgotten: bool,
    queued_deadline: Option<forget::QueuedDeadline>,
    overlap: bool,
    class: RequestClass,
}
/// A job a worker took, until its outcome is complete.
struct Running {
    started: std::time::Instant,
    ticket: u64,
    lane: usize,
    /// The machine's transport; delivery and bytes stay on `lane` (LLP 1162 D1).
    transport_lane: usize,
    charge: usize,
    limit: usize,
    /// A safe read's own abort, which forgetting fires; other work has none.
    abort: Option<AbortController>,
    /// Any plain HTTP exchange's abort, whatever its method: what an app's
    /// explicit abort fires ([`Core::cancel`]). None for work and streams.
    exchange: Option<AbortController>,
    forgotten: bool,
    stream: bool,
    /// An upload waiting for its start (`stream::Uploads`), or a process
    /// one-shot, which has room of its own (`process::admit_one_shot`):
    /// it holds no count or bytes of its lane until then, or at all.
    waiting: bool,
    /// An ordered safe read: once complete it holds no effect slot.
    read: bool,
    overlap: bool,
    class: RequestClass,
    /// The asynchronous core's (LLP 1120 D3, D5); workers leave them be.
    /// The outcome is published.
    terminal: bool,
    /// Physical work still running: hops on the wire, a reader.
    live: usize,
    /// Its outcome is drained or dropped while `live` held its bytes.
    drained: bool,
    /// Its end still holds dependent requests (LLP 1162 D1).
    blocking: bool,
    /// Its run's generation (a parked upload starts a new one): events and
    /// deadlines of another are stale.
    generation: u64,
    /// Who claimed the terminal transition: only its outcome settles it.
    claim: asynch::Claim,
    cached: bool,
    cache_exchange: Option<cache::Exchange>,
    cache_barrier: bool,
    cache_replacing: Option<cache::Key>,
    invalidates: Option<(String, bool)>,
}
struct Completed {
    cached: bool,
    elapsed_ms: u64,
    finished_at: std::time::Instant,
    ticket: u64,
    outcome: Outcome,
    bytes: usize,
    /// One message of a stream that is still open (LLP 1016.000): its
    /// reservation stays with the running stream until the stream ends.
    message: bool,
    /// A stream's last outcome: draining it frees a stream slot.
    stream: bool,
    /// An ordered outcome that holds no effect slot until drained: a
    /// re-ask's marker or a finished safe read (counted in `light`).
    light: bool,
    /// A re-ask's marker (counted in `agains`).
    again: bool,
    /// A long poll's reply that is no event (LLP 1016.000 D8,
    /// `LONG_POLL_REPLY`): an event that follows takes its place.
    reply: bool,
}
/// A place behind the ordered barrier.
enum Fenced {
    /// A request, its retained buffers, and whether its outcome is forgotten.
    Held(Box<RequestOut>, Option<OwnedWork>, usize, bool),
    /// A request refused while the barrier was up: refused when its turn
    /// comes, after the held requests before it are admitted.
    Refused(u64, &'static str),
}
impl Fenced {
    fn ticket(&self) -> u64 {
        match self {
            Fenced::Held(r, ..) => r.ticket,
            Fenced::Refused(ticket, _) => *ticket,
        }
    }
}
#[derive(Default)]
struct State {
    dispatched: bool,
    cache: Option<cache::Cache>,
    jobs: [VecDeque<Job>; 2],
    running: Vec<Running>,
    completed: [VecDeque<Completed>; 2],
    /// Every admitted ticket until drained or forgotten: queued, running,
    /// complete. `counts[0] == 0` is the ordered lane's idle (refusals
    /// settle only then, and the render host lifts its fence).
    counts: [usize; 2],
    /// Of `counts[0]`, the complete outcomes that hold no effect slot: re-ask
    /// markers and finished safe reads. The sixteen counts the rest.
    light: usize,
    /// Of `light`, re-ask markers: their own window of 128, so that the
    /// 128 real tickets are `counts[0] - agains`.
    agains: usize,
    /// Re-asks not yet in the ordered sequence, oldest first: their window
    /// was full, or a refusal is retained (a re-ask never takes the fence's
    /// room for real requests, nor settles before the refusal). One record
    /// per ticket, eight bytes: bounded by the runner's current tickets, one
    /// per target, and never refused for room.
    waiting: VecDeque<u64>,
    /// The tickets in `waiting`, to keep one record each.
    waiting_set: std::collections::HashSet<u64>,
    /// Admitted streams, also counted in `counts[1]`.
    streams: usize,
    bytes: [usize; 2],
    next: usize,
    ordered: VecDeque<u64>,
    retired: bool,
    ordered_barrier: bool,
    /// Ordered requests behind the barrier, in order: held rather than
    /// refused with the refusal before them, and admitted when the host
    /// lifts it (`resume_ordered`), or effect slots free for waiting reads
    /// (LLP 1162 D1). Held
    /// requests are capped at 128 and 64 MiB of request buffers; one refused
    /// meanwhile (invalid, or past those caps) keeps its place as a refusal,
    /// so that it settles after the held work before it.
    fenced: VecDeque<Fenced>,
    fenced_bytes: usize,
    /// Work let go on the host thread (a held request forgotten or refused
    /// when the fence lifts), for a worker to destroy, as queued jobs are.
    discard: Vec<OwnedWork>,
    notified: bool,
    wake: Option<Wake>,
    /// The asynchronous core's bookkeeping (LLP 1120); idle with workers.
    book: asynch::Book,
}
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
    /// Notifications issued by the executor, independent of wait scheduling.
    #[cfg(test)]
    worker_notifications: AtomicUsize,
    #[cfg(test)]
    queued_deadlines: Arc<AtomicUsize>,
    abort: AbortController,
    /// Where the app's files are, as the host names them; unset on a host or
    /// drive that has none.
    root_paths: std::sync::OnceLock<[std::path::PathBuf; 3]>,
    /// Their directory handles, for a body read from or written to one
    /// (`Request::body_from`, `body_to`): opened at the first such request and pinned
    /// from then on, as storage's are, or why they would not open. Not at
    /// boot: opening makes the directories, and a cold boot opens no app
    /// storage (fieldnotes' `apple_module_replacement_…` test).
    roots: std::sync::OnceLock<Result<ibex2::stdlib::app_fs::AppDirectories, String>>,
    /// Open two-way sockets by stream ticket (LLP 1016.000 D7; `stream.rs`).
    sockets: Mutex<std::collections::HashMap<u64, stream::Socket>>,
    /// Long polls waiting to ask again, by stream ticket (LLP 1016.000 D8;
    /// `longpoll.rs`): the attempt, when it asks, and what failed.
    retrying: Mutex<std::collections::HashMap<u64, (u32, std::time::Instant, String)>>,
    /// What upload streams hold in memory (LLP 1016.000 D10; `stream.rs`).
    uploads: stream::Uploads,
    /// This, for a path that frees room uploads wait for holding only a
    /// reference (`complete`): it starts them (`stream::start_uploads`).
    me: std::sync::Weak<Shared>,
    /// Child processes (LLP 1133.007; `process.rs`).
    processes: process::Processes,
}

impl Shared {
    /// The app's directories for `request`: opened by the first request,
    /// worker's or stream's, whose body is or goes to an app file; `None` for any other
    /// request, which leaves them unopened, or when the host named none.
    pub(crate) fn roots_for(&self, request: &Request) -> body::Roots<'_> {
        request.body_from.as_ref().or(request.body_to.as_ref())?;
        let paths = self.root_paths.get()?;
        Some(self.roots.get_or_init(|| body::open(paths)))
    }
}

/// Count/byte reservations last until the UI takes the result, not merely
/// until transport finishes. Rejections return directly to the host: there
/// is no unbounded queue of overload failures. Byte reservations cover owned
/// request/result buffers, NOT arbitrary closure captures or allocations during
/// native work. Those trusted-source costs are count/worker bounded only.
/// On both lanes a waiting job is charged its request buffers, a running one
/// its response ceiling, and a completed one what it retains; a worker waits
/// for bytes unless open streams leave no room for its ceiling
/// (LLP 1041 §8.4, LLP 1054.000 R3). The ordered queue also bounds waiting
/// request buffers, leaving room to start its head.
pub struct Core {
    shared: Arc<Shared>,
    disabled: bool,
    /// The workers not started yet: each owner's bindings and its slot.
    /// They start at the first request admitted (an app that never asks for
    /// anything carries no threads); `None` once they have, or could not.
    idle: Mutex<Option<Vec<Starter>>>,
    /// A worker could not be started: as `disabled` from then on.
    failed: std::sync::atomic::AtomicBool,
    /// The app's grants, for a stream's own transport.
    grants: String,
    /// A stream's own transport (the platform's; a test's scripted one).
    stream_host: StreamHost,
    /// The asynchronous core instead of workers (LLP 1120), not yet started.
    machine: Option<Arc<asynch::Machine>>,
    preloads: Mutex<Option<preload::Preloads>>,
}
/// Makes the host (and its transport) an owner or a stream fetches
/// through: the platform's, unless the embedder names another.
pub type StreamHost = Arc<dyn Fn() -> ibex2::host::Host + Send + Sync>;

/// The ordered owner (the boot's bindings) and the independent ones. The
/// boot endowed its bindings with the data module's grants, before the plan
/// was read; their fetch takes `grants`, the app's, which add the origins
/// its declared sources carry (LLP 1162.001 D3).
fn owners(
    bindings: Option<ibex2::host::Bindings>,
    grants: &str,
) -> Vec<Option<ibex2::host::Bindings>> {
    let bindings = bindings.map(|mut b| {
        if let Ok(set) = ibex2::grant::GrantSet::parse(&exact_runner::io_grants(grants)) {
            b.fetch = b.fetch.with_grants(set);
        }
        b
    });
    std::iter::once(bindings)
        .chain(std::iter::repeat_with(|| None).take(INDEPENDENT))
        .collect()
}

impl Core {
    /// One ordered owner and six shared HTTP owners on the platform's
    /// transport, or, switched on, the machine (LLP 1120): Apple's.
    pub fn start_switched(
        bindings: Option<ibex2::host::Bindings>,
        grants: &str,
        wake: Wake,
        budget: WorkerBudget,
    ) -> Self {
        let host: StreamHost = Arc::new(ibex2::host::Host::new);
        let lanes = asynchronous().then(|| (asynch_lanes(), asynch::THREADS));
        Self::assemble(owners(bindings, grants), grants, wake, host, lanes, budget)
    }

    /// The workers, with every transport the other owners, scoped grants
    /// and streams open made by `host` (the Linux host's, and the render
    /// host's, LLP 1048.000 D10).
    pub fn start_on(
        bindings: Option<ibex2::host::Bindings>,
        grants: &str,
        wake: Wake,
        host: StreamHost,
        budget: WorkerBudget,
    ) -> Self {
        Self::assemble(owners(bindings, grants), grants, wake, host, None, budget)
    }

    /// For a host's tests (Apple's launch tests, LLP 1162.000 D4):
    /// [`Core::start_switched`]'s owners, and, when `asynchronous`, the
    /// machine (LLP 1120) with each lane on its owner's transport over
    /// bounded workers on every target, as this crate's own tests run it.
    pub fn test_start(
        asynchronous: bool,
        bindings: Option<ibex2::host::Bindings>,
        grants: &str,
        wake: Wake,
        budget: WorkerBudget,
    ) -> Self {
        let host: StreamHost = Arc::new(ibex2::host::Host::new);
        let lanes = asynchronous.then(|| (asynch::portable_lanes(), asynch::PORTABLE_THREADS));
        Self::assemble(owners(bindings, grants), grants, wake, host, lanes, budget)
    }

    #[cfg(test)]
    fn start(bindings: Option<ibex2::host::Bindings>, grants: &str, wake: Wake) -> Self {
        // Additional transports are constructed on their own threads, not
        // while the presenter is trying to publish its first frame.
        Self::with_owners_on(
            owners(bindings, grants),
            grants,
            wake,
            Arc::new(ibex2::host::Host::new),
        )
    }

    #[cfg(test)]
    fn with_owners(owners: Vec<Option<ibex2::host::Bindings>>, grants: &str, wake: Wake) -> Self {
        let host: StreamHost = Arc::new(ibex2::host::Host::new);
        let lanes = asynchronous().then(asynch_lanes);
        Self::with_owners_lanes(owners, grants, wake, host, lanes)
    }

    #[cfg(test)]
    fn with_owners_on(
        owners: Vec<Option<ibex2::host::Bindings>>,
        grants: &str,
        wake: Wake,
        host: StreamHost,
    ) -> Self {
        Self::with_owners_lanes(owners, grants, wake, host, None)
    }

    /// A test's core: its workers count against the test thread's budget.
    #[cfg(test)]
    fn with_owners_lanes(
        owners: Vec<Option<ibex2::host::Bindings>>,
        grants: &str,
        wake: Wake,
        host: StreamHost,
        lanes: Option<asynch::Lanes>,
    ) -> Self {
        let lanes = lanes.map(|lanes| (lanes, asynch::THREADS));
        Self::assemble(owners, grants, wake, host, lanes, WorkerBudget::Thread)
    }

    /// The core, on workers or, given `lanes` and the threads they take,
    /// the asynchronous machine.
    fn assemble(
        owners: Vec<Option<ibex2::host::Bindings>>,
        grants: &str,
        wake: Wake,
        host: StreamHost,
        lanes: Option<(asynch::Lanes, usize)>,
        budget: WorkerBudget,
    ) -> Self {
        let shared = Arc::new_cyclic(|me| Shared {
            state: Mutex::new(State {
                wake: Some(wake),
                ..State::default()
            }),
            ready: Condvar::new(),
            #[cfg(test)]
            worker_notifications: AtomicUsize::new(0),
            #[cfg(test)]
            queued_deadlines: Arc::default(),
            abort: AbortController::new(),
            root_paths: std::sync::OnceLock::new(),
            roots: std::sync::OnceLock::new(),
            sockets: Mutex::default(),
            retrying: Mutex::default(),
            uploads: stream::Uploads::default(),
            me: me.clone(),
            processes: process::Processes::new(),
        });
        let (lanes, count) = match lanes {
            Some((lanes, threads)) => (Some(lanes), threads),
            None => (None, owners.len()),
        };
        let budget = budget.live();
        let reserved = budget
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n + count <= MAX_WORKERS).then_some(n + count)
            })
            .is_ok();
        let machine = lanes.map(|lanes| {
            asynch::Machine::new(shared.clone(), owners.clone(), grants, host.clone(), &lanes)
        });
        // Reserved now, so a core past the bound is refused at once; started
        // by the first request ([`Core::start_workers`]).
        // One slot a thread: the workers', or the machine's base set.
        let owners = match machine {
            Some(_) => (0..count).map(|_| None).collect(),
            None => owners,
        };
        let idle = reserved.then(|| {
            owners
                .into_iter()
                .enumerate()
                .map(|(index, bindings)| (index, bindings, WorkerSlot(budget.clone())))
                .collect()
        });
        Self {
            shared,
            disabled: !reserved,
            idle: Mutex::new(idle),
            failed: std::sync::atomic::AtomicBool::new(false),
            grants: grants.to_string(),
            stream_host: host,
            machine,
            preloads: Mutex::new(None),
        }
    }

    /// Start `lane`'s workers, once, when the first request for it is
    /// admitted: the ordered owner, or the independent lane's six, which an
    /// app that reads only its own files never needs. False when a thread
    /// could not be made: no job is admitted then, and the workers that did
    /// start retire with the core.
    fn start_workers(&self, state: &mut State, lane: usize) -> bool {
        let mut waiting = self.idle.lock().unwrap();
        let Some(starters) = waiting.as_mut() else {
            return !self.failed.load(Ordering::Acquire);
        };
        let idle = if self.machine.is_some() {
            std::mem::take(starters)
        } else {
            let (now, later) = std::mem::take(starters)
                .into_iter()
                .partition(|(index, _, _)| usize::from(*index != 0) == lane);
            *starters = later;
            now
        };
        if starters.is_empty() {
            *waiting = None;
        }
        drop(waiting);
        if let Some(machine) = &self.machine {
            // The base set holds the slots the workers would have.
            let guards: Vec<_> = idle.into_iter().map(|(_, _, guard)| guard).collect();
            if !machine.start(guards) {
                // The threads made exit now, with their slots: this core
                // takes no work.
                self.failed.store(true, Ordering::Release);
                state.retired = true;
                #[cfg(test)]
                self.shared
                    .worker_notifications
                    .fetch_add(1, Ordering::Relaxed);
                self.shared.ready.notify_all();
                machine.stop(state);
            }
            return !self.failed.load(Ordering::Acquire);
        }
        for (index, bindings, guard) in idle {
            let shared = self.shared.clone();
            let grants = self.grants.clone();
            let host = self.stream_host.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("exact-io-{index}"))
                .spawn(move || {
                    let _guard = guard;
                    worker(shared, usize::from(index != 0), bindings, grants, host);
                });
            if spawned.is_err() {
                self.failed.store(true, Ordering::Release);
            }
        }
        !self.failed.load(Ordering::Acquire)
    }

    /// Hand work that will not run to a worker to destroy. The ordered owner
    /// may be busy with a held request, so the independent lane starts too:
    /// its idle workers destroy it now.
    fn discard(&self, state: &mut State, work: Option<OwnedWork>) {
        if work.is_some() {
            self.start_workers(state, 1);
        }
        discard(&self.shared, state, work);
    }

    /// Where `app:/data`, `app:/cache` and `app:/tmp` are, for a request
    /// whose body is one of the app's files (LLP 1108 D6 R2). Set once,
    /// before the first request; without it such a request is refused.
    pub fn set_app_roots(&self, roots: [std::path::PathBuf; 3]) {
        let _ = self.shared.root_paths.set(roots);
        process::launched(&self.shared);
    }

    #[cfg(test)]
    fn run(&self, r: RequestOut, work: Option<Work>) -> Result<(), &'static str> {
        self.run_owned(r, work.map(OwnedWork::Now))
    }

    /// Admit work, or return a refusal without an overflow queue.
    pub fn run_owned(&self, r: RequestOut, work: Option<OwnedWork>) -> Result<(), &'static str> {
        self.admit(r, work, false, false)
            .map_err(|(reason, _work)| reason)
    }

    /// Take one outcome in lane order (LLP 1162 D3): a pump requests the
    /// next only after fulfilling this one, so a shaping error loses none.
    pub fn drain(&self) -> Vec<(u64, Outcome, Option<CompletionTiming>)> {
        self.drain_with_clock(std::time::Instant::now)
    }

    fn drain_with_clock(
        &self,
        now: impl FnOnce() -> std::time::Instant,
    ) -> Vec<(u64, Outcome, Option<CompletionTiming>)> {
        self.resume_waiting();
        let mut state = self.shared.state.lock().unwrap();
        let ready = |lane: usize, state: &State| {
            if lane == 0 {
                state.ordered.front().and_then(|ticket| {
                    state.completed[0]
                        .iter()
                        .position(|done| done.ticket == *ticket)
                })
            } else {
                (!state.completed[1].is_empty()).then_some(0)
            }
        };
        let first = state.next;
        let (lane, index) = if let Some(index) = ready(first, &state) {
            (first, index)
        } else if let Some(index) = ready(1 - first, &state) {
            (1 - first, index)
        } else {
            return vec![];
        };
        let done = state.completed[lane]
            .remove(index)
            .expect("ready completion");
        let timing = CompletionTiming {
            cached: done.cached,
            elapsed_ms: done.elapsed_ms,
            delivery_wait_ms: now()
                .saturating_duration_since(done.finished_at)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        };
        let release_count = drained(&mut state, done.ticket);
        if lane == 0 {
            state.ordered.pop_front();
        }
        state.next = 1 - lane;
        if !done.message {
            if release_count {
                state.counts[lane] -= 1;
            }
            state.light -= usize::from(done.light);
            state.agains -= usize::from(done.again);
            state.streams -= usize::from(done.stream);
            state.bytes[lane] -= done.bytes;
        }
        // An ordered job may be waiting for these bytes, a re-ask for this
        // room in the window, and a child's output for room in its queue.
        rouse(&self.shared, &state);
        self.shared.processes.drained.notify_all();
        place_waiting(&mut state);
        cache::serve_ready(&mut state);
        if has_ready(&state) {
            wake(&mut state);
        }
        // The independent lane's bytes and perhaps a stream's place, which
        // an upload waiting may take.
        stream::place_freed(&self.shared, state, done.ticket, lane == 1 && !done.message);
        self.resume_waiting();
        vec![(done.ticket, done.outcome, Some(timing))]
    }

    /// Earlier admitted ordered outcomes have all reached the host's pump.
    pub fn ordered_idle(&self) -> bool {
        self.shared.state.lock().unwrap().counts[0] == 0
    }

    /// Settle `ticket`, a re-ask (`Dispatch::Again`), in its place in the
    /// ordered sequence, with no work: complete at once, charged one place
    /// in the markers' own window of 128 and its record's bytes, never the
    /// sixteen or the 128 real tickets (LLP 1041 §8.4, amended 2026-10-09).
    /// With its window full, or a refusal retained, it waits here, one
    /// record per ticket, and is placed oldest first when there is room and
    /// no fence (a drain, a forget, the fence lifting, each of which wakes
    /// the host when something is ready). The answer stays pending: room
    /// never refuses a re-ask. Refused only when the executor is retired or
    /// never started, as all work is.
    pub fn again(&self, ticket: u64) -> Result<(), &'static str> {
        let mut state = self.shared.state.lock().unwrap();
        state.dispatched = true;
        if self.disabled {
            return Err("native executor worker limit reached");
        }
        if state.retired {
            return Err("native executor retired");
        }
        if state.waiting_set.contains(&ticket) || state.ordered.contains(&ticket) {
            return Ok(());
        }
        state.waiting_set.insert(ticket);
        state.waiting.push_back(ticket);
        place_waiting(&mut state);
        if has_ready(&state) {
            wake(&mut state);
        }
        Ok(())
    }

    /// Wake the host: for a refusal stored on a runner ticket.
    pub fn notify(&self) {
        wake(&mut self.shared.state.lock().unwrap());
    }

    /// `notify`, as a handle another thread keeps (a native module's
    /// announcements, LLP 1016.002).
    pub fn waker(&self) -> std::sync::Arc<dyn Fn() + Send + Sync> {
        let shared = self.shared.clone();
        std::sync::Arc::new(move || wake(&mut shared.state.lock().unwrap()))
    }

    /// Acknowledge a coalesced wake, including turns used by refusal
    /// settlement.
    pub fn begin_pump(&self) {
        // A refusal made by waiting admission keeps its wake (LLP 1162 D1).
        self.shared.state.lock().unwrap().notified = false;
        self.resume_waiting();
        let mut state = self.shared.state.lock().unwrap();
        // A refusal may use this turn instead of a completion. Preserve the
        // completion wake even in that case. An extra empty pump is harmless.
        if has_ready(&state) {
            wake(&mut state);
        }
    }

    fn retire(&mut self) {
        {
            let mut state = self.shared.state.lock().unwrap();
            state.retired = true;
            if let Some(cache) = state.cache.as_mut() {
                preload::cancel_all(cache);
            }
            state.cache = None;
            for run in &mut state.running {
                run.cache_exchange = None;
            }
            // Synchronizes with all wake callbacks. None can start after this
            // returns; platform callbacks must only enqueue onto their UI loop.
            state.wake = None;
            // Queued closures are destroyed by their executor owner. Doing
            // it here could run an arbitrary destructor on the UI thread.
            if self.machine.is_none() {
                for queue in &mut state.completed {
                    queue.clear();
                }
            } else if let Some(machine) = &self.machine {
                machine.stop(&mut state);
            }
            state.waiting.clear();
            state.waiting_set.clear();
        }
        #[cfg(test)]
        self.shared
            .worker_notifications
            .fetch_add(1, Ordering::Relaxed);
        self.shared.ready.notify_all();
        self.preloads.lock().unwrap().take();
        if let Some(machine) = &self.machine {
            machine.retire();
            self.idle.lock().unwrap().take();
        }
        let _ = stream::close_sockets(&self.shared, None);
        process::quit(&self.shared);
        self.shared.abort.abort();
        // Never join arbitrary native work on the UI thread. Its WorkerSlot
        // remains charged until return, including across repeated replacement.
    }
}
impl Drop for Core {
    fn drop(&mut self) {
        self.retire();
    }
}
/// A worker to start: its index, its owner's bindings, its reserved slot.
type Starter = (usize, Option<ibex2::host::Bindings>, WorkerSlot);
struct WorkerSlot(Arc<AtomicUsize>);
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// What a core's workers count against: `MAX_WORKERS` live at once,
/// retired ones included until they exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerBudget {
    /// The process's one bound, which every core of every host in the
    /// process shares.
    Process,
    /// A bound of the thread that starts the core, for tests that run side
    /// by side and must not refuse each other's cores.
    Thread,
}

impl WorkerBudget {
    /// The count of live workers this budget bounds.
    fn live(self) -> Arc<AtomicUsize> {
        match self {
            // Cores created by one test share a bound. Slots retain that budget
            // when their workers outlive the test thread or retire elsewhere.
            WorkerBudget::Thread => {
                thread_local! {
                    static BUDGET: Arc<AtomicUsize> = Arc::default();
                }
                BUDGET.with(Arc::clone)
            }
            WorkerBudget::Process => {
                static BUDGET: std::sync::LazyLock<Arc<AtomicUsize>> =
                    std::sync::LazyLock::new(Arc::default);
                Arc::clone(&BUDGET)
            }
        }
    }
}

/// Wake the workers when one of them has something to do: a queued job (it
/// may now fit its bytes) or work to destroy. With neither, nothing a worker
/// waits for has changed, and none is woken: `forget` runs after every commit
/// and woke all seven for nothing (1-2 ms/s each through a list's fling).
/// Retirement wakes them itself.
fn rouse(shared: &Shared, state: &State) {
    if !state.discard.is_empty() || state.jobs.iter().any(|lane| !lane.is_empty()) {
        #[cfg(test)]
        shared.worker_notifications.fetch_add(1, Ordering::Relaxed);
        shared.ready.notify_all();
    }
}

/// Work for a worker to destroy (none, on a core with no workers: it goes
/// with the state).
fn discard(shared: &Shared, state: &mut State, work: Option<OwnedWork>) {
    if let Some(work) = work {
        state.discard.push(work);
        #[cfg(test)]
        shared.worker_notifications.fetch_add(1, Ordering::Relaxed);
        shared.ready.notify_all();
    }
}

fn fenced_bytes(fenced: &VecDeque<Fenced>) -> usize {
    fenced
        .iter()
        .map(|f| match f {
            Fenced::Held(_, _, charge, _) => *charge,
            Fenced::Refused(..) => 0,
        })
        .sum()
}

/// Place waiting re-asks, oldest first, in their window while it has room
/// and no refusal is retained: behind a refusal they wait here, so that they
/// neither settle before it nor take the fence's room for real requests.
fn place_waiting(state: &mut State) {
    while let Some(&ticket) = state.waiting.front() {
        if state.ordered_barrier || !state.fenced.is_empty() || !settle_again(state, ticket) {
            return;
        }
        state.waiting.pop_front();
        state.waiting_set.remove(&ticket);
    }
}

/// A re-ask's marker, complete, at the end of the ordered sequence; false
/// when its window (128 markers, the lane's bytes) has no room.
fn settle_again(state: &mut State, ticket: u64) -> bool {
    if state.agains >= AGAINS || AGAIN_BYTES > BYTES[0].saturating_sub(state.bytes[0]) {
        return false;
    }
    state.ordered.push_back(ticket);
    state.counts[0] += 1;
    state.light += 1;
    state.agains += 1;
    state.bytes[0] += AGAIN_BYTES;
    state.completed[0].push_back(Completed {
        cached: false,
        elapsed_ms: 0,
        finished_at: std::time::Instant::now(),
        ticket,
        outcome: exact_runner::Dispatch::again_outcome(),
        bytes: AGAIN_BYTES,
        message: false,
        stream: false,
        light: true,
        again: true,
        reply: false,
    });
    true
}

/// An ordered request that may wait among the 128: a plain read with no
/// work beside it (RFC 9110 §9.2.1's safe methods; not auth, a surface, or
/// a native call).
fn read(request: &Request, work: bool) -> bool {
    !work
        && safe(request)
        && request.surface.is_none()
        && !request.is_native()
        && !request.is_auth()
}

fn stream_room(state: &State, lane: usize, limit: usize) -> Result<(), &'static str> {
    let streams: usize = state
        .running
        .iter()
        .filter(|run| run.lane == lane && run.stream)
        .map(|run| run.charge)
        .sum();
    if lane == 1 && limit > BYTES[lane].saturating_sub(streams) {
        Err("native response ceiling exceeds the lane budget held by open streams")
    } else {
        Ok(())
    }
}

fn has_ready(state: &State) -> bool {
    !state.completed[1].is_empty()
        || state
            .ordered
            .front()
            .is_some_and(|ticket| state.completed[0].iter().any(|done| done.ticket == *ticket))
}

fn complete(shared: &Shared, ticket: u64, outcome: Outcome) {
    let finished_at = std::time::Instant::now();
    let mut state = shared.state.lock().unwrap();
    if state.retired {
        if state.book.retiring {
            asynch::external_end(shared, &mut state, ticket);
        }
        return;
    }
    let Some(at) = state.running.iter().position(|run| run.ticket == ticket) else {
        return;
    };
    let run = state.running.swap_remove(at);
    let outcome = bounded_outcome(outcome, run.limit);
    cache::invalidate(&mut state, run.invalidates.as_ref(), &outcome);
    let lane = run.lane;
    // A completion frees ordered bytes a waiting job may need.
    rouse(shared, &state);
    if run.forgotten {
        state.counts[lane] -= usize::from(!run.waiting);
        state.streams -= usize::from(run.stream);
        state.bytes[lane] -= run.charge;
        cache::serve_ready(&mut state);
        // The lane going idle can let a retained ordered refusal settle,
        // and the work held behind it go: the host needs a pump for that.
        if lane == 0 && (state.counts[0] == 0 || !state.fenced.is_empty()) {
            wake(&mut state);
        }
        // The room it held, in its lane and among the streams, may start an
        // upload waiting for it; one that held none frees none.
        drop(state);
        if lane == 1 && (run.charge > 0 || run.stream) {
            stream::start_uploads(shared);
        }
        return;
    }
    // An upload that ends before it starts holds a count until drained, as
    // every end does.
    state.counts[lane] += usize::from(run.waiting);
    let bytes = if lane == 0 {
        retained(&outcome) + std::mem::size_of::<Completed>()
    } else {
        run.charge
    };
    state.bytes[lane] = state.bytes[lane] - run.charge + bytes;
    // A finished read holds no effect slot while it waits to be drained.
    state.light += usize::from(run.read);
    state.completed[lane].push_back(Completed {
        cached: run.cached,
        elapsed_ms: finished_at
            .duration_since(run.started)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
        finished_at,
        ticket,
        outcome,
        bytes,
        message: false,
        stream: run.stream,
        light: run.read,
        again: false,
        reply: false,
    });
    cache::serve_ready(&mut state);
    if has_ready(&state) || (run.read && !state.ordered_barrier && !state.fenced.is_empty()) {
        wake(&mut state);
    }
}

/// A continuation a worker hands to the module's owner instead of running,
/// and a long native call it hands to the app's native module.
fn handoff(request: &Request) -> bool {
    (request.continuation.is_some() && request.storage.is_none()) || request.is_native()
}

fn handed_off(job: &Job) -> bool {
    handoff(&job.request) && matches!(job.work, Some(OwnedWork::Later(_)))
}

/// An HTTP read with no effect to lose if it is never sent (RFC 9110 §9.2.1).
fn safe(request: &Request) -> bool {
    request.continuation.is_none()
        && request.storage.is_none()
        && ["GET", "HEAD"]
            .iter()
            .any(|m| request.method.eq_ignore_ascii_case(m))
}

fn wake(state: &mut State) {
    if !state.retired && !state.notified {
        state.notified = true;
        if let Some(wake) = &state.wake {
            wake();
        }
    }
}

/// The lane, the charge at admission and the ceiling a job may retain.
fn reservation(request: &Request) -> Result<(usize, usize, usize), &'static str> {
    if request.stream
        && (request.http == HttpScheduling::Ordered
            || request.storage.is_some()
            || request.continuation.is_some()
            || request.is_native())
    {
        return Err("a stream is independent HTTP (LLP 1016.000)");
    }
    let (lane, body) = match request.http {
        HttpScheduling::Ordered => (0, MAX_BODY),
        HttpScheduling::Independent { max_response_bytes } => {
            if request.storage.is_some() || request.continuation.is_some() {
                return Err("only HTTP may opt into independent transport");
            }
            // 1..=64 MiB, or 1 GiB for a body written to a file.
            if let Some(why) = request.ceiling_refusal() {
                return Err(why);
            }
            (1, max_response_bytes as usize)
        }
    };
    // A body written to a file holds no more than a reply that is not 2xx.
    let body = match request.body_to {
        Some(_) => body.min(exact_runner::BODY_TO_HELD_BYTES as usize),
        None => body,
    };
    let mut bytes = std::mem::size_of::<Job>();
    // Capacity, not length: a caller cannot hide an oversized retained buffer.
    let sizes = [
        request.url.capacity(),
        request.method.capacity(),
        request.body.capacity(),
        request.grants.as_ref().map_or(0, String::capacity),
        request.body_from.as_ref().map_or(0, String::capacity),
        request.body_to.as_ref().map_or(0, String::capacity),
        request.storage.as_ref().map_or(0, Vec::capacity),
        request
            .headers
            .capacity()
            .saturating_mul(std::mem::size_of::<(String, String)>()),
    ];
    for n in sizes.into_iter().chain(
        request
            .headers
            .iter()
            .flat_map(|(k, v)| [k.capacity(), v.capacity()]),
    ) {
        bytes = bytes.checked_add(n).ok_or("request size overflow")?;
    }
    if bytes > MAX_REQUEST {
        return Err("native request exceeds 4 MiB");
    }
    // A plain independent reply is read into one buffer of its ceiling
    // (`body_to::receive`; the asynchronous core grows its buffer only up to
    // the ceiling). Elsewhere Vec growth while collecting can reserve up to
    // twice it. Headers, error text and queue bookkeeping have a separate
    // allowance.
    let body = if reads_within_ceiling(request) {
        body
    } else {
        body.saturating_mul(2)
    };
    let limit = bytes + body.max(32 << 10) + MAX_HEADERS * 2;
    // Admission charges the request buffers; the worker charges the
    // ceiling when it starts the job, waiting for bytes (LLP 1054.000 R3).
    // A ceiling the lane can never hold is refused here, not left waiting.
    if limit > BYTES[lane] {
        return Err("native response ceiling exceeds the lane's byte budget");
    }
    Ok((lane, bytes, limit))
}

/// Whether `request`'s reply is held in a buffer no larger than its ceiling:
/// plain HTTP on the independent lane, not a stream, a process or a native
/// call, whose outcomes are collected elsewhere.
fn reads_within_ceiling(request: &Request) -> bool {
    matches!(request.http, HttpScheduling::Independent { .. })
        && !request.stream
        && !request.is_process()
        && !request.is_native()
}

fn failed(kind: FailureKind, message: impl Into<String>) -> Outcome {
    Outcome::Failed {
        kind,
        message: message.into(),
    }
}
fn worker(
    shared: Arc<Shared>,
    lane: usize,
    bindings: Option<ibex2::host::Bindings>,
    grants: String,
    host: StreamHost,
) {
    let parsed = || ibex2::grant::GrantSet::parse(&exact_runner::io_grants(&grants));
    let bindings = if lane == 1 && bindings.is_none() {
        parsed().ok().map(|g| host().endow(g))
    } else {
        bindings
    };
    // Why a request is refused when this owner holds no bindings: grants
    // that do not parse are named, never reported as absent.
    let unbound = match (&bindings, parsed()) {
        (None, Err(e)) => format!("the app's grants did not parse: {e}"),
        _ => "the app declares no grants".to_string(),
    };
    loop {
        let (job, abort) = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if state.retired {
                    let abandoned = std::mem::take(&mut state.jobs[lane]);
                    let fenced = std::mem::take(&mut state.fenced);
                    let discarded = std::mem::take(&mut state.discard);
                    drop(state);
                    drop((abandoned, fenced, discarded));
                    return;
                }
                if !state.discard.is_empty() {
                    let discarded = std::mem::take(&mut state.discard);
                    drop(state);
                    drop(discarded);
                    state = shared.state.lock().unwrap();
                    continue;
                }
                if let Some(cache) = state.cache.as_mut() {
                    preload::expire(cache);
                }
                if let Some(next) = reads::next_job(&shared, &mut state, lane) {
                    break next;
                }
                let delay = state.cache.as_ref().and_then(|cache| {
                    cache
                        .preload_definitions_due
                        .map(|due| due.saturating_duration_since((cache.clock)()))
                });
                state = match delay {
                    Some(delay) => shared.ready.wait_timeout(state, delay).unwrap().0,
                    None => shared.ready.wait(state).unwrap(),
                };
            }
        };
        let Job {
            ticket,
            mut request,
            forced,
            work,
            queued_deadline,
            overlap,
            class,
            ..
        } = job;
        if let Some(deadline) = queued_deadline {
            if !deadline.remaining(&mut request) {
                complete(
                    &shared,
                    ticket,
                    failed(FailureKind::Timeout, "the request timed out while queued"),
                );
                continue;
            }
        }
        let work = match work {
            Some(OwnedWork::Later(hand)) if handoff(&request) => {
                let owner = shared.clone();
                let reply = Reply::new(move |outcome| complete(&owner, ticket, outcome));
                // The module owner completes asynchronously; the I/O owner stays free.
                // Reply's drop path publishes an aborted outcome if the handoff panics.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hand(reply)));
                let mut state = shared.state.lock().unwrap();
                if let Some(run) = state.running.iter_mut().find(|r| r.ticket == ticket) {
                    run.blocking = false;
                }
                rouse(&shared, &state);
                continue;
            }
            Some(OwnedWork::Now(work)) => Some(work),
            _ => None,
        };
        // Retirement aborts every job; forgetting aborts a safe read.
        let _retiring = {
            let abort = abort.clone();
            shared.abort.signal().register(move || abort.abort())
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let files = Files {
                roots: shared.roots_for(&request),
                grants: &grants,
            };
            // A source's narrower scope runs on this worker's own transport,
            // so it reuses the connections that transport keeps.
            let scoped = match scoped_grants(&grants, request.grants.as_deref()) {
                Some(Err(message)) => return failed(FailureKind::Refused, message),
                Some(Ok(scope)) => Some(bindings.as_ref().map(|b| b.fetch.with_grants(scope))),
                None => None,
            };
            let fetch = match &scoped {
                Some(scoped) => scoped.as_ref(),
                None => bindings.as_ref().map(|b| &b.fetch),
            };
            execute(
                fetch.ok_or(unbound.as_str()),
                request,
                forced,
                work,
                &abort,
                &files,
                overlap && class == RequestClass::Read,
                (&shared, ticket, class),
            )
        }))
        .unwrap_or_else(|_| failed(FailureKind::Aborted, "native work panicked"));
        complete(&shared, ticket, outcome);
    }
}

/// A source's narrower grant scope, checked against the app's; `None` unscoped.
fn scoped_grants(
    grants: &str,
    scope: Option<&str>,
) -> Option<Result<ibex2::grant::GrantSet, String>> {
    scope.map(|scope| {
        exact_data::storage::scope(grants, Some(scope)).and_then(|s| {
            ibex2::grant::GrantSet::parse(&exact_runner::io_grants(s)).map_err(|e| e.to_string())
        })
    })
}

#[cfg(test)]
impl Core {
    /// Streams open on `host`'s transport: a test's scripted one.
    fn streams_on(mut self, host: impl Fn() -> ibex2::host::Host + Send + Sync + 'static) -> Self {
        self.stream_host = Arc::new(host);
        self
    }
}

/// What a body read from an app file needs: where the files are, and the
/// app's grants (the request's own scope narrows them).
struct Files<'a> {
    roots: body::Roots<'a>,
    grants: &'a str,
}

#[allow(clippy::too_many_arguments)]
fn execute(
    fetch: Result<&ibex2::host::Fetch, &str>,
    mut request: Request,
    forced: bool,
    work: Option<Work>,
    abort: &AbortController,
    files: &Files<'_>,
    overlap: bool,
    cache_job: (&Arc<Shared>, u64, RequestClass),
) -> Outcome {
    if abort.signal().aborted() {
        return failed(FailureKind::Aborted, "native request aborted");
    }
    if request.storage.is_some() {
        return failed(
            FailureKind::Unsupported,
            "storage requires an app storage adapter",
        );
    }
    if request.continuation.is_some() {
        return work.map_or_else(
            || {
                failed(
                    FailureKind::Unsupported,
                    "missing or consumed native continuation",
                )
            },
            |work| work(),
        );
    }
    // Work beside a plain request is a driver fault's failure (LLP 1103 D1):
    // it runs instead of the transport, so the request never goes out.
    if let Some(work) = work {
        return work();
    }
    let fetch = match fetch {
        Ok(fetch) => fetch,
        Err(unbound) => return failed(FailureKind::Refused, unbound),
    };
    if let Some(why) = request.timeout_refusal().or(request.long_poll_refusal()) {
        return failed(FailureKind::Refused, why);
    }
    let limit = body_to::limit(&request);
    let within = overlap || reads_within_ceiling(&request);
    let timeout = request.timeout_ms;
    let fetch = reach::gate_fetch(fetch.clone(), request.reach);
    // The whole exchange, headers and body, ends by the deadline: the
    // platform's idle timeout alone would let a server that trickles bytes
    // hold the ordered lane for as long as it likes. It is armed before a
    // body is read from a file, which counts against it too.
    let deadline = match timeout.map(|ms| Deadline::arm(ms, abort)).transpose() {
        Ok(deadline) => deadline,
        // A deadline that cannot be kept is refused, never silently none.
        Err(why) => return failed(FailureKind::Refused, why),
    };
    // The body from an app file, read now, as late as can be, and refused
    // before anything is sent (LLP 1108 D6 R2). The deadline or an abort
    // ends the request while the file is read, and nothing is sent.
    let ended = || {
        if deadline.as_ref().is_some_and(Deadline::passed) {
            Some(failed(
                FailureKind::Timeout,
                format!("the request timed out after {} ms", timeout.unwrap_or(0)),
            ))
        } else if abort.signal().aborted() {
            Some(failed(FailureKind::Aborted, "native request aborted"))
        } else {
            None
        }
    };
    if let Err(outcome) = body::resolve(files.roots, files.grants, &mut request, &ended) {
        return outcome;
    }
    // The file a 2xx body goes to (LLP 1108 D6 R2): staged before sending.
    let staged = match body_to::stage(files.roots, files.grants, &request) {
        Ok(staged) => staged,
        Err(outcome) => return outcome,
    };
    if let Some(outcome) = ended() {
        return outcome;
    }
    if let Some(outcome) =
        cache::prepare(cache_job.0, cache_job.1, &mut request, cache_job.2, forced)
            .or_else(|| cache::missing(&request, forced))
    {
        return outcome;
    }
    let mut req = fetch_request(request, forced);
    cache::observe(cache_job.0, &mut req);
    req.max_body = Some(limit);
    // The platform's idle timeout (URLSession's, 60 s by default) follows
    // the deadline a second later: it never ends a request before the
    // deadline does (so the reply says Timeout, not a network error), and a
    // deadline over 60 s is not cut short by it.
    req.timeout = timeout.map(|ms| std::time::Duration::from_millis(u64::from(ms) + 1_000));
    let retry = cache::retry_request(cache_job.0, cache_job.1, &req);
    let result = fetch.stream(req, &abort.signal()).and_then(check_headers);
    let redirected_conditional = result.as_ref().is_ok_and(|r| {
        cache::retry(
            &mut cache_job.0.state.lock().unwrap(),
            cache_job.1,
            r.status,
            r.redirected,
        )
    });
    let result = if redirected_conditional {
        drop(result);
        fetch
            .stream(retry.expect("a cache conditional"), &abort.signal())
            .and_then(check_headers)
    } else {
        result
    }
    .and_then(|r| body_to::receive(r, staged, limit, within));
    match result {
        Ok(r) => {
            let mut response = Response {
                status: r.status,
                headers: r.headers.entries().to_vec(),
                body: r.body,
            };
            cache::response(
                &mut cache_job.0.state.lock().unwrap(),
                cache_job.1,
                &mut response,
                r.redirected,
            );
            Outcome::Response(response)
        }
        Err(_) if deadline.as_ref().is_some_and(Deadline::passed) => failed(
            FailureKind::Timeout,
            format!("the request timed out after {} ms", timeout.unwrap_or(0)),
        ),
        // @ref LLP 1109 D3 — a response over its size limit is the host
        // refusing it (`refused`), as the event stream, the socket and the
        // web say; ibex2 spells the overflow one way on every transport.
        Err(e) if !abort.signal().aborted() && e == ibex2::stdlib::fetch::over_limit(limit) => {
            failed(FailureKind::Refused, "HTTP response exceeds limit")
        }
        Err(e) => fetch_failure(e, abort),
    }
}

// The process transport exists where a host starts programs (LLP 1133.007
// §8); elsewhere every request answers `Unsupported` and none of it links.
#[cfg_attr(any(target_os = "macos", target_os = "linux"), path = "process.rs")]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux")),
    path = "process_off.rs"
)]
mod process;
pub(crate) mod stream;
pub use stream::listen;

#[cfg(test)]
mod async_tests;
#[path = "async.rs"]
mod asynch;
mod outcome;
use asynch::{asynch_lanes, asynchronous, drained};
use outcome::{
    bounded_outcome, build_number, check_headers, fetch_failure, fetch_request, retained, Deadline,
    Quiet,
};

mod forget;
mod reach;

mod body;
mod body_to;

mod admission;
mod cache;
pub mod preload;
pub mod pump;
mod reads;

#[cfg(test)]
mod read_tests;

#[cfg(test)]
mod idle_tests;
#[cfg(test)]
mod scope_tests;
#[cfg(test)]
mod tests;
