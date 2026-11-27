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
    let Some(Plan::Menu { entries, .. }) = p.surface_plan(menu, true) else {
        panic!("m is a menu");
    };
    let lines: Vec<String> = (entries.iter())
        .map(|e| match e {
            Entry::Item(row) => row.title.clone(),
            Entry::Separator => "—".into(),
            Entry::Header(title, _, _) => format!("# {title}"),
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

/// LLP 1035.001 D11 on a shell that makes windows, as Android's does: a
/// dialog whose author painted its surface is no window of the platform's
/// but the author's box in the page's top layer, as on the desktop hosts
/// (alert.rs); the page under it is inert, Back and its own button close it.
/// A dialog laid out on the platform's surface is still refused.
#[test]
fn a_dialog_whose_author_painted_its_surface_is_the_authors_box_in_the_page() {
    let source = r#"component App
  state said = "nothing"
  state name = ""
  action askMine
    showModal("mine")
  action askPlain
    showModal("plain")
  action keep
    said = "kept"
  action edit(v: string)
    name = v
  view
    column width=360 height=640
      text said testId="said"
      button press=askMine testId="ask-mine" width=100 height=40
        text "Mine"
      button press=askPlain testId="ask-plain" width=100 height=40
        text "Plain"
      button press=keep testId="poke" width=100 height=40
        text "Poke"
      dialog id="mine" closedby="any" background-color="rgb(255, 244, 244)" border-radius=24 padding=24 width=280
        text "Your own box"
        input value=name input=edit testId="mine-field" width=200 height=30
        button press=keep commandfor="mine" command="close" testId="mine-ok" background-color="rgb(181, 0, 0)" border-radius=999
          text "Got it"
      dialog id="plain"
        text "Laid out on the platform's surface"
        button commandfor="plain" command="close"
          text "OK"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (360., 640.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    struct NoopPainter;
    impl SurfacePainter for NoopPainter {
        fn record(
            &mut self,
            _size: (f32, f32),
            _scale: f32,
            _paint: &mut dyn FnMut(&mut dyn crate::paint::Backend),
        ) -> Vec<u32> {
            Vec::new()
        }
    }
    p.serve_windows(Box::new(NoopPainter));
    p.frame();

    let id_of = |p: &Presenter<()>, test_id: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
    };
    let named = |p: &Presenter<()>, name: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_id(name)[0]).unwrap().id
    };
    let tap_id = |p: &mut Presenter<()>, test_id: &str| {
        let view = id_of(p, test_id);
        p.tap(view).unwrap();
        p.run_commands(|| ());
        p.frame();
    };
    let says = |p: &Presenter<()>, needle: &str| {
        p.host()
            .runner()
            .journal()
            .filter(|line| line.contains(needle))
            .count()
    };

    tap_id(&mut p, "ask-plain");
    assert!(p.alert_root().is_none() && p.surface().is_none());
    assert_eq!(says(&p, "showModal \"plain\" refused"), 1);

    tap_id(&mut p, "ask-mine");
    assert!(p.surface().is_none(), "no window of the platform's");
    assert_eq!(p.alert_root(), Some(named(&p, "mine")));
    assert_eq!(says(&p, "showModal \"mine\" refused"), 0);
    assert!(
        !p.alert_allows(id_of(&p, "poke")),
        "the page under it is inert"
    );
    assert!(p.alert_allows(id_of(&p, "mine-field")), "its field is live");
    assert!(p.back_goes());
    assert!(p.system_back(1.0));
    p.run_commands(|| ());
    assert!(p.alert_root().is_none());

    tap_id(&mut p, "ask-mine");
    assert_eq!(p.alert_root(), Some(named(&p, "mine")));
    tap_id(&mut p, "mine-ok");
    assert!(p.alert_root().is_none());
    assert_eq!(
        p.host().runner().slot("said"),
        Some(&exact_runner::Value::str("kept"))
    );
}
