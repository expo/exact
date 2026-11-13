use exact_kernel::*;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    tree: Element,
    focus: Option<String>,
    key: Option<String>,
    axis: Option<String>,
    expected: Option<Expected>,
    stop: Option<String>,
}

#[derive(Deserialize)]
struct Expected {
    target: Option<String>,
    press: bool,
}

#[derive(Deserialize)]
struct Element {
    id: String,
    tag: Option<String>,
    contract: Option<String>,
    role: Option<String>,
    direction: Option<String>,
    visibility: Option<String>,
    #[serde(default)]
    attrs: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    children: Vec<Element>,
}

fn tree(element: &Element) -> (Kernel, BTreeMap<String, ViewId>) {
    fn add(e: &Element, ops: &mut Vec<Op>, ids: &mut BTreeMap<String, ViewId>) -> ViewId {
        let id = ids.len() as u32 + 1;
        assert!(ids.insert(e.id.clone(), id).is_none());
        let tag = e.tag.as_deref().unwrap_or("div");
        // A node a Contract element names is built as contract/lower builds
        // it (tags.rs, controls.rs `native_tag`); any other from its tag.
        let (node_type, semantic, fixed): (_, _, &[(PropId, &str)]) = match e.contract.as_deref() {
            None if matches!(tag, "button" | "a") => (NodeType::Pressable, Some(tag), &[]),
            None => (NodeType::View, Some(tag), &[]),
            Some("button") => (
                NodeType::Control,
                None,
                &[
                    (PropId::AccessibilityRole, "button"),
                    (PropId::Type, "button"),
                ],
            ),
            Some("button appearance=none") => (
                NodeType::Pressable,
                None,
                &[(PropId::AccessibilityRole, "button")],
            ),
            Some("link") => (
                NodeType::Pressable,
                None,
                &[(PropId::AccessibilityRole, "link")],
            ),
            Some("row") => (NodeType::View, None, &[]),
            Some("select") => (
                NodeType::Control,
                None,
                &[
                    (PropId::Type, "select"),
                    (PropId::AccessibilityRole, "combobox"),
                ],
            ),
            // A `text` in a `text` is an inline run of its paragraph.
            Some("text") => (NodeType::Text, None, &[]),
            Some(other) => panic!("unknown Contract element {other}"),
        };
        ops.push(Op::CreateView { id, node_type });
        let mut prop = |prop, value| ops.push(Op::SetProp { id, prop, value });
        if let Some(semantic) = semantic {
            prop(PropId::SemanticTag, PropValue::Str(semantic.into()));
        }
        for (p, value) in fixed {
            prop(*p, PropValue::Str((*value).into()));
        }
        if let Some(role) = &e.role {
            prop(
                PropId::AccessibilityRole,
                PropValue::Str(role.as_str().into()),
            );
        }
        for (name, value) in &e.attrs {
            let p = match name.as_str() {
                "disabled" => PropId::Disabled,
                "aria-disabled" => PropId::AccessibilityDisabled,
                "aria-checked" => PropId::AccessibilityChecked,
                "aria-selected" => PropId::AccessibilitySelected,
                "aria-orientation" => PropId::AccessibilityOrientation,
                "tabindex" => PropId::TabIndex,
                "inert" => PropId::Inert,
                "href" => PropId::Href,
                "hidden" => continue,
                _ => panic!("unknown case attribute {name}"),
            };
            let value = match value {
                serde_json::Value::Bool(v) => PropValue::Bool(*v),
                serde_json::Value::Number(v) => PropValue::Int(v.as_i64().unwrap()),
                serde_json::Value::String(v) => PropValue::Str(v.as_str().into()),
                _ => panic!("not a literal attribute: {value}"),
            };
            prop(p, value);
        }
        let mut style = StyleProps::default();
        if let Some(direction) = &e.direction {
            style
                .set_dynamic(
                    StyleId::Direction,
                    &StyleValue::Text(direction.as_str().into()),
                )
                .unwrap();
        }
        if let Some(visibility) = &e.visibility {
            style
                .set_dynamic(
                    StyleId::Visibility,
                    &StyleValue::Text(visibility.as_str().into()),
                )
                .unwrap();
        }
        if e.attrs.get("hidden").and_then(|v| v.as_bool()) == Some(true) {
            style
                .set_dynamic(StyleId::Display, &StyleValue::Text("none".into()))
                .unwrap();
        }
        ops.push(Op::SetStyle {
            id,
            patch: Box::new(style),
        });
        let children = e.children.iter().map(|e| add(e, ops, ids)).collect();
        ops.push(Op::SetChildren { id, children });
        id
    }
    let mut ops = Vec::new();
    let mut ids = BTreeMap::new();
    let root = add(element, &mut ops, &mut ids);
    ops.push(Op::AttachRoot { id: root });
    let mut kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
    kernel.apply(0, 1, &ops).unwrap();
    (kernel, ids)
}

