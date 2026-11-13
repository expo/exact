//! Frame health on the Linux family's hosts (LLP 1079.000): one source's
//! collector in the native hosts' reducer (`exact_runner::frame_health`), and
//! its readouts in the Apple host's formats, `exact.frame-health.metrics/2`
//! and `exact.frame-health/2` (docs/contract-for-agents.md, "Compact
//! frame-health reports on physical devices"). The Android reader feeds two
//! sources (`canvas/frames.rs`); this file knows nothing of either.
//!
//! @ref LLP 1079.000 §4.4 (metrics), §4.5 (evidence), §5.3 (windows and
//! cursors), §6.2 (Android), §7.1 (the protocol), §7.3 (assessments)

use exact_runner::frame_health as core;
use serde_json::{json, Value};

pub(crate) const METRICS: &str = "exact.frame-health.metrics/2";
pub(crate) const REPORT: &str = "exact.frame-health/2";
const BINS: &str = "frame-health-bins/2";
const POLICY: &str = "exact-frame-health-policy/2";
/// A reply's bytes, the carrier's newline included (LLP 1079.000 §5.2).
pub(crate) const BYTES: usize = 4000;
const MAXIMUM_AGE_NS: u64 = 120_000_000_000;
const SAFE_INTEGER: u64 = 9_007_199_254_740_991;
/// The cursor's cumulative values, laid out as Apple's `FrameHealthMetrics.Sample.values`,
/// then the frames whose work was lost.
const VALUES: usize = 18;
/// Frames whose work waits on a publication's ingest, at most: past it the oldest is lost,
/// and counted, while the main thread stalls behind more streams than this.
const PENDING: usize = 8;
const COLUMNS: [&str; 23] = [
    "sequence",
    "capture",
    "generation",
    "age_us",
    "flags",
    "target_offset_us",
    "period_us",
    "requested_period_us",
    "gap_us",
    "callback_us",
    "before_gap_us",
    "before_callback_us",
    "after_offset_us",
    "after_gap_us",
    "after_callback_us",
    "owner_queued_offset_us",
    "owner_began_offset_us",
    "owner_finished_offset_us",
    "main_landed_offset_us",
    "apply_began_offset_us",
    "apply_finished_offset_us",
    "work_target_offset_us",
    "operations",
];

/// What a source is: its name and evidence, and what it leaves unobserved.
pub(crate) struct Kind {
    pub source: &'static str,
    /// The metrics' evidence (§4.5).
    pub evidence: &'static str,
    /// The report's evidence.
    pub report_evidence: &'static str,
    /// The cadence assessment's evidence; the source's frames have no schedule without one.
    pub cadence_evidence: &'static str,
    /// What the report names as unobserved.
    pub unobserved: &'static [&'static str],
    /// The metrics' `coverage`.
    pub coverage: fn() -> Value,
}

/// A frame whose work finishes when the publication it sent is ingested.
#[derive(Clone, Copy)]
struct Pending {
    start: u64,
    deadline: u64,
    publication: u64,
}

/// One source's collector and what the host keeps beside it.
pub(crate) struct Health {
    pub kind: &'static Kind,
    handle: u32,
    unavailable: Option<&'static str>,
    /// Why lateness is unknown, where the source's deadlines carry no information.
    lateness_unknown: Option<&'static str>,
    pending: [Option<Pending>; PENDING],
    /// Frames whose work was lost because more waited than [`PENDING`].
    lost: u64,
    capture: u64,
    /// When the agent's capture lease ends, monotonic nanoseconds; 0 for none.
    lease_until: u64,
    virtual_clock: bool,
    running: bool,
    /// Reports the source's API dropped before they reached it.
    pub dropped: u64,
}

