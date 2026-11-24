//! An `edge-effect` bar's registration (LLP 1123 D12): its physical edge and the first scroller
//! in tree order beneath its slot, which keeps its own terms, as do the others beneath it.
use super::*;
use crate::kernel::BarEdgeEffect;
use crate::Edge;

/// `bar` with `edge-effect`.
fn effect_bar(edge: &'static str, depth: &'static str) -> Vec<(StyleId, &'static str)> {
    vec![(StyleId::SafeAreaBar, edge), (StyleId::Height, depth)]
}

fn effect(k: &Kernel, id: ViewId) -> Option<BarEdgeEffect> {
    k.bar_edge_effect(k.arena.key(slot(k, id)))
}

fn registered(k: &Kernel, id: ViewId) -> Option<ViewId> {
    let e = effect(k, id).expect("an edge-effect bar");
    e.scroller
        .map(|s| k.arena.local_id(k.arena.resolve(s).unwrap()))
}

/// A 50-point top bar over a row of two scrollers, 5 twice as wide as 6, in `order`.
fn panes(order: &[ViewId]) -> Kernel {
    let row = column(&[(StyleId::FlexDirection, "row")]);
    let pane = |grow| scroller(&[(StyleId::FlexGrow, grow)]);
    let nested: Vec<(ViewId, ViewId, Rows)> = order
        .iter()
        .map(|&id| (4, id, pane(if id == 5 { "2" } else { "1" })))
        .collect();
    owner_with(
        Some(BAND),
        &[],
        &[(3, effect_bar("top edge-effect", "50")), (4, row)],
        &nested,
        &[],
        None,
    )
}

#[test]
fn a_bar_registers_with_the_first_scroller_beneath_it_and_each_keeps_its_terms() {
    // Both panes reach under the bar from 97 to the band's 47; 5 comes first, and both reserve
    let k = panes(&[5, 6]);
    assert_eq!(effect(&k, 3).map(|e| e.edge), Some(Edge::Top));
    assert_eq!(registered(&k, 3), Some(5));
    for id in [5, 6] {
        let t = terms(&k, id);
        assert_eq!((t.reach[0], t.reserved[0]), (97.0, Some(97.0)), "{id}");
    }
    // Tree order, not width, picks it
    assert_eq!(registered(&panes(&[6, 5]), 3), Some(6));
}

#[test]
fn a_bar_without_edge_effect_or_a_box_that_is_no_bar_has_none() {
    let k = holding(&[(3, bar("top", "50")), (4, scroller(&[]))], None);
    assert_eq!(effect(&k, 3), None);
    assert_eq!(effect(&k, 4), None, "a scroller is no bar");
}

#[test]
fn a_scroller_that_does_not_reach_under_the_slot_is_not_beneath_it() {
    // Reaching no edge, the scroller's top stays at the bar's inner side; below a filler it
    // touches no bar at all
    let none = holding(
        &[
            (3, effect_bar("top edge-effect", "50")),
            (4, scroller(&reach("none"))),
        ],
        None,
    );
    assert_eq!(terms(&none, 4).reserved[0], Some(0.0));
    assert_eq!(registered(&none, 3), None);
    let filler = vec![(StyleId::Height, "40")];
    let clear = holding(
        &[
            (3, effect_bar("top edge-effect", "50")),
            (4, filler),
            (5, scroller(&[])),
        ],
        None,
    );
    assert_eq!(registered(&clear, 3), None);
}

#[test]
fn each_stacked_bar_registers_with_the_scroller_beneath_its_own_slot() {
    let k = holding(
        &[
            (3, effect_bar("top edge-effect", "30")),
            (4, effect_bar("top edge-effect", "20")),
            (5, scroller(&[])),
            (6, effect_bar("bottom edge-effect", "40")),
        ],
        None,
    );
    assert_eq!(registered(&k, 3), Some(5));
    assert_eq!(registered(&k, 4), Some(5));
    assert_eq!(registered(&k, 6), Some(5));
    assert_eq!(effect(&k, 6).map(|e| e.edge), Some(Edge::Bottom));
}

#[test]
fn a_bars_start_and_end_are_physical_edges_by_its_parents_direction() {
    // A 60-point bar on the right in `rtl`, `start` there, over a scroller filling the row
    let side = vec![
        (StyleId::SafeAreaBar, "start edge-effect"),
        (StyleId::Width, "60"),
    ];
    let k = owner_with(
        Some(BAND),
        &[(StyleId::Direction, "rtl"), (StyleId::FlexDirection, "row")],
        &[(3, side), (4, scroller(&[]))],
        &[],
        &[],
        None,
    );
    assert_eq!(effect(&k, 3).map(|e| e.edge), Some(Edge::Right));
    assert_eq!(registered(&k, 3), Some(4));
}

#[test]
fn a_bar_with_no_band_registers_with_no_scroller() {
    // While the band is unavailable no scroller reaches or has a `reserved` term
    let k = owner_with(
        None,
        &[],
        &[(3, effect_bar("top edge-effect", "50")), (4, scroller(&[]))],
        &[],
        &[],
        None,
    );
    assert_eq!(effect(&k, 3).map(|e| e.scroller), Some(None));
}

