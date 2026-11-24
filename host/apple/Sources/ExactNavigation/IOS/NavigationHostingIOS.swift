// Navigation as the core reaches it (`NavigationLink.swift`): the hosts'
// conformances, and the queries core code asks of routes, which live here
// because they read the family's own types.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

extension RouteController: RouteSurface {
    var headerSearchEditor: UIView? {
        #if os(iOS)
        search?.field?.headerField
        #else
        nil
        #endif
    }
    var searchBarEditing: Bool {
        #if os(iOS)
        search?.controller.searchBar.searchTextField.isEditing == true
        #else
        false
        #endif
    }
    var hosting: (any NavigationHosting)? { host }
    #if os(iOS)
    func bandEdges(top: UIScrollView?, bottom: UIScrollView?) {
        if bandedTop !== top { bandedTop = top; projectContentScrollView(automaticTopScroll, for: .top) }
        if bandedBottom !== bottom { bandedBottom = bottom; projectContentScrollView(automaticBottomScroll, for: .bottom) }
        // A batch that restyled the node can change what its viewport's top follows
        refollowTopEdge()
    }
    #endif
}

extension PlainTab: BootHolding {
    var routeSurface: (any RouteSurface)? { route }
}

extension ModalHost: ModalHosting {}

extension Presenter {
    /// The family's own hosts, for its code: the slots made them.
    var navigationHost: NavigationHost { navigation as! NavigationHost }
    var modalHost: ModalHost { modals as! ModalHost }
}

extension NavigationHost: NavigationHosting {
    #if os(iOS)
    package var lifted: (any LiftedNodes)? { liftedNodes }
    package var barItemBlocks: [UInt32: HostBlock] { barHosted.blocks }
    #else
    package var lifted: (any LiftedNodes)? { siblings }
    package var barItemBlocks: [UInt32: HostBlock] { [:] }
    #endif
    var routeSurfaces: [UInt32: any RouteSurface] { controllers }
    var primaryRouteSurfaces: [any RouteSurface] { primaryRoutes }
    var activeRouteSurface: (any RouteSurface)? { activeRoute }
    var bootHolder: (any BootHolding)? { bootOwner }

    func contentScrollView(of route: any RouteSurface) -> UIScrollView? {
        (route as? RouteController).flatMap { contentScrollView(of: $0) }
    }
    func automaticContentScroller(of route: any RouteSurface) -> NodeView? {
        (route as? RouteController).flatMap { automaticContentScroller(of: $0) }
    }
    func routeShowsBar(_ route: any RouteSurface, in nav: UINavigationController) -> Bool {
        (route as? RouteController).map { routeShowsBar($0, in: nav) } ?? false
    }
    func arrangementPeers(in route: any RouteSurface) -> [NodeView] {
        (route as? RouteController).map { arrangementPeers(in: $0) } ?? []
    }
    func arrangementInsets(_ scroll: UIScrollView, in route: any RouteSurface) -> Bool {
        (route as? RouteController).map { arrangementInsets(scroll, in: $0) } ?? false
    }

    #if EXACT_AGENT
    func searchScopeTabs(of segments: UISegmentedControl) -> [UInt32]? {
        #if os(iOS)
        controllers.values.compactMap(\.search).first(where: { $0.scopeControl === segments })?.scopeTabs
        #else
        nil
        #endif
    }
    #endif

    #if EXACT_AGENT
    func drawnByBar(_ v: NodeView, shown: String) -> String? {
        if tabController != nil, !tablistAppearanceNone, let list = adoptedTablist.flatMap({ presenter.views[$0] }), v === list || v.isDescendant(of: list) {
            return "is in the authored tablist \(shown) UIKit's tab bar"
        }
        for routes in barStackRoutes {
            for case let route as RouteController in routes {
                guard let shape = HeaderShape.bar(of: route.node) else { continue }
                let replaced = shape.authored.isEmpty ? [shape.header] : shape.placed
                if replaced.contains(where: { v === $0 || v.isDescendant(of: $0) }) {
                    return "is in the authored header \(shown) UIKit's navigation bar"
                }
            }
        }
        return nil
    }
    #endif

    /// An authored scroller holds the native container alongside the root's children.
    func ownerMount(_ root: NodeView) -> UIView { root.scroll ?? root }

