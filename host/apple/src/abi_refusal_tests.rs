use super::*;
use exact_runner::exact_canvas::{Context2d, DrawError, Frame};
use serde_json::{json, Value as Json};

struct Dot;

impl DataSource for Dot {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("dot".into(), 0)]
    }
    fn draw_2d(
        &mut self,
        _: &str,
        _: &[Value],
        ctx: &Context2d,
        _: &Frame,
    ) -> Result<bool, DrawError> {
        ctx.fill_rect(0., 0., 4., 4.);
        Ok(false)
    }
}

fn output(bridge: &Bridge<Dot>, len: u32) -> Json {
    serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap()
}

fn preserves_state(refuse: impl FnOnce(&mut Bridge<Dot>, u32) -> u32, error: &str) {
    let plan = contract::compile(
        r#"component App
  state shown = false
  action show
    shown = true
  task timer mount
    every(1000, show)
  view
    column
      button press=show testId="show"
        text "Show"
      box opacity=(shown ? 0.5 : 1) transition="opacity 200ms linear"
      if shown
        canvas surface=dot() width=40 height=40 testId="dot"
"#,
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Dot, Hooks::none(), 390., 844.);
    bridge.canvas_defer(true);
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let view = kernel
        .node_by_key(kernel.find_by_test_id("show")[0])
        .unwrap()
        .id;
    let len = bridge.dispatch(view, 0, 0, 10.);
    let made = output(&bridge, len);
    assert_eq!(made["timers"], true, "{made}");
    assert_eq!(made["motion"], true, "{made}");
    assert_eq!(made["canvasOwed"], true, "{made}");
    assert_eq!(made["timer_due_ms"], 1000., "{made}");
    assert!(made["ops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|op| op["op"] == "create"));
    assert!(!made["ops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|op| op["op"] == "canvas2d"));

    let len = refuse(&mut bridge, view);
    let refused = output(&bridge, len);
    assert!(
        refused["error"].as_str().is_some_and(|s| s.contains(error)),
        "{refused}"
    );
    assert_eq!(refused["ops"], json!([]), "{refused}");
    for field in ["timers", "timer_due_ms", "motion", "canvasOwed", "clock"] {
        assert_eq!(refused[field], made[field], "{field}: {refused}");
    }
    let len = bridge.canvas_draw();
    let drawn = output(&bridge, len);
    assert!(
        drawn["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["op"] == "canvas2d"),
        "{drawn}"
    );
    assert_ne!(drawn["canvasOwed"], true, "{drawn}");
}

fn dispatch_refusal(kind: u32, payload: &str, error: &str) {
    preserves_state(
        |bridge, view| {
            let len = bridge.input_write(payload.as_bytes());
            bridge.dispatch(view, kind, len, 500.)
        },
        error,
    );
}

#[test]
fn invalid_press_keeps_state() {
    dispatch_refusal(0, "invalid", "invalid press modifiers");
}

#[test]
fn invalid_picked_files_keeps_state() {
    dispatch_refusal(26, "invalid", "invalid picked files");
}

#[test]
fn invalid_checked_change_keeps_state() {
    dispatch_refusal(24, "invalid", "invalid checked state");
}

#[test]
fn invalid_checked_input_keeps_state() {
    dispatch_refusal(25, "invalid", "invalid checked state");
}

#[test]
fn invalid_host_event_keeps_state() {
    dispatch_refusal(13, "invalid", "invalid scroll");
}

#[test]
fn invalid_reorder_keeps_state() {
    dispatch_refusal(18, "invalid", "invalid reorder event");
}

#[test]
fn unknown_event_keeps_state() {
    dispatch_refusal(u32::MAX, "", "unknown event kind");
}

#[test]
fn invalid_surface_record_keeps_state() {
    preserves_state(
        |bridge, _| {
            let len = bridge.input_write(b"\xff");
            bridge.surface_record(len)
        },
        "surface record: invalid UTF-8",
    );
}

#[test]
fn invalid_place_keeps_state() {
    preserves_state(
        |bridge, _| {
            let len = bridge.input_write(b"en-US");
            bridge.set_place(len)
        },
        "place: expected locale NUL timeZone",
    );
}

#[test]
fn invalid_mode_keeps_state() {
    preserves_state(
        |bridge, _| {
            let len = bridge.input_write(b"invalid");
            bridge.set_mode(len)
        },
        "mode: expected axis=value pins",
    );
}

#[test]
fn rejected_mode_pins_keep_state() {
    preserves_state(
        |bridge, _| {
            let len = bridge.input_write(b"missing=value");
            bridge.set_mode(len)
        },
        "mode:",
    );
}

#[test]
fn a_borrowed_runtime_marks_its_answer_busy() {
    thread_local! {
        static RUNTIMES: RefCell<Registry<Dot>> = RefCell::new(Registry::default());
    }
    let rt = RUNTIMES.with(|r| r.borrow_mut().create());
    with_runtime(
        &RUNTIMES,
        rt,
        false,
        |_, _| {
            let len = with_runtime(
                &RUNTIMES,
                rt,
                false,
                |_, _| panic!("a reentrant call must not reach the borrowed host"),
                |len| len,
            );
            REFUSAL.with(|r| {
                let r = r.borrow();
                let batch: Json = serde_json::from_slice(&r[..len as usize]).unwrap();
                assert_eq!(batch["busy"], true, "{batch}");
                assert!(batch["error"].as_str().unwrap().contains("busy:"));
            });
        },
        |_| panic!("the outer call is available"),
    );
    RUNTIMES.with(|r| assert!(r.borrow_mut().destroy(rt)));
}

#[test]
fn truncated_intrinsics_keeps_state() {
    preserves_state(
        |bridge, _| {
            let len = bridge.input_write(&[0]);
            bridge.intrinsics(len)
        },
        "intrinsics: truncated record",
    );
}

#[test]
fn truncated_covers_keeps_state() {
    let placement = |kind: u32| {
        let mut record = vec![0; 28];
        record[4..8].copy_from_slice(&kind.to_le_bytes());
        record
    };
    for record in [vec![0], placement(6), placement(13)] {
        preserves_state(
            |bridge, view| {
                if record.len() == 28 {
                    let mut complete = record.clone();
                    complete.resize(36, 0);
                    complete[..4].copy_from_slice(&view.to_le_bytes());
                    complete[24..28].copy_from_slice(&view.to_le_bytes());
                    let len = bridge.input_write(&complete);
                    let batch = bridge.covers(len);
                    assert!(output(bridge, batch)["error"].is_null());
                }
                let len = bridge.input_write(&record);
                bridge.covers(len)
            },
            "covers: truncated record",
        );
    }
}

#[test]
fn truncated_colors_keeps_state() {
    preserves_state(
        |bridge, _| {
            let len = bridge.input_write(&[0]);
            bridge.colors(len)
        },
        "colors: truncated record",
    );
}

#[test]
fn failed_fresh_boot_keeps_state() {
    preserves_state(
        |bridge, _| bridge.boot(&[], Dot, Hooks::none(), 390., 844.),
        "boot:",
    );
}

#[test]
fn malformed_view_keeps_state() {
    preserves_state(
        |bridge, _| {
            let len = bridge.input_write(&[0]);
            bridge.view(len)
        },
        "view: malformed",
    );
}

/// `exact_view`'s words: the mask, eight floats, the regions' count and
/// `regions`, then `tail` (the segments' count and rects).
fn view_words(mask: u32, regions: &[f32], tail: &[u32]) -> Vec<u8> {
    let mut words = vec![mask];
    words.extend([0.0f32; 8].map(f32::to_bits));
    words.push(regions.len() as u32);
    words.extend(regions.iter().map(|r| r.to_bits()));
    words.extend(tail);
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// The segments' rects (mask 16) without their count, or with a count the
/// buffer does not hold or holds more than, are refused, never read past
/// the buffer.
#[test]
fn a_view_whose_segments_are_short_keeps_state() {
    for (regions, tail) in [
        (vec![], vec![]),
        (vec![0.0; 20], vec![]),
        (vec![], vec![2]),
        (vec![], vec![1, 0, 0, 0, 0, 0, 0, 0, 0]),
        (vec![], vec![u32::MAX]),
    ] {
        preserves_state(
            |bridge, _| {
                let len = bridge.input_write(&view_words(16, &regions, &tail));
                bridge.view(len)
            },
            "view: malformed",
        );
    }
}

/// A keyboard's rects (mask 16) after the regions, with and without them,
/// reach the kernel's grid with the viewport.
#[test]
fn a_view_carries_a_keyboards_segment_rects() {
    let plan = contract::compile("component App\n  view\n    column\n")
        .unwrap()
        .encode();
    for regions in [vec![], vec![0.0f32; 20]] {
        let mut bridge = Bridge::new();
        bridge.boot(&plan, Dot, Hooks::none(), 402., 874.);
        let rects = |h: f32| [0., 0., 402., 400., 0., 440., 402., h].map(f32::to_bits);
        bridge.input_write(
            &rects(434.)
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        bridge.segments(1, 1, 2, 2);
        let mut tail = vec![2];
        tail.extend(rects(15.));
        let mask = 16 | 8 | if regions.is_empty() { 0 } else { 4 };
        let mut words = view_words(mask, &regions, &tail);
        words[28..36].copy_from_slice(&[402.0f32.to_le_bytes(), 455.0f32.to_le_bytes()].concat());
        let len = bridge.input_write(&words);
        bridge.view(len);
        let env = bridge.host.as_ref().unwrap().runner().kernel().env();
        assert_eq!((env.cols, env.rows), (1, 2));
        assert_eq!(
            env.segments[1],
            exact_kernel::Rect::new(0., 440., 402., 15.)
        );
        assert_eq!(env.viewport_height, 455.);
        // The keyboard's overlap (mask 32) is the last word, after the rects.
        let mut tail = vec![2];
        tail.extend(rects(15.));
        tail.push(419.0f32.to_bits());
        let len = bridge.input_write(&view_words((mask & !8) | 32, &regions, &tail));
        let out = bridge.view(len);
        let batch = std::str::from_utf8(bridge.output_bytes(out as usize)).unwrap();
        assert!(!batch.contains("\"error\":\""), "{batch}");
        assert_eq!(
            bridge
                .host
                .as_ref()
                .unwrap()
                .runner()
                .viewport()
                .keyboard_height,
            419.
        );
        // Without that word, mask 32 is malformed and changes nothing.
        let len = bridge.input_write(&view_words(32, &regions, &[]));
        let out = bridge.view(len);
        let batch = std::str::from_utf8(bridge.output_bytes(out as usize)).unwrap();
        assert!(batch.contains("view: malformed"), "{batch}");
        assert_eq!(
            bridge
                .host
                .as_ref()
                .unwrap()
                .runner()
                .viewport()
                .keyboard_height,
            419.
        );
    }
}

#[test]
fn failed_plan_preparation_keeps_state() {
    preserves_state(
        |bridge, _| bridge.prepare_plan(0, Dot, Hooks::none(), 390., 844.),
        "boot:",
    );
}

#[test]
fn refused_preparation_keeps_state() {
    preserves_state(
        |bridge, _| bridge.refuse_preparation("invalid \"context\""),
        "invalid \"context\"",
    );
}

#[test]
fn missing_prepared_plan_keeps_state() {
    preserves_state(|bridge, _| bridge.commit_plan(), "no prepared plan");
}

#[test]
fn activities_link_only_when_the_presenter_declares_a_packaged_arm() {
    let plan = contract::compile(
        r#"shape Facts
  linked: bool
shape ID
  id: string
shape State
  title: string
component App
  resource facts = exactActivities() as shape Facts
  mutation sent as shape ID
  action start
    send sent = startActivity("train", State(title="Train"))
  view
    text facts.linked.toString()
"#,
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::<()>::new();
    bridge.boot(&plan, (), Hooks::none(), 390.0, 844.0);
    let host = bridge.host.as_mut().unwrap();
    assert!(format!("{:?}", host.runner_mut().act("start", vec![])).contains("not linked"));
    assert!(!host.runner_mut().take_activities_demand());
    bridge.link_activities();
    for _ in 0..2 {
        bridge.boot(&plan, (), Hooks::none(), 390.0, 844.0);
        let host = bridge.host.as_mut().unwrap();
        assert_eq!(
            host.runner_mut().resource("facts"),
            Some(&Value::record(vec![Value::Bool(true)]))
        );
        host.runner_mut().act("start", vec![]).unwrap();
        assert_eq!(host.runner_mut().take_requests().len(), 1);
        let failure = host.activities(r#"{"error":"dlopen: missing activity arm"}"#);
        assert!(
            failure.contains("dlopen: missing activity arm"),
            "{failure}"
        );
        assert!(host
            .runner()
            .journal()
            .any(|line| line.contains("dlopen: missing activity arm")));
    }
}

#[test]
fn a_same_clock_signed_zero_change_refuses_the_stale_candidate() {
    let plan = contract::compile(
        "component App\n  state value = 0\n  action negate\n    value = -value\n  view\n    button press=negate testId=\"negate\"\n      text `${value}`\n",
    ).unwrap().encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Dot, Hooks::none(), 390., 844.);
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), Dot, Hooks::none(), 390., 844.);
    let before = bridge.host.as_ref().unwrap().carry();
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let button = kernel
        .node_by_key(kernel.find_by_test_id("negate")[0])
        .unwrap()
        .id;
    bridge.dispatch(button, 0, 0, before.now_ms);
    let latest = bridge.host.as_ref().unwrap().carry();
    assert_eq!(latest.now_ms, before.now_ms);
    let value = |bridge: &Bridge<Dot>| {
        bridge
            .host
            .as_ref()
            .unwrap()
            .runner()
            .slot("value")
            .unwrap()
            .as_number()
            .unwrap()
            .to_bits()
    };
    assert_eq!(value(&bridge), (-0.0f64).to_bits());
    let len = bridge.commit_plan();
    assert_eq!(output(&bridge, len)["pending"], true);
    assert_eq!(value(&bridge), (-0.0f64).to_bits());
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), Dot, Hooks::none(), 390., 844.);
    let len = bridge.commit_plan();
    assert_ne!(output(&bridge, len)["pending"], true);
    assert_eq!(value(&bridge), (-0.0f64).to_bits());
}

#[test]
fn mode_batches_carry_presenter_work_after_inspection() {
    let plan = contract::compile(
        r#"shape ID
  id: string
shape State
  title: string
component App
  mutation sent as shape ID
  action start
    send sent = startActivity("train", State(title="Train"))
  view
    text "Train"
"#,
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.link_activities();
    bridge.boot(&plan, Dot, Hooks::none(), 390., 844.);
    bridge
        .host
        .as_mut()
        .unwrap()
        .runner_mut()
        .act("start", vec![])
        .unwrap();

    let len = bridge.input_write(b"?");
    let len = bridge.set_mode(len);
    assert_eq!(output(&bridge, len), json!({"modes": {}}));
    let len = bridge.input_write(b"https://example.com/train");
    let len = bridge.location_of(len);
    assert_eq!(bridge.output_bytes(len as usize), b"/train");

    let len = bridge.input_write(b"");
    let len = bridge.set_mode(len);
    let reply = output(&bridge, len);
    assert_eq!(reply["modes"], json!({}));
    assert_eq!(reply["error"], Json::Null);
    let ops = reply["ops"].as_array().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], "activity");
    assert_eq!(ops[0]["request"]["command"], "startActivity");
    assert!(bridge.request_active(ops[0]["ticket"].as_u64().unwrap()));
}

#[test]
fn live_batches_flush_presenter_and_executor_work() {
    struct Work(std::sync::mpsc::Sender<()>);
    impl DataSource for Work {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::Later(exact_runner::Request::continuation(1)))
        }
        fn continuation(&mut self, _: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
            let ran = self.0.clone();
            Some(Box::new(move || {
                ran.send(()).unwrap();
                Outcome::Storage(vec![])
            }))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            _: Outcome,
        ) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::record(vec![Value::str("saved")])))
        }
    }
    let plan = contract::compile(
        r#"shape ID
  id: string
shape State
  title: string
component App
  mutation sent as shape ID
  mutation saved as shape ID
  action start
    send sent = startActivity("train", State(title="Train"))
    send saved = write()
  view
    text "Train"
"#,
    )
    .unwrap();
    let tokens = exact_runner::design::tokens(&plan);
    let design = exact_runner::design::digest(&tokens);
    let snapshot = exact_runner::design::snapshot(&exact_runner::design::shape(&plan), &tokens);
    let mut batch = snapshot.clone();
    batch.base = Some(design.clone());
    let mut stale = batch.clone();
    stale.base = Some("stale".into());
    let (ran, received) = std::sync::mpsc::channel();
    let mut bridge = Bridge::new();
    bridge.link_activities();
    bridge.boot(&plan.encode(), Work(ran), Hooks::none(), 390., 844.);
    for (frame, status) in [(batch, "applied"), (snapshot, "applied"), (stale, "stale")] {
        bridge
            .host
            .as_mut()
            .unwrap()
            .runner_mut()
            .act("start", vec![])
            .unwrap();
        let len = bridge.input_write(b"{}");
        let len = bridge.design_rows(len);
        let invalid: Json = serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
        assert_eq!(invalid["error"], "live: not a tamber batch or snapshot");
        assert!(received.try_recv().is_err());

        let len = bridge.input_write(frame.json().as_bytes());
        let len = bridge.design_rows(len);
        let reply: Json = serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
        assert_eq!(reply["live"], status);
        assert_eq!(reply["unsupported"], false);
        assert_eq!(reply["design"], design);
        let ops = reply["ops"].as_array().unwrap();
        let activity = ops.iter().find(|op| op["op"] == "activity");
        let work = received.recv_timeout(std::time::Duration::from_secs(5));
        assert!(
            activity.is_some() && work.is_ok(),
            "{reply}; executor: {work:?}"
        );
        let activity = activity.unwrap();
        assert_eq!(activity["request"]["command"], "startActivity");
        assert!(bridge.request_active(activity["ticket"].as_u64().unwrap()));

        let len = bridge.pump(0.);
        let reply: Json = serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
        assert!(reply["ops"]
            .as_array()
            .unwrap()
            .iter()
            .all(|op| op["op"] != "activity"));
    }
}

