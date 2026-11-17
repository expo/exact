#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// An authored scroll owns a `scrollFollowEnd` scroller until it settles
/// (LLP 1070.000 §15, expo/exact#370): the follow moves nothing meanwhile,
/// `scrollFollowEnd` turning true in the same batch included.
final class FollowEndAuthoredIOSTests: XCTestCase {
    private var window: UIWindow!

    /// A 200-point scroller over twenty 30-point rows (ids 10…29): its end
    /// is at 400. At `top`, following its end when `follows`, and scrolling
    /// smoothly when `smooth`.
    private func fixture(top: CGFloat, follows: Bool, smooth: Bool = true) -> (Presenter, UIScrollView) {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "view", "props": ["scrollFollowEnd": String(follows)],
             "style": ["overflow_y": "scroll", "scroll_behavior": smooth ? "smooth" : "auto"]],
        ]
        // A prop keeps each row a view, not a flat leaf (LLP 1068 §6.1).
        for i in 0..<20 { ops.append(["op": "create", "id": 10 + i, "kind": "view", "props": ["id": "r\(i)"], "style": ["height": 30.0]]) }
        ops += [
            ["op": "children", "id": 1, "ids": Array(10..<30)],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 200.0],
        ]
        p.apply(wireBatch(ops + rows(20)))
        let sv = p.views[1]!.scroll!
        sv.contentOffset = CGPoint(x: 0, y: top)
        return (p, sv)
    }
    private func rows(_ n: Int) -> [[String: Any]] {
        var ops: [[String: Any]] = []
        for i in 0..<n { ops.append(["op": "frame", "id": 10 + i, "x": 0.0, "y": Double(30 * i), "w": 300.0, "h": 30.0]) }
        ops.append(["op": "content", "id": 1, "w": 300.0, "h": Double(30 * n)])
        return ops
    }

    /// The reporter's handoff: the app turns following on in the batch that
    /// writes a smooth `scrollTop`. The port is not put at the end first; the
    /// write's animation starts from where the reader was.
    func testTurningTheFollowOnWithASmoothWriteDoesNotJumpToTheEndFirst() {
        let (p, sv) = fixture(top: 100, follows: false)
        defer { p.reset() }
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["scrollFollowEnd": "true", "scrollTop": "300"]]]))
        XCTAssertEqual(sv.contentOffset.y, 100, accuracy: 0.5, "no jump to the end ahead of the write")
        XCTAssertTrue(p.views[1]!.authoredScrollAnimating)
        XCTAssertTrue(p.views[1]!.authorOwnsScroll(sv))
    }

    /// While a reveal runs on a follower its end is not kept across an inset
    /// change, nor followed as rows arrive: the reveal heads on for its target.
    func testARunningRevealOwnsAFollowedScroller() {
        let (p, sv) = fixture(top: 400, follows: true)
        defer { p.reset() }
        let node = p.views[1]!
        p.elementScrolls.revealBegan(sv, to: .zero, target: (10, p.views[10]!.incarnation),
                                     realigned: { .zero }, arrived: {})
        XCTAssertTrue(node.authorOwnsScroll(sv))
        sv.contentInset.bottom = 50
        XCTAssertEqual(sv.contentOffset.y, 400, accuracy: 0.5, "the end is not kept under a reveal")
        p.elementScrolls.cutReveal(sv)
        XCTAssertFalse(node.authorOwnsScroll(sv))
    }

    /// Once the authored scroll is over, the follow is the reader's place:
    /// at the end it follows, elsewhere it does not.
    func testWhereItRestsDecidesTheFollow() {
        // Not smooth, so the follow lands at once: a smooth scroller follows
        // its end with UIKit's scroll animation, which this is not about.
        let (p, sv) = fixture(top: 400, follows: true, smooth: false)
        defer { p.reset() }
        var ops: [[String: Any]] = []
        for i in 20..<22 { ops.append(["op": "create", "id": 10 + i, "kind": "view", "props": ["id": "r\(i)"], "style": ["height": 30.0]]) }
        ops.append(["op": "children", "id": 1, "ids": Array(10..<32)])
        p.apply(wireBatch(ops + rows(22)))
        XCTAssertEqual(sv.contentOffset.y, 460, accuracy: 0.5, "at the end, followed")
        sv.contentOffset.y = 200
        ops = [["op": "create", "id": 32, "kind": "view", "props": ["id": "r22"], "style": ["height": 30.0]],
               ["op": "children", "id": 1, "ids": Array(10..<33)]]
        p.apply(wireBatch(ops + rows(23)))
        XCTAssertEqual(sv.contentOffset.y, 200, accuracy: 0.5, "rested above the end, not followed")
    }
}
#endif
