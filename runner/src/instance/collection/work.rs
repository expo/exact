//! Measured virtual-row operations, attributed to the owning collection site.
//! @ref LLP 1142 §13.4 — physical work is counted even if its commit is refused.
use super::*;

impl Collection {
    pub(super) fn count_row(
        &self,
        u: &mut Update<'_>,
        count: impl FnOnce(&mut crate::perf::SiteWork),
    ) {
        if let Some(site) = u.ids.site(self.view) {
            if let Some(w) = u.ids.work.sites.get_mut(site.0 as usize) {
                count(w);
            }
        }
    }

    /// Count each mounted row's subtree as retired, as its list is destroyed.
    pub(in crate::instance) fn retire_rows(&self, u: &mut Update<'_>) {
        for row in &self.mounted {
            self.retire_row(u, row);
        }
    }

    pub(super) fn retire_row(&self, u: &mut Update<'_>, row: &Mounted) {
        if u.ids.work.sites.is_empty() {
            return;
        }
        self.count_row(u, |w| w.row_retired += 1);
        crate::instance::retire::retire_children(u, &row.row.roots);
    }
}
