//! The Linux agent protocol's `aria-keyshortcuts`, driven through `handle`.
use super::*;

/// `aria-keyshortcuts` (the web's input-glue rule, the Apple hosts'
/// `Shortcuts.swift`): a declared chord presses its button before the
/// focus's `key` handlers hear it, and goes no further — F13–F24 as F1 —
/// with or without a focus; the paste chord is one, so a button declaring
/// Control+V takes it and the clipboard event never lands; a text field
/// keeps its plain keys; nothing behind a shown `aria-modal`.
#[test]
fn aria_keyshortcuts_press_their_button_before_the_key_handlers() {
    let plan = contract::compile(
        r#"component App
  state log = ""
  state selected = false
  state armed = false
  state modal = false
  state text = ""
  action keyed(k: string)
    log = `${log}key:${k};`
  action pasted(e: ClipboardEvent)
    log = `${log}paste:${e.text};`
  action pressed(what: string)
    log = `${log}${what};`
  action select
    selected = true
    log = `${log}s;`
  action arm
    armed = not armed
  action edit(v: string)
    text = v
  action keepKey(k: string)
    preventDefault()
    log = `${log}kept:${k};`
  action open
    modal = true
  view
    column width=300
      box key=keyed paste=pasted tabindex=0 testId="pad" width=80 height=24
      input value=text input=edit testId="field" height=24
      if armed
        button "Paste" aria-keyshortcuts="Meta+V Control+V" press=pressed("button-paste") testId="paste" height=24
      text formatShortcut("Mod+Shift+O") testId="cap"
      button "Choose" aria-keyshortcuts="Mod+Shift+O" press=pressed("choose") height=24
      button "F13" aria-keyshortcuts="F13" press=pressed("f13") testId="f13" height=24
      button "F24" aria-keyshortcuts="Shift+F24" press=pressed("f24") testId="f24" height=24
      box role="button" aria-keyshortcuts="s" press=pressed("box") height=12
      link href="https://example.org/" aria-keyshortcuts="s" press=pressed("link") height=12
        text "Anchor"
      link href="/next" role="button" tabindex=0 key=keepKey testId="anchor" height=12
        text "Next"
      input type="checkbox" checked=false key=keepKey testId="mark" height=12
      box role="textbox"
        box tabindex=0 key=keepKey testId="editor-child" height=12
      button "Focused" appearance="none" role="tab" tabindex=0 key=keyed testId="focused" height=12
      button "Elsewhere" aria-keyshortcuts="Enter Space" press=pressed("stolen") height=12
      button "Disabled" appearance="none" role="checkbox" disabled=true aria-keyshortcuts="d" press=pressed("disabled") height=12
      row role="radiogroup" appearance="none" aria-label="Tool"
        button "S" appearance="none" role="radio" aria-checked=selected aria-keyshortcuts="s" press=select testId="s" height=24
      button "arm" press=arm testId="arm" height=24
      button "open" press=open testId="open" height=24
      button "Away" appearance="none" role="link" aria-keyshortcuts="Escape" press=pressed("away") testId="away" height=24
      if modal
        column aria-modal=true testId="dialog" height=40
          button "Close" appearance="none" role="menuitem" aria-keyshortcuts="Escape" press=pressed("close") testId="close" height=24
      text log testId="log" height=20
"#,
    )
    .unwrap();
    let (mut p, boot_error) = Presenter::boot_with_fixture_fonts(
        &plan.encode(),
        (),
        (300.0, 600.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(boot_error.is_none(), "{boot_error:?}");
    let named = |p: &Presenter<()>, id: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(id)[0]).unwrap().id
    };
    let log = |p: &mut Presenter<()>| {
        let k = p.host().kernel();
        let id = k.node_by_key(k.find_by_test_id("log")[0]).unwrap().id;
        let text = k.node(id).unwrap().props.str(exact_kernel::PropId::Text);
        text.unwrap_or("").to_string()
    };
    let cap = p.host().kernel().node(named(&p, "cap")).unwrap();
    assert_eq!(
        cap.props.str(exact_kernel::PropId::Text),
        Some("Ctrl+Shift+O")
    );
    let (pad, field, root) = (named(&p, "pad"), named(&p, "field"), named(&p, "log"));
    let key = |p: &mut Presenter<()>, id: u32, chord: &str| {
        handle(p, &format!(r#"{{"op":"type","id":{id},"key":"{chord}"}}"#))
    };
    for (chord, want) in [
        ("Control+Shift+O", "key:Control;key:Shift;choose;"),
        ("F13", "f13;"),
        ("Shift+F24", "key:Shift;f24;"),
        ("F24", "key:F24;"),
        ("d", ""),
        ("s", "s;"),
    ] {
        let before = log(&mut p);
        let reply = key(&mut p, pad, chord);
        assert!(!reply.contains("error"), "{chord}: {reply}");
        assert_eq!(log(&mut p), format!("{before}{want}"), "{chord}");
    }
    let radio = p.host().kernel().node(named(&p, "s")).unwrap();
    assert_eq!(
        radio.props.bool(exact_kernel::PropId::AccessibilityChecked),
        Some(true)
    );
    let focused = named(&p, "focused");
    for (chord, name) in [("Enter", "Enter"), ("Space", " ")] {
        let before = log(&mut p);
        let reply = key(&mut p, focused, chord);
        assert!(!reply.contains("error"), "{chord}: {reply}");
        assert_eq!(
            log(&mut p),
            format!("{before}key:{name};"),
            "{chord} reaches the focused button"
        );
    }
    // A target that takes no focus (a text) leaves it where it is; the
    // page's shortcuts hear the key all the same.
    let before = log(&mut p);
    assert!(!key(&mut p, root, "F13").contains("error"));
    assert_eq!(log(&mut p), format!("{before}f13;"));
    for (target, chord) in [("anchor", "Enter"), ("editor-child", "s"), ("mark", "s")] {
        let before = log(&mut p);
        let id = named(&p, target);
        let reply = key(&mut p, id, chord);
        assert!(!reply.contains("error"), "{target}: {reply}");
        assert_eq!(log(&mut p), format!("{before}kept:{chord};"));
    }
    // A field keeps its plain keys: `s` is typed, not the shortcut.
    let before = log(&mut p);
    key(&mut p, field, "s");
    assert_eq!(log(&mut p), before, "a field's plain key is its typing");
    assert_eq!(
        p.host()
            .kernel()
            .node(field)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Value),
        Some("s")
    );
    // The paste chord: the key handler, then the paste, until a button declares it.
    let paste = |p: &mut Presenter<()>| {
        handle(
            p,
            &format!(r#"{{"op":"type","id":{pad},"clipboard":"paste","text":"x"}}"#),
        )
    };
    let before = log(&mut p);
    paste(&mut p);
    assert_eq!(log(&mut p), format!("{before}key:Control;key:v;paste:x;"));
    p.tap(named(&p, "arm")).unwrap();
    let before = log(&mut p);
    paste(&mut p);
    assert_eq!(
        log(&mut p),
        format!("{before}key:Control;button-paste;"),
        "Control's own keydown, then the button took the chord"
    );
    // Behind a shown `aria-modal`, only its own shortcuts.
    let before = log(&mut p);
    key(&mut p, pad, "Escape");
    assert_eq!(log(&mut p), format!("{before}away;"));
    p.tap(named(&p, "open")).unwrap();
    let before = log(&mut p);
    key(&mut p, pad, "Escape");
    key(&mut p, pad, "F13");
    assert_eq!(log(&mut p), format!("{before}close;key:F13;"));
}
