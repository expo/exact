use super::{DataError, DataSource, Runner, RunnerError};
use crate::canvas_performance::{MAX_BYTES, SOURCE};
use crate::surface_record::surface_name;
use exact_kernel::CommitReceipt;

impl<D: DataSource> Runner<D> {
    /// Whether this plan opts a named canvas into ephemeral HUD metrics.
    pub fn wants_canvas_performance(&self, name: &str) -> bool {
        self.plan
            .resources
            .iter()
            .any(|r| self.plan.str(r.source) == SOURCE && surface_name(&self.plan, r) == Some(name))
    }

    /// Replace one opted-in canvas's latest metrics, or clear them on disposal.
    /// Invalid snapshots retain the preceding answer. No fact enters kept storage.
    pub fn set_canvas_performance(
        &mut self,
        name: &str,
        json: Option<&str>,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let which: Vec<_> = self
            .resource_rows()
            .enumerate()
            .filter_map(|(i, row)| {
                (self.plan.str(row.source) == SOURCE && surface_name(&self.plan, row) == Some(name))
                    .then_some(i)
            })
            .collect();
        if which.is_empty() || self.canvas_performance.get(name).map(String::as_str) == json {
            return Ok(None);
        }
        if json.is_some_and(|s| s.len() > MAX_BYTES) {
            return Err(RunnerError::Data {
                resource: self.plan.str(self.resource_row(which[0]).name).into(),
                error: DataError::Unavailable(format!(
                    "canvas performance exceeds {MAX_BYTES} bytes"
                )),
            });
        }
        let mut records = self.canvas_performance.clone();
        match json {
            Some(json) => {
                records.insert(name.to_owned(), json.to_owned());
            }
            None => {
                records.remove(name);
            }
        }
        self.replace_fact(
            |r| &mut r.canvas_performance,
            records,
            SOURCE,
            |r, _, _| r.recommit(which, &format!("canvas performance {name}")),
        )
    }
}
