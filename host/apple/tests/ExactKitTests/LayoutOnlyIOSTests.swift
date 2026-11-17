#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class LayoutOnlyIOSTests: XCTestCase {
    private func fixture(_ flat: Bool, scene: UIWindowScene? = nil) throws -> (Presenter, UIWindow) {
        let p = Presenter()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: .zero)
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 400)
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "view", "handlers": ["press"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["testId": "first"]],
            ["op": "create", "id": 5, "kind": "view", "handlers": ["press"], "props": ["testId": "last"]],
            ["op": "frame", "id": 1, "x": 0.25, "y": 0.25, "w": 300, "h": 200],
            ["op": "frame", "id": 5, "x": 150, "y": 20, "w": 30, "h": 40],
            ["op": "roots", "ids": [1]],
        ]
        if flat {
            ops += [["op": "children", "id": 1, "ids": [4, 5]],
                    ["op": "frame", "id": 4, "x": 18.5, "y": 27.5, "w": 20, "h": 30],
                    ["op": "layoutHits", "id": 1, "boxes": [[4, [11.25, 13.25, 100, 100], 2], [4, [18.5, 27.5, 40, 50], 3]]]]
        } else {
            ops += [["op": "create", "id": 2, "kind": "view"], ["op": "create", "id": 3, "kind": "view"],
                    ["op": "children", "id": 1, "ids": [2, 5]], ["op": "children", "id": 2, "ids": [3]],
                    ["op": "children", "id": 3, "ids": [4]],
                    ["op": "frame", "id": 2, "x": 11.25, "y": 13.25, "w": 100, "h": 100],
                    ["op": "frame", "id": 3, "x": 7.25, "y": 14.25, "w": 40, "h": 50],
                    ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 20, "h": 30]]
        }
        p.apply(wireBatch(ops))
        return (p, window)
    }

    func testFlattenedGeometryAndBubblingMatchTheWrapperTree() throws {
        let (full, w1) = try fixture(false), (flat, w2) = try fixture(true)
        defer { full.reset(); flat.reset(); w1.isHidden = true; w2.isHidden = true }
        for id: UInt32 in [4, 5] {
            let a = try XCTUnwrap(full.views[id]), b = try XCTUnwrap(flat.views[id])
            let af = a.convert(a.bounds, to: w1), bf = b.convert(b.bounds, to: w2)
            for (a, b) in zip([af.minX, af.minY, af.width, af.height], [bf.minX, bf.minY, bf.width, bf.height]) {
                XCTAssertEqual(a, b, accuracy: 1e-9)
            }
        }
        XCTAssertEqual(full.views.count, 5)
        XCTAssertEqual(flat.views.count, 3)
        XCTAssertNil(flat.views[2]); XCTAssertNil(flat.views[3])
        var heard: [UInt32] = []
        flat.onPress = { heard.append($0) }
        let child = try XCTUnwrap(flat.views[4])
        let center = CGPoint(x: child.bounds.midX, y: child.bounds.midY)
        let hit = try XCTUnwrap(flat.views[1]?.hitTest(child.convert(center, to: flat.views[1]), with: nil) as? NodeView)
        XCTAssertTrue(hit === child)
        hit.activate(at: child.convert(center, to: w2))
        XCTAssertEqual(heard, [1])
    }

    func testPromotionReparentsInEitherBatchOrderAndInstallsTheHandler() throws {
        for outsideFirst in [false, true] {
            let (p, window) = try fixture(true)
            defer { p.reset(); window.isHidden = true }
            let child = try XCTUnwrap(p.views[4]), before = child.convert(child.bounds, to: window)
            let outer: [String: Any] = ["op": "children", "id": 1, "ids": [2, 5]]
            let inner: [[String: Any]] = [["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 3, "ids": [4]]]
            p.apply(wireBatch([
                ["op": "create", "id": 2, "kind": "view", "handlers": ["press"], "props": ["id": "promoted"]],
                ["op": "create", "id": 3, "kind": "view"],
            ] + (outsideFirst ? [outer] + inner : inner + [outer]) + [
                ["op": "layoutHits", "id": 1, "boxes": []],
                ["op": "frame", "id": 2, "x": 11.25, "y": 13.25, "w": 100, "h": 100],
                ["op": "frame", "id": 3, "x": 7.25, "y": 14.25, "w": 40, "h": 50],
                ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 20, "h": 30],
            ]))
            XCTAssertEqual(p.views.count, 5)
            let after = child.convert(child.bounds, to: window)
            for (a, b) in zip([before.minX, before.minY, before.width, before.height], [after.minX, after.minY, after.width, after.height]) {
                XCTAssertEqual(a, b, accuracy: 1e-9)
            }
            XCTAssertTrue(child.nodeSuperview === p.views[3])
            XCTAssertNil(p.views[1]?.layoutHits)
            var heard: [UInt32] = []
            p.onPress = { heard.append($0) }
            child.activate(at: child.convert(CGPoint(x: 5, y: 5), to: window))
            XCTAssertEqual(heard, [2])
        }
    }

    private final class PointTouch: UITouch {
        let point: CGPoint
        let target: UIView
        init(on target: UIView, at point: CGPoint = CGPoint(x: 5, y: 5)) {
            self.target = target
            self.point = target.convert(point, to: nil)
            super.init()
        }
        override var view: UIView? { target }
        override func location(in view: UIView?) -> CGPoint { view.map { $0.convert(point, from: nil) } ?? point }
    }

    func testDownPromotionUpKeepsThePressAndItsCapturedResponderChain() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "promotion keeps a focused descendant in its window")
        }
        for flat in [false, true] {
            for ownsPress in [false, true] {
                let (p, window) = try fixture(flat, scene: scene)
                defer { p.reset(); window.isHidden = true }
                let child = try XCTUnwrap(p.views[4])
                child.handlers = ownsPress ? ["press", "focus", "blur"] : ["focus", "blur"]
                p.apply(wireBatch([
                    ["op": "create", "id": 6, "kind": "view", "style": ["opacity": 1]],
                    ["op": "children", "id": 1, "ids": flat ? [6, 4, 5] : [6, 2, 5]],
                    ["op": "style", "id": 4, "style": ["position": "relative"]],
                ]))
                var heard: [UInt32] = []
                var focused: [UInt32] = [], blurred: [UInt32] = []
                p.onPress = { heard.append($0) }
                p.onFocus = { focused.append($0) }
                p.onBlur = { blurred.append($0) }
                XCTAssertTrue(child.becomeFirstResponder())
                let before = child.convert(child.bounds, to: window)
                let touch = PointTouch(on: child)
                child.touchesBegan([touch], with: nil)
                let contact = try XCTUnwrap(child.pressContact)
                let responder = contact.destination, downstream = contact.responderContact
                XCTAssertTrue(contact.routeActive)
                var ops: [[String: Any]] = [["op": "style", "id": 6, "style": ["opacity": 0.5]]]
                if flat {
                    ops += [
                        ["op": "create", "id": 2, "kind": "view"],
                        ["op": "layoutPromote", "id": 2, "parent": 1, "frame": [11.25, 13.25, 100, 100], "children": [[4, [7.25, 14.25, 20, 30]]], "keep": [4]],
                        ["op": "create", "id": 3, "kind": "view"],
                        ["op": "layoutPromote", "id": 3, "parent": 2, "frame": [7.25, 14.25, 40, 50], "children": [[4, [0, 0, 20, 30]]], "keep": [4]],
                        ["op": "children", "id": 1, "ids": [6, 2, 5]],
                        ["op": "children", "id": 3, "ids": [4]],
                        ["op": "children", "id": 2, "ids": [3]],
                        ["op": "layoutHits", "id": 1, "boxes": []],
                    ]
                }
                p.apply(wireBatch(ops))
                let after = child.convert(child.bounds, to: window)
                for (a, b) in zip([before.minX, before.minY, before.width, before.height], [after.minX, after.minY, after.width, after.height]) {
                    XCTAssertEqual(a, b, accuracy: 1e-9)
                }
                XCTAssertTrue(child.isFirstResponder)
                XCTAssertEqual(focused, [4])
                XCTAssertTrue(blurred.isEmpty)
                XCTAssertTrue(child.pressContact === contact)
                XCTAssertTrue(contact.routeActive)
                XCTAssertTrue(contact.destination === responder)
                XCTAssertTrue(contact.responderContact === downstream)
                child.touchesEnded([touch], with: nil)
                XCTAssertEqual(heard, [ownsPress ? 4 : 1])
                XCTAssertNil(child.pressContact)
                if flat {
                    // An authored move after promotion still cancels a held press.
                    child.touchesBegan([touch], with: nil)
                    p.apply(wireBatch([["op": "children", "id": 1, "ids": [6, 2, 4, 5]],
                                       ["op": "children", "id": 3, "ids": []]]))
                    child.touchesEnded([touch], with: nil)
                    XCTAssertEqual(heard, [ownsPress ? 4 : 1])
                }
            }
        }
    }

    func testRemovingAPaddingHitCancelsItsCapturedPress() throws {
        for flat in [false, true] {
            let (p, window) = try fixture(flat)
            defer { p.reset(); window.isHidden = true }
            let root = try XCTUnwrap(p.views[1])
            let point = CGPoint(x: 80, y: 80)
            let target = try XCTUnwrap(root.hitTest(point, with: nil) as? NodeView)
            XCTAssertEqual(target.id, flat ? 1 : 2)
            let touch = PointTouch(on: target, at: target.convert(point, from: root))
            var heard: [UInt32] = []
            p.onPress = { heard.append($0) }
            target.touchesBegan([touch], with: nil)
            let contact = try XCTUnwrap(target.pressContact)
            if flat {
                p.apply(wireBatch([["op": "destroy", "id": 4],
                                   ["op": "children", "id": 1, "ids": [5]],
                                   ["op": "layoutHits", "id": 1, "boxes": []]]))
            } else {
                p.apply(wireBatch([["op": "destroy", "id": 2], ["op": "destroy", "id": 3], ["op": "destroy", "id": 4],
                                   ["op": "children", "id": 1, "ids": [5]]]))
            }
            XCTAssertTrue(contact.cancelled)
            target.touchesEnded([touch], with: nil)
            XCTAssertTrue(heard.isEmpty)
        }
    }

    /// A painted leaf inside an omitted column is a layer of the row: a press on it
    /// still ends when the column goes, as it would with the column's view.
    func testRemovingAnOmittedColumnCancelsAPressOnItsPaintedLeaf() throws {
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": ["press"]],
            ["op": "create", "id": 6, "kind": "view",
             "style": ["width": 30.0, "height": 20.0, "background_color": [0, 122, 255, 255], "text_color": [0, 0, 0, 255],
                       "written": ["background_color"]]],
            ["op": "create", "id": 5, "kind": "view", "handlers": ["press"]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
            ["op": "frame", "id": 6, "x": 20, "y": 20, "w": 30, "h": 20],
            ["op": "frame", "id": 5, "x": 150, "y": 20, "w": 30, "h": 40],
            ["op": "children", "id": 1, "ids": [6, 5]],
            ["op": "roots", "ids": [1]],
            ["op": "layoutHits", "id": 1, "boxes": [[6, [10, 10, 60, 60], 2, 5]]],
        ]))
        XCTAssertNil(p.views[6], "the leaf is a layer")
        let root = try XCTUnwrap(p.views[1])
        let point = CGPoint(x: 35, y: 30)
        let target = try XCTUnwrap(root.hitTest(point, with: nil) as? NodeView)
        XCTAssertEqual(target.id, 1)
        let touch = PointTouch(on: target, at: target.convert(point, from: root))
        var heard: [UInt32] = []
        p.onPress = { heard.append($0) }
        target.touchesBegan([touch], with: nil)
        let contact = try XCTUnwrap(target.pressContact)
        p.apply(wireBatch([["op": "destroy", "id": 6],
                           ["op": "children", "id": 1, "ids": [5]],
                           ["op": "layoutHits", "id": 1, "boxes": []]]))
        XCTAssertTrue(contact.cancelled)
        target.touchesEnded([touch], with: nil)
        XCTAssertTrue(heard.isEmpty)
    }

    /// Presses `point` in an owner (id 1) whose children are `leaves` (blue
    /// 30x20 layers at the given origins) then `after`, removes what `removal`
    /// says, and answers whether the press ended without firing.
    private func pressEndsOnRemoval(leaves: [(UInt32, CGPoint)], after: [UInt32], boxes: [[Any]],
                                    at point: CGPoint, removal: [[String: Any]]) throws -> Bool {
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { p.reset(); window.isHidden = true }
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view", "handlers": ["press"]],
                                    ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200]]
        for (id, at) in leaves {
            ops.append(["op": "create", "id": id, "kind": "view",
                        "style": ["width": 30.0, "height": 20.0, "background_color": [0, 122, 255, 255], "text_color": [0, 0, 0, 255],
                                  "written": ["background_color"]]])
            ops.append(["op": "frame", "id": id, "x": at.x, "y": at.y, "w": 30, "h": 20])
        }
        for id in after {
            ops.append(["op": "create", "id": id, "kind": "view", "handlers": ["press"]])
            ops.append(["op": "frame", "id": id, "x": 250, "y": 150, "w": 30, "h": 40])
        }
        ops += [["op": "children", "id": 1, "ids": leaves.map(\.0) + after], ["op": "roots", "ids": [1]],
                ["op": "layoutHits", "id": 1, "boxes": boxes]]
        p.apply(wireBatch(ops))
        for (id, _) in leaves { XCTAssertNil(p.views[id], "leaf \(id) is a layer") }
        let root = try XCTUnwrap(p.views[1])
        let target = try XCTUnwrap(root.hitTest(point, with: nil) as? NodeView)
        XCTAssertEqual(target.id, 1)
        let touch = PointTouch(on: target, at: target.convert(point, from: root))
        var heard: [UInt32] = []
        p.onPress = { heard.append($0) }
        target.touchesBegan([touch], with: nil)
        let contact = try XCTUnwrap(target.pressContact)
        p.apply(wireBatch(removal))
        target.touchesEnded([touch], with: nil)
        return contact.cancelled && heard.isEmpty
    }

    func testRemovingTheInnerOfTwoColumnsThatStartTogetherCancelsAPressOnTheirLeaf() throws {
        XCTAssertTrue(try pressEndsOnRemoval(
            leaves: [(6, CGPoint(x: 20, y: 20))], after: [],
            boxes: [[6, [10, 10, 100, 100], 2], [6, [15, 15, 50, 50], 3]], at: CGPoint(x: 35, y: 30),
            removal: [["op": "destroy", "id": 6], ["op": "children", "id": 1, "ids": []],
                      ["op": "layoutHits", "id": 1, "boxes": [[NSNull(), [10, 10, 100, 100], 2]]]]))
    }

    func testAlikeLeavesInTwoColumnsStayApartSoRemovingOneColumnCancelsItsPress() throws {
        XCTAssertTrue(try pressEndsOnRemoval(
            leaves: [(6, CGPoint(x: 20, y: 20)), (7, CGPoint(x: 50, y: 20))], after: [],
            boxes: [[6, [20, 20, 30, 20], 2, 7], [7, [50, 20, 30, 20], 3]], at: CGPoint(x: 35, y: 30),
            removal: [["op": "destroy", "id": 6], ["op": "children", "id": 1, "ids": [7]],
                      ["op": "layoutHits", "id": 1, "boxes": [[7, [50, 20, 30, 20], 3]]]]))
    }

    func testRemovingAColumnCancelsAPressOnALeafOverflowingIt() throws {
        XCTAssertTrue(try pressEndsOnRemoval(
            leaves: [(6, CGPoint(x: 20, y: 10))], after: [5],
            boxes: [[6, [10, 10, 20, 60], 2, 5]], at: CGPoint(x: 45, y: 20),
            removal: [["op": "destroy", "id": 6], ["op": "children", "id": 1, "ids": [5]],
                      ["op": "layoutHits", "id": 1, "boxes": []]]))
    }

    func testAWithdrawnColumnCancelsAPressWhileItsGeometryIsWithheld() throws {
        XCTAssertTrue(try pressEndsOnRemoval(
            leaves: [(6, CGPoint(x: 20, y: 20))], after: [5],
            boxes: [[6, [10, 10, 60, 60], 2, 5]], at: CGPoint(x: 35, y: 30),
            removal: [["op": "destroy", "id": 6], ["op": "children", "id": 1, "ids": [5]],
                      ["op": "layoutHitsWithdrawn", "id": 1, "ids": [2]]]))
    }

    func testAPointerDownOutsideTheRowCanRemoveItsPaddingHitBeforeThePressBegins() throws {
        for flat in [false, true] {
            let (p, window) = try fixture(flat)
            defer { p.reset(); window.isHidden = true }
            p.apply(wireBatch([
                ["op": "create", "id": 6, "kind": "view", "handlers": ["pointerdown"]],
                ["op": "children", "id": 6, "ids": [1]], ["op": "roots", "ids": [6]],
                ["op": "frame", "id": 6, "x": 0, "y": 0, "w": 400, "h": 400],
            ]))
            let root = try XCTUnwrap(p.views[1]), observer = try XCTUnwrap(p.views[6])
            let target = try XCTUnwrap(root.hitTest(CGPoint(x: 80, y: 80), with: nil) as? NodeView)
            let touch = PointTouch(on: target, at: target.convert(CGPoint(x: 80, y: 80), from: root))
            let pointer = try XCTUnwrap(observer.gestureRecognizers?.first { $0 is PointerRecognizer } as? PointerRecognizer)
            var heard: [UInt32] = [], downs = 0
            p.onPress = { heard.append($0) }
            p.onPointer = { _, kind, _ in
                guard kind == .down else { return }
                downs += 1
                p.apply(wireBatch([
                    ["op": "destroy", "id": 2], ["op": "destroy", "id": 3], ["op": "destroy", "id": 4],
                    ["op": "children", "id": 1, "ids": [5]], ["op": "layoutHits", "id": 1, "boxes": []],
                ]))
            }
            pointer.touchesBegan([touch], with: UIEvent())
            XCTAssertEqual(downs, 1)
            target.touchesBegan([touch], with: nil)
            pointer.touchesEnded([touch], with: UIEvent())
            target.touchesEnded([touch], with: nil)
            XCTAssertTrue(heard.isEmpty, "flat=\(flat): \(heard)")
        }
    }

    func testAPaddingContactSurvivesLayoutAndPromotionButEndsWithItsNode() throws {
        for promote in [false, true] {
            let (p, window) = try fixture(true)
            defer { p.reset(); window.isHidden = true }
            let root = try XCTUnwrap(p.views[1])
            var heard: [UInt32] = []
            p.onPress = { heard.append($0) }
            for remove in [false, true] {
                let target = p.views[2] ?? root
                let touch = PointTouch(on: target, at: target.convert(CGPoint(x: 80, y: 80), from: root))
                target.touchesBegan([touch], with: nil)
                let contact = try XCTUnwrap(target.pressContact)
                if promote, p.views[2] == nil {
                    p.apply(wireBatch([
                        ["op": "create", "id": 2, "kind": "view"],
                        ["op": "layoutPromote", "id": 2, "parent": 1, "frame": [11.25, 13.25, 100, 100], "children": [[4, [7.25, 14.25, 20, 30]]], "keep": [4]],
                        ["op": "children", "id": 1, "ids": [2, 5]],
                        ["op": "layoutHits", "id": 1, "boxes": []],
                    ]))
                } else if !promote {
                    p.apply(wireBatch([["op": "layoutHits", "id": 1, "boxes": [[4, [11.25, 13.25, 101, 100], 2], [4, [18.5, 27.5, 40, 50], 3]]]]))
                }
                if remove {
                    if promote { p.apply(wireBatch([["op": "destroy", "id": 2]])) }
                    else { p.apply(wireBatch([["op": "layoutHits", "id": 1, "boxes": []]])) }
                    XCTAssertTrue(contact.cancelled)
                } else { XCTAssertFalse(contact.cancelled) }
                target.touchesEnded([touch], with: nil)
                XCTAssertEqual(heard, [1])
            }
        }
    }

    func testAnInnerPaddingContactSurvivesItsOuterWrappersPromotion() throws {
        for innerFirst in [false, true] {
            let (p, window) = try fixture(true)
            defer { p.reset(); window.isHidden = true }
            let root = try XCTUnwrap(p.views[1])
            let touch = PointTouch(on: root, at: CGPoint(x: 50, y: 70))
            var heard: [UInt32] = []
            p.onPress = { heard.append($0) }
            root.touchesBegan([touch], with: nil)
            let contact = try XCTUnwrap(root.pressContact)
            XCTAssertEqual(contact.layoutHit?.id, 3)
            let old: [String: Any] = ["op": "layoutHits", "id": 1, "boxes": []]
            let new: [String: Any] = ["op": "layoutHits", "id": 2, "boxes": [[4, [7.25, 14.25, 40, 50], 3]]]
            p.apply(wireBatch([
                ["op": "create", "id": 2, "kind": "view"],
                ["op": "layoutPromote", "id": 2, "parent": 1, "frame": [11.25, 13.25, 100, 100], "children": [[4, [7.25, 14.25, 20, 30]]], "keep": [4]],
                ["op": "children", "id": 1, "ids": [2, 5]],
            ] + (innerFirst ? [new, old] : [old, new])))
            XCTAssertFalse(contact.cancelled)
            XCTAssertTrue(contact.layoutHit === p.views[2]?.layoutHits?.boxes.first?.2)
            root.touchesEnded([touch], with: nil)
            XCTAssertEqual(heard, [1])
        }
    }

    func testTheDefaultAgentHoldsAnOmittedBoxAndCancelsItsRemoval() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "recognized input needs a scene") }
        for flat in [false, true] {
            let (session, window) = try grid(flat, scene: scene)
            defer { session.destroy(); window.isHidden = true }
            try requireFront(window)
            let agent = Agent(session: session), p = session.presenter
            let card = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "card-0-0" })
            let bytes = Data(session.agent("{\"op\":\"node\",\"id\":\(card.id)}").utf8)
            let node = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
            let parent = try XCTUnwrap(node["parent"] as? Int)
            let parentBytes = Data(session.agent("{\"op\":\"node\",\"id\":\(parent)}").utf8)
            let parentNode = try XCTUnwrap(JSONSerialization.jsonObject(with: parentBytes) as? [String: Any])
            let id = try XCTUnwrap(parentNode["parent"] as? Int)
            let host = try XCTUnwrap(flat ? agent.flatBoxForRequest(["id": id])?.host : p.views[UInt32(id)])
            host.handlers = ["press"]
            var heard: [UInt32] = []
            p.onPress = { heard.append($0) }
            let rect = flat ? try XCTUnwrap(agent.flatBoxForRequest(["id": id])).rect : host.bounds
            let point = agent.box(host.container, region: CGRect(x: rect.minX + 1, y: rect.minY + 1, width: 0, height: 0)).origin
            let drag = agent.tap(["id": id, "scrollDrag": [0.0, 20.0], "ms": 80.0])
            XCTAssertTrue((drag["error"] as? String)?.hasPrefix("drag #\(id) starts on nothing the agent scrolls") == true, "\(drag)")
            for remove in [false, true] {
                let down = agent.tap(["id": id, "phase": "down", "x": point.x, "y": point.y])
                XCTAssertNil(down["error"], "\(down)")
                XCTAssertEqual(down["delivery"] as? String, "recognized", "\(down)")
                XCTAssertEqual(down["contact"] as? Int, id, "\(down)")
                if remove {
                    if flat { p.apply(wireBatch([["op": "layoutHits", "id": host.id, "boxes": []]])) }
                    else { p.apply(wireBatch([["op": "destroy", "id": id]])) }
                }
                let up = agent.tap(["phase": "up"])
                XCTAssertEqual(up["delivery"] as? String, "recognized", "\(up)")
                XCTAssertEqual(heard, [host.id])
            }
        }
    }

    func testAnOpaqueOwnerKeepsItsViewlessHitRegionsWhenRestyled() throws {
        let (p, window) = try fixture(true)
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 6, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [5, 6]],
            ["op": "layoutHits", "id": 1, "boxes": []],
            ["op": "frame", "id": 6, "x": 0, "y": 0, "w": 20, "h": 20],
            ["op": "layoutHits", "id": 6, "boxes": [[NSNull(), [0, 0, 100, 100], 2]]],
            ["op": "rank", "id": 6, "rank": 0],
        ]))
        let owner = try XCTUnwrap(p.views[6])
        XCTAssertNil(p.flats.leaves[6])
        XCTAssertTrue(p.views[1]?.hitTest(CGPoint(x: 75, y: 25), with: nil) === owner)
        p.apply(wireBatch([
            ["op": "style", "id": 6, "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "rank", "id": 6, "rank": 0],
        ]))
        XCTAssertTrue(p.views[6] === owner)
        XCTAssertNotNil(owner.layoutHits)
        XCTAssertTrue(p.views[1]?.hitTest(CGPoint(x: 75, y: 25), with: nil) === owner)
    }

    func testAnAuthoredMoveDuringPromotionStillCancelsTheContact() throws {
        let (p, window) = try fixture(true)
        defer { p.reset(); window.isHidden = true }
        let child = try XCTUnwrap(p.views[4])
        let touch = PointTouch(on: child)
        var heard: [UInt32] = []
        p.onPress = { heard.append($0) }
        child.touchesBegan([touch], with: nil)
        let contact = try XCTUnwrap(child.pressContact)
        p.apply(wireBatch([
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "layoutPromote", "id": 2, "parent": 1, "frame": [11.25, 13.25, 100, 100],
             "children": [[4, [7.25, 14.25, 20, 30]]], "keep": []],
            ["op": "children", "id": 1, "ids": [2, 5]],
            ["op": "children", "id": 2, "ids": [4]],
        ]))
        XCTAssertTrue(contact.cancelled)
        child.touchesEnded([touch], with: nil)
        XCTAssertTrue(heard.isEmpty)
    }

    func testPromotedFragmentsKeepGapHitsAndTheNumericTapBox() throws {
        let (p, window) = try fixture(true)
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 6, "kind": "view", "handlers": ["press"]],
            ["op": "frame", "id": 6, "x": 0, "y": 0, "w": 120, "h": 50],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "layoutPromote", "id": 2, "parent": 1, "frame": [0, 0, 120, 50], "children": [[4, [0, 0, 20, 20]]], "keep": [4]],
            ["op": "children", "id": 1, "ids": [6, 2]],
            ["op": "children", "id": 2, "ids": [4]],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 20, "h": 20],
            ["op": "fragments", "id": 2, "fragments": [[0, 0, 50, 50, 0, 0, 0, 0], [70, 0, 50, 50, 0, 0, 70, -50]]],
            ["op": "layoutHits", "id": 1, "boxes": []],
        ]))
        let root = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[2]), earlier = try XCTUnwrap(p.views[6])
        XCTAssertTrue(root.hitTest(CGPoint(x: 60, y: 25), with: nil) === earlier)
        XCTAssertTrue(root.hitTest(CGPoint(x: 90, y: 25), with: nil) === box)
        XCTAssertEqual(box.tapBox(box.frame), CGRect(x: 0, y: 0, width: 50, height: 50))
        XCTAssertNil(root.layoutHits)
    }

    func testAnEmptyWrapperStillCoversAnEarlierSiblingWithoutALayer() throws {
        let p = Presenter(), window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": ["press"]],
            ["op": "create", "id": 2, "kind": "view", "handlers": ["press"]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 100],
        ]))
        let parent = try XCTUnwrap(p.views[1]), earlier = try XCTUnwrap(p.views[2])
        XCTAssertTrue(parent.hitTest(CGPoint(x: 10, y: 10), with: nil) === earlier)
        let layers = parent.layer.sublayers?.count
        p.apply(wireBatch([["op": "layoutHits", "id": 1, "boxes": [[NSNull(), [0, 0, 50, 50], 3]]]]))
        XCTAssertTrue(parent.hitTest(CGPoint(x: 10, y: 10), with: nil) === parent)
        XCTAssertTrue(parent.hitTest(CGPoint(x: 60, y: 60), with: nil) === earlier)
        XCTAssertEqual(parent.layer.sublayers?.count, layers)
        p.apply(wireBatch([["op": "layoutHits", "id": 1, "boxes": []]]))
        XCTAssertTrue(parent.hitTest(CGPoint(x: 10, y: 10), with: nil) === earlier)
    }
    func testARetiredRowReusesItsViewsAndReplacesItsViewlessRegions() throws {
        let p = Presenter(), window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { p.reset(); window.isHidden = true }
        func row(_ base: Int, _ width: Int) -> [[String: Any]] {
            [["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 50,
                "totalExtent": 2000, "rows": [["view": base, "root": base, "epoch": 1]], "correction": NSNull()]]],
             ["op": "create", "id": base, "kind": "view", "props": ["testId": "row-\(base)"]],
             ["op": "create", "id": base + 1, "kind": "view", "props": ["testId": "child-\(base)"]],
             ["op": "children", "id": base, "ids": [base + 1]],
             ["op": "frame", "id": base, "x": 0, "y": 0, "w": 200, "h": 50],
             ["op": "frame", "id": base + 1, "x": 10, "y": 10, "w": 20, "h": 20],
             ["op": "layoutHits", "id": base, "boxes": [[base + 1, [0, 0, width, 50], base + 2]]],
             ["op": "children", "id": 1, "ids": [base]]]
        }
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]]]
            + row(10, 100) + [["op": "roots", "ids": [1]],
                             ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 300],
                             ["op": "content", "id": 1, "w": 300, "h": 2000]]))
        let oldRow = try XCTUnwrap(p.views[10]), oldChild = try XCTUnwrap(p.views[11])
        XCTAssertTrue(oldRow.hitTest(CGPoint(x: 75, y: 25), with: nil) === oldRow)
        let next = row(20, 50).filter { $0["op"] as? String != "layoutHits" }
        p.apply(wireBatch([["op": "destroy", "id": 10], ["op": "destroy", "id": 11]] + next))
        let newRow = try XCTUnwrap(p.views[20]), newChild = try XCTUnwrap(p.views[21])
        XCTAssertTrue(newRow === oldRow && newChild === oldChild)
        XCTAssertNil(newRow.layoutHits, "the retired item's regions do not follow its pooled view")
        p.apply(wireBatch([["op": "layoutHits", "id": 20, "boxes": [[21, [0, 0, 50, 50], 22]]]]))
        XCTAssertEqual(newRow.layoutHits?.boxes.map(\.1), [CGRect(x: 0, y: 0, width: 50, height: 50)])
        XCTAssertTrue(newRow.hitTest(CGPoint(x: 15, y: 15), with: nil) === newChild)
        XCTAssertEqual(p.pool.count, 0)
    }

    private func grid(_ flat: Bool, scene: UIWindowScene, planKey: String = "EXACT_LAYOUT_ONLY_ROW_PLAN") throws -> (ExactSession, UIWindow) {
        let session = ExactApp.shared.makeSession(label: "layout-only-row")
        session.runtime.useFlatNodes(flat)
        let window = UIWindow(windowScene: scene)
        window.frame = scene.screen.bounds
        let view = ExactView(session: session), controller = UIViewController()
        window.rootViewController = controller
        window.makeKeyAndVisible()
        controller.view.addSubview(view)
        view.frame = controller.view.bounds
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment[planKey])
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.layoutIfNeeded()
        return (session, window)
    }

    func testShopRowWindowFramesAndNumericInspection() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "numeric input uses a window scene coordinate space")
        }
        let (full, w1) = try grid(false, scene: scene), (flat, w2) = try grid(true, scene: scene)
        defer { full.destroy(); flat.destroy(); w1.isHidden = true; w2.isHidden = true }
        let p = flat.presenter
        XCTAssertLessThan(p.views.count, full.presenter.views.count)
        for (id, b) in p.views {
            let a = try XCTUnwrap(full.presenter.views[id], "view \(id)")
            let af = a.convert(a.bounds, to: w1), bf = b.convert(b.bounds, to: w2)
            for (x, y) in zip([af.minX, af.minY, af.width, af.height], [bf.minX, bf.minY, bf.width, bf.height]) {
                XCTAssertEqual(x, y, accuracy: 1e-8, "view \(id)")
            }
        }
        let card = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "card-0-0" })
        let tree = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(flat.agent("{\"op\":\"tree\"}").utf8)) as? [String: Any])
        let nodes = try XCTUnwrap(tree["nodes"] as? [[String: Any]])
        let node = try XCTUnwrap(nodes.first { ($0["id"] as? NSNumber)?.uint32Value == card.id })
        let wrapper = try XCTUnwrap((node["parent"] as? NSNumber)?.uint32Value)
        XCTAssertNil(p.views[wrapper])
        let agent = Agent(session: flat), req = ["id": Int(wrapper)]
        XCTAssertEqual(agent.reveal(req)["revealed"] as? Int, Int(wrapper))
        let layout = try XCTUnwrap(agent.layout(req)["node"] as? [String: Any])
        XCTAssertEqual((layout["native"] as? [String: Any])?["layoutOnly"] as? Bool, true)
        try requireFront(w2)
        let aim = agent.aim(["id": Int(wrapper), "aim": ["at": [1.0, 1.0]]])
        XCTAssertNil(aim["error"], "\(aim)")
        XCTAssertEqual((aim["aim"] as? [String: Any])?["node"] as? Int, Int(wrapper))
        let box = try XCTUnwrap(agent.flatBoxForRequest(req))
        let holder = box.host, alpha = holder.alpha
        holder.alpha = 0
        let hidden = try XCTUnwrap(agent.layout(req)["node"] as? [String: Any])
        XCTAssertEqual((hidden["visible"] as? [String: Any])?["hidden"] as? Bool, true)
        holder.alpha = alpha
        let clip = holder.style
        holder.style["overflow_x"] = .string("hidden")
        holder.style["overflow_y"] = .string("hidden")
        holder.clipsToBounds = true
        holder.bounds.size = .zero
        let clipped = try XCTUnwrap(agent.layout(req)["node"] as? [String: Any])
        XCTAssertEqual((clipped["visible"] as? [String: Any])?["clipped"] as? Bool, true)
        holder.style = clip
    }

    func testAnOrdinaryTapOnAnOmittedBoxLightDismissesAnOutsidePopover() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "agent input uses a window") }
        guard ExactEnv.agentPaints else { throw missing("agent", "run with EXACT_AGENT=1 and an agent socket") }
        for flat in [false, true] {
            let (session, window) = try grid(flat, scene: scene, planKey: "EXACT_LAYOUT_ONLY_PAINT_PLAN")
            defer { session.destroy(); window.isHidden = true }
            try requireFront(window)
            let agent = Agent(session: session), p = session.presenter
            let image = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "last" })
            let bytes = Data(session.agent("{\"op\":\"node\",\"id\":\(image.id)}").utf8)
            let node = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
            let id = try XCTUnwrap(node["parent"] as? Int)
            let opener = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "opener" })
            for point in [false, true] {
                let open = agent.tap(["id": Int(opener.id)])
                XCTAssertNil(open["error"], "\(open)")
                XCTAssertNotNil(p.menus.presented.first { $0.props["id"] == "choices" })
                var req: [String: Any] = ["id": id]
                if point { req["at"] = [1.0, 1.0] }
                let reply = agent.tap(req)
                XCTAssertNil(reply["error"], "\(reply)")
                XCTAssertNil(p.menus.presented.first { $0.props["id"] == "choices" })
            }
        }
    }

    /// A numeric tap on a wrapper is refused where a later wrapper's empty area
    /// covers it, with the wrappers omitted as with their views.
    func testANumericTapOnACoveredOmittedWrapperIsRefused() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "agent input uses a window") }
        guard ExactEnv.agentPaints else { throw missing("agent", "run with EXACT_AGENT=1 and an agent socket") }
        for flat in [false, true] {
            let (session, window) = try grid(flat, scene: scene, planKey: "EXACT_LAYOUT_ONLY_PAINT_PLAN")
            defer { session.destroy(); window.isHidden = true }
            try requireFront(window)
            let agent = Agent(session: session), p = session.presenter
            let text = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "a-text" })
            let bytes = Data(session.agent("{\"op\":\"node\",\"id\":\(text.id)}").utf8)
            let node = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
            let column = try XCTUnwrap(node["parent"] as? Int)
            XCTAssertEqual(p.views[UInt32(column)] == nil, flat, "the column is omitted only when flattening")
            let reply = agent.tap(["id": column, "at": [70.0, 50.0]])
            let error = try XCTUnwrap(reply["error"] as? String, "\(reply)")
            XCTAssertTrue(error.contains("covers its middle"), error)
        }
    }

    func testRankedDescendantsKeepPaintAndHitOrderBeforeAndAfterRestyling() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "paint order uses a window") }
        let (full, w1) = try grid(false, scene: scene, planKey: "EXACT_LAYOUT_ONLY_PAINT_PLAN")
        let (flat, w2) = try grid(true, scene: scene, planKey: "EXACT_LAYOUT_ONLY_PAINT_PLAN")
        defer { full.destroy(); flat.destroy(); w1.isHidden = true; w2.isHidden = true }
        for changed in [false, true] {
            for session in [full, flat] {
                let p = session.presenter
                if changed {
                    let change = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "fade" })
                    session.apply(session.runtime.press(change.id, now: session.now()))
                }
                let first = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "first" })
                let last = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "last" })
                let row = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "overlap" })
                let point = row.convert(CGPoint(x: 60, y: 20), to: nil)
                XCTAssertTrue(row.hitTest(row.convert(point, from: nil), with: nil) === last)
                if changed {
                    XCTAssertGreaterThan(first.layer.zPosition, 0)
                    XCTAssertGreaterThan(last.nodeSuperview?.layer.zPosition ?? 0, 0)
                    XCTAssertTrue(first.nodeSuperview === row || first.nodeSuperview?.nodeSuperview === row)
                }
                // A capture of the row must be blue in the overlap in either projection.
                let format = UIGraphicsImageRendererFormat()
                format.preferredRange = .standard
                let image = UIGraphicsImageRenderer(bounds: row.bounds, format: format).image { row.layer.render(in: $0.cgContext) }
                let cg = try XCTUnwrap(image.cgImage), data = try XCTUnwrap(cg.dataProvider?.data)
                let bytes = try XCTUnwrap(CFDataGetBytePtr(data)), x = Int(60 * image.scale), y = Int(20 * image.scale)
                let offset = y * cg.bytesPerRow + x * 4
                let pixel = Array(UnsafeBufferPointer(start: bytes + offset, count: 4))
                XCTAssertEqual(pixel, [255, 0, 0, 255], "the BGRA overlap is blue")
                for (id, b) in flat.presenter.views {
                    guard let a = full.presenter.views[id] else { continue }
                    let af = a.convert(a.bounds, to: w1), bf = b.convert(b.bounds, to: w2)
                    // Compare after both sessions have applied the same state below.
                    if session === flat { XCTAssertEqual(af, bf, "view \(id)") }
                }
            }
        }
    }

    func testApplyingTheShopGridCreatesFewerViews() throws {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_LAYOUT_ONLY_ROW_PLAN"])
        let plan = try Data(contentsOf: URL(fileURLWithPath: path))
        func run(_ flat: Bool) throws -> (count: Int, median: Double) {
            let session = ExactApp.shared.makeSession(label: "grid-apply-cost")
            session.runtime.useFlatNodes(flat)
            let p = session.presenter, window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 600))
            p.viewport.frame = window.bounds
            window.addSubview(p.viewport)
            window.makeKeyAndVisible()
            defer { session.destroy(); window.isHidden = true }
            let batch = session.boot(plan: plan, size: window.bounds.size)
            XCTAssertNil(batch.error)
            let count = p.views.count
            var samples: [Double] = []
            for i in 0..<120 {
                p.reset()
                let start = CACurrentMediaTime()
                p.apply(batch)
                let elapsed = CACurrentMediaTime() - start
                if i >= 20 { samples.append(elapsed * 1000) }
            }
            return (count, samples.sorted()[samples.count / 2])
        }
        let full = try run(false), flat = try run(true)
        XCTAssertLessThan(flat.count, full.count)
        print("D2 grid apply: views \(full.count) -> \(flat.count), median ms \(full.median) -> \(flat.median), 100 samples after 20 warmups")
    }

    func testMergedLeavesKeepTheWrapperBelowALaterSibling() throws {
        let (p, window) = try fixture(true)
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 6, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "create", "id": 7, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "frame", "id": 6, "x": 0, "y": 0, "w": 20, "h": 20],
            ["op": "frame", "id": 7, "x": 20, "y": 0, "w": 20, "h": 20],
            ["op": "frame", "id": 5, "x": 0, "y": 0, "w": 50, "h": 50],
            ["op": "children", "id": 1, "ids": [6, 7, 5]],
            ["op": "layoutHits", "id": 1, "boxes": [[6, [0, 0, 100, 100], 2]]],
        ]))
        let parent = try XCTUnwrap(p.views[1]), later = try XCTUnwrap(p.views[5])
        XCTAssertEqual(p.flats.inspectionLayers(under: 1).map(\.leaves), [[6, 7]])
        XCTAssertNil(p.flats.leaves[6]?.layer.superlayer)
        XCTAssertTrue(parent.hitTest(CGPoint(x: 10, y: 10), with: nil) === later)
        XCTAssertTrue(parent.hitTest(CGPoint(x: 75, y: 25), with: nil) === parent)
        var heard: [UInt32] = []
        p.onPress = { heard.append($0) }
        later.activate(at: later.convert(CGPoint(x: 10, y: 10), to: window))
        XCTAssertEqual(heard, [5])
        p.apply(wireBatch([["op": "rank", "id": 5, "rank": -2]]))
        XCTAssertTrue(parent.hitTest(CGPoint(x: 10, y: 10), with: nil) === parent)
        p.apply(wireBatch([["op": "rank", "id": 5, "rank": 2]]))
        XCTAssertTrue(parent.hitTest(CGPoint(x: 10, y: 10), with: nil) === later)
    }

    func testAViewlessOverflowBoxIsReachableWithoutSubviewsOrLayers() throws {
        let (p, window) = try fixture(true)
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([
            ["op": "children", "id": 1, "ids": []],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 50, "h": 50],
            ["op": "layoutHits", "id": 1, "boxes": [[NSNull(), [0, 0, 100, 100], 2]]],
        ]))
        let parent = try XCTUnwrap(p.views[1])
        XCTAssertTrue(parent.subviews.isEmpty)
        XCTAssertTrue(parent.hitTest(CGPoint(x: 75, y: 25), with: nil) === parent)
        XCTAssertNil(parent.hitTest(CGPoint(x: 125, y: 25), with: nil))
    }
}
#endif
