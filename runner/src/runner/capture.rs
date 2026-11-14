//! Captures wait for a completed drawing of their mounted canvas (LLP 1056 D9).
use super::*;

impl<D: DataSource> Runner<D> {
    /// Inspect a canvas request against this turn's state. The runner uses this
    /// to complete its owned capture jobs; hosts consume `Request::take_canvas_capture`
    /// after geometry instead of selecting a live canvas here.
    pub fn canvas_capture(
        &self,
        request: &crate::Request,
    ) -> Option<Result<crate::canvas_capture::CanvasCapture, crate::Outcome>> {
        self.canvas_capture_target(request, None)
    }

    fn canvas_capture_target(
        &self,
        request: &crate::Request,
        target: Option<(ViewId, u64, u64)>,
    ) -> Option<Result<crate::canvas_capture::CanvasCapture, crate::Outcome>> {
        let Some(crate::SurfaceRequest::Canvas { name, path, .. }) = request.surface.as_deref()
        else {
            return None;
        };
        let found = match (
            crate::canvas_capture::path_refusal(path),
            self.canvases.as_ref(),
        ) {
            (Some(why), _) => Err(("NotAllowedError", why)),
            (None, Some(c)) => c.capture(name, path, target, &|v| self.kernel.node(v).is_some()),
            (None, None) => Err((
                "NotFoundError",
                format!("no canvas of surface {name:?} is mounted"),
            )),
        };
        Some(found.map_err(|(name, why)| crate::canvas_capture::refused(name, &why)))
    }

    /// After draining draws, fail a capture that has no executor or drawing left to wait for.
    pub fn fail_stalled_canvas_capture(&self, request: &crate::Request) {
        if request.canvas_capture_pending()
            && self.data.ready()
            && !self.canvas_owed(&|_| true)
            && !self.canvases.as_ref().is_some_and(|c| c.in_flight())
        {
            if let Some(crate::SurfaceRequest::Canvas { job: Some(job), .. }) =
                request.surface.as_deref()
            {
                job.fail();
            }
        }
    }

    pub(super) fn pin_canvas_captures(&mut self) {
        for i in 0..self.requests.len() {
            if !matches!(
                self.requests[i].request.surface.as_deref(),
                Some(crate::SurfaceRequest::Canvas { job: None, .. })
            ) {
                continue;
            }
            let request = self.requests[i].request.clone();
            let Some(crate::SurfaceRequest::Canvas { name, .. }) = request.surface.as_deref()
            else {
                unreachable!()
            };
            let kernel = &self.kernel;
            let target = self
                .canvases
                .as_mut()
                .and_then(|c| c.request_capture(name, &|v| kernel.node(v).is_some()));
            let refusal = target
                .is_none()
                .then(|| self.canvas_capture(&request).unwrap());
            let job = crate::canvas_capture::CaptureJob::new(target, refusal, &self.canvas_owner);
            self.canvas_captures.push((request, job.weak()));
            if let Some(crate::SurfaceRequest::Canvas { job: slot, .. }) =
                self.requests[i].request.surface.as_deref_mut()
            {
                *slot = Some(job);
            }
        }
        self.retire_unmounted_captures();
        if self.plan.surfaces.is_empty() || self.canvases.is_none() {
            self.complete_canvas_captures();
        }
    }

    pub(super) fn retire_unmounted_captures(&self) {
        for (_, job) in &self.canvas_captures {
            if let Some(job) = job.upgrade() {
                job.lock().unwrap().unmount(|v, lifetime| {
                    self.kernel.node(v).is_some()
                        && self
                            .canvases
                            .as_ref()
                            .is_some_and(|c| c.mounted(v, lifetime))
                });
            }
        }
    }

    pub(super) fn complete_canvas_captures(&mut self) {
        self.retire_unmounted_captures();
        self.canvas_captures
            .retain(|(_, job)| job.strong_count() != 0);
        for (request, job) in &self.canvas_captures {
            if let Some(job) = job.upgrade() {
                let mut job = job.lock().unwrap();
                if let crate::canvas_capture::CaptureState::Pending {
                    target: Some(target),
                    ..
                } = &*job
                {
                    let found = self.canvas_capture_target(request, Some(*target)).unwrap();
                    job.complete(found);
                }
            }
        }
    }
}
