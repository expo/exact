//! The Android reader's frame timing (LLP 1079.000 §6.2): two sources, read
//! by `perf frames` (`canvas/frames.rs` carries them).
//!
//! - **`exact-app-choreographer`** (`app`): the exact thread's Choreographer
//!   turns. A turn is a frame, timestamped at its vsync. Its work runs from
//!   that vsync until the stream it published is ingested on the main thread,
//!   or until the turn returned when it published none, against the frame's
//!   deadline: on Android 13 and later Choreographer's own
//!   (`FrameData.getPreferredFrameTimeline().getDeadlineNanos()`), before it
//!   the next vsync. The period is the vsync spacing Choreographer gives
//!   (13 and later), else the display's refresh. The run ends when no turn
//!   is asked for, so idle time is no frame time.
//! - **`android-frame-metrics`** (`window`): the window's
//!   [FrameMetrics](https://developer.android.com/reference/android/view/FrameMetrics),
//!   each drawn frame of the reader (its ingest, replay and RenderThread)
//!   from its intended vsync for `TOTAL_DURATION`, against `DEADLINE`
//!   (Android 12 and later; before, lateness is unknown). A window draws only
//!   when something changed and FrameMetrics does not say when it idled, so
//!   this source has work and no frame times, as Apple's screen source has.
//!
//! Neither observes presentation, and a GPU canvas's `SurfaceView` is drawn
//! outside the window's frames.

use super::{Health, Kind, METRICS, REPORT};
use serde_json::{json, Value};

static APP: Kind = Kind {
    source: "exact-app-choreographer",
    evidence: "choreographer-and-ingest-proxies",
    report_evidence: "choreographer-and-ingest-timing-proxies",
    cadence_evidence: "callback-and-ingest-proxies",
    unobserved: &[
        "presentation",
        "RenderThread and GPU execution",
        "CPU-versus-descheduled time",
        "other main-thread work",
        "reader-played layer animations",
        "app input latency",
    ],
    coverage: app_coverage,
};

static WINDOW: Kind = Kind {
    source: "android-frame-metrics",
    evidence: "frame-metrics-render-deadlines",
    report_evidence: "frame-metrics-render-deadlines",
    cadence_evidence: "render-deadline",
    unobserved: &[
        "frame times",
        "presentation",
        "GPU canvases' SurfaceViews",
        "CPU-versus-descheduled time",
        "app input latency",
    ],
    coverage: window_coverage,
};

fn app_coverage() -> Value {
    json!({"presentation": "unobserved", "renderThread": "unobserved", "inputLatency": "unobserved",
        "GPU": "unobserved", "window": "android-frame-metrics", "liveAgent": "lent-wall-clock"})
}

fn window_coverage() -> Value {
    json!({"presentation": "unobserved", "frameTimes": "unobserved", "surfaceViews": "unobserved",
        "inputLatency": "unobserved", "liveAgent": "lent-wall-clock"})
}

/// Words per window frame the reader hands over: intended vsync, vsync,
/// total duration, deadline (0 before Android 12), GPU duration (0 likewise).
pub(crate) const WINDOW_WORDS: usize = 5;

/// What a request asked of the clock, besides its reply.
#[derive(Debug, PartialEq)]
pub(crate) enum Lend {
    /// Nothing changes.
    Keep,
    /// A capture lease starts: the display owns time until it ends.
    Wall,
    /// It ended: time is the driver's again.
    Driver,
}

/// The reader's sources and the period they share with GPU canvases.
pub(crate) struct Reader {
    pub app: Health,
    pub window: Health,
    /// The display's refresh period as the reader reads it, nanoseconds.
    display: u64,
    /// The latest turn's period, nanoseconds; 0 where Choreographer gave none.
    turn: u64,
    /// The period last given to GPU canvases.
    given: u64,
    /// No agent report before this, monotonic nanoseconds (one per 15 s).
    next_report: u64,
    /// The device, for a report's identity.
    pub identity: Value,
}

impl Reader {
    pub fn new(collect: bool) -> Reader {
        Reader {
            app: Health::new(&APP, collect),
            window: Health::new(&WINDOW, collect),
            display: 0,
            turn: 0,
            given: 0,
            next_report: 0,
            identity: json!({"host": "android"}),
        }
    }

    /// The clock is the agent's (`true`) or the display's, at `now`.
    pub fn clock(&mut self, virtual_clock: bool, now: u64) {
        self.app.set_virtual(virtual_clock, now);
        self.window.set_virtual(virtual_clock, now);
    }

    /// The display's refresh period changed. The period GPU canvases are
    /// given (ms), when it changed.
    pub fn display_period(&mut self, period: u64) -> Option<f64> {
        self.display = period;
        self.give()
    }

    /// The display's refresh period, told before a host runs to give it:
    /// the first turn gives it.
    pub fn remember_display_period(&mut self, period: u64) {
        self.display = period;
    }

    /// The period a frame is scheduled at: Choreographer's where it gave one.
    fn period(&self) -> u64 {
        if self.turn != 0 {
            self.turn
        } else {
            self.display
        }
    }

