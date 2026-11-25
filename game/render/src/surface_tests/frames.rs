//! Render on demand, the frame-rate cap, runtime scene samples and the
//! frame's timing facts: presentation only, never the world.
use super::*;
use exact_game::{Quality, SceneSamples};

#[derive(Default, exact_game::Args)]
struct DiceArgs {
    /// The dice fly: the world moves and is not idle.
    #[live]
    rolling: bool,
    /// 1 asks for one scene sample.
    #[live]
    rough: u32,
    /// The game's own cap.
    #[live]
    fps: u32,
    #[live]
    paused: bool,
}
struct Dice;
impl Game for Dice {
    const ID: &'static str = "frames-dice";
    type Args = DiceArgs;
    fn quality(_: &DiceArgs) -> Quality {
        Quality::High
    }
    fn render_idle(_: &World, args: &DiceArgs) -> bool {
        !args.rolling
    }
    fn max_fps(_: &World, args: &DiceArgs) -> u32 {
        args.fps
    }
    fn paused(args: &DiceArgs) -> bool {
        args.paused
    }
    fn samples(args: &DiceArgs) -> SceneSamples {
        if args.rough == 1 {
            SceneSamples::One
        } else {
            Self::SAMPLES
        }
    }
    fn setup(w: &mut World, _: &DiceArgs) {
        w.spawn_named("die", (Transform::at(0., 0.5, 0.), Mesh::cube(1.)));
        w.spawn((
            Transform::at(0., 6., 6.).looking_at(Vec3::ZERO, Vec3::Y),
            Camera::default(),
        ));
    }
    fn tick(w: &mut World, _: &Input, args: &DiceArgs) {
        if args.rolling {
            if let Some(mut t) = w.get_mut::<Transform>("die") {
                t.position.x += 0.01;
            }
        }
    }
}

const PERIOD: f64 = 1000.0 / 60.0;

fn live(now_ms: f64) -> Frame {
    Frame {
        seekable: false,
        period_ms: PERIOD,
        ..frame(now_ms)
    }
}

fn args(rolling: bool, rough: u32, fps: u32) -> [Value; 4] {
    [
        Value::Bool(rolling),
        Value::Number(rough.into()),
        Value::Number(fps.into()),
        Value::Bool(false),
    ]
}

fn live_dice(rolling: bool) -> WorldSurface<Dice> {
    let mut s = WorldSurface::<Dice>::default();
    exact_gpu::Surface::clock(&mut s, false);
    s.bind(&args(rolling, 0, 0), None).unwrap();
    s
}

fn tick(s: &WorldSurface<Dice>) -> u64 {
    s.sim().unwrap().world().tick()
}

