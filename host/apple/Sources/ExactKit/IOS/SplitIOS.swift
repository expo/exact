// @ref LLP 1127 — a list and its detail on iOS are UIKit's split view.
// Where a stack's root route holds a list whose rows push (the runner's
// rule, `StackSplit`), the stack is the two columns of a
// `UISplitViewController`: the root route in the list's navigation
// controller, the routes above it in the detail's. How many columns show,
// their widths and where the bars stand are UIKit's. Collapsed, UIKit nests
// the detail's controller in the list's and takes it back as it expands
// (LLP 1127 §1, item 2), so a pop in either is the router's Back. Expanded,
// a sheet that says `navigationInspector` is the split's inspector column
// (D14).
#if os(iOS) || os(tvOS)
import UIKit

/// UIKit's split, which says when a layout changed whether the list shows
/// beside the detail: a split laid out wide from the start sends no
/// expansion to its delegate.
private final class SplitController: UISplitViewController {
    var laidOut: () -> Void = {}
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        laidOut()
    }
}

final class SplitIOS: NSObject, UISplitViewControllerDelegate {
    let controller: UISplitViewController = SplitController(style: .doubleColumn)
    /// The list's column: the stack's root route.
    let list: UINavigationController
    /// The detail's column: the routes above the root.
    let detail: UINavigationController
    /// The detail while the stack is at its root: the platform's empty page
    /// (LLP 1127 Q2).
    let empty = UIViewController()
    /// UIKit collapsed or expanded the columns, or showed or hid one.
    var changed: () -> Void = {}
    /// The stack last shown, root first: expanded again, the columns show it.
    private var stack: [RouteController] = []
    /// `showsList` when `changed` was last called.
    private var told: Bool?
    /// The inspector's column (iOS 26), made when a route first goes there.
    private(set) var inspector: UINavigationController?
    /// The person closed the inspector's column; the router has yet to hear it.
    var inspectorClosed: () -> Void = {}
    /// UIKit is moving the columns, inside its animation (`moveAlong`).
    var columnsMove: () -> Void = {}
    /// UIKit has said the columns will move and has yet to open its animation. It lays them out itself, inside that
    /// animation, as it does a view constrained to the safe area: nothing here may lay them out before then, or what
    /// the safe area moves jumps to its end.
    private(set) var columnsAboutToMove = false
    /// The list whose rows made the stack a split, which keeps it one while
    /// it has no row to act on (a search that matches nothing).
    var listNode: UInt32?
    /// The location of the detail's top route: after a Back to the list while collapsed, the next expansion returns
    /// to it, as UIKit's own split shows the detail the person left (LLP 1127 §1, item 4). The stack at its root on
    /// a split settled expanded forgets it (`NavigationHost.noteDetail`).
    var left: String?
    /// Asks the router for `left`'s location.
    var returnTo: (String) -> Void = { _ in }

    init(list: UINavigationController, detail: UINavigationController) {
        self.list = list
        self.detail = detail
        super.init()
        #if os(iOS)
        empty.view.backgroundColor = .systemBackground
        #endif
        controller.delegate = self
        #if os(iOS)
        // The list's column is a sidebar, in UIKit's style for one, as SwiftUI's split sets it.
        controller.primaryBackgroundStyle = .sidebar
        #endif
        (controller as? SplitController)?.laidOut = { [weak self] in
            guard let self, told != showsList else { return }
            told = showsList
            DispatchQueue.main.async { self.changed() }
        }
        controller.setViewController(list, for: .primary)
        controller.setViewController(detail, for: .secondary)
    }

