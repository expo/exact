//! LLP 1123 C11b (AN1). Follow a declared visit through the linked tab panels.
//! An unknown key, branch or presentation leaves the route undiagnosed.
use super::*;
use contract_checking::{Site, Visit};
use contract_syntax::{BinOp, RoutesDecl, UnOp};
use std::collections::BTreeMap;

fn attr<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a Expr> {
    attrs
        .iter()
        .rev()
        .find(|a| a.name == name)
        .map(|a| &a.value)
}

fn property(a: &Attr) -> &str {
    contract_syntax::state::state_of(&a.name).map_or(&a.name, |(_, row)| row)
}

pub(super) fn visible(attrs: &[Attr]) -> bool {
    attrs.iter().all(|a| match property(a) {
        "class" | "popover" | "hidden" | "animation" | "animation-name" => false,
        "visibility" => matches!(&a.value, Expr::Str(s, _) if s == "visible"),
        "display" => matches!(&a.value, Expr::Str(s, _) if matches!(s.as_str(),
            "block" | "flex" | "grid" | "inline" | "inline-flex")),
        _ => true,
    })
}

fn member(expr: &Expr, owner: &str, field: &str) -> bool {
    matches!(expr, Expr::Member(base, f, _) if f == field && matches!(&**base, Expr::Ident(n, _) if n == owner))
}

/// Keep the literal text on both sides of the varying id so equal templates
/// link each tab to its own panel and each visit to its own column.
fn key(expr: &Expr, varies: impl Fn(&Expr) -> bool) -> Option<(&str, &str)> {
    if varies(expr) {
        return Some(("", ""));
    }
    let Expr::Template(parts, _) = expr else {
        return None;
    };
    let (prefix, rest) = match parts.as_slice() {
        [TemplatePart::Text(s), rest @ ..] => (s.as_str(), rest),
        rest => ("", rest),
    };
    match rest {
        [TemplatePart::Expr(e)] if varies(e) => Some((prefix, "")),
        [TemplatePart::Expr(e), TemplatePart::Text(s)] if varies(e) => Some((prefix, s)),
        _ => None,
    }
}

fn only_child(node: Visit<'_>) -> Option<Visit<'_>> {
    // The expanded panel wraps its contents in a literal-true lazy region.
    let mut children = node.regions()[0]
        .walk_regions_with((), |n, _| match n.condition() {
            Some(Expr::Bool(true, _)) => [Some(()), None],
            Some(Expr::Bool(false, _)) => [None, Some(())],
            _ => [None, None],
        })
        .map(|(n, ())| n)
        .filter(|n| !matches!(n.condition(), Some(Expr::Bool(..))));
    let first = children.next()?;
    children.next().is_none().then_some(first)
}

fn each<'a>(node: Visit<'a>, owner: &str, field: &str, by: &str) -> Option<&'a str> {
    let (_, var, None, list, key) = node.repetition()? else {
        return None;
    };
    (member(list, owner, field) && member(key, var, by)).then_some(var)
}

fn plain_box(node: Visit<'_>) -> bool {
    node.element().is_some_and(|(tag, _, attrs)| {
        matches!(tag, "column" | "row" | "view") && attr(attrs, "role").is_none()
    })
}

fn control_key<'a>(list: Visit<'a>, router: &str) -> Option<(&'a str, &'a str)> {
    let ("row", _, attrs) = list.element()? else {
        return None;
    };
    // The suggested expression names the app's router at the authored list.
    if list.instance() != Some(0)
        || literal(attrs, "role") != Some("tablist")
        || attrs.iter().any(|a| match property(a) {
            "display" => true,
            "appearance" => !matches!(&a.value, Expr::Str(s, _) if s == "auto"),
            _ => false,
        })
    {
        return None;
    }
    let tabs = only_child(list)?;
    let tab = each(tabs, router, "tabs", "name")?;
    let button = only_child(tabs)?;
    let ("button", _, attrs) = button.element()? else {
        return None;
    };
    if !visible(attrs) || literal(attrs, "role") != Some("tab") {
        return None;
    }
    for child in button.children() {
        let ("image" | "text", _, attrs) = child.element()? else {
            return None;
        };
        if attr(attrs, "role").is_some()
            || attr(attrs, "class").is_some()
            || child.children().next().is_some()
        {
            return None;
        }
    }
    key(attr(attrs, "aria-controls")?, |e| member(e, tab, "name"))
}

// The kernel collects roles even on a keyed column, before pruning its children.
// A competing tablist can own the root. A key in another panel can also
// select that panel unless every column uses its own entry id.
fn only_route_columns(entries: Structure<'_>, var: &str, route_key: (&str, &str)) -> bool {
    entries
        .walk_regions_with((), |n, _| {
            let descend = n.element().is_none().then_some(());
            [descend, descend]
        })
        .all(|(n, ())| match n.element() {
            Some(("column", _, a)) => {
                attr(a, "navigationKey").and_then(|k| key(k, |e| member(e, var, "id")))
                    == Some(route_key)
                    && attr(a, "role").is_none()
                    && attr(a, "class").is_none()
            }
            None => n.condition().is_some(),
            _ => false,
        })
}

struct Stack<'a> {
    entries: Structure<'a>,
    var: &'a str,
}

