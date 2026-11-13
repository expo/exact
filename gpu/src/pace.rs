//! A canvas's draw cadence and the timing facts its surface reads: a
//! frame-rate cap ([`crate::Surface::max_fps`]) held by a draw deadline on the
//! lattice of the cap's interval, so a cap below the display's rate draws on
//! a steady share of its callbacks; and the last frame's interval, the
//! display's period, the host's callback timing and the device's thermal
//! state ([`FrameTiming`]). Nothing here reaches saved state.

use crate::Module;

/// The device's thermal state as the platform reports it: Apple's
/// `ProcessInfo.ThermalState` with an unknown below it. A host without a
/// cheap source reports `Unknown`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ThermalState {
    /// The host does not say.
    #[default]
    Unknown = 0,
    /// Within normal limits.
    Nominal = 1,
    /// Slightly elevated.
    Fair = 2,
    /// High: the system is reducing performance.
    Serious = 3,
    /// Critical: the app should reduce its work at once.
    Critical = 4,
}

impl ThermalState {
    /// The host ABI's code, 0 unknown to 4 critical; any other is unknown.
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Nominal,
            2 => Self::Fair,
            3 => Self::Serious,
            4 => Self::Critical,
            _ => Self::Unknown,
        }
    }

    /// The state's word, as `ProcessInfo.ThermalState` names its cases;
    /// `None` while unknown.
    pub fn keyword(self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::Nominal => Some("nominal"),
            Self::Fair => Some("fair"),
            Self::Serious => Some("serious"),
            Self::Critical => Some("critical"),
        }
    }
}

/// The host's display callback that drew a frame, on the host's clock in
/// milliseconds (the clock of [`crate::Frame::now_ms`]); zero while unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HostTiming {
    /// The callback's own timestamp before any pacing: a web frame's
    /// `requestAnimationFrame` time, a `CADisplayLink`'s `timestamp`.
    pub raw_ms: f64,
    /// When the host's callback began to run; less `raw_ms`, how late it ran.
    pub callback_ms: f64,
    /// The display's fastest period (Apple's `maximumFramesPerSecond`).
    pub fastest_period_ms: f64,
}

/// What a surface may adapt to (a render-scale controller, a thermal cap),
/// delivered before each frame it draws ([`crate::Surface::timing`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameTiming {
    /// Milliseconds since this canvas's previous drawn frame; 0 for its first
    /// and for the first after it rested.
    pub interval_ms: f64,
    /// The display's period as the host knows it ([`crate::Frame::period_ms`]).
    pub display_period_ms: f64,
    /// The host's callback.
    pub host: HostTiming,
    /// The device's thermal state.
    pub thermal_state: ThermalState,
}

/// One canvas's draws: when its next is due under the cap, and when its last was.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Cadence {
    /// The deadline the cap set, on its lattice; `None` draws at once.
    next_ms: Option<f64>,
    /// The cap's interval `next_ms` is on.
    interval_ms: f64,
    /// The last drawn frame's clock; `None` before the first and after a rest.
    last_ms: Option<f64>,
}

/// How early a callback may draw: half a display period, so a callback that
/// jitters past the deadline's display frame is not pushed to the next one.
fn slack(interval: f64, period: f64) -> f64 {
    if period > 0.0 {
        period.min(interval) / 2.0
    } else {
        interval / 4.0
    }
}

/// The lowest cap honoured: a longer gap than the simulation's live clock
/// carries in one frame (250 ms) would slow its ticks, not just its drawing.
const MIN_FPS: u32 = 5;

fn interval(cap: u32) -> Option<f64> {
    (cap > 0).then(|| 1000.0 / f64::from(cap.max(MIN_FPS)))
}

impl Cadence {
    /// The deadline for a cap's interval: a changed cap counts from the last draw.
    fn deadline(&self, interval: f64) -> Option<f64> {
        let next = self.next_ms?;
        Some(if interval == self.interval_ms {
            next
        } else {
            self.last_ms.map_or(next, |last| last + interval)
        })
    }

    /// Whether a frame at `now` may draw under `cap` (0: no cap).
    pub(crate) fn due(&self, cap: u32, now: f64, period: f64) -> bool {
        let Some(interval) = interval(cap) else {
            return true;
        };
        self.deadline(interval)
            .is_none_or(|next| now >= next - slack(interval, period))
    }

    /// A frame drew at `now`: the next deadline is the lattice's next point
    /// at least an interval less a display period away (half an interval while
    /// the period is unknown), so a late frame skips the slots it missed
    /// instead of drawing them in a burst. A frame drawn before its
    /// deadline (a resize, which renders outside the frame loop) takes no slot.
    pub(crate) fn drew(&mut self, cap: u32, now: f64, period: f64) {
        let deadline = interval(cap).and_then(|i| self.deadline(i));
        self.last_ms = Some(now);
        let Some(interval) = interval(cap) else {
            self.next_ms = None;
            return;
        };
        if deadline.is_some_and(|d| now < d - slack(interval, period)) {
            return;
        }
        let gap = if period > 0.0 {
            (interval - period).max(0.0)
        } else {
            interval / 2.0
        };
        let mut next = deadline.unwrap_or(now) + interval;
        if next - now < gap {
            next += ((now + gap - next) / interval).ceil() * interval;
        }
        self.next_ms = Some(next);
        self.interval_ms = interval;
    }

