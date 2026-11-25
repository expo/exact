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
    k.grouped_row(&k.node(1).unwrap(), &|_| false)
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
fn a_rows_markdown_text_keeps_its_views() {
    for column in [false, true] {
        for lines in [1, 2] {
            for markdown in 3..3 + lines as ViewId {
                let mut k = Kernel::with_monospace();
                let mut ops = vec![];
                for (id, node_type) in [
                    (1, NodeType::View),
                    (3, NodeType::Text),
                    (4, NodeType::Text),
                ] {
                    ops.push(Op::CreateView { id, node_type });
                }
                if column {
                    ops.push(style(
                        1,
                        &[
                            (StyleId::Display, "flex"),
                            (StyleId::FlexDirection, "column"),
                        ],
                        false,
                    ));
                }
                ops.extend([
                    Op::SetProp {
                        id: 3,
                        prop: PropId::Text,
                        value: "A **bold** word".into(),
                    },
                    Op::SetProp {
                        id: 4,
                        prop: PropId::Text,
                        value: "Detail".into(),
                    },
                    Op::SetProp {
                        id: markdown,
                        prop: PropId::Markup,
                        value: "markdown".into(),
                    },
                    Op::SetChildren {
                        id: 1,
                        children: (3..3 + lines as ViewId).collect(),
                    },
                    Op::AttachRoot { id: 1 },
                ]);
                k.apply(0, 1, &ops).unwrap();
                let r = k.grouped_row(&k.node(1).unwrap(), &|_| false);
                assert!(
                    r.custom,
                    "column={column}, lines={lines}, markdown text={markdown}"
                );
                assert_eq!((r.title, r.secondary), (None, None));
            }
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
                let row = k.grouped_row(&k.node(1).unwrap(), &|_| false);
                assert!(row.custom);
                assert!(!row.pressable, "the row itself has no press handler");
                assert_eq!(row.accessory, Accessory::Radio(3, checked));
                assert_eq!(row.title, None, "the carried content owns the label");
            }
        }
    }
}

#[test]
fn a_standard_rows_switch_is_its_toggle_only_in_a_grouped_list() {
    for list_style in [Some("inset-grouped"), None] {
        let mut k = Kernel::with_monospace();
        let mut ops = vec![];
        for (id, node_type) in [
            (1, NodeType::View),
            (2, NodeType::View),
            (3, NodeType::View),
            (4, NodeType::View),
            (5, NodeType::Text),
            (6, NodeType::Control),
            (7, NodeType::View),
            (8, NodeType::View),
            (9, NodeType::Control),
        ] {
            ops.push(Op::CreateView { id, node_type });
        }
        if let Some(value) = list_style {
            ops.push(Op::SetProp {
                id: 1,
                prop: PropId::ListStyle,
                value: value.into(),
            });
        }
        let prop = |id, prop, value: &str| Op::SetProp {
            id,
            prop,
            value: value.into(),
        };
        let children = |id, children: &[ViewId]| Op::SetChildren {
            id,
            children: children.to_vec(),
        };
        ops.extend([
            prop(5, PropId::Text, "Dark mode"),
            prop(6, PropId::Type, "checkbox"),
            prop(9, PropId::Type, "checkbox"),
            prop(7, PropId::SemanticTag, "header"),
            // A section of a header holding a row with a checkbox, then its
            // group of one row: a text and a checkbox.
            children(1, &[2]),
            children(2, &[7, 3]),
            children(3, &[4]),
            children(4, &[5, 6]),
            children(7, &[8]),
            children(8, &[9]),
            style(4, &[(StyleId::Display, "flex")], false),
            Op::AttachRoot { id: 1 },
        ]);
        k.apply(0, 1, &ops).unwrap();
        let grouped = list_style.is_some();
        assert_eq!(
            k.grouped_toggle(4, &|_| false),
            grouped.then_some(6),
            "{list_style:?}"
        );
        for other in [1, 2, 3, 5, 6, 8] {
            assert_eq!(
                k.grouped_toggle(other, &|_| false),
                None,
                "{other} is no row of a section's group"
            );
        }
    }
}

