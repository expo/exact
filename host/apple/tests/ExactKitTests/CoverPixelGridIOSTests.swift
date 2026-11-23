#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A route's sides are told on the device's pixel grid. A side is measured from the route's box, laid out in the side
/// last told, so it carries that layout's rounding back: a sidebar 495⅔ points wide at an iPhone Duo's partial fold
/// told a side an ulp smaller every pass, each a change that laid the route out again, and the app froze.
final class CoverPixelGridIOSTests: XCTestCase {
    func testASideLaidOutAnUlpOffIsToldOnThePixelGrid() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "a route's safe area is UIKit's only in a window a scene holds")
        }
        let session = try fixture("cover-pixel-grid", scene.screen.bounds.size)
        let p = session.presenter, agent = Agent(session: session)
        p.press(try node(session, "home-open-viewport").id)
        XCTAssertEqual(agent.clock(["settle": true])["settled"] as? Bool, true)
        let nav = try XCTUnwrap(p.navigationHost.primaryNavigation)
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let scale = route.view.traitCollection.displayScale
        func left() -> CGFloat? { if case .edges(let e)? = p.navigationHost.covers[route.node.id] { e.left } else { nil } }
        // A third of a point no binary fraction holds, as a sidebar's width at a partial fold
        let side = (495.67 * scale).rounded(.down) / scale
        let insets = route.additionalSafeAreaInsets
        route.additionalSafeAreaInsets = UIEdgeInsets(top: insets.top, left: side, bottom: insets.bottom, right: insets.right)
        route.view.layoutIfNeeded()
        until("the route is told its left side") { left().map { abs($0 - side) < 0.5 } == true && p.navigationHost.delivered == p.navigationHost.covers }
        // The side held as the layout's round trip leaves it, an ulp off: what the box laid out in it measures again
        let told = try XCTUnwrap(left())
        guard case .edges(var held)? = p.navigationHost.covers[route.node.id] else { return XCTFail("the route has no sides") }
        held.left = told.nextDown
        p.navigationHost.covers[route.node.id] = .edges(held)
        p.navigationHost.reportCovers()
        let again = try XCTUnwrap(left())
        XCTAssertEqual(again, PixelGrid.round(again, scale: scale), "the side is on the pixel grid")
        XCTAssertEqual(again, told, "the side measured again is the side told")
        // Settled, it is told no more
        var reports = 0
        let tell = p.onCovers
        p.onCovers = { reports += 1; tell?($0) }
        until("the covers settle") { p.navigationHost.delivered == p.navigationHost.covers }
        let settled = reports
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        XCTAssertEqual(reports, settled, "no report follows once the covers settle")
    }

    private var window: UIWindow?
    private var host: UIViewController?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        host = nil
        super.tearDown()
    }

    private func until(_ what: @autoclosure () -> String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what())
    }

    /// The pushed-title fixture booted in a window of the test host's scene, `size` large.
    private func fixture(_ label: String, _ size: CGSize) throws -> ExactSession {
        let key = "EXACT_PUSHED_TITLE_PLAN"
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment[key], "build.mjs --test --ios compiles \(key)")))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let frame = CGRect(origin: .zero, size: size)
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: frame)
        window.frame = frame
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        self.host = host
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the stack is installed") { session.presenter.navigationHost.primaryOwner != nil }
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }
}
#endif
