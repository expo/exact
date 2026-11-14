//! File reply destinations on the blocking adapter (LLP 1162 D1, D8).
use super::*;
use ibex2::stdlib::app_fs::Staged;
use ibex2::stdlib::fetch::{over_limit, Headers};
use ibex2::stdlib::fetch_async::{aborted, BodyDestination, ResponseBody};

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Phase {
    Prepare,
    Write,
    Publish,
    Renamed,
    Cleaned,
    RedirectEnded,
    ReplyEnded,
}
#[cfg(test)]
pub(in super::super) type Hook = Arc<dyn Fn(Phase, Run) + Send + Sync>;

impl Machine {
    /// Prepare the file on a bounded reader, then give the starter its first
    /// hop. A stalled root open holds a reader and this run's charge; its
    /// starter stays free.
    pub(super) fn file_reply(self: &Arc<Self>, lane: usize, run: Run, file: FileJob) {
        if let Some(ms) = file.request.timeout_ms {
            self.timer.arm(ms, lane, run, file.abort.clone());
        }
        {
            let mut state = self.shared.state.lock().unwrap();
            if state.retired {
                return;
            }
            let Some(r) =
                find(&mut state.running, run).filter(|r| !r.terminal && r.claim == Claim::None)
            else {
                return;
            };
            r.live += 1;
        }
        let m = self.clone();
        let ended = Ended(m.clone(), lane, run);
        let job = move || {
            let _ended = ended;
            let prepared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                m.body_destination(lane, run, &file.request, &file.abort)
            }));
            let destination = match prepared {
                Ok(Ok(destination)) => destination,
                Ok(Err(outcome)) => return m.answer(lane, run, outcome),
                Err(_) => {
                    return m.answer(
                        lane,
                        run,
                        failed(FailureKind::Aborted, "native reader panicked"),
                    );
                }
            };
            let url = file.request.url.clone();
            let start: Box<dyn FnOnce() + Send> = {
                let m = m.clone();
                Box::new(move || {
                    m.fetch(
                        lane,
                        run,
                        file.request,
                        None,
                        file.forced,
                        file.abort,
                        file.retiring,
                        file.limit,
                        file.fetch,
                        None,
                        destination,
                    );
                })
            };
            m.post(lane, Event::Hop(run, url, start));
        };
        match super::super::body::reader(job) {
            Ok(thread) => self.threads.lock().unwrap().push(thread),
            Err(outcome) => self.answer(lane, run, outcome),
        }
    }

    #[cfg(test)]
    pub(super) fn body_to_phase(&self, phase: Phase, run: Run) {
        let hook = self.body_to_hook.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook(phase, run);
        }
    }

    pub(super) fn body_destination(
        self: &Arc<Self>,
        lane: usize,
        run: Run,
        request: &Request,
        abort: &AbortController,
    ) -> Result<Option<ResponseBody>, Outcome> {
        if request.body_to.is_none() {
            return Ok(None);
        }
        #[cfg(test)]
        self.body_to_phase(Phase::Prepare, run);
        let roots = self.shared.roots_for(request);
        if abort.signal().aborted() {
            return Err(failed(FailureKind::Aborted, "native request aborted"));
        }
        let staged =
            super::super::body_to::stage(roots, &self.grants, request)?.expect("file destination");
        find(&mut self.shared.state.lock().unwrap().running, run)
            .expect("preparing run")
            .live += 1;
        Ok(Some(ResponseBody {
            destination: Arc::new(Destination {
                file: Mutex::new((Some(staged), 0)),
                machine: self.clone(),
                abort: abort.clone(),
                limit: super::super::body_to::limit(request),
                ended: Ended(self.clone(), lane, run),
            }),
            error_limit: exact_runner::BODY_TO_HELD_BYTES as usize,
        }))
    }
}

struct Destination {
    file: Mutex<(Option<Staged>, u64)>,
    machine: Arc<Machine>,
    abort: AbortController,
    limit: usize,
    ended: Ended,
}

impl BodyDestination for Destination {
    fn write(&self, bytes: &[u8]) -> Result<(), HostError> {
        #[cfg(test)]
        self.machine.body_to_phase(Phase::Write, self.ended.2);
        self.abort.signal().check()?;
        let mut file = self.file.lock().unwrap();
        if file.1.saturating_add(bytes.len() as u64) > self.limit as u64 {
            return Err(over_limit(self.limit));
        }
        file.0
            .as_mut()
            .expect("unpublished destination")
            .write_all(bytes)?;
        file.1 += bytes.len() as u64;
        Ok(())
    }

    fn finish(&self, headers: &mut Headers) -> Result<(), HostError> {
        #[cfg(test)]
        self.machine.body_to_phase(Phase::Publish, self.ended.2);
        self.abort.signal().check()?;
        let (staged, written) = {
            let mut file = self.file.lock().unwrap();
            (file.0.take().expect("unpublished destination"), file.1)
        };
        staged.publish_if(|| {
            let mut state = self.machine.shared.state.lock().unwrap();
            if state.retired || self.abort.signal().aborted() {
                return Err(aborted());
            }
            // The rename cannot be cancelled. Its result owns the transition;
            // no disk operation runs with this lock held (LLP 1162 D1).
            if !claim(&mut state, self.ended.2, Claim::Publishing) {
                return Err(aborted());
            }
            Ok(())
        })?;
        #[cfg(test)]
        self.machine.body_to_phase(Phase::Renamed, self.ended.2);
        headers.set(exact_runner::BODY_WRITTEN, &written.to_string());
        Ok(())
    }
}

impl Drop for Destination {
    fn drop(&mut self) {
        drop(self.file.get_mut().unwrap().0.take());
        #[cfg(test)]
        self.machine.body_to_phase(Phase::Cleaned, self.ended.2);
        // `ended` posts only after the staging file's cleanup has returned.
    }
}

/// Whether a drained reply returns its admission place now. File bodies keep
/// theirs through physical cleanup, as their worker does (LLP 1162 D1).
pub(in super::super) fn release_count(book: &mut Book, run: &mut Running) -> bool {
    run.drained = true;
    if book.downloads.contains(&(run.ticket, run.generation)) && run.live > 0 {
        book.held_counts.insert((run.ticket, run.generation));
        false
    } else {
        true
    }
}

#[cfg(test)]
#[path = "async_body_to_tests.rs"]
mod tests;