    /// The primary controller keeps its allocation while route content avoids the keyboard.
    func ownerFrame(_ owner: UIViewController, in root: UIView) -> CGRect {
        let viewport = presenter.viewport
        var frame: CGRect
        if let node = container, node.scroll != nil {
            // Content coordinates stay fixed when the authored scroller moves.
            frame = CGRect(origin: .zero, size: node.bounds.size)
        } else {
            frame = !ExactEnv.authoredChrome && presenter.inDocument(root) ? viewport.convert(viewport.pageBounds, to: root) : root.bounds
        }
        frame.size.height += ownerKeyboardExtension(owner)
        // The authored tablist the agent paints takes its own place in the
        // root, as on the web; the routes end at its edge, not under it.
        if owner === tabOwner, ExactEnv.authoredChrome, let list = adoptedTablist.flatMap({ presenter.views[$0] }),
           list.style["display"]?.string != "none", list.isDescendant(of: root) {
            let rect = list.convert(list.bounds, to: root)
            if rect.height > 0, rect.minY >= frame.midY {
                frame.size.height = max(0, min(frame.maxY, rect.minY) - frame.minY)
            } else if rect.height > 0, rect.maxY <= frame.midY {
                let cut = max(0, rect.maxY - frame.minY)
                frame.origin.y += cut
                frame.size.height = max(0, frame.height - cut)
            }
        }
        return frame
    }

    func ownerKeyboardExtension(_ owner: UIViewController) -> CGFloat {
        #if os(iOS)
        if ExactEnv.authoredChrome, owner === primaryOwner, presenter.keyboardPolicy == "resizes-content" { return presenter.keyboardAccessoryHeight }
        #endif
        return 0
    }

    /// Native selection wins over `navigationScroll`, then the first vertical scroller
    /// (LLP 1115.000 D11.1). The bar, scroll-to-top actions and route hatch share this rule.
    func contentScroller(of c: RouteController) -> NodeView? {
        c.nativeContentScrollEdges.contains(.top) ? c.nativeContentScroller : automaticContentScroller(of: c)
    }

    /// The content scroller with no native selection: the one `navigationScroll` names (none when
    /// no scrolling element carries the name), else the first vertical scroller.
    func automaticContentScroller(of c: RouteController) -> NodeView? {
        if let name = c.node.props["navigationScroll"], !name.isEmpty {
            return element(named: name, in: c.node).flatMap { $0.scrollView == nil ? nil : $0 }
        }
        return Presenter.firstVerticalScroller(under: c.node)
    }

    /// The native selection, else the automatic content scroller's scroll view, else, with no
    /// `navigationScroll`, the route's own viewport where it has one.
    func contentScrollView(of route: RouteController) -> UIScrollView? {
        if route.nativeContentScrollEdges.contains(.top) { return route.contentScrollView(for: .top) }
        if let node = automaticContentScroller(of: route) { return shownScroll(node) }
        if let name = route.node.props["navigationScroll"], !name.isEmpty { return nil }
        return route.contentViewport
    }

    /// Follow neutral wrappers only while the named scroller remains the first content at their top
    static func topScroller(_ start: NodeView, target: NodeView?) -> NodeView? {
        guard let target else { return nil }
        var node = start
        while node !== target {
            guard node.scrollView == nil, !node.bounds.isEmpty, node.transform.isIdentity,
                  CATransform3DIsIdentity(node.layer.transform),
                  !node.overflowClips, node.clipPath == nil,
                  node.contentBox().minY <= 0.5 else { return nil }
            let children = node.container.subviews.compactMap { $0 as? NodeView }.filter(flowingTopContent)
            guard let first = children.first, abs(first.frame.minY - node.contentBox().minY) < 1,
                  children.allSatisfy({ $0 === first || $0.frame.minY >= first.frame.maxY - 0.5 }) else { return nil }
            node = first
        }
        return target.scrollView == nil ? nil : target
    }

    static func flowingTopContent(_ node: NodeView) -> Bool {
        (!node.isHidden || node.presenter?.menus.isLaidOut(node) == true) && RouteGeometry.inFlow(node)
    }

    func arrangementScrollers(in route: RouteController, top: Bool) -> [NodeView] {
        #if os(iOS)
        if #available(iOS 27.1, *), !RouteGeometry.insetsNoTop(route.node) { return presenter.arrangements.scrollers(in: route.node, top: top) }
        #endif
        return []
    }

    func arrangementInsets(_ scroll: UIScrollView, in route: RouteController) -> Bool {
        arrangementPeers(in: route).contains { $0.scrollView === scroll }
    }

