//! A finger under the Material look keeps the control it chose at down
//! (LLP 1115.001.001 §5 Q5): through a slide into the control's own margin,
//! past a control `pointerdown` reveals, and from outside every layout box.
use super::material_tests::{id, material_process, said};
use super::*;

const TOUCH: &str = r#"component App
  state checked = false
  state revealed = false
  action change(value: bool)
    checked = value
  action reveal
    revealed = true
  view
    column width=400 height=400 align-items="flex-start"
      input type="checkbox" checked=checked change=change testId="check" flex-shrink=0 margin-left=32 margin-top=32
      text `${checked}` testId="said"
"#;

fn touch(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.slop = 8.;
    p.set_pointer_kind(PointerKind::Touch);
    p.frame();
    p
}

#[test]
fn materials_touch_keeps_the_down_control_while_sliding_into_its_margin() {
    if !material_process() {
        return;
    }
    let mut p = touch(TOUCH);
    let r = p.rect_of(id(&p, "check")).unwrap();
    let (x, y) = (r.0 + 12., r.1 + r.3 - 2.);
    p.pointer_down(x, y, 0.).unwrap();
    assert_eq!(p.brush.feedback.1, Some(id(&p, "check")));
    p.pointer_move(x, y + 6., 1.).unwrap();
    assert_ne!(p.hit(x, y + 6.), Some(id(&p, "check")));
    p.pointer_up(x, y + 6., 2.).unwrap();
    assert_eq!(said(&p), "true");
}

#[test]
fn materials_touch_does_not_choose_a_control_revealed_by_pointerdown() {
    if !material_process() {
        return;
    }
    let source = TOUCH
        .replace(
            "column width=400 height=400",
            "column width=400 height=400 pointerdown=reveal",
        )
        .replace(
            "      input type=",
            "      if revealed\n        input type=",
        );
    let mut p = touch(&source);
    let (x, y) = (26., 44.);
    assert!(p.host.kernel().find_by_test_id("check").is_empty());
    p.pointer_down(x, y, 0.).unwrap();
    p.frame();
    let hit = p.hit(x, y);
    assert_eq!(
        p.touch_target(hit, x, y),
        Some(id(&p, "check")),
        "the newly visible control is eligible at release"
    );
    p.pointer_up(x, y, 1.).unwrap();
    assert_eq!(said(&p), "false");

    let source = TOUCH
        .replace("state revealed = false", "state revealed = false\n  state second = false\n  action other(value: bool)\n    second = value")
        .replace("column width=400 height=400", "column width=400 height=400 position=\"relative\" pointerdown=reveal")
        .replace("      text `${checked}`", "      if revealed\n        input type=\"checkbox\" checked=second change=other position=\"absolute\" left=70 top=32 testId=\"second\"\n      text `${checked}/${second}`");
    let mut p = touch(&source);
    let (x, y) = (66., 44.);
    let before = p.hit(x, y);
    assert_eq!(p.touch_target(before, x, y), Some(id(&p, "check")));
    p.pointer_down(x, y, 0.).unwrap();
    assert_eq!(p.brush.feedback.1, None);
    p.frame();
    let after = p.hit(x, y);
    assert_eq!(after, before, "the geometric hit stays the parent");
    assert_eq!(p.touch_target(after, x, y), Some(id(&p, "second")));
    p.pointer_up(x, y, 1.).unwrap();
    assert_eq!(said(&p), "true/false", "the down target keeps the touch");
}

