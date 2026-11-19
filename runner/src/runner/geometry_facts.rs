//! The geometry facts' answers and their delivery (LLP 1123 D6): each answered from the layout
//! just committed, and answered again after a later layout changes what it reads, so a branch
//! on one lays out in a later commit. A chain of such commits, from a layout no delivery caused
//! through the layouts of the commits it made, stops at [`BOUND`], D1's bound of corrections:
//! the facts hold their last answers, a development build alerts `geometry-feedback` once, and
//! the next layout with another cause starts a new chain.
use super::{CommitReceipt, DataError, DataSource, Runner, RunnerError};
use crate::native_geometry::{self as facts, Fact, Query};
use exact_plan::Value;
use std::collections::HashMap;

/// The corrections one chain of commits may make (LLP 1123 D1, D6).
pub const BOUND: u32 = 8;

/// What the runner keeps of the geometry facts.
#[derive(Default)]
pub(super) struct Facts {
    /// The `generation` each geometry resource was last answered at, by its index: what an
    /// answer now is compared at, so a later layout that changed nothing it reads is no change
    /// to it.
    answered: HashMap<usize, u64>,
    /// The kernel's count of layouts when a delivery last read them.
    read_at: u64,
    /// The commits the current chain made.
    chain: u32,
    /// Whether the last delivery committed, so the layout the next one reads is that commit's
    /// and continues the chain. A delivery that commits nothing ends the chain, and so does one
    /// that finds no layout since the last, as after a commit the host did not lay out because
    /// it moved no node.
    committed: bool,
    /// Whether this chain's `geometry-feedback` was said.
    said: bool,
}

impl<D: DataSource> Runner<D> {
    /// What geometry resource `i` reads now, as the shape it declares. Its `generation` is the
    /// kernel's count of layouts, the number of the layout it reads.
    pub(super) fn geometry_answer(&mut self, i: usize, args: &[Value]) -> Result<Value, DataError> {
        let generation = self.kernel.layouts();
        self.geometry.answered.insert(i, generation);
        self.geometry_answer_at(i, args, generation)
    }

    fn geometry_answer_at(
        &self,
        i: usize,
        args: &[Value],
        generation: u64,
    ) -> Result<Value, DataError> {
        let row = self.resource_row(i);
        let source = self.plan.str(row.source);
        let fact: Fact = match facts::query(source, args)? {
            Query::Geometry { id, space } => facts::geometry(&self.kernel, &id, &space, generation),
            Query::Chain => facts::chain(generation),
            Query::Unsettled => facts::unsettled(&self.kernel, generation),
        };
        facts::project(&self.plan, row.ty, &fact, source)
    }

    /// After a layout: answer again, in one commit, every geometry resource whose answer the
    /// layout changed, for [`Runner::refresh_geometry`]. `None` when no layout came since the
    /// last delivery, when nothing changed, or when the chain reached [`BOUND`]. A band or
    /// margins reported since the last layout reach the facts with the frames the next layout
    /// makes, never with the last one's. An idle resource is asked nothing, as a refresh asks it
    /// nothing; one whose answer is refused is asked again, so its commit says why.
    pub(super) fn deliver_geometry(&mut self) -> Result<Option<CommitReceipt>, RunnerError> {
        let layouts = self.kernel.layouts();
        if layouts == self.geometry.read_at {
            self.geometry.committed = false;
            return Ok(None);
        }
        self.geometry.read_at = layouts;
        // Start a chain at a layout no delivery's commit made: an external change, or the first
        if !std::mem::take(&mut self.geometry.committed) {
            self.geometry.chain = 0;
            self.geometry.said = false;
        }
        let changed: Vec<usize> = (0..self.resources.len())
            .filter(|&i| facts::is_geometry(self.plan.str(self.resource_row(i).source)))
            .filter(|&i| {
                let Some(state) = self.resources[i].state.as_ref() else {
                    return false;
                };
                if state.asleep || state.placeholder {
                    return false;
                }
                let at = self.geometry.answered.get(&i).copied();
                let now = self.geometry_answer_at(i, &state.args, at.unwrap_or(layouts));
                now.map_or(true, |now| &now != state.value.get(&self.plan))
            })
            .collect();
        if changed.is_empty() {
            return Ok(None);
        }
        if self.geometry.chain >= BOUND {
            if self.live.admitted && !std::mem::replace(&mut self.geometry.said, true) {
                self.alert(self.geometry_feedback(&changed));
            }
            return Ok(None);
        }
        self.geometry.chain += 1;
        let result = self.recommit(changed, "geometry");
        self.geometry.committed = matches!(result, Ok(Some(_)));
        result
    }

    /// `geometry-feedback`: what moved after each answer, and what to do about it.
    fn geometry_feedback(&self, changed: &[usize]) -> String {
        let names: Vec<String> = changed
            .iter()
            .map(|&i| format!("`{}`", self.plan.str(self.resource_row(i).name)))
            .collect();
        format!(
            "geometry-feedback: {} changed after each of {BOUND} commits that answered them again, so what they read moves with their answers; they hold their last answers until a layout with another cause. Keep a branch on a geometry fact from moving the box it reads (LLP 1123 D6)",
            names.join(", ")
        )
    }
}
