#if os(macOS)
import AppKit
import CoreText
import XCTest
@testable import ExactKit

extension TextSelectionMacTests {
    func testSelectionRectanglesFollowColumnFragments() throws {
        let p = fixture(measured: true), node = try XCTUnwrap(p.views[2])
        node.props = ["id": "p", "text": "Alpha\nbeta\ngamma\ndelta"]
        let arial = CTFontDescriptorCreateWithNameAndSize("ArialMT" as CFString, 16)
        session?.text.catalog = [9: [RegisteredFace(family: "Arial", generic: nil, weight: 400, italic: false, descriptor: arial)]]
        p.apply(wireBatch([
            ["op": "style", "id": 2, "style": ["font_family": 9.0, "font_size": 16.0, "line_height": "24px", "white_space": "pre"]],
            ["op": "frame", "id": 2, "x": 8, "y": 29, "w": 220, "h": 48],
            ["op": "fragments", "id": 2, "fragments": [[0, 0, 100, 48, 0, 2, 0, 0], [120, 0, 100, 48, 2, 4, 120, -48]]],
        ]))
        var boxes: [CGRect] = []
        p.onSelectionChange = { _, _, _, _, rects in boxes = rects }
        p.fieldSelections.setSelectionRange(["p", 11, 16])
        let rect = try XCTUnwrap(boxes.first)
        XCTAssertEqual(boxes.count, 1)
        // Chrome: the third line starts the second 100px column, after a 20px gap.
        XCTAssertEqual(rect.minX, 128, accuracy: 1)
        XCTAssertEqual(rect.minY, 32, accuracy: 1)
        XCTAssertEqual(rect.maxY, 49, accuracy: 1)
    }

    /// Chrome's centered flex and grid text uses textOffset while whole.
    /// A column fragment includes placement in its own translation.
    func testAnonymousTextPlacementReachesSelectionAndMouseHits() throws {
        let p = fixture(measured: true), node = try XCTUnwrap(p.views[2])
        let arial = CTFontDescriptorCreateWithNameAndSize("ArialMT" as CFString, 16)
        session?.text.catalog = [9: [RegisteredFace(family: "Arial", generic: nil, weight: 400, italic: false, descriptor: arial)]]
        p.apply(wireBatch([
            ["op": "props", "id": 2, "set": ["id": "p", "text": "Alpha", "textOffset": "40"]],
            ["op": "style", "id": 2, "style": ["font_family": 9.0, "font_size": 16.0, "line_height": "20px", "white_space": "pre"]],
            ["op": "frame", "id": 2, "x": 8, "y": 29, "w": 100, "h": 100],
        ]))
        var boxes: [CGRect] = []
        p.onSelectionChange = { _, _, _, _, rects in boxes = rects }
        p.fieldSelections.setSelectionRange(["p", 0, 5])
        XCTAssertEqual(try XCTUnwrap(boxes.first).minY, 70, accuracy: 1)
        for (width, height, fragments, firstX, firstY, lastX) in [
            (220, 60, [[0, 0, 100, 60, 0, 2, 0, 20], [120, 0, 100, 60, 2, 4, 120, -40]], 0, 20, 120),
            (220, 60, [[0, 0, 100, 60, 0, 2, 0, 10], [120, 0, 100, 40, 2, 4, 120, -40]], 0, 10, 120),
            (340, 40, [[0, 0, 100, 40, 0, 0, 0, 10], [120, 0, 100, 40, 0, 2, 120, 0], [240, 0, 100, 40, 2, 4, 240, -40]], 120, 0, 240),
            (340, 40, [[0, 0, 100, 40, 0, 0, 0, 20], [120, 0, 100, 40, 0, 2, 120, 0], [240, 0, 100, 40, 2, 4, 240, -40]], 120, 0, 240),
        ] {
            p.selection.clear()
            p.apply(wireBatch([
                ["op": "props", "id": 2, "set": ["text": "Alpha\nbeta\ngamma\ndelta"], "clear": ["textOffset"]],
                ["op": "frame", "id": 2, "x": 8, "y": 29, "w": width, "h": height],
                ["op": "fragments", "id": 2, "fragments": fragments],
            ]))
            p.fieldSelections.setSelectionRange(["p", 0, 5])
            XCTAssertEqual(try XCTUnwrap(boxes.first).minY, CGFloat(30 + firstY), accuracy: 1)
            XCTAssertEqual(try XCTUnwrap(boxes.first).minX, CGFloat(8 + firstX), accuracy: 1)
            p.fieldSelections.setSelectionRange(["p", 11, 16])
            XCTAssertEqual(boxes.count, 1)
            XCTAssertEqual(try XCTUnwrap(boxes.first).minX, CGFloat(8 + lastX), accuracy: 1)
            XCTAssertEqual(try XCTUnwrap(boxes.first).minY, 30, accuracy: 1)
            XCTAssertNil(node.paragraphPoint(NSPoint(x: 110, y: 10)))
            p.selection.clear()
            let line = try XCTUnwrap(node.paragraphLayout()?.lines[2])
            let from = CTLineGetOffsetForStringIndex(line, 11, nil), to = CTLineGetOffsetForStringIndex(line, 16, nil)
            let down = node.convert(NSPoint(x: CGFloat(lastX) + from + 0.1, y: 10), to: nil)
            let up = node.convert(NSPoint(x: CGFloat(lastX) + to - 0.1, y: 10), to: nil)
            node.mouseDown(with: mouse(.leftMouseDown, down))
            node.mouseDragged(with: mouse(.leftMouseDragged, up))
            node.mouseUp(with: mouse(.leftMouseUp, up))
            XCTAssertEqual(p.selection.selectedText(), "gamma")
            if firstY == 10 {
                p.selection.clear()
                let caret = node.convert(NSPoint(x: 5, y: 55), to: nil)
                node.mouseDown(with: mouse(.leftMouseDown, caret))
                node.mouseUp(with: mouse(.leftMouseUp, caret))
                XCTAssertLessThan(try XCTUnwrap(p.selection.range(node)).location, 11,
                                  "a caret in the first fragment stays on its assigned lines")
            }
            if height == 40 { XCTAssertNil(node.textOffset(at: NSPoint(x: 5, y: 25))) }
        }
    }

