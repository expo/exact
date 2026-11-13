//! A canvas child placed by its surface (LLP 1014 D5): painted apart and
//! warped into the frame.
use super::*;

impl Painter {
    pub(super) fn placed(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
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
        let h = placement::compose(h, ts, parent.frame.x - offset.0, parent.frame.y - offset.1);
        let Some(inv) = placement::inverse(h) else {
            return true;
        };
        let f = node.frame;
        if f.width <= 0. || f.height <= 0. {
            return true;
        }
        let mut painter = self.apart(f.width, f.height);
        painter.placements.remove(&id);
        let mut child_walk = Walk {
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
        painter.node(&mut child_walk, id, Transform::identity(), (f.x, f.y), None);
        walk.text.extend(child_walk.text);
        walk.fields.extend(child_walk.fields);
        walk.inline.extend(child_walk.inline);
        self.gather(&mut painter);
        if let Ok(source) = painter.backend.finish() {
            if let Some((pixels, rect)) =
                placement::warp_clipped(&source, h, clip_depth, self.scale, self.viewport)
            {
                self.backend.surface_image(Arc::new(pixels), rect);
            }
        }
        for mut b in child_walk.boxes {
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

    #[test]
    fn placed_paragraph_keeps_inline_ownership_and_geometry() {
        let source = r#"component App
  view
    canvas testId="canvas" width=200 height=200
      text testId="paragraph" font-size=16 line-height="20px"
        text "Prefix "
        text "Tail" testId="tail"
"#;
        let (p, error) = Presenter::boot_with(
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
                clip_depth: [[0., 0., 1.]; 2],
                canvas: id("canvas"),
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
        let frame = painter.paint(&scene, (200., 200.)).unwrap();
        let painted = frame.boxes.iter().find(|b| b.id == paragraph).unwrap();
        assert!(painted.projected());
        let inline = painter
            .inline_text
            .get(&paragraph)
            .expect("placed inline capture");
        assert!(inline.views().any(|view| view == tail));
        let local = inline.frame(tail).unwrap();
        let rect = painted.with_scroll_rect(local).unwrap().rect;
        assert!((rect.0 - local.0 - 10.).abs() < 0.01, "{local:?}: {rect:?}");
        assert!(
            (rect.1 - local.1 * 0.8 - 20.).abs() < 0.01,
            "{local:?}: {rect:?}"
        );
    }
}
