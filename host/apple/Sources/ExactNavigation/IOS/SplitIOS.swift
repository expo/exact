// @ref LLP 1127 D2, D5, D6, D8 — a list and its detail on iOS are UIKit's
// split view. Where a stack's root route holds a list whose rows push (the
// runner's rule, `StackSplit`), the stack is the two columns of a
// `.doubleColumn` `UISplitViewController`: the root route in the list's
// navigation controller (N0), the routes above it in the detail's (N1). It
// is installed at every size, and how many columns show, their widths and
// where the bars stand are UIKit's. Collapsed (a compact width: an iPhone,
// the iPhone Duo folded), UIKit nests N1 in N0 and takes it back as it
// expands (LLP 1127 §1, item 2), so Back from a collapsed detail is N0's pop
// of N1, with N1 still holding its routes.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// UIKit's split, which says when a layout changed what it shows: a split
/// laid out wide from the start sends its delegate no expansion.
private final class SplitController: UISplitViewController {
    var laidOut: () -> Void = {}
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        laidOut()
    }
}

final class SplitIOS: NSObject, UISplitViewControllerDelegate {
    private let write: ([UIViewController], UINavigationController, Bool) -> Bool
    let controller: UISplitViewController = SplitController(style: .doubleColumn)
    /// The list's column, N0: the stack's root route.
    let list: UINavigationController
    /// The detail's column, N1: the routes above the root.
    let detail: UINavigationController
    /// The detail while the stack is at its root (LLP 1127 D7).
    let empty = UIViewController()
    /// What UIKit shows changed: it collapsed or expanded, or showed or hid the list.
    var changed: () -> Void = {}
    var columnsWillChange: () -> Void = {}
    var listWillBeCovered: () -> Void = {}
    var columnsDidChange: () -> Void = {}
    var left: String?
    var returnTo: (String) -> Void = { _ in }
    /// The inspector's column (iOS 26), made when a route first goes there.
    private(set) var inspector: UINavigationController?
    /// The person closed the inspector's column; the router has yet to hear it.
    var inspectorClosed: () -> Void = {}
    #if os(iOS)
    private(set) var searchState = SplitSearchState()
    private(set) weak var searchOwner: RouteController?
    var pairedSearch: HeaderSearch? { searchOwner?.search.flatMap { $0.paired ? $0 : nil } }
    var searchProjectionEpoch: UInt64 { searchState.projectionEpoch }
    var stageForExpansion: () -> Void = {}
    #endif
    var paired: Bool {
        #if os(iOS)
        pairedSearch != nil
        #else
        false
        #endif
    }
    private(set) var writing = false
    var columnsAboutToMove: Bool { aboutToMove != nil }
    var showsList: Bool { !controller.isCollapsed && controller.displayMode != .secondaryOnly }
    var overlaysDetail: Bool { !controller.isCollapsed && [.oneOverSecondary, .twoOverSecondary].contains(controller.displayMode) }
    var projectedDetailTop: RouteController? { stack.dropFirst().last }

