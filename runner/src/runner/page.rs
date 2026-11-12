//! Host-owned page answers; never baked or carried device data.
//! @ref LLP 1069.000 D2
use super::{DataError, DataSource, Runner, RunnerError};
use crate::page::{Page, SOURCE};
use exact_kernel::CommitReceipt;
use exact_plan::{TypeKind, Value};

impl<D: DataSource> Runner<D> {
    /// What the host last said about the page.
    pub fn page(&self) -> Page {
        self.page
    }

    /// The page's visibility, connectivity, share sheet, pickers or focus
    /// changed: re-answer, in one commit, every `exactPage` resource whose
    /// shape reads a field that changed; the same facts again, or no such
    /// reader, commit nothing. Focus moves each time the person switches
    /// windows, and asks nothing of a reader of `onLine` alone (#114). Size
    /// readers are not asked.
    pub fn set_page(&mut self, page: Page) -> Result<Option<CommitReceipt>, RunnerError> {
        self.replace_fact(
            |r| &mut r.page,
            page,
            SOURCE,
            |r, previous, mut readers| {
                // The GPU tier is read through `exactViewport` (LLP 1139.003 D2).
                let tier = previous.gpu_tier != page.gpu_tier;
                readers.retain(|i| r.page_reads_changed(*i, *previous));
                if tier {
                    readers.extend(r.resource_rows().enumerate().filter_map(|(i, row)| {
                        (r.plan.str(row.source) == crate::viewport::SOURCE).then_some(i)
                    }));
                }
                // About to be suspended: a sequence stops in this commit, once it
                // stands, and the commit carries its ends to the output (LLP 1125 D4).
                let result = if page.suspended && !previous.suspended && r.transport.playing() {
                    r.transport.suspend = true;
                    r.commit_again(readers, "page").map(Some)
                } else {
                    r.recommit(readers, "page")
                };
                // The stop is the standing commit's (a recovery commit after
                // owed reads included); a call that failed as a whole made none.
                if result.is_err() {
                    r.transport.suspend = false;
                } else if previous.in_background() != page.in_background() {
                    r.samples_visibility(page.in_background());
                    r.tell_foreground();
                    r.arrival_visibility(page.hidden);
                }
                result
            },
        )
    }

    /// Whether an `exactPage` resource reads `field`: one whose shape names
    /// it, or one that is no record (refused, but asked again on every
    /// change). A host watches what no resource reads only if it wants to.
    pub fn reads_page_field(&self, field: &str) -> bool {
        self.resource_rows().any(|r| {
            let ty = self.plan.type_(r.ty);
            self.plan.str(r.source) == SOURCE
                && (ty.kind != TypeKind::Record
                    || ty
                        .fields
                        .iter()
                        .any(|f| self.plan.str(self.plan.field(f).name) == field))
        })
    }

    /// Whether resource `i`'s shape names a field `previous` answered
    /// otherwise; a shape that is no record is asked again, to be refused.
    fn page_reads_changed(&self, i: usize, previous: Page) -> bool {
        let ty = self.plan.type_(self.resource_row(i).ty);
        ty.kind != TypeKind::Record
            || ty.fields.iter().any(|f| {
                let name = self.plan.str(self.plan.field(f).name);
                previous.field(name) != self.page.field(name)
            })
    }

    pub(super) fn page_answer(&self, i: usize) -> Result<Value, DataError> {
        let row = self.resource_row(i);
        if row.args.len > 0 {
            return Err(DataError::BadArguments(format!(
                "{SOURCE} takes no arguments"
            )));
        }
        let ty = self.plan.type_(row.ty);
        if ty.kind != TypeKind::Record {
            return Err(DataError::Unavailable(format!(
                "{SOURCE} answers a record; this one is declared `{}`",
                self.plan.str(ty.name)
            )));
        }
        let mut fields = Vec::with_capacity(ty.fields.len as usize);
        for f in ty.fields.iter() {
            let name = self.plan.str(self.plan.field(f).name);
            match self.page.field(name) {
                Some(v) => fields.push(v),
                None => {
                    return Err(DataError::Unavailable(format!(
                        "{SOURCE} has no field `{name}`"
                    )))
                }
            }
        }
        Ok(Value::record(fields))
    }
}
