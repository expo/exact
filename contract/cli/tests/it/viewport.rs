//! @ref LLP 1039 §5 — bake, first frame, resize, carry and invalid facts.
use crate::source::{boxed, Source};
use exact_kernel::{Kernel, PropId};
use exact_runner::{agent, DataError, DataSource, Runner, RunnerError, Value, Viewport};

struct NoData;

#[test]
fn window_toolbar_intention_survives_compile_bake_and_boot() {
    let plan = contract::compile(
        "component App\n  view\n    row role=\"toolbar\" toolbarPlacement=\"window\" testId=\"commands\"\n      text \"Home\" role=\"heading\" aria-level=1\n",
    )
    .unwrap();
    let runner = Runner::boot(
        contract::bake(plan, boxed(NoData)).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = runner.kernel().find_by_test_id("commands")[0];
    let props = &runner.kernel().node_by_key(key).unwrap().props;
    assert_eq!(props.str(PropId::ToolbarPlacement), Some("window"));
    assert_eq!(props.str(PropId::AccessibilityRole), Some("toolbar"));
}

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("host fact reached data: {source}")
    }
    fn ready(&self) -> bool {
        false
    }
}
fn plan() -> exact_plan::Plan {
    contract::bake(
        contract::compile(include_str!("../../../corpus/viewport.contract")).unwrap(),
        boxed(NoData),
    )
    .unwrap()
}
fn present(r: &Runner<Source>, id: &str) -> bool {
    !r.kernel().find_by_test_id(id).is_empty()
}
fn state(r: &Runner<Source>) -> serde_json::Value {
    serde_json::from_str(&agent::state(r)).unwrap()
}

#[test]
fn baked_phone_first_wide_batch_then_one_narrow_commit() {
    let baked = plan();
    let initial = Value::from_bytes(baked.bytes(baked.resources[0].initial)).unwrap();
    assert_eq!(
        initial,
        Value::record(vec![Value::Number(844.0), Value::Number(390.0)])
    );
    assert!(baked.resources[0].reader);
    assert!(!contract::typescript(&baked)
        .unwrap()
        .contains("exactViewport"));
    let mut r = Runner::boot(
        baked,
        boxed(NoData),
        Kernel::with_monospace(),
        Viewport::sized(1280.0, 900.0),
        "/",
    )
    .unwrap();
    assert_eq!(r.kernel().epoch(), 1);
    assert!(present(&r, "wide") && !present(&r, "narrow"));
    assert_eq!(
        state(&r)["resources"]["viewport"],
        serde_json::json!({"width":1280,"height":900})
    );
    let rail = r.kernel().find_by_test_id("rail")[0];
    assert_eq!(
        r.kernel()
            .node_by_key(rail)
            .unwrap()
            .props
            .str(PropId::AccessibilityOrientation),
        Some("vertical")
    );
    let receipt = r.set_viewport(390.0, 844.0).unwrap().unwrap();
    assert_eq!(receipt.epoch, 2);
    assert!(present(&r, "narrow") && !present(&r, "wide"));
    assert!(r.set_viewport(390.0, 844.0).unwrap().is_none());
    assert_eq!(
        state(&r)["resources"]["viewport"],
        serde_json::json!({"width":390,"height":844})
    );
    println!("bake 390x844; boot 1280x900 wide at epoch 1; resize 390x844 narrow at epoch {}; identical resize None", receipt.epoch);
    let carried = r.carry();
    let r = Runner::boot_carrying(
        plan(),
        boxed(NoData),
        Kernel::with_monospace(),
        &carried,
        Viewport::sized(1280.0, 900.0),
        "/",
    )
    .unwrap();
    assert!(present(&r, "wide"));
}

