use crate::Stats;
use std::fmt::Write;
#[cfg(test)]
thread_local! { pub(crate) static CLOCK_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[derive(Clone, Copy, Debug)]
pub(crate) struct Stamp {
    #[cfg(not(target_arch = "wasm32"))]
    time: std::time::Instant,
    #[cfg(target_arch = "wasm32")]
    time: f64,
}
impl Stamp {
    pub fn now() -> Self {
        #[cfg(test)]
        CLOCK_READS.with(|n| n.set(n.get() + 1));
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            time: std::time::Instant::now(),
            #[cfg(target_arch = "wasm32")]
            time: performance_now(),
        }
    }
    pub fn elapsed_at(&self, end: Self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            end.time.duration_since(self.time).as_secs_f64() * 1000.0
        }
        #[cfg(target_arch = "wasm32")]
        {
            end.time - self.time
        }
    }

    pub fn wall_ms(&self) -> f64 {
        #[cfg(target_arch = "wasm32")]
        {
            performance_now()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.time.elapsed().as_secs_f64() * 1000.0
        }
    }
    pub fn elapsed(&self) -> f64 {
        #[cfg(test)]
        CLOCK_READS.with(|n| n.set(n.get() + 1));
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.time.elapsed().as_secs_f64() * 1000.0
        }
        #[cfg(target_arch = "wasm32")]
        {
            performance_now() - self.time
        }
    }
}
#[cfg(target_arch = "wasm32")]
fn performance_now() -> f64 {
    use web_sys::js_sys::{global, Reflect};
    use web_sys::wasm_bindgen::{JsCast, JsValue};
    thread_local! {
        static PERFORMANCE: web_sys::Performance = Reflect::get(&global(), &JsValue::from_str("performance"))
            .and_then(JsCast::dyn_into).expect("game rendering requires globalThis.performance");
    }
    PERFORMANCE.with(web_sys::Performance::now)
}
thread_local! {
    // Whether this thread times pipeline creation, and the milliseconds it
    // has spent creating pipelines since the last `pipeline_ms`.
    static PIPELINES: std::cell::Cell<(bool, f64)> = const { std::cell::Cell::new((false, 0.)) };
}
/// Times pipeline creation on this thread until the scope ends.
pub(crate) struct PipelineTiming;
impl PipelineTiming {
    pub fn start(on: bool) -> Self {
        PIPELINES.with(|p| p.set((on, 0.)));
        Self
    }
}
impl Drop for PipelineTiming {
    fn drop(&mut self) {
        PIPELINES.with(|p| p.set((false, p.get().1)));
    }
}
/// Whether this thread is timing pipeline creation now.
pub(crate) fn timing() -> bool {
    PIPELINES.with(|p| p.get().0)
}
/// The milliseconds this thread spent creating pipelines since the last call,
/// or since its `PipelineTiming` started.
pub(crate) fn pipeline_ms() -> f64 {
    PIPELINES.with(|p| {
        let (on, ms) = p.get();
        p.set((on, 0.));
        ms
    })
}
/// Create a pipeline, adding its time to this thread's while timing is on.
/// A worker thread compiling a variant never times it: it stalls no frame.
pub(crate) fn pipeline<T>(make: impl FnOnce() -> T) -> T {
    if !PIPELINES.with(|p| p.get().0) {
        return make();
    }
    let start = Stamp::now();
    let made = make();
    let ms = start.elapsed();
    PIPELINES.with(|p| {
        let (on, sum) = p.get();
        p.set((on, sum + ms));
    });
    made
}
pub(crate) use exact_game::diagnostics::Ring;
use exact_game::LiveGap;

// LLP 1182.000 D5.
#[derive(Default)]
struct CatchUp {
    overload_frames: u64,
    clamp_frames: u64,
    clamp_ms: f64,
    idle_frames: u64,
    idle_ms: f64,
    warned: bool,
}

