//! A long list in a sheet at a lower detent (expo/exact#842).
use super::*;

const LIST: &str = r#"routes nav
  home "/"
  pick "/pick"
component App
  action follow(url: string)
    nav = go(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" navigate=follow viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          section display="flex" flex-direction="column" position="absolute" inset=0 navigationKey=`${e.id}`
            button press=follow("/pick") testId="open"
              text "Open"
        if e.name == "pick"
          section display="flex" flex-direction="column" position="absolute" inset=0 navigationKey=`${e.id}` navigationPresentation="modal" navigationDetent="50% large" testId="sheet"
            scroll flex=1 min-height=0 testId="rows"
              for i in range(40) key=i
                text `Row ${i}` padding=16 testId=`row-${i}`
"#;

#[test]
fn a_list_drag_raises_its_sheet_before_the_list_scrolls() {
    let mut p = phone_source(LIST);
    tap(&mut p, "open");
    after(&mut p, 1000.);
    let (_, before) = p.top_sheet().unwrap();
    let y = before.1 + SHEET_HANDLE + 60.;
    let rows = id(&p, "rows");
    let row = p.hit(200., y).unwrap();
    let probe =
        serde_json::json!({"id":row,"scrollDrag":[0,-100],"from":[200,y],"ms":200,"probe":true});
    let reply: serde_json::Value =
        serde_json::from_str(&p.agent_scroll_drag(&probe).unwrap()).unwrap();
    assert_eq!(reply["scrollDrag"], "presenter");
    let under = p.scrollers_at(200., y);
    assert!(under.chain.iter().any(|s| s.id == rows && s.axes.1));
    let t = p.host().now();
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y - 100., t + 50.).unwrap();
    assert!(p.stage_follows_finger(), "a list drag must raise the sheet");
    assert!(near(p.top_sheet().unwrap().1 .1, before.1 - 100.));
    p.pointer_up(200., y - 120., t + 60.).unwrap();
    after(&mut p, 1000.);
    assert!(near(p.top_sheet().unwrap().1 .1, STATUS));
    let y = STATUS + SHEET_HANDLE + 60.;
    p.reader_under(200., y, 1.);
    assert!(
        !p.scrollers_at(200., y).claims.1,
        "scrolling stays the reader's"
    );
    p.reader_wheel(rows, 0., 10000.).unwrap();
    p.frame();
    let last = rect(&mut p, "row-39");
    assert!(
        last.1 >= STATUS && last.1 + last.3 <= H - FOOT + 0.5,
        "{last:?}"
    );
}

#[test]
fn a_nested_lists_downward_drag_lowers_the_sheet_only_at_the_lists_start() {
    let mut p = phone_source(LIST);
    tap(&mut p, "open");
    after(&mut p, 1000.);
    let rows = id(&p, "rows");
    let (_, sheet) = p.top_sheet().unwrap();
    let y = sheet.1 + SHEET_HANDLE + 60.;
    assert_eq!(p.reader_under(200., y, 1.)[0] & 31, 0b110);
    p.reader_wheel(rows, 0., 100.).unwrap();
    p.frame();
    assert_eq!(
        p.reader_under(200., y, 1.)[0] & 31,
        0b1000,
        "only up raises the sheet"
    );
    let t = p.host().now();
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y + 30., t + 50.).unwrap();
    assert!(
        !p.stage_follows_finger(),
        "a scrolled list keeps downward drags"
    );
    p.pointer_cancel(t + 60.).unwrap();
    p.reader_wheel(rows, 0., -100.).unwrap();
    p.frame();
    // A fast upward release takes the higher detent, even after a short drag.
    let t = p.host().now();
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y - 40., t + 20.).unwrap();
    p.pointer_up(200., y - 60., t + 30.).unwrap();
    after(&mut p, 1000.);
    let (_, tall) = p.top_sheet().unwrap();
    assert!(near(tall.1, STATUS));
    let y = tall.1 + SHEET_HANDLE + 60.;
    assert_eq!(
        p.reader_under(200., y, 1.)[0] & 31,
        0b10000,
        "only down lowers a tall sheet"
    );
    // Upward movement is left to the reader; no sheet gesture survives it.
    let t = p.host().now();
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y - 30., t + 30.).unwrap();
    assert!(!p.stage_follows_finger());
    p.pointer_cancel(t + 40.).unwrap();
    let t = p.host().now();
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y + 30., t + 20.).unwrap();
    assert!(p.stage_follows_finger());
    p.pointer_cancel(t + 30.).unwrap();
    after(&mut p, 1000.);
    assert!(
        near(p.top_sheet().unwrap().1 .1, tall.1),
        "cancel releases with no fling"
    );
    let t = p.host().now();
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y + 40., t + 20.).unwrap();
    p.pointer_up(200., y + 60., t + 30.).unwrap();
    after(&mut p, 1000.);
    assert!(
        near(p.top_sheet().unwrap().1 .1, sheet.1),
        "a downward fling takes the lower detent"
    );

    let mut p = phone_source(&LIST.replace("50% large", "large"));
    tap(&mut p, "open");
    after(&mut p, 1000.);
    assert_eq!(
        p.reader_under(200., y, 1.)[0] & 31,
        0,
        "a single-detent sheet leaves its list alone"
    );
}

