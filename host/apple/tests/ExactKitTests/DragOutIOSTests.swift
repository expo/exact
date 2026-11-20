#if os(iOS)
import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import ExactCore
@testable import ExactDragOut
@testable import ExactKit

/// LLP 1094.000 D4 on UIKit: a node carrying `draggable=true` is a
/// `UIDragInteraction`, enabled on every device, whose items and lift are
/// built from the node's admitted props. A row view the node pool lends keeps
/// its interaction only for a draggable node. UIKit, so a simulator runs it:
/// bun host/apple/build.mjs --test --ios --only DragOutIOSTests
final class DragOutIOSTests: XCTestCase {
    private var window: UIWindow!

    private func presenter(_ ops: [[String: Any]]) -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch(ops))
        return p
    }
    /// A card (`id`), draggable with `props`, holding a text.
    private func card(_ id: Int, _ props: [String: Any]) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": "view", "props": props],
         ["op": "create", "id": id + 1, "kind": "text", "props": ["text": "Card"]],
         ["op": "children", "id": id, "ids": [id + 1]],
         ["op": "frame", "id": id, "x": 20.0, "y": 20.0, "w": 200.0, "h": 120.0],
         ["op": "frame", "id": id + 1, "x": 8.0, "y": 8.0, "w": 100.0, "h": 20.0]]
    }
    private func page(_ props: [String: Any]) -> Presenter {
        presenter(card(10, props) + [["op": "create", "id": 1, "kind": "view"], ["op": "children", "id": 1, "ids": [10]],
                                    ["op": "roots", "ids": [1]], ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0]])
    }
    private func drags(_ v: UIView) -> [UIDragInteraction] { v.interactions.compactMap { $0 as? UIDragInteraction } }
    private func sources(_ p: Presenter) throws -> DragSources { try XCTUnwrap(p.dragOut as? DragSources) }

    func testADraggableNodeHasOneEnabledInteractionWhileItDrags() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        let view = try XCTUnwrap(p.views[10])
        XCTAssertEqual(drags(view).count, 1)
        XCTAssertEqual(drags(view).first?.isEnabled, true, "on every device, whatever enabledByDefault says")
        XCTAssertTrue(try drags(view).first?.delegate === sources(p))
        XCTAssertTrue(drags(try XCTUnwrap(p.views[11])).isEmpty, "its text drags with it, not on its own")
        if #available(iOS 27.0, *) { XCTAssertEqual(drags(view).first?.allowsPointerDragBeforeLiftDelay, true) }
        let added = try (view.gestureRecognizers ?? []).filter { try sources(p).owns($0, on: view) }.map(ObjectIdentifier.init)
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["draggable": "false"]]]))
        XCTAssertTrue(drags(view).isEmpty, "draggable=false takes it off")
        XCTAssertTrue(view.interactions.isEmpty, "with the interactions UIKit added beside it")
        XCTAssertTrue((view.gestureRecognizers ?? []).allSatisfy { !added.contains(ObjectIdentifier($0)) }, "and its recognizers")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["draggable": "true"]]]))
        XCTAssertEqual(drags(view).count, 1, "and true puts one back")
    }

    func testBesideASwipeAPointerWaitsForTheLift() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        guard #available(iOS 27.0, *) else { throw XCTSkip("iOS 27's allowsPointerDragBeforeLiftDelay") }
        let p = presenter([["op": "create", "id": 10, "kind": "view", "handlers": ["swipeleft"], "props": ["draggable": "true", "dataTransferText": "a"]],
                           ["op": "roots", "ids": [10]], ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 200.0, "h": 100.0]])
        XCTAssertEqual(drags(try XCTUnwrap(p.views[10])).first?.allowsPointerDragBeforeLiftDelay, false)
    }

    func testTheItemsAreTheFilesThenTheURLAndTextOnTheFirst() throws {
        let p = page(["draggable": "true", "dataTransferFiles": "assets/apple/1016-thumb.jpg\napp:/data/notes.pdf",
                      "dataTransferUrl": "https://example.com/1016", "dataTransferText": "Mount Fuji"])
        let items = DragItems.items(of: try XCTUnwrap(p.views[10]), in: nil)
        XCTAssertEqual(items.count, 2, "one item a file")
        XCTAssertEqual(items.map { $0.itemProvider.suggestedName }, ["1016-thumb.jpg", "notes.pdf"])
        let first = items[0].itemProvider.registeredTypeIdentifiers
        XCTAssertEqual(first.first, UTType.jpeg.identifier, "the file first, its type its extension's")
        XCTAssertTrue(first.contains(UTType.url.identifier) && first.contains(UTType.utf8PlainText.identifier), "\(first)")
        XCTAssertLessThan(try XCTUnwrap(first.firstIndex(of: UTType.url.identifier)), try XCTUnwrap(first.firstIndex(of: UTType.utf8PlainText.identifier)))
        XCTAssertEqual(items[1].itemProvider.registeredTypeIdentifiers, [UTType.pdf.identifier])
        XCTAssertEqual((items[0].localObject as? DragItems.Lifted)?.id, 10)
    }

    func testWithNoFileOneItemCarriesTheURLAndText() throws {
        let p = page(["draggable": "true", "dataTransferUrl": "https://example.com/1016", "dataTransferText": ""])
        let items = DragItems.items(of: try XCTUnwrap(p.views[10]), in: nil)
        XCTAssertEqual(items.count, 1)
        XCTAssertTrue(items[0].itemProvider.hasItemConformingToTypeIdentifier(UTType.url.identifier))
        XCTAssertFalse(items[0].itemProvider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier), "an empty text is none, as tree reads it")
    }

    func testTheLiftIsTheNodeOverItsController() throws {
        let host = UIViewController()
        let p = page(["draggable": "true", "dataTransferText": "a"])
        host.view.frame = window.bounds
        window.rootViewController = host
        host.view.addSubview(p.viewport)
        let view = try XCTUnwrap(p.views[10])
        let preview = try XCTUnwrap(DragSources.preview(of: view))
        XCTAssertTrue(preview.view === view, "the node itself")
        XCTAssertTrue(preview.target.container === host.view, "over the controller's view, so a clipping strip does not cut it")
        XCTAssertEqual(preview.parameters.backgroundColor, .clear, "no fill of its own")
        view.layer.backgroundColor = UIColor.systemTeal.cgColor
        XCTAssertEqual(DragSources.preview(of: view)?.parameters.backgroundColor?.cgColor, UIColor.systemTeal.cgColor, "on its own fill, as the menu's preview")
        view.isHidden = true
        XCTAssertNil(DragSources.preview(of: view), "a hidden node lifts nothing")
    }

    func testAControlOfANodeInsideKeepsTheContact() throws {
        let p = presenter([["op": "create", "id": 10, "kind": "view", "props": ["draggable": "true", "dataTransferText": "a"]],
                           ["op": "create", "id": 12, "kind": "view", "props": ["testId": "inner"]], ["op": "children", "id": 10, "ids": [12]], ["op": "roots", "ids": [10]],
                           ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 200.0, "h": 120.0], ["op": "frame", "id": 12, "x": 8.0, "y": 8.0, "w": 100.0, "h": 40.0]])
        let card = try XCTUnwrap(p.views[10])
        XCTAssertFalse(DragSources.startsInControl(card, at: CGPoint(x: 150, y: 80), in: p), "the card's own face lifts")
        XCTAssertFalse(DragSources.startsInControl(card, at: CGPoint(x: 20, y: 20), in: p), "and a box inside it")
        let inner = try XCTUnwrap(p.views[12])
        let field = UITextField(frame: inner.bounds)
        inner.addSubview(field)
        XCTAssertTrue(DragSources.startsInControl(card, at: CGPoint(x: 20, y: 20), in: p), "a text input inside keeps its own drag")
        field.removeFromSuperview()
        let slider = UISlider(frame: inner.bounds)
        inner.addSubview(slider)
        XCTAssertTrue(DragSources.startsInControl(card, at: CGPoint(x: 20, y: 20), in: p), "and a control that tracks a drag")
        slider.removeFromSuperview()
        let button = UIButton(frame: inner.bounds)
        inner.addSubview(button)
        XCTAssertFalse(DragSources.startsInControl(card, at: CGPoint(x: 20, y: 20), in: p), "a child button is a press: a rested finger lifts the card (D8)")
        button.removeFromSuperview()
        let checkbox = UIControl(frame: inner.bounds)
        inner.addSubview(checkbox)
        XCTAssertFalse(DragSources.startsInControl(card, at: CGPoint(x: 20, y: 20), in: p), "and so is any other control, a checkbox")
        checkbox.removeFromSuperview()
        let face = UIButton(frame: card.bounds)
        card.insertSubview(face, at: 0)
        XCTAssertFalse(DragSources.startsInControl(card, at: CGPoint(x: 150, y: 80), in: p), "the node's own control is the node")
    }

    /// A delivered update's resolver removes its files when released: the
    /// load handler holds it, so its file is there at the drop however long
    /// after the lift, and goes with the handler.
    func testADeliveredFileIsThereAtTheDropAndGoesWithTheHandler() throws {
        let bytes = Data("jpeg".utf8)
        var resolver: AssetResolver? = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: ["assets/a.jpg": bytes])
        var load: DragItems.Load? = DragItems.shipped("assets/a.jpg", resolver, nil)
        weak let held = resolver
        resolver = nil
        XCTAssertNotNil(held, "the handler holds the resolver")
        var file: URL?
        _ = load?({ url, coordinated, error in file = url; XCTAssertFalse(coordinated); XCTAssertNil(error) })
        let url = try XCTUnwrap(file)
        XCTAssertEqual(try Data(contentsOf: url), bytes)
        load = nil
        XCTAssertNil(held)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path), "its directory goes with the resolver")
    }

    func testAFileTheAppDoesNotShipCompletesWithTheError() throws {
        let resolver = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: [:])
        let done = expectation(description: "the load completes")
        _ = DragItems.shipped("assets/gone.jpg", resolver, nil)({ url, _, error in
            XCTAssertNil(url)
            XCTAssertEqual((error as NSError?)?.localizedDescription, "drag: assets/gone.jpg could not be read: the app does not ship it")
            done.fulfill()
        })
        wait(for: [done], timeout: 5)
    }

    /// An `app:/` file the confined copy refuses (this session has no
    /// grants) completes with the error and journals it, and the directory
    /// made for the copy goes with the handler.
    func testAnAppFileTheCopyRefusesCompletesWithTheErrorAndLeavesNothing() throws {
        let session = ExactApp.shared.makeSession(label: "drag-out-app-file")
        defer { session.destroy() }
        let before = Set((try? FileManager.default.contentsOfDirectory(atPath: NSTemporaryDirectory())) ?? []).filter { $0.hasPrefix("exact-drag-") }
        var load: DragItems.Load? = DragItems.appFile("app:/data/notes.pdf", session)
        let done = expectation(description: "the load completes")
        var failure: Error?
        _ = load?({ url, _, error in XCTAssertNil(url); failure = error; done.fulfill() })
        wait(for: [done], timeout: 5)
        XCTAssertTrue((failure as NSError?)?.localizedDescription.hasPrefix("drag: app:/data/notes.pdf could not be read: ") == true, "\(String(describing: failure))")
        load = nil
        let after = Set((try? FileManager.default.contentsOfDirectory(atPath: NSTemporaryDirectory())) ?? []).filter { $0.hasPrefix("exact-drag-") }
        XCTAssertEqual(after, before, "the copy's directory went with the handler")
    }

    // MARK: The node pool

    private func collections(_ rows: [Int]) -> [String: Any] {
        ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 50, "totalExtent": 2000,
            "rows": rows.map { ["view": $0, "root": $0, "epoch": 1] }, "correction": NSNull()]]]
    }
    private func row(_ base: Int, draggable: Bool) -> [[String: Any]] {
        card(base, draggable ? ["testId": "row-\(base)", "draggable": "true", "dataTransferText": "Row \(base)"] : ["testId": "row-\(base)"])
    }
    private func list() -> Presenter {
        presenter([collections([10]), ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]], ["op": "create", "id": 9, "kind": "view"]]
            + row(10, draggable: true)
            + [["op": "children", "id": 1, "ids": [10]], ["op": "children", "id": 9, "ids": [1]], ["op": "roots", "ids": [9]],
               ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0], ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 600.0],
               ["op": "content", "id": 1, "w": 300.0, "h": 2000.0]])
    }
    private func next(_ p: Presenter, _ from: Int, _ to: Int, draggable: Bool) {
        p.apply(wireBatch([collections([to]), ["op": "destroy", "id": from], ["op": "destroy", "id": from + 1]] + row(to, draggable: draggable)
            + [["op": "children", "id": 1, "ids": [to]]]))
    }

    func testARowViewLentToADraggableRowKeepsItsInteraction() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = list()
        let view = try XCTUnwrap(p.views[10])
        let kept = try XCTUnwrap(drags(view).first)
        next(p, 10, 20, draggable: true)
        XCTAssertTrue(p.views[20] === view, "the same view, lent to the next row")
        XCTAssertEqual(p.pool.takes, 1, "the interaction does not keep the row out of the pool")
        XCTAssertEqual(drags(view).count, 1)
        XCTAssertTrue(drags(view).first === kept, "the same interaction, now the new node's")
        XCTAssertEqual((DragItems.items(of: view, in: nil).first?.localObject as? DragItems.Lifted)?.id, 20)
        next(p, 20, 30, draggable: false)
        XCTAssertTrue(p.views[30] === view)
        XCTAssertTrue(drags(view).isEmpty, "lent to a row that does not drag, it comes off")
        XCTAssertTrue(view.interactions.isEmpty, "with what UIKit added beside it")
    }

    func testACancelledDragReturnsOnlyToItsOwnNode() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = list()
        let view = try XCTUnwrap(p.views[10])
        let interaction = try XCTUnwrap(drags(view).first)
        let item = try XCTUnwrap(DragItems.items(of: view, in: nil).first)
        let fallback = UITargetedDragPreview(view: view)
        XCTAssertNotNil(try sources(p).dragInteraction(interaction, previewForCancelling: item, withDefault: fallback))
        next(p, 10, 20, draggable: true)
        XCTAssertNil(try sources(p).dragInteraction(interaction, previewForCancelling: item, withDefault: fallback), "the view is another row's now: the item fades")
    }
}
#endif
