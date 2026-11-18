//! Taking the band inside the root (LLP 1123 D4): a box whose margin box
//! touches the area its parent passes it extends to that area's outer side,
//! its place in its parent's layout kept, as SwiftUI's `ignoresSafeArea`
//! measured on iOS 27.0 and 27.1 (§"Evidence: measured on expo/exact").
use super::*;

#[path = "ignore_due_tests.rs"]
mod due;

/// The owner 2 under a 34-point bottom band, its root clear of the band
/// and filling its 810-point content root as a column with `root`'s rows:
/// 3 grows, then 4 with `rows`, 100 tall, holding 5 at its full height.
fn column(root: &[(StyleId, &str)], rows: &[(StyleId, &str)]) -> Kernel {
    let mut own = vec![
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::Height, "100%"),
    ];
    own.extend_from_slice(root);
    let mut held = vec![(StyleId::Height, "100")];
    held.extend_from_slice(rows);
    let mut ops = create(3..=5);
    ops.push(style(2, vec![], &own));
    ops.push(style(3, vec![], &[(StyleId::FlexGrow, "1")]));
    ops.push(style(4, vec![], &held));
    ops.push(style(5, vec![], &[(StyleId::Height, "100%")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4]), children(4, &[5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    k
}

/// The owner 2 under `band`, its root clear of it with `root`'s rows,
/// holding `ops`' boxes, laid out once.
pub(super) fn laid(band: [f32; 4], root: &[(StyleId, &str)], mut ops: Vec<Op>) -> Kernel {
    let mut own = vec![(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    own.extend_from_slice(root);
    ops.push(style(2, vec![], &own));
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some(band), &[]).unwrap();
    lay_out(&mut k);
    k
}

const BOTTOM: (StyleId, &str) = (StyleId::SafeAreaIgnore, "container bottom");

#[test]
fn a_box_at_the_band_extends_to_the_allocations_edge_in_its_place() {
    // 4 sits at the safe area's bottom: it extends to the allocation's edge,
    // and 5, laid out in it, with it; 3 stays where it was
    let k = column(&[], &[BOTTOM]);
    assert_eq!(frame(&k, 4), (0.0, 710.0, 390.0, 134.0));
    assert_eq!(frame(&k, 5), frame(&k, 4));
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 710.0));
    // One layout more measures it
    assert_eq!(k.arena.observations.rounds, 1);
    // Its margin box touches: the margin lies inside what it takes
    let k = column(&[], &[BOTTOM, (StyleId::MarginBottom, "10")]);
    assert_eq!(frame(&k, 4), (0.0, 700.0, 390.0, 134.0));
    // Its parent's padding holds it clear, and the keyboard region is not
    // the container's
    let padded = [
        (StyleId::BoxSizing, "border-box"),
        (StyleId::PaddingBottom, "10"),
    ];
    let k = column(&padded, &[BOTTOM]);
    assert_eq!(frame(&k, 4), (0.0, 700.0, 390.0, 100.0));
    let keyboard = (StyleId::SafeAreaIgnore, "keyboard bottom");
    assert_eq!(frame(&column(&[], &[keyboard]), 4).3, 100.0);
    // Nor does it take an edge it does not name
    let top = (StyleId::SafeAreaIgnore, "container top");
    assert_eq!(frame(&column(&[], &[top]), 4).3, 100.0);
    // A box that stops ignoring is back in its place, and one made where a
    // removed one was is laid out as its own style says
    let mut k = column(&[], &[BOTTOM]);
    let none = [(StyleId::SafeAreaIgnore, "none")];
    k.apply(0, 2, &[style(4, vec![], &none)]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 710.0, 390.0, 100.0));
    let mut k = column(&[], &[BOTTOM]);
    let removed = [Op::DestroyView { id: 5 }, Op::DestroyView { id: 4 }];
    k.apply(0, 2, &removed).unwrap();
    assert!(k.arena.ignoring.is_empty());
    let mut ops = create(6..=6);
    ops.push(style(6, vec![], &[(StyleId::Height, "100")]));
    ops.push(children(2, &[3, 6]));
    k.apply(0, 3, &ops).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 6), (0.0, 710.0, 390.0, 100.0));
}

