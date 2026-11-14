//! The preload set's exchanges and idle expiry (LLP 1162.000 D1, D5).
use super::*;
use ibex2::stdlib::fetch_async::{FetchEvents, HeaderCaps};

struct Events {
    generation: usize,
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
        let _ = self
            .send
            .send(Message::Reply(self.index, self.generation, result, at));
    }
    fn next_hop(&self, url: &str, start: Box<dyn FnOnce() + Send>) {
        self.visited.lock().unwrap().push(url.into());
        let _ = self.send.send(Message::Hop(start));
    }
}

fn finish(
    cache: &mut cache::Cache,
    index: usize,
    generation: usize,
    result: Result<Collected, HostError>,
    completed: Instant,
) {
    let held = &cache.preloads[index];
    if held.generation != generation {
        return;
    }
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
        + request.preload.as_ref().map_or(0, |name| name.len())
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

pub(super) fn run(
    destination: &Arc<Location>,
    send: &mpsc::Sender<Message>,
    receive: mpsc::Receiver<Message>,
    host: &StreamHost,
) {
    let mut launch_workers = None;
    let mut prefetch_workers = None;
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
                .chain(pending_due(cache))
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
                let prefetch = update(destination, |cache| {
                    indices
                        .first()
                        .is_some_and(|(i, _)| cache.preloads[*i].prefetch)
                })
                .unwrap_or(false);
                let workers = if prefetch {
                    &mut prefetch_workers
                } else {
                    &mut launch_workers
                };
                if workers.is_none() {
                    let guards: Vec<_> = (0..MAX_PRELOADS)
                        .map(|_| ThreadGuard::new(destination))
                        .collect();
                    match ibex2::stdlib::fetch_async::BoundedWorkerPool::with_worker_guards(
                        guards,
                        MAX_PRELOADS,
                    ) {
                        Ok(pool) => *workers = Some(Arc::new(pool)),
                        Err(_) => {
                            update(destination, cancel_all);
                            return;
                        }
                    }
                }
                start_indices(
                    destination,
                    send,
                    host,
                    &grants,
                    workers.as_ref().unwrap(),
                    indices,
                )
            }
            Ok(Message::Reply(i, generation, result, at)) => {
                update(destination, |c| finish(c, i, generation, result, at));
            }
            Ok(Message::Hop(start)) => start(),
            Ok(Message::Barrier(ready)) => {
                let _ = ready.send(());
            }
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
    indices: Vec<(usize, usize)>,
) {
    for (i, generation) in indices {
        let start = update(destination, |cache| {
            expire(cache);
            let held = &cache.preloads[i];
            if held.generation != generation {
                return None;
            }
            let ReplyState::Pending { abort, .. } = &held.reply else {
                return None;
            };
            let request = held.request.as_ref().expect("pending request");
            if !valid_request(request) {
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
            if !held.prefetch {
                cache.journal.push(held.fact(Event::Sent));
            }
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
                generation,
                clock,
                answered_at,
                observed,
                visited,
            }),
        );
    }
}
