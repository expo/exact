//! The keyboard (LLP 1123 D7): what an owner's root clears of it (D3), what
//! a box inside the root takes of its layer (D4, D9), and what a box reads
//! of it, with the values UIKit's guides give, measured on iOS 27.0 and 27.1
//! (§"Evidence: measured on expo/exact").
use super::*;
use crate::kernel::{Keyboard, KeyboardPolicy};

/// A docked keyboard whose top lies `top` points down an 844-point
/// allocation, across its 390-point width.
fn docked(top: f32) -> Option<Keyboard> {
    Some(Keyboard {
        frame: [0.0, top, 390.0, 844.0 - top],
        docked: true,
    })
}

/// The 18 Pro's keyboard, 328 points deep: its top 516 points down.
const UP: f32 = 516.0;

/// The owner 2 under a 34-point bottom band, its root clear of the band and
/// filling its content root as a column with `root`'s rows: 3 grows, then 4
/// with `rows`, 100 tall, holding 5 at its full height, `border-box`, which
/// pads its bottom by its own observation. Its keyboard is `keyboard` under
/// `resizes-content`.
fn column(
    root: &[(StyleId, &str)],
    rows: &[(StyleId, &str)],
    keyboard: Option<Keyboard>,
) -> Kernel {
    let mut own = vec![
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
        (StyleId::Height, "100%"),
    ];
    own.extend_from_slice(root);
    let mut held = vec![(StyleId::Height, "100")];
    held.extend_from_slice(rows);
    let reads = vec![(StyleId::PaddingBottom, observed(Edge::Bottom, None))];
    let mut ops = create(3..=5);
    ops.push(style(2, vec![], &own));
    ops.push(style(3, vec![], &[(StyleId::FlexGrow, "1")]));
    ops.push(style(4, vec![], &held));
    let full = [
        (StyleId::Height, "100%"),
        (StyleId::BoxSizing, "border-box"),
    ];
    ops.push(style(5, reads, &full));
    ops.extend([children(1, &[2]), children(2, &[3, 4]), children(4, &[5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent));
    k.set_owner_keyboard(2, keyboard).unwrap();
    lay_out(&mut k);
    k
}

/// The bottom of `id`'s frame.
fn bottom(k: &Kernel, id: ViewId) -> f32 {
    let (_, y, _, height) = frame(k, id);
    y + height
}

fn ignore(regions: &str) -> (StyleId, &str) {
    (StyleId::SafeAreaIgnore, regions)
}

const PADDED: (StyleId, &str) = (StyleId::SafeAreaPaddingBottom, "20");

#[test]
fn the_content_root_ends_at_the_keyboards_top_under_resizes_content() {
    // The keyboard covers the 34-point band rather than adding to it: the
    // root ends at its top, where UIKit's keyboard guide does
    let mut k = column(&[], &[], docked(UP));
    assert_eq!(bottom(&k, 2), UP);
    // Dismissed, the root ends at the band's inner side again
    assert!(k.set_owner_keyboard(2, None).unwrap());
    lay_out(&mut k);
    assert_eq!(bottom(&k, 2), 810.0);
    // Undocked, a keyboard occupies nothing, nor does one clear of the
    // allocation or one shallower than the band, which it covers
    let undocked = docked(UP).map(|k| Keyboard { docked: false, ..k });
    let beside = Some(Keyboard {
        frame: [390.0, UP, 300.0, 328.0],
        docked: true,
    });
    for keyboard in [undocked, beside, docked(824.0)] {
        k.set_owner_keyboard(2, keyboard).unwrap();
        lay_out(&mut k);
        assert_eq!(bottom(&k, 2), 810.0);
    }
    // The other policies clear nothing, and with none reported the keyboard
    // lays nothing out
    k.set_owner_keyboard(2, docked(UP)).unwrap();
    for policy in [
        Some(KeyboardPolicy::ResizesVisual),
        Some(KeyboardPolicy::OverlaysContent),
        None,
    ] {
        k.set_keyboard_policy(policy);
        lay_out(&mut k);
        assert_eq!(bottom(&k, 2), 810.0);
    }
    assert!(k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent)));
    lay_out(&mut k);
    assert_eq!(bottom(&k, 2), UP);
    // A frame that is not finite, or has a negative size, is refused
    let nan = Some(Keyboard {
        frame: [0.0, f32::NAN, 390.0, 328.0],
        docked: true,
    });
    let negative = Some(Keyboard {
        frame: [0.0, UP, 390.0, -1.0],
        docked: true,
    });
    for refused in [nan, negative] {
        let refusal = k.set_owner_keyboard(2, refused);
        assert!(matches!(
            refusal,
            Err(KernelError::Layout(LayoutError::Keyboard(_)))
        ));
    }
    assert_eq!(bottom(&k, 2), UP);
}

