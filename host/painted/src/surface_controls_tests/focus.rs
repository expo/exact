use super::*;

#[test]
fn additional_canvas_fingers_restore_pointer_modality_after_a_hardware_key() {
    for target in ["raw", "remove", "outside"] {
        let (mut p, path) = fixture();
        p.brush.touch_mode = true;
        p.set_pointer_kind(crate::presenter::PointerKind::Touch);
        let editor = find(&p, "editor");
        assert!(p.set_focus(Some(editor), 0.).is_none());
        let (x, y, w, h) = p.rect_of(find(&p, "raw")).unwrap();
        p.pointer_down(x + w / 2., y + h / 2., 1.).unwrap();
        assert_eq!(p.contact_canvas(), Some(find(&p, "raw")));
        p.hardware_key("ShiftLeft", "Shift", true, false);
        p.hardware_key("ShiftLeft", "Shift", false, false);
        assert!(!p.brush.pointer_modality);
        let (x, y) = if target == "outside" {
            (150., 400.)
        } else {
            let (x, y, w, h) = p.rect_of(find(&p, target)).unwrap();
            (x + w / 2., y + h / 2.)
        };
        assert!(p.canvas_finger(2, 0, x, y, 2.));
        assert_eq!(p.focus, Some(editor));
        assert_eq!(p.brush.ringless, None, "the editor stays focus-visible");
        let next = find(&p, "remove");
        assert!(p.set_focus(Some(next), 3.).is_none());
        assert_eq!(p.brush.ringless, Some(next), "{target}");
        p.pointer_cancel(4.).unwrap();
        done(p, path);
    }
}

