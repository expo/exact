use crate::presenter::{PainterChoice, Presenter};
use std::path::PathBuf;

#[test]
fn svg_filter_pixels_follow_inherited_color_and_appearance() {
    let app = r##"component App
  view
    svg width=100 height=100
      filter id="f"
        feFlood flood-color="light-dark(#ff0000, #ff00ff)"
      g color="light-dark(#ff0000, #ff00ff)"
        rect x=10 y=10 width=20 height=10 fill="blue" filter="drop-shadow(8px 8px 0px)"
        rect x=10 y=40 width=20 height=10 fill="blue" filter="drop-shadow(8px 8px 0px light-dark(#ff0000, #ff00ff))"
        rect x=60 y=10 width=20 height=10 filter="url(#f)"
"##;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(app),
        (),
        (100.0, 100.0),
        1.0,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    for dark in [false, true, false] {
        p.set_system_scheme(dark);
        let frame = p.frame();
        let rgb = |x, y| {
            let c = frame.pixel(x, y).unwrap();
            (c.red(), c.green(), c.blue())
        };
        assert_eq!(rgb(15, 15), (0, 0, 255));
        let expected = if dark { (255, 0, 255) } else { (255, 0, 0) };
        for (x, y) in [(35, 25), (35, 55), (65, 15)] {
            assert_eq!(rgb(x, y), expected, "({x}, {y}), dark={dark}");
        }
    }
}