impl Health {
    /// A collector on this thread, or one that reports why there is none.
    pub fn new(kind: &'static Kind, collect: bool) -> Health {
        let handle = if collect { core::create() } else { 0 };
        let unavailable = if !collect {
            Some("collection-disabled")
        } else if handle == 0 {
            Some("collector-capacity")
        } else {
            None
        };
        Health {
            kind,
            handle,
            unavailable,
            lateness_unknown: None,
            pending: [None; PENDING],
            lost: 0,
            capture: 0,
            lease_until: 0,
            virtual_clock: false,
            running: false,
            dropped: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.unavailable.is_none()
    }

    fn collecting(&self) -> bool {
        self.enabled() && !self.virtual_clock
    }

    /// Where the source's deadlines carry no information, lateness is unknown
    /// (`reason`), not zero; `None` once they do.
    pub fn lateness(&mut self, reason: Option<&'static str>) {
        self.lateness_unknown = reason;
    }

    /// The agent's clock owns time (`true`) or the display's does. Either way
    /// the window starts again: a virtual frame is no measurement.
    pub fn set_virtual(&mut self, virtual_clock: bool, now: u64) {
        if virtual_clock != self.virtual_clock {
            self.virtual_clock = virtual_clock;
            self.reset(now);
        }
    }

    /// One frame: `timestamp` its schedule's start, its work due by
    /// `deadline`, its callback from `arrival` to `finished`. A frame that
    /// sent `publication` (non-zero) finishes when that is ingested
    /// ([`Health::landed`]); one without finished with its callback.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        arrival: u64,
        timestamp: u64,
        deadline: u64,
        period: u64,
        finished: u64,
        publication: u64,
        flags: u64,
    ) {
        if !self.collecting() {
            return;
        }
        self.expire(finished);
        let outcome = if publication != 0 {
            core::PENDING
        } else {
            core::COMPLETE
        };
        let accepted = core::record_at(
            self.handle,
            arrival,
            timestamp,
            deadline,
            period,
            finished,
            outcome,
            flags,
        );
        self.running = true;
        if accepted && publication != 0 {
            let frame = Pending {
                start: timestamp,
                deadline,
                publication,
            };
            // With every slot waiting, the oldest frame gives way: its work is lost, and counted
            let slot = match self.pending.iter().position(Option::is_none) {
                Some(free) => free,
                None => {
                    self.lost = self.lost.saturating_add(1);
                    (0..PENDING)
                        .min_by_key(|i| self.pending[*i].map_or(0, |p| p.publication))
                        .unwrap_or(0)
                }
            };
            self.pending[slot] = Some(frame);
        }
    }

    /// The reader ingested every publication through `applied`, from `began`
    /// to `ended`: the frames waiting on them finish then.
    pub fn landed(&mut self, applied: u64, began: u64, ended: u64) {
        if !self.collecting() || ended == 0 {
            return;
        }
        let mut deadline = 0;
        for slot in &mut self.pending {
            let Some(p) = *slot else { continue };
            if p.publication <= applied {
                core::worked(self.handle, p.start, p.deadline, ended);
                deadline = deadline.max(p.deadline);
                *slot = None;
            }
        }
        if self.capture != 0 && began != 0 && ended >= began {
            let work = core::Work {
                capture: self.capture,
                began_ns: began,
                finished_ns: ended,
                target_ns: deadline,
                landed_ns: began,
                ..core::Work::default()
            };
            core::applied(self.handle, work, 0);
        }
    }

    /// One frame that has work and no schedule: from `start` to `finished`,
    /// due by `deadline` (0 where the source gives none).
    pub fn work(&mut self, arrival: u64, start: u64, deadline: u64, finished: u64) {
        if !self.collecting() || start == 0 || finished < start {
            return;
        }
        self.expire(finished);
        // Where no deadline is given, none is passed: lateness is reported unknown.
        let target = if deadline == 0 { finished } else { deadline };
        let arrival = arrival.clamp(start, finished);
        core::record_at(
            self.handle,
            arrival,
            start,
            target,
            0,
            finished,
            core::COMPLETE,
            0,
        );
        self.running = true;
    }

    /// The source stopped wanting frames: the run ends, so idle time is no frame time.
    pub fn stop(&mut self) {
        if self.enabled() && self.running {
            self.running = false;
            core::stop(self.handle);
        }
    }

    /// A new generation: a reload, or a change of clock.
    pub fn reset(&mut self, now: u64) {
        if !self.enabled() {
            return;
        }
        core::reset(self.handle);
        self.pending = [None; PENDING];
        self.lost = 0;
        self.running = false;
        self.dropped = 0;
        self.capture = 0;
        self.update_capture(now);
    }

    fn snapshot(&self) -> core::Snapshot {
        if self.enabled() {
            core::snapshot(self.handle)
        } else {
            core::Snapshot::default()
        }
    }

    fn bins(&self, kind: u32) -> [u32; 64] {
        if self.enabled() {
            core::distribution(self.handle, kind).bins
        } else {
            [0; 64]
        }
    }

    /// Hitches retained for a report.
    pub fn retained(&self) -> u64 {
        if self.enabled() {
            core::history_status(self.handle).retained
        } else {
            0
        }
    }

