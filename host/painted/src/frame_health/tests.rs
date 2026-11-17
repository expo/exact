use super::android::{Lend, Reader, WINDOW_WORDS};
use serde_json::{json, Value};

const P: u64 = 8_333_333;
const T0: u64 = 1_000_000_000;

fn ask(r: &mut Reader, q: Value, now: u64) -> Value {
    serde_json::from_str(&r.answer(&q, now).0).unwrap()
}

fn health(r: &mut Reader, now: u64) -> Value {
    ask(
        r,
        json!({"op": "perf", "frames": true, "health": true}),
        now,
    )
}

/// Turn `n` at its vsync, publishing `n` (ingested `ingest` after the vsync).
fn turn(r: &mut Reader, n: u64, ingest: u64) {
    let vsync = T0 + n * P;
    r.turned(
        vsync + 100_000,
        vsync,
        vsync + P,
        P,
        vsync + 2_000_000,
        n + 1,
    );
    r.landed(n + 1, vsync + ingest - 500_000, vsync + ingest);
}

#[test]
fn a_turn_is_a_frame_whose_work_lands_with_its_stream() {
    let mut r = Reader::new(true);
    assert_eq!(r.display_period(16_666_667), Some(16_666_667.0 / 1e6));
    // Choreographer's vsync spacing replaces the display's: GPU canvases are told.
    let vsync = T0;
    assert_eq!(
        r.turned(vsync + 100_000, vsync, vsync + P, P, vsync + 2_000_000, 1),
        Some(P as f64 / 1e6)
    );
    r.landed(1, vsync + 3_000_000, vsync + 4_000_000);
    for n in 1..4 {
        turn(&mut r, n, 4_000_000);
    }
    let h = health(&mut r, T0 + 5 * P);
    assert_eq!(h["format"], "exact.frame-health.metrics/2");
    assert_eq!(h["source"], "exact-app-choreographer");
    assert_eq!(h["status"], "observing", "{h}");
    assert_eq!(h["frames"]["intervals"], "3");
    assert_eq!(h["work"]["records"], "4");
    assert_eq!(h["work"]["late"], "0");
    // Work runs from the vsync until the ingest ended: 4 ms, the [4, 4.5) ms bin
    assert_eq!(h["work"]["medianBoundsNs"], json!([4_000_000, 4_500_000]));
    assert_eq!(h["assessments"]["cadence"]["result"], "proxy-pass");
    assert_eq!(h["assessments"]["target"]["reason"], "no-rate-request");
    assert_eq!(h["scheduledPeriodNs"], P.to_string());
    // A stream ingested after the frame's deadline is late work
    turn(&mut r, 4, P + 1_000_000);
    let late = ask(
        &mut r,
        json!({"op": "perf", "frames": true, "health": true, "cursor": h["cursor"]}),
        T0 + 6 * P,
    );
    assert_eq!(late["window"]["kind"], "delta");
    assert_eq!(late["frames"]["intervals"], "1");
    assert_eq!(late["work"]["late"], "1");
    assert_eq!(late["assessments"]["cadence"]["result"], "fail");
    assert_eq!(late["assessments"]["cadence"]["reason"], "late-work");
}

#[test]
fn idle_between_runs_is_no_frame_time() {
    let mut r = Reader::new(true);
    turn(&mut r, 0, 3_000_000);
    turn(&mut r, 1, 3_000_000);
    r.idle();
    // A second later the next run starts: no interval spans the idle time.
    turn(&mut r, 120, 3_000_000);
    turn(&mut r, 121, 3_000_000);
    let h = health(&mut r, T0 + 122 * P);
    assert_eq!(h["frames"]["intervals"], "2");
    assert_eq!(
        h["frames"]["maximumBoundsNs"],
        json!([8_000_000, 8_500_000])
    );
    assert_eq!(
        h["assessments"]["cadence"]["predicates"]["skippedRefreshes"],
        "0"
    );
    // A skipped vsync within a run fails cadence
    turn(&mut r, 123, 3_000_000);
    let h = health(&mut r, T0 + 124 * P);
    assert_eq!(h["assessments"]["cadence"]["reason"], "skipped-refreshes");
}

