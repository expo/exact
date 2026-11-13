//! The runner's `scrollToFragment` on Linux (LLP 1153 D5): the element with
//! that id in the visit's own route, though the route beneath holds one too,
//! at the start of its scroller; no such element, the route's top and a line
//! in the journal; a scroll by the person first (a wheel, a key's or a
//! scrollbar's `scroll_to`), no scroll; the next Tab from the target; a link
//! to a fragment of the visit shown, again after the person scrolled; and an
//! author's `scrollIntoView` over the fragment's hold.
use super::*;

/// A list route that holds an `id="moves"` of its own, beneath a detail with
/// a link to its `#moves`, buttons that commit or scroll, and a scroller of
/// ten 50 px rows, the `moves` box, a button, then ten more.
fn boot() -> Presenter<()> {
    let rows = |from: usize| -> String {
        (from..from + 10)
            .map(|i| format!("                box height=50 id=\"row-{i}\"\n"))
            .collect()
    };
    let source = format!(
        r##"routes nav
  home "/"
    detail "/detail"
component App
  state count = 0
  action bump
    count = count + 1
  action aim
    scrollIntoView("row-0", block="start")
  action open(url: string)
    nav = push(nav, url)
  action follow(url: string)
    nav = go(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${{top(nav).id}}` navigationBack="back" navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${{e.id}}` position="absolute" inset=0
          if e.name == "home"
            button press=open("/detail#moves") testId="moves"
              text "Moves"
            button press=open("/detail#nothing") testId="nothing"
              text "Nothing"
            box id="moves" height=50
          if e.name == "detail"
            link href="#moves" testId="to-moves"
              text "Moves"
            button press=bump tabindex=1 testId="bump"
              text "Bump"
            button press=aim testId="aim"
              text "Aim"
            scroll testId="port" height=100
              column
{}                box id="moves" testId="target" height=50
                button tabindex=2 testId="after"
                  text "After"
{}"##,
        rows(0),
        rows(10)
    );
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    settle(&mut p);
    p
}

fn settle(p: &mut Presenter<()>) {
    for _ in 0..4 {
        assert!(p.pump(p.host.now()).is_none());
        if p.dirty() {
            let _ = p.frame();
        }
    }
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host().kernel();
    let key = k.find_by_test_id(name).first().copied();
    let node = key.and_then(|key| k.node_by_key(key));
    node.unwrap_or_else(|| panic!("no node {name}")).id
}

/// Press `link` on the list, then run what the commits asked of the host.
fn open(
    p: &mut Presenter<()>,
    link: &str,
    first: Option<fn(&mut Presenter<()>, ViewId)>,
) -> ViewId {
    p.tap(id(p, link)).unwrap();
    settle(p);
    let port = id(p, "port");
    if let Some(scroll) = first {
        scroll(p, port);
    }
    p.run_commands(|| ());
    settle(p);
    port
}

#[test]
fn a_fragment_scrolls_its_own_routes_element_to_the_start() {
    let mut p = boot();
    let port = open(&mut p, "moves", None);
    assert_eq!(p.scroll_of(port).1, 500., "ten 50 px rows above it");
    // The next Tab starts from the target, not from the list's button: the
    // stop after it in tree order, though a lower tabindex comes first in
    // the order (HTML's sequential navigation search algorithm).
    p.move_focus(false, 1.);
    assert_eq!(p.focus, Some(id(&p, "after")));
    let _ = open(&mut p, "to-moves", None);
    p.move_focus(true, 2.);
    assert_eq!(
        p.focus,
        Some(id(&p, "aim")),
        "Shift-Tab: the stop before it"
    );
}

#[test]
fn a_link_to_the_fragment_shown_scrolls_again_after_the_person_scrolled() {
    let mut p = boot();
    let port = open(&mut p, "moves", None);
    let _ = open(&mut p, "bump", None);
    p.wheel(port, 0., -100.).unwrap();
    settle(&mut p);
    assert_eq!(p.scroll_of(port).1, 400.);
    let _ = open(&mut p, "to-moves", None);
    assert_eq!(p.scroll_of(port).1, 500., "a new activation, armed now");
}

#[test]
fn the_authors_scroll_into_view_ends_the_fragments_hold() {
    let mut p = boot();
    let port = open(&mut p, "moves", None);
    // As while the route's pictures load.
    p.fragment_anchor = Some((id(&p, "target"), p.person_scrolls));
    let _ = open(&mut p, "aim", None);
    assert!(!p.hold_fragment(), "nothing held");
    assert_eq!(p.scroll_of(port).1, 0., "where the author put it");
}

#[test]
fn no_such_element_says_so_and_leaves_the_route_at_its_top() {
    let mut p = boot();
    let port = open(&mut p, "nothing", None);
    assert_eq!(p.scroll_of(port).1, 0.);
    let journal = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
    assert!(
        journal.contains("fragment #nothing: no element"),
        "{journal}"
    );
}

#[test]
fn a_link_to_a_fragment_is_the_visit_shown_at_that_fragment() {
    let mut p = boot();
    let port = open(&mut p, "nothing", None);
    let visit = p.host().router_op().unwrap().top;
    p.tap(id(&p, "to-moves")).unwrap();
    settle(&mut p);
    let op = p.host().router_op().unwrap();
    assert_eq!(
        (op.top, op.fragment.as_str()),
        (visit, "moves"),
        "the same visit"
    );
    p.run_commands(|| ());
    settle(&mut p);
    assert_eq!(p.scroll_of(port).1, 500.);
}

#[test]
fn a_scroll_by_the_person_before_the_scroll_cancels_it() {
    let wheel: fn(&mut Presenter<()>, ViewId) = |p, port| {
        p.wheel(port, 0., 30.).unwrap();
    };
    let key: fn(&mut Presenter<()>, ViewId) = |p, port| {
        p.scroll_to(port, (0., 30.)).unwrap();
    };
    for first in [wheel, key] {
        let mut p = boot();
        let port = open(&mut p, "moves", Some(first));
        assert_eq!(p.scroll_of(port).1, 30., "where the person left it");
        let journal = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
        assert!(journal.contains("the person scrolled first"), "{journal}");
    }
}

fn nested_scroll(block: &str, row: usize) -> Presenter<()> {
    nested_scroll_styled(block, row, "", "", "")
}

fn nested_scroll_styled(
    block: &str,
    row: usize,
    port: &str,
    sticky: &str,
    parent: &str,
) -> Presenter<()> {
    let rows: String = (0..40)
        .map(|i| {
            let attrs = if i == row && sticky == "target" { "position=\"sticky\" top=20" } else { "" };
            if i == row && sticky == "ancestor" {
                format!("          box height=50 position=\"sticky\" top=20\n            box height=50 id=\"row-{i}\" testId=\"row-{i}\"\n")
            } else {
                format!("          box height=50 id=\"row-{i}\" testId=\"row-{i}\" {attrs}\n")
            }
        })
        .collect();
    let source = format!(
        r#"component App
  action aim
    scrollIntoView("row-{row}", block="{block}")
  view
    column width=400 {parent}
      button press=aim testId="aim" height=30 flex-shrink=0
        text "Aim"
      box height=200 flex-shrink=0
      scroll testId="port" height=100 flex-shrink=0 {port}
        column
{rows}      box height=2000 flex-shrink=0
"#
    );
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (400., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

#[test]
fn nested_scroll_into_view_uses_each_inner_scrollers_clamped_move() {
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (block, row, scroll, page, y) in [
        ("start", 30, 1500., 230., 0.),
        ("nearest", 30, 1450., 30., 250.),
        ("start", 39, 1900., 280., 0.),
    ] {
        for mode in ["headless", "acknowledged", "pending"] {
            let mut p = nested_scroll(block, row);
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            let pending = (mode == "pending").then(|| p.display_frame().unwrap());
            p.tap(id(&p, "aim")).unwrap();
            p.run_commands(|| ());
            if let Some(frame) = pending {
                assert!(p.display_complete(&frame));
            }
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            actual.push((
                block,
                row,
                mode,
                p.scroll_of(id(&p, "port")).1,
                p.page.1,
                p.box_of(id(&p, &format!("row-{row}"))).unwrap().rect.1,
            ));
            expected.push((block, row, mode, scroll, page, y));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn nested_reveal_reports_its_move_before_the_display_acknowledges_it() {
    let mut p = nested_scroll("start", 30);
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    let row = id(&p, "row-30");
    let result: serde_json::Value = serde_json::from_str(&p.reveal(row).unwrap()).unwrap();
    assert_eq!(p.scroll_of(id(&p, "port")).1, 1475.);
    assert_eq!(p.page.1, 0.);
    assert_eq!(result["scrolled"], true, "{result}");
    assert_eq!(
        result["from"], result["to"],
        "the acknowledged picture is unchanged"
    );
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    let rect = p.box_of(row).unwrap().rect;
    assert_eq!(rect.1, 255.);
    let result: serde_json::Value = serde_json::from_str(&p.reveal(row).unwrap()).unwrap();
    assert_eq!(result["scrolled"], false, "the row is already in view");
}

#[test]
fn nested_scroll_into_view_projects_both_axes_through_two_ports_and_the_page() {
    let plan = crate::test_plan(
        r#"component App
  view
    column width=2000
      box height=60 flex-shrink=0
      scroll testId="outer" width=200 height=150 margin-left=70 flex-shrink=0 overflow-x="scroll"
        column width=1200
          box height=180 flex-shrink=0
          scroll testId="inner" width=100 height=100 margin-left=300 flex-shrink=0 overflow-x="scroll"
            box width=1000 height=950
            box id="target" testId="target" width=50 height=50 margin-left=950
          box height=1000 flex-shrink=0
      box height=2000 flex-shrink=0
"#,
    );
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for display in [false, true] {
        for already_scrolled in [false, true] {
            let (mut p, error) = Presenter::boot_with(
                &plan,
                (),
                (400., 300.),
                1.,
                PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            let (inner, outer, target) = (id(&p, "inner"), id(&p, "outer"), id(&p, "target"));
            if already_scrolled {
                p.scroll.insert(inner, (800., 800.));
                p.scroll.insert(outer, (400., 500.));
                p.page = (300., 600.);
            }
            if display {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            } else {
                let _ = p.frame();
            }
            p.scroll_element_into_view(&["target".into(), "start".into(), "start".into()]);
            if display {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            } else {
                let _ = p.frame();
            }
            actual.push((
                display,
                already_scrolled,
                p.scroll_of(inner),
                p.scroll_of(outer),
                p.page,
                p.box_of(target).unwrap().rect,
            ));
            expected.push((
                display,
                already_scrolled,
                (900., 900.),
                (350., 230.),
                (70., 60.),
                (0., 0., 50., 50.),
            ));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn nested_scroll_into_view_preserves_transforms_and_sticky_offsets() {
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (port, sticky, row, scroll, page) in [
        ("scale=2 transform-origin=\"0 0\"", "", 39, 1900., 330.),
        ("scale=2 transform-origin=\"0 0\"", "", 30, 1500., 230.),
        ("", "target", 30, 1500., 250.),
        ("", "ancestor", 30, 1500., 250.),
        (
            "scale=2 transform-origin=\"0 0\"",
            "target",
            30,
            1500.,
            270.,
        ),
        (
            "scale=2 transform-origin=\"0 0\"",
            "ancestor",
            30,
            1500.,
            270.,
        ),
    ] {
        for mode in ["headless", "acknowledged", "pending"] {
            let mut p = nested_scroll_styled("start", row, port, sticky, "");
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            let pending = (mode == "pending").then(|| p.display_frame().unwrap());
            p.scroll_element_into_view(&[format!("row-{row}").into(), "start".into()]);
            if let Some(frame) = pending {
                assert!(p.display_complete(&frame));
            }
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            actual.push((
                port,
                sticky,
                row,
                mode,
                p.scroll_of(id(&p, "port")).1,
                p.page.1,
                p.box_of(id(&p, &format!("row-{row}"))).unwrap().rect.1,
            ));
            expected.push((port, sticky, row, mode, scroll, page, 0.));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn nested_repeated_scroll_into_view_uses_offsets_awaiting_acknowledgement() {
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (port, sticky, scroll, page) in [
        ("", "", 1500., 230.),
        ("scale=2 transform-origin=\"0 0\"", "", 1500., 230.),
        ("", "target", 1520., 250.),
        ("", "ancestor", 1520., 250.),
        ("scale=2 transform-origin=\"0 0\"", "target", 1520., 270.),
        ("scale=2 transform-origin=\"0 0\"", "ancestor", 1520., 270.),
    ] {
        for mode in ["headless", "acknowledged", "pending"] {
            let mut p = nested_scroll_styled("start", 30, port, sticky, "");
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            p.scroll_element_into_view(&["row-30".into(), "start".into()]);
            let pending = (mode == "pending").then(|| p.display_frame().unwrap());
            p.scroll_element_into_view(&["row-30".into(), "start".into()]);
            if let Some(frame) = pending {
                assert!(p.display_complete(&frame));
            }
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            actual.push((
                port,
                mode,
                p.scroll_of(id(&p, "port")).1,
                p.page.1,
                p.box_of(id(&p, "row-30")).unwrap().rect.1,
            ));
            expected.push((port, mode, scroll, page, 0.));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn scroll_into_view_uses_the_page_offset_of_an_immediate_frame() {
    let plan = crate::test_plan(
        r#"component App
  view
    column
      box height=500
      box height=50 id="target" testId="target"
      box height=2000
"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (400., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    p.page = (0., 100.);
    let _ = p.frame();
    assert_eq!(p.box_of(id(&p, "target")).unwrap().rect.1, 400.);
    p.scroll_element_into_view(&["target".into(), "start".into()]);
    assert_eq!(p.page.1, 500.);
}

#[test]
fn nested_scroll_into_view_projects_spatial_transforms() {
    let mut actual = Vec::new();
    for parent in ["", "perspective=10000"] {
        for mode in ["headless", "acknowledged", "pending"] {
            let mut p = nested_scroll_styled(
                "start",
                30,
                "rotate=\"x 60deg\" transform-origin=\"0 0\"",
                "",
                parent,
            );
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            let pending = (mode == "pending").then(|| p.display_frame().unwrap());
            for _ in 0..2 {
                p.scroll_element_into_view(&["row-30".into(), "start".into()]);
                actual.push((parent, mode, p.scroll_of(id(&p, "port")).1, p.page.1));
            }
            if let Some(frame) = pending {
                assert!(p.display_complete(&frame));
            }
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            let y = p.box_of(id(&p, "row-30")).unwrap().rect.1;
            assert!(y.abs() < 0.01, "{parent} {mode}: {y}; {actual:?}");
        }
    }
    assert!(
        actual.iter().all(|(_, _, scroll, page)| {
            (scroll - 1500.).abs() < 0.01 && (page - 230.).abs() < 0.01
        }),
        "{actual:?}"
    );
}

#[test]
fn scroll_into_view_reveals_an_inline_targets_glyphs() {
    for scale in [1., 2.] {
        let plan = crate::test_plan(&format!(
            r#"component App
  view
    scroll testId="port" width=200 height=100 scale={scale} transform-origin="0 0"
      text font-size=16 line-height=20 white-space="pre-wrap"
        text "{}"
        text "Tail" id="tail" testId="tail"
"#,
            "Line\\n".repeat(12)
        ));
        let (mut p, error) = Presenter::boot_with(
            &plan,
            (),
            (400., 300.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let _ = p.frame();
        p.scroll_element_into_view(&["tail".into(), "nearest".into()]);
        let offset = p.scroll_of(id(&p, "port"));
        assert!(offset.1 > 100., "scale={scale}: {offset:?}");
        let _ = p.frame();
        let (_, y, _, height) = p.box_of(id(&p, "tail")).unwrap().rect;
        assert!(
            y >= 0. && y + height <= 100. * scale + 0.01,
            "scale={scale}: y={y}, height={height}"
        );
        p.scroll_element_into_view(&["tail".into(), "nearest".into()]);
        assert_eq!(p.scroll_of(id(&p, "port")), offset);
    }
}

#[test]
fn scroll_into_view_reveals_the_visible_part_of_a_projective_target() {
    let mut actual = Vec::new();
    for port in [true, false] {
        let container = if port {
            "scroll testId=\"port\" height=100"
        } else {
            "column"
        };
        let plan = crate::test_plan(&format!(
            r#"component App
  view
    {container} width=400 perspective=100 perspective-origin="0 0"
      box height=500
      box height=200 id="target" testId="target" rotate="x 60deg" transform-origin="0 0"
      box height=1000
"#
        ));
        for mode in ["headless", "acknowledged", "pending"] {
            let (mut p, error) = Presenter::boot_with(
                &plan,
                (),
                (400., 300.),
                1.,
                PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            let pending = (mode == "pending").then(|| p.display_frame().unwrap());
            for _ in 0..2 {
                p.scroll_element_into_view(&["target".into(), "start".into()]);
                let offset = if port {
                    p.scroll_of(id(&p, "port")).1
                } else {
                    p.page.1
                };
                actual.push((port, mode, offset));
            }
            if let Some(frame) = pending {
                assert!(p.display_complete(&frame));
            }
            if mode == "headless" {
                let _ = p.frame();
            } else {
                let frame = p.display_frame().unwrap();
                assert!(p.display_complete(&frame));
            }
            let y = p.box_of(id(&p, "target")).unwrap().rect.1;
            assert!(y.abs() < 0.01, "port={port} {mode}: {y}; {actual:?}");
        }
    }
    assert!(
        actual
            .iter()
            .all(|(_, _, offset)| (offset - 500.).abs() < 0.01),
        "{actual:?}"
    );
}