#[test]
fn invalid_sizes_refuse_without_poisoning_and_unread_sizes_do_not_commit() {
    let mut r = Runner::boot(
        plan(),
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for (width, height) in [
        (0.0, 844.0),
        (-1.0, 844.0),
        (390.0, 0.0),
        (f64::NAN, 844.0),
        (390.0, f64::INFINITY),
    ] {
        assert!(matches!(
            r.set_viewport(width, height),
            Err(RunnerError::InvalidViewport)
        ));
        assert_eq!(r.viewport(), Viewport::default());
        assert!(present(&r, "narrow"));
    }
    assert!(agent::logs(&r, 0).contains("InvalidViewport"));
    assert_eq!(r.set_viewport(1280.0, 900.0).unwrap().unwrap().epoch, 2);
    let plan = contract::compile("component A\n  view\n    text \"empty\"\n").unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(r.set_viewport(1280.0, 900.0).unwrap().is_none());
    assert_eq!(r.viewport().width, 1280.0);
}

#[test]
fn fields_and_arguments_refuse_at_the_fact_boundary() {
    for source in ["shape V\n  width: string\ncomponent A\n  resource viewport = exactViewport() as shape V\n  view\n    text viewport.width\n", "shape V\n  width: number\ncomponent A\n  resource viewport = exactViewport(1) as shape V\n  view\n    text `${viewport.width}`\n"] {
        let p = contract::compile(source).unwrap();
        assert!(contract::bake(p,boxed(NoData)).is_err());
    }
}

#[test]
fn every_declared_viewport_is_reanswered_in_one_commit_by_field_name() {
    let src = r#"shape Width
  width: number
component App
  resource a = exactViewport() as shape Width
  resource b = exactViewport() as shape Width
  view
    column
      text `${a.width}` testId="width-a"
      text `${b.width}` testId="width-b"
"#;
    let plan = contract::bake(contract::compile(src).unwrap(), boxed(NoData)).unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let receipt = r.set_viewport(1280.0, 900.0).unwrap().unwrap();
    assert_eq!(receipt.epoch, 2);
    assert_eq!(state(&r)["resources"]["a"]["width"], 1280);
    assert_eq!(state(&r)["resources"]["b"]["width"], 1280);
    assert!(agent::logs(&r, 0).contains("viewport (2 asked again)"));
}

/// LLP 1078 D2: the fold's three fields ride `exactViewport` — the bake
/// answers `continuous`, 1, 1; a host's `set_fold` re-answers every reader
/// in one commit; an unknown or mistyped field is refused at the bake.
#[test]
fn fold_fields_are_viewport_fields_the_bake_answers_flat() {
    let src = r#"shape Fold
  devicePosture: string
  horizontalViewportSegments: number
  verticalViewportSegments: number
component App
  resource m = exactViewport() as shape Fold
  derive two = m.horizontalViewportSegments == 2
  view
    column
      text `${m.devicePosture} ${m.horizontalViewportSegments}x${m.verticalViewportSegments}` testId="fold"
      if two
        text "two panes" testId="panes"
"#;
    let plan = contract::bake(contract::compile(src).unwrap(), boxed(NoData)).unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let text = |r: &Runner<Source>, id: &str| {
        let key = r.kernel().find_by_test_id(id)[0];
        r.kernel()
            .node_by_key(key)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Text)
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(text(&r, "fold"), "continuous 1x1");
    assert!(r.kernel().find_by_test_id("panes").is_empty());
    let state: serde_json::Value = serde_json::from_str(&agent::state(&r)).unwrap();
    assert_eq!(state["device"]["devicePosture"], "continuous");
    assert_eq!(state["device"]["horizontalViewportSegments"], 1);
    let folded = exact_runner::Fold {
        posture: exact_runner::Posture::Folded,
        cols: 2,
        rows: 1,
    };
    let receipt = r.set_fold(folded).unwrap().unwrap();
    assert_eq!(receipt.epoch, 2, "one commit");
    assert_eq!(text(&r, "fold"), "folded 2x1");
    assert_eq!(r.kernel().find_by_test_id("panes").len(), 1);
    assert!(r.set_fold(folded).unwrap().is_none(), "the same fold again");
    assert!(matches!(
        r.set_fold(exact_runner::Fold { cols: 0, ..folded }),
        Err(RunnerError::InvalidViewport)
    ));
    assert_eq!(r.viewport().fold, folded, "nothing changed");
    // A resize re-answers the same resource (one source, LLP 1039 D2) and
    // keeps the fold.
    assert!(r.set_viewport(900.0, 600.0).unwrap().is_some());
    assert_eq!(r.viewport().fold, folded, "a resize keeps the fold");
    assert_eq!(text(&r, "fold"), "folded 2x1");
    let state: serde_json::Value = serde_json::from_str(&agent::state(&r)).unwrap();
    assert_eq!(state["device"]["devicePosture"], "folded");
    assert_eq!(state["device"]["horizontalViewportSegments"], 2);
    for wrong in [
        "shape F\n  devicePosture: number\ncomponent A\n  resource m = exactViewport() as shape F\n  view\n    text `${m.devicePosture}`\n",
        "shape F\n  hingeAngle: number\ncomponent A\n  resource m = exactViewport() as shape F\n  view\n    text `${m.hingeAngle}`\n",
    ] {
        assert!(contract::bake(contract::compile(wrong).unwrap(), boxed(NoData)).is_err(), "{wrong}");
    }
}

#[test]
fn display_ratio_is_optional_host_data_and_updates_without_display_lengths() {
    let plan = contract::bake(contract::compile(
        "shape Viewport\n  devicePixelRatio: option<number>\ncomponent App\n  resource viewport = exactViewport() as shape Viewport\n  view\n    text \"Display\"\n",
    ).unwrap(), boxed(NoData)).unwrap();
    let mut r = Runner::boot(
        plan.clone(),
        boxed(NoData),
        Kernel::with_monospace(),
        Viewport::default(),
        "/",
    )
    .unwrap();
    assert_eq!(
        r.resource("viewport"),
        Some(&Value::record(vec![Value::NONE]))
    );
    for scale in [1.0, 2.0, 1.5] {
        assert!(r.set_display_scale(scale).unwrap().is_some());
        assert_eq!(
            r.resource("viewport"),
            Some(&Value::record(vec![Value::some(Value::Number(scale))]))
        );
        assert!(r.set_display_scale(scale).unwrap().is_none());
    }
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(r.set_display_scale(scale).is_err());
        assert_eq!(r.viewport().device_pixel_ratio, Some(1.5));
    }
    assert!(r.take_store_writes().is_empty());
    let carried = r.carry();
    let fresh = Runner::boot_carrying(
        plan.clone(),
        boxed(NoData),
        Kernel::with_monospace(),
        &carried,
        Viewport::default(),
        "/",
    )
    .unwrap();
    assert_eq!(
        fresh.resource("viewport"),
        Some(&Value::record(vec![Value::NONE]))
    );
    let mut kernel = Kernel::with_monospace();
    kernel.set_display_scale(1.0).unwrap();
    let primed = Runner::boot(plan, boxed(NoData), kernel, Viewport::default(), "/").unwrap();
    assert_eq!(
        primed.resource("viewport"),
        Some(&Value::record(vec![Value::some(Value::Number(1.0))]))
    );
}

