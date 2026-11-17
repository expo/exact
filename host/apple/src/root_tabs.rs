//! Each navigation root's tabs by the kernel's rule (`Kernel::root_tabs`, LLP
//! 1115.001 D11), as a batch carries them to the presenter. A batch says them
//! with the ops it applies, so the presenter reads the tabs of the tree it
//! shows: a question asked of the runtime between batches can meet a tree the
//! presenter has not applied yet, or be refused from inside a callback the
//! runtime waits on.

use super::Host;
use exact_kernel::{Display, FlexDirection, HostCover, PositionType, PropId, ViewId};
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
    /// for a tab not built yet. `unsaid`: the app left the bar's placement
    /// unsaid, by the web's rule for its own sidebar (`host/web/navigation.js`
    /// `unsaidBar`, LLP 1116.004 D1.1): the tablist is a direct child of the
    /// root, not vertical, in flow in the root's flex column, and itself a
    /// row. It is read here because the layout rows it asks of never cross to
    /// the presenter (`style::presenter_ignores`). `covered`: the tablist is
    /// covered whole, as the boot covers one a tab bar replaces (`closed.rs`).
    fn root_tabs_json(&self, root: ViewId) -> Option<String> {
        let list = |ids: &[ViewId]| ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let kernel = self.runner.kernel();
        let tabs = kernel.root_tabs(root)?;
        let (node, bar) = (kernel.node(root)?, kernel.node(tabs.tablist)?);
        let column = |d| matches!(d, FlexDirection::Column | FlexDirection::ColumnReverse);
        let unsaid = node.children().contains(&tabs.tablist)
            && bar.props.str(PropId::AccessibilityOrientation) != Some("vertical")
            && node.style.display == Display::Flex
            && column(node.style.flex_direction)
            && matches!(
                bar.layout_position(),
                PositionType::Static | PositionType::Relative
            )
            && !column(bar.style.flex_direction);
        let arena = kernel.arena();
        let covered =
            arena.slot_of(tabs.tablist).and_then(|s| arena.cover(s)) == Some(HostCover::Whole);
        let navigation = self.runner.panels_need_navigation(&tabs.panels);
        let navigation = navigation
            .iter()
            .map(bool::to_string)
            .collect::<Vec<_>>()
            .join(",");
        Some(format!(
            "{{\"root\":{root},\"tablist\":{},\"tabs\":[{}],\"panels\":[{}],\"navigation\":[{navigation}],\"unsaid\":{unsaid},\"covered\":{covered}}}",
            tabs.tablist,
            list(&tabs.tabs),
            list(&tabs.panels)
        ))
    }
}
