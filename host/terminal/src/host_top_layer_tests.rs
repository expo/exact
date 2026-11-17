//! Terminal top-layer placement: invoker regions, viewport insets, centring,
//! inline anchoring, and isolation from the page's scrolling.
use super::*;

fn app(insets: &str) -> String {
    format!(
        r#"component App
  state at = []
  state hit = ""
  action read
    at = [frame("pop").x, frame("pop").y, frame("pop").width]
    hit = match elementFromPoint(24, 40) {{ case some(id) => id, case none => "none" }}
  view
    scroll id="page" height="100%"
      box height=1000
      box id="narrow" position="relative" width=40 height=32
        column id="pop" popover="manual" width=80 height=32 {insets}
          button id="inside" width=80
            text "In"
      box height=1000
"#
    )
}

fn boot(insets: &str, mode: Mode) -> Host<()> {
    let plan = contract::compile(&app(insets)).unwrap();
    Host::boot(plan, (), mode, 40, 20).unwrap()
}

fn node(host: &Host<()>, name: &str) -> ViewId {
    host.find(|n| n.props.str(PropId::Id) == Some(name))
        .unwrap()
}

/// The open popover's cells, full screen.
fn layer(host: &mut Host<()>) -> (i32, i32, i32, i32) {
    let pop = node(host, "pop");
    host.open_layer(pop, false);
    let r = host.frame().layer.expect("a layer");
    (r.x, r.y, r.w, r.h)
}

#[test]
fn a_popover_is_placed_by_its_insets_in_the_grid() {
    let mut host = boot("top=32 left=16", Mode::Fullscreen);
    assert!(host
        .runner
        .kernel()
        .host_block(node(&host, "pop"))
        .is_some());
    assert_eq!(layer(&mut host), (2, 2, 10, 2), "2 columns in, 2 rows down");
    let mut host = boot("right=0 bottom=0", Mode::Fullscreen);
    assert_eq!(
        layer(&mut host),
        (30, 18, 10, 2),
        "at the grid's far corner"
    );
    let mut host = boot("", Mode::Fullscreen);
    assert_eq!(layer(&mut host), (15, 9, 10, 2), "centred with no inset");
    let mut host = boot(
        "left=0 right=0 margin-left=\"auto\" margin-right=\"auto\" top=0",
        Mode::Fullscreen,
    );
    assert_eq!(
        layer(&mut host),
        (15, 0, 10, 2),
        "centred between its insets by auto margins"
    );
    let mut host = boot(
        "left=0 right=0 margin-left=\"auto\" margin-right=0 top=0",
        Mode::Fullscreen,
    );
    assert_eq!(
        layer(&mut host),
        (30, 0, 10, 2),
        "one auto margin takes the room left"
    );
}

/// The runner reads the popover where it paints: `frame` and
/// `elementFromPoint` through the block's rectangle the host tells it.
#[test]
fn the_runner_reads_a_popover_where_it_paints() {
    let mut host = boot("top=32 left=16", Mode::Fullscreen);
    layer(&mut host);
    host.act("read");
    let at = host.runner.slot("at").cloned();
    let n = |v: f64| exact_runner::Value::Number(v);
    assert_eq!(
        at,
        Some(exact_runner::Value::list(vec![n(16.0), n(32.0), n(80.0)]))
    );
    let hit = host.runner.slot("hit").cloned();
    assert_eq!(
        hit,
        Some(exact_runner::Value::str("inside")),
        "a point in its painted cells"
    );
}

/// Inline, the offer's height is the content's: no viewport, so no block,
/// and the layer is anchored to the bottom of the rows painted.
#[test]
fn inline_a_popover_is_anchored_to_the_bottom() {
    let mut host = boot("top=32 left=16", Mode::Inline);
    let pop = node(&host, "pop");
    assert!(
        host.runner.kernel().host_block(pop).is_none(),
        "no viewport inline"
    );
    host.open_layer(pop, false);
    let r = host.render(0, 6).layer.expect("a layer");
    assert_eq!((r.y, r.h), (4, 2));
}

/// Focus inside a popover scrolls nothing behind it: the walk up to the
/// scrollers stops at the popover, the root of its block.
#[test]
fn reveal_stops_at_the_popover() {
    let mut host = boot("top=32 left=16", Mode::Fullscreen);
    let page = node(&host, "page");
    host.scroll.insert(page, 500.0);
    let pop = node(&host, "pop");
    host.open_layer(pop, false);
    let inside = node(&host, "inside");
    host.reveal_to(inside, "start");
    assert_eq!(host.scroll.get(&page).copied(), Some(500.0));
}

