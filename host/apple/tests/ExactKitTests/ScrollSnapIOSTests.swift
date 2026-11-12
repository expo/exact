#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1008: nearest projected rest on both axes; oversized areas remain freely scrollable.
final class ScrollSnapIOSTests: XCTestCase {
    func testVirtualizedFlingReachesBeyondTheMountedWindow() throws {
        let session = ExactApp.shared.makeSession(label: "virtual-snap")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: CGSize(width: 300, height: 100)).error)
        let p = session.presenter
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "strip" })
        let scroll = try XCTUnwrap(node.scroll)
        let snapshot = try XCTUnwrap(p.collections.entries[node.id]?.snapshot)
        XCTAssertLessThan(snapshot.rows.map(\.index).max() ?? 1000, 10)
        let (eager, eagerScroll) = try strip((0..<300).map { ("start", Double($0) * 150, 150) })
        eagerScroll.contentSize.width = 45000
        for projection: CGFloat in [1380, 30020] {
            var target = CGPoint(x: projection, y: 0)
            node.scrollViewWillEndDragging(scroll, withVelocity: CGPoint(x: 5, y: 0), targetContentOffset: &target)
            XCTAssertEqual(target.x, (projection / 150).rounded() * 150, accuracy: 0.01)
            XCTAssertEqual(target.x, rest(eager, eagerScroll, from: projection), "virtualization keeps the eager strip's target")
        }
    }

    func testMountedAndUnmountedRelativeRootsUseTheSameDisplacement() throws {
        let session = ExactApp.shared.makeSession(label: "relative-snap")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: CGSize(width: 300, height: 500)).error)
        let p = session.presenter
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "relative" })
        let scroll = try XCTUnwrap(node.scroll)
        let mounted = rest(node, scroll, from: 20)
        XCTAssertEqual(mounted, 30, accuracy: 0.01)
        XCTAssertEqual(rest(node, scroll, from: 30020) - 30000, mounted, accuracy: 0.01)
    }

    func testWindowEdgeTieKeepsTheEarlierUnmountedRow() throws {
        let session = ExactApp.shared.makeSession(label: "tie-snap")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: CGSize(width: 300, height: 500)).error)
        let p = session.presenter
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "strip" })
        let scroll = try XCTUnwrap(node.scroll)
        scroll.contentOffset.x = 3000
        p.collections.flush()
        let snapshot = try XCTUnwrap(p.collections.entries[node.id]?.snapshot)
        let first = try XCTUnwrap(snapshot.rows.map(\.index).min())
        XCTAssertGreaterThan(first, 0)
        let earlier = CGFloat(first - 1) * 150
        XCTAssertEqual(rest(node, scroll, from: earlier + 75), earlier, accuracy: 0.01)
    }

    func testDescendantsAndLayoutDependentOffsetsAreOnlyKnownWhileMounted() throws {
        let session = ExactApp.shared.makeSession(label: "snap-limits")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: CGSize(width: 300, height: 500)).error)
        let p = session.presenter
        for name in ["percent", "transform", "descendant"] {
            let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == name })
            let scroll = try XCTUnwrap(node.scroll)
            let logical = session.runtime.collectionSnap(node.id, proposed: 30020, port: 300, maximum: 149700)
            if name == "descendant" {
                XCTAssertEqual(rest(node, scroll, from: 140), 150, accuracy: 0.01)
                XCTAssertNil(logical, "unmounted descendants require row layout")
            } else {
                if name == "transform" {
                    XCTAssertEqual(rest(node, scroll, from: 20), 30, accuracy: 0.01)
                } else {
                    XCTAssertGreaterThan(rest(node, scroll, from: 0), 0)
                }
                XCTAssertNil(logical, "layout-dependent displacement requires a mounted row")
            }
        }
    }

    func testVirtualizedCenterMatchesMountedAreasOnBothAxes() throws {
        let session = ExactApp.shared.makeSession(label: "center-snap")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: CGSize(width: 300, height: 1000)).error)
        for vertical in [false, true] {
            let name = vertical ? "center-y" : "center-x"
            let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
            let scroll = try XCTUnwrap(node.scroll)
            let (eager, eagerScroll) = try strip((0..<1000).map { ("center", Double($0) * 150, 150) }, vertical: vertical)
            eagerScroll.contentSize = vertical ? CGSize(width: 100, height: 150000) : CGSize(width: 150000, height: 100)
            for projection: CGFloat in [200, 1380, 30020] {
                let target = vertical ? CGPoint(x: 0, y: projection) : CGPoint(x: projection, y: 0)
                let actual = node.snappedOffset(target, in: scroll)
                let expected = eager.snappedOffset(target, in: eagerScroll)
                XCTAssertEqual(vertical ? actual.y : actual.x, vertical ? expected.y : expected.x, accuracy: 0.01)
            }
        }
    }

    func testVerticalCenterUsesTheUncoveredPort() throws {
        let (node, scroll) = try strip([("center", 300, 150)], vertical: true)
        scroll.contentInset = UIEdgeInsets(top: 20, left: 0, bottom: 40, right: 0)
        XCTAssertEqual(node.snappedOffset(CGPoint(x: 0, y: 240), in: scroll).y, 235, accuracy: 0.01)
    }

    func testCenterUsesThePaddingBoxInsideAsymmetricBorders() throws {
        for vertical in [false, true] {
            let (node, scroll) = try strip([("center", 10, 300), ("center", 310, 150), ("center", 460, 300)], vertical: vertical)
            node.style[vertical ? "border_width_top" : "border_width_left"] = 10
            node.style[vertical ? "border_width_bottom" : "border_width_right"] = 30
            if vertical { scroll.bounds.size.height = 340 } else { scroll.bounds.size.width = 340 }
            let result = node.snappedOffset(CGPoint(x: 190, y: 190), in: scroll)
            XCTAssertEqual(vertical ? result.y : result.x, 225, accuracy: 0.01)
        }
    }

    func testNestedOverflowOwnsItsDescendantsExceptForClip() throws {
        for vertical in [false, true] {
            for overflow in ["scroll", "auto", "hidden", "clip"] {
                let p = Presenter()
                let axis = vertical ? "y" : "x"
                p.apply(wireBatch([
                    ["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "scroll", "overflow_y": "scroll", "scroll_snap_type": "\(axis) mandatory"]],
                    ["op": "create", "id": 2, "kind": "view", "style": ["overflow_x": overflow, "overflow_y": overflow, "scroll_snap_type": "\(axis) mandatory", "scroll_snap_align": "start"]],
                    ["op": "create", "id": 3, "kind": "view", "style": ["scroll_snap_align": "center"]],
                    ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]],
                    ["op": "roots", "ids": [1]],
                    ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 300],
                    ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 300, "h": 300],
                    ["op": "frame", "id": 3, "x": vertical ? 0 : 300, "y": vertical ? 300 : 0, "w": 150, "h": 150],
                    ["op": "content", "id": 1, "w": 900, "h": 900],
                    ["op": "content", "id": 2, "w": 600, "h": 600]
                ]))
                let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
                let result = node.snappedOffset(CGPoint(x: 190, y: 190), in: scroll)
                XCTAssertEqual(vertical ? result.y : result.x, overflow == "clip" ? 225 : 0, "\(axis), \(overflow)")
            }
        }
    }

    private func strip(_ aligns: [(String, Double, Double)], vertical: Bool = false) throws -> (NodeView, UIScrollView) {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 100)
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": vertical ? "hidden" : "scroll", "overflow_y": vertical ? "scroll" : "hidden", "scroll_snap_type": vertical ? "y mandatory" : "x mandatory"]]]
        for (i, (align, x, w)) in aligns.enumerated() {
            ops.append(["op": "create", "id": 10 + i, "kind": "view", "style": ["scroll_snap_align": align]])
            ops.append(["op": "frame", "id": 10 + i, "x": vertical ? 0.0 : x, "y": vertical ? x : 0.0, "w": vertical ? 100.0 : w, "h": vertical ? w : 100.0])
        }
        ops += [["op": "children", "id": 1, "ids": aligns.indices.map { 10 + $0 }], ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": vertical ? 100.0 : 300.0, "h": vertical ? 300.0 : 100.0],
                ["op": "content", "id": 1, "w": vertical ? 100.0 : 900.0, "h": vertical ? 900.0 : 100.0]]
        p.apply(wireBatch(ops))
        let node = try XCTUnwrap(p.views[1])
        return (node, try XCTUnwrap(node.scroll))
    }
    private func rest(_ node: NodeView, _ scroll: UIScrollView, from x: CGFloat) -> CGFloat {
        var target = CGPoint(x: x, y: 0)
        node.scrollViewWillEndDragging(scroll, withVelocity: .zero, targetContentOffset: &target)
        return target.x
    }

    func testBothAxesCenterAndOversizedAreasKeepTheProjectedRestRule() throws {
        for vertical in [false, true] {
            let (node, scroll) = try strip([("center", 0, 150), ("center", 300, 150), ("center", 450, 450)], vertical: vertical)
            XCTAssertEqual(scroll.decelerationRate, .fast)
            for (projection, expected): (CGFloat, CGFloat) in [(40, 0), (190, 225), (500, 500), (700, 600)] {
                var target = vertical ? CGPoint(x: 17, y: projection) : CGPoint(x: projection, y: 17)
                node.scrollViewWillEndDragging(scroll, withVelocity: .zero, targetContentOffset: &target)
                XCTAssertEqual(vertical ? target.y : target.x, expected, accuracy: 0.01)
                XCTAssertEqual(vertical ? target.x : target.y, 17)
            }
            Agent.scroll(from: scroll, dx: vertical ? 0 : 190, dy: vertical ? 190 : 0)
            XCTAssertEqual(vertical ? scroll.contentOffset.y : scroll.contentOffset.x, 225, accuracy: 0.01)
        }
    }

    func testEndRestsAnAreasEndAtTheViewportsEnd() throws {
        // A at 0 (start); B 300–450 (end: rests at 150); C 450–600 (start: 450).
        let (node, scroll) = try strip([("start", 0, 300), ("end", 300, 150), ("start", 450, 150)])
        XCTAssertEqual(rest(node, scroll, from: 60), 0)
        XCTAssertEqual(rest(node, scroll, from: 140), 150, "B's end at the viewport's end")
        XCTAssertEqual(rest(node, scroll, from: 400), 450)
    }

    func testAWideEndAreaIsFreeWhileItCoversTheViewport() throws {
        // 0–600, wider than the 300-point viewport: anywhere in 0…300.
        let (node, scroll) = try strip([("end", 0, 600), ("start", 600, 300)])
        XCTAssertEqual(rest(node, scroll, from: 120), 120)
        XCTAssertEqual(rest(node, scroll, from: 520), 600)
    }

    func testAgentRevealAndWheelRestAtAPagesSnapPosition() throws {
        let session = ExactApp.shared.makeSession(label: "snap-reveal")
        defer { session.destroy() }
        let p = session.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "scroll", "overflow_y": "hidden", "scroll_snap_type": "x mandatory"]],
            ["op": "create", "id": 10, "kind": "view", "style": ["scroll_snap_align": "start"]],
            ["op": "create", "id": 11, "kind": "view", "style": ["scroll_snap_align": "start"]],
            ["op": "create", "id": 20, "kind": "button", "handlers": ["press"]],
            ["op": "children", "id": 1, "ids": [10, 11]], ["op": "children", "id": 11, "ids": [20]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 100, "w": 402, "h": 100],
            ["op": "frame", "id": 10, "x": 0, "y": 0, "w": 402, "h": 100],
            ["op": "frame", "id": 11, "x": 402, "y": 0, "w": 402, "h": 100],
            ["op": "frame", "id": 20, "x": 12, "y": 20, "w": 30, "h": 44],
            ["op": "content", "id": 1, "w": 804, "h": 100]
        ]))
        let scroll = try XCTUnwrap(p.views[1]?.scroll), agent = Agent(session: session)
        XCTAssertNil(agent.reveal(["id": 20])["error"])
        XCTAssertEqual(scroll.contentOffset.x, 402, accuracy: 0.01)
        scroll.contentOffset.x = 388
        XCTAssertNil(agent.reveal(["id": 20])["error"], "already visible, but between pages")
        XCTAssertEqual(scroll.contentOffset.x, 402, accuracy: 0.01)
        scroll.contentOffset.x = 0
        Agent.scroll(from: scroll, dx: 388, dy: 0)
        XCTAssertEqual(scroll.contentOffset.x, 402, accuracy: 0.01)
    }
}
#endif
