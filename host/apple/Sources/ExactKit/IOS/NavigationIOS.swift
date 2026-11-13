// @ref LLP 1008 §9 — Contract routes projected into UIKit's navigation
// controller. UIKit owns recognition, arbitration, progress and cancellation.
// A pop UIKit commits invokes the Contract back control
// (NavigationPopsIOS.swift); a swipe commits only when it completes. The bar
// a stack shows, what a route projects into it and when the app module's
// hatches run are LLP 1075.003's (NavigationBarIOS.swift, NativeHatches.swift).
#if os(iOS) || os(tvOS)
import UIKit

final class RouteController: UIViewController, UIScrollViewDelegate {
    let node: NodeView
    var key: String { node.props["navigationKey"] ?? "" }
    fileprivate var usesPageViewport = false
    private(set) var contentViewport: Viewport?
    private let keyboardInset = KeyboardInset()
    var contentMount: UIView { contentViewport ?? view }
    private var viewportKeyboardDismissal: UIScrollView.KeyboardDismissMode { node.props["keyboardDismissMode"] == nil ? .interactive : node.keyboardDismissal }
    /// What Exact last projected into the navigation item, and that plus
    /// what the route hatch last saw; whether the hatch has run; the header
    /// lifted into the bar; the content scroll view a hatch was handed.
    var projectedSource: BarSource?, projected: HatchSource?, backSource: String?
    /// `projectedSource` with every item's enabling left out: an item that is only enabled or disabled changes
    /// in place, on the item kept for it (`NavigationHost.enable`, `barItemsMade`).
    var projectedFaces: BarSource?
    var hatched = false
    /// Whether a route of its stack is beneath it, to go back to: written
    /// as the stack's routes are prepared, before UIKit shows any of them
    /// (`NavigationRules.bar`).
    var pushed = false
    weak var lifted: NodeView?
    var barNodes: [NodeView] = []
    /// The filter tablist the search bar shows as its scope bar (LLP 1134).
    weak var scoped: NodeView?
    #if EXACT_INSPECT
    /// The header the bar shows the title of, and the node each bar item
    /// stands for (inspect mode selects them, InspectIOS.swift).
    weak var shownHeader: NodeView?
    var itemNodes: [ObjectIdentifier: UInt32] = [:]
    #endif
    weak var host: NavigationHost?
    weak var ownedScroll: NodeView?
    /// The scroller a large title collapses with (LLP 1075.003 Stage 3).
    weak var collapseScroll: NodeView?
    /// The physical scroller bound to UIKit; a node can replace its backend.
    var collapseScrollView: UIScrollView?
    /// A scroller drawn through control bars at the top or the foot (LLP 1131 D2), the content scroll view there.
    weak var bandedTop: UIScrollView?, bandedBottom: UIScrollView?
    /// Its insets were last taken with no bar shown (LLP 1130 D3), so a title's are measured afresh.
    var insetsBarless = false
    var ridingBars: [NodeView] = []
    func rideBars() {
        let offset = node.convert(CGPoint(x: 0, y: view.safeAreaInsets.top), from: view).y
        for bar in ridingBars where abs(bar.controlRide - offset) > 0.001 {
            bar.controlRide = offset
            bar.applyTransform()
        }
    }
    /// The scroller that is the route's last in-flow child, which reaches
    /// the screen's bottom edge (LLP 1116 D2).
    weak var edgeScroll: NodeView?
    /// The bottom cover the route has with no such scroller (`reportCovers`).
    var bottomClearance: CGFloat = 0
    /// The targets of the bar items projected from its header.
    var barPresses: [BarPress] = []
    /// The bar items last made, by node: one whose kind is unchanged is
    /// updated in place, so UIKit's glass animates the change (`barItemMade`).
    var barItemsMade: [UInt32: (item: HeaderShape.Item, bar: UIBarButtonItem, press: BarPress, menuFrom: UInt64?)] = [:]
    /// Whether the bar has been projected: the items of a later projection animate where it shows.
    var barProjected = false
    /// The space before each group, by the item that follows it.
    var barSpaces: [UInt32: UIBarButtonItem] = [:]
    /// The bar items of its header's readouts, by node, kept across
    /// projections so a readout's view changes in place (§9.15).
    var readouts: [UInt32: UIBarButtonItem] = [:]
    /// The header's search field as UIKit's search controller (§9.6).
    var search: HeaderSearch?
    /// The title Exact drew from the heading's group, and the subtitle it
    /// wrote; whether the root's tablist was hidden when it last looked (§9.10).
    let titleInk = NavigationTitleInk()
    var titleView: HeaderTitleView?, subtitle: String?, tablistHidden: Bool?
    /// The route appeared, first or again after a cancelled swipe: a zoom
    /// push's landing hears it (ZoomPushIOS).
    var appeared: (() -> Void)?
    init(_ node: NodeView) {
        self.node = node
        super.init(nibName: nil, bundle: nil)
        #if os(iOS)
        registerForTraitChanges([UITraitPreferredContentSizeCategory.self]) { (c: RouteController, _: UITraitCollection) in c.reopenTitle() }
        #endif
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        appeared?()
    }
    required init?(coder: NSCoder) { nil }
    // The bar's height changed (a push to an inline title, a rotation, a
    // hatch): the route's content area follows (LLP 1075.003 §3.5).
    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()
        rideBars()
        host?.coversChanged()
    }
    // Back on a screen whose list pushed the one leaving: the row that was
    // tapped fades with the transition (LLP 1115.000 D11.3).
    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        host?.presenter.groupedLists?.appearing(node, alongside: animated ? transitionCoordinator : nil)
        host?.presenter.rowCells.appearing(node, alongside: animated ? transitionCoordinator : nil)
    }
    override func viewIsAppearing(_ animated: Bool) {
        super.viewIsAppearing(animated)
        // Follow a tab UIKit selected with no call Exact hears (an app's own code selecting it) as its route
        // appears (`followSelection`)
        host?.followSelection(showing: self)
        // Its block, at the size UIKit gives it, before its first frame (LLP 1123 D2)
        host?.presenter.owners.publish()
        // UIKit has attached and sized the real destination. Finish its
        // content and focus here, before the transition's first frame.
        host?.presenter.afterBatch { [weak self] in
            guard let host = self?.host else { return }
            host.coversChanged()
            if let pending = host.presenter.pendingFocusNode, host.isArriving(pending) {
                host.presenter.flushPendingFocus()
            }
            host.presenter.syncAccessibility()
            host.presenter.paintVisibleText()
        }
    }

    // Its route is laid out in this view's bounds (LLP 1123 D2).
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        fitContentViewport()
        host?.presenter.owners.publish()
        #if os(iOS)
        search?.labelScopes()
        #endif
    }

    override func loadView() {
        view = PresentationMount()
        paintBackdrop()
        view.addSubview(node)
        host?.presenter.session?.natives.routeLoaded(node)
    }

    /// The backdrop's colours last written, light and dark.
    private var backdropKey: [[Double]?]?
    /// The controller's view paints what shows behind the route: a dynamic
    /// colour resolved for the controller's current appearance, re-resolved
    /// by UIKit when it changes (a route loaded before its window has a trait
    /// collection would otherwise keep the light colour in dark mode), and
    /// written again when a batch changes it. A pushed route shows its
    /// backdrop (`backdrop`); a route under no background shows the system
    /// background, not white. A sheet is the route's own background where it
    /// has one, as a grouped sheet's surface is the grouped background (LLP
    /// 1116.021 D7, amending LLP 1115 D2), so no white band shows where the
    /// route stops above a keyboard's bar; else clear, over the surface its
    /// presentation paints (`ModalController.paintSurface`: UIKit's glass at
    /// a partial detent, LLP 1141.001 D3). What is behind the sheet is not,
    /// and dimming belongs to UIKit's presentation. A full-screen route that
    /// paints its own background is clear, as its presentation is
    /// (`NodeView.paintsOverPage`): the route paints it once, over the page.
    func paintBackdrop() {
        guard isViewLoaded else { return }
        // The route's view takes its accent as its tint, which its backdrop in the tint is resolved by
        assign(view, \.tintColor, node.ownAccent)
        let modal = node.props["navigationPresentation"] == "modal"
        let overPage = node.paintsOverPage
        let key = overPage ? []
            : modal ? [ownBackdrop(dark: false), ownBackdrop(dark: true)]
            : [backdrop(dark: false), backdrop(dark: true)]
        guard view.backgroundColor == nil || key != backdropKey else { return }
        backdropKey = key
        if overPage { view.backgroundColor = .clear; return }
        #if os(tvOS)
        // tvOS has no system backgrounds; white stands in, as the viewport's.
        let system = UIColor.white
        #else
        // Where nothing is painted, the platform's surface (LLP 1115 D2).
        let system = UIColor.platformBackground(in: host?.presenter.root ?? view)
        #endif
        view.backgroundColor = UIColor { [weak self] traits in
            let dark = traits.userInterfaceStyle == .dark
            let colour = modal ? self?.ownBackdrop(dark: dark, in: traits) : self?.backdrop(dark: dark, in: traits)
            return colour.map { TextEngine.color($0) } ?? (modal ? .clear : system)
        }
    }

    /// The route's own background, if it paints one; in `traits`, as the view's dynamic colour
    /// resolves it, the tint by the view's (`channels(_:in:)`), which is the route's accent.
    private func ownBackdrop(dark: Bool, in traits: UITraitCollection? = nil) -> [Double]? {
        if node.groundIsSheets { return nil }
        return Self.painted(node, dark: dark, in: traits)
    }
    private static func painted(_ node: NodeView, dark: Bool, in traits: UITraitCollection?) -> [Double]? {
        (traits.map { node.channels("background_color", in: $0) } ?? node.channels("background_color", dark: dark)).flatMap { $0.count < 4 || $0[3] > 0 ? $0 : nil }
    }

    /// What shows behind the route where it paints nothing: its own
    /// background, else the nearest one its logical ancestors paint — its
    /// tabpanel's, then the boxes up to the root — as the web shows them
    /// through a transparent route (LLP 1116 D2: a root's
    /// `-exact-grouped-background` under every screen).
    func backdrop(dark: Bool, in traits: UITraitCollection? = nil) -> [Double]? {
        let painted = { (n: NodeView) in Self.painted(n, dark: dark, in: traits) }
        if let own = ownBackdrop(dark: dark, in: traits) { return own }
        guard let host, let root = host.container else { return nil }
        var next: UIView? = host.logicalParent(of: node)
        while let v = next {
            if let n = v as? NodeView, let colour = painted(n) { return colour }
            if v === root { break }
            next = v.superview
        }
        return nil
    }
    func mount() {
        loadViewIfNeeded()
        if usesPageViewport, contentViewport == nil {
            let scroll = Viewport(frame: view.bounds)
            scroll.documentNode = node
            scroll.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            scroll.contentInsetAdjustmentBehavior = .never
            scroll.keyboardDismissMode = viewportKeyboardDismissal
            #if os(iOS)
            scroll.scrollsToTop = false
            #endif
            scroll.delegate = self
            view.addSubview(scroll)
            contentViewport = scroll
        } else if !usesPageViewport, let scroll = contentViewport {
            contentViewport = nil
            keyboardInset.apply(0, to: scroll)
            view.addSubview(node)
            scroll.removeFromSuperview()
        }
        if node.superview !== contentMount { contentMount.addSubview(node) }
        host?.presenter.regrid(node) // from the controller's view, a grid origin of its own
    }
    /// The route's room for the keys, and under `resizes-visual` its
    /// viewport's frame, extent and inset.
    func fitContentViewport(in block: CGRect? = nil) {
        #if os(iOS)
        // Under a search in the bar the route keeps its layout (`block(in:)`)
        // and takes the keys as safe area, as SwiftUI's keyboard region is:
        // UIKit insets its edge scroller so the last rows reach above them.
        // A visual viewport takes them as its inset instead, without reflow.
        if let presenter = host?.presenter {
            var keys: CGFloat = 0
            if contentViewport == nil, presenter.editsInBar(of: view), let top = presenter.keyboardTop, let window = view.window {
                let edge = view.convert(CGPoint(x: 0, y: top), from: window).y
                keys = max(0, view.bounds.maxY - edge - (view.safeAreaInsets.bottom - additionalSafeAreaInsets.bottom))
            }
            if additionalSafeAreaInsets.bottom != keys { additionalSafeAreaInsets.bottom = keys }
        }
        #endif
        guard let scroll = contentViewport, let presenter = host?.presenter else { return }
        let oldSize = scroll.bounds.size, oldContent = scroll.contentSize, oldInset = scroll.contentInset
        let allocation = block ?? presenter.owners.block(in: view)
        let frame = ExactEnv.authoredChrome ? allocation : view.bounds
        if scroll.frame != frame { scroll.frame = frame }
        scroll.keyboardDismissMode = viewportKeyboardDismissal
        let extent = CGSize(width: max(node.bounds.width, node.style["overflow_x"]?.string == "visible" || node.style["overflow_x"] == nil ? node.content.width : 0),
                            height: max(node.bounds.height, node.style["overflow_y"]?.string == "visible" || node.style["overflow_y"] == nil ? node.content.height : 0))
        let size = CGSize(width: max(allocation.width, node.frame.minX + extent.width), height: max(allocation.height, node.frame.minY + extent.height))
        if scroll.contentSize != size { scroll.contentSize = size }
        // Keep drawing under the keyboard while the content clears its occupied area
        var overlap = max(0, frame.maxY - allocation.maxY)
        if presenter.keyboardPolicy == "resizes-visual", presenter.ownsKeyboard(in: view),
           let top = presenter.keyboardTop, let window = view.window {
            let keys = view.convert(CGPoint(x: 0, y: top), from: window).y
            overlap = min(allocation.height, max(0, allocation.maxY - keys))
        }
        keyboardInset.apply(overlap, to: scroll)
        if oldSize != scroll.bounds.size || oldContent != size || oldInset != scroll.contentInset {
            let offset = PixelGrid.offset(scroll.contentOffset, in: scroll)
            if offset != scroll.contentOffset { scroll.contentOffset = offset }
        }
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard scrollView === contentViewport, let presenter = host?.presenter else { return }
        presenter.owners.publish()
        presenter.stickies.scrolled(node.id)
        presenter.transformGeometry.changed()
        presenter.videoVisibility?.changed()
        presenter.scrollPump.scrolled(nil)
        FrameSampler.scrolled(scrollView, in: presenter.session, person: false)
        presenter.session?.frameHealth.trackScrolled(scrollView)
        node.repaintThrough()
    }

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        host?.presenter.elementScrolls.userScrolled(scrollView)
        if let session = host?.presenter.session { session.frameHealth.beginScroll(scrollView, session: session) }
    }
    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate { host?.presenter.session?.frameHealth.endScroll(scrollView) }
    }
    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) { host?.presenter.session?.frameHealth.endScroll(scrollView) }

    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        host?.presenter.elementScrolls.revealEnded(scrollView)
    }

    /// Whether UIKit has held this route in a stack since a pop of it last
    /// reached the router (`NavigationHost.followPops`), and the navigation
    /// controller that last held it. With these a route of the router's that
    /// no stack holds is told apart: one UIKit popped, or one the app asked
    /// for that Exact has not written yet.
    var held = false
    weak var lastStack: UINavigationController?

    /// The route's node, left in place after a batch removed the route while
    /// UIKit still shows it (`NavigationHost.prepare`), until UIKit takes the
    /// route's view out of the window.
    var retiringRoot: NodeView?
    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        retiringRoot?.removeFromSuperview()
        retiringRoot = nil
    }

    // Follow UIKit moving this route into another controller (`NavigationHost.moved(into:)`)
    override func willMove(toParent parent: UIViewController?) {
        super.willMove(toParent: parent)
        guard let parent else { return }
        held = true
        if let nav = parent as? UINavigationController { lastStack = nav }
        host?.moved(into: parent)
    }

    private(set) var nativeContentScrollEdges: NSDirectionalRectEdge = []
    var nativeContentScroller: NodeView? {
        guard let selected = contentScrollView(for: .top) else { return nil }
        var owner = (selected as? Viewport)?.documentNode
        var ancestor = selected.superview
        while owner == nil, let view = ancestor {
            if let node = view as? NodeView, node.scrollView === selected { owner = node }
            ancestor = view.superview
        }
        return owner.flatMap { $0 === node || $0.isDescendant(of: node) ? $0 : nil }
    }

    /// Exact's selection for the top edge: the scroller the title collapses with, else one drawn
    /// through control bars there (LLP 1131 D2).
    var automaticTopScroll: UIScrollView? { collapseScrollView ?? bandedTop }
    /// Exact's selection for the bottom edge: the scroller at the route's bottom edge, else one drawn
    /// through control bars there.
    var automaticBottomScroll: UIScrollView? { edgeScroll?.scrollView ?? bandedBottom }

    /// Select Exact's scroller for an edge the app leaves automatic. UIKit
    /// makes an automatic selection by finding the scroll view itself, and it
    /// cannot find an authored scroller through the route's views, so Exact's
    /// selection stands in for it. An app's native selection holds the edge
    /// until it selects nil, UIKit's automatic selection, again.
    func projectContentScrollView(_ scrollView: UIScrollView?, for edge: NSDirectionalRectEdge) {
        guard nativeContentScrollEdges.intersection(edge).isEmpty else { return }
        select(scrollView, for: edge)
    }

    /// An app's selection; Exact's own goes through `projectContentScrollView`.
    override func setContentScrollView(_ scrollView: UIScrollView?, for edge: NSDirectionalRectEdge) {
        if let scrollView {
            // Record only the top and bottom edges: UIKit takes no other
            nativeContentScrollEdges.formUnion(edge.intersection([.top, .bottom]))
            select(scrollView, for: edge)
        } else {
            // An app's nil makes the edges it selected automatic again, and the navigation host
            // selects them as for a route no app touched. UIKit gets the nil first: the passes take
            // an edge's current scroll view for Exact's last selection and would undo its insets.
            let released = nativeContentScrollEdges.intersection(edge)
            guard !released.isEmpty else { return }
            nativeContentScrollEdges.subtract(released)
            select(nil, for: released)
            host?.selectScrollEdges(of: self)
        }
        #if os(iOS)
        if edge.contains(.top) { host?.presenter.syncScrollsToTop() }
        #endif
    }

    // A plain tab's tab bar follows the route's scroll views as a navigation controller's follows its top's.
    private func select(_ scrollView: UIScrollView?, for edge: NSDirectionalRectEdge) {
        super.setContentScrollView(scrollView, for: edge)
        (parent as? PlainTab)?.setContentScrollView(scrollView, for: edge)
        if edge.contains(.top) { followTopEdge(scrollView) }
    }

    private var topEdgeWatch: NSKeyValueObservation?
    /// iOS 26 draws the bar's edge effect on the outermost scroll view under
    /// it, which for a route whose scroller (a list) lies in its page is the
    /// page's viewport, not that scroller (measured on iOS 27: UIKit alone
    /// does the same with a table view in a scroll view). So the viewport's
    /// top effect shows only while that scroller is scrolled from its top,
    /// as UIKit's does over a list that is the controller's own: no effect
    /// over a list at rest. Its style stays `automatic`: the rows blurred and
    /// faded under the whole bar to a line at its foot, as a UIKit or SwiftUI
    /// list or scroll view draws them there (iOS 27, measured), where `soft`
    /// left them legible under the title.
    private func followTopEdge(_ scrollView: UIScrollView?) {
        topEdgeWatch = nil
        guard #available(iOS 26.0, tvOS 26.0, *), let page = contentViewport else { return }
        guard let scrollView, scrollView !== page, node.style["scroll_edge_effect"] == nil else {
            if page.topEdgeEffect.isHidden, node.style["scroll_edge_effect"]?.string != "none" { page.topEdgeEffect.isHidden = false }
            return
        }
        let follow = { [weak page] (sv: UIScrollView) in
            let rest = sv.contentOffset.y <= 0.5 - sv.adjustedContentInset.top
            if let page, page.topEdgeEffect.isHidden != rest { page.topEdgeEffect.isHidden = rest }
        }
        follow(scrollView)
        topEdgeWatch = scrollView.observe(\.contentOffset) { sv, _ in follow(sv) }
    }
}