#[test]
fn markdown_rows_keep_their_accessories() {
    // Whether the row is a column, its children, and its nested column's.
    let shapes: [(bool, &[ViewId], &[ViewId]); 4] = [
        (false, &[5, 3], &[]),
        (false, &[3, 5], &[]),
        (false, &[3, 4, 5], &[]),
        (false, &[2, 5], &[3, 4]),
    ];
    for (kind, accessory_type, expected) in [
        (NodeType::Control, "checkbox", Accessory::Toggle(5)),
        (NodeType::Control, "radio", Accessory::Radio(5, true)),
        (NodeType::Image, "chevron.forward", Accessory::Disclosure),
        (NodeType::Image, "checkmark", Accessory::Checkmark),
        (NodeType::Pressable, "detail", Accessory::Detail(5)),
    ] {
        for (column, parts, lines) in shapes {
            if parts.first() == Some(&5) && accessory_type != "radio" {
                continue;
            }
            for markdown in [None, Some(3), Some(4)] {
                let mut k = Kernel::with_monospace();
                let mut ops = vec![];
                for (id, node_type) in [
                    (1, NodeType::View),
                    (2, NodeType::View),
                    (3, NodeType::Text),
                    (4, NodeType::Text),
                    (5, kind),
                    (6, NodeType::Image),
                    (10, NodeType::List),
                    (11, NodeType::View),
                    (12, NodeType::View),
                ] {
                    ops.push(Op::CreateView { id, node_type });
                }
                let direction = if column { "column" } else { "row" };
                let prop = |id, prop, value: &str| Op::SetProp {
                    id,
                    prop,
                    value: value.into(),
                };
                ops.extend([
                    style(
                        1,
                        &[
                            (StyleId::Display, "flex"),
                            (StyleId::FlexDirection, direction),
                        ],
                        false,
                    ),
                    style(
                        2,
                        &[
                            (StyleId::Display, "flex"),
                            (StyleId::FlexDirection, "column"),
                        ],
                        false,
                    ),
                    prop(3, PropId::Text, "A **bold** title"),
                    prop(4, PropId::Text, "A *slanted* line"),
                    prop(5, PropId::Type, accessory_type),
                    Op::SetProp {
                        id: 5,
                        prop: PropId::Checked,
                        value: true.into(),
                    },
                    prop(
                        5,
                        PropId::ImageSource,
                        &format!("symbol:sf/{accessory_type}"),
                    ),
                    prop(6, PropId::ImageSource, "symbol:sf/info.circle"),
                    prop(10, PropId::ListStyle, "inset-grouped"),
                    Op::SetChildren {
                        id: 10,
                        children: vec![11],
                    },
                    Op::SetChildren {
                        id: 11,
                        children: vec![12],
                    },
                    Op::SetChildren {
                        id: 12,
                        children: vec![1],
                    },
                    Op::SetChildren {
                        id: 2,
                        children: lines.to_vec(),
                    },
                    Op::SetChildren {
                        id: 1,
                        children: parts.to_vec(),
                    },
                ]);
                if accessory_type == "detail" {
                    ops.push(Op::SetChildren {
                        id: 5,
                        children: vec![6],
                    });
                }
                if let Some(id) = markdown {
                    ops.push(prop(id, PropId::Markup, "markdown"));
                }
                ops.push(Op::AttachRoot { id: 10 });
                k.apply(0, 1, &ops).unwrap();
                let row = k.grouped_row(&k.node(1).unwrap(), &|_| false);
                let shown = markdown.is_some_and(|id| parts.contains(&id) || lines.contains(&id));
                let note = format!("column row={column}, parts={parts:?}, Markdown={markdown:?}, accessory={accessory_type}");
                assert_eq!(row.custom, shown, "{note}");
                let title = (!shown).then_some("A **bold** title");
                assert_eq!(row.title.as_deref(), title, "{note}");
                let accessory = if parts.contains(&5) {
                    expected
                } else {
                    Accessory::None
                };
                assert_eq!(row.accessory, accessory, "the cell's accessory: {note}");
                assert_eq!(
                    k.grouped_toggle(1, &|_| false),
                    (parts.contains(&5) && accessory_type == "checkbox").then_some(5),
                    "{note}"
                );
                if shown && parts.contains(&5) {
                    // A platform cover removes only the replaced accessory's
                    // flex slot. The model still names it on the next read.
                    k.set_host_cover(5, Some(crate::HostCover::Whole)).unwrap();
                    k.set_host_cover(1, Some(crate::HostCover::Edges([0.0, 40.0, 0.0, 0.0])))
                        .unwrap();
                    k.compute_layout(10, crate::Offer::definite(320.0, 600.0))
                        .unwrap();
                    assert_eq!(k.node(5).unwrap().frame.width, 0.0, "{note}");
                    assert_eq!(
                        k.grouped_row(&k.node(1).unwrap(), &|_| false)
                            .accessory_view,
                        Some(5)
                    );
                    k.set_host_cover(5, None).unwrap();
                    k.set_host_cover(1, None).unwrap();
                    k.apply(1, 2, &[style(1, &[(StyleId::Height, "80")], true)])
                        .unwrap();
                    let authored = k.grouped_row(&k.node(1).unwrap(), &|_| false);
                    assert!(authored.custom);
                    assert_eq!(
                        authored.accessory,
                        if accessory_type == "radio" {
                            expected
                        } else {
                            Accessory::None
                        },
                        "an authored box keeps its control"
                    );
                    assert_eq!(k.grouped_toggle(1, &|_| false), None);
                }
            }
        }
    }
}

