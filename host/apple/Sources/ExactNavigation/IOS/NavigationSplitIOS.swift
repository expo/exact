// @ref LLP 1127 D2, D5 — the navigation host's side of a split
// (SplitIOS.swift): when a stack splits, installing and writing its columns,
// following UIKit's changes to them, and which of its routes are live.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// A split rule the owner read (`SplitRead`), decoded.
struct HeardSplit {
    let read: SplitRead
    let rule: StackSplit?
    init(_ read: SplitRead, rule: StackSplit?? = nil) {
        self.read = read
        self.rule = rule ?? StackSplit(json: read.json)
    }
}

extension NavigationHost {
    /// The runner's rule for the stack whose first route is `root`
    /// (`Runtime.stackSplit`), where the device admits its split (`admitted`): nil for a
    /// stack that pushes, under the agent's own chrome too; `.none` while the runner is busy and cannot answer. It is
    /// the answer the last applied batch that could change it carried
    /// (`Batch.split`), so main does not wait on the owner for it, as at a
    /// transition's end while a reply's pump builds a page (LLP 1072 T2). A
    /// stack no batch has answered for yet (one being installed, a new root
    /// route, another list retained) is asked of the runner, and the batches
    /// after answer for it.
    func splitRule(_ root: RouteController?) -> StackSplit?? {
        // tvOS takes the split once a hand-built one is measured with the focus engine (LLP 1127 §4).
        #if os(tvOS)
        return .some(nil)
        #else
        guard !ExactEnv.authoredChrome, let root, let session = presenter.session else { return .some(nil) }
        // The tab or window can have a custom primary. An explicit split
        // still uses the runner's row associations when it finds a list.
        let requested = tabWantsSplit(root) ? StackSplit(asked: true, list: nil, width: nil, rows: [:]) : nil
        let retained = split?.list.stackRoutes.first === root ? split?.listNode ?? 0 : 0
        let results = candidateSearchResult(in: root)?.id ?? 0
        if let heard = heardSplit, heard.read.view == root.node.id, heard.read.retained == retained, heard.read.results == results { return .some(Self.admitted(heard.rule) ?? requested) }
        guard let read = session.runtime.on(busy: nil, { session.runtime.readSplit(root.node.id, retained: retained, results: results, sequence: session.runtime.sequence.newest) }) else { return nil }
        heardSplit = HeardSplit(read)
        return .some(Self.admitted(heardSplit?.rule) ?? requested)
        #endif
    }

    /// The runner's rule where the device admits its split, for a stack outside a tab: an iPad's splits where the
    /// root route asks or the rule finds a list whose rows push (LLP 1127 D19); any other device's only where the
    /// route asks, since UIKit builds no split an app did not make (D1, D13).
    static func admitted(_ rule: StackSplit?, idiom: UIUserInterfaceIdiom = UIDevice.current.userInterfaceIdiom) -> StackSplit? {
        rule?.asked == true || idiom == .pad ? rule : nil
    }

    #if os(iOS)
    /// A batch's split rule (`Batch.split`), as it prepares: the newest the
    /// runtime made is the one main keeps.
    func hearSplit(_ batch: Batch) {
        guard let read = batch.split, read.sequence > heardSplit?.read.sequence ?? 0 else { return }
        heardSplit = heardSplit?.read.json == read.json ? heardSplit.map { HeardSplit(read, rule: $0.rule) } : HeardSplit(read)
    }
    #endif

    /// Whether the stack whose first route is `root` splits; nil while the runner is busy.
    func columnLayoutHeld(_ nav: UINavigationController) -> Bool {
        if let split, split.holds(nav), split.laysOutItself { return true }
        return tabSplits.values.contains { $0.holds(nav) && $0.laysOutItself }
    }

    func splits(_ root: RouteController?) -> Bool? { splitRule(root).map { $0 != nil } }

