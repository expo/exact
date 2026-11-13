use super::*;

const P: u64 = 8_333_333;

/// A callback for the frame starting at `frame`, arriving `late` after it.
fn callback(c: &mut Collector, frame: u64, late: u64, period: u64, outcome: u32) {
    c.record(
        frame + late,
        frame + period,
        period,
        frame + late + 500_000,
        outcome,
        0,
    );
}

#[test]
fn frame_intervals_exclude_idle_held_callbacks_and_nothing_else() {
    let mut c = Collector::default();
    callback(&mut c, 1_000_000_000, 0, P, COMPLETE);
    // Arrival jitter does not move a frame's timestamp
    callback(&mut c, 1_000_000_000 + P, 6_000_000, P, COMPLETE);
    // A held callback is not a frame: the next interval spans it
    callback(&mut c, 1_000_000_000 + 2 * P, 0, P, HELD);
    callback(&mut c, 1_000_000_000 + 3 * P, 0, P, COMPLETE);
    // A callback with no frame work due is not a frame either
    callback(&mut c, 1_000_000_000 + 4 * P, 0, P, IDLE);
    c.stop();
    callback(&mut c, 9_000_000_000, 0, P, COMPLETE);
    // A schedule change keeps the interval: its time is well defined
    callback(&mut c, 9_000_000_000 + 2 * P, 0, 2 * P, COMPLETE);
    let s = c.snapshot;
    assert_eq!((s.callbacks, s.held, s.worked), (7, 1, 5));
    assert_eq!(s.intervals, 3);
    assert_eq!(s.active_ns, 5 * P);
    assert_eq!(s.max_interval_ns, 2 * P);
    assert_eq!(
        s.cadence_changes, 1,
        "the change across stopped time is not counted"
    );
    assert_eq!(c.intervals.0[16] + c.intervals.0[33], 3);
    assert_eq!(
        c.refreshes.0[..2],
        [1, 1],
        "the interval across the change counts no refreshes"
    );
}

#[test]
fn missed_frames_are_requested_slots_without_a_frame() {
    let mut c = Collector::default();
    let base = 1_000_000_000;
    callback(&mut c, base, 0, P, COMPLETE);
    callback(&mut c, base + P, 0, P, COMPLETE);
    assert_eq!(c.snapshot.requested_slots, 0, "no rate requested");
    c.history.requested_period_ns = P;
    // A skipped frame, then a held callback, then two skipped
    for (n, outcome) in [
        (2, COMPLETE),
        (4, COMPLETE),
        (5, HELD),
        (6, COMPLETE),
        (9, COMPLETE),
    ] {
        callback(&mut c, base + n * P, 0, P, outcome);
    }
    assert_eq!(
        (c.snapshot.requested_slots, c.snapshot.missed_frames),
        (8, 4)
    );
    assert_eq!(c.snapshot.restricted_intervals, 0);
    // An 80 Hz schedule under the 120 FPS request misses 40 a second, though every frame is
    // even at its own rate; nearest slots keep timestamp jitter from adding any
    let p80 = 12_500_000;
    let start = base + 9 * P;
    for n in 1..=80 {
        let jitter = if n % 2 == 0 { 30_000 } else { 0 };
        callback(&mut c, start + n * p80 - jitter, 0, p80, COMPLETE);
    }
    assert_eq!(c.snapshot.missed_frames - 4, 40);
    assert_eq!(
        c.snapshot.restricted_intervals, 80,
        "each interval ends on the slower schedule"
    );
    assert_eq!(
        c.refreshes.0[0],
        2 + 79,
        "each 80 Hz frame held one of its own refreshes"
    );
}

