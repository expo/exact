use super::*;
use crate::presenter::PainterChoice;
use ::accesskit::{ActionRequest, TextPosition, TextSelection};

fn window(source: &str) -> Presenter<()> {
    let (p, warning) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (320., 240.),
        1.,
        Default::default(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(warning.is_none(), "{warning:?}");
    p
}

fn update(tree: &mut WindowTree, p: &mut Presenter<()>) -> Option<TreeUpdate> {
    let (update, changed) = tree.update(p, "Editor", (320., 240.), 1., false, |_| false);
    changed.then_some(update)
}

fn request(action: Action, target_node: NodeId, data: Option<ActionData>) -> ActionRequest {
    ActionRequest {
        action,
        target_node,
        data,
        target_tree: TreeId::ROOT,
    }
}

#[test]
fn focus_click_and_caret_requests_address_the_published_controls() {
    let mut p = window(
        r#"component Editor
  state status = "Ready"
  action save
    status = "Saved"
  view
    column
      button "Save" press=save
      input aria-label="Draft" value="A😀Z"
      box tabindex=0 aria-label="Preview" height=30
      text status
"#,
    );
    let mut tree = WindowTree::default();
    let first = update(&mut tree, &mut p).unwrap();
    let root = &first.nodes.iter().find(|(id, _)| *id == ROOT).unwrap().1;
    assert_eq!(first.focus, ROOT);
    assert!(!root.supports_action(Action::Focus));
    let named = |name| {
        first
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(name))
            .unwrap()
            .0
    };
    let (save, field) = (named("Save"), named("Draft"));
    tree.action(&mut p, request(Action::Focus, save, None), 40.);
    assert_eq!(update(&mut tree, &mut p).unwrap().focus, save);
    tree.action(&mut p, request(Action::Click, save, None), 40.);
    let changed = update(&mut tree, &mut p).unwrap();
    assert!(changed
        .nodes
        .iter()
        .any(|(_, n)| n.label() == Some("Saved")));
    tree.action(&mut p, request(Action::Focus, field, None), 40.);
    assert_eq!(update(&mut tree, &mut p).unwrap().focus, field);
    let run = first
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::TextRun && n.value() == Some("A😀Z"))
        .unwrap()
        .0;
    // Two AccessKit graphemes in this run span three UTF-16 units in the editor.
    let caret = TextPosition {
        node: run,
        character_index: 2,
    };
    let selection = TextSelection {
        anchor: caret,
        focus: caret,
    };
    tree.action(
        &mut p,
        request(
            Action::SetTextSelection,
            field,
            Some(ActionData::SetTextSelection(selection)),
        ),
        40.,
    );
    let changed = update(&mut tree, &mut p).unwrap();
    let node = &changed.nodes.iter().find(|(id, _)| *id == field).unwrap().1;
    assert_eq!(node.value(), Some("A😀Z"));
    assert_eq!(node.text_selection(), Some(&selection));
    assert_eq!(p.access_selection(field.0 as i32).unwrap().start, 3);
    assert!(update(&mut tree, &mut p).is_none());
    p.key_down("Tab", 1.);
    let preview = named("Preview");
    assert_eq!(update(&mut tree, &mut p).unwrap().focus, preview);
    let preview = &first.nodes.iter().find(|(id, _)| *id == preview).unwrap().1;
    assert!(preview.supports_action(Action::Focus));
    assert!(!preview.supports_action(Action::Click));
}

#[test]
fn status_alert_and_explicit_live_settings_reach_both_desktop_adapters() {
    let mut p = window(
        r#"component App
  view
    column
      text "Status" role="status"
      text "Alert" role="alert"
      text "Slow alert" role="alert" aria-live="polite"
      text "Explicit" aria-live="assertive"
      text "Ordinary"
      column aria-live="assertive" aria-label="Updates"
        column aria-live="off"
          text "Muted"
          text "Resumed" role="status"
      text role="status"
        text "Items"
        text "Inline muted" aria-live="off"
"#,
    );
    let frame = p.display_frame().unwrap();
    p.display_complete(&frame);
    let first = update(&mut WindowTree::default(), &mut p).unwrap();
    use ::accesskit::Live;
    for (name, expected) in [
        ("Status", Some(Live::Polite)),
        ("Alert", Some(Live::Assertive)),
        ("Slow alert", Some(Live::Polite)),
        ("Explicit", Some(Live::Assertive)),
        ("Ordinary", None),
        ("Muted", Some(Live::Off)),
        ("Inline muted", Some(Live::Off)),
        ("Resumed", Some(Live::Polite)),
    ] {
        let node = &first
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(name))
            .unwrap()
            .1;
        assert_eq!(node.live(), expected, "{name}");
    }
}
