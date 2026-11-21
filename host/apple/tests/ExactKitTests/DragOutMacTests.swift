#if os(macOS)
import AppKit
import UniformTypeIdentifiers
import XCTest
@testable import ExactCore
@testable import ExactDragOut
@testable import ExactKit

/// LLP 1094.000 D5 on AppKit: a mouse drag on a node carrying
/// `draggable=true` is a dragging session whose items are file promises, the
/// first also offering `url` and `text`, with the node as drawn for an
/// image; the drag is the mouse chain's last candidate, and a child that
/// hears `press` keeps its contact (D8).
/// bun host/apple/build.mjs --test --only DragOutMacTests
final class DragOutMacTests: XCTestCase {
    private var window: NSWindow!

    /// A card (10) with `props`, holding a box (12), and a button (14) that
    /// hears `press`.
    private func presenter(_ props: [String: Any], handlers: [String] = []) -> Presenter {
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = p.viewport
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 10, "kind": "view", "handlers": handlers, "props": props],
            ["op": "create", "id": 12, "kind": "view", "props": ["testId": "inner"]],
            ["op": "create", "id": 14, "kind": "view", "handlers": ["press"], "props": ["testId": "pin"]],
            ["op": "children", "id": 10, "ids": [12, 14]], ["op": "children", "id": 1, "ids": [10]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 10, "x": 20.0, "y": 20.0, "w": 200.0, "h": 120.0],
            ["op": "frame", "id": 12, "x": 8.0, "y": 8.0, "w": 100.0, "h": 40.0],
            ["op": "frame", "id": 14, "x": 8.0, "y": 60.0, "w": 60.0, "h": 40.0]]))
        return p
    }

    func testADownInsideADraggableNodeArmsThatNodeUnlessAChildHearsPress() throws {
        let p = presenter(["draggable": "true", "dataTransferText": "Mount Fuji"])
        let card = try XCTUnwrap(p.views[10])
        XCTAssertTrue(MouseDragOut.source(from: card, in: p) === card, "the card's own face")
        XCTAssertTrue(MouseDragOut.source(from: try XCTUnwrap(p.views[12]), in: p) === card, "a box inside it")
        XCTAssertNil(MouseDragOut.source(from: try XCTUnwrap(p.views[14]), in: p), "a child that hears press keeps its contact")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["disabled": "true"]]]))
        XCTAssertNil(MouseDragOut.source(from: card, in: p), "a disabled node does not drag")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["disabled": "false", "draggable": "false"]]]))
        XCTAssertNil(MouseDragOut.source(from: card, in: p), "draggable=false")
        let link = presenter(["draggable": "true", "dataTransferText": "a"], handlers: ["press"])
        XCTAssertTrue(MouseDragOut.source(from: try XCTUnwrap(link.views[12]), in: link) === link.views[10], "its own press is the node's: a drag cancels it (D8)")
    }

    func testTheDragIsTheChainsLastCandidate() throws {
        let p = presenter(["draggable": "true", "dataTransferText": "a"])
        XCTAssertTrue(p.mouseChain.ranked.last === p.mouseDragOut, "after reorder, transform, height, pan and swipe (D8)")
    }

    func testAMoveWithinTheSlopTakesNothing() throws {
        let p = presenter(["draggable": "true", "dataTransferText": "a"])
        let drag = try XCTUnwrap(p.mouseDragOut as? MouseDragOut)
        let card = try XCTUnwrap(p.views[10])
        let down = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: NSPoint(x: 100, y: 300), modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        drag.arm(card, event: down)
        XCTAssertEqual(drag.armed, ExactEnv.agentMode ? nil : card)
        let near = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDragged, location: NSPoint(x: 101, y: 301), modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        XCTAssertFalse(drag.drag(near), "inside the slop the press may still click")
        // The slop is an axis's, as the swipe measures it, so a sideways
        // drag reaches a swipe on the node first (D8).
        for (dx, dy) in [(4.0, 1.0), (3.0, 3.0), (-4.0, 4.0)] {
            let moved = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDragged, location: NSPoint(x: 100 + dx, y: 300 + dy), modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
            XCTAssertFalse(drag.drag(moved), "(\(dx), \(dy))")
        }
        XCTAssertFalse(drag.engaged)
    }

    /// AppKit's session takes the button's release: the end posts one, which
    /// releases the contact as any up does.
    func testTheEndOfASessionReleasesTheContact() throws {
        if ExactEnv.agentMode { throw XCTSkip("no drag arms under the agent") }
        let p = presenter(["draggable": "true", "dataTransferText": "a"], handlers: ["pointerdown", "pointerup"])
        window.makeKeyAndOrderFront(nil)
        let card = try XCTUnwrap(p.views[10])
        let drag = try XCTUnwrap(p.mouseDragOut as? MouseDragOut)
        let at = card.convert(NSPoint(x: 150, y: 20), to: nil)
        let down = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: at, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        card.mouseDown(with: down)
        XCTAssertNotNil(p.pointerHeld, "the button holds the pointer")
        XCTAssertTrue(drag.armed === card)
        drag.began(in: window) // as a session past the slop does
        drag.ended(at: window.convertPoint(toScreen: at))
        let up = try XCTUnwrap(NSApp.nextEvent(matching: .leftMouseUp, until: Date().addingTimeInterval(2), inMode: .default, dequeue: true))
        XCTAssertEqual(up.windowNumber, window.windowNumber)
        card.mouseUp(with: up) // the window sends an up to the view its down went to
        XCTAssertNil(p.pointerHeld, "pointerup went, and the hold with it")
        XCTAssertFalse(drag.engaged)
        XCTAssertNil(drag.armed)
        // A node a batch removed mid-drag still has its release posted.
        drag.arm(card, event: down)
        drag.began(in: window)
        p.apply(wireBatch([["op": "destroy", "id": 12], ["op": "destroy", "id": 14], ["op": "destroy", "id": 10]]))
        drag.ended(at: window.convertPoint(toScreen: at))
        XCTAssertNotNil(NSApp.nextEvent(matching: .leftMouseUp, until: Date().addingTimeInterval(2), inMode: .default, dequeue: true), "to the session's window")
    }

    func testTheItemsArePromisesThenTheURLAndTextOnTheFirst() throws {
        let p = presenter(["draggable": "true", "dataTransferFiles": "assets/apple/1016-thumb.jpg\napp:/data/notes.pdf",
                           "dataTransferUrl": "https://example.com/1016", "dataTransferText": "Mount Fuji"])
        let items = DragItemsMac.items(of: try XCTUnwrap(p.views[10]), in: nil)
        XCTAssertEqual(items.count, 2, "one item a file")
        let first = try XCTUnwrap(items[0].item as? DragItemsMac.Promise)
        XCTAssertEqual(first.fileType, UTType.jpeg.identifier)
        XCTAssertEqual(first.delegate?.filePromiseProvider(first, fileNameForType: first.fileType), "1016-thumb.jpg")
        let types = first.writableTypes(for: .general)
        XCTAssertEqual(Array(types.suffix(2)), [.URL, .string], "url, then text, after the promise's own")
        XCTAssertEqual(first.pasteboardPropertyList(forType: .URL) as? String, "https://example.com/1016")
        XCTAssertEqual(first.pasteboardPropertyList(forType: .string) as? String, "Mount Fuji")
        let second = try XCTUnwrap(items[1].item as? DragItemsMac.Promise)
        XCTAssertEqual(second.fileType, UTType.pdf.identifier)
        XCTAssertFalse(second.writableTypes(for: .general).contains(.URL))
        let pasteboard = NSPasteboard(name: NSPasteboard.Name("exact-drag-\(UUID().uuidString)"))
        defer { pasteboard.releaseGlobally() }
        pasteboard.clearContents()
        XCTAssertTrue(pasteboard.writeObjects([first]), "the promise writes to a pasteboard")
        XCTAssertEqual(pasteboard.string(forType: .URL), "https://example.com/1016", "with its url")
        XCTAssertEqual(pasteboard.string(forType: .string), "Mount Fuji", "and its text")
        let promised = NSPasteboard.PasteboardType("com.apple.pasteboard.promised-file-content-type")
        XCTAssertEqual(pasteboard.string(forType: promised), UTType.jpeg.identifier, "and still promises the file: \(pasteboard.types ?? [])")
        XCTAssertEqual(items[0].draggingFrame.size, CGSize(width: 200, height: 120), "the node's own frame")
        XCTAssertNotNil(items[0].imageComponents?.first, "and its picture")
    }

    func testWithNoFileOnePasteboardItemCarriesTheURLAndText() throws {
        let p = presenter(["draggable": "true", "dataTransferUrl": "https://example.com/1016", "dataTransferText": ""])
        let items = DragItemsMac.items(of: try XCTUnwrap(p.views[10]), in: nil)
        XCTAssertEqual(items.count, 1)
        let item = try XCTUnwrap(items[0].item as? NSPasteboardItem)
        XCTAssertEqual(item.string(forType: .URL), "https://example.com/1016")
        XCTAssertNil(item.string(forType: .string), "an empty text is none, as tree reads it")
    }

    func testACopyOutsideTheAppAndNothingInside() throws {
        XCTAssertEqual(MouseDragOut.operations(.outsideApplication), .copy)
        XCTAssertEqual(MouseDragOut.operations(.withinApplication), [])
    }

    /// The promise writes the shipped file where the destination asks, from
    /// the resolver found when the drag began, which the provider holds.
    func testAPromiseWritesTheShippedFileFromItsResolver() throws {
        let bytes = Data("jpeg".utf8)
        var resolver: AssetResolver? = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: ["assets/a.jpg": bytes])
        let writer = DragItemsMac.PromiseWriter(entry: "assets/a.jpg", name: "a.jpg", resolver: resolver, session: nil)
        weak let held = resolver
        resolver = nil
        XCTAssertNotNil(held, "the writer holds the resolver")
        let destination = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString).jpg")
        defer { try? FileManager.default.removeItem(at: destination) }
        var failure: Error?
        writer.filePromiseProvider(NSFilePromiseProvider(), writePromiseTo: destination) { failure = $0 }
        XCTAssertNil(failure)
        XCTAssertEqual(try Data(contentsOf: destination), bytes)
        XCTAssertEqual((try FileManager.default.attributesOfItem(atPath: destination.path)[.posixPermissions] as? NSNumber)?.intValue, 0o644, "the person's copy is writable")
        let missing = DragItemsMac.PromiseWriter(entry: "assets/gone.jpg", name: "gone.jpg", resolver: held, session: nil)
        missing.filePromiseProvider(NSFilePromiseProvider(), writePromiseTo: destination.appendingPathExtension("2")) { failure = $0 }
        XCTAssertEqual((failure as NSError?)?.localizedDescription, "drag: assets/gone.jpg could not be read: the app does not ship it")
    }
}
#endif
