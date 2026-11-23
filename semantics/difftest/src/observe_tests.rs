use crate::{compile, observe, oracle::Oracle, script};

#[test]
fn state_paths_deliver_nested_fields_and_indices() {
    let scripts = script::scripted(
        "paths",
        include_str!("../../corpus/records/observation-paths.contract"),
        None,
    )
    .unwrap();
    assert_eq!(scripts.len(), 2);
    for s in &scripts {
        assert!(s.delivery.stopped.is_none(), "{:?}", s.delivery.stopped);
        crate::expectations(s).unwrap();
    }
    assert_eq!(scripts[1].delivery.until_met, 6);
}

#[test]
fn missing_and_invalid_state_paths_never_match_none() {
    let source = include_str!("../../corpus/records/observation-paths.contract");
    let s = script::scripted("paths", source, None).unwrap().remove(0);
    let plan = compile(&s.case).unwrap();
    let (lines, _) = observe::run(plan.clone(), Oracle::for_case(&plan, &s.case), &[]);
    assert_eq!(
        crate::state_path::value(&plan, &lines, "gateway.matrix.0.1"),
        Some(observe::number(2.0).as_str())
    );
    assert_eq!(
        crate::state_path::value(&plan, &lines, "gateway.rows.1.absent"),
        Some(observe::number(9.0).as_str())
    );
    assert!(crate::state_path::value(&plan, &lines, "remote.state").is_some());
    for path in [
        "unknown",
        "unknown.state",
        "gateway.missing",
        "gateway.rows.2.state",
        "gateway.rows.999999999999999999999999999999999999999",
        "gateway.rows.-1",
        "gateway.rows.+0",
        "gateway.rows.01",
        "gateway.rows.length",
        "gateway.matrix.0.9",
        "gateway.phase.state.field",
        "gateway.phase.absent.field",
        "saved.state",
        "gateway.",
        "gateway..phase",
        "",
        ".gateway",
    ] {
        assert_eq!(
            crate::state_path::value(&plan, &lines, path),
            None,
            "{path}"
        );
        let mut missing = s.clone();
        missing.items = vec![script::Item::Expect(
            script::Expect::State(
                path.into(),
                contract_syntax::StateComparison::Equal,
                vec!["none".into()],
            ),
            Default::default(),
        )];
        assert!(
            crate::expectations_on(&missing, &plan, &lines).is_err(),
            "{path}"
        );
    }
    for path in [
        "gateway.missing",
        "gateway.rows.2.state",
        "gateway.phase.absent.field",
        "saved.state",
    ] {
        let source = format!("{source}\ntest \"missing wait\"\n  clock until state {path} == null within 10\n  tap \"start\"\n");
        let s = script::scripted("missing wait", &source, None)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(s.case.events, [script::Event::Land], "{path}");
        assert_eq!(s.delivery.taken, 0, "{path}");
        let (_, kind, why) = s.delivery.stopped.unwrap();
        assert_eq!(kind, "clock until (not met)", "{path}");
        assert!(why.contains("<not printed>"), "{why}");
    }
    let absent = lines
        .iter()
        .filter(|l| !l.starts_with("slot gateway "))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        crate::state_path::value(&plan, &absent, "gateway.phase.state"),
        None
    );
}

#[test]
#[ignore = "async lane: needs Lean (semantics/README.md)"]
fn state_paths_match_on_runner_and_lean_observations() {
    let scripts = script::scripted(
        "paths",
        include_str!("../../corpus/records/observation-paths.contract"),
        None,
    )
    .unwrap();
    crate::leanrun::build().unwrap();
    let cases = scripts.iter().map(|s| s.case.clone()).collect();
    let outcomes = crate::check(cases, 2, "state-paths").unwrap();
    for (s, outcome) in scripts.iter().zip(outcomes) {
        assert!(matches!(outcome.verdict, crate::Verdict::Agree));
        let plan = compile(&s.case).unwrap();
        let (runner, lean) = outcome.observations.unwrap();
        for lines in [runner, lean] {
            crate::expectations_on(s, &plan, &lines).unwrap();
            let last = crate::chunks(&lines).last().copied().unwrap();
            assert_eq!(
                crate::state_path::value(&plan, last, "held.phase.state"),
                Some(
                    observe::quote(if s.delivery.until_met == 0 {
                        "idle"
                    } else {
                        "ready"
                    })
                    .as_str()
                )
            );
        }
    }
}