    /// The split's two columns, each a navigation controller of its own
    /// (`makeNavigation`, so the `navigation` hatch receives each), as the
    /// primary owner, holding the first routes before its view enters the
    /// window, where UIKit collapses or expands it.
    func makeSplit(first: [RouteController], rule: StackSplit) -> SplitIOS {
        let list = makeNavigation(first: first.first?.node)
        let detail = makeNavigation(first: nil)
        let split = SplitIOS(list: list, detail: detail) { [weak self] routes, nav, animated in
            self?.writeStack(routes, in: nav, animated: animated, visible: true) ?? false
        }
        split.listNode = rule.list
        split.rule = rule
        split.changed = { [weak self, weak split] in
            guard let self, let split else { return }
            if self.split === split { splitChanged() } else { runHeldSync() }
        }
        split.inspectorClosed = { [weak self, weak split] in
            guard let self, let split, self.split === split,
                  split.inspecting?.key == selectedKey else { return }
            goBack()
        }
        split.columnsMove = { [weak self, weak split] in
            guard let self, let split, self.split === split else { return }
            coversChanged()
            presenter.owners.publish()
        }
        split.columnsWillChange = { [weak self] in self?.focusAcrossColumns = self?.presenter.focusedNode }
        split.columnsDidChange = { [weak self] in
            guard let self, let node = focusAcrossColumns else { return }
            focusAcrossColumns = nil
            guard presenter.views[node.id] === node, presenter.focusedNode !== node,
                  !isInactiveRoute(containing: node) else { return }
            presenter.focusNode(node)
        }
        #if os(iOS)
        if let root = first.first { split.setSearchOwner(root, children: searchChildIDs(of: root.node)) }
        split.stageForExpansion = { [weak self, weak split] mode in
            guard let self, let split, self.split === split else { return }
            split.stageExpansion(committedSplitRoutes, mode: mode, awaitingCommit: split.searchState.pop != nil)
        }
        #endif
        split.listWillBeCovered = { [weak split] in
            (split?.list.stackRoutes.first as? RouteController)?.search?.resign()
        }
        split.willPush = { [weak self] route, beneath in self?.prepareRoutePush(route, beneath: beneath) }
        split.baseChanged = { [weak self, weak split] base in
            guard let self, let split else { return }
            self.prepareColumnTransition(base, in: split)
        }
        return split
    }

    func startSplit(_ split: SplitIOS, first: [RouteController]) {
        let list = split.list, detail = split.detail
        prepareRoutes(Array(first.prefix(1)), in: list)
        prepareRoutes(Array(first.dropFirst()), in: detail)
        // The detail's column keeps its bar, the empty detail's too (LLP 1127 D7)
        detail.setNavigationBarHidden(!barShows(detail), animated: false)
        split.show(first, animated: false)
        for nav in [list, detail] { watchPops(nav) }
    }

    func installSplit(_ p: Projection, first: [RouteController], rule: StackSplit, in parent: UIViewController) {
        let split = makeSplit(first: first, rule: rule)
        self.split = split
        primaryNavigation = split.list
        startSplit(split, first: first)
        parent.addChild(split.controller)
        let mount = ownerMount(p.root)
        split.controller.view.frame = ownerFrame(split.controller, in: mount)
        split.controller.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        mount.addSubview(split.controller.view)
        split.controller.view.setPaintForeground(aboveAuthored: false)
        fitOwner()
        split.controller.didMove(toParent: parent)
        presenter.session?.log("navigation: the stack is a split, its list in one column and its detail in the other (LLP 1127)")
    }

