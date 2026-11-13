// @ref LLP 1075.003 §3.7 — Exact's tab container on iOS: a
// `UITabBarController` with one controller per tab, made with it and never
// replaced: a navigation controller for a tab that can push or show a header,
// a plain one for a tab that shows one fixed route with no header
// (PlainTabIOS.swift).
// Each tab's stack is kept as the router keeps it (LLP 1038 §7's "one
// navigation owner per tab"), so a scroll offset, a draft or a pushed screen
// survives a switch. The authored `tablist` supplies the items — a symbol
// over a label, its `-fill` symbol when selected — and takes no room while
// the bar shows. A tap on the bar is UIKit's (`TabDelegateProxy`): on another
// tab, Exact presses that tab's authored tab inside `shouldSelect`, so the
// router follows; on the selected tab above its root, UIKit pops the stack,
// which the router follows as a Back. Under the agent
// the authored tablist paints and the bar stays hidden. A tab not yet
// selected has no routes: the runner builds a tab when it is first selected,
// as UIKit loads a tab's view, or when More's list is about to show its row
// (LLP 1075.003 §9.14). Which tabs a root has is NavigationTabs.swift's;
// what a tab shows, TabFace.swift's.
#if os(iOS) || os(tvOS)
import UIKit

/// The tab delegate Exact keeps (LLP 1075.003 §3.7): UIKit selects unless the
/// app's delegate or the router declines (`mayShow`), and the router follows
/// what UIKit selected (`followUIKit`); what Exact does not implement goes
/// straight to the app's delegate.
final class TabDelegateProxy: NSObject, UITabBarControllerDelegate {
    weak var host: NavigationHost?
    weak var app: UITabBarControllerDelegate?

    func tabBarController(_ tabs: UITabBarController, shouldSelect controller: UIViewController) -> Bool {
        if app?.tabBarController?(tabs, shouldSelect: controller) == false { return false }
        // A container built from `UITab`s asks `shouldSelectTab` (TabsIOS.swift)
        guard host?.builtTabs.isEmpty != false else { return true }
        return host?.mayShow(tabs, controller) ?? true
    }

    func tabBarController(_ tabs: UITabBarController, didSelect controller: UIViewController) {
        if host?.builtTabs.isEmpty != false { host?.followUIKit(tabs) }
        app?.tabBarController?(tabs, didSelect: controller)
    }

    #if os(iOS)
    /// More's Edit moved tabs between the bar and More, and UIKit their
    /// routes with them (`syncNextTurn`).
    func tabBarController(_ tabs: UITabBarController, didEndCustomizing controllers: [UIViewController], changed: Bool) {
        if changed { host?.syncNextTurn() }
        app?.tabBarController?(tabs, didEndCustomizing: controllers, changed: changed)
    }
    #endif

    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || (app?.responds(to: selector) ?? false)
    }

    override func forwardingTarget(for selector: Selector!) -> Any? {
        app?.responds(to: selector) == true ? app : nil
    }
}

extension NavigationHost {
    /// Every stack Exact built: the tabs' in tab order (or the one), then
    /// the sheets'.
    var allNavigations: [UINavigationController] {
        primaryStacks.compactMap { $0 as? UINavigationController } + presentedNavigations
    }

    /// Whether Exact's tab bar shows: not under the agent's own chrome,
    /// which sees the authored tablist (LLP 1021 D4's one presentation).
    var tabBarShows: Bool { tabController != nil && tabOwner === tabController && !ExactEnv.authoredChrome && !tablistAppearanceNone }
    /// Whether UIKit's tab bar stands for `node`: a root tab, or what it holds, while the bar shows.
    func tabBarStands(for node: NodeView) -> Bool {
        guard tabBarShows, let tabs = container.flatMap({ NavigationTabs.of($0, presenter) }) else { return false }
        return tabs.tabs.contains { node === $0 || node.isDescendant(of: $0) }
    }

    /// What a tab other than the selected one holds: its stack up to its
    /// first presentation. A sheet is presented only over the selected tab;
    /// another tab's stays the router's until that tab is selected again.
    func base(_ stack: [RouteController]) -> [RouteController] {
        Array(stack[NavigationRules.segments(presentations: stack.map { $0.node.props["navigationPresentation"] })[0]])
    }

