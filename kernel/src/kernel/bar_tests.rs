//! Bars (LLP 1123 D12): a child that names an edge lies in a slot on that edge of its parent,
//! out of its flow, and reserves the slot's depth, which the parent's in-flow children are laid
//! out clear of and its descendants observe.
use super::*;

/// The owner root 2, 390 by 844 under a 47-point top band and a 34-point bottom one, which it
/// clears, so its content root, which it fills, runs from 47 to 810. Frames are in the owner's
/// space.
fn root(rows: &[(StyleId, &'static str)], ops: Vec<Op>) -> Kernel {
    let mut all = vec![(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    all.extend_from_slice(rows);
    let mut ops = ops;
    ops.insert(0, style(2, vec![], &all));
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    k
}

fn bar(edge: &'static str) -> (StyleId, &'static str) {
    (StyleId::SafeAreaBar, edge)
}

#[test]
fn a_bar_lies_on_its_edge_and_the_content_clear_of_it() {
    // 3, a 50-point top bar, and 5, a 40-point bottom one, lie on the content root's edges,
    // across its width, and 4, in flow between them, starts below the top bar
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(4, vec![], &[(StyleId::Height, "100")]));
    ops.push(style(5, vec![], &[bar("bottom"), (StyleId::Height, "40")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5])]);
    let k = root(&[], ops);
    assert_eq!(frame(&k, 3), (0.0, 47.0, 390.0, 50.0));
    assert_eq!(frame(&k, 5), (0.0, 810.0 - 40.0, 390.0, 40.0));
    assert_eq!(frame(&k, 4), (0.0, 97.0, 390.0, 100.0));
    // A bar takes no round of the readers' solve
    assert_eq!(k.arena.observations.rounds, 0);
}

#[test]
fn later_bars_lie_inside_earlier_ones() {
    // 3 on the top, then 4 on the left, below 3 and down to the bottom, then 5 on the top again,
    // below 3 and right of 4: tree order, the first nearest the edge
    let mut ops = create(3..=6);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(4, vec![], &[bar("left"), (StyleId::Width, "60")]));
    ops.push(style(5, vec![], &[bar("top"), (StyleId::Height, "30")]));
    ops.push(style(6, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5, 6])]);
    let k = root(&[], ops);
    assert_eq!(frame(&k, 3), (0.0, 47.0, 390.0, 50.0));
    assert_eq!(frame(&k, 4), (0.0, 97.0, 60.0, 810.0 - 97.0));
    assert_eq!(frame(&k, 5), (60.0, 97.0, 330.0, 30.0));
    // The in-flow child lies clear of all three
    assert_eq!(frame(&k, 6), (60.0, 127.0, 330.0, 10.0));
}

#[test]
fn a_bar_is_placed_along_its_edge_by_align_self_and_held_by_its_margins() {
    // A 100-point bar centred in the 390-point slot, and one stretched less its 16-point margins,
    // whose depth includes its vertical margins
    let mut ops = create(3..=4);
    let centred = [
        bar("top"),
        (StyleId::Height, "20"),
        (StyleId::Width, "100"),
        (StyleId::AlignSelf, "center"),
    ];
    ops.push(style(3, vec![], &centred));
    let margined = [
        bar("top"),
        (StyleId::Height, "20"),
        (StyleId::MarginLeft, "16"),
        (StyleId::MarginRight, "16"),
        (StyleId::MarginBottom, "8"),
    ];
    ops.push(style(4, vec![], &margined));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let k = root(&[], ops);
    assert_eq!(frame(&k, 3), (145.0, 47.0, 100.0, 20.0));
    assert_eq!(frame(&k, 4), (16.0, 67.0, 358.0, 20.0));
    assert_eq!(
        k.arena
            .observations
            .bars
            .slots
            .get(&slot(&k, 4))
            .map(|s| s.depth),
        Some(28.0)
    );
}

#[test]
fn a_parent_sized_by_its_content_is_as_wide_as_its_widest_bar() {
    // 3, a row's item sized by its content, holds a 9-point box and a 300-point top bar: it is
    // 300 wide, and as tall as the bar and the box
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[]));
    ops.push(style(
        4,
        vec![],
        &[(StyleId::Width, "9"), (StyleId::Height, "10")],
    ));
    ops.push(style(
        5,
        vec![],
        &[
            bar("top"),
            (StyleId::Width, "300"),
            (StyleId::Height, "20"),
            (StyleId::AlignSelf, "flex-start"),
        ],
    ));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    let row = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "row"),
        (StyleId::AlignItems, "flex-start"),
    ];
    let k = root(&row, ops);
    assert_eq!(frame(&k, 3).2, 300.0);
    assert_eq!(frame(&k, 3).3, 30.0);
    assert_eq!(frame(&k, 4).1, 47.0 + 20.0);
}

