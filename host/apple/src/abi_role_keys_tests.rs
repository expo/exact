//! A painted group's keys and Tab stop through `exact_role_keys` (LLP 1166.000 D2).
use super::*;
use serde_json::{json, Value};

/// A painted radio group whose middle radio is `aria-disabled`, a vertical
/// tablist, a horizontal right-to-left one, and a menu, which sets no stop,
/// and a radio group of `input type="radio"`s, whose group is HTML's.
const APP: &str = r#"component App
  state size = "s"
  state tab = "b"
  state pages = 0
  action setSize(v: string)
    size = v
  action setTab(v: string)
    tab = v
  action turn
    pages = pages + 1
  view
    column
      row role="radiogroup" appearance="none" aria-label="Size" testId="sizes"
        for v in ["s", "m", "l"] key=v
          button press=setSize(v) appearance="none" role="radio" aria-checked=(v == size) aria-disabled=(v == "m") testId=`size-${v}`
            text v
      column role="tablist" aria-orientation="vertical" aria-label="Sections" testId="tabs"
        for v in ["a", "b", "c"] key=v
          button press=setTab(v) appearance="none" role="tab" aria-selected=(v == tab) testId=`tab-${v}`
            text v
      row role="tablist" appearance="none" direction="rtl" aria-label="Pages" testId="pages"
        for v in ["x", "y", "z"] key=v
          button press=turn appearance="none" role="tab" testId=`page-${v}`
            text v
      column role="menu" aria-label="Actions" testId="menu"
        button press=turn testId="cut"
          text "Cut"
        button press=turn testId="copy"
          text "Copy"
        button press=turn testId="paste"
          text "Paste"
      row role="radiogroup" aria-label="Fit" testId="fits"
        input type="radio" name="fit" value="a" testId="fit-a"
        input type="radio" name="fit" value="b" checked=true testId="fit-b"
"#;

fn booted() -> Bridge<StorageModule> {
    let plan = contract::compile(APP).unwrap().encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, StorageModule::default(), Hooks::none(), 400., 800.);
    bridge
}

fn id(bridge: &Bridge<StorageModule>, test_id: &str) -> u32 {
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .id
}

/// The export's answer for `view` with `key` ("" for none) and the focus at
/// `focused` (0 for none).
fn ask(bridge: &mut Bridge<StorageModule>, view: u32, key: &str, focused: u32) -> Value {
    let len = bridge.input_write(key.as_bytes());
    let n = bridge.role_keys(view, focused, len);
    serde_json::from_slice(bridge.output_bytes(n as usize)).unwrap()
}