    /// When the newest retained hitch happened, 0 for none; the newest need
    /// not be the last recorded.
    pub fn newest(&self) -> u64 {
        (0..self.retained().min(16) as u32)
            .map(|i| core::hitch(self.handle, i).at_ns)
            .max()
            .unwrap_or(0)
    }

    /// Frames whose work is in flight, and those of them past their deadline at `now`.
    fn pending_frames(&self, now: u64) -> (u64, u64) {
        let frames = self.pending.iter().flatten();
        let overdue = frames.clone().filter(|p| now > p.deadline).count();
        (frames.count() as u64, overdue as u64)
    }

    /// The agent's capture lease (`start` for `duration_ms`, `stop`, `clear`).
    pub fn capture(&mut self, action: &str, duration_ms: f64, now: u64) -> Value {
        if !["start", "stop", "clear"].contains(&action)
            || !duration_ms.is_finite()
            || !(1.0..=120_000.0).contains(&duration_ms)
            || duration_ms.fract() != 0.0
        {
            return json!({"error": "Frame capture requires start, stop or clear and a duration of 1–120000 ms."});
        }
        if !self.enabled() {
            return json!({"available": false, "capture": false, "reason": self.unavailable});
        }
        match action {
            "start" => self.lease_until = now + (duration_ms * 1e6) as u64,
            "stop" => self.lease_until = 0,
            _ => {}
        }
        self.update_capture(now);
        if action == "clear" {
            core::capture(self.handle, 2, now);
            if self.capture != 0 {
                self.capture = core::history_status(self.handle).capture;
            }
        }
        json!({"available": true, "capture": self.capture != 0,
            "agentLeaseRemainingMs": self.lease_ms(now), "reason": Value::Null,
            "capacity": core::HITCH_CAPACITY, "maximumAgeMs": 120_000, "maximumReportBytes": BYTES})
    }

    /// Whether detail capture runs.
    pub fn capturing(&self) -> bool {
        self.capture != 0
    }

    /// Whether an agent's lease holds at `now`.
    pub fn leased(&self, now: u64) -> bool {
        self.lease_until > now
    }

    fn lease_ms(&self, now: u64) -> u64 {
        self.lease_until.saturating_sub(now).div_ceil(1_000_000)
    }

    /// An ended lease ends capture; checked by existing work and reads, never a timer.
    pub fn expire(&mut self, now: u64) {
        if self.lease_until != 0 && now >= self.lease_until {
            self.lease_until = 0;
            self.update_capture(now);
        }
    }

    fn update_capture(&mut self, now: u64) {
        let wanted = self.collecting() && self.lease_until > now;
        if !self.enabled() || wanted == (self.capture != 0) {
            return;
        }
        core::capture(self.handle, u32::from(wanted), now);
        self.capture = if wanted {
            core::history_status(self.handle).capture
        } else {
            0
        };
    }

