// @ref LLP 1075.003 §3.5 — what Exact's containers cover of each route, told
// to the kernel as covers, and the development check of what Exact owns on
// the objects it handed a hatch.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

extension NavigationHost {
    // MARK: The content area (LLP 1075.003 §3.5)

    /// Whether the root's routes have a native primary container (a stack
    /// or the tabs), so the session's view takes the whole of its own
    /// (ExactViewIOS `fit`): the document and the window the viewport units
    /// resolve against. Its bars, whether shown or not, and a
    /// presentation's containers never decide it (LLP 1123 D1); its routes
    /// clear what covers them (`reportCovers`). Not under the agent's own
    /// chrome, where no native container shows.
    var wantsWholeView: Bool { (primaryOwner != nil || bootOwner != nil) && !ExactEnv.authoredChrome }

    /// @ref LLP 1075.003 §3.7, LLP 1083.000 D4 — the native container stands
    /// for the routes in the navigation root's paint order. Its box can extend
    /// past the routes' authored boxes: a column the author placed beside
    /// routes offset past it shows on the web and would be covered here. A
    /// child of the root beside the routes, which they therefore paint nothing
    /// over (`PanelSiblings.beside`: no shown route's box meets it, and no
    /// route's view covers it except to its side), paints over the container;
    /// one a route's box meets keeps CSS's order under it. A child with no area
    /// is decided by the box its children paint over, within what clips them,
    /// and one with a `clip-path` by its own (`paintedBox`). The tab panels and
    /// a tablist replaced by the native bar are the container's own. Such a
    /// child wholly to one side of the routes grows the container's safe area
    /// on that side up to where CSS lays the routes out (`sideReach`), so UIKit
    /// lays the bars' contents out clear of it, as a hand-built screen insets
    /// its bars from a view beside them, and a tab's route, which UIKit draws
    /// over the page from the container's origin, is covered only for the part
    /// of that inset its own box does not clear (`reportCovers`). With the tab
    /// bar hidden, an in-flow row below the panels bounds the bottom too,
    /// whether the root's child or nested beside them. A node nested beside the
    /// panels that paints over the container is lifted into a layer just after
    /// it (`panelSiblings`, LLP 1083.000 D4), and grows the safe area as a root
    /// child does.
    @discardableResult func clearBesideRoutes(tabBarHidden: Bool? = nil) -> [NodeView] {
        guard let owner = primaryOwner, let root = container, let holder = owner.viewIfLoaded,
              holder.superview === ownerMount(root) else { return [] }
        // Each shown route's box where CSS lays it out, within its logical parent (UIKit holds a tab's route at its
        // container's origin, sized to the page, which leaves out the panel's place; a parent with no box of its
        // own leaves the frame as it is), and where its view is drawn
        let shown = primaryRoutes.filter { $0.viewIfLoaded?.window != nil && presenter.views[$0.node.id] === $0.node }
        let routes = shown.map { c in
            let parent = logicalParent(of: c.node) ?? root
            let frame = parent.container.convert(c.node.frame, to: root), within = frame.intersection(parent.convert(parent.bounds, to: root))
            return within.isEmpty ? frame : within
        }.filter { !$0.isEmpty }
        let drawn = shown.map { $0.node.convert($0.node.bounds, to: root) }.filter { !$0.isEmpty }
        var owned = Set(tabPanels + (tablistAppearanceNone ? [] : [adoptedTablist].compactMap { $0 }))
        if let footer = sidebarFooter() { owned.insert(footer.id) }
        #if os(iOS)
        if let accessory = tabAccessory, let node = accessory.node, accessory.holds { owned.insert(node.id) }
        #endif
        let area = holder.convert(holder.bounds, to: root)
        let id = ContributionsIOS.Identity(node: root.id, operation: .containerClearance)
        // What Exact's part did to the container's safe area, as measured, which UIKit's floor can hold
        let before = besideOwner === owner && besideIdentity == id ? ContributionsIOS.existing(on: owner)?.effect(of: id) ?? .zero : .zero
        var changed = false, reach = UIEdgeInsets.zero, above: [NodeView] = []
        siblings.place(in: ownerMount(root))
        var walk = panelSiblings(in: area, routes: routes, drawn: drawn, skipping: owned, hidden: tabBarHidden)
        if !walk.promote.isEmpty {
            // A flat leaf that paints over the container gets a view, mounted now, and lifts with the rest
            for id in walk.promote { presenter.flats.promote(id) }
            presenter.flats.flush()
            walk = panelSiblings(in: area, routes: routes, drawn: drawn, skipping: owned, hidden: tabBarHidden)
        }
        let (bottom, rows, lifts) = (walk.bottom, walk.rows, walk.lifts)
        #if os(iOS)
        // The adopted More on screen at its list, as its stack names it from the start of a pop to it (reading
        // `moreNavigationController` makes a More where UIKit has none); not the tab controller's selection, which
        // names More only after More's `didShow` for its pop to the list
        let moreList = more.map { $0.viewIfLoaded?.window != nil && $0.topViewController === $0.viewControllers.first } ?? false
        #else
        let moreList = false
        #endif
        // With no route in the window, under a full-screen cover, the last decision stands, as the covers do;
        // More's list, inside the container, shows no route for a node to be beside, so it lifts nothing
        if !routes.isEmpty || moreList { siblings.apply(lifts, in: ownerMount(root), after: holder) }
        // A lifted node beside the routes; one over them, an overlay, insets nothing, as CSS moves nothing for it
        for lift in lifts {
            let box = paintedBox(lift.node, in: root)
            guard PanelSiblings.beside(box, routes, drawn: drawn) else { continue }
            let asked = Self.reach(box, beside: routes, in: area)
            reach = UIEdgeInsets(top: max(reach.top, asked.top), left: max(reach.left, asked.left), bottom: 0, right: max(reach.right, asked.right))
            if asked.top > 0 { above.append(lift.node) }
        }
        for case let child as NodeView in ownerMount(root).subviews {
            let box = child.isHidden ? .null : paintedBox(child, in: root)
            let below = rows.contains { $0 === child }
            let clears = !ExactEnv.authoredChrome && !routes.isEmpty && !owned.contains(child.id) && !box.isEmpty
                && box.intersects(area) && (below || PanelSiblings.beside(box, routes, drawn: drawn))
            let paint: NavigationPaintRole = owned.contains(child.id) ? .represented : (clears ? .besideRoutes : .sibling)
            if child.navigationPaint != paint {
                child.navigationPaint = paint
                changed = true
            }
            guard clears else { continue }
            let asked = Self.reach(box, beside: routes, in: area)
            reach = UIEdgeInsets(top: max(reach.top, asked.top), left: max(reach.left, asked.left), bottom: 0, right: max(reach.right, asked.right))
            if asked.top > 0 { above.append(child) }
        }
        // Add only what the container's own safe area leaves: UIKit's (the indicator, a landscape notch, an outer
        // container's inset) and what an app's hatch added count toward the reach
        let own = holder.safeAreaInsets
        reach.top = max(0, reach.top - max(0, own.top - before.top))
        reach.left = max(0, reach.left - max(0, own.left - before.left))
        reach.right = max(0, reach.right - max(0, own.right - before.right))
        reach.bottom = max(0, bottom - max(0, own.bottom - before.bottom))
        if changed { PaintOrder.changed(ownerMount(root)) }
        // Exact's part, on the container's ledger, what an app's hatch set there staying (LLP 1123 D11); a container
        // or root replaced gives its part up under the identity it was written with
        retireBeside(unless: owner, id)
        besideInsets = reach
        (besideOwner, besideIdentity) = (owner, id)
        ContributionsIOS.of(owner).write(id, reach)
        return above
    }

