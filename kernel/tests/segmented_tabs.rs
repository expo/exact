//! The painted looks choose tab segments without changing tab semantics.
use exact_kernel::style::metrics::{self, Column};
use exact_kernel::{Kernel, NodeType, Op, PropId, PropValue, SegmentPlace};

fn prop(id: u32, prop: PropId, value: &str) -> Op {
    Op::SetProp {
        id,
        prop,
        value: PropValue::Str(value.into()),
    }
}

#[test]
fn content_tabs_are_segments_on_the_painted_looks() {
    for column in [Column::Material, Column::Neutral] {
        metrics::set_column(column);
        let mut k = Kernel::with_monospace();
        let mut ops = vec![
            Op::CreateView {
                id: 1,
                node_type: NodeType::View,
            },
            prop(1, PropId::AccessibilityRole, "tablist"),
        ];
        for (id, label) in [(2, "Details"), (4, "Logs")] {
            ops.extend([
                Op::CreateView {
                    id,
                    node_type: NodeType::Control,
                },
                prop(id, PropId::Type, "button"),
                prop(id, PropId::AccessibilityRole, "tab"),
                Op::CreateView {
                    id: id + 1,
                    node_type: NodeType::Text,
                },
                prop(id + 1, PropId::Text, label),
                Op::SetChildren {
                    id,
                    children: vec![id + 1],
                },
            ]);
        }
        ops.extend([
            Op::SetChildren {
                id: 1,
                children: vec![2, 4],
            },
            Op::CreateView {
                id: 10,
                node_type: NodeType::View,
            },
            Op::SetChildren {
                id: 10,
                children: vec![1],
            },
            Op::AttachRoot { id: 10 },
        ]);
        k.apply(0, 1, &ops).unwrap();
        assert_eq!(
            k.button_face_style(2).unwrap().segment,
            Some(SegmentPlace::First),
            "{column:?}"
        );
        assert_eq!(
            k.button_face_style(4).unwrap().segment,
            Some(SegmentPlace::Last)
        );
        // Material can segment tabs that name local panels. The neutral sheet
        // reserves aria-controls for its tab-bar treatment.
        k.apply(1, 2, &[prop(2, PropId::AccessibilityControls, "details")])
            .unwrap();
        assert_eq!(
            k.button_face_style(2).unwrap().segment.is_some(),
            column == Column::Material
        );
        k.apply(
            2,
            3,
            &[prop(1, PropId::AccessibilityOrientation, "vertical")],
        )
        .unwrap();
        assert_eq!(k.button_face_style(2).unwrap().segment, None);
        // A navigation root's controlled tabs stay its navigation bar, even
        // when their faces are text alone.
        k.apply(
            3,
            4,
            &[
                prop(1, PropId::AccessibilityOrientation, "horizontal"),
                Op::CreateView {
                    id: 10,
                    node_type: NodeType::View,
                },
                prop(10, PropId::NavigationKey, "route"),
                Op::CreateView {
                    id: 11,
                    node_type: NodeType::View,
                },
                prop(11, PropId::AccessibilityRole, "tabpanel"),
                prop(11, PropId::Id, "details"),
                Op::SetChildren {
                    id: 10,
                    children: vec![1, 11],
                },
                Op::AttachRoot { id: 10 },
            ],
        )
        .unwrap();
        assert_eq!(k.root_tabs(10).unwrap().tablist, 1);
        assert_eq!(k.button_face_style(2).unwrap().segment, None);
        // The same controls inside a route are local panel switches.
        k.apply(
            4,
            5,
            &[
                Op::CreateView {
                    id: 12,
                    node_type: NodeType::View,
                },
                prop(12, PropId::NavigationKey, "route"),
                Op::SetChildren {
                    id: 12,
                    children: vec![1, 11],
                },
                Op::SetChildren {
                    id: 10,
                    children: vec![12],
                },
            ],
        )
        .unwrap();
        assert_eq!(k.root_tabs(10), None);
        assert_eq!(
            k.button_face_style(2).unwrap().segment.is_some(),
            column == Column::Material
        );
        let mut own = exact_kernel::StyleProps::default();
        own.set_dynamic(
            exact_kernel::StyleId::Appearance,
            &exact_kernel::style::StyleValue::Text("none".into()),
        )
        .unwrap();
        own.written = own.mask;
        k.apply(
            5,
            6,
            &[Op::SetStyle {
                id: 1,
                patch: Box::new(own),
            }],
        )
        .unwrap();
        assert_eq!(k.button_face_style(2).unwrap().segment, None);
    }
    gaining_a_navigation_key_restyles_tabs_behind_the_new_boundary();
}

