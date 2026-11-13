use super::*;

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    p.host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id(name)[0])
        .unwrap()
        .id
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
    assert_eq!(selection(&p), "initial");
    assert!(p.reader_module_event(menu, key, session, "change", "week".into(), 3.));
    assert_eq!(selection(&p), "week");
    let replacement = source.replace("testId=\"menu\"", "testId=\"replacement\"");
    assert!(p
        .reload(&crate::test_plan(&replacement), ())
        .unwrap()
        .is_none());
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
    assert!(!p.reader_module_event(menu, key, session, "change", "hidden".into(), 4.));
    assert!(p.host.dispatch_at(toggle, Event::Press, 4.).is_none());
    assert!(p.after_commit().is_none());
    assert!(p
        .host
        .dispatch_at(id(&p, "disable"), Event::Press, 5.)
        .is_none());
    assert!(p.after_commit().is_none());
    assert_eq!(p.host.kernel().node(menu).unwrap().key, key);
    assert!(!p.reader_module_event(menu, key, session, "change", "disabled".into(), 6.));
    assert_eq!(selection(&p), "week");
    assert!(p.host.dispatch_at(remove, Event::Press, 7.).is_none());
    assert!(p.after_commit().is_none());
    assert!(!p.reader_module_event(menu, key, session, "change", "retired".into(), 8.));
    assert_eq!(selection(&p), "week");
}
