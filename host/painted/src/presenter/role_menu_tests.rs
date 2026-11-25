//! A menu painted in the page, not opened as a surface: the kernel's keys (LLP 1166.000 D2).
use super::*;
use exact_runner::Value;

fn boot(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
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

#[test]
fn a_button_shortcut_fires_with_every_authored_role() {
    let roles = exact_kernel::ARIA_ROLES;
    let source = format!(
        r#"component App
  state count = 0
  derive roles = {roles:?}
  derive current = match roles.at(count) {{ case some(r) => r, case none => "button" }}
  action pick
    count = count + 1
  view
    column
      button "Pen" appearance="none" role=current aria-keyshortcuts="p" press=pick testId="choice" height=24
"#
    );
    let mut p = boot(&source);
    let choice = id(&p, "choice");
    for (i, role) in roles.iter().copied().chain(["button"]).enumerate() {
        if i > 0 {
            p.key_down("p", 0.);
            p.run_commands(|| ());
        }
        let button = p.host.kernel().node(choice).unwrap();
        assert_eq!(button.props.str(PropId::AccessibilityRole), Some(role));
    }
}

#[test]
fn button_element_identity_follows_html_semantic_overrides() {
    let mut p = boot(FORMS);
    for name in ["button", "button-none"] {
        let button = id(&p, name);
        for (tag, is_button) in [("section", false), ("audio", true)] {
            p.host.apply_test_ops(&[exact_kernel::Op::SetProp {
                id: button,
                prop: PropId::SemanticTag,
                value: exact_kernel::PropValue::Str(tag.into()),
            }]);
            assert_eq!(
                p.host.kernel().node(button).unwrap().is_button_element(),
                is_button,
                "{name}: {tag}"
            );
        }
    }
    for (name, prop, value, is_button) in [
        ("button", PropId::Href, "/next", true),
        ("button-none", PropId::PanelHref, "/next", false),
        ("link-nohref", PropId::Href, "", false),
    ] {
        let button = id(&p, name);
        p.host.apply_test_ops(&[exact_kernel::Op::SetProp {
            id: button,
            prop,
            value: exact_kernel::PropValue::Str(value.into()),
        }]);
        assert_eq!(
            p.host.kernel().node(button).unwrap().is_button_element(),
            is_button,
            "{name}: {prop:?}"
        );
    }
}

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

const INLINE: &str = r#"component App
  view
    column width=480 height=400
      button "Before" testId="before"
      column role="menu"
        button "Cut" testId="cut"
        text testId="note"
          text "Read the "
          text "docs" href="/docs" testId="docs"
        button "Paste" testId="paste"
      text testId="line"
        text "See "
        text "the guide" href="https://example.com/guide" testId="guide"
      button "After" testId="after"
"#;

/// A link in a line of text, with no `press` or `tabindex`, takes the focus
/// as the web's `a[href]` does: a menu's arrows reach it and Tab stops on
/// it, and Enter follows it.
#[test]
fn a_link_in_a_line_of_text_is_focusable_a_menu_member_and_a_tab_stop() {
    let mut p = boot(INLINE);
    assert!(p.focusable(id(&p, "docs")));
    assert!(!p.focusable(id(&p, "note")));
    assert!(p.set_focus(Some(id(&p, "cut")), 0.).is_none());
    for (name, next) in [
        ("ArrowDown", "docs"),
        ("ArrowDown", "paste"),
        ("ArrowUp", "docs"),
        ("ArrowUp", "cut"),
    ] {
        key(&mut p, name, next);
    }
    assert!(p.set_focus(Some(id(&p, "before")), 0.).is_none());
    for next in ["cut", "docs", "paste", "guide", "after"] {
        key(&mut p, "Tab", next);
    }
    assert!(p.set_focus(Some(id(&p, "guide")), 0.).is_none());
    p.key_down("Enter", 0.);
    p.run_commands(|| ());
    assert!(p
        .host
        .runner()
        .journal()
        .any(|line| line.contains("openURL https://example.com/guide")));
}

/// Enter on a focused `link href` with no `press` follows it, as on the web.
#[test]
fn enter_follows_a_focused_link() {
    let mut p = boot(
        r#"component App
  view
    column width=480 height=400
      link href="https://example.com/block" testId="block"
        text "Block"
"#,
    );
    assert!(p.set_focus(Some(id(&p, "block")), 0.).is_none());
    p.key_down("Enter", 0.);
    p.run_commands(|| ());
    let journal: Vec<_> = p.host.runner().journal().map(|l| l.to_string()).collect();
    assert!(
        journal
            .iter()
            .any(|line| line.contains("openURL https://example.com/block")),
        "{journal:?}"
    );
    // It has no `press` to refuse.
    assert!(
        !journal.iter().any(|line| line.contains("NoHandler")),
        "{journal:?}"
    );
}

const BOTS: &str = r#"component App
  state n = 0
  action bump()
    n = n + 1
  view
    column role="menu" width=480 height=400
      button "Ada" role="menuitem" press=bump() testId="Ada"
      button "Bishop" role="menuitem" press=bump() testId="Bishop"
      button "Cortana" role="menuitem" press=bump() testId="Cortana"
      button "Dax" role="menuitem" disabled=true press=bump() testId="Dax"
      button "Data" role="menuitem" press=bump() testId="Data"
      button "Dora" role="menuitem" press=bump() testId="Dora"
      button "Eve" role="menuitem" press=bump() testId="Eve"
"#;

/// #550 on the painted hosts, as `host/web/tests/role-keys.test.mjs` has the web's: a letter moves
/// the focus to the next item it starts, the same letter again to the one after, wrapping, past a
/// disabled item; more letters refine the match. A letter no item starts is left to the page, and
/// Space with no search under way presses.
#[test]
fn a_letter_in_a_page_menu_moves_the_focus_to_the_item_it_starts() {
    let mut p = boot(BOTS);
    assert!(p.set_focus(Some(id(&p, "Cortana")), 0.).is_none());
    for (name, at, next) in [
        ("d", 0., "Data"),
        ("d", 100., "Dora"),
        ("d", 200., "Data"),
        ("a", 800., "Ada"),
        ("D", 1400., "Data"),
        ("o", 1500., "Dora"),
        ("z", 2100., "Dora"),
    ] {
        p.key_down(name, at);
        p.run_commands(|| ());
        assert_eq!(p.focus, Some(id(&p, next)), "{name} at {at}");
    }
    assert_eq!(p.host.runner().slot("n"), Some(&Value::Number(0.)));
    p.key_down(" ", 2700.);
    p.key_up(" ", "", 2700.);
    p.run_commands(|| ());
    assert_eq!(
        p.host.runner().slot("n"),
        Some(&Value::Number(1.)),
        "Space presses"
    );
}
