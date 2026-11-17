//! A child's requests (LLP 1035.005.004). A resource a child declares is
//! owned by each instance of the child's use: one request per instance, with
//! its own value, `pending`, ticket and refresh, made with the instance and
//! let go with it.
//!
//! The render makes instances, so it is where an owned request starts: a new
//! instance's storage gets a request showing its placeholder. After the
//! render, [`Runner::reconcile_owned`] reads each live instance's gates and
//! arguments in that instance's frames, gives a new request a runtime row (a
//! row after the declared ones, as a list's members are), records what each
//! row wants, and lets go of the rows of instances the render destroyed. A
//! settlement then asks what changed, through the root's machinery: the row
//! settles as a root resource does, with the recorded gate and arguments in
//! place of its plan's code. The render that follows shows what it asked
//! (`pending`) or what answered, copied into each instance's storage before
//! the render ([`Runner::sync_owned`]). So an answer, even an immediate one,
//! lands in a render after the one that made the instance.
//!
//! A child used outside every region has one instance, as long-lived as the
//! root: its resources are the root's rows, marked `late`, which settle with
//! the root's once its slots are initialized at boot (D2).

use super::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// The runner's row for one instance's request.
#[derive(Clone)]
pub(super) struct OwnedRow {
    /// The plan's resource.
    pub(super) site: usize,
    /// The instance's path, for the agent's `state` and the journal.
    pub(super) path: String,
    /// The instance's storage: gone once the render destroys the instance.
    storage: Weak<RefCell<vm::Owned>>,
    /// The gate and arguments the instance's frames read at the last render.
    pub(super) want: Want,
    /// Its instance was destroyed: its request was let go, and it settles to
    /// nothing until a new request takes its row.
    pub(super) gone: bool,
    /// Its instance is a launch instance, made by the boot render, and the
    /// request has not settled yet: it takes the bake's answer (D8).
    pub(super) launch: bool,
}

/// What an instance's request asks for.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Want {
    /// Its `when` holds (true when it has none).
    pub(super) open: bool,
    /// Its arguments.
    pub(super) args: Vec<Value>,
}

/// How many renders one commit may make while owned requests answer at once
/// and ask again: past it, a request's arguments read its own answer.
const RENDERS: usize = 64;

impl<D: DataSource> Runner<D> {
    /// Row `i`'s instance request, when it is one.
    pub(super) fn owned_row(&self, i: usize) -> Option<&OwnedRow> {
        self.resources.get(i).and_then(|row| row.owned.as_ref())
    }

    /// Whether settlement leaves row `i` alone: a declared resource a
    /// child's instances own (each has a row of its own), a `late` one before
    /// its instance's slots exist, or the row of a destroyed instance.
    pub(super) fn settles_to_nothing(&self, i: usize) -> bool {
        match self.owned_row(i) {
            Some(o) => o.gone,
            None if i < self.plan.resources.len() => {
                let row = &self.plan.resources[i];
                row.owner.is_some() || row.late && !self.late_ready
            }
            None => false,
        }
    }

    /// At boot, once the `late` slots exist: a child used outside every
    /// region asks what it owns, reading them (D2).
    pub(super) fn settle_late(&mut self, compiled: bool) -> Result<(), RunnerError> {
        self.late_ready = true;
        if self.plan.resources.iter().any(|r| r.late) {
            self.settle(compiled)?;
        }
        Ok(())
    }

