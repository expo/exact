//! The list seam (LLP 1047.001 D11; LLP 1047.000 §9): the runner's list
//! work behind [`Lists`], which only an entry's [`super::ModuleLinks`] names,
//! so a binary that does not link lists holds none of the engine.
//!
//! The hosts' list methods stay here as fronts. Each keeps what a runner
//! with no list does that anyone can observe (its `poisoned` check, argument
//! checks and errors, the `Update` it builds) and asks the seam only for the
//! part that finds a list or holds a list's state. A provided method is the
//! answer of that part when the tree holds no list.
use super::*;
use crate::list_api::{
    CollectionEdges, CollectionFeedback, CollectionFill, CollectionSnapshot, ListTextPolicy,
    ListTextPosition, ReorderBinding, ReorderFrame, ReorderGeometry, ReorderProgress, ReorderStart,
    ReorderStep, ReorderToken,
};
use exact_kernel::{Display, Kernel, NodeKey, NodeType, Op, PropId};

/// The runner's list work. The provided methods are what the runner answers
/// with no list; `LinkedLists` (`lists.rs`) implements every one.
pub trait Lists<D: DataSource> {
    /// Whether this is the list engine; the boot runs the instance tree's
    /// list table only beside it.
    fn linked(&self) -> bool {
        false
    }
    /// Each mounted list's snapshot, with at most `rows` mounted rows.
    fn collections(&self, _: &Tree, _rows: usize) -> Vec<CollectionSnapshot> {
        Vec::new()
    }
    /// List `view`'s snapshot.
    fn collection(&self, _: &Tree, _view: ViewId) -> Option<CollectionSnapshot> {
        None
    }
    /// List `view`'s mounted rows into `out`.
    fn collection_mounted(&self, _: &Tree, _view: ViewId, _out: &mut Vec<(ViewId, u64)>) {}
    /// Each mounted list's view, data generation and axis.
    fn collection_data(&self, _: &Tree) -> Vec<(ViewId, u64, bool)> {
        Vec::new()
    }
    /// [`Runner::collections_bounded`]'s walk.
    fn collections_bounded(
        &self,
        _: &Tree,
        _limits: [usize; 4],
    ) -> Result<Vec<CollectionSnapshot>, &'static str> {
        Ok(Vec::new())
    }
    /// A host's latest travel of list `view`.
    fn collection_travel(&self, _: &mut Tree, _view: ViewId, _travel: (u64, u64, f64)) {}
    /// A host's report on list `view`: whether it changed, and the edges it
    /// reached.
    fn update_collection(
        &self,
        tree: &mut Tree,
        u: &mut Update<'_>,
        _: CollectionFeedback,
        _: CollectionFill,
    ) -> Result<(bool, Option<CollectionEdges>), crate::instance::InstanceError> {
        tree.last_work = u.work;
        Ok((false, None))
    }
    /// The edges a report reached, offered to list `view`'s handlers.
    fn collection_edges(
        &self,
        _: &mut Runner<D>,
        _view: ViewId,
        _: CollectionEdges,
        _: &mut Advanced,
    ) {
    }
    /// List `view`'s port is at `offset`.
    fn show_collection(&self, _: &mut Tree, _: &mut Update<'_>, _view: ViewId, _offset: f64) {}
    /// The edges waiting under a covered route, offered once it shows.
    fn release_held_edges(&self, _: &mut Runner<D>) -> Result<(), RunnerError> {
        Ok(())
    }
    /// The edges deferred until a settled commit, woken.
    fn wake_deferred_edges(&self, _: &mut Runner<D>) -> Result<(), RunnerError> {
        Ok(())
    }
    /// [`Runner::collection_snap`]'s search.
    fn collection_snap(
        &self,
        _: &Tree,
        _: &mut Update<'_>,
        _: &Kernel,
        _view: ViewId,
        _geometry: [f64; 5],
    ) -> Option<f64> {
        None
    }
    /// [`Runner::collection_snap_areas`]'s search.
    fn collection_snap_areas(
        &self,
        _: &Tree,
        _: &mut Update<'_>,
        _: &Kernel,
        _view: ViewId,
        _around: (f64, [f64; 2], usize),
        _snapport: [f64; 3],
    ) -> Vec<(f64, f64, bool)> {
        Vec::new()
    }
    /// [`Runner::collection_snap_stop`]'s search.
    fn collection_snap_stop(
        &self,
        _: &Tree,
        _: &mut Update<'_>,
        _: &Kernel,
        _view: ViewId,
        _geometry: [f64; 5],
    ) -> Option<f64> {
        None
    }
    /// Headers and body rows in the authored sections of a virtualized list.
    fn list_sections(
        &self,
        _: &Tree,
        _: &mut Update<'_>,
        _: ViewId,
    ) -> Result<Option<crate::list_api::SectionTable>, crate::instance::InstanceError> {
        Ok(None)
    }
    /// Row `key`'s logical index in list `view`.
    fn list_index(&self, _: &Tree, _view: ViewId, _key: &str) -> Option<usize> {
        None
    }
    /// List `view`'s logical text.
    fn list_text(
        &self,
        _: &Tree,
        _: &mut Update<'_>,
        _view: ViewId,
        _range: Option<(ListTextPosition<'_>, ListTextPosition<'_>)>,
        _policy: ListTextPolicy,
    ) -> Result<String, crate::instance::InstanceError> {
        Err(crate::instance::InstanceError::List("unknown list"))
    }
    /// The reorder binding list `list` gives `handle`.
    fn reorder_binding(
        &self,
        _: &Tree,
        _: &Kernel,
        _list: ViewId,
        _handle: NodeKey,
    ) -> Option<ReorderBinding> {
        None
    }
    /// List `list`'s reorder geometry.
    fn reorder_geometry(&self, _: &Tree, _view: ViewId, _list: NodeKey) -> Option<ReorderGeometry> {
        None
    }
    /// [`Runner::begin_reorder`], or a grouped session's start.
    fn begin_reorder(
        &self,
        _: &mut Runner<D>,
        _: ReorderBinding,
        _: ReorderGeometry,
        _group: Option<bool>,
    ) -> Result<Option<ReorderStart>, RunnerError> {
        Ok(None)
    }
    /// [`Runner::has_reorder`].
    fn has_reorder(&self, _: &Runner<D>, _: ReorderToken) -> bool {
        false
    }
    /// [`Runner::preview_reorder`].
    fn preview_reorder(
        &self,
        _: &mut Runner<D>,
        _: ReorderToken,
        _: ReorderGeometry,
        _y: f64,
    ) -> Result<ReorderProgress, RunnerError> {
        Ok(ReorderProgress::Stale)
    }
    /// [`Runner::preview_reorder_into`].
    fn preview_reorder_into(
        &self,
        _: &mut Runner<D>,
        _: ReorderToken,
        _target: NodeKey,
        _: ReorderGeometry,
        _y: f64,
    ) -> Result<ReorderProgress, RunnerError> {
        Ok(ReorderProgress::Stale)
    }
    /// [`Runner::drop_reorder`], once it arrived.
    fn drop_reorder(
        &self,
        _: &mut Runner<D>,
        _: ReorderToken,
        _: ReorderGeometry,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        Ok(None)
    }
    /// [`Runner::cancel_reorder`].
    fn cancel_reorder(
        &self,
        _: &mut Runner<D>,
        _: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        Ok(None)
    }
    /// [`Runner::reorder_frame`].
    fn reorder_frame(&self, _: &Runner<D>, _: ReorderToken) -> Option<ReorderFrame> {
        None
    }
    /// [`Runner::finish_reorder`], for the list that owns the reorder.
    fn finish_reorder(
        &self,
        _: &mut Runner<D>,
        _: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        Ok(None)
    }
    /// [`Runner::reorder_step`].
    fn reorder_step(
        &self,
        _: &mut Runner<D>,
        _: ReorderToken,
        _: ReorderStep,
    ) -> Result<Vec<CommitReceipt>, RunnerError> {
        Ok(Vec::new())
    }
    /// `state.reorder` for the session the runner holds.
    fn reorder_json(&self, _: &Runner<D>) -> String {
        "null".into()
    }
    /// The ops that end a reorder the last commit finished.
    fn reconcile_reorder(&self, _: &mut Runner<D>) -> Result<Vec<Op>, RunnerError> {
        Ok(Vec::new())
    }
    /// A held session's deadline.
    fn reorder_deadline(&self, _: &Runner<D>) -> Option<f64> {
        None
    }
    /// A held session's deadline passed.
    fn reorder_timeout(&self, _: &mut Runner<D>) -> Result<CommitReceipt, RunnerError> {
        Ok(CommitReceipt::default())
    }
}

