#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class RouteOverflowMacTests: XCTestCase {
    func testScaledRouteKeepsItsOriginalBorderAndExtendsEachAxis() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 120, height: 120)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 100, "h": 100],
            ["op": "content", "id": 1, "w": 100, "h": 100],
            ["op": "present", "id": 1, "property": "scale", "x": 3, "y": 0.5]
        ]))
        let route = try XCTUnwrap(p.views[1])
        let reach = route.documentOverflow(in: p.root)
        XCTAssertEqual(reach.minX, -100, accuracy: 0.01)
        XCTAssertEqual(reach.maxX, 200, accuracy: 0.01)
        XCTAssertEqual(reach.minY, 0, accuracy: 0.01)
        XCTAssertEqual(reach.maxY, 100, accuracy: 0.01)
    }

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
        let pumps = session.replyPumpsForTests
        try press("toggle-home")
        waitForMainQueue()
        window.setContentSize(NSSize(width: 500, height: 500))
        view.layoutSubtreeIfNeeded()
        waitFor("the window offer follows AppKit") { session.screenSize == session.presenter.viewportSize && session.screenSize?.width == 500 }
        // The check is woken at most once every 100 ms (a layout that changes every frame would wake it every frame),
        // so a fit shorter than that is not observed: it coalesces with the overflow after it. Let the check see the
        // fit (a reply pump lands after it) before the route overflows again.
        waitFor("the check sees the fit") { session.replyPumpsForTests > pumps && session.repliesInFlight == 0 }
        try press("toggle-home")
        waitFor("fitting rearms the journal after resize") { warnings() == 2 }
    }
}
#endif
