//! The geometry facts on the Apple host (LLP 1123 D6): each layout of a batch is followed by
//! their delivery, and the host lays out the commit it makes before the batch is finished.
use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::DataError;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// `hero` is half the window, and `half` is as wide as `hero` last read.
const HALF: &str = r#"shape Rect
  x: number
  y: number
  width: number
  height: number
shape G
  frame: option<Rect>

component App
  resource g = exactGeometry("hero") as shape G
  derive width = match g.frame { case some(f) => f.width, case none => 0 }
  view
    column
      column id="hero" width="50%" height=40
      column testId="half" width=width height=10
"#;

#[test]
fn a_fact_follows_each_layout_the_window_makes() {
    let plan = contract::compile(HALF).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    let half = |host: &Host<NoData>| {
        let k = host.runner.kernel();
        k.node_by_key(k.find_by_test_id("half")[0])
            .unwrap()
            .frame
            .width
    };
    assert_eq!(half(&host), 195.0, "the boot's layout reaches it");
    host.resize(400.0, 844.0);
    assert_eq!(
        half(&host),
        200.0,
        "and a resize's, laid out in the same batch"
    );
}

/// `hero` widens at each press, and nothing on screen reads `g`.
const PRESSED: &str = r#"shape Rect
  x: number
  y: number
  width: number
  height: number
shape G
  frame: option<Rect>

component App
  state n = 0
  action grow
    n = n + 1
  resource g = exactGeometry("hero") as shape G
  view
    column
      button press=grow testId="grow" width=60 height=20
      column id="hero" width=(100 + n * 10) height=40
"#;

#[test]
fn a_press_after_a_delivery_that_moved_nothing_starts_a_chain() {
    // Each press lays `hero` out wider, and its fact's answer, which moves no node, commits
    // without a layout: the next press is another cause, so ten presses are ten chains
    let plan = contract::compile(PRESSED).unwrap().encode();
    let measurer = Box::new(MonospaceMeasurer::default());
    let (mut host, _) = Host::boot(&plan, NoData, measurer, 390.0, 844.0).unwrap();
    let id_of = |host: &Host<NoData>, test_id: &str| {
        let k = host.runner.kernel();
        k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
    };
    let grow = id_of(&host, "grow");
    for n in 1..=10 {
        host.dispatch_at(grow, exact_runner::Event::Press, 0.0);
        let state: serde_json::Value =
            serde_json::from_str(&exact_runner::agent::state(&host.runner)).unwrap();
        let width = &state["resources"]["g"]["frame"]["width"];
        assert_eq!(
            width.as_f64(),
            Some(100.0 + 10.0 * f64::from(n)),
            "press {n}: {state}"
        );
    }
}