fn state(s: &mut WorldSurface<Dice>) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(&s.agent(r#"{"op":"state"}"#).unwrap()).unwrap()
        ["world"]
        .clone()
}

#[test]
fn a_resting_world_ticks_input_before_deciding_to_draw() {
    use std::task::Poll::{Pending, Ready};
    struct Shake;
    impl Game for Shake {
        const ID: &'static str = "frames-shake";
        type Args = DiceArgs;
        fn setup(w: &mut World, a: &DiceArgs) {
            Dice::setup(w, a);
        }
        fn tick(w: &mut World, input: &Input, _: &DiceArgs) {
            if let Some(sample) = input.sensor(exact_game::SensorKind::Accelerometer) {
                if sample.values[0] < 0. {
                    w.get_mut::<Transform>("die").unwrap().position.y = 2.;
                } else if sample.values[0] > 4. {
                    w.get_mut::<Transform>("die").unwrap().position.x += 0.5;
                }
            }
        }
        fn render_idle(w: &World, _: &DiceArgs) -> bool {
            w.get::<Transform>("die").unwrap().position.x == 0.
        }
    }
    let Some(gpu) = gpu() else { return };
    let mut world = WorldSurface::<Shake>::default();
    world.poll_input_before_draw();
    world.clock(false);
    world.bind(&args(false, 0, 0), None).unwrap();
    let rest = (0..10)
        .map(|i| f64::from(i) * PERIOD)
        .find(|&now| !fixture::render(&gpu, &mut world, &live(now)).unwrap().1)
        .expect("world rests");
    let mut now = rest + 5000.;
    let sample = |at_ms, x| InputEvent::Sensor {
        sensor: exact_gpu::SensorKind::Accelerometer,
        values: [x, 9.8, 0., 0.],
        dropped: 0,
        at_ms,
    };
    let tick = world.sim().unwrap().world().tick();
    world.input(&InputEvent::Pad {
        index: 0,
        connected: true,
        standard: true,
        buttons: [0.; 17],
        axes: [0.; 4],
        at_ms: now - 4.,
    });
    for repeat in [false, true] {
        world.input(&InputEvent::Key {
            code: "KeyA".into(),
            key: "a".into(),
            down: true,
            repeat,
            at_ms: now - 3.,
        });
    }
    world.input(&InputEvent::Pointer {
        id: 1,
        phase: exact_gpu::PointerPhase::Move,
        x: 4.,
        y: 5.,
        dx: 1.,
        dy: 1.,
        kind: exact_gpu::PointerKind::Mouse,
        buttons: 0,
        shift_key: false,
        ctrl_key: false,
        alt_key: false,
        meta_key: false,
        at_ms: now - 2.,
    });
    world.input(&sample(now - 2., 0.));
    world.input(&sample(now - 1., 0.));
    assert_eq!(world.redraw_for_input(now, PERIOD), Ready(false));
    assert!(world.sim().unwrap().world().tick() > tick);
    assert!(world.still && world.sim().unwrap().render_idle());

    world.input(&sample(now, 0.));
    assert_eq!(
        world.redraw_for_input(now, PERIOD),
        Pending,
        "between ticks"
    );
    now += 6. * PERIOD;
    let waiting_at = world.sim().unwrap().world().tick();
    assert_eq!(world.redraw_for_input(now, PERIOD), Ready(false));
    assert!(
        world.sim().unwrap().world().tick() >= waiting_at + 5,
        "a late retry keeps elapsed time"
    );

    now += PERIOD;
    world.input(&sample(now - PERIOD, -9.8));
    assert_eq!(
        world.redraw_for_input(now, PERIOD),
        Ready(true),
        "a one-tick change of pose"
    );
    for _ in 0..4 {
        fixture::render(&gpu, &mut world, &live(now)).unwrap();
        if world.resting {
            break;
        }
        now += PERIOD;
    }
    assert!(world.resting);
    now += 3. * PERIOD;
    world.input(&sample(now - PERIOD, 9.8));
    assert_eq!(world.redraw_for_input(now, PERIOD), Ready(true));
    let tick = world.sim().unwrap().world().tick();
    assert!(fixture::render(&gpu, &mut world, &live(now)).unwrap().1);
    assert_eq!(world.sim().unwrap().world().tick(), tick);
    assert!(!world.still);
    for _ in 0..12 {
        now += PERIOD;
        world.input(&sample(now - 1., 9.8));
        assert_eq!(world.redraw_for_input(now, PERIOD), Ready(true));
        assert!(fixture::render(&gpu, &mut world, &live(now)).unwrap().1);
    }
}

#[test]
fn slow_and_paused_worlds_rest_with_input_already_running() {
    use std::task::Poll;
    struct Slow;
    impl Game for Slow {
        const ID: &'static str = "slow-sensor";
        const HZ: u32 = 10;
        type Args = DiceArgs;
        fn setup(w: &mut World, a: &DiceArgs) {
            Dice::setup(w, a);
        }
        fn tick(_: &mut World, _: &Input, _: &DiceArgs) {}
        fn render_idle(_: &World, _: &DiceArgs) -> bool {
            true
        }
        fn paused(a: &DiceArgs) -> bool {
            a.paused
        }
    }
    let Some(gpu) = gpu() else { return };
    for paused in [false, true] {
        let mut world = WorldSurface::<Slow>::default();
        world.poll_input_before_draw();
        world.clock(false);
        let mut options = args(false, 0, 0);
        options[3] = Value::Bool(paused);
        world.bind(&options, None).unwrap();
        let mut draw = true;
        let mut quiet = 0;
        for i in 0..40 {
            let now = f64::from(i) * PERIOD;
            world.input(&InputEvent::Sensor {
                sensor: exact_gpu::SensorKind::Accelerometer,
                values: [0., 9.8, 0., 0.],
                dropped: 0,
                at_ms: now,
            });
            if draw || world.redraw_for_input(now, PERIOD) == Poll::Ready(true) {
                draw = fixture::render(&gpu, &mut world, &live(now)).unwrap().1;
                quiet = 0;
            } else {
                quiet += 1;
            }
        }
        assert!(
            quiet >= 20,
            "rest with samples arriving before every tick, paused={paused}"
        );
        assert!(world.still);
        if paused {
            assert!(
                !world.sim().unwrap().input_pending(),
                "paused samples are applied at delivery"
            );
            world
                .sim
                .as_mut()
                .unwrap()
                .input(exact_game::InputEvent::Message {
                    player: 0,
                    text: "resume".into(),
                    at_ms: 42. * PERIOD,
                });
            assert!(world.sim().unwrap().input_pending());
            assert_eq!(
                world.redraw_for_input(40. * PERIOD, PERIOD),
                Poll::Ready(false)
            );
            options[3] = Value::Bool(false);
            world.bind(&options, None).unwrap();
            for i in 41..55 {
                fixture::render(&gpu, &mut world, &live(f64::from(i) * PERIOD)).unwrap();
            }
            assert!(!world.sim().unwrap().input_pending());
        } else {
            assert!(world.sim().unwrap().world().tick() > 3);
        }
    }
}

#[test]
fn a_quiet_hook_redraws_for_an_observed_tick_or_a_paused_sensor_change() {
    #[derive(Default)]
    struct Indicator(bool);
    impl crate::Hooks for Indicator {
        fn prepare(
            &mut self,
            _: &crate::HookGpu<'_>,
            _: &crate::FrameView<'_>,
            world: &crate::RenderWorld<'_>,
        ) -> Result<(), crate::RenderError> {
            if self.0 || world.tick() % 2 == 0 {
                let _ = world.resource::<exact_game::SensorState>();
            }
            Ok(())
        }
    }
    let Some(gpu) = gpu() else { return };
    for paused in [false, true] {
        let mut s = WorldSurface::<Dice, (), false, Indicator>::default();
        s.hooks.0 = paused;
        s.poll_input_before_draw();
        s.clock(false);
        let mut options = args(false, 0, 0);
        options[3] = Value::Bool(paused);
        s.bind(&options, None).unwrap();
        for i in 0..8 {
            if !fixture::render(&gpu, &mut s, &live(f64::from(i) * PERIOD))
                .unwrap()
                .1
            {
                break;
            }
        }
        assert!(s.still);
        s.input(&InputEvent::Sensor {
            sensor: exact_gpu::SensorKind::Accelerometer,
            values: [0., 9.8, 0., 0.],
            dropped: 0,
            at_ms: 950.,
        });
        assert_eq!(
            s.redraw_for_input(1000., PERIOD),
            std::task::Poll::Ready(true),
            "paused={paused}"
        );
    }
}

#[test]
fn catch_up_draw_and_pause_without_timing() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    for now in [0., 300.] {
        fixture::render(&gpu, &mut s, &live(now)).unwrap();
    }
    let before = state(&mut s);
    let mut paused = args(true, 0, 0);
    paused[3] = Value::Bool(true);
    s.bind(&paused, Some(300.)).unwrap();
    s.bind(&args(true, 0, 0), None).unwrap();
    fixture::render(&gpu, &mut s, &live(2300.)).unwrap();
    assert_eq!(
        [
            before["perf"]["catchUp"].clone(),
            state(&mut s)["perf"]["catchUp"].clone()
        ],
        [
            serde_json::json!({"clampFrames":1,"clampMs":50,"idleFrames":0,"idleMs":0,"overloadFrames":1}),
            serde_json::json!({"clampFrames":1,"clampMs":50,"idleFrames":1,"idleMs":1750,"overloadFrames":1}),
        ]
    );
}

