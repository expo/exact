use super::*;
use exact_runner::Value;

const APP: &str = r#"component App
  state presses = 0
  action press
    presses = presses + 1
  view
    column width=480 height=400
      button "Before" testId="before"
      row role="tablist" aria-label="Views" testId="tabs"
        button "One" role="tab" aria-selected="true" press=press testId="one"
        button "Two" role="tab" press=press testId="two"
        button "Three" role="tab" press=press testId="three"
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

fn focus(p: &mut Presenter<()>, name: &str) {
    assert!(p.set_focus(Some(id(p, name)), 0.).is_none());
    assert_eq!(p.focus, Some(id(p, name)));
}

fn keys(p: &mut Presenter<()>, steps: &[(&str, &str)]) {
    for (key, next) in steps {
        p.key_down(key, 0.);
        p.run_commands(|| ());
        assert_eq!(p.focus, Some(id(p, next)), "{key}: expected {next}");
    }
}

fn arrows(source: &str, forward: &str, backward: &str, other: &str) {
    let mut p = boot(source);
    focus(&mut p, "one");
    keys(
        &mut p,
        &[
            (other, "one"),
            (forward, "two"),
            (forward, "three"),
            ("End", "three"),
            ("Home", "one"),
            (backward, "three"),
            (forward, "one"),
        ],
    );
    assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(0.)));
    keys(&mut p, &[("Enter", "one"), (" ", "one")]);
    assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(2.)));
}

#[test]
fn horizontal_tabs_rove_without_pressing() {
    arrows(APP, "ArrowRight", "ArrowLeft", "ArrowDown");
}

#[test]
fn vertical_tabs_rove_without_pressing() {
    arrows(
        &APP.replace("row role=", "column aria-orientation=\"vertical\" role="),
        "ArrowDown",
        "ArrowUp",
        "ArrowRight",
    );
}

#[test]
fn horizontal_tabs_follow_inherited_rtl_direction() {
    arrows(
        &APP.replace("column width=", "column direction=\"rtl\" width="),
        "ArrowLeft",
        "ArrowRight",
        "ArrowDown",
    );
}

#[test]
fn tab_navigation_skips_unavailable_members_and_falls_back_without_a_selection() {
    for unavailable in [
        "disabled=true",
        "aria-disabled=true",
        "display=\"none\"",
        "visibility=\"hidden\"",
        "inert=true",
    ] {
        let source = APP.replace("testId=\"one\"", &format!("testId=\"one\" {unavailable}"));
        let mut p = boot(&source);
        keys(
            &mut p,
            &[("Tab", "before"), ("Tab", "two"), ("Tab", "after")],
        );
        focus(&mut p, "two");
        keys(
            &mut p,
            &[
                ("End", "three"),
                ("ArrowRight", "two"),
                ("ArrowLeft", "three"),
                ("Home", "two"),
            ],
        );
    }
    let mut p = boot(&APP.replace("aria-selected=\"true\"", ""));
    keys(
        &mut p,
        &[("Tab", "before"), ("Tab", "one"), ("Tab", "after")],
    );
    let source = APP
        .replace("aria-selected=\"true\"", "")
        .replace("testId=\"two\"", "testId=\"two\" aria-selected=true");
    let mut p = boot(&source);
    keys(
        &mut p,
        &[("Tab", "before"), ("Tab", "two"), ("Tab", "after")],
    );
}

#[test]
fn tabs_under_wrappers_belong_to_their_nearest_tablist() {
    let source = APP.replace(
        "        button \"Two\" role=\"tab\" press=press testId=\"two\"",
        r#"        column
          button "Two" role="tab" press=press testId="two"
          row role="tablist"
            button "Inner one" role="tab" testId="inner-one"
            button "Inner two" role="tab" testId="inner-two""#,
    );
    let mut p = boot(&source);
    focus(&mut p, "one");
    keys(
        &mut p,
        &[
            ("ArrowRight", "two"),
            ("ArrowRight", "three"),
            ("ArrowRight", "one"),
        ],
    );
    focus(&mut p, "inner-one");
    keys(
        &mut p,
        &[("ArrowRight", "inner-two"), ("ArrowRight", "inner-one")],
    );
}

#[test]
fn authored_key_handlers_can_prevent_or_redirect_tab_navigation() {
    let source = APP
        .replace(
            "  action press",
            r#"  action intercept(k: string)
    if k == "ArrowRight"
      preventDefault()
    if k == "End"
      focus("after")
  action press"#,
        )
        .replace("testId=\"tabs\"", "testId=\"tabs\" key=intercept")
        .replace("testId=\"after\"", "testId=\"after\" id=\"after\"");
    let mut p = boot(&source);
    focus(&mut p, "one");
    keys(
        &mut p,
        &[
            ("ArrowRight", "one"),
            ("ArrowLeft", "three"),
            ("End", "after"),
        ],
    );
    assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(0.)));
}

#[test]
fn a_tablist_has_one_stop_and_tab_leaves_the_roved_tab() {
    let mut p = boot(APP);
    keys(
        &mut p,
        &[
            ("Tab", "before"),
            ("Tab", "one"),
            ("Tab", "after"),
            ("Tab", "before"),
            ("Tab", "one"),
            ("ArrowRight", "two"),
            ("Tab", "after"),
        ],
    );
    p.hardware_key("ShiftLeft", "Shift", true, false);
    keys(
        &mut p,
        &[("Tab", "one"), ("ArrowRight", "two"), ("Tab", "before")],
    );
    p.hardware_key("ShiftLeft", "Shift", false, false);
}

