// @ref LLP 1075.003 §3.5 — what Exact's containers cover of each route, told
// to the kernel as covers, and the development check of what Exact owns on
// the objects it handed a hatch.
#if os(iOS) || os(tvOS)
import UIKit

extension NavigationHost {
    // MARK: The content area (LLP 1075.003 §3.5)

    /// Whether the root's routes have a native primary container (a stack
    /// or the tabs), so the session's view takes the whole of its own
    /// (ExactViewIOS `fit`): the document and the window the viewport units
    /// resolve against. Its bars, whether shown or not, and a
    /// presentation's containers never decide it (LLP 1123 D1); its routes
    /// clear what covers them (`reportCovers`). Not under the agent's own
    /// chrome, where no native container shows.
    var wantsWholeView: Bool { primaryOwner != nil && !ExactEnv.authoredChrome }

    /// @ref LLP 1075.003 §3.7, LLP 1083.000 D4 — the native container stands
    /// for the routes in the navigation root's paint order, but its box is
    /// the page's, where the routes' boxes may not be: a column the author
    /// placed beside routes offset past it shows on the web and would be
    /// covered here. A child of the root that no shown route's box
    /// meets, which the routes therefore paint nothing over, paints over the
    /// container; one a route's box meets keeps CSS's order under it. The
    /// tab panels and the tablist the bar takes the place of are the
    /// container's own. Such a child that reaches the container's top edge
    /// and one side (a column) grows the container's safe area on that side
    /// by its width, so UIKit lays the bars' contents out clear of it, as a
    /// hand-built screen insets its bars from a view beside them; the routes
    /// clear only the part of that inset their own boxes reach (`reportCovers`).
    func clearBesideRoutes() {
        guard let owner = primaryOwner, let root = container, let holder = owner.viewIfLoaded, holder.superview === root else { return }
        let routes = primaryRoutes.filter { $0.viewIfLoaded?.window != nil && presenter.views[$0.node.id] === $0.node }
            .map { $0.node.convert($0.node.bounds, to: root) }.filter { !$0.isEmpty }
        let owned = Set(tabPanels + [adoptedTablist].compactMap { $0 })
        let area = holder.convert(holder.bounds, to: root)
        var changed = false, reach = UIEdgeInsets.zero
        for case let child as NodeView in root.subviews {
            let box = child.convert(child.bounds, to: root)
            let clears = !ExactEnv.authoredChrome && !routes.isEmpty && !owned.contains(child.id) && !box.isEmpty
                && box.intersects(area) && !routes.contains { $0.intersects(box) }
            if child.clearsNavigation != clears {
                child.clearsNavigation = clears
                changed = true
            }
            guard clears, box.minY <= area.minY + 0.5 else { continue }
            let left = box.minX <= area.minX + 0.5, right = box.maxX >= area.maxX - 0.5
            if left, !right { reach.left = max(reach.left, box.maxX - area.minX) }
            if right, !left { reach.right = max(reach.right, area.maxX - box.minX) }
        }
        if changed { PaintOrder.changed(root) }
        // Exact's part only: what an app's hatch set on its container stays
        let before = besideOwner === owner ? besideInsets : .zero, now = owner.additionalSafeAreaInsets
        let wanted = UIEdgeInsets(top: now.top - before.top + reach.top, left: now.left - before.left + reach.left,
                                  bottom: now.bottom - before.bottom + reach.bottom, right: now.right - before.right + reach.right)
        if now != wanted { owner.additionalSafeAreaInsets = wanted }
        besideInsets = reach
        besideOwner = owner
    }

    /// What a route's padding clears of its controller's safe area (`safe`),
    /// its local inset on each edge it executes (LLP 1123 D3): the kernel
    /// adds it to the route's authored padding, or credits a padding that
    /// reads the inset, and turns off an edge the route opts out of (D4).
    /// The top is a collapsing title's scroller's when it has one; the sides
    /// and the bottom only while the containers take the whole view
    /// (`whole`) or the page covers it (`covered`, `viewport-fit="cover"`),
    /// and the bottom only as far as the route reaches it.
    static func routeEdges(safe: UIEdgeInsets, whole: Bool, covered: Bool = false, collapses: Bool, reached: CGFloat) -> HostCover.Edges {
        let edges = whole || covered
        return .init(top: collapses ? 0 : max(0, safe.top),
                     right: edges ? max(0, safe.right) : 0,
                     bottom: edges ? max(0, min(safe.bottom, reached)) : 0,
                     left: edges ? max(0, safe.left) : 0)
    }

