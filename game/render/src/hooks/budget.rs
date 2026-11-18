use super::{Deadline, HookTime, Needs};
use crate::perf::Stamp;
use std::fmt::Write;

const HISTORY: usize = 8;

#[derive(Clone, Copy)]
pub(crate) struct FramePeriod {
    display_ms: f64,
    target_ms: f64,
}

impl FramePeriod {
    pub fn new(display_ms: f64, interval_ms: f64, cap: u32) -> Option<Self> {
        if !display_ms.is_finite() || display_ms <= 0. {
            return None;
        }
        let target_ms = if cap > 0 && interval_ms.is_finite() && interval_ms >= 1.5 * display_ms {
            // The GPU module clamps caps to at least 5 Hz.
            1000. / f64::from(cap.max(5))
        } else {
            display_ms
        };
        Some(Self {
            display_ms,
            target_ms,
        })
    }
}

pub(crate) fn preparing(needs: Needs, seekable: bool) -> bool {
    needs.contains(Needs::PENDING) && (!seekable || !needs.contains(Needs::STARVED))
}

fn deadline_offset(period: f64, tails: &[f64]) -> f64 {
    let tail = if tails.len() < HISTORY {
        period / 2.
    } else {
        tails.iter().copied().fold(0., f64::max)
    };
    period - tail
}

pub(crate) fn entry_allowance(allowance: u32, remaining_ms: f64) -> u32 {
    if remaining_ms <= 0. {
        allowance.min(1)
    } else {
        allowance
    }
}

#[derive(Default)]
pub(crate) struct Budget {
    last: Option<(f64, bool)>,
    bound_at: Option<f64>,
    period: Option<FramePeriod>,
    tails: [f64; HISTORY],
    samples: usize,
    over_frames: u64,
    over_ms: f64,
}
impl Budget {
    pub fn frame(
        &mut self,
        time: &mut HookTime,
        now_ms: f64,
        limit: u32,
        period: Option<FramePeriod>,
        start: Option<Stamp>,
    ) {
        let advanced = self.begin(now_ms, time.seekable, period, time.reset);
        time.allowance = if advanced { limit } else { 0 };
        time.deadline = start
            .zip(self.offset())
            .map(|(start, ms)| Deadline::new(start, ms));
    }

    pub fn bind(&mut self, now: f64) {
        if self.last.is_none() {
            self.bound_at.get_or_insert(now);
        }
    }

    fn begin(
        &mut self,
        now: f64,
        seekable: bool,
        period: Option<FramePeriod>,
        reset: bool,
    ) -> bool {
        let period = if seekable { None } else { period };
        let discontinuity = self.last.is_some_and(|(last, was_seekable)| {
            seekable != was_seekable || (!seekable && now - last > 250.)
        });
        if reset
            || discontinuity
            || self.period.map(|p| p.display_ms) != period.map(|p| p.display_ms)
        {
            self.clear_history();
        }
        self.period = period;
        let previous = self
            .last
            .filter(|&(_, was_seekable)| seekable == was_seekable)
            .map(|(last, _)| last)
            .or(self.bound_at.take());
        let advanced = previous.map_or(!seekable, |last| now > last);
        self.last = Some((previous.map_or(now, |last| last.max(now)), seekable));
        advanced
    }

    fn offset(&self) -> Option<f64> {
        self.period
            .map(|period| deadline_offset(period.target_ms, &self.tails[..self.samples]))
    }

    pub fn reset_clock(&mut self) {
        self.last = None;
        self.clear_history();
    }

    fn clear_history(&mut self) {
        self.samples = 0;
    }

    pub fn finish(&mut self, tail_ms: f64) {
        if self.samples == HISTORY {
            self.tails.rotate_left(1);
            self.samples -= 1;
        }
        self.tails[self.samples] = tail_ms;
        self.samples += 1;
    }

    pub fn record(&mut self, remaining_ms: f64) {
        if remaining_ms < 0. {
            self.over_frames += 1;
            self.over_ms -= remaining_ms;
        }
    }

