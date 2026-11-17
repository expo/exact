//! Bounded display-callback timing. These are scheduling proxies, never presentation evidence.
//!
//! One reducer for the native hosts (LLP 1079.000 §5.1): the Apple host's display-link callbacks
//! and the Android reader's Choreographer turns and window frames are recorded here, each source
//! in a collector of its own on the thread that owns it.

use std::cell::RefCell;
#[path = "frame_health/history.rs"]
mod history;
pub use history::{Hitch, Status as HistoryStatus, Work, CAPACITY as HITCH_CAPACITY, MAX_AGE_NS};

/// Scalar counters copied only when a reader asks, on the producer's thread. A frame is a
/// callback that ran its work; LLP 1079.000 §4.6 defines each measure.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    /// Collector generation, advanced by reset.
    pub generation: u64,
    /// Callbacks observed, frames and held callbacks alike.
    pub callbacks: u64,
    /// Callbacks that started no frame work because earlier work was still in flight: not frames.
    pub held: u64,
    /// Intervals between consecutive frames' timestamps while the source ran.
    pub intervals: u64,
    /// Sum of those intervals.
    pub active_ns: u64,
    /// Most recent frame's scheduled period.
    pub period_ns: u64,
    /// Longest frame interval since reset.
    pub max_interval_ns: u64,
    /// Scheduled-period changes between callbacks within a run; the frame interval across one has
    /// no refresh count.
    pub cadence_changes: u64,
    /// Frame intervals whose refresh count differs from the previous interval's.
    pub pacing_changes: u64,
    /// Most recent callback arrival.
    pub last_arrival_ns: u64,
    /// Whether an existing app frame clock is running.
    pub running: u64,
    /// Frames whose work has finished, one record each.
    pub worked: u64,
    /// Those that finished after their deadline.
    pub late: u64,
    /// Arrival of the callback that finalized the latest interval.
    pub interval_through_ns: u64,
    /// Finish of the latest work record.
    pub work_through_ns: u64,
    /// Saturated histogram increments invalidate distribution deltas.
    pub histogram_overflows: u64,
    /// Requested slots that have passed while the host requested a rate, each counted once.
    pub requested_slots: u64,
    /// Those slots in which no frame started.
    pub missed_frames: u64,
    /// Frame intervals with an end scheduled slower than the rate requested for that end.
    pub restricted_intervals: u64,
    /// Scheduled slots frame intervals skipped: their refreshes beyond the first.
    pub skipped_refreshes: u64,
    /// Scheduled-period changes between callbacks, across stopped time too, so a reader knows
    /// when a window's refresh counts came from more than one schedule.
    pub schedule_epochs: u64,
    /// Frame intervals across a schedule change, which have no refresh count.
    pub unclassified_intervals: u64,
    /// Callbacks whose frame timestamp did not advance; each ends the run.
    pub out_of_order: u64,
}

/// A callback that ran all its work before returning.
pub const COMPLETE: u32 = 0;
/// A callback whose owner work lands later and reports its finish through `worked`.
pub const PENDING: u32 = 1;
/// A callback that started no frame work because earlier work was still in flight: not a frame.
pub const HELD: u32 = 2;
/// A callback with no frame work due, such as one kept for a timer: not a frame, and it ends the
/// run, so idle time never becomes a frame time.
pub const IDLE: u32 = 3;

/// Saturating counts in half-open bins; a reader receives the first `N` of 64.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Distribution {
    /// Counts in half-open bins; layouts with fewer bins leave the rest zero.
    pub bins: [u32; 64],
}

impl Default for Distribution {
    fn default() -> Self {
        Self { bins: [0; 64] }
    }
}

#[derive(Clone, Copy)]
struct Bins<const N: usize>([u32; N]);

impl<const N: usize> Default for Bins<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> Bins<N> {
    /// Count one value in `index`, returning false when the bin is saturated.
    fn add(&mut self, index: usize) -> bool {
        let bin = &mut self.0[index.min(N - 1)];
        if *bin == u32::MAX {
            return false;
        }
        *bin += 1;
        true
    }

    /// Bins of 0.5 ms up to `linear` (N/2 of them), then doubling tails and an open last bin.
    fn duration(&mut self, duration: u64, linear: u64) -> bool {
        self.add(if duration < linear {
            (duration / 500_000) as usize
        } else {
            (linear / 500_000) as usize + (duration / linear).ilog2() as usize
        })
    }

    fn copy(&self) -> Distribution {
        let mut out = Distribution::default();
        out.bins[..N].copy_from_slice(&self.0);
        out
    }
}

