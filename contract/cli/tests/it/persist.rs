//! `state … persist` (LLP 1116 D5): a root state the store keeps across
//! launches — restored before the first frame where it still fits its type,
//! written when changed by a commit that stands.

use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{Runner, StoreWrite};

const SRC: &str = r#"
component App
  state tipPercent = 18 persist
  state units = "metric" persist
  state seen = none persist
  state recent = [1, 2] persist
  state zero = 0
  action tip(n: number)
    tipPercent = n
  action convert
    units = units == "metric" ? "imperial" : "metric"
    seen = some(units)
  action add
    recent = recent.concat([-1.5])
  action refuse
    tipPercent = 99
    zero = 1 / zero
  view
    column
      text "{tipPercent}% {units}" testId="t"
"#;

fn boot(snapshot: Vec<(&str, &str)>) -> Runner<()> {
    let plan = contract::compile(SRC).unwrap();
    let snapshot = snapshot
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
        .collect();
    Runner::boot_stored(
        plan,
        (),
        Kernel::with_monospace(),
        snapshot,
        Default::default(),
        "/",
    )
    .unwrap()
}

fn kept(writes: Vec<StoreWrite>) -> Vec<(String, Vec<u8>)> {
    writes
        .into_iter()
        .map(|w| {
            (
                w.name,
                w.value.expect("a persisted state is never forgotten"),
            )
        })
        .collect()
}

fn pair(name: &str, value: &str) -> (String, Vec<u8>) {
    (format!("exact.state.{name}"), value.as_bytes().to_vec())
}

#[test]
fn a_fresh_launch_keeps_the_declared_values() {
    let mut r = boot(vec![]);
    assert_eq!(r.slot("tipPercent"), Some(&Value::Number(18.0)));
    assert_eq!(
        kept(r.take_store_writes()),
        vec![
            pair("tipPercent", "18"),
            pair("units", "\"metric\""),
            pair("seen", "null"),
            pair("recent", "[1,2]"),
        ],
        "after boot the store holds every persisted state; `zero` is not one"
    );
    assert!(r.journal().any(|l| l.ends_with("store exact.state.units")));
}

#[test]
fn a_commit_writes_what_changed_and_a_refused_one_writes_nothing() {
    let mut r = boot(vec![]);
    r.take_store_writes();
    r.act("tip", vec![Value::Number(22.0)]).unwrap();
    assert_eq!(kept(r.take_store_writes()), vec![pair("tipPercent", "22")]);
    r.act("tip", vec![Value::Number(22.0)]).unwrap();
    assert!(
        r.take_store_writes().is_empty(),
        "an unchanged value is not written again"
    );
    assert!(r.act("refuse", vec![]).is_err());
    assert!(r.take_store_writes().is_empty());
    assert_eq!(r.persisted("tipPercent"), Some(Value::Number(22.0)));
}

#[test]
fn the_next_launch_reads_back_what_the_last_kept() {
    let mut first = boot(vec![]);
    first.act("tip", vec![Value::Number(-0.0)]).unwrap();
    first.act("convert", vec![]).unwrap();
    first.act("add", vec![]).unwrap();
    let snapshot = first.store().snapshot();
    assert!(snapshot.contains(&pair("tipPercent", "-0")));
    assert!(snapshot.contains(&pair("recent", "[1,2,-1.5]")));
    let plan = contract::compile(SRC).unwrap();
    let second = Runner::boot_stored(
        plan,
        (),
        Kernel::with_monospace(),
        snapshot,
        Default::default(),
        "/",
    )
    .unwrap();
    let tip = second.slot("tipPercent").unwrap().as_number().unwrap();
    assert!(tip == 0.0 && tip.is_sign_negative(), "{tip}");
    assert_eq!(second.slot("units"), Some(&Value::str("imperial")));
    assert_eq!(
        second.slot("seen"),
        Some(&Value::Option(Some(Value::str("metric").into())))
    );
    assert_eq!(second.persisted("recent"), second.slot("recent").cloned());
}

#[test]
fn a_stored_value_of_another_shape_keeps_the_initial_value_and_says_so() {
    let mut r = boot(vec![
        ("exact.state.tipPercent", "\"eighteen\""),
        ("exact.state.units", "\"imperial\""),
        ("exact.state.recent", "[1,\"x\"]"),
        ("exact.state.seen", "{not json"),
        ("exact.state.gone", "1"),
    ]);
    assert_eq!(r.slot("tipPercent"), Some(&Value::Number(18.0)));
    assert_eq!(r.slot("units"), Some(&Value::str("imperial")));
    assert_eq!(r.slot("seen"), Some(&Value::Option(None)));
    let lines: Vec<&str> = r
        .journal()
        .filter(|l| l.contains("persisted state"))
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[0].ends_with(
        "persisted state tipPercent: the stored value does not fit number: the initial value stands"
    ));
    assert!(lines[1].contains("seen: the stored value does not fit option<string>"));
    assert!(lines[2].contains("recent: the stored value does not fit number[]"));
    // The store mirrors the state again from the boot on.
    assert_eq!(
        kept(r.take_store_writes()),
        vec![
            pair("tipPercent", "18"),
            pair("seen", "null"),
            pair("recent", "[1,2]")
        ]
    );
}

