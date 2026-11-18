//! Material's fade through between top-level destinations.
use super::*;

const TABS: &str = r##"routes nav
  tab home "/"
    sheet "/sheet"
  tab people "/people"
    person "/people/one"
component App
  action pick(name: string)
    nav = select(nav, name)
  action open(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" viewport-fit="cover" width="100%" height="100%" background-color="#000000"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` navigationPresentation=(e.name == "sheet" ? "modal" : "push") testId=`route-${e.name}` position="absolute" inset=0 padding-top=100 background-color=(e.name == "home" ? "#ff0000" : "#0000ff")
          button press=pick("home") testId=`home-${e.name}` height=40
            text "Home"
          button press=pick("people") testId=`people-${e.name}` height=40
            text "People"
          button press=open("/people/one") testId=`open-${e.name}` height=40
            text "One"
          button press=open("/sheet") testId=`sheet-${e.name}` height=40
            text "Sheet"
          column testId=`body-${e.name}` width=200 height=100 background-color="#cccccc"
"##;

pub(super) fn through(p: &Presenter<()>) -> bool {
    let (out, arrive) = p.shift_poses(p.host().now(), None);
    p.stage_moving() && out.dx == 0.0 && arrive.scale < 1.0
}

#[test]
fn a_tab_switch_fades_through() {
    let mut p = phone_source(TABS);
    let home = p.host().kernel().node(id(&p, "route-home")).unwrap().key;
    tap(&mut p, "people-home");
    after(&mut p, 0.0);
    assert!(through(&p), "a tab switch starts a fade through");
    assert!(p.brush.kept(home), "the leaving root's drawing is kept");
    // The removed route must still paint during its fade.
    assert!(p.host().kernel().node_by_key(home).is_none());
    let (out, arrive) = p.shift_poses(p.host().now() + 20.0, None);
    assert!(out.opacity > 0.0 && out.opacity < 1.0, "{out:?}");
    assert_eq!(arrive.opacity, 0.0, "{arrive:?}");
    after(&mut p, 20.0);
    let leaving = p.frame().pixel(200, 500).unwrap();
    assert!(leaving.red() > 0 && leaving.red() < 255, "{leaving:?}");
    assert_eq!(leaving.blue(), 0, "the arriving route is still transparent");
    after(&mut p, 180.0);
    let (out, arrive) = p.shift_poses(p.host().now(), None);
    assert_eq!(out.opacity, 0.0, "{out:?}");
    assert!(arrive.opacity > 0.0 && arrive.scale > 0.92 && arrive.scale < 1.0);
    let body = rect(&mut p, "body-people");
    assert!(body.2 < 200.0 && body.2 > 200.0 * 0.92, "{body:?}");
    after(&mut p, 100.0);
    assert!(!p.stage_moving());
    assert!(near(rect(&mut p, "body-people").2, 200.0));
    assert!(!p.brush.kept(home), "the leaving drawing is released");
}

#[test]
fn a_push_and_a_pop_inside_a_tab_keep_the_shared_axis() {
    let mut p = phone_source(TABS);
    tap(&mut p, "people-home");
    after(&mut p, 1000.0);
    tap(&mut p, "open-people");
    after(&mut p, 150.0);
    assert!(p.stage_moving() && !through(&p));
    let x = rect(&mut p, "body-person").0;
    assert!(x > 0.0 && x < AXIS_SHIFT, "{x}");
    after(&mut p, 1000.0);
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert!(p.stage_moving() && !through(&p));
}

#[test]
fn a_tab_switch_from_a_pushed_page_fades_through_too() {
    let mut p = phone_source(TABS);
    tap(&mut p, "people-home");
    after(&mut p, 1000.0);
    tap(&mut p, "open-people");
    after(&mut p, 1000.0);
    tap(&mut p, "home-person");
    after(&mut p, 0.0);
    assert!(through(&p));
}

#[test]
fn a_removed_fullscreen_route_fades_through_and_releases_its_drawing() {
    let source = TABS.replace(
        "e.name == \"sheet\" ? \"modal\" : \"push\"",
        "e.name == \"sheet\" ? \"modal\" : e.name == \"home\" ? \"fullscreen\" : \"push\"",
    );
    let mut p = phone_source(&source);
    let home = p.host().kernel().node(id(&p, "route-home")).unwrap().key;
    tap(&mut p, "people-home");
    after(&mut p, 20.0);
    assert!(p.host().kernel().node_by_key(home).is_none());
    let leaving = p.frame().pixel(200, 500).unwrap();
    assert!(leaving.red() > 0 && leaving.red() < 255, "{leaving:?}");
    assert_eq!(leaving.blue(), 0);
    after(&mut p, 280.0);
    assert!(!p.stage_moving());
    assert!(!p.brush.kept(home));
    assert_eq!(p.frame().pixel(200, 500).unwrap().blue(), 255);
}

#[test]
fn a_switch_to_a_fullscreen_tab_fades_through() {
    let source = TABS.replace(
        "e.name == \"sheet\" ? \"modal\" : \"push\"",
        "e.name == \"sheet\" ? \"modal\" : e.name == \"people\" ? \"fullscreen\" : \"push\"",
    );
    let mut p = phone_source(&source);
    tap(&mut p, "people-home");
    after(&mut p, 20.0);
    let leaving = p.frame().pixel(200, 500).unwrap();
    assert!(leaving.red() > 0 && leaving.red() < 255, "{leaving:?}");
    assert_eq!(leaving.blue(), 0);
    after(&mut p, 280.0);
    assert!(!p.stage_moving());
    assert_eq!(p.frame().pixel(200, 500).unwrap().blue(), 255);
}

#[test]
fn a_tab_switch_cuts_when_the_person_asks_for_less_motion() {
    let mut p = phone_source(TABS);
    tap(&mut p, "people-home");
    after(&mut p, 20.0);
    assert!(through(&p));
    let mut preferences = p.host().runner().viewport().preferences;
    preferences.reduced_motion = true;
    p.set_preferences(preferences);
    after(&mut p, 0.0);
    assert!(!p.stage_moving(), "changing the preference ends a crossing");
    tap(&mut p, "home-people");
    after(&mut p, 0.0);
    assert!(!p.stage_moving());
    assert!(near(rect(&mut p, "body-home").2, 200.0));
}

#[test]
fn a_tab_switch_with_a_sheet_open_has_no_page_crossing() {
    let mut p = phone_source(TABS);
    tap(&mut p, "sheet-home");
    after(&mut p, 350.0);
    assert!(p.sheet_open());
    tap(&mut p, "people-sheet");
    after(&mut p, 0.0);
    assert!(p.stage.shift.is_none());
    assert_eq!(
        p.shift_poses(p.host().now(), None),
        (Pose::REST, Pose::REST)
    );
    assert!(near(rect(&mut p, "body-people").2, 200.0));
}

#[test]
fn fade_through_uses_materials_emphasized_progress_for_fade_and_scale() {
    let mut p = phone_source(TABS);
    tap(&mut p, "people-home");
    after(&mut p, 0.0);
    let start = p.host().now();
    // Midpoints of M3's two cubic segments, and their join. The 35 %
    // threshold is interpolated progress, as in FadeThroughProvider.
    for (fraction, eased) in [
        (0.089583125, 0.0725),
        (0.166666, 0.4),
        (0.317708125, 0.8575),
    ] {
        let (out, arrive) = p.shift_poses(start + fraction * 300.0, None);
        let expected_out = (1.0f32 - eased / 0.35).max(0.0);
        let expected_in = ((eased - 0.35) / 0.65).max(0.0);
        assert!((out.opacity - expected_out).abs() < 0.0001, "{out:?}");
        assert!((arrive.opacity - expected_in).abs() < 0.0001, "{arrive:?}");
        assert!((arrive.scale - (0.92 + 0.08 * eased)).abs() < 0.0001);
        assert_eq!(arrive.origin, (p.viewport.0 / 2.0, p.viewport.1 / 2.0));
        assert_eq!((out.dx, out.dy, out.scale), (0.0, 0.0, 1.0));
    }
}

/// The navigation bar keeps each tab's stack mounted in its panel.
const PANELS: &str = r##"routes nav
  tab home "/"
  tab people "/people"
component App
  action pick(name: string)
    nav = select(nav, name)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%" display="flex" flex-direction="column" background-color="#000000"
      column flex=1 min-height=0 position="relative"
        for t in nav.tabs key=t.name
          column role="tabpanel" id=`panel-${t.name}` position="absolute" inset=0
            for e in t.stack key=e.id
              column navigationKey=`${e.id}` testId=`route-${e.name}` position="absolute" inset=0 padding-top=100 background-color=(e.name == "home" ? "#ff0000" : "#0000ff")
                column testId=`body-${e.name}` width=200 height=100 background-color="#cccccc"
      row role="tablist" flex-shrink=0 height=56
        button role="tab" aria-controls="panel-home" aria-selected=(nav.tab == "home") press=pick("home") testId="tab-home" flex=1
          text "Home"
        button role="tab" aria-controls="panel-people" aria-selected=(nav.tab == "people") press=pick("people") testId="tab-people" flex=1
          text "People"
"##;

#[test]
fn a_tab_of_the_navigation_bar_fades_through_and_back() {
    let mut p = phone_source(PANELS);
    let home = p.host().kernel().node(id(&p, "route-home")).unwrap().key;
    let tab = id(&p, "tab-people");
    let bar = p.bar_box(tab).unwrap();
    tap(&mut p, "tab-people");
    after(&mut p, 20.0);
    let leaving = p.frame().pixel(200, 500).unwrap();
    assert!(leaving.red() > 0 && leaving.red() < 255, "{leaving:?}");
    assert_eq!(leaving.blue(), 0);
    after(&mut p, 180.0);
    assert!(through(&p));
    assert_eq!(p.bar_box(tab).unwrap(), bar, "the navigation bar stays put");
    let body = rect(&mut p, "body-people");
    assert!(body.2 < 200.0 && body.2 > 200.0 * 0.92, "{body:?}");
    after(&mut p, 100.0);
    assert!(!p.stage_moving());
    assert!(!p.brush.kept(home), "a hidden panel's drawing is released");
    tap(&mut p, "tab-home");
    after(&mut p, 0.0);
    assert!(through(&p), "back to a tab whose stack stayed mounted");
    after(&mut p, 300.0);
    assert!(near(rect(&mut p, "body-home").2, 200.0));
}

#[test]
fn tab_crossings_keep_each_panels_opacity_in_both_directions() {
    for (home, people, common, expected) in [
        (0.5, 1.0, 1.0, 128),
        (1.0, 0.25, 1.0, 255),
        (0.5, 0.0, 0.5, 192),
    ] {
        let source = PANELS
            .replace("    main ", &format!("    main opacity={common} "))
            .replace(
                "column role=\"tabpanel\"",
                &format!(
                    "column opacity=(t.name == \"home\" ? {home} : {people}) role=\"tabpanel\""
                ),
            );
        let mut p = phone_source(&source);
        let resting = p.frame().pixel(200, 500).unwrap().red();
        assert_eq!(resting, expected);
        tap(&mut p, "tab-people");
        after(&mut p, 0.0);
        assert_eq!(p.frame().pixel(200, 500).unwrap().red(), resting);
        after(&mut p, 20.0);
        let leaving = p.frame().pixel(200, 500).unwrap().red();
        assert!(
            leaving > 0 && leaving < resting,
            "{home}, {people}, {common}: {leaving}"
        );
        after(&mut p, 279.0);
        let arriving = p.frame().pixel(200, 500).unwrap().blue();
        after(&mut p, 1.0);
        let resting = p.frame().pixel(200, 500).unwrap().blue();
        assert!(arriving.abs_diff(resting) <= 1);
        tap(&mut p, "tab-home");
        after(&mut p, 0.0);
        assert_eq!(p.frame().pixel(200, 500).unwrap().blue(), resting);
        after(&mut p, 299.0);
        let arriving = p.frame().pixel(200, 500).unwrap().red();
        after(&mut p, 1.0);
        assert!(arriving.abs_diff(p.frame().pixel(200, 500).unwrap().red()) <= 1);
    }
}

#[test]
fn tab_crossings_keep_panel_clips() {
    let source = PANELS
        .replace(
            "id=`panel-${t.name}` position=\"absolute\" inset=0",
            "id=`panel-${t.name}` position=\"absolute\" left=0 top=0 width=100 height=\"100%\" overflow=\"hidden\"",
        )
        .replace("inset=0 padding-top=100", "inset=0 width=412 padding-top=100");
    let mut p = phone_source(&source);
    assert_eq!(red(&mut p, 200, 500), 0);
    tap(&mut p, "tab-people");
    after(&mut p, 20.0);
    assert_eq!(red(&mut p, 200, 500), 0, "the leaving panel clips");
    after(&mut p, 279.0);
    assert_eq!(
        p.frame().pixel(200, 500).unwrap().blue(),
        0,
        "the arriving panel clips"
    );
}

#[test]
fn tab_crossings_composite_panel_opacity_as_a_group() {
    let source = PANELS.replace(
        "column role=\"tabpanel\"",
        "column opacity=0.5 background-color=\"#00ff00\" role=\"tabpanel\"",
    );
    let mut p = phone_source(&source);
    tap(&mut p, "tab-people");
    after(&mut p, 299.0);
    let arriving = p.frame().pixel(200, 500).unwrap();
    after(&mut p, 1.0);
    let resting = p.frame().pixel(200, 500).unwrap();
    assert_eq!(resting.green(), 0);
    assert!(
        arriving.green().abs_diff(resting.green()) <= 1,
        "{arriving:?} -> {resting:?}"
    );
    assert!(
        arriving.blue().abs_diff(resting.blue()) <= 1,
        "{arriving:?} -> {resting:?}"
    );
}

#[test]
fn tab_crossings_keep_later_siblings_above_routes() {
    let source = PANELS.replace(
        "      row role=\"tablist\"",
        "            column position=\"absolute\" left=300 top=400 width=50 height=50 background-color=\"#00ff00\"\n      row role=\"tablist\"",
    );
    let mut p = phone_source(&source);
    tap(&mut p, "tab-people");
    after(&mut p, 299.0);
    let arriving = p.frame().pixel(320, 420).unwrap();
    after(&mut p, 1.0);
    let resting = p.frame().pixel(320, 420).unwrap();
    assert_eq!(resting.green(), 255);
    assert_eq!(arriving, resting, "the later sibling stays above the route");
}

pub(super) const SWAP: &str = r##"component App
  state second = false
  action swap
    second = !second
  view
    main navigationKey=(second ? "b" : "a") viewport-fit="cover" width="100%" height="100%" background-color="#000000"
      if !second
        column navigationKey="a" testId="original" position="absolute" left=40 top=100 right=0 bottom=0 background-color="#ff0000"
          button press=swap testId="swap" height=40
            text "Swap"
      if second
        column navigationKey="b" testId="replacement" position="absolute" left=60 top=150 right=0 bottom=0 background-color="#0000ff"
          text "Second root"
"##;

#[test]
fn a_removed_root_fades_at_its_original_position() {
    let mut p = phone_source(SWAP);
    let original = p.host().kernel().node(id(&p, "original")).unwrap().key;
    assert_eq!(red(&mut p, 200, 80), 0);
    assert_eq!(red(&mut p, 20, 200), 0);
    assert_eq!(red(&mut p, 200, 500), 255);
    tap(&mut p, "swap");
    after(&mut p, 20.0);
    assert!(p.host().kernel().node_by_key(original).is_none());
    assert_eq!(red(&mut p, 200, 80), 0, "nothing moves above the root");
    assert_eq!(red(&mut p, 20, 200), 0, "nothing moves left of the root");
    let leaving = red(&mut p, 200, 500);
    assert!(leaving > 0 && leaving < 255, "{leaving}");
    after(&mut p, 280.0);
    assert!(!p.brush.kept(original));
}

const REMOVED_PANELS: &str = r##"component App
  state second = false
  action swap
    second = !second
  view
    main navigationKey=(second ? "b" : "a") viewport-fit="cover" width="100%" height="100%" background-color="#000000"
      if !second
        column role="tabpanel" id="panel-a" opacity=0.5 position="absolute" inset=0
          column navigationKey="a" testId="original" position="absolute" inset=0 background-color="#ff0000"
            button press=swap testId="swap" height=40
              text "Swap"
      if second
        column role="tabpanel" id="panel-b" opacity=0.25 position="absolute" inset=0
          column navigationKey="b" position="absolute" inset=0 background-color="#0000ff"
      row role="tablist" position="absolute" bottom=0 height=56 width="100%"
        button role="tab" aria-controls="panel-a" aria-selected=(!second) press=swap
          text "Home"
        button role="tab" aria-controls="panel-b" aria-selected=second press=swap
          text "People"
      column position="absolute" left=300 top=400 width=50 height=50 background-color="#00ff00"
"##;

#[test]
fn a_removed_root_keeps_its_own_panels_opacity_below_later_siblings() {
    let mut p = phone_source(REMOVED_PANELS);
    let original = p.host().kernel().node(id(&p, "original")).unwrap().key;
    let resting = red(&mut p, 200, 500);
    assert_eq!(resting, 128);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(p.host().kernel().node_by_key(original).is_none());
    assert_eq!(red(&mut p, 200, 500), resting);
    assert_eq!(p.frame().pixel(320, 420).unwrap().green(), 255);
    after(&mut p, 20.0);
    let leaving = red(&mut p, 200, 500);
    assert!(leaving > 0 && leaving < resting, "{leaving}");
    after(&mut p, 279.0);
    let arriving = p.frame().pixel(200, 500).unwrap().blue();
    after(&mut p, 1.0);
    assert_eq!(p.frame().pixel(200, 500).unwrap().blue(), 64);
    assert!(arriving.abs_diff(64) <= 1);
    assert!(!p.brush.kept(original));
}

/// Panels shown one at a time inside a container both keep: the leaving
/// page fades out in the container and the arriving one in, each from
/// where it was, whether the panel or a wrapper of its own arrives.
#[test]
fn panels_inside_a_kept_container_fade_through_it() {
    let (head, rest) = REMOVED_PANELS.split_at(REMOVED_PANELS.find("      if !second").unwrap());
    let (panels, tail) = rest.split_at(rest.find("      row role=\"tablist\"").unwrap());
    for wrapper in ["", "column position=\"absolute\" inset=0"] {
        // The panels go under the kept container, each, with `wrapper`,
        // under a wrapper of its own.
        let mut wrapped = String::new();
        for line in panels.lines() {
            let branch = line.trim_start().starts_with("if ");
            let deeper = if branch || wrapper.is_empty() { 2 } else { 4 };
            wrapped += &format!("{}{line}\n", " ".repeat(deeper));
            if branch && !wrapper.is_empty() {
                wrapped += &format!("          {wrapper}\n");
            }
        }
        let source = format!("{head}      column position=\"absolute\" inset=0\n{wrapped}{tail}");
        let mut p = phone_source(&source);
        let original = p.host().kernel().node(id(&p, "original")).unwrap().key;
        assert_eq!(red(&mut p, 200, 500), 128, "{source}");
        tap(&mut p, "swap");
        after(&mut p, 0.0);
        assert!(p.host().kernel().node_by_key(original).is_none());
        assert_eq!(
            red(&mut p, 200, 500),
            128,
            "{wrapper}: the leaving page as it was"
        );
        after(&mut p, 20.0);
        let leaving = red(&mut p, 200, 500);
        assert!(leaving > 0 && leaving < 128, "{wrapper}: {leaving}");
        after(&mut p, 280.0);
        assert_eq!(p.frame().pixel(200, 500).unwrap().blue(), 64, "{wrapper}");
    }
}

#[test]
fn a_removed_panel_keeps_its_clip_opacity_group_and_sibling_order() {
    let source = REMOVED_PANELS
        .replace("id=\"panel-a\" opacity=0.5 position=\"absolute\" inset=0", "id=\"panel-a\" opacity=0.5 position=\"absolute\" left=0 top=0 width=100 height=\"100%\" overflow=\"hidden\" background-color=\"#00ff00\"")
        .replace("testId=\"original\" position=", "testId=\"original\" width=412 position=")
        .replace("testId=\"swap\" height=40", "testId=\"swap\" width=80 height=40")
        .replace("      if second", "          column position=\"absolute\" left=20 top=400 width=50 height=50 background-color=\"#00ff00\"\n      if second");
    let mut p = phone_source(&source);
    let panel = p
        .host()
        .kernel()
        .node(id(&p, "original"))
        .unwrap()
        .parent
        .unwrap();
    let panel = p.host().kernel().node(panel).unwrap().key;
    let resting = p.frame().pixel(80, 500).unwrap();
    assert_eq!(resting.red(), 128);
    assert_eq!(resting.green(), 0);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(p.host().kernel().node_by_key(panel).is_none());
    assert_eq!(p.frame().pixel(80, 500).unwrap(), resting);
    assert_eq!(p.frame().pixel(40, 420).unwrap().green(), 128);
    after(&mut p, 20.0);
    assert_eq!(red(&mut p, 200, 500), 0, "the retained panel still clips");
    assert_eq!(
        p.frame().pixel(40, 420).unwrap().red(),
        0,
        "the later sibling stays above the route"
    );
    let leaving = red(&mut p, 80, 500);
    assert!(leaving > 0 && leaving < resting.red());
    after(&mut p, 280.0);
    assert!(!p.brush.kept(panel), "the retained panel is released");
}

#[test]
fn a_removed_root_keeps_its_ancestors_painted_transform() {
    for (transform, outside, inside) in [
        ("translate=\"30px 0px\"", (50, 500), (80, 500)),
        ("scale=0.8 transform-origin=\"0 0\"", (20, 500), (35, 500)),
    ] {
        let source = SWAP.replace("    main ", &format!("    main {transform} "));
        let mut p = phone_source(&source);
        assert_eq!(red(&mut p, outside.0, outside.1), 0, "{transform}");
        assert_eq!(red(&mut p, inside.0, inside.1), 255, "{transform}");
        tap(&mut p, "swap");
        after(&mut p, 20.0);
        assert_eq!(
            red(&mut p, outside.0, outside.1),
            0,
            "the leaving root stays inside its painted bounds: {transform}"
        );
        let leaving = red(&mut p, inside.0, inside.1);
        assert!(leaving > 0 && leaving < 255, "{transform}: {leaving}");
    }
}

#[test]
fn an_arriving_root_keeps_its_ancestors_transform() {
    for transform in [
        "translate=\"30px 0px\"",
        "scale=0.8 transform-origin=\"0 0\"",
        "rotate=5 transform-origin=\"0 0\"",
    ] {
        let source = SWAP.replace("    main ", &format!("    main {transform} "));
        let mut p = phone_source(&source);
        tap(&mut p, "swap");
        after(&mut p, 50.0);
        let growing = rect(&mut p, "replacement");
        after(&mut p, 249.0);
        let almost = rect(&mut p, "replacement");
        after(&mut p, 1.0);
        let rest = rect(&mut p, "replacement");
        let page = p.chrome().frame;
        let centre = (WINDOW.size.0 / 2.0 - page[0], WINDOW.size.1 / 2.0 - page[1]);
        let expected = (
            rest.0 * 0.952 + centre.0 * 0.048,
            rest.1 * 0.952 + centre.1 * 0.048,
            rest.2 * 0.952,
            rest.3 * 0.952,
        );
        for (actual, expected) in [
            (growing.0, expected.0),
            (growing.1, expected.1),
            (growing.2, expected.2),
            (growing.3, expected.3),
            (almost.0, rest.0),
            (almost.1, rest.1),
            (almost.2, rest.2),
            (almost.3, rest.3),
        ] {
            assert!(
                near(actual, expected),
                "{transform}: {actual}, expected {expected}"
            );
        }
    }
}

#[test]
fn mounted_tabs_keep_their_ancestors_transform_through_the_crossing() {
    for transform in [
        "translate=\"30px 0px\"",
        "scale=0.8 transform-origin=\"0 0\"",
        "rotate=5 transform-origin=\"0 0\"",
    ] {
        let source = PANELS.replace("    main ", &format!("    main {transform} "));
        let mut p = phone_source(&source);
        let rest = rect(&mut p, "body-home");
        tap(&mut p, "tab-people");
        after(&mut p, 20.0);
        assert_eq!(rect(&mut p, "body-home"), rest, "{transform}");
        after(&mut p, 30.0);
        let arrive = rect(&mut p, "body-people");
        let page = p.chrome().frame;
        let centre = (WINDOW.size.0 / 2.0 - page[0], WINDOW.size.1 / 2.0 - page[1]);
        assert!(
            near(arrive.0, rest.0 * 0.952 + centre.0 * 0.048),
            "{transform}: {arrive:?}"
        );
        assert!(
            near(arrive.1, rest.1 * 0.952 + centre.1 * 0.048),
            "{transform}: {arrive:?}"
        );
        assert!(near(arrive.2, rest.2 * 0.952), "{transform}: {arrive:?}");
        assert!(near(arrive.3, rest.3 * 0.952), "{transform}: {arrive:?}");
        after(&mut p, 249.0);
        let almost = rect(&mut p, "body-people");
        after(&mut p, 1.0);
        assert_eq!(rect(&mut p, "body-people"), rest, "{transform}");
        assert!(
            near(almost.0, rest.0) && near(almost.1, rest.1),
            "{transform}: {almost:?}"
        );
    }
}

#[test]
fn a_popped_top_bar_keeps_its_window_size_under_transformed_ancestors() {
    for (transform, ancestor) in [
        (
            "scale=0.8 transform-origin=\"0 0\"",
            Transform::from_scale(0.8, 0.8),
        ),
        (
            "rotate=5 transform-origin=\"0 0\"",
            Transform::from_rotate(5.0),
        ),
    ] {
        let source = TABS
            .replace("    main ", &format!("    main {transform} "))
            .replace(
                "          button press=pick(\"home\")",
                "          header\n            text e.name role=\"heading\"\n          button press=pick(\"home\")",
            );
        let mut p = phone_source(&source);
        tap(&mut p, "people-home");
        after(&mut p, 300.0);
        tap(&mut p, "open-people");
        after(&mut p, 300.0);
        let bar = p.bars().0.unwrap().frame;
        assert!(p.system_back(p.host().now()));
        p.run_commands(|| ());
        after(&mut p, 20.0);
        let actual = p.brush.stage.bars.last().unwrap();
        // The bar slides as the route does on screen, at its own size.
        let (out, _) = p.shift_poses(p.host().now(), None);
        let slide = (ancestor.sx * out.dx, ancestor.ky * out.dx);
        let expected = Transform::from_translate(slide.0, slide.1)
            .pre_concat(p.window_to_page((bar[0], bar[1], bar[2], bar[3])));
        for (actual, expected) in [
            (actual.sx, expected.sx),
            (actual.kx, expected.kx),
            (actual.ky, expected.ky),
            (actual.sy, expected.sy),
            (actual.tx, expected.tx),
            (actual.ty, expected.ty),
        ] {
            assert!((actual - expected).abs() < 0.001, "{transform}: {actual}");
        }
    }
}

#[test]
fn top_app_bars_fade_in_place_while_the_page_grows() {
    let source = PANELS.replace(
        "                column testId=",
        "                header\n                  text e.name role=\"heading\"\n                column testId=",
    );
    let mut p = phone_source(&source);
    let bar = p.bars().0.unwrap().frame;
    let bottom = p.bar_box(id(&p, "tab-people")).unwrap();
    tap(&mut p, "tab-people");
    for ms in [20.0, 30.0] {
        after(&mut p, ms);
        let pictures = &p.brush.stage.bars;
        assert_eq!(pictures.len(), 2, "both top bars are drawn");
        let items = p.brush.stage.items.iter();
        let navigation = items.filter(|item| matches!(item, Item::Picture(..)));
        assert_eq!(navigation.count(), 1, "and the navigation bar");
        let expected = p.window_to_page((bar[0], bar[1], bar[2], bar[3]));
        assert_eq!(pictures[0], expected, "the leaving top bar stays put");
        assert_eq!(pictures[1], expected, "the arriving top bar stays put");
        assert_eq!(p.bar_box(id(&p, "tab-people")).unwrap(), bottom);
        assert!(rect(&mut p, "body-people").2 < 200.0);
    }
    after(&mut p, 250.0);
    assert!(!p.stage_moving());
    assert_eq!(p.bars().0.unwrap().frame, bar);
}

#[test]
fn an_offset_root_grows_about_the_window_centre() {
    for source in [SWAP.to_owned(), SWAP.replace("viewport-fit=\"cover\"", "")] {
        let mut p = phone_source(&source);
        tap(&mut p, "swap");
        after(&mut p, 50.0);
        // At 1/6 of 300 ms the emphasized curve reaches 0.4: scale is 0.952.
        let page = p.chrome().frame;
        let centre = (WINDOW.size.0 / 2.0 - page[0], WINDOW.size.1 / 2.0 - page[1]);
        let expected = (
            60.0 * 0.952 + centre.0 * 0.048,
            150.0 * 0.952 + centre.1 * 0.048,
        );
        let body = rect(&mut p, "replacement");
        assert!(
            near(body.0, expected.0) && near(body.1, expected.1),
            "{body:?}, expected {expected:?}"
        );
        let frame = p.frame();
        assert_eq!(
            frame
                .pixel(200, expected.1.floor() as u32 - 2)
                .unwrap()
                .blue(),
            0
        );
        assert!(
            frame
                .pixel(200, expected.1.ceil() as u32 + 2)
                .unwrap()
                .blue()
                > 0
        );
    }
}

#[test]
fn a_replaced_root_fades_through_but_a_swap_inside_a_sheet_cuts() {
    let mut p = phone_source(SWAP);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(through(&p));
    after(&mut p, 300.0);
    assert!(!p.stage_moving());
    let sheet = SWAP.replace(
        "      if !second",
        "      column navigationKey=\"sheet\" navigationPresentation=\"modal\" position=\"absolute\" inset=0\n      if !second",
    );
    let mut p = phone_source(&sheet);
    after(&mut p, 350.0);
    assert!(p.sheet_open());
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(p.stage.shift.is_none(), "a swap inside the sheet cuts");
}

#[test]
fn a_clipped_button_is_hit_only_where_it_shows_through_the_crossing() {
    let source = PANELS
        .replace(
            "column flex=1 min-height=0 position=\"relative\"",
            "column flex=1 min-height=0 position=\"relative\" width=100 overflow=\"hidden\"",
        )
        .replace(
            "                column testId=`body-${e.name}`",
            "                button testId=`far-${e.name}` position=\"absolute\" left=200 top=300 width=80 height=40\n                  text \"Far\"\n                column testId=`body-${e.name}`",
        );
    let mut p = phone_source(&source);
    let far = rect(&mut p, "far-home");
    let (x, y) = (far.0 + far.2 / 2.0, far.1 + far.3 / 2.0);
    let clipped = |p: &mut Presenter<()>, name: &str| p.hit(x, y) == Some(id(p, name));
    assert!(!clipped(&mut p, "far-home"));
    let unclipped = source.replace(" width=100 overflow=\"hidden\"", "");
    let mut shown = phone_source(&unclipped);
    assert!(
        clipped(&mut shown, "far-home"),
        "the button is hit where it shows"
    );
    tap(&mut p, "tab-people");
    for ms in [20.0, 279.0] {
        after(&mut p, ms);
        assert!(!clipped(&mut p, "far-people"), "{ms}");
    }
    after(&mut p, 1.0);
    assert!(!clipped(&mut p, "far-people"));
}

#[test]
fn a_crossing_keeps_the_authored_stacking_order() {
    let overlay = "column position=\"absolute\" left=150 top=400 width=100 height=100 z-index=1 background-color=\"#00ff00\"";
    let source = PANELS
        .replace(
            "column role=\"tabpanel\"",
            "column z-index=(t.name == \"home\" ? 2 : 0) role=\"tabpanel\"",
        )
        .replace(
            "      row role=\"tablist\"",
            &format!("        {overlay}\n      row role=\"tablist\""),
        );
    let mut p = phone_source(&source);
    assert_eq!(red(&mut p, 200, 450), 255, "Home is above the overlay");
    tap(&mut p, "tab-people");
    after(&mut p, 300.0);
    assert_eq!(
        p.frame().pixel(200, 450).unwrap().green(),
        255,
        "People is below it"
    );
    tap(&mut p, "tab-home");
    after(&mut p, 299.0);
    assert!(
        red(&mut p, 200, 450) > 250,
        "the arriving Home stays above it"
    );
    // A removed destination keeps its place in the stacking order too.
    let source = REMOVED_PANELS
        .replace("id=\"panel-a\" opacity=0.5", "id=\"panel-a\" z-index=2")
        .replace(
            "left=300 top=400 width=50 height=50",
            "left=300 top=400 width=50 height=50 z-index=1",
        );
    let mut p = phone_source(&source);
    assert_eq!(red(&mut p, 320, 420), 255);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert_eq!(
        red(&mut p, 320, 420),
        255,
        "the leaving page stays above it"
    );
}

#[test]
fn a_push_and_a_pop_keep_their_ancestors_transform() {
    for transform in [
        "translate=\"30px 0px\"",
        "scale=0.8 transform-origin=\"0 0\"",
        "rotate=5 transform-origin=\"0 0\"",
    ] {
        let source = TABS.replace("    main ", &format!("    main {transform} "));
        let mut p = phone_source(&source);
        let home = rect(&mut p, "body-home");
        tap(&mut p, "open-home");
        after(&mut p, 1.0);
        let leaving = rect(&mut p, "body-home");
        assert!(near(leaving.1, home.1), "{transform}: {leaving:?} {home:?}");
        assert!(
            near(leaving.2, home.2) && near(leaving.3, home.3),
            "{transform}"
        );
        after(&mut p, 150.0);
        let arriving = rect(&mut p, "body-person");
        after(&mut p, 400.0);
        let rest = rect(&mut p, "body-person");
        assert!(
            near(arriving.2, rest.2) && near(arriving.3, rest.3),
            "{transform}"
        );
        assert!(
            arriving.0 > rest.0 && arriving.0 < rest.0 + AXIS_SHIFT,
            "{transform}: {arriving:?}"
        );
        assert!(p.system_back(p.host().now()));
        p.run_commands(|| ());
        after(&mut p, 1.0);
        let back = rect(&mut p, "body-home");
        assert!(near(back.2, home.2) && near(back.3, home.3), "{transform}");
        assert!(
            back.0 < home.0 && back.0 > home.0 - AXIS_SHIFT,
            "{transform}"
        );
    }
}

#[test]
fn a_switch_and_a_pop_during_a_scroll_cross() {
    let list = "          scroll testId=`list-${e.name}` width=200 height=300\n            column height=2000 background-color=\"#00ff00\"\n          column testId=`body-${e.name}`";
    let source = TABS.replace("          column testId=`body-${e.name}`", list);
    let mut p = phone_source(&source);
    let home = id(&p, "list-home");
    p.scroll_to(home, (0.0, 100.0));
    after(&mut p, 16.0);
    // The press lands on the list's next frame, while it still moves.
    p.scroll_to(home, (0.0, 120.0));
    p.frame();
    let press = p
        .host
        .dispatch_at(id(&p, "people-home"), Event::Press, p.host().now());
    assert_eq!(press, None);
    assert_eq!(p.after_commit(), None);
    after(&mut p, 0.0);
    assert!(through(&p), "the switch fades through");
    after(&mut p, 300.0);
    tap(&mut p, "open-people");
    after(&mut p, 300.0);
    let person = id(&p, "list-person");
    p.scroll_to(person, (0.0, 100.0));
    after(&mut p, 16.0);
    p.scroll_to(person, (0.0, 120.0));
    p.frame();
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert!(p.stage_moving() && !through(&p), "the pop slides");
}

/// A route's earlier siblings removed with it: its drawing keeps its place
/// after the nearest one that remains, here above an overlay.
#[test]
fn a_removed_root_stays_above_the_sibling_before_those_removed_with_it() {
    let source = SWAP.replace(
        "      if !second\n        column navigationKey=\"a\"",
        "      column position=\"absolute\" left=0 top=400 width=412 height=100 background-color=\"#00ff00\"\n      if !second\n        column position=\"absolute\" left=0 top=0 width=10 height=10\n      if !second\n        column navigationKey=\"a\"",
    );
    let mut p = phone_source(&source);
    assert_eq!(red(&mut p, 200, 450), 255, "the root is above the overlay");
    tap(&mut p, "swap");
    after(&mut p, 20.0);
    let leaving = p.frame().pixel(200, 450).unwrap();
    assert!(leaving.red() > 0 && leaving.green() < 255, "{leaving:?}");
}

/// Tabpanels in a container that the third tab's switch removes with them:
/// the container's own paint is `WRAP`.
const CONTAINED: &str = r##"component App
  state full = false
  action swap
    full = !full
  view
    main navigationKey=(full ? "c" : "a") viewport-fit="cover" width="100%" height="100%" background-color="#000000"
      if !full
        column position="absolute" inset=0 WRAP
          column role="tabpanel" id="panel-a" position="absolute" inset=0
            column navigationKey="a" testId="original" position="absolute" top=0 left=0 width="100%" height=900 background-color="#ff0000"
              button press=swap testId="swap" height=40
                text "Swap"
          column role="tabpanel" id="panel-b" position="absolute" inset=0
            column navigationKey="b" position="absolute" inset=0
      if full
        column role="tabpanel" id="panel-c" position="absolute" inset=0
          column navigationKey="c" position="absolute" inset=0 background-color="#0000ff"
      row role="tablist" position="absolute" bottom=0 height=56 width="100%"
        button role="tab" aria-controls="panel-a" aria-selected=(!full) press=swap
          text "Home"
        button role="tab" aria-controls="panel-b" aria-selected=false
          text "People"
        button role="tab" aria-controls="panel-c" aria-selected=full press=swap
          text "Full"
"##;

/// The leaving page keeps the opacity of an ancestor removed with it, above
/// the tabpanels' common ancestor, which the destination stops below.
#[test]
fn a_removed_containers_opacity_stays_on_the_leaving_page() {
    let mut p = phone_source(&CONTAINED.replace("WRAP", "opacity=0.5"));
    let original = p.host().kernel().node(id(&p, "original")).unwrap().key;
    let resting = red(&mut p, 200, 500);
    assert_eq!(resting, 128);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(p.host().kernel().node_by_key(original).is_none());
    assert!(through(&p));
    assert_eq!(red(&mut p, 200, 500), resting);
    after(&mut p, 20.0);
    let (out, _) = p.shift_poses(p.host().now(), None);
    let leaving = red(&mut p, 200, 500);
    let expected = 128.0 * out.opacity;
    assert!(
        (leaving as f32 - expected).abs() <= 3.0,
        "{leaving}, {expected}"
    );
}

/// And the clip of one that hides its overflow.
#[test]
fn a_removed_containers_clip_stays_on_the_leaving_page() {
    let mut p = phone_source(&CONTAINED.replace("WRAP", "height=300 overflow=\"hidden\""));
    assert_eq!((red(&mut p, 200, 200), red(&mut p, 200, 500)), (255, 0));
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(through(&p));
    assert_eq!((red(&mut p, 200, 200), red(&mut p, 200, 500)), (255, 0));
    after(&mut p, 20.0);
    assert!(red(&mut p, 200, 200) > 0);
    assert_eq!(red(&mut p, 200, 500), 0);
}

#[test]
fn a_removed_branch_follows_its_surviving_ancestors_transform() {
    for wrap in [
        "translate=\"30px 0px\"",
        "translate=\"30px 0px\" opacity=0.5",
    ] {
        let source = CONTAINED.replace("WRAP", wrap).replace(
            "main navigationKey=",
            "main scale=(full ? 0.8 : 1) transform-origin=\"0 0\" navigationKey=",
        );
        let mut p = phone_source(&source);
        let resting = red(&mut p, 35, 500);
        assert!(resting > 0);
        assert_eq!(red(&mut p, 25, 500), 0);
        tap(&mut p, "swap");
        after(&mut p, 0.0);
        assert!(through(&p));
        assert_eq!(red(&mut p, 23, 500), 0);
        assert_eq!(red(&mut p, 25, 500), resting, "{wrap}");
        assert_eq!(p.frame().pixel(355, 500).unwrap().blue(), 255);
        after(&mut p, 20.0);
        let (out, _) = p.shift_poses(p.host().now(), None);
        let expected = resting as f32 * out.opacity;
        assert!((red(&mut p, 25, 500) as f32 - expected).abs() <= 3.0);
    }
}

#[test]
fn a_removed_branch_uses_its_surviving_ancestors_live_clip_and_opacity() {
    let source = CONTAINED.replace("WRAP", "translate=\"30px 0px\"").replace(
        "main navigationKey=",
        "main opacity=(full ? 0.5 : 1) max-height=(full ? 400 : 916) overflow=\"hidden\" navigationKey=",
    );
    let mut p = phone_source(&source);
    assert_eq!(red(&mut p, 35, 500), 255);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(p.frame().pixel(35, 200).unwrap().green().abs_diff(128) <= 1);
    assert_eq!(p.frame().pixel(35, 500).unwrap().blue(), 255);
    after(&mut p, 20.0);
    let (out, _) = p.shift_poses(p.host().now(), None);
    // The translucent black root composites over the painter's white page.
    let expected = 127.0 + 128.0 * out.opacity;
    assert!((red(&mut p, 35, 200) as f32 - expected).abs() <= 3.0);
    assert_eq!(p.frame().pixel(35, 500).unwrap().blue(), 255);
}

#[test]
fn a_removed_branch_follows_its_surviving_ancestors_layout_and_scroll() {
    let source = CONTAINED
        .replace("WRAP", "translate=\"30px 0px\"")
        .replace("    main ", "    main testId=\"port\" overflow=\"scroll\" position=\"absolute\" top=(full ? 100 : 0) ")
        .replace("height=\"100%\" background-color=\"#000000\"", "height=400 background-color=\"#000000\"")
        .replace("              button press=swap", "              column position=\"absolute\" left=100 top=200 width=40 height=40 background-color=\"#00ff00\"\n              button press=swap");
    let mut p = phone_source(&source);
    assert_eq!(p.frame().pixel(150, 210).unwrap().green(), 255);
    let port = id(&p, "port");
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert_eq!(p.frame().pixel(150, 310).unwrap().green(), 255);
    assert_eq!(p.frame().pixel(150, 210).unwrap().green(), 0);
    p.scroll.insert(port, (0.0, 100.0));
    p.dirty = true;
    let frame = p.frame();
    assert_eq!(frame.pixel(150, 210).unwrap().green(), 255);
    assert_eq!(frame.pixel(150, 310).unwrap().green(), 0);
    assert_eq!(frame.pixel(150, 90).unwrap().blue(), 255);
}

#[test]
fn a_removed_branch_keeps_its_ancestors_own_paint() {
    let source = CONTAINED
        .replace("WRAP", "background-color=\"#ff0000\"")
        .replace(
            "height=900 background-color=\"#ff0000\"",
            "height=900 background-color=\"transparent\"",
        );
    let mut p = phone_source(&source);
    assert_eq!(red(&mut p, 200, 500), 255);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert_eq!(red(&mut p, 200, 500), 255);
}

#[test]
fn a_removed_sticky_route_keeps_its_position() {
    let source = SWAP
        .replace(
            "main navigationKey=",
            "main testId=\"port\" overflow=\"scroll\" navigationKey=",
        )
        .replace("height=\"100%\"", "height=400")
        .replace(
            "position=\"absolute\" left=40 top=100 right=0 bottom=0",
            "position=\"sticky\" top=100 height=100",
        )
        .replace(
            "      if second",
            "      column height=1000\n      if second",
        );
    let mut p = phone_source(&source);
    let port = id(&p, "port");
    p.scroll.insert(port, (0.0, 100.0));
    p.dirty = true;
    assert_eq!((red(&mut p, 200, 150), red(&mut p, 200, 350)), (255, 0));
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    // The surviving scrollport moves while the branch fades.
    p.scroll.insert(port, (0.0, 200.0));
    p.dirty = true;
    assert_eq!((red(&mut p, 200, 150), red(&mut p, 200, 350)), (255, 0));
}

/// Compare the complete outgoing branch with the same branch still live.
/// The surviving context changes after removal, while the crossing's clock
/// stays at zero so its opacity is one and the incoming page is invisible.
#[test]
fn removed_branch_paint_compositing_and_positioning_matrix() {
    std::thread::scope(|scope| {
        for position in ["static", "relative", "sticky", "scrolled"] {
            scope.spawn(move || {
                for paint in [
                    "background-color=\"#ff0000\"",
                    "border=\"8px solid #ff0000\"",
                    "box-shadow=\"inset 8px 8px 0px #ff0000, -12px -16px 4px #ff0000, 12px 16px 0px #ff0000\"",
                ] {
                    for effect in [
                        "",
                        "opacity=0.5",
                        "overflow=\"hidden\"",
                        "clip-path=\"inset(4px)\"",
                        "translate=\"30px 0px\"",
                        "filter=\"opacity(0.7)\"",
                        "mask-image=\"linear-gradient(black, transparent)\"",
                    ] {
                        for changed in [false, true] {
                            branch_matrix_case(position, paint, effect, changed);
                        }
                    }
                }
            });
        }
    });
}

fn branch_matrix_case(position: &str, paint: &str, effect: &str, changed: bool) {
    let placement = match position {
        "relative" => "position=\"relative\" left=10 top=20",
        "sticky" => "position=\"sticky\" top=100",
        _ => "",
    };
    let source = format!(
        r##"component App
  state second = false
  state moved = false
  action swap
    second = true
  action move
    moved = true
  view
    main testId="port" navigationKey=(second ? "b" : "a") viewport-fit="cover" position="absolute" left=(moved ? 10 : 0) top=(moved ? 20 : 0) width=412 height=400 overflow="scroll" background-color="#000000" translate=(moved ? "40px 10px" : "30px 0px") scale=(moved ? 0.8 : 1) transform-origin="0 0" opacity=(moved ? 0.7 : 1) clip-path=(moved ? "inset(5px)" : "none") filter=(moved ? "opacity(0.9)" : "none") mask-image=(moved ? "linear-gradient(black, transparent)" : "none")
      column height=100
      if !second
        column role="tabpanel" id="panel-a" testId="branch" width=120 height=100 {placement} {paint} {effect}
          column navigationKey="a" width=120 height=100 background-color="transparent"
            column position="relative" left=40 top=60 width=100 height=60 background-color="#00ff00"
      if second
        column role="tabpanel" id="panel-b" position="absolute" inset=0
          column navigationKey="b" position="absolute" inset=0 background-color="#0000ff"
      column height=1000
      button press=swap testId="swap" position="absolute" top=600 height=40
        text "Swap"
      button press=move testId="move" position="absolute" top=640 height=40
        text "Move"
      row role="tablist" position="absolute" top=800 height=56 width=412
        button role="tab" aria-controls="panel-a" aria-selected=(!second)
          text "A"
        button role="tab" aria-controls="panel-b" aria-selected=second
          text "B"
"##
    );
    let label = format!("{position}, {paint}, {effect}, changed={changed}");
    let mut outgoing = phone_source(&source);
    let mut live = phone_source(&source);
    let scroll = matches!(position, "sticky" | "scrolled");
    for p in [&mut outgoing, &mut live] {
        if scroll {
            p.scroll.insert(id(p, "port"), (0.0, 100.0));
            p.dirty = true;
            p.frame();
        }
    }
    let branch = outgoing
        .host()
        .kernel()
        .node(id(&outgoing, "branch"))
        .unwrap()
        .key;
    // Dispatch directly: scrolling may have moved the fixture's controls
    // outside the scrollport, but input targeting is not under test.
    let dispatch = |p: &mut Presenter<()>, name: &str| {
        assert_eq!(
            p.host
                .dispatch_at(id(p, name), Event::Press, p.host().now()),
            None
        );
        assert_eq!(p.after_commit(), None);
        after(p, 0.0);
    };
    dispatch(&mut outgoing, "swap");
    if changed {
        for p in [&mut outgoing, &mut live] {
            dispatch(p, "move");
        }
    }
    for p in [&mut outgoing, &mut live] {
        if scroll {
            p.scroll
                .insert(id(p, "port"), (0.0, if changed { 140.0 } else { 100.0 }));
            p.dirty = true;
        }
    }
    assert!(through(&outgoing), "{label}");
    let expected = live.frame().clone();
    let actual = outgoing.frame().clone();
    let mut red_pixels = 0;
    for y in (0..400).step_by(2) {
        for x in (0..400).step_by(2) {
            let a = actual.pixel(x, y).unwrap();
            let b = expected.pixel(x, y).unwrap();
            red_pixels += usize::from(b.red() > b.green().saturating_add(10));
            // A filtered branch keeps its raster from before the scale.
            // Resampling its edges differs from rasterizing the live branch
            // at the new scale; allow one pixel of edge coverage, but keep
            // the interior colour comparison tight.
            let channels = |p: tiny_skia::PremultipliedColorU8| [p.red(), p.green(), p.blue()];
            let mut low = channels(b);
            let mut high = low;
            for ny in y.saturating_sub(1)..=(y + 1) {
                for nx in x.saturating_sub(1)..=(x + 1) {
                    for (i, value) in channels(expected.pixel(nx, ny).unwrap())
                        .into_iter()
                        .enumerate()
                    {
                        low[i] = low[i].min(value);
                        high[i] = high[i].max(value);
                    }
                }
            }
            assert!(
                channels(a)
                    .into_iter()
                    .enumerate()
                    .all(|(i, value)| value >= low[i].saturating_sub(2)
                        && value <= high[i].saturating_add(2)),
                "{label} at ({x}, {y}): {a:?}, expected {b:?}"
            );
        }
    }
    assert!(
        red_pixels > 20,
        "{label}: the removed ancestor's own paint is visible"
    );
    after(&mut outgoing, 300.0);
    assert!(
        !outgoing.brush.kept(branch),
        "{label}: the branch is released"
    );
}
