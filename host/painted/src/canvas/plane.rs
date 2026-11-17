//! Projective drawing delegated to Android's Canvas, with clips in the
//! coordinate system where they were pushed. Page-space culling and retained
//! animation layers cannot describe geometry inside a projective plane.
use super::*;
use crate::paint::space::mul3;

impl Recorder {
    pub(super) fn plane_open(&mut self, matrix: [f32; 9]) -> bool {
        let combined = self
            .planes
            .last()
            .map_or(matrix, |outer| mul3(outer, &matrix));
        if !combined.iter().all(|v| v.is_finite()) {
            return false;
        }
        // Pending outer clips must be emitted before the coordinate system changes.
        if !self.clips.is_empty() {
            self.materialize(self.clips.len() - 1);
        }
        self.planes.push(combined);
        true
    }

    pub(super) fn plane_matrix(&mut self, ts: Transform) -> bool {
        let Some(plane) = self.planes.last() else {
            return false;
        };
        let local = [ts.sx, ts.kx, ts.tx, ts.ky, ts.sy, ts.ty, 0., 0., 1.];
        let m = mul3(plane, &local);
        let (x, y) = self.origin;
        let s = self.scale;
        self.ops.push(MATRIX3);
        // Scale and the recording's origin act after perspective division.
        for value in [
            s * (m[0] - x * m[6]),
            s * (m[1] - x * m[7]),
            s * (m[2] - x * m[8]),
            s * (m[3] - y * m[6]),
            s * (m[4] - y * m[7]),
            s * (m[5] - y * m[8]),
            m[6],
            m[7],
            m[8],
        ] {
            self.f(value);
        }
        self.matrix = None;
        true
    }

