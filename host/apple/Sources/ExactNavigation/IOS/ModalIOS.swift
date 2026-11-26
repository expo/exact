// @ref LLP 1008 §9 — UIKit owns modal presentation and its keyboard guide.
// @ref LLP 1123 D2 — a presentation mounts only its own navigation: the
// session's viewport and the document stay in the session's view.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

#if os(iOS)
/// A presented controller that paints its own surface, which a live theme
/// edit has it paint again (`Presenter.applyThemeTint`).
extension ModalController: PaintsSheetSurface {}
#endif

final class ModalController: UIViewController, UIGestureRecognizerDelegate {
    weak var host: ModalHost?
    let routeID: UInt32
    /// The route a `fit-content` detent measures (LLP 1075.003 §9.11).
    private weak var route: NodeView?
    var retiringRoot: NodeView?
    var retiringNavigation: UIViewController?
    /// The keyboard probe and a freeze's pixels: what this controller adds
    /// to its view beside the mount (LLP 1080.001 D3).
    private(set) var probe: UIView?
    private(set) var pixels: UIView?
    /// What the presentation's navigation is mounted in.
    let mount = PresentationMount()
    /// A full-screen route's own presentation and dismissal
    /// (RouteTransitionIOS), kept here: UIKit holds the delegate weakly.
    var routeTransitions: RoutePresenting? { didSet { transitioningDelegate = routeTransitions } }
    init(host: ModalHost, route: NodeView, kind: String, detent: String?, grabber: String?) {
        self.host = host
        routeID = route.id
        self.route = route
        super.init(nibName: nil, bundle: nil)
        #if os(tvOS)
        // tvOS presents a modal route over the full screen (LLP 1123 §"Host matrix"): it has no page sheet, popover or sheet controller
        modalPresentationStyle = .overFullScreen
        #else
        switch kind {
        case "fullscreen": modalPresentationStyle = .overFullScreen
        // A popover adapts to a sheet in a compact-width window, as UIKit adapts it (`PopoverDelegate`)
        case "popover": modalPresentationStyle = .popover
        default:
            modalPresentationStyle = ModalController.sheetStyle(route.props["navigationSizing"])
            updateSheet(detent: detent, cornerRadius: ModalController.cornerRadius(of: route), grabber: grabber)
        }
        #endif
    }
    /// A sheet route's presentation, its `navigationSizing` as it is presented (LLP 1123 D7): UIKit's own choice
    /// unsaid or `automatic`, which resolves a form sheet on iOS 18 and later and a page sheet before, as UIKit
    /// presents a controller that names no style; `page` its page sheet and `form` its form sheet. tvOS presents a route over the full screen (§"Host matrix").
    static func sheetStyle(_ sizing: String?) -> UIModalPresentationStyle {
        #if os(tvOS)
        .overFullScreen
        #else
        switch sizing {
        case "page": .pageSheet
        case "form": .formSheet
        default: .automatic
        }
        #endif
    }
    /// Presented as a sheet, at whichever of its sizings
    private var isSheet: Bool { modalPresentationStyle != .overFullScreen }
    /// The sheet rests at or below its undimmed detent, so it dims nothing and is no modal dialog: the screen under it
    /// stays live (LLP 1123 D7). Its detents ascend, as Exact keeps them.
    var restsUndimmed: Bool {
        #if os(tvOS)
        return false
        #else
        guard isSheet, let sheet = sheetPresentationController, let undimmed = sheet.largestUndimmedDetentIdentifier else { return false }
        let detents = sheet.detents.map(\.identifier)
        guard let top = detents.firstIndex(of: undimmed),
              let at = (sheet.selectedDetentIdentifier ?? detents.first).flatMap(detents.firstIndex(of:)) else { return false }
        return at <= top
        #endif
    }
    /// A sheet route's authored `border-radius`, its top-left corner's: the
    /// sheet's corner radius. Nil leaves UIKit's, which on iOS 26 and later
    /// follows the display's corners and insets a large sheet from the bottom edge.
    static func cornerRadius(of route: NodeView) -> CGFloat? {
        guard route.wrote("border_radius_top_left") else { return nil }
        return BorderPaint.radii(route.style, in: route.bounds)[0].width
    }
    private var detentValue: String?, cornerRadius: CGFloat?, grabberValue: String?
    /// The route's other sheet choices as last applied (LLP 1123 D7): the detent it holds the sheet at, its edge
    /// attachment in a compact height, whether it then takes its content's preferred width, and its scroll expansion.
    private var choices = SheetChoices()
    private struct SheetChoices: Equatable {
        var selected: String?, undimmed: String?, edgeAttached: String?, widthFollows: String?, scrollExpands: String?
        init(_ props: [String: String]? = nil) {
            selected = props?["navigationSelectedDetent"].flatMap { $0.isEmpty ? nil : $0 }
            undimmed = props?["navigationUndimmedDetent"].flatMap { $0.isEmpty ? nil : $0 }
            edgeAttached = props?["navigationEdgeAttachedInCompactHeight"]
            widthFollows = props?["navigationWidthFollowsPreferredContentSize"]
            scrollExpands = props?["navigationScrollExpands"]
        }
    }
    func updateSheet(detent value: String?, cornerRadius radius: CGFloat?, grabber: String?) {
        #if !os(tvOS)
        let next = SheetChoices(route?.props)
        // No detent selected yet: UIKit's own (`large`), never set here, as
        // when the sheet is made; a selection is set with every detent list.
        guard isSheet, let sheet = sheetPresentationController,
              detentValue != value || cornerRadius != radius || grabberValue != grabber || choices != next
                || sheet.selectedDetentIdentifier == nil else { return }
        // A grabber or radius alone changes only itself: the detents and the
        // one the person dragged the sheet to stay.
        let newDetents = detentValue != value || sheet.selectedDetentIdentifier == nil
        // A held detent moves the sheet only as it changes or over new detents; let go, the sheet returns to the
        // one it opens at, as unsaid
        let held = next.selected != choices.selected || newDetents ? next.selected : nil
        let released = choices.selected != nil && next.selected == nil
        detentValue = value
        cornerRadius = radius
        grabberValue = grabber
        choices = next
        let configure = {
            if let route = self.route {
                let owner = "route \"\(route.props["navigationKey"] ?? "")\""
                if newDetents || released { SheetDetents.select(on: sheet, node: route, owner: owner) }
                if let held { SheetDetents.hold(sheet, at: held, node: route, owner: owner) }
                sheet.largestUndimmedDetentIdentifier = SheetDetents.undimmed(next.undimmed, node: route, owner: owner)
            }
            SheetDetents.updateGrabber(grabber, on: sheet)
            if let expands = next.scrollExpands { sheet.prefersScrollingExpandsWhenScrolledToEdge = expands == "true" }
            sheet.prefersEdgeAttachedInCompactHeight = next.edgeAttached == "true"
            sheet.widthFollowsPreferredContentSizeWhenEdgeAttached = next.widthFollows == "true"
            sheet.preferredCornerRadius = radius
            // In `animateChanges`, the surface moves with the sheet.
            self.paintSurface()
        }
        if viewIfLoaded?.window != nil { sheet.animateChanges(configure) }
        else { configure() }
        #endif
    }
    /// The route's content extent changed: a `fit-content` detent resolves
    /// again, animated as UIKit animates a detent change, in the same sheet.
    func contentChanged() {
        #if !os(tvOS)
        guard isSheet, let sheet = sheetPresentationController,
              (detentValue ?? "").split(separator: " ").contains("fit-content") else { return }
        if viewIfLoaded?.window != nil { sheet.animateChanges { sheet.invalidateDetents() } }
        else { sheet.invalidateDetents() }
        #endif
    }
    required init?(coder: NSCoder) { nil }
    #if os(iOS)
    // LLP 1105 D6: UIKit asks a presented controller that covers the screen;
    // it answers the presenter's one resolved style and hiding, as the root does.
    override var preferredStatusBarStyle: UIStatusBarStyle { host?.presenter.statusBar.style ?? .default }
    override var prefersStatusBarHidden: Bool { host?.presenter.statusBar.hidden ?? false }
    override var prefersHomeIndicatorAutoHidden: Bool { host?.presenter.statusBar.hidden ?? false }
    override var preferredStatusBarUpdateAnimation: UIStatusBarAnimation { host?.presenter.statusBarUpdateAnimation ?? .none }
    // LLP 1133.029 D5: and the session's orientation lock, as the root does.
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
        host?.presenter.orientationLock.supported ?? super.supportedInterfaceOrientations
    }
    @available(iOS 26, *)
    override var prefersInterfaceOrientationLocked: Bool { host?.presenter.orientationLock.locked ?? false }
    #endif
    private var backdropTap: UITapGestureRecognizer?
    /// Each appearance, the first and one after a cancelled interactive dismissal.
    var appeared: (() -> Void)?
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        appeared?()
        #if os(iOS)
        // Back after a cancelled drag or zoom dismissal, which no completion
        // resolves: what landed during the gesture shows now (LLP 1105 D5).
        // The finished coordinator may still be installed: only a newly
        // started push or pop holds the style.
        let resolve = { [weak self] in self?.host?.presenter.resolveStatusBar(settled: true) }
        if !(transitionCoordinator?.animate(alongsideTransition: nil, completion: { _ in resolve() }) ?? false) { resolve() }
        #endif
        guard backdropTap == nil, let container = presentationController?.containerView else { return }
        let tap = UITapGestureRecognizer(target: self, action: #selector(tappedBackdrop))
        tap.delegate = self
        container.addGestureRecognizer(tap)
        backdropTap = tap
    }
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        if CanvasInputs.owns(touch.view) { return false }
        guard let container = presentationController?.containerView,
              let presented = presentationController?.presentedView,
              host?.canDismissByBackdrop(routeID) == true else { return false }
        return !presented.convert(presented.bounds, to: container).contains(touch.location(in: container))
    }
    @objc private func tappedBackdrop() { host?.dismissByBackdrop(routeID) }
    override func loadView() {
        view = UIView()
        mount.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(mount)
        #if os(tvOS)
        // tvOS has neither grouped backgrounds nor a keyboard layout guide.
        view.backgroundColor = .white
        #else
        ThemeRoles.adopt(view, from: host?.presenter.root)
        // Its presenter's tint, as the root's (LLP 1095 D4): a sheet sits outside it.
        if let tint = ThemeRoles.color("AccentColor", in: view) { view.tintColor = tint }
        paintSurface()
        registerForTraitChanges([UITraitHorizontalSizeClass.self]) { (controller: ModalController, _: UITraitCollection) in controller.paintSurface() }
        let probe = UIView()
        probe.isHidden = true
        probe.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(probe)
        self.probe = probe
        // The guide is the keys alone: a route ends at them, not at the home
        // indicator (`OwnerBlocks.block`)
        view.keyboardLayoutGuide.usesBottomSafeArea = false
        NSLayoutConstraint.activate([
            probe.topAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor),
            probe.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            probe.widthAnchor.constraint(equalToConstant: 0),
            probe.heightAnchor.constraint(equalToConstant: 0),
        ])
        #endif
    }
    #if os(iOS)
    /// What shows under the route where it paints nothing (LLP 1115 D2):
    /// the theme's ground; else, on a sheet resting below its large detent
    /// in a compact width, nothing, so UIKit draws its glass as it does for
    /// a hand-built sheet whose view is left clear (LLP 1141.001 D3); else
    /// the system background, which UIKit elevates in dark mode. An authored
    /// `transparent` is clear at any detent, and so is a full-screen route
    /// that paints its own background (`NodeView.paintsOverPage`), which
    /// shows the page under it where it is translucent. The route's own paint is its
    /// controller's (`RouteController.paintBackdrop`). While the keyboard
    /// ends the content above the screen's bottom, the colour the content
    /// goes on with shows behind the keys' bar (`ExactView.paintUnderKeys`).
    func paintSurface() {
        guard isViewLoaded else { return }
        let presenter = host?.presenter
        let keys = (presenter?.keyboardInset ?? 0) > 0 ? presenter?.bottomEdgeColor() : nil
        let surface = keys ?? (transparent || route?.paintsOverPage == true ? .clear
            : ThemeRoles.color("background", in: view) ?? ThemeRoles.color("Canvas", in: view) ?? (glass ? .clear : .systemBackground))
        if view.backgroundColor != surface { view.backgroundColor = surface }
    }
    private var glass: Bool {
        // A popover shows UIKit's material, and adapted to a sheet, UIKit gives its arrow no direction and the
        // sheet's own rule holds
        if let popover = popoverPresentationController, modalPresentationStyle == .popover {
            guard popover.arrowDirection == .unknown else { return true }
            let sheet = popover.adaptiveSheetPresentationController
            return (sheet.selectedDetentIdentifier ?? sheet.detents.first?.identifier) != .large
        }
        // Unspecified until it is presented: an iPhone's, compact.
        guard isSheet, traitCollection.horizontalSizeClass != .regular,
              let sheet = sheetPresentationController else { return false }
        return (sheet.selectedDetentIdentifier ?? sheet.detents.first?.identifier) != .large
    }
    /// The route's author wrote a background with no alpha.
    private var transparent: Bool {
        guard let route, route.wrote("background_color") else { return false }
        return [false, true].allSatisfy { route.channels("background_color", dark: $0).map { $0.count == 4 && $0[3] == 0 } ?? false }
    }
    #endif
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        // The keyboard probe lays the sheet out as its keyboard layout guide
        // moves, where its routes end (`OwnerBlocks.block`)
        host?.presenter.owners.publish()
    }
    func freeze() {
        guard isViewLoaded, let pixels = view.snapshotView(afterScreenUpdates: false) else { return }
        pixels.frame = view.bounds
        pixels.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(pixels)
        self.pixels = pixels
    }
}

