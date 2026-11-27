use super::*;

#[test]
fn custom_actions_press_the_named_target() {
    const RANGE: &str = include_str!("../../../../apple/tests/fixtures/range-adjust.contract");
    let mut p = boot(RANGE, (412., 1000.), 0);
    let shown = statement(&mut p);
    let floor = named(&shown.nodes, "Floor");
    let (action, name) = &floor.actions[0];
    assert_eq!(name, "Reset quantity");
    assert_eq!(*action, named(&shown.nodes, "Reset quantity").view);
    assert!(p.access_custom_action(floor.id, *action, 0.));

    settle(&mut p);
    let reset = statement(&mut p);
    assert_eq!(named(&reset.nodes, "Floor").reader_range()[2], 1.);
    assert!(named(&reset.nodes, "Floor").actions.is_empty());
    let others = named(&reset.nodes, "Other controls");
    assert!(others.actions.is_empty());
    assert_eq!(named(&reset.nodes, "Back").parent, others.id);
    assert!(!p.access_custom_action(floor.id, *action, 0.));
}

#[test]
fn keyed_row_actions_press_distinct_live_targets() {
    let mut p = boot(
        r#"component App
  state keys = [44042, 56408]
  state refs = "reset-44042 reset-56408"
  state picked = 0
  action pick(n: number)
    picked = n
  view
    column width=320 height=480
      text `Picked ${picked}` role="status"
      column aria-label="Rows" aria-actions=refs height=24
      list virtualized=true width=320 height=400
        for n in keys key=n
          column height=40
            button `Reset ${n}` id=`reset-${n}` testId=`reset-${n}` press=pick(n)
"#,
        (320., 480.),
        0,
    );
    let shown = statement(&mut p);
    let owner = named(&shown.nodes, "Rows");
    let first = id(&p, "reset-44042");
    let second = id(&p, "reset-56408");
    assert_eq!(
        owner.actions,
        [
            (first, "Reset 44042".into()),
            (second, "Reset 56408".into())
        ]
    );
    for (target, name) in [(second, "Picked 56408"), (first, "Picked 44042")] {
        assert!(p.access_custom_action(owner.id, target, 0.));
        settle(&mut p);
        assert_eq!(named(&statement(&mut p).nodes, name).name, name);
    }
}

#[test]
fn inline_actions_run_control_defaults_and_skip_unavailable_targets() {
    let src = r#"component App
  state on = false
  state pressed = false
  action hit
    pressed = true
  action change(value: bool)
    on = value
  view
    column width=320 height=400
      text
        text "Actions: "
        text "Word" aria-actions="box docs"
      column
        input type="checkbox" id="box" aria-label="Checkbox" checked=on press=hit change=change
      link id="docs" href="https://example.com/docs"
        text "Docs"
"#;
    let mut p = boot(src, (320., 400.), 0);
    let got = statement(&mut p);
    let word = named(&got.nodes, "Word");
    assert_eq!(word.actions.len(), 2);
    assert!(p.access_custom_action(word.id, word.actions[0].0, 0.));
    for name in ["pressed", "on"] {
        assert_eq!(p.host.runner().slot(name), Some(&Value::Bool(true)));
    }
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = opened.clone();
    p.set_url_opener(move |url| {
        seen.borrow_mut().push(url.to_owned());
        Ok(())
    });
    assert!(p.access_custom_action(word.id, word.actions[1].0, 0.));
    assert_eq!(&*opened.borrow(), &["https://example.com/docs"]);
    for attr in ["disabled=true", "visibility=\"hidden\""] {
        let src = src.replace("      column\n", &format!("      column {attr}\n"));
        let mut p = boot(&src, (320., 400.), 0);
        let got = statement(&mut p);
        let word = named(&got.nodes, "Word");
        assert_eq!(word.actions.len(), 1);
        assert_eq!(word.actions[0].1, "Docs");
    }
}

#[test]
fn a_grouped_switch_keeps_its_own_actions() {
    let src = TOGGLES.replace("testId=\"dark\"", "testId=\"dark\" aria-actions=\"reset\"")
        .replace("          text \"Display\"", "          text \"Display\"\n        row\n          button \"Reset\" id=\"reset\" press=flip(true)");
    let mut p = boot(&src, (320., 600.), 0);
    let got = statement(&mut p);
    let dark = named(&got.nodes, "Dark mode");
    assert_eq!(dark.actions[0].1, "Reset");
    assert!(p.access_custom_action(dark.id, dark.actions[0].0, 0.));
    assert_eq!(p.host.runner().slot("dark"), Some(&Value::Bool(true)));
}

#[test]
fn a_muted_child_does_not_announce_its_live_ancestor() {
    let mut p = boot(
        r#"component App
  state message = ""
  action edit
    message = "Muted change"
  view
    column
      button "Edit" press=edit testId="edit"
      column aria-label="Changes" aria-live="assertive"
        column aria-live="off"
          text message
"#,
        (320., 240.),
        0,
    );
    let first = statement(&mut p);
    let group = named(&first.nodes, "Changes").id;
    tap(&mut p, "edit");
    let next = statement(&mut p);
    assert_eq!(named(&next.nodes, "Changes").id, group);
    assert_ne!(named(&next.nodes, "Muted change").flags & flag::LIVE_OFF, 0);
    assert!(next
        .changes
        .iter()
        .all(|(_, what, ..)| what & change::LIVE == 0));
}

#[test]
fn ancestor_and_cyclic_aria_labelledby_and_describedby_terminate() {
    let mut p = boot(
        r#"component App
  state on = false
  action flip(checked: bool)
    on = checked
  action act
    on = !on
  view
    column width=320 height=600
      row id="wifi-row"
        text "Wi-Fi"
        input type="checkbox" role="switch" checked=on change=flip aria-labelledby="wifi-row"
      column id="cycle-a"
        text "Alpha"
        button press=act aria-labelledby="cycle-b"
          text "In A"
      column id="cycle-b"
        text "Beta"
        button press=act aria-labelledby="cycle-a"
          text "In B"
      row id="desc-row"
        text "Connects automatically"
        input type="checkbox" role="switch" checked=on change=flip aria-labelledby="desc-row"
      button "Details" press=act aria-describedby="desc-row"
      text id="prefix" "Price:"
      button id="self-btn" press=act aria-labelledby="prefix self-btn"
        text "Buy"
"#,
        (320., 600.),
        0,
    );
    let shown = statement(&mut p);
    let switches: Vec<_> = shown
        .nodes
        .iter()
        .filter(|n| n.role == role::SWITCH)
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(switches, ["Wi-Fi", "Connects automatically"]);
    assert_eq!(named(&shown.nodes, "Beta In B").role, role::BUTTON);
    assert_eq!(named(&shown.nodes, "Alpha In A").role, role::BUTTON);
    let details = named(&shown.nodes, "Details");
    assert_eq!(details.hint, "Connects automatically");
    assert_eq!(named(&shown.nodes, "Price: Buy").role, role::BUTTON);
}