    /// The stack in the split's columns, its routes projected into each,
    /// while UIKit moves neither them nor the columns (`changing`). Returns
    /// whether a lifted header or a column move holds the projection.
    func syncSplit(_ split: SplitIOS, _ wanted: [RouteController], animated: Bool, reveal: Bool? = nil) -> Bool {
        let stack = wanted
        prepareRoutes(Array(stack.prefix(1)), in: split.list)
        prepareRoutes(Array(stack.dropFirst()), in: split.detail)
        let top = split.detailRoutes.last
        let changingRoutes: Bool
        #if os(iOS)
        if split.pairedSearch?.placesResultsInDetail == true {
            changingRoutes = !split.matchesResultsProjection(stack) || split.searchOwner !== stack.first
        } else {
            changingRoutes = !split.held.elementsEqual(stack, by: ===)
        }
        #else
        changingRoutes = !split.held.elementsEqual(stack, by: ===)
        #endif
        // UIKit lays an arriving route out as it is given the stack: a lifted header must have left the layout by then
        if changingRoutes, liftHeld { return true }
        let location = stack.count > 1 ? stack.last.flatMap { split.rule?.locations[$0.node.id] } : nil
        let relocated = location != nil && split.topLocation != nil && location != split.topLocation && stack.last === split.detailRoutes.last
        // A row's press whose navigation the router took owes the detail a reveal, the same visit too: the row of the
        // detail kept behind the folded list shows it again (LLP 1127 D14)
        let revision = routerRevision
        let owed = reveal ?? (revision != split.syncedRevision && !split.presses.isEmpty && stack.count > 1 ? true : nil)
        guard split.show(stack, animated: animated, relocated: relocated, revealsDetail: owed) else {
            #if os(iOS)
            if split.pairedSearch?.placesResultsInDetail == true {
                scheduleSplitProjection()
                return true
            }
            #endif
            return split.moving
        }
        split.topLocation = location
        split.syncedRevision = revision
        if changingRoutes { lastTransition = "idle" }
        // A press that opened a detail may hand its control's focus to the arriving route's first autofocus (PressHandoffIOS)
        if let arrived = split.detailRoutes.last, arrived !== top { presenter.pressShows(arrived.node) }
        // A sidebar's selected row follows the detail's base, a `replace` of its item included (LLP 1127 D3, D7)
        split.pressedRows = split.pressedRows.filter { id, _ in stack.contains { $0.node.id == id } }
        let row = detailRow(split, in: split.rule, base: stack.dropFirst().first)
        if !split.controller.isCollapsed, let list = split.listNode, row != split.sidebarRow {
            split.sidebarRow = row
            presenter.groupedLists?.sync(changed: [list])
        }
        // Slices published before a press's batch land ahead of it: only the presses' own batches end them
        if !split.presses.isEmpty, split.presses.allSatisfy(\.landed) { endPress(split, poll: false) }
        if !split.laysOutItself { ApplyProfile.time("nav.layout") { split.controller.view.layoutIfNeeded() } }
        return false
    }

    /// An inactive tab keeps its columns and its row. Read its rule without replacing the active tab's subscription.
    func syncInactiveSplit(_ columns: SplitIOS, _ routes: [RouteController], sequence: UInt64) {
        let changed = !columns.held.elementsEqual(routes, by: ===)
        guard changed || !columns.presses.isEmpty else {
            #if os(iOS)
            reconcileBackHistory(in: columns.list)
            reconcileBackHistory(in: columns.detail)
            #endif
            return
        }
        if let root = routes.first, let runtime = presenter.session?.runtime {
            if let rule = runtime.stackSplitAnswer(root.node.id, retaining: columns.listNode, excluding: candidateSearchResult(in: root)?.id) {
                columns.rule = rule
                columns.listNode = rule?.list
            } else {
                askSplitNextTurn()
            }
        } else if routes.isEmpty {
            columns.rule = nil
            columns.listNode = nil
        }
        creditPresses(columns, batch: sequence, stack: routes)
        if changed && (columns.inTransition || routesInFlight(routes)) {
            tabsHeld = true
            for owner in [columns.controller, columns.list, columns.detail] {
                if let transition = owner.transitionCoordinator { runHeld(after: transition) }
            }
            return
        }
        prepareRoutes(Array(routes.prefix(1)), in: columns.list)
        prepareRoutes(Array(routes.dropFirst()), in: columns.detail)
        guard columns.show(routes, animated: false) else { tabsHeld = true; return }
        columns.pressedRows = columns.pressedRows.filter { id, _ in routes.contains { $0.node.id == id } }
        columns.sidebarRow = detailRow(columns, in: columns.rule, base: routes.dropFirst().first)
        if let list = columns.listNode { presenter.groupedLists?.sync(changed: [list]) }
    }

    /// A route pushed within N1 zooms from its `navigationSource` in the
    /// route beneath (ZoomPushIOS), as a pushed stack's does.
    private func prepareRoutePush(_ route: RouteController, beneath: RouteController) {
        #if os(iOS)
        prepareZoomPush(route, beneath: beneath)
        #endif
    }

