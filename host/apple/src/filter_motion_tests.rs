//! LLP 1003.002 on Apple: a `filter` transition is sampled, each frame
//! re-sending the box's style with the presented chain, and what is under a
//! box whose filter runs to `none` stays sampled while the picture shows.
use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::{DataError, Event};

#[derive(Clone, Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const APP: &str = r##"keyframes pulse
  from opacity=1
  to opacity=0.2
component A
  state blurred = true
  action go
    blurred = not blurred
  view
    column
      button press=go testId="go"
        text "Go"
      box testId="frame" width=60 height=60 filter=(blurred ? "blur(8px)" : "none") transition="filter 1s linear"
        box testId="square" width=40 height=40 background-color="#ff0000" animation=(blurred ? "none" : "pulse 1s linear infinite alternate")
"##;

fn id(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// The `filter` entry of the last style op for `view` in a batch: `Some(None)`
/// for a style op without one, `None` for no style op.
fn filter_of(batch: &str, view: u32) -> Option<Option<serde_json::Value>> {
    let v: serde_json::Value = serde_json::from_str(batch).unwrap();
    v["ops"]
        .as_array()?
        .iter()
        .rev()
        .find(|op| op["op"] == "style" && op["id"] == view)
        .map(|op| op["style"].get("filter").cloned())
}

fn boot() -> Host<NoData> {
    let plan = contract::compile(APP).unwrap().encode();
    Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap()
    .0
}

#[test]
fn each_frame_of_a_filter_transition_sends_the_presented_chain_and_the_end_the_row() {
    let mut host = boot();
    let frame = id(&host, "frame");
    host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    let mid = host.tick(600.0);
    let program = filter_of(&mid, frame)
        .expect("a style op")
        .expect("a presented chain");
    assert!(program["p"].is_array(), "{program}");
    let later = host.tick(700.0);
    let next = filter_of(&later, frame)
        .flatten()
        .expect("a presented chain");
    assert_ne!(program, next, "the chain moves frame by frame");
    let end = host.tick(1200.0);
    assert_eq!(filter_of(&end, frame), Some(None), "the row, `none`, shows");
    assert_eq!(
        host.engine
            .presented_filter(exact_kernel::motion::motion_node(
                host.runner.kernel().node(frame).unwrap().key
            )),
        None
    );
}

#[test]
fn what_is_under_a_filter_running_to_none_is_sampled() {
    for ios in [false, true] {
        let mut host = boot();
        if ios {
            host.svg.box_motion = true;
            host.engine.set_lowered_properties(&svg::lowered(true));
        }
        let square = id(&host, "square");
        let key = exact_kernel::motion::motion_node(host.runner.kernel().node(square).unwrap().key);
        // The commit that clears the row starts the animation under it: the
        // picture still shows, so Core Animation must not play it.
        host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
        assert!(host.engine.node_sampled(key), "ios {ios}");
    }
}
