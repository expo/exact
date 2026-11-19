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
    // Whatever the first frame may show in another colour leaves the launch
    // screen to the platform: a role (the page's, the tint's, any other the
    // OS draws), a platform or wide-gamut colour, a translucent, computed or
    // pictured ground, a computed scheme, and none.
    for written in [
        "background-color=\"-exact-background\"",
        "background-color=\"-exact-grouped-background\"",
        "background-color=\"-exact-secondary-background\"",
        "background-color=\"AccentColor\"",
        "background-color=\"-exact-platform-color(ios systemPurpleColor, #800080)\"",
        "background-color=\"color(display-p3 1 0 0)\"",
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
