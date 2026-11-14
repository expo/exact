#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// `window-controls-overlay` (LLP 1039 D8): the page draws its own title row
/// under the window's empty unified toolbar, and a click on a button in that
/// row presses the button, as one below the title bar does.
final class TitleRowTapMacTests: XCTestCase {
    private static let plan: Result<Data, Error> = Result {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        component App
          state top = 0
          state below = 0
          action pressTop
            top = top + 1
          action pressBelow
            below = below + 1
          view
            main width="100%" height="100%" display="flex" flex-direction="column"
              row margin-left="env(titlebar-area-x, 0px)" height="env(titlebar-area-height, 52px)" align-items="center" justify-content="space-between"
                button press=pressTop testId="left"
                  text `left ${top}`
                button press=pressTop testId="right"
                  text `right ${top}`
              button press=pressBelow testId="below"
                text `below ${below}`

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
        let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 900, height: 600),
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

    /// A double click on the empty part of the title row does what one on
    /// the title bar does by the system setting; one on a button in the row
    /// leaves the window as it was.
    func testADoubleClickOnTheTitleRowsGroundActsAsTheTitleBars() throws {
        let (window, session) = try mount()
        defer { window.close(); session.destroy() }
        func node(_ id: String) throws -> NodeView { try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == id }) }
        func doubleClick(at p: NSPoint) throws {
            for clicks in 1...2 {
                for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
                    NSApp.sendEvent(try XCTUnwrap(NSEvent.mouseEvent(with: type, location: p, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                                     windowNumber: window.windowNumber, context: nil, eventNumber: clicks,
                                                                     clickCount: clicks, pressure: type == .leftMouseDown ? 1 : 0)))
                }
            }
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        }
        let frame = window.frame
        let left = try node("left"), right = try node("right")
        try doubleClick(at: right.convert(NSPoint(x: right.bounds.midX, y: right.bounds.midY), to: nil))
        XCTAssertEqual(window.frame, frame, "a button in the row takes its clicks")
        XCTAssertFalse(window.isMiniaturized)

        let a = left.convert(NSPoint(x: left.bounds.maxX, y: left.bounds.midY), to: nil)
        let b = right.convert(NSPoint(x: right.bounds.minX, y: right.bounds.midY), to: nil)
        let ground = NSPoint(x: (a.x + b.x) / 2, y: a.y)
        XCTAssertGreaterThan(ground.y, window.contentLayoutRect.maxY, "in the title bar's band")
        try doubleClick(at: ground)
        switch TitleBarAreaMac.action(.standard) {
        case .zoom: XCTAssertNotEqual(window.frame, frame, "zoomed")
        case .minimize: XCTAssertTrue(window.isMiniaturized)
        case .none: XCTAssertEqual(window.frame, frame)
        }
    }

    func testTheTitleBarsDoubleClickFollowsTheSystemSetting() throws {
        let suite = "exact.title-bar-action.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        XCTAssertEqual(TitleBarAreaMac.action(defaults), .zoom, "unset: zoom")
        defaults.set(true, forKey: "AppleMiniaturizeOnDoubleClick")
        XCTAssertEqual(TitleBarAreaMac.action(defaults), .minimize, "the older setting")
        for (value, action) in [("Maximize", TitleBarAreaMac.Action.zoom), ("Fill", .zoom), ("Minimize", .minimize), ("None", .none)] {
            defaults.set(value, forKey: "AppleActionOnDoubleClick")
            XCTAssertEqual(TitleBarAreaMac.action(defaults), action, value)
        }
    }
}
#endif
