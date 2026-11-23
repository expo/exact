//! Windows common item dialogs (LLP 1173 D1–D2).
use exact_runner::file_pickers::Kind;

#[cfg(windows)]
mod os;
#[cfg(windows)]
pub(super) use os::Dialogs;

use exact_painted::presenter::file_dialog::{FileDialog, FileType, Outcome, StartFolder};
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

/// One entry of the dialog's file-type list (`COMDLG_FILTERSPEC`).
#[derive(Debug, PartialEq)]
struct Filter {
    /// What the list shows: the handler's name and, as Windows' own apps show
    /// it, its patterns: `Documents (*.pdf)`.
    label: String,
    /// The patterns, which Windows matches without regard to case.
    patterns: Vec<String>,
}

impl Filter {
    fn spec(&self) -> String {
        self.patterns.join(";")
    }
}

/// The dialog's file types: one per declared type, in the manifest's order
/// and nothing beside them, as the other hosts' pickers offer only the
/// declared types. A type declared by MIME alone matches the extension
/// Windows registers for that MIME type (`extension`); a type with neither is
/// left out, and its MIME types are added to `unmatched`.
fn filters(
    types: &[FileType],
    extension: impl Fn(&str) -> Option<String>,
    unmatched: &mut Vec<String>,
) -> Vec<Filter> {
    types
        .iter()
        .filter_map(|t| {
            let suffixes = if t.suffixes.is_empty() {
                t.mimes.iter().filter_map(|mime| extension(mime)).collect()
            } else {
                t.suffixes.clone()
            };
            let patterns: Vec<String> = suffixes.iter().map(|s| format!("*{s}")).collect();
            if patterns.is_empty() {
                unmatched.extend(t.mimes.iter().cloned());
                return None;
            }
            let spec = patterns.join(";");
            let label = match &t.name {
                Some(name) => format!("{name} ({spec})"),
                None => spec,
            };
            Some(Filter { label, patterns })
        })
        .collect()
}

/// The save dialog's starting type, 1-based as `SetFileTypeIndex` takes it,
/// and the extension Windows adds to a name typed without one: the type with
/// the longest pattern that ends the suggested name (`.tar.gz` over `.gz`),
/// else the first.
fn save_type(filters: &[Filter], suggested: &str) -> (u32, String) {
    let name = suggested.to_lowercase();
    let (index, pattern) = filters
        .iter()
        .enumerate()
        .flat_map(|(i, f)| f.patterns.iter().map(move |p| (i, p)))
        .filter(|(_, p)| name.ends_with(&p[1..]))
        .max_by_key(|(i, p)| (p.len(), std::cmp::Reverse(*i)))
        .unwrap_or((0, &filters[0].patterns[0]));
    (index as u32 + 1, pattern[2..].to_owned())
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
            types: Vec::new(),
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
            types: Vec::new(),
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

    fn declared(name: Option<&str>, suffixes: &[&str], mimes: &[&str]) -> FileType {
        FileType {
            name: name.map(str::to_owned),
            suffixes: suffixes.iter().map(|s| s.to_string()).collect(),
            mimes: mimes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn each_declared_type_is_one_labelled_filter_and_nothing_else() {
        let types = [
            declared(Some("Documents"), &[".pdf"], &["application/pdf"]),
            declared(Some("Notes"), &[".md", ".markdown"], &["text/markdown"]),
            declared(Some("Gzip"), &[".gz"], &["application/gzip"]),
            declared(None, &[".tar.gz"], &["application/gzip"]),
            declared(Some("Pictures"), &[], &["image/png", "image/x-unknown"]),
            declared(Some("Nothing here"), &[], &["application/x-unregistered"]),
        ];
        let extension = |mime: &str| (mime == "image/png").then(|| ".png".to_owned());
        let mut unmatched = Vec::new();
        let filters = filters(&types, extension, &mut unmatched);
        assert_eq!(unmatched, ["application/x-unregistered"]);
        let shown: Vec<_> = filters
            .iter()
            .map(|f| (f.label.as_str(), f.spec()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Documents (*.pdf)", "*.pdf".to_owned()),
                ("Notes (*.md;*.markdown)", "*.md;*.markdown".to_owned()),
                ("Gzip (*.gz)", "*.gz".to_owned()),
                ("*.tar.gz", "*.tar.gz".to_owned()),
                ("Pictures (*.png)", "*.png".to_owned()),
            ]
        );
        assert!(super::filters(&[], extension, &mut unmatched).is_empty());
        for (suggested, expected) in [
            ("Receipt.PDF", (1, "pdf")),
            ("notes.markdown", (2, "markdown")),
            ("Backup.tar.gz", (4, "tar.gz")),
            ("log.gz", (3, "gz")),
            ("Untitled", (1, "pdf")),
            ("", (1, "pdf")),
            (".md", (2, "md")),
            ("md", (1, "pdf")),
        ] {
            let (index, extension) = save_type(&filters, suggested);
            assert_eq!((index, extension.as_str()), expected, "{suggested:?}");
        }
    }
}