    /// N1's own transition, which N0's push and pop of N1 play, belongs to
    /// N1's base (LLP 1127 D6, "How the columns move"): the zoom from or
    /// into the list's route that the base's `navigationSource` names
    /// (ZoomPushIOS), as the base pushed onto a stack would have, and none
    /// for a base that names none or for no base. A route pushed within N1
    /// has its own.
    private func prepareColumnTransition(_ base: RouteController?, in split: SplitIOS) {
        #if os(iOS)
        guard #available(iOS 18.0, *) else { return }
        split.detail.preferredTransition = nil
        guard let base, let root = split.list.stackRoutes.first as? RouteController else { return }
        prepareZoomPush(base, beneath: root, on: split.detail)
        #endif
    }

    /// The route a split's column stands for in UIKit's push or pop: N1 is
    /// pushed onto N0 and popped off it with the route on its top (LLP 1127
    /// §1, item 2).
    func pushedRoute(_ controller: UIViewController) -> RouteController? {
        controller as? RouteController ?? (controller === split?.detail ? split?.detailRoutes.last : nil)
    }

    /// UIKit changed what the split shows: the bars, the routes' insets,
    /// which routes are live (`routesShown`) and a sync its move held follow.
    func splitChanged() {
        guard let split else { return }
        #if os(iOS)
        reconcileBackHistory(in: split.list)
        reconcileBackHistory(in: split.detail)
        if split.paired {
            presenter.groupedLists?.sync(changed: nil)
            scheduleSplitProjection()
            return
        }
        #endif
        // Nested, N1's bar is UIKit's; N0's shows the top of the column on top (`showBar`)
        if !split.nested { showBar(split.detail, animated: false) }
        showBar(split.list, animated: false)
        coversChanged()
        presenter.owners.publish()
        // Beside the detail the list is UIKit's sidebar list, collapsed its authored style (LLP 1127 D7)
        presenter.groupedLists?.sync(changed: nil)
        if split.inspecting != nil || presenter.modalHost.routes.last?.node.props["navigationInspector"] == "true" {
            projectionHeld = true
        }
        runHeldSync()
    }

    /// The selected visit the expanded split gave to its inspector.
    var inspectorKey: String? {
        guard let split, !split.controller.isCollapsed, let route = split.inspecting,
              route.key == selectedKey else { return nil }
        return route.key
    }

    /// The selected visit a pop of the detail carries away from the inspector's column with the route it was over
    /// (the inspector, LLP 1127 §7): the one the column shows, or the one it showed as the pop began, which UIKit's pop has taken
    /// out of the column by the time the pop is followed (`SplitIOS.carried`).
    var poppingInspectorKey: String? {
        inspectorKey ?? split?.carried.flatMap { $0 == selectedKey ? $0 : nil }
    }

    /// The sheet an expanded split can show in its inspector column.
    func inspectorRoute(_ parts: [Range<Int>], _ wanted: [RouteController]) -> RouteController? {
        guard let split, split.inspects, !split.controller.isCollapsed, parts.count == 2, parts[1].count == 1 else { return nil }
        let route = wanted[parts[1].lowerBound]
        guard route.node.props["navigationPresentation"] == "modal", route.node.props["navigationInspector"] == "true" else { return nil }
        return route
    }

    func placeInspector(_ route: RouteController?, animated: Bool = true) {
        guard let split else { return }
        let nav = route.map { split.inspector ?? makeNavigation(first: $0.node) }
        if let route, let nav { prepareRoutes([route], in: nav) }
        split.inspect(route, in: nav, animated: animated)
    }

    /// Whether UIKit's pop recognizer of `nav` pops a route of the router's:
    /// that of the stack in front, and of a split's either column.
    func follows(_ nav: UINavigationController) -> Bool {
        nav === navigation || (modalNavigation == nil && split?.holds(nav) == true)
    }

    /// Whether a pop on `nav` took `route` away: from its own stack, or, as
    /// N1 left N0, with N1.
    func pops(_ route: RouteController, on nav: UINavigationController) -> Bool {
        route.lastStack === nav || (split.map { nav === $0.list && route.lastStack === $0.detail } ?? false)
    }

