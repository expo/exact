#if os(iOS)
import ExactCore
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections

/// A batch that only builds, moves or drops a virtualized list's rows (a
/// fling's fills, many a second) skips the route projection; anything the
/// projection reads, or anything outside such rows, runs it.
final class ListRowsIOSTests: XCTestCase {
    private func snapshot(_ view: Int, revision: Int) -> [String: Any] {
        ["view": view, "revision": revision, "scrollSequence": 0, "count": 50, "totalExtent": 2000,
         "rows": [["view": view * 10, "root": view * 10, "epoch": 1]]]
    }
    /// 30 holds a header 50, which holds a virtualized list 4 (row 40 > 42),
    /// a virtualized list 1 (row 10 > 11), a tab 70 whose face is 71, and a
    /// virtualized list 5 whose row 80 holds a tablist 81.
    private func presenter() -> Presenter {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "collections", "items": [snapshot(1, revision: 1), snapshot(4, revision: 1), snapshot(5, revision: 1)]],
            ["op": "create", "id": 30, "kind": "view"],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 10, "kind": "view"], ["op": "create", "id": 11, "kind": "view"],
            ["op": "create", "id": 50, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 4, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 40, "kind": "view"], ["op": "create", "id": 42, "kind": "view"],
            ["op": "children", "id": 10, "ids": [11]], ["op": "children", "id": 1, "ids": [10]],
            ["op": "children", "id": 40, "ids": [42]], ["op": "children", "id": 4, "ids": [40]],
            ["op": "children", "id": 50, "ids": [4]],
            ["op": "create", "id": 70, "kind": "view", "props": ["accessibilityRole": "tab"]], ["op": "create", "id": 71, "kind": "view"],
            ["op": "children", "id": 70, "ids": [71]],
            ["op": "create", "id": 5, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 80, "kind": "view"], ["op": "create", "id": 81, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "children", "id": 80, "ids": [81]], ["op": "children", "id": 5, "ids": [80]],
            ["op": "children", "id": 30, "ids": [50, 1, 70, 5]], ["op": "roots", "ids": [30]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0],
        ]))
        return p
    }

    func testRowsBuiltMovedAndDroppedInAVirtualizedListAreOnlyListRows() {
        let p = presenter()
        XCTAssertTrue(p.listHost.owns(1))
        XCTAssertTrue(p.onlyListRows(wireBatch([
            ["op": "create", "id": 20, "kind": "view"], ["op": "create", "id": 21, "kind": "view"],
            ["op": "children", "id": 20, "ids": [21]], ["op": "children", "id": 1, "ids": [10, 20]],
            ["op": "frame", "id": 20, "x": 0.0, "y": 40.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 300.0, "h": 40.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 2000.0],
            ["op": "style", "id": 11, "style": ["opacity": 0.5]],
            ["op": "present", "id": 11, "property": "opacity", "x": 0.5],
            ["op": "destroy", "id": 11],
        ])))
        // A fade outside any list that no tab, header or Back reads, and an
        // op on a node the presenter never made, change nothing it projects.
        XCTAssertTrue(p.onlyListRows(wireBatch([
            ["op": "present", "id": 30, "property": "opacity", "x": 0.0],
            ["op": "frame", "id": 999, "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0],
            ["op": "collections", "items": [snapshot(1, revision: 2)]],
        ])))
    }

    func testWhatTheProjectionReadsOrWhatIsOutsideTheRowsIsNot() {
        let p = presenter()
        for ops: [[String: Any]] in [
            [["op": "props", "id": 1, "set": ["testId": "t"]]],                    // the list's own props
            [["op": "style", "id": 30, "style": ["opacity": 0.5]]],                // outside any list
            [["op": "children", "id": 30, "ids": [1, 50]]],                        // reorders a header
            [["op": "frame", "id": 30, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0]],
            [["op": "present", "id": 71, "property": "opacity", "x": 0.0]],        // a tab's badge fading
            [["op": "style", "id": 42, "style": ["opacity": 0.5]]],                // a row under a header
            [["op": "children", "id": 4, "ids": [40]]],                             // a header's list
            [["op": "props", "id": 11, "set": ["accessibilityRole": "tab"]]],      // a row becoming a tab
            [["op": "style", "id": 80, "style": ["accent_color": [[0, 0, 255, 255]]]]], // a row holding a tablist
            [["op": "create", "id": 60, "kind": "view"]],                           // placed nowhere
            [["op": "create", "id": 61, "kind": "view", "props": ["accessibilityRole": "tablist"]],
             ["op": "children", "id": 10, "ids": [11, 61]]],                          // a tablist placed in a row
            [["op": "roots", "ids": [30]]],
        ] {
            XCTAssertFalse(p.onlyListRows(wireBatch(ops)), "\(ops)")
        }
        XCTAssertTrue(p.onlyListRows(wireBatch([])), "an empty batch changes no projection input")
        // A batch that says the root tabs, with no op: a placement change the views do not carry
        var said = wireBatch([])
        said.rootTabs = [RootTabs(root: 30, tablist: 61, tabs: [11], panels: [40], unsaid: false)]
        XCTAssertFalse(p.onlyListRows(said), "the projection reads the root tabs")
    }

    /// Whether a header button opens a popover is the header rule's to read
    /// (`Invocation`), wherever the popover is: rows that create, rename or
    /// drop what a header button names, a view or a paragraph's run, are no
    /// rows-only batch.
    func testWhatAHeaderButtonNamesIsReadInsideTheRows() throws {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "collections", "items": [snapshot(4, revision: 1)]],
            ["op": "create", "id": 1, "kind": "view", "props": ["navigationKey": "home"]],
            ["op": "create", "id": 5, "kind": "view", "props": ["navigationKey": "home"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 3, "kind": "button", "props": ["popovertarget": "m"]],
            ["op": "create", "id": 4, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 40, "kind": "view"], ["op": "create", "id": 43, "kind": "view", "props": ["id": "m", "popover": "auto"]],
            ["op": "create", "id": 44, "kind": "text"],
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 4, "ids": [40, 43, 44]],
            ["op": "children", "id": 5, "ids": [2, 4]], ["op": "children", "id": 1, "ids": [5]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0],
        ]))
        XCTAssertTrue(p.onlyListRows(wireBatch([
            ["op": "create", "id": 41, "kind": "view"], ["op": "children", "id": 4, "ids": [40, 41, 43, 44]],
            ["op": "paragraph", "id": 44, "runs": [["id": 46, "parent": 44, "paint": true, "props": ["text": "plain"], "style": [:]]]],
        ])), "a plain row and a plain run")
        for ops: [[String: Any]] in [
            [["op": "create", "id": 42, "kind": "view", "props": ["id": "m", "popover": "auto"]], ["op": "children", "id": 4, "ids": [40, 42, 43]]],
            [["op": "props", "id": 40, "set": ["id": "m"]]],
            [["op": "props", "id": 43, "set": ["id": "other"]]],
            [["op": "destroy", "id": 43], ["op": "children", "id": 4, "ids": [40, 44]]],
            [["op": "paragraph", "id": 44, "runs": [["id": 45, "parent": 44, "paint": true, "props": ["id": "m", "text": "m"], "style": [:]]]]],
        ] {
            XCTAssertFalse(p.onlyListRows(wireBatch(ops)), "\(ops)")
        }
        // A paragraph holding a run the header names, destroyed with that run.
        p.apply(wireBatch([["op": "paragraph", "id": 44, "runs": [["id": 45, "parent": 44, "paint": true, "props": ["id": "m", "text": "m"], "style": [:]]]]]))
        XCTAssertFalse(p.onlyListRows(wireBatch([["op": "destroy", "id": 45], ["op": "destroy", "id": 44], ["op": "children", "id": 4, "ids": [40, 43]]])))
        XCTAssertFalse(p.onlyListRows(wireBatch([["op": "destroy", "id": 44], ["op": "children", "id": 4, "ids": [40, 43]]])))
    }

    /// A scroll view's content is rows too (a sequencer's step strips redraw
    /// eight times a second), unless the projection reads it or into it.
    func testAScrollViewsContentIsRows() {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "scroll"], ["op": "create", "id": 20, "kind": "view"], ["op": "create", "id": 21, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 4, "kind": "scroll"], ["op": "create", "id": 40, "kind": "view"],
            ["op": "create", "id": 5, "kind": "scroll"], ["op": "create", "id": 50, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "children", "id": 20, "ids": [21]], ["op": "children", "id": 2, "ids": [20]],
            ["op": "children", "id": 4, "ids": [40]], ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 5, "ids": [50]],
            ["op": "children", "id": 1, "ids": [3, 2, 5]], ["op": "roots", "ids": [1]],
        ]))
        XCTAssertTrue(p.onlyListRows(wireBatch([
            ["op": "create", "id": 22, "kind": "view"], ["op": "children", "id": 20, "ids": [21, 22]],
            ["op": "frame", "id": 22, "x": 0.0, "y": 0.0, "w": 6.0, "h": 6.0],
            ["op": "style", "id": 21, "style": ["opacity": 0.4]],
            ["op": "animations", "id": 21, "specs": []],
            ["op": "destroy", "id": 22],
        ])))
        for ops: [[String: Any]] in [
            [["op": "style", "id": 40, "style": ["opacity": 0.5]]],   // a scroll in a header
            [["op": "style", "id": 50, "style": ["opacity": 0.5]]],   // a scroll holding a tablist
            [["op": "animations", "id": 1, "specs": []]],            // outside every scroll
        ] {
            XCTAssertFalse(p.onlyListRows(wireBatch(ops)), "\(ops)")
        }
    }

    /// The gate itself: a rows batch asks no sync, anything else does.
    func testOnlyARowsBatchSkipsTheProjection() {
        let p = presenter()
        let before = p.navigation.syncCalls
        p.apply(wireBatch([["op": "style", "id": 11, "style": ["opacity": 0.5]]]))
        XCTAssertEqual(p.navigation.syncCalls, before, "rows only")
        p.apply(wireBatch([["op": "style", "id": 30, "style": ["opacity": 0.5]]]))
        XCTAssertEqual(p.navigation.syncCalls, before + 1, "outside the rows")
    }

    func testFlatWrapperFillsSkipNavigationWhileABarChangeStillSyncs() throws {
        let p = presenter()
        defer { p.reset() }
        let scroll = try XCTUnwrap(p.views[1]?.scroll)
        let before = p.navigation.syncCalls
        var owed = 0
        var dependencies: ListRowDependencies?
        for row in 200..<220 {
            scroll.contentOffset.y = CGFloat((row - 200) * 40)
            if p.navigation.syncOwed { owed += 1 }
            p.apply(wireBatch([
                ["op": "create", "id": row, "kind": "view"],
                ["op": "children", "id": 1, "ids": [10, row]],
                ["op": "layoutHits", "id": row, "boxes": [[NSNull(), [0, 0, 300, 40], row + 1000]]],
                ["op": "frame", "id": row, "x": 0, "y": (row - 200) * 40, "w": 300, "h": 40],
                ["op": "content", "id": 1, "w": 300, "h": 2000],
            ]))
            XCTAssertNotNil(p.views[UInt32(row)]?.layoutHits)
            if let dependencies { XCTAssertTrue(p.listRowDependencies === dependencies) }
            else { dependencies = try XCTUnwrap(p.listRowDependencies) }
        }
        let syncs = p.navigation.syncCalls - before
        print("LANE L flat fills=20 navigation_syncs=\(syncs) sync_owed=\(owed)")
        XCTAssertEqual(owed, 0)
        XCTAssertEqual(syncs, 0)
        p.apply(wireBatch([["op": "props", "id": 50, "set": ["text": "Updated bar"]]]))
        XCTAssertEqual(p.navigation.syncCalls, before + syncs + 1, "bar changes sync in their own batch")
        XCTAssertNil(p.listRowDependencies, "a bar change invalidates the projection inputs")
    }

    func testBenchmarkNamedScrollerKeepsOneDependencyBuildAcrossFlatFills() throws {
        let p = presenter()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "props", "id": 30, "set": ["navigationKey": "collection", "navigationScroll": "content-3"]],
            ["op": "props", "id": 1, "set": ["id": "content-3", "testId": "product-grid"]],
        ]))
        let syncs = p.navigation.syncCalls, builds = p.listRowDependencyBuilds
        // Supply the projection's resolved name without a UIKit scene.
        p.listRowDependencies = ListRowDependencies(p, scrolls: ["content-3"])
        var rows: [[String: Any]] = [["view": 10, "root": 10, "epoch": 1, "index": 0, "size": 40, "measured": true]]
        var wrappers = [10], previousRoot = 10
        // The benchmark's first fill: ten creates, five child lists, four
        // paragraphs, three flat regions, content, frames, ranks, snapshot.
        for fill in 0..<20 {
            let root = 500 + fill * 20, wrapper = root + 14
            let ids = [root, root + 3, root + 4, root + 5, root + 7,
                       root + 9, root + 10, root + 11, root + 13, wrapper]
            let kinds = ["view", "button", "image", "text", "text", "button", "image", "text", "text", "view"]
            var ops: [[String: Any]] = zip(ids, kinds).map { id, kind in
                var op: [String: Any] = ["op": "create", "id": id, "kind": kind]
                if id == root + 5 || id == root + 11 {
                    op["style"] = ["overflow_x": "hidden", "overflow_y": "hidden", "line_clamp": 2]
                }
                return op
            }
            wrappers.append(wrapper)
            for (parent, children) in [(root, [root + 3, root + 9]),
                                       (root + 3, [root + 4, root + 5, root + 7]),
                                       (root + 9, [root + 10, root + 11, root + 13]),
                                       (wrapper, [root]), (1, wrappers)] {
                ops.append(["op": "children", "id": parent, "ids": children])
            }
            for id in [root + 5, root + 7, root + 11, root + 13] {
                ops.append(["op": "paragraph", "id": id, "runs": [
                    ["id": id + 10000, "parent": id, "paint": true, "props": ["text": "Product \(fill)"], "style": [:]],
                ]])
            }
            for (id, box) in [(root + 9, root + 12), (root + 3, root + 6), (root, root + 1)] {
                ops.append(["op": "layoutHits", "id": id, "boxes": [[NSNull(), [0, 0, 300, 40], box]]])
            }
            ops.append(["op": "content", "id": 1, "w": 300, "h": 2000])
            for id in [previousRoot, root, root + 3, root + 4, root + 5, root + 7,
                       root + 9, root + 10, root + 11, root + 13, wrapper] {
                ops.append(["op": "frame", "id": id, "x": 0, "y": 0, "w": 300, "h": 40])
                if id == root + 5 || id == root + 11 { ops.append(["op": "content", "id": id, "w": 300, "h": 40]) }
            }
            for id in [root + 7, root + 4, root + 5, root + 3, root + 13,
                       root + 10, root + 11, root + 9, root, wrapper] {
                ops.append(["op": "rank", "id": id, "rank": 0])
            }
            rows.append(["view": wrapper, "root": root, "epoch": 1, "index": fill + 1, "size": 40, "measured": true])
            ops.append(["op": "collections", "items": [["view": 1, "revision": fill + 2, "scrollSequence": 0,
                                                       "totalExtent": 2000, "count": 50, "rows": rows],
                                                      snapshot(4, revision: 1), snapshot(5, revision: 1)]])
            p.apply(wireBatch(ops))
            p.apply(wireBatch([]))
            XCTAssertEqual(p.views[UInt32(root + 5)]?.accessibilityLabel, "Product \(fill)")
            XCTAssertNotNil(p.views[UInt32(root)]?.layoutHits)
            XCTAssertNil(p.views[UInt32(root + 5)]?.scroll, "hidden overflow clips without making a scroller")
            previousRoot = root
        }
        let last = 500 + 19 * 20
        var retire: [[String: Any]] = [last + 14, last, last + 3, last + 4, last + 5,
                                       last + 7, last + 9, last + 10, last + 11, last + 13].map {
            ["op": "destroy", "id": $0]
        }
        retire += [
            ["op": "create", "id": 9900, "kind": "view"],
            ["op": "children", "id": 1, "ids": Array(wrappers.dropLast()) + [9900]],
            ["op": "frame", "id": last - 20, "x": 0, "y": 0, "w": 300, "h": 40],
            ["op": "frame", "id": 9900, "x": 0, "y": 800, "w": 300, "h": 1200],
            ["op": "rank", "id": 9900, "rank": 0],
            ["op": "collections", "items": [["view": 1, "revision": 22, "scrollSequence": 0,
                                              "totalExtent": 2000, "count": 50, "rows": Array(rows.dropLast())],
                                             snapshot(4, revision: 1), snapshot(5, revision: 1)]],
        ]
        p.apply(wireBatch(retire))
        print("LANE L benchmark fills=20 navigation_syncs=\(p.navigation.syncCalls - syncs) dependency_builds=\(p.listRowDependencyBuilds - builds)")
        XCTAssertEqual(p.navigation.syncCalls - syncs, 0)
        XCTAssertEqual(p.listRowDependencyBuilds - builds, 1)
        XCTAssertEqual(p.listHost.entries[1]?.snapshot.rows.count, 20)
        for op: [String: Any] in [
            ["op": "props", "id": 1, "set": ["id": "other"]],
            ["op": "children", "id": 1, "ids": [10, 50]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
            ["op": "rank", "id": 1, "rank": 2],
            ["op": "props", "id": 50, "set": ["text": "Updated bar"]],
            ["op": "props", "id": 70, "set": ["accessibilityLabel": "Updated tab"]],
            ["op": "props", "id": 30, "set": ["navigationKey": "other"]],
        ] {
            XCTAssertFalse(p.onlyListRows(wireBatch([op])))
        }
        let nested = wireBatch([["op": "style", "id": last - 20 + 5, "style": ["overflow_y": "auto"]]])
        XCTAssertTrue(p.onlyListRows(nested))
        XCTAssertFalse(p.onlyRowFill(nested), "a real nested scroller still needs the page passes")
        p.apply(wireBatch([["op": "props", "id": 50, "set": ["text": "Updated bar"]]]))
        XCTAssertEqual(p.navigation.syncCalls - syncs, 1, "a bar change syncs in its own batch")
        XCTAssertNil(p.listRowDependencies)
    }

    func testFlatRegionWithdrawalAndPromotionStayWithinTheirRows() {
        let p = presenter()
        defer { p.reset() }
        XCTAssertTrue(p.onlyListRows(wireBatch([
            ["op": "layoutHits", "id": 11, "boxes": [[NSNull(), [0, 0, 20, 20], 99]]],
            ["op": "layoutHitsWithdrawn", "id": 11, "ids": [99]],
            ["op": "create", "id": 99, "kind": "view"],
            ["op": "layoutPromote", "id": 99, "parent": 10, "frame": [0, 0, 20, 20], "children": []],
        ])))
        XCTAssertFalse(p.onlyListRows(wireBatch([["op": "layoutHits", "id": 50, "boxes": []]])), "a header is still projected")
        XCTAssertTrue(p.onlyListRows(wireBatch([["op": "style", "id": 11, "style": ["opacity": 0.5]]])))
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["semanticTag": "header"]]]))
        XCTAssertFalse(p.onlyListRows(wireBatch([["op": "style", "id": 11, "style": ["opacity": 1]]])), "cached ancestry follows a new header")
    }

    func testARowFillOrdersMaterialWithoutRestoringAnUncapturedScroller() throws {
        let p = presenter()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "props", "id": 50, "set": ["backgroundMaterial": "regular"]],
            ["op": "create", "id": 300, "kind": "scroll", "props": ["scrollFollowEnd": "true"], "style": ["overflow_y": "scroll"]],
            ["op": "children", "id": 30, "ids": [50, 1, 70, 5, 300]],
            ["op": "frame", "id": 300, "x": 0, "y": 0, "w": 300, "h": 100],
            ["op": "content", "id": 300, "w": 300, "h": 1000],
        ]))
        let scroll = try XCTUnwrap(p.views[300]?.scroll)
        let header = try XCTUnwrap(p.views[50]), material = try XCTUnwrap(header.materialView)
        let background = header.glassSlot ?? material
        XCTAssertTrue(p.materialNodes.contains(50))
        header.bringSubviewToFront(background)
        scroll.contentOffset.y = 200
        let fill = wireBatch([
            ["op": "create", "id": 200, "kind": "view"],
            ["op": "children", "id": 1, "ids": [10, 200]],
            ["op": "layoutHits", "id": 200, "boxes": [[NSNull(), [0, 0, 300, 40], 1200]]],
            ["op": "frame", "id": 200, "x": 0, "y": 40, "w": 300, "h": 40],
        ])
        XCTAssertTrue(p.onlyListRows(fill))
        XCTAssertTrue(p.onlyRowFill(fill))
        let before = p.navigation.syncCalls
        p.apply(fill)
        XCTAssertEqual(scroll.contentOffset.y, 200, accuracy: 0.5)
        XCTAssertTrue(header.subviews.first === background, "material still goes behind its content")
        XCTAssertEqual(p.navigation.syncCalls, before)
    }

    func testARowFillStillFlushesAPendingAuthoredOffset() throws {
        let p = presenter()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "create", "id": 300, "kind": "scroll", "style": ["overflow_y": "scroll"]],
            ["op": "children", "id": 30, "ids": [50, 1, 70, 5, 300]],
            ["op": "frame", "id": 300, "x": 0, "y": 0, "w": 300, "h": 100],
            ["op": "content", "id": 300, "w": 300, "h": 2000],
        ]))
        let node = try XCTUnwrap(p.views[300]), scroll = try XCTUnwrap(node.scroll)
        node.pendingScrollTop = 80
        let before = p.navigation.syncCalls
        p.apply(wireBatch([
            ["op": "create", "id": 200, "kind": "view"],
            ["op": "children", "id": 1, "ids": [10, 200]],
            ["op": "frame", "id": 200, "x": 0, "y": 40, "w": 300, "h": 40],
        ]))
        XCTAssertEqual(scroll.contentOffset.y, 80, accuracy: 0.5)
        XCTAssertNil(node.pendingScrollTop)
        XCTAssertFalse(p.pendingScrolls.contains(300))
        XCTAssertEqual(p.navigation.syncCalls, before)
    }

    func testARowsBatchFollowsAReplacedPhysicalScrollBackend() throws {
        for snapshotsOnly in [false, true] {
            let p = presenter()
            defer { p.reset() }
            p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 300)
            p.apply(wireBatch([["op": "frame", "id": 30, "x": 0, "y": 0, "w": 300, "h": 300]]))
            let list = try XCTUnwrap(p.views[1]), original = try XCTUnwrap(list.scroll)
            XCTAssertTrue(p.navigation.scrollsToTopSelection === original)
            let replacement = UIScrollView(frame: original.frame)
            replacement.contentSize = original.contentSize
            list.addSubview(replacement)
            list.mountNativeScrollView(replacement)
            if snapshotsOnly {
                p.apply(wireBatch([["op": "collections", "items": [snapshot(1, revision: 2), snapshot(4, revision: 2), snapshot(5, revision: 2)]]]))
            } else {
                p.apply(wireBatch([
                    ["op": "create", "id": 200, "kind": "view"],
                    ["op": "children", "id": 1, "ids": [10, 200]],
                    ["op": "frame", "id": 200, "x": 0, "y": 40, "w": 300, "h": 40],
                ]))
            }
            XCTAssertTrue(p.navigation.scrollsToTopSelection === replacement)
            XCTAssertTrue(replacement.scrollsToTop)
            XCTAssertFalse(original.scrollsToTop)
        }
    }

    func testFlatShopRowsDoNotSyncNavigationDuringCollectionScroll() throws {
        let session = ExactApp.shared.makeSession(label: "flat-fill-navigation")
        session.runtime.useFlatNodes(true)
        session.clock = 0
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 240))
        let host = UIViewController(), view = ExactView(session: session)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.frame = window.bounds
        defer { session.destroy(); window.isHidden = true }
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_LAYOUT_ONLY_ROW_PLAN"])
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.layoutIfNeeded()
        let p = session.presenter
        p.listHost.settle()
        session.drainFill()
        let list = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "list" })
        let scroll = try XCTUnwrap(list.scroll)
        let before = p.navigation.syncCalls
        var hits = 0, owed = 0
        session.onApplyForTests = { batch in
            hits += batch.ops.filter { $0.op == .layoutHits }.count
            if p.navigation.syncOwed { owed += 1 }
        }
        for y in [300, 600, 900, 1200, 900, 600, 300, 0] {
            scroll.contentOffset.y = CGFloat(y)
            p.listHost.settle()
            session.drainFill()
        }
        session.onApplyForTests = nil
        let syncs = p.navigation.syncCalls - before
        print("LANE L shop scroll layout_hits=\(hits) navigation_syncs=\(syncs) sync_owed=\(owed)")
        XCTAssertGreaterThan(hits, 0, "the scroll builds rows with omitted wrappers")
        XCTAssertEqual(owed, 0)
        XCTAssertEqual(syncs, 0)
    }

    /// A batch that changes a list's rows configures the controls of the
    /// rows it builds, and leaves those of the rows it keeps as they were;
    /// a row it moves under a box that says it is unavailable takes that.
    func testARowsBatchLeavesTheControlsOfTheRowsItKeeps() throws {
        let p = Presenter()
        func row(_ id: Int, y: Double) -> [[String: Any]] {
            [["op": "create", "id": id, "kind": "view"],
             ["op": "create", "id": id + 1, "kind": "control", "props": ["type": "checkbox", "accessibilityLabel": "Done \(id)"]],
             ["op": "children", "id": id, "ids": [id + 1]],
             ["op": "frame", "id": id, "x": 0.0, "y": y, "w": 300.0, "h": 48.0],
             ["op": "frame", "id": id + 1, "x": 16.0, "y": 13.0, "w": 22.0, "h": 22.0]]
        }
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 50, "totalExtent": 2400,
                                              "rows": [["view": 10, "root": 10, "epoch": 1]]]]],
            ["op": "create", "id": 30, "kind": "view"],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 40, "kind": "view", "props": ["accessibilityDisabled": "true"]],
        ] + row(10, y: 0) + [
            ["op": "children", "id": 1, "ids": [10]], ["op": "children", "id": 30, "ids": [1, 40]], ["op": "roots", "ids": [30]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0],
        ]))
        let kept = try XCTUnwrap(p.controls.controls[11] as? ExactCheckbox)
        XCTAssertEqual(kept.accessibilityLabel, "Done 10")
        kept.accessibilityLabel = "as the last sync left it"

        p.apply(wireBatch(row(20, y: 48) + [["op": "children", "id": 1, "ids": [10, 20]],
                                            ["op": "content", "id": 1, "w": 300.0, "h": 2400.0]]))
        XCTAssertEqual(try XCTUnwrap(p.controls.controls[21]).accessibilityLabel, "Done 20", "a new row's control is configured")
        XCTAssertEqual(kept.accessibilityLabel, "as the last sync left it", "a kept row's control is not configured again")
        XCTAssertFalse(kept.ariaUnavailable)

        p.apply(wireBatch([["op": "children", "id": 1, "ids": [20]], ["op": "children", "id": 40, "ids": [10]]]))
        XCTAssertTrue(kept.ariaUnavailable, "a row moved under an unavailable box takes it")
        XCTAssertEqual(kept.accessibilityLabel, "Done 10")
    }
}
#endif
