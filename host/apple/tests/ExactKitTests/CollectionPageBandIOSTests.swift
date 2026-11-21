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
}
#endif