#[test]
fn a_bar_lies_past_its_parents_own_reservations() {
    // 3 pads its safe area by 20 points on top: its bar lies below that, and its in-flow child
    // below both; a flex row's children are laid out in a row below its bar, which is no item
    let mut ops = create(3..=6);
    let row = [
        (StyleId::SafeAreaPaddingTop, "20"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "row"),
    ];
    ops.push(style(3, vec![], &row));
    ops.push(style(4, vec![], &[bar("top"), (StyleId::Height, "50")]));
    for id in [5, 6] {
        ops.push(style(
            id,
            vec![],
            &[(StyleId::Width, "40"), (StyleId::Height, "10")],
        ));
    }
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4, 5, 6]),
    ]);
    let k = root(&[], ops);
    assert_eq!(frame(&k, 4), (0.0, 47.0 + 20.0, 390.0, 50.0));
    assert_eq!((frame(&k, 5).0, frame(&k, 5).1), (0.0, 47.0 + 70.0));
    assert_eq!((frame(&k, 6).0, frame(&k, 6).1), (40.0, 47.0 + 70.0));
    assert_eq!(frame(&k, 3).3, 80.0);
    // A parent under no band takes no round
    assert_eq!(k.arena.observations.rounds, 0);
}

#[test]
fn descendants_observe_the_bars_and_a_bar_only_the_earlier_ones() {
    // 5, absolutely placed at the content root's top, observes the band's clearance and both
    // bars; 4, the second bar, lies past the first and observes none of its own depth
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    let reads = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    ops.push(style(
        4,
        reads.clone(),
        &[bar("top"), (StyleId::Height, "30")],
    ));
    let pinned = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Top, "0"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(5, reads, &pinned));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5])]);
    let k = root(&[], ops);
    assert_eq!(k.observation(4).unwrap()[0], Some(0.0));
    assert_eq!(k.observation(5).unwrap()[0], Some(80.0));
}

#[test]
fn a_box_out_of_flow_or_an_owners_root_is_no_bar() {
    // An absolutely positioned box, a hidden one and a value naming two edges make no bar: the
    // in-flow 6 starts at the content root's top
    let mut ops = create(3..=6);
    ops.push(style(
        3,
        vec![],
        &[
            bar("top"),
            (StyleId::PositionType, "absolute"),
            (StyleId::Height, "50"),
        ],
    ));
    ops.push(style(
        4,
        vec![],
        &[
            bar("top"),
            (StyleId::Display, "none"),
            (StyleId::Height, "50"),
        ],
    ));
    ops.push(style(
        5,
        vec![],
        &[bar("top bottom"), (StyleId::Height, "0")],
    ));
    ops.push(style(6, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5, 6])]);
    let k = root(&[], ops);
    assert_eq!(frame(&k, 6).1, 47.0);
    // An owner's root that names an edge is laid out as its host's block, no bar
    let mut k = root(&[bar("top")], vec![children(1, &[2])]);
    lay_out(&mut k);
    assert_eq!(frame(&k, 2).3, 810.0 - 47.0);
}

/// A docked keyboard whose top lies at `top` of an 844-point allocation.
fn keyboard(top: f32) -> Option<crate::kernel::Keyboard> {
    Some(crate::kernel::Keyboard {
        frame: [0.0, top, 390.0, 844.0 - top],
        docked: true,
    })
}

#[test]
fn a_roots_bar_rests_on_the_keyboard_and_the_area_follows_it() {
    // Under `resizes-content` the content root ends at the keyboard's top at 516 and the root's
    // 44-point bottom bar rests on it, 472 to 516. 4, in flow above the bar, takes the area to
    // its outer side when it ignores: both regions to the allocation's edge, the container
    // region to the keyboard's top. 5, pinned to the content root's bottom, observes the bar
    let run = |ignore: &'static str| {
        let mut ops = create(3..=5);
        let column = [
            (StyleId::SafeAreaIgnore, "none"),
            (StyleId::Height, "100%"),
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
        ];
        ops.push(style(2, vec![], &column));
        ops.push(style(3, vec![], &[(StyleId::FlexGrow, "1")]));
        ops.push(style(
            4,
            vec![],
            &[(StyleId::Height, "100"), (StyleId::SafeAreaIgnore, ignore)],
        ));
        let pinned = vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))];
        ops.push(style(
            5,
            pinned,
            &[
                (StyleId::PositionType, "absolute"),
                (StyleId::Bottom, "0"),
                (StyleId::Height, "10"),
            ],
        ));
        ops.extend(create(6..=6));
        ops.push(style(6, vec![], &[bar("bottom"), (StyleId::Height, "44")]));
        ops.extend([children(1, &[2]), children(2, &[3, 4, 6, 5])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
            .unwrap();
        k.set_keyboard_policy(Some(crate::kernel::KeyboardPolicy::ResizesContent));
        k.set_owner_keyboard(2, keyboard(516.0)).unwrap();
        lay_out(&mut k);
        k
    };
    let k = run("none");
    assert_eq!(frame(&k, 6), (0.0, 472.0, 390.0, 44.0));
    assert_eq!(frame(&k, 4).1 + frame(&k, 4).3, 472.0);
    assert_eq!(k.observation(5).unwrap()[2], Some(44.0));
    let bottom = |k: &Kernel| frame(k, 4).1 + frame(k, 4).3;
    assert_eq!(bottom(&run("bottom")), 844.0);
    assert_eq!(bottom(&run("container bottom")), 516.0);
}

