//! Desktop file dialogs, handed to the window without borrowing the presenter
//! across a native modal loop (LLP 1173 D1–D2).
use super::*;
use exact_runner::file_pickers::{Kind, Picker};
use exact_runner::save_file::SaveFile;

/// A dialog the window shows on its own thread.
#[derive(Clone, Debug, PartialEq)]
pub struct FileDialog {
    /// The command, used for startIn refusals.
    pub command: &'static str,
    /// The dialog's kind, suggested name, selection mode and starting folder.
    pub picker: Picker,
}

/// What the person chose, or why the window could not show the dialog.
#[derive(Debug)]
pub enum Outcome {
    /// Native paths, borrowed from the person even for a file input.
    Chosen(Vec<PathBuf>),
    /// The person dismissed the dialog.
    Cancelled,
    /// No dialog could be shown or its result could not be read.
    Refused(String),
}

/// The two Windows starting-folder policies (LLP 1173 D2).
#[derive(Debug, PartialEq)]
pub enum StartFolder {
    /// A default, used only when Windows has no remembered folder.
    Known(String),
    /// An explicit choice, which overrides the remembered folder.
    Chosen(PathBuf),
}

impl FileDialog {
    /// Resolve a held document without exposing an arbitrary native path.
    pub fn start_folder(&self) -> Result<Option<StartFolder>, String> {
        let Some(start) = &self.picker.start_in else {
            return Ok(None);
        };
        let why = if exact_runner::file_pickers::WELL_KNOWN.contains(&start.as_str()) {
            return Ok(Some(StartFolder::Known(start.clone())));
        } else if start.starts_with(exact_data::documents::PREFIX) {
            if let Some(folder) = exact_data::documents::folder(start, 0) {
                return Ok(Some(StartFolder::Chosen(folder)));
            }
            "is not a document this session holds"
        } else {
            "is not a folder here"
        };
        Err(format!("{}: startIn: refused: {start} {why}", self.command))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Route {
    Document(Kind),
    Import,
    Export(SaveFile),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Waiting {
    pub(super) view: Option<ViewId>,
    route: Route,
}

impl<D: DataSource> Presenter<D> {
    /// The window has file dialogs. Headless hosts leave them unavailable.
    pub fn serve_file_dialogs(&mut self) {
        self.requests.file_dialogs = true;
    }

    pub(super) fn queue_file_dialog(&mut self, dialog: FileDialog, view: ViewId, route: Route) {
        if self
            .requests
            .waiting
            .values()
            .any(|wait| matches!(wait, requests::Waiting::FileDialog(w) if w.view == Some(view)))
        {
            self.host.log(format!("picker: {view} is already open"));
            return;
        }
        let id = self.request(Request::FileDialog(dialog));
        self.requests.waiting.insert(
            id,
            requests::Waiting::FileDialog(Waiting {
                view: Some(view),
                route,
            }),
        );
    }

    /// Take desktop dialogs from the shared request queue, once each.
    /// Showing them must happen after this borrow has ended.
    pub fn take_file_dialogs(&mut self) -> Vec<(u32, FileDialog)> {
        let mut dialogs = Vec::new();
        self.requests.queued.retain(|(id, request)| {
            if let Request::FileDialog(dialog) = request {
                dialogs.push((*id, dialog.clone()));
                false
            } else {
                true
            }
        });
        dialogs
    }

    /// Consume a queued dialog's answer on the presenter's next turn.
    /// A renewed or replaced input receives no stale result.
    pub fn answer_file_dialog(&mut self, id: u32, outcome: Outcome) {
        if let Some(requests::Waiting::FileDialog(waiting)) = self.requests.waiting.remove(&id) {
            self.finish_file_dialog(id, waiting, outcome);
        }
    }

    pub(super) fn finish_file_dialog(&mut self, id: u32, waiting: Waiting, outcome: Outcome) {
        let Some(view) = waiting
            .view
            .filter(|view| self.host.kernel().node(*view).is_some())
        else {
            return;
        };
        let command = match &waiting.route {
            Route::Document(kind) => kind.command(),
            Route::Import => "picker",
            Route::Export(_) => "saveFile",
        };
        let paths = match outcome {
            Outcome::Chosen(paths) if !paths.is_empty() => paths,
            outcome => {
                self.host.log(match outcome {
                    Outcome::Refused(reason) => format!("{command}: refused: {reason}"),
                    _ => format!("{command}: cancelled"),
                });
                self.deliver_picker(view, Event::Cancel);
                return;
            }
        };
        match waiting.route {
            Route::Document(_) => {
                self.documents_chosen(view, command, paths.iter().map(|p| p.as_path()))
            }
            Route::Import => self.borrowed_files_chosen(id, view, paths),
            Route::Export(save) => {
                self.save_chosen(view, &paths[0], &save.from, save.text.as_deref())
            }
        }
    }
}

#[cfg(test)]
#[test]
fn an_agent_holds_dialogs_even_with_a_native_window() {
    use exact_plan::Value;
    let plan = crate::test_plan("component App\n  view\n    input id=\"result\"\n");
    for command in [
        "showOpenFilePicker",
        "showDirectoryPicker",
        "showSaveFilePicker",
        "saveFile",
    ] {
        let mut p = Presenter::boot_with(
            &plan,
            (),
            (400., 300.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap()
        .0;
        p.serve_file_dialogs();
        if command == "saveFile" {
            p.save_file(&[
                Value::str("result"),
                Value::Option(None),
                Value::str("Shot.txt"),
                Value::str("bytes"),
                Value::str("screenshots"),
            ]);
        } else {
            let mut args = vec![Value::str("result")];
            match command {
                "showOpenFilePicker" => args.push(Value::Bool(true)),
                "showSaveFilePicker" => args.push(Value::str("Shot.txt")),
                _ => {}
            }
            args.push(Value::str("screenshots"));
            p.document_picker(command, &args);
        }
        assert!(p.take_file_dialogs().is_empty());
        let state = crate::agent::handle(&mut p, r#"{"op":"state"}"#);
        assert!(state.contains("screenshots"), "{command}: {state}");
        assert!(
            p.host
                .runner()
                .journal()
                .any(|line| line.contains("held (agent)")),
            "{state}"
        );
    }
}
