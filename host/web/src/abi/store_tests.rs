//! The page's store reaches a dev boot (`boot_plan`) as it reaches a built
//! site's (`boot`): a reload in the dev loop keeps its `persist` states.
use super::Bridge;
use exact_plan::Value;
use exact_runner::{DataError, DataSource};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const UNITS: &str = r#"component App
  state units = "C" persist
  view
    text units
"#;

fn units(bridge: &Bridge<NoData>) -> Value {
    bridge
        .host
        .as_ref()
        .unwrap()
        .runner()
        .slot("units")
        .cloned()
        .unwrap()
}

#[test]
fn a_first_dev_boot_reads_the_pages_store() {
    crate::link::link_for_tests();
    let plan = contract::compile(UNITS).unwrap().encode();
    let mut bridge = Bridge::<NoData>::new();
    let stored = bridge.input_write(b"exact.state.units\0\"F\"");
    bridge.store(stored);
    let len = bridge.input_write(&plan);
    bridge.boot_plan(len, NoData, 400., 400., "/");
    assert_eq!(units(&bridge), Value::from("F"));
    // A restart beside the running host carries its store over a snapshot.
    let stored = bridge.input_write(b"exact.state.units\0\"K\"");
    bridge.store(stored);
    let len = bridge.input_write(&plan);
    bridge.boot_plan(len, NoData, 400., 400., "/");
    assert_eq!(units(&bridge), Value::from("F"));
}
