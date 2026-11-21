//! The list engine's half of the list seam (LLP 1047.001 D11): each method
//! reaches the engine, which no core function names.
use super::list_seam::Lists;
use super::*;
use crate::list_api::{
    CollectionEdges, CollectionFeedback, CollectionFill, CollectionSnapshot, ListTextPolicy,
    ListTextPosition, ReorderBinding, ReorderFrame, ReorderGeometry, ReorderProgress, ReorderStart,
    ReorderStep, ReorderToken,
};
use exact_kernel::{Kernel, NodeKey, Op};

/// The list engine, for a runner whose entry links lists.
pub(crate) struct LinkedLists;

impl<D: DataSource> Lists<D> for LinkedLists {
    fn linked(&self) -> bool {
        true
    }
    fn collections(&self, tree: &Tree, rows: usize) -> Vec<CollectionSnapshot> {
        match rows {
            1 => tree.collections_shallow(),
            _ => tree.collections(),
        }
    }
    fn collection(&self, tree: &Tree, view: ViewId) -> Option<CollectionSnapshot> {
        tree.collection(view)
    }
    fn collection_mounted(&self, tree: &Tree, view: ViewId, out: &mut Vec<(ViewId, u64)>) {
        tree.collection_mounted(view, out)
    }
    fn collection_data(&self, tree: &Tree) -> Vec<(ViewId, u64, bool)> {
        tree.collection_data()
    }
    fn collections_bounded(
        &self,
        tree: &Tree,
        [collections, rows, traversal, json_bytes]: [usize; 4],
    ) -> Result<Vec<CollectionSnapshot>, &'static str> {
        tree.collections_bounded(collections, rows, traversal, json_bytes)
    }
    fn collection_travel(
        &self,
        tree: &mut Tree,
        view: ViewId,
        (revision, sequence, offset): (u64, u64, f64),
    ) {
        tree.collection_travel(view, revision, sequence, offset)
    }
    fn update_collection(
        &self,
        tree: &mut Tree,
        u: &mut Update<'_>,
        feedback: CollectionFeedback,
        fill: CollectionFill,
    ) -> Result<(bool, Option<CollectionEdges>), InstanceError> {
        tree.update_collection(u, feedback, fill)
    }
    fn collection_edges(
        &self,
        r: &mut Runner<D>,
        view: ViewId,
        edges: CollectionEdges,
        result: &mut Advanced,
    ) {
        r.collection_edges_linked(view, edges, result)
    }
    fn show_collection(&self, tree: &mut Tree, u: &mut Update<'_>, view: ViewId, offset: f64) {
        tree.show_collection(u, view, offset)
    }
    fn release_held_edges(&self, r: &mut Runner<D>) -> Result<(), RunnerError> {
        r.release_held_edges_linked()
    }
    fn wake_deferred_edges(&self, r: &mut Runner<D>) -> Result<(), RunnerError> {
        r.wake_deferred_edges_linked()
    }
    fn collection_snap(
        &self,
        tree: &Tree,
        u: &mut Update<'_>,
        kernel: &Kernel,
        view: ViewId,
        geometry: [f64; 5],
    ) -> Option<f64> {
        tree.collection_snap(u, kernel, view, geometry)
            .ok()
            .flatten()
    }
    fn collection_snap_areas(
        &self,
        tree: &Tree,
        u: &mut Update<'_>,
        kernel: &Kernel,
        view: ViewId,
        around: (f64, [f64; 2], usize),
        snapport: [f64; 3],
    ) -> Vec<(f64, f64, bool)> {
        tree.collection_snap_areas(u, kernel, view, around, snapport)
            .unwrap_or_default()
    }
    fn collection_snap_stop(
        &self,
        tree: &Tree,
        u: &mut Update<'_>,
        kernel: &Kernel,
        view: ViewId,
        geometry: [f64; 5],
    ) -> Option<f64> {
        tree.collection_snap_stop(u, kernel, view, geometry)
            .ok()
            .flatten()
    }
    fn list_sections(
        &self,
        tree: &Tree,
        u: &mut Update<'_>,
        view: ViewId,
    ) -> Result<Option<crate::list_api::SectionTable>, crate::instance::InstanceError> {
        tree.list_sections(u, view)
    }
    fn list_index(&self, tree: &Tree, view: ViewId, key: &str) -> Option<usize> {
        tree.list_index(view, key)
    }
    fn list_text(
        &self,
        tree: &Tree,
        u: &mut Update<'_>,
        view: ViewId,
        range: Option<(ListTextPosition<'_>, ListTextPosition<'_>)>,
        policy: ListTextPolicy,
    ) -> Result<String, InstanceError> {
        tree.list_text(u, view, range, policy)
    }
    fn reorder_binding(
        &self,
        tree: &Tree,
        kernel: &Kernel,
        list: ViewId,
        handle: NodeKey,
    ) -> Option<ReorderBinding> {
        tree.reorder_collection(list)?
            .reorder_binding(kernel, handle)
    }
    fn reorder_geometry(
        &self,
        tree: &Tree,
        view: ViewId,
        list: NodeKey,
    ) -> Option<ReorderGeometry> {
        tree.reorder_collection(view)?.reorder_geometry(list)
    }
    fn begin_reorder(
        &self,
        r: &mut Runner<D>,
        binding: ReorderBinding,
        geometry: ReorderGeometry,
        group: Option<bool>,
    ) -> Result<Option<ReorderStart>, RunnerError> {
        match group {
            None => r.begin_session(binding, geometry, None, false),
            Some(ghost) => r.begin_group_reorder_linked(binding, geometry, ghost),
        }
    }
    fn has_reorder(&self, r: &Runner<D>, token: ReorderToken) -> bool {
        r.has_reorder_linked(token)
    }
    fn preview_reorder(
        &self,
        r: &mut Runner<D>,
        token: ReorderToken,
        geometry: ReorderGeometry,
        y: f64,
    ) -> Result<ReorderProgress, RunnerError> {
        r.preview_reorder_linked(token, geometry, y)
    }
    fn preview_reorder_into(
        &self,
        r: &mut Runner<D>,
        token: ReorderToken,
        target: NodeKey,
        geometry: ReorderGeometry,
        y: f64,
    ) -> Result<ReorderProgress, RunnerError> {
        r.preview_reorder_into_linked(token, target, geometry, y)
    }
    fn drop_reorder(
        &self,
        r: &mut Runner<D>,
        token: ReorderToken,
        geometry: ReorderGeometry,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        r.drop_reorder_now(token, geometry)
    }
    fn cancel_reorder(
        &self,
        r: &mut Runner<D>,
        token: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        r.cancel_reorder_linked(token)
    }
    fn reorder_frame(&self, r: &Runner<D>, token: ReorderToken) -> Option<ReorderFrame> {
        r.reorder_frame_linked(token)
    }
    fn finish_reorder(
        &self,
        r: &mut Runner<D>,
        token: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        r.finish_reorder_linked(token)
    }
    fn reorder_step(
        &self,
        r: &mut Runner<D>,
        token: ReorderToken,
        step: ReorderStep,
    ) -> Result<Vec<CommitReceipt>, RunnerError> {
        r.reorder_step_linked(token, step)
    }
    fn reorder_json(&self, r: &Runner<D>) -> String {
        r.reorder_json_linked()
    }
    fn reconcile_reorder(&self, r: &mut Runner<D>) -> Result<Vec<Op>, RunnerError> {
        let ops = r.reconcile_reorder_linked()?;
        r.sync_reorder_target();
        Ok(ops)
    }
    fn reorder_deadline(&self, r: &Runner<D>) -> Option<f64> {
        r.reorder_deadline_linked()
    }
    fn reorder_timeout(&self, r: &mut Runner<D>) -> Result<CommitReceipt, RunnerError> {
        r.reorder_timeout_linked()
    }
}
