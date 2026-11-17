#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// LLP 1039 D9 on the Mac: `exactViewport().devicePixelRatio` is the window's
/// scale as the kernel is told it, and follows a new one; a Mac has no
/// software keyboard, no safe area outside `viewport-fit=cover` and one
/// segment, the viewport.
final class ViewportValuesMacTests: XCTestCase {
    private static let plan: Result<Data, Error> = Result {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        shape Segment
          x: number
          width: number
        shape Viewport
          devicePixelRatio: option<number>
          keyboardHeight: number
          safeAreaTop: number
          segments: Segment[]
        component App
          resource viewport = exactViewport() as shape Viewport
          view
            text "values"

        """
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    private func viewport(_ session: ExactSession) throws -> [String: Any] {
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.runtime.agent("{\"op\":\"state\"}").utf8)) as? [String: Any])
        return try XCTUnwrap((state["resources"] as? [String: Any])?["viewport"] as? [String: Any])
    }

    func testTheWindowsScaleIsTheAppsAndFollowsANewOne() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "viewport-values")
        let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 600, height: 400), styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = ExactView(session: session)
        defer { PageFacts.agentDisplayScale = nil; window.close(); session.destroy() }
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 600, height: 400)).error)
        session.tellDisplayScale()
        let scale = PageFacts.displayScale(session.presenter.viewport)
        XCTAssertGreaterThan(scale, 0)
        var v = try viewport(session)
        XCTAssertEqual(v["devicePixelRatio"] as? Double, scale)
        XCTAssertEqual(v["keyboardHeight"] as? Double, 0)
        XCTAssertEqual(v["safeAreaTop"] as? Double, 0)
        let segments = try XCTUnwrap(v["segments"] as? [[String: Any]])
        XCTAssertEqual(segments.count, 1)
        XCTAssertEqual(segments.first?["width"] as? Double, 600)
        // Another display's scale, as the agent's `prefer display-scale` tells it.
        let other = scale == 3 ? 2.0 : 3.0
        XCTAssertNil(session.agentInstance.prefer(["page": ["display-scale": other]])["error"])
        session.tellDisplayScale()
        v = try viewport(session)
        XCTAssertEqual(v["devicePixelRatio"] as? Double, other)
    }
}
#endif
