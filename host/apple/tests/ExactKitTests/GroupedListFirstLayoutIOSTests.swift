#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// A grouped list laid out for the first time inside an animation UIKit has open, as an inspector's column slides in
/// with its route.
final class GroupedListFirstLayoutIOSTests: GroupedListFixture {
    /// A list no frame has shown lays its cells out at once inside an animation UIKit has open (a column sliding in
    /// with its route), as any view no frame has shown takes its place: its self-sizing cells do not grow from their
    /// estimated size down the list.
    func testAListNoFrameShowedPlacesItsCellsAtOnceInsideAnAnimation() throws {
        let p = presenter { self.model() }
        let l = try list(p)
        XCTAssertFalse(p.showed(try XCTUnwrap(p.views[1])), "no frame has shown the list yet")
        UIView.animate(withDuration: 0.5) {
            l.collection.setNeedsLayout()
            l.collection.layoutIfNeeded()
        }
        XCTAssertFalse(l.collection.visibleCells.isEmpty)
        let moving = l.collection.visibleCells.filter { !($0.layer.animationKeys() ?? []).isEmpty }
        XCTAssertTrue(moving.isEmpty, "\(moving.count) of its cells moved in the animation")
    }
}
#endif
