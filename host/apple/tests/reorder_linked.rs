//! Production linkage regression for expo/exact#1851: a reorder-only list plan
//! with no authored `transition` style must link the CSS transition grammar
//! through `apple_uses` + `Links::of` so runtime row transitions succeed and
//! never poison the runner.
//!
//! Spawns a fresh child process that never calls `contract::compile()`, keeping
//! `exact_motion`'s process-global `LINKED` OnceLock uninitialized until
//! `Bridge::with_links(Links::of(apple_uses(&plan)))` boots.

use exact_apple::abi::{Bridge, Hooks};
use exact_apple::link::{self, Links};
use exact_runner::list_api::{CollectionFeedback, RowMeasurement};
use std::process::Command;

struct NoData;

impl exact_runner::DataSource for NoData {
    fn query(
        &mut self,
        name: &str,
        _: &[exact_plan::Value],
    ) -> Result<exact_plan::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(name.into()))
    }
}

const VIRTUALIZED_REORDER: &str = r#"component ReorderMin
  state count = 0
  state items = ["a", "b", "c", "d"]

  action bump
    count = count + 1

  action reorder(item: string, before: option<string>)
    let rest = items.filter(i => i != item)
    items = match before { case some(b) => rest.slice(0, rest.indexOf(b)).concat([item]).concat(rest.slice(rest.indexOf(b))), case none => rest.concat([item]) }

  view
    column height=400
      button "Bump" testId="bump" press=bump
      text `Count ${count}` testId="count"
      text items.join("") testId="order"
      list id="items" virtualized=true estimated-item-height=40 height=200 reorderdrop=reorder
        for x in items key=x
          row height=40
            text x testId=`grip-${x}` reorderFor="items"
            text `Item ${x}`
"#;

const MOUNTED_REORDER: &str = r#"component MountedReorderMin
  state count = 0
  state items = ["a", "b", "c"]

  action bump
    count = count + 1

  action reorder(item: string, before: option<string>)
    let rest = items.filter(i => i != item)
    items = match before { case some(b) => rest.slice(0, rest.indexOf(b)).concat([item]).concat(rest.slice(rest.indexOf(b))), case none => rest.concat([item]) }

  view
    column height=400
      button "Bump" testId="bump" press=bump
      text `Count ${count}` testId="count"
      text items.join("") testId="order"
      list id="items" listStyle="inset-grouped" reorderdrop=reorder
        section
          for x in items key=x
            row height=40
              text x testId=`grip-${x}` reorderFor="items"
              text `Item ${x}`
"#;

fn view_by_test_id(batch: &serde_json::Value, test_id: &str) -> u32 {
    batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|op| {
            (op["op"] == "create" && op["props"]["testId"] == test_id)
                .then(|| op["id"].as_u64().unwrap() as u32)
        })
        .unwrap_or_else(|| panic!("missing testId={test_id} in {batch}"))
}

fn u64_field(v: &serde_json::Value) -> u64 {
    v.as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| v.as_u64())
        .unwrap_or_else(|| panic!("invalid u64 field {v}"))
}

fn collection_item<'a>(batch: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    batch["ops"]
        .as_array()?
        .iter()
        .rfind(|op| op["op"] == "collections")
        .and_then(|op| op["items"].as_array()?.first())
}

fn reorder_token(op: &serde_json::Value) -> u64 {
    u64_field(&op["token"])
}

fn reorder_op<'a>(batch: &'a serde_json::Value) -> &'a serde_json::Value {
    batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .rfind(|op| op["op"] == "reorder")
        .unwrap_or_else(|| panic!("missing reorder op in {batch}"))
}

fn run_json(bridge: &Bridge<NoData>, len: u32) -> serde_json::Value {
    let raw = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{e}: {raw}"));
    assert!(v["error"].is_null(), "unexpected runner error: {raw}");
    v
}

