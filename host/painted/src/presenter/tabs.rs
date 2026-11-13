//! Tablist focus follows the web's rule (LLP 1115.000 D4.5;
//! LLP 1115.004 D9.1): arrows move focus, Enter or Space activates.
use super::*;
use std::collections::BTreeSet;

impl<D: DataSource> Presenter<D> {
    fn tablist_of(&self, id: ViewId) -> Option<ViewId> {
        let kernel = self.host.kernel();
        let tab = kernel.node(id)?;
        if tab.props.str(PropId::AccessibilityRole) != Some("tab") {
            return None;
        }
        let mut at = tab.parent;
        while let Some(id) = at {
            let node = kernel.node(id)?;
            match node.props.str(PropId::AccessibilityRole) {
                Some("tablist") => return Some(id),
                Some("menu") => return None,
                _ => at = node.parent,
            }
        }
        None
    }

    fn tab_members(&self, list: ViewId) -> Vec<ViewId> {
        self.host
            .kernel()
            .preorder_where(&[list], |_, props| {
                props.str(PropId::AccessibilityRole) == Some("tab")
            })
            .into_iter()
            .filter(|id| self.tablist_of(*id) == Some(list))
            .collect()
    }

    fn tab_focusable(&self, id: ViewId, boxed: &BTreeSet<ViewId>) -> bool {
        self.focus_refusal(id).is_none()
            && self.focusable(id)
            && (boxed.contains(&id) || self.inline_box(id).is_some())
    }

    fn tab_usable(&self, id: ViewId, boxed: &BTreeSet<ViewId>) -> bool {
        self.tab_focusable(id, boxed)
            && self
                .host
                .kernel()
                .node(id)
                .is_some_and(|n| n.props.bool(PropId::AccessibilityDisabled) != Some(true))
    }

    pub(super) fn tab_key(&mut self, id: ViewId, name: &str, now_ms: f64) -> bool {
        let held = self.modifiers();
        if held.ctrl
            || held.alt
            || held.meta
            || !matches!(
                name,
                "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End"
            )
        {
            return false;
        }
        let Some(list) = self.tablist_of(id) else {
            return false;
        };
        let node = self.host.kernel().node(list).unwrap();
        let vertical = node.props.str(PropId::AccessibilityOrientation) == Some("vertical");
        let rtl = node.computed_row(exact_kernel::StyleId::Direction, |s| s.direction)
            == exact_kernel::Direction::Rtl;
        let forward = match name {
            "ArrowDown" if vertical => Some(true),
            "ArrowUp" if vertical => Some(false),
            "ArrowRight" if !vertical => Some(!rtl),
            "ArrowLeft" if !vertical => Some(rtl),
            "Home" | "End" => None,
            _ => return false,
        };
        let boxed = self.shown_boxes();
        let tabs: Vec<_> = self
            .tab_members(list)
            .into_iter()
            .filter(|tab| *tab == id || self.tab_usable(*tab, &boxed))
            .collect();
        let at = tabs.iter().position(|tab| *tab == id).unwrap();
        let next = match forward {
            Some(true) => (at + 1) % tabs.len(),
            Some(false) => (at + tabs.len() - 1) % tabs.len(),
            None if name == "Home" => 0,
            None => tabs.len() - 1,
        };
        if let Some(error) = self.set_focus(Some(tabs[next]), now_ms) {
            eprintln!("exact: {error}");
        }
        true
    }

    /// A focused tab that is still rendered and focusable is the list's only
    /// stop, even with `aria-disabled` or a negative `tabindex`. Re-entering
    /// chooses the selected eligible tab, or the first.
    pub(super) fn tab_stop(&self, id: ViewId, boxed: &BTreeSet<ViewId>) -> bool {
        let Some(list) = self.tablist_of(id) else {
            return self.focus_stop(id, boxed);
        };
        let mut tabs = self.tab_members(list);
        if let Some(at) = self
            .focus
            .filter(|at| tabs.contains(at) && self.tab_focusable(*at, boxed))
        {
            return at == id;
        }
        tabs.retain(|tab| self.focus_stop(*tab, boxed) && self.tab_usable(*tab, boxed));
        let selected = tabs.iter().find(|tab| {
            self.host
                .kernel()
                .node(**tab)
                .is_some_and(|n| n.props.bool(PropId::AccessibilitySelected) == Some(true))
        });
        selected.or_else(|| tabs.first()) == Some(&id)
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tabs_tests;
