#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// Exact's tab container built from `UITab`s (TabsIOS.swift) over
/// `scripts/fixtures/search-tabs.contract`: the search tab a `UISearchTab`
/// (LLP 1141.002 D1–D3) and, on an iPad, the unsaid root tab bar UIKit's
/// sidebar-adaptable one (LLP 1116.004 D1.1). And over
/// `scripts/fixtures/beside-routes.contract`, a box beside the routes the
/// native container does not cover (LLP 1083.000 D4).
final class TabsIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []
    private var animationsWereEnabled = true
    private var turned = false

    override func setUp() {
        super.setUp()
        animationsWereEnabled = UIView.areAnimationsEnabled
        UIView.setAnimationsEnabled(false)
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        if turned { turn(.portrait) }
        window?.isHidden = true
        window = nil
        UIView.setAnimationsEnabled(animationsWereEnabled)
        super.tearDown()
    }

    private func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    /// The fixture `key` names in a window the size of the scene's screen.
    private func fixture(_ key: String) throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment[key], "build.mjs --test --ios compiles \(key)")
        let session = ExactApp.shared.makeSession(label: key)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let frame = window.windowScene?.screen.bounds ?? CGRect(x: 0, y: 0, width: 402, height: 874)
        window.frame = frame
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: frame.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the navigation is ready") {
            let selected = session.presenter.navigationHost.tabController?.selectedViewController ?? session.presenter.navigationHost.primaryNavigation
            return selected?.viewIfLoaded?.window != nil && !Agent(session: session).nativeInFlight()
        }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId && $0.window != nil }
            ?? session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    private func topRoute(_ session: ExactSession) -> String? {
        let navigation = session.presenter.navigationHost
        let stack = navigation.tabController?.selectedViewController ?? navigation.primaryNavigation
        return (stack?.stackRoutes.last as? RouteController)?.node.props["testId"]
    }

    private func turn(_ orientation: UIInterfaceOrientationMask) {
        guard let scene = window?.windowScene ?? UIApplication.shared.connectedScenes.first as? UIWindowScene else { return }
        scene.requestGeometryUpdate(.iOS(interfaceOrientations: orientation))
        spin(1)
    }

    /// LLP 1141.002 D1, D2: the tab that says `rel="search"` is a
    /// `UISearchTab` with its authored face, the others `UITab`s named by
    /// their panels; a tap on it presses the authored tab, and the router's
    /// selection of another selects that one.
    func testTheSearchTabIsAUISearchTabWithItsFace() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let navigation = session.presenter.navigationHost
        let tabs = try XCTUnwrap(navigation.tabController)
        XCTAssertNil(tabs.viewControllers, "the container holds UITabs")
        XCTAssertEqual(tabs.tabs.count, 3)
        XCTAssertEqual(tabs.tabs.prefix(2).map(\.identifier), ["panel-home", "panel-saved"], "a tab is its panel")
        let search = try XCTUnwrap(tabs.tabs.last as? UISearchTab, "rel=\"search\" is the search tab")
        XCTAssertEqual(search.title, "Speakers", "its own label")
        XCTAssertNotNil(search.image, "its own symbol")
        XCTAssertEqual(search.accessibilityIdentifier, "tab-find", "its test id")
        XCTAssertEqual(tabs.tabs[0].title, "Home")
        XCTAssertEqual(tabs.tabs[0].accessibilityIdentifier, "tab-home")
        XCTAssertEqual(tabs.selectedTab?.identifier, "panel-home")
        // The panel is built after the first frame, so the field's `autofocus` is known before a person can tap.
        if #available(iOS 26.0, *) {
            until("the search tab activates its field when selected") { search.automaticallyActivatesSearch }
        }
        tapTab(tabs, 2)
        until("the router shows the search tab's root") { self.topRoute(session) == "route-find" }
        XCTAssertTrue(tabs.selectedTab === search)
        XCTAssertEqual(navigation.routerTab, 2)
        let root = try XCTUnwrap(navigation.tabStack(2)?.stackRoutes.first as? RouteController)
        XCTAssertNotNil(root.navigationItem.searchController, "the search tab's field is its root's header search")
        // The router selects another tab: UIKit's selection follows.
        tapTab(tabs, 0)
        until("Home again") { self.topRoute(session) == "route-home" }
        XCTAssertEqual(tabs.selectedTab?.identifier, "panel-home")
        XCTAssertEqual(navigation.routerTab, 0)
    }

    /// The router's handshake on the UITab path: a push in a tab, a visit to
    /// another tab that keeps it, and a tap on the tab above its root that
    /// pops it, which the router follows as a Back.
    func testATabKeepsItsStackAndAReselectPopsIt() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        XCTAssertFalse(tabs.tabs.isEmpty)
        _ = Agent(session: session).tap(["id": Int(try node(session, "open-detail").id)])
        until("Detail pushed") { self.topRoute(session) == "route-detail" }
        tapTab(tabs, 1)
        until("Saved shown") { self.topRoute(session) == "route-saved" }
        tapTab(tabs, 0)
        until("Home's stack kept its Detail") { self.topRoute(session) == "route-detail" }
        tapTab(tabs, 0)
        until("a tap above the root pops to it") { self.topRoute(session) == "route-home" }
        XCTAssertEqual(session.presenter.navigationHost.selectedKey, (tabs.selectedViewController?.stackRoutes.first as? RouteController)?.key,
                       "the router followed the pop")
    }

    /// A tab selected in code, as an app's own code may select one: on the
    /// UITab path UIKit sends `didSelectTab` for it (LLP 1141.002 §6), which
    /// is `followUIKit`, so the router follows within the selection, before
    /// the tab's route appears (LLP 1075.003 §3.7).
    func testATabSelectedInCodeIsFollowedWithinTheSelection() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let navigation = session.presenter.navigationHost
        let tabs = try XCTUnwrap(navigation.tabController)
        XCTAssertFalse(tabs.tabs.isEmpty)
        tabs.selectedTab = tabs.tabs[1]
        XCTAssertEqual(navigation.routerTab, 1, "the router follows within the selection")
        until("Saved shown") { self.topRoute(session) == "route-saved" }
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
    }

    /// Home popped and Saved selected in code in one turn, on the UITab
    /// path, where the app declines Saved: `didSelectTab` comes before the
    /// pop's `willShow`, so the pop reaches the router there first, as
    /// Home's Back, and Saved's press after it leaves the router on Home,
    /// which UIKit selects again.
    func testAPopAndADeclinedSelectionInCodeInOneTurn() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let navigation = session.presenter.navigationHost
        let tabs = try XCTUnwrap(navigation.tabController)
        let home = try XCTUnwrap(navigation.primaryNavigation)
        let agent = Agent(session: session)
        _ = agent.tap(["id": Int(try node(session, "hold").id)])
        _ = agent.tap(["id": Int(try node(session, "open-detail").id)])
        until("Detail pushed") { self.topRoute(session) == "route-detail" && !navigation.inTransition }
        let root = try XCTUnwrap(home.viewControllers.first as? RouteController)
        home.popViewController(animated: false)
        tabs.selectedTab = tabs.tabs[1]
        until("UIKit selects Home again") { tabs.selectedTab === tabs.tabs[0] && !navigation.inTransition }
        XCTAssertEqual(navigation.routerTab, 0, "the app declined Saved")
        XCTAssertEqual(navigation.selectedKey, root.key, "Home's Back reached the router")
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
    }

    /// LLP 1116.004 D1.1: on an iPad, the root tab bar whose placement the
    /// author left unsaid is UIKit's sidebar-adaptable bar. Where UIKit shows
    /// the sidebar (a landscape window), the selected route clears it as it
    /// clears any bar, and its content is laid out beside it; hidden, the
    /// content takes the width back.
    func testAnIPadShowsTheUnsaidTabBarAsASidebar() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        guard UIDevice.current.userInterfaceIdiom == .pad else { throw XCTSkip("the sidebar is an iPad's") }
        guard UIApplication.shared.connectedScenes.contains(where: { $0 is UIWindowScene }) else {
            throw missing("scene", "the iPad sidebar needs a window with the scene's regular width")
        }
        turned = true
        turn(.landscapeRight)
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        XCTAssertEqual(tabs.mode, .tabSidebar)
        let search = try node(session, "tab-find")
        let agent = Agent(session: session)
        waitFor("UIKit presents the pinned search tab") {
            guard case .success(let view)? = agent.tabItem(search), let model = tabs.tabs.first(where: { $0 is UISearchTab }),
                  let frame = model.frame(in: tabs.view), !frame.isEmpty else { return false }
            return view.convert(view.bounds, to: tabs.view) == frame
        }
        tabs.sidebar.isHidden = false
        let route = try XCTUnwrap(tabs.selectedViewController?.stackRoutes.first as? RouteController)
        until("the route clears the sidebar") { route.view.safeAreaInsets.left > 100 }
        let text = try node(session, "text-home")
        let start = try node(session, "bar-start-home"), end = try node(session, "bar-end-home")
        let sidebar = route.view.safeAreaInsets.left
        until("the start reads the sidebar width") { abs(start.bounds.width - sidebar) < 1 && end.bounds.width == 0 }
        let farColumn = UIView(frame: CGRect(x: tabs.view.bounds.midX, y: 0, width: 100, height: 100))
        route.view.addSubview(farColumn)
        XCTAssertEqual(tabs.barInsets(beside: farColumn), .zero)
        farColumn.removeFromSuperview()
        until("its content is laid out beside the sidebar") { text.convert(text.bounds, to: nil).minX >= sidebar }
        XCTAssertTrue(try node(session, "tabs").isHidden, "the sidebar takes the authored tablist's place")
        let footer = try node(session, "sidebar-footer"), account = try node(session, "account")
        let accessory = try XCTUnwrap(tabs.sidebar.bottomBarView, "the native sidebar holds the authored footer")
        XCTAssertTrue(footer === accessory || footer.isDescendant(of: accessory))
        let home = try XCTUnwrap(session.presenter.authoredParent(of: footer))
        until("UIKit gives the sidebar footer room for its button") { accessory.bounds.width > 0 && account.bounds.height > 0 }
        let center = account.convert(CGPoint(x: account.bounds.midX, y: account.bounds.midY), to: window)
        let hit = try XCTUnwrap(window?.hitTest(center, with: nil))
        XCTAssertTrue(hit === account || hit.isDescendant(of: account), "Account at \(center), footer \(accessory.bounds), button \(account.bounds), hit \(type(of: hit))")
        tabs.sidebar.isHidden = true
        until("both sides read zero with the sidebar hidden") { start.bounds.width == 0 && end.bounds.width == 0 }
        until("hidden, the content takes the width back") { text.convert(text.bounds, to: nil).minX < sidebar }
        tabs.sidebar.isHidden = false
        try tapNode(session, "account")
        until("Account selects Saved from the sidebar") { self.topRoute(session) == "route-saved" }
        tapTab(tabs, 0)
        until("Home shown") { self.topRoute(session) == "route-home" }
        try tapNode(session, "change-bar")
        until("the app-drawn bar returns the footer to its parent") { tabs.sidebar.bottomBarView == nil && footer.superview === home }
        try tapNode(session, "account")
        until("Account selects Saved from the page") { self.topRoute(session) == "route-saved" }
    }

    func testAnIndependentSafeAreaCanBeWiderThanTheIPadSidebar() throws {
        guard #available(iOS 18.0, *), UIDevice.current.userInterfaceIdiom == .pad else { throw XCTSkip("the iPad tab sidebar needs iOS 18") }
        turned = true
        turn(.landscapeRight)
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        tabs.sidebar.isHidden = false
        let route = try XCTUnwrap(tabs.selectedViewController?.stackRoutes.first?.view)
        let start = try node(session, "bar-start-home")
        until("UIKit reserves the sidebar") { route.safeAreaInsets.left > 100 }
        let sidebar = route.safeAreaInsets.left
        tabs.additionalSafeAreaInsets.left = sidebar + 20
        until("the sidebar keeps its width when the tab controller’s safe area is wider") {
            route.safeAreaInsets.left > sidebar * 2 && abs(start.bounds.width - sidebar) < 1
        }
    }

    func testAClippedHorizontalTabBarHasNoInlineInset() throws {
        guard #available(iOS 27.1, *), UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the Duo’s rail needs iOS 27.1") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController), bar = tabs.tabBar
        let route = try XCTUnwrap(tabs.selectedViewController?.stackRoutes.first?.view)
        let parent = try XCTUnwrap(bar.superview), root = try XCTUnwrap(tabs.viewIfLoaded)
        let bounds = bar.bounds, center = bar.center
        defer { bar.bounds = bounds; bar.center = center }
        bar.bounds.size = CGSize(width: 360, height: 49)
        for x in [root.bounds.minX - 340, root.bounds.maxX - 20] {
            let frame = CGRect(x: x, y: root.bounds.maxY - 49, width: 360, height: 49)
            bar.center = root.convert(CGPoint(x: frame.midX, y: frame.midY), to: parent)
            XCTAssertEqual(bar.convert(bar.bounds, to: root).intersection(root.bounds).width, 20, accuracy: 0.5)
            XCTAssertEqual(tabs.barInsets(beside: route), .zero, "horizontal tab bar at \(frame)")
        }

        guard bar.traitCollection.placesVerticalBar else { return }
        let right = root.convert(center, from: parent).x > root.bounds.midX
        let x = right ? root.bounds.maxX - 20 : root.bounds.minX - bounds.width + 20
        let rail = CGRect(x: x, y: root.bounds.minY, width: bounds.width, height: 200)
        bar.bounds.size = rail.size
        bar.center = root.convert(CGPoint(x: rail.midX, y: rail.midY), to: parent)
        XCTAssertEqual(tabs.barInsets(beside: route), UIEdgeInsets(top: 0, left: right ? 0 : 20, bottom: 0, right: right ? 20 : 0))
        let below = UIView(frame: route.convert(CGRect(x: root.bounds.minX, y: rail.maxY, width: root.bounds.width, height: 100), from: root))
        route.addSubview(below)
        XCTAssertEqual(tabs.barInsets(beside: below), .zero)

        let edge = right ? tabs.contentLayoutGuide.layoutFrame.maxX : tabs.contentLayoutGuide.layoutFrame.minX
        let middle = CGPoint(x: edge + (right ? 1 : -1) * (bounds.width / 2 - 12), y: root.bounds.midY)
        bar.center = root.convert(middle, to: parent)
        let insets = tabs.barInsets(beside: route)
        XCTAssertEqual(right ? insets.right : insets.left, bounds.width, accuracy: 0.5)

        let clip = UIView(frame: route.convert(CGRect(x: root.bounds.midX - 100, y: 0, width: 80, height: root.bounds.height), from: root))
        clip.clipsToBounds = true
        route.addSubview(clip)
        let pane = UIView(frame: clip.convert(route.bounds, from: route))
        clip.addSubview(pane)
        XCTAssertEqual(tabs.barInsets(beside: pane), .zero)
    }

    func testTheDuoRailReportsItsSideOnAPushedRoute() throws {
        guard #available(iOS 27.1, *), UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the Duo’s rail needs iOS 27.1") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        guard tabs.tabBar.traitCollection.placesVerticalBar else { throw XCTSkip("this phone has no vertical rail") }
        try tapNode(session, "open-detail")
        until("the detail route is visible") { self.topRoute(session) == "route-detail" }
        let start = try node(session, "bar-start-detail"), end = try node(session, "bar-end-detail")
        let rail = tabs.tabBar.convert(tabs.tabBar.bounds, to: tabs.view)
        XCTAssertNotNil(tabs.tabBar.window)
        XCTAssertGreaterThan(rail.height, rail.width)
        let right = rail.midX > tabs.view.bounds.midX
        until("the route reads the visible rail’s width on its side") {
            abs(start.bounds.width - (right ? 0 : rail.width)) < 1 && abs(end.bounds.width - (right ? rail.width : 0)) < 1
        }

        let split = UIArrangementViewController()
        split.updateArrangement(UISplitArrangement().axes(.horizontal))
        XCTAssertTrue(session.presenter.arrangements.set(try node(session, "arranged-detail").id, controller: split))
        let first = (try node(session, "first-pane-start"), try node(session, "first-pane-end"))
        let second = (try node(session, "second-pane-start"), try node(session, "second-pane-end"))
        let primary = try XCTUnwrap(split.viewController(for: .primary)?.view)
        let secondary = try XCTUnwrap(split.viewController(for: .secondary)?.view)
        // UIKit collapses to one pane on the cover display.
        until("the visible pane at the end reads the rail and a pane beside it reads zero") {
            self.window?.layoutIfNeeded()
            let visible = [(primary, first), (secondary, second)].filter { pane, _ in
                pane.window != nil && !pane.bounds.isEmpty && sequence(first: pane, next: { $0.superview }).allSatisfy { !$0.isHidden }
            }.sorted { $0.0.convert($0.0.bounds, to: tabs.view).midX < $1.0.convert($1.0.bounds, to: tabs.view).midX }
            guard !visible.isEmpty else { return false }
            return visible.enumerated().allSatisfy { index, entry in
                let (start, end) = entry.1
                return abs(start.bounds.width - (!right && index == 0 ? rail.width : 0)) < 1
                    && abs(end.bounds.width - (right && index == visible.count - 1 ? rail.width : 0)) < 1
            }
        }
    }

    func testThePhonesTabBarHidesTheSidebarFooterAndReturnsItsControls() throws {
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the phone's bottom tab bar") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let footer = try node(session, "sidebar-footer"), account = try node(session, "account")
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("the platform bar takes the footer out of layout") { navigation.covers[footer.id] == .whole }
        XCTAssertTrue(footer.isHidden)
        XCTAssertNotNil(agent.tap(["id": Int(account.id)])["error"])
        let change = try node(session, "change-bar")
        XCTAssertNil(agent.tap(["id": Int(change.id)])["error"])
        until("the app-drawn bar gives the footer back") { navigation.covers[footer.id] == nil && !footer.isHidden }
        XCTAssertNil(agent.tap(["id": Int(account.id)])["error"])
        until("Account selects Saved") { self.topRoute(session) == "route-saved" }
    }

    /// The search tab's face and form follow its author: with its symbol,
    /// label and field gone it is UIKit's own again, the magnifying glass and
    /// "Search", and a landing page that activates nothing.
    func testASearchTabLosingItsFaceAndFieldIsUIKitsAgain() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let search = try XCTUnwrap(tabs.tabs.last as? UISearchTab)
        until("the authored label") { search.title == "Speakers" }
        if #available(iOS 26.0, *) { until("its field activates") { search.automaticallyActivatesSearch } }
        _ = Agent(session: session).tap(["id": Int(try node(session, "strip").id)])
        let platform = UISearchTab(viewControllerProvider: nil)
        until("UIKit's label again") { search.title == platform.title }
        XCTAssertEqual(search.image, platform.image, "UIKit's symbol again")
        if #available(iOS 26.0, *) { until("no field, no activation") { !search.automaticallyActivatesSearch } }
    }

    func testTouchingEachTabsLayoutCenterSelectsIt() throws {
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "UIKit selects the tab at the reported center")
        }
        let session = try fixture("EXACT_MORE_TABS_PLAN"), agent = Agent(session: session)
        let window = try XCTUnwrap(window), scene = try XCTUnwrap(window.windowScene)
        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        let controller = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let stacks = try XCTUnwrap(controller.viewControllers), items = try XCTUnwrap(controller.tabBar.items)
        let names = ["home", "barless", "three", "four", "five", "six"]
        guard stacks.count == names.count else { return XCTFail("expected a controller for each tab") }
        XCTAssertGreaterThan(items.count, 1)
        let tabs = try names.map { try node(session, "tab-" + $0) }
        // Start with an unselected tab so a missed touch cannot pass.
        for index in Array(names.indices.dropFirst()) + [0] {
            let tab = tabs[index]
            let detail = try XCTUnwrap(agent.layout(["id": Int(tab.id)])["node"] as? [String: Any])
            let hidden = try XCTUnwrap((detail["visible"] as? [String: Any])?["hidden"] as? Bool)
            let space = try XCTUnwrap(detail["space"] as? [String: Any])
            let box = try XCTUnwrap(space["viewport"] as? [String: Double])
            let width = try XCTUnwrap(box["w"]), height = try XCTUnwrap(box["h"])
            if !items.contains(stacks[index].tabBarItem) {
                XCTAssertTrue(hidden, names[index])
                XCTAssertEqual(width, 0)
                XCTAssertEqual(height, 0)
                continue
            }
            XCTAssertFalse(hidden, names[index])
            XCTAssertGreaterThanOrEqual(width, 20)
            XCTAssertGreaterThanOrEqual(height, 20)
            let center = CGPoint(x: try XCTUnwrap(box["x"]) + width / 2, y: try XCTUnwrap(box["y"]) + height / 2)
            let point = window.convert(agent.windowPoint(center), to: scene.coordinateSpace)
            let aim = agent.aim(["id": Int(tab.id), "aim": ["press": true]])
            let named = try XCTUnwrap(aim["aim"] as? [String: Any], "\(aim)")
            XCTAssertEqual(named["point"] as? [Double], [Agent.r2(point.x), Agent.r2(point.y)])
            var request = try TouchRunner.aim(point, in: window)
            request["op"] = "tap"
            XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
            until("only the touched tab is selected") {
                tabs.allSatisfy { ($0.props["accessibilitySelected"] == "true") == ($0 === tab) }
            }
        }
    }

    /// A tap the agent delivers asks the app's delegate as UIKit asks it on
    /// a bar built from `UITab`s, by tab, and a reselect tells it as a tap
    /// does; a tab the app declines is not selected.
    func testTheAgentsTapAsksTheAppsDelegateByTab() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let navigation = session.presenter.navigationHost
        let tabs = try XCTUnwrap(navigation.tabController)
        let app = RecordingTabDelegate()
        navigation.setAppDelegate(tabs, app)
        XCTAssertNil(navigation.activateChrome(try node(session, "tab-saved"))?["error"])
        XCTAssertEqual(app.calls, ["shouldSelectTab panel-saved", "didSelectTab panel-saved"])
        app.calls = []
        XCTAssertNil(navigation.activateChrome(try node(session, "tab-saved"))?["error"])
        XCTAssertEqual(app.calls, ["shouldSelectTab panel-saved", "didSelectTab panel-saved"], "a reselect is told too")
        app.calls = []
        app.declines = "panel-home"
        let refused = navigation.activateChrome(try node(session, "tab-home"))?["error"] as? String
        XCTAssertTrue(refused?.contains("declined") == true, "\(String(describing: refused))")
        XCTAssertEqual(app.calls, ["shouldSelectTab panel-home"])
        XCTAssertEqual(tabs.selectedTab?.identifier, "panel-saved")
    }

    /// A phone's tab bar with no search tab stays on `viewControllers`.
    func testAPhoneTabBarWithNoSearchTabKeepsItsControllers() throws {
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("an iPad builds UITabs for its sidebar") }
        let session = try fixture("EXACT_TAB_EDGE_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        XCTAssertNotNil(tabs.viewControllers)
        XCTAssertTrue(session.presenter.navigationHost.builtTabs.isEmpty)
    }

    /// The tabs of a container built from `UITab`s are Exact's (LLP 1075.003
    /// §3.7): the development check journals a change made outside Exact
    /// once, by the next batch.
    func testTabsChangedOutsideExactAreJournaledOnce() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        let session = try fixture("EXACT_SEARCH_TABS_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let line = "tabs: tabs changed outside Exact, which owns it"
        try journaledOnce(session, line, tabs) {
            tabs.tabs = tabs.tabs + [UITab(title: "Extra", image: nil, identifier: "extra") { _ in UIViewController() }]
        }
    }

    /// The same for a container built from controllers: its
    /// `viewControllers` are Exact's.
    func testATabControllerAddedOutsideExactIsJournaledOnce() throws {
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("an iPad builds UITabs for its sidebar") }
        let session = try fixture("EXACT_TAB_EDGE_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        XCTAssertTrue(session.presenter.navigationHost.builtTabs.isEmpty)
        let line = "tabs: viewControllers changed outside Exact, which owns it"
        try journaledOnce(session, line, tabs) { tabs.viewControllers = (tabs.viewControllers ?? []) + [UIViewController()] }
    }

    /// A container's controllers are compared as members: a reorder, as
    /// More's Edit makes, is no change, and a controller put in place of one
    /// of Exact's is.
    func testTabControllersAreComparedAsMembers() throws {
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("an iPad builds UITabs for its sidebar") }
        let session = try fixture("EXACT_TAB_EDGE_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let line = "tabs: viewControllers changed outside Exact, which owns it"
        let count = { session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: line).count - 1 }
        var shown = try XCTUnwrap(tabs.viewControllers)
        tabs.viewControllers = shown.reversed()
        tapTab(tabs, of: try XCTUnwrap(session.presenter.navigationHost.tabStack(1)))
        until("a selection after the reorder") { !Agent(session: session).nativeInFlight() && session.presenter.navigationHost.routerTab == 1 }
        XCTAssertEqual(count(), 0, "a reorder is no change")
        shown = try XCTUnwrap(tabs.viewControllers)
        let kept = try XCTUnwrap(tabs.selectedViewController)
        let index = try XCTUnwrap(shown.firstIndex { $0 !== kept })
        shown[index] = UIViewController()
        tabs.viewControllers = shown
        tapTab(tabs, of: kept)
        until("journaled by the next batch") { count() == 1 }
    }

    /// A container built from controllers whose `viewControllers` the app
    /// clears holds none of Exact's: the check names it at the next batch.
    func testTabControllersClearedOutsideExactAreJournaled() throws {
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("an iPad builds UITabs for its sidebar") }
        let session = try fixture("EXACT_TAB_EDGE_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let line = "tabs: viewControllers changed outside Exact, which owns it"
        let count = { session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: line).count - 1 }
        XCTAssertEqual(count(), 0)
        tabs.viewControllers = nil
        XCTAssertEqual(tabs.viewControllers?.count, 0, "UIKit reads no controllers back, not nil")
        session.presenter.apply(wireBatch([]))
        XCTAssertEqual(count(), 1, "journaled at the next batch")
    }

    /// A container built from controllers on which the app sets `tabs`
    /// holds `UITab`s, and its `viewControllers` reads nil: the check names
    /// `tabs` at the next batch.
    func testTabsSetOnAContainerBuiltFromControllersAreJournaled() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UITab is iOS 18") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("an iPad builds UITabs for its sidebar") }
        let session = try fixture("EXACT_TAB_EDGE_PLAN")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        XCTAssertTrue(session.presenter.navigationHost.builtTabs.isEmpty)
        let logs = { session.agent(#"{"op":"logs","since":0}"#) }
        let line = "tabs: tabs changed outside Exact, which owns it"
        XCTAssertFalse(logs().contains(line))
        // The app's own tabs, which hand back Exact's controllers
        let controllers = try XCTUnwrap(tabs.viewControllers)
        tabs.tabs = controllers.enumerated().map { index, controller in UITab(title: "Tab \(index)", image: nil, identifier: "app-\(index)") { _ in controller } }
        XCTAssertNil(tabs.viewControllers, "a container that holds UITabs reads no controllers")
        session.presenter.apply(wireBatch([]))
        XCTAssertEqual(logs().components(separatedBy: line).count - 1, 1, "journaled at the next batch")
        XCTAssertFalse(logs().contains("tabs: viewControllers changed outside Exact"), "named by the property the app set")
    }

    /// `line` is journaled by no selection before `change`, once by the
    /// selection after it, and not again at the next.
    private func journaledOnce(_ session: ExactSession, _ line: String, _ tabs: UITabBarController, change: () -> Void) throws {
        let count = { session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: line).count - 1 }
        tapTab(tabs, 1)
        until("the second tab selected") { session.presenter.navigationHost.routerTab == 1 && !Agent(session: session).nativeInFlight() }
        XCTAssertEqual(count(), 0, "a selection changes nothing Exact owns")
        change()
        tapTab(tabs, 0)
        until("journaled at the next batch") { count() == 1 }
        tapTab(tabs, 1)
        until("the second tab selected again") { session.presenter.navigationHost.routerTab == 1 && !Agent(session: session).nativeInFlight() }
        XCTAssertEqual(count(), 1, "once")
    }

    /// LLP 1115.001 D11: the root's tabs are the first tablist in tree order
    /// whose tabs name a panel, as the kernel and every other host take them,
    /// though one made later than another has the higher view id.
    func testTheRootsTabsAreTheFirstTablistInTreeOrder() throws {
        let session = try fixture("EXACT_ROOT_TABS_ORDER_PLAN")
        let navigation = session.presenter.navigationHost
        let panels = { (names: [String]) in names.compactMap { name in session.presenter.views.values.first { $0.props["id"] == "panel-\(name)" }?.id } }
        XCTAssertEqual(navigation.tabPanels, panels(["other", "home"]), "the one tablist's order")
        try tapNode(session, "show-home")
        until("the tablist added ahead is the root's") {
            navigation.tabPanels == panels(["home", "other"]) && !Agent(session: session).nativeInFlight()
        }
    }

    /// LLP 1083.000 D4: a column the author placed beside routes offset past it, below the
    /// container's top edge, paints over the native container, whose box is the page's, takes
    /// the touches there, as the web shows it, and grows the container's leading safe area up
    /// to where the routes begin. A box a route's box meets keeps CSS's order, under the
    /// container that stands for the routes.
    func testABoxBesideTheRoutesPaintsOverTheNativeContainer() throws {
        let session = try fixture("EXACT_BESIDE_ROUTES_PLAN")
        let navigation = session.presenter.navigationHost
        let holder = try XCTUnwrap(navigation.primaryOwner?.view)
        let sidebar = try node(session, "sidebar"), over = try node(session, "over")
        until("the column beside the routes is over the container") { sidebar.layer.zPosition > holder.layer.zPosition }
        XCTAssertLessThanOrEqual(over.layer.zPosition, holder.layer.zPosition, "a box the route's box meets stays under it")
        let window = try XCTUnwrap(self.window)
        let point = sidebar.convert(CGPoint(x: sidebar.bounds.midX, y: sidebar.bounds.maxY - 40), to: window)
        let hit = try XCTUnwrap(window.hitTest(point, with: nil))
        XCTAssertTrue(hit === sidebar || hit.isDescendant(of: sidebar), "the column takes the touch, not \(hit)")
        let route = try node(session, "route")
        XCTAssertGreaterThanOrEqual(route.convert(route.bounds, to: window).minX, 289, "the route stays where it was laid out")
        // The bar's contents start where the route begins, past the column, and the route is not inset again.
        let owner = try XCTUnwrap(navigation.primaryOwner)
        let begins = route.convert(route.bounds, to: holder).minX
        XCTAssertEqual(owner.additionalSafeAreaInsets.left, begins, accuracy: 0.5)
        let clearance = ContributionsIOS.existing(on: owner)?.entries.filter { $0.identity.operation == .containerClearance }
        XCTAssertEqual(clearance?.map(\.insets.left), [begins], "the container's ledger holds Exact's part")
        let nav = try XCTUnwrap(navigation.primaryNavigation)
        until("the bar's layout clears the column") { nav.navigationBar.safeAreaInsets.left >= begins - 0.5 }
        if case .edges(let edges)? = navigation.covers[route.id] { XCTAssertEqual(edges.left, 0, "no left cover on the route") }
        // What an app's hatch adds to the container's safe area stays beside Exact's.
        owner.additionalSafeAreaInsets.bottom = 50
        navigation.reportCovers()
        XCTAssertEqual(owner.additionalSafeAreaInsets.bottom, 50)
        XCTAssertEqual(owner.additionalSafeAreaInsets.left, begins, accuracy: 0.5)
        // Two boxes beside the routes keep their z-index order above the container.
        let low = try node(session, "low"), lower = try node(session, "lower")
        XCTAssertGreaterThan(lower.layer.zPosition, holder.layer.zPosition)
        XCTAssertGreaterThan(low.layer.zPosition, lower.layer.zPosition, "z-index -1 over -2")
        XCTAssertGreaterThan(sidebar.layer.zPosition, low.layer.zPosition, "z-index auto over -1")
        // A bar below routes that end above it, with no native bar there: it paints over the container and takes its touches
        let player = try node(session, "player")
        until("the bar below the routes is over the container") { player.layer.zPosition > holder.layer.zPosition }
        let below = player.convert(CGPoint(x: player.bounds.midX, y: player.bounds.midY), to: window)
        let under = try XCTUnwrap(window.hitTest(below, with: nil))
        XCTAssertTrue(under === player || under.isDescendant(of: player), "the bar takes the touch, not \(under)")
        // A box with no area beside the routes is decided by the badge it holds, which shows outside it
        let pin = try node(session, "pin")
        XCTAssertTrue(pin.bounds.isEmpty, "the box has no area")
        until("the box with no area is over the container") { pin.layer.zPosition > holder.layer.zPosition }
    }
}

