//! A `Composite` canvas's children (LLP 1014 D2, D3): laid out by the kernel
//! in the canvas's box, painted apart at the painter's scale into one
//! picture, which the host uploads for the surface to sample
//! (`gpu_texture`, `Surface::children`), and not painted over the canvas.
//! They keep their hit boxes, at their layout frames.
use super::*;
use std::collections::BTreeSet;

/// The canvases whose surfaces composite their children, and the pictures
/// of their children this paint made.
#[derive(Default)]
pub(crate) struct Composites {
    pub(crate) canvases: BTreeSet<ViewId>,
    pub(crate) pictures: Vec<(ViewId, Arc<Pixmap>)>,
}

impl Painter {
    /// A painter for a subtree painted apart, `width` × `height` points at
    /// this one's scale, onto a transparent raster: what this paint knows of
    /// the page's canvases (their bitmaps, the hatches' overlays, where
    /// surfaces place and composite their children) goes with it.
    pub(super) fn apart(&self, width: f32, height: f32) -> Painter {
        let mut painter = Painter::new(
            self.text.clone(),
            self.scale,
            Box::new(crate::raster::Raster::transparent()),
        );
        painter.dark = self.dark;
        painter.symbols = self.symbols;
        painter.placements = self.placements.clone();
        painter.canvases = self.canvases.clone();
        painter.overlays = self.overlays.clone();
        painter.composites.canvases = self.composites.canvases.clone();
        painter.viewport = (width, height);
        painter.backend.begin(width, height, self.scale);
        painter
    }

    /// What a painter made [`Painter::apart`] leaves this one: the pictures
    /// of the composite canvases inside its subtree.
    pub(super) fn gather(&mut self, apart: &mut Painter) {
        self.composites
            .pictures
            .append(&mut apart.composites.pictures);
    }

    /// Paint `canvas`'s `children`, in paint order, apart: its surface
    /// composites them. `offset` and `clip` are what they would otherwise be
    /// painted with.
    pub(super) fn composite(
        &mut self,
        walk: &mut Walk<'_, '_>,
        canvas: &NodeRef<'_>,
        children: &[ViewId],
        ts: Transform,
        offset: (f32, f32),
        clip: Option<Rect4>,
    ) {
        use crate::placement;
        self.row_refuse();
        let f = canvas.frame;
        if f.width <= 0. || f.height <= 0. {
            return;
        }
        let mut painter = self.apart(f.width, f.height);
        let mut apart = Walk {
            scene: walk.scene,
            boxes: Vec::new(),
            text: BTreeMap::new(),
            fields: HashMap::new(),
            inline: HashMap::new(),
            skip: None,
            replay: None,
            ranks: Rc::clone(&walk.ranks),
            reveal: walk.reveal,
            dialog: walk.dialog,
        };
        // Kernel frames are the page's: the canvas's own origin is the picture's.
        for child in children {
            painter.node(&mut apart, *child, Transform::identity(), (f.x, f.y), None);
        }
        walk.text.extend(apart.text);
        walk.fields.extend(apart.fields);
        walk.inline.extend(apart.inline);
        self.gather(&mut painter);
        if let Ok(pixels) = painter.backend.finish() {
            self.composites.pictures.push((canvas.id, Arc::new(pixels)));
        }
        // Hit boxes map from the picture to the page as the canvas does.
        let identity = [1., 0., 0., 0., 1., 0., 0., 0., 1.];
        let h = placement::compose(identity, ts, f.x - offset.0, f.y - offset.1);
        let Some(inverse) = placement::inverse(h) else {
            return;
        };
        let planes = [[0., 0., 1.]; 2];
        for mut b in apart.boxes {
            b.projective = Some((inverse, b.rect, b.clip, planes));
            b.rect = placement::clipped_bounds(&h, planes, b.rect);
            b.clip = clip;
            walk.boxes.push(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    #[test]
    fn a_composite_picture_keeps_paint_order_and_the_canvases_inside_it() {
        // The red box comes first in the tree and paints over the blue one by
        // its z-index; a 2D canvas inside paints its bitmap.
        let source = r##"component App
  view
    canvas testId="glass" width=100 height=100 position="relative"
      view position="absolute" left=0 top=0 width=50 height=50 z-index=2 background-color="#FF0000"
      view position="absolute" left=0 top=0 width=50 height=50 background-color="#0000FF"
      canvas testId="inner" position="absolute" left=60 top=60 width=20 height=20
"##;
        let (p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (100., 100.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let kernel = p.host().kernel();
        let id = |name| {
            kernel
                .node_by_key(kernel.find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        let (glass, inner) = (id("glass"), id("inner"));
        let mut painter =
            Painter::new(p.text().clone(), 1., Box::new(crate::raster::Raster::new()));
        painter.composites.canvases.insert(glass);
        let mut green = Pixmap::new(20, 20).unwrap();
        green.fill(tiny_skia::Color::from_rgba8(0, 255, 0, 255));
        painter.canvases.insert(
            inner,
            crate::canvas2d::CanvasPaint {
                pixels: Arc::new(green),
            },
        );
        let scene = Scene {
            kernel,
            roots: &kernel.roots(),
            hidden: &|_| false,
            presented: &|_| Presented::IDENTITY,
            paths: &|_| None,
            scroll: &BTreeMap::new(),
            page: (0., 0.),
            images: &BTreeMap::new(),
            focus: None,
            selection: None,
            caret_upstream: false,
            pointer: None,
            controls: &BTreeMap::new(),
            chosen: &BTreeMap::new(),
            menu: None,
        };
        painter.paint(&scene, (100., 100.)).unwrap();
        let (canvas, picture) = &painter.composites.pictures[0];
        assert_eq!(*canvas, glass);
        let at = |x, y| picture.pixel(x, y).unwrap();
        assert_eq!(
            (at(25, 25).red(), at(25, 25).blue()),
            (255, 0),
            "z-index orders the picture"
        );
        assert_eq!(
            at(70, 70).green(),
            255,
            "the 2D canvas's bitmap is in the picture"
        );
    }
}