#[test]
fn trace_names_due_work_and_handlers_whose_arguments_refused() {
    let scripts = script::scripted(
        "trace",
        include_str!("../../corpus/trace/tasks.contract"),
        None,
    )
    .unwrap();
    let traces: Vec<Vec<String>> = scripts
        .iter()
        .map(|s| {
            let plan = compile(&s.case).unwrap();
            let oracle = Oracle::for_case(&plan, &s.case);
            let (lines, _) = observe::run(plan, oracle, &s.case.events);
            lines
                .into_iter()
                .filter(|l| l.starts_with("ran ") || l.starts_with("outcome "))
                .collect()
        })
        .collect();
    assert_eq!(
        traces[0],
        [
            "outcome ok",
            "outcome refused",
            "ran task refuse refused",
            "outcome ok",
            // The task due at the tap's arrival, in its own step; the tap's
            // outcome is still the due work's refusal.
            "outcome refused",
            "ran task refuse refused",
            "outcome refused",
            "ran handler add ok",
            "outcome ok",
        ]
    );
    assert_eq!(
        traces[1],
        [
            "outcome ok",
            "outcome refused",
            "ran task refuse refused",
            "outcome refused",
            "ran task refuse refused",
            "outcome ok",
            "ran task add ok",
            "ran task add ok",
        ]
    );
    assert_eq!(
        traces[2],
        [
            "outcome ok",
            "outcome refused",
            "outcome refused",
            "ran handler take refused",
            "outcome ok"
        ]
    );
}

#[test]
fn queued_commits_name_the_mutation_and_then_names_the_action() {
    let scripts = script::scripted(
        "trace",
        include_str!("../../corpus/trace/queue.contract"),
        None,
    )
    .unwrap();
    let s = &scripts[0];
    let plan = compile(&s.case).unwrap();
    let oracle = Oracle::for_case(&plan, &s.case);
    let (lines, _) = observe::run(plan, oracle, &s.case.events);
    let ran: Vec<_> = lines
        .iter()
        .filter(|l| l.starts_with("ran "))
        .map(String::as_str)
        .collect();
    assert_eq!(
        ran,
        [
            "ran handler save ok",
            "ran then landed ok",
            "ran queue saved ok",
            "ran then landed ok"
        ]
    );
}

#[test]
fn land_has_no_timer_commit_and_an_unmodeled_receipt_is_marked() {
    let source = include_str!("../../corpus/properties/land.contract");
    let mut s = script::scripted("land", source, None).unwrap().remove(0);
    s.case.events = vec![
        script::Event::Clock(1.0),
        script::Event::Land,
        script::Event::Land,
        script::Event::Tap("one".into()),
        script::Event::InputLand,
    ];
    let plan = compile(&s.case).unwrap();
    let (lines, _) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    let steps = crate::chunks(&lines);
    assert!(steps[2].contains(&"ran then landed ok".into()));
    assert!(!steps[2].iter().any(|l| l.starts_with("ran task")));
    assert!(!steps[3].iter().any(|l| l.starts_with("ran ")));
    // The timer due exactly now fires at the tap's arrival, in its own step.
    assert_eq!(steps[4][0], "== due");
    assert!(steps[4].contains(&"ran task tick ok".into()));
    let ran: Vec<_> = steps[5].iter().filter(|l| l.starts_with("ran ")).collect();
    assert_eq!(ran, ["ran handler one ok"]);
    // `old` is the configuration the handler met, after the tick.
    let properties =
        contract::properties("after \"one\" \"isolated\"\n  expect state ticks == old\n").unwrap();
    let report = crate::property::evaluate(&properties, &lines);
    assert_eq!(report.counts[0][0].counted, 0);
    assert_eq!(report.counts[0][0].checked, 1);
    assert!(report.passed());

    let source = "component App\n  view\n    time datetime=0 format=\"timer\" testId=\"time\"\n\ntest \"tick\"\n  clock +1000\n";
    let s = script::scripted("time", source, None).unwrap().remove(0);
    let plan = compile(&s.case).unwrap();
    let (lines, _) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    assert!(crate::chunks(&lines)[1].contains(&observe::OMITTED_COMMIT.into()));
    assert!(!lines.iter().any(|l| l.starts_with("ran ")));
}

