//! Shared elements across a route change on the stage (LLP 1013.004).
use super::*;

/// A thumbnail on the page and a hero on the route pushed over it share a
/// name; the hero's authored `none` keeps it from easing in place.
const PAIRED: &str = r##"routes nav
  home "/"
    detail "/detail"
component App
  action open
    nav = push(nav, "/detail")
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` testId="route-home" position="absolute" inset=0 background-color="#ffffff"
            button press=open testId="open" height=40
              text "Open"
            box sharedElement="photo" testId="thumb" width=80 height=60 background-color="#ff0000"
        if e.name == "detail"
          column navigationKey=`${e.id}` testId="route-detail" position="absolute" inset=0 background-color="#ffffff"
            box sharedElement="photo" testId="hero" width=400 height=300 background-color="#ff0000" -exact-layout-transition="none"
"##;

fn paired() -> Presenter<()> {
    paired_with(PAIRED)
}

fn paired_with(plan: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(plan),
        (),
        (412., 916.),
        1.,
        PathBuf::new(),
        PainterChoice::Canvas,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.host_mut().flights.enabled = true;
    p.serve_windows(Box::new(Pixels));
    assert_eq!(p.serve_bars(Box::new(Pixels)), None);
    assert_eq!(p.set_window(WINDOW), None);
    p.frame();
    p
}

fn key(p: &Presenter<()>, name: &str) -> NodeKey {
    p.host().kernel().find_by_test_id(name)[0]
}

/// The one flight, from and to, and its progress.
fn flight(p: &Presenter<()>) -> (NodeKey, NodeKey, f32) {
    let frames = p.host().flight_frames();
    assert_eq!(frames.len(), 1, "{frames:?}");
    (frames[0].from, frames[0].to, frames[0].progress)
}

#[test]
fn a_push_and_a_pop_fly_the_names_both_routes_hold_on_composes_spring() {
    let mut p = paired();
    let thumb = key(&p, "thumb");
    tap(&mut p, "open");
    after(&mut p, 0.0);
    let hero = key(&p, "hero");
    assert_eq!(
        flight(&p),
        (thumb, hero, 0.0),
        "the authored none is not asked"
    );
    assert!(
        p.host().flight_frames()[0].dissolve,
        "a route's flight dissolves"
    );
    after(&mut p, 100.0);
    // A critically damped spring of stiffness 400: 1 - (1 + 20t)e^(-20t).
    let progress = flight(&p).2;
    assert!(
        (progress - (1.0 - 3.0 * (-2.0f32).exp())).abs() < 0.01,
        "{progress}"
    );
    after(&mut p, 1000.0);
    assert!(p.host().flight_frames().is_empty());
    // The pop removes the route that held the hero, which still flies.
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert_eq!(flight(&p), (hero, thumb, 0.0));
}

#[test]
fn predictive_back_seeks_the_pops_flights_and_springs_on_or_back() {
    for commit in [false, true] {
        let mut p = paired();
        tap(&mut p, "open");
        after(&mut p, 1000.0);
        let (hero, thumb) = (key(&p, "hero"), key(&p, "thumb"));
        let now = p.host().now();
        assert!(p.back_progress(0, 0.0, 0, now));
        assert!(p.back_progress(1, 0.4, 0, now + 50.));
        after(&mut p, 500.0);
        // Compose's NavHost seeks its shared transition by the gesture.
        // By play time: the bounds spring's value 40 % into it.
        let held = 1.0 - 4.6 * (-3.6f32).exp();
        let (from, to, progress) = flight(&p);
        assert!(
            (from, to) == (hero, thumb) && near(progress, held),
            "{progress}"
        );
        let now = p.host().now();
        if commit {
            assert!(p.system_back(now));
            p.run_commands(|| ());
        } else {
            p.back_progress(2, 0.0, 0, now);
        }
        after(&mut p, 50.0);
        let progress = flight(&p).2;
        match commit {
            true => assert!(progress > held && progress < 1.0, "{progress}"),
            false => assert!(progress < held && progress > 0.0, "{progress}"),
        }
        after(&mut p, 1000.0);
        assert!(p.host().flight_frames().is_empty());
        assert_eq!(has(&p, "route-detail"), !commit);
    }
}

#[test]
fn back_while_the_push_flight_lands_flies_the_name_once() {
    let mut p = paired();
    tap(&mut p, "open");
    after(&mut p, 320.0);
    assert!(!p.host().flight_frames().is_empty(), "still landing");
    let (hero, thumb) = (key(&p, "hero"), key(&p, "thumb"));
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert_eq!(flight(&p), (hero, thumb, 0.0));
}

#[test]
fn a_containers_names_move_on_its_clock() {
    let plan = PAIRED
        .replace(
            "box sharedElement=\"photo\" testId=\"thumb\"",
            "box id=\"thumb\" sharedElement=\"photo\" testId=\"thumb\"",
        )
        .replace(
            "testId=\"route-detail\"",
            "navigationSource=\"thumb\" testId=\"route-detail\"",
        );
    let mut p = paired_with(&plan);
    // The names start at the first frame that shows them, the box too.
    p.presents_pictures(true);
    p.set_live_clock(Some(std::rc::Rc::new(|| 0.)));
    let t = p.host().now();
    tap(&mut p, "open");
    p.reader_clock(t + 16.0, true, true);
    p.frame();
    let carried = p.picture_carried();
    p.picture_presented(carried, t + 50.0);
    for i in 1..6 {
        p.reader_clock(t + 50.0 + 60.0 * f64::from(i), true, true);
        p.frame();
        let now = p.host().now();
        let shown = p
            .stage
            .shift
            .as_ref()
            .expect("a container crossing")
            .progress
            .at(now);
        // As painted, not as the host holds it after the frame.
        let name = p.brush.shared.frames[0].progress;
        assert!((name - shown).abs() < 0.01, "{name} {shown}");
        // Aimed at its rest, not at its pose in the growing box.
        let unpose = p.brush.shared.frames[0].unpose[0];
        assert!(shown > 0.9 || unpose > 1.01, "{unpose} at {shown}");
    }
    p.set_live_clock(None);
    after(&mut p, 1000.0);
    assert!(p.host().flight_frames().is_empty(), "landed with the box");
}

#[test]
fn a_route_change_ends_the_flights_into_the_route_it_covers() {
    let plan = PAIRED
        .replace(
            "    detail \"/detail\"\n",
            "    detail \"/detail\"\n    more \"/more\"\n",
        )
        .replace(
            "  action back\n",
            "  action more\n    nav = push(nav, \"/more\")\n  action back\n",
        )
        .replace(
            "            box sharedElement=\"photo\" testId=\"hero\"",
            "            button press=more testId=\"more\" height=40\n              text \"More\"\n            box sharedElement=\"photo\" testId=\"hero\"",
        )
        + "        if e.name == \"more\"\n          column navigationKey=`${e.id}` testId=\"route-more\" position=\"absolute\" inset=0 background-color=\"#ffffff\"\n";
    let mut p = paired_with(&plan);
    tap(&mut p, "open");
    after(&mut p, 50.0);
    assert!(
        !p.host().flight_frames().is_empty(),
        "the photo flies to the detail"
    );
    tap(&mut p, "more");
    after(&mut p, 16.0);
    assert!(
        p.host().flight_frames().is_empty(),
        "not over the route that covers it"
    );
}

/// A container whose arriving root holds the name.
fn named_root() -> String {
    PAIRED
        .replace(
            "box sharedElement=\"photo\" testId=\"thumb\"",
            "box id=\"thumb\" sharedElement=\"photo\" testId=\"thumb\"",
        )
        .replace(
            "testId=\"route-detail\"",
            "navigationSource=\"thumb\" sharedElement=\"photo\" testId=\"route-detail\"",
        )
        .replace(
            " sharedElement=\"photo\" testId=\"hero\"",
            " testId=\"hero\"",
        )
}

#[test]
fn a_container_whose_arriving_root_is_named_lands() {
    let mut p = paired_with(&named_root());
    tap(&mut p, "open");
    assert!(!p.host().flight_frames().is_empty(), "the root flies");
    after(&mut p, 1000.0);
    assert!(p.stage.shift.is_none(), "the crossing ends");
    assert!(p.host().flight_frames().is_empty(), "and its name lands");
}

#[test]
fn reduce_motion_during_a_container_lands_its_names() {
    let mut p = paired_with(&named_root());
    tap(&mut p, "open");
    after(&mut p, 50.0);
    let mut preferences = p.host().runner().viewport().preferences;
    preferences.reduced_motion = true;
    p.set_preferences(preferences);
    after(&mut p, 16.0);
    preferences.reduced_motion = false;
    p.set_preferences(preferences);
    after(&mut p, 16.0);
    assert!(p.stage.shift.is_none());
    assert!(p.host().flight_frames().is_empty(), "the root takes input");
}

/// A container's crossing waits for its arriving root's own flight, one
/// that is not the container's.
#[test]
fn a_container_holds_for_its_roots_own_flight() {
    let plan = PAIRED
        .replace(
            "box sharedElement=\"photo\" testId=\"thumb\"",
            "box id=\"thumb\" sharedElement=\"photo\" testId=\"thumb\"",
        )
        .replace(
            "testId=\"route-detail\"",
            "navigationSource=\"thumb\" sharedElement=\"badge\" -exact-layout-transition=\"1s linear\" testId=\"route-detail\"",
        )
        .replace(
            "      for e in stack(nav) key=e.id\n",
            "      if top(nav).name == \"home\"\n        box sharedElement=\"badge\" position=\"absolute\" top=600 width=40 height=40 -exact-layout-transition=\"1s linear\"\n      for e in stack(nav) key=e.id\n",
        );
    let mut p = paired_with(&plan);
    tap(&mut p, "open");
    after(&mut p, 600.0);
    assert!(!p.host().flight_frames().is_empty(), "the badge flies");
    assert!(p.stage.shift.is_some(), "the crossing waits for it");
    after(&mut p, 1000.0);
    assert!(p.stage.shift.is_none());
}

#[test]
fn a_cancelled_container_preview_takes_its_names_back_with_the_box() {
    let plan = PAIRED
        .replace(
            "box sharedElement=\"photo\" testId=\"thumb\"",
            "box id=\"thumb\" sharedElement=\"photo\" testId=\"thumb\"",
        )
        .replace(
            "testId=\"route-detail\"",
            "navigationSource=\"thumb\" testId=\"route-detail\"",
        );
    let mut p = paired_with(&plan);
    tap(&mut p, "open");
    after(&mut p, 1000.0);
    let now = p.host().now();
    assert!(p.back_progress(0, 0.0, 0, now));
    assert!(p.back_progress(1, 0.6, 0, now + 8.));
    after(&mut p, 8.0);
    assert!(p
        .stage
        .preview
        .as_ref()
        .is_some_and(|p| p.container.is_some()));
    p.back_progress(2, 0.0, 0, p.host().now());
    for _ in 0..4 {
        after(&mut p, 80.0);
        let now = p.host().now();
        let shown = p.stage.preview.as_ref().map_or(0.0, |s| s.shown(now));
        let progress = flight(&p).2;
        assert!(
            (progress - crate::presenter::stage::zoom::emphasized(shown)).abs() < 0.03,
            "{progress} {shown}"
        );
    }
}

#[test]
fn a_flight_aims_at_its_destinations_rest_not_its_crossing_pose() {
    let mut p = paired();
    tap(&mut p, "open");
    after(&mut p, 50.0);
    // The route comes in from the right; the reader takes that off the hero.
    let posed = rect(&mut p, "route-detail").0;
    assert!(posed > 1.0, "{posed}");
    let [sx, sy, x, y] = p.brush.shared.frames[0].unpose;
    assert_eq!((sx, sy, y), (1.0, 1.0, 0.0));
    assert!(near(x, -posed), "{x}");
}

/// Under an authored scale, in a route away from the window's origin, the
/// reader's map takes the posed route back to where it rests.
#[test]
fn a_flight_aims_at_the_rest_of_a_scaled_route_off_the_origin() {
    let plan = PAIRED
        .replace(
            "component App",
            "keyframes grow\n  from scale=\"0.5\"\nstyle Grows\n  ::view-transition-new\n    animation=\"grow 400ms linear both\"\ncomponent App",
        )
        .replace(
            "viewport-fit=\"cover\" width",
            "viewport-fit=\"cover\" position=\"relative\" left=40 width",
        )
        .replace(
            "testId=\"route-detail\"",
            "class=Grows testId=\"route-detail\"",
        );
    let mut p = paired_with(&plan);
    tap(&mut p, "open");
    after(&mut p, 200.0);
    let posed = rect(&mut p, "route-detail");
    let view = id(&p, "route-detail");
    let rest = p.host().kernel().node(view).unwrap().frame;
    assert!(posed.2 < rest.width - 1.0, "{posed:?} is scaled");
    let [sx, sy, x, y] = p.brush.shared.frames[0].unpose;
    assert!(
        near(posed.0 * sx + x, rest.x),
        "{} {}",
        posed.0 * sx + x,
        rest.x
    );
    assert!(near(posed.1 * sy + y, rest.y));
    assert!(near(posed.2 * sx, rest.width));
}

#[test]
fn a_route_part_that_cuts_flies_no_name() {
    let plan = PAIRED
        .replace(
            "component App",
            "style Cuts\n  ::view-transition-new\n    animation=\"none\"\ncomponent App",
        )
        .replace(
            "testId=\"route-detail\"",
            "class=Cuts testId=\"route-detail\"",
        );
    let mut p = paired_with(&plan);
    tap(&mut p, "open");
    after(&mut p, 16.0);
    assert!(p.host().flight_frames().is_empty());
}

#[test]
fn a_pop_leaves_its_flying_source_out_of_the_routes_kept_drawing() {
    let mut p = paired();
    tap(&mut p, "open");
    after(&mut p, 1000.0);
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert_eq!(p.host().flight_frames().len(), 1);
    // The hero's 400 × 300 fill would come from the removed route's kept drawing.
    let ops = crate::canvas::FINISHED
        .with(|f| f.borrow().clone())
        .unwrap();
    let hero = [400f32.to_bits(), 300f32.to_bits()];
    assert!(!ops.windows(2).any(|w| w == hero));
}

#[test]
fn the_names_fly_beside_a_routes_authored_part() {
    let faded = PAIRED
        .replace(
            "component App",
            "keyframes fade\n  from opacity=0\nstyle Fades\n  ::view-transition-new\n    animation=\"fade 150ms both\"\n  ::view-transition-old\n    animation=\"fade 150ms reverse both\"\ncomponent App",
        )
        .replace(r#"testId="route-detail""#, r#"class=Fades testId="route-detail""#);
    let mut p = paired_with(&faded);
    let thumb = key(&p, "thumb");
    tap(&mut p, "open");
    after(&mut p, 0.0);
    assert!(p.stage.shift.as_ref().unwrap().authored.is_some());
    let hero = key(&p, "hero");
    assert_eq!(flight(&p), (thumb, hero, 0.0));
    after(&mut p, 1000.0);
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert!(p.stage.shift.as_ref().unwrap().authored.is_some());
    assert_eq!(flight(&p), (hero, thumb, 0.0));
}

#[test]
fn a_route_that_flies_in_whole_keeps_the_route_beneath_drawn_until_it_lands() {
    let whole = PAIRED
        .replace(
            r#"            box sharedElement="photo" testId="thumb""#,
            "            box sharedElement=\"page\" width=50 height=50\n            box sharedElement=\"photo\" testId=\"thumb\"",
        )
        .replace(r#"testId="route-detail""#, r#"sharedElement="page" testId="route-detail""#);
    let mut p = paired_with(&whole);
    tap(&mut p, "open");
    // Past the shared axis, the route's flight is short of covering the home.
    after(&mut p, 350.0);
    assert_eq!(p.host().flight_frames().len(), 2);
    assert!(p.stage_shows().contains(&id(&p, "route-home")));
    after(&mut p, 1000.0);
    assert!(p.host().flight_frames().is_empty() && p.stage.shift.is_none());
}