#[test]
fn materials_touch_can_start_outside_every_layout_box() {
    if !material_process() {
        return;
    }
    let mut p = touch(
        r#"component App
  state checked = false
  action change(value: bool)
    checked = value
  view
    input type="checkbox" checked=checked change=change testId="check"
"#,
    );
    let check = id(&p, "check");
    let r = p.rect_of(check).unwrap();
    let (x, y) = (r.0 + r.2 + 6., r.1 + 12.);
    assert_eq!((r.2, r.3), (24., 24.));
    assert_eq!(p.hit(x, y), None);
    assert!(p.pointer_down(x, y, 0.).unwrap());
    assert_eq!(p.contact_hit(), None);
    assert_eq!(p.brush.feedback.1, None);
    p.pointer_up(x, y, 1.).unwrap();
    assert_eq!(
        p.host
            .kernel()
            .node(check)
            .unwrap()
            .props
            .bool(PropId::Checked),
        Some(true)
    );
    p.set_pointer_kind(PointerKind::Mouse);
    assert!(!p.pointer_down(x, y, 2.).unwrap());
    p.pointer_up(x, y, 3.).unwrap();
    assert_eq!(
        p.host
            .kernel()
            .node(check)
            .unwrap()
            .props
            .bool(PropId::Checked),
        Some(true)
    );
}

#[test]
fn materials_touch_retires_its_activation_target_before_a_late_release() {
    if !material_process() {
        return;
    }
    for change in [
        "disabled=retired",
        "visibility=(retired ? \"hidden\" : \"visible\")",
        "display=(retired ? \"none\" : \"block\")",
        "inert=retired",
        "pointer-events=(retired ? \"none\" : \"auto\")",
    ] {
        let source = TOUCH
            .replace("state revealed = false", "state revealed = false\n  state retired = false\n  action retire\n    retired = true")
            .replace("margin-left=32", &format!("{change} margin-left=32"))
            + "      button \"Retire\" press=retire testId=\"retire\"\n";
        let mut p = touch(&source);
        let r = p.rect_of(id(&p, "check")).unwrap();
        let (x, y) = (r.0 - 6., r.1 + 12.);
        p.pointer_down(x, y, 0.).unwrap();
        assert!(p.contact.is_some(), "{change}");
        let retire = id(&p, "retire");
        assert_eq!(p.host.dispatch_at(retire, Event::Press, 1.), None);
        assert_eq!(p.after_commit(), None);
        assert!(
            p.contact.is_some(),
            "{change}: the column keeps its contact"
        );
        p.pointer_up(x, y, 2.).unwrap();
        assert_eq!(said(&p), "false", "{change}");
    }
    let source = TOUCH
        .replace(
            "action reveal\n    revealed = true",
            "action reveal\n    revealed = !revealed",
        )
        .replace(
            "      input type=",
            "      if !revealed\n        input type=",
        )
        + "      button \"Retire\" press=reveal testId=\"retire\"\n";
    let mut p = touch(&source);
    let key = p.host.kernel().node(id(&p, "check")).unwrap().key;
    p.pointer_down(26., 44., 0.).unwrap();
    let retire = id(&p, "retire");
    for ms in [1., 2.] {
        assert_eq!(p.host.dispatch_at(retire, Event::Press, ms), None);
        assert_eq!(p.after_commit(), None);
    }
    assert_ne!(p.host.kernel().node(id(&p, "check")).unwrap().key, key);
    assert!(p.contact.is_some());
    p.pointer_up(26., 44., 3.).unwrap();
    assert_eq!(said(&p), "false");
}

#[test]
fn materials_touch_uses_acknowledged_geometry_during_collection_display_work() {
    if !material_process() {
        return;
    }
    let rows = (0..100)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "{TOUCH}      list virtualized=true width=300 height=180 testId=\"list\"\n        for x in [{rows}] key=x\n          text `${{x}}` height=24\n"
    );
    for (x, collapsed) in [(44., false), (26., false), (26., true)] {
        let source = if collapsed {
            source
                .replace(
                    "testId=\"check\"",
                    "testId=\"check\" width=(revealed ? 0 : 24)",
                )
                .replace(
                    "column width=400 height=400",
                    "column width=400 height=400 pointerdown=reveal",
                )
        } else {
            source.clone()
        };
        let mut p = touch(&source);
        let first = p.display_frame().unwrap();
        assert!(p.display_complete(&first));
        let list = id(&p, "list");
        p.collection.limit = Some(1);
        p.scroll.insert(list, (0., 1500.));
        p.queue_collections();
        p.pointer_down(x, 44., 0.).unwrap();
        assert!(p.contact.is_some());
        assert!(p.collection.pending());
        let epoch = p.host.kernel().epoch();
        let next = p.display_frame().unwrap();
        assert!(p.host.kernel().epoch() > epoch, "the list committed rows");
        assert!(p.contact.is_some(), "submission cancelled the contact");
        assert!(p.display_complete(&next));
        p.pointer_up(x, 44., 1.).unwrap();
        assert_eq!(said(&p), if collapsed { "false" } else { "true" });
    }
}

