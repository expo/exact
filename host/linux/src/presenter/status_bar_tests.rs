//! Style-resolution cases from StatusBarIOSTests.swift, plus route scope
//! and the Android page presentation in place of UIKit's detented sheet.
use super::*;

fn boot(body: &str) -> Presenter<()> {
    source(&format!("component App\n  view\n{body}"))
}

fn source(source: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn choice(p: &Presenter<()>, style: &str, from: Option<&str>, light: bool) {
    let bar = p.status_bar();
    assert_eq!(bar.style, style);
    assert_eq!(bar.source, from.map(|name| id(p, name)));
    assert_eq!(p.chrome().status_light_icons, light);
}

// Contract refuses nested navigation roots. These host guards still need
// a tree with one, built through the kernel's property operations.
fn nested_navigation(p: &mut Presenter<()>) {
    p.host.apply_test_ops(&[exact_kernel::Op::SetProp {
        id: id(p, "navigator"),
        prop: PropId::NavigationBack,
        value: exact_kernel::PropValue::Str("back".into()),
    }]);
    assert_eq!(p.host.commit(&[], None), None);
    assert_eq!(p.after_commit(), None);
}

fn screen(route: &str, header: &str, overlay: &str, header_style: &str) -> Presenter<()> {
    boot(&format!(
        r#"    main
      box testId="route" {route}
        box
          box testId="header" {header} {header_style}
      box testId="overlay" {overlay}
"#
    ))
}

#[test]
fn no_declaration_is_the_default() {
    let mut p = screen("", "", "", "");
    choice(&p, "default", None, false);
    p.set_system_scheme(true);
    choice(&p, "default", None, true);
    assert!(p.chrome().dark);
}

#[test]
fn the_declaration_painted_on_top_wins() {
    let light = r#"status-bar-style="light-content""#;
    let dark = r#"status-bar-style="dark-content""#;
    choice(
        &screen(light, "", "", ""),
        "light-content",
        Some("route"),
        true,
    );
    choice(
        &screen(light, dark, "", ""),
        "dark-content",
        Some("header"),
        false,
    );
    choice(
        &screen("", dark, light, ""),
        "light-content",
        Some("overlay"),
        true,
    );
    for hidden in [r#"display="none""#, r#"visibility="hidden""#] {
        choice(
            &screen(light, dark, "", hidden),
            "light-content",
            Some("route"),
            true,
        );
    }
}

#[test]
fn auto_is_the_scheme_where_it_sits() {
    let auto = r#"status-bar-style="auto""#;
    let mut p = screen("", auto, "", "");
    choice(&p, "default", Some("header"), false);
    p.set_system_scheme(true);
    choice(&p, "default", Some("header"), true);
    for (scheme, style, light) in [
        ("dark", "light-content", true),
        ("light", "dark-content", false),
    ] {
        let mut p = screen(&format!("color-scheme=\"{scheme}\""), auto, "", "");
        p.set_system_scheme(!light);
        choice(&p, style, Some("header"), light);
        assert_eq!(
            p.chrome().dark,
            !light,
            "navigation icons keep the page scheme"
        );
    }
}

#[test]
fn a_threshold_flip_changes_the_bar_in_its_own_commit() {
    let mut p = source(
        r#"component App
  state compact = false
  action flip
    compact = not compact
  view
    main status-bar-style="light-content" testId="route"
      if compact
        box status-bar-style="dark-content" status-bar-animation="fade" testId="header"
      button "Flip" press=flip testId="flip"
"#,
    );
    choice(&p, "light-content", Some("route"), true);
    let flip = id(&p, "flip");
    p.tap(flip).unwrap();
    choice(&p, "dark-content", Some("header"), false);
    p.tap(flip).unwrap();
    choice(&p, "light-content", Some("route"), true);
}

#[test]
fn visibility_can_be_restored_but_display_none_cannot() {
    for (hidden, from) in [
        ("visibility=\"hidden\"", "header"),
        ("display=\"none\"", "root"),
    ] {
        let p = boot(&format!(
            r#"    main testId="root" status-bar-style="light-content"
      box {hidden}
        box testId="header" visibility="visible" status-bar-style="dark-content"
"#
        ));
        choice(
            &p,
            if from == "root" {
                "light-content"
            } else {
                "dark-content"
            },
            Some(from),
            from == "root",
        );
    }
}

#[test]
fn paint_rank_beats_later_siblings_but_not_a_descendant() {
    let p = screen(
        r#"position="relative" z-index=2"#,
        r#"status-bar-style="dark-content""#,
        r#"status-bar-style="light-content" position="relative" z-index=1"#,
        "",
    );
    choice(&p, "dark-content", Some("header"), false);
    let p = screen(
        r#"status-bar-style="light-content""#,
        r#"status-bar-style="dark-content""#,
        "",
        r#"position="relative" z-index=-1"#,
    );
    choice(&p, "dark-content", Some("header"), false);
}

#[test]
fn inline_text_and_projected_dialogs_do_not_compete() {
    let p = boot(
        r#"    main testId="root" status-bar-style="light-content"
      text
        text "inline" status-bar-style="dark-content"
      dialog status-bar-style="dark-content"
        text "dialog"
      box popover="auto" status-bar-style="dark-content"
        text "menu"
"#,
    );
    choice(&p, "light-content", Some("root"), true);
}

#[test]
fn invalid_bound_values_are_unset() {
    let p = source(
        r#"component App
  state invalid = "other"
  view
    main status-bar-style="light-content" testId="root"
      box status-bar-style=invalid
"#,
    );
    choice(&p, "light-content", Some("root"), true);
    let mut p = source(
        r#"component App
  state invalid = "other"
  view
    main status-bar-style=invalid
"#,
    );
    choice(&p, "default", None, false);
    p.set_system_scheme(true);
    choice(&p, "default", None, true);
}

fn routes(presentation: &str, declaration: &str) -> Presenter<()> {
    source(&format!(
        r#"component App
  state route = "home"
  action open
    route = "detail"
  action push
    route = "next"
  action back
    route = "home"
  view
    main navigationKey=route navigationBack="back" status-bar-style="light-content" testId="root"
      box navigationKey="home" testId="home" status-bar-style="light-content"
        button "Open" press=open testId="open"
      box navigationKey="detail" navigationPresentation="{presentation}" {declaration} testId="detail"
        header status-bar-style="dark-content" testId="header"
          h1 "Detail"
        button "Push" press=push testId="push"
        button "Back" press=back id="back"
      box navigationKey="next" testId="next" status-bar-style="light-content"
        button "Back" press=back id="back"
      box testId="overlay" status-bar-style="light-content" position="relative" z-index=10
"#
    ))
}

#[test]
fn an_overlay_painted_above_a_covering_page_keeps_its_declaration() {
    for presentation in ["push", "fullscreen", "modal"] {
        let mut p = routes(presentation, "");
        choice(&p, "light-content", Some("overlay"), true);
        let open = id(&p, "open");
        p.tap(open).unwrap();
        choice(&p, "light-content", Some("overlay"), true);
        let push = id(&p, "push");
        p.tap(push).unwrap();
        choice(&p, "light-content", Some("overlay"), true);
        assert!(p.system_back(0.));
        choice(&p, "light-content", Some("overlay"), true);
    }
}

#[test]
fn a_covering_page_without_a_declaration_uses_the_scheme() {
    let source = r#"component App
  view
    main navigationKey="cover" navigationBack="back" status-bar-style="light-content"
      box navigationKey="home" status-bar-style="light-content"
      box navigationKey="cover" navigationPresentation="fullscreen"
        text "Cover"
"#;
    let mut p = self::source(source);
    choice(&p, "default", None, false);
    p.set_system_scheme(true);
    choice(&p, "default", None, true);
}

#[test]
fn explicit_icons_override_either_scheme_and_leave_navigation_alone() {
    for dark in [false, true] {
        for (style, light) in [("light-content", true), ("dark-content", false)] {
            let mut p = screen(&format!("status-bar-style=\"{style}\""), "", "", "");
            p.set_system_scheme(dark);
            choice(&p, style, Some("route"), light);
            assert_eq!(p.chrome().dark, dark);
        }
    }
}

#[test]
fn an_unselected_tab_does_not_compete() {
    let mut p = source(
        r#"component App
  state selected = "a"
  action pick
    selected = "b"
  action back
    selected = "a"
  view
    main navigationKey=selected navigationBack="back"
      box role="tabpanel" id="panel-a"
        box navigationKey="a" status-bar-style="light-content" testId="a"
          button "Pick" press=pick testId="pick"
      box role="tabpanel" id="panel-b"
        box navigationKey="b" status-bar-style="dark-content" testId="b"
      row role="tablist"
        button "A" role="tab" aria-controls="panel-a" aria-selected=(selected == "a")
        button "B" role="tab" aria-controls="panel-b" aria-selected=(selected == "b")
"#,
    );
    choice(&p, "light-content", Some("a"), true);
    let pick = id(&p, "pick");
    p.tap(pick).unwrap();
    choice(&p, "dark-content", Some("b"), false);
}

#[test]
fn a_header_lifted_into_the_platform_bar_still_counts() {
    struct Empty;
    impl top::SurfacePainter for Empty {
        fn record(
            &mut self,
            _: (f32, f32),
            _: f32,
            _: &mut dyn FnMut(&mut dyn Backend),
        ) -> Vec<u32> {
            Vec::new()
        }
    }
    let mut p = source(
        r#"component App
  view
    main navigationKey="home" navigationBack="back" width="100%" height="100%" status-bar-style="light-content"
      column navigationKey="home" width="100%" height="100%"
        header testId="header" status-bar-style="dark-content"
          h1 "Home"
        text "Page"
"#,
    );
    assert!(p.serve_bars(Box::new(Empty)).is_none());
    assert!(
        p.host.route_visibility(id(&p, "header")).0,
        "the page omits the lifted header"
    );
    choice(&p, "dark-content", Some("header"), false);
}

#[test]
fn a_covering_routes_hidden_style_does_not_restore_the_underlying_scope() {
    let p = boot(
        r#"    main navigationKey="cover" navigationBack="back" status-bar-style="light-content"
      box navigationKey="home" status-bar-style="light-content"
      box navigationKey="cover" navigationPresentation="fullscreen" visibility="hidden" status-bar-style="light-content"
        box testId="visible" visibility="visible" status-bar-style="dark-content"
"#,
    );
    choice(&p, "dark-content", Some("visible"), false);
}

#[test]
fn a_push_after_a_hidden_presentation_keeps_the_root_declaration() {
    for presentation in ["modal", "fullscreen"] {
        let p = boot(&format!(
            r#"    main navigationKey="detail" navigationBack="back" testId="root" status-bar-style="light-content"
      box navigationKey="home"
      box navigationKey="add" navigationPresentation="{presentation}" testId="add"
      box navigationKey="detail" testId="detail"
"#
        ));
        assert!(
            p.host
                .navigation
                .visibility_of(p.host.kernel(), id(&p, "add"), false)
                .0
        );
        choice(&p, "light-content", Some("root"), true);
    }
}

#[test]
fn a_display_none_navigation_root_cannot_cover_the_bar() {
    let mut p = boot(
        r#"    main testId="root" status-bar-style="light-content"
      text "Visible page"
      box display="none" navigationKey="cover" testId="navigator"
        box navigationKey="cover" navigationPresentation="fullscreen"
          text "Hidden page"
"#,
    );
    nested_navigation(&mut p);
    choice(&p, "light-content", Some("root"), true);
}

#[test]
fn toggling_a_navigation_roots_display_releases_and_recaptures_the_bar() {
    let mut p = source(
        r#"component App
  state shown = true
  action flip
    shown = not shown
  view
    main testId="root" status-bar-style="light-content"
      button "Flip" press=flip testId="flip"
      box display=(shown ? "block" : "none") navigationKey="cover" testId="navigator"
        box navigationKey="cover" navigationPresentation="fullscreen"
          text "Cover"
"#,
    );
    nested_navigation(&mut p);
    choice(&p, "default", None, false);
    let flip = id(&p, "flip");
    p.tap(flip).unwrap();
    choice(&p, "light-content", Some("root"), true);
    p.tap(flip).unwrap();
    choice(&p, "default", None, false);
}

#[test]
fn navigation_in_a_projection_cannot_cover_the_bar() {
    for projection in ["dialog", "box popover=\"auto\""] {
        let mut p = boot(&format!(
            r#"    main testId="root" status-bar-style="light-content"
      {projection}
        box navigationKey="cover" testId="navigator"
          box navigationKey="cover" navigationPresentation="fullscreen"
            text "Projected page"
"#
        ));
        nested_navigation(&mut p);
        choice(&p, "light-content", Some("root"), true);
    }
}

#[test]
fn an_overlay_above_a_modal_is_visible_in_the_painted_frame() {
    for presentation in ["modal", "fullscreen"] {
        let mut p = boot(&format!(
            r##"    main navigationKey="cover" navigationBack="back" width=400 height=800
      box navigationKey="home"
      box navigationKey="cover" navigationPresentation="{presentation}" background-color="#FFFFFF" width=400 height=800
      box position="absolute" top=0 left=0 width=400 height=80 z-index=10 background-color="#B50000" status-bar-style="light-content" testId="overlay"
"##
        ));
        let pixel = p.frame().pixel(20, 20).unwrap().demultiply();
        assert_eq!([pixel.red(), pixel.green(), pixel.blue()], [181, 0, 0]);
        choice(&p, "light-content", Some("overlay"), true);
    }
}

fn resolutions(p: &Presenter<()>) -> usize {
    p.window.status_bar.resolutions.get()
}

fn scrolling_page() -> Presenter<()> {
    source(
        r#"component App
  view
    main status-bar-style="light-content" width=400 height=800
      scroll testId="scroll" width=400 height=600
        column
          for item in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] key=item
            box height=100
              text "Row"
"#,
    )
}

