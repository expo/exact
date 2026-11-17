//! LLP 1047.002 D2.1 — the navigation behaviours a page leaves out: each one
//! the plan cannot make what it acts on, a bound value counting as every word
//! it could take.

use crate::emit::NAVIGATION as ALL;

const NONE: [&str; 0] = [];

/// The behaviours left out of a page whose routes say `route` and hold `body`. It declares
/// its not-found route, whose page the compiler otherwise writes with a header of its own.
fn unused(route: &str, body: &str) -> Vec<&'static str> {
    let source = format!("routes nav\n  home \"/\"\n  notfound\ncomponent App\n  state on = true\n  view\n    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` {route}\n{body}");
    crate::emit::emit(&contract::compile(&source).unwrap(), false, false)
        .unwrap()
        .navigation
}

fn used(route: &str, body: &str) -> Vec<&'static str> {
    let left = unused(route, body);
    ALL.into_iter().filter(|b| !left.contains(b)).collect()
}

const TEXT: &str = "          text \"Card\"\n";
const HEADER: &str = "          header\n            text \"Card\" role=\"heading\"\n";

#[test]
fn a_route_with_nothing_navigation_draws_leaves_every_behaviour_out() {
    assert_eq!(unused("", TEXT), ALL);
}

#[test]
fn a_presented_route_links_sheets() {
    for route in [
        "navigationPresentation=\"modal\"",
        "navigationPresentation=\"fullscreen\"",
        "navigationPresentation=(on ? \"modal\" : \"default\")",
    ] {
        assert_eq!(used(route, TEXT), ["SHEETS"], "{route}");
    }
    assert_eq!(used("navigationPresentation=\"default\"", TEXT), NONE);
}

#[test]
fn a_tablist_and_a_list_or_split_link_theirs() {
    assert_eq!(used("", "          row role=\"tablist\"\n"), ["TABS"]);
    assert_eq!(
        used("", "          row role=(on ? \"tablist\" : \"group\")\n"),
        ["TABS"]
    );
    assert_eq!(used("", "          row role=\"group\"\n"), NONE);
    assert_eq!(
        used("", "          list\n            text \"Row\"\n"),
        ["SPLITS"]
    );
    assert_eq!(used("navigationSplit=\"280\"", TEXT), ["SPLITS"]);
}

/// A header is a route's (routes.js `headerOf`) where its element is the
/// route's child: under a region too, which writes none, but not under another
/// node, as a site's header in a page's column is.
#[test]
fn only_a_routes_own_header_links_the_app_bar() {
    assert_eq!(used("", HEADER), ["BARS"]);
    let under = |parent: &str| {
        format!("          {parent}\n            header\n              text \"Card\" role=\"heading\"\n")
    };
    assert_eq!(used("", &under("if on")), ["BARS"]);
    assert_eq!(used("", &under("column")), NONE);
    assert_eq!(
        used("navigationBar=\"transparent\"", HEADER),
        ["BARS", "CLEAR_BARS"]
    );
    assert_eq!(
        used("navigationBar=(on ? \"transparent\" : \"auto\")", HEADER),
        ["BARS", "CLEAR_BARS"]
    );
    assert_eq!(used("navigationBar=\"transparent\"", TEXT), NONE);
}

/// Without a navigation root nothing is projected, so a tablist or a list
/// links nothing.
#[test]
fn a_plan_with_no_navigation_root_leaves_every_behaviour_out() {
    let plan = contract::compile("component App\n  view\n    column\n      row role=\"tablist\"\n      list\n        text \"Row\"\n").unwrap();
    assert_eq!(
        crate::emit::emit(&plan, false, false).unwrap().navigation,
        ALL
    );
}
