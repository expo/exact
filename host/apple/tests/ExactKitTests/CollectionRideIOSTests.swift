#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections

/// LLP 1010 §6.12: a list's rows move as a collection view's batch update
/// moves its cells, each from where it showed, on UIKit's batch spring (a
/// response of 0.3 s, no bounce), when a row leaves with an exit animation
/// or a runner commit moved them (`Batch.rides`); the port takes its new
/// place at once.
final class CollectionRideIOSTests: XCTestCase {
    private var window: UIWindow!
    override func tearDown() { window?.isHidden = true; window = nil; super.tearDown() }
    private func presenter() -> Presenter {
        let p = Presenter()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 400)
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        return p
    }
    /// Rows `ids` at consecutive logical positions from `first`.
    private func rows(_ ids: [Int], from first: Int = 47) -> [[String: Any]] {
        ids.enumerated().map { ["view": $1, "root": $1, "epoch": 1, "index": first + $0] }
    }
    private func ride(_ view: UIView) -> CASpringAnimation? {
        (view.layer.animationKeys() ?? []).compactMap { view.layer.animation(forKey: $0) as? CASpringAnimation }.first { $0.keyPath == "position.y" }
    }
    /// A list of three 40-point rows at the end of 2,000 points, its port at that end.
    private func list(_ p: Presenter) throws -> UIScrollView {
        p.apply(wireBatch([["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 50, "totalExtent": 2000, "rows": rows([10, 12, 14]), "correction": NSNull()]]],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 10, "kind": "view"], ["op": "create", "id": 12, "kind": "view"], ["op": "create", "id": 14, "kind": "view"],
            ["op": "children", "id": 1, "ids": [10, 12, 14]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0],
            ["op": "frame", "id": 10, "x": 0.0, "y": 1880.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 12, "x": 0.0, "y": 1920.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 14, "x": 0.0, "y": 1960.0, "w": 300.0, "h": 40.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 2000.0]]))
        let scroll = try XCTUnwrap(p.views[1]?.scroll)
        p.apply(wireBatch([["op": "collections", "items": [["view": 1, "revision": 2, "scrollSequence": 0, "count": 50, "totalExtent": 2000, "rows": rows([10, 12, 14]),
            "correction": ["scrollSequence": "0", "offset": 1700, "smooth": false]]]]]))
        XCTAssertEqual(scroll.contentOffset.y, 1700, accuracy: 0.5, "at its end")
        return scroll
    }

    /// A row that leaves with an exit animation at a followed end (a deleted
    /// message fading): the rows before it come down over the gap with it, a
    /// row the port's move mounts above them comes down with them, and the
    /// row after it stays where it showed.
    func testARemovalMovesItsRowsFromWhereTheyShowed() throws {
        let p = presenter()
        let scroll = try list(p)
        let leaving = try XCTUnwrap(p.views[12]), seq = String(try XCTUnwrap(p.listHost.entries[1]?.cursor.sequence))
        p.apply(wireBatch([["op": "collections", "items": [["view": 1, "revision": 3, "scrollSequence": 0, "count": 49, "totalExtent": 1960, "rows": [["view": 8, "root": 8, "epoch": 1, "index": 46], ["view": 10, "root": 10, "epoch": 1, "index": 47], ["view": 14, "root": 14, "epoch": 1, "index": 48]],
            "correction": ["scrollSequence": seq, "offset": 1660, "smooth": true]]]],
            ["op": "exit", "id": 12],
            ["op": "create", "id": 8, "kind": "view"],
            ["op": "children", "id": 1, "ids": [8, 10, 14]],
            ["op": "frame", "id": 8, "x": 0.0, "y": 1840.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 14, "x": 0.0, "y": 1920.0, "w": 300.0, "h": 40.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 1960.0]]))
        XCTAssertEqual(scroll.contentOffset.y, 1660, accuracy: 0.5, "the port is at the new end at once")
        XCTAssertFalse(p.listHost.animating.contains(1), "the rows move, not the port")
        let above = try XCTUnwrap(ride(try XCTUnwrap(p.views[10])), "the row before it moves")
        XCTAssertTrue(above.isAdditive)
        XCTAssertEqual(try XCTUnwrap(above.fromValue as? CGFloat), -40, accuracy: 0.5, "from 40 points up, where it showed")
        XCTAssertEqual(above.perceptualDuration, 0.3, accuracy: 0.001)
        XCTAssertEqual(above.bounce, 0, accuracy: 0.001)
        XCTAssertEqual(try XCTUnwrap(ride(leaving)?.fromValue as? CGFloat), -40, accuracy: 0.5, "the leaving row comes down with it")
        XCTAssertEqual(try XCTUnwrap(ride(try XCTUnwrap(p.views[8]))?.fromValue as? CGFloat), -40, accuracy: 0.5, "and the row the move mounted above it")
        XCTAssertNil(ride(try XCTUnwrap(p.views[14])), "the row after it stays where it showed")
    }

    /// A batch that replaces the window the list showed (a far smooth
    /// `scrollIntoView`) keeps no row to move from where it showed: its
    /// smooth correction keeps its own motion, though a commit rode it.
    func testAWindowTheBatchReplacedKeepsItsSmoothCorrection() throws {
        let p = presenter()
        _ = try list(p)
        let seq = String(try XCTUnwrap(p.listHost.entries[1]?.cursor.sequence))
        var far = wireBatch([["op": "collections", "items": [["view": 1, "revision": 3, "scrollSequence": 0, "count": 50, "totalExtent": 2000, "rows": rows([30, 32], from: 5),
            "correction": ["scrollSequence": seq, "offset": 200, "smooth": true]]]],
            ["op": "create", "id": 30, "kind": "view"], ["op": "create", "id": 32, "kind": "view"],
            ["op": "children", "id": 1, "ids": [30, 32]],
            ["op": "frame", "id": 30, "x": 0.0, "y": 200.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 32, "x": 0.0, "y": 240.0, "w": 300.0, "h": 40.0]])
        far.rides = [1]
        p.apply(far)
        XCTAssertTrue(p.listHost.animating.contains(1), "the port moves on its own motion")
        XCTAssertNil(ride(try XCTUnwrap(p.views[30])), "no row rides")
    }

    /// A far smooth reveal whose batch also lets a shown row leave keeps its
    /// own motion: a row still leaving, like one kept mounted off the port,
    /// carries no ride.
    func testALeavingRowDoesNotTakeAFarRevealsMotion() throws {
        let p = presenter()
        _ = try list(p)
        let seq = String(try XCTUnwrap(p.listHost.entries[1]?.cursor.sequence))
        var far = wireBatch([["op": "collections", "items": [["view": 1, "revision": 3, "scrollSequence": 0, "count": 50, "totalExtent": 2000, "rows": rows([30, 32], from: 5),
            "correction": ["scrollSequence": seq, "offset": 200, "smooth": true]]]],
            ["op": "exit", "id": 12],
            ["op": "create", "id": 30, "kind": "view"], ["op": "create", "id": 32, "kind": "view"],
            ["op": "children", "id": 1, "ids": [30, 32]],
            ["op": "frame", "id": 30, "x": 0.0, "y": 200.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 32, "x": 0.0, "y": 240.0, "w": 300.0, "h": 40.0]])
        far.rides = [1]
        p.apply(far)
        XCTAssertTrue(p.listHost.animating.contains(1), "the port moves on its own motion")
    }

    /// A row the batch mounts takes the move of the run it joins, not of a
    /// row across a gap of rows not mounted.
    func testAMountedRowTakesItsRunsMoveOnly() throws {
        let p = presenter()
        let scroll = try list(p)
        let seq = String(try XCTUnwrap(p.listHost.entries[1]?.cursor.sequence))
        var grown = wireBatch([["op": "collections", "items": [["view": 1, "revision": 3, "scrollSequence": 0, "count": 50, "totalExtent": 2020,
            "rows": [["view": 2, "root": 2, "epoch": 1, "index": 3], ["view": 8, "root": 8, "epoch": 1, "index": 46]] + rows([10, 12, 14]),
            "correction": ["scrollSequence": seq, "offset": 1720, "smooth": false]]]],
            ["op": "create", "id": 2, "kind": "view"], ["op": "create", "id": 8, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2, 8, 10, 12, 14]],
            ["op": "frame", "id": 2, "x": 0.0, "y": 120.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 8, "x": 0.0, "y": 1840.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 14, "x": 0.0, "y": 1960.0, "w": 300.0, "h": 60.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 2020.0]])
        grown.rides = [1]
        p.apply(grown)
        XCTAssertEqual(scroll.contentOffset.y, 1720, accuracy: 0.5)
        XCTAssertEqual(try XCTUnwrap(ride(try XCTUnwrap(p.views[8]))?.fromValue as? CGFloat), 20, accuracy: 0.5, "the row joining the run moves with it")
        XCTAssertNil(ride(try XCTUnwrap(p.views[2])), "a row across a gap does not")
    }

    /// A runner commit that grows the last row at a followed end (a receipt
    /// under it): the rows above it go up from where they showed as the port
    /// takes the new end at once. The same move in a batch the runner did not
    /// commit (a resize) takes its place with no motion.
    func testARowThatGrowsMovesTheRowsAboveItOnlyWhenACommitRides() throws {
        let p = presenter()
        let scroll = try list(p)
        let seq = String(try XCTUnwrap(p.listHost.entries[1]?.cursor.sequence))
        var grown = wireBatch([["op": "collections", "items": [["view": 1, "revision": 3, "scrollSequence": 0, "count": 50, "totalExtent": 2020, "rows": rows([10, 12, 14]),
            "correction": ["scrollSequence": seq, "offset": 1720, "smooth": false]]]],
            ["op": "frame", "id": 14, "x": 0.0, "y": 1960.0, "w": 300.0, "h": 60.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 2020.0]])
        grown.rides = [1]
        p.apply(grown)
        XCTAssertEqual(scroll.contentOffset.y, 1720, accuracy: 0.5, "the port is at the new end at once")
        XCTAssertEqual(try XCTUnwrap(ride(try XCTUnwrap(p.views[10]))?.fromValue as? CGFloat), 20, accuracy: 0.5, "the row above goes up from 20 points below")
        XCTAssertEqual(try XCTUnwrap(ride(try XCTUnwrap(p.views[14]))?.fromValue as? CGFloat), 20, accuracy: 0.5, "and the row that grew with it")
        let seq2 = String(try XCTUnwrap(p.listHost.entries[1]?.cursor.sequence))
        for view in [10, 12, 14] { p.views[UInt32(view)]?.layer.removeAllAnimations() }
        p.apply(wireBatch([["op": "collections", "items": [["view": 1, "revision": 4, "scrollSequence": 0, "count": 50, "totalExtent": 2040, "rows": rows([10, 12, 14]),
            "correction": ["scrollSequence": seq2, "offset": 1740, "smooth": false]]]],
            ["op": "frame", "id": 14, "x": 0.0, "y": 1960.0, "w": 300.0, "h": 80.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 2040.0]]))
        XCTAssertEqual(scroll.contentOffset.y, 1740, accuracy: 0.5)
        XCTAssertNil(ride(try XCTUnwrap(p.views[10])), "no commit rode it: no motion")
    }
}
#endif
