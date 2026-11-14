use super::*;
use crate::{Clock, Game, Input, Sim, Transform, World};
thread_local! { static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.) }; }
fn now() -> f64 {
    TIME.with(|t| t.get())
}
fn add(ms: f64) {
    TIME.with(|t| t.set(t.get() + ms));
}
struct Nested;
impl Game for Nested {
    const ID: &'static str = "spans";
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.spawn(Transform::default());
    }
    fn tick(w: &mut World, _: &Input, _: &()) {
        add(1.);
        let outer = w.span("outer");
        add(2.);
        {
            let _inner = w.span("inner");
            add(3.);
        }
        add(2.);
        drop(outer);
        {
            let _again = w.span("inner");
            add(1.);
        }
        add(1.);
    }
}
#[allow(non_snake_case)]
#[derive(Default, crate::Data)]
struct Sample {
    name: String,
    at: String,
    p50: f64,
    p95: f64,
    perTick: f64,
    count: u64,
}
#[allow(non_snake_case)]
#[derive(Default, crate::Data)]
struct Report {
    incarnation: String,
    generation: u64,
    spanTicks: u64,
    tickMs: Distribution,
    spans: Vec<Sample>,
}
impl Report {
    fn cursor(&self, ticks: u64) -> SpanCursor {
        SpanCursor {
            incarnation: self.incarnation.clone(),
            generation: self.generation,
            ticks,
        }
    }
}
#[test]
fn spans_are_exclusive_aggregate_per_tick_and_never_change_saves_or_hashes() {
    let mut off = Sim::<Nested>::new(()).unwrap();
    let mut on = Sim::<Nested>::new(()).unwrap();
    on.measure_spans(now, false, "test");
    for sim in [&mut off, &mut on] {
        sim.advance(0., Clock::Seekable);
        sim.advance(50., Clock::Seekable);
    }
    assert_eq!(off.world().hash(), on.world().hash());
    assert_eq!(off.save().unwrap(), on.save().unwrap());
    let report: Report = crate::json::from_str(&on.spans_json(None)).unwrap();
    let find = |name| report.spans.iter().find(|s| s.name == name).unwrap();
    assert_eq!(
        (find("outer").p50, find("inner").p50, find("tick.other").p50),
        (4., 4., 2.)
    );
    assert!(find("outer").at.contains("diagnostics/tests.rs:"));
    assert_eq!(find("outer").count, 3);
    assert_eq!(
        report.spans.iter().map(|s| s.perTick).sum::<f64>(),
        report.tickMs.mean
    );
    assert_eq!(report.tickMs.mean, 10.);
    let delta: Report = crate::json::from_str(&on.spans_json(Some(report.cursor(2)))).unwrap();
    assert_eq!(delta.spans[0].count, 1);
    assert!(on
        .spans_json(Some(report.cursor(4)))
        .contains("counters went back"));
    assert_eq!(find("inner").p95, 4.);
}

#[derive(Default, crate::Resource)]
struct Visited(u32);
struct Footgun;
impl Game for Footgun {
    const ID: &'static str = "span-query-scaling";
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.insert_resource(Visited::default());
    }
    fn tick(w: &mut World, _: &Input, _: &()) {
        let _span = w.span("render-column-query");
        // Constructing the exclusive render-column query touches its pages even
        // when the loop writes nothing. The control visits the same column shared.
        let count = if w.resource::<Visited>().0 == 0 {
            w.query::<&mut Transform>().iter().count()
        } else {
            w.query::<&Transform>().iter().count()
        };
        std::hint::black_box(count);
        // Mutable access invalidates the column's cached digest even without
        // assignments. Include the observation that pays for that invalidation.
        std::hint::black_box(w.hash());
    }
}
#[test]
fn hundred_thousand_entity_query_span() {
    let mut measurements = Vec::new();
    for count in [1000, 100_000] {
        for exclusive in [true, false] {
            let mut sim = Sim::<Footgun>::new(()).unwrap();
            for _ in 0..count {
                sim.world_mut().spawn(Transform::default());
            }
            sim.world().resource_mut::<Visited>().0 = u32::from(!exclusive);
            // This detects mutable-query digest invalidation and population scaling,
            // not a release-time budget. Each arm uses its fastest of five trials
            // so unrelated work on the machine can only inflate the other trials.
            sim.advance(0., Clock::Live);
            let mut minimum = f64::INFINITY;
            for trial in 1..=5 {
                sim.measure_spans(clock, true, "test");
                sim.advance(trial as f64 * 17., Clock::Live);
                let text = sim.spans_json(None);
                let report: Report = crate::json::from_str(&text).unwrap();
                let sample = report
                    .spans
                    .iter()
                    .find(|s| s.name == "render-column-query")
                    .unwrap();
                assert!(sample.count >= 1);
                minimum = minimum.min(sample.p50);
            }
            measurements.push(minimum);
            println!("perf world: entities={count} exclusive={exclusive} minimum ms={minimum}");
        }
    }
    let [small_mut, _small_shared, large_mut, large_shared] = measurements[..] else {
        unreachable!()
    };
    assert!(
        large_mut > large_shared * 2.,
        "mutable/shared ratio: {measurements:?}"
    );
    assert!(
        large_mut > small_mut * 5.,
        "mutable scaling: {measurements:?}"
    );
}