    /// `perf frames health`: since the collector's reset, or since `cursor`.
    pub fn metrics(&mut self, cursor: Option<&Value>, now: u64) -> Value {
        self.expire(now);
        let s = self.snapshot();
        let current = [self.bins(0), self.bins(1)];
        let values = values(&s, self.lost);
        let mut baseline = [0u64; VALUES];
        let mut prior = [[0u32; 64]; 2];
        let mut previous_read = 0;
        if let Some(cursor) = cursor {
            let parsed = parse_cursor(cursor, self.handle, s.generation, now);
            let Some((v, bins, read)) = parsed.filter(|(v, bins, _)| {
                v.iter().zip(&values).all(|(a, b)| a <= b)
                    && bins
                        .iter()
                        .zip(&current)
                        .all(|(a, b)| a.iter().zip(b).all(|(x, y)| x <= y))
            }) else {
                return json!({"format": METRICS, "available": false, "error": "baseline-required",
                    "reason": "The cursor is malformed or belongs to another producer/generation. Read again without a cursor."});
            };
            (baseline, prior, previous_read) = (v, bins, read);
        }
        let delta: Vec<u64> = values.iter().zip(baseline).map(|(a, b)| a - b).collect();
        let d: Vec<Vec<u32>> = current
            .iter()
            .zip(&prior)
            .map(|(c, p)| c.iter().zip(p).map(|(a, b)| a - b).collect())
            .collect();
        let (intervals, work) = (&d[0], &d[1]);
        let sum = |b: &[u32]| b.iter().map(|x| u64::from(*x)).sum::<u64>();
        let valid =
            s.histogram_overflows == 0 && sum(intervals) == delta[2] && sum(work) == delta[6];
        if !valid && cursor.is_some() && s.histogram_overflows == 0 {
            return json!({"format": METRICS, "available": false, "error": "baseline-required",
                "reason": "Cursor bins do not match their populations. Read again without a cursor."});
        }
        let q = |b: &[u32], f: f64| if valid { quantile(b, f) } else { Value::Null };
        let frames = json!({"callbacks": delta[0].to_string(), "held": delta[1].to_string(),
            "intervals": delta[2].to_string(), "observedNs": delta[3].to_string(),
            "perSecond": if delta[3] == 0 { Value::Null } else { json!(delta[2] as f64 * 1e9 / delta[3] as f64) },
            "lifetimeMaximumIntervalNs": s.max_interval_ns.to_string(),
            "medianBoundsNs": q(intervals, 0.5), "p99BoundsNs": q(intervals, 0.99), "maximumBoundsNs": q(intervals, 1.0),
            "cadenceChanges": delta[4].to_string(), "pacingChanges": delta[5].to_string(),
            "unclassifiedIntervals": delta[15].to_string(), "outOfOrder": delta[16].to_string(),
            "requestedSlots": delta[11].to_string(), "missedFrames": delta[12].to_string(),
            "restrictedIntervals": delta[13].to_string()});
        let known = self.lateness_unknown.is_none();
        let (pending, overdue) = self.pending_frames(now);
        let work_reply = json!({"records": delta[6].to_string(),
            "late": if known { json!(delta[7].to_string()) } else { Value::Null },
            "latenessUnknown": self.lateness_unknown, "pending": pending, "lost": delta[17].to_string(),
            "overdue": if known { json!(overdue) } else { Value::Null },
            "medianBoundsNs": q(work, 0.5), "p99BoundsNs": q(work, 0.99), "maximumBoundsNs": q(work, 1.0)});
        let stale =
            s.running != 0 && s.last_arrival_ns != 0 && now > s.last_arrival_ns + 1_000_000_000;
        let window = Window {
            delta: &delta,
            scheduled: s.period_ns,
            running: s.running != 0,
            overdue,
            stale,
        };
        let status = if self.virtual_clock {
            "virtual"
        } else if !self.enabled() {
            "unavailable"
        } else if s.running == 0 {
            "inactive"
        } else if stale {
            "stale"
        } else {
            "observing"
        };
        let reply = json!({"format": METRICS, "available": self.collecting(),
            "source": self.kind.source, "evidence": self.kind.evidence, "qualification": "unqualified",
            "status": status, "unavailableReason": self.blocked(),
            "window": {"generation": s.generation.to_string(), "kind": if cursor.is_none() { "since-reset" } else { "delta" },
                "readFromNs": previous_read.to_string(), "readThroughNs": now.to_string(),
                "intervalFromNs": baseline[8].to_string(), "intervalThroughNs": s.interval_through_ns.to_string(),
                "workFromNs": baseline[9].to_string(), "workThroughNs": s.work_through_ns.to_string()},
            "frames": frames, "work": work_reply, "assessments": self.assess(&window),
            "histogram": {"version": BINS, "valid": valid, "overflows": s.histogram_overflows.to_string(),
                "quantiles": "half-open bounds; null upper is overflow"},
            "scheduledPeriodNs": s.period_ns.to_string(), "requestedPeriodNs": "0",
            "observationAgeNs": if s.last_arrival_ns == 0 { Value::Null } else { json!(now.saturating_sub(s.last_arrival_ns).to_string()) },
            "coverage": (self.kind.coverage)(), "droppedReports": self.dropped.to_string(),
            "capture": {"agentLeaseRemainingMs": self.lease_ms(now)},
            "cursor": {"version": 3, "producer": self.handle.to_string(), "generation": s.generation.to_string(),
                "values": values.iter().map(u64::to_string).collect::<Vec<_>>(),
                "intervals": current[0].to_vec(), "work": current[1].to_vec(), "readNs": now.to_string()}});
        if reply.to_string().len() >= BYTES {
            return json!({"format": METRICS, "available": false, "error": "metrics-budget",
                "reason": "The metrics exceeded the 4000-byte carrier budget. Use a matching Exact reader."});
        }
        reply
    }