// Each boundary owns its native controller and the navigation it mounts;
// covered owners remain mounted, inert.
private final class Presentation {
    let controller: ModalController
    let route: NodeView
    let kind: String
    let navigation: UIViewController
    let background: UIViewController
    let backgroundInteraction: Bool
    let backgroundAccessibility: Bool
    var presenting = true
    var animated = false
    /// Samples where a zoom lands while the route's nodes are still mapped:
    /// run as a Close destroys the route (`prepare`), so the zoom back starts
    /// from the photo as it is then, zoomed or panned.
    var sampleLanding: (() -> Void)?
    var alreadyDismissed = false
    var presentationsDidDismiss: (() -> Void)?
    #if os(iOS)
    /// A popover route's anchor and choices, and its presentation's delegate (LLP 1123 D7).
    var popover: PopoverRoute?
    #endif

    init(host: ModalHost, route: NodeView, navigation: UIViewController,
         background: UIViewController) {
        self.route = route
        kind = route.props["navigationPresentation"] ?? "modal"
        controller = ModalController(host: host, route: route, kind: kind, detent: route.props["navigationDetent"], grabber: route.props["navigationGrabber"])
        #if os(iOS)
        popover = kind == "popover" ? PopoverRoute(host: host, route: route) : nil
        #endif
        self.navigation = navigation
        self.background = background
        backgroundInteraction = background.view.isUserInteractionEnabled
        backgroundAccessibility = background.view.accessibilityElementsHidden
    }
}