/// The longest frame interval and the slowest work, with its budget (deadline minus start),
/// since the card's reader last took them.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Maxima {
    /// Longest interval between frame timestamps.
    pub longest_ns: u64,
    /// Slowest frame's work, from its start to its finish.
    pub slowest_ns: u64,
    /// That frame's deadline minus its start.
    pub budget_ns: u64,
}

#[derive(Default)]
struct Collector {
    snapshot: Snapshot,
    /// The previous frame's timestamp in this run, zero before the first.
    previous_frame: u64,
    /// The missed-frame grid's origin; zero re-anchors it at the next callback.
    origin: u64,
    /// The first slot on that grid not yet counted.
    next_slot: u64,
    /// One past a held callback's slot, which stays open until a frame starts in it or the run ends.
    held_slot: u64,
    /// The previous interval's refresh count, zero when it had none.
    previous_refreshes: u64,
    /// The previous frame's scheduled period, and the period requested when it started.
    frame_period: u64,
    frame_requested: u64,
    /// The previous callback's scheduled period in this run, zero at its start.
    run_period: u64,
    /// The schedule changed since the previous frame.
    changed: bool,
    /// The previous callback's scheduled period in any run.
    epoch_period: u64,
    /// The previous callback's frame timestamp.
    last_timestamp: u64,
    maxima: Maxima,
    history: history::History,
    /// Frame intervals: 0.5 ms bins below 24 ms, then doubling.
    intervals: Bins<64>,
    /// Work from each frame's start, in the same bins.
    work: Bins<64>,
    /// Refreshes each frame interval held, 1 to 31 and over.
    refreshes: Bins<32>,
    /// Finish past a late frame's deadline: 0.5 ms bins below 12 ms, then doubling.
    late_by: Bins<32>,
}

impl Collector {
    /// A callback whose frame starts at its target less its period, as a display link's
    /// timestamp does; without a period, at its arrival.
    fn record(
        &mut self,
        arrival: u64,
        target: u64,
        period: u64,
        finished: u64,
        outcome: u32,
        flags: u64,
    ) -> bool {
        if target < period {
            return false;
        }
        let timestamp = if period == 0 {
            arrival
        } else {
            target - period
        };
        self.record_at(arrival, timestamp, target, period, finished, outcome, flags)
    }

    /// A callback for the frame starting at `timestamp`, whose work is due by `target`. Android's
    /// Choreographer gives the two apart: a frame's deadline is not its vsync plus a period.
    #[allow(clippy::too_many_arguments)]
    fn record_at(
        &mut self,
        arrival: u64,
        timestamp: u64,
        target: u64,
        period: u64,
        finished: u64,
        outcome: u32,
        flags: u64,
    ) -> bool {
        if arrival == 0 || finished < arrival || target == 0 {
            return false;
        }
        let s = &mut self.snapshot;
        s.callbacks = s.callbacks.saturating_add(1);
        s.last_arrival_ns = arrival;
        s.running = 1;
        self.history
            .record(arrival, target, period, finished, flags);
        s.held = s.held.saturating_add(u64::from(outcome == HELD));
        // Schedule changes on every callback, held ones too: an interval spanning one has no
        // single period
        if period != 0 {
            // A timestamp that does not advance bounds no interval: it ends the run, and the next
            // callback starts another
            let backward = self.last_timestamp != 0 && timestamp <= self.last_timestamp;
            self.last_timestamp = timestamp;
            if backward {
                s.out_of_order = s.out_of_order.saturating_add(1);
                self.end_run();
                return false;
            }
            let s = &mut self.snapshot;
            let differs = |previous: u64| previous != 0 && previous.abs_diff(period) > 100_000;
            s.schedule_epochs = s
                .schedule_epochs
                .saturating_add(u64::from(differs(self.epoch_period)));
            if differs(self.run_period) {
                s.cadence_changes = s.cadence_changes.saturating_add(1);
                self.changed = true;
            }
            (self.epoch_period, self.run_period, s.period_ns) = (period, period, period);
        }
        if outcome == IDLE {
            self.end_run();
            return true;
        }
        // A source without a schedule (the native-scroll observer, a window's frame metrics) has
        // work but no frame times
        if period != 0 {
            self.slot(timestamp, outcome == HELD);
            if outcome != HELD {
                self.frame(arrival, timestamp, period);
            }
        }
        let start = timestamp;
        if outcome == COMPLETE {
            self.worked(start, target, finished);
        }
        true
    }