    /// Build the stacks, a stack a panel, and what holds them, before any
    /// child is laid out: the app's container when its module, already
    /// connected, returns one; else Exact's tab controller.
    func installTabs(_ p: Projection, _ tabs: NavigationTabs, first: [RouteController], in parent: UIViewController) {
        tabPanels = tabs.panels.map(\.id)
        routerTab = p.at
        // Read from the plan which tabs need a navigation controller, so a tab not built yet gets its controller;
        // assume one where the plan gives no answer
        let kernel = presenter.session?.runtime.rootTabs(p.root.id)
        let needsNavigation = Dictionary(zip(kernel?.panels ?? [], kernel?.navigation ?? []), uniquingKeysWith: { $1 })
        let navs = tabs.panels.enumerated().map { index, panel in
            makeTabStack(panel.id, index == p.at ? first : base(p.wanted[index]), navigation: needsNavigation[panel.id] ?? true)
        }
        primaryNavigation = navs[p.at] as? UINavigationController
        syncItems(tabs)
        let owned = askTabContainer(tabs, selected: p.at)
        tabContainerAsked = presenter.session?.natives.hatchesConnected == true
        let holder: UIViewController = owned ?? {
            let container = UITabBarController()
            tabProxy.host = self
            if #available(iOS 18.0, tvOS 18.0, *), buildsTabs(tabs) {
                buildTabs(container, navs, tabs, unsaid: kernel?.unsaid ?? false, selected: p.at)
            } else {
                container.setViewControllers(navs, animated: false)
                container.selectedIndex = p.at
            }
            container.delegate = tabProxy
            tablistAppearanceNone = tabs.tablist.wrote("appearance") && tabs.tablist.style["appearance"]?.string == "none"
            if ExactEnv.authoredChrome || tablistAppearanceNone { hideTabBar(container) }
            tabController = container
            // A path's glyph is sized by the idiom and the size class: the items again as either changes (and
            // once the bar is in its window, whose traits it then has).
            container.tabBar.registerForTraitChanges([UITraitUserInterfaceIdiom.self, UITraitHorizontalSizeClass.self]) { [weak self] (_: UITabBar, _: UITraitCollection) in
                self?.resyncTabItems()
            }
            // A width becoming regular on an iPad makes the bar the floating one, which the stack's wish then hides.
            // A size class change on either axis moves tabs between the bar and More, their routes with them, which
            // the routes' moves report (`moved(into:)`, LLP 1075.003 §9.16); a sync in the next turn follows them and
            // builds the rows a More list on screen gains.
            container.registerForTraitChanges([UITraitHorizontalSizeClass.self, UITraitVerticalSizeClass.self]) { [weak self] (tabs: UITabBarController, previous: UITraitCollection) in
                let now = tabs.traitCollection
                if previous.horizontalSizeClass != now.horizontalSizeClass { self?.refollowFloatingBar() }
                // Skip the change UIKit reports as the container enters or leaves a window, from or to unspecified
                let known = [previous.horizontalSizeClass, previous.verticalSizeClass, now.horizontalSizeClass, now.verticalSizeClass].allSatisfy { $0 != .unspecified }
                if known { self?.syncNextTurn() }
            }
            #if os(iOS)
            adoptMore(container.moreNavigationController)
            #endif
            return container
        }()
        tabOwner = holder
        mount(holder, in: parent, at: p.root)
        tint(tabs.tablist)
        // Kept under the agent too, where the authored tablist shows in the
        // bar's place: a real touch refuses it (LLP 1080.000 D7).
        adoptedTablist = tabs.tablist.id
        if owned == nil, presenter.session?.natives.hatchesConnected == true, let container = tabController {
            presenter.session?.natives.tabsHatch(container, event: 0)
            tabsHatched = true
        }
        presenter.session?.log("navigation: \(navs.count) tabs in \(owned.map { "the app's \(type(of: $0))" } ?? "a UITabBarController")\(ExactEnv.authoredChrome ? " (the authored tablist shows)" : "")")
    }

    private func mount(_ holder: UIViewController, in parent: UIViewController, at root: NodeView) {
        parent.addChild(holder)
        if holder.view.accessibilityIdentifier == nil {
            holder.view.accessibilityIdentifier = "exact-navigation-\(UUID().uuidString)"
        }
        // Sized before it enters the window, where UIKit lays it out.
        holder.view.frame = ownerFrame(holder, in: root)
        holder.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        root.addSubview(holder.view)
        holder.view.setPaintForeground(aboveAuthored: false)
        fitOwner()
        holder.didMove(toParent: parent)
    }

    /// The module's `tabContainer`, when it is connected: the app's own
    /// container for these tabs' controllers, or nil.
    private func askTabContainer(_ tabs: NavigationTabs, selected: Int) -> UIViewController? {
        guard let natives = presenter.session?.natives, natives.hatchesConnected, natives.tabCalls != nil else { return nil }
        let names = tabs.panels.map { $0.props["id"] ?? "" }
        return natives.tabContainerHatch(names: names, nodes: tabs.tabs.map(\.id), selected: selected, controllers: tabPanels.compactMap { tabStacks[$0] }) as? UIViewController
    }