    /// The coordinator of the pop that took `route` out of its stack: its
    /// stack's, or N0's while N1 is what leaves (LLP 1127 D10).
    func popCoordinator(of route: RouteController) -> UIViewControllerTransitionCoordinator? {
        guard let stack = route.lastStack else { return nil }
        let columns = split?.detail === stack ? split : tabSplits.values.first { $0.detail === stack }
        if let columns, !columns.nested { return columns.list.transitionCoordinator ?? stack.transitionCoordinator }
        return stack.transitionCoordinator
    }

    /// An interactive pop's source on `nav` and the depth of the app's stack
    /// before it began. N1 leaving N0 takes its top with it, above the root;
    /// beneath N1's own routes is the root (LLP 1127 D10).
    func popSource(_ transition: UIViewControllerTransitionCoordinator, on nav: UINavigationController) -> Swipe? {
        let from = transition.viewController(forKey: .from)
        if let split, nav === split.list, from === split.detail {
            if popOnlyShowsList(split) { return nil }
            return split.detailRoutes.last.map { ($0.node, $0.key, 1 + split.detailRoutes.count) }
        }
        let beneath = split?.detail === nav ? 2 : 1
        return (from as? RouteController).map { ($0.node, $0.key, nav.viewControllers.count + beneath) }
    }

    /// Whether UIKit's pop of N1 off N0, collapsed, only shows the list (LLP 1127 D11, D12): N1 keeps its routes and
    /// the router keeps them, unless N1 holds only its first route and that route declares a Back of its own, which
    /// the pop is then (LLP 1115 D5). A history choice of the list while N1 holds more keeps N1 whole.
    func popOnlyShowsList(_ split: SplitIOS) -> Bool {
        let routes = split.detailRoutes
        guard let first = routes.first else { return false }
        // The route's node as the tree holds it now: a route written anew keeps its controller
        let node = routeIDs.compactMap { presenter.views[$0] }.first { $0.props["navigationKey"] == first.key } ?? first.node
        return routes.count > 1 || !declaresBack(node)
    }

    /// How the split's routes show against the stack's top (LLP 1127 D5):
    /// as the columns show them while nothing is presented over the split
    /// (`SplitIOS.showing`). A presentation covers what is under it first:
    /// under sheets the routes the columns show are shown and not live; a
    /// full-screen one hides them.
    var splitShown: (beside: [RouteController], under: [RouteController]) {
        guard let split else { return ([], []) }
        #if os(iOS)
        // A split's search says which column a person sees (`publishSplitSearch`), collapsed too, where the
        // columns show nothing beside: the detail it holds behind the list is under it
        if split.paired && !presenter.modalHost.routes.contains(where: { $0.kind == "fullscreen" }) {
            let list = split.searchOwner, detail = split.detail.topViewController as? RouteController
            let state = split.searchState
            let live = (state.primaryExposed ? [list] : []) + (state.detailExposed ? [detail] : [])
            let under = (state.primaryExposed ? [] : [list]) + (state.detailExposed ? [] : [detail])
            return (live.compactMap { $0 }, under.compactMap { $0 })
        }
        #endif
        let columns = split.showing, over = presenter.modalHost.routes
        if over.isEmpty { return columns }
        return ([], over.allSatisfy { NavigationRules.sheetLike($0.kind) } ? columns.beside + columns.under + [split.inspecting].compactMap { $0 } : [])
    }

    /// Each route the split shows beside the stack's top or under a pane
    /// over it, as the runner is told (claim kind 23 of `exact_host_covers`).
    var routesShown: [UInt32: BlockShown] {
        let shown = splitShown
        var routes: [UInt32: BlockShown] = Dictionary(shown.beside.map { ($0.node.id, BlockShown.beside) } + shown.under.map { ($0.node.id, BlockShown.under) },
                                                      uniquingKeysWith: { first, _ in first })
        // The detail UIKit keeps behind the folded list is out of sight, whatever is presented over it
        for route in split?.kept ?? [] { routes[route.node.id] = .hidden }
        return routes
    }

