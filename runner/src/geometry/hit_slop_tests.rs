//! `-exact-hit-slop` in the hit walk (Charlie, 2026-11-10): a box takes a
//! point past its edge without moving, the front box still wins where boxes
//! overlap, and a clip above it or on it cuts the slop.
use super::*;
use crate::{DataError, DataSource, Runner};
use exact_kernel::Offer;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

fn boot(body: &str) -> Runner<NoData> {
    let source = format!(
        "component App\n  view\n    column id=\"page\" position=\"relative\" width=200 height=200\n{body}"
    );
    let mut r = Runner::boot(
        contract::compile(&source).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let root = r.roots()[0];
    r.kernel_mut()
        .compute_layout(root, Offer::definite(200., 200.))
        .unwrap();
    r
}

fn hit(r: &Runner<NoData>, x: f64, y: f64) -> Value {
    let scrolled = Scrolled::default();
    GeometryEnv::new(r.kernel(), &scrolled, &KERNEL, &[])
        .read(
            r.plan(),
            exact_plan::Stdlib::ElementFromPoint,
            &[Value::Number(x), Value::Number(y)],
        )
        .unwrap()
}

fn at(id: &str) -> Value {
    Value::some(Value::str(id))
}

#[test]
fn a_slop_takes_points_past_the_box_and_moves_nothing() {
    let body = |slop: &str| {
        format!(
            "      box id=\"small\" position=\"absolute\" left=40 top=40 width=20 height=20{slop}\n"
        )
    };
    let plain = boot(&body(""));
    let r = boot(&body(" -exact-hit-slop=\"10px 4px 0 6px\""));
    let frame = |r: &Runner<NoData>| {
        let view = view_of(r.kernel(), "small").unwrap();
        kernel_frame(r.kernel(), &Scrolled::default(), view)
    };
    assert_eq!(frame(&r), frame(&plain), "layout never sees the slop");
    for (x, y, id) in [
        (50.0, 31.0, "small"),
        (50.0, 29.0, "page"),
        (63.5, 50.0, "small"),
        (64.5, 50.0, "page"),
        (50.0, 60.5, "page"),
        (34.5, 50.0, "small"),
        (33.5, 50.0, "page"),
    ] {
        assert_eq!(hit(&r, x, y), at(id), "at {x}, {y}");
    }
    assert_eq!(hit(&plain, 50.0, 31.0), at("page"));
}

#[test]
fn the_front_box_wins_slop_and_all() {
    // `front` follows `back` in the tree, so it paints over it: its slop
    // takes the edge of `back`'s box, while `back`'s slop yields to
    // `front`'s box.
    let r = boot(
        "      box id=\"back\" position=\"absolute\" left=0 top=0 width=30 height=30 -exact-hit-slop=10\n      box id=\"front\" position=\"absolute\" left=30 top=0 width=30 height=30 -exact-hit-slop=10\n",
    );
    assert_eq!(hit(&r, 25.0, 15.0), at("front"));
    assert_eq!(hit(&r, 35.0, 15.0), at("front"));
    assert_eq!(hit(&r, 15.0, 15.0), at("back"));
    assert_eq!(hit(&r, 15.0, 35.0), at("back"));
}

#[test]
fn a_clip_cuts_the_slop() {
    // An `overflow` ancestor clips the slop at its padding box, and a box
    // that clips its own overflow clips its own slop, as the web's
    // pseudo-element is clipped.
    let r = boot(
        "      box id=\"clip\" position=\"absolute\" left=0 top=0 width=50 height=50 overflow=\"hidden\"\n        box id=\"inner\" position=\"absolute\" left=30 top=10 width=20 height=20 -exact-hit-slop=10\n      box id=\"own\" position=\"absolute\" left=100 top=100 width=20 height=20 overflow=\"hidden\" -exact-hit-slop=10\n",
    );
    assert_eq!(hit(&r, 25.0, 20.0), at("inner"));
    assert_eq!(hit(&r, 55.0, 20.0), at("page"));
    assert_eq!(hit(&r, 95.0, 110.0), at("page"));
    assert_eq!(hit(&r, 105.0, 110.0), at("own"));
}

#[test]
fn a_one_axis_clip_cuts_that_axis_only() {
    // CSS Overflow 3 §3.1: `clip` beside `visible` clips one axis, so the
    // child is hit below the box and not beside it.
    let r = boot(
        "      box id=\"clip\" position=\"absolute\" left=0 top=0 width=50 height=50 overflow-x=\"clip\"\n        box id=\"inner\" width=100 height=100 flex-shrink=0\n",
    );
    assert_eq!(hit(&r, 25.0, 25.0), at("inner"));
    assert_eq!(hit(&r, 25.0, 75.0), at("inner"));
    assert_eq!(hit(&r, 75.0, 25.0), at("page"));
}

#[test]
fn a_scroll_element_clips_its_children_at_its_scrollport() {
    // `scroll` scrolls on y with no overflow rows, so its child is not hit
    // below its box, as `elementFromPoint` misses it on the web.
    let r = boot(
        "      scroll id=\"scroller\" width=100 height=100\n        box id=\"child\" width=100 height=300 flex-shrink=0\n",
    );
    assert_eq!(hit(&r, 50.0, 50.0), at("child"));
    assert_eq!(hit(&r, 50.0, 150.0), at("page"));
}

#[test]
fn a_negative_or_percentage_slop_is_refused() {
    for value in ["-4px", "10%", "4px -2px"] {
        let source = format!(
            "component App\n  view\n    box width=20 height=20 -exact-hit-slop=\"{value}\"\n"
        );
        assert!(contract::compile(&source).is_err(), "`{value}` compiled");
    }
}

/// LLP 1123 D9: a slop that reads the safe area where the box is reaches as
/// far as its owner's inset, not the document's 0.
#[test]
fn a_slop_reads_the_boxes_own_inset() {
    let mut r = boot(
        "      box id=\"small\" position=\"absolute\" left=40 top=40 width=20 height=20 -exact-hit-slop-top=\"var(--exact-safe-area-inset-top)\"\n",
    );
    let page = r.roots()[0];
    let local = Some([12.0, 0.0, 0.0, 0.0]);
    r.kernel_mut().set_local_insets(page, local).unwrap();
    assert_eq!(hit(&r, 50.0, 30.0), at("small"), "12 above it");
    assert_eq!(hit(&r, 50.0, 27.0), at("page"), "and no further");
}
