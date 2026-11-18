//! When the solve measures again what a box that ignores takes (LLP 1123
//! D4): wherever its taking can move while its frame holds.
use super::*;

#[test]
fn a_box_under_a_parent_that_starts_to_scroll_takes_anew() {
    // 4 ends the root's column and takes the bottom band; once the root
    // scrolls, its content inset takes the band and 4 takes nothing
    let mut k = column(&[], &[BOTTOM]);
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 710.0, 390.0, 134.0));
    let scrolls = [(StyleId::OverflowY, "scroll")];
    k.apply(0, 2, &[style(2, vec![], &scrolls)]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 710.0, 390.0, 100.0));
}

#[test]
fn a_box_takes_anew_what_its_parent_reserves_anew() {
    // 4 ends 3's content box and takes the 20 points 3 reserves below it;
    // when 3 reserves 30, 4 takes 30
    let mut ops = create(3..=4);
    let reserving = |points| {
        let rows = [
            (StyleId::SafeAreaPaddingBottom, points),
            (StyleId::Height, "100"),
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
            (StyleId::JustifyContent, "flex-end"),
        ];
        style(3, vec![], &rows)
    };
    ops.push(reserving("20"));
    ops.push(style(4, vec![], &[BOTTOM, (StyleId::Height, "50")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = laid([47.0, 0.0, 34.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 4).3, 70.0);
    k.apply(0, 2, &[reserving("30")]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4).3, 80.0);
}

#[test]
fn a_box_in_a_scroller_that_comes_to_reach_the_band_takes_it() {
    // The scroller 4 ignores the bottom and ends above the 44-point 6; 7
    // ends the scroller's content at the band's inner side, where 4 passes
    // nothing. When 6 goes, 4 reaches the band and passes it, and 7, which
    // has not moved, takes it
    let mut ops = create(3..=8);
    let scroller = [
        BOTTOM,
        (StyleId::OverflowY, "scroll"),
        (StyleId::FlexGrow, "1"),
    ];
    ops.push(style(4, vec![], &scroller));
    ops.push(style(6, vec![], &[(StyleId::Height, "44")]));
    ops.push(style(7, vec![], &[BOTTOM, (StyleId::Height, "50")]));
    ops.push(style(8, vec![], &[(StyleId::Height, "713")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[4, 6]),
        children(4, &[8, 7]),
    ]);
    let column = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let mut k = laid([47.0, 0.0, 34.0, 0.0], &column, ops);
    assert_eq!(frame(&k, 7), (0.0, 760.0, 390.0, 50.0));
    k.apply(0, 2, &[style(6, vec![], &[(StyleId::Display, "none")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 7), (0.0, 760.0, 390.0, 84.0));
}

#[test]
fn a_box_that_names_other_edges_takes_anew() {
    // 3 takes the 47-point top band; once it names the bottom instead, which
    // it lies clear of, it takes nothing, though its frame need not move
    let mut ops = create(3..=3);
    let top = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(3, vec![], &top));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = laid([47.0, 0.0, 34.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 57.0));
    k.apply(0, 2, &[style(3, vec![], &[BOTTOM])]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), (0.0, 47.0, 390.0, 10.0));
}

#[test]
fn a_box_hidden_and_shown_takes_again() {
    // 4 takes the top band inside 3; hidden with 3 it takes nothing, and
    // shown again it takes the band as before
    let mut ops = create(3..=4);
    let top = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(4, vec![], &top));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = laid([47.0, 0.0, 0.0, 0.0], &[], ops);
    let taken = frame(&k, 4);
    assert_eq!(taken, (0.0, 0.0, 390.0, 57.0));
    k.apply(0, 2, &[style(3, vec![], &[(StyleId::Display, "none")])])
        .unwrap();
    lay_out(&mut k);
    k.apply(0, 3, &[style(3, vec![], &[(StyleId::Display, "block")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), taken);
}

#[test]
fn a_box_whose_margin_changes_in_place_takes_anew() {
    // 4, absolute at the bottom of the 60-point 3, has a top margin of 10%
    // of 3's width, which places its margin edge and not its box: at 100
    // wide it takes 30 of the 47-point band, and at 300 wide 10, as in a
    // fresh layout
    let tree = |width| {
        let mut ops = create(3..=4);
        let holder = [
            (StyleId::PositionType, "relative"),
            (StyleId::Height, "60"),
            (StyleId::Width, width),
        ];
        let taking = [
            (StyleId::SafeAreaIgnore, "container top"),
            (StyleId::PositionType, "absolute"),
            (StyleId::Left, "0"),
            (StyleId::Bottom, "0"),
            (StyleId::Width, "50"),
            (StyleId::Height, "20"),
            (StyleId::MarginTop, "10%"),
        ];
        ops.push(style(3, vec![], &holder));
        ops.push(style(4, vec![], &taking));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        k
    };
    let mut k = tree("100");
    assert_eq!(frame(&k, 4), (0.0, 10.0, 50.0, 50.0));
    k.apply(0, 2, &[style(3, vec![], &[(StyleId::Width, "300")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 30.0, 50.0, 30.0));
    assert_eq!(frame(&k, 4), frame(&tree("300"), 4));
}

#[test]
fn a_box_that_names_its_start_takes_anew_when_its_direction_turns() {
    // 4 names its start inside 3, which pads 44 on each side under 44-point
    // side bands: it takes the left band, and once 3 turns to `rtl` the
    // right, as in a fresh layout
    let tree = |direction| {
        let mut ops = create(3..=4);
        let padded = [
            (StyleId::PaddingLeft, "44"),
            (StyleId::PaddingRight, "44"),
            (StyleId::Direction, direction),
        ];
        let start = [
            (StyleId::SafeAreaIgnore, "container start"),
            (StyleId::Height, "10"),
        ];
        ops.push(style(3, vec![], &padded));
        ops.push(style(4, vec![], &start));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([0.0, 44.0, 0.0, 44.0]), &[])
            .unwrap();
        lay_out(&mut k);
        k
    };
    let mut k = tree("ltr");
    assert_eq!(frame(&k, 4), (0.0, 0.0, 346.0, 10.0));
    let rtl = [(StyleId::Direction, "rtl")];
    k.apply(0, 2, &[style(3, vec![], &rtl)]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (44.0, 0.0, 346.0, 10.0));
    assert_eq!(frame(&k, 4), frame(&tree("rtl"), 4));
}

#[test]
fn a_box_in_a_content_region_takes_anew_where_its_place_moves() {
    // In the region's content 3, 9 ends 8's content box at the 20 points 8
    // reserves below it and takes them; when 10 before it shrinks by 4, 9
    // lies clear and takes nothing
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3, 5, 8, 9, 10]);
    let reserving = [
        (StyleId::SafeAreaPaddingBottom, "20"),
        (StyleId::Height, "100"),
    ];
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    ops.push(style(2, vec![], &super::super::REGION));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    ops.push(style(8, vec![], &reserving));
    ops.push(style(9, vec![], &[BOTTOM, (StyleId::Height, "50")]));
    ops.push(style(10, vec![], &[(StyleId::Height, "50")]));
    ops.extend([children(8, &[10, 9]), children(3, &[8])]);
    ops.extend([children(2, &[3, 5]), children(1, &[2])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    super::super::lay_out_region(&mut k, Default::default());
    assert_eq!(frame(&k, 9).3, 70.0);
    k.apply(0, 2, &[style(10, vec![], &[(StyleId::Height, "46")])])
        .unwrap();
    super::super::lay_out_region(&mut k, Default::default());
    assert_eq!(frame(&k, 9).3, 50.0);
}

#[test]
fn a_row_aligned_by_baselines_reads_no_baseline_from_a_cached_box() {
    // In a row aligned by baselines, 3 ignores and holds text; when the row
    // lays out again for a change beside it, 3's answer comes from the cache
    // and still gives the row no baseline, as in a fresh layout
    let tree = |size| {
        let mut ops = create([3, 6]);
        ops.push(style(
            6,
            vec![],
            &[(StyleId::Width, "10"), (StyleId::Height, "10")],
        ));
        for (id, size) in [(4, "60"), (5, size)] {
            ops.push(Op::CreateView {
                id,
                node_type: NodeType::Text,
            });
            ops.push(Op::SetProp {
                id,
                prop: crate::PropId::Text,
                value: "word".into(),
            });
            ops.push(style(id, vec![], &[(StyleId::FontSize, size)]));
        }
        let root = [
            (StyleId::SafeAreaIgnore, "none"),
            (StyleId::Display, "flex"),
            (StyleId::AlignItems, "baseline"),
        ];
        let taking = [
            (StyleId::SafeAreaIgnore, "container top"),
            (StyleId::Height, "30"),
        ];
        ops.push(style(2, vec![], &root));
        ops.push(style(3, vec![], &taking));
        ops.extend([
            children(1, &[2]),
            children(2, &[3, 5, 6]),
            children(3, &[4]),
        ]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        k
    };
    let mut k = tree("50");
    k.apply(0, 2, &[style(5, vec![], &[(StyleId::FontSize, "40")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), frame(&tree("40"), 3));
    // So does a change to a sibling that leaves 3's inputs as they were
    let wide = [(StyleId::Width, "20")];
    k.apply(0, 3, &[style(6, vec![], &wide)]).unwrap();
    lay_out(&mut k);
    let mut fresh = tree("40");
    fresh.apply(0, 2, &[style(6, vec![], &wide)]).unwrap();
    lay_out(&mut fresh);
    assert_eq!(frame(&k, 3), frame(&fresh, 3));
}
