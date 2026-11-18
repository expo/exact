//! A route's own entrance and exit on the stage (LLP 1013.001): a page that
//! slides in and turns out, a full-screen route that fades, a route that
//! cuts, and one with no part, which keeps Material's shared axis.
use super::*;

/// Linear parts, so that a test can name where the owner is.
const ROUTES: &str = r##"routes nav
  home "/"
    slide "/slide"
    fade "/fade"
    cut "/cut"
    plain "/plain"
keyframes enter
  from translate="50% 0"
keyframes leave
  to opacity=0 rotate="10deg"
keyframes fade-in
  from opacity=0
keyframes fade-out
  to opacity=0
style Slides
  ::view-transition-new
    animation="enter 400ms linear both"
  ::view-transition-old
    animation="leave 400ms linear both"
style Fades
  ::view-transition-new
    animation="fade-in 400ms linear both"
  ::view-transition-old
    animation="fade-out 400ms linear both"
style Cuts
  ::view-transition-new
    animation="none"
  ::view-transition-old
    animation="none"
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
            text "Home" testId="home-body"
            button press=open("/slide") testId="open-slide" height=40
              text "Slide"
            button press=open("/fade") testId="open-fade" height=40
              text "Fade"
            button press=open("/cut") testId="open-cut" height=40
              text "Cut"
            button press=open("/plain") testId="open-plain" height=40
              text "Plain"
        if e.name == "slide"
          column navigationKey=`${e.id}` class=Slides testId="route-slide" position="absolute" inset=0 background-color="#ffffff"
            text "Slides" testId="slide-body"
        if e.name == "fade"
          column navigationKey=`${e.id}` class=Fades navigationPresentation="fullscreen" testId="route-fade" position="absolute" inset=0 background-color="#ffffff"
            text "Fades" testId="fade-body"
        if e.name == "cut"
          column navigationKey=`${e.id}` class=Cuts testId="route-cut" position="absolute" inset=0 background-color="#ffffff"
            text "Cuts" testId="cut-body"
        if e.name == "plain"
          column navigationKey=`${e.id}` testId="route-plain" position="absolute" inset=0 background-color="#ffffff"
            text "Plain" testId="plain-body"
"##;

fn inert(p: &Presenter<()>, name: &str) -> bool {
    p.host().route_visibility(id(p, name)).1
}

fn drawn(p: &Presenter<()>, name: &str) -> bool {
    let route = id(p, name);
    !p.host().route_visibility(route).0 || p.stage_shows().contains(&route)
}

/// The leaving and the arriving route's poses now.
fn poses(p: &Presenter<()>) -> (Pose, Pose) {
    p.shift_poses(p.host().now(), None)
}

fn back(p: &mut Presenter<()>) {
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
}

#[test]
fn a_push_plays_the_entrance_over_the_route_beneath_and_the_pop_its_exit() {
    let mut p = phone_source(ROUTES);
    tap(&mut p, "open-slide");
    after(&mut p, 0.0);
    assert!(p.stage_moving());
    // Halfway, the route is a quarter of the window's width in, from 50 %;
    // the route beneath is drawn, at rest, and the arriving route is inert.
    after(&mut p, 200.0);
    let x = rect(&mut p, "slide-body").0;
    assert!(near(x, 412.0 / 4.0), "{x}");
    assert!(drawn(&p, "route-home") && inert(&p, "route-slide"));
    assert_eq!(poses(&p).0, Pose::REST);
    after(&mut p, 250.0);
    assert!(!p.stage_moving() && !inert(&p, "route-slide"));
    assert!(near(rect(&mut p, "slide-body").0, 0.0));
    // The pop poses the kept drawing of the route that left over the route
    // it reveals, which rests.
    let slide = p.host().kernel().node(id(&p, "route-slide")).unwrap().key;
    assert!(
        p.brush.kept(slide),
        "a route whose exit is declared is kept"
    );
    back(&mut p);
    after(&mut p, 200.0);
    assert!(!has(&p, "route-slide") && p.brush.kept(slide));
    let (out, incoming) = poses(&p);
    assert!(
        near(out.rotate, 5.0) && near(out.opacity * 10.0, 5.0),
        "{out:?}"
    );
    assert_eq!(incoming, Pose::REST);
    assert!(near(rect(&mut p, "home-body").0, 0.0));
    after(&mut p, 250.0);
    assert!(!p.stage_moving());
}

