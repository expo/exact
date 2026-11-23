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
            let (kept, refused) = self.admitted_drag(prop, value);
            for why in refused {
                self.log(format!("drag: refused: {why}"));
            }
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

    /// A drag prop's value as this host carries it, and why each part it
    /// leaves out is refused.
    pub(super) fn admitted_drag(&self, prop: PropId, value: &str) -> (String, Vec<String>) {
        let mut refused = Vec::new();
        if prop == PropId::DataTransferUrl {
            let kept = match exact_plan::http_url_refusal(value).filter(|_| !value.is_empty()) {
                None => value.to_owned(),
                Some(why) => {
                    // The URL itself stays out of the journal: it may hold a password or a token.
                    refused.push(format!("url is not an absolute http: or https: URL: {why}"));
                    String::new()
                }
            };
            return (kept, refused);
        }
        let web = self.platform == crate::page::Platform::Web;
        let kept: Vec<&str> = value
            .split('\n')
            .filter(|e| !e.is_empty())
            .filter(|e| match crate::transfer::drag_file(self, e, web) {
                Ok(()) => true,
                Err(why) => {
                    refused.push(why);
                    false
                }
            })
            .collect();
        (kept.join("\n"), refused)
    }
}