#[test]
fn catch_up_first_draw_after_stamped_bind() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Dice>::default();
    s.clock(false);
    s.bind(&args(true, 0, 0), Some(0.)).unwrap();
    fixture::render(&gpu, &mut s, &live(2000.)).unwrap();
    let perf = &state(&mut s)["perf"];
    assert_eq!(perf["catchUp"]["idleMs"], 1750);
    assert_eq!(perf["catchUp"]["clampMs"], 0);
    assert_eq!(perf["ticksOwed"]["max"], 120);
}

#[test]
fn catch_up_keeps_submission_across_early_returns() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    fixture::render(&gpu, &mut s, &live(0.)).unwrap();
    for skipped in [
        Frame {
            width: 0.,
            ..live(100.)
        },
        live(f64::NAN),
    ] {
        fixture::render(&gpu, &mut s, &skipped).unwrap();
        assert!(s.perf.submitted_live.is_some());
    }
    fixture::render(&gpu, &mut s, &live(300.)).unwrap();
    let perf = &state(&mut s)["perf"];
    assert_eq!(perf["catchUp"]["clampMs"], 50);
    assert_eq!(perf["ticksOwed"]["count"], 2);
}

#[test]
fn catch_up_timed_bind_counts_once_without_sampling() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    fixture::render(&gpu, &mut s, &live(0.)).unwrap();
    s.bind(&args(true, 0, 0), Some(300.)).unwrap();
    let before = state(&mut s);
    fixture::render(&gpu, &mut s, &live(300.)).unwrap();
    let after = state(&mut s);
    for perf in [&before["perf"], &after["perf"]] {
        assert_eq!(
            perf["catchUp"],
            serde_json::json!({"clampFrames":1,"clampMs":50,"idleFrames":0,"idleMs":0,"overloadFrames":1})
        );
    }
    assert_eq!(before["perf"]["ticksOwed"]["count"], 1);
    assert_eq!(after["perf"]["ticksOwed"]["count"], 2);
    assert_eq!(after["perf"]["ticksOwed"]["max"], 0);
}

