#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// A UIKit list applies a snapshot at once unless its code asks
/// (`apply(_:animatingDifferences: true)`). A list whose rows ask for motion
/// (`animates`: a layout transition or an exit animation) and that already
/// shows rows animates a change in which rows it holds, or in their order: a
/// row slides to its new place and a row that comes or goes fades. Its first
/// snapshot, and a change that keeps the same rows in the same order, apply
/// at once.
/// UIKit, so a simulator runs it:
/// bun host/apple/build.mjs --test --ios --only GroupedListDifferencesIOSTests
final class GroupedListDifferencesIOSTests: GroupedListFixture {
    /// Whether the row's cell has a Core Animation animation in flight.
    func animating(_ p: Presenter, _ id: UInt32) throws -> Bool {
        !(try cell(p, id).layer.animationKeys() ?? []).isEmpty
    }

    /// The fixture's batch that makes the host read the list's model again.
    func reread(_ p: Presenter) {
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["testId": "s0"], "clear": [String]()]]))
    }

    func testTheFirstSnapshotAppliesAtOnce() throws {
        let p = presenter { self.model() }
        XCTAssertFalse(try animating(p, 10))
        XCTAssertFalse(try animating(p, 11))
    }

    func testAReorderedListSlidesItsRowsToTheirNewPlaces() throws {
        var swapped = false
        let p = presenter {
            var m = self.model()
            m.animates = true
            if swapped { m.sections[0].rows.swapAt(0, 1) }
            return m
        }
        _ = try cell(p, 10)
        swapped = true
        reread(p)
        XCTAssertTrue(try animating(p, 10) || animating(p, 11), "the swapped rows slide")
    }

    func testAListWhoseRowsAskNoMotionReordersAtOnce() throws {
        var swapped = false
        let p = presenter {
            var m = self.model()
            if swapped { m.sections[0].rows.swapAt(0, 1) }
            return m
        }
        _ = try cell(p, 10)
        swapped = true
        reread(p)
        XCTAssertFalse(try animating(p, 10), "as a UIKit list's snapshot applies, unless asked")
        XCTAssertFalse(try animating(p, 11))
    }

    func testARowMovedToAnotherSectionSlidesThere() throws {
        var moved = false
        let p = presenter {
            var m = self.model()
            m.animates = true
            if moved {
                let row = m.sections[0].rows.removeFirst()
                m.sections[1].rows.insert(row, at: 0)
            }
            return m
        }
        _ = try cell(p, 10)
        moved = true
        reread(p)
        XCTAssertTrue(try animating(p, 10), "the row slides into the other section")
    }

    func testTheSameRowsInTheSameOrderApplyAtOnce() throws {
        var renamed = false
        let p = presenter {
            var m = self.model()
            m.animates = true
            if renamed { m.sections[0].rows[1].secondary = "Off" }
            return m
        }
        _ = try cell(p, 11)
        renamed = true
        reread(p)
        XCTAssertFalse(try animating(p, 10))
        XCTAssertFalse(try animating(p, 11), "a row whose text changed is configured again, not moved")
    }
}
#endif
