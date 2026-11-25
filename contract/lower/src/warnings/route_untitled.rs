//! Route-local Back history diagnostics (LLP 1075.003 §9.10).
use super::{computed, has, header_bar, literal};
use crate::LowerError;
use contract_checking::Visit;
use contract_syntax::{Expr, RoutesDecl, Span};
use exact_kernel::bars::blank_history_label;

pub(super) fn warning(span: Span) -> LowerError {
    LowerError {
        id: "lower-route-untitled",
        message: "This route has no stable Back history label. Give it a heading or `head title`, or write `head title=none` to allow an unnamed row.".into(),
        span,
    }
}

/// Whether a stack of the app's routes can hold two routes, one under the other: a table with a route that is not a
/// tab's root or the fallback, beside another. Tabs alone, or a single route, never show Back's history.
pub(super) fn stacks_grow(routes: Option<&RoutesDecl>) -> bool {
    let Some(routes) = routes else { return false };
    let rows = || routes.rows.iter().filter(|row| !row.notfound);
    rows().count() > 1 && rows().any(|row| !row.tab)
}

/// Whether a route is presented over its stack, as a dialog or a sheet, rather than pushed onto it: no row of the
/// stack's Back history, under which nothing is pushed. A computed presentation may be either.
pub(super) fn presented(attrs: &[contract_syntax::Attr]) -> bool {
    matches!(literal(attrs, "role"), Some("dialog" | "alertdialog"))
        || has(attrs, "navigationPresentation")
            && !matches!(literal(attrs, "navigationPresentation"), Some("push"))
}

pub(super) fn check(route: Visit<'_>, no_bar: bool) -> bool {
    let (_, _, attrs) = route.element().expect("a route element");
    let children = route.regions()[0];
    if !no_bar && (computed(attrs, "navigationBar") || !header_bar::blank_heading(children)) {
        return false;
    }
    let mut title = None;
    let mut unresolved = false;
    for (visit, (depth, conditional)) in
        children.walk_regions_with((0, false), |visit, &(depth, conditional)| {
            match visit.element() {
                Some((tag, _, attrs)) if tag != "head" && !boundary(tag, attrs) => [
                    Some((depth + 1, conditional || computed(attrs, "role"))),
                    None,
                ],
                Some(_) => [None, None],
                None => [Some((depth, true)), Some((depth, true))],
            }
        })
    {
        let Some((tag, _, attrs)) = visit.element() else {
            unresolved |= !visit.is_choice() && visit.children().next().is_none();
            continue;
        };
        if tag == "head" {
            if let Some(field) = attrs.iter().rfind(|a| a.name == "title") {
                if title.is_none_or(|(at, _, _)| depth >= at) {
                    title = Some((depth, conditional, &field.value));
                }
            }
        }
    }
    !unresolved
        && match title {
            None => true,
            Some((_, false, Expr::Str(text, _))) => blank_history_label(text),
            Some(_) => false,
        }
}

fn boundary(tag: &str, attrs: &[contract_syntax::Attr]) -> bool {
    tag == "dialog"
        || matches!(literal(attrs, "role"), Some("dialog" | "alertdialog"))
        || has(attrs, "navigationKey") && !has(attrs, "navigationBack")
}
