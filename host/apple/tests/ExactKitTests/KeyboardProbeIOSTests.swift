#if os(iOS)
import ObjectiveC.runtime
import UIKit
import XCTest
@testable import ExactKit
import ExactGroupedLists

/// The keyboard guide's first constraint makes UIKit lay the window out at
/// once, inside the fit that adds it, and that layout fits the view again.
/// A re-entered fit queues one layout (34676bfa2): marking the view dirty in
/// place kept UIKit in the same pass without end (3e8ed35b7), wherever the
/// fit came from. Each way a fit adds the guide settles.
final class KeyboardProbeIOSTests: XCTestCase {
    fileprivate static var passes = 0

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    private func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

    /// Whether a quiet 0.3 s, with no layout of the view, comes within
    /// `seconds`. The keyboard's own work may lay it out for a while; a
    /// re-entered fit that queued a layout for every pass never stops.
    private func settles(within seconds: Double = 5) throws -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        repeat { if try passes({ spin(0.3) }) == 0 { return true } } while Date() < deadline
        return false
    }

    /// One turn of the main queue, then a layout: what a fit queued with
    /// `DispatchQueue.main.async` before this has run.
    private func turn(_ view: UIView) {
        let turned = expectation(description: "a turn of the main queue")
        DispatchQueue.main.async { turned.fulfill() }
        wait(for: [turned], timeout: 5)
        view.layoutIfNeeded()
    }

    /// How many times UIKit laid an ExactView out while `body` ran.
    private func passes(_ body: () throws -> Void) throws -> Int {
        let original = try XCTUnwrap(class_getInstanceMethod(ExactView.self, #selector(UIView.layoutSubviews)))
        let counted = try XCTUnwrap(class_getInstanceMethod(ExactView.self, #selector(UIView.countedExactLayout)))
        method_exchangeImplementations(original, counted)
        defer { method_exchangeImplementations(original, counted) }
        Self.passes = 0
        try body()
        return Self.passes
    }

    /// A booted session's view in a scene window, with a prompt of the
    /// session's outside the view: it edits for the session whether or not
    /// the view is in the window, and focusing it adds no guide by itself.
    private func fixture(_ label: String) throws -> (ExactSession, ExactView, UIViewController, UITextField) {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let session = ExactApp.shared.makeSession(label: label)
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        host.view.addSubview(view)
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_FIXTURE_PLAN"])
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        view.fit()
        let prompt = UITextField(frame: CGRect(x: 20, y: 80, width: 180, height: 40))
        host.view.addSubview(prompt)
        session.presenter.keyboardPrompt = prompt
        addTeardownBlock { window.endEditing(true); window.isHidden = true; session.destroy() }
        return (session, view, host, prompt)
    }

    /// A fit called while an editor has the keys and the view has no guide yet.
    func testAFitThatAddsTheKeyboardGuideSettles() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the keyboard guide is a scene window's") }
        let (session, view, _, prompt) = try fixture("keyboard-probe-fit")
        XCTAssertTrue(prompt.becomeFirstResponder())
        XCTAssertTrue(session.presenter.hasKeyboardEditor)
        XCTAssertFalse(view.hasKeyboardGuide, "nothing has added the guide before the fit")
        let inFit = try passes { view.fit() }
        XCTAssertTrue(view.hasKeyboardGuide)
        XCTAssertLessThanOrEqual(inFit, 3, "the guide's layout inside the fit ends")
        let next = try passes { turn(view) }
        XCTAssertTrue((1...3).contains(next), "the re-entered fit queued one layout, which the next turn runs: \(next) passes")
        XCTAssertTrue(try settles(), "the view settles")
    }

    /// The view moved into a window while its session's editor has the keys:
    /// `didMoveToWindow` fits, and that fit adds the guide.
    func testAViewMovedIntoAWindowWhileEditingSettles() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the keyboard guide is a scene window's") }
        let (session, view, host, prompt) = try fixture("keyboard-probe-window")
        view.removeFromSuperview()
        XCTAssertTrue(prompt.becomeFirstResponder())
        XCTAssertTrue(session.presenter.hasKeyboardEditor)
        XCTAssertFalse(view.hasKeyboardGuide, "a view outside a window adds no guide")
        let moved = try passes { host.view.addSubview(view) }
        XCTAssertTrue(view.hasKeyboardGuide)
        XCTAssertLessThanOrEqual(moved, 3, "the guide's layout inside the move ends")
        XCTAssertTrue(try settles(), "the view settles")
    }

    /// A push whose route's search takes the keys as it arrives settles.
    /// The transition's end fits the view (`NavigationHost.completeTransition`),
    /// and that fit can be the one that adds the guide, as when the keyboard's
    /// notification comes after it: with 3e8ed35b7's guard this test hung.
    func testAPushEndingOnAnEditingRouteSettles() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "native search editing needs the hosted app responder lifecycle") }
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_PUSHED_TITLE_PLAN"], "build.mjs --test --ios compiles the pushed-title fixture")
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        ExactGroupedLists.install()
        let session = ExactApp.shared.makeSession(label: "keyboard-probe-push")
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        addTeardownBlock { window.endEditing(true); window.isHidden = true; session.destroy() }
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: CGSize(width: 402, height: 874)).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        let p = session.presenter
        let nav = try XCTUnwrap(p.navigation.tabController?.selectedViewController as? UINavigationController)
        let rests = { nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !p.navigation.transitioning }
        until("Home shown") { nav.topViewController?.viewIfLoaded?.window != nil }
        XCTAssertFalse(view.hasKeyboardGuide)
        let open = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "home-open-seek" })
        p.press(open.id)
        until("the pushed route rests with its search editing") {
            rests() && nav.topViewController?.navigationItem.searchController?.searchBar.searchTextField.isFirstResponder == true
        }
        XCTAssertTrue(view.hasKeyboardGuide)
        XCTAssertTrue(try settles(), "the view settles")
    }
}

extension UIView {
    /// `KeyboardProbeIOSTests.passes`: exchanged with ExactView's `layoutSubviews`.
    @objc fileprivate func countedExactLayout() {
        KeyboardProbeIOSTests.passes += 1
        countedExactLayout()
    }
}
#endif