#[test]
fn each_end_of_an_interval_meets_its_own_request() {
    let mut c = Collector::default();
    let base = 1_000_000_000;
    // A 60 Hz frame under a 60 FPS request, then a 120 Hz frame under a 120 FPS request
    c.history.requested_period_ns = 2 * P;
    callback(&mut c, base, 0, 2 * P, COMPLETE);
    c.history.requested_period_ns = P;
    callback(&mut c, base + 2 * P, 0, P, COMPLETE);
    assert_eq!(
        c.snapshot.restricted_intervals, 0,
        "each schedule met its own request"
    );
    // A 120 Hz frame, then a 60 Hz one under the same 120 FPS request: its end is restricted
    callback(&mut c, base + 4 * P, 0, 2 * P, COMPLETE);
    assert_eq!(c.snapshot.restricted_intervals, 1);
}

#[test]
fn a_new_request_or_a_stop_restarts_the_grid() {
    let id = create();
    let base = 1_000_000_000;
    requested_period(id, 2 * P);
    let call = |n: u64, period: u64| {
        record(
            id,
            base + n,
            base + n + period,
            period,
            base + n + 1,
            COMPLETE,
            0,
        )
    };
    call(0, 2 * P);
    call(2 * P, 2 * P);
    // The request rises to 120 FPS; the interval across the change counts nothing
    requested_period(id, P);
    call(5 * P, P);
    assert_eq!(
        (snapshot(id).requested_slots, snapshot(id).missed_frames),
        (3, 0)
    );
    call(6 * P, P);
    call(8 * P, P);
    assert_eq!(
        (snapshot(id).requested_slots, snapshot(id).missed_frames),
        (6, 1)
    );
    // A repeated request changes nothing; stopped time is not missed
    requested_period(id, P);
    call(9 * P, P);
    stop(id);
    call(1_000 * P, P);
    call(1_001 * P, P);
    assert_eq!(
        (snapshot(id).requested_slots, snapshot(id).missed_frames),
        (9, 1)
    );
    // Timestamps that go backwards count nothing and never panic
    call(10 * P, P);
    call(11 * P, P);
    assert_eq!(snapshot(id).missed_frames, 1);
    destroy(id);
}

#[test]
fn held_callbacks_miss_their_slots_even_when_the_run_ends() {
    let mut c = Collector::default();
    c.history.requested_period_ns = P;
    let base = 1_000_000_000;
    // A frame whose work stays in flight, then two held callbacks, then the run ends
    callback(&mut c, base, 0, P, PENDING);
    callback(&mut c, base + P, 0, P, HELD);
    callback(&mut c, base + 2 * P, 0, P, HELD);
    assert_eq!(
        (c.snapshot.requested_slots, c.snapshot.missed_frames),
        (2, 1),
        "the newest held slot is open"
    );
    c.stop();
    assert_eq!(
        (c.snapshot.requested_slots, c.snapshot.missed_frames),
        (3, 2)
    );
    assert_eq!(c.snapshot.held, 2);
}

#[test]
fn a_slot_counts_once_when_callbacks_outpace_the_request() {
    let mut c = Collector::default();
    // 60 FPS requested, 120 Hz callbacks: frames share slots, and none is missed
    c.history.requested_period_ns = 2 * P;
    for n in 0..5 {
        callback(&mut c, 1_000_000_000 + n * P, 0, P, COMPLETE);
    }
    assert_eq!(
        (c.snapshot.requested_slots, c.snapshot.missed_frames),
        (3, 0)
    );
    // A held callback in a slot a later frame fills misses nothing
    callback(&mut c, 1_000_000_000 + 5 * P, 0, P, HELD);
    callback(&mut c, 1_000_000_000 + 6 * P, 0, P, COMPLETE);
    c.stop();
    assert_eq!(
        (c.snapshot.requested_slots, c.snapshot.missed_frames),
        (4, 0)
    );
}

