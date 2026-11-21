#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// An arranged pane's owed scroll position while UIKit adopts its inset (LLP 1123 D14): written again
/// whenever the adjusted inset moves, held while nothing moves however many passes run, and given up to
/// a newer position, whatever safe area UIKit settles on.
final class ScrollHandoffIOSTests: XCTestCase {
    func testAHandoffWritesAsTheInsetMovesHoldsOtherwiseAndGivesWayToANewerPosition() {
        let scroll = UIScrollView()
        let same = { (a: CGPoint, b: CGPoint) in PixelGrid.same(a, b, in: scroll) }
        var handoff = ScrollHandoff(top: 300, scroll: scroll)
        XCTAssertEqual(handoff.step(offset: .zero, inset: .zero, same: same), .write, "the first pass writes")
        handoff.wrote(offset: CGPoint(x: 0, y: 300), inset: .zero)
        for _ in 0..<3 {
            XCTAssertEqual(handoff.step(offset: CGPoint(x: 0, y: 300), inset: .zero, same: same), .hold, "nothing moved")
        }
        // UIKit adopts the bar's inset, and moves the offset as it does: the owed position is written again
        let bar = UIEdgeInsets(top: 82, left: 0, bottom: 34, right: 0)
        XCTAssertEqual(handoff.step(offset: CGPoint(x: 0, y: 282), inset: bar, same: same), .write)
        handoff.wrote(offset: CGPoint(x: 0, y: 218), inset: bar)
        // A late bar moves the inset again
        let late = UIEdgeInsets(top: 126, left: 0, bottom: 34, right: 0)
        XCTAssertEqual(handoff.step(offset: CGPoint(x: 0, y: 218), inset: late, same: same), .write)
        handoff.wrote(offset: CGPoint(x: 0, y: 174), inset: late)
        XCTAssertEqual(handoff.step(offset: CGPoint(x: 0, y: 174.001), inset: late, same: same), .hold, "UIKit's read-back slop")
        // An authored write, a reveal or a status-bar tap moves the offset under the same inset: the handoff ends
        XCTAssertEqual(handoff.step(offset: CGPoint(x: 0, y: 500), inset: late, same: same), .end)
    }
}
#endif