/// The owner 2 under a 47-point top band, its root clear of it: 4, of
/// automatic height, takes the top band with the top padding `padding` and
/// holds 5, 10 tall; 6, 10 tall, follows it.
fn header(padding: Node) -> Kernel {
    let mut ops = create(3..=6);
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    let pad = vec![(StyleId::PaddingTop, padding)];
    ops.push(style(2, vec![], &root));
    ops.push(style(4, pad, &[(StyleId::SafeAreaIgnore, "container top")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "10")]));
    ops.push(style(6, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[4, 6]), children(4, &[5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    k
}

#[test]
fn a_box_that_takes_the_top_pads_by_what_it_took() {
    // C31: 4 pads by its own observation, which the band fills, so 5 stays
    // clear of the band and 6 follows 4's content, as on the web under
    // `cover`, where 4 pads by `env(safe-area-inset-top)`
    let k = header(observed(Edge::Top, None));
    assert_eq!(frame(&k, 4), (0.0, 0.0, 390.0, 57.0));
    assert_eq!(frame(&k, 5), (0.0, 47.0, 390.0, 10.0));
    assert_eq!(frame(&k, 6).1, 57.0);
    // Its content's extent counts from its extended box
    assert_eq!(k.arena.content(slot(&k, 4)), (390.0, 57.0));
    // C32: at least 16, or what it took; nothing doubles
    let at_least = Node::Max(vec![px(16.0), observed(Edge::Top, None)]);
    let k = header(at_least);
    assert_eq!(
        (frame(&k, 4), frame(&k, 5).1),
        ((0.0, 0.0, 390.0, 57.0), 47.0)
    );
    // A padding the band covers lies in the band, measured from its outer
    // side, and its content moves into it
    let k = header(px(16.0));
    assert_eq!(
        (frame(&k, 4), frame(&k, 5).1),
        ((0.0, 0.0, 390.0, 57.0), 16.0)
    );
    assert_eq!(frame(&k, 6).1, 57.0);
    // So does a percentage, of the 390-point width
    let k = header(Node::Value(10.0, Unit::Percent));
    assert_eq!(
        (frame(&k, 4), frame(&k, 5).1),
        ((0.0, 0.0, 390.0, 57.0), 39.0)
    );
}

#[test]
fn a_row_that_takes_the_bottom_keeps_its_content_clear_as_a_tab_bar_does() {
    // A 49-point row pads its bottom by its own observation over a 34-point
    // band: 83 tall at the allocation's edge, its content above the band, as
    // UIKit's tab bar. Under `border-box` too, the band adds outside the
    // length, which need not count it
    for sizing in ["content-box", "border-box"] {
        let mut ops = create(3..=5);
        let pad = vec![(StyleId::PaddingBottom, observed(Edge::Bottom, None))];
        let rows = [
            BOTTOM,
            (StyleId::Height, "49"),
            (StyleId::BoxSizing, sizing),
        ];
        ops.push(style(3, vec![], &[(StyleId::FlexGrow, "1")]));
        ops.push(style(4, pad, &rows));
        ops.push(style(5, vec![], &[(StyleId::Height, "100%")]));
        ops.extend([children(1, &[2]), children(2, &[3, 4]), children(4, &[5])]);
        let column = [
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
        ];
        let k = laid([0.0, 0.0, 34.0, 0.0], &column, ops);
        assert_eq!(frame(&k, 4), (0.0, 761.0, 390.0, 83.0), "{sizing}");
        assert_eq!(frame(&k, 5), (0.0, 761.0, 390.0, 49.0), "{sizing}");
        assert_eq!(frame(&k, 3).3, 761.0, "{sizing}");
    }
}

#[test]
fn a_box_past_the_allocations_edge_takes_nothing_there() {
    // 4 touches the right band's inner side and lies 16 past the bottom
    // edge: it extends to the right edge and keeps its bottom
    let mut ops = create(3..=4);
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    let absolute = [
        (StyleId::SafeAreaIgnore, "container"),
        (StyleId::PositionType, "absolute"),
        (StyleId::Right, "0"),
        (StyleId::Bottom, "-50"),
        (StyleId::Width, "100"),
        (StyleId::Height, "100"),
    ];
    ops.push(style(2, vec![], &root));
    ops.push(style(4, vec![], &absolute));
    ops.extend([children(1, &[2]), children(2, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 44.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (246.0, 760.0, 144.0, 100.0));
}

#[test]
fn an_aligned_box_keeps_its_size_and_moves_within_what_it_takes() {
    let at = |align: &str| {
        let k = column(&[], &[BOTTOM, (StyleId::SafeAreaIgnoreAlign, align)]);
        assert_eq!(frame(&k, 5), frame(&k, 4));
        frame(&k, 4)
    };
    assert_eq!(at("top"), (0.0, 710.0, 390.0, 100.0));
    assert_eq!(at("center"), (0.0, 727.0, 390.0, 100.0));
    assert_eq!(at("bottom"), (0.0, 744.0, 390.0, 100.0));
    // A box at three bands, 47 above, 34 below and 20 on the left, moves
    // within all three
    let both = |align: &str| {
        let mut ops = create(4..=4);
        let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
        let rows = [
            (StyleId::SafeAreaIgnore, "container"),
            (StyleId::SafeAreaIgnoreAlign, align),
            (StyleId::Height, "100%"),
        ];
        ops.push(style(2, vec![], &root));
        ops.push(style(4, vec![], &rows));
        ops.extend([children(1, &[2]), children(2, &[4])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([47.0, 0.0, 34.0, 20.0]), &[])
            .unwrap();
        lay_out(&mut k);
        frame(&k, 4)
    };
    assert_eq!(both("top"), (10.0, 0.0, 370.0, 763.0));
    assert_eq!(both("center"), (10.0, 40.5, 370.0, 763.0));
    assert_eq!(both("bottom-leading"), (0.0, 81.0, 370.0, 763.0));
}

#[test]
fn a_box_inside_a_parent_that_took_the_band_extends_across_its_padding() {
    // 6 takes the band and pads 10 at its bottom; 4, at the bottom of its
    // content, lies 24 into the band and extends to the edge across the
    // padding
    let mut ops = create(3..=6);
    let root = [
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::Height, "100%"),
    ];
    let taking = [
        BOTTOM,
        (StyleId::FlexGrow, "1"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::JustifyContent, "flex-end"),
        (StyleId::PaddingBottom, "10"),
    ];
    ops.push(style(2, vec![], &root));
    ops.push(style(3, vec![], &[(StyleId::Height, "400")]));
    ops.push(style(6, vec![], &taking));
    ops.push(style(4, vec![], &[BOTTOM, (StyleId::Height, "100")]));
    ops.extend([children(1, &[2]), children(2, &[3, 6]), children(6, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 6), (0.0, 400.0, 390.0, 444.0));
    assert_eq!(frame(&k, 4), (0.0, 734.0, 390.0, 110.0));
    // 4 touches once 6 takes the band, a layout later
    assert_eq!(k.arena.observations.rounds, 2);
    // Without its padding, 4 is at the edge already and moves no further
    let rows = [(StyleId::PaddingBottom, "0")];
    let align = [(StyleId::SafeAreaIgnoreAlign, "bottom")];
    k.apply(0, 2, &[style(6, vec![], &rows), style(4, vec![], &align)])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 744.0, 390.0, 100.0));
}

#[test]
fn a_box_takes_a_reservation_to_its_outer_side() {
    // 6 reserves 20 at its bottom, as padding, above a 100-point 7: 4, at
    // the bottom of its content, extends to 6's padding edge, not the band
    let reserving = |below: bool| {
        let mut ops = create(3..=7);
        let root = [
            (StyleId::SafeAreaIgnore, "none"),
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
            (StyleId::Height, "100%"),
        ];
        let column = [
            (StyleId::SafeAreaPaddingBottom, "20"),
            (StyleId::Height, "300"),
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
            (StyleId::JustifyContent, "flex-end"),
        ];
        // 4 reads its own bottom observation into its left margin
        let reader = vec![(StyleId::MarginLeft, observed(Edge::Bottom, None))];
        ops.push(style(2, vec![], &root));
        ops.push(style(3, vec![], &[(StyleId::FlexGrow, "1")]));
        ops.push(style(6, vec![], &column));
        ops.push(style(4, reader, &[BOTTOM, (StyleId::Height, "50")]));
        ops.push(style(7, vec![], &[(StyleId::Height, "100")]));
        let held: &[ViewId] = if below { &[3, 6, 7] } else { &[3, 6] };
        ops.extend([children(1, &[2]), children(2, held), children(6, &[4])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        k
    };
    let k = reserving(true);
    assert_eq!(frame(&k, 6), (0.0, 390.0, 390.0, 320.0));
    assert_eq!(frame(&k, 4), (20.0, 640.0, 370.0, 70.0));
    // At the band's inner side, 6's reservation adds to the band, and 4
    // extends to the allocation's edge over both, observing their 54
    let k = reserving(false);
    assert_eq!(frame(&k, 4), (54.0, 740.0, 336.0, 104.0));
}

#[test]
fn logical_edges_follow_the_direction_and_an_auto_margin_aligns() {
    // A 44-point band on the right: under `rtl`, 4 takes its start there,
    // narrow or not, and `leading` aligns it to that side
    let side = |rows: &[(StyleId, &str)]| {
        let mut ops = create(3..=4);
        let root = [
            (StyleId::SafeAreaIgnore, "none"),
            (StyleId::Direction, "rtl"),
            (StyleId::Height, "100%"),
        ];
        let mut held = vec![(StyleId::Height, "100")];
        held.extend_from_slice(rows);
        ops.push(style(2, vec![], &root));
        ops.push(style(4, vec![], &held));
        ops.extend([children(1, &[2]), children(2, &[4])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([0.0, 44.0, 0.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        frame(&k, 4)
    };
    let start = (StyleId::SafeAreaIgnore, "container start");
    assert_eq!(side(&[start]), (0.0, 0.0, 390.0, 100.0));
    let narrow = [start, (StyleId::Width, "100")];
    assert_eq!(side(&narrow), (246.0, 0.0, 144.0, 100.0));
    let leading = [start, (StyleId::SafeAreaIgnoreAlign, "leading")];
    assert_eq!(side(&leading), (44.0, 0.0, 346.0, 100.0));
    // An `auto` margin only aligns: a centred box is clear of the band
    let centred = [
        start,
        (StyleId::Width, "100"),
        (StyleId::MarginLeft, "auto"),
        (StyleId::MarginRight, "auto"),
    ];
    assert_eq!(side(&centred), (123.0, 0.0, 100.0, 100.0));
}

#[test]
fn a_box_that_ignores_holds_its_childrens_margins() {
    // In block flow, 4 is a formatting context of its own, so 5's margin
    // stays inside it whether or not it touches the band
    let mut ops = create(3..=5);
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    ops.push(style(2, vec![], &root));
    ops.push(style(3, vec![], &[(StyleId::Height, "750")]));
    ops.push(style(4, vec![], &[BOTTOM]));
    let margin = [(StyleId::Height, "50"), (StyleId::MarginTop, "10")];
    ops.push(style(5, vec![], &margin));
    ops.extend([children(1, &[2]), children(2, &[3, 4]), children(4, &[5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 750.0, 390.0, 94.0));
    assert_eq!(frame(&k, 5), (0.0, 760.0, 390.0, 50.0));
}

#[test]
fn a_list_row_replayed_like_another_lays_out_its_extended_box() {
    // Each row of the list 6 reserves 20 at its bottom, where a box that
    // ignores lies with a child at its full height: a list's rows are laid
    // out by replaying one like them (Taffy patch 29), which asks the
    // extended box for real, as the first row recorded it
    let rows: Vec<ViewId> = (10..14).collect();
    let mut ops = vec![Op::CreateView {
        id: 6,
        node_type: NodeType::List,
    }];
    ops.extend(create(rows.iter().flat_map(|&r| [r, r + 10, r + 20])));
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    ops.push(style(2, vec![], &root));
    ops.push(style(6, vec![], &[(StyleId::Height, "100%")]));
    let row = [
        (StyleId::SafeAreaPaddingBottom, "20"),
        (StyleId::Height, "100"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::JustifyContent, "flex-end"),
    ];
    for &r in &rows {
        ops.push(style(r, vec![], &row));
        ops.push(style(r + 10, vec![], &[BOTTOM, (StyleId::Height, "30")]));
        ops.push(style(r + 20, vec![], &[(StyleId::Height, "100%")]));
        ops.extend([children(r, &[r + 10]), children(r + 10, &[r + 20])]);
    }
    ops.extend([children(1, &[2]), children(2, &[6]), children(6, &rows)]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    for (i, &r) in rows.iter().enumerate() {
        let y = 120.0 * i as f32 + 70.0;
        assert_eq!(frame(&k, r + 10), (0.0, y, 390.0, 50.0), "row {r}");
        assert_eq!(frame(&k, r + 20), frame(&k, r + 10), "row {r}");
    }
    assert!(k.tree().row_memo_counts().0 > 0);
}

#[test]
fn a_change_inside_an_extended_box_lays_out_only_its_path() {
    // 3, of automatic height, takes the 47-point top band and holds 100
    // rows: a row's change lays that row out again, not every row, since
    // patch 80 sizes 3 apart from its final layout and keeps its height
    // automatic for its rows
    let laid = |ignore: &str| {
        let rows: Vec<ViewId> = (10..110).collect();
        let mut ops = create(3..=3);
        ops.extend(create(rows.iter().copied()));
        let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
        ops.push(style(2, vec![], &root));
        ops.push(style(3, vec![], &[(StyleId::SafeAreaIgnore, ignore)]));
        for &r in &rows {
            ops.push(style(r, vec![], &[(StyleId::Height, "10")]));
        }
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &rows)]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        let before = k.tree().work.1;
        k.apply(0, 2, &[style(50, vec![], &[(StyleId::Width, "11")])])
            .unwrap();
        lay_out(&mut k);
        (frame(&k, 3).1, k.tree().work.1 - before)
    };
    let (still, alone) = laid("none");
    let (extended, laid) = laid("container top");
    assert_eq!((still, extended), (47.0, 0.0));
    assert!(
        laid <= alone + 1,
        "{laid} leaves laid out, {alone} unextended"
    );
}

#[test]
fn an_absolute_box_in_an_extended_box_is_placed_from_its_containing_block() {
    // 3 takes the 47-point top band; 4, absolute in it, is 8 below the
    // root's top, which its insets are measured from
    let mut ops = create(3..=4);
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    let absolute = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Top, "8"),
        (StyleId::Left, "8"),
        (StyleId::Width, "20"),
        (StyleId::Height, "20"),
    ];
    ops.push(style(2, vec![], &root));
    ops.push(style(
        3,
        vec![],
        &[
            (StyleId::SafeAreaIgnore, "container top"),
            (StyleId::Height, "10"),
        ],
    ));
    ops.push(style(4, vec![], &absolute));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 57.0));
    assert_eq!(frame(&k, 4), (8.0, 55.0, 20.0, 20.0));
}

#[test]
fn a_baseline_aligned_box_that_takes_the_band_settles() {
    // In a row aligned by baselines, 3, 30 tall, holds text whose baseline
    // lies below the 5's beside it, and 3 below that; were 3 to give the row
    // its text's baseline it would touch the band, take it, lose it and touch
    // again. It gives the row none, so the row places it the same either way
    let mut ops = create(3..=3);
    for (id, size) in [(4, "60"), (5, "50")] {
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
    ops.extend([children(1, &[2]), children(2, &[3, 5]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    let settled = frame(&k, 3);
    assert!(
        k.arena.observations.rounds <= 1,
        "{} rounds",
        k.arena.observations.rounds
    );
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), settled);
}

#[test]
fn a_band_change_owes_a_layout_where_a_box_takes_it() {
    // The root takes the top band and pads 10; 3 lies 10 into it and takes
    // it. No box reads an observation, yet the band going owes a layout
    let ops = vec![
        style(2, vec![], &[(StyleId::PaddingTop, "10")]),
        style(
            3,
            vec![],
            &[
                (StyleId::SafeAreaIgnore, "container top"),
                (StyleId::Height, "50"),
            ],
        ),
        children(1, &[2]),
        children(2, &[3]),
    ];
    let mut ops = ops;
    ops.splice(0..0, create(3..=3));
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 60.0));
    assert!(k.set_owner_band(2, Some([0.0; 4]), &[]).unwrap());
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), (0.0, 10.0, 390.0, 50.0));
}

#[test]
fn a_scroller_that_ignores_an_edge_passes_the_band_to_its_content() {
    // The scroller 3 takes the top band; its content 4 lies in it and pads
    // by its observation there, as UIKit's under `never` at rest
    let mut ops = create(3..=5);
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    let scroller = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::OverflowY, "scroll"),
        (StyleId::Height, "100%"),
    ];
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
    ops.push(style(2, vec![], &root));
    ops.push(style(3, vec![], &scroller));
    ops.push(style(4, pad, &[]));
    ops.push(style(5, vec![], &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
    ]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3).1, 0.0);
    assert_eq!(top(&k, 4), 47.0);
    // The bottom it does not ignore stays its own
    assert_eq!(k.observation(4).unwrap()[2], Some(0.0));
    // There a reservation above it does not reach its content: 6 reserves
    // 20 at its bottom around the same scroller, and 5, at the scroller's
    // end, takes none of it
    let mut ops = create(3..=6);
    let reserving = [
        (StyleId::SafeAreaPaddingBottom, "20"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::Height, "100%"),
    ];
    let scroller = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::OverflowY, "scroll"),
        (StyleId::FlexGrow, "1"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::JustifyContent, "flex-end"),
    ];
    ops.push(style(2, vec![], &root));
    ops.push(style(6, vec![], &reserving));
    ops.push(style(3, vec![], &scroller));
    ops.push(style(5, vec![], &[BOTTOM, (StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[6]),
        children(6, &[3]),
        children(3, &[5]),
    ]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 5).3, 10.0);
    // An owner's root that scrolls and takes the band passes it too
    let mut ops = create(3..=3);
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
    ops.push(style(2, vec![], &[(StyleId::OverflowY, "scroll")]));
    ops.push(style(3, pad, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 47.0);
}

