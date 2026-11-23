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
    /// The file types the dialog offers, one filter each; none offers every
    /// file.
    pub types: Vec<FileType>,
}

/// A file type the manifest declares in `file_handlers`, as the binary's
/// compatibility record carries it. A document picker offers one filter per
/// type, labelled with the handler's `name` (LLP 1069.010 D2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileType {
    /// The handler's `name`, when it has one.
    pub name: Option<String>,
    /// Canonical suffixes: a leading dot, lowercase (`.md`, `.tar.gz`).
    pub suffixes: Vec<String>,
    /// The MIME types it declares.
    pub mimes: Vec<String>,
}

/// The file types in a compatibility record's `inputs.fileHandlers`, in the
/// manifest's order. A directory declaration is the directory picker's, and a
/// handler declaring nothing else offers no type. A record without
/// `fileHandlers` (no handlers, or a binary from before they were embedded)
/// declares none, as does a presenter booted with no record. The build wrote
/// canonical suffixes, so one that is not is refused rather than repaired.
pub fn declared_types(compat: &str) -> Result<Vec<FileType>, String> {
    if compat.is_empty() {
        return Ok(Vec::new());
    }
    let json = serde_json::from_str::<serde_json::Value>(compat)
        .map_err(|error| format!("compatibility metadata is not JSON: {error}"))?;
    let Some(handlers) = json.pointer("/inputs/fileHandlers") else {
        return Ok(Vec::new());
    };
    let handlers = handlers
        .as_array()
        .ok_or_else(|| "compatibility metadata's fileHandlers is not an array".to_owned())?;
    let mut types = Vec::new();
    for (handler_index, handler) in handlers.iter().enumerate() {
        let accept = handler
            .get("accept")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| format!("file handler {handler_index}'s accept is not an object"))?;
        let (mut suffixes, mut mimes) = (Vec::new(), Vec::new());
        for (mime, extensions) in accept {
            if mime == "inode/directory" {
                continue;
            }
            mimes.push(mime.clone());
            let extensions = extensions.as_array().ok_or_else(|| {
                format!("file handler {handler_index}'s extensions for {mime} are not an array")
            })?;
            for (extension_index, extension) in extensions.iter().enumerate() {
                match extension.as_str().filter(|s| canonical_suffix(s)) {
                    Some(suffix) => suffixes.push(suffix.to_owned()),
                    None => {
                        return Err(format!(
                            "file handler {handler_index}'s extension {extension_index} is invalid"
                        ))
                    }
                }
            }
        }
        if mimes.is_empty() {
            continue;
        }
        let name = match handler.get("name") {
            Some(name) => Some(
                name.as_str()
                    .filter(|name| !name.is_empty() && !name.chars().any(char::is_control))
                    .ok_or_else(|| {
                        format!("file handler {handler_index}'s name is not a valid label")
                    })?
                    .to_owned(),
            ),
            None => None,
        };
        types.push(FileType {
            name,
            suffixes,
            mimes,
        });
    }
    Ok(types)
}

/// The File System Access API's suffix, as the manifest readers write it.
fn canonical_suffix(value: &str) -> bool {
    value.starts_with('.')
        && !value.ends_with('.')
        && value.len() <= 16
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'+' | b'.')
        })
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

#[derive(Debug)]
pub(super) struct Waiting {
    pub(super) view: Option<ViewId>,
    route: Route,
    source: Option<Export>,
    live: std::sync::Arc<()>,
}

#[derive(Debug)]
pub(super) enum Export {
    Text(String),
    File(std::fs::File),
}

impl Export {
    pub(super) fn open(from: &str, text: Option<&str>) -> Result<Self, String> {
        match text {
            Some(text) => Ok(Self::Text(text.to_owned())),
            None => crate::picker::open(from)
                .map(Self::File)
                .ok_or_else(|| "no app file to copy".into()),
        }
    }

    pub(super) fn write(self, to: &std::path::Path) -> Result<(), String> {
        if to.to_str().is_none() {
            return Err("a chosen path is not Unicode".into());
        }
        // All views and windows serialize truncation and copying through this lock.
        static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _write = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        let copy = || -> std::io::Result<()> {
            let mut destination = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(to)?;
            if let Self::File(from) = &self {
                let source = same_file::Handle::from_file(from.try_clone()?)?;
                let chosen = same_file::Handle::from_file(destination.try_clone()?)?;
                if source == chosen {
                    return Ok(());
                }
            }
            destination.set_len(0)?;
            match self {
                Self::Text(text) => {
                    use std::io::Write;
                    destination.write_all(text.as_bytes())?;
                }
                Self::File(mut from) => {
                    std::io::copy(&mut from, &mut destination)?;
                }
            }
            Ok(())
        };
        copy().map_err(|error| error.to_string())
    }
}