#[test]
fn materials_touch_retires_a_control_collapsed_by_pointerdown() {
    if !material_process() {
        return;
    }
    for (collapse, x) in [
        ("scale=(revealed ? 0 : 1)", 26.),
        ("width=(revealed ? 0 : 24)", 26.),
        ("width=(revealed ? 0 : 24) rotate=45", 44.),
    ] {
        let source = TOUCH
            .replace(
                "column width=400 height=400",
                "column width=400 height=400 pointerdown=reveal",
            )
            .replace("margin-left=32", &format!("{collapse} margin-left=32"));
        let y = 44.;
        for (repaint, moved) in [(false, false), (true, false), (true, true)] {
            let mut p = touch(&source);
            let check = id(&p, "check");
            let hit = p.hit(x, y);
            assert_eq!(p.touch_target(hit, x, y), Some(check));
            p.pointer_down(x, y, 0.).unwrap();
            if repaint {
                p.frame();
                assert_ne!(p.hit(x, y), Some(check));
            }
            if moved {
                p.pointer_move(x, y, 1.).unwrap();
                // A contact in the margin stays the column's; one on the
                // control goes with it.
                assert_eq!(p.contact.is_none(), hit == Some(check), "{collapse}");
            }
            p.pointer_up(x, y, 2.).unwrap();
            assert_eq!(
                said(&p),
                "false",
                "{collapse}: a collapsed control activated"
            );
            assert!(p.contact.is_none());
        }
    }
}

#[test]
fn materials_touch_margin_contacts_cancel_and_discover_no_ancestor_pan() {
    if !material_process() {
        return;
    }
    let source = TOUCH
        .replace(
            "state revealed = false",
            "state revealed = false\n  state pans = 0\n  action pan(dx, dy)\n    pans = pans + 1",
        )
        .replace(
            "column width=400 height=400",
            "column width=24 height=24 pan=pan touch-action=\"none\"",
        )
        .replace(
            "margin-left=32 margin-top=32",
            "margin-left=64 margin-top=64",
        )
        .replace("`${checked}`", "`${checked}/${pans}`");
    for moved in [false, true] {
        let mut p = touch(&source);
        let r = p.rect_of(id(&p, "check")).unwrap();
        let (x, y) = (r.0 + r.2 + 6., r.1 + 12.);
        assert_eq!(p.hit(x, y), None);
        assert!(p.pointer_down(x, y, 0.).unwrap());
        assert_eq!(p.contact_hit(), None);
        assert_eq!(p.brush.feedback.1, None);
        if moved {
            assert!(!p.pointer_move(x, y + 16., 1.).unwrap());
        } else {
            p.pointer_cancel(1.).unwrap();
        }
        assert!(p.contact.is_none());
        p.pointer_up(x, y, 2.).unwrap();
        assert_eq!(said(&p), "false/0");
    }
    let mut p = touch(&source.replace("column width=24 height=24", "column width=400 height=400"));
    let r = p.rect_of(id(&p, "check")).unwrap();
    let (x, y) = (r.0 + r.2 + 6., r.1 + 12.);
    assert!(p.hit(x, y).is_some());
    p.pointer_down(x, y, 0.).unwrap();
    assert!(p.pointer_move(x, y + 16., 1.).unwrap());
    p.pointer_up(x, y + 16., 2.).unwrap();
    assert_eq!(
        said(&p),
        "false/1",
        "a geometric ancestor still discovers its pan"
    );
}

