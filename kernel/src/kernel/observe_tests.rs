//! Observations of the safe area per box and their solve (LLP 1123 D9),
//! driven through the kernel's leaf before lowering produces it.
use super::*;
use crate::generated::StyleProps;
use crate::kernel::{BlockAxis, HostBlock};
use crate::style::calc::{self, Node, Observe, Unit};
use crate::style::targets::Target;
use crate::style::Edge;
use crate::wire::Op;
use crate::{NodeType, StyleId, StyleValue};

/// The observation of the box's own safe area on `edge`, `fallback` where
/// it is unavailable.
fn observed(edge: Edge, fallback: Option<f64>) -> Node {
    observe(Target::Box, edge, fallback)
}

/// `target`'s observation on `edge`, `fallback` where it is unavailable.
fn observe(target: Target, edge: Edge, fallback: Option<f64>) -> Node {
    Node::Observe(Observe {
        target,
        edge,
        fallback: fallback.map(|px| Box::new(Node::Value(px, Unit::Px))),
    })
}

fn px(v: f64) -> Node {
    Node::Value(v, Unit::Px)
}

fn number(v: f64) -> Node {
    Node::Value(v, Unit::Number)
}

/// `id`'s rows: each tree as a length, each text as written.
fn style(id: ViewId, trees: Vec<(StyleId, Node)>, texts: &[(StyleId, &str)]) -> Op {
    let mut patch = StyleProps::default();
    for (row, text) in texts {
        let value = match text.parse::<f64>() {
            Ok(n) => StyleValue::Number(n),
            Err(_) if *text == "auto" => StyleValue::Auto,
            Err(_) => StyleValue::Text((*text).into()),
        };
        patch.set_dynamic(*row, &value).unwrap();
    }
    for (row, tree) in trees {
        let d = calc::dimension(tree);
        match row {
            StyleId::PaddingTop => patch.padding_top = d,
            StyleId::PaddingLeft => patch.padding_left = d,
            StyleId::MarginTop => patch.margin_top = d,
            StyleId::MarginLeft => patch.margin_left = d,
            StyleId::Width => patch.width = d,
            StyleId::Height => patch.height = d,
            StyleId::AdditionalSafeAreaTop => patch.rare.additional_safe_area_top = d,
            StyleId::SafeAreaPaddingTop => patch.rare.safe_area_padding_top = d,
            other => panic!("no field for {other:?}"),
        }
        patch.mask.set(row);
    }
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

fn create(ids: impl IntoIterator<Item = ViewId>) -> Vec<Op> {
    (ids.into_iter())
        .map(|id| Op::CreateView {
            id,
            node_type: NodeType::View,
        })
        .collect()
}

fn children(id: ViewId, children: &[ViewId]) -> Op {
    Op::SetChildren {
        id,
        children: children.to_vec(),
    }
}

/// Make `id` a popover, a document top-layer box.
fn popover(id: ViewId) -> Op {
    Op::SetProp {
        id,
        prop: crate::PropId::Popover,
        value: "auto".into(),
    }
}

fn block(width: BlockAxis, height: BlockAxis) -> HostBlock {
    HostBlock {
        tactic: Default::default(),
        width,
        height,
    }
}

/// The owner 2 as a 100-point flex column that centres what it holds.
fn centring() -> Op {
    style(
        2,
        vec![],
        &[
            (StyleId::Display, "flex"),
            (StyleId::FlexDirection, "column"),
            (StyleId::JustifyContent, "center"),
            (StyleId::Height, "100"),
        ],
    )
}

/// A document (1) holding an owner (2) with a block of `width` by
/// `height`, whose root takes the container region so that its boxes lie
/// under its band (LLP 1123 D3, D4), and `ops` for what it holds.
fn owner(width: f32, height: f32, mut ops: Vec<Op>) -> Kernel {
    let mut k = Kernel::with_monospace();
    ops.splice(0..0, create(1..=2).into_iter().chain([takes_container(2)]));
    ops.push(style(1, vec![], &[(StyleId::Height, "844")]));
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    let block = block(BlockAxis::Is(width), BlockAxis::Is(height));
    k.set_host_block(2, Some(block)).unwrap();
    k
}

/// `id` taking the container region on every edge, as a content owner's
/// root that lies under its band (LLP 1123 D4).
fn takes_container(id: ViewId) -> Op {
    style(id, vec![], &[(StyleId::SafeAreaIgnore, "container")])
}

fn lay_out(k: &mut Kernel) {
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
}

fn top(k: &Kernel, id: ViewId) -> f32 {
    k.resolved_padding(k.arena.key(slot(k, id))).unwrap().1
}

fn left(k: &Kernel, id: ViewId) -> f32 {
    k.resolved_padding(k.arena.key(slot(k, id))).unwrap().0
}

fn slot(k: &Kernel, id: ViewId) -> u32 {
    k.arena.slot_of(id).unwrap()
}

/// `id`'s frame: x, y, width and height in its layout's space.
fn frame(k: &Kernel, id: ViewId) -> (f32, f32, f32, f32) {
    let f = k.node(id).unwrap().frame;
    (f.x, f.y, f.width, f.height)
}

#[test]
fn a_box_and_its_child_each_observe_what_reaches_them() {
    // D9's first worked example: 2, the owner's root, takes a 50-point top
    // band and pads by its observation; 3, in flow in it, pads by its own
    let mut ops = create(3..=3);
    ops.push(style(
        2,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &[],
    ));
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([50.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 2), 50.0);
    assert_eq!(top(&k, 3), 0.0, "3 starts where the band ends");
    assert_eq!(k.observation(3), Some([Some(0.0); 4]));
    assert!(k.safe_area_unsettled().is_empty());
}

#[test]
fn a_box_whose_observation_outgrows_it_reaches_the_bands_depth() {
    // 3, 10 tall and centred in a 100-point block under a 46-point band,
    // pads by 2.2 times its overlap: each round's overlap is 1 + 1.1 times
    // the last, steeper than the line, so its one fixed point is the band's
    // depth, which a step past the line's root below 0 lands on
    let padding = Node::Product(vec![number(2.2), observed(Edge::Top, None)]);
    let mut ops = create(3..=3);
    ops.push(centring());
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, padding)],
        &[(StyleId::Height, "10"), (StyleId::FlexShrink, "0")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 100.0, ops);
    k.set_owner_band(2, Some([46.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(3).unwrap()[0], Some(46.0));
    assert!(
        k.arena.observations.rounds <= 3,
        "{}",
        k.arena.observations.rounds
    );
    assert!(k.safe_area_unsettled().is_empty());
}

#[test]
fn a_reader_moved_into_a_scroller_observes_none() {
    // 4, and 6 in 5, sit at 50 under 2's bottom band; 3 scrolls; 4 and 5
    // move into 3, which positions neither, so the readers' frames stay
    let mut ops = create(3..=6);
    ops.push(style(
        3,
        vec![],
        &[(StyleId::OverflowY, "scroll"), (StyleId::Height, "10")],
    ));
    ops.extend([fixed_reader(4, "50"), fixed_reader(6, "50")]);
    ops.push(style(2, vec![], &[(StyleId::Height, "100")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 4, 5]),
        children(5, &[6]),
    ]);
    let mut k = owner(390.0, 100.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 30.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    for id in [4, 6] {
        assert_eq!(k.observation(id).unwrap()[2], Some(20.0), "{id}");
    }
    k.apply(0, 2, &[children(2, &[3]), children(3, &[4, 5])])
        .unwrap();
    lay_out(&mut k);
    for id in [4, 6] {
        assert_eq!(k.observation(id).unwrap()[2], Some(0.0), "{id}");
    }
}

#[test]
fn a_reader_follows_its_owners_block_narrowing_past_its_root() {
    // The owner 2's root is 100 points wide in a 390-point block under a
    // 30-point right band, holding the 100-point reader 3; the block
    // narrows to 120, which leaves the root's frame, and the band's inner
    // side comes to 90
    let mut ops = create(3..=3);
    ops.push(style(2, vec![], &[(StyleId::Width, "100")]));
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Right, None))];
    let rows = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Width, "100"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(3, pad, &rows));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 30.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(3).unwrap()[1], Some(0.0));
    let block = block(BlockAxis::Is(120.0), BlockAxis::Is(844.0));
    k.set_host_block(2, Some(block)).unwrap();
    lay_out(&mut k);
    assert_eq!(k.arena.frame(slot(&k, 2)).width, 100.0);
    assert_eq!(k.observation(3).unwrap()[1], Some(10.0));
}

