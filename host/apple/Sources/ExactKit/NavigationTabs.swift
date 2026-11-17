// @ref LLP 1075.003 §3.7 — a navigation root's tabs as the kernel finds them
// (`Kernel::root_tabs`, LLP 1115.001 D11; the web's `panelsOf`,
// `host/web/navigation.js`): the tabpanels its own tablist's tabs name with
// `aria-controls`, in tab order, the first tablist in tree order that names
// one. Each panel holds one tab's stack — its route rows — and every tab's
// stack stays mounted. A tablist or a panel inside a route is that route's,
// never the root's. Shared by the AppKit presenter (hidden and inert, LLP 1038
// D6's rule) and the UIKit one (a tab bar controller, one controller per tab).
import ExactCore
import Foundation

struct NavigationTabs {
    let tablist: NodeView
    /// The tabs that name a panel, and those panels, in tab order.
    let tabs: [NodeView]
    let panels: [NodeView]

    /// A panel's stack: its children that carry a `navigationKey`.
    static func routes(in node: NodeView) -> [NodeView] {
        node.container.subviews.compactMap { $0 as? NodeView }.filter { $0.props["navigationKey"] != nil }
    }

    /// The root's tabs, or nil when it has none (one stack): the kernel's,
    /// as the last batch to say them gave them (`Presenter.rootTabs`),
    /// resolved to the presenter's views. A tab whose tab or panel has no
    /// view is left out.
    static func of(_ root: NodeView, _ presenter: Presenter) -> NavigationTabs? {
        guard let kernel = presenter.rootTabs[root.id], let tablist = presenter.views[kernel.tablist] else { return nil }
        var tabs: [NodeView] = [], panels: [NodeView] = []
        for (tab, panel) in zip(kernel.tabs, kernel.panels) {
            guard let tab = presenter.views[tab], let panel = presenter.views[panel] else { continue }
            tabs.append(tab)
            panels.append(panel)
        }
        return panels.isEmpty ? nil : NavigationTabs(tablist: tablist, tabs: tabs, panels: panels)
    }

    /// Which stack holds the route the root names, and each stack's routes.
    func stacks(selecting key: String) -> (at: Int?, stacks: [[NodeView]]) {
        let stacks = panels.map(NavigationTabs.routes(in:))
        return (stacks.firstIndex { $0.contains { $0.props["navigationKey"] == key } }, stacks)
    }
}

extension Presenter {
    /// Hold each navigation root's tabs a batch says (`Batch.rootTabs`), before
    /// its ops apply. They are the tree's once the batch is applied, and a
    /// read resolves them to the views the presenter has then (`of`), so a read
    /// while the ops apply (the first batch mounts the native owner at the
    /// root's first frame) has them.
    func takeRootTabs(_ batch: Batch) {
        guard let said = batch.rootTabs else { return }
        rootTabs = Dictionary(said.map { ($0.root, $0) }, uniquingKeysWith: { first, _ in first })
    }
}
