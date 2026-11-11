// @ref LLP 1008 §9 — Contract routes projected into UIKit's navigation
// controller. UIKit owns recognition, arbitration, progress and cancellation; only a completed pop invokes the Contract back control.
// The bar a stack shows, what a route projects into it and when the app module's hatches run: LLP 1075.003 (NavigationBarIOS.swift, NativeHatches.swift).
#if os(iOS) || os(tvOS)
import UIKit

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
    /// A list beside its detail: the primary stack as UIKit's split (SplitIOS.swift).
    var split: SplitIOS?
    private(set) var presentedNavigations: [UINavigationController] = []
    /// The theme's title ink each bar was last given (`titleInk`), held no
    /// longer than its bar.
    let titleInks = NSMapTable<UINavigationBar, UIColor>.weakToStrongObjects()
    var modalNavigation: UINavigationController? { presentedNavigations.last }
    var navigation: UINavigationController? { modalNavigation ?? split?.front ?? primaryNavigation }
    /// Whether a stack's pushes and pops are the router's: the one in front,
    /// and with a split, either column's.
    private func follows(_ nav: UINavigationController) -> Bool { nav === navigation || (modalNavigation == nil && split?.holds(nav) == true) }
    /// The route on top of the stack in front: the one the person sees.
    var activeRoute: RouteController? { (navigation?.topViewController ?? primaryHeld?.route) as? RouteController }
    private(set) var syncing = false
    private var mounting = false
    private(set) weak var container: NodeView?
    private var routeIDs: [UInt32] = []
    private(set) var controllers: [UInt32: RouteController] = [:]
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
    var tabNavigations: [UInt32: UINavigationController] = [:]
    /// The tabs whose stacks have no navigation controller yet (HeldStackIOS).
    var heldTabs: [UInt32: HeldStack] = [:]
    /// A sync is armed for when the transition a held tab waits on ends.
    var syncAfterTransitionArmed = false
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
    /// The tabs and stacks the items were last made from, made again when
    /// the size class (a path glyph's size) changes.
    var tabItemsSource: (NavigationTabs, [UIViewController])?
    /// The bar's tint as last written as a value: the selected tab's own tint, light and dark; nil
    /// where the bar takes an accent as its tint.
    var tabTint: [[Double]]?
    let tabProxy = TabDelegateProxy()
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
    /// The covers the kernel was last told (`deliverCovers`).
    var delivered: [UInt32: HostCover] = [:]
    /// Each shown route's values as a content owner (LLP 1123 D9), and
    /// those the kernel was last told.
    var ownerValues: [UInt32: OwnerValues] = [:]
    var deliveredOwners: [UInt32: OwnerValues] = [:]
    /// Whether this boot's first batch looked for the tablist the boot
    /// covered (`adoptBootCover`); every boot covers its own.
    private(set) var bootCoverSeen = false
    var ownedReported: Set<String> = []
    var replayingHatches = false
    private var coversPending = false
    /// Whether the session's view last took the whole of its own for a bar.
    var tookWholeView = false
    private var pendingSync = false
    /// The press whose commit a deferred sync still owes (PressHandoffIOS).
    private weak var pendingPress: PressHandoff?
    /// A sync was deferred (a transition ran, or there was no window yet)
    /// and is still owed, or UIKit moved a stack itself (its back button, a
    /// swipe) since the last one: the next batch syncs, whatever it holds.
    var syncOwed: Bool {
        pendingSync || changing || windowless || nativeMoved || !settled
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
    private var shownKeys: [String] = [], fallbackKey: String?
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

    func prepare(_ batch: Batch) {
        checkOwned()
        presenter.elements.checkOwned()
        guard let top = navigation?.topViewController as? RouteController else { return }
        if batch.ops.contains(where: { $0.op == .destroy && $0.id == top.node.id }) {
            top.freeze()
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
        }
        for op in batch.ops where op.op == .children && logicalChildren[op.id] != nil {
            logicalChildren[op.id] = op.ids
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
        defer { primaryOwner?.view.layoutIfNeeded() } // UIKit must mount retained routes in a replacement container too.
        if let tabs = p.tabs {
            installTabs(p, tabs, first: first, in: parent)
            return
        }
        if wantsSplit(first.first) { return installSplit(p, first: first, in: parent) }
        // The stack holds its first routes before its view enters the window,
        // as the tabs' stacks do (`installTabs`): UIKit shows a root's large
        // title collapsed when the stack was laid out in the window before it
        // had that root.
        let nav = makeNavigation(first: first.first?.node)
        primaryNavigation = nav
        prepareRoutes(first, in: nav)
        nav.setViewControllers(first, animated: false)
        recordOwned(nav)
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
        let window = presenter.session?.view?.window
        windowless = presenter.session != nil && window == nil
        // With no session (a presenter on its own) nothing is projected or owed.
        if presenter.session == nil { settled = true }
        guard window != nil else { return }
        nativeMoved = false; settled = false
        syncing = true
        defer { syncing = false; presenter.flushPendingFocus(); presenter.elementScrolls.flush() }
        presenter.owners.publish() // PROTO: covers must find a block before deriving child styles.
        guard let p = projection(batch) else { settled = true; return }
        let root = p.root, wanted = p.chosen
        // A presentation whose route a logical ancestor's `display: none`
        // hides is not presented, nor is any above it: its sheet is out of
        // the document's tree, which no longer hides it (LLP 1123 D2)
        let segments = NavigationRules.segments(presentations: p.routes[...p.selected].map { $0.props["navigationPresentation"] })
        var parts = Array(segments.enumerated().prefix { $0.offset == 0 || !logicallyHidden(wanted[$0.element.lowerBound].node) }.map(\.element))
        // A sheet that is an inspector shows in an expanded split's column instead (LLP 1127).
        let inspecting = inspectorRoute(parts, wanted)
        if inspecting != nil { parts = Array(parts.prefix(1)) }
        let rebuilt = reshape(p)
        installPrimary(p, first: Array(wanted[parts[0]]))
        // A list the split held is a list again, or the other way (its sidebar look).
        if rebuilt { presenter.groupedLists?.sync(changed: nil) }
        // The other tabs' stacks, the selected tab and its items (LLP 1075.003 §3.7).
        if p.tabs != nil { syncTabs(p) }
        // Initial content draws before a presentation covers it.
        if parts.count > 1, let session = presenter.session, session.firstDrawMs == nil { return }
        // The selected tab held while its navigation controller waits for
        // a transition in the way (`syncTabs`, HeldStackIOS) shows nothing
        // new until it is built.
        let heldWaits = primaryHeld != nil && needsNavigation(Array(wanted[parts[0]]))
        guard !changing, !presenter.modals.inTransition, !heldWaits else {
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
        // The selected tab while it is held (HeldStackIOS): `syncTabs` built its
        // navigation controller if this batch pushes onto it or shows its bar.
        if let held = primaryHeld {
            show(Array(wanted[parts[0]]), in: held)
            held.view.layoutIfNeeded()
        }
        // Mount the destination under a departing presentation before starting
        // dismissal, so its editor can accept the batch's focus handoff.
        let owners: [UINavigationController?] = (primaryHeld != nil ? [nil] : [primaryNavigation].compactMap { $0 }) + presentedNavigations
        for index in 0...common where index < owners.count {
            if index == 0, let split, primaryHeld == nil {
                // An inspector's column the stack no longer shows closes first: UIKit lays the
                // detail out once, where the columns end, not beside the column and again as it goes.
                if inspecting == nil, split.inspecting != nil, mounted.count <= common { placeInspector(nil) }
                let animated = owners.count == 1 && mounted.count == boundaries.count && !ExactEnv.agentFreezes && !unanimated && split.controller.view.window != nil
                syncSplit(split, Array(wanted[parts[0]]), animated: animated)
                split.controller.view.layoutIfNeeded()
                continue
            }
            guard let nav = owners[index] else { continue }
            let stack = Array(wanted[parts[index]])
            prepareRoutes(stack, in: nav)
            let same = nav.viewControllers.count == stack.count && zip(nav.viewControllers, stack).allSatisfy { $0 === $1 }
            if !same {
                // @ref LLP 1038 D6 — a tab change swaps immediately. A replacement within one stack
                // whose new top was not on it (a finished screen giving way to its result) arrives
                // as UIKit's own push, rather than cutting.
                let pushOrPop = NavigationRules.isPushOrPop(from: nav.viewControllers.map(ObjectIdentifier.init), to: stack.map(ObjectIdentifier.init))
                let arrives = stack.count > 1 && nav.viewControllers.first === stack.first
                    && !nav.viewControllers.contains { $0 === stack.last }
                #if os(iOS)
                // A route pushed from a source zooms from it (ZoomPushIOS).
                if (pushOrPop || arrives), stack.count > 1, let top = stack.last as? RouteController, !nav.viewControllers.contains(where: { $0 === top }) {
                    prepareZoomPush(top, beneath: stack[stack.count - 2] as? RouteController)
                }
                #endif
                nav.setViewControllers(stack, animated: (pushOrPop || arrives) && index == owners.count - 1 && mounted.count == boundaries.count && !ExactEnv.agentFreezes && !unanimated && nav.view.window != nil)
                recordOwned(nav)
                // A press that pushed may hand its control's focus to the arriving route's first autofocus (PressHandoffIOS).
                if pushOrPop || arrives { presenter.pressShows((stack.last as? RouteController)?.node) }
            }
        }
        unanimated = false
        // As the split expands, the inspector's sheet gives its route to the
        // column and goes at once: UIKit's own inspector is in its column
        // from the expansion's first frame (LLP 1127 D14).
        let toColumn = inspecting != nil && mounted.last?.node === inspecting?.node
        let sheet = toColumn ? inspecting?.navigationController?.parent : nil
        // A split still expanding has no column for it yet: the sheet stays until it has.
        if toColumn, !placeInspector(inspecting, animated: false) {
            pendingSync = true
            return
        }
        // The emptied sheet shows no more: UIKit dismisses it only once the
        // expansion is over, and until then it was blank over the column.
        sheet?.presentationController?.containerView?.isHidden = true
        if mounted.count > common {
            pendingSync = true
            presenter.modals.closeTop(animated: !toColumn && !ExactEnv.agentFreezes)
            return
        }
        placeInspector(inspecting)
        if boundaries.count > mounted.count, let background = modalNavigation ?? primaryOwner, let owner {
            let part = parts[mounted.count + 1], route = wanted[part.lowerBound].node
            guard presenter.modals.canPresent(from: owner, route: route) else { return }
            // The sheet mounts its own navigation (`ModalHost.present`)
            let nav = makeNavigation(first: route)
            nav.view.frame = root.bounds
            presentedNavigations.append(nav)
            prepareRoutes(Array(wanted[part]), in: nav)
            nav.setViewControllers(Array(wanted[part]), animated: false)
            recordOwned(nav)
            presenter.pressShows(wanted[part].last?.node) // a route pushed in the sheet as it opens
            watchPops(nav)
            nav.view.layoutIfNeeded()
            pendingSync = boundaries.count > mounted.count + 1
            let preceding = part.lowerBound > 0 ? wanted[part.lowerBound - 1].node : nil
            presenter.modals.present(route, navigation: nav, from: background,
                                     preceding: preceding, owner: owner)
        }
        // A presented navigation fills its sheet; the primary owner fills the root, which no presentation resizes (LLP 1123 D1).
        if primaryOwner != nil {
            fitOwner()
            placeOwner()
        }
        for (id, c) in controllers where !routeIDs.contains(id) { end(c) }
        controllers = controllers.filter { routeIDs.contains($0.key) }
        presenter.modals.updatePermissions()
        if p.tabs != nil { replayTabs() }
        if !presenter.awaitingFrame.isEmpty {
            // Deliver the covers of routes this batch installed before UIKit first lays out their new views, not after
            // the batch as a report during it waits to: the batch laid them out without their covers, and UIKit
            // animates a native view it has laid out from that size to the covered one (a segmented control's
            // selection slides). The batch's frames are all applied here.
            reportCovers()
            deliverCovers()
            window?.layoutIfNeeded() // Lay out new views before reporting their geometry
        }
        reportCovers()
        refitForBars()
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
        c.safeArea.end()
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
        // (Held stacks too, whose routes' hatches have not run either; a tab
        // container the app took built every tab's navigation controller.)
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
        guard !syncing, !coversPending else { return }
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
    var primaryOwner: UIViewController? { tabOwner ?? split?.controller ?? primaryNavigation }

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

    func modalDidDismiss() {
        guard pendingSync else { return }
        pendingSync = false
        sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        presenter.syncModal()
    }

    /// Retire only the named owner; a late completion cannot remove its
    /// successor or the retained owner beneath it.
    func retireNavigation(_ controller: UIViewController, preserving: Bool) {
        guard let nav = controller as? UINavigationController,
              presentedNavigations.contains(where: { $0 === nav }) else { return }
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

    /// How the last transition ended — `completed` (the Back control was
    /// pressed), `cancelled` (an interactive pop returned), `idle` (a
    /// programmatic change, or nothing yet) — for `state.navigation`.
    private var lastTransition = "idle"
    /// Whether UIKit is pushing or popping the stack navigation follows: from
    /// the transition's first frame through `didShow`, until the completion
    /// `willShow` registers
    var changing: Bool { navigation.map { nativePopPending(on: $0) || pushOrPop(on: $0) != nil } ?? false }

    /// UIKit removes a controller before resigning its search field and calling willShow.
    /// A shorter prefix of our last write belongs to that unfinished native pop.
    private func nativePopPending(on nav: UINavigationController) -> Bool {
        guard let written = stacks[ObjectIdentifier(nav)]?.written else { return false }
        let current = nav.viewControllers.map(ObjectIdentifier.init)
        return current.count < written.count && current.elementsEqual(written.prefix(current.count))
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
            guard let bar = backBar, bar.nav.viewIfLoaded?.window != nil, !bar.nav.isNavigationBarHidden,
                  bar.depth > 1, canInvokeBack else { return nil }
            let owner: UIViewController? = split == nil && bar.nav === primaryNavigation ? primaryOwner : bar.nav
            return owner?.view.accessibilityIdentifier
        }()
        let routes: [UIViewController]? = modalNavigation == nil ? split?.shown : nil
        let stack = (routes ?? navigation?.viewControllers ?? primaryHeld?.stackRoutes ?? []).compactMap { ($0 as? RouteController)?.key }
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

    func invokeBack(from source: NodeView) {
        guard presenter.session?.view?.window != nil,
              presenter.views[source.id] === source, routeIDs.contains(source.id),
              source.props["navigationKey"] == selectedKey else { return }
        goBack()
    }

    /// Whether the platform's Back goes from the selected route (LLP 1115 D5).
    var canInvokeBack: Bool { back != nil }

    /// The controller whose bar holds Back for the stack in front, and how
    /// many routes that bar goes back through: in a split, the list's while
    /// collapsed, else the detail's over its own routes.
    private var backBar: (nav: UINavigationController, depth: Int)? {
        guard modalNavigation == nil, let split else { return navigation.map { ($0, $0.viewControllers.count) } }
        return split.controller.isCollapsed ? (split.list, split.shown.count) : (split.detail, split.shown.count - 1)
    }

    /// The agent's `back` from a pushed route (LLP 1116.006 D4): UIKit's
    /// Back button, ending where its pop ends (`goBack`), as tvOS's Menu
    /// does. nil when the stack in front has nothing beneath its top (a
    /// sheet's own root is the sheet's); "" when it went, else why not.
    func agentBack() -> String? {
        guard (backBar?.depth ?? 0) > 1 else { return nil }
        guard back != nil else { return backRefusal }
        if let to = NavigationRules.beneathPoppedTop(selected: selectedKey ?? "", inspecting: inspectorKey, shownKeys: shownKeys) { goBack(from: selectedKey ?? "", to: to) } else { goBack() }
        return ""
    }

    /// The platform's Back, completed: the authored control pressed, else
    /// the root's `navigate` with the location beneath (LLP 1115 D5).
    func goBack() {
        switch back {
        case .press(let id)?: presenter.press(id)
        case .navigate(let location)?:
            guard let root = container, let session = presenter.session else { return }
            session.navigate(location, at: root)
        case .pop?:
            guard let key = selectedRoute?.props["navigationKey"], let session = presenter.session else { return }
            session.hostBack(key)
        case nil: break
        }
    }

    /// A history jump uses navigation; the authored Back control handles the adjacent intent.
    private func goBack(from source: String, to destination: String) {
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
        // A split's list shows beside its detail, live (LLP 1127 D8), as does the detail beside its
        // inspector; the detail under the list that overlays it, and the columns under a sheet, show, not live.
        let split = splitShown
        if split.beside.contains(where: { $0.key == key }) { return false }
        if split.under.contains(where: { $0.key == key }) { return true }
        return key != selected
    }

    /// D1: resolved at use, by HTML id, among live enabled press controls —
    /// never captured at a gesture's start (`NavigationBack.control`).
    private var backControl: NodeView? {
        guard let route = selectedRoute else { return nil }
        let routes = routeIDs.compactMap { presenter.views[$0] }
        return NavigationBack.control(named: container?.props["navigationBack"], among: presenter.carrying("id"),
                                           id: \.id, htmlID: { $0.props["id"] }, pressable: { $0.handlers.contains("press") }, disabled: \.disabled,
                                           inActiveRoute: { $0 === route || $0.isDescendant(of: route) },
                                           inAnyRoute: { control in routes.contains { control === $0 || control.isDescendant(of: $0) } })
    }

    var selectedRoute: NodeView? {
        guard let key = selectedKey else { return nil }
        return routeIDs.compactMap({ presenter.views[$0] }).first(where: { $0.props["navigationKey"] == key })
    }

    /// How Back goes from the selected route, resolved at use as its
    /// control is (`NavigationBack.way`, LLP 1115 D5).
    var back: NavigationBack.Way? {
        guard let root = container, let route = selectedRoute else { return nil }
        return NavigationBack.way(control: backControl?.id, declared: declaresBack(route), hearsNavigate: root.handlers.contains("navigate"),
                                    beneath: { [presenter] in route.props["navigationKey"].flatMap { presenter.session?.runtime.locationBeneath($0) } })
    }

    /// Whether a route holds an element named by the root's `navigationBack`.
    private func declaresBack(_ route: NodeView) -> Bool {
        guard let name = container?.props["navigationBack"] else { return false }
        return presenter.carrying("id").contains { $0.props["id"] == name && ($0 === route || $0.isDescendant(of: route)) }
    }

    /// Why Back does not go from the selected route, for the journal.
    var backRefusal: String {
        if let route = selectedRoute, declaresBack(route) { return "its navigationBack control is disabled or has no press handler" }
        return "no visit beneath the active route"
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
        let owner: UINavigationController? = nil
        #else
        let owner = (allNavigations + [split?.detail].compactMap { $0 }).first { nav in
            if nav.interactivePopGestureRecognizer === gestureRecognizer { return true }
            if #available(iOS 26.0, *) { return nav.interactiveContentPopGestureRecognizer === gestureRecognizer }
            return false
        }
        #endif
        if let owner, !follows(owner) { return false }
        // Of a split's two stacks, one pops: the detail's while it has a route
        // beneath its top, else (collapsed) the list's, which holds the
        // detail's stack. The other, under the same finger, would take the swipe.
        if let owner, let split, split.holds(owner),
           owner !== (split.controller.isCollapsed && split.detail.viewControllers.count < 2 ? split.list : split.detail) { return false }
        let depth = backBar?.depth ?? 0
        let goes = back != nil
        guard NavigationRules.popMayBegin(depth: depth, changing: changing, modalActive: presenter.modals.inTransition,
                                          hasBackControl: goes,
                                          contextPreviewActive: !presenter.chrome.ids("contextTarget").isEmpty) else {
            if depth > 1, !changing, !presenter.modals.inTransition, !goes {
                presenter.session?.log("back gesture refused: \(backRefusal)")
            }
            return false
        }
        guard let start, let view else { return true }
        let hit = view.hitTest(start, with: nil)
        if CanvasInputs.owns(hit) { return false }
        return NavigationRules.panMayBegin(startX: start.x, overSwipeRight: Self.overRightSwipe(hit, in: view), velocity: velocity)
    }

    /// Whether the content pop stays off a touch on `hit`: anywhere on a
    /// `swiperight` row (LLP 1057.001 §7), on a `swipeleft` row while its
    /// swipe is caught left of rest, on its way back, or on a node with a
    /// one-finger `swipe` to the right with no `press` node between (§8.4,
    /// rule 3).
    static func overRightSwipe(_ hit: UIView?, in view: UIView) -> Bool {
        var current = hit, pressBetween = false
        while let v = current {
            if let node = v as? NodeView {
                if node.handlers.contains("swiperight") || (node.handlers.contains("swipeleft") && node.translate.x < -0.5)
                    || (!pressBetween && node.takesRightSwipe) { return true }
                if node.handlers.contains("press") { pressBetween = true }
            }
            if v === view { return false }
            current = v.superview
        }
        return false
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
        guard follows(navigationController) else { return }
        HostLog.note(presenter.session, {
            let by = navigationController.transitionCoordinator?.isInteractive == true ? "the edge swipe" : nativePopPending(on: navigationController) ? "UIKit's Back" : "the app"
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
        // the finger is down. Beneath a split's detail is the list's route.
        let depth = navigationController.viewControllers.count + (split?.detail === navigationController ? 2 : 1)
        let source = transition.viewController(forKey: .from) as? RouteController
        let swipe: Swipe? = transition.initiallyInteractive ? source.map { ($0.node, $0.key, depth) } : nil
        if let split, split.detail === navigationController, let key = inspectorKey, source?.key == NavigationRules.poppedTop(selected: key, inspecting: key, shownKeys: shownKeys) { split.closeInspector(along: transition) }
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
        guard follows(navigationController), navigationController.topViewController === viewController else {
            recordPop(navigationController)
            return
        }
        HostLog.note(presenter.session, "navigation: didShow \((viewController as? RouteController)?.key ?? "?")")
        // An animated push or pop ends in its coordinator's completion
        guard pushOrPop(on: navigationController) == nil else { return }
        transitionEnded(navigationController, swipe: nil, cancelled: false)
    }

    /// An interactive pop's source and the depth of the app's stack before it began.
    typealias Swipe = (node: NodeView, key: String, depth: Int)

    /// A push or pop of the stack navigation follows is over: the deferred
    /// work runs and a Back UIKit performed reaches the router. One on a
    /// stack navigation stopped following (another tab was selected, or
    /// the stacks were reset) is not navigation's.
    private func transitionEnded(_ navigationController: UINavigationController,
                                 swipe: Swipe?, cancelled: Bool) {
        // Finish the departed search field while its authored handlers still exist
        if !cancelled, follows(navigationController),
           let written = stacks[ObjectIdentifier(navigationController)]?.written {
            let departed = Set(written).subtracting(navigationController.viewControllers.map(ObjectIdentifier.init))
            for route in controllers.values where departed.contains(ObjectIdentifier(route)) {
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
        // Collapsed, UIKit's Back took the detail's controller off the list's, with the detail's routes in it;
        // an expansion takes it off too, for its column, and is no Back.
        let detail = split.flatMap { split in
            split.list === navigationController && split.controller.isCollapsed && !split.expanding && !split.nested ? split.detail : nil
        }
        // Identify the visit UIKit removed before recording its shorter stack: else the detail's top, while the router selects it
        let departed = stacks[ObjectIdentifier(navigationController)]?.written.last.flatMap { id in
            controllers.values.first { ObjectIdentifier($0) == id && !navigationController.viewControllers.contains($0) }
        } ?? (detail?.topViewController as? RouteController).flatMap { $0.key == selectedKey ? $0 : nil }
        // The visits UIKit removed are among those Exact last wrote to the stack, and to the detail's it took
        let written = Set([navigationController, detail].compactMap { $0 }.flatMap { stacks[ObjectIdentifier($0)]?.written ?? [] })
        let poppedKeys = controllers.values.filter { written.contains(ObjectIdentifier($0)) && !navigationController.viewControllers.contains($0) }.map(\.key)
        let source = swipe.map { (node: $0.node, key: $0.key) } ?? departed.map { (node: $0.node, key: $0.key) }
        recordPop(navigationController)
        guard follows(navigationController), let viewController = navigationController.topViewController else { return }
        nativeMoved = true
        stopRevealing()
        // The route a pop took away keeps its block until it leaves the stack (LLP 1123 D2)
        defer { presenter.owners.publish() }
        defer {
            // Tree updates during UIKit's transition retain their latest
            // intent. Apply it once the native stack is available again.
            if pendingSync {
                pendingSync = false
                sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
            }
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
        let sourceKey = source.flatMap { presenter.views[$0.node.id] === $0.node ? $0.key : nil }
        // Replaced in place while the finger was down: the source's node is gone and the app's
        // stack is as deep as it was when the swipe began.
        let sourceReplaced = swipe != nil && sourceKey == nil
            && selectedRouteCount == swipe?.depth
        let rootKey = selectedKey ?? "", uikitTop = (viewController as? RouteController)?.key ?? ""
        let topKey = NavigationRules.poppedTop(selected: rootKey, inspecting: inspectorKey, shownKeys: shownKeys)
        let sourceRule = (viewController as? RouteController).map {
            NavigationRules.dispatchesBack(shownKey: $0.key, rootKey: topKey,
                                           sourceKey: sourceKey, sourceReplaced: sourceReplaced,
                                           modalActive: presenter.modals.inTransition)
        } ?? false
        let behindUIKit = !presenter.modals.inTransition
            && NavigationRules.appIsBehind(poppedKeys: poppedKeys, uikitTop: uikitTop, shownKeys: shownKeys, rootKey: topKey)
        // A cancelled pop performed no Back, whatever the app did to its source meanwhile
        let dispatches = !cancelled && (sourceRule || behindUIKit)
        if dispatches, !sourceRule {
            presenter.session?.log("navigation: UIKit popped to \(uikitTop); the app's stack follows it from \(rootKey)")
        }
        lastTransition = dispatches ? "completed" : (cancelled ? "cancelled" : "idle")
        guard dispatches else { return }
        if let destination = NavigationRules.beneathPoppedTop(selected: rootKey, inspecting: inspectorKey, shownKeys: shownKeys)
            ?? (swipe == nil ? (viewController as? RouteController)?.key : nil),
           let from = shownKeys.firstIndex(of: rootKey), let to = shownKeys.firstIndex(of: destination), from - to > 1 {
            goBack(from: rootKey, to: destination)
            return
        }
        goBack()
    }

    /// A stack Exact retired: its handle goes.
    func retireStack(_ nav: UINavigationController) {
        guard let stack = stacks.removeValue(forKey: ObjectIdentifier(nav)), stack.hatched else { return }
        _ = presenter.session?.natives.navigationHatch(nav, built: false, showsBar: stack.showsBar, label: stack.label)
    }

    func reset(clearFocus: Bool = true) {
        syncAfterTransitionArmed = false
        unanimated = false
        presenter.modals.reset()
        for nav in presentedNavigations { retireNavigation(nav, preserving: false) }
        for c in controllers.values { end(c) }
        retireTabs()
        if let primaryNavigation { retireStack(primaryNavigation) }
        retireSplit()
        if !covers.isEmpty || !delivered.isEmpty || !deliveredOwners.isEmpty {
            let cleared = Set(covers.keys).union(delivered.keys).filter { presenter.views[$0] != nil }
            let owners = deliveredOwners.keys.filter { presenter.views[$0] != nil }
            covers = [:]
            delivered = [:]
            ownerValues = [:]
            deliveredOwners = [:]
            if !cleared.isEmpty || !owners.isEmpty { presenter.onCovers?(cleared.map { ($0, nil) }, owners.map { ($0, nil) }) }
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
        lastTransition = "idle"
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
