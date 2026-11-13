//! Physical releases outlive retirement and its logical work (LLP 1162 D8).
use super::*;

impl Machine {
    pub(in super::super) fn retire(&self) {
        self.stop(&mut self.shared.state.lock().unwrap());
        let transports = std::mem::take(&mut *self.transports.lock().unwrap());
        for (transport, _) in transports.iter().flatten() {
            transport.retire();
        }
        self.worker_budget.0.lock().unwrap().clear();
        self.shared.ready.notify_all();
    }

    pub(in super::super) fn stop(&self, state: &mut State) {
        if state.book.retiring {
            return;
        }
        self.retired.store(true, Ordering::Release);
        state.book.retiring = true;
        state
            .book
            .trash
            .push(Box::new(self.shared.uploads.retire()));
        self.timer.stop();
        state.book.deciders.clear();
        for lane in 0..2 {
            let jobs = std::mem::take(&mut state.jobs[lane]);
            state.bytes[lane] -= jobs.iter().map(|j| j.charge).sum::<usize>();
            state.book.trash.push(Box::new(jobs));
            let outcomes = std::mem::take(&mut state.completed[lane]);
            state.bytes[lane] -= outcomes.iter().map(|d| d.bytes).sum::<usize>();
            state.book.trash.push(Box::new(outcomes));
        }
        state
            .book
            .trash
            .push(Box::new(std::mem::take(&mut state.fenced)));
        state
            .book
            .trash
            .push(Box::new(std::mem::take(&mut state.book.hops)));
        state
            .book
            .trash
            .push(Box::new(std::mem::take(&mut state.book.files)));
        state.book.sized.clear();
        state.book.held_counts.clear();
        state.book.multiplexed.iter_mut().for_each(HashSet::clear);
        state.fenced_bytes = 0;
        state.light = 0;
        state.agains = 0;
        state.ordered.clear();
        state.waiting.clear();
        state.waiting_set.clear();
        state.counts = [0, 0];
        state.streams = 0;
        let mut runs = Vec::new();
        state.running.retain(|r| !r.waiting);
        for r in &mut state.running {
            let run = (r.ticket, r.generation);
            let counted = r.generation == 0 || !r.terminal || state.book.downloads.contains(&run);
            state.counts[r.lane] += usize::from(counted);
            state.streams += usize::from(r.stream);
            if r.generation != 0 {
                if counted {
                    state.book.held_counts.insert(run);
                }
                r.terminal = true;
                r.drained = true;
                runs.push(run);
            }
        }
        for run in runs {
            physical(state, run);
        }
    }

    pub(super) fn apply_retired(
        self: &Arc<Self>,
        state: &mut State,
        lane: usize,
        event: Event,
    ) -> Option<After> {
        match event {
            Event::Ended(..) | Event::ReaderEnded(_) | Event::Released(_) => {
                self.apply(state, lane, event)
            }
            Event::Read(run, Ok(prepared)) => self.read(state, lane, run, Ok(prepared)),
            event => {
                state.book.trash.push(Box::new(event));
                None
            }
        }
    }

    pub(super) fn disposer(self: Arc<Self>) {
        let mut state = self.shared.state.lock().unwrap();
        loop {
            if state.retired && !state.book.retiring {
                self.stop(&mut state);
            }
            self.threads
                .lock()
                .unwrap()
                .retain(|thread| !thread.is_finished());
            if !state.discard.is_empty() || !state.book.trash.is_empty() {
                let discarded = std::mem::take(&mut state.discard);
                let trash = std::mem::take(&mut state.book.trash);
                drop(state);
                drop((discarded, trash));
                state = self.shared.state.lock().unwrap();
                continue;
            }
            if state.retired
                && state.running.is_empty()
                && state.book.uploads.is_empty()
                && state.book.readers.is_empty()
            {
                for f in &self.finishers {
                    f.lock().unwrap().take();
                }
                drop(state);
                loop {
                    let threads = std::mem::take(&mut *self.threads.lock().unwrap());
                    if threads.is_empty() {
                        break;
                    }
                    for thread in threads {
                        let _ = thread.join();
                    }
                }
                state = self.shared.state.lock().unwrap();
                if state.discard.is_empty() && state.book.trash.is_empty() {
                    return;
                }
                continue;
            }
            state = self.shared.ready.wait(state).unwrap();
        }
    }
}

/// Streams and processes still run through their original physical owner.
pub(in super::super) fn external_end(shared: &Shared, state: &mut State, ticket: u64) {
    if let Some(at) = state.running.iter().position(|r| r.ticket == ticket) {
        let run = state.running.swap_remove(at);
        state.bytes[run.lane] -= run.charge;
        state.counts[run.lane] -= usize::from(!run.waiting);
        state.streams -= usize::from(run.stream);
    }
    shared.ready.notify_all();
}