    /// Why a reader gets no assessment: the clock is the agent's, or nothing collects.
    fn blocked(&self) -> Option<&'static str> {
        self.unavailable
            .or(self.virtual_clock.then_some("virtual-clock"))
    }

    /// LLP 1079.000 §7.3's predicates over one window, as Apple's
    /// `FrameHealthAssessment` states them: missing evidence never passes,
    /// and callback evidence passes at most `proxy-pass`.
    fn assess(&self, w: &Window) -> Value {
        let d = w.delta;
        let (callbacks, intervals, worked, late) = (d[0], d[2], d[6], d[7]);
        let (slots, missed, restricted, skipped, unclassified, out_of_order) =
            (d[11], d[12], d[13], d[14], d[15], d[16]);
        let known = self.lateness_unknown.is_none();
        let blocked = self.blocked();
        let no_schedule = if w.scheduled == 0 {
            "schedule-unknown"
        } else {
            "no-frame-intervals"
        };
        let idle = |running: bool| {
            if running {
                ("insufficient-evidence", "no-callbacks")
            } else {
                ("not-applicable", "idle")
            }
        };
        let cadence = if let Some(b) = blocked {
            ("insufficient-evidence", b)
        } else if skipped > 0 {
            ("fail", "skipped-refreshes")
        } else if known && (late > 0 || w.overdue > 0) {
            ("fail", "late-work")
        } else if callbacks == 0 {
            idle(w.running)
        } else if w.stale {
            ("insufficient-evidence", "stale")
        } else if d[17] > 0 {
            // Frames whose work was lost could have been late: missing data cannot pass
            ("insufficient-evidence", "work-lost")
        } else if out_of_order > 0 {
            ("insufficient-evidence", "out-of-order")
        } else if intervals == 0 {
            ("insufficient-evidence", no_schedule)
        } else if unclassified > 0 {
            ("insufficient-evidence", "schedule-changed")
        } else if let Some(reason) = self.lateness_unknown {
            ("insufficient-evidence", reason)
        } else {
            ("proxy-pass", "presentation-unobserved")
        };
        let target = if let Some(b) = blocked {
            ("insufficient-evidence", b)
        } else if restricted > 0 {
            ("restricted", "unknown")
        } else if missed > 0 {
            ("fail", "missed-frames")
        } else if callbacks == 0 {
            idle(w.running)
        } else if intervals == 0 {
            ("insufficient-evidence", no_schedule)
        } else if slots == 0 {
            ("not-applicable", "no-rate-request")
        } else {
            ("insufficient-evidence", "presentation-unobserved")
        };
        let latency = if let Some(b) = blocked {
            ("insufficient-evidence", b)
        } else if callbacks == 0 && worked == 0 {
            ("not-applicable", "idle")
        } else {
            ("insufficient-evidence", "content-lateness-unobserved")
        };
        let counted = |n: u64| {
            if known {
                json!(n.to_string())
            } else {
                Value::Null
            }
        };
        json!({"policy": POLICY,
            "cadence": {"result": cadence.0, "reason": cadence.1, "evidence": self.kind.cadence_evidence, "scope": "frames",
                "predicates": {"frameIntervals": intervals.to_string(), "skippedRefreshes": skipped.to_string(),
                    "lateFrames": counted(late), "overdueFrames": counted(w.overdue), "unclassifiedIntervals": unclassified.to_string(), "lostWork": d[17].to_string()}},
            "target": {"result": target.0, "reason": target.1, "evidence": "callback-proxy", "scope": "requested-rate",
                "predicates": {"requestedSlots": slots.to_string(), "missedFrames": missed.to_string(),
                    "restrictedIntervals": restricted.to_string()}, "context": Vec::<String>::new()},
            "contentLatency": {"result": latency.0, "reason": latency.1, "evidence": "unavailable", "scope": "content"}})
    }

    /// `perf frames report`'s packet for this source, encoded within
    /// [`BYTES`]: its retained hitches, newest kept when they do not all fit.
    pub fn report(&mut self, now: u64, identity: Value, others: Vec<Value>) -> String {
        self.expire(now);
        let s = self.snapshot();
        let history = if self.enabled() {
            core::history_status(self.handle)
        } else {
            core::HistoryStatus::default()
        };
        let mut clamped = false;
        let mut expired = 0;
        let mut events: Vec<core::Hitch> = (0..history.retained.min(16) as u32)
            .map(|i| core::hitch(self.handle, i))
            .collect();
        events.sort_by_key(|e| e.at_ns);
        let stored = |v: i32| if v == i32::MIN { Value::Null } else { json!(v) };
        let mut rows = Vec::new();
        for e in events {
            if e.sequence == 0 {
                continue;
            }
            if e.at_ns > now || now - e.at_ns > MAXIMUM_AGE_NS {
                expired += 1;
                continue;
            }
            clamped |= e.flags & 64 != 0;
            let (w, work, after) = (e.work, e.flags & 16 != 0, e.flags & 8 != 0);
            let when = |known: bool, v: Value| if known { v } else { Value::Null };
            rows.push(json!([
                e.sequence.to_string(),
                e.capture.to_string(),
                e.generation.to_string(),
                ((now - e.at_ns) / 1000).min(SAFE_INTEGER),
                e.flags.min(SAFE_INTEGER),
                stored(e.target_us),
                e.period_us,
                e.requested_period_us,
                e.gap_us,
                e.callback_us,
                e.before_gap_us,
                e.before_callback_us,
                when(after, stored(e.after_at_us)),
                when(after && e.after_gap_us != 0, json!(e.after_gap_us)),
                when(after, json!(e.after_callback_us)),
                when(work, stored(w.queued_us)),
                when(work, stored(w.owner_began_us)),
                when(work, stored(w.owner_finished_us)),
                when(work, stored(w.landed_us)),
                when(work, stored(w.began_us)),
                when(work, stored(w.finished_us)),
                when(work, stored(w.target_us)),
                when(work, json!(w.operations))
            ]));
        }
        let offset = |v: u64| {
            if v == 0 {
                Value::Null
            } else if v >= now {
                json!((v - now).min(SAFE_INTEGER))
            } else {
                json!(-((now - v).min(SAFE_INTEGER) as i64))
            }
        };
        let bounds = |bins: &[u32], count: u64| {
            if count == 0
                || s.histogram_overflows != 0
                || bins.iter().map(|b| u64::from(*b)).sum::<u64>() != count
            {
                Value::Null
            } else {
                json!({"median": quantile(bins, 0.5), "p99": quantile(bins, 0.99), "maximum": quantile(bins, 1.0)})
            }
        };
        let known = self.lateness_unknown.is_none();
        let packet = json!({"format": REPORT, "source": self.kind.source, "identity": identity,
            "capturedAt": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()),
            "qualification": "unqualified", "evidence": self.kind.report_evidence,
            "available": self.collecting(), "unavailableReason": self.blocked(),
            "time": {"clock": "CLOCK_MONOTONIC", "unit": "us", "clamped": clamped},
            "scope": {"generation": s.generation.to_string(), "capture": history.capture.to_string(),
                "armed": history.active != 0, "callbackSourceRunning": s.running != 0,
                "segments": history.segments.to_string(), "observedCallbacks": history.callbacks.to_string(),
                "captureStartedOffsetNs": offset(history.started_ns), "captureChangedOffsetNs": offset(history.changed_ns),
                "droppedReports": self.dropped.to_string(), "liveAgentParity": "unqualified"},
            "retention": {"capacity": core::HITCH_CAPACITY, "maximumAgeMs": 120_000,
                "retainedBeforeAgeFilter": history.retained, "expiredAtRead": expired,
                "overwrittenSinceClear": history.overwritten.to_string(), "eventsSinceClear": history.total.to_string(),
                "automaticDiskBytes": 0, "maximumReportBytes": BYTES, "omittedForBudget": 0, "selection": "newest"},
            "coreSinceReload": {"callbacks": s.callbacks.to_string(), "heldCallbacks": s.held.to_string(),
                "frameIntervals": s.intervals.to_string(), "activeNs": s.active_ns.to_string(),
                "maximumFrameIntervalNs": s.max_interval_ns.to_string(), "scheduledPeriodNs": s.period_ns,
                "missedFrames": s.missed_frames.to_string(),
                "lateFrames": if known { json!(s.late.to_string()) } else { Value::Null },
                "lostWorkRecords": self.lost.to_string(),
                "frameTimeBoundsNs": bounds(&self.bins(0), s.intervals), "workBoundsNs": bounds(&self.bins(1), s.worked),
                "detailMayHaveBeenOff": true},
            "unobserved": self.kind.unobserved, "columns": COLUMNS,
            "otherSources": others, "selection": "one-source; request app or window explicitly for another source",
            "hitches": rows});
        encode(packet)
    }
}