/// A zoom's last alignment answer (`ModalHost.present`), and whether the
/// zoom now running is the person's interactive dismissal, which lands
/// unaligned.
final class ZoomLanding {
    var rect: CGRect?
    private(set) var interactive = false
    /// `interactiveDismissShouldBegin`'s answer, passed through: an accepted
    /// dismissal lands unaligned until the route appears again.
    func dismissal(begins: Bool) -> Bool {
        if begins { interactive = true }
        return begins
    }
    /// The route appeared: first, or again after the finger cancelled.
    func appeared() { interactive = false }
    /// What `alignmentRectProvider` answers for a resolved rect: none while an
    /// interactive dismissal runs. UIKit follows the finger with the whole
    /// route and, given an alignment rect, re-bases the content onto it at
    /// lift-off: a visible jump and a held frame (a plain UIKit fixture does
    /// the same). Unaligned, the release carries the drag on in one motion,
    /// the route cross-fading into its source.
    func answer(_ resolved: CGRect?) -> CGRect? {
        if let resolved { rect = resolved }
        return interactive ? nil : rect
    }
}

final class ModalHost: NSObject, UIAdaptivePresentationControllerDelegate {
    unowned let presenter: Presenter
    private var layers: [Presentation] = []
    /// Each presentation's controller, bottom first.
    var controllers: [UIViewController] { layers.map(\.controller) }
    // Native dismissal outlives logical removal and reload. Retire from the
    // top down, without letting an old completion touch a new owner.
    private var retiring: [Presentation] = []
    /// Closed and yet to be dismissed: retiring, or still a layer whose route its batch destroyed (`prepare`),
    /// which a held sync or a sheet above it keeps for a later pass.
    func retires(_ controller: UIViewController) -> Bool {
        retiring.contains { $0.controller === controller } || layers.contains { $0.controller === controller && ($0.controller.retiringRoot != nil || presenter.views[$0.route.id] !== $0.route) }
    }
    private var dismissing: Presentation?
    private var closing = false
    /// The fullscreen controller hidden until its zoom starts (`present`).
    private weak var zoomGuard: UIViewController?
    private var refusedRoute: UInt32?
    var active: Bool { !layers.isEmpty || !retiring.isEmpty || closing }
    var inTransition: Bool {
        defersFocus || isDismissing || layers.contains { $0.controller.isBeingPresented || $0.controller.isBeingDismissed }
    }
    var isDismissing: Bool { !retiring.isEmpty }
    /// LLP 1080.001 D3: every presentation's controller view, live and
    /// retiring, with what the presenter put in it: its probes and the mount
    /// its navigation is in.
    func surfaces(includingRetiring: Bool) -> [SessionSurface] {
        var seen = Set<ObjectIdentifier>()
        return (layers.map { ($0, false) } + (includingRetiring ? retiring.map { ($0, true) } : [])).compactMap { layer, retiring in
            guard let view = layer.controller.viewIfLoaded, seen.insert(ObjectIdentifier(view)).inserted else { return nil }
            let owned = [layer.controller.probe, layer.controller.pixels, layer.controller.mount].compactMap { $0 }
            return SessionSurface(view: view, label: "modal #\(layer.route.id)\(retiring ? " (retiring)" : "")", kind: .route, owned: owned)
        }
    }
    /// The removed routes and navigations a dismissal keeps on screen until
    /// UIKit ends it: unmapped node views on purpose.
    var inspectionRetiring: [UIView] {
        (layers + retiring).flatMap { [$0.controller.retiringRoot, $0.controller.retiringNavigation?.viewIfLoaded].compactMap { $0 } }
    }
    var defersFocus: Bool { closing || layers.contains { $0.presenting } }
    var closedby: String? { layers.last?.route.props["closedby"] }
    var presentation: String? { layers.last?.kind }
    var coordinateView: UIView? { layers.last?.controller.viewIfLoaded }
    /// The top modal presentation: every presentation is modal (LLP 1123 D2) but a sheet resting at or below its
    /// undimmed detent (D7), which leaves what is under it live. It and what is above it are live, the rest inert.
    private var boundary: Int? { layers.lastIndex { !$0.controller.restsUndimmed } }
    /// Whether the top modal presentation makes `view` inert: it is in neither that presentation nor one above it,
    /// nor in a `dialog`'s sheet or confirmation, which UIKit presents over it (HTML modal dialogs).
    func inertBehind(_ view: UIView) -> Bool {
        guard let boundary else { return false }
        let live = layers[boundary...].contains { $0.controller.viewIfLoaded.map(view.isDescendant(of:)) ?? false }
        return !live && !presenter.dialogs.shows(view) && !presenter.menus.confirmationShows(view)
    }
    var owner: UIViewController? { layers.last?.controller }
    /// The presentations' navigations, live and retiring, bottom to top.
    var navigations: [UINavigationController] { (layers + retiring).compactMap { $0.navigation as? UINavigationController } }
    /// The top modal presentation's navigation, of those nothing is dismissing: UIKit's presentation makes what is
    /// under it inert (`boundary`).
    var topNavigation: UINavigationController? { boundary.flatMap { layers[$0].navigation as? UINavigationController } }
    func mounts(_ view: UIView) -> Bool { surfaces(includingRetiring: true).contains { view.isDescendant(of: $0.view) } }
    var routes: [(node: NodeView, kind: String)] { layers.map { ($0.route, $0.kind) } }
    /// Each live presentation UIKit draws at another size than it lays it
    /// out, and the factor. A page sheet resting below `large` on an iOS 26
    /// iPhone floats, its whole content scaled (0.96 on a 402-pt screen), so
    /// its nodes' boxes are their authored frames times this.
    var drawnScales: [(route: NodeView, kind: String, scale: CGFloat)] {
        layers.compactMap { layer in
            guard let view = layer.controller.viewIfLoaded, let window = view.window, view.bounds.width > 0 else { return nil }
            let scale = view.convert(view.bounds, to: window).width / view.bounds.width
            #if os(iOS)
            let kind = layer.controller.modalPresentationStyle != .overFullScreen ? "sheet" : layer.kind
            #else
            let kind = layer.kind
            #endif
            return abs(scale - 1) >= 0.005 ? (layer.route, kind, scale) : nil
        }
    }
    /// Each presentation's mount and the route it holds.
    var mounted: [(mount: PresentationMount, route: NodeView)] { layers.map { ($0.controller.mount, $0.route) } }
    #if os(iOS)
    /// The topmost presented route the status bar sits over (LLP 1105 D2): a
    /// full-screen one always; a sheet only at its large detent on a
    /// compact-width screen. Nil when the bar is over the screen behind.
    /// The topmost covering presentation's navigation view (its routes and
    /// their pushes, not the app's root nodes that ride along with the
    /// viewport); nil when the bar is over the primary screen. Every
    /// presentation's navigation view, which the primary screen's scope
    /// leaves out.
    var statusBarScope: (covering: UIView?, presented: [UIView]) {
        let covering = layers.last { layer in
            let c = layer.controller
            if c.modalPresentationStyle == .overFullScreen { return true }
            guard let sheet = c.sheetPresentationController, c.traitCollection.horizontalSizeClass == .compact else { return false }
            return (sheet.selectedDetentIdentifier ?? sheet.detents.first?.identifier) == .large
        }
        return (covering?.navigation.viewIfLoaded, layers.compactMap { $0.navigation.viewIfLoaded })
    }
    /// Every presented controller re-reads the style with the root (D6):
    /// UIKit asks the topmost one that covers, which a sheet over it may hide.
    func statusBarChanged() {
        for layer in layers {
            layer.controller.setNeedsStatusBarAppearanceUpdate()
            layer.controller.setNeedsUpdateOfHomeIndicatorAutoHidden()
        }
    }
    #endif

