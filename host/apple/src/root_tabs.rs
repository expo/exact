//! Each navigation root's tabs by the kernel's rule (`Kernel::root_tabs`, LLP
//! 1115.001 D11), as a batch carries them to the presenter. A batch says them
//! with the ops it applies, so the presenter reads the tabs of the tree it
//! shows: a question asked of the runtime between batches can meet a tree the
//! presenter has not applied yet, or be refused from inside a callback the
//! runtime waits on.

use super::Host;
use crate::batch::Batch;
use exact_kernel::{HostCover, ViewId};
use exact_runner::DataSource;

impl<D: DataSource> Host<D> {
    /// Send changed navigation shapes with the tree that produced them. A layout
    /// can change an inspector's eligibility without adding or removing a node.
    pub(super) fn navigation_shapes(&mut self, batch: &mut Batch) {
        let kernel = self.runner.kernel();
        let mut roots = kernel.navigation_roots();
        roots.sort_unstable();
        let mut tabs_json = Vec::new();
        let mut inspectors = Vec::new();
        for root in roots {
            let tabs = kernel.root_tabs(root);
            if let Some(tabs) = &tabs {
                tabs_json.push(self.root_tabs_json(root, tabs));
            }
            if cfg!(target_os = "macos") {
                let panels = tabs
                    .as_ref()
                    .map_or(std::slice::from_ref(&root), |t| t.panels.as_slice());
                for panel in panels.iter().filter_map(|&id| kernel.node(id)) {
                    for route in panel.child_nodes() {
                        if let Some(inspector) = self.runner.route_inspector(route.id) {
                            inspectors.push((route.id, inspector));
                        }
                    }
                }
            }
        }
        let said = format!("[{}]", tabs_json.join(","));
        if self.root_tabs_told.as_ref() != Some(&said) {
            self.root_tabs_told = Some(said.clone());
            batch.root_tabs = Some(said);
        }
        if inspectors != self.route_inspectors_told {
            let rows: Vec<_> = inspectors
                .iter()
                .map(|(route, i)| (route, i.view, i.width, i.minimum, i.maximum))
                .collect();
            batch.route_inspectors = Some(serde_json::to_string(&rows).unwrap());
            self.route_inspectors_told = inspectors;
        }
    }

    /// Navigation root `root`'s tabs on the Apple wire:
    /// `{"root":view,"tablist":view,"tabs":[view...],"panels":[view...],
    /// "navigation":[bool...],"unsaid":bool,"covered":bool}`, the tabs that
    /// name a panel and those panels in tab order. `navigation`: whether each
    /// panel's tab needs a navigation controller
    /// (`Runner::panels_need_navigation`), read from the plan, so it is known
    /// for a tab not built yet. `unsaid` follows `RootTabs::allows_sidebar`. An
    /// optional `footer` names the view below those tabs. `covered` says the
    /// boot has removed the tablist from layout for a platform bar (`closed.rs`).
    fn root_tabs_json(&self, root: ViewId, tabs: &exact_kernel::RootTabs) -> String {
        let list = |ids: &[ViewId]| ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let kernel = self.runner.kernel();
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
        format!(
            "{{\"root\":{root},\"tablist\":{},\"tabs\":[{}],\"panels\":[{}],\"navigation\":[{navigation}],\"unsaid\":{unsaid},\"covered\":{covered}{footer}}}",
            tabs.tablist,
            list(&tabs.tabs),
            list(&tabs.panels)
        )
    }
}