#[test]
fn an_extended_box_of_automatic_height_keeps_its_childrens_percentages_automatic() {
    // 3, a column that packs to its end, takes the top band past its 30-point
    // maximum, the band filling its 10-point padding; its 50% child 4 stays
    // automatic, as it would be if 3 did not touch, and 5 packs to the end
    // of the extended box
    let mut ops = create(3..=5);
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    let column = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::JustifyContent, "flex-end"),
        (StyleId::MaxHeight, "30"),
        (StyleId::PaddingTop, "10"),
    ];
    ops.push(style(2, vec![], &root));
    ops.push(style(3, vec![], &column));
    ops.push(style(4, vec![], &[(StyleId::Height, "50%")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 57.0));
    assert_eq!(frame(&k, 4).3, 0.0);
    assert_eq!(frame(&k, 5).1, 47.0);
}

#[test]
fn a_content_regions_shell_box_takes_what_it_touches() {
    // In the shell of the region 2, 6 reserves 20 at its bottom and holds
    // 7, which ignores the bottom and takes the reservation; 8 and 9 do the
    // same in the region's content 3, 9 with a 10-point margin that lies in
    // what it takes
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3, 5, 6, 7, 8, 9]);
    let reserving = [
        (StyleId::SafeAreaPaddingBottom, "20"),
        (StyleId::Height, "100"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::JustifyContent, "flex-end"),
    ];
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    ops.push(style(2, vec![], &super::REGION));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    for (outer, inner, margin) in [(6, 7, "0"), (8, 9, "10")] {
        ops.push(style(outer, vec![], &reserving));
        let rows = [
            BOTTOM,
            (StyleId::Height, "50"),
            (StyleId::MarginBottom, margin),
        ];
        ops.push(style(inner, vec![], &rows));
        ops.push(children(outer, &[inner]));
    }
    ops.extend([
        children(3, &[8]),
        children(2, &[3, 5]),
        children(1, &[6, 2]),
    ]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    super::lay_out_region(&mut k, Default::default());
    assert_eq!(frame(&k, 7), (0.0, 50.0, 390.0, 70.0));
    let (outer, inner) = (frame(&k, 8), frame(&k, 9));
    assert_eq!((inner.1, inner.3), (outer.1 + 40.0, 70.0));
    assert!(k.arena.observations.rounds < 3);
    // One that stops ignoring takes nothing
    let none = [(StyleId::SafeAreaIgnore, "none")];
    k.apply(0, 2, &[style(7, vec![], &none)]).unwrap();
    super::lay_out_region(&mut k, Default::default());
    assert_eq!(frame(&k, 7), (0.0, 50.0, 390.0, 50.0));
}

