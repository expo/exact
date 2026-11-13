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