    /// Render `update` and every render owned requests need after it: while
    /// a render leaves an instance whose request changed, settle what it
    /// asks and render again (module doc).
    pub(super) fn render_owned(
        &mut self,
        settled: bool,
        update: impl FnOnce(&mut Tree, &mut Update<'_>) -> Result<(), crate::instance::InstanceError>,
    ) -> Result<CommitReceipt, RunnerError> {
        let mut receipt = self.render_once(settled, update)?;
        for _ in 0..RENDERS {
            if !self.reconcile_owned()? {
                return Ok(receipt);
            }
            let next = self.render_once(true, |tree, u| tree.update(u))?;
            receipt = merged(receipt, next);
        }
        self.poison();
        Err(RunnerError::Cycle)
    }

    /// After the boot's first frame, whose `receipt` it extends: the renders
    /// its instances' requests need.
    pub(super) fn boot_owned(&mut self, receipt: &mut CommitReceipt) -> Result<(), RunnerError> {
        self.owned_launch = true;
        let asked = self.reconcile_owned();
        self.owned_launch = false;
        // A checkpoint's answers that no launch instance took are dropped,
        // so an instance made later asks.
        for i in 0..self.plan.resources.len() {
            if self.plan.resources[i].owner.is_some() {
                self.resources[i].page_answers = None;
            }
        }
        if !asked? {
            return Ok(());
        }
        for _ in 0..RENDERS {
            let next = self.render_once(true, |tree, u| tree.update(u))?;
            *receipt = merged(std::mem::take(receipt), next);
            if !self.reconcile_owned()? {
                return Ok(());
            }
        }
        self.poison();
        Err(RunnerError::Cycle)
    }

    /// After a render: each live instance's requests' gates and arguments,
    /// read in its frames; a row for each new request; the requests of
    /// destroyed instances let go. When anything changed, settle it, and say
    /// so: the caller renders again. (A `refresh` is the commit's own
    /// settlement's, which settles every row.)
    pub(super) fn reconcile_owned(&mut self) -> Result<bool, RunnerError> {
        let requests = self.plan.resources.iter().any(|r| r.owner.is_some());
        if !requests && !self.sends_live() {
            return Ok(false);
        }
        let owners = self.tree.as_ref().map(Tree::owners).unwrap_or_default();
        // A destroyed instance's sends go with it (D6); nothing a live
        // instance shows reads them.
        if self.sends_live() {
            let live: Vec<_> = owners.iter().map(|o| o.storage.clone()).collect();
            self.reconcile_sends(&live);
        }
        if !requests {
            return Ok(false);
        }
        let mut changed = false;
        let mut live = Vec::new();
        for owner in &owners {
            let requests: Vec<(usize, Option<usize>)> = (owner.storage.borrow().requests)
                .iter()
                .map(|q| (q.resource as usize, q.row))
                .collect();
            for (k, (site, row)) in requests.into_iter().enumerate() {
                let want = self.owned_want(site, &owner.frames)?;
                let mine = row.filter(|&r| {
                    self.owned_row(r).is_some_and(|o| {
                        !o.gone && std::ptr::eq(o.storage.as_ptr(), Rc::as_ptr(&owner.storage))
                    })
                });
                let r = match mine {
                    Some(r) => {
                        let o = self.resources[r].owned.as_mut().expect("owned");
                        if o.path != owner.path {
                            o.path.clone_from(&owner.path);
                        }
                        // NaN arguments are unchanged, as a root resource's are.
                        let same = o.want.open == want.open
                            && crate::compare::unchanged_arguments(&o.want.args, &want.args);
                        if !same {
                            o.want = want;
                            changed = true;
                        }
                        r
                    }
                    None => {
                        let r = self.add_owned(site, owner, want);
                        owner.storage.borrow_mut().requests[k].row = Some(r);
                        changed = true;
                        r
                    }
                };
                live.push(r);
            }
        }
        let gone: Vec<usize> = (0..self.resources.len())
            .filter(|r| self.owned_row(*r).is_some_and(|o| !o.gone) && !live.contains(r))
            .collect();
        if !gone.is_empty() {
            self.let_go_owned(&gone);
            changed = true;
        }
        if changed {
            self.settle(false)?;
            for row in &mut self.resources {
                if let Some(o) = row.owned.as_mut() {
                    o.launch = false;
                }
            }
        }
        Ok(changed)
    }

    /// Resource `site`'s gate and arguments in an instance's `frames`.
    fn owned_want(&self, site: usize, frames: &[Frame]) -> Result<Want, RunnerError> {
        let row = &self.plan.resources[site];
        let name = || self.plan.str(row.name).to_string();
        let open = match row.gate.iter().next() {
            None => true,
            Some(gate) => {
                let code = self.plan.arg(gate).expr;
                let value = self
                    .eval(code, &[], frames)
                    .map_err(|e| e.evaluated(|| format!("resource {:?} gate", name())))?;
                value == Value::Bool(true)
            }
        };
        let mut args = Vec::with_capacity(row.args.len as usize);
        for (k, a) in row.args.iter().enumerate() {
            let code = self.plan.arg(a).expr;
            let value = self
                .eval(code, &[], frames)
                .map_err(|e| e.evaluated(|| format!("resource {:?} argument {}", name(), k + 1)))?;
            args.push(value);
        }
        Ok(Want { open, args })
    }

    /// A row for `owner`'s request for resource `site`: a destroyed
    /// instance's row of the same resource with nothing in flight, or a new
    /// one after the rest.
    fn add_owned(&mut self, site: usize, owner: &crate::instance::OwnerInst, want: Want) -> usize {
        let free = (0..self.resources.len()).find(|&r| {
            self.owned_row(r).is_some_and(|o| o.gone && o.site == site)
                && !self.pending.iter().any(|p| p.target == Target::Resource(r))
        });
        let row = resources::ResourceRow {
            owned: Some(OwnedRow {
                site,
                path: owner.path.clone(),
                storage: Rc::downgrade(&owner.storage),
                want,
                gone: false,
                launch: self.owned_launch,
            }),
            ..Default::default()
        };
        match free {
            Some(r) => {
                self.refresh_next.retain(|x| *x != r);
                self.forget_resource_row(r);
                self.resources[r] = row;
                r
            }
            None => {
                self.resources.push(row);
                self.resources.len() - 1
            }
        }
    }

    /// Let go of each row in `gone`, whose instance the render destroyed: its
    /// request goes, and a late answer to it is dropped (D4).
    fn let_go_owned(&mut self, gone: &[usize]) {
        for &r in gone {
            self.forget(Target::Resource(r));
            let mut owned = self.resources[r].owned.take().expect("owned");
            owned.gone = true;
            owned.storage = Weak::new();
            self.resources[r] = resources::ResourceRow {
                owned: Some(owned),
                ..Default::default()
            };
            self.refresh_next.retain(|x| *x != r);
        }
        self.sync_pending_flags();
    }

    /// Copy what each live instance request's row shows into its instance's
    /// storage, which its view reads; the resources whose requests changed,
    /// whose readers the render must visit.
    pub(super) fn sync_owned(&mut self) -> Vec<usize> {
        let mut changed = Vec::new();
        for r in 0..self.resources.len() {
            let Some(o) = self.owned_row(r).filter(|o| !o.gone) else {
                continue;
            };
            let Some(storage) = o.storage.upgrade() else {
                continue;
            };
            let site = o.site;
            let row = &self.resources[r];
            let failed = crate::failure::state(
                crate::vm::Column::rows(&self.resources, |row| &row.failed_args),
                crate::vm::Column::rows(&self.resources, |row| &row.failed_why),
                r,
            );
            let mut storage = storage.borrow_mut();
            let Some(request) = storage.requests.iter_mut().find(|q| q.row == Some(r)) else {
                continue;
            };
            let value = row.value.as_ref().unwrap_or(&request.value);
            let same = crate::held::Held::same(value, &request.value)
                && request.pending == row.pending
                && request.current == row.current
                && request.failed == failed;
            if !same {
                request.value = value.clone();
                request.pending = row.pending;
                request.current = row.current;
                request.failed = failed;
                if !changed.contains(&site) {
                    changed.push(site);
                }
            }
        }
        changed
    }

    /// Run an action's `refresh` of resource `i` in the frames it ran in: a
    /// child's reaches only the instance whose action ran (D5).
    pub(super) fn refresh_in(&mut self, i: usize, frames: &[Frame]) {
        let Some(arm) = self.plan.resources.get(i).and_then(|r| r.owner) else {
            self.force_refresh(i);
            return;
        };
        let region = self.plan.arm(arm).region.0;
        let row = Frame::row_of(frames, region)
            .and_then(|s| s.borrow().request(i as u32).and_then(|q| q.row));
        if let Some(r) = row {
            self.force_refresh(r);
        }
    }

    /// Compile into `b` the answers the launch instances' requests settled
    /// on (D8), by resource and arguments, under the root's skips: what
    /// waits, shows a placeholder, drew randomness, read the store or read
    /// the clock (`dated`). A resource whose launch instances settled on
    /// different arguments compiles the first; a launch with other
    /// arguments asks.
    pub fn bake_launch_answers(&self, b: &mut exact_plan::builder::PlanBuilder, dated: &[bool]) {
        for r in 0..self.resources.len() {
            let Some(o) = self.owned_row(r).filter(|o| !o.gone) else {
                continue;
            };
            let row = &self.resources[r];
            let Some(state) = row.state.as_ref().filter(|s| !s.placeholder) else {
                continue;
            };
            let skip = row.pending || row.entropy_reader || row.store_reader;
            let id = exact_plan::ResourcesId(o.site as u32);
            if skip
                || dated.get(o.site) == Some(&true)
                || b.plan().resources[o.site].initial.len > 0
            {
                continue;
            }
            b.set_resource_initial(id, state.value.get(&self.plan));
            b.set_resource_initial_args(id, &state.args);
        }
    }

    /// Each live instance request, for the agent's `state` (D8): its
    /// instance's path, its resource and the value its view shows.
    pub fn instance_resources(&self) -> Vec<(&str, usize, Option<&Value>)> {
        (0..self.resources.len())
            .filter_map(|r| {
                let o = self.owned_row(r).filter(|o| !o.gone)?;
                Some((o.path.as_str(), o.site, self.shown_resource_at(r)))
            })
            .collect()
    }
}

/// One commit's receipt for two renders, `a` then `b`: what a host reads
/// of the commit as if it had been one batch. A node made by `a` and
/// destroyed by `b` is in neither list.
fn merged(mut a: CommitReceipt, b: CommitReceipt) -> CommitReceipt {
    use std::collections::BTreeSet;
    let gone: BTreeSet<_> = b.destroyed.iter().copied().collect();
    let made: BTreeSet<_> = a.created.iter().copied().collect();
    a.created.retain(|k| !gone.contains(k));
    a.created.extend(b.created);
    a.destroyed
        .extend(b.destroyed.into_iter().filter(|k| !made.contains(k)));
    a.exits
        .extend(b.exits.into_iter().filter(|e| !made.contains(&e.key)));
    a.handoffs.extend(b.handoffs);
    let created: BTreeSet<_> = a.created.iter().copied().collect();
    let live = |k: &exact_kernel::NodeKey| !gone.contains(k) && !created.contains(k);
    for (into, from) in [
        (&mut a.touched, b.touched),
        (&mut a.display_changed, b.display_changed),
        (&mut a.timelines, b.timelines),
        (&mut a.renewed, b.renewed),
        (&mut a.revealed, b.revealed),
    ] {
        into.extend(from);
        let mut seen = BTreeSet::new();
        into.retain(|k| seen.insert(*k));
    }
    a.touched.retain(live);
    a.display_changed.retain(|k| !gone.contains(k));
    a.timelines.retain(|k| !gone.contains(k));
    a.renewed.retain(|k| !gone.contains(k));
    a.revealed.retain(|k| !gone.contains(k));
    a.layout_invalidated |= b.layout_invalidated;
    a.batch = b.batch;
    a.root_id = b.root_id;
    a.epoch = b.epoch;
    a
}
