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

    func testSetBoundsLaysOutEveryStepAndAppKitSavesTheFrame() throws {
        let (session, _, window) = try page(NSSize(width: 420, height: 270))
        defer { window.close(); session.destroy() }
        window.center()
        let name = "exact-bounds-" + UUID().uuidString
        defer { NSWindow.removeFrame(usingName: name) }
        XCTAssertTrue(window.setFrameAutosaveName(name))
        let compact = window.frame, placement = window.placement
        let restored = NSWindow(contentRect: compact, styleMask: window.styleMask, backing: .buffered, defer: false)
        restored.isReleasedWhenClosed = false
        defer { restored.close() }
        let top = try XCTUnwrap(NSScreen.screens.first).frame.maxY
        var steps = 0
        let observer = NotificationCenter.default.addObserver(forName: NSWindow.didResizeNotification, object: window, queue: nil) { _ in
            steps += 1
            XCTAssertEqual(session.presenter.root.subviews.first?.frame.size, session.presenter.viewport.contentView.bounds.size)
        }
        defer { NotificationCenter.default.removeObserver(observer) }
        for frame in [compact.insetBy(dx: -70, dy: -60), compact] {
            let before = window.frame
            steps = 0
            placement.setBounds([[frame.minX, top - frame.maxY, frame.width, frame.height], true], on: window) { XCTFail($0) }
            XCTAssertEqual(before, window.frame)
            drain(5) { window.frame == frame }
            XCTAssertEqual(window.frame, frame)
            if ExactEnv.agentFreezes || NSWorkspace.shared.accessibilityDisplayShouldReduceMotion { XCTAssertEqual(steps, 1) }
            else { XCTAssertGreaterThan(steps, 1) }
            window.saveFrame(usingName: name)
            XCTAssertTrue(restored.setFrameUsingName(name))
            XCTAssertEqual(restored.frame, frame)
        }
        placement.setBounds([[compact.minX, top - compact.maxY, compact.width, compact.height]], on: window) { XCTFail($0) }
        waitForMainQueue()
        window.setFrameOrigin(NSPoint(x: compact.minX + 24, y: compact.minY))
        window.saveFrame(usingName: name)
        XCTAssertEqual(window.frameAutosaveName, name)
        XCTAssertTrue(restored.setFrameUsingName(name))
        XCTAssertEqual(restored.frame, window.frame)
        XCTAssertNotEqual(restored.frame, compact)
    }

    func testBoundsRequestedDuringAnotherBoundsAnimationWins() throws {
        let (session, _, window) = try page(NSSize(width: 420, height: 270))
        defer { window.close(); session.destroy() }
        let start = window.frame, top = try XCTUnwrap(NSScreen.screens.first).frame.maxY
        let finish = start.insetBy(dx: -10, dy: -5)
        func set(_ r: NSRect, _ animate: Bool) {
            window.placement.setBounds([[r.minX, top - r.maxY, r.width, r.height], animate], on: window) { XCTFail($0) }
        }
        let native = start.insetBy(dx: -35, dy: -8)
        for nativeLast in [true, false] {
            window.setFrame(start, display: true)
            Owner.shared.sync({ Owner.shared.callMain {
                if nativeLast { set(finish, false) }
                window.setFrame(native, display: true, animate: true)
                if !nativeLast { set(finish, false) }
            } }, busy: ())
            waitForMainQueue()
            XCTAssertEqual(window.frame, nativeLast ? native : finish)
        }
        window.setFrame(start, display: true)
        set(finish, false)
        window.setFrame(start.insetBy(dx: -24, dy: -14), display: true)
        window.setFrame(start, display: true)
        waitForMainQueue()
        XCTAssertEqual(window.frame, start, "a later pair of changes can return to the starting frame")
        for (queueAnother, intervene) in [(true, false), (true, true), (false, true)] {
            window.setFrame(start, display: true)
            var requested = false
            let other = start.insetBy(dx: -24, dy: -14)
            let observer = NotificationCenter.default.addObserver(forName: NSWindow.didResizeNotification, object: window, queue: nil) { _ in
                guard !requested else { return }
                requested = true
                if queueAnother { set(finish, false) }
                if intervene { window.setFrame(other, display: true) }
            }
            set(start.insetBy(dx: -80, dy: -40), true)
            let expected = intervene ? other : finish
            drain(5) { window.frame == expected }
            let settled = expectation(description: "queued frame applied or superseded")
            DispatchQueue.main.async { settled.fulfill() }
            wait(for: [settled], timeout: 2)
            NotificationCenter.default.removeObserver(observer)
            XCTAssertTrue(requested)
            XCTAssertEqual(window.frame, expected)
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