    /// Takes Exact's part beside the routes off the container it was written to, unless that is `owner` under `id`.
    func retireBeside(unless owner: UIViewController? = nil, _ id: ContributionsIOS.Identity? = nil) {
        guard let old = besideOwner, let was = besideIdentity, old !== owner || was != id else { return }
        ContributionsIOS.existing(on: old)?.write(was, .zero)
        (besideOwner, besideIdentity, besideInsets) = (nil, nil, .zero)
    }

    /// The edges of the session's view that `node`, page content outside the routes, reaches where the page did not ask
    /// to cover them: on each edge, the part of its box past the view's safe area, within the view, so its content
    /// clears the status bar, a notch and the indicator while its background stays full-size. A sheet leaves it where
    /// it is (LLP 1123 D1).
    func unsafeEdges(of node: NodeView) -> HostCover.Edges? {
        guard wantsWholeView, presenter.viewportFit != "cover", let view = presenter.session?.view, view.window != nil else { return nil }
        let rect = node.convert(node.bounds, to: view).intersection(view.bounds)
        guard !rect.isNull, !rect.isEmpty else { return nil }
        let safe = view.bounds.inset(by: view.safeAreaInsets)
        let edges = HostCover.Edges(top: max(0, min(rect.height, safe.minY - rect.minY)), right: max(0, min(rect.width, rect.maxX - safe.maxX)),
                                    bottom: max(0, min(rect.height, rect.maxY - safe.maxY)), left: max(0, min(rect.width, safe.minX - rect.minX)))
        return edges == .init(top: 0, right: 0, bottom: 0, left: 0) ? nil : edges
    }

