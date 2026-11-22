#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections

final class CollectionPageBandIOSTests: XCTestCase {
    func testAnAuthoredOffsetWinsAfterTheListInsetChangesInABatch() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 200)
        defer { p.onViewportFit = nil; p.reset() }
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 20,
                "totalExtent": 1000, "rows": [["view": 2, "root": 2, "epoch": 1]], "correction": NSNull()]]],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 300, "h": 50],
            ["op": "content", "id": 1, "w": 300, "h": 1000]
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        scroll.contentInsetAdjustmentBehavior = .always
        XCTAssertTrue(p.listHost.owns(1))
        XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), 0, accuracy: 0.5)
        var nested = 0
        p.onViewportFit = {
            XCTAssertTrue(p.applying)
            scroll.contentInset.top = 40
            p.afterBatch {
                nested += 1
                XCTAssertFalse(p.applying)
                XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), 100, accuracy: 0.5,
                               "closing work sees the authored offset after collection finalization")
                p.apply(wireBatch([["op": "props", "id": 2, "set": ["testId": "nested-row"]]]))
            }
        }
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["viewportFit": "cover", "scrollTop": "100"]]]))
        XCTAssertEqual(nested, 1)
        XCTAssertEqual(scroll.adjustedContentInset.top, 40)
        XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), 100, accuracy: 0.5)
    }

    func testThePhysicalPortAndEveryOffsetConversionIncludeThePageBands() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 200)
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 20, "totalExtent": 1000,
                "rows": [["view": 2, "root": 2, "epoch": 1]], "correction": NSNull()]]],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll", "padding_top": 20]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 20, "w": 300, "h": 50],
            contentOp(1, 300, 200, padding: [20, 0, 0, 0]),
            ["op": "content", "id": 1, "w": 300, "h": 1020]
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        node.setPageBand(UIEdgeInsets(top: 60, left: 0, bottom: 30, right: 0))
        // What the list draws through, with no reach, is the page band (LLP 1123 D13's `scrollDrawnThrough`)
        XCTAssertEqual(node.scrollDrawnThrough(scroll), UIEdgeInsets(top: 60, left: 0, bottom: 30, right: 0))
        let rest = scroll.contentOffset.y
        let facts = try XCTUnwrap(p.listHost.geometry(1))
        XCTAssertEqual(facts.portMain, 290)
        XCTAssertEqual(facts.paddingStart, 80)
        XCTAssertEqual(facts.paddingEnd, 30)
        XCTAssertEqual(facts.offset, -80)
        XCTAssertEqual(p.listHost.covered(1), [60, 30], "what the runner aligns rows clear of: the bands, not the list's padding")
        p.listHost.keepStart(1, at: -80)
        XCTAssertEqual(scroll.contentOffset.y, rest)
        p.listHost.shift(1, by: 40, extent: 1000, from: -80)
        XCTAssertEqual(scroll.contentOffset.y, rest + 40)
        XCTAssertEqual(try XCTUnwrap(p.listHost.geometry(1)).offset, -40)
        p.listHost.correct(1, top: 200, extent: 1000)
        XCTAssertEqual(scroll.contentOffset.y, 220)
        XCTAssertEqual(try XCTUnwrap(p.listHost.geometry(1)).offset, 200)
        p.listHost.keepStart(1, at: 200)
        XCTAssertEqual(scroll.contentOffset.y, 220)
        p.listHost.correct(1, top: -80, extent: 1000)
        XCTAssertEqual(scroll.contentOffset.y, rest)
        p.listHost.beginBatch(wireBatch([]))
        node.setPageBand(UIEdgeInsets(top: 80, left: 0, bottom: 30, right: 0))
        p.listHost.endBatch()
        XCTAssertEqual(scroll.contentOffset.y, rest - 20, "a growing band keeps the content's resting place")
        XCTAssertEqual(try XCTUnwrap(p.listHost.geometry(1)).offset, -100)
        // A band that grows while another inset goes (a route pushed under
        // control bars: the bar's inset leaves as the band takes its place)
        // keeps the list at its top, the new band's.
        p.listHost.beginBatch(wireBatch([]))
        node.setPageBand(UIEdgeInsets(top: 120, left: 0, bottom: 30, right: 0))
        scroll.contentInset.top -= 10
        p.listHost.endBatch()
        XCTAssertEqual(scroll.contentOffset.y, -scroll.adjustedContentInset.top, "at the top, not 40 points into the list")
        XCTAssertEqual(try XCTUnwrap(p.listHost.geometry(1)).offset, -140)
    }

    func testARowListWithNoReachCountsThePageBandAsPortAcross() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 200)
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "axis": "x", "revision": 1, "scrollSequence": 0, "count": 20, "totalExtent": 2000,
                "rows": [["view": 2, "root": 2, "epoch": 1]], "correction": NSNull()]]],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_x": "scroll", "overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 200],
            ["op": "content", "id": 1, "w": 2000, "h": 200]
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        XCTAssertEqual(p.listHost.entries[1]?.snapshot.horizontal, true)
        node.setPageBand(UIEdgeInsets(top: 60, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(scroll.adjustedContentInset.top, 60, "the band's inset")
        // With no reach the band is port across and no inset of the batch's, as for a vertical list
        XCTAssertEqual(try XCTUnwrap(p.listHost.geometry(1)).portCross, 260)
        XCTAssertEqual(p.listHost.portInsets(1)?.first, 0)
    }

    func testARowListDrawsThroughItsSideReachAndAddsNoRangeForIt() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        defer { p.reset() }
        let (node, scroll) = try reachedList(in: p, axis: "x", reach: UIEdgeInsets(top: 0, left: 30, bottom: 0, right: 20))
        let facts = try XCTUnwrap(p.listHost.geometry(1))
        XCTAssertEqual(facts.portMain, 350, "the reached frame")
        XCTAssertEqual(facts.portCross, 200)
        XCTAssertEqual([facts.paddingStart, facts.paddingEnd], [30, 20], "the reach it draws through")
        XCTAssertEqual(facts.offset, -30, "at rest before the first item")
        XCTAssertEqual(p.listHost.covered(1), [30, 20])
        XCTAssertEqual(p.listHost.portInsets(1), [0, 0])
        // The content is no narrower than the box, so the reach adds no range
        p.listHost.shift(1, by: 0, extent: 100, from: nil)
        XCTAssertEqual(scroll.contentSize.width, 300)
        p.listHost.correct(1, top: 200, extent: 2000)
        XCTAssertEqual(scroll.contentOffset.x, 200)
        XCTAssertEqual(try XCTUnwrap(p.listHost.geometry(1)).offset, 200)
        p.listHost.keepStart(1, at: 100)
        XCTAssertEqual(scroll.contentOffset.x, 100)
        p.listHost.shift(1, by: 40, extent: 2000, from: 100)
        XCTAssertEqual(scroll.contentOffset.x, 140)
        XCTAssertEqual(node.scrollDrawnThrough(scroll), UIEdgeInsets(top: 0, left: 30, bottom: 0, right: 20))
    }

    func testAVerticalListsSideReachIsNeitherPortNorPadding() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        defer { p.reset() }
        _ = try reachedList(in: p, axis: "y", reach: UIEdgeInsets(top: 0, left: 30, bottom: 0, right: 0))
        let facts = try XCTUnwrap(p.listHost.geometry(1))
        XCTAssertEqual(facts.portCross, 300, "the reached frame less the whole side inset: the box")
        XCTAssertEqual(facts.cross, 300, "rows as wide as the box")
        XCTAssertEqual([facts.portMain, facts.paddingStart, facts.paddingEnd], [200, 0, 0])
        XCTAssertEqual(p.listHost.covered(1), [0, 0])
        XCTAssertEqual(p.listHost.portInsets(1), [0, 30])
    }

    func testABandUnderRidingBarsMovesTheBatchsStartByTheBandAlone() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        defer { p.reset() }
        let (node, scroll) = try reachedList(in: p, axis: "y", reach: .zero)
        p.apply(wireBatch([["op": "content", "id": 1, "w": 300, "h": 2000]]))
        node.setPageBand(UIEdgeInsets(top: 60, left: 0, bottom: 0, right: 0))
        let top = { scroll.contentOffset.y + node.scrollTopInset(scroll) }
        scroll.contentOffset.y = 100 - node.scrollTopInset(scroll)
        XCTAssertEqual(top(), 100)
        // A band 20 points deeper mid-batch whose inset grows 40, riding bars' depth with it (`NavigationHost`'s
        // `riders`): the list's offset moves by the band, the native one by the inset
        p.listHost.beginBatch(wireBatch([]))
        node.setPageBand(UIEdgeInsets(top: 80, left: 0, bottom: 0, right: 0), inset: UIEdgeInsets(top: 100, left: 0, bottom: 0, right: 0))
        p.listHost.endBatch()
        XCTAssertEqual(top(), 100, "the batch kept `scrollTop`")
        XCTAssertEqual(try XCTUnwrap(p.listHost.geometry(1)).offset, 20)
    }

    func testABandMidBatchLeavesARowListsStart() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        defer { p.reset() }
        let (node, scroll) = try reachedList(in: p, axis: "x", reach: .zero)
        p.apply(wireBatch([["op": "content", "id": 1, "w": 2000, "h": 200]]))
        scroll.contentOffset.x = 100
        // A plain band: the batch keeps no start, but a row correction counts from it
        p.listHost.beginBatch(wireBatch([]))
        node.setPageBand(UIEdgeInsets(top: 60, left: 0, bottom: 0, right: 0))
        p.listHost.shift(1, by: 10, extent: 2000, from: p.listHost.entries[1]?.batchStart)
        p.listHost.endBatch()
        XCTAssertEqual(scroll.contentOffset.x, 110, "a band moves nothing along a row list")
        // A band under riding bars: the batch keeps the start
        scroll.contentOffset.x = 100
        p.listHost.beginBatch(wireBatch([]))
        node.setPageBand(UIEdgeInsets(top: 60, left: 0, bottom: 0, right: 0), inset: UIEdgeInsets(top: 80, left: 0, bottom: 0, right: 0))
        p.listHost.endBatch()
        XCTAssertEqual(scroll.contentOffset.x, 100)
    }

    func testAListAtItsTopOverABorderStaysThereAsABatchDeepensItsBand() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        defer { p.reset() }
        let (node, scroll) = try reachedList(in: p, axis: "y", reach: .zero)
        // Its port above the padding's start, over a 20-point top border, where `geometry`'s offset is clamped
        p.apply(wireBatch([["op": "content", "id": 1, "w": 300, "h": 2000], contentOp(1, 300, 200, padding: [0, 0, 0, 0], border: [20, 0, 0, 0])]))
        node.setPageBand(UIEdgeInsets(top: 60, left: 0, bottom: 0, right: 0))
        let top = { scroll.contentOffset.y + node.scrollTopInset(scroll) }
        XCTAssertEqual(top(), 0)
        p.listHost.beginBatch(wireBatch([]))
        node.setPageBand(UIEdgeInsets(top: 80, left: 0, bottom: 0, right: 0), inset: UIEdgeInsets(top: 100, left: 0, bottom: 0, right: 0))
        p.listHost.endBatch()
        XCTAssertEqual(top(), 0)
    }

    func testTheBatchsStartFollowsTheListsOffsetAcrossABandAndAReach() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        defer { p.reset() }
        let (node, scroll) = try reachedList(in: p, axis: "y", reach: .zero)
        // A list that fits, under a 20-point top border
        p.apply(wireBatch([contentOp(1, 300, 200, padding: [0, 0, 0, 0], border: [20, 0, 0, 0])]))
        let start = { try XCTUnwrap(p.listHost.entries[1]?.batchStart) }, offset = { try XCTUnwrap(p.listHost.geometry(1)).offset }
        p.listHost.beginBatch(wireBatch([]))
        // A band under riding bars takes the list to its new end, which `geometry` holds at the padding's start
        node.setPageBand(UIEdgeInsets(top: 60, left: 0, bottom: 0, right: 0), inset: UIEdgeInsets(top: 100, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(try start(), try offset(), accuracy: 0.5, "the band")
        // A reach deeper than the band, inside the inset, moves what the list draws through and not the native offset
        let native = scroll.contentOffset
        node.setReach(UIEdgeInsets(top: 80, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(scroll.contentOffset, native)
        XCTAssertEqual(try start(), try offset(), accuracy: 0.5, "the reach")
        p.listHost.endBatch()
    }

    func testAnEndFollowingListStaysAtItsEndAsTheSwitchHelperDeepensItsInsetInABatch() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        defer { p.reset() }
        let (node, scroll) = try reachedList(in: p, axis: "y", reach: .zero)
        p.apply(wireBatch([["op": "content", "id": 1, "w": 300, "h": 2000], ["op": "props", "id": 1, "set": ["scrollFollowEnd": "true"]]]))
        let list = try XCTUnwrap(scroll as? ScrollView), end = { list.end(under: list.adjustedContentInset) }
        list.contentOffset.y = end()
        list.kept(under: list.adjustedContentInset)
        XCTAssertTrue(list.restsAtEnd)
        // An 82-point top inset set through the switch helper, standing in for joining UIKit's adjustment under a bar:
        // the helper keeps the end, and the batch's start moves with it, so the batch keeps it too
        p.listHost.beginBatch(wireBatch([]))
        node.keepingScrollTop(of: list) { list.contentInset.top += 82 }
        p.listHost.endBatch()
        XCTAssertEqual(list.contentOffset.y, end(), accuracy: 0.5)
    }

    /// A list in a 300 by 200 box, `axis` "x" for a row list, given `reach` and the insets and offset D14's writer
    /// gives it for that reach, its content back where layout put it.
    private func reachedList(in p: Presenter, axis: String, reach: UIEdgeInsets) throws -> (NodeView, UIScrollView) {
        let row = axis == "x" ? (w: 100.0, h: 200.0) : (w: 300.0, h: 50.0)
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "axis": axis, "revision": 1, "scrollSequence": 0, "count": 20,
                "totalExtent": 2000, "rows": [["view": 2, "root": 2, "epoch": 1]], "correction": NSNull()]]],
            ["op": "create", "id": 1, "kind": "list", "style": axis == "x" ? ["overflow_x": "scroll"] : ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 50, "y": 50, "w": 300, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": row.w, "h": row.h],
            ["op": "content", "id": 1, "w": row.w, "h": row.h]
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        XCTAssertEqual(p.listHost.entries[1]?.snapshot.horizontal, axis == "x")
        node.setReach(reach)
        XCTAssertEqual(scroll.frame.width, 300 + reach.left + reach.right)
        scroll.contentInset = reach
        scroll.contentOffset = CGPoint(x: -reach.left, y: -reach.top)
        return (node, scroll)
    }
}
#endif
