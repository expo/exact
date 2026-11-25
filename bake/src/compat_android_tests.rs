use super::*;

#[test]
fn an_apk_records_the_drag_adapters_it_ships_whatever_its_plan_uses() {
    let plan = contract::compile("component A\n  view\n    text \"No drags here\"\n").unwrap();
    let linked = android_linked(&plan, None, "");
    let uses: Vec<&str> = linked["uses"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|u| u.as_str())
        .collect();
    assert!(
        uses.contains(&"drag_in") && uses.contains(&"drag_out"),
        "{uses:?}"
    );
    let dropping = contract::compile(
        "component A\n  state drops = 0\n  action took(e: DragEvent)\n    drops = drops + 1\n  view\n    column drop=took\n",
    )
    .unwrap();
    let linked = android_linked(&dropping, None, "");
    let uses = linked["uses"].as_array().unwrap();
    assert_eq!(
        uses.iter()
            .filter(|u| u.as_str() == Some("drag_in"))
            .count(),
        1,
        "a plan that takes drops names drag_in once: {uses:?}"
    );
}
