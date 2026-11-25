//! A list's geometric edges (`reachstart`, `reachend`): offered once a
//! report reaches them, held under a covered route, deferred until the
//! state they asked for settles. The list engine's half of the list seam.
use super::*;
use crate::compare::{equivalent, equivalent_all};
use crate::list_api::CollectionEdges;
use std::fmt::Write as _;

impl<D: DataSource> Runner<D> {
    /// The edges a report on list `view` reached, offered to its handlers
    /// once each, start before end ([`Runner::collection_feedback_filled`]).
    pub(super) fn collection_edges_linked(
        &mut self,
        view: ViewId,
        edges: CollectionEdges,
        result: &mut Advanced,
    ) {
        let edge = match Some(edges) {
            Some(edges) if self.inactive(view) => {
                let tree = self.tree.as_mut().expect("booted");
                tree.rearm_collection_edge(view, edges.first);
                if !self.held_edges.contains(&view) {
                    self.held_edges.push(view);
                    let name = if edges.first == EventKind::Reachstart {
                        "reachstart"
                    } else {
                        "reachend"
                    };
                    self.log(format!(
                        "{name} view {view} waits: its list is on a covered route; it is offered when the route shows"
                    ));
                }
                None
            }
            edge => edge,
        };
        if let Some(edges) = edge {
            let mut end_after_noop = edges.end_after_noop;
            for (position, event) in [edges.first, EventKind::Reachend].into_iter().enumerate() {
                if position == 1
                    && (!end_after_noop
                        || !self
                            .tree
                            .as_mut()
                            .expect("booted")
                            .take_collection_end(view))
                {
                    break;
                }
                // Geometry can keep arriving while an async answer is retained.
                // Do not let that old answer supersede the first edge's request.
                if event == EventKind::Reachend
                    && self.deferred_edges.iter().any(|(v, _)| *v == view)
                {
                    self.tree
                        .as_mut()
                        .expect("booted")
                        .rearm_collection_edge(view, event);
                    break;
                }
                let first_ticket = self.next_ticket;
                let shown = !self.page.hidden;
                let edge = |r: &mut Self| r.arrived(|r| r.dispatch_edge(view, event));
                match self.as_shown(shown, edge) {
                    Ok((receipt, changed)) => {
                        result.receipts.push(Timed {
                            at_ms: self.now_ms,
                            receipt,
                        });
                        if position == 0 && edges.end_after_noop && changed {
                            end_after_noop = false;
                            let targets = self
                                .pending
                                .iter()
                                .filter(|p| p.ticket >= first_ticket)
                                .map(|p| p.target)
                                .collect();
                            self.deferred_edges.retain(|(v, _)| *v != view);
                            self.deferred_edges.push((view, targets));
                            if let Err(error) = self.wake_deferred_edges() {
                                self.poison();
                                result.error = Some(error);
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(tree) = self.tree.as_mut() {
                            tree.rearm_collection_edge(view, event);
                        }
                        result.error = Some(error);
                        break;
                    }
                }
            }
        }
    }

    /// Runner-only collection events use the same action transaction and journal
    /// as host events. They have no payload; their bound arguments are evaluated
    /// in the list's scope when the edge dispatches (LLP 1054.000.006).
    pub(super) fn dispatch_edge(
        &mut self,
        view: ViewId,
        kind: EventKind,
    ) -> Result<(CommitReceipt, bool), RunnerError> {
        let name = match kind {
            EventKind::Reachstart => "reachstart",
            EventKind::Reachend => "reachend",
            _ => unreachable!("collection edge"),
        };
        let mut what = format!("{name} view {view}");
        let was_poisoned = self.poisoned;
        let mut changed = false;
        let result = (|| {
            let (node, frames) = self.find(view).ok_or(RunnerError::UnknownView(view))?;
            let handler = self
                .plan
                .node(node)
                .handlers
                .iter()
                .map(|h| self.plan.handler(h))
                .find(|h| h.event == kind)
                .cloned()
                .ok_or(RunnerError::NoHandler { view, event: name })?;
            let action = handler.action;
            let _ = write!(what, " ({})", self.plan.str(self.plan.action(action).name));
            let mut args = Vec::new();
            for (k, a) in handler.args.iter().enumerate() {
                let code = self.plan.arg(a).expr;
                args.push(self.eval_argument(code, &frames, (view, node, name, k))?);
            }
            let before = EdgeState::capture(self, &frames);
            let receipt = self.run_action(action, args, &frames)?;
            changed = before.changed(self);
            Ok(receipt)
        })();
        self.log_outcome(&what, &result, was_poisoned);
        result.map(|receipt| (receipt, changed))
    }

    /// After an ordinary commit: a list whose edge waited under a covered
    /// route asks its host for a report once the route shows, which offers
    /// the edge it reaches then.
    pub(super) fn release_held_edges_linked(&mut self) -> Result<(), RunnerError> {
        for view in std::mem::take(&mut self.held_edges) {
            if !self.tree.as_ref().expect("booted").has_collection(view) {
                continue;
            }
            if self.inactive(view) {
                self.held_edges.push(view);
            } else {
                self.tree
                    .as_mut()
                    .expect("booted")
                    .wake_collection_edge(view)?;
            }
        }
        Ok(())
    }
    /// Called once per ordinary commit, after settlement. Follow targets across
    /// continuation tickets, discard unmounted owners, and wake each ready edge
    /// once. Feedback-only commits never replenish this work.
    pub(super) fn wake_deferred_edges_linked(&mut self) -> Result<(), RunnerError> {
        let tree = self.tree.as_mut().expect("booted");
        for (view, targets) in std::mem::take(&mut self.deferred_edges) {
            if !tree.has_collection(view) {
                continue;
            }
            if self.pending.iter().any(|p| targets.contains(&p.target)) {
                self.deferred_edges.push((view, targets));
            } else {
                tree.wake_collection_edge(view)?;
            }
        }
        Ok(())
    }
}

/// Capture immutable state around an edge action, never around offset-only
/// feedback. Shared values compare by identity first, so a no-op never scans
/// the contents of a resident answer. New values still compare semantically.
pub(super) struct EdgeState {
    slots: Vec<Value>,
    resources: Vec<Option<ResourceState>>,
    rows: Vec<(RowSlots, std::collections::BTreeMap<u32, Value>)>,
    pending: Vec<(Target, u64)>,
    ticket: u64,
    store: u64,
    commands: usize,
}
impl EdgeState {
    pub(super) fn capture<D: DataSource>(r: &Runner<D>, frames: &[Frame]) -> Self {
        Self {
            slots: r.slots.clone(),
            resources: r.resources.iter().map(|row| row.state.clone()).collect(),
            rows: frames
                .iter()
                .filter_map(|f| f.row.as_ref())
                .map(|row| (row.clone(), row.borrow().slots.clone()))
                .collect(),
            pending: r.pending.iter().map(|p| (p.target, p.ticket)).collect(),
            ticket: r.next_ticket,
            store: r.store.revision(),
            commands: r.commands.len(),
        }
    }
    pub(super) fn changed<D: DataSource>(&self, r: &Runner<D>) -> bool {
        self.ticket != r.next_ticket
            || self.store != r.store.revision()
            || self.commands != r.commands.len()
            || !self
                .pending
                .iter()
                .copied()
                .eq(r.pending.iter().map(|p| (p.target, p.ticket)))
            || !equivalent_all(&self.slots, &r.slots)
            || self
                .resources
                .iter()
                .zip(r.resources.iter().map(|row| &row.state))
                .any(|(a, b)| match (a, b) {
                    (None, None) => false,
                    (Some(a), Some(b)) => {
                        !equivalent_all(&a.args, &b.args)
                            || !crate::held::Held::equivalent(&a.value, &b.value)
                    }
                    _ => true,
                })
            || self.rows.iter().any(|(row, old)| {
                let now = &row.borrow().slots;
                old.len() != now.len()
                    || old
                        .iter()
                        .zip(now.iter())
                        .any(|((ak, av), (bk, bv))| ak != bk || !equivalent(av, bv))
            })
    }
}
