#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
import ExactGroupedLists

/// What a full-screen route stands on, over `scripts/fixtures/navigation-bars.contract`
/// (Charlie, 2026-11-10): a route that paints its own background leaves its
/// presentation's view and its controller's clear, so the background is drawn
/// once, by the route, over the page it covers, which `overFullScreen` keeps in
/// the window; a route that paints nothing keeps the system background.
final class ModalSurfaceIOSTests: XCTestCase {
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

    private func booted(_ label: String) throws -> (ExactSession, RouteController) {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_NAVIGATION_BARS_PLAN"], "build.mjs --test --ios compiles the fixtures' plans")))
        ExactGroupedLists.install() // the fixture's `rows` route is a grouped list
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = try UIWindow(windowScene: XCTUnwrap(hostedWindowScene(), "a presentation enters its presenter's window only through a scene"))
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        return (session, try XCTUnwrap(nav.topViewController as? RouteController))
    }

    /// `name`'s full-screen route, opened from `start` and presented: the
    /// presentation's controller and the route's.
    private func open(_ session: ExactSession, _ name: String) throws -> (UIViewController, RouteController) {
        let navigation = session.presenter.navigation
        let button = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "start-open-\(name)" })
        let reply = Agent(session: session).tap(["id": Int(button.id)])
        XCTAssertNil(reply["error"], "the opener for \(name) is tapped")
        until("the route is presented") {
            navigation.modalNavigation?.viewControllers.count == 1 && navigation.modalNavigation?.parent?.presentingViewController != nil
        }
        let presented = try XCTUnwrap(navigation.modalNavigation?.parent)
        let route = try XCTUnwrap(navigation.modalNavigation?.topViewController as? RouteController)
        XCTAssertEqual(route.node.props["testId"], "route-\(name)")
        XCTAssertEqual(presented.modalPresentationStyle, .overFullScreen)
        // The layer takes its fill when Core Animation displays it, in the window
        until("the route is in the window and drawn") { route.node.window != nil && !route.node.layer.needsDisplay() }
        return (presented, route)
    }

    private func alpha(_ color: UIColor?, in view: UIView) -> CGFloat? { color?.resolvedColor(with: view.traitCollection).cgColor.alpha }

    func testATranslucentFullScreenRouteIsPaintedOnceOverThePage() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a full-screen presentation shows in a window only under a scene") }
        let (session, start) = try booted("surface-veil")
        let (presented, route) = try open(session, "veil")
        XCTAssertEqual(presented.view.backgroundColor, .clear, "the presentation paints no surface")
        XCTAssertEqual(alpha(route.view.backgroundColor, in: route.view), 0, "nor does the route's controller")
        XCTAssertEqual(route.node.layer.backgroundColor?.alpha ?? 0, 0.5, accuracy: 0.01,
                       "the route paints its own half-black, once")
        XCTAssertNotNil(start.view.window, "and the page it covers is still in the window under it")
    }

    func testAnOpaqueFullScreenRouteLeavesTheSurfacesClearToo() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a full-screen presentation shows in a window only under a scene") }
        let (session, _) = try booted("surface-solid")
        let (presented, route) = try open(session, "solid")
        XCTAssertEqual(presented.view.backgroundColor, .clear)
        XCTAssertEqual(alpha(route.view.backgroundColor, in: route.view), 0)
        XCTAssertEqual(route.node.layer.backgroundColor?.alpha, 1, "its own colour covers the page")
    }

    func testAFullScreenRouteThatPaintsNothingKeepsTheSystemBackground() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a full-screen presentation shows in a window only under a scene") }
        let (session, _) = try booted("surface-full")
        let (presented, route) = try open(session, "full")
        XCTAssertEqual(presented.view.backgroundColor, .systemBackground, "the platform's surface (LLP 1115 D2)")
        XCTAssertEqual(alpha(route.view.backgroundColor, in: route.view), 1, "under the route, which paints none")
    }
}
#endif
