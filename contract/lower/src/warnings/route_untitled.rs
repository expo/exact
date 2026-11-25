//! Route-local Back history diagnostics (LLP 1075.003 §9.10).
use super::{computed, has, header_bar, literal};
use crate::LowerError;
use contract_checking::Visit;
use contract_syntax::{Expr, Span};
use exact_kernel::bars::blank_history_label;

pub(super) fn warning(span: Span) -> LowerError {
    LowerError {
        id: "lower-route-untitled",
        message: "This route has no stable Back history label. Give it a heading or `head title`, or write `head title=none` to allow an unnamed row.".into(),
        span,
    }
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
