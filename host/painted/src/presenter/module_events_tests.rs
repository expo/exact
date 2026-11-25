use super::*;

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    p.host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id(name)[0])
        .unwrap()
        .id
}

#[test]
fn iframe_load_commits_for_the_current_frame_allocation() {
    let source = r#"component App
  state loaded = false
  state shown = true
  action finish
    loaded = true
  action remove
    shown = false
  task retire mount
    after(1, remove)
  view
    column
      if shown
        iframe src="https://example.com/" width=300 height=200 load=finish testId="frame"
      iframe src="https://example.com/" width=300 height=200 testId="unhandled"
      image "" width=10 height=10 load=finish testId="image"
      text (loaded ? "loaded" : "waiting") testId="status"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = id(&p, "frame");
    let key = p.host.kernel().node(frame).unwrap().key;
    let session = p.brush.native_session;
    let status = id(&p, "status");
    let unhandled = id(&p, "unhandled");
    let unhandled_key = p.host.kernel().node(unhandled).unwrap().key;
    let image = id(&p, "image");
    let image_key = p.host.kernel().node(image).unwrap().key;
    let status_text = |p: &Presenter<()>| {
        p.host
            .kernel()
            .node(status)
            .unwrap()
            .props
            .str(PropId::Text)
            .unwrap()
            .to_owned()
    };
    assert!(!p.reader_frame_loaded(unhandled, unhandled_key, session));
    assert!(!p.reader_frame_loaded(image, image_key, session));
    assert!(!p.reader_frame_loaded(frame, unhandled_key, session));
    let stale = exact_kernel::NodeKey {
        generation: key.generation.wrapping_add(1),
        ..key
    };
    assert!(!p.reader_frame_loaded(frame, stale, session));
    assert!(!p.reader_frame_loaded(frame, key, session.wrapping_add(1)));
    assert!(!p.reader_module_event(frame, key, session, "load", String::new(), 0.));
    assert_eq!(status_text(&p), "waiting");
    assert!(p.reader_frame_loaded(frame, key, session));
    assert_eq!(status_text(&p), "loaded");
    assert!(p.host.advance(2.).is_none());
    assert!(!p
        .host
        .runner()
        .journal()
        .any(|line| line.contains("NoHandler")));
    assert!(p.host.kernel().node(frame).is_none());
}