/// No list engine: the core alone.
pub(crate) struct NoLists;
impl<D: DataSource> Lists<D> for NoLists {}

impl<D: DataSource> Runner<D> {
    /// Nearest unmounted row snap area in a collection, along its axis, using
    /// its measured sizes or current estimates. At most 64 local rows are
    /// evaluated; no row is mounted. `geometry`: the snapport start's
    /// projected rest, the snapport's length, the least and greatest rests,
    /// and the distance of the nearest mounted target (infinity when there is
    /// none), which bounds the search.
    pub fn collection_snap(&self, view: ViewId, geometry: [f64; 5]) -> Option<f64> {
        let [proposed, port, minimum, maximum, distance] = geometry;
        if self.poisoned
            || ![proposed, port, minimum, maximum]
                .into_iter()
                .all(f64::is_finite)
            || port <= 0.0
            || maximum < minimum
            || distance.is_nan()
            || distance <= 0.0
        {
            return None;
        }
        let mut ids = Ids::default();
        let mut u = self.instance_update(&mut ids);
        let tree = self.tree.as_ref()?;
        self.seams
            .lists
            .collection_snap(tree, &mut u, &self.kernel, view, geometry)
    }

    /// The positions each unmounted row root with a snap alignment along a
    /// collection's axis rests at, first to last, in
    /// [`Runner::collection_snap`]'s coordinates: of the rows `window` covers,
    /// at most `cap` up to and including the one at `around` and `cap` after
    /// it. `snapport`: its length and the least and greatest positions of its
    /// start. No row is mounted.
    pub fn collection_snap_areas(
        &self,
        view: ViewId,
        (around, window, cap): (f64, [f64; 2], usize),
        snapport: [f64; 3],
    ) -> Vec<(f64, f64, bool)> {
        let [port, minimum, maximum] = snapport;
        if self.poisoned
            || ![around, window[0], window[1], port, minimum, maximum]
                .into_iter()
                .all(f64::is_finite)
            || port <= 0.0
            || maximum < minimum
        {
            return Vec::new();
        }
        let mut ids = Ids::default();
        let mut u = self.instance_update(&mut ids);
        match &self.tree {
            Some(tree) => self.seams.lists.collection_snap_areas(
                tree,
                &mut u,
                &self.kernel,
                view,
                (around, window, cap),
                snapport,
            ),
            None => Vec::new(),
        }
    }