    func testGridItemWidthAndSignedPlacementReachSelectionAndHits() throws {
        let p = fixture(measured: true), node = try XCTUnwrap(p.views[2])
        let arial = CTFontDescriptorCreateWithNameAndSize("ArialMT" as CFString, 16)
        session?.text.catalog = [9: [RegisteredFace(family: "Arial", generic: nil, weight: 400, italic: false, descriptor: arial)]]
        p.apply(wireBatch([
            ["op": "props", "id": 2, "set": ["id": "p", "text": "XXXX XXXX", "textLeft": "25", "textWidth": "50", "textOffset": "30"]],
            ["op": "style", "id": 2, "style": ["font_family": 9.0, "font_size": 16.0, "line_height": "20px"]],
            ["op": "frame", "id": 2, "x": 8, "y": 29, "w": 100, "h": 100],
        ]))
        XCTAssertEqual(node.paragraphBox().minX, 25)
        XCTAssertEqual(node.paragraphBox().width, 50)
        XCTAssertEqual(node.paragraphLayout()?.lines.count, 2)
        var boxes: [CGRect] = []
        p.onSelectionChange = { _, _, _, _, rects in boxes = rects }
        p.fieldSelections.setSelectionRange(["p", 5, 9])
        XCTAssertEqual(try XCTUnwrap(boxes.first).minX, 33, accuracy: 1)
        XCTAssertEqual(try XCTUnwrap(boxes.first).minY, 80, accuracy: 1)
        XCTAssertEqual(node.textOffset(at: CGPoint(x: 26, y: 60)), 5)
        XCTAssertNil(node.textOffset(at: CGPoint(x: 5, y: 60)))
        p.apply(wireBatch([
            ["op": "props", "id": 2, "set": ["text": "one two three four"], "clear": ["textOffset"]],
            ["op": "frame", "id": 2, "x": 8, "y": 29, "w": 220, "h": 60],
            ["op": "fragments", "id": 2, "fragments": [[0, 0, 100, 60, 0, 2, 0, 20], [120, 0, 100, 60, 2, 4, 120, -40]]],
        ]))
        XCTAssertEqual(node.paragraphBox().width, 50)
        XCTAssertEqual(node.paragraphLayout()?.lines.count, 4)
        p.fieldSelections.setSelectionRange(["p", 8, 13])
        XCTAssertEqual(try XCTUnwrap(boxes.first).minX, 153, accuracy: 1)
        XCTAssertEqual(try XCTUnwrap(boxes.first).minY, 30, accuracy: 1)
        XCTAssertEqual(node.textOffset(at: CGPoint(x: 146, y: 10)), 8)
        p.apply(wireBatch([
            ["op": "props", "id": 2, "set": ["text": "X", "textLeft": "0", "textWidth": "100", "textOffset": "-5"]],
            ["op": "fragments", "id": 2, "fragments": []],
            ["op": "frame", "id": 2, "x": 8, "y": 29, "w": 100, "h": 100],
        ]))
        XCTAssertEqual(node.paragraphBox().minY, -5)
        XCTAssertEqual(node.paragraphLayout()?.lines.count, 1)
        p.fieldSelections.setSelectionRange(["p", 0, 1])
        XCTAssertEqual(try XCTUnwrap(boxes.first).minY, 25, accuracy: 1)
    }

}
#endif
