//! A route's Material container transform from its navigation source.
use super::*;

const ZOOM: &str = r##"routes nav
  home "/"
    detail "/detail"
component App
  state missing = false
  state moved = false
  action open
    nav = push(nav, "/detail")
  action back
    nav = back(nav)
  action remove
    missing = true
    nav = back(nav)
  action displace
    moved = true
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` testId="route-home" position="absolute" inset=0 background-color="#ffffff"
            text "Home" testId="home-body"
            button press=open testId="open" height=40
              text "Open"
            if !missing
              button id="card" press=open testId="card" margin-left=32 margin-top=(moved ? 2000 : 80) width=160 height=80 flex-shrink=0 background-color="#cc4433" border-radius=16
                text "Open record"
            button id="card" testId="duplicate" width=90 height=40
              text "Another card"
        if e.name == "detail"
          column navigationKey=`${e.id}` navigationSource="card" testId="route-detail" position="absolute" inset=0 background-color="#225599"
            text "Record" testId="detail-body"
            button press=remove testId="remove"
              text "Remove source and return"
            button press=displace testId="displace"
              text "Move source off screen and return"
"##;

fn poses(p: &Presenter<()>) -> (Pose, Pose) {
    p.shift_poses(p.host().now(), None)
}

fn back(p: &mut Presenter<()>) {
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
}

#[test]
fn a_source_route_grows_from_the_first_card_and_returns_from_its_kept_drawing() {
    for presentation in ["", " navigationPresentation=\"fullscreen\""] {
        let source = ZOOM.replace(
            "navigationSource=\"card\"",
            &format!("navigationSource=\"card\"{presentation}"),
        );
        let mut p = phone_source(&source);
        let card = rect(&mut p, "card");
        tap(&mut p, "card");
        after(&mut p, 0.0);
        let (beneath, start) = poses(&p);
        assert!(near(start.scale * 412., card.2), "{start:?} {card:?}");
        assert_eq!(beneath, Pose::REST);
        assert!(near(start.height.unwrap() * start.scale, card.3));
        assert!(near(start.radius * start.scale, 16.));
        assert!(p
            .stage_paint()
            .crossing
            .iter()
            .any(|cross| cross.height.is_some_and(|h| near(h * start.scale, card.3))));
        assert!(
            near(start.dx, card.0) && near(start.dy, card.1),
            "{start:?} {card:?}"
        );
        after(&mut p, 200.0);
        let middle = poses(&p).1;
        assert!(
            middle.scale > start.scale && middle.scale < 1.0,
            "{middle:?}"
        );
        assert!(middle.opacity > 0.99);
        after(&mut p, 200.0);
        assert!(!p.stage_moving());
        let detail = p.host.kernel().node(id(&p, "route-detail")).unwrap().key;
        assert!(
            p.brush.kept(detail),
            "full-screen routes also keep their drawing"
        );
        back(&mut p);
        after(&mut p, 175.0);
        assert!(!has(&p, "route-detail") && p.brush.kept(detail));
        let (leaving, beneath) = poses(&p);
        assert_eq!(beneath, Pose::REST);
        assert!(
            leaving.scale > start.scale && leaving.scale < 1.0,
            "{leaving:?}"
        );
        assert!(leaving.opacity > 0.99);
        after(&mut p, 130.0);
        assert!(poses(&p).0.opacity < 0.5);
        after(&mut p, 45.0);
        assert!(!p.stage_moving());
        assert!(!p.brush.kept(detail));
    }
}

#[test]
fn a_source_that_is_missing_hidden_or_outside_its_clip_uses_the_platform_fallback() {
    for source in [
        ZOOM.replace("navigationSource=\"card\"", "navigationSource=\"absent\""),
        ZOOM.replace("margin-left=32", "visibility=\"hidden\" margin-left=32"),
        ZOOM.replace("margin-left=32", "margin-left=2000"),
        ZOOM.replace("margin-left=32", "margin-left=32 translate=\"2000px 0\""),
        ZOOM.replace("margin-left=32", "margin-left=32").replace(
            "inset=0 background-color=\"#ffffff\"",
            "inset=0 overflow=\"hidden\" height=50 background-color=\"#ffffff\"",
        ),
    ] {
        for source in [
            source.clone(),
            source.replace(
                "navigationSource=\"",
                "navigationPresentation=\"fullscreen\" navigationSource=\"",
            ),
        ] {
            let mut p = phone_source(&source);
            tap(&mut p, "open");
            after(&mut p, 0.0);
            let (out, incoming) = poses(&p);
            assert_eq!(incoming.scale, 1.0);
            if source.contains("navigationPresentation=\"fullscreen\"") {
                assert!(!p.stage_moving(), "an unresolved full-screen source cuts");
                assert_eq!((out, incoming), (Pose::REST, Pose::REST));
                let detail = p.host.kernel().node(id(&p, "route-detail")).unwrap().key;
                assert!(p.stage.zooms.is_empty());
                assert!(!p.back_progress(0, 0., 0, p.host.now()));
                back(&mut p);
                after(&mut p, 0.);
                assert!(!p.brush.kept(detail));
            } else {
                assert!(near(incoming.dx, AXIS_SHIFT), "{incoming:?}");
                assert_eq!(out.scale, 1.0);
            }
        }
    }
}

#[test]
fn a_source_removed_or_moved_off_screen_before_back_keeps_the_shared_axis() {
    for action in ["remove", "displace"] {
        let mut p = phone_source(ZOOM);
        tap(&mut p, "card");
        after(&mut p, 400.0);
        tap(&mut p, action);
        after(&mut p, 0.0);
        let (out, incoming) = poses(&p);
        assert_eq!(out.scale, 1.0);
        assert!(near(incoming.dx, -AXIS_SHIFT), "{incoming:?}");
    }
}

#[test]
fn a_zoom_pop_starts_at_the_predictive_back_pose() {
    for source in [
        ZOOM.to_owned(),
        ZOOM.replace(
            "navigationSource=\"card\"",
            "navigationSource=\"card\" navigationPresentation=\"fullscreen\"",
        ),
    ] {
        let mut p = phone_source(&source);
        tap(&mut p, "card");
        after(&mut p, 400.0);
        let now = p.host().now();
        assert!(p.back_progress(0, 0.0, 0, now));
        assert!(p.back_progress(1, 0.6, 0, now));
        after(&mut p, 0.0);
        let preview = poses(&p).0;
        back(&mut p);
        after(&mut p, 0.0);
        let leaving = poses(&p).0;
        let before = preview.transform((0., 0.));
        let committed = leaving.transform((0., 0.));
        assert!(
            near(before.sx, committed.sx)
                && near(before.tx, committed.tx)
                && near(before.ty, committed.ty)
        );
        after(&mut p, 200.0);
        assert!(poses(&p).0.scale < preview.scale);
    }
}

#[test]
fn reduced_motion_cuts_a_source_routes_push_and_pop() {
    let mut p = phone_source(ZOOM);
    let reply = crate::agent::handle(
        &mut p,
        r#"{"op":"prefer","media":{"prefers-reduced-motion":"reduce"}}"#,
    );
    assert!(!reply.contains("error"), "{reply}");
    tap(&mut p, "card");
    after(&mut p, 0.0);
    assert!(!p.stage_moving());
    back(&mut p);
    after(&mut p, 0.0);
    assert!(!p.stage_moving());
}

#[test]
fn a_source_uses_its_scrolled_transformed_box_and_rechecks_it_on_back() {
    let source = ZOOM
        .replace("column navigationKey=", "scroll navigationKey=")
        .replace(
            "testId=\"duplicate\" width",
            "testId=\"duplicate\" margin-top=1200 width",
        )
        .replace(
            "margin-top=(moved ? 2000 : 80)",
            "translate=\"20px 30px\" margin-top=(moved ? 2000 : 180)",
        );
    let mut p = phone_source(&source);
    let home = id(&p, "route-home");
    p.scroll.insert(home, (0., 90.));
    p.dirty = true;
    let card = rect(&mut p, "card");
    tap(&mut p, "card");
    after(&mut p, 0.);
    let start = poses(&p).1;
    assert!(
        near(start.dx, card.0) && near(start.dy, card.1),
        "{start:?} {card:?}"
    );
    after(&mut p, 400.);
    p.scroll.insert(home, (0., 120.));
    back(&mut p);
    let end = p.stage.shift.as_ref().unwrap().progress.start + 350.;
    let leaving = p.shift_poses(end - 0.001, None).0;
    assert!(
        near(leaving.dx, card.0) && near(leaving.dy, card.1 - 30.),
        "{leaving:?} {card:?}"
    );
}

#[test]
fn a_container_entrance_holds_coordinate_input_until_it_finishes() {
    let source = ZOOM.replace(
        "button press=remove testId=\"remove\"",
        "button press=back testId=\"hidden-back\" margin-top=480 height=40 flex-shrink=0",
    );
    let mut p = phone_source(&source);
    tap(&mut p, "card");
    after(&mut p, 50.);
    let (x, y, w, h) = rect(&mut p, "hidden-back");
    let now = p.host.now();
    p.pointer_down(x + w / 2., y + h / 2., now).unwrap();
    p.pointer_up(x + w / 2., y + h / 2., now + 1.).unwrap();
    p.run_commands(|| ());
    assert!(
        has(&p, "route-detail"),
        "the invisible control took the press"
    );
    after(&mut p, 400.);
    let (x, y, w, h) = rect(&mut p, "hidden-back");
    let now = p.host.now();
    p.pointer_down(x + w / 2., y + h / 2., now).unwrap();
    p.pointer_up(x + w / 2., y + h / 2., now + 1.).unwrap();
    p.run_commands(|| ());
    assert!(!has(&p, "route-detail"));
}

#[test]
fn a_source_survives_switching_tabs_until_its_route_leaves_the_stack() {
    let source = ZOOM
        .replace("  home \"/\"", "  tab home \"/\"")
        .replace("component App", "  tab other \"/other\"\ncomponent App")
        .replace(
            "  action back",
            "  action pick(name: string)\n    nav = select(nav, name)\n  action back",
        );
    let (head, routes) = source
        .split_once("      for e in stack(nav) key=e.id\n")
        .unwrap();
    let routes = routes
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>();
    let source = format!(
        r#"{head}      for t in nav.tabs key=t.name
        column role="tabpanel" id=`panel-${{t.name}}` position="absolute" inset=0
          for e in t.stack key=e.id
{routes}            if e.name == "other"
              column navigationKey=`${{e.id}}` position="absolute" inset=0
                text "Other"
      row role="tablist"
        button role="tab" aria-controls="panel-home" aria-selected=(nav.tab == "home") press=pick("home") testId="tab-home"
          text "Home"
        button role="tab" aria-controls="panel-other" aria-selected=(nav.tab == "other") press=pick("other") testId="tab-other"
          text "Other"
"#
    );
    let mut p = phone_source(&source);
    let card = rect(&mut p, "card");
    tap(&mut p, "card");
    after(&mut p, 400.);
    tap(&mut p, "tab-other");
    after(&mut p, 0.);
    tap(&mut p, "tab-home");
    after(&mut p, 0.);
    back(&mut p);
    let end = p.stage.shift.as_ref().unwrap().progress.start + 350.;
    let leaving = p.shift_poses(end - 0.001, None).0;
    assert!(near(leaving.scale * 412., card.2), "{leaving:?}");
    assert!(
        p.stage.zooms.is_empty(),
        "a popped route retires its source"
    );
}

fn changed_source(value: &str) -> Presenter<()> {
    let source = ZOOM
        .replace(
            "  state missing",
            "  state source = \"card\"\n  state missing",
        )
        .replace(
            "  action back",
            &format!("  action change\n    source = \"{value}\"\n  action back"),
        )
        .replace("navigationSource=\"card\"", "navigationSource=source")
        .replace(
            "button id=\"card\" testId=\"duplicate\"",
            "button id=\"second\" testId=\"duplicate\"",
        )
        .replace("button press=remove", "button press=change");
    let mut p = phone_source(&source);
    tap(&mut p, "card");
    after(&mut p, 400.);
    tap(&mut p, "remove");
    after(&mut p, 0.);
    p
}

#[test]
fn a_return_uses_the_routes_current_source_name_and_identity() {
    let mut p = changed_source("second");
    let frame = p.host.kernel().node(id(&p, "duplicate")).unwrap().frame;
    let second = (frame.x, frame.y, frame.width, frame.height);
    back(&mut p);
    let end = p.stage.shift.as_ref().unwrap().progress.start + 350.;
    let leaving = p.shift_poses(end - 0.001, None).0;
    assert!(
        near(leaving.scale * 412., second.2),
        "{leaving:?} {second:?}"
    );
    assert!(near(leaving.dy, second.1), "{leaving:?} {second:?}");
}

#[test]
fn clearing_a_live_routes_source_disables_its_return_zoom() {
    let mut p = changed_source("");
    back(&mut p);
    after(&mut p, 100.);
    assert_eq!(poses(&p).0.scale, 1.);
}

fn shape_clipped_source() -> String {
    ZOOM.replace("  state missing", "  state clipped = false\n  state missing")
        .replace("  action back", "  action clip\n    clipped = true\n  action back")
        .replace("button press=remove", "button press=clip")
        .replace("inset=0 background-color=\"#ffffff\"",
            "inset=0 clip-path=(clipped ? \"inset(0 0 90% 0)\" : \"none\") background-color=\"#ffffff\"")
}

#[test]
fn an_ancestor_shape_clip_can_hide_a_source_on_push() {
    let source = shape_clipped_source().replace("state clipped = false", "state clipped = true");
    let mut p = phone_source(&source);
    tap(&mut p, "open");
    after(&mut p, 0.);
    assert!(near(poses(&p).1.dx, AXIS_SHIFT));
    assert_eq!(poses(&p).1.scale, 1.);
}

#[test]
fn an_ancestor_shape_clip_can_hide_a_source_on_return() {
    let mut p = phone_source(&shape_clipped_source());
    tap(&mut p, "card");
    after(&mut p, 400.);
    tap(&mut p, "remove");
    after(&mut p, 0.);
    back(&mut p);
    after(&mut p, 100.);
    assert_eq!(poses(&p).0.scale, 1.);
}

const CLIPPED_CARD: &str = r##"routes nav
  home "/"
    detail "/detail"
component App
  state clipped = false
  state moving = false
  action open
    nav = push(nav, "/detail")
  action animate
    moving = true
  action clip
    clipped = true
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` testId="route-home" position="absolute" inset=0
            button "Open" press=open testId="open" height=40
            button "Animate" press=animate testId="animate" height=40
            box testId="parent" position="absolute" left=32 top=160 width=100 height=100 overflow="hidden" border-radius=(clipped ? 50 : 0)
              box id="card" testId="card" position="absolute" left=0 top=0 width=5 height=5 background-color="red"
        if e.name == "detail"
          column navigationKey=`${e.id}` navigationSource="card" testId="route-detail" position="absolute" inset=0
            button "Clip" press=clip testId="clip"
            button "Animate" press=animate testId="animate-detail"