#[derive(Default)]
pub(crate) struct CatchUpGap {
    owed: Option<u64>,
    clamped_ms: f64,
    pub idle_ms: f64,
}

pub(crate) fn warn_catch_up(message: Option<String>) {
    if let Some(message) = message {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::warn_1(&message.into());
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!("{message}");
    }
}

#[derive(Default)]
pub(crate) struct Perf {
    pub reporting: Option<crate::surface::performance::Report>,
    frame: Ring,
    pub tick: Ring,
    pub feed: Ring,
    pub encode: Ring,
    /// Of the frame's feed and encode, the time creating pipelines on its thread.
    pub pipeline: Ring,
    /// Pipeline time from renderer creation to the first frame log, if measured.
    pub pipeline_ready: Option<f64>,
    pub pipeline_ready_seekable: bool,
    /// Pipeline time for the next drawn frame of each clock mode.
    pipeline_pending: [f64; 2],
    /// Whether the last rendered frame was live, for submission accounting.
    pub live: bool,
    pub rendering: bool,
    /// Reused for pipeline warm-up after submission.
    pub measure_frame: bool,
    cpu_seekable: bool,
    pub ticks: Ring,
    ticks_owed: Ring,
    catch_up: CatchUp,
    /// Picture generation of the preceding submitted live frame if it was playing.
    pub submitted_live: Option<u64>,
    last_live: Option<f64>,
    pub stats: Stats,
    pub pixels: (u32, u32),
    /// Instances drawn per view (camera, cascades 0–2) in the last read frame.
    pub culled: Option<[u64; 4]>,
    /// Camera-view triangles the GPU cull kept in that frame (levels of detail
    /// and frustum culling applied, unlike `stats.triangles`).
    pub culled_triangles: Option<u64>,
    /// Local lights drawn and dropped beyond `MAX_LIGHTS` in the last frame.
    lights: (usize, usize),
    /// Look frames drawn (LLP 1127.000) and the bytes they read back.
    pub look: (u64, u64),
    warned: bool,
}
impl Perf {
    pub fn armed(&self) -> bool {
        !self.frame.values.is_empty()
    }
    // LLP 1182.000 D3. An unsampled clock change leaves the window readable.
    pub fn begin_cpu_sample(
        &mut self,
        seekable: bool,
        hooks: Option<&mut crate::hooks::metrics::Metrics>,
    ) {
        if self.cpu_seekable == seekable || !self.armed() {
            return;
        }
        self.cpu_seekable = seekable;
        for ring in [
            &mut self.tick,
            &mut self.feed,
            &mut self.encode,
            &mut self.pipeline,
        ] {
            ring.clear();
        }
        if let Some(hooks) = hooks {
            hooks.clear_cpu();
        }
    }
    pub fn keep_pipeline_time(&mut self, seekable: bool) {
        self.pipeline_pending[usize::from(seekable)] += pipeline_ms();
    }
    pub fn frame_pipeline_time(&mut self, seekable: bool) -> f64 {
        pipeline_ms() + std::mem::take(&mut self.pipeline_pending[usize::from(seekable)])
    }
    pub fn arm(&mut self) {
        for r in [
            &mut self.frame,
            &mut self.tick,
            &mut self.feed,
            &mut self.encode,
            &mut self.pipeline,
            &mut self.ticks,
            &mut self.ticks_owed,
        ] {
            r.values.resize(16384, 0.0);
        }
    }
    pub fn reset(&mut self) {
        for r in [
            &mut self.frame,
            &mut self.tick,
            &mut self.feed,
            &mut self.encode,
            &mut self.pipeline,
            &mut self.ticks,
            &mut self.ticks_owed,
        ] {
            r.values.resize(16384, 0.0);
            r.clear();
        }
        self.last_live = None;
        self.catch_up = CatchUp {
            warned: self.catch_up.warned,
            ..Default::default()
        };
    }
    pub fn restart(&mut self) {
        *self = Self {
            reporting: self.reporting.take().map(|_| Default::default()),
            ..Default::default()
        };
    }
    pub fn frame(&mut self, now: f64, seekable: bool) {
        if seekable {
            self.last_live = None;
            return;
        }
        if let Some(last) = self.last_live {
            self.frame.push(now - last);
        }
        self.last_live = Some(now);
    }
    pub fn record_ticks<G: exact_game::Game>(
        &mut self,
        frame: &exact_gpu::Frame,
        ticks: u32,
        gap: CatchUpGap,
    ) -> Option<String> {
        self.ticks.push(f64::from(ticks));
        if frame.seekable {
            self.ticks_owed.push(f64::from(ticks));
            return None;
        }
        self.ticks_owed
            .push(gap.owed.unwrap_or(u64::from(ticks)) as f64);
        self.record_overload::<G>(frame.period_ms, ticks, gap)
    }
    pub fn record_gap(&mut self, note: Option<LiveGap>, picture: u64) -> CatchUpGap {
        let (excess_ms, owed, continued) = match note {
            Some(LiveGap::Advance {
                excess_ms,
                ticks_owed,
                paused,
            }) => (
                excess_ms,
                // A paused clock asks for no ticks, whatever the gap.
                (!paused).then_some(ticks_owed),
                !paused && self.submitted_live == Some(picture),
            ),
            Some(LiveGap::Wake { excess_ms }) => (excess_ms, None, false),
            None => return CatchUpGap::default(),
        };
        if continued {
            self.catch_up.clamp_frames += 1;
            self.catch_up.clamp_ms += excess_ms;
            CatchUpGap {
                owed,
                clamped_ms: excess_ms,
                idle_ms: 0.0,
            }
        } else {
            self.catch_up.idle_frames += 1;
            self.catch_up.idle_ms += excess_ms;
            CatchUpGap {
                owed,
                clamped_ms: 0.0,
                idle_ms: excess_ms,
            }
        }
    }
    pub fn record_overload<G: exact_game::Game>(
        &mut self,
        period_ms: f64,
        ticks: u32,
        gap: CatchUpGap,
    ) -> Option<String> {
        let ratio = period_ms * (f64::from(G::HZ) / 1000.0);
        if ratio > 0.0 && f64::from(ticks) > ratio.ceil() + 1.0 {
            self.catch_up.overload_frames += 1;
            if !std::mem::replace(&mut self.catch_up.warned, true) {
                let owed = gap.owed.unwrap_or(u64::from(ticks));
                let CatchUpGap {
                    clamped_ms,
                    idle_ms,
                    ..
                } = gap;
                return Some(format!(
                    "game tick overload: {owed} ticks owed, {ticks} run, {ratio:.3} ticks per display period, {clamped_ms:.3} ms clamped, {idle_ms:.3} ms idle. `perf world:<surface>` names the systems"
                ));
            }
        }
        None
    }
    /// Record the frame's light counts; the first frame that drops lights says so.
    pub fn lights(&mut self, drawn: usize, dropped: usize) {
        self.lights = (drawn, dropped);
        if dropped > 0 && !std::mem::replace(&mut self.warned, true) {
            let message = format!(
                "{dropped} local lights dropped beyond the {} nearest the camera",
                crate::MAX_LIGHTS
            );
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(&message.into());
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("{message}");
        }
    }
    pub fn append(&self, out: &mut String) {
        out.push_str(",\"perf\":{\"wallClock\":true");
        let clock = if self.cpu_seekable { "agent" } else { "live" };
        write!(out, ",\"clock\":\"{clock}\"").unwrap();
        write!(out, ",\"armed\":{}", self.armed()).unwrap();
        write!(out, ",\"pixels\":[{},{}]", self.pixels.0, self.pixels.1).unwrap();
        for (name, ring) in [
            ("frameMs", &self.frame),
            ("tickMs", &self.tick),
            ("feedMs", &self.feed),
            ("encodeMs", &self.encode),
            ("pipelineMs", &self.pipeline),
            ("ticksPerFrame", &self.ticks),
            ("ticksOwed", &self.ticks_owed),
        ] {
            write!(out, ",\"{name}\":").unwrap();
            ring.json(out);
        }
        let c = &self.catch_up;
        write!(
            out,
            ",\"catchUp\":{{\"overloadFrames\":{},\"clampFrames\":{},\"clampMs\":{},\"idleFrames\":{},\"idleMs\":{}}}",
            c.overload_frames, c.clamp_frames, c.clamp_ms, c.idle_frames, c.idle_ms
        )
        .unwrap();
        write!(
            out,
            ",\"draws\":{},\"instances\":{},\"triangles\":{},\"directFrames\":{}",
            self.stats.draws, self.stats.instances, self.stats.triangles, self.stats.direct_frames
        )
        .unwrap();
        write!(
            out,
            ",\"lights\":{{\"drawn\":{},\"dropped\":{}}}",
            self.lights.0, self.lights.1
        )
        .unwrap();
        write!(
            out,
            ",\"look\":{{\"frames\":{},\"readbackBytes\":{}}}",
            self.look.0, self.look.1
        )
        .unwrap();
        match self.culled {
            Some([camera, a, b, c]) => write!(
                out,
                ",\"culled\":{{\"camera\":{camera},\"cameraTriangles\":{},\"shadows\":[{a},{b},{c}]}}}}",
                self.culled_triangles.unwrap_or(0)
            ),
            None => write!(out, ",\"culled\":null}}"),
        }
        .unwrap();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use exact_game::{Clock, Game, Input, Sim, Value, World};
    use exact_gpu::Frame;
    use serde_json::{json, Value as Json};

    struct TestGame;
    #[derive(Default, exact_game::Args)]
    struct Args {
        #[live]
        paused: bool,
    }
    impl Game for TestGame {
        const ID: &'static str = "catch-up";
        const NAME: &'static str = "scene";
        const HZ: u32 = 120;
        type Args = Args;
        fn setup(_: &mut World, _: &Args) {}
        fn tick(_: &mut World, _: &Input, _: &Args) {}
        fn paused(args: &Args) -> bool {
            args.paused
        }
    }

    fn frame(now_ms: f64, period_ms: f64) -> Frame {
        Frame {
            width: 64.,
            height: 64.,
            scale: 1.,
            now_ms,
            period_ms,
            seekable: false,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.,
        }
    }

    fn state(perf: &Perf) -> Json {
        let mut out = String::from("{\"tick\":0");
        perf.append(&mut out);
        out.push('}');
        serde_json::from_str::<Json>(&out).unwrap()["perf"].take()
    }

    fn advance_gap(excess_ms: f64, ticks_owed: u64) -> Option<LiveGap> {
        Some(LiveGap::Advance {
            excess_ms,
            ticks_owed,
            paused: false,
        })
    }

    #[test]
    fn catch_up_counts_clamp_and_pause() {
        let mut sim = Sim::<TestGame>::new(Args::default()).unwrap();
        let mut perf = Perf::default();
        let mut advance = |sim: &mut Sim<TestGame>, now, submitted: bool| {
            let picture = sim.picture_generation();
            perf.submitted_live = submitted.then_some(picture);
            let ticks = sim.advance(now, Clock::Live);
            let gap = perf.record_gap(sim.take_live_gap(), picture);
            perf.record_ticks::<TestGame>(&frame(now, 0.0), ticks, gap);
        };
        for i in 0..=10 {
            advance(&mut sim, f64::from(i) * 300.0, i > 0);
        }
        sim.bind(&[Value::Bool(true)], None).unwrap();
        advance(&mut sim, 5000.0, true);
        assert_eq!(sim.world().tick(), 300);
        let state = state(&perf);
        assert_eq!(state["armed"], false);
        assert_eq!(
            state["catchUp"],
            json!({"clampFrames":10,"clampMs":500,"idleFrames":1,"idleMs":1750,"overloadFrames":0})
        );
        assert_eq!(state["ticksOwed"]["count"], 12);
        assert_eq!(state["ticksPerFrame"]["max"], 30);
        // The paused gap asks for none: the largest ask is a clamped frame's.
        assert_eq!(state["ticksOwed"]["max"], 36);
    }

    #[test]
    fn catch_up_excludes_seeks_and_classifies_first_and_resumed_frames_as_idle() {
        let mut perf = Perf::default();
        perf.arm();
        let mut sim = Sim::<TestGame>::new(Args::default()).unwrap();
        // Even with a prior stamp from a bind, the first frame follows no picture.
        sim.advance(0., Clock::Seekable);
        let mut now = 0.;
        for (gap, continued, resting) in [
            (2000., false, false),
            (250., true, false),
            (249., true, false),
            (2000., false, true),
        ] {
            now += gap;
            let picture = sim.picture_generation();
            perf.submitted_live = continued.then_some(picture);
            if resting {
                sim.wake(now);
                perf.record_gap(sim.take_live_gap(), picture);
            }
            let ticks = sim.advance(now, Clock::Live);
            let gap = perf.record_gap(sim.take_live_gap(), picture);
            perf.record_ticks::<TestGame>(&frame(now, 0.), ticks, gap);
        }
        now += 2000.;
        let mut seek = frame(now, 1000. / 120.);
        seek.seekable = true;
        let ticks = sim.advance(now, Clock::Seekable);
        assert!(ticks > 2);
        assert!(perf
            .record_ticks::<TestGame>(&seek, ticks, Default::default())
            .is_none());
        assert_eq!(perf.ticks_owed.count, 5);
        assert_eq!(perf.ticks.count, 5);
        assert_eq!(perf.ticks_owed.values[0], 240.);
        assert_eq!(&perf.ticks_owed.values[1..5], &perf.ticks.values[1..5]);
        assert_eq!(
            state(&perf)["catchUp"],
            json!({"clampFrames":0,"clampMs":0,"idleFrames":2,"idleMs":3500,"overloadFrames":0})
        );
    }

    #[test]
    fn catch_up_first_overload_reports_idle_time_and_canvas() {
        let mut perf = Perf::default();
        let display = frame(2000., 1000. / 60.);
        let gap = perf.record_gap(advance_gap(1750., 240), 0);
        let message = perf.record_ticks::<TestGame>(&display, 30, gap).unwrap();
        assert_eq!(message, "game tick overload: 240 ticks owed, 30 run, 2.000 ticks per display period, 0.000 ms clamped, 1750.000 ms idle. `perf world:<surface>` names the systems");
    }

    #[test]
    fn catch_up_overload_threshold_warning_and_reset() {
        for (hz, limit) in [(60., 3), (120., 2)] {
            let mut perf = Perf::default();
            perf.arm();
            let display = frame(0., 1000. / hz);
            assert!(perf
                .record_ticks::<TestGame>(&display, limit, Default::default())
                .is_none());
            assert_eq!(perf.catch_up.overload_frames, 0);
            perf.submitted_live = Some(0);
            let gap = perf.record_gap(advance_gap(50., 36), 0);
            let message = perf
                .record_ticks::<TestGame>(&display, limit + 1, gap)
                .unwrap();
            assert_eq!(message, format!(
                "game tick overload: 36 ticks owed, {} run, {:.3} ticks per display period, 50.000 ms clamped, 0.000 ms idle. `perf world:<surface>` names the systems",
                limit + 1, 120. / hz
            ));
            let gap = perf.record_gap(advance_gap(1750., 240), 1);
            assert!(perf
                .record_ticks::<TestGame>(&display, limit + 1, gap)
                .is_none());
            assert!(perf
                .record_ticks::<TestGame>(&frame(0., 0.), 30, Default::default())
                .is_none());
            assert_eq!(
                state(&perf)["catchUp"],
                json!({"clampFrames":1,"clampMs":50,"idleFrames":1,"idleMs":1750,"overloadFrames":2})
            );
            assert_eq!(perf.ticks_owed.len, 4);
            perf.reset();
            assert_eq!(
                state(&perf)["catchUp"],
                json!({"clampFrames":0,"clampMs":0,"idleFrames":0,"idleMs":0,"overloadFrames":0})
            );
            assert_eq!(perf.ticks_owed.count, 0);
            assert_eq!(perf.ticks_owed.len, 0);
            assert!(perf
                .record_ticks::<TestGame>(&display, limit + 1, Default::default())
                .is_none());
            perf.restart();
            assert!(perf
                .record_ticks::<TestGame>(&display, limit + 1, Default::default())
                .is_some());
        }
    }

    #[test]
    fn cadence_excludes_seeks_and_keeps_dropped_intervals() {
        let mut p = Perf::default();
        assert!(!p.armed());
        p.tick.push(3.0);
        assert_eq!(p.tick.count, 1);
        assert_eq!(p.tick.values.capacity(), 0);
        p.reset();
        assert!(p.armed());
        for (now, seek) in [
            (0., false),
            (8., false),
            (24., false),
            (1000., true),
            (2000., false),
            (2008., false),
        ] {
            p.frame(now, seek);
        }
        assert_eq!(&p.frame.values[..p.frame.len], &[8., 16., 8.]);
        for i in 0..300 {
            p.tick.push(i as f64);
        }
        let mut json = String::new();
        p.tick.json(&mut json);
        assert_eq!(
            json,
            r#"{"p50":149,"p95":284,"p99":296,"max":299,"count":300,"mean":149.5}"#
        );
    }
}

// Loaded modules need separate identities even when their world counters start alike.
fn namespace() -> &'static str {
    static NAMESPACE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAMESPACE.get_or_init(|| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::hash::BuildHasher;
            let random = std::collections::hash_map::RandomState::new();
            format!("{:016x}{:016x}", random.hash_one(0u8), random.hash_one(1u8))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut random = [0; 16];
            web_sys::window()
                .expect("WorldSurface requires window")
                .crypto()
                .expect("WorldSurface requires crypto")
                .get_random_values_with_u8_array(&mut random)
                .expect("world incarnation randomness");
            format!("{:032x}", u128::from_le_bytes(random))
        }
    })
}

