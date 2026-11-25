// @ref LLP 1075.003 §3.7 — UIKit's stacks are the state the router follows.
// A pop UIKit commits (its Back button, a tap on the selected tab above its
// root, More's pop to its list, a swipe that completes) reaches the router
// at the first call Exact hears after it. Until then no sync writes the
// routes it took back into a stack, nor any route UIKit is moving.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

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
        inFlight(route) && popCoordinator(of: route)?.initiallyInteractive != true
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

    /// Where UIKit and the router disagree while nothing is in flight
    /// (LLP 1075.003 §3.5): a tab's stack, below any sheet, that UIKit holds
    /// with other routes than the router's, or a tab UIKit shows that the
    /// router is not on (More's list may show while the router keeps its
    /// tab). A stack UIKit is moving is skipped, as is a tab not built. Nil
    /// when they agree, during a transition, sync or tap, or while a stack
    /// write has a retry scheduled.
    var restDifference: String? {
        guard container != nil, primaryOwner != nil, !syncing, !changing, !pendingSync, !resyncOwed, tapping == nil,
              !stackWriteRetry, !presenter.modalHost.inTransition, split?.inTransition != true else { return nil }
        for (index, routes) in routerStacks.enumerated() where !routes.isEmpty {
            let wanted = base(routes)
            guard let holder = tabPanels.isEmpty ? primaryNavigation : routesStack(index),
                  (holder as? UINavigationController)?.transitionCoordinator == nil, !routesInFlight(wanted) else { continue }
            // A split shows its columns' routes as one stack (`SplitIOS.shown`)
            let columns = tabPanels.isEmpty ? split : tabSplits[tabPanels[index]]
            let held = columns?.shown ?? holder.stackRoutes
            if !held.elementsEqual(wanted, by: ===) {
                return (tabPanels.isEmpty ? "the stack" : "tab \(index)") + Self.routerStackDifference(held: held, wanted: wanted)
            }
        }
        // Each sheet's stack, against the selected stack's presentations
        let chosen = selectedStack
        let parts = NavigationRules.segments(presentations: chosen.map { $0.node.props["navigationPresentation"] })
        for (index, nav) in presentedNavigations.enumerated() where parts.indices.contains(index + 1) && nav.transitionCoordinator == nil {
            let wanted = Array(chosen[parts[index + 1]])
            if !routesInFlight(wanted), !nav.stackRoutes.elementsEqual(wanted, by: ===) {
                return "sheet \(index + 1)" + Self.routerStackDifference(held: nav.stackRoutes, wanted: wanted)
            }
        }
        if let tabs = tabController, let selected = tabs.selectedViewController, selected !== more,
           let shown = tabIndex(of: selected), shown != routerTab {
            return "UIKit shows tab \(shown) and the router is on tab \(routerTab)"
        }
        return nil
    }

    private static func routerStackDifference(held: [UIViewController], wanted: [UIViewController]) -> String {
        " holds \(stackKeys(held)) in UIKit and \(stackKeys(wanted)) in the router"
    }

    private static func stackKeys(_ routes: [UIViewController]) -> String {
        "[" + routes.map { ($0 as? RouteController)?.key ?? "?" }.joined(separator: ", ") + "]"
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
        // Update known child lists before UIKit can react to the batch
        foldChildren(batch)
        #if os(iOS)
        hearSplit(batch)
        freezeTabAccessory(for: batch)
        #endif
        // Keep a route removed while its view is on screen for the transition that takes it away, UIKit's own pop,
        // which reached the router as it began (`followPops`), or one the app asked for, which Exact animates: its
        // pixels as drawn, and under them its views, until UIKit takes its view out of the window, as a sheet's views
        // stay while UIKit dismisses it. An editor among them stays focused while UIKit moves the keyboard away, as
        // the outgoing editor of UIKit's own pop does.
        for op in batch.ops where op.op == .destroy {
            guard let c = controllers[op.id], c.viewIfLoaded?.window != nil else { continue }
            c.freeze()
            c.retire()
            retiringRoutes.add(c)
        }
    }

    /// The nodes of removed routes UIKit still shows (`prepare`).
    var retiringRoots: [NodeView] { retiringRoutes.allObjects.compactMap(\.retiringRoot) }

    /// Whether `node`, which a batch removed, stays in its route's view (`prepare`).
    func retainsRemovedView(_ node: NodeView) -> Bool {
        retiringRoots.contains { node === $0 || node.isDescendant(of: $0) }
    }

    /// Run the sync a transition or refused stack write held.
    func runHeldSync() {
        // Leave each hold for its own release while a projection is running, or for the sync owed in the next turn: UIKit
        // reports its own move of a tab's routes as a transition's end, from within the move, where a layout of the tab
        // container can leave it a More that UIKit let go, as a tab
        guard !syncing, !resyncOwed else { return }
        guard pendingSync || tabsHeld || stackWritesOwed else {
            settleScrollEdges()
            return
        }
        projectionHeld = false
        tabsHeld = false
        resync()
    }

    /// Returns true when UIKit holds the requested controllers. A new
    /// refusal schedules a retry even when UIKit supplies no coordinator.
    @discardableResult
    func writeStack(_ routes: [UIViewController], in nav: UINavigationController, animated: Bool, visible: Bool = false) -> Bool {
        nav.setViewControllers(routes, animated: animated)
        let held = nav.viewControllers
        let id = ObjectIdentifier(nav)
        if held.elementsEqual(routes, by: ===) {
            stackWriteRefusals.removeValue(forKey: id)
            exhaustedStackWrites.removeValue(forKey: id)
            return true
        }
        let exhausted = exhaustedStackWrites[id]?.matches(held: held, wanted: routes) == true
        let label = stacks[id]?.label ?? String(describing: id)
        let difference = "stack \(label) holds \(Self.stackKeys(held)) in UIKit and was asked for \(Self.stackKeys(routes))"
        stackWriteRefusals[id] = StackWriteRefusal(held: held, wanted: routes, visible: visible, difference: difference, exhausted: exhausted)
        if !stackWriteRetry, !exhausted {
            stackWriteRetry = true
            retryStackWrite(remaining: Self.stackWriteRetryLimit)
        }
        return false
    }

    private static let stackWriteRetryLimit = 60

    private var stackWriteCanRetry: Bool {
        stackWriteRefusals.values.contains { !$0.exhausted }
    }

    /// Probe until the projection takes or the series' retry budget runs out;
    /// a write declined while a series runs shares what is left of it. Keep
    /// the debt on exhaustion so the next batch can make another attempt.
    private func retryStackWrite(remaining: Int) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0 / 60) { [weak self] in
            guard let self else { return }
            if !syncing, !presenter.applying, stackWriteCanRetry { resync() }
            if !stackWriteCanRetry {
                stackWriteRetry = false
            } else if remaining > 1 {
                retryStackWrite(remaining: remaining - 1)
            } else {
                let refusals = stackWriteRefusals.filter { !$0.value.exhausted }
                for (id, refusal) in refusals {
                    stackWriteRefusals[id]?.exhausted = true
                    exhaustedStackWrites[id] = refusal
                }
                stackWriteRetry = false
                for difference in refusals.values.map(\.difference).sorted() {
                    reportOnce("navigation: stack write still owed after \(Self.stackWriteRetryLimit) retry turns. \(difference). The next batch will retry.")
                }
                presenter.menus.resumeAfterRouteDismissal()
            }
        }
    }

    /// Runs what `transition` held when it ends, on the turn after its completion: the held sync (`runHeldSync`: a
    /// stack's write or a sync a route's sheet deferred) and the selected stack's tablist,
    /// which the bar could not follow while it ran, at the root (`tablistRoot`) or at a pushed route on top
    /// (`tablistOwed`). The stack's own push or pop also runs the held sync from `willShow`'s completion, `didShow` and
    /// `completeTransition`; this release also serves coordinators for presentations and dismissals over the stack. A
    /// call that finds a transition whose run is pending asks nothing more and loses nothing: the run reads the router
    /// and the tree as they are then.
    func runHeld(after transition: UIViewControllerTransitionCoordinator) {
        guard !heldTransitions.contains(transition) else { return }
        heldTransitions.add(transition)
        let run = { [weak self] in
            guard let self else { return }
            runHeldSync()
            guard let stack = routesStack(routerTab) else { return }
            followTablist(stack.stackRoutes.compactMap { $0 as? RouteController }, in: stack)
        }
        let release = { [weak self, weak transition] in self?.heldTransitions.remove(transition) }
        let queued = transition.animate(alongsideTransition: nil) { _ in DispatchQueue.main.async { release(); run() } }
        // Run on the next turn too when the coordinator answers that it queued nothing, as one does once its completions
        // have begun (`ModalController.viewDidAppear` meets one still installed), holding it through that run so it does
        // not ask again; a stack that still reports it then leaves the work to the next batch, and a completion UIKit runs
        // all the same finds nothing left to do

        if !queued { DispatchQueue.main.async { run(); release() } }
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
    /// A split's detail column keeps its routes as UIKit takes it off the
    /// list's column for a person's Back collapsed: they are in no stack the
    /// person sees until UIKit nests the column again or expands the split
    /// (`SplitIOS.showsDetail`, LLP 1127 D5).
    var inStack: Bool {
        guard let stack, stack.stackRoutes.contains(where: { $0 === self }) else { return false }
        return host?.split.map { $0.detail !== stack || $0.showsDetail } ?? true
    }

    /// The stack it is in: its plain tab, checked first because under More
    /// the plain tab's `navigationController` is More's; else its navigation
    /// controller.
    var stack: UIViewController? { (parent as? PlainTab) ?? navigationController ?? (bootSurface != nil ? host?.bootOwner : nil) }

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
