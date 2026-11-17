//! LLP 1057.001.000: a zoomable scroll container's rows, its `scrollZoom`
//! prop and `ScrollEvent`'s zoom, and the refusals of D4.

use exact_kernel::{Kernel, PropId, PropValue};
use exact_runner::{DataError, DataSource, Event, Runner, ScrollEvent, Value};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

fn app(body: &str) -> String {
    let body = body
        .lines()
        .map(|l| format!("    {l}\n"))
        .collect::<String>();
    format!(
        "component App\n  state zoom = 2\n  state heard = 0\n  action moved(x: number, y: number, e: ScrollEvent)\n    heard = e.scrollZoom\n  view\n{body}"
    )
}

fn refusal(body: &str) -> String {
    contract::compile(&app(body))
        .err()
        .unwrap_or_else(|| panic!("{body} compiled"))
        .id
        .to_string()
}

#[test]
fn a_zoomable_scroll_carries_its_range_and_zoom() {
    let source = app(
        "scroll height=400 testId=\"port\" -exact-min-zoom=\"50%\" -exact-max-zoom=4 scrollZoom=zoom scroll=moved\n  box width=600 height=600",
    );
    let plan = contract::compile(&source).unwrap_or_else(|e| panic!("{e}"));
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    let node = k.node_by_key(k.find_by_test_id("port")[0]).unwrap();
    assert_eq!(
        (node.style.rare.min_zoom, node.style.rare.max_zoom),
        (0.5, 4.0)
    );
    assert_eq!(
        node.props.get(PropId::ScrollZoom),
        Some(&PropValue::Float(2.0))
    );
    let view = node.id;
    let event = ScrollEvent {
        width: 1200.0,
        height: 1200.0,
        client_width: 600.0,
        client_height: 600.0,
        zoom: 2.5,
        ..Default::default()
    };
    r.dispatch(view, Event::Scroll(event)).unwrap();
    assert_eq!(r.slot("heard"), Some(&Value::Number(2.5)));
}

#[test]
fn a_host_that_does_not_zoom_reports_one() {
    let Some(Event::Scroll(e)) = Event::scroll_payload("0,0,600,600,600,600") else {
        panic!()
    };
    assert_eq!(e.zoom, 1.0);
    let Some(Event::Scroll(e)) = Event::scroll_payload("0,0,600,600,600,600,1.5") else {
        panic!()
    };
    assert_eq!(e.zoom, 1.5);
    assert!(Event::scroll_payload("0,0,600,600,600,600,0").is_none());
}

#[test]
fn the_rows_need_one_child_in_a_container_that_scrolls_both_ways() {
    // A `scroll` scrolls both ways unless a row says otherwise; a box says so.
    contract::compile(&app(
        "box height=400 overflow=\"auto\" -exact-max-zoom=3\n  box width=10 height=10",
    ))
    .unwrap();
    contract::compile(&app(
        "scroll height=400 -exact-max-zoom=3\n  if zoom > 1\n    box width=10 height=10",
    ))
    .unwrap();
    contract::compile(&app(
        "scroll height=400 -exact-max-zoom=3 -exact-min-zoom=3\n  box width=10 height=10",
    ))
    .unwrap();
    for (body, id) in [
        ("scroll height=400 overflow-x=\"hidden\" -exact-max-zoom=3\n  box width=10", "lower-zoom-overflow"),
        ("box height=400 -exact-max-zoom=3\n  box width=10", "lower-zoom-overflow"),
        ("scroll height=400 -exact-max-zoom=3\n  box width=10\n  box width=10", "lower-zoom-child"),
        ("scroll height=400 -exact-max-zoom=3\n  for n in [1, 2] key=n\n    box width=10", "lower-zoom-child"),
        ("list height=400 -exact-max-zoom=3\n  box width=10", "lower-zoom-child"),
        ("scroll height=400 -exact-max-zoom=2 -exact-min-zoom=3\n  box width=10", "lower-zoom-range"),
        ("scroll height=400 -exact-max-zoom=0\n  box width=10", "lower-attr-value"),
        ("scroll height=400 -exact-max-zoom=2 display=\"flex\" align-items=\"center\"\n  box width=10", "lower-zoom-align"),
        ("scroll height=400 -exact-max-zoom=2\n  box width=10 margin=\"0 auto\"", "lower-zoom-align"),
        ("scroll height=400 -exact-max-zoom=2\n  if zoom > 1\n    box align-self=\"center\"", "lower-zoom-align"),
    ] {
        assert_eq!(refusal(body), id, "{body}");
    }
}
