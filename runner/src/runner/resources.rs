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
    /// Topics watched by this answer; an announcement asks this row again.
    pub(super) watching: Vec<String>,
    /// A stand-in waiting for its source to become ready.
    pub(super) stale: bool,
    /// Its next ask is the launch's own, replacing a stand-in. Nobody asked
    /// for fresh data, so the request is not sent as a refresh, and a reply
    /// fetched for this launch may answer it (LLP 1162.000 D1).
    pub(super) launch_ask: bool,
    /// No answer yet because the source is not ready: the resource reads pending.
    pub(super) awaiting: bool,
    pub(super) member: Option<each::Member>,
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
}