    /// A frame drew at `now`, and the surface wants another (`drew`) or not
    /// (`rest`).
    pub(crate) fn after(&mut self, cap: u32, now: f64, period: f64, wants: bool) {
        if wants {
            self.drew(cap, now, period);
        } else {
            self.rest();
        }
    }

    /// The surface asked for no other frame: the next, a wake, draws at once.
    pub(crate) fn rest(&mut self) {
        *self = Self::default();
    }

    /// The interval a frame at `now` would report.
    pub(crate) fn interval(&self, now: f64) -> f64 {
        self.last_ms.map_or(0.0, |last| (now - last).max(0.0))
    }
}

/// The lower of two caps, either 0 for none.
pub fn lower_cap(a: u32, b: u32) -> u32 {
    match (a, b) {
        (0, b) => b,
        (a, 0) => a,
        (a, b) => a.min(b),
    }
}

impl Module {
    /// The canvas's frame-rate cap, 0 for none ([`crate::Surface::max_fps`]).
    /// A cap below the lowest honoured is reported as that.
    pub fn max_fps(&self, id: u32) -> u32 {
        match self.instances.get(&id).map_or(0, |i| i.surface.max_fps()) {
            0 => 0,
            cap => cap.max(MIN_FPS),
        }
    }

    /// Whether the canvas's next frame is due at `now_ms` under its cap: a
    /// host asks before each render it would make for a frame of the display,
    /// and renders a canvas that is not due at a later one. The agent's clock
    /// is never held. A host that does not ask draws at the display's rate.
    pub fn due(&self, id: u32, now_ms: f64) -> bool {
        self.seekable
            || self
                .instances
                .get(&id)
                .is_none_or(|i| i.cadence.due(i.surface.max_fps(), now_ms, self.period_ms))
    }

    /// The host's display callback for the frames that follow; codes as [`ThermalState::from_code`].
    pub fn set_host_timing(&mut self, host: HostTiming, thermal: ThermalState) {
        let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
        self.host = (
            HostTiming {
                raw_ms: finite(host.raw_ms),
                callback_ms: finite(host.callback_ms),
                fastest_period_ms: finite(host.fastest_period_ms).max(0.0),
            },
            thermal,
        );
    }

