#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1123 D9 over `scripts/fixtures/bar-insets.contract`: a route's content
/// reads its controller's safe area, the navigation bar's height in it, as
/// `var(--exact-safe-area-inset-*)`, so a background reaches up under the bar
/// without a hard-coded height (expo/exact#36), on an iPhone and an iPad
/// alike; a box outside every route reads the screen's insets, and a sheet's
/// content its sheet's. And over `scripts/fixtures/bar-hero.contract`, a
/// route whose scroller starts at its top opts that edge out of its
/// clearance (D4), so the scroller's first box reaches under the bar.
final class LocalInsetsIOSTests: XCTestCase {
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

    /// The fixture in a window of the test host's scene, the screen's size.
    private func fixture(_ scene: UIWindowScene, _ key: String = "BAR_INSETS") throws -> (ExactSession, ExactView) {
        let session = ExactApp.shared.makeSession(label: "bar-insets")
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        self.window = window
        XCTAssertNil(session.boot(plan: try fixturePlan(key), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        return (session, view)
    }

    private func node(_ session: ExactSession, _ testId: String) -> NodeView? {
        session.presenter.views.values.first { $0.props["testId"] == testId && $0.window != nil }
    }

    private func onScreen(_ view: UIView) -> CGRect { view.convert(view.bounds, to: nil) }

    func testATransparentBarsContentReadsItsLocalInsetWhileTheDocumentInsetIsZero() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a transparent bar requires UIKit containment") }
        let (session, _) = try fixture(scene, "TRANSPARENT_BAR")
        let p = session.presenter
        waitFor("the transparent route appears") {
            guard let route = self.node(session, "route-home"), let c = p.navigation.controllers[route.id] else { return false }
            return c.viewIfLoaded?.window != nil && c.view.safeAreaInsets.top > 0 && !p.navigation.inTransition
        }
        let route = try XCTUnwrap(node(session, "route-home"))
        let controller = try XCTUnwrap(p.navigation.controllers[route.id])
        let scroll = try XCTUnwrap(node(session, "home-scroll")?.scrollView)
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .never)
        waitFor("the hero's control clears the bar using the route's inset", facts: {
            "control=\(self.node(session, "hero-control").map(self.onScreen) ?? .zero), hero=\(self.node(session, "hero").map(self.onScreen) ?? .zero), safe=\(controller.view.safeAreaInsets), local=\(p.navigation.localInsets.wanted[route.id] ?? []), offset=\(scroll.contentOffset)"
        }) {
            guard let control = self.node(session, "hero-control") else { return false }
            return abs(self.onScreen(control).minY - controller.view.safeAreaInsets.top) < 0.5
        }
        XCTAssertGreaterThan(controller.view.safeAreaInsets.top, try XCTUnwrap(window).safeAreaInsets.top)
        XCTAssertEqual(onScreen(try XCTUnwrap(node(session, "hero"))).minY, 0, accuracy: 0.5)
        XCTAssertEqual(try XCTUnwrap(node(session, "document-inset")).bounds.height, 0)
        XCTAssertEqual(p.insets.top, 0, "env() remains the document's inset under auto")
    }