    /// UIKit is moving the columns, inside its animation (`moveAlong`).
    var columnsMove: () -> Void = {}
    /// UIKit is about to push a route within N1, over the route beneath it.
    var willPush: (_ route: RouteController, _ beneath: RouteController) -> Void = { _, _ in }
    /// N1 has a new base, or none: prepare N1's own transition, which N0's
    /// push and pop of N1 play, as the base's zoom
    /// (`NavigationHost.prepareColumnTransition`).
    var baseChanged: (_ base: RouteController?) -> Void = { _ in }
    /// The base N1's own transition was last prepared for, and that
    /// transition (`UIViewController.Transition`, iOS 18).
    private weak var transitionBase: RouteController?
    private var baseTransition: AnyObject?
    /// The list whose rows made the stack a split, which keeps it one while
    /// it has no row to act on (`Runtime.stackSplit`'s `retaining`).
    var listNode: UInt32?
    /// The row the sidebar list was last told is the detail's (`syncSplit`).
    var sidebarRow: UInt32?
    /// Each base a row's press brought or moved, by its route, with the row
    /// and the base's location then (`NavigationHost.listPress`).
    var pressedRows: [UInt32: (row: UInt32, at: String?)] = [:]
    /// The rows' presses, oldest first, until the session holds none of
    /// their batches (`NavigationHost.listPress`).
    var presses: [Press] = []
    struct Press {
        let row: UInt32
        /// The batches the press made, numbered above `after` and through
        /// `through`, and whether one of them has synced (`creditPresses`).
        let after: UInt64
        var through: UInt64 = .max, landed = false
    }
    /// The base, by its route, and its location, as the last batch synced
    /// left them (`NavigationHost.creditPresses`).
    var lastBase: (id: UInt32, at: String?)?
    /// The runner's rule as this sync's `reshape` read it, and the location
    /// of N1's top when it was last shown: a `replace` keeps the route and
    /// moves its location, and is a new detail all the same (`show`).
    var rule: StackSplit?
    var topLocation: String?
    /// The latest requested stack, root first, reapplied when the move settles.
    private var stack: [RouteController] = []
    /// What `changed` last reported.
    private var told: Showing?
    private struct Showing: Equatable {
        let collapsed: Bool, mode: UISplitViewController.DisplayMode, nested: Bool
    }
    /// UIKit's expansion or collapse, from the question it asks the delegate
    /// until it says it is done (`moving`).
    private var move: Move?
    private enum Move { case expanding, collapsing }

    init(list: UINavigationController, detail: UINavigationController,
         write: @escaping ([UIViewController], UINavigationController, Bool) -> Bool) {
        self.write = write
        self.list = list
        self.detail = detail
        super.init()
        var unavailable = UIContentUnavailableConfiguration.empty()
        // English until the host has a strings table for its own words, as on the web (LLP 1116.006 D2)
        unavailable.text = "No Selection"
        empty.contentUnavailableConfiguration = unavailable
        #if os(iOS)
        empty.view.backgroundColor = .systemBackground
        #endif
        controller.delegate = self
        #if os(iOS)
        // The list's column is a sidebar, in UIKit's style for one, as SwiftUI's split sets it (LLP 1127 D7)
        controller.primaryBackgroundStyle = .sidebar
        #endif
        (controller as? SplitController)?.laidOut = { [weak self] in self?.laidOut() }
        controller.setViewController(list, for: .primary)
        controller.setViewController(detail, for: .secondary)
    }

    /// Whether UIKit is moving N1 between the columns: expanding, from asking
    /// which display mode to expand to until it has, while the split still
    /// reads collapsed (an iPhone Duo unfolding); collapsing, from asking
    /// which column to put on top until it says it collapsed, while it still
    /// reads expanded. A move of N1 meanwhile is no Back.
    var moving: Bool {
        switch move {
        case .expanding?: controller.isCollapsed
        case .collapsing?: true
        case nil: false
        }
    }
    /// UIKit is transferring navigation items between columns. Laying out their bars here can assert.
    var laysOutItself: Bool { move != nil || aboutToMove != nil }
    private weak var aboutToMove: UIViewControllerTransitionCoordinator?
    private weak var along: UIViewControllerTransitionCoordinator?

