//! EXACT PATCH 79: the grid algorithm reached through a registered pointer
//! (LLP 1047.001 D10).
//!
//! [`TaffyTree`](super::TaffyTree)'s dispatch calls [`compute_grid_layout`]
//! only through [`LINKED`], which [`link_grid`] sets. A binary that never
//! calls it (an Apple app whose plan lays out no grid) names none of the
//! algorithm, and the linker drops it. The algorithm sees every tree view
//! through one type, [`GridView`], so it is compiled once, however many
//! measure functions a host lays out with.

use std::sync::OnceLock;

use super::taffy_tree::{TaffyTreeChildIter, TaffyView};
use crate::compute::compute_grid_layout;
#[cfg(feature = "detailed_layout_info")]
use crate::compute::grid::DetailedGridInfo;
use crate::geometry::Point;
use crate::style::Style;
use crate::sys::DefaultCheapStr;
use crate::tree::{
    Layout, LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree, NodeId, StaticPosition,
    TraversePartialTree,
};

/// The linked algorithm.
pub(crate) type GridLayout = fn(&mut GridView<'_>, NodeId, LayoutInput) -> LayoutOutput;

/// Set by [`link_grid`].
static LINKED: OnceLock<GridLayout> = OnceLock::new();

/// Link the grid algorithm into every tree's dispatch, for the process.
/// [`TaffyTree::new`](super::TaffyTree::new) links it; a tree made with
/// [`TaffyTree::by_use`](super::TaffyTree::by_use) lays out a grid only once
/// something has. Unlinked, a grid container is laid out as a block, which
/// a host that links by use never asks for: it refuses a plan with a grid
/// before laying anything out.
pub fn link_grid() {
    let _ = LINKED.set(layout);
}

fn layout(tree: &mut GridView<'_>, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
    compute_grid_layout(tree, node, inputs)
}

/// The linked algorithm, if one is.
#[inline(always)]
pub(crate) fn linked() -> Option<GridLayout> {
    LINKED.get().copied()
}

/// What the grid algorithm asks of the tree it lays out, as an object.
pub(crate) trait GridTree {
    fn child_ids(&self, node: NodeId) -> TaffyTreeChildIter<'_>;
    fn child_count(&self, node: NodeId) -> usize;
    fn get_child_id(&self, node: NodeId, index: usize) -> NodeId;
    /// A container's or an item's style, read as the view reads it.
    fn style(&self, node: NodeId) -> &Style;
    fn resolve_calc_value(&self, val: *const (), basis: f32) -> f32;
    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout);
    fn invalidate_final_layout(&mut self, node: NodeId);
    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput;
    fn set_static_position(&mut self, node: NodeId, position: StaticPosition);
    fn set_grid_origin(&mut self, node: NodeId, origin: Point<f32>);
    fn set_multicol_bounds(&mut self, node: NodeId, min: Option<f32>, max: Option<f32>);
    fn hoisted_absolute_count(&self, node: NodeId) -> usize;
    fn hoisted_absolute(&self, node: NodeId, index: usize) -> Option<(NodeId, StaticPosition, Point<f32>)>;
    #[cfg(feature = "detailed_layout_info")]
    fn set_detailed_grid_info(&mut self, node: NodeId, info: DetailedGridInfo);
}

/// A tree view, as the grid algorithm lays it out.
pub(crate) struct GridView<'a>(pub(crate) &'a mut dyn GridTree);

impl TraversePartialTree for GridView<'_> {
    type ChildIter<'b>
        = TaffyTreeChildIter<'b>
    where
        Self: 'b;

    fn child_ids(&self, node: NodeId) -> Self::ChildIter<'_> {
        self.0.child_ids(node)
    }

    fn child_count(&self, node: NodeId) -> usize {
        self.0.child_count(node)
    }

    fn get_child_id(&self, node: NodeId, index: usize) -> NodeId {
        self.0.get_child_id(node, index)
    }
}

