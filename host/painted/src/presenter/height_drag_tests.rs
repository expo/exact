use super::*;
use exact_kernel::Dimension;
use exact_runner::{DataError, Value};
#[derive(Default)]
struct Rows;
impl DataSource for Rows {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::list(
            (0..25_000).map(|n| Value::Number(n as f64)).collect(),
        ))
    }
}
const APP: &str = r#"component App
  resource rows = rows() as shape number[]
  state target = 180
  state count = 0
  state seen = 0
  state velocity = 0
  state disabled = false
  state showing = true
  state draft = ""
  action edit(value)
    draft = value
  action release(h: number, v: number)
    count = count + 1
    seen = h
    velocity = v
    target = h < 270 ? 180 : h < 500 ? 360 : 640
  action disable
    disabled = true
  action hide
    showing = false
  view
    box width=400 height=500
      button press=disable testId="disable"
        text "disable"
      button press=hide testId="hide"
        text "hide"
      input value=draft input=edit testId="input"
      if showing
        column id="panel" testId="panel" position="absolute" bottom=0 width=400 height=target max-height="100%" box-sizing="border-box" transition="height -exact-spring(300,30,1)"
          box testId="handle" heightDragFor="panel" heightrelease=release height=40 touch-action="none" disabled=disabled
            text "drag header"
          list virtualized=true scrollFollowEnd=true flex=1 min-height=0 width="100%" testId="port"
            for x in rows key=x
              text `row ${x}` height=24
      text `${count}` testId="count"
      text `${seen}` testId="seen"
      text `${velocity}` testId="velocity"
"#;
fn boot(source: &str) -> Presenter<Rows> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        Rows,
        (400., 500.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    for _ in 0..10 {
        assert!(p.pump(0.).is_none());
        let _ = p.frame();
    }
    p
}
fn id(p: &Presenter<Rows>, name: &str) -> ViewId {
    p.host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id(name)[0])
        .unwrap()
        .id
}
fn text<'a>(p: &'a Presenter<Rows>, name: &str) -> &'a str {
    p.host
        .kernel()
        .node(id(p, name))
        .unwrap()
        .props
        .str(PropId::Text)
        .unwrap()
}
fn panel_height(p: &Presenter<Rows>) -> f32 {
    p.host.kernel().node(id(p, "panel")).unwrap().frame.height
}
fn recognize(p: &mut Presenter<Rows>) {
    assert!(p.pointer_down(200., 340., 0.).unwrap());
    assert!(p.pointer_move(200., 330., 10.).unwrap());
    assert_eq!(
        panel_height(p),
        180.,
        "recognition catches with zero displacement"
    );
}
#[test]
fn header_drag_final_sample_typed_action_release_and_pin_order() {
    let mut p = boot(APP);
    recognize(&mut p);
    let token = p
        .contact
        .as_ref()
        .unwrap()
        .hold
        .as_ref()
        .unwrap()
        .motion()
        .token;
    assert_eq!(p.collection_interaction(), Some(id(&p, "handle")));
    // Up itself is the final 120px rise; no earlier move can supply it.
    assert!(p.pointer_up(200., 210., 40.).unwrap());
    assert_eq!(text(&p, "count"), "1");
    assert_eq!(text(&p, "seen"), "300");
    assert_eq!(panel_height(&p), 300.);
    assert_eq!(
        p.host.kernel().node(id(&p, "panel")).unwrap().style.height,
        Dimension::Points(360.)
    );
    assert!(!p.host.has_hold(token));
    assert!(p.collection_interaction().is_none());
    assert!(!p.pointer_up(200., 100., 50.).unwrap());
    assert_eq!(text(&p, "count"), "1");
    p.tick(10_000.);
    let _ = p.frame();
    assert_eq!(panel_height(&p), 360.);
}
#[test]
fn header_is_exclusive_inner_list_wheel_and_typing_keep_working() {
    let mut p = boot(APP);
    assert!(p.pointer_down(200., 420., 0.).unwrap());
    assert!(!p.pointer_move(200., 390., 10.).unwrap());
    assert!(!p.pointer_up(200., 390., 20.).unwrap());
    assert_eq!(panel_height(&p), 180.);
    assert_eq!(text(&p, "count"), "0");
    p.wheel(id(&p, "port"), 0., 10_000_000.).unwrap();
    for _ in 0..10 {
        p.pump(p.host.now());
        let _ = p.frame();
    }
    let snapshot = p.host.collections().pop().unwrap();
    assert_eq!(snapshot.rows.last().unwrap().index, 24_999);
    assert!(snapshot.rows.len() < 64);
    p.pointer_down(200., 340., 30.).unwrap();
    p.pointer_move(200., 330., 40.).unwrap();
    p.pointer_move(200., 210., 50.).unwrap();
    p.type_text(id(&p, "input"), "still typing").unwrap();
    assert_eq!(panel_height(&p), 300.);
    p.wheel(id(&p, "port"), 0., -100.).unwrap();
    assert!(p.contact.is_none());
    assert!(p.collection_interaction().is_none());
    assert!(!p.pointer_up(200., 100., 60.).unwrap());
    assert_eq!(text(&p, "count"), "0");
}
#[test]
fn constrained_height_plateau_has_zero_displayed_release_velocity() {
    let mut p = boot(APP);
    recognize(&mut p);
    p.pointer_move(200., -300., 20.).unwrap();
    assert_eq!(panel_height(&p), 500.);
    p.pointer_move(200., -500., 130.).unwrap();
    p.pointer_move(200., -700., 160.).unwrap();
    p.pointer_up(200., -900., 190.).unwrap();
    assert_eq!(text(&p, "seen"), "500");
    assert_eq!(text(&p, "velocity"), "0");
    assert_eq!(
        p.host.kernel().node(id(&p, "panel")).unwrap().style.height,
        Dimension::Points(640.)
    );
    assert_eq!(panel_height(&p), 500.);
}
#[test]
fn cancellation_disable_deletion_and_reload_never_dispatch_late_release() {
    for action in ["cancel", "disable", "hide"] {
        let mut p = boot(APP);
        recognize(&mut p);
        p.pointer_move(200., 210., 20.).unwrap();
        let token = p
            .contact
            .as_ref()
            .unwrap()
            .hold
            .as_ref()
            .unwrap()
            .motion()
            .token;
        match action {
            "cancel" => p.pointer_cancel(30.).unwrap(),
            name => {
                p.tap(id(&p, name)).unwrap();
            }
        }
        assert!(!p.host.has_hold(token), "{action}");
        assert!(p.collection_interaction().is_none(), "{action}");
        assert!(!p.pointer_up(200., 100., 40.).unwrap());
        assert_eq!(text(&p, "count"), "0");
    }
}