    /// At a cold launch the module connects after first pixel: Exact's tab
    /// controller hears `tabs`, then `tabContainer` may take its stacks into
    /// the app's own container; the content moves once (LLP 1075.003 Q3 (c)).
    /// A sheet presented over the tabs keeps the controller it was presented
    /// from: the app's container takes their place once nothing is presented
    /// (each batch asks again until then).
    func replayTabs() {
        guard let container = tabController, let natives = presenter.session?.natives, natives.hatchesConnected else { return }
        if !tabsHatched {
            tabsHatched = true
            natives.tabsHatch(container, event: 0)
        }
        guard !tabContainerAsked, presenter.modals.routes.isEmpty, !presenter.modals.inTransition, presentedNavigations.isEmpty,
              let root = self.container, let tabs = NavigationTabs.of(root, presenter), let parent = container.parent else { return }
        tabContainerAsked = true
        // Offer the stacks in place: a container the hook returns takes them from Exact's, as UIKit moves any
        // child. Emptying Exact's first would lose its bar's selection: a tab bar controller emptied and filled
        // again keeps its selected index but shows no tab selected (iOS 26 and 27).
        guard let owned = askTabContainer(tabs, selected: routerTab) else {
            let navs = tabPanels.compactMap { tabStacks[$0] }
            if !restoreBuiltTabs(container, navs), container.viewControllers?.count != navs.count {
                presenter.session?.log("hatch: tabContainer took the tabs' stacks but returned no container; Exact's has them back")
                container.setViewControllers(navs, animated: false)
                container.selectedIndex = max(0, routerTab)
            }
            return
        }
        natives.tabsHatch(container, event: 1)
        // Release More with Exact's container. The routes UIKit moved into it go first to their tab's own controller,
        // which the app's container now shows; a plain tab More showed is the app's container's already
        let heldInMore = moreTab != nil
        if let more, let tab = moreTab, let own = tabStack(tab) as? UINavigationController {
            setRoutes(more.stackRoutes.compactMap { $0 as? RouteController }, in: own)
        }
        if let more, primaryNavigation === more { primaryNavigation = tabStack(routerTab) as? UINavigationController }
        releaseMore()
        container.delegate = nil
        tablistRoot = nil
        tablistHidBar = false
        container.willMove(toParent: nil)
        container.view.removeFromSuperview()
        container.removeFromParent()
        tabController = nil
        tabOwner = owned
        mount(owned, in: parent, at: root)
        if heldInMore { syncNextTurn() }
        presenter.session?.log("hatch: the app's \(type(of: owned)) holds the tabs; the content moved after the first frame")
    }

    private func hideTabBar(_ container: UITabBarController) {
        if #available(iOS 18.0, tvOS 18.0, *) { container.setTabBarHidden(true, animated: false) } else { container.tabBar.isHidden = true }
    }

    /// Before installing: a root that gains, loses or changes its tabs gets
    /// its containers again (the router's stacks are untouched).
    func reshape(_ p: Projection) {
        let panels = p.tabs?.panels.map(\.id) ?? []
        guard primaryOwner != nil, panels != tabPanels || builtTabsChanged(p) else { return }
        presenter.session?.log("navigation: the tabs changed; the containers are built again")
        if tabOwner != nil { retireTabs() } else if let nav = primaryNavigation {
            release(nav)
            nav.willMove(toParent: nil)
            nav.view.removeFromSuperview()
            nav.removeFromParent()
        }
        primaryNavigation = nil
    }

    /// Each batch: every other tab's stack, the selected tab, its items.
    func syncTabs(_ p: Projection) {
        guard tabOwner != nil, let tabs = p.tabs else { return }
        // Re-adopt a replacement over unchanged panels and remove the standalone bar projected earlier in this batch
        if adoptedTablist != tabs.tablist.id {
            adoptedTablist = tabs.tablist.id
            measuredTablist = nil
            if tabController == nil { presenter.session?.natives.tabNodesChanged(tabs.tabs.map(\.id)) }
            presenter.segments.sync()
        }
        let authored = tabs.tablist.wrote("appearance") && tabs.tablist.style["appearance"]?.string == "none"
        if authored != tablistAppearanceNone {
            tablistAppearanceNone = authored
            presenter.session?.view?.setNeedsLayout()
            tablistRoot = nil
            tablistHidBar = false
            if let container = tabController, tabOwner === container, !ExactEnv.authoredChrome {
                let hidden = authored || tabs.tablist.style["display"]?.string == "none"
                if #available(iOS 18.0, tvOS 18.0, *) { container.setTabBarHidden(hidden, animated: false) }
                else { container.tabBar.isHidden = hidden }
                tablistHidBar = hidden && !authored
            }
        }
        followSidebar(p)
        let navs = tabs.panels.compactMap { tabStacks[$0.id] }
        guard navs.count == tabs.panels.count else { return }
        // Write each other tab's routes where UIKit holds them: in More's
        // stack for the tab whose routes UIKit moved there (`routesStack`),
        // else in the tab's own. A tab's controller is empty until the runner
        // builds the tab, when it is first selected or More's list is about
        // to show it (LLP 1075.003 §9.14). A stack that holds the routes
        // already is only projected again; one UIKit is moving (`writable`) is
        // written once the move ends.
        for index in navs.indices where index != p.at {
            let stack = routesStack(index) ?? navs[index]
            let routes = base(p.wanted[index])
            if stack.stackRoutes.elementsEqual(routes, by: ===) || writable(stack, routes) { setRoutes(routes, in: stack) } else { tabsHeld = true }
        }
        // Fill the router's tab's controller with its first routes, or project
        // its bar when the router's tab changes, before UIKit shows it, where
        // nothing animates in; `sync` writes the rest of its stack.
        let selected = routesStack(p.at) ?? navs[p.at]
        if selected.stackRoutes.isEmpty { if writable(selected, p.wanted[p.at]) { setRoutes(base(p.wanted[p.at]), in: selected) } }
        else if routerTab != p.at { prepareRoutes(base(p.wanted[p.at]), in: selected) }
        // @ref LLP 1038 D6 — a tab change swaps immediately. UIKit's selection
        // is the state (LLP 1075.003 §3.7): it is written only when the
        // router's own tab changes (a link, the agent's authored tablist, a
        // tapped tab's press that selects another), so no batch undoes a tap.
        // It is written by controller, not by index, because More's Edit
        // reorders tabs. A tab UIKit is selecting (`tapping`) is left to UIKit.
        if routerTab != p.at {
            routerTab = p.at
            selectionPressed = nil
            if let container = tabController, container.selectedViewController !== navs[p.at], navs[p.at] !== tapping {
                container.selectedViewController = navs[p.at]
                refollowFloatingBar() // (no push or pop: the stack it shows keeps its wish on an iPad)
            }
            if tabController == nil { presenter.session?.natives.tabsHatch(nil, event: 2, index: p.at) }
        }
        // Follow More's stack for a tab under More: selecting it moved its routes there
        primaryNavigation = (routesStack(p.at) ?? navs[p.at]) as? UINavigationController
        syncItems(tabs)
    }

