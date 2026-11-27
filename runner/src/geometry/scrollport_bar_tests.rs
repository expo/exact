use super::*;
use crate::Runner;
use exact_kernel::Offer;

/// A page holding `outer`, a scroller scrolled `outer_top` down, which holds a 100-point box
/// and then `inner`, a scroller scrolled `inner_top` down; `inner` holds a 50-point top bar on
/// its scrollport, `bar`, styled `bar_style`, with a 60-by-40 `button`, then its content: `row`,
/// 100 points, and a 1000-point box.
fn scene(bar_style: &str, outer_top: f64, inner_top: f64) -> (Runner<()>, Scrolled) {
    scene_in(
        r#"box id="inner" width=400 height=300 overflow-y="scroll""#,
        bar_style,
        outer_top,
        inner_top,
    )
}

/// [`scene`] with `inner` written as `inner`.
fn scene_in(
    inner: &str,
    bar_style: &str,
    outer_top: f64,
    inner_top: f64,
) -> (Runner<()>, Scrolled) {
    let source = format!(
        r#"component App
  view
    box id="page" width=400 height=600
      box id="outer" width=400 height=400 overflow-y="scroll"
        box id="above" width=400 height=100
        {inner}
          box id="bar" -exact-safe-area-bar="top" height=50 {bar_style}
            box id="button" width=60 height=40 pointer-events="auto"
          box id="row" width=400 height=100
          box id="tall" width=400 height=1000
        box id="after" width=400 height=1000
"#
    );
    let mut r = Runner::boot(
        contract::compile(&source).unwrap(),
        (),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.kernel_mut().set_scrollport_bars(true);
    let root = r.roots()[0];
    r.kernel_mut()
        .compute_layout(root, Offer::definite(400.0, 600.0))
        .unwrap();
    let mut scrolled = Scrolled::default();
    for (id, top) in [("outer", outer_top), ("inner", inner_top)] {
        scrolled.note(r.kernel(), view_of(r.kernel(), id), 0.0, top);
    }
    (r, scrolled)
}

fn shown(r: &Runner<()>, scrolled: &Scrolled, id: &str) -> (f64, f64, f64, f64) {
    let f = kernel_frame(r.kernel(), scrolled, view_of(r.kernel(), id).unwrap());
    (f.x, f.y, f.width, f.height)
}

fn hits(r: &Runner<()>, scrolled: &Scrolled, x: f64, y: f64) -> Option<Value> {
    let env = GeometryEnv::new(r.kernel(), scrolled, &KERNEL, &[]);
    env.read(
        r.plan(),
        Stdlib::ElementsFromPoint,
        &[Value::Number(x), Value::Number(y)],
    )
}

fn ids(ids: &[&str]) -> Option<Value> {
    Some(Value::list(ids.iter().map(|id| Value::str(id)).collect()))
}

#[test]
fn a_scrollport_bar_moves_with_its_scrollers_box_and_not_with_its_content() {
    // `inner` lies 100 - 30 = 70 down: its bar there, whatever `inner` has scrolled, and its row
    // 20 above it, scrolled with its content
    let (r, scrolled) = scene("", 30.0, 20.0);
    assert_eq!(shown(&r, &scrolled, "bar"), (0.0, 70.0, 400.0, 50.0));
    assert_eq!(shown(&r, &scrolled, "button"), (0.0, 70.0, 60.0, 40.0));
    assert_eq!(shown(&r, &scrolled, "row"), (0.0, 50.0, 400.0, 100.0));
}

#[test]
fn a_point_meets_a_scrollport_bar_before_the_content_it_lies_over() {
    let (r, scrolled) = scene("", 30.0, 20.0);
    let content = ["row", "inner", "outer", "page"];
    assert_eq!(
        hits(&r, &scrolled, 10.0, 80.0),
        ids(&[&["button", "bar"][..], &content].concat())
    );
    assert_eq!(
        hits(&r, &scrolled, 200.0, 80.0),
        ids(&[&["bar"][..], &content].concat())
    );
    // A bar that takes no pointer passes it to the content beneath, but for its button
    let (r, scrolled) = scene("pointer-events=\"none\"", 30.0, 20.0);
    assert_eq!(
        hits(&r, &scrolled, 10.0, 80.0),
        ids(&[&["button"][..], &content].concat())
    );
    assert_eq!(hits(&r, &scrolled, 200.0, 80.0), ids(&content));
}

#[test]
fn a_scrollport_bars_intersection_root_is_the_scroller_above_its_own() {
    // `outer` has scrolled `inner` 20 above its port: the button lies half inside it, and its
    // own scroller, which it does not scroll in, only clips it
    let (r, scrolled) = scene("", 120.0, 20.0);
    let button = view_of(r.kernel(), "button").unwrap();
    assert_eq!(shown(&r, &scrolled, "button"), (0.0, -20.0, 60.0, 40.0));
    let (root, _, visible, ratio) = intersection(r.kernel(), &scrolled, button);
    let outer = view_of(r.kernel(), "outer").unwrap();
    assert_eq!(
        root,
        Some(IntersectionRoot::Element(
            r.kernel().arena().key_of(outer).unwrap()
        ))
    );
    assert!(visible);
    assert_eq!(ratio, 0.5);
}

#[test]
fn a_scroll_that_says_no_overflow_clips_its_scrollport_bar() {
    // `inner` is a 200-point-wide `scroll` that says no `overflow-y`, and its 300-point bar runs
    // past its right edge: the root is `outer`, which shows the whole bar, and `inner`, the
    // scroller it is, clips the bar to two thirds
    let (r, scrolled) = scene_in(
        r#"scroll id="inner" width=200 height=300"#,
        "width=300",
        0.0,
        0.0,
    );
    let bar = view_of(r.kernel(), "bar").unwrap();
    let (_, _, visible, ratio) = intersection(r.kernel(), &scrolled, bar);
    assert!(visible);
    assert!((ratio - 2.0 / 3.0).abs() < 1e-6, "{ratio}");
}