/// A replacement during the drag waits for its release, which reaches the
/// runner that saw the press (LLP 1029.000.002 D9).
#[test]
fn a_reload_during_a_drag_waits_for_the_release() {
    let mut p = boot(APP);
    recognize(&mut p);
    p.pointer_move(200., 210., 20.).unwrap();
    let token = p
        .contact
        .as_ref()
        .unwrap()
        .hold
        .as_ref()
        .unwrap()
        .motion()
        .token;
    assert_eq!(
        p.reload(&crate::test_plan(APP), Rows).unwrap(),
        crate::presenter::ReloadResult::Held("pointer")
    );
    assert!(p.host.has_hold(token), "the drag goes on");
    p.pointer_up(200., 100., 40.).unwrap();
    p.run_commands(|| Rows);
    assert_eq!(p.replacement_pending(), None);
    assert!(!p.host.has_hold(token));
}

#[test]
fn agent_contact_phases_drive_presenter_and_refuse_malformed_or_second_down() {
    let mut p = boot(APP);
    let handle = id(&p, "handle");
    let down = format!(r#"{{"op":"tap","id":{handle},"phase":"down","x":200,"y":340}}"#);
    let reply = crate::agent::handle(&mut p, &down);
    assert!(reply.contains(r#""delivery":"presenter""#), "{reply}");
    assert!(!reply.contains(r#""delivery":"platform""#));
    let decoded: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(decoded["at"], serde_json::json!([200., 340.]));
    assert_eq!(decoded["clock"], 0.);

    let before = p.host.now();
    for invalid in [
        down.as_str(),
        r#"{"op":"tap","phase":"move","x":200,"y":"bad","ms":100}"#,
        r#"{"op":"tap","phase":"move","dx":1,"dy":2,"x":3,"y":4}"#,
        r#"{"op":"tap","phase":"hold","ms":-1}"#,
        r#"{"op":"tap","phase":"cancel","ms":1}"#,
        r#"{"op":"tap","phase":"hold","ms":100000000}"#,
    ] {
        assert!(
            crate::agent::handle(&mut p, invalid).contains("error"),
            "{invalid}"
        );
        assert_eq!(p.host.now(), before);
        assert_eq!(p.contact_position(), Some((200., 340.)));
    }
    for (command, point, clock) in [
        (
            r#"{"op":"tap","phase":"move","x":200,"y":330,"ms":10}"#,
            [200., 330.],
            0.,
        ),
        (
            r#"{"op":"tap","phase":"move","dx":0,"dy":-120,"ms":30}"#,
            [200., 210.],
            0.,
        ),
        (
            r#"{"op":"tap","phase":"hold","ms":100}"#,
            [200., 210.],
            100.,
        ),
    ] {
        let reply = crate::agent::handle(&mut p, command);
        assert!(!reply.contains("error"), "{reply}");
        let decoded: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(decoded["at"], serde_json::json!(point));
        assert_eq!(decoded["clock"], clock);
    }
    assert_eq!(panel_height(&p), 300.);
    assert_eq!(text(&p, "count"), "0");
    let reply = crate::agent::handle(&mut p, r#"{"op":"tap","phase":"up"}"#);
    assert!(!reply.contains("error"), "{reply}");
    let decoded: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(decoded["at"], serde_json::json!([200., 210.]));
    assert_eq!(decoded["clock"], 100.);
    assert_eq!(decoded["contact"], false);
    assert_eq!(text(&p, "count"), "1");
    assert_eq!(text(&p, "velocity"), "0");
    assert_eq!(p.host.now(), 100.);
    assert!(p.contact.is_none());
    assert!(p.collection_interaction().is_none());
}

/// The driver's move keeps its own timeline while the clock stands still: the
/// release velocity is the move's, 130 px in 80 ms (LLP 1057.001 §3), and the
/// lift where the finger already was is no sample.
#[test]
fn agent_height_drag_releases_at_the_moves_pace_without_advancing_time() {
    let mut p = boot(APP);
    let handle = id(&p, "handle");
    for command in [
        format!(r#"{{"op":"tap","id":{handle},"phase":"down","x":200,"y":340}}"#),
        r#"{"op":"tap","phase":"move","dx":0,"dy":-130,"ms":80}"#.into(),
        r#"{"op":"tap","phase":"up"}"#.into(),
    ] {
        let reply: serde_json::Value =
            serde_json::from_str(&crate::agent::handle(&mut p, &command)).unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(reply["clock"], 0.);
        assert_eq!(p.host.now(), 0.);
    }
    assert_eq!(text(&p, "count"), "1");
    let velocity: f64 = text(&p, "velocity").parse().unwrap();
    assert!((velocity - 1625.).abs() < 1e-6, "{velocity}");
    assert!(text(&p, "seen").parse::<f64>().unwrap() > 180.);
}

#[test]
#[cfg(unix)]
fn reader_height_release_uses_the_batched_moves_and_the_lifts_input_time() {
    let uptime = || crate::launch_marks::clock(libc::CLOCK_MONOTONIC) * 1000.;
    for (lift, expected) in [(32, 1000.), (160, 0.)] {
        let mut p = boot(APP);
        let mut input = contact::InputClock::default();
        let start = uptime().floor() as i64 - 200;
        for (action, y, at) in [
            (0, 340., 0),
            (2, 330., 10),
            (2, 320., 20),
            (2, 310., 30),
            (1, 310., lift),
        ] {
            let ns = (start + at) * 1_000_000;
            let before = uptime();
            let event = input.android(action == 0, ns, 400.);
            if action == 0 {
                let age = 400. - event;
                assert!((before - start as f64..=uptime() - start as f64).contains(&age));
            }
            p.reader_touch(action, 200., y, 400., event).unwrap();
        }
        assert_eq!(text(&p, "count"), "1");
        let velocity: f64 = text(&p, "velocity").parse().unwrap();
        assert!(
            (velocity - expected).abs() < 1e-6,
            "{velocity} at lift {lift}"
        );
    }
}

#[test]
fn a_live_driver_lifting_a_delayed_reader_move_counts_the_time_since_input() {
    let mut p = boot(APP);
    p.set_live_clock(Some(std::rc::Rc::new(|| 400.)));
    for (action, y, at) in [
        (0, 340., 0.),
        (2, 330., 10.),
        (2, 320., 20.),
        (2, 310., 30.),
    ] {
        p.reader_touch(action, 200., y, 400., at).unwrap();
    }
    assert_eq!(p.pump(400.), None);
    assert_eq!(p.contact_event_ms(None), Some(30.));
    let reply = crate::agent::handle_for_window(&mut p, r#"{"op":"tap","phase":"up"}"#);
    assert!(!reply.contains("error"), "{reply}");
    assert_eq!(text(&p, "count"), "1");
    assert_eq!(text(&p, "velocity"), "0");
}

/// A pause before the lift is sampled: the same drag held 120 ms releases at
/// rest (LLP 1057.001 §6).
#[test]
fn agent_height_drag_paused_before_its_lift_releases_at_rest() {
    let mut p = boot(APP);
    let handle = id(&p, "handle");
    for command in [
        format!(r#"{{"op":"tap","id":{handle},"phase":"down","x":200,"y":340}}"#),
        r#"{"op":"tap","phase":"move","dx":0,"dy":-130,"ms":80}"#.into(),
        r#"{"op":"tap","phase":"hold","ms":120}"#.into(),
        r#"{"op":"tap","phase":"up"}"#.into(),
    ] {
        let reply: serde_json::Value =
            serde_json::from_str(&crate::agent::handle(&mut p, &command)).unwrap();
        assert!(reply.get("error").is_none(), "{reply}");
    }
    assert_eq!(text(&p, "count"), "1");
    assert_eq!(text(&p, "velocity"), "0");
}

#[test]
fn agent_cancel_retires_contact_without_release_action() {
    let mut p = boot(APP);
    recognize(&mut p);
    p.pointer_move(200., 210., 20.).unwrap();
    // Align the runner with the preceding live pointer samples.
    assert!(p.clock(20.).1.is_none());
    let reply = crate::agent::handle(&mut p, r#"{"op":"tap","phase":"cancel"}"#);
    assert!(!reply.contains("error"), "{reply}");
    let decoded: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(decoded["at"], serde_json::json!([200., 210.]));
    assert_eq!(decoded["clock"], 20.);
    assert_eq!(decoded["contact"], false);
    assert!(p.contact.is_none());
    assert!(p.collection_interaction().is_none());
    assert_eq!(text(&p, "count"), "0");
}

#[test]
fn agent_aborted_move_reports_last_delivered_step_not_requested_endpoint() {
    let mut p = boot(APP);
    p.pointer_down(200., 340., 0.).unwrap();
    // Ten planned samples; the first horizontal sample declines this vertical handle.
    let reply = crate::agent::handle(
        &mut p,
        r#"{"op":"tap","phase":"move","x":300,"y":340,"ms":160}"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(decoded["at"], serde_json::json!([210., 340.]));
    assert_eq!(decoded["clock"], 0.);
    assert_eq!(decoded["contact"], false);
    assert_eq!(p.host.now(), 0.);
    assert_eq!(text(&p, "count"), "0");
}

#[test]
fn agent_late_up_and_cancel_after_accepted_action_retirement_allow_new_down() {
    for phase in ["up", "cancel"] {
        let source = APP.replace("disabled = true", "disabled = not disabled");
        let mut p = boot(&source);
        let down = format!(r#"{{"op":"tap","phase":"down","id":{}}}"#, id(&p, "handle"));
        let reply = crate::agent::handle(&mut p, &down);
        assert!(!reply.contains("error"), "{reply}");
        let reply = crate::agent::handle(
            &mut p,
            r#"{"op":"tap","phase":"move","dx":0,"dy":-10,"ms":10}"#,
        );
        assert!(!reply.contains("error"), "{reply}");
        let disable = format!(r#"{{"op":"tap","id":{}}}"#, id(&p, "disable"));
        let reply = crate::agent::handle(&mut p, &disable);
        assert!(!reply.contains("error"), "{reply}");
        assert!(p.contact.is_none());
        let before = p.host.now();
        let late = format!(r#"{{"op":"tap","phase":"{phase}"}}"#);
        for _ in 0..2 {
            let reply: serde_json::Value =
                serde_json::from_str(&crate::agent::handle(&mut p, &late)).unwrap();
            assert_eq!(reply["contact"], false, "late {phase}: {reply}");
            assert!(reply["at"].is_null());
            assert!(reply.get("error").is_none(), "{reply}");
            assert_eq!(p.host.now(), before);
            assert_eq!(text(&p, "count"), "0");
        }
        // The other input re-enables the handle. A new contact must be usable.
        let reply = crate::agent::handle(&mut p, &disable);
        assert!(!reply.contains("error"), "{reply}");
        let reply: serde_json::Value =
            serde_json::from_str(&crate::agent::handle(&mut p, &down)).unwrap();
        assert_eq!(reply["contact"], true, "{reply}");
        assert!(p.contact.is_some());
        p.pointer_cancel(p.host.now()).unwrap();
    }
}

const READING: &str = include_str!("../../../apple/tests/fixtures/sheet-scroll.contract");

fn release_state(p: &mut Presenter<Rows>) -> (f64, f64) {
    let state: serde_json::Value =
        serde_json::from_str(&crate::agent::handle(p, r#"{"op":"state"}"#)).unwrap();
    let slots = &state["slots"];
    (
        slots["releases"].as_f64().unwrap(),
        slots["speed"].as_f64().unwrap(),
    )
}

fn sheet_state(p: &Presenter<Rows>) -> (f32, f32) {
    let height = p.host.kernel().node(id(p, "sheet")).unwrap().frame.height;
    (height, p.scroll_of(id(p, "port")).1)
}

#[test]
fn one_contact_shares_growth_scroll_and_reversals() {
    for source in [READING, &READING.replace("list testId=", "scroll testId=")] {
        let mut p = boot(source);
        p.reader_under(180., 460., 1.);
        p.pointer_down(180., 460., 0.).unwrap();
        p.pointer_move(180., 445., 10.).unwrap();
        for (t, y, expected) in [
            (30., 365., (400., 0.)),
            (70., 225., (480., 60.)),
            (85., 260., (480., 25.)),
            (115., 350., (415., 0.)),
            (145., 390., (375., 0.)),
            (185., 230., (480., 55.)),
        ] {
            p.pointer_move(180., y, t).unwrap();
            assert_eq!(sheet_state(&p), expected, "move to {y}");
        }
        p.reader_touch(1, 180., 220., 190., 190.).unwrap();
        assert_eq!(sheet_state(&p), (480., 65.));
        assert_eq!(release_state(&mut p), (1., 0.));
        let (port, velocity, _) = p.take_height_scroll_release().unwrap();
        assert_eq!(port, id(&p, "port"));
        assert!(velocity > 0.);
    }
}

#[test]
fn a_short_list_hands_downward_travel_to_the_sheet_immediately() {
    let mut p = boot(&READING.replace("range(24)", "range(2)"));
    p.pointer_down(180., 340., 0.).unwrap();
    p.pointer_move(180., 350., 10.).unwrap();
    p.pointer_move(180., 390., 30.).unwrap();
    assert_eq!(sheet_state(&p), (280., 0.));
    p.pointer_move(180., 360., 50.).unwrap();
    assert_eq!(sheet_state(&p), (310., 0.));
}

#[test]
fn downward_release_belongs_to_the_last_box_that_moved() {
    for (distance, expected) in [(45., (480., 35.)), (130., (430., 0.))] {
        let mut p = boot(&READING.replace("state px = 320", "state px = 480"));
        let port = id(&p, "port");
        p.scroll_to(port, (0., 80.)).unwrap();
        p.frame();
        p.reader_under(180., 230., 1.);
        p.pointer_down(180., 230., 0.).unwrap();
        p.pointer_move(180., 245., 10.).unwrap();
        p.pointer_move(180., 245. + distance * 0.5, 30.).unwrap();
        p.reader_touch(1, 180., 245. + distance, 50., 50.).unwrap();
        assert_eq!(sheet_state(&p), expected);
        let (releases, sheet_velocity) = release_state(&mut p);
        if expected.1 == 0. {
            assert_eq!(releases, 1.);
            assert!(sheet_velocity < 0.);
            assert!(p.take_height_scroll_release().is_none());
        } else {
            assert_eq!(releases, 0.);
            let (coasting, velocity, _) = p.take_height_scroll_release().unwrap();
            assert_eq!(coasting, port);
            assert!(velocity < 0.);
        }
    }
}

#[test]
fn a_list_coasts_on_the_headless_presenters_clock() {
    let source = READING.replace("state px = 320", "state px = 480").replace(
        "  view", "  state ends = 0\n  action lifted(e: ScrollDragEvent)\n    speed = e.velocityY\n  action stopped\n    ends = ends + 1\n  view",
    ).replace("list testId=", "list scrolldragend=lifted scrollend=stopped testId=");
    let mut p = boot(&source);
    p.reader_under(180., 420., 1.);
    p.pointer_down(180., 420., 0.).unwrap();
    p.pointer_move(180., 410., 10.).unwrap();
    p.pointer_move(180., 380., 35.).unwrap();
    p.pointer_up(180., 350., 60.).unwrap();
    let (height, offset) = sheet_state(&p);
    assert_eq!((height, offset), (480., 60.));
    p.tick(110.);
    let (height, coast) = sheet_state(&p);
    assert_eq!(height, 480.);
    assert!(coast > offset);
    let (releases, speed) = release_state(&mut p);
    assert_eq!(releases, 0.);
    assert!(speed > 0.);
    let ends = |p: &mut Presenter<Rows>| {
        let state: serde_json::Value =
            serde_json::from_str(&crate::agent::handle(p, r#"{"op":"state"}"#)).unwrap();
        state["slots"]["ends"].as_f64().unwrap()
    };
    assert_eq!(ends(&mut p), 0.);
    p.tick(4_000.);
    assert_eq!(p.advance(4_000.), None);
    assert_eq!(ends(&mut p), 1.);
}

#[test]
fn a_scaled_list_consumes_its_share_in_local_points() {
    let mut p = boot(&READING.replace(
        "list testId=",
        r#"list transform="scaleY(2)" transform-origin="top left" testId="#,
    ));
    p.pointer_down(180., 460., 0.).unwrap();
    p.pointer_move(180., 445., 10.).unwrap();
    p.pointer_move(180., 225., 50.).unwrap();
    assert_eq!(sheet_state(&p), (480., 30.));
    p.pointer_move(180., 325., 70.).unwrap();
    assert_eq!(sheet_state(&p), (440., 0.));
}

#[test]
fn an_inner_vertical_port_keeps_its_pressable_row() {
    let source = READING.replace("overflow-x=\"scroll\" overflow-y=\"hidden\"", "")
        .replace("text \"First row\" width=900 height=50", "button testId=\"inner-row\" press=clicked height=300\n                  text \"Nested\"")
        .replace("  view", "  action clicked\n    releases = releases + 1\n  view");
    let p = boot(&source);
    let row = p.host.kernel().node(id(&p, "inner-row")).unwrap();
    assert!(p.height_candidate(row.key).is_none());
}

#[test]
fn the_next_reader_contact_closes_an_undelivered_list_release() {
    let mut p = boot(&READING.replace("state px = 320", "state px = 480"));
    let port = id(&p, "port");
    p.reader_under(180., 420., 1.);
    for (action, y, time) in [
        (0, 420., 0.),
        (2, 410., 10.),
        (2, 380., 35.),
        (1, 350., 60.),
    ] {
        p.reader_touch(action, 180., y, time, time).unwrap();
    }
    assert!(p.take_height_scroll_release().is_some());
    assert!(p.scroll_held(port));
    p.reader_touch(0, 180., 350., 80., 80.).unwrap();
    assert!(!p.scroll_held(port));
}

#[test]
fn a_list_only_contact_leaves_a_settling_sheet_to_its_spring() {
    let mut p = boot(READING);
    p.pointer_down(180., 450., 0.).unwrap();
    p.pointer_move(180., 440., 10.).unwrap();
    p.pointer_move(180., 385., 30.).unwrap();
    p.pointer_up(180., 375., 35.).unwrap();
    let port = id(&p, "port");
    p.scroll_to(port, (0., 100.));
    p.tick(60.);
    let before = sheet_state(&p).0;
    p.pointer_down(180., 350., 60.).unwrap();
    p.pointer_move(180., 360., 70.).unwrap();
    p.pointer_move(180., 385., 90.).unwrap();
    assert_eq!(sheet_state(&p).1, 75.);
    p.tick(130.);
    assert!(sheet_state(&p).0 > before);
    p.pointer_up(180., 385., 140.).unwrap();
    assert_eq!(release_state(&mut p).0, 1.);
}

#[test]
fn a_lift_that_crosses_the_top_gives_its_velocity_to_the_list() {
    let mut p = boot(&READING.replace("state px = 320", "state px = 460"));
    p.reader_under(180., 420., 1.);
    p.reader_touch(0, 180., 420., 0., 0.).unwrap();
    p.reader_touch(2, 180., 410., 10., 10.).unwrap();
    p.reader_touch(2, 180., 400., 20., 20.).unwrap();
    p.reader_touch(1, 180., 389., 30., 30.).unwrap();
    assert_eq!(sheet_state(&p), (480., 1.));
    assert_eq!(release_state(&mut p), (1., 0.));
    assert!(p.take_height_scroll_release().unwrap().1 > 0.);
}

#[test]
fn a_pressable_row_taps_but_a_drag_cancels_its_press() {
    let source = READING
        .replace(
            "  view",
            "  action pressed\n    released = released + 1\n  view",
        )
        .replace(
            "text `Row ${name}` height=50",
            "button testId=`row-${name}` press=pressed height=50\n                text `Row ${name}`",
        );
    let mut p = boot(&source);
    let pressed = |p: &mut Presenter<Rows>| {
        let state: serde_json::Value =
            serde_json::from_str(&crate::agent::handle(p, r#"{"op":"state"}"#)).unwrap();
        state["slots"]["released"].as_f64().unwrap()
    };
    p.set_pointer_kind(PointerKind::Touch);
    let row = id(&p, "row-2");
    let (left, top, width, height) = p.rect_of(row).unwrap();
    let (x, y) = (left + width / 2., top + height / 2.);
    p.pointer_down(x, y, 0.).unwrap();
    p.pointer_up(x, y, 10.).unwrap();
    assert_eq!(pressed(&mut p), 1.);
    p.pointer_down(x, y, 20.).unwrap();
    p.pointer_move(x, y - 20., 30.).unwrap();
    p.pointer_move(x, y - 60., 50.).unwrap();
    assert_eq!(sheet_state(&p), (360., 0.));
    assert_eq!(pressed(&mut p), 1.);
}

#[test]
fn a_fractional_offset_gives_the_rest_of_the_same_move_to_the_sheet() {
    let mut p = boot(&READING.replace("state px = 320", "state px = 480"));
    let port = id(&p, "port");
    p.scroll_to(port, (0., 0.5));
    p.pointer_down(180., 320., 0.).unwrap();
    p.pointer_move(180., 330., 10.).unwrap();
    p.pointer_move(180., 340., 20.).unwrap();
    assert_eq!(sheet_state(&p), (470.5, 0.));
}

#[test]
fn removing_the_list_at_release_retires_its_drag_and_velocity() {
    let (head, list) = READING.split_once("        list testId=").unwrap();
    let list = format!("        list testId={list}")
        .lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!("{head}        if showing\n{list}\n")
        .replace("  state px", "  state showing = true\n  state px")
        .replace(
            "releases = releases + 1",
            "releases = releases + 1\n    showing = false",
        );
    let mut p = boot(&source);
    let port = id(&p, "port");
    p.reader_under(180., 430., 1.);
    p.reader_touch(0, 180., 430., 0., 0.).unwrap();
    p.reader_touch(2, 180., 410., 10., 10.).unwrap();
    p.reader_touch(2, 180., 210., 50., 50.).unwrap();
    p.reader_touch(1, 180., 190., 60., 60.).unwrap();
    assert!(p.host.kernel().node(port).is_none());
    assert!(!p.scroll_held(port));
    assert!(p.take_height_scroll_release().is_none());
}

#[test]
fn the_first_list_move_respects_a_scroll_written_by_its_start_handler() {
    let source = READING
        .replace(
            "  view",
            "  state top = 0\n  action jump\n    top = 200\n  view",
        )
        .replace(
            "list testId=",
            "list scrolldragstart=jump scrollTop=top testId=",
        );
    let mut p = boot(&source);
    p.pointer_down(180., 420., 0.).unwrap();
    p.pointer_move(180., 410., 10.).unwrap();
    p.pointer_move(180., 245., 60.).unwrap();
    assert_eq!(sheet_state(&p), (480., 205.));
}
