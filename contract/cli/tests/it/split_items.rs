//! LLP 1123 D15: a Mac split item's controls are read on the element that projects the item, the
//! root tablist, a route or a route's own `aside`, and its accessories' on the element shown.

/// A root with tabs whose one route has an inspector `aside`: `tablist`, `route` and `aside` are
/// each element's further attributes, and `other` an element's in the route's content.
fn app(tablist: &str, route: &str, aside: &str, other: &str) -> String {
    format!(
        "routes nav\n  tab home \"/\"\ncomponent App\n  action pick(name: string)\n    nav = select(nav, name)\n  view\n    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\" display=\"flex\" flex-direction=\"column\"\n      column flex=1\n        for t in nav.tabs key=t.name\n          column role=\"tabpanel\" id=`panel-${{t.name}}`\n            for e in t.stack key=e.id\n              row navigationKey=`${{e.id}}` {route}\n                column flex=1 {other}\n                  text \"Home\"\n                aside width=200 {aside}\n                  text \"Inspector\"\n      row role=\"tablist\" {tablist}\n        button role=\"tab\" aria-controls=\"panel-home\" aria-selected=true press=pick(\"home\")\n          text \"Home\"\n      column id=\"tools\" navigationAccessoryContentInsets=\"false\" navigationAccessoryScrollEdgeEffect=\"soft\"\n        text \"Tools\"\n"
    )
}

#[test]
fn split_item_controls_are_read_where_an_item_is_projected() {
    for (tablist, route, aside) in [
        ("navigationSplitItemFullHeight=\"false\" navigationSplitItemBottomAccessory=\"tools\"", "", ""),
        ("", "navigationSplitItemAdjustsForOverlays=\"true\" navigationSplitItemTopAccessory=\"tools\"", ""),
        ("", "", "navigationSplitItemFullHeight=\"true\" navigationSplitItemTopAccessory=\"tools\""),
    ] {
        let source = app(tablist, route, aside, "");
        contract::compile(&source).unwrap_or_else(|e| panic!("{tablist:?} {route:?} {aside:?}: {e}"));
    }
    for (tablist, route, aside, other, says) in [
        ("navigationSplitItemAdjustsForOverlays=\"true\"", "", "", "", "`navigationSplitItemAdjustsForOverlays` is read only on a route"),
        ("", "navigationSplitItemFullHeight=\"true\"", "", "", "`navigationSplitItemFullHeight` is read only on the navigation root's tablist"),
        ("", "", "", "navigationSplitItemTopAccessory=\"tools\"", "`navigationSplitItemTopAccessory` is read only on a route, the navigation root's tablist or a route's own `aside`"),
        ("navigationSplitItemFullHeight=\"no\"", "", "", "", "`navigationSplitItemFullHeight` takes"),
        ("", "navigationSplitItemBottomAccessory=\"\"", "", "", "takes the `id` of the element"),
        ("", "", "", "navigationAccessoryScrollEdgeEffect=\"blurred\"", "`navigationAccessoryScrollEdgeEffect` takes"),
    ] {
        let error = contract::compile(&app(tablist, route, aside, other)).unwrap_err().to_string();
        assert!(error.contains(says), "{tablist:?} {route:?} {aside:?} {other:?}: {error}");
    }
}
