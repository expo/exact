// @ref LLP 1127.000 D1–D3 — project the list's authored results into UIKit's search viewport.
#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

#if os(iOS)
final class HeaderSearchResults: UIViewController {
    weak var host: NavigationHost?
    weak var owner: RouteController?
    private(set) weak var root: NodeView?
    private(set) var visible = false
    private(set) var covered: [NodeView] = []
    private let mount = PresentationMount()
    private let clearance = SearchClearance()
    private var focusedBeforeChange = false
    private weak var movingAccessibilityFocus: UIView?

    init(host: NavigationHost, owner: RouteController) {
        self.host = host
        self.owner = owner
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemGroupedBackground
        view.addSubview(mount)
        if let presenter = host?.presenter { mount.relay = MountRelay(presenter, mount: mount) }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        visibilityChanged()
        host?.presenter.owners.publish()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        visibilityChanged()
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        visibilityChanged()
    }

    var exposed: Bool {
        guard let owner, let search = owner.search, let host,
              authoredVisible, let view = viewIfLoaded,
              view.window != nil, !view.bounds.isEmpty else { return false }
        if let split = host.split, parent === split.detail {
            // N1's whenever the split is expanded, the list beside it, over it or hidden (LLP 1127.000 D2)
            guard search.open, split.detail.topViewController === self, split.resultsInDetail, host.modalNavigation == nil,
                  !host.presenter.modalHost.inTransition, host.presenter.dialogs.top == nil else { return false }
        } else {
            // Over the list, they show where the list does
            guard host.splitPrimaryExposed, !search.placesResultsInDetail || host.presenter.dialogs.top == nil,
                  search.controller.isActive, search.controller.showsSearchResultsController else { return false }
        }
        return sequence(first: view, next: \.superview).allSatisfy { !$0.isHidden && $0.alpha > 0.01 }
    }

    private func visibilityChanged() {
        if visible != exposed { host?.scheduleSplitProjection() }
    }

    var coversList: Bool { visible && parent !== host?.split?.detail }

    var block: (root: NodeView, view: UIView, block: CGRect, shown: Bool)? {
        guard let root, let owner else { return nil }
        return (root, mount, mount.bounds.isEmpty ? owner.surface.bounds : mount.bounds, visible)
    }

    private var containsFocus: Bool {
        guard let root, let presenter = host?.presenter else { return false }
        return [presenter.focusedNode, presenter.keyboardEditor].compactMap { $0 }.contains {
            $0 === root || $0.isDescendant(of: root)
        }
    }

    /// A remove or hide can resign the result's responder before the binding is updated.
    func prepare(_ batch: Batch) {
        guard visible, let root, let owner,
              batch.ops.contains(where: { op in
                  (op.id == root.id && (op.op == .style || op.op == .destroy))
                      || (op.id == owner.search?.field?.id && op.op == .props)
              }) else { return }
        focusedBeforeChange = containsFocus
    }

    /// The results root is physically mounted here and keeps its authored parent for input and inheritance.
    func bind(_ next: NodeView?) {
        guard root !== next, let owner, let host else { return }
        let previous = root
        let restoreFocus = focusedBeforeChange || containsFocus
        focusedBeforeChange = false
        clearance.clear()
        root = next
        if let previous {
            if host.presenter.views[previous.id] === previous,
               let index = host.searchChildIDs(of: owner.node).firstIndex(of: previous.id) {
                let parent = owner.node.container
                let following = host.searchChildIDs(of: owner.node).dropFirst(index + 1).compactMap { host.presenter.views[$0] }.first { $0.superview === parent }
                if let following { parent.insertSubview(previous, belowSubview: following) }
                else { parent.addSubview(previous) }
                host.presenter.regrid(previous)
            } else {
                previous.removeFromSuperview()
            }
        }
        if let next {
            loadViewIfNeeded()
            mount.addSubview(next)
            host.presenter.regrid(next)
            refit()
        }
        if restoreFocus { owner.search?.returnAccessibilityFocus() }
    }

    func update(_ search: HeaderSearch) {
        let shown = search.controller.isActive && authoredVisible &&
            (!search.placesResultsInDetail || isInSearchContainer(search))
        if search.controller.showsSearchResultsController != shown { search.controller.showsSearchResultsController = shown }
        if search.placesResultsInDetail, let split = host?.split, split.pairedSearch === search, split.needsResultsProjection {
            host?.scheduleSplitProjection()
        }
        visibilityChanged()
    }