#[test]
fn ten_seconds_of_tick_spans_are_joined_to_submitted_frames() {
    let mut sim = Sim::<Nested>::new(()).unwrap();
    sim.measure_spans(now, true, "test");
    sim.advance(0., Clock::Seekable);
    for tick in 1..=600 {
        sim.advance(tick as f64 * 1000. / 60. + 0.001, Clock::Seekable);
        if tick % 10 == 0 {
            sim.trace_frame();
        }
    }
    #[derive(Default, crate::Data)]
    struct Args {
        tick: u64,
        frame: u64,
    }
    #[derive(Default, crate::Data)]
    struct Interval {
        name: String,
        ts: f64,
        dur: f64,
        args: Args,
    }
    let events = sim.trace_events_json();
    let rows: Vec<Interval> = crate::json::from_str(&events).unwrap();
    let ticks: Vec<_> = rows.iter().filter(|e| e.name == "tick").collect();
    assert_eq!(ticks.len(), 600);
    for tick in ticks {
        let frame = rows
            .iter()
            .find(|e| e.name == "frame" && e.args.frame == tick.args.frame)
            .unwrap();
        assert!(frame.ts <= tick.ts && frame.ts + frame.dur >= tick.ts + tick.dur);
        assert_eq!(tick.args.frame, tick.args.tick.div_ceil(10));
        for name in ["outer", "inner"] {
            let named: Vec<_> = rows
                .iter()
                .filter(|e| e.name == name && e.args.tick == tick.args.tick)
                .collect();
            assert_eq!(named.len(), if name == "inner" { 2 } else { 1 });
            for span in named {
                assert_eq!(span.args.frame, tick.args.frame);
                assert!(span.ts >= tick.ts && span.ts + span.dur <= tick.ts + tick.dur);
            }
        }
    }
    if let Ok(path) = std::env::var("EXACT_DIAGNOSTIC_TRACE") {
        std::fs::write(path, format!("{{\"traceEvents\":{events}}}")).unwrap();
    }
    println!(
        "trace export: 600 ticks under 60 submitted frames; {} intervals",
        rows.len()
    );
}

#[test]
fn a_difference_from_zero_refuses_when_its_prefix_has_expired() {
    let mut sim = Sim::<Nested>::new(()).unwrap();
    sim.measure_spans(now, true, "test");
    let start: Report = crate::json::from_str(&sim.spans_json(None)).unwrap();
    sim.advance(0., Clock::Seekable);
    sim.advance(300_000., Clock::Seekable);
    assert!(sim
        .spans_json(Some(start.cursor(0)))
        .contains("window expired"));
    assert!(!sim.spans_json(None).contains("error"));
}

#[test]
fn remainder_uses_its_name_and_difference_statistics_exclude_the_prefix() {
    let mut s = Spans::default();
    s.arm(now, true, "test");
    s.row("first", Location::caller());
    s.rows.swap(0, 1);
    s.begin(1);
    add(100.);
    s.end();
    s.begin(2);
    add(2.);
    s.end();
    let report: Report = crate::json::from_str(&s.json(Some(SpanCursor {
        incarnation: s.incarnation.clone(),
        generation: s.generation,
        ticks: 1,
    })))
    .unwrap();
    assert_eq!(report.tickMs.p50, 2.);
    let other = report
        .spans
        .iter()
        .find(|r| r.name == "tick.other")
        .unwrap();
    assert_eq!(other.p50, 2.);
    assert_eq!(
        report.spans.iter().find(|r| r.name == "first").unwrap().p50,
        0.
    );
}

#[test]
fn span_cursors_distinguish_worlds_restores_and_recordings_with_equal_counters() {
    let read =
        |sim: &Sim<Nested>| -> Report { crate::json::from_str(&sim.spans_json(None)).unwrap() };
    let mut sim = Sim::<Nested>::new(()).unwrap();
    sim.measure_spans(now, false, "test");
    let saved = sim.save().unwrap();
    let first = read(&sim);
    assert_eq!(first.spanTicks, 0);
    sim.measure_spans(now, false, "test");
    assert_eq!(read(&sim).generation, first.generation);
    assert!(!sim.spans_json(Some(first.cursor(0))).contains("error"));

    sim.measure_spans(now, true, "test");
    let reset = read(&sim);
    assert_eq!(reset.spanTicks, first.spanTicks);
    assert_eq!(reset.incarnation, first.incarnation);
    assert_ne!(reset.generation, first.generation);
    assert!(sim
        .spans_json(Some(first.cursor(0)))
        .contains("recording generation"));
    assert_eq!(sim.save().unwrap(), saved);

    sim.restore(&saved).unwrap();
    sim.measure_spans(now, false, "test");
    let restored = read(&sim);
    assert_eq!(restored.spanTicks, first.spanTicks);
    assert_eq!(restored.generation, first.generation);
    assert_ne!(restored.incarnation, first.incarnation);
    assert!(sim
        .spans_json(Some(first.cursor(0)))
        .contains("world incarnation"));
    assert_eq!(sim.save().unwrap(), saved);

    let mut remounted = Sim::<Nested>::new(()).unwrap();
    remounted.measure_spans(now, false, "test");
    let mounted = read(&remounted);
    assert_eq!(mounted.spanTicks, first.spanTicks);
    assert_eq!(mounted.generation, first.generation);
    assert_ne!(mounted.incarnation, first.incarnation);
    assert!(remounted
        .spans_json(Some(first.cursor(0)))
        .contains("world incarnation"));
}
