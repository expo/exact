//! A batch's end: the last commit's requests handed to the page, work a
//! source answers at once fulfilled, and the app's aborts answered and
//! cancelled.

use super::*;
use exact_runner::{Dispatch, Work};

impl<D: DataSource> Host<D> {
    /// A fetch the app aborted outside this host's turns (a worker realm's
    /// door, between batches): its answer resumes, and the batch cancels it.
    pub fn collect_aborts(&mut self) -> String {
        self.batch_for(&[], None)
    }

    /// Hand the last commit's requests to the page and finish the batch. A
    /// continuation is dispatched here, on this thread, with the store as
    /// committed (LLP 1027.002 D3); one a source holds is parked and
    /// released after a later commit. Work a source answers at once — a
    /// main member's turn in an ordered set — is fulfilled here, and the
    /// commits it makes join the batch.
    pub(super) fn complete(&mut self, mut batch: Batch, error: Option<String>) -> String {
        let mut error = error;
        loop {
            let mut immediate = Vec::new();
            for r in self.runner.take_requests() {
                self.emit_request(r, &mut batch, &mut immediate);
            }
            for (token, dispatch) in self.runner.release_work() {
                if let Some(r) = self.parked.remove(&token) {
                    self.emit_dispatch(r, dispatch, &mut batch, &mut immediate);
                }
            }
            // A fetch the app aborted, in these turns or before: its answer
            // resumes now, and the page cancels the request after the batch,
            // whatever its method.
            self.runner.collect_aborts();
            while let Some(aborted) = self.runner.take_aborted_answer() {
                immediate.push(aborted);
            }
            if immediate.is_empty() || error.is_some() {
                break;
            }
            let mut receipts = Vec::new();
            for (ticket, outcome) in immediate {
                match self.runner.fulfill(ticket, outcome) {
                    Ok(Some(receipt)) => receipts.push(Timed {
                        at_ms: self.now_ms,
                        receipt,
                    }),
                    Ok(None) => {}
                    Err(e) => {
                        error = Some(format!("{e:?}"));
                        break;
                    }
                }
            }
            self.emit_receipts(&receipts, &mut batch);
        }
        for ticket in self.runner.take_explicit_aborts() {
            batch.abort(ticket);
        }
        let collections = self.runner.collections_json();
        if collections != self.collections {
            self.collections = collections;
            batch.collections(&self.collections);
        }
        let language = self.runner.resolved_locale();
        if self.language.as_deref() != Some(language) {
            batch.language(language, self.runner.direction());
            self.language = Some(language.into());
        }
        // A bar reserves the large title's line even when its heading has authored typography.
        use exact_kernel::style::relative::{text_style, text_style_size};
        let large_title = text_style_size(
            text_style("-exact-large-title").expect("schema text style"),
            self.runner.kernel().root_font_size(),
        );
        // As with a resolved style row, overflow keeps the last finite size.
        if large_title.is_finite() && self.large_title_size != Some(large_title) {
            batch.large_title(large_title);
            self.large_title_size = Some(large_title);
        }
        self.finish(batch, error.as_deref())
    }

    fn emit_request(
        &mut self,
        r: RequestOut,
        batch: &mut Batch,
        immediate: &mut Vec<(u64, Outcome)>,
    ) {
        if let Some(message) = (r.request.prefetch && r.class != exact_runner::RequestClass::Read)
            .then_some("exactPrefetch requires a plain HTTP read")
            .or(crate::batch::request_refusal(&r.request))
        {
            batch.refuse(r.ticket, message);
            return;
        }
        if r.request.is_auth() {
            return self.emit_auth(r, batch);
        }
        let Some(token) = r.request.continuation else {
            batch.request(&r);
            return;
        };
        let dispatch = self.runner.dispatch_work(token);
        self.emit_dispatch(r, dispatch, batch, immediate);
    }

    fn emit_dispatch(
        &mut self,
        mut r: RequestOut,
        dispatch: Dispatch,
        batch: &mut Batch,
        immediate: &mut Vec<(u64, Outcome)>,
    ) {
        match dispatch {
            Dispatch::Host(registry) => {
                r.request.continuation = Some(registry);
                batch.request(&r);
            }
            Dispatch::Run(Work::Now(work)) => immediate.push((r.ticket, work())),
            // A re-ask settles at once, as the no-op it replaces did.
            Dispatch::Again => immediate.push((r.ticket, Dispatch::again_outcome())),
            Dispatch::Run(Work::Later(_)) => immediate.push((
                r.ticket,
                Outcome::Failed {
                    kind: FailureKind::Unsupported,
                    message: "an owner thread is unavailable on this host".into(),
                },
            )),
            Dispatch::Held => {
                if let Some(token) = r.request.continuation {
                    self.parked.insert(token, r);
                }
            }
            Dispatch::Missing => immediate.push((
                r.ticket,
                Outcome::Failed {
                    kind: FailureKind::Unsupported,
                    message: "missing or consumed browser continuation".into(),
                },
            )),
        }
    }
}