#[test]
fn a_clock_only_change_refuses_a_candidate_that_would_rewind_time() {
    let plan = contract::compile("component App\n  state value = 0\n  view\n    text `${value}`\n")
        .unwrap()
        .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Dot, Hooks::none(), 390., 844.);
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), Dot, Hooks::none(), 390., 844.);
    let before = bridge.host.as_ref().unwrap().carry();
    bridge.advance(before.now_ms + 1000., 0);
    let latest = bridge.host.as_ref().unwrap().carry();
    assert_eq!(before.slots, latest.slots, "only time changed");
    assert_eq!(before.store, latest.store);
    assert_eq!(before.resources, latest.resources);
    assert_eq!(latest.now_ms, before.now_ms + 1000.);
    assert_eq!(
        bridge.prepared.as_ref().unwrap().host.carry().now_ms,
        before.now_ms,
        "accepting this host would replace the live clock with its earlier value"
    );
    let len = bridge.check_prepared();
    assert_eq!(output(&bridge, len)["pending"], true);
    assert!(bridge.prepared.is_none());
    assert_eq!(bridge.host.as_ref().unwrap().carry().now_ms, latest.now_ms);
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), Dot, Hooks::none(), 390., 844.);
    let len = bridge.commit_plan();
    assert_ne!(output(&bridge, len)["pending"], true);
    assert_eq!(bridge.host.as_ref().unwrap().carry().now_ms, latest.now_ms);
}

#[test]
fn a_pick_after_preparation_refuses_a_candidate_that_would_reuse_its_path() {
    let plan = contract::compile("component App\n  view\n    text \"Pick\"\n")
        .unwrap()
        .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Dot, Hooks::none(), 390., 844.);
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), Dot, Hooks::none(), 390., 844.);
    let path = bridge
        .host
        .as_mut()
        .unwrap()
        .runner_mut()
        .picked_path("photo.jpg");
    let len = bridge.check_prepared();
    assert_eq!(output(&bridge, len)["pending"], true);
    assert!(bridge.prepared.is_none());
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), Dot, Hooks::none(), 390., 844.);
    let len = bridge.commit_plan();
    assert_ne!(output(&bridge, len)["pending"], true);
    assert_ne!(
        bridge
            .host
            .as_mut()
            .unwrap()
            .runner_mut()
            .picked_path("photo.jpg"),
        path
    );
}
