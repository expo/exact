//! What the grnl port added to the `animation` row's bytes (LLP 1055 D5):
//! a `light-dark()` keyframe's dark value and `box-shadow`'s two halves
//! round-trip bit for bit (LLP 1062 D9), and an `-exact-exit-animation` that never
//! ends is refused on both ingress paths (LLP 1063 D2).

use exact_kernel::wire::codec::{Reader, Writer};
use exact_kernel::{
    wire, ApplyError, Kernel, KernelError, NodeType, Op, StyleId, StyleProps, StyleValue,
};
use exact_motion::{AnimationError, Animations, Keyframes};

const LIT: &str = "from{color:light-dark(#4f6657, #b7c9ac);box-shadow:0 6px 32px light-dark(#171b171a, #0000004d)}to{color:#171b17;box-shadow:none}";

fn rows(shorthand: &str) -> Animations {
    let rule = Keyframes::parse(LIT).unwrap();
    let mut a = Animations::parse(shorthand).unwrap();
    assert!(a.resolve(|_| Some(&rule)).is_empty());
    a
}

fn through_the_wire(row: &Animations) -> Animations {
    let mut w = Writer::new();
    w.animations(row);
    let bytes = w.into_vec();
    let mut r = Reader::new(&bytes);
    let read = r.animations().unwrap();
    assert_eq!(r.remaining(), 0);
    read
}

fn patch(id: StyleId, animations: &Animations) -> Box<StyleProps> {
    let mut s = StyleProps::default();
    match id {
        StyleId::ExitAnimation => s.rare.exit_animation = animations.clone(),
        _ => s.animation = animations.clone().into(),
    }
    s.mask.set(id);
    Box::new(s)
}

#[test]
fn dark_values_and_shadows_round_trip_through_exwf_bit_for_bit() {
    let row = through_the_wire(&rows("lit 900ms linear 120ms both"));
    assert_eq!(through_the_wire(&row), row);
    let first = &row.0[0].keyframes.0[0];
    assert_eq!(first.dark.len(), 2, "the colour's and the shadow's");
    let ops = vec![
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 1,
            patch: patch(StyleId::Animation, &row),
        },
        Op::AttachRoot { id: 1 },
    ];
    let mut k = Kernel::with_monospace();
    k.apply_frame(&wire::encode(0, 1, &ops)).unwrap();
    assert_eq!(*k.node(1).unwrap().style.animation, row);
}

#[test]
fn an_exit_that_never_ends_is_refused_on_both_paths() {
    for text in ["lit 1s infinite", "lit 1s paused"] {
        let row = rows(text);
        let mut k = Kernel::with_monospace();
        let result = k.apply(
            0,
            1,
            &[
                Op::CreateView {
                    id: 1,
                    node_type: NodeType::View,
                },
                Op::SetStyle {
                    id: 1,
                    patch: patch(StyleId::ExitAnimation, &row),
                },
            ],
        );
        assert!(
            matches!(
                result,
                Err(KernelError::Apply(ApplyError::InvalidAnimation {
                    op_index: 1,
                    error: AnimationError::Endless
                }))
            ),
            "{text}: {result:?}"
        );
        let mut s = StyleProps::default();
        assert!(s
            .set_dynamic(StyleId::ExitAnimation, &StyleValue::Text(text.into()))
            .is_err());
        // The same row is an ordinary `animation`.
        assert!(s
            .set_dynamic(StyleId::Animation, &StyleValue::Text(text.into()))
            .is_ok());
    }
}

/// `-exact-animation-trigger: view`, the default (LLP 1055 D13): below a row a
/// list says it mounted out of its port, the row reaches motion paused; the
/// commit that names the row revealed resumes it. A node with `none`, and a
/// row with nothing waiting below it, are untouched.
#[test]
fn an_animation_waits_for_its_row_unless_its_trigger_is_none() {
    let row = rows("lit 900ms linear");
    let waiting = patch(StyleId::Animation, &row);
    let mut at_once = patch(StyleId::Animation, &row);
    at_once.rare.animation_trigger = exact_kernel::AnimationTrigger::None;
    at_once.mask.set(StyleId::AnimationTrigger);
    let view = |id| Op::CreateView {
        id,
        node_type: NodeType::View,
    };
    let ops = vec![
        view(1),
        view(2),
        view(3),
        view(4),
        view(5),
        Op::SetStyle {
            id: 3,
            patch: waiting,
        },
        Op::SetStyle {
            id: 5,
            patch: at_once,
        },
        Op::SetChildren {
            id: 2,
            children: vec![3],
        },
        Op::SetChildren {
            id: 4,
            children: vec![5],
        },
        Op::SetChildren {
            id: 1,
            children: vec![2, 4],
        },
        Op::AttachRoot { id: 1 },
    ];
    let mut k = Kernel::with_monospace();
    k.apply(0, 1, &ops).unwrap();
    let paused = |k: &Kernel, id| {
        let node = k.node(id).unwrap();
        k.animation_row(&node).0.iter().all(|a| a.paused)
    };
    assert!(!paused(&k, 3), "no list said its row is out of the port");
    assert!(k.await_view(2));
    assert!(!k.await_view(4), "nothing below row 4 waits");
    assert!(paused(&k, 3));
    assert!(!paused(&k, 5));
    let mut sync = Default::default();
    k.motion_sync_node(k.node(3).unwrap().key, &mut sync);
    assert!(sync.animations[0].1 .0[0].paused);

    assert!(k.reveal(2));
    assert!(!k.reveal(2), "once");
    let receipt = exact_kernel::CommitReceipt {
        revealed: vec![k.node(2).unwrap().key],
        ..Default::default()
    };
    let sync = k.motion_sync(&receipt);
    assert_eq!(sync.animations.len(), 1);
    assert_eq!(sync.animations[0].1, row, "running, from its start");
}