#[test]
fn an_authored_vertical_gesture_in_the_list_keeps_its_drag() {
    let source = LIST
        .replace(
            "  action back",
            "  action drag(dx: number, dy: number)\n    nav = nav\n  action back",
        )
        .replace(
            "scroll flex=1",
            "scroll pan=drag touch-action=\"none\" flex=1",
        );
    let mut p = phone_source(&source);
    tap(&mut p, "open");
    after(&mut p, 1000.);
    let (_, sheet) = p.top_sheet().unwrap();
    let y = sheet.1 + SHEET_HANDLE + 60.;
    let under = p.scrollers_at(200., y);
    assert!(under.claims.1);
    assert!(!under.sheet);
    let t = p.host().now();
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y - 100., t + 50.).unwrap();
    assert!(!p.stage_follows_finger(), "the authored pan owns the drag");
    p.pointer_cancel(t + 60.).unwrap();
    assert!(near(p.top_sheet().unwrap().1 .1, sheet.1));
    let rows = id(&p, "rows");
    assert!(p
        .agent_scroll_drag(&serde_json::json!({
            "id": rows, "scrollDrag": [0, -100], "from": [200, y], "ms": 250
        }))
        .unwrap_err()
        .contains("gesture owns"));
}

#[test]
fn a_horizontal_pan_keeps_its_contact_after_vertical_drift() {
    let source = LIST
        .replace("  action back", "  state pans = 0\n  action drag(dx: number, dy: number)\n    pans = pans + 1\n  action back")
        .replace("scroll flex=1", "scroll pan=drag touch-action=\"pan-y\" flex=1")
        .replace("            scroll", "            text `${pans}` testId=\"pans\"\n            scroll");
    let mut p = phone_source(&source);
    tap(&mut p, "open");
    after(&mut p, 1000.);
    let (_, sheet) = p.top_sheet().unwrap();
    let y = sheet.1 + SHEET_HANDLE + 60.;
    let under = p.scrollers_at(100., y);
    assert!(under.claims.0 && under.sheet);
    let t = p.host().now();
    p.pointer_down(100., y, t).unwrap();
    p.pointer_move(200., y, t + 30.).unwrap();
    p.pointer_move(200., y - 12., t + 50.).unwrap();
    assert!(
        !p.stage_follows_finger(),
        "horizontal movement wins the axis"
    );
    assert_eq!(
        p.host
            .kernel()
            .node(id(&p, "pans"))
            .unwrap()
            .props
            .str(PropId::Text),
        Some("2")
    );
    p.pointer_move(200., y - 150., t + 100.).unwrap();
    assert!(!p.stage_follows_finger(), "later drift cannot take the pan");
    assert_eq!(
        p.host
            .kernel()
            .node(id(&p, "pans"))
            .unwrap()
            .props
            .str(PropId::Text),
        Some("3")
    );
    p.pointer_cancel(t + 110.).unwrap();
    assert!(near(p.top_sheet().unwrap().1 .1, sheet.1));
}

#[test]
fn short_refreshing_lists_keep_downward_pulls_in_sheets() {
    refreshing_lists_keep_pulls(&[0, 1]);
}

#[test]
fn overflowing_refreshing_lists_keep_downward_pulls_in_sheets() {
    refreshing_lists_keep_pulls(&[40]);
}

