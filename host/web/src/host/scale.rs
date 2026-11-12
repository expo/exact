//! The display's scale as a layout input (LLP 1115.000 D10.3): the page's
//! `devicePixelRatio`, which a hairline (`-exact-hairline`) is one device
//! pixel of. The glue says it after boot and when the page's facts change,
//! beside the root font size.

use super::*;

impl<D: DataSource> Host<D> {
    /// Publish ephemeral HUD metrics without involving the application data seam.
    pub fn canvas_performance_record(&mut self, name: &str, json: Option<&str>) -> String {
        let (receipts, error) = match self.runner.set_canvas_performance(name, json) {
            Ok(Some(receipt)) => (
                vec![Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => (vec![], None),
            Err(e) => (vec![], Some(format!("canvas performance: {e:?}"))),
        };
        self.batch_for(&receipts, error.as_deref())
    }

    /// Keep numeric viewport geometry in the same coordinate space as CSS env().
    pub fn viewport_geometry(&mut self, insets: [f32; 4], rects: Vec<[f32; 4]>) -> String {
        let [top, right, bottom, left] = insets;
        let fold = self.runner.viewport().fold;
        let (Ok(cols), Ok(rows)) = (u8::try_from(fold.cols), u8::try_from(fold.rows)) else {
            return self.batch_for(&[], Some("viewport geometry: grid exceeds 255"));
        };
        let before = self.runner.kernel().env();
        let env = before.with_insets(top, right, bottom, left).with_segments(
            cols,
            rows,
            rects
                .into_iter()
                .map(|[x, y, w, h]| exact_kernel::Rect::new(x, y, w, h))
                .collect(),
        );
        if !env.is_finite() || !env.segments_consistent() {
            return self.batch_for(
                &[],
                Some("viewport geometry: invalid insets or segment grid"),
            );
        }
        let kernel = self.runner.kernel_mut();
        if let Err(error) = kernel
            .set_env(env.clone())
            .and_then(|_| kernel.set_segments(cols, rows, env.segments))
        {
            let _ = kernel.set_env(before.clone());
            let _ = kernel.set_segments(before.cols, before.rows, before.segments);
            return self.batch_for(&[], Some(&format!("viewport geometry: {error:?}")));
        }
        let (receipts, error) = match self.runner.refresh_viewport_geometry() {
            Ok(Some(receipt)) => (
                vec![Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => (vec![], None),
            Err(e) => (vec![], Some(format!("viewport geometry: {e:?}"))),
        };
        self.batch_for(&receipts, error.as_deref())
    }

    /// Tell the runner the display's scale: a relayout in one batch where a
    /// row is a hairline, nothing where none is.
    pub fn set_display_scale(&mut self, scale: f64) -> String {
        let (receipts, error) = match self.runner.set_display_scale(scale) {
            Ok(Some(receipt)) => (
                vec![Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => (vec![], None),
            Err(e) => (vec![], Some(format!("display scale: {e:?}"))),
        };
        self.batch_for(&receipts, error.as_deref())
    }
}
