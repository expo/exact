//! `lower-list-beside-fixed`: a box that stays put above or below the
//! list that scrolls its route, holding more than controls (a caption, a
//! count, an image, a fill). On iOS 26 a hand-built screen's list runs under
//! what is pinned over its edges and the scroll edge effect blurs it there;
//! Exact draws a list under its neighbours only when they are bars of
//! controls (LLP 1131 D1), so beside this box the rows stop in a hard line.
//! Inbox's "320 messages" footer did this in bake-off r24.
//!
//! An `if`, `match`, use or slot beside the list, and a computed attribute,
//! are given the benefit of the doubt.
//!
//! The list in a row beside another box (contacts' own section index, bake-off
//! r28) is named too: the route's list is then no longer its own, so its
//! large title stays open and its rows stop over the bottom bar.

use super::{computed, has, literal};
use crate::LowerError;
use contract_checking::{Site, Structure, Visit};
use contract_syntax::{Attr, Expr, Span};
use std::collections::BTreeMap;

/// The plain boxes the walk looks inside, down to the scroller.
const BOXES: &[&str] = &[
    "view", "box", "column", "row", "nav", "section", "footer", "article", "aside", "main",
];

/// The tags taken whole as controls (LLP 1131 D1).
const CONTROLS: &[&str] = &["button", "link", "input", "textarea", "select"];

/// The route's scroller and the boxes above and below it on the way down,
/// warned. A `header` is left to the route's bar and its own warnings.
pub(super) fn fixed_beside(route: Visit<'_>, out: &mut Vec<LowerError>) {
    let Some((tag, _, attrs)) = route.element() else {
        return;
    };
    if across(tag, attrs) {
        return;
    }
    // A route whose first child is no `header` has no bar to run the list under.
    if route
        .children()
        .next()
        .and_then(|node| node.element())
        .is_some_and(|(tag, _, _)| tag != "header")
    {
        return;
    }
    let facts = facts(route);
    let mut selected = BTreeMap::new();
    for region in route.regions() {
        select(region, &facts, &mut selected);
    }
    for region in route.regions() {
        for _ in region.events((), |node, _| {
            let neighbours = selected.remove(&node.site())?;
            for (span, below) in neighbours {
                if out.iter().any(|e| e.id == "lower-list-beside-fixed" && e.span == span) { continue; }
                out.push(LowerError {
                    id: "lower-list-beside-fixed",
                    message: format!(
                        "this box stays put {} the list that scrolls this route, and holds more than controls (a text, an image or a fill), so on iOS the rows stop at it in a hard line where a hand-built screen's list scrolls under what is pinned over it, blurred by the scroll edge effect. Put what it says in the list (a section's `header` or `footer`, or the first row), or keep only buttons, fields and a tablist in it, unfilled: those the list runs under, faded and blurred where it passes as iOS does under a hand-built composer, so a field's text over it stays legible (docs/recipes/list-rows.md)",
                        if below { "below" } else { "above" }
                    ),
                    span,
                });
            }
            if let Some((tag, _, attrs)) = node.element() {
                if !BOXES.contains(&tag) || scrolls(tag, attrs) { return None; }
                if across(tag, attrs) {
                    if node.children().any(|child| !facts[&child.site()].holds && facts[&child.site()].beside[0]) {
                        out.push(LowerError {
                            id: "lower-list-beside-fixed",
                            message: "the list that scrolls this route sits in a row beside another box, so on iOS it is not the route's own: its large title never collapses, and its rows stop above the bottom bar (a search field there) where a hand-built screen's list collapses the title and runs under the bar. Make the list the route's child after its `header`, and leave what stands beside it to the platform: a list whose sections are each headed by one letter has UIKit's A–Z index, and sections above the letters (Favorites) are left out of it".into(),
                            span: node.span(),
                        });
                    }
                    return None;
                }
            } else if node.component_use().is_some() || node.is_slot() { return None; }
            for region in node.regions() { select(region, &facts, &mut selected); }
            Some(())
        }) {}
    }
}

