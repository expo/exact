use super::*;
mod cases;

#[test]
fn position_area_region_matches_chrome() {
    for rtl in [false, true] {
        for c in cases::cases(rtl) {
            let mut p = boot(&cases::source(&c, rtl, 1., false));
            assert!(p.resize(400., 600.).is_none());
            p.frame();
            tap(&mut p, "anchor");
            let pop = id(&p, "pop");
            assert_eq!(p.rect_of(pop), Some(c.rect), "rtl={rtl}: {c:?}");
            let block = p.host().kernel().host_block(pop).unwrap();
            assert!(matches!(
                (block.width, block.height),
                (
                    exact_kernel::BlockAxis::Region(_),
                    exact_kernel::BlockAxis::Region(_)
                )
            ));
        }
    }
}

#[test]
fn authored_none_keeps_viewport_sizing() {
    for area in ["none", "bottom span-right"] {
        let case = cases::Case {
            anchor: (300., 100.),
            area: area.into(),
            fallbacks: "none",
            rect: (100., 120., 300., 18.),
            boxes: 6,
            css: String::new(),
        };
        let state =
            format!("  state area = \"{area}\"\n  action unset\n    area = \"none\"\n  state at");
        let source = cases::source(&case, false, 1., false)
            .replace("  state at", &state)
            .replace(&format!("position-area=\"{area}\""), "position-area=area")
            .replace("row id=\"pop\"", "row id=\"pop\" role=\"dialog\"")
            .replacen(
                "box width=50 height=18 flex-shrink=0",
                concat!(
                    "button testId=\"unset\" press=unset appearance=\"none\" ",
                    "padding=0 border-width=0 width=50 height=18 flex-shrink=0"
                ),
                1,
            );
        let mut p = boot(&source);
        p.resize(400., 600.);
        p.frame();
        tap(&mut p, "anchor");
        let pop = id(&p, "pop");
        if area != "none" {
            assert_eq!(p.rect_of(pop), Some((300., 120., 100., 54.)));
            tap(&mut p, "unset");
        }
        assert_eq!(p.rect_of(pop), Some((100., 120., 300., 18.)));
        assert!(p.host().kernel().host_block_implied(pop));
    }
}

#[test]
fn sticky_popovers_keep_their_laid_out_size() {
    for position in ["sticky", "absolute"] {
        let source = format!(
            r#"component App
  state position = "{position}"
  action stick
    position = "sticky"
  view
    box
      button id="anchor" testId="anchor" popovertarget="pop" position="absolute" left=100 top=100 width=40 height=20 appearance="none" padding=0 border-width=0
      row id="pop" testId="pop" role="dialog" popover="manual" position=position position-area="bottom" width=80 height=32 margin=0 padding=0 border-width=0
        button testId="stick" press=stick appearance="none" width=80 height=32 padding=0 border-width=0
"#
        );
        let mut p = boot(&source);
        tap(&mut p, "anchor");
        let pop = id(&p, "pop");
        assert_eq!(p.rect_of(pop), Some((80., 120., 80., 32.)));
        if position != "sticky" {
            tap(&mut p, "stick");
        }
        assert_eq!(p.rect_of(pop), Some((80., 120., 80., 32.)));
        assert!(p.host().kernel().host_block(pop).is_none());
    }
}

#[test]
fn scrolling_moves_a_percentage_popover_without_resizing_it() {
    let source = r#"component App
  view
    scroll testId="page" height="100%"
      box height=1200
      button id="anchor" testId="anchor" popovertarget="pop" position="absolute" left=100 top=100 width=40 height=20 appearance="none" padding=0 border-width=0
      row id="pop" testId="pop" popover="manual" position-area="bottom span-right" width="50%" height="50%" margin=0 padding=0 border-width=0
"#;
    let mut p = boot(source);
    p.resize(400., 600.);
    p.frame();
    tap(&mut p, "anchor");
    let pop = id(&p, "pop");
    assert_eq!(p.rect_of(pop), Some((100., 120., 150., 240.)));
    p.scroll_to(id(&p, "page"), (0., 60.));
    p.frame();
    assert_eq!(p.rect_of(pop), Some((100., 60., 150., 240.)));
}