    var authoredVisible: Bool {
        root.map { $0.style["display"]?.string != "none" && $0.style["visibility"]?.string != "hidden" } == true
    }

    func isInSearchContainer(_ search: HeaderSearch) -> Bool {
        guard let container = search.resultsContainer else { return true }
        return parent === container
    }

    var isDetached: Bool { parent == nil && navigationController == nil && viewIfLoaded?.superview == nil }

    /// Takes the view out of a stack's wrapper that no longer holds the controller: UIKit leaves it there when the
    /// stack drops the results off screen, and with no parent left nothing else removes it
    func releaseOrphanedView() {
        guard parent == nil, navigationController == nil, transitionCoordinator == nil,
              let view = viewIfLoaded, view.superview != nil, view.window == nil else { return }
        view.removeFromSuperview()
    }

    func rememberAccessibilityFocus() {
        guard UIAccessibility.isVoiceOverRunning, let view = viewIfLoaded,
              let focused = UIAccessibility.focusedElement(using: .notificationVoiceOver) as? UIView,
              focused.isDescendant(of: view) else { return }
        movingAccessibilityFocus = focused
    }

    func moveIntoSearchContainer(_ search: HeaderSearch) -> Bool {
        guard let container = search.resultsContainer, isDetached, container.transitionCoordinator == nil,
              !container.children.contains(where: { $0 === self }) else { return false }
        container.addChild(self)
        view.frame = container.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        container.view.addSubview(view)
        didMove(toParent: container)
        return true
    }

    func leaveSearchContainer(_ search: HeaderSearch) -> Bool {
        guard let container = search.resultsContainer, parent === container else { return true }
        guard transitionCoordinator == nil else { return false }
        rememberAccessibilityFocus()
        willMove(toParent: nil)
        view.removeFromSuperview()
        removeFromParent()
        return true
    }

    func prepareDetailBar() {
        if navigationItem.title == nil {
            navigationItem.title = NSLocalizedString("Search Results", comment: "Back landing for a result opened from split search")
            navigationItem.largeTitleDisplayMode = .never
        }
    }

    /// Publish coverage together with the placed block once UIKit settles.
    func publishVisibility() {
        guard let host, let owner else { return }
        let wasVisible = visible
        visible = exposed
        let inDetail = parent === host.split?.detail
        if !visible {
            covered = root.map { [$0] } ?? []
        } else if !inDetail {
            let header = HeaderShape.bar(of: owner.node)?.header
            covered = host.searchBody(of: owner.node).filter { $0 !== root && $0 !== header }
        } else {
            covered = []
        }
        // The router can still select the detail visit while the results occupy its column.
        if let split = host.split, split.detail.topViewController === self, let retained = split.projectedDetailTop {
            covered.append(retained.node)
        }
        if owner.search?.placesResultsInDetail == true {
            viewIfLoaded?.accessibilityElementsHidden = !visible
            viewIfLoaded?.isUserInteractionEnabled = visible
        }
        if let focused = movingAccessibilityFocus {
            movingAccessibilityFocus = nil
            if visible, focused.window != nil, focused.isDescendant(of: view),
               UIAccessibility.focusedElement(using: .notificationVoiceOver) as? UIView !== focused {
                UIAccessibility.post(notification: .layoutChanged, argument: focused)
            } else if !visible {
                owner.search?.returnAccessibilityFocus()
            }
        }
        if wasVisible && !visible, focusedBeforeChange || containsFocus {
            owner.search?.returnAccessibilityFocus()
        }
        focusedBeforeChange = false
    }

