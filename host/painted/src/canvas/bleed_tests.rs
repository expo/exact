//! What the reader is sent for a box at an edge of the page (LLP 1115.001 D3.7),
//! and for a sheet over a page laid out between the system bars (D10).
use super::super::{stream, Recorder, FINISHED, ROW_BEGIN, RRECT};
use crate::paint::Backend;
use crate::presenter::top::SurfacePainter;
use crate::presenter::{PainterChoice, Presenter, Window};

#[test]
fn a_bar_at_the_top_fills_the_status_bar_and_one_below_it_does_not() {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(
            r##"component App
  view
    main width="100%" height="100%"
      box height=56 background-color="#ff0000"
      box height=56 margin-top=8 background-color="#0000ff"
"##,
        ),
        (),
        (400., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let phone = Window {
        size: (400., 800.),
        bars: [30., 0., 16., 0.],
        ..Window::default()
    };
    assert!(p.set_window(phone).is_none());
    p.brush.replace_backend(Box::new(Recorder::new()));
    p.frame();
    let fills = fills();
    let fill = |argb: u32| fills.iter().find(|f| f.0 == argb).map(|f| f.1);
    // The red bar touches the top: its fill grows by the 30 pt band above
    // the page. The blue one, 8 pt below it, stays its own size.
    assert_eq!(fill(0xffff0000), Some([0., -30., 400., 86.]), "{fills:?}");
    assert_eq!(fill(0xff0000ff), Some([0., 64., 400., 56.]), "{fills:?}");
}

#[test]
fn a_sheet_and_its_scrim_reach_the_window_edges() {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(
            r##"routes nav
  home "/"
    sheet "/sheet"
component App
  action open
    nav = push(nav, "/sheet")
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` position="absolute" inset=0
            button press=open testId="open" height=40
              text "Open"
        if e.name == "sheet"
          column navigationKey=`${e.id}` navigationPresentation="modal" position="absolute" inset=0
            text "In the sheet"
"##,
        ),
        (),
        (400., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    struct Shell;
    impl SurfacePainter for Shell {
        fn record(
            &mut self,
            _: (f32, f32),
            _: f32,
            _: &mut dyn FnMut(&mut dyn Backend),
        ) -> Vec<u32> {
            Vec::new()
        }
    }
    p.serve_windows(Box::new(Shell));
    assert_eq!(p.serve_bars(Box::new(Shell)), None);
    let phone = Window {
        size: (400., 800.),
        bars: [30., 0., 16., 0.],
        ..Window::default()
    };
    assert!(p.set_window(phone).is_none());
    p.frame();
    let open = p.host().kernel().find_by_test_id("open")[0];
    let open = p.host().kernel().node_by_key(open).unwrap().id;
    p.tap(open).unwrap();
    p.run_commands(|| ());
    let now = p.host().now() + 1000.0;
    p.tick(now);
    p.frame();
    p.brush.replace_backend(Box::new(Recorder::new()));
    p.frame();
    let fills = fills();
    // The page is 754 from y 30: the scrim covers the 30 above and the 16
    // below it, and the sheet drawn on it runs on under the gesture bar to
    // the window's foot, at y 770.
    let scrim = fills
        .iter()
        .position(|f| f.0 == 0x5200_0000)
        .expect("a scrim");
    assert_eq!(fills[scrim].1, [0., -30., 400., 800.], "{fills:?}");
    let body = fills[scrim + 1].1;
    assert_eq!(body[1] + body[3], 770., "{fills:?}");
}

/// The fills, with their colour, in the frame the reader was last sent.
fn fills() -> Vec<(u32, [f32; 4])> {
    let ops = FINISHED.with(|f| f.borrow_mut().take()).expect("a frame");
    let mut fills = Vec::new();
    let mut at = 0;
    while at < ops.len() {
        if ops[at] == RRECT {
            let rect = [2, 3, 4, 5].map(|k| f32::from_bits(ops[at + k]));
            fills.push((ops[at + 1], rect));
        }
        at = match ops[at] {
            ROW_BEGIN => at + 6,
            _ => stream::end(&ops, at).expect("a valid stream"),
        };
    }
    fills
}