"##;

fn source_presentation(source: &str, fullscreen: bool) -> String {
    source.replace(
        "navigationSource=\"card\"",
        if fullscreen {
            "navigationSource=\"card\" navigationPresentation=\"fullscreen\""
        } else {
            "navigationSource=\"card\""
        },
    )
}

fn source_fallback(p: &Presenter<()>, on_push: bool, fullscreen: bool) {
    let (out, incoming) = poses(p);
    assert_eq!((out.scale, incoming.scale), (1., 1.));
    if fullscreen {
        assert!(!p.stage_moving());
        assert_eq!((out, incoming), (Pose::REST, Pose::REST));
    } else {
        assert!(near(
            incoming.dx,
            if on_push { AXIS_SHIFT } else { -AXIS_SHIFT }
        ));
    }
}

#[test]
fn a_rounded_overflow_corner_hides_a_source_on_push_and_return() {
    for fullscreen in [false, true] {
        for on_push in [true, false] {
            let source = CLIPPED_CARD.replace(
                "state clipped = false",
                &format!("state clipped = {on_push}"),
            );
            let mut p = phone_source(&source_presentation(&source, fullscreen));
            let card = rect(&mut p, "card");
            assert_eq!((card.2, card.3), (5., 5.));
            tap(&mut p, "open");
            after(&mut p, 0.);
            if !on_push {
                assert!(near(poses(&p).1.scale * 412., 5.));
                after(&mut p, 400.);
                tap(&mut p, "clip");
                after(&mut p, 0.);
                back(&mut p);
                after(&mut p, 0.);
            }
            source_fallback(&p, on_push, fullscreen);
        }
    }
}

