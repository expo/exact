#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// `window-controls-overlay` (LLP 1039 D8): the page draws its own title row
/// under the window's empty unified toolbar, and a click on a button in that
/// row presses the button, as one below the title bar does.
final class TitleRowTapMacTests: XCTestCase {
    private final class DragWindow: NSWindow {
        var drags = 0
        override func performDrag(with event: NSEvent) { drags += 1 }
    }

    private static let plan: Result<Data, Error> = Result {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        component App
          state top = 0
          state below = 0
          state doubles = 0
          action pressTop
            top = top + 1
          action pressBelow
            below = below + 1
          action double
            doubles = doubles + 1
          view
            main width="100%" height="100%" display="flex" flex-direction="column"
              row margin-left="env(titlebar-area-x, 0px)" height="env(titlebar-area-height, 52px)" align-items="center" justify-content="space-between"
                button press=pressTop testId="left"
                  text `left ${top}`
                box dblclick=double testId="listener" width=120 height=30 align-items="center" justify-content="center"
                  box testId="inside" width=40 height=20
                box app-region="no-drag" testId="fixed" width=24 height=22
                button press=pressTop testId="right"
                  text `right ${top}`
              button press=pressBelow testId="below"
                text `below ${below}`
              row app-region="drag" height=40
                box dblclick=double testId="drag-child" width=40 height=40
                button app-region="no-drag" press=pressBelow testId="no-drag"
                  text "Press"

        """
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    /// The window ExactMac's `coverChrome` makes for the overlay, showing
    /// the plan; the caller closes it and destroys the session.
    private func mount() throws -> (NSWindow, ExactSession) {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "title-row-tap")
        let window = DragWindow(contentRect: NSRect(x: 100, y: 100, width: 900, height: 600),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.titlebarSeparatorStyle = .none
        window.toolbar = NSToolbar(identifier: "title-row-tap")
        window.toolbarStyle = .unified
        let view = ExactView(session: session)
        view.controlsOverlay = true
        window.contentView = view
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 900, height: 600)).error)
        window.orderFront(nil)
        view.layoutSubtreeIfNeeded()
        view.syncInsets()
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        return (window, session)
    }

    func testAButtonInTheTitleRowTakesTheAgentsTapAndTheWindowsClick() throws {
        let (window, session) = try mount()
        defer { window.close(); session.destroy() }

        let close = try XCTUnwrap(window.standardWindowButton(.closeButton))
        let band = try XCTUnwrap(close.superview).frame.height
        XCTAssertGreaterThan(band, 32, "the unified toolbar's tall row")
        func node(_ id: String) throws -> NodeView { try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == id }) }
        func slot(_ name: String) -> String {
            let json = try? JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
            return ((json?["slots"] as? [String: Any])?[name]).map { "\($0)" } ?? ""
        }
        func tap(_ id: String) throws {
            let reply = session.agentInstance.tap(["op": "tap", "id": Int(try node(id).id)])
            XCTAssertNil(reply["error"], "\(id): \(reply)")
            XCTAssertEqual(reply["delivery"] as? String, "platform", "a click through the application, not an activation")
        }

        // Each title-row button lies under the title bar, and the window's
        // own hit test, which routes a person's click too, finds the page there
        let frame = try XCTUnwrap(window.contentView?.superview)
        for id in ["left", "right"] {
            let button = try node(id)
            let middle = button.convert(NSPoint(x: button.bounds.midX, y: button.bounds.midY), to: nil)
            XCTAssertGreaterThan(middle.y, window.contentLayoutRect.maxY, "\(id) is in the title bar's band")
            let hit = try XCTUnwrap(frame.hitTest(middle), id)
            XCTAssertTrue(hit === button || hit.isDescendant(of: button), "\(id): the window hits \(hit), not the page")
        }

        try tap("left")
        try tap("right")
        try tap("below")
        XCTAssertEqual(slot("top"), "2", "both title-row taps pressed")
        XCTAssertEqual(slot("below"), "1")

        // A click the application receives, as a person's arrives, outside
        // the agent's tap. A native button tracks the click inside its
        // `mouseDown`, taking the release from the queue, so the release is
        // queued first. (A test process is never active, so its window is
        // not key; the hit test above is the same for a key window.)
        let right = try node("right")
        let p = right.convert(NSPoint(x: right.bounds.midX, y: right.bounds.midY), to: nil)
        let t = ProcessInfo.processInfo.systemUptime
        func click(_ type: NSEvent.EventType) throws -> NSEvent {
            try XCTUnwrap(NSEvent.mouseEvent(with: type, location: p, modifierFlags: [], timestamp: t, windowNumber: window.windowNumber,
                                             context: nil, eventNumber: 1, clickCount: 1, pressure: type == .leftMouseDown ? 1 : 0))
        }
        NSApp.postEvent(try click(.leftMouseUp), atStart: true)
        NSApp.sendEvent(try click(.leftMouseDown))
        if let up = NSApp.nextEvent(matching: .leftMouseUp, until: Date(), inMode: .default, dequeue: true) { NSApp.sendEvent(up) }
        XCTAssertEqual(slot("top"), "3", "the application's click pressed")
    }

    func testADragRegionIncludesChildrenAndANoDragButtonKeepsItsPress() throws {
        let (window, session) = try mount()
        defer { window.close(); session.destroy() }
        let dragWindow = try XCTUnwrap(window as? DragWindow)
        for id in ["drag-child", "no-drag"] {
            let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == id })
            let point = node.convert(NSPoint(x: node.bounds.midX, y: node.bounds.midY), to: nil)
            let event = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: point, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, eventNumber: 1, clickCount: 1, pressure: 1))
            XCTAssertEqual(TitleBarAreaMac.press(event, on: node, titleGround: false), id == "drag-child")
        }
        XCTAssertEqual(dragWindow.drags, 1)
    }

    /// A double click on the empty part of the title row does what one on
    /// the title bar does by the system setting; one on a button in the row
    /// or on a `dblclick` listener leaves the window as it was.
    func testADoubleClickOnTheTitleRowsGroundActsAsTheTitleBars() throws {
        let (window, session) = try mount()
        defer { window.close(); session.destroy() }
        func node(_ id: String) throws -> NodeView { try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == id }) }
        func event(_ type: NSEvent.EventType, at p: NSPoint, clicks: Int) throws -> NSEvent {
            try XCTUnwrap(NSEvent.mouseEvent(with: type, location: p, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                             windowNumber: window.windowNumber, context: nil, eventNumber: clicks,
                                             clickCount: clicks, pressure: type == .leftMouseDown ? 1 : 0))
        }
        // Each mouseUp is queued before its mouseDown is sent, as the single click above does: a button's
        // tracking loop waits in mouseDown for the up, which nothing would post once it is waiting.
        func doubleClick(at p: NSPoint) throws {
            for clicks in 1...2 {
                NSApp.postEvent(try event(.leftMouseUp, at: p, clicks: clicks), atStart: true)
                NSApp.sendEvent(try event(.leftMouseDown, at: p, clicks: clicks))
                if let up = NSApp.nextEvent(matching: .leftMouseUp, until: Date(), inMode: .default, dequeue: true) { NSApp.sendEvent(up) }
            }
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        }
        let frame = window.frame
        let left = try node("left"), listener = try node("listener"), right = try node("right")
        try doubleClick(at: right.convert(NSPoint(x: right.bounds.midX, y: right.bounds.midY), to: nil))
        XCTAssertEqual(window.frame, frame, "a button in the row takes its clicks")
        XCTAssertFalse(window.isMiniaturized)

        // The row's ground and a listener on it can move the window, so a mouse-down there that the
        // application sends itself never reaches the page (AppKit's title-bar tracking keeps it), while a
        // person's does. What the page decides for that second click is asked of it directly; that the
        // window server's own double click reaches it, and nothing else acts, needs real input
        // (issues/closed/20261113-mac-page-title-row-double-click-does-not-zoom.md).
        let content = try XCTUnwrap(window.contentView?.superview)
        func secondClick(at p: NSPoint) throws -> Bool {
            XCTAssertGreaterThan(p.y, window.contentLayoutRect.maxY, "in the title bar's band")
            return TitleBarAreaMac.press(try event(.leftMouseDown, at: p, clicks: 2), on: content.hitTest(p), titleGround: true)
        }
        let inside = try node("inside")
        let l = inside.convert(NSPoint(x: inside.bounds.midX, y: inside.bounds.midY), to: nil)
        XCTAssertFalse(try secondClick(at: l), "the author's dblclick has precedence over the title bar's action")
        // The first node the second mouse-down reaches, under the listener, takes the click's own release from
        // the queue, as a control's tracking does, so the window never routes it to the title bar; the release
        // runs the author's dblclick once. The listener, which the mouse-down reaches next, takes nothing: a
        // second release queued behind is still there.
        NSApp.postEvent(try event(.leftMouseUp, at: l, clicks: 2), atStart: true)
        NSApp.postEvent(try event(.leftMouseUp, at: l, clicks: 2), atStart: false)
        inside.mouseDown(with: try event(.leftMouseDown, at: l, clicks: 2))
        XCTAssertNotNil(NSApp.nextEvent(matching: .leftMouseUp, until: Date(), inMode: .default, dequeue: true), "one release taken")
        XCTAssertNil(NSApp.nextEvent(matching: .leftMouseUp, until: Date(), inMode: .default, dequeue: true))
        let state = try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
        XCTAssertEqual(((state?["slots"] as? [String: Any])?["doubles"]).map { "\($0)" }, "1", "the dblclick ran")
        XCTAssertEqual(window.frame, frame)
        XCTAssertFalse(window.isMiniaturized)

        let child = try node("drag-child")
        let p = child.convert(NSPoint(x: child.bounds.midX, y: child.bounds.midY), to: nil)
        NSApp.postEvent(try event(.leftMouseUp, at: p, clicks: 2), atStart: true)
        XCTAssertTrue(TitleBarAreaMac.press(try event(.leftMouseDown, at: p, clicks: 2), on: child, titleGround: false))
        let after = try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
        XCTAssertEqual(((after?["slots"] as? [String: Any])?["doubles"]).map { "\($0)" }, "2")
        let fixed = try node("fixed")
        let fixedPoint = fixed.convert(NSPoint(x: fixed.bounds.midX, y: fixed.bounds.midY), to: nil)
        XCTAssertTrue(TitleBarAreaMac.takesRelease(of: try event(.leftMouseDown, at: fixedPoint, clicks: 2), at: fixed))
        XCTAssertEqual(window.frame, frame)
        XCTAssertFalse(window.isMiniaturized)

        let a = left.convert(NSPoint(x: left.bounds.maxX, y: left.bounds.midY), to: nil)
        let b = listener.convert(NSPoint(x: listener.bounds.minX, y: listener.bounds.midY), to: nil)
        let ground = NSPoint(x: (a.x + b.x) / 2, y: a.y)
        let row = try XCTUnwrap(content.hitTest(ground).flatMap { sequence(first: $0, next: \.superview).lazy.compactMap { $0 as? NodeView }.first })
        XCTAssertNil(row.props["testId"], "the row's own ground")
        XCTAssertFalse(TitleBarAreaMac.takesRelease(of: try event(.leftMouseDown, at: ground, clicks: 2), at: row), "the ground leaves its release to the window")
        XCTAssertTrue(try secondClick(at: ground), "the ground acts as the title bar")
        switch TitleBarAreaMac.action(.standard) {
        case .zoom: XCTAssertNotEqual(window.frame, frame, "zoomed")
        case .minimize: XCTAssertTrue(window.isMiniaturized)
        case .none: XCTAssertEqual(window.frame, frame)
        }
    }

    /// Read from the two settings' values, not a defaults suite: a suite also reads the global domain, where this
    /// Mac's own AppleMiniaturizeOnDoubleClick would win over the test's.
    func testTheTitleBarsDoubleClickFollowsTheSystemSetting() throws {
        let action = TitleBarAreaMac.action(actionOnDoubleClick:miniaturizeOnDoubleClick:)
        XCTAssertEqual(action(nil, false), .zoom, "unset: zoom")
        XCTAssertEqual(action(nil, true), .minimize, "the older setting")
        for (value, expected) in [("Maximize", TitleBarAreaMac.Action.zoom), ("Fill", .zoom), ("Minimize", .minimize), ("None", .none)] {
            XCTAssertEqual(action(value, true), expected, value)
        }
    }
}
#endif