#[test]
fn materials_touch_releases_only_on_a_control_the_acknowledged_picture_shows() {
    if !material_process() {
        return;
    }
    let source = TOUCH
        .replace(
            "      input type=",
            "      column testId=\"a\" width=400 height=100 align-items=\"flex-start\"\n        input type=",
        )
        .replace(
            "      text `${checked}`",
            "      column testId=\"b\" width=400 height=100\n      text `${checked}`",
        );
    for moved in [false, true] {
        let mut p = touch(&source);
        let first = p.display_frame().unwrap();
        assert!(p.display_complete(&first));
        let check = id(&p, "check");
        let r = p.rect_of(check).unwrap();
        let (x, y) = (r.0 - 6., r.1 + 12.);
        assert_eq!(p.hit(x, y), Some(id(&p, "a")));
        p.pointer_down(x, y, 0.).unwrap();
        if moved {
            let (a, b) = (id(&p, "a"), id(&p, "b"));
            p.host.apply_test_ops(&[
                exact_kernel::Op::SetChildren {
                    id: a,
                    children: vec![],
                },
                exact_kernel::Op::SetChildren {
                    id: b,
                    children: vec![check],
                },
            ]);
            p.host.layout().unwrap();
            p.dirty = true;
            p.retire_pointer();
            assert!(p.contact.is_some(), "the contact outlives the move");
            assert!(!p.display.allows(p.host.kernel(), check));
        }
        // The runner refuses a dispatch to a node moved by bare kernel ops,
        // so the release's own test is what decides here.
        let key = p.host.kernel().node(check).unwrap().key;
        assert_eq!(p.touch_activation_contains(key, None, x, y), !moved);
        p.pointer_up(x, y, 1.).unwrap();
        assert_eq!(said(&p), if moved { "false" } else { "true" });
    }
}

/// A child `pointerdown` hides leaves the press with the button round it,
/// whose press feedback the contact holds.
#[test]
fn materials_touch_on_a_buttons_child_presses_the_button_when_the_child_hides() {
    if !material_process() {
        return;
    }
    let mut p = touch(
        r#"component App
  state pressed = 0
  state hidden = false
  action hide
    hidden = true
  action press
    pressed = pressed + 1
  view
    column width=400 height=400 align-items="flex-start"
      button press=press pointerdown=hide -exact-press-scale=0.9 testId="button"
        text "Go" testId="label" visibility=(hidden ? "hidden" : "visible")
      text `${pressed}` testId="said"
"#,
    );
    let r = p.rect_of(id(&p, "label")).unwrap();
    let (x, y) = (r.0 + r.2 / 2., r.1 + r.3 / 2.);
    assert_eq!(p.hit(x, y), Some(id(&p, "label")));
    p.pointer_down(x, y, 0.).unwrap();
    p.frame();
    assert!(p.contact.is_some());
    p.pointer_up(x, y, 1.).unwrap();
    assert_eq!(said(&p), "1");
}

#[test]
fn materials_touch_keeps_an_ancestor_pan_when_pointerdown_disables_the_chosen_control() {
    if !material_process() {
        return;
    }
    let mut p = touch(
        r#"component App
  state checked = false
  state off = false
  state pans = 0
  action change(value: bool)
    checked = value
  action disable
    off = true
  action pan(dx, dy)
    pans = pans + 1
  view
    column width=400 height=400 align-items="flex-start" pan=pan touch-action="none" pointerdown=disable
      input type="checkbox" checked=checked change=change disabled=off testId="check" flex-shrink=0 margin-left=32 margin-top=32
      text `${checked}/${pans}` testId="said"
"#,
    );
    let r = p.rect_of(id(&p, "check")).unwrap();
    let (x, y) = (r.0 - 6., r.1 + 12.);
    assert_ne!(p.hit(x, y), Some(id(&p, "check")));
    p.pointer_down(x, y, 0.).unwrap();
    assert!(
        p.contact.is_some(),
        "disabling the control kept the contact"
    );
    assert!(p.pointer_move(x, y + 16., 1.).unwrap());
    p.pointer_up(x, y + 16., 2.).unwrap();
    assert_eq!(said(&p), "false/1");
}
