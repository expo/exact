//! When the window draws its next frame: the display's cadence, a retry,
//! and a presentation the compositor has not taken yet.
use std::time::{Duration, Instant};
use winit::event::WindowEvent;

pub(super) fn next_frame_deadline(
    deadline: Instant,
    started: Instant,
    period: Duration,
) -> Instant {
    if started < deadline {
        // An input-triggered redraw does not move the display's cadence.
        deadline
    } else if started < deadline + period {
        // Keep timer wake-up latency from accumulating on every frame.
        deadline + period
    } else {
        // After a stall or suspension, resume without a burst of catch-up frames.
        started + period
    }
}

pub(super) struct FrameSchedule {
    pub(super) cadence: Option<Instant>,
    pub(super) retry: Option<Instant>,
    pub(super) present: Option<Instant>,
}

pub(super) fn presentation_resumes(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::Occluded(false)
            | WindowEvent::Focused(true)
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
    )
}

impl FrameSchedule {
    pub(super) fn admit(&self, now: Instant) -> Instant {
        self.cadence
            .into_iter()
            .chain(self.present)
            .fold(now, Instant::max)
    }

    pub(super) fn defer(&self, now: Instant, pending: &mut bool) -> bool {
        *pending = self.admit(now) > now;
        *pending
    }

    pub(super) fn redraw(&self, now: Instant, requested: bool) -> Option<Instant> {
        let requested = if requested { Some(now) } else { self.retry }?;
        Some(requested.max(self.admit(now)))
    }
}

/// The run's frame statistics, on stderr: the means, and the recent frames'
/// medians and 95th percentiles.
pub(super) fn report(frames: u64, paint_ms: f64, present_ms: f64, samples: &[(f64, f64, f64)]) {
    if frames > 0 {
        eprintln!(
            "exact-windows: {} frames; mean paint {:.2} ms; mean present {:.2} ms",
            frames,
            paint_ms / frames as f64,
            present_ms / frames as f64
        );
        let percentile = |column: fn(&(f64, f64, f64)) -> f64, fraction: f64| {
            let mut values: Vec<_> = samples.iter().map(column).collect();
            values.sort_by(f64::total_cmp);
            values[((values.len() - 1) as f64 * fraction).round() as usize]
        };
        if !samples.is_empty() {
            eprintln!("exact-windows: recent {} frames p50/p95 ms: interval {:.2}/{:.2}, paint {:.2}/{:.2}, present {:.2}/{:.2}",
                samples.len(), percentile(|s| s.0, 0.5), percentile(|s| s.0, 0.95),
                percentile(|s| s.1, 0.5), percentile(|s| s.1, 0.95),
                percentile(|s| s.2, 0.5), percentile(|s| s.2, 0.95));
        }
    }
}

impl<D: exact_runner::DataSource + Default + 'static> super::App<D> {
    /// Tell the counters the window's display's refresh rate.
    pub(super) fn observe_monitor(&self) {
        if exact_ink::counters::enabled() {
            let hz = self
                .window
                .as_ref()
                .and_then(|w| w.current_monitor())
                .and_then(|m| m.refresh_rate_millihertz())
                .unwrap_or(60_000);
            exact_ink::counters::refresh_rate(hz);
        }
    }
}
