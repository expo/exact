//! Runtime resource rows; the compiled plan contains declarations only.
use super::*;

#[derive(Clone, Default)]
pub(crate) struct ResourceRow {
    pub(super) state: Option<ResourceState>,
    pub(super) value: Option<crate::held::Held>,
    /// Failed arguments suppress another ask until they change or refresh.
    pub(super) failed_args: Option<Vec<Value>>,
    pub(super) failed_why: Option<crate::failure::Failure>,
    pub(super) pending: bool,
    pub(super) current: bool,
    pub(super) overlay: Option<writes::OverlayCache>,
    /// Store dependence determines what the bake may keep.
    pub(super) store_reader: bool,
    /// Once randomness was drawn, even by a refused call, bake cannot keep an answer.
    pub(super) entropy_reader: bool,
    /// It read what the store holds (a secret, a file, a database, an
    /// argument derived from such a reader), not only randomness.
    pub(super) contents_reader: bool,
    /// `contents_reader` is known: it was asked in this run, or a reload
    /// carried, or a rendered page says, what its last ask read.
    pub(super) reads_known: bool,
    /// Topics watched by this answer; an announcement asks this row again.
    pub(super) watching: Vec<String>,
    /// A stand-in waiting for its source to become ready.
    pub(super) stale: bool,
    /// No answer yet because the source is not ready: the resource reads pending.
    pub(super) awaiting: bool,
    pub(super) member: Option<each::Member>,
    /// While a boot from a rendered page settles, a list's answers for the
    /// elements it makes (LLP 1048.000 D6). Boxed: every commit copies each
    /// row, and only a list's row during a boot holds them.
    pub(super) page_answers: Option<Box<each::PageAnswers>>,
    pub(super) inspection: crate::perf::ResourceWork,
}

impl<D: DataSource> Runner<D> {
    /// Runtime rows include list members; declaration walks use the plan.
    pub(super) fn resource_row(&self, i: usize) -> &exact_plan::ResourcesRow {
        if let Some(member) = &self.resources[i].member {
            &member.row
        } else {
            &self.plan.resources[i]
        }
    }

    pub(super) fn resource_rows(&self) -> impl Iterator<Item = &exact_plan::ResourcesRow> {
        (0..self.resources.len()).map(|i| self.resource_row(i))
    }

    /// Whether a change to the store asks resource `i` again: it read what
    /// the store holds, directly or through its arguments, and not only
    /// randomness, since a draw asks nothing again (LLP 1069.005 D2). A
    /// reader the plan marked, whose reads are not yet known, counts as one
    /// that read.
    pub(super) fn asks_on_store_change(&self, i: usize) -> bool {
        let row = &self.resources[i];
        row.store_reader && (row.contents_reader || !row.reads_known)
    }

    /// Whether resource `i`'s arguments read, directly or through derives, a
    /// resource a store change asks again: then so must `i`. Its arguments'
    /// provenance otherwise came from randomness alone (`id = token()`,
    /// `live = connect(id)`). Every read the arguments could make counts.
    pub(super) fn args_read_contents(&self, i: usize) -> bool {
        use crate::instance::Input;
        let deps = self.sites.deps();
        let mut work: Vec<_> = (self.resource_row(i).args.iter())
            .map(|a| deps.reads_of(&self.plan, self.plan.arg(a).expr))
            .collect();
        let mut derives = vec![false; self.plan.derives.len()];
        while let Some(reads) = work.pop() {
            for input in deps.inputs(&reads) {
                match input {
                    Input::Resource(r) | Input::PendingResource(r) | Input::FailedResource(r)
                        if r != i && self.asks_on_store_change(r) =>
                    {
                        return true;
                    }
                    Input::Derive(d) if !derives[d] => {
                        derives[d] = true;
                        work.push(deps.derives[d].clone());
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// What an ask of resource `i` read, from the store's counts before it:
    /// any read makes it the device's, and one beyond its draws of
    /// randomness a reader of what the store holds.
    pub(super) fn note_reads(&mut self, i: usize, reads_before: usize, draws_before: usize) {
        let reads = self.store.reads() - reads_before;
        let draws = self.store.entropy_draws() - draws_before;
        let row = &mut self.resources[i];
        row.store_reader |= reads > 0;
        row.entropy_reader |= draws > 0;
        row.contents_reader |= reads > draws;
        row.reads_known = true;
    }

    /// What resource `i`'s answer from a rendered page read, which the render
    /// knew: what the store holds when the page lists it in `readers` (LLP
    /// 1048.000 D6), else nothing a store change asks it again for.
    pub(super) fn seed_reads(&mut self, i: usize, contents: bool) {
        let row = &mut self.resources[i];
        row.store_reader |= contents;
        row.contents_reader = contents;
        row.reads_known = true;
    }
}