    /// The picture a tab's raster source names: the app's own file under
    /// `app:/`, read through no link (`AppFiles.open`), else a bundled asset
    /// of the session's resolver.
    private func raster(_ source: String) -> UIImage? {
        if source.hasPrefix("app:/") { return AppFiles.open(source).flatMap { try? $0.readToEnd() }.flatMap { UIImage(data: $0) } }
        return presenter.session?.app.resolver?.url(source, maximumBytes: 4 << 20).flatMap { UIImage(contentsOfFile: $0.path) }
    }

    /// The items, from the authored tabs: each tab's own item, written in
    /// place when its face changes, so what else the app set on it (a badge)
    /// and a handle to it stay good.
    /// The items again, from the tabs they were last made from (a size class change).
    func resyncTabItems() {
        if let tabs = tabItemsSource { syncItems(tabs) }
    }

    private func syncItems(_ tabs: NavigationTabs) {
        tabItemsSource = tabs
        for tab in tabs.tabs { tab.rejectNativeActiveProperties(on: "UITabBarItem") }
        let navs = tabs.panels.compactMap { tabStacks[$0.id] }
        let faces = tabs.tabs.map(TabFace.init)
        // A tab's test id is its item's identifier, which XCUITest and
        // Maestro address it by (LLP 1116 D2); without one, its symbol or title.
        let ids = zip(tabs.tabs, faces).map { tab, face in tab.props["testId"] ?? face.base.flatMap { FaceSymbol.isPath($0) ? nil : $0 } ?? face.title }
        // The idiom and the size class size a path's glyph (`FaceSymbol.tabBarPoints`).
        // The bar's traits once it is in its window; before, the page's.
        let traits = tabController.flatMap { $0.tabBar.window == nil ? nil : $0.tabBar.traitCollection }
            ?? presenter.session?.view?.traitCollection ?? UITraitCollection.current
        let points = FaceSymbol.tabBarPoints(traits)
        let signature = zip(faces, ids).map { face, id in "\(points)|\(id)|\(face.base ?? "")|\(face.raster ?? "")|\(face.title)|\(face.disabled)|\(face.accessibilityDisabled)|\(face.badge ?? "")" }
        for (index, (nav, face)) in zip(navs, faces).enumerated() where !tabItems.indices.contains(index) || tabItems[index] != signature[index] {
            let item: UITabBarItem = nav.tabBarItem
            // An svg-only icon's path (stage 9 A2) at a tab bar's glyph size, one image for both states;
            // a bundled raster the same way, as a hand-built bar takes a custom image.
            let image = face.base.flatMap { FaceSymbol.image($0, points: FaceSymbol.tabBarPoints(traits)) }
                ?? face.raster.flatMap { raster($0) }.flatMap { TabFace.rasterImage($0, points: FaceSymbol.tabBarPoints(traits)) }
            // Title the tab's controller, which titles its item and its navigation item (`UIViewController.title`):
            // More's bar shows a plain tab's when More pushes the tab.
            nav.title = face.title
            item.image = image
            item.selectedImage = face.base.flatMap(TabFace.filled) ?? image
            item.accessibilityIdentifier = ids[index]
            item.isEnabled = !face.disabled
            if face.disabled || face.accessibilityDisabled { item.accessibilityTraits.insert(.notEnabled) }
            else { item.accessibilityTraits.remove(.notEnabled) }
            // An authored badge, or one it just lost; a badge a hatch set on
            // a tab that never authored one is left alone.
            let had = tabItems.indices.contains(index) && !tabItems[index].hasSuffix("|")
            if face.badge != nil || had { item.badgeValue = face.badge }
            writeBuiltTab(index, item: item, title: face.title)
        }
        tabItems = signature
        tint(tabs.tablist)
        followSearchTab()
    }

