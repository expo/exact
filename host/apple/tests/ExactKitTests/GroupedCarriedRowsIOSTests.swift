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

    /// A route's column UIKit resizes (a fold, the split's divider) gives its route a new block and
    /// each row a new box and content box in one batch. None changes what a row reads at home, so
    /// the rows stay in their cells, where an animation UIKit has open moves them; a row sent home
    /// is placed at once at its new size while its column still moves.
    func testANewBlockAndARowsNewContentBoxLeaveTheRowsInTheirCells() throws {
        let p = presenter { self.model(custom: true) }
        let row = try XCTUnwrap(p.views[21])
        let shown = try cell(p, 21)
        let resize = wireBatch([["op": "block", "id": 1, "w": 402.0, "h": 874.0, "x": 0.0, "y": 0.0],
                                ["op": "frame", "id": 21, "x": 16.0, "y": 60.0, "w": 340.0, "h": 61.0],
                                ["op": "fieldContent", "id": 21, "rect": [16.0, 0.0, 308.0, 61.0]]])
        host(p).prepare(resize.ops)
        XCTAssertTrue(row.superview === shown.contentView, "a block and a content box are geometry its cell takes")
        p.apply(resize)
        XCTAssertTrue(row.superview === (try cell(p, 21)).contentView, "applied, the row is still in a cell")
        XCTAssertEqual(row.frame.width, 340, accuracy: 0.5, "at its new width")
        host(p).prepare(wireBatch([["op": "style", "id": 21, "style": ["opacity": 0.5]]]).ops)
        XCTAssertFalse(row.superview === shown.contentView, "another op on the row still takes it home")
    }
}
#endif
