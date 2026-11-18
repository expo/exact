//! A request on an archive that links no I/O (LLP 1047.001, "The host's I/O
//! by use"): no executor runs it, so it is answered at once as
//! `Unlinked("io")`, where it would wait for ever.
use super::*;

/// What a source is answered with; the journal's line for the request says it.
pub(super) const UNLINKED_IO: &str =
    "Unlinked(\"io\"): this binary links no I/O, which runs this request; the source declares exact_runner::HostWork, or app.json's link names \"io\"";

impl<D: DataSource> Bridge<D> {
    /// An activity's or a surface's work, which the presenter takes whether
    /// or not an executor runs: `false` for work only the executor runs.
    pub(super) fn presenter_work(
        h: &mut Host<D>,
        presenter: &mut crate::batch::Batch,
        r: &exact_runner::RequestOut,
    ) -> bool {
        if r.request.url == "exact-activity:" {
            if let Ok(request) = std::str::from_utf8(&r.request.body) {
                presenter.activity(r.ticket, request);
            }
            return true;
        }
        let Some(surface) = r.request.surface.as_deref() else {
            return false;
        };
        let oversized = matches!(surface, SurfaceRequest::Restore { bytes, .. } if bytes.len() > MAX_HOST_WORK_BYTES);
        let refusal = oversized
            .then(|| "surface restore exceeds 16 MiB".to_string())
            .or_else(|| r.request.check_surface_grant(&h.grants()).err());
        h.surface_work(presenter, r, refusal.as_deref());
        true
    }

    /// Hand the presenter its work and refuse the rest, then wake the
    /// session so its next pump settles the refusals and any aborts:
    /// nothing else would.
    pub(super) fn emit_without_io(
        h: &mut Host<D>,
        wake: Option<(crate::executor::WakeFn, *mut c_void)>,
        s: &mut String,
    ) {
        let mut presenter = crate::batch::Batch::new();
        for r in h.take_requests() {
            // A deadline or long poll no host can keep is refused with its
            // own reason, as the executor's path refuses it.
            let unkept = r
                .request
                .timeout_refusal()
                .or(r.request.long_poll_refusal());
            if let Some(why) = unkept {
                h.refuse_request(r.ticket, why, r.request.is_ordered());
            } else if !Self::presenter_work(h, &mut presenter, &r) {
                h.refuse_request(r.ticket, UNLINKED_IO, r.request.is_ordered());
            }
        }
        h.runner_mut().collect_aborts();
        Self::wake_for_refusals(h, wake);
        presenter.prepend_to(s);
    }

    /// Wake the session while refusals (an abort's answer among them) wait
    /// to settle.
    pub(super) fn wake_for_refusals(
        h: &Host<D>,
        wake: Option<(crate::executor::WakeFn, *mut c_void)>,
    ) {
        if let Some((wake, ctx)) = wake.filter(|_| h.has_request_refusals(true)) {
            wake(ctx);
        }
    }
}

#[cfg(test)]
#[path = "without_io_tests.rs"]
mod tests;