#[test]
fn numeric_safe_area_and_segments_follow_document_environment() {
    let plan = contract::bake(
        contract::compile(
            r#"shape Segment
  height: number
  x: number
  width: number
  y: number
shape Viewport
  safeAreaTop: number
  safeAreaRight: number
  safeAreaBottom: number
  safeAreaLeft: number
  segments: Segment[]
component App
  resource viewport = exactViewport() as shape Viewport
  view
    text `${viewport.safeAreaTop}`
"#,
        )
        .unwrap(),
        boxed(NoData),
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Viewport::sized(900., 600.),
        "/",
    )
    .unwrap();
    assert_eq!(
        state(&r)["resources"]["viewport"]["segments"],
        serde_json::json!([{ "x": 0, "y": 0, "width": 900, "height": 600 }])
    );
    r.kernel_mut()
        .set_env(exact_kernel::Env::new(24., 1., 34., 2.))
        .unwrap();
    assert!(r.refresh_geometry().unwrap().is_some());
    assert!(r.refresh_geometry().unwrap().is_none());
    let v = &state(&r)["resources"]["viewport"];
    assert_eq!(
        [
            v["safeAreaTop"].as_f64(),
            v["safeAreaRight"].as_f64(),
            v["safeAreaBottom"].as_f64(),
            v["safeAreaLeft"].as_f64()
        ],
        [Some(24.), Some(1.), Some(34.), Some(2.)]
    );
    let fold = exact_runner::Fold {
        posture: exact_runner::Posture::Folded,
        cols: 2,
        rows: 1,
    };
    for gap in [20., 40.] {
        r.kernel_mut()
            .set_segments(
                2,
                1,
                vec![
                    exact_kernel::Rect::new(0., 0., 450. - gap / 2., 600.),
                    exact_kernel::Rect::new(450. + gap / 2., 0., 450. - gap / 2., 600.),
                ],
            )
            .unwrap();
        assert!(r.set_fold(fold).unwrap().is_some());
        assert_eq!(
            state(&r)["resources"]["viewport"]["segments"][1]["x"],
            450. + gap / 2.
        );
        assert!(r.set_fold(fold).unwrap().is_none());
    }
    r.kernel_mut().set_segments(1, 1, vec![]).unwrap();
    r.set_fold(exact_runner::Fold::FLAT).unwrap();
    r.set_viewport(600., 900.).unwrap();
    assert_eq!(
        state(&r)["resources"]["viewport"]["segments"],
        serde_json::json!([{ "x": 0, "y": 0, "width": 600, "height": 900 }])
    );
    assert!(r.take_store_writes().is_empty());
}