    /// Count the requested slots this callback's timestamp has passed. Each callback takes its
    /// nearest slot, so timestamp jitter cancels, and a slot counts once however many frames start in
    /// it. Slots no frame started in are missed; a held callback's own slot stays open, since a
    /// later callback may still start a frame in it.
    fn slot(&mut self, at: u64, held: bool) {
        let requested = self.history.requested_period_ns;
        if requested == 0 {
            return;
        }
        if self.origin == 0 {
            (self.origin, self.next_slot, self.held_slot) = (at, 0, 0);
        }
        let slot = at.saturating_sub(self.origin).saturating_add(requested / 2) / requested;
        let s = &mut self.snapshot;
        let passed = slot.saturating_sub(self.next_slot);
        if held {
            s.requested_slots = s.requested_slots.saturating_add(passed);
            s.missed_frames = s.missed_frames.saturating_add(passed);
            self.next_slot = self.next_slot.max(slot);
            self.held_slot = slot.saturating_add(1);
        } else if slot >= self.next_slot {
            s.requested_slots = s.requested_slots.saturating_add(passed.saturating_add(1));
            s.missed_frames = s.missed_frames.saturating_add(passed);
            self.next_slot = slot.saturating_add(1);
            self.held_slot = 0;
        }
    }

    /// Count the interval ending at this frame's timestamp.
    fn frame(&mut self, arrival: u64, frame: u64, period: u64) {
        let s = &mut self.snapshot;
        let changed = std::mem::take(&mut self.changed);
        let previous = self.previous_frame;
        if previous != 0 && frame > previous {
            let interval = frame - previous;
            s.intervals = s.intervals.saturating_add(1);
            s.active_ns = s.active_ns.saturating_add(interval);
            s.max_interval_ns = s.max_interval_ns.max(interval);
            s.interval_through_ns = arrival;
            self.maxima.longest_ns = self.maxima.longest_ns.max(interval);
            let mut saturated = !self.intervals.duration(interval, 24_000_000);
            // Refreshes need one period; across a schedule change there is none
            if changed {
                s.unclassified_intervals = s.unclassified_intervals.saturating_add(1);
                self.previous_refreshes = 0;
            } else {
                let refreshes = (interval.saturating_add(period / 2) / period).max(1);
                s.skipped_refreshes = s.skipped_refreshes.saturating_add(refreshes - 1);
                s.pacing_changes = s.pacing_changes.saturating_add(u64::from(
                    self.previous_refreshes != 0 && refreshes != self.previous_refreshes,
                ));
                self.previous_refreshes = refreshes;
                saturated |= !self.refreshes.add(refreshes.min(32) as usize - 1);
            }
            // Each end of an interval ran on its own schedule under its own request, which a schedule
            // or request change between them makes differ: one end slower than its request restricts it
            let slower = |period: u64, requested: u64| {
                requested != 0 && period > requested.saturating_add(100_000)
            };
            s.restricted_intervals = s.restricted_intervals.saturating_add(u64::from(
                slower(self.frame_period, self.frame_requested)
                    || slower(period, self.history.requested_period_ns),
            ));
            s.histogram_overflows = s.histogram_overflows.saturating_add(u64::from(saturated));
        }
        self.frame_period = period;
        self.frame_requested = self.history.requested_period_ns;
        self.previous_frame = frame;
    }

    /// Record one frame's work, from its start to its finish, against its deadline. Work may begin after its
    /// deadline, as the screen's does when its update waited past it.
    fn worked(&mut self, start: u64, deadline: u64, finished: u64) -> bool {
        if start == 0 || finished < start {
            return false;
        }
        let s = &mut self.snapshot;
        let duration = finished - start;
        s.worked = s.worked.saturating_add(1);
        s.work_through_ns = finished;
        let mut saturated = !self.work.duration(duration, 24_000_000);
        let late = finished > deadline;
        if late {
            s.late = s.late.saturating_add(1);
            saturated |= !self.late_by.duration(finished - deadline, 12_000_000);
        }
        s.histogram_overflows = s.histogram_overflows.saturating_add(u64::from(saturated));
        if duration > self.maxima.slowest_ns {
            self.maxima.slowest_ns = duration;
            self.maxima.budget_ns = deadline.saturating_sub(start);
        }
        late
    }

    /// Close the requested-slot grid. A held callback's slot is final now: no frame can start in it.
    fn close_grid(&mut self) {
        let open = self.held_slot.saturating_sub(self.next_slot);
        let s = &mut self.snapshot;
        s.requested_slots = s.requested_slots.saturating_add(open);
        s.missed_frames = s.missed_frames.saturating_add(open);
        self.origin = 0;
        self.held_slot = 0;
    }

