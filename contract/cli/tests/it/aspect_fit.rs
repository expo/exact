//! LLP 1123 D16: `-exact-safe-area-aspect-fit: none | <ratio>` is the window guide's, admitted on the
//! app's first root under `viewport-fit="cover"`, where the root is no navigation root.

fn app(root: &str, child: &str) -> String {
    format!("component App\n  state fit = \"4 / 3\"\n  view\n    column {root} width=\"100%\" height=\"100%\"\n      column {child}\n        text \"Media\"\n")
}

#[test]
fn the_first_cover_root_takes_none_or_a_positive_ratio() {
    for value in ["\"none\"", "\"16 / 9\"", "\"2\"", "\"0.5\"", "(fit)"] {
        let src = app(
            &format!("viewport-fit=\"cover\" -exact-safe-area-aspect-fit={value}"),
            "",
        );
        contract::compile(&src).unwrap_or_else(|e| panic!("{value}: {e}"));
    }
    for value in ["auto", "0", "1 / 0", "-2", "16:9", "auto 16 / 9"] {
        let src = app(
            &format!("viewport-fit=\"cover\" -exact-safe-area-aspect-fit=\"{value}\""),
            "",
        );
        let error = contract::compile(&src).unwrap_err().to_string();
        assert!(
            error.contains("expected none or a positive ratio"),
            "{value}: {error}"
        );
    }
}

#[test]
fn aspect_fit_elsewhere_or_without_cover_or_on_a_navigation_root_is_refused() {
    let refused = |src: &str, says: &str| {
        let error = contract::compile(src).unwrap_err().to_string();
        assert!(error.contains(says), "{error}");
    };
    refused(
        &app(
            "viewport-fit=\"cover\"",
            "-exact-safe-area-aspect-fit=\"16 / 9\"",
        ),
        "is read only on the app's first root",
    );
    refused(
        &app("-exact-safe-area-aspect-fit=\"16 / 9\"", ""),
        "needs `viewport-fit=\"cover\"`",
    );
    refused(
        &app(
            "viewport-fit=\"auto\" -exact-safe-area-aspect-fit=\"16 / 9\"",
            "",
        ),
        "needs `viewport-fit=\"cover\"`",
    );
    let routed = "routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${top(nav).id}` navigationBack=\"back\" viewport-fit=\"cover\" -exact-safe-area-aspect-fit=\"16 / 9\" width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}`\n          text \"Home\"\n";
    refused(routed, "is on a navigation root");
}
