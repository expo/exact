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

    private func strip(_ aligns: [(String, Double, Double)], vertical: Bool = false, type: String? = nil, stop: String = "normal") throws -> (NodeView, UIScrollView) {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 100)
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": vertical ? "hidden" : "scroll", "overflow_y": vertical ? "scroll" : "hidden", "scroll_snap_type": type ?? (vertical ? "y mandatory" : "x mandatory")]]]
        for (i, (align, x, w)) in aligns.enumerated() {
            ops.append(["op": "create", "id": 10 + i, "kind": "view", "style": ["scroll_snap_align": align, "scroll_snap_stop": stop]])
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

    /// CSS Scroll Snap 1 §6.2: a release does not pass over an `always`
    /// area's position, either way; `normal` does.
    func testAlwaysStopsARestAtTheNextAreaInItsWay() throws {
        let cards = (0..<6).map { ("start", Double($0) * 150, 150.0) }
        let (node, scroll) = try strip(cards, stop: "always")
        XCTAssertEqual(node.snappedOffset(CGPoint(x: 520, y: 0), in: scroll, release: CGPoint(x: 20, y: 0)).x, 150)
        XCTAssertEqual(node.snappedOffset(CGPoint(x: 10, y: 0), in: scroll, release: CGPoint(x: 440, y: 0)).x, 300)
        XCTAssertEqual(node.snappedOffset(CGPoint(x: 170, y: 0), in: scroll, release: CGPoint(x: 160, y: 0)).x, 150, "a short release rests where it would")
        let (free, freeScroll) = try strip(cards)
        XCTAssertEqual(free.snappedOffset(CGPoint(x: 520, y: 0), in: freeScroll, release: CGPoint(x: 20, y: 0)).x, 450)
        // UIKit's drag end releases at the scroller's offset.
        scroll.contentOffset.x = 20
        var target = CGPoint(x: 520, y: 0)
        node.scrollViewWillEndDragging(scroll, withVelocity: CGPoint(x: 2, y: 0), targetContentOffset: &target)
        XCTAssertEqual(target.x, 150)
    }

    /// CSS Scroll Snap 1 §5.5: a batch that moves the areas keeps a scroller
    /// at rest on the area it rested on.
    func testALayoutChangeKeepsTheAreaItRestedOn() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 100)
        func cards(_ width: Double) -> [[String: Any]] {
            (0..<6).map { ["op": "frame", "id": 10 + $0, "x": Double($0) * width, "y": 0.0, "w": width, "h": 100.0] }
                + [["op": "content", "id": 1, "w": 6 * width, "h": 100.0]]
        }
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "scroll", "overflow_y": "hidden", "scroll_snap_type": "x mandatory"]]]
            + (0..<6).map { ["op": "create", "id": 10 + $0, "kind": "view", "style": ["scroll_snap_align": "start"]] }
            + [["op": "children", "id": 1, "ids": (0..<6).map { 10 + $0 }], ["op": "roots", "ids": [1]],
               ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 100.0]] + cards(150)))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        var target = CGPoint(x: 280, y: 0)
        node.scrollViewWillEndDragging(scroll, withVelocity: .zero, targetContentOffset: &target)
        scroll.contentOffset = target
        XCTAssertEqual(scroll.contentOffset.x, 300, accuracy: 0.01)
        node.resnap()
        XCTAssertEqual(scroll.contentOffset.x, 300, accuracy: 0.01, "nothing moved")
        p.apply(wireBatch(cards(200)))
        XCTAssertEqual(scroll.contentOffset.x, 400, accuracy: 0.01, "the third card, where it now is")
    }

    /// LLP 1008 §9 as amended for #219: a flick reaches at least the next
    /// position its way from where its drag began, as a UIKit pager's does;
    /// a long fling still rests where UIKit projects.
    func testAFlickReachesTheNextPositionFromWhereItsDragBegan() throws {
        let (node, scroll) = try strip((0..<6).map { ("start", Double($0) * 150, 150.0) })
        func rest(_ projected: CGFloat, release: CGFloat, start: CGFloat, velocity: CGFloat) -> CGFloat {
            node.snappedOffset(CGPoint(x: projected, y: 0), in: scroll, release: CGPoint(x: release, y: 0),
                               drag: (CGPoint(x: start, y: 0), CGPoint(x: velocity, y: 0))).x
        }
        XCTAssertEqual(rest(60, release: 40, start: 0, velocity: 0.3), 150, "a short quick flick goes on")
        XCTAssertEqual(rest(60, release: 40, start: 0, velocity: 0.01), 0, "a release too slow to flick goes back")
        XCTAssertEqual(rest(190, release: 170, start: 0, velocity: 0.3), 150, "a drag that already passed a card adds none")
        XCTAssertEqual(rest(290, release: 280, start: 300, velocity: -0.3), 150, "the other way")
        XCTAssertEqual(rest(560, release: 40, start: 0, velocity: 3), 600, "a long fling rests where UIKit projects")
    }

    /// Viewport-long `always` pages page as UIKit's `isPagingEnabled` does;
    /// `normal` pages, shorter areas and a padded snapport keep the rule.
    func testViewportLongAlwaysPagesPageAsUIKitDoes() throws {
        let pages = (0..<3).map { ("start", Double($0) * 300, 300.0) }
        let (paged, pagedScroll) = try strip(pages, stop: "always")
        XCTAssertTrue(paged.pagesByViewport(pagedScroll))
        paged.pageIfPager(pagedScroll)
        XCTAssertTrue(pagedScroll.isPagingEnabled)
        for case let area as NodeView in pagedScroll.subviews { area.style["scroll_snap_stop"] = .string("normal") }
        paged.pageIfPager(pagedScroll)
        XCTAssertFalse(pagedScroll.isPagingEnabled, "off again once its pages may be passed over")
        let (free, freeScroll) = try strip(pages)
        XCTAssertFalse(free.pagesByViewport(freeScroll), "`normal` may pass over pages")
        let (cards, cardScroll) = try strip((0..<6).map { ("start", Double($0) * 150, 150.0) }, stop: "always")
        XCTAssertFalse(cards.pagesByViewport(cardScroll), "areas shorter than the viewport snap by the rule")
        let (sparse, sparseScroll) = try strip([("start", 0, 300), ("start", 600, 300)], stop: "always")
        XCTAssertFalse(sparse.pagesByViewport(sparseScroll), "UIKit would rest at 300, where no page is")
        let (padded, paddedScroll) = try strip(pages, stop: "always")
        padded.style["scroll_padding_left"] = 20
        XCTAssertFalse(padded.pagesByViewport(paddedScroll), "a padded snapport is not the scrollport UIKit pages by")
        let (margined, marginedScroll) = try strip(pages, stop: "always")
        for case let area as NodeView in marginedScroll.subviews { area.style["scroll_margin_left"] = -10 }
        XCTAssertEqual(margined.snappedOffset(CGPoint(x: 290, y: 0), in: marginedScroll).x, 310, "its snap positions are 10 past the pages")
        XCTAssertFalse(margined.pagesByViewport(marginedScroll), "UIKit would rest at 300, where no snap position is")
        let (centred, centredScroll) = try strip(pages.map { ("center", $0.1, $0.2) }, stop: "always")
        XCTAssertTrue(centred.pagesByViewport(centredScroll), "centred pages snap at the multiples UIKit pages to")
    }

    /// A pager's rest is recorded for a re-snap after layout (CSS Scroll Snap 1
    /// §5.5) whether or not anything listens for `scrolldragend`.
    func testAPagerKeepsThePageItRestedOnAcrossALayoutChange() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 100)
        func pages(_ lead: Double) -> [[String: Any]] {
            (0..<3).map { ["op": "frame", "id": 10 + $0, "x": lead + Double($0) * 300, "y": 0.0, "w": 300.0, "h": 100.0] }
                + [["op": "content", "id": 1, "w": lead + 900, "h": 100.0]]
        }
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "scroll", "overflow_y": "hidden", "scroll_snap_type": "x mandatory"]]]
            + (0..<3).map { ["op": "create", "id": 10 + $0, "kind": "view", "style": ["scroll_snap_align": "start", "scroll_snap_stop": "always"]] }
            + [["op": "children", "id": 1, "ids": (0..<3).map { 10 + $0 }], ["op": "roots", "ids": [1]],
               ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 100.0]] + pages(0)))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        node.pageIfPager(scroll)
        XCTAssertTrue(scroll.isPagingEnabled)
        scroll.contentOffset = CGPoint(x: 300, y: 0)
        node.pagedDragEnded(scroll)
        p.apply(wireBatch(pages(50)))
        XCTAssertEqual(scroll.contentOffset.x, 350, accuracy: 0.01, "the second page, where it now is")
    }

    /// `proximity` snaps only within a third of the snapport of the projection.
    func testProximityLeavesARestFarFromEveryArea() throws {
        let (node, scroll) = try strip([("start", 0, 100), ("start", 400, 100)], type: "x")
        XCTAssertEqual(scroll.decelerationRate, .normal)
        XCTAssertEqual(rest(node, scroll, from: 330), 400)
        XCTAssertEqual(rest(node, scroll, from: 200), 200)
        XCTAssertNil(node.snapTarget(CGPoint(x: 200, y: 0), in: scroll))
    }

    /// `both`, two alignments (block then inline), `scroll-padding` and
    /// `scroll-margin`.
    func testBothAxesReadEachAlignmentAndTheSnapportsPadding() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 300)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "scroll", "overflow_y": "scroll", "scroll_snap_type": "both mandatory",
                                                                "scroll_padding_left": 20, "scroll_padding_top": 10]],
            ["op": "create", "id": 2, "kind": "view", "style": ["scroll_snap_align": "end center", "scroll_margin_top": 5]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 300],
            ["op": "frame", "id": 2, "x": 400, "y": 400, "w": 100, "h": 100],
            ["op": "content", "id": 1, "w": 900, "h": 900]
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        let rest = node.snappedOffset(CGPoint(x: 300, y: 300), in: scroll)
        // x: centred in the 280 snapport that starts 20 in: 450 - 140 - 20.
        XCTAssertEqual(rest.x, 290, accuracy: 0.01)
        // y: its end (500) at the 290 snapport's end, which starts 10 in.
        XCTAssertEqual(rest.y, 200, accuracy: 0.01)
    }
}
#endif