fn cases() -> Vec<Case> {
    serde_json::from_str::<Cases>(include_str!("../../tables/role-key-cases.json"))
        .unwrap()
        .cases
}

#[test]
fn shared_role_key_cases() {
    let cases = cases();
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(16);
    let failures = std::thread::scope(|scope| {
        let handles: Vec<_> = cases
            .chunks(cases.len().div_ceil(threads))
            .map(|chunk| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for case in chunk {
                        let (kernel, ids) = tree(&case.tree);
                        let focused = case.focus.as_ref().map(|id| ids[id]);
                        let mismatch = if let Some(key) = &case.key {
                            let axis = case.axis.as_ref().map(|a| match a.as_str() {
                                "vertical" => RoleAxis::Vertical,
                                _ => panic!("unknown orientation override {a}"),
                            });
                            let answer = kernel.role_step(focused.unwrap(), key, axis, |_| true);
                            let want = case.expected.as_ref().unwrap();
                            let expected = want.target.as_ref().map(|id| (ids[id], want.press));
                            (answer != expected).then(|| {
                                format!("{}: got {answer:?}, expected {expected:?}", case.name)
                            })
                        } else {
                            let stops: Vec<_> = ids
                                .values()
                                .copied()
                                .filter(|id| {
                                    kernel.role_tab_stop(*id, focused, |_| true) == Some(true)
                                })
                                .collect();
                            let expected: Vec<_> = case.stop.iter().map(|id| ids[id]).collect();
                            (stops != expected).then(|| {
                                format!("{}: got stops {stops:?}, expected {expected:?}", case.name)
                            })
                        };
                        failures.extend(mismatch);
                    }
                    failures
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn host_focusability_inert_and_display_filter_destinations_and_entry() {
    let case = cases()
        .into_iter()
        .find(|c| c.name == "tablist unselected entry")
        .unwrap();
    let (mut kernel, ids) = tree(&case.tree);
    let (a, b, c) = (ids["a"], ids["b"], ids["c"]);
    assert_eq!(
        kernel.role_step(a, "ArrowRight", None, |id| id != b),
        Some((c, false))
    );
    assert_eq!(kernel.role_tab_stop(b, None, |id| id != a), Some(true));
    assert_eq!(kernel.role_tab_stop(a, Some(a), |_| false), Some(true));
    for prop in [
        PropId::Inert,
        PropId::Disabled,
        PropId::AccessibilityDisabled,
    ] {
        kernel
            .apply(
                1,
                2,
                &[Op::SetProp {
                    id: b,
                    prop,
                    value: PropValue::Bool(true),
                }],
            )
            .unwrap();
        assert_eq!(
            kernel.role_step(a, "ArrowRight", None, |_| true),
            Some((c, false)),
            "{prop:?}"
        );
        assert_eq!(
            kernel.role_step(b, "ArrowRight", None, |_| true),
            Some((c, false)),
            "{prop:?}"
        );
        kernel
            .apply(
                2,
                3,
                &[Op::SetProp {
                    id: b,
                    prop,
                    value: PropValue::Bool(false),
                }],
            )
            .unwrap();
    }
    kernel
        .apply(
            3,
            4,
            &[Op::SetProp {
                id: ids["group"],
                prop: PropId::Inert,
                value: PropValue::Bool(true),
            }],
        )
        .unwrap();
    assert_eq!(
        kernel.role_step(a, "ArrowRight", None, |_| true),
        Some((a, false))
    );
    assert_eq!(kernel.role_tab_stop(a, None, |_| true), Some(false));
    kernel
        .apply(
            4,
            5,
            &[Op::SetProp {
                id: ids["group"],
                prop: PropId::Inert,
                value: PropValue::Bool(false),
            }],
        )
        .unwrap();
    let mut hidden = StyleProps::default();
    hidden
        .set_dynamic(StyleId::Visibility, &StyleValue::Text("hidden".into()))
        .unwrap();
    kernel
        .apply(
            5,
            6,
            &[Op::SetStyle {
                id: b,
                patch: Box::new(hidden),
            }],
        )
        .unwrap();
    assert_eq!(
        kernel.role_step(a, "ArrowRight", None, |_| true),
        Some((c, false))
    );
    let mut hidden = StyleProps::default();
    hidden
        .set_dynamic(StyleId::Display, &StyleValue::Text("none".into()))
        .unwrap();
    kernel
        .apply(
            6,
            7,
            &[Op::SetStyle {
                id: ids["group"],
                patch: Box::new(hidden),
            }],
        )
        .unwrap();
    assert_eq!(
        kernel.role_step(a, "ArrowRight", None, |_| true),
        Some((a, false))
    );
    assert_eq!(kernel.role_tab_stop(a, None, |_| true), Some(false));
}

/// A native radio's role is the lowering's (`contract/lower/src/controls.rs`):
/// in a `radiogroup` it keeps HTML's group and keys, and is no painted member.
#[test]
fn native_radios_in_a_radiogroup_are_no_painted_members() {
    let mut ops = vec![
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        Op::SetProp {
            id: 1,
            prop: PropId::AccessibilityRole,
            value: PropValue::Str("radiogroup".into()),
        },
    ];
    for id in 2..=4 {
        ops.push(Op::CreateView {
            id,
            node_type: NodeType::Control,
        });
        for (prop, value) in [
            (PropId::Type, PropValue::Str("radio".into())),
            (PropId::AccessibilityRole, PropValue::Str("radio".into())),
            (PropId::Name, PropValue::Str("choice".into())),
            (PropId::Checked, PropValue::Bool(id == 3)),
        ] {
            ops.push(Op::SetProp { id, prop, value });
        }
        ops.push(Op::SetChildren {
            id,
            children: vec![],
        });
    }
    ops.push(Op::SetChildren {
        id: 1,
        children: vec![2, 3, 4],
    });
    ops.push(Op::AttachRoot { id: 1 });
    let mut kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
    kernel.apply(0, 1, &ops).unwrap();
    for id in 2..=4 {
        for key in [
            "ArrowRight",
            "ArrowLeft",
            "ArrowDown",
            "ArrowUp",
            "Home",
            "End",
        ] {
            assert_eq!(
                kernel.role_step(id, key, None, |_| true),
                None,
                "{id} {key}"
            );
            assert_eq!(kernel.role_step_order(id, key, None), None, "{id} {key}");
        }
        for focused in [None, Some(3), Some(id)] {
            assert_eq!(kernel.role_tab_stop(id, focused, |_| true), None, "{id}");
            assert_eq!(kernel.role_tab_order(id, focused), None, "{id}");
        }
    }
    assert_eq!(kernel.radio_group(3), vec![2, 3, 4]);
    assert_eq!(kernel.radio_step(3, true), Some(4));
}
