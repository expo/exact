//! A canvas child placed by its surface (LLP 1014 D5): painted apart and
//! warped into the frame.
use super::*;

impl Painter {
    pub(super) fn warp_extent(&self) -> crate::placement::RasterExtent {
        self.backend.raster_extent().unwrap_or_else(|| {
            let pixels = |points: f32| (points * self.scale).round() as u32;
            ((0., 0.), (pixels(self.viewport.0), pixels(self.viewport.1)))
        })
    }

    pub(super) fn placed(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: PaintOffset,
        clip: Option<Rect4>,
    ) -> bool {
        use crate::placement::{self, Placement};
        let Some(p) = self.placements.get(&id).copied() else {
            return false;
        };
        let Placement::Visible {
            h,
            canvas,
            clip_depth,
            opacity,
            ..
        } = p
        else {
            return true;
        };
        let Some(node) = walk.scene.kernel.node(id) else {
            return true;
        };
        let Some(parent) = walk.scene.kernel.node(canvas) else {
            return true;
        };
        let (hit_ts, hit_clip) = walk.hit_space.unwrap_or((ts, clip));
        let (x, y) = grid::paint_origin((parent.frame.x, parent.frame.y), offset);
        let hit_h = placement::compose(h, hit_ts, x, y);
        let h = placement::compose(h, ts, x, y);
        let Some(inv) = placement::inverse(h) else {
            return true;
        };
        let f = node.frame;
        if f.width <= 0. || f.height <= 0. {
            return true;
        }
        let mut painter = self.apart(walk.scene.kernel, f.width, f.height);
        painter.placements.remove(&id);
        let mut child_walk = Walk {
            scene: walk.scene,
            scrollbar_cover: walk.scrollbar_cover,
            scroll_grids: walk.scroll_grids.clone(),
            hit_space: None,
            boxes: Vec::new(),
            text: BTreeMap::new(),
            fields: HashMap::new(),
            inline: HashMap::new(),
            skip: None,
            replay: None,
            ranks: Rc::clone(&walk.ranks),
            reveal: walk.reveal,
            alert: walk.alert,
            dialog: walk.dialog,
        };
        painter.node(
            &mut child_walk,
            id,
            Transform::identity(),
            (f.x.into(), f.y.into()),
            None,
        );
        walk.text.extend(child_walk.text);
        walk.fields.extend(child_walk.fields);
        walk.inline.extend(child_walk.inline);
        self.gather(walk.scene.kernel, &mut painter);
        if let Ok(source) = painter.backend.finish() {
            if let Some((pixels, rect)) =
                placement::warp_clipped(&source, h, clip_depth, self.scale, self.warp_extent())
            {
                if opacity < 1. {
                    self.backend.push_opacity(opacity);
                }
                self.backend.surface_image(Arc::new(pixels), rect);
                if opacity < 1. {
                    self.backend.pop_opacity();
                }
            }
        }
        for mut b in child_walk.boxes {
            let mut hit = b.for_hit();
            hit.map_clips(placement::inverse(hit_h).unwrap_or(inv));
            b.map_clips(inv);
            hit.projective = Some((
                placement::inverse(hit_h).unwrap_or(inv),
                hit.rect,
                hit.clip,
                clip_depth,
            ));
            hit.rect = placement::clipped_bounds(&hit_h, clip_depth, hit.rect);
            hit.clip = hit_clip;
            b.hit = Some(hit.geometry());
            b.projective = Some((inv, b.rect, b.clip, clip_depth));
            b.rect = placement::clipped_bounds(&h, clip_depth, b.rect);
            b.clip = clip;
            walk.boxes.push(b);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    fn paint_placed(source: &str, viewport: (f32, f32), scale: f32, at: f32) -> Arc<Pixmap> {
        let (p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            viewport,
            scale,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert_eq!(error, None);
        let kernel = p.host().kernel();
        let id = |name| {
            kernel
                .node_by_key(kernel.find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        let mut painter = Painter::new(
            p.text().clone(),
            scale,
            Box::new(crate::raster::Raster::new()),
        );
        painter.placements.insert(
            id("ink"),
            crate::placement::Placement::Visible {
                h: [1., 0., at, 0., 1., 0., 0., 0., 1.],
                depth: 0.,
                opacity: 1.,
                clip_depth: [[0., 0., 1.]; 2],
                canvas: id("canvas"),
            },
        );
        let scene = Scene {
            kernel,
            roots: &kernel.roots(),
            hidden: &|_| false,
            presented: &|_| Presented::IDENTITY,
            scaled_presented: &Default::default(),
            paths: &|_| None,
            filters: &|_| None,
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
        painter.paint(&scene, viewport).unwrap().pixmap
    }

    #[test]
    fn placed_child_paints_the_last_column_of_a_software_backing() {
        let pixels = paint_placed(
            r#"component App
  view
    box width=4.7 height=4 overflow="hidden" translate="0px 0px 1px"
      canvas width=4.7 height=4 testId="canvas"
        box width=0.4 height=2 background-color="red" testId="ink"
"#,
            (20., 20.),
            2.75,
            4.4,
        );
        assert!(pixels.pixel(12, 2).unwrap().green() < 255);
        for x in (0..12).chain(13..pixels.width()) {
            assert_eq!(pixels.pixel(x, 2).unwrap().green(), 255, "column {x}");
        }
    }

    #[test]
    fn an_offset_filter_backing_keeps_projected_and_placed_children() {
        let pixels = paint_placed(
            r#"component App
  view
    box width=50 height=50 margin-left=100 margin-top=100 filter="brightness(1)"
      box width=20 height=20 translate="0px 0px 1px" background-color="red"
      canvas width=50 height=20 testId="canvas"
        box width=10 height=10 background-color="blue" testId="ink"
"#,
            (180., 180.),
            1.,
            25.,
        );
        let projected = pixels.pixel(105, 105).unwrap();
        let placed = pixels.pixel(130, 125).unwrap();
        assert_eq!(
            [projected.red(), projected.green(), projected.blue()],
            [255, 0, 0]
        );
        assert_eq!([placed.red(), placed.green(), placed.blue()], [0, 0, 255]);
    }

    #[test]
    fn placed_paragraph_keeps_inline_ownership_and_geometry() {
        let source = r#"component App
  view
    canvas testId="canvas" width=200 height=200
      text testId="paragraph" font-size=16 line-height="20px"
        text "Prefix "
        text "Tail" testId="tail"
"#;
        let (p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (200., 200.),
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
        let paragraph = id("paragraph");
        let tail = id("tail");
        let mut painter =
            Painter::new(p.text().clone(), 1., Box::new(crate::raster::Raster::new()));
        painter.placements.insert(
            paragraph,
            crate::placement::Placement::Visible {
                h: [1., 0., 10., 0., 0.8, 20., 0., 0., 1.],
                depth: 0.,
                opacity: 1.,
                clip_depth: [[0., 0., 1.]; 2],
                canvas: id("canvas"),
            },
        );
        let scene = Scene {
            kernel,
            roots: &kernel.roots(),
            hidden: &|_| false,
            presented: &|_| Presented::IDENTITY,
            scaled_presented: &Default::default(),
            paths: &|_| None,
            filters: &|_| None,
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
        let frame = painter.paint(&scene, (200., 200.)).unwrap();
        let painted = frame.boxes.iter().find(|b| b.id == paragraph).unwrap();
        assert!(painted.projected());
        let inline = painter
            .inline_text
            .get(&paragraph)
            .expect("placed inline capture");
        assert!(inline.views().any(|view| view == tail));
        let local = inline.frame(tail).unwrap();
        let rect = painted.clone().with_scroll_rect(local).unwrap().rect;
        assert!((rect.0 - local.0 - 10.).abs() < 0.01, "{local:?}: {rect:?}");
        assert!(
            (rect.1 - local.1 * 0.8 - 20.).abs() < 0.01,
            "{local:?}: {rect:?}"
        );
    }
}