#[test]
fn native_module_events_commit_only_for_the_view_allocation_that_emitted_them() {
    let source = r#"component App
  state selection = "initial"
  state visible = true
  state enabled = true
  state shown = true
  action choose(value: string)
    selection = value
  action remove
    visible = false
  action disable
    enabled = false
  action toggle
    shown = not shown
  view
    column width=400 height=400
      text selection testId="selection" height=24
      button "Remove" press=remove testId="remove" height=32
      button "Disable" press=disable testId="disable" height=32
      button "Toggle" press=toggle testId="toggle" height=32
      if visible
        column visibility=(shown ? "visible" : "hidden")
          native-menu change=choose disabled=(not enabled) width=120 height=40 testId="menu"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let menu = id(&p, "menu");
    let key = p.host.kernel().node(menu).unwrap().key;
    let session = p.brush.native_session;
    let selection = |p: &Presenter<()>| {
        p.host
            .kernel()
            .node(id(p, "selection"))
            .unwrap()
            .props
            .str(PropId::Text)
            .unwrap()
            .to_string()
    };
    assert!(!p.reader_module_event(
        menu,
        exact_kernel::NodeKey {
            generation: key.generation.wrapping_add(1),
            ..key
        },
        session,
        "change",
        "stale".into(),
        1.
    ));
    assert!(!p.reader_module_event(
        menu,
        exact_kernel::NodeKey {
            index: key.index.wrapping_add(1),
            ..key
        },
        session,
        "change",
        "other allocation".into(),
        1.
    ));
    assert!(!p.reader_module_event(menu, key, session, "unsupported", "invalid".into(), 2.));
    for kind in ["press", "input", "load", "message"] {
        assert!(
            !p.reader_module_event(menu, key, session, kind, String::new(), 2.),
            "{kind}"
        );
    }
    assert!(p.host.advance(2.).is_none());
    assert!(!p
        .host
        .runner()
        .journal()
        .any(|line| line.contains("NoHandler")));
    assert!(p.reader_module_event(menu, key, session, "focus", String::new(), 2.));
    assert_eq!(p.focus, Some(menu));
    let other = p.host.kernel().node(id(&p, "toggle")).unwrap().key;
    p.host.flights.focus = Some(other);
    assert!(p.reader_module_event(menu, key, session, "focus", String::new(), 2.));
    assert!(p.host.flights.focus.is_none());
    assert!(!p.reader_module_event(
        menu,
        key,
        session.wrapping_add(1),
        "blur",
        String::new(),
        2.
    ));
    assert_eq!(p.focus, Some(menu));
    p.host.flights.focus = Some(key);
    assert!(p.reader_module_event(menu, key, session, "blur", String::new(), 2.));
    assert_eq!(p.focus, None);
    assert_eq!(selection(&p), "initial");
    assert!(p.reader_module_event(menu, key, session, "change", "week".into(), 3.));
    assert_eq!(selection(&p), "week");
    let replacement = source.replace("testId=\"menu\"", "testId=\"replacement\"");
    assert!(
        p.reload(&crate::test_plan(&replacement), ()).unwrap()
            == crate::presenter::ReloadResult::Applied(None)
    );
    assert_eq!(id(&p, "replacement"), menu);
    assert_eq!(p.host.kernel().node(menu).unwrap().key, key);
    assert!(!p.reader_module_event(menu, key, session, "change", "old popup".into(), 3.));
    let session = p.brush.native_session;
    assert!(p.reader_module_event(menu, key, session, "change", "week".into(), 3.));
    assert_eq!(selection(&p), "week");
    let remove = id(&p, "remove");
    let other_key = p.host.kernel().node(remove).unwrap().key;
    assert!(!p.reader_module_event(remove, other_key, session, "press", String::new(), 4.));
    // A popup opened before its control was hidden or disabled sends nothing after.
    let toggle = id(&p, "toggle");
    assert!(p.host.dispatch_at(toggle, Event::Press, 4.).is_none());
    assert!(p.after_commit().is_none());
    assert!(!p.reader_module_event(menu, key, session, "focus", String::new(), 4.));
    assert!(!p.reader_module_event(menu, key, session, "change", "hidden".into(), 4.));
    assert!(p.host.dispatch_at(toggle, Event::Press, 4.).is_none());
    assert!(p.after_commit().is_none());
    assert!(p
        .host
        .dispatch_at(id(&p, "disable"), Event::Press, 5.)
        .is_none());
    assert!(p.after_commit().is_none());
    assert_eq!(p.host.kernel().node(menu).unwrap().key, key);
    assert!(!p.reader_module_event(menu, key, session, "focus", String::new(), 4.));
    assert!(!p.reader_module_event(menu, key, session, "change", "disabled".into(), 6.));
    assert_eq!(selection(&p), "week");
    assert!(p.host.dispatch_at(remove, Event::Press, 7.).is_none());
    assert!(p.after_commit().is_none());
    assert!(!p.reader_module_event(menu, key, session, "change", "retired".into(), 8.));
    assert_eq!(selection(&p), "week");
}

#[test]
fn handled_native_events_deliver_the_payload_and_focus_transitions_once() {
    let source = r#"component App
  state heard = ""
  action record(s: string)
    heard = heard + s
  view
    column width=300 height=300
      native-menu testId="guest" width=80 height=40 load=record("L") message=record focus=record("F") blur=record("B")
      column role="menu" id="popup" popover="auto"
        button "Dismiss" press=record("D") popovertarget="popup" popovertargetaction="hide"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none());
    p.serve_windows(Box::new(crate::presenter::drag_out_tests::Blank));
    let guest = id(&p, "guest");
    let key = p.host.kernel().node(guest).unwrap().key;
    let session = p.brush.native_session;
    for (kind, payload) in [
        ("load", ""),
        ("message", "M"),
        ("focus", ""),
        ("focus", ""),
        ("blur", ""),
    ] {
        assert!(p.reader_module_event(guest, key, session, kind, payload.into(), 1.));
    }
    assert_eq!(
        p.host
            .runner()
            .slot("heard")
            .and_then(exact_runner::Value::as_str),
        Some("LMFB")
    );
    p.popover_command("showPopover", &[exact_plan::Value::str("popup")]);
    assert!(p.top_open());
    assert!(!p.reader_module_event(guest, key, session, "focus", String::new(), 2.));
    assert_ne!(p.focus, Some(guest));
}
