//! What a kept row paints past its boxes: Ink keeps drawing a row whose
//! boxes are outside the port while its paint reaches into it.
use super::*;

/// Ink, keeping rows, draws red where the CPU painter does in the frame
/// after `drive`, at 200 by 120 points; Ink's red pixels, row by row, or
/// none without an Ink device.
fn ink_draws_the_red_the_cpu_draws(
    source: &str,
    drive: impl Fn(&mut Presenter<()>),
) -> Option<Vec<bool>> {
    let boot = || {
        let (p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (200., 120.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p
    };
    let red = |pixel: &tiny_skia::PremultipliedColorU8| {
        pixel.red() > 200 && pixel.green() < 50 && pixel.blue() < 50
    };
    let mut cpu = boot();
    drive(&mut cpu);
    let expected: Vec<_> = cpu.frame().pixels().iter().map(red).collect();
    assert!(
        expected.iter().any(|red| *red),
        "the CPU draws red in the port"
    );
    let mut ink = boot();
    ink.reader_replaced(Box::new(super::ink()?));
    drive(&mut ink);
    let drawn: Vec<_> = ink.frame().pixels().iter().map(red).collect();
    assert!(
        drawn
            .iter()
            .zip(expected)
            .any(|(drawn, expected)| *drawn && expected),
        "Ink draws it"
    );
    Some(drawn)
}

fn scroll(p: &mut Presenter<()>, y: f32) {
    let k = p.host.kernel();
    let port = k.node_by_key(k.find_by_test_id("port")[0]).unwrap().id;
    p.scroll_to(port, (0., y)).unwrap();
}

#[test]
fn an_inline_background_still_draws_when_its_sticky_section_row_is_offscreen() {
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=5
        column width=200
          box position="sticky" top=0 width=200 height=20
          box width=200 height=150
          text width=200 height=20 font-size=16 line-height="20px"
            text "MMMM" background-color="red" padding-top=100
          box width=200 height=300
"#,
        |p| {
            p.frame();
        },
    );
}

#[test]
fn a_wide_text_stroke_still_draws_when_its_row_is_offscreen() {
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=175
        text "MMMM" width=200 height=20 font-size=16 line-height="20px" -webkit-text-stroke="200px #ff0000"
        box width=200 height=300
"#,
        |p| {
            p.frame();
        },
    );
}

#[test]
fn a_box_a_layout_transition_holds_larger_still_draws_when_its_row_is_offscreen() {
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  state small = false
  task shrink mount
    after(1000, go)
  action go
    small = true
  view
    scroll testId="port" width=200 height=100
      column
        box width=200 height=(small ? 20 : 200) flex-shrink=0 background-color="red" -exact-layout-transition="1s linear"
        box width=200 height=500 flex-shrink=0
"#,
        |p| {
            p.frame();
            scroll(p, 150.);
            p.frame();
            p.clock(1000.);
            p.frame();
            // The box is 20 points tall above the port; its surface is still
            // most of its 200.
            p.clock(1100.);
        },
    );
}

#[test]
fn an_outgoing_number_still_draws_when_its_row_is_offscreen() {
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  state n = 12345678901
  task bump mount
    after(1000, step)
  action step
    n = 1
  view
    scroll width=200 height=100
      column
        box width=200 height=10 flex-shrink=0
        text `${n}` width=20 margin-left=-100 font-size=24 color="red" flex-shrink=0 -exact-content-transition="numeric"
        box width=200 height=300 flex-shrink=0
"#,
        |p| {
            p.frame();
            p.clock(1000.);
            p.frame();
            p.clock(1050.);
        },
    );
}

#[test]
fn a_pinned_and_a_pushed_off_heading_draw_over_their_section() {
    let source = r#"component App
  view
    scroll testId="port" width=200 height=100
      column
        box width=200 height=40
        column width=200 height=180
          box position="sticky" top=0 z-index=1 width=200 height=20 background-color="red"
          box width=200 height=80 background-color="blue"
          box width=200 height=80 background-color="blue"
        box width=200 height=120
"#;
    // Pinned, the heading covers y = 17; pushed off by 205, it ends at 15.
    for (at, covers) in [(80., true), (205., false)] {
        let Some(red) = ink_draws_the_red_the_cpu_draws(source, |p| {
            p.frame();
            scroll(p, at);
        }) else {
            return;
        };
        assert_eq!(red[17 * 200 + 100], covers, "scrolled {at}");
    }
}

#[test]
fn an_outgoing_number_aligned_away_from_its_start_still_draws() {
    // One commit narrows the box and changes the digit: the old one rolls
    // out at the old right edge, past the new number's width.
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  state n = 7
  task bump mount
    after(1000, step)
  action step
    n = 4
  view
    scroll width=200 height=100
      column
        box width=200 height=10 flex-shrink=0
        text `${n}` width=(n > 5 ? 400 : 30) margin-left=-250 text-align="right" font-size=24 color="red" flex-shrink=0 -exact-content-transition="numeric"
        box width=200 height=300 flex-shrink=0
"#,
        |p| {
            p.frame();
            p.clock(1000.);
            p.frame();
            // At 16 ms the old digit is still opaque enough to read as red;
            // by 50 ms it has faded to two thirds.
            p.clock(1016.);
        },
    );
}