    /// Whether the list shows beside the detail.
    var showsList: Bool { !controller.isCollapsed && controller.displayMode != .secondaryOnly }
    /// Whether the list shows over the detail: then only the list is live (LLP 1127 D8).
    var overlaysDetail: Bool { !controller.isCollapsed && [.oneOverSecondary, .twoOverSecondary].contains(controller.displayMode) }
    /// Whether UIKit nests the detail's controller in the list's now.
    var nested: Bool { controller.isCollapsed && list.viewControllers.contains { $0 === detail } }
    /// Whether UIKit is expanding the columns, from asking which display mode
    /// to expand to until it has: the detail's controller leaves the list's
    /// for its column meanwhile, which is no Back. The split's own size class
    /// can still be compact then (an iPhone Duo unfolding).
    var expanding: Bool { expansion && controller.isCollapsed }
    private var expansion = false
    /// Whether UIKit is collapsing the columns, from asking which column to
    /// put on top until it has: the detail's controller enters the list's
    /// meanwhile, while the split still reads expanded. UIKit can ask and then
    /// expand instead, with no word of a collapse (an iPhone Duo unfolding):
    /// the expansion's question ends the mark, as the collapse's ends
    /// `expansion`, for a split moves one way at a time. A mark left set held
    /// every later stack and the inspector back (`show`, `inspect`).
    private var collapsing = false
    /// Whether the split is expanded and no longer moving between its forms: UIKit reads it expanded before it has
    /// said so (`splitViewControllerDidExpand`), and a sync can come between.
    var settledExpanded: Bool { !controller.isCollapsed && !expansion && !collapsing }
    /// Whether the person sees the detail's routes: collapsed, only while
    /// nested, as a pop of the detail's controller leaves its routes in it.
    private var showsDetail: Bool { (controller.isCollapsed ? nested : true) && detail.viewControllers.contains { $0 is RouteController } }
    /// The controller whose top the person sees.
    var front: UINavigationController { showsDetail ? detail : list }
    /// The routes UIKit shows, root first.
    var shown: [RouteController] {
        list.viewControllers.prefix(1).compactMap { $0 as? RouteController } + (showsDetail ? detail.viewControllers.compactMap { $0 as? RouteController } : [])
    }
    /// Whether a stack is one of the columns'.
    func holds(_ nav: UINavigationController) -> Bool { nav === list || nav === detail }

    /// The stack, root first, in the two columns: what UIKit shows of it is
    /// UIKit's. A push or pop within the detail is animated as a stack's is;
    /// a new detail replaces the last at once, as a hand-built split's does.
    func show(_ stack: [RouteController], animated: Bool) {
        guard let root = stack.first else { return }
        let moved = self.stack.map(ObjectIdentifier.init) != stack.map(ObjectIdentifier.init)
        self.stack = stack
        // UIKit is moving the detail's controller between the columns: the
        // stack shows once it has expanded or collapsed. Shown now, the
        // columns' stacks would be set from the split's state before the
        // move, and UIKit, finding them changed, pushes the detail twice and
        // asserts.
        if expanding || collapsing { return }
        let rest = Array(stack.dropFirst())
        let current = detail.viewControllers.compactMap { $0 as? RouteController }
        let within = !current.isEmpty && !rest.isEmpty && NavigationRules.isPushOrPop(from: current.map(ObjectIdentifier.init), to: rest.map(ObjectIdentifier.init))
        if controller.isCollapsed {
            if list.viewControllers.first !== root { list.setViewControllers([root], animated: false) }
            if rest.isEmpty {
                // Nested, UIKit pops the detail's controller; out of sight, it empties.
                if nested { controller.show(.primary) } else if !same([empty]) { detail.setViewControllers([empty], animated: false) }
                return
            }
            if !same(rest) { detail.setViewControllers(rest, animated: animated && within && nested) }
            if !nested { controller.show(.secondary) }
            return
        }
        if list.viewControllers.count != 1 || list.viewControllers.first !== root { list.setViewControllers([root], animated: false) }
        let next: [UIViewController] = rest.isEmpty ? [empty] : rest
        if !same(next) { detail.setViewControllers(next, animated: animated && within) }
        // A selection while UIKit shows the list over the detail shows the
        // detail (LLP 1127 D6); any other change (a search typed in the list) leaves the list.
        if moved, !rest.isEmpty, controller.displayMode == .oneOverSecondary { controller.show(.secondary) }
    }

    /// The route the inspector's column shows, while the split is expanded.
    var inspecting: RouteController? {
        guard !controller.isCollapsed, let inspector else { return nil }
        return inspector.viewControllers.first as? RouteController
    }

    /// The routes the columns show beside the stack's top, live: the list
    /// beside its detail, and the detail beside its inspector. A
    /// presentation over the split covers them first
    /// (`NavigationHost.splitShown`).
    var beside: [RouteController] {
        let list = showsList ? list.viewControllers.prefix(1).compactMap { $0 as? RouteController } : []
        let detail = inspecting != nil ? (self.detail.topViewController as? RouteController).map { [$0] } ?? [] : []
        return list + detail
    }