impl LayoutPartialTree for GridView<'_> {
    type CoreContainerStyle<'b>
        = &'b Style
    where
        Self: 'b;

    type CustomIdent = DefaultCheapStr;

    fn get_core_container_style(&self, node: NodeId) -> Self::CoreContainerStyle<'_> {
        self.0.style(node)
    }

    fn resolve_calc_value(&self, val: *const (), basis: f32) -> f32 {
        self.0.resolve_calc_value(val, basis)
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        self.0.set_unrounded_layout(node, layout)
    }

    fn invalidate_final_layout(&mut self, node: NodeId) {
        self.0.invalidate_final_layout(node)
    }

    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.0.compute_child_layout(node, inputs)
    }

    fn set_static_position(&mut self, node: NodeId, position: StaticPosition) {
        self.0.set_static_position(node, position)
    }

    fn set_grid_origin(&mut self, node: NodeId, origin: Point<f32>) {
        self.0.set_grid_origin(node, origin)
    }

    fn set_multicol_bounds(&mut self, node: NodeId, min: Option<f32>, max: Option<f32>) {
        self.0.set_multicol_bounds(node, min, max)
    }

    fn hoisted_absolute_count(&self, node: NodeId) -> usize {
        self.0.hoisted_absolute_count(node)
    }

    fn hoisted_absolute(&self, node: NodeId, index: usize) -> Option<(NodeId, StaticPosition, Point<f32>)> {
        self.0.hoisted_absolute(node, index)
    }
}

impl LayoutGridContainer for GridView<'_> {
    type GridContainerStyle<'b>
        = &'b Style
    where
        Self: 'b;
    type GridItemStyle<'b>
        = &'b Style
    where
        Self: 'b;

    fn get_grid_container_style(&self, node: NodeId) -> Self::GridContainerStyle<'_> {
        self.0.style(node)
    }

    fn get_grid_child_style(&self, child: NodeId) -> Self::GridItemStyle<'_> {
        self.0.style(child)
    }

    #[cfg(feature = "detailed_layout_info")]
    fn set_detailed_grid_info(&mut self, node: NodeId, info: DetailedGridInfo) {
        self.0.set_detailed_grid_info(node, info)
    }
}

impl<NodeContext, MeasureFunction> GridTree for TaffyView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    fn child_ids(&self, node: NodeId) -> TaffyTreeChildIter<'_> {
        TraversePartialTree::child_ids(self, node)
    }

    fn child_count(&self, node: NodeId) -> usize {
        TraversePartialTree::child_count(self, node)
    }

    fn get_child_id(&self, node: NodeId, index: usize) -> NodeId {
        TraversePartialTree::get_child_id(self, node, index)
    }

    fn style(&self, node: NodeId) -> &Style {
        self.get_grid_container_style(node)
    }

    fn resolve_calc_value(&self, val: *const (), basis: f32) -> f32 {
        LayoutPartialTree::resolve_calc_value(self, val, basis)
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        LayoutPartialTree::set_unrounded_layout(self, node, layout)
    }

    fn invalidate_final_layout(&mut self, node: NodeId) {
        LayoutPartialTree::invalidate_final_layout(self, node)
    }

    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
        LayoutPartialTree::compute_child_layout(self, node, inputs)
    }

    fn set_static_position(&mut self, node: NodeId, position: StaticPosition) {
        LayoutPartialTree::set_static_position(self, node, position)
    }

    fn set_grid_origin(&mut self, node: NodeId, origin: Point<f32>) {
        LayoutPartialTree::set_grid_origin(self, node, origin)
    }

    fn set_multicol_bounds(&mut self, node: NodeId, min: Option<f32>, max: Option<f32>) {
        LayoutPartialTree::set_multicol_bounds(self, node, min, max)
    }

    fn hoisted_absolute_count(&self, node: NodeId) -> usize {
        LayoutPartialTree::hoisted_absolute_count(self, node)
    }

    fn hoisted_absolute(&self, node: NodeId, index: usize) -> Option<(NodeId, StaticPosition, Point<f32>)> {
        LayoutPartialTree::hoisted_absolute(self, node, index)
    }

    #[cfg(feature = "detailed_layout_info")]
    fn set_detailed_grid_info(&mut self, node: NodeId, info: DetailedGridInfo) {
        LayoutGridContainer::set_detailed_grid_info(self, node, info)
    }
}