#[test]
fn curried_refusal_traces_the_action_and_names_its_arguments() {
    let s = script::scripted(
        "trace",
        include_str!("../../corpus/trace/tasks.contract"),
        None,
    )
    .unwrap()
    .remove(2);
    let plan = compile(&s.case).unwrap();
    let (lines, _) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    assert!(lines
        .iter()
        .any(|l| l.contains("HandlerArguments { action: \"take\"")));
    assert!(lines.iter().any(|l| l == "ran handler take refused"));
    assert!(lines.iter().any(|l| l.contains("arguments refused")));
}

#[test]
fn typing_uses_the_inputs_handler_before_falling_back_to_change() {
    let source = r#"component App
  state note = ""
  action edit(value)
    note = value
  view
    column
      input testId="input" value=note input=edit
      input testId="change" value=note change=edit
test "type in both fields"
  type "input" "first"
  expect state note == "first"
  type "change" "second"
  expect state note == "second"
"#;
    let s = script::scripted("input", source, None).unwrap().remove(0);
    let plan = compile(&s.case).unwrap();
    let (lines, _) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    crate::expectations_on(&s, &plan, &lines).unwrap();
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.as_str() == "ran handler edit ok")
            .count(),
        2
    );
}

#[test]
fn host_facts_are_the_runners_own_sources_never_a_gated_resources() {
    let sources = |file: &str, text: &str| {
        let scripts = script::scripted(file, text, None).unwrap();
        let plan = compile(&scripts[0].case).unwrap();
        let oracle = Oracle::for_case(&plan, &scripts[0].case);
        let (_, oracle) = observe::run(plan, oracle, &scripts[0].case.events);
        let oracle = oracle.unwrap();
        let mut names: Vec<String> = oracle
            .host_sources()
            .into_iter()
            .map(str::to_string)
            .collect();
        names.sort();
        names.dedup();
        names
    };
    // Closed at boot, these resources ask the oracle once their `when` opens.
    assert!(sources("gate", include_str!("../../corpus/resources/gate.contract")).is_empty());
    assert_eq!(
        sources(
            "device-facts",
            include_str!("../../corpus/components/device-facts.contract")
        ),
        ["exactPage", "exactViewport"]
    );
}

/// A native module's read is answered by the runner from the module's
/// substitute (#401): its boot answer is a host fact, the oracle's text names
/// the module's source, and a step that re-asks the app's own source beside
/// it leaves the module's answer where it was.
#[test]
fn a_module_read_is_a_host_fact_and_its_source_a_module() {
    let path = crate::leanrun::project().join("corpus/resources/module.contract");
    let source = std::fs::read_to_string(&path).unwrap();
    let s = script::scripted("module", &source, Some(&path))
        .unwrap()
        .remove(0);
    let plan = compile(&s.case).unwrap();
    let (lines, oracle) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    crate::expectations_on(&s, &plan, &lines).unwrap();
    let oracle = oracle.unwrap();
    assert_eq!(oracle.host_sources(), ["geolocationStatus"]);
    let text = oracle.text();
    assert!(text.contains("module \"geolocationStatus\"\n"), "{text}");
    assert!(text.contains("fact \"status\" "), "{text}");
    // The app's own source is the oracle's, asked once per argument.
    assert_eq!(text.matches("answer \"here\"").count(), 2, "{text}");
    assert!(
        !lines.iter().any(|l| l.starts_with(observe::MODULE_MOVED)),
        "{lines:#?}"
    );
}

