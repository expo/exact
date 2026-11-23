//! A `portal` mover hidden at boot (LLP 1013.002 D5) is a live subtree no
//! root reaches: the first tree makes its views, and a later destination
//! receives them without making any again.
use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::{DataError, Event};
use serde_json::Value as Json;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// The card exists only once `tab` is pressed: until then the player has
/// nowhere to be shown.
const APP: &str = r#"component App
  state tab = false
  state open = false
  action flip
    tab = not tab
  action show
    open = not open
  view
    column
      button press=flip testId="tab"
        text "Tab"
      button press=show testId="open"
        text "Open"
      if tab
        box id="card" testId="card" width=168 height=148
      box id="detail" testId="detail" width=300 height=200
      box
        column portal=(open ? "detail" : "card") testId="player"
          video "motion.mp4" testId="video" autoplay=true muted=true
          button testId="count"
            text "Count"
"#;

fn ops(batch: &str) -> Vec<Json> {
    let batch: Json = serde_json::from_str(batch).unwrap();
    batch["ops"].as_array().unwrap().clone()
}

fn id(host: &Host<NoData>, test_id: &str) -> u64 {
    let k = host.runner.kernel();
    u64::from(k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id)
}

fn press(host: &mut Host<NoData>, test_id: &str) -> Vec<Json> {
    let view = id(host, test_id) as ViewId;
    ops(&host.dispatch_at(view, Event::Press, 0.0))
}

fn children(ops: &[Json], parent: u64) -> Option<Vec<u64>> {
    (ops.iter().rev())
        .find(|op| op["op"] == "children" && op["id"] == parent)
        .map(|op| {
            op["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_u64().unwrap())
                .collect()
        })
}

fn made(ops: &[Json]) -> Vec<u64> {
    (ops.iter())
        .filter(|op| op["op"] == "create" || op["op"] == "destroy")
        .map(|op| op["id"].as_u64().unwrap())
        .collect()
}

#[test]
fn a_mover_hidden_at_boot_is_made_and_later_shown_as_itself() {
    let plan = contract::compile(APP).unwrap().encode();
    let (mut host, boot) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    let boot = ops(&boot);
    let (player, video, count) = (id(&host, "player"), id(&host, "video"), id(&host, "count"));
    assert_eq!(host.runner.hidden_movers(), [player as ViewId]);
    let created = made(&boot);
    for view in [player, video, count] {
        assert!(created.contains(&view), "{view} is made at boot: {boot:?}");
    }
    assert_eq!(children(&boot, player), Some(vec![video, count]));
    // Its events bubble through the `box` it is declared in.
    let declared = host.runner.declared_parent(player as ViewId).unwrap();
    let create = (boot.iter())
        .find(|op| op["op"] == "create" && op["id"] == player)
        .unwrap();
    assert_eq!(create["props"]["declaredParent"], declared.to_string());
    assert!(boot.iter().all(
        |op| op["op"] != "children" || !op["ids"].as_array().unwrap().contains(&player.into())
    ));

    let tab = press(&mut host, "tab");
    assert!(host.runner.hidden_movers().is_empty());
    assert_eq!(children(&tab, id(&host, "card")), Some(vec![player]));
    assert!(
        !made(&tab)
            .iter()
            .any(|v| [player, video, count].contains(v)),
        "{tab:?}"
    );

    let open = press(&mut host, "open");
    assert_eq!(children(&open, id(&host, "detail")), Some(vec![player]));
    assert_eq!(children(&open, id(&host, "card")), Some(vec![]));
    assert!(made(&open).is_empty(), "{open:?}");
    // The declaration parent stayed: it is not said again.
    assert!(
        !(open.iter()).any(|op| op["set"].get("declaredParent").is_some()),
        "{open:?}"
    );
    // The list that receives the player comes before the one it leaves.
    assert!(
        at_in(&open, id(&host, "detail")) < at_in(&open, id(&host, "card")),
        "{open:?}"
    );

    let back = press(&mut host, "open");
    assert!(
        at_in(&back, id(&host, "card")) < at_in(&back, id(&host, "detail")),
        "{back:?}"
    );
}

fn at_in(ops: &[Json], parent: u64) -> usize {
    (ops.iter())
        .position(|op| op["op"] == "children" && op["id"] == parent)
        .unwrap()
}