    /// The routes shown under a pane over them, not live: the detail under
    /// the list that overlays it.
    var under: [RouteController] {
        overlaysDetail ? (detail.topViewController as? RouteController).map { [$0] } ?? [] : []
    }

    /// Whether this split can show an inspector's column (UIKit's, iOS 26).
    var inspects: Bool {
        #if os(iOS)
        if #available(iOS 26.0, *) { return true }
        #endif
        return false
    }

    /// The sheet route that says `navigationInspector` in the inspector's
    /// column, or none there (LLP 1127 D14). Collapsed, the column empties at
    /// once, for the route to go to its sheet; closed while expanded, the
    /// route stays until the column's transition is over. `animated` false
    /// puts the column in place at once (a route that leaves its sheet as
    /// the split expands, as UIKit's own inspector is there from the
    /// expansion's first frame).
    func inspect(_ route: RouteController?, in made: UINavigationController?, animated: Bool = true) {
        // tvOS has no inspector's column.
        #if os(iOS)
        guard #available(iOS 26.0, *) else { return }
        // A split that collapsed, or is collapsing, since its route was
        // chosen (a fold during the sync) has no column to show it in: its
        // route goes to its sheet.
        if let route, let nav = made, !controller.isCollapsed, !collapsing {
            // A close still in its transition is superseded: its end must not empty the column.
            closing = nil
            endSlide()
            inspector = nav
            // The column is the split's again after a collapse took it (`topColumnForCollapsing`).
            if controller.viewController(for: .inspector) !== nav { controller.setViewController(nav, for: .inspector) }
            if nav.viewControllers.first !== route { nav.setViewControllers([route], animated: false) }
            if !animated, !controller.isShowing(.inspector) {
                UIView.performWithoutAnimation {
                    controller.show(.inspector)
                    controller.view.layoutIfNeeded()
                }
            } else if !controller.isShowing(.inspector) {
                // UIKit lays its own inspector out where it ends and then
                // slides the column in: the route, which no frame has shown,
                // is told its place still (`NavigationHost.tellStill`).
                controller.show(.inspector)
                moveAlong()
            }
            return
        }
        guard let nav = inspector, !nav.viewControllers.isEmpty else { return endSlide() }
        // Collapsing, the column goes with the split (`topColumnForCollapsing`).
        if controller.isCollapsed || collapsing || controller.viewController(for: .inspector) !== nav {
            nav.setViewControllers([], animated: false)
            return endSlide()
        }
        // A close in its transition keeps its route until it ends.
        guard closing == nil else { return }
        if !controller.isShowing(.inspector) {
            nav.setViewControllers([], animated: false)
            return endSlide()
        }
        closes += 1
        let close = closes
        closing = close
        controller.hide(.inspector)
        moveAlong()
        let empty = { [weak self, weak nav] (cancelled: Bool) in
            guard let self, let nav, closing == close else { return }
            closing = nil
            // A swipe let go takes the column back with its route (`closeInspector`)
            guard !cancelled else { return }
            nav.setViewControllers([], animated: false)
            endSlide()
        }
        if let coordinator = controller.transitionCoordinator { coordinator.animate(alongsideTransition: nil) { empty($0.isCancelled) } } else { empty(false) }
        #endif
    }

    /// UIKit pops the route the inspector's route is above, which takes the
    /// inspector's route too (`NavigationRules.poppedTop`): the column closes
    /// as the pop begins. UIKit's Back slides it away on the split's own
    /// clock. A swipe's pop carries the column's transition (iOS 27.1): the
    /// column follows the finger, leaves with the page when the swipe goes
    /// back, and comes back with the page when it is let go. Closed only as
    /// the finger lifts or as the pop ends, it would stay until the page had
    /// gone, as UIKit holds a split's change while a swipe's pop finishes.
    /// Let go, UIKit still lays the split out as last asked, without the
    /// column, so the column is shown again at once as the pop ends; asked
    /// for later, it would vanish and slide in again. `inspect(nil)` keeps the
    /// route in the column until the column has gone.
    func closeInspector(along pop: UIViewControllerTransitionCoordinator) {
        if pop.isInteractive, let nav = inspector, let route = nav.viewControllers.first as? RouteController {
            pop.animate(alongsideTransition: nil) { [weak self, weak nav, weak route] context in
                // A batch that took the route while the finger was down left it sliding out: the deferred sync decides
                guard context.isCancelled, let self, let nav, let route, sliding == nil, nav.viewControllers.first === route else { return }
                inspect(route, in: nav, animated: false)
            }
        }
        inspect(nil, in: nil)
    }

