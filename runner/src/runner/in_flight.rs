//! What the agent sees in flight: the requests out by target and ticket
//! (`state.pending`), what `settle` waits on, what the server holds
//! (`clock held`) and the open streams (`state.streams`).

use super::*;

impl<D: DataSource> Runner<D> {
    /// Whether resource `i` has a stream open that has committed a value: it is not
    /// pending, but a newer answer must still let its ticket go.
    pub(super) fn streaming(&self, i: usize) -> bool {
        self.pending
            .iter()
            .any(|p| p.target == Target::Resource(i) && !p.in_flight())
    }

    /// The flags `current(x)` reads (LLP 1161.001 D1), from the shown
    /// answers and the arguments of the last settlement.
    pub(super) fn sync_current_flags(&mut self) {
        let now = self.settled.as_ref().map(|s| &s.resource_args);
        for i in 0..self.resources.len() {
            let args = now.and_then(|a| a.get(i)).map_or(&[][..], Vec::as_slice);
            self.resources[i].current = self.is_current(i, self.resources[i].state.as_ref(), args);
        }
        // A family is current when every member it lists is.
        for i in 0..self.resources.len() {
            if self.is_family(i) {
                let members = (0..self.resources.len())
                    .filter(|&m| self.member(m).is_some_and(|x| x.family == i && !x.gone));
                let all = members
                    .collect::<Vec<_>>()
                    .iter()
                    .all(|&m| self.resources[m].current);
                self.resources[i].current = all;
            }
        }
    }

    /// Whether `state` answers resource `i`'s arguments `now`: an answer, not
    /// its placeholder, asked with the call arguments it has now. `with`
    /// values are context, not identity (LLP 1027.005 D1), so they are not
    /// compared.
    pub(super) fn is_current(
        &self,
        i: usize,
        state: Option<&ResourceState>,
        now: &[Value],
    ) -> bool {
        let Some(s) = state else { return false };
        if s.placeholder || s.slept {
            return false;
        }
        let context = self.resource_row(i).context as usize;
        let identity = |a: &[Value]| a.len().saturating_sub(context).min(a.len());
        let asked = s.answered_args();
        asked[..identity(asked)] == now[..identity(now)]
    }

    /// The pending flags `pending(x)` reads, from the tickets in flight and
    /// what waits for its source.
    pub(super) fn sync_pending_flags(&mut self) {
        for row in &mut self.resources {
            row.pending = false;
        }
        self.pending_mut = vec![false; self.plan.mutations.len()];
        // A stream is pending until its first value commits (LLP 1016.000 D1).
        for p in self.pending.iter().filter(|p| p.in_flight()) {
            match p.target {
                Target::Resource(i) => self.resources[i].pending = true,
                Target::Mutation(m) | Target::Sent(m, _) => self.pending_mut[m] = true,
            }
        }
        // A queue's waiting send is pending too (LLP 1092 D4).
        for m in 0..self.pending_mut.len() {
            self.pending_mut[m] |= self.queues.waits(m);
        }
        // A placeholder shown until the source can answer is pending too,
        // and so is a send waiting for it.
        for row in &mut self.resources {
            row.pending |= row.awaiting;
        }
        for m in self.unsent.iter().filter_map(|u| u.0.mutation()) {
            self.pending_mut[m] = true;
        }
        self.pending_families();
        self.sync_current_flags();
    }

    /// A target's resource or mutation name.
    pub(super) fn target_name(&self, t: Target) -> String {
        match t {
            Target::Resource(i) => self.plan.str(self.resource_row(i).name).to_string(),
            Target::Mutation(m) | Target::Sent(m, _) => {
                self.plan.str(self.plan.mutations[m].name).to_string()
            }
        }
    }

    /// Drop the requests in flight for `target`: their replies are dropped too
    /// (a `POST` already sent is not unsent — LLP 1016 D5).
    pub(super) fn forget(&mut self, target: Target) {
        self.forget_ticket(target);
        if let Target::Mutation(_) = target {
            self.unsent.retain(|u| u.0 != target);
        }
        self.sync_pending_flags();
    }

    /// Let go of `target`'s requests in flight, keeping a send that waits
    /// for the source; `false` keeps a child (LLP 1133.007 D13).
    pub(super) fn forget_ticket(&mut self, target: Target) -> bool {
        if self.holds_child(target) {
            return false;
        }
        while let Some(pos) = self.pending.iter().position(|p| p.target == target) {
            let p = self.pending.remove(pos);
            self.note_retired(&p);
            let t = p.ticket;
            self.note_let_go(t, target);
            self.let_go_request(p);
            self.forgot = true;
            self.log(super::lines::forgot(t, &self.target_name(target)));
        }
        true
    }

    /// Every request in flight: the resource's or mutation's name and its ticket.
    pub fn pending(&self) -> Vec<(String, u64)> {
        let mut out: Vec<(String, u64)> = self
            .pending
            .iter()
            .filter(|p| !self.device_holds.iter().any(|h| h.ticket == p.ticket))
            .map(|p| (self.target_name(p.target), p.ticket))
            .collect();
        out.extend(
            self.background_ticket()
                .map(|t| (super::background::NAME.to_string(), t)),
        );
        out.extend(self.tool_tickets());
        out.extend(self.module_work_tickets());
        out.extend(self.mapper_turn_tickets());
        out
    }