#[test]
fn a_painted_group_answers_its_orders_and_skips_an_aria_disabled_radio() {
    let mut bridge = booted();
    let [s, m, l] = ["size-s", "size-m", "size-l"].map(|t| id(&bridge, t));
    let [a, b, c] = ["tab-a", "tab-b", "tab-c"].map(|t| id(&bridge, t));
    let [x, y, z] = ["page-x", "page-y", "page-z"].map(|t| id(&bridge, t));
    let (sizes, tabs, pages) = (
        id(&bridge, "sizes"),
        id(&bridge, "tabs"),
        id(&bridge, "pages"),
    );

    // A radio group: the arrows press, the aria-disabled radio is passed over
    // both ways, and it is never the Tab stop. The checked radio is.
    let answer = ask(&mut bridge, s, "ArrowRight", 0);
    assert_eq!(
        answer,
        json!({"stop": {"group": sizes, "role": "radiogroup", "held": false, "order": [s, l]},
               "step": {"group": sizes, "role": "radiogroup", "to": [l], "press": true}})
    );
    assert_eq!(
        ask(&mut bridge, l, "ArrowDown", 0)["step"]["to"],
        json!([s])
    );
    assert_eq!(ask(&mut bridge, s, "ArrowUp", 0)["step"]["to"], json!([l]));
    assert_eq!(
        ask(&mut bridge, m, "ArrowRight", m)["step"]["to"],
        json!([l, s]),
        "the focused member is the origin whatever its state"
    );
    assert_eq!(
        ask(&mut bridge, s, "Home", 0)["step"],
        Value::Null,
        "a radio group has no Home"
    );
    // While a member has the focus it alone is the stop.
    assert_eq!(ask(&mut bridge, l, "", l)["stop"]["held"], json!(true));
    assert_eq!(ask(&mut bridge, s, "", l)["stop"]["order"], json!([l]));
    assert_eq!(
        ask(&mut bridge, s, "", 0)["step"],
        Value::Null,
        "no key, no step"
    );

    // A vertical tablist: Up and Down, wrapping, Home and End, nothing
    // pressed; its stop is the selected tab, then the rest in tree order.
    let answer = ask(&mut bridge, c, "ArrowDown", 0);
    assert_eq!(
        answer,
        json!({"stop": {"group": tabs, "role": "tablist", "held": false, "order": [b, a, c]},
               "step": {"group": tabs, "role": "tablist", "to": [a, b], "press": false}})
    );
    assert_eq!(
        ask(&mut bridge, a, "ArrowUp", 0)["step"]["to"],
        json!([c, b])
    );
    assert_eq!(
        ask(&mut bridge, b, "ArrowRight", 0)["step"],
        Value::Null,
        "a vertical tablist's Right is not its key"
    );
    // Home and End: every member from that end, the focused one in its
    // place, for the host to take the first it can focus.
    assert_eq!(
        ask(&mut bridge, c, "Home", 0)["step"]["to"],
        json!([a, b, c])
    );
    assert_eq!(
        ask(&mut bridge, a, "End", 0)["step"]["to"],
        json!([c, b, a])
    );
    assert_eq!(
        ask(&mut bridge, a, "Home", 0)["step"]["to"],
        json!([a, b, c]),
        "at the first: itself first, which moves nothing"
    );

    // A horizontal tablist mirrors Left and Right right to left.
    assert_eq!(
        ask(&mut bridge, y, "ArrowLeft", 0)["step"]["to"],
        json!([z, x])
    );
    assert_eq!(
        ask(&mut bridge, y, "ArrowRight", 0)["step"]["to"],
        json!([x, z])
    );
    assert_eq!(ask(&mut bridge, y, "ArrowDown", 0)["step"], Value::Null);
    assert_eq!(ask(&mut bridge, y, "", 0)["stop"]["group"], json!(pages));

    // A menu's item moves by Up, Down, Home and End, wrapping and pressing
    // nothing, and the menu sets no Tab stop: each item keeps its host's rule.
    let [cut, copy, paste] = ["cut", "copy", "paste"].map(|t| id(&bridge, t));
    let menu = id(&bridge, "menu");
    assert_eq!(
        ask(&mut bridge, cut, "ArrowDown", 0),
        json!({"stop": null, "step": {"group": menu, "role": "menu", "to": [copy, paste], "press": false}})
    );
    assert_eq!(
        ask(&mut bridge, cut, "ArrowUp", 0)["step"]["to"],
        json!([paste, copy])
    );
    assert_eq!(
        ask(&mut bridge, paste, "Home", 0)["step"]["to"],
        json!([cut, copy, paste])
    );
    assert_eq!(
        ask(&mut bridge, cut, "End", 0)["step"]["to"],
        json!([paste, copy, cut])
    );
    assert_eq!(
        ask(&mut bridge, cut, "ArrowRight", 0),
        Value::Null,
        "no stop, and not a menu's key"
    );
    assert_eq!(ask(&mut bridge, cut, "", 0), Value::Null);
    // An `input type="radio"` is HTML's radio, whose group is its `name`.
    let [fit_a, fit_b] = ["fit-a", "fit-b"].map(|t| id(&bridge, t));
    assert_eq!(ask(&mut bridge, fit_a, "ArrowRight", 0), Value::Null);
    assert_eq!(ask(&mut bridge, fit_b, "", 0), Value::Null);
    // Nor does a node of no group, or a group itself, get one.
    assert_eq!(ask(&mut bridge, sizes, "ArrowRight", 0), Value::Null);
    assert_eq!(ask(&mut bridge, menu, "", 0), Value::Null);
}