    init(presenter: Presenter) { self.presenter = presenter }


    func canDismissByBackdrop(_ id: UInt32) -> Bool {
        guard let layer = layers.last, layer.route.id == id, layer.route.props["closedby"] == "any",
              !inTransition else { return false }
        return !refusesDismissal(of: layer.route)
    }
    func dismissByBackdrop(_ id: UInt32) {
        guard canDismissByBackdrop(id), let route = layers.last?.route else { return }
        presenter.navigationHost.sheetBack(from: route)?()
    }
    /// The agent's `back` on the top sheet (LLP 1116.006 D4): its swipe
    /// down, refused as UIKit's is (`presentationControllerShouldDismiss`)
    /// and ending as a completed one does (`sheetBack`). nil with no sheet
    /// up; "" when it went, else why not.
    func agentDismiss() -> String? {
        guard let route = layers.last?.route else { return nil }
        if inTransition { return "the sheet is still moving: `clock settle` first" }
        if NavigationRules.modalRefusesDismissal(closedby: route.props["closedby"]) { return "its closedby is \"none\"" }
        if refusesDismissal(of: route) { return presenter.navigationHost.backRefusal }
        presenter.navigationHost.sheetBack(from: route)?()
        return ""
    }

    func prepare(_ batch: Batch) {
        for layer in layers where batch.ops.contains(where: {
            $0.op == .destroy && $0.id == layer.route.id
        }) {
            let controller = layer.controller
            layer.sampleLanding?()
            controller.freeze()
            controller.retiringRoot = layer.route
            controller.retiringNavigation = presenter.navigationHost.preserveModalContent(in: controller)
        }
    }

