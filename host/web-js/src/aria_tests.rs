//! What the emitted program writes for ARIA roles and states: the native control a role is,
//! the input piece a composite role loads, and the attributes a state is.

/// LLP 1075.003 §9.9 (expo/exact#391): a tab that names its panel and holds
/// a badge is the platform's tab, which the neutral look draws as a tab with
/// its badge; it was a bare button, drawn as a plain one.
#[test]
fn a_tab_with_a_badge_is_a_native_tab() {
    let plan = contract::compile(
        "component App\n  state n = 1\n  view\n    column\n      row role=\"tablist\"\n        button role=\"tab\" aria-controls=\"panel\" aria-selected=true\n          image \"symbol:bookmark\"\n          text \"Saved\"\n          if n > 0\n            row background-color=\"-exact-system-red\"\n              text `${n}`\n      column role=\"tabpanel\" id=\"panel\"\n        text \"Panel\"\n",
    )
    .unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    let tab = js
        .split("const e")
        .find(|made| made.contains("\"role\":\"tab\""))
        .expect("a tab");
    assert!(
        tab.contains("\"data-button-style\":\"bordered\""),
        "a native tab: {tab}"
    );
}

/// LLP 1116.004 D5.2: a radio group of radios is native buttons whose shell
/// look is the segmented control, and its page loads the input piece, which
/// gives it the arrow keys and one Tab stop, even when its radios are bare.
#[test]
fn a_radio_group_is_native_radios_and_loads_the_input_piece() {
    let group = |attrs: &str| {
        let plan = contract::compile(&format!(
            "component App\n  state v = 1\n  action pick(n: number)\n    v = n\n  view\n    row role=\"radiogroup\" aria-label=\"Size\"\n      button role=\"radio\" aria-checked=(v == 1) press=pick(1) {attrs}\n        text \"S\"\n      button role=\"radio\" aria-checked=(v == 2) press=pick(2) {attrs}\n        text \"M\"\n"
        ))
        .unwrap();
        crate::emit::emit(&plan, false, false).unwrap().js
    };
    let js = group("");
    let radio = js
        .split("const e")
        .find(|made| made.contains("\"role\":\"radio\""))
        .expect("a radio");
    assert!(radio.contains("\"button\""), "{radio}");
    assert!(
        radio.contains("\"data-button-style\":\"bordered\""),
        "a native radio: {radio}"
    );
    assert!(js.contains("pressFeedback()"), "{js}");
    let bare = group("appearance=\"none\"");
    assert!(!bare.contains("data-button-style"), "{bare}");
    assert!(
        bare.contains("pressFeedback()"),
        "the group alone loads it: {bare}"
    );
}

/// LLP 1115.000 D4.5: a page with a `tablist` or a `menu` loads the input
/// piece, which gives each its arrow keys, even when nothing in it declares a
/// press look; a page with neither does not.
#[test]
fn a_tablist_and_a_menu_load_the_input_piece() {
    let page = |role: &str| {
        let plan = contract::compile(&format!(
            "component App\n  state v = 1\n  action pick(n: number)\n    v = n\n  view\n    row{role}\n      button appearance=\"none\" press=pick(1)\n        text \"One\"\n      button appearance=\"none\" press=pick(2)\n        text \"Two\"\n"
        ))
        .unwrap();
        crate::emit::emit(&plan, false, false).unwrap().js
    };
    for role in ["tablist", "menu"] {
        let js = page(&format!(" role=\"{role}\""));
        assert!(js.contains("pressFeedback()"), "{role}: {js}");
    }
    let plain = page("");
    assert!(!plain.contains("pressFeedback()"), "{plain}");
}

