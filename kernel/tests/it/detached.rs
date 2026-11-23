//! A child detached in the batch that destroys its parent (LLP 1001 §3), the
//! sequence portal placement emits (LLP 1013.002 D2).

use exact_kernel::{Kernel, NodeType, Op};

/// root(1) → a(2), b(3) → b holds c(4).
fn build() -> Kernel {
    let mut k = Kernel::with_monospace();
    let create = |id| Op::CreateView {
        id,
        node_type: NodeType::View,
    };
    k.apply(
        0,
        1,
        &[
            create(1),
            create(2),
            create(3),
            create(4),
            Op::SetChildren {
                id: 1,
                children: vec![2, 3],
            },
            Op::SetChildren {
                id: 3,
                children: vec![4],
            },
            Op::AttachRoot { id: 1 },
        ],
    )
    .unwrap();
    k
}

/// A child taken out of a parent that the same batch destroys stays alive,
/// detached and outside every layout, keeps its key when attached elsewhere
/// later, and is destroyed with its subtree by its own op.
#[test]
fn a_detached_child_survives_its_parent_and_is_destroyed_once_by_its_own_op() {
    let mut k = build();
    let key_c = k.node(4).unwrap().key;
    let receipt = k
        .apply(
            0,
            2,
            &[
                Op::SetChildren {
                    id: 3,
                    children: vec![],
                },
                Op::DestroyView { id: 3 },
            ],
        )
        .unwrap();
    assert!(!receipt.destroyed.contains(&key_c));
    assert_eq!(k.node(4).unwrap().parent, None);
    assert_eq!(k.node(1).unwrap().children(), vec![2]);
    k.apply(
        0,
        3,
        &[Op::SetChildren {
            id: 2,
            children: vec![4],
        }],
    )
    .unwrap();
    assert_eq!(k.node(4).unwrap().key, key_c);
    assert_eq!(k.node(4).unwrap().parent, Some(2));
    k.apply(
        0,
        4,
        &[
            Op::SetChildren {
                id: 2,
                children: vec![],
            },
            Op::DestroyView { id: 4 },
        ],
    )
    .unwrap();
    assert!(k.node_by_key(key_c).is_none());
    assert_eq!(k.live_count(), 2);
}