#[test]
fn a_roots_bar_lies_inside_its_border() {
    // The root's 6-point top border lies from 47 to 53, its 50-point top bar from 53 to 103,
    // and 5, pinned to its padding box's top, lies under the whole bar
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(4, vec![], &[(StyleId::Height, "10")]));
    let reads = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    let pinned = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Top, "0"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(5, reads, &pinned));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5])]);
    let border = [
        (StyleId::BorderWidthTop, "6"),
        (StyleId::BorderStyleTop, "solid"),
    ];
    let k = root(&border, ops);
    assert_eq!((frame(&k, 3).1, frame(&k, 4).1), (53.0, 103.0));
    assert_eq!(k.observation(5).unwrap()[0], Some(50.0));
}

#[test]
fn a_bar_on_a_root_that_takes_its_edge_lies_past_the_band() {
    // The root takes its top band, so its content root starts at the allocation's edge and its
    // bar's slot past the band, where a bar pinned to the controller view's safe-area guide
    // lies; 4, laid out past the bar's depth, observes the band and the bar to 97, and the bar
    // follows the band to 59
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    let reads = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    ops.push(style(4, reads, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = root(&[(StyleId::SafeAreaIgnore, "container top")], ops);
    assert_eq!(frame(&k, 3), (0.0, 47.0, 390.0, 50.0));
    assert_eq!(frame(&k, 4).1, 50.0);
    assert_eq!(k.observation(4).unwrap()[0], Some(47.0));
    k.set_owner_band(2, Some([59.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3).1, 59.0);
}

#[test]
fn a_bar_of_a_box_under_an_owners_band_lies_past_it() {
    // In a root that takes its top band, 3 lies at the allocation's edge, under the band: its
    // top bar lies past the band, and 5 past the bar's depth
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[(StyleId::Height, "200")]));
    ops.push(style(4, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    let k = root(&[(StyleId::SafeAreaIgnore, "container top")], ops);
    assert_eq!(frame(&k, 3).1, 0.0);
    assert_eq!((frame(&k, 4).1, frame(&k, 5).1), (47.0, 50.0));
}

#[test]
fn a_bar_that_changes_lays_its_parents_ancestors_out_again() {
    // 4, positioned and sized by its content in a column, holds a 50-point top bar and 6, which
    // grows to 100; with the bar 70 tall it is 170 tall, its child 100, as laid out afresh
    let mut ops = create(3..=6);
    let column = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    ops.push(style(3, vec![], &[(StyleId::Height, "20")]));
    ops.push(style(
        4,
        vec![],
        &[column[0], column[1], (StyleId::PositionType, "relative")],
    ));
    ops.push(style(5, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(
        6,
        vec![],
        &[(StyleId::FlexBasis, "100"), (StyleId::MinHeight, "0")],
    ));
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 4]),
        children(4, &[5, 6]),
    ]);
    let mut k = root(&column, ops);
    assert_eq!((frame(&k, 4).3, frame(&k, 6).3), (150.0, 100.0));
    k.apply(
        0,
        10,
        &[style(5, vec![], &[bar("top"), (StyleId::Height, "70")])],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 4).3, frame(&k, 6).3), (170.0, 100.0));
}