/// The deltas a window's assessments read, laid out as [`values`].
struct Window<'a> {
    delta: &'a [u64],
    scheduled: u64,
    running: bool,
    overdue: u64,
    stale: bool,
}

/// The cumulative values a cursor carries, in Apple's order; 8 and 9 are watermarks.
fn values(s: &core::Snapshot, lost: u64) -> [u64; VALUES] {
    [
        s.callbacks,
        s.held,
        s.intervals,
        s.active_ns,
        s.cadence_changes,
        s.pacing_changes,
        s.worked,
        s.late,
        s.interval_through_ns,
        s.work_through_ns,
        s.histogram_overflows,
        s.requested_slots,
        s.missed_frames,
        s.restricted_intervals,
        s.skipped_refreshes,
        s.unclassified_intervals,
        s.out_of_order,
        lost,
    ]
}

type Cursor = ([u64; VALUES], [[u32; 64]; 2], u64);

/// A cursor this producer and generation gave, read no later than `now`.
fn parse_cursor(cursor: &Value, producer: u32, generation: u64, now: u64) -> Option<Cursor> {
    if cursor["version"] != 3
        || cursor["producer"].as_str() != Some(producer.to_string().as_str())
        || cursor["generation"].as_str() != Some(generation.to_string().as_str())
    {
        return None;
    }
    let number = |v: &Value| {
        let t = v.as_str()?;
        (!t.is_empty() && t.len() <= 20 && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse::<u64>().ok())
            .flatten()
    };
    let list = cursor["values"].as_array()?;
    if list.len() != VALUES {
        return None;
    }
    let mut values = [0u64; VALUES];
    for (slot, v) in values.iter_mut().zip(list) {
        *slot = number(v)?;
    }
    let mut bins = [[0u32; 64]; 2];
    for (slot, key) in bins.iter_mut().zip(["intervals", "work"]) {
        let list = cursor[key].as_array()?;
        if list.len() != 64 {
            return None;
        }
        for (bin, v) in slot.iter_mut().zip(list) {
            *bin = u32::try_from(v.as_u64()?).ok()?;
        }
    }
    let read = number(&cursor["readNs"]).filter(|r| *r <= now)?;
    Some((values, bins, read))
}

