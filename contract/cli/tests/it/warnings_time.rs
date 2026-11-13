//! Launch-relative times (`performanceNow()`) sent to a source or kept in a
//! persisted state: a relaunch reads them against another origin.

use super::warnings::warned;

/// A stopwatch's actions over a mutation, each `send` on line 18.
fn stopwatch(state: &str, send: &str) -> String {
    format!(
        "shape Time\n  epochAtZero: number\n  anchorAt: number\n  anchorBeat: number\nshape Ack\n  ok: bool\nfn later(t: number): number = t + 10\nfn mark(): number = performanceNow()\ncomponent App\n  resource time = exactTime() as shape Time\n  mutation wrote as shape Ack\n  state start = 0\n  state other = 0\n{state}  action begin\n    start = performanceNow()\n  action save\n    send wrote = keep({send})\n  view\n    button \"save\" press=save\n"
    )
}

const PERSISTED: &str = "type-performance-now-persisted";

#[test]
fn a_launch_relative_time_sent_to_a_source_is_named_with_its_repair() {
    for send in [
        "performanceNow()",
        "start",
        "start + 1000",
        "floor(start / 1000)",
        "later(start)",
        "mark()",
        "`${start}`",
        "(other > 0 ? start : 0)",
        "max(start, 5)",
        // Less a constant, a time is still a time.
        "start - 1000",
        "performanceNow() - 2 * 1000",
        "start - max(500, 1000)",
        // A reading less a span, the origin, or a never-set 0 is still a time.
        "performanceNow() - (performanceNow() - start)",
        "performanceNow() - time.epochAtZero",
        "performanceNow() - other",
        // The origin added on one branch only: the other is a launch's time.
        "performanceNow() + (other > 0 ? time.epochAtZero : 0)",
        // A reading with a span added is a later reading.
        "performanceNow() + (performanceNow() - start)",
    ] {
        assert_eq!(
            warned(&stopwatch("", send)),
            [(PERSISTED.to_string(), 17)],
            "{send}"
        );
    }
    let (_, warnings) = contract::compile_warned(&stopwatch("", "start"));
    assert!(
        warnings[0].message.contains("this argument to `keep`")
            && warnings[0]
                .message
                .contains("`time.epochAtZero + performanceNow()`"),
        "{warnings:?}"
    );
    // Through a derive, and a `let` in the action.
    let derived = stopwatch("  derive since = start\n", "since");
    assert_eq!(warned(&derived)[0].0, PERSISTED);
    let local = stopwatch("", "t").replace(
        "    send wrote = keep(t)",
        "    let t = performanceNow()\n    send wrote = keep(t)",
    );
    assert_eq!(warned(&local), [(PERSISTED.to_string(), 18)]);
}

#[test]
fn the_wall_clock_time_and_a_duration_are_not_named() {
    for send in [
        "time.epochAtZero + performanceNow()",
        "performanceNow() + time.epochAtZero",
        "time.epochAtZero + start",
        "performanceNow() - start",
        "(performanceNow() - start) / 1000",
        "other",
        "`${start}`.length",
        "(start > 5).toString()",
        // A musical position from an anchor a source holds (euxy's beat):
        // the time since the anchor, scaled and offset.
        "time.anchorBeat + (performanceNow() - time.anchorAt) * 120 / 60000",
        "max(0, time.anchorBeat + (performanceNow() - time.anchorAt) * 2 / 60000)",
    ] {
        assert_eq!(warned(&stopwatch("", send)), [], "{send}");
    }
    // A derive of the origin, added as `epochAtZero` is.
    let origin = stopwatch("  derive origin = time.epochAtZero\n", "origin + start");
    assert_eq!(warned(&origin), []);
}

#[test]
fn a_launch_relative_time_in_a_persisted_state_is_named() {
    // stopwatch's start mark, kept by `persist` instead of a source.
    let src = "shape Time\n  epochAtZero: number\ncomponent App\n  resource time = exactTime() as shape Time\n  state start = 0 persist\n  state wall = 0 persist\n  state lap = 0 persist\n  action begin\n    lap = performanceNow() - start\n    start = performanceNow()\n    wall = time.epochAtZero + performanceNow()\n  view\n    button \"go\" press=begin\n";
    assert_eq!(warned(src), [(PERSISTED.to_string(), 5)]);
    // A state kept as a reading less itself: from its initial 0 the first
    // is a time, the next a span, and so on (a timestamp kept every other run).
    let toggled = "component App\n  state mark = 0 persist\n  action tick\n    mark = performanceNow() - mark\n  view\n    button \"go\" press=tick\n";
    assert_eq!(warned(toggled), [(PERSISTED.to_string(), 2)]);
    // A state that is a launch's time in one action and the origin in
    // another is sometimes a time: so is a number added to it.
    let either = "shape Time\n  epochAtZero: number\nshape Ack\n  ok: bool\ncomponent App\n  resource time = exactTime() as shape Time\n  mutation wrote as shape Ack\n  state mark = 0\n  action a\n    mark = performanceNow()\n  action b\n    mark = time.epochAtZero\n  action save\n    send wrote = keep(mark + 1000)\n  view\n    button \"save\" press=save\n";
    assert_eq!(warned(either), [(PERSISTED.to_string(), 14)]);
    // A derive of the origin, read before the fixed point resolves it, is
    // the origin: added to a reading it is the wall-clock time, kept as is.
    let origin = "shape Time\n  epochAtZero: number\ncomponent App\n  resource time = exactTime() as shape Time\n  state at = 0 persist\n  derive origin = time.epochAtZero\n  action go\n    at = origin + performanceNow()\n  view\n    button \"go\" press=go\n";
    assert_eq!(warned(origin), []);
}

#[test]
fn a_banked_duration_with_a_reset_is_not_named() {
    // A stopwatch with a Reset banks what was banked plus a reading less
    // its start: a span, kept across Stop and Start, however it is grouped.
    let src = |total: &str, keep: &str| {
        format!("shape Ack\n  ok: bool\ncomponent App\n  mutation wrote as shape Ack\n  state start = 0\n  state banked = 0\n  action begin\n    start = performanceNow()\n  action stop\n    let total = {total}\n    banked = total\n  action reset\n    banked = 0\n  action save\n    send wrote = keep({keep})\n  view\n    button \"save\" press=save\n")
    };
    for total in [
        "banked + performanceNow() - start",
        "banked + (performanceNow() - start)",
        "performanceNow() - start + banked",
    ] {
        assert_eq!(warned(&src(total, "banked")), [], "{total}");
    }
    // The bank added to a reading is still a launch's time.
    let reading = src(
        "banked + performanceNow() - start",
        "banked + performanceNow()",
    );
    assert_eq!(warned(&reading), [(PERSISTED.to_string(), 15)]);
}
