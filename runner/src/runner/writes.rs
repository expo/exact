//! Optimistic writes: a record per send to a mutation that declares
//! `refreshes`, and the overlay that shows them over each refreshed
//! resource's answer.
//!
//! A record is made when the send is accepted and lives in the commit's
//! checkpoint. It is pending until its final reply lands, then landed until
//! each refreshed resource has an answer, asked after the landing, that the
//! source's overlay does not keep it over. A pending record that ends
//! without landing is removed. A newer send to a newest-wins mutation ends
//! its older pending records; a queue's and a parallel one's keep theirs,
//! and a parallel mutation's are matched to their sends by send number
//! (LLP 1161.000 D3).

use super::*;

#[derive(Clone, Debug)]
pub(super) struct WriteRecord {
    /// The send's id and its place in send order.
    pub(super) id: u64,
    pub(super) mutation: usize,
    /// A parallel mutation's send number ([`Target::Sent`]); 0 for any
    /// other mutation's.
    pub(super) send: u64,
    pub(super) source: String,
    pub(super) args: Vec<Value>,
    pub(super) landed: Option<Landed>,
}

#[derive(Clone, Debug)]
pub(super) struct Landed {
    pub(super) reply: Value,
    /// The sequence when it landed: an answer asked later can retire it.
    pub(super) at: u64,
    /// The refreshed resources it still shows in, with lists expanded to
    /// their member rows.
    pub(super) showing: Vec<usize>,
}

/// The write records and the one sequence that orders sends, landings and
/// asks.
#[derive(Clone, Debug, Default)]
pub(super) struct Writes {
    pub(super) records: Vec<WriteRecord>,
    pub(super) seq: u64,
}