    func retainsRemovedView(_ node: NodeView) -> Bool {
        (layers + retiring).contains { layer in
            guard let root = layer.controller.retiringRoot else { return false }
            return node === root || node.isDescendant(of: root)
        }
    }

    /// A presentation's background is inert while the presentation is modal and as it was before it while it rests
    /// undimmed (LLP 1123 D7), as UIKit passes touches through an undimmed sheet's dimming.
    private func syncBackground(_ layer: Presentation) {
        let modal = !layer.controller.restsUndimmed, view = layer.background.view!
        let interaction = !modal && layer.backgroundInteraction, hidden = modal || layer.backgroundAccessibility
        if view.isUserInteractionEnabled != interaction { view.isUserInteractionEnabled = interaction }
        if view.accessibilityElementsHidden != hidden { view.accessibilityElementsHidden = hidden }
    }

    private func releaseBackground(_ layer: Presentation) {
        let background = layer.background
        background.view.isUserInteractionEnabled = layer.backgroundInteraction
        background.view.accessibilityElementsHidden = layer.backgroundAccessibility
    }

    func updatePermissions() {
        for layer in layers {
            layer.controller.isModalInPresentation = refusesDismissal(of: layer.route)
            layer.controller.updateSheet(detent: layer.route.props["navigationDetent"], cornerRadius: ModalController.cornerRadius(of: layer.route), grabber: layer.route.props["navigationGrabber"])
            #if os(iOS)
            if let popover = layer.popover, let source = layer.controller.popoverPresentationController {
                popover.configure(source)
                // Its anchor gone, the popover closes once, as the person's tap outside it closes it
                if popover.anchorLost(in: presenter) { presenter.navigationHost.sheetBack(from: layer.route)?() }
            }
            #endif
            syncBackground(layer)
            #if os(iOS)
            layer.controller.paintSurface()
            #endif
        }
    }

    /// What each presentation's route inherits from its logical ancestors,
    /// which UIKit's hierarchy no longer carries to it, mounted in its sheet
    /// (LLP 1123 D2): an authored `aria-hidden` and `inert` above it (WAI-ARIA
    /// tree exclusion, HTML inert subtrees), set on the view the navigation
    /// is mounted in, apart from the hiding a presentation over it gives its
    /// navigation; and a `color-scheme`, the route's own or else the nearest
    /// above it: the appearance of the presented controller, which UIKit
    /// carries to everything in it, and of its presentation controller,
    /// whose chrome (a partial detent's glass, the grabber) UIKit draws
    /// outside the controller's view, from the window's appearance unless
    /// told. After every batch, a transition running or
    /// not, and as a presentation is made.
    func inheritFromAncestors() {
        for layer in layers where layer.controller.isViewLoaded { inherit(layer) }
    }

    private func inherit(_ layer: Presentation) {
        var hidden = false, inert = false, scheme = layer.route.colorScheme
        var at = presenter.eventParent(of: layer.route)
        while let view = at {
            if let node = view as? NodeView {
                hidden = hidden || node.props["accessibilityElementsHidden"] == "true"
                inert = inert || node.props["inert"] == "true"
                if scheme == .unspecified { scheme = node.overrideUserInterfaceStyle }
            }
            at = presenter.eventParent(of: view)
        }
        let mount = layer.controller.mount
        if mount.accessibilityElementsHidden != (hidden || inert) { mount.accessibilityElementsHidden = hidden || inert }
        if mount.isUserInteractionEnabled == inert {
            mount.isUserInteractionEnabled = !inert
            // An inert subtree holds no focus (HTML's inert), as `endEditing`
            // on an authored inert node ends what is under it
            if inert { mount.endEditing(true) }
        }
        // The trait update is taken now, as `applyColorScheme` takes it
        if layer.controller.overrideUserInterfaceStyle != scheme {
            layer.controller.overrideUserInterfaceStyle = scheme
            layer.controller.updateTraitsIfNeeded()
        }
        if let sheet = layer.controller.presentationController {
            let said = sheet.traitOverrides.contains(UITraitUserInterfaceStyle.self)
            if scheme == .unspecified { if said { sheet.traitOverrides.remove(UITraitUserInterfaceStyle.self) } }
            else if !said || sheet.traitOverrides.userInterfaceStyle != scheme { sheet.traitOverrides.userInterfaceStyle = scheme }
        }
    }

