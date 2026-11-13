#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A real full-screen transition (`toggleFullScreen`, what ⌃⌘F, the green
/// button and the View menu send): `env(titlebar-area-*)` is zero in full
/// screen and AppKit's again after it, and `displayMode` flips with it (LLP
/// 1039 D7, D8). `TitleBarAreaMacTests` and `DisplayModeMacTests` supply the
/// window's state and notification instead.
final class FullScreenTransitionMacTests: XCTestCase {
    private static let plan: Result<Data, Error> = Result {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        shape Viewport
          displayMode: string
        component App
          resource viewport = exactViewport() as shape Viewport
          view
            main viewport-fit="cover" width="100%" height="100%"
              row testId="bar" margin-left="env(titlebar-area-x, 5px)" width="env(titlebar-area-width, 50px)" height="env(titlebar-area-height, 9px)"
              text viewport.displayMode testId="mode"

        """
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    /// What the runtime sees: the display mode in agent `state` and the
    /// resource, and the bar's laid-out x, width and height.
    private struct Seen: Equatable {
        var mode: String?
        var resource: String?
        var bar: [CGFloat]
    }

    private func seen(_ session: ExactSession) throws -> Seen {
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.runtime.agent("{\"op\":\"state\"}").utf8)) as? [String: Any])
        let resource = ((state["resources"] as? [String: Any])?["viewport"] as? [String: Any])?["displayMode"] as? String
        let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "bar" })
        return Seen(mode: (state["device"] as? [String: Any])?["displayMode"] as? String, resource: resource,
                    bar: [node.frame.minX, node.frame.width, node.frame.height])
    }

    /// Spins the run loop until `ready`, as the transition's animation and
    /// the session's batches need it to.
    private func settle(_ what: String, _ ready: () throws -> Bool) rethrows {
        let end = Date().addingTimeInterval(5)
        while try !ready(), Date() < end { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
        XCTAssertTrue(try ready(), what)
    }

    func testARealFullScreenTransitionZeroesTheTitleBarAreaAndFlipsTheDisplayMode() throws {
        // A locked session's window server makes no Space for the window.
        let console = CGSessionCopyCurrentDictionary() as? [String: Any]
        try XCTSkipIf(console?["CGSSessionScreenIsLocked"] as? Bool == true, "the screen is locked")
        let app = NSApplication.shared
        app.setActivationPolicy(.regular)
        app.activate(ignoringOtherApps: true)
        let session = ExactApp.shared.makeSession(label: "full-screen-transition")
        let window = NSWindow(contentRect: NSRect(x: 200, y: 200, width: 900, height: 600),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.collectionBehavior.insert(.fullScreenPrimary)
        window.isReleasedWhenClosed = false
        let view = ExactView(session: session)
        window.contentView = view
        defer {
            if window.styleMask.contains(.fullScreen) {
                let exit = expectation(forNotification: NSWindow.didExitFullScreenNotification, object: window)
                window.toggleFullScreen(nil)
                wait(for: [exit], timeout: 10)
            }
            window.close(); session.destroy()
        }
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 900, height: 600)).error)
        window.makeKeyAndOrderFront(nil)
        view.layoutSubtreeIfNeeded()
        let windowed = try XCTUnwrap(TitleBarAreaMac.rect(of: view, cover: true))
        XCTAssertGreaterThan(windowed[0], 0, "the traffic lights reserve the area's left")
        XCTAssertGreaterThan(windowed[3], 0)
        let before = Seen(mode: "standalone", resource: "standalone", bar: [windowed[0], windowed[2], windowed[3]])
        try settle("the windowed area and mode reach the runtime") { try seen(session) == before }

        let enter = expectation(forNotification: NSWindow.didEnterFullScreenNotification, object: window)
        window.toggleFullScreen(nil)
        wait(for: [enter], timeout: 10)
        XCTAssertTrue(window.styleMask.contains(.fullScreen))
        XCTAssertEqual(session.presenter.titlebarArea, [0, 0, 0, 0])
        try settle("full screen reaches the runtime") {
            try seen(session) == Seen(mode: "fullscreen", resource: "fullscreen", bar: [0, 0, 0])
        }

        let exit = expectation(forNotification: NSWindow.didExitFullScreenNotification, object: window)
        window.toggleFullScreen(nil)
        wait(for: [exit], timeout: 10)
        XCTAssertFalse(window.styleMask.contains(.fullScreen))
        try settle("leaving full screen restores the area and mode") {
            view.layoutSubtreeIfNeeded()
            return try seen(session) == before && session.presenter.titlebarArea == TitleBarAreaMac.rect(of: view, cover: true)
        }
        XCTAssertEqual(session.presenter.titlebarArea, windowed)
    }
}
#endif
