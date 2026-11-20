//! A scroller's terms for its host's writer (LLP 1123 D13, D14): its reach on each edge, and
//! from the reached edge the depth of what Exact reserves there and of the keyboard it takes.
use super::*;
use crate::kernel::{Keyboard, KeyboardPolicy, ScrollTerms};

/// A box's style rows, as `style` takes them.
type Rows = Vec<(StyleId, &'static str)>;

/// The owner root 2, 390 by 844 under `band`, which it clears, a flex column filling its
/// content root with `root`'s rows, holding `held`, each `(id, rows)`, and `nested`, each
/// `(parent, id, rows)` under a box `held` makes; `scroll_views` are made as scroll views by
/// their type. Its keyboard is `keyboard`, `(policy, top)`. Frames are in the owner's space.
fn owner_with(
    band: Option<[f32; 4]>,
    root: &[(StyleId, &'static str)],
    held: &[(ViewId, Rows)],
    nested: &[(ViewId, ViewId, Rows)],
    scroll_views: &[ViewId],
    keyboard: Option<(KeyboardPolicy, f32)>,
) -> Kernel {
    let mut own = vec![
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::Height, "100%"),
    ];
    own.extend_from_slice(root);
    let mut ops: Vec<Op> = (3..=9)
        .map(|id| match scroll_views.contains(&id) {
            true => Op::CreateView {
                id,
                node_type: NodeType::ScrollView,
            },
            false => create(id..=id).remove(0),
        })
        .collect();
    ops.push(style(2, vec![], &own));
    for (id, rows) in held {
        ops.push(style(*id, vec![], rows));
    }
    for (_, id, rows) in nested {
        ops.push(style(*id, vec![], rows));
    }
    ops.push(children(1, &[2]));
    ops.push(children(
        2,
        &held.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
    ));
    assert!(
        nested
            .iter()
            .all(|(parent, _, _)| held.iter().any(|(id, _)| id == parent)),
        "a nested box's parent is held"
    );
    for (parent, _) in held {
        let under: Vec<ViewId> = nested
            .iter()
            .filter(|(p, _, _)| p == parent)
            .map(|(_, id, _)| *id)
            .collect();
        if !under.is_empty() {
            ops.push(children(*parent, &under));
        }
    }
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, band, &[]).unwrap();
    if let Some((policy, top)) = keyboard {
        k.set_keyboard_policy(Some(policy));
        let frame = [0.0, top, 390.0, 844.0 - top];
        k.set_owner_keyboard(
            2,
            Some(Keyboard {
                frame,
                docked: true,
            }),
        )
        .unwrap();
    }
    lay_out(&mut k);
    k
}

/// [`owner_with`] for boxes the root holds alone.
fn holding(held: &[(ViewId, Rows)], keyboard: Option<(KeyboardPolicy, f32)>) -> Kernel {
    owner_with(Some(BAND), &[], held, &[], &[], keyboard)
}

const BAND: [f32; 4] = [47.0, 0.0, 34.0, 0.0];

/// The 18 Pro's keyboard, 328 points deep: its top 516 points down.
const UP: f32 = 516.0;

fn scroller(rows: &[(StyleId, &'static str)]) -> Vec<(StyleId, &'static str)> {
    let mut all = vec![(StyleId::OverflowY, "scroll"), (StyleId::FlexGrow, "1")];
    all.extend_from_slice(rows);
    all
}

/// A flex column growing to fill what its siblings leave, with `rows`.
fn column(rows: &[(StyleId, &'static str)]) -> Vec<(StyleId, &'static str)> {
    let mut all = vec![
        (StyleId::FlexGrow, "1"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    all.extend_from_slice(rows);
    all
}

/// `-exact-safe-area-reach` naming `edges`, as Contract lowers it: `edges` and the edges.
fn reach(edges: &'static str) -> [(StyleId, &'static str); 2] {
    [
        (StyleId::SafeAreaReach, "edges"),
        (StyleId::SafeAreaReachEdges, edges),
    ]
}

fn bar(edge: &'static str, depth: &'static str) -> Vec<(StyleId, &'static str)> {
    vec![(StyleId::SafeAreaBar, edge), (StyleId::Height, depth)]
}

fn terms(k: &Kernel, id: ViewId) -> ScrollTerms {
    k.scroll_terms(k.arena.key(slot(k, id))).unwrap()
}

#[test]
fn a_scroller_on_the_band_reaches_it_and_nothing_is_reserved() {
    // 4 fills the content root, 47 to 810: it reaches the band's 47 and 34, and with nothing
    // reserved its own native band is the whole container term
    let k = holding(&[(4, scroller(&[]))], None);
    assert_eq!(frame(&k, 4), (0.0, 47.0, 390.0, 763.0));
    let t = terms(&k, 4);
    assert_eq!(t.reach, [47.0, 0.0, 34.0, 0.0]);
    assert_eq!(
        (t.reserved, t.keyboard, t.container),
        ([None; 4], [0.0; 4], [true; 4])
    );
}

#[test]
fn a_scroller_between_bars_reaches_under_them_and_reserves_their_far_sides() {
    // A 50-point top bar and a 40-point bottom one: 4 runs from 97 to 770, reaches both edges
    // and reserves 47 + 50 and 34 + 40, as D14's 84 for a 50-point bar on a 34-point band
    let k = holding(
        &[
            (3, bar("top", "50")),
            (4, scroller(&[])),
            (5, bar("bottom", "40")),
        ],
        None,
    );
    assert_eq!(frame(&k, 4), (0.0, 97.0, 390.0, 673.0));
    let t = terms(&k, 4);
    assert_eq!(t.reach, [97.0, 0.0, 74.0, 0.0]);
    assert_eq!(t.reserved, [Some(97.0), None, Some(74.0), None]);
}

#[test]
fn reach_follows_the_edges_it_names_and_the_terms_start_from_the_reached_edge() {
    // `top` alone: the bottom stays at the bar's inner side, from which nothing is reserved;
    // `none`: neither edge reaches, and each bar's far side is the scroller's own edge
    let rows = |edges| {
        [
            (3, bar("top", "50")),
            (4, scroller(&reach(edges))),
            (5, bar("bottom", "40")),
        ]
    };
    let top = terms(&holding(&rows("top"), None), 4);
    assert_eq!(
        (top.reach, top.reserved),
        ([97.0, 0.0, 0.0, 0.0], [Some(97.0), None, Some(0.0), None])
    );
    let none = terms(&holding(&rows("none"), None), 4);
    assert_eq!(
        (none.reach, none.reserved),
        ([0.0; 4], [Some(0.0), None, Some(0.0), None])
    );
}

#[test]
fn start_and_end_follow_the_scrollers_direction() {
    // A 30-point left band and a 20-point right one: `start` is the left in `ltr` and the right
    // in `rtl`, `end` the other
    let band = [47.0, 20.0, 34.0, 30.0];
    let with = |edge, direction| {
        let [kind, edges] = reach(edge);
        let k = owner_with(
            Some(band),
            &[],
            &[(4, scroller(&[kind, edges, (StyleId::Direction, direction)]))],
            &[],
            &[],
            None,
        );
        terms(&k, 4).reach
    };
    assert_eq!(with("start", "ltr"), [0.0, 0.0, 0.0, 30.0]);
    assert_eq!(with("start", "rtl"), [0.0, 20.0, 0.0, 0.0]);
    assert_eq!(with("end", "ltr"), [0.0, 20.0, 0.0, 0.0]);
}

#[test]
fn a_scroller_clear_of_the_band_does_not_reach_it() {
    // 3, 100 tall, puts 4's top 100 below the band's inner side
    let t = terms(
        &holding(
            &[(3, vec![(StyleId::Height, "100")]), (4, scroller(&[]))],
            None,
        ),
        4,
    );
    assert_eq!(t.reach, [0.0, 0.0, 34.0, 0.0]);
}

#[test]
fn reach_goes_to_a_reserving_ancestors_padding_edge_clear_of_the_band() {
    // 6, 200 below the band, adds 20 on its top: 4, laid out clear of it at 267, reaches 6's
    // padding edge, 20 up, not the allocation's edge, and reserves the 20
    let rows = vec![
        (StyleId::AdditionalSafeAreaTop, "20"),
        (StyleId::Height, "300"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    let k = owner_with(
        Some(BAND),
        &[],
        &[(3, vec![(StyleId::Height, "200")]), (6, rows)],
        &[(6, 4, scroller(&[]))],
        &[],
        None,
    );
    assert_eq!(frame(&k, 4).1, 267.0);
    let t = terms(&k, 4);
    assert_eq!((t.reach[0], t.reserved[0]), (20.0, Some(20.0)));
}

#[test]
fn a_negative_additional_inset_inside_the_root_is_the_reserved_term() {
    // 6 takes the band on its top and adds −20 there: 4 observes 27, which replaces the native
    // band's 47, which no controller's insets reduce
    let rows = column(&[
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::AdditionalSafeAreaTop, "-20"),
    ]);
    let k = owner_with(
        Some(BAND),
        &[],
        &[(6, rows)],
        &[(6, 4, scroller(&[]))],
        &[],
        None,
    );
    let t = terms(&k, 4);
    assert_eq!(t.reserved[0], Some(27.0));
    assert_eq!((frame(&k, 4).1, t.reach[0]), (0.0, 0.0));
}

#[test]
fn an_ancestor_that_clips_transforms_or_has_a_border_stops_reach_at_its_padding_edge() {
    // 6 fills the content root and holds 4: clipping on both axes or translated, it keeps 4
    // from reaching either band; neither, it does not
    for (rows, reach) in [
        (vec![], [47.0, 0.0, 34.0, 0.0]),
        (
            vec![
                (StyleId::OverflowX, "hidden"),
                (StyleId::OverflowY, "hidden"),
            ],
            [0.0; 4],
        ),
        (vec![(StyleId::Translate, "0px 1px")], [0.0; 4]),
    ] {
        let k = owner_with(
            Some(BAND),
            &[],
            &[(6, column(&rows))],
            &[(6, 4, scroller(&[]))],
            &[],
            None,
        );
        assert_eq!(terms(&k, 4).reach, reach, "{rows:?}");
    }
    // 6 takes the band on its top, with a 5-point top border and 42 of padding, so 4 lies at the
    // band's inner side: 4 reaches up to 6's padding edge, 42, not to the allocation's edge, 47
    let rows = column(&[
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::BorderWidthTop, "5"),
        (StyleId::BorderStyleTop, "solid"),
        (StyleId::PaddingTop, "42"),
    ]);
    let k = owner_with(
        Some(BAND),
        &[],
        &[(6, rows)],
        &[(6, 4, scroller(&[]))],
        &[],
        None,
    );
    assert_eq!(frame(&k, 4).1, 47.0);
    assert_eq!(terms(&k, 4).reach[0], 42.0);
}

#[test]
fn a_scroll_view_by_its_type_stops_reach_at_its_scrollport() {
    // 6 is a scroll view with no overflow row, passing the band on its top while aligned to its
    // bottom, so it stays at 47: 4 at its top touches the band but stays inside 6's scrollport
    let rows = column(&[
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::SafeAreaIgnoreAlign, "bottom"),
    ]);
    let k = owner_with(
        Some(BAND),
        &[],
        &[(6, rows)],
        &[(
            6,
            4,
            scroller(&[(StyleId::Height, "100"), (StyleId::FlexGrow, "0")]),
        )],
        &[6],
        None,
    );
    assert_eq!((frame(&k, 6).1, frame(&k, 4).1), (47.0, 47.0));
    assert_eq!(terms(&k, 4).reach[0], 0.0);
}

#[test]
fn an_owners_root_that_scrolls_reaches_past_its_own_additional_insets() {
    // The root is the scroller: it reaches the band, and past 60 more of its own on the bottom,
    // as a controller's view lies under its own additional insets
    let rows = |extra: &[(StyleId, &'static str)]| {
        let mut all = vec![(StyleId::OverflowY, "scroll")];
        all.extend_from_slice(extra);
        all
    };
    let plain = terms(&owner_with(Some(BAND), &rows(&[]), &[], &[], &[], None), 2);
    assert_eq!(
        (plain.reach, plain.reserved),
        ([47.0, 0.0, 34.0, 0.0], [None; 4])
    );
    let inset = terms(
        &owner_with(
            Some(BAND),
            &rows(&[(StyleId::AdditionalSafeAreaBottom, "60")]),
            &[],
            &[],
            &[],
            None,
        ),
        2,
    );
    assert_eq!((inset.reach[2], inset.reserved[2]), (94.0, Some(94.0)));
}

#[test]
fn under_resizes_visual_a_vertical_scroller_takes_the_keyboard_from_its_reached_edge() {
    // Layout keeps its size, and 4 above a 40-point bottom bar reaches the allocation's bottom,
    // so it takes the keyboard's whole 328 there; a horizontal one, or one that ignores the
    // keyboard region, takes none
    let rows = |extra| [(4, scroller(extra)), (5, bar("bottom", "40"))];
    let visual = Some((KeyboardPolicy::ResizesVisual, UP));
    let t = terms(&holding(&rows(&[]), visual), 4);
    assert_eq!(
        (t.reach[2], t.reserved[2], t.keyboard[2]),
        (74.0, Some(74.0), 328.0)
    );
    let sideways = [
        (StyleId::OverflowY, "hidden"),
        (StyleId::OverflowX, "scroll"),
    ];
    assert_eq!(
        terms(&holding(&rows(&sideways), visual), 4).keyboard,
        [0.0; 4]
    );
    let ignores = [(StyleId::SafeAreaIgnore, "keyboard")];
    let t = terms(&holding(&rows(&ignores), visual), 4);
    assert_eq!((t.keyboard, t.container), ([0.0; 4], [true; 4]));
}

#[test]
fn a_scroller_inside_one_that_takes_the_keyboard_takes_none_of_it() {
    // 4 takes the keyboard; 6, a scroller inside it, reaches nothing and takes no keyboard
    let k = owner_with(
        Some(BAND),
        &[],
        &[(
            4,
            scroller(&[
                (StyleId::Display, "flex"),
                (StyleId::FlexDirection, "column"),
            ]),
        )],
        &[(
            4,
            6,
            scroller(&[(StyleId::Height, "100"), (StyleId::FlexGrow, "0")]),
        )],
        &[],
        Some((KeyboardPolicy::ResizesVisual, UP)),
    );
    assert_eq!(terms(&k, 4).keyboard[2], 328.0);
    let inner = terms(&k, 6);
    assert_eq!(
        (inner.reach, inner.reserved, inner.keyboard),
        ([0.0; 4], [None; 4], [0.0; 4])
    );
}

#[test]
fn under_resizes_content_reach_stops_at_the_keyboards_top() {
    // The content root ends at the keyboard's top, 516: the bottom bar rests on it, 476 to
    // 516, and 4 reaches under the bar alone, holding none of the keyboard, nor taking it
    let k = holding(
        &[(4, scroller(&[])), (5, bar("bottom", "40"))],
        Some((KeyboardPolicy::ResizesContent, UP)),
    );
    assert_eq!(frame(&k, 5).1 + 40.0, UP);
    let t = terms(&k, 4);
    assert_eq!(
        (t.reach[2], t.reserved[2], t.keyboard[2]),
        (40.0, Some(40.0), 0.0)
    );
}

#[test]
fn a_scroller_that_ignores_the_container_region_on_one_edge_observes_it_on_the_others() {
    let t = terms(
        &holding(
            &[(4, scroller(&[(StyleId::SafeAreaIgnore, "container top")]))],
            None,
        ),
        4,
    );
    assert_eq!(t.container, [false, true, true, true]);
}

#[test]
fn with_no_band_a_scroller_reaches_nothing() {
    let t = terms(
        &owner_with(None, &[], &[(4, scroller(&[]))], &[], &[], None),
        4,
    );
    assert_eq!(
        (t.reach, t.reserved, t.container),
        ([0.0; 4], [None; 4], [true; 4])
    );
}

#[test]
fn only_a_laid_out_scroller_has_terms() {
    // 3 is no scroller, and 7, styled as one, was never attached
    let mut k = holding(
        &[(3, vec![(StyleId::Height, "10")]), (4, scroller(&[]))],
        None,
    );
    k.apply(0, 2, &[style(7, vec![], &scroller(&[]))]).unwrap();
    lay_out(&mut k);
    assert_eq!(k.scroll_terms(k.arena.key(slot(&k, 3))), None);
    assert!(k.scroll_terms(k.arena.key(slot(&k, 4))).is_some());
    assert_eq!(k.scroll_terms(k.arena.key(slot(&k, 7))), None);
}

#[test]
fn an_owners_root_that_scrolls_holds_its_padding_on_the_keyboard() {
    // Under `resizes-content` the root's 10 of safe-area padding rides on the keyboard's top: the
    // root reaches and reserves that 10 alone, and takes no keyboard
    let rows = [
        (StyleId::OverflowY, "scroll"),
        (StyleId::SafeAreaPaddingBottom, "10"),
    ];
    let k = owner_with(
        Some(BAND),
        &rows,
        &[],
        &[],
        &[],
        Some((KeyboardPolicy::ResizesContent, UP)),
    );
    let t = terms(&k, 2);
    assert_eq!(
        (t.reach[2], t.reserved[2], t.keyboard[2]),
        (10.0, Some(10.0), 0.0)
    );
}

#[test]
fn an_ancestor_clipping_one_axis_stops_reach_on_that_axis_alone() {
    // Under side bands of 20 and 30, a filling ancestor that clips only across keeps 4 from the
    // side bands and lets it reach the top and bottom
    let band = [47.0, 20.0, 34.0, 30.0];
    let k = owner_with(
        Some(band),
        &[],
        &[(6, column(&[(StyleId::OverflowX, "clip")]))],
        &[(6, 4, scroller(&[]))],
        &[],
        None,
    );
    assert_eq!(terms(&k, 4).reach, [47.0, 0.0, 34.0, 0.0]);
}

#[test]
fn an_owners_root_that_clips_stops_its_scrollers_reach() {
    let root = [
        (StyleId::OverflowX, "hidden"),
        (StyleId::OverflowY, "hidden"),
    ];
    let k = owner_with(Some(BAND), &root, &[(4, scroller(&[]))], &[], &[], None);
    assert_eq!(terms(&k, 4).reach, [0.0; 4]);
}

#[test]
fn an_owners_root_that_scrolls_and_takes_its_bottom_keeps_its_own_inset_there() {
    // The root takes the container region on its bottom and adds 60 there: it reaches and
    // reserves the 60 and observes it, as D4 applies the root's reservations after its ignore,
    // so its content stays clear of its own inset
    let rows = [
        (StyleId::OverflowY, "scroll"),
        (StyleId::SafeAreaIgnore, "container bottom"),
        (StyleId::AdditionalSafeAreaBottom, "60"),
    ];
    let t = terms(&owner_with(Some(BAND), &rows, &[], &[], &[], None), 2);
    assert_eq!(
        (t.reach[2], t.reserved[2], t.container[2]),
        (60.0, Some(60.0), true)
    );
}
