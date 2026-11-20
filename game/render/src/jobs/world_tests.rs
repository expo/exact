use super::{tests::Work, Jobs, JobsReport, Limits};
use crate::{FrameView, HookGpu, Hooks, Needs, RenderError, RenderWorld, WorldSurface};
use exact_game::{Actions, Args, Camera, Game, Input, Material, Mesh, Sim, Transform, Vec2, World};
use exact_gpu::{fixture, wgpu, Frame, Gpu, InputEvent, Restore, Surface, Value};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

#[derive(Args, Default)]
struct Options {
    value: u32,
    #[live]
    paused: bool,
    #[live]
    boosted: bool,
}

struct TestGame;
impl Game for TestGame {
    const ID: &'static str = "render-jobs-test";
    type Args = Options;
    const SAMPLES: exact_game::SceneSamples = exact_game::SceneSamples::One;

    fn actions() -> Actions {
        Actions::new().button("hold", &["KeyH"])
    }

    fn setup(world: &mut World, options: &Options) {
        world.insert_resource(TestValue(options.value));
        world.spawn_named(
            "box",
            (
                Transform::default(),
                Mesh::cube(1.),
                Material::rgb(0.8, 0.1, 0.1),
            ),
        );
        world.spawn((Transform::at(0., 0., 4.), Camera::default()));
    }

    fn tick(world: &mut World, input: &Input, options: &Options) {
        if input.held("hold") {
            world.require_mut::<Transform>("box").position.x +=
                if options.boosted { 0.02 } else { 0.01 };
        }
        if let Some(message) = input.messages().last() {
            world.publish("answer", format!("{} {message}", options.value));
        }
    }

    fn paused(options: &Options) -> bool {
        options.paused
    }
}

const LIMITS: Limits = Limits {
    queued_bytes: 64,
    running: 1,
    completed_bytes: 64,
    take_per_frame: 1,
};
const TIMEOUT: Duration = Duration::from_secs(5);
const SLOW: u32 = 99;
const FAIL: u32 = 98;

fn tape<const HOLD: bool>(gpu: &Gpu, seekable: bool) -> Vec<(Vec<u8>, u64)> {
    let mut canvas = WorldSurface::<TestGame, (), false, JobHooks<HOLD>>::default();
    let args = [Value::Number(7.), Value::Bool(false), Value::Bool(false)];
    canvas.bind(&args, None).unwrap();
    canvas.clock(seekable);
    draw(gpu, &mut canvas, 0., seekable);
    let mut finish = None;
    if HOLD {
        let hooks = canvas.render_hooks();
        hooks.start.as_ref().unwrap().recv_timeout(TIMEOUT).unwrap();
        finish = hooks.finish.clone();
    } else if !seekable {
        wait_to_install(canvas.render_hooks());
    }
    let snapshot = |canvas: &WorldSurface<TestGame, (), false, JobHooks<HOLD>>, tick| {
        let sim = canvas.sim().unwrap();
        assert_eq!(sim.world().tick(), tick);
        (sim.save().unwrap(), sim.world().hash())
    };
    let mut checkpoints = vec![snapshot(&canvas, 0)];
    // Whole microseconds: the live and the agent clock then agree on the world time to the last one.
    let stamp = |tick: u32| f64::from(tick) * 17.;
    let (_, wants) = draw_frame(gpu, &mut canvas, stamp(1), seekable);
    assert!(wants);
    assert_eq!(
        canvas.render_hooks().installed.as_slice(),
        if HOLD { &[][..] } else { &[14] }
    );
    checkpoints.push(snapshot(&canvas, 1));
    for tick in 2..=5 {
        assert!(draw_frame(gpu, &mut canvas, stamp(tick), seekable).1);
        checkpoints.push(snapshot(&canvas, tick.into()));
    }

    canvas.input(&InputEvent::Key {
        code: "KeyH".into(),
        key: "h".into(),
        down: true,
        repeat: false,
        at_ms: stamp(5) + 1.,
    });
    canvas.input(&InputEvent::Message {
        text: "message".into(),
        at_ms: stamp(5) + 1.,
    });
    canvas
        .bind(
            &[args[0].clone(), args[1].clone(), Value::Bool(true)],
            Some(stamp(6)),
        )
        .unwrap();
    checkpoints.push(snapshot(&canvas, 6));
    for tick in 7..=8 {
        draw(gpu, &mut canvas, stamp(tick), seekable);
        if tick == 8 {
            canvas.input(&InputEvent::Message {
                text: "queued".into(),
                at_ms: stamp(tick) + 1.,
            });
        }
        checkpoints.push(snapshot(&canvas, tick.into()));
    }
    assert!(canvas.published().unwrap().contains("7 message"));
    assert!(
        canvas
            .sim()
            .unwrap()
            .world()
            .require::<Transform>("box")
            .position
            .x
            > 0.04
    );
    if let Some(finish) = finish {
        assert!(canvas.render_hooks().installed.is_empty());
        finish.send(()).unwrap();
    }
    checkpoints
}

