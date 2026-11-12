#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists
/// A grouped list's section card as its author styles it, on UIKit.
final class GroupedCardIOSTests: GroupedListFixture {
    /// A section's `box-shadow` is its card node's, cast by one layer under
    /// the cells from the outline its rows span; none once it has none.
    func testACardsShadowIsCastUnderItsCells() throws {
        let p = presenter {
            var m = self.model()
            m.sections[1].cardView = 5
            return m
        }
        p.apply(wireBatch([["op": "style", "id": 5, "style": ["box_shadow": [["o": [0, 4], "b": 14, "s": 0, "c": [0, 0, 0, 255]]]]]]))
        let l = try list(p)
        func casters() -> [CALayer] {
            l.collection.setNeedsLayout()
            l.collection.layoutIfNeeded()
            return (l.collection.layer.sublayers ?? []).filter { $0.shadowPath != nil }
        }
        let cast = casters()
        XCTAssertEqual(cast.count, 1, "the one section that has a shadow casts it")
        let first = try cell(p, 20), last = try cell(p, 21)
        let outline = first.frame.union(last.frame), frame = try XCTUnwrap(cast.first?.frame)
        for (a, b) in [(frame.minX, outline.minX), (frame.minY, outline.minY), (frame.width, outline.width), (frame.height, outline.height)] {
            XCTAssertEqual(a, b, accuracy: 0.001, "around its rows")
        }
        XCTAssertEqual(cast.first?.shadowRadius, 7, "half the CSS blur, a deviation")
        XCTAssertEqual(cast.first?.shadowOffset, CGSize(width: 0, height: 4))
        XCTAssertEqual(l.collection.layer.sublayers?.first, cast.first, "under every cell")
        p.apply(wireBatch([["op": "style", "id": 5, "style": ["box_shadow": [Any]()]]]))
        XCTAssertTrue(casters().isEmpty, "and gone with it")
    }
}
#endif
