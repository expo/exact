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
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["draggable": "true"]]]))
        XCTAssertTrue(MouseDragOut.source(from: try XCTUnwrap(p.views[12]), in: p) === card, "a draggable box inside that carries nothing yields to the card")
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["dataTransferText": "Box"]]]))
        XCTAssertTrue(MouseDragOut.source(from: try XCTUnwrap(p.views[12]), in: p) === p.views[12], "one that carries something drags itself")
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

    /// What a batch changed between the down and the move past the slop is
    /// read as the drag would begin: a node disabled on the way up, or one no
    /// longer draggable, begins none; with no change, the session begins.
    func testABatchSinceTheDownIsReadAsTheDragBegins() throws {
        if ExactEnv.agentMode { throw XCTSkip("no drag arms under the agent") }
        let p = presenter(["draggable": "true", "dataTransferText": "a"])
        let drag = try XCTUnwrap(p.mouseDragOut as? MouseDragOut)
        var begun: [NodeView] = []
        drag.begin = { node, _, _, _ in begun.append(node) }
        let card = try XCTUnwrap(p.views[10]), inner = try XCTUnwrap(p.views[12])
        let event = { (type: NSEvent.EventType, x: Double) in
            try XCTUnwrap(NSEvent.mouseEvent(with: type, location: NSPoint(x: x, y: 300), modifierFlags: [], timestamp: 0, windowNumber: self.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        }
        // What the card carries changed, as a list row renewed for another item keeps its view and id
        for (id, set, unset) in [(10, ["disabled": "true"], ["disabled": "false"]), (12, ["disabled": "true"], ["disabled": "false"]),
                                 (10, ["draggable": "false"], ["draggable": "true"]), (10, ["dataTransferText": "b"], ["dataTransferText": "a"])] {
            drag.arm(inner, event: try event(.leftMouseDown, 100))
            XCTAssertTrue(drag.armed === card)
            p.apply(wireBatch([["op": "props", "id": id, "set": set]]))
            XCTAssertFalse(drag.drag(try event(.leftMouseDragged, 140)), "\(id) \(set)")
            XCTAssertNil(drag.armed)
            XCTAssertFalse(drag.engaged)
            p.apply(wireBatch([["op": "props", "id": id, "set": unset]]))
        }
        drag.arm(inner, event: try event(.leftMouseDown, 100))
        p.apply(wireBatch([["op": "style", "id": 10, "style": ["visibility": "hidden"]]]))
        XCTAssertFalse(drag.drag(try event(.leftMouseDragged, 140)), "hidden by its style")
        p.apply(wireBatch([["op": "style", "id": 10, "style": ["visibility": "visible"]]]))
        drag.arm(inner, event: try event(.leftMouseDown, 100))
        card.isHidden = true
        XCTAssertFalse(drag.drag(try event(.leftMouseDragged, 140)), "hidden")
        card.isHidden = false
        // The row renewed for another item, its view, id and payload kept
        drag.arm(inner, event: try event(.leftMouseDown, 100))
        p.renew(10)
        XCTAssertFalse(drag.drag(try event(.leftMouseDragged, 140)), "renewed")
        XCTAssertTrue(begun.isEmpty)
        drag.arm(inner, event: try event(.leftMouseDown, 100))
        XCTAssertTrue(drag.drag(try event(.leftMouseDragged, 140)))
        XCTAssertEqual(begun.map(ObjectIdentifier.init), [ObjectIdentifier(card)], "from the card")
        drag.cancel()
        // A move from another window, the page still in the down's
        let other = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        drag.arm(inner, event: try event(.leftMouseDown, 100))
        let foreign = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDragged, location: NSPoint(x: 140, y: 300), modifierFlags: [], timestamp: 0,
                                                        windowNumber: other.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        XCTAssertFalse(drag.drag(foreign), "a move from another window")
        drag.cancel()
        let foreignDown = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: NSPoint(x: 100, y: 300), modifierFlags: [], timestamp: 0,
                                                            windowNumber: other.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        drag.arm(inner, event: foreignDown)
        XCTAssertNil(drag.armed, "a down from another window arms nothing")
        XCTAssertFalse(drag.drag(try event(.leftMouseDragged, 140)))
        // The page moved to another window since the down
        drag.arm(inner, event: try event(.leftMouseDown, 100))
        other.contentView = p.viewport
        XCTAssertFalse(drag.drag(try event(.leftMouseDragged, 140)), "another window")
        XCTAssertEqual(begun.count, 1)
        drag.cancel()
    }

    /// An exit keeps the node the down landed on mounted inside the card, out
    /// of the presenter's index: its contact ended, so the move begins no drag.
    func testADownOnANodeThatExitsBeginsNoDrag() throws {
        if ExactEnv.agentMode { throw XCTSkip("no drag arms under the agent") }
        let p = presenter(["draggable": "true", "dataTransferText": "a"])
        let drag = try XCTUnwrap(p.mouseDragOut as? MouseDragOut)
        var begun: [NodeView] = []
        drag.begin = { node, _, _, _ in begun.append(node) }
        let card = try XCTUnwrap(p.views[10]), inner = try XCTUnwrap(p.views[12])
        let event = { (type: NSEvent.EventType, x: Double) in
            try XCTUnwrap(NSEvent.mouseEvent(with: type, location: NSPoint(x: x, y: 300), modifierFlags: [], timestamp: 0, windowNumber: self.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        }
        drag.arm(inner, event: try event(.leftMouseDown, 100))
        XCTAssertTrue(drag.armed === card)
        p.apply(wireBatch([["op": "exit", "id": 12]]))
        XCTAssertTrue(inner.isDescendant(of: card), "still mounted in the card")
        XCTAssertNil(p.views[12])
        XCTAssertFalse(drag.drag(try event(.leftMouseDragged, 140)))
        XCTAssertNil(drag.armed)
        XCTAssertTrue(begun.isEmpty)
    }

    /// The image's drawing is deferred to each item's provider, which AppKit
    /// calls if it needs the image: making the items draws nothing, and the
    /// provider draws the card when called.
    func testTheDragImageIsDrawnOnlyWhenAppKitAsks() throws {
        if ExactEnv.agentMode { throw XCTSkip("no drag arms under the agent") }
        let p = presenter(["draggable": "true", "dataTransferText": "a"])
        let drag = try XCTUnwrap(p.mouseDragOut as? MouseDragOut)
        var begun: [NSDraggingItem] = []
        drag.begin = { _, items, _, _ in begun = items }
        let card = try XCTUnwrap(p.views[10])
        let drawn = Drawn(frame: NSRect(x: 150, y: 80, width: 10, height: 10))
        card.addSubview(drawn)
        let event = { (type: NSEvent.EventType, x: Double) in
            try XCTUnwrap(NSEvent.mouseEvent(with: type, location: NSPoint(x: x, y: 300), modifierFlags: [], timestamp: 0, windowNumber: self.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        }
        drag.arm(try XCTUnwrap(p.views[12]), event: try event(.leftMouseDown, 100))
        XCTAssertTrue(drag.drag(try event(.leftMouseDragged, 140)))
        XCTAssertEqual(begun.count, 1)
        XCTAssertFalse(drawn.drew, "nothing drawn as the items are made")
        let components = try XCTUnwrap(begun.first?.imageComponentsProvider?())
        XCTAssertTrue(drawn.drew, "drawn by the provider")
        XCTAssertEqual(components.map(\.frame.size), [card.bounds.size])
    }

    private final class Drawn: NSView {
        private(set) var drew = false
        override func draw(_ dirtyRect: NSRect) { drew = true }
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

    /// `writer`'s promise written to `url` on the queue AppKit runs it on
    /// (`operationQueue(for:)`), waited for: the error it completed with.
    /// `completed` runs in its completion.
    private func write(_ writer: DragItemsMac.PromiseWriter, to url: URL, completed: @escaping () -> Void = {}) -> Error? {
        let done = expectation(description: "the promise completes")
        var failure: Error?
        DragFiles.copies.addOperation {
            writer.filePromiseProvider(NSFilePromiseProvider(), writePromiseTo: url) { failure = $0; completed(); done.fulfill() }
        }
        wait(for: [done], timeout: 15)
        return failure
    }

    /// `DragFiles.place` on the copy queue, where it runs: the error it threw.
    private func place(_ fd: Int32, at url: URL) -> Error? {
        let done = expectation(description: "the copy ends")
        var failure: Error?
        DragFiles.copies.addOperation {
            do { try DragFiles.place(fd, at: url) } catch { failure = error }
            done.fulfill()
        }
        wait(for: [done], timeout: 15)
        return failure
    }

    func testACopyOutsideTheAppAndNothingInside() throws {
        XCTAssertEqual(MouseDragOut.operations(.outsideApplication), .copy)
        XCTAssertEqual(MouseDragOut.operations(.withinApplication), [])
    }

    /// The promise writes the shipped file where the destination asks, from
    /// the resolver found when the drag began, which the provider holds: its
    /// data alone, as a new file's mode.
    func testAPromiseWritesTheShippedFileFromItsResolver() throws {
        let bytes = Data("jpeg".utf8)
        var resolver: AssetResolver? = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: ["assets/a.jpg": bytes])
        let writer = DragItemsMac.PromiseWriter(entry: "assets/a.jpg", name: "a.jpg", resolver: resolver, journal: DragFiles.Journal())
        weak let held = resolver
        resolver = nil
        XCTAssertNotNil(held, "the writer holds the resolver")
        let destination = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString).jpg")
        defer { try? FileManager.default.removeItem(at: destination) }
        // An extended attribute on the delivered file, which the copy leaves behind
        let shipped = try XCTUnwrap(held?.url("assets/a.jpg")).path
        chmod(shipped, 0o600)
        XCTAssertEqual(setxattr(shipped, "com.example.drag", "private", 7, 0, 0), 0)
        chmod(shipped, 0o400)
        XCTAssertNil(write(writer, to: destination))
        XCTAssertEqual(try Data(contentsOf: destination), bytes)
        XCTAssertEqual(getxattr(destination.path, "com.example.drag", nil, 0, 0, 0), -1, "the data alone")
        let mode = { (url: URL) in try XCTUnwrap(FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions] as? NSNumber).intValue }
        let reference = destination.deletingPathExtension().appendingPathExtension("reference")
        defer { try? FileManager.default.removeItem(at: reference) }
        close(open(reference.path, O_WRONLY | O_CREAT | O_EXCL, 0o666))
        XCTAssertEqual(try mode(destination), try mode(reference), "a new file's mode, as the umask makes one, not the delivered update's read-only one")
        let missing = DragItemsMac.PromiseWriter(entry: "assets/gone.jpg", name: "gone.jpg", resolver: held, journal: DragFiles.Journal())
        XCTAssertEqual((write(missing, to: destination.appendingPathExtension("2")) as NSError?)?.localizedDescription,
                       "drag: assets/gone.jpg could not be read: the app does not ship it")
    }

    /// Each promise is written on the queue the drag's promises share, as
    /// AppKit runs it there, off the main queue.
    func testAPromiseIsWrittenOnItsOwnQueue() throws {
        let bytes = Data("jpeg".utf8)
        let resolver = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: ["assets/a.jpg": bytes])
        let writer = DragItemsMac.PromiseWriter(entry: "assets/a.jpg", name: "a.jpg", resolver: resolver, journal: DragFiles.Journal())
        let promise = NSFilePromiseProvider(fileType: UTType.jpeg.identifier, delegate: writer)
        let queue = try XCTUnwrap(writer.operationQueue(for: promise))
        XCTAssertTrue(queue === DragFiles.copies)
        let destination = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString).jpg")
        defer { try? FileManager.default.removeItem(at: destination) }
        let done = expectation(description: "the promise is written")
        var ran: (OperationQueue?, Bool, Error?) = (nil, true, nil)
        queue.addOperation {
            writer.filePromiseProvider(promise, writePromiseTo: destination) { ran = (OperationQueue.current, Thread.isMainThread, $0); done.fulfill() }
        }
        wait(for: [done], timeout: 5)
        XCTAssertTrue(ran.0 === queue)
        XCTAssertFalse(ran.1)
        XCTAssertNil(ran.2)
        XCTAssertEqual(try Data(contentsOf: destination), bytes)
        // An `app:/` file's open reads the session on the main thread, which the queue's write waits on
        let session = ExactApp.shared.makeSession(label: "drag-out-queue")
        defer { session.destroy() }
        let app = DragItemsMac.PromiseWriter(entry: "app:/data/notes.txt", name: "notes.txt", resolver: nil, journal: DragFiles.Journal(session: session))
        let refused = expectation(description: "the app file's write completes")
        var failure: Error?
        queue.addOperation {
            app.filePromiseProvider(promise, writePromiseTo: destination.appendingPathExtension("txt")) { failure = $0; refused.fulfill() }
        }
        wait(for: [refused], timeout: 15)
        XCTAssertEqual((failure as NSError?)?.localizedDescription, "drag: app:/data/notes.txt could not be read: not booted")
    }

    /// A write that cannot be put at its destination, a file already there
    /// or a link pointing nowhere, leaves the destination as it was.
    func testAWriteNeverReplacesWhatIsAtItsDestination() throws {
        let resolver = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: ["assets/a.jpg": Data("jpeg".utf8)])
        let writer = DragItemsMac.PromiseWriter(entry: "assets/a.jpg", name: "a.jpg", resolver: resolver, journal: DragFiles.Journal())
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("a.jpg"), link = folder.appendingPathComponent("b.jpg")
        try Data("the person's".utf8).write(to: file)
        try FileManager.default.createSymbolicLink(atPath: link.path, withDestinationPath: folder.appendingPathComponent("nowhere").path)
        for destination in [file, link] { XCTAssertNotNil(write(writer, to: destination), destination.lastPathComponent) }
        XCTAssertEqual(try Data(contentsOf: file), Data("the person's".utf8))
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: link.path), folder.appendingPathComponent("nowhere").path)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: folder.path).sorted(), ["a.jpg", "b.jpg"], "nothing else left")
    }

    /// A copy that fails once its file is made, from a descriptor that
    /// cannot be read, fails the write and leaves the file it made, since a
    /// name in a folder others write to can be another's by the time it is
    /// removed.
    func testACopyThatFailsAfterMakingItsFileLeavesIt() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        try Data("the backup".utf8).write(to: folder.appendingPathComponent("notes.txt"))
        let source = open(folder.appendingPathComponent("notes.txt").path, O_WRONLY)
        defer { close(source) }
        let destination = folder.appendingPathComponent("dropped.txt")
        XCTAssertNotNil(place(source, at: destination))
        XCTAssertTrue(FileManager.default.fileExists(atPath: destination.path))
    }

    /// A copy carries the file's data alone, none of its extended
    /// attributes.
    func testACopyCarriesTheDataAlone() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("notes.txt"), copy = folder.appendingPathComponent("dropped.txt")
        try Data("the backup".utf8).write(to: file)
        XCTAssertEqual(setxattr(file.path, "com.example.drag", "private", 7, 0, 0), 0)
        let source = open(file.path, O_RDONLY)
        defer { close(source) }
        XCTAssertNil(place(source, at: copy))
        XCTAssertEqual(try Data(contentsOf: copy), Data("the backup".utf8))
        XCTAssertEqual(getxattr(copy.path, "com.example.drag", nil, 0, 0, 0), -1)
        XCTAssertEqual(errno, ENOATTR)
    }

    /// An `app:/` file is opened by the session's runtime when the
    /// destination asks for it: one the runtime refuses (this session is not
    /// booted) fails with the runtime's reason, and one asked for after the
    /// session was destroyed, though still held, fails as well; neither
    /// makes a file. Nothing is opened as the drag begins: one opened (here
    /// by a stand-in, of a file made after the drag began) is written whole,
    /// its descriptor closed before the write completes.
    func testAnAppFileIsOpenedWhenTheDestinationAsks() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let p = presenter(["draggable": "true", "dataTransferFiles": "app:/data/notes.txt"])
        var fd: Int32 = -1, opens = 0
        let items = DragItemsMac.items(of: try XCTUnwrap(p.views[10]), in: nil) { _, _ in
            opens += 1
            fd = open(folder.appendingPathComponent("notes.txt").path, O_RDONLY)
            return fd >= 0 ? .success(fd) : .failure(DragFiles.posix())
        }
        XCTAssertEqual(opens, 0, "none as the drag begins")
        let opened = try XCTUnwrap((items.first?.item as? NSFilePromiseProvider)?.userInfo as? DragItemsMac.PromiseWriter)
        try Data("the backup".utf8).write(to: folder.appendingPathComponent("notes.txt"))
        var closed = false
        XCTAssertNil(write(opened, to: folder.appendingPathComponent("dropped.txt")) { closed = fcntl(fd, F_GETFD) == -1 && errno == EBADF })
        XCTAssertEqual(try Data(contentsOf: folder.appendingPathComponent("dropped.txt")), Data("the backup".utf8))
        XCTAssertTrue(closed, "the descriptor closed before the write completed")
        XCTAssertEqual(opens, 1)
        // A descriptor that cannot be read fails the write, closed before the failure is told
        opened.open = { _, _ in
            fd = open(folder.appendingPathComponent("notes.txt").path, O_WRONLY)
            return fd >= 0 ? .success(fd) : .failure(DragFiles.posix())
        }
        var closedOnFailure = false
        XCTAssertNotNil(write(opened, to: folder.appendingPathComponent("failed.txt")) { closedOnFailure = fcntl(fd, F_GETFD) == -1 && errno == EBADF })
        XCTAssertTrue(closedOnFailure)
        XCTAssertTrue(FileManager.default.fileExists(atPath: folder.appendingPathComponent("failed.txt").path), "the partial file stays for the destination")
        let session = ExactApp.shared.makeSession(label: "drag-out-app-file")
        let writer = DragItemsMac.PromiseWriter(entry: "app:/data/notes.txt", name: "notes.txt", resolver: nil, journal: DragFiles.Journal(session: session))
        let destination = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString).txt")
        let refusal = { (self.write(writer, to: destination) as NSError?)?.localizedDescription }
        XCTAssertEqual(refusal(), "drag: app:/data/notes.txt could not be read: not booted")
        session.destroy()
        XCTAssertEqual(refusal(), "drag: app:/data/notes.txt could not be read: the app's session has ended")
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))
        withExtendedLifetime(session) {}
    }
}
#endif