/// An open dialog with no background of its own covers the page in the
/// terminal's default ground, as every other host's opaque surface does.
#[test]
fn a_layer_without_a_background_covers_the_page() {
    let plan = contract::compile_terminal(
        r#"component App
  view
    column width="100%" height="100%"
      text "aaaaaaaaaaaaaaaaaaaaaaaa"
      text "bbbbbbbbbbbbbbbbbbbbbbbb"
      text "cccccccccccccccccccccccc"
      text "dddddddddddddddddddddddd"
      dialog id="d" width="10ch" height="2lh"
        text "hi"
"#,
    )
    .unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 24, 6).unwrap();
    host.open_layer(node(&host, "d"), true);
    let screen = host.frame().grid.text();
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows[2], "ccccccchi        ccccccc", "{screen}");
    assert_eq!(rows[3], "ddddddd          ddddddd", "{screen}");
}

/// A scroller's indicator is painted over its rows: a row that reaches the
/// last column does not hide it.
#[test]
fn a_scrollers_indicator_is_over_its_rows() {
    let rows = "      text \"0123456789\"\n".repeat(8);
    let plan = contract::compile_terminal(&format!(
        "component App\n  view\n    scroll width=\"10ch\" height=\"4lh\"\n{rows}"
    ))
    .unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 20, 6).unwrap();
    let screen = host.frame().grid.text();
    let ends: String = screen
        .lines()
        .take(4)
        .map(|l| l.chars().nth(9).unwrap_or(' '))
        .collect();
    assert_eq!(ends, "┃┃││", "{screen}");
}

const BLUR_OPENS: &str = r#"component App
  state typed = ""
  state other = ""
  action open
    showModal("dialog")
  action put(value: string)
    other = value
  action opened(key: string)
    showModal("dialog")
  action edit(value: string)
    typed = value
  view
    column
      input value=typed input=edit blur=open id="field"
      button id="next"
        text "Next"
      input value=other input=put key=opened id="opener"
      dialog id="dialog"
        input autofocus=true id="inside"
"#;

#[test]
fn a_dialog_a_blur_opens_keeps_its_autofocus() {
    let boot = || {
        let plan = contract::compile_terminal(BLUR_OPENS).unwrap();
        let mut host = Host::boot(plan, (), Mode::Fullscreen, 40, 12).unwrap();
        host.arm = false;
        host
    };
    let mut host = boot();
    host.focus(Some(node(&host, "field")));
    host.key(Key::Named("Tab"));
    assert_eq!(host.focus, Some(node(&host, "inside")), "the dialog's");
    host.key(Key::Char('x'));
    assert_eq!(host.value(node(&host, "field")), "", "the covered field");
    // A key whose handler opens the dialog types nothing into the field
    // the dialog now covers.
    let mut host = boot();
    host.focus(Some(node(&host, "opener")));
    host.key(Key::Char('x'));
    assert_eq!(host.focus, Some(node(&host, "inside")));
    assert_eq!(host.value(node(&host, "opener")), "");
}

#[path = "../../painted/tests/position_area_region/cases.rs"]
mod region_cases;

#[test]
fn position_area_region_matches_chrome() {
    // Terminal entries refuse direction. The LTR cases use whole-cell margins
    // before and after exchanging axes.
    const SCALE: f32 = 16.;
    for c in region_cases::cases(false) {
        let plan =
            contract::compile_terminal(&region_cases::source(&c, false, SCALE, true)).unwrap();
        let mut host = Host::boot(plan, (), Mode::Fullscreen, 800, 600).unwrap();
        let anchor = node(&host, "anchor");
        host.press(anchor);
        host.act("read");
        let (x, y, w, h) = c.rect;
        let (x, y, w, h) = (SCALE * x, SCALE * y, SCALE * w, SCALE * h);
        let expected = exact_plan::Value::list(
            [x, y, w, h]
                .map(|v| exact_plan::Value::Number(v.into()))
                .to_vec(),
        );
        assert_eq!(host.runner.slot("at"), Some(&expected), "{c:?}");
        let rect = host.frame().layer.unwrap();
        let snap_x = |v: f32| (v / COLUMN).round() as i32;
        let snap_y = |v: f32| (v / ROW).round() as i32;
        assert_eq!(
            (rect.x, rect.y, rect.w, rect.h),
            (
                snap_x(x),
                snap_y(y),
                snap_x(x + w) - snap_x(x),
                snap_y(y + h) - snap_y(y)
            ),
            "{c:?}"
        );
    }
}

#[test]
fn an_invoker_keeps_authored_insets_and_releases_the_region_on_close() {
    let plan = contract::compile_terminal(
        r#"component App
  view
    column
      button id="anchor" popovertarget="pop" width="5ch" height="1lh"
      column id="pop" popover="manual" width="10ch" height="2lh" top="2lh" left="2ch"
"#,
    )
    .unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 40, 20).unwrap();
    host.press(node(&host, "anchor"));
    let r = host.frame().layer.unwrap();
    assert_eq!((r.x, r.y, r.w, r.h), (2, 2, 10, 2));
    host.close_layer(node(&host, "pop"));
    assert!(host.kernel().host_block_implied(node(&host, "pop")));
}

