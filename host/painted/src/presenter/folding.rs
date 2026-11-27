//! Android's window-relative FoldingFeature in layout viewport points (LLP 1078 D11).
use exact_kernel::Rect;
use exact_runner::{Fold, Posture};

#[derive(Clone, Copy)]
pub(super) struct Feature {
    pub posture: Posture,
    pub axis: i32,
    pub bounds: [f32; 2],
}

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub(super) struct Segments {
    pub fold: Fold,
    pub rects: Option<[Rect; 2]>,
}

impl Feature {
    /// The window decides the count before safe areas or the keyboard cover a
    /// pane. Fit each rectangle to the space the page can use.
    pub(super) fn segments(
        self,
        [x, y, w, h]: [f32; 4],
        visible_height: f32,
        last_row_only: bool,
    ) -> Segments {
        let [start, end] = self.bounds;
        let (cols, rows, rects) = match self.axis {
            1 => {
                let left = (start - x).clamp(0., w);
                let right = (end - x).clamp(0., w);
                (
                    2,
                    1,
                    Some([
                        Rect::new(0., 0., left, visible_height),
                        Rect::new(right, 0., w - right, visible_height),
                    ]),
                )
            }
            2 => {
                let top = (start - y).clamp(0., if last_row_only { h } else { visible_height });
                let bottom = (end - y).clamp(0., h);
                (
                    1,
                    2,
                    Some([
                        Rect::new(0., 0., w, top),
                        Rect::new(0., bottom, w, (visible_height - bottom).max(0.)),
                    ]),
                )
            }
            _ => (1, 1, None),
        };
        Segments {
            fold: Fold {
                posture: self.posture,
                cols,
                rows,
            },
            rects,
        }
    }
}