#[test]
fn a_replayed_row_aligns_a_box_that_ignores_as_a_fresh_one_does() {
    // Each row of the list 6 aligns by baselines a box that ignores, holding
    // 60-point text, beside 50-point text: a row laid out by a replay asks
    // that box for its answer, which has no baseline, as a fresh row's does
    let placed = |memo: bool| {
        let rows: Vec<ViewId> = (10..14).collect();
        let mut ops = vec![Op::CreateView {
            id: 6,
            node_type: NodeType::List,
        }];
        ops.extend(create(rows.iter().flat_map(|&r| [r, r + 10])));
        for &r in &rows {
            for (id, size) in [(r + 20, "60"), (r + 30, "50")] {
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
            let row = [
                (StyleId::Display, "flex"),
                (StyleId::AlignItems, "baseline"),
            ];
            let taking = [
                (StyleId::SafeAreaIgnore, "container top"),
                (StyleId::Height, "30"),
            ];
            ops.push(style(r, vec![], &row));
            ops.push(style(r + 10, vec![], &taking));
            ops.extend([children(r, &[r + 10, r + 30]), children(r + 10, &[r + 20])]);
        }
        let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
        ops.push(style(2, vec![], &root));
        ops.push(style(6, vec![], &[(StyleId::Height, "100%")]));
        ops.extend([children(1, &[2]), children(2, &[6]), children(6, &rows)]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_row_layout_memo(memo);
        k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        lay_out(&mut k);
        rows.iter().map(|&r| frame(&k, r + 10)).collect::<Vec<_>>()
    };
    assert_eq!(placed(true), placed(false));
}

#[test]
fn a_box_that_filling_its_padding_moves_clear_keeps_it_whole() {
    // 4 ends a column where its 16-point padding reaches the band: filling
    // that padding would move its edge clear, so it took the band, gave it
    // back and touched again, and keeps the padding whole
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[(StyleId::Height, "734")]));
    ops.push(style(4, vec![], &[BOTTOM, (StyleId::PaddingBottom, "16")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "60")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4]), children(4, &[5])]);
    let mut k = laid([0.0, 0.0, 34.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 4), (0.0, 734.0, 390.0, 110.0));
    assert_eq!(frame(&k, 5), (0.0, 734.0, 390.0, 60.0));
    assert_eq!(k.arena.observations.rounds, 3);
    lay_out(&mut k);
    assert_eq!(k.arena.observations.rounds, 0);
    assert_eq!(frame(&k, 4), (0.0, 734.0, 390.0, 110.0));
    // Without the padding it no longer reaches the band
    let none = [(StyleId::PaddingBottom, "0")];
    k.apply(0, 2, &[style(4, vec![], &none)]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 734.0, 390.0, 60.0));
    // So does a padded box that a row aligns by baselines, which filling its
    // padding would move 10 lower
    let mut ops = create(3..=4);
    let taking = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::PaddingTop, "16"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(3, vec![], &taking));
    ops.push(style(4, vec![], &[(StyleId::Height, "20")]));
    ops.push(children(1, &[2]));
    ops.push(children(2, &[3, 4]));
    let row = [
        (StyleId::Display, "flex"),
        (StyleId::AlignItems, "baseline"),
    ];
    let mut k = laid([47.0, 0.0, 0.0, 0.0], &row, ops);
    let settled = frame(&k, 3);
    assert_eq!((settled.1, settled.3), (0.0, 73.0));
    assert_eq!(k.arena.observations.rounds, 3);
    lay_out(&mut k);
    assert_eq!((frame(&k, 3), k.arena.observations.rounds), (settled, 0));
}

