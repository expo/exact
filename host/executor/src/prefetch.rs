//! An admitted read may keep its exchange after its caller lets go (LLP 1162.000 D5).
use super::*;

pub(in super::super) const MAX_PREFETCHES: usize = 8;

pub(in super::super) fn start(state: &mut State, job: &mut Job, abort: &AbortController) {
    if !job.hold_prefetch
        || !job.request.prefetch
        || job.work.is_some()
        || job.class != RequestClass::Read
        || job.request.reach.origin != exact_runner::CallOrigin::App
    {
        return;
    }
    let Some(cache) = state.cache.as_mut() else {
        return;
    };
    expire(cache);
    cache
        .prefetches
        .retain(|p| !matches!(p.reply, ReplyState::Gone));
    job.request
        .timeout_ms
        .get_or_insert(DEFAULT_DEADLINE.as_millis() as u32);
    let now = (cache.clock)();
    let mut held = Held::new(
        Definition {
            name: String::new(),
            request: job.request.clone(),
            deadline_ms: None,
            secrets: Vec::new(),
        },
        now,
        None,
    );
    held.prefetch = Some(job.ticket);
    held.key = Equal::of(&job.request);
    if let ReplyState::Pending { abort: own, .. } = &mut held.reply {
        *own = abort.clone();
    }
    let visited = vec![job.request.url.clone()];
    let bytes = held_bytes(&held)
        + visited.capacity() * std::mem::size_of::<String>()
        + visited.iter().map(String::capacity).sum::<usize>();
    *held.answered_at.as_ref().unwrap().lock().unwrap() = Some(Answered { at: now, visited });
    if bytes > cache::MAX_ENTRY || !cache.room(bytes) {
        return;
    }
    held.pending_bytes = bytes;
    cache.charge(bytes);
    cache.prefetches.push(held);
}

pub(in super::super) fn observe(
    shared: &Arc<Shared>,
    ticket: u64,
    request: &mut ibex2::stdlib::fetch::Request,
) {
    let shared = Arc::downgrade(shared);
    request.received = Some(ibex2::stdlib::fetch::Received(Arc::new(
        move |_, url, _, _| {
            let Some(shared) = shared.upgrade() else {
                return;
            };
            let mut state = shared.state.lock().unwrap();
            let Some(cache) = state.cache.as_mut() else {
                return;
            };
            let Some(i) = cache
                .prefetches
                .iter()
                .position(|p| p.prefetch == Some(ticket))
            else {
                return;
            };
            let held = &cache.prefetches[i];
            let Some(answer) = &held.answered_at else {
                return;
            };
            let delta = {
                let mut answer = answer.lock().unwrap();
                let Some(answer) = answer.as_mut() else {
                    return;
                };
                if answer.visited.iter().any(|u| u == url) {
                    return;
                }
                let before = answer.visited.capacity();
                answer.visited.push(url.into());
                (answer.visited.capacity() - before) * std::mem::size_of::<String>()
                    + answer.visited.last().unwrap().capacity()
            };
            if cache.prefetches[i].pending_bytes + delta > cache::MAX_ENTRY || !cache.room(delta) {
                drop_held(cache, cache.preloads.len() + i, Event::Oversized);
            } else {
                cache.prefetches[i].pending_bytes += delta;
                cache.charge(delta);
            }
        },
    )));
}

pub(in super::super) fn completed(state: &mut State, ticket: u64) {
    if let Some(cache) = &mut state.cache {
        if let Some(held) = cache
            .prefetches
            .iter_mut()
            .find(|p| p.prefetch == Some(ticket))
        {
            if let Some(answer) = &held.answered_at {
                if let Some(answer) = answer.lock().unwrap().as_mut() {
                    answer.at = (cache.clock)();
                }
            }
        }
    }
}

pub(in super::super) fn finish(state: &mut State, ticket: u64, outcome: &Outcome) {
    let Some(cache) = state.cache.as_mut() else {
        return;
    };
    let Some(i) = cache
        .prefetches
        .iter()
        .position(|p| p.prefetch == Some(ticket) && matches!(p.reply, ReplyState::Pending { .. }))
    else {
        return;
    };
    let index = cache.preloads.len() + i;
    let Outcome::Response(response) = outcome else {
        drop_held(cache, index, Event::Failed);
        return;
    };
    let held = &cache.prefetches[i];
    let Answered {
        at: completed,
        visited,
    } = held
        .answered_at
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .take()
        .unwrap();
    let bytes = held_bytes(held)
        + response_bytes(response)
        + visited.capacity() * std::mem::size_of::<String>()
        + visited.iter().map(String::capacity).sum::<usize>();
    let pending = std::mem::take(&mut cache.prefetches[i].pending_bytes);
    cache.release(pending);
    if bytes > cache::MAX_ENTRY {
        drop_held(cache, index, Event::Oversized);
        return;
    }
    if cache
        .prefetches
        .iter()
        .filter(|p| p.used().is_some())
        .count()
        >= MAX_PREFETCHES
    {
        let oldest = cache
            .prefetches
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.reply {
                ReplyState::Ready(r) => Some((i, r.completed_order)),
                _ => None,
            })
            .min_by_key(|(_, at)| *at)
            .unwrap()
            .0;
        drop_held(cache, cache.preloads.len() + oldest, Event::NeverTaken);
    }
    if !cache.room(bytes) {
        drop_held(cache, index, Event::Oversized);
        return;
    }
    let used = cache.touch();
    cache.prefetches[i].reply = ReplyState::Ready(Ready {
        response: response.clone(),
        visited,
        completed,
        completed_order: used,
        bytes,
        used,
    });
    cache.charge(bytes);
    timer::arm(cache);
}

pub(in super::super) fn cancel(state: &mut State, tickets: &[u64]) {
    let Some(cache) = state.cache.as_mut() else {
        return;
    };
    for i in 0..cache.prefetches.len() {
        if cache.prefetches[i]
            .prefetch
            .is_some_and(|t| tickets.contains(&t))
        {
            drop_held(cache, cache.preloads.len() + i, Event::Cancelled);
        }
    }
}

// Abort callbacks can reenter the core: its existing disposer runs them off the lock (LLP 1162.000 D5).
struct Cancel(AbortController);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub(in super::super) fn dispose(state: &mut State) {
    let Some(cache) = state.cache.as_mut() else {
        return;
    };
    for (ticket, abort) in cache.prefetch_drops.drain(..) {
        let Some(run) = state
            .running
            .iter_mut()
            .find(|r| r.ticket == ticket && !r.terminal)
        else {
            continue;
        };
        // Losing the hold leaves the caller's exchange; a later forget can cancel it (LLP 1162.000 D5).
        run.abort = Some(abort.clone());
        if !run.forgotten {
            continue;
        }
        let cancel = Cancel(abort);
        state.discard.push(OwnedWork::Now(Box::new(move || {
            drop(cancel);
            failed(FailureKind::Aborted, "prefetch cancelled")
        })));
    }
}

#[cfg(test)]
#[path = "prefetch_tests.rs"]
mod tests;