/// The bounds of the frame-time bin holding `fraction` of the values, by
/// nearest rank: 0.5 ms bins below 24 ms, doubling tails, an open last bin.
fn quantile(bins: &[u32], fraction: f64) -> Value {
    let total: u64 = bins.iter().map(|b| u64::from(*b)).sum();
    if total == 0 {
        return Value::Null;
    }
    let rank = ((total as f64 * fraction).ceil() as u64).max(1);
    let mut count = 0;
    for (index, value) in bins.iter().enumerate() {
        count += u64::from(*value);
        if count >= rank {
            let (lower, upper) = if index < 48 {
                (index as u64 * 500_000, Some((index as u64 + 1) * 500_000))
            } else {
                let lower = 24_000_000u64 << (index - 48);
                (lower, (index != 63).then_some(lower * 2))
            };
            return json!([lower, upper]);
        }
    }
    Value::Null
}

/// The packet within [`BYTES`] with the carrier's newline: the newest rows
/// that fit, in order, and how many were left out.
fn encode(mut packet: Value) -> String {
    let failure = format!(
        r#"{{"format":"{REPORT}","available":false,"error":"report-size-or-encoding-budget"}}"#
    );
    let budget = BYTES - 1;
    let rows = packet["hitches"].as_array().cloned().unwrap_or_default();
    packet["hitches"] = json!([]);
    packet["retention"]["omittedForBudget"] = json!(rows.len());
    let mut size = packet.to_string().len() + 1;
    if size > budget {
        return failure;
    }
    let mut count = 0;
    for row in rows.iter().rev() {
        let added = row.to_string().len() + usize::from(count != 0);
        if size + added > budget {
            break;
        }
        size += added;
        count += 1;
    }
    packet["hitches"] = json!(rows[rows.len() - count..]);
    packet["retention"]["omittedForBudget"] = json!(rows.len() - count);
    let encoded = packet.to_string();
    if encoded.len() > budget {
        failure
    } else {
        encoded
    }
}

#[path = "frame_health/android.rs"]
pub(crate) mod android;
#[cfg(test)]
#[path = "frame_health/tests.rs"]
mod tests;
