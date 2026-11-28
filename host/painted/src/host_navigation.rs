//! The host's projection of navigation: the router op each commit publishes, and the stacks and bars it shows.

use super::*;

impl<D: DataSource> Host<D> {
    // @ref LLP 1038 D6/D7/D11 — no batch consumer on this host. Keep the
    // last coalesced op for inspection; navigation's agent section stays unavailable.
    pub(super) fn project_navigation(&mut self) -> bool {
        let mut changed = false;
        if let Some(change) = self.runner.take_router_change() {
            self.router_op = Some(change);
            changed = true;
        }
        // Only stacks and popovers matter to it: the walk keeps those, in
        // preorder, not every node of every mounted row.
        // None at all (most apps, most commits): no walk.
        let kernel = self.runner.kernel();
        let navigation = if kernel.has_prop(exact_kernel::PropId::NavigationBack)
            || kernel.has_prop(exact_kernel::PropId::Popover)
        {
            kernel.preorder_where(&self.runner.roots(), |_, props| {
                props.str(exact_kernel::PropId::NavigationBack).is_some()
                    || props.str(exact_kernel::PropId::Popover).is_some()
            })
        } else {
            Vec::new()
        };
        for line in self.navigation.sync(self.runner.kernel(), &navigation) {
            self.runner.log(line);
        }
        changed | self.project_bars()
    }

    /// The last router op; this host has no foreign batch consumer.
    pub fn router_op(&self) -> Option<&exact_runner::RouterChange> {
        self.router_op.as_ref()
    }
}
