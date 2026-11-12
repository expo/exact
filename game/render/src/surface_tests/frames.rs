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

fn args(rolling: bool, rough: u32, fps: u32) -> [Value; 3] {
    [
        Value::Bool(rolling),
        Value::Number(rough.into()),
        Value::Number(fps.into()),
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
