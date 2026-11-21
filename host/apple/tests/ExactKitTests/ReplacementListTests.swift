#if os(macOS) || os(iOS)
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import XCTest
@testable import ExactKit

final class ReplacementListTests: XCTestCase {
    func testRemovingTheReadersRowKeepsItsSuccessorAtTheNativeOffset() throws {
        let size = CGSize(width: 430, height: 500)
        let rect = CGRect(origin: .zero, size: size)
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "replace-feed")
        #if os(macOS)
        let window = NSWindow(contentRect: rect, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        window.contentView = ExactView(session: session)
        #else
        let window = UIWindow(frame: rect)
        defer { window.isHidden = true }
        let view = ExactView(session: session)
        view.frame = rect
        window.addSubview(view)
        window.makeKeyAndVisible()
        #endif
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("QUIET_SCROLL"), size: size).error)
        let presenter = session.presenter
        func node(_ testId: String) -> NodeView? {
            presenter.views.values.first { $0.props["testId"] == testId }
        }
        func offset() -> CGFloat? {
            #if os(macOS)
            node("feed")?.scroll?.contentView.bounds.minY
            #else
            node("feed")?.scrollView?.contentOffset.y
            #endif
        }
        presenter.settlePump()
        let feed = try XCTUnwrap(node("feed"))
        let point = CGPoint(x: 0, y: 1807)
        #if os(macOS)
        try XCTUnwrap(feed.scroll).contentView.scroll(to: point)
        #else
        try XCTUnwrap(feed.scrollView).setContentOffset(point, animated: false)
        #endif
        waitFor("row 18 in the scrolled window") {
            presenter.settlePump()
            return node("coin-18") != nil && offset() == point.y && !session.fillInFlight
        }
        XCTAssertTrue(session.apply(try fixturePlan("FILTERED_FEED"), label: "generation"))
        // The odd keys remain. Row 19 is now the tenth 100-point row.
        waitFor("row 19 seven points above the port", facts: { "offset=\(String(describing: offset()))" }) {
            presenter.settlePump()
            return node("coin-18") == nil && node("coin-19") != nil
                && offset().map { abs($0 - 907) <= 1 } == true && !session.fillInFlight
        }
        let prepend = try XCTUnwrap(node("prepend")).id
        presenter.press(prepend) // -100 is filtered out.
        presenter.press(prepend) // -101 goes above row 19.
        waitFor("row 19 stays put as new output arrives", facts: { "offset=\(String(describing: offset()))" }) {
            presenter.settlePump()
            return node("coin-19") != nil && offset().map { abs($0 - 1007) <= 1 } == true && !session.fillInFlight
        }
    }
}
#endif
