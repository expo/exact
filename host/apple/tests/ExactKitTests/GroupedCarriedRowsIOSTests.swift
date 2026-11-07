#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// A custom row stays in its cell through what changes no row: a box moved
/// outside a batch, and a batch's far frames.
final class GroupedCarriedRowsIOSTests: GroupedListFixture {
    /// A box moved outside a batch, as a sheet's bar moving its route's edge while the list
    /// scrolls moves the list (`placeEdge`), leaves the rows in their cells and the authored
    /// sections hidden: sent home, each showed its cell empty and the sections over it for a frame.
    func testABoxMovedOutsideABatchLeavesTheRowsInTheirCells() throws {
        let p = presenter { self.model(custom: true) }
        let row = try XCTUnwrap(p.views[21]), section = try XCTUnwrap(p.views[3])
        let shown = try cell(p, 21)
        XCTAssertTrue(row.superview === shown.contentView)
        p.applyGeometry(wireBatch([["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 800.0]]).ops[0])
        XCTAssertTrue(row.superview === shown.contentView, "the list's box moved, not the row's")
        XCTAssertTrue(section.isHidden, "its authored section stays hidden under the native list")
        p.applyGeometry(wireBatch([["op": "frame", "id": 21, "x": 16.0, "y": 60.0, "w": 354.0, "h": 61.0]]).ops[0])
        XCTAssertTrue(row.superview === shown.contentView, "the row's own box is placed in its cell")
        XCTAssertEqual(row.frame, CGRect(x: 16, y: 0, width: 354, height: 61))
    }

    /// A far frame says where a box goes as its route's edge moves and moves nothing, so its
    /// batch leaves the rows in their cells.
    func testAFarFrameLeavesTheRowsInTheirCells() throws {
        let p = presenter { self.model(custom: true) }
        let row = try XCTUnwrap(p.views[21])
        let shown = try cell(p, 21)
        host(p).prepare(wireBatch([["op": "far", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 900.0, "property": "9 0 0 402 874"],
                                   ["op": "far", "id": 21, "x": 16.0, "y": 80.0, "w": 354.0, "h": 61.0, "property": "9 16 52 354 61"]]).ops)
        XCTAssertTrue(row.superview === shown.contentView)
    }
}
#endif
