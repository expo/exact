//! LLP 1115.004 D9.6: in an app with a Windows host, a button chord that
//! never presses it on Windows is a warning.

use crate::scratch::Scratch;

const SOURCE: &str = "component App
  view
    column
      button aria-keyshortcuts=\"Meta+N\" aria-label=\"New\"
        text \"New\"
      button aria-keyshortcuts=\"Meta+S Control+s\" aria-label=\"Save\"
        text \"Save\"
      button aria-keyshortcuts=\"Meta+Control+F Control+F Shift+Meta+Plus\" aria-label=\"Find\"
        text \"Find\"
      button aria-keyshortcuts=\"Alt+F4 Alt+Shift+Tab Control+Escape Alt+Space F4\" aria-label=\"Close\"
        text \"Close\"
      button aria-keyshortcuts=\"Meta+Escape\" aria-label=\"Stop\"
        text \"Stop\"
      Command(chord=\"Meta+.\")
component Command
  props
    chord: string
  view
    button aria-keyshortcuts=chord aria-label=\"Command\"
      text \"Command\"
";

fn app(name: &str, manifest: Option<&str>) -> Scratch {
    let dir = Scratch::new(format!(
        "contract-windows-shortcuts-{name}-{}",
        std::process::id()
    ));
    std::fs::write(dir.join("app.contract"), SOURCE).unwrap();
    if let Some(manifest) = manifest {
        std::fs::write(dir.join("app.json"), manifest).unwrap();
    }
    dir
}

/// A `Meta` chord with no `Control` sibling names the sibling to add, in
/// the list as it would read, through a component prop too; a chord
/// Windows keeps for itself is named with what Windows does with it; a
/// sibling Windows keeps is not suggested. An app without a Windows host
/// hears none of it, and a warning refuses nothing.
#[test]
fn a_chord_windows_never_delivers_warns_in_an_app_with_a_windows_host() {
    let without = app(
        "none",
        Some(r#"{"app":{"id":"com.exact.none","name":"None"},"host":{"macos":{}}}"#),
    );
    assert!(contract::shortcut_findings(&without.join("app.contract"))
        .unwrap()
        .is_empty());
    std::fs::remove_dir_all(without).unwrap();

    let dir = app(
        "host",
        Some(r#"{"app":{"id":"com.exact.host","name":"Host"},"host":{"windows":{}}}"#),
    );
    let path = dir.join("app.contract");
    let found = contract::shortcut_findings(&path).unwrap();
    let said: Vec<_> = found
        .iter()
        .map(|e| (e.id.as_str(), e.is_warning(), e.span.line))
        .collect();
    assert_eq!(
        said,
        [
            ("analyze-shortcut-meta-only", true, 4),
            ("analyze-shortcut-meta-only", true, 8),
            ("analyze-shortcut-windows-reserved", true, 10),
            ("analyze-shortcut-windows-reserved", true, 10),
            ("analyze-shortcut-windows-reserved", true, 10),
            ("analyze-shortcut-windows-reserved", true, 10),
            ("analyze-shortcut-meta-only", true, 12),
            ("analyze-shortcut-meta-only", true, 19),
        ],
        "{found:#?}"
    );
    let message = |i: usize| found[i].message.as_str();
    assert!(
        message(0).starts_with("`aria-keyshortcuts` \"Meta+N\" has no Control chord beside it")
            && message(0).contains("add \"Control+N\" beside it: \"Meta+N Control+N\""),
        "{}",
        message(0)
    );
    // `Meta+Control+F`'s sibling is `Control+F`; only the Plus chord lacks one.
    assert!(
        message(1).contains(
            "add \"Shift+Control+Plus\" beside it: \"Meta+Control+F Control+F Shift+Meta+Plus Shift+Control+Plus\""
        ),
        "{}",
        message(1)
    );
    let reserved: Vec<&str> = (2..6).map(message).collect();
    for (chord, kept) in [
        ("Alt+F4", "closing the window"),
        ("Alt+Shift+Tab", "switching windows"),
        ("Control+Escape", "opening Start"),
        ("Alt+Space", "the window's system menu"),
    ] {
        assert!(
            reserved.iter().any(|m| m.starts_with(&format!(
                "`aria-keyshortcuts` \"{chord}\" is a chord Windows keeps for {kept}"
            ))),
            "{chord}: {reserved:#?}"
        );
    }
    assert!(
        message(6).contains("its Control chord, \"Control+Escape\", Windows keeps for opening Start, so add another"),
        "{}",
        message(6)
    );
    assert_eq!(found[7].related.len(), 1, "the use that passed \"Meta+.\"");
    assert_eq!(found[7].related[0].span.line, 14);
    let compiled = contract::compile_path_all(&path, false);
    assert!(
        compiled.is_ok(),
        "a warning refuses nothing: {:?}",
        compiled.err()
    );
    let (_, warned) = contract::compile_path_all_with_warnings(&path, false);
    assert_eq!(
        warned
            .iter()
            .filter(|e| e.id.starts_with("analyze-shortcut-"))
            .count(),
        8,
        "a build prints them: {warned:#?}"
    );
    std::fs::remove_dir_all(dir).unwrap();

    let portable = app(
        "portable",
        Some(r#"{"app":{"id":"com.exact.portable","name":"Portable"},"host":{"windows":{}}}"#),
    );
    let path = portable.join("app.contract");
    std::fs::write(&path, SOURCE.replace("Control+Escape", "Mod+Escape")).unwrap();
    let found = contract::shortcut_findings(&path).unwrap();
    assert!(found
        .iter()
        .any(|e| e.id == "analyze-shortcut-windows-reserved"
            && e.message.contains("Mod+Escape")
            && e.message.contains("opening Start")));

    std::fs::remove_dir_all(portable).unwrap();

    // A Windows crate is a Windows host too.
    let dir = app("crate", None);
    std::fs::create_dir_all(dir.join("windows")).unwrap();
    std::fs::write(dir.join("windows/Cargo.toml"), "").unwrap();
    assert_eq!(
        contract::shortcut_findings(&dir.join("app.contract"))
            .unwrap()
            .len(),
        8
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn hosts_check_chords_in_literals_list_items_conditional_arms_concatenations_and_state_initializers(
) {
    for chord in ["Hyper+O", "Alt+Alt+O", "Mod+Meta+O", "Mod+Control+O"] {
        let prefix = chord.strip_suffix('O').unwrap();
        let cases = [
            ("literal", format!("button \"Open\" aria-keyshortcuts=\"{chord}\"")),
            ("list item", format!("column\n      for item in [\"{chord}\"] key=item\n        button \"Open\" aria-keyshortcuts=item")),
            ("conditional arm", format!("button \"Open\" aria-keyshortcuts=(armed ? \"{chord}\" : \"Escape\")")),
            ("concatenation", format!("button \"Open\" aria-keyshortcuts=(\"{prefix}\" + \"O\")")),
            ("state initializer", "button \"Open\" aria-keyshortcuts=key".into()),
        ];
        for (position, view) in cases {
            let source = format!("component App\n  state armed = false\n  state key = \"{chord}\"\n  view\n    {view}\n");
            contract::compile(&source).unwrap_or_else(|e| panic!("{position}: {chord}: {e}"));
        }
    }
}

#[test]
fn the_literal_close_window_override_refuses_ambiguous_and_reserved_chords() {
    let compile = |chord: &str| {
        contract::compile(&format!("component App\n  view\n    column -exact-menu-keys=\"close-window={chord}\"\n      text \"Open\"\n"))
    };
    compile("Mod+Shift+W").unwrap();
    for chord in ["Mod+Meta+J", "Mod+Control+J", "Mod+Q"] {
        assert!(
            compile(chord)
                .unwrap_err()
                .to_string()
                .contains("lower-menu-keys"),
            "{chord}"
        );
    }
}