    /// What covers `node` but for `kept`, which stays in the page: the node
    /// itself, or, where it holds any of `kept`, each part of it that holds
    /// none (host/linux/src/bars.rs `covered`).
    static func covered(_ node: NodeView, keeping kept: [NodeView]) -> [NodeView] {
        guard kept.contains(where: { $0.isDescendant(of: node) }) else { return [node] }
        return node.container.subviews.compactMap { $0 as? NodeView }
            .filter { child in !kept.contains { $0 === child } }
            .flatMap { child in kept.contains { $0.isDescendant(of: child) } ? covered(child, keeping: kept) : [child] }
    }

    /// Whether `sv` is pulled past where its title's inset rests (`origin`),
    /// as to refresh. UIKit grows the scroller's safe area with the pull, and
    /// with the refresh control's room while it shows, and gives it back as
    /// the list returns: neither is the bar's inset, where CSS scrollTop 0
    /// rests, and a hand-built list lays out nothing for it. Reported, each
    /// frame of a pull laid the page out again.
    static func stretched(_ sv: UIScrollView, origin: CGFloat) -> Bool {
        origin > 0 && sv.contentOffset.y < -origin - 0.5
    }

    /// What Exact's containers cover of each route — its controller's safe
    /// area past the insets the kernel already has as `env()`: a shown bar's
    /// top, and every edge while the containers take the whole view for a
    /// page that did not ask to cover it — and the header a bar replaces,
    /// reported when they change, after the batch being applied.
    func reportCovers() {
        clearBesideRoutes()
        var changes: [(UInt32, HostCover?)] = []
        var wanted: [UInt32: HostCover] = [:]
        var extras: [ObjectIdentifier: (UIScrollView, top: CGFloat, bottom: CGFloat)] = [:]
        #if os(iOS)
        if #available(iOS 27.1, *) {
            for id in presenter.arrangements.hiddenPanes { wanted[id] = .whole }
        }
        #endif
        for c in controllers.values where presenter.views[c.node.id] === c.node {
            // A lifted header takes no room, on a stack yet or not (`coverLifted`).
            for node in (c.lifted.map({ [$0] }) ?? c.barNodes) + [c.scoped].compactMap({ $0 }) { wanted[node.id] = .whole }
            // Include the departing route until UIKit finishes its transition
            let nav = c.navigationController
            let stacked = c.stack?.stackRoutes.contains(c) == true || nav?.transitionCoordinator?.viewController(forKey: .from) === c
            guard stacked else { continue }
            // A presented route's controller takes its sheet's whole view, and
            // the document's insets (`env()`) are not the sheet's: its route is
            // covered for its own safe area whatever the document's fit (LLP
            // 1123 D2; its own clearance and credit are stage 2's)
            let presented = isPresented(nav)
            let covered = wantsWholeView && presenter.viewportFit == "cover"
            let whole = presented || (wantsWholeView && !covered)
            let shows = nav.map(barShows) ?? !ExactEnv.authoredChrome
            guard shows || whole || covered else { continue }
            let edges: HostCover.Edges?
            // A collapsing title's scroller goes under the bar, which insets it;
            // its expanded inset is where CSS scrollTop 0 rests.
            // While the bar is not as the route wants it (mid-transition to or
            // from a route without one, §9.10), or the route is not in a window
            // (a tab UIKit has yet to show), its last cover stands.
            // An active search's bar is UIKit's: the content follows it, as
            // UIKit's own does, but a title's insets are not sampled.
            // A plain tab's route has no bar of its own: More's shows above it
            // under More, and none on the tab bar.
            let search = searching(c)
            let barAbove = nav.map { c.parent is PlainTab ? !$0.isNavigationBarHidden : routeShowsBar(c, in: $0) } ?? false
            let settled = c.viewIfLoaded?.window != nil && (search || (nav?.isNavigationBarHidden ?? true) != barAbove)
            // A route showing no bar has no title: its inset is the status bar's, none in landscape (LLP 1130 D3).
            let barless = !barAbove
            let topPeers = arrangementScrollers(in: c, top: true), bottomPeers = arrangementScrollers(in: c, top: false)
            if settled, !search, let node = c.collapseScroll, let sv = c.collapseScrollView, sv === node.scrollView, sv.adjustedContentInset.top > 0 || barless {
                let inset = sv.adjustedContentInset.top
                if c.insetsBarless != barless { c.insetsBarless = barless; node.scrollCollapsed = 0 }
                // tvOS has no large titles: every title is inline.
                #if os(tvOS)
                let inline = true
                #else
                let inline = c.navigationItem.largeTitleDisplayMode != .always || barless
                #endif
                if Self.stretched(sv, origin: node.scrollOrigin) {
                    // Pulled past its top, as to refresh: no title's inset.
                } else if inline {
                    // An inline title does not collapse: its inset is the one it has.
                    node.scrollOrigin = inset; node.scrollCollapsed = inset
                } else if nav?.transitionCoordinator == nil {
                    // A large title's widest and narrowest, at rest only.
                    if inset > node.scrollOrigin { node.scrollOrigin = inset }
                    if node.scrollCollapsed == 0 || inset < node.scrollCollapsed { node.scrollCollapsed = inset }
                }
            }
            if settled {
                // A route offset in its block, past a box beside it (`clearBesideRoutes`), clears only the part of
                // each side's inset its box reaches
                let box = c.node.convert(c.node.bounds, to: c.view)
                var safe = c.view.safeAreaInsets
                safe.left = max(0, safe.left - max(0, box.minX))
                safe.right = max(0, safe.right - max(0, c.view.bounds.maxX - box.maxX))
                // The route's end with its page unscrolled: scrolling moves the route, not the bars.
                // A route the keyboard shortens can end above the bars, with no bottom cover.
                let end = c.node.convert(c.node.bounds, to: c.contentMount).maxY + (c.contentViewport?.frame.minY ?? 0)
                let reached = end - (c.view.bounds.maxY - safe.bottom)
                // A transparent bar covers nothing of the route's top, scroller or not: the content starts under it.
                edges = Self.routeEdges(safe: safe, whole: whole, covered: covered,
                                        collapses: c.collapseScroll != nil || !topPeers.isEmpty || !c.ridingBars.isEmpty || Self.insetsNoTop(c.node),
                                        reached: reached)
                // An edge the route opts out of has no clearance to take (LLP 1123 D4).
                c.bottomClearance = Self.clearanceOff(c.node) & 4 != 0 ? 0 : edges?.bottom ?? 0
                if let node = c.collapseScroll, let sv = node.scrollView, !Self.insetsNoTop(c.node) {
                    let top = clearanceExtra(c, scroller: node, top: true)
                    let held = extras[ObjectIdentifier(sv)]
                    extras[ObjectIdentifier(sv)] = (sv, top, held?.bottom ?? 0)
                }
                if let node = c.edgeScroll ?? c.collapseScroll, let sv = node.scrollView,
                   node === c.edgeScroll || Self.clearanceOff(c.node) & 4 != 0 && !Self.insetsNoTop(c.node) {
                    let bottom = clearanceExtra(c, scroller: node, top: false)
                    let held = extras[ObjectIdentifier(sv)]
                    extras[ObjectIdentifier(sv)] = (sv, held?.top ?? 0, bottom)
                }
            } else if case .edges(let e)? = covers[c.node.id] {
                edges = e
            } else { edges = nil }
            // Unsettled, the scrollers keep the clearance they hold, as the route its cover.
            for sv in [c.collapseScrollView, c.edgeScroll?.scrollView].compactMap({ $0 }) where !settled {
                if let held = clearance[ObjectIdentifier(sv)] { extras[ObjectIdentifier(sv)] = (sv, held.top, held.bottom) }
            }
            // A scroller UIKit insets lays its content out between those
            // insets (LLP 1116.000 D1.2): content that fills its height rests
            // under the bar and above the home indicator, and scrolling
            // content still passes under both. A large title's inset is its
            // widest, at rest (CSS scrollTop 0).
            // A transparent bar insets nothing of its scroller: the content
            // starts under it (`collapse`), so no `.inset` is reported there.
            var insetNodes = [c.collapseScroll, c.edgeScroll].compactMap { $0 }
            for peer in topPeers + bottomPeers where !insetNodes.contains(where: { $0 === peer }) { insetNodes.append(peer) }
            for node in insetNodes where wanted[node.id] != .whole && node.scrollView != nil && !(node === c.collapseScroll && Self.insetsNoTop(c.node)) {
                if settled, let safe = node.scrollView?.safeAreaInsets {
                    // The bottom edge's scroller takes the route's bottom
                    // clearance, which the route then pads nothing of
                    // (`scroller_insets_bottom`) and still moves an
                    // absolutely positioned footer past. Its own safe area
                    // holds none while that padding ends it above the bars:
                    // read alone, it would leave it there for good.
                    let peerTop = topPeers.contains { $0 === node }, peerBottom = bottomPeers.contains { $0 === node }
                    let reach = node === c.edgeScroll || peerBottom ? c.bottomClearance : 0
                    let ownsTop = node === c.collapseScroll || peerTop
                    #if os(iOS)
                    if peerTop, let scroll = node.scrollView {
                        if c.navigationItem.largeTitleDisplayMode == .always { node.scrollOrigin = max(node.scrollOrigin, scroll.adjustedContentInset.top) }
                        else { node.scrollOrigin = scroll.adjustedContentInset.top }
                        if node !== c.collapseScroll { node.scrollCollapsed = node.scrollOrigin }
                    }
                    #endif
                    // A band's part of the safe area is a UI inset over control bars, not layout (LLP 1131 D2).
                    let title = ownsTop ? node.scrollOrigin : 0
                    let stretched = title > 0 && node.scrollView.map { Self.stretched($0, origin: title) } == true
                    // What an opted-out bottom takes back of UIKit's inset (ClearanceIOS.swift); what
                    // clearance adds is the title's at the top, and the route's clearance (`reach`) below.
                    let back = min(0, node.scrollView.flatMap { clearance[ObjectIdentifier($0)]?.bottom } ?? 0)
                    let top = max(0, max(stretched ? min(safe.top, title) : safe.top, title) - node.pageBand.top)
                    let bottom = max(0, max(safe.bottom, reach) + back - node.pageBand.bottom)
                    if top > 0 || bottom > 0 { wanted[node.id] = .inset(top: top, bottom: bottom) }
                } else if case .inset? = covers[node.id] { wanted[node.id] = covers[node.id] }
            }
            guard let edges else { continue }
            if edges != .init(top: 0, right: 0, bottom: 0, left: 0) { wanted[c.node.id] = .edges(edges) }
        }
        // The tablist whose place the tab bar takes takes no room. One that
        // holds the root's panels keeps them, and their routes, in the page.
        if let list = adoptedTablist.flatMap({ presenter.views[$0] }) {
            measuredTablist = presenter.session?.view.map { list.convert(list.bounds, to: $0) }
            if adopts(tablist: list) {
                for node in Self.covered(list, keeping: tabPanels.compactMap { presenter.views[$0] }) { wanted[node.id] = .whole }
            }
            else if tablistAppearanceNone, list.style["display"]?.string != "none",
                    wantsWholeView, presenter.viewportFit != "cover",
                    let view = presenter.session?.view, view.window != nil {
                // Authored tabs are page content outside the routes. Protect
                // only the edges they reach; leave the viewport and bar
                // backgrounds full-size. A sheet leaves them where they are
                // (LLP 1123 D1).
                let rect = list.convert(list.bounds, to: view)
                let safe = view.bounds.inset(by: view.safeAreaInsets)
                let edges = HostCover.Edges(
                    top: max(0, min(rect.height, safe.minY - rect.minY)),
                    right: max(0, min(rect.width, rect.maxX - safe.maxX)),
                    bottom: max(0, min(rect.height, rect.maxY - safe.maxY)),
                    left: max(0, min(rect.width, safe.minX - rect.minX)))
                if edges != .init(top: 0, right: 0, bottom: 0, left: 0) { wanted[list.id] = .edges(edges) }
            }
        }
        applyClearance(extras)
        for (id, cover) in wanted where covers[id] != cover { changes.append((id, cover)) }
        for id in covers.keys where wanted[id] == nil && presenter.views[id] != nil { changes.append((id, nil)) }
        covers = wanted
        let locals = reportLocalInsets()
        guard !changes.isEmpty || locals else { return }
        // A scroller's least content height is the port its cover leaves
        // (`scrollFloor`); the kernel sends no new content size when only that changed.
        for (id, _) in changes { presenter.views[id]?.fitScroll() }
        // Under Q3 (c) a cold launch's first frame runs no app code: a hatch
        // that changes a bar the plan already showed moves the content once.
        if replayingHatches, changes.contains(where: { presenter.views[$0.0]?.props["navigationKey"] != nil }) {
            presenter.session?.log("hatch: the content area moved after the first frame, by a hatch run at launch")
        }
        // Never inside a batch: the stack is installed partway through the
        // first one, whose remaining frames would overwrite the covered ones.
        presenter.afterBatch { [weak self] in self?.deliverCovers() }
    }

    /// What the kernel is told is what is wanted when it is told. A report
    /// queued behind a batch ran after later ones that did not wait, and its
    /// covers, measured under the bar's first frame before the status bar's
    /// inset (116 points with a large title of 168), won: team-kanban's
    /// persona switcher lay under the large title, drawn over by it and
    /// untappable (LLP 1116.006 D1).
    func deliverCovers() {
        // A callback the runtime waits on can make no runtime call: deliver once it returns
        if Owner.shared.isServingMain {
            DispatchQueue.main.async { [weak self] in self?.deliverCovers() }
            return
        }
        let views = presenter.views
        presenter.session?.localInsets(localInsets.take { views[$0] != nil })
        var changes: [(UInt32, HostCover?)] = []
        for (id, cover) in covers where delivered[id] != cover { changes.append((id, cover)) }
        for id in delivered.keys where covers[id] == nil && presenter.views[id] != nil { changes.append((id, nil)) }
        delivered = covers
        guard !changes.isEmpty else { return }
        presenter.onCovers?(changes)
    }

    /// @ref LLP 1123 D9 — each route's own safe area, which its content reads
    /// as `var(--exact-safe-area-inset-*)`: its controller's view's, the bars,
    /// the status bar and the home indicator in it, on the sides only as far
    /// as the route's box reaches them, as its covers are. A route out of a
    /// window keeps the last it had, UIKit's zero there being no measurement,
    /// and so does one while its stack's transition runs, as its cover does;
    /// one that has none yet takes what it has from its first frame. And the
    /// session root's while the containers take the whole view, where `env()`
    /// is 0 but the screen's insets are not. Whether any changed.
    func reportLocalInsets() -> Bool {
        var wanted: [UInt32?: [CGFloat]] = [:]
        for c in controllers.values where presenter.views[c.node.id] === c.node {
            let last = localInsets.wanted[c.node.id]
            guard let view = c.viewIfLoaded, view.window != nil, c.node.window != nil,
                  last == nil || c.navigationController?.transitionCoordinator == nil else {
                wanted[c.node.id] = last
                continue
            }
            let box = c.node.convert(c.node.bounds, to: view)
            let safe = view.safeAreaInsets
            wanted[c.node.id] = [safe.top, max(0, safe.right - max(0, view.bounds.maxX - box.maxX)),
                                 safe.bottom, max(0, safe.left - max(0, box.minX))]
        }
        if wantsWholeView, let view = presenter.session?.view, view.window != nil {
            let safe = view.safeAreaInsets
            wanted[nil] = [safe.top, safe.right, safe.bottom, safe.left]
        }
        defer { localInsets.wanted = wanted }
        return localInsets.wanted != wanted
    }

    // MARK: The development check (LLP 1075.003 §3.5)

    /// Whether this build checks what Exact owns: every build but a
    /// production bake.
    static let checksOwnership = !ExactEnv.productionBake

    /// Record the controllers Exact set on a stack.
    func recordOwned(_ nav: UINavigationController) {
        stacks[ObjectIdentifier(nav)]?.written = nav.viewControllers.map(ObjectIdentifier.init)
    }

    /// UIKit completed a transition (`didShow`): a pop the user made (UIKit's
    /// back button, the edge swipe) leaves a prefix of what Exact wrote, and
    /// is recorded; any other change is not Exact's and stays to be reported.
    func recordPop(_ nav: UINavigationController) {
        guard let stack = stacks[ObjectIdentifier(nav)] else { return }
        let now = nav.viewControllers.map(ObjectIdentifier.init)
        if now.count <= stack.written.count, Array(stack.written.prefix(now.count)) == now { stack.written = now }
    }

    /// Before each batch: compare what Exact owns on each hatched object with
    /// what Exact last wrote, and journal a difference once, by name. It is
    /// detection, not enforcement; a change made and undone between two
    /// batches is not seen.
    func checkOwned() {
        guard NavigationHost.checksOwnership, !changing, !syncing, presenter.session?.natives.hatchesConnected == true else { return }
        func say(_ what: String, _ property: String) {
            let line = "\(what): \(property) changed outside Exact, which owns it"
            if ownedReported.insert(line).inserted { presenter.session?.log(line) }
        }
        // `allNavigations` leaves out a tab's own navigation controller while UIKit holds its routes in More; More has
        // no hatch, so the `hatched` guard skips it
        for nav in allNavigations {
            guard let stack = stacks[ObjectIdentifier(nav)], stack.hatched else { continue }
            if nav.delegate !== stack.proxy { say("navigation \(stack.label)", "delegate") }
            if nav.transitionCoordinator == nil, !searching(nav.topViewController as? RouteController), nav.isNavigationBarHidden == topShowsBar(nav) { say("navigation \(stack.label)", "navigation bar visibility") }
            if nav.viewControllers.map(ObjectIdentifier.init) != stack.written { say("navigation \(stack.label)", "viewControllers") }
            #if !os(tvOS)
            if let pop = nav.interactivePopGestureRecognizer, pop.delegate !== self { say("navigation \(stack.label)", "the pop gesture's delegate") }
            if #available(iOS 26.0, tvOS 26.0, *), let pop = nav.interactiveContentPopGestureRecognizer, pop.delegate !== self {
                say("navigation \(stack.label)", "the content pop gesture's delegate")
            }
            #endif
        }
        for c in controllers.values where c.hatched && presenter.views[c.node.id] === c.node {
            if c.navigationController != nil, c.isViewLoaded, c.node.superview !== c.contentMount { say("route \(c.key)", "view") }
            guard let node = c.ownedScroll, let scroll = node.scrollView else { continue }
            for property in NavigationHost.ownedChanges(scroll, of: node, insetByUIKit: insetByUIKit(node)) { say("route \(c.key)", property) }
        }
    }

    /// What Exact sets on a node's scroll view and nothing else may (LLP
    /// 1075.003 §3.5) and differs from what Exact writes: no inset (a
    /// refresh control insets it while it spins), automatic adjustment for a
    /// route's collapse or bottom-edge scroller and arranged peers, the backend's
    /// Exact-owned delegate, the node's keyboard dismissal (authored, or
    /// the platform's for what it scrolls, LLP 1115.000 D9.1). The offset
    /// and the content size are Exact's too but are not checked: they move
    /// with the user and with layout, so no last write predicts them.
    static func ownedChanges(_ s: UIScrollView, of node: NodeView, insetByUIKit: Bool) -> [String] {
        var out: [String] = []
        // A native backend owns layout/footer insets, composed with the
        // refresh control and keyboard; zero is only the plain-scroll baseline.
        if s === node.scroll {
            #if os(tvOS)
            if s.contentInset != node.bandInset { out.append("contentInset") }
            #else
            if s.contentInset != node.bandInset, s.refreshControl?.isRefreshing != true { out.append("contentInset") } // a band's is the host's
            #endif
        }
        if s.contentInsetAdjustmentBehavior != (insetByUIKit ? .automatic : .never) { out.append("contentInsetAdjustmentBehavior") }
        if s !== node.scrollView || !node.ownsScrollDelegate { out.append("delegate") }
        if s.keyboardDismissMode != node.keyboardDismissal { out.append("keyboardDismissMode") }
        return out
    }
}

extension RouteController {
    /// A large title at rest stays open when the text size changes. UIKit
    /// draws the title at the new size in the bar's old height, which grows
    /// only when the list is pulled: at the largest size the title ran under
    /// the bar's buttons (meal-planner, bake-off round 34), and a hand-built
    /// bar clips it the same way. A scroller at its start is pulled open on
    /// the next turn, after UIKit's layout, as a title that turns large
    /// again in place is (`restAtTop`).
    func reopenTitle() {
        guard collapseScroll != nil, let host, let sv = host.contentScrollView(of: self), !sv.isTracking, !sv.isDecelerating,
              sv.contentOffset.y <= 0.5 - sv.adjustedContentInset.top else { return }
        DispatchQueue.main.async { [weak self, weak sv] in
            guard let self, let sv, let host = self.host, sv.contentOffset.y <= 0.5 - sv.adjustedContentInset.top else { return }
            host.restAtTop(sv, in: self)
        }
    }
}
#endif