#[test]
fn an_offset_routes_exit_scales_its_bar_about_the_routes_centre() {
    let source = ROUTES
        .replace("to opacity=0 rotate=\"10deg\"", "to scale=0.5")
        .replace(
            "class=Slides testId=\"route-slide\" position=\"absolute\" inset=0",
            "class=Slides testId=\"route-slide\" position=\"absolute\" left=40 top=100 right=0 bottom=0",
        )
        .replace(
            "            text \"Slides\" testId=\"slide-body\"",
            "            header\n              text \"Slides\" role=\"heading\"\n            text \"Slides\" testId=\"slide-body\"",
        );
    for transform in [
        "",
        "scale=0.8 transform-origin=\"0 0\"",
        "rotate=5 transform-origin=\"0 0\"",
    ] {
        let source = source.replace("    main ", &format!("    main {transform} "));
        let mut p = phone_source(&source);
        tap(&mut p, "open-slide");
        after(&mut p, 400.0);
        let key = p.host().kernel().node(id(&p, "route-slide")).unwrap().key;
        let route = rect(&mut p, "route-slide");
        let bar = p.bars().0.unwrap().frame;
        let page = p.chrome().frame;
        let centre = (route.0 + route.2 / 2.0, route.1 + route.3 / 2.0);
        back(&mut p);
        after(&mut p, 200.0);
        assert!(p.host().kernel().node_by_key(key).is_none());
        let bar_ts = *p.brush.stage.bars.last().unwrap();
        // Halfway to 0.5, both page and bar scale by 0.75 about the route's
        // painted centre; its ancestors' transform does not shrink the bar.
        let expected = Transform::from_row(
            0.75,
            0.0,
            0.0,
            0.75,
            (bar[0] - page[0]) * 0.75 + centre.0 * 0.25,
            (bar[1] - page[1]) * 0.75 + centre.1 * 0.25,
        );
        let parts = |t: Transform| [t.sx, t.kx, t.ky, t.sy, t.tx, t.ty];
        for (actual, expected) in parts(bar_ts).into_iter().zip(parts(expected)) {
            assert!((actual - expected).abs() < 0.001, "{transform}: {bar_ts:?}");
        }
    }
}

#[test]
fn a_full_screen_route_fades_in_and_out() {
    let mut p = phone_source(ROUTES);
    tap(&mut p, "open-fade");
    after(&mut p, 100.0);
    let (_, arriving) = poses(&p);
    assert!(near(arriving.opacity * 4.0, 1.0), "{arriving:?}");
    assert!(drawn(&p, "route-home"));
    after(&mut p, 400.0);
    // Predictive Back seeks its exit, which only an exit makes possible on
    // a full-screen route.
    let now = p.host().now();
    assert!(p.back_progress(0, 0.0, 0, now));
    assert!(p.back_progress(1, 0.5, 0, now + 50.));
    after(&mut p, 50.0);
    let (out, _) = poses(&p);
    assert!(near(out.opacity * 2.0, 1.0), "{out:?}");
    p.back_progress(2, 0.0, 0, now + 60.);
    after(&mut p, 400.0);
    back(&mut p);
    after(&mut p, 300.0);
    let (out, _) = poses(&p);
    assert!(near(out.opacity * 4.0, 1.0), "{out:?}");
}

#[test]
fn none_cuts_and_no_part_keeps_the_shared_axis() {
    let mut p = phone_source(ROUTES);
    tap(&mut p, "open-cut");
    after(&mut p, 0.0);
    assert!(!p.stage_moving());
    back(&mut p);
    after(&mut p, 0.0);
    assert!(!p.stage_moving());
    tap(&mut p, "open-plain");
    after(&mut p, 0.0);
    let shift = p.stage.shift.as_ref().expect("the shared axis");
    assert!(shift.authored.is_none());
}

#[test]
fn predictive_back_seeks_the_exit_and_the_pop_plays_on_from_there() {
    let mut p = phone_source(ROUTES);
    tap(&mut p, "open-slide");
    after(&mut p, 1000.0);
    let now = p.host().now();
    assert!(p.back_progress(0, 0.0, 0, now));
    assert!(p.back_progress(1, 0.5, 0, now + 50.));
    after(&mut p, 50.0);
    // Linear in time, square-cornered: halfway is the exit's middle.
    let (out, _) = poses(&p);
    assert!(near(out.rotate, 5.0) && out.radius == 0.0, "{out:?}");
    back(&mut p);
    after(&mut p, 0.0);
    let (out, _) = poses(&p);
    assert!(near(out.rotate, 5.0), "{out:?}");
    after(&mut p, 250.0);
    assert!(!p.stage_moving());
}

#[test]
fn a_second_change_ends_the_first_and_plays_its_own() {
    let mut p = phone_source(ROUTES);
    tap(&mut p, "open-slide");
    after(&mut p, 100.0);
    back(&mut p);
    after(&mut p, 0.0);
    let shift = p.stage.shift.as_ref().expect("the exit");
    assert!(!shift.forward && shift.authored.is_some());
    assert!(!inert(&p, "route-home"));
}

#[test]
fn a_part_cuts_when_the_person_asks_for_less_motion() {
    let mut p = phone_source(ROUTES);
    let mut preferences = p.host().runner().viewport().preferences;
    preferences.reduced_motion = true;
    p.set_preferences(preferences);
    tap(&mut p, "open-slide");
    after(&mut p, 0.0);
    assert!(!p.stage_moving() && !inert(&p, "route-slide"));
    assert!(near(rect(&mut p, "slide-body").0, 0.0));
}

/// A shell with no bars has no motion of its own for routes, and plays the
/// motion an author asks for.
#[test]
fn without_bars_a_part_plays() {
    let mut p = boot_source(ROUTES);
    p.frame();
    tap(&mut p, "open-slide");
    after(&mut p, 200.0);
    assert!(p.stage_moving());
    assert!(near(rect(&mut p, "slide-body").0, 412.0 / 4.0));
    after(&mut p, 250.0);
    back(&mut p);
    after(&mut p, 200.0);
    assert!(p.stage_moving());
    assert!(near(poses(&p).0.rotate, 5.0));
    after(&mut p, 250.0);
    tap(&mut p, "open-plain");
    after(&mut p, 0.0);
    assert!(!p.stage_moving(), "no part, no motion");
}