#[test]
fn an_outer_scroller_beneath_the_bar_takes_it_from_the_one_inside() {
    // 4 takes the top: it is first in tree order, and the scroller inside it lies in no area
    // the bar reserves
    let k = owner_with(
        Some(BAND),
        &[],
        &[(3, effect_bar("top edge-effect", "50")), (4, scroller(&[]))],
        &[(
            4,
            5,
            vec![(StyleId::OverflowY, "scroll"), (StyleId::Height, "200")],
        )],
        &[],
        None,
    );
    assert_eq!(registered(&k, 3), Some(4));
}

#[test]
fn a_scroller_beside_the_slot_along_its_edge_is_not_beneath_it() {
    // A 100-point left bar narrows the top bar's slot to 100 to 390; a 100-point scroller pulled
    // under the left bar by its margin reaches under the top bar's depth, but beside its slot
    let left = vec![(StyleId::SafeAreaBar, "left"), (StyleId::Width, "100")];
    let row = column(&[(StyleId::FlexDirection, "row")]);
    let beside = vec![
        (StyleId::OverflowY, "scroll"),
        (StyleId::Width, "100"),
        (StyleId::MarginLeft, "-100"),
    ];
    let k = owner_with(
        Some(BAND),
        &[],
        &[
            (3, left),
            (4, effect_bar("top edge-effect", "50")),
            (5, row),
        ],
        &[(5, 6, beside)],
        &[],
        None,
    );
    assert_eq!(frame(&k, 6).0, 0.0, "{:?}", frame(&k, 6));
    assert_eq!(
        terms(&k, 6).reach[0],
        97.0,
        "it reaches under the top bar's depth"
    );
    assert_eq!(registered(&k, 4), None);
}

#[test]
fn a_scroller_in_the_bar_itself_is_not_beneath_it() {
    // A row of chips scrolling sideways inside the bar comes first in tree order, and is the bar's
    let chips = vec![(StyleId::OverflowX, "scroll"), (StyleId::Height, "40")];
    let k = owner_with(
        Some(BAND),
        &[],
        &[(3, effect_bar("top edge-effect", "50")), (4, scroller(&[]))],
        &[(3, 5, chips)],
        &[],
        None,
    );
    assert_eq!(registered(&k, 3), Some(4));
}

#[test]
fn a_scroller_whose_area_starts_apart_from_the_bars_is_not_beneath_them() {
    // 6 starts 40 below the bar's inner side and reserves 20 of its own: its area starts at its
    // padding edge, apart from the bar's, so the scroller its margin pulls up into the bar's
    // slot reserves only 6's
    let rows = vec![
        (StyleId::AdditionalSafeAreaTop, "20"),
        (StyleId::Height, "300"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let pulled = vec![
        (StyleId::OverflowY, "scroll"),
        (StyleId::Height, "200"),
        (StyleId::MarginTop, "-97"),
    ];
    let k = owner_with(
        Some(BAND),
        &[],
        &[
            (3, effect_bar("top edge-effect", "50")),
            (5, vec![(StyleId::Height, "40")]),
            (6, rows),
        ],
        &[(6, 4, pulled)],
        &[],
        None,
    );
    assert_eq!(frame(&k, 4).1, 60.0, "{:?}", frame(&k, 4));
    assert!(terms(&k, 4).reserved[0].is_some());
    assert_eq!(registered(&k, 3), None);
}

#[test]
fn a_scroller_reaching_into_the_inner_of_two_slots_is_beneath_that_one_alone() {
    // Reaching no edge, its margin puts its top at 87: past the inner bar's side at 97, short
    // of the outer one's at 77
    let into = scroller(&[
        reach("none")[0],
        reach("none")[1],
        (StyleId::MarginTop, "-10"),
    ]);
    let k = holding(
        &[
            (3, effect_bar("top edge-effect", "30")),
            (4, effect_bar("top edge-effect", "20")),
            (5, into),
        ],
        None,
    );
    assert_eq!(frame(&k, 5).1, 87.0, "{:?}", frame(&k, 5));
    assert_eq!(registered(&k, 4), Some(5));
    assert_eq!(registered(&k, 3), None);
}

#[test]
fn an_outer_scroller_beneath_the_bar_or_taking_its_area_keeps_it_from_the_one_inside() {
    // Reaching no edge, 4 lies short of the slot and takes the area from 5 inside it; ignoring
    // the container's top it extends under the bar and is beneath it
    let inner = vec![(StyleId::OverflowY, "scroll"), (StyleId::Height, "200")];
    let with = |outer: Vec<(StyleId, &'static str)>| {
        owner_with(
            Some(BAND),
            &[],
            &[(3, effect_bar("top edge-effect", "50")), (4, outer)],
            &[(4, 5, inner.clone())],
            &[],
            None,
        )
    };
    let short = with(scroller(&reach("none")));
    assert_eq!(registered(&short, 3), None);
    let under = with(scroller(&[(StyleId::SafeAreaIgnore, "container top")]));
    assert_eq!(registered(&under, 3), Some(4));
    // Ignoring the container's top at its laid-out size, 4 lies short of the slot and passes the
    // area to 5, whose reach its clip stops at its padding edge, short of the slot too
    let passing = with(scroller(&[
        reach("none")[0],
        reach("none")[1],
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::SafeAreaIgnoreAlign, "bottom"),
    ]));
    assert_eq!(frame(&passing, 4).1, 97.0, "{:?}", frame(&passing, 4));
    assert_eq!(terms(&passing, 5).reach[0], 0.0);
    assert_eq!(registered(&passing, 3), None);
}

#[test]
fn a_slot_with_no_depth_or_one_a_nearer_reservation_cancels_has_no_scroller_beneath() {
    // 4's 20 points less its 20-point margin toward the content leave its slot no depth, past
    // 3's 30; the scroller beneath both is beneath 3 alone
    let none = vec![
        (StyleId::SafeAreaBar, "top edge-effect"),
        (StyleId::Height, "20"),
        (StyleId::MarginBottom, "-20"),
    ];
    let k = holding(
        &[
            (3, effect_bar("top edge-effect", "30")),
            (4, none),
            (5, scroller(&[])),
        ],
        None,
    );
    let depth = |id| {
        k.arena
            .observations
            .bars
            .slots
            .get(&slot(&k, id))
            .map(|s| s.depth)
    };
    assert_eq!((depth(3), depth(4)), (Some(30.0), Some(0.0)));
    assert_eq!(registered(&k, 3), Some(5));
    assert_eq!(registered(&k, 4), None);
    // A column that takes the top area, extending under the 47-to-97 slot, adds a -50-point inset,
    // which ends its area at 47: the scroller at its top lies past no slot that reserves there
    let wrapper = column(&[
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::AdditionalSafeAreaTop, "-50"),
    ]);
    let cancelled = owner_with(
        Some(BAND),
        &[],
        &[(3, effect_bar("top edge-effect", "50")), (4, wrapper)],
        &[(4, 5, scroller(&reach("none")))],
        &[],
        None,
    );
    assert_eq!(frame(&cancelled, 5).1, 0.0, "{:?}", frame(&cancelled, 5));
    assert_eq!(terms(&cancelled, 5).reserved[0], Some(47.0));
    assert_eq!(registered(&cancelled, 3), None);
}