/// Choose one scroller per sibling region, preserving neighbour order.
fn select(
    region: Structure<'_>,
    facts: &BTreeMap<Site, Facts>,
    selected: &mut BTreeMap<Site, Vec<(Span, bool)>>,
) {
    let Some((index, node)) = region
        .roots()
        .enumerate()
        .find(|(_, node)| facts[&node.site()].holds)
    else {
        return;
    };
    let neighbours = region
        .roots()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .filter_map(|(i, node)| facts[&node.site()].fixed.map(|span| (span, i > index)))
        .collect();
    selected.insert(node.site(), neighbours);
}

/// Whether a box lays its children side by side: its neighbours then
/// are beside the scroller, not over its ends.
fn across(tag: &str, attrs: &[Attr]) -> bool {
    match literal(attrs, "flex-direction") {
        Some(d) => d.starts_with("row"),
        None => tag == "row" || computed(attrs, "flex-direction"),
    }
}

#[derive(Clone, Copy)]
struct Facts {
    holds: bool,
    beside: [bool; 2],
    controls: Option<bool>,
    fixed: Option<Span>,
}

impl Default for Facts {
    fn default() -> Self {
        Self {
            holds: false,
            beside: [false; 2],
            controls: Some(true),
            fixed: None,
        }
    }
}

/// Postorder facts avoid rewalking descendants for each ancestor predicate.
fn facts(route: Visit<'_>) -> BTreeMap<Site, Facts> {
    let mut out = BTreeMap::new();
    let mut stack = Vec::<Facts>::new();
    for (node, (), entering) in route.subtree().events((), |_, _| Some(())) {
        if entering {
            stack.push(Facts::default());
            continue;
        }
        let children = stack.pop().unwrap();
        let fact = if let Some((tag, _, attrs)) = node.element() {
            let controls = if CONTROLS.contains(&tag) || literal(attrs, "role") == Some("tablist") {
                Some(true)
            } else if !BOXES.contains(&tag)
                || has(attrs, "background-color")
                || has(attrs, "background")
            {
                Some(false)
            } else {
                children.controls
            };
            let out_of_flow = matches!(literal(attrs, "position"), Some("absolute" | "fixed"))
                || literal(attrs, "display") == Some("none")
                || has(attrs, "popover")
                || tag == "dialog";
            let strip = match attrs
                .iter()
                .rev()
                .find(|a| a.name == "width")
                .map(|a| &a.value)
            {
                Some(Expr::Number(w, _)) => *w <= 48.0,
                Some(Expr::Str(w, _)) => w
                    .trim_end_matches("px")
                    .parse::<f64>()
                    .is_ok_and(|w| w <= 48.0),
                _ => false,
            };
            let fixed = !out_of_flow
                && !crate::routes::bottom_toolbar::bottom(attrs)
                && !has(attrs, "navigationSplit")
                && tag != "header"
                && !computed(attrs, "position")
                && !computed(attrs, "display")
                && controls == Some(false);
            Facts {
                holds: if scrolls(tag, attrs) {
                    has(attrs, "flex") || has(attrs, "flex-grow")
                } else {
                    BOXES.contains(&tag) && children.holds
                },
                beside: [!out_of_flow, !out_of_flow && strip],
                controls,
                fixed: fixed.then_some(node.span()),
            }
        } else if node.component_use().is_some() || node.is_slot() {
            Facts {
                beside: [node.component_use().is_some(), false],
                controls: None,
                ..Facts::default()
            }
        } else if node.is_choice() {
            Facts {
                beside: [children.beside[1]; 2],
                controls: children.controls.filter(|only| *only),
                fixed: None,
                ..children
            }
        } else {
            children
        };
        out.insert(node.site(), fact);
        if let Some(parent) = stack.last_mut() {
            parent.holds |= fact.holds;
            parent.beside[0] |= fact.beside[0];
            parent.beside[1] |= fact.beside[1];
            parent.controls = match (parent.controls, fact.controls) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (None, _) | (_, None) => None,
                _ => Some(true),
            };
            parent.fixed = parent.fixed.or(fact.fixed);
        }
    }
    out
}

fn scrolls(tag: &str, attrs: &[Attr]) -> bool {
    tag == "list" || (tag == "scroll" && literal(attrs, "overflow-y") != Some("hidden"))
}
