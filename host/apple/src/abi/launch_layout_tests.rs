//! What a boot's first layout reads of its view. The safe-area insets told
//! before a boot are its first layout's: the latest told wins, a refused boot
//! or a discarded candidate leaves them for the next, and a later boot (the
//! dev loop's reload) is told its own after it, as before. The root font size
//! and display scale are the latest valid ones told, before or after a boot.
use super::{Bridge, Hooks};
use exact_runner::{DataError, DataSource, Value};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const PAGE: &str = r#"component App
  view
    column width="100%" padding-top="env(safe-area-inset-top)"
      column testId="first" height=10
"#;

/// Boots `plan`, refused by nothing: a refused boot keeps the old host,
/// whose layout would pass for the new one's.
fn boots(bridge: &mut Bridge<NoData>, plan: &[u8]) {
    let len = bridge.boot(plan, NoData, Hooks::none(), 390.0, 844.0);
    let batch: serde_json::Value =
        serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
    assert!(batch["error"].is_null(), "{}", batch["error"]);
}

/// Where the first row sits: the top inset the layout read.
fn top(bridge: &Bridge<NoData>) -> f32 {
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id("first")[0])
        .unwrap()
        .frame
        .y
}

#[test]
fn the_latest_insets_told_before_a_boot_are_its_first_layouts() {
    let plan = contract::compile(PAGE).unwrap().encode();
    let mut bridge = Bridge::<NoData>::new();
    // Rotations between the first report and the boot: the latest wins.
    bridge.insets(59.0, 0.0, 34.0, 0.0);
    bridge.insets(0.0, 59.0, 21.0, 59.0);
    bridge.insets(47.0, 0.0, 34.0, 0.0);
    boots(&mut bridge, &plan);
    assert_eq!(top(&bridge), 47.0);

    // The boot took them: a reload lays out without them until told, as
    // every reload did.
    boots(&mut bridge, &plan);
    assert_eq!(top(&bridge), 0.0);
    bridge.insets(47.0, 0.0, 34.0, 0.0);
    assert_eq!(top(&bridge), 47.0);
}

#[test]
fn primed_insets_wait_through_a_refused_boot_or_a_discarded_candidate() {
    let plan = contract::compile(PAGE).unwrap().encode();
    let mut bridge = Bridge::<NoData>::new();
    bridge.insets(47.0, 0.0, 34.0, 0.0);
    bridge.boot(b"not a plan", NoData, Hooks::none(), 390.0, 844.0);
    assert!(bridge.host.is_none());
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), NoData, Hooks::none(), 390.0, 844.0);
    bridge.discard_plan();
    boots(&mut bridge, &plan);
    assert_eq!(top(&bridge), 47.0);
}

#[test]
fn a_committed_candidate_takes_the_primed_insets() {
    let plan = contract::compile(PAGE).unwrap().encode();
    let mut bridge = Bridge::<NoData>::new();
    bridge.insets(47.0, 0.0, 34.0, 0.0);
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), NoData, Hooks::none(), 390.0, 844.0);
    bridge.commit_plan();
    assert_eq!(top(&bridge), 47.0);
    boots(&mut bridge, &plan);
    assert_eq!(top(&bridge), 0.0);
}

const REM: &str = r#"component App
  view
    column testId="first" height="2rem"
"#;

#[test]
fn a_reload_lays_out_at_the_latest_root_font_size() {
    let height = |bridge: &Bridge<NoData>| {
        let kernel = bridge.host.as_ref().unwrap().runner().kernel();
        (kernel.node_by_key(kernel.find_by_test_id("first")[0]))
            .unwrap()
            .frame
            .height
    };
    let plan = contract::compile(REM).unwrap().encode();
    let mut bridge = Bridge::<NoData>::new();
    bridge.set_root_font_size(17.0);
    boots(&mut bridge, &plan);
    assert_eq!(height(&bridge), 34.0);
    // Dynamic Type changes while the app runs; a reload starts from it.
    bridge.set_root_font_size(24.0);
    boots(&mut bridge, &plan);
    assert_eq!(height(&bridge), 48.0);
    // What the running host refuses is not kept for the next boot.
    bridge.set_root_font_size(f64::NAN);
    bridge.set_root_font_size(1e-50);
    boots(&mut bridge, &plan);
    assert_eq!(height(&bridge), 48.0);
}

#[test]
fn a_reload_lays_out_at_the_latest_display_scale() {
    let scale = |bridge: &Bridge<NoData>| {
        let kernel = bridge.host.as_ref().unwrap().runner().kernel();
        kernel.reported_display_scale()
    };
    let plan = contract::compile(REM).unwrap().encode();
    let mut bridge = Bridge::<NoData>::new();
    bridge.set_display_scale(2.0);
    boots(&mut bridge, &plan);
    assert_eq!(scale(&bridge), Some(2.0));
    // The window moves to a 3x display; a reload starts from it.
    bridge.set_display_scale(3.0);
    bridge.set_display_scale(1e-50);
    boots(&mut bridge, &plan);
    assert_eq!(scale(&bridge), Some(3.0));
}
