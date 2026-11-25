#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// `swipe` (LLP 1057.001 §8): UIKit's `UISwipeGestureRecognizer` in the
/// node's direction, through a booted session.
final class DiscreteSwipeTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?
    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func booted() throws -> ExactSession {
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_DISCRETE_SWIPE_PLAN"], "build.mjs --test compiles tests/fixtures/discrete-swipe.contract")))
        let session = ExactApp.shared.makeSession(label: "discrete-swipe")
        self.session = session
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 700))
        session.presenter.viewport.frame = window.bounds
        window.addSubview(session.presenter.viewport)
        window.makeKeyAndVisible()
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 400, height: 700)).error)
        until(session, "swipes", "0", "booted")
        return session
    }
    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }
    private func until(_ session: ExactSession, _ testId: String, _ value: String, _ message: String) {
        waitFor(message) { session.presenter.views.values.first { $0.props["testId"] == testId }?.props["text"] == value }
    }

    /// The node's view keeps one recognizer in its direction with no
    /// delegate, makes a new one when the direction changes and removes it
    /// when a computed direction names none
    func testTheNodesViewKeepsOneRecognizerInItsDirection() throws {
        let session = try booted()
        let bar = try node(session, "bar")
        let first = try XCTUnwrap(bar.discreteSwipe, "a `swipe` with a direction has a recognizer")
        XCTAssertEqual(first.direction, .down)
        XCTAssertTrue(first.view === bar)
        XCTAssertNil(first.delegate, "UIKit arbitrates it: no delegate refuses a field's touches")
        XCTAssertEqual(bar.gestureRecognizers?.filter { $0 is UISwipeGestureRecognizer }.count, 1)
        XCTAssertTrue(bar.gestureRecognizerShouldBegin(first))
        session.presenter.press(try node(session, "turn").id)
        waitFor("a new direction makes a new recognizer") { bar.discreteSwipe !== first }
        let second = try XCTUnwrap(bar.discreteSwipe)
        XCTAssertEqual(second.direction, .up)
        XCTAssertNil(first.view, "the old recognizer is removed, so a pending contact's swipe fails")
        session.presenter.press(try node(session, "turn").id)
        waitFor("a computed direction that names none leaves the swipe off") { bar.discreteSwipe == nil }
        XCTAssertNil(second.view)
    }

    func testRecognitionRunsTheActionAndANodeInsideAnInertOneDoesNotBegin() throws {
        let session = try booted()
        try node(session, "bar").discreteSwiped()
        until(session, "swipes", "1", "a recognized swipe runs the node's action")
        let inside = try node(session, "inside-inert"), swipe = try XCTUnwrap(inside.discreteSwipe)
        XCTAssertFalse(inside.gestureRecognizerShouldBegin(swipe), "a node inside an inert one takes no swipe")
    }

    /// The field sized to its text by the kernel's layout fits its box as
    /// TextKit lays it out, so its pan does not begin and a flick on it is
    /// the bar's (`TextArea.fits`)
    func testTheFieldSizedToItsTextDoesNotTakeAFlick() throws {
        let session = try booted()
        let text = try XCTUnwrap(try node(session, "field").textArea as? TextArea)
        XCTAssertTrue(text.fits, "laid out at \(text.bounds.size), content \(text.contentSize)")
        XCTAssertFalse(text.gestureRecognizerShouldBegin(text.panGestureRecognizer))
    }
}
#endif
