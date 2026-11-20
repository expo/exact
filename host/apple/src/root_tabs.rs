//! Each navigation root's tabs by the kernel's rule (`Kernel::root_tabs`, LLP
//! 1115.001 D11), as a batch carries them to the presenter. A batch says them
//! with the ops it applies, so the presenter reads the tabs of the tree it
//! shows: a question asked of the runtime between batches can meet a tree the
//! presenter has not applied yet, or be refused from inside a callback the
//! runtime waits on.

use super::Host;
use exact_kernel::{HostCover, ViewId};
use exact_runner::DataSource;

impl<D: DataSource> Host<D> {
    /// Every navigation root's tabs when they changed since a batch last
    /// said them: a JSON array with an object for each root that has tabs
    /// (`root_tabs_json`), by view id. The presenter replaces what it holds
    /// with each array it hears. A new host says them at its first batch.
    /// They are read at the end of every batch: the walk stops at each root's
    /// routes, so it reads the root's own structure, and a trigger that skipped
    /// it would have to see every way that structure, its styles and its covers
    /// change, with or without a commit.
    pub(super) fn root_tabs_moved(&mut self) -> Option<String> {
        let mut roots = self.runner.kernel().navigation_roots();
        // The roots in a fixed order, so the same tabs say the same array
        roots.sort_unstable();
        let tabs: Vec<String> = roots
            .into_iter()
            .filter_map(|root| self.root_tabs_json(root))
            .collect();
        let said = format!("[{}]", tabs.join(","));
        if self.root_tabs_told.as_ref() == Some(&said) {
            return None;
        }
        self.root_tabs_told = Some(said.clone());
        Some(said)
    }

    /// Navigation root `root`'s tabs, or `None` for a root with one stack:
    /// `{"root":view,"tablist":view,"tabs":[view...],"panels":[view...],
    /// "navigation":[bool...],"unsaid":bool,"covered":bool}`, the tabs that
    /// name a panel and those panels in tab order. `navigation`: whether each
    /// panel's tab needs a navigation controller
    /// (`Runner::panels_need_navigation`), read from the plan, so it is known
    /// for a tab not built yet. `unsaid` follows `RootTabs::allows_sidebar`. An
    /// optional `footer` names the view below those tabs. `covered` says the
    /// boot has removed the tablist from layout for a platform bar (`closed.rs`).
    fn root_tabs_json(&self, root: ViewId) -> Option<String> {
        let list = |ids: &[ViewId]| ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let kernel = self.runner.kernel();
        let tabs = kernel.root_tabs(root)?;
        let unsaid = tabs.allows_sidebar(kernel, root);
        let footer = tabs.footer.filter(|_| unsaid);
        let arena = kernel.arena();
        let covered =
            arena.slot_of(tabs.tablist).and_then(|s| arena.cover(s)) == Some(HostCover::Whole);
        let navigation = self.runner.panels_need_navigation(&tabs.panels);
        let navigation = navigation
            .iter()
            .map(bool::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let footer = footer.map_or_else(String::new, |id| format!(",\"footer\":{id}"));
        Some(format!(
            "{{\"root\":{root},\"tablist\":{},\"tabs\":[{}],\"panels\":[{}],\"navigation\":[{navigation}],\"unsaid\":{unsaid},\"covered\":{covered}{footer}}}",
            tabs.tablist,
            list(&tabs.tabs),
            list(&tabs.panels)
        ))
    }
}
