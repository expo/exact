//! Canvas 2D on Apple, the Rust half (LLP 1056 D4, D7): after each turn's
//! layout, every 2D canvas's geometry from the kernel, its due draws, and
//! their stamped lists onto the batch as `canvas2d` ops, which the Swift
//! presenter replays into Core Graphics off the main thread (§8.3).

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// The device's physical memory, as a presenter last said: process-wide.
/// The display's scale is each session's (`Host::canvas_scale`): two
/// windows can be on different displays or at different page zoom.
static MEMORY: AtomicU64 = AtomicU64::new(8 << 30);

/// The scale a session's canvases back at before its presenter tells one.
pub(crate) const DEFAULT_SCALE: f64 = 2.0;

/// What the presenter knows of the device (`exact_canvas_display`).
pub fn set_memory(memory: f64) {
    if memory.is_finite() && memory > 0.0 {
        MEMORY.store(memory as u64, Ordering::Relaxed);
    }
}

/// `scale` when a canvas can back at it.
pub(crate) fn usable_scale(scale: f64) -> Option<f64> {
    (scale.is_finite() && scale > 0.0).then_some(scale)
}

impl<D: DataSource> Host<D> {
    /// The limits for this device: WebKit's iOS area on iOS, tvOS and watchOS, a quarter of
    /// physical memory everywhere (LLP 1056 D4, r3).
    pub(crate) fn canvas_limits(&mut self) {
        let memory = MEMORY.load(Ordering::Relaxed);
        self.runner.set_canvas_limits(exact_runner::Limits::native(
            memory,
            cfg!(any(
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos"
            )),
        ));
    }

    /// This turn's canvas work, after layout: geometry, then due draws and
    /// their lists, unless draws are deferred (LLP 1072 §8.5): then the batch
    /// says whether a draw is owed, and `exact_canvas_draw` runs it.
    pub(crate) fn canvas_turn(&mut self, batch: &mut Batch) {
        if !self.canvas_deferred {
            self.canvas_draw_turn(batch);
            return;
        }
        if self.runner.plan().surfaces.is_empty() {
            return;
        }
        self.runner.layout_canvases(self.canvas_scale);
        self.canvas_owed_onto(batch);
    }

    /// Where draws are deferred, whether one is owed now, on every batch:
    /// the presenter takes the last batch's word for it, and a turn that
    /// ran no canvas work (a list's report inside its window, a draw's own
    /// turn, a tick) must not say "none" while a new canvas waits.
    pub(crate) fn canvas_owed_onto(&self, batch: &mut Batch) {
        if self.canvas_deferred {
            let held = &self.canvas_held;
            batch.canvas_owed(self.runner.canvas_owed(&|v| !held.contains(&v)));
        }
    }

    /// Where draws are deferred, a turn of canvas work alone: geometry, due
    /// draws, their lists (`exact_canvas_draw`, LLP 1072 §8.5). Main does not
    /// wait for it: the owner runs it after the turn that owed it.
    pub fn canvas_draw(&mut self) -> String {
        let mut batch = Batch::new();
        self.canvas_draw_turn(&mut batch);
        self.finish(batch, None)
    }

    /// Draw canvases in the turns main waits on (false), or only in their
    /// own turn and a frame's tick (true: LLP 1072 §8.5, iOS off the agent).
    pub fn set_canvas_deferred(&mut self, deferred: bool) {
        self.canvas_deferred = deferred;
    }

    /// Geometry, every due draw, and the lists onto the batch.
    pub(crate) fn canvas_draw_turn(&mut self, batch: &mut Batch) {
        if self.runner.plan().surfaces.is_empty() {
            return;
        }
        self.runner.layout_canvases(self.canvas_scale);
        let held = &self.canvas_held;
        self.runner.draw_canvases(&|v| !held.contains(&v));
        let lists = self.runner.take_canvas_lists();
        for c in &lists {
            batch.canvas2d(c);
        }
        if !lists.is_empty() {
            // The last two batches' lists stay alive: the presenter's reader
            // copies them out of this memory (`Batch::canvas2d`).
            self.canvas_kept.swap(0, 1);
            self.canvas_kept[1] = lists;
        }
        batch.canvas_images(self.runner.take_canvas_image_requests());
    }

    /// The presenter's replay of `view` is (`held`) or is no longer behind
    /// (LLP 1056 D5, §8.3): while it is, the canvas's frame request waits,
    /// as a browser's animation frame waits for the last one to present, so
    /// frames drop rather than queue. Every other cause still draws.
    pub fn canvas_held(&mut self, view: ViewId, held: bool) {
        if held {
            self.canvas_held.insert(view);
        } else {
            self.canvas_held.remove(&view);
        }
    }

    /// The app's Core Text measurer for Canvas 2D (LLP 1056 D8).
    pub(crate) fn set_canvas_text(
        &mut self,
        f: crate::canvas_text::CanvasTextFn,
        ctx: *mut std::ffi::c_void,
    ) {
        self.runner
            .set_canvas_text(std::sync::Arc::new(crate::canvas_text::CallbackText::new(
                f, ctx,
            )));
    }

    /// The presenter decoded image handle `src` (`size` in pixels) or could
    /// not (`None`): the canvases that asked for it draw again (LLP 1056 D9).
    pub fn canvas_image(&mut self, src: &str, size: Option<(u32, u32)>) -> String {
        self.runner.canvas_image(
            src,
            size.ok_or_else(|| "the image could not be loaded or decoded".to_string()),
            &[],
        );
        let mut batch = Batch::new();
        self.canvas_turn(&mut batch);
        self.finish(batch, None)
    }

    /// The presenter let the decoded `src` go: a canvas that draws it again
    /// asks for it again (LLP 1056 D9).
    pub fn canvas_image_released(&mut self, src: &str) {
        self.runner.canvas_image_released(src);
    }

    /// The display changed: canvases redraw at this session's new scale (a
    /// new generation, LLP 1056 D4).
    pub fn canvas_display(&mut self, scale: f64) -> String {
        if let Some(scale) = usable_scale(scale) {
            self.canvas_scale = scale;
        }
        self.canvas_limits();
        let mut batch = Batch::new();
        self.canvas_turn(&mut batch);
        self.finish(batch, None)
    }
}