    /// The tab bar's own tint, as a hand-built app tints one: the selected tab's own tint where it
    /// writes a colour of its own (or a share of the tint), as a value, else the accent of the tab or
    /// its tablist (`ownAccent`), nil where that is the window's. Its selected item in the tint draws
    /// it, and UIKit greys it behind a sheet or an alert. Resolved per appearance, so dark mode follows.
    func tint(_ list: NodeView) {
        guard let bar = tabController?.tabBar else { return }
        var accented = list
        if let tabs = tabItemsSource, tabs.tablist === list, tabs.tabs.indices.contains(routerTab) {
            let selected = tabs.tabs[routerTab]
            accented = selected
            if selected.ownColor("tint_color") != nil, selected.style["tint_color"]?.tintAlpha != 1,
               let light = selected.carriedChannels("tint_color", dark: false), let dark = selected.carriedChannels("tint_color", dark: true) {
                let wanted = [light, dark]
                guard tabTint != wanted else { return }
                tabTint = wanted
                bar.tintColor = UIColor { TextEngine.color($0.userInterfaceStyle == .dark ? dark : light) }
                return
            }
        }
        tabTint = nil
        assign(bar, \.tintColor, accented.ownAccent)
    }

    /// The tab whose controller is `controller`, found by controller because
    /// More's Edit may have moved it.
    func tabIndex(of controller: UIViewController?) -> Int? {
        guard let controller else { return nil }
        return tabPanels.firstIndex { tabStacks[$0] === controller }
    }

    /// UIKit's More navigation controller as Exact adopted it (iOS): it holds
    /// the routes of a tab under More that it shows, above its list. Read
    /// from Exact's record, not `moreNavigationController`, which makes a
    /// More where UIKit has none.
    var more: UINavigationController? { adoptedMore }

    /// Whether Exact may write `routes` into `stack` now: UIKit runs no
    /// transition there (a tab tapped in the turn of its pop leaves one there
    /// until the tab shows again, LLP 1075.003 §9.16), and moves none of the
    /// routes (`routesInFlight`). The write waits for the transition's end
    /// (`runHeldSync`) or the move's.
    func writable(_ stack: UIViewController, _ routes: [RouteController]) -> Bool {
        (stack as? UINavigationController)?.transitionCoordinator == nil && !routesInFlight(routes)
    }

    /// The tab whose routes UIKit holds in More's stack: a row of its list
    /// picked, or the tab selected in code, moves a navigation controller's
    /// routes there, and a size class change moves them between More and the
    /// tab's own controller (LLP 1075.003 §9.16). A reorder by More's Edit
    /// that puts the tab on the bar is taken to move them back, as a width
    /// does; it is not measured.
    var moreTab: Int? {
        guard let route = more?.viewControllers.lazy.compactMap({ $0 as? RouteController }).first else { return nil }
        return tabIndex(ofRoute: route)
    }

    /// The stack that holds tab `index`'s routes: More while UIKit holds them
    /// there, else the tab's own.
    func routesStack(_ index: Int) -> UIViewController? {
        index == moreTab ? more : tabStack(index)
    }

    /// Tab `index`'s item, when Exact's tab bar shows it: a tab whose item
    /// the bar does not show is a row of More's list (iOS).
    func barItem(ofTab index: Int) -> UITabBarItem? {
        guard let item = tabStack(index)?.tabBarItem else { return nil }
        // UIKit makes the items of a bar built from `UITab`s: Exact's item carries the tab's face and test id
        if !builtTabs.isEmpty { return tabController != nil && builtTabInBar(index) ? item : nil }
        return tabController?.tabBar.items?.contains(where: { $0 === item }) == true ? item : nil
    }

    /// The tab a route belongs to, by its tabpanel.
    func tabIndex(ofRoute route: RouteController) -> Int? {
        logicalParent(of: route.node).flatMap { tabPanels.firstIndex(of: $0.id) }
    }

    /// More shows a controller. A tab UIKit selected that the router is not
    /// on is pressed now, before More's push draws (`followSelection`), as
    /// `mayShow` presses a tab a tap on the bar selects: UIKit reports the tab
    /// selected by More's `willShow` for a row of its list picked, which
    /// sends no tab delegate call, and for the tab selected in code. For a tap
    /// on More from another tab, UIKit reports it only at `didSelect`, which
    /// presses it (`followUIKit`), and during a pop within the tab's routes it
    /// reports the tab the router is on, so neither presses anything here
    /// (LLP 1075.003 §9.16). The routes UIKit moved into More are then
    /// projected as pushed over its list, and More's bar shown as the top one
    /// wants, before the push draws: the bar was decided for them before
    /// this, as their own stack's. More's list, shown again by a pop or a
    /// swipe, gets the tabs it shows built (`buildTabsUnderMore`).
    func followMore(_ shown: UIViewController, animated: Bool) {
        guard let more else { return }
        watchPops(more)
        guard let index = tabIndex(ofShown: shown) else {
            if shown === more.viewControllers.first { buildTabsUnderMore() }
            return
        }
        // Follow More when the router's own tab shows again there: UIKit moved its routes back into More
        if index == routerTab { primaryNavigation = routesStack(index) as? UINavigationController ?? primaryNavigation }
        followSelection(showing: shown)
        let routes = more.stackRoutes.compactMap { $0 as? RouteController }
        guard !routes.isEmpty else { return }
        prepareRoutes(routes, in: more)
        showBar(more, for: shown as? RouteController, animated: animated)
    }

