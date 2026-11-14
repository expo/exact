//! The layout regions as the Apple host sets them (LLP 1123 D10): a node
//! that reads one is re-sent and laid out again when the host reports them.

use super::Host;
use exact_kernel::MonospaceMeasurer;
use exact_runner::{DataError, DataSource};

struct NoData;
impl DataSource for NoData {
    fn query(
        &mut self,
        name: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

/// The x of the child inside `test_id`: its parent's left padding.
fn padding_left(h: &Host<NoData>, test_id: &str) -> f32 {
    let kernel = h.runner().kernel();
    let child = kernel
        .node_by_key(kernel.find_by_test_id(&format!("{test_id}-child"))[0])
        .unwrap();
    child.frame.x
}

#[test]
fn the_host_sets_the_regions_and_the_readers_lay_out_again() {
    let source = r#"component App
  view
    column width="100%"
      column testId="margins" width="100%" padding-left="env(-exact-layout-margins-inset-left)"
        column testId="margins-child" height=10
      column testId="plain" width="100%" padding-left=4
        column testId="plain-child" height=10
"#;
    let plan = contract::compile(source).unwrap().encode();
    let (mut h, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.,
        800.,
    )
    .unwrap();
    // No regions: the safe area's
    assert!(!h.set_insets(0., 0., 34., 6.).is_empty());
    assert_eq!(padding_left(&h, "margins"), 6.);
    // The host's: margins (region 1) left is 20
    let mut flat = [0f32; 20];
    flat[4 + 3] = 20.;
    let regions = Host::<NoData>::regions_from(&flat);
    assert_eq!(regions.map(|r| r[1][3]), Some(20.));
    assert!(
        !h.set_regions(regions).is_empty(),
        "a batch moves the reader"
    );
    assert_eq!(padding_left(&h, "margins"), 20.);
    assert_eq!(padding_left(&h, "plain"), 4.);
    assert_eq!(h.runner().kernel().env().regions, regions);
    // None again: the safe area's
    assert!(!h.set_regions(None).is_empty());
    assert_eq!(padding_left(&h, "margins"), 6.);
    assert_eq!(
        Host::<NoData>::regions_from(&flat[..19]),
        None,
        "only twenty is a report"
    );
}

/// The window, the insets, the regions and the viewport told at once leave
/// the page as the four calls one by one do, in one batch.
#[test]
fn the_view_told_at_once_is_laid_out_as_told_one_by_one() {
    let source = r#"component App
  view
    column width="100%" height="100%"
      column testId="margins" width="50vw" padding-left="env(-exact-layout-margins-inset-left)" padding-top="env(safe-area-inset-top)"
        column testId="margins-child" height=10
      column testId="rest" flex=1
"#;
    let plan = contract::compile(source).unwrap().encode();
    let boot = || {
        let measurer = Box::new(MonospaceMeasurer::default());
        Host::boot(&plan, NoData, measurer, 400., 700.).unwrap().0
    };
    let mut flat = [0f32; 20];
    flat[4 + 3] = 20.;
    let regions = Host::<NoData>::regions_from(&flat);
    let frames = |h: &Host<NoData>| {
        let kernel = h.runner().kernel();
        ["margins", "margins-child", "rest"].map(|t| {
            let f = kernel
                .node_by_key(kernel.find_by_test_id(t)[0])
                .unwrap()
                .frame;
            (f.x, f.y, f.width, f.height)
        })
    };
    let mut apart = boot();
    apart.set_screen(Some((402., 874.)));
    apart.set_insets(62., 0., 34., 0.);
    apart.set_regions(regions);
    apart.resize(402., 874.);
    let mut together = boot();
    let batch = together.set_view(
        Some(Some((402., 874.))),
        Some([62., 0., 34., 0.]),
        Some(regions),
        Some((402., 874.)),
        None,
        None,
        &[],
        &[],
    );
    assert!(!batch.contains("\"error\":\""), "{batch}");
    assert_eq!(frames(&together), frames(&apart));
    assert_eq!(
        together.runner().kernel().env(),
        apart.runner().kernel().env()
    );
    assert_eq!(padding_left(&together, "margins"), 20.);
    // Only what is given changes: the insets alone.
    together.set_view(
        None,
        Some([0., 0., 0., 0.]),
        None,
        None,
        None,
        None,
        &[],
        &[],
    );
    apart.set_insets(0., 0., 0., 0.);
    assert_eq!(frames(&together), frames(&apart));
    // A keyboard folded like a laptop: the viewport and the lower segment's
    // extent in one batch, as the two calls leave them; a rect off the
    // kernel's grid is refused and changes nothing.
    let stacked = |h: f32| {
        vec![
            exact_kernel::Rect::new(0., 0., 402., 400.),
            exact_kernel::Rect::new(0., 440., 402., h),
        ]
    };
    apart.set_segments(1, 1, 2, stacked(434.));
    together.set_segments(1, 1, 2, stacked(434.));
    apart.resize(402., 455.);
    apart.set_segments(1, 1, 2, stacked(15.));
    let batch = together.set_view(
        None,
        None,
        None,
        Some((402., 455.)),
        Some(stacked(15.)),
        None,
        &[],
        &[],
    );
    assert!(!batch.contains("\"error\":\""), "{batch}");
    assert_eq!(frames(&together), frames(&apart));
    assert_eq!(
        together.runner().kernel().env(),
        apart.runner().kernel().env()
    );
    let refused = together.set_view(
        None,
        None,
        None,
        None,
        Some(stacked(15.)[..1].to_vec()),
        None,
        &[],
        &[],
    );
    assert!(refused.contains("segments off the grid"), "{refused}");
    assert_eq!(
        together.runner().kernel().env(),
        apart.runner().kernel().env()
    );
}

#[test]
fn numeric_safe_area_and_css_environment_move_together_and_rollback() {
    let source = r#"shape Viewport
  safeAreaLeft: number
component App
  resource viewport = exactViewport() as shape Viewport
  derive quotient = 1 / (24 - viewport.safeAreaLeft)
  view
    column
      column testId="margins" width="100%" padding-left="env(safe-area-inset-left)"
        column testId="margins-child" height=10
      column testId="numeric" width=viewport.safeAreaLeft height=10
      text `${quotient}`
"#;
    let plan = contract::compile(source).unwrap().encode();
    let (mut h, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.,
        800.,
    )
    .unwrap();
    let width = |h: &Host<NoData>| {
        let k = h.runner().kernel();
        k.node_by_key(k.find_by_test_id("numeric")[0])
            .unwrap()
            .frame
            .width
    };
    h.set_insets(0., 0., 0., 12.);
    assert_eq!(padding_left(&h, "margins"), 12.);
    assert_eq!(width(&h), 12.);
    let refused = h.set_insets(0., 0., 0., 24.);
    assert!(refused.contains("viewport geometry"), "{refused}");
    assert_eq!(padding_left(&h, "margins"), 12.);
    assert_eq!(width(&h), 12.);
    let refused = h.set_view(
        None,
        Some([0., 0., 0., 24.]),
        None,
        Some((500., 900.)),
        None,
        None,
        &[],
        &[],
    );
    assert!(refused.contains("viewport:"), "{refused}");
    assert_eq!(padding_left(&h, "margins"), 12.);
    assert_eq!(width(&h), 12.);
    let before = h.runner().kernel().env();
    let viewport = h.runner().viewport();
    for (insets, size) in [
        (Some([0., 0., 0., 18.]), Some((f32::NAN, 900.))),
        (Some([f32::NAN, 0., 0., 18.]), Some((500., 900.))),
    ] {
        let refused = h.set_view(None, insets, None, size, None, None, &[], &[]);
        assert!(refused.contains("error"), "{refused}");
        assert_eq!(h.runner().kernel().env(), before);
        assert_eq!(h.runner().viewport(), viewport);
    }
    h.set_view(
        None,
        Some([0., 0., 0., 16.]),
        None,
        None,
        None,
        None,
        &[],
        &[],
    );
    assert_eq!(padding_left(&h, "margins"), 16.);
    assert_eq!(width(&h), 16.);
}

/// LLP 1039 D9: the keyboard's overlap rides the view's change: with the
/// viewport it shrinks in one commit, or alone; every reader sees it, and
/// a negative one is refused, changing nothing.
#[test]
fn the_keyboard_rides_the_view() {
    let source = "shape V\n  height: number\n  keyboardHeight: number\ncomponent App\n  resource v = exactViewport() as shape V\n  view\n    column width=\"100%\" height=100\n";
    let plan = contract::compile(source).unwrap().encode();
    let measurer = Box::new(MonospaceMeasurer::default());
    let (mut h, _) = Host::boot(&plan, NoData, measurer, 402., 874.).unwrap();
    let read = |h: &Host<NoData>| {
        let state: serde_json::Value =
            serde_json::from_str(&exact_runner::agent::state(h.runner())).unwrap();
        state["resources"]["v"].clone()
    };
    let batch = h.set_view(
        None,
        None,
        None,
        Some((402., 538.)),
        None,
        Some(336.),
        &[],
        &[],
    );
    assert!(!batch.contains("\"error\":\""), "{batch}");
    assert_eq!(
        read(&h),
        serde_json::json!({"height": 538, "keyboardHeight": 336})
    );
    let logs = exact_runner::agent::logs(h.runner(), 0);
    assert_eq!(
        logs.matches("viewport (1 asked again)").count(),
        1,
        "{logs}"
    );
    h.set_view(None, None, None, None, None, Some(0.), &[], &[]);
    assert_eq!(read(&h)["keyboardHeight"], 0);
    let refused = h.set_view(None, None, None, None, None, Some(-1.), &[], &[]);
    assert!(refused.contains("viewport:"), "{refused}");
    assert_eq!(read(&h)["keyboardHeight"], 0);
}
