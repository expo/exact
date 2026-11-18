//! Reservations (LLP 1123 D11): what a box adds to the unsafe area inside
//! it, laid out as padding inside its border and counted in what its
//! descendants observe (D9); on an owner's root, cleared by its host around
//! its content root (D3).
use super::*;

const BAND: [f32; 4] = [47.0, 0.0, 34.0, 0.0];

/// The owner 2 under `BAND`, its root taking the container region, holding
/// the relatively positioned 3 with the rows `rows`, which holds 4, 10 tall
/// and padding its top by its own observation, then 5, absolute at its top
/// and padding its left by its top observation.
fn reserving(rows: &[(StyleId, &str)]) -> Kernel {
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
    let mut ops = create(3..=5);
    let mut held = vec![(StyleId::PositionType, "relative")];
    held.extend_from_slice(rows);
    ops.push(style(3, vec![], &held));
    ops.push(style(4, pad, &[(StyleId::Height, "10")]));
    let absolute = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Top, "0"),
        (StyleId::Height, "5"),
    ];
    let left = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    ops.push(style(5, left, &absolute));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
    lay_out(&mut k);
    k
}

#[test]
fn a_box_lays_its_children_out_clear_of_what_it_reserves() {
    // 3 lies under the band's 47 and adds 20 at the top: 4 sits below the
    // addition and observes the rest of the area, 47, which it pads by, so
    // its content clears both; 3 grows by the addition as by padding. The
    // absolute 5 resolves against 3's padding box, which the addition is in
    let rows = [
        (StyleId::AdditionalSafeAreaTop, "20"),
        (StyleId::SafeAreaPaddingLeft, "10"),
    ];
    let mut k = reserving(&rows);
    assert_eq!(frame(&k, 4), (10.0, 20.0, 380.0, 57.0));
    assert_eq!(
        (top(&k, 4), frame(&k, 3).3, frame(&k, 5).1),
        (47.0, 77.0, 0.0)
    );
    // A negative addition lays nothing out and takes from what 4 observes,
    // clamped at 0; changing it moves 4's observation, though no frame
    // moves with the change
    let mut txn = 1;
    let mut set = |k: &mut Kernel, rows: &[(StyleId, &str)]| {
        txn += 1;
        k.apply(0, txn, &[style(3, vec![], rows)]).unwrap();
        lay_out(k);
        (frame(k, 4).1, top(k, 4))
    };
    let rows = [
        (StyleId::AdditionalSafeAreaTop, "-30"),
        (StyleId::SafeAreaPaddingLeft, "0"),
    ];
    assert_eq!(set(&mut k, &rows), (0.0, 17.0));
    assert_eq!(
        set(&mut k, &[(StyleId::AdditionalSafeAreaTop, "-10")]),
        (0.0, 37.0)
    );
    assert_eq!(
        set(&mut k, &[(StyleId::AdditionalSafeAreaTop, "-50")]),
        (0.0, 0.0)
    );
    // The padding comes after the clamp, and is never negative
    let rows = [(StyleId::SafeAreaPaddingTop, "10")];
    assert_eq!(set(&mut k, &rows), (10.0, 0.0));
    let negative = StyleValue::Number(-10.0);
    let mut patch = StyleProps::default();
    assert!(patch
        .set_dynamic(StyleId::SafeAreaPaddingTop, &negative)
        .is_err());
    let rows = [
        (StyleId::AdditionalSafeAreaTop, "0"),
        (StyleId::SafeAreaPaddingTop, "calc(10px - 30px)"),
    ];
    assert_eq!(set(&mut k, &rows), (0.0, 47.0));
    // The padding lies inside the reservation: 4 sits below the addition
    // and a percentage padding, 39 of 390, or a calculated one, and
    // observes the rest of the area, 67 deep
    let rows = [
        (StyleId::SafeAreaPaddingTop, "0"),
        (StyleId::AdditionalSafeAreaTop, "20"),
        (StyleId::PaddingTop, "10%"),
        (StyleId::BoxSizing, "border-box"),
        (StyleId::Height, "100"),
    ];
    assert_eq!(set(&mut k, &rows), (59.0, 8.0));
    assert_eq!(frame(&k, 3).3, 100.0);
    let calculated = [(StyleId::PaddingTop, "calc(10% - 9px)")];
    assert_eq!(set(&mut k, &calculated), (50.0, 17.0));
}