/// A text that is more than one label (inline runs, a link, a press of its
/// own) keeps its row's parts as they are written, whatever the accessory
/// and whether or not the title is Markdown. One that is not shown
/// (`display: none`) changes nothing: the row is the standard row it is
/// without it.
#[test]
fn a_text_that_stands_alone_keeps_each_part_of_its_row() {
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Alone {
        No,
        Run,
        Link,
        Press,
    }
    for (accessory_type, kind, expected) in [
        ("checkbox", NodeType::Control, Accessory::Toggle(5)),
        ("radio", NodeType::Control, Accessory::Radio(5, false)),
        ("chevron.forward", NodeType::Image, Accessory::Disclosure),
    ] {
        for markdown in [false, true] {
            for alone in [Alone::No, Alone::Run, Alone::Link, Alone::Press] {
                // The title is the text that stands alone, or the value beside it is.
                // `hidden`: what would stand alone is a `display: none` run of that text.
                for (text, hidden) in [(3, false), (4, false), (3, true), (4, true)] {
                    if hidden && alone == Alone::No {
                        continue;
                    }
                    let own = if hidden { 6 } else { text };
                    let mut k = Kernel::with_monospace();
                    let mut ops = vec![];
                    for (id, node_type) in [
                        (1, NodeType::View),
                        (2, NodeType::View),
                        (3, NodeType::Text),
                        (4, NodeType::Text),
                        (5, kind),
                        (6, NodeType::Text),
                        (10, NodeType::List),
                        (11, NodeType::View),
                    ] {
                        ops.push(Op::CreateView { id, node_type });
                    }
                    let mut props = vec![
                        (3, PropId::Text, "**Home**"),
                        (4, PropId::Text, "Help"),
                        (5, PropId::Type, accessory_type),
                        (5, PropId::ImageSource, "symbol:sf/chevron.forward"),
                        (6, PropId::Text, "Documentation"),
                        (10, PropId::ListStyle, "inset-grouped"),
                    ];
                    if markdown {
                        props.push((3, PropId::Markup, "markdown"));
                    }
                    if alone == Alone::Link {
                        props.push((own, PropId::Href, "https://example.com"));
                    }
                    for (id, prop, value) in props {
                        ops.push(Op::SetProp {
                            id,
                            prop,
                            value: value.into(),
                        });
                    }
                    let mut children = vec![
                        (10, vec![11]),
                        (11, vec![2]),
                        (2, vec![1]),
                        (1, vec![3, 4, 5]),
                    ];
                    if alone == Alone::Run || hidden {
                        children.push((text, vec![6]));
                    }
                    if hidden {
                        ops.push(style(6, &[(StyleId::Display, "none")], true));
                    }
                    for (id, children) in children {
                        ops.push(Op::SetChildren { id, children });
                    }
                    ops.push(Op::AttachRoot { id: 10 });
                    k.apply(0, 1, &ops).unwrap();
                    let acts = |id: ViewId| alone == Alone::Press && id == own;
                    let row = k.grouped_row(&k.node(1).unwrap(), &acts);
                    let note = format!(
                        "{accessory_type}, Markdown={markdown}, {alone:?} on {text}, hidden={hidden}"
                    );
                    let alone = if hidden { Alone::No } else { alone };
                    assert_eq!(row.custom, markdown || alone != Alone::No, "{note}");
                    if alone == Alone::No {
                        // Markdown alone keeps the text and the platform's accessory.
                        assert_eq!(row.accessory, expected, "{note}");
                        assert_eq!(row.accessory_view, Some(5), "{note}");
                        assert_eq!(
                            k.grouped_toggle(1, &acts),
                            (accessory_type == "checkbox").then_some(5),
                            "{note}"
                        );
                        continue;
                    }
                    // Its row's cell replaces no part: only a radio's target stays,
                    // which a tap on the row checks.
                    let kept = match expected {
                        radio @ Accessory::Radio(..) => radio,
                        _ => Accessory::None,
                    };
                    assert_eq!(row.accessory, kept, "{note}");
                    assert_eq!(row.accessory_view, None, "{note}");
                    assert_eq!(k.grouped_toggle(1, &acts), None, "{note}");
                    let list = k.grouped_list_with(10, &acts).unwrap();
                    assert_eq!(list.sections[0].rows[0], row, "{note}");
                    if alone == Alone::Press {
                        // A host that does not say so sees one label.
                        assert_eq!(
                            k.grouped_list(10).unwrap().sections[0].rows[0].accessory_view,
                            Some(5)
                        );
                    }
                }
            }
        }
    }
}

