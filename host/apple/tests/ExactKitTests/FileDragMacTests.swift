#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A drag from another app over a window (FileDragMac.swift): a file drag
/// over a textarea goes to the `drop` above it instead of inserting its
/// path, a text or link drag is the field's unless its own node declares
/// `drop`, `dragenter` and `dragleave` bracket a drag at the box that hears
/// them, and a paste's files are claimed from the pasteboard
/// (PastedFiles.swift).
final class FileDragMacTests: XCTestCase {
    private var window: NSWindow!
    private var heard: [String] = []
    private var file: URL!

    override func setUpWithError() throws {
        file = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString).txt")
        try Data("notes".utf8).write(to: file)
    }
    override func tearDown() {
        FileDrag.ended()
        window?.close(); window = nil
        try? FileManager.default.removeItem(at: file)
    }

    /// A column (1) hearing `drop`, `dragenter` and `dragleave` when
    /// `column` says so, holding a textarea (2) hearing `editor`, a box (3)
    /// and an input (4) hearing `line`.
    private func presenter(column: [String], editor: [String] = ["input"], line: [String] = ["input"]) -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.onClipboard = { [unowned self] id, kind, line in heard.append("\(id):\(kind)") }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": column],
            ["op": "create", "id": 2, "kind": "textarea", "props": ["id": "editor", "value": "hello"], "handlers": editor],
            ["op": "create", "id": 3, "kind": "view", "handlers": [String]()],
            ["op": "create", "id": 4, "kind": "input", "props": ["id": "line", "value": "hello"], "handlers": line],
            ["op": "children", "id": 1, "ids": [2, 3, 4]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 200.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 300.0, "h": 80.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 100.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 150.0, "w": 300.0, "h": 24.0],
        ]))
        window.layoutIfNeeded()
        return p
    }

    private func middle(_ view: NSView) -> NSPoint { view.convert(NSPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil) }

    /// A drag of `urls`, then `text` as an item of its own, then `link` and
    /// `title` as one item, as a browser's link and Exact's link card drag
    /// them, from a source allowing `operations`.
    private func drag(_ urls: [URL], at point: NSPoint, text: String? = nil, link: String? = nil, title: String? = nil,
                      operations: NSDragOperation = .copy) -> DraggingInfo {
        let board = NSPasteboard(name: .init("exact-test-\(UUID().uuidString)"))
        board.clearContents()
        var items: [NSPasteboardWriting] = urls.map { $0 as NSURL }
        if let text { items.append(text as NSString) }
        if link != nil || title != nil {
            let item = NSPasteboardItem()
            if let link { item.setString(link, forType: .URL) }
            if let title { item.setString(title, forType: .string) }
            items.append(item)
        }
        board.writeObjects(items)
        return DraggingInfo(board: board, location: point, window: window, operations: operations)
    }

    func testAFileDragOverATextareaIsTheColumnsDrop() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let editor = try XCTUnwrap(p.views[2]?.textArea)
        let info = drag([file], at: middle(editor))
        XCTAssertEqual(editor.draggingEntered(info), .copy, "the column above takes it")
        XCTAssertEqual(editor.draggingUpdated(info), .copy)
        XCTAssertTrue(editor.prepareForDragOperation(info))
        _ = editor.performDragOperation(info)
        editor.concludeDragOperation(info)
        XCTAssertEqual(editor.string, "hello", "no path is inserted")
        // This presenter has no session to mint the file, so no drop lands
        // and the drag ends at the column with a leave
        // (`testADropThatLandsIsTheColumnsActionAndNoLeave` mints it).
        XCTAssertEqual(heard, ["1:55", "1:56"])
    }

    func testAFileDragWithNoDropAboveAndATextDragAreTheFieldsOwn() throws {
        let p = presenter(column: ["dragenter", "dragleave"])
        let editor = try XCTUnwrap(p.views[2]?.textArea)
        let files = drag([file], at: middle(editor))
        XCTAssertFalse(FileDrag.takes(editor, files), "no node declares drop")
        XCTAssertNil(FileDrag.moved(editor, files), "the field's own")
        XCTAssertEqual(heard, ["1:55"], "a drag no drop takes is still heard")
        let text = drag([], at: middle(editor), text: "words")
        XCTAssertNil(FileDrag.moved(editor, text), "a text drag is the field's")
        XCTAssertFalse(FileDrag.takes(editor, text))
    }

    func testATextOrLinkDragIsTheColumnsOverItsBoxAndTheFieldsOverAField() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let column = try XCTUnwrap(p.views[1]), editor = try XCTUnwrap(p.views[2]?.textArea), box = try XCTUnwrap(p.views[3])
        XCTAssertEqual(column.draggingEntered(drag([], at: middle(box), text: "words")), .copy)
        XCTAssertEqual(column.draggingUpdated(drag([], at: middle(box), link: "https://example.com/fuji", title: "Mount Fuji")), .copy)
        let text = drag([], at: middle(editor), text: "words")
        XCTAssertNil(FileDrag.moved(editor, text), "the field inserts it")
        XCTAssertFalse(FileDrag.takes(editor, text))
        XCTAssertNil(FileDrag.moved(editor, drag([], at: middle(editor), link: "https://example.com/fuji", title: "Mount Fuji")), "and a link")
        XCTAssertEqual(heard, ["1:55"], "the column hears the drag over its field too")
    }

    /// A drop the field takes is the column's leave, once.
    func testATextDropAFieldTakesIsOneLeaveAtTheColumn() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let editor = try XCTUnwrap(p.views[2]?.textArea)
        let text = drag([], at: middle(editor), text: "words")
        _ = editor.draggingEntered(text)
        XCTAssertNil(FileDrag.perform(editor, text), "the field's own")
        editor.draggingEnded(text)
        XCTAssertEqual(heard, ["1:55", "1:56"])
        // With a drop waiting at the column, its held enter is heard before the field's drop leaves it
        heard = []
        FileDrag.waits([FileDrag.Shown(try XCTUnwrap(p.views[1]))])
        _ = editor.draggingEntered(text)
        XCTAssertEqual(heard, [], "held")
        XCTAssertNil(FileDrag.perform(editor, text))
        editor.draggingEnded(text)
        XCTAssertEqual(heard, ["1:55", "1:56"])
        FileDrag.lands([FileDrag.Shown(try XCTUnwrap(p.views[1]))])
        XCTAssertEqual(heard, ["1:55", "1:56"])
    }

    func testAFieldWhoseOwnNodeDeclaresDropGivesItEveryDrag() throws {
        let p = presenter(column: ["drop"], editor: ["input", "drop"], line: ["input", "drop"])
        let editor = try XCTUnwrap(p.views[2]?.textArea)
        let text = drag([], at: middle(editor), text: "words")
        XCTAssertEqual(FileDrag.moved(editor, text), .copy)
        XCTAssertTrue(FileDrag.destination(editor, text) === p.views[2])
        XCTAssertTrue(FileDrag.takes(editor, drag([file], at: middle(editor))))
        // An input being edited takes the drag in the window's field editor, which AppKit places inside the input
        // and whose delegate is the input
        let input = try XCTUnwrap(p.views[4]?.field), fieldEditor = FieldEditor(frame: .zero)
        input.addSubview(fieldEditor)
        fieldEditor.delegate = try XCTUnwrap(input as Any as? NSTextViewDelegate, "an input is its field editor's delegate")
        XCTAssertTrue(FileDrag.destination(fieldEditor, files: false) === p.views[4])
        fieldEditor.delegate = nil
        XCTAssertTrue(FileDrag.destination(fieldEditor, files: false) === p.views[4], "an editor no input delegates to is no field")
        XCTAssertTrue(FileDrag.destination(fieldEditor, files: true) === p.views[4], "a drag with files goes to the nearest drop")
    }

    func testADragWhoseSourceAllowsNoCopyIsNotTaken() throws {
        let p = presenter(column: ["drop"])
        let column = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[3])
        XCTAssertEqual(column.draggingEntered(drag([file], at: middle(box), operations: .move)), [])
        XCTAssertEqual(column.draggingEntered(drag([], at: middle(box), text: "words", operations: .move)), [])
    }

    func testEnterAndLeaveAreTheColumnsBoxNotItsChildren() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let column = try XCTUnwrap(p.views[1]), editor = try XCTUnwrap(p.views[2]?.textArea), box = try XCTUnwrap(p.views[3])
        XCTAssertEqual(column.draggingEntered(drag([file], at: middle(box))), .copy)
        // Into a registered child: the column's exit, the child's enter.
        column.draggingExited(drag([file], at: middle(editor)))
        _ = editor.draggingEntered(drag([file], at: middle(editor)))
        XCTAssertEqual(heard, ["1:55"], "moving inside the column is no leave")
        // Out of the window: what was under the last point lets go.
        editor.draggingExited(drag([file], at: NSPoint(x: -20, y: -20)))
        XCTAssertEqual(heard, ["1:55", "1:56"])
        // Back in and dropped where no file could be opened (no session): a leave.
        heard = []
        _ = column.draggingEntered(drag([file], at: middle(box)))
        _ = column.performDragOperation(drag([file], at: middle(box)))
        XCTAssertEqual(heard, ["1:55", "1:56"])
    }

    /// A node a drop still waiting for promised files was over holds a newer
    /// drag's enter until that drop lands; a leave of the newer drag cancels
    /// it. AppKit receives promised files only inside a real drag, so the
    /// waiting drop is stood in for (`waits`).
    func testANewerDragsEnterAtAWaitingDropsNodeWaitsForItToLand() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let column = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[3])
        XCTAssertEqual(column.draggingEntered(drag([file], at: middle(box))), .copy)
        FileDrag.ended(dropped: true)
        FileDrag.waits([FileDrag.Shown(column)])
        _ = column.draggingEntered(drag([file], at: middle(box)))
        XCTAssertEqual(heard, ["1:55"], "the newer drag's enter waits")
        FileDrag.lands([FileDrag.Shown(column)])
        XCTAssertEqual(heard, ["1:55", "1:55"])
        column.draggingExited(drag([file], at: NSPoint(x: -20, y: -20)))
        heard = []
        _ = column.draggingEntered(drag([file], at: middle(box)))
        FileDrag.ended(dropped: true)
        FileDrag.waits([FileDrag.Shown(column)])
        _ = column.draggingEntered(drag([file], at: middle(box)))
        column.draggingExited(drag([file], at: NSPoint(x: -20, y: -20)))
        FileDrag.lands([FileDrag.Shown(column)])
        XCTAssertEqual(heard, ["1:55"], "a newer drag that left was neither entered nor left")
        // A drop that waits over a row a list then renews for another item (LLP 1078) no longer holds the row's
        // enters: a newer drop over the new item waits on its own, and the old drop's landing leaves it waiting
        heard = []
        _ = column.draggingEntered(drag([file], at: middle(box)))
        FileDrag.ended(dropped: true)
        let old = [FileDrag.Shown(column)]
        FileDrag.waits(old)
        p.renew(1)
        _ = column.draggingEntered(drag([file], at: middle(box)))
        FileDrag.ended(dropped: true)
        let newer = [FileDrag.Shown(column)]
        FileDrag.waits(newer)
        _ = column.draggingEntered(drag([file], at: middle(box)))
        XCTAssertEqual(heard, ["1:55", "1:55"], "the third drag's enter waits for the second drop")
        FileDrag.lands(old)
        XCTAssertEqual(heard, ["1:55", "1:55"], "the old drop's landing releases nothing of the new item")
        FileDrag.lands(newer)
        XCTAssertEqual(heard, ["1:55", "1:55", "1:55"])
    }

    /// An input that is not being edited takes the drag itself (`Field`).
    func testAFileDragOverAnInputIsTheColumnsDrop() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let field = try XCTUnwrap(p.views[4]?.field as? Field)
        let info = drag([file], at: middle(field))
        XCTAssertEqual(field.draggingEntered(info), .copy)
        XCTAssertTrue(field.prepareForDragOperation(info))
        _ = field.performDragOperation(info)
        field.concludeDragOperation(info)
        XCTAssertEqual(field.stringValue, "hello", "no path is inserted")
        XCTAssertEqual(heard, ["1:55", "1:56"])
    }

    /// A dropped file of a type the manifest declares is kept (bookmarked,
    /// on Open Recent); any other, and a folder, is only minted.
    /// A scroller's clip view is the drag's one destination over it while a
    /// node hears a drag's events (LLP 1094.000 D15): the page's and a scroll
    /// node's clip views register and the zone inside them does not; the
    /// target is the node under the drag's point, whichever view AppKit
    /// reports to, and the drop through the clip view is the zone's.
    func testAScrollersClipViewIsTheDragsOneDestinationOverIt() throws {
        _ = NSApplication.shared
        let source = """
        component App
          state log = ""
          action entered
            log = `${log}enter;`
          action dropped(e: DragEvent)
            log = `${log}drop:${e.files.length};`
          view
            column width="100%" height="100%"
              scroll testId="list" width=400 height=200 overflow-y="scroll"
                box testId="zone" width=400 height=100 drop=dropped dragenter=entered
                  text "Zone" testId="leaf"
                box testId="below" width=400 height=400

        """
        let session = ExactApp.shared.makeSession()
        defer { session.destroy() }
        let view = ExactView(session: session)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        XCTAssertNil(session.boot(plan: try compile(source), size: CGSize(width: 400, height: 300)).error)
        window.layoutIfNeeded()
        let p = session.presenter
        func node(_ id: String) throws -> NodeView { try XCTUnwrap(p.views.values.first { $0.props["testId"] == id }) }
        let page = try XCTUnwrap(p.viewport.contentView as? DropClipView)
        let clip = try XCTUnwrap(try node("list").scroll?.contentView as? DropClipView)
        let zone = try node("zone"), below = try node("below")
        XCTAssertTrue(page.isDestination && clip.isDestination, "both clip views")
        XCTAssertFalse(clip.registeredDraggedTypes.isEmpty)
        XCTAssertTrue(zone.registeredDraggedTypes.isEmpty, "a node inside a clip view registers for nothing")
        XCTAssertNil(FileDrag.moved(clip, drag([file], at: middle(below))), "nothing takes it below the zone")
        XCTAssertEqual(FileDrag.moved(page, drag([file], at: middle(zone))), .copy, "an outer clip view AppKit still reports to answers for the point")
        XCTAssertEqual(FileDrag.moved(clip, drag([file], at: middle(try node("leaf")))), .copy, "a leaf inside the zone gives it the zone's")
        let info = drag([file], at: middle(zone))
        XCTAssertEqual(clip.draggingUpdated(info), .copy, "the zone under the point")
        XCTAssertTrue(clip.performDragOperation(info), "the drop through the clip view")
        clip.draggingEnded(info)
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
        XCTAssertEqual((state["slots"] as? [String: Any])?["log"] as? String, "enter;drop:1;", "is the zone's")
    }

    /// The clip views stop being destinations once no node hears a drag's
    /// events, and a scroller made while one does is one (LLP 1094.000 D15).
    func testTheClipViewsFollowWhetherANodeHearsADrag() throws {
        _ = NSApplication.shared
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 5, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "children", "id": 1, "ids": [5]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 300.0],
            ["op": "frame", "id": 5, "x": 0.0, "y": 0.0, "w": 400.0, "h": 200.0],
        ]))
        let page = try XCTUnwrap(p.viewport.contentView as? DropClipView)
        let first = try XCTUnwrap(p.views[5]?.scroll?.contentView as? DropClipView)
        XCTAssertFalse(page.isDestination || first.isDestination, "no node hears a drag's events")
        p.apply(wireBatch([
            ["op": "create", "id": 6, "kind": "view", "handlers": ["dragleave"]],
            ["op": "create", "id": 7, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "children", "id": 5, "ids": [6, 7]],
            ["op": "frame", "id": 6, "x": 0.0, "y": 0.0, "w": 400.0, "h": 100.0],
            ["op": "frame", "id": 7, "x": 0.0, "y": 100.0, "w": 400.0, "h": 100.0],
        ]))
        let second = try XCTUnwrap(p.views[7]?.scroll?.contentView as? DropClipView)
        XCTAssertTrue(page.isDestination && first.isDestination && second.isDestination, "each clip view, a new one's too")
        XCTAssertTrue(try XCTUnwrap(p.views[6]).registeredDraggedTypes.isEmpty, "the node inside registers for nothing")
        p.apply(wireBatch([["op": "destroy", "id": 6]]))
        XCTAssertFalse(page.isDestination || first.isDestination || second.isDestination, "none once no node hears")
        XCTAssertTrue(first.registeredDraggedTypes.isEmpty)
    }

    /// A drag that moves into another window leaves the nodes of the window
    /// it left with their records measured there (LLP 1094.000 D15).
    func testALeaveAcrossWindowsKeepsTheLeftWindowsPoint() throws {
        let a = presenter(column: ["drop", "dragenter", "dragleave"])
        let windowA = try XCTUnwrap(window)
        var lines: [String] = []
        a.onClipboard = { id, kind, line in if kind == 56 { lines.append("\(id):\(line)") } }
        let column = try XCTUnwrap(a.views[1])
        let inA = middle(try XCTUnwrap(a.views[3]))
        XCTAssertEqual(FileDrag.moved(column, drag([file], at: inA)), .copy)
        let b = presenter(column: ["drop"])
        defer { windowA.close() }
        XCTAssertEqual(FileDrag.moved(try XCTUnwrap(b.views[1]), drag([file], at: NSPoint(x: 5, y: 5))), .copy)
        XCTAssertEqual(lines, ["1:\(column.dragLine(inA))"], "the column's leave, at the point it last had in its window")
    }

    /// An enter whose handler makes a node further out inert skips that
    /// node's queued enter, and the node owes no leave (LLP 1094.000 D15).
    func testAnEnterThatMakesAnOuterNodeInertSkipsItsEnter() throws {
        _ = NSApplication.shared
        let source = """
        component App
          state log = ""
          state locked = false
          action innerEnter(e: DragEvent)
            log = `${log}inner-enter;`
            locked = true
          action innerLeave(e: DragEvent)
            log = `${log}inner-leave;`
          action outerEnter(e: DragEvent)
            log = `${log}outer-enter;`
          action outerLeave(e: DragEvent)
            log = `${log}outer-leave;`
          view
            column testId="outer" width="100%" height="100%" dragenter=outerEnter dragleave=outerLeave inert=locked
              box testId="inner" width=200 height=100 dragenter=innerEnter dragleave=innerLeave

        """
        let session = ExactApp.shared.makeSession()
        defer { session.destroy() }
        let view = ExactView(session: session)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        XCTAssertNil(session.boot(plan: try compile(source), size: CGSize(width: 400, height: 300)).error)
        window.layoutIfNeeded()
        let inner = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "inner" })
        let page = try XCTUnwrap(session.presenter.viewport.contentView as? DropClipView)
        let info = drag([file], at: middle(inner))
        _ = FileDrag.moved(page, info)
        XCTAssertNil(FileDrag.perform(page, info), "nothing takes the drop")
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
        XCTAssertEqual((state["slots"] as? [String: Any])?["log"] as? String, "inner-enter;inner-leave;")
    }

    /// A release in another window that nothing takes sends the leaves of
    /// the window the drag left with their records measured there.
    func testAReleaseInAnotherWindowLeavesWithTheLeftWindowsPoint() throws {
        let a = presenter(column: ["drop", "dragenter", "dragleave"])
        let windowA = try XCTUnwrap(window)
        defer { windowA.close() }
        var lines: [String] = []
        a.onClipboard = { id, kind, line in if kind == 56 { lines.append("\(id):\(line)") } }
        let column = try XCTUnwrap(a.views[1])
        let inA = middle(try XCTUnwrap(a.views[3]))
        XCTAssertEqual(FileDrag.moved(column, drag([file], at: inA)), .copy)
        let b = presenter(column: [])
        XCTAssertNil(FileDrag.perform(try XCTUnwrap(b.views[1]), drag([file], at: NSPoint(x: 5, y: 5))), "nothing in this window takes it")
        XCTAssertEqual(lines, ["1:\(column.dragLine(inA))"], "the column's leave, at the point it last had in its window")
    }

    func testOnlyADeclaredTypeIsKept() {
        let notes = URL(fileURLWithPath: "/tmp/a.notes"), other = URL(fileURLWithPath: "/tmp/b.zip"), folder = URL(fileURLWithPath: "/tmp/photos", isDirectory: true)
        var kept: [String] = [], minted: [String] = []
        let docs = NodeView.droppedDocuments([notes, other, folder, URL(string: "https://example.com/x")!],
            accepts: { $0.pathExtension == "notes" },
            keep: { kept.append($0.lastPathComponent); return "doc:/k/\($0.lastPathComponent)" },
            mint: { minted.append($0.lastPathComponent); return "doc:/m/\($0.lastPathComponent)" })
        XCTAssertEqual(docs, ["doc:/k/a.notes", "doc:/m/b.zip", "doc:/m/photos"])
        XCTAssertEqual(kept, ["a.notes"])
        XCTAssertEqual(minted, ["b.zip", "photos"], "a web URL is no file")
    }

    /// With a session: the file over the textarea is minted, the column's
    /// `drop` action runs with its handle, and no `dragleave` follows it,
    /// as a browser's drop is the last event of a drag that lands.
    func testADropThatLandsIsTheColumnsActionAndNoLeave() throws {
        _ = NSApplication.shared
        let source = """
        component App
          state log = ""
          action entered
            log = `${log}enter;`
          action left
            log = `${log}leave;`
          action dropped(e: DragEvent)
            log = `${log}drop:${e.files.join(",")};`
          view
            column testId="column" width="100%" height="100%" dragenter=entered dragleave=left drop=dropped
              textarea testId="field" value="hello" height=80

        """
        let session = ExactApp.shared.makeSession()
        defer { session.destroy() }
        let view = ExactView(session: session)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        XCTAssertNil(session.boot(plan: try compile(source), size: CGSize(width: 400, height: 300)).error)
        window.layoutIfNeeded()
        let editor = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "field" }?.textArea)
        let info = drag([file], at: middle(editor))
        XCTAssertEqual(editor.draggingEntered(info), .copy)
        XCTAssertTrue(editor.prepareForDragOperation(info))
        XCTAssertTrue(editor.performDragOperation(info), "the column took the drop")
        editor.concludeDragOperation(info)
        editor.draggingEnded(info)
        XCTAssertEqual(editor.string, "hello", "no path is inserted")
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
        let log = try XCTUnwrap((state["slots"] as? [String: Any])?["log"] as? String)
        XCTAssertTrue(log.hasPrefix("enter;drop:doc:/") && log.hasSuffix("/\(file.lastPathComponent);"), log)
        XCTAssertFalse(log.contains("leave"), "a drop that lands is the drag's last event: \(log)")
    }

    /// A link and its title dropped in a session, as one item, and a text
    /// of its own: the action hears the title and the other text joined, the
    /// link, and no file.
    func testALinkAndTextThatLandAreTheColumnsActionWithNoFile() throws {
        _ = NSApplication.shared
        let source = """
        component App
          state log = ""
          action entered
            log = `${log}enter;`
          action left
            log = `${log}leave;`
          action dropped(e: DragEvent)
            log = `${log}drop:${e.text}|${e.url}|${e.files.length};`
          view
            column testId="column" width="100%" height="100%" dragenter=entered dragleave=left drop=dropped
              textarea testId="field" value="hello" height=80

        """
        let session = ExactApp.shared.makeSession()
        defer { session.destroy() }
        let view = ExactView(session: session)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        XCTAssertNil(session.boot(plan: try compile(source), size: CGSize(width: 400, height: 300)).error)
        window.layoutIfNeeded()
        let column = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "column" })
        let below = column.convert(NSPoint(x: column.bounds.midX, y: column.isFlipped ? 200 : column.bounds.height - 200), to: nil)
        let info = drag([], at: below, text: "Japan", link: "https://example.com/fuji", title: "Mount Fuji")
        XCTAssertEqual(column.draggingEntered(info), .copy)
        XCTAssertTrue(column.performDragOperation(info), "the column took the drop")
        column.draggingEnded(info)
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
        let log = try XCTUnwrap((state["slots"] as? [String: Any])?["log"] as? String)
        XCTAssertEqual(log, "enter;drop:Japan\nMount Fuji|https://example.com/fuji|0;")
    }

    /// An Exact node's file item, its file with its link and title and
    /// `NodeItem.type` (LLP 1094.000 D5), drops all three; the same item
    /// without that type drops its file alone (LLP 1094.001 D3).
    func testAnExactNodesFileItemDropsItsFileLinkAndText() throws {
        _ = NSApplication.shared
        let source = """
        component App
          state log = ""
          action dropped(e: DragEvent)
            log = `${log}drop:${e.text}|${e.url}|${e.files.length};`
          view
            column testId="column" width="100%" height="100%" drop=dropped

        """
        for exact in [true, false] {
            let session = ExactApp.shared.makeSession()
            defer { session.destroy() }
            let view = ExactView(session: session)
            window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
            window.contentView = mount
            view.frame = mount.bounds; mount.addSubview(view)
            XCTAssertNil(session.boot(plan: try compile(source), size: CGSize(width: 400, height: 300)).error)
            window.layoutIfNeeded()
            let column = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "column" })
            let board = NSPasteboard(name: .init("exact-test-\(UUID().uuidString)"))
            board.clearContents()
            let item = NSPasteboardItem()
            item.setString(file.absoluteString, forType: .fileURL)
            item.setString("https://example.com/fuji", forType: .URL)
            item.setString("Mount Fuji", forType: .string)
            if exact { item.setData(Data(), forType: .init(NodeItem.type)) }
            board.writeObjects([item])
            let info = DraggingInfo(board: board, location: column.convert(NSPoint(x: column.bounds.midX, y: column.bounds.midY), to: nil), window: window, operations: .copy)
            XCTAssertEqual(column.draggingEntered(info), .copy)
            XCTAssertTrue(column.performDragOperation(info), "the column took the drop")
            column.draggingEnded(info)
            let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
            let log = try XCTUnwrap((state["slots"] as? [String: Any])?["log"] as? String)
            XCTAssertEqual(log, exact ? "drop:Mount Fuji|https://example.com/fuji|1;" : "drop:||1;")
        }
    }

    private func compile(_ source: String) throws -> Data {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"], "build.mjs --test builds the Contract compiler and names it"))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    func testAPastesFilesAreItsFileURLsElseItsImageAsPNG() throws {
        let board = NSPasteboard(name: .init("exact-test-\(UUID().uuidString)"))
        defer { board.releaseGlobally() }
        let icon = try XCTUnwrap(NSImage(size: NSSize(width: 4, height: 4), flipped: false) { r in NSColor.red.setFill(); r.fill(); return true }.tiffRepresentation)
        // A Finder copy: the file, its icon and its name.
        board.clearContents()
        let item = NSPasteboardItem()
        item.setString(file.absoluteString, forType: .fileURL)
        item.setData(icon, forType: .tiff)
        item.setString(file.lastPathComponent, forType: .string)
        board.writeObjects([item])
        let finder = NodeView.pasteClaim(board)
        XCTAssertEqual(finder.urls.map(\.standardizedFileURL), [file.standardizedFileURL])
        XCTAssertNil(finder.png, "the icon is not pasted beside its file")
        // A screenshot: PNG data alone.
        let png = try XCTUnwrap(NSBitmapImageRep(data: icon)?.representation(using: .png, properties: [:]))
        board.clearContents()
        board.setData(png, forType: .png)
        XCTAssertEqual(NodeView.pasteClaim(board).png, png)
        XCTAssertTrue(NodeView.pasteClaim(board).urls.isEmpty)
        // An image copied from an app that writes TIFF: converted.
        board.clearContents()
        board.setData(icon, forType: .tiff)
        let converted = try XCTUnwrap(NodeView.pasteClaim(board).png)
        XCTAssertEqual(Array(converted.prefix(4)), [0x89, 0x50, 0x4E, 0x47])
        // Text alone has none.
        board.clearContents()
        board.setString("words", forType: .string)
        let words = NodeView.pasteClaim(board)
        XCTAssertTrue(words.urls.isEmpty && words.png == nil)
        let written = try XCTUnwrap(NodeView.writePastedImage(converted))
        defer { try? FileManager.default.removeItem(at: written.deletingLastPathComponent()) }
        XCTAssertEqual(written.lastPathComponent, "image.png")
        XCTAssertEqual(try Data(contentsOf: written), converted)
    }

    /// LLP 1094.000 D16: the app's own drag, which the Mac's drop side
    /// refuses (D5), runs its source's `dragstart` before the first enter,
    /// and at its end the leave the drop side deferred goes before the
    /// source's `dragend`.
    func testTheAppsOwnDragStartsBeforeItsEnterAndEndsAfterItsLeave() throws {
        let p = presenter(column: ["dragenter", "dragleave"])
        let column = try XCTUnwrap(p.views[1]), source = OwnSource()
        source.own = OwnDrag { [unowned self] in heard.append("start") }
        let info = DraggingInfo(board: drag([], at: .zero, text: "Mount Fuji").board, location: middle(try XCTUnwrap(p.views[3])), window: window,
                                operations: [], source: source)
        XCTAssertNil(FileDrag.moved(column, info), "the app's own drag takes no drop in the app")
        XCTAssertEqual(heard, ["start", "1:55"])
        // AppKit's exit at a point still in the window lets go a moment later
        FileDrag.exited(column, info)
        XCTAssertEqual(heard, ["start", "1:55"])
        source.own?.ended(taken: false) { [unowned self] taken in heard.append("end \(taken)") }
        XCTAssertEqual(heard, ["start", "1:55", "1:56", "end false"])
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        XCTAssertEqual(heard, ["start", "1:55", "1:56", "end false"], "the deferred exit is stale, and the start went once")
        // A start whose handler renews the node under the drag for another item (LLP 1078): the old item's enter,
        // taken before the handler ran, does not reach the new one
        heard = []
        let renewing = OwnSource()
        renewing.own = OwnDrag { [unowned self] in heard.append("start"); p.renew(1) }
        let next = DraggingInfo(board: info.board, location: info.location, window: window, operations: [], source: renewing)
        XCTAssertNil(FileDrag.moved(column, next))
        XCTAssertEqual(heard, ["start"])
        _ = FileDrag.moved(column, next)
        XCTAssertEqual(heard, ["start", "1:55"], "the next report finds the node as it is now")
        FileDrag.ended()
        // An enter a waiting drop holds sends nothing, so the start waits too, until the enter is heard
        heard = []
        let waiting = OwnSource()
        waiting.own = OwnDrag { [unowned self] in heard.append("start") }
        let held = DraggingInfo(board: info.board, location: info.location, window: window, operations: [], source: waiting)
        let landing = [FileDrag.Shown(column)]
        FileDrag.waits(landing)
        _ = FileDrag.moved(column, held)
        XCTAssertEqual(heard, [], "nothing sent, nothing started")
        FileDrag.lands(landing)
        XCTAssertEqual(heard, ["start", "1:55"])
        FileDrag.ended()
        // A held enter whose node the start's handler makes inert is heard no more, nor its leave
        heard = []
        let inerting = OwnSource()
        inerting.own = OwnDrag { [unowned self] in heard.append("start"); p.apply(wireBatch([["op": "props", "id": 1, "set": ["inert": "true"]]])) }
        let inert = DraggingInfo(board: info.board, location: info.location, window: window, operations: [], source: inerting)
        let still = [FileDrag.Shown(column)]
        FileDrag.waits(still)
        _ = FileDrag.moved(column, inert)
        FileDrag.lands(still)
        FileDrag.ended()
        XCTAssertEqual(heard, ["start"])
    }
}

