//! The keyboard reaches and works what a pointer does, as on the web: Tab
//! stops at every form control HTML lists as focusable.
use super::*;

fn boot(source: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 600.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn id(p: &Presenter<()>, test_id: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// The test id of the focus, or "-" for none.
fn focused(p: &Presenter<()>) -> String {
    let k = p.host.kernel();
    p.focus()
        .and_then(|f| k.node(f))
        .and_then(|n| n.props.str(PropId::TestId).map(str::to_owned))
        .unwrap_or_else(|| "-".into())
}

/// The focus after each of `keys`, pressed in turn.
fn walk(p: &mut Presenter<()>, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .map(|k| {
            p.key_down(k, 0.);
            focused(p)
        })
        .collect()
}

const FORM: &str = r#"component App
  state level = 5
  state pick = "a"
  action moved(v: number)
    level = v
  action picked(v: string)
    pick = v
  view
    column width=400 height=600
      input testId="text" height=30
      input type="range" min=0 max=10 value=level input=moved change=moved testId="range" width=200
      input type="checkbox" testId="check"
      select value=pick change=picked aria-label="Pick" testId="select"
        option "A" value="a"
        option "B" value="b"
      input type="date" testId="date"
      input type="time" testId="time"
      input type="file" id="upload" testId="file"
      button "Save" testId="save" height=40
"#;

#[test]
fn tab_stops_at_a_range_a_select_and_a_file_control_as_htmls_focusable_areas() {
    let mut p = boot(FORM);
    // With no picker to open, a date or time field is no stop: its press
    // does nothing either (LLP 1069.001).
    assert_eq!(
        walk(&mut p, &["Tab"; 7]),
        ["text", "range", "check", "select", "file", "save", "text"]
    );
    // Where a press opens the platform's picker, the date and time are stops.
    let mut p = boot(FORM);
    p.agent = true;
    assert_eq!(
        walk(&mut p, &["Tab"; 9]),
        ["text", "range", "check", "select", "date", "time", "file", "save", "text"]
    );
    // A press focuses each, as Chrome's does.
    for name in ["range", "select", "file"] {
        let mut p = boot(FORM);
        let (x, y, w, h) = p.rect_of(id(&p, name)).unwrap();
        p.pointer_down(x + w / 2., y + h / 2., 0.).unwrap();
        p.pointer_up(x + w / 2., y + h / 2., 1.).unwrap();
        assert_eq!(focused(&p), name);
    }
}

#[test]
fn tab_from_a_focused_negative_index_control_continues_in_tree_order() {
    let source = r#"component App
  view
    column width=400 height=600
      button "Before" testId="before" height=30
      button "Middle" tabindex=-1 testId="middle" height=30
      button "After" testId="after" height=30
"#;
    for positive in [false, true] {
        let extra = if positive {
            r#"      button "Two" tabindex=2 testId="two" height=30
      button "One" tabindex=1 testId="one" height=30
"#
        } else {
            ""
        };
        let mut p = boot(&format!("{source}{extra}"));
        for backward in [false, true] {
            assert_eq!(p.set_focus(Some(id(&p, "middle")), 0.), None);
            p.hold_modifier("ShiftLeft", backward);
            let expected: &[&str] = match (positive, backward) {
                (false, false) => &["after", "before", "after"],
                (false, true) => &["before", "after", "before"],
                (true, false) => &["after", "one", "two", "before", "after"],
                (true, true) => &["before", "two", "one", "after", "before"],
            };
            assert_eq!(walk(&mut p, &vec!["Tab"; expected.len()]), expected);
            p.hold_modifier("ShiftLeft", false);
        }
    }
}

#[test]
fn tab_from_a_focused_negative_index_control_uses_tree_order_before_tabindex_order() {
    let cases = [
        (
            &[("before", 1), ("middle", -1), ("after", 2)][..],
            &["after", "before", "after"][..],
            &["before", "after", "before"][..],
        ),
        (
            &[("middle", -1), ("one", 1), ("zero", 0)],
            &["one", "zero", "one"],
            &["zero", "one", "zero"],
        ),
        (
            &[("zero", 0), ("one", 1), ("middle", -1)],
            &["one", "zero", "one"],
            &["one", "zero", "one"],
        ),
        (
            &[("before", 2), ("middle", -1), ("after", 1), ("zero", 0)],
            &["after", "before", "zero", "after"],
            &["before", "after", "zero", "before"],
        ),
        (
            &[("middle", -1), ("only", 1)],
            &["only", "only"],
            &["only", "only"],
        ),
    ];
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (buttons, forward, backward) in cases {
        let rows: String = buttons
            .iter()
            .map(|(name, index)| {
                format!("      button \"{name}\" testId=\"{name}\" tabindex={index} height=30\n")
            })
            .collect();
        let mut p = boot(&format!(
            "component App\n  view\n    column width=400 height=600\n{rows}"
        ));
        for (back, stops) in [(false, forward), (true, backward)] {
            assert_eq!(p.set_focus(Some(id(&p, "middle")), 0.), None);
            p.hold_modifier("ShiftLeft", back);
            actual.push(walk(&mut p, &vec!["Tab"; stops.len()]));
            expected.push(stops.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn tab_keeps_a_focused_negative_index_control_when_there_are_no_stops() {
    let mut p = boot(
        r#"component App
  view
    button "Middle" tabindex=-1 autofocus testId="middle" width=100 height=30
"#,
    );
    for backward in [false, true] {
        p.hold_modifier("ShiftLeft", backward);
        assert_eq!(walk(&mut p, &["Tab"]), ["middle"]);
        p.hold_modifier("ShiftLeft", false);
    }
}

/// A `tabindex=-1` box a click focused is the starting point too, as in
/// Chrome: the same four elements as HTML give `b` for Tab and `a` for
/// Shift+Tab.
#[test]
fn tab_from_a_clicked_negative_index_box_continues_in_tree_order() {
    let mut p = boot(
        r#"component App
  view
    column width=400 height=300
      button "A" testId="a" height=30
      box tabindex=-1 testId="mid" width=100 height=30
        text "Heading"
      button "B" testId="b" height=30
      button "C" testId="c" height=30
"#,
    );
    let mut actual = Vec::new();
    for backward in [false, true] {
        let (x, y, w, h) = p.rect_of(id(&p, "mid")).unwrap();
        p.pointer_down(x + w / 2., y + h / 2., 0.).unwrap();
        p.pointer_up(x + w / 2., y + h / 2., 1.).unwrap();
        let clicked = focused(&p);
        p.hold_modifier("ShiftLeft", backward);
        actual.push((clicked, walk(&mut p, &["Tab"])));
        p.hold_modifier("ShiftLeft", false);
    }
    let mid = || "mid".to_string();
    assert_eq!(
        actual,
        [(mid(), vec!["b".into()]), (mid(), vec!["a".into()])]
    );
}

#[test]
fn space_and_enter_at_a_file_or_date_control_click_it() {
    let source = r#"component App
  state presses = 0
  action pressed
    presses = presses + 1
  view
    column width=400 height=600
      input type="file" id="upload" press=pressed testId="file"
      input type="date" value="2026-11-10" testId="date"
"#;
    for key in [" ", "Enter"] {
        let mut p = boot(source);
        p.agent = true;
        let (file, date) = (id(&p, "file"), id(&p, "date"));
        let holds = |p: &Presenter<()>| {
            (p.host.runner().device_holds().iter())
                .map(|h| h.node)
                .collect::<Vec<_>>()
        };
        assert_eq!(walk(&mut p, &["Tab", key]), ["file", "file"]);
        assert_eq!(
            p.host.runner().slot("presses"),
            Some(&exact_runner::Value::Number(1.))
        );
        assert_eq!(holds(&p), [Some(file)], "{key:?} opens the file's picker");
        assert_eq!(walk(&mut p, &["Tab", key]), ["date", "date"]);
        assert!(holds(&p).contains(&Some(date)), "{key:?} opens the date's");
    }
}

const TOGGLES: &str = r#"component App
  state on = false
  state sw = false
  state role = false
  state log = ""
  action saw(v: bool)
    log = `${log}input:${v} `
  action flip(v: bool)
    on = v
    log = `${log}change:${v} `
  action flick(v: bool)
    sw = v
  action flock(v: bool)
    role = v
  action pressed
    log = `${log}press `
  view
    column width=400 height=600
      input type="checkbox" checked=on input=saw change=flip press=pressed testId="check"
      input type="checkbox" switch checked=sw change=flick testId="switch"
      input type="checkbox" role="switch" checked=role change=flock testId="role"
      input type="checkbox" testId="loose"
"#;

#[test]
fn space_toggles_a_focused_checkbox_or_switch_as_a_click_and_enter_does_not() {
    use exact_runner::Value;
    let mut p = boot(TOGGLES);
    let slot = |p: &Presenter<()>, name: &str| p.host.runner().slot(name).cloned();
    let on = |v: bool| Some(Value::Bool(v));
    assert_eq!(walk(&mut p, &["Tab", "Enter"]), ["check", "check"]);
    assert_eq!(slot(&p, "on"), on(false), "Enter toggles no checkbox");
    p.key_down(" ", 0.);
    assert_eq!(slot(&p, "on"), on(true));
    assert_eq!(
        slot(&p, "log").as_ref().and_then(Value::as_str),
        Some("press press input:true change:true "),
        "Enter presses; Space presses, then the toggle's input and change"
    );
    for (name, state) in [("switch", "sw"), ("role", "role")] {
        assert_eq!(walk(&mut p, &["Tab", "Enter"]), [name, name]);
        assert_eq!(slot(&p, state), on(false), "Enter toggles no {name}");
        p.key_down(" ", 0.);
        assert_eq!(slot(&p, state), on(true), "Space toggles the {name}");
    }
    // An unbound checkbox keeps its own state.
    let loose = id(&p, "loose");
    assert_eq!(walk(&mut p, &["Tab", " "]), ["loose", "loose"]);
    assert_eq!(p.controls.get(&loose), Some(&true));
}

#[test]
fn tab_scrolls_the_focus_into_view_in_its_scroller_and_the_page() {
    let source = r#"component App
  state n = 0
  action bump
    n = n + 1
  view
    column width=400
      button press=bump testId="top" height=30
        text "Top"
      column width=300 height=100 overflow-y="auto" testId="list"
        box height=400 flex-shrink=0
          text "Inner"
        button press=bump testId="inner" height=30 flex-shrink=0
          text "Inner button"
      box height=2000 flex-shrink=0
        text "Tall"
      button press=bump testId="bottom" height=30 flex-shrink=0
        text "Bottom"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let _ = p.frame();
    let list = id(&p, "list");
    // Chrome scrolls each only as far as it must (`nearest`): the inner
    // button to the scroller's bottom edge, then the last to the window's.
    assert_eq!(walk(&mut p, &["Tab", "Tab"]), ["top", "inner"]);
    let _ = p.frame();
    assert_eq!((p.scroll_of(list).1, p.page().1), (330., 0.));
    assert_eq!(p.rect_of(id(&p, "inner")).map(|r| r.1), Some(100.));
    assert_eq!(walk(&mut p, &["Tab"]), ["bottom"]);
    let _ = p.frame();
    assert_eq!(p.page().1, 1860.);
    assert_eq!(p.rect_of(id(&p, "bottom")).map(|r| r.1), Some(270.));
}