#[test]
fn a_root_that_takes_one_region_clears_the_other() {
    // Taking the keyboard region leaves the band, as UIKit's safe-area
    // layout guide ends at 810 with the keyboard up; taking the container
    // region leaves the keyboard, as the guide without the bottom safe area
    // ends at its top, even where the keyboard is shallower than the band;
    // taking both clears neither
    let cases = [
        ("keyboard bottom", UP, 810.0),
        ("container bottom", UP, UP),
        ("container bottom", 824.0, 824.0),
        ("bottom", UP, 844.0),
    ];
    for (regions, top, ends) in cases {
        let k = column(&[ignore(regions)], &[], docked(top));
        assert_eq!(bottom(&k, 2), ends, "{regions} under a keyboard at {top}");
    }
}

#[test]
fn the_keyboard_covers_the_additional_inset_and_the_padding_stacks_on_it() {
    // A 20-point additional inset with the band is 54, which the keyboard
    // covers as UIKit's guide covers `additionalSafeAreaInsets`; the 20-point
    // safe-area padding lies inside the content root and stacks on it
    let added = (StyleId::AdditionalSafeAreaBottom, "20");
    let k = column(&[added, PADDED], &[], None);
    assert_eq!(bottom(&k, 2), 844.0 - 54.0 - 20.0);
    let k = column(&[added, PADDED], &[], docked(UP));
    assert_eq!(bottom(&k, 2), UP - 20.0);
    // A negative inset reduces the band before the comparison, never the
    // occupancy; one deeper than the keyboard is the deeper
    let less = (StyleId::AdditionalSafeAreaBottom, "-20");
    assert_eq!(
        bottom(&column(&[less, PADDED], &[], docked(UP)), 2),
        UP - 20.0
    );
    let more = (StyleId::AdditionalSafeAreaBottom, "400");
    let k = column(&[more, PADDED], &[], docked(UP));
    assert_eq!(bottom(&k, 2), 844.0 - 434.0 - 20.0);
}

#[test]
fn a_box_on_the_keyboard_takes_what_its_regions_let_it() {
    // 4 sits on the keyboard: ignoring the container region it stays at the
    // keyboard's top, ignoring the keyboard it reaches the band's inner side,
    // and ignoring both the allocation's edge, where UIKit ends a view pinned
    // to its keyboard guide without the bottom safe area, to its safe-area
    // layout guide and to its controller view's edge
    let cases = [
        ("container bottom", UP),
        ("keyboard bottom", 810.0),
        ("bottom", 844.0),
    ];
    for (regions, ends) in cases {
        let k = column(&[], &[ignore(regions)], docked(UP));
        assert_eq!(bottom(&k, 4), ends, "{regions}");
        assert_eq!(frame(&k, 4).1, UP - 100.0, "{regions} keeps its place");
    }
    // Under safe-area padding stacked on the keyboard, ignoring the container
    // region passes the padding to the keyboard's top, and ignoring only the
    // keyboard leaves the box on the padding, as under a bar
    let on = UP - 20.0;
    let cases = [
        ("container bottom", UP),
        ("keyboard bottom", on),
        ("bottom", 844.0),
    ];
    for (regions, ends) in cases {
        let k = column(&[PADDED], &[ignore(regions)], docked(UP));
        assert_eq!(
            (frame(&k, 4).1, bottom(&k, 4)),
            (on - 100.0, ends),
            "{regions}"
        );
    }
    // With the keyboard down, ignoring only the keyboard takes nothing, and
    // a keyboard shallower than the band leaves one that ignores only the
    // container region at its top
    let k = column(&[], &[ignore("keyboard bottom")], None);
    assert_eq!(bottom(&k, 4), 810.0);
    let k = column(&[], &[ignore("container bottom")], docked(824.0));
    assert_eq!(bottom(&k, 4), 824.0);
}

#[test]
fn the_layer_reaches_every_box_and_bounds_without_moving_an_edge_in() {
    // The root takes the keyboard region and ends at the band's inner side,
    // under the keyboard: a box there that ignores the container region
    // observes the keyboard, which covers the band, so it keeps its edge;
    // one that ignores both reaches the allocation's edge
    let root = [ignore("keyboard bottom")];
    let k = column(&root, &[ignore("container bottom")], docked(UP));
    assert_eq!(bottom(&k, 4), 810.0);
    let k = column(&root, &[ignore("bottom")], docked(UP));
    assert_eq!(bottom(&k, 4), 844.0);
}

