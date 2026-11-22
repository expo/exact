use super::*;

impl<D: DataSource> Host<D> {
    /// Canvas 2D after this turn's layout (LLP 1056 D4): the kernel's
    /// content boxes at `scale`, a presented frame for canvases that asked
    /// (`frame`), the due draws, and their lists replayed. Whether any
    /// bitmap changed.
    pub(crate) fn sync_canvases(
        &mut self,
        scale: f64,
        frame: bool,
        assets: &crate::image::Assets,
    ) -> bool {
        if !self.runner.plan().surfaces.is_empty() {
            if self.canvas2d.text.is_none() {
                // Text measured and drawn by one engine (LLP 1056 D8).
                let text = std::sync::Arc::new(crate::canvas2d::text::CanvasText::new(
                    self.runner.plan().clone(),
                    assets.clone(),
                ));
                self.canvas2d.text = Some(text.clone());
                self.runner.set_canvas_text(text);
            }
            self.runner.layout_canvases(scale);
            if frame {
                self.runner.canvas_frame();
            }
            self.runner.draw_canvases(&|_| true);
            // The images the draws asked for, decoded now; each redraws the
            // canvases that asked (LLP 1056 D9).
            let asked = self.runner.take_canvas_image_requests();
            for src in &asked {
                self.load_canvas_picture(src, assets);
            }
            if !asked.is_empty() {
                self.runner.draw_canvases(&|_| true);
            }
        }
        let lists = self.runner.take_canvas_lists();
        let changed = !lists.is_empty();
        for e in self.canvas2d.apply(lists) {
            self.runner.log(e);
        }
        self.prune_canvas_pixels();
        changed
    }

    pub(super) fn prune_canvas_pixels(&mut self) {
        let live = self.runner.canvas_views();
        self.canvas2d.retain(&live);
        // An image no mounted canvas's latest draw asked for is let go.
        let runner = &mut self.runner;
        self.canvas2d.images.retain(|src, _| {
            let named = runner.canvas_image_named(src);
            if !named {
                runner.canvas_image_released(src);
            }
            named
        });
    }

    /// A canvas capture (`exact_runner::canvas_capture`), carried out on
    /// the bitmaps as [`Host::sync_canvases`] left them.
    pub(crate) fn capture_canvas(&mut self, r: &RequestOut) -> exact_runner::Outcome {
        let found = r
            .request
            .take_canvas_capture()
            .expect("a canvas capture request");
        let (roots, grants) = (self.app_roots(), self.grants());
        let scope = r.request.grants.as_deref();
        crate::canvas2d::capture(&self.canvas2d, found, roots, &grants, scope)
    }

    /// Each 2D canvas's latest bitmap, for the painters.
    pub(crate) fn canvas_snapshots(&self) -> BTreeMap<ViewId, crate::canvas2d::CanvasPaint> {
        self.canvas2d.snapshots()
    }

    /// Whether a 2D canvas asked for another frame (LLP 1056 D5).
    pub fn canvas_wants_frame(&self) -> bool {
        self.runner.canvas_wants_frame()
    }

    /// The decoded pixels the canvases share.
    #[cfg(test)]
    pub(crate) fn cached_canvas_image(
        &self,
        src: &str,
    ) -> Option<&std::sync::Arc<tiny_skia::Pixmap>> {
        self.canvas2d.images.get(src).map(|image| &image.pixels)
    }

    /// `source` changed: forgotten, decoded or not, and decoded again
    /// when a mounted canvas's latest draw asked for it.
    pub(crate) fn canvas_picture_changed(&mut self, source: &str, assets: &crate::image::Assets) {
        self.canvas2d.images.remove(source);
        self.runner.canvas_image_released(source);
        if self.runner.canvas_image_named(source) {
            self.load_canvas_picture(source, assets);
        }
    }

    fn load_canvas_picture(&mut self, source: &str, assets: &crate::image::Assets) {
        let result = assets
            .read(source)
            .ok_or_else(|| format!("{source} is not an asset"))
            .and_then(|bytes| {
                tiny_skia::Pixmap::decode_png(&bytes)
                    .map_err(|e| format!("{source} does not decode: {e}"))
            })
            .map(|pixels| {
                let size = (pixels.width(), pixels.height());
                self.canvas2d
                    .images
                    .insert(source.to_owned(), crate::canvas2d::CachedImage::new(pixels));
                size
            });
        self.runner.canvas_image(source, result, &[]);
    }
}