/// An image with a press of its own keeps its row's parts as they are
/// written, whether it is where the accessory or the leading symbol would
/// be and whether or not the title is Markdown: a cell's chevron or icon
/// would press nothing. One that is not shown changes nothing.
#[test]
fn an_image_with_a_press_of_its_own_keeps_each_part_of_its_row() {
    for markdown in [false, true] {
        // The image is the row's last part (4), or its first (2).
        for image in [4, 2] {
            for (presses, hidden) in [(false, false), (true, false), (true, true)] {
                let mut k = Kernel::with_monospace();
                let mut ops = vec![];
                for (id, node_type) in [
                    (1, NodeType::Pressable),
                    (image, NodeType::Image),
                    (3, NodeType::Text),
                    (10, NodeType::List),
                    (11, NodeType::View),
                    (12, NodeType::View),
                ] {
                    ops.push(Op::CreateView { id, node_type });
                }
                let mut props = vec![
                    (3, PropId::Text, "**Profile**"),
                    (image, PropId::ImageSource, "symbol:sf/chevron.right"),
                    (10, PropId::ListStyle, "inset-grouped"),
                ];
                if markdown {
                    props.push((3, PropId::Markup, "markdown"));
                }
                for (id, prop, value) in props {
                    ops.push(Op::SetProp {
                        id,
                        prop,
                        value: value.into(),
                    });
                }
                if hidden {
                    ops.push(style(image, &[(StyleId::Display, "none")], true));
                }
                let parts = if image == 4 { vec![3, 4] } else { vec![2, 3] };
                for (id, children) in [(10, vec![11]), (11, vec![12]), (12, vec![1]), (1, parts)] {
                    ops.push(Op::SetChildren { id, children });
                }
                ops.push(Op::AttachRoot { id: 10 });
                k.apply(0, 1, &ops).unwrap();
                let acts = |id: ViewId| presses && id == image;
                let row = k.grouped_row(&k.node(1).unwrap(), &acts);
                let note = format!(
                    "image {image}, Markdown={markdown}, presses={presses}, hidden={hidden}"
                );
                let alone = presses && !hidden;
                assert_eq!(row.custom, markdown || alone, "{note}");
                assert_eq!(
                    k.grouped_list_with(10, &acts).unwrap().sections[0].rows[0],
                    row
                );
                if alone {
                    // The cell replaces no part of it.
                    assert_eq!(row.accessory, Accessory::None, "{note}");
                    assert_eq!(row.accessory_view, None, "{note}");
                    assert_eq!(row.symbol, None, "{note}");
                    continue;
                }
                let shown = !hidden;
                let (accessory, view) = if image == 4 && shown {
                    (Accessory::Disclosure, Some(4))
                } else {
                    (Accessory::None, None)
                };
                assert_eq!(row.accessory, accessory, "{note}");
                assert_eq!(row.accessory_view, view, "{note}");
                let symbol = (image == 2 && shown).then(|| "chevron.right".to_owned());
                assert_eq!(row.symbol, symbol, "{note}");
            }
        }
    }
}

