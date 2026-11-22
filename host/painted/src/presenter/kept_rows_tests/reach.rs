//! What a kept row paints past its boxes: Ink keeps drawing a row whose
//! boxes are outside the port while its paint reaches into it.
use super::*;

/// Ink, keeping rows, draws red where the CPU painter does in the frame
/// after `drive`, at 200 by 120 points.
fn ink_draws_the_red_the_cpu_draws(source: &str, drive: impl Fn(&mut Presenter<()>)) {
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
    ink.reader_replaced(Box::new(crate::ink::Ink::new().expect("an Ink device")));
    drive(&mut ink);
    assert!(
        ink.frame()
            .pixels()
            .iter()
            .zip(expected)
            .any(|(pixel, expected)| expected && red(pixel)),
        "Ink draws it"
    );
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
    for at in [80., 205.] {
        ink_draws_the_red_the_cpu_draws(source, |p| {
            p.frame();
            scroll(p, at);
        });
    }
}