    /// UIKit animates the columns into place from a show or hide: what the
    /// routes beside them clear is told inside that animation
    /// (`columnsMove`), so their content moves with the columns, as a
    /// hand-built detail's does.
    private func moveAlong() {
        controller.transitionCoordinator?.animate(alongsideTransition: { [weak self] _ in self?.columnsMove() })
    }

    /// Exact's close of the inspector's column, until its transition ends:
    /// UIKit's report of it is not the person's. Each close has its number,
    /// so that an opening or a later close supersedes it.
    private var closing: Int?
    private var closes = 0

    /// The views of the inspector's route a batch destroyed, live while its
    /// column slides out (`NavigationHost.slideOutInspector`).
    var sliding: Leaving?
    /// The sliding route's views go: its column let it go.
    func endSlide() {
        guard let ended = sliding else { return }
        sliding = nil
        for member in ended.members { member.forget() }
        ended.view.removeFromSuperview()
    }

    private func same(_ next: [UIViewController]) -> Bool {
        detail.viewControllers.count == next.count && zip(detail.viewControllers, next).allSatisfy { $0 === $1 }
    }

    // Collapsed, the detail is on top while it holds a route (LLP 1127 D3's answer).
    func splitViewController(_ svc: UISplitViewController,
                             topColumnForCollapsingToProposedTopColumn proposedTopColumn: UISplitViewController.Column) -> UISplitViewController.Column {
        expansion = false
        collapsing = true
        // Collapsing with its inspector shown, UIKit presents the inspector's
        // column as a sheet of its own: the route goes to the sheet it says
        // instead (LLP 1127 D14), and the column, emptied for it, would be a
        // blank sheet over it. Out of the split, the column is not presented.
        #if os(iOS)
        if #available(iOS 26.0, *), let inspector, svc.viewController(for: .inspector) === inspector { svc.setViewController(nil, for: .inspector) }
        #endif
        return detail.viewControllers.contains { $0 is RouteController } ? .secondary : .primary
    }
    func splitViewController(_ svc: UISplitViewController,
                             displayModeForExpandingToProposedDisplayMode proposedDisplayMode: UISplitViewController.DisplayMode) -> UISplitViewController.DisplayMode {
        collapsing = false
        expansion = true
        return proposedDisplayMode
    }

    func splitViewController(_ svc: UISplitViewController, didHide column: UISplitViewController.Column) {
        #if os(iOS)
        guard #available(iOS 26.0, *), column == .inspector, closing == nil, !svc.isCollapsed else { return }
        // A report for a column shown again by then (a swipe let go put it back) is no close of the person's
        DispatchQueue.main.async { if !svc.isShowing(.inspector) { self.inspectorClosed() } }
        #endif
    }

    func splitViewControllerDidCollapse(_ svc: UISplitViewController) {
        expansion = false
        collapsing = false
        show(stack, animated: false)
        DispatchQueue.main.async { self.changed() }
    }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) {
        collapsing = false
        expansion = false
        // Collapsed, a pop of the detail's controller left its routes in it.
        show(stack, animated: false)
        // At the list after a Back while collapsed, the detail goes back to the page the person left. Asked once the
        // expansion is over, as the router's answer changes the columns.
        if stack.count == 1, let location = left {
            DispatchQueue.main.async { [weak self] in
                guard let self, stack.count == 1, !controller.isCollapsed else { return }
                returnTo(location)
            }
        }
        DispatchQueue.main.async { self.changed() }
    }
    func splitViewController(_ svc: UISplitViewController, willChangeTo displayMode: UISplitViewController.DisplayMode) {
        if let coordinator = svc.transitionCoordinator {
            columnsAboutToMove = coordinator.animate(alongsideTransition: { [weak self] _ in self?.columnsAboutToMove = false },
                                                     completion: { [weak self] _ in self?.columnsAboutToMove = false })
        }
        DispatchQueue.main.async { self.changed() }
    }
}

