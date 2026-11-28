//! `Kernel::set_owner_layout`: LLP 1123's stage 5 layout, before the switch turns it on.
use super::super::owner::tests::{children, create, style};
use super::*;
use crate::id::Offer;
use crate::kernel::{BlockAxis, HostBlock};
use crate::wire::Op;
use crate::{HostCover, StyleId, ViewId};

const BAND: [f32; 4] = [47.0, 0.0, 34.0, 0.0];
const TOP: StyleId = StyleId::PaddingTop;

/// 1 the document, 2 an owner's root (a 390 by 844 block) with `root`'s rows, 3 in it with
/// `inner`'s, and 4, 10 tall, in 3; the owner layout `on` from the start.
fn document(on: bool, root: &[(StyleId, &str)], inner: &[(StyleId, &str)]) -> Kernel {
    let mut k = Kernel::with_monospace();
    k.set_owner_layout(on);
    build(&mut k, root, inner);
    k
}

/// [`document`]'s nodes, made in `k`.
fn build(k: &mut Kernel, root: &[(StyleId, &str)], inner: &[(StyleId, &str)]) {
    let mut ops = create(1..=4);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(2, root));
    ops.push(style(3, inner));
    ops.push(style(4, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_host_block(2, Some(block())).unwrap();
}

fn block() -> HostBlock {
    HostBlock {
        tactic: Default::default(),
        width: BlockAxis::Is(390.0),
        height: BlockAxis::Is(844.0),
    }
}

/// What a host reports for the owner under either layout: the cover and local insets main's
/// layout reads, and the band stage 5's reads.
fn report(k: &mut Kernel) {
    k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
    k.set_local_insets(2, Some(BAND)).unwrap();
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
}

fn y(k: &mut Kernel, id: ViewId) -> f32 {
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    k.node(id).unwrap().frame.y
}

/// `id`'s top padding as layout resolved it.
fn top(k: &Kernel, id: ViewId) -> f32 {
    let slot = k.arena.slot_of(id).unwrap();
    k.resolved_padding(k.arena.key(slot)).unwrap().1
}

/// `id`'s padding as the engine laid it out, top right bottom left.
fn laid_padding(k: &Kernel, id: ViewId) -> [f32; 4] {
    let tree = k
        .layout
        .as_deref()
        .and_then(crate::layout::LayoutMirror::tree_ref);
    let node = k.arena.taffy(k.arena.slot_of(id).unwrap()).unwrap();
    let p = tree.unwrap().layout(node).padding;
    [p.top, p.right, p.bottom, p.left]
}

#[test]
fn an_owners_root_takes_no_padding_from_its_edge_cover() {
    // Main's layout pads the root by the cover its hosts send today
    let mut k = document(false, &[], &[]);
    k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
    assert_eq!((y(&mut k, 2), y(&mut k, 3)), (0.0, 47.0));
    // Stage 5's clears the band through the content root, with the cover or without it, and a
    // host reads back the root's authored padding and no bottom clearance from the cover
    for cover in [false, true] {
        let mut k = document(true, &[(TOP, "10")], &[]);
        k.set_owner_band(2, Some(BAND), &[]).unwrap();
        if cover {
            k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
        }
        assert_eq!((y(&mut k, 2), y(&mut k, 3)), (47.0, 57.0), "cover {cover}");
        let key = k.arena.key(k.arena.slot_of(2).unwrap());
        assert_eq!(
            (top(&k, 2), k.cover_bottom(key)),
            (10.0, 0.0),
            "cover {cover}"
        );
    }
}

#[test]
fn an_owners_root_decides_its_own_bottom() {
    // 2, covered, holds the owner 3 whose last child 4 is a scroller the platform insets at
    // its bottom: whether or not 3 has a cover, 2 keeps its bottom clearance, since what is
    // inset below 3 is 3's
    let bottom = |cover: bool| {
        let mut k = Kernel::with_monospace();
        k.set_owner_layout(true);
        let mut ops = create(1..=4);
        ops.push(style(1, &[(StyleId::Height, "844")]));
        ops.push(style(
            4,
            &[(StyleId::OverflowY, "scroll"), (StyleId::Height, "100")],
        ));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
        k.set_host_block(3, Some(block())).unwrap();
        if cover {
            k.set_host_cover(3, Some(HostCover::Edges(BAND))).unwrap();
        }
        k.set_host_cover(4, Some(HostCover::Inset([0.0, 34.0])))
            .unwrap();
        y(&mut k, 4);
        k.cover_bottom(k.arena.key(k.arena.slot_of(2).unwrap()))
    };
    assert_eq!((bottom(false), bottom(true)), (34.0, 34.0));
}

#[test]
fn giving_or_taking_a_block_moves_a_covered_boxs_bottom() {
    // 2 is covered, and its last child 3 holds a scroller 4 the platform insets at its bottom:
    // 4 takes 2's bottom clearance while 3 is laid out in 2, and gives it back while 3 is an
    // owner in a block of its own, in either order of the block's coming and going. With 3
    // itself the inset scroller, an owner's root keeps it too
    for inner_scroller in [false, true] {
        let mut k = Kernel::with_monospace();
        k.set_owner_layout(true);
        let mut ops = create(1..=4);
        ops.push(style(1, &[(StyleId::Height, "844")]));
        let scroll = [(StyleId::OverflowY, "scroll"), (StyleId::Height, "100")];
        ops.push(style(if inner_scroller { 3 } else { 4 }, &scroll));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
        let inset = HostCover::Inset([0.0, 34.0]);
        k.set_host_cover(if inner_scroller { 3 } else { 4 }, Some(inset))
            .unwrap();
        y(&mut k, 4);
        assert_eq!(laid_padding(&k, 2)[2], 0.0, "{inner_scroller}");
        k.set_host_block(3, Some(block())).unwrap();
        y(&mut k, 4);
        assert_eq!(laid_padding(&k, 2)[2], 34.0, "{inner_scroller}");
        k.set_host_block(3, None).unwrap();
        y(&mut k, 4);
        assert_eq!(laid_padding(&k, 2)[2], 0.0, "{inner_scroller}");
    }
}

#[test]
fn an_owners_ignored_cover_leaves_its_padding_alone() {
    // A checkbox's native appearance takes no padding, whatever it authors; the owner's
    // cover, which pads nothing under the owner layout, must not bring the authored 10 back
    let padding = |cover: Option<HostCover>| {
        let mut k = document(true, &[(TOP, "10")], &[]);
        let control = [
            Op::CreateView {
                id: 5,
                node_type: crate::NodeType::Control,
            },
            Op::SetProp {
                id: 5,
                prop: crate::PropId::Type,
                value: "checkbox".into(),
            },
            style(5, &[(TOP, "10")]),
            children(1, &[2, 5]),
        ];
        k.apply(0, 2, &control).unwrap();
        k.set_host_block(5, Some(block())).unwrap();
        k.set_owner_band(5, Some(BAND), &[]).unwrap();
        k.set_host_cover(5, cover).unwrap();
        y(&mut k, 5);
        laid_padding(&k, 5)
    };
    let edges = Some(HostCover::Edges(BAND));
    assert_eq!(padding(edges), padding(None));
}

#[test]
fn a_box_that_becomes_an_owner_in_the_same_block_moves_a_covered_boxs_bottom() {
    // 3 is a dialog the host measured, in the block the kernel implies for it, holding the
    // scroller 4 the platform insets at its bottom: given that same block, 3 becomes an owner
    // and 2's bottom clearance comes back; released, 4 takes it again
    let mut k = Kernel::with_monospace();
    k.set_owner_layout(true);
    let mut ops = create(1..=4);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    let scroll = [(StyleId::OverflowY, "scroll"), (StyleId::Height, "100")];
    ops.push(style(4, &scroll));
    ops.push(Op::SetProp {
        id: 3,
        prop: crate::PropId::SemanticTag,
        value: "dialog".into(),
    });
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
    k.set_host_cover(4, Some(HostCover::Inset([0.0, 34.0])))
        .unwrap();
    assert!(k.measure_dialog(3, true));
    y(&mut k, 4);
    let implied = k.host_block(3).unwrap();
    let at = |k: &mut Kernel| (y(k, 4), laid_padding(k, 2)[2]);
    let free = at(&mut k);
    k.set_host_block(3, Some(implied)).unwrap();
    assert_eq!(at(&mut k).1, 34.0);
    k.set_host_block(3, None).unwrap();
    assert_eq!(at(&mut k), free);
    assert_eq!(free.1, 0.0);
}

#[test]
fn an_owner_beside_a_scroller_leaves_it_the_covered_boxs_bottom() {
    // 2, covered, holds the scroller 3 the platform insets at its bottom and the owner 4, laid
    // out in its own block, out of 2's flow: in either order 3 is 2's last box in flow and
    // takes its bottom clearance
    for owner_last in [true, false] {
        let mut k = Kernel::with_monospace();
        k.set_owner_layout(true);
        let mut ops = create(1..=4);
        ops.push(style(1, &[(StyleId::Height, "844")]));
        let scroll = [(StyleId::OverflowY, "scroll"), (StyleId::Height, "100")];
        ops.push(style(3, &scroll));
        let order: &[ViewId] = if owner_last { &[3, 4] } else { &[4, 3] };
        ops.extend([children(1, &[2]), children(2, order)]);
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
        k.set_host_block(4, Some(block())).unwrap();
        k.set_host_cover(3, Some(HostCover::Inset([0.0, 34.0])))
            .unwrap();
        y(&mut k, 3);
        assert_eq!(laid_padding(&k, 2)[2], 0.0, "owner last {owner_last}");
    }
}

#[test]
fn turning_it_on_or_off_lays_out_as_a_kernel_made_so() {
    // An owner's root under a cover and a band, holding a covered box over an inset scroller
    let frames = |k: &mut Kernel| [2, 3, 4].map(|id| (y(k, id), laid_padding(k, id)));
    let made = |on| {
        let mut k = document(on, &[], &[]);
        report(&mut k);
        frames(&mut k)
    };
    let mut k = document(false, &[], &[]);
    report(&mut k);
    assert!(!k.owner_layout());
    assert_eq!(frames(&mut k), made(false));
    k.set_owner_layout(true);
    assert!(k.owner_layout());
    assert_eq!(frames(&mut k), made(true));
    k.set_owner_layout(false);
    assert_eq!(frames(&mut k), made(false));
    assert_ne!(made(true), made(false));
    // A reset keeps it, as it keeps the session's other settings
    k.set_owner_layout(true);
    k.reset();
    assert!(k.owner_layout());
    build(&mut k, &[], &[]);
    report(&mut k);
    assert_eq!(frames(&mut k), made(true));
}

#[test]
fn the_name_keeps_mains_meaning_until_stage_5_lowers_it() {
    // `var(--exact-safe-area-inset-top)` is the owner's local inset either way; stage 5's parser
    // makes it the box's own observation
    for on in [false, true] {
        let mut k = document(on, &[], &[(TOP, "var(--exact-safe-area-inset-top)")]);
        k.set_local_insets(2, Some([30.0, 0.0, 0.0, 0.0])).unwrap();
        y(&mut k, 4);
        assert_eq!(top(&k, 3), 30.0, "on {on}");
    }
}

#[test]
fn a_content_region_lays_out_again_when_it_turns() {
    // 2 is a content region's owner, 3 its content holding the scroller 4 with a 50-point top
    // bar 5 and the row 6, and 7 its placeholder: off, the bar is in the scroller's flow and
    // the row below it; on, the bar is on the scrollport and the row at the scroller's top.
    // Turned either way live, the region publishes what a kernel made so publishes
    let made = |on: bool| {
        let mut k = Kernel::with_monospace();
        k.set_owner_layout(on);
        let mut ops = create(1..=7);
        ops.push(style(1, &[(StyleId::Height, "844")]));
        ops.push(style(
            2,
            &[
                (StyleId::PositionType, "relative"),
                (StyleId::Width, "100%"),
                (StyleId::Height, "180"),
                (StyleId::OverflowX, "hidden"),
                (StyleId::OverflowY, "hidden"),
            ],
        ));
        let scroll = [(StyleId::OverflowY, "scroll"), (StyleId::Height, "150")];
        ops.push(style(4, &scroll));
        ops.push(style(
            5,
            &[(StyleId::SafeAreaBar, "top"), (StyleId::Height, "50")],
        ));
        ops.push(style(6, &[(StyleId::Height, "20")]));
        ops.extend([
            children(1, &[2]),
            children(2, &[3, 7]),
            children(3, &[4]),
            children(4, &[5, 6]),
        ]);
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        k
    };
    let row = |k: &mut Kernel| {
        let key = |k: &Kernel, id| k.arena.key(k.arena.slot_of(id).unwrap());
        let binding = crate::ContentRegion {
            owner: key(k, 2),
            content: key(k, 3),
            pending: key(k, 7),
        };
        k.set_content_region_profile(Some(binding), Default::default())
            .unwrap();
        let inputs = crate::RegionInputs {
            catalog: 1,
            consumer_revision: 1,
        };
        let receipt = k
            .compute_region_layout(1, Offer::definite(390.0, 300.0), inputs)
            .unwrap();
        receipt.current_frame(key(k, 6)).map(|f| f.y)
    };
    let (off, on) = (row(&mut made(false)), row(&mut made(true)));
    assert_eq!((off, on), (Some(50.0), Some(0.0)));
    for start in [false, true] {
        let mut k = made(start);
        row(&mut k);
        k.set_owner_layout(!start);
        assert_eq!(row(&mut k), if start { off } else { on }, "from {start}");
    }
}

#[test]
fn an_owners_cover_pads_again_once_it_is_no_owner() {
    // The kernel keeps an owner's ignored cover: given back its block, the box is no owner
    // and the cover pads it again
    let mut k = document(true, &[], &[]);
    k.set_host_cover(2, Some(HostCover::Edges(BAND))).unwrap();
    assert_eq!(y(&mut k, 3), 0.0);
    k.set_host_block(2, None).unwrap();
    assert_eq!(y(&mut k, 3), 47.0);
}

#[test]
fn a_content_region_measures_again_when_a_box_becomes_an_owner() {
    // In the content region of 2, the content 3 is covered 34 at its bottom and holds the row 4
    // and the dialog 5 the host measured, which holds the scroller 6 the platform insets at
    // its bottom: while 5 is laid out in 3, 6 takes 3's bottom clearance and 3 is the row's 20
    // tall; given the block the kernel implied for it, 5 is an owner, 3 pads its 34 and is 54;
    // given it back, 20 again. The region publishes each without new inputs
    let mut k = Kernel::with_monospace();
    k.set_owner_layout(true);
    let mut ops = create(1..=7);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(
        2,
        &[
            (StyleId::PositionType, "relative"),
            (StyleId::Width, "100%"),
            (StyleId::Height, "300"),
            (StyleId::OverflowX, "hidden"),
            (StyleId::OverflowY, "hidden"),
        ],
    ));
    ops.push(style(4, &[(StyleId::Height, "20")]));
    let scroll = [(StyleId::OverflowY, "scroll"), (StyleId::Height, "100")];
    ops.push(style(6, &scroll));
    ops.push(Op::SetProp {
        id: 5,
        prop: crate::PropId::SemanticTag,
        value: "dialog".into(),
    });
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 7]),
        children(3, &[4, 5]),
        children(5, &[6]),
    ]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_host_cover(3, Some(HostCover::Edges([0.0, 0.0, 34.0, 0.0])))
        .unwrap();
    k.set_host_cover(6, Some(HostCover::Inset([0.0, 34.0])))
        .unwrap();
    assert!(k.measure_dialog(5, true));
    let key = |k: &Kernel, id| k.arena.key(k.arena.slot_of(id).unwrap());
    let binding = crate::ContentRegion {
        owner: key(&k, 2),
        content: key(&k, 3),
        pending: key(&k, 7),
    };
    k.set_content_region_profile(Some(binding), Default::default())
        .unwrap();
    let height = |k: &mut Kernel| {
        let inputs = crate::RegionInputs {
            catalog: 1,
            consumer_revision: 1,
        };
        let receipt = k
            .compute_region_layout(1, Offer::definite(390.0, 844.0), inputs)
            .unwrap();
        receipt.current_frame(key(k, 3)).map(|f| f.height)
    };
    assert_eq!(height(&mut k), Some(20.0));
    let implied = k.host_block(5).unwrap();
    k.set_host_block(5, Some(implied)).unwrap();
    assert_eq!(height(&mut k), Some(54.0));
    k.set_host_block(5, None).unwrap();
    assert_eq!(height(&mut k), Some(20.0));
}