    pub(super) fn on_page(&self, rect: Rect4, ts: Transform) -> Option<Rect4> {
        if self.planes.is_empty() {
            clip::map(rect, ts)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    fn record(app: &str, size: (f32, f32)) -> Vec<u32> {
        record_at_scale(app, size, 1.)
    }

    fn record_at_scale(app: &str, size: (f32, f32), scale: f32) -> Vec<u32> {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            size,
            scale,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        FINISHED.with(|f| f.borrow_mut().take()).unwrap()
    }

    #[test]
    fn svg_blend_area_limit_falls_back_before_allocating() {
        let ops = record_at_scale(
            r##"component App
  view
    svg width=200 height=200 overflow="visible"
      rect width=4000 height=4000 fill="#34f1ec"
      rect width=4000 height=4000 fill="#fd2f7c" mix-blend-mode="multiply"
"##,
            (200., 200.),
            3.,
        );
        let mut at = 0;
        let mut colors = Vec::new();
        while at < ops.len() {
            assert_ne!(
                ops[at], IMAGE_DEF,
                "the 144-million-pixel island is refused"
            );
            if ops[at] == PATH {
                colors.push(ops[at + 1]);
            }
            at = stream::end(&ops, at).unwrap();
        }
        assert_eq!(colors, [0xff34f1ec, 0xfffd2f7c]);
    }

    #[test]
    fn svg_blend_keeps_overflowing_diagonal_square_caps() {
        let ops = record(
            r##"component App
  view
    svg width=40 height=40 overflow="visible"
      path d="M10 10 L30 30" fill="none" stroke="#fd2f7c" stroke-width=20 stroke-linecap="square" stroke-miterlimit=1 mix-blend-mode="multiply"
"##,
            (80., 80.),
        );
        let mut at = 0;
        let mut picture = None;
        let mut destination = None;
        while at < ops.len() {
            if ops[at] == IMAGE_DEF {
                picture = with_pending(|p| p.remove(&ops[at + 1]));
            }
            if matches!(ops[at], IMAGE | IMAGE_RRECT) {
                destination = Some(
                    ops[at + 2..at + 6]
                        .iter()
                        .map(|v| f32::from_bits(*v))
                        .collect::<Vec<_>>(),
                );
            }
            at = stream::end(&ops, at).unwrap();
        }
        let p = picture.unwrap();
        let d = destination.unwrap();
        for (x, y) in [(-2., 10.), (10., -2.), (42., 30.), (30., 42.)] {
            assert!(
                x >= d[0] && y >= d[1] && x < d[0] + d[2] && y < d[1] + d[3],
                "cap outside bitmap: {d:?}"
            );
            let i = (((y - d[1]) as usize) * p.width() as usize + (x - d[0]) as usize) * 4;
            assert_eq!(&p.as_ref()[i..i + 4], &[253, 47, 124, 255]);
        }
    }

    #[test]
    fn a_retained_blended_use_repaints_when_its_external_source_animates() {
        let app = r##"keyframes fade
  from opacity=1
  to opacity=0.2
component App
  view
    scroll width=100 height=200
      column
        svg width=100 height=100 flex-shrink=0
          rect id="ink" width=100 height=100 fill="#fd2f7c" animation="fade 1s linear infinite"
        svg width=100 height=100 flex-shrink=0
          rect width=100 height=100 fill="#34f1ec"
          use href="#ink" mix-blend-mode="multiply"
"##;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (100., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        for (time, expected) in [
            (0., [52, 44, 115, 255]),
            (250., [52, 84, 139, 255]),
            (500., [52, 123, 163, 255]),
        ] {
            p.tick(time);
            p.frame();
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let mut at = 0;
            let mut found = false;
            while at < ops.len() {
                if ops[at] == ROW_BEGIN {
                    at += 6;
                    continue;
                }
                if ops[at] == IMAGE_DEF {
                    let image = with_pending(|p| p.remove(&ops[at + 1])).unwrap();
                    let i = (50 * image.width() as usize + 50) * 4;
                    for (actual, expected) in image.as_ref()[i..i + 4].iter().zip(expected) {
                        assert!(
                            actual.abs_diff(expected) <= 2,
                            "{time}: {:?}",
                            &image.as_ref()[i..i + 4]
                        );
                    }
                    found = true;
                }
                at = stream::end(&ops, at).unwrap();
            }
            assert!(found, "consumer's retained row needs new pixels at {time}");
        }
    }

    #[test]
    fn svg_multiply_overprint_records_its_backdrop_in_one_picture() {
        let ops = record(
            r##"component App
  view
    svg width=200 height=120 viewBox="0 0 200 120"
      rect width=200 height=120 fill="#f6f2e9"
      rect x=20 y=40 width=160 height=16 fill="#34f1ec"
      path d="M60 0 V120" fill="none" stroke="#fd2f7c" stroke-width=16 mix-blend-mode="multiply"
"##,
            (200., 120.),
        );
        let mut at = 0;
        let mut pictures = Vec::new();
        while at < ops.len() {
            if ops[at] == IMAGE_DEF {
                pictures.push(with_pending(|p| p.remove(&ops[at + 1])).unwrap());
            }
            at = stream::end(&ops, at).unwrap();
        }
        assert_eq!(pictures.len(), 1);
        let picture = &pictures[0];
        let at = (48 * picture.width() as usize + 60) * 4;
        assert_eq!(&picture.as_ref()[at..at + 4], &[52, 44, 115, 255]);
        let web = Pixmap::decode_png(include_bytes!(
            "../../../apple/tests/ExactKitTests/ColorFixtures/svg-overprint.png"
        ))
        .unwrap();
        assert_eq!(
            (picture.width(), picture.height()),
            (web.width(), web.height())
        );
        let worst = picture
            .as_ref()
            .iter()
            .zip(web.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 1, "Chrome SVG pixel difference {worst}");
    }

    #[test]
    fn svg_overprint_with_text_and_clipped_large_paths_records_one_picture() {
        for artwork in [
            r##"rect x=20 y=40 width=160 height=16 fill="#34f1ec"
      path d="M60 0 V120" fill="none" stroke="#fd2f7c" stroke-width=16 mix-blend-mode="multiply"
      text x=8 y=20 fill="#111111" "ink""##,
            r##"defs
        clipPath id="strip"
          rect x=20 y=40 width=160 height=16
      g transform="translate(0 0)" clip-path="url(#strip)"
        rect width=20000 height=120 fill="#34f1ec"
        rect width=20000 height=120 fill="#fd2f7c" mix-blend-mode="multiply""##,
            r##"defs
        clipPath id="strip"
          rect x=20 y=40 width=160 height=16
      rect width=20000 height=120 fill="#34f1ec" clip-path="url(#strip)"
      rect width=20000 height=120 fill="#fd2f7c" mix-blend-mode="multiply" clip-path="url(#strip)""##,
        ] {
            let ops = record(
                &format!(
                    r##"component App
  view
    svg width=200 height=120 overflow="visible" viewBox="0 0 200 120"
      rect width=200 height=120 fill="#f6f2e9"
      {artwork}
"##
                ),
                (200., 120.),
            );
            let mut at = 0;
            let mut pictures = Vec::new();
            let mut destination = None;
            while at < ops.len() {
                if ops[at] == IMAGE_DEF {
                    pictures.push(with_pending(|p| p.remove(&ops[at + 1])).unwrap());
                }
                if matches!(ops[at], IMAGE | IMAGE_RRECT) {
                    destination = Some(
                        ops[at + 2..at + 6]
                            .iter()
                            .map(|v| f32::from_bits(*v))
                            .collect::<Vec<_>>(),
                    );
                }
                at = stream::end(&ops, at).unwrap();
            }
            assert_eq!(pictures.len(), 1, "{artwork}");
            let picture = &pictures[0];
            let destination = destination.unwrap();
            let x = (60. - destination[0]) as usize;
            let y = (48. - destination[1]) as usize;
            let pixel = (y * picture.width() as usize + x) * 4;
            assert_eq!(&picture.as_ref()[pixel..pixel + 4], &[52, 44, 115, 255]);
            if artwork.contains("text") {
                assert!(
                    picture
                        .as_ref()
                        .as_chunks::<4>()
                        .0
                        .contains(&[17, 17, 17, 255]),
                    "the label is painted too"
                );
            }
        }
    }

    #[test]
    fn svg_overprint_below_the_window_survives_a_retained_row_scroll() {
        let app = r##"component App
  view
    scroll testId="scroll" width=200 height=120
      column height=1000 flex-shrink=0
        box height=700 flex-shrink=0
        svg width=200 height=120 flex-shrink=0
          rect width=200 height=120 fill="#34f1ec"
          rect x=40 width=40 height=120 fill="#fd2f7c" mix-blend-mode="multiply"
"##;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (200., 120.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let first = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut row = None;
        let mut ink_row = None;
        while at < first.len() {
            match first[at] {
                ROW_BEGIN => {
                    row = Some(first[at + 1]);
                    at += 6;
                    continue;
                }
                ROW_END => row = None,
                IMAGE_DEF => {
                    let picture = with_pending(|p| p.remove(&first[at + 1])).unwrap();
                    assert!(
                        picture
                            .as_ref()
                            .as_chunks::<4>()
                            .0
                            .contains(&[52, 44, 115, 255]),
                        "the retained picture contains the crossing 700 points below the window"
                    );
                    ink_row = row;
                }
                _ => (),
            }
            at = stream::end(&first, at).unwrap();
        }
        let ink_row = ink_row.expect("the SVG was recorded inside a retained row");
        let key = p.host().kernel().find_by_test_id("scroll")[0];
        let scroll = p.host().kernel().node_by_key(key).unwrap().id;
        assert_eq!(p.scroll_to(scroll, (0., 700.)), Some((0., 700.)));
        p.frame();
        let second = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut reused = false;
        while at < second.len() {
            if second[at] == ROW_DRAW {
                reused |= second[at + 1] == ink_row;
            }
            assert_ne!(
                second[at], IMAGE_DEF,
                "scrolling replays the retained pixels"
            );
            at = stream::end(&second, at).unwrap();
        }
        assert!(reused, "scrolling reveals the already recorded SVG row");
    }

    #[test]
    fn svg_blend_keeps_a_wide_masked_backdrop() {
        let ops = record(
            r##"component App
  view
    svg width=200 height=100 overflow="visible"
      defs
        mask id="wide" maskUnits="userSpaceOnUse" x=0 y=0 width=5000 height=100
          rect width=5000 height=100 fill="white"
      rect width=5000 height=100 fill="#34f1ec" mask="url(#wide)"
      rect x=40 width=40 height=100 fill="#fd2f7c" mix-blend-mode="multiply"
"##,
            (200., 100.),
        );
        let mut at = 0;
        let mut pictures = Vec::new();
        while at < ops.len() {
            if ops[at] == IMAGE_DEF {
                pictures.push(with_pending(|p| p.remove(&ops[at + 1])).unwrap());
            }
            at = stream::end(&ops, at).unwrap();
        }
        assert_eq!(pictures.len(), 1);
        let picture = &pictures[0];
        assert_eq!((picture.width(), picture.height()), (5000, 100));
        for (x, expected) in [
            (20, [52, 241, 236, 255]),
            (60, [52, 44, 115, 255]),
            (4990, [52, 241, 236, 255]),
        ] {
            let pixel = (50 * picture.width() as usize + x) * 4;
            assert_eq!(&picture.as_ref()[pixel..pixel + 4], &expected);
        }
    }

    #[test]
    fn svg_blend_allocation_failure_keeps_every_shape() {
        let ops = record(
            r##"component App
  view
    svg width=20000 height=100
      rect width=20000 height=80 fill="#34f1ec"
      rect width=20000 height=40 fill="#fd2f7c" mix-blend-mode="multiply"
"##,
            (crate::raster::MAX_FRAME_SIDE as f32 + 1., 100.),
        );
        let mut colors = Vec::new();
        let mut at = 0;
        while at < ops.len() {
            if ops[at] == PATH {
                colors.push(ops[at + 1]);
            }
            assert_ne!(ops[at], IMAGE_DEF);
            at = stream::end(&ops, at).unwrap();
        }
        assert_eq!(
            colors,
            [0xff34f1ec, 0xfffd2f7c],
            "both shapes draw unblended"
        );
    }

    #[test]
    fn svg_overflow_blend_keeps_transformed_paint_outside_the_content_box() {
        let ops = record(
            r##"component App
  view
    svg width=40 height=40 overflow="visible" viewBox="0 0 20 20"
      g transform="translate(0 100)"
        rect width=20 height=20 fill="#34f1ec"
        rect width=10 height=20 fill="#fd2f7c" mix-blend-mode="multiply"
"##,
            (40., 300.),
        );
        let mut at = 0;
        let mut found = false;
        let mut destination = None;
        while at < ops.len() {
            if ops[at] == IMAGE_DEF {
                let picture = with_pending(|p| p.remove(&ops[at + 1])).unwrap();
                assert_eq!((picture.width(), picture.height()), (40, 40));
                assert!(picture
                    .as_ref()
                    .as_chunks::<4>()
                    .0
                    .contains(&[52, 44, 115, 255]));
                found = true;
            }
            if matches!(ops[at], IMAGE | IMAGE_RRECT) {
                destination = Some(
                    ops[at + 2..at + 6]
                        .iter()
                        .map(|v| f32::from_bits(*v))
                        .collect::<Vec<_>>(),
                );
            }
            at = stream::end(&ops, at).unwrap();
        }
        assert!(found);
        assert_eq!(destination, Some(vec![0., 200., 40., 40.]));
    }

    #[test]
    fn svg_marker_blends_with_the_scene_backdrop() {
        let ops = record(
            r##"component App
  view
    svg width=80 height=80 viewBox="0 0 80 80"
      defs
        marker id="ink" markerUnits="userSpaceOnUse" markerWidth=20 markerHeight=20 refX=10 refY=10
          rect width=20 height=20 fill="#fd2f7c" mix-blend-mode="multiply"
      rect width=80 height=80 fill="#34f1ec"
      path d="M20 40 H40" fill="none" stroke="none" marker-end="url(#ink)"
"##,
            (80., 80.),
        );
        let mut at = 0;
        let mut pictures = Vec::new();
        while at < ops.len() {
            if ops[at] == IMAGE_DEF {
                pictures.push(with_pending(|p| p.remove(&ops[at + 1])).unwrap());
            }
            at = stream::end(&ops, at).unwrap();
        }
        assert_eq!(
            pictures.len(),
            1,
            "the marker and backdrop share one picture"
        );
        let p = &pictures[0];
        let at = (40 * p.width() as usize + 40) * 4;
        assert_eq!(&p.as_ref()[at..at + 4], &[52, 44, 115, 255]);
    }

    #[test]
    fn svg_overprint_survives_a_departing_route_recording() {
        let app = r##"routes nav
  home "/"
    ink "/ink"
keyframes leave
  to translate="100% 0"
style Page
  ::view-transition-old
    animation="leave 400ms linear both"
component App
  action open
    nav = push(nav, "/ink")
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width=200 height=120
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` position="absolute" inset=0
            button testId="open" press=open
              text "Open"
        if e.name == "ink"
          column class=Page navigationKey=`${e.id}` testId="ink" position="absolute" inset=0
            svg width=200 height=120
              rect width=200 height=120 fill="#34f1ec"
              rect x=40 width=40 height=120 fill="#fd2f7c" mix-blend-mode="multiply"
"##;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (200., 120.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let button = p.host().kernel().find_by_test_id("open")[0];
        let button = p.host().kernel().node_by_key(button).unwrap().id;
        p.tap(button).unwrap();
        p.run_commands(|| ());
        p.tick(1000.);
        p.frame();
        FINISHED.with(|f| f.borrow_mut().take());
        assert!(p.system_back(1000.));
        p.run_commands(|| ());
        assert!(p.host().kernel().find_by_test_id("ink").is_empty());
        p.tick(1100.);
        assert!(p.stage_moving());
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut crossing = false;
        while at < ops.len() {
            if ops[at] == IMAGE_DEF {
                let picture = with_pending(|p| p.remove(&ops[at + 1])).unwrap();
                crossing |= picture
                    .as_ref()
                    .as_chunks::<4>()
                    .0
                    .contains(&[52, 44, 115, 255]);
            }
            at = stream::end(&ops, at).unwrap();
        }
        assert!(
            crossing,
            "the retained route still contains the multiplied crossing during Back"
        );
    }

    fn tags(ops: &[u32]) -> Vec<u32> {
        let mut at = 0;
        let mut tags = Vec::new();
        while at < ops.len() {
            tags.push(ops[at]);
            at = stream::end(ops, at).unwrap();
        }
        tags
    }

    fn software(app: &str, size: (f32, f32)) {
        let tags = tags(&record(app, size));
        assert!(
            !tags.contains(&MATRIX3),
            "the whole subtree must stay in its software island: {tags:?}"
        );
        assert_eq!(tags.iter().filter(|&&tag| tag == IMAGE_DEF).count(), 1);
    }

    fn wide_card(content: &str) -> String {
        format!("component App\n  view\n    box perspective=600 width=200 height=200\n      box width=400 height=100 scale=0.25 rotate=\"y 40deg\" transform-origin=\"left top\" background-color=\"#ff0000\"\n{content}\n")
    }

    #[test]
    fn projective_filter_keeps_content_outside_the_page_in_software() {
        software(&wide_card("        box width=400 height=100 filter=\"grayscale(1)\"\n          box width=100 height=100 margin-left=300 background-color=\"blue\""), (200., 200.));
    }

    #[test]
    fn projective_svg_mask_keeps_content_outside_the_page_in_software() {
        software(
            &wide_card(
                r##"        svg width=400 height=100
          mask id="m" maskUnits="userSpaceOnUse" x=0 y=0 width=400 height=100
            rect width=400 height=100 fill="white"
          rect x=300 width=100 height=100 fill="blue" mask="url(#m)""##,
            ),
            (200., 200.),
        );
    }

    #[test]
    fn projective_nested_fallback_keeps_its_island_coordinates() {
        software(&wide_card("        box width=400 height=100 perspective=20\n          box width=400 height=100 rotate=\"y 60deg\" background-color=\"blue\""), (200., 200.));
    }

    #[test]
    fn projective_svg_pattern_stays_in_software() {
        software(
            &wide_card(
                r##"        svg width=400 height=100
          pattern id="p" patternUnits="userSpaceOnUse" width=20 height=20
            rect width=10 height=20 fill="blue"
          rect width=400 height=100 fill="url(#p)""##,
            ),
            (200., 200.),
        );
    }

    #[test]
    fn projective_svg_blend_stays_in_software() {
        software(
            &wide_card(
                r##"        svg width=400 height=100
          rect width=400 height=100 fill="red"
          rect x=200 width=200 height=100 fill="blue" mix-blend-mode="multiply""##,
            ),
            (200., 200.),
        );
    }

    #[test]
    fn projective_capped_overflow_and_negative_w_stay_in_software() {
        for (x, width) in [(-800, 100), (-1100, 100), (-900, 140)] {
            software(
                &format!(
                    r##"component App
  view
    box perspective=600 width=400 height=700
      box width=200 height=100 rotate="y 40deg" background-color="red"
        box width={width} height=100 translate="{x}px 0px" background-color="blue"
"##
                ),
                (400., 700.),
            );
        }
    }

    #[test]
    fn projective_backdrop_stays_in_software() {
        software(
            &wide_card("        box width=400 height=100 backdrop-filter=\"blur(20px)\""),
            (200., 200.),
        );
    }

    const SHADOW_CARD: &str = r##"component App
  view
    box width=320 height=240 perspective=350
      box margin=60 width=200 height=100 rotate="y 60deg" background-color="white" box-shadow="0 0 24px #000000"
"##;

    #[test]
    fn projective_shadow_records_the_software_bands() {
        let ops = record(SHADOW_CARD, (320., 240.));
        let tags = tags(&ops);
        assert!(tags.contains(&MATRIX3));
        assert!(
            !tags.contains(&40),
            "device-space blur must not replace projected bands"
        );
        assert!(!tags.contains(&IMAGE_DEF));
        assert!(tags.iter().filter(|&&tag| tag == PATH).count() > 10);
        // The JVM replays this same stream against the CPU island's picture.
        let bytes: Vec<u8> = ops.iter().flat_map(|n| n.to_le_bytes()).collect();
        assert_eq!(
            bytes,
            include_bytes!("../../../android/canvas/src/test/resources/projective-shadow.ops")
        );
        let (mut cpu, error) = Presenter::boot_with(
            &crate::test_plan(SHADOW_CARD),
            (),
            (320., 240.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let reference = Pixmap::decode_png(include_bytes!(
            "../../../android/canvas/src/test/resources/projective-shadow.png"
        ))
        .unwrap();
        assert_eq!(cpu.frame().data(), reference.data());
    }

    #[test]
    fn a_plane_crossing_the_viewer_keeps_depth_clipping_in_software() {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan("component App\n  view\n    box perspective=20 width=100 height=100 margin=50\n      box width=100 height=100 rotate=\"y 60deg\" background-color=\"#ff0000\"\n"),
            (), (200., 200.), 1., std::path::PathBuf::new(), PainterChoice::Canvas,
        ).unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut pictures = 0;
        while at < ops.len() {
            assert_ne!(ops[at], MATRIX3, "depth clipping needs the software route");
            pictures += usize::from(ops[at] == IMAGE_DEF);
            at = stream::end(&ops, at).unwrap();
        }
        assert_eq!(pictures, 1);
    }

    #[test]
    fn planes_preserve_hit_maps_in_scrolled_rows_and_nested_clips() {
        let plan = crate::test_plan(
            r##"component App
  view
    scroll testId="list" width=300 height=240 margin=20
      column
        box width=240 height=80
        box width=240 height=180 perspective=500
          box testId="card" width=180 height=150 rotate="y 30deg" overflow="hidden" background-color="#ff0000"
            scroll width=140 height=110 perspective=400
              column
                box width=100 height=80 rotate="x 20deg" background-color="#0000ff"
                box width=100 height=80 background-color="#00ff00"
        box width=240 height=180
"##,
        );
        let boot = |choice| {
            let (p, error) = Presenter::boot_with(
                &plan,
                (),
                (360., 320.),
                1.,
                std::path::PathBuf::new(),
                choice,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p
        };
        let mut cpu = boot(PainterChoice::Cpu);
        let mut canvas = boot(PainterChoice::Canvas);
        let k = cpu.host().kernel();
        let list = k.node_by_key(k.find_by_test_id("list")[0]).unwrap().id;
        let card = k.node_by_key(k.find_by_test_id("card")[0]).unwrap().id;
        for dy in [0., 50., 30.] {
            for p in [&mut cpu, &mut canvas] {
                if dy != 0. {
                    p.wheel(list, 0., dy).unwrap();
                }
                p.frame();
                // The centre line has z=0; its red strip lies below the inner port.
                let y = 235. - p.scroll_of(list).1;
                assert_eq!(
                    p.hit(110., y),
                    Some(card),
                    "the row's hit map moves with its picture"
                );
            }
            for y in (0..320).step_by(7) {
                for x in (0..360).step_by(7) {
                    let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
                    assert_eq!(
                        cpu.hit(x, y),
                        canvas.hit(x, y),
                        "hit at ({x}, {y}), scroll delta {dy}"
                    );
                }
            }
        }
    }

    #[test]
    fn planes_materialize_outer_clips_before_changing_coordinates() {
        let mut r = Recorder::new();
        r.begin(300., 300., 2.);
        r.origin = (7., 11.);
        let shape = Shape::rect((10., 20., 100., 80.));
        r.clip_push(&shape, Transform::identity());
        let h = [1., 0., 30., 0., 1., 40., 0.002, 0., 1.];
        assert!(r.plane_begin(h));
        r.fill(&shape, [255, 0, 0, 255], Transform::identity());
        r.plane_end();
        r.clip_pop();
        r.fill(&shape, [0, 0, 255, 255], Transform::identity());
        let mut at = 0;
        let mut tags = Vec::new();
        while at < r.ops.len() {
            tags.push(r.ops[at]);
            if r.ops[at] == MATRIX3 {
                let m: [f32; 9] = std::array::from_fn(|i| f32::from_bits(r.ops[at + i + 1]));
                let (px, py) = crate::placement::map(&h, 80., 60.);
                let (x, y) = crate::placement::map(&m, 80., 60.);
                assert!((x - (px - 7.) * 2.).abs() < 0.001);
                assert!((y - (py - 11.) * 2.).abs() < 0.001);
            }
            at = stream::end(&r.ops, at).unwrap();
        }
        assert_eq!(
            tags,
            [MATRIX, CLIP_RRECT, MATRIX3, RRECT, RESTORE, MATRIX, RRECT]
        );
    }
}

#[cfg(test)]
mod measurement {
    use super::*;
    use crate::paint::{Backend, Painter, Scene};
    use crate::presenter::{PainterChoice, Presenter};
    use std::collections::BTreeMap;
    use std::time::Instant;

    #[test]
    #[ignore = "manual issue 231 measurement"]
    fn turned_boxes_frame_cost() {
        for scale in [1., 3.] {
            for turned in [false, true] {
                let mut app = String::from(
                    "component App\n  view\n    view position=\"relative\" width=864 height=480\n",
                );
                for i in 0..16 {
                    app.push_str(&format!("      view position=\"absolute\" left={} top={} perspective=600 width=200 height=100\n        view width=200 height=100 {} background-color=\"#ff0000\"\n          text \"Card {i}\" font-size=18 color=\"white\" margin=12\n", 16 + (i % 4) * 212, 16 + (i / 4) * 112, if turned { "rotate=\"y 40deg\"" } else { "" }));
                }
                let (p, error) = Presenter::boot_with(
                    &crate::test_plan(&app),
                    (),
                    (864., 480.),
                    scale,
                    std::path::PathBuf::new(),
                    PainterChoice::Cpu,
                )
                .unwrap();
                assert!(error.is_none(), "{error:?}");
                let kernel = p.host().kernel();
                for name in ["cpu", "gpu", "canvas"] {
                    let backend: Box<dyn Backend> = match name {
                        "cpu" => Box::new(crate::raster::Raster::new()),
                        "gpu" => Box::new(crate::gpu::Gpu::new().unwrap()),
                        _ => Box::new(crate::canvas::Recorder::new()),
                    };
                    let mut painter = Painter::new(p.text().clone(), scale, backend);
                    let mut times = Vec::new();
                    for frame in 0..70 {
                        let angle = 30. + (frame % 30) as f32 * 20. / 30.;
                        let scene = Scene {
                            kernel,
                            roots: &kernel.roots(),
                            hidden: &|_| false,
                            presented: &|id| {
                                let mut shown = p.host().presented(id);
                                if kernel.node(id).unwrap().style.rotate_axis.is_3d() {
                                    shown.rotate = angle;
                                }
                                shown
                            },
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
                        let start = Instant::now();
                        let painted = painter.paint(&scene, (864., 480.)).unwrap();
                        std::hint::black_box(&painted);
                        let elapsed = start.elapsed().as_secs_f64() * 1000.;
                        if name == "canvas" {
                            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
                            let mut at = 0;
                            let mut planes = 0;
                            let mut glyphs = 0;
                            while at < ops.len() {
                                assert_ne!(
                                    ops[at], IMAGE_DEF,
                                    "plain cards must not use software islands"
                                );
                                planes += usize::from(ops[at] == MATRIX3);
                                glyphs += usize::from(ops[at] == GLYPHS);
                                at = stream::end(&ops, at).unwrap();
                            }
                            assert_eq!(planes > 0, turned);
                            assert_eq!(glyphs, 16);
                        }
                        if frame >= 10 {
                            times.push(elapsed);
                        }
                    }
                    times.sort_by(f64::total_cmp);
                    eprintln!("ISSUE231 {name} scale={scale} turned={turned} frames={} p50={:.3}ms p95={:.3}ms", times.len(), times[30], times[57]);
                }
            }
        }
    }
}
