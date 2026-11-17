// @ref LLP 1031 D3 — an embedded view's translucent page shows its host.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A containing app places an `ExactView` over its own views, as the sample
/// host (`ExactHostMac`) places its sessions in a split view's panes. A page
/// whose root background is translucent draws no canvas there, so the
/// host's ground shows behind it; an opaque one is the canvas, as in the
/// app's own window (LLP 1115 D2).
@MainActor
final class EmbeddedCanvasMacTests: XCTestCase {
    private func plan(_ background: String) throws -> Data {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        component App
          view
            main background-color="\(background)" width="100%" height="100%"
              row align-items="center" gap=10 padding=12
                text "Name"

        """
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"], "build.mjs --test builds the Contract compiler and names it"))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    /// A host view filled with one colour: the host's ground, and a swatch
    /// of the page's colour beside the view to compare a pixel with (a
    /// pixel read back is in the bitmap's colour space, not sRGB's).
    private final class Fill: NSView {
        let color: NSColor
        init(_ color: NSColor, frame: NSRect) { self.color = color; super.init(frame: frame) }
        required init?(coder: NSCoder) { nil }
        override func draw(_ dirtyRect: NSRect) { color.setFill(); dirtyRect.fill() }
    }

    /// The colour at `at` (in the host's coordinates, y up) of the window's
    /// content as AppKit draws it, as the bitmap's channels, 0 to 255.
    private func pixel(_ view: NSView, at: NSPoint) throws -> [Int] {
        let content = try XCTUnwrap(view.window?.contentView)
        let rep = try XCTUnwrap(content.bitmapImageRepForCachingDisplay(in: content.bounds))
        content.cacheDisplay(in: content.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / content.bounds.width
        let x = Int(at.x * scale), y = Int((content.bounds.height - at.y) * scale)
        let color = try XCTUnwrap(rep.colorAt(x: x, y: y))
        return [color.redComponent, color.greenComponent, color.blueComponent].map { Int(($0 * 255).rounded()) }
    }

    private func embed(_ background: String, dark: Bool = true) throws -> (ExactSession, ExactView, NSWindow) {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "embedded-canvas-\(UUID().uuidString.prefix(8))")
        XCTAssertNil(session.boot(plan: try plan(background), size: CGSize(width: 300, height: 200)).error)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        let host = Fill(NSColor(srgbRed: 1, green: 0, blue: 0, alpha: 1), frame: NSRect(x: 0, y: 0, width: 300, height: 400))
        window.contentView = host
        // The page's opaque colour, below the view, where the host shows.
        host.addSubview(Fill(NSColor(srgbRed: 0x33 / 255, green: 0x66 / 255, blue: 0x99 / 255, alpha: 1), frame: NSRect(x: 0, y: 0, width: 20, height: 20)))
        let view = ExactView(session: session)
        view.frame = NSRect(x: 0, y: 200, width: 300, height: 200)
        host.addSubview(view)
        host.layoutSubtreeIfNeeded()
        return (session, view, window)
    }

    /// The issue's page, `transparent`, at the foot of a host's pane: no
    /// view between the host and the page's nodes is opaque or backed by a
    /// ground, `pageBackground` says clear, and the host's colour is what
    /// shows beside the text. One translucent colour does the same.
    func testATranslucentPageShowsTheHostBehindIt() throws {
        for background in ["transparent", "rgba(0, 0, 255, 0.5)"] {
            let (session, view, window) = try embed(background)
            defer { window.close(); session.destroy() }
            let p = session.presenter
            XCTAssertEqual(session.pageBackground.alphaComponent, 0, "\(background): the view draws no canvas, and says so")
            XCTAssertFalse(p.viewport.drawsBackground, background)
            // The viewport and what it scrolls, which the issue found opaque
            // and backed by the window background.
            let chain: [NSView] = [view, p.viewport, p.viewport.contentView] + (p.viewport.documentView.map { [$0] } ?? [])
            for v in chain {
                XCTAssertFalse(v.isOpaque, "\(background): \(type(of: v)) is not opaque")
                XCTAssertEqual(v.layer?.backgroundColor?.alpha ?? 0, 0, "\(background): \(type(of: v)) has no ground of its own")
            }
            if background == "transparent" {
                let host = try pixel(view, at: NSPoint(x: 280, y: 100))
                XCTAssertEqual(try pixel(view, at: NSPoint(x: 280, y: 220)), host, "the host's ground shows behind the page")
            }
        }
    }

    /// An opaque root is the canvas the view draws, over the host, as in
    /// the app's own window; a page that says nothing draws the window
    /// background (LLP 1115 D2).
    func testAnOpaquePageIsItsOwnCanvas() throws {
        let (session, view, window) = try embed("#336699")
        defer { window.close(); session.destroy() }
        XCTAssertTrue(session.presenter.viewport.drawsBackground)
        XCTAssertTrue(session.presenter.viewport.isOpaque)
        let page = try XCTUnwrap(session.pageBackground.usingColorSpace(.sRGB))
        XCTAssertEqual([page.redComponent, page.greenComponent, page.blueComponent].map { Int(($0 * 255).rounded()) }, [0x33, 0x66, 0x99])
        XCTAssertEqual(try pixel(view, at: NSPoint(x: 280, y: 220)), try pixel(view, at: NSPoint(x: 10, y: 10)), "the page's own ground covers the host")
    }
}
#endif
