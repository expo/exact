//! An action, timer or request reply commits one transaction: settle, update
//! the tree and apply one batch, or refuse and leave the kernel as it was.

use super::*;

/// A string argument or write past [`crate::vm::MAX_STRING`] bytes.
fn too_long(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.len() > crate::vm::MAX_STRING)
}

/// A refused commit restores state, store and request bookkeeping (§6 atomicity),
/// including settlement flags: deferred resource staleness, store provenance,
/// and refreshes requested by an action.
pub(super) struct Checkpoint {
    store: crate::store::StoreCheckpoint,
    /// How many let-go requests with effects were collected before it.
    letting: usize,
    slots: Vec<Value>,
    resources: Vec<super::resources::ResourceRow>,
    module_readers: std::collections::BTreeMap<String, std::collections::BTreeMap<usize, String>>,
    uploads: super::uploads::Uploads,
    failed_mut: Vec<Option<crate::failure::Failure>>,
    refresh_next: Vec<usize>,
    writes: super::writes::Writes,
    unsent: Vec<Unsent>,
    reconciling: Vec<usize>,
    pending: Vec<PendingReq>,
    queues: super::queue::Saved,
    timers: Vec<super::Timer>,
    published: Option<Box<Published>>,
    commands: usize,
    /// Socket batches of earlier commits, not yet taken (LLP 1016.000 D7):
    /// a refusal takes back the batches staged after them.
    sockets: usize,
    /// The root font size, and the window's app tuple (`setMode`).
    root_font: (super::root_font::RootFont, f32, u32),
    /// Kept answers refused before this commit (`kept::refuse`).
    refused: usize,
    /// Who each request and `then` is from (LLP 1133.023 D7).
    origins: super::modules::OriginsSaved,
    /// The arguments refused a child's resource (LLP 1133.007 D13).
    refused_args: super::modules::Refused,
}

/// What a settlement publishes that a refusal after it — the gate step
/// (LLP 1092 D8) — must put back; taken only for a plan with a gated task
/// or a sequence (LLP 1125 D2), whose gate steps are the one refusal that
/// follows a settlement that stood.
pub(super) struct Published {
    settled: Option<super::settlement::Settled>,
    derives: Vec<Option<Value>>,
    derive_store_dependent: Vec<bool>,
    requests: usize,
}

impl<D: DataSource> Runner<D> {
    /// The presenter's on-demand geometry for shaped inline runs.
    pub fn set_inline_geometry(
        &mut self,
        read: Option<std::rc::Rc<dyn crate::geometry::InlineGeometry>>,
    ) {
        self.scrolled.inline = read;
    }

    /// Before a commit, including every resource field a refusal restores.
    pub(super) fn checkpoint(&mut self) -> Checkpoint {
        self.start_commit();
        self.derives_evaluated = 0;
        self.copied_at_checkpoint = self.store.copied_bytes();
        let store = self.store.checkpoint();
        // What the presence backend moved since the last commit shows in
        // this one (LLP 1133.010 D5); a refused commit puts it back.
        self.adopt_presence();
        Checkpoint {
            store,
            slots: self.slots.clone(),
            resources: self.resources.clone(),
            module_readers: self.extensions.readers.clone(),
            uploads: self.uploads.clone(),
            failed_mut: self.failed_mut.clone(),
            refresh_next: self.refresh_next.clone(),
            writes: self.writes.clone(),
            unsent: self.unsent.clone(),
            reconciling: self.reconciling.clone(),
            pending: self.pending.clone(),
            queues: self.queues.save(),
            timers: self.timers.clone(),
            published: (self.plan.timers.iter().any(|t| t.gated || t.keyed)
                || !self.plan.sequences.is_empty())
            .then(|| {
                Box::new(Published {
                    settled: self.settled.clone(),
                    derives: self.derives.clone(),
                    derive_store_dependent: self.derive_store_dependent.clone(),
                    requests: self.requests.len(),
                })
            }),
            commands: self.commands.len(),
            sockets: self.sockets.len(),
            root_font: (
                self.root_font,
                self.kernel.root_font_size(),
                self.ids.theme.window,
            ),
            refused: self.keeping.refused.len(),
            origins: self.origins_saved(),
            refused_args: self.refused_saved(),
            letting: self.letting_len(),
        }
    }

