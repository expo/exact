//! Windows common item dialogs (LLP 1173 D1–D2).
use exact_runner::file_pickers::Kind;

#[cfg(windows)]
mod os;
#[cfg(windows)]
pub(super) use os::Dialogs;

use exact_painted::presenter::file_dialog::{FileDialog, Outcome, StartFolder};
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

fn start_folder(
    request: &FileDialog,
    lines: &mut Vec<String>,
    apply: impl FnOnce(&StartFolder) -> Result<(), String>,
) {
    match request.start_folder() {
        Ok(Some(folder)) => {
            if let Err(reason) = apply(&folder) {
                lines.push(format!(
                    "{}: startIn: refused: {} is not a folder here ({reason})",
                    request.command,
                    request.picker.start_in.as_deref().unwrap_or_default()
                ));
            }
        }
        Err(reason) => lines.push(reason),
        Ok(None) => {}
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
    fn starting_folders_preserve_named_defaults_and_journal_fallback_reasons() {
        use exact_runner::file_pickers::Picker;
        let request = |start: Option<&str>| FileDialog {
            command: "showOpenFilePicker",
            picker: Picker {
                kind: Kind::Open,
                id: "result".into(),
                multiple: false,
                suggested_name: String::new(),
                start_in: start.map(str::to_owned),
            },
        };
        let mut lines = Vec::new();
        for name in exact_runner::file_pickers::WELL_KNOWN {
            start_folder(&request(Some(name)), &mut lines, |folder| {
                assert_eq!(folder, &StartFolder::Known(name.into()));
                Ok(())
            });
        }
        start_folder(&request(None), &mut lines, |_| panic!("no starting folder"));
        assert!(lines.is_empty());
        let reason = "path is too long for the Windows shell (MAX_PATH)";
        start_folder(&request(Some("pictures")), &mut lines, |_| {
            Err(reason.into())
        });
        assert_eq!(
            lines.pop().unwrap(),
            format!(
                "showOpenFilePicker: startIn: refused: pictures is not a folder here ({reason})"
            )
        );
        for (start, reason) in [
            ("elsewhere", "is not a folder here"),
            (
                "doc:/999999/missing",
                "is not a document this session holds",
            ),
        ] {
            start_folder(&request(Some(start)), &mut lines, |_| {
                panic!("an unresolved starting folder reached the shell")
            });
            assert_eq!(
                lines.pop().unwrap(),
                format!("showOpenFilePicker: startIn: refused: {start} {reason}")
            );
        }
    }

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
