//! `filter` under `transition` through the kernel (LLP 1003.002): what the
//! kernel sends the engine at each commit, and what the engine presents.

use exact_kernel::svg::filter::FilterList;
use exact_kernel::{motion_node, Kernel, NodeType, Op, StyleId, StyleProps, Transitions};
use exact_motion::{Engine, Property};

fn style(f: impl FnOnce(&mut StyleProps)) -> Box<StyleProps> {
    let mut s = StyleProps::default();
    f(&mut s);
    Box::new(s)
}

/// LLP 1003.002 D6: a box's `filter` reaches the engine whether or not a
/// declaration covers it, so a commit that sets the `transition` and a new
/// filter together starts from the filter before it; `currentcolor` in a
/// shadow is no change when only `color` moves; a box with neither row
/// keeps nothing.
#[test]
fn a_filter_moves_from_the_filter_before_its_commit() {
    let filter = |css: &str| {
        let list = FilterList::parse(css).unwrap();
        style(move |s| {
            s.filter = list;
            s.mask.set(StyleId::Filter);
        })
    };
    let mut kernel = Kernel::with_monospace();
    let receipt = kernel
        .apply(
            0,
            1,
            &[
                Op::CreateView {
                    id: 1,
                    node_type: NodeType::View,
                },
                Op::CreateView {
                    id: 2,
                    node_type: NodeType::View,
                },
                Op::SetChildren {
                    id: 1,
                    children: vec![2],
                },
                Op::SetStyle {
                    id: 2,
                    patch: filter("blur(12px) drop-shadow(2px 2px 2px)"),
                },
                Op::AttachRoot { id: 1 },
            ],
        )
        .unwrap();
    let mut engine = Engine::new();
    kernel.motion_sync(&receipt).apply(&mut engine).unwrap();
    let node = motion_node(kernel.node(2).unwrap().key);
    let mut both = *filter("blur(0px) drop-shadow(2px 2px 2px)");
    both.transition = Transitions::parse("filter 1s linear").unwrap();
    both.mask.set(StyleId::Transition);
    let receipt = kernel
        .apply(
            0,
            2,
            &[Op::SetStyle {
                id: 2,
                patch: Box::new(both),
            }],
        )
        .unwrap();
    kernel.motion_sync(&receipt).apply(&mut engine).unwrap();
    engine.advance(0.25).unwrap();
    let shown = engine.presented_filter(node).expect("running");
    assert_eq!(shown.0[0].args[0], 9.0);
    assert_eq!(shown.0[1].current, 1.0, "currentcolor is all share");
    // A `color` change alone: the shadow's keyword, no new transition.
    let receipt = kernel
        .apply(
            0,
            3,
            &[Op::SetStyle {
                id: 2,
                patch: style(|s| {
                    s.text_color = exact_kernel::style::ColorValue::Fixed(
                        exact_kernel::style::Color(0xff0000ff),
                    );
                    s.mask.set(StyleId::TextColor);
                }),
            }],
        )
        .unwrap();
    kernel.motion_sync(&receipt).apply(&mut engine).unwrap();
    assert_eq!(engine.settle_time(), Some(1.0), "the same transition");
    // The box's rows cleared: nothing is kept.
    let mut mask = exact_kernel::StyleMask::EMPTY;
    mask.set(StyleId::Transition);
    mask.set(StyleId::Filter);
    let receipt = kernel
        .apply(
            0,
            4,
            &[Op::ClearStyle {
                id: 2,
                mask,
                written: false,
            }],
        )
        .unwrap();
    kernel.motion_sync(&receipt).apply(&mut engine).unwrap();
    assert_eq!(engine.presented_filter(node), None);
    assert!(!engine.is_active(node, Property::Filter));
}

/// CSS Transitions §3 starts from the before-change value: an existing
/// node given `filter` and `transition` in one commit moves from `none`,
/// and a node created with them takes its filter (LLP 1003.002 D6).
#[test]
fn an_existing_node_given_a_filter_and_its_transition_moves_from_none() {
    let both = || {
        style(|s| {
            s.filter = FilterList::parse("blur(12px)").unwrap();
            s.transition = Transitions::parse("filter 1s linear").unwrap();
            s.mask.set(StyleId::Filter);
            s.mask.set(StyleId::Transition);
        })
    };
    let create = |id| Op::CreateView {
        id,
        node_type: NodeType::View,
    };
    let mut kernel = Kernel::with_monospace();
    let receipt = kernel
        .apply(
            0,
            1,
            &[
                create(1),
                create(2),
                Op::SetChildren {
                    id: 1,
                    children: vec![2],
                },
                Op::AttachRoot { id: 1 },
            ],
        )
        .unwrap();
    let mut engine = Engine::new();
    kernel.motion_sync(&receipt).apply(&mut engine).unwrap();
    let receipt = kernel
        .apply(
            0,
            2,
            &[
                Op::SetStyle {
                    id: 2,
                    patch: both(),
                },
                create(3),
                Op::SetStyle {
                    id: 3,
                    patch: both(),
                },
                Op::SetChildren {
                    id: 1,
                    children: vec![2, 3],
                },
            ],
        )
        .unwrap();
    kernel.motion_sync(&receipt).apply(&mut engine).unwrap();
    engine.advance(0.25).unwrap();
    let existing = motion_node(kernel.node(2).unwrap().key);
    let created = motion_node(kernel.node(3).unwrap().key);
    let shown = engine.presented_filter(existing).expect("running");
    assert_eq!(shown.0[0].args[0], 3.0);
    assert_eq!(engine.presented_filter(created), None);
    assert!(!engine.is_active(created, Property::Filter));
}