    fn give(&mut self) -> Option<f64> {
        let period = self.period();
        (period != self.given).then(|| {
            self.given = period;
            period as f64 / 1e6
        })
    }

    /// One Choreographer turn (see the module's comment): `deadline` and
    /// `period` 0 where Choreographer gave none. The period GPU canvases are
    /// given (ms), when it changed.
    #[allow(clippy::too_many_arguments)]
    pub fn turned(
        &mut self,
        arrival: u64,
        vsync: u64,
        deadline: u64,
        period: u64,
        finished: u64,
        publication: u64,
    ) -> Option<f64> {
        self.turn = period;
        let period = self.period();
        // Before Android 13 a frame's work is due by the next vsync.
        let deadline = if deadline != 0 {
            deadline
        } else {
            vsync + period
        };
        if period != 0 {
            self.app
                .frame(arrival, vsync, deadline, period, finished, publication, 0);
        }
        self.give()
    }

    /// The main thread ingested every publication through `applied`, from
    /// `began` to `ended`.
    pub fn landed(&mut self, applied: u64, began: u64, ended: u64) {
        self.app.landed(applied, began, ended);
    }

    /// No turn is asked for: the run ends.
    pub fn idle(&mut self) {
        self.app.stop();
    }

    /// Window frames, [`WINDOW_WORDS`] each, and the reports FrameMetrics
    /// dropped before the reader took them.
    pub fn window_frames(&mut self, words: &[i64], dropped: u64) {
        self.window.dropped = self.window.dropped.saturating_add(dropped);
        for f in words.chunks_exact(WINDOW_WORDS) {
            let [intended, vsync, total, deadline] =
                [f[0], f[1], f[2], f[3]].map(|w| w.max(0) as u64);
            if intended == 0 {
                continue;
            }
            self.window
                .lateness((deadline == 0).then_some("deadline-unavailable"));
            let due = if deadline == 0 {
                0
            } else {
                intended + deadline
            };
            self.window.work(vsync, intended, due, intended + total);
        }
    }

    /// The source a request names: `app` (the default) or `window`.
    fn source(&mut self, q: &Value) -> Result<&mut Health, Value> {
        match q["source"].as_str() {
            None | Some("app") => Ok(&mut self.app),
            Some("window") => Ok(&mut self.window),
            Some("scroll") => Err(
                json!({"format": METRICS, "available": false, "error": "native-scroll-unobserved",
                "reason": "Android's reader scrolls in its window: read perf frames health window."}),
            ),
            Some(other) => Err(
                json!({"format": METRICS, "available": false, "error": "unknown-source",
                "reason": format!("No frame source {other}: app or window.")}),
            ),
        }
    }

    /// A `perf` request with `frames` (`{"op":"perf","frames":true,…}`), at
    /// `now`: its reply, and what it asks of the clock.
    pub fn answer(&mut self, q: &Value, now: u64) -> (String, Lend) {
        let keep = |v: Value| (v.to_string(), Lend::Keep);
        if q["track"].is_string() {
            return keep(
                json!({"format": "exact.frame-health.track/1", "available": false, "track": false,
                "reason": "Android's reader keeps no scroll track; perf frames health window reads its scrolling frames."}),
            );
        }
        if let Some(action) = q["capture"].as_str() {
            let duration = q["durationMs"].as_f64().unwrap_or(60_000.0);
            let leased = self.app.leased(now);
            let reply = self.app.capture(action, duration, now);
            self.window.capture(action, duration, now);
            let lend = match (leased, self.app.leased(now)) {
                (false, true) => Lend::Wall,
                (true, false) => Lend::Driver,
                _ => Lend::Keep,
            };
            return (reply.to_string(), lend);
        }
        if q["report"] == true {
            // Without a source, the one whose retained hitch is newest (Apple's `reportSource`).
            let window = match q["source"].as_str() {
                None => self.window.newest() > self.app.newest(),
                Some("app") => false,
                Some("window") => true,
                Some(_) => return keep(self.source(q).err().unwrap_or_default()),
            };
            if now < self.next_report {
                let wait = (self.next_report - now).div_ceil(1_000_000);
                return keep(
                    json!({"format": REPORT, "available": false, "error": "report-rate-limit", "retryAfterMs": wait}),
                );
            }
            self.next_report = now + 15_000_000_000;
            let identity = self.identity.clone();
            let (health, other) = if window {
                (&mut self.window, &self.app)
            } else {
                (&mut self.app, &self.window)
            };
            let others =
                vec![json!({"source": other.kind.source, "retainedEvents": other.retained()})];
            return (health.report(now, identity, others), Lend::Keep);
        }
        if q["late"].is_number() || q["live"].is_number() {
            return keep(
                json!({"format": METRICS, "error": "use-bounded-frame-reader",
                "reason": "Use perf frames report for hitches, or the driver's perf frames live <ms> for a live window."}),
            );
        }
        match self.source(q) {
            Ok(h) => {
                let cursor = q.get("cursor").filter(|c| !c.is_null());
                keep(h.metrics(cursor, now))
            }
            Err(e) => keep(e),
        }
    }
}
