//! The root tab bar's warnings: one tab (LLP 1116 D6), and the search tab's
//! place (LLP 1141.002 D1).

use super::{computed, has, literal};
use crate::LowerError;
use contract_checking::Visit;

/// `lower-single-tab`: a root tab bar (a `role="tablist"` whose tabs name
/// their panels with `aria-controls`) with fewer than two tabs (LLP 1116
/// D6): a bar of one tab under a single screen, which builders copied from
/// a tab skeleton (polls, chat-rooms).
pub(super) fn single_tab(tablist: Visit<'_>, out: &mut Vec<LowerError>) {
    let (mut tabs, mut controls, mut doubt) = (0, false, false);
    for (node, ()) in tablist.regions()[0].walk_regions_with((), |node, _| {
        let descend = node.element().is_some_and(|(_, _, attrs)| {
            !computed(attrs, "role") && literal(attrs, "role") != Some("tab")
        });
        [descend.then_some(()), None]
    }) {
        if let Some((_, _, attrs)) = node.element() {
            if computed(attrs, "role") {
                doubt = true;
            } else if literal(attrs, "role") == Some("tab") {
                tabs += 1;
                controls |= has(attrs, "aria-controls");
            }
        } else {
            doubt = true;
        }
    }
    if controls && !doubt && tabs < 2 {
        out.push(LowerError {
            id: "lower-single-tab",
            message: format!(
                "this tab bar has {} tab{}: a tab bar is for moving between two or more sections, and one tab is a bar that does nothing. For a single screen, drop the `role=\"tablist\"` and the `tabpanel` around the routes, so the routes are the navigation root's children (one stack); add the other tabs if the app has them (docs/contract-for-agents.md, \"Tabs and stacks\")",
                tabs,
                if tabs == 1 { "" } else { "s" }
            ),
            span: tablist.span(),
        });
    }
}

/// @ref LLP 1141.002 D1 — a root tablist's search tab: the first tab whose
/// `rel` holds `search`. `lower-search-tab-second`: another one is an
/// ordinary tab on every host. `lower-search-tab-last`: iOS 26 draws the
/// search tab at the trailing end, so the hosts that keep the author's
/// order agree with it only when it is written last.
pub(super) fn search_tab(tablist: Visit<'_>, out: &mut Vec<LowerError>) {
    let mut tabs: Vec<(&[contract_syntax::Attr], contract_syntax::Span)> = Vec::new();
    for (node, ()) in tablist.regions()[0].walk_regions_with((), |node, _| {
        let descend = node
            .element()
            .is_some_and(|(_, _, attrs)| literal(attrs, "role") != Some("tab"));
        [descend.then_some(()), None]
    }) {
        let Some((_, _, attrs)) = node.element() else {
            // A computed tab list is not judged.
            return;
        };
        if literal(attrs, "role") == Some("tab") {
            tabs.push((attrs, node.span()));
        }
    }
    let search: Vec<usize> = tabs
        .iter()
        .enumerate()
        .filter(|(_, (attrs, _))| {
            literal(attrs, "rel").is_some_and(|rel| exact_kernel::bars::rel_has(rel, "search"))
        })
        .map(|(i, _)| i)
        .collect();
    if let Some(&second) = search.get(1) {
        out.push(LowerError {
            id: "lower-search-tab-second",
            message: "this tablist already has a search tab (the first tab with `rel=\"search\"`); this one is an ordinary tab on every host. Drop its `rel`, or move the word to the tab that is the app's search".into(),
            span: tabs[second].1,
        });
    }
    if let Some(&first) = search.first() {
        if first + 1 != tabs.len() {
            out.push(LowerError {
                id: "lower-search-tab-last",
                message: "write the search tab last: iOS 26 draws it at the trailing end of the tab bar, and the hosts that keep the written order (the web, the Mac's sidebar, Android) then agree with it".into(),
                span: tabs[first].1,
            });
        }
    }
}