    /// The first position past `release` toward `rest` an unmounted row
    /// root with `scroll-snap-stop: always` holds a snap at, in
    /// [`Runner::collection_snap`]'s coordinates; `snapport`: its length and
    /// the least and greatest positions of its start. At most 256 rows past
    /// the release are read; no row is mounted.
    pub fn collection_snap_stop(
        &self,
        view: ViewId,
        [release, rest]: [f64; 2],
        snapport: [f64; 3],
    ) -> Option<f64> {
        let [port, minimum, maximum] = snapport;
        if self.poisoned
            || ![release, rest, port, minimum, maximum]
                .into_iter()
                .all(f64::is_finite)
            || port <= 0.0
            || maximum < minimum
        {
            return None;
        }
        let mut ids = Ids::default();
        let mut u = self.instance_update(&mut ids);
        let tree = self.tree.as_ref()?;
        self.seams.lists.collection_snap_stop(
            tree,
            &mut u,
            &self.kernel,
            view,
            [release, rest, port, minimum, maximum],
        )
    }

    /// Native table rows in a list whose items are sections, including items
    /// outside the viewport. Reading their structure creates no views.
    pub fn list_sections(
        &self,
        view: ViewId,
    ) -> Result<Option<crate::list_api::SectionTable>, RunnerError> {
        let Some(tree) = self.tree.as_ref() else {
            return Ok(None);
        };
        let mut ids = Ids::default();
        let mut u = self.instance_update(&mut ids);
        Ok(self.seams.lists.list_sections(tree, &mut u, view)?)
    }

