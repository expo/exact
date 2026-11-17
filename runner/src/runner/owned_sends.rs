//! A child's sends (LLP 1035.005.004 D6). A mutation a child declares is
//! sent by each instance of the child's use on its own: its own `pending`,
//! failure, ticket, queue, written-through overlay and `then`. Its answer
//! is the instance's slot (the plan's mutation slot, owned by the same arm).
//!
//! An instance's first send to such a mutation gives it a runtime row after
//! the declared ones, as an owned resource's request has, and the root's
//! machinery sends, queues and lands it as it does a root mutation's, with
//! the row in place of the plan's index ([`Runner::mutation_row`]). The
//! row's value is written into the instance's storage, its `then` runs in
//! the instance's frames, with the arguments the plan reads there
//! (`then_args`), and the row is let go once a render destroys the
//! instance: a send already made stays made, but its reply changes nothing
//! and its `then` does not run, and its waiting sends are dropped (D4).

use super::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// A `then`'s arguments and the frames it runs in.
pub(super) type ThenCall = (Vec<Value>, Vec<Frame>);

/// The runner's row for one instance's sends to one mutation.
#[derive(Clone)]
pub(super) struct OwnedSend {
    /// The plan's mutation.
    pub(super) site: usize,
    /// The instance's storage: gone once the render destroys the instance.
    storage: Weak<RefCell<vm::Owned>>,
    /// Its instance was destroyed: its sends were let go, and a new
    /// instance's first send may take the row.
    pub(super) gone: bool,
    /// The `pending` flag and failure its readers last saw.
    shown: (bool, Option<crate::failure::Failure>),
}

impl<D: DataSource> Runner<D> {
    /// Mutation row `m`'s plan mutation: `m` itself, or the one a child's
    /// send row was made for.
    pub(super) fn mutation_site(&self, m: usize) -> usize {
        match self.owned_sends.get(m) {
            Some(Some(o)) => o.site,
            _ => m,
        }
    }

    /// Mutation row `m`'s plan row.
    pub(super) fn mutation_row(&self, m: usize) -> &exact_plan::MutationsRow {
        &self.plan.mutations[self.mutation_site(m)]
    }

    /// Mutation row `m`'s name, as its component declares it.
    pub(super) fn mutation_name(&self, m: usize) -> &str {
        let name = self.plan.str(self.mutation_row(m).name);
        if m < self.plan.mutations.len() {
            name
        } else {
            crate::agent::declared_name(name)
        }
    }

    /// How many mutation rows there are: the declared ones, then a child's.
    pub(super) fn mutation_rows(&self) -> usize {
        self.owned_sends.len().max(self.plan.mutations.len())
    }

    /// Row `m`'s instance storage, while the instance lives: `None` for a
    /// declared row.
    fn send_storage(&self, m: usize) -> Option<Rc<RefCell<vm::Owned>>> {
        match self.owned_sends.get(m) {
            Some(Some(o)) if !o.gone => o.storage.upgrade(),
            _ => None,
        }
    }

    /// The row a send to plan mutation `site`, made in `frames`, goes to:
    /// `site` for the root's, else the instance's own row, made at its first
    /// send.
    pub(super) fn send_row(&mut self, site: usize, frames: &[Frame]) -> Result<usize, RunnerError> {
        let Some(arm) = self.plan.mutations[site].owner else {
            return Ok(site);
        };
        let region = self.plan.arm(arm).region.0;
        let Some(storage) = Frame::row_of(frames, region).cloned() else {
            return Err(RunnerError::from(crate::machine::Trap::BadScope {
                pc: 0,
                depth: 0,
            }));
        };
        let held = storage.borrow().send(site as u32).filter(|&r| {
            self.send_storage(r)
                .is_some_and(|s| Rc::ptr_eq(&s, &storage))
        });
        if let Some(r) = held {
            return Ok(r);
        }
        let row = OwnedSend {
            site,
            storage: Rc::downgrade(&storage),
            gone: false,
            shown: (false, None),
        };
        // A destroyed instance's row of the same mutation with nothing in
        // flight, or a new one after the rest.
        let free = (self.plan.mutations.len()..self.mutation_rows()).find(|&r| {
            matches!(&self.owned_sends[r], Some(o) if o.gone && o.site == site)
                && !self.pending.iter().any(|p| p.target.mutation() == Some(r))
        });
        let r = match free {
            Some(r) => {
                self.owned_sends[r] = Some(row);
                self.failed_mut[r] = None;
                self.then_due[r] = f64::INFINITY;
                self.queues.reset(r);
                r
            }
            None => {
                self.owned_sends.push(Some(row));
                self.pad_mutation_rows();
                self.owned_sends.len() - 1
            }
        };
        let mut s = storage.borrow_mut();
        s.sends.retain(|(m, _)| *m != site as u32);
        s.sends.push((site as u32, r));
        Ok(r)
    }

    /// Every per-row column as long as the rows: a refused commit puts back
    /// columns its rows outgrew.
    pub(super) fn pad_mutation_rows(&mut self) {
        let n = self.mutation_rows();
        self.owned_sends.resize(n, None);
        self.pending_mut.resize(n, false);
        self.failed_mut.resize(n, None);
        self.then_due.resize(n, f64::INFINITY);
        self.queues.grow(n);
    }

    /// Write row `m`'s answer: the root's slot, or its instance's, where a
    /// refusal of this commit takes it back ([`Runner::undo_sends`]).
    pub(super) fn land_send(&mut self, m: usize, slot: usize, value: Value) {
        match self.send_storage(m) {
            Some(storage) => {
                let old = storage.borrow_mut().slots.insert(slot as u32, value);
                self.send_undo
                    .push((Rc::downgrade(&storage), slot as u32, old));
                if let Some(frames) = self.owner_frames(&storage) {
                    self.row_writes.record(&frames, &storage, slot as u32);
                }
            }
            None if m < self.plan.mutations.len() => self.slots[slot] = value,
            // A destroyed instance's: nothing reads it.
            None => {}
        }
    }