#[test]
fn refused_numeric_geometry_restores_the_previous_environment() {
    let plan = contract::compile(
        r#"shape Viewport
  safeAreaTop: number
component App
  resource viewport = exactViewport() as shape Viewport
  derive quotient = 1 / (24 - viewport.safeAreaTop)
  view
    text `${quotient}`
"#,
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Viewport::default(),
        "/",
    )
    .unwrap();
    r.kernel_mut()
        .set_env(exact_kernel::Env::new(24., 0., 0., 0.))
        .unwrap();
    assert!(r.refresh_geometry().is_err());
    assert_eq!(r.kernel().env().top, 0.);
    assert_eq!(state(&r)["resources"]["viewport"]["safeAreaTop"], 0.);
    r.kernel_mut()
        .set_env(exact_kernel::Env::new(12., 0., 0., 0.))
        .unwrap();
    assert!(r.refresh_geometry().unwrap().is_some());
    assert_eq!(state(&r)["resources"]["viewport"]["safeAreaTop"], 12.);
}

#[test]
fn refused_first_display_scale_keeps_the_absence_of_a_report() {
    let plan = contract::compile(
        r#"shape Viewport
  devicePixelRatio: option<number>
component App
  resource viewport = exactViewport() as shape Viewport
  derive ratio = match viewport.devicePixelRatio { case some(n) => n, case none => 1 }
  derive quotient = 1 / (2 - ratio)
  view
    text `${quotient}`
"#,
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Viewport::default(),
        "/",
    )
    .unwrap();
    assert!(r.set_display_scale(2.0).is_err());
    assert_eq!(r.viewport().device_pixel_ratio, None);
    assert_eq!(r.kernel().reported_display_scale(), None);
    assert!(r.set_display_scale(1.5).unwrap().is_some());
    assert_eq!(r.viewport().device_pixel_ratio, Some(1.5));
}