/// What the runner does itself is outside the semantics: a declared
/// source's request (LLP 1162.001), which the oracle's text names so the
/// semantics calls it unsupported, and a live time's text (LLP 1126 D6),
/// which the observation leaves out.
#[test]
fn a_declared_source_and_a_live_time_are_the_runners_own() {
    let source = r#"shape Now
  t: number

source current(city: string) as shape Now
  url "https://example.test/now?city={city}"

component App
  resource now = current("Berlin") as shape Now
  view
    time datetime=0 format="timer" testId="time"

test "tick"
  clock +1000
"#;
    let s = script::scripted("own", source, None).unwrap().remove(0);
    let plan = compile(&s.case).unwrap();
    let (lines, oracle) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    let text = oracle.unwrap().text();
    assert!(text.contains("fetch \"current\"\n"), "{text}");
    assert!(!text.contains("answer \"current\""), "{text}");
    assert!(lines.contains(&"view \"time\" -".to_string()), "{lines:#?}");
}

#[test]
fn scripted_state_bounds_and_membership_check_the_runner_observation() {
    let source = r#"component App
  state n = -2
  state word = "ready"
  view
    text "Bounds"
test "bounds"
  expect state n != 0
  expect state n >= -2
  expect state n <= 2
  expect state n in 0 -2 2
  expect state word in "idle" "ready"
"#;
    assert!(!script::state_holds(
        None,
        contract_syntax::StateComparison::NotEqual,
        &["none".into()]
    ));
    let s = script::scripted("bounds", source, None).unwrap().remove(0);
    assert!(s.delivery.stopped.is_none());
    let plan = compile(&s.case).unwrap();
    let (lines, _) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    crate::expectations_on(&s, &plan, &lines).unwrap();
    let wrong = script::scripted("bounds", &source.replace("n >= -2", "n >= -1"), None)
        .unwrap()
        .remove(0);
    assert!(crate::expectations_on(&wrong, &plan, &lines)
        .unwrap_err()
        .contains("does not hold"));
}

/// A module read held at boot (its permission still prompt) shows its
/// placeholder, which is no answer: no host fact, so the semantics leaves the
/// read out (#401).
#[test]
fn an_unanswered_module_read_is_no_fact() {
    let path = crate::leanrun::project().join("corpus/resources/module-unanswered.contract");
    let source = std::fs::read_to_string(&path).unwrap();
    let s = script::scripted("unanswered", &source, Some(&path))
        .unwrap()
        .remove(0);
    let plan = compile(&s.case).unwrap();
    let (lines, oracle) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    crate::expectations_on(&s, &plan, &lines).unwrap();
    let oracle = oracle.unwrap();
    assert!(oracle.host_sources().is_empty());
    let text = oracle.text();
    assert!(!text.contains("fact \"position\""), "{text}");
    assert!(text.contains("module \"watchPosition\"\n"), "{text}");
}

/// A polled substitute (an accelerometer's samples) moves its read on a clock
/// advance, a commit the trace leaves out: the step carries both notes, and
/// the semantics' observation, which holds the boot answer, differs there.
/// That difference is outside the semantics, not a divergence (#401).
#[test]
fn a_polled_module_answer_that_moves_is_outside() {
    let path = crate::leanrun::project().join("corpus/resources/module.contract");
    let source = "use accelerometer, Accelerometer from \"@exact/motion\"\n\ncomponent App\n  resource motion = accelerometer() as shape Accelerometer else empty()\n  view\n    text `${motion.sequence}` testId=\"motion\"\n\ntest \"polls\"\n  clock +3000\n";
    let s = script::scripted("polled", source, Some(&path))
        .unwrap()
        .remove(0);
    let plan = compile(&s.case).unwrap();
    let mut oracle = Oracle::for_case(&plan, &s.case);
    oracle.grants = "device.motion".into();
    let (rust, oracle) = observe::run(plan, oracle, &s.case.events);
    assert_eq!(oracle.unwrap().host_sources(), ["accelerometer"]);
    let steps = crate::chunks(&rust);
    let moved = steps[1];
    assert!(
        moved.contains(&observe::OMITTED_COMMIT.to_string()),
        "{rust:#?}"
    );
    assert!(
        moved.iter().any(|l| l.starts_with(observe::MODULE_MOVED)),
        "{rust:#?}"
    );
    // What the semantics prints: the boot answer through the advance.
    let held: Vec<&String> = steps[0]
        .iter()
        .filter(|l| l.starts_with("resource ") || l.starts_with("view "))
        .collect();
    let mut lean: Vec<String> = steps[0].to_vec();
    lean.extend(["== clock +3000".to_string(), "outcome ok".to_string()]);
    lean.extend(held.into_iter().cloned());
    assert_ne!(
        lean[4..],
        moved
            .iter()
            .filter(|l| !l.starts_with('#'))
            .cloned()
            .collect::<Vec<_>>()[..]
    );
    assert!(
        matches!(crate::compare(rust, lean), crate::Verdict::Unsupported(why) if why.starts_with(observe::MODULE_MOVED))
    );
}

