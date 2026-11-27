//! Hits inside an `svg` (LLP 1055.000 D17): the element under a point, by
//! the kernel's resolved scene and `pointer-events`.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// Whether `node` is an element drawn inside the `svg` `drawing`: the
    /// agent's tap on an `svg` presses the element at its centre, as a
    /// finger there would.
    pub(super) fn drawn_in(&self, node: ViewId, drawing: ViewId) -> bool {
        let kernel = self.host.kernel();
        if kernel
            .node(drawing)
            .is_none_or(|n| n.node_type != NodeType::Svg)
        {
            return false;
        }
        let ancestor = drawing;
        let mut cur = kernel.node(node).and_then(|n| n.parent);
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = kernel.node(p).and_then(|n| n.parent);
        }
        false
    }

    /// What a point in box `b` hits: inside an `svg`, the element under it
    /// by `pointer-events`; else the box itself.
    pub(super) fn svg_hit(&self, b: &crate::paint::PaintedBox, x: f32, y: f32) -> Option<ViewId> {
        let kernel = self.host.kernel();
        let box_hit = || b.pointer_hit.then_some(b.id);
        let Some(node) = kernel.node(b.id).filter(|n| n.node_type == NodeType::Svg) else {
            return box_hit();
        };
        let content = exact_kernel::svg::scene::content_box(kernel, &node);
        let scene = crate::paint::resolve_with(
            kernel,
            &node,
            content,
            &|id| self.host.presented(id),
            &|id| self.host.presented_path(id),
            // Filters take no part in hits (LLP 1055.000 D14).
            &|_| None,
            self.dark(),
        );
        let (lx, ly) = b.local_point(x, y)?;
        scene
            .hit((lx - content.0, ly - content.1))
            // A root-none can still contain an explicitly auto child. The
            // resolved scene also owns use-instance styles; do not refilter a
            // shape's hit using the referenced node's uninstanced style.
            .or_else(box_hit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot(source: &str) -> Presenter<()> {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (400., 400.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        p
    }

    fn id(p: &Presenter<()>, test_id: &str) -> ViewId {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
    }

    #[test]
    fn transformed_svg_hits_shapes_at_their_painted_positions() {
        // 100x100 svg at (100, 100) scaled 2x around its center (150, 150) paints at [50..250, 50..250]:
        // left rect covers [50..150], right rect covers [150..250].
        let mut p = boot(
            r##"component App
  view
    box position="relative" width=400 height=400 testId="bg"
      svg position="absolute" left=100 top=100 width=100 height=100 scale=2 viewBox="0 0 100 100" pointer-events="none" testId="svg"
        rect x=0 y=0 width=50 height=100 fill="#f00" pointer-events="auto" testId="left"
        rect x=50 y=0 width=50 height=100 fill="#00f" pointer-events="auto" testId="right"
"##,
        );
        let (left, right) = (id(&p, "left"), id(&p, "right"));
        assert_eq!(p.hit(125., 100.), Some(left));
        assert_eq!(p.hit(200., 100.), Some(right));

        // 90deg clockwise rotation around (150, 150): left half [0..50] moves to top [100..150],
        // right half [50..100] moves to bottom [150..200].
        let mut p = boot(
            r##"component App
  view
    box position="relative" width=400 height=400
      svg position="absolute" left=100 top=100 width=100 height=100 rotate=90 viewBox="0 0 100 100"
        rect x=0 y=0 width=50 height=100 fill="#f00" testId="left"
        rect x=50 y=0 width=50 height=100 fill="#00f" testId="right"
"##,
        );
        assert_eq!(p.hit(150., 125.), Some(id(&p, "left")));
        assert_eq!(p.hit(150., 175.), Some(id(&p, "right")));

        // Padding inset under scale=2 plus transformed ancestor and pointer-events:none fallback.
        // Inner 100x100 svg with padding=10 has 80x80 content box at local [10..90].
        // Under scale=2 on the ancestor centered at (150, 150), local [10..90] maps to [70..230]:
        // left rect covers [70..150], right rect covers [150..230], and [50..70] is padding (falls through to bg).
        let mut p = boot(
            r##"component App
  view
    box position="relative" width=400 height=400 testId="bg"
      box position="absolute" left=100 top=100 width=100 height=100 scale=2 pointer-events="none"
        svg width=100 height=100 padding=10 viewBox="0 0 80 80" pointer-events="none"
          rect x=0 y=0 width=40 height=80 fill="#f00" pointer-events="auto" testId="left"
          rect x=40 y=0 width=40 height=80 fill="#00f" pointer-events="auto" testId="right"
"##,
        );
        assert_eq!(p.hit(60., 150.), Some(id(&p, "bg")));
        assert_eq!(p.hit(100., 150.), Some(id(&p, "left")));
        assert_eq!(p.hit(200., 150.), Some(id(&p, "right")));

        // Projective 3D transform (rotate="y 45deg" under perspective): left stays left of center, right stays right.
        let mut p = boot(
            r##"component App
  view
    box position="relative" width=400 height=400 perspective=400
      svg position="absolute" left=100 top=100 width=100 height=100 rotate="y 45deg" viewBox="0 0 100 100"
        rect x=0 y=0 width=50 height=100 fill="#f00" testId="left"
        rect x=50 y=0 width=50 height=100 fill="#00f" testId="right"
"##,
        );
        assert_eq!(p.hit(130., 150.), Some(id(&p, "left")));
        assert_eq!(p.hit(170., 150.), Some(id(&p, "right")));
    }
}
