//! What layout derives on a node (`emit_layout`) is layout's to set and clear: an update of the node's authored
//! props leaves it as it is, so the host never sees it go and come back (`update_props`).

use crate::host::{view, NoData};
use exact_apple::Host;
use exact_kernel::MonospaceMeasurer;
use exact_runner::Event;
use serde_json::Value;

/// The `props` ops on `id` in `batch`.
fn props_ops(batch: &str, id: u32) -> Vec<Value> {
    let json: Value = serde_json::from_str(batch).unwrap();
    json["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| op["op"] == "props" && op["id"] == id)
        .cloned()
        .collect()
}

#[test]
fn an_update_of_a_pictures_props_leaves_the_extension_layout_gave_it() {
    // A picture that mirrors itself (`-exact-safe-area-extend`, a length on its top so the kernel sizes that edge
    // with no safe area), its label changed by a press
    let plan = contract::compile(
        r#"component Hero
  state dim = false
  action toggle
    dim = not dim
  view
    column width="100%" height="100%"
      image "assets/hero.jpg" testId="hero" width="100%" height=200 aria-label=(dim ? "Dimmed" : "Hero")
        -exact-safe-area-extend="top start end" -exact-safe-area-extend-bounds="20 0 0 0"
      button testId="toggle" press=toggle height=44
        text "Toggle"
"#,
    )
    .unwrap();
    let (mut host, first) = Host::boot(
        &plan.encode(),
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        600.0,
    )
    .unwrap();
    let hero = view(&host, "hero");
    let set: Vec<Value> = props_ops(&first, hero)
        .iter()
        .map(|op| op["set"].clone())
        .collect();
    assert!(
        set.iter().any(|s| s["boxExtendEdges"] == "1 1 0 1"),
        "{set:?}"
    );
    assert!(
        set.iter().any(|s| s["boxExtension"] == "20 0 0 0"),
        "{set:?}"
    );

    let pressed = host.dispatch_at(view(&host, "toggle"), Event::Press, 0.0);
    let ops = props_ops(&pressed, hero);
    assert!(
        ops.iter()
            .any(|op| op["set"]["accessibilityLabel"] == "Dimmed"),
        "the label changes: {ops:?}"
    );
    for op in &ops {
        let cleared = op["clear"].as_array().unwrap();
        assert!(
            !cleared
                .iter()
                .any(|k| k == "boxExtendEdges" || k == "boxExtension"),
            "{op}"
        );
        assert!(
            op["set"].get("boxExtendEdges").is_none() && op["set"].get("boxExtension").is_none(),
            "{op}"
        );
    }
}