#[test]
fn a_bar_is_measured_in_the_space_its_parent_is_offered() {
    // Two panes share a 390-point row, each holding a top bar of wrapping text: each pane's
    // least size is its bar's least, not its longest line, so each is 195 wide
    let mut ops = create(3..=6);
    for (pane, bar_id) in [(3, 5), (4, 6)] {
        ops.push(style(
            pane,
            vec![],
            &[(StyleId::FlexGrow, "1"), (StyleId::FlexBasis, "0")],
        ));
        ops.push(style(bar_id, vec![], &[bar("top")]));
        ops.push(Op::CreateView {
            id: bar_id + 10,
            node_type: NodeType::Text,
        });
        let words = "word ".repeat(40);
        ops.push(Op::SetProp {
            id: bar_id + 10,
            prop: crate::PropId::Text,
            value: words.as_str().into(),
        });
        ops.extend([children(pane, &[bar_id]), children(bar_id, &[bar_id + 10])]);
    }
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let row = [(StyleId::Display, "flex"), (StyleId::FlexDirection, "row")];
    let k = root(&row, ops);
    assert_eq!((frame(&k, 3).2, frame(&k, 4).2), (195.0, 195.0));
}

#[test]
fn order_moves_no_bar_until_it_rejoins_its_flow() {
    // In the column 5, 3, a top bar with `order: 1`, comes before the top bar 6 in the tree and
    // lies on the edge, 6 inside it and 4 in flow below both; when 5 starts scrolling neither is
    // a bar, and `order` puts 3 last
    let mut ops = create(3..=6);
    ops.push(style(
        3,
        vec![],
        &[bar("top"), (StyleId::Height, "50"), (StyleId::Order, "1")],
    ));
    ops.push(style(4, vec![], &[(StyleId::Height, "30")]));
    let column = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::Height, "200"),
    ];
    ops.push(style(5, vec![], &column));
    ops.push(style(6, vec![], &[bar("top"), (StyleId::Height, "30")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[5]),
        children(5, &[3, 6, 4]),
    ]);
    let mut k = root(&[], ops);
    let tops = |k: &Kernel| [3, 6, 4].map(|id| frame(k, id).1);
    assert_eq!(tops(&k), [47.0, 97.0, 127.0]);
    k.apply(
        0,
        10,
        &[style(5, vec![], &[(StyleId::OverflowY, "scroll")])],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(tops(&k), [107.0, 47.0, 77.0]);
}

#[test]
fn a_bar_its_host_replaces_takes_no_slot() {
    // 3, a 50-point top bar with an 8-point margin below it, holds 4 down to 105, and 5, pinned
    // to the root's top, observes its 58 points; replaced whole by a native control it takes no
    // room, its margin included, and 5 observes none of it
    let mut ops = create(3..=5);
    let rows = [
        bar("top"),
        (StyleId::Height, "50"),
        (StyleId::MarginBottom, "8"),
    ];
    ops.push(style(3, vec![], &rows));
    ops.push(style(4, vec![], &[(StyleId::Height, "10")]));
    let reads = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    let pinned = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Top, "0"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(5, reads, &pinned));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5])]);
    let mut k = root(&[], ops);
    assert_eq!(frame(&k, 4).1, 105.0);
    assert_eq!(k.observation(5).unwrap()[0], Some(58.0));
    k.set_host_cover(3, Some(crate::kernel::HostCover::Whole))
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4).1, 47.0);
    assert_eq!(k.observation(5).unwrap()[0], Some(0.0));
}

#[test]
fn a_bar_keeps_to_its_bounds_and_its_auto_margins_place_it() {
    // A stretched bottom bar stops at its 300-point maximum; one 200 wide with `auto` margins
    // along the edge is centred; a padded one's maximum is its content's
    let mut ops = create(3..=4);
    ops.push(style(
        3,
        vec![],
        &[
            bar("bottom"),
            (StyleId::Height, "20"),
            (StyleId::MaxWidth, "300"),
        ],
    ));
    let centred = [
        bar("bottom"),
        (StyleId::Height, "20"),
        (StyleId::Width, "200"),
        (StyleId::MarginLeft, "auto"),
        (StyleId::MarginRight, "auto"),
    ];
    ops.push(style(4, vec![], &centred));
    // A `content-box` bar's maximum leaves out its padding, as a flex item's does
    let padded = [
        bar("bottom"),
        (StyleId::Height, "20"),
        (StyleId::MaxWidth, "300"),
        (StyleId::PaddingLeft, "16"),
        (StyleId::PaddingRight, "16"),
    ];
    ops.extend(create(5..=5));
    ops.push(style(5, vec![], &padded));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5])]);
    let k = root(&[], ops);
    assert_eq!((frame(&k, 3).0, frame(&k, 3).2), (0.0, 300.0));
    assert_eq!((frame(&k, 4).0, frame(&k, 4).2), (95.0, 200.0));
    assert_eq!(frame(&k, 5).2, 332.0);
}

