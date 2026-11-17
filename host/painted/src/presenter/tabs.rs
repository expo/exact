//! Painted radio groups, tablists and menus use the kernel's keys (LLP 1166.000 D2), and a menu
//! its type-ahead. An open surface of the top layer takes every key before these (`key_pressed`,
//! `surface_key`).
use super::*;
use std::collections::BTreeSet;

impl<D: DataSource> Presenter<D> {
    fn role_focusable(&self, id: ViewId, boxed: &BTreeSet<ViewId>) -> bool {
        self.focus_refusal(id).is_none()
            && self.focusable(id)
            && (boxed.contains(&id) || self.inline_box(id).is_some())
    }

    pub(super) fn tab_key(&mut self, id: ViewId, name: &str, now_ms: f64) -> bool {
        let held = self.modifiers();
        if held.ctrl || held.alt || held.meta {
            return false;
        }
        // The kernel answers `None` for a view or key that is not a group's.
        let Some(step) = self.host.kernel().role_step_order(id, name, None) else {
            return false;
        };
        let boxed = self.shown_boxes();
        let next = step
            .order
            .into_iter()
            .find(|id| self.role_focusable(*id, &boxed))
            .unwrap_or(id);
        if next != id {
            if let Some(error) = self.set_focus(Some(next), now_ms) {
                eprintln!("exact: {error}");
            }
            if step.press {
                self.note_press(next);
                self.activate_with(next, now_ms, false);
            }
        }
        true
    }

    /// A printable key at a menu's item moves the focus to the next item it starts
    /// (`Kernel::role_type_ahead`). Whether the key was the menu's; one that starts no item, or
    /// Space with no search under way, is left to the page.
    pub(super) fn type_ahead_key(&mut self, id: ViewId, name: &str, now_ms: f64) -> bool {
        let held = self.modifiers();
        if held.ctrl || held.alt || held.meta {
            return false;
        }
        let mut typed = std::mem::take(&mut self.type_ahead);
        let step = self
            .host
            .kernel()
            .role_type_ahead(id, name, now_ms, &mut typed);
        self.type_ahead = typed;
        // Only for a menu's key: the shown boxes lay the page out.
        let Some(step) = step else {
            return false;
        };
        let boxed = self.shown_boxes();
        let Some(next) = step
            .order
            .into_iter()
            .find(|id| self.role_focusable(*id, &boxed))
        else {
            return false;
        };
        if next != id {
            if let Some(error) = self.set_focus(Some(next), now_ms) {
                eprintln!("exact: {error}");
            }
        }
        true
    }

    pub(super) fn tab_stop(&self, id: ViewId, boxed: &BTreeSet<ViewId>) -> bool {
        self.host
            .kernel()
            .role_tab_stop(id, self.focus, |id| self.role_focusable(id, boxed))
            .unwrap_or_else(|| self.focus_stop(id, boxed))
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tabs_tests;

#[cfg(test)]
#[path = "role_radio_tests.rs"]
mod role_radio_tests;

#[cfg(test)]
#[path = "role_menu_tests.rs"]
mod role_menu_tests;