    /// After a commit: journal the store's writes if it stood; otherwise
    /// put everything back — only the store, when this very commit poisoned
    /// the runner (its tree matches nothing to go back to).
    pub(super) fn conclude<T>(
        &mut self,
        c: Checkpoint,
        result: &Result<T, RunnerError>,
        was_poisoned: bool,
    ) {
        let refused = result.is_err();
        self.conclude_router(refused);
        match result {
            Ok(_) => {
                self.module_work.standing = None;
                self.module_work.reply_reader = None;
                self.persist_slots();
                self.journal_refused();
                self.log_store_writes(c.store.writes);
                self.transport_stood();
                self.samples_stood();
                self.invocations_let_go();
            }
            Err(e) if self.poisoned && !was_poisoned => {
                self.samples_refused();
                self.restore_store_refused(c.store);
                self.refuse_batches(c.sockets, &crate::failure::Failure::of(e).message);
            }
            Err(e) => {
                self.samples_refused();
                self.refuse_batches(c.sockets, &crate::failure::Failure::of(e).message);
                // What the refused commit asked is let go with it.
                self.forgot = true;
                let kept = self.restore_store_refused(c.store);
                self.slots = c.slots;
                let mut resources = c.resources;
                // A refused call still drew randomness; the bake must not keep its answer.
                for (old, attempted) in resources.iter_mut().zip(&self.resources) {
                    old.entropy_reader |= attempted.entropy_reader;
                }
                self.resources = resources;
                self.extensions.readers = c.module_readers;
                self.release_sampled();
                // A spent reply's turn read the store: its failure stands, and so does the read.
                if let Some(i) = self.module_work.reply_reader.take() {
                    self.resources[i].store_reader = true;
                    self.resources[i].contents_reader = true;
                }
                self.uploads = c.uploads;
                self.failed_mut = c.failed_mut;
                self.refresh_next = c.refresh_next;
                if kept {
                    self.ask_failed_readers(None);
                }
                // Ids stay unique: a rolled-back send's id is never reused.
                let seq = self.writes.seq;
                self.writes = c.writes;
                self.writes.seq = seq;
                self.resources
                    .iter_mut()
                    .map(|row| &mut row.overlay)
                    .for_each(|o| *o = None);
                self.unsent = c.unsent;
                self.reconciling = c.reconciling;
                // A send its source refused when it was sent stays ended.
                for (target, id) in std::mem::take(&mut self.refused_unsent) {
                    self.unsent.retain(|u| u.0 != target);
                    self.writes.records.retain(|r| Some(r.id) != id);
                    // And what it showed in is still read again.
                    if let Some(m) = target.mutation() {
                        self.reconcile_after(m);
                    }
                }
                self.pending = c.pending;
                self.letting_restored(c.letting);
                self.origins_restore(c.origins);
                self.refused_restore(c.refused_args);
                self.queues.restore(c.queues);
                self.timers = c.timers;
                if let Some(p) = c.published {
                    self.settled = p.settled;
                    self.derives = p.derives;
                    self.derive_store_dependent = p.derive_store_dependent;
                    // A resource request the settlement handed out goes
                    // with it: never taken, its continuation is let go.
                    let dropped: Vec<RequestOut> = self.requests.drain(p.requests..).collect();
                    for r in &dropped {
                        self.discard_request(&r.request);
                    }
                }
                self.sync_pending_flags();
                self.commands.truncate(c.commands);
                // A refusal the restore undid is not said.
                self.keeping.refused.truncate(c.refused);
                self.root_font = c.root_font.0;
                let _ = self.kernel.set_root_font_size(c.root_font.1);
                self.ids.theme.window = c.root_font.2;
            }
        }
        self.refused_unsent.clear();
        // What a refused commit's gate step read is let go (LLP 1125 D2).
        self.transport.pending = None;
        self.hear_forgotten();
        // Every commit ends so: what the source set aside is kept or let go.
        self.keep_sent(refused);
        self.sample();
    }

    /// After any restore, or a request let go outside a commit: the source
    /// hears what is really in flight (a re-ask's wait is not, `overrun.rs`).
    pub(super) fn hear_forgotten(&mut self) {
        if std::mem::take(&mut self.forgot) {
            let in_flight: Vec<InFlight<'_>> = self
                .pending
                .iter()
                .filter(|p| p.reask_at.is_none())
                .map(|p| InFlight {
                    target: p.target,
                    source: &p.source,
                    args: &p.args,
                    fetch: p.fetch,
                    continuation: p.continuation,
                })
                .collect();
            self.data.forgotten(&self.store, &in_flight);
        }
    }

    /// Run an action by name with `args` — what a test or an agent does.
    pub fn act(&mut self, name: &str, args: Vec<Value>) -> Result<CommitReceipt, RunnerError> {
        let what = format!("act {name}");
        let was_poisoned = self.poisoned;
        let result = match self
            .plan
            .actions
            .iter()
            .position(|a| self.plan.str(a.name) == name)
            .map(|i| ActionsId(i as u32))
        {
            Some(id) => self.run_action(id, args, &[]),
            None => Err(RunnerError::NoHandler {
                view: 0,
                event: "action",
            }),
        };
        self.log_outcome(&what, &result, was_poisoned);
        result
    }

    /// Run an action: its body, its sends, its writes, settlement, the
    /// update — one commit. What it kept in the store is journaled once the
    /// commit stands and rolled back with everything else when it does not
    /// (LLP 1018 D1): nothing reaches the host from a refused action.
    pub(super) fn run_action(
        &mut self,
        action: ActionsId,
        args: Vec<Value>,
        frames: &[Frame],
    ) -> Result<CommitReceipt, RunnerError> {
        let was_poisoned = self.poisoned;
        let checkpoint = self.checkpoint();
        let result = self.run_action_inner(action, args, frames);
        self.conclude(checkpoint, &result, was_poisoned);
        self.arm_then(result.is_ok());
        self.arm_next(result.is_ok());
        result
    }