final class NavigationHost: NSObject, UINavigationControllerDelegate, UIGestureRecognizerDelegate {
    weak var scrollsToTopSelection: UIScrollView?
    unowned let presenter: Presenter
    /// The selected tab's stack. An app's tab container can show a stack
    /// without appearance callbacks, so restart its reveal here.
    var primaryNavigation: UINavigationController? {
        didSet {
            guard primaryNavigation !== oldValue else { return }
            if changing { startRevealing() }
        }
    }
    var primaryRoutes: [RouteController] { primaryStacks.flatMap(\.stackRoutes).compactMap { $0 as? RouteController } }
    private(set) var presentedNavigations: [UINavigationController] = []
    func isPresented(_ nav: UINavigationController?) -> Bool { presentedNavigations.contains { $0 === nav } }
    /// The theme's title ink each bar was last given (`titleInk`), held no
    /// longer than its bar.
    let titleInks = NSMapTable<UINavigationBar, UIColor>.weakToStrongObjects()
    var modalNavigation: UINavigationController? { presentedNavigations.last }
    var navigation: UINavigationController? { modalNavigation ?? primaryNavigation }
    /// The route on top of the stack in front: the one the person sees.
    var activeRoute: RouteController? { (navigation?.topViewController ?? primaryPlain?.route) as? RouteController }
    private(set) var syncing = false
    /// A pop UIKit committed with no call Exact heard is followed in the next turn (`followPopsNextTurn`).
    var followOwed = false
    private var mounting = false
    private(set) weak var container: NodeView?
    private(set) var routeIDs: [UInt32] = []
    private(set) var controllers: [UInt32: RouteController] = [:]
    /// The removed routes UIKit still shows, whose views stay until it takes them away (`prepare`).
    let retiringRoutes = NSHashTable<RouteController>.weakObjects()
    /// Each route owner's logical children (the root's, each tabpanel's),
    /// while UIKit containment holds the route views (LLP 1075.003 §3.7).
    var logicalChildren: [UInt32: [UInt32]] = [:]
    /// Exact's tab container and its stacks by panel id; the tablist whose
    /// place its bar takes; the tab delegate Exact keeps and forwards.
    var tabController: UITabBarController?
    /// What holds the tabs: Exact's tab controller, or a container the app's
    /// `tabContainer` hatch returned (LLP 1075.003 §3.6).
    var tabOwner: UIViewController?
    var tabsHatched = false, tabContainerAsked = false, routerTab = -1
    /// The router's tab's root route when a tap reselects it at that root,
    /// kept from `shouldSelect` to `didSelect` to scroll it to its top.
    weak var reselected: RouteController?
    /// The tab UIKit is selecting for a person: a tap on the bar, from UIKit's `shouldSelect` to its
    /// `didSelect`, which UIKit sends in one call with its selection between them and no transition
    /// coordinator, or a row of More's list, during More's `willShow`. UIKit selects it, so `syncTabs`
    /// does not, and the safe areas UIKit lays it out through are not reported until `followUIKit`.
    weak var tapping: UIViewController?
    /// The tab controller UIKit selected whose press `followSelection` made, so later calls for the same
    /// selection do not press it again while the router stays on the tab it was on.
    weak var selectionPressed: UIViewController?
    /// Each tab's controller by its panel id (PlainTabIOS): a navigation
    /// controller, or a plain tab.
    var tabStacks: [UInt32: UIViewController] = [:]
    /// UIKit's More as Exact last adopted it (iOS), held until the next (`adoptMore`).
    var adoptedMore: UINavigationController?
    /// Whether a sync is owed in the next turn (`syncNextTurn`).
    var resyncOwed = false
    var tabPanels: [UInt32] = []
    /// How many routes the selected stack declared at the last projection.
    private var selectedRouteCount = 0
    var adoptedTablist: UInt32?
    var tablistAppearanceNone = false
    /// Where the authored tablist was when its covers were last measured.
    var measuredTablist: CGRect?
    /// The root whose tablist last decided the bar, and whether Exact hid the
    /// bar for a root's hidden tablist (LLP 1075.003 §3.7, amended).
    weak var tablistRoot: RouteController?
    var tablistHidBar = false
    var tabItems: [String] = []
    /// The tabs the items were last made from, made again when the size
    /// class (a path glyph's size) changes.
    var tabItemsSource: NavigationTabs?
    /// The bar's tint as last written as a value: the selected tab's own tint, light and dark; nil
    /// where the bar takes an accent as its tint.
    var tabTint: [[Double]]?
    let tabProxy = TabDelegateProxy()
    /// Each tab's `UITab`, in tab order, when Exact's container is built
    /// from them (TabsIOS.swift); empty for one built from `viewControllers`.
    var builtTabs: [AnyObject] = []
    /// What Exact added to its owner's `additionalSafeAreaInsets` for the
    /// boxes beside the routes, and that owner (`clearBesideRoutes`).
    var besideInsets = UIEdgeInsets.zero
    weak var besideOwner: UIViewController?
    /// A context menu's commit pushes without the stack's animation: UIKit
    /// animates it (`.pop`, LLP 1021 §5.1). The selected route's key tells
    /// whether its press navigated.
    var unanimated = false
    /// Whether a reselected tab's scroll to the top (`scrollToTop`) is
    /// UIKit's animated one: not under the agent's frozen clock, where Exact
    /// starts no UIKit animation, nor in a test outside an app's scene, where
    /// UIKit never moves an animated offset.
    var scrollsToTopAnimated = !ExactEnv.agentFreezes
    var activeKey: String? { container?.props["navigationKey"] }
    /// While a push or pop runs: paints what each frame newly reveals.
    /// LLP 1075.003: each stack Exact built, by controller; what each shown
    /// bar covers of its route; the ownership changes already journaled;
    /// whether the hatches are being replayed for a cold launch's objects.
    var stacks: [ObjectIdentifier: NavigationStack] = [:]
    var stackCount = 0
    var covers: [UInt32: HostCover] = [:]
    /// The covers the kernel was last told (`deliverCovers`), each route's local insets (LLP 1123 D9), and
    /// the clearance its scrollers' insets hold (D3, ClearanceIOS.swift).
    var delivered: [UInt32: HostCover] = [:], localInsets = LocalInsetsReport(), clearance: [ObjectIdentifier: ClearanceInset] = [:]
    var deliveringCovers = false
    /// Whether this boot's first batch looked for the tablist the boot
    /// covered (`adoptBootCover`); every boot covers its own.
    private(set) var bootCoverSeen = false
    var ownedReported: Set<String> = []
    var replayingHatches = false
    private var coversPending = false
    /// Whether the session's view last took the whole of its own for a bar.
    var tookWholeView = false
    var pendingSync = false
    /// Whether a stack's write waits for a transition or a move UIKit runs there (`writable`, `runHeldSync`).
    var tabsHeld = false
    /// The press whose commit a deferred sync still owes (PressHandoffIOS).
    private weak var pendingPress: PressHandoff?
    /// A sync was deferred (a transition ran, or there was no window yet)
    /// and is still owed, or UIKit moved a stack itself (its back button, a
    /// swipe) since the last one: the next batch syncs, whatever it holds.
    var syncOwed: Bool {
        pendingSync || tabsHeld || changing || windowless || nativeMoved || !settled
            || ([primaryNavigation].compactMap { $0 } + presentedNavigations).contains { $0.transitionCoordinator != nil }
    }
    /// No window yet; UIKit moved a stack since the last sync; the last
    /// sync ran to its end (one that stopped early, waiting for the first
    /// draw, a transition or a presentation it could not make yet, is owed).
    private var windowless = false, nativeMoved = false, settled = false
    /// Syncs asked for, for tests.
    private(set) var syncCalls = 0
    /// The root key last journaled as matching no route, so a refusal is one
    /// line, not one per batch (LLP 1035.001 D6).
    private var refusedKey: String?
    /// The keys of the stack last shown, through its selected route, and the
    /// route selected in the root's place while its key matches none (D1).
    private(set) var shownKeys: [String] = [], fallbackKey: String?
    /// The key of the selected route: the root's, or while the root's
    /// matches no route, the route left selected of the stack last shown.
    var selectedKey: String? {
        guard let key = container?.props["navigationKey"] else { return nil }
        return key == refusedKey ? fallbackKey ?? key : key
    }

