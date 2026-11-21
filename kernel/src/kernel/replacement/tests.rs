use super::*;
use crate::{Dimension, NodeType, PropId, PropValue, StyleId, LOOK_HOVER};

fn view(id: ViewId, node_type: NodeType) -> Op {
    Op::CreateView { id, node_type }
}

fn children(id: ViewId, children: &[ViewId]) -> Op {
    Op::SetChildren {
        id,
        children: children.to_vec(),
    }
}

fn tree(ops: &[Op]) -> Kernel {
    let mut k = Kernel::with_monospace();
    k.apply(0, 1, ops).unwrap();
    k
}

fn equal(left: &Kernel, right: &Kernel) {
    assert_eq!(left.live_count(), right.live_count());
    assert_eq!(left.roots(), right.roots());
    for slot in left.arena.iter_live() {
        let left = left.node_at(slot);
        let right = right.node(left.id).unwrap();
        assert_eq!(left.node_type, right.node_type);
        assert_eq!(left.parent, right.parent);
        assert_eq!(left.children(), right.children());
        assert_eq!(left.props, right.props);
        assert_eq!(left.style, right.style);
        assert_eq!(left.looks(), right.looks());
    }
}

#[test]
fn a_replacement_changes_only_the_fields_that_differ() {
    let editor = view(2, NodeType::TextInput);
    let text = Op::SetProp {
        id: 2,
        prop: PropId::Value,
        value: PropValue::Str("Piranesi".into()),
    };
    let shared = [
        view(1, NodeType::View),
        editor,
        text,
        children(1, &[2]),
        Op::AttachRoot { id: 1 },
    ];
    let mut live = tree(&shared);
    let mut target = tree(&shared);
    let key = live.node(2).unwrap().key;
    let style = |width: f32| {
        let mut style = StyleProps::default();
        style.mask.set(StyleId::Width);
        style.width = Dimension::Points(width);
        style.written.set(StyleId::Width);
        style
    };
    let mut old = style(10.);
    old.mask.set(StyleId::Height);
    old.height = Dimension::Points(8.);
    old.written.set(StyleId::Opacity);
    live.apply(
        0,
        2,
        &[
            Op::SetStyle {
                id: 2,
                patch: Box::new(old),
            },
            Op::SetProp {
                id: 2,
                prop: PropId::TestId,
                value: PropValue::Str("old".into()),
            },
            Op::SetLooks {
                id: 2,
                looks: vec![(LOOK_HOVER, StyleProps::default())],
            },
        ],
    )
    .unwrap();
    let mut new = style(10.);
    new.written.set(StyleId::Height);
    target
        .apply(
            0,
            2,
            &[
                view(3, NodeType::Text),
                children(1, &[3, 2]),
                Op::SetStyle {
                    id: 2,
                    patch: Box::new(new),
                },
            ],
        )
        .unwrap();

    let ops = target.replacement_ops(&live);
    assert!(!ops.iter().any(|op| matches!(
        op,
        Op::SetProp {
            prop: PropId::Value,
            ..
        }
    )));
    assert!(!ops
        .iter()
        .any(|op| matches!(op, Op::SetStyle { patch, .. } if patch.mask.has(StyleId::Width))));
    live.apply(0, 3, &ops).unwrap();
    assert_eq!(live.node(2).unwrap().key, key);
    equal(&live, &target);
    assert!(target.replacement_ops(&live).is_empty());
}

#[test]
fn independent_trees_with_overlapping_ids_keep_only_matching_subtrees() {
    let mut live = tree(&[
        view(1, NodeType::View),
        view(2, NodeType::View),
        view(3, NodeType::View),
        view(4, NodeType::View),
        view(5, NodeType::View),
        view(6, NodeType::View),
        children(1, &[3]),
        children(3, &[4]),
        children(5, &[6]),
        Op::AttachRoot { id: 1 },
        Op::AttachRoot { id: 2 },
    ]);
    let target = tree(&[
        view(6, NodeType::Text),
        view(5, NodeType::View),
        view(4, NodeType::View),
        view(3, NodeType::View),
        view(2, NodeType::View),
        view(1, NodeType::View),
        children(1, &[3]),
        children(5, &[4, 6]),
        Op::AttachRoot { id: 2 },
        Op::AttachRoot { id: 1 },
    ]);
    let detached = live.node(5).unwrap().key;
    let other_root = live.node(2).unwrap().key;
    let child_of_replaced = live.node(3).unwrap().key;
    live.apply(0, 2, &target.replacement_ops(&live)).unwrap();
    equal(&live, &target);
    assert_eq!(live.node(5).unwrap().key, detached);
    assert_eq!(live.node(2).unwrap().key, other_root);
    assert_ne!(live.node(3).unwrap().key, child_of_replaced);
}