#[test]
fn a_move_elsewhere_keeps_a_readers_readout() {
    // 5 moves from 6 to 7, apart from the reader 3
    let mut ops = create(3..=7);
    ops.push(fixed_reader(3, "50"));
    ops.extend([
        children(1, &[2, 6, 7]),
        children(2, &[3]),
        children(6, &[5]),
    ]);
    let mut k = owner(390.0, 100.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 30.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    k.apply(0, 2, &[children(6, &[]), children(7, &[5])])
        .unwrap();
    assert!(k.arena.observations.readouts.contains_key(&slot(&k, 3)));
    lay_out(&mut k);
    assert_eq!(k.observation(3).unwrap()[2], Some(20.0));
}

#[test]
fn a_centred_box_reaches_its_fixed_point_by_the_secant_step() {
    // D9's second worked example: 3, 10 tall and centred in a 100-point
    // block under a 46-point band, pads by 1.9 times its own overlap, so
    // each round's overlap is 1 + 0.95 times the last: 0, 1, then 20
    let padding = Node::Product(vec![number(1.9), observed(Edge::Top, None)]);
    let mut ops = create(3..=3);
    ops.push(centring());
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, padding)],
        &[(StyleId::Height, "10"), (StyleId::FlexShrink, "0")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 100.0, ops);
    k.set_owner_band(2, Some([46.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    let observed = k.observation(3).unwrap()[0].unwrap();
    assert!((observed - 20.0).abs() <= 0.5, "{observed}");
    assert!((top(&k, 3) - 38.0).abs() <= 1.0, "{}", top(&k, 3));
    assert_eq!(k.arena.observations.rounds, 2, "at the third layout");
    assert!(k.safe_area_unsettled().is_empty());
    // Laid out again with nothing changed, it is settled: no layout is added
    lay_out(&mut k);
    assert_eq!(k.arena.observations.rounds, 0);
}

#[test]
fn a_reader_with_no_fixed_point_is_left_unsettled() {
    // 4 sits beside a 60-point box in a 100-point wrapping row, as wide as
    // 30 points plus twice its top overlap: past 5 it wraps below, where
    // the band does not reach, and back on the first line it overlaps 20
    let width = Node::Sum(vec![
        px(30.0),
        Node::Product(vec![number(2.0), observed(Edge::Top, None)]),
    ]);
    let mut ops = create(3..=4);
    ops.push(style(
        2,
        vec![],
        &[
            (StyleId::Display, "flex"),
            (StyleId::FlexWrap, "wrap"),
            (StyleId::Width, "100"),
        ],
    ));
    ops.push(style(
        3,
        vec![],
        &[
            (StyleId::Width, "60"),
            (StyleId::Height, "50"),
            (StyleId::FlexShrink, "0"),
        ],
    ));
    ops.push(style(
        4,
        vec![(StyleId::Width, width)],
        &[(StyleId::Height, "50"), (StyleId::FlexShrink, "0")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = owner(100.0, 844.0, ops);
    k.set_owner_band(2, Some([20.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.safe_area_unsettled(), vec![4]);
    let readout = k.observation(4).unwrap()[0].unwrap();
    assert!(
        readout == 0.0 || readout == 20.0,
        "a readout is a measurement: {readout}"
    );
    // Its frame is what the input it keeps gives, and it stays unsettled
    let input = k.arena.observations.input(slot(&k, 4))[0].unwrap();
    assert_eq!(k.arena.frame(slot(&k, 4)).width, 30.0 + 2.0 * input);
    lay_out(&mut k);
    assert_eq!(k.safe_area_unsettled(), vec![4], "still unsettled");
}

/// The top and left cases are those of a UIKit probe on iOS 27.1 (iPhone
/// Duo, 2026-11-16) under a 40-point top and a 20-point left band, each the
/// `safeAreaInsets` UIKit gave a view at that frame; the bottom ones apply
/// the same rule, which the probe of a scroll view's subviews showed for a
/// 34-point bottom band: the distance from the view's edge to the band's
/// inner side, at most the band's depth, however small the view.
#[test]
fn a_box_observes_what_uikit_gives_a_view() {
    // In a 390 by 100 block under bands of 40 on top, 30 at the bottom and
    // 20 on the left, each box reads its top into its left padding, which
    // moves none of its other edges
    let cases: [(ViewId, [&str; 4], [f32; 4]); 7] = [
        // x, y, width, height; then top, right, bottom, left observed
        (3, ["0", "0", "100", "10"], [40.0, 0.0, 0.0, 20.0]),
        (4, ["0", "30", "100", "10"], [10.0, 0.0, 0.0, 20.0]),
        (5, ["0", "20", "100", "100"], [20.0, 0.0, 30.0, 20.0]),
        (6, ["0", "45", "100", "10"], [0.0, 0.0, 0.0, 20.0]),
        (7, ["0", "60", "5", "10"], [0.0, 0.0, 0.0, 20.0]),
        (8, ["30", "50", "100", "40"], [0.0, 0.0, 20.0, 0.0]),
        // Wholly past the block's bottom: the band's whole depth
        (9, ["30", "120", "100", "40"], [0.0, 0.0, 30.0, 0.0]),
    ];
    let mut ops = create(3..=9);
    for (id, [x, y, w, h], _) in cases {
        let rows = [
            (StyleId::PositionType, "absolute"),
            (StyleId::Left, x),
            (StyleId::Top, y),
            (StyleId::Width, w),
            (StyleId::Height, h),
        ];
        ops.push(style(
            id,
            vec![(StyleId::PaddingLeft, observed(Edge::Top, None))],
            &rows,
        ));
    }
    ops.push(style(2, vec![], &[(StyleId::Height, "100")]));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5, 6, 7, 8, 9])]);
    let mut k = owner(390.0, 100.0, ops);
    k.set_owner_band(2, Some([40.0, 0.0, 30.0, 20.0]), &[])
        .unwrap();
    lay_out(&mut k);
    for (id, _, want) in cases {
        assert_eq!(k.observation(id), Some(want.map(Some)), "box {id}");
    }
}

/// A reader at the block's top, 40 tall and 100 wide, that reads its
/// bottom observation into its left padding, so its frame stays put.
fn fixed_reader(id: ViewId, top: &'static str) -> Op {
    style(
        id,
        vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))],
        &[
            (StyleId::PositionType, "absolute"),
            (StyleId::Top, top),
            (StyleId::Width, "100"),
            (StyleId::Height, "40"),
        ],
    )
}

