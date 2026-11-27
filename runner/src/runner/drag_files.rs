//! @ref LLP 1094.000 D2, D14 — a commit's drag data, rewritten before the
//! batch is sent to what this host carries, so a host classifies nothing.
//! `dataTransferEntries`, the files and names as the compiler binds them, is
//! decoded, and each file is admitted with its name, by the file-entry rule
//! and D14's name rule, into `dataTransferFiles` and `dataTransferNames`; the
//! `dataTransferFiles` a plan compiled before D14 binds is admitted alone; a
//! `url` that is not absolute `http:` or `https:` is refused, since no other
//! app can open it. A prop left with nothing is cleared. A refusal is
//! journaled once while it stands on its node: each is known by what was
//! refused, not by its message, so a list changed beside it does not repeat
//! it, and a second refusal of the same kind is journaled as its own.

use super::*;
use exact_kernel::{Op, PropId, PropValue, ViewId};

/// A node's admitted files, newline-joined, and their names, one line per
/// file, empty for a file's own; both empty for none.
#[derive(Default)]
pub(super) struct AdmittedFiles {
    pub files: String,
    pub names: String,
}

/// A refusal: what was refused, which tells one refusal from another, and
/// the journal's line for it, which may leave that out (a URL may hold a
/// password or a token).
pub(super) struct Refusal {
    what: String,
    line: String,
}

impl<D: DataSource> Runner<D> {
    pub(super) fn admit_drag_files(&mut self, ops: &mut Vec<Op>) {
        // Forget the refusals of nodes the batches before this one destroyed
        if !self.drag_refused.is_empty() {
            let kernel = &self.kernel;
            self.drag_refused.retain(|(id, _), _| kernel.node(*id).is_some());
        }
        // Each node's names, written after its files once the pass is done
        let mut names_after = Vec::new();
        for at in 0..ops.len() {
            let (id, prop, value) = match &ops[at] {
                Op::CreateView { id, .. } => {
                    // A node made in this batch has refused nothing yet, whatever its id held before
                    self.drag_refused.remove(&(*id, true));
                    self.drag_refused.remove(&(*id, false));
                    continue;
                }
                Op::SetProp { id, prop, value } if is_drag(*prop) => match value {
                    PropValue::Str(value) => (*id, *prop, Some(&**value)),
                    _ => continue,
                },
                Op::ClearProp { id, prop } if is_drag(*prop) => (*id, *prop, None),
                _ => continue,
            };
            let (replaced, names) = match prop {
                PropId::DataTransferUrl => {
                    let (kept, refused) = value.map(|v| self.admitted_url(v)).unwrap_or_default();
                    self.journal_drag(id, true, refused);
                    (set_or_clear(id, prop, kept), None)
                }
                PropId::DataTransferFiles => {
                    let (kept, refused) = value.map(|v| self.admitted_files(v)).unwrap_or_default();
                    self.journal_drag(id, false, refused);
                    (set_or_clear(id, prop, kept.files), None)
                }
                _ => {
                    let (kept, refused) = value.map(|v| self.admitted_entries(v)).unwrap_or_default();
                    self.journal_drag(id, false, refused);
                    let names = set_or_clear(id, PropId::DataTransferNames, kept.names);
                    (set_or_clear(id, PropId::DataTransferFiles, kept.files), Some(names))
                }
            };
            ops[at] = replaced;
            if let Some(names) = names {
                names_after.push((at, names));
            }
        }
        if names_after.is_empty() {
            return;
        }
        let before = std::mem::take(ops);
        ops.reserve(before.len() + names_after.len());
        let mut names = names_after.into_iter().peekable();
        for (at, op) in before.into_iter().enumerate() {
            ops.push(op);
            if let Some((_, op)) = names.next_if(|(after, _)| *after == at) {
                ops.push(op);
            }
        }
    }

    /// Journals the refusals of `id`'s url (`url`) or files that did not
    /// stand before, matched one for one by what was refused, and keeps the
    /// ones that stand now.
    fn journal_drag(&mut self, id: ViewId, url: bool, refused: Vec<Refusal>) {
        let before = self.drag_refused.remove(&(id, url)).unwrap_or_default();
        // How many of each stood before, spent as the same ones stand again
        let mut stood = std::collections::HashMap::<&str, usize>::new();
        for what in &before {
            *stood.entry(what).or_default() += 1;
        }
        let mut now = Vec::with_capacity(refused.len());
        for Refusal { what, line } in refused {
            match stood.get_mut(what.as_str()).filter(|left| **left > 0) {
                Some(left) => *left -= 1,
                None => self.log(format!("drag: refused: {line}")),
            }
            now.push(what);
        }
        if !now.is_empty() {
            self.drag_refused.insert((id, url), now);
        }
    }

    /// The `url` this host carries, or none, and why it is refused.
    pub(super) fn admitted_url(&self, value: &str) -> (String, Vec<Refusal>) {
        match exact_plan::http_url_refusal(value).filter(|_| !value.is_empty()) {
            None => (value.to_owned(), Vec::new()),
            // The URL itself stays out of the journal: it may hold a password or a token.
            Some(why) => (
                String::new(),
                vec![Refusal { what: value.to_owned(), line: format!("url is not an absolute http: or https: URL: {why}") }],
            ),
        }
    }

    /// The newline-joined files a plan compiled before D14 binds, as this
    /// host carries them, and why each it leaves out is refused.
    pub(super) fn admitted_files(&self, value: &str) -> (AdmittedFiles, Vec<Refusal>) {
        let files: Vec<String> = value.split('\n').filter(|e| !e.is_empty()).map(str::to_owned).collect();
        self.admitted_pairs(&files, None)
    }

    /// `dataTransferEntries`, decoded and admitted pair by pair.
    pub(super) fn admitted_entries(&self, value: &str) -> (AdmittedFiles, Vec<Refusal>) {
        match exact_plan::decode_drag_entries(value) {
            Some((files, names)) => self.admitted_pairs(&files, names.as_deref()),
            None => (
                AdmittedFiles::default(),
                vec![Refusal {
                    what: value.to_owned(),
                    line: "the drag's files could not be read: rebuild the app with this Exact".into(),
                }],
            ),
        }
    }

    /// Each file this host carries with the name it is offered under, a
    /// refused file taking its name with it, and why each part left out is
    /// refused.
    fn admitted_pairs(&self, files: &[String], names: Option<&[String]>) -> (AdmittedFiles, Vec<Refusal>) {
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
            if let Err(line) = admitted {
                refused.push(Refusal { what: format!("file\0{file}"), line });
                continue;
            }
            kept.push(file.as_str());
            offered.push(match offered_name(file, name) {
                Ok(name) => name,
                Err(line) => {
                    refused.push(Refusal { what: format!("name\0{file}\0{name}"), line });
                    String::new()
                }
            });
        }
        for name in names.unwrap_or_default().iter().skip(files.len()) {
            refused.push(Refusal {
                what: format!("surplus\0{name}"),
                line: format!("name {name:?}: there is no file in files= for it"),
            });
        }
        let names = if offered.iter().all(String::is_empty) { String::new() } else { offered.join("\n") };
        (AdmittedFiles { files: kept.join("\n"), names }, refused)
    }
}

/// Whether the commit pass admits `prop`.
fn is_drag(prop: PropId) -> bool {
    matches!(prop, PropId::DataTransferUrl | PropId::DataTransferFiles | PropId::DataTransferEntries)
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
