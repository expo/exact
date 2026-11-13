//! The stage on a shell with bars (Android's reader): a modal route as
//! Material's bottom sheet (expo/exact#143, #234), a route pushed inside
//! it (#326), pushes and pops crossing and predictive Back (#144); and on
//! a shell with none, a route pushed inside a sheet in the sheet's box.
use super::super::top::SurfacePainter;
use super::*;
use crate::paint::Backend;

#[path = "rules_tests.rs"]
mod rules;

/// A page, a sheet with two detents and one with none, a route pushed
/// inside the first sheet, and a page pushed over the first.
const APP: &str = r##"routes nav
  home "/"
    detail "/detail"
    info "/info"
      settings "/info/settings"
    form "/form"
    stuck "/stuck"
component App
  action open(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` testId="route-home" position="absolute" inset=0 background-color="#ffffff"
            text "The page under the sheet" testId="home-body"
            button press=open("/detail") testId="open-detail" height=40
              text "Detail"
            button press=open("/info") testId="open-info" height=40
              text "About"
            button press=open("/form") testId="open-form" height=40
              text "Form"
            button press=open("/stuck") testId="open-stuck" height=40
              text "Stuck"
        if e.name == "detail"
          column navigationKey=`${e.id}` testId="route-detail" position="absolute" inset=0 background-color="#ffffff"
            text "Pushed" testId="detail-body"
        if e.name == "info"
          column navigationKey=`${e.id}` navigationPresentation="modal" navigationDetent="medium large" testId="route-info" position="absolute" inset=0
            text "About" testId="info-body"
            button press=open("/info/settings") testId="open-settings" height=40
              text "Settings"
            button press=back testId="info-close" height=40
              text "Close"
        if e.name == "settings"
          column navigationKey=`${e.id}` testId="route-settings" position="absolute" inset=0
            text "Settings, inside the sheet" testId="settings-body"
        if e.name == "form"
          column navigationKey=`${e.id}` navigationPresentation="modal" testId="route-form" position="absolute" inset=0
            text "Top of the modal" testId="form-body"
        if e.name == "stuck"
          column navigationKey=`${e.id}` navigationPresentation="modal" navigationDetent="medium" closedby="none" testId="route-stuck" position="absolute" inset=0
            text "Stays" testId="stuck-body"
"##;

/// A Pixel's window in points: a 52 status bar and a 24 gesture bar.
const WINDOW: Window = Window {
    size: (412.0, 916.0),
    bars: [52.0, 0.0, 24.0, 0.0],
    cutout: [0.0; 4],
    keyboard: [0.0; 4],
    gestures: [0.0; 4],
};
const H: f32 = 916.0;
const STATUS: f32 = 52.0;
const FOOT: f32 = 24.0;

struct Pixels;
impl SurfacePainter for Pixels {
    fn record(
        &mut self,
        size: (f32, f32),
        scale: f32,
        paint: &mut dyn FnMut(&mut dyn Backend),
    ) -> Vec<u32> {
        let mut raster = crate::raster::Raster::transparent();
        raster.begin(size.0, size.1, scale);
        paint(&mut raster);
        let pixels = raster.finish().unwrap();
        vec![pixels.width(), pixels.height()]
    }
}

fn boot_source(source: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (412., 916.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

/// A presenter whose shell shows bars, in a phone's window.
fn phone() -> Presenter<()> {
    phone_source(APP)
}

fn phone_source(source: &str) -> Presenter<()> {
    let mut p = boot_source(source);
    p.serve_windows(Box::new(Pixels));
    assert_eq!(p.serve_bars(Box::new(Pixels)), None);
    assert_eq!(p.set_window(WINDOW), None);
    p.frame();
    p
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let kernel = p.host().kernel();
    let found = kernel.find_by_test_id(name);
    assert!(!found.is_empty(), "no node has the test id {name}");
    kernel.node_by_key(found[0]).unwrap().id
}

fn has(p: &Presenter<()>, name: &str) -> bool {
    !p.host().kernel().find_by_test_id(name).is_empty()
}

fn tap(p: &mut Presenter<()>, name: &str) {
    let view = id(p, name);
    p.tap(view).unwrap();
    p.run_commands(|| ());
}

/// Advance the clock by `ms` and paint.
fn after(p: &mut Presenter<()>, ms: f64) {
    let now = p.host().now() + ms;
    p.tick(now);
    p.frame();
}

/// Where a node shows: its painted box.
fn rect(p: &mut Presenter<()>, name: &str) -> Rect4 {
    let view = id(p, name);
    p.rect_of(view).expect("painted")
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

/// A pixel's red channel.
fn red(p: &mut Presenter<()>, x: u32, y: u32) -> u8 {
    p.frame().pixel(x, y).unwrap().red()
}

#[test]
fn a_modal_route_with_detents_is_a_bottom_sheet_over_the_page() {
    let mut p = phone();
    tap(&mut p, "open-info");
    // It slides up from the window's foot.
    after(&mut p, 0.0);
    let start = rect(&mut p, "info-body").1;
    after(&mut p, 120.0);
    let moving = rect(&mut p, "info-body").1;
    after(&mut p, 1000.0);
    let rest = rect(&mut p, "info-body").1;
    assert!(start > moving && moving > rest, "{start} {moving} {rest}");
    // At `medium`: half the tallest, with the gesture bar below; its
    // content starts under the handle.
    let tallest = H - STATUS;
    let top = H - (tallest / 2.0 + FOOT);
    assert!(near(rest, top + SHEET_HANDLE), "{rest} {top}");
    let (sheet, frame) = p.top_sheet().unwrap();
    assert_eq!(sheet, id(&p, "route-info"));
    assert!(near(frame.1, top) && near(frame.2, 412.0), "{frame:?}");
    // The page beneath shows, dimmed, and takes nothing.
    assert_eq!(
        p.host().route_visibility(id(&p, "route-home")),
        (false, true)
    );
    assert!(red(&mut p, 200, 400) < 200, "the scrim dims the page");
    assert!(
        red(&mut p, 200, (top + 200.0) as u32) > 200,
        "the sheet is light"
    );
    // The shell is told no bars while a sheet is up.
    assert_eq!(p.reader_bars(1.0), Some(vec![0]));
}

#[test]
fn a_modal_route_with_no_detent_rests_below_the_status_bar() {
    let mut p = phone();
    tap(&mut p, "open-form");
    after(&mut p, 1000.0);
    let body = rect(&mut p, "form-body");
    assert!(near(body.1, STATUS + SHEET_HANDLE), "{body:?}");
    // Laid out to the window's foot, the gesture bar's room left out.
    let route = p.host().kernel().node(id(&p, "route-form")).unwrap().frame;
    assert!(near(route.height, H - STATUS), "{route:?}");
}

#[test]
fn a_sheet_drags_to_its_tallest_detent_and_down_out_of_the_window() {
    let mut p = phone();
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    let (_, frame) = p.top_sheet().unwrap();
    let t = p.host().now();
    // From the handle, up past half way to the top: it rests at the top.
    let y = frame.1 + 10.0;
    p.pointer_down(200., y, t).unwrap();
    for (k, dy) in [-4.0, -100.0, -250.0, -300.0].iter().enumerate() {
        p.pointer_move(200., y + dy, t + 50. * (k + 1) as f64)
            .unwrap();
    }
    p.pointer_up(200., y - 300.0, t + 400.).unwrap();
    after(&mut p, 1000.0);
    let (_, frame) = p.top_sheet().unwrap();
    assert!(near(frame.1, STATUS), "{frame:?}");
    // Flung down from there, it leaves the window: the route pops.
    let t = p.host().now();
    let y = frame.1 + 10.0;
    p.pointer_down(200., y, t).unwrap();
    p.pointer_move(200., y + 200., t + 50.).unwrap();
    p.pointer_move(200., y + 500., t + 100.).unwrap();
    p.pointer_up(200., y + 520., t + 110.).unwrap();
    p.run_commands(|| ());
    assert!(!has(&p, "route-info"), "dragged out, it is dismissed");
    // It slides out, then the bars return to the shell.
    after(&mut p, 50.0);
    assert!(p.stage_moving());
    after(&mut p, 1000.0);
    assert!(!p.stage_moving());
    assert!(p.top_sheet().is_none());
}

#[test]
fn a_tap_on_a_sheet_presses_what_is_there_and_on_the_scrim_dismisses() {
    let mut p = phone();
    tap(&mut p, "open-stuck");
    after(&mut p, 1000.0);
    // `closedby="none"`: the scrim takes the press and nothing closes.
    let t = p.host().now();
    p.pointer_down(200., 100., t).unwrap();
    p.pointer_up(200., 100., t + 50.).unwrap();
    p.run_commands(|| ());
    assert!(has(&p, "route-stuck"));
    let mut p = phone();
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    // A press on the sheet's button is the button's.
    let close = rect(&mut p, "info-close");
    let t = p.host().now();
    let (x, y) = (close.0 + close.2 / 2.0, close.1 + close.3 / 2.0);
    p.pointer_down(x, y, t).unwrap();
    p.pointer_up(x, y, t + 50.).unwrap();
    p.run_commands(|| ());
    assert!(!has(&p, "route-info"), "the sheet's Close was pressed");
    let mut p = phone();
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    let t = p.host().now();
    p.pointer_down(200., 100., t).unwrap();
    p.pointer_up(200., 100., t + 50.).unwrap();
    p.run_commands(|| ());
    assert!(!has(&p, "route-info"), "a press on the scrim dismisses");
}

#[test]
fn back_closes_a_sheet_which_slides_out_of_the_window() {
    let mut p = phone();
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    let open = p.frame();
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    assert!(!has(&p, "route-info"));
    // Its drawing is still there as it starts down, then gone.
    after(&mut p, 16.0);
    assert!(p.stage_moving(), "its exit plays");
    let leaving = p.frame();
    let row = |f: &Pixmap, y: u32| {
        (0..412)
            .map(|x| f.pixel(x, y).unwrap().red() as u32)
            .sum::<u32>()
    };
    assert_eq!(row(&leaving, 560), row(&open, 560), "its drawing is kept");
    assert!(red(&mut p, 200, 400) < 255, "the scrim fades");
    after(&mut p, 1000.0);
    assert!(!p.stage_moving());
    assert_eq!(red(&mut p, 200, 400), 255, "the page is lit again");
    let gone = p.frame();
    assert_ne!(row(&gone, 560), row(&open, 560));
}

#[test]
fn a_route_pushed_inside_a_sheet_shows_in_it_over_the_page() {
    let mut p = phone();
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    let (_, frame) = p.top_sheet().unwrap();
    tap(&mut p, "open-settings");
    // Mid-crossing, both routes are drawn whole in the sheet.
    after(&mut p, 60.0);
    let leaving = id(&p, "info-body");
    assert!(
        p.rect_of(leaving).is_some(),
        "the sheet's own route slides out"
    );
    after(&mut p, 1000.0);
    // In the sheet, which keeps its detent; the page still beneath.
    let (shown, again) = p.top_sheet().unwrap();
    assert_eq!(shown, id(&p, "route-settings"));
    assert!(near(again.1, frame.1), "{again:?} {frame:?}");
    let body = rect(&mut p, "settings-body");
    assert!(near(body.1, frame.1 + SHEET_HANDLE), "{body:?}");
    assert_eq!(
        p.host().route_visibility(id(&p, "route-home")),
        (false, true)
    );
    assert!(
        p.host().route_visibility(id(&p, "route-info")).0,
        "the sheet's own route waits"
    );
    // Back pops inside the sheet.
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 1000.0);
    assert!(!has(&p, "route-settings"));
    let (shown, _) = p.top_sheet().unwrap();
    assert_eq!(shown, id(&p, "route-info"));
}

#[test]
fn a_push_and_a_pop_cross_on_the_shared_x_axis() {
    let mut p = phone();
    tap(&mut p, "open-detail");
    after(&mut p, 0.0);
    assert!(p.stage_moving());
    // Halfway, the arriving route is still coming in from the right.
    after(&mut p, 150.0);
    let x = rect(&mut p, "detail-body").0;
    assert!(x > 0.0 && x < AXIS_SHIFT, "{x}");
    after(&mut p, 400.0);
    assert!(near(rect(&mut p, "detail-body").0, 0.0));
    assert!(!p.stage_moving());
    // A pop: the route beneath comes back from the left, and the one that
    // leaves is drawn from its kept drawing.
    let detail = p.host().kernel().node(id(&p, "route-detail")).unwrap().key;
    assert!(p.brush.kept(detail), "a route Back would pop is kept");
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert!(!has(&p, "route-detail") && p.brush.kept(detail));
    let left = rect(&mut p, "home-body").0;
    assert!(left < 0.0, "{left}");
    after(&mut p, 400.0);
    assert!(near(rect(&mut p, "home-body").0, 0.0));
}

#[test]
fn predictive_back_previews_a_pop_and_a_cancel_puts_it_back() {
    let mut p = phone();
    tap(&mut p, "open-detail");
    after(&mut p, 1000.0);
    let now = p.host().now();
    assert!(p.back_progress(0, 0.0, 0, now));
    assert!(p.back_progress(1, 0.5, 0, now + 50.));
    after(&mut p, 50.0);
    // The route shrinks and moves right; the page beneath shows.
    let body = rect(&mut p, "detail-body");
    assert!(body.0 > 0.0, "{body:?}");
    assert!(
        !p.host().route_visibility(id(&p, "route-home")).0
            || p.stage_shows().contains(&id(&p, "route-home"))
    );
    p.back_progress(2, 0.0, 0, now + 60.);
    after(&mut p, 400.0);
    assert!(near(rect(&mut p, "detail-body").0, 0.0), "back at rest");
    assert!(has(&p, "route-detail"), "nothing committed");
    // At the root there is nothing to preview.
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 1000.0);
    assert!(!p.back_progress(0, 0.0, 0, p.host().now()));
}

#[test]
fn predictive_back_shrinks_a_sheet_that_back_closes() {
    let mut p = phone();
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    let now = p.host().now();
    let before = rect(&mut p, "info-body");
    assert!(p.back_progress(0, 0.0, 1, now));
    assert!(p.back_progress(1, 1.0, 1, now + 50.));
    after(&mut p, 50.0);
    let shrunk = rect(&mut p, "info-body");
    assert!(shrunk.2 < before.2, "{shrunk:?} {before:?}");
    // The commit is Back.
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    assert!(!has(&p, "route-info"));
    // `closedby="none"` refuses the scrim and a drag, not Back (D6), so a
    // sheet that says it is still previewed.
    tap(&mut p, "open-stuck");
    after(&mut p, 1000.0);
    assert!(p.back_progress(0, 0.0, 0, p.host().now()));
}

#[test]
fn nothing_moves_when_the_person_asks_for_less_motion() {
    let mut p = phone();
    let mut preferences = p.host().runner().viewport().preferences;
    preferences.reduced_motion = true;
    p.set_preferences(preferences);
    tap(&mut p, "open-detail");
    after(&mut p, 0.0);
    assert!(!p.stage_moving());
    assert!(near(rect(&mut p, "detail-body").0, 0.0));
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    tap(&mut p, "open-info");
    after(&mut p, 0.0);
    assert!(!p.stage_moving(), "the sheet is at rest at once");
}

/// A shell with no bars shows a modal route as written, and a route pushed
/// inside it in the box it was laid out in (expo/exact#326).
#[test]
fn without_bars_a_route_pushed_inside_a_sheet_takes_its_box() {
    let source = APP.replace(
        r#"navigationPresentation="modal" navigationDetent="medium large" testId="route-info" position="absolute" inset=0"#,
        r#"navigationPresentation="modal" navigationDetent="medium large" testId="route-info" position="absolute" top=100 left=40 width=300 height=500"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (412., 916.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    tap(&mut p, "open-info");
    p.frame();
    assert!(p.top_sheet().is_none(), "no sheet without bars");
    tap(&mut p, "open-settings");
    p.frame();
    let body = rect(&mut p, "settings-body");
    assert!(near(body.0, 40.0) && near(body.1, 100.0), "{body:?}");
    let route = p
        .host()
        .kernel()
        .node(id(&p, "route-settings"))
        .unwrap()
        .frame;
    assert!(
        near(route.width, 300.0) && near(route.height, 500.0),
        "{route:?}"
    );
    assert_eq!(
        p.host().route_visibility(id(&p, "route-home")),
        (false, true)
    );
}

/// A press on the scrim closes the sheet whole, with the route pushed
/// inside it, as a sheet swiped down on iOS does.
#[test]
fn dismissing_a_sheet_closes_what_was_pushed_inside_it() {
    let mut p = phone();
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    tap(&mut p, "open-settings");
    after(&mut p, 1000.0);
    let t = p.host().now();
    p.pointer_down(200., 400., t).unwrap();
    p.pointer_up(200., 400., t + 50.).unwrap();
    p.run_commands(|| ());
    assert!(!has(&p, "route-settings") && !has(&p, "route-info"));
    assert_eq!(
        p.host().route_visibility(id(&p, "route-home")),
        (false, false)
    );
}