#[test]
fn a_box_clear_of_the_band_starts_its_own_area() {
    // 3, 100 below the root's top and clear of the band, adds 20: the area
    // it passes runs from its own edge, so the absolute 5, 50 above that
    // edge, observes the 20 and none of the safe space between
    let rows = [
        (StyleId::MarginTop, "100"),
        (StyleId::AdditionalSafeAreaTop, "20"),
    ];
    let mut k = reserving(&rows);
    k.apply(0, 2, &[style(5, vec![], &[(StyleId::Top, "-50")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 5).1, left(&k, 5), top(&k, 4)), (50.0, 20.0, 0.0));
}

#[test]
fn a_padding_that_resolves_negative_pads_nothing() {
    // 3 pads by its own top observation less 100, negative under the
    // band's 47 and clamped at 0, as CSS clamps a calculation: 4 observes
    // the band
    let mut k = reserving(&[]);
    let less = Node::Sum(vec![observed(Edge::Top, None), px(-100.0)]);
    let rows = vec![(StyleId::SafeAreaPaddingTop, less)];
    k.apply(0, 2, &[style(3, rows, &[])]).unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 4).1, top(&k, 4)), (0.0, 47.0));
}

#[test]
fn a_box_observes_none_of_what_it_reserves() {
    // 3, under the band's 47 and adding 20 inside it, is as wide as it
    // observes at its top
    let mut k = reserving(&[(StyleId::AdditionalSafeAreaTop, "20")]);
    let wide = vec![(StyleId::Width, observed(Edge::Top, None))];
    k.apply(0, 2, &[style(3, wide, &[])]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3).2, 47.0);
}

#[test]
fn an_auto_edge_pads_by_the_hosts_default() {
    // None reported pads nothing; a default moves 4 and what the absolute 5
    // observes, though nothing moves 5
    let mut k = reserving(&[(StyleId::SafeAreaPaddingTop, "auto")]);
    assert_eq!((frame(&k, 4).1, left(&k, 5)), (0.0, 47.0));
    k.set_env(k.env().with_default_padding(Some([12.0; 4])))
        .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 4).1, left(&k, 5)), (12.0, 59.0));
}

#[test]
fn an_owners_root_has_its_host_clear_what_it_reserves() {
    // The root reserves 20 below and pads by default above: its host clears
    // the band and its reservations, and 3, at the content root's top,
    // observes nothing, as the root's own observation does not count them
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
    let mut ops = create(3..=3);
    let rows = [
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::Height, "100%"),
        (StyleId::AdditionalSafeAreaBottom, "20"),
        (StyleId::SafeAreaPaddingTop, "auto"),
    ];
    ops.push(style(2, vec![], &rows));
    ops.push(style(3, pad, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    let padded = |k: &mut Kernel, padding| {
        k.set_env(k.env().with_default_padding(padding)).unwrap();
    };
    padded(&mut k, Some([16.0; 4]));
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (0.0, 63.0, 390.0, 727.0));
    assert_eq!(top(&k, 3), 0.0);
    // Another default moves the root, and none pads nothing
    padded(&mut k, Some([8.0; 4]));
    lay_out(&mut k);
    assert_eq!(frame(&k, 2).1, 55.0);
    padded(&mut k, None);
    lay_out(&mut k);
    assert_eq!(frame(&k, 2).1, 47.0);
    padded(&mut k, Some([16.0; 4]));
    // Taking the top leaves its padding cleared, and 3 observes the band
    // under it; a negative addition lets the root under the chrome
    let mut txn = 1;
    let mut set = |k: &mut Kernel, rows: &[(StyleId, &str)]| {
        txn += 1;
        k.apply(0, txn, &[style(2, vec![], rows)]).unwrap();
        lay_out(k);
        (frame(k, 2), top(k, 3))
    };
    let taken = [(StyleId::SafeAreaIgnore, "container top")];
    assert_eq!(set(&mut k, &taken), ((0.0, 16.0, 390.0, 774.0), 47.0));
    let under = [(StyleId::AdditionalSafeAreaBottom, "-34")];
    assert_eq!(set(&mut k, &under).0, (0.0, 16.0, 390.0, 828.0));
    // While the band is unavailable the reservations still apply
    k.set_owner_band(2, None, &[]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (0.0, 16.0, 390.0, 828.0));
}

