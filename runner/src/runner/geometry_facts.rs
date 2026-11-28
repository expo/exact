//! The geometry facts' answers and their delivery (LLP 1123 D6): each answered from the layout
//! just committed, and answered again after a later layout changes what it reads, so a branch
//! on one lays out in a later commit. A host that marks the person's acts
//! ([`Runner::mark_inputs`]) gives the session one budget of [`BOUND`] such corrections from its
//! boot, which only an identified independent input renews: the facts then hold their last
//! answers with their comparison owed, and a development build alerts `geometry-feedback` once.
//! Without that mark, a chain of commits from a layout no delivery caused through the layouts
//! of the commits it made stops at [`BOUND`], and the next layout with another cause starts a
//! new chain.
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
    /// The layouts a page that lays out has reported ([`Runner::refresh_geometry`]), which the
    /// facts count in place of the kernel's there.
    page_layouts: u64,
    /// The host marks the person's acts ([`Runner::mark_inputs`]): the session keeps one budget.
    marked: bool,
    /// The corrections the session's budget has left.
    left: u32,
    /// A comparison the spent budget held: the next renewal delivers it, at the same layout
    /// count too.
    owed: bool,
    /// A person's act's own dispatch is being made: its completion renews, not its commit.
    in_act: bool,
    /// What the commit being made applied, under its own cause.
    applied: Option<Applied>,
    /// A delivery's commit is being made, and whether it stood once concluded: a refusal its
    /// own commit recovers from returns that recovery's receipt, which is not the delivery's.
    delivering: Option<Option<bool>>,
    /// A delivery was refused: the next renewal compares again, at the same layout count too.
    refused: bool,
}

/// What a commit applied, read under its own cause when it applied it: whether it invalidated
/// layout, whether that cause traces to an identified independent input or the boot, and
/// whether it was a person's act's own dispatch.
#[derive(Clone, Copy)]
struct Applied {
    moved: bool,
    independent: bool,
    in_act: bool,
}

impl<D: DataSource> Runner<D> {
    /// The host marks the person's acts where only a person can begin them (`Event::Act`, and
    /// `Event::ActEnd` at each act's end; LLP 1123 D6): the session keeps one budget of
    /// [`BOUND`] corrections, full from now, which only an identified independent input renews.
    pub fn mark_inputs(&mut self, on: bool) {
        let g = &mut self.geometry;
        (g.marked, g.left, g.owed, g.refused) = (on, BOUND, false, false);
    }

    /// An identified independent input: a person's act ended, or a named action ran. The
    /// budget is full again, and a comparison a refusal left is owed. Only a delivery reads the
    /// budget, after the input's own layout, so the renewal covers that layout.
    pub(super) fn renew_geometry(&mut self) {
        let g = &mut self.geometry;
        if g.marked {
            (g.left, g.said) = (BOUND, false);
            g.owed |= std::mem::take(&mut g.refused);
        }
    }

