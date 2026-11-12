//! Commit host facts with their readers, or restore the previous facts.
use super::*;

impl<D: DataSource> Runner<D> {
    /// A host fact and its readers change together. The caller handles only
    /// its additional state, such as a locale slot or the kernel's font size.
    pub(super) fn replace_fact<T: PartialEq>(
        &mut self,
        field: impl Fn(&mut Self) -> &mut T,
        value: T,
        source: &str,
        commit: impl FnOnce(&mut Self, &T, Vec<usize>) -> Result<Option<CommitReceipt>, RunnerError>,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let changed = *field(self) != value;
        let previous = std::mem::replace(field(self), value);
        let readers = if changed {
            self.resource_rows()
                .enumerate()
                .filter_map(|(i, row)| (self.plan.str(row.source) == source).then_some(i))
                .collect()
        } else {
            Vec::new()
        };
        let result = commit(self, &previous, readers);
        if result.is_err() {
            *field(self) = previous;
        }
        result
    }
}
