#if os(macOS)
import AppKit
import IOSurface
import XCTest
@testable import ExactKit
@testable import ExactImages
@testable import ExactSurfaces

private var capturedPaintPixels: [[UInt8]] = []
private var capturedPaintRows: [(top: [UInt8], bottom: [UInt8])] = []

/// Red over blue, drawn into its backing store, not a layer's colour.
private final class DrawnHalves: NSView {
    var duringDraw: (() -> Void)?
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        duringDraw?()
        NSColor.red.setFill(); NSRect(x: 0, y: 0, width: bounds.width, height: bounds.height / 2).fill()
        NSColor.blue.setFill(); NSRect(x: 0, y: bounds.height / 2, width: bounds.width, height: bounds.height / 2).fill()
    }
}

final class PaintCaptureMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4

    func testCapturesKeepLogicalOrderAndFocusWhileDrawing() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        root.wantsLayer = true
        let front = DrawnHalves(frame: root.bounds), back = DrawnHalves(frame: root.bounds)
        root.addSubview(front); root.addSubview(back)
        let field = NSTextField(frame: NSRect(x: 10, y: 10, width: 80, height: 24))
        root.addSubview(field)
        let window = NSWindow(contentRect: root.bounds, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = root
        defer { window.close() }
        window.makeKeyAndOrderFront(nil)
        XCTAssertTrue(window.makeFirstResponder(field))
        let editor = try XCTUnwrap(field.currentEditor())
        editor.selectedRange = NSRange(location: 0, length: 0)
        window.displayIfNeeded()
        front.layer?.zPosition = 0.001
        let order = root.subviews
        var observations = 0
        front.duringDraw = {
            observations += 1
            XCTAssertEqual(root.subviews, order, "drawing keeps the order AppKit uses for focus and accessibility")
            XCTAssertTrue(window.firstResponder === editor)
        }
        for capture in [{ Capture.bitmap(of: root, scale: 1) }, { Capture.picture(of: root) }] {
            front.needsDisplay = true
            _ = try XCTUnwrap(capture())
            XCTAssertTrue(window.firstResponder === editor)
            XCTAssertEqual(editor.selectedRange, NSRange(location: 0, length: 0))
            XCTAssertEqual(root.subviews, order)
        }
        XCTAssertGreaterThan(observations, 0, "the order was observed during drawing")
    }

    func testCanvasCapturesRanksFromTheCurrentBatch() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "paint-capture")
        defer { session.destroy() }
        let p = session.presenter
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 100, height: 100),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        defer { window.close() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "canvas"],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "create", "id": 5, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "create", "id": 6, "kind": "view", "style": ["background_color": [255, 255, 255, 255], "border_radius_top_left": 16]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 3, "ids": [6, 4, 5]], ["op": "roots", "ids": [1]],
        ] + (1...6).map { ["op": "frame", "id": $0, "x": 0, "y": 0, "w": 100, "h": 100] }))
        let canvas = try XCTUnwrap(p.views[2])
        window.displayIfNeeded()
        let module = GpuModule(create: { _, _, _, _, _ in 7 }, bind: { _, _, _ in 0 },
            render: { _, _, _, _, _ in 1 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, width, height, bytes, _ in
                if let bytes {
                    let index = (Int(height / 2) * Int(width) + Int(width / 2)) * 4
                    capturedPaintPixels.append(Array(UnsafeBufferPointer(start: bytes + index, count: 4)))
                }
                return 0
            }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 1 }, readback: { _, _, _, _, _, _, _ in 0 },
            child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 }, childrenCount: { _, _ in 0 }, placement: { _, _, _, _ in 0 },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 }, errorPtr: { nil },
            wantsInput: nil, input: nil, messages: nil, published: nil, agent: nil, outPtr: nil)
        let entry = CanvasesHost.Entry(view: canvas, name: "paint", values: Data("[]".utf8))
        entry.id = 7; entry.module = module; entry.through = true
        session.surfaceHost.entries[2] = entry
        capturedPaintPixels = []
        canvas.needsCapture = true
        session.surfaceHost.captureIfNeeded()
        XCTAssertEqual(capturedPaintPixels, [[0, 0, 255, 255]])
        canvas.needsCapture = false; canvas.paintedThisTurn = false
        _ = try XCTUnwrap(Capture.bitmap(of: try XCTUnwrap(p.views[6]), scale: 1, prepared: true))
        XCTAssertFalse(canvas.needsCapture, "redrawing a backing store for capture schedules no further upload")
        // Capture must see pending ranks even inside a nested transaction.
        PaintOrder.begin()
        defer { PaintOrder.end() }
        p.apply(wireBatch([["op": "rank", "id": 4, "rank": 1]]))
        XCTAssertEqual(capturedPaintPixels.count, 2)
        XCTAssertEqual(capturedPaintPixels.last, [255, 0, 0, 255], "the same batch uploads the new front sibling")
        XCTAssertFalse(canvas.needsCapture, "no later recapture is owed")
        p.apply(wireBatch([["op": "rank", "id": 4, "rank": -2]]))
        XCTAssertEqual(capturedPaintPixels.last, [0, 0, 255, 255])
        p.apply(wireBatch([["op": "rank", "id": 4, "rank": 10]]))
        XCTAssertEqual(capturedPaintPixels.last, [255, 0, 0, 255])
        XCTAssertEqual(capturedPaintPixels.count, 4)
    }

    /// A canvas's children reach its surface top row first, as they show
    /// (Caltrain's sky, LLP 1014). In an app's window AppKit flips the page
    /// scroller's layer, and the layer copy the capture draws has no
    /// ancestors to carry that: what a view drew came out upside down.
    func testCanvasChildrenUploadTopRowFirst() throws { try canvasChildrenUpload(firstEach: false) }
    func testCanvasChildrenFirstUploadPerChild() throws { try canvasChildrenUpload(firstEach: true) }

    private func canvasChildrenUpload(firstEach: Bool) throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "paint-orientation")
        defer { session.destroy() }
        let p = session.presenter
        let window = CaptureDisplayWindow(contentRect: NSRect(x: 0, y: 0, width: 100, height: 100), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        // As an app's window holds it: in an unflipped, layer-backed view (`ExactView`).
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        content.wantsLayer = true
        window.contentView = content
        p.viewport.frame = content.bounds
        content.addSubview(p.viewport)
        for v: NSView? in [p.viewport, p.viewport.contentView, p.viewport.documentView] { v?.wantsLayer = true }
        defer { window.close() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "canvas"],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
        ] + (1...3).map { ["op": "frame", "id": $0, "x": 0, "y": 0, "w": 100, "h": 100] }))
        let canvas = try XCTUnwrap(p.views[2])
        if firstEach {
            window.orderFrontRegardless(); window.displayIfNeeded(); CATransaction.flush()
        }
        let drawn = DrawnHalves(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        drawn.wantsLayer = true
        let card = try XCTUnwrap(p.views[3])
        card.addSubview(drawn)
        let field = NSTextField(frame: NSRect(x: 0, y: 30, width: 80, height: 24))
        field.placeholderString = "Search"; card.addSubview(field)
        capturedPaintRows = []
        let module = GpuModule(create: { _, _, _, _, _ in 7 }, bind: { _, _, _ in 0 },
            render: { _, _, _, _, _ in 1 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, width, height, bytes, _ in
                if let bytes {
                    let pixel = { (y: Int) in Array(UnsafeBufferPointer(start: bytes + (y * Int(width) + Int(width / 2)) * 4, count: 4)) }
                    capturedPaintRows.append((pixel(Int(height) / 10), pixel(Int(height) * 9 / 10)))
                }
                return 0
            }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 1 }, readback: { _, _, _, _, _, _, _ in 0 },
            child: { _, _, _, _, _, _, _, _, width, height, bytes, _ in
                if let bytes {
                    let pixel = { (y: Int) in Array(UnsafeBufferPointer(start: bytes + (y * Int(width) + Int(width / 2)) * 4, count: 4)) }
                    capturedPaintRows.append((pixel(Int(height) / 10), pixel(Int(height) * 9 / 10)))
                }
                return 0
            }, childrenCount: { _, _ in 0 }, placement: { _, _, _, _ in 0 },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 }, errorPtr: { nil },
            wantsInput: nil, input: nil, messages: nil, published: nil, agent: nil, outPtr: nil)
        let entry = CanvasesHost.Entry(view: canvas, name: "paint", values: Data("[]".utf8))
        entry.id = 7; entry.module = module; entry.through = true
        session.surfaceHost.entries[2] = entry
        if firstEach {
            entry.each = true
            XCTAssertNil(drawn.layer?.superlayer, "the first per-child upload starts with a detached backing layer")
            XCTAssertNil(field.layer, "the field has not inherited backing before the first per-child upload")
        } else {
            window.orderFrontRegardless(); content.displayIfNeeded(); window.display(); CATransaction.flush()
        }
        XCTAssertEqual(p.viewport.layer?.isGeometryFlipped, true, "AppKit flips the page scroller's layer in an unflipped window")
        window.displays = 0
        canvas.needsCapture = true
        session.surfaceHost.captureIfNeeded()
        XCTAssertEqual(window.displays, firstEach ? 1 : 0, "prepare the whole upload once, only for new layers")
        let uploaded = try XCTUnwrap(capturedPaintRows.last)
        XCTAssertEqual(uploaded.top, [255, 0, 0, 255], "the first row is the top half's")
        XCTAssertEqual(uploaded.bottom, [0, 0, 255, 255])
        entry.each = true; canvas.needsCapture = true
        session.surfaceHost.captureIfNeeded()
        XCTAssertEqual(capturedPaintRows.count, 2)
        XCTAssertEqual(window.displays, firstEach ? 1 : 0, "the second upload needs no window display")
        let child = try XCTUnwrap(capturedPaintRows.last)
        XCTAssertEqual(child.top, [255, 0, 0, 255], "per-child uploads have the same orientation")
        XCTAssertEqual(child.bottom, [0, 0, 255, 255])
    }

    func testBranchLookupWorkIsLinear() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 10, height: 10))
        root.wantsLayer = true
        for i in 0..<1_000 {
            let child = NSView(frame: NSRect(x: i, y: 0, width: 10, height: 10))
            child.wantsLayer = true
            root.addSubview(child)
        }
        var probes = 0
        let copy = Capture.paintOrderLayer(of: root, root: try XCTUnwrap(root.layer), branchProbe: { probes += 1 })
        XCTAssertEqual(copy.sublayers?.map { $0.position.x }, (0..<1_000).map { CGFloat($0) })
        XCTAssertLessThanOrEqual(probes, 1_000, "branch membership work must be linear in children")
    }

    func testIntermediateLayersKeepGeometryAndDrawOnlyOnce() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        root.wantsLayer = true
        let window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = root
        defer { window.close() }
        let presenter = Presenter()
        let drawn = NodeView(id: 1, kind: "view", presenter: presenter)
        drawn.frame = NSRect(x: 10, y: 20, width: 40, height: 40)
        drawn.wantsLayer = true
        root.addSubview(drawn)
        window.orderFront(nil); root.displayIfNeeded(); CATransaction.flush()
        let backing = try XCTUnwrap(root.layer), ink = try XCTUnwrap(drawn.layer)
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        for (y, color) in [(0, NSColor.red), (20, NSColor.blue)] {
            let half = CALayer(); half.frame = CGRect(x: 0, y: y, width: 40, height: 20)
            half.backgroundColor = color.cgColor; ink.addSublayer(half)
        }
        let bridge = CALayer()
        bridge.frame = root.bounds; bridge.bounds.origin = CGPoint(x: 3, y: 7)
        bridge.transform = CATransform3DMakeTranslation(9, 11, 0)
        bridge.isGeometryFlipped = true; bridge.opacity = 0.5
        backing.addSublayer(bridge); bridge.addSublayer(ink)
        for filtered in [false, true] {
            if filtered {
                let shadow: [Double] = [0,0,0,0, 1,6,-1,-3, 0,0,0,0, 0,0,0,0,0, 1,0,0,1]
                drawn.applyStyle(["filter": ["p": .array(shadow.map(BatchValue.number))]])
                let filter = try XCTUnwrap(drawn.boxFilter)
                // Stand in for the GPU's result; this test checks composition,
                // independently of how BoxFilter produces its pixels.
                filter.picture.frame = ink.frame
                filter.picture.backgroundColor = NSColor.red.cgColor
                filter.picture.opacity = 0.5
                ink.mask = filter.hide
                bridge.insertSublayer(filter.picture, above: ink)
                bridge.opacity = 1
            }
            let original = NSBitmapImageRep(cgImage: try XCTUnwrap(Capture.picture(of: backing, size: root.bounds.size, scale: 1)))
            let copy = try XCTUnwrap(Capture.picture(of: root, scale: 1))
            var colored = 0, difference: CGFloat = 0
            for y in 0..<100 {
                for x in 0..<100 {
                    let a = try XCTUnwrap(original.colorAt(x: x, y: y)), b = try XCTUnwrap(copy.colorAt(x: x, y: y))
                    difference = max(difference, abs(b.alphaComponent - a.alphaComponent))
                    if a.alphaComponent > 0.1 {
                        colored += 1
                        difference = max(difference, abs(b.redComponent - a.redComponent), abs(b.blueComponent - a.blueComponent))
                    }
                }
            }
            XCTAssertLessThan(difference, 0.01, "the copy keeps the bridge's flip, bounds, transform and opacity")
            XCTAssertEqual(colored, 1600, "the reference paints the whole 40-by-40 child")
            XCTAssertTrue(ink.superlayer === bridge, "capture leaves the live geometry in place")
            if filtered {
                let peak = (0..<100).flatMap { y in (0..<100).compactMap { x in copy.colorAt(x: x, y: y)?.alphaComponent } }.max()!
                XCTAssertEqual(peak, 0.5, accuracy: 0.01, "a filtered translucent child paints once")
            }
        }
    }

    func testCaptureSamplesAnimatedShapeOpacityTransformAndMask() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        root.wantsLayer = true
        let window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = root
        defer { window.close() }
        let backing = try XCTUnwrap(root.layer)
        backing.backgroundColor = NSColor.white.cgColor
        func frozen(_ layer: CALayer, _ key: String, from: Any, to: Any) {
            let animation = CABasicAnimation(keyPath: key)
            animation.fromValue = from; animation.toValue = to
            animation.duration = 10; animation.speed = 0; animation.timeOffset = 5
            animation.fillMode = .both; animation.isRemovedOnCompletion = false
            layer.add(animation, forKey: key)
        }
        let stroke = CAShapeLayer()
        let path = CGMutablePath(); path.move(to: CGPoint(x: 10, y: 20)); path.addLine(to: CGPoint(x: 90, y: 20))
        stroke.path = path; stroke.strokeColor = NSColor.red.cgColor; stroke.lineWidth = 10
        backing.addSublayer(stroke)
        frozen(stroke, "strokeEnd", from: 0, to: 1)
        let faded = CALayer()
        faded.frame = CGRect(x: 10, y: 40, width: 20, height: 20); faded.backgroundColor = NSColor.blue.cgColor
        backing.addSublayer(faded)
        frozen(faded, "opacity", from: 0, to: 1)
        let moved = CALayer()
        moved.frame = CGRect(x: 10, y: 70, width: 20, height: 20); moved.backgroundColor = NSColor.red.cgColor
        backing.addSublayer(moved)
        frozen(moved, "transform", from: NSValue(caTransform3D: CATransform3DIdentity),
               to: NSValue(caTransform3D: CATransform3DMakeTranslation(80, 0, 0)))
        let masked = CALayer()
        masked.frame = CGRect(x: 70, y: 40, width: 20, height: 20); masked.backgroundColor = NSColor.red.cgColor
        let mask = CALayer(); mask.frame = masked.bounds; mask.backgroundColor = NSColor.black.cgColor
        masked.mask = mask; backing.addSublayer(masked)
        frozen(mask, "opacity", from: 0, to: 1)
        window.orderFront(nil)
        root.displayIfNeeded(); CATransaction.flush()
        let deadline = Date().addingTimeInterval(1)
        while stroke.presentation() == nil && Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
        XCTAssertEqual(try XCTUnwrap(stroke.presentation()).strokeEnd, 0.5, accuracy: 0.01)
        XCTAssertEqual(try XCTUnwrap(faded.presentation()).opacity, 0.5, accuracy: 0.01)
        XCTAssertEqual(try XCTUnwrap(moved.presentation()).transform.m41, 40, accuracy: 0.01)
        XCTAssertEqual(try XCTUnwrap(mask.presentation()).opacity, 0.5, accuracy: 0.01)
        let rep = try XCTUnwrap(Capture.picture(of: root))
        func pixel(_ x: Int, _ y: Int) throws -> NSColor {
            try XCTUnwrap(rep.paintedColorAt(x: x * rep.pixelsWide / 100, y: y * rep.pixelsHigh / 100))
        }
        XCTAssertLessThan(try pixel(20, 20).greenComponent, 0.05)
        XCTAssertGreaterThan(try pixel(80, 20).greenComponent, 0.95, "the stroke is only half drawn")
        XCTAssertEqual(try pixel(20, 50).redComponent, 0.5, accuracy: 0.02, "presented opacity")
        XCTAssertGreaterThan(try pixel(20, 80).greenComponent, 0.95, "the model transform is not drawn")
        XCTAssertLessThan(try pixel(60, 80).greenComponent, 0.05, "presented transform")
        XCTAssertEqual(try pixel(80, 50).greenComponent, 0.5, accuracy: 0.02, "presented mask")
        XCTAssertEqual(stroke.strokeEnd, 1); XCTAssertEqual(faded.opacity, 1)
        XCTAssertEqual(moved.transform.m41, 0); XCTAssertEqual(mask.opacity, 1)
        XCTAssertTrue(stroke.superlayer === backing)
    }

    func testRasterizedTextShadowsReachPastTheInkInBothCaptures() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "capture-text-shadow")
        defer { session.destroy() }
        let p = session.presenter
        p.viewport.frame = NSRect(x: 0, y: 0, width: 640, height: 180)
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = p.viewport
        defer { window.close() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Ink"],
             "style": ["font_size": 32, "text_color": [0, 0, 0, 255],
                       "text_shadow": ["o": [350, 40], "b": 6, "c": [255, 0, 0, 128]]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 640, "h": 180],
            ["op": "frame", "id": 2, "x": 30, "y": 20, "w": 320, "h": 60],
        ]))
        let node = try XCTUnwrap(p.views[2]), root = try XCTUnwrap(p.views[1])
        XCTAssertTrue(node.canRasterText)
        XCTAssertTrue(p.textRasters.ensure(node, urgent: true))
        window.displayIfNeeded()
        for scale in [CGFloat(1), 2] {
            session.pageZoom = Double(scale / window.backingScaleFactor)
            window.displayIfNeeded()
            XCTAssertEqual(try XCTUnwrap(root.pixelScale), scale, accuracy: 0.001)
            // A fresh raster avoids testing asynchronous replacement here.
            node.dropTextRaster()
            XCTAssertTrue(p.textRasters.ensure(node, urgent: true))
            let ink = try XCTUnwrap(node.textRasterOverflowLayer)
            XCTAssertNotNil(ink.contents); XCTAssertNil(ink.shadowPath)
            XCTAssertEqual(ink.shadowOpacity, 1)
            let surface = try XCTUnwrap(node.textRaster)
            let source = CapturePixels.bytes(TextRasterImage(surface, frame: .zero, covered: .zero, headroom: 0))
            var weight: CGFloat = 0, expected = CGPoint.zero
            for index in stride(from: 0, to: source.count, by: 4) {
                let alpha = CGFloat(source[index + 3]) / 255
                weight += alpha
                expected.x += (CGFloat(index / 4 % surface.width) + 0.5) * alpha
                expected.y += (CGFloat(index / 4 / surface.width) + 0.5) * alpha
            }
            XCTAssertGreaterThan(weight, 0)
            expected.x = expected.x / weight / ink.contentsScale + node.frame.minX + ink.frame.minX
            expected.y = expected.y / weight / ink.contentsScale + node.frame.minY + ink.frame.minY
            for capture in [{ Capture.picture(of: root, scale: scale) }, { Capture.bitmap(of: root, scale: scale) }] {
                let rep = try XCTUnwrap(capture())
                var darkInk = 0, redShadow = 0
                var inkWeight: CGFloat = 0, shadowWeight: CGFloat = 0
                var inkCenter = CGPoint.zero, shadowCenter = CGPoint.zero
                for y in 0..<rep.pixelsHigh {
                    for x in 0..<rep.pixelsWide {
                        let color = try XCTUnwrap(rep.colorAt(x: x, y: y))
                        let alpha = 1 - color.redComponent, cast = max(0, color.redComponent - color.greenComponent)
                        inkWeight += alpha; shadowWeight += cast
                        inkCenter.x += (CGFloat(x) + 0.5) / scale * alpha
                        inkCenter.y += (CGFloat(y) + 0.5) / scale * alpha
                        shadowCenter.x += (CGFloat(x) + 0.5) / scale * cast
                        shadowCenter.y += (CGFloat(y) + 0.5) / scale * cast
                        if color.redComponent < 0.2 { darkInk += 1 }
                        if CGFloat(x) / scale > node.frame.maxX, color.redComponent > 0.95, color.greenComponent < 0.9 {
                            redShadow += 1
                            XCTAssertGreaterThan(color.greenComponent, 0.45, "shadow alpha is applied once")
                        }
                    }
                }
                XCTAssertGreaterThan(darkInk, 50, "the raster's ink is retained")
                XCTAssertGreaterThan(redShadow, 50, "the glyph shadow extends beyond its layer bounds")
                XCTAssertGreaterThan(shadowWeight, 0)
                XCTAssertEqual(inkCenter.x / inkWeight, expected.x, accuracy: 0.5)
                XCTAssertEqual(inkCenter.y / inkWeight, expected.y, accuracy: 0.5, "the raster stays upright")
                XCTAssertEqual(shadowCenter.x / shadowWeight, expected.x + 350, accuracy: 1)
                XCTAssertEqual(shadowCenter.y / shadowWeight, expected.y + 40, accuracy: 1, "the shadow follows the glyph alpha")
                XCTAssertTrue(node.textRasterOverflowLayer === ink)
                XCTAssertEqual(ink.shadowOpacity, 1, "capture leaves the live shadow intact")
            }
        }
    }

    func testShadowRasterizersKeepBlurInPointsAtEveryScale() throws {
        let caster = CALayer()
        caster.bounds = CGRect(x: 0, y: 0, width: 100, height: 100)
        caster.shadowPath = CGPath(rect: CGRect(x: 25, y: 25, width: 30, height: 30), transform: nil)
        caster.shadowColor = NSColor.black.cgColor; caster.shadowOpacity = 1
        caster.shadowOffset = CGSize(width: 15, height: 10); caster.shadowRadius = 4
        let ink = CALayer()
        ink.bounds = CGRect(x: 0, y: 0, width: 30, height: 30)
        let shape = CALayer(); shape.frame = ink.bounds; shape.backgroundColor = NSColor.black.cgColor
        ink.contents = try XCTUnwrap(Capture.picture(of: shape, size: ink.bounds.size, scale: 1))
        ink.shadowColor = caster.shadowColor; ink.shadowOpacity = 1
        ink.shadowOffset = CGSize(width: 50, height: 10); ink.shadowRadius = 4
        for scale in [CGFloat(1), 2, 2.4] {
            let box = NSBitmapImageRep(cgImage: try XCTUnwrap(Capture.shadowPicture(of: caster, flipped: true, scale: scale)))
            let glyph = try XCTUnwrap(Capture.contentShadowPicture(of: ink, flipped: true, scale: scale))
            let contents = try XCTUnwrap(glyph.contents as! CGImage?)
            let text = NSBitmapImageRep(cgImage: contents)
            func alpha(_ rep: NSBitmapImageRep, _ point: CGPoint, in bounds: CGRect) throws -> CGFloat {
                try XCTUnwrap(rep.colorAt(x: Int((point.x - bounds.minX) * scale), y: Int((point.y - bounds.minY) * scale))).alphaComponent
            }
            // Four points past the hard edge is within the penumbra of a blur
            // of eight points at any raster scale; ten points is outside.
            for (rep, bounds, edge) in [(box, caster.bounds, CGPoint(x: 70, y: 50)),
                                        (text, glyph.frame, CGPoint(x: 80, y: 25))] {
                XCTAssertGreaterThan(try alpha(rep, CGPoint(x: edge.x - 10, y: edge.y), in: bounds), 0.95)
                XCTAssertGreaterThan(try alpha(rep, CGPoint(x: edge.x + 4, y: edge.y), in: bounds), 0.03)
                XCTAssertLessThan(try alpha(rep, CGPoint(x: edge.x + 4, y: edge.y), in: bounds), 0.3)
                XCTAssertLessThan(try alpha(rep, CGPoint(x: edge.x + 10, y: edge.y), in: bounds), 0.01)
            }
        }
    }

    /// An iframe's snapshot stands in for its remote layers in a capture,
    /// unless the node is `visibility: hidden`, as the window shows it.
    func testGuestSnapshotsOfHiddenNodesStayOutOfCaptures() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "capture-hidden-guest")
        defer { session.destroy() }
        let p = session.presenter
        p.viewport.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = p.viewport
        defer { window.close() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "style": ["visibility": "hidden"]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 100, "h": 100],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 50, "h": 100],
            ["op": "frame", "id": 3, "x": 50, "y": 0, "w": 50, "h": 100],
        ]))
        window.displayIfNeeded()
        let red = NSImage(size: NSSize(width: 50, height: 100), flipped: false) { rect in
            NSColor.red.setFill(); rect.fill(); return true
        }
        for id in [UInt32(2), 3] { Capture.web[ObjectIdentifier(try XCTUnwrap(p.views[id]))] = red }
        defer { Capture.web = [:] }
        let rep = try XCTUnwrap(Capture.picture(of: try XCTUnwrap(p.views[1]), scale: 1))
        let shown = try XCTUnwrap(rep.colorAt(x: 25, y: 50)), hidden = try XCTUnwrap(rep.colorAt(x: 75, y: 50))
        XCTAssertLessThan(shown.greenComponent, 0.05, "a visible node draws its guest's snapshot")
        XCTAssertGreaterThan(hidden.greenComponent, 0.95, "a hidden node's snapshot is not drawn")
    }

    /// A small label is drawn by AppKit, not rastered, and a capture draws it
    /// again into pixels of its own. Its `text-shadow` keeps its offset and
    /// blur in points at a 2x device scale, as a 2x display's backing store
    /// has them.
    func testDrawnTextShadowsKeepPointsWhenDrawnAgain() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "capture-drawn-text-shadow")
        defer { session.destroy() }
        let p = session.presenter
        p.viewport.frame = NSRect(x: 0, y: 0, width: 200, height: 80)
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = p.viewport
        defer { window.close() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Ink"],
             "style": ["font_size": 16, "text_color": [0, 0, 0, 255],
                       "text_shadow": ["o": [40, 2], "b": 6, "c": [255, 0, 0, 255]]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 80],
            ["op": "frame", "id": 2, "x": 20, "y": 20, "w": 100, "h": 36],
        ]))
        let node = try XCTUnwrap(p.views[2]), root = try XCTUnwrap(p.views[1])
        /// The shadow's centre less the ink's, and the shadow's horizontal
        /// variance less the ink's (the blur's), in points.
        func measure(_ rep: NSBitmapImageRep, scale: CGFloat) throws -> (offset: CGPoint, blur: CGFloat) {
            var weights = (ink: CGFloat(0), cast: CGFloat(0))
            var sums = (ink: CGPoint.zero, cast: CGPoint.zero), squares = (ink: CGFloat(0), cast: CGFloat(0))
            for y in 0..<rep.pixelsHigh {
                for x in 0..<rep.pixelsWide {
                    let color = try XCTUnwrap(rep.colorAt(x: x, y: y))
                    let ink = 1 - color.redComponent, cast = max(0, color.redComponent - color.greenComponent)
                    let point = CGPoint(x: (CGFloat(x) + 0.5) / scale, y: (CGFloat(y) + 0.5) / scale)
                    weights.ink += ink; weights.cast += cast
                    sums.ink.x += point.x * ink; sums.ink.y += point.y * ink
                    sums.cast.x += point.x * cast; sums.cast.y += point.y * cast
                    squares.ink += point.x * point.x * ink; squares.cast += point.x * point.x * cast
                }
            }
            XCTAssertGreaterThan(weights.ink, 10); XCTAssertGreaterThan(weights.cast, 10)
            let ink = CGPoint(x: sums.ink.x / weights.ink, y: sums.ink.y / weights.ink)
            let cast = CGPoint(x: sums.cast.x / weights.cast, y: sums.cast.y / weights.cast)
            let spread = (squares.cast / weights.cast - cast.x * cast.x) - (squares.ink / weights.ink - ink.x * ink.x)
            return (CGPoint(x: cast.x - ink.x, y: cast.y - ink.y), spread)
        }
        var blurs: [CGFloat] = []
        for scale in [CGFloat(1), 2] {
            session.pageZoom = Double(scale / window.backingScaleFactor)
            window.displayIfNeeded()
            XCTAssertEqual(try XCTUnwrap(root.pixelScale), scale, accuracy: 0.001)
            let layer = try XCTUnwrap(node.layer)
            XCTAssertNil(node.textRaster, "the label is not rastered")
            for capture in [{ Capture.picture(of: root, scale: scale) }, { Capture.bitmap(of: root, scale: scale) }] {
                let shot = try measure(try XCTUnwrap(capture()), scale: scale)
                XCTAssertEqual(shot.offset.x, 40, accuracy: 1, "at \(scale)x")
                XCTAssertEqual(shot.offset.y, 2, accuracy: 1, "at \(scale)x")
                blurs.append(shot.blur)
            }
            XCTAssertTrue(Capture.borrowsBackingStore(layer) || layer.sublayers?.contains(where: Capture.borrowsBackingStore) == true,
                          "AppKit draws the label at \(scale)x: the case this covers")
        }
        XCTAssertEqual(blurs.min() ?? 0, blurs.max() ?? 0, accuracy: max(2, (blurs.max() ?? 0) * 0.2), "the blur is in points: \(blurs)")
    }

    /// A box AppKit draws (a text box's background) with a lone `drop-shadow()`
    /// casts from its backing store. The capture draws the box again, once,
    /// and casts the shadow from that drawing, at a page zoom too.
    func testDrawnBoxDropShadowCastsFromItsOwnDrawing() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "capture-drawn-drop-shadow")
        defer { session.destroy() }
        let p = session.presenter
        p.viewport.frame = NSRect(x: 0, y: 0, width: 200, height: 80)
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = p.viewport
        defer { window.close() }
        let (dx, sigma) = (60.0, 2.0), reach = 3 * sigma
        let region = [-reach, -reach, 2 * reach + dx, 2 * reach]
        let program = region + [1, 6, -1, -3] + region + [0, sigma, sigma, dx, 0] + [0, 0, 1, 1]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": " "],
             "style": ["background_color": [255, 0, 0, 128], "filter": ["p": program]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 80],
            ["op": "frame", "id": 2, "x": 10, "y": 10, "w": 40, "h": 40],
        ]))
        let box = try XCTUnwrap(p.views[2]), root = try XCTUnwrap(p.views[1])
        for scale in [CGFloat(1), 1.2, 2.4] {
            session.pageZoom = Double(scale / window.backingScaleFactor)
            window.displayIfNeeded()
            p.boxFilters!.render()
            window.displayIfNeeded()
            let layer = try XCTUnwrap(box.layer)
            for capture in [{ Capture.picture(of: root, scale: scale) }, { Capture.bitmap(of: root, scale: scale) }] {
                let rep = try XCTUnwrap(capture())
                func pixel(_ x: CGFloat, _ y: CGFloat) throws -> NSColor {
                    try XCTUnwrap(rep.colorAt(x: Int(x * scale), y: Int(y * scale)))
                }
                let fill = try pixel(35, 35)
                XCTAssertEqual(fill.greenComponent, 0.5, accuracy: 0.05, "the translucent box composites once at \(scale)x")
                let shadow = try pixel(95, 30)
                XCTAssertGreaterThan(shadow.blueComponent - shadow.redComponent, 0.3, "the shadow is cast at \(scale)x")
                XCTAssertGreaterThan(try pixel(150, 30).redComponent, 0.95, "nothing past the shadow at \(scale)x")
            }
            XCTAssertTrue(Capture.borrowsBackingStore(layer), "AppKit draws the box: the case this covers")
            XCTAssertGreaterThan(layer.shadowOpacity, 0, "the box's own layer casts the shadow")
        }
    }

    func testRanksNegativeChildrenAndOpacityAreComposedOnce() throws {
        _ = NSApplication.shared
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 80, "h": 80],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 80, "h": 80],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 80, "h": 80],
            ["op": "rank", "id": 2, "rank": 1],
        ]))
        let root = try XCTUnwrap(p.views[1])
        let window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = root
        window.displayIfNeeded(); root.displayIfNeeded()
        let liveParent = p.views[2]?.layer?.superlayer
        func color() throws -> NSColor {
            let rep = try XCTUnwrap(Capture.picture(of: root))
            return try XCTUnwrap(rep.colorAt(x: rep.pixelsWide / 2, y: rep.pixelsHigh / 2))
        }
        XCTAssertGreaterThan(try color().redComponent, 0.95)
        p.views[2]?.frame = CGRect(x: 0, y: 0, width: 80, height: 20)
        let oriented = try XCTUnwrap(Capture.picture(of: root))
        XCTAssertGreaterThan(try XCTUnwrap(oriented.colorAt(x: 10, y: 10)).redComponent, 0.95)
        XCTAssertGreaterThan(try XCTUnwrap(oriented.colorAt(x: 10, y: oriented.pixelsHigh - 10)).blueComponent, 0.95)
        p.views[2]?.frame = root.bounds
        p.views[2]?.alphaValue = 0.5
        let mixed = try color()
        XCTAssertEqual(mixed.redComponent, 0.5, accuracy: 0.02)
        XCTAssertEqual(mixed.blueComponent, 0.5, accuracy: 0.02)
        p.views[2]?.setRank(-2)
        XCTAssertGreaterThan(try color().blueComponent, 0.95)
        p.views[3]?.isHidden = true
        let negative = try color()
        XCTAssertEqual(negative.redComponent, 1, accuracy: 0.02)
        XCTAssertEqual(negative.greenComponent, 0.5, accuracy: 0.02, "negative child still paints above its parent's background")
        XCTAssertEqual(root.subviews.compactMap { ($0 as? NodeView)?.id }, [2, 3])
        XCTAssertTrue(p.views[2]?.layer?.superlayer === liveParent, "capture leaves live layers in place")
    }

    func testCaptureOverlaysOnlyAnimatedPropertiesOnSameTurnModelWrites() throws {
        _ = NSApplication.shared
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        root.wantsLayer = true
        let window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = root
        defer { window.close() }
        let backing = try XCTUnwrap(root.layer)
        backing.backgroundColor = NSColor.white.cgColor
        func animate(_ layer: CALayer, _ keyPath: String, grouped: Bool = false) {
            let animation = CABasicAnimation(keyPath: keyPath)
            animation.fromValue = 0; animation.toValue = 1; animation.duration = 10
            let frozen: CAAnimation
            if grouped {
                let group = CAAnimationGroup(); group.animations = [animation]; frozen = group
            } else { frozen = animation }
            frozen.duration = 10; frozen.speed = 0; frozen.timeOffset = 5
            frozen.fillMode = .both; frozen.isRemovedOnCompletion = false
            // An animation's storage key need not be its property's key path.
            layer.add(frozen, forKey: "capture-test")
        }
        func line(at y: CGFloat) -> CGPath {
            let path = CGMutablePath()
            path.move(to: CGPoint(x: 10, y: y)); path.addLine(to: CGPoint(x: 90, y: y))
            return path
        }
        let stroke = CAShapeLayer()
        stroke.path = line(at: 10); stroke.strokeColor = NSColor.red.cgColor; stroke.lineWidth = 10
        backing.addSublayer(stroke); animate(stroke, "strokeEnd")
        let faded = CALayer()
        faded.anchorPoint = .zero; faded.position = CGPoint(x: 10, y: 40)
        faded.bounds = CGRect(x: 0, y: 0, width: 10, height: 20)
        faded.backgroundColor = NSColor.red.cgColor
        backing.addSublayer(faded); animate(faded, "opacity", grouped: true)
        let masked = CALayer()
        masked.frame = CGRect(x: 10, y: 70, width: 80, height: 20)
        masked.backgroundColor = NSColor.blue.cgColor
        let mask = CAShapeLayer()
        mask.path = CGPath(rect: CGRect(x: 0, y: 0, width: 10, height: 20), transform: nil)
        masked.mask = mask; backing.addSublayer(masked); animate(mask, "opacity", grouped: true)
        window.orderFront(nil)
        root.displayIfNeeded(); CATransaction.flush()
        let deadline = Date().addingTimeInterval(1)
        while stroke.presentation() == nil && Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
        XCTAssertEqual(try XCTUnwrap(stroke.presentation()).strokeEnd, 0.5, accuracy: 0.01)
        XCTAssertEqual(try XCTUnwrap(faded.presentation()).opacity, 0.5, accuracy: 0.01)
        XCTAssertEqual(try XCTUnwrap(mask.presentation()).opacity, 0.5, accuracy: 0.01)

        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        stroke.path = line(at: 25)
        faded.backgroundColor = NSColor.blue.cgColor
        faded.bounds.size.width = 80
        mask.path = CGPath(rect: masked.bounds, transform: nil)
        // Deliberately capture before committing: presentation still has the
        // old path, red fill and narrow bounds, but the active opacity/stroke
        // values must still come from it (also for a mask and animation group).
        for capture in [
            { Capture.bitmap(of: root, scale: 1) },
            { Capture.picture(of: root) },
        ] {
            let rep = try XCTUnwrap(capture())
            func pixel(_ x: Int, _ y: Int) throws -> NSColor {
                try XCTUnwrap(rep.paintedColorAt(x: x * rep.pixelsWide / 100, y: y * rep.pixelsHigh / 100))
            }
            XCTAssertGreaterThan(try pixel(20, 10).greenComponent, 0.95, "the old path is gone")
            XCTAssertLessThan(try pixel(20, 25).greenComponent, 0.05, "the fresh path is drawn")
            XCTAssertGreaterThan(try pixel(80, 25).greenComponent, 0.95, "strokeEnd stays half drawn")
            for y in [50, 80] {
                let color = try pixel(70, y)
                XCTAssertEqual(color.redComponent, 0.5, accuracy: 0.02, "fresh geometry with presented opacity at y=\(y)")
                XCTAssertEqual(color.greenComponent, 0.5, accuracy: 0.02)
                XCTAssertGreaterThan(color.blueComponent, 0.95, "fresh blue fill")
            }
        }
        XCTAssertEqual(stroke.strokeEnd, 1); XCTAssertEqual(faded.opacity, 1); XCTAssertEqual(mask.opacity, 1)
        XCTAssertTrue(stroke.superlayer === backing); XCTAssertTrue(masked.mask === mask)
    }
}
#endif
