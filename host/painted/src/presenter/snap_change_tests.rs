use super::*;

fn changeable(kind: &str) -> String {
    READER_PAGER.replace("if inserted", "if false").replace(
        "\"x mandatory\"",
        &format!("(inserted ? \"{kind}\" : \"none\")"),
    )
}

#[test]
fn unchanged_none_and_proximity_or_hidden_changes_add_no_probe_paint() {
    let p = boot(READER_PAGER);
    assert_eq!(p.snap_layout.probe_paints, 0, "boot only seeds snap kinds");
    for (kind, hidden) in [
        ("none", false),
        ("x proximity", false),
        ("x mandatory", true),
    ] {
        let mut source = changeable(kind);
        if hidden {
            source = source.replace("display=\"flex\"", "display=\"none\"");
        }
        let mut p = boot(&source);
        if !hidden {
            let pager = id(&p, "pager");
            p.scroll_to(pager, (180., 0.));
            present(&mut p);
            assert_eq!(p.scroll_of(pager).0, 180.);
        }
        let before = p.snap_layout.probe_paints;
        p.tap(id(&p, "insert")).unwrap();
        assert!(!p.snap_probe_needed(), "{kind}, hidden={hidden}");
        present(&mut p);
        assert_eq!(p.snap_layout.probe_paints, before);
        assert!(!p.snap_probe_needed());
        if !hidden {
            assert_eq!(p.scroll_of(id(&p, "pager")).0, 180.);
        }
    }
    let source =
        changeable("none").replace("press=insert", "press=insert width=(inserted ? 102 : 100)");
    let mut p = boot(&source);
    p.tap(id(&p, "insert")).unwrap();
    assert!(!p.snap_probe_needed());
    present(&mut p);
    assert_eq!(p.snap_layout.probe_paints, 0);
    assert!(!p.snap_probe_needed());
}

#[test]
fn enabling_both_axes_and_reenabling_settles_once() {
    let source = r#"component App
  state enabled = false
  action toggle
    enabled = not enabled
  view
    column
      button "Toggle" press=toggle testId="toggle"
      scroll testId="pager" width=400 height=200 overflow="scroll" scroll-snap-type=(enabled ? "both mandatory" : "none")
        box width=1200 height=600
          box width=400 height=200 scroll-snap-align="start" position="absolute" left=400 top=200
"#;
    let mut p = boot(source);
    let pager = id(&p, "pager");
    for _ in 0..2 {
        p.scroll_to(pager, (260., 130.));
        present(&mut p);
        let probes = p.snap_layout.probe_paints;
        p.tap(id(&p, "toggle")).unwrap();
        present(&mut p);
        assert_eq!(p.scroll_of(pager), (400., 200.));
        assert_eq!(p.snap_layout.probe_paints, probes + 1);
        assert!(!p.snap_probe_needed());
        p.tap(id(&p, "toggle")).unwrap();
        present(&mut p);
    }
}

#[test]
fn an_enabled_axis_waits_for_the_reader_to_lift() {
    let mut p = boot(&changeable("x mandatory"));
    let pager = reader_pager(&mut p);
    p.reader_drag_began(pager);
    assert_eq!(p.reader_scrolled(pager, (260., 0.), 0), Some((260., 0.)));
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    assert_eq!(p.scroll_of(pager).0, 260.);
    assert_eq!(p.snap_layout.probe_paints, 0);
    p.reader_drag_ended(pager, (0., 0.), None, false);
    present(&mut p);
    assert_eq!(p.scroll_of(pager).0, 400.);
    assert!(!p.snap_probe_needed());
}

#[test]
fn a_pending_app_scroll_snaps_before_its_picture_is_acknowledged() {
    let source = changeable("x mandatory").replace(
        "testId=\"pager\"",
        "testId=\"pager\" scrollLeft=(inserted ? 260 : 0)",
    );
    let mut p = boot(&source);
    let pager = reader_pager(&mut p);
    p.tap(id(&p, "insert")).unwrap();
    assert!(p.pending_model_scroll(pager).is_some());
    let frame = p.display_frame().unwrap();
    assert_eq!(p.pending_model_scroll(pager).unwrap().left, Some(400.));
    assert_eq!(
        p.scroll_of(pager).0,
        0.,
        "input still belongs to the old picture"
    );
    assert!(p.display_complete(&frame));
    assert_eq!(p.box_of(pager).unwrap().scroll, Some((400., 0.)));
    assert_eq!(p.scroll_of(pager).0, 400.);
    assert!(p.pending_model_scroll(pager).is_none());
    assert!(!p.snap_probe_needed());
}

#[test]
fn a_hidden_changed_pager_snaps_when_shown_without_repeated_probes() {
    let source = changeable("x mandatory")
        .replace(
            "  state inserted = false",
            "  state inserted = false\n  state shown = true\n  action show\n    shown = true",
        )
        .replace("display=\"flex\"", "display=(shown ? \"flex\" : \"none\")")
        .replace(
            "      button testId=\"insert\"",
            "      button \"Show\" press=show testId=\"show\"\n      button testId=\"insert\"",
        );
    let source = source.replace(
        "    inserted = true",
        "    inserted = true\n    shown = false",
    );
    // Hiding can clamp to zero; the first valid snap must still be distinguishable.
    let source = source.replace(
        "        for i in [1, 2, 3] key=i",
        "        box width=\"100%\" height=200 flex-shrink=0\n        for i in [1, 2] key=i",
    );
    let mut p = boot(&source);
    let pager = id(&p, "pager");
    p.scroll_to(pager, (260., 0.));
    present(&mut p);
    assert_eq!(p.scroll_of(pager).0, 260.);
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    assert!(!p.snap_probe_needed());
    assert_eq!(p.snap_layout.probe_paints, 0);
    p.tap(id(&p, "show")).unwrap();
    // The shown range's staged clamp owns its first picture. Its ACK releases the snap.
    assert!(p.pending_model_scroll(pager).is_some());
    assert!(!p.snap_probe_needed());
    present(&mut p);
    assert!(p.snap_probe_needed());
    assert!(p.dirty());
    present(&mut p);
    assert_eq!(p.scroll_of(pager).0, 400.);
    assert_eq!(p.snap_layout.probe_paints, 1);
    assert!(!p.snap_probe_needed());
}

#[test]
fn a_viewport_bound_snap_kind_is_observed_in_its_receipt() {
    let source = changeable("x mandatory")
        .replace("component App", "shape Viewport\n  width: number\n  height: number\ncomponent App\n  resource viewport = exactViewport() as shape Viewport")
        .replace("(inserted ? \"x mandatory\" : \"none\")", "(viewport.width >= 800 ? \"x mandatory\" : \"none\")");
    let mut p = boot(&source);
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (260., 0.));
    present(&mut p);
    assert_eq!(p.scroll_of(pager).0, 260.);
    p.resize(800., 600.);
    present(&mut p);
    assert_eq!(p.scroll_of(pager).0, 0.);
    assert!(!p.snap_probe_needed());
}