    /// Whether UIKit nests N1 in N0 now (collapsed, or collapsing).
    var settledExpanded: Bool { !controller.isCollapsed && !moving }
    var settledCollapsed: Bool { controller.isCollapsed && !moving }
    var nested: Bool { list.viewControllers.contains { $0 === detail } }
    /// Whether N1's routes are in the stack UIKit shows: in their column, or
    /// nested in N0. Collapsed, a pop of N1 off N0 leaves its routes in it.
    var showsDetail: Bool { moving || !controller.isCollapsed || nested }
    /// The routes N1 holds.
    var detailRoutes: [RouteController] { detail.stackRoutes.compactMap { $0 as? RouteController } }
    /// The controller whose top the person sees.
    var front: UINavigationController { showsDetail && !detailRoutes.isEmpty ? detail : list }
    /// The routes UIKit shows as one stack, root first.
    var shown: [RouteController] {
        list.stackRoutes.prefix(1).compactMap { $0 as? RouteController } + (showsDetail ? detailRoutes : [])
    }
    #if os(iOS)
    func setSearchOwner(_ owner: RouteController, children: [UInt32]) {
        guard searchOwner !== owner else { return }
        if paired { searchOwner?.detachSearch() }
        FrameClock.shared.drop(searchState)
        searchState.showInstalledDetail()
        searchState = SplitSearchState()
        searchOwner = owner
        searchState.children = children
    }

    func stageExpansion(_ stack: [RouteController], awaitingCommit: Bool = false) {
        let routes = Array(stack.dropFirst())
        let next: [UIViewController] = routes.isEmpty ? [empty] : routes
        if awaitingCommit || writing || list.transitionCoordinator != nil || detail.transitionCoordinator != nil {
            searchState.concealDetail(in: detail.view)
        } else if !Self.same(detail.viewControllers, next) {
            writing = true
            defer { writing = false }
            keepTransition(routes)
            if !write(next, detail, false) { searchState.concealDetail(in: detail.view) }
        }
    }

    func reconcileMoveAfterLayout() {
        if move != nil, controller.viewIfLoaded?.window != nil,
           controller.transitionCoordinator == nil, list.transitionCoordinator == nil, detail.transitionCoordinator == nil {
            settle()
        }
    }
    #endif

    /// The route the inspector's column shows, while the split is expanded.
    var inspecting: RouteController? {
        guard !controller.isCollapsed, let inspector else { return nil }
        return inspector.viewControllers.first as? RouteController
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
        if let route, let nav = made, !controller.isCollapsed, move != .collapsing {
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
        if controller.isCollapsed || move == .collapsing || controller.viewController(for: .inspector) !== nav {
            empty(nav)
            return endSlide()
        }
        // A close in its transition keeps its route until it ends.
        guard closing == nil else { return }
        if !controller.isShowing(.inspector) {
            empty(nav)
            return endSlide()
        }
        closes += 1
        let close = closes
        closing = close
        controller.hide(.inspector)
        moveAlong()
        let finish = { [weak self, weak nav] (cancelled: Bool) in
            guard let self, let nav, closing == close else { return }
            closing = nil
            guard !cancelled else { return }
            empty(nav)
            endSlide()
        }
        if let coordinator = controller.transitionCoordinator { coordinator.animate(alongsideTransition: nil) { finish($0.isCancelled) } } else { finish(false) }
        #endif
    }

    #if os(iOS)
    func reconcileInspectorClose() {
        guard #available(iOS 26.0, *), let closing, let inspector, controller.transitionCoordinator == nil else { return }
        if !controller.isShowing(.inspector) {
            empty(inspector)
            endSlide()
        }
        if self.closing == closing { self.closing = nil }
    }
    #endif

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
        // One move per transition: a sync before UIKit lays the columns out shows the column again
        guard let coordinator = controller.transitionCoordinator, coordinator !== along else { return }
        along = coordinator
        holdLayout(until: coordinator) { [weak self] in self?.columnsMove() }
    }

