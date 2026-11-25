// @ref LLP 1075.003 §3.7 — each tab's controller, made with the tab container
// and never replaced. "You can use navigation controllers or custom view
// controllers as the root view controller for a tab" (UITabBarController): a
// tab whose panel can hold more than one route, or whose route can show a
// header, is a navigation controller, which shows that header as the
// platform's navigation bar and hides the bar where its route shows none; a
// tab whose panel shows one fixed route with no header is a plain view
// controller (`Runner::panels_need_navigation` reads which from the plan, so
// it is known for a tab not built yet). A tab the runner has not built yet
// (§9.14) has its controller from launch, empty until its routes arrive.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// A tab that shows one fixed route with no header, with no navigation
/// controller of its own.
final class PlainTab: UIViewController {
    private(set) var route: RouteController?
    weak var host: NavigationHost?
    /// At launch only the content is mounted here. Its controller receives
    /// its first containment and appearance from the real owner.
    var bootContent = false

    /// Move the content back; a live handoff mounts the destination first.
    func releaseBootContent() {
        guard let route else { return }
        route.view.backgroundColor = view.backgroundColor
        route.view.tintColor = route.node.ownAccent
        moveBootChildren(view.subviews, of: route, into: route.view)
        route.bootSurface = nil
        self.route = nil
    }

    private func moveBootChildren(_ children: [UIView], of route: RouteController, into container: UIView) {
        NodeView.containerMoves += 1
        defer { NodeView.containerMoves -= 1 }
        for child in children {
            let finish = route.node.presenter?.session?.natives.prepareScreenMove(child, into: container)
            container.addSubview(child)
            finish?()
        }
    }

    override func loadView() { view = UIView() }

    // Follow UIKit moving this tab into another controller (`NavigationHost.moved(into:)`): More pushes it for its row
    override func willMove(toParent parent: UIViewController?) {
        super.willMove(toParent: parent)
        if let parent { host?.moved(into: parent) }
    }

    /// Show `next` in place of the route shown now, unanimated.
    func show(_ next: RouteController?) {
        guard next !== route else { return }
        if let old = route, bootContent {
            releaseBootContent()
            old.bootSurface = nil
        }
        if let old = route {
            old.willMove(toParent: nil)
            old.view.removeFromSuperview()
            old.removeFromParent()
            route = nil
        }
        guard let next else {
            for edge in [NSDirectionalRectEdge.top, .bottom] { setContentScrollView(nil, for: edge) }
            return
        }
        route = next
        if bootContent {
            next.bootSurface = view
            view.backgroundColor = next.view.backgroundColor
            view.tintColor = next.node.ownAccent
            moveBootChildren(next.view.subviews, of: next, into: view)
            next.fitContentViewport()
            return
        }
        addChild(next)
        next.view.frame = view.bounds
        next.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(next.view)
        next.didMove(toParent: self)
        for edge in [NSDirectionalRectEdge.top, .bottom] { setContentScrollView(next.contentScrollView(for: edge), for: edge) }
    }

    // The tab bar follows its tab's content scroll view, which a navigation
    // controller takes from its top: here, the route's.
    override func contentScrollView(for edge: NSDirectionalRectEdge) -> UIScrollView? {
        route?.contentScrollView(for: edge) ?? super.contentScrollView(for: edge)
    }
}

/// A panel template may choose its route in an `if` inside a repeated tab.
/// UIKit needs a permanent tab before the runner builds that panel's first route.
final class PendingTab: UIViewController {
    let navigation: Bool

    init(navigation: Bool) {
        self.navigation = navigation
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:)") }

    func install(_ child: UIViewController) {
        addChild(child)
        child.view.frame = view.bounds
        child.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(child.view)
        child.didMove(toParent: self)
    }
}

extension NavigationHost {
    func tabWantsSplit(_ route: RouteController?) -> Bool {
        UIDevice.current.userInterfaceIdiom == .pad && !ExactEnv.authoredChrome &&
            route?.node.props["navigationSplit"] == "split" &&
            !["modal", "fullscreen"].contains(route?.node.props["navigationPresentation"] ?? "")
    }

