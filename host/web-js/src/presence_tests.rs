//! Computed presence declarations use the kernel’s values and paint policy.
use exact_kernel::{NodeType, StyleId};
use exact_plan::BindingKind;

/// Values a binding can push: curves, a spring, `none`, a transition of no
/// layout, and ones that clear the row (lowering refuses a literal the kernel
/// would).
const VALUES: [&str; 11] = [
    "none",
    "260ms ease",
    "320ms cubic-bezier(.32,.72,0,1)",
    "-exact-spring(300, 24, 1)",
    "opacity 1s",
    "d 1s",
    "d 1s, 200ms ease",
    "all 1s, d 2s",
    "1s 200ms linear",
    "",
    "unset",
];
/// Strings supplied at runtime, including spellings absent from the plan.
const COMPUTED: [&str; 5] = ["bogus", "260ms  ease", "d 1s,  200ms ease", "  ", " UNSET"];

/// Whether the runner holds the row for `v`: not cleared before parsing
/// (`bridge::unsets`), and parsed.
fn runner_holds(v: &str) -> bool {
    let mut p = exact_kernel::StyleProps::default();
    !exact_runner::bridge::unsets(StyleId::LayoutTransition as u16, &exact_plan::Value::str(v))
        && p.set_dynamic(
            StyleId::LayoutTransition,
            &exact_kernel::StyleValue::Text(v.into()),
        )
        .is_ok()
        && p.mask.has(StyleId::LayoutTransition)
}

fn plan() -> exact_plan::Plan {
    let mut chain = String::from("\"none\"");
    for (k, v) in VALUES.iter().enumerate().rev() {
        chain = format!("(k == {k} ? {v:?} : {chain})");
    }
    contract::compile(&format!(
        "component App\n  state k = 0\n  action next\n    k = k + 1\n  view\n    column testId=\"c\" -exact-layout-transition={chain}\n      button press=next testId=\"b\"\n        text \"Next\"\n"
    ))
    .unwrap()
}

fn bun(script: &str) -> serde_json::Value {
    use std::io::Write;
    use std::process::Stdio;
    let mut child =
        std::process::Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bun");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn row(plan: &exact_plan::Plan, id: StyleId) -> exact_plan::BindingsRow {
    node_row(plan, id).1
}

fn node_row(plan: &exact_plan::Plan, id: StyleId) -> (NodeType, exact_plan::BindingsRow) {
    plan.nodes
        .iter()
        .flat_map(|n| {
            n.bindings
                .iter()
                .map(|b| (NodeType::from_wire(n.node_type).unwrap(), plan.binding(b)))
                .collect::<Vec<_>>()
        })
        .find(|(_, b)| b.kind == BindingKind::Style && b.id == id as u16)
        .expect("the bound row")
}

#[test]
fn a_bound_layout_transition_compiles_and_is_followed() {
    let js = crate::emit::emit(&plan(), false, false).unwrap().js;
    assert!(js.contains("\"--exact-layout-transition\""), "{js}");
    assert!(js.contains(";lt(e0,$$t"), "{js}");
}

#[test]
fn named_and_computed_layout_values_write_what_the_kernel_writes() {
    let mut expected = VALUES
        .map(|v| {
            serde_json::json!(runner_holds(v)
                .then(|| crate::presence::layout_transition(v).unwrap())
                .flatten())
        })
        .to_vec();
    let map = crate::presence::layout_transition_map(&crate::paint::transitions(), "7", "say");
    let inputs: Vec<serde_json::Value> = VALUES
        .iter()
        .map(|v| serde_json::json!(v))
        .chain(COMPUTED.iter().map(|v| serde_json::json!(v)))
        .chain([serde_json::json!(5), serde_json::Value::Null])
        .collect();
    let got = bun(&format!(
        "const journal=[];const say=l=>journal.push(l);const m={map};console.log(JSON.stringify({{out:{}.map(m),journal}}))",
        serde_json::Value::Array(inputs)
    ));
    expected.extend(COMPUTED.map(|v| {
        serde_json::json!(runner_holds(v)
            .then(|| crate::presence::layout_transition(v).unwrap())
            .flatten())
    }));
    expected.extend([serde_json::Value::Null, serde_json::Value::Null]);
    assert_eq!(got["out"], serde_json::Value::Array(expected));
    assert_eq!(crate::presence::layout_transition("bogus"), None);
    assert!(runner_holds("260ms  ease"));
    assert!(!runner_holds("  ") && !runner_holds(" UNSET"));
    let why = "; unset";
    assert_eq!(
        got["journal"],
        serde_json::json!([
            format!("view 7: invalid layout-transition value \"bogus\"{why}"),
            format!("view 7: invalid layout-transition value 5{why}"),
        ])
    );
}

