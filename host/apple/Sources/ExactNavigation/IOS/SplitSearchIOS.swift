// @ref LLP 1127 D3–D6 — the committed destinations and the two UIKit columns.
#if os(iOS)
import ExactKit
import UIKit

final class SplitSearchState {
    enum Activation {
        case result, primary, incoming
    }

    final class Press {
        let kind: Activation
        var revision: UInt64
        init(_ kind: Activation, revision: UInt64) { (self.kind, self.revision) = (kind, revision) }
    }

    struct Reveal {
        let revision: UInt64
        let activation: Activation?
    }

    final class Pop {
        let navigation: UINavigationController
        let destination: RouteController
        let origin: String
        let source: String
        var landing: String { destination.key }
        let steps: Int
        let revision: UInt64
        var tracksCompletion = false
        var cancelled: Bool?
        init(in navigation: UINavigationController, origin: String, source: String, to destination: RouteController, steps: Int, revision: UInt64) {
            (self.navigation, self.destination) = (navigation, destination)
            (self.origin, self.source, self.steps, self.revision) = (origin, source, steps, revision)
        }
    }

    var primaryForeground = true
    // UIKit can nest the secondary during collapse even when the delegate chose primary.
    var collapseToPrimary = false
    private(set) var projectionEpoch: UInt64 = 0
    var ownerMoving = false
    var children: [UInt32]?
    var committed: [RouteController] = []
    var press: Press?
    var reveal: Reveal?
    var pop: Pop?
    var pending = false
    var draining = false
    var primaryExposed = false
    var detailExposed = false
    private(set) var detailWasNested = false
    private var concealment: UIView?
    var detailConcealed: Bool { concealment != nil }

    func beginOwnerMove() {
        projectionEpoch &+= 1
        ownerMoving = true
    }

    func noteContainment(nested: Bool) {
        detailWasNested = nested
        if !nested { collapseToPrimary = false }
    }

    func concealDetail(in view: UIView) {
        guard concealment == nil else { return } // The mask stays until this projection settles
        let cover = UIView(frame: view.bounds)
        cover.backgroundColor = .systemBackground
        cover.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        cover.accessibilityViewIsModal = true
        cover.layer.zPosition = 1
        view.addSubview(cover)
        concealment = cover
    }

    func showInstalledDetail() {
        concealment?.removeFromSuperview()
        concealment = nil
    }
}

extension NavigationHost {
    var splitPrimaryExposed: Bool {
        guard let split, split.searchOwner?.viewIfLoaded?.window != nil,
              modalNavigation == nil, !presenter.modalHost.inTransition else { return false }
        return split.controller.isCollapsed ? !split.nested : split.showsList
    }

    var splitSearchTransition: Bool {
        guard let split, let search = split.pairedSearch else { return false }
        return split.moving || split.writing || split.columnsAboutToMove || split.controller.transitionCoordinator != nil
            || split.list.transitionCoordinator != nil || split.detail.transitionCoordinator != nil
            || search.presenting || search.dismissing || search.controller.transitionCoordinator != nil
    }

    var committedSplitRoutes: [RouteController] {
        Array(selectedStack.prefix { !["modal", "fullscreen"].contains($0.node.props["navigationPresentation"] ?? "") })
    }

    var routerRevision: UInt64 {
        presenter.session?.routerRevision ?? 0
    }

    func mayCancelSearch(_ search: HeaderSearch) -> Bool {
        guard let split, !split.laysOutItself, !split.writing, !split.searchState.ownerMoving else { return false }
        return exposesSearchOwner(search)
    }

    func exposesSearchOwner(_ search: HeaderSearch) -> Bool {
        // The visible X belongs to search even when the detail owns Back.
        let committed = committedSplitRoutes
        guard let split, split.pairedSearch === search, split.searchOwner === committed.first,
              presenter.dialogs.top == nil, !presenter.menus.inTransition else { return false }
        return splitPrimaryExposed && selectedStack.last === committed.last
    }

    func searchInteracted(_ search: HeaderSearch) {
        guard let split, split.pairedSearch === search, splitPrimaryExposed,
              !split.searchState.ownerMoving, !split.moving, !split.writing,
              split.list.transitionCoordinator == nil, split.detail.transitionCoordinator == nil else { return }
        split.searchState.primaryForeground = true
    }

