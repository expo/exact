//! A list's engine as its node's [`ListState`]: the one place a
//! [`Collection`] is boxed as the core holds it, and taken back out.
use super::*;
use crate::instance::document::ListChild;
use crate::instance::rare::{ListState, Rare};
use std::any::Any;

impl ListState for Collection {
    fn clone_box(&self) -> Box<dyn ListState> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn contains(&self, view: ViewId) -> bool {
        Collection::contains(self, view)
    }
    fn row_by_wrapper(&self, view: ViewId) -> Option<(&Frame, &[Child])> {
        Collection::row_by_wrapper(self, view).map(|row| (&row.frame, row.roots.as_slice()))
    }
    fn site(&self, view: ViewId, path: &mut Vec<InstanceStep>) -> Option<(NodesId, u32)> {
        Collection::site(self, view, path)
    }
    fn reading(&self) -> Option<ListReading> {
        self.capture_reading()
    }
    fn visit_rows(&self, visit: &mut dyn FnMut(RegionsId, &Row)) {
        for (_, row) in self.mounted_rows() {
            visit(self.region, row);
        }
    }
    fn fixed_split(&self) -> Option<usize> {
        self.fixed.as_ref().map(|f| f.split)
    }
    fn document_children<'a>(&'a self, plan: &Plan) -> Result<Vec<ListChild<'a>>, InstanceError> {
        Collection::document_children(self, plan)
    }
    fn retire(&self, u: &mut Update<'_>) {
        self.retire_rows(u)
    }
}

/// [`Collection::create`], boxed as the core holds it ([`super::super::LISTS`]).
pub(in crate::instance) fn create_state(
    u: &mut Update<'_>,
    node: NodesId,
    view: ViewId,
    frames: &[Frame],
    children: &mut Vec<Child>,
) -> Result<Option<Box<dyn ListState>>, InstanceError> {
    Ok(Collection::create(u, node, view, frames, children)?.map(|c| c as Box<dyn ListState>))
}

/// [`update_collection`] on a node's list, in the modes around it now.
pub(in crate::instance) fn update_state(
    list: &mut dyn ListState,
    u: &mut Update<'_>,
    data: DataUpdate<'_>,
) -> Result<(), InstanceError> {
    let c = list
        .as_any_mut()
        .downcast_mut::<Collection>()
        .ok_or_else(|| {
            InstanceError::Collection("a list's state is not the list engine's".into())
        })?;
    c.around = theme::Around {
        retheme: false,
        ..u.theme.clone()
    };
    update_collection(c, u, data)
}

impl NodeInst {
    /// Its list engine, when it is a virtualized list.
    pub(in crate::instance::collection) fn collection(&self) -> Option<&Collection> {
        self.list()?.as_any().downcast_ref()
    }
    pub(in crate::instance::collection) fn collection_mut(&mut self) -> Option<&mut Collection> {
        list_mut(&mut self.rare)
    }
}

/// A node's list engine, borrowing only its [`Rare`].
pub(in crate::instance::collection) fn list_mut(
    rare: &mut Option<Box<Rare>>,
) -> Option<&mut Collection> {
    rare.as_mut()?
        .collection
        .as_deref_mut()?
        .as_any_mut()
        .downcast_mut()
}
