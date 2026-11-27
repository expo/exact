// @ref LLP 1075.003 §3.7, §9.16 — UIKit's change of which tabs its bar and More hold.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// What UIKit's change of a container's tabs leaves to its end: a
/// projection's layout of the container; the fit of the session that a
/// transition's end makes, and the focus and scrolls that follow it;
/// `reportCovers`, whoever calls it; and the projection that writes back what
/// UIKit dropped.
struct LeftToTabsChange {
    var layout = false, fit = false, input = false, covers = false, dropped = false
}

extension NavigationHost {
    /// Run `change`, UIKit's own change of `tabs`'s tabs. UIKit makes it in
    /// steps within the container's trait change: it moves the selected tab's
    /// routes between the tab's own stack and More, reports the moves as
    /// pushes and pops, and changes the routes' safe areas. Laid out or sized
    /// between two steps, the container can keep a More that UIKit let go, as
    /// a tab. So the work above waits for the last step. The covers wait for
    /// the layout too, since a batch answers them and does what the app asks.
    func whileTabsChange(_ tabs: TabController, _ change: () -> Void) {
        let presenter = presenter
        let outer = tabs.moving
        tabs.moving = outer.filter { !$0.root.inStack } + tabPanels.indices.compactMap { index in
            guard let routes = routesStack(index)?.stackRoutes as? [RouteController], routes.count > 1 else { return nil }
            return (routes[0], Array(routes.dropFirst()))
        }
        tabs.changes += 1
        change()
        tabs.changes -= 1
        tabs.moving = outer
        guard tabs.changes == 0 else { return }
        // No pop of the person's: a projection writes back the dropped routes the router shows, and animates no push of them
        let dropped = tabs.dropped.filter { !$0.inStack }
        tabs.dropped = []
        for route in dropped { route.held = false }
        if !dropped.isEmpty {
            unanimated = true
            tabs.left.dropped = true
        }
        let left = tabs.left
        tabs.left.layout = false
        tabs.left.fit = false
        let installed = { [unowned self] in tabController === tabs && presenter.session?.view?.window != nil }
        tabs.ends += 1
        if left.layout || left.fit, installed() {
            tabs.layingOut += 1
            layOutOwner()
            tabs.layingOut -= 1
        }
        if left.fit {
            if installed() { presenter.session?.view?.fit() }
            presenter.owners.endViewChange()
        }
        if left.layout, installed() { presenter.owners.publish() }
        tabs.ends -= 1
        guard tabs.ends == 0 else { return }
        guard installed() else { return tabs.left = LeftToTabsChange() }
        // (A sync under way reports them, and so does a tap's end)
        if tabs.left.covers, !syncing, tapping == nil { reportCovers() }
        writeCarriedScrollsWhenAnswered()
        if tabs.left.input {
            tabs.left.input = false
            presenter.flushPendingFocus()
            presenter.elementScrolls.flush()
        }
        // Ask for that projection now, so that it need not wait for the sync the root's move owes in the next turn
        if tabs.left.dropped {
            tabs.left.dropped = false
            presenter.afterBatch { [weak self] in self?.resync() }
        }
    }

    /// Whether UIKit, changing its tabs, took `route` from its stack with the tab's root: it has put the root in no other yet,
    /// or did without the route.
    func leftWithRoot(_ route: RouteController) -> Bool {
        guard let tabs = tabController as? TabController, tabs.changes > 0 else { return false }
        return tabs.dropped.contains { $0 === route }
            || tabs.moving.contains { !$0.root.inStack && $0.pushed.contains { $0 === route } }
    }

    /// A route is moving into a stack. If it is a root UIKit moves as it changes its tabs, the routes over it that no stack
    /// holds yet count as dropped until the outermost change ends: those in a stack by then were moved too, and UIKit left the
    /// others out, as it can one that hides the tab bar (LLP 1075.003 §9.16).
    func moved(root: RouteController) {
        guard let tabs = tabController as? TabController, let moving = tabs.moving.first(where: { $0.root === root }) else { return }
        tabs.dropped += moving.pushed.filter { !$0.inStack }
    }

    /// Leave work to the end of UIKit's change of its container's tabs, if one is under way. Whether it is left.
    func leaveToTabsChange(_ work: (inout LeftToTabsChange) -> Void) -> Bool {
        guard let tabs = tabController as? TabController, tabs.changes > 0 else { return false }
        let fitted = tabs.left.fit
        work(&tabs.left)
        // The fit tells the routes' blocks with the viewport
        if tabs.left.fit, !fitted { presenter.owners.beginViewChange() }
        return true
    }

    /// Size, place and lay out the primary owner.
    func layOutOwner() {
        guard let top = primaryOwner else { return }
        fitOwner()
        placeOwner()
        top.view.layoutIfNeeded()
    }
}
#endif