#[test]
fn unchanged_chrome_reuses_the_status_bar_resolution() {
    let p = scrolling_page();
    assert!(p.chrome().status_light_icons);
    let before = resolutions(&p);
    for _ in 0..60 {
        assert!(p.chrome().status_light_icons);
    }
    assert_eq!(resolutions(&p), before);
}

#[test]
fn scrolling_frames_reuse_the_status_bar_resolution() {
    let mut p = scrolling_page();
    p.frame();
    assert!(p.chrome().status_light_icons);
    let before = resolutions(&p);
    let scroll = id(&p, "scroll");
    for y in [20., 50., 100., 200.] {
        assert_eq!(p.scroll_to(scroll, (0., y)), Some((0., y)));
        p.frame();
        assert!(p.chrome().status_light_icons);
    }
    assert_eq!(resolutions(&p), before);
}

#[test]
fn status_bar_cache_follows_props_routes_display_and_scheme() {
    let mut p = source(
        r#"component App
  state route = "home"
  state light = true
  state shown = true
  action flip
    light = not light
  action open
    route = "detail"
  action hide
    shown = not shown
  view
    main navigationKey=route navigationBack="back"
      button "Flip" press=flip testId="flip"
      button "Open" press=open testId="open"
      button "Hide" press=hide testId="hide"
      box navigationKey="home" status-bar-style=(light ? "light-content" : "dark-content") testId="home"
      box navigationKey="detail" display=(shown ? "block" : "none") status-bar-style="light-content" testId="detail"
"#,
    );
    choice(&p, "light-content", Some("home"), true);
    for (button, style, from, light) in [
        ("flip", "dark-content", Some("home"), false),
        ("open", "light-content", Some("detail"), true),
        ("hide", "default", None, false),
    ] {
        let before = resolutions(&p);
        let button = id(&p, button);
        p.tap(button).unwrap();
        choice(&p, style, from, light);
        assert_eq!(resolutions(&p), before + 1);
    }
    let before = resolutions(&p);
    p.set_system_scheme(true);
    choice(&p, "default", None, true);
    assert_eq!(resolutions(&p), before + 1);
    let before = resolutions(&p);
    p.set_system_scheme(true);
    choice(&p, "default", None, true);
    assert_eq!(resolutions(&p), before);
}