    /// Run `f`, a person's act's own dispatch, whose commit does not renew the budget: the
    /// act's completion does, after the act's work has landed.
    pub(super) fn in_act<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let was = std::mem::replace(&mut self.geometry.in_act, true);
        let out = f(self);
        self.geometry.in_act = was;
        out
    }

    /// A commit begins: nothing applied outside it counts as its own.
    pub(super) fn begin_geometry(&mut self) {
        self.geometry.applied = None;
    }

    /// A commit's kernel receipt, before the commit stands, read under that commit's own
    /// cause: a cause the runner puts back before the commit concludes (a reply's, a stream
    /// message's) is not the one its renewal reads.
    pub(super) fn note_receipt(&mut self, receipt: &CommitReceipt) {
        let independent = self.cause().independent();
        let in_act = self.geometry.in_act;
        let applied = self.geometry.applied.get_or_insert(Applied {
            moved: false,
            independent,
            in_act,
        });
        applied.moved |= receipt.layout_invalidated;
    }

    /// The commit being made stood, or not: one traced to an identified independent input or
    /// the boot (a reply, a timer, `data_ready`) whose own commit invalidated layout renews
    /// the budget; the act's own dispatch does not. A delivery's own commit records whether it
    /// stood.
    pub(super) fn conclude_geometry(&mut self, stood: bool) {
        let applied = self.geometry.applied.take();
        if let Some(outcome @ None) = self.geometry.delivering.as_mut() {
            *outcome = Some(stood);
        }
        let Some(a) = applied.filter(|_| stood) else {
            return;
        };
        if a.moved && a.independent && !a.in_act {
            self.renew_geometry();
        }
    }

    /// What geometry resource `i` reads now, as the shape it declares. Its `generation` is the
    /// number of the layout it reads, the kernel's or the page's ([`Runner::count_page_layout`]).
    pub(super) fn geometry_answer(&mut self, i: usize, args: &[Value]) -> Result<Value, DataError> {
        let generation = self.facts_layouts();
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
        let fact: Fact = match (facts::query(source, args)?, self.page_box()) {
            (Query::Geometry { id, space, .. }, Some(read)) => {
                facts::page_geometry(&self.kernel, &id, &space, generation, read)
            }
            (
                Query::Geometry {
                    id,
                    space,
                    inactive,
                },
                None,
            ) => facts::geometry(&self.kernel, &id, &space, inactive, generation),
            (Query::Chain { id }, _) => facts::chain(&self.kernel, &id, generation),
            (Query::Unsettled, _) => facts::unsettled(&self.kernel, generation),
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
        if self.geometry.marked {
            return self.deliver_budgeted();
        }
        let layouts = self.facts_layouts();
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
        let changed = self.changed_geometry(layouts);
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

    /// [`Self::deliver_geometry`] under the session's one budget: a comparison is made at a new
    /// layout or where one is owed; at a spent budget it is held, owed, and alerted once.
    fn deliver_budgeted(&mut self) -> Result<Option<CommitReceipt>, RunnerError> {
        let layouts = self.facts_layouts();
        let g = &mut self.geometry;
        if layouts == g.read_at && !g.owed {
            return Ok(None);
        }
        g.read_at = layouts;
        // This comparison supersedes any refused before it.
        g.refused = false;
        let changed = self.changed_geometry(layouts);
        let g = &mut self.geometry;
        if changed.is_empty() {
            g.owed = false;
            return Ok(None);
        }
        if g.left == 0 {
            g.owed = true;
            if self.live.admitted && !std::mem::replace(&mut self.geometry.said, true) {
                self.alert(self.geometry_feedback(&changed));
            }
            return Ok(None);
        }
        // A refused correction is said by its refusal, charges nothing and is compared again
        // at the next layout or renewal.
        g.owed = false;
        g.delivering = Some(None);
        let result = self.recommit(changed, "geometry");
        let g = &mut self.geometry;
        match g.delivering.take() {
            Some(Some(true)) => g.left -= 1,
            _ => g.refused = true,
        }
        result
    }

    /// The geometry resources whose answer `layouts` changed.
    fn changed_geometry(&self, layouts: u64) -> Vec<usize> {
        (0..self.resources.len())
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
            .collect()
    }

    /// Count a layout of a page that lays out and whose kernel does not (the web's wasm target),
    /// which calls [`Runner::refresh_geometry`] after each: the facts number their answers by
    /// these, as by the kernel's layouts elsewhere.
    pub(super) fn count_page_layout(&mut self) {
        if self.page_box().is_some() {
            self.geometry.page_layouts += 1;
        }
    }

    /// The layouts the facts read: the page's where it lays out, else the kernel's.
    fn facts_layouts(&self) -> u64 {
        match self.page_box() {
            Some(_) => self.geometry.page_layouts,
            None => self.kernel.layouts(),
        }
    }

    fn page_box(&self) -> Option<crate::geometry::PageBoxRead> {
        self.links.geometry.and_then(|g| g.page_box)
    }

    /// `geometry-feedback`: what moved after each answer, and what to do about it.
    fn geometry_feedback(&self, changed: &[usize]) -> String {
        let names: Vec<String> = changed
            .iter()
            .map(|&i| format!("`{}`", self.plan.str(self.resource_row(i).name)))
            .collect();
        let names = names.join(", ");
        match self.geometry.marked {
            true => format!(
                "geometry-feedback: the session's {BOUND} geometry corrections are spent, so {names} hold their last answers until the next identified independent input. A branch on a geometry fact that moves the box it reads spends them (LLP 1123 D6)"
            ),
            false => format!(
                "geometry-feedback: {names} changed after each of {BOUND} commits that answered them again, so what they read moves with their answers; they hold their last answers until a layout with another cause. Keep a branch on a geometry fact from moving the box it reads (LLP 1123 D6)"
            ),
        }
    }
}
