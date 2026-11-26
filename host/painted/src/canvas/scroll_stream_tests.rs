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
fn a_nested_scroller_opens_its_own_group_inside_its_outer_scroller() {
    use crate::presenter::{PainterChoice, Presenter};

    let source = r##"component App
  state count = 0
  action bump
    count = count + 1
  view
    scroll testId="outer" width=360 height=400
      column
        column testId="top" height=120 background-color="#112233"
          text "Header"
        column testId="card" padding=12 background-color="#223344"
          text "Section"
          scroll testId="inner" width=300 height=200
            column
              button testId="r0" press=bump height=80 background-color="#334455"
                text `Row 0: ${count}`
              column testId="r1" height=80 background-color="#445566"
                text "Row 1"
              column testId="r2" height=80 background-color="#556677"
                text "Row 2"
              column testId="r3" height=80 background-color="#667788"
                text "Row 3"
        column testId="bottom" height=240 background-color="#778899"
          text "Footer"
"##;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (360., 400.),
        2.,
        std::path::PathBuf::new(),
        PainterChoice::Canvas,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
    let by_test_id = |name: &str| {
        let k = p.host().kernel();
        k.arena()
            .iter_live()
            .map(|s| k.arena().local_id(s))
            .find(|&id| {
                k.node(id)
                    .and_then(|n| n.props.str(exact_kernel::PropId::TestId))
                    == Some(name)
            })
            .expect(name)
    };
    let outer = by_test_id("outer");
    let inner = by_test_id("inner");

    // Verify nested GROUP_BEGIN/GROUP_END in stream and inner rows drawn inside inner's group.
    let mut at = 0;
    let mut stack = Vec::new();
    let mut saw_nested = false;
    let mut inner_rows = 0;
    while at < ops.len() {
        match ops[at] {
            GROUP_BEGIN => {
                let id = ops[at + 1];
                if id == inner && stack.last() == Some(&outer) {
                    saw_nested = true;
                }
                stack.push(id);
            }
            GROUP_END => {
                stack.pop().expect("balanced GROUP_END");
            }
            ROW_DRAW if stack.last() == Some(&inner) => inner_rows += 1,
            _ => (),
        }
        at = end(&ops, at).expect("valid Canvas stream");
    }
    assert!(stack.is_empty(), "every group closed");
    assert!(saw_nested, "inner group nested inside outer group");
    assert!(
        inner_rows >= 4,
        "inner scroller draws its own retained rows: {inner_rows}"
    );

    // Both scrollers are reported as groups with outer's box range containing inner's.
    let groups = p.brush.row_groups();
    let og = groups.iter().find(|g| g.0 == outer).expect("outer group");
    let ig = groups.iter().find(|g| g.0 == inner).expect("inner group");
    assert!(
        og.1 <= ig.1 && ig.2 <= og.2,
        "outer {og:?} contains inner {ig:?}"
    );

    // Both scrollers expose `node = true` so the Android reader translates each directly.
    let under = p.scrollers_at(100., 220.);
    assert!(
        under.chain.iter().any(|s| s.id == inner && s.node),
        "inner scroller has node=true: {:?}",
        under.chain
    );
    assert!(
        under.chain.iter().any(|s| s.id == outer && s.node),
        "outer scroller has node=true: {:?}",
        under.chain
    );

    // Scrolling the inner list can move via SHIFT without repainting the outer scroller.
    let painted = scroll::Painted::capture(&p).expect("painted snapshot");
    assert!(p.scroll_by(inner, 40.));
    let shift = painted
        .shift(&mut p, None, 2., false)
        .expect("move-only shift");
    assert_eq!(shift[0], SHIFT);
    assert_eq!(shift[1], 2);
}

#[test]
fn offscreen_nested_scroller_records_its_background_and_rows_for_outer_shift() {
    // Inner scroller starts completely below the outer viewport (y=400 > height=240).
    // Its own background and its rows must still record inside outer's group so shifting
    // outer into view reveals them without a blank frame.
    let src = r##"component App
  state items = ["a", "b", "c", "d"]
  view
    scroll testId="outer" width=240 height=240
      column width=240 gap=0
        box width=240 height=400 background-color="#eeeeee"
        scroll testId="inner" width=240 height=160 background-color="#336699"
          column width=240 gap=4
            for x in items key=x
              box width=240 height=50 background-color="#ff0000"
                text x
"##;
    use crate::presenter::{PainterChoice, Presenter};
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(src),
        (),
        (240., 240.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Canvas,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
    let by_test_id = |name: &str| {
        let k = p.host().kernel();
        k.arena()
            .iter_live()
            .map(|s| k.arena().local_id(s))
            .find(|&id| {
                k.node(id)
                    .and_then(|n| n.props.str(exact_kernel::PropId::TestId))
                    == Some(name)
            })
            .expect(name)
    };
    let outer = by_test_id("outer");
    let inner = by_test_id("inner");

    let mut at = 0;
    let mut stack = Vec::new();
    let mut outer_fills = 0;
    let mut inner_rows = 0;
    while at < ops.len() {
        match ops[at] {
            GROUP_BEGIN => stack.push(ops[at + 1]),
            GROUP_END => {
                stack.pop().expect("balanced GROUP_END");
            }
            2 if stack.last() == Some(&outer) => outer_fills += 1,
            ROW_DRAW if stack.last() == Some(&inner) => inner_rows += 1,
            _ => (),
        }
        at = end(&ops, at).expect("valid Canvas stream");
    }
    assert!(
        outer_fills >= 1,
        "offscreen inner container background inside outer group must not be culled: {outer_fills}"
    );
    assert_eq!(
        inner_rows, 4,
        "offscreen inner scroller must still record all rows for outer SHIFT: {inner_rows}"
    );
}