pub(super) type Inbox = std::sync::Arc<std::sync::Mutex<Vec<(u32, PathBuf, Result<(), String>)>>>;

impl<D: DataSource> Presenter<D> {
    /// The window has file dialogs. Headless hosts leave them unavailable.
    pub fn serve_file_dialogs(&mut self) {
        self.requests.file_dialogs = true;
    }

    pub(super) fn queue_file_dialog(&mut self, dialog: FileDialog, view: ViewId, route: Route) {
        if self.requests.waiting.values().any(|wait| match wait {
            requests::Waiting::FileDialog(w) => w.view == Some(view),
            requests::Waiting::Export(v) => *v == Some(view),
            _ => false,
        }) {
            self.host.log(format!("picker: {view} is already open"));
            return;
        }
        // Timers can replace the name while the dialog is open; keep the opened file.
        let source = if let Route::Export(save) = &route {
            match Export::open(&save.from, save.text.as_deref()) {
                Ok(source) => Some(source),
                Err(reason) => {
                    self.host.log(format!("saveFile: refused: {reason}"));
                    self.deliver_picker(view, Event::Cancel);
                    return;
                }
            }
        } else {
            None
        };
        let id = self.request(Request::FileDialog(dialog));
        self.requests.waiting.insert(
            id,
            requests::Waiting::FileDialog(Waiting {
                view: Some(view),
                route,
                source,
                live: std::sync::Arc::new(()),
            }),
        );
    }

    /// Take desktop dialogs from the shared request queue, once each.
    /// Showing them must happen after this borrow has ended and only while
    /// the returned weak lifetime still has an owner.
    pub fn take_file_dialogs(&mut self) -> Vec<(u32, FileDialog, std::sync::Weak<()>)> {
        let mut dialogs = Vec::new();
        self.requests.queued.retain(|(id, request)| {
            if let Request::FileDialog(dialog) = request {
                if let Some(requests::Waiting::FileDialog(wait)) = self.requests.waiting.get(id) {
                    dialogs.push((*id, dialog.clone(), std::sync::Arc::downgrade(&wait.live)));
                }
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
            Route::Export(_) => {
                self.export_chosen(id, view, paths[0].clone(), waiting.source.unwrap())
            }
        }
    }

    fn export_chosen(&mut self, id: u32, view: ViewId, to: PathBuf, source: Export) {
        if to.to_str().is_none() {
            self.finish_save(view, &to, Err("a chosen path is not Unicode".into()));
            return;
        }
        self.requests
            .waiting
            .insert(id, requests::Waiting::Export(Some(view)));
        let (inbox, wake) = (self.requests.exports.clone(), self.executor.waker());
        let failed_to = to.clone();
        let started = std::thread::Builder::new()
            .name("exact-file-export".into())
            .spawn(move || {
                let copied = source.write(&to);
                inbox
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((id, to, copied));
                wake();
            });
        match started {
            Ok(worker) => {
                self.requests.export_workers.insert(id, worker);
            }
            Err(error) => {
                self.requests.waiting.remove(&id);
                self.finish_save(view, &failed_to, Err(error.to_string()));
            }
        }
    }

    pub(super) fn finish_file_exports(&mut self) {
        let done = std::mem::take(
            &mut *self
                .requests
                .exports
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        for (id, to, copied) in done {
            if let Some(worker) = self.requests.export_workers.remove(&id) {
                if worker.join().is_err() {
                    self.host.log("saveFile: refused: export worker panicked");
                }
            }
            if let Some(requests::Waiting::Export(Some(view))) = self.requests.waiting.remove(&id) {
                self.finish_save(view, &to, copied);
            } else if let Err(reason) = copied {
                self.host.log(format!("saveFile: refused: {reason}"));
            }
        }
    }

    /// Finish accepted exports before the window host exits; return failures to report.
    pub fn drain_file_exports(&mut self) -> Vec<String> {
        let mut failures = Vec::new();
        for (id, worker) in std::mem::take(&mut self.requests.export_workers) {
            if worker.join().is_err() {
                failures.push("saveFile: refused: export worker panicked".to_owned());
            }
            self.requests.waiting.remove(&id);
        }
        let done = std::mem::take(
            &mut *self
                .requests
                .exports
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        for (_, _, copied) in done {
            if let Err(reason) = copied {
                failures.push(format!("saveFile: refused: {reason}"));
            }
        }
        for failure in &failures {
            self.host.log(failure);
        }
        failures
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

#[cfg(test)]
#[path = "file_dialog_tests.rs"]
mod tests;