    /// Whether `view` is in the split's list column while the split is
    /// expanded: UIKit's sidebar, which keeps its look while UIKit hides it.
    func showsAsSidebar(_ view: UIView) -> Bool {
        sidebar(containing: view) != nil
    }

    private func sidebar(containing view: UIView) -> SplitIOS? {
        func contains(_ columns: SplitIOS) -> Bool {
            !columns.controller.isCollapsed && columns.list.stackRoutes.first?.viewIfLoaded.map(view.isDescendant(of:)) == true
        }
        if let split, contains(split) { return split }
        return tabSplits.values.first(where: contains)
    }

    /// The row of the split's list whose detail shows, which a sidebar keeps
    /// selected; nil, keeping the row a person pressed, while a row's press
    /// has batches still to apply (`listPress`).
    func sidebarSelection(_ view: UIView) -> UInt32?? {
        guard let columns = sidebar(containing: view) else { return .some(nil) }
        guard columns.presses.isEmpty else { return nil }
        return columns === split ? detailRow : .some(columns.sidebarRow)
    }

    /// `detailRow(in:)` by the runner's rule; nil while the runner cannot
    /// say (`splitRule`).
    var detailRow: UInt32?? {
        guard let split, let root = split.list.stackRoutes.first as? RouteController else { return .some(nil) }
        guard let rule = splitRule(root) else { return nil }
        return .some(detailRow(split, in: rule, base: base(selectedStack).dropFirst().first))
    }

    /// The row whose detail shows, for the stack's base, its first route
    /// above the root below any presentation (r_b with b = 1, LLP 1127 D3),
    /// so a route pushed within the detail keeps the base's row: the row
    /// that links to the base (`StackSplit.rows`), else the row whose press
    /// brought it while it keeps that location, else, for a detail no row
    /// brought (one the app opened, at launch or after Back), the row the
    /// app marks current (`StackSplit.current`), as the Mac's split and the
    /// web's take it. Read from the router: the row the columns are being
    /// written for. None without a detail.
    func detailRow(_ split: SplitIOS, in rule: StackSplit?, base: RouteController?) -> UInt32? {
        guard let base else { return nil }
        if let row = rule?.rows[base.node.id] { return row }
        if let pressed = split.pressedRows[base.node.id], pressed.at == rule?.locations[base.node.id] { return pressed.row }
        return rule?.current
    }

    /// A press of a row in the split's list column, collapsed or expanded:
    /// a base that a batch the runtime makes during `press` brings or moves,
    /// which no row links to, is that row's while it keeps that location
    /// (LLP 1127 D3), as a detail that comes while a Mac table's row is
    /// pressed (SplitMac.swift). Such a batch may apply after `press`
    /// returns, behind a fill in flight; each batch is judged as it syncs
    /// (`creditPresses`), and the press ends once none waits (`endPress`).
    func listPress(_ row: UInt32, in view: UIView, _ press: () -> Void) {
        guard let split, let list = split.list.stackRoutes.first?.viewIfLoaded, view.isDescendant(of: list),
              let sequence = presenter.session?.runtime.sequence else { return press() }
        split.presses.append(.init(row: row, after: sequence.newest))
        let index = split.presses.count - 1
        press()
        if split.presses.indices.contains(index) { split.presses[index].through = sequence.newest }
        endPress(split)
    }

    /// Each batch's sync, after its projection and before any hold: a base
    /// the batch brought or moved, against the base the batch before it
    /// left, which no row links to, is the row's whose press made the batch
    /// (the latest press's, when several did). So a press that changed
    /// nothing credits nothing, and a sync UIKit's transition holds is
    /// judged all the same.
    func creditPresses(_ split: SplitIOS, batch sequence: UInt64, stack: [RouteController]) {
        let base = stack.dropFirst().first.map { (id: $0.node.id, at: split.rule?.locations[$0.node.id]) }
        let before = split.lastBase
        split.lastBase = base
        let made = split.presses.indices.filter { split.presses[$0].after < sequence && sequence <= split.presses[$0].through }
        guard let latest = made.last else { return }
        for i in made { split.presses[i].landed = true }
        guard let base, base.id != before?.id || base.at != before?.at, split.rule?.rows[base.id] == nil else { return }
        split.pressedRows[base.id] = (split.presses[latest].row, base.at)
    }

