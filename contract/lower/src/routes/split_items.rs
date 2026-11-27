//! @ref LLP 1123 D15 — a Mac split item's controls, read on the element that projects the item: the
//! root tablist the sidebar, a route the page (its stack's first) or the detail (the pane's first),
//! and a route's own `aside` its inspector; and its accessories' own controls, read on the element
//! an accessory shows. Each is one AppKit property of the item or its accessory's view controller.

use super::NavPlace;
use crate::error::err;
use crate::LowerError;
use contract_syntax::{Attr, Expr};

/// What projects an item: a route, the root tablist, or a route's `aside`.
#[derive(Clone, Copy, PartialEq)]
enum Projects {
    Route,
    Tablist,
    Aside,
}

/// Each item control, the elements that read it, and the words it takes (none for an `id`).
const ITEM: [(&str, &[Projects], &[&str], &str); 4] = [
    ("navigationSplitItemAdjustsForOverlays", &[Projects::Route], &["true", "false"], "\"true\" (the page's or the detail's safe area accounts for the sidebars and inspectors overlaid on it) or \"false\" (the default)"),
    ("navigationSplitItemFullHeight", &[Projects::Tablist, Projects::Aside], &["true", "false"], "\"true\" (the default: under full-size content the sidebar or inspector runs the window's full height) or \"false\" (it starts below the title bar)"),
    ("navigationSplitItemTopAccessory", &[Projects::Route, Projects::Tablist, Projects::Aside], &[], ""),
    ("navigationSplitItemBottomAccessory", &[Projects::Route, Projects::Tablist, Projects::Aside], &[], ""),
];

/// Each accessory control and the words it takes.
const ACCESSORY: [(&str, &[&str], &str); 2] = [
    ("navigationAccessoryContentInsets", &["true", "false"], "\"true\" (the default: AppKit applies its standard content insets to the accessory) or \"false\" (it lays out edge to edge)"),
    ("navigationAccessoryScrollEdgeEffect", &["automatic", "soft", "hard"], "\"automatic\" (the default, the platform's), \"soft\" or \"hard\": the edge effect content scrolling behind the accessory takes"),
];

/// Refuses an item control on an element that projects no item that reads it, and a literal value
/// that is none of a control's words; `route`: whether the element is a route.
pub(crate) fn check(
    place: &NavPlace,
    tag: &str,
    attrs: &[Attr],
    route: bool,
) -> Result<(), LowerError> {
    let literal = |name: &str| match attrs
        .iter()
        .rev()
        .find(|a| a.name == name)
        .map(|a| &a.value)
    {
        Some(Expr::Str(s, _)) => Some(Some(s.as_str())),
        Some(_) => Some(None),
        None => None,
    };
    let in_root = !matches!(
        place,
        NavPlace::Outside | NavPlace::UnplacedRouteChild | NavPlace::UnplacedRouteRow
    );
    let in_route = matches!(
        place,
        NavPlace::RouteChild { .. } | NavPlace::RouteRow { .. } | NavPlace::InRoute
    );
    let projects = if route {
        Some(Projects::Route)
    } else if literal("role") == Some(Some("tablist")) && in_root && !in_route {
        Some(Projects::Tablist)
    } else if (tag == "aside" || literal("role") == Some(Some("complementary")))
        && matches!(place, NavPlace::RouteChild { .. })
    {
        Some(Projects::Aside)
    } else {
        None
    };
    for (name, readers, words, takes) in ITEM {
        let Some(a) = attrs.iter().rev().find(|a| a.name == name) else {
            continue;
        };
        if !projects.is_some_and(|p| readers.contains(&p)) {
            let on = match readers {
                [Projects::Route] => "a route (an element with a `navigationKey`), which projects the page's item or the detail's",
                [Projects::Tablist, Projects::Aside] => "the navigation root's tablist, which projects the sidebar, or a route's own `aside`, which projects its inspector",
                _ => "a route, the navigation root's tablist or a route's own `aside`, each of which projects a split item",
            };
            return err(
                "lower-attr-value",
                format!("`{name}` is read only on {on}; elsewhere no host reads it"),
                a.span,
            );
        }
        match &a.value {
            Expr::Str(s, _) if words.is_empty() && s.is_empty() => {
                return err(
                    "lower-attr-value",
                    format!(
                        "`{name}` takes the `id` of the element the item shows as an accessory"
                    ),
                    a.span,
                );
            }
            Expr::Str(s, _) if !words.is_empty() && !words.contains(&s.as_str()) => {
                return err(
                    "lower-attr-value",
                    format!("`{name}` takes {takes}"),
                    a.span,
                );
            }
            _ => {}
        }
    }
    for (name, words, takes) in ACCESSORY {
        if let Some(a) = attrs.iter().rev().find(|a| a.name == name) {
            if matches!(&a.value, Expr::Str(s, _) if !words.contains(&s.as_str())) {
                return err(
                    "lower-attr-value",
                    format!("`{name}` takes {takes}"),
                    a.span,
                );
            }
        }
    }
    Ok(())
}
