//! A box's `clip-path` under `transition` through the kernel (LLP 1003.003
//! D6): what the kernel sends the engine at each commit, and what the engine
//! presents.

use exact_kernel::{motion_node, Kernel, NodeType, Op, StyleId, StyleProps, StyleValue};
use exact_motion::{Engine, Property};

const ROOT: u32 = 1;
const PARENT: u32 = 2;
const BOX: u32 = 3;
const OTHER: u32 = 4;

fn clip(tip: u32, transition: Option<&str>) -> Box<StyleProps> {
    let mut s = StyleProps::default();
    s.set_dynamic(
        StyleId::ClipPath,
        &StyleValue::Text(
            format!("path('M0 0 H120 V40 H0 Z M104 40 L120 40 L120 {tip} Z')").into(),
        ),
    )
    .unwrap();
    if let Some(t) = transition {
        s.set_dynamic(StyleId::Transition, &StyleValue::Text(t.into()))
            .unwrap();
    }
    Box::new(s)
}

fn display(none: bool) -> Box<StyleProps> {
    let mut s = StyleProps::default();
    s.set_dynamic(
        StyleId::Display,
        &StyleValue::Text(if none { "none" } else { "flex" }.into()),
    )
    .unwrap();
    Box::new(s)
}

/// A root holding a parent and another box, the parent holding the box,
/// whose clip `tip` is 48 and whose `transition` is `transition`.
fn mounted(transition: Option<&str>) -> (Kernel, Engine, u64) {
    let mut kernel = Kernel::with_monospace();
    let view = |id| Op::CreateView {
        id,
        node_type: NodeType::View,
    };
    let ops = [
        view(ROOT),
        view(PARENT),
        view(BOX),
        view(OTHER),
        Op::SetChildren {
            id: ROOT,
            children: vec![PARENT, OTHER],
        },
        Op::SetChildren {
            id: PARENT,
            children: vec![BOX],
        },
        Op::SetStyle {
            id: BOX,
            patch: clip(48, transition),
        },
        Op::AttachRoot { id: ROOT },
    ];
    let receipt = kernel.apply(0, 1, &ops).unwrap();
    let mut engine = Engine::new();
    kernel.motion_sync(&receipt).apply(&mut engine).unwrap();
    let node = motion_node(kernel.node(BOX).unwrap().key);
    (kernel, engine, node)
}

fn commit(kernel: &mut Kernel, engine: &mut Engine, batch: u64, ops: &[Op]) {
    let receipt = kernel.apply(0, batch, ops).unwrap();
    kernel.motion_sync(&receipt).apply(engine).unwrap();
}

fn tip(engine: &Engine, node: u64) -> Option<f64> {
    engine.presented_clip(node).map(|c| c.path.0[7].args[1])
}

/// CSS Transitions §3 starts from the before-change value: a box whose clip
/// no declaration covered, given `transition: clip-path` and a new clip in
/// one commit, moves from the clip it had.
#[test]
fn a_clip_moves_from_the_clip_before_its_commit() {
    exact_motion::link_transitions();
    let (mut kernel, mut engine, node) = mounted(None);
    commit(
        &mut kernel,
        &mut engine,
        2,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(40, Some("clip-path 1s linear")),
        }],
    );
    engine.advance(0.5).unwrap();
    assert_eq!(tip(&engine, node), Some(44.0), "half way from 48 to 40");
}

/// A clip that changes while no declaration covers it settles at once, and
/// `transition: none` mid-run stops it.
#[test]
fn an_uncovered_clip_settles_at_once_and_none_stops_a_run() {
    exact_motion::link_transitions();
    let (mut kernel, mut engine, node) = mounted(None);
    commit(
        &mut kernel,
        &mut engine,
        2,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(40, None),
        }],
    );
    assert_eq!(tip(&engine, node), None);
    commit(
        &mut kernel,
        &mut engine,
        3,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(48, Some("clip-path 1s linear")),
        }],
    );
    assert!(engine.is_active(node, Property::ClipPath));
    commit(
        &mut kernel,
        &mut engine,
        4,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(48, Some("none")),
        }],
    );
    assert!(!engine.is_active(node, Property::ClipPath));
}