#[test]
fn an_idle_spell_ends_the_run_and_a_held_callback_can_change_the_schedule() {
    let mut c = Collector::default();
    callback(&mut c, 1_000_000_000, 0, P, COMPLETE);
    // A timer keeps the link running with no frame work; the next frame starts a new run
    callback(&mut c, 1_000_000_000 + P, 0, P, IDLE);
    callback(&mut c, 2_000_000_000, 0, P, COMPLETE);
    assert_eq!(c.snapshot.intervals, 0, "idle time is never a frame time");
    assert_eq!(c.snapshot.running, 1);
    // A held callback on a 60 Hz schedule between two 120 Hz frames: the interval has no single period
    callback(&mut c, 2_000_000_000 + P, 0, 2 * P, HELD);
    callback(&mut c, 2_000_000_000 + 3 * P, 0, P, COMPLETE);
    assert_eq!(c.snapshot.intervals, 1);
    assert_eq!(c.refreshes.0.iter().sum::<u32>(), 0);
    assert_eq!(c.snapshot.cadence_changes, 2);
    // A schedule change across stopped time interrupts no interval but still separates schedules
    c.stop();
    callback(&mut c, 9_000_000_000, 0, 2 * P, COMPLETE);
    assert_eq!(c.snapshot.cadence_changes, 2);
    assert_eq!(c.snapshot.schedule_epochs, 3);
}

#[test]
fn backward_time_ends_the_run_and_a_new_request_keeps_old_misses() {
    let mut c = Collector::default();
    // Timestamps 1.000, 1.008, 0.500, 1.016 s: no interval spans the step back
    for at in [1_000_000_000, 1_008_000_000, 500_000_000, 1_016_000_000] {
        c.record(at, at + 8_000_000, 8_000_000, at + 1_000_000, COMPLETE, 0);
    }
    assert_eq!(c.snapshot.out_of_order, 1);
    assert_eq!(c.snapshot.intervals, 1);
    assert_eq!(
        c.snapshot.max_interval_ns, 8_000_000,
        "no interval spans the step back"
    );
    let id = create();
    requested_period(id, P);
    let call = |n: u64, outcome| {
        record(
            id,
            2_000_000_000 + n * P,
            2_000_000_000 + (n + 1) * P,
            P,
            2_000_000_000 + n * P + 1,
            outcome,
            0,
        )
    };
    call(0, COMPLETE);
    call(1, HELD);
    // The request changes before a frame starts in the held slot: that slot was missed
    requested_period(id, 2 * P);
    assert_eq!(
        (snapshot(id).requested_slots, snapshot(id).missed_frames),
        (2, 1)
    );
    destroy(id);
}

#[test]
fn an_interval_across_a_schedule_change_is_counted_unclassified() {
    let mut c = Collector::default();
    callback(&mut c, 1_000_000_000, 0, P, COMPLETE);
    callback(&mut c, 1_000_000_000 + P, 0, 2 * P, HELD);
    callback(&mut c, 1_000_000_000 + 3 * P, 0, P, COMPLETE);
    assert_eq!(
        (c.snapshot.intervals, c.snapshot.unclassified_intervals),
        (1, 1)
    );
}

#[test]
fn work_begun_past_its_deadline_is_late() {
    let mut c = Collector::default();
    c.worked(2_000_000, 1_000_000, 3_000_000);
    assert_eq!((c.snapshot.worked, c.snapshot.late), (1, 1));
    assert_eq!((c.maxima.slowest_ns, c.maxima.budget_ns), (1_000_000, 0));
}

#[test]
fn extreme_observations_saturate_and_never_panic() {
    let mut c = Collector::default();
    c.history.requested_period_ns = u64::MAX - 1;
    for at in [1, u64::MAX / 2, u64::MAX - 2] {
        c.record(at, at.saturating_add(1), 1, at, COMPLETE, 0);
        c.record(at, at, at, at, HELD, 0);
    }
    c.worked(1, u64::MAX, u64::MAX);
    c.stop();
    assert!(c.snapshot.callbacks >= 5);
}

