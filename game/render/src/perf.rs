use crate::Stats;
use std::fmt::Write;
#[cfg(test)]
thread_local! { pub(crate) static CLOCK_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

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
    thread_local! {
        static PERFORMANCE: web_sys::Performance = web_sys::window()
            .and_then(|w| w.performance()).expect("WorldSurface requires window.performance");
    }
    PERFORMANCE.with(web_sys::Performance::now)
}
thread_local! {
    // Whether this thread times pipeline creation, and the milliseconds it
    // has spent creating pipelines since the last `pipeline_ms`.
    static PIPELINES: std::cell::Cell<(bool, f64)> = const { std::cell::Cell::new((false, 0.)) };
}
/// Times this thread's pipeline creation from zero while it lives, when `on`:
/// a seekable frame reads no clock, so it times nothing. Every return of the
/// timed work stops it.
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
#[derive(Default)]
pub(crate) struct Perf {
    pub reporting: Option<crate::surface::performance::Report>,
    frame: Ring,
    pub tick: Ring,
    pub feed: Ring,
    pub encode: Ring,
    /// Of the frame's feed and encode, the time creating pipelines on its thread.
    pub pipeline: Ring,
    /// Pipeline time from the renderer's making to the first frame log; None
    /// once a seekable frame, which times nothing, came between.
    pub pipeline_ready: Option<f64>,
    /// Pipeline time not yet in a frame's: a frame that stopped before
    /// encoding, the `Game::PREPARE` warm-up after the last submit. The next
    /// drawn frame counts it.
    pub pipeline_pending: f64,
    /// Whether the last rendered frame was live, so its warm-up may be timed,
    /// and whether one is rendering now (its own timing covers what it prepares).
    pub live: bool,
    pub rendering: bool,
    pub ticks: Ring,
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
    pub fn arm(&mut self) {
        for r in [
            &mut self.frame,
            &mut self.tick,
            &mut self.feed,
            &mut self.encode,
            &mut self.pipeline,
            &mut self.ticks,
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
        ] {
            r.values.resize(16384, 0.0);
            r.count = 0;
            r.sum = 0.0;
            r.max = 0.0;
            r.len = 0;
            r.next = 0;
        }
        self.last_live = None;
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
        write!(out, ",\"armed\":{}", self.armed()).unwrap();
        write!(out, ",\"pixels\":[{},{}]", self.pixels.0, self.pixels.1).unwrap();
        for (name, ring) in [
            ("frameMs", &self.frame),
            ("tickMs", &self.tick),
            ("feedMs", &self.feed),
            ("encodeMs", &self.encode),
            ("pipelineMs", &self.pipeline),
            ("ticksPerFrame", &self.ticks),
        ] {
            write!(out, ",\"{name}\":").unwrap();
            ring.json(out);
        }
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
