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
        if page == self.page {
            return Ok(None);
        }
        let previous = std::mem::replace(&mut self.page, page);
        // The GPU tier is read through `exactViewport` (LLP 1139.003 D2).
        let tier = previous.gpu_tier != page.gpu_tier;
        let which: Vec<usize> = (0..self.plan.resources.len())
            .filter(|i| {
                let source = self.plan.str(self.plan.resources[*i].source);
                (source == SOURCE && self.page_reads_changed(*i, previous))
                    || (tier && source == crate::viewport::SOURCE)
            })
            .collect();
        // About to be suspended: a sequence stops in this commit, once it
        // stands, and the commit carries its ends to the output (LLP 1125 D4).
        let result = if page.suspended && !previous.suspended && self.transport.playing() {
            self.transport.suspend = true;
            self.commit_again(which, "page").map(Some)
        } else {
            self.recommit(which, "page")
        };
        // The stop is the standing commit's (a recovery commit after
        // owed reads included); a call that failed as a whole made none.
        if result.is_err() {
            self.transport.suspend = false;
            self.page = previous;
        } else if previous.in_background() != page.in_background() {
            self.samples_visibility(page.in_background());
            self.tell_foreground();
            self.arrival_visibility(page.hidden);
        }
        result
    }

    /// The `navigationKey` of each route a person can see and use now (LLP 1127 D8): what
    /// `exactPage().shownRoutes` answers.
    pub fn shown_routes(&self) -> Vec<String> {
        crate::head::shown_routes(&self.kernel, &self.roots())
    }

    /// Ask every `exactPage` reader of `shownRoutes` again, in one commit, when the routes a person can
    /// see changed: after a host's report of how its columns show them (a fold) and after a commit
    /// that moved the navigation. `None` when nothing reads it or the routes are the same.
    pub fn refresh_shown_routes(&mut self) -> Result<Option<CommitReceipt>, RunnerError> {
        if !self.reads_page_field("shownRoutes") {
            return Ok(None);
        }
        let now = self.shown_routes();
        if now == self.shown_routes {
            return Ok(None);
        }
        self.shown_routes = now;
        let which: Vec<usize> = (0..self.plan.resources.len())
            .filter(|&i| {
                let r = &self.plan.resources[i];
                let ty = self.plan.type_(r.ty);
                self.plan.str(r.source) == SOURCE
                    && ty
                        .fields
                        .iter()
                        .any(|f| self.plan.str(self.plan.field(f).name) == "shownRoutes")
            })
            .collect();
        self.recommit(which, "shown routes")
    }

    /// `receipts` and, after them, the commit that answers `shownRoutes` again when they changed the
    /// routes a person can see (a push, a pop): what a host's commit path applies, once per batch.
    pub fn with_shown_routes<'a>(
        &mut self,
        receipts: &'a [crate::Timed],
        now_ms: f64,
    ) -> (std::borrow::Cow<'a, [crate::Timed]>, Option<RunnerError>) {
        match self.refresh_shown_routes() {
            Ok(Some(receipt)) => {
                let mut all = receipts.to_vec();
                all.push(crate::Timed {
                    at_ms: now_ms,
                    receipt,
                });
                (all.into(), None)
            }
            Ok(None) => (receipts.into(), None),
            Err(e) => (receipts.into(), Some(e)),
        }
    }

    /// Whether an `exactPage` resource reads `field`: one whose shape names
    /// it, or one that is no record (refused, but asked again on every
    /// change). A host watches what no resource reads only if it wants to.
    pub fn reads_page_field(&self, field: &str) -> bool {
        self.plan.resources.iter().any(|r| {
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
        let ty = self.plan.type_(self.plan.resources[i].ty);
        ty.kind != TypeKind::Record
            || ty.fields.iter().any(|f| {
                let name = self.plan.str(self.plan.field(f).name);
                previous.field(name) != self.page.field(name)
            })
    }

    pub(super) fn page_answer(&self, i: usize) -> Result<Value, DataError> {
        let row = &self.plan.resources[i];
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
            if name == "shownRoutes" {
                fields.push(Value::list(
                    self.shown_routes()
                        .into_iter()
                        .map(|key| Value::str(&key))
                        .collect(),
                ));
                continue;
            }
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

#[cfg(test)]
#[path = "shown_routes_tests.rs"]
mod shown_routes_tests;