impl<'a> Stack<'a> {
    fn linked(root: Visit<'a>, router: &str) -> Option<Self> {
        let ("main" | "column" | "view", _, attrs) = root.element()? else {
            return None;
        };
        if !visible(attrs) {
            return None;
        }
        let route_key = key(attr(attrs, "navigationKey")?, |e| {
            let Expr::Member(base, field, _) = e else {
                return false;
            };
            let Expr::Call(f, args, _) = &**base else {
                return false;
            };
            field == "id" && f == "top" && matches!(&args[..], [Expr::Ident(n, _)] if n == router)
        })?;
        let mut controls = None;
        let mut panel_id = None;
        let mut stack = None;
        // The kernel takes the first linked list outside a keyed route. Only
        // accept one list and one panel loop, with plain boxes between them.
        for (node, ()) in
            root.regions()[0].walk_regions_with((), |n, _| [plain_box(n).then_some(()), None])
        {
            if let Some((_, _, attrs)) = node.element() {
                if !visible(attrs) || attr(attrs, "navigationKey").is_some() {
                    return None;
                }
                if !plain_box(node) && controls.replace(control_key(node, router)?).is_some() {
                    return None;
                }
            } else {
                let tab = each(node, router, "tabs", "name")?;
                let panel = only_child(node)?;
                let ("column", _, attrs) = panel.element()? else {
                    return None;
                };
                if stack.is_some()
                    || !visible(attrs)
                    || literal(attrs, "role") != Some("tabpanel")
                    || attr(attrs, "navigationKey").is_some()
                {
                    return None;
                }
                panel_id = Some(key(attr(attrs, "id")?, |e| member(e, tab, "name"))?);
                let entries = only_child(panel)?;
                let var = each(entries, tab, "stack", "id")?;
                let entries = entries.regions()[0];
                if !only_route_columns(entries, var, route_key) {
                    return None;
                }
                stack = Some(Self { entries, var });
            }
        }
        stack.filter(|_| controls.is_some() && controls == panel_id)
    }

    fn column(&self, route: &str) -> Option<Site> {
        let mut column = None;
        let mut known = true;
        for (node, ()) in self.entries.walk_regions_with((), |n, _| {
            let arm = n.condition().and_then(|e| guard(e, self.var, route));
            if n.element().is_none() && arm.is_none() {
                known = false;
            }
            [
                (arm == Some(true)).then_some(()),
                (arm == Some(false)).then_some(()),
            ]
        }) {
            if let Some((_, _, attrs)) = node.element() {
                if column.is_some()
                    || !visible(attrs)
                    || attr(attrs, "navigationPresentation").is_some()
                {
                    return None;
                }
                column = Some(node.site());
            }
        }
        column.filter(|_| known)
    }
}

fn guard(expr: &Expr, var: &str, route: &str) -> Option<bool> {
    let word = |e: &Expr| match e {
        Expr::Str(s, _) => Some(route == s),
        _ => None,
    };
    match expr {
        Expr::Bool(b, _) => Some(*b),
        Expr::Unary(UnOp::Not, e, _) => Some(!guard(e, var, route)?),
        Expr::Binary(op @ (BinOp::Eq | BinOp::Ne), a, b, _) => {
            let same = if member(a, var, "name") {
                word(b)?
            } else if member(b, var, "name") {
                word(a)?
            } else {
                return None;
            };
            Some(same == (*op == BinOp::Eq))
        }
        Expr::Binary(BinOp::And, a, b, _) => Some(guard(a, var, route)? && guard(b, var, route)?),
        Expr::Binary(BinOp::Or, a, b, _) => Some(guard(a, var, route)? || guard(b, var, route)?),
        _ => None,
    }
}

pub(super) fn check(
    view: Structure<'_>,
    routes: &RoutesDecl,
    marked: &BTreeMap<Site, Span>,
    warnings: &mut Vec<LowerError>,
) {
    let Some(stack) = view
        .roots()
        .next()
        .and_then(|root| Stack::linked(root, &routes.slot))
    else {
        return;
    };
    // A visit below a presented or unidentified ancestor has no proven
    // place in the tab's navigation controller. Parents precede children.
    let mut in_tab = Vec::with_capacity(routes.rows.len());
    let mut pushed = Vec::new();
    for route in &routes.rows {
        let column = (route.tab || route.parent.is_some_and(|p| in_tab[p]))
            .then(|| stack.column(&route.name))
            .flatten();
        in_tab.push(column.is_some());
        let Some(&span) = column.and_then(|c| marked.get(&c)) else {
            continue;
        };
        if route.tab {
            warnings.push(LowerError {
                id: "lower-toolbar-tab-root",
                span,
                message: format!("On an iPhone, UIKit draws the bottom toolbar under the app's tab bar, which can cover its buttons. The `{}` tab root keeps its tabs. Put the actions in the header or push the screen.", route.name),
            });
        } else {
            pushed.push((route.name.as_str(), span));
        }
    }
    // These routes share one tablist. A separate display per route would
    // silence the other warnings as soon as the author applied the first.
    if let Some(&(first, span)) = pushed.first() {
        let top = format!("top({}).name", routes.slot);
        let hide = if pushed.len() == 1 {
            format!("{top} == \"{first}\"")
        } else {
            let names = pushed
                .iter()
                .map(|(n, _)| format!("\"{n}\""))
                .collect::<Vec<_>>();
            format!("[{}].includes({top})", names.join(", "))
        };
        warnings.push(LowerError {
            id: "lower-toolbar-tabs",
            span,
            message: format!("On an iPhone, UIKit draws the bottom toolbar under the app's tab bar, which can cover its buttons. Hide the tablist on the pushed toolbar routes by writing `display=({hide} ? \"none\" : \"flex\")` on the app's tablist."),
        });
    }
}