#[test]
fn catch_up_advance_only_counts_once_without_sampling() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    fixture::render(&gpu, &mut s, &live(0.)).unwrap();
    s.advance(&live(300.), &gpu.device, &gpu.queue);
    let before = state(&mut s);
    fixture::render(&gpu, &mut s, &live(300.)).unwrap();
    let after = state(&mut s);
    for perf in [&before["perf"], &after["perf"]] {
        assert_eq!(
            perf["catchUp"],
            serde_json::json!({"clampFrames":1,"clampMs":50,"idleFrames":0,"idleMs":0,"overloadFrames":1})
        );
    }
    assert_eq!(before["perf"]["ticksOwed"]["count"], 1);
    assert_eq!(after["perf"]["ticksOwed"]["count"], 2);
    assert_eq!(after["perf"]["ticksOwed"]["max"], 0);
}

#[test]
fn catch_up_counts_overload_on_short_binds_and_advance_only_frames() {
    let Some(gpu) = gpu() else { return };
    // 100 ms of ticks in one advance at a 60 Hz display, under the 250 ms limit.
    let mut s = live_dice(true);
    fixture::render(&gpu, &mut s, &live(0.)).unwrap();
    s.advance(&live(100.), &gpu.device, &gpu.queue);
    let mut bound = live_dice(true);
    fixture::render(&gpu, &mut bound, &live(0.)).unwrap();
    bound.bind(&args(true, 0, 0), Some(100.)).unwrap();
    for s in [&mut s, &mut bound] {
        assert_eq!(
            state(s)["perf"]["catchUp"],
            serde_json::json!({"clampFrames":0,"clampMs":0,"idleFrames":0,"idleMs":0,"overloadFrames":1})
        );
    }
}

