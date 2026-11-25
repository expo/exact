use super::*;

fn plan(source: &str) -> exact_plan::Plan {
    contract::compile(source).unwrap()
}

fn uses(linked: &serde_json::Value) -> Vec<String> {
    let names = linked["uses"].as_array().unwrap().iter();
    names.map(|u| u.as_str().unwrap().to_owned()).collect()
}

#[test]
fn an_apk_takes_a_generation_that_first_drops_or_drags_out() {
    let installed = uses(&android_linked(
        &plan("component A\n  view\n    text \"No drags here\"\n"),
        None,
        "",
    ));
    for later in [
        "component A\n  state n = 0\n  action took(e: DragEvent)\n    n = n + 1\n  view\n    column drop=took\n",
        "component A\n  view\n    column draggable=true dataTransfer=DataTransfer(text=\"Fuji\")\n      text \"Card\"\n",
    ] {
        for need in android_uses(&plan(later)) {
            assert!(installed.iter().any(|have| have == need), "{need} is not in {installed:?}: {later}");
        }
    }
}

#[test]
fn the_same_linked_capabilities_record_the_same_roster() {
    let pasted = "component A\n  state n = 0\n  action pasted(e: ClipboardEvent)\n    n = n + 1\n  view\n    column paste=pasted\n";
    let dropped = "component A\n  state n = 0\n  action took(e: DragEvent)\n    n = n + 1\n  view\n    column drop=took\n";
    assert_eq!(
        uses(&android_linked(&plan(pasted), None, "")),
        uses(&android_linked(&plan(dropped), None, "")),
    );
}
