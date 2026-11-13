//! Admission and the bounded ordered fence (LLP 1041 §8.4, LLP 1162 D1).
use super::*;

impl Core {
    /// Admit `r`, hold it behind the ordered barrier, or refuse it, handing
    /// back its work for the caller to destroy.
    pub(super) fn admit(
        &self,
        r: RequestOut,
        work: Option<OwnedWork>,
        forgotten: bool,
        waiting: bool,
    ) -> Result<(), (&'static str, Option<OwnedWork>)> {
        let ordered = r.request.is_ordered();
        let overlap = reads::overlaps(&r, work.is_some());
        let mut state = self.shared.state.lock().unwrap();
        state.dispatched = true;
        // A deadline on work that cannot take one (a stream but an upload, a continuation, native work), or a
        // long poll that is no stream's HTTP GET (LLP 1016.000 D8), is refused here, before any path.
        let checked = r
            .request
            .long_poll_refusal()
            .or(r.request.timeout_refusal())
            .or(r.request.protocols_refusal())
            .or(r.request.body_from_refusal())
            .or(r.request.body_to_refusal())
            .or(r.request.upload_progress_refusal())
            .map_or_else(
                || reads::reservation(&r.request, overlap && r.class == RequestClass::Read),
                Err,
            )
            .and_then(|charge| state.book.reserve_inline_copy(&r.request, charge));
        let lane = checked.as_ref().map_or(0, |(lane, _, _)| *lane);
        if self.disabled
            || !self.start_workers(&mut state, lane)
            || (overlap && !self.start_workers(&mut state, 1))
        {
            return Err(("native executor worker limit reached", work));
        }
        if state.retired {
            return Err(("native executor retired", work));
        }
        let effect_read = reads::effect_read(&r, work.is_some());
        let waits_for_effect =
            (effect_read || waiting) && preload::effect_count(&state) >= COUNTS[0];
        if ordered && (state.ordered_barrier || !state.fenced.is_empty() || waits_for_effect) {
            // Later requests, GETs included, wait behind a held read so
            // admission preserves reply order (LLP 1162 D1).
            let held = state
                .fenced
                .iter()
                .filter(|f| matches!(f, Fenced::Held(..)))
                .count();
            let place = match checked {
                Err(why) => Err(why),
                Ok((_, charge, _))
                    if held >= ORDERED_READS
                        || charge > ORDERED_WAITING_BYTES.saturating_sub(state.fenced_bytes) =>
                {
                    Err("earlier ordered admission refusal must settle first")
                }
                Ok((_, charge, _)) => Ok(charge),
            };
            match place {
                Ok(charge) => {
                    state.fenced_bytes += charge;
                    state
                        .fenced
                        .push_back(Fenced::Held(Box::new(r), work, charge, forgotten));
                }
                Err(why) => {
                    state.fenced.push_back(Fenced::Refused(r.ticket, why));
                    self.discard(&mut state, work);
                }
            }
            return Ok(());
        }
        // An upload waits for its start holding no count, bytes or place of
        // its lane, so a queue of them refuses no other request; it takes
        // them as it starts (`stream::start`).
        let waits = r.request.stream && r.request.upload_progress;
        // A process one-shot runs on a thread of its own with room of its
        // own, so a slow one leaves the lane's requests theirs (LLP 1133.007 D3).
        let one_shot = r.request.is_process() && !r.request.stream;
        let admitted = (|| {
            let (lane, charge, limit) = checked?;
            if !r.request.stream {
                stream_room(&state, lane, limit)?;
            }
            // A stream starts at once, so it is charged its ceiling now.
            let (full, charge) = if r.request.stream {
                (state.streams >= STREAMS && !waits, limit)
            } else {
                let streams = if lane == 1 { state.streams } else { 0 };
                // Plain reads may wait in a larger, still ordered backlog:
                // every ordered ticket counts against its 128. Writes and
                // opaque work keep the sixteen, counted over everything
                // admitted but re-ask markers and finished reads, which
                // hold no effect (LLP 1041 §8.4, amended 2026-10-09).
                let full = if lane == 1 {
                    state.counts[1] - streams >= COUNTS[1]
                } else if reads::safe_read(&r.request, work.is_some(), r.class) {
                    state.counts[0] - state.agains >= ORDERED_READS
                } else {
                    preload::effect_count(&state) >= COUNTS[0]
                        || state.counts[0] - state.agains >= ORDERED_READS
                };
                (full && !one_shot, if one_shot { 0 } else { charge })
            };
            // A parked upload's outstanding charge counts as taken, so a
            // later request can't crowd it out (LLP 1120 §6 question 8).
            let parked = state.book.parked(lane, &state.jobs[lane]);
            if full || (!waits && charge > BYTES[lane].saturating_sub(state.bytes[lane] + parked)) {
                return Err("native executor admission limit reached");
            }
            if waits && stream::uploads_full(&self.shared, r.request.body.len()) {
                return Err("native executor upload queue limit reached");
            }
            if lane == 0 {
                // Count every waiting request, including writes. Otherwise
                // 128 large reads could fill the lane before its first job
                // can reserve the response bytes it needs to start.
                let waiting = state.jobs[0].iter().map(|job| job.waiting_charge).sum();
                if charge > ORDERED_WAITING_BYTES.saturating_sub(waiting) {
                    return Err("native ordered queue byte limit reached");
                }
            }
            if one_shot {
                process::admit_one_shot(&self.shared, r.ticket, limit)?;
            }
            Ok((lane, charge, limit))
        })();
        let (lane, charge, limit) = match admitted {
            Ok(value) => value,
            Err(reason) => {
                // Failure parsing can mutate Store too. Refusals live on
                // runner tickets, but fence later ordered work here until
                // the host has settled/forgotten all those tickets.
                if ordered {
                    state.ordered_barrier = true;
                }
                return Err((reason, work));
            }
        };
        let queued_deadline = if lane == 1 && !r.request.stream && work.is_none() {
            match r
                .request
                .timeout_ms
                .map(|ms| match &self.machine {
                    Some(machine) => Ok(forget::QueuedDeadline::on_machine(
                        machine.timer.clone(),
                        lane,
                        r.ticket,
                        ms,
                    )),
                    None => forget::QueuedDeadline::arm(&self.shared, lane, r.ticket, ms),
                })
                .transpose()
            {
                Ok(deadline) => deadline,
                Err(()) => return Err(("native queued deadline could not be armed", work)),
            }
        } else {
            None
        };
        if ordered && !forgotten {
            state.ordered.push_back(r.ticket);
        }
        if !waits && !one_shot {
            state.counts[lane] += 1;
            state.bytes[lane] += charge;
        }
        if r.request.stream || r.request.is_process() {
            // Never queued behind held replies: a stream opens on its own
            // thread with its own transport, and reads there (LLP 1067 D3).
            // An upload takes its place and thread once its budget has room.
            // A process request runs on a thread of its own, and only a
            // child's stream takes a stream's place (LLP 1133.007).
            let upload = r.request.upload_progress;
            let stream = r.request.stream && !upload;
            state.streams += usize::from(stream);
            let abort = AbortController::new();
            state.running.push(Running {
                started: std::time::Instant::now(),
                ticket: r.ticket,
                lane,
                transport_lane: lane,
                charge: if waits || one_shot { 0 } else { charge },
                limit,
                abort: Some(abort.clone()),
                // An app's abort ends a one-shot's child, as a forget does (LLP 1133.007 D3).
                exchange: (r.request.is_process() && !r.request.stream).then(|| abort.clone()),
                forgotten,
                stream,
                waiting: waits || one_shot,
                read: false,
                overlap: false,
                class: r.class,
                terminal: false,
                live: 0,
                drained: false,
                blocking: false,
                generation: 0,
                claim: asynch::Claim::None,
                cached: false,
                cache_exchange: None,
                cache_replacing: cache::replacing(&r.request, r.class, r.forced, work.is_some()),
                cache_barrier: cache::barrier(&r.request, r.class, work.is_some()),
                invalidates: cache::invalidates(&r.request, work.is_some()),
            });
            rouse(&self.shared, &state);
            drop(state);
            let (shared, grants) = (self.shared.clone(), self.grants.clone());
            let (ticket, request, forced) = (r.ticket, r.request, r.forced);
            let host = self.stream_host.clone();
            if upload {
                stream::queue_upload(&shared, ticket, request, forced, grants, host, abort);
                return Ok(());
            }
            let spawned = std::thread::Builder::new()
                .name(format!("exact-stream-{ticket}"))
                .spawn({
                    let shared = shared.clone();
                    move || match request.is_process() {
                        true => process::run(&shared, ticket, request, &grants, &*host, abort),
                        false => {
                            stream::run(&shared, ticket, request, forced, &grants, &*host, abort)
                        }
                    }
                });
            if let Err(e) = spawned {
                process::release_one_shot(&shared, ticket);
                complete(&shared, ticket, failed(FailureKind::Network, e.to_string()));
            }
            return Ok(());
        }
        if let Some(cache) = state.cache.as_mut() {
            preload::refresh(cache, &r);
        }
        state.jobs[lane].push_back(Job {
            ticket: r.ticket,
            request: r.request,
            forced: r.forced,
            work,
            waiting_charge: charge,
            charge,
            limit,
            forgotten,
            queued_deadline,
            overlap,
            class: r.class,
        });
        cache::serve_ready(&mut state);
        #[cfg(test)]
        self.shared
            .worker_notifications
            .fetch_add(1, Ordering::Relaxed);
        self.shared.ready.notify_all();
        Ok(())
    }