#[test]
fn a_canvas_action_key_runs_its_focus_handler_once_when_it_navigates() {
    let source = r#"routes nav
  home "/"
    detail "/detail"
component Controls
  state count = 0
  action entered
    count = count + 1
    nav = push(nav, "/detail")
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width=100 height=360
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` navigationPresentation="modal"
          if e.name == "home"
            canvas testId="a" width=100 height=100
              button action="jump" testId="button" focus=entered width=100 height=100
          if e.name == "detail"
            header
              text "Detail" role="heading" testId="next"
"#;
    for (code, key) in [("Space", " "), ("Enter", "Enter"), ("NumpadEnter", "Enter")] {
        let (mut p, path) = boot(source, &["a"]);
        assert!(p.pump(p.host.now()).is_none());
        let button = find(&p, "button");
        p.type_key(button, code, key, true, false).unwrap();
        assert_eq!(
            p.host.runner().slot("count"),
            Some(&exact_runner::Value::Number(1.))
        );
        assert_eq!(p.host.kernel().find_by_test_id("next").len(), 1);
        assert_ne!(
            p.focus,
            Some(button),
            "{code}: navigation released the control"
        );
        p.type_key(button, code, key, false, false).unwrap();
        assert_ne!(p.focus, Some(button), "{code}: release");
        done(p, path);
    }
}

#[test]
fn a_canvas_action_press_moves_unringed_focus_to_the_control() {
    for (winui, android) in [(false, false), (true, false), (false, true)] {
        for agent in [false, true] {
            let (mut p, path) = fixture();
            p.brush.winui_focus = winui;
            p.brush.touch_mode = android;
            p.set_pointer_kind(if android {
                crate::presenter::PointerKind::Touch
            } else {
                crate::presenter::PointerKind::Mouse
            });
            let old = find(&p, "remove");
            assert!(p.set_focus(Some(old), 0.).is_none());
            p.brush.ringless = Some(old);
            let button = find(&p, "a-jump");
            if agent {
                assert_eq!(
                    p.control_tap(&json!({"id":button})).unwrap()["tapped"],
                    button
                );
            } else {
                let (x, y, w, h) = p.rect_of(button).unwrap();
                assert_eq!(p.press_at(x + w / 2., y + h / 2., 1.), Some(button));
            }
            assert_eq!(p.focus, Some(button));
            assert_eq!(
                p.brush.ringless,
                Some(button),
                "winui={winui}, android={android}, agent={agent}"
            );
            done(p, path);
        }
    }
}

#[test]
fn a_canvas_touch_keeps_an_editors_focus_visible_and_script_focus_unringed() {
    for android in [false, true] {
        let (mut p, path) = fixture();
        p.brush.touch_mode = android;
        p.set_pointer_kind(crate::presenter::PointerKind::Touch);
        p.hardware_key("ShiftLeft", "Shift", true, false);
        p.hardware_key("ShiftLeft", "Shift", false, false);
        let editor = find(&p, "editor");
        assert!(p.set_focus(Some(editor), 0.).is_none());
        let (x, y, w, h) = p.rect_of(find(&p, "raw")).unwrap();
        p.pointer_down(x + w / 2., y + h / 2., 1.).unwrap();
        assert_eq!(p.focus, Some(editor));
        assert_eq!(p.brush.ringless, None);
        p.pointer_up(x + w / 2., y + h / 2., 2.).unwrap();
        assert_eq!(p.focus, Some(editor));
        let next = find(&p, "remove");
        assert!(p.set_focus(Some(next), 3.).is_none());
        assert_eq!(p.brush.ringless, Some(next));
        done(p, path);
    }
}

#[test]
fn a_pointer_hud_press_returns_unringed_focus_to_its_canvas() {
    let (mut p, path) = fixture();
    p.hardware_key("ShiftLeft", "Shift", true, false);
    p.hardware_key("ShiftLeft", "Shift", false, false);
    p.tap(find(&p, "hud-remove")).unwrap();
    assert_eq!(p.focus, Some(find(&p, "raw")));
    assert_eq!(p.brush.ringless, p.focus);
    done(p, path);
}

#[test]
fn mouse_canvas_action_focus_emits_events_unless_an_editor_keeps_focus() {
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for initial in ["none", "old", "editor"] {
        for target in ["button", "label"] {
            for feedback in ["", "-exact-press-opacity=0.5"] {
                let source = format!(
                    r#"component Controls
  state seen = ""
  action record(name: string)
    seen = `${{seen}}${{name}} `
  action entered(e: FocusEvent)
    seen = `${{seen}}in `
  action left(e: FocusEvent)
    seen = `${{seen}}out `
  action clear
    seen = ""
  view
    column focusin=entered focusout=left
      button testId="old" width=100 height=30 blur=record("blur")
      input testId="editor" width=100 height=30 blur=record("blur")
      canvas testId="a" width=100 height=100
        button action="jump" testId="button" width=100 height=100 focus=record("focus") {feedback}
          box testId="label" width=60 height=60
"#
                );
                let (mut p, path) = boot(&source, &["a"]);
                let old = (initial != "none").then(|| find(&p, initial));
                assert!(p.set_focus(old, 0.).is_none());
                p.brush.ringless = old;
                p.host.runner_mut().act("clear", vec![]).unwrap();
                assert!(p.after_commit().is_none());
                let (x, y, w, h) = p.rect_of(find(&p, target)).unwrap();
                p.pointer_down(x + w / 2., y + h / 2., 1.).unwrap();
                let down = (p.focus(), p.host.runner().slot("seen").cloned());
                p.pointer_up(x + w / 2., y + h / 2., 2.).unwrap();
                actual.push((
                    initial,
                    target,
                    feedback,
                    down,
                    p.focus(),
                    p.host.runner().slot("seen").cloned(),
                    p.brush.ringless,
                ));
                let (focus, events, ringless) = if initial == "editor" {
                    (old, "", old)
                } else {
                    (
                        Some(find(&p, "button")),
                        if old.is_some() {
                            "blur out focus in "
                        } else {
                            "focus in "
                        },
                        Some(find(&p, "button")),
                    )
                };
                let seen = Some(exact_runner::Value::str(events));
                expected.push((
                    initial,
                    target,
                    feedback,
                    (focus, seen.clone()),
                    focus,
                    seen,
                    ringless,
                ));
                done(p, path);
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn a_mouse_click_on_a_canvas_action_runs_its_redirecting_focus_handler_once() {
    let source = r#"component Controls
  state count = 0
  action entered
    count = count + 1
    focus("other")
  view
    column
      canvas testId="a" width=100 height=100
        button action="jump" testId="button" focus=entered width=100 height=100
      button testId="other" id="other" width=100 height=30
"#;
    let (mut p, path) = boot(source, &["a"]);
    let (x, y, w, h) = p.rect_of(find(&p, "button")).unwrap();
    p.pointer_down(x + w / 2., y + h / 2., 1.).unwrap();
    p.run_commands(|| ());
    assert_eq!(p.focus, Some(find(&p, "other")));
    p.pointer_up(x + w / 2., y + h / 2., 2.).unwrap();
    p.run_commands(|| ());
    assert_eq!(
        p.host.runner().slot("count"),
        Some(&exact_runner::Value::Number(1.))
    );
    assert_eq!(p.focus, Some(find(&p, "other")));
    done(p, path);
}
