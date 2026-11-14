//! Selection and dependencies in the ordered sequence (LLP 1162 D1).
use super::*;

pub(super) fn overlaps(r: &RequestOut, work: bool) -> bool {
    r.class != RequestClass::Ordered && r.request.is_plain_http() && !work
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
// class waits for all earlier classes except its peers (LLP 1162 D1).
fn waits_for(class: RequestClass, before: RequestClass) -> bool {
    match class {
        RequestClass::Read => before != RequestClass::Read,
        RequestClass::Parallel(m) => before != RequestClass::Parallel(m),
        RequestClass::Ordered => true,
    }
}

pub(super) fn reservation(
    request: &Request,
    overlap: bool,
) -> Result<(usize, usize, usize), &'static str> {
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
    let index = state.jobs[0]
        .iter()
        .position(|job| !preload::waiting(state, job))?;
    let job = &state.jobs[0][index];
    if index > 0 && job.class != RequestClass::Read {
        return None;
    }
    if !job.overlap && worker != 0 {
        return None;
    }
    if !can_start(state, job) {
        return None;
    }
    take_job(state, 0, index)
}

pub(super) fn can_start(state: &State, job: &Job) -> bool {
    let class = if job.overlap {
        job.class
    } else {
        RequestClass::Ordered
    };
    let mut overlapping = 0;
    let mut parallel = 0;
    for run in state.running.iter().filter(|r| r.lane == 0 && r.blocking) {
        let before = if run.overlap {
            run.class
        } else {
            RequestClass::Ordered
        };
        if waits_for(class, before) {
            return false;
        }
        overlapping += usize::from(run.overlap);
        parallel += usize::from(matches!(before, RequestClass::Parallel(_)));
    }
    // The ordered worker provides progress under held independent HTTP;
    // it does not enlarge the overlap limits (LLP 1162 D1).
    if job.overlap
        && (overlapping >= INDEPENDENT
            || (matches!(class, RequestClass::Parallel(_)) && parallel >= 3))
    {
        return false;
    }
    true
}

fn take_job(state: &mut State, lane: usize, index: usize) -> Option<(Job, AbortController)> {
    let job = state.jobs[lane].get(index)?;
    let more = job.limit - job.charge;
    if more > BYTES[lane].saturating_sub(state.bytes[lane]) {
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
        exchange: (job.work.is_none() && !job.request.stream).then(|| abort.clone()),
        forgotten: job.forgotten,
        stream: job.request.stream,
        waiting: false,
        read: lane == 0 && safe_read(&job.request, job.work.is_some(), job.class),
        overlap: job.overlap,
        class: job.class,
        terminal: false,
        live: 0,
        drained: false,
        blocking: lane == 0,
        generation: 0,
        claim: asynch::Claim::None,
        cached: false,
        cache_exchange: None,
        cache_replacing: cache::replacing(&job.request, job.class, job.forced, job.work.is_some()),
        cache_barrier: cache::barrier(&job.request, job.class, job.work.is_some()),
        invalidates: cache::invalidates(&job.request, job.work.is_some()),
    });
    Some((job, abort))
}
