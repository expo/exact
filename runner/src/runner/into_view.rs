//! `scrollIntoView` (LLP 1070.000): the command an action states, and the
//! agent's `tap … into`, both run here, never by a host.
use super::*;
use crate::list_api::{Align, IntoView, IntoViewStatus};

impl<D: DataSource> Runner<D> {
    /// Materialize a logical row for a host's keyboard focus traversal, using
    /// the same nearest alignment as `scrollIntoView`. The host can inspect
    /// the mounted row, focus a descendant, then send its measurement/pin
    /// feedback. No unmounted records or keys cross the host seam.
    /// `None` means that the list or row no longer exists.
    pub fn reveal_collection_index(
        &mut self,
        view: ViewId,
        index: usize,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let Some(lists) = self.links.lists else {
            return Ok(None);
        };
        let mut found = false;
        let checkpoint = self.checkpoint();
        let result = self.update_tree(false, |tree, u| {
            found = (lists.reveal_index)(tree, u, view, index)?;
            Ok(())
        });
        self.conclude(checkpoint, &result, false);
        result.map(|receipt| found.then_some(receipt))
    }

    /// Bring a virtualized list's row into view by key, as the agent's `tap
    /// <list> into <key>` asks (LLP 1070.000 §5): the same request an
    /// action's command makes, committed now. Its outcome is in `state`.
    pub fn scroll_into_view(&mut self, request: IntoView) -> Result<CommitReceipt, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let Some(_) = self.links.lists else {
            let why = "this artifact doesn't link lists".to_string();
            self.record_into_view(&request, Some(IntoViewStatus::Refused(why)));
            return Ok(CommitReceipt::default());
        };
        let checkpoint = self.checkpoint();
        let result = self.update_into_views(&[request], false);
        self.conclude(checkpoint, &result, false);
        result
    }

    /// A refusal is journaled and kept for `state` (a request that started
    /// is its list's to report).
    pub(super) fn record_into_view(&mut self, request: &IntoView, status: Option<IntoViewStatus>) {
        if let Some(IntoViewStatus::Refused(why)) = status {
            self.record_refusal(&request.list, why);
        }
    }
    pub(super) fn record_refusal(&mut self, list: &str, why: String) {
        self.log(format!("scrollIntoView {list} refused: {why}"));
        let mut entry = String::from("{\"list\":");
        crate::json::quote(list, &mut entry);
        entry.push_str(",\"status\":");
        IntoViewStatus::Refused(why).json(&mut entry);
        entry.push('}');
        self.into_view_refused.push_back(entry);
        if self.into_view_refused.len() > 8 {
            self.into_view_refused.pop_front();
        }
    }

    /// The agent's `tap <list> into <key>` (LLP 1070.000 §5): the mounted
    /// list by its view, the key as text, alignments by CSS's names.
    pub fn scroll_into_view_at(
        &mut self,
        view: ViewId,
        key: &str,
        block: &str,
        inline: &str,
    ) -> Result<CommitReceipt, RunnerError> {
        let bad = |name: &str| {
            RunnerError::Instance(crate::instance::InstanceError::Collection(format!(
                "no alignment {name}: start, center, end or nearest"
            )))
        };
        let request = IntoView {
            list: format!("#{view}"),
            key: Value::str(key),
            block: Align::parse(block).ok_or_else(|| bad(block))?,
            inline: Align::parse(inline).ok_or_else(|| bad(inline))?,
            row: None,
            view: Some(view),
            smooth: false,
            offset: None,
        };
        self.scroll_into_view(request)
    }

    /// What the platform covers of list `view`'s port at its start and end:
    /// UIKit's bars over a list drawn under them, which its host reports as
    /// padding (LLP 1130 D3). A row aligned by `scrollIntoView` or a focus
    /// lands clear of it, as UIKit's `scrollToItem(at:at:animated:)` does.
    /// Nothing moves now. Whether `view` is a mounted list.
    pub fn set_list_covered(&mut self, view: ViewId, covered: [f64; 2]) -> bool {
        match (self.links.lists, self.tree.as_mut()) {
            (Some(lists), Some(tree)) => (lists.covered)(tree, view, covered),
            _ => false,
        }
    }

    /// `state.scrollIntoView`: each list's latest request, then refusals.
    pub fn into_view_json(&self) -> String {
        let mut out = match (self.links.lists, &self.tree) {
            (Some(lists), Some(tree)) => (lists.into_view_json)(tree),
            _ => "[]".to_string(),
        };
        out.pop();
        for entry in &self.into_view_refused {
            if out.len() > 1 {
                out.push(',');
            }
            out.push_str(entry);
        }
        out.push(']');
        out
    }
}

