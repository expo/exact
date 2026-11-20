use super::*;

#[test]
fn a_plan_says_the_ground_its_first_screen_stands_on() {
    const SCREEN: &str = "routes nav
  home \"/\"
component App
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` navigationScroll=\"l\" ROUTE position=\"absolute\" inset=0
          header
            text \"Laps\" role=\"heading\" aria-level=1
          LIST
            section
              row
                text \"One\"
";
    const GROUPED: &str =
        "list id=\"l\" appearance=\"auto\" listStyle=\"inset-grouped\" flex=1 min-height=0";
    let launch = |route: &str, list: &str| {
        let plan = contract::compile(&SCREEN.replace("ROUTE", route).replace("LIST", list))
            .unwrap_or_else(|e| panic!("{e}"));
        artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-ios-sim").unwrap()
            ["launchBackground"]
            .clone()
    };
    assert_eq!(launch("", GROUPED), json!("grouped"));
    assert_eq!(
        launch("", &GROUPED.replace("inset-grouped", "grouped")),
        json!("grouped")
    );
    // Any other scroll, or a page of its own, is the platform's.
    assert_eq!(
        launch("", "scroll id=\"l\" flex=1 min-height=0"),
        json!("system")
    );
    assert_eq!(
        launch("background-color=\"-exact-background\"", GROUPED),
        json!("system")
    );
    // No route at all, as a plan with no navigation root is.
    let bare = contract::compile("component App\n  view\n    text \"a\" testId=\"a\"\n").unwrap();
    assert_eq!(
        artifact_graph(&bare, &json!({}), &[], false, "aarch64-apple-ios-sim").unwrap()
            ["launchBackground"],
        json!("system")
    );
    // LLP 1163 D2: the app's own ground, each route's colour, else the app
    // root's, as hex for both schemes or `light-dark()` of two.
    let ground = |source: &str| {
        let plan = contract::compile(source).unwrap_or_else(|e| panic!("{e}"));
        artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-ios-sim").unwrap()
            ["launchGround"]
            .clone()
    };
    let route = |written: &str| {
        ground(
            &SCREEN
                .replace("ROUTE", written)
                .replace("LIST", "scroll id=\"l\" flex=1 min-height=0"),
        )
    };
    assert_eq!(
        route("background-color=\"light-dark(#f5f6f8, #1c1814)\""),
        json!({"light": "#f5f6f8", "dark": "#1c1814"})
    );
    assert_eq!(
        route("background-color=\"#f5f6f8\""),
        json!({"light": "#f5f6f8", "dark": "#f5f6f8"})
    );
    // A colour as the kernel reads it: a name, `rgb()`, a theme's name; a
    // pinned scheme's side, the route's or the root's.
    assert_eq!(
        route("background-color=\"white\""),
        json!({"light": "#ffffff", "dark": "#ffffff"})
    );
    assert_eq!(
        route("background-color=\"rgb(10 20 30)\""),
        json!({"light": "#0a141e", "dark": "#0a141e"})
    );
    assert_eq!(
        route("background-color=\"light-dark(#f5f6f8, #1c1814)\" color-scheme=\"dark\""),
        json!({"light": "#1c1814", "dark": "#1c1814"})
    );
    assert_eq!(
        ground(
            &SCREEN
                .replace(
                    "navigationBack=\"back\"",
                    "navigationBack=\"back\" color-scheme=\"light\""
                )
                .replace("ROUTE", "background-color=\"light-dark(#f5f6f8, #1c1814)\"")
                .replace("LIST", "scroll id=\"l\" flex=1 min-height=0")
        ),
        json!({"light": "#f5f6f8", "dark": "#f5f6f8"})
    );
    let themed = SCREEN
        .replace(
            "component App",
            "theme\n  ground=\"light-dark(#fafafa, #101010)\"\ncomponent App",
        )
        .replace("ROUTE", "background-color=\"ground\"")
        .replace("LIST", "scroll id=\"l\" flex=1 min-height=0");
    assert_eq!(
        ground(&themed),
        json!({"light": "#fafafa", "dark": "#101010"})
    );
    // A wide colour as the host draws it: within sRGB as hex, beyond it in
    // Display P3 (one beyond Display P3 is below).
    assert_eq!(
        route("background-color=\"oklch(1 0 0)\""),
        json!({"light": "#ffffff", "dark": "#ffffff"})
    );
    assert_eq!(
        route("background-color=\"light-dark(color(display-p3 1 0 0), #000000)\""),
        json!({"light": "color(display-p3 1.0000 0.0000 0.0000)", "dark": "#000000"})
    );
    // Whatever the first frame may show in another colour leaves the launch
    // screen to the platform: a role (the page's, the tint's, any other the
    // OS draws), a platform colour or one beyond Display P3, a translucent, computed or
    // pictured ground, a computed scheme, and none.
    for written in [
        "background-color=\"-exact-background\"",
        "background-color=\"-exact-grouped-background\"",
        "background-color=\"-exact-secondary-background\"",
        "background-color=\"AccentColor\"",
        "background-color=\"-exact-platform-color(ios systemPurpleColor, #800080)\"",
        "background-color=\"color(rec2020 0 1 0)\"",
        "background-color=\"#ffffff80\"",
        "background-color=(e.id == 0 ? \"#ffffff\" : \"#000000\")",
        "background-color=\"#ffffff\" background-image=\"linear-gradient(#ff0000, #0000ff)\"",
        "background-color=\"#ffffff\" color-scheme=(e.id == 0 ? \"dark\" : \"light\")",
        "",
    ] {
        assert_eq!(route(written), json!(null), "{written}");
    }
    // A route in a component of its own stands on the root the app starts from.
    const ROOTED: &str = "routes nav
  home \"/\"
component App
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\" background-color=\"light-dark(#ffffff, #000000)\"
      for e in stack(nav) key=e.id
        Page(id=`${e.id}`)
component Page
  props
    id: string
  view
    column navigationKey=id position=\"absolute\" inset=0
      text \"Hi\"
";
    assert_eq!(
        ground(ROOTED),
        json!({"light": "#ffffff", "dark": "#000000"})
    );
    // A sheet or a full-screen cover is shown over another route, never first.
    const COVERED: &str = "routes nav
  home \"/\"
  camera \"/camera\"
component App
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\"
      for e in stack(nav) key=e.id
        if e.name == \"camera\"
          column navigationKey=`${e.id}` navigationPresentation=\"fullscreen\" background-color=\"#000000\"
            text \"Camera\"
        else
          column navigationKey=`${e.id}` background-color=\"#f5f6f8\"
            text \"Home\"
";
    assert_eq!(
        ground(COVERED),
        json!({"light": "#f5f6f8", "dark": "#f5f6f8"})
    );
    // A route in a tab panel may stand on the panel or a box above it.
    assert_eq!(
        ground(&COVERED.replace(
            "column navigationKey=`${e.id}` background-color=\"#f5f6f8\"",
            "column navigationKey=`${e.id}` background-color=\"#f5f6f8\" role=\"tabpanel\""
        )),
        json!(null)
    );
    // A theme's colour that varies by an app mode may be another mode's at
    // the first frame; one that does not is read.
    let dir = crate::TempDir(
        std::env::temp_dir().join(format!("exact-launch-ground-modes-{}", std::process::id())),
    );
    std::fs::create_dir_all(&*dir).unwrap();
    std::fs::write(
        dir.join("app.forma"),
        "forma probe\n\naxes\n  dark-style = black | dim  under scheme=dark\n\ntheme\n  surface.page = light-dark(#ffffff, #000000)\n    when dark-style=dim: light-dark(#ffffff, #15202b)\n  surface.plain = light-dark(#fafafa, #101010)\n",
    )
    .unwrap();
    let moded = |token: &str| {
        std::fs::write(
            dir.join("app.contract"),
            format!("component App\n  view\n    column background-color=\"{token}\"\n      text \"Hi\"\n"),
        )
        .unwrap();
        let plan = crate::colors::compiled(&dir).unwrap().unwrap();
        artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-ios-sim").unwrap()
            ["launchGround"]
            .clone()
    };
    assert_eq!(moded("surface.page"), json!(null));
    assert_eq!(
        moded("surface.plain"),
        json!({"light": "#fafafa", "dark": "#101010"})
    );
    // Routes that stand on different grounds leave it to the platform: the
    // plan does not say which one the app opens on.
    assert_eq!(
        ground(&COVERED.replace("navigationPresentation=\"fullscreen\" ", "")),
        json!(null)
    );
}

#[test]
fn a_ground_something_may_paint_over_is_left_to_the_platform() {
    let graph = |source: &str| {
        let plan = contract::compile(source).unwrap_or_else(|e| panic!("{e}\n{source}"));
        artifact_graph(&plan, &json!({}), &[], false, "aarch64-apple-ios-sim").unwrap()
    };
    // An app root of white with `root` written on it and `child` (lines
    // indented as its children) in it
    let app = |root: &str, child: &str| {
        graph(&format!(
            "keyframes glow\n  from background-color=\"#000000\"\n  to background-color=\"#ffffff\"\ncomponent App\n  state big = true\n  view\n    column width=\"100%\" height=\"100%\" background-color=\"#ffffff\" {root}\n      {child}\n      text \"Hi\"\n"
        ))["launchGround"]
            .clone()
    };
    const BLACK: &str = "background-color=\"#000000\"";
    // A node that may fill the screen with a colour or pixels of its own,
    // through every box between it and the screen, or an effect on the ground
    for (root, child) in [
        ("", format!("column position=\"absolute\" inset=0 {BLACK}")),
        ("", format!("column position=\"absolute\" top=0 bottom=0 left=0 width=\"100%\" {BLACK}")),
        ("", format!("column position=\"relative\" flex=1\n        column position=\"absolute\" inset=0 {BLACK}")),
        ("", format!("column flex=1 {BLACK}")),
        ("", format!("column flex=1\n        column flex=1 {BLACK}")),
        ("", "column flex=1\n        list appearance=\"auto\" listStyle=\"inset-grouped\" flex=1 min-height=0\n          section\n            row\n              text \"One\"".into()),
        ("", format!("column height=\"100%\" {BLACK}")),
        ("", format!("column min-height=\"100vh\" {BLACK}")),
        ("", format!("column flex-basis=\"100%\" {BLACK}")),
        ("", format!("scroll flex=1\n        column min-height=\"100%\" {BLACK}")),
        ("", format!("column width=(big ? \"100%\" : \"50%\") height=(big ? \"100%\" : \"50%\") {BLACK}")),
        ("flex-direction=\"row\"", format!("column width=\"100%\" {BLACK}")),
        ("", "canvas width=\"100%\" height=\"100%\"".into()),
        ("display=\"grid\"", format!("column {BLACK}")),
        ("", format!("column height=\"calc(100vh - 44px)\" {BLACK}")),
        ("", format!("column height=0 flex-grow=1 {BLACK}")),
        ("", format!("scroll flex=1\n        for i in [1, 2, 3] key=i\n          row height=44 {BLACK}")),
        ("", "column flex=1 background-color=\"#ffffff\" filter=\"invert(1)\"".into()),
        ("padding-left=16", format!("column flex=1 {BLACK}")),
        ("padding=16", format!("column flex=1 margin=-16 {BLACK}")),
        ("filter=\"invert(1)\"", "text \"a\"".into()),
        ("opacity=0.5", "text \"a\"".into()),
        ("visibility=\"hidden\"", "text \"a\"".into()),
        ("display=\"none\"", "text \"a\"".into()),
        ("animation=\"glow 5s linear both\"", "text \"a\"".into()),
        ("mask-image=\"linear-gradient(#000000, transparent)\"", "text \"a\"".into()),
        ("clip-path=\"circle(10px)\"", "text \"a\"".into()),
    ] {
        assert_eq!(app(root, &child), json!(null), "{root} {child}");
    }
    // One that stops short of the screen's edges, or paints the ground's own
    // colour, leaves the ground: a padded parent, a size in points, a box that
    // takes its content's height, one centred, one on two edges
    let white = json!({"light": "#ffffff", "dark": "#ffffff"});
    for (root, child) in [
        ("", format!("column position=\"relative\" height=100\n        column position=\"absolute\" inset=0 {BLACK}")),
        ("padding=16", format!("column flex=1 {BLACK}")),
        ("padding=16", format!("column flex=1 margin=8 {BLACK}")),
        ("padding=\"1rem\"", format!("column flex=1 {BLACK}")),
        ("", format!("scroll flex=1\n        svg width=\"100%\" height=220")),
        ("", format!("scroll flex=1\n        column margin=16 {BLACK}")),
        ("", format!("column width=\"100%\" height=240 {BLACK}")),
        ("", format!("row padding=8 {BLACK}\n        text \"Tab\"")),
        ("", format!("button width=\"100%\" {BLACK}\n        text \"Go\"")),
        ("align-items=\"center\"", format!("column flex=1 {BLACK}")),
        ("flex-direction=\"row\"", format!("column width=280 {BLACK}")),
        ("", format!("column position=\"absolute\" left=16 right=16 bottom=24 {BLACK}")),
        ("", format!("column position=\"absolute\" right=16 bottom=16 width=56 height=56 {BLACK}")),
        ("", format!("column position=\"absolute\" left=0 top=0 bottom=0 width=56 {BLACK}")),
        ("", "column flex=1 background-color=\"#ffffff\"".into()),
        ("", "column flex=1 background-color=\"transparent\"".into()),
        ("opacity=1", "text \"a\"".into()),
    ] {
        assert_eq!(app(root, &child), white, "{root} {child}");
    }
    // A colour written as the ground's under another scheme is not the ground
    assert_eq!(
        graph("component App\n  view\n    column width=\"100%\" height=\"100%\" background-color=\"light-dark(#ffffff, #000000)\"\n      column flex=1 background-color=\"light-dark(#ffffff, #000000)\" color-scheme=\"dark\"\n        text \"Hi\"\n")
            ["launchGround"],
        json!(null)
    );
    // Routes: a presentation of "" or a computed one is a page on its own
    // ground; an effect on the navigation root or a box outside every route
    // reaches every screen; a box of another route, a sheet's, does not
    const STACK: &str = "routes nav
  home \"/\"
  camera \"/camera\"
component App
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\" background-color=\"#ffffff\" ROOT
      for e in stack(nav) key=e.id
        if e.name == \"camera\"
          column navigationKey=`${e.id}` navigationPresentation=\"fullscreen\" position=\"absolute\" inset=0
            column position=\"absolute\" inset=0 background-color=\"#ff0000\"
        else
          column navigationKey=`${e.id}` PRESENTATION position=\"absolute\" inset=0 ROUTE
            text \"Home\"
      SHARED
";
    let stack = |root: &str, presentation: &str, route: &str, shared: &str| {
        graph(
            &STACK
                .replace("ROOT", root)
                .replace("PRESENTATION", presentation)
                .replace("ROUTE", route)
                .replace("SHARED", shared),
        )["launchGround"]
            .clone()
    };
    let black = json!({"light": "#000000", "dark": "#000000"});
    assert_eq!(stack("", "", BLACK, ""), black);
    assert_eq!(stack("", "navigationPresentation=\"\"", BLACK, ""), black);
    assert_eq!(
        stack(
            "",
            "navigationPresentation=(e.id == 0 ? \"\" : \"modal\")",
            BLACK,
            ""
        ),
        black
    );
    assert_eq!(stack("filter=\"invert(1)\"", "", BLACK, ""), json!(null));
    assert_eq!(
        stack(
            "",
            "",
            BLACK,
            "column position=\"absolute\" inset=0 background-color=\"#ff0000\""
        ),
        json!(null)
    );
    assert_eq!(
        stack("", "", BLACK, "row position=\"absolute\" left=0 right=0 bottom=0 height=49 background-color=\"#ff0000\""),
        black
    );
    // A route's scheme wins over the root's; a pictured root under a route of
    // no colour of its own is no ground
    assert_eq!(
        stack(
            "color-scheme=\"light\"",
            "",
            "background-color=\"light-dark(#ffffff, #000000)\" color-scheme=\"dark\"",
            ""
        ),
        black
    );
    assert_eq!(stack("", "", "", ""), white);
    assert_eq!(
        stack(
            "background-image=\"linear-gradient(#000000, #ffffff)\"",
            "",
            "",
            ""
        ),
        json!(null)
    );
    // The grouped grey needs every route to scroll a grouped list
    let grouped = |settings: &str| {
        graph(&format!(
            "routes nav
  home \"/\"
  settings \"/settings\"
component App
  view
    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\"
      for e in stack(nav) key=e.id
        if e.name == \"settings\"
          column navigationKey=`${{e.id}}` navigationScroll=\"s\" position=\"absolute\" inset=0
            list id=\"s\" appearance=\"auto\" listStyle=\"inset-grouped\" flex=1 min-height=0
              section
                row
                  text \"One\"
        else
          column navigationKey=`${{e.id}}` navigationScroll=\"h\" position=\"absolute\" inset=0
            {settings}
"
        ))["launchBackground"]
            .clone()
    };
    assert_eq!(
        grouped("list id=\"h\" appearance=\"auto\" listStyle=\"inset-grouped\" flex=1 min-height=0\n              section\n                row\n                  text \"Two\""),
        json!("grouped")
    );
    assert_eq!(
        grouped("scroll id=\"h\" flex=1 min-height=0\n              text \"Home\""),
        json!("system")
    );
}