#[test]
fn a_bar_that_gains_or_gives_back_a_block_leaves_its_scroller_as_it_would_be() {
    // The scroller 3 pads its top by 20 of safe-area padding and holds the top bar 4 and the
    // sticky row 5: a box with a block of its own is no bar, so given a block, and given it back, the
    // scroller lays its row and its extent out as a kernel made so does
    let made = |given: bool| {
        let mut k = Kernel::with_monospace();
        k.set_owner_layout(true);
        let mut ops = create(1..=5);
        ops.push(style(1, &[(StyleId::Height, "844")]));
        let scroll = [
            (StyleId::OverflowY, "scroll"),
            (StyleId::Height, "200"),
            (StyleId::SafeAreaPaddingTop, "20"),
        ];
        ops.push(style(3, &scroll));
        ops.push(style(
            4,
            &[(StyleId::SafeAreaBar, "top"), (StyleId::Height, "30")],
        ));
        let sticky = [
            (StyleId::PositionType, "sticky"),
            (StyleId::Top, "0"),
            (StyleId::Height, "20"),
        ];
        ops.push(style(5, &sticky));
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        if given {
            k.set_host_block(4, Some(block())).unwrap();
        }
        k
    };
    let at = |k: &mut Kernel| {
        let row = y(k, 5);
        (
            row,
            k.scrollport_bars(3).len(),
            k.node(3).unwrap().content.1,
        )
    };
    // On the scrollport the bar takes the reservation, and the row is at the content's top; with
    // a block the box is no bar, and the scroller pads its content by the 20
    let (free, held) = (at(&mut made(false)), at(&mut made(true)));
    assert_eq!((free, held), ((0.0, 1, 20.0), (20.0, 0, 40.0)));
    let mut k = made(false);
    at(&mut k);
    k.set_host_block(4, Some(block())).unwrap();
    assert_eq!(at(&mut k), held, "given");
    k.set_host_block(4, None).unwrap();
    assert_eq!(at(&mut k), free, "given back");
}