fn gaining_a_navigation_key_restyles_tabs_behind_the_new_boundary() {
    use exact_kernel::Offer;
    metrics::set_column(Column::Material);
    let tree = |keyed: bool| {
        let mut k = Kernel::with_monospace();
        let mut ops = vec![
            Op::CreateView {
                id: 1,
                node_type: NodeType::View,
            },
            prop(1, PropId::NavigationKey, "root"),
            Op::CreateView {
                id: 2,
                node_type: NodeType::View,
            },
        ];
        if keyed {
            ops.push(prop(2, PropId::NavigationKey, "route"));
        }
        for base in [10, 20] {
            ops.extend([
                Op::CreateView {
                    id: base,
                    node_type: NodeType::View,
                },
                prop(base, PropId::AccessibilityRole, "tablist"),
            ]);
            for (offset, label) in [(1, "Details"), (3, "Logs")] {
                let id = base + offset;
                ops.extend([
                    Op::CreateView {
                        id,
                        node_type: NodeType::Control,
                    },
                    prop(id, PropId::Type, "button"),
                    prop(id, PropId::AccessibilityRole, "tab"),
                    prop(id, PropId::AccessibilityControls, label),
                    Op::CreateView {
                        id: id + 1,
                        node_type: NodeType::Text,
                    },
                    prop(id + 1, PropId::Text, label),
                    Op::SetChildren {
                        id,
                        children: vec![id + 1],
                    },
                ]);
            }
            ops.push(Op::SetChildren {
                id: base,
                children: vec![base + 1, base + 3],
            });
        }
        for (id, name) in [(30, "Details"), (31, "Logs")] {
            ops.extend([
                Op::CreateView {
                    id,
                    node_type: NodeType::View,
                },
                prop(id, PropId::AccessibilityRole, "tabpanel"),
                prop(id, PropId::Id, name),
            ]);
        }
        ops.extend([
            Op::SetChildren {
                id: 2,
                children: vec![10, 20, 30, 31],
            },
            Op::SetChildren {
                id: 1,
                children: vec![2],
            },
            Op::AttachRoot { id: 1 },
        ]);
        k.apply(0, 1, &ops).unwrap();
        // Host-reported 40 dp button boxes with unequal natural widths.
        for (id, width) in [(11, 100.), (13, 60.), (21, 100.), (23, 60.)] {
            k.set_intrinsic_size(id, Some((width, 40.))).unwrap();
        }
        k.compute_layout(1, Offer::definite(400., 600.)).unwrap();
        k
    };
    let fresh = tree(true);
    // The key written on the tabs' parent, or the tabs moved into a new
    // keyed container attached under it: either way they leave the root.
    let key = vec![prop(2, PropId::NavigationKey, "route")];
    let moved = vec![
        Op::CreateView {
            id: 40,
            node_type: NodeType::View,
        },
        prop(40, PropId::NavigationKey, "route"),
        Op::SetChildren {
            id: 40,
            children: vec![10, 20, 30, 31],
        },
        Op::SetChildren {
            id: 2,
            children: vec![40],
        },
    ];
    for ops in [key, moved] {
        let mut mounted = tree(false);
        assert_eq!(mounted.root_tabs(1).unwrap().tablist, 10);
        mounted.apply(1, 2, &ops).unwrap();
        mounted
            .compute_layout(1, Offer::definite(400., 600.))
            .unwrap();
        assert_eq!(mounted.root_tabs(1), None);
        same_segments(&mounted, &fresh);
    }
}

/// `mounted`'s two tablists are laid out as `fresh`'s, as segments.
fn same_segments(mounted: &Kernel, fresh: &Kernel) {
    for base in [10, 20] {
        for id in base..=base + 4 {
            assert_eq!(
                mounted.node(id).unwrap().frame,
                fresh.node(id).unwrap().frame,
                "node {id}"
            );
        }
        let first = mounted.node(base + 1).unwrap().frame;
        let last = mounted.node(base + 3).unwrap().frame;
        assert_eq!((first.height, last.height), (40., 40.));
        assert!(
            (first.width - last.width).abs() <= 1.,
            "equal columns: {first:?} {last:?}"
        );
        assert_eq!(last.x, first.x + first.width - 1., "shared outline");
    }
}