#[test]
fn negative_tabindex_excludes_tab_entry_but_not_arrow_navigation() {
    for selected in ["aria-selected=\"true\"", ""] {
        let source = APP
            .replace("aria-selected=\"true\"", selected)
            .replace("testId=\"one\"", "testId=\"one\" tabindex=-1");
        let mut p = boot(&source);
        keys(
            &mut p,
            &[("Tab", "before"), ("Tab", "two"), ("Tab", "after")],
        );
        p.hold_modifier("ShiftLeft", true);
        keys(&mut p, &[("Tab", "two"), ("Tab", "before")]);
        p.hold_modifier("ShiftLeft", false);
        keys(
            &mut p,
            &[
                ("Tab", "two"),
                ("ArrowLeft", "one"),
                ("ArrowLeft", "three"),
                ("Home", "one"),
                ("Tab", "after"),
            ],
        );
        p.hold_modifier("ShiftLeft", true);
        keys(
            &mut p,
            &[("Tab", "two"), ("ArrowLeft", "one"), ("Tab", "before")],
        );
        p.hold_modifier("ShiftLeft", false);
        assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(0.)));
    }
    let mut p = boot(&APP.replace("role=\"tab\"", "role=\"tab\" tabindex=-1"));
    keys(&mut p, &[("Tab", "before"), ("Tab", "after")]);
    focus(&mut p, "one");
    keys(
        &mut p,
        &[("ArrowRight", "two"), ("End", "three"), ("Tab", "after")],
    );
    focus(&mut p, "two");
    p.hold_modifier("ShiftLeft", true);
    keys(&mut p, &[("Home", "one"), ("Tab", "before")]);
    p.hold_modifier("ShiftLeft", false);
}

#[test]
fn tab_leaves_a_focused_aria_disabled_tab_in_either_direction() {
    for tabindex in ["", "tabindex=-1"] {
        let source = APP.replace(
            "testId=\"two\"",
            &format!("testId=\"two\" aria-disabled=true {tabindex}"),
        );
        let mut p = boot(&source);
        focus(&mut p, "two");
        keys(&mut p, &[("Tab", "after")]);
        focus(&mut p, "two");
        p.hold_modifier("ShiftLeft", true);
        keys(&mut p, &[("Tab", "before")]);
        p.hold_modifier("ShiftLeft", false);
        keys(
            &mut p,
            &[
                ("Tab", "one"),
                ("ArrowRight", "three"),
                ("ArrowLeft", "one"),
            ],
        );
        focus(&mut p, "two");
        keys(&mut p, &[("ArrowRight", "three")]);
        assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(0.)));
    }
}

#[test]
fn modified_navigation_keys_do_not_rove_tabs() {
    for vertical in [false, true] {
        let source = if vertical {
            APP.replace("row role=", "column aria-orientation=\"vertical\" role=")
        } else {
            APP.to_string()
        };
        let mut p = boot(&source);
        focus(&mut p, "two");
        for modifier in ["ControlLeft", "AltLeft", "MetaLeft"] {
            p.hold_modifier(modifier, true);
            keys(
                &mut p,
                &[
                    ("ArrowLeft", "two"),
                    ("ArrowRight", "two"),
                    ("ArrowUp", "two"),
                    ("ArrowDown", "two"),
                    ("Home", "two"),
                    ("End", "two"),
                ],
            );
            p.hold_modifier(modifier, false);
        }
        keys(&mut p, &[("Home", "one"), ("End", "three")]);
        assert_eq!(p.host.runner().slot("presses"), Some(&Value::Number(0.)));
    }
}

#[test]
fn a_modal_tablist_reports_its_tabs_and_takes_their_keys() {
    const SOURCE: &str = r#"component App
  view
    column width=480 height=400
      row role="tablist" aria-label="Page views"
        button "P one" role="tab"
        button "P two" role="tab"
      button "Open" testId="open" commandfor="box" command="show-modal"
      dialog id="box" width=280
        row role="tablist" aria-label="Views"
          button "One" role="tab" aria-selected="true" autofocus=true testId="one"
          button "Two" role="tab" testId="two"
        button "Close" testId="close" commandfor="box" command="close"
"#;
    let mut p = boot(SOURCE);
    let sets = |p: &mut Presenter<()>| {
        p.access_tree()
            .0
            .into_iter()
            .filter(|n| matches!(n.role, access::role::TABLIST | access::role::TAB))
            .map(|n| (n.name, n.set))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sets(&mut p),
        [
            ("Page views".into(), (0, 2)),
            ("P one".into(), (1, 2)),
            ("P two".into(), (2, 2))
        ]
    );
    p.tap(id(&p, "open")).unwrap();
    p.run_commands(|| ());
    p.frame();
    assert_eq!(
        sets(&mut p),
        [
            ("Views".into(), (0, 2)),
            ("One".into(), (1, 2)),
            ("Two".into(), (2, 2))
        ]
    );
    keys(
        &mut p,
        &[("ArrowRight", "two"), ("Tab", "close"), ("Tab", "one")],
    );
}
