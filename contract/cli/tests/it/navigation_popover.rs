//! LLP 1123 D7: a route presented as a popover is anchored to the element its
//! `navigationPopoverAnchor` names, and its margins, arrows, source overlap and adaptation are
//! attributes only such a route reads.

fn route(attrs: &str, inner: &str) -> String {
    format!(
        "routes nav\n  home \"/\"\n    menu \"/menu\"\ncomponent App\n  state kind: string = \"popover\"\n  view\n    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` {attrs}\n          column {inner}\n            button id=\"open\"\n              text \"Open\"\n"
    )
}

#[test]
fn a_popover_route_lowers_its_anchor_and_its_choices() {
    for attrs in [
        "navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\"",
        "navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverMargins=\"16\" navigationPopoverArrows=\"any\"",
        "navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverMargins=\"16 8 24 8\" navigationPopoverArrows=\"up down\"",
        "navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverOverlapsSource=\"true\" navigationPopoverAdaptation=\"none\"",
        // A computed presentation may be a popover, and its anchor is read when it is
        "navigationPresentation=(kind) navigationPopoverAnchor=\"open\" navigationPopoverAdaptation=\"sheet\"",
        "navigationPresentation=\"modal\"",
        "navigationPresentation=\"fullscreen\"",
        "navigationPresentation=\"\"",
    ] {
        contract::compile(&route(attrs, "")).unwrap_or_else(|e| panic!("{attrs:?}: {e}"));
    }
}

#[test]
fn a_popover_attribute_off_a_popover_route_and_a_malformed_one_are_refused() {
    for (attrs, inner, says) in [
        ("navigationPresentation=\"popover\"", "", "write `navigationPopoverAnchor`"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"\"", "", "write `navigationPopoverAnchor`"),
        ("navigationPresentation=\"modal\" navigationPopoverArrows=\"up\"", "", "`navigationPopoverArrows` is read only on a route presented as a popover"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\"", "navigationPopoverAdaptation=\"none\"", "`navigationPopoverAdaptation` is read only on a route presented as a popover"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverMargins=\"1 2 3 4 5\"", "", "`navigationPopoverMargins` takes"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverMargins=\"-4\"", "", "`navigationPopoverMargins` takes"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverArrows=\"up up\"", "", "`navigationPopoverArrows` takes"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverArrows=\"any up\"", "", "`navigationPopoverArrows` takes"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverOverlapsSource=\"yes\"", "", "`navigationPopoverOverlapsSource` takes"),
        ("navigationPresentation=\"popover\" navigationPopoverAnchor=\"open\" navigationPopoverAdaptation=\"fullscreen\"", "", "`navigationPopoverAdaptation` takes"),
    ] {
        let error = contract::compile(&route(attrs, inner)).unwrap_err().to_string();
        assert!(error.contains(says), "{attrs:?} {inner:?}: {error}");
    }
}