    pub fn reset_counters(&mut self) {
        self.over_frames = 0;
        self.over_ms = 0.;
    }

    pub fn append(&self, out: &mut String, allowance: u32) {
        write!(
            out,
            "{{\"allowance\":{allowance},\"overFrames\":{},\"overMs\":{}}}",
            self.over_frames, self.over_ms
        )
        .unwrap();
    }
}

pub(crate) struct PrepareSample {
    pub remaining_ms: f64,
    pub end: Stamp,
}

#[cfg(test)]
mod tests {
    use super::{deadline_offset, entry_allowance, preparing, Budget, Deadline, FramePeriod};
    use crate::{HookTime, Hooks, Needs};

    #[test]
    fn allowance_follows_clock_advances() {
        assert_eq!(<() as Hooks>::ALLOWANCE, 0);
        assert_eq!(entry_allowance(64, 1.), 64);
        assert_eq!(entry_allowance(64, 0.), 1);
        assert_eq!(entry_allowance(64, -1.), 1);
        assert_eq!(entry_allowance(0, -1.), 0);

        let mut budget = Budget::default();
        let frames = [
            (0., true),
            (0., true),
            (16., true),
            (16., true),
            (8., true),
            (32., false),
            (32., false),
            (31., false),
            (48., true),
            (64., true),
        ];
        assert_eq!(
            frames.map(|(now, seekable)| budget.begin(now, seekable, None, false)),
            [false, false, true, false, false, true, false, false, false, true]
        );

        let look = HookTime::default();
        assert_eq!(look.allowance, 0);
        assert!(look.deadline.is_none());

        let mut time = look;
        let mut budget = Budget::default();
        budget.frame(&mut time, 0., 64, None, None);
        assert_eq!(time.allowance, 0);
        assert!(time.deadline.is_none());
        budget.frame(
            &mut time,
            16.,
            64,
            FramePeriod::new(16., 0., 0),
            Some(crate::perf::Stamp::now()),
        );
        assert_eq!(time.allowance, 64);
        assert!(time.deadline.is_none());
    }

    #[test]
    fn first_live_frame_has_an_allowance() {
        let mut budget = Budget::default();
        assert!(budget.begin(0., false, None, true));
        assert!(!budget.begin(0., false, None, false));
    }

    #[test]
    fn preparation_budget_never_grants_twice_after_a_backward_stamp() {
        for seekable in [true, false] {
            let mut budget = Budget::default();
            budget.begin(16., seekable, None, false);
            assert_eq!(
                [32., 31., 32.].map(|now| budget.begin(now, seekable, None, false)),
                [true, false, false],
                "seekable={seekable}"
            );
        }
    }

    #[test]
    fn preparation_budget_targets_the_cap_on_both_skipped_intervals() {
        assert_eq!(
            [33.3, 50., 16.7, 0., f64::NAN, f64::INFINITY]
                .map(|interval| FramePeriod::new(16.7, interval, 25).map(|p| p.target_ms)),
            [
                Some(40.),
                Some(40.),
                Some(16.7),
                Some(16.7),
                Some(16.7),
                Some(16.7)
            ]
        );
    }

    #[test]
    fn preparation_budget_keeps_tail_history_across_targets() {
        let mut budget = Budget::default();
        budget.begin(0., false, FramePeriod::new(16.7, 33.3, 25), true);
        for _ in 0..8 {
            budget.finish(3.);
        }
        for (now, display, interval, offset) in [
            (50., 16.7, 50., 37.),
            (66.7, 16.7, 16.7, 16.7 - 3.),
            (100., 16.7, 33.3, 37.),
            (133., 33., 33., 16.5),
        ] {
            budget.begin(now, false, FramePeriod::new(display, interval, 25), false);
            assert_eq!(budget.offset(), Some(offset));
        }
    }

    #[test]
    fn agent_settles_pending_work_that_can_progress_on_redraw() {
        let needs = [
            Needs::PENDING,
            Needs::PENDING | Needs::STARVED,
            Needs::STARVED,
            Needs::NONE,
        ];
        assert_eq!(
            needs.map(|needs| preparing(needs, true)),
            [true, false, false, false]
        );
        assert_eq!(
            needs.map(|needs| preparing(needs, false)),
            [true, true, false, false]
        );
    }

