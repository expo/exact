// @ref LLP 1075.003 §3.7 — UIKit's stacks are the state the router follows.
// A pop UIKit commits (its Back button, a tap on the selected tab above its
// root, More's pop to its list, a swipe that completes) reaches the router
// at the first call Exact hears after it. Until then no sync writes the
// routes it took back into a stack, nor any route UIKit is moving.
#if os(iOS) || os(tvOS)
import UIKit

extension NavigationHost {
    /// The router's stacks as the tree holds them: each tab panel's routes in
    /// tab order (the root's without tabs), the selected stack through its
    /// selected route.
    var routerStacks: [[RouteController]] {
        (tabPanels.isEmpty ? [container?.id].compactMap { $0 } : tabPanels).map(routerStack)
    }

    /// The router's selected stack.
    var selectedStack: [RouteController] {
        guard let owner = tabPanels.isEmpty ? container?.id : (tabPanels.indices.contains(routerTab) ? tabPanels[routerTab] : nil) else { return [] }
        return routerStack(owner)
    }

    /// The routes the tree holds under `owner`, a tab panel or the root,
    /// through the selected route when it is among them.
    private func routerStack(_ owner: UInt32) -> [RouteController] {
        let routes = (logicalChildren[owner] ?? []).compactMap { id -> RouteController? in
            guard let c = controllers[id], let node = presenter.views[id], c.node === node else { return nil }
            return c
        }
        guard let selected = selectedKey, let at = routes.firstIndex(where: { $0.key == selected }) else { return routes }
        return Array(routes[...at])
    }

    /// Whether any of `routes` is one UIKit took out of a stack of Exact's and
    /// holds in none: one it popped, a swipe's source while the swipe runs,
    /// or a tab's first route while More pops to its list, which UIKit puts
    /// back in the tab's own controller as that pop ends (LLP 1075.003
    /// §9.16).
    func routesInFlight(_ routes: [RouteController]) -> Bool {
        routes.contains(where: inFlight)
    }

    private func inFlight(_ route: RouteController) -> Bool {
        route.held && !route.inStack && route.lastStack.map { stacks[ObjectIdentifier($0)] != nil } == true
    }

    /// Whether `route` is in flight from a pop UIKit committed: any but a
    /// swipe still under way, which commits only at its end.
    private func committed(_ route: RouteController) -> Bool {
        inFlight(route) && route.lastStack?.transitionCoordinator?.initiallyInteractive != true
    }

    /// The routes UIKit popped that the router has not followed: committed
    /// pops of routes above their stack's first route.
    func poppedByUIKit() -> [RouteController] {
        routerStacks.flatMap { $0.dropFirst().filter(committed) }
    }

    /// Report the pops UIKit committed to the router, each once. It runs at
    /// each UIKit call Exact hears (a stack's `willShow` and `didShow`, a
    /// transition's end, the tab delegate's calls) before anything else the
    /// call does, so a Back made before a tap reaches the router before the
    /// tap's press. A departed header search field finishes its edit first,
    /// while its handlers still exist (LLP 1035.001 D2). A pop on the tab
    /// UIKit selects follows that selection first (`followSelection`): a pop
    /// on a tab selected in code can reach Exact before the tab's route
    /// appears. One route off the router's selected route is the platform's
    /// Back, which presses the authored control; more is a history jump. A
    /// pop that leaves the router's selected route in place (under a route
    /// the app pushed meanwhile, or on a tab the router has left and UIKit
    /// does not select) is not followed, and the router's stack is written
    /// back.
    @discardableResult
    func followPops() -> Bool {
        guard !syncing else { return false }
        let popped = poppedByUIKit()
        guard !popped.isEmpty else { return false }
        for route in popped where route.search?.endEditing(completion: { [weak self] in self?.followPops() }) == true { return false }
        for shown in popped.compactMap({ $0.lastStack?.topViewController }) { followSelection(showing: shown) }
        for route in popped { route.held = false }
        guard let top = selectedKey, let target = NavigationRules.backTarget(shownKeys: shownKeys, popped: Set(popped.map(\.key)), selected: top) else {
            presenter.session?.log("navigation: UIKit popped \(popped.map(\.key).joined(separator: ", ")), which leaves the router's selected route \(selectedKey ?? "none") in place; the router's stacks are written back")
            return false
        }
        lastTransition = "completed"
        if target.steps == 1 { goBack() } else { goBack(from: top, to: target.to) }
        return true
    }

    /// Where UIKit and the router disagree while Exact owes UIKit nothing
    /// (LLP 1075.003 §3.5): a tab's stack, below any sheet, that UIKit holds
    /// with other routes than the router's, or a tab UIKit shows that the
    /// router is not on (More's list may show while the router keeps its
    /// tab). A stack UIKit is moving is skipped, as is a tab not built. Nil
    /// when they agree, or while a transition, a sync or a tap is under way.
    var restDifference: String? {
        guard container != nil, primaryOwner != nil, !syncing, !changing, !pendingSync, !resyncOwed, tapping == nil,
              !presenter.modals.inTransition else { return nil }
        let keys = { (routes: [UIViewController]) in "[" + routes.map { ($0 as? RouteController)?.key ?? "?" }.joined(separator: ", ") + "]" }
        for (index, routes) in routerStacks.enumerated() where !routes.isEmpty {
            let wanted = base(routes)
            guard let holder = tabPanels.isEmpty ? primaryNavigation : routesStack(index),
                  (holder as? UINavigationController)?.transitionCoordinator == nil, !routesInFlight(wanted) else { continue }
            let held = holder.stackRoutes
            if !held.elementsEqual(wanted, by: ===) {
                return (tabPanels.isEmpty ? "the stack" : "tab \(index)") + " holds \(keys(held)) in UIKit and \(keys(wanted)) in the router"
            }
        }
        // Each sheet's stack, against the selected stack's presentations
        let chosen = selectedStack
        let parts = NavigationRules.segments(presentations: chosen.map { $0.node.props["navigationPresentation"] })
        for (index, nav) in presentedNavigations.enumerated() where parts.indices.contains(index + 1) && nav.transitionCoordinator == nil {
            let wanted = Array(chosen[parts[index + 1]])
            if !routesInFlight(wanted), !nav.stackRoutes.elementsEqual(wanted, by: ===) {
                return "sheet \(index + 1) holds \(keys(nav.stackRoutes)) in UIKit and \(keys(wanted)) in the router"
            }
        }
        if let tabs = tabController, let selected = tabs.selectedViewController, selected !== more,
           let shown = tabIndex(of: selected), shown != routerTab {
            return "UIKit shows tab \(shown) and the router is on tab \(routerTab)"
        }
        return nil
    }