    init(presenter: Presenter) { self.presenter = presenter }

    /// The node a route is a child of in the tree, where UIKit holds its
    /// view elsewhere: the root or, with tabs, its tabpanel.
    func logicalParent(of route: NodeView) -> NodeView? {
        guard let owner = logicalChildren.first(where: { $0.value.contains(route.id) })?.key else { return nil }
        return owner == container?.id ? container : presenter.views[owner]
    }

    /// Logical child-list edits leave declared, retained routes inside their
    /// controllers; newly added or no-longer-declared nodes use normal mounting.
    /// A route's logical parent is the root or, with tabs, its tabpanel.
    func ownsContainment(of node: NodeView, under parent: NodeView) -> Bool {
        (parent === container || (parent !== container && logicalChildren[parent.id] != nil)) &&
            container?.props["navigationBack"] != nil &&
            node.props["navigationKey"] != nil && controllers[node.id]?.node === node
    }

    /// One batch's projection: the root, each stack's routes — one a tab
    /// (LLP 1075.003 §3.7), or the root's own — which stack holds the route
    /// the root names and where, and each stack's wanted controllers (the
    /// selected stack's prefix through that route; another tab's whole).
    struct Projection {
        let root: NodeView
        let tabs: NavigationTabs?
        let stacks: [[NodeView]]
        let at: Int, selected: Int
        let wanted: [[RouteController]]
        var routes: [NodeView] { stacks[at] }
        var chosen: [RouteController] { wanted[at] }
    }

