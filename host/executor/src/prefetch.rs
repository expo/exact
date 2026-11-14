//! A prefetch starts the executor's own preload; its caller stays a read (LLP 1162.000 D5).
use super::*;

impl Core {
    pub(in super::super) fn prefetch(
        &self,
        state: &mut State,
        owned: &mut Option<Preloads>,
        r: &RequestOut,
        work: bool,
    ) {
        if !r.request.prefetch || !eligible(&r.request, r.class, r.forced, work) {
            return;
        }
        // A preload must not outrun its read's ordering dependencies (LLP 1162.000 D5).
        if state.ordered_barrier
            || !state.fenced.is_empty()
            || state.running.iter().any(|before| {
                before.lane == 0
                    && before.blocking
                    && reads::waits_for(r.class, before.class, before.overlap)
            })
            || state.jobs[0]
                .iter()
                .any(|before| reads::waits_for(r.class, before.class, before.overlap))
        {
            return;
        }
        let Some(cache) = state.cache.as_mut() else {
            return;
        };
        expire(cache);
        if !valid_request(&r.request)
            || cache::granted(&self.grants, &r.request).is_err()
            || matching(cache, &r.request, true).is_some()
        {
            return;
        }
        let slot = cache
            .preloads
            .iter()
            .position(|p| p.prefetch && matches!(p.reply, ReplyState::Gone));
        let count = cache.preloads.iter().filter(|p| p.prefetch).count();
        let slot = slot.or_else(|| {
            if count < MAX_PRELOADS {
                return None;
            }
            cache
                .preloads
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    if let ReplyState::Ready(ready) = &p.reply {
                        p.prefetch.then_some((i, ready.completed, p.generation))
                    } else {
                        None
                    }
                })
                .min_by_key(|(_, completed, generation)| (*completed, *generation))
                .map(|(i, _, _)| i)
        });
        if count >= MAX_PRELOADS && slot.is_none() {
            return;
        }
        if owned.is_none() {
            let set = Preloads::start_clock(
                Vec::new(),
                &self.grants,
                self.stream_host.clone(),
                cache.clock.clone(),
            );
            *set.destination.state.lock().unwrap() = Destination::Adopted(
                Arc::downgrade(&self.shared),
                self.machine.as_ref().map(|m| Arc::downgrade(&m.timer)),
            );
            *owned = Some(set);
        }
        let generation = cache.touch();
        let mut held = Held::new(
            Definition {
                name: String::new(),
                request: r.request.clone(),
                deadline_ms: None,
                secrets: Vec::new(),
            },
            (cache.clock)(),
            None,
            generation,
        );
        held.prefetch = true;
        let index = if let Some(i) = slot {
            drop_held(cache, i, Event::NeverTaken);
            cache.preloads[i] = held;
            i
        } else {
            cache.preloads.push(held);
            cache.preloads.len() - 1
        };
        let set = owned.as_ref().expect("preload owner");
        if !set.control().start_worker()
            || set
                .send
                .send(Message::Start(
                    vec![(index, generation)],
                    self.grants.clone(),
                ))
                .is_err()
        {
            drop_held(cache, index, Event::Cancelled);
        }
    }
}
