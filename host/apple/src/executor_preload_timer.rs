//! Held replies expire even while the executor is idle (LLP 1162.000 D1, D5).
use super::*;

pub(in super::super) struct Expiry {
    shared: Weak<Shared>,
    timer: Arc<asynch::timer::Timer>,
    armed: Option<Instant>,
}

pub(in super::super) fn install(
    cache: &mut cache::Cache,
    shared: &Arc<Shared>,
    machine: &Arc<asynch::Machine>,
) {
    cache.expiry = Some(Expiry {
        shared: Arc::downgrade(shared),
        timer: machine.timer.clone(),
        armed: None,
    });
    arm(cache);
}

pub(in super::super) fn wake_due(cache: &cache::Cache) -> Option<Instant> {
    cache
        .holds()
        .filter_map(|held| match &held.reply {
            ReplyState::Pending { due, .. } if held.prefetch.is_none() => Some(*due),
            ReplyState::Ready(ready) => Some(ready.completed + HOLD),
            _ => None,
        })
        .chain(pending_due(cache))
        .min()
}

pub(super) fn arm(cache: &mut cache::Cache) {
    let due = wake_due(cache);
    let now = (cache.clock)();
    let Some(expiry) = cache.expiry.as_mut() else {
        return;
    };
    if expiry.armed == due {
        return;
    }
    expiry.timer.remove((0, 0));
    expiry.armed = due;
    let Some(due) = due else { return };
    let shared = expiry.shared.clone();
    expiry.timer.arm_callback(
        due.saturating_duration_since(now),
        Box::new(move || {
            let Some(shared) = shared.upgrade() else {
                return;
            };
            let mut state = shared.state.lock().unwrap();
            if state.retired {
                return;
            }
            let Some(cache) = state.cache.as_mut() else {
                return;
            };
            let Some(expiry) = cache.expiry.as_mut() else {
                return;
            };
            if expiry.armed != Some(due) {
                return;
            }
            expiry.armed = None;
            expire(cache);
            arm(cache);
            cache::serve_ready(&mut state);
            rouse(&shared, &state);
            shared.ready.notify_all();
        }),
    );
}
