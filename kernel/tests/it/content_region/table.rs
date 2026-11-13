use super::*;

/// A box `w`×`h` under a fresh id.
fn sized(ops: &mut Vec<Op>, next: &mut ViewId, w: f32, h: f32) -> ViewId {
    let id = *next;
    *next += 1;
    let mut s = StyleProps::default();
    s.mask = mask(&[StyleId::Width, StyleId::Height]);
    s.width = Dimension::Points(w);
    s.height = Dimension::Points(h);
    ops.push(Op::CreateView {
        id,
        node_type: NodeType::View,
    });
    ops.push(Op::SetStyle {
        id,
        patch: Box::new(s),
    });
    id
}

/// An element `tag` under a fresh id holding `children`, with `props`.
fn element(
    ops: &mut Vec<Op>,
    next: &mut ViewId,
    tag: &str,
    props: &[(PropId, i64)],
    children: Vec<ViewId>,
) -> ViewId {
    let id = *next;
    *next += 1;
    ops.push(Op::CreateView {
        id,
        node_type: NodeType::View,
    });
    ops.push(Op::SetProp {
        id,
        prop: PropId::SemanticTag,
        value: tag.into(),
    });
    for (prop, v) in props {
        ops.push(Op::SetProp {
            id,
            prop: *prop,
            value: PropValue::Int(*v),
        });
    }
    ops.push(Op::SetChildren { id, children });
    id
}

/// A region lays a table out as ordinary layout does: its cells share the
/// table's grid, a `colspan` cell covering two columns, and each is placed
/// from the table's origin, not its row's.
#[test]
fn a_region_lays_a_table_out_as_ordinary_layout() {
    use exact_kernel::region::RegionProfile;
    for profile in [RegionProfile::PinnedOffers, RegionProfile::SplitFacts] {
        let mut k = fixture();
        let mut ops = Vec::new();
        let mut next = 10;
        let (a, b) = (
            sized(&mut ops, &mut next, 50., 10.),
            sized(&mut ops, &mut next, 30., 10.),
        );
        let wide = element(&mut ops, &mut next, "td", &[(PropId::ColSpan, 2)], vec![a]);
        let narrow = element(&mut ops, &mut next, "td", &[], vec![b]);
        let first = element(&mut ops, &mut next, "tr", &[], vec![wide, narrow]);
        let mut cells = Vec::new();
        for _ in 0..3 {
            let inner = sized(&mut ops, &mut next, 20., 10.);
            cells.push(element(&mut ops, &mut next, "td", &[], vec![inner]));
        }
        let second = element(&mut ops, &mut next, "tr", &[], cells);
        let table = element(&mut ops, &mut next, "table", &[], vec![first, second]);
        let after = sized(&mut ops, &mut next, 20., 10.);
        ops.push(Op::SetChildren {
            id: 3,
            children: vec![table, after],
        });
        k.apply(0, 0, &ops).unwrap();
        k.set_content_region_profile(Some(binding(&k)), profile)
            .unwrap();
        assert!(ready(&mut k, 400., 1).current);
        let mut oracle = k.rehydrate(Box::new(MonospaceMeasurer::default()));
        oracle
            .compute_layout(1, Offer::definite(400., 300.))
            .unwrap();
        assert_eq!(
            oracle.node(wide).unwrap().frame.width,
            52.,
            "the oracle spans two columns"
        );
        for id in 10..next {
            assert!(
                k.node(id)
                    .unwrap()
                    .frame
                    .bits_eq(oracle.node(id).unwrap().frame),
                "{profile:?}: {id} at {:?}, not {:?}",
                k.node(id).unwrap().frame,
                oracle.node(id).unwrap().frame
            );
        }
    }
}