pub(crate) fn measure_spans<G: exact_game::Game>(sim: &mut exact_game::Sim<G>, reset: bool) {
    sim.measure_spans(clock_ms, reset, namespace());
}

// One monotonic host domain for engine spans, including wasm (std::Instant is unavailable there).
pub(crate) fn clock_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        performance_now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        thread_local! { static ORIGIN: std::time::Instant = std::time::Instant::now(); }
        ORIGIN.with(|t| t.elapsed().as_secs_f64() * 1000.)
    }
}

#[derive(Default)]
pub(crate) struct PerfRequest {
    pub perf_reset: bool,
    pub perf: bool,
    pub span_since: Option<exact_game::diagnostics::SpanCursor>,
    pub trace_export: bool,
}
impl PerfRequest {
    pub fn parse(text: &str) -> Result<Self, exact_game::DataError> {
        use exact_game::{Data, Reader};
        let mut r = exact_game::json::Decoder::new(text);
        let mut q = Self::default();
        r.begin_struct()?;
        while let Some(field) = r.field()? {
            match field.as_str() {
                "perf_reset" => q.perf_reset.read(&mut r)?,
                "perf" => q.perf.read(&mut r)?,
                "trace_export" => q.trace_export.read(&mut r)?,
                "span_since" => {
                    let mut cursor = exact_game::diagnostics::SpanCursor::default();
                    cursor.read(&mut r)?;
                    q.span_since = Some(cursor);
                }
                _ => r.skip()?,
            }
        }
        r.finish()?;
        Ok(q)
    }
}