    /// Press the tab UIKit selected when the router is on another and UIKit
    /// shows that tab (`shown` is one of its controllers), once for that selection, with
    /// `tapping` set as a tap's press runs, because UIKit has selected it.
    /// While a selection is applied, UIKit can show the new tab with the old
    /// one still reported selected (LLP 1075.003 §9.16), and a shown
    /// controller of another tab than the selected one presses nothing. A
    /// press that leaves the router where it was is the app declining the tab
    /// (`declineMoreRow`).
    func followSelection(showing shown: UIViewController) {
        guard let tabs = tabController, let selected = tabs.selectedViewController, let index = tabIndex(of: selected),
              index != routerTab, tabIndex(ofShown: shown) == index, tapping == nil, selected !== selectionPressed,
              let tab = authoredTab(index) else { return }
        selectionPressed = selected
        tapping = selected
        defer { tapping = nil }
        pressTab(tab)
    }

    /// The tab of a controller More shows: a plain tab it pushed for a row,
    /// or a route UIKit moved into it.
    func tabIndex(ofShown shown: UIViewController) -> Int? {
        tabIndex(of: shown) ?? (shown as? RouteController).flatMap(tabIndex(ofRoute:))
    }

    /// Whether More showed a row's tab that the app declined: the row's press
    /// left the router on another tab. Exact pops More to its list, so More
    /// shows its list rather than the declined tab the next time it shows,
    /// and selects the router's tab again.
    func declineMoreRow(_ shown: UIViewController) -> Bool {
        guard let more, let index = tabIndex(ofShown: shown), index != routerTab, tabStack(routerTab) != nil else { return false }
        more.popToRootViewController(animated: false)
        showRouterTab(declining: index)
        return true
    }

    /// A tap on the bar, before UIKit selects. A pop UIKit made before the tap
    /// reaches the router first (`followPops`). A tap on another tab presses
    /// that tab's authored tab now, as a click on the web's tablist presses
    /// it, so the tab is built and projected before UIKit shows it, as a
    /// hand-built app's tab is ready before it is tapped. A press that leaves
    /// the router on another tab is the app's answer: the tap selects
    /// nothing, and UIKit shows the router's tab (`syncTabs` selects the
    /// router's tab when the press moved the router to a third tab). A tap on
    /// the tab UIKit shows, above its root, lets UIKit pop it to the root
    /// only where Back goes (`backAllows`), and the tab is pressed after that
    /// Back; at the root, its screen scrolls to its top after the tab's press
    /// (`followUIKit`). More is UIKit's: a tap that shows its list gets the
    /// tabs it lists built first (`buildTabsUnderMore`).
    func mayShow(_ tabs: UITabBarController, _ controller: UIViewController) -> Bool {
        followPops()
        selectionPressed = nil
        #if os(iOS)
        // Adopt the More a tap on its item shows, which UIKit may have made anew; only a controller that is no tab's is
        // read against `moreNavigationController`, which makes a More where UIKit has none
        if tabIndex(of: controller) == nil, let nav = controller as? UINavigationController, nav === tabs.moreNavigationController { adoptMore(nav) }
        #endif
        if controller === more, let more {
            // Let Back's rule decide More's pop to its list when More is tapped above a tab's root: UIKit pops to the list (measured)
            if more.viewIfLoaded?.window != nil, more.stackRoutes.count > 1 { return backAllows("More's pop to its list") }
            buildTabsUnderMore()
            return true
        }
        guard let index = tabIndex(of: controller) else { return true }
        if controller === tabs.selectedViewController {
            if controller.stackRoutes.count > 1 {
                return backAllows("tab reselect")
            }
            reselected = reselectedRoot(index)
            return true
        }
        tapping = controller
        // Let it go after this turn if no `didSelect` came: on iOS UIKit sends it within this call (measured);
        // tvOS selects by focus, and its order is not measured
        DispatchQueue.main.async { [weak self, weak controller] in
            if let self, tapping === controller { tapping = nil }
        }
        guard index != routerTab, let tab = authoredTab(index) else { return true }
        pressTab(tab)
        guard routerTab != index else { return true }
        tapping = nil
        presenter.session?.log("navigation: tab \(index)'s press left the router on tab \(routerTab), which UIKit shows")
        return false
    }

    /// UIKit selected a tab. A pop UIKit committed reaches the router first
    /// (`followPops`): a tap on the selected tab above its root has popped
    /// its stack (LLP 1075.003 §9.16: UIKit's pop has its coordinator by
    /// `didSelect` and its `willShow` after it), and on the `tabs` path code
    /// can pop a stack in the turn it selects a tab. After a tap on another
    /// tab the router shows it already (`mayShow`), and the tab is laid out
    /// and its covers reported once, as `sync` reports after its own layout
    /// (`tapping`). A tab UIKit shows that the router does not (one under
    /// More, after a tap on More from another tab, or one selected in code on
    /// the `tabs` path) is pressed now, and a press that leaves the router on
    /// another tab selects the router's tab again. A tab tapped again is
    /// pressed: after the Back above its root; at its root, and then its
    /// screen scrolls to its top. So the tab's press runs on every tap of its
    /// item, as the web's tablist presses a tab on every click.
    func followUIKit(_ tabs: UITabBarController) {
        let popped = followPops()
        let screen = reselected
        reselected = nil
        if tapping != nil {
            // Sync again when the press's sync waited for a transition still running on the tapped tab or a sheet,
            // now that UIKit selected the tab, before the tab's first frame
            if pendingSync {
                tapping = nil
                resync()
                return refollowFloatingBar()
            }
            refollowFloatingBar()
            tabs.view.layoutIfNeeded()
            tapping = nil
            return reportCovers()
        }
        guard let index = tabIndex(of: tabs.selectedViewController) else { return }
        if index == routerTab, screen == nil, !popped { return refollowFloatingBar() }
        guard let tab = authoredTab(index) else { return }
        let again = index == routerTab
        pressTab(tab, scrolling: screen)
        // Leave the router's own tab, tapped again, to its press; another tab's press can leave the router elsewhere
        if again { return }
        guard routerTab != index, tabStack(routerTab) != nil else { return refollowFloatingBar() }
        showRouterTab(declining: index)
    }