    func testARoutesContentReadsItsBarsAndABackgroundReachesUnderThem() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the bars and the safe area are a scene window's")
        }
        let (session, view) = try fixture(scene)
        until("the route shows under its bar") {
            guard let route = node(session, "route"), let c = session.presenter.navigation.controllers[route.id] else { return false }
            return c.viewIfLoaded?.window != nil && c.view.safeAreaInsets.top > view.safeAreaInsets.top
        }
        let route = try XCTUnwrap(node(session, "route"))
        let c = try XCTUnwrap(session.presenter.navigation.controllers[route.id])
        let bars = c.view.safeAreaInsets
        until("the content reads the route's safe area") {
            guard let first = self.node(session, "sidebar-first") else { return false }
            return abs(self.onScreen(first).minY - bars.top) < 0.5
        }
        let sidebar = try XCTUnwrap(node(session, "sidebar")), list = try XCTUnwrap(node(session, "list"))
        XCTAssertEqual(onScreen(sidebar).minY, onScreen(route).minY, accuracy: 0.5, "the sidebar's background starts at the route's top, under the bar")
        XCTAssertEqual(onScreen(sidebar).minY, 0, accuracy: 0.5, "which is the screen's")
        let first = try XCTUnwrap(node(session, "sidebar-first"))
        XCTAssertEqual(onScreen(first).minY, bars.top, accuracy: 0.5, "its content starts below the bar: \(bars.top) points, read, not written")
        XCTAssertEqual(onScreen(list).minY, bars.top, accuracy: 0.5, "the list beside it already cleared the bar and stays")
        let absolute = try XCTUnwrap(node(session, "absolute-probe"))
        XCTAssertEqual(onScreen(absolute).minY, bars.top, accuracy: 0.5, "the absolute child's local inset clears the route's cover once")
        let probe = try XCTUnwrap(node(session, "root-probe"))
        XCTAssertEqual(probe.bounds.height, view.safeAreaInsets.top, accuracy: 0.5, "outside every route, the screen's top inset, though env() is 0")
        XCTAssertEqual(session.presenter.insets.top, 0, "env() stays the document's")
    }

    func testASheetsContentReadsTheSheetsInsets() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the bars and the safe area are a scene window's")
        }
        let (session, _) = try fixture(scene)
        until("the route shows") { self.node(session, "open") != nil }
        _ = Agent(session: session).tap(["id": Int(try XCTUnwrap(node(session, "open")).id)])
        until("the sheet is presented") {
            guard let sheet = self.node(session, "sheet"), let c = session.presenter.navigation.controllers[sheet.id] else { return false }
            return c.presentingViewController != nil && c.viewIfLoaded?.window != nil
        }
        let sheet = try XCTUnwrap(node(session, "sheet"))
        let c = try XCTUnwrap(session.presenter.navigation.controllers[sheet.id])
        until("the probe reads the sheet's own bottom inset") {
            guard let probe = self.node(session, "sheet-probe") else { return false }
            return abs(probe.bounds.height - c.view.safeAreaInsets.bottom) < 0.5
        }
    }

    func testAScrollerAtTheTopOptsOutAndItsFirstBoxReachesUnderTheBar() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the bars and the safe area are a scene window's")
        }
        let (session, _) = try fixture(scene, "BAR_HERO")
        until("the route shows under its bar") {
            guard let route = self.node(session, "route"), let c = session.presenter.navigation.controllers[route.id] else { return false }
            return c.viewIfLoaded?.window != nil && c.view.safeAreaInsets.top > 0 && c.collapseScroll != nil
        }
        let route = try XCTUnwrap(node(session, "route"))
        let c = try XCTUnwrap(session.presenter.navigation.controllers[route.id])
        let list = try XCTUnwrap(node(session, "list")), sv = try XCTUnwrap(list.scrollView)
        XCTAssertEqual(sv.contentInsetAdjustmentBehavior, .never, "UIKit insets nothing of the top")
        let top = c.view.safeAreaInsets.top
        until("the hero's text clears the bar it reads") {
            guard let text = self.node(session, "hero-text") else { return false }
            return abs(self.onScreen(text).minY - top) < 0.5
        }
        let hero = try XCTUnwrap(node(session, "hero"))
        XCTAssertEqual(onScreen(hero).minY, 0, accuracy: 0.5, "the hero's background starts at the screen's top, under the bar")
        XCTAssertEqual(sv.contentOffset.y + sv.adjustedContentInset.top, 0, accuracy: 0.5, "at its start")
        if case .edges(let edges)? = session.presenter.navigation.covers[route.id] { XCTAssertEqual(edges.top, 0, "no top cover") }
    }
}
#endif