fn normal_under_scheme(scheme: &str, light: bool) {
    let mut p = source(&format!(
        r##"component App
  view
    main color-scheme="{scheme}" width=400 height=800
      box status-bar-style="auto" testId="normal" width=100 height=100 background-color="light-dark(#FFFFFF, #000000)"
"##
    ));
    // Contract rejects literal `normal` and unsets a bound one. Exercise the
    // kernel row directly: the presenter must also handle this explicit value.
    let normal = id(&p, "normal");
    let mut style = exact_kernel::StyleProps::default();
    style.mask.set(exact_kernel::StyleId::ColorScheme);
    p.host.apply_test_ops(&[exact_kernel::Op::SetStyle {
        id: normal,
        patch: Box::new(style),
    }]);
    assert_eq!(
        p.host.kernel().node(normal).unwrap().color_scheme_dark(),
        None
    );
    p.set_system_scheme(!light);
    let pixel = p.frame().pixel(20, 20).unwrap().demultiply();
    let painted = if light { 0 } else { 255 };
    assert_eq!([pixel.red(), pixel.green(), pixel.blue()], [painted; 3]);
    choice(
        &p,
        if light {
            "light-content"
        } else {
            "dark-content"
        },
        Some("normal"),
        light,
    );
    assert_eq!(p.chrome().dark, !light);
}

