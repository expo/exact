//! Selection and dependencies in the ordered sequence (LLP 1162 D1).
use super::*;

/// A response substitute retains HTTP scheduling and cancellation.
pub(super) fn opaque(work: &Option<OwnedWork>) -> bool {
    matches!(work, Some(OwnedWork::Now(_) | OwnedWork::Later(_)))
}

pub(super) enum Pending<'a> {
    Running(&'a Running),
    Queued(&'a Job),
}
impl Pending<'_> {
    pub(super) fn ticket(&self) -> u64 {
        match self {
            Self::Running(r) => r.ticket,
            Self::Queued(j) => j.ticket,
        }
    }
}
pub(super) fn pending(state: &State) -> impl Iterator<Item = Pending<'_>> {
    state
        .running
        .iter()
        .filter(|r| r.lane == 0 && !r.terminal)
        .map(Pending::Running)
        .chain(state.jobs[0].iter().map(Pending::Queued))
}

/// Delivery class of an entry in `State::ordered` (LLP 1162 §8 Q2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OrderedKind {
    /// Plain HTTP `Read`: settles on arrival; passes and is passed by reads, locals, and re-asks.
    Read,
    /// Local storage or JS module-turn continuation: passes earlier reads while staying FIFO with locals and re-asks.
    Local,
    /// `Dispatch::Again` marker: passes earlier reads; stays behind barriers and earlier locals.
    Again,
    /// Ordered/parallel sends, file-body HTTP, native/auth/surface, opaque work: two-sided barrier.
    Barrier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OrderedEntry {
    pub(super) ticket: u64,
    pub(super) kind: OrderedKind,
    /// A forgotten non-cancellable barrier or local job kept until it ends so
    /// later reads, locals, and re-asks cannot overtake it; never delivered.
    pub(super) forgotten: bool,
}

impl OrderedEntry {
    pub(super) fn new(ticket: u64, kind: OrderedKind) -> Self {
        Self {
            ticket,
            kind,
            forgotten: false,
        }
    }
}

pub(super) fn again_kind(class: RequestClass) -> OrderedKind {
    if class == RequestClass::Local {
        OrderedKind::Again
    } else {
        OrderedKind::Barrier
    }
}

pub(super) struct SharedBusy<'a>(pub(super) &'a Shared, pub(super) bool);
impl SharedBusy<'_> {
    pub(super) fn release_locked(&mut self, state: &mut State) {
        if self.1 {
            state.shared_workers_busy = state.shared_workers_busy.saturating_sub(1);
            self.1 = false;
        }
    }
}
impl Drop for SharedBusy<'_> {
    fn drop(&mut self) {
        if self.1 {
            let mut state = self.0.state.lock().unwrap();
            self.release_locked(&mut state);
        }
    }
}

pub(super) fn ordered_kind(r: &RequestOut, work: bool) -> OrderedKind {
    if overlaps(r, work) && r.class == RequestClass::Read {
        OrderedKind::Read
    } else if r.class == RequestClass::Local && r.request.is_local_work() {
        OrderedKind::Local
    } else {
        OrderedKind::Barrier
    }
}