impl Writes {
    pub(super) fn tick(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

/// A resource's last overlay, by what it was computed from.
#[derive(Clone)]
pub(super) struct OverlayCache {
    answer: crate::held::Held,
    args: Vec<Value>,
    writes: Vec<(u64, bool, bool)>,
    shown: crate::held::Held,
}

impl<D: DataSource> Runner<D> {
    /// What a resource shows, by name: its answer with the writes that
    /// affect it laid over it, as derives and views read it.
    pub fn shown_resource(&self, name: &str) -> Option<&Value> {
        self.resource_rows()
            .position(|r| self.plan.str(r.name) == name)
            .and_then(|i| self.shown_resource_at(i))
    }

    /// What the plan's `i`th resource shows ([`Runner::shown_resource`]).
    pub(crate) fn shown_resource_at(&self, i: usize) -> Option<&Value> {
        let held = crate::vm::Column::rows(&self.resources, |row| &row.value)
            .get(i)?
            .as_ref()?;
        Some(held.get(&self.plan))
    }

    /// The writes shown over resources, in send order: each send's id, its
    /// mutation, and whether its reply has landed (the agent's `state.writes`).
    pub fn writes(&self) -> Vec<(u64, &str, bool)> {
        self.writes
            .records
            .iter()
            .map(|w| {
                let name = self.plan.str(self.plan.mutations[w.mutation].name);
                (w.id, name, w.landed.is_some())
            })
            .collect()
    }

    /// A send (`target`) was accepted. A newer send to a newest-wins
    /// mutation ends the older pending ones (LLP 1092 D1).
    pub(super) fn accept_write(
        &mut self,
        target: Target,
        source: &str,
        args: &[Value],
    ) -> Option<u64> {
        let m = target.mutation()?;
        if self.plan.mutations[m].refreshes.len == 0 {
            return None;
        }
        // Recorded before the older sends end, so ending them does not ask
        // their resources again while this one still shows in them
        let id = self.writes.tick();
        self.writes.records.push(WriteRecord {
            id,
            mutation: m,
            send: match target {
                Target::Sent(_, n) => n,
                _ => 0,
            },
            source: source.to_string(),
            args: args.to_vec(),
            landed: None,
        });
        if self.plan.mutations[m].mode == MutationMode::Newest {
            self.end_pending(m, Some(id));
        }
        Some(id)
    }

    /// The pending record a reply to `target` belongs to: a parallel send's
    /// own, a queue's oldest (its head), a newest-wins mutation's newest.
    fn in_flight_write(&self, target: Target) -> Option<usize> {
        let m = target.mutation()?;
        let mut pending = (self.writes.records.iter().enumerate())
            .filter(|(_, r)| r.mutation == m && r.landed.is_none())
            .map(|(i, _)| i);
        match (target, self.plan.mutations[m].mode) {
            (Target::Sent(_, n), _) => pending.find(|i| self.writes.records[*i].send == n),
            (_, MutationMode::Queue) => pending.next(),
            _ => pending.next_back(),
        }
    }

    /// The pending record `write` names (a ticket's), or for a send asked
    /// just now (`None`), `target`'s in flight.
    fn pending_write(&self, target: Target, write: Option<u64>) -> Option<usize> {
        match write {
            Some(id) => self
                .writes
                .records
                .iter()
                .position(|r| r.id == id && r.landed.is_none()),
            None => self.in_flight_write(target),
        }
    }

    /// The id of the write a send (`target`) asked now carries on its ticket.
    pub(super) fn asked_write(&self, target: Target) -> Option<u64> {
        (self.in_flight_write(target)).map(|i| self.writes.records[i].id)
    }

    /// A send's (`target`'s; `write`, its ticket's) reply landed: it shows
    /// until each refreshed resource has an answer asked after now that
    /// includes it. A stream's later messages land nothing more.
    pub(super) fn land_write(&mut self, target: Target, write: Option<u64>, reply: &Value) {
        let (Some(i), Some(m)) = (self.pending_write(target, write), target.mutation()) else {
            return;
        };
        let at = self.writes.tick();
        // The runner's own resources show their facts, never a write.
        let showing: Vec<usize> = self
            .declared_refreshes(m)
            .into_iter()
            .flat_map(|r| {
                if self.is_family(r) {
                    self.resources
                        .iter()
                        .map(|row| &row.member)
                        .enumerate()
                        .filter(|(_, row)| row.as_ref().is_some_and(|m| m.family == r && !m.gone))
                        .map(|(i, _)| i)
                        .collect()
                } else {
                    vec![r]
                }
            })
            .filter(|r| {
                !exact_plan::runner_owned_source(self.plan.str(self.resource_row(*r).source))
            })
            .collect();
        // Shown nowhere, it has nothing to wait for
        if showing.is_empty() {
            self.writes.records.remove(i);
            return;
        }
        self.writes.records[i].landed = Some(Landed {
            reply: reply.clone(),
            at,
            showing,
        });
    }

    /// A send (`target`) ended without landing: its record goes, and
    /// each refreshed resource is read again to reconcile.
    pub(super) fn end_write(&mut self, target: Target, write: Option<u64>) -> bool {
        let (Some(i), Some(m)) = (self.pending_write(target, write), target.mutation()) else {
            return false;
        };
        self.writes.records.remove(i);
        self.reconcile_after(m);
        true
    }

    /// Every pending record of `m` but `except` ends: an assignment to its
    /// slot, or a newer send (`except`).
    pub(super) fn end_pending(&mut self, m: usize, except: Option<u64>) {
        let before = self.writes.records.len();
        self.writes
            .records
            .retain(|r| r.mutation != m || r.landed.is_some() || Some(r.id) == except);
        if self.writes.records.len() != before {
            self.reconcile_after(m);
        }
    }

    /// Each resource `m` refreshes is asked again, unless another pending
    /// write still shows in it (its own end asks). The ask reconciles: a
    /// refusal marks the resource failed and keeps its answer, and never
    /// refuses the commit that publishes the end.
    pub(super) fn reconcile_after(&mut self, m: usize) {
        for r in self.declared_refreshes(m) {
            let still =
                self.writes.records.iter().any(|w| {
                    w.landed.is_none() && self.declared_refreshes(w.mutation).contains(&r)
                });
            if !still {
                self.force_refresh(r);
                self.owe(&[r]);
            }
        }
    }

    /// The asks of `owed` reconcile: a refusal fails the resource instead
    /// of refusing the commit.
    pub(super) fn owe(&mut self, owed: &[usize]) {
        for i in owed {
            if !self.reconciling.contains(i) {
                self.reconciling.push(*i);
            }
        }
    }

    /// The records showing in resource `i`, in the order an overlay sees
    /// them (LLP 1161.000 D3): those landed, in the order they landed, then
    /// those still out, in send order. Only a parallel mutation's land out
    /// of send order.
    fn writes_for(&self, i: usize) -> Vec<&WriteRecord> {
        let declared = self.member(i).map_or(i, |m| m.family);
        let mut writes: Vec<&WriteRecord> = (self.writes.records.iter())
            .filter(|r| match &r.landed {
                Some(l) => l
                    .showing
                    .iter()
                    .any(|&r| r == i || self.member(r).is_some_and(|m| m.family == i)),
                None => self.declared_refreshes(r.mutation).contains(&declared),
            })
            .collect();
        writes.sort_by_key(|r| r.landed.as_ref().map_or((1, r.id), |l| (0, l.at)));
        writes
    }

    /// While a list still shows a landed write, its joining members show it too.
    pub(super) fn join_member_writes(&mut self, family: usize, member: usize) {
        for w in &mut self.writes.records {
            if let Some(l) = &mut w.landed {
                if !l.showing.contains(&member)
                    && l.showing.iter().any(|&i| {
                        self.resources[i]
                            .member
                            .as_ref()
                            .is_some_and(|m| m.family == family)
                    })
                {
                    l.showing.push(member);
                }
            }
        }
    }

    /// A removed member no longer holds a landed write over its answer.
    pub(super) fn forget_member_writes(&mut self, leaving: &[usize]) {
        if leaving.is_empty() {
            return;
        }
        for w in &mut self.writes.records {
            if let Some(l) = &mut w.landed {
                l.showing.retain(|i| !leaving.contains(i));
            }
        }
        self.writes
            .records
            .retain(|w| w.landed.as_ref().is_none_or(|l| !l.showing.is_empty()));
    }

    /// Whether any write shows in resource `i` (it is not carried).
    pub(super) fn written(&self, i: usize) -> bool {
        !self.writes_for(i).is_empty()
    }

    /// What resource `i` shows: `answer`, asked at `origin`, or the source's
    /// overlay of the writes showing in it. A landed write the answer was
    /// asked after is answered by it and retired here, unless the overlay
    /// keeps it. An overlay outside the resource's shape, or one that fails,
    /// is journaled and counts as none: the answer shows.
    pub(super) fn shown(
        &mut self,
        i: usize,
        args: &[Value],
        answer: &crate::held::Held,
        origin: u64,
    ) -> crate::held::Held {
        let row = self.resource_row(i);
        let source = self.plan.str(row.source).to_string();
        let resource = self.plan.str(row.name).to_string();
        let writes: Vec<WriteRecord> = if exact_plan::runner_owned_source(&source) {
            Vec::new()
        } else {
            self.writes_for(i).into_iter().cloned().collect()
        };
        if writes.is_empty() {
            self.resources[i].overlay = None;
            return answer.clone();
        }
        let answered = |w: &WriteRecord| w.landed.as_ref().is_some_and(|l| l.at < origin);
        let key: Vec<(u64, bool, bool)> = writes
            .iter()
            .map(|w| (w.id, w.landed.is_some(), answered(w)))
            .collect();
        if let Some(o) = &self.resources[i].overlay {
            if crate::held::Held::same(&o.answer, answer) && o.args == args && o.writes == key {
                return o.shown.clone();
            }
        }
        let names: Vec<String> = writes
            .iter()
            .map(|w| {
                self.plan
                    .str(self.plan.mutations[w.mutation].name)
                    .to_string()
            })
            .collect();
        let list: Vec<Write<'_>> = writes
            .iter()
            .zip(&names)
            .map(|(w, name)| Write {
                id: w.id,
                mutation: name,
                source: &w.source,
                args: &w.args,
                reply: w.landed.as_ref().map(|l| &l.reply),
                answered: answered(w),
            })
            .collect();
        let result = self
            .data
            .overlay(&source, args, answer.get(&self.plan), &list);
        drop(list);
        self.hear_aborted();
        for line in self.data.take_logs() {
            self.log(line);
        }
        let (shown, keep) = match result {
            Ok(Some(o)) if self.check_shape(i, &o.value).is_ok() => {
                self.log(format!("overlay {resource} ({} writes)", writes.len()));
                // An equal value keeps its object, so readers are not asked again.
                let value = crate::held::Held::new(o.value);
                let value = match &self.resources[i].overlay {
                    Some(last) if crate::held::Held::equivalent(&last.shown, &value) => {
                        last.shown.clone()
                    }
                    _ => value,
                };
                (value, o.keep)
            }
            // Outside its shape, it counts as none
            Ok(Some(_)) => {
                self.log(format!(
                    "overlay {resource}: outside its shape, so its answer shows"
                ));
                (answer.clone(), Vec::new())
            }
            Ok(None) => (answer.clone(), Vec::new()),
            Err(e) => {
                self.log(format!(
                    "overlay {resource} failed, so its answer shows: {e:?}"
                ));
                (answer.clone(), Vec::new())
            }
        };
        // Answered writes the overlay did not keep are retired from `i`.
        let retire: Vec<u64> = writes
            .iter()
            .filter(|w| answered(w) && !keep.contains(&w.id))
            .map(|w| w.id)
            .collect();
        if !retire.is_empty() {
            for r in &mut self.writes.records {
                if let (true, Some(l)) = (retire.contains(&r.id), &mut r.landed) {
                    l.showing.retain(|x| *x != i);
                }
            }
            self.writes
                .records
                .retain(|r| r.landed.as_ref().is_none_or(|l| !l.showing.is_empty()));
        }
        // Keyed by what remains, so the next settlement reuses it
        let remaining: Vec<(u64, bool, bool)> = key
            .into_iter()
            .filter(|(id, ..)| !retire.contains(id))
            .collect();
        self.resources[i].overlay = Some(OverlayCache {
            answer: answer.clone(),
            args: args.to_vec(),
            writes: remaining,
            shown: shown.clone(),
        });
        shown
    }
}
