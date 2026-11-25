//! The Android frame and input clock, also read by extensions on other threads.
//! Live time retains its origin across turns that commit no runner work.

use std::{sync::Mutex, time::Instant};

#[derive(Clone, Copy)]
pub(crate) enum Clock {
    Live { started: Instant, offset: f64 },
    Held(f64),
}

impl Clock {
    pub(super) fn for_turn(started: Instant, held: Option<f64>, offset: f64) -> Self {
        held.map_or(Self::Live { started, offset }, Self::Held)
    }

    pub(super) fn now(self) -> f64 {
        self.at(Instant::now())
    }

    fn at(self, wall: Instant) -> f64 {
        match self {
            Self::Live { started, offset } => {
                wall.duration_since(started).as_secs_f64() * 1000.0 - offset
            }
            Self::Held(now) => now,
        }
    }
}

static CLOCK: Mutex<Clock> = Mutex::new(Clock::Held(0.0));

pub(super) fn set(clock: Clock) {
    *CLOCK.lock().unwrap_or_else(|e| e.into_inner()) = clock;
}

/// Publish the frame clock after a host turn, including one with no runner commit.
pub(super) fn publish(started: Instant, held: Option<f64>, offset: f64) {
    set(Clock::for_turn(started, held, offset));
}

/// The reader can ask while the owner is inside an agent operation.
/// A capture lease keeps its live origin even when that operation services it.
pub(crate) fn advance(now: f64) {
    let mut clock = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Clock::Held(at) = &mut *clock {
        *at = now.max(*at);
    }
}

pub(super) fn hold(floor: f64) -> f64 {
    let mut clock = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = clock.now().max(floor);
    *clock = Clock::Held(now);
    now
}

pub(super) fn now() -> f64 {
    read_at(Instant::now())
}

fn read_at(wall: Instant) -> f64 {
    CLOCK.lock().unwrap_or_else(|e| e.into_inner()).at(wall)
}

#[cfg(target_os = "android")]
impl<D: exact_runner::DataSource + Default> super::CanvasHost<D> {
    pub(super) fn clock(&self) -> Clock {
        Clock::for_turn(
            self.started,
            super::agent::clock(&self.p),
            super::agent::offset(),
        )
    }

    pub(super) fn now(&self) -> f64 {
        self.clock().now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_runner::{DataError, DataSource, Value};
    use std::time::Duration;

    struct NoData;
    impl DataSource for NoData {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
    }

    #[test]
    fn published_frame_clock_delivers_sixty_hz_samples_without_runner_commits() {
        use crate::presenter::{PainterChoice, Presenter};

        let plan = crate::test_plan("component App\n  view\n    canvas surface=ball()\n");
        let (mut p, error) = Presenter::boot_with(
            &plan,
            NoData,
            (40.0, 40.0),
            1.0,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let started = Instant::now();
        let period = 1000.0 / 60.0;
        let runner_at = p.host().runner().now_ms();
        for held in [false, true] {
            set(Clock::Held(0.0));
            let mut next = f64::NEG_INFINITY;
            let mut delivered = Vec::new();
            for frame in 1..=60 {
                let wall = started + Duration::from_secs_f64(f64::from(frame) / 60.0);
                let frame_at = wall.duration_since(started).as_secs_f64() * 1000.0;
                // GPU frames move presentation without committing runner work.
                p.tick(frame_at);
                assert_eq!(p.host().runner().now_ms(), runner_at);
                publish(started, held.then_some(now()), 0.0);
                if held {
                    advance(frame_at);
                }
                // Five milliseconds of delivery age: the event keeps its own time.
                let at = read_at(wall) - 5.0;
                if at < next - period / 2.0 {
                    continue;
                }
                next = (next + period).max(at + period / 2.0);
                delivered.push(at);
            }
            assert_eq!(delivered.len(), 60, "held={held}");
            assert!(delivered.windows(2).all(|pair| pair[0] < pair[1]));
            assert!((delivered[59] - 995.0).abs() < 1e-6);
        }

        set(Clock::Held(250.0));
        assert_eq!(std::thread::spawn(now).join().unwrap(), 250.0);
        advance(500.0);
        assert_eq!(now(), 500.0);

        let started = Instant::now() - Duration::from_secs(10);
        publish(started, None, 9750.0);
        let before = std::thread::spawn(now).join().unwrap();
        assert!(before >= 250.0);
        advance(0.0);
        assert!(
            now() >= before,
            "a serviced callback must not rebase live time"
        );
        let stopped = hold(0.0);
        assert!(
            stopped >= before,
            "returning a lease cannot move time backwards"
        );
        advance(0.0);
        assert_eq!(now(), stopped);
        assert_eq!(hold(stopped + 1000.0), stopped + 1000.0);
    }

    #[test]
    fn a_capture_lease_advances_from_the_held_time_then_holds_again() {
        let started = Instant::now();
        let wall = started + Duration::from_secs(10);
        let held = Clock::for_turn(started, Some(250.0), 0.0);
        assert_eq!(held.at(wall), 250.0);
        assert_eq!(held.at(wall + Duration::from_secs(1)), 250.0);
        let lent = Clock::for_turn(started, None, 9750.0);
        assert_eq!(lent.at(wall), 250.0);
        assert_eq!(lent.at(wall + Duration::from_secs(1)), 1250.0);
        let held = Clock::for_turn(started, Some(1250.0), 0.0);
        assert_eq!(held.at(wall + Duration::from_secs(2)), 1250.0);
    }
}
