use super::*;

impl<D: DataSource> Runner<D> {
    /// Rewrite a batch to publish the final placement by `portal`
    /// (LLP 1013.002 D2); every kernel commit with structure passes here.
    pub(super) fn place_portals(&mut self, ops: &mut Vec<exact_kernel::Op>) {
        if let (Some(tree), false) = (self.tree.as_mut(), self.kernel.is_detached()) {
            tree.place(&self.plan, &self.sites, ops, &mut self.notes);
        }
    }

    /// The `portal` movers alive and shown nowhere (LLP 1013.002 D5): each
    /// heads a subtree a host keeps, unattached, beside the roots' trees.
    pub fn hidden_movers(&self) -> Vec<ViewId> {
        self.tree
            .as_ref()
            .map(|t| t.hidden_movers())
            .unwrap_or_default()
    }

    /// Each `portal` mover shown away from its declaration or hidden, with
    /// its declaration parent, in id order.
    pub fn declared_parents(&self) -> Vec<(ViewId, ViewId)> {
        (self.tree.as_ref())
            .map(|t| t.declared_parents())
            .unwrap_or_default()
    }

    /// A mover's declaration parent: the next step of an event's path up
    /// from it (LLP 1013.002 D1), where a host walks that path itself.
    pub fn declared_parent(&self, view: ViewId) -> Option<ViewId> {
        self.tree.as_ref().and_then(|t| t.declared_parent(view))
    }

    /// The views a destroy in the batch just placed leaves alive: each
    /// extracted mover's subtree, which the kernel still shows inside the
    /// destroyed node until the batch applies.
    pub(super) fn kept_by_portals(&self) -> std::collections::HashSet<ViewId> {
        let arena = self.kernel.arena();
        (self.tree.iter())
            .flat_map(|t| t.extracted())
            .filter_map(|v| self.kernel.node(*v))
            .flat_map(|n| arena.subtree(n.key.index))
            .map(|slot| arena.local_id(slot))
            .collect()
    }

    pub(super) fn apply(
        &mut self,
        mut ops: Vec<exact_kernel::Op>,
    ) -> Result<CommitReceipt, RunnerError> {
        let _roles = self.bind_roles(); // Its own (LLP 1121 stage 5).
                                        // The queued patches' colours stay kept until this commit marks them.
        let (mut prefix, _queued) = self.take_reorder_ops();
        if !prefix.is_empty() {
            prefix.append(&mut ops);
            ops = prefix;
        }
        self.place_portals(&mut ops);
        self.admit_drag_files(&mut ops);
        let change = self.router_change()?;
        let live_undo = self.prepare_live_times(&mut ops);
        let axes = self.prepare_text_axes(&ops);
        self.batch += 1;
        let (language, direction) = self.document_facts();
        let shown = std::mem::take(&mut self.shown);
        let mut receipt =
            match self
                .kernel
                .apply_document(0, self.batch, &ops, Some((&language, direction)))
            {
                Ok(receipt) => receipt,
                Err(e) => {
                    live_undo.restore(&mut self.live_times);
                    self.notes.clear();
                    return Err(e.into());
                }
            };
        let renewed = self.settle_shown(shown, &mut receipt);
        self.tally(&ops, &receipt.touched);
        for note in std::mem::take(&mut self.notes) {
            self.log(note);
        }
        let arena = self.kernel.arena();
        receipt.renewed = renewed
            .into_iter()
            .filter_map(|view| arena.key_of(view))
            .collect();
        // Forget destroyed views once they outnumber the live ones (a
        // detached kernel holds none, and its runner asks for none).
        if !self.kernel.is_detached() && self.ids.remembered() > 2 * self.kernel.live_count() + 256
        {
            let kernel = &self.kernel;
            self.ids.retain(|view| kernel.node(view).is_some());
        }
        let mut cleanup = self.reconcile_reorder()?;
        self.place_portals(&mut cleanup);
        if !cleanup.is_empty() {
            self.batch += 1;
            let tail = self.kernel.apply(0, self.batch, &cleanup)?;
            self.tally(&cleanup, &tail.touched);
            take_tail(&mut receipt, tail);
        }
        // The font rows' presented values (LLP 1019 §13 D10).
        if let Some(tail) = self.settle_text_axes(axes, &receipt)? {
            take_tail(&mut receipt, tail);
        }
        self.commit_router(change);
        self.retire_holds();
        Ok(receipt)
    }
}

/// One receipt for a commit and the one that followed it in the same apply.
pub(super) fn take_tail(receipt: &mut CommitReceipt, tail: CommitReceipt) {
    receipt.batch = tail.batch;
    receipt.epoch = tail.epoch;
    for key in tail.touched {
        if !receipt.created.contains(&key) && !receipt.touched.contains(&key) {
            receipt.touched.push(key);
        }
    }
    receipt.layout_invalidated |= tail.layout_invalidated;
}