#[test]
fn an_extended_leaf_places_its_content_inside_its_whole_padding() {
    // The text 3 pads by its observation: its content sits below the band
    let ops = vec![
        Op::CreateView {
            id: 3,
            node_type: NodeType::Text,
        },
        Op::SetProp {
            id: 3,
            prop: crate::PropId::Text,
            value: "word".into(),
        },
        style(
            3,
            vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
            &[(StyleId::SafeAreaIgnore, "container top")],
        ),
        children(1, &[2]),
        children(2, &[3]),
    ];
    let k = laid([47.0, 0.0, 0.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 3).1, 0.0);
    assert_eq!(top(&k, 3), 47.0);
}

#[test]
fn an_extended_box_keeps_its_maximum_extended() {
    // 3, a column at most 100 tall holding two 80-point children, takes the
    // top band: its maximum is 147 extended, and its children shrink to it
    let mut ops = create(3..=6);
    let column = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::MaxHeight, "100"),
    ];
    ops.push(style(3, vec![], &column));
    for id in [4, 5] {
        ops.push(style(id, vec![], &[(StyleId::Height, "80")]));
    }
    ops.push(style(6, vec![], &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 6]),
        children(3, &[4, 5]),
    ]);
    let k = laid([47.0, 0.0, 0.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 147.0));
    assert_eq!((frame(&k, 4).3, frame(&k, 5).3), (73.5, 73.5));
    assert_eq!(frame(&k, 6).1, 147.0);
}

#[test]
fn an_aligned_box_moves_without_what_its_containing_block_holds_outside_it() {
    // 3 takes the 47-point top band with `align top` and moves 47 up, or the
    // 20-point left band with `align leading` and moves 20 left; 4, absolute
    // in it and held by the root, stays 8 from the root's edge
    let cases = [
        (
            [47.0, 0.0, 0.0, 0.0],
            "container top",
            "top",
            StyleId::Top,
            (0.0, 55.0),
        ),
        (
            [0.0, 0.0, 0.0, 20.0],
            "container left",
            "leading",
            StyleId::Left,
            (28.0, 0.0),
        ),
    ];
    for (band, ignore, align, inset, at) in cases {
        let mut ops = create(3..=4);
        let aligned = [
            (StyleId::SafeAreaIgnore, ignore),
            (StyleId::SafeAreaIgnoreAlign, align),
            (StyleId::Width, "100"),
            (StyleId::Height, "10"),
        ];
        let absolute = [
            (StyleId::PositionType, "absolute"),
            (inset, "8"),
            (StyleId::Width, "20"),
            (StyleId::Height, "20"),
        ];
        ops.push(style(3, vec![], &aligned));
        ops.push(style(4, vec![], &absolute));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
        let k = laid(band, &[], ops);
        assert_eq!(frame(&k, 3), (0.0, 0.0, 100.0, 10.0), "{align}");
        assert_eq!((frame(&k, 4).0, frame(&k, 4).1), at, "{align}");
    }
}

