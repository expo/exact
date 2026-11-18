//! Sends that run together (LLP 1161.000): a `parallel` mutation asks every
//! send when it is made, and each is its own target at the data seam,
//! [`Target::Sent`], numbered in send order. Replies land in any order; the
//! slot and the failure follow the latest send that has concluded (D4).

use super::*;

impl<D: DataSource> Runner<D> {
    /// The target a send of `m` made now answers: a parallel mutation's next
    /// send number, or the mutation itself.
    pub(super) fn send_target(&mut self, m: usize) -> Target {
        if self.mutation_row(m).mode != MutationMode::Parallel {
            return Target::Mutation(m);
        }
        self.queues.sends += 1;
        Target::Sent(m, self.queues.sends)
    }

    /// The send `target` concludes: its reply landed or it failed without
    /// one. Whether it is the latest of its mutation to conclude, so its
    /// outcome reaches the slot or the failure (D4): a send concluding
    /// after a later one has changes neither. Always, for any send but a
    /// parallel one's.
    pub(super) fn conclude_send(&mut self, target: Target) -> bool {
        let Target::Sent(m, n) = target else {
            return true;
        };
        if self.queues.concluded[m] > n {
            return false;
        }
        self.queues.concluded[m] = n;
        true
    }

    /// Whether a later send of `target`'s parallel mutation has already
    /// concluded, so that this one's answer leaves the slot and the failure
    /// (D4). [`Self::conclude_send`] would say no for it.
    pub(super) fn concluded_after(&self, target: Target) -> bool {
        matches!(target, Target::Sent(m, n) if self.queues.concluded[m] > n)
    }

    /// Refuse a send of parallel mutation `m` that would be the 65th in
    /// flight, counting `more` of this commit not yet handed out (D2), as
    /// a queue refuses its 65th waiting send.
    pub(super) fn parallel_bound(&self, m: usize, more: usize) -> Result<(), RunnerError> {
        let out = (self.pending.iter())
            .filter(|p| matches!(p.target, Target::Sent(s, _) if s == m))
            .count()
            + (self.unsent.iter())
                .filter(|u| u.0.mutation() == Some(m))
                .count();
        if out + more < queue::QUEUE_BOUND {
            return Ok(());
        }
        Err(RunnerError::QueueFull {
            mutation: self.mutation_name(m).to_string(),
        })
    }

    /// Whether `m` sends newest-wins: a send, an answer at once or an
    /// assignment lets go of its request in flight.
    pub(super) fn newest_wins(&self, m: usize) -> bool {
        self.mutation_row(m).mode == MutationMode::Newest
    }
}