    /// Put each route node back under its logical parent, before native
    /// ownership retires, so surviving content stays in the window.
    private func restoreLogicalChildren() {
        for (owner, ids) in logicalChildren {
            guard let parent = owner == container?.id ? container : presenter.views[owner] else { continue }
            for (index, node) in ids.compactMap({ presenter.views[$0] }).enumerated() where presenter.views[node.id] === node {
                parent.container.insertSubview(node, at: index)
                presenter.regrid(node)
            }
        }
    }

    /// Resolve the declared route stacks once for both structural installation
    /// and later presentation. UIKit containment never becomes route state.
    private func projection(_ batch: Batch) -> Projection? {
        guard let root = presenter.root.subviews.first as? NodeView,
              root.props["navigationBack"] != nil else {
            // Retiring native ownership must not remove surviving content.
            // Restore the logical children before detaching the controller so
            // an editor remains in the same window through the handoff.
            if container != nil { restoreLogicalChildren() }
            reset(clearFocus: container != nil)
            return nil
        }
        if container !== root {
            // A fresh owner after unmount must preserve focus requested while
            // offscreen. Replacing an existing root still retires its command.
            reset(clearFocus: container != nil)
            container = root
            logicalChildren = [root.id: root.container.subviews.compactMap { ($0 as? NodeView)?.id }]
            // Fold this batch's lists again for a new root, known only from here, which the first install reads
            // while the batch is still being applied (`installInitialOwner`)
            foldChildren(batch)
        }
        // With tabs, each panel holds a stack; a panel seen for the first
        // time still holds its rows, and its children ops say the rest.
        let tabs = NavigationTabs.of(root, presenter)
        for panel in tabs?.panels ?? [] where logicalChildren[panel.id] == nil {
            logicalChildren[panel.id] = panel.container.subviews.compactMap { ($0 as? NodeView)?.id }
        }
        let owners = tabs?.panels ?? [root]
        let stacks = owners.map { owner in
            (logicalChildren[owner.id] ?? []).compactMap { presenter.views[$0] }.filter { $0.props["navigationKey"] != nil }
        }
        routeIDs = stacks.flatMap { $0.map(\.id) }
        // Every route left the tree (the root shows something else now): the
        // containers go with them, or their views — a removed route's frozen
        // snapshot among them — stay over the new content (mail F21).
        if routeIDs.isEmpty, primaryOwner != nil || !presentedNavigations.isEmpty {
            presenter.session?.log("navigation: the root has no routes now; its containers are retired")
            reset(clearFocus: false)
            return nil
        }
        // D1: the stack is the prefix through the route the root names; a
        // key that names none leaves the stack alone, less the routes that
        // left the tree (a closed sheet's), and says so once.
        let rootKey = root.props["navigationKey"] ?? ""
        let keys = { (routes: [NodeView]) in routes.map { $0.props["navigationKey"] ?? "" } }
        var selecting = rootKey
        if !stacks.contains(where: { keys($0).contains(rootKey) }) {
            if refusedKey != rootKey {
                refusedKey = rootKey
                presenter.session?.log("navigationKey \"\(rootKey)\" matches no route among the root's children or those of the tabpanels its tablist names; the stack is unchanged")
            }
            guard let survivor = NavigationRules.survivor(of: shownKeys, declared: stacks.map(keys)) else { fallbackKey = nil; return nil }
            selecting = survivor
        } else {
            refusedKey = nil
        }
        fallbackKey = selecting == rootKey ? nil : selecting
        guard let at = stacks.firstIndex(where: { NavigationRules.stack(routeKeys: keys($0), selected: selecting) != nil }),
              let range = NavigationRules.stack(routeKeys: keys(stacks[at]), selected: selecting) else { return nil }
        shownKeys = Array(keys(stacks[at])[range])
        selectedRouteCount = stacks[at].count
        let wanted = stacks.enumerated().map { index, routes in
            var presented = false
            return (index == at ? Array(routes[range]) : routes).map { node -> RouteController in
                presented = presented || ["modal", "fullscreen"].contains(node.props["navigationPresentation"] ?? "")
                let c = controllers[node.id] ?? RouteController(node)
                // A separate visual viewport accommodates the keyboard without reflowing the route
                c.usesPageViewport = !presented && !ExactEnv.authoredChrome && presenter.keyboardPolicy == "resizes-visual"
                c.host = self
                controllers[node.id] = c
                c.mount()
                return c
            }
        }
        return Projection(root: root, tabs: tabs, stacks: stacks, at: at, selected: range.upperBound - 1, wanted: wanted)
    }

    /// The native container fills the page's area. It owns the screen, so
    /// the document is in its mount, where no scroll view holds it (`hostDocument`).
    func fitOwner() {
        presenter.hostDocument(fixed: wantsWholeView)
        guard let owner = primaryOwner, let view = owner.viewIfLoaded, let root = view.superview else { return }
        let frame = ownerFrame(owner, in: root)
        if view.frame != frame { view.frame = frame }
    }

    private func installPrimary(_ p: Projection, first: [RouteController]) {
        guard primaryOwner == nil else { return }
        // Find the containing controller before installing our child.
        guard let parent = ViewPresentation.controller(for: p.root) else { return }
        if let tabs = p.tabs {
            installTabs(p, tabs, first: first, in: parent)
            return
        }
        // The stack holds its first routes before its view enters the window,
        // as the tabs' stacks do (`installTabs`): UIKit shows a root's large
        // title collapsed when the stack was laid out in the window before it
        // had that root.
        let nav = makeNavigation(first: first.first?.node)
        primaryNavigation = nav
        prepareRoutes(first, in: nav)
        nav.setViewControllers(first, animated: false)
        watchPops(nav)
        parent.addChild(nav)
        nav.view.frame = ownerFrame(nav, in: p.root)
        nav.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        p.root.addSubview(nav.view)
        nav.view.setPaintForeground(aboveAuthored: false)
        fitOwner()
        nav.didMove(toParent: parent)
    }