fn refreshing_lists_keep_pulls(counts: &[usize]) {
    for detents in ["50% large", "large"] {
        for &count in counts {
            let source = LIST
                .replace("  action back", "  state refreshes = 0\n  action reload\n    refreshes = refreshes + 1\n  action back")
                .replace("50% large", detents)
                .replace("range(40)", &format!("range({count})"))
                .replace("scroll flex=1", "scroll refresh=reload flex=1")
                .replace("            scroll", "            text `${refreshes}` testId=\"refreshes\"\n            scroll");
            let mut p = phone_source(&source);
            tap(&mut p, "open");
            after(&mut p, 1000.);
            let (_, sheet) = p.top_sheet().unwrap();
            let y = sheet.1 + SHEET_HANDLE + 60.;
            let rows = id(&p, "rows");
            let under = p.scrollers_at(200., y);
            let list = under.chain.iter().find(|s| s.id == rows).unwrap();
            assert!(list.axes.1);
            assert_eq!(list.max.1 > 0., count == 40);
            let expected = if detents == "50% large" && count == 40 {
                8
            } else {
                0
            };
            assert_eq!(
                p.reader_under(200., y, 1.)[0] & 31,
                expected,
                "{detents}, {count} rows"
            );
            let t = p.host().now();
            p.pointer_down(200., y, t).unwrap();
            p.pointer_move(200., y + 100., t + 50.).unwrap();
            assert!(!p.stage_follows_finger(), "refresh keeps a downward pull");
            p.pointer_cancel(t + 60.).unwrap();
            assert_eq!(p.reader_refresh(rows), None);
            assert_eq!(
                p.host
                    .kernel()
                    .node(id(&p, "refreshes"))
                    .unwrap()
                    .props
                    .str(PropId::Text),
                Some("1")
            );
            assert!(near(p.top_sheet().unwrap().1 .1, sheet.1));
        }
    }
}

#[test]
fn an_agent_scroll_drag_accepts_a_sheet_only_claim() {
    let mut p = phone_source(LIST);
    tap(&mut p, "open");
    after(&mut p, 1000.);
    let (_, sheet) = p.top_sheet().unwrap();
    let y = sheet.1 + SHEET_HANDLE + 60.;
    let rows = id(&p, "rows");
    for dy in [400, -400] {
        let request =
            serde_json::json!({"id": rows, "scrollDrag": [0, dy], "from": [200, y], "ms": 250});
        p.agent_scroll_drag(&request).unwrap();
        let mut probe = request;
        probe["probe"] = true.into();
        let reply: serde_json::Value =
            serde_json::from_str(&p.agent_scroll_drag(&probe).unwrap()).unwrap();
        assert_eq!(reply["scrollDrag"], "reader");
    }
    assert!(near(p.scroll[&rows].1, 400.));
    assert!(near(p.top_sheet().unwrap().1 .1, sheet.1));
}

#[test]
fn reader_shares_a_sheet_only_with_the_route_hit_on_down() {
    let mut p = phone_source(LIST);
    tap(&mut p, "open");
    after(&mut p, 1000.);
    let (_, bounds) = p.top_sheet().unwrap();
    let words = p.reader_under(200., bounds.1 + SHEET_HANDLE + 55., 1.);
    assert_ne!(words[0] & 32, 0);
    let at = 3 + 11 * words[1] as usize;
    let modal = exact_kernel::NodeKey {
        index: words[at] as u32,
        generation: words[at + 1] as u32,
    };
    let top = f32::from_bits(words[at + 3] as u32);
    let now = p.host().now();
    p.reader_sheet_drag(
        exact_kernel::NodeKey {
            generation: modal.generation.wrapping_add(1),
            ..modal
        },
        top,
        None,
        now,
    );
    assert!(near(p.top_sheet().unwrap().1 .1, bounds.1));
    p.reader_sheet_drag(modal, top, None, now);
    assert!(near(p.top_sheet().unwrap().1 .1, top));
    assert!(p.stage_follows_finger());
    p.reader_sheet_drag(modal, top, Some(0.), now);
    assert!(!p.stage_follows_finger());
    after(&mut p, 1000.);
    assert!(near(p.top_sheet().unwrap().1 .1, STATUS));
}
