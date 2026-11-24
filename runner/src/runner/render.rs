//! One render of the instance tree, applied as one batch: the commit
//! machinery every plan runs, with or without lists.
use super::*;

impl<D: DataSource> Runner<D> {
    pub(super) fn instance_update<'a>(&'a self, ids: &'a mut Ids) -> Update<'a> {
        let mut update = Update::new(self.env(&[], &[]), &self.sites, ids);
        update.fonts = Some(self.fonts.clone());
        update.roles = Some(self.roles());
        update
    }

    pub(super) fn prepare_view_ids(
        &self,
        ids: &mut Ids,
        carried: Option<&Carried>,
    ) -> Result<crate::instance::CandidateLists, RunnerError> {
        let mut lists = crate::instance::CandidateLists::default();
        let Some(carried) = carried else {
            return Ok(lists);
        };
        if self.kernel.is_detached() || (carried.views.is_empty() && carried.lists.is_empty()) {
            return Ok(lists);
        }
        // Restoring an outer list may expose inner lists at new places. Match
        // those too before assigning ids to the rows the first frame will build.
        loop {
            let mut preview = Ids::default();
            preview.tabs.carried = ids.tabs.carried.clone();
            let mut update = self.instance_update(&mut preview);
            update.fonts = Some(std::rc::Rc::new(std::cell::RefCell::new(
                self.fonts.borrow().clone(),
            )));
            update.list_seeds = Some(&lists);
            let mut tree = Tree::create(&mut update)?;
            // The candidates sit where the carried views were shown.
            tree.place(&self.plan, &self.sites, &mut update.ops, &mut Vec::new());
            let mut kernel =
                Kernel::on_demand(Box::new(exact_kernel::MonospaceMeasurer::default()));
            let _roles = self.bind_roles();
            kernel.apply(0, 1, &update.ops)?;
            if lists.match_places(&carried.lists, &tree, &self.plan, &kernel) {
                continue;
            }
            let candidate = tree.retainable_views(&self.plan, &kernel, &preview);
            carried.views.assign(&candidate, preview.last_issued(), ids);
            return Ok(lists);
        }
    }

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
            let scroll_padding = |view, port| scroll_padding(&self.kernel, view, port);
            let mut u = self.instance_update(&mut ids);
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
pub(super) fn scroll_padding(
    kernel: &exact_kernel::Kernel,
    view: ViewId,
    measured: [f64; 2],
) -> [f64; 4] {
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
    // The browser supplies its measured port when it owns layout. A native
    // collection's reported viewport may include the node's borders.
    let [width, height] = kernel
        .laid_out_scrollport(node.key)
        .map_or_else(|| measured.map(|n| n as f32), |p| [p.width, p.height]);
    let basis = [height, width, height, width];
    std::array::from_fn(|i| f64::from(on_grid(env, sides[i].at_basis(env, basis[i]), CELLS[i])))
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
