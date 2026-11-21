//! @ref LLP 1094.000 D2 — a commit's drag data, rewritten before the batch
//! is sent to what this host carries, so a host classifies nothing: each
//! file by the file-entry rule, and a `url` that is not absolute `http:` or
//! `https:`, which no other app can open. A prop left with nothing is
//! cleared. Each refusal is journaled as its prop is written: when the
//! binding's value changes or its node is built.

use super::*;
use exact_kernel::{Op, PropId, PropValue};

impl<D: DataSource> Runner<D> {
    pub(super) fn admit_drag_files(&mut self, ops: &mut [Op]) {
        let web = self.platform == crate::page::Platform::Web;
        for op in ops.iter_mut() {
            let Op::SetProp {
                id,
                prop: prop @ (PropId::DataTransferFiles | PropId::DataTransferUrl),
                value: PropValue::Str(value),
            } = op
            else {
                continue;
            };
            let (id, prop) = (*id, *prop);
            let kept = if prop == PropId::DataTransferUrl {
                let url = value.to_string();
                match exact_plan::http_url_refusal(&url).filter(|_| !url.is_empty()) {
                    None => url,
                    Some(why) => {
                        // The URL itself stays out of the journal: it may hold a password or a token.
                        self.log(format!(
                            "drag: refused: url is not an absolute http: or https: URL: {why}"
                        ));
                        String::new()
                    }
                }
            } else {
                let entries: Vec<String> = value
                    .split('\n')
                    .filter(|e| !e.is_empty())
                    .map(String::from)
                    .collect();
                let mut kept = Vec::with_capacity(entries.len());
                for entry in entries {
                    match crate::transfer::drag_file(self, &entry, web) {
                        Ok(()) => kept.push(entry),
                        Err(reason) => self.log(format!("drag: refused: {reason}")),
                    }
                }
                kept.join("\n")
            };
            *op = if kept.is_empty() {
                Op::ClearProp { id, prop }
            } else {
                Op::SetProp {
                    id,
                    prop,
                    value: PropValue::Str(kept.into()),
                }
            };
        }
    }
}
