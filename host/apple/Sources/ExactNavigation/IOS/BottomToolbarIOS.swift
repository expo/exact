// LLP 1123 C11b: the route owns the items; its navigation controller owns their placement.
#if os(iOS)
import UIKit
import ExactKit

final class BottomToolbarItems: ProjectedBarItems {
    var rows: [NodeView] = []
}

extension NavigationHost {
    func projectBottomToolbar(_ c: RouteController, in nav: UINavigationController?) {
        let placed = presenter.chrome.ids("toolbarPlacement")
        let rows = nav != nil && !ExactEnv.authoredChrome && !placed.isEmpty ? c.node.childNodes.filter {
            placed.contains($0.id) && $0.props["toolbarPlacement"] == "bottom" && $0.props["accessibilityRole"] == "toolbar"
                && $0.style["display"]?.string != "none" && !$0.cssVisibilityHidden
        } : []
        guard !rows.isEmpty || c.bottomToolbar != nil else { return }
        let previous = c.liftedBottomRows
        let changed = !previous.elementsEqual(rows, by: ===)
        if changed {
            for old in previous where !rows.contains(where: { $0 === old }) && presenter.views[old.id] === old {
                old.isHidden = old.style["display"]?.string == "none"
            }
        }
        guard let row = rows.first else {
            c.bottomToolbar = nil
            c.setToolbarItems(nil, animated: false)
            presenter.controls.syncShown()
            return
        }
        if rows.count > 1 {
            reportOnce("navigation: route \(c.key) has multiple bottom toolbars; UIKit shows the first row")
        }
        let store = c.bottomToolbar ?? BottomToolbarItems(c)
        c.bottomToolbar = store
        store.rows = rows
        let needsHiding = rows.contains { !$0.isHidden }
        if needsHiding { for source in rows { source.isHidden = true } }
        if changed || needsHiding { presenter.controls.syncShown() }
        // A spacer with no props can be a flat leaf, absent from the view hierarchy.
        let children = presenter.flats.children(of: row.id) ?? row.childNodes.map(\.id)
        let shown = children.filter { id in
            guard let style = presenter.flats.leaves[id]?.style ?? presenter.views[id]?.style else { return false }
            return style["display"]?.string != "none" && !cssVisibilityHides(style)
        }
        let live = Set(shown)
        store.barPresses.removeAll(keepingCapacity: true)
        store.barItemsMade = store.barItemsMade.filter { live.contains($0.key) }
        store.barSpaces = store.barSpaces.filter { live.contains($0.key) }
        #if EXACT_INSPECT
        store.itemNodes.removeAll(keepingCapacity: true)
        #endif
        let items = shown.map { id -> UIBarButtonItem in
            if let node = presenter.views[id], node.kind != "view" {
                return barItem(HeaderShape.Item(node), store)
            }
            let space = store.barSpaces[id] ?? UIBarButtonItem(systemItem: .flexibleSpace)
            store.barSpaces[id] = space
            return space
        }
        if !Self.same(c.toolbarItems, items) { c.setToolbarItems(items, animated: false) }
    }

    func showBottomToolbar(_ nav: UINavigationController, for route: RouteController?, animated: Bool) {
        guard let stack = stacks[ObjectIdentifier(nav)] else { return }
        let shows = route?.bottomToolbar != nil
        guard shows || stack.showingRouteToolbar else { return }
        stack.showingRouteToolbar = shows
        if shows == nav.isToolbarHidden { nav.setToolbarHidden(!shows, animated: animated) }
    }

    /// UIKit can put these controls beside the tab bar while its toolbar is detached.
    func bottomItem(_ id: UInt32) -> (UIBarButtonItem, UINavigationController)? {
        for route in controllers.values {
            guard let item = route.bottomToolbar?.barItemsMade[id]?.bar,
                  let nav = route.navigationController, nav.topViewController === route,
                  route.viewIfLoaded?.window != nil else { continue }
            let owner = split.flatMap { $0.nested && nav === $0.detail ? $0.list : nil } ?? nav
            if !owner.isToolbarHidden { return (item, owner) }
        }
        return nil
    }
}
#endif