/// An accessory with `visibility: hidden` or `collapse`, written on it, on
/// its row or on the symbol of a detail button, keeps each part of its row
/// as written, in a Markdown row and in a plain one; shown again, the cell
/// draws it again.
#[test]
fn a_hidden_accessory_keeps_each_part_of_its_row() {
    for (kind, accessory_type, expected) in [
        (NodeType::Control, "checkbox", Accessory::Toggle(5)),
        (NodeType::Control, "radio", Accessory::Radio(5, false)),
        (NodeType::Image, "chevron.forward", Accessory::Disclosure),
        (NodeType::Image, "checkmark", Accessory::Checkmark),
        (NodeType::Pressable, "detail", Accessory::Detail(5)),
    ] {
        for markdown in [false, true] {
            // The radio leads its row or ends it, as every other accessory does.
            for parts in [vec![3, 5], vec![5, 3]] {
                if parts[0] == 5 && accessory_type != "radio" {
                    continue;
                }
                // The node that says it: the accessory, its row, or a detail button's symbol.
                for says in [5, 1, 6] {
                    if says == 6 && accessory_type != "detail" {
                        continue;
                    }
                    let mut k = Kernel::with_monospace();
                    let mut ops = vec![];
                    for (id, node_type) in [
                        (1, NodeType::View),
                        (3, NodeType::Text),
                        (5, kind),
                        (6, NodeType::Image),
                        (10, NodeType::List),
                        (11, NodeType::View),
                        (12, NodeType::View),
                    ] {
                        ops.push(Op::CreateView { id, node_type });
                    }
                    let mut props = vec![
                        (3, PropId::Text, "**Notifications**".to_owned()),
                        (5, PropId::Type, accessory_type.to_owned()),
                        (
                            5,
                            PropId::ImageSource,
                            format!("symbol:sf/{accessory_type}"),
                        ),
                        (6, PropId::ImageSource, "symbol:sf/info.circle".to_owned()),
                        (10, PropId::ListStyle, "inset-grouped".to_owned()),
                    ];
                    if markdown {
                        props.push((3, PropId::Markup, "markdown".to_owned()));
                    }
                    for (id, prop, value) in props {
                        ops.push(Op::SetProp {
                            id,
                            prop,
                            value: value.as_str().into(),
                        });
                    }
                    let mut children = vec![(10, vec![11]), (11, vec![12]), (12, vec![1])];
                    children.push((1, parts.clone()));
                    if accessory_type == "detail" {
                        children.push((5, vec![6]));
                    }
                    for (id, children) in children {
                        ops.push(Op::SetChildren { id, children });
                    }
                    ops.push(Op::AttachRoot { id: 10 });
                    k.apply(0, 1, &ops).unwrap();
                    for (batch, visibility) in
                        (2..).zip(["hidden", "visible", "collapse", "visible"])
                    {
                        k.apply(
                            0,
                            batch,
                            &[style(says, &[(StyleId::Visibility, visibility)], true)],
                        )
                        .unwrap();
                        let row = k.grouped_row(&k.node(1).unwrap(), &|_| false);
                        let note = format!(
                            "{accessory_type}, Markdown={markdown}, parts={parts:?}, {visibility} on {says}"
                        );
                        assert_eq!(
                            k.grouped_list(10).unwrap().sections[0].rows[0],
                            row,
                            "{note}"
                        );
                        if visibility == "visible" {
                            assert_eq!(row.custom, markdown, "{note}");
                            assert_eq!(row.accessory, expected, "{note}");
                            assert_eq!(row.accessory_view, Some(5), "{note}");
                            assert_eq!(
                                k.grouped_toggle(1, &|_| false),
                                (accessory_type == "checkbox").then_some(5),
                                "{note}"
                            );
                            continue;
                        }
                        // The cell replaces no part: only a radio's target stays, as in
                        // any row that keeps its views.
                        assert!(row.custom, "{note}");
                        let kept = match expected {
                            radio @ Accessory::Radio(..) => radio,
                            _ => Accessory::None,
                        };
                        assert_eq!(row.accessory, kept, "{note}");
                        assert_eq!(row.accessory_view, None, "{note}");
                        assert_eq!(row.title, None, "{note}");
                        assert_eq!(k.grouped_toggle(1, &|_| false), None, "{note}");
                    }
                }
            }
        }
    }
}

