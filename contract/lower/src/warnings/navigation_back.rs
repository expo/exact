//! A literal Back name with no element's id in the expanded view.
//! @ref LLP 1116.010 D5 — a provable miswiring warns and names its repair.

use super::{has, literal};
use crate::LowerError;
use contract_checking::View;
use contract_syntax::Expr;
use std::collections::BTreeSet;

pub(super) fn unnamed(view: &View<'_>, out: &mut Vec<LowerError>) {
    let Some((_, _, attrs)) = view
        .structure()
        .roots()
        .next()
        .and_then(|node| node.element())
    else {
        return;
    };
    if !has(attrs, "navigationKey") {
        return;
    }
    let Some(back) = attrs.iter().rev().find(|a| a.name == "navigationBack") else {
        return;
    };
    let Expr::Str(name, _) = &back.value else {
        return;
    };
    if name.is_empty() {
        return;
    }
    let (mut ids, mut test_ids) = (BTreeSet::new(), BTreeSet::new());
    for node in view.walk() {
        if let Some((_, _, attrs)) = node.element() {
            if let Some(id) = attrs.iter().rev().find(|a| a.name == "id") {
                let Expr::Str(id, _) = &id.value else {
                    // A computed element id could be the name at run time.
                    return;
                };
                ids.insert(id.as_str());
            }
            if let Some(id) = literal(attrs, "testId") {
                test_ids.insert(id);
            }
        } else if node.component_use().is_some() || node.is_slot() {
            return;
        }
    }
    if ids.contains(name.as_str()) {
        return;
    }
    let repair = if test_ids.contains(name.as_str()) {
        format!("`testId=\"{name}\"` names a control for tests and the agent; give it `id=\"{name}\"` as well")
    } else if let Some(near) = contract_syntax::suggestion(name, ids.iter().copied()) {
        format!("did you mean `navigationBack=\"{near}\"`? Otherwise give the intended control `id=\"{name}\"` and a `press` handler")
    } else {
        format!("give the intended control `id=\"{name}\"` and a `press` handler, or omit `navigationBack` to use the fallback")
    };
    out.push(LowerError {
        id: "lower-navigation-back-id",
        message: format!("`navigationBack=\"{name}\"` names a control's `id`, not an action, but no element in the view has that id. Back falls back to the root's `navigate` handler, or to the router's Back when there is no handler; {repair}. The control must be in the active route or outside every route, and enabled, for Back to press it (docs/recipes/navigation-stack.md)"),
        span: back.span,
    });
}