/// An app's tab bar delegate that records what UIKit, or the agent, asks it.
@available(iOS 18.0, *)
private final class RecordingTabDelegate: NSObject, UITabBarControllerDelegate {
    var calls: [String] = []
    var declines: String?
    func tabBarController(_ tabs: UITabBarController, shouldSelectTab tab: UITab) -> Bool {
        calls.append("shouldSelectTab \(tab.identifier)")
        return tab.identifier != declines
    }
    func tabBarController(_ tabs: UITabBarController, didSelectTab tab: UITab, previousTab: UITab?) {
        calls.append("didSelectTab \(tab.identifier)")
    }
    func tabBarController(_ tabs: UITabBarController, shouldSelect controller: UIViewController) -> Bool {
        calls.append("shouldSelect controller")
        return true
    }
}

final class FloatingTabIOSTests: XCTestCase {
    func testFloatingTabsWithTheSameTitleHaveTheirOwnCells() throws {
        guard #available(iOS 18.0, *), UIDevice.current.userInterfaceIdiom == .pad else { throw XCTSkip("iPad tab list") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "UIKit lays out its tab cells in a regular-width window")
        }
        let window = UIWindow(windowScene: scene)
        defer { window.isHidden = true }
        let session = ExactApp.shared.makeSession(label: "tab cells")
        defer { session.destroy() }
        let agent = Agent(session: session)
        let animations = UIView.areAnimationsEnabled
        UIView.setAnimationsEnabled(false)
        defer { UIView.setAnimationsEnabled(animations) }
        for count in [3, 6] {
            let controller = UITabBarController()
            let pages = (0..<count).map { _ -> UIViewController in
                let page = UIViewController()
                page.tabBarItem = UITabBarItem(title: "Shared", image: nil, tag: 0)
                return page
            }
            if count == 3 {
                controller.mode = .tabSidebar
                controller.tabs = pages.enumerated().map { index, page in
                    UITab(title: "Shared", image: nil, identifier: "panel-\(index)") { _ in page }
                }
            } else {
                controller.viewControllers = pages
                XCTAssertTrue(controller.tabs.isEmpty)
            }
            window.rootViewController = controller
            window.makeKeyAndVisible()
            for sidebar in count == 3 ? [false, true] : [false] {
                controller.sidebar.isHidden = !sidebar
                window.layoutIfNeeded()
                waitFor("UIKit presents all \(count) tabs in the requested bar") {
                    guard let list = controller.shownTabBar as? UICollectionView else { return false }
                    return list.visibleCells.count == count && (list.bounds.height > list.bounds.width) == sidebar
                }
                let list = try XCTUnwrap(controller.shownTabBar as? UICollectionView)
                let sample = try XCTUnwrap(list.visibleCells.first)
                let frame = sample.convert(sample.bounds, to: controller.view)
                let layout = UICollectionViewFlowLayout()
                layout.itemSize = frame.size
                let other = UICollectionView(frame: frame, collectionViewLayout: layout)
                other.isUserInteractionEnabled = false
                other.register(UICollectionViewCell.self, forCellWithReuseIdentifier: "unrelated")
                let data = UICollectionViewDiffableDataSource<Int, Int>(collectionView: other) { list, path, _ in
                    list.dequeueReusableCell(withReuseIdentifier: "unrelated", for: path)
                }
                var snapshot = NSDiffableDataSourceSnapshot<Int, Int>()
                snapshot.appendSections([0])
                snapshot.appendItems([0])
                data.apply(snapshot, animatingDifferences: false)
                controller.view.addSubview(other)
                other.layoutIfNeeded()
                XCTAssertEqual(other.visibleCells.first.map { $0.convert($0.bounds, to: controller.view) }, frame)
                defer { other.removeFromSuperview(); withExtendedLifetime(data) {} }
                var visited = Set<ObjectIdentifier>()
                for (index, page) in pages.enumerated() {
                    // Hosted XCTest loads accessibility. Make these cells as unidentifiable by
                    // accessibility as the cold simulator's, even with the client attached.
                    var pending: [UIView] = list.visibleCells
                    while let view = pending.popLast() {
                        view.isAccessibilityElement = false
                        view.accessibilityIdentifier = nil
                        view.accessibilityLabel = ""
                        pending += view.subviews
                    }
                    let shown = try XCTUnwrap(controller.shownTabBar)
                    XCTAssertTrue(shown === list)
                    guard case .success(let view) = agent.presentedTabItem(page.tabBarItem, in: shown, controller: controller) else {
                        return XCTFail("no cell for tab \(index)")
                    }
                    XCTAssertTrue(list.visibleCells.contains { $0 === view })
                    if count == 6 {
                        controller.selectedIndex = index
                        window.layoutIfNeeded()
                    }
                    let model = count == 3 ? controller.tabs[index] : controller.selectedTab
                    XCTAssertEqual(view.convert(view.bounds, to: list), model?.frame(in: list))
                    XCTAssertTrue(visited.insert(ObjectIdentifier(view)).inserted)
                }
            }
        }
    }
}
#endif