#[test]
fn a_size_and_its_bounds_keep_their_border_box() {
    // A `content-box` parent with a 50-point top bar is 300 tall at a height of 300, its filling
    // child 250, at a 300-point maximum over 400 points of content, and at a 300-point minimum
    // over 10, as a block's child and as a flex item alike: the bar takes its room from the
    // content
    let cases = [
        ((StyleId::Height, "300"), "100%", Some(250.0)),
        ((StyleId::MaxHeight, "300"), "400", None),
        ((StyleId::MinHeight, "300"), "10", None),
    ];
    let flex = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    for (bound, content, child) in cases {
        for parent in [&[][..], &flex[..]] {
            let mut ops = create(3..=5);
            ops.push(style(3, vec![], &[bound, (StyleId::FlexShrink, "0")]));
            ops.push(style(4, vec![], &[bar("top"), (StyleId::Height, "50")]));
            let rows = [(StyleId::Height, content), (StyleId::FlexShrink, "0")];
            ops.push(style(5, vec![], &rows));
            ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
            let k = root(parent, ops);
            assert_eq!(frame(&k, 3).3, 300.0, "{bound:?} in {parent:?}");
            if let Some(child) = child {
                assert_eq!(frame(&k, 5).3, child, "{bound:?} in {parent:?}");
            }
        }
    }
}

#[test]
fn a_bar_that_becomes_absolutely_positioned_is_placed_by_its_containing_block() {
    // 4, a 44-point bottom bar of the static block 3, becomes a 100 by 44 box positioned 16
    // points in from the content root's bottom right corner: the root contains it, and 3,
    // which no longer reserves it, is as tall as its content
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[]));
    ops.push(style(4, vec![], &[bar("bottom"), (StyleId::Height, "44")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    let mut k = root(&[], ops);
    assert_eq!(frame(&k, 4), (0.0, 57.0, 390.0, 44.0));
    let placed = [
        bar("none"),
        (StyleId::PositionType, "absolute"),
        (StyleId::Bottom, "16"),
        (StyleId::Right, "16"),
        (StyleId::Width, "100"),
        (StyleId::Height, "44"),
    ];
    k.apply(0, 10, &[style(4, vec![], &placed)]).unwrap();
    lay_out(&mut k);
    assert_eq!(
        frame(&k, 4),
        (390.0 - 16.0 - 100.0, 810.0 - 16.0 - 44.0, 100.0, 44.0)
    );
    assert_eq!(frame(&k, 3).3, 10.0);
}

#[test]
fn a_positioned_parent_holds_its_bars_once_and_lays_out_again_when_one_leaves() {
    // 3, a positioned column sized by its content in the scroller 8, holds the 50-point top bar
    // 4, the 40-point bottom bar 5 and 6, 10 tall: 100 tall, 7 below it, and the scroller's
    // content 110, with no rectangle left where a bar's static position would be. When 5
    // becomes absolutely positioned 3 is 60 tall, and 7 and the content end with it
    let mut ops = create(3..=8);
    let column = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::PositionType, "relative"),
    ];
    ops.push(style(3, vec![], &column));
    ops.push(style(4, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(5, vec![], &[bar("bottom"), (StyleId::Height, "40")]));
    ops.push(style(6, vec![], &[(StyleId::Height, "10")]));
    ops.push(style(7, vec![], &[(StyleId::Height, "10")]));
    ops.push(style(
        8,
        vec![],
        &[(StyleId::OverflowY, "scroll"), (StyleId::Height, "200")],
    ));
    ops.extend([
        children(1, &[2]),
        children(2, &[8]),
        children(8, &[3, 7]),
        children(3, &[4, 5, 6]),
    ]);
    let mut k = root(&[], ops);
    let laid = |k: &Kernel| (frame(k, 3).3, frame(k, 7).1, k.node(8).unwrap().content.1);
    assert_eq!(laid(&k), (100.0, 147.0, 110.0));
    let placed = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Bottom, "0"),
        (StyleId::Height, "40"),
    ];
    k.apply(0, 10, &[style(5, vec![], &placed)]).unwrap();
    lay_out(&mut k);
    assert_eq!(laid(&k), (60.0, 107.0, 70.0));
}