    /// Select the router's tab again: tab `index`'s press left the router on
    /// it, the app's answer to a person's pick of `index`.
    func showRouterTab(declining index: Int) {
        guard let tabs = tabController, let stack = tabStack(routerTab) else { return }
        selectionPressed = nil
        tabs.selectedViewController = stack
        presenter.session?.log("navigation: tab \(index)'s press left the router on tab \(routerTab), which UIKit shows again")
    }

    /// Whether a tap may let UIKit pop a stack to its root: only where Back
    /// goes (LLP 1035.001 D1); else the journal says why.
    private func backAllows(_ pop: String) -> Bool {
        if canInvokeBack { return true }
        presenter.session?.log("\(pop) refused: \(backRefusal)")
        return false
    }

    /// Press `tab` as a tap on its item does, unless the tab is disabled.
    /// Then scroll `screen` (`reselectedRoot`), if given, to its top unless
    /// the press asked for a row there (`scrollToTop`).
    func pressTab(_ tab: NodeView, scrolling screen: RouteController? = nil) {
        let requests = screen == nil ? nil : presenter.collections.lastRequests()
        if !tab.disabled { presenter.press(tab.id) }
        if let screen { scrollToTop(screen, since: requests) }
    }

    /// The authored tab whose panel is tab `index`.
    private func authoredTab(_ index: Int) -> NodeView? {
        guard let root = container, let authored = NavigationTabs.of(root, presenter), authored.tabs.indices.contains(index) else { return nil }
        return authored.tabs[index]
    }

    /// The screen a tap on the bar's item for tab `index` scrolls to its
    /// top: the selected tab's, tapped again at its stack's root with
    /// nothing presented over it, as every tab bar app does (LLP 1115.000
    /// D11.3); UIKit itself scrolls nothing then. Above the root a tap on the
    /// bar pops to it (`mayShow`), the agent's as a person's, and the agent's
    /// tap on a row of More's list presses the tab, whose `select` pops;
    /// another tab's tap selects it: nil for all. Read before
    /// the tab's press, scrolled after it (`scrollToTop`).
    func reselectedRoot(_ index: Int) -> RouteController? {
        guard index == routerTab, let routes = routesStack(index)?.stackRoutes,
              routes.count == 1, presentedNavigations.isEmpty else { return nil }
        return routes.last as? RouteController
    }

