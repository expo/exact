//! A menu painted in the page, not opened as a surface: the kernel's keys (LLP 1166.000 D2).
use super::*;
use exact_runner::Value;

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

const FORMS: &str = r#"component App
  state n = 0
  action bump()
    n = n + 1
  view
    column role="menu" width=480 height=400
      link press=bump() testId="link-nohref"
        text "No href"
      link href="/x" testId="link-href"
        text "With href"
      button "Native" press=bump() testId="button"
      button "Bare" appearance="none" press=bump() testId="button-none"
      row press=bump() testId="box-press"
        text "Box with press"
      row role="button" testId="box-role"
        text "Box with role"
      row role="menuitem" tabindex=-1 testId="box-menuitem"
        text "Box menuitem"
"#;

/// What each element a menu could hold lowers to, and that the kernel's
/// members are the elements the web counts (`button`, `a[href]`,
/// `[role^="menuitem"]`), which the web makes of these nodes as the comments say.
#[test]
fn a_page_menu_s_members_are_the_elements_the_web_counts() {
    let mut p = boot(FORMS);
    for (name, lowered) in [
        // <button role="link">
        (
            "link-nohref",
            (NodeType::Pressable, Some("link"), None, None),
        ),
        // <a href="/x">
        (
            "link-href",
            (NodeType::Pressable, Some("link"), Some("/x"), None),
        ),
        // <button>
        (
            "button",
            (NodeType::Control, Some("button"), None, Some("button")),
        ),
        // <button>
        (
            "button-none",
            (NodeType::Pressable, Some("button"), None, None),
        ),
        // <div>
        ("box-press", (NodeType::View, None, None, None)),
        // <div role="button">
        ("box-role", (NodeType::View, Some("button"), None, None)),
        // <div role="menuitem">
        (
            "box-menuitem",
            (NodeType::View, Some("menuitem"), None, None),
        ),
    ] {
        let n = p.host.kernel().node(id(&p, name)).unwrap();
        let facts = (
            n.node_type,
            n.props.str(PropId::AccessibilityRole),
            n.props.str(PropId::Href),
            n.props.str(PropId::Type),
        );
        assert_eq!(facts, lowered, "{name}");
        assert_eq!(n.props.str(PropId::SemanticTag), None, "{name}");
    }
    assert!(p.set_focus(Some(id(&p, "link-nohref")), 0.).is_none());
    for next in [
        "link-href",
        "button",
        "button-none",
        "box-menuitem",
        "link-nohref",
    ] {
        key(&mut p, "ArrowDown", next);
    }
    assert_eq!(p.host.runner().slot("n"), Some(&Value::Number(0.)));
}

const MENU: &str = r#"component App
  state n = 0
  action bump()
    n = n + 1
  view
    column width=480 height=400
      button "Before" testId="before"
      column role="menu"
        button "One" press=bump() testId="one"
        button "Two" press=bump() testId="two"
        button "Three" press=bump() testId="three"
        row role="tablist"
          button "A" role="tab" aria-selected=true press=bump() testId="tab-a"
          button "B" role="tab" press=bump() testId="tab-b"
      button "After" testId="after"
"#;

#[test]
fn a_page_menu_moves_the_focus_by_its_axis_and_each_item_is_its_own_stop() {
    let mut p = boot(MENU);
    assert!(p.set_focus(Some(id(&p, "one")), 0.).is_none());
    for (name, next) in [
        ("ArrowDown", "two"),
        ("ArrowDown", "three"),
        ("ArrowDown", "one"),
        ("ArrowUp", "three"),
        ("Home", "one"),
        ("End", "three"),
        ("ArrowRight", "three"),
        ("ArrowLeft", "three"),
    ] {
        key(&mut p, name, next);
    }
    assert_eq!(p.host.runner().slot("n"), Some(&Value::Number(0.)));
    // A menu sets no stop: Tab reaches each item; the tablist in it has one.
    assert!(p.set_focus(Some(id(&p, "before")), 0.).is_none());
    for next in ["one", "two", "three", "tab-a", "after"] {
        key(&mut p, "Tab", next);
    }
}

#[test]
fn a_tablist_in_a_page_menu_moves_by_its_own_arrows() {
    let mut p = boot(MENU);
    assert!(p.set_focus(Some(id(&p, "tab-a")), 0.).is_none());
    for (name, next) in [
        ("ArrowRight", "tab-b"),
        ("ArrowRight", "tab-a"),
        ("ArrowLeft", "tab-b"),
        ("ArrowDown", "tab-b"),
        ("Home", "tab-a"),
    ] {
        key(&mut p, name, next);
    }
    // The menu's own arrows pass over the tablist's tabs.
    assert!(p.set_focus(Some(id(&p, "three")), 0.).is_none());
    key(&mut p, "ArrowDown", "one");
    assert_eq!(p.host.runner().slot("n"), Some(&Value::Number(0.)));
}