#[test]
fn jobs_world_save_and_hash_match_slow_fast_and_inline_surface_drives() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let fast = tape::<false>(&gpu, false);
    for other in [tape::<true>(&gpu, false), tape::<false>(&gpu, true)] {
        for (tick, ((save, hash), (other_save, other_hash))) in fast.iter().zip(other).enumerate() {
            assert_eq!(*hash, other_hash, "hash at tick {tick}");
            assert!(*save == other_save, "save at tick {tick}");
        }
    }
}

struct JobHooks<const HOLD: bool = false> {
    jobs: Jobs<Work>,
    generation: u64,
    submit: bool,
    installed: Vec<u64>,
    error: Option<String>,
    start: Option<Receiver<()>>,
    finish: Option<Sender<()>>,
    pipeline: Option<wgpu::RenderPipeline>,
}

impl<const HOLD: bool> Default for JobHooks<HOLD> {
    fn default() -> Self {
        Self {
            jobs: Jobs::new(LIMITS),
            generation: 0,
            submit: true,
            installed: Vec::new(),
            error: None,
            start: None,
            finish: None,
            pipeline: None,
        }
    }
}

impl<const HOLD: bool> Hooks for JobHooks<HOLD> {
    const ALLOWANCE: u32 = 1;

    fn prepare(
        &mut self,
        gpu: &HookGpu<'_>,
        view: &FrameView<'_>,
        world: &RenderWorld<'_>,
    ) -> Result<(), RenderError> {
        if view.time.reset {
            self.generation += 1;
            self.jobs.clear();
            self.submit = true;
            self.installed.clear();
            self.error = None;
        }
        let mut results = self.jobs.take(gpu, view);
        if view.time.allowance > 0 && self.submit {
            let value = world.resource::<TestValue>().unwrap().0;
            let mut work = Work::value(u64::from(value) * 2);
            if (HOLD || value == SLOW) && self.finish.is_none() && !view.time.seekable {
                let gated = Work::gated(u64::from(value) * 2);
                work = gated.0;
                work.ignore_cancel = true;
                self.start = Some(gated.1);
                self.finish = Some(gated.2);
            }
            work.fail = value == FAIL;
            results.submit(0, self.generation, 0, work).unwrap();
            self.submit = false;
        }
        for done in results {
            assert_eq!(done.generation, self.generation);
            match done.result {
                Ok(value) => self.installed.push(value),
                Err(error) => self.error = Some(error.0),
            }
        }
        if !self.installed.is_empty() {
            self.pipeline.get_or_insert_with(|| triangle(gpu, view));
        }
        Ok(())
    }

    fn opaque(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        _: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        if let (Some(value), Some(pipeline)) = (self.installed.last(), &self.pipeline) {
            pass.set_pipeline(pipeline);
            let triangle = ((*value / 2) % 2) as u32 * 3;
            pass.draw(triangle..triangle + 3, 0..1);
        }
        Ok(())
    }

