//! Canvas PNG jobs share the surface completion loop.
use super::*;

impl<D: DataSource> Presenter<D> {
    pub(crate) fn draw_and_capture_canvases(&mut self, frame: bool) {
        // LLP 1056: the 2D canvases' draws for this turn's commits.
        self.dirty |= self
            .host
            .sync_canvases(self.brush.scale as f64, frame, &self.assets);
        for r in std::mem::take(&mut self.surfaces.captures) {
            if !self.host.runner().holds(r.ticket) {
                continue;
            }
            self.host.runner().fail_stalled_canvas_capture(&r.request);
            if r.request.canvas_capture_pending() {
                self.surfaces.captures.push(r);
                continue;
            }
            let outcome = self.host.capture_canvas(&r);
            self.surfaces.outcomes.push((r.ticket, outcome));
        }
    }
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