    /// Tab `index`'s controller.
    func tabStack(_ index: Int) -> UIViewController? { tabPanels.indices.contains(index) ? tabStacks[tabPanels[index]] : nil }

    /// Where UIKit holds each tab's routes, in tab order (`routesStack`), or
    /// the one stack of a root without tabs: a split's two columns.
    var primaryStacks: [UIViewController] {
        if let bootOwner { return [bootOwner] }
        guard !tabPanels.isEmpty else { return [primaryNavigation, split?.detail].compactMap { $0 } }
        let held = moreTab
        return tabPanels.indices.flatMap { index -> [UIViewController] in
            if let columns = tabSplits[tabPanels[index]] { return [columns.list, columns.detail] }
            return [index == held ? more : routesStack(index)].compactMap { $0 }
        }
    }

    /// The router's tab, when it is a plain tab.
    var primaryPlain: PlainTab? { routesStack(routerTab) as? PlainTab }

    /// The routes of each navigation controller whose hatch left its bar
    /// shown (`showsBar`): the primary stacks first, then the sheets'.
    var barStackRoutes: [[UIViewController]] {
        (primaryStacks + presentedNavigations).compactMap { $0 as? UINavigationController }
            .filter { stacks[ObjectIdentifier($0)]?.showsBar == true }.map(\.stackRoutes)
    }

    /// Make the controller the tab's root asks for, even before UIKit shows it.
    func makeTabStack(_ panel: UInt32, _ routes: [RouteController], navigation: Bool) -> UIViewController {
        if tabWantsSplit(routes.first) {
            let requested = StackSplit(list: nil, width: nil, rows: [:])
            let rule = splitRule(routes.first) ?? nil
            let columns = makeSplit(first: routes, rule: rule ?? requested)
            tabSplits[panel] = columns
            // More pushes a tab's controller. UIKit forbids pushing a split view controller.
            // A tab that can move under More contains its split as a child.
            let owner: UIViewController
            if tabPanels.count > 5 && !(tabStacks[panel] is PendingTab) {
                let tab = PendingTab(navigation: navigation)
                tab.install(columns.controller)
                owner = tab
            } else {
                owner = columns.controller
            }
            if tabPanels.indices.contains(routerTab), tabPanels[routerTab] == panel {
                split = columns
                primaryNavigation = columns.list
            }
            startSplit(columns, first: routes)
            return owner
        }
        guard navigation else {
            let plain = PlainTab()
            plain.host = self
            setRoutes(routes, in: plain)
            return plain
        }
        let nav = makeNavigation(first: routes.first?.node)
        prepareRoutes(routes, in: nav)
        nav.setViewControllers(routes, animated: false)
        watchPops(nav)
        return nav
    }

    /// Write a tab's routes, unanimated, where UIKit holds them
    /// (`routesStack`). A plain tab shows its route. A navigation controller
    /// keeps what UIKit holds before its routes (`stackFront`), and one
    /// written for the first time shows its bar as its first route says, as
    /// a stack built with them would (`makeNavigation`).
    func setRoutes(_ routes: [RouteController], in stack: UIViewController) {
        let nav = stack as? UINavigationController
        if let nav, nav.stackRoutes.isEmpty, let first = routes.first {
            nav.setNavigationBarHidden(!(barShows(nav) && bar(of: first.node, pushed: false) != .hidden), animated: false)
        }
        prepareRoutes(routes, in: stack)
        if let plain = stack as? PlainTab { return plain.show(routes.first) }
        guard let nav else { return }
        if !nav.stackRoutes.elementsEqual(routes, by: ===) { writeStack(nav.stackFront + routes, in: nav, animated: false) }
    }