extension Presenter {
    /// Whether a grouped list shows as UIKit's sidebar: in a split's list column beside its detail.
    package func showsAsSidebar(_ list: UIView) -> Bool { navigation.showsAsSidebar(list) }
    /// The row of a sidebar whose detail shows, which stays selected.
    package func sidebarSelection(_ list: UIView) -> UInt32? { navigation.sidebarSelection(list) }
    /// Whether a view is in a split's column whose ground is UIKit's: the sidebar or the inspector.
    package func onColumnGround(_ view: UIView) -> Bool { navigation.onColumnGround(view) }
    /// Whether `id`'s view is the inspector's route's, or in it, live while its column slides out.
    package func slidesOut(_ id: UInt32) -> Bool { navigation.split?.sliding?.members.contains { $0.id == id } == true }
}

extension NavigationHost {
    /// Whether the stack whose first route is `root` shows as UIKit's split:
    /// its list's rows push, by the runner's rule, under the platform's chrome.
    /// An installed split keeps its list while that list has no row to act on.
    func wantsSplit(_ root: RouteController?) -> Bool {
        // tvOS takes it once a hand-built split is measured with the focus engine (LLP 1127 §4).
        #if os(tvOS)
        return false
        #else
        guard !ExactEnv.authoredChrome, let root, let session = presenter.session else { return false }
        return session.runtime.stackSplit(root.node.id, retaining: split?.listNode) != nil
        #endif
    }

    /// The split's two columns, each a navigation controller of its own, as
    /// the primary owner.
    func installSplit(_ p: Projection, first: [RouteController], in parent: UIViewController) {
        let list = makeNavigation(first: first.first?.node)
        let detail = makeNavigation(first: nil)
        let split = SplitIOS(list: list, detail: detail)
        split.listNode = first.first.flatMap { presenter.session?.runtime.stackSplit($0.node.id, retaining: nil)?.list }
        split.changed = { [weak self] in self?.splitChanged() }
        split.returnTo = { [weak self] location in self?.go(to: location) }
        split.inspectorClosed = { [weak self] in self?.inspectorClosed() }
        split.columnsMove = { [weak self] in self?.reportCovers(); self?.presenter.owners.publish() }
        self.split = split
        primaryNavigation = list
        prepareRoutes(Array(first.prefix(1)), in: list)
        prepareRoutes(Array(first.dropFirst()), in: detail, firstPushed: true)
        detail.setNavigationBarHidden(false, animated: false)
        split.show(first, animated: false)
        noteDetail(split, first)
        for nav in [list, detail] { recordOwned(nav); watchPops(nav) }
        parent.addChild(split.controller)
        split.controller.view.frame = ownerFrame(split.controller, in: p.root)
        split.controller.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        p.root.addSubview(split.controller.view)
        split.controller.view.setPaintForeground(aboveAuthored: false)
        fitOwner()
        split.controller.didMove(toParent: parent)
    }

    /// The stack in the split's columns, its routes projected into each.
    func syncSplit(_ split: SplitIOS, _ stack: [RouteController], animated: Bool) {
        prepareRoutes(Array(stack.prefix(1)), in: split.list)
        prepareRoutes(Array(stack.dropFirst()), in: split.detail, firstPushed: true)
        let shown = split.shown
        split.show(stack, animated: animated)
        noteDetail(split, stack)
        for nav in [split.list, split.detail] { recordOwned(nav) }
        // A sidebar's selected row follows the detail.
        if split.shown != shown, split.showsList, let root = stack.first,
           let list = presenter.session?.runtime.stackSplit(root.node.id, retaining: nil)?.list {
            presenter.groupedLists?.sync(changed: [list])
        }
    }

    /// The page the detail shows, which the split returns to after a Back to the list while collapsed
    /// (`SplitIOS.left`). The stack at its root on a split settled expanded leaves none: the router took the page
    /// away, not a Back the person made from it on the collapsed split.
    private func noteDetail(_ split: SplitIOS, _ stack: [RouteController]) {
        guard let root = stack.first, let top = stack.dropFirst().last else {
            if split.settledExpanded { split.left = nil }
            return
        }
        // Read each time: a `replace` keeps the route and changes its location
        if let location = presenter.session?.runtime.stackSplit(root.node.id, retaining: nil)?.locations[top.node.id] { split.left = location }
    }

