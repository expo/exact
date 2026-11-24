#if os(macOS)
import AppKit
import XCTest
import ObjectiveC
@testable import ExactKit
import ExactCore

/// A window's frame changed in steps: an animation a module's call asks for
/// (expo/exact#1412), and a person's live resize, each step a frame change.
final class WindowFrameMacTests: XCTestCase {
    @objc private class func legacyScrollers() -> NSScroller.Style { .legacy }

    /// A session on a page whose root fills and clips the window, in Exact's window.
    private func page(_ size: NSSize) throws -> (ExactSession, ExactView, ExactSessionWindow) {
        _ = NSApplication.shared
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        component App
          view
            main width="100%" height="100%" overflow="hidden"
              text "corner"

        """
        let contract = dir.appendingPathComponent("app.contract"), plan = dir.appendingPathComponent("app.plan")
        try source.write(to: contract, atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", contract.path, "-o", plan.path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        let session = ExactApp.shared.makeSession(label: "window-frame")
        XCTAssertNil(session.boot(plan: try Data(contentsOf: plan), size: size).error)
        let view = ExactView(session: session)
        let window = ExactSessionWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        window.orderFrontRegardless() // AppKit animates only a window on screen
        view.layoutSubtreeIfNeeded()
        return (session, view, window)
    }

    func testAnAnimatedResizeAskedInsideACallLaysOutEachStep() throws {
        let (session, _, window) = try page(NSSize(width: 600, height: 400))
        defer { window.close(); session.destroy() }
        let p = session.presenter
        // The viewport and the root's laid-out size at each step.
        var steps: [(viewport: NSSize, root: NSSize)] = []
        let observer = NotificationCenter.default.addObserver(forName: NSWindow.didResizeNotification, object: window, queue: nil) { _ in
            steps.append((p.viewport.contentView.bounds.size, p.root.subviews.first?.frame.size ?? .zero))
        }
        defer { NotificationCenter.default.removeObserver(observer) }
        for change: CGFloat in [200, -200] {
            steps = []
            var target = window.frame
            target.size.width += change
            target.size.height += change
            let before = window.frame
            Owner.shared.sync({ Owner.shared.callMain { window.setFrame(target, display: true, animate: true) } }, busy: ())
            XCTAssertEqual(window.frame, before, "the animation runs after the call")
            drain(5) { window.frame == target }
            XCTAssertEqual(window.frame, target)
            XCTAssertFalse(steps.isEmpty, "the frame changed")
            for (viewport, root) in steps { XCTAssertEqual(root, viewport, "each step is laid out at its size") }
        }
    }

    /// A frame change made after the animation was asked for, in the call or
    /// before its turn, is the window's frame: the animation does not undo it.
    func testAFrameChangeAfterADeferredAnimationWins() throws {
        let (session, _, window) = try page(NSSize(width: 600, height: 400))
        defer { window.close(); session.destroy() }
        let start = window.frame
        var animated = start, set = start
        animated.size.width -= 100
        set.size.width += 100
        Owner.shared.sync({ Owner.shared.callMain {
            window.setFrame(animated, display: true, animate: true)
            window.setFrame(set, display: true)
        } }, busy: ())
        drain(0.5)
        XCTAssertEqual(window.frame, set)
        for change in [{ window.setContentSize(NSSize(width: 500, height: 300)) }, { window.setFrameOrigin(NSPoint(x: 40, y: 60)) }] {
            Owner.shared.sync({ Owner.shared.callMain { window.setFrame(start, display: true, animate: true) } }, busy: ())
            change()
            let changed = window.frame
            drain(0.5)
            XCTAssertEqual(window.frame, changed)
        }
    }

    /// A window not on screen takes its frame inside the call.
    func testAHiddenWindowTakesItsFrameAtOnce() throws {
        let (session, _, window) = try page(NSSize(width: 600, height: 400))
        defer { window.close(); session.destroy() }
        window.orderOut(nil)
        var target = window.frame
        target.size.width += 100
        Owner.shared.sync({ Owner.shared.callMain { window.setFrame(target, display: true, animate: true) } }, busy: ())
        XCTAssertEqual(window.frame, target)
    }

    /// Runs the main run loop for `seconds`, or until `done`.
    private func drain(_ seconds: TimeInterval, until done: () -> Bool = { false }) {
        let end = Date().addingTimeInterval(seconds)
        while !done(), Date() < end { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
    }

    /// A person's live resize shrinking the window, each step laid out as it
    /// is made: no step shows a scroller, and no step's layout is told a
    /// viewport one narrowed.
    func testShrinkStepsOfAClippedRootOfferNoGutter() throws {
        // Scrollers that take room, whatever this Mac's setting.
        let original = try XCTUnwrap(class_getClassMethod(NSScroller.self, #selector(getter: NSScroller.preferredScrollerStyle)))
        let replacement = try XCTUnwrap(class_getClassMethod(Self.self, #selector(Self.legacyScrollers)))
        method_exchangeImplementations(original, replacement)
        defer { method_exchangeImplementations(original, replacement) }
        let (session, view, window) = try page(NSSize(width: 600, height: 400))
        defer { window.close(); session.destroy() }
        let p = session.presenter
        var told: [NSSize] = []
        let fit = p.viewport.visibleSizeChanged
        p.viewport.visibleSizeChanged = { told.append(p.viewportSize); fit?() }
        let sizes = (1...6).map { NSSize(width: 600 - CGFloat($0) * 9, height: 400 - CGFloat($0) * 7) }
        for size in sizes {
            window.setContentSize(size)
            view.layoutSubtreeIfNeeded()
            XCTAssertEqual(p.viewportSize, size)
            XCTAssertTrue(p.viewport.verticalScroller!.isHidden)
            XCTAssertTrue(p.viewport.horizontalScroller!.isHidden)
        }
        XCTAssertFalse(told.isEmpty)
        XCTAssertTrue(told.allSatisfy(sizes.contains), "no step is told a viewport a gutter narrowed: \(told)")
    }
}
#endif
