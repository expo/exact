//! @ref LLP 1094.000 D2, D14 — a commit's drag data, rewritten before the
//! batch is sent to what this host carries, so a host classifies nothing.
//! `dataTransferEntries`, the files and names as the compiler binds them, is
//! decoded, and each file is admitted with its name, by the file-entry rule
//! and D14's name rule, into `dataTransferFiles` and `dataTransferNames`; the
//! `dataTransferFiles` a plan compiled before D14 binds is admitted alone; a
//! `url` that is not absolute `http:` or `https:` is refused, since no other
//! app can open it. A prop left with nothing is cleared. A refusal is
//! journaled once while it stands on its node, so a list changed beside it
//! does not repeat it.

use super::*;
use exact_kernel::{Op, PropId, PropValue, ViewId};

/// A node's admitted files, newline-joined, and their names, one line per
/// file, empty for a file's own; both empty for none.
pub(super) struct AdmittedFiles {
    pub files: String,
    pub names: String,
}

impl<D: DataSource> Runner<D> {
    pub(super) fn admit_drag_files(&mut self, ops: &mut Vec<Op>) {
        // Forget the refusals of nodes the batches before this one destroyed
        if !self.drag_refused.is_empty() {
            let kernel = &self.kernel;
            self.drag_refused.retain(|(id, _), _| kernel.node(*id).is_some());
        }
        let mut at = 0;
        while at < ops.len() {
            let (id, prop) = match &ops[at] {
                Op::SetProp { id, prop, .. } | Op::ClearProp { id, prop } => (*id, *prop),
                Op::CreateView { id, .. } => {
                    // A node made in this batch has refused nothing yet, whatever its id held before
                    self.drag_refused.retain(|(view, _), _| view != id);
                    at += 1;
                    continue;
                }
                _ => {
                    at += 1;
                    continue;
                }
            };
            let value = match &ops[at] {
                Op::SetProp { value: PropValue::Str(value), .. } => Some(value.to_string()),
                Op::ClearProp { .. } => None,
                _ => {
                    at += 1;
                    continue;
                }
            };
            match prop {
                PropId::DataTransferUrl => {
                    let (kept, refused) = match &value {
                        Some(value) => self.admitted_url(value),
                        None => (String::new(), Vec::new()),
                    };
                    self.journal_drag(id, true, refused);
                    ops[at] = set_or_clear(id, prop, kept);
                }
                PropId::DataTransferFiles => {
                    let (kept, refused) = match &value {
                        Some(value) => self.admitted_files(value),
                        None => (AdmittedFiles { files: String::new(), names: String::new() }, Vec::new()),
                    };
                    self.journal_drag(id, false, refused);
                    ops[at] = set_or_clear(id, prop, kept.files);
                }
                PropId::DataTransferEntries => {
                    let (kept, refused) = match &value {
                        Some(value) => self.admitted_entries(value),
                        None => (AdmittedFiles { files: String::new(), names: String::new() }, Vec::new()),
                    };
                    self.journal_drag(id, false, refused);
                    ops[at] = set_or_clear(id, PropId::DataTransferFiles, kept.files);
                    at += 1;
                    ops.insert(at, set_or_clear(id, PropId::DataTransferNames, kept.names));
                }
                _ => {}
            }
            at += 1;
        }
    }

    /// Journals the refusals of `id`'s url (`url`) or files that did not
    /// stand before, and keeps the ones that stand now.
    fn journal_drag(&mut self, id: ViewId, url: bool, refused: Vec<String>) {
        let before = self.drag_refused.remove(&(id, url)).unwrap_or_default();
        for why in refused.iter().filter(|why| !before.contains(why)) {
            self.log(format!("drag: refused: {why}"));
        }
        if !refused.is_empty() {
            self.drag_refused.insert((id, url), refused);
        }
    }

    /// The `url` this host carries, or none, and why it is refused.
    pub(super) fn admitted_url(&self, value: &str) -> (String, Vec<String>) {
        match exact_plan::http_url_refusal(value).filter(|_| !value.is_empty()) {
            None => (value.to_owned(), Vec::new()),
            // The URL itself stays out of the journal: it may hold a password or a token.
            Some(why) => (String::new(), vec![format!("url is not an absolute http: or https: URL: {why}")]),
        }
    }

    /// The newline-joined files a plan compiled before D14 binds, as this
    /// host carries them, and why each it leaves out is refused.
    pub(super) fn admitted_files(&self, value: &str) -> (AdmittedFiles, Vec<String>) {
        let files: Vec<String> = value.split('\n').filter(|e| !e.is_empty()).map(str::to_owned).collect();
        self.admitted_pairs(&files, None)
    }

    /// `dataTransferEntries`, decoded and admitted pair by pair.
    pub(super) fn admitted_entries(&self, value: &str) -> (AdmittedFiles, Vec<String>) {
        match exact_plan::decode_drag_entries(value) {
            Some((files, names)) => self.admitted_pairs(&files, names.as_deref()),
            None => (
                AdmittedFiles { files: String::new(), names: String::new() },
                vec!["the drag's files could not be read: rebuild the app with this Exact".into()],
            ),
        }
    }

    /// Each file this host carries with the name it is offered under, a
    /// refused file taking its name with it, and why each part left out is
    /// refused.
    fn admitted_pairs(&self, files: &[String], names: Option<&[String]>) -> (AdmittedFiles, Vec<String>) {
        let web = self.platform == crate::page::Platform::Web;
        let (mut kept, mut offered, mut refused) = (Vec::new(), Vec::new(), Vec::new());
        for (at, file) in files.iter().enumerate() {
            let name = names.and_then(|names| names.get(at)).map(String::as_str).unwrap_or("");
            if file.is_empty() {
                continue;
            }
            // The admitted files reach the hosts one per line
            let admitted = if file.contains(['\r', '\n']) {
                Err(format!("{file:?}: a file with a line break cannot be carried"))
            } else {
                crate::transfer::drag_file(self, file, web)
            };
            if let Err(why) = admitted {
                refused.push(why);
                continue;
            }
            kept.push(file.as_str());
            offered.push(match offered_name(file, name) {
                Ok(name) => name,
                Err(why) => {
                    refused.push(why);
                    String::new()
                }
            });
        }
        for name in names.unwrap_or_default().iter().skip(files.len()) {
            refused.push(format!("name {name:?}: there is no file in files= for it"));
        }
        let names = if offered.iter().all(String::is_empty) { String::new() } else { offered.join("\n") };
        (AdmittedFiles { files: kept.join("\n"), names }, refused)
    }
}

/// The name `file` is offered under for `name`, empty for its own, or why
/// the name is refused: as written and as offered (LLP 1094.000 D14).
fn offered_name(file: &str, name: &str) -> Result<String, String> {
    if name.is_empty() {
        return Ok(String::new());
    }
    let offered = exact_plan::offered_drag_name(file, name);
    match exact_plan::drag_name_refusal(name).or_else(|| exact_plan::drag_name_refusal(&offered)) {
        None => Ok(offered),
        Some(why) => Err(format!("name {name:?}: {why}")),
    }
}

fn set_or_clear(id: ViewId, prop: PropId, value: String) -> Op {
    if value.is_empty() {
        Op::ClearProp { id, prop }
    } else {
        Op::SetProp { id, prop, value: PropValue::Str(value.into()) }
    }
}
