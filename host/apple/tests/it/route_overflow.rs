use crate::host::{view, NoData};
use exact_apple::Host;
use exact_kernel::{BlockAxis, HostBlock, HostCover, MonospaceMeasurer};
use exact_runner::Event;

fn boot(content: &str) -> (Host<NoData>, String) {
    boot_with(content, "")
}

fn boot_with(content: &str, style: &str) -> (Host<NoData>, String) {
    boot_at(content, style, "home")
}

fn boot_at(content: &str, style: &str, top: &str) -> (Host<NoData>, String) {
    let source = format!(
        r#"component App
  state tall = true
  action toggle
    tall = !tall
  action fit(w: number, h: number, rect: DOMRectReadOnly)
    if h > 400
      tall = false
  view
    main navigationKey="{top}" navigationBack="back" width="100%" height="100%" overflow="hidden"
      column navigationKey="home" testId="route" position="absolute" inset=0 display="block" {style}
        button testId="toggle" press=toggle position="absolute"
          text "Toggle"
{content}
"#
    );
    boot_source(&source)
}

fn boot_source(source: &str) -> (Host<NoData>, String) {
    let plan = contract::compile(source).unwrap().encode();
    Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.,
        400.,
    )
    .unwrap()
}

#[test]
fn only_in_flow_overflow_outside_a_scroll_or_clip_counts() {
    for (content, style, expected) in [
        ("        view height=401", "", 1),
        ("        view height=400", "", 0),
        ("        view height=900 position=\"absolute\"", "", 0),
        ("        view height=100 transform=\"translateY(900px)\"", "", 0),
        ("        view height=100\n          view height=500", "", 1),
        ("        view height=100 overflow-y=\"auto\"\n          view height=500", "", 0),
        ("        view height=100 overflow-y=\"hidden\"\n          view height=500", "", 0),
        ("        view height=500", "overflow-y=\"auto\"", 0),
        ("        view height=350", "padding-bottom=40 border-bottom-width=10 border-bottom-style=\"solid\"", 0),
        ("        view height=351", "padding-bottom=40 border-bottom-width=10 border-bottom-style=\"solid\"", 1),
        ("        view height=100 overflow-y=\"auto\"\n          view height=900\n        view height=500", "", 1),
        ("        view height=900 display=\"none\"", "", 0),
        ("        text \"X\" height=100 display=\"grid\" grid-template-rows=\"600px\" align-items=\"end\"", "", 1),
        ("        text \"X\" height=100 display=\"grid\" grid-template-rows=\"600px\" align-items=\"end\" overflow=\"hidden\"", "", 0),
        ("        text \"X\" height=100 display=\"grid\" grid-template-rows=\"600px\" align-items=\"end\" line-clamp=1", "", 0),
    ] {
        let (mut host, _) = boot_with(content, style);
        host.set_screen(Some((400., 400.)));
        host.activate_data();
        assert_eq!(warnings(&mut host), expected, "{style}: {content}");
    }
}

#[test]
fn the_document_scroll_reaches_visible_route_overflow_but_not_clipped_overflow() {
    for (outer, style, height, expected) in [
        ("", "", 500, 0),
        ("", "overflow=\"hidden\"", 500, 1),
        // The document extent holds the child's border box, not the route's
        // end padding below it.
        ("", "padding-bottom=16", 500, 0),
        // Content inside the route's border box moves with it: scrolling the
        // document never takes it out of the padding.
        ("", "padding-bottom=60", 350, 1),
        // An outer scroller reaches what the route does not clip.
        ("overflow=\"auto\"", "", 500, 0),
        ("overflow=\"auto\"", "padding-bottom=16", 500, 0),
        ("overflow=\"auto\"", "overflow=\"hidden\"", 500, 1),
        ("overflow=\"auto\"", "padding-bottom=60", 350, 1),
    ] {
        let source = format!(
            r#"component App
  view
    main navigationKey="home" width="100%" height="100%" {outer}
      view navigationKey="home" position="absolute" inset=0 {style}
        view height={height}
"#
        );
        let (mut host, _) = boot_source(&source);
        host.set_screen(Some((400., 400.)));
        host.activate_data();
        assert_eq!(
            warnings(&mut host),
            expected,
            "{outer} | {style} | {height}"
        );
    }
}