    /// Put back what the sends of a refused commit wrote into instances,
    /// from `since`.
    pub(super) fn undo_sends(&mut self, since: usize) {
        for (storage, slot, old) in self.send_undo.drain(since..).rev() {
            let Some(storage) = storage.upgrade() else {
                continue;
            };
            let mut s = storage.borrow_mut();
            match old {
                Some(v) => s.slots.insert(slot, v),
                None => s.slots.remove(&slot),
            };
        }
    }

    /// The frames of the live instance that holds `storage`.
    fn owner_frames(&self, storage: &Rc<RefCell<vm::Owned>>) -> Option<Vec<Frame>> {
        let owners = self.tree.as_ref()?.owners();
        owners
            .into_iter()
            .find(|o| Rc::ptr_eq(&o.storage, storage))
            .map(|o| o.frames)
    }

    /// Whether an action's row writes (`(row, slot, old)`) assigned row
    /// `m`'s slot: an assignment in its instance's action.
    pub(super) fn assigned_in(
        &self,
        m: usize,
        row_writes: &[(RowSlots, u32, Option<Value>)],
    ) -> bool {
        let slot = self.mutation_row(m).slot.0;
        let Some(storage) = self.send_storage(m) else {
            return false;
        };
        row_writes
            .iter()
            .any(|(rows, s, _)| *s == slot && Rc::ptr_eq(rows, &storage))
    }

    /// The resources a send of row `m` refreshes: the declared ones for the
    /// root's, its instance's requests for a child's (D6).
    pub(super) fn refreshed_rows(&self, m: usize, declared: Vec<usize>) -> Vec<usize> {
        if m < self.plan.mutations.len() {
            return declared;
        }
        let Some(storage) = self.send_storage(m) else {
            return Vec::new();
        };
        let s = storage.borrow();
        declared
            .into_iter()
            .filter_map(|r| s.request(r as u32).and_then(|q| q.row))
            .collect()
    }

    /// The arguments of row `m`'s `then`, read in the frames it runs in,
    /// and those frames. `None` when its instance is gone.
    pub(super) fn then_call(&self, m: usize) -> Result<Option<ThenCall>, RunnerError> {
        let row = self.mutation_row(m);
        let frames = if m < self.plan.mutations.len() {
            Vec::new()
        } else {
            let Some(frames) = self.send_storage(m).and_then(|s| self.owner_frames(&s)) else {
                return Ok(None);
            };
            frames
        };
        let mut args = Vec::with_capacity(row.then_args.len as usize);
        for a in row.then_args.iter() {
            let code = self.plan.arg(a).expr;
            args.push(self.eval(code, &[], &frames).map_err(|e| {
                e.evaluated(|| format!("mutation {:?} then", self.mutation_name(m)))
            })?);
        }
        Ok(Some((args, frames)))
    }

    /// After a render: let go of the sends of each instance it destroyed
    /// (D4), and the mutations whose `pending` or failure some instance's
    /// readers have not seen.
    pub(super) fn reconcile_sends(&mut self, live: &[Rc<RefCell<vm::Owned>>]) {
        for m in self.plan.mutations.len()..self.mutation_rows() {
            let alive = match &self.owned_sends[m] {
                Some(o) if !o.gone => o
                    .storage
                    .upgrade()
                    .is_some_and(|s| live.iter().any(|l| Rc::ptr_eq(l, &s))),
                _ => continue,
            };
            if !alive {
                self.let_go_send(m);
            }
        }
    }

    /// Whether any child's send row is live: the render's instances are
    /// then walked for the ones it destroyed.
    pub(super) fn sends_live(&self) -> bool {
        self.owned_sends
            .iter()
            .any(|o| o.as_ref().is_some_and(|o| !o.gone))
    }

    /// Let go of row `m`, whose instance a render destroyed: its requests in
    /// flight are forgotten (a write already sent stays sent), its waiting
    /// sends and their writes are dropped, and its `then` is not run.
    fn let_go_send(&mut self, m: usize) {
        let targets: Vec<Target> = (self.pending.iter())
            .map(|p| p.target)
            .filter(|t| t.mutation() == Some(m))
            .collect();
        for t in targets {
            self.forget_ticket(t);
        }
        self.unsent.retain(|u| u.0.mutation() != Some(m));
        self.drop_waiting(m);
        self.then_due[m] = f64::INFINITY;
        self.landed.retain(|l| *l != m);
        self.writes.records.retain(|w| w.mutation != m);
        self.failed_mut[m] = None;
        if let Some(o) = self.owned_sends[m].as_mut() {
            o.gone = true;
            o.storage = Weak::new();
            o.shown = (false, None);
        }
        self.sync_pending_flags();
    }

    /// The plan mutations whose `pending` or failure changed in some live
    /// instance since its readers last saw it, whose readers the render
    /// must visit.
    pub(super) fn sync_sends(&mut self) -> Vec<usize> {
        let mut changed = Vec::new();
        for m in self.plan.mutations.len()..self.mutation_rows() {
            let now = (self.pending_mut[m], self.failed_mut[m].clone());
            let Some(o) = self.owned_sends[m].as_mut().filter(|o| !o.gone) else {
                continue;
            };
            if o.shown != now {
                o.shown = now;
                if !changed.contains(&o.site) {
                    changed.push(o.site);
                }
            }
        }
        changed
    }
}