    /// Lift the barrier once the runner retains no ordered refusal, and
    /// admit the requests held behind it, in order. One past a limit is
    /// refused, raising the barrier again over the rest: its ticket and
    /// reason are returned for the host to record (`refuse_request`).
    pub fn resume_ordered(&self) -> Vec<(u64, &'static str)> {
        let (fenced, automatic) = {
            let mut state = self.shared.state.lock().unwrap();
            let automatic = !state.ordered_barrier;
            state.ordered_barrier = false;
            state.fenced_bytes = 0;
            (std::mem::take(&mut state.fenced), automatic)
        };
        let mut refused = Vec::new();
        for place in fenced {
            match place {
                Fenced::Held(r, work, _, forgotten) => {
                    let ticket = r.ticket;
                    // Behind a refusal made just now, it is held again.
                    if let Err((reason, work)) = self.admit(*r, work, forgotten, automatic) {
                        refused.push((ticket, reason));
                        let mut state = self.shared.state.lock().unwrap();
                        self.discard(&mut state, work);
                    }
                }
                Fenced::Refused(ticket, reason) => {
                    let mut state = self.shared.state.lock().unwrap();
                    if state.ordered_barrier || !state.fenced.is_empty() {
                        state.fenced.push_back(Fenced::Refused(ticket, reason));
                    } else {
                        state.ordered_barrier = true;
                        refused.push((ticket, reason));
                    }
                }
            }
        }
        // The re-asks that waited enter after the requests held before them.
        let mut state = self.shared.state.lock().unwrap();
        place_waiting(&mut state);
        if has_ready(&state) {
            wake(&mut state);
        }
        refused
    }

    /// Waiting reads need no refusal lifted by the host (LLP 1162 D1).
    pub(super) fn resume_waiting(&self) {
        {
            let state = self.shared.state.lock().unwrap();
            if state.ordered_barrier || state.fenced.is_empty() {
                return;
            }
        }
        let refused = self.resume_ordered();
        if !refused.is_empty() {
            let mut state = self.shared.state.lock().unwrap();
            for (ticket, reason) in refused.into_iter().rev() {
                state.fenced.push_front(Fenced::Refused(ticket, reason));
            }
            state.ordered_barrier = true;
            wake(&mut state);
        }
    }
}
