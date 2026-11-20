#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// `startViewTransition(seconds)`: the action's commit lands in UIKit's
/// `UIView.transition(with: window, options: .transitionCrossDissolve)`.
/// Under a sheet the commit closes, the sheet slides out while the page
/// beneath it cross-fades; one that stays fades with the window.
final class ViewTransitionIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 10, file: StaticString = #filePath, line: UInt = #line, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
        XCTAssertTrue(done(), what, file: file, line: line)
    }

    private func named(_ id: String) -> NodeView? { session?.presenter.views.values.first { $0.props["testId"] == id } }

    private func fixture(_ scene: UIWindowScene) throws -> (ExactSession, UIViewController) {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_VIEW_TRANSITION_PLAN"], "build.mjs --test --ios compiles view-transition.contract")
        let session = ExactApp.shared.makeSession(label: "view-transition")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        view.layoutIfNeeded()
        until("the home route is up") { self.named("pick")?.window != nil }
        return (session, host)
    }

    private func assertCrossFade(_ layer: CALayer, file: StaticString = #filePath, line: UInt = #line) throws {
        let fade = try XCTUnwrap(layer.animation(forKey: "transition") as? CATransition, file: file, line: line)
        XCTAssertEqual(fade.type, .fade, file: file, line: line)
        XCTAssertEqual(fade.duration, 0.3, accuracy: 0.001, file: file, line: line)
    }

    func testACommitCrossFadesTheWindow() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a window transition needs the hosted app's window scene") }
        let (session, _) = try fixture(scene)
        let window = try XCTUnwrap(window)
        session.presenter.press(try XCTUnwrap(named("pick")).id)
        until("the window cross-fades") { window.layer.animation(forKey: "transition") != nil }
        try assertCrossFade(window.layer)
    }

    func testASheetSlidesOutWhileThePageBeneathCrossFades() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a sheet needs the hosted app's window scene") }
        let (session, host) = try fixture(scene)
        let p = session.presenter, window = try XCTUnwrap(window)
        p.press(try XCTUnwrap(named("open")).id)
        until("the sheet is presented") {
            guard let sheet = host.presentedViewController else { return false }
            return !sheet.isBeingPresented && self.named("pick-and-close")?.window != nil
        }
        let sheet = try XCTUnwrap(host.presentedViewController)
        p.press(try XCTUnwrap(named("pick-and-close")).id)
        until("the page beneath cross-fades") { host.view.layer.animation(forKey: "transition") != nil }
        try assertCrossFade(host.view.layer)
        XCTAssertNil(window.layer.animation(forKey: "transition"), "the sheet's container is outside the fade")
        until("the sheet slides out") { sheet.isBeingDismissed }
        XCTAssertEqual(sheet.transitionCoordinator?.isAnimated, true, "the dismissal keeps its own animation")
        until("the sheet is gone") { host.presentedViewController == nil }
    }

    func testASheetThatStaysFadesWithTheWindow() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a sheet needs the hosted app's window scene") }
        let (session, host) = try fixture(scene)
        let p = session.presenter, window = try XCTUnwrap(window)
        p.press(try XCTUnwrap(named("open")).id)
        until("the sheet is presented") {
            guard let sheet = host.presentedViewController else { return false }
            return !sheet.isBeingPresented && self.named("pick-and-stay")?.window != nil
        }
        let sheet = try XCTUnwrap(host.presentedViewController)
        p.press(try XCTUnwrap(named("pick-and-stay")).id)
        until("the window cross-fades") { window.layer.animation(forKey: "transition") != nil }
        try assertCrossFade(window.layer)
        XCTAssertNil(host.view.layer.animation(forKey: "transition"))
        XCTAssertTrue(host.presentedViewController === sheet && !sheet.isBeingDismissed, "the sheet stays")
    }
}
#endif