#[test]
fn an_outer_scroller_never_reaches_a_route_in_its_own_native_block() {
    let (mut host, _) = boot_source(
        r#"component App
  view
    main navigationKey="home" width="100%" height="100%" overflow="auto"
      view navigationKey="home" testId="route" position="absolute" inset=0
        view height=500
"#,
    );
    host.set_screen(Some((400., 400.)));
    host.set_blocks(&[(view(&host, "route"), Some(block(400.)))]);
    host.activate_data();
    assert_eq!(warnings(&mut host), 1);
}

#[test]
fn host_padding_counts_and_fitting_after_a_cover_change_rearms_the_warning() {
    let (mut host, _) = boot("        view height=350");
    host.set_screen(Some((400., 400.)));
    host.activate_data();
    let route = view(&host, "route");
    let covered = [(route, Some(HostCover::Edges([0., 0., 60., 0.])))];
    assert_eq!(warnings(&mut host), 0);
    host.set_covers(&covered);
    assert_eq!(warnings(&mut host), 1);
    host.set_covers(&[(route, None)]);
    assert_eq!(warnings(&mut host), 1);
    host.set_covers(&covered);
    assert_eq!(warnings(&mut host), 2);
}

#[test]
fn sheet_and_keyboard_offers_never_warn_or_latch_the_window_warning() {
    let (mut host, _) = boot("        view height=350");
    let route = view(&host, "route");
    // A boot offer is not yet a native window report.
    host.activate_data();
    host.resize(400., 200.);
    assert_eq!(warnings(&mut host), 0);
    host.set_screen(Some((400., 400.)));
    assert_eq!(warnings(&mut host), 0);
    host.resize(400., 400.);
    host.set_blocks(&[(route, Some(block(200.)))]);
    assert_eq!(warnings(&mut host), 0);
    host.set_blocks(&[(route, Some(block(400.)))]);
    host.set_view(None, None, None, None, None, Some(200.), &[], &[]);
    host.set_blocks(&[(route, Some(block(200.)))]);
    assert_eq!(warnings(&mut host), 0);
    host.set_blocks(&[(route, Some(block(400.)))]);
    host.set_view(None, None, None, None, None, Some(0.), &[], &[]);
    assert_eq!(warnings(&mut host), 0);
    host.resize(400., 200.);
    host.set_blocks(&[(route, Some(block(200.)))]);
    host.set_screen(Some((400., 200.)));
    assert_eq!(
        warnings(&mut host),
        1,
        "a real smaller window still diagnoses overflow"
    );
}

#[test]
fn a_retained_covered_route_never_journals_even_while_a_sheet_is_top() {
    for presentation in ["", "navigationPresentation=\"modal\""] {
        let content = format!("        view height=900\n      column navigationKey=\"covered\" {presentation} position=\"absolute\" inset=0\n        view height=100");
        let (mut host, _) = boot_at(&content, "", "covered");
        host.set_screen(Some((400., 400.)));
        host.activate_data();
        assert_eq!(warnings(&mut host), 0, "{presentation}");
    }
}

fn block(height: f32) -> HostBlock {
    HostBlock {
        width: BlockAxis::Is(400.),
        height: BlockAxis::Is(height),
        tactic: Default::default(),
    }
}

fn warnings(host: &mut Host<NoData>) -> usize {
    host.fulfill_all(vec![], 0.);
    recorded_warnings(host)
}