    fn needs(&self) -> Needs {
        if self.error.is_none() && (self.submit || self.jobs.pending() > 0) {
            Needs::PENDING | Needs::STARVED
        } else {
            Needs::NONE
        }
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn jobs(&self) -> Option<JobsReport> {
        Some(self.jobs.report())
    }
}

fn triangle(gpu: &HookGpu<'_>, view: &FrameView<'_>) -> wgpu::RenderPipeline {
    let module = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(
                r#"
            @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
                let p = array(vec2f(-0.7, -0.6), vec2f(0.7, -0.6), vec2f(0., 0.7));
                return vec4f(p[i % 3] + vec2f(f32(i / 3) * 0.1, 0.), 0.01, 1.);
            }
            @fragment fn fs() -> @location(0) vec4f { return vec4f(0., 1., 0., 1.); }
        "#
                .into(),
            ),
        });
    gpu.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            cache: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(view.color_format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: view.depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: view.sample_count,
                ..Default::default()
            },
            multiview_mask: None,
        })
}

#[derive(Clone, Copy, Default, exact_game::Resource)]
struct TestValue(u32);

type Canvas = WorldSurface<TestGame, (), false, JobHooks>;

fn canvas(value: u32, paused: bool, seekable: bool) -> Canvas {
    let mut canvas = Canvas::default();
    canvas
        .bind(
            &[
                Value::Number(value.into()),
                Value::Bool(paused),
                Value::Bool(false),
            ],
            None,
        )
        .unwrap();
    canvas.clock(seekable);
    canvas
}

fn draw(gpu: &Gpu, canvas: &mut dyn Surface, now_ms: f64, seekable: bool) -> Vec<u8> {
    draw_frame(gpu, canvas, now_ms, seekable).0
}

fn draw_frame(gpu: &Gpu, canvas: &mut dyn Surface, now_ms: f64, seekable: bool) -> (Vec<u8>, bool) {
    let frame = Frame {
        width: 64.,
        height: 64.,
        scale: 1.,
        now_ms,
        seekable,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    };
    let (pixels, wants) = fixture::render(gpu, canvas, &frame).unwrap();
    (pixels.data, wants)
}

fn wait_to_install<const HOLD: bool>(hooks: &JobHooks<HOLD>) {
    if hooks.installed.is_empty() {
        hooks.jobs.wait_for_completions(1);
    }
}

#[test]
fn jobs_world_pause_installs_without_ticking() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut canvas = canvas(SLOW, true, false);
    let (_, wants) = draw_frame(&gpu, &mut canvas, 0., false);
    let hooks = canvas.render_hooks();
    hooks.start.as_ref().unwrap().recv_timeout(TIMEOUT).unwrap();
    hooks.finish.as_ref().unwrap().send(()).unwrap();
    if wants {
        wait_to_install(hooks);
        draw(&gpu, &mut canvas, 17., false);
    }
    assert_eq!(canvas.render_hooks().installed, [u64::from(SLOW) * 2]);
    assert_eq!(canvas.sim().unwrap().world().tick(), 0);
}

#[test]
fn jobs_world_restore_drops_an_in_flight_generation() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut canvas = canvas(SLOW, false, false);
    draw(&gpu, &mut canvas, 0., false);
    let hooks = canvas.render_hooks();
    hooks.start.as_ref().unwrap().recv_timeout(TIMEOUT).unwrap();
    let finish = hooks.finish.as_ref().unwrap().clone();
    let generation = hooks.generation;
    let saved = Sim::<TestGame>::new(Options {
        value: 5,
        ..Options::default()
    })
    .unwrap()
    .save()
    .unwrap();
    canvas.restore(&saved, Restore::Open).unwrap();
    draw(&gpu, &mut canvas, 17., false);
    finish.send(()).unwrap();
    // The one worker drops the old generation's result as it returns, then runs the new job.
    canvas.render_hooks().jobs.wait_for_completions(1);
    draw(&gpu, &mut canvas, 34., false);
    assert!(canvas.render_hooks().generation > generation);
    assert_eq!(canvas.render_hooks().installed, [10]);
    assert_eq!(canvas.render_hooks().jobs.report().stale, 1);
}

