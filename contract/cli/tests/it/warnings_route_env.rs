//! The window's `env()` written in a route, which reads its own values
//! (LLP 1123 D9).

use super::warnings::warned;

#[test]
fn the_windows_env_in_a_route_is_named_with_the_routes_var() {
    // LLP 1123 D9: under a bar, `env(safe-area-inset-top)` clears the
    // status bar a second time; a route reads its own values. A dialog is
    // the window's.
    let src = "routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${top(nav).id}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}`\n          text \"x\" padding-top=\"calc(env(safe-area-inset-top) + 8px)\"\n          text \"y\" width=\"env(viewport-segment-width 0 0)\" padding-left=\"var(--exact-safe-area-inset-left)\"\n          dialog\n            text \"z\" padding-top=\"env(safe-area-inset-top)\"\n      text \"outside\" padding-top=\"env(safe-area-inset-top)\"\n";
    let mut owner: Vec<u32> = warned(src)
        .into_iter()
        .filter(|(id, _)| id == "lower-env-in-owner")
        .map(|(_, line)| line)
        .collect();
    owner.sort();
    assert_eq!(owner, [8, 9], "{src}");
    let (_, warnings) = contract::compile_warned(src);
    let first = warnings.iter().find(|w| w.span.line == 8).unwrap();
    assert!(
        first
            .message
            .contains("`padding-top=\"calc(var(--exact-safe-area-inset-top) + 8px)\"`"),
        "{first:?}"
    );
}

#[test]
fn the_windows_env_with_a_fallback_in_a_route_is_named_with_the_routes_var() {
    let route = |value: &str| {
        format!("routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}`\n          text \"x\" padding-top=\"{value}\"\n")
    };
    let src = route("env(safe-area-inset-top, 0px)");
    let (_, warnings) = contract::compile_warned(&src);
    let named = warnings
        .iter()
        .find(|w| w.id == "lower-env-in-owner")
        .unwrap_or_else(|| panic!("{warnings:?}"));
    assert!(
        named
            .message
            .contains("`padding-top=\"var(--exact-safe-area-inset-top, 0px)\"`"),
        "{named:?}"
    );
    // What it names compiles, and is the route's.
    let owned = warned(&route("var(--exact-safe-area-inset-top, 0px)"));
    assert!(
        owned.iter().all(|(id, _)| id != "lower-env-in-owner"),
        "{owned:?}"
    );
}