/// LLP 1172.011 E1: generic element construction carries no input loader;
/// the plan links it for state looks and literal or bound shortcuts.
#[test]
fn looks_and_shortcuts_link_the_input_piece() {
    for view in [
        "column aria-keyshortcuts=\"Control+K\"\n      text \"Page\"",
        "column aria-keyshortcuts=chord\n      text \"Page\"",
        "column\n      :hover\n        opacity = 0.5\n      text \"Page\"",
    ] {
        let plan = contract::compile(&format!(
            "component App\n  state chord = \"Control+K\"\n  view\n    {view}\n"
        ))
        .unwrap();
        let js = crate::emit::emit(&plan, false, false).unwrap().js;
        assert!(js.contains("pressFeedback()"), "{view}: {js}");
    }
    let plan = contract::compile("component App\n  view\n    text \"Hello\"\n").unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(!js.contains("pressFeedback"), "{js}");
}

#[test]
fn aria_disabled_is_an_attribute_for_static_and_bound_values() {
    let plan = contract::compile(
        "component App\n  state unavailable = false\n  view\n    column\n      button \"Update\" aria-disabled=\"true\"\n      button \"Other\" aria-disabled=unavailable\n",
    )
    .unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(js.contains("\"aria-disabled\":\"true\""), "{js}");
    assert!(
        js.matches("\"aria-disabled\"").count() >= 2,
        "the bound value uses the same DOM attribute: {js}"
    );
}

/// LLP 1166.001: a field's `aria-activedescendant`, literal or bound, is the DOM attribute.
#[test]
fn an_active_descendant_uses_the_live_hosts_attribute_static_and_bound() {
    let plan = contract::compile(
        "component App\n  state active = \"option-a\"\n  view\n    column\n      input aria-activedescendant=\"option-b\"\n      input aria-activedescendant=active\n",
    )
    .unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(
        js.contains("\"aria-activedescendant\":\"option-b\""),
        "{js}"
    );
    assert!(js.matches("\"aria-activedescendant\"").count() >= 2, "{js}");
}

/// LLP 1047.003 D1: where data sets a node's props or styles, the compiled
/// module writes its tag and attributes from its literal rows (a sample's tag
/// where a prop decides it) and the Rust renderer from the data's, so the
/// module lists what data can give (`boundTokens`) and the build keeps their
/// rules: a heading's tags for its bound level or role, an `a` or a `button`
/// for a pressable's bound `href`, and the attributes `props_of` derives for
/// the node's type (a switch's `data-exact-switch`). Literal rows decide at
/// compile time, and the module lists nothing.
#[test]
fn a_module_lists_what_data_can_give_its_elements() {
    exact_web::link(exact_web_capabilities::ALL);
    let tokens = |node: &str| {
        let plan = contract::compile(&format!(
            "component App\n  state big = false\n  state url = \"/a\"\n  view\n    column\n      {node}\n"
        ))
        .unwrap();
        let js = crate::emit::emit(&plan, false, false).unwrap().js;
        let list = js
            .split("export const boundTokens=")
            .nth(1)
            .map(|rest| &rest[..rest.find(';').unwrap()]);
        list.map(|l| serde_json::from_str::<Vec<String>>(l).unwrap())
            .unwrap_or_default()
    };
    let has = |node: &str, want: &[&str]| {
        let got = tokens(node);
        for token in want {
            assert!(got.iter().any(|t| t == token), "{node}: {token} in {got:?}");
        }
        got
    };
    has(
        "text role=\"heading\" aria-level=(big ? 1 : 5) \"T\"",
        &["t:h1", "t:h5", "t:h6", "t:div"],
    );
    has("text role=(big ? \"heading\" : \"note\") \"T\"", &["t:h2"]);
    let link = has("button \"Go\" href=url", &["t:a", "t:button"]);
    assert!(!link.iter().any(|t| t == "a:data-exact-switch"), "{link:?}");
    has(
        "input type=\"checkbox\" role=(big ? \"switch\" : \"checkbox\")",
        &["a:data-exact-switch", "a:switch"],
    );
    for literal in [
        "text role=\"heading\" aria-level=1 \"T\"",
        "button \"Go\" href=\"/a\"",
    ] {
        assert_eq!(tokens(literal), Vec::<String>::new(), "{literal}");
    }
}
