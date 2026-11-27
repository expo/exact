use super::*;

#[test]
fn reader_access_reemits_whole_tree_when_scale_changes_in_place() {
    let mut p = boot(CONTROLS, (320., 600.), 0);
    let at_2x = told(&p.reader_access(2.0, true).expect("initial statement"));
    assert!(at_2x.whole);
    assert_eq!(p.reader_access(2.0, false), None, "unchanged scale");

    // Fast path (access_key): scale changes without repaint or commit.
    let at_3x = told(
        &p.reader_access(3.0, false)
            .expect("scale change bypasses access_key"),
    );
    assert!(at_3x.whole, "every pixel rect moved");
    let save_2x = named(&at_2x.nodes, "Save").rect;
    let save_3x = named(&at_3x.nodes, "Save").rect;
    for (two, three) in save_2x.into_iter().zip(save_3x) {
        assert!((three - (two / 2.0 * 3.0).round()).abs() <= 1.0);
    }
    assert_eq!(p.reader_access(3.0, false), None, "cached at new scale");

    // Slow path (cached node equality): boxes_serial advances after resize_scaled + frame
    // while logical CSS nodes stay identical.
    assert_eq!(p.resize_scaled(320., 600., 1.5), None);
    let _ = p.frame();
    let at_1_5x = told(
        &p.reader_access(1.5, false)
            .expect("scale change bypasses logical node equality"),
    );
    assert!(at_1_5x.whole);
    let save_1_5x = named(&at_1_5x.nodes, "Save").rect;
    for (two, one_half) in save_2x.into_iter().zip(save_1_5x) {
        assert!((one_half - (two / 2.0 * 1.5).round()).abs() <= 1.0);
    }
    assert_eq!(p.reader_access(1.5, false), None);
}

#[test]
fn visibility_hidden_and_collapse_excluded_while_visible_descendants_and_refs_survive() {
    const SRC: &str = r#"component App
  state level = 40
  state draft = "orig"
  state hideLive = false
  action setLevel(value: number)
    level = value
  action edit(value: string)
    draft = value
  action hide
    hideLive = true
  action noop
    level = level
  view
    column width=320 height=600 align-items="flex-start"
      text id="ref-lbl" visibility="hidden" "Referenced label"
      text id="ref-desc" visibility="hidden" "Referenced hint"
      button press=noop aria-labelledby="ref-lbl" aria-describedby="ref-desc" height=24
        text "Ignored face"
      button press=noop height=24
        text "Save"
        text "Secret" visibility="hidden"
        text "Collapsed" visibility="collapse"
        text
          text " "
          text "Whisper" visibility="hidden"
          text "Now"
        box visibility="hidden" aria-label="Hidden wrapper"
          text "Inside hidden"
          text "Revealed" visibility="visible"
      button press=noop visibility="hidden" height=24
        text "Hidden button"
        text "Shown inside hidden button" visibility="visible"
      box visibility="collapse" height=24
        button press=noop height=24
          text "Collapsed child"
        button press=noop visibility="visible" height=24
          text "Visible inside collapse"
      text visibility="hidden"
        text "Hidden span"
        text "Visible span" visibility="visible"
      button press=noop opacity=0 height=24
        text "Transparent"
      input value=draft input=edit aria-label="Hidden field" visibility="hidden" width=120 height=24
      input type="range" min=0 max=100 step=10 value=level input=setLevel aria-label="Hidden slider" visibility="hidden" width=120 height=24
      button press=hide testId="hide" height=24
        text "Hide live"
      input value=draft input=edit aria-label="Live field" visibility=(hideLive ? "hidden" : "visible") testId="live-field" width=120 height=24
      input type="range" min=0 max=100 step=10 value=level input=setLevel aria-label="Live slider" visibility=(hideLive ? "hidden" : "visible") testId="live-slider" width=120 height=24
"#;
    let mut p = boot(SRC, (320., 600.), 0);
    let shown = statement(&mut p);
    let names: Vec<&str> = shown.nodes.iter().map(|n| n.name.as_str()).collect();

    assert!(names.contains(&"Referenced label"), "{names:?}");
    assert_eq!(
        named(&shown.nodes, "Referenced label").hint,
        "Referenced hint"
    );
    assert!(names.contains(&"Save Now Revealed"), "{names:?}");
    assert!(names.contains(&"Shown inside hidden button"), "{names:?}");
    assert!(names.contains(&"Visible inside collapse"), "{names:?}");
    assert!(names.contains(&"Visible span"), "{names:?}");
    assert!(names.contains(&"Transparent"), "opacity:0 stays exposed");

    for omitted in [
        "Referenced hint",
        "Hidden button",
        "Collapsed child",
        "Hidden span",
        "Hidden field",
        "Hidden slider",
        "Secret",
        "Collapsed",
        "Whisper",
        "Inside hidden",
    ] {
        assert!(
            !names.contains(&omitted),
            "{omitted:?} must not be exposed: {names:?}"
        );
    }

    // Stale AT actions after a commit hides controls via visibility:hidden must not mutate them.
    let live_field = named(&shown.nodes, "Live field");
    let live_slider = named(&shown.nodes, "Live slider");
    tap(&mut p, "hide");

    assert_eq!(p.access_view(live_field.id), None);
    assert!(p.type_text(live_field.view, "hacked").is_err());
    assert_eq!(p.field_text(live_field.view), "orig");

    assert!(!p.access_act(live_slider.id, act::SET, 90.0, 0.0));
    assert!(!p.access_act(live_slider.id, act::FORWARD, 0.0, 0.0));
    assert!(!p.access_act(live_slider.id, act::BACKWARD, 0.0, 0.0));
    assert_eq!(p.host.runner().slot("level"), Some(&Value::Number(40.0)));
}