impl<D: DataSource> Runner<D> {
    /// Compute styles between requested scrollers, but publish one logical
    /// commit. In particular, an outer seek can create the inner list whose
    /// scroll-padding the next step needs. Ordinary updates keep the one-phase
    /// path; only explicit requests pay for continuation state.
    pub(super) fn update_into_views(
        &mut self,
        requests: &[IntoView],
        settled: bool,
    ) -> Result<CommitReceipt, RunnerError> {
        let (ops, surfaces, settled) = self.prepare_into_views(settled)?;
        let result = self.update_into_views_inner(ops, surfaces, requests, settled);
        if result.is_err() {
            self.poison();
        }
        result
    }

    /// Run the initial render and any transactional owned reconciliation before
    /// starting the kernel document commit, so a refusal rolls back cleanly
    /// without poisoning the runner.
    fn prepare_into_views(
        &mut self,
        settled: bool,
    ) -> Result<(Vec<exact_kernel::Op>, Vec<crate::SurfaceUpdate>, bool), RunnerError> {
        let (mut ops, mut surfaces, _) =
            self.update_ops(
                settled,
                |tree, u| {
                    if settled {
                        tree.update(u)
                    } else {
                        Ok(())
                    }
                },
            )?;
        let mut settled = settled;
        if settled && self.transactional_render() {
            for _ in 0..super::owned::RENDERS {
                let owners = self.tree.as_ref().expect("booted").owners();
                let changed = self.reconcile_owned(&owners)?;
                if !changed {
                    self.reconcile_tasks(&owners)?;
                }
                drop(owners);
                if !changed {
                    return Ok((ops, surfaces, settled));
                }
                let (next, next_surfaces, _) = self.update_ops(true, |tree, u| tree.update(u))?;
                ops.extend(next);
                surfaces.extend(next_surfaces);
                settled = true;
            }
            self.poison();
            return Err(RunnerError::Cycle);
        }
        Ok((ops, surfaces, settled))
    }

