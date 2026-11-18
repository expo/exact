use super::*;

const STATE: &str = r#"{"op":"state"}"#;
const ARM: &str = r#"{"op":"state","perf":true}"#;
const CPU_RINGS: [&str; 4] = ["tickMs", "feedMs", "encodeMs", "pipelineMs"];

struct TimingGame;
impl Game for TimingGame {
    const ID: &'static str = "surface-cpu-timing";
    const HZ: u32 = 10;
    type Args = MoveArgs;
    fn setup(world: &mut World, args: &MoveArgs) {
        Move::setup(world, args);
    }
    fn tick(world: &mut World, input: &Input, args: &MoveArgs) {
        Move::tick(world, input, args);
    }
}

#[derive(Default)]
struct TimingHooks;
impl crate::Hooks for TimingHooks {
    fn needs(&self) -> crate::Needs {
        crate::Needs::SCENE_COPY | crate::Needs::HDR_POST
    }
}

fn timing_state(surface: &mut dyn Surface, request: &str) -> serde_json::Value {
    let reply = surface.agent(request).unwrap();
    let mut state: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert!(state.get("error").is_none(), "{state}");
    state["world"].take()
}

#[test]
fn agent_cpu_timing_samples_headless_ticks_only_when_armed() {
    use crate::perf::CLOCK_READS;
    let mut surface = WorldSurface::<TimingGame>::default();
    surface
        .bind(&[Value::Bool(true), Value::Number(0.)], None)
        .unwrap();
    surface.clock(true);
    surface.agent(r#"{"op":"clock","now":0}"#).unwrap();
    timing_state(&mut surface, ARM);
    surface.agent(r#"{"op":"clock","now":100}"#).unwrap();
    let state = timing_state(&mut surface, STATE);
    assert_eq!(state["tick"], 1);
    assert_duration(&state["perf"]["tickMs"], 1);
    assert_eq!(state["perf"]["clock"], "agent");
    for ring in ["frameMs", "feedMs", "encodeMs", "pipelineMs"] {
        assert_eq!(state["perf"][ring]["count"], 0, "{ring}");
    }

    surface.clock(false);
    let closed = timing_state(&mut surface, STATE);
    assert_same_cpu_window(&state, &closed);

    surface
        .bind(&[Value::Bool(true), Value::Number(1.)], None)
        .unwrap();
    surface.clock(true);
    CLOCK_READS.with(|n| n.set(0));
    surface.agent(r#"{"op":"clock","now":100}"#).unwrap();
    surface.agent(r#"{"op":"clock","now":200}"#).unwrap();
    let state = timing_state(&mut surface, STATE);
    assert_eq!(state["tick"], 1);
    assert_eq!(CLOCK_READS.with(|n| n.get()), 0);
    assert_eq!(state["perf"]["armed"], false);
    assert_eq!(state["perf"]["tickMs"]["count"], 0);
}

fn assert_duration(ring: &serde_json::Value, count: u64) {
    assert_eq!(ring["count"], count, "{ring}");
    if count > 0 {
        assert!(
            ring["max"]
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= 0.),
            "{ring}"
        );
        assert!(
            ring["p50"]
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= 0.),
            "{ring}"
        );
    }
}

fn assert_same_cpu_window(before: &serde_json::Value, after: &serde_json::Value) {
    for field in CPU_RINGS.into_iter().chain(["clock", "frameMs"]) {
        assert_eq!(after["perf"][field], before["perf"][field], "{field}");
    }
    assert_eq!(
        after["renderHooks"]["cpuMs"],
        before["renderHooks"]["cpuMs"]
    );
}

fn assert_cpu_rings<H: crate::Hooks>(world: &serde_json::Value, samples: u64) {
    for ring in CPU_RINGS {
        assert_duration(&world["perf"][ring], samples);
    }
    if H::ENABLED {
        for stage in crate::hooks::metrics::NAMES {
            assert_duration(&world["renderHooks"]["cpuMs"][stage], samples);
        }
    } else {
        assert!(world.get("renderHooks").is_none(), "{world}");
    }
}

fn timing_surface<H: crate::Hooks>() -> WorldSurface<TimingGame, (), false, H> {
    let mut surface = WorldSurface::default();
    surface
        .bind(&[Value::Bool(true), Value::Number(0.)], None)
        .unwrap();
    surface
}

fn live_frame(now_ms: f64) -> Frame {
    Frame {
        seekable: false,
        ..frame(now_ms)
    }
}

fn armed_live_surface<H: crate::Hooks>(gpu: &Gpu) -> WorldSurface<TimingGame, (), false, H> {
    let mut surface = timing_surface();
    surface.clock(false);
    timing_state(&mut surface, ARM);
    for now in [0., 100.] {
        fixture::render(gpu, &mut surface, &live_frame(now)).unwrap();
    }
    surface
}

fn check_agent_cpu_timing<H: crate::Hooks>() {
    use crate::perf::CLOCK_READS;
    let Some(gpu) = gpu() else { return };
    let mut surface = armed_live_surface::<H>(&gpu);
    let live = timing_state(&mut surface, STATE);
    assert_eq!(live["perf"]["clock"], "live");
    assert_duration(&live["perf"]["frameMs"], 1);
    assert_duration(&live["perf"]["encodeMs"], 2);
    assert_duration(&live["perf"]["pipelineMs"], 2);

    surface.clock(true);
    assert_same_cpu_window(&live, &timing_state(&mut surface, STATE));
    surface.agent(r#"{"op":"clock","now":200}"#).unwrap();
    let advanced = timing_state(&mut surface, STATE);
    assert_eq!(advanced["perf"]["clock"], "agent");
    assert_duration(&advanced["perf"]["tickMs"], 1);
    assert_duration(&advanced["perf"]["feedMs"], 1);
    for ring in ["encodeMs", "pipelineMs"] {
        assert_duration(&advanced["perf"][ring], 0);
    }
    if H::ENABLED {
        for stage in crate::hooks::metrics::NAMES {
            assert_duration(&advanced["renderHooks"]["cpuMs"][stage], 0);
        }
    }
    fixture::render(&gpu, &mut surface, &frame(200.)).unwrap();
    let agent = timing_state(&mut surface, STATE);
    assert_cpu_rings::<H>(&agent, 1);
    assert_eq!(agent["perf"]["frameMs"], live["perf"]["frameMs"]);
    for ring in ["ticksPerFrame", "ticksOwed"] {
        assert_eq!(agent["perf"][ring]["count"], 3, "{ring}");
    }

    surface.clock(false);
    assert_same_cpu_window(&agent, &timing_state(&mut surface, STATE));
    fixture::render(&gpu, &mut surface, &live_frame(300.)).unwrap();
    let live_again = timing_state(&mut surface, STATE);
    assert_cpu_rings::<H>(&live_again, 1);
    assert_eq!(live_again["perf"]["clock"], "live");
    assert_eq!(live_again["perf"]["frameMs"], live["perf"]["frameMs"]);

    surface
        .bind(&[Value::Bool(true), Value::Number(1.)], None)
        .unwrap();
    surface.clock(true);
    CLOCK_READS.with(|n| n.set(0));
    fixture::render(&gpu, &mut surface, &frame(300.)).unwrap();
    fixture::render(&gpu, &mut surface, &frame(400.)).unwrap();
    let restarted = timing_state(&mut surface, STATE);
    assert_eq!(CLOCK_READS.with(|n| n.get()), 0);
    assert_eq!(restarted["perf"]["armed"], false);
    assert_cpu_rings::<H>(&restarted, 0);
}

#[test]
fn agent_cpu_timing_on_a_hookless_surface() {
    check_agent_cpu_timing::<()>();
}

#[test]
fn agent_cpu_timing_on_a_hooked_surface() {
    check_agent_cpu_timing::<TimingHooks>();
}

#[test]
fn an_armed_timed_bind_samples_the_tick_and_feed() {
    let Some(gpu) = gpu() else { return };
    let mut surface = timing_surface::<()>();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    timing_state(&mut surface, ARM);
    surface
        .bind(&[Value::Bool(false), Value::Number(0.)], Some(100.))
        .unwrap();
    let world = timing_state(&mut surface, STATE);
    assert_eq!(world["tick"], 1);
    assert_eq!(world["perf"]["clock"], "agent");
    assert_duration(&world["perf"]["tickMs"], 1);
    assert_duration(&world["perf"]["feedMs"], 1);
}

#[test]
fn a_refused_stamp_keeps_the_live_window() {
    let Some(gpu) = gpu() else { return };
    let mut surface = armed_live_surface::<TimingHooks>(&gpu);
    let live = timing_state(&mut surface, STATE);
    surface.clock(true);
    for now in [200., 1e20] {
        let request = format!(r#"{{"op":"state","width":64,"now":{now}}}"#);
        let reply = surface.agent(&request).unwrap();
        assert!(
            reply.contains("width and height must be supplied together"),
            "{reply}"
        );
        let after = timing_state(&mut surface, STATE);
        assert_eq!(after["tick"], live["tick"]);
        assert_same_cpu_window(&live, &after);
    }
}

#[test]
fn a_refused_stamp_runs_no_work_or_cpu_clock_headless() {
    use crate::perf::CLOCK_READS;
    let mut surface = timing_surface::<()>();
    surface.agent(r#"{"op":"clock","now":-1000}"#).unwrap();
    timing_state(&mut surface, ARM);
    surface.agent(r#"{"op":"clock","now":-900}"#).unwrap();
    let before = timing_state(&mut surface, STATE);
    CLOCK_READS.with(|n| n.set(0));
    let reply = surface
        .agent(r#"{"op":"state","width":64,"now":1e20}"#)
        .unwrap();
    assert!(
        reply.contains("width and height must be supplied together"),
        "{reply}"
    );
    assert_eq!(CLOCK_READS.with(|n| n.get()), 0);
    let after = timing_state(&mut surface, STATE);
    assert_eq!(after["tick"], before["tick"]);
    assert_same_cpu_window(&before, &after);
}

#[test]
fn armed_agent_work_keeps_the_contract_snapshot_and_save_equal() {
    let Some(gpu) = gpu() else { return };
    let drive = |armed| {
        let mut surface = timing_surface::<()>();
        surface.set_performance_reporting(true);
        if armed {
            timing_state(&mut surface, ARM);
        }
        fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
        surface.agent(r#"{"op":"clock","now":100}"#).unwrap();
        surface
            .bind(&[Value::Bool(false), Value::Number(0.)], Some(200.))
            .unwrap();
        fixture::render(&gpu, &mut surface, &frame(300.)).unwrap();
        let world = timing_state(&mut surface, STATE);
        assert_eq!(world["tick"], 3);
        assert_duration(&world["perf"]["tickMs"], if armed { 3 } else { 0 });
        // GPU query results depend on when the device finishes the frame.
        surface.gpu_timing = None;
        let snapshot: serde_json::Value =
            serde_json::from_str(&surface.take_performance(300.).unwrap()).unwrap();
        assert_eq!(snapshot["frames"], 2);
        (surface.sim().unwrap().save().unwrap(), snapshot)
    };
    let (unarmed_save, unarmed_snapshot) = drive(false);
    let (armed_save, armed_snapshot) = drive(true);
    assert_eq!(armed_save, unarmed_save);
    assert_eq!(armed_snapshot, unarmed_snapshot);
}

#[test]
fn switching_cpu_windows_reuses_all_ten_rings() {
    let Some(gpu) = gpu() else { return };
    let mut surface = armed_live_surface::<TimingHooks>(&gpu);
    surface.perf.tick.values.reserve_exact(1);
    let storage = |surface: &WorldSurface<TimingGame, (), false, TimingHooks>| {
        let perf = &surface.perf;
        [&perf.tick, &perf.feed, &perf.encode, &perf.pipeline]
            .map(|ring| (ring.values.as_ptr(), ring.values.capacity()))
            .into_iter()
            .chain(
                surface
                    .render
                    .as_ref()
                    .unwrap()
                    .0
                    .hook_metrics
                    .as_ref()
                    .unwrap()
                    .cpu_storage(),
            )
            .collect::<Vec<_>>()
    };
    let before = storage(&surface);
    surface.clock(true);
    surface.agent(r#"{"op":"clock","now":200}"#).unwrap();
    let agent = timing_state(&mut surface, STATE);
    assert_duration(&agent["perf"]["tickMs"], 1);
    assert_duration(&agent["perf"]["feedMs"], 1);
    for ring in ["encodeMs", "pipelineMs"] {
        assert_duration(&agent["perf"][ring], 0);
    }
    for stage in crate::hooks::metrics::NAMES {
        assert_duration(&agent["renderHooks"]["cpuMs"][stage], 0);
    }
    assert_eq!(storage(&surface), before);
}

#[test]
fn stamped_requests_on_a_live_surface_preserve_its_cpu_window_and_cadence() {
    let Some(gpu) = gpu() else { return };
    let mut surface = armed_live_surface::<TimingHooks>(&gpu);
    let live = timing_state(&mut surface, STATE);
    let stamped = timing_state(&mut surface, r#"{"op":"state","now":200}"#);
    assert_eq!(stamped["tick"], 2);
    assert_same_cpu_window(&live, &stamped);
    let reply = surface.agent(r#"{"op":"clock","now":300}"#).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reply).unwrap()["tick"],
        3
    );
    assert_same_cpu_window(&live, &timing_state(&mut surface, STATE));
    fixture::render(&gpu, &mut surface, &live_frame(316.)).unwrap();
    let next = timing_state(&mut surface, STATE);
    assert_eq!(next["perf"]["clock"], "live");
    assert_duration(&next["perf"]["frameMs"], 2);
    assert_eq!(next["perf"]["frameMs"]["max"], 216.);
}

#[test]
fn a_clock_round_trip_between_live_frames_keeps_the_display_interval() {
    let Some(gpu) = gpu() else { return };
    let mut surface = timing_surface::<TimingHooks>();
    surface.clock(false);
    timing_state(&mut surface, ARM);
    fixture::render(&gpu, &mut surface, &live_frame(0.)).unwrap();
    surface.clock(true);
    surface.clock(false);
    fixture::render(&gpu, &mut surface, &live_frame(16.)).unwrap();
    let next = timing_state(&mut surface, STATE);
    assert_duration(&next["perf"]["frameMs"], 1);
    assert_eq!(next["perf"]["frameMs"]["p50"], 16.);
}

#[test]
fn a_look_frame_times_nothing_and_keeps_the_cpu_window() {
    let Some(gpu) = gpu() else { return };
    let mut surface = WorldSurface::<TimingGame, (), false, TimingHooks>::default();
    surface
        .bind(&[Value::Bool(true), Value::Number(0.)], None)
        .unwrap();
    surface.clock(true);
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    timing_state(&mut surface, ARM);
    fixture::render(&gpu, &mut surface, &frame(100.)).unwrap();
    let before = timing_state(&mut surface, STATE);
    assert_eq!(before["perf"]["encodeMs"]["count"], 1);
    let look = surface
        .agent(r#"{"op":"state","look":true,"now":100}"#)
        .unwrap();
    assert!(look.contains("\"look\""), "{look}");
    let after = timing_state(&mut surface, STATE);
    for ring in ["frameMs", "ticksPerFrame", "ticksOwed"]
        .into_iter()
        .chain(CPU_RINGS)
    {
        assert_eq!(after["perf"][ring], before["perf"][ring], "{ring}");
    }
    assert_eq!(
        after["renderHooks"]["cpuMs"],
        before["renderHooks"]["cpuMs"]
    );
    assert_eq!(after["perf"]["clock"], "agent");
    assert_eq!(after["perf"]["look"]["frames"], 1);
    assert_eq!(after["hash"], before["hash"]);
}

#[test]
fn contract_performance_is_opt_in_and_does_not_change_simulation() {
    let Some(gpu) = gpu() else { return };
    let mut surface = surface();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(surface.take_performance(0.).is_none());
    let saved = surface.sim().unwrap().save().unwrap();
    let hash = surface.sim().unwrap().world().hash();
    surface.set_performance_reporting(true);
    assert!(surface.take_performance(0.).is_none());
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&surface.take_performance(0.).unwrap()).unwrap();
    assert_eq!(report["frames"], 1);
    assert_eq!(report["pixelWidth"], 64);
    assert_eq!(report["pixelHeight"], 64);
    assert!(report["draws"].as_u64().unwrap() > 0);
    assert!(report["geometryMemoryBytes"].as_u64().unwrap() > 0);
    assert!(report.get("textureMemoryBytes").is_some());
    assert!(
        report.get("thermalState").is_none(),
        "the fixture's host says none"
    );
    assert!(
        report.get("audioState").is_none(),
        "the fixture links no audio"
    );
    assert!(report.get("gpuMs").is_none());
    assert!(report.get("fps").is_none());
    assert!(report.get("windowMs").is_none());
    assert!(
        report.get("lateFrames").is_none(),
        "virtual time has no cadence"
    );
    assert_eq!(surface.sim().unwrap().save().unwrap(), saved);
    assert_eq!(surface.sim().unwrap().world().hash(), hash);
    surface
        .bind(&[Value::Bool(true), Value::Number(1.)], None)
        .unwrap();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(
        surface.take_performance(0.).is_some(),
        "new setup retains opt-in"
    );
    let carry = surface.carry().unwrap().unwrap();
    surface.restore(&carry, exact_gpu::Restore::Carry).unwrap();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(
        surface.take_performance(0.).is_some(),
        "restore retains opt-in"
    );
    surface.set_performance_reporting(false);
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    assert!(surface.take_performance(0.).is_none());
}

#[test]
fn gpu_time_is_measured_only_for_a_reader_and_only_with_timestamps() {
    let Some(gpu) = gpu() else { return };
    let timestamps = gpu
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY);
    let mut surface = surface();
    fixture::render(&gpu, &mut surface, &frame(0.)).unwrap();
    fixture::render(&gpu, &mut surface, &frame(167.)).unwrap();
    assert!(surface.gpu_timing.is_none(), "no reader, no timestamp work");
    surface.set_performance_reporting(true);
    for i in 2..8 {
        fixture::render(&gpu, &mut surface, &frame(i as f64 * 167.)).unwrap();
        // The frame finishes; the next frame's poll resolves and maps it.
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }
    assert_eq!(surface.gpu_timing.is_some(), timestamps);
    assert!(
        surface.gpu_timing.as_ref().is_none_or(|t| !t.armed()),
        "a reader's timing keeps no agent samples"
    );
    // At rest, the snapshot's own poll publishes the last mapped frame.
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&surface.take_performance(0.).unwrap()).unwrap();
    if timestamps {
        let (mean, max) = (report["gpuMs"].as_f64(), report["gpuMaxMs"].as_f64());
        assert!(mean.is_some_and(|ms| ms > 0.) && max >= mean, "{report}");
    } else {
        assert!(report.get("gpuMs").is_none(), "no timestamp queries: none");
    }
    let state = surface.agent(r#"{"op":"state"}"#).unwrap();
    assert!(
        !state.contains("\"gpuMs\""),
        "a reader's timing is not the agent's"
    );
    surface.set_performance_reporting(false);
    assert!(
        surface.gpu_timing.is_none(),
        "the last reader gone, none left"
    );
    fixture::render(&gpu, &mut surface, &frame(9. * 167.)).unwrap();
    assert!(surface.gpu_timing.is_none());
    surface.set_performance_reporting(true);
    fixture::render(&gpu, &mut surface, &frame(10. * 167.)).unwrap();
    surface.agent(r#"{"op":"state","perf":true}"#).unwrap();
    assert!(
        surface.gpu_timing.is_none(),
        "arming perf drops a reader's sample-less timing at once"
    );
}

#[test]
fn world_perf_difference_reads_only_the_requested_ticks_over_the_wire() {
    #[derive(Default, exact_game::Data)]
    struct Sample {
        count: u64,
    }
    #[allow(non_snake_case)]
    #[derive(Default, exact_game::Data)]
    struct Spans {
        incarnation: String,
        generation: u64,
        spanTicks: u64,
        spans: Vec<Sample>,
        error: String,
    }
    #[allow(non_snake_case)]
    #[derive(Default, exact_game::Data)]
    struct Metrics {
        tickSpans: Spans,
    }
    #[derive(Default, exact_game::Data)]
    struct WorldState {
        perf: Metrics,
    }
    #[derive(Default, exact_game::Data)]
    struct Reply {
        world: WorldState,
    }
    let mut s = surface();
    s.agent(r#"{"op":"clock","now":0}"#);
    s.agent(r#"{"op":"state","perf":true}"#);
    s.agent(r#"{"op":"clock","now":34}"#);
    let read = |text: String| {
        exact_game::json::from_str::<Reply>(&text)
            .unwrap()
            .world
            .perf
            .tickSpans
    };
    let first = read(s.agent(r#"{"op":"state","perf":true}"#).unwrap());
    let request = |ticks| {
        format!(
            r#"{{"op":"state","perf":true,"span_since":{{"incarnation":{},"generation":{},"ticks":{ticks}}}}}"#,
            serde_json::to_string(&first.incarnation).unwrap(),
            first.generation
        )
    };
    let window = read(s.agent(&request(1)).unwrap());
    assert!(!window.spans.is_empty());
    assert!(window.spans.iter().all(|s| s.count == 1));
    let all = read(s.agent(&request(0)).unwrap());
    assert!(all.spans.iter().all(|s| s.count == 2));
    let backwards = read(s.agent(&request(3)).unwrap());
    assert!(backwards.error.contains("counters went back"));
    s.agent(r#"{"op":"state","perf_reset":true}"#);
    s.agent(r#"{"op":"clock","now":68}"#);
    let reset = read(s.agent(r#"{"op":"state","perf":true}"#).unwrap());
    assert_eq!(reset.spanTicks, first.spanTicks);
    assert_eq!(reset.incarnation, first.incarnation);
    assert_ne!(reset.generation, first.generation);
    assert!(read(s.agent(&request(2)).unwrap())
        .error
        .contains("recording generation"));
    let saved = s.carry().unwrap().unwrap();
    s.restore(&saved, exact_gpu::Restore::Carry).unwrap();
    assert!(read(s.agent(&request(2)).unwrap())
        .error
        .contains("world incarnation"));
    s = surface();
    assert!(read(s.agent(&request(2)).unwrap())
        .error
        .contains("world incarnation"));
}