    /// Nothing here lays the columns out until `coordinator` opens its animation (`columnsAboutToMove`), which then
    /// runs `alongside`. Only this transition's start or end lets go of its own hold: a later one that took over keeps its own.
    private func holdLayout(until coordinator: UIViewControllerTransitionCoordinator, alongside: (() -> Void)? = nil) {
        aboutToMove = coordinator
        let letGo = { [weak self, weak coordinator] in
            guard let self, let coordinator, aboutToMove === coordinator else { return }
            aboutToMove = nil
        }
        let taken = coordinator.animate(alongsideTransition: { _ in
            letGo()
            alongside?()
        }, completion: { _ in letGo() })
        if !taken { letGo() }
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

    /// Empty the inspector's column. Its route is Exact's to move (to its
    /// sheet, folded) or to let go, not one UIKit popped: no stack holds it
    /// until Exact writes it again (`RouteController.held`).
    private func empty(_ nav: UINavigationController) {
        let routes = nav.viewControllers.compactMap { $0 as? RouteController }
        nav.setViewControllers([], animated: false)
        for route in routes { route.held = false }
    }

    /// Whether `nav` is one of the columns'.
    func holds(_ nav: UINavigationController) -> Bool { nav === list || nav === detail || nav === inspector }

    /// Whether UIKit is moving the columns or their stacks: a push or pop in
    /// either, the split's own transition (`show(_:)`, a rotation), or an
    /// expansion or a collapse (LLP 1127 D5.8).
    var inTransition: Bool {
        moving || controller.transitionCoordinator != nil
            || [list, detail].contains { $0.transitionCoordinator?.presentationStyle == UIModalPresentationStyle.none }
    }

    /// How deep the stack is that `nav`'s pop recognizer pops, for the gate
    /// (`NavigationHost.popMayBegin`, LLP 1127 D5.3); nil for a stack not the
    /// split's. N1's pops its routes while it shows them. Collapsed, N0's pops
    /// N1 only while N1 holds just one route: with more, it would take them
    /// all, bypassing a refused Back on the top one.
    func popDepth(_ nav: UINavigationController) -> Int? {
        if nav === detail { return showsDetail ? detailRoutes.count : 0 }
        guard nav === list else { return nil }
        return nested && detailRoutes.count == 1 ? 2 : 0
    }

    /// The controller whose bar holds Back for the stack, and how many
    /// routes that Back goes through: collapsed, N0's over the whole stack;
    /// expanded, N1's over its own routes.
    var backBar: (nav: UINavigationController, depth: Int) {
        controller.isCollapsed && !moving ? (list, shown.count) : (detail, detailRoutes.count)
    }

    /// How UIKit shows the columns against the stack's top while nothing is
    /// presented over them (LLP 1127 D8): beside the detail the list is live
    /// with it, and over the detail only the list is. With the detail alone
    /// only N1's top is live, so a list the stack's top is shows and is not
    /// live there. Collapsed, the routes are one stack.
    var showing: (beside: [RouteController], under: [RouteController]) {
        guard !controller.isCollapsed, let root = list.stackRoutes.first as? RouteController else { return ([], []) }
        let top = detailRoutes.last
        if inspecting != nil {
            switch controller.displayMode {
            case .oneOverSecondary, .twoOverSecondary: return ([root], top.map { [$0] } ?? [])
            case .secondaryOnly: return (top.map { [$0] } ?? [], [])
            default: return ([root] + (top.map { [$0] } ?? []), [])
            }
        }
        switch controller.displayMode {
        case .oneOverSecondary, .twoOverSecondary: return ([root], top.map { [$0] } ?? [])
        case .secondaryOnly: return ([], top == nil ? [root] : [])
        default: return ([root], [])
        }
    }

    /// The stack, root first, in the two columns; what UIKit shows of it is
    /// UIKit's. A push or pop within the detail is animated as a stack's is;
    /// a new detail replaces the last at once, as a hand-built split's does.
    /// `show(_:)` reveals a new detail under a list shown over it, and the
    /// list over the detail alone when the stack goes back to its root
    /// (LLP 1127 D6); collapsed, it pushes and pops N1. `relocated`: N1's
    /// top stays and shows another location. Returns whether the projection
    /// went through, including a stack already in place.
    @discardableResult
    func show(_ stack: [RouteController], animated: Bool, relocated: Bool = false, revealsDetail: Bool? = nil) -> Bool {
        self.stack = stack
        // UIKit is moving N1 between the columns: the stack shows once it has
        // expanded or collapsed (`settle`). Written now, the columns would be
        // set from the split's state before the move, and UIKit, finding
        // them changed, pushed the detail twice and asserted.
        if laysOutItself { return false }
        writing = true
        defer { writing = false }
        guard let root = stack.first else {
            keepTransition([])
            if !list.viewControllers.isEmpty, !write([], list, false) { return false }
            return Self.same(detail.viewControllers, [empty]) || write([empty], detail, false)
        }
        let rest = Array(stack.dropFirst())
        let current = detailRoutes
        // Compare with the installed columns so a request kept during a move still reveals its detail
        let moved = relocated || list.stackRoutes.first !== root || !current.elementsEqual(rest, by: ===)
        let hadDetail = !current.isEmpty
        let within = !current.isEmpty && !rest.isEmpty
            && NavigationRules.isPushOrPop(from: current.map(ObjectIdentifier.init), to: rest.map(ObjectIdentifier.init))
        let column: [UIViewController] = [root] + (nested ? [detail] : [])
        if !Self.same(list.viewControllers, column), !write(column, list, false) { return false }
        let collapsed = controller.isCollapsed
        let revealDetail = revealsDetail ?? moved
        var showsPrimary = rest.isEmpty
        #if os(iOS)
        if paired {
            if revealDetail && !rest.isEmpty { searchState.collapseToPrimary = false }
            showsPrimary = showsPrimary || searchState.collapseToPrimary
        }
        #endif
        if collapsed, nested, showsPrimary {
            // Let UIKit pop nested N1 with its routes and zoom still in place
            reveal(.primary, animated: animated)
            return true
        }
        let next: [UIViewController] = rest.isEmpty ? [empty] : rest
        let writingDetail = !Self.same(detail.viewControllers, next)
        if !collapsed || !rest.isEmpty || writingDetail { keepTransition(rest) }
        if writingDetail {
            // Animate within N1 when it is nested or expanded. Expanded, a replacement over the
            // same base arrives as a push and a replacement of the base swaps in place.
            let arrives = !current.isEmpty && !current.contains { $0 === rest.last }
            let movesWithinDetail = collapsed ? nested && (within || arrives)
                : within || (arrives && rest.count > 1 && current.first === rest.first)
            if movesWithinDetail, let top = rest.last, !current.contains(where: { $0 === top }), collapsed || rest.count > 1 {
                willPush(top, rest.count > 1 ? rest[rest.count - 2] : root)
            }
            guard write(next, detail, animated && movesWithinDetail) else { return false }
        }
        if collapsed {
            // UIKit animates the push of N1 itself when it is outside N0
            if !rest.isEmpty, !nested, !paired || revealDetail { reveal(.secondary, animated: animated) }
        } else if moved || revealDetail {
            if !rest.isEmpty, revealDetail, overlaysDetail { reveal(.secondary, animated: animated) }
            if rest.isEmpty, hadDetail, controller.displayMode == .secondaryOnly { reveal(.primary, animated: animated) }
        }
        return true
    }

    /// N1's own transition for `rest`, the routes it holds: its base's zoom,
    /// prepared again when the base changes (expanded or collapsed), so that
    /// N0 never pushes or pops N1 with an earlier base's zoom, whose source
    /// may be gone; and none while N1 holds more than its base, so that N0's
    /// pop of N1 is its top's, as a pushed stack's pop from that top is.
    private func keepTransition(_ rest: [RouteController]) {
        #if os(iOS)
        guard #available(iOS 18.0, *) else { return }
        if rest.first !== transitionBase || rest.isEmpty {
            transitionBase = rest.first
            baseChanged(rest.first)
            baseTransition = detail.preferredTransition
        }
        detail.preferredTransition = rest.count == 1 ? baseTransition as? UIViewController.Transition : nil
        #endif
    }

    /// UIKit's `show(_:)`, animated as UIKit animates it only where `animated`
    /// (not under the agent's frozen clock, nor before the split's first frame).
    private func reveal(_ column: UISplitViewController.Column, animated: Bool) {
        if animated { controller.show(column) } else { UIView.performWithoutAnimation { controller.show(column) } }
    }

    private static func same(_ held: [UIViewController], _ next: [UIViewController]) -> Bool { held.elementsEqual(next, by: ===) }

    /// After each layout: what shows is reported when it changed, and a move
    /// UIKit ended without saying so ends here, its stack shown.
    private func laidOut() {
        if move != nil, !moving || (move == .collapsing && controller.isCollapsed && controller.transitionCoordinator == nil) { settle() }
        let now = Showing(collapsed: controller.isCollapsed, mode: controller.displayMode, nested: nested)
        guard now != told else { return }
        told = now
        DispatchQueue.main.async { [weak self] in self?.changed() }
    }

    /// UIKit expanded or collapsed: the stack shows as the columns now are.
    private func settle() {
        let expanded = !controller.isCollapsed && move == .expanding
        move = nil
        if !paired { show(stack, animated: false) }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            changed()
            columnsDidChange()
            if expanded, !paired, stack.count == 1, let location = left { returnTo(location) }
        }
    }