#[test]
fn a_child_becomes_a_bar_and_stops_being_one() {
    // 3 in flow, then a bar, then in flow again, and a removed bar leaves its parent with none;
    // a scroller's child that names an edge stays in its flow
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::Height, "50")]));
    ops.push(style(4, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = root(&[], ops);
    assert_eq!(frame(&k, 4).1, 97.0);
    k.apply(
        0,
        10,
        &[style(3, vec![], &[bar("bottom"), (StyleId::Height, "50")])],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 3).1, frame(&k, 4).1), (810.0 - 50.0, 47.0));
    // A style patch keeps the rows it does not name, so the bar is unset by name
    k.apply(
        0,
        11,
        &[style(3, vec![], &[bar("none"), (StyleId::Height, "50")])],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4).1, 97.0);
    k.apply(
        0,
        12,
        &[style(3, vec![], &[bar("top"), (StyleId::Height, "50")])],
    )
    .unwrap();
    lay_out(&mut k);
    k.apply(0, 13, &[children(2, &[4])]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4).1, 47.0);
    let scroller = [(StyleId::OverflowY, "scroll"), (StyleId::Height, "100%")];
    k.apply(
        0,
        14,
        &[
            style(
                2,
                vec![],
                &[(StyleId::SafeAreaIgnore, "none"), scroller[0], scroller[1]],
            ),
            children(2, &[3, 4]),
        ],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4).1, 97.0);
}

/// A document (1), 390 by 844 and outside every owner, under the session's `band`, holding 2
/// with `rows` and what `ops` makes. Frames are in the document's space.
fn document(band: [f32; 4], rows: &[(StyleId, &'static str)], mut ops: Vec<Op>) -> Kernel {
    let mut k = Kernel::with_monospace();
    ops.splice(0..0, create(1..=2));
    ops.push(style(1, vec![], &[(StyleId::Height, "844")]));
    ops.push(style(2, vec![], rows));
    ops.extend([children(1, &[2]), Op::AttachRoot { id: 1 }]);
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band(band).unwrap();
    lay_out(&mut k);
    k
}

#[test]
fn a_bar_lies_past_its_parents_observation_and_follows_it() {
    // 2 lies under the session's 47-point top band, which it does not clear: its 50-point top
    // bar lies past the band, where one pinned to its safe-area guide would, while 4, in flow,
    // is laid out past the bar's depth alone, at 50; the band's change moves the bar, though
    // no box reads an observation or ignores the band
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(4, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(2, &[3, 4])]);
    let mut k = document([47.0, 0.0, 34.0, 0.0], &[(StyleId::Height, "300")], ops);
    assert_eq!(frame(&k, 3), (0.0, 47.0, 390.0, 50.0));
    assert_eq!(frame(&k, 4).1, 50.0);
    assert_eq!(k.arena.observations.rounds, 1, "one round moves the slot");
    assert!(k.set_session_band([59.0, 0.0, 34.0, 0.0]).unwrap());
    lay_out(&mut k);
    assert_eq!((frame(&k, 3).1, frame(&k, 4).1), (59.0, 50.0));
    assert!(k.set_session_band([0.0, 0.0, 34.0, 0.0]).unwrap());
    lay_out(&mut k);
    assert_eq!((frame(&k, 3).1, frame(&k, 4).1), (0.0, 50.0));
}

#[test]
fn a_bars_slot_spans_the_room_its_parents_side_observations_leave() {
    // Under an 84-point left band, 2's top bar 3 spans 84 to 390; 5, an item of the row 4 sized
    // by its content, holds a 9-point box and a 300-point top bar, 7, which lies beside the
    // band, so 5 is 384 wide, as a view sized by its content grows to fit a bar pinned to its
    // safe-area guide
    let mut ops = create(3..=7);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "20")]));
    let row = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "row"),
        (StyleId::AlignItems, "flex-start"),
    ];
    ops.extend([style(4, vec![], &row), style(5, vec![], &[])]);
    let item = [(StyleId::Width, "9"), (StyleId::Height, "10")];
    ops.push(style(6, vec![], &item));
    let wide = [
        bar("top"),
        (StyleId::Width, "300"),
        (StyleId::Height, "20"),
        (StyleId::AlignSelf, "flex-start"),
    ];
    ops.push(style(7, vec![], &wide));
    ops.extend([
        children(2, &[3, 4]),
        children(4, &[5]),
        children(5, &[6, 7]),
    ]);
    let k = document([0.0, 0.0, 0.0, 84.0], &[], ops);
    assert_eq!(frame(&k, 3), (84.0, 0.0, 306.0, 20.0));
    assert_eq!(frame(&k, 5).2, 384.0);
    assert_eq!((frame(&k, 7).0, frame(&k, 7).2), (84.0, 300.0));
}