/// Earliest deliverable lane-0 entry as `(ordered_index, completed_index)` (LLP 1162 §8 Q2).
/// Shared by `drain_with_clock` and `has_ready` so readiness and delivery never diverge.
/// Among eligible entries before any barrier, chooses the earliest completion in `completed[0]`
/// so batched read completions drain in completion order while locals and re-asks stay FIFO.
pub(super) fn deliverable(state: &State) -> Option<(usize, usize)> {
    if state.completed[0].is_empty() || state.ordered.is_empty() {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    let mut saw_local = false;
    for (ordered_idx, entry) in state.ordered.iter().enumerate() {
        let consider = match entry.kind {
            OrderedKind::Barrier => {
                if !entry.forgotten && ordered_idx == 0 {
                    if let Some(completed_idx) = state.completed[0]
                        .iter()
                        .position(|done| done.ticket == entry.ticket)
                    {
                        return Some((0, completed_idx));
                    }
                }
                break;
            }
            OrderedKind::Local | OrderedKind::Again => {
                let first = !saw_local;
                saw_local = true;
                first && !entry.forgotten
            }
            OrderedKind::Read => !entry.forgotten,
        };
        if consider {
            if let Some(completed_idx) = state.completed[0]
                .iter()
                .position(|done| done.ticket == entry.ticket)
            {
                if best.is_none_or(|(_, best_completed)| completed_idx < best_completed) {
                    best = Some((ordered_idx, completed_idx));
                    if completed_idx == 0 {
                        return best;
                    }
                }
            }
        }
    }
    best
}

/// Update `state.ordered` when tickets are forgotten: drop cancelled reads and
/// re-asks, but retain still-running or queued non-cancellable barriers/locals
/// as tombstones until they finish so later work cannot overtake them.
pub(super) fn retain_on_forget(state: &mut State, held: impl Fn(u64) -> bool) {
    let surviving: std::collections::HashSet<u64> = state
        .running
        .iter()
        .filter(|r| r.lane == 0 && r.blocking)
        .map(|r| r.ticket)
        .chain(state.jobs[0].iter().map(|j| j.ticket))
        .collect();
    state.ordered.retain_mut(|entry| {
        if held(entry.ticket) {
            true
        } else if matches!(entry.kind, OrderedKind::Barrier | OrderedKind::Local)
            && surviving.contains(&entry.ticket)
        {
            entry.forgotten = true;
            true
        } else {
            false
        }
    });
}

pub(super) fn overlaps(r: &RequestOut, work: bool) -> bool {
    matches!(r.class, RequestClass::Read | RequestClass::Parallel(_))
        && r.request.is_plain_http()
        && !work
}

pub(super) fn effective_class(class: RequestClass, overlap: bool) -> RequestClass {
    match class {
        RequestClass::Local => RequestClass::Local,
        _ if overlap => class,
        _ => RequestClass::Ordered,
    }
}

pub(super) fn effect_read(r: &RequestOut, work: bool) -> bool {
    r.class == RequestClass::Read && overlaps(r, work) && !safe(&r.request)
}

pub(super) fn safe_to_forget(request: &Request, class: RequestClass) -> bool {
    safe(request) && !matches!(class, RequestClass::Parallel(_))
}

pub(super) fn safe_read(request: &Request, work: bool, class: RequestClass) -> bool {
    read(request, work) && safe_to_forget(request, class)
}

// A made handoff has ended for scheduling (LLP 1120 D3). Otherwise each
// class waits for all earlier classes except its peers (LLP 1162 D1, §8 Q2).
pub(super) fn waits_for(class: RequestClass, before: RequestClass, overlap: bool) -> bool {
    let before = effective_class(before, overlap);
    match class {
        RequestClass::Read => !matches!(before, RequestClass::Read | RequestClass::Local),
        RequestClass::Local => before != RequestClass::Read,
        RequestClass::Parallel(m) => before != RequestClass::Parallel(m),
        RequestClass::Ordered => true,
    }
}

pub(super) fn reservation(
    request: &Request,
    overlap: bool,
) -> Result<(usize, usize, usize), String> {
    super::reservation(request).map(|(lane, charge, limit)| {
        // LLP 1162 D1: one buffer reserved before reading, on the ordered lane.
        (lane, charge, limit - if overlap { MAX_BODY } else { 0 })
    })
}

pub(super) fn next_job(
    shared: &Shared,
    state: &mut State,
    worker: usize,
) -> Option<(Job, AbortController)> {
    if worker == 1 {
        while forget::refuse_crowded(shared, state, 1, 0) {}
        if let Some(next) = take_job(state, 1, 0) {
            return Some(next);
        }
    }
    while forget::refuse_crowded(shared, state, 0, 0) {}
    if !preload::reserve_front(state) {
        return None;
    }
    let mut saw_preload_wait = false;
    let mut index = 0;
    while index < state.jobs[0].len() {
        let job = &state.jobs[0][index];
        if preload::waiting(state, job) {
            saw_preload_wait = true;
            index += 1;
            continue;
        }
        // Only a plain HTTP read passes a job waiting on a launch preload (LLP 1162.000 D1).
        if saw_preload_wait && job.class != RequestClass::Read {
            return None;
        }
        // Keep worker 0 free for `Local` and `Ordered` work whenever a shared
        // worker is idle to take an overlapping HTTP request.
        let yield_to_shared =
            worker == 0 && job.overlap && state.shared_workers_started > state.shared_workers_busy;
        let eligible = (job.overlap || worker == 0) && !yield_to_shared;
        let passable = job.overlap && job.class == RequestClass::Read;
        if eligible && can_start(state, job) {
            if let Some(next) = take_job(state, 0, index) {
                return Some(next);
            }
        }
        // A queued `Local` or later read may pass a queued overlapping `Read`
        // that cannot start yet (e.g. shared cap or byte room); nothing passes
        // a queued non-`Read` barrier.
        if passable {
            index += 1;
            continue;
        }
        return None;
    }
    None
}

pub(super) fn unblocked(state: &State, job: &Job) -> bool {
    let class = effective_class(job.class, job.overlap);
    !state
        .running
        .iter()
        .any(|run| run.lane == 0 && run.blocking && waits_for(class, run.class, run.overlap))
        && !state.jobs[0]
            .iter()
            .take_while(|before| before.ticket != job.ticket)
            .any(|before| waits_for(class, before.class, before.overlap))
}

pub(super) fn can_start(state: &State, job: &Job) -> bool {
    if !unblocked(state, job) {
        return false;
    }
    let mut overlapping = 0;
    let mut parallel = 0;
    for run in state.running.iter().filter(|r| r.lane == 0 && r.blocking) {
        let before = effective_class(run.class, run.overlap);
        overlapping += usize::from(run.overlap);
        parallel += usize::from(matches!(before, RequestClass::Parallel(_)));
    }
    // The ordered worker provides progress under held independent HTTP;
    // it does not enlarge the overlap limits (LLP 1162 D1).
    if job.overlap
        && (overlapping >= INDEPENDENT
            || (matches!(job.class, RequestClass::Parallel(_)) && parallel >= 3))
    {
        return false;
    }
    !job.overlap || job.class != RequestClass::Read || room_to_start(state, job)
}

pub(super) fn has_room(state: &State, lane: usize, more: usize) -> bool {
    more <= BYTES[lane].saturating_sub(state.bytes[lane])
}

pub(super) fn room_to_start(state: &State, job: &Job) -> bool {
    // The front job reserves even while waiting on a hold. Other reads
    // ahead take their own reservations before this one (LLP 1162.000 D5).
    let more = state.jobs[0]
        .iter()
        .take_while(|before| before.ticket != job.ticket)
        .chain(std::iter::once(job))
        .enumerate()
        .filter(|(index, before)| {
            *index == 0 || before.ticket == job.ticket || !preload::waiting(state, before)
        })
        .fold(0_usize, |more, (_, before)| {
            more.saturating_add(before.limit - before.charge)
        });
    has_room(state, 0, more)
}

fn take_job(state: &mut State, lane: usize, index: usize) -> Option<(Job, AbortController)> {
    let job = state.jobs[lane].get(index)?;
    let more = job.limit - job.charge;
    if !has_room(state, lane, more) {
        return None;
    }
    let mut job = state.jobs[lane].remove(index)?;
    job.charge += more;
    state.bytes[lane] += more;
    let abort = AbortController::new();
    state.running.push(Running {
        started: std::time::Instant::now(),
        ticket: job.ticket,
        lane,
        transport_lane: lane,
        charge: job.charge,
        limit: job.limit,
        abort: safe_to_forget(&job.request, job.class).then(|| abort.clone()),
        exchange: (!opaque(&job.work) && !job.request.stream).then(|| abort.clone()),
        forgotten: job.forgotten,
        fixture_stream: false,
        // Queued jobs (including buffered `answer stream` replies) never reserve a StreamKind slot.
        stream: None,
        waiting: false,
        read: lane == 0 && safe_read(&job.request, opaque(&job.work), job.class),
        overlap: job.overlap,
        class: job.class,
        terminal: false,
        live: 0,
        drained: false,
        blocking: lane == 0,
        generation: 0,
        claim: asynch::Claim::None,
        cached: false,
        cache_exchange: cache::exchange(state, &job),
        cache_replacing: cache::replacing(
            &job.request,
            job.class,
            job.forced,
            super::reads::opaque(&job.work),
        ),
        refresh: preload::Refresh::of(&job.request, job.class, job.forced),
        cache_barrier: cache::barrier(&job.request, job.class, super::reads::opaque(&job.work)),
        invalidates: cache::invalidates(&job.request, super::reads::opaque(&job.work)),
    });
    Some((job, abort))
}