    /// The router goes to `location`, as a link there takes it: the root's `navigate`, else the runner's own `go`.
    /// Nothing is asked under a presentation, as the router would push the page onto the presented stack.
    private func go(to location: String) {
        guard let session = presenter.session, presenter.modals.routes.isEmpty else { return }
        if let root = container, root.handlers.contains("navigate") { session.navigate(location, at: root) } else { session.hostGo(location) }
    }

    /// UIKit changed what the split shows: the bars' insets, which columns'
    /// routes are live, and the list's appearance follow.
    func splitChanged() {
        // Beside the detail, the list and its route are on UIKit's sidebar.
        if let root = split?.list.viewControllers.first as? RouteController { repaintGrounds(root) }
        coversChanged()
        presenter.owners.publish()
        presenter.groupedLists?.sync(changed: nil)
        // An inspector moves between its column and its sheet as the split expands and collapses.
        if selectedRoute?.props["navigationInspector"] == "true" { sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)) }
    }

    /// The sheet an expanded split shows as its inspector (`navigationInspector`,
    /// LLP 1127 D14): the stack's one sheet, of one route.
    func inspectorRoute(_ parts: [Range<Int>], _ wanted: [RouteController]) -> RouteController? {
        guard let split, split.inspects, !split.controller.isCollapsed, parts.count == 2, parts[1].count == 1 else { return nil }
        let route = wanted[parts[1].lowerBound]
        return route.node.props["navigationInspector"] == "true" ? route : nil
    }

    /// The route the split's inspector column shows, or none
    /// (`SplitIOS.inspect`); whether the column holds `inspecting` now.
    @discardableResult
    func placeInspector(_ inspecting: RouteController?, animated: Bool = true) -> Bool {
        guard let split else { return false }
        let nav = inspecting.map { split.inspector ?? makeNavigation(first: $0.node) }
        if let nav, let inspecting { prepareRoutes([inspecting], in: nav) }
        let was = split.inspecting
        split.inspect(inspecting, in: nav, animated: animated)
        if let nav { recordOwned(nav) }
        for route in [was, inspecting].compactMap({ $0 }) where was !== inspecting { repaintGrounds(route) }
        return inspecting != nil && split.inspecting === inspecting
    }

    /// Tells `tell` what is to be told of the routes' boxes, keyed by node.
    /// Inside an animation UIKit has open, the part about routes no frame has
    /// shown (a route a selection shows as the split's columns move, the
    /// inspector's as its column comes in) is told first and still, and
    /// their controllers' views laid out with it: UIKit lays a view out where
    /// it ends before it animates it in, and a new route told inside the
    /// animation moved from where it was laid out before, unseen. The rest,
    /// in a last call that may be empty, moves with the animation.
    func tellStill<A, B>(_ a: [(UInt32, A)], _ b: [(UInt32, B)], _ tell: ([(UInt32, A)], [(UInt32, B)]) -> Void) {
        guard UIView.inheritedAnimationDuration > 0 else { return tell(a, b) }
        var unshown: [UInt32: RouteController] = [:]
        for id in a.map(\.0) + b.map(\.0) {
            guard let node = presenter.views[id],
                  let route = ViewPresentation.controllers(for: node).lazy.compactMap({ $0 as? RouteController }).first,
                  !presenter.showed(route.node) else { continue }
            unshown[id] = route
        }
        guard !unshown.isEmpty else { return tell(a, b) }
        UIView.performWithoutAnimation {
            tell(a.filter { unshown[$0.0] != nil }, b.filter { unshown[$0.0] != nil })
            var laidOut = Set<ObjectIdentifier>()
            for route in unshown.values where laidOut.insert(ObjectIdentifier(route)).inserted { route.viewIfLoaded?.layoutIfNeeded() }
        }
        tell(a.filter { unshown[$0.0] == nil }, b.filter { unshown[$0.0] == nil })
    }

    /// A batch destroys the route the inspector's column shows: its views
    /// stay live while the column slides out, as UIKit keeps a hidden
    /// column's, retired from the presenter as an exit retires a view
    /// (`retire`). They go once the column has let the route go
    /// (`SplitIOS.endSlide`): closed, emptied, or showing another route. A
    /// snapshot of them lacked what UIKit draws live: the segmented
    /// control's glass and the grouped rows' cards.
    func slideOutInspector(_ batch: Batch) {
        guard let split, let route = split.inspector?.topViewController as? RouteController,
              batch.ops.contains(where: { $0.op == .destroy && $0.id == route.node.id }) else { return }
        split.endSlide()
        split.sliding = presenter.retire(route.node.id)
    }

    /// The selected route's key while it is the inspector's (LLP 1127 D14):
    /// a sheet that says `navigationInspector`, which an expanded split shows
    /// in its column above the detail's top. The app's stack says so, not the
    /// column, which a pop under it may already be sliding away
    /// (`SplitIOS.closeInspector`).
    var inspectorKey: String? {
        guard let split, split.inspects, !split.controller.isCollapsed, selectedRoute?.props["navigationInspector"] == "true" else { return nil }
        return selectedKey
    }

    /// The person closed the inspector's column: Back from its route, as a
    /// swipe down that closes its sheet is.
    func inspectorClosed() {
        guard let route = split?.inspector?.viewControllers.first as? RouteController, route.key == selectedKey else { return }
        goBack()
    }

    /// How the split's routes show against the stack's top (LLP 1127 D8),
    /// which the host's checks read and the runner is told: as the columns
    /// show them while nothing is presented over the split, the inspector's
    /// column being the split's own (D14). A presentation covers what is
    /// under it first: under sheets the routes the columns show are shown
    /// and not live; a full-screen one hides them.
    var splitShown: (beside: [RouteController], under: [RouteController]) {
        guard let split else { return ([], []) }
        let over = presenter.modals.routes
        if over.isEmpty { return (split.beside, split.under) }
        return ([], over.allSatisfy { $0.kind == "modal" } ? split.beside + split.under : [])
    }

    /// Whether `view` is in the split's list column while the split is
    /// expanded: UIKit's sidebar, which keeps its look as it slides away (an
    /// inspector coming in on the iPhone Duo hides it) and while hidden.
    func showsAsSidebar(_ view: UIView) -> Bool {
        guard let split, !split.controller.isCollapsed, let list = split.list.viewControllers.first?.viewIfLoaded else { return false }
        return view.isDescendant(of: list)
    }

    /// Whether `view` is in a split's column whose ground is UIKit's: the
    /// list beside its detail (the sidebar), or the inspector, sliding out too.
    func onColumnGround(_ view: UIView) -> Bool {
        if showsAsSidebar(view) { return true }
        if let sliding = split?.sliding?.view, view.isDescendant(of: sliding) { return true }
        guard let route = split?.inspecting?.viewIfLoaded else { return false }
        return view.isDescendant(of: route)
    }

    /// A route's grounds, its grouped lists' and its boxes on the page's
    /// ground follow the column it is in.
    func repaintGrounds(_ route: RouteController) {
        route.paintBackdrop()
        route.node.applyBoxLayer()
        for box in presenter.views.values where box !== route.node && box.paintsPageGround && box.isDescendant(of: route.node) { box.applyBoxLayer() }
        let lists = presenter.carrying("listStyle").filter { $0.kind == "list" && $0.isDescendant(of: route.node) }
        for list in lists { list.applyBoxLayer() }
        if !lists.isEmpty { presenter.groupedLists?.sync(changed: Set(lists.map(\.id))) }
    }

    /// The row of the split's list whose detail shows: the detail's highest
    /// route that is a row's (`StackSplit.rows`), a page beneath what it pushed.
    func sidebarSelection(_ view: UIView) -> UInt32? {
        guard showsAsSidebar(view), let split, let root = split.list.viewControllers.first as? RouteController,
              let rule = presenter.session?.runtime.stackSplit(root.node.id, retaining: nil) else { return nil }
        return split.detail.viewControllers.reversed().lazy.compactMap { ($0 as? RouteController).flatMap { rule.rows[$0.node.id] } }.first
    }

    /// The split goes. `releasing`, its routes leave its columns first, for
    /// the containers that take them next (`reshape`).
    func retireSplit(releasing: Bool = false) {
        guard let split else { return }
        split.endSlide()
        for nav in [split.list, split.detail] + [split.inspector].compactMap({ $0 }) {
            retireStack(nav)
            if releasing { nav.delegate = nil; nav.setViewControllers([], animated: false) }
        }
        split.detail.delegate = nil
        split.controller.willMove(toParent: nil)
        split.controller.view.removeFromSuperview()
        split.controller.removeFromParent()
        self.split = nil
    }
}
#endif