#[test]
fn a_bar_lies_past_its_parents_observation_with_its_reservations() {
    // 2, under a 47-point top band and inside a 6-point top border that the band covers, adds
    // 10 to its safe area: its bar lies at 57 and 4 at 66, clear of the border, the addition
    // and the bar; taking 20 instead, its bar lies at 27 and 4 at 56
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(4, vec![], &[(StyleId::Height, "10")]));
    ops.extend([children(2, &[3, 4])]);
    let rows = |added: &'static str| {
        [
            (StyleId::Height, "300"),
            (StyleId::BorderWidthTop, "6"),
            (StyleId::BorderStyleTop, "solid"),
            (StyleId::AdditionalSafeAreaTop, added),
        ]
    };
    let mut k = document([47.0, 0.0, 34.0, 0.0], &rows("10"), ops);
    assert_eq!((frame(&k, 3).1, frame(&k, 4).1), (57.0, 66.0));
    k.apply(0, 2, &[style(2, vec![], &rows("-20"))]).unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 3).1, frame(&k, 4).1), (27.0, 56.0));
}

#[test]
fn a_bar_past_its_parents_observation_observes_none_of_it_and_its_descendants_all() {
    // Under a 47-point top band, 3, 2's bar, observes 0 at its top; 4, in flow at 50, and 5,
    // pinned to 2's top, observe the band and the bar, to 97
    let mut ops = create(3..=5);
    let reads = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    ops.push(style(
        3,
        reads.clone(),
        &[bar("top"), (StyleId::Height, "50")],
    ));
    ops.push(style(4, reads.clone(), &[(StyleId::Height, "10")]));
    let pinned = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Top, "0"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(5, reads, &pinned));
    ops.extend([children(2, &[3, 4, 5])]);
    let rows = [
        (StyleId::Height, "300"),
        (StyleId::PositionType, "relative"),
    ];
    let k = document([47.0, 0.0, 34.0, 0.0], &rows, ops);
    assert_eq!(frame(&k, 3).1, 47.0);
    let top = |id| k.observation(id).unwrap()[0];
    assert_eq!(
        [top(3), top(4), top(5)],
        [Some(0.0), Some(47.0), Some(97.0)]
    );
}

#[test]
fn a_parent_whose_slots_do_not_settle_is_unsettled() {
    // 4, sized by its content at the end of the row 2 under an 84-point left band, holds a
    // 307-point top bar: each layout widens it by what it observes, which moves it a point
    // further into the band, so it is still off at the bound; once it holds no bar, nothing
    // is listed, though no box reads an observation or ignores the band
    let mut k = unsettled(vec![]);
    assert_eq!(k.arena.observations.rounds, MAX_ROUNDS);
    assert_eq!(k.safe_area_unsettled(), vec![4]);
    let clear = Op::ClearStyle {
        id: 3,
        mask: crate::StyleMask::of(StyleId::SafeAreaBar),
        written: true,
    };
    k.apply(0, 2, &[clear]).unwrap();
    lay_out(&mut k);
    assert_eq!(k.safe_area_unsettled(), Vec::<ViewId>::new());
}

#[test]
fn a_parent_off_as_a_reader_and_for_its_slots_is_listed_once() {
    // The same 4, padding by what it observes too
    let k = unsettled(vec![(StyleId::PaddingLeft, observed(Edge::Left, None))]);
    assert_eq!(k.safe_area_unsettled(), vec![4]);
}

/// [`a_parent_whose_slots_do_not_settle_is_unsettled`]'s document, 4 reading `reads`.
fn unsettled(reads: Vec<(StyleId, Node)>) -> Kernel {
    let mut ops = create(3..=5);
    let wide = [
        bar("top"),
        (StyleId::Width, "307"),
        (StyleId::Height, "20"),
        (StyleId::AlignSelf, "flex-start"),
    ];
    ops.extend([style(3, vec![], &wide), style(4, reads, &[])]);
    ops.extend([children(2, &[4]), children(4, &[3, 5])]);
    let row = [
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "row"),
        (StyleId::JustifyContent, "flex-end"),
        (StyleId::AlignItems, "flex-start"),
    ];
    document([0.0, 0.0, 0.0, 84.0], &row, ops)
}

#[test]
fn a_parent_sized_by_its_content_holds_its_slot_across_its_edge() {
    // Under a 47-point top band, 4, sized by its content, holds only a 50-point top bar, which
    // lies past the band: 4 is 97 tall, as a view sized by its content grows to hold a bar
    // pinned to its safe-area guide, and 5 follows it
    let mut ops = create(3..=5);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.extend([
        style(4, vec![], &[]),
        style(5, vec![], &[(StyleId::Height, "10")]),
    ]);
    ops.extend([children(2, &[4, 5]), children(4, &[3])]);
    let k = document([47.0, 0.0, 34.0, 0.0], &[], ops);
    assert_eq!(frame(&k, 4), (0.0, 0.0, 390.0, 97.0));
    assert_eq!((frame(&k, 3).1, frame(&k, 5).1), (47.0, 97.0));
}

