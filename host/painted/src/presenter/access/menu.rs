//! A select's open menu, as HTML-AAM maps a `select` and the WAI-ARIA
//! combobox pattern has it (<https://www.w3.org/TR/html-aam-1.0/#el-select-combobox>,
//! <https://www.w3.org/WAI/ARIA/apg/patterns/combobox/>): the select is a
//! combobox that says it is expanded, and its popup is a listbox under it
//! whose options are the menu's rows, the chosen one selected. The menu is
//! light-dismiss and takes every key and press while it is open
//! (`select.rs`), so the page under it is left out, as Android's
//! `Spinner` popup and WinUI's `ComboBox` list leave it.
use super::*;

/// The open menu's listbox, which stands for no view.
const LIST_BOX: i32 = 0x7fff_0005;

impl<D: DataSource> Presenter<D> {
    /// Narrow the tree, built from fresh boxes, to the open menu's select
    /// and its listbox; nothing changes while no menu is open.
    pub(super) fn access_menu(&self, b: &mut Build) {
        let Some(select) = self.menu else {
            return;
        };
        let Some(menu) = self.menu_paint() else {
            return;
        };
        let kernel = self.host.kernel();
        let kept = b.out.iter().find(|n| n.view == select).cloned();
        b.out.clear();
        b.used.clear();
        let parent = match kept {
            Some(mut dropdown) => {
                b.used.insert(dropdown.id);
                dropdown.parent = 0;
                let id = dropdown.id;
                b.out.push(dropdown);
                id
            }
            None => 0,
        };
        let (mx, my, mw, mh) = menu.rect;
        let pad = crate::paint::control::MENU_PAD;
        let full = menu.row * menu.rows.len() as f32 + 2.0 * pad;
        let max = (full - mh).max(0.0);
        let mut flags = 0;
        if menu.offset < max - 0.5 {
            flags |= flag::FORWARD;
        }
        if menu.offset > 0.5 {
            flags |= flag::BACKWARD;
        }
        let (ox, oy) = b.origin;
        let list = b.unique(LIST_BOX);
        b.out.push(Node {
            id: list,
            parent,
            view: 0,
            role: role::LIST_BOX,
            flags,
            rect: [mx + ox, my + oy, mw, mh],
            range: [menu.offset, max, 0.0],
            set: (0, 0),
            name: String::new(),
            value: String::new(),
            hint: String::new(),
        });
        let chosen = self.selected_row(select);
        let choices = kernel.select_choices(select);
        let count = choices.len() as i32;
        for (at, choice) in choices.into_iter().enumerate() {
            let top = my + pad + at as f32 * menu.row - menu.offset;
            let Some(shows) = intersect((mx, top, mw, menu.row), menu.rect) else {
                continue;
            };
            let mut flags = match choice.disabled {
                true => flag::DISABLED,
                false => flag::CLICK,
            };
            if chosen == Some(at) {
                flags |= flag::SELECTED;
            }
            let id = b.unique(identity(kernel, choice.view));
            b.out.push(Node {
                id,
                parent: list,
                view: choice.view,
                role: role::OPTION,
                flags,
                rect: [shows.0 + ox, shows.1 + oy, shows.2, shows.3],
                range: [0.0; 3],
                set: (at as i32 + 1, count),
                name: choice.label,
                value: String::new(),
                hint: String::new(),
            });
        }
    }

    /// The node of the open menu's row the keys are at, which is where the
    /// focus is while the menu is open (the combobox pattern's active
    /// descendant). Windows reads it (`desktop_access.rs`).
    #[cfg(any(windows, test))]
    pub(in crate::presenter) fn access_menu_focus(&self) -> Option<i32> {
        let select = self.menu?;
        let at = self.menu_paint()?.chosen?;
        let view = self.host.kernel().select_choices(select).get(at)?.view;
        (self.access.nodes.iter())
            .find(|n| n.role == role::OPTION && n.view == view)
            .map(|n| n.id)
    }

    /// A service chooses option `view` of the open menu, as a press on its
    /// row does: whether it was chosen.
    pub(super) fn access_choose(&mut self, view: ViewId) -> bool {
        let Some(select) = self.menu else {
            return false;
        };
        let choices = self.host.kernel().select_choices(select);
        let Some(choice) = choices.iter().find(|c| c.view == view && !c.disabled) else {
            return false;
        };
        let done = self.set_control_value(select, &choice.value);
        if let Err(e) = &done {
            self.host.log(format!("accessibility: select: {e}"));
        }
        done.is_ok()
    }

    /// A service scrolls the open menu by what it shows, less a row kept in
    /// view: whether it moved.
    pub(super) fn access_menu_scroll(&mut self, forward: bool) -> bool {
        let Some(menu) = self.menu_now() else {
            return false;
        };
        let shows = menu.rect.3 - 2.0 * crate::paint::control::MENU_PAD;
        let step = (shows - menu.row).max(menu.row).max(1.0);
        self.menu_scroll = menu.offset + if forward { step } else { -step };
        // Held to the menu's range, as its geometry holds it.
        let to = self.menu_now().map_or(0.0, |m| m.offset);
        self.menu_scroll = to;
        self.dirty = true;
        to != menu.offset
    }
}
