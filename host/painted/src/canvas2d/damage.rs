//! Changed backing pixels between two canvas snapshots.
use super::{prefix::Checkpoint, Replayer};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tiny_skia::{IntRect, Rect};

/// A canvas revision and the device pixels that may differ from its predecessor.
#[derive(Clone, Copy, Debug)]
pub struct CanvasUpdate {
    pub(crate) previous: u64,
    pub(crate) revision: u64,
    pub(crate) area: Option<IntRect>,
}

impl CanvasUpdate {
    pub(super) fn after(previous: u64) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            previous,
            revision: NEXT.fetch_add(1, Ordering::Relaxed),
            area: None,
        }
    }
}

impl Replayer {
    fn canvas_bounds(&self) -> IntRect {
        IntRect::from_xywh(0, 0, self.pixmap.width(), self.pixmap.height()).unwrap()
    }

    pub(super) fn damage_all(&mut self) {
        self.update.area = Some(self.canvas_bounds());
    }

    pub(super) fn damage_edges(&mut self, x0: usize, y0: usize, x1: usize, y1: usize) {
        self.include_damage(IntRect::from_ltrb(
            x0 as i32, y0 as i32, x1 as i32, y1 as i32,
        ));
    }

    pub(super) fn damage_path(&mut self, bounds: Option<Rect>) {
        let canvas = self.canvas_bounds();
        // Include edge coverage and hairline strokes after applying the transform.
        let bounds = bounds
            .and_then(|r| r.outset(2., 2.))
            .and_then(|r| r.round_out());
        self.include_damage(bounds.unwrap_or(canvas).intersect(&canvas));
    }

    pub(super) fn include_damage(&mut self, area: Option<IntRect>) {
        let Some(new) = area else { return };
        self.update.area = match self.update.area {
            Some(old) => IntRect::from_ltrb(
                old.left().min(new.left()),
                old.top().min(new.top()),
                old.right().max(new.right()),
                old.bottom().max(new.bottom()),
            ),
            None => Some(new),
        };
    }

    pub(super) fn restore_tail(&mut self, checkpoint: &Checkpoint) {
        let Some(rect) = checkpoint.tail else { return };
        self.include_damage(Some(rect));
        let Some(working) = Arc::get_mut(&mut self.pixmap) else {
            // A painter still owns this frame. The next write must detach.
            self.pixmap = checkpoint.pixmap.clone();
            return;
        };
        let width = working.width() as usize;
        let src = checkpoint.pixmap.pixels();
        let dst = working.pixels_mut();
        for row in rect.top()..rect.bottom() {
            let start = row as usize * width + rect.left() as usize;
            let end = start + rect.width() as usize;
            dst[start..end].copy_from_slice(&src[start..end]);
        }
    }
}
