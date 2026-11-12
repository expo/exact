//! Neutral route titles use the web's large-title line (LLP 1115.000 D17).
//! Metric columns are process-wide, so the cases share one test.
use exact_kernel::style::metrics::{self, Column};
use exact_kernel::{
    Kernel, MonospaceMeasurer, NodeType, Offer, Op, PropId, PropValue, StyleId, StyleMask,
    StyleProps, StyleValue,
};

fn prop(id: u32, prop: PropId, value: impl Into<PropValue>) -> Op {
    Op::SetProp {
        id,
        prop,
        value: value.into(),
    }
}

fn style(id: u32, rows: &[(StyleId, &str)]) -> Op {
    let mut patch = StyleProps::default();
    for &(row, value) in rows {
        let value = value
            .parse::<f64>()
            .map_or_else(|_| StyleValue::Text(value.into()), StyleValue::Number);
        patch.set_dynamic(row, &value).unwrap();
    }
    patch.written = patch.mask;
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

fn route(text: &str, button: bool) -> Kernel {
    let mut k = Kernel::with_monospace();
    let mut ops = vec![
        prop(1, PropId::NavigationKey, "route"),
        prop(2, PropId::SemanticTag, "header"),
        prop(3, PropId::AccessibilityRole, "heading"),
        prop(3, PropId::AccessibilityHeadingLevel, PropValue::Int(1)),
        prop(3, PropId::Text, text),
        style(3, &[(StyleId::FontSize, "1.6875rem")]),
        Op::SetChildren {
            id: 1,
            children: vec![2],
        },
        Op::SetChildren {
            id: 2,
            children: if button { vec![3, 4] } else { vec![3] },
        },
        Op::AttachRoot { id: 1 },
    ];
    if button {
        ops.extend([
            style(
                2,
                &[(StyleId::Display, "flex"), (StyleId::AlignItems, "center")],
            ),
            style(4, &[(StyleId::Width, "44px"), (StyleId::Height, "24px")]),
        ]);
    }
    for id in (1..=5).rev() {
        ops.insert(
            0,
            Op::CreateView {
                id,
                node_type: if id == 3 {
                    NodeType::Text
                } else {
                    NodeType::View
                },
            },
        );
    }
    k.apply(0, 1, &ops).unwrap();
    k
}

fn frame(k: &mut Kernel, id: u32) -> exact_kernel::Frame {
    k.compute_layout(1, Offer::definite(390., 800.)).unwrap();
    k.node(id).unwrap().frame
}

fn near(got: f32, want: f32) {
    assert!((got - want).abs() < 0.001, "{got} != {want}");
}

fn change(k: &mut Kernel, ops: &[Op]) {
    k.apply(0, k.epoch() + 1, ops).unwrap();
}

#[test]
fn a_route_title_keeps_the_large_title_line() {
    metrics::set_column(Column::Neutral);
    for text in ["Inbox", "", " "] {
        for button in [false, true] {
            let mut k = route(text, button);
            for root in [13., 16., 20., 32., 16.] {
                k.set_root_font_size(root).unwrap();
                change(&mut k, &[]);
                let line = root * 1.6875 * 1.2;
                let large = root * 2.0625 * 1.2;
                near(frame(&mut k, 2).height, large + 13.);
                let title = frame(&mut k, 3);
                near(title.height, line);
                near(title.y, 6. + (large - line) / 2.);
                if button {
                    let item = frame(&mut k, 4);
                    near(title.y + title.height / 2., item.y + item.height / 2.);
                }
                let mut fresh = k.rehydrate(Box::new(MonospaceMeasurer::default()));
                assert_eq!(frame(&mut fresh, 3), title);
            }
        }
    }

    let mut k = route("Inbox", false);
    near(frame(&mut k, 2).height, 52.6);
    // Normal leading is measured, not guessed from the font size.
    let mut tall = k.rehydrate(Box::new(MonospaceMeasurer {
        line_height_em: 1.4,
        ..Default::default()
    }));
    near(frame(&mut tall, 3).height, 37.8);
    near(frame(&mut tall, 3).y, 6.9);
    near(frame(&mut tall, 2).height, 52.6);

    for (rows, height, y) in [
        (vec![(StyleId::FontSize, "17px")], 20.4, 15.6),
        (vec![(StyleId::LineHeight, "1")], 17., 17.3),
        (vec![(StyleId::LineHeight, "50px")], 50., 6.),
        (
            vec![
                (StyleId::FontSize, "27px"),
                (StyleId::LineHeight, "1.2"),
                (StyleId::Height, "44px"),
            ],
            44.,
            9.6,
        ),
        (vec![(StyleId::Height, "20px")], 32.4, 9.6),
        (
            vec![
                (StyleId::Height, "44px"),
                (StyleId::MarginTop, "0"),
                (StyleId::MarginBottom, "0"),
            ],
            44.,
            6.,
        ),
        (
            vec![(StyleId::Height, "20px"), (StyleId::MinHeight, "0")],
            20.,
            6.,
        ),
    ] {
        change(&mut k, &[style(3, &rows)]);
        let title = frame(&mut k, 3);
        near(title.height, height);
        near(title.y, y);
    }

    let mut k = route("Inbox", false);
    change(&mut k, &[style(3, &[(StyleId::Height, "60px")])]);
    let title = frame(&mut k, 3);
    near(title.height, 60.);
    let key = k.node(3).unwrap().key;
    near(k.measure_auto_height(key).unwrap().0.height, 32.4);
    assert_eq!(frame(&mut k, 3), title);

    let mut k = route("Inbox", false);
    // Each eligibility change restores the ordinary style on the retained node.
    let no_line = [
        prop(3, PropId::AccessibilityHeadingLevel, PropValue::Int(2)),
        prop(3, PropId::AccessibilityRole, "status"),
        prop(2, PropId::Popover, "auto"),
        prop(2, PropId::SemanticTag, "dialog"),
        prop(1, PropId::NavigationPresentation, "modal"),
        style(2, &[(StyleId::Appearance, "none")]),
        Op::SetChildren {
            id: 1,
            children: vec![5, 2],
        },
        Op::SetChildren {
            id: 1,
            children: vec![2, 3],
        },
        Op::ClearProp {
            id: 1,
            prop: PropId::NavigationKey,
        },
    ];
    let restore = [
        prop(3, PropId::AccessibilityHeadingLevel, PropValue::Int(1)),
        prop(3, PropId::AccessibilityRole, "heading"),
        Op::ClearProp {
            id: 2,
            prop: PropId::Popover,
        },
        prop(2, PropId::SemanticTag, "header"),
        Op::ClearProp {
            id: 1,
            prop: PropId::NavigationPresentation,
        },
        Op::ClearStyle {
            id: 2,
            mask: StyleMask::of(StyleId::Appearance),
            written: false,
        },
        Op::SetChildren {
            id: 1,
            children: vec![2],
        },
        Op::SetChildren {
            id: 2,
            children: vec![3],
        },
        prop(1, PropId::NavigationKey, "route"),
    ];
    for op in no_line {
        near(frame(&mut k, 3).y, 9.6);
        change(&mut k, &[op]);
        let title = frame(&mut k, 3);
        let mut own = k.rehydrate(Box::new(MonospaceMeasurer::default()));
        change(
            &mut own,
            &[style(
                3,
                &[
                    (StyleId::MarginTop, "0"),
                    (StyleId::MarginBottom, "0"),
                    (StyleId::MinHeight, "0"),
                ],
            )],
        );
        assert_eq!(title, frame(&mut own, 3));
        change(&mut k, &restore);
    }
    change(
        &mut k,
        &[
            prop(5, PropId::SemanticTag, "nav"),
            Op::SetChildren {
                id: 2,
                children: vec![3, 5],
            },
        ],
    );
    near(frame(&mut k, 3).y, 6.);
    change(
        &mut k,
        &[Op::ClearProp {
            id: 5,
            prop: PropId::SemanticTag,
        }],
    );
    near(frame(&mut k, 3).y, 9.6);
    for (container, value) in [(PropId::SemanticTag, "dialog"), (PropId::Popover, "manual")] {
        for button in [false, true] {
            let mut k = route("Inbox", button);
            change(
                &mut k,
                &[
                    Op::ClearProp {
                        id: 1,
                        prop: PropId::NavigationKey,
                    },
                    prop(5, PropId::NavigationKey, "nested"),
                    Op::SetChildren {
                        id: 5,
                        children: vec![2],
                    },
                    Op::SetChildren {
                        id: 1,
                        children: vec![5],
                    },
                ],
            );
            for root in [16., 20., 32., 16.] {
                k.set_root_font_size(root).unwrap();
                change(&mut k, &[]);
                near(frame(&mut k, 2).height, root * 2.0625 * 1.2 + 13.);
                change(&mut k, &[prop(1, container, value)]);
                let bar = frame(&mut k, 2);
                near(bar.height, root * 1.6875 * 1.2 + 13.);
                near(frame(&mut k, 3).y - bar.y, 6.);
                change(
                    &mut k,
                    &[Op::ClearProp {
                        id: 1,
                        prop: container,
                    }],
                );
                near(frame(&mut k, 2).height, root * 2.0625 * 1.2 + 13.);
            }
        }
    }
    for column in [Column::Ios, Column::Mac, Column::Material] {
        metrics::set_column(column);
        let mut k = route("Inbox", false);
        near(frame(&mut k, 3).height, 32.4);
        near(frame(&mut k, 3).y, 0.);
    }
}