#[test]
fn auto_under_normal_keeps_a_dark_ancestors_painted_scheme() {
    normal_under_scheme("dark", true);
}

#[test]
fn auto_under_normal_keeps_a_light_ancestors_painted_scheme() {
    normal_under_scheme("light", false);
}

#[test]
fn status_bar_cache_follows_preferences_and_view_appearance() {
    let mut p = screen("", r#"status-bar-style="auto""#, "", "");
    choice(&p, "default", Some("header"), false);
    let before = resolutions(&p);
    let epoch = p.host.kernel().epoch();
    let preferences = exact_runner::Preferences {
        reduced_motion: true,
        ..p.host.runner().viewport().preferences
    };
    assert!(p.set_preferences(preferences).is_none());
    assert_eq!(p.host.kernel().epoch(), epoch);
    choice(&p, "default", Some("header"), false);
    assert_eq!(resolutions(&p), before + 1);
    let before = resolutions(&p);
    let route = id(&p, "route");
    p.set_view_scheme(route, true);
    assert_eq!(p.host.kernel().epoch(), epoch);
    choice(&p, "light-content", Some("header"), true);
    assert_eq!(resolutions(&p), before + 1);
    let before = resolutions(&p);
    p.set_view_scheme(route, true);
    choice(&p, "light-content", Some("header"), true);
    assert_eq!(resolutions(&p), before);
}

fn hidden(p: &Presenter<()>, bars: bool, from: Option<&str>) {
    let bar = p.status_bar();
    assert_eq!(bar.hidden, bars);
    assert_eq!(bar.hidden_source, from.map(|name| id(p, name)));
    assert_eq!(p.chrome().bars_hidden, bars);
}

/// `system-bars-hidden` resolves by the style's rule and apart from it: a
/// descendant's `false` over its ancestor's `true`, a later branch over an
/// earlier one, and a hidden declaration does not compete.
#[test]
fn hiding_resolves_as_the_style_does_and_apart_from_it() {
    let route = r#"status-bar-style="light-content" system-bars-hidden=true"#;
    let p = screen(route, "", "", "");
    hidden(&p, true, Some("route"));
    choice(&p, "light-content", Some("route"), true);
    let p = screen(route, "system-bars-hidden=false", "", "");
    hidden(&p, false, Some("header"));
    choice(&p, "light-content", Some("route"), true);
    let p = screen(
        route,
        "system-bars-hidden=false",
        "system-bars-hidden=true",
        "",
    );
    hidden(&p, true, Some("overlay"));
    let p = screen(route, "system-bars-hidden=false", "", r#"display="none""#);
    hidden(&p, true, Some("route"));
    let p = screen("", "", "", "");
    hidden(&p, false, None);
}

/// A full-screen toy over a stack hides the bars and going back brings them
/// back; a covering page's scope leaves out the stack's declaration, so a
/// root that hides them does not reach into a page that says nothing.
#[test]
fn a_covering_route_decides_the_bars_and_going_back_restores_them() {
    for presentation in ["push", "fullscreen", "modal"] {
        let mut p = routes(presentation, "system-bars-hidden=true");
        hidden(&p, false, None);
        let open = id(&p, "open");
        p.tap(open).unwrap();
        hidden(&p, true, Some("detail"));
        let layout: serde_json::Value = serde_json::from_str(&p.layout_json(None, false)).unwrap();
        assert_eq!(layout["statusBar"]["hidden"], true, "{presentation}");
        assert_eq!(layout["statusBar"]["hiddenSource"], id(&p, "detail"));
        assert!(p.system_back(0.));
        hidden(&p, false, None);
    }
    let p = boot(
        r#"    main navigationKey="cover" navigationBack="back" system-bars-hidden=true
      box navigationKey="home"
      box navigationKey="cover" navigationPresentation="fullscreen"
        text "Cover"
"#,
    );
    hidden(&p, false, None);
}

/// Bound to state, the bars follow it in the commit that changes it.
#[test]
fn bound_hiding_follows_state() {
    let mut p = source(
        r#"component App
  state playing = false
  action flip
    playing = not playing
  view
    main testId="toy" width="100%" height="100%" system-bars-hidden=playing
      button "Play" press=flip testId="flip"
"#,
    );
    hidden(&p, false, Some("toy"));
    let flip = id(&p, "flip");
    p.tap(flip).unwrap();
    hidden(&p, true, Some("toy"));
    p.tap(flip).unwrap();
    hidden(&p, false, Some("toy"));
}