#[test]
fn jobs_world_look_leaves_the_next_picture_and_pick_unchanged() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let drive = |looks| {
        let mut canvas = canvas(5, false, true);
        let before = draw(&gpu, &mut canvas, 0., true);
        for _ in 0..looks {
            let reply = canvas
                .agent(r#"{"op":"state","look":true,"now":0}"#)
                .unwrap();
            assert!(reply.contains("\"look\""), "{reply}");
            assert!(canvas.render_hooks().installed.is_empty());
        }
        let after = draw(&gpu, &mut canvas, 17., true);
        assert_ne!(before, after);
        let center = (32 * 64 + 32) * 4;
        assert!(
            after[center + 1] > after[center],
            "the job triangle covers the pick pixel"
        );
        let sim = canvas.sim().unwrap();
        assert_eq!(
            sim.pick(Vec2::splat(32.)).unwrap().entity,
            sim.world().named("box").unwrap()
        );
        assert_eq!(canvas.render_hooks().installed, [10]);
        after
    };
    assert_eq!(drive(20), drive(0));
}

#[test]
fn jobs_world_failure_clears_pending_and_reports_error() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut canvas = canvas(FAIL, false, true);
    draw(&gpu, &mut canvas, 0., true);
    draw(&gpu, &mut canvas, 17., true);
    assert!(!canvas.render_hooks().needs().contains(Needs::PENDING));
    assert_eq!(canvas.render_hooks().error(), Some("fixture failure"));
    assert!(canvas
        .ready_reasons()
        .iter()
        .any(|reason| reason.contains("fixture failure")));
}

#[test]
fn jobs_world_canvases_install_only_their_own_results() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut first = canvas(7, false, false);
    let mut second = canvas(11, false, false);
    for canvas in [&mut first, &mut second] {
        draw(&gpu, canvas, 0., false);
    }
    for canvas in [&mut second, &mut first] {
        wait_to_install(canvas.render_hooks());
        if canvas.render_hooks().installed.is_empty() {
            draw(&gpu, canvas, 17., false);
        }
    }
    assert_eq!(first.render_hooks().installed, [14]);
    assert_eq!(second.render_hooks().installed, [22]);
}

#[test]
fn jobs_world_device_loss_replaces_the_hook_with_a_job_running() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut canvas = canvas(SLOW, false, false);
    draw(&gpu, &mut canvas, 0., false);
    let hooks = canvas.render_hooks();
    hooks.start.as_ref().unwrap().recv_timeout(TIMEOUT).unwrap();
    let finish = hooks.finish.as_ref().unwrap().clone();
    let shared = hooks.jobs.shared.clone();
    let release = std::thread::spawn(move || {
        let state = shared.state.lock().unwrap();
        let (state, _) = shared
            .changed
            .wait_timeout_while(state, TIMEOUT, |state| !state.closed)
            .unwrap();
        let closed = state.closed;
        drop(state);
        finish.send(()).unwrap();
        assert!(closed, "the job finished before the device was lost");
    });
    canvas.device_lost();
    release.join().unwrap();
    let hooks = canvas.render_hooks();
    assert!(hooks.installed.is_empty());
    assert_eq!((hooks.jobs.pending(), hooks.jobs.report().done), (0, 0));

    canvas
        .bind(
            &[Value::Number(5.), Value::Bool(false), Value::Bool(false)],
            None,
        )
        .unwrap();
    draw(&gpu, &mut canvas, 17., false);
    wait_to_install(canvas.render_hooks());
    if canvas.render_hooks().installed.is_empty() {
        draw(&gpu, &mut canvas, 34., false);
    }
    assert_eq!(canvas.render_hooks().installed, [10]);
}