#[test]
fn reader_owned_source_or_ancestor_motion_uses_the_fallback_on_push_and_return() {
    for (property, from, to) in [
        ("translate", "0px 0", "200px 0"),
        ("scale", "1", "0.5"),
        ("rotate", "0deg", "90deg"),
        ("opacity", "1", "0"),
    ] {
        for target in ["card", "parent"] {
            for fullscreen in [false, true] {
                for on_push in [true, false] {
                    let source = format!("keyframes move\n  from {property}=\"{from}\"\n  to {property}=\"{to}\"\n{CLIPPED_CARD}")
                        .replace(&format!("testId=\"{target}\" position="),
                            &format!("testId=\"{target}\" animation=(moving ? \"move 1s linear infinite\" : \"none\") position="));
                    let mut p = phone_source(&source_presentation(&source, fullscreen));
                    p.host_mut().lowering = true;
                    p.host_mut()
                        .engine
                        .set_lowered_properties(&crate::host::lower::LOWERED);
                    if !on_push {
                        tap(&mut p, "open");
                        after(&mut p, 400.);
                    }
                    tap(&mut p, if on_push { "animate" } else { "animate-detail" });
                    after(&mut p, 500.);
                    let key = p.host.kernel().node(id(&p, target)).unwrap().key;
                    assert_ne!(
                        p.host.lowered_mask(key)
                            & (crate::host::lower::LOWER_TRANSFORM
                                | crate::host::lower::LOWER_OPACITY),
                        0,
                        "{property} on {target} must be reader-owned"
                    );
                    if on_push {
                        tap(&mut p, "open");
                    } else {
                        back(&mut p);
                    }
                    after(&mut p, 0.);
                    source_fallback(&p, on_push, fullscreen);
                }
            }
        }
    }
}

