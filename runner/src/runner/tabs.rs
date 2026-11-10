//! A navigation tab built when it is first selected, as the runner shows it
//! (LLP 1075.003 §9.14; the instance tree's half is `instance/tabs.rs`).

use super::*;
use crate::instance::tabs::CarriedPanels;

impl<D: DataSource> Runner<D> {
    /// The tab panels whose contents wait for their first selection: empty
    /// in the runner until their root selects them (LLP 1075.003 §3.7). The
    /// agent's `tree` marks each `unopened`.
    pub fn unopened_tabs(&self) -> &[ViewId] {
        self.ids.tabs.waiting()
    }

    /// Build every tab panel now and from now on, none waiting for its first
    /// selection (LLP 1075.003 §9.14): a page rendered ahead of its runtime
    /// holds every panel, which the JS target adopts at boot.
    pub fn build_every_tab(&mut self) -> Result<CommitReceipt, RunnerError> {
        self.ids.tabs.build_every_tab();
        self.update_tree(true, |tree, u| tree.open_every_tab(u))
    }

    /// Build the waiting tab panels among `panels` now, selecting none (LLP
    /// 1075.003 §9.14): a host whose container lets a person pick a tab it
    /// cannot show unbuilt asks for that tab first (UIKit's More list does
    /// nothing for a row whose navigation controller is empty).
    pub fn build_tabs(&mut self, panels: &[ViewId]) -> Result<CommitReceipt, RunnerError> {
        self.update_tree(true, |tree, u| tree.open_panels(u, panels))
    }

    /// Whether each tab panel in `panels` needs a navigation controller
    /// ([`crate::instance::tabs::sites_need_navigation`]); `true` where the
    /// plan cannot tell, for a view that is no panel site the plan names (a
    /// panel whose `role` is computed): a navigation controller can show any
    /// of its routes.
    pub fn panels_need_navigation(&self, panels: &[ViewId]) -> Vec<bool> {
        let sites = crate::instance::tabs::panel_sites(&self.plan);
        let navigation = crate::instance::tabs::sites_need_navigation(&self.plan, &sites);
        panels
            .iter()
            .map(|p| {
                self.plan_site_of(*p)
                    .and_then(|s| sites.binary_search(&s).ok())
                    .is_none_or(|i| navigation[i])
            })
            .collect()
    }

    /// The tab panels' state a reload carries: each built panel's, and what
    /// the last reload carried for one not built since.
    pub(super) fn carry_panels(&self) -> CarriedPanels {
        let carried = &self.ids.tabs.carried;
        self.tree.as_ref().map_or_else(
            || carried.clone(),
            |tree| tree.carry_panels(&self.plan, &self.sites, carried),
        )
    }
}
