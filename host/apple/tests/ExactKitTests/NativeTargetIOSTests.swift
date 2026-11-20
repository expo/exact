#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// A registered native target read back through boxes (LLP 1123 D9, "Registration"): each edge of
/// a view's raw insets, a disposal, another view under the name, and a restart's new kernel.
@MainActor
final class NativeTargetIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testARegistrationReachesEachEdgeAndOutlivesARestart() throws {
        let (session, plan, host) = try boot()
        // A child controller's view with different insets on each edge
        let child = controller(in: host, adding: UIEdgeInsets(top: 11, left: 12, bottom: 13, right: 14))
        var target: ExactNativeTarget? = try ExactNativeTarget.register(name: "map", view: child.view, in: session)
        XCTAssertNotNil(target)
        until("each box reads its edge") { (try? read(session)) == child.view.safeAreaInsets }
        // Disposed of: the fallbacks
        target?.dispose()
        target = nil
        until("each box reads 7") { (try? read(session)) == UIEdgeInsets(top: 7, left: 7, bottom: 7, right: 7) }
        // Another view under the name, though the disposed one held it at its generation
        let other = controller(in: host, adding: UIEdgeInsets(top: 21, left: 22, bottom: 23, right: 24))
        let held = try ExactNativeTarget.register(name: "map", view: other.view, in: session)
        until("each box reads the other view") { (try? read(session)) == other.view.safeAreaInsets }
        // Replaced with the first view: its insets again
        held.replace(view: child.view)
        until("each box reads the replacing view") { (try? read(session)) == child.view.safeAreaInsets }
        held.replace(view: other.view)
        until("and the other view once more") { (try? read(session)) == other.view.safeAreaInsets }
        // A restart's new kernel is told the live registration
        XCTAssertTrue(session.apply(plan, label: "generation"))
        until("the new tree reads it too") { (try? read(session)) == other.view.safeAreaInsets }
        withExtendedLifetime(held) {}
    }

    func testANameTheKernelRefusesIsRefusedByByte() throws {
        let (session, _, _) = try boot()
        for name in ["b\u{301}", "5\u{FE0F}\u{20E3}", String(repeating: "b\u{301}\u{301}", count: 64), String(repeating: "a", count: 65), "-map", "map-", ""] {
            XCTAssertThrowsError(try ExactNativeTarget.register(name: name, view: UIView(), in: session), name) {
                XCTAssertEqual($0 as? ExactNativeTarget.Refusal, .invalidName(name))
            }
        }
        XCTAssertNoThrow(try ExactNativeTarget.register(name: String(repeating: "a", count: 64), view: UIView(), in: session))
    }

    func testATokenReleasedOffTheMainThreadDisposesOfItsName() throws {
        let (session, _, host) = try boot()
        let child = controller(in: host, adding: UIEdgeInsets(top: 11, left: 12, bottom: 13, right: 14))
        var target: ExactNativeTarget? = try ExactNativeTarget.register(name: "map", view: child.view, in: session)
        until("each box reads its edge") { (try? read(session)) == child.view.safeAreaInsets }
        // The last reference goes on another queue: the box hands it over, and that queue empties it
        final class Box: @unchecked Sendable { var token: ExactNativeTarget? }
        let box = Box()
        box.token = target
        target = nil
        let released = expectation(description: "released")
        DispatchQueue.global().async {
            box.token = nil
            released.fulfill()
        }
        wait(for: [released], timeout: 5)
        until("each box reads 7") { (try? read(session)) == UIEdgeInsets(top: 7, left: 7, bottom: 7, right: 7) }
    }

    /// Each box's one padded edge, as UIKit's edge insets.
    private func read(_ session: ExactSession) throws -> UIEdgeInsets {
        func size(_ id: String) throws -> CGSize {
            try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == id }, id).frame.size
        }
        return UIEdgeInsets(top: try size("top").height, left: try size("left").width,
                            bottom: try size("bottom").height, right: try size("right").width)
    }

    /// A child controller filling `host`'s view, adding `insets` to what it inherits.
    private func controller(in host: UIViewController, adding insets: UIEdgeInsets) -> UIViewController {
        let child = UIViewController()
        host.addChild(child)
        child.view.frame = host.view.bounds
        host.view.insertSubview(child.view, at: 0)
        child.didMove(toParent: host)
        child.additionalSafeAreaInsets = insets
        host.view.layoutIfNeeded()
        return child
    }

    /// The registered-target fixture booted in a window of the test host's scene.
    private func boot() throws -> (ExactSession, Data, UIViewController) {
        let plan = try fixturePlan("REGISTERED_TARGET")
        let session = ExactApp.shared.makeSession(label: "native-target")
        self.session = session
        let size = CGSize(width: 402, height: 874)
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(origin: .zero, size: size))
        window.frame = CGRect(origin: .zero, size: size)
        let host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        let view = ExactView(session: session)
        view.frame = host.view.bounds
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        until("the boxes mount") { (try? read(session)) == UIEdgeInsets(top: 7, left: 7, bottom: 7, right: 7) }
        return (session, plan, host)
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }
}
#endif
