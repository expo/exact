use super::{boot, press, text_of};

fn source(binding: &str, declarations: &str, view: &str) -> String {
    format!(
        "shape Viewport\n  width: number\nfn pick(n: number): number = n + 1\ncomponent App\n  view\n    Child()\ncomponent Child\n  {binding}\n{declarations}  view\n{view}\ncomponent Frame\n  slot\n  view\n    children\ncomponent Label\n  props\n    value: string\n  view\n    text value testId=\"result\"\n"
    )
}

const STATE: &str = "state pick = 7";
const DEVICE: &str = "resource pick = exactViewport() as shape Viewport";

#[test]
fn value_props_and_injects_named_like_an_app_fn_leave_calls_intact() {
    let prop = "fn pick(n: number): number = n + 1\nfn other(n: number): number = n + 100\ncomponent App\n  state other = 7\n  view\n    Seg(pick=other)\ncomponent Seg\n  props\n    pick: number\n  view\n    text `${pick(1)} ${pick}` testId=\"result\"\n";
    let inject = prop
        .replace(
            "  view\n    Seg(pick=other)",
            "  provide\n    pick = other\n  view\n    Seg()",
        )
        .replace("  props\n", "  inject\n");
    for source in [prop, &inject] {
        assert_eq!(text_of(&boot(source), "result"), "2 7");
    }
}

#[test]
fn value_props_and_injects_leave_stdlib_call_heads_intact() {
    let prop = "component App\n  state max = 3\n  view\n    column\n      Child(min=max)\ncomponent Child\n  props\n    min: number\n  view\n    text `${min(4, 9)} ${min}` testId=\"result\"\n";
    let inject = prop
        .replace(
            "  view\n    column\n      Child(min=max)",
            "  provide\n    min = max\n  view\n    Child()",
        )
        .replace("  props\n", "  inject\n");
    for source in [prop.to_string(), prop.replace("min=max", "min=3"), inject] {
        assert_eq!(text_of(&boot(&source), "result"), "4 3");
    }
}

#[test]
fn value_props_and_injects_preserve_calls_in_declarations_fills_and_actions() {
    let prop = include_str!("../../../../../semantics/corpus/components/value-prop-fn.contract");
    let inject = prop
        .replace(
            "  action change()",
            "  provide\n    min = lo\n    max = hi\n    value = v\n    pick = other\n  action change()",
        )
        .replace("Child(min=lo, max=hi, value=v, pick=other)", "Child()")
        .replace("component Child\n  props", "component Child\n  inject");
    for source in [prop, &inject] {
        let mut r = boot(source);
        assert_eq!(text_of(&r, "fill"), "4 0 10 7");
        assert_eq!(text_of(&r, "values"), "5 6 0");
        assert_eq!(text_of(&r, "passed"), "6 6");
        press(&mut r, "up");
        assert_eq!(text_of(&r, "values"), "5 6 14");
        press(&mut r, "change");
        assert_eq!(text_of(&r, "values"), "5 10 14");
        assert_eq!(text_of(&r, "passed"), "10 10");
        press(&mut r, "relay");
        assert_eq!(text_of(&r, "values"), "5 10 18");
    }
}

#[test]
fn lean_enum_prop_literals_emit_as_strings() {
    let src = r#"component App
  view
    Child(value="column")
component Child
  props
    value: enum<"column", "row">
  view
    text value
"#;
    let string = src.replace("enum<\"column\", \"row\">", "string");
    for emit in [contract::lean::lean, contract::lean::lean_plain] {
        assert_eq!(emit(src, "app").unwrap(), emit(&string, "app").unwrap());
    }
}