    /// A route's content extent changed (a `content` op): its sheet's
    /// `fit-content` detent follows.
    func contentChanged(_ node: NodeView) {
        for layer in layers where layer.route === node { layer.controller.contentChanged() }
    }

    /// The image a zoom's end shows: the node itself, or the first image
    /// among its authored descendants (each node's children are in its
    /// `container`), depth first.
    static func zoomImage(_ node: NodeView) -> NodeView? {
        if node.kind == "image" { return node }
        for child in node.container.subviews.compactMap({ $0 as? NodeView }) {
            if let image = zoomImage(child) { return image }
        }
        return nil
    }

    /// Where a zoom lands, in the zoomed controller's view: the target's box,
    /// or for an image the image as drawn (its `object-fit` in its content
    /// box, clipped there), sized from its own pixels or, before they land,
    /// `fallbackNatural`.
    static func zoomAlignment(target: NodeView, fallbackNatural: CGSize?, in zoomed: UIView) -> CGRect {
        let box = target.convert(target.bounds, to: zoomed)
        guard let image = zoomImage(target),
              let natural = image.raster?.image.naturalSize ?? fallbackNatural, natural.width > 0, natural.height > 0 else { return box }
        let content = image.contentBox()
        let drawn = RasterGeometry.rect(natural: natural, content: content, fit: image.style["object_fit"]?.string ?? "fill", position: image.style["object_position"]?.array)
        return image.convert(drawn.intersection(content), to: zoomed)
    }

    func canPresent(from parent: UIViewController, route: NodeView) -> Bool {
        #if os(iOS)
        // A popover opens once its anchor is in a window: the batch that brings it syncs again (LLP 1123 D7)
        if route.props["navigationPresentation"] == "popover", PopoverRoute.anchor(of: route, in: presenter) == nil {
            if refusedRoute != route.id {
                refusedRoute = route.id
                presenter.session?.log("popover route #\(route.id) waits: no element in a window has the id its navigationPopoverAnchor names, \"\(route.props["navigationPopoverAnchor"] ?? "")\"")
            }
            return false
        }
        #endif
        guard parent.presentedViewController == nil, !parent.isBeingPresented, !parent.isBeingDismissed,
              parent.viewIfLoaded?.window != nil else {
            if refusedRoute != route.id {
                refusedRoute = route.id
                let reason = parent.presentedViewController != nil ? "the owning controller already presents"
                    : parent.viewIfLoaded?.window == nil ? "the owning controller has no window"
                    : "the owning controller is in a transition"
                presenter.session?.log("modal route #\(route.id) refused: \(reason)")
            }
            return false
        }
        refusedRoute = nil
        return true
    }

