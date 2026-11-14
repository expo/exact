#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class RouteOverflowMacTests: XCTestCase {
    func testWindowOverflowAfterFirstPixelAndAfterAResize() throws {
        _ = NSApplication.shared
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_ROUTE_OVERFLOW_PLAN"])
        let session = ExactApp.shared.makeSession(label: "route-overflow")
        defer { session.destroy() }
        let view = ExactView(session: session)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400),
                              styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        window.contentView = view
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        func warnings() -> Int {
            session.runtime.agent("{\"op\":\"logs\"}").components(separatedBy: "route-overflow").count - 1
        }
        func press(_ name: String) throws {
            let button = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
            session.presenter.press(button.id)
        }
        XCTAssertEqual(warnings(), 0)
        window.makeKeyAndOrderFront(nil)
        view.layoutSubtreeIfNeeded()
        waitForActivation(session)
        waitForMainQueue()
        try press("toggle-home")
        waitForMainQueue()
        let p = session.presenter
        XCTAssertGreaterThan(p.root.frame.height, p.viewportSize.height)
        p.viewport.contentView.scroll(to: NSPoint(x: 0, y: p.root.frame.height - p.viewportSize.height))
        p.viewport.reflectScrolledClipView(p.viewport.contentView)
        XCTAssertGreaterThan(p.viewport.contentView.bounds.minY, 0, "visible overflow is reachable by the native document scroll")
        XCTAssertEqual(warnings(), 0)
        try press("clip-home")
        waitFor("clipped route overflow journals") { warnings() == 1 }
        XCTAssertEqual(p.root.frame.height, p.viewportSize.height, accuracy: 0.5)
        XCTAssertEqual(session.screenSize, session.presenter.viewportSize)
        try press("toggle-home")
        waitForMainQueue()
        window.setContentSize(NSSize(width: 500, height: 500))
        view.layoutSubtreeIfNeeded()
        waitFor("the window offer follows AppKit") { session.screenSize == session.presenter.viewportSize && session.screenSize?.width == 500 }
        try press("toggle-home")
        waitFor("fitting rearms the journal after resize") { warnings() == 2 }
    }
}
#endif
