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
