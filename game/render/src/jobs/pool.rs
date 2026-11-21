use super::{Cancel, Completed, Done, Job, JobError, Jobs, Lane, Order, Queued, Shared, Ticket};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::thread;

impl<J: Job> Jobs<J> {
    pub(super) fn wake_pool(&mut self) {
        #[cfg(test)]
        if self.fail_pool_start {
            self.wake_pool_with(|_| Err(std::io::ErrorKind::WouldBlock.into()));
            return;
        }
        self.wake_pool_with(|shared| thread::Builder::new().spawn(move || work(&shared)));
    }

    pub(super) fn wake_pool_with(
        &mut self,
        mut spawn: impl FnMut(Arc<Shared<J>>) -> std::io::Result<thread::JoinHandle<()>>,
    ) {
        if self.workers.is_empty() {
            let state = self.shared.state.lock().unwrap();
            if state.lane != Lane::Pool || state.queue.is_empty() {
                return;
            }
            let available = thread::available_parallelism().map_or(1, |n| n.get());
            let count = available
                .saturating_sub(2)
                .clamp(1, 4)
                .min(self.shared.limits.running);
            drop(state);
            for _ in 0..count {
                if let Ok(worker) = spawn(self.shared.clone()) {
                    self.workers.push(worker);
                }
            }
            if self.workers.is_empty() {
                self.pool_unavailable = true;
                self.shared.state.lock().unwrap().lane = Lane::Inline;
            }
        }
        self.shared.changed.notify_all();
    }

    #[cfg(test)]
    pub(super) fn wait_for_completions(&self, count: usize) {
        let state = self.shared.state.lock().unwrap();
        let (state, _) = self
            .shared
            .changed
            .wait_timeout_while(state, std::time::Duration::from_secs(5), |state| {
                state.completed.len() < count
            })
            .unwrap();
        let completed = state.completed.len();
        drop(state);
        assert!(completed >= count, "results did not complete");
    }

    #[cfg(test)]
    pub(super) fn wait_for_workers(&self) {
        let state = self.shared.state.lock().unwrap();
        let (state, _) = self
            .shared
            .changed
            .wait_timeout_while(state, std::time::Duration::from_secs(5), |state| {
                !state.running.is_empty()
            })
            .unwrap();
        assert!(state.running.is_empty(), "workers did not finish");
    }
}

pub(super) fn work<J: Job>(shared: &Shared<J>) {
    loop {
        let mut state = shared.state.lock().unwrap();
        while !state.closed
            && (state.lane != Lane::Pool
                || state.queue.is_empty()
                || (!state.completed.is_empty()
                    && state.completed_bytes >= shared.limits.completed_bytes as u64))
        {
            state = shared.changed.wait(state).unwrap();
        }
        if state.closed {
            return;
        }
        let (order, Queued { ticket, job, .. }, _) = state.pop().unwrap();
        let cancel = Cancel::default();
        state.running.insert(order, cancel.clone());
        drop(state);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let mut running = Running {
                shared,
                order,
                ticket,
                output: None,
            };
            let result = shared.run(job, &cancel, None);
            let bytes = match &result {
                Ok(output) => J::output_bytes(output),
                Err(error) => error.0.capacity(),
            };
            running.output = Some((result, bytes));
        }));
    }
}

struct Running<'a, J: Job> {
    shared: &'a Shared<J>,
    order: Order,
    ticket: Ticket,
    output: Option<(Result<J::Output, JobError>, usize)>,
}

impl<J: Job> Drop for Running<'_, J> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.running.remove(&self.order);
        if !state.closed {
            if state.latest.get(&self.ticket.key) == Some(&(self.ticket.generation, self.order)) {
                let (result, bytes) = self.output.take().unwrap_or_else(|| {
                    let message = String::from("job panicked");
                    let bytes = message.capacity();
                    (Err(JobError(message)), bytes)
                });
                let done = Done {
                    key: self.ticket.key,
                    generation: self.ticket.generation,
                    result,
                };
                state.completed_bytes += bytes as u64;
                state.record_retained_peak();
                state
                    .completed
                    .insert(self.order, Completed { done, bytes });
            } else {
                self.shared.counts.lock().unwrap().stale += 1;
            }
        }
        self.shared.changed.notify_all();
    }
}
