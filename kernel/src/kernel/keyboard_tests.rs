//! The keyboard region (LLP 1123 D7): what an owner's root clears of the
//! keyboard (D3) and what a box inside the root takes of its layer (D4, D9),
//! with the values UIKit's guides give, measured on iOS 27.0 and 27.1
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