#[test]
fn a_slot_that_widens_with_its_parent_is_read_as_it_lies_now() {
    // A 100-point bar at its slot's start over a row whose scroller lies past 390: the slot
    // reaches it once the owner widens to 500, the bar's own box unchanged
    let start = vec![
        (StyleId::SafeAreaBar, "top edge-effect"),
        (StyleId::Height, "50"),
        (StyleId::Width, "100"),
        (StyleId::AlignSelf, "flex-start"),
    ];
    let row = column(&[(StyleId::FlexDirection, "row")]);
    let fixed = |w| vec![(StyleId::Width, w), (StyleId::FlexShrink, "0")];
    let mut pane = scroller(&[]);
    pane.extend(fixed("100"));
    let mut k = owner_with(
        Some(BAND),
        &[],
        &[(3, start), (4, row)],
        &[(4, 5, fixed("390")), (4, 6, pane)],
        &[],
        None,
    );
    assert_eq!(frame(&k, 6).0, 390.0, "{:?}", frame(&k, 6));
    assert_eq!(registered(&k, 3), None);
    let bar = frame(&k, 3);
    k.set_host_block(2, Some(block(BlockAxis::Is(500.0), BlockAxis::Is(844.0))))
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3), bar, "the bar's own box is unchanged");
    assert_eq!(registered(&k, 3), Some(6));
}

#[test]
fn a_scroller_under_a_transformed_box_does_not_reach_under_the_bar() {
    // The translated column stops reach at its padding edge, the bar's inner side
    let moved = column(&[(StyleId::Translate, "0px 1px")]);
    let k = owner_with(
        Some(BAND),
        &[],
        &[(3, effect_bar("top edge-effect", "50")), (4, moved)],
        &[(4, 5, scroller(&[]))],
        &[],
        None,
    );
    assert_eq!(terms(&k, 5).reach[0], 0.0);
    assert_eq!(registered(&k, 3), None);
}

#[test]
fn a_bar_that_stops_being_one_has_no_edge_effect() {
    let mut k = holding(
        &[(3, effect_bar("top edge-effect", "50")), (4, scroller(&[]))],
        None,
    );
    assert_eq!(registered(&k, 3), Some(4));
    k.apply(0, 2, &[style(3, vec![], &[(StyleId::SafeAreaBar, "none")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(effect(&k, 3), None);
}

#[test]
fn a_bars_end_is_the_right_edge_left_to_right() {
    let side = vec![
        (StyleId::SafeAreaBar, "end edge-effect"),
        (StyleId::Width, "60"),
    ];
    let k = owner_with(
        Some(BAND),
        &[(StyleId::FlexDirection, "row")],
        &[(3, side), (4, scroller(&[]))],
        &[],
        &[],
        None,
    );
    assert_eq!(effect(&k, 3).map(|e| e.edge), Some(Edge::Right));
    assert_eq!(registered(&k, 3), Some(4));
}