#[test]
fn catch_up_bind_wakes_a_resting_canvas() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(false);
    let now = (0..10)
        .map(|i| f64::from(i) * PERIOD)
        .find(|&now| !fixture::render(&gpu, &mut s, &live(now)).unwrap().1)
        .expect("canvas did not rest");
    s.bind(&args(true, 0, 0), Some(now + 2000.)).unwrap();
    let perf = &state(&mut s)["perf"];
    assert_eq!(perf["catchUp"]["idleFrames"], 1);
    assert_eq!(perf["catchUp"]["idleMs"], 1750);
    assert_eq!(perf["catchUp"]["clampMs"], 0);
}

#[test]
fn catch_up_interrupted_canvas_can_keep_submitting() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    fixture::render(&gpu, &mut s, &live(0.)).unwrap();
    s.lifecycle(Lifecycle::Interrupted);
    for now in [300., 600.] {
        fixture::render(&gpu, &mut s, &live(now)).unwrap();
    }
    assert_eq!(
        state(&mut s)["perf"]["catchUp"],
        serde_json::json!({"clampFrames":1,"clampMs":50,"idleFrames":1,"idleMs":50,"overloadFrames":2})
    );
}

#[test]
fn catch_up_hidden_resume_owes_the_full_gap() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    fixture::render(&gpu, &mut s, &live(0.)).unwrap();
    s.lifecycle(Lifecycle::Hidden);
    s.lifecycle(Lifecycle::Visible);
    fixture::render(&gpu, &mut s, &live(2000.)).unwrap();
    let perf = &state(&mut s)["perf"];
    assert_eq!(perf["catchUp"]["idleMs"], 1750);
    assert_eq!(perf["catchUp"]["clampMs"], 0);
    assert_eq!(perf["ticksOwed"]["max"], 120);
}

#[test]
fn catch_up_seek_samples_both_rings_without_counting() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    s.clock(true);
    for now in [0., 2000.] {
        fixture::render(&gpu, &mut s, &frame(now)).unwrap();
    }
    let perf = &state(&mut s)["perf"];
    assert_eq!(
        perf["catchUp"],
        serde_json::json!({"clampFrames":0,"clampMs":0,"idleFrames":0,"idleMs":0,"overloadFrames":0})
    );
    assert_eq!(perf["ticksPerFrame"]["max"], 120);
    assert_eq!(perf["ticksOwed"], perf["ticksPerFrame"]);
}