#[test]
fn an_extended_boxs_content_counts_from_its_start_under_rtl() {
    // Under `rtl`, 3 takes its start, the right band's 44 points; 4, 100
    // wide at its start, ends its content 100 from the extended box's start
    let mut ops = create(3..=4);
    ops.push(style(
        3,
        vec![],
        &[(StyleId::SafeAreaIgnore, "container start")],
    ));
    ops.push(style(
        4,
        vec![],
        &[(StyleId::Width, "100"), (StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let k = laid([0.0, 44.0, 0.0, 0.0], &[(StyleId::Direction, "rtl")], ops);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 10.0));
    assert_eq!(frame(&k, 4).0, 290.0);
    assert_eq!(k.arena.content(slot(&k, 3)).0, 100.0);
}

#[test]
fn an_extended_box_with_a_ratio_lays_its_subtree_out_at_its_extended_size() {
    // 3, 390 wide at a ratio of 2, takes the top band: 242 tall, its 20-point
    // child packed to the end of the extended box
    let mut ops = create(3..=4);
    let ratio = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::AspectRatio, "2"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::JustifyContent, "flex-end"),
    ];
    ops.push(style(3, vec![], &ratio));
    ops.push(style(4, vec![], &[(StyleId::Height, "20")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let k = laid([47.0, 0.0, 0.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 242.0));
    assert_eq!(frame(&k, 4).1, 222.0);
}

#[test]
fn a_padded_box_in_a_centred_row_settles() {
    // The row centres 3, which pads 16 around a 20-point box, beside the
    // 30-point 4: filling its padding would centre it clear of the band, so
    // it keeps the padding whole
    let mut ops = create(3..=5);
    let taking = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::PaddingTop, "16"),
    ];
    ops.push(style(3, vec![], &taking));
    ops.push(style(5, vec![], &[(StyleId::Height, "20")]));
    ops.push(style(4, vec![], &[(StyleId::Height, "30")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4]), children(3, &[5])]);
    let row = [
        (StyleId::Display, "flex"),
        (StyleId::AlignItems, "center"),
        (StyleId::Height, "auto"),
    ];
    let mut k = laid([47.0, 0.0, 0.0, 0.0], &row, ops.clone());
    let settled = frame(&k, 3);
    assert_eq!(settled.1, 0.0);
    assert_eq!(k.arena.observations.rounds, 3);
    lay_out(&mut k);
    assert_eq!((frame(&k, 3), k.arena.observations.rounds), (settled, 0));
    // Once the row stops centring it, filling the padding leaves it at the
    // band, so the band fills it, as in a fresh layout of the same tree
    let start = [(StyleId::AlignItems, "flex-start")];
    k.apply(0, 2, &[style(2, vec![], &start)]).unwrap();
    lay_out(&mut k);
    let row = [row[0], start[0], row[2]];
    let fresh = laid([47.0, 0.0, 0.0, 0.0], &row, ops);
    assert_eq!(frame(&k, 3).3, 67.0);
    assert_eq!(
        (frame(&k, 3), frame(&k, 5)),
        (frame(&fresh, 3), frame(&fresh, 5))
    );
}

#[test]
fn a_taking_still_moving_at_the_bound_is_left_and_listed() {
    // 26 nested columns each pad their bottom 1 and take the bottom band:
    // each extension moves the boxes inside it a point, so at the 24th round
    // the two innermost, 27 and 28, still take more, and are left as they
    // were laid out and listed
    let levels: Vec<ViewId> = (3..=28).collect();
    let mut ops = create(levels.iter().copied());
    let column = [
        BOTTOM,
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::FlexGrow, "1"),
        (StyleId::PaddingBottom, "1"),
    ];
    for &id in &levels {
        ops.push(style(id, vec![], &column));
    }
    ops.push(children(1, &[2]));
    ops.push(children(2, &[3]));
    for pair in levels.windows(2) {
        ops.push(children(pair[0], &[pair[1]]));
    }
    let root = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let k = laid([0.0, 0.0, 34.0, 0.0], &root, ops);
    assert_eq!(k.arena.observations.rounds, 24);
    assert_eq!(k.safe_area_unsettled(), vec![27, 28]);
    for id in [27, 28] {
        let taken = k.arena.observations.taken(slot(&k, id)).unwrap();
        let placed = k
            .tree()
            .layout(k.arena.taffy(slot(&k, id)).unwrap())
            .size
            .height;
        assert_eq!(frame(&k, id).3 - placed, taken.by[0] + taken.by[2], "{id}");
    }
    // The next layout goes on from there
    let mut k = k;
    lay_out(&mut k);
    assert!(k.arena.observations.rounds > 0);
}

