// A route owns its content viewport and projects its scroll edges to UIKit.
#if os(iOS) || os(tvOS)
import UIKit

final class RouteController: UIViewController, UIScrollViewDelegate {
    let node: NodeView
    var key: String { node.props["navigationKey"] ?? "" }
    var usesPageViewport = false
    var pageScrollAxes: (x: Bool, y: Bool)?
    private(set) var contentViewport: Viewport?
    private let keyboardInset = KeyboardInset()
    weak var bootSurface: UIView?
    var surface: UIView { bootSurface ?? view }
    var contentMount: UIView { contentViewport ?? surface }
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
        let offset = node.convert(CGPoint(x: 0, y: surface.safeAreaInsets.top), from: surface).y
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
        // UIKit sized its block (LLP 1123 D2); a deferred boot already published from its holder.
        host?.presenter.owners.publish()
        // Run the edge passes that an app's selection owes it, whatever is under way, before the transition's first frame;
        // the covers follow below
        host?.runOwedScrollEdges(of: self)
        // Finish the destination's content and focus before UIKit displays it.
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
        if scrollEdgesOwed { host?.settleScrollEdges() }
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
    /// The controller's view paints what shows behind the route: a dynamic colour resolved for the
    /// controller's current appearance, re-resolved by UIKit when it changes (a route loaded before
    /// its window has a trait collection would otherwise keep the light colour in dark mode), and
    /// written again when a batch changes it. A pushed route shows its backdrop (`backdrop`); a
    /// route under no background shows the system background, not white. A sheet is the route's own
    /// background where it has one, as a grouped sheet's surface is the grouped background (LLP
    /// 1116.021 D7, amending LLP 1115 D2), so no white band shows where the route stops above a
    /// keyboard's bar; else clear, over the surface its presentation paints
    /// (`ModalController.paintSurface`: UIKit's glass at a partial detent, LLP 1141.001 D3). What
    /// is behind the sheet is not, and dimming belongs to UIKit's presentation. A full-screen route
    /// that paints its own background is clear, as its presentation is (`NodeView.paintsOverPage`):
    /// the route paints it once, over the page.
    func paintBackdrop() {
        guard isViewLoaded else { return }
        let view = surface
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
        let view = surface
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
    /// The route's room for the keys, and its page viewport's frame, extent and inset.
    func fitContentViewport(in block: CGRect? = nil) {
        let view = surface
        #if os(iOS)
        // Under a search in the bar the route keeps its layout (`block(in:)`)
        // and takes the keys as safe area, as SwiftUI's keyboard region is:
        // UIKit insets its edge scroller so the last rows reach above them.
        // A visual viewport takes them as its inset instead, without reflow.
        if let presenter = host?.presenter {
            var keys: CGFloat = 0
            if contentViewport == nil || presenter.keyboardPolicy != "resizes-visual", presenter.editsInBar(of: view), let top = presenter.keyboardTop, let window = view.window {
                let edge = view.convert(CGPoint(x: 0, y: top), from: window).y
                // Extend the inherited safe area (including a row beside the tabs) to the keys;
                // the keyboard and that row obstruct the same edge.
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
        scroll.scrollsX = pageScrollAxes?.x ?? true
        scroll.scrollsY = pageScrollAxes?.y ?? true
        let extent = CGSize(width: max(node.bounds.width, node.style["overflow_x"]?.string == "visible" || node.style["overflow_x"] == nil ? node.content.width : 0),
                            height: max(node.bounds.height, node.style["overflow_y"]?.string == "visible" || node.style["overflow_y"] == nil ? node.content.height : 0))
        // An axis the root does not scroll keeps its extent: CSS `overflow: hidden` still
        // scrolls programmatically (a fragment, `scrollIntoView`). The person's drag
        // cannot move it (`scrollViewDidScroll`).
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
        guard let scroll = contentViewport, scrollView === scroll, let presenter = host?.presenter else { return }
        if let origin = dragOrigin {
            let held = CGPoint(x: scroll.scrollsX ? scroll.contentOffset.x : origin.x, y: scroll.scrollsY ? scroll.contentOffset.y : origin.y)
            if held != scroll.contentOffset { scroll.contentOffset = held; return }
        }
        presenter.owners.publish()
        presenter.stickies.scrolled(node.id)
        presenter.transformGeometry.changed()
        presenter.videoVisibility?.changed()
        presenter.scrollPump.scrolled(nil)
        FrameSampler.scrolled(scrollView, in: presenter.session, person: false)
        presenter.session?.frameHealth.trackScrolled(scrollView)
        node.repaintThrough()
    }

    /// Where the person's drag of the page viewport began, until it comes to rest.
    private var dragOrigin: CGPoint?
    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        if scrollView === contentViewport { dragOrigin = scrollView.contentOffset }
        host?.presenter.elementScrolls.userScrolled(scrollView)
        if let session = host?.presenter.session { session.frameHealth.beginScroll(scrollView, session: session) }
    }
    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate { dragOrigin = nil; host?.presenter.session?.frameHealth.endScroll(scrollView) }
    }
    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        dragOrigin = nil
        host?.presenter.session?.frameHealth.endScroll(scrollView)
    }

    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        host?.presenter.elementScrolls.revealEnded(scrollView)
    }

    /// The tabpanel the route was last projected from (`NavigationHost.tabIndex(ofRoute:)`).
    var panel: UInt32?

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
    /// The exit it declares, kept while it is its stack's top: it has left
    /// the tree when UIKit asks for its pop's animator (RouteTransitionIOS).
    var exitPart: RoutePart?
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
    /// An app changed the top or bottom selection, and the edge passes and covers that follow are owed
    /// (`NavigationHost.scrollEdgesChanged`)
    var scrollEdgesOwed = false
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
        let edges = edge.intersection([.top, .bottom])
        if let scrollView {
            let changed = !nativeContentScrollEdges.isSuperset(of: edges)
                || (edges.contains(.top) && contentScrollView(for: .top) !== scrollView)
                || (edges.contains(.bottom) && contentScrollView(for: .bottom) !== scrollView)
            // Record only the top and bottom edges: UIKit takes no other
            nativeContentScrollEdges.formUnion(edges)
            select(scrollView, for: edge)
            if changed { host?.scrollEdgesChanged(self) }
        } else {
            // An app's nil makes the edges it selected automatic again, and the navigation host
            // selects them as for a route no app touched. UIKit gets the nil first: the passes take
            // an edge's current scroll view for Exact's last selection and would undo its insets.
            let released = nativeContentScrollEdges.intersection(edge)
            guard !released.isEmpty else { return }
            nativeContentScrollEdges.subtract(released)
            select(nil, for: released)
            host?.scrollEdgesChanged(self)
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
    /// iOS 26 draws the bar's edge effect on the outermost scroll view under it, which for a route
    /// whose scroller (a list) lies in its page is the page's viewport, not that scroller (measured
    /// on iOS 27: UIKit alone does the same with a table view in a scroll view). So the viewport's
    /// top effect shows only while that scroller is scrolled from its top, as UIKit's does over a
    /// list that is the controller's own: no effect over a list at rest. Its style stays
    /// `automatic`: the rows blurred and faded under the whole bar to a line at its foot, as a
    /// UIKit or SwiftUI list or scroll view draws them there (iOS 27, measured), where `soft` left
    /// them legible under the title.
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

#endif
