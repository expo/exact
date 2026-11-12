//! A menu's titled sections (LLP 1021 D3, amended 2026-11-12): a
//! `role="group"` row is a section of its own items, set apart by lines and
//! headed by its `aria-label`, which takes no press and is read as a heading.
use super::shape::Entry;
use super::*;
use crate::presenter::access::role;

/// A pull-down whose rows are an item, a separator, a titled section, an
/// untitled one, one titled by a text it holds (`aria-labelledby`, which
/// the web draws as written), and a titled section with no rows (no
/// section at all).
const APP: &str = r#"component App
  state said = "nothing"
  action pick(name: string)
    said = name
  view
    column
      text said testId="said"
      button popovertarget="m" testId="pick" width=80 height=40
        text "Pick"
      column id="m" popover="auto" role="menu" aria-label="Presets"
        button press=pick("pair") popovertarget="m" popovertargetaction="hide" testId="pair"
          text "Pair"
        hr
        column role="group" aria-label="Your presets"
          button press=pick("fireball") popovertarget="m" popovertargetaction="hide" testId="fireball"
            text "Fireball"
          button press=pick("comet") popovertarget="m" popovertargetaction="hide" testId="comet"
            text "Comet"
        column role="group"
          button press=pick("reset") popovertarget="m" popovertargetaction="hide" testId="reset"
            text "Reset"
        column role="group" aria-labelledby="mine"
          text "Mine" id="mine"
          button press=pick("saved") popovertarget="m" popovertargetaction="hide" testId="saved"
            text "Saved"
        column role="group" aria-label="Empty"
"#;

fn boot() -> Presenter<()> {
    boot_app(APP)
}

fn boot_app(app: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(app),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let kernel = p.host().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(name)[0])
        .unwrap()
        .id
}

#[test]
fn a_group_in_a_menu_is_a_section_headed_by_its_label() {
    let mut p = boot();
    let menu = {
        let kernel = p.host().kernel();
        kernel.node_by_key(kernel.find_by_id("m")[0]).unwrap().id
    };
    let Some(Plan::Menu(entries)) = p.surface_plan(menu, true) else {
        panic!("m is a menu");
    };
    let lines: Vec<String> = (entries.iter())
        .map(|e| match e {
            Entry::Item(row) => row.title.clone(),
            Entry::Separator => "—".into(),
            Entry::Header(title, _) => format!("# {title}"),
        })
        .collect();
    assert_eq!(
        lines,
        [
            "Pair",
            "—",
            "# Your presets",
            "Fireball",
            "Comet",
            "—",
            "Reset",
            "—",
            "# Mine",
            "Saved"
        ]
    );

    // Shown: the title is drawn and read as a heading before its items,
    // and takes no press; the rows keep theirs.
    p.serve_overlays();
    p.frame();
    let view = id(&p, "pick");
    p.tap(view).unwrap();
    p.run_commands(|| ());
    let shown = p.surface().expect("the menu is open");
    assert_eq!(shown.targets.len(), 5, "a title is no target");
    let titled = shown
        .texts
        .iter()
        .find(|(_, r)| r.3 > 0.0 && r.1 < shown.targets[1].rect.1);
    assert!(titled.is_some(), "the title is drawn above its first item");
    assert_eq!(
        p.surface_json()["headers"],
        serde_json::json!(["Your presets", "Mine"])
    );
    let nodes = p.access_tree().0;
    let names: Vec<(&str, bool)> = (nodes.iter())
        .filter(|n| n.role == role::HEADING || n.role == role::BUTTON)
        .map(|n| (n.name.as_str(), n.role == role::HEADING))
        .collect();
    assert_eq!(
        names,
        [
            ("Pair", false),
            ("Your presets", true),
            ("Fireball", false),
            ("Comet", false),
            ("Reset", false),
            ("Mine", true),
            ("Saved", false)
        ]
    );
    let comet = id(&p, "comet");
    let target = *shown_target(&p, comet);
    let serial = p.surface().unwrap().serial;
    let (x, y, w, h) = target.rect;
    for action in [0, 1] {
        p.surface_event(serial, action, x + w / 2.0, y + h / 2.0, 0.0);
    }
    p.run_commands(|| ());
    let said = id(&p, "said");
    let node = p.host().kernel().node(said).unwrap();
    let text: String = node.text_runs().iter().map(|r| &*r.text).collect();
    assert_eq!(text, "comet", "an item in a section presses its row");
}

/// A menu whose section holds `ODD` beside a row.
const ODD_SECTION: &str = r#"component App
  state said = "nothing"
  action pick(name: string)
    said = name
  action type(value: string)
    said = value
  view
    column
      text said testId="said"
      button popovertarget="m" testId="pick" width=80 height=40
        text "Pick"
      column id="m" popover="auto" role="menu" aria-label="Presets"
        button press=pick("pair") popovertarget="m" popovertargetaction="hide" testId="pair"
          text "Pair"
        column role="group" aria-label="Your presets"
          ODD
          button press=pick("fireball") popovertarget="m" popovertargetaction="hide" testId="fireball"
            text "Fireball"
"#;

/// A section holding anything but button rows, a field or another group,
/// keeps the popover from being a menu: it is painted as written, as on
/// Apple, not shown as a menu of the rows it has.
#[test]
fn a_section_of_more_than_rows_is_no_menu() {
    let field = r#"input value=said input=type testId="field" width=120 height=30"#;
    let group = r#"column role="group" aria-label="Inner"
            button press=pick("inner") testId="inner"
              text "Inner""#;
    for (what, odd) in [("a field", field), ("a nested group", group)] {
        let mut p = boot_app(&ODD_SECTION.replace("ODD", odd));
        let menu = {
            let kernel = p.host().kernel();
            kernel.node_by_key(kernel.find_by_id("m")[0]).unwrap().id
        };
        assert!(p.surface_plan(menu, false).is_none(), "{what}: no menu");
        // The desktop's overlays (Windows) paint it as written too.
        p.serve_overlays();
        assert!(
            p.surface_plan(menu, false).is_none(),
            "{what}: no menu, overlaid"
        );
        p.frame();
        p.tap(id(&p, "pick")).unwrap();
        p.run_commands(|| ());
        p.frame();
        assert!(p.surface().is_none(), "{what}: no menu surface");
        let nodes = p.access_tree().0;
        let buttons: Vec<&str> = (nodes.iter())
            .filter(|n| n.role == role::BUTTON)
            .map(|n| n.name.as_str())
            .collect();
        assert!(
            buttons.contains(&"Pair") && buttons.contains(&"Fireball"),
            "{what}: the popover's rows are on the page, {buttons:?}"
        );
        if what == "a field" {
            assert!(
                (nodes.iter()).any(|n| n.role == role::EDIT || n.role == role::FIELD),
                "the field is on the page"
            );
        }
    }
}

fn shown_target(p: &Presenter<()>, node: ViewId) -> &look::Target {
    p.top
        .open
        .as_ref()
        .unwrap()
        .layout
        .targets
        .iter()
        .find(|t| t.node == node)
        .unwrap()
}
