use crate::{compile, observe, oracle::Oracle, script};

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
            "outcome refused",
            "ran task refuse refused",
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
    assert!(steps[4].contains(&"ran task tick ok".into()));
    let properties = contract::properties("after \"one\" \"isolated\"\n  writes none\n").unwrap();
    let report = crate::property::evaluate(&properties, &lines);
    assert_eq!(report.counts[0][0].counted, 1);
    assert_eq!(report.counts[0][0].checked, 0);

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
    crate::expectations_on(&s, &lines).unwrap();
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
    crate::expectations_on(&s, &lines).unwrap();
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

#[test]
fn scripted_state_bounds_and_membership_check_the_runner_observation() {
    let source = r#"component App
  state n = -2
  state word = "ready"
  view
    text "Bounds"
test "bounds"
  expect state n >= -2
  expect state n <= 2
  expect state n in 0 -2 2
  expect state word in "idle" "ready"
"#;
    let s = script::scripted("bounds", source, None).unwrap().remove(0);
    assert!(s.delivery.stopped.is_none());
    let plan = compile(&s.case).unwrap();
    let (lines, _) = observe::run(
        plan.clone(),
        Oracle::for_case(&plan, &s.case),
        &s.case.events,
    );
    crate::expectations_on(&s, &lines).unwrap();
    let wrong = script::scripted("bounds", &source.replace("n >= -2", "n >= -1"), None)
        .unwrap()
        .remove(0);
    assert!(crate::expectations_on(&wrong, &lines)
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
    crate::expectations_on(&s, &lines).unwrap();
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