    /// A hidden navigation bar needs its availability check here. The
    /// recognizer, competing scroll views and transition stay UIKit's.
    /// UIKit gives both recognizers its own delegate when the controller's
    /// view loads: a tab's stack, built before its tab controller shows it,
    /// is loaded first, or the swipe never asks Exact.
    func watchPops(_ nav: UINavigationController) {
        nav.loadViewIfNeeded()
        if nav.view.accessibilityIdentifier == nil {
            nav.view.accessibilityIdentifier = "exact-navigation-\(UUID().uuidString)"
        }
        // tvOS has no interactive pop gesture.
        #if !os(tvOS)
        nav.interactivePopGestureRecognizer?.delegate = self
        if #available(iOS 26.0, tvOS 26.0, *) { nav.interactiveContentPopGestureRecognizer?.delegate = self }
        #endif
    }

    /// Initial containment is ready before child frames. It does not present
    /// a sheet or flush focus commands from the partially applied batch.
    func installInitialOwner(_ batch: Batch) {
        guard primaryOwner == nil, !syncing, presenter.session?.view?.window != nil else { giveBackBootCover(); return }
        syncing = true
        defer { syncing = false }
        guard let p = ApplyProfile.time("nav.projection", { projection(batch) }) else { giveBackBootCover(); return }
        // After the projection, whose first `reset` clears the covers.
        _ = adoptBootCover()
        let parts = NavigationRules.segments(presentations: p.routes[...p.selected].map { $0.props["navigationPresentation"] })
        ApplyProfile.time("nav.primary") { installPrimary(p, first: Array(p.chosen[parts[0]])) }
        ApplyProfile.time("nav.layout") { primaryOwner?.view.layoutIfNeeded() }
        // The content area the bar leaves reaches layout in this turn, before
        // the first frame (LLP 1075.003 Q3 (c)).
        ApplyProfile.time("nav.covers") { reportCovers() }
        refitForBars()
    }

    /// The root's tablist the runtime covered before the first layout,
    /// for the tab bar it expects to take its place (host/apple/src/closed.rs
    /// `cover_root_tablist`), becomes one of the presenter's covers, which
    /// `reportCovers` keeps while the bar shows and gives back otherwise.
    /// Once, at the first batch's install.
    private func adoptBootCover() -> Bool {
        guard !bootCoverSeen else { return false }
        bootCoverSeen = true
        guard let root = presenter.root.subviews.first as? NodeView,
              let tabs = presenter.session?.runtime.rootTabs(root.id), tabs.covered, covers[tabs.tablist] == nil else { return false }
        covers[tabs.tablist] = .whole
        delivered[tabs.tablist] = .whole
        return true
    }

    /// No containers at the first batch: the tablist the boot covered is
    /// laid out again (`adoptBootCover`).
    private func giveBackBootCover() {
        if adoptBootCover() { reportCovers() }
    }

    /// The runtime restarts (`Presenter.reset`): its next boot covers the
    /// tablist again, for its first batch to adopt or give back.
    func rebooting() { bootCoverSeen = false }

    /// A native primary container coming or going changes the view the
    /// session takes (ExactViewIOS `fit`, LLP 1123 D1), whatever its bars
    /// show: lay it out again this turn.
    func refitForBars() {
        guard wantsWholeView != tookWholeView else { return }
        tookWholeView = wantsWholeView
        // Now, not at the layout: under `cover` or the agent's own size the
        // view's area does not change, and `fit` refits the owner only when it does.
        presenter.hostDocument(fixed: wantsWholeView)
        presenter.session?.view?.setNeedsLayout()
    }

    func sync(_ batch: Batch) {
        // Installing or moving a controller can synchronously cause layout.
        // That layout must not start another containment handoff inside this one.
        syncCalls += 1
        guard !syncing else { return }
        tabsHeld = false
        let window = presenter.session?.view?.window
        windowless = presenter.session != nil && window == nil
        // With no session (a presenter on its own) nothing is projected or owed.
        if presenter.session == nil { settled = true }
        guard window != nil else { return }
        nativeMoved = false; settled = false
        syncing = true
        defer { syncing = false; presenter.flushPendingFocus(); presenter.elementScrolls.flush() }
        guard let p = projection(batch) else { settled = true; return }
        followHeld()
        // Follow by the next turn a pop UIKit committed that no call has reported yet (its `willShow` is still to
        // come, or an app's own delegate on the stack hears it); until then its tab is not written (`routesInFlight`)
        if !poppedByUIKit().isEmpty { followPopsNextTurn() }
        let root = p.root, wanted = p.chosen
        // A presentation whose route a logical ancestor's `display: none`
        // hides is not presented, nor is any above it: its sheet is out of
        // the document's tree, which no longer hides it (LLP 1123 D2)
        let segments = NavigationRules.segments(presentations: p.routes[...p.selected].map { $0.props["navigationPresentation"] })
        let parts = Array(segments.enumerated().prefix { $0.offset == 0 || !logicallyHidden(wanted[$0.element.lowerBound].node) }.map(\.element))
        reshape(p)
        installPrimary(p, first: Array(wanted[parts[0]]))
        // The other tabs' stacks, the selected tab and its items (LLP 1075.003 §3.7).
        if p.tabs != nil { syncTabs(p) }
        // Initial content draws before a presentation covers it.
        if parts.count > 1, let session = presenter.session, session.firstDrawMs == nil { return }
        guard !changing, !presenter.modals.inTransition else {
            pendingSync = true
            owePress()
            reportCovers()
            return
        }
        pendingSync = false
        // The press a deferred sync is owed to: what this sync presents or
        // pushes is that press's work, and it is owed again if this defers too.
        let owed = pendingPress.flatMap { $0 === presenter.pressHandoff ? $0 : nil }
        pendingPress = nil
        owed?.syncing = true
        defer {
            owed?.syncing = false
            owed?.pending = false
            if pendingSync { owePress(owed) }
            owed?.settle()
        }
        let boundaries = parts.dropFirst().map { wanted[$0.lowerBound].node }
        let mounted = presenter.modals.routes
        let common = zip(boundaries, mounted).prefix {
            $0.0 === $0.1.node && $0.0.props["navigationPresentation"] == $0.1.kind
        }.count
        // Mount the destination under a departing presentation before starting
        // dismissal, so its editor can accept the batch's focus handoff.
        let primary: UIViewController? = primaryPlain ?? primaryNavigation
        let owners = [primary].compactMap { $0 } + presentedNavigations
        var held = false
        for index in 0...common where index < owners.count {
            let stack = Array(wanted[parts[index]])
            // Show the router's tab's route when it is a plain tab (PlainTabIOS)
            guard let nav = owners[index] as? UINavigationController else {
                setRoutes(stack, in: owners[index])
                owners[index].view.layoutIfNeeded()
                continue
            }
            // Write a stack with a transition UIKit runs there (or left there, off screen) once it is over; one that
            // holds the routes already is projected now
            guard nav.transitionCoordinator == nil || nav.stackRoutes.elementsEqual(stack, by: ===) else {
                tabsHeld = true
                continue
            }
            let waits = prepareRoutes(stack, in: nav)
            // Compare and write only the routes: under More, UIKit's list stays before them (`stackFront`)
            let routes = nav.stackRoutes
            if !routes.elementsEqual(stack, by: ===) {
                // UIKit lays an arriving route out as it is given the stack: a header
                // lifted in this batch must have left the layout by then (`coverLifted`).
                if waits {
                    held = true
                    continue
                }
                // @ref LLP 1038 D6 — a tab change swaps immediately. A replacement within one stack
                // whose new top was not on it (a finished screen giving way to its result) arrives
                // as UIKit's own push, rather than cutting.
                let pushOrPop = NavigationRules.isPushOrPop(from: routes.map(ObjectIdentifier.init), to: stack.map(ObjectIdentifier.init))
                let arrives = stack.count > 1 && routes.first === stack.first
                    && !routes.contains { $0 === stack.last }
                #if os(iOS)
                // A route pushed from a source zooms from it (ZoomPushIOS).
                if (pushOrPop || arrives), stack.count > 1, let top = stack.last as? RouteController, !nav.viewControllers.contains(where: { $0 === top }) {
                    prepareZoomPush(top, beneath: stack[stack.count - 2] as? RouteController)
                }
                #endif
                nav.setViewControllers(nav.stackFront + stack, animated: (pushOrPop || arrives) && index == owners.count - 1 && mounted.count == boundaries.count && !ExactEnv.agentFreezes && !unanimated && nav.view.window != nil)
                lastTransition = "idle"
                // A press that pushed may hand its control's focus to the arriving route's first autofocus (PressHandoffIOS).
                if pushOrPop || arrives { presenter.pressShows((stack.last as? RouteController)?.node) }
            }
            ApplyProfile.time("nav.layout") { nav.view.layoutIfNeeded() }
        }
        if held {
            syncAfterBatch()
        } else {
            unanimated = false
        }
        if mounted.count > common {
            pendingSync = true
            presenter.modals.closeTop()
            return
        }
        if boundaries.count > mounted.count, let background = modalNavigation ?? primaryOwner, let owner {
            let part = parts[mounted.count + 1], route = wanted[part.lowerBound].node
            guard presenter.modals.canPresent(from: owner, route: route) else { return }
            // The sheet mounts its own navigation (`ModalHost.present`)
            let nav = makeNavigation(first: route)
            nav.view.frame = root.bounds
            presentedNavigations.append(nav)
            prepareRoutes(Array(wanted[part]), in: nav)
            nav.setViewControllers(Array(wanted[part]), animated: false)
            presenter.pressShows(wanted[part].last?.node) // a route pushed in the sheet as it opens
            watchPops(nav)
            nav.view.layoutIfNeeded()
            pendingSync = boundaries.count > mounted.count + 1
            let preceding = part.lowerBound > 0 ? wanted[part.lowerBound - 1].node : nil
            presenter.modals.present(route, navigation: nav, from: background,
                                     preceding: preceding, owner: owner)
        }
        // A presented navigation fills its sheet; the primary owner the root,
        // which no presentation resizes (LLP 1123 D1)
        if let top = primaryOwner {
            fitOwner()
            placeOwner()
            top.view.layoutIfNeeded()
        }
        for (id, c) in controllers where !routeIDs.contains(id) { end(c) }
        controllers = controllers.filter { routeIDs.contains($0.key) }
        presenter.modals.updatePermissions()
        if p.tabs != nil { replayTabs() }
        reportCovers()
        refitForBars()
        // A route that left every stack gives its block back (LLP 1123 D2)
        presenter.owners.publish()
        settled = true
    }