#[test]
fn pacing_counts_refreshes_and_their_changes() {
    let mut c = Collector::default();
    let mut at = 1_000_000_000;
    // One, two, one, one, three refreshes: three changes
    for refreshes in [0, 1, 2, 1, 1, 3] {
        at += refreshes * P;
        callback(&mut c, at, 0, P, COMPLETE);
    }
    assert_eq!(c.refreshes.0[..3], [3, 1, 1]);
    assert_eq!(c.snapshot.pacing_changes, 3);
    // An interval over 31 refreshes falls in the open last bin
    callback(&mut c, at + 40 * P, 0, P, COMPLETE);
    assert_eq!(c.refreshes.0[31], 1);
    assert_eq!(
        c.snapshot.skipped_refreshes,
        1 + 2 + 39,
        "exact past the open bin"
    );
    // Intervals of 40 and 50 refreshes share the open bin but are a change
    callback(&mut c, at + 90 * P, 0, P, COMPLETE);
    assert_eq!(c.snapshot.pacing_changes, 5);
}

#[test]
fn work_is_recorded_once_per_frame_against_its_deadline() {
    let id = create();
    let frame = 1_000_000_000;
    // A complete frame finishes when its callback returns
    record(
        id,
        frame + 100_000,
        frame + P,
        P,
        frame + 4_100_000,
        COMPLETE,
        0,
    );
    // A pending frame finishes when its owner work lands, here late by 1.7 ms
    record(
        id,
        frame + P + 100_000,
        frame + 2 * P,
        P,
        frame + P + 600_000,
        PENDING,
        0,
    );
    assert_eq!(snapshot(id).worked, 1);
    worked(id, frame + P, frame + 2 * P, frame + P + 10_000_000);
    // A held callback adds no work record
    record(
        id,
        frame + 2 * P + 100_000,
        frame + 3 * P,
        P,
        frame + 2 * P + 200_000,
        HELD,
        0,
    );
    let s = snapshot(id);
    assert_eq!((s.worked, s.late), (2, 1));
    assert_eq!(s.work_through_ns, frame + P + 10_000_000);
    let work = distribution(id, 1).bins;
    assert_eq!((work[8], work[20]), (1, 1));
    assert_eq!(distribution(id, 3).bins[3], 1, "late by 1.67 ms");
    // Applies are hitch evidence only
    applied(
        id,
        Work {
            began_ns: frame,
            finished_ns: frame + 50_000_000,
            target_ns: frame + P,
            ..Work::default()
        },
        0,
    );
    assert_eq!(snapshot(id).worked, 2);
    destroy(id);
}

#[test]
fn late_pending_work_has_history_without_an_apply() {
    let id = create();
    let start = 1_000_000_000;
    capture(id, 1, start);
    record(id, start, start + P, P, start + 1_000_000, PENDING, 0);
    worked(id, start, start + P, start + 10_000_000);
    assert_eq!((snapshot(id).worked, snapshot(id).late), (1, 1));
    let status = history_status(id);
    assert_eq!(
        (status.total, status.retained, status.apply_deadlines),
        (1, 1, 0)
    );
    let event = hitch(id, 0);
    assert_eq!(event.at_ns, start + 10_000_000);
    assert_eq!(event.target_us, -1_667);
    assert_eq!(event.frame_start_us, -10_000);
    assert_eq!(event.flags, history::WORK_DEADLINE);
    worked(id, start + P, start + 2 * P, start + 2 * P);
    assert_eq!(history_status(id).total, 1, "on-time work adds no event");
    assert_eq!((snapshot(id).worked, snapshot(id).late), (2, 1));
    destroy(id);
}

