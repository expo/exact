#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

// Exercise either reported occlusion state on a locked Mac, where AppKit reports
// every window as covered. CGWindowListCreateImage still supplies the frame.
private final class CanvasCaptureWindow: NSWindow {
    var covered = false
    override var occlusionState: NSWindow.OcclusionState { covered ? [] : [.visible] }
}

final class AgentCanvasCaptureMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() }

    func testWindowCaptureKeepsCanvasPixelsForEitherOcclusionState() throws {
        _ = NSApplication.shared
        let sessions = (0..<2).map { ExactApp.shared.makeSession(label: "window-canvas-\($0)") }
        defer { sessions.forEach { $0.destroy() } }
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 240, height: 160))
        content.wantsLayer = true
        let window = CanvasCaptureWindow(contentRect: NSRect(x: 160, y: 160, width: 240, height: 160),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = content
        defer { window.close() }
        for (index, session) in sessions.enumerated() {
            let p = session.presenter
            p.viewport.frame = NSRect(x: index * 120, y: 0, width: 120, height: 160)
            content.addSubview(p.viewport)
            p.apply(wireBatch([
                ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
                ["op": "create", "id": 2, "kind": "view", "style": ["overflow_x": "hidden", "overflow_y": "hidden"]],
                ["op": "create", "id": 3, "kind": "canvas"],
                ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [255, 0, 0, 128]]],
                ["op": "create", "id": 5, "kind": "view", "style": ["backdrop_filter": [["blur": 6.0]]]],
                ["op": "create", "id": 6, "kind": "view", "style": ["background_color": index == 0 ? [0, 255, 0, 255] : [0, 0, 255, 255]]],
                ["op": "create", "id": 7, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
                ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3, 6, 7, 4, 5]],
                ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 120, "h": 160],
                ["op": "frame", "id": 2, "x": 10, "y": 20, "w": 80, "h": 100],
                ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 100, "h": 100],
                ["op": "frame", "id": 4, "x": 0, "y": 40, "w": 80, "h": 20],
                ["op": "frame", "id": 5, "x": 0, "y": 0, "w": 80, "h": 40],
                ["op": "frame", "id": 6, "x": 60, "y": 0, "w": 20, "h": 100],
                ["op": "frame", "id": 7, "x": 60, "y": 0, "w": 20, "h": 20],
            ]))
            let canvas = try XCTUnwrap(p.views[3])
            let entry = CanvasesHost.Entry(view: canvas, name: "window-pixels", values: [])
            entry.id = UInt32(index + 1); entry.module = module()
            session.surfaceHost.entries[3] = entry
        }
        let child = NSWindow(contentRect: NSRect(x: window.frame.minX + 100, y: window.frame.minY + 20, width: 20, height: 20),
                             styleMask: [.borderless], backing: .buffered, defer: false)
        child.isReleasedWhenClosed = false; child.backgroundColor = .yellow
        defer { child.close() }
        window.orderFrontRegardless(); window.displayIfNeeded(); CATransaction.flush()
        guard Agent.ownWindowImage(window.windowNumber) != nil else {
            throw XCTSkip("the window server gave no picture (a display asleep or a locked screen)")
        }
        window.addChildWindow(child, ordered: .above)
        child.orderFrontRegardless()
        let backing = try sessions.map { session -> (CALayer, Bool, [CALayer]) in
            let metal = try XCTUnwrap(session.presenter.views[3]?.metal?.layer)
            return (metal, metal.isHidden, metal.superlayer?.sublayers ?? [])
        }
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("window-canvas-\(UUID().uuidString).png")
        defer { try? FileManager.default.removeItem(at: path) }
        for covered in [false, true, false] {
            window.covered = covered
            CATransaction.flush()
            let reply = sessions[0].agentInstance.screenshot(["path": path.path, "window": true])
            XCTAssertNil(reply["error"], "\(reply)")
            XCTAssertEqual(reply["content"] as? String, "painted", "covered=\(covered)")
            let rep = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: path)))
            let scale = CGFloat(rep.pixelsWide) / window.frame.width
            func pixel(_ x: CGFloat, _ y: CGFloat) throws -> NSColor {
                let point = content.convert(NSPoint(x: x, y: content.bounds.height - y), to: nil)
                return try XCTUnwrap(rep.paintedColorAt(x: Int(point.x * scale), y: Int((window.frame.height - point.y) * scale))?.usingColorSpace(.sRGB))
            }
            for (index, x) in [CGFloat(30), 150].enumerated() {
                let top = try pixel(x, 23), bottom = try pixel(x, 105), overlay = try pixel(x, 70)
                XCTAssertGreaterThan(top.redComponent, 0.95, "the readback's top stripe is upright")
                XCTAssertLessThan(top.greenComponent, 0.2)
                XCTAssertLessThan(bottom.redComponent, 0.05, "GPU pixels, covered=\(covered), session=\(index)")
                XCTAssertGreaterThan(index == 0 ? bottom.greenComponent : bottom.blueComponent, 0.95)
                let reference = try pixel(x + 50, 70)
                XCTAssertEqual(overlay.redComponent, reference.redComponent, accuracy: 0.02, "the overlay blends as it does over ordinary paint")
                XCTAssertEqual(overlay.greenComponent, reference.greenComponent, accuracy: 0.02)
                XCTAssertEqual(overlay.blueComponent, reference.blueComponent, accuracy: 0.02)
                let blurred = try pixel(x, 38)
                XCTAssertGreaterThan(blurred.redComponent, 0.2, "the panel blurs the GPU stripe")
                XCTAssertGreaterThan(index == 0 ? blurred.greenComponent : blurred.blueComponent, 0.2, "the window server keeps backdrop filters")
                let referenceBlur = try pixel(x + 50, 38)
                XCTAssertGreaterThan(referenceBlur.redComponent, 0.2)
                XCTAssertGreaterThan(index == 0 ? referenceBlur.greenComponent : referenceBlur.blueComponent, 0.2, "blur over ordinary paint in a window with a canvas")
                let clipped = try pixel(x + 70, 105)
                XCTAssertGreaterThan(clipped.redComponent, 0.95, "the canvas stays clipped to its parent")
                XCTAssertGreaterThan(clipped.greenComponent, 0.95)
                XCTAssertGreaterThan(clipped.blueComponent, 0.95)
            }
            let attached = try pixel(110, 130)
            XCTAssertGreaterThan(attached.redComponent, 0.95, "the attached window stays above the painted content")
            XCTAssertGreaterThan(attached.greenComponent, 0.95)
            XCTAssertLessThan(attached.blueComponent, 0.05)
            for (metal, hidden, layers) in backing {
                XCTAssertEqual(metal.isHidden, hidden, "capture restores the Metal layer")
                XCTAssertEqual(metal.superlayer?.sublayers, layers, "capture removes its temporary layer")
            }
        }
        let original = try XCTUnwrap(Agent.ownWindowImage(window.windowNumber))
        let before = NSBitmapImageRep(cgImage: original)
        let after = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: path)))
        XCTAssertEqual(after.pixelsWide, original.width); XCTAssertEqual(after.pixelsHigh, original.height)
        for (x, y) in [(0, 0), (original.width / 2, 5), (original.width - 1, 0)] {
            let actual = try XCTUnwrap(after.colorAt(x: x, y: y))
            let expected = try XCTUnwrap(before.colorAt(x: x, y: y))
            XCTAssertEqual(actual.alphaComponent, expected.alphaComponent, accuracy: 0.01, "window shape")
            XCTAssertEqual(actual.redComponent, expected.redComponent, accuracy: 0.02, "title bar")
        }
    }

    func testAttachedWindowsUseTheParentCaptureScale() throws {
        func solid(_ color: NSColor, size: CGSize, scale: CGFloat) throws -> CGImage {
            let layer = CALayer()
            layer.bounds = CGRect(origin: .zero, size: size)
            layer.backgroundColor = color.cgColor
            return try XCTUnwrap(Capture.picture(of: layer, size: size, scale: scale))
        }
        let frame = CGRect(x: 40, y: 80, width: 100, height: 100)
        let clipped = CGRect(x: 100, y: 140, width: 50, height: 50)
        let inside = CGRect(x: 60, y: 100, width: 20, height: 20)
        for (parentScale, childScale) in [(CGFloat(2), CGFloat(1)), (1, 2)] {
            let parent = try solid(.white, size: frame.size, scale: parentScale)
            let children = [(clipped, try solid(.yellow, size: clipped.size, scale: childScale)),
                            (inside, try solid(.blue, size: inside.size, scale: childScale))]
            let image = Agent.drawingWindows(children, over: parent, in: frame)
            let rep = NSBitmapImageRep(cgImage: image)
            func pixel(_ x: CGFloat, _ y: CGFloat) throws -> NSColor {
                try XCTUnwrap(rep.colorAt(x: Int(x * parentScale), y: Int(y * parentScale))?.usingColorSpace(.sRGB))
            }
            let yellow = try pixel(95, 5), blue = try pixel(30, 70), white = try pixel(45, 55)
            XCTAssertGreaterThan(yellow.redComponent, 0.95)
            XCTAssertGreaterThan(yellow.greenComponent, 0.95)
            XCTAssertLessThan(yellow.blueComponent, 0.05, "the child is scaled to points and clipped to the parent")
            XCTAssertLessThan(blue.redComponent, 0.05)
            XCTAssertGreaterThan(blue.blueComponent, 0.95)
            XCTAssertGreaterThan(white.redComponent, 0.95, "a higher resolution child keeps its point size")
        }
    }

    // The module boundary returns asymmetric RGBA pixels. Metal's layer has
    // no such pixels for the window server to copy.
    private func module() -> GpuModule {
        GpuModule(create: { _, _, _, _, _ in 1 }, bind: { _, _, _ in 0 },
            render: { _, _, _, _, _ in 0 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, _, _, _, _ in 0 }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 0 }, readback: { id, _, height, scale, _, bytes, length in
                guard let bytes else { return 1 }
                let row = length / Int((height * scale).rounded())
                for offset in stride(from: 0, to: length, by: 4) {
                    let top = offset < row * Int((20 * scale).rounded())
                    bytes[offset] = top ? 255 : 0
                    bytes[offset + 1] = !top && id == 1 ? 255 : 0
                    bytes[offset + 2] = !top && id == 2 ? 255 : 0
                    bytes[offset + 3] = 255
                }
                return 0
            }, child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 }, childrenCount: { _, _ in 0 }, placement: { _, _, _, _ in 0 },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 }, errorPtr: { nil },
            wantsInput: nil, input: nil, messages: nil, published: nil, agent: nil, outPtr: nil)
    }
}
#endif
