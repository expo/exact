//! When a contact's input happened, which its holds' release velocity reads
//! (LLP 1057.001 §3).
use super::*;

/// A platform's input times carried onto the session clock. A contact's
/// down is placed by its age when it is delivered, and the rest of the
/// contact keeps the down's offset: its samples stay as far apart as the
/// platform stamped them, however late each is delivered and whatever the
/// session clock does meanwhile.
#[cfg(any(target_os = "android", test))]
#[derive(Default)]
pub(crate) struct InputClock {
    offset: Option<f64>,
}

#[cfg(any(target_os = "android", test))]
impl InputClock {
    /// Android gives both current and historical samples in uptime nanoseconds.
    #[cfg(any(target_os = "android", all(test, unix)))]
    pub(crate) fn android(&mut self, down: bool, at_ns: i64, now: f64) -> f64 {
        if at_ns < 0 {
            if down {
                self.offset = None;
            }
            return now;
        }
        let read = crate::launch_marks::clock(libc::CLOCK_MONOTONIC) * 1000.;
        self.place(down, at_ns as f64 / 1e6, read, now)
    }

    /// The session time (ms) of an input the platform stamped `at` ms on
    /// its own clock, delivered when that clock reads `read` and the
    /// session's `now`. A `down` begins a contact.
    fn place(&mut self, down: bool, at: f64, read: f64, now: f64) -> f64 {
        let offset = match self.offset {
            Some(offset) if !down => offset,
            _ => *self.offset.insert(now - (read - at).max(0.) - at),
        };
        at + offset
    }
}

impl<D: DataSource> Presenter<D> {
    /// [`Presenter::pointer_down`] for a down that happened at `event_ms` on
    /// the session clock.
    pub(crate) fn pointer_down_at(
        &mut self,
        x: f32,
        y: f32,
        now_ms: f64,
        event_ms: f64,
    ) -> Result<bool, String> {
        self.pointer_down_counted(x, y, now_ms, event_ms, 1)
    }
    pub(crate) fn contact_event_ms(&self, platform_ms: Option<f64>) -> Option<f64> {
        let (event_ms, delivered_ms) = self
            .contact
            .as_ref()
            .map(|c| (c.event_ms, c.last_ms))
            .or_else(|| self.stage_contact().map(|c| (c.event_ms, c.delivered_ms)))?;
        // A real sample may already be old at delivery. A synthesized move
        // can instead end ahead of delivery and accumulates idle time from there.
        Some(platform_ms.map_or(event_ms, |now| {
            event_ms + (now - event_ms.min(delivered_ms)).max(0.)
        }))
    }
    pub(crate) fn delay_contact_event(&mut self, ms: f64) {
        if let Some(contact) = &mut self.contact {
            contact.event_ms += ms;
        }
        self.stage_delay_contact(ms);
    }
}

#[cfg(test)]
mod tests {
    use super::InputClock;

    #[test]
    fn a_contact_keeps_its_downs_offset_however_late_each_input_arrives() {
        let mut clock = InputClock::default();
        // Down at 1000 on the platform's clock, read 5 ms later at session 200.
        assert_eq!(clock.place(true, 1000., 1005., 200.), 195.);
        // Two moves 8 ms apart, delivered together 40 ms late.
        assert_eq!(clock.place(false, 1008., 1056., 200.), 203.);
        assert_eq!(clock.place(false, 1016., 1056., 200.), 211.);
        // The next down starts again from its own age.
        assert_eq!(clock.place(true, 2000., 2000., 900.), 900.);
        assert_eq!(clock.place(true, 2000., 0., 900.), 900.);
    }
}
