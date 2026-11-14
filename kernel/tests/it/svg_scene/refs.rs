use super::*;

// The uncached rule: compare every candidate's path with the referrer's.
fn reference(k: &Kernel, from: u32, id: &str) -> Option<u32> {
    let arena = k.arena();
    let candidates: Vec<_> = arena
        .iter_live()
        .filter(|s| arena.props(*s).str(PropId::Id) == Some(id))
        .collect();
    match candidates.as_slice() {
        [] => return None,
        [one] => return Some(arena.local_id(*one)),
        _ => {}
    }
    let chain = |slot| {
        let mut path = vec![slot];
        while let Some(parent) = arena.parent(*path.last().unwrap()) {
            path.push(parent);
        }
        path.reverse();
        path
    };
    let from = chain(arena.slot_of(from)?);
    candidates
        .into_iter()
        .map(|slot| {
            let path = chain(slot);
            let shared = path.iter().zip(&from).take_while(|(a, b)| a == b).count();
            let order: Vec<_> = path
                .windows(2)
                .map(|w| {
                    arena
                        .children(w[0])
                        .iter()
                        .position(|c| *c == w[1])
                        .unwrap()
                })
                .collect();
            (std::cmp::Reverse(shared), order, slot)
        })
        .min()
        .map(|(_, _, slot)| arena.local_id(slot))
}

fn agrees(k: &Kernel) {
    for id in ["fade", "other", "missing"] {
        for from in k
            .arena()
            .iter_live()
            .map(|s| k.arena().local_id(s))
            .chain([u32::MAX])
        {
            assert_eq!(
                k.resolve_id(from, id),
                reference(k, from, id),
                "{from}: #{id}"
            );
        }
    }
}

fn children(id: u32, children: &[u32]) -> Op {
    Op::SetChildren {
        id,
        children: children.to_vec(),
    }
}

#[test]
fn duplicate_ids_include_self_ancestors_descendants_and_detached_trees() {
    let mut d = Doc::new();
    let a = d.node(NodeType::View, &[], &[(PropId::Id, "fade")]);
    let b = d.node(NodeType::View, &[], &[(PropId::Id, "fade")]);
    let c = d.node(NodeType::View, &[], &[(PropId::Id, "fade")]);
    let x = d.node(NodeType::View, &[], &[]);
    let y = d.node(NodeType::View, &[], &[]);
    let detached = d.node(NodeType::View, &[], &[]);
    d.children(b, &[x]);
    d.children(a, &[b, y]);
    d.children(detached, &[c]);
    d.children(1, &[a]);
    let mut k = Kernel::with_monospace_on_demand();
    k.apply(0, 0, &d.ops).unwrap();
    agrees(&k);
    assert_eq!(
        k.resolve_id(a, "fade"),
        Some(a),
        "an ancestor precedes its descendants"
    );
    assert_eq!(
        k.resolve_id(x, "fade"),
        Some(b),
        "the deepest shared ancestor wins"
    );
    assert_eq!(k.resolve_id(y, "fade"), Some(a));
    assert_eq!(
        k.resolve_id(detached, "fade"),
        Some(c),
        "detached trees participate"
    );
    k.apply(0, 0, &[prop(1, PropId::Id, "fade")]).unwrap();
    agrees(&k);
    assert_eq!(k.resolve_id(1, "fade"), Some(1));
}

#[test]
fn duplicate_ids_follow_tree_and_id_mutations() {
    let mut d = Doc::new();
    let left = d.node(NodeType::View, &[], &[]);
    let right = d.node(NodeType::View, &[], &[]);
    let a = d.node(NodeType::View, &[], &[(PropId::Id, "fade")]);
    let b = d.node(NodeType::View, &[], &[(PropId::Id, "fade")]);
    let x = d.node(NodeType::View, &[], &[]);
    let y = d.node(NodeType::View, &[], &[]);
    let outside = d.node(NodeType::View, &[], &[]);
    d.children(left, &[a, x]);
    d.children(right, &[b, y]);
    d.children(1, &[right, left, outside]);
    d.ops.push(Op::AttachRoot { id: 1 });
    let mut k = Kernel::with_monospace_on_demand();
    k.apply(0, 0, &d.ops).unwrap();
    agrees(&k);
    assert_eq!(
        k.resolve_id(outside, "fade"),
        Some(b),
        "tree order, not allocation order"
    );
    assert_eq!(k.resolve_id(x, "fade"), Some(a));
    assert_eq!(k.resolve_id(y, "fade"), Some(b));
    let c = d.next;
    let changes = [
        vec![children(1, &[left, right, outside])],
        vec![
            Op::CreateView {
                id: c,
                node_type: NodeType::View,
            },
            prop(c, PropId::Id, "fade"),
        ],
        vec![children(left, &[c, a, x])],
        vec![prop(c, PropId::Id, "other")],
        vec![Op::ClearProp {
            id: a,
            prop: PropId::Id,
        }],
        vec![prop(c, PropId::Id, "fade")],
        vec![children(right, &[c, b, y])],
        vec![children(right, &[b, c, y])],
        vec![Op::DestroyView { id: b }],
        vec![Op::CreateView {
            id: b,
            node_type: NodeType::View,
        }],
        vec![prop(b, PropId::Id, "fade"), children(left, &[b, x, a])],
        vec![children(right, &[y])],
        vec![children(1, &[right, outside])],
        vec![children(right, &[left, y])],
        vec![Op::DestroyView { id: left }],
        vec![Op::ClearProp {
            id: c,
            prop: PropId::Id,
        }],
        vec![prop(y, PropId::Id, "fade")],
    ];
    for ops in changes {
        k.apply(0, 0, &ops).unwrap();
        agrees(&k);
    }
    k.reset();
    agrees(&k);
    k.apply(0, 0, &d.ops).unwrap();
    agrees(&k);
    let copy = k.rehydrate(Box::new(exact_kernel::MonospaceMeasurer::default()));
    agrees(&copy);
    assert!(k.apply(0, 0, &[children(left, &[1])]).is_err());
    agrees(&k);
    // Reads between phases must see changes before the epoch is published.
    k.begin_document_commit(0, 1, &[children(1, &[left, right, outside])], None)
        .unwrap();
    agrees(&k);
    k.continue_document_commit(&[children(left, &[a, b, x])])
        .unwrap();
    agrees(&k);
    k.finish_document_commit().unwrap();
    agrees(&k);
}

#[test]
fn repeated_component_reference_scaling() {
    for n in [128, 256, 512] {
        let mut d = Doc::new();
        let mut groups = Vec::new();
        let mut pairs = Vec::new();
        for _ in 0..n {
            let group = d.node(NodeType::View, &[], &[]);
            let definition = d.node(NodeType::View, &[], &[(PropId::Id, "fade")]);
            let reference = d.node(NodeType::View, &[], &[]);
            d.children(group, &[definition, reference]);
            groups.push(group);
            pairs.push((reference, definition));
        }
        d.children(1, &groups);
        let mut k = Kernel::on_demand(Box::new(exact_kernel::MonospaceMeasurer::default()));
        k.apply(0, 0, &d.ops).unwrap();
        let start = std::time::Instant::now();
        for (reference, definition) in pairs {
            assert_eq!(k.resolve_id(reference, "fade"), Some(definition));
        }
        eprintln!("{n} component references: {:?}", start.elapsed());
    }
}