#[test]
fn catch_up_classifies_drawn_hidden_and_resting_frames() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(true);
    let draw = |s: &mut WorldSurface<Dice>, now, interval_ms| {
        s.timing(&exact_gpu::FrameTiming {
            interval_ms,
            display_period_ms: PERIOD,
            ..Default::default()
        });
        fixture::render(&gpu, s, &live(now)).unwrap().1
    };
    assert!(draw(&mut s, 0., 0.));
    assert!(draw(&mut s, 300., 300.));
    s.lifecycle(Lifecycle::Hidden);
    s.lifecycle(Lifecycle::Visible);
    assert!(draw(&mut s, 2300., 2000.));
    assert_eq!(
        state(&mut s)["perf"]["catchUp"],
        serde_json::json!({"clampFrames":1,"clampMs":50,"idleFrames":1,"idleMs":1750,"overloadFrames":2})
    );
    let reset = s.agent(r#"{"op":"state","perf_reset":true}"#).unwrap();
    let reset: serde_json::Value = serde_json::from_str(&reset).unwrap();
    assert_eq!(reset["world"]["perf"]["ticksOwed"]["count"], 0);
    assert_eq!(
        reset["world"]["perf"]["catchUp"],
        serde_json::json!({"clampFrames":0,"clampMs":0,"idleFrames":0,"idleMs":0,"overloadFrames":0})
    );

    // A canvas at rest that wakes after 2 s, whatever interval the host names.
    let mut s = live_dice(false);
    let mut t = 0.;
    while draw(&mut s, t, if t == 0. { 0. } else { PERIOD }) {
        t += PERIOD;
        assert!(t < 10. * PERIOD, "still drawing");
    }
    assert!(draw(&mut s, t + 2000., 2000.));
    assert_eq!(
        state(&mut s)["perf"]["catchUp"],
        serde_json::json!({"clampFrames":0,"clampMs":0,"idleFrames":1,"idleMs":1750,"overloadFrames":0})
    );
}

#[test]
fn a_world_at_rest_draws_its_last_frames_then_asks_for_none() {
    let Some(gpu) = gpu() else { return };
    let mut s = live_dice(false);
    let mut frames = 0;
    let mut t = 0.0;
    while fixture::render(&gpu, &mut s, &live(t)).unwrap().1 {
        frames += 1;
        t += PERIOD;
        assert!(frames < 10, "still asking after {frames} frames");
    }
    // The last frame drew between two idle ticks.
    let rest = tick(&s);
    assert!(rest >= 1, "{rest}");
    // Ten minutes later a layout wakes it: it draws until two more idle
    // ticks, a period of ticks on, not ten minutes of them.
    let mut wake = t + 600_000.0;
    // What woke it may have moved something: the wake frame does not rest.
    assert!(fixture::render(&gpu, &mut s, &live(wake)).unwrap().1);
    wake += PERIOD;
    while fixture::render(&gpu, &mut s, &live(wake)).unwrap().1 {
        wake += PERIOD;
        assert!(wake - t - 600_000.0 < 5.0 * PERIOD, "still drawing");
    }
    assert!(
        (1..=4).contains(&(tick(&s) - rest)),
        "{} ticks",
        tick(&s) - rest
    );
    // A live bind at the next minute wakes the clock as a frame does: the dice
    // fly from a period of ticks on, and the canvas draws again.
    let rest = tick(&s);
    let bind = wake + 60_000.0;
    s.bind(&args(true, 0, 0), Some(bind)).unwrap();
    assert!(
        fixture::render(&gpu, &mut s, &live(bind + PERIOD))
            .unwrap()
            .1
    );
    assert!(tick(&s) - rest <= 3, "{} ticks", tick(&s) - rest);
}

#[test]
fn the_agents_clock_never_rests() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Dice>::default();
    s.bind(&args(false, 0, 0), None).unwrap();
    for i in 0..20 {
        assert!(
            fixture::render(&gpu, &mut s, &frame(i as f64 * 100.))
                .unwrap()
                .1
        );
    }
    assert_eq!(tick(&s), 114, "every millisecond ticks");
}

#[derive(Default)]
struct Capped {
    seen: Option<exact_gpu::FrameTiming>,
}
impl crate::Hooks for Capped {
    fn prepare(
        &mut self,
        _: &crate::HookGpu<'_>,
        view: &crate::FrameView<'_>,
        _: &crate::RenderWorld<'_>,
    ) -> Result<(), crate::RenderError> {
        let t = view.time;
        self.seen = Some(exact_gpu::FrameTiming {
            interval_ms: t.interval_ms,
            display_period_ms: t.display_period_ms,
            host: t.host,
            thermal_state: t.thermal_state,
        });
        Ok(())
    }
    fn max_fps(&self) -> u32 {
        if self
            .seen
            .is_some_and(|t| t.thermal_state >= exact_gpu::ThermalState::Serious)
        {
            30
        } else {
            0
        }
    }
}

