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
        images: &mut crate::image::Images,
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
            for source in self.runner.take_canvas_image_requests() {
                images.ask_canvas(source);
            }
            images.retain_canvas_reads(|source| {
                if self.runner.canvas_image_named(source) {
                    return true;
                }
                self.runner.canvas_image_released(source);
                false
            });
            let answers = std::mem::take(&mut images.canvas_answers);
            let redraw = !answers.is_empty();
            for (source, answer) in answers {
                if self.runner.canvas_image_named(&source) {
                    let result = answer.map(|lease| {
                        let size = lease.metadata().natural;
                        let pixels = crate::canvas2d::ImagePixels::Raster(lease);
                        self.canvas2d.images.insert(
                            source.clone(),
                            crate::canvas2d::CachedImage::from_pixels(pixels),
                        );
                        (size.width, size.height)
                    });
                    self.runner.canvas_image(&source, result, &[]);
                } else {
                    self.runner.canvas_image_released(&source);
                }
            }
            if redraw {
                self.runner.draw_canvases(&|_| true);
                // An image-dependent draw can name a second source.
                for source in self.runner.take_canvas_image_requests() {
                    images.ask_canvas(source);
                }
                images.start_canvas_reads();
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

    pub(crate) fn canvas_snapshots(&self) -> &crate::canvas2d::CanvasSnapshots {
        &self.canvas2d.snapshots
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
    ) -> Option<&std::sync::Arc<crate::canvas2d::ImagePixels>> {
        self.canvas2d.images.get(src).map(|image| &image.pixels)
    }

    /// `source` changed: forgotten, decoded or not, and decoded again
    /// when a mounted canvas's latest draw asked for it.
    pub(crate) fn canvas_picture_changed(
        &mut self,
        source: &str,
        images: &mut crate::image::Images,
    ) {
        images.cancel_canvas(source);
        self.canvas2d.images.remove(source);
        self.runner.canvas_image_released(source);
        if self.runner.canvas_image_named(source) {
            images.ask_canvas(source.to_owned());
        }
    }
}
