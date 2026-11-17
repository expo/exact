//! A lone worker is one ordered set (LLP 1027.002 Change 2): everything that
//! reads or writes the Store on its owner takes one reservation at a time,
//! in order, until its reply has been parsed or its call let go.
use super::*;

/// What holds the Store while its reply is outstanding: an owner turn (its
/// token and call), or a kept reply's delivery (its kept token; its
/// continuation maps to it in `deliveries_of`).
pub(super) enum Running {
    Turn { token: u64, key: Key },
    Delivery { kept: u64 },
}

/// What waits for the Store, in order: a turn the host dispatched, or a kept
/// reply to send to the owner when its turn comes.
pub(super) enum Waiting {
    Turn(u64),
    Delivery {
        kept: u64,
        child: u64,
        outcome: Outcome,
        reply: Reply,
    },
}

impl<D: DataSource + 'static> Placed<D> {
    /// Whether this proxy skips the order (a queue-mechanics test).
    pub(super) fn unordered(&self) -> bool {
        #[cfg(test)]
        return self.unordered;
        #[cfg(not(test))]
        false
    }

    /// Whether turn `token` must wait: something holds the Store, or
    /// something dispatched before it still waits.
    pub(super) fn waits(&self, token: u64) -> bool {
        self.running.is_some()
            || self
                .held
                .front()
                .is_some_and(|w| !matches!(w, Waiting::Turn(t) if *t == token))
    }

    /// Turn `token` holds the Store (`dispatch`), or waits its turn.
    pub(super) fn reserve(&mut self, token: u64) -> bool {
        let Some(call) = self.recorded.get(&token) else {
            return true;
        };
        if self.waits(token) {
            if !self
                .held
                .iter()
                .any(|w| matches!(w, Waiting::Turn(t) if *t == token))
            {
                self.held.push_back(Waiting::Turn(token));
            }
            return false;
        }
        let key = recorded_key(call);
        if matches!(self.held.front(), Some(Waiting::Turn(t)) if *t == token) {
            self.held.pop_front();
        }
        self.running = Some(Running::Turn { token, key });
        true
    }

    /// Turn `token`'s reply was parsed, or its continuation discarded.
    pub(super) fn turn_ended(&mut self, token: u64) {
        if matches!(self.running, Some(Running::Turn { token: t, .. }) if t == token) {
            self.running = None;
        }
    }

    /// The delivery of kept reply `kept` ended: its envelope came back to
    /// `module_delivery`, or its continuation was discarded.
    pub(super) fn delivery_ended(&mut self, kept: u64) {
        if matches!(self.running, Some(Running::Delivery { kept: k, .. }) if k == kept) {
            self.running = None;
        }
        self.held
            .retain(|w| !matches!(w, Waiting::Delivery { kept: k, .. } if *k == kept));
        self.deliveries_of.retain(|_, k| *k != kept);
    }

    /// A kept reply's delivery: sent to the owner now if nothing holds or
    /// waits for the Store, else in its turn (`release_next`).
    pub(super) fn deliver_in_turn(
        &mut self,
        (kept, continuation, child): (u64, u64, u64),
        outcome: Outcome,
        reply: Reply,
        store: &Store,
    ) {
        self.deliveries_of.insert(continuation, kept);
        if self.running.is_some() || !self.held.is_empty() {
            self.held.push_back(Waiting::Delivery {
                kept,
                child,
                outcome,
                reply,
            });
        } else {
            self.deliver((kept, child), outcome, reply, store);
        }
    }

    /// Send a kept reply to the owner against the Store as it is now; it
    /// holds the Store until its envelope comes back.
    fn deliver(
        &mut self,
        (kept, child): (u64, u64),
        outcome: Outcome,
        reply: Reply,
        store: &Store,
    ) {
        let Some(owner) = self.owner.clone() else {
            return;
        };
        self.running = Some(Running::Delivery { kept });
        let unsent = owner.lock().unwrap().turn(Job::KeptReply {
            token: child,
            outcome,
            snapshot: envelope::snapshot(store, &self.grants),
            clock: store.clock_ms(),
            reply,
        });
        drop(unsent);
    }

    /// What waits next, once nothing holds the Store: a held turn through
    /// `dispatch`, or a waiting delivery sent to the owner.
    pub(super) fn release_next(&mut self, store: &Store, out: &mut Vec<(u64, Dispatch)>) {
        let recorded = &self.recorded;
        self.held
            .retain(|w| !matches!(w, Waiting::Turn(t) if !recorded.contains_key(t)));
        if self.running.is_some() {
            return;
        }
        match self.held.front() {
            Some(Waiting::Turn(token)) => {
                let token = *token;
                out.push((token, DataSource::dispatch(self, token, store)));
            }
            Some(Waiting::Delivery { .. }) => {
                let Some(Waiting::Delivery {
                    kept,
                    child,
                    outcome,
                    reply,
                }) = self.held.pop_front()
                else {
                    unreachable!("the front is a delivery")
                };
                self.deliver((kept, child), outcome, reply, store);
            }
            None => {}
        }
    }

    /// After a let-go: the running turn ends if its targeted call is gone or
    /// replaced by a newer recorded answer, as a `Mixed` set's does; an
    /// untargeted call (a tool's) is never let go. `live` is what the runner
    /// still has in flight, by key.
    pub(super) fn let_go(&mut self, live: &HashMap<Key, Option<u64>>) {
        let gone = matches!(&self.running, Some(Running::Turn { token, key }) if key.0.is_some()
        && match live.get(key) {
            None => true,
            Some(Some(t)) => t != token && matches!(self.recorded.get(t), Some(Recorded::Answer { .. })),
            Some(None) => false,
        });
        if gone {
            self.running = None;
        }
        let recorded = &self.recorded;
        self.held
            .retain(|w| !matches!(w, Waiting::Turn(t) if !recorded.contains_key(t)));
    }

    /// The held turns whose call the runner keeps in flight under its key
    /// alone (a stream mapper's continuation): kept through a let-go.
    pub(super) fn held_live(&self, live: &HashMap<Key, Option<u64>>) -> HashSet<u64> {
        self.held
            .iter()
            .filter_map(|w| match w {
                Waiting::Turn(t) => Some(*t),
                Waiting::Delivery { .. } => None,
            })
            .filter(|t| {
                self.recorded
                    .get(t)
                    .is_some_and(|call| live.get(&recorded_key(call)) == Some(&None))
            })
            .collect()
    }

    /// The generation failed: nothing it held or queued will reply.
    pub(super) fn clear_order(&mut self) {
        self.running = None;
        self.held.clear();
        self.deliveries_of.clear();
    }
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