#[test]
fn the_lower_cap_stands_and_the_hooks_read_the_frames_timing() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Dice, (), false, Capped>::default();
    s.bind(&args(true, 0, 60), None).unwrap();
    use exact_gpu::Surface as _;
    assert_eq!(s.max_fps(), 60, "the game's alone");
    let timing = exact_gpu::FrameTiming {
        interval_ms: 16.5,
        display_period_ms: PERIOD,
        host: exact_gpu::HostTiming {
            raw_ms: 99.0,
            callback_ms: 100.5,
            fastest_period_ms: 1000.0 / 120.0,
        },
        thermal_state: exact_gpu::ThermalState::Serious,
    };
    s.timing(&timing);
    fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    assert_eq!(s.render_hooks().seen, Some(timing));
    assert_eq!(s.max_fps(), 30, "the hook's thermal cap is lower");
    s.bind(&args(true, 0, 24), None).unwrap();
    assert_eq!(s.max_fps(), 24, "the game's is lower");
    s.bind(&args(true, 0, 0), None).unwrap();
    assert_eq!(s.max_fps(), 30, "the game has none");
}

#[test]
fn new_scene_samples_make_the_renderer_again_under_the_same_world() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Dice>::default();
    s.bind(&args(true, 0, 0), None).unwrap();
    for i in 0..3 {
        fixture::render(&gpu, &mut s, &frame(i as f64 * 100.)).unwrap();
    }
    assert_eq!(state(&mut s)["quality"]["samples"], 4);
    assert_eq!(s.samples, Some(SceneSamples::Four));
    let world = state(&mut s);
    s.bind(&args(true, 1, 0), None).unwrap();
    assert!(s.render.is_none(), "released at the bind");
    let after = state(&mut s);
    assert_eq!(
        (&after["hash"], &after["tick"]),
        (&world["hash"], &world["tick"])
    );
    fixture::render(&gpu, &mut s, &frame(300.)).unwrap();
    assert!(s.error().is_none(), "{:?}", s.error());
    assert_eq!(s.samples, Some(SceneSamples::One));
    assert_eq!(state(&mut s)["quality"]["samples"], 1);
    // Unchanged samples keep the renderer.
    s.bind(&args(false, 1, 0), None).unwrap();
    assert!(s.render.is_some());
}

/// A live argument steps the raster scale at the next frame: the module reads
/// `render_scale` at every render.
#[test]
fn a_live_render_scale_steps_at_the_next_frame() {
    #[derive(Default, exact_game::Args)]
    struct Scaled {
        #[live]
        quarter: bool,
    }
    struct Stepper;
    impl Game for Stepper {
        const ID: &'static str = "frames-render-scale";
        type Args = Scaled;
        fn render_scale(args: &Scaled) -> f32 {
            if args.quarter {
                0.25
            } else {
                1.0
            }
        }
        fn setup(_: &mut World, _: &Scaled) {}
        fn tick(_: &mut World, _: &Input, _: &Scaled) {}
    }
    let mut s = WorldSurface::<Stepper>::default();
    s.bind(&[Value::Bool(false)], None).unwrap();
    assert_eq!(exact_gpu::Surface::render_scale(&s), 1.0);
    s.bind(&[Value::Bool(true)], None).unwrap();
    assert_eq!(exact_gpu::Surface::render_scale(&s), 0.25);
}