    /// A route's content scroller (`contentScroller`, the status bar's) to
    /// its top, as the status bar's tap takes it: UIKit's animation, to where
    /// a large title is open, which UIKit opens as the offset arrives.
    /// UIKit's tab bar does not do this itself: a selected tab tapped again
    /// scrolls nothing in UIKit alone (iOS 27, a real touch). It is the
    /// reader's move of a list there (`CollectionHost.readerMoves`), and it
    /// gives way to a `scrollIntoView` the tab's press asked for there,
    /// landed, under way or already shown: the author's answer to the tap
    /// wins over the platform's (LLP 1070.000 §2.6). `requests`: every
    /// list's latest request before the press (`CollectionHost.lastRequests`),
    /// read for them all because the press may make the screen's list, or
    /// make another list the screen's.
    func scrollToTop(_ c: RouteController, since requests: [UInt32: UInt64]? = nil) {
        guard let sv = contentScrollView(of: c) else { return }
        let node = contentScroller(of: c)
        if let node, presenter.collections.owns(node.id) {
            if let requests, presenter.collections.lastRequest(node.id) != requests[node.id] ?? 0 { return }
            presenter.collections.readerMoves(node.id)
        }
        // At its top under a collapsed title it goes on to the open title's
        // inset, as UIKit's status-bar tap does, and opens it.
        let inset = max(sv.adjustedContentInset.top, node != nil && node === c.collapseScroll ? node?.scrollOrigin ?? 0 : 0)
        guard sv.contentOffset.y > 0.5 - inset else { return restAtTop(sv, in: c) }
        guard scrollsToTopAnimated else { return restAtTop(sv, in: c) }
        sv.setContentOffset(PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: -inset), in: sv, clamped: false), animated: true)
    }

    /// Where `scrollToTop` ends, in one turn, under the agent's frozen clock,
    /// where Exact starts no UIKit animation: the content at its top and a
    /// large title expanded. Measured in UIKit alone on iOS 27, an offset written
    /// at the top under a collapsed title leaves it collapsed, and one written
    /// above the top opens the title and leaves the content that far below
    /// the bar. So the scroller is pulled past its top and put at rest there,
    /// unanimated: no frame shows the pull. This is the one place Exact moves
    /// a large title itself.
    func restAtTop(_ sv: UIScrollView, in c: RouteController) {
        sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: -sv.adjustedContentInset.top), in: sv, clamped: false)
        // tvOS has no large titles.
        #if !os(tvOS)
        guard c.navigationItem.largeTitleDisplayMode == .always, c.viewIfLoaded?.window != nil,
              let nav = c.navigationController, nav.topViewController === c, nav.transitionCoordinator == nil,
              !nav.isNavigationBarHidden else { return }
        UIView.performWithoutAnimation {
            // By the bar's height at a time, while the title still opens:
            // UIKit grows the inset by what the pull opened of it.
            for _ in 0..<4 {
                let top = sv.adjustedContentInset.top
                sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: -top - nav.navigationBar.frame.height), in: sv, clamped: false)
                nav.view.layoutIfNeeded()
                if sv.adjustedContentInset.top <= top + 0.5 { break }
            }
            sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: -sv.adjustedContentInset.top), in: sv, clamped: false)
        }
        #endif
    }

    /// What holds the tabs goes: its stacks are retired, the authored
    /// tablist paints again.
    func retireTabs() {
        // The tablist's hold on the bar goes with its container (§3.7).
        tablistRoot = nil
        tablistHidBar = false
        for stack in tabPanels.compactMap({ tabStacks[$0] }) {
            if let nav = stack as? UINavigationController { release(nav) } else { (stack as? PlainTab)?.show(nil) }
        }
        releaseMore()
        if tabsHatched, tabController != nil { presenter.session?.natives.tabsHatch(tabController, event: 1) }
        if tabController == nil, tabOwner != nil { presenter.session?.natives.tabsHatch(nil, event: 3) }
        if let holder = tabOwner {
            (holder as? UITabBarController)?.delegate = nil
            holder.willMove(toParent: nil)
            holder.view.removeFromSuperview()
            holder.removeFromParent()
        }
        tabController = nil
        tabOwner = nil
        builtTabs = []
        tabsHatched = false
        tabContainerAsked = false
        routerTab = -1
        tabStacks = [:]
        tabPanels = []
        tabItems = []
        tabItemsSource = nil
        tabTint = nil
        adoptedTablist = nil
        tablistAppearanceNone = false
        measuredTablist = nil
    }

    /// After a batch: the projection, unless the batch only moved a list's
    /// rows and nothing is owed; then what presentations inherit and the
    /// authored tablist's covers.
    func batchApplied(_ batch: Batch, rowsOnly: Bool) {
        if !rowsOnly || syncOwed { sync(batch) }
        presenter.modals.inheritFromAncestors()
        followAuthoredTablist()
    }

    /// The authored tablist moved or changed size since its covers were
    /// measured: measure again. Its own cover's change (the bar's `whole`
    /// given back, an edge's padding) moves it in a batch that projects
    /// nothing, after the sync that measured it, so the covers settle only
    /// when they are measured where it now is.
    func followAuthoredTablist() {
        if ExactEnv.authoredChrome, !syncing { fitOwner() }
        guard tablistAppearanceNone, !syncing, let view = presenter.session?.view,
              let list = adoptedTablist.flatMap({ presenter.views[$0] }) else { return }
        if list.convert(list.bounds, to: view) != measuredTablist { reportCovers() }
    }

    /// A bar visible in its window, including every ancestor's visibility.
    static func shownBar(_ bar: UIView?) -> UIView? {
        guard let bar, let window = bar.window else { return nil }
        var at: UIView? = bar
        while let view = at {
            if view.isHidden || view.alpha < 0.01 { return nil }
            at = view.superview
        }
        let frame = bar.convert(bar.bounds, to: window)
        return frame.width > 1 && frame.height > 1 && frame.intersects(window.bounds) ? bar : nil
    }

    /// Whether a tab container takes this tablist's place.
    func adopts(tablist: NodeView) -> Bool {
        adoptedTablist == tablist.id && tabOwner != nil && !ExactEnv.authoredChrome &&
            !(tablist.wrote("appearance") && tablist.style["appearance"]?.string == "none")
    }
}

extension UITabBarController {
    /// Its tabs' controllers in tab order, whether it holds them as
    /// `viewControllers` or as `UITab`s, whose `viewControllers` is nil.
    var tabControllers: [UIViewController]? {
        if let viewControllers { return viewControllers }
        guard #available(iOS 18.0, tvOS 18.0, *), !tabs.isEmpty else { return nil }
        return tabs.compactMap(\.viewController)
    }

    /// The bar the controller projects, including iPadOS's floating list of tab items.
    var projectedTabBar: UIView? {
        if #available(iOS 18.0, tvOS 18.0, *), isTabBarHidden { return nil }
        #if os(iOS)
        return tabBar.window != nil ? tabBar : floatingTabList
        #else
        return tabBar
        #endif
    }

    /// The projected bar, when it is visibly on screen.
    var shownTabBar: UIView? { NavigationHost.shownBar(projectedTabBar) }
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
    var floatingTabList: UICollectionView? {
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
#endif
