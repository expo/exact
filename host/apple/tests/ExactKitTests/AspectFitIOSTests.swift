#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1123 D16: the first root of a standalone session claims the window's aspect-fit guide at its
/// ratio and is laid out in the guide's rectangle; the claim ends with the window's ratio back.
final class AspectFitIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []
    /// The window's own guide ratio, before the session booted.
    private var windowRatio: CGFloat = 0

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

    /// The fixture booted in an `ExactView` filling a window of `scene`, as the standalone adapter mounts it or not.
    private func plan() throws -> Data {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_ASPECT_FIT_PLAN"], "build.mjs --test --ios compiles the fixture's plan")
        return try Data(contentsOf: URL(fileURLWithPath: path))
    }

    private func fixture(_ scene: UIWindowScene, standalone: Bool) throws -> (ExactSession, ExactView, UIWindow) {
        let plan = try plan()
        let session = ExactApp.shared.makeSession(label: "aspect-fit")
        sessions.append(session)
        let view = ExactView(session: session)
        view.standaloneWindow = standalone
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        windowRatio = window.safeAreaAspectFitLayoutGuide.aspectRatio
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.layoutIfNeeded()
        until("booted") { session.presenter.views.values.contains { $0.props["testId"] == "root" } }
        return (session, view, window)
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    /// The guide's rectangle in the root's parent's coordinates, where the root's frame is.
    private func guide(_ window: UIWindow, in root: NodeView) -> CGRect {
        root.superview!.convert(window.safeAreaAspectFitLayoutGuide.layoutFrame, from: window)
    }

    private func near(_ a: CGRect, _ b: CGRect) -> Bool {
        abs(a.minX - b.minX) < 0.5 && abs(a.minY - b.minY) < 0.5 && abs(a.width - b.width) < 0.5 && abs(a.height - b.height) < 0.5
    }

    func testTheFirstRootClaimsTheGuideFollowsItsRatioAndGivesItBack() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the guide is a window's") }
        let (session, _, window) = try fixture(scene, standalone: true)
        let guideRatio = { window.safeAreaAspectFitLayoutGuide.aspectRatio }
        let saved = windowRatio
        XCTAssertNotEqual(saved, 16.0 / 9.0)
        let root = try node(session, "root")
        until("the guide takes the root's ratio") { abs(guideRatio() - 16.0 / 9.0) < 0.001 }
        until("the root is laid out in the guide") { self.near(root.frame, self.guide(window, in: root)) }
        XCTAssertLessThan(root.frame.height, window.bounds.height, "a rectangle of its own, not the window")
        XCTAssertEqual(try node(session, "half").frame.width, root.frame.width / 2, accuracy: 0.5, "a percentage of the guide")
        // The ratio follows the root's state, which a timer changes with no press laying the view out
        session.presenter.press(try node(session, "wide").id)
        until("the guide takes the new ratio") { abs(guideRatio() - 2) < 0.001 }
        until("the root follows the guide") { self.near(root.frame, self.guide(window, in: root)) }
        // Given up, the window's ratio comes back and the root fills the view again
        session.presenter.press(try node(session, "off").id)
        until("the window's own ratio is back") { guideRatio() == saved }
        until("the root fills the view again") { abs(root.frame.height - window.bounds.height) < 0.5 }
    }

    func testARestartedRunnerIsToldTheGuideAgain() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the guide is a window's") }
        let (session, _, window) = try fixture(scene, standalone: true)
        let fitted = { (try? self.node(session, "root")).map { self.near($0.frame, self.guide(window, in: $0)) } ?? false }
        until("the root is laid out in the guide") { fitted() }
        // The same rectangle, to a runner that has never heard it
        XCTAssertNil(session.boot(plan: try plan(), size: window.bounds.size).error)
        until("the new runner's root is laid out in the guide") { fitted() }
    }

    func testAnEmbeddedSessionClaimsNoGuide() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the guide is a window's") }
        let (session, _, window) = try fixture(scene, standalone: false)
        let saved = windowRatio
        let root = try node(session, "root")
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        XCTAssertEqual(window.safeAreaAspectFitLayoutGuide.aspectRatio, saved, "the window's ratio untouched")
        XCTAssertEqual(root.frame.height, window.bounds.height, accuracy: 0.5, "the root fills the view")
    }
}
#endif