#[test]
fn an_invoker_retains_its_fitting_option_until_reopened() {
    let plan = contract::compile_terminal(r#"component App
  view
    scroll id="page" height="100%"
      box height="100lh"
      button id="anchor" popovertarget="pop" position="absolute" left="10ch" top="10lh" width="5ch" height="1lh"
      column id="pop" popover="manual" width="10ch" height="2lh" position-area="bottom span-right" position-try-fallbacks="flip-block" margin=0 padding=0 border-width=0
"#).unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 40, 12).unwrap();
    let pop = node(&host, "pop");
    host.press(node(&host, "anchor"));
    assert_eq!(host.frame().layer.unwrap().y, 8, "flipped above");
    host.resize(40, 20);
    assert_eq!(host.frame().layer.unwrap().y, 8, "keeps the fitting flip");
    host.close_layer(pop);
    assert!(host.kernel().host_block_implied(pop));
    host.press(node(&host, "anchor"));
    assert_eq!(
        host.frame().layer.unwrap().y,
        11,
        "a fresh opening tries the base first"
    );
    host.scroll_node_by(node(&host, "page"), 32.);
    assert_eq!(
        host.frame().layer.unwrap().y,
        9,
        "follows the scrolled invoker"
    );
}

#[test]
fn an_invoker_inside_a_placed_dialog_uses_screen_coordinates() {
    let plan = contract::compile_terminal(r#"component App
  view
    scroll id="page" height="100%"
      box height="100lh"
      dialog id="dialog" width="20ch" height="6lh" margin=0 padding=0 border-width=0
        button id="anchor" popovertarget="pop" width="5ch" height="1lh" appearance="none" padding=0 border-width=0
        column id="pop" popover="manual" width="10ch" height="2lh" margin=0 padding=0 border-width=0
"#).unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 40, 12).unwrap();
    host.scroll.insert(node(&host, "page"), 160.);
    host.open_layer(node(&host, "dialog"), true);
    host.press(node(&host, "anchor"));
    let r = host.frame().layer.unwrap();
    assert_eq!((r.x, r.y, r.w, r.h), (10, 4, 10, 2));
}

#[test]
fn scrolling_keeps_a_percentage_popovers_selected_region() {
    let plan = contract::compile_terminal(r#"component App
  view
    scroll id="page" height="100%"
      box height="100lh"
      button id="anchor" popovertarget="pop" position="absolute" left="10ch" top="6lh" width="5ch" height="1lh" appearance="none" padding=0 border-width=0
      column id="pop" popover="manual" position-area="bottom span-right" width="50%" height="50%" margin=0 padding=0 border-width=0
"#).unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 40, 40).unwrap();
    host.press(node(&host, "anchor"));
    let pop = node(&host, "pop");
    let before = host.kernel().node(pop).unwrap().frame;
    host.scroll_node_by(node(&host, "page"), 64.);
    let r = host.frame().layer.unwrap();
    let after = host.kernel().node(pop).unwrap().frame;
    assert_eq!((before.width, before.height), (120., 264.));
    assert_eq!((after.width, after.height), (120., 264.));
    assert_eq!((r.x, r.y), (10, 3));
}

#[test]
fn authored_none_keeps_a_terminal_popover_in_the_viewport_block() {
    for area in ["none", "bottom span-right"] {
        let case = region_cases::Case {
            anchor: (300., 100.),
            area: area.into(),
            fallbacks: "none",
            rect: (100., 120., 300., 18.),
            boxes: 6,
            css: String::new(),
        };
        let source = region_cases::source(&case, false, 16., true)
            .replace(
                "  state at",
                &format!(
                    "  state area = \"{area}\"\n  action unset\n    area = \"none\"\n  state at"
                ),
            )
            .replace(&format!("position-area=\"{area}\""), "position-area=area");
        let plan = contract::compile_terminal(&source).unwrap();
        let mut host = Host::boot(plan, (), Mode::Fullscreen, 800, 600).unwrap();
        host.press(node(&host, "anchor"));
        if area != "none" {
            assert!(!host.kernel().host_block_implied(node(&host, "pop")));
            host.act("unset");
        }
        let r = host.frame().layer.unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (200, 120, 600, 18));
        assert!(host.kernel().host_block_implied(node(&host, "pop")));
    }
}

#[test]
fn a_popover_changed_to_sticky_releases_its_region() {
    let plan = contract::compile_terminal(r#"component App
  state position = "absolute"
  action stick
    position = "sticky"
  view
    column
      button id="anchor" popovertarget="pop" width="5ch" height="1lh"
      column id="pop" popover="manual" position=position position-area="bottom" width="10ch" height="2lh" margin=0 padding=0 border-width=0
"#).unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 40, 20).unwrap();
    host.press(node(&host, "anchor"));
    let pop = node(&host, "pop");
    assert!(host.kernel().host_block(pop).is_some());
    host.act("stick");
    host.frame();
    assert!(host.kernel().host_block(pop).is_none());
}