#[test]
fn lean_components_preserve_child_device_resources() {
    let src = include_str!("../../../../../semantics/corpus/components/device-facts.contract");
    let emitted = contract::lean::lean_components(src, "facts").unwrap();
    let child = emitted.split("name := \"Child\", props :=").nth(1).unwrap();
    let resources = child
        .split("resources := ")
        .nth(1)
        .unwrap()
        .split(",\n      mutations :=")
        .next()
        .unwrap();
    assert_eq!(
        resources,
        "[{ name := \"pick\", ty := (.record \"Viewport\"), source := \"exactViewport\", args := [] },\n    \
         { name := \"online\", ty := (.record \"Page\"), source := \"exactPage\", args := [] }]"
    );
    assert!(!emitted.contains("pick@Child"));
    assert!(!emitted.contains("online@Child"));
}

#[test]
fn state_values_leave_function_calls_intact() {
    let mut r = boot(include_str!(
        "../../../../../semantics/corpus/components/state-fn.contract"
    ));
    for (id, expected) in [
        ("direct", "7 6"),
        ("derived", "8"),
        ("initial", "6"),
        ("fill", "7 6"),
        ("argument", "8"),
        ("provided", "8"),
        ("shown", "0"),
    ] {
        assert_eq!(text_of(&r, id), expected, "{id}");
    }
    for (button, value, derived, shown) in [("update", 10, 11, 8), ("relay", 13, 14, 11)] {
        press(&mut r, button);
        assert_eq!(text_of(&r, "direct"), format!("{value} 6"));
        assert_eq!(text_of(&r, "fill"), format!("{value} 6"));
        for id in ["derived", "argument", "provided"] {
            assert_eq!(text_of(&r, id), derived.to_string(), "{id}");
        }
        assert_eq!(text_of(&r, "shown"), shown.to_string());
        assert_eq!(text_of(&r, "chosen"), "16");
    }
}

#[test]
fn device_values_leave_view_and_initializer_calls_intact() {
    for view in [
        "    text `${pick.width} ${pick(5)} ${initial}` testId=\"result\"",
        "    Frame()\n      text `${pick.width} ${pick(5)} ${initial}` testId=\"result\"",
        "    Label(value=`${pick.width} ${pick(5)} ${initial}`)",
    ] {
        let mut r = boot(&source(DEVICE, "  state initial = pick(3)\n", view));
        r.set_viewport(900.0, 600.0).unwrap();
        assert_eq!(text_of(&r, "result"), "900 6 4");
    }
}

#[test]
fn state_and_device_names_leave_calls_in_view_derives_intact() {
    for (binding, read, value) in [(STATE, "pick", 7), (DEVICE, "pick.width", 900)] {
        let src = source(
            binding,
            &format!("  derive computed = pick({read})\n"),
            "    text `${computed}` testId=\"result\"",
        );
        let mut r = boot(&src);
        r.set_viewport(900.0, 600.0).unwrap();
        assert_eq!(text_of(&r, "result"), (value + 1).to_string());
    }
}

#[test]
fn state_and_device_names_leave_calls_in_action_derives_intact() {
    for (binding, read, value) in [(STATE, "pick", 7), (DEVICE, "pick.width", 900)] {
        for expression in [format!("pick({read})"), "computed".into()] {
            let src = source(
                binding,
                &format!("  derive computed = pick({read})\n  state shown = 0\n  action update()\n    shown = {expression}\n  action relay()\n    update()\n"),
                "    column\n      text `${shown}` testId=\"result\"\n      button \"Update\" press=update testId=\"update\"\n      button \"Relay\" press=relay testId=\"relay\"",
            );
            for button in ["update", "relay"] {
                let mut r = boot(&src);
                r.set_viewport(900.0, 600.0).unwrap();
                assert_eq!(text_of(&r, "result"), "0");
                press(&mut r, button);
                assert_eq!(text_of(&r, "result"), (value + 1).to_string());
            }
        }
    }
}

#[test]
fn state_names_leave_stdlib_calls_intact() {
    let src = source(
        "state max = 7",
        "  derive computed = max(max, 5)\n",
        "    text `${max} ${computed} ${max(3, 5)}` testId=\"result\"",
    );
    assert_eq!(text_of(&boot(&src), "result"), "7 7 5");
}
