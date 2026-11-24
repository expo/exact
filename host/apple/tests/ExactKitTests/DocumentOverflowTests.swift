import XCTest
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
@testable import ExactKit

final class DocumentOverflowTests: XCTestCase {
    func testACollapsedSubtreeAddsNoTransformedDocumentOverflow() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 120, height: 120)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 120, "h": 120],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 100],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 160, "h": 400],
            ["op": "content", "id": 1, "w": 120, "h": 120],
            ["op": "present", "id": 2, "property": "scale", "x": 1, "y": 0]
        ]))
        let reach = try XCTUnwrap(p.views[1]).documentOverflow(in: p.root)
        XCTAssertEqual(reach.maxX, 120, accuracy: 0.01)
        XCTAssertEqual(reach.maxY, 120, accuracy: 0.01)
    }

    func testPerAxisScaleExtendsOnlyItsOwnAxisAndKeepsLayoutOverflow() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 150, "y": 150, "w": 100, "h": 100],
            ["op": "content", "id": 1, "w": 250, "h": 250],
            ["op": "present", "id": 2, "property": "scale", "x": 3, "y": 0.5]
        ]))
        let root = try XCTUnwrap(p.views[1])
        let reach = root.documentOverflow(in: p.root)
        XCTAssertEqual(reach.maxX, 350, accuracy: 0.01)
        XCTAssertEqual(reach.maxY, 250, accuracy: 0.01)
        p.apply(wireBatch([["op": "present", "id": 2, "property": "scale", "x": -3, "y": 0]]))
        let collapsed = root.documentOverflow(in: p.root)
        XCTAssertTrue(OverflowPaint.finite(collapsed))
        XCTAssertGreaterThanOrEqual(collapsed.maxY, 250)
    }

    func testAFractionalRootEndingAtTheViewportDoesNotAddAScroller() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 500, height: 300)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 103.6875, "y": 0, "w": 396.3125, "h": 80],
            ["op": "content", "id": 1, "w": 396.3125, "h": 80],
        ]))
        let root = try XCTUnwrap(p.views[1])
        XCTAssertEqual(root.documentOverflow(in: p.root).maxX, 500, accuracy: 0.01)
        p.apply(wireBatch([["op": "content", "id": 1, "w": 398.3125, "h": 80]]))
        XCTAssertEqual(root.documentOverflow(in: p.root).maxX, 502, accuracy: 0.01)
    }

    func testTransformedDescendantsExtendTheDocumentUntilAClippingBox() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 150, "w": 100, "h": 100],
            ["op": "content", "id": 1, "w": 200, "h": 250],
            ["op": "present", "id": 2, "property": "translate", "x": 0, "y": 250]
        ]))
        XCTAssertEqual(p.root.frame.height, 500, accuracy: 0.01)
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["overflow_x": "hidden", "overflow_y": "hidden"]]]))
        XCTAssertEqual(p.root.frame.height, 200, accuracy: 0.01)
    }

    func testRootRotationBringsNegativeOverflowIntoTheDocument() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": -100, "w": 100, "h": 100],
            ["op": "content", "id": 1, "w": 200, "h": 200],
            ["op": "present", "id": 1, "property": "rotate", "x": 180]
        ]))
        XCTAssertEqual(p.root.frame.height, 300, accuracy: 0.01)
    }
}
