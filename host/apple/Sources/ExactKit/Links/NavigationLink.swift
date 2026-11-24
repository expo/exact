// @ref LLP 1047.001 D10 — navigation (stacks, bars, titles, tabs, search,
// covers) with its route modals (sheets, full-screen covers, detents) is a
// family the composition selects. The core reaches it only through what this
// file declares: the presenter holds `any NavigationHosting` and
// `any ModalHosting`, and core code that meets a route or a plain tab casts to
// `RouteSurface` or `BootHolding`, never to the family's classes. The
// `ExactNavigation` product fills the slots when the composition links it.
//
// Without the family the presenter holds `NoNavigation` and `NoModals`. A plan
// without `navigationBack` never builds a stack, but the host still did core
// work on its lifecycle, which the null object keeps at the same points:
// `prepare` checks what hatches own; every `sync` (window attach, each batch)
// clears the list rows' dependencies, resets (scroll-to-top selection,
// `owners.publish()`), then flushes pending focus and element scrolls; focus
// waits between window attach and `mounted()`.
#if os(iOS) || os(tvOS)
import UIKit

/// A route's controller as the core sees it: its node and the views it draws in.
package protocol RouteSurface: UIViewController {
    var node: NodeView { get }
    /// Where the route draws: its boot surface while it has one, else its view.
    var surface: UIView { get }
    var bootSurface: UIView? { get }
    var contentViewport: Viewport? { get }
    /// The scrollers the bar's collapse and the bottom edge follow.
    var collapseScroll: NodeView? { get }
    var edgeScroll: NodeView? { get }
    /// A node lifted out of the content to float over the route.
    var lifted: NodeView? { get }
    /// Control bars that ride the route's bottom edge.
    var ridingBars: [NodeView] { get }
    /// The text field of the search the route's bar shows, while it shows one.
    var headerSearchEditor: UIView? { get }
    /// Whether UIKit's search field in the route's bar is editing.
    var searchBarEditing: Bool { get }
    /// The stack that holds the route (a navigation controller, a plain tab or the boot owner); nil once popped.
    var stack: UIViewController? { get }
    /// The navigation this route belongs to.
    var hosting: (any NavigationHosting)? { get }
    /// Fit the content viewport to the route's block (the part its bars leave).
    func fitContentViewport(in block: CGRect?)
    #if os(iOS)
    /// Point the bar's and the bottom bar's edge effects at the scrollers banded under them.
    func bandEdges(top: UIScrollView?, bottom: UIScrollView?)
    #endif
}

/// A plain tab: a controller holding one route, shown before or instead of a
/// navigation controller.
package protocol BootHolding: UIViewController {
    /// Whether it holds the boot's content before the stacks are made.
    var bootContent: Bool { get }
    /// The route it shows.
    var routeSurface: (any RouteSurface)? { get }
}

/// A sheet controller that paints its own surface (a route sheet's or a
/// dialog's), which the theme and the view repaint when the appearance changes.
package protocol PaintsSheetSurface: AnyObject { func paintSurface() }

extension UIViewController {
    /// The routes of a stack Exact built, its root first: a navigation
    /// controller's, after what UIKit keeps before them (`stackFront`), or
    /// the one a plain tab shows.
    package var stackRoutes: [UIViewController] {
        if let nav = self as? UINavigationController { return Array(nav.viewControllers.drop { !($0 is RouteSurface) }) }
        return ((self as? BootHolding)?.routeSurface).map { [$0] } ?? []
    }

    /// What UIKit keeps before a navigation controller's routes in
    /// `viewControllers`: More's list, and a plain tab More pushed over it
    /// (iOS). Every write of the routes keeps it.
    package var stackFront: [UIViewController] {
        (self as? UINavigationController).map { Array($0.viewControllers.prefix { !($0 is RouteSurface) }) } ?? []
    }
}