    func refit() {
        // A hidden results viewport keeps the allocation that owns its scroll position.
        guard let host, let root, let view = viewIfLoaded, exposed else { return }
        // With the list hidden the search's bar and keys take nothing from the results (`searchOccupancy`), which
        // still fit the detail as it is now
        let occupancy = host.searchOccupancy(in: view) ?? SearchClearance.Occupancy()
        let policy = host.presenter.keyboardPolicy
        // The results run under the bottom band (the search bar, the keys, the home indicator), which their scroller
        // takes as its inset (`SearchClearance.fit`), as a UIKit results controller's list runs under the bar
        var insets = view.safeAreaInsets
        insets.bottom = 0
        var safe = view.bounds.inset(by: insets)
        if let split = host.split, parent === split.detail, split.overlaysDetail, let sidebar = split.list.viewIfLoaded {
            // A sidebar UIKit shows over the detail covers part of its view, which UIKit's safe area leaves out; a hidden
            // one's view keeps its frame and covers nothing.
            let overlap = safe.intersection(view.convert(sidebar.bounds, from: sidebar))
            if !overlap.isEmpty {
                if overlap.minX <= safe.minX { safe = CGRect(x: overlap.maxX, y: safe.minY, width: safe.maxX - overlap.maxX, height: safe.height) }
                else if overlap.maxX >= safe.maxX { safe.size.width = overlap.minX - safe.minX }
            }
        }
        // The keys change the results' scroller's inset, not their frame, as a UIKit search results controller's
        // view keeps its frame while its list insets for the keyboard: hiding the keys only uncovers rows the list
        // already laid out (`SearchClearance.fit`). Results with no scroller to inset end above the keys.
        var frame = safe
        frame.size.height -= clearance.fit(root, in: view, allocation: frame, occupancy: occupancy, policy: policy, enabled: true)
        if mount.frame != frame { mount.frame = frame }
    }
}

extension NavigationHost {
    var pairedSearch: HeaderSearch? {
        split?.pairedSearch
    }

    func searchChildIDs(of owner: NodeView) -> [UInt32] {
        if let split, split.searchOwner?.node === owner, let children = split.searchState.children { return children }
        return presenter.flats.children(of: owner.id) ?? mountedChildIDs(of: owner)
    }

    func searchBody(of owner: NodeView) -> [NodeView] {
        searchChildIDs(of: owner).compactMap { presenter.views[$0] }
    }

    /// Find the authored results child before projecting the header or asking for the split rule.
    func candidateSearchResult(in owner: RouteController) -> NodeView? {
        let field = HeaderShape.bar(of: owner.node)?.search
        return searchResult(named: field?.props["accessibilityControls"], in: owner.node)
    }

    func searchResult(named controls: String?, in owner: NodeView) -> NodeView? {
        let ids = (controls ?? "").split { $0.isWhitespace }
        guard ids.count == 1, let name = ids.first else { return nil }
        let matches = presenter.carrying("id").filter {
            $0.props["id"] == String(name) && presenter.logicalAncestry(of: $0).contains { $0 === owner }
        }
        guard matches.count == 1, let root = matches.first,
              searchBody(of: owner).contains(where: { $0 === root }),
              ["view", "scroll", "list"].contains(root.kind), root.props["semanticTag"] != "header",
              root.props["navigationKey"] == nil, root.props["popover"] == nil,
              root.props["navigationSplit"] != "placeholder",
              !["dialog", "alertdialog"].contains(root.props["accessibilityRole"] ?? "") else { return nil }
        return root
    }

    func searchOwns(_ node: NodeView, under parent: NodeView) -> Bool {
        guard let results = pairedSearch?.results else { return false }
        return results.root === node && results.owner?.node === parent
    }
}

extension RouteController {
    func detachSearch() {
        guard let search else { return }
        search.field = nil
        search.results?.bind(nil)
        search.close()
        navigationItem.searchController = nil
        self.search = nil
    }
}
#endif

extension NavigationHost {
    func mountedChildIDs(of node: NodeView) -> [UInt32] {
        node.container.subviews.compactMap { ($0 as? NodeView)?.id }
    }

    package func barSearchOpen(around view: UIView) -> Bool {
        guard let search = ViewPresentation.controllers(for: view).lazy.compactMap({ ($0 as? RouteController)?.search }).first else { return false }
        // A paired search leaves the fallback's grouped padding to UIKit.
        return !search.paired && (search.presenting || search.controller.isActive)
    }

    var searchInUseRoute: RouteController? {
        controllers.values.first { c in c.search.map { $0.presenting || $0.editor.isFirstResponder } == true }
    }

    func searchCovered(by stack: [RouteController], pressed: Bool) -> HeaderSearch? {
        guard let route = searchInUseRoute, let search = route.search, let index = stack.firstIndex(of: route) else { return nil }
        if search.paired { return nil }
        if let split, split.settledExpanded {
            return pressed && !stack.elementsEqual(split.shown, by: ===) ? search : nil
        }
        return index < stack.count - 1 ? search : nil
    }

    func settleSearches() {
        for search in controllers.values.compactMap(\.search) { search.settle() }
    }
}
#endif