    func startPairedActivation(from node: NodeView) -> (() -> Void)? {
        guard let split, let owner = split.searchOwner, let search = split.pairedSearch else { return nil }
        let state = split.searchState
        guard splitPrimaryExposed, presenter.authored(node, inside: owner.node) else {
            if state.detailExposed && split.detail.viewIfLoaded.map({ node.isDescendant(of: $0) }) == true { state.primaryForeground = false }
            return nil
        }
        let result = search.results?.visible == true && search.results?.root.map { presenter.authored(node, inside: $0) } == true
        if result { searchInteracted(search) }
        return startPairedActivation(result ? .result : .primary, in: split)
    }

    func startPairedActivation(_ kind: SplitSearchState.Activation, in split: SplitIOS) -> () -> Void {
        let state = split.searchState
        let press = SplitSearchState.Press(kind, revision: routerRevision)
        let previous = state.press
        state.press = press
        return { [weak self, weak split] in
            guard let self, let split, self.split === split, split.searchState.press === press else { return }
            let committed = committedSplitRoutes
            recordPairedActivation(in: split, hasPresenter: selectedStack.last !== committed.last)
            split.searchState.press = previous
            if previous != nil { previous?.revision = press.revision }
            if split.searchState.reveal?.revision == press.revision, split.searchState.pending { scheduleSplitProjection() }
        }
    }

    func recordPairedActivation(in split: SplitIOS, hasPresenter: Bool) {
        guard let press = split.searchState.press else { return }
        let revision = routerRevision
        guard revision != 0, revision != press.revision else { return }
        press.revision = revision
        let state = split.searchState
        if hasPresenter {
            state.beginOwnerMove()
        } else {
            state.reveal = .init(revision: revision, activation: press.kind)
            scheduleSplitProjection()
        }
    }

    func notePairedCommit(in split: SplitIOS, stack: [RouteController], presented: Bool) {
        let state = split.searchState
        if state.committed.first !== stack.first || state.committed.dropFirst().first?.key != stack.dropFirst().first?.key {
            state.reveal = stack.count > 1 ? .init(revision: routerRevision, activation: nil) : nil
            if stack.count == 1 { state.primaryForeground = true }
        }
        state.committed = stack
        recordPairedActivation(in: split, hasPresenter: presented)
    }

    /// Preserve the native endpoints across a fold, including N0's pop of N1.
    func observeSplitPop(in split: SplitIOS, from nav: UINavigationController, to landing: UIViewController, departing: UIViewController? = nil) {
        let state = split.searchState
        if let pop = state.pop {
            trackSplitPop(pop, in: split)
            return
        }
        guard !split.moving, !split.writing, let destination = landing as? RouteController,
              let origin = selectedKey else { return }
        let transition = nav.transitionCoordinator
        let from = departing ?? transition?.viewController(forKey: .from)
        let source: RouteController
        let removedController: UIViewController
        if nav === split.list {
            guard split.controller.isCollapsed, !state.collapseToPrimary,
                  nav.viewControllers.first === landing, let top = split.detail.topViewController as? RouteController else { return }
            // The secondary can keep the route in its own stack after leaving the primary's stack.
            (source, removedController) = (top, split.detail)
        } else {
            // A router update can already name its next route while UIKit is still popping the projected one.
            guard let top = split.projectedDetailTop else { return }
            (source, removedController) = (top, top)
        }
        let namesEndpoints = (departing != nil || transition?.viewController(forKey: .to) === landing)
            && (from === removedController || from === source)
        let removed = nav.topViewController === landing && !nav.viewControllers.contains { $0 === removedController }
            && (nav !== split.list || state.detailWasNested)
        guard namesEndpoints || removed,
              source.key == NavigationRules.poppedTop(selected: origin, inspecting: inspectorKey, shownKeys: shownKeys),
              let first = shownKeys.firstIndex(of: destination.key), let last = shownKeys.firstIndex(of: source.key), first < last else { return }
        let pop = SplitSearchState.Pop(in: nav, origin: origin, source: source.key, to: destination, steps: last - first, revision: routerRevision)
        state.pop = pop
        trackSplitPop(pop, in: split)
        scheduleSplitProjection()
    }

    /// The animator is chosen before UIKit publishes its coordinator.
    private func trackSplitPop(_ pop: SplitSearchState.Pop, in split: SplitIOS) {
        guard !pop.tracksCompletion, let transition = pop.navigation.transitionCoordinator,
              transition.viewController(forKey: .to) === pop.destination else { return }
        pop.tracksCompletion = true
        if transition.isInteractive {
            transition.notifyWhenInteractionChanges { [weak pop] context in pop?.cancelled = context.isCancelled }
        }
        if pop.navigation === split.detail, inspectorKey == pop.origin { split.closeInspector(along: transition) }
        transition.animate(alongsideTransition: nil) { [weak self, weak split, weak pop] context in
            guard let self, let split, let pop, self.split === split, split.searchState.pop === pop else { return }
            pop.cancelled = context.isCancelled
            scheduleSplitProjection()
        }
    }