#[test]
fn pending_work_evidence_is_separate_from_callback_and_apply_events() {
    let id = create();
    let start = 1_000_000_000;
    capture(id, 1, start);
    record(id, start, start + P, P, start + 10_000_000, COMPLETE, 0);
    assert_eq!(
        history_status(id).total,
        1,
        "synchronous work has its callback event"
    );
    assert_eq!(hitch(id, 0).flags, history::CALLBACK_DEADLINE);
    assert_eq!(hitch(id, 0).frame_start_us, history::ABSENT);
    applied(
        id,
        Work {
            capture: history_status(id).capture,
            began_ns: start + 12_000_000,
            finished_ns: start + 20_000_000,
            target_ns: start + 16_000_000,
            ..Work::default()
        },
        0,
    );
    worked(
        id,
        start + 8_000_000,
        start + 16_000_000,
        start + 20_000_000,
    );
    assert_eq!((snapshot(id).worked, snapshot(id).late), (2, 2));
    assert_eq!(
        (history_status(id).total, history_status(id).apply_deadlines),
        (3, 1)
    );
    assert_eq!(hitch(id, 2).flags, history::WORK_DEADLINE);
    capture(id, 0, start + 21_000_000);
    worked(
        id,
        start + 24_000_000,
        start + 32_000_000,
        start + 40_000_000,
    );
    capture(id, 1, start + 41_000_000);
    worked(
        id,
        start + 40_000_000,
        start + 48_000_000,
        start + 56_000_000,
    );
    assert_eq!(
        history_status(id).total,
        3,
        "paused and previous-segment work adds no detail"
    );
    for i in 0..20 {
        let at = start + 64_000_000 + i * 16_000_000;
        worked(id, at, at + 8_000_000, at + 10_000_000);
    }
    let status = history_status(id);
    assert_eq!(
        (status.total, status.retained, status.overwritten),
        (23, 16, 7)
    );
    assert_eq!(hitch(id, 0).sequence, 8);
    assert_eq!(hitch(id, 15).sequence, 23);
    destroy(id);
}