#[test]
fn a_column_rule_wider_than_its_gap_still_draws_when_its_row_is_offscreen() {
    // The rule is centred on the gap at x = 310 and reaches back to 160.
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=10 flex-shrink=0
        view width=100 height=20 margin-left=260 flex-shrink=0 column-count=2 column-gap=20 column-fill="auto" column-rule-style="solid" column-rule-width=300 column-rule-color="red"
          text "one\ntwo\nthree\nfour" white-space="pre"
"#,
        |p| {
            p.frame();
        },
    );
}

#[test]
fn an_outside_list_marker_still_draws_when_its_row_is_offscreen() {
    // The number is drawn before its item, which starts at x = 260.
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  view
    scroll width=200 height=120
      column
        box width=200 height=10 flex-shrink=0
        ol width=100 margin-left=260 padding-left=0 flex-shrink=0 font-size=80 color="red"
          li "One"
"#,
        |p| {
            p.frame();
        },
    );
}

#[test]
fn a_column_of_text_past_its_box_still_draws_when_its_row_is_offscreen() {
    // Two columns hold two lines; the other four overflow to the right.
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=10 flex-shrink=0
        text "one\ntwo\nthree\nfour\nfive\nsix" white-space="pre" width=100 height=20 margin-left=-200 column-count=2 column-gap=20 column-fill="auto" color="red" flex-shrink=0
"#,
        |p| {
            p.frame();
        },
    );
}

#[test]
fn an_indented_list_marker_still_draws_when_its_row_is_offscreen() {
    // The indent draws the number 100 points before where it would be.
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  view
    scroll width=200 height=120
      column
        box width=200 height=10 flex-shrink=0
        ol width=100 margin-left=260 padding-left=0 flex-shrink=0 font-size=80 text-indent=-100 color="red"
          li ""
"#,
        |p| {
            p.frame();
        },
    );
}

#[test]
fn a_number_its_indent_moves_before_its_start_still_rolls() {
    // The indent puts the digit 100 points before the paragraph's start,
    // past the overhang the first cell was clipped to.
    ink_draws_the_red_the_cpu_draws(
        r#"component App
  state n = 7
  task bump mount
    after(1000, step)
  action step
    n = 4
  view
    scroll width=200 height=100
      column
        box width=200 height=10 flex-shrink=0
        text `${n}` width=30 margin-left=120 text-indent=-100 font-size=24 color="red" flex-shrink=0 -exact-content-transition="numeric"
        box width=200 height=300 flex-shrink=0
"#,
        |p| {
            p.frame();
            p.clock(1000.);
            p.frame();
            p.clock(1016.);
        },
    );
}

/// The bounds of the one row a scroller's only column holds, on a backend
/// that keeps rows.
fn row_bounds(rows: &str) -> Rect4 {
    let source =
        format!("component App\n  view\n    scroll width=200 height=100\n      column\n{rows}");
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (200., 120.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.reader_replaced(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    p.frame();
    let bounds = heard.borrow().bounds.clone();
    assert_eq!(bounds.len(), 1, "{source}");
    bounds[0]
}

#[test]
fn a_column_of_text_reaches_only_as_far_as_its_clips_show_it() {
    // One line a column, two columns in the box: the other four are past its
    // right, from 120 to 340.
    let text = r#"text "one\ntwo\nthree\nfour\nfive\nsix" white-space="pre" width=100 height=20 column-count=2 column-gap=20 column-fill="auto" flex-shrink=0"#;
    let right = |b: Rect4| b.0 + b.2;
    let seven = text.replace(r#"six""#, r#"six\nseven""#);
    let clipped_x = row_bounds(&format!("        {text} overflow-x=\"clip\"\n"));
    assert!(right(clipped_x) <= 200. + 48., "{clipped_x:?}");
    // Turned, the clip still cuts the columns past the box away; its
    // unbounded y does not cover them. A seventh column, cut to an empty
    // edge at 360, would reach past the limit if an empty cut were kept.
    let turned = row_bounds(&format!(
        "        {seven} overflow-x=\"clip\" rotate=\"45deg\"\n"
    ));
    assert!(right(turned) <= 200. + 48., "{turned:?}");
    let clipped_y = row_bounds(&format!("        {text} overflow-y=\"clip\"\n"));
    assert!(right(clipped_y) >= 340., "{clipped_y:?}");
    // The text clips itself far below a row its ancestor clips to 20 points.
    let far = row_bounds(&format!(
        "        column height=20 overflow=\"clip\" flex-shrink=0\n          {text} overflow=\"clip\" margin-top=10000\n"
    ));
    assert!(far.1 + far.3 <= 20. + 48., "{far:?}");
}