#[test]
fn a_reader_follows_its_owner_growing_around_it() {
    // 2's block fits its content, up to 500: 3 fills 100 of it; 4 sits at
    // 50 under a 30-point bottom band, then 3 grows and takes the band away
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::Height, "100")]));
    ops.push(fixed_reader(4, "50"));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::AtMost(500.0));
    k.set_host_block(2, Some(block)).unwrap();
    k.set_owner_band(2, Some([0.0, 0.0, 30.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(4).unwrap()[2], Some(20.0));
    k.apply(0, 2, &[style(3, vec![], &[(StyleId::Height, "200")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(
        k.observation(4).unwrap()[2],
        Some(0.0),
        "4 stays at 50 as the band moves to 170"
    );
}

#[test]
fn a_measure_solves_the_readers_under_its_block() {
    // 2 fits its width to 3 and pads its left by what it observes at the
    // bottom: nothing in a 200-point block, 50 in a 100-point one, where
    // its bottom lies under the 50-point band
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))];
    let mut ops = create(3..=3);
    ops.push(style(2, pad, &[(StyleId::Height, "100")]));
    ops.push(style(
        3,
        vec![],
        &[(StyleId::Width, "100"), (StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 200.0, ops);
    let fit = |height| block(BlockAxis::Fit(390.0), BlockAxis::Is(height));
    k.set_host_block(2, Some(fit(200.0))).unwrap();
    k.set_owner_band(2, Some([0.0, 0.0, 50.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some((100.0, 200.0)));
    let measured = k.measure_block(2, fit(100.0)).unwrap();
    assert_eq!(measured.used, (150.0, 100.0));
    // The trial leaves the kernel's reader as it was, and the layout of the
    // block it measured gives what it answered
    assert_eq!(
        (left(&k, 2), k.observation(2).unwrap()[2]),
        (0.0, Some(0.0))
    );
    k.set_host_block(2, Some(fit(100.0))).unwrap();
    lay_out(&mut k);
    assert_eq!(k.host_block_size(2), Some(measured.used));
    // A band set since the last layout leaves the readers it edges to the
    // next, which a measure waits for
    k.set_owner_band(2, Some([0.0, 0.0, 20.0, 0.0]), &[])
        .unwrap();
    assert!(k.measure_block(2, fit(100.0)).is_err());
    lay_out(&mut k);
    assert_eq!(k.measure_block(2, fit(100.0)).unwrap().used, (120.0, 100.0));
}

#[test]
fn a_measure_gives_no_text_revision_a_later_one_gives() {
    // The popover 3 holds the paragraph 4, whose run 5 pads its left by its
    // top observation: the trial of a block its host gives, which the host
    // places, makes the band unavailable and the fallback apply, a restyle
    // that revises the paragraph
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut k = Kernel::new(Box::new(crate::kernel::measure_block_tests::Stamps(
        seen.clone(),
    )));
    let mut ops = create([1, 3]);
    for id in [4, 5] {
        ops.push(Op::CreateView {
            id,
            node_type: NodeType::Text,
        });
    }
    let text = |value: &str| Op::SetProp {
        id: 5,
        prop: crate::PropId::Text,
        value: value.into(),
    };
    ops.push(text("x"));
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, Some(7.0)))];
    ops.extend([style(5, pad, &[]), popover(3)]);
    ops.extend([children(1, &[3]), children(3, &[4]), children(4, &[5])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band([40.0, 0.0, 0.0, 0.0]).unwrap();
    lay_out(&mut k);
    seen.borrow_mut().clear();
    let given = block(BlockAxis::Is(100.0), BlockAxis::Is(100.0));
    k.measure_block(3, given).unwrap();
    let trial = std::mem::take(&mut *seen.borrow_mut());
    assert!(!trial.is_empty(), "the trial measured the paragraph anew");
    // The next edit's revision is not one the trial gave, which a measurer
    // holding the trial's answer would take for its text
    k.apply(0, 2, &[text("xx")]).unwrap();
    lay_out(&mut k);
    let edited = k.node(4).unwrap().paragraph_stamp().unwrap();
    assert!(!trial.contains(&edited), "{edited:?} {trial:?}");
}

#[test]
fn a_reader_outside_every_owner_follows_the_viewport() {
    // 2, outside every owner, sits at 760 under the session's 34-point
    // bottom band; a taller viewport takes the band away from it
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=2);
    ops.push(style(1, vec![], &[(StyleId::Height, "844")]));
    ops.push(fixed_reader(2, "780"));
    ops.extend([children(1, &[2]), Op::AttachRoot { id: 1 }]);
    k.apply(0, 1, &ops).unwrap();
    assert!(k.set_session_band([0.0, 0.0, 34.0, 0.0]).unwrap());
    lay_out(&mut k);
    assert_eq!(k.observation(2).unwrap()[2], Some(10.0), "820 less 810");
    k.compute_layout(1, Offer::definite(390.0, 900.0)).unwrap();
    assert_eq!(
        k.observation(2).unwrap()[2],
        Some(0.0),
        "the band now starts at 866"
    );
}

#[test]
fn an_unavailable_band_makes_the_fallback_apply() {
    let mut ops = create(3..=3);
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, observed(Edge::Top, Some(7.0)))],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, None, &[]).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 7.0);
    assert_eq!(k.observation(3), Some([None; 4]));
    k.set_owner_band(2, Some([30.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 30.0);
    // A change solves from the input 3 has, as UIKit lays a view out from
    // where it is: what it observes does not depend on its own padding, so
    // one round settles it
    k.set_owner_band(2, Some([40.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 40.0);
    assert_eq!(k.arena.observations.rounds, 1);
}

#[test]
fn the_wire_refuses_the_leaf_until_the_parser_reads_it() {
    // Its text, `var(--exact-safe-area-inset-top, 3px)`, parses to the local
    // inset, and a target's to a custom property
    for target in [Target::Box, Target::Chain(2)] {
        let leaf = observe(target, Edge::Top, Some(3.0));
        let c = crate::style::calc::Comparison::intern(leaf);
        let mut w = crate::wire::codec::Writer::new();
        c.encode(&mut w);
        let bytes = w.into_vec();
        let mut r = crate::wire::codec::Reader::new(&bytes);
        assert!(crate::style::calc::Comparison::decode(&mut r, StyleId::PaddingTop).is_err());
    }
}

#[test]
fn a_reader_follows_its_owner_shrinking_while_it_solves() {
    // 2's block fits its content: 3, in flow, is 100 tall less its own top
    // observation, so 2 shrinks as 3 solves; 4, at 20 and 40 tall, reads
    // its bottom observation under a 30-point band
    let height = Node::Sum(vec![
        px(100.0),
        Node::Negate(Box::new(observed(Edge::Top, None))),
    ]);
    let mut ops = create(3..=4);
    ops.push(style(3, vec![(StyleId::Height, height)], &[]));
    ops.push(fixed_reader(4, "20"));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::AtMost(500.0));
    k.set_host_block(2, Some(block)).unwrap();
    k.set_owner_band(2, Some([20.0, 0.0, 30.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    // 3 observes 20 and is 80 tall; 2 fits it, 80; its band starts at 50
    assert_eq!(k.observation(4).unwrap()[2], Some(10.0), "60 less 50");
}

#[test]
fn an_owner_root_under_a_scroller_observes_its_own_band() {
    // 3 scrolls and holds the owner 2, whose root pads by its top
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=3);
    ops.push(style(1, vec![], &[(StyleId::Height, "844")]));
    ops.push(style(
        3,
        vec![],
        &[(StyleId::OverflowY, "scroll"), (StyleId::Height, "400")],
    ));
    ops.push(style(
        2,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &[],
    ));
    ops.extend([
        takes_container(2),
        children(1, &[3]),
        children(3, &[2]),
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(844.0));
    k.set_host_block(2, Some(block)).unwrap();
    k.set_owner_band(2, Some([50.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 2), 50.0, "the block cuts the scroller off");
}

#[test]
fn a_popover_observes_the_session_band_where_it_is() {
    // The popover 3 is 10 points down by its margin under the session's
    // 40-point band, which it overlaps by 30; its inset from the viewport's
    // top grows from 0 to 30, which moves it in the viewport without
    // changing its frame in its block
    let mut ops = create(3..=3);
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    let rows = [
        (StyleId::Top, "0"),
        (StyleId::MarginTop, "10"),
        (StyleId::Width, "50"),
        (StyleId::Height, "10"),
    ];
    ops.extend([style(3, pad, &rows), popover(3)]);
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_session_band([40.0, 0.0, 0.0, 0.0]).unwrap();
    lay_out(&mut k);
    assert_eq!(k.top_layer_origin(3, (390.0, 844.0)).unwrap().1, 10.0);
    assert_eq!(k.observation(3).unwrap()[0], Some(30.0), "its margin once");
    k.apply(0, 2, &[style(3, vec![], &[(StyleId::Top, "30")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.top_layer_origin(3, (390.0, 844.0)).unwrap().1, 40.0);
    assert_eq!(k.observation(3).unwrap()[0], Some(0.0));
}

#[test]
fn a_box_that_joins_the_top_layer_observes_the_session_band() {
    // 3, at the top of the owner 2, which fills the viewport under a
    // 100-point band, becomes a popover laid out where it was, under the
    // session's 20-point band
    let mut ops = create(3..=3);
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    ops.push(style(
        3,
        pad,
        &[(StyleId::Width, "50"), (StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([100.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    k.set_session_band([20.0, 0.0, 0.0, 0.0]).unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(3).unwrap()[0], Some(100.0));
    k.apply(0, 2, &[popover(3)]).unwrap();
    lay_out(&mut k);
    assert!(k.is_top_layer(3));
    assert_eq!(k.observation(3).unwrap()[0], Some(20.0));
}

#[test]
fn content_that_a_box_only_clips_observes_its_band() {
    // 3 clips on both axes, as a plain view the host clips, and holds 4 at
    // its top under a 40-point band; beside a scrolling axis, the clip
    // computes to a scroller's
    let mut ops = create(3..=4);
    let clips = [
        (StyleId::OverflowX, "hidden"),
        (StyleId::OverflowY, "hidden"),
        (StyleId::Height, "100"),
    ];
    ops.push(style(3, vec![], &clips));
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, None))];
    ops.push(style(4, pad, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([40.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(4).unwrap()[0], Some(40.0));
    k.apply(
        0,
        2,
        &[style(3, vec![], &[(StyleId::OverflowX, "visible")])],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(
        k.observation(4).unwrap()[0],
        Some(0.0),
        "visible beside hidden scrolls"
    );
}

#[test]
fn content_observes_none_while_a_box_around_it_scrolls() {
    // 3 is a scroll view by its type, holding 4 under a 40-point top band;
    // an explicit overflow-y: visible stops it scrolling, with no row's value
    // changed but the declaration's, and overflow-y: scroll starts it again
    let mut ops = vec![Op::CreateView {
        id: 3,
        node_type: NodeType::ScrollView,
    }];
    ops.extend(create(4..=4));
    ops.push(style(3, vec![], &[(StyleId::Height, "400")]));
    ops.push(style(
        4,
        vec![(StyleId::PaddingLeft, observed(Edge::Top, None))],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([40.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(
        k.observation(4).unwrap()[0],
        Some(0.0),
        "the scroller takes the band into its content inset"
    );
    for (batch, overflow, want) in [(2, "visible", 40.0), (3, "scroll", 0.0)] {
        let change = style(3, vec![], &[(StyleId::OverflowY, overflow)]);
        k.apply(0, batch, &[change]).unwrap();
        lay_out(&mut k);
        assert_eq!(k.observation(4).unwrap()[0], Some(want), "{overflow}");
    }
}

#[test]
fn a_reader_that_returns_starts_again_from_zero() {
    // 3 settles at 50 at the top under a 50-point band; it stops reading,
    // the last reader gone, and returns at the top with a margin of 50
    // less its observation: every input is a fixed point, and a new reader
    // starts at 0, so its margin is 50
    let mut ops = create(3..=3);
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([50.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 50.0);
    k.apply(0, 2, &[style(3, vec![(StyleId::PaddingTop, px(0.0))], &[])])
        .unwrap();
    lay_out(&mut k);
    let margin = Node::Sum(vec![
        px(50.0),
        Node::Negate(Box::new(observed(Edge::Top, None))),
    ]);
    k.apply(0, 3, &[style(3, vec![(StyleId::MarginTop, margin)], &[])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(3).unwrap()[0], Some(0.0));
    assert_eq!(k.arena.frame(slot(&k, 3)).y, 50.0);
}

#[test]
fn an_edge_that_settles_keeps_its_input_while_another_moves() {
    // 3, 10 tall and centred in 100 points under bands of 46 on top and 20
    // on the left, pads each of those edges by its observation there, on a
    // display of three pixels a point: the left settles at once at 20,
    // while the top, 1 plus half its input, takes rounds to reach 2
    let mut ops = create(3..=3);
    ops.push(centring());
    ops.push(style(
        3,
        vec![
            (StyleId::PaddingTop, observed(Edge::Top, None)),
            (StyleId::PaddingLeft, observed(Edge::Left, None)),
        ],
        &[(StyleId::Height, "10"), (StyleId::FlexShrink, "0")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 100.0, ops);
    k.set_display_scale(3.0).unwrap();
    k.apply(0, 2, &[]).unwrap();
    k.set_owner_band(2, Some([46.0, 0.0, 0.0, 20.0]), &[])
        .unwrap();
    lay_out(&mut k);
    let seen = k.observation(3).unwrap();
    assert!((seen[0].unwrap() - 2.0).abs() <= 0.2, "{seen:?}");
    assert_eq!(left(&k, 3), 20.0);
    assert!(k.safe_area_unsettled().is_empty());
}

#[test]
fn a_root_laid_out_later_solves_its_own_readers() {
    // Two documents: 1, and 5 holding 6 with a margin of 50 less twice its
    // top observation under the session's 40-point band. Laying 1 out first
    // measures nothing of 5's: 6 starts at 0, with its margin 50
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 5, 6]);
    ops.push(style(1, vec![], &[(StyleId::Height, "844")]));
    ops.push(style(5, vec![], &[(StyleId::Height, "844")]));
    let margin = Node::Sum(vec![
        px(50.0),
        Node::Product(vec![number(-2.0), observed(Edge::Top, None)]),
    ]);
    ops.push(style(
        6,
        vec![(StyleId::MarginTop, margin)],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([
        children(5, &[6]),
        Op::AttachRoot { id: 1 },
        Op::AttachRoot { id: 5 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band([40.0, 0.0, 0.0, 0.0]).unwrap();
    lay_out(&mut k);
    k.compute_layout(5, Offer::definite(390.0, 844.0)).unwrap();
    assert_eq!(k.observation(6).unwrap()[0], Some(0.0));
    assert_eq!(k.arena.frame(slot(&k, 6)).y, 50.0);
}

#[test]
fn a_band_change_on_an_edge_no_reader_reads_adds_no_layout() {
    // 3, at the owner's right edge, pads its top by its top observation
    let mut ops = create(3..=3);
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([20.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    k.set_owner_band(2, Some([20.0, 30.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.arena.observations.rounds, 0);
    assert_eq!(
        k.observation(3).unwrap(),
        [Some(20.0), Some(30.0), Some(0.0), Some(0.0)]
    );
}

#[test]
fn a_band_change_measures_only_its_owners_readers() {
    // Owners 2 and 5 side by side, each holding a reader at its top under
    // a 30-point band; 2's band changes
    let mut ops = create(3..=6);
    for (reader, id) in [(3, 2), (6, 5)] {
        let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
        ops.push(style(reader, pad, &[(StyleId::Height, "10")]));
        ops.push(children(id, &[reader]));
    }
    ops.extend([takes_container(5), children(1, &[2, 5])]);
    let mut k = owner(390.0, 400.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(400.0));
    k.set_host_block(5, Some(block)).unwrap();
    for id in [2, 5] {
        k.set_owner_band(id, Some([30.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
    }
    lay_out(&mut k);
    k.set_owner_band(2, Some([40.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    let kept = |k: &Kernel, id| k.arena.observations.readouts.contains_key(&slot(k, id));
    assert!(!kept(&k, 3) && kept(&k, 6));
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 40.0);
    assert_eq!(top(&k, 6), 30.0);
}

#[test]
fn a_reader_follows_its_box_gaining_and_giving_back_a_block() {
    // 3 holds the reader 4 at its top under 2's 30-point top band; 3
    // becomes an owner the size it had, with no band yet, so the reader's
    // frame stays and its band is unavailable; then 3 gives its block back
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::Height, "100")]));
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, Some(7.0)))];
    ops.push(style(4, pad, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([30.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 30.0);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(block)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 7.0, "the fallback");
    k.set_host_block(3, None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 30.0);
}

#[test]
fn a_reader_that_reads_a_new_edge_is_measured_again() {
    // 3, a fixed 100-point border box at the owner's origin under bands of
    // 20 on top and 30 on the left, reads its top observation; then its
    // left padding reads its left one, which changes no frame
    let mut ops = create(3..=3);
    let fixed = [
        (StyleId::PositionType, "absolute"),
        (StyleId::BoxSizing, "border-box"),
        (StyleId::Width, "100"),
        (StyleId::Height, "100"),
    ];
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &fixed,
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([20.0, 0.0, 0.0, 30.0]), &[])
        .unwrap();
    lay_out(&mut k);
    let both = vec![
        (StyleId::PaddingTop, observed(Edge::Top, None)),
        (StyleId::PaddingLeft, observed(Edge::Left, None)),
    ];
    k.apply(0, 2, &[style(3, both, &[])]).unwrap();
    lay_out(&mut k);
    assert_eq!(left(&k, 3), 30.0);
}

#[test]
fn a_popover_its_host_places_is_no_owner() {
    // The popover 3 reads its top observation with a 7-point fallback; its
    // host gives it a block of its own, as it does a painted popover with a
    // `position-area`, and shows it where the kernel is not told
    let mut ops = create(3..=3);
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, Some(7.0)))];
    ops.extend([style(3, pad, &[(StyleId::Height, "10")]), popover(3)]);
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([40.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    k.set_session_band([40.0, 0.0, 0.0, 0.0]).unwrap();
    let block = block(BlockAxis::Is(200.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(block)).unwrap();
    // A band sent for it is no owner's, which a top-layer box never is
    k.set_owner_band(3, Some([40.0, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert!(k.is_top_layer(3));
    assert_eq!(k.observation(3).unwrap(), [None; 4]);
    assert_eq!(top(&k, 3), 7.0, "the fallback");
    // Given back, the kernel places it in the viewport, at the band
    k.set_host_block(3, None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 40.0);
}

#[test]
fn a_display_scale_change_measures_every_reader_again() {
    // 3, 10 tall and centred in 100 points under a 45.4-point band, pads
    // its top by 1.9 times its top observation, which is 0.4 plus 0.95
    // times its input: at one pixel a point 0 is within half a pixel of
    // 0.4, and at three it is not, and the input goes to its fixed point, 8
    let mut ops = create(3..=3);
    ops.push(centring());
    let pad = Node::Product(vec![number(1.9), observed(Edge::Top, None)]);
    let rows = [(StyleId::Height, "10"), (StyleId::FlexShrink, "0")];
    ops.push(style(3, vec![(StyleId::PaddingTop, pad)], &rows));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 100.0, ops);
    k.set_owner_band(2, Some([45.4, 0.0, 0.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 0.0);
    k.set_display_scale(3.0).unwrap();
    k.apply(0, 2, &[]).unwrap();
    lay_out(&mut k);
    let seen = k.observation(3).unwrap()[0].unwrap();
    assert!((seen - 8.0).abs() <= 1.0 / 6.0, "{seen}");
}

/// A content region's owner: clipped, 180 tall and as wide as the document.
const REGION: [(StyleId, &str); 5] = [
    (StyleId::PositionType, "relative"),
    (StyleId::Width, "100%"),
    (StyleId::Height, "180"),
    (StyleId::OverflowX, "hidden"),
    (StyleId::OverflowY, "hidden"),
];

/// Lay the document 1 out, 390 by 300, with the content region of the
/// owner 2, its content 3 and its placeholder 5, under `profile`.
fn lay_out_region(
    k: &mut Kernel,
    profile: crate::region::RegionProfile,
) -> crate::RegionLayoutReceipt {
    let key = |id| k.arena.key(slot(k, id));
    let binding = crate::ContentRegion {
        owner: key(2),
        content: key(3),
        pending: key(5),
    };
    k.set_content_region_profile(Some(binding), profile)
        .unwrap();
    let inputs = crate::RegionInputs {
        catalog: 1,
        consumer_revision: 1,
    };
    k.compute_region_layout(1, Offer::definite(390.0, 300.0), inputs)
        .unwrap()
}

#[test]
fn a_content_region_solves_its_shell_and_withholds_its_content() {
    // The document 1 holds a content region's owner 2 (content 3, holding
    // the text 4, the reader 7 and the popover 9, and the placeholder text
    // 5), the reader 6 in the shell and the popover 8, each reading its top
    // observation with a 7-point fallback under the session's 40-point
    // band. The shell and the popovers, laid out apart from the region,
    // observe it, and the content takes its fallback until the region goes
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3, 6, 7, 8, 9]);
    for id in [4, 5] {
        ops.push(Op::CreateView {
            id,
            node_type: NodeType::Text,
        });
        let text = Op::SetProp {
            id,
            prop: crate::PropId::Text,
            value: "word".into(),
        };
        ops.push(text);
    }
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    ops.push(style(2, vec![], &REGION));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    for id in [6, 7] {
        let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, Some(7.0)))];
        ops.push(style(id, pad, &[(StyleId::Height, "10")]));
    }
    for id in [8, 9] {
        let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, Some(7.0)))];
        let rows = [
            (StyleId::Top, "0"),
            (StyleId::Width, "50"),
            (StyleId::Height, "10"),
        ];
        ops.extend([style(id, pad, &rows), popover(id)]);
    }
    ops.extend([
        children(3, &[4, 7, 9]),
        children(2, &[3, 5]),
        children(1, &[6, 2, 8]),
    ]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band([40.0, 0.0, 0.0, 0.0]).unwrap();
    lay_out_region(&mut k, Default::default());
    assert_eq!(top(&k, 6), 40.0);
    for id in [8, 9] {
        assert_eq!(left(&k, id), 40.0, "the popover {id}");
    }
    // The content's lengths read no observation, and take the fallback
    assert_eq!(k.arena.observations.input(slot(&k, 7)), [None; 4]);
    assert_eq!(k.observation(7), None);
    k.set_content_region(None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 6), 40.0);
    assert!(k.observation(7).is_some());
}

#[test]
fn a_measure_beside_a_content_region_leaves_the_region_as_it_was() {
    // The popover 8, beside the content region of 2, pads its left by its
    // top observation, 0 where the kernel places it; in a block its host
    // gives, which the host places, the band is unavailable and the
    // fallback of 7 applies
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3, 8]);
    for id in [4, 5] {
        ops.push(Op::CreateView {
            id,
            node_type: NodeType::Text,
        });
    }
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    ops.push(style(2, vec![], &REGION));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, Some(7.0)))];
    let rows = [
        (StyleId::Bottom, "0"),
        (StyleId::Width, "50"),
        (StyleId::Height, "10"),
    ];
    ops.extend([style(8, pad, &rows), popover(8)]);
    ops.extend([
        children(3, &[4]),
        children(2, &[3, 5]),
        children(1, &[2, 8]),
    ]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band([40.0, 0.0, 0.0, 0.0]).unwrap();
    let ticket = |r: crate::RegionLayoutReceipt| match r.selection {
        crate::region::RegionSelection::Accepted(p) => p.ticket().clone(),
        crate::region::RegionSelection::Pending(_) => panic!("no accepted content"),
    };
    let accepted = ticket(lay_out_region(&mut k, Default::default()));
    assert_eq!(left(&k, 8), 0.0);
    let given = block(BlockAxis::Is(50.0), BlockAxis::Is(10.0));
    assert_eq!(k.measure_block(8, given).unwrap().border, (57.0, 10.0));
    assert_eq!(left(&k, 8), 0.0);
    // The next region layout keeps the content it accepted
    assert_eq!(ticket(lay_out_region(&mut k, Default::default())), accepted);
}

#[test]
fn a_region_branch_that_is_a_popover_observes_its_band() {
    // The region's content 3 is itself a popover, laid out apart from the
    // region, reading its top observation with a 7-point fallback under
    // the session's 40-point band
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3]);
    ops.push(Op::CreateView {
        id: 5,
        node_type: NodeType::Text,
    });
    ops.push(style(1, vec![], &[(StyleId::Height, "300")]));
    ops.push(style(2, vec![], &REGION));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Top, Some(7.0)))];
    let rows = [
        (StyleId::Top, "0"),
        (StyleId::Width, "50"),
        (StyleId::Height, "10"),
    ];
    ops.extend([style(3, pad, &rows), popover(3)]);
    ops.extend([
        children(2, &[3, 5]),
        children(1, &[2]),
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band([40.0, 0.0, 0.0, 0.0]).unwrap();
    lay_out_region(&mut k, Default::default());
    assert_eq!(left(&k, 3), 40.0);
}

#[test]
fn a_region_pass_lets_the_one_it_supersedes_go() {
    // A 390-point row holds the 250-point shell reader 6, whose left margin
    // is its left observation under the session's 40-point band, and the
    // region's owner 2 in what is left: 140, 100 and 120 points as the
    // reader goes from 0 to 40 to 20. A region holds two publications at
    // most, and the last pass's is current
    let mut k = Kernel::with_monospace();
    let mut ops = create([1, 2, 3, 5, 6]);
    let row = [
        (StyleId::Display, "flex"),
        (StyleId::Width, "390"),
        (StyleId::Height, "300"),
    ];
    ops.push(style(1, vec![], &row));
    let region = [
        (StyleId::PositionType, "relative"),
        (StyleId::Width, "200"),
        (StyleId::Height, "180"),
        (StyleId::MinWidth, "0"),
        (StyleId::OverflowX, "hidden"),
        (StyleId::OverflowY, "hidden"),
    ];
    ops.push(style(2, vec![], &region));
    ops.push(style(
        3,
        vec![],
        &[(StyleId::Width, "100%"), (StyleId::Height, "10")],
    ));
    ops.push(style(5, vec![], &[(StyleId::PositionType, "absolute")]));
    let margin = vec![(StyleId::MarginLeft, observed(Edge::Left, None))];
    ops.push(style(
        6,
        margin,
        &[(StyleId::Width, "250"), (StyleId::FlexShrink, "0")],
    ));
    ops.extend([
        children(2, &[3, 5]),
        children(1, &[6, 2]),
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    k.set_session_band([0.0, 0.0, 0.0, 40.0]).unwrap();
    let r = lay_out_region(&mut k, crate::region::RegionProfile::SplitFacts);
    assert_eq!(k.arena.frame(slot(&k, 6)).x, 20.0);
    assert_eq!(r.origin.width, 120.0);
    assert!(r.current, "the last pass's publication");
}

#[test]
fn a_reader_follows_its_owners_margin_growing_its_block() {
    // The owner 2's root is 100 tall in a block of at most 200, holding a
    // fixed 100-point reader at its origin under a 30-point bottom band;
    // the root's bottom margin grows to 20, which moves no frame and makes
    // the block 120 tall
    let mut ops = create(3..=3);
    ops.push(style(2, vec![], &[(StyleId::Height, "100")]));
    let pad = vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))];
    let fixed = [
        (StyleId::PositionType, "absolute"),
        (StyleId::BoxSizing, "border-box"),
        (StyleId::Width, "100"),
        (StyleId::Height, "100"),
    ];
    ops.push(style(3, pad, &fixed));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::AtMost(200.0));
    k.set_host_block(2, Some(block)).unwrap();
    k.set_owner_band(2, Some([0.0, 0.0, 30.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(3).unwrap()[2], Some(30.0));
    k.apply(0, 2, &[style(2, vec![], &[(StyleId::MarginBottom, "20")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(k.observation(3).unwrap()[2], Some(10.0));
}

#[test]
fn the_leaf_takes_its_fallback_and_reads_as_written() {
    use crate::style::calc::Comparison;
    // No fallback and no band: guaranteed-invalid, the row's initial value
    let mut ops = create(3..=4);
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &[(StyleId::Height, "10")],
    ));
    // A fallback that is a calculation, folded where it is simplified
    let fallback = Node::Sum(vec![px(2.0), px(3.0)]);
    let leaf = Node::Observe(Observe {
        target: Target::Box,
        edge: Edge::Top,
        fallback: Some(Box::new(fallback)),
    });
    ops.push(style(
        4,
        vec![(StyleId::PaddingTop, leaf.clone())],
        &[(StyleId::Height, "10")],
    ));
    // A fallback that reads another edge, unavailable too
    let nested = Node::Observe(Observe {
        target: Target::Box,
        edge: Edge::Top,
        fallback: Some(Box::new(observed(Edge::Left, Some(6.0)))),
    });
    ops.push(style(
        5,
        vec![(StyleId::PaddingTop, nested)],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5])]);
    ops.splice(0..0, create(5..=5));
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, None, &[]).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 0.0);
    assert_eq!(top(&k, 4), 5.0);
    assert_eq!(top(&k, 5), 6.0);
    let mut css = String::new();
    Comparison::intern(crate::style::calc::simplify::simplify(leaf)).css(&mut css);
    assert_eq!(css, "var(--exact-safe-area-inset-top, 5px)");
}

#[path = "observe_cost_tests.rs"]
mod cost;

#[path = "targets_tests.rs"]
mod targets;

#[path = "provenance_tests.rs"]
mod provenance;

#[path = "content_root_tests.rs"]
mod content_root;

#[path = "reservation_tests.rs"]
mod reservation;