#[test]
fn the_agents_clock_records_nothing_and_a_lease_lends_the_wall() {
    let mut r = Reader::new(true);
    r.clock(true, T0);
    turn(&mut r, 0, 3_000_000);
    turn(&mut r, 1, 3_000_000);
    let h = health(&mut r, T0 + 2 * P);
    assert_eq!(h["status"], "virtual");
    assert_eq!(h["available"], false);
    assert_eq!(h["frames"]["callbacks"], "0");
    assert_eq!(
        h["assessments"]["cadence"]["reason"], "virtual-clock",
        "no physical assessment under the agent's clock"
    );
    // A capture lease asks for the display's clock while it holds
    let (reply, lend) = r.answer(
        &json!({"op": "perf", "frames": true, "capture": "start", "durationMs": 1000}),
        T0,
    );
    assert_eq!(lend, Lend::Wall, "{reply}");
    r.clock(false, T0);
    assert!(r.app.capturing(), "capture starts once the clock is lent");
    turn(&mut r, 2, 3_000_000);
    turn(&mut r, 3, 3_000_000);
    assert_eq!(health(&mut r, T0 + 4 * P)["frames"]["intervals"], "1");
    let (reply, lend) = r.answer(
        &json!({"op": "perf", "frames": true, "capture": "stop"}),
        T0 + 5 * P,
    );
    assert_eq!(lend, Lend::Driver);
    let reply: Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["capture"], false);
    // A malformed lease is refused and lends nothing
    let (reply, lend) = r.answer(
        &json!({"op": "perf", "frames": true, "capture": "start", "durationMs": 120_001}),
        T0,
    );
    assert_eq!(lend, Lend::Keep);
    assert!(reply.contains("1–120000 ms"), "{reply}");
}

#[test]
fn window_frames_have_work_against_their_deadline_and_no_frame_times() {
    let mut r = Reader::new(true);
    let frame = |intended: u64, total: u64, deadline: u64| {
        [
            intended as i64,
            (intended + 200_000) as i64,
            total as i64,
            deadline as i64,
            0,
        ]
    };
    let words: Vec<i64> = [
        frame(T0, 6_000_000, 10_000_000),
        frame(T0 + 2 * P, 14_000_000, 10_000_000),
    ]
    .concat();
    assert_eq!(words.len(), 2 * WINDOW_WORDS);
    r.window_frames(&words, 3);
    let q = json!({"op": "perf", "frames": true, "health": true, "source": "window"});
    let h = ask(&mut r, q.clone(), T0 + 4 * P);
    assert_eq!(h["source"], "android-frame-metrics");
    assert_eq!(h["work"]["records"], "2");
    assert_eq!(h["work"]["late"], "1");
    assert_eq!(h["frames"]["intervals"], "0");
    assert_eq!(h["droppedReports"], "3");
    assert_eq!(h["assessments"]["cadence"]["reason"], "late-work");
    // Before Android 12 FrameMetrics has no deadline: lateness is unknown, not zero
    let mut old = Reader::new(true);
    old.window_frames(&frame(T0, 30_000_000, 0), 0);
    let h = ask(&mut old, q, T0 + 4 * P);
    assert_eq!(h["work"]["late"], Value::Null);
    assert_eq!(h["work"]["latenessUnknown"], "deadline-unavailable");
    assert_eq!(h["assessments"]["cadence"]["reason"], "schedule-unknown");
    // Native scrolling is the window's on Android
    let scroll = ask(
        &mut old,
        json!({"op": "perf", "frames": true, "health": true, "source": "scroll"}),
        T0,
    );
    assert_eq!(scroll["error"], "native-scroll-unobserved");
}

#[test]
fn a_landing_after_the_lease_ends_capture_before_it_records() {
    // A 10 ms lease; a landing inside it keeps a late-work and a late-apply event
    for (lands, kept) in [(T0 + 11_000_000, 0), (T0 + 9_000_000, 2)] {
        let mut r = Reader::new(true);
        r.answer(
            &json!({"op": "perf", "frames": true, "capture": "start", "durationMs": 10}),
            T0,
        );
        r.turned(T0 + 100_000, T0, T0 + 1_000_000, P, T0 + 500_000, 1);
        r.landed(1, T0 + 8_000_000, lands);
        assert_eq!(r.app.retained(), kept, "landing at {lands}");
        assert_eq!(r.app.capturing(), kept != 0, "landing at {lands}");
        let h = health(&mut r, lands);
        assert_eq!(h["work"]["late"], "1", "expiry keeps the core count");
    }
}

