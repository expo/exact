//! A select's painted menu and its keys (LLP 1069.001 D7), as the WAI-ARIA
//! Authoring Practices' select-only combobox has them: a focused select
//! opens on Space, Enter or an arrow, and a letter chooses the next option
//! its label starts with; an open menu's arrows, Home, End, PageUp and
//! PageDown move among its enabled rows, Enter or Space chooses one, a
//! letter moves to one, and Escape, Tab and system Back close it. A menu
//! longer than the window is held to it and scrolls, as WinUI's `ComboBox`
//! and Android's `Spinner` hold theirs.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// The row of select `id`'s chosen option: its choice since its bound
    /// value changed, else the option the kernel chose.
    pub(super) fn selected_row(&self, id: ViewId) -> Option<usize> {
        let node = self.host.kernel().node(id)?;
        let choices = self.host.kernel().select_choices(id);
        match crate::paint::control::choice(&node, self.chosen.get(&id)) {
            Some(value) => choices.iter().position(|c| c.value == value),
            None => {
                let chosen = self.host.kernel().select_chosen(id).map(|c| c.view);
                choices.iter().position(|c| Some(c.view) == chosen)
            }
        }
    }

    /// The open menu as it paints now, from fresh boxes.
    pub(super) fn menu_now(&mut self) -> Option<crate::paint::control::MenuPaint> {
        self.menu?;
        self.boxes();
        self.menu_paint()
    }

    /// Open select `id`'s menu at its chosen row.
    pub(super) fn open_menu(&mut self, id: ViewId) {
        self.menu = Some(id);
        self.menu_row = None;
        self.menu_scroll = 0.0;
        if let Some(at) = self.menu_now().and_then(|m| m.chosen) {
            self.reveal_menu_row(at);
        }
        self.dirty = true;
        self.back_changed();
    }

    /// Close the open menu; false when none was open.
    pub(super) fn close_menu(&mut self) -> bool {
        self.menu_row = None;
        self.menu_scroll = 0.0;
        let open = self.menu.take().is_some();
        if open {
            self.dirty = true;
            self.back_changed();
        }
        open
    }

    /// Scroll the open menu so that row `at` shows, and put the keys there.
    fn reveal_menu_row(&mut self, at: usize) {
        self.menu_row = Some(at);
        self.dirty = true;
        let Some(menu) = self.menu_now() else {
            return;
        };
        let shows = menu.rect.3 - 2.0 * crate::paint::control::MENU_PAD;
        let (top, bottom) = (at as f32 * menu.row, (at + 1) as f32 * menu.row);
        if top < menu.offset {
            self.menu_scroll = top;
        } else if bottom > menu.offset + shows {
            self.menu_scroll = bottom - shows;
        }
    }

    /// A wheel over the open menu scrolls it; elsewhere it is the page's.
    pub(super) fn menu_wheel(&mut self, x: f32, y: f32, dy: f32) -> bool {
        let Some(menu) = self.menu_now() else {
            return false;
        };
        let (mx, my, mw, mh) = menu.rect;
        if x < mx || x >= mx + mw || y < my || y >= my + mh {
            return false;
        }
        self.menu_scroll = menu.offset + dy;
        // Held to the menu's range, as its geometry holds it.
        if let Some(menu) = self.menu_now() {
            self.menu_scroll = menu.offset;
        }
        self.dirty = true;
        true
    }

    /// The row a letter moves to: the next enabled one after `after`
    /// (wrapping) whose label starts with `letter`, ignoring case, as a
    /// browser's select types ahead.
    fn typed_row(
        choices: &[exact_kernel::Choice],
        after: Option<usize>,
        letter: &str,
    ) -> Option<usize> {
        let letter = letter.to_lowercase();
        let n = choices.len();
        let from = after.map_or(0, |at| at + 1);
        (0..n).map(|k| (from + k) % n).find(|&i| {
            !choices[i].disabled
                && choices[i]
                    .label
                    .trim_start()
                    .to_lowercase()
                    .starts_with(&letter)
        })
    }

    /// A key while a menu is open: true when the menu took it. Tab closes
    /// the menu and goes on to move the focus; a chord with Control or Meta
    /// is the page's shortcut.
    pub(super) fn menu_key(&mut self, name: &str) -> bool {
        let Some(id) = self.menu else {
            return false;
        };
        let Some(menu) = self.menu_now() else {
            self.close_menu();
            return false;
        };
        let chord = self.modifiers().ctrl || self.modifiers().meta;
        let choices = self.host.kernel().select_choices(id);
        let enabled: Vec<usize> = (0..choices.len())
            .filter(|&i| !choices[i].disabled)
            .collect();
        let at = menu.chosen;
        let page = ((menu.rect.3 - 2.0 * crate::paint::control::MENU_PAD) / menu.row)
            .floor()
            .max(2.0) as usize
            - 1;
        let down = |goal: usize| {
            let past = |&&i: &&usize| at.is_none_or(|at| i > at);
            (enabled.iter().rev().filter(past).find(|&&i| i <= goal))
                .or_else(|| enabled.iter().find(past))
                .copied()
        };
        let up = |goal: usize| {
            let before = |&&i: &&usize| at.is_none_or(|at| i < at);
            (enabled.iter().filter(before).find(|&&i| i >= goal))
                .or_else(|| enabled.iter().rev().find(before))
                .copied()
        };
        let to = match name {
            "Escape" => {
                self.close_menu();
                return true;
            }
            "Tab" => {
                self.close_menu();
                return false;
            }
            "Enter" | " " => {
                match at.filter(|at| enabled.contains(at)) {
                    Some(at) => {
                        if let Err(e) = self.set_control_value(id, &choices[at].value) {
                            self.host.log(format!("select: {e}"));
                        }
                    }
                    None => {
                        self.close_menu();
                    }
                }
                return true;
            }
            _ if chord => return false,
            "ArrowDown" => down(at.map_or(0, |at| at + 1)),
            "ArrowUp" => up(at.map_or(0, |at| at.saturating_sub(1))),
            "PageDown" => down(at.map_or(0, |at| at + page)),
            "PageUp" => up(at.map_or(0, |at| at.saturating_sub(page))),
            "Home" => enabled.first().copied(),
            "End" => enabled.last().copied(),
            s if s.chars().count() == 1 => Self::typed_row(&choices, at, s),
            _ => None,
        };
        if let Some(to) = to {
            self.reveal_menu_row(to);
        }
        true
    }

    /// A key at a focused select whose menu is closed: true when it took
    /// it. Space, Enter, the arrows and F4 open the menu; Home and End open
    /// it at the first or last option; a letter chooses the next option its
    /// label starts with, `input` then `change`.
    pub(super) fn select_key(&mut self, id: ViewId, name: &str) -> bool {
        let Some(node) = self.host.kernel().node(id) else {
            return false;
        };
        if exact_kernel::ControlKind::of(node.node_type, node.props)
            != Some(exact_kernel::ControlKind::Select)
        {
            return false;
        }
        let chord = self.modifiers().ctrl || self.modifiers().meta;
        let choices = self.host.kernel().select_choices(id);
        let enabled = || (0..choices.len()).filter(|&i| !choices[i].disabled);
        match name {
            " " | "Enter" | "ArrowDown" | "ArrowUp" | "F4" => self.open_menu(id),
            "Home" | "End" => {
                let at = match name {
                    "Home" => enabled().next(),
                    _ => enabled().next_back(),
                };
                self.open_menu(id);
                if let Some(at) = at {
                    self.reveal_menu_row(at);
                }
            }
            s if s.chars().count() == 1 && !chord => {
                let at = self.selected_row(id);
                if let Some(to) = Self::typed_row(&choices, at, s).filter(|to| Some(*to) != at) {
                    if let Err(e) = self.set_control_value(id, &choices[to].value) {
                        self.host.log(format!("select: {e}"));
                    }
                }
            }
            _ => return false,
        }
        true
    }
}
