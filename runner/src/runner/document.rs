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
            for prop in [PropId::DataTransferFiles, PropId::DataTransferUrl] {
                let Some(value) = node.props.str(prop).map(str::to_owned) else {
                    continue;
                };
                match self.admitted_drag(prop, &value).0 {
                    kept if kept.is_empty() => {
                        node.props.remove(prop);
                    }
                    kept => {
                        node.props.set(prop, PropValue::Str(kept.into()));
                    }
                }
            }
        }
        Ok(doc)
    }
}
