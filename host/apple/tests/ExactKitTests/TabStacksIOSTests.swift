#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

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
        let navigation = session.presenter.navigation
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
        let navigation = session.presenter.navigation
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

    /// A warm deep link into a tab built earlier: one update selects Barless
    /// and pushes Detail onto it. Detail shows, over Barless's root.
    func testATabSelectedWithAPushOntoItShowsThePush() throws {
        let session = try fixture("stacks-select-push")
        let navigation = session.presenter.navigation
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
        let navigation = session.presenter.navigation
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
        let navigation = session.presenter.navigation
        let (_, nav, root) = try barless(session)
        try press(session, "open-two")
        waitFor("Barless holds the whole stack") { nav.viewControllers.count == 3 && nav.transitionCoordinator == nil && !navigation.inTransition }
        XCTAssertTrue(nav.viewControllers.first === root)
        XCTAssertEqual((nav.viewControllers.last as? RouteController)?.node.props["testId"], "route-more")
        XCTAssertEqual(navigation.observation()["route"] as? String, (nav.viewControllers.last as? RouteController)?.key, "no Back was read")
    }

    private func fixture(_ label: String) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_BARLESS_TAB_PLAN"], "build.mjs --test --ios compiles the barless-tab fixture")))
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
        waitFor("the tabs are installed") { session.presenter.navigation.tabOwner != nil }
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
        let tabs = try XCTUnwrap(session.presenter.navigation.tabController)
        let nav = try XCTUnwrap(tabs.tabControllers?.first as? UINavigationController, "Barless's panel draws its stack")
        let root = try XCTUnwrap(nav.viewControllers.first as? RouteController)
        return (tabs, nav, root)
    }
}
#endif
