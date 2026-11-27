#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactNativeViews

extension NavigationBarIOSTests {
    func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

    func until(_ what: String, _ seconds: Double = 30, _ done: () -> Bool) { waitFor(what, cap: seconds, done) }

    /// The native fixture booted in a window, its module's hatches connected
    /// or not; or, given `plan`, that fixture's plan (`fixturePlan`).
    func fixture(_ label: String, module: Bool, plan key: String? = nil, keyboardWindow: Bool = false) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try key.map(fixturePlan) ?? Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_FIXTURE_PLAN"], "build.mjs --test --ios compiles the fixture's plan")))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        // In the test host's scene when it has one: UIKit presents a sheet
        // only from a window a scene holds.
        let scene = keyboardWindow ? hostedWindowScene() : UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let compact = CGRect(x: 0, y: 0, width: 402, height: 874)
        var frame = compact
        if keyboardWindow, let scene { frame.size.height = scene.coordinateSpace.bounds.height }
        window.frame = frame
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        if module {
            session.nativeViews!.installArtifact(try XCTUnwrap(env["EXACT_FIXTURE_MODULE"], "build.mjs --test --ios builds the fixture's module"))
        }
        until("fixture route and native work ready") {
            session.presenter.navigationHost.primaryNavigation?.topViewController is RouteController &&
                !Agent(session: session).nativeInFlight()
        }
        return session
    }

    func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }
}
#endif