    #[test]
    fn deadline_uses_eight_recent_tail_samples() {
        let target =
            |display, interval, cap| FramePeriod::new(display, interval, cap).map(|p| p.target_ms);
        assert_eq!(target(16., 32., 0), Some(16.));
        assert_eq!(target(16., 16., 120), Some(16.));
        assert_eq!(target(16., 23.9, 25), Some(16.));
        assert_eq!(target(16., 24., 25), Some(40.));
        assert_eq!(target(16., 200., 2), Some(200.));
        assert_eq!(target(0., 40., 25), None);
        assert_eq!(target(f64::NAN, 40., 25), None);
        assert_eq!(deadline_offset(40., &[3.; 7]), 20.);
        assert_eq!(
            deadline_offset(40., &[3., 4., 5., 6., 7., 8., 9., 10.]),
            30.
        );
        assert_eq!(deadline_offset(16., &[20.; 8]), -4.);

        let mut budget = Budget::default();
        budget.begin(0., false, FramePeriod::new(40., 0., 0), true);
        budget.finish(10.);
        for _ in 1..8 {
            budget.finish(3.);
        }
        assert_eq!(budget.offset(), Some(30.));
        budget.finish(3.);
        assert_eq!(budget.offset(), Some(37.));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn preparation_budget_uses_the_canvas_interval_to_check_the_cap() {
        use crate::{FrameView, HookGpu, RenderError, RenderWorld, WorldSurface};
        use exact_game::{Game, Input, World};
        use exact_gpu::{fixture, Frame, FrameTiming, Surface};

        thread_local! {
            static CAP_READS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        }

        struct Empty;
        impl Game for Empty {
            type Args = ();
            const ID: &'static str = "budgeted-capped-world";
            fn setup(_: &mut World, _: &()) {}
            fn tick(_: &mut World, _: &Input, _: &()) {}
            fn max_fps(_: &World, _: &()) -> u32 {
                CAP_READS.with(|reads| reads.set(reads.get() + 1));
                25
            }
        }

        #[derive(Default)]
        struct Capped(Option<Deadline>);
        impl Hooks for Capped {
            const ALLOWANCE: u32 = 64;
            fn prepare(
                &mut self,
                _: &HookGpu<'_>,
                view: &FrameView<'_>,
                _: &RenderWorld<'_>,
            ) -> Result<(), RenderError> {
                self.0 = view.time.deadline;
                Ok(())
            }
        }

        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let mut surface = WorldSurface::<Empty, (), false, Capped>::default();
        surface.bind(&[], None).unwrap();
        surface.clock(false);
        let mut frame = Frame {
            width: 64.,
            height: 64.,
            scale: 1.,
            now_ms: 0.,
            seekable: false,
            period_ms: 16.7,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.0,
        };
        for (reads, interval_ms, target) in [(0, 33.3, 16.7), (1, 33.3, 40.), (2, 50., 40.)] {
            if reads > 0 {
                assert_eq!(surface.max_fps(), 25);
            }
            surface.timing(&FrameTiming {
                interval_ms,
                display_period_ms: frame.period_ms,
                ..FrameTiming::default()
            });
            frame.now_ms += interval_ms;
            fixture::render(&gpu, &mut surface, &frame).unwrap();
            assert_eq!(CAP_READS.with(|count| count.get()), reads);
            // Before eight samples, half the target is reserved for the tail.
            assert_eq!(surface.render_hooks().0.unwrap().offset_ms, target / 2.);
        }
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn preparation_budget_reserves_only_work_after_prepare() {
        use crate::{FrameView, HookGpu, RenderError, RenderWorld, WorldSurface};
        use exact_game::{Entity, Game, Input, World};
        use exact_gpu::{fixture, Frame, Surface};
        use std::time::{Duration, Instant};

        struct Empty;
        impl Game for Empty {
            type Args = ();
            const ID: &'static str = "budgeted-encode-tail";
            const SAMPLES: exact_game::SceneSamples = exact_game::SceneSamples::One;
            fn setup(_: &mut World, _: &()) {}
            fn tick(_: &mut World, _: &Input, _: &()) {}
        }

        fn spin(ms: u64) {
            let start = Instant::now();
            while start.elapsed() < Duration::from_millis(ms) {
                std::hint::spin_loop();
            }
        }

        #[derive(Default)]
        struct SlowPrepare(Option<Deadline>);
        impl Hooks for SlowPrepare {
            const ALLOWANCE: u32 = 64;
            fn entities(&self) -> &[Entity] {
                // Work before prepare: encode minus prepare would count it as tail.
                spin(20);
                &[]
            }
            fn prepare(
                &mut self,
                _: &HookGpu<'_>,
                view: &FrameView<'_>,
                _: &RenderWorld<'_>,
            ) -> Result<(), RenderError> {
                spin(2);
                self.0 = view.time.deadline;
                Ok(())
            }
        }

        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let mut surface = WorldSurface::<Empty, (), false, SlowPrepare>::default();
        surface.bind(&[], None).unwrap();
        let mut frame = Frame {
            width: 64.,
            height: 64.,
            scale: 1.,
            now_ms: 0.,
            seekable: false,
            period_ms: 0.,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.,
        };
        fixture::render(&gpu, &mut surface, &frame).unwrap();
        frame.period_ms = 16.;
        // The tail is the largest of eight frames, and a frame that makes a pipeline is
        // slow after prepare for real: take the best of a few windows.
        let mut tails = Vec::new();
        for _ in 0..5 {
            for _ in 0..=8 {
                frame.now_ms += 16.;
                fixture::render(&gpu, &mut surface, &frame).unwrap();
            }
            tails.push(frame.period_ms - surface.render_hooks().0.unwrap().offset_ms);
            if tails.last().is_some_and(|tail| *tail < 20.) {
                return;
            }
        }
        panic!("every window held the 20 ms before prepare in its tail: {tails:?} ms");
    }

    #[test]
    fn each_discontinuity_clears_the_deadline_history() {
        for (now, seekable, period, reset, expected) in [
            (250., false, Some(40.), false, Some(37.)),
            (251., false, Some(40.), false, Some(20.)),
            (16., false, Some(20.), false, Some(10.)),
            (16., false, Some(40.), true, Some(20.)),
            (16., true, Some(40.), false, None),
            (16., false, None, false, None),
        ] {
            let mut budget = Budget::default();
            let original = FramePeriod::new(40., 0., 0);
            budget.begin(0., false, original, true);
            for _ in 0..8 {
                budget.finish(3.);
            }
            assert_eq!(budget.offset(), Some(37.));
            budget.begin(
                now,
                seekable,
                period.and_then(|period| FramePeriod::new(period, 0., 0)),
                reset,
            );
            assert_eq!(budget.offset(), expected);
            if seekable {
                budget.begin(32., false, original, false);
                assert_eq!(budget.offset(), Some(20.));
            }
        }
    }

    #[test]
    fn deadline_methods_read_the_monotonic_clock_once_each() {
        use crate::perf::{Stamp, CLOCK_READS};
        let deadline = Deadline::new(Stamp::now(), -2.);
        CLOCK_READS.with(|n| n.set(0));
        assert!(deadline.passed());
        assert_eq!(deadline.remaining_ms(), 0.);
        assert_eq!(CLOCK_READS.with(|n| n.get()), 2);
    }

    #[test]
    fn overruns_count_only_elapsed_work_past_the_deadline() {
        let mut budget = Budget::default();
        for remaining in [1., 0., -1., -2.5] {
            budget.record(remaining);
        }
        let mut out = String::new();
        budget.append(&mut out, 64);
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"allowance": 64, "overFrames": 2, "overMs": 3.5})
        );
        budget.reset_counters();
        out.clear();
        budget.append(&mut out, 64);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&out).unwrap(),
            serde_json::json!({"allowance": 64, "overFrames": 0, "overMs": 0})
        );
    }
}