    /// The facts canvas `id`'s frame at `now_ms` reads.
    pub(crate) fn timing(&self, id: u32, now_ms: f64) -> FrameTiming {
        FrameTiming {
            interval_ms: self
                .instances
                .get(&id)
                .map_or(0.0, |i| i.cadence.interval(now_ms)),
            display_period_ms: self.period_ms,
            host: self.host.0,
            thermal_state: self.host.1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws over ten seconds of callbacks at `hz`, each `jitter(i)` ms off
    /// its vsync, under `cap`.
    fn draws(hz: f64, cap: u32, jitter: impl Fn(usize) -> f64) -> Vec<f64> {
        let period = 1000.0 / hz;
        let mut c = Cadence::default();
        let mut drawn = Vec::new();
        for i in 0..(10.0 * hz) as usize {
            let now = 5000.0 + i as f64 * period + jitter(i);
            if c.due(cap, now, period) {
                c.drew(cap, now, period);
                drawn.push(now);
            }
        }
        drawn
    }

    #[test]
    fn a_cap_draws_its_rate_or_the_displays_whichever_is_lower() {
        for hz in [30.0, 60.0, 90.0, 120.0, 144.0] {
            for cap in [0, 24, 30, 45, 60, 90, 120, 144, 240] {
                for wobble in [0.0, 0.9] {
                    // Callbacks wobble around their vsync, never across it.
                    let jitter = |i: usize| wobble * if i.is_multiple_of(3) { 1.0 } else { -0.7 };
                    let n = draws(hz, cap, jitter).len() as f64 / 10.0;
                    let want = if cap == 0 { hz } else { hz.min(f64::from(cap)) };
                    assert!(
                        (n - want).abs() <= want * 0.02 + 0.2,
                        "{cap} on {hz} Hz, jitter {wobble}: {n} fps"
                    );
                }
            }
        }
    }

    #[test]
    fn a_cap_that_divides_the_display_draws_on_a_steady_share_of_its_frames() {
        for (hz, cap, every) in [
            (60.0, 30, 2),
            (120.0, 60, 2),
            (120.0, 30, 4),
            (144.0, 72, 2),
            (90.0, 30, 3),
        ] {
            let period = 1000.0 / hz;
            let drawn = draws(hz, cap, |i| if i % 2 == 0 { 0.8 } else { -0.8 });
            for pair in drawn.windows(2) {
                let frames = ((pair[1] - pair[0]) / period).round() as u32;
                assert_eq!(frames, every, "{cap} on {hz} Hz");
            }
        }
    }

    #[test]
    fn a_missed_slot_is_skipped_not_caught_up() {
        let period = 1000.0 / 120.0;
        let mut c = Cadence::default();
        c.drew(30, 0.0, period);
        // A 100 ms hitch: one frame draws, then the cap's steady rate again.
        assert!(c.due(30, 100.0, period));
        c.drew(30, 100.0, period);
        assert!(!c.due(30, 100.0 + period, period));
        assert!(!c.due(30, 100.0 + 2.0 * period, period));
        let next = (1..10)
            .map(|k| 100.0 + k as f64 * period)
            .find(|&t| c.due(30, t, period))
            .unwrap();
        assert!((next - 100.0 - 1000.0 / 30.0).abs() < period, "{next}");
        // On the lattice: the deadline is a whole number of intervals from the first draw.
        let lattice = c.next_ms.unwrap() / (1000.0 / 30.0);
        assert!((lattice - lattice.round()).abs() < 1e-9, "{lattice}");
    }

    #[test]
    fn a_late_frame_takes_the_first_slot_far_enough_away() {
        // Cap 10 on a 100 Hz display: late at 210 ms, the next slot is 300 ms.
        let mut c = Cadence::default();
        c.drew(10, 0.0, 10.0);
        c.drew(10, 210.0, 10.0);
        assert!(!c.due(10, 290.0, 10.0));
        assert!(c.due(10, 300.0, 10.0));
    }

    #[test]
    fn a_late_frame_is_not_followed_by_a_burst() {
        let period = 1000.0 / 120.0;
        let mut c = Cadence::default();
        c.drew(30, 0.0, period);
        // Callbacks missed until 58.3 ms: the slot at 66.7 ms is too close.
        assert!(c.due(30, 7.0 * period, period));
        c.drew(30, 7.0 * period, period);
        assert!(!c.due(30, 8.0 * period, period));
        let next = (8..20)
            .map(|k| k as f64 * period)
            .find(|&t| c.due(30, t, period))
            .unwrap();
        assert!(next - 7.0 * period >= 1000.0 / 60.0, "{next}");
    }

    #[test]
    fn a_draw_before_its_deadline_takes_no_slot() {
        let period = 1000.0 / 60.0;
        let mut c = Cadence::default();
        c.drew(30, 0.0, period);
        // Resizes render between the cap's frames, outside the frame loop.
        for t in [3.0, 9.0, 15.0, 21.0] {
            c.drew(30, t, period);
        }
        assert!(c.due(30, 2.0 * period, period), "the next slot stands");
    }

    #[test]
    fn a_cap_below_five_draws_five_frames_a_second() {
        let drawn = draws(60.0, 1, |_| 0.0);
        assert_eq!(drawn.len(), 50);
    }

    #[test]
    fn a_wake_draws_at_once_and_a_changed_cap_counts_from_the_last_draw() {
        let period = 1000.0 / 120.0;
        let mut c = Cadence::default();
        c.drew(30, 0.0, period);
        assert!(!c.due(30, period, period));
        assert_eq!(c.interval(period), period);
        // Resting, then dirty at any moment: the frame draws now.
        c.rest();
        assert!(c.due(30, 3.0, period));
        assert_eq!(c.interval(3.0), 0.0, "no interval after a rest");
        c.drew(30, 3.0, period);
        assert!(!c.due(30, 3.0 + 2.0 * period, period));
        // Raised to 120: the next frame is due one new interval after the last draw.
        assert!(c.due(120, 3.0 + period, period));
        c.drew(120, 3.0 + period, period);
        assert!(c.due(120, 3.0 + 2.0 * period, period));
        // Lowered to 30 again: an interval of 30 from the last draw.
        assert!(!c.due(30, 3.0 + 3.0 * period, period));
        assert!(c.due(30, 3.0 + 5.0 * period, period));
        // No cap: every frame.
        assert!(c.due(0, 3.0 + 1.5 * period, period));
    }

    #[test]
    fn thermal_codes_are_the_hosts_and_any_other_is_unknown() {
        for (code, state) in [
            (0, ThermalState::Unknown),
            (1, ThermalState::Nominal),
            (2, ThermalState::Fair),
            (3, ThermalState::Serious),
            (4, ThermalState::Critical),
            (5, ThermalState::Unknown),
            (u32::MAX, ThermalState::Unknown),
        ] {
            assert_eq!(ThermalState::from_code(code), state);
            if state != ThermalState::Unknown {
                assert_eq!(state as u32, code);
            }
        }
        assert_eq!(ThermalState::Unknown.keyword(), None);
        assert_eq!(ThermalState::Serious.keyword(), Some("serious"));
    }

    #[test]
    fn the_lower_non_zero_cap_stands() {
        assert_eq!(lower_cap(0, 60), 60);
        assert_eq!(lower_cap(30, 0), 30);
        assert_eq!(lower_cap(30, 60), 30);
        assert_eq!(lower_cap(0, 0), 0);
    }
}
