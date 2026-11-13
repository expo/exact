use super::*;
use crate::Runner;
use exact_kernel::{BlockAxis, HostBlock, Offer};

fn fixture(panel: &str, route: &str) -> (Runner<()>, Scrolled) {
    scene(panel, route, "", "")
}

fn scene(panel: &str, route: &str, before: &str, after: &str) -> (Runner<()>, Scrolled) {
    let source = format!(
        r#"component App
  view
    box id="page" width=400 height=400
      box id="owner" position="absolute" left=20 top=30 width=200 height=200 overflow="clip"
{before}        box id="panel" role="tabpanel" width=200 height=100 overflow="hidden" {panel}
          box id="stack" width=200 height=80 overflow="clip"
            box id="route" width=200 height=250 {route}
              box id="target" position="absolute" left=20 top=150 width=40 height=100
            box id="ordinary" position="absolute" left=100 top=150 width=40 height=100
{after}
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
    let owner = view_of(r.kernel(), "owner").unwrap();
    let mut scrolled = Scrolled::default();
    for (id, height) in [("panel", 100.0), ("route", 250.0)] {
        let view = view_of(r.kernel(), id).unwrap();
        r.kernel_mut()
            .set_host_block(
                view,
                Some(HostBlock {
                    width: BlockAxis::Is(200.0),
                    height: BlockAxis::Is(height),
                }),
            )
            .unwrap();
        scrolled.place(
            r.kernel(),
            view,
            Some(([0.0, 0.0, 200.0, f64::from(height)], owner, None)),
        );
    }
    let root = r.roots()[0];
    r.kernel_mut()
        .compute_layout(root, Offer::definite(400.0, 400.0))
        .unwrap();
    (r, scrolled)
}

fn read(r: &Runner<()>, scrolled: &Scrolled, x: f64, y: f64, expected: &[&str]) {
    let env = GeometryEnv::new(r.kernel(), scrolled, &KERNEL, &[]);
    let args = [Value::Number(x), Value::Number(y)];
    assert_eq!(
        env.read(r.plan(), Stdlib::ElementFromPoint, &args),
        Some(
            expected
                .first()
                .map_or(Value::NONE, |id| Value::some(Value::str(id)))
        ),
        "elementFromPoint at ({x}, {y})"
    );
    assert_eq!(
        env.read(r.plan(), Stdlib::ElementsFromPoint, &args),
        Some(Value::list(
            expected.iter().map(|id| Value::str(id)).collect()
        )),
        "elementsFromPoint at ({x}, {y})"
    );
}

#[test]
fn a_placed_route_escapes_intervening_clips_but_keeps_its_holders_clip() {
    for panel in ["", "translate=\"500px 0\" rotate=90 scale=0"] {
        let (r, scrolled) = fixture(panel, "");
        let target = view_of(r.kernel(), "target").unwrap();
        let ordinary = view_of(r.kernel(), "ordinary").unwrap();
        let frame = kernel_frame(r.kernel(), &scrolled, target);
        assert_eq!(
            (frame.x, frame.y, frame.width, frame.height),
            (40.0, 180.0, 40.0, 100.0)
        );
        let (_, _, visible, ratio) = intersection(r.kernel(), &scrolled, target);
        assert!(visible);
        assert_eq!(ratio, 0.5);
        assert_eq!(intersection(r.kernel(), &scrolled, ordinary).3, 0.0);
        read(
            &r,
            &scrolled,
            50.0,
            190.0,
            &["target", "route", "owner", "page"],
        );
        read(&r, &scrolled, 130.0, 190.0, &["route", "owner", "page"]);
        read(&r, &scrolled, 50.0, 240.0, &["page"]);
    }
}

#[test]
fn a_placed_route_keeps_logical_inheritance_and_subtree_suppression() {
    for (panel, route, shown) in [
        ("pointer-events=\"none\"", "", false),
        ("visibility=\"hidden\"", "", false),
        ("pointer-events=\"none\"", "pointer-events=\"auto\"", true),
        ("visibility=\"hidden\"", "visibility=\"visible\"", true),
        (
            "inert=true",
            "pointer-events=\"auto\" visibility=\"visible\"",
            false,
        ),
        ("display=\"none\"", "", false),
    ] {
        let (r, scrolled) = fixture(panel, route);
        if panel == "display=\"none\"" {
            let target = r.kernel().find_by_id("target")[0];
            assert!(r
                .kernel()
                .arena()
                .flags(target.index)
                .has(NodeFlags::HIDDEN));
        }
        read(
            &r,
            &scrolled,
            50.0,
            190.0,
            if shown {
                &["target", "route", "owner", "page"]
            } else {
                &["owner", "page"]
            },
        );
    }
}

#[test]
fn a_placed_route_keeps_its_logical_paint_order() {
    let before = "        box id=\"back\" position=\"absolute\" left=20 top=150 width=40 height=100 z-index=-1\n";
    let after = "        box id=\"toast\" position=\"absolute\" left=20 top=150 width=40 height=100 z-index=1\n";
    for (panel, expected) in [
        (
            "z-index=0",
            vec!["toast", "target", "route", "back", "owner", "page"],
        ),
        (
            "z-index=2",
            vec!["target", "route", "toast", "back", "owner", "page"],
        ),
    ] {
        let (r, scrolled) = scene(panel, "", before, after);
        read(&r, &scrolled, 50.0, 190.0, &expected);
    }
}

#[test]
fn a_top_layer_route_replaces_its_ordinary_placement() {
    let (r, mut scrolled) = fixture("", "");
    let route = view_of(r.kernel(), "route").unwrap();
    let target = view_of(r.kernel(), "target").unwrap();
    let rect = [100.0, 100.0, 200.0, 250.0];
    scrolled.top_layer_frames(r.kernel(), &[(route, rect, None)], None);
    let frame = kernel_frame(r.kernel(), &scrolled, target);
    assert_eq!((frame.x, frame.y), (120.0, 250.0));
    assert_eq!(intersection(r.kernel(), &scrolled, target).3, 1.0);
    read(&r, &scrolled, 130.0, 270.0, &["target", "route", "page"]);
    scrolled.top_layer_frames(r.kernel(), &[(route, rect, None)], Some(0));
    read(&r, &scrolled, 130.0, 270.0, &["target", "route"]);
}

#[test]
fn an_allocation_without_a_block_clips_without_moving_or_resizing_its_subtree() {
    let (r, mut scrolled) = fixture("", "");
    let owner = view_of(r.kernel(), "owner").unwrap();
    let page = view_of(r.kernel(), "page").unwrap();
    let target = view_of(r.kernel(), "target").unwrap();
    for (sx, sy) in [(0., 0.), (10., 15.)] {
        scrolled.note(r.kernel(), Some(page), sx, sy);
        scrolled.place(
            r.kernel(),
            owner,
            Some(([20. - sx, 30. - sy, 40., 200.], page, None)),
        );
        let frame = kernel_frame(r.kernel(), &scrolled, target);
        assert_eq!(
            (frame.x, frame.y, frame.width, frame.height),
            (40. - sx, 180. - sy, 40., 100.)
        );
        assert_eq!(intersection(r.kernel(), &scrolled, target).3, 0.25);
        read(
            &r,
            &scrolled,
            50. - sx,
            190. - sy,
            &["target", "route", "owner", "page"],
        );
        read(&r, &scrolled, 70. - sx, 190. - sy, &["page"]);
        scrolled.place(r.kernel(), owner, None);
        assert_eq!(intersection(r.kernel(), &scrolled, target).3, 0.5);
        read(
            &r,
            &scrolled,
            70. - sx,
            190. - sy,
            &["target", "route", "owner", "page"],
        );
    }
}

#[test]
fn a_released_block_does_not_leave_a_clip_on_its_ordinary_subtree() {
    let (mut r, mut scrolled) = fixture("", "");
    let route = view_of(r.kernel(), "route").unwrap();
    let stack = view_of(r.kernel(), "stack").unwrap();
    scrolled.place(r.kernel(), route, Some(([0., 0., 40., 80.], stack, None)));
    let slot = r.kernel().arena().slot_of(route).unwrap();
    assert!(scrolled.placed_block(r.kernel(), slot).is_some());
    assert!(block_clip(
        r.kernel(),
        &scrolled,
        slot,
        r.kernel().node(route).unwrap().frame
    )
    .is_some());
    r.kernel_mut().set_host_block(route, None).unwrap();
    assert!(scrolled.placed_block(r.kernel(), slot).is_none());
    assert!(block_clip(
        r.kernel(),
        &scrolled,
        slot,
        r.kernel().node(route).unwrap().frame
    )
    .is_none());
}

#[test]
fn an_allocation_without_a_block_keeps_agent_absolute_in_layout_space() {
    let source = r#"component App
  view
    box id="page" width=600 height=400
      box id="list" width=600 height=400 overflow-y="auto" translate="-100px 0"
        box id="row" position="absolute" left=400 top=180 width=100 height=100 translate="0 20px"
"#;
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        (),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let root = r.roots()[0];
    r.kernel_mut()
        .compute_layout(root, Offer::definite(600., 400.))
        .unwrap();
    let page = view_of(r.kernel(), "page").unwrap();
    let list = view_of(r.kernel(), "list").unwrap();
    let row = view_of(r.kernel(), "row").unwrap();
    let mut scrolled = Scrolled::default();
    scrolled.note(r.kernel(), Some(list), 0., 100.);
    for placement in [None, Some(([0., 0., 320., 400.], page, None)), None] {
        scrolled.place(r.kernel(), list, placement);
        let frame = r.kernel().node(row).unwrap().frame;
        let shown = kernel_frame(r.kernel(), &scrolled, row);
        let absolute = scrolled.agent_frame(r.kernel(), row, frame);
        assert_eq!((shown.x, shown.y), (300., 100.));
        assert_eq!((absolute.x, absolute.y), (400., 180.));
        let (_, _, inside, ratio) = intersection(r.kernel(), &scrolled, row);
        assert!(inside);
        assert_eq!(ratio, if placement.is_some() { 0.2 } else { 1. });
    }
}
