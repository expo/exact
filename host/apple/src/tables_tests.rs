//! LLP 1153 D7.3 on the Apple host: the presenter hears where a table's
//! rows and cells stand and the order the table draws its rows in.
use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::DataError;

#[derive(Clone, Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const APP: &str = r#"component App
  state wide = false
  action widen
    wide = not wide
  view
    column
      button press=widen testId="widen"
        text "Widen"
      table testId="table"
        caption
          text "Moves"
        tfoot testId="foot1"
          tr testId="r-foot1"
            td colspan=2
              text "Foot 1"
        tbody testId="body"
          tr testId="r-body"
            th scope="row" colspan=(wide ? 2 : 1) testId="name"
              text "Tackle"
            td testId="power"
              text "40"
        tfoot testId="foot2"
          tr testId="r-foot2"
            td testId="foot2-cell"
              text "Foot 2"
"#;

/// The rows and cells the test reads.
const PARTS: [&str; 6] = [
    "r-body",
    "name",
    "power",
    "r-foot1",
    "r-foot2",
    "foot2-cell",
];

fn id(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner().kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// Each `table` op in `batch`, by view.
fn tables(batch: &str) -> std::collections::BTreeMap<u32, serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(batch).unwrap();
    (v["ops"].as_array().cloned().unwrap_or_default().into_iter())
        .filter(|op| op["op"] == "table")
        .map(|op| (op["id"].as_u64().unwrap() as u32, op))
        .collect()
}

/// A table says its rows and columns and its children as drawn (the first
/// `tfoot` last, CSS 2.2 §17.2), each row and cell its place from 0 in that
/// order; a change of span is sent for the cells it moves alone.
#[test]
fn a_tables_size_order_and_places_reach_the_presenter() {
    let plan = contract::compile(APP).unwrap().encode();
    let (mut host, boot) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    let ops = tables(&boot);
    let table = id(&host, "table");
    let caption = host.runner().kernel().node(table).unwrap().children()[0];
    let drawn: Vec<u32> = ["body", "foot2", "foot1"].map(|t| id(&host, t)).to_vec();
    assert_eq!(ops[&table]["size"], serde_json::json!([3, 2]));
    let order: Vec<u32> = (ops[&table]["order"].as_array().unwrap().iter())
        .map(|v| v.as_u64().unwrap() as u32)
        .collect();
    assert_eq!(order, [vec![caption], drawn].concat());
    let ids: std::collections::HashMap<&str, u32> =
        PARTS.into_iter().map(|t| (t, id(&host, t))).collect();
    let place = |ops: &std::collections::BTreeMap<u32, serde_json::Value>, t: &str| {
        let op = &ops[&ids[t]];
        assert_eq!(op["table"], table, "{t}");
        op["place"].clone()
    };
    assert_eq!(place(&ops, "r-body"), serde_json::json!([0, 1, 0, 2]));
    assert_eq!(place(&ops, "name"), serde_json::json!([0, 1, 0, 1]));
    assert_eq!(place(&ops, "power"), serde_json::json!([0, 1, 1, 1]));
    assert_eq!(place(&ops, "r-foot2"), serde_json::json!([1, 1, 0, 2]));
    assert_eq!(place(&ops, "r-foot1"), serde_json::json!([2, 1, 0, 2]));
    assert!(!ops.contains_key(&caption), "a caption has no place");

    let widen = id(&host, "widen");
    let ops = tables(&host.dispatch(widen, Event::Press));
    assert_eq!(place(&ops, "name"), serde_json::json!([0, 1, 0, 2]));
    assert_eq!(place(&ops, "power"), serde_json::json!([0, 1, 2, 1]));
    assert_eq!(ops[&table]["size"], serde_json::json!([3, 3]));
    assert!(
        !ops.contains_key(&ids["foot2-cell"]),
        "an unmoved cell is not sent again"
    );
}
