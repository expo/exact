use super::*;
use crate::{wire::Op, StyleProps, StyleValue};

fn style(id: ViewId, values: &[(StyleId, &str)], authored: bool) -> Op {
    let mut patch = StyleProps::default();
    for &(key, value) in values {
        let value = value
            .parse()
            .map_or_else(|_| StyleValue::Text(value.into()), StyleValue::Number);
        patch.set_dynamic(key, &value).unwrap();
    }
    if authored {
        patch.written = patch.mask;
    }
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

fn row(
    column: bool,
    lines: usize,
    target: ViewId,
    values: &[(StyleId, &str)],
    authored: bool,
) -> GroupedRow {
    let mut k = Kernel::with_monospace();
    let mut ops = vec![];
    for (id, node_type) in [
        (1, NodeType::Pressable),
        (2, NodeType::View),
        (3, NodeType::Text),
        (4, NodeType::Text),
        (5, NodeType::Control),
    ] {
        ops.push(Op::CreateView { id, node_type });
    }
    ops.extend([
        style(
            1,
            &[
                (StyleId::Display, "flex"),
                (
                    StyleId::FlexDirection,
                    if column { "column" } else { "row" },
                ),
            ],
            false,
        ),
        style(
            2,
            &[
                (StyleId::Display, "flex"),
                (StyleId::FlexDirection, "column"),
                (StyleId::FlexGrow, "1"),
            ],
            false,
        ),
        Op::SetProp {
            id: 3,
            prop: PropId::Text,
            value: "Puzzle".into(),
        },
        Op::SetProp {
            id: 4,
            prop: PropId::Text,
            value: "A tile".into(),
        },
        Op::SetProp {
            id: 5,
            prop: PropId::Type,
            value: "checkbox".into(),
        },
        Op::SetChildren {
            id: if column { 1 } else { 2 },
            children: (3..3 + lines as u32).collect(),
        },
    ]);
    if !column {
        ops.push(Op::SetChildren {
            id: 1,
            children: vec![2, 5],
        });
    }
    ops.extend([style(target, values, authored), Op::AttachRoot { id: 1 }]);
    k.apply(0, 1, &ops).unwrap();
    k.grouped_row(&k.node(1).unwrap())
}

#[test]
fn authored_row_and_subtitle_boxes_keep_their_views() {
    for lines in [1, 2] {
        for (property, value) in [
            (StyleId::Height, "300"),
            (StyleId::BackgroundColor, "transparent"),
            (StyleId::BorderRadiusTopLeft, "16"),
            (StyleId::BorderRadiusTopRight, "16"),
            (StyleId::BorderRadiusBottomLeft, "16"),
            (StyleId::BorderRadiusBottomRight, "16"),
        ] {
            for (column, target) in [(false, 1), (false, 2), (true, 1)] {
                let r = row(column, lines, target, &[(property, value)], true);
                assert!(
                    r.custom,
                    "{property:?}, column row={column}, target={target}, lines={lines}"
                );
                assert!(r.pressable);
                assert_eq!(
                    (r.title, r.subtitle, r.accessory),
                    (None, false, Accessory::None)
                );
            }
        }
        for (property, value) in [
            (StyleId::MinHeight, "300"),
            (StyleId::AlignItems, "center"),
            (StyleId::JustifyContent, "center"),
        ] {
            assert!(
                row(false, lines, 2, &[(property, value)], true).custom,
                "column {property:?}"
            );
        }
    }
}

#[test]
fn sheet_boxes_and_a_rows_minimum_height_keep_standard_cells() {
    for column in [false, true] {
        let r = row(column, 2, 1, &[(StyleId::MinHeight, "80")], true);
        assert!(!r.custom);
        assert!(r.subtitle);
        assert_eq!(r.title.as_deref(), Some("Puzzle"));
        assert_eq!(
            r.accessory,
            if column {
                Accessory::None
            } else {
                Accessory::Toggle(5)
            }
        );
        for target in [1, 2] {
            let r = row(
                column,
                2,
                target,
                &[
                    (StyleId::Height, "300"),
                    (StyleId::MinHeight, "300"),
                    (StyleId::BackgroundColor, "#abcdef"),
                    (StyleId::BorderRadiusTopLeft, "16"),
                    (StyleId::AlignItems, "center"),
                    (StyleId::JustifyContent, "center"),
                ],
                false,
            );
            assert!(!r.custom, "the platform's sheet does not author a box");
        }
    }
}

#[test]
fn custom_radio_rows_keep_their_selection_target_and_checked_state() {
    for (property, value) in [
        (StyleId::Height, "80"),
        (StyleId::BackgroundColor, "transparent"),
        (StyleId::BorderRadiusTopLeft, "16"),
    ] {
        for children in [vec![2, 3], vec![3, 2]] {
            for checked in [false, true] {
                let mut k = Kernel::with_monospace();
                k.apply(
                    0,
                    1,
                    &[
                        Op::CreateView {
                            id: 1,
                            node_type: NodeType::View,
                        },
                        Op::CreateView {
                            id: 2,
                            node_type: NodeType::Text,
                        },
                        Op::CreateView {
                            id: 3,
                            node_type: NodeType::Control,
                        },
                        Op::SetProp {
                            id: 2,
                            prop: PropId::Text,
                            value: "Choice".into(),
                        },
                        Op::SetProp {
                            id: 3,
                            prop: PropId::Type,
                            value: "radio".into(),
                        },
                        Op::SetProp {
                            id: 3,
                            prop: PropId::Checked,
                            value: checked.into(),
                        },
                        Op::SetChildren {
                            id: 1,
                            children: children.clone(),
                        },
                        style(1, &[(property, value)], true),
                        Op::AttachRoot { id: 1 },
                    ],
                )
                .unwrap();
                let row = k.grouped_row(&k.node(1).unwrap());
                assert!(row.custom);
                assert!(!row.pressable, "the row itself has no press handler");
                assert_eq!(row.accessory, Accessory::Radio(3, checked));
                assert_eq!(row.title, None, "the carried content owns the label");
            }
        }
    }
}