/// How routes and their content are laid out, which the core's arrangement,
/// control bars, scrolling and hatch checks read without navigation.
package enum RouteGeometry {
    /// The edges, as bits top 1, right 2, bottom 4 and left 8, whose
    /// clearance `route` turns off (`-exact-safe-area`).
    package static func clearanceOff(_ route: NodeView) -> Int { Int(route.style["safe_area"]?.number ?? 0) & 0xf }

    /// Whether a route asks `navigationBar="transparent"`: its bar shows
    /// with a clear scroll-edge appearance and insets no content.
    package static func transparentBar(_ route: NodeView) -> Bool { route.props["navigationBar"] == "transparent" }

    /// Whether UIKit insets nothing of `route`'s top scroller: under a
    /// transparent bar, or where the route opts out of its top clearance.
    package static func insetsNoTop(_ route: NodeView) -> Bool { transparentBar(route) || clearanceOff(route) & 1 != 0 }

    /// Whether `v` takes space in its route's flow: not hidden, not laid out
    /// absolute or fixed (the kernel lays an open popover out of flow, as
    /// HTML's top layer is, and a closed one not at all), and not a closed
    /// `dialog`, which the host leaves out of layout (`MenuHost.layOut`), as a
    /// browser's `dialog:not([open])` is `display: none`. So a route's menus and
    /// confirmations, written between its header and its scroller or after
    /// the scroller, leave the scroller its route's top and bottom edges. A
    /// dialog given back to layout to show in Exact's views counts as laid out.
    package static func inFlow(_ v: NodeView) -> Bool {
        // A popover lays out absolute (`layoutPosition`), unless sticky; a
        // sticky one is in flow while laid out, as a dialog is (`MenuHost.layOut`).
        let dialog = v.props["semanticTag"] == "dialog" || v.props["popover"] != nil
        return v.style["display"]?.string != "none" && !["absolute", "fixed"].contains(v.layoutPosition)
            && (!dialog || v.presenter?.menus.isLaidOut(v) == true)
    }

    /// A bar visible in its window, including every ancestor's visibility.
    package static func shownBar(_ bar: UIView?) -> UIView? {
        guard let bar, let window = bar.window else { return nil }
        var at: UIView? = bar
        while let view = at {
            if view.isHidden || view.alpha < 0.01 { return nil }
            at = view.superview
        }
        let frame = bar.convert(bar.bounds, to: window)
        return frame.width > 1 && frame.height > 1 && frame.intersects(window.bounds) ? bar : nil
    }

    /// What the agent says of a header node the bar leaves out.
    package static let unshownReason = "the iOS navigation bar does not show this node; in the app it is not visible (the bar shows a header's heading, the first text after it as its subtitle, one search field, one tablist and its buttons)"

    /// Whether this build checks what Exact owns: every build but a
    /// production bake.
    package static let checksOwnership = !ExactEnv.productionBake

    /// What Exact sets on a node's scroll view and nothing else may (LLP
    /// 1075.003 §3.5) and differs from what Exact writes: no inset (a
    /// refresh control insets it while it spins), automatic adjustment for a
    /// route's collapse or bottom-edge scroller and arranged peers, the backend's
    /// Exact-owned delegate, the node's keyboard dismissal (authored, or
    /// the platform's for what it scrolls, LLP 1115.000 D9.1). The offset
    /// and the content size are Exact's too but are not checked: they move
    /// with the user and with layout, so no last write predicts them.
    package static func ownedChanges(_ s: UIScrollView, of node: NodeView, insetByUIKit: Bool) -> [String] {
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

extension UITabBarController {
    /// Its tabs' controllers in tab order, whether it holds them as
    /// `viewControllers` or as `UITab`s, whose `viewControllers` is nil.
    package var tabControllers: [UIViewController]? {
        if let viewControllers { return viewControllers }
        guard #available(iOS 18.0, tvOS 18.0, *), !tabs.isEmpty else { return nil }
        return tabs.compactMap(\.viewController)
    }

    /// The bar the controller projects, including iPadOS's floating list of tab items.
    package var projectedTabBar: UIView? {
        if #available(iOS 18.0, tvOS 18.0, *), isTabBarHidden { return nil }
        #if os(iOS)
        return tabBar.window != nil ? tabBar : floatingTabList
        #else
        return tabBar
        #endif
    }

    /// The projected bar, when it is visibly on screen.
    package var shownTabBar: UIView? { RouteGeometry.shownBar(projectedTabBar) }
}

#if os(iOS)
extension UITabBarController {
    /// iPadOS 18's floating tab bar, in a regular width: no `UITabBar` in the
    /// window (`tabBar` is out of it), but a list of the tabs' items at the
    /// top, a collection view whose first section has a cell per tab, in
    /// their order, and whose others are empty (iPadOS 27 keeps a second,
    /// for tabs a person adds), beside the selected tab's view. Its cells
    /// carry their items' accessibility identifiers (the tabs' test ids,
    /// `syncItems`), or their titles as labels, as the agent's aim reads
    /// them: a list of the same shape whose cells name other things (a
    /// hatch's) is not it. A sidebar's list of tabs (`mode = .tabSidebar`,
    /// TabsIOS.swift) has the same shape, and while it shows the floating
    /// bar does not. Nil where the bar is UIKit's classic one, where no such
    /// list shows, or where two would.
    package var floatingTabList: UICollectionView? {
        guard tabBar.window == nil, let controllers = tabControllers, case let items = controllers.compactMap(\.tabBarItem), !items.isEmpty, items.count == controllers.count,
              let selected = selectedViewController?.viewIfLoaded else { return nil }
        func names(_ cell: UIView, _ item: UITabBarItem) -> Bool {
            let id = item.accessibilityIdentifier ?? ""
            if id.isEmpty, (item.title ?? "").isEmpty { return false }
            var pending = [cell]
            while !pending.isEmpty {
                let v = pending.removeFirst()
                if id.isEmpty ? v.accessibilityLabel == item.title : v.accessibilityIdentifier == id { return true }
                pending += v.subviews
            }
            return false
        }
        var found: [UICollectionView] = [], pending: [UIView] = view.subviews
        while !pending.isEmpty {
            let v = pending.removeFirst()
            guard v !== selected, !v.isHidden, !(v is NodeView) else { continue }
            if let list = v as? UICollectionView, list.numberOfSections >= 1, list.numberOfItems(inSection: 0) == items.count,
               (1..<list.numberOfSections).allSatisfy({ list.numberOfItems(inSection: $0) == 0 }) {
                let shown = list.indexPathsForVisibleItems.filter { $0.section == 0 }
                if !shown.isEmpty, shown.allSatisfy({ path in list.cellForItem(at: path).map { names($0, items[path.item]) } ?? false }) { found.append(list) }
                continue
            }
            pending += v.subviews
        }
        return found.count == 1 ? found[0] : nil
    }
}
#endif

/// What the core asks of a presenter's navigation. Call sites keep their
/// places, so the hook order is the presenter's source order: in `apply`,
/// `modals.prepare`, then `prepare`, the op loop with `installInitialOwner`
/// at the first frame or content op, controls and menus, then
/// `syncOwed`/`batchApplied`, the arrangements and keyboard toolbars, and
/// scroll-to-top; `resolveStatusBar` and the final focus flush close it.
package protocol NavigationHosting: AnyObject {
    // Lifecycle.
    func prepare(_ batch: Batch)
    func installInitialOwner(_ batch: Batch)
    var syncOwed: Bool { get }
    func batchApplied(_ batch: Batch, rowsOnly: Bool)
    func reset(clearFocus: Bool)
    func rebooting()
    func retheme()
    func appearanceChanged()
    func willMount()
    func mounted()
    func unmounted()
    func replayHatches()

    // Routes and containment.
    var container: NodeView? { get }
    var routeSurfaces: [UInt32: any RouteSurface] { get }
    var primaryRouteSurfaces: [any RouteSurface] { get }
    var activeRouteSurface: (any RouteSurface)? { get }
    var activeKey: String? { get }
    var primaryOwner: UIViewController? { get }
    var bootHolder: (any BootHolding)? { get }
    var primaryStacks: [UIViewController] { get }
    var chromeOwners: [UIViewController] { get }
    var tabController: UITabBarController? { get }
    var tabPanels: [UInt32] { get }
    func ownsContainment(of node: NodeView, under parent: NodeView) -> Bool
    func placeOwner()
    func retainsRemovedView(_ node: NodeView) -> Bool
    func logicalParent(of route: NodeView) -> NodeView?
    func routeKey(containing view: UIView, layout: Bool) -> String?
    func isInactiveRoute(containing view: UIView, layout: Bool) -> Bool
    func resolve(route key: String, id: String) -> NodeView?
    /// Each route shown beside its stack's top or under a pane over it, by
    /// its node (LLP 1127 D8): a split's columns. The others show in their stack.
    var routesShown: [UInt32: BlockShown] { get }
    /// The blocks of the nodes UIKit holds in views of their own (the sidebar's footer, the tab bar's accessory).
    var paneBlocks: [(id: UInt32, block: HostBlock, placement: BlockPlacement?)] { get }
    /// The boxes UIKit draws above the routes without presenting them (the tab bar's accessory), which the runner
    /// tests before the page, as it does a presentation's.
    var topLayerBoxes: [TopLayerBox] { get }
    #if os(iOS)
    /// The views that hold an element out of its parent (the tab bar's accessory), with the element: a menu an
    /// ancestor of it gives reaches a long press there, as one reaches a presentation's mount.
    var relayMounts: [(mount: any RelayMount, node: NodeView)] { get }
    #endif
    /// The blocks the bars' hosted items are laid out in, by element (LLP 1141.004 D3).
    var barItemBlocks: [UInt32: HostBlock] { get }
    /// Whether a grouped list shows as UIKit's sidebar: in a split's list column while the split is expanded (LLP 1127 D7).
    func showsAsSidebar(_ list: UIView) -> Bool
    /// The row of a sidebar list whose detail shows, which stays selected;
    /// `.some(nil)` for none, and nil while the runner cannot say.
    func sidebarSelection(_ list: UIView) -> UInt32??
    /// A grouped list's `row`, in `list`, pressed by `press`: in a split's
    /// list column, a detail its batches bring is the row's (LLP 1127 D3).
    func listPress(_ row: UInt32, in list: UIView, _ press: () -> Void)

    // Focus and transitions: separate answers, each read where it is read today.
    var defersFocus: Bool { get }
    var inTransition: Bool { get }
    var changing: Bool { get }
    var transitioning: Bool { get }
    var hasInteractiveTransition: Bool { get }
    var unanimated: Bool { get set }
    /// A batch inside `startViewTransition()`'s cross-fade: its pushes, pops and presentations land unanimated.
    var crossFading: Bool { get set }
    /// Whether `controller` presents a route that is closed and yet to be dismissed.
    func retires(_ controller: UIViewController) -> Bool
    func isArriving(_ node: NodeView) -> Bool

    // The boot owner: `bootOwnerDrawn` answers true only when it holds `ready`.
    func finishBootOwner()
    func bootOwnerDrawn(_ ready: @escaping () -> Void) -> Bool
    func afterBootOwner(_ work: @escaping () -> Void)

    // Covers, insets and fitting.
    var covers: [UInt32: HostCover] { get }
    var coversAnswerOwed: Bool { get }
    var wantsWholeView: Bool { get }
    func coversChanged()
    func reportCovers()
    func fitOwner()
    func insetByUIKit(_ node: NodeView) -> Bool

    // Scrolling.
    var scrollsToTopSelection: UIScrollView? { get set }
    func shownScroll(_ node: NodeView?) -> UIScrollView?
    func scrollBackendChanged(_ node: NodeView)
    func contentScrollView(of route: any RouteSurface) -> UIScrollView?
    func automaticContentScroller(of route: any RouteSurface) -> NodeView?
    func routeShowsBar(_ route: any RouteSurface, in nav: UINavigationController) -> Bool

    // Bars, tabs and search.
    func hiddenForBar(_ view: UIView) -> Bool
    /// A picture a bar face draws arrived: its stack's bars are projected again.
    func barPictureArrived(_ node: NodeView)
    func unshownByBar() -> [UInt32: String]
    func shownBarItem(_ id: UInt32) -> UIBarButtonItem?
    func barItem(_ node: NodeView) -> Result<(bar: UIView, item: CGRect), BarItemMissing>?
    func tipItem(_ node: NodeView) -> (UIBarButtonItem, UIViewController)?
    #if os(iOS)
    /// Whether `dialog`'s sheet would show its header as its bar now: its
    /// sheet is then presented in a navigation controller.
    func wantsDialogBar(_ dialog: NodeView) -> Bool
    /// Project `dialog`'s header onto `controller`'s navigation item, the root
    /// of the navigation controller its sheet presents (LLP 1141.004 D6);
    /// false where the header stays in the page, with the bar hidden.
    func projectDialogBar(_ dialog: NodeView, on controller: UIViewController) -> Bool
    /// `dialog`'s sheet has gone: its header shows in the page again.
    func endDialogBar(_ dialog: NodeView)
    #endif
    #if EXACT_INSPECT
    func barSegment(_ tab: NodeView) -> (control: UIControl, segment: CGRect)?
    #endif
    func adopts(tablist: NodeView) -> Bool
    func scopes(tablist: NodeView) -> Bool
    func tabIntent(_ kind: UInt32, _ text: String) -> Bool
    func setAppDelegate(_ controller: AnyObject, _ delegate: AnyObject?)
    /// The nodes it lifts out of their parents to paint over its tab container.
    var lifted: (any LiftedNodes)? { get }
    #if os(tvOS)
    /// The Siri remote's Menu goes back in a pushed stack.
    var menuGoesBack: Bool { get }
    func menuBack()
    #endif

    #if EXACT_INSPECT && os(iOS)
    /// The header group a bar's title view stands for, when `view` is one.
    func inspectTitle(_ view: UIView) -> UInt32?
    /// Where a route's bar shows `node`, for Inspect's outline.
    func inspectPlace(of node: NodeView) -> (() -> CGRect?)?
    /// The plan node behind a navigation bar's item or title at `p`, in the bar's coordinates.
    func inspectNode(in bar: UINavigationBar, at p: CGPoint) -> UInt32?
    #endif
    #if EXACT_AGENT
    var syncScheduled: Bool { get }
    /// Every stack Exact built: the tabs' in tab order (or the one), then the sheets'.
    var allNavigations: [UINavigationController] { get }
    /// Whether UIKit's tab bar draws the tablist (not authored chrome, not `appearance: none`).
    var tabBarShows: Bool { get }
    var retiringRoots: [NodeView] { get }
    /// Why a finger cannot aim at `v`: the platform's tab bar or navigation
    /// bar draws it in its place, saying so after `shown`. Nil otherwise.
    func drawnByBar(_ v: NodeView, shown: String) -> String?
    func paintedForBars() -> [NodeView]
    func barItem(ofTab index: Int) -> UITabBarItem?
    func tabBarStands(for node: NodeView) -> Bool
    func observation() -> [String: Any]
    /// The bar's search field submitting, as the keyboard's Search key does; false when `editor` is no bar's search field.
    func submitSearch(_ editor: UITextField) -> Bool
    func agentBack() -> String?
    /// A list beside its detail as UIKit's split shows it, for `state window.split` (LLP 1127 D11).
    var splitObservation: [String: Any] { get }
    func activateChrome(_ node: NodeView, before: (CGPoint?) -> Bool) -> [String: Any]?
    func drawnSearch(_ node: NodeView) -> UITextField?
    func barButtonItem(_ node: NodeView) -> (bar: UINavigationBar, item: UIBarButtonItem, of: UINavigationItem)?
    /// The tabs a route's search scope control shows, by segment; nil when
    /// `segments` is no route's scope control.
    func searchScopeTabs(of segments: UISegmentedControl) -> [UInt32]?
    #endif
}

/// The nodes a navigation lifts out of their parents to paint over its tab
/// container (LLP 1083.000 D4), as the core's walks and passes meet them.
package protocol LiftedNodes: AnyObject {
    /// The parent a lifted node, or a view in its place, is under in the authored tree.
    func parent(of view: UIView) -> NodeView?
    /// The release bounds for a press that bubbles out of a lifted view.
    func pressBox(crossing view: UIView, from start: UIView) -> UIView?
    /// Whether `parent` lifted `node`, which `parent`'s children op then leaves where it is.
    func holds(_ node: NodeView, under parent: NodeView) -> Bool
    /// A children op on `parent`, before the presenter places it.
    func children(_ parent: NodeView, _ ids: [UInt32], _ want: [NodeView])
    /// The nodes `parent` lifted.
    func lifted(from parent: NodeView) -> [NodeView]
    /// The ids `parent` was last given, where it lifted nodes or holds panels.
    func recordedIDs(of parent: NodeView) -> [UInt32]?
    /// The lifted nodes at their parents' places, at the end of a batch.
    func place()
    /// `view`'s children in Tab's order: a lifted node on its side of the routes, as in the tree.
    func focusChildren(of view: UIView) -> [UIView]
    /// The mount's accessibility children while a lifted node reads before the routes.
    func accessibilityElements(of view: UIView) -> [Any]?
    /// The lifted nodes positioned `absolute` among `parent`'s children after `view`.
    func absoluteNodes(after view: UIView, in parent: UIView) -> [NodeView]
}

/// A view that holds lifted nodes, which the focus search skips.
package protocol HoldsLiftedNodes: UIView {}

/// The view that holds a readout the navigation bar hosts (LLP 1141.004 D3).
package protocol HoldsBarReadout: HoldsLiftedNodes {}

extension NodeView {
    /// The readout a navigation bar hosts that holds this node, itself
    /// included; nil outside one.
    package var barReadout: NodeView? { authoredLine.first { $0.superview is HoldsBarReadout } }
}

extension Presenter {
    /// The view above `view` in a pass over the layout: a lifted node's
    /// parent (LLP 1083.000 D4), else its superview.
    package func layoutParent(of view: UIView) -> UIView? { navigation.lifted?.parent(of: view) ?? view.superview }
}

extension NavigationHosting {
    package func reset() { reset(clearFocus: true) }
    package func routeKey(containing view: UIView) -> String? { routeKey(containing: view, layout: false) }
    package func isInactiveRoute(containing view: UIView) -> Bool { isInactiveRoute(containing: view, layout: false) }
    #if EXACT_AGENT
    package func activateChrome(_ node: NodeView) -> [String: Any]? { activateChrome(node, before: { _ in true }) }
    #endif
}

/// What the core asks of a presenter's route modals (sheets, covers, detents).
package protocol ModalHosting: AnyObject {
    func prepare(_ batch: Batch)
    func inheritFromAncestors()
    func retainsRemovedView(_ node: NodeView) -> Bool
    func contentChanged(_ node: NodeView)
    func inertBehind(_ view: UIView) -> Bool
    func mounts(_ view: UIView) -> Bool
    func surfaces(includingRetiring: Bool) -> [SessionSurface]
    func reset()
    func unmounted()
    #if os(iOS)
    func statusBarChanged()
    #endif
    var defersFocus: Bool { get }
    var inTransition: Bool { get }
    var coordinateView: UIView? { get }
    #if os(iOS)
    var statusBarScope: (covering: UIView?, presented: [UIView]) { get }
    #endif
    var controllers: [UIViewController] { get }
    var navigations: [UINavigationController] { get }
    var topNavigation: UINavigationController? { get }
    var owner: UIViewController? { get }
    var routes: [(node: NodeView, kind: String)] { get }
    var mounted: [(mount: PresentationMount, route: NodeView)] { get }

    #if EXACT_AGENT
    var active: Bool { get }
    var presentation: String? { get }
    var closedby: String? { get }
    var inspectionRetiring: [UIView] { get }
    var drawnScales: [(route: NodeView, kind: String, scale: CGFloat)] { get }
    func agentDismiss() -> String?
    #endif
}

/// The slots are set before any presenter is made.
package enum NavigationLink {
    /// A presenter's navigation.
    package static var make: ((Presenter) -> any NavigationHosting)?
    /// A presenter's route modals.
    package static var makeModals: ((Presenter) -> any ModalHosting)?
}

/// Navigation where the composition links none: no stack, bar or tab is ever
/// made (the roster refuses a plan that asks for one), and the core work the
/// host did on its lifecycle runs at the same points.
package final class NoNavigation: NavigationHosting {
    private unowned let presenter: Presenter
    /// Between window attach and `mounted()`: focus waits, as child views
    /// moving to the window retry it before the parent is placed.
    private var mounting = false
    private var syncing = false
    private var windowless = false, settled = false
    package weak var scrollsToTopSelection: UIScrollView?
    package var unanimated = false
    package var crossFading = false
    package func retires(_ controller: UIViewController) -> Bool { false }

    package init(_ presenter: Presenter) { self.presenter = presenter }

    package func prepare(_ batch: Batch) { presenter.elements.checkOwned() }
    /// The first frame's projection finds no route and resets, as a sync does.
    package func installInitialOwner(_ batch: Batch) {
        guard !syncing, presenter.session?.view?.window != nil else { return }
        syncing = true
        defer { syncing = false }
        reset(clearFocus: false)
    }
    /// A sync stays owed until one has run in a window: a rows-only batch
    /// takes no shortcut before then, or while the view is out of its window.
    package var syncOwed: Bool { windowless || !settled }
    package func batchApplied(_ batch: Batch, rowsOnly: Bool) {
        if !rowsOnly || syncOwed { sync() } else if batch.ops.isEmpty { fitOwner() }
        presenter.modals.inheritFromAncestors()
        if ExactEnv.authoredChrome, !syncing { fitOwner() }
    }
    package func willMount() { mounting = true }
    package func mounted() { mounting = false; sync() }
    package func unmounted() { mounting = false; reset(clearFocus: false) }

    /// What `NavigationHost.sync` does when the root has no `navigationBack`.
    private func sync() {
        presenter.listRowDependencies = nil
        guard !syncing else { return }
        let window = presenter.session?.view?.window
        windowless = presenter.session != nil && window == nil
        // With no session (a presenter on its own) nothing is owed.
        if presenter.session == nil { settled = true }
        guard window != nil else { return }
        syncing = true
        defer { syncing = false; presenter.flushPendingFocus(); presenter.elementScrolls.flush() }
        reset(clearFocus: false)
        settled = true
    }

    package func reset(clearFocus: Bool) {
        unanimated = false
        presenter.modals.reset()
        if clearFocus { presenter.cancelPendingFocus() }
        #if os(iOS)
        scrollsToTopSelection?.scrollsToTop = false
        scrollsToTopSelection = nil
        #endif
        presenter.owners.publish()
    }

    package func rebooting() { told = [] }
    package func retheme() {}
    package func appearanceChanged() {}
    package func replayHatches() {}

    package var container: NodeView? { nil }
    package var routeSurfaces: [UInt32: any RouteSurface] { [:] }
    package var primaryRouteSurfaces: [any RouteSurface] { [] }
    package var activeRouteSurface: (any RouteSurface)? { nil }
    package var activeKey: String? { nil }
    package var primaryOwner: UIViewController? { nil }
    package var bootHolder: (any BootHolding)? { nil }
    package var primaryStacks: [UIViewController] { [] }
    package var chromeOwners: [UIViewController] { [] }
    package var tabController: UITabBarController? { nil }
    package var tabPanels: [UInt32] { [] }
    package func ownsContainment(of node: NodeView, under parent: NodeView) -> Bool { false }
    package func placeOwner() {}
    package func retainsRemovedView(_ node: NodeView) -> Bool { false }
    package func logicalParent(of route: NodeView) -> NodeView? { nil }
    package func routeKey(containing view: UIView, layout: Bool) -> String? { nil }
    package func isInactiveRoute(containing view: UIView, layout: Bool) -> Bool { false }
    package func resolve(route key: String, id: String) -> NodeView? { nil }
    package var routesShown: [UInt32: BlockShown] { [:] }
    package var paneBlocks: [(id: UInt32, block: HostBlock, placement: BlockPlacement?)] { [] }
    package var topLayerBoxes: [TopLayerBox] { [] }
    #if os(iOS)
    package var relayMounts: [(mount: any RelayMount, node: NodeView)] { [] }
    #endif
    package var barItemBlocks: [UInt32: HostBlock] { [:] }
    package func showsAsSidebar(_ list: UIView) -> Bool { false }
    package func sidebarSelection(_ list: UIView) -> UInt32?? { .some(nil) }
    package func listPress(_ row: UInt32, in list: UIView, _ press: () -> Void) { press() }

    package var defersFocus: Bool { syncing || mounting }
    package var inTransition: Bool { defersFocus }
    package var changing: Bool { false }
    package var transitioning: Bool { false }
    package var hasInteractiveTransition: Bool { false }
    package func isArriving(_ node: NodeView) -> Bool { false }

    package func finishBootOwner() {}
    package func bootOwnerDrawn(_ ready: @escaping () -> Void) -> Bool { false }
    package func afterBootOwner(_ work: @escaping () -> Void) { work() }

    package var covers: [UInt32: HostCover] { [:] }
    package var coversAnswerOwed: Bool { false }
    package var wantsWholeView: Bool { false }
    /// The panes the kernel was last told a native arrangement hides, covered whole: the only covers without navigation
    private var told: Set<UInt32> = []
    package func coversChanged() { reportCovers() }
    package func reportCovers() {
        #if os(iOS)
        guard #available(iOS 27.1, *) else { return }
        let views = presenter.views, hidden = presenter.arrangements.hidden.filter { views[$0] != nil }
        let changes = hidden.subtracting(told).map { ($0, HostCover.whole as HostCover?) }
            + told.subtracting(hidden).filter { views[$0] != nil }.map { ($0, nil as HostCover?) }
        told = hidden
        if !changes.isEmpty { presenter.onCovers?(changes) }
        #endif
    }
    /// The document stays in the scrolling viewport: no native container takes the whole view.
    package func fitOwner() { presenter.hostDocument(fixed: false) }
    /// An arrangement's scroller is inset by UIKit, as with navigation.
    package func insetByUIKit(_ node: NodeView) -> Bool {
        #if os(iOS)
        if #available(iOS 27.1, *) { return presenter.arrangements.insets(node) }
        #endif
        return false
    }

    package func shownScroll(_ node: NodeView?) -> UIScrollView? { node?.scrollView }
    package func scrollBackendChanged(_ node: NodeView) {
        presenter.pageGeometryOwed = true
        presenter.listRowDependencies = nil
    }
    package func contentScrollView(of route: any RouteSurface) -> UIScrollView? { nil }
    package func automaticContentScroller(of route: any RouteSurface) -> NodeView? { nil }
    package func routeShowsBar(_ route: any RouteSurface, in nav: UINavigationController) -> Bool { false }

    package func hiddenForBar(_ view: UIView) -> Bool { false }
    package func barPictureArrived(_ node: NodeView) {}
    package func unshownByBar() -> [UInt32: String] { [:] }
    package func shownBarItem(_ id: UInt32) -> UIBarButtonItem? { nil }
    package func barItem(_ node: NodeView) -> Result<(bar: UIView, item: CGRect), BarItemMissing>? { nil }
    package func tipItem(_ node: NodeView) -> (UIBarButtonItem, UIViewController)? { nil }
    #if os(iOS)
    package func wantsDialogBar(_ dialog: NodeView) -> Bool { false }
    package func projectDialogBar(_ dialog: NodeView, on controller: UIViewController) -> Bool {
        controller.navigationController?.setNavigationBarHidden(true, animated: false)
        return false
    }
    package func endDialogBar(_ dialog: NodeView) {}
    #endif
    #if EXACT_INSPECT
    package func barSegment(_ tab: NodeView) -> (control: UIControl, segment: CGRect)? { nil }
    #endif
    package func adopts(tablist: NodeView) -> Bool { false }
    package func scopes(tablist: NodeView) -> Bool { false }
    package func tabIntent(_ kind: UInt32, _ text: String) -> Bool { false }
    package func setAppDelegate(_ controller: AnyObject, _ delegate: AnyObject?) {}
    package var lifted: (any LiftedNodes)? { nil }
    #if os(tvOS)
    package var menuGoesBack: Bool { false }
    package func menuBack() {}
    #endif

    #if EXACT_INSPECT && os(iOS)
    package func inspectTitle(_ view: UIView) -> UInt32? { nil }
    package func inspectPlace(of node: NodeView) -> (() -> CGRect?)? { nil }
    package func inspectNode(in bar: UINavigationBar, at p: CGPoint) -> UInt32? { nil }
    #endif
    #if EXACT_AGENT
    package var syncScheduled: Bool { false }
    package var allNavigations: [UINavigationController] { [] }
    package var tabBarShows: Bool { false }
    package var retiringRoots: [NodeView] { [] }
    package func drawnByBar(_ v: NodeView, shown: String) -> String? { nil }
    package func paintedForBars() -> [NodeView] { [] }
    package func barItem(ofTab index: Int) -> UITabBarItem? { nil }
    package func tabBarStands(for node: NodeView) -> Bool { false }
    package func observation() -> [String: Any] { ["linked": false] }
    package func submitSearch(_ editor: UITextField) -> Bool { false }
    package func agentBack() -> String? { nil }
    package var splitObservation: [String: Any] { ["installed": false] }
    package func activateChrome(_ node: NodeView, before: (CGPoint?) -> Bool) -> [String: Any]? { nil }
    package func drawnSearch(_ node: NodeView) -> UITextField? { nil }
    package func barButtonItem(_ node: NodeView) -> (bar: UINavigationBar, item: UIBarButtonItem, of: UINavigationItem)? { nil }
    package func searchScopeTabs(of segments: UISegmentedControl) -> [UInt32]? { nil }
    #endif
}

/// Route modals where the composition links none: nothing is ever presented.
package final class NoModals: ModalHosting {
    package init() {}
    package func prepare(_ batch: Batch) {}
    package func inheritFromAncestors() {}
    package func retainsRemovedView(_ node: NodeView) -> Bool { false }
    package func contentChanged(_ node: NodeView) {}
    package func inertBehind(_ view: UIView) -> Bool { false }
    package func mounts(_ view: UIView) -> Bool { false }
    package func surfaces(includingRetiring: Bool) -> [SessionSurface] { [] }
    package func reset() {}
    package func unmounted() {}
    #if os(iOS)
    package func statusBarChanged() {}
    #endif
    package var defersFocus: Bool { false }
    package var inTransition: Bool { false }
    package var coordinateView: UIView? { nil }
    #if os(iOS)
    package var statusBarScope: (covering: UIView?, presented: [UIView]) { (nil, []) }
    #endif
    package var controllers: [UIViewController] { [] }
    package var navigations: [UINavigationController] { [] }
    package var topNavigation: UINavigationController? { nil }
    package var owner: UIViewController? { nil }
    package var routes: [(node: NodeView, kind: String)] { [] }
    package var mounted: [(mount: PresentationMount, route: NodeView)] { [] }

    #if EXACT_AGENT
    package var active: Bool { false }
    package var presentation: String? { nil }
    package var closedby: String? { nil }
    package var inspectionRetiring: [UIView] { [] }
    package var drawnScales: [(route: NodeView, kind: String, scale: CGFloat)] { [] }
    package func agentDismiss() -> String? { nil }
    #endif
}
#endif
