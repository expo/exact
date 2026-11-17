#if os(macOS)
import AppKit
import ImageIO
import XCTest
@testable import ExactKit

/// LLP 1053.000.000 on AppKit: a `glassGroup` node's container view is its
/// innermost view and holds its children (D2); a material wins over it
/// (D6); a grouped glass whose path fades or clips moves into a container
/// of its own and back (D4), keeping the first responder; the group view
/// is never a hit target.
final class GlassGroupMacTests: XCTestCase {
    private var window: NSWindow!

    override func setUpWithError() throws {
        guard #available(macOS 26.0, *) else { throw XCTSkip("Liquid Glass is macOS 26") }
    }
    override func tearDown() { window?.close(); window = nil }

    private func presenter(_ ops: [[String: Any]]) -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.apply(wireBatch(ops))
        return p
    }
    private func box(_ id: Int, _ props: [String: String] = [:], kind: String = "view", x: Double = 0, w: Double = 40, handlers: [String] = []) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": kind, "props": props, "handlers": handlers,
          "style": ["border_radius": 20.0, "text_color": [0, 0, 0, 255]]],
         ["op": "frame", "id": id, "x": x, "y": 0.0, "w": w, "h": 40.0]]
    }
    /// A group (1) of a glass (2) and, under a plain box (3), a glass (4).
    private func cluster(_ group: [String: String] = ["glassGroup": "12"]) -> [[String: Any]] {
        box(1, group, w: 200, handlers: ["press"]) + box(2, ["backgroundMaterial": "glass"])
            + box(3, x: 48, w: 48) + box(4, ["backgroundMaterial": "glass"])
            + [["op": "children", "id": 3, "ids": [4]], ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]]]
    }
    private func spacing(_ node: NodeView) -> CGFloat? {
        guard #available(macOS 26.0, *) else { return nil }
        return (node.glassGroupView as? NSGlassEffectContainerView)?.spacing
    }
    private func present(_ p: Presenter, _ id: Int, _ opacity: Double) {
        p.apply(wireBatch([["op": "present", "id": id, "property": "opacity", "x": opacity, "y": 0.0, "w": 0.0, "h": 0.0]]))
    }

    private func show() {
        window.orderFrontRegardless()
        window.layoutIfNeeded()
        window.displayIfNeeded()
        CATransaction.flush()
        let refresh = 1 / Double(max(30, window.screen?.maximumFramesPerSecond ?? 60))
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 2 * refresh + 0.004))
        XCTAssertNotNil(Agent.ownWindowImage(window.windowNumber), "the fixture committed a frame")
    }

    private func windowPicture(of view: NSView) -> NSBitmapImageRep? {
        guard let image = Agent.ownWindowImage(window.windowNumber) else { return nil }
        let frame = view.convert(view.bounds, to: nil)
        let x = CGFloat(image.width) / window.frame.width
        let y = CGFloat(image.height) / window.frame.height
        let pixels = CGRect(x: frame.minX * x, y: (window.frame.height - frame.maxY) * y,
                            width: frame.width * x, height: frame.height * y).integral
        guard let crop = image.cropping(to: pixels) else { return nil }
        let rep = NSBitmapImageRep(cgImage: crop)
        rep.size = view.bounds.size
        return rep
    }

    private func waitForWindowPicture(of view: NSView, until ready: (NSBitmapImageRep) -> Bool) throws -> NSBitmapImageRep {
        var picture: NSBitmapImageRep?, answered = false, failure = ""
        waitFor("the WindowServer to show the fixture", late: 0.1, cap: 5,
                facts: { answered ? "the window answered, but its pixels were stale" : "the window gave no picture" },
                onFailure: { failure = $0 }) {
            guard let candidate = windowPicture(of: view) else { return false }
            answered = true
            if ready(candidate) { picture = candidate; return true }
            return false
        }
        guard answered else { throw XCTSkip("the window server gave no picture (the screen is locked or the display is asleep)") }
        return try XCTUnwrap(picture, failure)
    }

    private func color(_ picture: NSBitmapImageRep, _ point: NSPoint) throws -> NSColor {
        let x = CGFloat(picture.pixelsWide) / picture.size.width
        let y = CGFloat(picture.pixelsHigh) / picture.size.height
        return try XCTUnwrap(picture.paintedColorAt(x: Int(point.x * x), y: Int(point.y * y))?.usingColorSpace(.sRGB))
    }

    private func assertColor(_ picture: NSBitmapImageRep, at point: NSPoint,
                             red: ClosedRange<CGFloat>, green: ClosedRange<CGFloat>, blue: ClosedRange<CGFloat>,
                             _ message: String, file: StaticString = #filePath, line: UInt = #line) throws {
        let sampled = try color(picture, point)
        XCTAssertTrue(red.contains(sampled.redComponent) && green.contains(sampled.greenComponent) && blue.contains(sampled.blueComponent),
                      "\(message): \(sampled)", file: file, line: line)
    }

    private func assertRed(_ picture: NSBitmapImageRep, at point: NSPoint, _ message: String,
                           file: StaticString = #filePath, line: UInt = #line) throws {
        try assertColor(picture, at: point, red: 0.9 ... 1, green: 0 ... 0.2, blue: 0 ... 0.2, message, file: file, line: line)
    }

    private func assertBlue(_ picture: NSBitmapImageRep, at point: NSPoint, _ message: String,
                            file: StaticString = #filePath, line: UInt = #line) throws {
        try assertColor(picture, at: point, red: 0 ... 0.2, green: 0 ... 0.2, blue: 0.9 ... 1, message, file: file, line: line)
    }

    private func assertPicturesAgree(_ actual: NSBitmapImageRep, _ expected: NSBitmapImageRep, in rect: NSRect,
                                     file: StaticString = #filePath, line: UInt = #line) throws {
        var worst: (delta: CGFloat, point: NSPoint, actual: NSColor, expected: NSColor)?
        for y in stride(from: rect.minY, to: rect.maxY, by: 2) {
            for x in stride(from: rect.minX, to: rect.maxX, by: 2) {
                let a = try color(actual, NSPoint(x: x, y: y)), e = try color(expected, NSPoint(x: x, y: y))
                let delta = max(abs(a.redComponent - e.redComponent), abs(a.greenComponent - e.greenComponent), abs(a.blueComponent - e.blueComponent))
                if worst == nil || delta > worst!.delta { worst = (delta, NSPoint(x: x, y: y), a, e) }
            }
        }
        if let worst {
            XCTAssertLessThanOrEqual(worst.delta, 0.04,
                "largest channel delta at \(worst.point): \(worst.actual) against \(worst.expected)", file: file, line: line)
        }
    }

    private func imageInGlass(alpha: CGFloat) throws -> (Presenter, NodeView, NodeView) {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("glass-image-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let context = try XCTUnwrap(CGContext(data: nil, width: 4, height: 4, bitsPerComponent: 8, bytesPerRow: 16,
                                              space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(red: 1, green: 0, blue: 0, alpha: alpha)
        context.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
        let url = directory.appendingPathComponent("red.png")
        let destination = try XCTUnwrap(CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(context.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))

        let p = presenter([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view", "props": ["backgroundMaterial": "glass"]],
            ["op": "create", "id": 3, "kind": "image", "style": ["object_fit": "fill"]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 3, "x": 20, "y": 20, "w": 80, "h": 80],
        ])
        let page = try XCTUnwrap(p.views[1]), image = try XCTUnwrap(p.views[3])
        let loader = RasterLoader(), resolver = AssetResolver(root: directory)
        defer { loader.shutdown(); withExtendedLifetime(resolver) {} }
        image.loadGeneration = 1
        loader.load(image, source: "red.png", resolver: resolver)
        waitFor("the image to load", cap: 5) { image.raster != nil && image.imageLayer != nil }
        return (p, page, image)
    }

    private func agentGlassFixture() throws -> (ExactSession, NSView) {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "glass-capture-\(UUID().uuidString.prefix(6))")
        let p = session.presenter
        window = NSWindow(contentRect: NSRect(x: 80, y: 80, width: 240, height: 160),
                          styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view", "props": ["backgroundMaterial": "glass"]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 240, "h": 160],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 240, "h": 160],
        ]))
        let root = p.navigation.split.agentRoot
        window.orderFrontRegardless()
        window.layoutIfNeeded()
        window.displayIfNeeded()
        return (session, root)
    }

    private func screenshotColor(_ rep: NSBitmapImageRep, at point: NSPoint, in root: NSView, windowed: Bool) throws -> NSColor {
        if !windowed { return try color(rep, point) }
        let inWindow = root.convert(point, to: nil)
        let x = CGFloat(rep.pixelsWide) / window.frame.width
        let y = CGFloat(rep.pixelsHigh) / window.frame.height
        return try XCTUnwrap(rep.paintedColorAt(x: Int(inWindow.x * x), y: Int((window.frame.height - inWindow.y) * y))?.usingColorSpace(.sRGB))
    }

    private func assertSignature(_ sampled: NSColor, matches expected: NSColor, _ message: String,
                                 file: StaticString = #filePath, line: UInt = #line) throws {
        let expected = try XCTUnwrap(expected.usingColorSpace(.sRGB), file: file, line: line)
        for (actual, wanted) in zip([sampled.redComponent, sampled.greenComponent, sampled.blueComponent],
                                    [expected.redComponent, expected.greenComponent, expected.blueComponent]) {
            if wanted >= 0.5 { XCTAssertGreaterThan(actual, 0.78, message, file: file, line: line) }
            else { XCTAssertLessThan(actual, 0.38, message, file: file, line: line) }
        }
    }

    func testOnlyNonHiddenNativeGlassSelectsTheWindowServerCapture() throws {
        let p = presenter([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "props": ["backgroundMaterial": "glass"]],
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 3, "x": 20, "y": 20, "w": 100, "h": 100],
        ])
        let root = try XCTUnwrap(p.views[1]), ancestor = try XCTUnwrap(p.views[2]), glass = try XCTUnwrap(p.views[3])
        XCTAssertTrue(Capture.hasGlass(in: root))

        glass.isHidden = true
        XCTAssertFalse(Capture.hasGlass(in: root), "hidden glass")
        glass.isHidden = false
        ancestor.isHidden = true
        XCTAssertFalse(Capture.hasGlass(in: root), "glass with a hidden ancestor")
        ancestor.isHidden = false

        p.apply(wireBatch([["op": "frame", "id": 3, "x": 240, "y": 20, "w": 100, "h": 100]]))
        XCTAssertTrue(Capture.hasGlass(in: root), "off-screen glass is a harmless false positive")
        p.apply(wireBatch([["op": "frame", "id": 3, "x": 20, "y": 20, "w": 0, "h": 100]]))
        XCTAssertTrue(Capture.hasGlass(in: root), "zero-width glass still needs no geometric judgement")
        p.apply(wireBatch([["op": "frame", "id": 3, "x": 20, "y": 20, "w": 100, "h": 0]]))
        XCTAssertTrue(Capture.hasGlass(in: root), "zero-height glass still needs no geometric judgement")
        glass.isHidden = true
        root.addSubview(NSVisualEffectView(frame: root.bounds))
        XCTAssertFalse(Capture.hasGlass(in: root), "an NSVisualEffectView material is not native glass")
    }

    func testConservativeGlassSelectionLeavesClippingToTheCapture() throws {
        let p = presenter([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "props": ["backgroundMaterial": "glass"]],
            ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "children", "id": 3, "ids": [4]], ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 200, "h": 0],
            ["op": "frame", "id": 3, "x": 20, "y": 20, "w": 80, "h": 80],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 80, "h": 80],
        ])
        let root = try XCTUnwrap(p.views[1]), wrapper = try XCTUnwrap(p.views[2]), glass = try XCTUnwrap(p.views[3])
        show()
        XCTAssertTrue(Capture.hasGlass(in: root), "visible overflow from a zero-height wrapper is painted")
        try assertRed(try XCTUnwrap(Capture.windowServerPicture(of: root)), at: NSPoint(x: 40, y: 40), "overflowing glass content")

        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_x": "hidden", "overflow_y": "hidden"]]]))
        XCTAssertTrue(Capture.hasGlass(in: root), "capture selection does not interpret the overflow clip")
        try assertBlue(try XCTUnwrap(Capture.windowServerPicture(of: root)), at: NSPoint(x: 40, y: 40), "the zero-height clip removes the content")
        p.apply(wireBatch([["op": "style", "id": 2, "style": [:]]]))

        wrapper.frame = NSRect(x: 0, y: 0, width: 200, height: 200)
        glass.frame = NSRect(x: 240, y: 20, width: 80, height: 80)
        p.apply(wireBatch([["op": "present", "id": 3, "property": "translate", "x": -220.0, "y": 0.0]]))
        XCTAssertTrue(Capture.hasGlass(in: root), "a layer transform carries off-layout glass into the viewport")
        try assertRed(try XCTUnwrap(Capture.windowServerPicture(of: root)), at: NSPoint(x: 40, y: 40), "translated glass content")
        glass.frame = NSRect(x: 20, y: 20, width: 80, height: 80)
        p.apply(wireBatch([["op": "present", "id": 3, "property": "translate", "x": 220.0, "y": 0.0]]))
        XCTAssertTrue(Capture.hasGlass(in: root), "capture selection does not interpret a layer transform")
        try assertBlue(try XCTUnwrap(Capture.windowServerPicture(of: root)), at: NSPoint(x: 40, y: 40), "translated-out glass leaves the ground")
        p.apply(wireBatch([["op": "present", "id": 3, "property": "translate", "x": 0.0, "y": 0.0],
                           ["op": "present", "id": 3, "property": "scale", "x": 0.0, "y": 0.0]]))
        XCTAssertTrue(Capture.hasGlass(in: root), "capture selection does not interpret a zero scale")
        try assertBlue(try XCTUnwrap(Capture.windowServerPicture(of: root)), at: NSPoint(x: 40, y: 40), "zero-scale glass leaves the ground")

        p.apply(wireBatch([
            ["op": "present", "id": 3, "property": "scale", "x": 1.0, "y": 0.0],
            ["op": "style", "id": 2, "style": ["clip_path": ["commands": [
                ["M", [0, 0]], ["L", [60, 0]], ["L", [60, 60]], ["L", [0, 60]], ["Z", []],
            ]]]],
        ]))
        XCTAssertNotNil(wrapper.layer?.mask, "the ancestor's clip-path is a shape mask")
        XCTAssertTrue(Capture.hasGlass(in: root), "a shape mask clips visible glass without hiding it")
        let clipped = try XCTUnwrap(Capture.windowServerPicture(of: root))
        try assertRed(clipped, at: NSPoint(x: 40, y: 40), "glass visible inside the clip-path")
        try assertBlue(clipped, at: NSPoint(x: 80, y: 40), "the clip-path removes glass outside its path")
    }

    func testTranslatedShapeClipGlassKeepsItsWindowServerBackdrop() throws {
        let p = presenter([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view", "style": ["clip_path": ["commands": [
                ["M", [0, 40]], ["C", [0, 18, 18, 0, 40, 0]], ["C", [62, 0, 80, 18, 80, 40]],
                ["C", [80, 62, 62, 80, 40, 80]], ["C", [18, 80, 0, 62, 0, 40]], ["Z", []],
            ]]]],
            ["op": "create", "id": 3, "kind": "view", "props": ["backgroundMaterial": "glass"]],
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 160],
            ["op": "frame", "id": 2, "x": 220, "y": 20, "w": 80, "h": 80],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 80, "h": 80],
            ["op": "present", "id": 2, "property": "translate", "x": -200.0, "y": 0.0],
        ])
        let root = try XCTUnwrap(p.views[1])
        show()
        XCTAssertTrue(Capture.hasGlass(in: root), "the layer transform does not move the clip's local path away from its glass")
        let live = try waitForWindowPicture(of: root) { (try? self.color($0, NSPoint(x: 60, y: 60)).blueComponent) ?? 0 > 0.5 }
        let captured = try XCTUnwrap(Capture.windowServerPicture(of: root))
        try assertPicturesAgree(captured, live, in: NSRect(x: 40, y: 40, width: 40, height: 40))
    }

    @available(macOS 26.0, *)
    func testDisjointNestedShapeClipsFallBackToTheLayerCapture() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor.blue.cgColor
        let outer = NSView(frame: root.bounds), inner = NSView(frame: root.bounds)
        for view in [outer, inner] { view.wantsLayer = true }
        let first = CAShapeLayer(), second = CAShapeLayer()
        first.path = CGPath(ellipseIn: NSRect(x: 5, y: 5, width: 40, height: 40), transform: nil)
        second.path = CGPath(ellipseIn: NSRect(x: 35, y: 35, width: 40, height: 40), transform: nil)
        outer.layer?.mask = first
        inner.layer?.mask = second
        inner.addSubview(NSGlassEffectView(frame: root.bounds))
        outer.addSubview(inner)
        root.addSubview(outer)
        window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = root
        Capture.windowImageOverride = { _ in nil }
        defer { Capture.windowImageOverride = nil }

        XCTAssertTrue(Capture.hasGlass(in: root), "the conservative walk sees the non-hidden glass")
        if case .success = Capture.windowServerPictureResult(of: root) { XCTFail("the test override supplied a server picture") }
        XCTAssertNotNil(Capture.picture(of: root), "an unavailable server picture falls back to the shared compositor")
    }

    @available(macOS 26.0, *)
    func testGlassWhollyOutsideAShapeClipUsesTheLayerCapture() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor.blue.cgColor
        let clipped = NSView(frame: root.bounds)
        clipped.wantsLayer = true
        let mask = CAShapeLayer()
        mask.frame = clipped.bounds
        mask.path = CGPath(ellipseIn: clipped.bounds, transform: nil)
        clipped.layer?.mask = mask
        let glass = NSGlassEffectView(frame: NSRect(x: 0, y: 0, width: 10, height: 10))
        clipped.addSubview(glass)
        root.addSubview(clipped)
        window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = root
        Capture.windowImageOverride = { _ in nil }
        defer { Capture.windowImageOverride = nil }

        XCTAssertTrue(Capture.hasGlass(in: root), "capture selection does not interpret the circular clip")
        if case .success = Capture.windowServerPictureResult(of: root) { XCTFail("the test override supplied a server picture") }
        XCTAssertNotNil(Capture.picture(of: root), "the ordinary compositor remains the fallback when no server picture is available")
    }

    @available(macOS 26.0, *)
    func testGlassOutsideANestedScrollClipStillCapturesTheClippedPage() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 200, height: 200))
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor.blue.cgColor
        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        let document = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 300))
        let glass = NSGlassEffectView(frame: NSRect(x: 150, y: 20, width: 40, height: 40))
        let ink = NSView(frame: glass.bounds)
        ink.wantsLayer = true
        ink.layer?.backgroundColor = NSColor.red.cgColor
        glass.contentView?.addSubview(ink)
        document.addSubview(glass)
        scroll.documentView = document
        root.addSubview(scroll)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = root
        show()
        XCTAssertTrue(Capture.hasGlass(in: root), "capture selection does not interpret the scroll view's clip")
        try assertBlue(try XCTUnwrap(Capture.windowServerPicture(of: root)), at: NSPoint(x: 170, y: 40), "the clipped glass paints no pixels")
    }

    func testBothScreenshotPathsUseFreshFramesWithoutChangingTheViewTree() throws {
        let (session, root) = try agentGlassFixture()
        defer { session.destroy() }
        XCTAssertTrue(Capture.hasGlass(in: root), "the viewport path uses the WindowServer")
        let viewMarker = GlassCaptureMarker(frame: NSRect(x: 30, y: 30, width: 24, height: 24), ink: .red)
        root.addSubview(viewMarker, positioned: .above, relativeTo: nil)
        let layerMarker = NSView(frame: NSRect(x: 90, y: 30, width: 24, height: 24))
        layerMarker.wantsLayer = true
        layerMarker.layer?.backgroundColor = NSColor.red.cgColor
        root.addSubview(layerMarker, positioned: .above, relativeTo: nil)
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("glass-frame-\(UUID().uuidString).png")
        defer { try? FileManager.default.removeItem(at: path) }

        func tree(_ view: NSView) -> [ObjectIdentifier] {
            [ObjectIdentifier(view)] + view.subviews.flatMap(tree)
        }

        func capture(_ windowed: Bool) throws -> NSBitmapImageRep {
            let before = tree(try XCTUnwrap(window.contentView))
            var during: [ObjectIdentifier]?
            if !windowed {
                Capture.windowImageOverride = { number in
                    during = tree(self.window.contentView!)
                    return Agent.ownWindowImage(number)
                }
            }
            defer { Capture.windowImageOverride = nil }
            let reply = session.agentInstance.screenshot(["path": path.path, "window": windowed])
            XCTAssertNil(reply["error"], "\(reply)")
            if !windowed { XCTAssertEqual(during, before, "the viewport capture has not inserted a view while taking its picture") }
            XCTAssertEqual(tree(try XCTUnwrap(window.contentView)), before, "capture leaves the view tree unchanged")
            return try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: path)))
        }
        let inks: [NSColor] = [.red, .green, .blue]
        for windowed in [false, true] {
            for ink in inks {
                viewMarker.ink = ink
                viewMarker.needsDisplay = true
                let picture = try capture(windowed)
                let point = viewMarker.convert(NSPoint(x: viewMarker.bounds.midX, y: viewMarker.bounds.midY), to: root)
                try assertSignature(try screenshotColor(picture, at: point, in: root, windowed: windowed), matches: ink,
                                    "a view paint requested immediately before window=\(windowed)")
            }
            for ink in inks {
                window.displayIfNeeded()
                layerMarker.layer?.backgroundColor = ink.cgColor
                let picture = try capture(windowed)
                let point = layerMarker.convert(NSPoint(x: layerMarker.bounds.midX, y: layerMarker.bounds.midY), to: root)
                try assertSignature(try screenshotColor(picture, at: point, in: root, windowed: windowed), matches: ink,
                                    "a layer-only commit requested immediately before window=\(windowed)")
            }
        }
    }

    func testGlassCaptureFallsBackWithoutChangingTheViewTree() throws {
        let (session, _) = try agentGlassFixture()
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("glass-failure-\(UUID().uuidString).png")
        defer {
            Capture.windowImageOverride = nil
            try? FileManager.default.removeItem(at: path)
            session.destroy()
        }
        let content = try XCTUnwrap(window.contentView)
        func tree(_ view: NSView) -> [ObjectIdentifier] {
            [ObjectIdentifier(view)] + view.subviews.flatMap(tree)
        }
        let before = tree(content)
        var during: [ObjectIdentifier]?
        Capture.windowImageOverride = { _ in
            during = tree(content)
            return nil
        }
        let reply = session.agentInstance.screenshot(["path": path.path])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertTrue((reply["note"] as? String)?.contains("glass was not composited") == true, "\(reply)")
        XCTAssertTrue(FileManager.default.fileExists(atPath: path.path))
        XCTAssertEqual(during, before, "the viewport fallback has not inserted a view while requesting its server picture")
        XCTAssertEqual(tree(content), before, "the viewport fallback leaves the view tree unchanged")
        let windowReply = session.agentInstance.screenshot(["path": path.path, "window": true])
        XCTAssertNil(windowReply["error"], "\(windowReply)")
        XCTAssertEqual(tree(content), before, "the window capture leaves the view tree unchanged")
    }

    func testViewportCaptureKeepsTheContentOfAGlassRoot() throws {
        let f = try SheetFixture(page: """
        component App
          view
            main navigationKey="glass" viewport-fit="cover" width="100%" height="100%" background-color="#0000ff"
              column navigationKey="glass" navigationBar="none" position="absolute" inset=0 width="100%" height="100%" backgroundMaterial="glass"
                box width=200 height=100 background-color="#ff0000"
                text "Glass content"

        """)
        defer { f.close() }
        f.view.controlsOverlay = true
        f.window.styleMask.insert(.fullSizeContentView)
        f.window.titlebarAppearsTransparent = true
        f.window.orderFrontRegardless()
        f.window.layoutIfNeeded()
        f.window.displayIfNeeded()
        window = f.window
        CATransaction.flush()
        let glass = try XCTUnwrap(f.presenter.views.values.first { $0.props["backgroundMaterial"] == "glass" })
        XCTAssertEqual(glass.appliedMaterial, "NSGlassEffectView(.regular)")
        let root = f.presenter.navigation.split.agentRoot
        let live = try waitForWindowPicture(of: root) {
            (try? self.color($0, NSPoint(x: 100, y: 50)).redComponent) ?? 0 > 0.8
        }
        let picture = try XCTUnwrap(Capture.windowServerPicture(of: root))
        let red = try color(picture, NSPoint(x: 100, y: 50))
        XCTAssertGreaterThan(red.redComponent, 0.9)
        XCTAssertLessThan(red.greenComponent, 0.3)
        XCTAssertLessThan(red.blueComponent, 0.3)
        try assertColor(live, at: NSPoint(x: 300, y: 300), red: 0 ... 0.7, green: 0 ... 0.7, blue: 0.9 ... 1,
                        "the live glass ground remains blue")
        try assertColor(picture, at: NSPoint(x: 300, y: 300), red: 0 ... 0.7, green: 0 ... 0.7, blue: 0.9 ... 1,
                        "the captured glass ground remains blue")
        try assertPicturesAgree(picture, live, in: NSRect(x: 80, y: 30, width: 40, height: 40))
        try assertPicturesAgree(picture, live, in: NSRect(x: 280, y: 280, width: 40, height: 40))
        let upload = try XCTUnwrap(Capture.bitmap(of: root, scale: root.pixelScale ?? 2))
        try assertRed(upload, at: NSPoint(x: 100, y: 50), "the shared upload compositor keeps the glass's content")
    }

    @available(macOS 26.0, *)
    func testViewportCaptureKeepsAGlassEditorsAndPressesLive() throws {
        let p = presenter(box(1, ["backgroundMaterial": "glass"], w: 200)
            + box(2, ["value": "clean"], kind: "input", w: 160, handlers: ["blur", "change"])
            + [["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]])
        let glass = try XCTUnwrap(p.views[1]?.materialView as? NSGlassEffectView)
        let content = try XCTUnwrap(glass.contentView)
        let marker = GlassCaptureMarker(frame: NSRect(x: 180, y: 20, width: 12, height: 12), ink: .red)
        content.addSubview(marker, positioned: .above, relativeTo: nil)
        let input = try XCTUnwrap(p.views[2]), field = try XCTUnwrap(input.field)
        var events: [String] = []
        p.onBlur = { events.append("blur \($0)") }
        p.onChange = { events.append("change \($0) \($1)") }
        XCTAssertTrue(window.makeFirstResponder(field))
        let editor = try XCTUnwrap(field.currentEditor() as? NSTextView)
        editor.setSelectedRange(NSRange(location: 5, length: 0))
        editor.insertText(" draft", replacementRange: editor.selectedRange())
        editor.setSelectedRange(NSRange(location: 2, length: 3))
        let press = PressNode(input, bubbled: false)

        show()
        let root = try XCTUnwrap(p.views[1])
        let picture = try XCTUnwrap(Capture.windowServerPicture(of: root))
        let markerPoint = marker.convert(NSPoint(x: marker.bounds.midX, y: marker.bounds.midY), to: root)
        let markerColor = try color(picture, markerPoint)
        XCTAssertGreaterThan(markerColor.redComponent, 0.8, "the captured fixture contains its known red marker")
        XCTAssertLessThan(markerColor.greenComponent, 0.3)
        XCTAssertLessThan(markerColor.blueComponent, 0.3)

        let manager = try XCTUnwrap(editor.layoutManager), container = try XCTUnwrap(editor.textContainer)
        func textRect(_ range: NSRange) -> NSRect {
            let glyphs = manager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            var rect = manager.boundingRect(forGlyphRange: glyphs, in: container)
            rect.origin.x += editor.textContainerOrigin.x; rect.origin.y += editor.textContainerOrigin.y
            return editor.convert(rect, to: root).insetBy(dx: -2, dy: -2).intersection(root.bounds)
        }
        let draft = textRect(NSRange(location: 6, length: 5))
        var darkest: CGFloat = 1, lightest: CGFloat = 0
        for y in stride(from: draft.minY, to: draft.maxY, by: 1) {
            for x in stride(from: draft.minX, to: draft.maxX, by: 1) {
                let c = try color(picture, NSPoint(x: x, y: y))
                let luma = 0.2126 * c.redComponent + 0.7152 * c.greenComponent + 0.0722 * c.blueComponent
                darkest = min(darkest, luma); lightest = max(lightest, luma)
            }
        }
        XCTAssertGreaterThan(lightest - darkest, 0.15, "the same captured frame contains the live editor's draft ink")

        XCTAssertTrue(window.firstResponder === editor)
        XCTAssertTrue(field.currentEditor() === editor)
        XCTAssertEqual(editor.string, "clean draft")
        XCTAssertEqual(editor.selectedRange(), NSRange(location: 2, length: 3))
        XCTAssertEqual(events, [])
        XCTAssertTrue(press.isLive)
        XCTAssertTrue(glass.contentView === content)
    }

    func testViewportCaptureKeepsNestedGlassContent() throws {
        let p = presenter([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view", "props": ["backgroundMaterial": "glass"]],
            ["op": "create", "id": 3, "kind": "view", "props": ["backgroundMaterial": "glass"]],
            ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 3, "ids": [4]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 3, "x": 30, "y": 30, "w": 100, "h": 100],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 100, "h": 100],
        ])
        let root = try XCTUnwrap(p.views[1])
        show()
        let live = try waitForWindowPicture(of: root) { (try? self.color($0, NSPoint(x: 60, y: 60)).redComponent) ?? 0 > 0.8 }
        let picture = try XCTUnwrap(Capture.windowServerPicture(of: root))
        try assertColor(live, at: NSPoint(x: 60, y: 60), red: 0.9 ... 1, green: 0 ... 0.15, blue: 0 ... 0.15,
                        "the live nested content is red")
        try assertColor(picture, at: NSPoint(x: 60, y: 60), red: 0.9 ... 1, green: 0 ... 0.15, blue: 0 ... 0.15,
                        "the captured nested content is red")
        try assertPicturesAgree(picture, live, in: NSRect(x: 50, y: 50, width: 20, height: 20))
    }

    func testViewportCaptureKeepsACleanLayerImage() throws {
        let (_, root, image) = try imageInGlass(alpha: 1)
        show()
        let live = try waitForWindowPicture(of: root) { (try? self.color($0, NSPoint(x: 50, y: 50)).redComponent) ?? 0 > 0.8 }
        image.needsDisplay = false
        XCTAssertNotNil(image.imageLayer)
        let picture = try XCTUnwrap(Capture.windowServerPicture(of: root))
        try assertColor(live, at: NSPoint(x: 50, y: 50), red: 0.9 ... 1, green: 0 ... 0.15, blue: 0 ... 0.15,
                        "the live image is red")
        try assertColor(picture, at: NSPoint(x: 50, y: 50), red: 0.9 ... 1, green: 0 ... 0.15, blue: 0 ... 0.15,
                        "the captured image is red")
        try assertPicturesAgree(picture, live, in: NSRect(x: 40, y: 40, width: 20, height: 20))
    }

    func testViewportCaptureLeavesADirtyTranslucentImageUnchangedInTheWindow() throws {
        let (_, root, image) = try imageInGlass(alpha: 0.5)
        let marker = GlassCaptureMarker(frame: NSRect(x: 170, y: 170, width: 20, height: 20), ink: .green)
        root.addSubview(marker, positioned: .above, relativeTo: nil)
        show()
        let before = try waitForWindowPicture(of: root) {
            guard let c = try? self.color($0, NSPoint(x: 50, y: 50)) else { return false }
            return c.redComponent > 0.8 && c.greenComponent > 0.3
        }
        image.needsDisplay = true
        marker.ink = .blue; marker.needsDisplay = true
        let picture = try XCTUnwrap(Capture.windowServerPicture(of: root))
        let markerColor = try color(picture, NSPoint(x: 180, y: 180))
        XCTAssertGreaterThan(markerColor.blueComponent, 0.8, "capture requested after the change contains the committed new frame")
        XCTAssertLessThan(markerColor.redComponent, 0.3)
        XCTAssertLessThan(markerColor.greenComponent, 0.3)
        marker.ink = .magenta; marker.needsDisplay = true; marker.displayIfNeeded()
        CATransaction.flush()
        _ = try waitForWindowPicture(of: root) {
            guard let c = try? self.color($0, NSPoint(x: 180, y: 180)) else { return false }
            return c.blueComponent > 0.8 && c.redComponent > 0.8
        }
        let after = try XCTUnwrap(windowPicture(of: root))
        let area = NSRect(x: 40, y: 40, width: 20, height: 20)
        for (name, image) in [("before", before), ("capture", picture), ("after", after)] {
            try assertColor(image, at: NSPoint(x: 50, y: 50), red: 0.9 ... 1, green: 0.2 ... 0.85, blue: 0.2 ... 0.85,
                            "the \(name) translucent image has its complete red-over-white signature")
        }
        try assertPicturesAgree(picture, before, in: area)
        try assertPicturesAgree(after, before, in: area)
    }

    func testViewportCaptureClipsGlassContentToUnequalCorners() throws {
        let p = presenter([
            ["op": "create", "id": 1, "kind": "view",
             "style": ["background_color": [0, 0, 255, 255], "background_clip": "padding-box"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["backgroundMaterial": "glass"],
             "style": ["border_radius_top_left": 40.0, "border_radius_top_right": 5.0,
                       "border_radius_bottom_right": 30.0, "border_radius_bottom_left": 15.0]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 160, "h": 160],
            ["op": "frame", "id": 2, "x": 20, "y": 20, "w": 100, "h": 100],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 100, "h": 100],
        ])
        let glass = try XCTUnwrap(p.views[2])
        XCTAssertNotNil(glass.materialView?.layer?.mask, "unequal corners use the explicit mask")
        XCTAssertFalse(glass.roundedPath(in: glass.bounds).contains(NSPoint(x: 2, y: 2)))
        let root = try XCTUnwrap(p.views[1])
        XCTAssertTrue(Capture.hasGlass(in: root), "the glass's own shape mask clips it without hiding it")
        show()
        let live = try waitForWindowPicture(of: root) { (try? self.color($0, NSPoint(x: 70, y: 70)).redComponent) ?? 0 > 0.8 }
        let picture = try XCTUnwrap(Capture.windowServerPicture(of: root))
        let scale = CGFloat(picture.pixelsWide) / picture.size.width
        let centre = try XCTUnwrap(picture.paintedColorAt(x: Int(70 * scale), y: Int(70 * scale))?.usingColorSpace(.sRGB))
        let corner = try XCTUnwrap(picture.paintedColorAt(x: Int(22 * scale), y: Int(22 * scale))?.usingColorSpace(.sRGB))
        XCTAssertTrue(centre.redComponent > 0.9 && centre.blueComponent < 0.1, "\(centre)")
        XCTAssertTrue(corner.blueComponent > 0.9 && corner.redComponent < 0.1, "\(corner)")
        try assertPicturesAgree(picture, live, in: NSRect(x: 60, y: 60, width: 20, height: 20))
    }

    /// LLP 1053.000.000.000 D2: an auto group's spacing is the points the
    /// Rust host resolved — the props' at creation, then the style key a
    /// gap change sends — and a number again when the prop is one.
    func testAnAutoGroupTakesItsSpacingFromWhatTheHostResolved() throws {
        let p = presenter(cluster(["glassGroup": "8", "glassGroupAuto": "true"]))
        let group = try XCTUnwrap(p.views[1])
        XCTAssertEqual(spacing(group), 8)
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["text_color": [0, 0, 0, 255], "glass_group_spacing": 20.0]]]))
        XCTAssertEqual(spacing(group), 20, "a gap change sends style, not props")
        var native: [String: Any] = [:]
        group.glassAgentFields(&native)
        XCTAssertEqual((native["glassGroup"] as? [String: Any])?["auto"] as? Bool, true)
        // The flag cleared alone: the prop rules, though a stale key remains.
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["glassGroup": "12"], "clear": ["glassGroupAuto"]]]))
        XCTAssertEqual(spacing(group), 12, "a number again")
        native = [:]
        group.glassAgentFields(&native)
        XCTAssertNil((native["glassGroup"] as? [String: Any])?["auto"], "no longer auto")
        // The key dropped alone, the flag set: the prop it came with.
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["glassGroup": "6", "glassGroupAuto": "true"], "clear": []],
                           ["op": "style", "id": 1, "style": ["text_color": [0, 0, 0, 255]]]]))
        XCTAssertEqual(spacing(group), 6)
    }

    func testAGroupHoldsItsChildrenInnermostAndGivesThemBack() throws {
        let p = presenter(cluster())
        let group = try XCTUnwrap(p.views[1])
        let view = try XCTUnwrap(group.glassGroupView)
        XCTAssertTrue(view.superview === group)
        XCTAssertTrue(group.container === group.glassGroupContent)
        XCTAssertTrue(p.views[2]?.superview === group.container && p.views[3]?.superview === group.container)
        XCTAssertEqual(spacing(group), 12)
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["glassGroup": "4"], "clear": []]]))
        XCTAssertTrue(group.glassGroupView === view)
        XCTAssertEqual(spacing(group), 4)
        p.apply(wireBatch([["op": "props", "id": 1, "set": [:], "clear": ["glassGroup"]]]))
        XCTAssertNil(group.glassGroupView)
        XCTAssertNil(view.superview)
        XCTAssertEqual(group.subviews.compactMap { ($0 as? NodeView)?.id }, [2, 3], "back in order")
    }

    func testAMaterialOnTheNodeWinsAndTheGroupComesBack() throws {
        let p = presenter(cluster())
        let group = try XCTUnwrap(p.views[1])
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["backgroundMaterial": "thin"], "clear": []]]))
        XCTAssertNil(group.glassGroupView)
        XCTAssertTrue(p.views[2]?.superview === group.container)
        p.apply(wireBatch([["op": "props", "id": 1, "set": [:], "clear": ["backgroundMaterial"]]]))
        XCTAssertNotNil(group.glassGroupView)
        XCTAssertTrue(p.views[2]?.superview === group.glassGroupContent)
    }

    func testAGlassMovesIntoItsOwnContainerWhileItsPathFadesOrClips() throws {
        let p = presenter(cluster())
        let inner = try XCTUnwrap(p.views[4]), material = try XCTUnwrap(inner.materialView)
        XCTAssertNil(inner.glassIsolation)
        present(p, 3, 0.5)
        let isolation = try XCTUnwrap(inner.glassIsolation)
        XCTAssertTrue(material.superview?.superview === isolation)
        XCTAssertTrue(isolation.superview === inner)
        var native: [String: Any] = [:]
        inner.glassAgentFields(&native)
        XCTAssertEqual(native["glassGroupOf"] as? String, "#1")
        XCTAssertEqual(native["isolated"] as? [String], ["opacity"])
        XCTAssertEqual(inner.appliedMaterial, "NSGlassEffectView(.regular)", "seen through its isolation")
        present(p, 3, 1)
        XCTAssertNil(inner.glassIsolation)
        XCTAssertTrue(material.superview === inner)
        XCTAssertNil(isolation.superview)
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["overflow_x": "hidden", "overflow_y": "hidden", "text_color": [0, 0, 0, 255]]]]))
        XCTAssertNotNil(inner.glassIsolation, "an ancestor clips")
        // The material changing takes the glass out of its isolation first.
        p.apply(wireBatch([["op": "props", "id": 4, "set": ["backgroundMaterial": "thin"], "clear": []]]))
        XCTAssertNil(inner.glassIsolation)
        XCTAssertTrue(inner.materialView?.superview === inner)
    }

    func testHiddenMaterialContentFollowsItsGlassIsolation() throws {
        let p = presenter(box(1, ["glassGroup": "12"], w: 200) + box(2, ["backgroundMaterial": "glass"], w: 160)
            + box(3, kind: "input", w: 140) + [["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]])
        let node = try XCTUnwrap(p.views[2]), material = try XCTUnwrap(node.materialView)
        let child = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["visibility": "hidden"]]]))
        XCTAssertTrue(child.superview?.superview === material.superview)
        present(p, 2, 0.5)
        XCTAssertNotNil(node.glassIsolation)
        XCTAssertTrue(child.superview?.superview === material.superview, "the parked content moves with its effect")
        XCTAssertFalse(child.isHiddenOrHasHiddenAncestor)
        present(p, 2, 1)
        XCTAssertNil(node.glassIsolation)
        XCTAssertTrue(child.superview?.superview === node)
        p.apply(wireBatch([["op": "style", "id": 2, "style": [:]]]))
        XCTAssertTrue(child.isDescendant(of: material))
    }

    func testAFieldInAGlassKeepsFocusThroughItsIsolation() throws {
        let p = presenter(box(1, ["glassGroup": "12"], w: 200) + box(2, ["backgroundMaterial": "glass"], w: 160)
            + box(3, kind: "input", w: 140) + [["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]])
        let field = try XCTUnwrap(p.views[3]?.field)
        XCTAssertTrue(window.makeFirstResponder(field))
        func editing() -> Bool { (window.firstResponder as? NSTextView)?.delegate as? NSTextField === field }
        XCTAssertTrue(editing())
        present(p, 2, 0.5)
        XCTAssertNotNil(p.views[2]?.glassIsolation)
        XCTAssertTrue(editing(), "isolated")
        present(p, 2, 1)
        XCTAssertNil(p.views[2]?.glassIsolation)
        XCTAssertTrue(editing(), "joined")
        p.apply(wireBatch([["op": "props", "id": 1, "set": [:], "clear": ["glassGroup"]]]))
        XCTAssertTrue(editing(), "the group gone")
    }

    /// Review fix: a clip box coming and going under a group keeps the field
    /// being edited.
    func testAFieldKeepsFocusThroughAClipBox() throws {
        let p = presenter(box(1, ["glassGroup": "8"], w: 200) + box(2, kind: "input", w: 160) + [["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]])
        let group = try XCTUnwrap(p.views[1]), field = try XCTUnwrap(p.views[2]?.field)
        XCTAssertTrue(window.makeFirstResponder(field))
        func editing() -> Bool { (window.firstResponder as? NSTextView)?.delegate as? NSTextField === field }
        let shadowed: [String: Any] = ["border_radius": 20.0, "text_color": [0, 0, 0, 255], "overflow_x": "hidden", "overflow_y": "hidden",
                                       "box_shadow": [["o": [0.0, 2.0], "b": 4.0, "s": 0, "c": [0, 0, 0, 128]]]]
        p.apply(wireBatch([["op": "style", "id": 1, "style": shadowed]]))
        XCTAssertNotNil(group.clipBox)
        XCTAssertTrue(editing(), "into the clip box")
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["border_radius": 20.0, "text_color": [0, 0, 0, 255]]]]))
        XCTAssertNil(group.clipBox)
        XCTAssertTrue(editing(), "out of it")
    }

    /// Review fix: a glass inside an isolated glass answers to that glass's
    /// isolation container, so a clip between them isolates it too.
    func testAGlassInsideAnIsolatedGlassIsIsolatedByItsOwnPath() throws {
        let p = presenter(box(1, ["glassGroup": "12"], w: 200) + box(2, w: 120) + box(3, ["backgroundMaterial": "glass"], w: 120)
            + box(4, w: 40) + box(5, ["backgroundMaterial": "glass"], w: 80)
            + [["op": "children", "id": 4, "ids": [5]], ["op": "children", "id": 3, "ids": [4]], ["op": "children", "id": 2, "ids": [3]],
               ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
               ["op": "present", "id": 2, "property": "opacity", "x": 0.5, "y": 0.0, "w": 0.0, "h": 0.0],
               ["op": "style", "id": 4, "style": ["overflow_x": "hidden", "overflow_y": "hidden", "text_color": [0, 0, 0, 255]]]])
        let outer = try XCTUnwrap(p.views[3]), inner = try XCTUnwrap(p.views[5])
        XCTAssertNotNil(outer.glassIsolation, "the outer glass's path fades")
        XCTAssertNotNil(inner.glassIsolation, "the inner glass's path to the outer's container clips")
        var native: [String: Any] = [:]
        inner.glassAgentFields(&native)
        XCTAssertEqual(native["glassGroupOf"] as? String, "#1", "its group, past the isolation")
    }

    func testTheGroupViewIsNeverAHitTarget() throws {
        let p = presenter(cluster())
        let group = try XCTUnwrap(p.views[1]), parent = try XCTUnwrap(group.superview)
        let empty = group.convert(NSPoint(x: 44, y: 20), to: parent)
        XCTAssertTrue(group.hitTest(empty) === group)
        let hit = try XCTUnwrap(group.hitTest(group.convert(NSPoint(x: 10, y: 20), to: parent))), glass = try XCTUnwrap(p.views[2])
        XCTAssertTrue(hit === glass || hit.isDescendant(of: glass), "\(hit)")
    }
}

private final class GlassCaptureMarker: NSView {
    var ink: NSColor
    init(frame: NSRect, ink: NSColor) { self.ink = ink; super.init(frame: frame) }
    required init?(coder: NSCoder) { nil }
    override func draw(_ dirtyRect: NSRect) { ink.setFill(); bounds.fill() }
}
#endif
