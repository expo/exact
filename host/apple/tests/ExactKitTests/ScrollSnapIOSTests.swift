#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1008's horizontal snap: `scroll-snap-align: start` rests an area's
/// start at the viewport's, `end` its end, and an area wider than the
/// viewport is free while it covers it (CSS Scroll Snap §5.2.2).
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

    private func strip(_ aligns: [(String, Double, Double)]) throws -> (NodeView, UIScrollView) {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 100)
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "scroll", "scroll_snap_type": "x mandatory"]]]
        for (i, (align, x, w)) in aligns.enumerated() {
            ops.append(["op": "create", "id": 10 + i, "kind": "view", "style": ["scroll_snap_align": align]])
            ops.append(["op": "frame", "id": 10 + i, "x": x, "y": 0.0, "w": w, "h": 100.0])
        }
        ops += [["op": "children", "id": 1, "ids": aligns.indices.map { 10 + $0 }], ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 100.0],
                ["op": "content", "id": 1, "w": 900.0, "h": 100.0]]
        p.apply(wireBatch(ops))
        let node = try XCTUnwrap(p.views[1])
        return (node, try XCTUnwrap(node.scroll))
    }
    private func rest(_ node: NodeView, _ scroll: UIScrollView, from x: CGFloat) -> CGFloat {
        var target = CGPoint(x: x, y: 0)
        node.scrollViewWillEndDragging(scroll, withVelocity: .zero, targetContentOffset: &target)
        return target.x
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