#[test]
fn maxima_are_exact_and_taken_only_by_their_reader() {
    let id = create();
    let frame = 1_000_000_000;
    record(id, frame, frame + P, P, frame + 3_000_000, COMPLETE, 0);
    record(
        id,
        frame + 3 * P,
        frame + 4 * P,
        P,
        frame + 3 * P + 9_000_000,
        COMPLETE,
        0,
    );
    // Snapshot reads leave them for the card's sampler
    let _ = snapshot(id);
    let maxima = take_maxima(id);
    assert_eq!(maxima.longest_ns, 3 * P);
    assert_eq!((maxima.slowest_ns, maxima.budget_ns), (9_000_000, P));
    assert_eq!(take_maxima(id).longest_ns, 0, "taking resets them");
    record(
        id,
        frame + 4 * P,
        frame + 5 * P,
        P,
        frame + 4 * P + 1_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(take_maxima(id).longest_ns, P);
    destroy(id);
}

#[test]
fn capacity_lifetime_and_generation_are_bounded() {
    let ids: Vec<_> = (0..16).map(|_| create()).collect();
    assert!(ids.iter().all(|id| *id != 0));
    assert_eq!(create(), 0);
    record(ids[0], 100, 200, 100, 150, COMPLETE, 0);
    reset(ids[0]);
    assert_eq!(snapshot(ids[0]).callbacks, 0);
    assert_eq!(snapshot(ids[0]).generation, 1);
    destroy(ids[0]);
    let replacement = create();
    assert_ne!(replacement, ids[0]);
    assert_eq!(snapshot(ids[0]).generation, 0);
    destroy(replacement);
    for id in ids.into_iter().skip(1) {
        destroy(id);
    }
    assert_eq!(std::mem::size_of::<history::Hitch>(), 112);
    assert_eq!(std::mem::size_of::<Collector>(), 3168);
}

#[test]
fn distributions_have_explicit_finalization_reset_and_saturation() {
    let id = create();
    record(
        id,
        1_000_000_000,
        1_000_000_000 + P,
        P,
        1_001_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(
        snapshot(id).interval_through_ns,
        0,
        "a baseline finalizes no interval"
    );
    record(
        id,
        1_000_000_000 + P,
        1_000_000_000 + 2 * P,
        P,
        1_009_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(distribution(id, 0).bins[16], 1);
    assert_eq!(snapshot(id).interval_through_ns, 1_000_000_000 + P);
    stop(id);
    record(
        id,
        9_000_000_000,
        9_000_000_000 + P,
        P,
        9_001_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(
        snapshot(id).intervals,
        1,
        "idle time contributes no interval"
    );
    reset(id);
    assert_eq!(distribution(id, 0).bins.iter().sum::<u32>(), 0);
    assert_eq!(distribution(id, 1).bins.iter().sum::<u32>(), 0);
    // Passive updates with unknown cadence have work but no intervals
    record(
        id,
        10_000_000_000,
        10_008_000_000,
        0,
        10_001_000_000,
        COMPLETE,
        0,
    );
    record(
        id,
        10_030_000_000,
        10_038_000_000,
        0,
        10_039_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(snapshot(id).intervals, 0);
    assert_eq!((snapshot(id).worked, snapshot(id).late), (2, 1));
    let mut bins = Bins::<64>::default();
    bins.0[0] = u32::MAX;
    assert!(!bins.duration(0, 24_000_000));
    assert_eq!(bins.0[0], u32::MAX);
    assert!(bins.duration(u64::MAX, 24_000_000));
    assert_eq!(bins.0[63], 1);
    let mut late = Bins::<32>::default();
    for duration in [
        11_999_999,
        12_000_000,
        1_535_999_999,
        1_536_000_000,
        u64::MAX,
    ] {
        late.duration(duration, 12_000_000);
    }
    assert_eq!(
        (late.0[23], late.0[24], late.0[30], late.0[31]),
        (1, 1, 1, 2)
    );
    destroy(id);
}

#[test]
fn forwarded_apply_context_does_not_add_a_second_apply_or_deadline() {
    let id = create();
    capture(id, 1, 1_000_000_000);
    let work = Work {
        capture: history_status(id).capture,
        began_ns: 1_001_000_000,
        finished_ns: 1_003_000_000,
        target_ns: 1_002_000_000,
        operations: 10,
        ..Work::default()
    };
    context(id, work);
    assert_eq!(history_status(id).total, 0);
    record(
        id,
        1_004_000_000,
        1_005_000_000,
        0,
        1_006_000_000,
        COMPLETE,
        0,
    );
    let event = hitch(id, 0);
    assert_eq!(event.flags & history::APPLY_DEADLINE, 0);
    assert_ne!(event.flags & history::CALLBACK_DEADLINE, 0);
    assert_ne!(event.flags & history::WORK_KNOWN, 0);
    assert_eq!(event.work.operations, 10);
    assert_eq!(event.work.finished_us, -1_000);
    assert_eq!(event.flags & history::CLAMPED, 0);
    // A boundary over 35 minutes from its event does not fit, and the hitch says so
    context(
        id,
        Work {
            began_ns: 1,
            finished_ns: 2,
            ..work
        },
    );
    record(
        id,
        3_000_000_000_000,
        3_000_001_000_000,
        0,
        3_000_002_000_000,
        COMPLETE,
        0,
    );
    assert_ne!(hitch(id, 1).flags & history::CLAMPED, 0);
    assert_eq!(snapshot(id).intervals, 0);
    assert_eq!(snapshot(id).max_interval_ns, 0);
    destroy(id);
}

#[test]
#[ignore = "manual optimized microbenchmark; whole Apple hooks still need device qualification"]
fn recorder_cost() {
    use std::hint::black_box;
    use std::time::Instant;

    let ids: Vec<_> = (0..16).map(|_| create()).collect();
    for (id, detail, storm) in [
        (ids[0], false, false),
        (ids[15], false, false),
        (ids[0], true, false),
        (ids[15], true, false),
        (ids[0], true, true),
        (ids[15], true, true),
    ] {
        if detail {
            capture(id, 1, 1);
        }
        requested_period(id, 8_333_333);
        let before = snapshot(id).callbacks;
        let mut samples = Vec::new();
        for batch in 0..31_u64 {
            let began = Instant::now();
            for i in 0..100_000 {
                let arrival = black_box(
                    1_000_000_000
                        + (batch * 100_000 + i) * if storm { 24_999_999 } else { 8_333_333 },
                );
                record(
                    black_box(id),
                    arrival,
                    arrival + 8_333_333,
                    8_333_333,
                    arrival + 1_000,
                    COMPLETE,
                    0,
                );
            }
            samples.push(began.elapsed().as_nanos() as f64 / 100_000.0);
        }
        samples.sort_by(f64::total_cmp);
        println!("slot {}, detail {detail}, hitch storm {storm}: median {:.2} ns/record, batch maximum {:.2} ns/record; collector {} B, registry {} B", if id == ids[0] { 1 } else { 16 }, samples[15], samples[30], std::mem::size_of::<Collector>(), std::mem::size_of::<Registry>());
        assert_eq!(snapshot(id).callbacks - before, 3_100_000);
    }
    for id in ids {
        destroy(id);
    }
}

#[test]
fn a_callback_does_not_follow_the_apply_it_ran() {
    let id = create();
    let start = 1_000_000_000;
    capture(id, 1, start);
    let incarnation = history_status(id).capture;
    let arrival = start + 16_000_000;
    // The apply finishes past its target within the callback, which records once it returns
    let work = Work {
        capture: incarnation,
        began_ns: arrival + 1_000_000,
        finished_ns: arrival + 5_000_000,
        target_ns: arrival + 2_000_000,
        ..Work::default()
    };
    applied(id, work, 0);
    record(
        id,
        arrival,
        arrival + 8_000_000,
        8_000_000,
        arrival + 6_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(hitch(id, 0).flags & history::AFTER_KNOWN, 0);
    let next = arrival + 16_000_000;
    record(
        id,
        next,
        next + 8_000_000,
        8_000_000,
        next + 1_000_000,
        COMPLETE,
        0,
    );
    let event = hitch(id, 0);
    assert_ne!(event.flags & history::AFTER_KNOWN, 0);
    assert_eq!(event.after_at_us, 11_000);
    destroy(id);
}

#[test]
fn recent_hitches_have_bounded_storage_loss_and_adjacent_work_evidence() {
    let id = create();
    let start = 1_000_000_000;
    capture(id, 1, start);
    let incarnation = history_status(id).capture;
    for i in 0..40 {
        let at = start + i * 24_000_000;
        record(
            id,
            at,
            at + 8_000_000,
            8_000_000,
            at + 1_000_000,
            COMPLETE,
            1 << 16,
        );
    }
    let status = history_status(id);
    assert_eq!(status.total, 39);
    assert_eq!(status.retained, HITCH_CAPACITY as u64);
    assert_eq!(status.overwritten, 23);
    assert_eq!(hitch(id, 0).sequence, 24);
    let event = hitch(id, 14);
    assert_eq!(event.gap_us, 24_000);
    assert_eq!(event.before_callback_us, 1_000);
    assert_eq!(event.after_gap_us, 24_000);
    assert_ne!(event.flags & history::AFTER_KNOWN, 0);
    let work = Work {
        capture: incarnation,
        began_ns: start + 960_000_000,
        finished_ns: start + 970_000_000,
        target_ns: start + 965_000_000,
        operations: 45,
        ..Work::default()
    };
    applied(id, work, 0);
    let last = hitch(id, 15);
    assert_ne!(last.flags & history::APPLY_DEADLINE, 0);
    assert_eq!(last.work.operations, 45);
    capture(id, 0, start + 1_000_000_000);
    record(
        id,
        start + 4_000_000_000,
        start + 4_008_000_000,
        8_000_000,
        start + 4_001_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(
        history_status(id).total,
        40,
        "paused capture records no new detail"
    );
    capture(id, 1, start + 5_000_000_000);
    applied(id, work, 0);
    assert_eq!(
        history_status(id).total,
        40,
        "old capture work cannot join the new segment"
    );
    record(
        id,
        start + 5_000_000_000,
        start + 5_008_000_000,
        8_000_000,
        start + 5_001_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(
        history_status(id).total,
        40,
        "resuming capture does not count paused time as a gap"
    );
    let old_event = hitch(id, 0);
    reset(id);
    assert_eq!(
        history_status(id).retained,
        16,
        "reload keeps recent evidence"
    );
    assert_eq!(hitch(id, 0).generation, old_event.generation);
    capture(id, 1, start + 6_000_000_000);
    let next = history_status(id).capture;
    assert!(next > incarnation);
    applied(
        id,
        Work {
            capture: next,
            ..work
        },
        0,
    );
    assert_eq!(hitch(id, 15).generation, 1);
    capture(id, 2, start + 6_000_000_000);
    assert_eq!(history_status(id).retained, 0);
    assert_eq!(history_status(id).active, 1);
    assert!(std::mem::size_of::<Collector>() <= 4096);
    assert!(std::mem::size_of::<Registry>() <= 256 * 1024);
    destroy(id);
}

#[test]
fn a_short_followup_does_not_hide_the_long_apply_before_a_callback_gap() {
    let id = create();
    capture(id, 1, 1_000_000_000);
    let current = history_status(id).capture;
    record(
        id,
        1_000_000_000,
        1_008_000_000,
        8_000_000,
        1_001_000_000,
        COMPLETE,
        0,
    );
    applied(
        id,
        Work {
            capture: current,
            began_ns: 1_001_000_000,
            finished_ns: 1_041_000_000,
            operations: 100,
            ..Work::default()
        },
        0,
    );
    applied(
        id,
        Work {
            capture: current,
            began_ns: 1_041_000_000,
            finished_ns: 1_042_000_000,
            operations: 1,
            ..Work::default()
        },
        0,
    );
    record(
        id,
        1_048_000_000,
        1_056_000_000,
        8_000_000,
        1_049_000_000,
        COMPLETE,
        0,
    );
    let event = hitch(id, 0);
    assert_ne!(event.flags & history::INTERVAL_MAXIMUM, 0);
    assert_eq!(event.work.operations, 100);
    assert_eq!(event.work.finished_us - event.work.began_us, 40_000);
    record(
        id,
        1_072_000_000,
        1_080_000_000,
        8_000_000,
        1_073_000_000,
        COMPLETE,
        0,
    );
    assert_eq!(
        hitch(id, 1).flags & history::WORK_KNOWN,
        0,
        "stale apply evidence is unavailable"
    );
    destroy(id);
}

#[test]
fn a_frame_due_before_its_next_vsync_is_late_by_its_own_deadline() {
    // Android's Choreographer: a frame at its vsync whose work is due 6 ms
    // later, not a period later, and one whose deadline is a period away
    let mut c = Collector::default();
    let frame = 1_000_000_000;
    assert!(c.record_at(
        frame + 100_000,
        frame,
        frame + 6_000_000,
        P,
        frame + 7_000_000,
        COMPLETE,
        0
    ));
    assert_eq!((c.snapshot.worked, c.snapshot.late), (1, 1));
    assert!(c.record_at(
        frame + P + 100_000,
        frame + P,
        frame + 2 * P,
        P,
        frame + P + 7_000_000,
        COMPLETE,
        0
    ));
    assert_eq!((c.snapshot.worked, c.snapshot.late), (2, 1));
    // The interval is between the vsyncs, whatever the deadlines
    assert_eq!((c.snapshot.intervals, c.snapshot.active_ns), (1, P));
    assert_eq!(c.snapshot.skipped_refreshes, 0);
}
