#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class CollectionOpeningIOSTests: XCTestCase {
    // PROTO: replace a scroll under a shown bar, with and without an authored offset
    func testAReplacementStartsUnderTheBarAndAcceptsTheOldScrollTop() throws {
        let session = ExactApp.shared.makeSession(label: "collection-replacement")
        let p = session.presenter
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: .zero)
        window.frame = window.screen.bounds
        let frame = window.bounds
        defer { session.destroy(); window.isHidden = true }
        // A refused boot keeps view layout from loading the baked plan.
        XCTAssertNotNil(session.boot(plan: Data(), size: frame.size).error)
        let view = ExactView(session: session), host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.frame = host.view.bounds
        p.viewport.frame = host.view.bounds
        p.apply(wireBatch([
            ["op": "create", "id": 100, "kind": "view", "props": ["navigationBack": "back", "navigationKey": "edges"]],
            ["op": "create", "id": 1, "kind": "view", "props": ["navigationKey": "edges", "navigationScroll": "edges-scroll"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Edges", "accessibilityRole": "heading", "accessibilityHeadingLevel": "2"]],
            ["op": "create", "id": 4, "kind": "scroll", "props": ["id": "edges-scroll"], "style": ["overflow_y": "scroll", "overflow_x": "hidden"], "handlers": ["scroll"]],
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "children", "id": 100, "ids": [1]], ["op": "roots", "ids": [100]],
            ["op": "frame", "id": 100, "x": 0, "y": 0, "w": frame.width, "h": frame.height],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": frame.width, "h": frame.height],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": frame.width, "h": frame.height],
            ["op": "content", "id": 4, "w": frame.width, "h": 3000]
        ]))
        let root = try XCTUnwrap(p.views[1]), old = try XCTUnwrap(p.views[4])
        let route = try XCTUnwrap(p.navigation.controllers[root.id])
        let nav = try XCTUnwrap(p.navigation.primaryNavigation)
        p.navigation.prepareRoutes([route], in: nav)
        nav.view.layoutIfNeeded()
        let safe = route.view.safeAreaInsets, box = route.view.bounds.inset(by: safe)
        XCTAssertGreaterThan(safe.top, 0)
        p.navigation.covers[root.id] = .edges(.init(top: safe.top, right: safe.right, bottom: safe.bottom, left: safe.left))
        p.apply(wireBatch([
            ["op": "frame", "id": 1, "x": box.minX, "y": box.minY, "w": box.width, "h": box.height],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": box.width, "h": box.height]
        ]))
        old.pendingScrollTop = 80
        old.applyPendingScroll()
        var saved: Double?
        p.onScroll = { id, metrics in if id == old.id { saved = metrics[1] } }
        old.sendScrollEvent()
        let top = try XCTUnwrap(saved)
        XCTAssertEqual(top, 80, accuracy: 0.5)

        func snapshot(_ id: Int, revision: Int = 1, correction: Any = NSNull()) -> [String: Any] {
            ["view": id, "revision": revision, "scrollSequence": 0, "count": 1000, "totalExtent": 44000,
             "rows": [["view": id + 1, "root": id + 1, "epoch": 1, "index": 0]], "correction": correction]
        }
        var previous = 4
        for (id, authored) in [(10, false), (20, true)] {
            var props = ["id": "edges-scroll"]
            if authored { props["scrollTop"] = String(top) }
            p.apply(wireBatch([
                ["op": "collections", "items": [snapshot(id)]],
                ["op": "create", "id": id, "kind": "list", "props": props, "style": ["overflow_y": "scroll", "overflow_x": "hidden"]],
                ["op": "create", "id": id + 1, "kind": "view"],
                ["op": "children", "id": id, "ids": [id + 1]], ["op": "children", "id": 1, "ids": [2, id]],
                ["op": "destroy", "id": previous],
                ["op": "frame", "id": id, "x": 0, "y": 0, "w": box.width, "h": box.height],
                ["op": "frame", "id": id + 1, "x": 0, "y": 0, "w": box.width, "h": 290],
                ["op": "content", "id": id, "w": box.width, "h": 44000]
            ]))
            let node = try XCTUnwrap(p.views[UInt32(id)]), scroll = try XCTUnwrap(node.scroll)
            p.navigation.prepareRoutes([route], in: nav)
            node.layoutIfNeeded()
            XCTAssertEqual(Double(scroll.contentOffset.y + scroll.adjustedContentInset.top), authored ? top : 0, accuracy: 0.5)
            XCTAssertEqual(try XCTUnwrap(p.collections.geometry(node.id)).offset, authored ? top : 0, accuracy: 0.5)
            if !authored { p.apply(wireBatch([["op": "props", "id": id, "set": ["scrollTop": String(top)]]])) }
            p.reachSafeEdges()
            p.navigation.prepareRoutes([route], in: nav)
            XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .automatic)
            let stale: [String: Any] = ["scrollSequence": 0, "from": 0, "offset": 210.7]
            p.apply(wireBatch([["op": "collections", "items": [snapshot(id, revision: 2, correction: stale)]]]))
            XCTAssertEqual(p.collections.entries[node.id]?.snapshot.revision, 2)
            XCTAssertEqual(Double(scroll.contentOffset.y + scroll.adjustedContentInset.top), top, accuracy: 0.5, "an earlier measurement cannot replace authored intent")
            previous = id
        }
    }

    func testOpeningReportsOnlyTheVisibleWindowBeforeLaterFillSlices() throws {
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { p.reset(); window.isHidden = true }
        var wires: [Data] = []
        p.collections.onFeedback = { wires.append($0) }
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 100,
                "totalExtent": 5000, "pending": false, "rows": [["view": 10, "root": 10, "epoch": 1, "index": 0]], "correction": NSNull()]]],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 10, "kind": "view"],
            ["op": "children", "id": 1, "ids": [10]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0],
            ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 300.0, "h": 50.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 5000.0],
        ]))
        p.collections.flush()
        func limit(_ wire: Data) -> UInt32 {
            wire.subdata(in: 88..<92).withUnsafeBytes { $0.loadUnaligned(as: UInt32.self) }
        }
        XCTAssertEqual(limit(try XCTUnwrap(wires.first)), 0, "opening rescues what shows without building the offscreen lead")
        wires.removeAll()
        p.collections.changed(1)
        p.collections.flush()
        XCTAssertTrue(wires.isEmpty, "unchanged geometry with no pending lead needs no report")
        p.collections.fillSlice(1, limit: 3)
        XCTAssertEqual(limit(try XCTUnwrap(wires.first)), 3, "a later slice can build its bounded lead")
    }
}
#endif
