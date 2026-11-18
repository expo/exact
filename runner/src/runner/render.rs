//! One render of the instance tree, applied as one batch: the commit
//! machinery every plan runs, with or without lists.
use super::*;

impl<D: DataSource> Runner<D> {
    /// Re-evaluate every site and apply one batch. A failure here means the
    /// instance tree and the kernel may disagree; the runner is poisoned and
    /// the host restarts it — never a half-applied frame.
    pub(super) fn update(&mut self) -> Result<CommitReceipt, RunnerError> {
        let requests = std::mem::take(&mut self.into_view);
        let receipt = if requests.is_empty() {
            self.update_tree(true, |tree, u| tree.update(u))?
        } else {
            self.update_into_views(&requests, true)?
        };
        self.release_compiled();
        Ok(receipt)
    }

    pub(super) fn update_tree(
        &mut self,
        settled: bool,
        update: impl FnOnce(&mut Tree, &mut Update<'_>) -> Result<(), crate::instance::InstanceError>,
    ) -> Result<CommitReceipt, RunnerError> {
        self.render_owned(settled, update)
    }

    /// One render, applied.
    pub(super) fn render_once(
        &mut self,
        settled: bool,
        update: impl FnOnce(&mut Tree, &mut Update<'_>) -> Result<(), crate::instance::InstanceError>,
    ) -> Result<CommitReceipt, RunnerError> {
        let (ops, surfaces, _) = self.update_ops(settled, update)?;
        self.publish_render(ops, surfaces, settled)
    }

    pub(super) fn publish_render(
        &mut self,
        ops: Vec<exact_kernel::Op>,
        surfaces: Vec<crate::SurfaceUpdate>,
        settled: bool,
    ) -> Result<CommitReceipt, RunnerError> {
        match self.apply(ops) {
            Ok(receipt) => {
                self.publish_surfaces(surfaces);
                if settled && !self.held_edges.is_empty() {
                    if let Err(error) = self.release_held_edges() {
                        self.poison();
                        return Err(error);
                    }
                }
                Ok(receipt)
            }
            Err(e) => {
                self.poison();
                Err(e)
            }
        }
    }
    /// Evaluate one phase without publishing it. Ordinary settlement runs once;
    /// a request phase only realizes the requested windows.
    pub(super) fn update_ops(
        &mut self,
        settled: bool,
        update: impl FnOnce(&mut Tree, &mut Update<'_>) -> Result<(), crate::instance::InstanceError>,
    ) -> Result<UpdateOps, RunnerError> {
        let owned = self.sync_owned();
        let owned_sends = self.sync_sends();
        let mut tree = self.tree.take().expect("booted");
        let mut ids = std::mem::take(&mut self.ids);
        let rows = std::mem::take(&mut self.row_writes);
        let result = {
            let cell_margins = |view| cell_margins(&self.kernel, view);
            let scroll_margin = |view| scroll_margin(&self.kernel, view);
            let scroll_padding = |view| scroll_padding(&self.kernel, view);
            let mut u = Update::new(self.env(&[], &[]), &self.sites, &mut ids);
            u.fonts = Some(self.fonts.clone());
            u.scroll_padding = Some(&scroll_padding);
            u.scroll_margin = Some(&scroll_margin);
            if self.kernel.arena().env().cell_spaces {
                u.cell_margins = Some(&cell_margins);
            }
            u.full = self.full;
            u.discard = self.kernel.is_detached();
            u.rows = rows;
            u.owned = owned;
            u.owned_sends = owned_sends;
            u.reuse = self.reuse;
            update(&mut tree, &mut u)
                .map(|_| (u.ops, u.surfaces, u.notes, u.renewed, u.shown, u.work))
        };
        self.ids = ids;
        self.tree = Some(tree);
        let (ops, surfaces, work) = match result {
            Ok((ops, surfaces, notes, renewed, shown, work)) => {
                self.notes = notes;
                self.renewed = renewed;
                self.shown.extend(shown);
                if settled {
                    let tree = self.tree.as_mut().expect("booted");
                    tree.last_work.rows_scanned += self.lookup_rows.take();
                    tree.last_work.derives_evaluated = self.derives_evaluated;
                    tree.last_work.store_bytes_copied =
                        self.store.copied_bytes() - self.copied_at_checkpoint;
                }
                (ops, surfaces, work)
            }
            Err(e) => {
                if settled || e != crate::instance::InstanceError::InvalidCollectionFeedback {
                    self.poison();
                }
                return Err(e.into());
            }
        };
        // Only ordinary settlement wakes deferred collection edges; geometry-only
        // feedback must not replenish this work.
        if settled {
            if let Err(error) = self.wake_deferred_edges() {
                self.poison();
                return Err(error);
            }
        }
        Ok((ops, surfaces, work))
    }
}

type UpdateOps = (
    Vec<exact_kernel::Op>,
    Vec<crate::SurfaceUpdate>,
    crate::instance::InstanceWork,
);

impl<D: DataSource> Runner<D> {
    /// After a batch applies (LLP 1055 D13): the views it renewed, taken.
    pub(super) fn settle_shown(
        &mut self,
        shown: crate::list_api::RowsShown,
        receipt: &mut exact_kernel::CommitReceipt,
    ) -> Vec<ViewId> {
        // Rows that showed stop holding their animations, and rows mounted
        // out of their port start to, before a host hears the commit.
        let renewed = std::mem::take(&mut self.renewed);
        for view in shown.revealed {
            // A row bound again where it shows is heard as new (`renewed`),
            // which starts it: only a row that stayed is named revealed.
            if self.kernel.reveal(view) && !renewed.contains(&view) {
                receipt.revealed.extend(self.kernel.arena().key_of(view));
            }
        }
        for view in shown.awaiting {
            self.kernel.await_view(view);
        }
        renewed
    }
}

/// `view`'s `scroll-padding`, top, right, bottom, left, in points, as its
/// committed style resolves it now ([`Update::scroll_padding`]): `env()` and a
/// computed value resolved.
pub(super) fn scroll_padding(kernel: &exact_kernel::Kernel, view: ViewId) -> [f64; 4] {
    let Some(node) = kernel.node(view) else {
        return [0.0; 4];
    };
    let env = &*kernel.arena().env_of(node.key.index);
    let r = &node.style.rare;
    let sides = [
        r.scroll_padding_top.clone(),
        r.scroll_padding_right.clone(),
        r.scroll_padding_bottom.clone(),
        r.scroll_padding_left.clone(),
    ];
    std::array::from_fn(|i| match sides[i].resolve(env) {
        exact_kernel::Dimension::Points(n) => f64::from(on_grid(env, n, CELLS[i])),
        _ => 0.0,
    })
}

/// Each side's cell, top, right, bottom, left.
const CELLS: [f32; 4] = {
    use exact_kernel::style::cells::{COLUMN, ROW};
    [ROW, COLUMN, ROW, COLUMN]
};

/// A scroll length as a terminal lays its spaces out: whole cells under its
/// space rule (LLP 1121 stage 11 (c)), so a keyed jump stays on the rows.
fn on_grid(env: &exact_kernel::Env, px: f32, cell: f32) -> f32 {
    match env.cell_spaces {
        true => exact_kernel::style::cells::space(px, cell),
        false => px,
    }
}

/// A row root's margins as the kernel laid them out (whole cells under a
/// terminal's space rule), for a keyed list jump.
pub(super) fn cell_margins(kernel: &exact_kernel::Kernel, view: ViewId) -> [f64; 4] {
    kernel
        .laid_margins(view)
        .map_or([0.0; 4], |m| m.map(f64::from))
}

/// The expanded target box used by a keyed list jump.
pub(super) fn scroll_margin(kernel: &exact_kernel::Kernel, view: ViewId) -> [f64; 4] {
    let Some(node) = kernel.node(view) else {
        return [0.0; 4];
    };
    let env = kernel.arena().env_of(node.key.index);
    let (env, r) = (&*env, &node.style.rare);
    let sides = [
        r.scroll_margin_top.clone(),
        r.scroll_margin_right.clone(),
        r.scroll_margin_bottom.clone(),
        r.scroll_margin_left.clone(),
    ];
    std::array::from_fn(|i| f64::from(on_grid(env, sides[i].at_basis(env, 0.0), CELLS[i])))
}