    func arrangementPeers(in route: RouteController) -> [NodeView] {
        arrangementScrollers(in: route, top: true) + arrangementScrollers(in: route, top: false)
    }

    /// The navigation bar item a shown route's bar draws for `node`, and
    /// the route it is in; nil when no shown bar stands for it, or something
    /// is presented over its route.
    func tipItem(_ node: NodeView) -> (UIBarButtonItem, UIViewController)? {
        for c in controllers.values {
            guard let nav = c.navigationController, nav.topViewController === c, barShows(nav), c.viewIfLoaded?.window != nil,
                  c.isTopForTips, let item = c.barItemsMade[node.id]?.bar else { continue }
            let shown = (c.navigationItem.leftBarButtonItems ?? []) + (c.navigationItem.rightBarButtonItems ?? [])
            if shown.contains(where: { $0 === item }) { return (item, c) }
        }
        #if os(iOS)
        // A dialog's sheet bar, while nothing is presented over its sheet.
        for bar in shownDialogBars {
            guard let controller = bar.controller, controller.presentedViewController == nil, let item = bar.barItemsMade[node.id]?.bar else { continue }
            let shown = (controller.navigationItem.leftBarButtonItems ?? []) + (controller.navigationItem.rightBarButtonItems ?? [])
            if shown.contains(where: { $0 === item }) { return (item, controller) }
        }
        #endif
        return nil
    }

    #if os(iOS)
    /// The bar button item made for an authored header node, in the bar of the route that shows it.
    func barButtonItem(_ node: NodeView) -> (bar: UINavigationBar, item: UIBarButtonItem, of: UINavigationItem)? {
        guard !ExactEnv.authoredChrome else { return nil }
        let testId = node.props["testId"]
        for route in controllers.values {
            guard let controller = route.navigationController, controller.topViewController === route, barShows(controller),
                  route.viewIfLoaded?.window != nil else { continue }
            let items = (route.navigationItem.leftBarButtonItems ?? []) + (route.navigationItem.rightBarButtonItems ?? [])
            guard let item = items.first(where: { ($0.target as? BarPress)?.id == node.id })
                    ?? items.first(where: { $0.target == nil && testId != nil && $0.accessibilityIdentifier == testId }) else { continue }
            return (shownBar(of: controller), item, route.navigationItem)
        }
        // A dialog's sheet bar (DialogBarIOS.swift).
        for dialog in shownDialogBars {
            guard let controller = dialog.controller, let nav = dialog.navigationController else { continue }
            let items = (controller.navigationItem.leftBarButtonItems ?? []) + (controller.navigationItem.rightBarButtonItems ?? [])
            if let item = items.first(where: { ($0.target as? BarPress)?.id == node.id }) { return (nav.navigationBar, item, controller.navigationItem) }
        }
        return nil
    }
    #elseif EXACT_AGENT
    func barButtonItem(_ node: NodeView) -> (bar: UINavigationBar, item: UIBarButtonItem, of: UINavigationItem)? { nil }
    #endif

    /// The navigation bar and, in it, the frame UIKit presents from for an
    /// authored header node's item: the bar's overflow button for an item in
    /// its overflow menu.
    func barItem(_ node: NodeView) -> Result<(bar: UIView, item: CGRect), BarItemMissing>? {
        #if os(iOS)
        guard let (bar, item, _) = barButtonItem(node) else { return nil }
        guard let frame = item.frame(in: bar), frame.width > 0, frame.height > 0 else { return .failure(BarItemMissing()) }
        return .success((bar, frame))
        #else
        return nil
        #endif
    }

    #if EXACT_AGENT
    /// The search field a shown route projects into UIKit's search bar.
    func drawnSearch(_ node: NodeView) -> UITextField? {
        #if os(iOS)
        guard !ExactEnv.authoredChrome else { return nil }
        for c in controllers.values {
            guard let nav = c.navigationController, nav.topViewController === c, barShows(nav),
                  c.viewIfLoaded?.window != nil, let search = c.search, search.field === node else { continue }
            return search.controller.searchBar.searchTextField
        }
        #endif
        return nil
    }
    #endif
}

#if os(iOS)
extension RouteController {
    func scrollViewShouldScrollToTop(_ scrollView: UIScrollView) -> Bool {
        host?.presenter.elementScrolls.userScrolled(scrollView)
        return true
    }
}
#endif
#endif

