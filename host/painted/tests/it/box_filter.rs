//! CSS `filter` on a box (LLP 1055.000 §0, LLP 1159.000): the box and what
//! it holds are pictured and the chain runs over the picture, where the
//! paint walk used to draw them unfiltered. Held to Chrome by the SVG parity
//! smoke's `fx-box-filter` and `fx-image-filter`; this holds what a picture
//! must not break.
use exact_painted::{presenter::PainterChoice, Presenter};

fn boot(source: &str, size: (f32, f32)) -> Presenter<()> {
    let plan = contract::compile(source).unwrap_or_else(|e| panic!("{e}"));
    let (presenter, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        size,
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    presenter
}

fn rgba(p: &mut Presenter<()>, x: u32, y: u32) -> (u8, u8, u8, u8) {
    let c = p.frame().pixel(x, y).unwrap().demultiply();
    (c.red(), c.green(), c.blue(), c.alpha())
}

#[test]
fn a_chain_runs_over_the_box_and_what_it_holds_and_nothing_else() {
    let mut p = boot(
        r##"component App
  view
    box width=100 height=100 background-color="#ffffff"
      box width=40 height=40 background-color="#ff0000" filter="grayscale(1)"
        box width=10 height=10 margin-left=20 background-color="#00ff00"
      box width=40 height=40 background-color="#ff0000"
"##,
        (100., 100.),
    );
    let grey = |(r, g, b, a): (u8, u8, u8, u8)| r == g && g == b && a == 255;
    let own = rgba(&mut p, 5, 5);
    assert!(
        grey(own) && own.0 < 100,
        "the box's own red is grey: {own:?}"
    );
    let held = rgba(&mut p, 25, 5);
    assert!(
        grey(held) && held.0 > own.0,
        "its child's green is a lighter grey: {held:?}"
    );
    assert_eq!(
        rgba(&mut p, 5, 45),
        (255, 0, 0, 255),
        "its sibling is not filtered"
    );
    assert_eq!(
        rgba(&mut p, 80, 5),
        (255, 255, 255, 255),
        "nor what is behind it"
    );
}

#[test]
fn a_shadow_falls_past_the_box_and_opacity_applies_after_the_filter() {
    let page = |opacity: &str| {
        format!(
            r##"component App
  view
    box width=100 height=100 background-color="#ffffff"
      box width=30 height=30 margin-left=10 margin-top=10 background-color="#ff0000" opacity={opacity} filter="drop-shadow(20px 0px 0px #0000ff)"
"##
        )
    };
    let mut p = boot(&page("1"), (100., 100.));
    assert_eq!(
        rgba(&mut p, 20, 20),
        (255, 0, 0, 255),
        "the box paints over its shadow"
    );
    assert_eq!(
        rgba(&mut p, 55, 20),
        (0, 0, 255, 255),
        "the shadow shows past it"
    );
    assert_eq!(
        rgba(&mut p, 70, 20),
        (255, 255, 255, 255),
        "and ends at its offset"
    );
    // Half opaque, box and shadow fade together over the white behind.
    let mut half = boot(&page("0.5"), (100., 100.));
    let (r, g, b, _) = rgba(&mut half, 55, 20);
    assert!(
        b == 255 && (120..=135).contains(&r) && r == g,
        "{:?}",
        (r, g, b)
    );
}

#[test]
fn a_press_inside_a_filtered_box_reaches_what_is_drawn_there() {
    let mut p = boot(
        r##"component App
  state picked = "none"
  action pick(which: string)
    picked = which
  view
    column
      text picked testId="picked"
      box width=100 height=60 background-color="#dddddd" filter="blur(1px)"
        button press=pick("inside") testId="inside" width=60 height=40 margin-left=20 margin-top=10 -exact-press-scale=1
          text "Go"
"##,
        (200., 200.),
    );
    let kernel = p.host().kernel();
    let button = kernel
        .node_by_key(kernel.find_by_test_id("inside")[0])
        .unwrap()
        .id;
    let b = p
        .boxes()
        .iter()
        .find(|b| b.id == button)
        .expect("the button's box")
        .clone();
    p.press_at(b.rect.0 + b.rect.2 / 2.0, b.rect.1 + b.rect.3 / 2.0, 0.0);
    let k = p.host().kernel();
    let text = k.node_by_key(k.find_by_test_id("picked")[0]).unwrap();
    assert_eq!(text.props.str(exact_kernel::PropId::Text), Some("inside"));
}

/// A filtered box more than twice the window's pixels is pictured only over
/// the window, from what lies within the chain's reach of it: a blurred fill
/// past the window's edge stays whole at the edge, and the shadow of a box
/// left of the window falls into it.
#[test]
fn a_chain_past_the_window_reads_what_lies_past_its_edge() {
    let mut fill = boot(
        r##"component App
  view
    box width=600 height=600 margin-left=-250 margin-top=-250 background-color="#2468ac" filter="blur(10px)"
"##,
        (100., 100.),
    );
    let edge = rgba(&mut fill, 0, 50);
    assert!(
        edge.3 == 255 && (edge.0 as i32 - 0x24).abs() <= 3 && (edge.2 as i32 - 0xac).abs() <= 3,
        "the fill is whole at the window's edge: {edge:?}"
    );
    let mut shadow = boot(
        r##"component App
  view
    box width=280 height=90 margin-left=-300 margin-top=5 background-color="#ff0000" filter="drop-shadow(60px 0px 0px #000000)"
"##,
        (100., 100.),
    );
    assert_eq!(
        rgba(&mut shadow, 10, 50),
        (0, 0, 0, 255),
        "the shadow of the box left of the window"
    );
    assert_eq!(
        rgba(&mut shadow, 60, 50).0,
        255,
        "and only as far as it falls"
    );
}
