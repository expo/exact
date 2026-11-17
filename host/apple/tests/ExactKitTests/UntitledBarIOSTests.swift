#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// Issue #364 over `host/apple/tests/fixtures/untitled-bar.contract`: a
/// route's first `header` holding a button and no heading is UIKit's bar
/// with no title, the button a trailing item that presses it, and the
/// header not painted. A header that also holds a text is the page's, and
/// the pushed route under it has the system Back alone.
final class UntitledBarIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    private func text(_ session: ExactSession, _ testId: String) -> String? {
        (try? node(session, testId))?.accessibleText
    }

    func testAHeaderOfAButtonAloneIsTheBarWithNoTitle() throws {
        let session = ExactApp.shared.makeSession(label: "untitled-bar")
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try fixturePlan("UNTITLED_BAR"), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the route is up") { session.presenter.navigation.primaryNavigation?.topViewController is RouteController }
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let home = try XCTUnwrap(nav.topViewController as? RouteController)
        until("the header is the bar") { !nav.isNavigationBarHidden }
        let item = home.navigationItem
        XCTAssertTrue(item.title?.isEmpty ?? true, "no heading: no title")
        XCTAssertEqual(item.largeTitleDisplayMode, .never)
        XCTAssertTrue((item.leftBarButtonItems ?? []).isEmpty, "nothing leads")
        let add = try XCTUnwrap(item.rightBarButtonItems?.first, "the button trails")
        XCTAssertEqual(item.rightBarButtonItems?.count, 1)
        XCTAssertEqual(add.accessibilityLabel, "Add")
        XCTAssertEqual(add.accessibilityIdentifier, "add-home")
        XCTAssertTrue(try node(session, "header-home").isHidden, "iOS does not paint the header the bar shows")
        // The item presses the authored button.
        let target = try XCTUnwrap(add.target as? NSObject)
        _ = target.perform(try XCTUnwrap(add.action), with: add)
        until("Add pressed") { self.text(session, "added") == "added 1" }

        // Detail's header holds a text beside its button: the page's, under the system Back alone.
        let open = Agent(session: session).tap(["id": Int(try node(session, "open-detail").id)])
        XCTAssertNil(open["error"], "\(open)")
        until("Detail pushed") { nav.viewControllers.count == 2 && nav.transitionCoordinator == nil }
        let detail = try XCTUnwrap(nav.topViewController as? RouteController)
        XCTAssertFalse(detail.navigationItem.hidesBackButton, "the system Back")
        XCTAssertTrue((detail.navigationItem.rightBarButtonItems ?? []).isEmpty, "the header is no bar's")
        XCTAssertFalse(try node(session, "header-detail").isHidden, "it paints in the page as written")
    }
}
#endif
