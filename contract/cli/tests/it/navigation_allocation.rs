//! LLP 1123 D4: a route's controller allocation and its vertical bar are attributes only a route
//! reads, each taking its words, `navigationExtendedLayout` the edges each once besides.

fn route(attrs: &str, inner: &str) -> String {
    format!(
        "routes nav\n  home \"/\"\n    menu \"/menu\"\ncomponent App\n  state edges: string = \"top\"\n  view\n    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` {attrs}\n          column {inner}\n"
    )
}

#[test]
fn a_routes_allocation_and_vertical_bar_lower() {
    for attrs in [
        "navigationExtendedLayout=\"all\"",
        "navigationExtendedLayout=\"none\"",
        "navigationExtendedLayout=\"top\"",
        "navigationExtendedLayout=\"bottom top left right\"",
        "navigationExtendedLayout=(edges)",
        "navigationExtendsUnderOpaqueBars=\"true\" navigationExtendedLayout=\"top bottom\"",
        "navigationVerticalBar=\"disabled\" navigationVerticalBarCompression=\"prefers-tab-bar\"",
        "navigationVerticalBar=\"automatic\" navigationVerticalBarCompression=\"prefers-bar-items\"",
    ] {
        contract::compile(&route(attrs, "")).unwrap_or_else(|e| panic!("{attrs:?}: {e}"));
    }
}

#[test]
fn off_a_route_or_with_another_word_they_are_refused() {
    for (attrs, inner, says) in [
        (
            "",
            "navigationVerticalBar=\"disabled\"",
            "read only on a route",
        ),
        (
            "",
            "navigationExtendedLayout=\"none\"",
            "read only on a route",
        ),
        ("navigationExtendedLayout=\"all top\"", "", "each once"),
        ("navigationExtendedLayout=\"top top\"", "", "each once"),
        ("navigationExtendedLayout=\"\"", "", "each once"),
        ("navigationExtendedLayout=\"leading\"", "", "each once"),
        (
            "navigationExtendsUnderOpaqueBars=\"yes\"",
            "",
            "opaque bars too",
        ),
        ("navigationVerticalBar=\"never\"", "", "\"disabled\""),
        (
            "navigationVerticalBarCompression=\"tab-bar\"",
            "",
            "\"prefers-tab-bar\"",
        ),
    ] {
        let error = contract::compile(&route(attrs, inner))
            .expect_err(&format!("{attrs:?} {inner:?}"))
            .to_string();
        assert!(error.contains(says), "{attrs:?} {inner:?}: {error}");
    }
}
