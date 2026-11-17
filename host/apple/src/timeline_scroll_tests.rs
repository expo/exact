//! LLP 1057.003 D5 on Apple: a scroller a timeline first reads after it has
//! scrolled, its own or the page, holds the consumer where the scroller
//! stands, not at its start. The presenter notes every scroll
//! (`exact_scrolled`), marked or not; placement reads that.
use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_motion::Property;
use exact_plan::Value;
use exact_runner::{DataError, Event};

#[derive(Clone, Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

// Once each consumer has mounted, the list scrolls 304 (404 in 100) and the
// page 1004 (1848 in 844).
const APP: &str = r##"keyframes fade
  from opacity=0
  to opacity=1
component App
  state shown = false
  action show
    shown = true
  view
    column width=390
      button press=show testId="show" height=44 flex-shrink=0
        text "Show"
      column testId="list" overflow-y="auto" height=100 flex-shrink=0
        view height=400 flex-shrink=0
        if shown
          view testId="inner" height=4 flex-shrink=0 opacity=0.25 animation="fade 1s linear both" animation-timeline="scroll()"
      view height=1700 flex-shrink=0
      if shown
        view testId="paged" height=4 flex-shrink=0 opacity=0.25 animation="fade 1s linear both" animation-timeline="scroll(root)"
"##;

fn id(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

fn opacity(host: &Host<NoData>, test_id: &str) -> f64 {
    let k = host.runner.kernel();
    let node = exact_kernel::motion_node(k.find_by_test_id(test_id)[0]);
    host.engine
        .sampled_value(node, Property::Opacity)
        .unwrap_or_else(|| panic!("`{test_id}` shows no opacity"))
        .x
}

#[test]
fn a_consumer_mounted_after_its_scroller_scrolled_starts_where_it_stands() {
    let plan = contract::compile(APP).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    // Scrolled while no timeline reads either: the presenter notes them.
    let list = id(&host, "list");
    host.runner_mut().scrolled(Some(list), 0.0, 150.0);
    host.runner_mut().scrolled(None, 0.0, 500.0);
    let batch = host.dispatch_at(id(&host, "show"), Event::Press, 100.0);
    assert!(batch.contains("scrollTimeline"), "{batch}");
    assert!(
        (opacity(&host, "inner") - 150.0 / 304.0).abs() < 1e-9,
        "{}",
        opacity(&host, "inner")
    );
    assert!(
        (opacity(&host, "paged") - 500.0 / 1004.0).abs() < 1e-9,
        "{}",
        opacity(&host, "paged")
    );
    // From here the scroll callback moves them.
    let _ = host.timeline_scrolled(Some(list), 0.0, 304.0);
    let _ = host.timeline_scrolled(None, 0.0, 1004.0);
    assert_eq!(
        (opacity(&host, "inner"), opacity(&host, "paged")),
        (1.0, 1.0)
    );
}
