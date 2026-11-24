#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1075.003 §3.7 (PlainTabIOS.swift): every tab's controller is made
/// with the tab container and never replaced. A tab whose panel draws its
/// stack is a navigation controller from launch, built or not; where its
/// root shows no bar the bar is hidden, and its first push animates in the
/// same controller. The plain tab, whose panel shows one fixed route with
/// no header, is NavigationMoreIOSTests's Six.
final class TabStacksIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []
    private var animationsWereEnabled = true

    func testEmptyProjectedPanelsExposeNoContainer() throws {
        let session = try fixture("panel-accessibility"), p = session.presenter
        let panels = p.navigationHost.tabPanels.compactMap { p.views[$0] }
        XCTAssertEqual(panels.count, 2)
        XCTAssertEqual(panels.filter { !$0.accessibilityElementsHidden }.count, 0)
        let route = try XCTUnwrap(p.navigationHost.activeRoute?.node)
        XCTAssertFalse(route.accessibilityElementsHidden)
        XCTAssertTrue(route.isDescendant(of: try XCTUnwrap(p.navigationHost.primaryOwner?.view)))
        func spokenLabels() -> [String] {
            let elements = p.axElements(roots: [p.documentHost])["elements"] as? [[String: Any]] ?? []
            return elements.compactMap { $0["name"] as? String }.filter { !$0.isEmpty }
        }
        let spoken = spokenLabels()
        XCTAssertFalse(spoken.isEmpty)
        let panel = try XCTUnwrap(panels.first)
        let extra = NodeView(id: 9000, kind: "button", presenter: p)
        extra.applyProps(set: ["accessibilityLabel": "Extra panel content"], clear: [])
        panel.container.addSubview(extra)
        XCTAssertFalse(panel.accessibilityElementsHidden, "content that remains in the panel is still reachable")
        extra.removeFromSuperview()
        XCTAssertTrue(panel.accessibilityElementsHidden)
        let virtual = ["First virtual element", "Second virtual element"].map { label in
            let element = UIAccessibilityElement(accessibilityContainer: panel)
            element.accessibilityLabel = label
            element.accessibilityFrameInContainerSpace = CGRect(x: 0, y: 0, width: 100, height: 40)
            return element
        }
        panel.accessibilityElements = virtual
        XCTAssertTrue(panel.container.subviews.isEmpty)
        XCTAssertFalse(panel.accessibilityElementsHidden, "virtual content needs no subviews")
        XCTAssertEqual(spokenLabels().filter { $0.contains("virtual element") }, virtual.compactMap(\.accessibilityLabel))
        panel.accessibilityElements = []
        XCTAssertTrue(panel.accessibilityElementsHidden, "an explicitly empty list exposes no shell")
        panel.accessibilityElements = nil
        let ids = p.navigationHost.tabPanels
        defer { p.navigationHost.tabPanels = ids }
        p.navigationHost.tabPanels = []
        XCTAssertFalse(panel.accessibilityElementsHidden, "an ordinary tabpanel keeps its accessibility")
        XCTAssertEqual(spokenLabels(), spoken, "projecting away empty shells preserves spoken content and its order")
    }

    func testAHiddenNativeTabBarExposesNoElementsAndShowsAgain() throws {
        let session = try fixture("tab-bar-accessibility"), p = session.presenter
        let list = try XCTUnwrap(p.navigationHost.adoptedTablist.flatMap { p.views[$0] })
        let tabs = try XCTUnwrap(p.navigationHost.tabController)
        XCTAssertFalse(tabs.tabBar.accessibilityElementsHidden)
        let style = list.style
        p.apply(wireBatch([["op": "style", "id": Int(list.id), "style": ["appearance": "none", "written": ["appearance"]]]]))
        XCTAssertFalse(tabs.tabBar.accessibilityElementsHidden, "suppression leaves the app's bar setting alone")
        XCTAssertTrue(sequence(first: tabs.tabBar as UIView, next: \.superview)
            .dropFirst().prefix(while: { $0 !== tabs.view }).contains { $0.accessibilityElementsHidden })
        list.applyStyle(style)
        p.navigationHost.sync(wireBatch([]))
        XCTAssertFalse(tabs.tabBar.accessibilityElementsHidden, "the native tabs are reachable when shown again")
    }

    func testTabBarSuppressionPreservesAppAccessibilitySettings() throws {
        let session = try fixture("tab-bar-app-accessibility"), p = session.presenter
        let tabs = try XCTUnwrap(p.navigationHost.tabController as? TabController)
        let bar = tabs.tabBar
        bar.accessibilityElementsHidden = true
        tabs.view.setNeedsLayout()
        tabs.view.layoutIfNeeded()
        XCTAssertTrue(bar.accessibilityElementsHidden, "layout preserves the app's visible-bar setting")
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden requires iOS 18") }
        for appHidden in [true, false] {
            bar.accessibilityElementsHidden = appHidden
            tabs.setTabBarHidden(true, animated: false)
            tabs.syncBarAccessibility()
            tabs.view.layoutIfNeeded()
            tabs.syncBarAccessibility()
            XCTAssertEqual(bar.accessibilityElementsHidden, appHidden)
            tabs.setTabBarHidden(false, animated: false)
            tabs.syncBarAccessibility()
            tabs.view.layoutIfNeeded()
            XCTAssertEqual(bar.accessibilityElementsHidden, appHidden, "showing restores the app's setting")
            bar.accessibilityElementsHidden = !appHidden
            tabs.syncBarAccessibility()
            XCTAssertEqual(bar.accessibilityElementsHidden, !appHidden, "visible bars stay app-owned")
        }
        for appHidden in [true, false] {
            bar.accessibilityElementsHidden = !appHidden
            tabs.setTabBarHidden(true, animated: false)
            tabs.syncBarAccessibility()
            bar.accessibilityElementsHidden = appHidden
            tabs.syncBarAccessibility()
            tabs.view.setNeedsLayout()
            tabs.view.layoutIfNeeded()
            XCTAssertEqual(bar.accessibilityElementsHidden, appHidden, "an app write while hidden stays stored")
            tabs.setTabBarHidden(false, animated: false)
            tabs.syncBarAccessibility()
            tabs.view.layoutIfNeeded()
            XCTAssertEqual(bar.accessibilityElementsHidden, appHidden, "showing preserves an app write made while hidden")
        }
    }

    func testAClippedProjectedPanelKeepsContentBesideItsContainer() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit derives a hatch label's name") }
        let session = try fixture("clipped-panel-accessibility"), p = session.presenter
        let panel = try XCTUnwrap(p.navigationHost.tabPanels.compactMap { p.views[$0] }.first)
        panel.applyStyle(["overflow_x": .string("clip")])
        XCTAssertFalse(panel.container === panel)
        XCTAssertTrue(panel.container.subviews.isEmpty)
        XCTAssertTrue(panel.accessibilityElementsHidden)
        let label = UILabel(frame: CGRect(x: 0, y: 0, width: 120, height: 40))
        label.text = "Hatch content"
        label.isAccessibilityElement = true
        panel.addSubview(label)
        XCTAssertTrue(panel.container.subviews.isEmpty)
        XCTAssertFalse(panel.accessibilityElementsHidden, "a hatch may put accessible content beside the clip")
        let elements = p.axElements(roots: [panel])["elements"] as? [[String: Any]] ?? []
        XCTAssertEqual(elements.compactMap { $0["name"] as? String }.filter { $0 == label.text }, ["Hatch content"])
        label.removeFromSuperview()
        XCTAssertTrue(panel.accessibilityElementsHidden, "removing the content leaves an empty shell again")
    }

    func testAnEmptyProjectedPanelStoresAuthoredAccessibilityHiding() {
        for property in ["inert", "accessibilityElementsHidden"] {
            for gainsContent in [true, false] {
                let p = Presenter(), panel = NodeView(id: 1, kind: "view", presenter: p)
                p.navigationHost.tabPanels = [panel.id]
                panel.applyProps(set: ["accessibilityRole": "tabpanel"], clear: [])
                XCTAssertTrue(panel.accessibilityElementsHidden)
                panel.applyProps(set: [property: "true"], clear: [])
                if gainsContent {
                    panel.addSubview(UILabel())
                } else {
                    p.navigationHost.tabPanels = []
                }
                XCTAssertTrue(panel.accessibilityElementsHidden, "\(property) survives the end of placeholder suppression")
                panel.applyProps(set: [:], clear: [property])
                XCTAssertFalse(panel.accessibilityElementsHidden, "clearing \(property) exposes the content")
            }
        }
    }

    override func setUp() {
        super.setUp()
        animationsWereEnabled = UIView.areAnimationsEnabled
        UIView.setAnimationsEnabled(false)
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        UIView.setAnimationsEnabled(animationsWereEnabled)
        super.tearDown()
    }

    func testATabThatDrawsAStackIsANavigationControllerFromLaunchAndKeepsIt() throws {
        let session = try fixture("stacks")
        let navigation = session.presenter.navigationHost
        let (tabs, nav, root) = try barless(session)
        XCTAssertTrue(nav.isNavigationBarHidden, "Barless's root shows no bar")
        XCTAssertTrue(navigation.primaryNavigation === nav)
        XCTAssertTrue(navigation.activeRoute === root)
        XCTAssertNotNil(root.view.window)
        XCTAssertGreaterThan(root.view.safeAreaInsets.bottom, 0, "the route's safe area clears the tab bar")
        XCTAssertEqual(navigation.observation()["stack"] as? [String], [root.key])
        XCTAssertTrue(navigation.tabStacks.values.allSatisfy { $0 is UINavigationController }, "both panels draw their stack")
        let titled = try XCTUnwrap(tabs.tabControllers?[1] as? UINavigationController, "a tab not built yet is its navigation controller")
        XCTAssertTrue(titled.viewControllers.isEmpty, "with no routes")
        try press(session, "tab-titled")
        waitFor("Titled is selected") { tabs.selectedIndex == 1 && titled.viewControllers.count == 1 }
        XCTAssertTrue(tabs.tabControllers?[1] === titled, "the tab keeps its controller")
        XCTAssertFalse(titled.isNavigationBarHidden, "its root's bar")
        XCTAssertTrue(navigation.primaryNavigation === titled)
        XCTAssertTrue(tabs.tabControllers?[0] === nav)
    }

    func testABarlessTabsFirstPushAnimatesInItsOwnController() throws {
        let session = try fixture("stacks-push")
        let navigation = session.presenter.navigationHost
        let (tabs, nav, root) = try barless(session)
        let frame = root.view.convert(root.view.bounds, to: nil)
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        try press(session, "open")
        XCTAssertEqual(nav.viewControllers.count, 2)
        XCTAssertNotNil(nav.transitionCoordinator, "the push animates")
        XCTAssertTrue(nav.viewControllers.first === root)
        XCTAssertTrue(tabs.tabControllers?.first === nav, "the tab keeps its controller")
        waitFor("the push is over") { nav.transitionCoordinator == nil && !navigation.changing }
        XCTAssertEqual(navigation.observation()["stack"] as? [String], nav.viewControllers.compactMap { ($0 as? RouteController)?.key })
        XCTAssertEqual(navigation.agentBack(), "", "the platform's Back goes")
        waitFor("popped to the root") { nav.viewControllers.count == 1 && nav.transitionCoordinator == nil && !navigation.changing }
        XCTAssertTrue(nav.isNavigationBarHidden)
        XCTAssertEqual(root.view.convert(root.view.bounds, to: nil), frame, "the root is where it was")
    }

    /// A refused write to an inactive tab keeps the agent waiting and is
    /// retried without selecting the tab or reporting an at-rest difference.
    func testAnInactiveTabsRefusedStackWriteIsRetried() throws {
        let session = try fixture("inactive-tab-refused-write"), presenter = session.presenter
        let navigation = presenter.navigationHost
        let (tabs, nav, previous) = try barless(session)
        try press(session, "tab-titled")
        waitFor("Titled is selected and the Barless stack is idle") {
            tabs.selectedIndex == 1 && !navigation.inTransition && nav.transitionCoordinator == nil
        }
        let panel = try XCTUnwrap(navigation.tabPanels.first)
        let delegate = nav.delegate
        nav.delegate = nil
        defer { nav.delegate = delegate }
        presenter.onCovers = nil
        let replacement: UInt32 = 900_001
        var refusing = true
        try withNavigationStackWrites(shouldRefuse: { stack, routes in
            stack === nav && refusing && (routes.last as? RouteController)?.node.id == replacement
        }) {
            presenter.apply(wireBatch([
                ["op": "create", "id": replacement, "kind": "column", "props": ["navigationKey": "replacement"]],
                ["op": "children", "id": panel, "ids": [replacement]]
            ]))
            XCTAssertTrue(navigation.stackWritesOwed)
            XCTAssertTrue(nav.topViewController === previous)
            XCTAssertFalse(navigation.inTransition, "the selected stack is idle")
            XCTAssertNil(navigation.restDifference, "the inactive tab still owes its write")
            let agent = session.agentInstance
            XCTAssertTrue(agent.hostsInTransition, "the inactive stack's retry is still in flight")
            refusing = false
            XCTAssertTrue(agent.awaitTransitions())
            XCTAssertEqual((nav.topViewController as? RouteController)?.node.id, replacement)
            XCTAssertNil(navigation.restDifference)
            XCTAssertEqual(tabs.selectedIndex, 1)
        }
    }

    /// A warm deep link into a tab built earlier: one update selects Barless
    /// and pushes Detail onto it. Detail shows, over Barless's root.
    func testATabSelectedWithAPushOntoItShowsThePush() throws {
        let session = try fixture("stacks-select-push")
        let navigation = session.presenter.navigationHost
        let (tabs, nav, root) = try barless(session)
        try press(session, "tab-titled")
        waitFor("Titled is selected") { tabs.selectedIndex == 1 && !navigation.inTransition }
        try press(session, "open-from-titled")
        waitFor("Barless shows Detail") { tabs.selectedIndex == 0 && nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !navigation.inTransition }
        let top = try XCTUnwrap(nav.topViewController as? RouteController)
        XCTAssertEqual(top.node.props["testId"], "route-detail")
        XCTAssertTrue(nav.viewControllers.first === root)
        XCTAssertTrue(tabs.selectedViewController === nav)
        XCTAssertTrue(navigation.primaryNavigation === nav)
        XCTAssertTrue(navigation.activeRoute === top)
        XCTAssertNotNil(top.view.window, "Detail is in the window")
        XCTAssertNil(root.view.window, "Barless's root is under it")
    }

    /// A bar the root comes to show shows in the tab's own controller.
    func testABarTheRootComesToShowShowsInItsOwnController() throws {
        let session = try fixture("stacks-title")
        let navigation = session.presenter.navigationHost
        let (tabs, nav, root) = try barless(session)
        try press(session, "title")
        waitFor("the bar shows") { !nav.isNavigationBarHidden }
        XCTAssertEqual(root.navigationItem.title, "barless")
        XCTAssertTrue(nav.viewControllers == [root])
        XCTAssertTrue(tabs.tabControllers?.first === nav)
        XCTAssertTrue(navigation.primaryNavigation === nav)
    }

    /// Two routes pushed in one batch, as a deep link does: one stack, and
    /// the router ends on its top.
    func testTwoRoutesPushedAtOnceEndOnTheTop() throws {
        let session = try fixture("stacks-deep")
        let navigation = session.presenter.navigationHost
        let (_, nav, root) = try barless(session)
        try press(session, "open-two")
        waitFor("Barless holds the whole stack") { nav.viewControllers.count == 3 && nav.transitionCoordinator == nil && !navigation.inTransition }
        XCTAssertTrue(nav.viewControllers.first === root)
        XCTAssertEqual((nav.viewControllers.last as? RouteController)?.node.props["testId"], "route-more")
        XCTAssertEqual(navigation.observation()["route"] as? String, (nav.viewControllers.last as? RouteController)?.key, "no Back was read")
    }

    /// A tabpanel inside its root's tablist: the tab bar takes the tabs'
    /// place, and the panel's routes stay in the page, shown and pressed.
    func testAPanelInsideTheTablistStaysInThePage() throws {
        let session = try fixture("nested-panel", plan: "EXACT_NESTED_PANEL_TABS_PLAN")
        let navigation = session.presenter.navigationHost
        XCTAssertNotNil(navigation.tabController, "the tab bar shows")
        XCTAssertEqual(navigation.covers[try node(session, "tab-home").id], .whole, "the tab is the bar's")
        XCTAssertNil(navigation.covers[try node(session, "tabs").id], "the tablist keeps the panel's room")
        try assertReached(session, "push")
        try press(session, "push")
        waitFor("Detail is pushed") { (navigation.activeRoute?.node.props["testId"]) == "route-detail" && !navigation.inTransition }
        try assertReached(session, "body")
    }

    /// The node is in the window with a size, and a touch at its center reaches it.
    private func assertReached(_ session: ExactSession, _ testId: String) throws {
        let view = try node(session, testId)
        let window = try XCTUnwrap(view.window, "\(testId) is in the window")
        window.layoutIfNeeded()
        let frame = view.convert(view.bounds, to: window)
        XCTAssertGreaterThan(frame.width * frame.height, 0, "\(testId) has a size: \(frame)")
        let hit = window.hitTest(CGPoint(x: frame.midX, y: frame.midY), with: nil)
        XCTAssertTrue(hit?.isDescendant(of: view) == true, "a touch at \(testId)'s center reaches it, not \(String(describing: hit))")
    }

    private func fixture(_ label: String, plan key: String = "EXACT_BARLESS_TAB_PLAN") throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env[key], "build.mjs --test --ios compiles the fixture")))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: frame)
        window.frame = frame
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: frame.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        waitFor("the tabs are installed") { session.presenter.navigationHost.tabOwner != nil }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    private func press(_ session: ExactSession, _ testId: String) throws {
        session.presenter.press(try node(session, testId).id)
    }

    /// The Barless tab's navigation controller and its root, as installed.
    private func barless(_ session: ExactSession) throws -> (tabs: UITabBarController, nav: UINavigationController, root: RouteController) {
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let nav = try XCTUnwrap(tabs.tabControllers?.first as? UINavigationController, "Barless's panel draws its stack")
        let root = try XCTUnwrap(nav.viewControllers.first as? RouteController)
        return (tabs, nav, root)
    }
}
#endif