#[test]
fn a_box_extended_under_the_keyboard_observes_to_the_container_regions_inner_side() {
    // 5 fills 4, which ignores both regions and reaches the allocation's
    // edge: with no reservation on the keyboard it observes the band alone,
    // the keyboard being no part of the container region; under padding
    // stacked on the keyboard it observes to the padding's inner side
    let k = column(&[], &[ignore("bottom")], docked(UP));
    assert_eq!(k.observation(5).unwrap()[2], Some(34.0));
    let k = column(&[PADDED], &[ignore("bottom")], docked(UP));
    assert_eq!(k.observation(5).unwrap()[2], Some(844.0 - (UP - 20.0)));
}

#[test]
fn a_box_clear_of_the_keyboard_takes_nothing_of_it_and_the_other_edges_still_count() {
    // 3 grows above 4, clear of the keyboard: ignoring both regions it keeps
    // its bottom; on the top, which the band reaches, it still takes the band
    let mut k = column(&[], &[], docked(UP));
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.apply(0, 9, &[style(3, vec![], &[ignore("top bottom")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 3).1, 0.0);
    assert_eq!(bottom(&k, 3), UP - 100.0);
}

#[test]
fn a_scroller_that_does_not_pass_the_layer_gives_its_content_none() {
    // 3 scrolls inside the root, which ends at the keyboard's top; 4, in its
    // content at rest below the fold, lies in the layer only by the scroll
    // offset at zero, so ignoring the keyboard or both regions takes nothing
    let rows = [(StyleId::OverflowY, "scroll"), (StyleId::FlexGrow, "1")];
    for regions in ["bottom", "keyboard bottom", "container bottom"] {
        let mut ops = create(3..=5);
        ops.push(style(
            2,
            vec![],
            &[
                (StyleId::SafeAreaIgnore, "none"),
                (StyleId::Display, "flex"),
                (StyleId::FlexDirection, "column"),
                (StyleId::Height, "100%"),
            ],
        ));
        ops.push(style(3, vec![], &rows));
        ops.push(style(4, vec![], &[(StyleId::Height, "700")]));
        ops.push(style(
            5,
            vec![],
            &[(StyleId::Height, "100"), ignore(regions)],
        ));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
            .unwrap();
        k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent));
        k.set_owner_keyboard(2, docked(UP)).unwrap();
        lay_out(&mut k);
        assert_eq!(frame(&k, 5), (0.0, 700.0, 390.0, 100.0), "{regions}");
    }
}

#[test]
fn a_fitted_sheet_measures_the_keyboard_on_its_bounds() {
    // A sheet that fits 300 points of content over a 34-point band is 334
    // tall; a keyboard 6 points down that view occupies 328 of it, not the
    // 794 its 800-point maximum would give, and the sheet grows to hold
    // its content above it
    let mut ops = create(3..=3);
    ops.push(style(2, vec![], &[(StyleId::SafeAreaIgnore, "none")]));
    ops.push(style(3, vec![], &[(StyleId::Height, "300")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    let sheet = block(BlockAxis::Is(390.0), BlockAxis::AtMost(800.0));
    k.set_host_block(2, Some(sheet)).unwrap();
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent));
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some((390.0, 334.0)));
    let keyboard = Keyboard {
        frame: [0.0, 6.0, 390.0, 328.0],
        docked: true,
    };
    k.set_owner_keyboard(2, Some(keyboard)).unwrap();
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some((390.0, 628.0)));
}

