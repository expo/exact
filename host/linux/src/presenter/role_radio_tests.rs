use super::*;
use exact_runner::Value;

const APP: &str = r#"component App
  state choice = "one"
  state presses = 0
  action choose(value: string)
    choice = value
    presses = presses + 1
  view
    column direction="ltr" width=480 height=400
      button "Before" testId="before"
      row role="radiogroup" appearance="none" testId="group"
        button "One" role="radio" aria-checked=(choice == "one") press=choose("one") testId="one"
        button "Disabled" role="radio" disabled=true testId="disabled"
        button "Unavailable" role="radio" aria-disabled=true press=choose("unavailable") testId="unavailable"
        button "Two" role="radio" aria-checked=(choice == "two") press=choose("two") testId="two"
        button "Three" role="radio" aria-checked=(choice == "three") press=choose("three") testId="three"
      button "After" testId="after"
"#;

fn boot(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (480., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_overlays();
    p.frame();
    p
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn key(p: &mut Presenter<()>, key: &str, next: &str) {
    p.key_down(key, 0.);
    p.run_commands(|| ());
    assert_eq!(p.focus, Some(id(p, next)), "{key}");
}

#[test]
fn painted_radio_arrows_move_and_press_with_wrap_and_group_direction() {
    for (direction, forward, backward) in [
        ("ltr", "ArrowRight", "ArrowLeft"),
        ("rtl", "ArrowLeft", "ArrowRight"),
    ] {
        let mut p = boot(&APP.replace("direction=\"ltr\"", &format!("direction=\"{direction}\"")));
        assert!(p.set_focus(Some(id(&p, "one")), 0.).is_none());
        let mut presses = 0;
        for (arrow, next) in [
            (forward, "two"),
            (forward, "three"),
            (forward, "one"),
            (backward, "three"),
            (backward, "two"),
            (backward, "one"),
            ("ArrowDown", "two"),
            ("ArrowUp", "one"),
        ] {
            key(&mut p, arrow, next);
            presses += 1;
            assert_eq!(p.host.runner().slot("choice"), Some(&Value::str(next)));
            assert_eq!(
                p.host.runner().slot("presses"),
                Some(&Value::Number(presses as f64))
            );
        }
        for modifier in ["ControlLeft", "AltLeft", "MetaLeft"] {
            p.hold_modifier(modifier, true);
            key(&mut p, forward, "one");
            p.hold_modifier(modifier, false);
        }
        key(&mut p, "Home", "one");
        key(&mut p, "End", "one");
        assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(8.)));
    }
}

#[test]
fn painted_radio_groups_have_one_tab_stop_and_leave_in_both_directions() {
    let mut p = boot(APP);
    for (name, next) in [
        ("Tab", "before"),
        ("Tab", "one"),
        ("ArrowDown", "two"),
        ("Tab", "after"),
    ] {
        key(&mut p, name, next);
    }
    p.hold_modifier("ShiftLeft", true);
    key(&mut p, "Tab", "two");
    key(&mut p, "Tab", "before");
    p.hold_modifier("ShiftLeft", false);
    let boxed = p.shown_boxes();
    let stops: Vec<_> = ["one", "disabled", "unavailable", "two", "three"]
        .into_iter()
        .filter(|name| p.tabbable(id(&p, name), &boxed))
        .collect();
    assert_eq!(stops, ["two"]);
}

#[test]
fn a_prevented_radio_arrow_neither_moves_nor_presses() {
    let source = APP.replace("  view\n", "  action intercept(k: string)\n    if k == \"ArrowRight\"\n      preventDefault()\n  view\n")
        .replace("testId=\"group\"", "testId=\"group\" key=intercept");
    let mut p = boot(&source);
    assert!(p.set_focus(Some(id(&p, "one")), 0.).is_none());
    key(&mut p, "ArrowRight", "one");
    assert_eq!(p.host.runner().slot("choice"), Some(&Value::str("one")));
    assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(0.)));
    key(&mut p, "ArrowLeft", "three");
    assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(1.)));
}

fn radio_stops(p: &mut Presenter<()>) -> Vec<&'static str> {
    let boxed = p.shown_boxes();
    ["one", "disabled", "unavailable", "two", "three"]
        .into_iter()
        .filter(|name| p.tabbable(id(p, name), &boxed))
        .collect()
}

/// A page's `role="menu"` is no surface (those take every key first,
/// `key_pressed`): a radio group in it keeps its arrows and one stop.
#[test]
fn a_radio_group_in_a_page_menu_keeps_its_keys_and_one_stop() {
    let mut p = boot(&APP.replace("column direction=", "column role=\"menu\" direction="));
    assert_eq!(radio_stops(&mut p), ["one"]);
    assert!(p.set_focus(Some(id(&p, "one")), 0.).is_none());
    key(&mut p, "ArrowDown", "two");
    assert_eq!(p.host.runner().slot("choice"), Some(&Value::str("two")));
    assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(1.)));
    assert_eq!(radio_stops(&mut p), ["two"]);
}

const NATIVE: &str = r#"component App
  state choice = "two"
  action choose(value: string)
    choice = value
  view
    column width=480 height=400
      button "Before" testId="before"
      row role="radiogroup" testId="group"
        input type="radio" name="choice" value="one" checked=(choice == "one") change=choose testId="one"
        input type="radio" name="choice" value="disabled" disabled=true testId="disabled"
        input type="radio" name="choice" value="two" checked=(choice == "two") change=choose testId="two"
        input type="radio" name="choice" value="three" checked=(choice == "three") change=choose testId="three"
      button "After" testId="after"
"#;

/// Native radios in a `radiogroup` are HTML's group, not a painted one:
/// their one Tab stop is the checked radio (`radio_tab_stop`), and an arrow
/// checks the next and fires `change` (`radio_key`).
#[test]
fn native_radios_in_a_radiogroup_keep_html_s_stop_and_arrows() {
    let mut p = boot(NATIVE);
    for (name, next) in [("Tab", "before"), ("Tab", "two"), ("ArrowRight", "three")] {
        key(&mut p, name, next);
    }
    assert_eq!(p.host.runner().slot("choice"), Some(&Value::str("three")));
    assert!(p.radio_checked(id(&p, "three")));
    key(&mut p, "Tab", "after");
    p.hold_modifier("ShiftLeft", true);
    key(&mut p, "Tab", "three");
    p.hold_modifier("ShiftLeft", false);
}
