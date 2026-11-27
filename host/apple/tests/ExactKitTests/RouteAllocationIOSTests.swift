#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1123 D4, D8: a route's `navigationExtendedLayout` and `navigationExtendsUnderOpaqueBars` set its controller's
/// allocation under bars, and on iOS 27.1 `navigationVerticalBar` and `navigationVerticalBarCompression` its
/// vertical bar, and `-exact-respects-system-minimum-layout-margins` its controller's minimum margins, each following the
/// route's state.
final class RouteAllocationIOSTests: XCTestCase {
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

    func testARoutesAllocationAndVerticalBarFollowItsState() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's controller is in a window") }
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_ROUTE_ALLOCATION_PLAN"], "build.mjs --test --ios compiles the fixture's plan")
        let session = ExactApp.shared.makeSession(label: "route-allocation")
        sessions.append(session)
        let view = ExactView(session: session), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.frame = host.view.bounds
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.layoutIfNeeded()
        var route: RouteController? { session.presenter.navigation.routeSurfaces.values.lazy.compactMap { $0 as? RouteController }.first }
        until("the route is up") { route?.view.window != nil }
        let shown = try XCTUnwrap(route)
        XCTAssertEqual(shown.edgesForExtendedLayout, [.top, .bottom])
        XCTAssertTrue(shown.extendedLayoutIncludesOpaqueBars)
        XCTAssertFalse(shown.viewRespectsSystemMinimumLayoutMargins, "D8's minimum margins")
        if #available(iOS 27.1, *) {
            XCTAssertEqual(shown.preferredVerticalBarBehavior, .disabled)
            XCTAssertEqual(shown.navigationItem.verticalBarCompressionBehavior, .prefersTabBar)
            XCTAssertEqual(shown.navigationController?.childForPreferredVerticalBarBehavior, shown, "UIKit's container asks the route")
        }
        let toggle = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "toggle" })
        session.presenter.press(toggle.id)
        until("the route follows its state") { shown.edgesForExtendedLayout == .all }
        XCTAssertFalse(shown.extendedLayoutIncludesOpaqueBars)
        XCTAssertTrue(shown.viewRespectsSystemMinimumLayoutMargins)
        if #available(iOS 27.1, *) {
            XCTAssertEqual(shown.preferredVerticalBarBehavior, .automatic)
            XCTAssertEqual(shown.navigationItem.verticalBarCompressionBehavior, .automatic)
        }
    }
}
#endif
