//! How many frames Ink keeps in flight on a composition swap chain.
use std::time::{Duration, Instant};

/// How many frames may be in flight, chosen as frames go: one, and two only
/// while frames miss their refresh.
///
/// At one, a frame starts at the refresh that took the last, which is the
/// least delay to the screen. But a frame that misses its refresh holds the
/// next one back until the refresh after, and the rate halves. At two, the
/// next frame is prepared while the last waits its turn, so a slow frame
/// costs only its own time; when frames are on time, though, the queue
/// stays full and everything shows later. So one is tried again once frames
/// reach their present well within a refresh, after a wait that doubles
/// each time the try misses at once.
///
/// On a 120 Hz display (Radeon 8060S), a frame that takes 6 ms shows 120
/// times a second at either limit, 18 ms after it starts at one and 26 ms
/// at two. A frame that takes 8 ms shows 60 times a second at one and 115
/// at two.
pub(super) struct Pace {
    pub(super) limit: u32,
    /// When the frame in hand was granted its place.
    started: Option<Instant>,
    /// How long the last frame took from its place to its present.
    took: Option<Duration>,
    /// When the place not yet granted was first asked for.
    pub(super) asked: Option<Instant>,
    /// Frames in a row that missed their refresh (at one), or that would
    /// have made it (at two).
    run: u32,
    /// At two: one is not tried before this, and how long that wait was.
    retry: Option<(Instant, Duration)>,
    /// When one was last tried.
    tried: Option<Instant>,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            limit: 1,
            started: None,
            took: None,
            asked: None,
            run: 0,
            retry: None,
            tried: None,
        }
    }
}

impl Pace {
    /// Misses in a row before two frames may be in flight.
    const MISSES: u32 = 2;
    /// Frames in a row with time to spare at two before one is tried again.
    pub(super) const SPARE: u32 = 30;
    /// The first wait before one is tried again, doubled up to the longest
    /// each time the try misses at once.
    const RETRY: Duration = Duration::from_secs(1);
    const LONGEST_RETRY: Duration = Duration::from_secs(16);

    /// The frame in hand goes to its present.
    pub(super) fn presented(&mut self, now: Instant) {
        self.took = self.started.map(|s| now.saturating_duration_since(s));
    }

    #[cfg(test)]
    pub(super) fn took(&self) -> Option<Duration> {
        self.took
    }

    /// A place asked for at `asked` and granted at `now`, on a display that
    /// refreshes every `period`: the limit to change to, if it changes.
    pub(super) fn granted(
        &mut self,
        asked: Instant,
        now: Instant,
        period: Duration,
    ) -> Option<u32> {
        let waited = now.saturating_duration_since(asked) > period / 8;
        let since = self.started.replace(now).map(|s| now.duration_since(s));
        let took = self.took.take();
        let counts = match self.limit {
            // Held back past the refresh after its own start: the last missed.
            1 => waited && since.is_some_and(|since| since > period * 3 / 2),
            // Presented well within a refresh of its place: on time at one
            // too. The wait for a place says nothing here, because the host
            // starts frames at the display's rate and none is held back.
            _ => took.is_some_and(|took| took < period * 3 / 4),
        };
        self.run = if counts { self.run + 1 } else { 0 };
        if self.limit == 1 && self.run >= Self::MISSES {
            let again = self
                .tried
                .is_some_and(|t| now.duration_since(t) < Self::RETRY);
            let wait = match self.retry {
                Some((_, last)) if again => (last * 2).min(Self::LONGEST_RETRY),
                _ => Self::RETRY,
            };
            self.retry = Some((now + wait, wait));
            self.limit = 2;
        } else if self.limit == 2
            && self.run >= Self::SPARE
            && self.retry.is_none_or(|(at, _)| now >= at)
        {
            self.tried = Some(now);
            self.limit = 1;
        } else {
            return None;
        }
        self.run = 0;
        Some(self.limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_frames_are_in_flight_only_while_frames_miss_their_refresh() {
        let period = Duration::from_micros(8333);
        let mut pace = Pace::default();
        let mut now = Instant::now();
        // The frame in hand presents `took` after its place. The next is
        // granted its place `since` after it, having waited `waited`.
        let mut frame = |pace: &mut Pace, took: Duration, waited: Duration, since: Duration| {
            pace.presented(now + took);
            now += since;
            (pace.granted(now - waited, now, period), now)
        };
        type Frame<'a> =
            &'a mut dyn FnMut(&mut Pace, Duration, Duration, Duration) -> (Option<u32>, Instant);
        let on_time = |pace: &mut Pace, frame: Frame| frame(pace, period / 2, period / 2, period);
        let missed =
            |pace: &mut Pace, frame: Frame| frame(pace, period * 9 / 8, period, period * 2);
        for _ in 0..100 {
            assert_eq!(on_time(&mut pace, &mut frame).0, None);
        }
        // One miss, and a frame after a pause, change nothing.
        assert_eq!(missed(&mut pace, &mut frame).0, None);
        assert_eq!(on_time(&mut pace, &mut frame).0, None);
        assert_eq!(
            frame(&mut pace, period / 2, Duration::ZERO, period * 40).0,
            None
        );
        // Two misses in a row: the next frame no longer waits for the last.
        assert_eq!(missed(&mut pace, &mut frame).0, None);
        let (limit, raised) = missed(&mut pace, &mut frame);
        assert_eq!(limit, Some(2));
        // Frames that take most of a refresh would miss at one, held back or
        // not: it stays.
        for waited in [Duration::ZERO, period / 2] {
            for _ in 0..200 {
                let slow = frame(&mut pace, period * 7 / 8, waited, period * 9 / 8);
                assert_eq!(slow.0, None);
            }
        }
        // Frames with time to spare would make it, though the host's own
        // cadence holds none of them back: one is tried again.
        let mut spare = 0;
        let tried = loop {
            spare += 1;
            assert!(spare <= Pace::SPARE);
            if let (Some(limit), at) = frame(&mut pace, period / 2, Duration::ZERO, period) {
                assert_eq!(limit, 1);
                break at;
            }
        };
        assert_eq!(spare, Pace::SPARE);
        assert!(tried - raised >= Pace::RETRY);
        // The try misses at once, so the next waits twice as long.
        assert_eq!(missed(&mut pace, &mut frame).0, None);
        let (limit, raised) = missed(&mut pace, &mut frame);
        assert_eq!(limit, Some(2));
        let tried = (0..1000)
            .find_map(|_| {
                let (limit, at) = frame(&mut pace, period / 2, Duration::ZERO, period);
                limit.map(|_| at)
            })
            .unwrap();
        assert!(tried - raised >= Pace::RETRY * 2);
        // A try that holds starts the waits over.
        for _ in 0..200 {
            assert_eq!(on_time(&mut pace, &mut frame).0, None);
        }
        assert_eq!(missed(&mut pace, &mut frame).0, None);
        assert_eq!(missed(&mut pace, &mut frame).0, Some(2));
        assert_eq!(pace.retry.unwrap().1, Pace::RETRY);
    }
}