#[test]
fn a_clean_extended_box_placed_anew_lays_its_subtree_out_again() {
    // 4 fills the column below 3 and takes the bottom band; 3 then grows by
    // the 34 points 4 took, so the engine asks 4 for the size its extended
    // layout had: its child 5 still fills the box it extends to
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[(StyleId::Height, "100")]));
    ops.push(style(4, vec![], &[BOTTOM, (StyleId::FlexGrow, "1")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "100%")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4]), children(4, &[5])]);
    let column = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let mut k = laid([0.0, 0.0, 34.0, 0.0], &column, ops);
    assert_eq!(frame(&k, 5), (0.0, 100.0, 390.0, 744.0));
    k.apply(0, 2, &[style(3, vec![], &[(StyleId::Height, "134")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 134.0, 390.0, 710.0));
    assert_eq!(frame(&k, 5), frame(&k, 4));
}

#[test]
fn a_list_row_that_ignores_is_laid_out_by_its_own_algorithm() {
    // A row without a baseline answers apart from its algorithm, which a
    // replayed trace does not hold, so the rows record and replay nothing
    let rows: Vec<ViewId> = (10..14).collect();
    let mut ops = vec![Op::CreateView {
        id: 6,
        node_type: NodeType::List,
    }];
    ops.extend(create(rows.iter().flat_map(|&r| [r, r + 10])));
    for &r in &rows {
        let row = [
            (StyleId::SafeAreaIgnore, "container"),
            (StyleId::Display, "flex"),
        ];
        ops.push(style(r, vec![], &row));
        ops.push(style(r + 10, vec![], &[(StyleId::Height, "30")]));
        ops.push(children(r, &[r + 10]));
    }
    ops.push(style(6, vec![], &[(StyleId::Height, "100%")]));
    ops.extend([children(1, &[2]), children(2, &[6]), children(6, &rows)]);
    let mut k = laid([47.0, 0.0, 0.0, 0.0], &[], ops);
    lay_out(&mut k);
    assert_eq!(k.row_layout_memo_counts(), (0, 0));
    assert_eq!(frame(&k, 13).1, 137.0);
}

#[test]
fn a_box_moved_clear_of_the_band_gives_it_back() {
    // The root takes a 47-point top band; 3 pads by its own observation, and
    // 4, which takes the top, lies 10 into the band until 3 grows past it.
    // With 16 points of padding it is still clear
    for padding in ["0", "16"] {
        let mut ops = create(3..=4);
        let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
        ops.push(style(3, pad, &[(StyleId::Height, "10")]));
        let taking = [
            (StyleId::SafeAreaIgnore, "container top"),
            (StyleId::Height, "100"),
            (StyleId::PaddingTop, padding),
        ];
        ops.push(style(4, vec![], &taking));
        ops.extend([children(1, &[2]), children(2, &[3, 4])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        assert_eq!(frame(&k, 3).3, 57.0);
        assert_eq!(frame(&k, 4).1, 57.0, "padding {padding}");
        assert!(k.arena.observations.taken(slot(&k, 4)).is_none());
        // Brought back into the band by a later change, it touches anew and
        // the band fills its padding, what it gave back before forgotten
        let flat = [(StyleId::PaddingTop, "0")];
        k.apply(0, 2, &[style(3, vec![], &flat)]).unwrap();
        lay_out(&mut k);
        let height = if padding == "16" { 116.0 } else { 110.0 };
        assert_eq!(frame(&k, 4), (0.0, 0.0, 390.0, height), "padding {padding}");
    }
    // A box after it, added once it has settled, moves it clear too
    let mut k = column(&[], &[BOTTOM]);
    lay_out(&mut k);
    let mut ops = create(6..=6);
    ops.push(style(6, vec![], &[(StyleId::Height, "20")]));
    ops.push(children(2, &[3, 4, 6]));
    k.apply(0, 2, &ops).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 690.0, 390.0, 100.0));
}

#[test]
fn an_extended_container_keeps_its_maximum_on_a_known_axis() {
    // 3 grows in a column to its 100-point maximum and takes the top band:
    // as a flex row or a grid, it lays its stretched child 4 out 147 tall
    for display in ["flex", "grid"] {
        let mut ops = create(3..=4);
        let rows = [
            (StyleId::SafeAreaIgnore, "container top"),
            (StyleId::Display, display),
            (StyleId::FlexGrow, "1"),
            (StyleId::MaxHeight, "100"),
        ];
        ops.push(style(3, vec![], &rows));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
        let column = [
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
        ];
        let k = laid([47.0, 0.0, 0.0, 0.0], &column, ops);
        assert_eq!(frame(&k, 3), (0.0, 0.0, 390.0, 147.0), "{display}");
        assert_eq!(frame(&k, 4).3, 147.0, "{display}");
    }
}

#[test]
fn a_scroller_that_comes_to_reach_the_band_passes_it() {
    // The scroller 4 ignores every edge above the 44-point 6; when 6 goes,
    // 4 reaches the band and its content 5 observes it, and when 6 comes
    // back it does not
    let mut ops = create(3..=6);
    let scroller = [
        (StyleId::SafeAreaIgnore, "container"),
        (StyleId::OverflowY, "scroll"),
        (StyleId::FlexGrow, "1"),
    ];
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))];
    ops.push(style(4, vec![], &scroller));
    ops.push(style(5, pad, &[(StyleId::Height, "2000")]));
    ops.push(style(6, vec![], &[(StyleId::Height, "44")]));
    ops.extend([children(1, &[2]), children(2, &[4, 6]), children(4, &[5])]);
    let column = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let mut k = laid([47.0, 0.0, 34.0, 0.0], &column, ops);
    assert_eq!(left(&k, 5), 0.0);
    k.apply(0, 2, &[style(6, vec![], &[(StyleId::Display, "none")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 34.0);
    k.apply(0, 3, &[style(6, vec![], &[(StyleId::Display, "block")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 0.0);
}

#[test]
fn a_scroller_passes_a_reservation_it_reaches() {
    // 3, clear of the band, reserves 20 at its bottom; the scroller 4 fills
    // it and ignores the bottom, so its content 5 observes the reservation
    let mut ops = create(3..=7);
    let reserving = [
        (StyleId::SafeAreaPaddingBottom, "20"),
        (StyleId::Height, "300"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let scroller = [
        BOTTOM,
        (StyleId::OverflowY, "scroll"),
        (StyleId::FlexGrow, "1"),
    ];
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))];
    ops.push(style(3, vec![], &reserving));
    ops.push(style(4, vec![], &scroller));
    ops.push(style(5, pad, &[(StyleId::Height, "2000")]));
    ops.push(style(7, vec![], &[(StyleId::Height, "100")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 7]),
        children(3, &[4]),
        children(4, &[5]),
    ]);
    let mut k = laid([47.0, 0.0, 34.0, 0.0], &[], ops);
    assert_eq!(left(&k, 5), 20.0);
    // Once 4 stops ignoring, its content inset takes the reservation again
    let none = [(StyleId::SafeAreaIgnore, "none")];
    k.apply(0, 2, &[style(4, vec![], &none)]).unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 0.0);
}

#[test]
fn a_box_that_gives_back_one_edge_keeps_its_padding_whole_there() {
    // 4 fills the root's 763-point content box but for 13 points and takes
    // the top and the bottom band: filling its 16-point bottom padding would
    // move its bottom clear, so it keeps that whole and fills the top
    let mut ops = create(4..=5);
    let both = [
        (StyleId::SafeAreaIgnore, "container top bottom"),
        (StyleId::PaddingBottom, "16"),
    ];
    ops.push(style(4, vec![], &both));
    ops.push(style(5, vec![], &[(StyleId::Height, "750")]));
    ops.extend([children(1, &[2]), children(2, &[4]), children(4, &[5])]);
    let k = laid([47.0, 0.0, 34.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 4), (0.0, 0.0, 390.0, 844.0));
    assert_eq!(frame(&k, 5), (0.0, 0.0, 390.0, 750.0));
    let taken = k.arena.observations.taken(slot(&k, 4)).unwrap();
    assert_eq!(taken.whole, [false, false, true, false]);
    assert_eq!(k.arena.observations.rounds, 3);
}

#[test]
fn a_box_whose_place_moves_tests_its_whole_padding_again() {
    // 4 ends the 724-point 3's wrapper 6, after the 10-point 7: filling its
    // padding would move it clear, so it keeps it whole. When 7 grows by 4,
    // 6 moves without being laid out anew, and with it 4's place, so 4 tests
    // the padding again; filling it still moves 4 clear, and 4 takes 4 less
    let mut ops = create(3..=7);
    ops.push(style(3, vec![], &[(StyleId::Height, "724")]));
    ops.push(style(4, vec![], &[BOTTOM, (StyleId::PaddingBottom, "16")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "60")]));
    ops.push(style(7, vec![], &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[7, 6]),
        children(6, &[3, 4]),
    ]);
    ops.push(children(4, &[5]));
    let mut k = laid([0.0, 0.0, 34.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 4), (0.0, 734.0, 390.0, 110.0));
    k.apply(0, 2, &[style(7, vec![], &[(StyleId::Height, "14")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4), (0.0, 738.0, 390.0, 106.0));
    assert_eq!(k.arena.observations.rounds, 3);
}

#[test]
fn a_box_in_a_row_resized_from_above_tests_its_whole_padding_again() {
    // The row 6 fills the 36-point 8 and centres 3, which pads 16 above a
    // 20-point box beside the 30-point 4: 3 keeps its padding whole. Once 8
    // is 20 tall, filling the padding leaves 3 at the band, so the band
    // fills it, as in a fresh layout of the same tree
    let tree = |height| {
        let mut ops = create(3..=8);
        let taking = [
            (StyleId::SafeAreaIgnore, "container top"),
            (StyleId::PaddingTop, "16"),
        ];
        let row = [
            (StyleId::Display, "flex"),
            (StyleId::AlignItems, "center"),
            (StyleId::Height, "100%"),
        ];
        ops.push(style(3, vec![], &taking));
        ops.push(style(4, vec![], &[(StyleId::Height, "30")]));
        ops.push(style(5, vec![], &[(StyleId::Height, "20")]));
        ops.push(style(6, vec![], &row));
        ops.push(style(8, vec![], &[(StyleId::Height, height)]));
        ops.extend([children(1, &[2]), children(2, &[8]), children(8, &[6])]);
        ops.extend([children(6, &[3, 4]), children(3, &[5])]);
        ops
    };
    let mut k = laid([47.0, 0.0, 0.0, 0.0], &[], tree("36"));
    assert_eq!(frame(&k, 3), (0.0, 0.0, frame(&k, 3).2, 83.0));
    k.apply(0, 2, &[style(8, vec![], &[(StyleId::Height, "20")])])
        .unwrap();
    lay_out(&mut k);
    let fresh = laid([47.0, 0.0, 0.0, 0.0], &[], tree("20"));
    assert_eq!(frame(&k, 3).3, 67.0);
    assert_eq!(
        (frame(&k, 3), frame(&k, 5)),
        (frame(&fresh, 3), frame(&fresh, 5))
    );
}

#[test]
fn a_box_in_a_content_region_keeps_its_padding_whole_with_nothing_to_lay_out() {
    // In the region's content 3, 8 reserves 20 below its 100-point content
    // box, and 9 ends that box after the 34-point 10, its 16-point
    // padding reaching the reservation: filling it would move 9 clear, so
    // 9 keeps it whole, and a layout with nothing changed lays out nothing
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3, 5, 8, 9, 10]);
    let reserving = [
        (StyleId::SafeAreaPaddingBottom, "20"),
        (StyleId::Height, "100"),
    ];
    let rows = [
        BOTTOM,
        (StyleId::Height, "50"),
        (StyleId::PaddingBottom, "16"),
    ];
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    ops.push(style(2, vec![], &super::REGION));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    ops.push(style(8, vec![], &reserving));
    ops.push(style(9, vec![], &rows));
    ops.push(style(10, vec![], &[(StyleId::Height, "34")]));
    ops.extend([children(8, &[10, 9]), children(3, &[8])]);
    ops.extend([children(2, &[3, 5]), children(1, &[2])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    super::lay_out_region(&mut k, Default::default());
    let settled = frame(&k, 9);
    assert_eq!(settled.3, 86.0);
    let taken = k.arena.observations.taken(slot(&k, 9)).unwrap();
    assert_eq!(taken.whole, [false, false, true, false]);
    super::lay_out_region(&mut k, Default::default());
    assert_eq!((frame(&k, 9), k.arena.observations.rounds), (settled, 0));
}

#[test]
fn an_owners_root_that_scrolls_passes_the_band_where_it_reaches_it() {
    // The root 2 scrolls and ignores the bottom; at half the allocation it
    // ends clear of the band, and its content 5 observes none of it, until
    // the root fills the allocation
    let mut ops = create(5..=5);
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))];
    let root = |height| {
        let rows = [
            (StyleId::SafeAreaIgnore, "container bottom"),
            (StyleId::OverflowY, "scroll"),
            (StyleId::Height, height),
        ];
        style(2, vec![], &rows)
    };
    ops.push(root("50%"));
    ops.push(style(5, pad, &[(StyleId::Height, "2000")]));
    ops.extend([children(1, &[2]), children(2, &[5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 0.0);
    k.apply(0, 2, &[root("100%")]).unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 34.0);
    k.apply(0, 3, &[root("50%")]).unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 5), 0.0);
}

#[test]
fn an_extended_box_publishes_its_padding_less_a_host_cover() {
    // 4 takes the 34-point bottom band, and a host covers 20 points of its
    // top: it publishes no padding of its own there and the cover once
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::FlexGrow, "1")]));
    ops.push(style(4, vec![], &[BOTTOM, (StyleId::Height, "49")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let column = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let mut k = laid([0.0, 0.0, 34.0, 0.0], &column, ops);
    k.set_host_cover(4, Some(crate::HostCover::Edges([20.0, 0.0, 0.0, 0.0])))
        .unwrap();
    lay_out(&mut k);
    let padding = k.layout_padding(k.arena.key(slot(&k, 4))).unwrap();
    assert_eq!((padding.1, padding.3), (20.0, 0.0));
}
