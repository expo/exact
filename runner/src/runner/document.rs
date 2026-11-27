//! The document of a runner that mirrors no kernel (LLP 1048.004).
use super::{DataSource, Runner};
use crate::instance::{DocTree, DocTreeError};
use exact_kernel::{PropId, PropValue};

impl<D: DataSource> Runner<D> {
    /// The current tree's document nodes, folded from its bindings' values:
    /// what a render writes when its kernel is detached
    /// (`exact_kernel::Kernel::detached`). A tree the fold doesn't cover, or
    /// one a kernel would have refused, is the error: render it with a
    /// kernel instead.
    pub fn document_tree(&mut self) -> Result<DocTree, DocTreeError> {
        let _roles = self.bind_roles();
        let mut doc = self
            .tree
            .as_ref()
            .ok_or(DocTreeError::Unsupported("a runner with no tree"))?
            .document(&self.plan, &self.sites, &self.ids.theme, self.fonts.clone())?;
        self.fold_live_times(&mut doc);
        // A drag carries what a commit admits (`admit_drag_files`); a
        // detached runner builds no prop op, so nothing journals a refusal.
        for node in doc.nodes_mut() {
            let mut admitted = Vec::new();
            if let Some(url) = node.props.str(PropId::DataTransferUrl) {
                admitted.push((PropId::DataTransferUrl, self.admitted_url(url).0));
            }
            let files = match (
                node.props.str(PropId::DataTransferEntries),
                node.props.str(PropId::DataTransferFiles),
            ) {
                (Some(entries), _) => Some(self.admitted_entries(entries).0),
                (None, Some(files)) => Some(self.admitted_files(files).0),
                (None, None) => None,
            };
            if let Some(files) = files {
                admitted.push((PropId::DataTransferFiles, files.files));
                admitted.push((PropId::DataTransferNames, files.names));
            }
            node.props.remove(PropId::DataTransferEntries);
            for (prop, kept) in admitted {
                if kept.is_empty() {
                    node.props.remove(prop);
                } else {
                    node.props.set(prop, PropValue::Str(kept.into()));
                }
            }
        }
        Ok(doc)
    }
}
