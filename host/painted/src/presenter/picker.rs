//! `showPicker` and its answer on Linux (LLP 1069.002 D8, D9): refused with
//! `cancel` outside the agent (ruled: no desktop host yet); under the agent,
//! held for `tap @t cancel` or `type @t <path>…`, whose files are copied
//! into `app:/tmp/picked/` before `change` fires. `saveFile` likewise
//! (LLP 1069.010 D3): refused with `cancel`, or held as `export` for
//! `type @t <path>`, which the `app:/` file is copied to. The three
//! document pickers (LLP 1069.010 D2) ask the desktop portal's file chooser
//! on Linux (`portal.rs`), and are held for the agent as the others are.
//! Notifications have no centre here either: refused, or listed for the
//! agent. On a host with a shell (Android) the platform has every one of
//! these, and the channel to it (`requests.rs`, LLP 1115.001 D5) carries
//! file inputs as cache copies; the remaining refusals name the missing request.
use super::*;
use exact_runner::picker_support as support;
use exact_runner::ControlValue;

pub(crate) mod share;

#[derive(Clone, serde::Deserialize)]
struct PickedSource {
    path: PathBuf,
    name: String,
    #[serde(rename = "type")]
    mime: Option<String>,
    #[serde(default)]
    extension: Option<String>,
    #[serde(default)]
    converted: bool,
    /// Android's MIME for the display name's suffix, separate from the provider's type.
    #[serde(default, rename = "name_type")]
    name_mime: Option<String>,
    /// The Android chooser's MIME mapping for the input's extension tokens.
    #[serde(default)]
    extensions: std::collections::BTreeMap<String, String>,
}

pub(super) type Inbox = std::sync::Arc<std::sync::Mutex<Vec<(u32, Result<Copies, String>)>>>;

pub(super) struct Copies(Vec<exact_runner::Picked>);