#[test]
fn a_report_keeps_a_captured_hitch_within_its_bytes_once_per_fifteen_seconds() {
    let mut r = Reader::new(true);
    r.answer(
        &json!({"op": "perf", "frames": true, "capture": "start", "durationMs": 60_000}),
        T0,
    );
    turn(&mut r, 0, 3_000_000);
    // A turn whose callback ran past its deadline is a hitch
    let vsync = T0 + P;
    r.turned(
        vsync + 100_000,
        vsync,
        vsync + P,
        P,
        vsync + P + 3_000_000,
        2,
    );
    r.landed(2, vsync + P + 3_100_000, vsync + P + 3_500_000);
    turn(&mut r, 2, 3_000_000);
    let report = r
        .answer(
            &json!({"op": "perf", "frames": true, "report": true}),
            T0 + 4 * P,
        )
        .0;
    assert!(report.len() < 4000, "{}", report.len());
    let report: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report["format"], "exact.frame-health/2");
    assert_eq!(report["source"], "exact-app-choreographer");
    assert_eq!(report["scope"]["armed"], true);
    let rows = report["hitches"].as_array().unwrap();
    assert!(!rows.is_empty(), "{report}");
    assert_eq!(
        report["columns"].as_array().unwrap().len(),
        rows[0].as_array().unwrap().len()
    );
    let columns = report["columns"].as_array().unwrap();
    let column = |name: &str| columns.iter().position(|v| v == name).unwrap();
    let work = rows
        .iter()
        .find(|row| row[column("flags")].as_u64().unwrap() & 128 != 0)
        .expect("late pending work is exported separately from the callback overrun");
    assert_eq!(work[column("frame_start_offset_us")], -11_833);
    assert_eq!(work[column("target_offset_us")], -3500);
    assert_eq!(work[column("apply_began_offset_us")], Value::Null);
    assert_eq!(report["otherSources"][0]["source"], "android-frame-metrics");
    let again: Value = serde_json::from_str(
        &r.answer(
            &json!({"op": "perf", "frames": true, "report": true}),
            T0 + 5 * P,
        )
        .0,
    )
    .unwrap();
    assert_eq!(again["error"], "report-rate-limit");
}

#[test]
fn a_cursor_from_another_generation_asks_for_a_baseline() {
    let mut r = Reader::new(true);
    turn(&mut r, 0, 3_000_000);
    let h = health(&mut r, T0 + P);
    r.clock(true, T0 + P);
    r.clock(false, T0 + P);
    let again = ask(
        &mut r,
        json!({"op": "perf", "frames": true, "health": true, "cursor": h["cursor"]}),
        T0 + 2 * P,
    );
    assert_eq!(again["error"], "baseline-required");
}

#[test]
fn a_period_told_before_the_host_runs_is_given_at_the_first_turn_without_collection() {
    // A production launch collects nothing and still gives GPU canvases the period.
    let mut r = Reader::new(false);
    r.remember_display_period(16_666_667);
    assert_eq!(
        r.turned(T0 + 1, T0, 0, 0, T0 + 2, 0),
        Some(16_666_667.0 / 1e6)
    );
    assert_eq!(r.turned(T0 + P + 1, T0 + P, 0, 0, T0 + P + 2, 0), None);
    assert_eq!(
        health(&mut r, T0 + 2 * P)["unavailableReason"],
        "collection-disabled"
    );
}

#[test]
fn work_past_eight_unlanded_streams_is_lost_counted_and_never_passes() {
    let mut r = Reader::new(true);
    turn(&mut r, 0, 3_000_000);
    let h = health(&mut r, T0 + P);
    // The main thread stalls behind ten streams, then ingests them all at once
    for n in 1..11 {
        let vsync = T0 + n * P;
        r.turned(
            vsync + 100_000,
            vsync,
            vsync + P,
            P,
            vsync + 2_000_000,
            n + 1,
        );
    }
    let end = T0 + 11 * P;
    r.landed(11, end - 500_000, end);
    let after = ask(
        &mut r,
        json!({"op": "perf", "frames": true, "health": true, "cursor": h["cursor"]}),
        end,
    );
    assert_eq!(after["work"]["records"], "8", "{after}");
    assert_eq!(after["work"]["lost"], "2");
    assert_eq!(
        after["assessments"]["cadence"]["predicates"]["lostWork"],
        "2"
    );
    // Every surviving frame landed past its deadline: an observed failure stands
    assert_eq!(after["assessments"]["cadence"]["reason"], "late-work");
    // A window in which nothing late was observed but work was lost does not pass
    let mut quiet = Reader::new(true);
    for n in 0..10 {
        let vsync = T0 + n * P;
        quiet.turned(
            vsync + 100_000,
            vsync,
            vsync + 20 * P,
            P,
            vsync + 2_000_000,
            n + 1,
        );
    }
    quiet.landed(10, T0 + 10 * P, T0 + 10 * P + 100_000);
    let h = health(&mut quiet, T0 + 11 * P);
    assert_eq!(h["work"]["lost"], "2");
    assert_eq!(
        h["assessments"]["cadence"]["result"],
        "insufficient-evidence"
    );
    assert_eq!(h["assessments"]["cadence"]["reason"], "work-lost");
}