    /// Arm the `then` action of each mutation answered in the commit just
    /// made, if it stood. It runs as its own commit when the host next
    /// advances the clock, which it does at once for a due time already
    /// past (`timer_due_ms`): the answer's commit is never extended by
    /// what it causes, and a refused `then` leaves the answer standing.
    pub(super) fn arm_then(&mut self, stood: bool) {
        for m in std::mem::take(&mut self.landed) {
            if stood && self.plan.mutations[m].then.is_some() {
                self.then_due[m] = self.armed_at();
                self.note_then(m);
            }
        }
    }

    /// Journal the store's writes from index `since`: the names, never the
    /// values.
    pub(super) fn log_store_writes(&mut self, since: usize) {
        let writes = self.store.writes();
        let lines: Vec<String> = writes[since.min(writes.len())..]
            .iter()
            .map(|w| super::lines::store_write(&w.name, w.value.is_some()))
            .collect();
        for line in lines {
            self.log(line);
        }
    }

    pub(super) fn run_action_inner(
        &mut self,
        action: ActionsId,
        args: Vec<Value>,
        frames: &[Frame],
    ) -> Result<CommitReceipt, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let row = self.plan.action(action).clone();
        let expected = row.params.len as usize;
        if args.len() != expected {
            return Err(RunnerError::Arity {
                action: self.plan.str(row.name).to_string(),
                expected,
                actual: args.len(),
            });
        }
        for (i, p) in row.params.iter().enumerate() {
            let param = self.plan.param(p);
            // A hidden parameter (`@capture:…`, a child's captured prop; `@`
            // begins no authored name) is the compiler's argument, not the
            // host's: it has its type, but its numbers need not be finite
            // nor its strings short, as a prop read in place needs neither.
            if self.plan.str(param.name).starts_with('@') {
                if !args[i].typed(&self.plan, param.ty) {
                    return Err(RunnerError::ArgumentType {
                        action: self.plan.str(row.name).to_string(),
                        param: self.plan.str(param.name).to_string(),
                    });
                }
                continue;
            }
            if !args[i].conforms(&self.plan, param.ty) {
                if args[i].typed(&self.plan, param.ty) {
                    return Err(RunnerError::NonFiniteValue {
                        name: self.plan.str(param.name).to_string(),
                    });
                }
                return Err(RunnerError::ArgumentType {
                    action: self.plan.str(row.name).to_string(),
                    param: self.plan.str(param.name).to_string(),
                });
            }
            if too_long(&args[i]) {
                return Err(RunnerError::StringTooLong {
                    name: self.plan.str(param.name).to_string(),
                });
            }
        }
        let allowed: Vec<u32> = row
            .writes
            .iter()
            .map(|w| self.plan.write(w).slot.0)
            .collect();
        // Geometry (LLP 1051.000 D2): every `measure` in the body is
        // answered now, with the kernel's engine tree to lay out; `frame`
        // reads through the borrowed kernel while the body runs.
        let measured;
        let geometry = match self.links.geometry {
            Some(links) => {
                measured = links.ahead(
                    &self.plan,
                    self.plan.code(row.body),
                    &mut self.kernel,
                    &self.scrolled,
                );
                Some(crate::geometry::GeometryEnv::new(
                    &self.kernel,
                    &self.scrolled,
                    links,
                    &measured,
                ))
            }
            None => None,
        };
        let outcome = {
            // The frames in force at the view the event hit (LLP 1017 P4c):
            // a row action reads and writes its row through them.
            let mut env = self.env(&args, frames);
            env.geometry = geometry.as_ref();
            vm::eval(self.plan.code(row.body), &env, &allowed)?
        };
        // Sends (LLP 1016 §4): each asks the source now. An answer lands in
        // the mutation's slot inside this commit; a request goes to the host
        // once the commit stands. A queue mutation's send is asked only when
        // the mutation is free and it is the commit's first; else it waits
        // its turn (LLP 1092 D2). A parallel one's is always asked, as its
        // own target (LLP 1161.000 D2, D5).
        let mut later: Vec<Later> = Vec::new();
        let mut answered: Vec<(u32, Value)> = Vec::new();
        let mut asked: Vec<usize> = Vec::new();
        let mut landed_now: Vec<usize> = Vec::new();
        let mut failed_now = Vec::new();
        // Newest-wins mutations whose send replaces one in flight without a
        // request of its own: an answer at once, or one waiting for the source.
        let mut replaced: Vec<usize> = Vec::new();
        // A send before the source can answer — a TypeScript module a native
        // host loads after first pixel (LLP 1027 D4) — waits for it, pending,
        // as a resource's ask does and as the web's send to a module still
        // loading does: `data_ready` sends it (notes diary: a document
        // opened at launch was refused, and lost). A queue's waits in its
        // queue instead, every send in order, its head asked once the source
        // is ready (LLP 1092 D2).
        let mut unsent: Vec<Unsent> = Vec::new();
        for (m, source, sargs) in &outcome.sends {
            let m = *m as usize;
            let mode = self.plan.mutations[m].mode;
            let queue = mode == MutationMode::Queue;
            if mode == MutationMode::Parallel {
                let more = (later.iter().map(|l| l.0)).chain(unsent.iter().map(|u| u.0));
                if let Err(e) =
                    self.parallel_bound(m, more.filter(|t| t.mutation() == Some(m)).count())
                {
                    self.discard_later(&later);
                    return Err(e);
                }
            }
            let target = self.send_target(m);
            let write = self.accept_write(target, source, sargs);
            let early = !self.source_ready(source) && !exact_plan::runner_owned_source(source);
            if early && !queue {
                if let Err(e) = self.check_send_arguments(m, sargs) {
                    self.discard_later(&later);
                    return Err(e);
                }
                // It replaces a send still in flight (newest wins).
                if mode == MutationMode::Newest {
                    unsent.retain(|u| u.0.mutation() != Some(m));
                    replaced.push(m);
                }
                unsent.push((target, source.clone(), sargs.clone(), self.sent_by()));
                continue;
            }
            if queue && (early || asked.contains(&m) || !self.asks_now(m)) {
                if let Err(e) = self.wait_turn(m, source, sargs, write) {
                    self.discard_later(&later);
                    return Err(e);
                }
                continue;
            }
            asked.push(m);
            match self.ask_send(target, source, sargs, write) {
                Ok(super::queue::Asked::Now(slot, v)) => {
                    answered.push((slot as u32, v));
                    self.landed.push(m);
                    landed_now.push(m);
                    // It replaces an earlier send of this action still to go
                    // out (newest wins): that request never goes.
                    if mode == MutationMode::Newest {
                        let (older, rest) = later.into_iter().partition(|l| l.0 == target);
                        later = rest;
                        self.discard_later(&older);
                    }
                }
                Ok(super::queue::Asked::Later(request)) => {
                    later.push((target, source.clone(), sargs.clone(), request))
                }
                Ok(super::queue::Asked::Failed) => {
                    if mode != MutationMode::Parallel {
                        let (older, rest) = later.into_iter().partition(|l| l.0 == target);
                        later = rest;
                        self.discard_later(&older);
                        self.pending_mut[m] = false;
                    }
                    failed_now.push(m);
                }
                Err(e) => {
                    self.discard_later(&later);
                    return Err(e);
                }
            }
        }
        // Commit the writes, then everything downstream. If settlement refuses
        // (a data source or shape refusal), the writes and commands roll back
        // and the kernel is exactly as it was.
        for (slot, value) in outcome
            .writes
            .iter()
            .map(|(s, v)| (s, v))
            .chain(outcome.row_writes.iter().map(|(s, v, _)| (s, v)))
        {
            let row = &self.plan.slots[*slot as usize];
            let refusal = if !self.conforms(value, row.ty) {
                Some(RunnerError::slot_type(
                    &self.plan,
                    value,
                    row.ty,
                    self.plan.str(row.name),
                ))
            } else if too_long(value) {
                Some(RunnerError::StringTooLong {
                    name: self.plan.str(row.name).to_string(),
                })
            } else {
                None
            };
            if let Some(refusal) = refusal {
                self.discard_later(&later);
                return Err(refusal);
            }
        }
        // Row writes land in their rows now, remembered for a rollback.
        let mut row_undo: Vec<(RowSlots, u32, Option<Value>)> = Vec::new();
        for (slot, value, rows) in outcome.row_writes {
            let old = rows.borrow_mut().insert(slot, value);
            row_undo.push((rows, slot, old));
        }
        let first_command = self.commands.len();
        for (slot, value) in answered {
            self.slots[slot as usize] = value;
        }
        let written: Vec<u32> = outcome.writes.iter().map(|(s, _)| *s).collect();
        for (slot, value) in outcome.writes {
            self.slots[slot as usize] = value;
        }
        for (name, args) in outcome.commands {
            self.commands.push(Command {
                name,
                args,
                source: self.input_source,
            });
        }
        // The app's chosen language, before the settlement reads its tables.
        self.take_set_locale(first_command);
        // An assignment to a mutation's slot tentatively makes it not
        // pending. The pending map is changed only after settlement stands:
        // a refused assignment did not change what reply the view wants.
        // A queue's or a parallel mutation's assignment forgets nothing:
        // every reply still lands (LLP 1092 D4, LLP 1161.000 D4).
        let assigned: Vec<usize> = (0..self.plan.mutations.len())
            .filter(|m| written.contains(&self.plan.mutations[*m].slot.0) && self.newest_wins(*m))
            .collect();
        for m in 0..self.plan.mutations.len() {
            if written.contains(&self.plan.mutations[m].slot.0) {
                self.failed_mut[m] = None;
            }
        }
        for m in asked.iter().chain(assigned.iter()) {
            if !failed_now.contains(m) {
                self.failed_mut[*m] = None;
            }
        }
        for m in &assigned {
            self.pending_mut[*m] = false;
            self.end_pending(*m, None);
        }
        // A send answered at once replaces one in flight: not pending.
        for m in &landed_now {
            if self.newest_wins(*m) {
                self.pending_mut[*m] = false;
            }
        }
        let of = |t: Target| t.mutation().expect("a send's target names its mutation");
        for m in later.iter().map(|l| of(l.0)) {
            if !assigned.contains(&m) {
                self.pending_mut[m] = true;
            }
        }
        // A send that goes now replaces one still waiting for the source
        // (newest wins)
        for m in asked
            .iter()
            .copied()
            .filter(|m| self.newest_wins(*m))
            .collect::<Vec<_>>()
        {
            self.unsent.retain(|u| u.0 != Target::Mutation(m));
        }
        // Waiting from here, so the settlement sees them pending; a refusal
        // puts the waiting sends back as they were (the checkpoint).
        for u in &unsent {
            let m = of(u.0);
            if self.newest_wins(m) {
                self.unsent.retain(|w| w.0 != u.0);
            }
            if !assigned.contains(&m) {
                self.unsent.push(u.clone());
                self.pending_mut[m] = true;
            }
        }
        // What the action refreshes, added to what is already waiting
        // (LLP 1054.000.000 D2). A mutation answered at once has landed:
        // what it changes is asked, as a reply's landing asks it
        // (`fulfill`). A send still in flight asks nothing: its write shows
        // through the overlay.
        for r in outcome.refreshes.iter() {
            self.force_refresh(*r as usize);
        }
        for m in &landed_now {
            for r in self.declared_refreshes(*m) {
                self.force_refresh(r);
            }
        }
        // What this commit replaces in flight leaves `pending` before the
        // settlement, whose flags follow the tickets: a derive or gate reads
        // it as no longer pending, as above. The checkpoint puts it back.
        let replacing: Vec<usize> = (assigned.iter().chain(&landed_now).chain(&replaced))
            .chain(&failed_now)
            .copied()
            .filter(|m| self.newest_wins(*m))
            .collect();
        let mut let_go = Vec::new();
        for m in replacing {
            if let Some(pos) = (self.pending.iter()).position(|p| p.target == Target::Mutation(m)) {
                let p = self.pending.remove(pos);
                let_go.push((p.ticket, m));
                self.note_retired(&p);
                self.let_go_request(p);
            }
        }
        // A refusal from here is put back by the checkpoint (run_action);
        // row slots live in the tree, so they are undone here. The gate
        // step joins this path (LLP 1092 D8): before `enqueue` and
        // `into_view`, which a refusal there must not leave applied.
        if let Err(e) = self
            .router_change()
            .and_then(|_| self.settle(false))
            .and_then(|_| {
                // Settlement's flags follow the tickets, and this commit's
                // requests are handed out after the step: a gate reads them
                // pending, as the settled derives did.
                for m in later.iter().map(|l| of(l.0)) {
                    self.pending_mut[m] |= !assigned.contains(&m);
                }
                self.gate_step()
            })
        {
            self.discard_later(&later);
            for (rows, slot, old) in row_undo.into_iter().rev() {
                match old {
                    Some(v) => rows.borrow_mut().insert(slot, v),
                    None => rows.borrow_mut().remove(&slot),
                };
            }
            return Err(e);
        }
        for (ticket, m) in let_go {
            self.forgot = true;
            self.log(super::lines::forgot(
                ticket,
                &self.target_name(Target::Mutation(m)),
            ));
        }
        for (rows, slot, _) in &row_undo {
            self.row_writes.record(frames, rows, *slot);
        }
        for m in 0..self.plan.mutations.len() {
            if written.contains(&self.plan.mutations[m].slot.0) {
                self.failed_mut[m] = None;
            }
        }
        for m in asked.iter().chain(assigned.iter()) {
            if !failed_now.contains(m) {
                self.failed_mut[*m] = None;
            }
        }
        for m in &assigned {
            self.forget(Target::Mutation(*m));
        }
        // A send answered at once, or waiting for the source, replaces one
        // still in flight (newest wins): the older reply is not wanted.
        for m in landed_now.into_iter().chain(replaced).chain(failed_now) {
            if self.newest_wins(m) {
                self.forget_ticket(Target::Mutation(m));
            }
        }
        self.sync_pending_flags();
        for (target, source, args, request) in later {
            self.enqueue(target, source, args, request, false);
            if assigned.contains(&of(target)) {
                self.forget(target);
            }
        }
        for m in unsent
            .iter()
            .map(|u| of(u.0))
            .filter(|m| !assigned.contains(m))
        {
            self.log(super::lines::unsent(
                self.plan.str(self.plan.mutations[m].name),
            ));
        }
        self.take_set_modes(first_command);
        // A module's sends that answer nothing (LLP 1133.004 D5): the
        // runner's, run once the commit stands.
        let effects = self.take_module_effects(first_command);
        self.refuse_in_wake(first_command);
        self.skip_in_call(first_command);
        // The scheme and root size a development host holds: kept as asked, not applied (LLP 1122 T7).
        self.take_held(first_command);
        // `setRootFontSize` is the runner's too: the size lands in this
        // commit's layout, and the command stays for a host that mirrors it;
        // a refused one is journaled as refused, never as a command.
        let mut i = first_command;
        while i < self.commands.len() {
            if self.commands[i].name == "setRootFontSize" {
                let args = self.commands[i].args.clone();
                if !self.app_root_font_size(&args) {
                    self.commands.remove(i);
                    continue;
                }
            }
            i += 1;
        }
        // A world hears whose commit posted to it (`world_origin.rs`).
        super::world_origin::mark_world_posts(
            self.call_origin(),
            &mut self.commands[first_command..],
        );
        let commands: Vec<String> = self.commands[first_command..]
            .iter()
            .map(|c| {
                let mut s = format!("command {}(", c.name);
                for (i, a) in c.args.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    crate::agent::untyped_json(a, &mut s);
                }
                s.push(')');
                s
            })
            .collect();
        // `scrollIntoView` of a list's row is the runner's own (LLP
        // 1070.000): it runs in this commit, after the update, and never
        // reaches a host. An element's (its id and three options, not the
        // row form's seven arguments) is the host's, as `focus` is.
        let stated: Vec<Command> = self.commands.drain(first_command..).collect();
        for command in stated {
            if command.name != "scrollIntoView" || command.args.len() != 7 {
                self.commands.push(command);
                continue;
            }
            match crate::instance::collection::IntoView::from_command(&command.args) {
                Ok(request) => self.into_view.push(request),
                Err(why) => self.log(format!("scrollIntoView refused: {why}")),
            }
        }
        let result = self.update();
        // Commands are journaled only once the update committed: a failure
        // there poisons the runner and clears them.
        if result.is_ok() {
            for line in commands {
                self.log(line);
            }
            self.apply_sounds(first_command);
            self.run_module_effects(effects);
        }
        result
    }

    /// Whether `value` conforms to `ty`, checking only the list items not
    /// already found conforming ([`crate::conform::Conformed`]).
    pub(super) fn conforms(&self, value: &Value, ty: exact_plan::TypesId) -> bool {
        self.conformed.borrow_mut().conforms(&self.plan, value, ty)
    }

    /// The refusal of `value`, which did not conform to `ty`.
    pub(super) fn shape(
        &self,
        resource: String,
        value: &Value,
        ty: exact_plan::TypesId,
    ) -> RunnerError {
        let why = crate::conform::mismatch(&self.plan, value, ty);
        RunnerError::Shape { resource, why }
    }

    pub(super) fn poison(&mut self) {
        self.poisoned = true;
        self.notes.clear();
        self.commands.clear();
        self.requests.clear();
        self.drop_background();
        self.forgot |= !self.pending.is_empty();
        self.pending.clear();
        self.unsent.clear();
        self.forget_waiting();
        self.sync_pending_flags();
    }

    /// Recheck the plan relation at the write boundary instead of trusting
    /// that a decoded plan is the only possible caller.
    pub(super) fn mutation_slot(&self, mutation: usize) -> Result<usize, RunnerError> {
        let mutation = MutationsId(mutation as u32);
        self.plan
            .validate_mutation_slot(mutation)
            .map_err(RunnerError::Plan)?;
        Ok(self.plan.mutation(mutation).slot.0 as usize)
    }

    /// The resources mutation `m` declares it refreshes.
    pub(super) fn declared_refreshes(&self, m: usize) -> Vec<usize> {
        let range = self.plan.mutations[m].refreshes;
        self.plan.mutation_refreshes[range.start as usize..(range.start + range.len) as usize]
            .iter()
            .map(|row| row.resource.0 as usize)
            .collect()
    }

    /// Ask resource `r` again, forced, at the next settlement that can.
    pub(super) fn force_refresh(&mut self, r: usize) {
        // A refresh ends the launch's exemption for every resource and every
        // wait, not only `r`: a list's members and whatever the refresh's cause
        // also changed are then asked fresh. The cost is a fetch a preload
        // could have answered, never an old answer (LLP 1162.000 D1).
        self.launch.clear();
        for p in &mut self.pending {
            p.launch = false;
        }
        if !self.refresh_next.contains(&r) {
            self.refresh_next.push(r);
        }
    }

    /// Ask resource `r` in place of the stand-in the launch shows: at the
    /// next settlement when its source can answer now, else at `data_ready`.
    /// The ask replaces whatever is in flight, as a refresh does, and is
    /// sent as an ordinary one (LLP 1162.000 D1).
    pub(super) fn ask_at_launch(&mut self, r: usize) {
        if !self.source_ready(self.plan.str(self.resource_row(r).source)) {
            self.resources[r].stale = true;
        } else if !self.refresh_next.contains(&r) {
            self.launch.push(r);
            self.refresh_next.push(r);
        }
    }

    /// One request uses the same replacement rules as a resource's set.
    pub(super) fn enqueue(
        &mut self,
        target: Target,
        source: String,
        args: Vec<Value>,
        request: Request,
        forced: bool,
    ) {
        self.enqueue_requests(target, source, args, vec![request], forced);
    }

    /// Replace the target's answer. Only one equal request on each side can
    /// keep its ticket when arguments move (LLP 1162 D2).
    pub(super) fn enqueue_requests(
        &mut self,
        target: Target,
        source: String,
        args: Vec<Value>,
        requests: Vec<Request>,
        forced: bool,
    ) {
        let single =
            requests.len() == 1 && self.pending.iter().filter(|p| p.target == target).count() == 1;
        let request = &requests[0];
        if single && Self::keepable(target, request) && !forced {
            if let Some(p) = self.pending.iter_mut().find(|p| {
                p.target == target
                    && !p.refused
                    && p.refusal.is_none()
                    && p.args != args
                    // Prefetch is a plain fetch (LLP 1162.000 D5); who asked
                    // is stamped as it goes out (LLP 1133.036 D6).
                    && p.keepable.as_ref().is_some_and(|k| {
                        *request
                            == Request {
                                reach: request.reach,
                                prefetch: request.prefetch,
                                ..k.clone()
                            }
                    })
            }) {
                p.args = args;
                // (new arguments: a new streak after a turn over its budget)
                p.reasks = 0;
                if let Target::Resource(i) = target {
                    p.inspection_generation = self
                        .resources
                        .get(i)
                        .map(|row| &row.inspection)
                        .map_or(0, |w| w.request);
                }
                let (ticket, upload) = (p.ticket, p.upload);
                // An upload kept for newer arguments is asked at them too (D10).
                if upload {
                    let args = p.args.clone();
                    self.upload_sent(target, &args, ticket);
                }
                // The newer arguments' causes join the request's (LLP 1133.035 D16 item 6).
                self.note_kept(target, ticket);
                // A source that parks calls hears the kept ticket's newer
                // arguments and drops the call parked under the old ones.
                self.forgot = true;
                self.log(super::lines::kept(ticket, &self.target_name(target)));
                return;
            }
        }
        if self.holds_child(target) {
            for request in &requests {
                self.discard_request(request);
            }
            return;
        }
        let reasks = self.overrun_reasks(target, &args, forced);
        self.forget_ticket(target);
        let first = self.pending.len();
        let launch = matches!(target, Target::Resource(i) if self.launch.contains(&i));
        let refresh = forced && !launch;
        for request in requests {
            self.append_request(target, source.clone(), args.clone(), request, refresh);
        }
        let origin = self.pending[first].origin;
        for p in &mut self.pending[first..] {
            p.origin = origin;
            p.reasks = reasks;
        }
    }

    fn keepable(target: Target, request: &Request) -> bool {
        matches!(target, Target::Resource(_))
            && request.continuation.is_none()
            && request.storage.is_none()
            && request.surface.is_none()
    }

    /// `refresh`: the executor bypasses what it holds for the request.
    pub(super) fn append_request(
        &mut self,
        target: Target,
        source: String,
        args: Vec<Value>,
        request: Request,
        refresh: bool,
    ) {
        let keepable = Self::keepable(target, &request);
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        let name = self.target_name(target);
        let mut request = request;
        self.number_data_call(&mut request);
        request.reach = self.reach_of(self.origin_for(Some(target)));
        self.log(super::lines::enqueued(ticket, &name, &request));
        let refusal = self.ask_refusal(target, &args, &request);
        self.note_asked(target, ticket);
        self.note_handles(ticket, &source, &request.body);
        self.note_send(ticket, &request.body);
        self.presence_noted(ticket, &request);
        self.pending.push(PendingReq {
            inspection_generation: match target {
                Target::Resource(i) => self.inspected_request(i),
                Target::Mutation(_) | Target::Sent(..) => 0,
            },
            refusal: refusal.map(|why| (why, false)),
            refused: false,
            ticket,
            target,
            source,
            args,
            continuation: request.continuation,
            fetch: request.fetch,
            keepable: keepable.then(|| request.clone()),
            stream: sockets::stream_count(&request),
            upload: request.stream && request.upload_progress,
            sent: false,
            grant: request
                .stream
                .then(|| crate::grants::admitting_request(self.data.grants(), &request))
                .flatten()
                .map(str::to_string),
            ask_again: false,
            module_again: None,
            effects: request.has_effects(),
            replay_unsafe: request.has_effects(),
            origin: self.writes.tick(),
            write: match target {
                Target::Mutation(_) | Target::Sent(..) => self.asked_write(target),
                Target::Resource(_) => None,
            },
            held: request.is_held(),
            long_poll: request.stream && request.long_poll.is_some(),
            replied: false,
            reask_at: None,
            launch: false,
            reasks: 0,
        });
        self.note_child(ticket, &request);
        if request.stream && request.upload_progress {
            let args = self
                .pending
                .last()
                .map(|p| p.args.clone())
                .unwrap_or_default();
            self.upload_sent(target, &args, ticket);
        }
        self.requests
            .extend(refusal.is_none().then_some(RequestOut {
                ticket,
                target: name,
                class: match target {
                    Target::Resource(_) if request.is_plain_http() => RequestClass::Read,
                    Target::Sent(m, _) if request.is_plain_http() => {
                        RequestClass::Parallel(m as u32)
                    }
                    _ => RequestClass::Ordered,
                },
                request,
                forced: refresh,
            }));
        // An abort heard before this request was enqueued (in the same
        // commit, or a worker's turn that landed late) names it now.
        self.match_aborts();
        self.sync_pending_flags();
    }

    pub(super) fn requests_for(
        &mut self,
        target: Target,
        first: Request,
    ) -> Result<Vec<Request>, DataError> {
        let mut requests = vec![first];
        requests.extend(self.data.more_for(target));
        if target.mutation().is_some() && requests.len() > 1 {
            for request in &requests {
                self.discard_request(request);
            }
            return Err(DataError::Unavailable(
                "a mutation's answer hands over one request at a time".into(),
            ));
        }
        Ok(requests)
    }

    pub(super) fn single_answer(
        &mut self,
        target: Target,
        answer: Result<Answer, DataError>,
    ) -> Result<Answer, DataError> {
        match answer? {
            Answer::Later(request) => self
                .requests_for(target, request)
                .map(|mut requests| Answer::Later(requests.remove(0))),
            answer => Ok(answer),
        }
    }

    /// The requests the host is to run since the last take (LLP 1016 D2).
    /// The module's background round goes out here when it has one (LLP
    /// 1097 D5), and its journal lines come in.
    pub fn take_requests(&mut self) -> Vec<RequestOut> {
        self.take_data_logs();
        self.arm_background();
        let mut requests = std::mem::take(&mut self.requests);
        // A request the app aborted before it was handed out is never sent:
        // the next pump answers it aborted.
        requests.retain(|r| !self.module_aborts.contains(&r.ticket));
        self.hold_data_timers(requests)
    }

    /// The work behind continuation `token` (LLP 1027.002 D3): a host asks
    /// on this thread, after the commit that handed the request out, so a
    /// worker's snapshot is the store as committed then.
    pub fn dispatch_work(&mut self, token: u64) -> Dispatch {
        // A module request the runner holds for the agent (LLP 1133.002 §6).
        if let Some(dispatch) = self.module_dispatch(token).or(Self::held_dispatch(token)) {
            return dispatch;
        }
        self.store.set_clock_ms(self.now_ms);
        self.data.dispatch(token, &self.store)
    }

    /// Where this app's long native calls go: the source's own slot, or,
    /// for a source with none (a Rust source), the runner's. The host
    /// installs the app module in it (LLP 1067.000 Q6, Q9).
    pub fn native_slot(&self) -> crate::Native {
        self.data.native().unwrap_or_else(|| self.native.clone())
    }

    /// The work behind a long native call ([`Request::is_native`]): hand its
    /// body to the source's native handler with the reply, on the host's
    /// worker, which returns at once — the module's own thread answers.
    pub fn native_work(&mut self, request: &Request) -> Dispatch {
        // A module request whose reply carries handles: watched as it leaves
        // the implementation (LLP 1133.009 D6).
        if let Some(dispatch) = self.handled_work(&request.body) {
            return dispatch;
        }
        // A store's presence call: the host's backend, on the store's own
        // path, never a module's (LLP 1133.010 D13).
        if let Some(dispatch) = self.presence_call(&request.body) {
            return dispatch;
        }
        // A module's name called from data code: the runner's admission, then
        // the module (LLP 1133.002 D14).
        if let Some(dispatch) = self.module_call(&request.body) {
            return dispatch;
        }
        let handler = self.native_slot().handler();
        let body = request.body.clone();
        Dispatch::Run(crate::Work::Later(Box::new(move |reply| match handler {
            Some(handler) => handler(body, reply),
            None => reply.send(crate::Outcome::Failed {
                kind: crate::FailureKind::Unsupported,
                message: "no native module here takes long calls".into(),
            }),
        })))
    }

    /// Work the source held at dispatch and the last commit releases, in
    /// order, then the waits an answer's timer released (`data_timers.rs`);
    /// a host asks after every commit and advance.
    pub fn release_work(&mut self) -> Vec<(u64, Dispatch)> {
        let mut out = self.data.release(&self.store);
        out.extend(self.module_release());
        // A wait whose answer's timer came due (`data_timers.rs`).
        out.extend(self.take_woken());
        out
    }

    /// A request a refused transaction dropped before it was handed out:
    /// its continuation token is never dispatched, so the source forgets it.
    pub(super) fn discard_request(&mut self, request: &Request) {
        if let Some(token) = request.continuation {
            self.data.discard(token);
        } else {
            // A plain request has no token: the source hears what is still
            // in flight (`forgotten`) and drops the call it parked for it.
            self.forgot = true;
        }
    }

    /// The `Later` sends an action collected before a refusal dropped them.
    pub(super) fn discard_later(&mut self, later: &[Later]) {
        for (_, _, _, request) in later {
            self.discard_request(request);
        }
    }
}
