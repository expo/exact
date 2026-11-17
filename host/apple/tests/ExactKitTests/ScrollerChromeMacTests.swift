#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// Under `viewport-fit=cover` the page runs under the title bar, and so do
/// a `scroll` node's content and the page's own. Their scrollers stop at the
/// bar, as a hand-built AppKit window's do with a full-size content view,
/// while the content keeps scrolling beneath it (#266).
@MainActor
final class ScrollerChromeMacTests: XCTestCase {
    private static let plan: Result<Data, Error> = Result {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        component App
          derive rows = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40]
          view
            main viewport-fit="cover" width="100%" height="100%" display="flex" flex-direction="column"
              scroll id="content" width="100%" flex=1 min-height=0 testId="content"
                column padding-top="env(safe-area-inset-top)" gap=16
                  for n in rows key=n
                    text `Row ${n}` padding=12
              scroll id="foot" width="100%" height=60 testId="foot"
                column
                  for n in rows key=n
                    text `Foot ${n}`

        """
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"], "build.mjs --test builds the Contract compiler and names it"))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    func testScrollersStopAtTheTitleBarAndTheContentScrollsBeneathIt() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "scroller-chrome")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 600, height: 500)).error)
        let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 600, height: 500), styleMask: [.titled, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let view = ExactView(session: session)
        window.contentView = view
        window.orderFrontRegardless()
        let p = session.presenter
        func node(_ testId: String) throws -> NodeView { try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId) }
        waitFor("the window's insets and the scroller") { p.insets.top > 0 && (try? node("content").scroll) != nil }
        let bar = window.contentView!.bounds.height - window.contentLayoutRect.maxY
        XCTAssertGreaterThan(bar, 0, "the title bar is over the content")
        XCTAssertEqual(p.insets.top, bar, "the page is told the bar as its safe area")

        let content = try XCTUnwrap(try node("content").scroll)
        XCTAssertEqual(content.scrollerInsets.top, bar, "the inner scroller is inset by the bar it runs under")
        XCTAssertEqual(content.scrollerInsets.bottom, 0)
        let scroller = try XCTUnwrap(content.verticalScroller)
        XCTAssertEqual(scroller.convert(scroller.bounds, to: nil).maxY, window.contentLayoutRect.maxY, accuracy: 0.5, "it ends where the bar begins")
        XCTAssertEqual(content.contentView.frame.height, content.bounds.height, "the clip still runs under the bar")
        XCTAssertTrue(content.contentInsets.top == 0 && content.contentInsets.bottom == 0, "the content is not inset: CSS lays it out")
        XCTAssertEqual(p.viewport.scrollerInsets.top, bar, "the page's own scroller too")

        content.contentView.scroll(to: NSPoint(x: 0, y: 20))
        content.reflectScrolledClipView(content.contentView)
        XCTAssertEqual(content.contentView.bounds.origin.y, 20, "the content scrolls beneath the bar")
        XCTAssertEqual(content.scrollerInsets.top, bar)

        let foot = try XCTUnwrap(try node("foot").scroll)
        XCTAssertEqual(foot.scrollerInsets.top, 0, "a scroller clear of the bar is not inset")

        // A window whose content starts below the bar has no scroller under it.
        window.styleMask.remove(.fullSizeContentView)
        view.syncInsets()
        waitFor("the scroller's insets follow the bar") { content.scrollerInsets.top == 0 }
        XCTAssertEqual(content.scrollerInsets.top, 0, "nothing of the window is over it")
        XCTAssertEqual(p.viewport.scrollerInsets.top, 0)
    }
}
#endif