impl Drop for Copies {
    fn drop(&mut self) {
        for file in &self.0 {
            if let Some(path) = crate::picker::resolve(&file.path) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

struct Import {
    sources: Vec<PickedSource>,
    targets: Result<Vec<(String, String)>, String>,
    remove_sources: bool,
}

impl Import {
    fn run(&self) -> Result<Copies, String> {
        let targets = self.targets.as_ref().map_err(Clone::clone)?;
        let mut files = Copies(Vec::new());
        for (source, (path, mime)) in self.sources.iter().zip(targets) {
            let mut file = crate::picker::copy_in(&source.path, path, mime)?;
            file.name = source.name.clone();
            files.0.push(file);
        }
        Ok(files)
    }
}

impl Drop for Import {
    fn drop(&mut self) {
        if !self.remove_sources {
            return;
        }
        for source in &self.sources {
            let _ = std::fs::remove_file(&source.path);
            if let Some(dir) = source.path.parent().filter(|dir| {
                dir.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("exact-picked-"))
            }) {
                let _ = std::fs::remove_dir(dir);
            }
        }
    }
}

impl<D: DataSource> Presenter<D> {
    /// `showPicker(id)`, from an action or a press on a visible file input.
    pub(crate) fn show_picker(&mut self, id: &str) {
        if self.has_shell() && !self.agent {
            if let Some(picker) = self.host.runner().picker(id) {
                self.request_files(picker);
            } else {
                self.host
                    .log(format!("picker: refused: no file input with id {id:?}"));
            }
            return;
        }
        if self.agent || crate::picker::agent() {
            let request = format!(
                "{{\"op\":\"showPicker\",\"id\":{}}}",
                serde_json::Value::from(id)
            );
            let _ = self.host.answer_hold(&request);
            self.dirty = true;
            return;
        }
        self.host.log("picker: refused: unavailable");
        self.not_carried("showPicker", "unavailable");
        if let Some(view) = self.host.runner().picker(id).map(|p| p.view) {
            self.deliver_picker(view, Event::Cancel);
        }
    }

    /// The agent's answer to a held picker, after the runner consumed it:
    /// `request` is the driver's line (its value never journalled), `reply`
    /// the runner's.
    pub(crate) fn answer_picker(&mut self, request: &str, reply: &str) {
        let r: serde_json::Value = serde_json::from_str(reply).unwrap_or_default();
        if r["capability"] != "pick" {
            return;
        }
        let Some(view) = r["node"].as_u64().map(|n| n as ViewId) else {
            return;
        };
        if r["answered"] == "cancel" {
            self.deliver_picker(view, Event::Cancel);
            return;
        }
        let q: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
        let value = q["text"].as_str().unwrap_or("");
        let paths = support::answer_paths(value)
            .into_iter()
            .map(|path| {
                let path = PathBuf::from(path);
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                PickedSource {
                    path,
                    name,
                    mime: None,
                    extension: None,
                    converted: false,
                    name_mime: None,
                    extensions: Default::default(),
                }
            })
            .collect::<Vec<_>>();
        let host = if self.has_shell() { "Android" } else { "Linux" };
        let result = self.copy_picked(view, &paths, host);
        self.finish_files(view, result);
    }

    /// Import work owns the staging paths, even if the input goes away before it completes.
    pub(super) fn files_chosen(
        &mut self,
        request: u32,
        view: Option<ViewId>,
        answer: Option<&str>,
    ) {
        let view = view.filter(|view| self.is_file_input(*view));
        let sources = match answer {
            None => Ok(Vec::new()),
            Some(text) => serde_json::from_str::<Option<Vec<PickedSource>>>(text)
                .map(Option::unwrap_or_default)
                .map_err(|_| text.to_owned()),
        };
        let mut sources = match sources {
            Ok(sources) if !sources.is_empty() => sources,
            result => {
                if let Some(view) = view {
                    self.finish_files(view, result.map(|_| Copies(Vec::new())));
                }
                return;
            }
        };
        for (index, source) in sources.iter_mut().enumerate() {
            let leaf = source
                .name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .replace('\0', "");
            source.name = if leaf.trim().is_empty() || matches!(leaf.as_str(), "." | "..") {
                format!("file-{index}")
            } else {
                leaf
            };
        }
        let targets = view.map_or_else(
            || Ok(Vec::new()),
            |view| self.picked_targets(view, &sources, "Android"),
        );
        let job = Import {
            sources,
            targets,
            remove_sources: true,
        };
        self.requests
            .waiting
            .insert(request, super::requests::Waiting::Files(view));
        self.requests.importing.insert(request);
        let (inbox, wake) = (self.requests.files.clone(), self.executor.waker());
        let started = std::thread::Builder::new()
            .name("exact-file-import".into())
            .spawn(move || {
                let result = job.run();
                drop(job);
                inbox
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((request, result));
                wake();
            });
        if let Err(error) = started {
            self.requests.waiting.remove(&request);
            self.requests.importing.remove(&request);
            if let Some(view) = view {
                self.finish_files(view, Err(error.to_string()));
            }
        }
    }

    pub(super) fn finish_file_imports(&mut self) {
        let done = std::mem::take(
            &mut *self
                .requests
                .files
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        for (request, result) in done {
            self.requests.importing.remove(&request);
            if let Some(super::requests::Waiting::Files(Some(view))) =
                self.requests.waiting.remove(&request)
            {
                self.finish_files(view, result);
            }
        }
    }

    fn is_file_input(&self, view: ViewId) -> bool {
        self.host.kernel().node(view).is_some_and(|node| {
            node.node_type == exact_kernel::NodeType::Control
                && node.props.str(PropId::Type) == Some("file")
        })
    }

    fn finish_files(&mut self, view: ViewId, result: Result<Copies, String>) {
        if !self.is_file_input(view) {
            return;
        }
        let event = match result {
            Ok(mut files) if !files.0.is_empty() => {
                let event = Event::Change(ControlValue::Files(files.0.clone()));
                let now = self.host.now();
                let advanced = self.host.runner_mut().dispatch_at_with_receipt(
                    view,
                    event,
                    now,
                    move |committed| {
                        if committed {
                            files.0.clear();
                        }
                    },
                );
                if let Some(error) = self.host.advanced(advanced).0 {
                    eprintln!("exact: {error}");
                }
                self.dirty = true;
                if let Some(error) = self.after_commit() {
                    eprintln!("exact: {error}");
                }
                return;
            }
            Ok(_) => {
                self.host.log("picker: cancelled");
                Event::Cancel
            }
            Err(reason) => {
                self.host.log(format!("picker: refused: {reason}"));
                Event::Cancel
            }
        };
        self.deliver_picker(view, event);
    }

    fn copy_picked(
        &mut self,
        view: ViewId,
        sources: &[PickedSource],
        host: &str,
    ) -> Result<Copies, String> {
        let targets = self.picked_targets(view, sources, host);
        let job = Import {
            sources: sources.to_vec(),
            targets,
            remove_sources: false,
        };
        job.run()
    }

    /// Mint app paths and validate metadata before starting a native import.
    fn picked_targets(
        &mut self,
        view: ViewId,
        sources: &[PickedSource],
        host: &str,
    ) -> Result<Vec<(String, String)>, String> {
        if !self.is_file_input(view) {
            return Err("the file input is gone".into());
        }
        let accept: Vec<String> = self
            .host
            .kernel()
            .node(view)
            .and_then(|n| n.props.str(PropId::Accept).map(str::to_owned))
            .unwrap_or_default()
            .split(',')
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        sources
            .iter()
            .map(|source| {
                let name = &source.name;
                let mime = source
                    .mime
                    .as_deref()
                    .filter(|mime| !mime.is_empty())
                    .unwrap_or_else(|| support::mime_for(name));
                let resolved: Vec<String> = accept
                    .iter()
                    .map(|token| {
                        source
                            .extensions
                            .get(token)
                            .filter(|_| token.starts_with('.'))
                            .unwrap_or(token)
                            .clone()
                    })
                    .collect();
                let delivered = support::delivered_type(mime, &resolved);
                // Conversion keeps the provider's HEIC name; validate the JPEG copy's suffix.
                let checked_name = if source.converted {
                    source
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(name)
                } else {
                    name
                };
                let suffix_mime = if source.converted {
                    support::mime_for(checked_name)
                } else {
                    source
                        .name_mime
                        .as_deref()
                        .unwrap_or_else(|| support::mime_for(name))
                };
                let mime_fallback = !checked_name.contains('.') || suffix_mime == delivered;
                if !support::accepts(&accept, checked_name, delivered)
                    && !(mime_fallback && support::accepts(&resolved, checked_name, delivered))
                {
                    return Err(format!(
                        "{name} ({mime}) is not among accept=\"{}\"",
                        accept.join(",")
                    ));
                }
                if delivered != mime {
                    return Err(format!("{mime} to JPEG is unavailable on {host}"));
                }
                let extension = source
                    .extension
                    .as_deref()
                    .filter(|e| {
                        !e.is_empty()
                            && e.len() <= 8
                            && e.bytes().all(|b| b.is_ascii_alphanumeric())
                    })
                    .map_or_else(|| support::extension_for(name, mime), str::to_owned);
                let path_name = format!("picked.{extension}");
                Ok((
                    self.host.runner_mut().picked_path(&path_name),
                    mime.to_owned(),
                ))
            })
            .collect()
    }

    /// `saveFile(id, from, suggestedName)`: the runner's rule, with no panel.
    pub(crate) fn save_file(&mut self, args: &[exact_plan::Value]) {
        use exact_runner::save_file::{arm, Arm, SaveFile};
        let request = SaveFile::from_args(args);
        let agent = crate::picker::agent();
        if let Arm::Refused(reason, view) = arm(self.host.runner_mut(), request, agent, false) {
            self.not_carried("saveFile", &reason);
            if let Some(view) = view {
                self.deliver_picker(view, Event::Cancel);
            }
        }
        self.dirty = true;
    }

    /// The agent's answer to a held export: copy the `app:/` file (or write
    /// the `text=`) to the driver's path and fire `change` with its name,
    /// or `cancel`.
    pub(crate) fn answer_save(&mut self, request: &str, reply: &str) {
        let r: serde_json::Value = serde_json::from_str(reply).unwrap_or_default();
        if r["capability"] != "export" {
            return;
        }
        let Some(view) = r["node"].as_u64().map(|n| n as ViewId) else {
            return;
        };
        if r["answered"] == "cancel" {
            self.host.log("saveFile: cancelled");
            self.deliver_picker(view, Event::Cancel);
            return;
        }
        let q: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
        let to = std::path::PathBuf::from(q["text"].as_str().unwrap_or("").trim());
        let copied = match r["request"]["text"].as_str() {
            Some(text) => std::fs::write(&to, text).map_err(|e| e.to_string()),
            None => r["request"]["from"]
                .as_str()
                .and_then(crate::picker::open)
                .ok_or_else(|| "no app file to copy".to_owned())
                .and_then(|mut from| {
                    let mut to = std::fs::File::create(&to).map_err(|e| e.to_string())?;
                    std::io::copy(&mut from, &mut to)
                        .map(drop)
                        .map_err(|e| e.to_string())
                }),
        };
        match copied {
            Ok(_) => {
                self.host.log("saveFile: saved");
                let name = exact_runner::save_file::chosen_name(&to.to_string_lossy()).to_owned();
                self.deliver_picker(view, Event::Change(ControlValue::Text(name)));
            }
            Err(e) => {
                self.host.log(format!("saveFile: refused: {e}"));
                self.deliver_picker(view, Event::Cancel);
            }
        }
    }

    /// `showOpenFilePicker`, `showDirectoryPicker`, `showSaveFilePicker`
    /// (LLP 1069.010 D2): the runner's rule, then the desktop portal's
    /// chooser on Linux; refused elsewhere.
    pub(crate) fn document_picker(&mut self, name: &str, args: &[exact_plan::Value]) {
        use exact_runner::file_pickers::{arm, Arm, Kind, Picker};
        let Some(kind) = Kind::of(name) else {
            return;
        };
        let request = Picker::from_args(kind, args);
        let agent = crate::picker::agent();
        let portal = cfg!(target_os = "linux") && !self.has_shell();
        match arm(self.host.runner_mut(), kind, request, agent, portal) {
            Arm::Refused(reason, view) => {
                self.not_carried(name, &reason);
                if let Some(view) = view {
                    self.deliver_picker(view, Event::Cancel);
                }
            }
            #[cfg(unix)]
            Arm::Present(request, view) => {
                let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
                self.ask_portal(request, view, address, crate::portal::TIMEOUT);
            }
            _ => {}
        }
        self.dirty = true;
    }

    /// Ask the portal on the session bus at `address` on a thread of its
    /// own; its outcome reaches the element at the next pump.
    #[cfg(unix)]
    pub(crate) fn ask_portal(
        &mut self,
        request: exact_runner::file_pickers::Picker,
        view: ViewId,
        address: Option<String>,
        timeout: std::time::Duration,
    ) {
        let (inbox, wake) = (self.portal.clone(), self.executor.waker());
        let kind = request.kind;
        let ask = {
            let mut asks = inbox.lock().unwrap_or_else(|e| e.into_inner());
            asks.next += 1;
            let ask = asks.next;
            asks.wanted.push((ask, view));
            ask
        };
        let asked = std::thread::Builder::new()
            .name("exact-portal".into())
            .spawn(move || {
                let outcome = crate::portal::choose(&request, address.as_deref(), timeout);
                (inbox.lock().unwrap_or_else(|e| e.into_inner()))
                    .answered
                    .push((ask, kind, outcome));
                wake();
            });
        if let Err(e) = asked {
            let mut asks = self.portal.lock().unwrap_or_else(|e| e.into_inner());
            asks.wanted.retain(|(a, _)| *a != ask);
            drop(asks);
            self.host.log(format!("{}: refused: {e}", kind.command()));
            self.deliver_picker(view, Event::Cancel);
        }
    }

    /// The outcomes the portal has answered since the last turn, each to the
    /// element it was asked for while that element stands.
    pub(super) fn finish_portal_picks(&mut self) {
        let done: Vec<_> = {
            let mut asks = self.portal.lock().unwrap_or_else(|e| e.into_inner());
            let answered = std::mem::take(&mut asks.answered);
            (answered.into_iter())
                .filter_map(|(ask, kind, outcome)| {
                    let at = asks.wanted.iter().position(|(a, _)| *a == ask)?;
                    Some((asks.wanted.remove(at).1, kind, outcome))
                })
                .collect()
        };
        for (view, kind, outcome) in done {
            let name = kind.command();
            match outcome {
                crate::portal::Outcome::Chosen(paths) => {
                    self.documents_chosen(view, name, paths.iter().map(|p| p.as_path()))
                }
                crate::portal::Outcome::Cancelled => {
                    self.host.log(format!("{name}: cancelled"));
                    self.deliver_picker(view, Event::Cancel);
                }
                crate::portal::Outcome::Refused(reason) => {
                    self.host.log(format!("{name}: refused: {reason}"));
                    self.deliver_picker(view, Event::Cancel);
                }
            }
        }
    }

    /// The agent's answer to a held picker: each path minted as a handle
    /// the person chose (D1), one per line in `change`, or `cancel`.
    pub(crate) fn answer_document(&mut self, request: &str, reply: &str) {
        let r: serde_json::Value = serde_json::from_str(reply).unwrap_or_default();
        let name = match r["capability"].as_str() {
            Some("open-file") => "showOpenFilePicker",
            Some("open-directory") => "showDirectoryPicker",
            Some("save-file") => "showSaveFilePicker",
            _ => return,
        };
        let Some(view) = r["node"].as_u64().map(|n| n as ViewId) else {
            return;
        };
        if r["answered"] == "cancel" {
            self.host.log(format!("{name}: cancelled"));
            self.deliver_picker(view, Event::Cancel);
            return;
        }
        let q: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
        let paths = exact_runner::file_pickers::answer_paths(q["text"].as_str().unwrap_or(""));
        self.documents_chosen(view, name, paths.into_iter().map(std::path::Path::new));
    }

    /// What the person chose for the picker `name` at `view`: each path
    /// minted as a handle (D1), one per line in `change`, or `cancel` when
    /// none mints.
    fn documents_chosen<'p>(
        &mut self,
        view: ViewId,
        name: &str,
        paths: impl Iterator<Item = &'p std::path::Path>,
    ) {
        let docs: Vec<String> = paths
            .filter_map(|p| {
                // Kept across launches where the app keeps documents (LLP 1069.010.000 D3).
                crate::kept::chosen(0, p).or_else(|| exact_data::documents::mint(p, 0))
            })
            .collect();
        if docs.is_empty() {
            self.host.log(format!("{name}: refused: nothing to open"));
            self.deliver_picker(view, Event::Cancel);
            return;
        }
        self.host.log(format!("{name}: chosen"));
        let value = ControlValue::Text(docs.join("\n"));
        self.deliver_picker(view, Event::Change(value));
    }

    /// `requestPermission(id, path, mode)` (LLP 1069.010.000 D7): the
    /// runner's rule, then `granted` at `id` for a document that resolves
    /// (the host owns no permission a person could withhold), else `denied`.
    pub(crate) fn request_permission(&mut self, args: &[exact_plan::Value]) {
        let arg = |i: usize| {
            args.get(i)
                .and_then(exact_plan::Value::as_str)
                .unwrap_or("")
        };
        let request = serde_json::json!({
            "command": "requestPermission", "id": arg(0), "path": arg(1), "mode": arg(2),
        });
        let ruling =
            exact_runner::file_pickers::request(self.host.runner_mut(), &request.to_string());
        let ruling: serde_json::Value = serde_json::from_str(&ruling).unwrap_or_default();
        self.dirty = true;
        let Some(view) = ruling["view"].as_u64() else {
            return;
        };
        let granted = exact_data::documents::resolve(arg(1), 0).is_ok();
        let answer = if granted { "granted" } else { "denied" };
        self.deliver_picker(
            view as ViewId,
            Event::Change(ControlValue::Text(answer.into())),
        );
    }

    fn deliver_picker(&mut self, view: ViewId, event: Event) {
        if self.host.kernel().node(view).is_none() {
            return;
        }
        let now = self.host.now();
        let error = self.host.dispatch_at(view, event, now);
        if let Some(e) = &error {
            eprintln!("exact: {e}");
        }
        self.dirty = true;
        if let Some(e) = self.after_commit() {
            eprintln!("exact: {e}");
        }
    }
}

#[cfg(test)]
#[test]
fn an_old_worker_cannot_overwrite_a_pick_after_runner_replacement() {
    let plan = crate::test_plan(
        "component App\n  view\n    input type=\"file\" id=\"attach\" accept=\"image/*\"\n",
    );
    let (mut p, _) = Presenter::boot_with(
        &plan,
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    let mut place = p.host.runner().place().clone();
    place.seed = 377999.;
    assert!(p.set_place(&place).is_none());
    let dir = std::env::temp_dir().join(format!("exact-overlap-worker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        PickedSource {
            path,
            name: name.into(),
            mime: Some("image/jpeg".into()),
            extension: None,
            converted: false,
            name_mime: None,
            extensions: Default::default(),
        }
    };
    let sources = vec![source("old.jpg", b"old")];
    let view = p.host.runner().picker("attach").unwrap().view;
    let job = Import {
        targets: p.picked_targets(view, &sources, "Android"),
        sources,
        remove_sources: true,
    };
    let (release, held) = std::sync::mpsc::channel();
    let old = std::thread::spawn(move || {
        held.recv().unwrap();
        job.run()
    });
    assert!(p.reload(&plan, ()).unwrap().is_none());
    let sources = vec![source("new.jpg", b"new")];
    let view = p.host.runner().picker("attach").unwrap().view;
    let job = Import {
        targets: p.picked_targets(view, &sources, "Android"),
        sources,
        remove_sources: true,
    };
    let new = job.run().unwrap();
    let new_path = crate::picker::resolve(&new.0[0].path).unwrap();
    release.send(()).unwrap();
    let stale = old.join().unwrap().unwrap();
    assert_ne!(stale.0[0].path, new.0[0].path);
    drop(stale);
    assert_eq!(std::fs::read(new_path).unwrap(), b"new");
    drop(new);
    drop(job);
    std::fs::remove_dir(dir).unwrap();
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use exact_kernel::{MonospaceMeasurer, TextMeasureRequest, TextMeasurer, TextMetrics};

    struct BadMetrics;
    impl TextMeasurer for BadMetrics {
        fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
            let mut metrics = MonospaceMeasurer::default().measure(request);
            if request.runs.iter().any(|run| run.text.starts_with("app:/")) {
                metrics.width = f32::NAN;
            }
            metrics
        }
    }

    fn fixture(mode: &str) -> (Presenter<()>, ViewId, Copies, PathBuf) {
        let task = match mode {
            "timer" | "agent" => "  task tick mount\n    every(frame, broken)\n",
            "deferred" => "  task sync key=number\n    change(changed)\n",
            _ => "",
        };
        let plan = crate::test_plan(&format!(
            r#"component App
  state path = ""
  state number = 0
  state ready = false
  action broken
    number = 1 / 0
  action changed
    ready = true
{task}  action picked(files: Picked[])
    path = files.map(f => f.path).join("")
    number = {}
  view
    column
      input type="file" id="attach" change=picked
      text path testId="path"
"#,
            if mode == "refused" { "1 / 0" } else { "1" }
        ));
        let (mut p, _) = Presenter::boot_with(
            &plan,
            (),
            (400., 800.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        if mode == "layout" {
            p.host = Host::boot(&plan, (), Box::new(BadMetrics), 400., 800.)
                .unwrap()
                .0;
        }
        static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(3778000);
        let mut place = p.host.runner().place().clone();
        place.seed = SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as f64;
        assert!(p.set_place(&place).is_none());
        if matches!(mode, "timer" | "agent") {
            p.host.runner_mut().present_frames(true);
            assert!(p.host.advance(20.).is_none());
            p.host.runner_mut().present_frames(false);
        }
        let view = p.host.runner().picker("attach").unwrap().view;
        let path = p.host.runner_mut().picked_path("photo.png");
        let file = crate::picker::resolve(&path).unwrap();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"picked bytes").unwrap();
        let picked = exact_runner::Picked {
            path,
            name: "photo.png".into(),
            mime: "image/png".into(),
            size: 12.,
            width: None,
            height: None,
            duration: None,
        };
        (p, view, Copies(vec![picked]), file)
    }

    #[test]
    fn file_copies_survive_a_timer_refusal_after_change_commits() {
        let (mut p, view, files, path) = fixture("timer");
        let published = files.0[0].path.clone();
        p.finish_files(view, Ok(files));
        assert!(p
            .host
            .runner()
            .journal()
            .any(|line| line.contains("NonFiniteValue")));
        assert_eq!(
            p.host.runner().slot("path"),
            Some(&exact_plan::Value::str(&published))
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"picked bytes");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn file_copies_survive_layout_failure_after_change_commits() {
        let (mut p, view, files, path) = fixture("layout");
        let published = files.0[0].path.clone();
        p.finish_files(view, Ok(files));
        assert_eq!(
            p.host.runner().slot("path"),
            Some(&exact_plan::Value::str(&published))
        );
        assert!(p.host.advance(0.).unwrap().contains("InvalidTextMetrics"));
        assert_eq!(std::fs::read(&path).unwrap(), b"picked bytes");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn file_copies_are_removed_when_change_itself_is_refused() {
        let (mut p, view, files, path) = fixture("refused");
        p.finish_files(view, Ok(files));
        assert!(p
            .host
            .runner()
            .journal()
            .any(|line| line.contains("NonFiniteValue")));
        assert_eq!(
            p.host.runner().slot("path"),
            Some(&exact_plan::Value::str(""))
        );
        assert!(!path.exists());
    }

    #[test]
    fn agent_file_copies_survive_a_timer_refusal_after_change_commits() {
        let (mut p, view, source, path) = fixture("agent");
        p.answer_picker(
            &serde_json::json!({"text": path}).to_string(),
            &serde_json::json!({"capability": "pick", "node": view, "answered": "files"})
                .to_string(),
        );
        let published = p.host.runner().slot("path").unwrap().as_str().unwrap();
        let imported = crate::picker::resolve(published).unwrap();
        assert_ne!(path, imported);
        assert_eq!(std::fs::read(&imported).unwrap(), b"picked bytes");
        std::fs::remove_file(imported).unwrap();
        drop(source);
        assert!(!path.exists());
    }

    #[test]
    fn deferred_file_copies_live_until_their_event_commits_or_is_dropped() {
        for ending in ["commit", "gone", "drop"] {
            let (mut p, view, files, path) = fixture("deferred");
            let published = files.0[0].path.clone();
            p.finish_files(view, Ok(files));
            assert_eq!(
                p.host.runner().slot("path"),
                Some(&exact_plan::Value::str(""))
            );
            assert!(path.exists(), "the deferred event still owns its files");
            if ending == "gone" {
                p.host
                    .apply_test_ops(&[exact_kernel::Op::DestroyView { id: view }]);
            }
            if ending != "drop" {
                let error = p.host.advance(0.);
                assert_eq!(error.is_some(), ending == "gone");
                if ending == "commit" {
                    assert_eq!(
                        p.host.runner().slot("path"),
                        Some(&exact_plan::Value::str(&published))
                    );
                }
            }
            drop(p);
            assert_eq!(path.exists(), ending == "commit");
            if ending == "commit" {
                std::fs::remove_file(path).unwrap();
            }
        }
    }
}