    /// Resolve an opaque row key without materializing that row.
    pub fn list_index(&self, view: ViewId, key: &str) -> Option<usize> {
        self.seams.lists.list_index(self.tree.as_ref()?, view, key)
    }

    /// Copy all logical text, or the range between two stable endpoints.
    /// At most one unmounted row is realized at a time; its operations are
    /// never committed. No data query, clock advance, or native view is created.
    pub fn list_text(
        &self,
        view: ViewId,
        range: Option<(ListTextPosition<'_>, ListTextPosition<'_>)>,
        policy: ListTextPolicy,
    ) -> Result<String, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let mut ids = Ids::default();
        let mut u = self.instance_update(&mut ids);
        let tree = self.tree.as_ref().expect("booted");
        Ok(self
            .seams
            .lists
            .list_text(tree, &mut u, view, range, policy)?)
    }

    /// Latest host travel, consumed before substantive work; no commit or edge.
    pub fn collection_travel(&mut self, view: ViewId, revision: u64, sequence: u64, offset: f64) {
        if let Some(tree) = &mut self.tree {
            (self.seams.lists).collection_travel(tree, view, (revision, sequence, offset));
        }
    }
    /// Mounted collection metadata for a host's post-commit layout/measurement.
    pub fn collections(&self) -> Vec<CollectionSnapshot> {
        match &self.tree {
            Some(tree) => self.seams.lists.collections(tree, usize::MAX),
            None => Vec::new(),
        }
    }
    /// Each mounted list's view, data generation and axis ([`Tree::collection_data`]).
    pub fn collection_data(&self) -> Vec<(ViewId, u64, bool)> {
        match &self.tree {
            Some(tree) => self.seams.lists.collection_data(tree),
            None => Vec::new(),
        }
    }
    /// One list's entry of [`Runner::collections`].
    pub fn collection(&self, view: ViewId) -> Option<CollectionSnapshot> {
        self.seams.lists.collection(self.tree.as_ref()?, view)
    }
    /// [`Tree::collection_mounted`].
    pub fn collection_mounted(&self, view: ViewId, out: &mut Vec<(ViewId, u64)>) {
        out.clear();
        if let Some(tree) = &self.tree {
            self.seams.lists.collection_mounted(tree, view, out);
        }
    }
    /// [`Runner::collections`] with only each list's first mounted row
    /// ([`Tree::collections_shallow`]).
    pub fn collections_shallow(&self) -> Vec<CollectionSnapshot> {
        match &self.tree {
            Some(tree) => self.seams.lists.collections(tree, 1),
            None => Vec::new(),
        }
    }
    /// Bound borrowed traversal and count all collections/rows before copying
    /// numeric host snapshots. No keys, records or action frames are captured.
    pub fn collections_bounded(
        &self,
        max_collections: usize,
        max_rows: usize,
        max_traversal: usize,
        max_json_bytes: usize,
    ) -> Result<Vec<CollectionSnapshot>, &'static str> {
        match &self.tree {
            Some(tree) => self.seams.lists.collections_bounded(
                tree,
                [max_collections, max_rows, max_traversal, max_json_bytes],
            ),
            None => Ok(Vec::new()),
        }
    }
    /// Compact numeric metadata for existing JSON batch envelopes, without
    /// serde: `[]` where the artifact doesn't link lists (LLP 1047.000 §9).
    pub fn collections_json(&self) -> String {
        match (self.links.lists, &self.tree) {
            (Some(lists), Some(tree)) => (lists.collections_json)(tree),
            _ => "[]".to_string(),
        }
    }
    /// Every outer list's kept inner positions (LLP 1070 §4.2), for `state`.
    pub fn kept_positions_json(&self) -> String {
        match (self.links.lists, &self.tree) {
            (Some(lists), Some(tree)) => (lists.kept_json)(tree),
            _ => "[]".to_string(),
        }
    }
    /// Decode the common numeric LE protocol before touching state.
    pub fn collection_feedback_bytes(&mut self, bytes: &[u8]) -> Result<Advanced, RunnerError> {
        let (feedback, fill) = CollectionFeedback::decode_with_fill(bytes).map_err(|_| {
            RunnerError::Instance(InstanceError::Collection(
                "malformed collection feedback".into(),
            ))
        })?;
        self.collection_feedback_filled(feedback, fill)
    }
    /// [`Runner::collection_feedback_filled`] with no limit and no motion.
    pub fn collection_feedback(
        &mut self,
        feedback: CollectionFeedback,
    ) -> Result<Advanced, RunnerError> {
        self.collection_feedback_filled(feedback, CollectionFill::default())
    }
    /// Update the addressed window and release transferred focus/interaction pins
    /// in other collections in the same commit, then dispatch each edge at most
    /// once. Start precedes end; any action state change defers end until settled.
    /// Pre-commit errors return Err; an edge refusal accompanies the
    /// committed receipts in Advanced.error. Hosts must consume both. No timers
    /// advance. Without an edge handler, no resources or keys are evaluated.
    pub fn collection_feedback_filled(
        &mut self,
        feedback: CollectionFeedback,
        fill: CollectionFill,
    ) -> Result<Advanced, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        if feedback.validate().is_err()
            || !fill.velocity.is_finite()
            || fill
                .view_offset
                .is_some_and(|n| !n.is_finite() || n.abs() > f32::MAX as f64)
        {
            return Err(RunnerError::Instance(InstanceError::Collection(
                "invalid collection feedback".into(),
            )));
        }
        let view = feedback.view;
        let mut tree = self.tree.take().expect("booted");
        let mut ids = std::mem::take(&mut self.ids);
        let result = {
            let cell_margins = |view| super::render::cell_margins(&self.kernel, view);
            let scroll_margin = |view| super::render::scroll_margin(&self.kernel, view);
            let scroll_padding = |view| super::render::scroll_padding(&self.kernel, view);
            let mut update = self.instance_update(&mut ids);
            update.reuse = self.reuse;
            update.scroll_padding = Some(&scroll_padding);
            update.scroll_margin = Some(&scroll_margin);
            if self.kernel.arena().env().cell_spaces {
                update.cell_margins = Some(&cell_margins);
            }
            (self.seams.lists)
                .update_collection(&mut tree, &mut update, feedback, fill)
                .map(|changed| {
                    (
                        changed,
                        update.ops,
                        update.surfaces,
                        update.notes,
                        update.renewed,
                        update.shown,
                        update.refused,
                    )
                })
        };
        self.tree = Some(tree);
        self.ids = ids;
        let ((changed, edge), ops, surfaces) = match result {
            Ok((changed, ops, surfaces, notes, renewed, shown, refused)) => {
                self.notes = notes;
                self.renewed = renewed;
                self.shown.extend(shown);
                for (list, why) in refused {
                    self.record_refusal(&list, why);
                }
                (changed, ops, surfaces)
            }
            Err(error) => {
                // This category is emitted only by pre-mutation geometry
                // validation. A bad host report must leave the runner usable.
                if !matches!(error, InstanceError::InvalidCollectionFeedback) {
                    self.poison();
                }
                return Err(error.into());
            }
        };
        let mut result = Advanced {
            receipts: Vec::new(),
            now_ms: self.now_ms,
            error: None,
            stopped_on_request: false,
            needs_apply: false,
        };
        // A row that showed with animations waiting on it is a commit of its
        // own when the report changed nothing else (LLP 1055 D13).
        let revealed = self
            .shown
            .revealed
            .iter()
            .any(|v| self.kernel.is_awaiting(*v));
        if changed || !ops.is_empty() || revealed {
            match self.apply(ops) {
                Ok(receipt) => {
                    self.publish_surfaces(surfaces);
                    result.receipts.push(Timed {
                        at_ms: self.now_ms,
                        receipt,
                    });
                }
                Err(error) => {
                    self.poison();
                    return Err(error);
                }
            }
        }
        if let Some(edges) = edge {
            let lists = self.seams.lists;
            lists.collection_edges(self, view, edges, &mut result);
        }
        result.now_ms = self.now_ms;
        Ok(result)
    }
    /// List `view`'s port is at `offset` (from its first row, as a report's):
    /// the rows mounted out of the port that now show stop holding what
    /// waits for them (LLP 1055 D13), in a commit of their own. No report:
    /// the window and its rows stay the last report's, for a host that
    /// waits for more travel before its next one.
    pub fn collection_shown(&mut self, view: ViewId, offset: f64) -> Result<Advanced, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let mut result = Advanced {
            receipts: Vec::new(),
            now_ms: self.now_ms,
            error: None,
            stopped_on_request: false,
            needs_apply: false,
        };
        if !offset.is_finite() {
            return Ok(result);
        }
        let mut tree = self.tree.take().expect("booted");
        let mut ids = std::mem::take(&mut self.ids);
        let shown = {
            let mut update = self.instance_update(&mut ids);
            (self.seams.lists).show_collection(&mut tree, &mut update, view, offset);
            update.shown
        };
        self.tree = Some(tree);
        self.ids = ids;
        self.shown.extend(shown);
        if (self.shown.revealed.iter()).any(|v| self.kernel.is_awaiting(*v)) {
            match self.apply(Vec::new()) {
                Ok(receipt) => result.receipts.push(Timed {
                    at_ms: self.now_ms,
                    receipt,
                }),
                Err(error) => {
                    self.poison();
                    return Err(error);
                }
            }
        }
        Ok(result)
    }
    /// After an ordinary commit: a list whose edge waited under a covered
    /// route asks its host for a report once the route shows, which offers
    /// the edge it reaches then.
    pub(super) fn release_held_edges(&mut self) -> Result<(), RunnerError> {
        if self.held_edges.is_empty() {
            return Ok(());
        }
        let lists = self.seams.lists;
        lists.release_held_edges(self)
    }
    /// Called once per ordinary commit, after settlement: each deferred edge
    /// that is ready wakes once.
    pub(super) fn wake_deferred_edges(&mut self) -> Result<(), RunnerError> {
        if self.deferred_edges.is_empty() {
            return Ok(());
        }
        let lists = self.seams.lists;
        lists.wake_deferred_edges(self)
    }

    /// Resolve a strict authored ancestor IDREF, then join only mounted collection
    /// rows. String key, current positive measurement and List handler are required.
    pub fn reorder_binding(&self, handle: NodeKey) -> Option<ReorderBinding> {
        if self.poisoned {
            return None;
        }
        let node = self.kernel.node_by_key(handle)?;
        let name = node.props.str(PropId::ReorderFor)?;
        if name.is_empty()
            || node.props.str(PropId::HeightDragFor).is_some()
            || node.props.str(PropId::TransformDragFor).is_some()
        {
            return None;
        }
        let mut at = node;
        let mut list = None;
        loop {
            if at.style.display == Display::None
                || at.props.bool(PropId::Disabled) == Some(true)
                || at.props.bool(PropId::Inert) == Some(true)
            {
                return None;
            }
            if at.key != handle && at.props.str(PropId::Id) == Some(name) {
                if list.is_some() {
                    return None;
                }
                list = Some(at.key);
            }
            if at.is_root {
                break;
            }
            at = self.kernel.node(at.parent?)?;
        }
        let list = self.kernel.node_by_key(list?)?;
        if list.node_type != NodeType::List
            || !self.handlers_of(list.id).contains(&EventKind::Reorderdrop)
        {
            return None;
        }
        let tree = self.tree.as_ref()?;
        let binding = (self.seams.lists).reorder_binding(tree, &self.kernel, list.id, handle)?;
        let mut at = node;
        loop {
            if at.style.rotate != 0.0
                || at.style.scale != 1.0
                || (at.key != binding.wrapper
                    && (at.style.translate.x != 0.0
                        || at.style.translate.y != 0.0
                        || at.style.translate_percent.x != 0.0
                        || at.style.translate_percent.y != 0.0))
            {
                return None;
            }
            if at.is_root {
                break;
            }
            at = self.kernel.node(at.parent?)?;
        }
        Some(binding)
    }
    /// Latest accepted scroll facts. Hosts also certify their own viewport mapping.
    pub fn reorder_geometry(&self, list: NodeKey) -> Option<ReorderGeometry> {
        let view = self.kernel.node_by_key(list)?.id;
        (self.seams.lists).reorder_geometry(self.tree.as_ref()?, view, list)
    }
    /// Begin a reorder of `binding`'s row, certified by `geometry`.
    pub fn begin_reorder(
        &mut self,
        binding: ReorderBinding,
        geometry: ReorderGeometry,
    ) -> Result<Option<ReorderStart>, RunnerError> {
        let lists = self.seams.lists;
        lists.begin_reorder(self, binding, geometry, None)
    }
    /// Begin a reorder that may move the row into another list of its
    /// `reorderGroup` (LLP 1094 D4); `ghost`: a ghost stands for the row
    /// while it moves, so the row itself hides until `finish_reorder`. A
    /// key's or a custom action's session (D9) has no ghost. `None` for a
    /// list without a group, or whatever `begin_reorder` refuses.
    pub fn begin_group_reorder(
        &mut self,
        binding: ReorderBinding,
        geometry: ReorderGeometry,
        ghost: bool,
    ) -> Result<Option<ReorderStart>, RunnerError> {
        let lists = self.seams.lists;
        lists.begin_reorder(self, binding, geometry, Some(ghost))
    }
    /// Whether `token`'s preview is still the list's, for the binding it began with.
    pub fn has_reorder(&self, token: ReorderToken) -> bool {
        self.seams.lists.has_reorder(self, token)
    }
    /// Move the gap to `content_y`, certified by `geometry`.
    pub fn preview_reorder(
        &mut self,
        token: ReorderToken,
        geometry: ReorderGeometry,
        content_y: f64,
    ) -> Result<ReorderProgress, RunnerError> {
        let lists = self.seams.lists;
        lists.preview_reorder(self, token, geometry, content_y)
    }
    /// Move the gap to `y` in `target`'s unpreviewed content coordinates
    /// (LLP 1094 D4, D5).
    pub fn preview_reorder_into(
        &mut self,
        token: ReorderToken,
        target: NodeKey,
        geometry: ReorderGeometry,
        y: f64,
    ) -> Result<ReorderProgress, RunnerError> {
        let lists = self.seams.lists;
        lists.preview_reorder_into(self, token, target, geometry, y)
    }
    /// Drop the row where its gap is: the list's `reorderdrop`, in one commit.
    pub fn drop_reorder(
        &mut self,
        token: ReorderToken,
        geometry: ReorderGeometry,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        self.arrived(|r| {
            let lists = r.seams.lists;
            lists.drop_reorder(r, token, geometry)
        })
    }
    /// Cancel `token`'s reorder: the row springs back.
    pub fn cancel_reorder(
        &mut self,
        token: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let lists = self.seams.lists;
        lists.cancel_reorder(self, token)
    }
    /// Where `token`'s row and gap are now, for the host's motion.
    pub fn reorder_frame(&self, token: ReorderToken) -> Option<ReorderFrame> {
        self.seams.lists.reorder_frame(self, token)
    }
    /// The host's motion of `token`'s row has landed: the preview ends.
    pub fn finish_reorder(
        &mut self,
        token: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        if self.reorder_owner != Some(token.list) {
            return Ok(None);
        }
        let lists = self.seams.lists;
        lists.finish_reorder(self, token)
    }
    /// One key's or custom action's move (LLP 1094 D9).
    pub fn reorder_step(
        &mut self,
        token: ReorderToken,
        step: ReorderStep,
    ) -> Result<Vec<CommitReceipt>, RunnerError> {
        let lists = self.seams.lists;
        lists.reorder_step(self, token, step)
    }
    /// `state.reorder` (LLP 1094 D12): the session's row, its two lists, the
    /// key the gap is before and its phase; `null` with none.
    pub fn reorder_json(&self) -> String {
        if self.reorder_session.is_none() {
            return "null".into();
        }
        self.seams.lists.reorder_json(self)
    }
    /// The queued patches, taken to be applied, and the keep of their
    /// colours, for the caller to hold until its commit marks them.
    pub(super) fn take_reorder_ops(&mut self) -> (Vec<Op>, exact_kernel::style::wide::Keep) {
        (
            std::mem::take(&mut self.reorder_ops),
            std::mem::take(&mut self.reorder_kept),
        )
    }
    /// The ops that end a reorder a commit finished; none while no list owns one.
    pub(super) fn reconcile_reorder(&mut self) -> Result<Vec<Op>, RunnerError> {
        if self.reorder_owner.is_none() {
            return Ok(Vec::new());
        }
        let lists = self.seams.lists;
        lists.reconcile_reorder(self)
    }
    /// The hold's deadline, which `timer_due_ms` reports beside the plan's
    /// timers so a host wakes for it (LLP 1094 D8).
    pub(super) fn reorder_deadline(&self) -> Option<f64> {
        self.reorder_session.as_ref()?;
        self.seams.lists.reorder_deadline(self)
    }
    /// The hold's deadline passed (LLP 1094 D8).
    pub(super) fn reorder_timeout(&mut self) -> Result<CommitReceipt, RunnerError> {
        let lists = self.seams.lists;
        lists.reorder_timeout(self)
    }
    /// `reorderdrop`'s `ReorderEvent` (LLP 1094 D2): the list the row came
    /// from and the one it was dropped on, by `id`; the same list for a drop
    /// within one and for a host's own `reorderdrop`.
    pub(super) fn reorder_record(&self, view: ViewId) -> exact_plan::Value {
        let to = self
            .kernel
            .node(view)
            .and_then(|n| n.props.str(PropId::Id).map(str::to_owned))
            .unwrap_or_default();
        let from = self
            .dropping_from
            .map_or_else(|| to.clone(), |l| self.list_id(l));
        exact_plan::Value::record(vec![
            exact_plan::Value::str(&from),
            exact_plan::Value::str(&to),
        ])
    }
    /// List `list`'s `id`, or nothing.
    pub(super) fn list_id(&self, list: NodeKey) -> String {
        self.kernel
            .node_by_key(list)
            .and_then(|n| n.props.str(PropId::Id).map(str::to_owned))
            .unwrap_or_default()
    }
}