#[test]
fn a_freed_owners_keyboard_goes_with_it() {
    // The owner holding a keyboard is destroyed and a new one takes its
    // slot: the new one has no keyboard until its host reports one
    let mut k = column(&[], &[], docked(UP));
    let slot = k.arena.slot_of(2).unwrap();
    let destroyed = [5, 4, 3, 2].map(|id| Op::DestroyView { id });
    k.apply(0, 9, &[children(1, &[])]).unwrap();
    k.apply(0, 10, &destroyed).unwrap();
    let mut ops = create(12..=12);
    ops.push(style(
        12,
        vec![],
        &[(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")],
    ));
    ops.push(children(1, &[12]));
    k.apply(0, 11, &ops).unwrap();
    assert_eq!(k.arena.slot_of(12), Some(slot), "the freed slot is reused");
    let owner = block(BlockAxis::Is(390.0), BlockAxis::Is(844.0));
    k.set_host_block(12, Some(owner)).unwrap();
    k.set_owner_band(12, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.set_keyboard_policy(None);
    k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent));
    lay_out(&mut k);
    assert_eq!(bottom(&k, 12), 810.0);
}

#[test]
fn a_keyboard_change_owes_a_layout_where_it_lays_something_out() {
    // A keyboard that moves the content root owes a layout; one that lays
    // nothing out, under another policy, owes none
    let mut k = column(&[], &[], None);
    assert!(k.set_owner_keyboard(2, docked(UP)).unwrap());
    lay_out(&mut k);
    k.set_keyboard_policy(Some(KeyboardPolicy::OverlaysContent));
    lay_out(&mut k);
    assert!(!k.set_owner_keyboard(2, docked(600.0)).unwrap());
    // A root that takes the keyboard region does not move, but the layer its
    // boxes take does, so a box that ignores owes one
    let mut k = column(&[ignore("keyboard bottom")], &[ignore("bottom")], None);
    assert!(k.set_owner_keyboard(2, docked(UP)).unwrap());
    lay_out(&mut k);
    assert_eq!((bottom(&k, 2), bottom(&k, 4)), (810.0, 844.0));
    // A reset keeps the policy, which is the session's, and each owner's
    // keyboard goes with the tree, as its band does
    let mut k = column(&[], &[], docked(UP));
    k.arena.observations = k.arena.observations.after_reset();
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(bottom(&k, 2), 810.0);
    k.set_owner_keyboard(2, docked(UP)).unwrap();
    lay_out(&mut k);
    assert_eq!(bottom(&k, 2), UP);
}

/// The box's own keyboard reading of `family` on `edge` (D7).
fn reads(family: calc::Family, edge: Edge) -> Node {
    Node::Observe(calc::Observe {
        target: Target::Box,
        family,
        side: edge.into(),
        fallback: None,
    })
}

/// `id`'s reading of `family`, top right bottom left.
fn reading(k: &Kernel, id: ViewId, family: calc::Family) -> super::Reading {
    k.observation_of(id, family).unwrap()
}

/// `id`'s resolved padding, top right bottom left.
fn padding(k: &Kernel, id: ViewId) -> [f32; 4] {
    let (left, top, right, bottom) = k.resolved_padding(k.arena.key(slot(k, id))).unwrap();
    [top, right, bottom, left]
}

/// Under the column, 3 (0 to 710) pads its left by its docked reading's
/// bottom, written as an author writes it with a fallback for where it is
/// unavailable, and 4 (710 to 810) its right by the one that follows an
/// undocked keyboard too. Neither padding moves a frame.
fn reading_column(policy: KeyboardPolicy, keyboard: Option<Keyboard>) -> Kernel {
    let mut k = column(&[], &[], None);
    let docked = [(
        StyleId::PaddingLeft,
        "var(--exact-keyboard-inset-bottom, 5px)",
    )];
    let followed = vec![(
        StyleId::PaddingRight,
        reads(calc::Family::KeyboardUndocked, Edge::Bottom),
    )];
    let held = [(StyleId::Height, "100")];
    let ops = [style(3, vec![], &docked), style(4, followed, &held)];
    k.apply(0, 9, &ops).unwrap();
    k.set_keyboard_policy(Some(policy));
    k.set_owner_keyboard(2, keyboard).unwrap();
    lay_out(&mut k);
    k
}

#[test]
fn a_box_reads_how_far_the_docked_keyboard_reaches_into_it() {
    use calc::Family::{Keyboard as Docked, KeyboardUndocked as Followed};
    // Over content that keeps its size, the keyboard's top at 516 lies 194
    // points above 3's bottom and 294 above 4's, more than 4's 100 points,
    // as UIKit's guide is the keyboard's frame in each view, not clipped to
    // it; the other edges read 0
    let k = reading_column(KeyboardPolicy::OverlaysContent, docked(UP));
    assert_eq!(
        reading(&k, 3, Docked),
        [Some(0.0), Some(0.0), Some(710.0 - UP), Some(0.0)]
    );
    assert_eq!(
        reading(&k, 4, Followed),
        [Some(0.0), Some(0.0), Some(810.0 - UP), Some(0.0)]
    );
    assert_eq!(
        (padding(&k, 3)[3], padding(&k, 4)[1]),
        (710.0 - UP, 810.0 - UP)
    );
    // Where the content root ends at the keyboard's top, nothing in it reads it
    let k = reading_column(KeyboardPolicy::ResizesContent, docked(UP));
    assert_eq!(reading(&k, 3, Docked)[2], Some(0.0));
    // Dismissed, it reads 0, not its fallback, and UIKit's default guide, the
    // deeper of it and the bottom safe area, is composed in a length: a box at
    // the allocation's edge pads by the band's 34 points, and with the
    // keyboard up by its 328, as UIKit's full-size view's guide does
    let mut k = reading_column(KeyboardPolicy::OverlaysContent, None);
    assert_eq!(reading(&k, 3, Docked)[2], Some(0.0));
    assert_eq!(padding(&k, 3)[3], 0.0);
    let guide = Node::Max(vec![
        reads(Docked, Edge::Bottom),
        observed(Edge::Bottom, None),
    ]);
    let rows = [(StyleId::SafeAreaIgnore, "bottom")];
    k.apply(
        0,
        10,
        &[style(4, vec![(StyleId::PaddingBottom, guide)], &rows)],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(padding(&k, 4)[2], 34.0);
    assert!(k.set_owner_keyboard(2, docked(UP)).unwrap());
    lay_out(&mut k);
    assert_eq!(padding(&k, 4)[2], 844.0 - UP);
}

#[test]
fn a_box_reads_an_undocked_keyboard_only_where_it_follows_it() {
    use calc::Family::{Keyboard as Docked, KeyboardUndocked as Followed};
    let floating = |frame| {
        Some(Keyboard {
            frame,
            docked: false,
        })
    };
    let at = |frame| {
        let k = reading_column(KeyboardPolicy::OverlaysContent, floating(frame));
        (reading(&k, 3, Docked), reading(&k, 4, Followed))
    };
    // Across 4's top (650 to 750 over 4's 710 to 810) it reaches 40 points
    // in from the top for the guide that follows it, and the docked guide
    // reads nothing of it
    let (docked, followed) = at([100.0, 650.0, 200.0, 100.0]);
    assert_eq!(followed, [Some(40.0), Some(0.0), Some(0.0), Some(0.0)]);
    assert_eq!(docked, [Some(0.0); 4]);
    // Across its bottom-left corner it reaches in from the left and the
    // bottom, each to its far side, and across its right edge from the right
    let (_, followed) = at([-50.0, 780.0, 150.0, 100.0]);
    assert_eq!(followed, [Some(0.0), Some(0.0), Some(30.0), Some(100.0)]);
    let (_, followed) = at([340.0, 730.0, 100.0, 60.0]);
    assert_eq!(followed, [Some(0.0), Some(50.0), Some(0.0), Some(0.0)]);
    // As wide as the box, it enters from neither side
    let (_, followed) = at([0.0, 760.0, 390.0, 100.0]);
    assert_eq!(followed, [Some(0.0), Some(0.0), Some(50.0), Some(0.0)]);
    // Wholly inside it, or clear of it, it crosses no edge
    for frame in [[100.0, 730.0, 200.0, 60.0], [100.0, 300.0, 200.0, 100.0]] {
        assert_eq!(at(frame).1, [Some(0.0); 4], "{frame:?}");
    }
}

#[test]
fn the_docked_reading_places_the_keyboard_as_the_occupancy_does() {
    // A keyboard beside the owner occupies nothing, and nothing reads it
    let beside = Some(Keyboard {
        frame: [390.0, UP, 300.0, 328.0],
        docked: true,
    });
    let k = reading_column(KeyboardPolicy::OverlaysContent, beside);
    assert_eq!(
        reading(&k, 4, calc::Family::KeyboardUndocked),
        [Some(0.0); 4]
    );
    // A fitted sheet that grows over the keyboard measures it up from its
    // bottom, as its occupancy is: its 300-point content, on the keyboard's
    // top, reads none of it
    let mut ops = create(3..=3);
    ops.push(style(2, vec![], &[(StyleId::SafeAreaIgnore, "none")]));
    let pad = vec![(
        StyleId::PaddingLeft,
        reads(calc::Family::Keyboard, Edge::Bottom),
    )];
    ops.push(style(3, pad, &[(StyleId::Height, "300")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    let sheet = block(BlockAxis::Is(390.0), BlockAxis::AtMost(800.0));
    k.set_host_block(2, Some(sheet)).unwrap();
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent));
    lay_out(&mut k);
    let keyboard = Keyboard {
        frame: [0.0, 6.0, 390.0, 328.0],
        docked: true,
    };
    k.set_owner_keyboard(2, Some(keyboard)).unwrap();
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some((390.0, 628.0)));
    assert_eq!(reading(&k, 3, calc::Family::Keyboard)[2], Some(0.0));
}

#[test]
fn a_reader_that_grows_by_its_reading_settles_at_once() {
    // At the column's end over content that keeps its size, a composer that
    // pads its bottom by its reading, and an empty spacer as tall as its
    // reading, each take the keyboard's 294 points over the content root's
    // end in the solve's first layout: the reading is not limited by the
    // box's size, so it does not climb with it
    let grows = |row: StyleId, rows: &[(StyleId, &'static str)]| {
        let mut ops = create(3..=4);
        let column = [
            (StyleId::SafeAreaIgnore, "none"),
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
            (StyleId::Height, "100%"),
        ];
        ops.push(style(2, vec![], &column));
        ops.push(style(3, vec![], &[(StyleId::FlexGrow, "1")]));
        let reader = vec![(row, reads(calc::Family::Keyboard, Edge::Bottom))];
        ops.push(style(4, reader, rows));
        ops.extend([children(1, &[2]), children(2, &[3, 4])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
            .unwrap();
        k.set_keyboard_policy(Some(KeyboardPolicy::OverlaysContent));
        lay_out(&mut k);
        k.set_owner_keyboard(2, docked(UP)).unwrap();
        lay_out(&mut k);
        assert_eq!(k.arena.observations.rounds, 1, "{row:?}");
        assert!(k.safe_area_unsettled().is_empty(), "{row:?}");
        k
    };
    let k = grows(StyleId::PaddingBottom, &[(StyleId::PaddingTop, "8")]);
    assert_eq!(padding(&k, 4)[2], 810.0 - UP);
    assert_eq!(frame(&k, 4).3, 8.0 + 810.0 - UP);
    let k = grows(StyleId::Height, &[(StyleId::FlexShrink, "0")]);
    assert_eq!(frame(&k, 4).3, 810.0 - UP);
}

#[test]
fn a_reader_whose_edge_moves_by_its_reading_takes_the_keyboards_depth() {
    // 4 follows 3 (526 tall) in flow and is as tall as its reading: its top
    // lies 10 points below the keyboard's top at 516, so each round its
    // bottom moves one for one with its input, and the solve takes the
    // keyboard's depth, the only fixed point, as UIKit's guide is 328 tall
    // wherever its view lies
    let mut ops = create(3..=4);
    ops.push(style(2, vec![], &[(StyleId::SafeAreaIgnore, "none")]));
    ops.push(style(3, vec![], &[(StyleId::Height, "526")]));
    let tall = vec![(StyleId::Height, reads(calc::Family::Keyboard, Edge::Bottom))];
    ops.push(style(4, tall, &[]));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.set_keyboard_policy(Some(KeyboardPolicy::OverlaysContent));
    lay_out(&mut k);
    k.set_owner_keyboard(2, docked(UP)).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 4).3, 844.0 - UP);
    assert_eq!(k.arena.observations.rounds, 2);
    assert!(k.safe_area_unsettled().is_empty());
}

#[test]
fn a_scroller_that_takes_the_keyboard_gives_its_content_none() {
    // 5, at rest below 3's fold, lies under the keyboard and pads its left
    // by its reading: over content that keeps its size nothing takes it;
    // under `resizes-visual` 3 takes the keyboard as its content inset and
    // under `resizes-content` 3 ends at its top, and 5 reads none of it.
    // Each restyle of the scroller writes every row it varies, since a style
    // patch keeps the rows it does not name
    let scroller = |rows: &[(StyleId, &'static str)]| {
        let mut all = vec![
            (StyleId::OverflowX, "visible"),
            (StyleId::OverflowY, "scroll"),
            (StyleId::FlexGrow, "1"),
            ignore("none"),
        ];
        all.extend_from_slice(rows);
        style(3, vec![], &all)
    };
    let mut k = {
        let mut ops = create(3..=5);
        let column = [
            (StyleId::SafeAreaIgnore, "none"),
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
            (StyleId::Height, "100%"),
        ];
        ops.push(style(2, vec![], &column));
        ops.push(scroller(&[]));
        ops.push(style(4, vec![], &[(StyleId::Height, "700")]));
        let reader = vec![(
            StyleId::PaddingLeft,
            reads(calc::Family::Keyboard, Edge::Bottom),
        )];
        ops.push(style(5, reader, &[(StyleId::Height, "100")]));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
            .unwrap();
        k.set_keyboard_policy(Some(KeyboardPolicy::OverlaysContent));
        k.set_owner_keyboard(2, docked(UP)).unwrap();
        lay_out(&mut k);
        k
    };
    let read = |k: &Kernel| reading(k, 5, calc::Family::Keyboard)[2];
    // 5 lies from 700 to 800, its bottom 284 points below the keyboard's top
    assert_eq!(read(&k), Some(800.0 - UP));
    // A policy that lays nothing else out still owes the reading's layout,
    // as `resizes-visual` does after `overlays-content`
    for policy in [
        KeyboardPolicy::ResizesVisual,
        KeyboardPolicy::ResizesContent,
    ] {
        assert!(k.set_keyboard_policy(Some(policy)), "{policy:?}");
        lay_out(&mut k);
        assert_eq!(read(&k), Some(0.0), "{policy:?}");
    }
    k.set_keyboard_policy(Some(KeyboardPolicy::ResizesVisual));
    lay_out(&mut k);
    // A scroller that comes to ignore the keyboard region passes it, though
    // no frame moves, and so does one that scrolls only sideways
    let sideways = [
        (StyleId::OverflowX, "scroll"),
        (StyleId::OverflowY, "hidden"),
    ];
    for (n, rows) in [&[ignore("keyboard bottom")][..], &sideways]
        .into_iter()
        .enumerate()
    {
        k.apply(0, 10 + n as u64, &[scroller(rows)]).unwrap();
        lay_out(&mut k);
        assert_eq!(read(&k), Some(800.0 - UP), "{rows:?}");
    }
    // And one that stops ignoring it takes it again
    k.apply(0, 12, &[scroller(&[])]).unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k), Some(0.0));
}

#[test]
fn a_box_outside_every_owner_reads_the_sessions_keyboard() {
    // 3 lies in the document, beside the owner; it reads the session's
    // keyboard in the document's space
    let mut ops = create(3..=3);
    let reader = vec![(
        StyleId::PaddingLeft,
        reads(calc::Family::Keyboard, Edge::Bottom),
    )];
    ops.push(style(3, reader, &[(StyleId::Height, "844")]));
    ops.push(children(1, &[2, 3]));
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!(reading(&k, 3, calc::Family::Keyboard)[2], Some(0.0));
    assert!(k.set_session_keyboard(docked(UP)).unwrap());
    lay_out(&mut k);
    assert_eq!(reading(&k, 3, calc::Family::Keyboard)[2], Some(844.0 - UP));
}

#[test]
fn a_popover_its_host_places_reads_no_keyboard() {
    // The popover 3, whose host gives it a block and shows it where the
    // kernel is not told, reads its keyboard with a 7-point fallback, which
    // applies: no keyboard frame can be placed in a space the kernel does
    // not know
    let mut ops = create(3..=3);
    let pad = vec![(
        StyleId::PaddingTop,
        Node::Observe(calc::Observe {
            fallback: Some(Box::new(Node::Value(7.0, calc::Unit::Px))),
            ..match reads(calc::Family::Keyboard, Edge::Bottom) {
                Node::Observe(o) => o,
                _ => unreachable!(),
            }
        }),
    )];
    // Its capture of the reading is invalid too, so 4's fallback applies
    let rows = [
        (StyleId::Height, "10"),
        (
            StyleId::SafeAreaProperties,
            "--kb: var(--exact-keyboard-inset-bottom)",
        ),
    ];
    ops.extend([style(3, pad, &rows), popover(3)]);
    ops.extend(create(4..=4));
    ops.push(style(
        4,
        vec![],
        &[(StyleId::PaddingLeft, "var(--kb, 7px)")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(200.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(block)).unwrap();
    k.set_session_keyboard(docked(UP)).unwrap();
    lay_out(&mut k);
    assert!(k.is_top_layer(3));
    assert_eq!(reading(&k, 3, calc::Family::Keyboard), [None; 4]);
    assert_eq!(padding(&k, 3)[0], 7.0, "the fallback");
    assert_eq!(padding(&k, 4)[3], 7.0, "the capture's reader's fallback");
}

#[test]
fn a_reading_in_a_value_held_as_text_is_read() {
    // A value whose `env()` has a fallback is held as text and parsed once
    // substituted; its keyboard reading is a reading all the same
    let mut k = reading_column(KeyboardPolicy::OverlaysContent, docked(UP));
    let text = "max(env(safe-area-inset-bottom, 0px), var(--exact-keyboard-inset-bottom))";
    k.apply(0, 10, &[style(3, vec![], &[(StyleId::PaddingRight, text)])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(padding(&k, 3)[1], 710.0 - UP);
}

#[test]
fn a_keyboard_leaf_crosses_the_wire_where_the_parser_reads_it() {
    // The box's own reading, with a fallback, round-trips through its text,
    // `var(--exact-keyboard-inset-bottom, 3px)`; a chain entry's names no
    // host name, so the decoder refuses it
    let leaf = |target| {
        Node::Observe(calc::Observe {
            target,
            family: calc::Family::Keyboard,
            side: Edge::Bottom.into(),
            fallback: Some(Box::new(Node::Value(3.0, calc::Unit::Px))),
        })
    };
    let decoded = |node: Node| {
        let mut w = crate::wire::codec::Writer::new();
        calc::Comparison::intern(node).encode(&mut w);
        let bytes = w.into_vec();
        let mut r = crate::wire::codec::Reader::new(&bytes);
        calc::Comparison::decode(&mut r, StyleId::PaddingBottom).map(|c| {
            let mut text = String::new();
            c.css(&mut text);
            text
        })
    };
    let text = decoded(leaf(Target::Box)).unwrap();
    assert_eq!(text, "var(--exact-keyboard-inset-bottom, 3px)");
    assert!(decoded(leaf(Target::Chain(2))).is_err());
}

#[test]
fn a_capture_holds_its_declaring_boxs_reading() {
    // 3 fills the content root and declares `--kb` as its own docked
    // reading; its child 4 pads by it, with a fallback that never applies:
    // the keyboard's top at 516 lies 294 points above 3's bottom at 810,
    // and dismissed 3 reads 0 (LLP 1123 D5). 5 declares `--pad` from it and
    // its child 6 pads by that, through the two declarations; 7 declares
    // `--kb` itself, which its own padding reads, nearer than 3's
    let mut ops = create(3..=7);
    let column = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    ops.push(style(2, vec![], &column));
    let declares = [
        (StyleId::Height, "100%"),
        (
            StyleId::SafeAreaProperties,
            "--kb: var(--exact-keyboard-inset-bottom)",
        ),
    ];
    ops.push(style(3, vec![], &declares));
    ops.push(style(
        4,
        vec![],
        &[(StyleId::PaddingLeft, "var(--kb, 7px)")],
    ));
    ops.push(style(
        5,
        vec![],
        &[(StyleId::SafeAreaProperties, "--pad: calc(var(--kb) + 8px)")],
    ));
    ops.push(style(6, vec![], &[(StyleId::PaddingLeft, "var(--pad)")]));
    let nearer = [
        (StyleId::SafeAreaProperties, "--kb: 5px"),
        (StyleId::PaddingLeft, "var(--kb)"),
    ];
    ops.push(style(7, vec![], &nearer));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4, 5, 7]),
        children(5, &[6]),
    ]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.set_keyboard_policy(Some(KeyboardPolicy::OverlaysContent));
    lay_out(&mut k);
    let left = |k: &Kernel| [4, 6, 7].map(|id| padding(k, id)[3]);
    assert_eq!(left(&k), [0.0, 8.0, 5.0]);
    assert!(k.set_owner_keyboard(2, docked(UP)).unwrap());
    lay_out(&mut k);
    assert_eq!(left(&k), [810.0 - UP, 810.0 - UP + 8.0, 5.0]);
    assert!(k.set_owner_keyboard(2, None).unwrap());
    lay_out(&mut k);
    assert_eq!(left(&k), [0.0, 8.0, 5.0]);
}

#[test]
fn a_capture_a_content_region_withholds_is_invalid() {
    // The content region's owner 2 holds the content 3, where 7 declares
    // `--kb` as its own reading and its child 10 pads by it with a 5-point
    // fallback: the region withholds 7's observations, so its capture is
    // invalid and 10 takes the fallback (LLP 1123 D5)
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3, 7, 10]);
    for id in [4, 5] {
        ops.push(Op::CreateView {
            id,
            node_type: NodeType::Text,
        });
        ops.push(Op::SetProp {
            id,
            prop: crate::PropId::Text,
            value: "word".into(),
        });
    }
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    ops.push(style(2, vec![], &REGION));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    let declares = [
        (StyleId::Height, "10"),
        (
            StyleId::SafeAreaProperties,
            "--kb: var(--exact-keyboard-inset-bottom)",
        ),
    ];
    ops.push(style(7, vec![], &declares));
    ops.push(style(
        10,
        vec![],
        &[(StyleId::PaddingLeft, "var(--kb, 5px)")],
    ));
    ops.extend([
        children(7, &[10]),
        children(3, &[4, 7]),
        children(2, &[3, 5]),
        children(1, &[2]),
    ]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    lay_out_region(&mut k, Default::default());
    let reader = &k.arena.style(slot(&k, 10)).padding_left;
    assert_eq!(reader.at_basis(&k.env_of(10), 0.0), 5.0);
}

#[test]
fn a_boxs_keyboard_facts_tell_a_dismissed_keyboard_from_an_unreported_one() {
    // Shown, the guide is the keyboard's frame and the occupancy its overlap with the owner's
    // allocation; dismissed, both rest on a bottom of no height, the guide on the box's; and
    // where the host reports nothing, as the session has not, there are none
    let k = column(&[], &[], docked(UP));
    let shown = k.keyboard_facts(4).unwrap();
    assert_eq!(shown.guide, Some([0.0, UP, 390.0, 844.0 - UP]));
    assert_eq!(shown.occupancy, [0.0, UP, 390.0, 844.0 - UP]);
    assert!(shown.docked);
    let mut k = column(&[], &[], None);
    let (x, y, width, height) = frame(&k, 4);
    let dismissed = k.keyboard_facts(4).unwrap();
    assert_eq!(dismissed.guide, Some([x, y + height, width, 0.0]));
    assert_eq!(dismissed.occupancy, [0.0, 844.0, 390.0, 0.0]);
    assert!(!dismissed.docked);
    assert_eq!(k.keyboard_facts(1), None);
    k.update_owner(2, crate::kernel::OwnerUpdate::default())
        .unwrap();
    assert_eq!(k.keyboard_facts(4), None);
    k.set_session_keyboard(None).unwrap();
    assert!(k.keyboard_facts(1).is_some_and(|f| !f.docked));
}
