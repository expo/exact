//! A select's menu as the top layer's (LLP 1069.001 D7, LLP 1115.001 D10,
//! LLP 1115.004 D10.4). Where a shell presents the top layer (Android's
//! windows, Windows' overlays), a select's options open as its menu, a
//! popup of the platform's, as Material's exposed dropdown menu, Android's
//! `Spinner` and WinUI's `ComboBox` open theirs: a row for each option, the
//! chosen one checked, hung under the select and no narrower than it (a
//! `ComboBox` opens its list over its box instead, which is not built).
//! Choosing a row sets the select's value as a press on the
//! painted menu's row does (`select.rs`), which is what desktop Linux,
//! with no top layer, still shows.
//!
//! The select keeps the focus while its menu is open, as a combobox keeps
//! it with its listbox open; the surface's focused row is the combobox's
//! active descendant. Its accessibility is a listbox of options
//! (`access/surface.rs`), as HTML-AAM maps a `select`'s popup.
use super::*;
use exact_kernel::ControlKind;

impl<D: DataSource> Presenter<D> {
    /// Whether `id` is a select.
    pub(in crate::presenter) fn is_select(&self, id: ViewId) -> bool {
        (self.host.kernel().node(id))
            .is_some_and(|n| ControlKind::of(n.node_type, n.props) == Some(ControlKind::Select))
    }

    /// The select whose menu is the open surface.
    pub(in crate::presenter) fn select_surface(&self) -> Option<ViewId> {
        let open = self.top.open.as_ref()?;
        self.is_select(open.node).then_some(open.node)
    }

    /// Select `id`'s menu: an item for each option, the chosen one checked.
    /// None for a disabled select or one with no option.
    pub(super) fn select_plan(&self, id: ViewId) -> Option<Plan> {
        let node = self.host.kernel().node(id)?;
        if node.props.bool(PropId::Disabled) == Some(true) {
            return None;
        }
        let chosen = self.selected_row(id);
        let choices = self.host.kernel().select_choices(id);
        let rows = (choices.into_iter().enumerate()).map(|(at, choice)| {
            Entry::Item(Row {
                node: choice.view,
                title: choice.label,
                symbol: None,
                destructive: false,
                enabled: !choice.disabled,
                cancel: false,
                checked: chosen == Some(at),
                opens: None,
            })
        });
        let rows: Vec<Entry> = rows.collect();
        (!rows.is_empty()).then_some(Plan::Menu(rows))
    }

    /// How wide select `id` is painted: its menu is no narrower.
    pub(super) fn select_width(&mut self, id: ViewId) -> Option<f32> {
        if !self.is_select(id) {
            return None;
        }
        self.box_of(id).map(|b| b.rect.2)
    }

    /// Open select `id`'s menu as the top layer's, scrolled to its chosen
    /// row. `keyboard`: a key opened it, and the surface's focus starts at
    /// the chosen row, as it does on a TV.
    pub(in crate::presenter) fn present_select(&mut self, id: ViewId, keyboard: bool) {
        let (Some(plan), Some(node)) = (self.select_plan(id), self.host.kernel().node(id)) else {
            return;
        };
        let key = node.key;
        let now = self.host.now();
        self.open_surface((id, key), plan, false, (Some(id), None), now);
        let keys = keyboard || self.remote();
        let Some(open) = self.top.open.as_mut() else {
            return;
        };
        let chosen = open.plan.rows().position(|row| row.checked);
        if let Some(at) = chosen {
            open.layout.reveal_menu_target(at);
        }
        if keys {
            match chosen.filter(|&at| open.layout.targets[at].enabled) {
                Some(at) => open.focused = Some(at),
                None => self.surface_step(false),
            }
        }
        self.surface_changed();
    }

    /// Row `index` of the open select's menu chosen: the menu closes and
    /// the select takes the row's option, `input` then `change`. Whether
    /// it was chosen.
    pub(super) fn choose_option(&mut self, select: ViewId, index: usize, now_ms: f64) -> bool {
        let open = self.top.open.as_ref();
        let Some(row) = open.and_then(|o| o.plan.rows().nth(index).filter(|r| r.enabled)) else {
            return false;
        };
        let view = row.node;
        self.close_surface(now_ms);
        let choices = self.host.kernel().select_choices(select);
        let Some(choice) = choices.iter().find(|c| c.view == view && !c.disabled) else {
            return false;
        };
        let done = self.set_control_value(select, &choice.value);
        if let Err(e) = &done {
            self.host.log(format!("select: {e}"));
        }
        done.is_ok()
    }

