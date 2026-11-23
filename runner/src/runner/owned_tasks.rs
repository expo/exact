//! Tasks use the scheduler's runtime rows, one per live owning instance
//! (LLP 1035.005.004 D6). A recycled row gets a new arming generation, so
//! a native callback from its predecessor cannot run it.
use super::*;
use std::rc::{Rc, Weak};

pub(super) type TaskOwner = Option<Weak<std::cell::RefCell<vm::Owned>>>;

/// The instance half of the commit checkpoint. Tree clones share storage,
/// so each owner's contents must be saved separately from its identity.
pub(super) struct RenderCheckpoint {
    tree: Tree,
    ids: crate::instance::IdsCheckpoint,
    storage: Vec<(RowSlots, vm::Owned)>,
    rows: crate::instance::RowWrites,
    notes: Vec<String>,
    renewed: Vec<ViewId>,
    shown: crate::list_api::RowsShown,
    deferred_edges: Vec<(u32, Vec<Target>)>,
    held_edges: Vec<u32>,
}

pub(super) fn same_owner(a: &TaskOwner, b: &TaskOwner) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Weak::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl<D: DataSource> Runner<D> {
    pub(super) fn checkpoint_render(&self) -> Option<RenderCheckpoint> {
        if !self.transactional_render() {
            return None;
        }
        let tree = self.tree.as_ref()?;
        Some(RenderCheckpoint {
            tree: tree.clone(),
            ids: self.ids.checkpoint(),
            storage: tree.save_storage(),
            rows: self.row_writes.clone(),
            notes: self.notes.clone(),
            renewed: self.renewed.clone(),
            shown: self.shown.clone(),
            deferred_edges: self.deferred_edges.clone(),
            held_edges: self.held_edges.clone(),
        })
    }

    pub(super) fn restore_render(&mut self, saved: RenderCheckpoint) {
        self.tree = Some(saved.tree);
        self.ids.restore(saved.ids);
        self.row_writes = saved.rows;
        self.notes = saved.notes;
        self.renewed = saved.renewed;
        self.shown = saved.shown;
        self.deferred_edges = saved.deferred_edges;
        self.held_edges = saved.held_edges;
        for (storage, value) in saved.storage {
            *storage.borrow_mut() = value;
        }
    }

    pub(super) fn transactional_render(&self) -> bool {
        self.plan.resources.iter().any(|r| r.owner.is_some())
            || self.plan.mutations.iter().any(|m| m.owner.is_some())
            || self.plan.timers.iter().any(|t| t.owner.is_some())
    }

    /// New task frames and immediate owned answers must all validate before
    /// any render reaches the kernel. A refusal keeps the previous instances.
    pub(super) fn render_task_owned(
        &mut self,
        settled: bool,
        update: impl FnOnce(&mut Tree, &mut Update<'_>) -> Result<(), crate::instance::InstanceError>,
    ) -> Result<CommitReceipt, RunnerError> {
        let (mut ops, mut surfaces, _) = self.update_ops(settled, update)?;
        let mut settled = settled;
        for _ in 0..super::owned::RENDERS {
            if !self.reconcile_owned()? {
                self.reconcile_tasks(&self.tree.as_ref().expect("booted").owners())?;
                return self.publish_render(ops, surfaces, settled);
            }
            let (next, next_surfaces, _) = self.update_ops(true, |tree, u| tree.update(u))?;
            ops.extend(next);
            surfaces.extend(next_surfaces);
            settled = true;
        }
        self.poison();
        Err(RunnerError::Cycle)
    }

    pub(super) fn timer_row(&self, i: usize) -> &exact_plan::TimersRow {
        &self.plan.timers[self.timers[i].site]
    }

    pub(super) fn task_name(&self, i: usize) -> String {
        let name = self.plan.str(self.timer_row(i).name);
        if self.timers[i].owner.is_some() {
            format!("{}@{}", name, self.timers[i].path)
        } else {
            name.to_string()
        }
    }

    pub(super) fn arm_task(&mut self, i: usize, on: bool, key: Option<String>) {
        let row = self.timer_row(i);
        let (frame, interval) = (row.frame, row.interval_ms);
        let cause = self.cause();
        let t = &mut self.timers[i];
        if !on {
            t.armed = false;
            t.next_ms = f64::INFINITY;
            t.key = None;
            t.completion = Default::default();
        } else if !t.armed || t.key != key {
            t.armed = true;
            t.cause = cause;
            t.generation = t.generation.wrapping_add(1);
            t.completion = Default::default();
            t.key = key;
            t.base = self.now_ms;
            t.k = 1;
            t.next_ms = if frame {
                virtual_frame(self.now_ms, 1)
            } else {
                self.now_ms + f64::from(interval)
            };
        }
    }

    pub(super) fn task_gate(
        &self,
        i: usize,
        frames: &[Frame],
    ) -> Result<(bool, Option<String>), RunnerError> {
        let row = self.timer_row(i);
        let on = !row.gated || self.eval(row.gate, &[], frames)? == Value::Bool(true);
        let key = if row.keyed && on {
            Some(
                crate::instance::key_text(&self.eval(row.key, &[], frames)?).ok_or_else(|| {
                    RunnerError::TaskKey {
                        task: self.plan.str(row.name).to_string(),
                    }
                })?,
            )
        } else {
            None
        };
        Ok((on, key))
    }

    pub(super) fn reconcile_tasks(
        &mut self,
        owners: &[crate::instance::OwnerInst],
    ) -> Result<(), RunnerError> {
        let first = self.plan.timers.len();
        for i in first..self.timers.len() {
            let alive = self.timers[i]
                .owner
                .as_ref()
                .and_then(|w| w.upgrade())
                .is_some_and(|s| owners.iter().any(|o| Rc::ptr_eq(&s, &o.storage)));
            if !alive {
                self.arm_task(i, false, None);
                self.timers[i].owner = None;
            }
        }
        let mut seen = Vec::new();
        for owner in owners {
            for &site in &owner.storage.borrow().tasks {
                let held = (first..self.timers.len()).find(|&i| {
                    self.timers[i].site == site as usize
                        && self.timers[i]
                            .owner
                            .as_ref()
                            .and_then(|w| w.upgrade())
                            .is_some_and(|s| Rc::ptr_eq(&s, &owner.storage))
                });
                let i = held.unwrap_or_else(|| {
                    let free = (first..self.timers.len()).find(|&i| self.timers[i].owner.is_none());
                    let mut timer = self.timers[site as usize].clone();
                    timer.instance = self.timers.iter().map(|t| t.instance).max().unwrap_or(0) + 1;
                    timer.owner = Some(Rc::downgrade(&owner.storage));
                    timer.path = owner.path.clone();
                    match free {
                        Some(i) => {
                            timer.generation = self.timers[i].generation.wrapping_add(1);
                            self.timers[i] = timer;
                            i
                        }
                        None => {
                            self.timers.push(timer);
                            self.timers.len() - 1
                        }
                    }
                });
                let (on, key) = self.task_gate(i, &owner.frames)?;
                seen.push((i, on, key));
            }
        }
        for (i, on, key) in seen {
            self.arm_task(i, on, key);
        }
        Ok(())
    }

    pub(super) fn task_action(&mut self, i: usize) -> Result<CommitReceipt, RunnerError> {
        let frames = match &self.timers[i].owner {
            None => Vec::new(),
            Some(weak) => {
                let storage = weak
                    .upgrade()
                    .ok_or(crate::machine::Trap::BadScope { pc: 0, depth: 0 })?;
                self.tree
                    .as_ref()
                    .unwrap()
                    .owners()
                    .into_iter()
                    .find(|o| Rc::ptr_eq(&o.storage, &storage))
                    .map(|o| o.frames)
                    .ok_or(crate::machine::Trap::BadScope { pc: 0, depth: 0 })?
            }
        };
        let row = self.timer_row(i);
        let args = row
            .args
            .iter()
            .map(|a| self.eval(self.plan.arg(a).expr, &[], &frames))
            .collect::<Result<Vec<_>, _>>()?;
        let action = row.action;
        self.as_cause(self.timers[i].cause, |r| {
            r.traced_action("task", action, args, &frames)
        })
    }
}

#[cfg(test)]
#[path = "owned_tasks_tests.rs"]
mod tests;
