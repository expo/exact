use super::*;

const POPOVER: &str = r##"component App
  state saves = 0
  action save
    saves = saves + 1
  view
    column width="100%" height="100%" background-color="#ffffff"
      button "Open" popovertarget="content" testId="open" width=80 height=32
      column id="content" popover="auto" role="dialog" width=160 font-size=30 testId="content" PADDING SURFACE
        button press=save appearance="none" width=160 height=32 min-height=0 padding=0 border="none" background-color="#ff0000" testId="save"
"##;

fn open(padding: &str, surface: &str) -> Presenter<()> {
    setup();
    let source = POPOVER
        .replace("PADDING", padding)
        .replace("SURFACE", surface);
    let mut p = boot(&source, 480.);
    p.tap(id(&p, "open")).unwrap();
    p.run_commands(|| ());
    p.frame();
    p
}

fn insets(p: &mut Presenter<()>) -> [f32; 4] {
    let (x, y, w, h) = p.rect_of(id(p, "content")).unwrap();
    let (cx, cy, cw, ch) = p.rect_of(id(p, "save")).unwrap();
    [cy - y, x + w - cx - cw, y + h - cy - ch, cx - x]
}

#[test]
fn authored_popovers_pad_unsaid_edges_by_the_root_font() {
    for surface in ["", "background-color=\"#eeeeee\""] {
        for (padding, expected, larger) in [
            ("", [16.; 4], [20.; 4]),
            ("padding=0", [0.; 4], [0.; 4]),
            ("padding=16", [16.; 4], [16.; 4]),
            (
                "padding-top=7 padding-left=0",
                [7., 16., 16., 0.],
                [7., 20., 20., 0.],
            ),
            (
                "direction=\"rtl\" padding-top=7 padding-left=0",
                [7., 16., 16., 0.],
                [7., 20., 20., 0.],
            ),
            (
                "padding=\"2px 3px 4px 5px\"",
                [2., 3., 4., 5.],
                [2., 3., 4., 5.],
            ),
        ] {
            let mut p = open(padding, surface);
            assert_eq!(insets(&mut p), expected, "{padding:?}, {surface:?}");
            assert!(p.set_root_font_size(20.).is_none());
            p.frame();
            assert_eq!(insets(&mut p), larger, "{padding:?}, root 20");
        }
    }
}

#[test]
fn popover_pixels_and_pointer_input_use_the_padded_content_box() {
    let mut p = open("", "background-color=\"#eeeeee\"");
    let (x, y, w, h) = p.rect_of(id(&p, "content")).unwrap();
    let shot = p.frame();
    let pixel = |px: f32, py: f32| {
        let c = shot.pixel(px as u32, py as u32).unwrap().demultiply();
        [c.red(), c.green(), c.blue()]
    };
    assert_eq!((w, h), (192., 64.));
    for (px, py) in [(x + 8., y + h / 2.), (x + w / 2., y + 8.)] {
        assert_eq!(pixel(px, py), [238; 3], "the padding shows the surface");
        p.pointer_down(px, py, p.host().now()).unwrap();
        p.pointer_up(px, py, p.host().now()).unwrap();
        p.run_commands(|| ());
        assert_eq!(
            p.host().runner().slot("saves"),
            Some(&exact_runner::Value::Number(0.))
        );
        assert!(
            p.rect_of(id(&p, "save")).is_some(),
            "padding stays inside the popover"
        );
    }
    let at = (x + 24., y + 24.);
    assert_eq!(pixel(at.0, at.1), [255, 0, 0]);
    p.pointer_down(at.0, at.1, p.host().now()).unwrap();
    p.pointer_up(at.0, at.1, p.host().now()).unwrap();
    p.run_commands(|| ());
    assert_eq!(
        p.host().runner().slot("saves"),
        Some(&exact_runner::Value::Number(1.))
    );
}
