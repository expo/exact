import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

#if os(macOS) || os(iOS)
/// `swipeleft`, the mirror of `swiperight`, and a scroller's `scrolldragstart`
/// and `scrolldragend` (Charlie, 2026-11-10), through a booted session:
/// each action runs once, toward the side the node hears, and the drag's
/// release carries UIKit's velocity (points per millisecond, positive as the
/// offset grows) and, on iOS, its rest in `scrollTop`'s coordinates.
final class SwipeScrollDragTests: XCTestCase {
    #if os(macOS)
    private var window: NSWindow?
    #else
    private var window: UIWindow?
    #endif
    private var session: ExactSession?
    override func tearDown() {
        session?.destroy(); session = nil
        #if os(macOS)
        window?.close()
        #else
        window?.isHidden = true
        #endif
        window = nil
        super.tearDown()
    }

    private func booted() throws -> ExactSession {
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_SWIPE_SCROLL_DRAG_PLAN"], "build.mjs --test compiles tests/fixtures/swipe-scroll-drag.contract")))
        let session = ExactApp.shared.makeSession(label: "swipe-scroll-drag")
        self.session = session
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 700), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = session.presenter.viewport
        #else
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 700))
        session.presenter.viewport.frame = window.bounds
        window.addSubview(session.presenter.viewport)
        window.makeKeyAndVisible()
        #endif
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 400, height: 700)).error)
        return session
    }
    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }
    private func text(_ session: ExactSession, _ testId: String) -> String {
        session.presenter.views.values.first { $0.props["testId"] == testId }?.props["text"] ?? ""
    }
    private func until(_ session: ExactSession, _ testId: String, _ value: String, _ message: String) {
        let end = Date().addingTimeInterval(10)
        while text(session, testId) != value, Date() < end { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertEqual(text(session, testId), value, message)
    }
    private func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }
    /// A swipe on `testId`'s node: the hold, one move, then the release.
    private func swipe(_ session: ExactSession, _ testId: String, by dx: Double) throws {
        let hold = try XCTUnwrap(SwipeHold(try node(session, testId)), "\(testId) begins a swipe")
        XCTAssertTrue(hold.move(dx))
        hold.finish(displacement: dx, fingerVelocity: nil, cancel: false)
        spin(0.6) // its return, so the next swipe starts from rest
    }

    func testSwipeleftRunsOnceTowardTheSideTheNodeHears() throws {
        let session = try booted()
        until(session, "swipes", "0:0", "booted")
        try swipe(session, "left", by: -100)
        until(session, "swipes", "1:0", "a left swipe past the knee runs `swipeleft` once")
        try swipe(session, "left", by: -40)
        try swipe(session, "right", by: -100)
        try swipe(session, "left", by: 100)
        XCTAssertEqual(text(session, "swipes"), "1:0", "short of the knee, or toward a side the node does not hear, runs nothing")
        try swipe(session, "right", by: 100)
        until(session, "swipes", "1:1", "a right swipe is unchanged")
    }

    func testAScrollDragReportsItsStartAndItsRelease() throws {
        let session = try booted()
        until(session, "drags", "0:0:0:none", "booted")
        let port = try node(session, "port")
        #if os(iOS)
        let sv = try XCTUnwrap(port.scrollView)
        port.scrollViewWillBeginDragging(sv)
        until(session, "drags", "1:0:0:none", "the drag began")
        var rest = CGPoint(x: 0, y: 300)
        port.scrollViewWillEndDragging(sv, withVelocity: CGPoint(x: 0, y: 1.5), targetContentOffset: &rest)
        let top = Double(rest.y + port.scrollTopInset(sv))
        until(session, "drags", "1:1:1.5:\(top == top.rounded() ? String(Int(top)) : String(top))", "UIKit's velocity, and its rest as scrollTop")
        #else
        let sv = try XCTUnwrap(port.scroll)
        // A trackpad's fingers, as `scrollWheel(with:)` notes them before
        // routing (AppKit's own tracking of a synthesized gesture blocks
        // outside a running app): CGScrollPhase began 1, changed 2, ended 4.
        // A negative delta moves the content up, so the offset grows.
        func finger(_ dy: Int32, phase: Int64, ms: Double) throws {
            let cg = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2, wheel1: dy, wheel2: 0, wheel3: 0))
            cg.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
            if phase != 0 { cg.setIntegerValueField(.scrollWheelEventScrollPhase, value: phase) }
            cg.timestamp = CGEventTimestamp(ms * 1_000_000)
            let event = try XCTUnwrap(NSEvent(cgEvent: cg))
            sv.noteDrag(event, delta: CGPoint(x: event.scrollingDeltaX, y: event.scrollingDeltaY), taken: true)
        }
        try finger(-30, phase: 0, ms: 900)
        spin(0.05)
        XCTAssertEqual(text(session, "drags"), "0:0:0:none", "a wheel without phases is no drag")
        try finger(0, phase: 1, ms: 1000)
        for i in 1...4 { try finger(-10, phase: 2, ms: 1000 + Double(i) * 10) }
        until(session, "drags", "1:0:0:none", "the drag began with its first move")
        try finger(0, phase: 4, ms: 1050)
        let end = Date().addingTimeInterval(10)
        while !text(session, "drags").hasPrefix("1:1:"), Date() < end { spin(0.02) }
        let fields = text(session, "drags").split(separator: ":")
        XCTAssertEqual(fields.count, 4, text(session, "drags"))
        XCTAssertEqual(Double(fields[2]) ?? 0, 1, accuracy: 1e-6, "a point a millisecond, the offset growing")
        XCTAssertEqual(fields.last, "none", "no rest AppKit tells")
        #endif
    }
}
#endif

#if os(macOS)
final class ScrollDragTrackTests: XCTestCase {
    func testTheVelocityIsTheLastTenthOfASecondsTravel() {
        var track = ScrollDragTrack()
        track.moved(at: 0, by: CGPoint(x: 0, y: 50))
        for i in 1...3 { track.moved(at: 0.2 + Double(i) * 0.01, by: CGPoint(x: -4, y: 10)) }
        let v = track.velocity(at: 0.24)
        XCTAssertEqual(v.y, 1, accuracy: 1e-9, "20 points over 20 ms; the move 200 ms before is out")
        XCTAssertEqual(v.x, -0.4, accuracy: 1e-9)
        XCTAssertEqual(track.velocity(at: 0.5), .zero, "fingers that rested before the lift throw nothing")
    }
}
#endif