#if EXACT_INSPECT && os(iOS)
extension NavigationHost {
    /// Where a route's bar shows `node`: its bar button, title segment, search field or title.
    func inspectPlace(of node: NodeView) -> (() -> CGRect?)? {
        let id = node.id
        for c in controllers.values {
            guard let nav = c.navigationController, nav.topViewController === c else { continue }
            let bar = nav.navigationBar
            let items = (c.navigationItem.leftBarButtonItems ?? []) + (c.navigationItem.rightBarButtonItems ?? [])
            if let item = items.first(where: { c.itemNodes[ObjectIdentifier($0)] == id }) {
                return { [weak bar, weak item] in
                    guard let bar, let item, bar.window != nil, let frame = item.frame(in: bar) else { return nil }
                    return bar.convert(frame, to: nil)
                }
            }
            if let segments = c.titleSegments, let index = segments.press.tabs.firstIndex(of: id) {
                return { [weak control = segments.control] in
                    guard let control, control.window != nil else { return nil }
                    let views = control.segmentViews
                    return views.indices.contains(index) ? views[index].convert(views[index].bounds, to: nil) : nil
                }
            }
            if let search = c.search, search.field === node {
                return { [weak searchBar = search.controller.searchBar] in searchBar.flatMap { $0.window != nil ? $0.searchTextField.convert($0.searchTextField.bounds, to: nil) : nil } }
            }
            if let header = c.shownHeader, header === node || Self.heading(in: header) === node || (c.navigationItem.titleView as? HeaderTitleView)?.groupId == id {
                return { [weak bar, weak c] in
                    guard let bar, let c, bar.window != nil else { return nil }
                    if let title = c.navigationItem.titleView, title.window != nil { return title.convert(title.bounds, to: nil) }
                    return bar.titleFrame(c.navigationItem.title).map { bar.convert($0, to: nil) }
                }
            }
        }
        return nil
    }

    /// The plan node behind a navigation bar's item or title at `p` (in the
    /// bar's coordinates), or nil for UIKit's own parts (Back, the bar).
    func inspectNode(in bar: UINavigationBar, at p: CGPoint) -> UInt32? {
        for c in controllers.values where c.navigationController?.navigationBar === bar && c.navigationController?.topViewController === c {
            let items = (c.navigationItem.leftBarButtonItems ?? []) + (c.navigationItem.rightBarButtonItems ?? [])
            for item in items {
                guard let frame = item.frame(in: bar), frame.contains(p) else { continue }
                if let id = c.itemNodes[ObjectIdentifier(item)] ?? (item.target as? BarPress)?.id { return id }
            }
            // The header's tablist as the title's segments: the tab under the point.
            if let segments = c.titleSegments, segments.control.window != nil, segments.control.convert(segments.control.bounds, to: bar).contains(p) {
                let views = segments.control.segmentViews, tabs = segments.press.tabs
                if views.count == tabs.count, let index = views.firstIndex(where: { $0.convert($0.bounds, to: bar).contains(p) }) { return tabs[index] }
            }
            // The header's search input as the bar's search field.
            if let search = c.search, let field = search.field {
                let searchBar = search.controller.searchBar
                if searchBar.window != nil, searchBar.convert(searchBar.bounds, to: bar).contains(p) { return field.id }
            }
            // The title UIKit draws (the label showing this route's title, not
            // Back's): the route header's heading.
            if let header = c.shownHeader, let title = bar.titleFrame(c.navigationItem.title), title.contains(p) { return Self.heading(in: header)?.id ?? header.id }
        }
        return nil
    }

    /// The heading in a route's header, the title the bar shows for it.
    private static func heading(in header: NodeView) -> NodeView? {
        var pending: [NodeView] = [header]
        while !pending.isEmpty {
            let node = pending.removeFirst()
            if node.props["accessibilityRole"] == "heading" { return node }
            pending += node.container.subviews.compactMap { $0 as? NodeView }
        }
        return nil
    }
}
#endif

#if EXACT_INSPECT && os(iOS)
extension NavigationHost {
    func inspectTitle(_ view: UIView) -> UInt32? { (view as? HeaderTitleView)?.groupId }
}
#endif

#if EXACT_AGENT && (os(iOS) || os(tvOS))
extension NavigationHost {
    func submitSearch(_ editor: UITextField) -> Bool {
        #if os(iOS)
        guard let search = editor.delegate as? HeaderSearch else { return false }
        let bar = search.controller.searchBar
        bar.delegate?.searchBarSearchButtonClicked?(bar)
        return true
        #else
        return false
        #endif
    }
}
#endif