#[test]
fn an_absent_component_with_the_same_name_cannot_mask_a_hook_read() {
    mod first {
        #[derive(Default, exact_game::Component)]
        pub struct Same {
            pub value: u32,
        }
    }
    mod second {
        #[derive(Default, exact_game::Component)]
        pub struct Same;
    }
    struct Lamp;
    impl Game for Lamp {
        const ID: &'static str = "hook-lamp";
        type Args = DiceArgs;
        fn setup(w: &mut World, args: &DiceArgs) {
            Dice::setup(w, args);
            w.spawn_named("lamp", first::Same::default());
        }
        fn tick(w: &mut World, input: &Input, _: &DiceArgs) {
            if input
                .sensor(exact_game::SensorKind::Accelerometer)
                .is_some_and(|s| s.values[0] > 4.)
            {
                w.get_mut::<first::Same>("lamp").unwrap().value = 1;
            }
        }
        fn render_idle(_: &World, _: &DiceArgs) -> bool {
            true
        }
    }
    #[derive(Default)]
    struct Indicator(u32, crate::Needs);
    impl crate::Hooks for Indicator {
        fn needs(&self) -> crate::Needs {
            self.1
        }
        fn prepare(
            &mut self,
            _: &crate::HookGpu<'_>,
            _: &crate::FrameView<'_>,
            world: &crate::RenderWorld<'_>,
        ) -> Result<(), crate::RenderError> {
            assert!(world.get::<second::Same>("lamp").is_none());
            self.0 = world.get::<first::Same>("lamp").unwrap().value;
            Ok(())
        }
    }

    let Some(gpu) = gpu() else { return };
    let mut surface = WorldSurface::<Lamp, (), false, Indicator>::default();
    surface.poll_input_before_draw();
    surface.clock(false);
    surface.bind(&args(false, 0, 0), None).unwrap();
    let rest = (0..10)
        .map(|i| f64::from(i) * PERIOD)
        .find(|&now| !fixture::render(&gpu, &mut surface, &live(now)).unwrap().1)
        .expect("lamp rests");
    assert_eq!(surface.render_hooks().0, 0);
    for (x, draw, now) in [(0., false, rest + 1000.), (9.8, true, rest + 2000.)] {
        surface.input(&InputEvent::Sensor {
            sensor: exact_gpu::SensorKind::Accelerometer,
            values: [x, 9.8, 0., 0.],
            dropped: 0,
            at_ms: now - PERIOD,
        });
        assert_eq!(
            surface.redraw_for_input(now, PERIOD),
            std::task::Poll::Ready(draw)
        );
        if draw {
            fixture::render(&gpu, &mut surface, &live(now)).unwrap();
            assert_eq!(surface.render_hooks().0, 1);
        }
    }
    for demand in [crate::Needs::ANIMATE, crate::Needs::PENDING] {
        surface.hooks.1 = crate::Needs::NONE;
        let rest = (0..10)
            .map(|i| 3000. + f64::from(i) * PERIOD)
            .find(|&now| !fixture::render(&gpu, &mut surface, &live(now)).unwrap().1)
            .expect("lamp rests after its change");
        surface.hooks.1 = demand;
        surface.input(&InputEvent::Sensor {
            sensor: exact_gpu::SensorKind::Accelerometer,
            values: [0.; 4],
            dropped: 0,
            at_ms: rest,
        });
        assert_eq!(
            surface.redraw_for_input(rest + PERIOD, PERIOD),
            std::task::Poll::Ready(true)
        );
    }
}

#[test]
fn a_refused_device_event_reports_its_error_at_delivery() {
    static GAMES: exact_gpu::Registry = exact_gpu::Registry {
        surfaces: &[("dice", 0, || Box::new(WorldSurface::<Dice>::default()))],
        shaders: &[],
    };
    let mut module = exact_gpu::Module::new(&GAMES);
    let id = module.create_headless("dice").unwrap();
    assert!(module.bind(id, &args(false, 0, 0), None));
    assert!(!module.input(
        id,
        &InputEvent::Key {
            code: "GamepadA".into(),
            key: "".into(),
            down: true,
            repeat: false,
            at_ms: 1.,
        }
    ));
    assert!(module.take_error().contains("is a pad button"));
}