    /// Work an answer started that the app must finish before it is
    /// suspended or ends: a request with effects in flight, an answer's, a
    /// kept one or a handler's (LLP 1146 D2), or the module's storage still
    /// landing (LLP 1097 D10). A host keeps the app running while there is
    /// (an iOS background task, a quit held), so a reply lands and what its
    /// turn writes is persisted.
    pub fn must_finish(&self) -> bool {
        self.effects_in_flight() || self.background_operations() > 0
    }

    /// Whether a request with effects the host has not refused is in flight
    /// (`PendingReq::admitted_effects`).
    pub fn effects_in_flight(&self) -> bool {
        self.module_work_effects()
            || self.pending.iter().any(PendingReq::admitted_effects)
            || self.tool_effects()
    }

    /// Whether any request is in flight (the agent's `settle` waits on it).
    /// An open stream counts until its first value commits (LLP 1016.000
    /// D5): after that it is open, not in flight, or `settle` never ends.
    /// A request held for the agent (LLP 1069.007 D3) is not I/O either: no
    /// clock waits on it, nor on one the server holds (LLP 1016.000 D9,
    /// [`Runner::held_requests`]), nor an answer's wait for its own timer
    /// (`data_timers.rs`). The module's background round is (LLP 1097 D9).
    pub fn has_pending(&self) -> bool {
        self.background_ticket().is_some()
            || (self.tool_tickets().iter()).any(|(_, t)| !self.data_timer_held(*t))
            || !self.module_work_tickets().is_empty()
            || !self.mapper_turn_tickets().is_empty()
            // A child's outcomes its work owes on the agent's clock (LLP 1133.007 §7).
            || self.work_owed()
            || self
                .pending
                .iter()
                .any(|p| p.waited() && !self.held_for_agent(p) && !self.data_timer_held(p.ticket))
    }

    /// Whether one-shot work is armed (LLP 1101.003 D2): an `after` timer
    /// not yet fired (a spent one is set to infinity), a mutation's `then`,
    /// or a queued mutation's next send. A repeating timer is not: an
    /// `every` spinner never ends, so it cannot hold a run open.
    pub fn one_shot_armed(&self) -> bool {
        (self.timers.iter().zip(&self.plan.timers))
            .any(|(t, row)| row.once && t.next_ms.is_finite())
            || self.then_due.iter().any(|due| due.is_finite())
            || self.queues.next_due.iter().any(|due| due.is_finite())
    }

    /// Replies landed for held requests, by target, whatever they said (LLP
    /// 1016.000 D9): what `clock held` waits on to rise. A request let go
    /// (superseded, forgotten) never counts: its late reply is dropped.
    pub fn held_replies(&self) -> &[(String, u64)] {
        &self.held_replies
    }

    /// The requests the server holds (LLP 1016.000 D9): in flight, but no
    /// clock waits on them; each reply that leaves them out names them, as
    /// `state.pending` does (`held: true`).
    pub fn held_requests(&self) -> Vec<(String, u64)> {
        self.pending
            .iter()
            .filter(|p| (p.in_flight() && p.held || p.replied) && !self.held_for_agent(p))
            .map(|p| (self.target_name(p.target), p.ticket))
            .collect()
    }

    /// A request held for the agent: a device's, or a module's (LLP 1133.002 §6).
    fn held_for_agent(&self, p: &PendingReq) -> bool {
        self.device_holds.iter().any(|h| h.ticket == p.ticket)
            || self.extensions.holds_token(p.continuation)
            || self.presence_holds(p.ticket)
    }

    /// The requests in flight, as `has_pending` counts them: the agent's
    /// `state.pending` (a held request is listed there under `device`).
    /// The module's background round is listed last, as `background`.
    pub fn in_flight(&self) -> Vec<(String, u64)> {
        let mut out: Vec<(String, u64)> = self
            .pending
            .iter()
            .filter(|p| p.waited() && !self.held_for_agent(p) && !self.data_timer_held(p.ticket))
            .map(|p| (self.target_name(p.target), p.ticket))
            .collect();
        // Child processes' outcomes their work owes on the agent's clock
        // (LLP 1133.007 §7), listed as `processes`.
        if self.work_owed() {
            out.push(("processes".into(), 0));
        }
        out.extend(
            self.background_ticket()
                .map(|t| (super::background::NAME.to_string(), t)),
        );
        out.extend((self.tool_tickets().into_iter()).filter(|(_, t)| !self.data_timer_held(*t)));
        out.extend(self.module_work_tickets());
        out.extend(self.mapper_turn_tickets());
        out
    }

    /// Whether request `ticket` is a stream that has answered: open, no
    /// longer in flight (LLP 1016.000 D5). An upload is in flight until its
    /// reply (D10).
    pub fn is_open_stream(&self, ticket: u64) -> bool {
        (self.pending.iter()).any(|p| p.ticket == ticket && !p.in_flight())
    }

    /// Every open stream: its resource, ticket, and counts (the agent's
    /// `state.streams`, LLP 1016.000 D5).
    pub fn streams(&self) -> Vec<(String, u64, StreamCount)> {
        self.pending
            .iter()
            .filter_map(|p| Some((self.target_name(p.target), p.ticket, p.stream?)))
            .collect()
    }

    /// The grant line that admitted stream `ticket`, when one of the app's
    /// does (`state.streams`).
    pub fn stream_grant(&self, ticket: u64) -> Option<&str> {
        let p = self.pending.iter().find(|p| p.ticket == ticket)?;
        p.grant.as_deref()
    }
}
