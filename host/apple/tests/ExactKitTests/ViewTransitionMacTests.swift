#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// `startViewTransition()` on the Mac (LLP 1013.003): the action's commit
/// fades the window's content, and a route the commit pushes or pops plays
/// no entrance or exit of its own.
final class ViewTransitionMacTests: XCTestCase {
    func testACommitFadesTheWindowsContentAndItsRoutesPlayNoPartOfTheirOwn() throws {
        _ = NSApplication.shared
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_VIEW_TRANSITION_PLAN"])
        let session = ExactApp.shared.makeSession(label: "view-transition")
        defer { session.destroy() }
        let view = ExactView(session: session)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400),
                              styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        window.contentView = view
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        window.makeKeyAndOrderFront(nil)
        view.layoutSubtreeIfNeeded()
        waitForActivation(session)
        waitForMainQueue()
        let p = session.presenter
        func named(_ name: String) -> NodeView? { p.views.values.first { $0.props["testId"] == name } }
        let layer = try XCTUnwrap(window.contentView?.layer)
        // Core Animation keeps a transition under its own key, whatever key it is added with
        func fade() -> CATransition? { layer.animation(forKey: kCATransition) as? CATransition }

        p.press(try XCTUnwrap(named("pick")).id)
        waitFor("the window's content fades") { fade() != nil }
        XCTAssertEqual(fade()?.type, .fade)
        XCTAssertEqual(try XCTUnwrap(fade()).duration, 0.25, accuracy: 0.001)
        layer.removeAnimation(forKey: kCATransition)

        p.press(try XCTUnwrap(named("pick-and-push")).id)
        waitFor("the window's content fades with the push") { fade() != nil && named("item") != nil }
        let item = try XCTUnwrap(named("item"))
        XCTAssertNil(item.layer?.animation(forKey: "exact.route-transition"), "the pushed route's entrance does not play")
        XCTAssertNil(p.navigationHost.transitions.held, "no route is held for an entrance")
        XCTAssertFalse(item.routeInert, "the pushed route takes input at once")
        layer.removeAnimation(forKey: kCATransition)

        p.press(try XCTUnwrap(named("pick-and-pop")).id)
        waitFor("the window's content fades with the pop") { fade() != nil && named("item") == nil }
        XCTAssertNil(p.navigationHost.transitions.exiting, "the popped route's exit does not play")
        XCTAssertNil(p.leaving[item.id], "its view is not kept for one")
    }
}
#endif
