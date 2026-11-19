use super::warnings::warned;

fn route(row: &str) -> String {
    format!("routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}`\n          text \"x\" {row}\n          dialog\n            text \"z\" width=\"env(viewport-segment-width 0 0)\"\n      text \"outside\" width=\"env(viewport-segment-width 0 0)\"\n")
}

fn owner_warnings(src: &str) -> Vec<(String, u32)> {
    let mut out: Vec<_> = (warned(src).into_iter())
        .filter(|(id, _)| id == "lower-env-in-owner")
        .collect();
    out.sort();
    out
}

#[test]
fn the_windows_segment_in_a_route_is_named_with_the_routes_var() {
    // A dialog and a box outside the route are the window's
    let src = route("width=\"calc(env(viewport-segment-width 0 0, 50%) + 2px)\"");
    assert_eq!(owner_warnings(&src).len(), 1, "{src}");
    let (_, warnings) = contract::compile_warned(&src);
    let named = warnings
        .iter()
        .find(|w| w.id == "lower-env-in-owner")
        .unwrap_or_else(|| panic!("{warnings:?}"));
    assert!(
        named
            .message
            .contains("`width=\"calc(var(--exact-viewport-segment-width-0-0, 50%) + 2px)\"`"),
        "{named:?}"
    );
    let owned = route("width=\"calc(var(--exact-viewport-segment-width-0-0, 50%) + 2px)\"");
    let owned = owner_warnings(&owned);
    assert!(
        owned.is_empty(),
        "what it names compiles, and is the route's: {owned:?}"
    );
}

#[test]
fn the_safe_area_inset_in_a_route_is_the_routes() {
    let src = route("padding-top=\"max(16px, env(safe-area-inset-top))\"");
    assert!(owner_warnings(&src).is_empty(), "{src}");
}