    @discardableResult
    func present(_ route: NodeView, navigation: UIViewController, from background: UIViewController,
                 preceding: NodeView?, owner: UIViewController) -> Bool {
        let layer = Presentation(host: self, route: route, navigation: navigation,
                                 background: background)
        // The press that opened the sheet may hand its control's focus to the
        // route's first autofocus until the route goes (PressHandoffIOS).
        presenter.pressShows(route)
        // The presenting screen stays where it is, inert while the sheet is modal (HTML modal dialogs)
        syncBackground(layer)
        let controller = layer.controller
        if #available(iOS 18.0, tvOS 18.0, *), layer.kind == "fullscreen", route.props["navigationSource"] != nil {
            let options = UIViewController.Transition.ZoomOptions()
            let landing = ZoomLanding()
            options.interactiveDismissShouldBegin = { [weak self, weak route] context in
                guard let self, let route, context.willBegin else { return false }
                return landing.dismissal(begins: !self.refusesDismissal(of: route))
            }
            controller.appeared = landing.appeared
            // Where the source lands: the presented route's own element with
            // the source's id, when it has one; for an image, the image as it
            // is drawn (`object-fit`), so the zoom morphs photo into photo
            // rather than the whole route into the thumbnail with two photos
            // cross-fading (LLP 1035.001, the zoom's alignment).
            // The last answer, for the zoom back after a Close: by then the
            // route's nodes have left the presenter's map (`release`) though
            // UIKit still shows them. `prepare` samples it just before.
            let resolve: (UIView, NodeView?) -> CGRect? = { [weak self, weak route] zoomed, sourceView in
                guard let self, let route, let name = route.props["navigationSource"],
                      let target = presenter.views.values.filter({
                          $0.props["id"] == name && $0.window != nil && $0.isDescendant(of: route)
                      }).min(by: { $0.id < $1.id }) else { return nil }
                // Before the viewer's own pixels land, the source's image (the
                // view UIKit zooms from) is the same photo.
                let source = sourceView.flatMap(Self.zoomImage)
                return Self.zoomAlignment(target: target, fallbackNatural: source?.raster?.image.naturalSize, in: zoomed)
            }
            options.alignmentRectProvider = { context in
                landing.answer(context.zoomedViewController.viewIfLoaded.flatMap { resolve($0, context.sourceView as? NodeView) })
            }
            layer.sampleLanding = { [weak controller] in
                if let zoomed = controller?.viewIfLoaded, let rect = resolve(zoomed, nil) { landing.rect = rect }
            }
            controller.preferredTransition = .zoom(options: options, sourceViewProvider: { [weak self, weak route, weak preceding] _ in
                guard let self, let route, let preceding,
                      presenter.views[preceding.id] === preceding,
                      let name = route.props["navigationSource"] else { return nil }
                return presenter.views.values.filter {
                    $0.props["id"] == name && $0.window != nil &&
                        ($0 === preceding || $0.isDescendant(of: preceding))
                }.min(by: { $0.id < $1.id })
            })
            // Hidden until UIKit's zoom has it: on a device a frame could be
            // drawn with the presented view in place at full size before the
            // zoom took it over (one frame of the whole destination, then the
            // presenting screen again, then the zoom).
            controller.view.alpha = 0
            zoomGuard = controller
        }
        // A full-screen route that declares its own entrance or exit plays it (LLP 1013.001 D2).
        if layer.kind == "fullscreen", let session = presenter.session {
            let size = owner.view.bounds.size
            controller.routeTransitions = RoutePresenting(entrance: session.routePart(route, exit: false, size: size),
                                                          exit: session.routePart(route, exit: true, size: size))
        }
        layers.append(layer)
        controller.isModalInPresentation = refusesDismissal(of: route)
        // Presented from the embedder's controller, it inherits none of the navigation owner's direction.
        presenter.turnChrome(controller)
        controller.loadViewIfNeeded()
        // What it inherits, from its first frame: a sheet a deferred sync
        // presents (the second of two, after the first is up) waits for the next batch otherwise.
        inherit(layer)
        controller.addChild(navigation)
        presenter.collections.watchContacts(in: controller.mount)
        controller.mount.addSubview(navigation.view)
        #if os(iOS)
        if controller.mount.relay == nil { controller.mount.relay = MountRelay(presenter, mount: controller.mount) }
        #endif
        navigation.view.setPaintForeground(aboveAuthored: false)
        navigation.view.frame = controller.mount.bounds
        navigation.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        navigation.didMove(toParent: controller)
        controller.presentationController?.delegate = self
        #if os(iOS)
        // A popover's delegate adapts it, and passes its dismissal here
        if let popover = layer.popover, let source = controller.popoverPresentationController {
            source.delegate = popover
            popover.configure(source)
        }
        #endif
        let guarded = zoomGuard === controller
        zoomGuard = nil
        HostLog.note(presenter.session, "presentation: presenting the \(layer.kind) \(layer.route.props["navigationKey"] ?? "?")")
        owner.present(controller, animated: !ExactEnv.agentFreezes && !presenter.navigation.crossFading) { [weak self, weak layer, weak controller] in
            if guarded { UIView.performWithoutAnimation { controller?.view.alpha = 1 } }
            guard let self, let layer,
                  layers.contains(where: { $0 === layer }) || retiring.contains(where: { $0 === layer }) else { return }
            HostLog.note(presenter.session, "presentation: presented the \(layer.kind) \(layer.route.props["navigationKey"] ?? "?")")
            layer.presenting = false
            #if os(iOS)
            // Whether it is a popover or a sheet is known now (`glass`)
            if layer.popover != nil { controller?.paintSurface() }
            // Up: what the bar sits over now (LLP 1105 D5).
            presenter.resolveStatusBar()
            #endif
            DispatchQueue.main.async { [weak self, weak layer] in
                guard let self, let layer else { return }
                if layers.contains(where: { $0 === layer }) {
                    fit()
                    presenter.navigationHost.modalDidDismiss()
                }
                // The transition's transforms are gone: a transform drag in
                // the presented route measures again, or it stays refused
                // until something else lays out (LLP 1057.001 §4).
                presenter.transformGeometry.changed()
                presenter.syncModal()
                drainRetired()
                presenter.menus.resumeAfterRouteDismissal()
            }
        }
        guard owner.presentedViewController === controller else {
            // UIKit refused the presentation and will not invoke its completion block.
            layer.presenting = false
            layers.removeAll { $0 === layer }
            endPress(layer)
            releaseBackground(layer)
            controller.presentationController?.delegate = nil
            #if os(iOS)
            controller.popoverPresentationController?.delegate = nil
            #endif
            navigation.willMove(toParent: nil)
            navigation.view.removeFromSuperview()
            navigation.removeFromParent()
            if guarded { UIView.performWithoutAnimation { controller.view.alpha = 1 } }
            presenter.session?.log("modal route #\(route.id) refused: UIKit declined presentation")
            return false
        }
        if guarded {
            // Shown as the transition starts, in the block UIKit runs with
            // its first frame; at once if there is no coordinator or it
            // refuses the block; and in any case when the presentation ends.
            let show = { UIView.performWithoutAnimation { controller.view.alpha = 1 } }
            let taken = controller.transitionCoordinator?.animate(alongsideTransition: { _ in show() }, completion: { _ in show() }) ?? false
            if !taken { show() }
        }
        fit()
        return true
    }

    func fit() {
        guard !closing else { return }
        presenter.session?.view?.fit()
        presenter.flushPendingFocus()
        presenter.elementScrolls.flush()
    }

    func closeTop(animated: Bool = !ExactEnv.agentFreezes, refit: Bool = true) {
        guard let layer = layers.last else { return }
        closing = true
        if !layer.alreadyDismissed { presenter.cancelPendingFocus() }
        presenter.elementScrolls.cancel(inside: layer.controller.view)
        let controller = layer.controller
        layer.presentationsDidDismiss = presenter.menus.retire(over: controller, removing: layer.route)
        presenter.navigationHost.retireNavigation(layer.navigation, preserving: controller.retiringNavigation != nil)
        layers.removeLast()
        endPress(layer)
        releaseBackground(layer)
        layer.animated = animated
        retiring.append(layer)
        if refit { presenter.session?.view?.fit() }
        closing = false
        // Post-batch focus transfers before dismissal releases the old editor.
        DispatchQueue.main.async { [weak self] in self?.drainRetired() }
    }

    private func drainRetired() {
        guard dismissing == nil, let layer = retiring.first, !layer.presenting else { return }
        let above = sequence(first: layer.controller.presentedViewController, next: { $0?.presentedViewController })
        if let moving = above.compactMap({ $0 }).first(where: { $0.isBeingPresented || $0.isBeingDismissed }),
           let coordinator = moving.transitionCoordinator,
           coordinator.animate(alongsideTransition: nil, completion: { [weak self] _ in
               DispatchQueue.main.async { self?.drainRetired() }
           }) { return }
        dismissing = layer
        let completion = { [weak self, layer] in
            guard let self, dismissing === layer else { return }
            dismissing = nil
            retiring.removeAll { $0 === layer }
            layer.presentationsDidDismiss?()
            layer.presentationsDidDismiss = nil
            // Its routes, if they live on, give their blocks back (LLP 1123 D2)
            presenter.owners.publish()
            #if os(iOS)
            // Gone: what the bar sits over now (LLP 1105 D5).
            presenter.resolveStatusBar()
            #endif
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                drainRetired()
                defer { presenter.menus.resumeAfterRouteDismissal() }
                guard let session = presenter.session, session.state != .destroyed else { return }
                presenter.navigationHost.modalDidDismiss()
                fit()
                // As after a presentation: what the dismissal's transition
                // covered measures again (a drag in the route left on top).
                presenter.transformGeometry.changed()
                presenter.syncModal()
            }
        }
        if !layer.alreadyDismissed, let owner = layer.controller.presentingViewController {
            HostLog.note(presenter.session, "presentation: dismissing the \(layer.kind) \(layer.route.props["navigationKey"] ?? "?")")
            // Dismissing the route itself would dismiss only what it presents.
            owner.dismiss(animated: layer.animated, completion: completion)
        } else { completion() }
    }

    private func refusesDismissal(of route: NodeView) -> Bool {
        presenter.views[route.id] !== route || NavigationRules.modalRefusesDismissal(closedby: route.props["closedby"]) ||
            presenter.navigationHost.sheetBack(from: route) == nil
    }

    func presentationControllerShouldDismiss(_ presentationController: UIPresentationController) -> Bool {
        guard let layer = layers.last, layer.controller === presentationController.presentedViewController else { return false }
        return !refusesDismissal(of: layer.route)
    }

    func presentationControllerDidAttemptToDismiss(_ presentationController: UIPresentationController) {
        guard layers.last?.controller === presentationController.presentedViewController,
              !presenter.navigationHost.canInvokeBack else { return }
        presenter.session?.log("modal dismissal refused: \(presenter.navigationHost.backRefusal)")
    }

    /// A sheet the person swipes down: what is beneath shows from the
    /// dismissal's first frame, not when the app hears of it after (Observe's
    /// render mark); a swipe let go cancels it.
    func presentationControllerWillDismiss(_ presentationController: UIPresentationController) {
        guard let layer = layers.last, layer.controller === presentationController.presentedViewController else { return }
        NavigationMarks.shared.platformBeganShowing()
        presentationController.presentedViewController.transitionCoordinator?.animate(alongsideTransition: nil) { context in
            if context.isCancelled { NavigationMarks.shared.platformCancelled() }
        }
    }

    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard let layer = layers.last, layer.controller === presentationController.presentedViewController else { return }
        HostLog.note(presenter.session, "presentation: the person dismissed the \(layer.kind) \(layer.route.props["navigationKey"] ?? "?")")
        closing = true
        presenter.cancelPendingFocus()
        layer.presenting = false
        layer.alreadyDismissed = true
        endPress(layer)
        // UIKit still uses this hierarchy on its callback stack. Remove its
        // owner before dispatch, so an action may safely keep the same route.
        DispatchQueue.main.async { [weak self, layer] in
            guard let self, layers.last === layer else { return }
            let back = presenter.navigationHost.sheetBack(from: layer.route)
            closeTop(animated: false, refit: false)
            back?()
            presenter.navigationHost.modalDidDismiss()
            fit()
        }
    }

    func unmounted() { reset() }

    /// The sheet went, or never came: its press hands nothing on through it.
    private func endPress(_ layer: Presentation) { presenter.pressHandoff?.drop(layer.route) }

    func reset() {
        // Dismiss native owners from the top down, each keeping what it shows
        // for its dismissal. New projection waits until all retired owners finish.
        for layer in layers.reversed() {
            if layer.controller.presentingViewController == nil { layer.presenting = false }
            layer.controller.freeze()
            layer.controller.retiringRoot = layer.route
            layer.controller.retiringNavigation = presenter.navigationHost.preserveModalContent(in: layer.controller)
            closeTop(animated: false, refit: false)
        }
    }
}
#endif

#if os(iOS)
// A sheet dragged between detents moves what the status bar sits over
// with no batch (LLP 1105 D5).
extension ModalHost: UISheetPresentationControllerDelegate {
    func sheetPresentationControllerDidChangeSelectedDetentIdentifier(_ sheetPresentationController: UISheetPresentationController) {
        presenter.resolveStatusBar()
        guard let layer = layers.first(where: { $0.controller.sheetPresentationController === sheetPresentationController }) else { return }
        layer.controller.paintSurface()
        // Dragged past its undimmed detent, the sheet begins or ends its modality
        syncBackground(layer)
    }
}
#endif
