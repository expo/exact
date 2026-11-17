#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// LLP 1021 §5.3.1: View's zoom, and the app's `setPageZoom`, are a page
/// zoom, as Chromium's: a CSS px is the factor's points, the page lays out
/// in the view's size over it, and the page's device pixels are the
/// window's backing scale times it (text line boxes, hairlines, rasters).
final class PageZoomMacTests: XCTestCase {
    private var window: NSWindow?
    override func tearDown() { window?.close(); window = nil }

    /// The viewport's width as the page reads it, a 100px box, a paragraph,
    /// buttons that set the page zoom, and an SVG pattern, which is drawn as
    /// pixels.
    private static let plan: Result<Data, Error> = Result {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = """
        shape Viewport
          width: number
        component App
          resource viewport = exactViewport() as shape Viewport
          action zoom
            setPageZoom(1.25)
          action refused(factor: number)
            setPageZoom(factor)
          view
            column
              text `${viewport.width}` testId="width"
              box testId="box" width=100 height=20
              text "A paragraph of text" testId="para" font-size=14
              button "Zoom" testId="zoom" press=zoom
              button "Refused" testId="refused" press=refused(0)
              svg testId="dots" width=60 height=24 viewBox="0 0 60 24"
                defs
                  pattern id="dots" patternUnits="userSpaceOnUse" width=6 height=6
                    circle cx=3 cy=3 r=2 fill="#0f172a"
                rect width=60 height=24 fill="url(#dots)"

        """
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"], "build.mjs --test builds the Contract compiler and names it"))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    func testAccumulatedZoomStepsSnapToTheNearbyLevel() {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "page-zoom-roundoff")
        defer { session.destroy() }
        for level in PageZoom.levels {
            session.setPageZoom([NSNumber(value: level - 3e-16)])
            XCTAssertEqual(session.pageZoom, level)
        }
        session.setPageZoom([NSNumber(value: 1.23456789)])
        XCTAssertEqual(session.pageZoom, 1.23456789, accuracy: 1e-12, "custom factors keep their precision")
    }

    func testAPageZoomScalesThePageAndShrinksItsViewport() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "page-zoom")
        defer { session.destroy() }
        let shown = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        shown.isReleasedWhenClosed = false
        window = shown
        let view = ExactView(session: session)
        shown.contentView = view
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 400, height: 300)).error)
        shown.makeKeyAndOrderFront(nil)
        view.layoutSubtreeIfNeeded()
        let backing = shown.backingScaleFactor
        func node(_ id: String) throws -> NodeView { try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == id }) }
        func tap(_ id: String) throws {
            XCTAssertNil(session.agentInstance.tap(["op": "tap", "id": Int(try node(id).id)])["error"])
            RunLoop.main.run(until: Date().addingTimeInterval(0.05)) // the command is delivered on the main queue
            view.layoutSubtreeIfNeeded()
        }
        XCTAssertEqual(try node("width").props["text"], "400")
        XCTAssertEqual(session.text.lineScale, backing)

        try tap("zoom")
        XCTAssertEqual(session.pageZoom, 1.25)
        XCTAssertEqual(session.presenter.viewportSize.width, 320, "the view's 400 points over the zoom")
        XCTAssertEqual(try node("width").props["text"], "320", "the page reads its viewport in CSS px")
        let box = try node("box")
        XCTAssertEqual(box.frame.width, 100, "laid out in CSS px")
        XCTAssertEqual(box.convert(box.bounds, to: nil).width, 125, accuracy: 0.01, "drawn at 1.25 points a CSS px")
        XCTAssertEqual(try node("para").pixelScale ?? 0, backing * 1.25, accuracy: 0.001, "the page's device pixels")
        XCTAssertEqual(session.text.lineScale, backing * 1.25, "line boxes round to them, as Chromium's do")
        XCTAssertEqual(PageFacts.displayScale(session.presenter.viewport), Double(backing) * 1.25, "and hairlines are one of them")

        try tap("refused")
        XCTAssertEqual(session.pageZoom, 1.25, "a factor that is not above 0 changes nothing")

        // View ▸ Actual Size, with no menu to send it.
        session.pageZoom = 1
        view.layoutSubtreeIfNeeded()
        XCTAssertEqual(try node("width").props["text"], "400")
        XCTAssertEqual(try node("box").convert(try node("box").bounds, to: nil).width, 100, accuracy: 0.01)
        XCTAssertEqual(session.text.lineScale, backing)
        session.presenter.viewport.scrollerStyle = .legacy
        for _ in 0..<3 {
            session.pageZoom = 1.25
            session.setPageZoom([NSNumber(value: 0.9999999999999997)])
            view.layoutSubtreeIfNeeded()
            XCTAssertEqual(session.pageZoom, 1)
            XCTAssertEqual(session.presenter.viewportSize, CGSize(width: 400, height: 300))
            XCTAssertTrue(session.presenter.viewport.horizontalScroller!.isHidden)
            XCTAssertTrue(session.presenter.viewport.verticalScroller!.isHidden)
        }
        // Clamped to Chromium's limits.
        session.pageZoom = 9
        XCTAssertEqual(session.pageZoom, 5)
    }

    /// An SVG's pictures (here a pattern's tile) are drawn at the page's
    /// device pixels, which AppKit does not put in the layer's
    /// `contentsScale`, and drawn again when the zoom changes.
    func testAnSvgPictureIsDrawnAtThePageZoomsPixels() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "page-zoom-svg")
        defer { session.destroy() }
        let shown = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        shown.isReleasedWhenClosed = false
        window = shown
        let view = ExactView(session: session)
        shown.contentView = view
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 400, height: 300)).error)
        shown.makeKeyAndOrderFront(nil)
        view.layoutSubtreeIfNeeded()
        let svg = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "dots" })
        // Device pixels per unit of the widest picture in the scene.
        func pictureScale() throws -> CGFloat {
            var widest: (pixels: Int, units: CGFloat) = (0, 0)
            func walk(_ layer: CALayer) {
                if let contents = layer.contents, CFGetTypeID(contents as CFTypeRef) == CGImage.typeID, layer.bounds.width > widest.units {
                    widest = ((contents as! CGImage).width, layer.bounds.width)
                }
                layer.sublayers?.forEach(walk)
            }
            walk(try XCTUnwrap(svg.layer))
            XCTAssertGreaterThan(widest.units, 0, "the pattern is a picture")
            return CGFloat(widest.pixels) / max(widest.units, 1)
        }
        let backing = shown.backingScaleFactor
        XCTAssertEqual(try pictureScale(), backing, accuracy: 0.1)
        session.pageZoom = 3
        view.layoutSubtreeIfNeeded()
        XCTAssertEqual(svg.pixelScale ?? 0, backing * 3, accuracy: 0.001)
        XCTAssertEqual(try pictureScale(), backing * 3, accuracy: 0.1, "the tile at the zoom's pixels, not stretched")
        session.pageZoom = 1
        XCTAssertEqual(try pictureScale(), backing, accuracy: 0.1)
    }
}
#endif