    /// End the presses once the session holds none of their batches, and
    /// show the sidebar's row for the detail, which waited for them
    /// (`sidebarSelection`). Asked at each sync of the split once every
    /// press has had a batch credited and, with `poll`, each frame until
    /// then, for batches the session drops as changing nothing.
    private func endPress(_ split: SplitIOS, poll: Bool = true) {
        guard !split.presses.isEmpty else { return }
        if presenter.session?.batchesWaiting == true {
            if poll { DispatchQueue.main.asyncAfter(deadline: .now() + 1.0 / 60) { [weak self, weak split] in if let split { self?.endPress(split) } } }
            return
        }
        split.presses = []
        if !split.controller.isCollapsed, let list = split.listNode { presenter.groupedLists?.sync(changed: [list]) }
    }

    #if EXACT_AGENT
    /// For `state window.split` (LLP 1127 D11): whether UIKit collapsed the
    /// split, its display mode and the list's width; each column's routes by
    /// key, `nested` while UIKit holds N1 in N0, `empty` while the detail is
    /// the empty page; the routes shown
    /// beside the stack's top and under a pane over it (D8); and the row
    /// whose detail shows, which the sidebar keeps selected while expanded.
    var splitObservation: [String: Any] {
        guard let split else { return ["installed": false] }
        let shown = splitShown
        let modes: [UISplitViewController.DisplayMode: String] = [.secondaryOnly: "secondaryOnly", .oneBesideSecondary: "oneBesideSecondary",
            .oneOverSecondary: "oneOverSecondary", .twoBesideSecondary: "twoBesideSecondary", .twoOverSecondary: "twoOverSecondary",
            .twoDisplaceSecondary: "twoDisplaceSecondary"]
        var observation: [String: Any] = ["installed": true, "collapsed": split.controller.isCollapsed, "nested": split.nested, "displayMode": modes[split.controller.displayMode] ?? "automatic",
                "listWidth": InspectionValue.r2(split.list.view.bounds.width),
                "list": split.list.stackRoutes.compactMap { ($0 as? RouteController)?.key }, "detail": split.detailRoutes.map(\.key),
                "empty": split.detail.topViewController === split.empty, "inspector": split.inspecting.map { [$0.key] } ?? [], "beside": shown.beside.map(\.key), "under": shown.under.map(\.key),
                "row": (detailRow ?? nil).map { Int($0) } ?? NSNull()]
        #if os(iOS)
        if let search = split.pairedSearch, let field = search.field, let results = search.results {
            let state = split.searchState
            let covering = modalNavigation != nil || presenter.dialogs.top != nil || split.inspecting != nil
            let column = results.visible ? (results.coversList ? "primary" : "secondary") : nil
            let foreground = covering ? "presentation" : state.primaryForeground && state.primaryExposed
                ? (column == nil ? "primary" : "results") : state.detailExposed ? "secondary" : "primary"
            observation["search"] = [
                "field": Int(field.id), "value": field.props["value"] ?? "",
                "open": search.placesResultsInDetail ? search.open : search.controller.isActive,
                "active": search.controller.isActive, "root": results.root.map { Int($0.id) } ?? NSNull(),
                "column": column ?? NSNull(), "covered": results.root != nil && !results.visible,
                "foreground": foreground,
            ] as [String: Any]
        }
        #endif
        return observation
    }
    #endif

    /// The split goes. `releasing`, its routes leave its columns first, for
    /// the containers that take them next (`reshape`).
    func retireSplit(releasing: Bool = false) {
        guard let split else { return }
        #if os(iOS)
        FrameClock.shared.drop(split.searchState)
        split.searchState.showInstalledDetail()
        split.searchOwner?.detachSearch()
        #endif
        for nav in [split.list, split.detail] + [split.inspector].compactMap({ $0 }) {
            if releasing { release(nav) } else { retireStack(nav) }
        }
        split.controller.willMove(toParent: nil)
        split.controller.view.removeFromSuperview()
        split.controller.removeFromParent()
        self.split = nil
        primaryNavigation = nil
    }
}
#endif