#[test]
fn bottom_toolbar_belongs_to_one_route_column() {
    let bar = "row role=\"toolbar\" toolbarPlacement=\"bottom\"";
    let route = |body: &str| {
        format!(
            "component App\n  state left = true\n  state right = false\n  view\n    main navigationKey=\"home\"\n      column navigationKey=\"home\"\n{body}\n"
        )
    };
    for (body, id) in [
        (
            format!("        {bar}\n        {bar}"),
            "lower-toolbar-count",
        ),
        (
            format!("        column\n          {bar}"),
            "lower-toolbar-place",
        ),
        (
            "        row toolbarPlacement=\"bottom\"".into(),
            "lower-toolbar-place",
        ),
        (
            "        row toolbarPlacement=\"\"".into(),
            "lower-attr-value",
        ),
        (
            "        row toolbarPlacement=\"botom\"".into(),
            "lower-attr-value",
        ),
    ] {
        assert_eq!(
            contract::compile(&route(&body)).unwrap_err().id,
            id,
            "{body}"
        );
    }
    for body in [
        format!(
            "        if left == right\n          {bar}\n        if left != right\n          {bar}"
        ),
        format!("        for n in [1,2] key=n\n          if n == 1\n            {bar}"),
        format!(
            "        if left == right\n          {bar}\n        if left != right\n          text \"Different\""
        ),
        format!("        {bar}\n        if left\n          {bar}"),
    ] {
        let (plan, warnings) = contract::compile_warned(&route(&body));
        plan.unwrap();
        assert!(
            !warnings.iter().any(|w| w.id == "lower-toolbar-last"),
            "{body}"
        );
    }
    for before in [
        format!("        {bar}"),
        format!("        if left\n          {bar}"),
    ] {
        let (plan, warnings) =
            contract::compile_warned(&route(&format!("{before}\n        text \"After\"")));
        plan.unwrap();
        assert_eq!(
            warnings
                .iter()
                .filter(|w| w.id == "lower-toolbar-last")
                .count(),
            1
        );
    }
    let dir = crate::scratch::Scratch::new(format!("toolbar-hosts-{}", std::process::id()));
    std::fs::write(
        dir.join("app.contract"),
        route(&format!(
            "        {bar}\n          if left\n            progress"
        )),
    )
    .unwrap();
    for target in ["web", "ios"] {
        std::fs::write(
            dir.join("app.json"),
            format!(r#"{{"app":{{"name":"Bar","id":"dev.test.bar"}},"host":{{"{target}":{{}}}}}}"#),
        )
        .unwrap();
        let (plan, _) = contract::compile_path_all_warned(&dir.join("app.contract"), false);
        if target == "web" {
            plan.unwrap();
        } else {
            assert_eq!(plan.err().unwrap()[0].id, "lower-toolbar-item");
        }
    }
}

#[test]
fn shared_placement_action_keeps_its_old_words_and_cannot_make_an_unchecked_bottom_toolbar() {
    let source = r#"component App
  state placement = "window"
  action place(value: string)
    placement = value
  view
    column
      row role="toolbar" toolbarPlacement=placement testId="tools"
        text "Actions"
      button "Keyboard" testId="keys" press=place("keyboard")
      button "Bottom" testId="bad" press=place("bottom")
"#;
    let plan = contract::bake(contract::compile(source).unwrap(), boxed(NoData)).unwrap();
    let mut runner = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = runner.kernel().find_by_test_id("tools")[0];
    for (button, expected) in [
        ("keys", Some("keyboard")),
        ("bad", None),
        ("keys", Some("keyboard")),
    ] {
        let target = runner.kernel().find_by_test_id(button)[0];
        let id = runner.kernel().node_by_key(target).unwrap().id;
        runner.dispatch(id, exact_runner::Event::Press).unwrap();
        assert_eq!(
            runner
                .kernel()
                .node_by_key(key)
                .unwrap()
                .props
                .str(PropId::ToolbarPlacement),
            expected
        );
    }
    assert!(agent::logs(&runner, 0).contains("write a literal bottom toolbar row"));
}
