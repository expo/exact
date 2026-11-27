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
    /// UIKit is about to push a route within N1, over the route beneath it.
    var willPush: (_ route: RouteController, _ beneath: RouteController) -> Void = { _, _ in }
    /// UIKit is about to move N1 between the columns: read the person's pops
    /// now, while UIKit still tells them apart from its move (LLP 1127 D15).
    var willMove: () -> Void = {}
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
    /// The list over the detail alone, owed by a return to the root that a
    /// collapse or an expansion interrupted, until a write shows it or a
    /// detail arrives (LLP 1127 D15).
    private var revealOwed = false
    /// What `changed` last reported.
    private var told: Showing?
    private struct Showing: Equatable {
        let collapsed: Bool, mode: UISplitViewController.DisplayMode, nested: Bool
    }
    /// UIKit's expansion or collapse, from the question it asks the delegate
    /// until it says it is done (`moving`).
    private var move: Move?
    private enum Move { case expanding, collapsing }
    /// Counts UIKit's questions, so that a question asked inside another's
    /// work owns the move from then on (`finishOwnTransitions`).
    private var questions = 0
    /// Whether a question's answer is being worked out. UIKit can report a
    /// move done from inside it, while D23's call ends a column's transition
    /// (an expansion's question heard `didExpand` from that call), before
    /// it has moved anything: the move goes on.
    private var answering = false

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

    /// Whether UIKit is moving N1 between the columns: from its question
    /// (which display mode to expand to, which column to put on top) until it
    /// says it expanded or collapsed, or a later turn finds the split at the
    /// target with no transition running (`settleCheckSoon`). Not until the split
    /// reads expanded: UIKit can lay the split out expanded inside its own
    /// question, still in its move. A move of N1 meanwhile is no Back.
    var moving: Bool { move != nil }
    /// Whether UIKit nests N1 in N0 now (collapsed, or collapsing).
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
    /// Whether `nav` is one of the columns'.
    func holds(_ nav: UINavigationController) -> Bool { nav === list || nav === detail }

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
    func show(_ stack: [RouteController], animated: Bool, relocated: Bool = false) -> Bool {
        // UIKit is moving N1 between the columns, and is their one writer until
        // it has expanded or collapsed: the host projects the router's visits
        // as they then stand (`settle`). Written now, the columns would be set
        // from the split's state before the move, and UIKit, finding them
        // changed, pushed the detail twice and asserted.
        if moving { return false }
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
        if collapsed, nested, rest.isEmpty {
            // Let UIKit pop nested N1 with its routes and zoom still in place
            reveal(.primary, animated: animated)
            return true
        }
        let next: [UIViewController] = rest.isEmpty ? [empty] : rest
        let writingDetail = !Self.same(detail.viewControllers, next)
        // A return to the root over the detail alone shows the list; owed until shown, since a question UIKit asks
        // inside the write below ends this pass, and the next finds no detail to return from
        if !collapsed, rest.isEmpty, hadDetail, controller.displayMode == .secondaryOnly { revealOwed = true }
        if !rest.isEmpty { revealOwed = false }
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
            guard write(next, detail, animated && movesWithinDetail), !moving else { return false }
        }
        if collapsed {
            // UIKit animates the push of N1 itself when it is outside N0
            if !rest.isEmpty, !nested { reveal(.secondary, animated: animated) }
        } else if moved {
            if !rest.isEmpty, [.oneOverSecondary, .twoOverSecondary].contains(controller.displayMode) { reveal(.secondary, animated: animated) }
        }
        if revealOwed, !collapsed, rest.isEmpty {
            revealOwed = false
            if controller.displayMode == .secondaryOnly { reveal(.primary, animated: animated) }
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
    /// UIKit may have ended without saying so is checked on a later turn.
    private func laidOut() {
        if move != nil { settleCheckSoon() }
        let now = Showing(collapsed: controller.isCollapsed, mode: controller.displayMode, nested: nested)
        guard now != told else { return }
        told = now
        DispatchQueue.main.async { [weak self] in self?.changed() }
    }

    /// Whether a later turn checks if UIKit ended the move (`settleCheckSoon`).
    private var settleCheckOwed = false

    /// On the main queue's next turn, outside UIKit's layout: a move whose
    /// split reads the target state, with neither the split nor a column in a
    /// transition, is over, as UIKit's own report would say (a split laid out
    /// wide from the start sends its delegate no expansion). A move waiting on
    /// a transition checks again when that transition ends: UIKit may neither
    /// report the move nor lay the split out again. N1's own push or pop asked
    /// for in the turn of an expansion has not begun, and UIKit leaves it, and
    /// the expansion with it, waiting for a layout N1 never gets in no window
    /// (the list across the window, the detail in none). Laid out inside the
    /// question it traps; laid out here, a turn later, it runs and the
    /// expansion ends (plain UIKit, 3 of 3).
    func settleCheckSoon() {
        guard !settleCheckOwed else { return }
        settleCheckOwed = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            settleCheckOwed = false
            guard let move else { return }
            if move == .expanding, let own = NavigationHost.pushOrPop(on: detail), own.viewController(forKey: .to)?.viewIfLoaded?.window == nil {
                detail.view.layoutIfNeeded()
            }
            let reached = move == .expanding ? !controller.isCollapsed : controller.isCollapsed
            let running = [controller.transitionCoordinator].compactMap { $0 }
                + [list, detail].compactMap { NavigationHost.pushOrPop(on: $0) }
            // A target UIKit abandoned (no transition runs, and the size class
            // now asks for the other state) is dropped (LLP 1127 D15)
            let sizeClass = controller.traitCollection.horizontalSizeClass
            let abandoned = move == .expanding ? sizeClass == .compact : sizeClass == .regular
            if running.isEmpty, reached || abandoned { settle(); return }
            for transition in running {
                transition.animate(alongsideTransition: nil) { [weak self] _ in self?.settleCheckSoon() }
            }
        }
    }

    /// UIKit expanded or collapsed. Settling writes nothing here, inside
    /// UIKit's adaptation: the host's next turn projects the router's visits
    /// as they now stand and publishes (`changed`, `NavigationHost.splitChanged`).
    private func settle() {
        move = nil
        DispatchQueue.main.async { [weak self] in self?.changed() }
    }

    /// At UIKit's question, D23's call (LLP 1127 D23): each column of
    /// `columns`, in order, whose own push or pop runs ends it at once,
    /// completed, as a plain navigation controller's ends at a fold; moving N1
    /// between the columns while one runs crashes UIKit ("Layout requested for
    /// visible navigation bar", RESULTS §3, §10). A coordinator a column only
    /// inherits is none of its own. At a collapse, one asked for in the same
    /// turn has not begun (its destination is not yet on screen) and would
    /// begin at UIKit's next layout with N1 nested, which crashes as well: it
    /// begins here, unanimated, so that the call ends it. That layout can run
    /// app delegates, which can end the transition and begin another: a
    /// replacement is tested afresh. At an expansion it is not laid out: the
    /// layout runs UIKit's scheduled expansion inside its own question, which
    /// then traps ("expected scheduled transition"), while the call alone
    /// leaves no crash (plain UIKit, a same-turn pop and size-class change,
    /// 2 of 2 each). A question asked meanwhile owns the move, and its own
    /// calls finish the columns.
    private func finishOwnTransitions(_ columns: [UINavigationController], beginUnbegun: Bool) {
        let question = questions
        for nav in columns {
            let begun = NSHashTable<AnyObject>.weakObjects()
            while beginUnbegun, questions == question, let transition = NavigationHost.pushOrPop(on: nav),
                  transition.viewController(forKey: .to)?.viewIfLoaded?.window == nil, !begun.contains(transition as AnyObject) {
                begun.add(transition as AnyObject)
                UIView.performWithoutAnimation { nav.view.layoutIfNeeded() }
            }
            guard questions == question else { return }
            guard NavigationHost.pushOrPop(on: nav) != nil else { continue }
            UIView.performWithoutAnimation { nav.setViewControllers(nav.viewControllers, animated: false) }
        }
    }

    // At each question the person's pops are read first, while UIKit still
    // tells them apart from its move (`willMove`); then the move is recorded
    // and D23 finishes the columns' own transitions, and the pops are read
    // again for a swipe D23 completed. Collapsing, N0's transition ends and
    // then N1's. Expanding, only N1's: N0's push and pop of N1 survived an
    // unfold left to UIKit (RESULTS §10), and ending N0's pop in the turn it
    // was asked trapped. Collapsed, the detail is on top while N1 holds a
    // route (LLP 1127 D4), wherever a presentation over the split was opened
    // from: UIKit can propose the list then (a size-class collapse), and the
    // list on top would leave N1 out of N0, read as the person's Back.
    func splitViewController(_ svc: UISplitViewController,
                             topColumnForCollapsingToProposedTopColumn proposedTopColumn: UISplitViewController.Column) -> UISplitViewController.Column {
        ask(.collapsing, finishing: [list, detail])
        return detailRoutes.isEmpty ? .primary : .secondary
    }
    func splitViewController(_ svc: UISplitViewController,
                             displayModeForExpandingToProposedDisplayMode proposedDisplayMode: UISplitViewController.DisplayMode) -> UISplitViewController.DisplayMode {
        ask(.expanding, finishing: [detail])
        return proposedDisplayMode
    }
    /// One question's work, before its answer: the pops read, the move
    /// recorded, the columns' own transitions finished (a not-yet-begun one
    /// begun first only at a collapse), the pops read again, and a check of the
    /// move on the next turn, for a move UIKit ends without laying the split out
    /// again.
    private func ask(_ target: Move, finishing columns: [UINavigationController]) {
        questions &+= 1
        answering = true
        defer { answering = false; settleCheckSoon() }
        willMove()
        move = target
        finishOwnTransitions(columns, beginUnbegun: target == .collapsing)
        willMove()
    }

    func splitViewControllerDidCollapse(_ svc: UISplitViewController) {
        if !answering { settle() }
    }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) {
        if !answering { settle() }
    }
    func splitViewController(_ svc: UISplitViewController, willChangeTo displayMode: UISplitViewController.DisplayMode) {
        DispatchQueue.main.async { [weak self] in self?.changed() }
    }
}
#endif