    fn update_into_views_inner(
        &mut self,
        mut ops: Vec<exact_kernel::Op>,
        mut surfaces: Vec<crate::SurfaceUpdate>,
        requests: &[IntoView],
        settled: bool,
    ) -> Result<CommitReceipt, RunnerError> {
        let _roles = self.bind_roles();
        // The queued patches' colours stay kept until this commit marks them.
        let (mut prefix, _queued) = self.take_reorder_ops();
        if !prefix.is_empty() {
            prefix.append(&mut ops);
            ops = prefix;
        }
        self.place_portals(&mut ops);
        self.admit_drag_files(&mut ops);
        let change = self.router_change()?;
        // Each phase is prepared as `apply` prepares a commit: its `time` and
        // `progress` nodes take their text and value in it, and the font rows
        // it changes are read before it applies. A failure poisons the
        // runner, so nothing is undone.
        self.prepare_live_times(&mut ops);
        let mut axes = self.prepare_text_axes(&ops);
        let mut created = created_views(&ops);
        self.batch += 1;
        let (language, direction) = self.document_facts();
        self.kernel
            .begin_document_commit(0, self.batch, &ops, Some((&language, direction)))?;
        let mut effects = CommitReceipt::default();
        self.settle_into_view_phase(&mut effects);
        let mut notes = std::mem::take(&mut self.notes);
        let into_view = self.links.lists.map(|lists| lists.into_view);
        let mut statuses = Vec::with_capacity(requests.len());
        if !settled {
            self.tree.as_mut().expect("booted").last_work = Default::default();
        }
        for request in requests {
            let mut step = Some(request.clone());
            let mut status = IntoViewStatus::Pending;
            // At most the outer and inner scroller of this request, in order.
            // This is a continuation of the command, not a convergence loop.
            while let Some(request) = step.take() {
                let (mut phase, writes, work) = self.update_ops(false, |tree, u| {
                    status = match into_view {
                        Some(run) => run(tree, u, &request, &mut step)?,
                        None => IntoViewStatus::Refused("this artifact doesn't link lists".into()),
                    };
                    Ok(())
                })?;
                self.place_portals(&mut phase);
                self.admit_drag_files(&mut phase);
                self.prepare_live_times(&mut phase);
                axes = self.join_text_axes(axes, self.prepare_text_axes(&phase), &created);
                created.extend(created_views(&phase));
                self.kernel.continue_document_commit(&phase)?;
                self.settle_into_view_phase(&mut effects);
                notes.extend(std::mem::take(&mut self.notes));
                surfaces.extend(writes);
                ops.extend(phase);
                let total = &mut self.tree.as_mut().expect("booted").last_work;
                total.nodes_visited += work.nodes_visited;
                total.rows_keyed += work.rows_keyed;
                total.rows_reused += work.rows_reused;
                total.regions_skipped += work.regions_skipped;
                total.bindings_evaluated += work.bindings_evaluated;
                total.rows_scanned += work.rows_scanned;
                total.derives_evaluated += work.derives_evaluated;
                total.store_bytes_copied += work.store_bytes_copied;
                total.rows_rebound += work.rows_rebound;
            }
            statuses.push(status);
        }
        // Reconcile owned resources, sends, and tasks for any rows materialized
        // or unmounted during the seek phases above.
        if self.transactional_render() {
            let mut converged = false;
            for _ in 0..super::owned::RENDERS {
                let owners = self.tree.as_ref().expect("booted").owners();
                let changed = self.reconcile_owned(&owners)?;
                if !changed {
                    self.reconcile_tasks(&owners)?;
                }
                drop(owners);
                if !changed {
                    converged = true;
                    break;
                }
                let (mut phase, writes, _) = self.update_ops(true, |tree, u| tree.update(u))?;
                self.place_portals(&mut phase);
                self.admit_drag_files(&mut phase);
                self.prepare_live_times(&mut phase);
                axes = self.join_text_axes(axes, self.prepare_text_axes(&phase), &created);
                created.extend(created_views(&phase));
                self.kernel.continue_document_commit(&phase)?;
                self.settle_into_view_phase(&mut effects);
                notes.extend(std::mem::take(&mut self.notes));
                surfaces.extend(writes);
                ops.extend(phase);
            }
            if !converged {
                return Err(RunnerError::Cycle);
            }
        }
        let mut cleanup = self.reconcile_reorder()?;
        self.place_portals(&mut cleanup);
        if !cleanup.is_empty() {
            self.admit_drag_files(&mut cleanup);
            self.prepare_live_times(&mut cleanup);
            axes = self.join_text_axes(axes, self.prepare_text_axes(&cleanup), &created);
            self.kernel.continue_document_commit(&cleanup)?;
            ops.extend(cleanup);
        }
        let mut receipt = self.kernel.finish_document_commit()?;
        let live_unique = |keys: &mut Vec<exact_kernel::NodeKey>| {
            keys.retain(|key| self.kernel.node_by_key(*key).is_some());
            keys.sort_unstable_by_key(|key| (key.index, key.generation));
            keys.dedup();
        };
        live_unique(&mut effects.renewed);
        live_unique(&mut effects.revealed);
        receipt.renewed = effects.renewed;
        receipt.revealed = effects.revealed;
        self.tally(&ops, &receipt.touched);
        for note in notes {
            self.log(note);
        }
        // The font rows' presented values (LLP 1019 §13 D10).
        if let Some(tail) = self.settle_text_axes(axes, &receipt)? {
            super::apply::take_tail(&mut receipt, tail);
        }
        if !self.kernel.is_detached() && self.ids.remembered() > 2 * self.kernel.live_count() + 256
        {
            let kernel = &self.kernel;
            self.ids.retain(|view| kernel.node(view).is_some());
        }
        self.commit_router(change);
        self.retire_holds();
        self.publish_surfaces(surfaces);
        if settled && !self.held_edges.is_empty() {
            self.release_held_edges()?;
        }
        for (request, status) in requests.iter().zip(statuses) {
            self.record_into_view(request, Some(status));
        }
        Ok(receipt)
    }

    /// Apply awaiting/revealing in phase order: a row awaited by settlement
    /// and revealed by a seek must finish revealed. Only live generations are
    /// published when the logical commit finishes.
    fn settle_into_view_phase(&mut self, effects: &mut CommitReceipt) {
        let shown = std::mem::take(&mut self.shown);
        let renewed = self.settle_shown(shown, effects);
        effects.renewed.extend(
            renewed
                .into_iter()
                .filter_map(|view| self.kernel.arena().key_of(view)),
        );
    }
}

/// The views `ops` create.
fn created_views(ops: &[exact_kernel::Op]) -> std::collections::BTreeSet<ViewId> {
    (ops.iter())
        .filter_map(|op| match op {
            exact_kernel::Op::CreateView { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}
