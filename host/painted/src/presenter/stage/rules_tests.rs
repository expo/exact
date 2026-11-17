//! The stage against what is presented above a sheet, Back's own rules,
//! a driver's contacts, and what a kept drawing keeps.
use super::*;

/// A page with a header (so the shell has a top bar), a sheet holding a
/// field and opening two full-screen routes, a route whose Back control is
/// disabled and one whose Back handler keeps it.
const APP: &str = r##"routes nav
  home "/"
    info "/info"
      cover "/info/cover"
      glass "/info/glass"
    detail "/detail"
    locked "/locked"
    kept "/kept"
component App
  state pressed = 0
  action open(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  action stay
    pressed = pressed + 1
  action press
    pressed = pressed + 1
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` testId="route-home" position="absolute" inset=0 background-color="#ffffff"
            header
              text "Home" role="heading"
            text "The page under the sheet" testId="home-body"
            button press=open("/info") testId="open-info" height=40
              text "About"
            button press=open("/detail") testId="open-detail" height=40
              text "Detail"
            button press=open("/locked") testId="open-locked" height=40
              text "Locked"
            button press=open("/kept") testId="open-kept" height=40
              text "Kept"
        if e.name == "info"
          column navigationKey=`${e.id}` navigationPresentation="modal" navigationDetent="medium large" testId="route-info" position="absolute" inset=0
            text "About" testId="info-body"
            input type="text" value="Kept text" testId="info-field" height=40
            button press=open("/info/cover") testId="open-cover" height=40
              text "Cover"
            button press=open("/info/glass") testId="open-glass" height=40
              text "Glass"
        if e.name == "cover"
          column navigationKey=`${e.id}` navigationPresentation="fullscreen" testId="route-cover" position="absolute" inset=0
            button press=press testId="cover-press" position="absolute" left=0 right=0 top=380 height=200
              text "Over the sheet"
        if e.name == "glass"
          column navigationKey=`${e.id}` navigationPresentation="fullscreen" background-color="#00000040" testId="route-glass" position="absolute" inset=0
            button press=press testId="glass-press" position="absolute" left=0 right=0 top=380 height=200
              text "Over the sheet, translucent"
        if e.name == "detail"
          column navigationKey=`${e.id}` testId="route-detail" position="absolute" inset=0 background-color="#ffffff"
            header
              text "Detail" role="heading"
            text "Pushed" testId="detail-body"
        if e.name == "locked"
          column navigationKey=`${e.id}` testId="route-locked" position="absolute" inset=0 background-color="#ffffff"
            button id="back" disabled=true press=back testId="locked-back"
              text "Back"
            text "Locked" testId="locked-body"
        if e.name == "kept"
          column navigationKey=`${e.id}` testId="route-kept" position="absolute" inset=0 background-color="#ffffff"
            button id="back" press=stay testId="kept-back"
              text "Back"
            text "Kept" testId="kept-body"
      text `${pressed}` testId="pressed" display="none"
"##;

fn pressed(p: &Presenter<()>) -> String {
    let node = p.host().kernel().node(id(p, "pressed")).unwrap();
    node.text_runs().iter().map(|r| &*r.text).collect()
}

/// A full-screen route presented over a sheet covers it: the sheet takes
/// no touch and is not the pane, the shell's bars come back, and the route
/// is pressed where it shows. One whose author wrote a background shows the
/// sheet beneath it.
#[test]
fn a_full_screen_route_over_a_sheet_is_over_it() {
    for (open, press, translucent) in [
        ("open-cover", "cover-press", false),
        ("open-glass", "glass-press", true),
    ] {
        let mut p = phone_source(APP);
        tap(&mut p, "open-info");
        after(&mut p, 1000.0);
        assert!(p.top_sheet().is_some());
        tap(&mut p, open);
        after(&mut p, 1000.0);
        assert!(p.top_sheet().is_none(), "{open}: no sheet in front");
        assert!(!p.sheet_open() && !p.stage_draws_bars(), "{open}");
        // A finger where the sheet is presses the full-screen route.
        let t = p.host().now();
        p.pointer_down(200., 480., t).unwrap();
        p.pointer_up(200., 480., t + 50.).unwrap();
        p.run_commands(|| ());
        assert_eq!(pressed(&p), "1", "{open}: the route above takes the touch");
        assert!(
            has(&p, "route-info"),
            "{open}: and the sheet is not dismissed"
        );
        assert!(p.rect_of(id(&p, press)).is_some());
        // Under a translucent route the sheet still shows, beneath it.
        let sheet = p.rect_of(id(&p, "info-body"));
        assert_eq!(sheet.is_some(), translucent, "{open}: {sheet:?}");
        if translucent {
            let boxes: Vec<ViewId> = p.boxes().iter().map(|b| b.id).collect();
            let at = |name: &str| boxes.iter().position(|b| *b == id(&p, name)).unwrap();
            assert!(at("info-body") < at(press), "drawn beneath the route above");
        }
        // Back from it returns to the sheet in front.
        assert!(p.system_back(p.host().now()));
        p.run_commands(|| ());
        after(&mut p, 1000.0);
        assert!(
            p.top_sheet().is_some(),
            "{open}: the sheet is in front again"
        );
    }
}

/// A cancelled predictive Back comes back to rest on the clock, and then
/// the shell's bars are its own again.
#[test]
fn a_cancelled_preview_returns_and_gives_the_bars_back() {
    let mut p = phone_source(APP);
    tap(&mut p, "open-detail");
    after(&mut p, 1000.0);
    assert_eq!(
        p.reader_bars(1.0).map(|w| w[0]),
        Some(1),
        "the shell's top bar"
    );
    let now = p.host().now();
    assert!(p.back_progress(0, 0.0, 0, now));
    assert!(p.back_progress(1, 0.4, 0, now + 30.));
    after(&mut p, 30.0);
    assert!(p.stage_draws_bars(), "the bar moves with its route");
    assert_eq!(
        p.reader_bars(1.0),
        Some(vec![0]),
        "the shell shows none meanwhile"
    );
    p.back_progress(2, 0.0, 0, p.host().now());
    // No callback follows a cancel: the frames it owes end it.
    while p.needs_animation_frame() {
        after(&mut p, 16.0);
    }
    assert!(!p.stage_draws_bars());
    assert_eq!(
        p.reader_bars(1.0).map(|w| w[0]),
        Some(1),
        "bars are the shell's again"
    );
    assert!(near(rect(&mut p, "detail-body").0, 0.0));
}

/// Back's rules decide what is previewed: not a route whose Back control is
/// disabled, and a route a Back handler keeps comes back to rest.
#[test]
fn predictive_back_previews_only_what_back_would_take() {
    let mut p = phone_source(APP);
    tap(&mut p, "open-locked");
    after(&mut p, 1000.0);
    assert!(
        !p.back_progress(0, 0.0, 0, p.host().now()),
        "its Back is refused"
    );
    let mut p = phone_source(APP);
    tap(&mut p, "open-kept");
    after(&mut p, 1000.0);
    let rest = rect(&mut p, "kept-body");
    let now = p.host().now();
    assert!(p.back_progress(0, 0.0, 0, now));
    assert!(p.back_progress(1, 1.0, 0, now + 30.));
    after(&mut p, 30.0);
    assert!(
        rect(&mut p, "kept-body").0 > rest.0,
        "shrunk toward its edge"
    );
    // The commit presses its Back control, whose handler keeps the route.
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    assert_eq!(pressed(&p), "1");
    assert!(has(&p, "route-kept"));
    while p.needs_animation_frame() {
        after(&mut p, 16.0);
    }
    let back = rect(&mut p, "kept-body");
    assert!(
        near(back.0, rest.0) && near(back.1, rest.1),
        "{back:?} {rest:?}"
    );
    assert!(!p.stage_draws_bars());
}

/// A driver's contact is the stage's as a finger's is: a press on the scrim
/// dismisses, and a drag on the handle moves the sheet.
#[test]
fn a_drivers_contact_drags_and_dismisses_a_sheet() {
    let answer = |p: &mut Presenter<()>, q: serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&crate::agent::contact::answer(p, &q)).unwrap()
    };
    let mut p = phone_source(APP);
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    let (_, frame) = p.top_sheet().unwrap();
    let on = id(&p, "info-body");
    let y = frame.1 + 10.0;
    let down = answer(
        &mut p,
        serde_json::json!({"op":"tap","id":on,"phase":"down","x":200.0,"y":y}),
    );
    assert_eq!(down["contact"], true, "{down}");
    let moved = answer(
        &mut p,
        serde_json::json!({"op":"tap","phase":"move","dx":0.0,"dy":-400.0,"ms":200.0}),
    );
    assert_eq!(
        moved["contact"], true,
        "the sheet's drag holds the contact: {moved}"
    );
    answer(&mut p, serde_json::json!({"op":"tap","phase":"up"}));
    after(&mut p, 1000.0);
    let (_, frame) = p.top_sheet().unwrap();
    assert!(near(frame.1, STATUS), "dragged to its tallest: {frame:?}");
    // A press on the scrim, beside the sheet's top edge: gone.
    let home = id(&p, "home-body");
    let t = answer(
        &mut p,
        serde_json::json!({"op":"tap","id":home,"phase":"down","x":200.0,"y":20.0}),
    );
    assert_eq!(t["contact"], true, "{t}");
    answer(&mut p, serde_json::json!({"op":"tap","phase":"up"}));
    p.run_commands(|| ());
    assert!(!has(&p, "route-info"), "the scrim dismissed it");
}

/// A sheet's kept drawing holds the text of a field the shell hosts: the
/// shell's own view goes with its node, and the field's text slides out
/// with the sheet.
#[test]
fn a_kept_drawing_holds_a_hosted_fields_text() {
    let _hosting = crate::fields::host_here(&Metrics, 1.0);
    let mut p = phone_source(APP);
    tap(&mut p, "open-info");
    after(&mut p, 1000.0);
    let field = rect(&mut p, "info-field");
    let row = (field.1 + field.3 / 2.0) as u32;
    let dark = |p: &mut Presenter<()>| {
        let frame = p.frame();
        (0..120)
            .filter(|x| frame.pixel(*x, row).unwrap().red() < 128)
            .count()
    };
    assert_eq!(
        dark(&mut p),
        0,
        "the shell draws a hosted field, not this painter"
    );
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 1.0);
    assert!(!has(&p, "route-info"));
    assert!(dark(&mut p) > 0, "the field's text leaves with the sheet");
}

struct Metrics;
impl crate::fields::FieldMetrics for Metrics {
    fn measure(&self, face: &crate::fields::FieldFace, _: &str, width: Option<f32>) -> [f32; 3] {
        [width.unwrap_or(100.0), face.size * 1.25, face.size]
    }
}
