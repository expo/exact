#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactImages

final class TextUpdateIOSTests: XCTestCase {
    /// Profile changes reuse coverage in the same UIKit view; complex paint switches to a pixel layer
    func testCoverageInkRedrawsInPlaceAndSwitchesKind() throws {
        let p = Presenter()
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "text", "props": ["text": "Ink"]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 20, "h": 20]]))
        defer { p.reset() }
        let node = try XCTUnwrap(p.views[1])
        let context = try XCTUnwrap(CGContext(data: nil, width: 20, height: 20, bitsPerComponent: 8,
            bytesPerRow: 80, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
        func drawing(_ color: UIColor) -> TextRasterImage {
            TextRasterImage(TextDrawing("HH", color: color.cgColor, baseline: 15, frame: node.bounds), covered: node.bounds)
        }
        // What Core Animation displayed for the layer, rendered: its pixels as RGBA
        func displayed(_ ink: CALayer) -> [[UInt8]] {
            context.clear(node.bounds)
            ink.render(in: context)
            return (0..<400).map { Array(UnsafeBufferPointer(start: bytes + $0 * 4, count: 4)) }
        }
        let key = TextRasterKey(spec: node.paragraphSpec(), size: node.bounds.size, box: node.paragraphBox(), scale: 1)
        let red = drawing(.red)
        node.textRasterKey = key; node.textRasterReady = false
        node.showTextRaster(red, for: key)
        let ink = try XCTUnwrap(node.textRasterLayer, "one-colour ink is drawn by its own layer")
        XCTAssertNil(ink.sublayers, "one-colour ink is one layer, as a label is")
        XCTAssertNil(ink.mask, "no mask")
        let coverageView = try XCTUnwrap(ink.delegate as? CoverageImageView)
        XCTAssertTrue(coverageView.isDescendant(of: node), "UIKit owns the coverage view")
        XCTAssertFalse(coverageView.isUserInteractionEnabled, "text keeps its authored input target")
        XCTAssertFalse(coverageView.isAccessibilityElement, "the authored paragraph owns accessibility")
        ink.displayIfNeeded()
        var shown = displayed(ink)
        XCTAssertTrue(shown.contains([255, 0, 0, 255]), "red glyphs")
        XCTAssertTrue(shown.contains { $0[3] == 0 }, "clear around them")
        CATransaction.begin(); CATransaction.setAnimationDuration(1)
        node.textRasterReady = false
        node.showTextRaster(drawing(.blue), for: key)
        CATransaction.commit()
        XCTAssertTrue(node.textRasterLayer === ink, "a new drawing reaches the existing layer")
        XCTAssertTrue(ink.animationKeys()?.isEmpty ?? true)
        ink.displayIfNeeded()
        shown = displayed(ink)
        XCTAssertTrue(shown.contains([0, 0, 255, 255]), "blue glyphs")
        XCTAssertFalse(shown.contains { $0[0] > 0 }, "none of the red")
        context.clear(node.bounds)
        context.setFillColor(UIColor.black.cgColor)
        context.fill(node.bounds)
        node.textRasterReady = false
        node.showTextRaster(TextRasterImage(try XCTUnwrap(context.makeImage()), frame: node.bounds, covered: node.bounds, headroom: 0), for: key)
        let pixels = try XCTUnwrap(node.textRasterLayer)
        XCTAssertFalse(pixels.delegate is CoverageImageView, "pixels are shown by a plain ink layer")
        XCTAssertNil(ink.superlayer, "switching kinds removes the previous ink")
        XCTAssertEqual(displayed(pixels)[0], [0, 0, 0, 255])
        node.textRasterReady = false
        node.showTextRaster(red, for: key)
        let redrawn = try XCTUnwrap(node.textRasterLayer)
        XCTAssertNil(pixels.superlayer)
        redrawn.displayIfNeeded()
        XCTAssertTrue(displayed(redrawn).contains([255, 0, 0, 255]), "red glyphs again")
    }

    func testWordsReuseTheParagraphsBoxLayer() throws {
        let session = ExactApp.shared.makeSession(label: "word-updates")
        defer { session.destroy() }
        let p = session.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 2200))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { window.isHidden = true; p.reset() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 2200],
        ]))
        var ops: [[String: Any]] = []
        for id in 2...101 {
            ops.append(["op": "create", "id": id, "kind": "text", "props": ["text": "Message \(id)"]])
            ops.append(["op": "frame", "id": id, "x": 0, "y": (id - 2) * 20, "w": 380, "h": 20])
        }
        ops.append(["op": "children", "id": 1, "ids": Array(2...101)])
        p.apply(wireBatch(ops))
        window.layoutIfNeeded()
        let nodes = try (2...101).map { try XCTUnwrap(p.views[UInt32($0)]) }
        nodes.forEach { $0.layer.displayIfNeeded() }
        func pixels(_ node: NodeView) throws -> [UInt8] {
            let c = try XCTUnwrap(CGContext(data: nil, width: 380, height: 20, bitsPerComponent: 8, bytesPerRow: 1520,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            node.layer.render(in: c)
            return Array(UnsafeBufferPointer(start: try XCTUnwrap(c.data).assumingMemoryBound(to: UInt8.self), count: 380 * 20 * 4))
        }
        p.paintVisibleText()
        let firstNode = try XCTUnwrap(nodes.first)
        XCTAssertNotNil(firstNode.text)
        let before = try pixels(firstNode)
        XCTAssertTrue(before.enumerated().contains { $0.offset % 4 == 3 && $0.element > 0 }, "the initial words have ink")
        var invalidations = 0
        let start = CACurrentMediaTime()
        for batch in 0..<16 {
            var updates: [[String: Any]] = []
            for (index, node) in nodes.enumerated() {
                let words = "Replacement \(batch + index + 1)"
                updates += [["op": "props", "id": node.id, "set": ["text": words], "clear": []],
                    ["op": "paragraph", "id": node.id, "runs": [["id": node.id + 1000, "parent": node.id,
                        "paint": true, "props": ["text": words], "style": [:], "handlers": []]]]]
            }
            p.apply(wireBatch(updates))
            for node in nodes {
                if node.layer.needsDisplay() { invalidations += 1 }
                node.layer.displayIfNeeded()
            }
            XCTAssertEqual(firstNode.textRasterKey?.spec.runs.map(\.text).joined(), "Replacement \(batch + 1)")
            XCTAssertTrue(firstNode.textRasterReady, "replacement ink is published in this batch")
        }
        print("D3 real words updates=1600 box_displays=\(invalidations) ms=\((CACurrentMediaTime() - start) * 1000)")
        XCTAssertEqual(invalidations, 0, "words have their own raster; the box has not changed")
        let after = try pixels(firstNode)
        XCTAssertNotEqual(before, after, "the replacement words reach the window's layers")
        XCTAssertTrue(after.enumerated().contains { $0.offset % 4 == 3 && $0.element > 0 })
        let first = try XCTUnwrap(nodes.first)
        first.applyProps(set: ["accessibilityLabel": "Latest message"], clear: [])
        XCTAssertEqual(first.accessibilityLabel, "Latest message")
        p.apply(wireBatch([["op": "style", "id": first.id, "style": ["background_color": [1, 0, 0, 1]]]]))
        XCTAssertTrue(first.layer.needsDisplay(), "a changed box still displays")
        p.apply(wireBatch([["op": "style", "id": first.id, "style": ["background_clip": "text"]]]))
        first.layer.displayIfNeeded()
        p.apply(wireBatch([["op": "paragraph", "id": first.id, "runs": [["id": 2000, "parent": first.id,
            "paint": true, "props": ["text": "Fallback ink"], "style": [:], "handlers": []]]]]))
        XCTAssertFalse(first.canRasterText, "background-clip text uses the drawing fallback")
        XCTAssertTrue(first.layer.needsDisplay(), "fallback text still displays")
    }
}
#endif