#[test]
fn production_linked_virtualized_reorder_does_not_poison_runner() {
    if let Ok(path) = std::env::var("EXACT_REORDER_VIRT_PLAN") {
        let bytes = std::fs::read(&path).unwrap();
        let plan = exact_plan::Plan::decode(&bytes).unwrap();
        let linked = exact_runner::uses::apple_uses(&plan);
        link::set(linked);
        let mut bridge = Bridge::<NoData>::with_links(Links::of(linked));
        let out = bridge.boot(&bytes, NoData, Hooks::none(), 390.0, 844.0);
        let booted = run_json(&bridge, out);

        let bump_id = view_by_test_id(&booted, "bump");
        let grip_d = view_by_test_id(&booted, "grip-d");
        let mut latest = booted;
        let feed = |bridge: &mut Bridge<NoData>, latest: &mut serde_json::Value, base_ms: f64| {
            for step in 1..=3 {
                let col = collection_item(latest)
                    .unwrap_or_else(|| panic!("missing collections op in {latest}"));
                let rows: Vec<RowMeasurement> = col["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| RowMeasurement {
                        view: u64_field(&r["view"]) as u32,
                        epoch: u64_field(&r["epoch"]),
                        size: 40.0,
                    })
                    .collect();
                let fb = CollectionFeedback {
                    view: u64_field(&col["view"]) as u32,
                    revision: u64_field(&col["revision"]),
                    scroll_sequence: u64_field(&col["scrollSequence"]) + 1,
                    offset: 0.0,
                    port_cross: 390.0,
                    port_main: 200.0,
                    cross: 390.0,
                    padding: [0.0; 2],
                    focus_view: None,
                    interaction_view: Some(grip_d),
                    measurements: rows,
                }
                .encode()
                .unwrap();
                let fb_len = bridge.input_write(&fb);
                let out = bridge.collection_feedback(fb_len, base_ms + step as f64 * 10.0);
                let reply = run_json(bridge, out);
                if collection_item(&reply).is_some() {
                    *latest = reply;
                }
            }
        };
        feed(&mut bridge, &mut latest, 0.0);

        // 1. Reorder drop (synthesizes `transition: translate -exact-spring(300,30,1)` and `none`).
        let out = bridge.reorder_begin(grip_d, 0.0, 100.0);
        let begun = run_json(&bridge, out);
        let op = reorder_op(&begun);
        assert_eq!(op["phase"], "active", "reorder_begin refused: {begun}");
        let token = reorder_token(op);
        let out = bridge.reorder_move(token, -80.0, 0.0, 1, 150.0);
        let _ = run_json(&bridge, out);
        let out = bridge.reorder_end(token, 1, -80.0, 0.0, 1, 0.0, 200.0);
        let ended = run_json(&bridge, out);
        let ended_raw = ended.to_string();
        assert!(
            ended_raw.contains("abdc"),
            "expected reordered list: {ended_raw}"
        );
        if collection_item(&ended).is_some() {
            latest = ended;
        }
        let out = bridge.tick(5000.0);
        let settled = run_json(&bridge, out);
        if collection_item(&settled).is_some() {
            latest = settled;
        }

        // 2. Reorder cancel also leaves runner healthy.
        feed(&mut bridge, &mut latest, 5100.0);
        let out = bridge.reorder_begin(grip_d, 0.0, 5200.0);
        let begun2 = run_json(&bridge, out);
        let op2 = reorder_op(&begun2);
        assert_eq!(
            op2["phase"], "active",
            "second reorder_begin refused: {begun2}"
        );
        let token2 = reorder_token(op2);
        let out = bridge.reorder_end(token2, 0, 0.0, 0.0, 1, 0.0, 5250.0);
        let _ = run_json(&bridge, out);

        // 3. Subsequent ordinary press succeeds (not Poisoned).
        let out = bridge.dispatch(bump_id, 0, 0, 5300.0);
        let pressed = run_json(&bridge, out);
        assert!(
            pressed.to_string().contains("Count 1"),
            "press after reorder failed: {pressed}"
        );
        return;
    }

    let bytes = contract::compile(VIRTUALIZED_REORDER).unwrap().encode();
    let path = std::env::temp_dir().join(format!("exact-reorder-virt-{}.plan", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "production_linked_virtualized_reorder_does_not_poison_runner",
            "--nocapture",
        ])
        .env("EXACT_REORDER_VIRT_PLAN", &path)
        .status()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(status.success(), "isolated child process failed");
}

#[test]
fn production_linked_mounted_reorder_does_not_poison_runner() {
    if let Ok(path) = std::env::var("EXACT_REORDER_MOUNTED_PLAN") {
        let bytes = std::fs::read(&path).unwrap();
        let plan = exact_plan::Plan::decode(&bytes).unwrap();
        let linked = exact_runner::uses::apple_uses(&plan);
        link::set(linked);
        let mut bridge = Bridge::<NoData>::with_links(Links::of(linked));
        let out = bridge.boot(&bytes, NoData, Hooks::none(), 390.0, 844.0);
        let booted = run_json(&bridge, out);

        let bump_id = view_by_test_id(&booted, "bump");
        let grip_c = view_by_test_id(&booted, "grip-c");

        // Lift, move, and cancel mounted grouped reorder (synthesizes row translate spring).
        let out = bridge.reorder_group_begin(grip_c, 0.0, 1, 100.0);
        let begun = run_json(&bridge, out);
        let op = reorder_op(&begun);
        assert_eq!(
            op["phase"], "active",
            "reorder_group_begin refused: {begun}"
        );
        let token = reorder_token(op);
        let out = bridge.reorder_group_end(token, 0, 200.0);
        let _ = run_json(&bridge, out);

        // Subsequent ordinary press succeeds (not Poisoned).
        let out = bridge.dispatch(bump_id, 0, 0, 300.0);
        let pressed = run_json(&bridge, out);
        assert!(
            pressed.to_string().contains("Count 1"),
            "press after mounted reorder failed: {pressed}"
        );
        return;
    }

    let bytes = contract::compile(MOUNTED_REORDER).unwrap().encode();
    let path =
        std::env::temp_dir().join(format!("exact-reorder-mounted-{}.plan", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "production_linked_mounted_reorder_does_not_poison_runner",
            "--nocapture",
        ])
        .env("EXACT_REORDER_MOUNTED_PLAN", &path)
        .status()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(status.success(), "isolated child process failed");
}
