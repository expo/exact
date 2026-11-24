use super::*;

/// One painted frame and the image required by its consumer.
pub struct Frame<P = Arc<Pixmap>> {
    /// Premultiplied RGBA at device scale. A window or geometry paint may omit
    /// its image when its backend does not need to return CPU pixels.
    pub pixmap: P,
    /// Every node's box, in paint order (a later box is above an earlier).
    pub boxes: Vec<PaintedBox>,
}

impl<P> Frame<P> {
    pub(crate) fn map_pixels<Q>(self, map: impl FnOnce(P) -> Q) -> Frame<Q> {
        Frame {
            pixmap: map(self.pixmap),
            boxes: self.boxes,
        }
    }
}

impl Painter {
    pub(crate) fn replaced(&mut self) {
        self.native_session = native::new_session();
        // A new runner's frames have issued no command; the reader runs what a session's props carry.
        self.media.frames.clear();
        // The new kernel can reuse the old tree's epochs, view IDs and node keys.
        self.paint_epoch = None;
        self.discard_rows();
        self.shared = SharedState::default();
        self.svg_warned.clear();
        self.ripples.clear();
        self.zoom.clear();
        self.reader_highlights.clear();
    }

    /// Locate a target for scrolling before publishing its first frame.
    pub(crate) fn reveal_probe(
        &mut self,
        scene: &Scene<'_>,
        viewport: (f32, f32),
    ) -> Result<Frame<Option<Arc<Pixmap>>>, String> {
        let backend = std::mem::replace(&mut self.backend, Box::new(layer::Unpainted::<false>));
        let rows = std::mem::take(&mut self.rows);
        let frame = self.paint_selected_for(scene, viewport, None, None, FrameTarget::Geometry);
        self.rows = rows;
        self.backend = backend;
        frame
    }

    pub(super) fn paint_selected(
        &mut self,
        scene: &Scene<'_>,
        viewport: (f32, f32),
        skip: Option<exact_kernel::NodeKey>,
        replay: Option<&region::Replay<'_>>,
    ) -> Result<Frame, String> {
        self.paint_selected_for(scene, viewport, skip, replay, FrameTarget::Pixels)
            .map(|frame| frame.map_pixels(|pixels| pixels.expect("capture validated its pixels")))
    }

    pub(crate) fn paint_for(
        &mut self,
        scene: &Scene<'_>,
        viewport: (f32, f32),
        target: FrameTarget,
    ) -> Result<Frame<Option<Arc<Pixmap>>>, String> {
        if target == FrameTarget::Geometry && !self.backend.needs_geometry_paint() {
            let unpainted: Box<dyn Backend> = if self.backend.independent_section_rows() {
                Box::new(layer::Unpainted::<true>)
            } else {
                Box::new(layer::Unpainted::<false>)
            };
            let backend = std::mem::replace(&mut self.backend, unpainted);
            let frame = self.paint_selected_for(scene, viewport, None, None, target);
            self.backend = backend;
            return frame;
        }
        self.paint_selected_for(scene, viewport, None, None, target)
    }
}