#[test]
fn a_route_change_during_a_full_screen_entrance_ends_it() {
    // A timer removes the source and pops the route 100 ms into its 400 ms
    // entrance (input is held, so no tap could): the pop cuts, and nothing
    // keeps growing the route that left.
    let source = ZOOM
        .replace(
            "navigationSource=\"card\"",
            "navigationSource=\"card\" navigationPresentation=\"fullscreen\"",
        )
        .replace(
            "  view\n",
            "  task leave when top(nav).name == \"detail\" && !missing\n    after(100, remove)\n  view\n",
        );
    let mut p = phone_source(&source);
    tap(&mut p, "card");
    after(&mut p, 50.0);
    assert!(p.stage_moving() && has(&p, "route-detail"));
    let at = p.host().now() + 60.0;
    p.advance(at);
    p.run_commands(|| ());
    after(&mut p, 0.0);
    assert!(!has(&p, "route-detail"));
    assert!(
        !p.stage_moving(),
        "the interrupted entrance ended with the pop"
    );
    assert_eq!(poses(&p), (Pose::REST, Pose::REST));
}

#[test]
fn a_source_under_a_rotation_takes_the_fallback() {
    for fullscreen in [false, true] {
        let source = ZOOM.replace("margin-left=32", "margin-left=32 rotate=\"10deg\"");
        let mut p = phone_source(&source_presentation(&source, fullscreen));
        tap(&mut p, "open");
        after(&mut p, 0.);
        source_fallback(&p, true, fullscreen);
    }
}