/// CSS Transitions 2 §3: a box that stops being rendered cancels its run,
/// though the commit never touched it (its parent took `display: none`),
/// and shown again with a new clip it takes it at once.
#[test]
fn a_hidden_parent_cancels_its_childs_run() {
    exact_motion::link_transitions();
    let (mut kernel, mut engine, node) = mounted(Some("clip-path 1s linear"));
    commit(
        &mut kernel,
        &mut engine,
        2,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(40, Some("clip-path 1s linear")),
        }],
    );
    assert!(engine.is_active(node, Property::ClipPath));
    commit(
        &mut kernel,
        &mut engine,
        3,
        &[Op::SetStyle {
            id: PARENT,
            patch: display(true),
        }],
    );
    assert!(
        !engine.is_active(node, Property::ClipPath),
        "not rendered: cancelled"
    );
    commit(
        &mut kernel,
        &mut engine,
        4,
        &[
            Op::SetStyle {
                id: PARENT,
                patch: display(false),
            },
            Op::SetStyle {
                id: BOX,
                patch: clip(48, Some("clip-path 1s linear")),
            },
        ],
    );
    assert!(
        !engine.is_active(node, Property::ClipPath),
        "rendered again: at once"
    );
}

/// A subtree detached mid-run is cancelled, and attached again it takes its
/// clip at once; a box moved between rendered parents keeps its run, as
/// DOM's `moveBefore()` keeps state, and one moved under a hidden parent
/// is cancelled.
#[test]
fn a_detached_box_is_cancelled_and_a_moved_one_keeps_its_run() {
    exact_motion::link_transitions();
    let (mut kernel, mut engine, node) = mounted(Some("clip-path 1s linear"));
    commit(
        &mut kernel,
        &mut engine,
        2,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(40, Some("clip-path 1s linear")),
        }],
    );
    commit(
        &mut kernel,
        &mut engine,
        3,
        &[Op::SetChildren {
            id: PARENT,
            children: vec![],
        }],
    );
    assert!(
        !engine.is_active(node, Property::ClipPath),
        "detached: cancelled"
    );
    commit(
        &mut kernel,
        &mut engine,
        4,
        &[Op::SetChildren {
            id: PARENT,
            children: vec![BOX],
        }],
    );
    assert!(
        !engine.is_active(node, Property::ClipPath),
        "attached again: no run"
    );
    assert_eq!(tip(&engine, node), None);

    commit(
        &mut kernel,
        &mut engine,
        5,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(48, Some("clip-path 1s linear")),
        }],
    );
    assert!(engine.is_active(node, Property::ClipPath));
    commit(
        &mut kernel,
        &mut engine,
        6,
        &[Op::SetChildren {
            id: OTHER,
            children: vec![BOX],
        }],
    );
    assert!(
        engine.is_active(node, Property::ClipPath),
        "moved under another rendered parent: it runs on"
    );
    commit(
        &mut kernel,
        &mut engine,
        7,
        &[Op::SetStyle {
            id: PARENT,
            patch: display(true),
        }],
    );
    commit(
        &mut kernel,
        &mut engine,
        8,
        &[Op::SetChildren {
            id: PARENT,
            children: vec![BOX],
        }],
    );
    assert!(
        !engine.is_active(node, Property::ClipPath),
        "moved under a hidden parent: cancelled"
    );
}