    /// End the run.
    fn end_run(&mut self) {
        self.close_grid();
        self.previous_frame = 0;
        self.previous_refreshes = 0;
        self.run_period = 0;
        self.changed = false;
    }

    /// The session stopped wanting frames.
    fn stop(&mut self) {
        self.end_run();
        self.snapshot.running = 0;
        self.history.break_baseline();
    }
}

// A fixed table bounds both storage and lookup. Handles never reuse an old identity.
struct Registry {
    next: u32,
    slots: [Option<(u32, Collector)>; 16],
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            next: 1,
            slots: std::array::from_fn(|_| None),
        }
    }
}

thread_local! {
    static COLLECTORS: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Register on the UI thread; zero means capacity exhausted. All calls use that thread.
pub fn create() -> u32 {
    COLLECTORS.with_borrow_mut(|r| {
        let Some(next) = r.next.checked_add(1) else {
            return 0;
        };
        let Some(slot) = r.slots.iter_mut().find(|s| s.is_none()) else {
            return 0;
        };
        let id = r.next;
        r.next = next;
        *slot = Some((id, Collector::default()));
        id
    })
}

fn with<T: Default>(id: u32, f: impl FnOnce(&mut Collector) -> T) -> T {
    COLLECTORS.with_borrow_mut(|r| {
        r.slots
            .iter_mut()
            .flatten()
            .find(|(key, _)| *key == id)
            .map(|(_, collector)| f(collector))
            .unwrap_or_default()
    })
}

/// Record one existing callback, without allocation, clock reads, I/O or locks. `outcome` is
/// `COMPLETE`, `PENDING`, `HELD` or `IDLE`. Returns false for an observation the collector rejected,
/// whose pending work the host must then drop.
pub fn record(
    id: u32,
    arrival: u64,
    target: u64,
    period: u64,
    finished: u64,
    outcome: u32,
    flags: u64,
) -> bool {
    with(id, |c| {
        c.record(arrival, target, period, finished, outcome, flags)
    })
}

/// [`record`] for a frame starting at `timestamp` with its work due by `deadline`, which need not
/// be one `period` later.
#[allow(clippy::too_many_arguments)]
pub fn record_at(
    id: u32,
    arrival: u64,
    timestamp: u64,
    deadline: u64,
    period: u64,
    finished: u64,
    outcome: u32,
    flags: u64,
) -> bool {
    with(id, |c| {
        c.record_at(
            arrival, timestamp, deadline, period, finished, outcome, flags,
        )
    })
}

/// Record the finish of a `PENDING` frame's work, once the owner work it sent has landed.
pub fn worked(id: u32, start: u64, deadline: u64, finished: u64) {
    with(id, |c| {
        if c.worked(start, deadline, finished) {
            c.history.worked(start, deadline, finished);
        }
    });
}

/// Return and reset the per-read maxima. Only the card's sampler takes them.
pub fn take_maxima(id: u32) -> Maxima {
    with(id, |c| std::mem::take(&mut c.maxima))
}

/// Start, pause or clear bounded detail capture (actions 1, 0 and 2).
pub fn capture(id: u32, action: u32, now: u64) {
    with(id, |c| c.history.control(action, now));
}

/// Read capture/loss accounting on the producer thread.
pub fn history_status(id: u32) -> HistoryStatus {
    with(id, |c| c.history.status)
}

/// Copy one retained hitch, oldest first. An out-of-range read returns zero sequence.
pub fn hitch(id: u32, index: u32) -> Hitch {
    with(id, |c| c.history.get(index as usize))
}

/// Retain a completed main-thread apply as hitch evidence; frame work is recorded per frame.
pub fn applied(id: u32, work: Work, flags: u64) {
    with(id, |c| c.history.applied(work, flags));
}

/// Copy a histogram on the producer thread: 0 frame intervals, 1 work, 2 refreshes and 3 lateness.
pub fn distribution(id: u32, kind: u32) -> Distribution {
    with(id, |c| match kind {
        0 => c.intervals.copy(),
        1 => c.work.copy(),
        2 => c.refreshes.copy(),
        _ => c.late_by.copy(),
    })
}

/// Retain temporal work context without adding another source's apply/deadline population.
pub fn context(id: u32, work: Work) {
    with(id, |c| {
        c.history.context(work);
    });
}

/// Cache a changed host request instead of querying platform policy in the hook. A new request
/// re-anchors the missed-frame grid at the next callback; the interval across it counts no slot.
pub fn requested_period(id: u32, period: u64) {
    with(id, |c| {
        if c.history.requested_period_ns != period {
            // Slots already passed on the old grid stay counted
            c.close_grid();
            c.history.requested_period_ns = period;
        }
    });
}

/// Read counters; an unknown handle returns an empty snapshot.
pub fn snapshot(id: u32) -> Snapshot {
    with(id, |c| c.snapshot)
}

/// Break the interval baseline when the source stops, excluding idle time from gaps.
pub fn stop(id: u32) {
    with(id, Collector::stop);
}

/// Start a new generation after a reload or clock-domain change.
pub fn reset(id: u32) {
    with(id, |c| {
        let generation = c.snapshot.generation.saturating_add(1);
        c.stop();
        c.snapshot = Snapshot {
            generation,
            ..Snapshot::default()
        };
        c.history.generation = generation;
        c.history.status.active = 0;
        c.history.requested_period_ns = 0;
        // A new generation may bring a new clock domain
        c.last_timestamp = 0;
        c.epoch_period = 0;
        c.maxima = Maxima::default();
        c.intervals = Bins::default();
        c.work = Bins::default();
        c.refreshes = Bins::default();
        c.late_by = Bins::default();
    });
}

/// Release a registration on its producer thread.
pub fn destroy(id: u32) {
    COLLECTORS.with_borrow_mut(|r| {
        if let Some(slot) = r
            .slots
            .iter_mut()
            .find(|s| s.as_ref().is_some_and(|(key, _)| *key == id))
        {
            *slot = None;
        }
    });
}

/// C exports instantiated by the app's host macro.
#[macro_export]
macro_rules! frame_health_exports {
    () => {
        /// Create bounded UI-thread callback counters.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_create() -> u32 {
            $crate::frame_health::create()
        }
        /// Record timing from an existing app frame callback.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_record(
            id: u32,
            arrival: u64,
            target: u64,
            period: u64,
            finished: u64,
            outcome: u32,
            flags: u64,
        ) -> u32 {
            u32::from($crate::frame_health::record(
                id, arrival, target, period, finished, outcome, flags,
            ))
        }
        /// Record the finish of a pending frame's work.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_worked(
            id: u32,
            start: u64,
            deadline: u64,
            finished: u64,
        ) {
            $crate::frame_health::worked(id, start, deadline, finished);
        }
        /// Return and reset the per-read maxima; only the card's sampler calls this.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_take_maxima(id: u32) -> $crate::frame_health::Maxima {
            $crate::frame_health::take_maxima(id)
        }
        /// Control optional bounded detail capture.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_capture(id: u32, action: u32, now: u64) {
            $crate::frame_health::capture(id, action, now);
        }
        /// Read detail capture accounting.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_history_status(
            id: u32,
        ) -> $crate::frame_health::HistoryStatus {
            $crate::frame_health::history_status(id)
        }
        /// Copy one retained hitch, oldest first.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_hitch(
            id: u32,
            index: u32,
        ) -> $crate::frame_health::Hitch {
            $crate::frame_health::hitch(id, index)
        }
        /// Retain one completed apply and its optional owner timeline as hitch evidence.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_applied(
            id: u32,
            work: $crate::frame_health::Work,
            flags: u64,
        ) {
            $crate::frame_health::applied(id, work, flags);
        }
        /// Cache the active host rate request.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_requested_period(id: u32, period: u64) {
            $crate::frame_health::requested_period(id, period);
        }
        /// Forward context only; this does not count another apply or deadline event.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_context(id: u32, work: $crate::frame_health::Work) {
            $crate::frame_health::context(id, work);
        }
        /// Copy one fixed histogram on the producer thread.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_distribution(
            id: u32,
            kind: u32,
        ) -> $crate::frame_health::Distribution {
            $crate::frame_health::distribution(id, kind)
        }
        /// Copy scalar counters on demand.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_snapshot(id: u32) -> $crate::frame_health::Snapshot {
            $crate::frame_health::snapshot(id)
        }
        /// Stop the spacing baseline.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_stop(id: u32) {
            $crate::frame_health::stop(id);
        }
        /// Clear counters and advance generation.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_reset(id: u32) {
            $crate::frame_health::reset(id);
        }
        /// Release a collector.
        #[no_mangle]
        pub extern "C" fn exact_frame_health_destroy(id: u32) {
            $crate::frame_health::destroy(id);
        }
    };
}

#[cfg(test)]
#[path = "frame_health/tests.rs"]
mod tests;
