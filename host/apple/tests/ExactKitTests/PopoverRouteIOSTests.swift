#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1123 D7: a route presented as a popover (`navigationPresentation="popover"`) is UIKit's popover,
/// anchored to the element its `navigationPopoverAnchor` names and shaped by its other choices.
final class PopoverRouteIOSTests: XCTestCase {
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

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    /// The fixture booted in a window of the test host's scene: UIKit presents only from a window a scene holds.
    private func fixture(_ scene: UIWindowScene) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_POPOVER_ROUTE_PLAN"], "build.mjs --test --ios compiles the fixture's plan")))
        let session = ExactApp.shared.makeSession(label: "popover-route")
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        self.host = host
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("booted") { session.presenter.views.values.contains { $0.props["testId"] == "open" } }
        // A presentation waits for the first drawn frame, which this window may not draw
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) -> NodeView? {
        session.presenter.views.values.first { $0.props["testId"] == testId && $0.window != nil }
    }

    private func press(_ session: ExactSession, _ testId: String) throws {
        session.presenter.press(try XCTUnwrap(node(session, testId), "no \(testId)").id)
    }

    /// The popover over the fixture, once presented.
    private var presented: UIViewController? { host?.presentedViewController }

    func testAPopoverRouteIsUIKitsPopoverAnchoredAndShapedAsItSays() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit presents a popover from a window a scene holds") }
        let session = try fixture(scene)
        let anchor = try XCTUnwrap(node(session, "anchor"))
        try press(session, "anchor")
        until("the popover is presented") { presented?.popoverPresentationController?.arrowDirection != .unknown }
        let controller = try XCTUnwrap(presented), source = try XCTUnwrap(controller.popoverPresentationController)
        XCTAssertEqual(controller.modalPresentationStyle, .popover)
        XCTAssertTrue(source.sourceView === anchor, "anchored to the element its navigationPopoverAnchor names")
        XCTAssertTrue(source.sourceRect.isNull, "the anchor's whole box")
        XCTAssertEqual(source.permittedArrowDirections, .any)
        XCTAssertEqual(source.popoverLayoutMargins, UIEdgeInsets(top: 12, left: 20, bottom: 12, right: 20))
        XCTAssertFalse(source.canOverlapSourceViewRect)
        XCTAssertEqual(controller.preferredContentSize, CGSize(width: 240, height: 180), "the route's box's own size")
        let compact = UITraitCollection(horizontalSizeClass: .compact)
        XCTAssertEqual(source.delegate?.adaptivePresentationStyle?(for: source, traitCollection: compact), UIModalPresentationStyle.none,
                       "`none`: a popover in a compact-width window too")
        // Its choices follow the route as it changes them
        try press(session, "down")
        until("its arrow follows the route") { source.permittedArrowDirections == .down }
        try press(session, "adapt")
        until("its adaptation follows the route") {
            source.delegate?.adaptivePresentationStyle?(for: source, traitCollection: compact) == .automatic
        }
        // Its anchor gone, it closes as a tap outside it closes it, through Back
        try press(session, "hide-anchor")
        until("it closes with its anchor gone") { self.presented == nil }
        until("its route left the stack") { self.node(session, "pop") == nil }
    }

    func testAPopoverRouteWaitsForItsAnchor() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit presents a popover from a window a scene holds") }
        let session = try fixture(scene)
        try press(session, "hide")
        until("the anchor is gone") { self.node(session, "anchor") == nil }
        try press(session, "open")
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        XCTAssertNil(presented, "no popover while its anchor is in no window")
        try press(session, "show")
        until("the popover is presented once its anchor is") { self.presented?.modalPresentationStyle == .popover }
        XCTAssertTrue(presented?.popoverPresentationController?.sourceView === node(session, "anchor"))
    }
}
#endif
