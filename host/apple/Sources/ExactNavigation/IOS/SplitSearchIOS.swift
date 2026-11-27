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
        let visits: [Visit]
        init(_ kind: Activation, revision: UInt64, visits: [Visit]) {
            (self.kind, self.revision, self.visits) = (kind, revision, visits)
        }
    }

    struct Visit: Equatable {
        let key: String
        let location: String?
    }

    struct Reveal {
        let revision: UInt64
        let activation: Activation?
    }

    final class Pop {
        let navigation: UINavigationController
        let destination: UIViewController
        let origin: String
        let source: String
        let landing: String
        let returnsToResults: Bool
        let steps: Int
        let revision: UInt64
        var tracksCompletion = false
        var cancelled: Bool?
        init(in navigation: UINavigationController, origin: String, source: String, to destination: UIViewController,
             landing: String, returnsToResults: Bool, steps: Int, revision: UInt64) {
            (self.navigation, self.destination) = (navigation, destination)
            (self.landing, self.returnsToResults) = (landing, returnsToResults)
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
    var resultBase: String?
    var retainedResultBase: String?
    var followingResultsBack = false
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
        split?.searchInTransition == true
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

    func searchInteracted(_ search: HeaderSearch, editing: Bool = false) {
        guard let split, split.pairedSearch === search, splitPrimaryExposed,
              !split.searchState.ownerMoving, !split.moving, !split.writing,
              split.list.transitionCoordinator == nil, split.detail.transitionCoordinator == nil else { return }
        split.searchState.primaryForeground = true
        let state = split.searchState
        if editing, search.placesResultsInDetail, let base = state.resultBase {
            state.retainedResultBase = base
            state.resultBase = nil
            scheduleSplitProjection()
        }
    }

    func startPairedActivation(from node: NodeView) -> (() -> Void)? {
        guard let split, let owner = split.searchOwner, let search = split.pairedSearch else { return nil }
        let state = split.searchState
        let result = search.results?.visible == true && search.results?.root.map { presenter.authored(node, inside: $0) } == true
        // A result the detail column shows is the search's wherever UIKit shows the list (LLP 1127.000 D2)
        let resultInDetail = result && search.results?.parent === split.detail
        guard splitPrimaryExposed || resultInDetail, presenter.authored(node, inside: owner.node) else {
            if state.detailExposed && split.detail.viewIfLoaded.map({ node.isDescendant(of: $0) }) == true { state.primaryForeground = false }
            return nil
        }
        if result { searchInteracted(search) }
        return startPairedActivation(result ? .result : .primary, in: split)
    }

    func startPairedActivation(_ kind: SplitSearchState.Activation, in split: SplitIOS) -> () -> Void {
        let state = split.searchState
        let visits = kind == .result && split.pairedSearch?.placesResultsInDetail == true ? splitVisits(committedSplitRoutes) : []
        let press = SplitSearchState.Press(kind, revision: routerRevision, visits: visits)
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

    private func splitVisits(_ routes: [RouteController]) -> [SplitSearchState.Visit] {
        routes.map { .init(key: $0.key, location: presenter.session?.runtime.visitLocation($0.key)) }
    }

    func recordPairedActivation(in split: SplitIOS, hasPresenter: Bool) {
        guard let press = split.searchState.press else { return }
        let revision = routerRevision
        guard revision != 0, revision != press.revision else { return }
        press.revision = revision
        let state = split.searchState
        if hasPresenter {
            split.pairedSearch?.prepareCoverMove()
            state.beginOwnerMove()
        } else {
            if press.kind == .incoming {
                state.resultBase = nil
                state.retainedResultBase = nil
            }
            if !press.visits.isEmpty {
                let after = splitVisits(committedSplitRoutes)
                let changed = zip(press.visits, after).prefix { $0 == $1 }.count
                if after.count > 1 {
                    let projectedBase = split.detailRoutes.first?.key
                    state.resultBase = changed > 0 && changed < after.count
                        ? after[changed].key
                        : (state.resultBase ?? state.retainedResultBase).flatMap { key in after.contains { $0.key == key } ? key : nil }
                            ?? after.dropFirst().first { $0.key == projectedBase }?.key ?? after[1].key
                    state.retainedResultBase = nil
                }
            }
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
        guard !split.moving, !split.writing, let origin = selectedKey else { return }
        let toResults = landing === split.pairedSearch?.results
            || (nav === split.list && landing === split.searchOwner && state.resultBase != nil)
        let landingKey: String
        if toResults, let base = state.resultBase, let index = shownKeys.firstIndex(of: base), index > 0 {
            landingKey = shownKeys[index - 1]
        } else if let route = landing as? RouteController {
            landingKey = route.key
        } else { return }
        let transition = nav.transitionCoordinator
        let from = departing ?? transition?.viewController(forKey: .from)
        let source: RouteController
        let removedController: UIViewController
        if nav === split.list {
            // A Back to the results is the result's; a Back that only shows the list keeps the detail (LLP 1127 D12)
            guard split.controller.isCollapsed, !state.collapseToPrimary, toResults || !popOnlyShowsList(split),
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
              source.key == NavigationRules.poppedTop(selected: origin, inspecting: poppingInspectorKey, shownKeys: shownKeys),
              let first = shownKeys.firstIndex(of: landingKey), let last = shownKeys.firstIndex(of: source.key), first < last else { return }
        let pop = SplitSearchState.Pop(in: nav, origin: origin, source: source.key, to: landing,
                                       landing: landingKey, returnsToResults: toResults, steps: last - first, revision: routerRevision)
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
              NavigationRules.poppedTop(selected: pop.origin, inspecting: poppingInspectorKey, shownKeys: shownKeys) == pop.source,
              (state.reveal?.revision ?? 0) <= pop.revision else { return }
        let clearing = pop.landing == shownKeys.first
        let location = pop.returnsToResults ? presenter.session?.runtime.visitLocation(pop.landing) : nil
        lastTransition = "completed"
        state.followingResultsBack = pop.returnsToResults
        if pop.steps == 1, pop.origin == pop.source { goBack() }
        else { goBack(from: pop.origin, to: pop.landing) }
        state.followingResultsBack = false
        if pop.returnsToResults {
            let returned = selectedKey == pop.landing && presenter.session?.runtime.visitLocation(pop.landing) == location
            if returned {
                state.resultBase = nil
                state.reveal = nil
                state.primaryForeground = true
                state.collapseToPrimary = split.controller.isCollapsed
            } else {
                if routerRevision != pop.revision { state.resultBase = nil }
                state.primaryForeground = false
                state.reveal = .init(revision: routerRevision, activation: nil)
            }
        } else if clearing {
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
        if state.ownerMoving, let search = split.pairedSearch, exposesSearchOwner(search) {
            state.ownerMoving = false
        }
        split.pairedSearch?.restoreActivationAfterPosture()
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
        // Moving the results and laying out the bar can still reset the editor's range during resync.
        if !state.pending { split.pairedSearch?.restoreEditingAfterPosture() }
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
            if state.primaryExposed != state.detailExposed,
               !(split.pairedSearch?.placesResultsInDetail == true && split.pairedSearch?.open == true && !state.primaryExposed) {
                state.primaryForeground = state.primaryExposed
            } else if state.detailExposed, state.committed.count > 1,
                      split.pairedSearch?.controller.isActive == false, split.pairedSearch?.open == false {
                state.primaryForeground = false
            }
        }
        split.pairedSearch?.results?.publishVisibility()
        presenter.owners.publish()
    }
}
#endif
