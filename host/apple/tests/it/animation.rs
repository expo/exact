//! Keyframe animations on Apple (LLP 1055 D5/D7): eligible box transforms
//! and opacity run in Core Animation; the engine samples other motion.
//! Endless sampled motion keeps frames coming, but settlement waits only
//! for finite motion.

use exact_apple::Host;
use exact_kernel::MonospaceMeasurer;
use exact_runner::{DataError, DataSource, Event, Value};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, s: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(s.into()))
    }
}

fn view(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner().kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// The last `present` value of `property` for `id` in a batch.
fn presented(batch: &str, id: u32, property: &str) -> Option<f64> {
    let marker = format!("\"op\":\"present\",\"id\":{id},\"property\":\"{property}\",\"x\":");
    let at = batch.rfind(&marker)?;
    batch[at + marker.len()..]
        .split([',', '}'])
        .next()?
        .parse()
        .ok()
}

/// Whether the batch hands `id` a lowered animation (a non-empty
/// `animations` op).
fn lowered(batch: &str, id: u32) -> bool {
    let marker = format!("\"op\":\"animations\",\"id\":{id},\"specs\":");
    batch
        .rfind(&marker)
        .is_some_and(|at| !batch[at + marker.len()..].starts_with("[]"))
}

fn track(batch: &str, id: u32, key: &str) -> serde_json::Value {
    let batch: serde_json::Value = serde_json::from_str(batch).unwrap();
    batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["op"] == "animations" && op["id"] == id)
        .unwrap()["specs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|spec| spec["k"] == key)
        .unwrap()
        .clone()
}

fn close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("presented");
    assert!((actual - expected).abs() < 1e-5, "{actual} vs {expected}");
}

#[test]
fn an_animation_is_presented_from_boot_and_stops_when_its_row_does() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contract/corpus/keyframes.contract"
    ))
    .unwrap();
    let plan = contract::compile(&src).unwrap();
    let (mut host, first) = Host::boot(
        &plan.encode(),
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    let (mark, added, toggle) = (
        view(&host, "mark"),
        view(&host, "added"),
        view(&host, "toggle"),
    );
    assert!(first.contains("\"motion\":false"), "{first}");
    assert!(lowered(&first, mark), "{first}");
    assert!(presented(&first, mark, "opacity").is_none(), "{first}");
    assert!(presented(&first, mark, "scale").is_none(), "{first}");
    assert!(presented(&first, added, "translate").is_none(), "{first}");
    let scale = track(&first, mark, "transform.scale");
    assert_eq!(scale["v"], serde_json::json!([0.9, 1]));
    assert_eq!(scale["d"], 1.6);
    assert_eq!(scale["n"], -1);
    let translate = track(&first, added, "transform.translation.y");
    assert_eq!(translate["v"], serde_json::json!([8, 0]));
    assert_eq!(translate["d"], 0.3);
    assert_eq!(translate["n"], 1);
    // The finite one settles at 300 ms; the endless one is not waited for.
    assert_eq!(host.agent("{\"op\":\"settle\"}"), "{\"settle\":300}");

    let tick = host.tick(400.0);
    assert!(presented(&tick, mark, "scale").is_none(), "{tick}");
    assert_eq!(host.agent("{\"op\":\"settle\"}"), "{\"settle\":null}");
    assert!(tick.contains("\"motion\":false"), "{tick}");

    // `none`: the row's own values again, and the frames stop.
    let off = host.dispatch_at(toggle, Event::Press, 500.0);
    assert!(!lowered(&off, mark), "{off}");
    close(presented(&off, mark, "scale"), 1.0);
    assert!(off.contains("\"motion\":false"), "{off}");
    // Named again, it starts again from its first keyframe.
    let on = host.dispatch_at(toggle, Event::Press, 600.0);
    assert!(lowered(&on, mark), "{on}");
    close(presented(&on, mark, "scale"), 1.0);
    let restarted = track(&on, mark, "transform.scale");
    assert_eq!(restarted["v"], scale["v"]);
    assert_eq!(restarted["s"], 0.6);
    assert!(on.contains("\"motion\":false"), "{on}");
    assert!(
        presented(&on, added, "translate").is_none(),
        "untouched: {on}"
    );
}

/// LLP 1061 D4: the batch says whether what moves changes place or size.
/// A breathing colour keeps frames coming but not at the panel's full rate; a
/// slide asks for it.
#[test]
fn a_colour_is_motion_and_a_slide_is_spatial() {
    let src = "keyframes breathe\n  from color=\"#666666\"\n  50% color=\"#000000\"\n  to color=\"#666666\"\ncomponent A\n  state sliding = false\n  action slide\n    sliding = not sliding\n  view\n    column\n      button press=slide testId=\"slide\"\n        text \"Slide\"\n      text \"Breathe\" animation=\"breathe 4.2s ease-in-out infinite\"\n      text \"Card\" transition=\"translate 300ms ease\" translate=(sliding ? \"0px 40px\" : \"0px 0px\")\n";
    let plan = contract::compile(src).unwrap();
    let (mut host, first) = Host::boot(
        &plan.encode(),
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    assert!(
        first.contains("\"motion\":true") && !first.contains("\"spatial\""),
        "{first}"
    );
    let slid = host.dispatch_at(view(&host, "slide"), Event::Press, 100.0);
    assert!(slid.contains("\"spatial\":true"), "{slid}");
    let settled = host.tick(1000.0);
    assert!(
        settled.contains("\"motion\":true") && !settled.contains("\"spatial\""),
        "{settled}"
    );
}
