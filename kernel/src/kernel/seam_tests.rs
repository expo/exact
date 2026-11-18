//! Stage 2's parts against one another (LLP 1123 D4, D5, D9, D11): an
//! incremental commit against a fresh layout of the tree it leaves, and the
//! seams between readers, reservations, what boxes take and owners.
use super::ignore::laid;
use super::*;
use crate::Frame;

/// What a layout shows of `ids`: each frame, padding and observation.
fn picture(k: &Kernel, ids: &[ViewId]) -> Vec<(ViewId, Frame, [f32; 4], Option<Observed>)> {
    let padding = |id| {
        let (l, t, r, b) = k.resolved_padding(k.arena.key(slot(k, id))).unwrap();
        [t, r, b, l]
    };
    (ids.iter())
        .map(|&id| {
            (
                id,
                k.node(id).unwrap().frame,
                padding(id),
                k.observation(id),
            )
        })
        .collect()
}

#[test]
fn a_reader_moved_between_owners_reads_as_in_a_fresh_layout() {
    // 4 pads its top by its observation; it moves from the owner 2, under a
    // 47-point band, into the owner 3, under a 20-point one
    let tree = |under| {
        let mut ops = create(3..=4);
        let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
        ops.extend([style(3, vec![], &[]), takes_container(3)]);
        ops.push(style(4, pad, &[(StyleId::Height, "10")]));
        ops.extend([children(1, &[2, 3]), children(under, &[4])]);
        let mut k = owner(390.0, 844.0, ops);
        let block = block(BlockAxis::Is(390.0), BlockAxis::Is(844.0));
        k.set_host_block(3, Some(block)).unwrap();
        k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
        k.set_owner_band(3, Some([20.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        k
    };
    let mut k = tree(2);
    assert_eq!(top(&k, 4), 47.0);
    k.apply(0, 2, &[children(2, &[]), children(3, &[4])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 20.0);
    assert_eq!(picture(&k, &[3, 4]), picture(&tree(3), &[3, 4]));
}

#[test]
fn a_reservation_follows_the_property_it_reads() {
    // 3 declares `--gap`, 4 reserves that much at its bottom, and 5,
    // absolute at 4's bottom, observes it; `--gap` changes from 20 to 30
    let tree = |gap: &str| {
        let mut ops = create(3..=5);
        let declared = format!("--gap: {gap}");
        let reserving = [
            (StyleId::SafeAreaPaddingBottom, "var(--gap)"),
            (StyleId::Height, "100"),
            (StyleId::PositionType, "relative"),
        ];
        let at_bottom = [
            (StyleId::PositionType, "absolute"),
            (StyleId::Bottom, "0"),
            (StyleId::Height, "40"),
        ];
        let pad = vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))];
        ops.push(style(
            3,
            vec![],
            &[(StyleId::SafeAreaProperties, declared.as_str())],
        ));
        ops.push(style(4, vec![], &reserving));
        ops.push(style(5, pad, &at_bottom));
        ops.extend([
            children(1, &[2]),
            children(2, &[3]),
            children(3, &[4]),
            children(4, &[5]),
        ]);
        laid([47.0, 0.0, 34.0, 0.0], &[], ops)
    };
    let mut k = tree("20px");
    assert_eq!(left(&k, 5), 20.0);
    let gap = [(StyleId::SafeAreaProperties, "--gap: 30px")];
    k.apply(0, 2, &[style(3, vec![], &gap)]).unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 30.0);
    assert_eq!(picture(&k, &[3, 4, 5]), picture(&tree("30px"), &[3, 4, 5]));
}

#[test]
fn an_inset_box_that_ignores_every_edge_takes_three_bands() {
    // C30's usual form: 3, absolute with `inset: 0` in the root that clears
    // 47 at the top, 44 at the left and 34 at the bottom, ignores every
    // edge and covers the allocation; the top band grows to 59
    let tree = |top: f32| {
        let mut ops = create(3..=3);
        let cover = [
            (StyleId::SafeAreaIgnore, "container"),
            (StyleId::PositionType, "absolute"),
            (StyleId::Top, "0"),
            (StyleId::Right, "0"),
            (StyleId::Bottom, "0"),
            (StyleId::Left, "0"),
        ];
        ops.push(style(3, vec![], &cover));
        ops.extend([children(1, &[2]), children(2, &[3])]);
        let root = [(StyleId::PositionType, "relative")];
        laid([top, 0.0, 34.0, 44.0], &root, ops)
    };
    let mut k = tree(47.0);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 844.0));
    k.set_owner_band(2, Some([59.0, 0.0, 34.0, 44.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 844.0));
    assert_eq!(picture(&k, &[2, 3]), picture(&tree(59.0), &[2, 3]));
}

#[test]
fn a_box_outside_every_owner_takes_the_session_band() {
    // 2, outside every owner, lies 10 below the document's top under the
    // session's 47-point band and ignores the top: it extends to the top
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=2);
    ops.push(style(
        1,
        vec![],
        &[(StyleId::Height, "844"), (StyleId::PaddingTop, "10")],
    ));
    let taking = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::Height, "20"),
    ];
    ops.push(style(2, vec![], &taking));
    ops.extend([children(1, &[2]), Op::AttachRoot { id: 1 }]);
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band([47.0, 0.0, 0.0, 0.0]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (0.0, 0.0, 390.0, 30.0));
}

#[test]
fn a_measure_over_a_box_that_takes_the_band_leaves_the_kernel() {
    // 3 takes the 47-point top band in the owner 2; measuring 2 in another
    // block leaves what 3 takes, the kernel's export and the next layout
    let mut ops = create(3..=3);
    let taking = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(3, vec![], &taking));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = laid([47.0, 0.0, 0.0, 0.0], &[], ops);
    let (before, taken) = (
        k.export(None).unwrap(),
        k.arena.observations.taken(slot(&k, 3)),
    );
    assert!(taken.is_some());
    let shown = picture(&k, &[2, 3]);
    let narrow = block(BlockAxis::Is(300.0), BlockAxis::Is(600.0));
    k.measure_block(2, narrow).unwrap();
    assert_eq!(k.export(None).unwrap(), before);
    assert_eq!(k.arena.observations.taken(slot(&k, 3)), taken);
    lay_out(&mut k);
    assert_eq!(picture(&k, &[2, 3]), shown);
}