#[test]
fn a_sheet_holds_its_reservations_around_its_content() {
    // A sheet of 300 points of content over a 34-point band, its root adding
    // 20 below, is 354 tall
    let mut ops = create(3..=3);
    let rows = [
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::AdditionalSafeAreaBottom, "20"),
    ];
    ops.push(style(2, vec![], &rows));
    ops.push(style(3, vec![], &[(StyleId::Height, "300")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    let sheet = block(BlockAxis::Is(390.0), BlockAxis::AtMost(800.0));
    k.set_host_block(2, Some(sheet)).unwrap();
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some((390.0, 354.0)));
    let measured = k.measure_block(2, sheet).unwrap();
    assert_eq!(measured.used, (390.0, 354.0));
}

#[test]
fn a_top_layer_box_lays_out_what_it_reserves() {
    // A popover is no owner (D2): it lays its reservations out as any box
    let rows = [(StyleId::Width, "50"), (StyleId::SafeAreaPaddingTop, "10")];
    let mut ops = create(3..=4);
    ops.extend([style(3, vec![], &rows), popover(3)]);
    ops.push(style(4, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!((frame(&k, 4).1, frame(&k, 3).3), (10.0, 20.0));
}

#[test]
fn the_kernel_keeps_the_boxes_that_reserve() {
    // Layout and observations ask no box what it reserves while none does
    let mut k = reserving(&[(StyleId::SafeAreaPaddingTop, "4")]);
    assert!(k.arena.reserving.contains(slot(&k, 3)));
    assert_eq!(left(&k, 5), 51.0);
    // A box that stops reserving no longer moves what its descendants
    // observe, though the absolute 5 does not move
    let unpadded = [(StyleId::SafeAreaPaddingTop, "0")];
    k.apply(0, 2, &[style(3, vec![], &unpadded)]).unwrap();
    assert!(k.arena.reserving.is_empty());
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 47.0);
    let padded = style(5, vec![], &[(StyleId::SafeAreaPaddingTop, "4")]);
    k.apply(0, 3, &[padded]).unwrap();
    assert!(k.arena.reserving.contains(slot(&k, 5)));
    k.apply(0, 4, &[Op::DestroyView { id: 5 }]).unwrap();
    assert!(k.arena.reserving.is_empty());
}

#[test]
fn a_reservation_moves_only_the_edges_it_reserves() {
    // 3, 100 below the band the root takes, pads 10 at its left and by
    // default at its top, where no host reports one: the absolute 5, moved
    // up to the root's top, still observes the band's 47 there
    let rows = [
        (StyleId::MarginTop, "100"),
        (StyleId::SafeAreaPaddingLeft, "10"),
        (StyleId::SafeAreaPaddingTop, "auto"),
    ];
    let mut k = reserving(&rows);
    k.apply(0, 2, &[style(5, vec![], &[(StyleId::Top, "-100")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 5).1, left(&k, 5)), (0.0, 47.0));
}

#[test]
fn a_reservation_lies_inside_the_border() {
    // 3, clear of the band under a 10-point top border, adds 20 inside it:
    // the in-flow 4 sits clear of both, and the absolute 5, at the top of
    // 3's padding box, observes the 20
    let rows = [
        (StyleId::MarginTop, "100"),
        (StyleId::BorderWidthTop, "10"),
        (StyleId::BorderStyleTop, "solid"),
        (StyleId::AdditionalSafeAreaTop, "20"),
    ];
    let k = reserving(&rows);
    assert_eq!((frame(&k, 4).1, top(&k, 4)), (130.0, 0.0));
    assert_eq!((frame(&k, 5).1, left(&k, 5)), (110.0, 20.0));
}

#[test]
fn a_nested_reservation_widens_the_area_it_is_in() {
    // 3 at 100 adds 20, an area from 100 to 120; 6 in it, moved up to 50,
    // adds 10 more: the absolute 7 at 6's top observes the 30
    let rows = [
        (StyleId::MarginTop, "100"),
        (StyleId::AdditionalSafeAreaTop, "20"),
    ];
    let mut k = reserving(&rows);
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    let mut ops = create(6..=7);
    let moved = [
        (StyleId::PositionType, "relative"),
        (StyleId::Top, "-80"),
        (StyleId::Height, "10"),
        (StyleId::AdditionalSafeAreaTop, "10"),
    ];
    ops.push(style(6, vec![], &moved));
    let absolute = [(StyleId::PositionType, "absolute"), (StyleId::Top, "0")];
    ops.push(style(7, pad, &absolute));
    ops.extend([children(3, &[4, 5, 6]), children(6, &[7])]);
    k.apply(0, 2, &ops).unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 7).1, left(&k, 7)), (50.0, 30.0));
}

#[test]
fn a_reservation_inside_a_scroller_reaches_its_content() {
    // The scroller 6 takes the band that reaches it, but 3 inside it pads
    // 20 at its top: the absolute 5 at 3's top observes the 20
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    let mut ops = create(3..=6);
    let scroller = [(StyleId::OverflowY, "scroll"), (StyleId::Height, "400")];
    ops.push(style(6, vec![], &scroller));
    let rows = [
        (StyleId::PositionType, "relative"),
        (StyleId::SafeAreaPaddingTop, "20"),
    ];
    ops.push(style(3, vec![], &rows));
    let absolute = [(StyleId::PositionType, "absolute"), (StyleId::Top, "0")];
    ops.push(style(5, pad, &absolute));
    ops.push(style(4, vec![], &[(StyleId::Height, "10")]));
    let tree = [children(2, &[6]), children(6, &[3]), children(3, &[4, 5])];
    ops.extend([children(1, &[2])].into_iter().chain(tree));
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 20.0);
}

