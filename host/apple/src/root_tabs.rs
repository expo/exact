//! Each navigation root's tabs by the kernel's rule (`Kernel::root_tabs`, LLP
//! 1115.001 D11), as a batch carries them to the presenter. A batch says them
//! with the ops it applies, so the presenter reads the tabs of the tree it
//! shows: a question asked of the runtime between batches can meet a tree the
//! presenter has not applied yet, or be refused from inside a callback the
//! runtime waits on.

use super::Host;
use crate::batch::Batch;
use exact_kernel::{HostCover, PropId, ViewId};
use exact_runner::DataSource;

impl<D: DataSource> Host<D> {
    /// Publish only the parts of a windowed split list that changed. A scroll
    /// typically replaces mounted cells while keeping the same logical index.
    #[cfg(target_os = "macos")]
    pub(super) fn publish_section_tables(
        &mut self,
        batch: &mut Batch,
        heads: &[exact_runner::CollectionSnapshot],
    ) -> Result<(), String> {
        use exact_kernel::PropId;
        use serde_json::json;
        if heads.is_empty() && self.section_tables.is_empty() {
            return Ok(());
        }
        let mut updates = Vec::new();
        let mut active = Vec::new();
        for head in heads {
            let list = head.view;
            let kernel = self.runner.kernel();
            let previous = self.section_tables.get(&list);
            let section = head
                .rows
                .first()
                .and_then(|row| kernel.node(row.root))
                .is_some_and(|n| n.props.str(PropId::SemanticTag) == Some("section"));
            if !section && previous.is_none() {
                continue;
            }
            let mut node = kernel.node(list);
            while let Some(n) = node {
                if n.props.str(PropId::NavigationKey).is_some() {
                    break;
                }
                node = n.parent.and_then(|id| kernel.node(id));
            }
            let Some(route) = node else { continue };
            let first = route
                .parent
                .and_then(|id| kernel.node(id))
                .and_then(|parent| {
                    parent
                        .child_nodes()
                        .find(|n| n.props.str(PropId::NavigationKey).is_some())
                        .map(|n| n.id)
                });
            if first != Some(route.id)
                || !self
                    .runner
                    .stack_split(route.id, previous.map(|_| list))
                    .is_some_and(|s| s.list == Some(list))
            {
                continue;
            }
            let Some(table) = self
                .runner
                .list_sections(list)
                .map_err(|e| format!("section rows: {e:?}"))?
            else {
                continue;
            };
            active.push(list);
            let same = previous
                .is_some_and(|p| std::rc::Rc::ptr_eq(&p.runs, &table.runs) || p.runs == table.runs);
            if same && previous.is_some_and(|p| p.mounted == table.mounted) {
                continue;
            }
            let runs = (!same).then(|| {
                table
                    .runs
                    .iter()
                    .map(|run| (&run.end, &run.headers))
                    .collect::<Vec<_>>()
            });
            updates.push(json!([list, runs, table.mounted]));
            self.section_tables.insert(list, table);
        }
        self.section_tables.retain(|id, _| {
            let keep = active.contains(id);
            if !keep {
                updates.push(json!([id]));
            }
            keep
        });
        if !updates.is_empty() {
            batch.collections("tables", &serde_json::Value::Array(updates).to_string());
        }
        Ok(())
    }

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
    /// An optional `accessory` names the element the tablist names as its tab
    /// bar's accessory, and an optional `accessoryNote` says why it names
    /// none, for a development build's journal (LLP 1075.003 §9.17).
    fn root_tabs_json(&self, root: ViewId, tabs: &exact_kernel::RootTabs) -> String {
        let list = |ids: &[ViewId]| ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let kernel = self.runner.kernel();
        let unsaid = tabs.allows_sidebar(kernel, root);
        let footer = tabs.footer.filter(|_| unsaid);
        let arena = kernel.arena();
        let covered =
            arena.slot_of(tabs.tablist).and_then(|s| arena.cover(s)) == Some(HostCover::Whole);
        let splits = self.runner.possible_split_panels(&tabs.panels);
        let splits = if splits.is_empty() {
            String::new()
        } else {
            format!(",\"possibleSplits\":[{}]", list(&splits))
        };
        let navigation = self.runner.panels_need_navigation(&tabs.panels);
        let navigation = navigation
            .iter()
            .map(bool::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let footer = footer.map_or_else(String::new, |id| format!(",\"footer\":{id}"));
        let accessory = tabs
            .accessory
            .map_or_else(String::new, |id| format!(",\"accessory\":{id}"));
        let note = kernel
            .node(tabs.tablist)
            .and_then(|list| {
                list.props
                    .str(PropId::NavigationTabAccessory)
                    .map(str::to_owned)
            })
            .and_then(|name| tabs.accessory_note(&name))
            .map_or_else(String::new, |note| {
                format!(",\"accessoryNote\":{}", serde_json::Value::String(note))
            });
        format!(
            "{{\"root\":{root},\"tablist\":{},\"tabs\":[{}],\"panels\":[{}],\"navigation\":[{navigation}],\"unsaid\":{unsaid},\"covered\":{covered}{splits}{footer}{accessory}{note}}}",
            tabs.tablist,
            list(&tabs.tabs),
            list(&tabs.panels)
        )
    }
}
