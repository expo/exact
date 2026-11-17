//! Windows common item dialogs (LLP 1173 D1–D2).
use exact_runner::file_pickers::Kind;

#[cfg(windows)]
mod os;
#[cfg(windows)]
pub(super) use os::Dialogs;

use exact_painted::presenter::file_dialog::{FileDialog, Outcome};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

#[derive(Default)]
struct State {
    posted: Cell<bool>,
    open: Cell<bool>,
    queued: RefCell<VecDeque<(u32, FileDialog, std::sync::Weak<()>)>>,
    answered: RefCell<Vec<Answer>>,
}

pub(crate) struct Answer {
    pub id: u32,
    pub outcome: Outcome,
    pub lines: Vec<String>,
}

impl State {
    fn show(
        &self,
        choose: impl FnOnce(&FileDialog, &mut Vec<String>) -> Outcome,
        wake: impl FnOnce(),
        post: impl FnOnce(),
    ) {
        self.posted.set(false);
        if self.open.replace(true) {
            return;
        }
        let request = self.queued.borrow_mut().pop_front();
        if let Some((id, dialog, _)) = request.filter(|(_, _, live)| live.strong_count() != 0) {
            let mut lines = Vec::new();
            let outcome = choose(&dialog, &mut lines);
            self.answered
                .borrow_mut()
                .push(Answer { id, outcome, lines });
            wake();
        }
        self.open.set(false);
        post();
    }
}

// FILEOPENDIALOGOPTIONS values, kept portable so the mapping runs on every host.
// https://learn.microsoft.com/windows/win32/api/shobjidl_core/ne-shobjidl_core-_fileopendialogoptions
fn options(kind: Kind, multiple: bool, defaults: u32) -> u32 {
    const FORCE_FILESYSTEM: u32 = 0x40;
    const NO_CHANGE_DIR: u32 = 0x8;
    const DONT_ADD_TO_RECENT: u32 = 0x0200_0000;
    const ALLOW_MULTISELECT: u32 = 0x200;
    const PICK_FOLDERS: u32 = 0x20;
    defaults
        | FORCE_FILESYSTEM
        | NO_CHANGE_DIR
        | DONT_ADD_TO_RECENT
        | match kind {
            Kind::Open if multiple => ALLOW_MULTISELECT,
            Kind::Directory => PICK_FOLDERS,
            _ => 0,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_dialogs_never_run_and_the_next_dialog_still_runs() {
        use exact_runner::file_pickers::Picker;
        use std::sync::Arc;
        let state = State::default();
        let live = Arc::new(());
        let retired = Arc::downgrade(&Arc::new(()));
        let dialog = FileDialog {
            command: "showOpenFilePicker",
            picker: Picker {
                kind: Kind::Open,
                id: "result".into(),
                multiple: false,
                suggested_name: String::new(),
                start_in: None,
            },
        };
        state.queued.borrow_mut().extend([
            (1, dialog.clone(), retired),
            (2, dialog, Arc::downgrade(&live)),
        ]);
        let shown = Cell::new(0);
        let posts = Cell::new(0);
        for _ in 0..2 {
            state.show(
                |_, _| {
                    shown.set(shown.get() + 1);
                    Outcome::Cancelled
                },
                || {},
                || posts.set(posts.get() + 1),
            );
        }
        assert_eq!(shown.get(), 1);
        assert_eq!(posts.get(), 2);
        let answered = state.answered.borrow();
        assert_eq!(answered.len(), 1);
        assert_eq!(answered[0].id, 2);
        assert!(matches!(answered[0].outcome, Outcome::Cancelled));
        assert!(answered[0].lines.is_empty());
        assert!(!state.open.get());
    }

    #[test]
    fn requests_preserve_shell_defaults_and_add_only_their_selection_options() {
        let base = 0x0200_0048;
        for (kind, multiple, defaults, expected) in [
            (Kind::Open, false, 0x1800, base | 0x1800),
            (Kind::Open, true, 0x1800, base | 0x1a00),
            (Kind::Directory, false, 0x1800, base | 0x1820),
            (Kind::Save, false, 0x8802, base | 0x8802),
            (Kind::Save, true, 0x8802, base | 0x8802),
        ] {
            assert_eq!(options(kind, multiple, defaults), expected, "{kind:?}");
        }
    }
}