#[test]
fn a_reserving_box_that_moves_moves_what_its_descendants_observe() {
    // 3, static, adds 20 under the band; the absolute 5, which the root
    // holds at 60, observes 7 of the area to 67. Moved 100 down, 3 starts
    // its own area, of which 5, not moved, observes the whole 20
    let rows = [
        (StyleId::PositionType, "static"),
        (StyleId::AdditionalSafeAreaTop, "20"),
    ];
    let mut k = reserving(&rows);
    k.apply(0, 2, &[style(5, vec![], &[(StyleId::Top, "60")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 5).1, left(&k, 5)), (60.0, 7.0));
    k.apply(0, 3, &[style(3, vec![], &[(StyleId::MarginTop, "100")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 5).1, left(&k, 5)), (60.0, 20.0));
}

#[test]
fn a_reader_solves_past_the_band_in_a_deeper_area() {
    // Under the band's 47 and 3's 20, the absolute 5's top margin is a
    // tenth of its own top observation: it settles at 670/11, past the band
    let mut k = reserving(&[(StyleId::AdditionalSafeAreaTop, "20")]);
    let tenth = Node::Product(vec![number(0.1), observed(Edge::Top, None)]);
    k.apply(0, 2, &[style(5, vec![(StyleId::MarginTop, tenth)], &[])])
        .unwrap();
    lay_out(&mut k);
    let seen = k.observation(5).unwrap()[0].unwrap();
    assert!((seen - 670.0 / 11.0).abs() < 0.5, "{seen}");
}

#[test]
fn a_reservation_keeps_its_physical_edge_right_to_left() {
    let rows = [
        (StyleId::Direction, "rtl"),
        (StyleId::AdditionalSafeAreaLeft, "20"),
    ];
    assert_eq!(frame(&reserving(&rows), 4).0, 20.0);
}

#[test]
fn a_table_lays_its_reservation_outside_its_padding_and_spacing() {
    // The table 3 reserves 20 at its top, pads 5 and spaces its cells 2:
    // its cell's content starts 27 down, and the table is that, the cell's
    // 10 and the spacing below it tall
    let prop = |id, value: &str| Op::SetProp {
        id,
        prop: crate::PropId::SemanticTag,
        value: value.into(),
    };
    let mut ops = create(3..=6);
    ops.extend([prop(3, "table"), prop(4, "tr"), prop(5, "td")]);
    let rows = [
        (StyleId::PaddingTop, "5"),
        (StyleId::SafeAreaPaddingTop, "20"),
        (StyleId::BorderSpacing, "2"),
    ];
    ops.push(style(3, vec![], &rows));
    let flat = [(StyleId::PaddingTop, "0"), (StyleId::PaddingBottom, "0")];
    ops.push(style(5, vec![], &flat));
    ops.push(style(6, vec![], &[(StyleId::Height, "10")]));
    let tree = [children(3, &[4]), children(4, &[5]), children(5, &[6])];
    ops.extend(
        [children(1, &[2]), children(2, &[3])]
            .into_iter()
            .chain(tree),
    );
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!((frame(&k, 6).1, frame(&k, 3).3), (27.0, 39.0));
}

#[test]
fn a_measure_clears_what_the_roots_reservation_reads() {
    // The root, 100 wide, adds its own right observation at its top: 20
    // under a 20-point right band in a 100-point block, none in a 200-point
    // one, which a measure finds as the layout of that block does
    let reads = vec![(StyleId::AdditionalSafeAreaTop, observed(Edge::Right, None))];
    let mut ops = create(3..=3);
    ops.push(style(2, reads, &[(StyleId::Width, "100")]));
    ops.push(style(3, vec![], &[(StyleId::Height, "300")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(100.0, 844.0, ops);
    let sheet = |width| block(BlockAxis::Is(width), BlockAxis::AtMost(800.0));
    k.set_host_block(2, Some(sheet(100.0))).unwrap();
    k.set_owner_band(2, Some([0.0, 20.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some((100.0, 320.0)));
    let measured = k.measure_block(2, sheet(200.0)).unwrap();
    assert_eq!(measured.used, (200.0, 300.0));
    k.set_host_block(2, Some(sheet(200.0))).unwrap();
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some(measured.used));
}
