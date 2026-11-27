#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// Tests for UIKit navigation-bar Back projection (`UINavigationItem.backAction`) and
/// agent Back button aiming across inline-title shifts and mixed `appearance: none` headers.
final class NavigationBackIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        UIView.setAnimationsEnabled(true)
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 8, file: StaticString = #filePath, line: UInt = #line, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
        XCTAssertTrue(done(), what, file: file, line: line)
    }

    private func node(_ testId: String) -> NodeView? {
        session?.presenter.views.values.first { $0.props["testId"] == testId }
    }

    private func stateInt(_ session: ExactSession, _ name: String) -> Int? {
        guard let data = session.agent("{\"op\":\"state\"}").data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let values = json["slots"] as? [String: Any] else { return nil }
        return values[name] as? Int
    }

    private func fixture(_ scene: UIWindowScene) throws -> (ExactSession, UITabBarController, UINavigationController) {
        let plan = try fixturePlan("NAV_BACK_GUARD")
        let session = ExactApp.shared.makeSession(label: "nav-back-guard")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 393, height: 852)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        session.presenter.press(try XCTUnwrap(node("tab-sessions")).id)
        until("sessions tab selected") { tabs.selectedIndex == 1 && session.presenter.navigationHost.primaryNavigation != nil }
        let nav = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        until("sessions root settled") {
            nav.viewControllers.count == 1 && nav.transitionCoordinator == nil && !session.presenter.navigationHost.transitioning
                && nav.topViewController?.viewIfLoaded?.window != nil
        }
        return (session, tabs, nav)
    }

    private func pushRoute(_ testId: String, in nav: UINavigationController) throws {
        let session = try XCTUnwrap(self.session)
        let button = try XCTUnwrap(node(testId))
        session.presenter.press(button.id)
        until("\(testId) pushed and settled") {
            nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !session.presenter.navigationHost.transitioning
                && nav.topViewController?.viewIfLoaded?.window != nil
        }
        window?.layoutIfNeeded()
    }

    /// Presses UIKit's rendered navigation-bar Back control (`drawnBackControl`) through
    /// UIKit's real control dispatch so both `backAction` and any native pop behavior run.
    private func pressRenderedBackControl(on nav: UINavigationController, file: StaticString = #filePath, line: UInt = #line) throws {
        nav.navigationBar.layoutIfNeeded()
        let control = try XCTUnwrap(nav.navigationBar.drawnBackControl(), "navigation bar must render a Back control", file: file, line: line)
        let events: UIControl.Event = control.allControlEvents.contains(.primaryActionTriggered) ? .primaryActionTriggered : .touchUpInside
        control.sendActions(for: events)
    }

    func testBackActionRefusingWithConfirmationKeepsControllerAndOpensAlert() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation bar back button and confirmation dialogs require a hosted window scene") }
        let (session, _, nav) = try fixture(scene)
        try pushRoute("open-detail", in: nav)
        let detail = try XCTUnwrap(nav.topViewController)
        let home = nav.viewControllers[0]
        XCTAssertEqual(home.navigationItem.backButtonDisplayMode, .minimal)
        XCTAssertNil(home.navigationItem.backButtonTitle)
        XCTAssertNotNil(detail.navigationItem.backAction, "pushed route must own backAction")

        try pressRenderedBackControl(on: nav)
        until("confirmation dialog presented without popping") {
            self.stateInt(session, "leaves") == 1 && session.presenter.menus.confirmation != nil
        }
        XCTAssertEqual(nav.viewControllers.count, 2, "UIKit must not pop while handler refuses")
        XCTAssertIdentical(nav.topViewController, detail, "same pushed controller stays mounted")
        // If a transition coordinator is active, it belongs to presenting the confirmation alert, never popping detail -> home.
        if let tc = nav.transitionCoordinator {
            XCTAssertFalse(tc.viewController(forKey: .from) === detail && tc.viewController(forKey: .to) === home,
                           "no pop transition should have started")
        }
        until("alert presentation settled without popping") { nav.transitionCoordinator == nil }
        XCTAssertEqual(nav.viewControllers.count, 2)
        XCTAssertIdentical(nav.topViewController, detail)

        // Choosing Stay closes the dialog while keeping the pushed route mounted.
        session.presenter.press(try XCTUnwrap(node("cancel-leave")).id)
        until("stay closes alert while keeping detail mounted") {
            session.presenter.menus.confirmation == nil && nav.transitionCoordinator == nil
        }
        XCTAssertEqual(nav.viewControllers.count, 2)
        XCTAssertIdentical(nav.topViewController, detail)
        XCTAssertEqual(stateInt(session, "leaves"), 1)

        // Pressing Back again and confirming Leave pops once to root.
        try pressRenderedBackControl(on: nav)
        until("second confirmation dialog presented") {
            self.stateInt(session, "leaves") == 2 && session.presenter.menus.confirmation != nil
        }
        session.presenter.press(try XCTUnwrap(node("confirm-leave")).id)
        until("confirmed leave pops to root") {
            nav.viewControllers.count == 1 && nav.transitionCoordinator == nil && session.presenter.menus.confirmation == nil
        }
        XCTAssertEqual(stateInt(session, "leaves"), 2, "confirming must not invoke leaveSession twice")
    }

    func testBackActionRedirectingToAnotherTabSwitchesCleanlyOnce() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation bar back button requires a hosted window scene") }
        let (session, tabs, nav) = try fixture(scene)
        session.presenter.press(try XCTUnwrap(node("mode-redirect")).id)
        try pushRoute("open-detail", in: nav)
        let detail = try XCTUnwrap(nav.topViewController)

        try pressRenderedBackControl(on: nav)
        until("redirected to agents tab") {
            tabs.selectedIndex == 0 && self.stateInt(session, "leaves") == 1 && !session.presenter.navigationHost.transitioning
        }
        XCTAssertEqual(stateInt(session, "leaves"), 1, "leaveSession runs exactly once")
        XCTAssertEqual(tabs.selectedIndex, 0)
        XCTAssertEqual(nav.viewControllers.count, 2, "source tab stack stays intact after redirect")
        XCTAssertIdentical(nav.topViewController, detail)
    }

    func testBackActionNormalPopPressesOnceAndPreservesTitledAppearance() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation bar back button requires a hosted window scene") }
        let (session, _, nav) = try fixture(scene)
        session.presenter.press(try XCTUnwrap(node("mode-pop")).id)
        session.presenter.press(try XCTUnwrap(node("toggle-titled")).id)
        try pushRoute("open-detail", in: nav)
        let home = nav.viewControllers[0]
        XCTAssertEqual(home.navigationItem.backButtonDisplayMode, .default)
        XCTAssertEqual(home.navigationItem.backButtonTitle, "Agents")

        try pressRenderedBackControl(on: nav)
        until("normal pop completes") {
            nav.viewControllers.count == 1 && nav.transitionCoordinator == nil && !session.presenter.navigationHost.transitioning
        }
        XCTAssertEqual(stateInt(session, "leaves"), 1, "followPops must not fire a second press")
        XCTAssertNil(home.navigationItem.backAction, "root item must never keep a backAction")
    }

    func testBackActionWithoutAuthoredControlRoutesThroughRootNavigateBeforePop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation bar back button requires a hosted window scene") }
        let (session, _, nav) = try fixture(scene)
        try pushRoute("open-plain", in: nav)
        let plain = try XCTUnwrap(nav.topViewController)

        try pressRenderedBackControl(on: nav)
        until("root navigate invoked and confirmation shown") {
            self.stateInt(session, "navigated") == 1 && session.presenter.menus.confirmation != nil
        }
        XCTAssertEqual(nav.viewControllers.count, 2)
        XCTAssertIdentical(nav.topViewController, plain)
    }

    func testRouteHatchBackActionIsNotClobberedByReprojection() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation bar back button requires a hosted window scene") }
        let (session, _, nav) = try fixture(scene)
        try pushRoute("open-detail", in: nav)
        let detail = try XCTUnwrap(nav.topViewController)
        let hatchID = UIAction.Identifier("app.custom.hatch.back")
        detail.navigationItem.backAction = UIAction(title: "Custom", identifier: hatchID) { _ in }

        // Force a re-projection pass (toggling Back title mutates `barSource`).
        session.presenter.press(try XCTUnwrap(node("detail-toggle-titled")).id)
        until("reprojection completes") { nav.viewControllers[0].navigationItem.backButtonTitle == "Agents" }
        XCTAssertEqual(detail.navigationItem.backAction?.identifier, hatchID, "hatch-owned backAction must survive reprojection")
    }

    func testAgentTapOnBackButtonSucceedsWithTwoTrailingButtonsShiftingInlineTitle() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation bar layout requires a hosted window scene") }
        let (session, _, nav) = try fixture(scene)
        try pushRoute("open-detail", in: nav)
        let bar = nav.navigationBar
        let agent = Agent(session: session)
        let back = try XCTUnwrap(node("back-detail"))

        if !bar.traitCollection.placesVerticalBar {
            let control = try XCTUnwrap(bar.drawnBackControl(), "drawnBackControl must locate Back")
            XCTAssertGreaterThanOrEqual(control.convert(control.bounds, to: bar).width, 20)
            let fallback = try XCTUnwrap(bar.drawnBackControlPublicFallback(), "public-API fallback must ignore the left-shifted inline title control")
            XCTAssertIdentical(fallback, control)
        }
        let reply = agent.tap(["id": Int(back.id)])
        XCTAssertNil(reply["error"], "agent tap on back must succeed: \(reply)")
        XCTAssertEqual(reply["pressed"] as? Int, Int(back.id))
        until("agent tap ran leaveSession") {
            self.stateInt(session, "leaves") == 1 && session.presenter.menus.confirmation != nil
        }
    }

    func testAgentTapOnBackButtonSucceedsWithAppearanceNoneSibling() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation bar layout requires a hosted window scene") }
        let (session, _, nav) = try fixture(scene)
        try pushRoute("open-mixed", in: nav)
        let agent = Agent(session: session)
        let back = try XCTUnwrap(node("back-mixed"))

        let reply = agent.tap(["id": Int(back.id)])
        XCTAssertNil(reply["error"], "agent tap on mixed header back must succeed: \(reply)")
        XCTAssertEqual(reply["pressed"] as? Int, Int(back.id))
        until("mixed back tap ran leaveSession") {
            self.stateInt(session, "leaves") == 1
        }
    }

    func testCollapsedSplitDetailBackRunsNavigationBackWithoutDisturbingBarItems() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "collapsed split bar requires a hosted window scene") }
        let plan = try fixturePlan("SPLIT_BACK_GUARD")
        let session = ExactApp.shared.makeSession(label: "split-back-guard")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.traitOverrides.horizontalSizeClass = .compact
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }

        let navHost = session.presenter.navigationHost
        until("collapsed split installed") {
            navHost.split != nil && navHost.split?.controller.isCollapsed == true && !navHost.transitioning
        }
        let columns = try XCTUnwrap(navHost.split)
        let listRoot = try XCTUnwrap(columns.list.stackRoutes.first as? RouteController)
        XCTAssertNil(listRoot.navigationItem.backButtonTitle, "empty detail stack leaves list root Back default")

        session.presenter.press(try XCTUnwrap(node("open-split-detail")).id)
        until("detail pushed in collapsed split") {
            columns.detailRoutes.count == 1 && columns.nested && columns.list.transitionCoordinator == nil
                && columns.detail.transitionCoordinator == nil && !navHost.transitioning
        }
        window.layoutIfNeeded()

        let detail = try XCTUnwrap(columns.detailRoutes.first)
        XCTAssertEqual(listRoot.navigationItem.backButtonDisplayMode, .default)
        XCTAssertEqual(listRoot.navigationItem.backButtonTitle, "Library")
        XCTAssertEqual(detail.navigationItem.backAction?.identifier, NavigationHost.exactBackActionID)
        XCTAssertEqual(detail.navigationItem.leftBarButtonItems?.count, 1, "detail leading item stays intact")
        XCTAssertEqual(detail.navigationItem.rightBarButtonItems?.count, 1, "detail trailing item stays intact")
        XCTAssertTrue(detail.navigationItem.leftItemsSupplementBackButton)
        let backControl = try XCTUnwrap(columns.list.navigationBar.drawnBackControl(), "collapsed split list bar must show one Back control")
        XCTAssertGreaterThanOrEqual(backControl.convert(backControl.bounds, to: columns.list.navigationBar).width, 20)

        // Pressing UIKit's rendered Back control runs leaveSession without popping the detail column.
        try pressRenderedBackControl(on: columns.list)
        until("collapsed split confirmation dialog opened without popping") {
            self.stateInt(session, "leaves") == 1 && session.presenter.menus.confirmation != nil
        }
        XCTAssertEqual(columns.detailRoutes.count, 1)
        XCTAssertTrue(columns.nested)
        XCTAssertIdentical(columns.detailRoutes.first, detail)
        XCTAssertEqual(detail.navigationItem.leftBarButtonItems?.count, 1)
        XCTAssertEqual(detail.navigationItem.rightBarButtonItems?.count, 1)

        // Confirming Leave pops detail cleanly and clears both the list root's projected Back and detail's backAction.
        session.presenter.press(try XCTUnwrap(node("confirm-leave")).id)
        until("collapsed split detail popped and list root Back cleared") {
            columns.shown.count == 1 && !columns.nested && !columns.showsDetail && session.presenter.menus.confirmation == nil
                && columns.list.transitionCoordinator == nil && !navHost.transitioning
                && detail.navigationItem.backAction == nil
        }
        XCTAssertEqual(stateInt(session, "leaves"), 1, "confirmed split leave must not double-invoke leaveSession")
        XCTAssertEqual(listRoot.navigationItem.backButtonDisplayMode, .default)
        XCTAssertNil(listRoot.navigationItem.backButtonTitle, "clearing detail stack must reset list root Back title")
        XCTAssertNil(detail.navigationItem.backAction, "popped detail must clear Exact's backAction once unnested")
    }
}
#endif
