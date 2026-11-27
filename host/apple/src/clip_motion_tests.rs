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

/// A balloon whose clip moves on `transition`, with `extra` attributes.
fn handed(transition: &str, extra: &str, grown: &str, small: &str) -> String {
    format!(
        "component A\n  state grown = true\n  action go\n    grown = not grown\n  view\n    column\n      button press=go testId=\"go\"\n        text \"Go\"\n      box testId=\"balloon\" width=120 height=48 background-color=\"#0088ff\" {extra} clip-path=(grown ? \"path('{grown}')\" : \"path('{small}')\") transition=\"{transition}\"\n"
    )
}

const GROWN: &str = "M0 0H120V40H0ZM104 40L120 40L120 48Z";
const SMALL: &str = "M0 0H120V40H0ZM104 40L120 40L120 40Z";

/// The `path` spec the `animations` op carries for `view`, if any.
fn path_spec(batch: &str, view: u32) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(batch).unwrap();
    v["ops"]
        .as_array()?
        .iter()
        .rev()
        .find(|op| op["op"] == "animations" && op["id"] == view)?["specs"]
        .as_array()?
        .iter()
        .find(|s| s["k"] == "path")
        .cloned()
}

/// Presses Go and gives the run its first frame, on a host that hands box
/// transitions to Core Animation: whether the clip is played, and the batch.
fn started(app: &str) -> (bool, Option<serde_json::Value>) {
    let mut host = booted(app);
    host.svg.box_motion = true;
    host.svg.play_transitions = true;
    host.engine.set_lowered_properties(&svg::lowered(true));
    host.start_on_frame(true, 0.0);
    let balloon = id(&host, "balloon");
    let key = exact_kernel::motion::motion_node(host.runner.kernel().node(balloon).unwrap().key);
    host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    let first = host.tick_at(112.0, 116.0);
    (
        host.engine.is_played(key, Property::ClipPath),
        path_spec(&first, balloon),
    )
}

/// LLP 1003.003 D7: Core Animation plays a box's clip from its start and its
/// end, a cubic within [0, 1] as one timing function between them, and a
/// `steps()` as whole paths that hold.
#[test]
fn core_animation_plays_a_clip_from_its_two_ends() {
    let (played, spec) = started(&handed("clip-path 1s ease", "", GROWN, SMALL));
    assert!(played, "the engine presents the target");
    let spec = spec.expect("a path spec");
    assert_eq!(
        spec["v"].as_array().unwrap().len(),
        2,
        "the two ends: {spec}"
    );
    assert_eq!(
        spec["c"],
        serde_json::json!([[0.25, 0.1, 0.25, 1]]),
        "CSS `ease`"
    );
    assert_eq!(spec["m"], 0);
    assert_eq!(spec["fill"], 2, "the start holds through a delay");
    let (played, spec) = started(&handed("clip-path 1s steps(4)", "", GROWN, SMALL));
    assert!(played);
    let spec = spec.expect("a path spec");
    assert_eq!(spec["m"], 1, "discrete keys");
    // Each of the four paths holds from its key time to the next, the last
    // to the end: one key time more than values (`kCAAnimationDiscrete`)
    assert_eq!(
        spec["t"],
        serde_json::json!([0, 0.25, 0.5, 0.75, 1]),
        "{spec}"
    );
    assert_eq!(spec["v"].as_array().unwrap().len(), 4, "{spec}");
}

/// Where Core Animation would not draw what the engine does, the engine
/// samples the clip: an arc blended between keys, a `mask-image` or rounded
/// corners on a box that clips, which nest the clip in another mask.
#[test]
fn a_clip_core_animation_cannot_play_is_sampled() {
    let arcs = handed(
        "clip-path 1s ease",
        "",
        "M10 30 A20 20 0 0 1 50 30Z",
        "M10 30 A30 30 0 0 1 50 30Z",
    );
    let masked = handed(
        "clip-path 1s ease",
        "mask-image=\"linear-gradient(black, transparent)\"",
        GROWN,
        SMALL,
    );
    let rounded = handed(
        "clip-path 1s ease",
        "overflow=\"hidden\" border-radius=12",
        GROWN,
        SMALL,
    );
    // A radius can outline a box that does not clip its overflow (a
    // backdrop's on the Mac), and a backdrop's material can mask it
    let radius = handed("clip-path 1s ease", "border-radius=\"50%\"", GROWN, SMALL);
    let backdrop = handed(
        "clip-path 1s ease",
        "backdrop-filter=\"blur(6px)\"",
        GROWN,
        SMALL,
    );
    // A run of no duration has no track: the engine switches it when its
    // delay ends
    let instant = handed("clip-path 0s linear 300ms", "", GROWN, SMALL);
    for app in [arcs, masked, rounded, radius, backdrop, instant] {
        let (played, spec) = started(&app);
        assert!(!played && spec.is_none(), "{app}");
    }
    let (played, _) = started(&handed(
        "clip-path 1s steps(3)",
        "",
        "M10 30 A20 20 0 0 1 50 30Z",
        "M10 30 A30 30 0 0 1 50 30Z",
    ));
    assert!(played, "under `steps()` an arc's keys hold, so it plays");
}
