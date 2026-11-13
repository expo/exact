//! Bounded, opt-in Contract snapshots. No samples or allocations while disabled.
use crate::{perf::Ring, Stats};
use exact_gpu::{Frame, FrameTiming};
use std::fmt::Write;

#[derive(Default)]
struct Metrics {
    pixels: (u32, u32),
    scale: f32,
    ratio: f32,
    samples: u32,
    stats: Stats,
}

/// What a snapshot reads from its surface when it is taken, not per frame.
#[derive(Default)]
pub(crate) struct Live {
    pub thermal: exact_gpu::ThermalState,
    /// The resolved quality rung's word, once a binding resolved it.
    pub quality: Option<&'static str>,
    /// The surface's frame cap, 0 for the display's rate.
    pub max_fps: u32,
    /// The canvas asks for no frames until something changes.
    pub idle: bool,
    /// The audio output's state, where the surface has one.
    pub audio: Option<&'static str>,
    /// Device bytes of the resident delivered textures and of the shared
    /// vertex and index buffers, once the renderer exists.
    pub memory: Option<(u64, u64)>,
}

/// The output's `state` as a snapshot names it: a suspension the host's
/// interruption caused (an iOS call or alarm, a Windows window losing focus)
/// is `interrupted`.
pub(crate) fn audio(state: Option<&'static str>, interrupted: bool) -> Option<&'static str> {
    state.map(|state| match state {
        "suspended" if interrupted => "interrupted",
        state => state,
    })
}

#[derive(Default)]
pub(crate) struct Report {
    pub phases: [Ring; 3],
    frames: u64,
    last_snapshot_ms: Option<f64>,
    live: Option<bool>,
    late: u64,
    missed: u64,
    gap: f64,
    has_period: bool,
    metrics: Metrics,
    pending: Option<Metrics>,
    pending_gap: Option<(f64, f64)>,
    rendered: bool,
}
impl Report {
    pub fn clock(&mut self, seekable: bool) {
        let live = !seekable;
        if self.live.is_some_and(|previous| previous != live) {
            self.last_snapshot_ms = None;
            self.clear_window();
            self.pending = None;
            self.pending_gap = None;
        }
        self.live = Some(live);
    }
    pub fn encoded(
        &mut self,
        frame: &Frame,
        timing: &FrameTiming,
        scale: f32,
        samples: u32,
        stats: Stats,
        cap: u32,
    ) {
        self.pending = Some(Metrics {
            pixels: frame.pixels(),
            scale,
            ratio: frame.scale,
            samples,
            stats,
        });
        let period = timing.display_period_ms;
        let expected = if cap > 0 {
            // The module honors caps down to 5 Hz (exact_gpu::Module::max_fps).
            period.max(1000. / f64::from(cap.max(5)))
        } else {
            period
        };
        self.pending_gap =
            (!frame.seekable && timing.interval_ms.is_finite() && timing.interval_ms > 0.)
                .then_some((timing.interval_ms, expected));
    }

    pub fn submitted(&mut self) {
        if let Some(metrics) = self.pending.take() {
            self.metrics = metrics;
            self.frames += 1;
            self.rendered = true;
            if let Some((gap, expected)) = self.pending_gap.take() {
                self.gap = self.gap.max(gap);
                if expected.is_finite() && expected > 0. {
                    self.has_period = true;
                    self.late += u64::from(gap > expected * 1.5);
                    self.missed += ((gap / expected).round() as u64).saturating_sub(1);
                }
            }
        }
    }
    pub fn take(&mut self, now_ms: f64, live: &Live) -> Option<String> {
        if !self.rendered {
            return None;
        }
        let window_ms = self
            .last_snapshot_ms
            .filter(|_| self.live == Some(true))
            .map(|last| now_ms - last)
            .filter(|ms| ms.is_finite() && *ms > 0.);
        self.last_snapshot_ms = now_ms.is_finite().then_some(now_ms);
        let m = &self.metrics;
        let mut out = format!("{{\"pixelWidth\":{},\"pixelHeight\":{},\"renderScale\":{},\"effectivePixelRatio\":{},\"devicePixelRatio\":{},\"samples\":{},\"draws\":{},\"triangles\":{},\"instances\":{},\"frames\":{}",
            m.pixels.0, m.pixels.1, m.scale, m.ratio, m.ratio / m.scale,
            m.samples, m.stats.draws, m.stats.triangles, m.stats.instances, self.frames);
        if self.has_period {
            write!(
                out,
                ",\"lateFrames\":{},\"missedFrames\":{}",
                self.late, self.missed
            )
            .unwrap();
        }
        if self.gap > 0. {
            write!(out, ",\"longestGapMs\":{}", self.gap).unwrap();
        }
        if let Some(window_ms) = window_ms {
            write!(
                out,
                ",\"windowMs\":{},\"fps\":{}",
                window_ms,
                self.frames as f64 * 1000. / window_ms
            )
            .unwrap();
        }
        for (name, ring) in ["Tick", "Feed", "Encode"].into_iter().zip(&self.phases) {
            if ring.count > 0 {
                write!(
                    out,
                    ",\"cpu{name}Ms\":{},\"cpu{name}MaxMs\":{}",
                    ring.sum / ring.count as f64,
                    ring.max
                )
                .unwrap();
            }
        }
        // The host's state arrives with each drawn frame; with none in the
        // window it may be stale.
        if let Some(state) = live.thermal.keyword().filter(|_| self.frames > 0) {
            write!(out, ",\"thermalState\":\"{state}\"").unwrap();
        }
        if let Some(rung) = live.quality {
            write!(out, ",\"quality\":\"{rung}\"").unwrap();
        }
        if live.max_fps > 0 {
            // The module honors caps down to 5 Hz (exact_gpu::Module::max_fps).
            write!(out, ",\"maxFps\":{}", live.max_fps.max(5)).unwrap();
        }
        if let Some(state) = live.audio {
            write!(out, ",\"audioState\":\"{state}\"").unwrap();
        }
        if let Some((textures, geometry)) = live.memory {
            write!(
                out,
                ",\"textureMemoryBytes\":{textures},\"geometryMemoryBytes\":{geometry}"
            )
            .unwrap();
        }
        // Only a live clock rests; under a virtual one the fact is unknown.
        if self.live == Some(true) {
            write!(out, ",\"idle\":{}", live.idle).unwrap();
        }
        out.push('}');
        self.clear_window();
        Some(out)
    }

    fn clear_window(&mut self) {
        self.frames = 0;
        self.late = 0;
        self.missed = 0;
        self.gap = 0.;
        self.has_period = false;
        self.phases = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_interrupted_output_says_so_and_no_output_says_nothing() {
        assert_eq!(audio(Some("suspended"), true), Some("interrupted"));
        assert_eq!(audio(Some("running"), false), Some("running"));
        assert_eq!(
            audio(Some("closed"), true),
            Some("closed"),
            "no device is not interrupted"
        );
        assert_eq!(audio(None, true), None);
    }

    #[test]
    fn live_facts_are_read_at_the_snapshot_and_unknown_ones_are_absent() {
        let mut report = Report::default();
        report.clock(true);
        let frame = Frame {
            width: 10.,
            height: 10.,
            scale: 1.,
            now_ms: 0.,
            seekable: true,
            period_ms: 0.,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.,
        };
        report.encoded(&frame, &FrameTiming::default(), 1., 1, Stats::default(), 0);
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(0., &Live::default()).unwrap()).unwrap();
        for absent in [
            "thermalState",
            "quality",
            "maxFps",
            "idle",
            "textureMemoryBytes",
            "audioState",
        ] {
            assert!(value.get(absent).is_none(), "{absent}");
        }
        report.clock(false);
        let live = Live {
            thermal: exact_gpu::ThermalState::Fair,
            quality: Some("mid"),
            max_fps: 2,
            idle: true,
            audio: Some("running"),
            memory: Some((4096, 1024)),
        };
        let value: serde_json::Value =
            serde_json::from_str(&report.take(0., &live).unwrap()).unwrap();
        assert!(
            value.get("thermalState").is_none(),
            "no frame in the window carried the host's state"
        );
        report.encoded(
            &Frame {
                seekable: false,
                ..frame
            },
            &FrameTiming::default(),
            1.,
            1,
            Stats::default(),
            0,
        );
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(1000., &live).unwrap()).unwrap();
        assert_eq!(value["thermalState"], "fair");
        assert_eq!(value["quality"], "mid");
        assert_eq!(value["maxFps"], 5, "caps below 5 Hz draw at 5 Hz");
        assert_eq!(value["idle"], true);
        assert_eq!(value["audioState"], "running");
        assert_eq!(value["textureMemoryBytes"], 4096);
        assert_eq!(value["geometryMemoryBytes"], 1024);
    }

    #[test]
    fn snapshots_count_submitted_frames_honor_caps_and_reset_the_window() {
        let mut report = Report::default();
        report.clock(false);
        let frame = Frame {
            width: 100.,
            height: 50.,
            scale: 1.5,
            now_ms: 0.,
            seekable: false,
            period_ms: 1000. / 120.,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.,
        };
        let mut timing = FrameTiming {
            interval_ms: 0.,
            display_period_ms: 1000. / 120.,
            ..Default::default()
        };
        report.encoded(&frame, &timing, 0.75, 4, Stats::default(), 30);
        assert!(
            report.take(0., &Live::default()).is_none(),
            "encoding is not submission"
        );
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(0., &Live::default()).unwrap()).unwrap();
        assert!(
            value.get("longestGapMs").is_none(),
            "first-after-rest has no interval"
        );
        assert!(value.get("lateFrames").is_none());
        assert!(value.get("fps").is_none());
        assert!(value.get("windowMs").is_none());
        report.encoded(
            &Frame {
                width: 200.,
                ..frame
            },
            &timing,
            0.75,
            4,
            Stats::default(),
            30,
        );
        let value: serde_json::Value =
            serde_json::from_str(&report.take(0., &Live::default()).unwrap()).unwrap();
        assert_eq!(
            value["pixelWidth"], 150,
            "an encoded frame is not submitted yet"
        );
        assert_eq!(value["frames"], 0);
        timing.interval_ms = 1000. / 30.;
        report.encoded(
            &frame,
            &timing,
            0.75,
            4,
            Stats {
                draws: 5,
                ..Default::default()
            },
            30,
        );
        report.phases[0].push(2.);
        report.phases[0].push(4.);
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(1000. / 30., &Live::default()).unwrap()).unwrap();
        assert_eq!(value["pixelWidth"], 150);
        assert_eq!(value["devicePixelRatio"], 2.);
        assert_eq!(value["lateFrames"], 0);
        assert_eq!(value["missedFrames"], 0);
        assert!((value["fps"].as_f64().unwrap() - 30.).abs() < 1e-9);
        assert!((value["windowMs"].as_f64().unwrap() - 1000. / 30.).abs() < 1e-9);
        assert_eq!(value["cpuTickMs"], 3.);
        assert_eq!(value["cpuTickMaxMs"], 4.);
        assert!(value.get("gpuMs").is_none());
        timing.interval_ms *= 2.;
        report.encoded(&frame, &timing, 0.75, 4, Stats::default(), 30);
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(100., &Live::default()).unwrap()).unwrap();
        assert_eq!(value["frames"], 1);
        assert_eq!(value["lateFrames"], 1);
        assert_eq!(value["missedFrames"], 1);
        assert!((value["fps"].as_f64().unwrap() - 15.).abs() < 1e-9);
        assert!(value.get("cpuTickMs").is_none());
        timing.interval_ms = 400.;
        report.encoded(&frame, &timing, 0.75, 4, Stats::default(), 1);
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(500., &Live::default()).unwrap()).unwrap();
        assert_eq!(value["missedFrames"], 1, "caps below 5 Hz draw at 5 Hz");
        timing.display_period_ms = 0.;
        timing.interval_ms = 20.;
        report.encoded(&frame, &timing, 0.75, 4, Stats::default(), 0);
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(520., &Live::default()).unwrap()).unwrap();
        assert_eq!(value["fps"], 50.);
        assert_eq!(value["longestGapMs"], 20.);
        assert!(value.get("lateFrames").is_none(), "unknown target cadence");
        let value: serde_json::Value =
            serde_json::from_str(&report.take(1520., &Live::default()).unwrap()).unwrap();
        assert_eq!(value["frames"], 0);
        assert_eq!(value["fps"], 0.);
        assert_eq!(value["windowMs"], 1000.);
        report.encoded(
            &Frame {
                seekable: true,
                ..frame
            },
            &timing,
            0.75,
            4,
            Stats::default(),
            0,
        );
        let value: serde_json::Value =
            serde_json::from_str(&report.take(2520., &Live::default()).unwrap()).unwrap();
        assert_eq!(
            value["fps"], 0.,
            "unsubmitted encode cannot switch clock mode"
        );
        report.encoded(&frame, &timing, 0.75, 4, Stats::default(), 0);
        report.submitted();
        report.phases[0].push(2.);
        report.clock(true);
        let value: serde_json::Value =
            serde_json::from_str(&report.take(3520., &Live::default()).unwrap()).unwrap();
        assert!(
            value.get("fps").is_none(),
            "virtual time has no measured FPS"
        );
        assert_eq!(
            value["frames"], 0,
            "mode change drops the unfinished window"
        );
        assert!(value.get("longestGapMs").is_none());
        assert!(value.get("cpuTickMs").is_none());
        report.clock(false);
        let value: serde_json::Value =
            serde_json::from_str(&report.take(4520., &Live::default()).unwrap()).unwrap();
        assert!(
            value.get("fps").is_none(),
            "mode change invalidates the window"
        );
        let value: serde_json::Value =
            serde_json::from_str(&report.take(5520., &Live::default()).unwrap()).unwrap();
        assert_eq!(value["fps"], 0.);
        timing.display_period_ms = 20.;
        timing.interval_ms = 3000.;
        report.encoded(&frame, &timing, 0.75, 4, Stats::default(), 0);
        report.submitted();
        let value: serde_json::Value =
            serde_json::from_str(&report.take(5540., &Live::default()).unwrap()).unwrap();
        assert_eq!(value["windowMs"], 20.);
        assert_eq!(
            value["longestGapMs"], 3000.,
            "completed intervals may span snapshots"
        );
        assert_eq!(value["missedFrames"], 149);
    }
}