    /// The container standing in for the routes paints where they are in
    /// the root's children, as CSS paints siblings in tree order (LLP
    /// 1083.000): above the children before them, under those after them —
    /// an authored tablist, a toast. On top of every child it hid a sibling
    /// tablist and a root overlay, and took their taps (shop F21, recipes F23).
    func placeOwner() {
        guard let root = container, let owner = primaryOwner?.view, owner.superview === root else { return }
        let ids = logicalChildren[root.id] ?? []
        let panels = tabPanels.compactMap { presenter.views[$0] }
        let region = ids.firstIndex { id in
            routeIDs.contains(id) || presenter.views[id].map { child in panels.contains { $0 === child || $0.isDescendant(of: child) } } == true
        }
        let after = Set(region.map { ids[($0 + 1)...] } ?? [])
        guard let next = root.subviews.first(where: { ($0 as? NodeView).map { after.contains($0.id) } == true }) else {
            if root.subviews.last !== owner { root.bringSubviewToFront(owner) }
            return
        }
        let at = root.subviews.firstIndex { $0 === next }!
        if at == 0 || root.subviews[at - 1] !== owner { root.insertSubview(owner, belowSubview: next) }
    }

    /// A route's controller leaves for good: its header paints again and the
    /// route hatch hears `routeEnded` (LLP 1075.003 §3.2).
    private func end(_ c: RouteController) {
        if let header = c.lifted { header.isHidden = false; c.lifted = nil }
        c.scoped?.isHidden = false
        c.scoped = nil
        for node in c.barNodes { node.isHidden = false }
        c.barNodes = []
        guard c.hatched else { return }
        c.hatched = false
        presenter.session?.natives.routeHatch(.ended, controller: c, navigation: nil, scroll: nil, key: c.key, dataset: c.node.props["dataset"])
    }

    /// At a cold launch the module loads a turn after first pixel (LLP
    /// 1075.003 Q3 (c)): each hatch runs once for every object already live,
    /// stacks first, in the order they were built.
    func replayHatches() {
        guard presenter.session?.natives.hatchesConnected == true else { return }
        replayingHatches = true
        defer { replayingHatches = false }
        for nav in allNavigations { hatchNavigation(nav) }
        replayTabs()
        // Prepare plain tabs' routes too: their route hatches have not run either
        for stack in primaryStacks + presentedNavigations {
            prepareRoutes(stack.stackRoutes.compactMap { $0 as? RouteController }, in: stack)
            stack.view.layoutIfNeeded()
        }
        reportCovers()
        refitForBars()
    }

    /// UIKit changed a route's safe area: lay out its content in this call.
    func coversChanged() {
        // sync reports the final containment below; intermediate safe areas
        // belong to the controller the route is leaving.
        guard !syncing, tapping == nil, !coversPending else { return }
        coversPending = true
        presenter.afterBatch { [weak self] in
            guard let self else { return }
            coversPending = false
            reportCovers()
        }
    }

    /// What presents a sheet: the last sheet, else the primary owner's
    /// parent — with tabs the tab controller's, never a tab's own stack.
    var owner: UIViewController? { presenter.modals.owner ?? primaryOwner?.parent }
    /// The controller Exact installed under the root: the tab container, or
    /// the one stack.
    var primaryOwner: UIViewController? { tabOwner ?? primaryNavigation }

    func willMount() { mounting = true }

    func mounted() {
        mounting = false
        sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
    }

    func unmounted() {
        mounting = false
        let root = container, logical = logicalChildren
        reset(clearFocus: false)
        // Release controller containment while preserving the actual nodes.
        // Offscreen batches now reconcile their ordinary logical child list.
        guard let root, presenter.views[root.id] === root else { return }
        for (owner, ids) in logical {
            guard let parent = owner == root.id ? root : presenter.views[owner] else { continue }
            for (index, node) in ids.compactMap({ presenter.views[$0] }).enumerated() where presenter.views[node.id] === node {
                parent.container.insertSubview(node, at: index)
                presenter.regrid(node)
            }
        }
    }

    /// A sync deferred behind a running transition is owed to the press
    /// whose work it is (`owed`, else the live press while its work runs):
    /// that press's handoff waits for it. A newer press owns it instead.
    private func owePress(_ owed: PressHandoff? = nil) {
        guard let press = owed ?? presenter.pressHandoff.flatMap({ $0.working ? $0 : nil }) else { return }
        if pendingPress !== press { pendingPress?.pending = false; pendingPress?.settle() }
        press.pending = true
        pendingPress = press
    }

    /// Sync again once the batch being applied and the covers it lifted have
    /// been applied. The runtime's answer to the covers syncs first when it
    /// moves anything, and this sync is then owed nothing.
    private func syncAfterBatch() {
        pendingSync = true
        presenter.afterBatch { [weak self] in
            guard let self, pendingSync, !syncing else { return }
            pendingSync = false
            sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        }
    }

    func modalDidDismiss() {
        guard pendingSync else { return }
        pendingSync = false
        sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        presenter.syncModal()
    }

    /// Retire only the named owner; a late completion cannot remove its
    /// successor or the retained owner beneath it.
    func retireNavigation(_ controller: UIViewController, preserving: Bool) {
        guard let nav = controller as? UINavigationController, isPresented(nav) else { return }
        presentedNavigations.removeAll { $0 === nav }
        // A presentation gone (UIKit's own dismissal included): the next
        // batch syncs, whatever it holds, so a route still declared returns.
        nativeMoved = true
        retireStack(nav)
        nav.delegate = nil
        if !preserving {
            nav.willMove(toParent: nil)
            nav.view.removeFromSuperview()
            nav.removeFromParent()
        }
    }

    /// The removed editor stays in its outgoing presentation while UIKit
    /// dismisses it.
    func preserveModalContent(in parent: UIViewController) -> UIViewController? {
        guard let nav = presentedNavigations.first(where: { $0.parent === parent }) else { return nil }
        let frame = nav.view.convert(nav.view.bounds, to: parent.view)
        parent.view.insertSubview(nav.view, at: 0)
        nav.view.setPaintForeground(false)
        nav.view.frame = frame
        nav.view.accessibilityElementsHidden = true
        return nav
    }

    /// Focus waits through installation and push/pop unless it targets the arriving route.
    /// `clock settle` observes the asynchronous part under platform timing.
    var defersFocus: Bool { changing || syncing || mounting }
    var inTransition: Bool { defersFocus || pendingSync }

    /// An explicit focus request may reach the arriving screen once UIKit mounts it.
    func isArriving(_ node: NodeView) -> Bool {
        guard node.headerField == nil, !syncing, !mounting, let navigation, let transition = pushOrPop(on: navigation),
              let route = transition.viewController(forKey: transition.isCancelled ? .from : .to) as? RouteController else { return false }
        return presenter.authored(node, inside: route.node)
    }

    /// How the last transition ended, for the agent's `state.navigation`: `completed` once a pop, a swipe's
    /// included, reached the router (`followPops`, `transitionEnded`), `cancelled` for a swipe that returned,
    /// `idle` once Exact writes a change the app asked for.
    var lastTransition = "idle"
    /// Whether UIKit is pushing or popping the stack navigation follows while
    /// it is on screen, from the transition's first frame through `didShow`
    /// (before `willShow` registers the coordinator's completion), or moving
    /// a route of the router's selected stack (`routesInFlight`): a pop it
    /// committed that the router has not followed yet, a swipe under way, or
    /// More's pop to its list. A transition on a stack off screen does not
    /// count: a tab tapped in the turn of its pop leaves the coordinator
    /// there until the tab shows again (LLP 1075.003 §9.16), and a write to
    /// that stack waits for it (`writable`).
    var changing: Bool {
        navigation.map { $0.viewIfLoaded?.window != nil && pushOrPop(on: $0) != nil } ?? false || routesInFlight(selectedStack)
    }
    var hasInteractiveTransition: Bool { allNavigations.contains { $0.transitionCoordinator?.isInteractive == true } }

