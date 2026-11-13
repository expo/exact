#if os(macOS)
import AppKit
import XCTest
import ObjectiveC
@testable import ExactKit

final class InnerScrollMacTests: XCTestCase {
    private static var preferredStyle: NSScroller.Style = .overlay
    @objc private class func forcedScrollerStyle() -> NSScroller.Style { preferredStyle }

    private func forceScrollerStyle() throws -> () -> Void {
        _ = NSApplication.shared
        let original = try XCTUnwrap(class_getClassMethod(NSScroller.self, #selector(getter: NSScroller.preferredScrollerStyle)))
        let replacement = try XCTUnwrap(class_getClassMethod(Self.self, #selector(Self.forcedScrollerStyle)))
        method_exchangeImplementations(original, replacement)
        Self.preferredStyle = .overlay
        return { method_exchangeImplementations(original, replacement) }
    }

    private func compile(_ source: String) throws -> Data {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let contract = dir.appendingPathComponent("app.contract"), plan = dir.appendingPathComponent("app.plan")
        try source.write(to: contract, atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", contract.path, "-o", plan.path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: plan)
    }

    /// Legacy scrollers whose gutter makes the content fit settle: the gutter
    /// is on the axis that overflows only, and is kept once its scroller hides.
    func testALegacyGutterThatMakesItsContentFitSettles() throws {
        let restore = try forceScrollerStyle()
        defer { restore() }
        let plan = try compile("""
        component App
          view
            column
              view width=300 height=200 overflow="auto" testId="fill"
                view width="100%" height="100%" padding-bottom=1 testId="fill-child"
              view width=300 height=200 overflow="auto" testId="ratio"
                view width="100%" aspect-ratio="300 / 201" testId="ratio-child"

        """)
        let session = ExactApp.shared.makeSession(label: "gutter-settles")
        defer { session.destroy() }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 500), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let view = ExactView(session: session)
        window.contentView = view
        XCTAssertNil(session.boot(plan: plan, size: NSSize(width: 400, height: 500)).error)
        window.orderFrontRegardless()
        view.layoutSubtreeIfNeeded()
        let p = session.presenter
        var deliveries = 0, depth = 0, deepest = 0
        let deliver = p.onCovers
        p.onCovers = { changes in
            deliveries += 1
            guard deliveries < 64 else { return }
            depth += 1
            deepest = max(deepest, depth)
            deliver?(changes)
            depth -= 1
        }
        defer { p.onCovers = deliver }
        Self.preferredStyle = .legacy
        NotificationCenter.default.post(name: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil)
        for _ in 0..<10 {
            view.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        XCTAssertLessThan(deliveries, 8, "the gutters settle")
        XCTAssertEqual(deepest, 1, "a delivery returns before the next one starts")
        for name in ["fill", "ratio"] {
            let scroll = try XCTUnwrap(p.views.values.first { $0.props["testId"] == name }?.scroll)
            XCTAssertTrue(scroll.horizontalScroller!.isHidden, "\(name): nothing overflows horizontally")
            if !ExactEnv.agentMode { XCTAssertEqual(p.covers.delivered[scroll.pointerOwner!.id], .gutter(right: scroll.verticalScroller!.frame.width, bottom: 0), name) }
        }
    }

    func testInnerContentMatchesTheClipAcrossSystemScrollerStyles() throws {
        let restore = try forceScrollerStyle()
        defer { restore() }
        let plan = try compile("""
        component App
          view
            column
              view width=300 height=200 overflow="auto" testId="vertical"
                view width="100%" height=600 testId="vertical-child"
              view width=300 height=200 overflow="auto" testId="horizontal"
                view width=600 height="100%" testId="horizontal-child"
              view width=300 height=200 overflow="auto" direction="rtl" testId="rtl"
                view width="100%" height=600 testId="rtl-child"
              view width=300 height=200 overflow="auto" scrollbar-width="none" testId="hidden"
                view width="100%" height=600 testId="hidden-child"

        """)
        let session = ExactApp.shared.makeSession(label: "inner-scroll")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: plan, size: NSSize(width: 800, height: 800)).error)
        let p = session.presenter
        func node(_ name: String) throws -> NodeView { try XCTUnwrap(p.views.values.first { $0.props["testId"] == name }) }
        for style in [NSScroller.Style.overlay, .legacy, .overlay, .legacy] {
            Self.preferredStyle = style
            NotificationCenter.default.post(name: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil)
            for name in ["vertical", "horizontal", "hidden", "rtl"] {
                let parent = try node(name), child = try node(name + "-child")
                let scroll = try XCTUnwrap(parent.scroll)
                XCTAssertEqual(scroll.scrollerStyle, ExactEnv.agentMode ? .overlay : style)
                XCTAssertEqual(scroll.userInterfaceLayoutDirection, name == "rtl" ? .rightToLeft : .leftToRight)
                if name == "rtl" {
                    let drawn = child.convert(child.bounds, to: parent)
                    XCTAssertEqual(drawn.minX, scroll.contentView.frame.minX, accuracy: 0.001)
                    XCTAssertEqual(drawn.maxX, scroll.contentView.frame.maxX, accuracy: 0.001)
                }
                if name == "horizontal" {
                    XCTAssertEqual(child.frame.height, scroll.contentView.bounds.height, accuracy: 0.001)
                    XCTAssertTrue(scroll.verticalScroller!.isHidden, "a horizontal gutter must not create vertical overflow")
                } else {
                    XCTAssertEqual(child.frame.width, scroll.contentView.bounds.width, accuracy: 0.001)
                    XCTAssertTrue(scroll.horizontalScroller!.isHidden, "\(name): a vertical gutter must not create horizontal overflow; content \(parent.content), document \(scroll.documentView!.frame), clip \(scroll.contentView.frame), bounds \(scroll.contentView.bounds)")
                    XCTAssertEqual(child.frame.width < 300, style == .legacy && name != "hidden" && !ExactEnv.agentMode)
                }
            }
        }
    }
}
#endif
