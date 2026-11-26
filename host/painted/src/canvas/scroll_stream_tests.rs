use super::*;

#[test]
fn a_scrolls_direct_child_background_keeps_the_device_scale() {
    use crate::presenter::{PainterChoice, Presenter};

    let source = r##"component App
  view
    main width="100%" height="100%" box-sizing="border-box" display="flex" flex-direction="column" gap=16 padding=16 color="#f5f4ef"
      column testId="outside" background-color="#2a2a2a" border-radius=16 padding=16
        text "Outside scroll"
        text "Second line"
      scroll flex-shrink=0 height=120
        column testId="direct" background-color="#123456" border-radius=16 padding=16
          text "Scroll's direct child"
          text "Second line"
      scroll flex=1 min-height=0
        column gap=16
          column testId="nested" background-color="#2a2a2a" border-radius=16 padding=16
            text "Nested inside scroll"
            text "Second line"
"##;
    for scale in [1., 2.625] {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (411., 500.),
            scale,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut matrix = [1., 0., 0., 1., 0., 0.];
        let mut found = false;
        while at < ops.len() {
            match ops[at] {
                GROUP_BEGIN => matrix = [1., 0., 0., 1., 0., 0.],
                MATRIX => matrix = std::array::from_fn(|i| f32::from_bits(ops[at + 1 + i])),
                RRECT if ops[at + 1] == 0xff123456 => {
                    assert_eq!(
                        (matrix[0], matrix[3]),
                        (scale, scale),
                        "direct child's background"
                    );
                    found = true;
                }
                _ => (),
            }
            at = end(&ops, at).expect("valid Canvas stream");
        }
        assert!(
            found,
            "the direct child's background is in its scroll group"
        );
    }
}

#[test]
fn scroll_groups_establish_their_own_matrix_and_leave_the_outer_drawing_independent() {
    for scale in [1., 2.625] {
        let mut r = Recorder::new();
        r.begin(411., 500., scale);
        let outer = Transform::from_translate(16., 102.);
        let inner = Transform::from_translate(16., 118.);
        let shape = Shape::new((0., 0., 379., 70.), [16.; 4]);
        r.push_clip(&Shape::rect((0., 0., 379., 120.)), outer);
        r.group_begin(7, (0., 0.));
        let start = r.ops.len();
        r.fill(&shape, [42, 42, 42, 255], outer);
        assert_eq!(
            r.ops[start], MATRIX,
            "scale {scale}: group's first background"
        );
        let matrix: Vec<_> = r.ops[start + 1..start + 7]
            .iter()
            .map(|v| f32::from_bits(*v))
            .collect();
        assert_eq!(matrix, [scale, 0., 0., scale, 16. * scale, 102. * scale]);
        r.fill(&shape, [20, 30, 40, 255], inner);
        r.group_end();
        let after = r.ops.len();
        r.fill(&shape, [20, 30, 40, 255], inner);
        assert_eq!(
            r.ops[after], MATRIX,
            "scale {scale}: drawing after the group"
        );
        r.pop_clip();
    }
}

#[test]
fn a_moved_software_row_has_painted_the_part_that_enters_the_view() {
    use super::scroll::Painted;
    use super::stream::tests::{canvas_fixture, software_picture};

    for height in [10, 120] {
        let mut p = canvas_fixture(
            &format!(
                r#"component App
  view
    scroll width=200 height=80 testId="port"
      column
        box height=75
        column height=200
          box width=10 height={height} background-color="red" filter="brightness(1)" translate="0px 0px 1px"
"#
            ),
            1.,
        );
        p.frame();
        let image = software_picture();
        let k = p.host().kernel();
        let port = k.node_by_key(k.find_by_test_id("port")[0]).unwrap().id;
        let painted = Painted::capture(&p).unwrap();
        assert_eq!(p.scroll_to(port, (0., 10.)), Some((0., 10.)));
        let shift = painted.shift(&mut p, None, 1., false);
        if height == 10 {
            assert_eq!(image.height(), 10);
            assert_eq!(image.pixel(5, 9).unwrap().red(), 255);
            assert_eq!(
                shift.unwrap(),
                [super::SHIFT, 1, port, 0, (-10_f32).to_bits()]
            );
        } else {
            assert_eq!(image.height(), 5);
            assert!(
                shift.is_none(),
                "the next frame reveals a new part of the picture"
            );
            p.frame();
            assert_eq!(software_picture().height(), 15);
        }
    }
}