    /// Once a batch is projected, follow what it did to routes UIKit popped.
    /// A route the batch wrote anew, above its stack's first route, with the
    /// key of one UIKit had popped before the batch knew of it is the route
    /// popped, since the person popped that screen (LLP 1035.001 D2). Any
    /// other popped route the router no longer shows (the app went back past
    /// it, or removed it) is no pop of UIKit's, so the router showing it
    /// again is a request.
    func followHeld() {
        let router = routerStacks
        let shown = Set(router.joined().map(ObjectIdentifier.init))
        let anew = router.flatMap { $0.dropFirst().filter { !$0.held } }
        for route in controllers.values where route.held && !shown.contains(ObjectIdentifier(route)) {
            if committed(route), let replacement = anew.first(where: { $0.key == route.key && !$0.held }) {
                replacement.held = true
                replacement.lastStack = route.lastStack
            }
            route.held = false
        }
    }

    /// Follow in the next turn a pop UIKit committed that no call reported.
    func followPopsNextTurn() {
        guard !followOwed else { return }
        followOwed = true
        DispatchQueue.main.async { [weak self] in
            self?.followOwed = false
            self?.followPops()
        }
    }

    func prepare(_ batch: Batch) {
        checkOwned()
        presenter.elements.checkOwned()
        // Fold every batch's child lists, whether or not a sync projects it: a sync asked for during a sync returns
        // without projecting its batch
        foldChildren(batch)
        // Keep a route removed while its view is on screen for the transition that takes it away, UIKit's own pop,
        // which reached the router as it began (`followPops`), or one the app asked for, which Exact animates: its
        // pixels as drawn, and under them its views, until UIKit takes its view out of the window, as a sheet's views
        // stay while UIKit dismisses it. An editor among them stays focused while UIKit moves the keyboard away, as
        // the outgoing editor of UIKit's own pop does.
        for op in batch.ops where op.op == .destroy {
            guard let c = controllers[op.id], c.viewIfLoaded?.window != nil else { continue }
            c.freeze()
            c.retiringRoot = c.node
            retiringRoutes.add(c)
        }
    }

    /// The nodes of removed routes UIKit still shows (`prepare`).
    var retiringRoots: [NodeView] { retiringRoutes.allObjects.compactMap(\.retiringRoot) }

    /// Whether `node`, which a batch removed, stays in its route's view (`prepare`).
    func retainsRemovedView(_ node: NodeView) -> Bool {
        retiringRoots.contains { node === $0 || node.isDescendant(of: $0) }
    }

    /// Run the sync a transition held, now that it has ended.
    func runHeldSync() {
        guard pendingSync || tabsHeld else { return }
        pendingSync = false
        tabsHeld = false
        resync()
    }

    /// The child lists `batch` sets on the root and the tab panels.
    func foldChildren(_ batch: Batch) {
        for op in batch.ops where op.op == .children && logicalChildren[op.id] != nil {
            logicalChildren[op.id] = op.ids
        }
    }

    /// Project the tree as it is now, with no batch of its own.
    func resync() {
        sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
    }
}

extension RouteController {
    /// Whether UIKit holds the route now: in its navigation controller's
    /// stack, or as the route its plain tab shows. A popped route is still
    /// its navigation controller's child when the pop is asked for, and a
    /// tab tapped in the turn of the pop leaves it a child for good (LLP
    /// 1075.003 §9.16), so the stack is read, not the parent.
    var inStack: Bool { stack?.stackRoutes.contains { $0 === self } == true }

    /// The stack it is in: its plain tab, checked first because under More
    /// the plain tab's `navigationController` is More's; else its navigation
    /// controller.
    var stack: UIViewController? { (parent as? PlainTab) ?? navigationController }

    /// Cover the route with its pixels as drawn now, before a batch removes
    /// the route (`NavigationHost.prepare`): a button can remove a route
    /// before UIKit starts its pop, and UIKit's own pop reaches the router as
    /// it begins, before UIKit has drawn the route where it animates it, when
    /// no snapshot of what was drawn is to be had.
    func freeze() {
        guard isViewLoaded, let snapshot = view.snapshotView(afterScreenUpdates: false) ?? view.snapshotView(afterScreenUpdates: true) else { return }
        snapshot.frame = view.bounds
        // Pinned to the top, never stretched: the viewport can grow under it
        // as the pop starts (a keyboard going away, see NavigationHost.sync).
        snapshot.autoresizingMask = [.flexibleWidth, .flexibleBottomMargin]
        view.addSubview(snapshot)
        snapshot.setPaintForeground()
    }
}
#endif
