//! LLP 1003.003 on Apple: a box's `clip-path` transition, where the engine
//! samples it, re-sends the box's style each frame with the presented clip,
//! which the presenter draws and hit-tests as the box's own.
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

const APP: &str = r##"component A
  state grown = true
  action go
    grown = not grown
  view
    column
      button press=go testId="go"
        text "Go"
      box testId="balloon" width=120 height=48 background-color="#0088ff" clip-path=(grown ? "path('M0 0H120V40H0ZM104 40L120 40L120 48Z')" : "path('M0 0H120V40H0ZM104 40L120 40L120 40Z')") transition="clip-path 1s linear"
"##;

fn id(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// The `clip_path` entry of the last style op for `view` in a batch:
/// `Some(None)` for a style op without one, `None` for no style op.
fn clip_of(batch: &str, view: u32) -> Option<Option<serde_json::Value>> {
    let v: serde_json::Value = serde_json::from_str(batch).unwrap();
    v["ops"]
        .as_array()?
        .iter()
        .rev()
        .find(|op| op["op"] == "style" && op["id"] == view)
        .map(|op| op["style"].get("clip_path").cloned())
}

/// The tip's y in a sent clip: the last `L` of its commands.
fn tip(clip: &serde_json::Value) -> f64 {
    let commands = clip["commands"].as_array().expect("commands");
    let line = commands.iter().rev().find(|c| c[0] == "L").expect("a line");
    line[1][1].as_f64().unwrap()
}

#[test]
fn each_frame_of_a_clip_transition_sends_the_presented_clip_and_the_end_the_row() {
    let plan = contract::compile(APP).unwrap().encode();
    let mut host = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap()
    .0;
    let balloon = id(&host, "balloon");
    host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    let mid = host.tick(600.0);
    let shown = clip_of(&mid, balloon).expect("a style op").expect("a clip");
    let half = tip(&shown);
    assert!(
        half > 40.0 && half < 48.0,
        "the tail part way to its origin: {shown}"
    );
    let later = host.tick(800.0);
    let next = clip_of(&later, balloon).flatten().expect("a clip");
    assert!(tip(&next) < half, "the tail shrinks frame by frame");
    let end = host.tick(1200.0);
    let row = clip_of(&end, balloon).flatten().expect("the row's clip");
    assert_eq!(tip(&row), 40.0, "the row shows");
    assert_eq!(
        host.engine
            .presented_clip(exact_kernel::motion::motion_node(
                host.runner.kernel().node(balloon).unwrap().key
            )),
        None
    );
}

const HIDING: &str = r##"component A
  state grown = true
  state shown = true
  state red = true
  action go
    grown = not grown
    red = not red
  action hide
    shown = not shown
  view
    column
      button press=go testId="go"
        text "Go"
      button press=hide testId="hide"
        text "Hide"
      box testId="parent" display=(shown ? "flex" : "none") color=(red ? "#ff0000" : "#0000ff") transition="color 1s linear"
        box testId="balloon" width=120 height=48 background-color="#0088ff" clip-path=(grown ? "path('M0 0H120V40H0ZM104 40L120 40L120 48Z')" : "path('M0 0H120V40H0ZM104 40L120 40L120 40Z')") transition="clip-path 1s linear"
          text "Hi"
"##;

fn booted(app: &str) -> Host<NoData> {
    let plan = contract::compile(app).unwrap().encode();
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

/// LLP 1003.003 D6: a box hidden mid-run cancels its run, and the host is
/// sent its row's clip then, so shown again it shows the row, not the clip
/// the run had reached.
#[test]
fn a_box_hidden_mid_run_is_sent_its_rows_clip() {
    let mut host = booted(&HIDING.replace(" transition=\"color 1s linear\"", ""));
    let balloon = id(&host, "balloon");
    host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    let mid = host.tick(600.0);
    let shown = clip_of(&mid, balloon).flatten().expect("a clip");
    assert!(tip(&shown) > 40.0 && tip(&shown) < 48.0, "{shown}");
    let hidden = host.dispatch_at(id(&host, "hide"), Event::Press, 650.0);
    let next = host.tick(700.0);
    let row = clip_of(&hidden, balloon)
        .flatten()
        .or_else(|| clip_of(&next, balloon).flatten())
        .expect("the row's clip is sent as the run is cancelled");
    assert_eq!(tip(&row), 40.0, "{row}");
}

/// A parent's inherited colour, re-sent to a child while both move, keeps
/// the child's own presented clip.
#[test]
fn an_inherited_colour_keeps_a_childs_moving_clip() {
    let mut host = booted(HIDING);
    let (parent, balloon) = (id(&host, "parent"), id(&host, "balloon"));
    host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    host.tick(600.0);
    let mut batch = crate::batch::Batch::new();
    host.present_colors(parent, &mut batch, true);
    let sent = host.finish(batch, None);
    let shown = clip_of(&sent, balloon)
        .flatten()
        .expect("the parent's colour re-sends the child's style");
    assert!(tip(&shown) > 40.0 && tip(&shown) < 48.0, "{shown}");
}