    /// A push or pop, or any stack's controller transition, including a sheet presentation.
    var transitioning: Bool {
        // A rotation's or resize's coordinator moves no controller: not one.
        changing || ([primaryNavigation].compactMap { $0 } + presentedNavigations).contains { $0.transitionCoordinator?.viewController(forKey: .from) != nil }
    }
    /// For `state.navigation` (LLP 1035.002 D2): the route the root names,
    /// UIKit's stack by key, and the transition's phase — observations.
    func observation() -> [String: Any] {
        // Scope Back to its container: Duo places the control outside the navigation bar
        let nativeBack: String? = {
            if let more = moreListBack { return more.isNavigationBarHidden ? nil : primaryOwner?.view.accessibilityIdentifier }
            guard let nav = navigation, nav.viewIfLoaded?.window != nil, !nav.isNavigationBarHidden,
                  nav.viewControllers.count > 1, canInvokeBack else { return nil }
            let owner: UIViewController? = nav === primaryNavigation ? primaryOwner : nav
            return owner?.view.accessibilityIdentifier
        }()
        let stack = (navigation?.viewControllers ?? primaryPlain?.stackRoutes ?? []).compactMap { ($0 as? RouteController)?.key }
        let transition: [String: Any] = ["interactive": navigation?.transitionCoordinator?.isInteractive ?? false,
                                         "phase": changing || pendingSync || presenter.modals.inTransition ? "in-progress" : lastTransition]
        let root = container ?? presenter.root.subviews.first as? NodeView
        let top = navigation?.topViewController as? RouteController
        return ["route": root?.props["navigationKey"] ?? NSNull(), "stack": stack, "transition": transition,
                "nativeBack": nativeBack ?? NSNull(), "barHeight": Agent.r2(navigation?.navigationBar.bounds.height ?? 0),
                "contentScroller": top.flatMap { contentScroller(of: $0).map { Int($0.id) } } ?? NSNull(),
                "collapseScroller": top?.collapseScroll.map { Int($0.id) } ?? NSNull(),
                "arrangedScrollers": top.map { arrangementScrollers(in: $0, top: true).map { Int($0.id) } } ?? []]
    }

    /// Whether a route or a logical ancestor of it (`eventParent`) is
    /// `display: none`.
    func logicallyHidden(_ route: NodeView) -> Bool {
        presenter.logicalAncestry(of: route).prefix { $0 !== presenter.root }
            .contains { ($0 as? NodeView)?.style["display"]?.string == "none" }
    }

    /// A history jump uses navigation; the authored Back control handles the adjacent intent.
    func goBack(from source: String, to destination: String) {
        pendingSync = true
        guard let session = presenter.session, let root = container,
              let location = session.runtime.visitLocation(destination) else {
            presenter.session?.log("Back history refused: the destination has no router visit; the declared stack is restored")
            return
        }
        if root.handlers.contains("navigate") {
            session.navigate(location, at: root)
            if selectedKey == source { session.log("Back history: the navigation handler has not changed the selected visit; the declared stack is restored") }
        } else {
            session.hostBack(from: source, to: destination)
            if selectedKey != destination { session.log("Back history: the requested destination is not selected; the declared stack is restored") }
        }
    }

    /// The key of the route a view is mounted under, for `layout <node>`
    /// (LLP 1035.002 D1) — an observation of UIKit's containment.
    func routeKey(containing view: UIView) -> String? {
        ViewPresentation.controllers(for: view).compactMap { ($0 as? RouteController)?.key }.first
    }

    func isInactiveRoute(containing view: UIView) -> Bool {
        guard let selected = selectedKey,
              let key = routeKey(containing: view) else { return false }
        return key != selected
    }