/// The source of a drag a node of the app began, as the drop side meets it.
private final class OwnSource: NSObject, OwnDragSource {
    var own: OwnDrag?
}

/// An `NSDraggingInfo` for a drag at a window point, as AppKit hands one.
private final class DraggingInfo: NSObject, NSDraggingInfo {
    let board: NSPasteboard, location: NSPoint
    weak var window: NSWindow?
    let operations: NSDragOperation
    let source: Any?
    init(board: NSPasteboard, location: NSPoint, window: NSWindow, operations: NSDragOperation = .copy, source: Any? = nil) {
        self.board = board; self.location = location; self.window = window; self.operations = operations; self.source = source
    }
    var draggingDestinationWindow: NSWindow? { window }
    var draggingSourceOperationMask: NSDragOperation { operations }
    var draggingLocation: NSPoint { location }
    var draggedImageLocation: NSPoint { location }
    var draggedImage: NSImage? { nil }
    var draggingPasteboard: NSPasteboard { board }
    var draggingSource: Any? { source }
    var draggingSequenceNumber: Int { 1 }
    func slideDraggedImage(to screenPoint: NSPoint) {}
    var draggingFormation: NSDraggingFormation = .default
    var animatesToDestination = false
    var numberOfValidItemsForDrop = 1
    /// Each item's file URL with its index, as AppKit enumerates a drag's
    /// items: the only file these drags carry.
    func enumerateDraggingItems(options enumOpts: NSDraggingItemEnumerationOptions = [], for view: NSView?, classes classArray: [AnyClass], searchOptions: [NSPasteboard.ReadingOptionKey: Any] = [:], using block: @escaping (NSDraggingItem, Int, UnsafeMutablePointer<ObjCBool>) -> Void) {
        guard classArray.contains(where: { ObjectIdentifier($0) == ObjectIdentifier(NSURL.self) }) else { return }
        var stop: ObjCBool = false
        for (index, item) in (board.pasteboardItems ?? []).enumerated() {
            guard let string = item.string(forType: .fileURL), let url = URL(string: string) else { continue }
            block(NSDraggingItem(pasteboardWriter: url as NSURL), index, &stop)
            if stop.boolValue { return }
        }
    }
    var springLoadingHighlight: NSSpringLoadingHighlight { .none }
    func resetSpringLoading() {}
}
#endif