    /// A service chooses option `view` of the open select's menu: whether
    /// it was chosen.
    pub(in crate::presenter) fn choose_surface_option(&mut self, view: ViewId) -> bool {
        let (Some(select), Some(open)) = (self.select_surface(), self.top.open.as_ref()) else {
            return false;
        };
        let Some(index) = open.layout.targets.iter().position(|t| t.node == view) else {
            return false;
        };
        let now = self.host.now();
        self.choose_option(select, index, now)
    }

    /// A service scrolls the open select's menu by what it shows, less a
    /// row kept in view: whether it moved.
    pub(in crate::presenter) fn scroll_select_surface(&mut self, forward: bool) -> bool {
        let Some(open) = self
            .top
            .open
            .as_mut()
            .filter(|o| o.layout.menu_scroll().is_some())
        else {
            return false;
        };
        let shows = open
            .layout
            .menu_clip()
            .map_or(open.layout.size.1, |clip| clip.3);
        let row = open.layout.targets.first().map_or(shows, |t| t.rect.3);
        let step = (shows - row).max(row).max(1.0);
        if !open
            .layout
            .scroll_content(if forward { step } else { -step })
        {
            return false;
        }
        (open.pressed, open.hovered) = (None, None);
        open.revision += 1;
        self.dirty = true;
        self.executor.notify();
        true
    }

    /// Put the open select's menu's focus at row `at`, scrolled into view.
    pub(in crate::presenter) fn focus_select_row(&mut self, at: usize) {
        let row = (self.top.open.as_ref()).and_then(|o| o.layout.targets.get(at).map(|t| t.node));
        if let Some(row) = row {
            self.surface_focus(row);
        }
    }

    /// A key at the open select's menu before the surface's own
    /// (`surface_key`), as the painted menu takes it (`select.rs`): Tab
    /// closes it and goes on to move the focus; Enter or Space with no row
    /// focused closes it; the arrows start from the chosen row; Home, End,
    /// PageUp and PageDown move among the enabled rows; a letter moves to
    /// the next row its label starts with. `None`: the surface's own keys
    /// take it; else whether the menu took it.
    pub(super) fn select_surface_key(&mut self, name: &str, now_ms: f64) -> Option<bool> {
        let select = self.select_surface()?;
        let chord = self.modifiers().ctrl || self.modifiers().meta;
        let open = self.top.open.as_ref()?;
        let targets = &open.layout.targets;
        let enabled: Vec<usize> = (0..targets.len()).filter(|&i| targets[i].enabled).collect();
        let chosen = open.plan.rows().position(|row| row.checked);
        let at = open.focused.or(chosen);
        let shows = open
            .layout
            .menu_clip()
            .map_or(open.layout.size.1, |clip| clip.3);
        let row = targets.first().map_or(shows, |t| t.rect.3).max(1.0);
        let page = ((shows / row).floor().max(2.0) as usize) - 1;
        let to = match name {
            "Tab" => {
                self.close_surface(now_ms);
                return Some(false);
            }
            "Enter" | " " if open.focused.is_none() => {
                self.close_surface(now_ms);
                return Some(true);
            }
            "ArrowDown" | "ArrowUp" if open.focused.is_none() && chosen.is_some() => chosen,
            "Home" => enabled.first().copied(),
            "End" => enabled.last().copied(),
            "PageDown" => {
                let goal = at.map_or(0, |at| at + page);
                (enabled
                    .iter()
                    .rev()
                    .find(|&&i| i <= goal && at.is_none_or(|at| i > at)))
                .or_else(|| enabled.iter().find(|&&i| at.is_none_or(|at| i > at)))
                .copied()
            }
            "PageUp" => {
                let goal = at.map_or(0, |at| at.saturating_sub(page));
                (enabled
                    .iter()
                    .find(|&&i| i >= goal && at.is_none_or(|at| i < at)))
                .or_else(|| enabled.iter().rev().find(|&&i| at.is_none_or(|at| i < at)))
                .copied()
            }
            s if s.chars().count() == 1 && s != " " && !chord => {
                let choices = self.host.kernel().select_choices(select);
                Self::typed_row(&choices, at, s)
            }
            _ => return None,
        };
        // The arrows go on to step from the chosen row.
        let step = matches!(name, "ArrowDown" | "ArrowUp");
        if let Some(to) = to {
            self.focus_select_row(to);
        }
        (!step).then_some(true)
    }
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
