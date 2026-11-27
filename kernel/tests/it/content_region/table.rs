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

#[test]
fn a_region_projects_displaced_rows_from_their_own_anonymous_grid() {
    let mut k = fixture();
    let mut ops = Vec::new();
    let mut next = 10;
    let mut children = Vec::new();
    for _ in 0..2 {
        children.push(sized(&mut ops, &mut next, 10., 7.3));
        let mut cells = Vec::new();
        for _ in 0..2 {
            let inner = sized(&mut ops, &mut next, 2., 10.2);
            cells.push(element(&mut ops, &mut next, "td", &[], vec![inner]));
        }
        let row = element(&mut ops, &mut next, "tr", &[], cells);
        let group = element(&mut ops, &mut next, "tbody", &[], vec![row]);
        for (id, top) in [(group, 0.2), (row, 0.1)] {
            let mut style = StyleProps::default();
            style.mask = mask(&[StyleId::PositionType, StyleId::Top]);
            style.position_type = PositionType::Relative;
            style.top = Dimension::Points(top);
            ops.push(Op::SetStyle {
                id,
                patch: Box::new(style),
            });
        }
        children.push(group);
    }
    let table = element(&mut ops, &mut next, "table", &[], children);
    let mut block = StyleProps::default();
    block.mask = mask(&[StyleId::Display]);
    block.display = Display::Block;
    ops.push(Op::SetStyle {
        id: table,
        patch: Box::new(block),
    });
    ops.push(Op::SetChildren {
        id: 3,
        children: vec![table],
    });
    let mut padding = StyleProps::default();
    padding.mask = mask(&[StyleId::PaddingTop]);
    padding.padding_top = Dimension::Points(113.6);
    ops.push(Op::SetStyle {
        id: 1,
        patch: Box::new(padding),
    });
    k.apply(0, 0, &ops).unwrap();

    let mut ordinary = k.rehydrate(Box::new(MonospaceMeasurer::default()));
    ordinary
        .compute_layout(1, Offer::definite(400., 300.))
        .unwrap();
    register(&mut k);
    let receipt = ready(&mut k, 400., 1);
    assert_eq!(receipt.origin.y, 113.6);
    super::projection::projected_matches_ordinary(&k, &ordinary, &receipt);

    let RegionSelection::Accepted(publication) = receipt.selection else {
        panic!("accepted");
    };
    let mut padding = StyleProps::default();
    padding.mask = mask(&[StyleId::PaddingTop]);
    padding.padding_top = Dimension::Points(57.3);
    ordinary
        .apply(
            0,
            1,
            &[Op::SetStyle {
                id: 1,
                patch: Box::new(padding),
            }],
        )
        .unwrap();
    ordinary
        .compute_layout(1, Offer::definite(400., 300.))
        .unwrap();
    let origin = ordinary.node(2).unwrap().frame;
    assert_eq!(origin.y, 57.3);
    for projected in publication.projected_frames(origin).unwrap() {
        let expected = ordinary.node_by_key(projected.node).unwrap().frame;
        assert!(projected.frame.bits_eq(expected));
        assert!(publication
            .frame(projected.node, origin)
            .unwrap()
            .bits_eq(expected));
    }
}

#[test]
fn a_hidden_cell_refuses_an_overflowing_row_even_when_placed_from_the_grid() {
    let mut k = fixture();
    let mut ops = Vec::new();
    let mut next = 10;
    let first_inner = sized(&mut ops, &mut next, 8., 2.0_f32.powi(106));
    let first_cell = element(&mut ops, &mut next, "td", &[], vec![first_inner]);
    let first = element(&mut ops, &mut next, "tr", &[], vec![first_cell]);
    let second_inner = sized(&mut ops, &mut next, 8., 10.);
    let second_cell = element(&mut ops, &mut next, "td", &[], vec![second_inner]);
    let hidden = element(&mut ops, &mut next, "td", &[], vec![]);
    let mut style = StyleProps::default();
    style.mask = mask(&[StyleId::Display]);
    style.display = Display::None;
    ops.push(Op::SetStyle {
        id: hidden,
        patch: Box::new(style),
    });
    let second = element(&mut ops, &mut next, "tr", &[], vec![second_cell, hidden]);
    let table = element(&mut ops, &mut next, "table", &[], vec![first, second]);
    ops.push(Op::SetChildren {
        id: 3,
        children: vec![table],
    });
    k.apply(0, 0, &ops).unwrap();
    register(&mut k);
    let receipt = ready(&mut k, 400., 1);
    let RegionSelection::Accepted(publication) = receipt.selection else {
        panic!("not accepted")
    };
    let origin = Frame {
        y: f32::MAX,
        ..receipt.origin
    };
    assert!(publication.frame(key(&k, hidden), receipt.origin).is_some());
    assert!(publication.projected_frames(origin).is_err());
    assert!(publication.frame(key(&k, second), origin).is_none());
    assert!(publication.frame(key(&k, hidden), origin).is_none());
    assert!(publication.frame(key(&k, first), origin).is_some());
}