    /// How far a box beside the routes asks the container's top, left and right safe area to reach: on a side of
    /// the routes it lies wholly to, or above them all, up to where CSS lays the routes out, as the web starts their
    /// content and headers there. A route offset from the container's edge with nothing beside it asks nothing:
    /// UIKit's bars span the page with their items at its edges, as a hand-built screen's do around a readable-width
    /// column. Below the routes, a row bounds them only while the tab bar is hidden (`panelSiblings`).
    static func reach(_ box: CGRect, beside routes: [CGRect], in area: CGRect) -> UIEdgeInsets {
        guard let start = routes.map(\.minX).min(), let end = routes.map(\.maxX).max(), let top = routes.map(\.minY).min() else { return .zero }
        return UIEdgeInsets(top: box.maxY <= top + 0.5 ? max(0, top - area.minY) : 0,
                            left: box.maxX <= start + 0.5 ? max(0, start - area.minX) : 0,
                            bottom: 0,
                            right: box.minX >= end - 0.5 ? max(0, area.maxX - end) : 0)
    }

    /// What a route's padding clears of its controller's safe area (`safe`),
    /// its local inset on each edge it executes (LLP 1123 D3): the kernel
    /// adds it to the route's authored padding, or credits a padding that
    /// reads the inset, and turns off an edge the route opts out of (D4).
    /// The top is a collapsing title's scroller's when it has one; the sides
    /// and the bottom only while the containers take the whole view
    /// (`whole`) or the page covers it (`covered`, `viewport-fit="cover"`),
    /// and the bottom only as far as the route reaches it.
    /// What of its controller's safe area (`safe`) a route's box (`box`, within its view's `bounds`) leaves to cover:
    /// a route offset in its block, past a box beside or above it (`clearBesideRoutes`), already clears the part of
    /// each of those edges' insets its box reaches
    static func routeSafe(_ safe: UIEdgeInsets, box: CGRect, in bounds: CGRect) -> UIEdgeInsets {
        UIEdgeInsets(top: max(0, safe.top - max(0, box.minY - bounds.minY)), left: max(0, safe.left - max(0, box.minX - bounds.minX)),
                     bottom: safe.bottom, right: max(0, safe.right - max(0, bounds.maxX - box.maxX)))
    }

    static func routeEdges(safe: UIEdgeInsets, whole: Bool, covered: Bool = false, collapses: Bool, reached: CGFloat) -> HostCover.Edges {
        let edges = whole || covered
        return .init(top: collapses ? 0 : max(0, safe.top),
                     right: edges ? max(0, safe.right) : 0,
                     bottom: edges ? max(0, min(safe.bottom, reached)) : 0,
                     left: edges ? max(0, safe.left) : 0)
    }