#[test]
fn a_header_of_more_than_one_text_is_custom_and_named_by_its_texts() {
    let mut k = Kernel::with_monospace();
    let mut ops = vec![];
    for (id, node_type) in [
        (1, NodeType::View),
        (2, NodeType::View),
        (3, NodeType::View),
        (4, NodeType::View),
        (5, NodeType::Text),
        (6, NodeType::View),
        (7, NodeType::Image),
        (8, NodeType::Text),
        (9, NodeType::Text),
        (10, NodeType::Text),
        (11, NodeType::Pressable),
    ] {
        ops.push(Op::CreateView { id, node_type });
    }
    let prop = |id, prop, value: &str| Op::SetProp {
        id,
        prop,
        value: value.into(),
    };
    let children = |id, children: &[ViewId]| Op::SetChildren {
        id,
        children: children.to_vec(),
    };
    ops.extend([
        prop(1, PropId::ListStyle, "plain"),
        prop(3, PropId::SemanticTag, "header"),
        prop(5, PropId::Text, "Move"),
        prop(8, PropId::Text, "Ryu"),
        prop(9, PropId::Text, "12 moves"),
        prop(10, PropId::Text, "Avatar"),
        Op::SetProp {
            id: 6,
            prop: PropId::AccessibilityElementsHidden,
            value: true.into(),
        },
        // A header of an avatar (an image and a text no one hears) and two
        // texts, then the group of one row.
        children(1, &[2]),
        children(2, &[3, 4]),
        children(3, &[6, 8, 9]),
        children(6, &[7, 10]),
        children(4, &[5]),
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    let section = k.grouped_list(1).unwrap().sections.remove(0);
    assert_eq!(section.header_view, Some(3));
    assert_eq!(section.header.as_deref(), Some("Ryu 12 moves"));
    assert_eq!(section.header_look, TextLook::default());
    assert!(!section.header_parts, "texts and an image: one heading");

    // A button in it is its own element, as a text with a press is.
    k.apply(1, 2, &[children(3, &[6, 8, 9, 11])]).unwrap();
    assert!(k.grouped_list(1).unwrap().sections[0].header_parts);
    k.apply(2, 3, &[children(3, &[6, 8, 9])]).unwrap();
    let pressed = k.grouped_list_with(1, &|id| id == 9).unwrap();
    assert!(pressed.sections[0].header_parts);

    // An image alone is a custom header with no name.
    k.apply(3, 4, &[children(3, &[6]), children(6, &[7])])
        .unwrap();
    let section = k.grouped_list(1).unwrap().sections.remove(0);
    assert_eq!((section.header_view, section.header), (Some(3), None));

    // One text, the others hidden, is the platform's header.
    k.apply(
        4,
        5,
        &[
            children(3, &[6, 8]),
            style(6, &[(StyleId::Display, "none")], true),
            style(8, &[(StyleId::TextColor, "#ff0000")], true),
        ],
    )
    .unwrap();
    let section = k.grouped_list(1).unwrap().sections.remove(0);
    assert_eq!(section.header_view, None);
    assert_eq!(section.header.as_deref(), Some("Ryu"));
    assert!(section.header_look.color.is_some(), "its look, as before");
}

#[test]
fn a_bar_on_a_lists_scrollport_is_no_section_where_the_kernel_lays_such_bars_out() {
    // 12 names the list's top edge; 11 is a section
    for on in [false, true] {
        let mut k = Kernel::with_monospace();
        k.set_scrollport_bars(on);
        let mut ops = vec![];
        for (id, node_type) in [
            (10, NodeType::List),
            (11, NodeType::View),
            (12, NodeType::View),
        ] {
            ops.push(Op::CreateView { id, node_type });
        }
        ops.push(Op::SetProp {
            id: 10,
            prop: PropId::ListStyle,
            value: "inset-grouped".into(),
        });
        let bar = [(StyleId::SafeAreaBar, "top"), (StyleId::Height, "50")];
        ops.push(style(12, &bar, true));
        ops.push(Op::SetChildren {
            id: 10,
            children: vec![12, 11],
        });
        ops.push(Op::AttachRoot { id: 10 });
        k.apply(0, 1, &ops).unwrap();
        let list = k.grouped_list(10).unwrap();
        let sections: Vec<ViewId> = list.sections.iter().map(|s| s.view).collect();
        let expected = if on { vec![11] } else { vec![12, 11] };
        assert_eq!(sections, expected, "scrollport bars {on}");
    }
}