/// A box under a hidden parent, moved in one commit into a wrapper the
/// commit makes and attached where it renders, is observed then: its next
/// compatible change moves.
#[test]
fn a_box_attached_through_a_new_wrapper_is_observed() {
    exact_motion::link_transitions();
    let (mut kernel, mut engine, node) = mounted(Some("clip-path 1s linear"));
    commit(
        &mut kernel,
        &mut engine,
        2,
        &[Op::SetStyle {
            id: PARENT,
            patch: display(true),
        }],
    );
    commit(
        &mut kernel,
        &mut engine,
        3,
        &[
            Op::CreateView {
                id: 9,
                node_type: NodeType::View,
            },
            Op::SetChildren {
                id: 9,
                children: vec![BOX],
            },
            Op::SetChildren {
                id: ROOT,
                children: vec![PARENT, OTHER, 9],
            },
        ],
    );
    commit(
        &mut kernel,
        &mut engine,
        4,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(40, Some("clip-path 1s linear")),
        }],
    );
    assert!(
        engine.is_active(node, Property::ClipPath),
        "observed as it showed, so the change moves"
    );
}

/// A parent hidden and its child detached in one commit: the child is not
/// rendered, so its run is cancelled, though the display walk no longer
/// finds it under the parent.
#[test]
fn a_child_detached_as_its_parent_hides_is_cancelled() {
    exact_motion::link_transitions();
    let (mut kernel, mut engine, node) = mounted(Some("clip-path 1s linear"));
    commit(
        &mut kernel,
        &mut engine,
        2,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(40, Some("clip-path 1s linear")),
        }],
    );
    assert!(engine.is_active(node, Property::ClipPath));
    commit(
        &mut kernel,
        &mut engine,
        3,
        &[
            Op::SetStyle {
                id: PARENT,
                patch: display(true),
            },
            Op::SetChildren {
                id: PARENT,
                children: vec![],
            },
        ],
    );
    assert!(!engine.is_active(node, Property::ClipPath));
}

/// LLP 1003.003 D6: what a commit leaves rendered decides, whatever order
/// its ops take a box there in: hiding or detaching its parent, then moving
/// it to a box that is not rendered, cancels its run.
#[test]
fn a_box_moved_from_a_parent_the_commit_hid_or_detached_is_cancelled() {
    exact_motion::link_transitions();
    let start = |kernel: &mut Kernel, engine: &mut Engine, node| {
        commit(
            kernel,
            engine,
            3,
            &[Op::SetStyle {
                id: BOX,
                patch: clip(40, Some("clip-path 1s linear")),
            }],
        );
        assert!(engine.is_active(node, Property::ClipPath));
    };
    // Hidden, then moved under a box already hidden
    let (mut kernel, mut engine, node) = mounted(Some("clip-path 1s linear"));
    commit(
        &mut kernel,
        &mut engine,
        2,
        &[Op::SetStyle {
            id: OTHER,
            patch: display(true),
        }],
    );
    start(&mut kernel, &mut engine, node);
    commit(
        &mut kernel,
        &mut engine,
        4,
        &[
            Op::SetStyle {
                id: PARENT,
                patch: display(true),
            },
            Op::SetChildren {
                id: OTHER,
                children: vec![BOX],
            },
        ],
    );
    assert!(!engine.is_active(node, Property::ClipPath));
    commit(
        &mut kernel,
        &mut engine,
        5,
        &[Op::SetStyle {
            id: OTHER,
            patch: display(false),
        }],
    );
    assert!(
        !engine.is_active(node, Property::ClipPath),
        "shown again, at its clip"
    );
    commit(
        &mut kernel,
        &mut engine,
        6,
        &[Op::SetStyle {
            id: BOX,
            patch: clip(48, Some("clip-path 1s linear")),
        }],
    );
    assert!(
        engine.is_active(node, Property::ClipPath),
        "and moves from it"
    );
    // Detached, then moved under a box no root reaches
    let (mut kernel, mut engine, node) = mounted(Some("clip-path 1s linear"));
    start(&mut kernel, &mut engine, node);
    commit(
        &mut kernel,
        &mut engine,
        4,
        &[
            Op::SetChildren {
                id: ROOT,
                children: vec![OTHER],
            },
            Op::CreateView {
                id: 5,
                node_type: NodeType::View,
            },
            Op::SetChildren {
                id: 5,
                children: vec![BOX],
            },
        ],
    );
    assert!(!engine.is_active(node, Property::ClipPath));
}
