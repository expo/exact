#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// Plain UIKit scrollers keep the reader's content still when a box above
/// the port changes size or is inserted, as CSS scroll anchoring does in
/// Chrome (#138 X23d).
final class ScrollAnchoringIOSTests: XCTestCase {
    private var window: UIWindow!

    /// A 200-point scroller: a 48-point header (id 2), then twenty 30-point
    /// rows (ids 10…29), scrolled to `top`.
    private func fixture(top: CGFloat) -> (Presenter, UIScrollView) {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["id": "header"], "style": ["height": 48.0]],
        ]
        // A prop keeps each row a view, not a flat leaf (LLP 1068 §6.1).
        for i in 0..<20 { ops.append(["op": "create", "id": 10 + i, "kind": "view", "props": ["id": "r\(i)"], "style": ["height": 30.0]]) }
        ops += [
            ["op": "children", "id": 1, "ids": [2] + Array(10..<30)],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 200.0],
        ]
        p.apply(wireBatch(ops + rows(header: 48)))
        let sv = p.views[1]!.scroll!
        sv.contentOffset = CGPoint(x: 0, y: top)
        return (p, sv)
    }
    private func rows(header: Double, inserted: Double = 0) -> [[String: Any]] {
        var ops: [[String: Any]] = [["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 300.0, "h": header]]
        for i in 0..<20 { ops.append(["op": "frame", "id": 10 + i, "x": 0.0, "y": header + inserted + Double(30 * i), "w": 300.0, "h": 30.0]) }
        ops.append(["op": "content", "id": 1, "w": 300.0, "h": header + inserted + 600])
        return ops
    }

    func testAFoldAboveThePortLeavesTheContentStill() {
        let (p, sv) = fixture(top: 200)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["height": 0.0]]] + rows(header: 0)))
        XCTAssertEqual(sv.contentOffset.y, 152)
    }

    func testRetiringNavigationRestoresTheReaderBeforeDeliveringCovers() {
        for followEnd in [false, true] {
            let (p, sv) = fixture(top: 200)
            p.apply(wireBatch([["op": "props", "id": 1, "set": ["scrollFollowEnd": String(followEnd)]]]))
            sv.contentOffset.y = 200
            // The retired navigation had covered the header before this batch.
            p.navigation.covers = [2: .whole]
            p.navigation.delivered = p.navigation.covers
            var deliveries = 0
            p.onCovers = { changes in
                deliveries += 1
                XCTAssertFalse(p.applying)
                XCTAssertEqual(changes.count, 1)
                XCTAssertEqual(changes.first?.0, 2)
                XCTAssertNil(changes.first?.1)
                XCTAssertEqual(sv.contentOffset.y, 152, accuracy: 0.5)
                p.apply(wireBatch(self.rows(header: 0, inserted: 60)))
            }
            // Retirement runs while the batch still holds its scroll captures.
            p.onViewportFit = { p.navigation.reset() }
            defer { p.onViewportFit = nil; p.onCovers = nil; p.reset() }
            p.apply(wireBatch([["op": "props", "id": 1, "set": ["viewportFit": "cover"]]] + rows(header: 0)))
            XCTAssertEqual(deliveries, 1)
            XCTAssertEqual(sv.contentOffset.y, 212, accuracy: 0.5, "followEnd=\(followEnd)")
        }
    }

    func testAReplacementCoverWinsOverTheRetiredNavigationsCover() {
        let (p, _) = fixture(top: 200)
        let replacement = HostCover.inset(top: 20, bottom: 0)
        p.navigation.covers = [2: .whole]
        p.navigation.delivered = p.navigation.covers
        var deliveries: [[(UInt32, HostCover?)]] = []
        p.onCovers = { changes in
            XCTAssertFalse(p.applying)
            deliveries.append(changes)
        }
        p.onViewportFit = {
            p.navigation.reset()
            p.navigation.covers = [2: replacement]
        }
        defer { p.onViewportFit = nil; p.onCovers = nil; p.reset() }
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["viewportFit": "cover"]]]))
        XCTAssertEqual(deliveries.count, 1)
        XCTAssertEqual(deliveries.first?.count, 1)
        XCTAssertEqual(deliveries.first?.first?.0, 2)
        XCTAssertEqual(deliveries.first?.first?.1, replacement)
        p.navigation.reset()
        XCTAssertEqual(deliveries.count, 2, "outside a batch, retirement delivers immediately")
        XCTAssertNil(deliveries.last?.first?.1)
    }

    func testRowsInsertedAboveThePortLeaveTheContentStill() {
        let (p, sv) = fixture(top: 200)
        var ops: [[String: Any]] = []
        for i in 0..<5 { ops.append(["op": "create", "id": 50 + i, "kind": "view", "props": ["id": "n\(i)"], "style": ["height": 40.0]]) }
        ops.append(["op": "children", "id": 1, "ids": [2] + Array(50..<55) + Array(10..<30)])
        for i in 0..<5 { ops.append(["op": "frame", "id": 50 + i, "x": 0.0, "y": 48.0 + Double(40 * i), "w": 300.0, "h": 40.0]) }
        p.apply(wireBatch(ops + rows(header: 48, inserted: 200)))
        XCTAssertEqual(sv.contentOffset.y, 400)
    }

    func testAtTheStartNothingIsAnchored() {
        let (p, sv) = fixture(top: 0)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["height": 100.0]]] + rows(header: 100)))
        XCTAssertEqual(sv.contentOffset.y, 0)
    }

    func testALayoutChangeOnTheAnchorSuppressesTheAdjustment() {
        let (p, sv) = fixture(top: 200)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["height": 0.0]], ["op": "style", "id": 15, "style": ["height": 31.0]]] + rows(header: 0)))
        XCTAssertEqual(sv.contentOffset.y, 200)
    }

    func testSettingOrClearingPopoverSuppressesTheSameBatchsAnchorCorrection() {
        for clearing in [false, true] {
            let (p, sv) = fixture(top: 200)
            if clearing { p.apply(wireBatch([["op": "props", "id": 2, "set": ["popover": "manual"]]])) }
            sv.contentOffset.y = 200
            let prop: [String: Any] = clearing
                ? ["op": "props", "id": 2, "clear": ["popover"]]
                : ["op": "props", "id": 2, "set": ["popover": "manual"]]
            p.apply(wireBatch([prop] + rows(header: 0)))
            XCTAssertEqual(sv.contentOffset.y, 200, clearing ? "back into flow" : "out of flow")
        }
    }

    func testAnExplicitScrollTopWins() {
        let (p, sv) = fixture(top: 200)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["height": 0.0]], ["op": "props", "id": 1, "set": ["scrollTop": "100"]]] + rows(header: 0)))
        XCTAssertEqual(sv.contentOffset.y, 100)
    }

    func testPresentedTransformsDoNotMoveTheScrollOffset() {
        for (property, x, y) in [("translate", 0.0, 20.0), ("scale", 1.5, 0.0), ("rotate", 10.0, 0.0)] {
            let (p, sv) = fixture(top: 200)
            p.apply(wireBatch([["op": "present", "id": 15, "property": property, "x": x, "y": y]]))
            XCTAssertEqual(sv.contentOffset.y, 200, "changing the anchor's \(property) suppresses anchoring")
        }
    }

    func testUnchangedAndUnrelatedPresentationDoesNotSuppressAnchoring() {
        for (id, property, x) in [(15, "translate", 0.0), (15, "scale", 1.0), (15, "rotate", 0.0),
                                  (15, "opacity", 0.5), (17, "translate", 20.0)] {
            let (p, sv) = fixture(top: 200)
            p.apply(wireBatch([["op": "present", "id": id, "property": property, "x": x],
                               ["op": "style", "id": 2, "style": ["height": 0.0]]] + rows(header: 0)))
            XCTAssertEqual(sv.contentOffset.y, 152, "\(property) on \(id) leaves the fold above anchored")
        }
    }

    func testPresentedTransformOnTheAnchorsAncestorSuppressesAnchoring() {
        let (p, sv) = fixture(top: 200)
        p.apply(wireBatch([["op": "create", "id": 100, "kind": "view", "props": ["id": "child"]],
                           ["op": "children", "id": 15, "ids": [100]],
                           ["op": "frame", "id": 100, "x": 0.0, "y": 3.0, "w": 100.0, "h": 20.0]]))
        p.apply(wireBatch([["op": "present", "id": 15, "property": "translate", "y": 20.0]]))
        XCTAssertEqual(sv.contentOffset.y, 200)
    }

    /// A list's `scrollTop` written outside its mounted rows reports from
    /// its scroll callback, and the session applies the batch that answers
    /// at once, inside this batch's restore. Here the first authored scroll
    /// applies that batch; the follow-end scrollers the restore reaches in
    /// any order among the authored ones keep where their readers left them.
    func testABatchAppliedFromAnAuthoredScrollKeepsTheOtherScrollersPlaces() {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        let authored = Array(2..<7), followers = Array(10..<16)
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view"]]
        for id in authored + followers {
            ops.append(["op": "create", "id": id, "kind": "view", "style": ["overflow_y": "scroll"],
                        "props": ["scrollFollowEnd": String(followers.contains(id))]])
        }
        ops += [
            ["op": "children", "id": 1, "ids": authored + followers],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
        ]
        for id in authored + followers {
            ops.append(["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 400.0, "h": 100.0])
            ops.append(["op": "content", "id": id, "w": 400.0, "h": 1000.0])
        }
        p.apply(wireBatch(ops))
        defer { p.onScrolled = nil; p.reset() }
        for id in followers { p.views[UInt32(id)]!.scroll!.contentOffset.y = 200 }
        var nested = false
        p.onScrolled = { id, _, top in
            guard let id, authored.contains(Int(id)), abs(top - 500) < 0.5, !nested else { return }
            nested = true
            XCTAssertTrue(p.applying)
            p.apply(wireBatch([["op": "props", "id": 1, "set": ["id": "root"]]]))
        }
        p.apply(wireBatch(authored.map { ["op": "props", "id": $0, "set": ["scrollTop": "500"]] }))
        XCTAssertTrue(nested)
        for id in authored { XCTAssertEqual(p.views[UInt32(id)]!.scroll!.contentOffset.y, 500, "authored \(id)") }
        for id in followers { XCTAssertEqual(p.views[UInt32(id)]!.scroll!.contentOffset.y, 200, "follower \(id)") }
    }
}
#endif