/// An enum is a string with a vocabulary: it persists, and a stored member
/// a later version dropped falls back to the initializer.
#[test]
fn enum_settings_persist_and_a_dropped_member_keeps_the_initial_value() {
    // The second member is the one `dim` chooses.
    let src = |other: &str| {
        format!(
            r#"
component App
  state mode: enum<"light", "{other}"> = "light" persist
  state chosen: option<enum<"light", "{other}">> = some("light") persist
  state modes: enum<"light", "{other}">[] = ["light"] persist
  action dim
    mode = "{other}"
    chosen = some("{other}")
    modes = ["light", "{other}"]
  view
    text "Settings"
"#
        )
    };
    let boot = |src: &str, snapshot| {
        Runner::boot_stored(
            contract::compile(src).unwrap(),
            (),
            Kernel::with_monospace(),
            snapshot,
            Default::default(),
            "/",
        )
        .unwrap()
    };
    let mut first = boot(&src("dark"), vec![]);
    first.act("dim", vec![]).unwrap();
    let snapshot = first.store().snapshot();
    assert!(snapshot.contains(&pair("mode", "\"dark\"")));
    assert!(snapshot.contains(&pair("modes", "[\"light\",\"dark\"]")));

    let second = boot(&src("dark"), snapshot.clone());
    assert_eq!(second.slot("mode"), Some(&Value::str("dark")));
    assert_eq!(
        second.slot("chosen"),
        Some(&Value::Option(Some(Value::str("dark").into())))
    );
    assert_eq!(
        second.slot("modes"),
        Some(&Value::list(vec![Value::str("light"), Value::str("dark")]))
    );

    let narrowed = boot(&src("sepia"), snapshot);
    assert_eq!(narrowed.slot("mode"), Some(&Value::str("light")));
    assert_eq!(
        narrowed.slot("chosen"),
        Some(&Value::Option(Some(Value::str("light").into())))
    );
    assert_eq!(
        narrowed.slot("modes"),
        Some(&Value::list(vec![Value::str("light")]))
    );
    let lines: Vec<&str> = narrowed
        .journal()
        .filter(|l| l.contains("persisted state"))
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    let fits = |line: &str, name: &str, ty: &str| {
        assert!(
            line.contains(&format!("{name}: the stored value does not fit {ty}:")),
            "{line}"
        );
    };
    fits(lines[0], "mode", r#"enum<"light", "sepia">"#);
    fits(lines[1], "chosen", r#"option<enum<"light", "sepia">>"#);
    fits(lines[2], "modes", r#"enum<"light", "sepia">[]"#);
}

#[test]
fn an_oversized_value_is_not_kept_and_is_reported_only_when_it_changes() {
    let src = r#"
component App
  state note = "" persist
  state n = 0
  action fill(s: string)
    note = s
  action bump
    n = n + 1
  view
    text `${n}`
"#;
    let plan = contract::compile(src).unwrap();
    let mut r = Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap();
    r.take_store_writes();
    let oversized = Value::str(&"x".repeat(17_000));
    let reports = |r: &Runner<()>| {
        r.journal()
            .filter(|l| l.contains("persisted state note:"))
            .count()
    };
    r.act("fill", vec![oversized.clone()]).unwrap();
    assert!(r.take_store_writes().is_empty());
    assert_eq!(r.persisted("note"), Some(Value::str("")));
    assert!(r
        .journal()
        .any(|l| l.contains("persisted state note: 17002 bytes is over the 16384")));
    assert_eq!(reports(&r), 1);
    for _ in 0..50 {
        r.act("bump", vec![]).unwrap();
    }
    assert_eq!(
        reports(&r),
        1,
        "unrelated commits do not repeat the warning"
    );
    r.act("fill", vec![oversized.clone()]).unwrap();
    assert_eq!(
        reports(&r),
        1,
        "assigning the same value does not warn again"
    );
    assert!(r.take_store_writes().is_empty());

    r.act("fill", vec![Value::str(&"y".repeat(17_000))])
        .unwrap();
    assert_eq!(reports(&r), 2, "a different value of the same size warns");
    r.act("fill", vec![oversized.clone()]).unwrap();
    assert_eq!(
        reports(&r),
        3,
        "returning to a previous oversized value warns"
    );
    assert_eq!(r.persisted("note"), Some(Value::str("")));
    assert!(r.take_store_writes().is_empty());

    r.act("fill", vec![Value::str("")]).unwrap();
    assert!(r.take_store_writes().is_empty());
    r.act("fill", vec![oversized.clone()]).unwrap();
    assert_eq!(
        reports(&r),
        4,
        "returning through the stored value resets the warning"
    );

    r.act("fill", vec![Value::str("saved")]).unwrap();
    assert_eq!(kept(r.take_store_writes()), vec![pair("note", "\"saved\"")]);
    r.act("fill", vec![oversized]).unwrap();
    assert_eq!(reports(&r), 5);
    assert_eq!(r.persisted("note"), Some(Value::str("saved")));
    assert!(r.take_store_writes().is_empty());
}

#[test]
fn oversized_initial_values_are_reported_once_for_each_slot() {
    let large = "x".repeat(17_000);
    let src = format!(
        r#"
component App
  state first = "{large}" persist
  state second = "{large}" persist
  state n = 0
  action bump
    n = n + 1
  view
    text `${{n}}`
"#
    );
    let plan = contract::compile(&src).unwrap();
    let mut r = Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap();
    for _ in 0..3 {
        r.act("bump", vec![]).unwrap();
    }
    for name in ["first", "second"] {
        assert_eq!(r.persisted(name), None);
        assert_eq!(
            r.journal()
                .filter(|l| l.contains(&format!("persisted state {name}:")))
                .count(),
            1,
            "{name}'s initial value is reported once"
        );
    }
    assert!(r.take_store_writes().is_empty());
}