    // Collapsed, the detail is on top while N1 holds a route (LLP 1127 D3's
    // answer), wherever a presentation over the split was opened from.
    func splitViewController(_ svc: UISplitViewController,
                             topColumnForCollapsingToProposedTopColumn proposedTopColumn: UISplitViewController.Column) -> UISplitViewController.Column {
        move = .collapsing
        columnsWillChange()
        #if os(iOS)
        if #available(iOS 26.0, *), let inspector, svc.viewController(for: .inspector) === inspector {
            svc.setViewController(nil, for: .inspector)
        }
        #endif
        #if os(iOS)
        if paired {
            searchState.beginOwnerMove()
            searchState.collapseToPrimary = detailRoutes.isEmpty || searchState.primaryForeground
            changed()
            return searchState.collapseToPrimary ? .primary : .secondary
        }
        #endif
        let coversList = !detailRoutes.isEmpty
        if coversList { listWillBeCovered() }
        else { for route in detailRoutes { route.held = false } }
        return coversList ? .secondary : .primary
    }
    func splitViewController(_ svc: UISplitViewController,
                             displayModeForExpandingToProposedDisplayMode proposedDisplayMode: UISplitViewController.DisplayMode) -> UISplitViewController.DisplayMode {
        move = .expanding
        columnsWillChange()
        #if os(iOS)
        if paired {
            searchState.collapseToPrimary = false
            searchState.beginOwnerMove()
            stageForExpansion()
            changed()
            if searchState.primaryForeground && proposedDisplayMode == .secondaryOnly { return .oneOverSecondary }
        }
        #endif
        return proposedDisplayMode
    }
    func splitViewController(_ svc: UISplitViewController, didHide column: UISplitViewController.Column) {
        #if os(iOS)
        guard #available(iOS 26.0, *), column == .inspector, closing == nil, !svc.isCollapsed else { return }
        DispatchQueue.main.async { if !svc.isShowing(.inspector) { self.inspectorClosed() } }
        #endif
    }

    func splitViewControllerDidCollapse(_ svc: UISplitViewController) {
        settle()
    }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) { settle() }
    func splitViewController(_ svc: UISplitViewController, willChangeTo displayMode: UISplitViewController.DisplayMode) {
        #if os(iOS)
        if paired { searchState.beginOwnerMove() }
        #endif
        if let coordinator = svc.transitionCoordinator { holdLayout(until: coordinator) }
        DispatchQueue.main.async { [weak self] in self?.changed() }
    }
}
#endif