    /// Build the tabs More's list shows a row for that the runner has not
    /// built (LLP 1075.003 §9.14), selecting none: a row for an empty
    /// navigation controller does nothing (LLP 1075.003 §9.16), so each is
    /// built before a person can pick it. A plain tab needs nothing: More
    /// pushes a plain controller, empty or not.
    func buildTabsUnderMore() {
        #if os(iOS)
        guard !syncing, tabController != nil, let session = presenter.session else { return }
        // Read the panel's logical children, not its subviews: UIKit containment holds a built panel's route nodes
        let waiting = tabPanels.indices.filter { index in
            barItem(ofTab: index) == nil && tabStack(index) is UINavigationController && logicalChildren[tabPanels[index]]?.isEmpty != false
        }.map { tabPanels[$0] }
        guard !waiting.isEmpty else { return }
        session.log("navigation: More's list shows tabs not built yet (panels \(waiting)); the runner builds them")
        session.buildTabs(waiting)
        #endif
    }

    /// UIKit moved tabs between the bar and More, their routes or plain tabs
    /// with them (a size class change, a row of More's list, More's Edit), or
    /// an app's container took tabs More held: one sync in the next turn
    /// writes each tab's routes where UIKit now holds them and follows the
    /// router's tab, and a More list on screen gets the tabs it now shows
    /// built.
    func syncNextTurn() {
        guard !resyncOwed else { return }
        resyncOwed = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let held = pendingSync
            // Let UIKit first apply what it deferred to layout (More shows a tab selected in code in a later turn,
            // LLP 1075.003 §9.16), so More's delegate calls and the press they make run before this sync, not inside
            // it; a move that layout makes is this sync's to follow
            tabOwner?.view.layoutIfNeeded()
            resyncOwed = false
            if tabOwner == nil {
                runHeldSync()
            } else {
                // Release what was left to this sync, whose projection takes the holds it still needs
                projectionHeld = false
                resync()
                if let more, more.viewIfLoaded?.window != nil, more.topViewController === more.viewControllers.first { buildTabsUnderMore() }
            }
            // A transition's end posts no screen change while its sync is held
            if held { presenter.syncModal() }
        }
    }

    /// UIKit is moving a route or a plain tab into `parent`.
    /// `willMove(toParent:)` comes as UIKit applies the move, before the
    /// navigation controller's `willShow` for it, and More's parent is then
    /// Exact's tab container (LLP 1075.003 §9.16). UIKit makes a new More
    /// when it needs one again, and Exact adopts it here, so its delegate
    /// hears this move: a navigation controller that Exact's tab container
    /// holds and Exact has no record of is More, since each tab's own has
    /// one. It is not read from `moreNavigationController`, which makes a
    /// More where UIKit has none and names a new one once the routes have
    /// left the last. A move Exact did not write (a size class change, a row
    /// of More's list, More's Edit) is followed by a sync in the next turn.
    func moved(into parent: UIViewController) {
        #if os(iOS)
        if let nav = parent as? UINavigationController, let tabController, nav.parent === tabController,
           stacks[ObjectIdentifier(nav)] == nil { adoptMore(nav) }
        #endif
        if tabOwner != nil, !syncing { syncNextTurn() }
    }

    /// Adopt UIKit's More as one of Exact's stacks, its list left to UIKit,
    /// in place of the More adopted before it, whose record and delegate go:
    /// UIKit owns what that one holds. Exact holds the adopted More until it
    /// adopts the next, so no record outlives its More.
    func adoptMore(_ nav: UINavigationController) {
        #if os(iOS)
        guard nav !== adoptedMore else { return }
        if let old = adoptedMore { retireStack(old) }
        registerStack(nav)
        adoptedMore = nav
        #endif
    }

    /// The adopted More leaves Exact: its record and its delegate go, and
    /// the routes and the plain tab it holds leave it.
    func releaseMore() {
        if let adoptedMore { release(adoptedMore) }
        adoptedMore = nil
    }
}

extension ExactSession {
    /// Build the tab panels `panels`, selecting none (LLP 1075.003 §9.14).
    func buildTabs(_ panels: [UInt32]) {
        guard state != .destroyed, booted else { return }
        apply(runtime.buildTabs(panels, now: now()))
    }
}
#endif
