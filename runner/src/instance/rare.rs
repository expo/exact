//! A realized node's state that few nodes have, behind one pointer: a
//! `Child` is 112 bytes, a size class of Apple's allocator, and most child
//! vectors hold one.

use super::document::ListChild;
use super::{Child, InstanceError, InstanceStep, NodeInst, Update};
use crate::vm::Frame;
use exact_kernel::ViewId;
use exact_plan::{NodesId, Plan, Value};
use std::any::Any;

/// A [`NodeInst`]'s state that few nodes have.
#[derive(Debug, Default, Clone)]
pub(super) struct Rare {
    /// A virtualized list's engine, which only the linked list engine makes
    /// (`ListLinks.create`, LLP 1047.000 §9).
    pub(super) collection: Option<Box<dyn ListState>>,
    /// Last published surface inputs (a canvas node).
    pub(super) last_surface: Option<Vec<Value>>,
    /// A mode boundary's implied rows, by [`super::theme::INHERITED`] (D7).
    pub(super) implied: Option<[Option<Value>; 8]>,
    /// The bindings, by `BindingsId`, whose last evaluation trapped
    /// (`trapped.rs`): unset and journaled once.
    pub(super) trapped: Vec<u32>,
}

/// What the core asks of a list node's engine. Only the engine implements
/// it, so a binary without lists holds none of its drop, clone or debug
/// code: they are this trait's vtable, made where the engine boxes one.
pub(in crate::instance) trait ListState: std::fmt::Debug {
    /// A copy, for a cloned tree (a render checkpoint).
    fn clone_box(&self) -> Box<dyn ListState>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Whether `view` is in one of its mounted rows.
    fn contains(&self, view: ViewId) -> bool;
    /// The frame and roots of the mounted row whose wrapper is `view`.
    fn row_by_wrapper(&self, view: ViewId) -> Option<(&Frame, &[Child])>;
    /// `view`'s site within a mounted row, its row steps pushed on `path`.
    fn site(&self, view: ViewId, path: &mut Vec<InstanceStep>) -> Option<(NodesId, u32)>;
    fn reading(&self) -> Option<super::collection::ListReading>;
    /// The mounted rows with the loop that owns their keys.
    fn visit_rows(&self, visit: &mut dyn FnMut(exact_plan::RegionsId, &super::Row));
    /// Its view's children as a document written without a kernel holds them.
    fn document_children<'a>(&'a self, plan: &Plan) -> Result<Vec<ListChild<'a>>, InstanceError>;
    /// Retire its mounted rows with the rest of the subtree.
    fn retire(&self, u: &mut Update<'_>);
}

impl Clone for Box<dyn ListState> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl NodeInst {
    pub(super) fn rare(&mut self) -> &mut Rare {
        self.rare.get_or_insert_with(Box::default)
    }

    /// Its list engine, when it is a virtualized list.
    pub(super) fn list(&self) -> Option<&dyn ListState> {
        self.rare.as_ref()?.collection.as_deref()
    }

    pub(super) fn set_list(&mut self, made: Option<Box<dyn ListState>>) {
        if made.is_some() || self.rare.is_some() {
            self.rare().collection = made;
        }
    }
}
