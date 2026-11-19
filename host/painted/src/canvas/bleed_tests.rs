//! What the reader is sent for a box at an edge of the page (LLP 1115.001 D3.7).
use super::super::{stream, Recorder, FINISHED, ROW_BEGIN, RRECT};
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
    let fill = |argb: u32| fills.iter().find(|f| f.0 == argb).map(|f| f.1);
    // The red bar touches the top: its fill grows by the 30 pt band above
    // the page. The blue one, 8 pt below it, stays its own size.
    assert_eq!(fill(0xffff0000), Some([0., -30., 400., 86.]), "{fills:?}");
    assert_eq!(fill(0xff0000ff), Some([0., 64., 400., 56.]), "{fills:?}");
}