/// Fading parts on a route whose `navigationSource` is computed.
const PARTS_AND_SOURCE: &str = r##"keyframes fade-in
  from opacity=0
keyframes fade-out
  to opacity=0
style Fades
  ::view-transition-new
    animation="fade-in 300ms linear both"
  ::view-transition-old
    animation="fade-out 300ms linear both"
"##;

/// `part` gives no part to a route whose current source is set, so a computed
/// zoom is the platform's transition both ways (LLP 1013.001 D2).
#[test]
fn a_computed_zoom_wins_over_the_routes_authored_parts_both_ways() {
    let source = format!("{PARTS_AND_SOURCE}{ZOOM}")
        .replace(
            "state moved = false",
            "state moved = false\n  state zoom = true",
        )
        .replace(
            "navigationSource=\"card\"",
            "class=Fades navigationSource=(zoom ? \"card\" : \"\")",
        );
    let mut p = phone_source(&source);
    let card = rect(&mut p, "card");
    tap(&mut p, "card");
    after(&mut p, 0.0);
    let shift = p.stage.shift.as_ref().expect("the push moves");
    assert!(
        shift.authored.is_none() && shift.container.is_some(),
        "the push zooms, not the parts"
    );
    let start = poses(&p).1;
    assert!(
        near(start.scale * 412., card.2),
        "the push zooms: {start:?}"
    );
    after(&mut p, 400.0);
    assert!(!p.stage_moving());
    back(&mut p);
    after(&mut p, 175.0);
    let leaving = poses(&p).0;
    assert!(
        leaving.scale < 1.0,
        "the pop returns into the card: {leaving:?}"
    );
}