const PAIRING: &str = r#"shape Pairing
  phase: enum<"idle", "code", "done">
  code: string
  peer: string
component App
  resource pairing = pairing() as shape Pairing else empty(phase="idle", code="", peer="")
  derive phase = pairing.phase
  state taps = 0
  action press
    taps = taps + 1
  view
    column
      button "Press" testId="press" press=press
      text pairing.peer testId="peer"
      if pairing.phase == "code"
        text pairing.code testId="code"
      if pairing.phase == "done"
        text "Paired" testId="done"
"#;

/// A test chooses what a source answers, before boot and between steps: each
/// later line asks the resource again and changes only the fields it names,
/// and fields no line names keep the oracle's answer.
#[test]
fn a_test_chooses_a_sources_answers_and_changes_them_between_steps() {
    let source = format!(
        r#"{PAIRING}test "phases"
  answer source pairing with {{"phase": "idle"}}
  expect state phase == "idle"
  expect tree missing "code"
  answer source pairing with {{"phase": "code", "code": "123456"}}
  expect state phase == "code"
  expect text "code" == "123456"
  tap "press"
  expect text "code" == "123456"
  answer source pairing with {{"phase": "done"}}
  expect tree missing "code"
  expect tree has "done"
  answer source pairing with {{"phase": "code"}}
  expect text "code" == "123456"
"#
    );
    let s = script::scripted("answers", &source, None)
        .unwrap()
        .remove(0);
    assert!(s.delivery.stopped.is_none(), "{:?}", s.delivery.stopped);
    assert_eq!(
        s.case.answers,
        [("pairing".into(), r#"{"phase":"idle"}"#.into())]
    );
    let plan = compile(&s.case).unwrap();
    let (lines, data) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    crate::expectations_on(&s, &plan, &lines).unwrap();
    let oracle = data.unwrap();
    let boot = oracle.text();
    assert!(boot.contains(r#""idle""#) && !boot.contains(r#""123456""#));
    for (revision, phase) in [(1, "code"), (2, "done"), (3, "code")] {
        let calls = oracle.calls_text(Some("pairing"), revision);
        assert!(
            calls.contains(&crate::observe::quote(phase)) && calls.contains("123456"),
            "{calls}"
        );
    }
    assert!(lines.iter().all(|line| line != observe::OMITTED_COMMIT));
    let peers: std::collections::BTreeSet<&String> = lines
        .iter()
        .filter(|l| l.starts_with("view \"peer\" "))
        .collect();
    assert_eq!(
        peers.len(),
        1,
        "an unnamed field keeps one answer: {peers:?}"
    );
    assert!(
        lines.contains(&r#"== answer source pairing with {"phase":"done"}"#.to_string()),
        "{lines:?}"
    );
    let prepared = crate::prepare(&s.case, 0).expect("prepare the same script for Lean");
    assert_eq!(
        prepared.lean.events.len(),
        s.case.events.len(),
        "compare every delivered step"
    );
}

#[test]
fn an_answer_the_source_cannot_give_is_refused_by_name() {
    let plan = contract::compile(PAIRING).unwrap();
    let check = |source: &str, json: &str| crate::oracle::check_answer(&plan, source, json);
    assert_eq!(check("pairing", r#"{"phase": "code"}"#), Ok(()));
    assert_eq!(
        check("pairng", "{}").unwrap_err(),
        "the app has no source `pairng`"
    );
    assert_eq!(
        check("pairing", r#"{"stage": "code"}"#).unwrap_err(),
        "`Pairing` has no field `stage`"
    );
    assert_eq!(
        check("pairing", r#"{"phase": "lost"}"#).unwrap_err(),
        r#"phase: "lost" is not a member of "idle", "code", "done""#
    );
    assert_eq!(
        check("pairing", r#"{"code": 7}"#).unwrap_err(),
        "code: 7 is not a string"
    );
}

const KEYED: &str = r#"component App
  state closed = false
  state note = ""
  action keys(key: string)
    if key == "Escape"
      closed = true
      note = "closed-by-key"
  action released(key: string)
    if key == "F2"
      note = "renaming"
  action typed(value: string)
    note = value
  action save
    note = "saved"
  view
    column key=keys testId="window"
      column keyup=released testId="release"
      column press=save key=keys testId="pressable"
      input value=note input=typed key=keys testId="note"
      button "Save" press=save key=keys testId="save"
"#;

#[test]
fn a_tests_key_press_runs_its_handlers_unless_its_default_is_the_hosts() {
    let tests = r#"test "Escape closes the window"
  expect state closed == false
  type "window" key "Escape"
  expect state closed == true

test "a key at a field types into it, which the host does"
  type "note" key "Escape"
  expect state closed == true

test "the host activates a pressable box"
  type "pressable" key "Enter"

test "the host moves focus"
  type "window" key "Shift+Tab"
"#;
    let source = format!("{KEYED}\n{tests}");
    let scripts = script::scripted("keys", &source, None).unwrap();
    let closes = &scripts[0];
    assert!(
        closes.delivery.stopped.is_none(),
        "{:?}",
        closes.delivery.stopped
    );
    assert_eq!(
        closes.case.events,
        [
            script::Event::Key("window".into(), "Escape".into()),
            script::Event::InputLand
        ]
    );
    assert_eq!(
        closes.case.events[0].label(),
        r#"type "window" key "Escape""#
    );
    crate::expectations(closes).unwrap();
    for (script, reason) in scripts[1..]
        .iter()
        .zip(["TextInput", "press handler", "focus"])
    {
        let (_, kind, why) = script.delivery.stopped.as_ref().unwrap();
        assert_eq!(kind, "type key (a control's default)");
        assert!(why.contains(reason), "{why}");
        assert!(script.case.events.is_empty());
    }
}

#[test]
fn a_walk_presses_the_keys_its_key_handlers_hold() {
    let base = script::Case {
        name: "keys".into(),
        seed: Some(3),
        source: KEYED.into(),
        events: Vec::new(),
        path: None,
        stored: false,
        prerendered: false,
        launch: Default::default(),
        answers: Vec::new(),
    };
    let plan = compile(&base).unwrap();
    let source = format!("{KEYED}\ntest \"focus traversal\"\n  type \"window\" key \"Tab\"\n");
    let tests = contract_syntax::parse(&source).unwrap().tests;
    let inputs = crate::walk::Inputs::new(&source, tests.into_iter());
    let walk = crate::walk::generate(&plan, &base, None, 3, 0, 40, &inputs);
    let keys: Vec<_> = walk
        .scripted
        .case
        .events
        .iter()
        .filter_map(|e| match e {
            script::Event::Key(target, key) => Some((target.as_str(), key.as_str())),
            _ => None,
        })
        .collect();
    let expected = [("window", "Escape"), ("release", "F2")];
    for key in expected {
        assert!(keys.contains(&key), "{keys:?}");
    }
    assert!(keys.iter().all(|k| expected.contains(k)), "{keys:?}");
    for action in ["keys", "released"] {
        assert!(walk.rendered.contains(action));
        assert!(walk
            .observation
            .iter()
            .any(|line| line.starts_with("ran handler") && line.contains(action)));
    }
}

#[test]
fn a_key_runs_capture_outermost_first_then_bubbles_until_a_stop() {
    let source = include_str!("../../corpus/edge/keyboard-propagation.contract");
    for s in script::scripted("route", source, None).unwrap() {
        assert!(
            s.delivery.stopped.is_none(),
            "{}: {:?}",
            s.case.name,
            s.delivery.stopped
        );
        crate::expectations(&s).unwrap_or_else(|e| panic!("{}: {e}", s.case.name));
    }
}

#[test]
fn input_runs_after_every_change_due_at_its_arrival() {
    let source = include_str!("../../corpus/timers/input-order.contract");
    for s in script::scripted("input order", source, None).unwrap() {
        assert!(s.delivery.stopped.is_none(), "{:?}", s.delivery.stopped);
        crate::expectations(&s).unwrap_or_else(|e| panic!("{}: {e}", s.case.name));
    }
}

/// The fixture's test cases with the properties in its test file, each
/// script's runner observation and its property report.
fn fixture_report(name: &str) -> Vec<(Vec<String>, crate::property::Report)> {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus/properties")
        .join(format!("{name}.contract"));
    let source = std::fs::read_to_string(&file).unwrap();
    let text = std::fs::read_to_string(file.with_extension("test.contract")).unwrap();
    let properties = contract::properties(&text).unwrap();
    crate::property_corpus::cases(&file, name, &source)
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|s| {
            crate::expectations(&s).unwrap_or_else(|e| panic!("{}: {e}", s.case.name));
            let plan = compile(&s.case).unwrap();
            let oracle = Oracle::for_case(&plan, &s.case);
            let (lines, _) = observe::run(plan, oracle, &s.case.events);
            let report = crate::property::evaluate(&properties, &lines);
            (lines, report)
        })
        .collect()
}

#[test]
fn an_after_pair_reads_the_configuration_the_due_changes_left() {
    let mut scripts = fixture_report("due-press");
    assert_eq!(scripts.len(), 1);
    let (lines, report) = scripts.remove(0);
    let steps: Vec<Vec<&str>> = crate::chunks(&lines)
        .iter()
        .map(|s| {
            s.iter()
                .filter(|l| l.starts_with("== ") || l.starts_with("ran "))
                .map(String::as_str)
                .collect()
        })
        .collect();
    assert_eq!(
        steps[1..3],
        [
            vec!["== due", "ran task markA ok", "ran task markB ok"],
            vec!["== tap \"go\"", "ran handler press ok"],
        ]
    );
    for count in &report.counts[0] {
        assert_eq!((count.checked, count.counted, count.false_), (1, 0, 0));
    }
}

#[test]
fn after_clock_checks_the_work_due_at_an_inputs_arrival() {
    let mut scripts = fixture_report("due-clock");
    assert_eq!(scripts.len(), 1);
    let (_, report) = scripts.remove(0);
    assert_eq!(report.counts[0][0].false_, 1);
    assert_eq!(report.failures[0].step, 1);
}

#[test]
fn due_commands_stay_with_the_due_step_and_fold_ahead_of_the_handlers() {
    let source = "component App\n  state n = 1\n  state d = 1\n  state done = false\n  action sync\n    copyText(\"due\")\n  action press\n    copyText(\"press\")\n    done = true\n  action broken\n    copyText(\"never\")\n    n = n / (d - 1)\n  task watch when not done key=done\n    change(sync)\n  view\n    column\n      button \"Go\" press=press testId=\"go\"\n      button \"Break\" press=broken testId=\"break\"\n\ntest \"press\"\n  tap \"go\"\n\ntest \"break\"\n  tap \"break\"\n";
    let scripts = script::scripted("due commands", source, None).unwrap();
    let observe = |s: &script::Scripted| {
        let plan = compile(&s.case).unwrap();
        let oracle = Oracle::for_case(&plan, &s.case);
        observe::run(plan, oracle, &s.case.events).0
    };
    let lines = observe(&scripts[0]);
    let steps = crate::chunks(&lines);
    assert_eq!(steps[1][0], "== due");
    assert!(steps[1].contains(&"command copyText \"due\"".into()));
    assert!(!steps[2].contains(&"command copyText \"due\"".into()));
    let folded = crate::fold_due(&lines);
    let commands: Vec<_> = folded
        .iter()
        .filter(|l| l.starts_with("command "))
        .collect();
    assert_eq!(
        commands,
        ["command copyText \"due\"", "command copyText \"press\""]
    );
    assert_eq!(
        crate::chunks(&folded).len(),
        crate::chunks(&lines).len() - 1
    );
    // A refused handler after due commands moves nothing untraced.
    let lines = observe(&scripts[1]);
    let steps = crate::chunks(&lines);
    assert_eq!(steps[1][0], "== due");
    assert!(steps[2].contains(&"ran handler broken refused".into()));
    assert!(!steps[2].iter().any(|l| l == observe::OMITTED_COMMIT));
}