    func followSplitPop(_ pop: SplitSearchState.Pop, in split: SplitIOS, cancelled: Bool) {
        let state = split.searchState
        if cancelled { lastTransition = "cancelled"; return }
        // `replace` preserves visit identity even if it recreates the route's node. `open` mints a new visit.
        guard selectedKey == pop.origin, shownKeys.contains(pop.landing),
              NavigationRules.poppedTop(selected: pop.origin, inspecting: inspectorKey, shownKeys: shownKeys) == pop.source,
              (state.reveal?.revision ?? 0) <= pop.revision else { return }
        let clearing = pop.landing == shownKeys.first
        lastTransition = "completed"
        if pop.steps == 1, pop.origin == pop.source { goBack() }
        else { goBack(from: pop.origin, to: pop.landing) }
        if clearing {
            state.primaryForeground = selectedKey == pop.landing
            if !state.primaryForeground { state.reveal = .init(revision: routerRevision, activation: nil) }
        }
    }

    /// One wake source rechecks every hold, including a reversal without a delegate
    /// completion and a search dismissal that doesn't send didDismiss.
    func scheduleSplitProjection() {
        guard let split, split.paired else { return }
        let state = split.searchState
        state.pending = true
        if !FrameClock.shared.wants(state) {
            FrameClock.shared.want(state, .navigationReveal, rate: FrameClock.full(on: presenter.viewport.window?.screen)) { [weak self, weak split, weak state] _ in
                guard let self, let split, let state, self.split === split, split.searchState === state else { return }
                drainSplitProjection(split)
            }
            DispatchQueue.main.async { [weak self, weak split] in
                guard let self, let split, self.split === split, split.searchState === state, state.pending else { return }
                drainSplitProjection(split)
            }
        }
    }

    private func drainSplitProjection(_ split: SplitIOS) {
        let state = split.searchState
        guard state.pending, !state.draining, !syncing, !presenter.applying, !Owner.shared.isServingMain,
              presenter.session?.hasStagedBatches != true else {
            return // The display link retries a pending projection
        }
        if split.controller.viewIfLoaded?.window == nil {
            state.pending = false
            FrameClock.shared.drop(state) // A later mount resyncs the committed tree
            return
        }
        state.draining = true
        defer { state.draining = false }
        if let pop = state.pop { trackSplitPop(pop, in: split) }
        split.reconcileMoveAfterLayout()
        guard !split.moving, !split.writing else { return }
        split.reconcileInspectorClose()
        split.pairedSearch?.reconcilePresentation()
        split.pairedSearch?.settlePairedClose()
        guard !splitSearchTransition, !presenter.modalHost.inTransition else { return }
        state.pending = false
        if let pop = state.pop {
            state.pop = nil
            followSplitPop(pop, in: split, cancelled: pop.cancelled ?? (pop.navigation.topViewController !== pop.destination))
        }
        resync()
        guard self.split === split, split.searchState === state else { return }
        guard !pendingSync, !state.pending, !splitSearchTransition else {
            state.pending = true
            return
        }
        state.showInstalledDetail()
        FrameClock.shared.drop(state)
        publishSplitSearch()
        coversChanged()
        presenter.flushPendingFocus()
    }

    func publishSplitSearch() {
        guard let split, split.paired, !splitSearchTransition, !presenter.modalHost.inTransition else { return }
        let state = split.searchState
        state.ownerMoving = false
        state.primaryExposed = splitPrimaryExposed
        state.detailExposed = modalNavigation == nil && (split.controller.isCollapsed ? split.nested : !split.overlaysDetail)
        state.noteContainment(nested: split.nested)
        // UIKit can hide the primary while the inspector is shown. Keep the foreground choice underneath it.
        if split.inspecting == nil {
            if state.primaryExposed != state.detailExposed {
                state.primaryForeground = state.primaryExposed
            } else if state.detailExposed, state.committed.count > 1, split.pairedSearch?.controller.isActive == false {
                state.primaryForeground = false
            }
        }
        split.pairedSearch?.results?.publishVisibility()
        presenter.owners.publish()
    }
}
#endif
