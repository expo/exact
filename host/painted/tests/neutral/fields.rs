use super::{id, pixel, PainterChoice, Presenter};
use exact_painted::presenter::PointerKind;

pub(super) fn appearance_none_keeps_values() {
    for (tag, value, child) in [
        ("select", "Pear", "        option \"Pear\"\n"),
        ("input type=\"date\"", "2026-11-04", ""),
    ] {
        let source = format!(
            r##"component App
  view
    column width=300 height=100 background-color="#ffffff" color="#cc0000" line-height="32px"
      {tag} value="{value}" appearance="none" width=180 height=32 padding=0 border-width=0 testId="field"
{child}      text "{value}" width=180 height=32 testId="reference"
"##
        );
        let plan = contract::compile(&source).unwrap();
        let (mut p, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (300., 100.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let field = super::text_pixels(&mut p, "field");
        let reference = super::text_pixels(&mut p, "reference");
        assert!(reference.iter().any(|c| c[1] < 200));
        assert!(
            field == reference,
            "{tag}: no appearance keeps the value's text"
        );
    }
}

pub(super) fn focus_changes_during_flow_damage() {
    let mut failures = Vec::new();
    for scheme in ["light", "dark"] {
        for kind in ["select", "date", "time", "datetime-local"] {
            let tag = if kind == "select" {
                "select".to_string()
            } else {
                format!("input type=\"{kind}\"")
            };
            let child = if kind == "select" {
                "        option \"One\"\n"
            } else {
                ""
            };
            let source = format!(
                r##"component App
  state offset = 100
  action move(k: string)
    offset = offset + 5
    if k == "f"
      focus("field")
    if k == "n"
      focus("after")
  view
    column width=400 height=360 padding=20 gap=10 align-items="flex-start"
      {tag} id="field" testId="field" width=160 height=40 accent-color="#008800" key=move
{child}      button "After" id="after" testId="after" key=move
      view width=300 height=200 overflow="hidden"
        text "one two three four five six seven eight nine ten one two three four five six seven eight nine ten" width=300 height=200 testId="para"
        view position="absolute" left=offset top=0 width=80 height=60 wrap-flow="both"
"##
            );
            let plan = contract::compile(&source).unwrap();
            let (mut p, error) = Presenter::boot_with(
                &plan.encode(),
                (),
                (440.0, 400.0),
                1.0,
                std::path::PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.set_system_scheme(scheme == "dark");
            let field = id(&p, "field");
            let before = p.frame();
            let unfocused = before.pixel(100, 21).unwrap();
            assert!(!p.paragraph(id(&p, "para")).unwrap().fragments().is_empty());
            key(&mut p, "after", "m");
            p.frame();
            assert!(
                !p.damage_rects().is_empty(),
                "{scheme} {kind}: flow-only damage"
            );
            for (target, command, focused) in [("after", "f", true), ("field", "n", false)] {
                key(&mut p, target, command);
                p.run_commands(<()>::default);
                assert_eq!(p.focus() == Some(field), focused);
                // Read the first frame: another paint would clear the flow damage
                // and hide a stale or missing ring behind a full repaint.
                let frame = p.frame();
                let expected = if focused {
                    [0, 136, 0]
                } else {
                    [unfocused.red(), unfocused.green(), unfocused.blue()]
                };
                let c = frame.pixel(100, 21).unwrap();
                let actual = [c.red(), c.green(), c.blue()];
                if actual != expected || !p.damage_rects().is_empty() {
                    failures.push(format!("{scheme} {kind}, focused={focused}: ring {actual:?} != {expected:?}, damage {:?}", p.damage_rects()));
                }
                // Establish the correct previous frame for the next transition.
                p.frame();
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

pub(super) fn select_and_date_focus() {
    let mut failures = Vec::new();
    for scheme in ["light", "dark"] {
        for kind in ["select", "date", "time", "datetime-local"] {
            for bare in [false, true] {
                let tag = if kind == "select" {
                    "select".to_string()
                } else {
                    format!("input type=\"{kind}\"")
                };
                let child = if kind == "select" {
                    "        option \"One\" value=\"one\"\n"
                } else {
                    ""
                };
                let appearance = if bare { "none" } else { "auto" };
                let source = format!(
                    r##"component App
  state invalid = false
  state disabled = false
  action change(k: string)
    if k == "v"
      invalid = !invalid
  action focusField
    focus("field")
  action disable
    disabled = true
  view
    column width=300 height=280 padding=20 gap=10 align-items="flex-start" color-scheme="{scheme}" background-color="#204060"
      button "Before" testId="before"
      {tag} id="field" testId="field" width=160 height=40 appearance="{appearance}" accent-color="#008800" aria-invalid=(invalid ? "true" : "false") disabled=disabled key=change
{child}      button "Focus" press=focusField testId="focus"
      button "Disable" press=disable testId="disable"
      row width=20 height=20 background-color="-exact-system-red" testId="red"
"##
                );
                let plan = contract::compile(&source).unwrap();
                let (mut p, error) = Presenter::boot_with(
                    &plan.encode(),
                    (),
                    (300.0, 280.0),
                    1.0,
                    std::path::PathBuf::new(),
                    PainterChoice::Cpu,
                )
                .unwrap();
                let _roles = p.host().runner().bind_roles();
                assert!(error.is_none(), "{error:?}");
                let field = id(&p, "field");
                // The neutral border occupies pixel 0; a ring also paints pixel 1.
                let unfocused = pixel(&mut p, "field", 80.0, 1.0);
                let red = pixel(&mut p, "red", 10.0, 10.0);
                let context = format!("{scheme} {kind} appearance={appearance}");
                for programmatic in [false, true] {
                    focus(&mut p, programmatic);
                    for (step, invalid) in [false, true, false].into_iter().enumerate() {
                        assert_eq!(p.focus(), Some(field), "{context}");
                        // A native field a press's action focuses shows no ring until
                        // a key, as Chrome's `:focus-visible` has it for `focus()`.
                        let expected = if !bare && programmatic && step == 0 {
                            unfocused
                        } else if invalid && !bare {
                            red
                        } else {
                            [0, 136, 0]
                        };
                        let actual = pixel(&mut p, "field", 80.0, 1.0);
                        if actual != expected {
                            failures.push(format!("{context}, programmatic={programmatic}, invalid={invalid}: {actual:?} != {expected:?}"));
                        }
                        key(&mut p, "field", "Tab");
                        assert_ne!(p.focus(), Some(field), "{context}: blur");
                        assert_eq!(
                            pixel(&mut p, "field", 80.0, 1.0),
                            unfocused,
                            "{context}: blur, invalid={invalid}"
                        );
                        focus(&mut p, programmatic);
                        if step < 2 {
                            key(&mut p, "field", "v");
                        }
                    }
                }
                for pointer in [PointerKind::Touch, PointerKind::Mouse] {
                    p.set_pointer_kind(pointer);
                    let (x, y, w, h) = p.rect_of(field).unwrap();
                    p.press_at(x + w / 2.0, y + h / 2.0, 0.0);
                    assert_eq!(p.focus(), Some(field), "{context}: {pointer:?}");
                    // A native field's press focus shows no ring, by finger or mouse, as
                    // WinUI's ComboBox and DatePicker and GTK's drop-down have
                    // it (LLP 1115.004 D9.2); Chrome rings a clicked select.
                    let expected = if bare { [0, 136, 0] } else { unfocused };
                    let actual = pixel(&mut p, "field", 80.0, 1.0);
                    if actual != expected {
                        failures.push(format!(
                            "{context}, {pointer:?}: {actual:?} != {expected:?}"
                        ));
                    }
                    if kind == "select" {
                        p.press_at(0.0, 0.0, 0.0);
                    }
                }
                p.tap(id(&p, "disable")).unwrap();
                p.tap(id(&p, "focus")).unwrap();
                p.run_commands(<()>::default);
                assert_ne!(p.focus(), Some(field), "{context}: disabled focus refused");
                let disabled = pixel(&mut p, "field", 80.0, 1.0);
                key(&mut p, "before", "Tab");
                assert_ne!(p.focus(), Some(field), "{context}: disabled Tab skipped");
                assert_eq!(pixel(&mut p, "field", 80.0, 1.0), disabled);
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn focus(p: &mut Presenter<()>, programmatic: bool) {
    if programmatic {
        p.tap(id(p, "focus")).unwrap();
        p.run_commands(<()>::default);
    } else {
        key(p, "before", "Tab");
    }
}

fn key(p: &mut Presenter<()>, target: &str, key: &str) {
    let target = id(p, target);
    p.type_key(target, key, key, true, false).unwrap();
    p.type_key(target, key, key, false, false).unwrap();
}

pub(super) fn bare_size_and_empty_date_regressions() {
    let mut failures = Vec::new();
    for kind in ["date", "time", "datetime-local"] {
        let source = format!(
            r##"component App
  state bare = false
  action toggle
    bare = !bare
  view
    column width=300 height=300 align-items="flex-start"
      input type="{kind}" appearance=(bare ? "none" : "auto") testId="field"
      input type="{kind}" testId="reference"
      button "Toggle" press=toggle testId="toggle"
"##
        );
        let (mut p, error) = Presenter::boot_with(
            &contract::compile(&source).unwrap().encode(),
            (),
            (300., 300.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let field = id(&p, "field");
        let original = super::size(&mut p, "reference");
        assert_eq!(super::size(&mut p, "field"), original);
        for _ in 0..2 {
            p.tap(id(&p, "toggle")).unwrap();
            p.frame();
            assert_eq!(id(&p, "field"), field, "same control");
        }
        let restored = super::size(&mut p, "field");
        if restored != original {
            failures.push(format!(
                "{kind}: restored {restored:?}, native {original:?}"
            ));
        }
        for hint in ["", "Add a date"] {
            let source = format!(
                r##"component App
  view
    column width=300 height=100 background-color="#ffffff" color="#cc0000" line-height="32px"
      input type="{kind}" appearance="none" placeholder="{hint}" width=180 height=32 padding=0 border-width=0 testId="field"
      input appearance="none" placeholder="{hint}" width=180 height=32 padding=0 border-width=0 testId="reference"
"##
            );
            let (mut p, error) = Presenter::boot_with(
                &contract::compile(&source).unwrap().encode(),
                (),
                (300., 100.),
                1.,
                Default::default(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            if super::text_pixels(&mut p, "field") != super::text_pixels(&mut p, "reference") {
                failures.push(format!(
                    "empty {kind}, hint={hint:?}: differs from bare text"
                ));
            }
        }
    }
    for populated in [false, true] {
        let source = format!(
            r##"component App
  state populated = {populated}
  action clear
    populated = false
  view
    column width=300 height=160 align-items="flex-start" line-height="24px"
      select appearance="none" testId="field"
        if populated
          option "A wide option"
      button "Clear" press=clear testId="clear"
"##
        );
        let (mut p, error) = Presenter::boot_with(
            &contract::compile(&source).unwrap().encode(),
            (),
            (300., 160.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        if populated {
            p.tap(id(&p, "clear")).unwrap();
        }
        let size = super::size(&mut p, "field");
        let logs = exact_runner::agent::logs(p.host().runner(), 0);
        if logs.contains("InvalidIntrinsicSize") {
            failures.push(format!(
                "empty select, initially populated={populated}: InvalidIntrinsicSize"
            ));
        }
        if size != (1., 24.) {
            failures.push(format!(
                "empty select, initially populated={populated}: {size:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