    /// Where each pop recognizer's first touch went down, in its view. A
    /// pan's translation at `shouldBegin` leaves out the travel before it
    /// recognized: a real touch from x = 1 read as starting at 30, past the
    /// 20-point edge, and over a `swiperight` row the pop was refused (LLP
    /// 1080.000 §11). The edge rule judges where the finger landed.
    /// Keyed weakly: a retired stack's recognizers take their entries with them.
    private let popTouchDown = NSMapTable<UIGestureRecognizer, NSValue>.weakToStrongObjects()

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        // The first finger of each gesture replaces the last gesture's; a second finger does not.
        if gestureRecognizer.numberOfTouches == 0 { notePopTouchDown(gestureRecognizer, at: touch.location(in: gestureRecognizer.view)) }
        return true
    }
    func notePopTouchDown(_ gestureRecognizer: UIGestureRecognizer, at point: CGPoint) { popTouchDown.setObject(NSValue(cgPoint: point), forKey: gestureRecognizer) }

    /// The swipe's start: its first touch's point, else (no touch seen) its translation's origin.
    func popStart(_ pan: UIPanGestureRecognizer, in view: UIView) -> CGPoint {
        if let down = popTouchDown.object(forKey: pan)?.cgPointValue { return down }
        let location = pan.location(in: view), delta = pan.translation(in: view)
        return CGPoint(x: location.x - delta.x, y: location.y - delta.y)
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard let pan = gestureRecognizer as? UIPanGestureRecognizer, let view = pan.view else {
            return popMayBegin(gestureRecognizer, from: nil, in: nil, velocity: .zero)
        }
        // A pan's velocity at shouldBegin is often zero (a finger that
        // started slowly, or one sample in): read that way every back swipe
        // is refused as not horizontal. Its travel so far says the direction.
        return popShouldBegin(pan, in: view, velocity: NavigationRules.popDirection(velocity: pan.velocity(in: view), travel: pan.translation(in: view)))
    }
    /// `shouldBegin` for a pan at `velocity`: from where its finger landed.
    func popShouldBegin(_ pan: UIPanGestureRecognizer, in view: UIView, velocity: CGPoint) -> Bool {
        popMayBegin(pan, from: popStart(pan, in: view), in: view, velocity: velocity)
    }

    /// Whether a pop recognizer's swipe, starting at `start` in `view` (a
    /// pan's) with `velocity`, may pop: the showing stack's (a tab's, or a
    /// sheet's over it), by its own depth, with an enabled back control in
    /// its active route (D1), and not a pan a `swiperight` node or a canvas
    /// owns, nor more vertical than horizontal.
    func popMayBegin(_ gestureRecognizer: UIGestureRecognizer, from start: CGPoint?, in view: UIView?, velocity: CGPoint) -> Bool {
        // tvOS has no interactive pop gesture.
        #if os(tvOS)
        let owner: UINavigationController? = nil, toList = false
        #else
        let pops = { (nav: UINavigationController) -> Bool in
            if nav.interactivePopGestureRecognizer === gestureRecognizer { return true }
            if #available(iOS 26.0, *) { return nav.interactiveContentPopGestureRecognizer === gestureRecognizer }
            return false
        }
        let owner = (allNavigations + [more].compactMap { $0 }).first(where: pops)
        // Let More's swipe back to its list from a tab's root or a plain tab begin: it is UIKit's own, and no route pops.
        // On More's list alone there is nothing to pop.
        let toList = owner != nil && owner === more && (more?.topViewController is PlainTab || (navigation === more && more?.stackRoutes.count == 1))
        #endif
        if let owner, owner !== navigation, !toList { return false }
        let depth = (toList ? owner : navigation)?.viewControllers.count ?? 0
        let goes = toList || back != nil
        guard NavigationRules.popMayBegin(depth: depth, changing: changing, modalActive: presenter.modals.inTransition,
                                          hasBackControl: goes,
                                          contextPreviewActive: !presenter.chrome.ids("contextTarget").isEmpty) else {
            if depth > 1, !changing, !presenter.modals.inTransition, !goes {
                presenter.session?.log("back gesture refused: \(backRefusal)")
            }
            return false
        }
        guard let start, let view else { return true }
        var overSwipeRight = false
        var hit = view.hitTest(start, with: nil)
        if CanvasInputs.owns(hit) { return false }
        while let current = hit {
            // A rightward pan is a `swiperight` node's, and a `swipeleft`
            // node's while its swipe is caught left of rest, on its way back.
            if let node = current as? NodeView, node.handlers.contains("swiperight")
                || (node.handlers.contains("swipeleft") && node.translate.x < -0.5) { overSwipeRight = true; break }
            if current === view { break }
            hit = current.superview
        }
        return NavigationRules.panMayBegin(startX: start.x, overSwipeRight: overSwipeRight, velocity: velocity)
    }

    /// UIKit's push or pop on `nav`, while it runs: its coordinator presents
    /// nothing (a sheet's has a presentation style; a rotation's is not the
    /// stack's) and moves the stack's top controller in or out. The top is
    /// the destination until a cancelled swipe's rollback ends, then the
    /// source again in its `didShow`.
    private func pushOrPop(on nav: UINavigationController) -> UIViewControllerTransitionCoordinator? {
        guard let transition = nav.transitionCoordinator, transition.presentationStyle == .none else { return nil }
        let top = nav.topViewController
        return transition.viewController(forKey: .to) === top || transition.viewController(forKey: .from) === top ? transition : nil
    }

    func navigationController(_ navigationController: UINavigationController, willShow viewController: UIViewController, animated: Bool) {
        // Follow a pop UIKit committed (its Back button, a tap on the selected tab, More's pop to its list) as it
        // begins, before the call's other work; a swipe commits only at its end
        if navigationController.transitionCoordinator?.initiallyInteractive != true { followPops() }
        if navigationController === more { followMore(viewController, animated: animated) }
        // Run a sync this stack's transition held at its end, whichever stack navigation follows by then
        navigationController.transitionCoordinator?.animate(alongsideTransition: nil) { [weak self] _ in self?.runHeldSync() }
        guard navigationController === navigation else { return }
        HostLog.note(presenter.session, {
            let by = navigationController.transitionCoordinator?.isInteractive == true ? "the edge swipe" : "UIKit or the app"
            return "navigation: willShow \((viewController as? RouteController)?.key ?? "?") (\(animated ? "animated" : "unanimated"), by \(by))"
        }())
        // The route coming into view was out of the window while it was
        // covered, so no scroll asked for its text; paint what shows of it
        // as the transition starts, not when the reader next scrolls.
        navigationController.transitionCoordinator?.animate(alongsideTransition: { [weak self] _ in self?.presenter.paintVisibleText() })
        // @ref LLP 1038 D6/D11; LLP 1035.001 D2 — a cancelled swipe calls
        // willShow again for its source, in the transition already begun.
        guard let transition = pushOrPop(on: navigationController), transition.viewController(forKey: .to) === viewController else { return }
        // UIKit's own (its Back button, its menu, a swipe): Observe's render
        // mark is this transition's first frame, unless it is cancelled.
        if !syncing {
            NavigationMarks.shared.platformBeganShowing()
            transition.animate(alongsideTransition: nil) { context in
                if context.isCancelled { NavigationMarks.shared.platformCancelled() }
            }
        }
        // As the swipe begins: an update may remove its source's node while
        // the finger is down
        let swipe: Swipe? = transition.initiallyInteractive
            ? (transition.viewController(forKey: .from) as? RouteController).map { ($0.node, $0.key, navigationController.viewControllers.count + 1) }
            : nil
        transition.animate(alongsideTransition: nil) { [weak self, weak navigationController] context in
            guard let self, let navigationController else { return }
            transitionEnded(navigationController, swipe: swipe, cancelled: context.isCancelled)
        }
        // That paints what shows as it starts. A swipe reveals the route a
        // strip at a time, and painting stopped there until the swipe ended
        // (a back swipe showed the list's top half, then its bottom half).
        // Paint each frame's newly revealed text until the transition is over.
        startRevealing()
    }

    private func startRevealing() {
        guard !FrameClock.shared.wants(self) else { return }
        FrameClock.shared.want(self, .navigationReveal, rate: FrameClock.full(on: presenter.viewport.window?.screen)) { [weak self] _ in self?.revealTick() }
    }

    private func revealTick() {
        guard changing else { stopRevealing(); return }
        presenter.paintVisibleText()
    }

    private func stopRevealing() {
        FrameClock.shared.drop(self)
    }

    func navigationController(_ navigationController: UINavigationController, didShow viewController: UIViewController, animated: Bool) {
        followPops()
        if navigationController === more {
            followSelection(showing: viewController)
            if declineMoreRow(viewController) { return }
        }
        guard navigationController === navigation, navigationController.topViewController === viewController else {
            // Run the sync a transition held when a stack navigation does not follow reports its end: More's pop to
            // its list, or a stack shown again after its pop ended off screen (LLP 1075.003 §9.16)
            runHeldSync()
            return
        }
        HostLog.note(presenter.session, "navigation: didShow \((viewController as? RouteController)?.key ?? "?")")
        // An animated push or pop ends in its coordinator's completion
        guard pushOrPop(on: navigationController) == nil else { return }
        transitionEnded(navigationController, swipe: nil, cancelled: false)
    }

    /// An interactive pop's source and the stack's depth before it began.
    typealias Swipe = (node: NodeView, key: String, depth: Int)

    /// A push or pop of the stack navigation follows is over: a swipe that
    /// completed reaches the router (LLP 1035.001 D2), and the deferred work
    /// runs. One on a stack navigation stopped following (another tab was
    /// selected, or the stacks were reset) is not navigation's.
    private func transitionEnded(_ navigationController: UINavigationController,
                                 swipe: Swipe?, cancelled: Bool) {
        // Finish the departed search field while its authored handlers still exist
        if !cancelled, navigationController === navigation {
            for route in poppedByUIKit() where route.lastStack === navigationController {
                if route.search?.endEditing(completion: { [weak self, weak navigationController] in
                    guard let self, let navigationController else { return }
                    completeTransition(navigationController, swipe: swipe, cancelled: cancelled)
                }) == true { return }
            }
        }
        completeTransition(navigationController, swipe: swipe, cancelled: cancelled)
    }

    private func completeTransition(_ navigationController: UINavigationController,
                                    swipe: Swipe?, cancelled: Bool) {
        guard navigationController === navigation, let viewController = navigationController.topViewController else { return }
        nativeMoved = true
        stopRevealing()
        // The route a pop took away keeps its block until it leaves the stack (LLP 1123 D2)
        defer { presenter.owners.publish() }
        defer {
            // Tree updates during UIKit's transition retain their latest
            // intent. Apply it once the native stack is available again.
            runHeldSync()
            presenter.session?.view?.fit()
            presenter.syncModal()
            // What the settled route shows has pixels (the swipe may have
            // left a band of it unpainted, or a cancelled pop the source).
            presenter.paintVisibleText()
            presenter.flushPendingFocus()
            presenter.elementScrolls.flush()
            #if os(iOS)
            presenter.syncScrollsToTop() // the route now on top owns the status-bar tap
            #endif
            // Settled on a stack's root: once UIKit has finished the
            // transition (its own bar restoration included), a root whose
            // arrival no projection has handled yet reconciles the bar with
            // its tablist (§3.7).
            DispatchQueue.main.async { [weak self, weak navigationController] in
                if let navigationController { self?.settleTablist(navigationController) }
                self?.presenter.menus.resumeAfterRouteDismissal()
            }
            // At rest: a large title's insets are sampled now (§9.10).
            coversChanged()
            #if os(iOS)
            presenter.resolveStatusBar(settled: true)
            #endif
        }
        // A cancelled pop performed no Back, whatever the app did to its source meanwhile
        if cancelled {
            lastTransition = "cancelled"
            return
        }
        guard let swipe else {
            followPops()
            return
        }
        // Press Back once for a swipe, while its source is still the selected route, or for the route that replaced
        // it in place while the finger was down: then the source's node is gone and the app's stack is as deep as it
        // was when the swipe began (LLP 1035.001 D2)
        let sourceKey = presenter.views[swipe.node.id] === swipe.node ? swipe.key : nil
        let sourceReplaced = sourceKey == nil && selectedRouteCount == swipe.depth
        let dispatches = (viewController as? RouteController).map {
            NavigationRules.dispatchesBack(shownKey: $0.key, rootKey: selectedKey ?? "", sourceKey: sourceKey,
                                           sourceReplaced: sourceReplaced, modalActive: presenter.modals.inTransition)
        } ?? false
        guard dispatches else {
            followPops()
            return
        }
        for route in poppedByUIKit() where route.lastStack === navigationController { route.held = false }
        lastTransition = "completed"
        goBack()
    }

    /// A stack Exact retired: its handle goes.
    func retireStack(_ nav: UINavigationController) {
        guard let stack = stacks.removeValue(forKey: ObjectIdentifier(nav)), stack.hatched else { return }
        _ = presenter.session?.natives.navigationHatch(nav, built: false, showsBar: stack.showsBar, label: stack.label)
    }

    func reset(clearFocus: Bool = true) {
        unanimated = false
        presenter.modals.reset()
        for nav in presentedNavigations { retireNavigation(nav, preserving: false) }
        for c in controllers.values { end(c) }
        retireTabs()
        if let primaryNavigation { retireStack(primaryNavigation) }
        if !covers.isEmpty || !delivered.isEmpty || !localInsets.wanted.isEmpty {
            covers = [:]; localInsets.wanted = [:]
            presenter.afterBatch { [weak self] in self?.deliverCovers() }
        }
        primaryNavigation?.delegate = nil
        primaryNavigation?.willMove(toParent: nil)
        primaryNavigation?.view.removeFromSuperview()
        primaryNavigation?.removeFromParent()
        primaryNavigation = nil
        container = nil
        logicalChildren = [:]
        shownKeys = []; fallbackKey = nil
        refitForBars()
        for controller in controllers.values { presenter.pressHandoff?.drop(controller.node) } // reset: no route of a press stays
        controllers.removeAll()
        routeIDs = []
        pendingSync = false
        pendingPress?.pending = false; pendingPress?.settle(); pendingPress = nil
        lastTransition = "idle"; reselected = nil; tabsHeld = false
        if clearFocus { presenter.cancelPendingFocus() }
        #if os(iOS)
        scrollsToTopSelection?.scrollsToTop = false
        scrollsToTopSelection = nil
        #endif
        // The routes no stack holds now give their blocks back (LLP 1123 D2)
        presenter.owners.publish()
    }
}
#endif