#[test]
fn delayed_lights_keep_independent_timing_through_patches_and_clears() {
    use exact_kernel::{motion::motion_node, StyleMask};
    use exact_motion::{Engine, Property, Value};

    exact_kernel::timeline::link();
    let light = |delay| {
        let mut animations =
            Animations::parse(&format!("pulse 1s linear {delay}ms infinite both")).unwrap();
        let frames = Keyframes::parse("from{opacity:0}to{opacity:1}").unwrap();
        animations.resolve(|_| Some(&frames));
        let mut style = patch(StyleId::Animation, &animations);
        for (id, value) in [
            (StyleId::AnimationTimeline, "-exact-clock(Beat)"),
            (StyleId::BoxShadow, "0 0 8px #ff8800"),
        ] {
            style
                .set_dynamic(id, &StyleValue::Text(value.into()))
                .unwrap();
        }
        style.written = style.mask;
        style
    };
    let mut k = Kernel::with_monospace();
    let receipt = k
        .apply_frame(&wire::encode(
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
                Op::CreateView {
                    id: 3,
                    node_type: NodeType::View,
                },
                Op::SetStyle {
                    id: 2,
                    patch: light(0),
                },
                Op::SetStyle {
                    id: 3,
                    patch: light(250),
                },
                Op::SetChildren {
                    id: 1,
                    children: vec![2, 3],
                },
                Op::AttachRoot { id: 1 },
            ],
        ))
        .unwrap();
    let a = motion_node(k.node(2).unwrap().key);
    let b = motion_node(k.node(3).unwrap().key);
    let mut engine = Engine::new();
    k.motion_sync(&receipt).apply(&mut engine).unwrap();
    let sample = |engine: &Engine, id| {
        let play = &engine.animation_plays(id)[0];
        play.animation
            .sample(
                play.local(engine.now()),
                Property::Opacity,
                Value::scalar(1.0),
            )
            .unwrap()
            .x
    };
    engine.advance(0.5).unwrap();
    assert_eq!(sample(&engine, a), 0.5);
    assert_eq!(sample(&engine, b), 0.25);
    let start = engine.animation_plays(b)[0].start;
    let receipt = k
        .apply(
            0,
            2,
            &[Op::SetStyle {
                id: 3,
                patch: light(100),
            }],
        )
        .unwrap();
    k.motion_sync(&receipt).apply(&mut engine).unwrap();
    assert_eq!(
        engine.animation_plays(b)[0].start,
        start,
        "retimed in place"
    );
    assert_eq!(sample(&engine, a), 0.5);
    assert_eq!(sample(&engine, b), 0.4);
    assert_eq!(k.node(2).unwrap().style.animation.0[0].delay, 0.0);
    assert_eq!(
        k.node(3)
            .unwrap()
            .computed_style(StyleMask::ALL)
            .animation
            .0[0]
            .delay,
        0.1
    );
    let animation = StyleMask::of(StyleId::Animation);
    for (batch, written) in [(3, true), (4, false)] {
        let receipt = k
            .apply(
                0,
                batch,
                &[Op::ClearStyle {
                    id: 3,
                    mask: animation,
                    written,
                }],
            )
            .unwrap();
        k.motion_sync(&receipt).apply(&mut engine).unwrap();
        let node = k.node(3).unwrap();
        assert!(!node.style.mask.has(StyleId::Animation));
        assert_eq!(node.style.written.has(StyleId::Animation), written);
        assert!(node.style.animation.0.is_empty());
        assert!(engine.animation_plays(b).is_empty());
        assert_eq!(sample(&engine, a), 0.5);
    }
    let receipt = k
        .apply(
            0,
            5,
            &[Op::SetStyle {
                id: 3,
                patch: light(250),
            }],
        )
        .unwrap();
    k.motion_sync(&receipt).apply(&mut engine).unwrap();
    assert_eq!(sample(&engine, b), 0.25, "rejoins the live clock");
}
