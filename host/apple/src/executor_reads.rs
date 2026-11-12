//! Worker selection for reads that overlap in the ordered sequence (LLP 1162 D1).
use super::*;

pub(super) fn overlaps(r: &RequestOut, work: bool) -> bool {
    r.class == exact_runner::RequestClass::Read
        && r.request.is_ordered()
        && r.request.is_plain_http()
        && read(&r.request, work)
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

pub(super) fn next_job(state: &mut State, worker: usize) -> Option<(Job, AbortController)> {
    if worker == 1 {
        if let Some(next) = take_job(state, 1) {
            return Some(next);
        }
    }
    let job = state.jobs[0].front()?;
    if job.overlap {
        let mut reads = 0;
        for run in state.running.iter().filter(|r| r.lane == 0) {
            if !run.overlap {
                return None;
            }
            reads += 1;
        }
        // The ordered worker is an escape from held independent requests,
        // not a seventh overlapping read (LLP 1162 D1).
        if reads >= INDEPENDENT {
            return None;
        }
    } else if worker != 0 || state.running.iter().any(|r| r.overlap) {
        return None;
    }
    take_job(state, 0)
}

fn take_job(state: &mut State, lane: usize) -> Option<(Job, AbortController)> {
    let job = state.jobs[lane].front()?;
    // A module handoff's outcome is the owner's to make; it is charged
    // what it retains when it completes.
    let more = if handed_off(job) {
        0
    } else {
        job.limit - job.charge
    };
    if more > BYTES[lane].saturating_sub(state.bytes[lane]) {
        return None;
    }
    let mut job = state.jobs[lane].pop_front()?;
    job.charge += more;
    state.bytes[lane] += more;
    let abort = AbortController::new();
    state.running.push(Running {
        started: std::time::Instant::now(),
        ticket: job.ticket,
        lane,
        charge: job.charge,
        limit: job.limit,
        abort: safe(&job.request).then(|| abort.clone()),
        exchange: (job.work.is_none() && !job.request.stream).then(|| abort.clone()),
        forgotten: job.forgotten,
        stream: job.request.stream,
        waiting: false,
        read: lane == 0 && read(&job.request, job.work.is_some()),
        overlap: job.overlap,
        terminal: false,
        live: 0,
        drained: false,
        blocking: false,
        generation: 0,
        claim: asynch::Claim::None,
    });
    Some((job, abort))
}