    /// What covers `node` but for `kept`, which stays in the page: the node
    /// itself, or, where it holds any of `kept`, each part of it that holds
    /// none (host/painted/src/bars.rs `covered`).
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
        projectSidebarFooter(sidebarFooter())
        #if os(iOS)
        projectTabAccessory()
        #endif
        let above = clearBesideRoutes()
        var changes: [(UInt32, HostCover?)] = []
        var wanted: [UInt32: HostCover] = [:]
        if let hiddenSidebarFooter { wanted[hiddenSidebarFooter.id] = .whole }
        #if os(iOS)
        wanted.merge(dialogBarCovers) { a, _ in a }
        #endif
        var extras: [ObjectIdentifier: ClearanceInset] = [:]
        for c in controllers.values where presenter.views[c.node.id] === c.node {
            // A lifted header takes no room, on a stack yet or not (`coverLifted`).
            for node in c.coveredBarNodes { wanted[node.id] = NavigationHost.cover(of: node, in: c) }
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
            // Nested in a split's list column, the detail's routes are under that column's bar (LLP 1127 §1, item 2)
            let barNav = split.flatMap { $0.nested && nav === $0.detail ? $0.list : nil } ?? nav
            let settled = c.surface.window != nil && (search || (barNav?.isNavigationBarHidden ?? true) != barAbove)
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
                let safe = Self.routeSafe(c.surface.safeAreaInsets, box: c.node.convert(c.node.bounds, to: c.surface), in: c.surface.bounds)
                // The route's end with its page unscrolled: scrolling moves the route, not the bars.
                // A route the keyboard shortens can end above the bars, with no bottom cover.
                let end = c.node.convert(c.node.bounds, to: c.contentMount).maxY + (c.contentViewport?.frame.minY ?? 0)
                let reached = end - (c.surface.bounds.maxY - safe.bottom)
                // A transparent bar covers nothing of the route's top, scroller or not: the content starts under it.
                edges = Self.routeEdges(safe: safe, whole: whole, covered: covered,
                                        collapses: c.collapseScroll != nil || !topPeers.isEmpty || !c.ridingBars.isEmpty || RouteGeometry.insetsNoTop(c.node),
                                        reached: reached)
                // Keep the potential cover for discovery: restoring clearance can pad
                // the route before its bottom scroller is selected again.
                c.bottomClearance = edges?.bottom ?? 0
                if let node = c.collapseScroll, let sv = node.scrollView, !RouteGeometry.insetsNoTop(c.node) {
                    let top = clearanceExtra(c, scroller: node, top: true)
                    extras[ObjectIdentifier(sv), default: ClearanceInset(scroll: sv)].top = top
                }
                if let node = c.edgeScroll ?? c.collapseScroll, let sv = node.scrollView,
                   node === c.edgeScroll || RouteGeometry.clearanceOff(c.node) & 4 != 0 && !RouteGeometry.insetsNoTop(c.node) {
                    let bottom = clearanceExtra(c, scroller: node, top: false)
                    let id = ObjectIdentifier(sv)
                    extras[id, default: ClearanceInset(scroll: sv)].bottom = bottom
                    if node === c.edgeScroll && sv.contentInsetAdjustmentBehavior == .never {
                        extras[id]?.unadjustedBottom = node.safeAreaInsets.bottom
                    }
                }
            } else if case .edges(let e)? = covers[c.node.id] {
                edges = e
            } else { edges = nil }
            // Unsettled, the scrollers keep the clearance they hold, as the route its cover.
            for sv in [c.collapseScrollView, c.edgeScroll?.scrollView].compactMap({ $0 }) where !settled {
                if var held = clearance[ObjectIdentifier(sv)] {
                    if sv.contentInsetAdjustmentBehavior != .never { held.unadjustedBottom = 0 }
                    extras[ObjectIdentifier(sv)] = held
                }
            }
            // A scroller UIKit insets lays its content out between those
            // insets (LLP 1116.000 D1.2): content that fills its height rests
            // under the bar and above the home indicator, and scrolling
            // content still passes under both. A large title's inset is its
            // widest, at rest (CSS scrollTop 0).
            var insetNodes = [c.collapseScroll, c.edgeScroll].compactMap { $0 }
            for peer in topPeers + bottomPeers where !insetNodes.contains(where: { $0 === peer }) { insetNodes.append(peer) }
            for node in insetNodes where wanted[node.id] != .whole && wanted[node.id] != .bar && node.scrollView != nil {
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
                    let noTop = node === c.collapseScroll && RouteGeometry.insetsNoTop(c.node)
                    let top = noTop ? 0 : max(0, max(stretched ? min(safe.top, title) : safe.top, title) - node.pageBand.top)
                    // Only the scroller whose clearance takes back UIKit's inset loses it in layout.
                    let back = min(0, node.scrollView.flatMap { extras[ObjectIdentifier($0)]?.bottom } ?? 0)
                    let bottomOff = RouteGeometry.clearanceOff(c.node) & 4 != 0
                    let bottom = noTop && node !== c.edgeScroll ? 0 : max(0, max(safe.bottom, bottomOff ? 0 : reach) + back - node.pageBand.bottom)
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
            // Clear authored tabs, page content outside the routes, of the unsafe edges they reach
            else if tablistAppearanceNone, list.style["display"]?.string != "none", let edges = unsafeEdges(of: list) {
                wanted[list.id] = .edges(edges)
            }
        }
        // Clear a row above the routes, over the container, of them too: its content would show under the status bar
        for node in above { if let edges = unsafeEdges(of: node) { wanted[node.id] = .edges(edges) } }
        applyClearance(extras)
        #if os(iOS)
        if #available(iOS 27.1, *) {
            // Keep the cover of a pane UIKit hid since the arrangement's last pass, as it owns no edge, until the next pass
            for id in presenter.arrangements.hiddenPanes where presenter.uncovers(id) { wanted[id] = covers[id] }
        }
        wanted.merge(arrangedCovers) { _, whole in whole }
        #endif
        for (id, cover) in wanted where covers[id] != cover { changes.append((id, cover)) }
        for id in covers.keys where wanted[id] == nil && presenter.uncovers(id) { changes.append((id, nil)) }
        covers = wanted
        let locals = reportLocalInsets()
        guard !changes.isEmpty || locals else { return writeCarriedScrollsWhenAnswered() }
        // A scroller's least content height is the port its cover leaves
        // (`scrollFloor`); the kernel sends no new content size when only that changed.
        for (id, _) in changes { presenter.views[id]?.fitScroll() }
        countReport()
        // Under Q3 (c) a cold launch's first frame runs no app code: a hatch
        // that changes a bar the plan already showed moves the content once.
        if replayingHatches, changes.contains(where: { presenter.views[$0.0]?.props["navigationKey"] != nil }) {
            presenter.session?.log("hatch: the content area moved after the first frame, by a hatch run at launch")
        }
        // Never inside a batch: the stack is installed partway through the
        // first one, whose remaining frames would overwrite the covered ones.
        presenter.afterBatch { [weak self] in self?.deliverCovers() }
    }

    #if os(iOS)
    /// The panes a native arrangement hides: each takes no room.
    var arrangedCovers: [UInt32: HostCover] {
        guard #available(iOS 27.1, *) else { return [:] }
        return Dictionary(uniqueKeysWithValues: presenter.arrangements.covered.map { ($0, HostCover.whole) })
    }
    #endif

    /// What the kernel is told is what is wanted when it is told. A report
    /// queued behind a batch ran after later ones that did not wait, and its
    /// covers, measured under the bar's first frame before the status bar's
    /// inset (116 points with a large title of 168), won: team-kanban's
    /// persona switcher lay under the large title, drawn over by it and
    /// untappable (LLP 1116.006 D1).
    func deliverCovers() {
        // A delivery asked for while one runs (an answer applied in it reports again, or the write or the sync that
        // closes a pass does) runs in that one, after the pass
        guard !deliveringCovers else { deliverAgain = true; return }
        // A callback the runtime waits on can make no runtime call: deliver once it returns
        if Owner.shared.isServingMain {
            DispatchQueue.main.async { [weak self] in self?.deliverCovers() }
            return
        }
        deliveringCovers = true
        defer { deliveringCovers = false }
        for _ in 0..<8 {
            deliverAgain = false
            deliverCoversOnce()
            guard deliverAgain else { return }
            if untoldChain?.spent == true { break }
        }
        // What a report past the chain's eighth pass changed stays untold until a report outside the chain tells it
        if untoldChain?.spent == true {
            presenter.session?.log("Navigation covers did not settle in 8 passes; what the last changed is told with the next change no answer causes")
            return
        }
        // A chain a slice landed meanwhile started is told on the main queue's next turn, so one call holds main for
        // eight passes at most and leaves no counted report untold
        DispatchQueue.main.async { [weak self] in self?.deliverCovers() }
    }

    /// Count a report that changed the covers or the local insets, in the chain of the work making it
    /// (`ExactSession.coversCause`), or in a new one. Reports told together are told in the chain with more passes left,
    /// so a change no answer causes keeps its own eight. A report in a spent chain is not counted: no delivery tells it in
    /// that chain, so no carried position waits for its answer (`coversAnswerOwed`).
    func countReport() {
        let chain = presenter.session?.coversCause as? CoversChain ?? CoversChain()
        untoldChain = untoldChain.map { $0.passes <= chain.passes ? $0 : chain } ?? chain
        if !chain.spent { coversReported += 1 }
    }

    /// Deliver the covers and local insets not yet told, in their chain, unless it is spent. Every pass is one of its
    /// chain's eight, and the runtime's answers and the write and the sync that close it run in the chain, so what they
    /// report is told in that chain.
    private func deliverCoversOnce() {
        let views = presenter.views
        let untold = !untoldCovers().isEmpty || localInsets.changed(live: { views[$0] != nil })
        let chain = untoldChain ?? CoversChain(), spent = chain.spent
        // A spent chain keeps what it left untold, so a delivery asked again tells nothing in it
        if untold, spent { presenter.session?.log("Navigation covers did not settle in 8 passes; what the last changed is told with the next change no answer causes") }
        if !untold || !spent { untoldChain = nil }
        if !spent { chain.passes += 1 }
        // The last report this pass answers: every one counted before it sends (a spent chain's untold ones are not)
        let told = coversReported
        let inChain = { (work: () -> Void) in if let session = self.presenter.session { session.inCoversChain(chain, work) } else { work() } }
        inChain {
            // A stack change held for a lifted header (`liftHeld`) is given to
            // UIKit by the sync of the runtime's answer to its cover. The session
            // applies answers in the order asked, a slice in flight first: the
            // hold ends just before this answer, and what it did not sync (an
            // answer that moved nothing) syncs after it. An answer applied here
            // can lift another header, whose own delivery runs once this one ends.
            let release = { [weak self] in
                guard let self else { return }
                liftReleased = max(liftReleased, told)
            }
            let idle = { [weak self] (work: @escaping () -> Void) in
                guard let session = self?.presenter.session else { return work() }
                session.whenIdle(bound: true, work, cancelled: release)
            }
            let held = liftHeld
            if held { idle(release) }
            if untold, !spent {
                presenter.session?.localInsets(localInsets.take { views[$0] != nil })
                // Read the covers once the local insets are told, so covers their answer changes at once, as it does
                // when no work waits, go out in this pass. A report that answer makes stays owed, and the next pass
                // answers it, telling what is still untold, local insets among it, or nothing.
                let changes = untoldCovers()
                delivered = covers
                if !changes.isEmpty { presenter.onCovers?(changes) }
            }
            writeCarriedScrollsWhenAnswered(through: told)
            if held || liftHeld { idle { [weak self] in release(); self?.liftAnswered() } }
        }
    }

    /// The covers the kernel has not been told, and the scrollers still shown that no longer have the one it was told
    private func untoldCovers() -> [(UInt32, HostCover?)] {
        var changes: [(UInt32, HostCover?)] = []
        for (id, cover) in covers where delivered[id] != cover { changes.append((id, cover)) }
        for id in delivered.keys where covers[id] == nil && presenter.uncovers(id) { changes.append((id, nil)) }
        return changes
    }

    /// Write the positions Exact carried across a change of its own (`Presenter.writeCarriedScrolls`) once every report
    /// that changed the covers or local insets is answered, so each lands in the layout it is for. A delivery passes
    /// the last report it told (`report`): the session runs this after the runtime's answer to it, which answers it,
    /// and a reset cancels both, which makes it moot. A call for covers and local insets that changed nothing passes
    /// none and answers nothing. While a report is unanswered a batch's restore leaves the positions
    /// (`coversAnswerOwed`), a fill's among them; inside a batch with none unanswered, the batch's restore writes them.
    func writeCarriedScrollsWhenAnswered(through report: Int? = nil) {
        let answered = { [weak self] in
            guard let self, let report else { return }
            coversAnswered = max(coversAnswered, report)
        }
        let write = { [weak self] in
            answered()
            guard let self, !coversAnswerOwed, !presenter.applying else { return }
            presenter.writeCarriedScrolls()
        }
        // Covers that changed nothing, with no answer owed, have nothing to wait for: written now, before work waiting
        // in the session can scroll the scroller
        guard let session = presenter.session, report != nil || coversAnswerOwed else { return write() }
        session.whenIdle(bound: true, write, cancelled: answered)
    }

    /// The side insets of a route in a sheet still being presented, as they are where the sheet lands
    /// (`UIPresentationController.frameOfPresentedViewInContainerView`). The sheet's own safe area during the
    /// transition is not that: on the iPhone Duo the side by the button strip read 8 points wider.
    static func landingSides(_ c: RouteController) -> (left: CGFloat, right: CGFloat)? {
        var top: UIViewController = c
        while let parent = top.parent { top = parent }
        guard top.isBeingPresented, let sheet = top.presentationController, let container = sheet.containerView else { return nil }
        let landed = sheet.frameOfPresentedViewInContainerView
        let box = c.node.convert(c.node.bounds, to: top.view)
        let safe = container.safeAreaInsets
        let minX = landed.minX + max(0, box.minX), maxX = landed.maxX - max(0, top.view.bounds.maxX - box.maxX)
        return (max(0, safe.left - minX), max(0, safe.right - (container.bounds.maxX - maxX)))
    }

    /// @ref LLP 1123 D9 — each route's own safe area, which its content reads as
    /// `var(--exact-safe-area-inset-*)`: its controller's view's, the bars, the
    /// status bar and the home indicator in it, on the sides only as far as the
    /// route's box reaches them, as its covers are. A route out of a window keeps
    /// the last it had, UIKit's zero there being no measurement, and so does one
    /// while its stack's transition runs, as its cover does, but for its sides
    /// while it appears (`RouteController.appearing`), since UIKit can give a route
    /// coming back in a pop its new sides during the transition, after
    /// `viewIsAppearing`. One that has none yet takes what it has, and an appearing
    /// one its sides, from where a sheet being presented lands (`landingSides`).
    /// And the session root's while the containers take the whole view, where
    /// `env()` is 0 but the screen's insets are not. Whether any changed.
    func reportLocalInsets() -> Bool {
        var wanted: [UInt32?: [CGFloat]] = [:]
        for c in controllers.values where presenter.views[c.node.id] === c.node {
            let last = localInsets.wanted[c.node.id], moving = c.navigationController?.transitionCoordinator != nil
            let view = c.surface
            guard view.window != nil, c.node.window != nil, last == nil || !moving || c.appearing else {
                wanted[c.node.id] = last
                continue
            }
            let box = c.node.convert(c.node.bounds, to: view)
            let safe = view.safeAreaInsets
            var sides = (left: max(0, safe.left - max(0, box.minX)), right: max(0, safe.right - max(0, view.bounds.maxX - box.maxX)))
            if last == nil || c.appearing, let landing = Self.landingSides(c) { sides = landing }
            // An appearing route takes its sides afresh while the transition runs and keeps its top and bottom, which the
            // bars' own transition moves, until it ends
            if let last, moving { wanted[c.node.id] = [last[0], sides.right, last[2], sides.left] }
            else { wanted[c.node.id] = [safe.top, sides.right, safe.bottom, sides.left] }
        }
        if wantsWholeView, let view = presenter.session?.view, view.window != nil {
            // The agent's explicit size is drawn inside the view's safe area
            let safe = ExactView.agentSize == nil ? view.safeAreaInsets : .zero
            wanted[nil] = [safe.top, safe.right, safe.bottom, safe.left]
        }
        defer { localInsets.wanted = wanted }
        return localInsets.wanted != wanted
    }

    // MARK: The development check (LLP 1075.003 §3.5)

    /// The delegate slots Exact keeps (LLP 1075.003 §3.5): every stack's,
    /// hooked or not, More's once adopted, and its own tab container's. A
    /// slot written outside Exact takes its container's calls from Exact, so
    /// the journal names it once each time it leaves Exact: what it holds,
    /// what Exact no longer hears and what to set instead. Checked at each
    /// batch, also while Exact is changing a stack: such a slot keeps Exact
    /// from hearing a transition end.
    func checkDelegates() {
        func say(_ what: String, _ now: AnyObject?, _ effect: String) {
            presenter.session?.log("\(what): delegate changed outside Exact, which owns it (now \(now.map { "\(type(of: $0))" } ?? "nil")); \(effect)")
        }
        // `stacks` holds a record for each slot Exact keeps: `registerStack` fills the slot, `retireStack` retires it
        for stack in stacks.values.sorted(by: { $0.order < $1.order }) {
            guard let nav = stack.controller else { continue }
            let written = !stack.proxy.fillsSlot
            if written, !stack.proxy.slotNamed {
                if nav === adoptedMore {
                    say("navigation More", nav.delegate, "Exact hears none of More's calls, and More has no forwarded delegate")
                } else {
                    say("navigation \(stack.label)", nav.delegate, "Exact hears none of this stack's calls: set ExactNavigation.delegate, which Exact forwards to")
                }
            }
            stack.proxy.slotNamed = written
        }
        guard let tabs = tabController, let proxy = tabProxy else { return }
        let written = !proxy.fillsSlot
        if written, !proxy.slotNamed {
            say("tabs", tabs.delegate, "Exact hears none of the tab container's calls: set ExactTabs.delegate, which Exact forwards to")
        }
        proxy.slotNamed = written
    }

    /// Before each batch: compare what Exact owns on each hatched object and
    /// the tabs of its own tab container with what Exact last wrote or made,
    /// and UIKit's stacks and selection at rest with the router's
    /// (`restDifference`), and journal a difference once, by name. It is
    /// detection, not enforcement; a change made and undone between two
    /// batches is not seen.
    func checkOwned() {
        guard RouteGeometry.checksOwnership else { return }
        checkDelegates()
        checkTabs()
        if let difference = restDifference { reportOnce("navigation: at rest, \(difference)") }
        guard !changing, !syncing, presenter.session?.natives.hatchesConnected == true else { return }
        // `allNavigations` leaves out a tab's own navigation controller while UIKit holds its routes in More; More has
        // no hatch, so the `hatched` guard skips it
        for nav in allNavigations {
            guard let stack = stacks[ObjectIdentifier(nav)], stack.hatched else { continue }
            // Nested, a split's detail column's bar is UIKit's to hide
            if nav.transitionCoordinator == nil, split.map({ !$0.nested || nav !== $0.detail }) ?? true, !searching(nav.topViewController as? RouteController), nav.isNavigationBarHidden == topShowsBar(nav) { say("navigation \(stack.label)", "navigation bar visibility") }
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
            for property in RouteGeometry.ownedChanges(scroll, of: node, insetByUIKit: insetByUIKit(node)) { say("route \(c.key)", property) }
        }
    }

    /// The tabs and their controllers Exact made with its container, hooked
    /// or not (LLP 1075.003 §3.7): Exact never replaces a tab's controller,
    /// and makes a container's `UITab`s again only when an app's
    /// `tabContainer` took their stacks and returned no container
    /// (`restoreBuiltTabs`). They are compared as members, since More's Edit
    /// reorders them as UIKit lets a person do. A container built from
    /// `UITab`s holds them in its `tabs`, and its `viewControllers` reads nil,
    /// so it is compared by its tabs. One built from controllers is compared
    /// by its `viewControllers`, which reads nil once the app sets `tabs` on it.
    private func checkTabs() {
        // Skip during a sync, which can set `builtTabs` before the container's tabs: the next batch checks them
        guard !syncing, let tabs = tabController, tabOwner === tabs else { return }
        // Compare by identity as sets: Exact's are distinct, so equal counts and equal sets are the same members
        func same(_ shown: [AnyObject], _ made: [AnyObject]) -> Bool {
            shown.count == made.count && Set(shown.map(ObjectIdentifier.init)) == Set(made.map(ObjectIdentifier.init))
        }
        if #available(iOS 18.0, tvOS 18.0, *), !builtTabs.isEmpty {
            if !same(tabs.tabs, builtTabs) { say("tabs", "tabs") }
        } else if #available(iOS 18.0, tvOS 18.0, *), tabs.viewControllers == nil {
            // Name `tabs` for a container built from controllers that now holds `UITab`s: a cleared
            // `viewControllers` reads back as no controllers, not nil (measured on iOS 27)
            say("tabs", "tabs")
        } else if !same(tabs.viewControllers ?? [], tabPanels.compactMap { tabStacks[$0] }) {
            say("tabs", "viewControllers")
        }
    }

    /// Journal that `what`'s `property`, which Exact owns, changed outside it, the first time the check finds it.
    private func say(_ what: String, _ property: String) { reportOnce("\(what): \(property) changed outside Exact, which owns it") }

    /// Journal a navigation diagnostic once per host.
    func reportOnce(_ line: String) {
        if reportedDiagnostics.insert(line).inserted { presenter.session?.log(line) }
    }

}

extension RouteController {
    /// A large title at rest opens after its route appears, once UIKit has
    /// given its scroller the bar's insets. The same repair keeps it open
    /// when the text size changes. UIKit draws the title at the new size in
    /// the bar's old height, which grows only when the list is pulled:
    /// at the largest size the title ran under
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

/// A chain of covers corrections: a change no answer causes, and every report that the runtime's answers to its
/// deliveries, and the writes and syncs closing them, make in turn. Covers that never settle stop after its eighth
/// pass, however its answers branch and whether they apply at once or wait behind a slice.
final class CoversChain {
    var passes = 0
    var spent: Bool { passes >= 8 }
}
#endif
