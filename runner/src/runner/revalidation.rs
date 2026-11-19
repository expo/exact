//! The journal's word when an answer lands on what stood in for it.
use super::{DataSource, ResourceState, Runner};
use exact_plan::Value;

impl<D: DataSource> Runner<D> {
    /// The journal's word when an answer lands on a build-time one shown
    /// until its source was ready (LLP 1048.003 D6), or contradicts a page's
    /// answer asked again at `data_ready` (LLP 1048.000 D6) or a kept one
    /// (LLP 1027 D4): the first frame showed a value a reader of only the
    /// final frame never sees (LLP 1102 §3.17).
    pub(super) fn revalidated(&mut self, i: usize, shown: Option<&ResourceState>, answer: &Value) {
        let Some(shown) = shown.filter(|_| self.resources[i].stale) else {
            return;
        };
        let compiled = shown.value.is_compiled();
        if !compiled && shown.placeholder {
            return;
        }
        let same = crate::compare::equivalent(shown.value.get(&self.plan), answer);
        let name = self.plan.str(self.resource_row(i).name);
        // A value a reload carried is the replaced runner's, neither the page's
        // nor kept: nothing to say of it
        let line = if compiled {
            super::lines::revalidated(name, same)
        } else if same {
            return;
        } else if shown.from_page {
            super::lines::page_contradicted(name)
        } else if self.holds_kept(i, shown) {
            super::lines::kept_contradicted(name)
        } else {
            return;
        };
        self.log(line);
    }
}