fn recorded_warnings(host: &Host<NoData>) -> usize {
    let logs = exact_runner::agent::handle(host.runner(), r#"{"op":"logs"}"#);
    logs.matches("route-overflow").count()
}

#[test]
fn overflow_is_journaled_after_pixel_once_until_the_route_fits_again() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let (mut host, first) = boot("        view height=(tall ? 500 : 100)");
    let wakes = Arc::new(AtomicUsize::new(0));
    let heard = wakes.clone();
    host.listen(Arc::new(move || {
        heard.fetch_add(1, Ordering::Relaxed);
    }));
    assert!(!first.contains("route-overflow"));
    host.set_screen(Some((400., 400.)));
    assert_eq!(warnings(&mut host), 0);
    host.activate_data();
    assert_eq!(
        recorded_warnings(&host),
        0,
        "the native batch is still unapplied"
    );
    assert!(wakes.load(Ordering::Relaxed) > 0, "layout requests a pump");
    assert_eq!(
        warnings(&mut host),
        1,
        "the overflowing route needs a diagnostic"
    );
    host.resize(400., 400.);
    assert_eq!(warnings(&mut host), 1);
    let settled = wakes.load(Ordering::Relaxed);
    host.fulfill_all(vec![], 0.);
    assert_eq!(
        wakes.load(Ordering::Relaxed),
        settled,
        "an idle pump requests no further pump"
    );
    let toggle = view(&host, "toggle");
    host.dispatch(toggle, Event::Press);
    assert_eq!(warnings(&mut host), 1);
    host.dispatch(toggle, Event::Press);
    assert_eq!(warnings(&mut host), 2, "fitting resets the suppression key");
}

#[test]
fn a_resize_handler_can_fit_the_route_before_its_layout_is_journaled() {
    let (mut host, _) = boot("        view height=(tall ? 500 : 100) resize=fit");
    host.set_screen(Some((400., 400.)));
    host.activate_data();
    assert_eq!(warnings(&mut host), 0);
    host.dispatch(view(&host, "toggle"), Event::Press);
    assert_eq!(
        host.runner().slot("tall"),
        Some(&exact_runner::Value::Bool(false))
    );
    assert_eq!(warnings(&mut host), 0);
}

#[test]
fn a_pushed_route_waits_for_its_native_block_and_lifted_header() {
    let (mut host, _) = boot_source(
        r#"component App
  state pushed = false
  action push
    pushed = true
  view
    main navigationKey=(pushed ? "detail" : "home") width="100%" height="100%"
      column navigationKey="home" position="absolute" inset=0
        button press=push testId="push"
          text "Push"
      if pushed
        column navigationKey="detail" position="absolute" inset=0 display="block" testId="pushed"
          header height=600 testId="lifted"
            h1 "Pushed"
          view height=300
"#,
    );
    host.set_screen(Some((400., 800.)));
    host.resize(400., 800.);
    host.activate_data();
    assert_eq!(warnings(&mut host), 0);
    host.dispatch(view(&host, "push"), Event::Press);
    assert_eq!(
        recorded_warnings(&host),
        0,
        "UIKit has not applied the push yet"
    );
    let pushed = view(&host, "pushed");
    host.set_blocks(&[(pushed, Some(block(800.)))]);
    assert_eq!(
        recorded_warnings(&host),
        0,
        "the bar has not lifted its header yet"
    );
    host.set_covers(&[(view(&host, "lifted"), Some(HostCover::Whole))]);
    host.fulfill_all(vec![], 0.);
    assert_eq!(warnings(&mut host), 0, "the presented route fits");
}

#[test]
fn a_modal_at_the_window_offer_journals_overflow() {
    let (mut host, _) = boot_with(
        "        view height=500",
        "navigationPresentation=\"modal\"",
    );
    host.set_screen(Some((400., 400.)));
    host.set_blocks(&[(view(&host, "route"), Some(block(400.)))]);
    host.activate_data();
    host.fulfill_all(vec![], 0.);
    assert_eq!(warnings(&mut host), 1);
}