#[test]
fn computed_layout_transitions_match_the_kernel_grammar_and_css() {
    let mut values = crate::paint_tests::transition_values();
    values.extend(
        [
            "260ms  ease",
            "\u{85}260ms ease\u{85}",
            "\u{feff}260ms ease",
            "all 1s, opacity 2s",
            "all 1s, all 2s",
            "all 1s, bogus 2s",
            " UNSET ",
            "0.000000123456789s",
            "1e35s",
            "1e40s",
            "1s cubic-bezier(0, 1e40, 1, -1e-12)",
        ]
        .map(str::to_owned),
    );
    let parser = crate::paint::transitions();
    let actual = bun(&format!(
        "const parse={parser};console.log(JSON.stringify({}.map(v=>{{const t=parse(v);return [t?.held??false,t?.css??null]}})))",
        serde_json::json!(values)
    ));
    let mut disagreements = Vec::new();
    for (value, actual) in values.iter().zip(actual.as_array().unwrap()) {
        let held = runner_holds(value);
        let css = held
            .then(|| crate::presence::layout_transition(value).unwrap())
            .flatten();
        let expected = serde_json::json!([held, css]);
        if *actual != expected {
            disagreements.push(format!("{value:?}: JS {actual}, kernel {expected}"));
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} disagreements:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

/// Paint's layout bit (`own_from`'s `holds_layout_transition`, flag 256,
/// with the policy bit 4) is set where the kernel's mask holds the row: a
/// parsed value, `none` included; never a blank, `unset` or invalid value.
#[test]
fn the_paint_flag_is_set_where_the_kernel_holds_the_row() {
    let plan = plan();
    let (kind, b) = node_row(&plan, StyleId::LayoutTransition);
    let (_, test) =
        crate::paint::binding(&plan, kind, &b, crate::paint::transitions).expect("a paint flag");
    let mut expected: Vec<u32> = VALUES
        .map(|v| if runner_holds(v) { 260 } else { 0 })
        .to_vec();
    expected.extend(COMPUTED.map(|v| if runner_holds(v) { 260 } else { 0 }));
    expected.extend([0, 0]);
    let got = bun(&format!(
        "console.log(JSON.stringify([...{},...{},5,null].map(v=>{test})))",
        serde_json::to_string(&VALUES).unwrap(),
        serde_json::to_string(&COMPUTED).unwrap()
    ));
    assert_eq!(got, serde_json::json!(expected));
}

const EXITS: [&str; 6] = [
    "none",
    "leave 200ms both",
    "leave 200ms 50ms ease-out",
    "__proto__",
    "",
    "unset",
];

fn exit_plan() -> exact_plan::Plan {
    exact_web::link(exact_web_capabilities::ALL);
    let mut value = String::from("\"none\"");
    for (k, text) in EXITS.iter().enumerate().rev() {
        value = format!("(k == {k} ? {text:?} : {value})");
    }
    contract::compile(&format!(
        "keyframes leave\n  to opacity=0 scale=0.75\nkeyframes __proto__\n  to opacity=0\n\ncomponent App\n  state k = 0\n  action next\n    k = k + 1\n  view\n    column press=next -exact-press-scale=0.97 -exact-exit-animation={value}\n      button \"Next\" press=next\n"
    )).unwrap()
}

fn native_exit(plan: &exact_plan::Plan, value: &str, press: bool) -> (bool, Option<String>) {
    let mut style = exact_kernel::StyleProps::default();
    if exact_runner::bridge::unsets(
        StyleId::ExitAnimation as u16,
        &exact_plan::Value::str(value),
    ) || style
        .set_dynamic(
            StyleId::ExitAnimation,
            &exact_kernel::StyleValue::Text(value.into()),
        )
        .is_err()
    {
        return (false, None);
    }
    let _ = exact_runner::bridge::keyframes(plan).resolve(&mut style.rare.exit_animation);
    if press {
        style
            .set_dynamic(StyleId::PressScale, &exact_kernel::StyleValue::Number(0.97))
            .unwrap();
    }
    let (css, _) = exact_web::css::css_text(&style, &[]);
    (
        style.mask.has(StyleId::ExitAnimation),
        css.split(';')
            .find_map(|decl| decl.strip_prefix("--exact-exit-animation:"))
            .map(str::to_string),
    )
}

#[test]
fn exit_values_write_native_css_and_clear_unlisted_values() {
    let plan = exit_plan();
    crate::emit::emit(&plan, false, false).expect("a computed exit builds");
    assert!(native_exit(&plan, "leave  200ms both", false).0);
    let inputs: Vec<_> = EXITS
        .iter()
        .map(|v| serde_json::json!(v))
        .chain([
            serde_json::json!("leave  200ms both"),
            serde_json::json!("bogus"),
            serde_json::json!("leave 1s infinite"),
            serde_json::json!(5),
            serde_json::Value::Null,
            serde_json::json!(" UNSET"),
        ])
        .collect();
    for press in [false, true] {
        let values = crate::presence::exit_animations(
            &plan,
            plan.code(row(&plan, StyleId::ExitAnimation).expr),
            press,
        );
        let map = crate::presence::declaration_map(&values, "7", "say", "exit-animation");
        let got = bun(&format!("const journal=[];const say=v=>journal.push(v);const m={map};console.log(JSON.stringify({{values:{}.map(m),journal}}))", serde_json::json!(inputs)));
        let mut expected: Vec<_> = EXITS
            .iter()
            .map(|v| serde_json::json!(native_exit(&plan, v, press).1))
            .collect();
        expected.extend(std::iter::repeat_n(serde_json::Value::Null, 6));
        assert_eq!(got["values"], serde_json::json!(expected));
        assert_eq!(got["journal"].as_array().unwrap().len(), 4);
        assert_eq!(values.held["none"], None);
        assert!(values.held["__proto__"].is_some());
    }
}

#[test]
fn an_exit_paint_flag_follows_the_kernel_mask() {
    let plan = exit_plan();
    let (kind, binding) = node_row(&plan, StyleId::ExitAnimation);
    let (_, flag) =
        crate::paint::binding(&plan, kind, &binding, crate::paint::transitions).unwrap();
    let got = bun(&format!(
        "const f=v=>({flag});console.log(JSON.stringify({}.map(f)))",
        serde_json::json!(EXITS)
    ));
    let expected: Vec<_> = EXITS
        .iter()
        .map(|v| if native_exit(&plan, v, false).0 { 4 } else { 0 })
        .collect();
    assert_eq!(got, serde_json::json!(expected));
}