#[test]
fn a_bottom_bar_lies_above_the_band_and_an_edge_no_slot_reads_takes_no_round() {
    // 2 fills the document over a 34-point bottom band: its 40-point bottom bar lies above
    // the band; a top band's change moves no slot of a bottom bar's edges, so takes no round
    let mut ops = create(3..=3);
    ops.push(style(3, vec![], &[bar("bottom"), (StyleId::Height, "40")]));
    ops.extend([children(2, &[3])]);
    let mut k = document([0.0, 0.0, 34.0, 0.0], &[(StyleId::Height, "844")], ops);
    assert_eq!(frame(&k, 3).1, 844.0 - 34.0 - 40.0);
    k.set_session_band([47.0, 0.0, 34.0, 0.0]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3).1, 844.0 - 34.0 - 40.0);
    assert_eq!(k.arena.observations.rounds, 0);
}

#[test]
fn a_parent_that_becomes_an_owners_root_begins_its_slots_where_its_block_does() {
    // 2, under the session's 47-point band, has its bar past it; given a block, it is an
    // owner's root whose host has reported no band, so its bar lies at its content root's edge
    let mut ops = create(3..=3);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.extend([children(2, &[3])]);
    let mut k = document([47.0, 0.0, 34.0, 0.0], &[(StyleId::Height, "300")], ops);
    assert_eq!(frame(&k, 3).1, 47.0);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(300.0));
    k.set_host_block(2, Some(block)).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3).1, 0.0);
}

#[test]
fn a_band_change_on_an_edge_a_root_takes_owes_a_layout_for_its_bars() {
    // The root takes its top band and reads no observation: a deeper band moves no content
    // root, but its bar, past the band, follows it
    let mut ops = create(3..=3);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = root(&[(StyleId::SafeAreaIgnore, "container top")], ops);
    assert_eq!(frame(&k, 3).1, 47.0);
    assert!(k
        .set_owner_band(2, Some([59.0, 0.0, 34.0, 0.0]), &[])
        .unwrap());
    lay_out(&mut k);
    assert_eq!(frame(&k, 3).1, 59.0);
}

#[test]
fn a_parent_with_a_ratio_under_no_band_keeps_its_content_floor() {
    // 3, 200 wide at a ratio of 1, holds a 50-point top bar and a 300-point child in a
    // content root that clears the band: the ratio's height is a floor, so 3 is 350 tall
    let mut ops = create(3..=5);
    let ratio = [(StyleId::Width, "200"), (StyleId::AspectRatio, "1")];
    ops.push(style(3, vec![], &ratio));
    ops.push(style(4, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "300")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    let k = root(&[], ops);
    assert_eq!(frame(&k, 3).3, 350.0);
}

#[test]
fn a_border_on_an_edge_a_root_takes_lies_within_the_band() {
    // The root takes its top band and has a 6-point top border, which lies within the band:
    // its bar lies past the band at 47, as a box's under the band does, not past the border,
    // and 4, laid out past the border and the bar at 56, observes them to the bar's inner side
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[bar("top"), (StyleId::Height, "50")]));
    let reads = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    ops.push(style(4, reads, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let rows = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::BorderWidthTop, "6"),
        (StyleId::BorderStyleTop, "solid"),
    ];
    let k = root(&rows, ops);
    assert_eq!((frame(&k, 3).1, frame(&k, 4).1), (47.0, 56.0));
    assert_eq!(k.observation(4).unwrap()[0], Some(41.0));
}

#[test]
fn a_parent_with_a_ratio_and_hairline_borders_keeps_its_content_floor() {
    // The same 3 with a third of a point of border above and below, which do not add exactly:
    // its floor stands, so it holds the bar and the 300 points of content
    let mut ops = create(3..=5);
    let ratio = [
        (StyleId::Width, "200"),
        (StyleId::AspectRatio, "1"),
        (StyleId::BorderWidthTop, "0.33333334"),
        (StyleId::BorderStyleTop, "solid"),
        (StyleId::BorderWidthBottom, "0.33333334"),
        (StyleId::BorderStyleBottom, "solid"),
    ];
    ops.push(style(3, vec![], &ratio));
    ops.push(style(4, vec![], &[bar("top"), (StyleId::Height, "50")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "300")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    let k = root(&[], ops);
    assert!(frame(&k, 3).3 > 350.0, "{:?}", frame(&k, 3));
}

// A scroller beside bars takes them in D14's `reserved` term (D12)
#[path = "scroll_terms_tests.rs"]
mod scroll_terms;
