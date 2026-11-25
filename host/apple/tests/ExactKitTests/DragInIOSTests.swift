#if os(iOS)
import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import ExactDragIn
@testable import ExactKit

/// LLP 1094.001 D4 on UIKit: what a dropped item is, read from its own types
/// and the system's type tree; its files copied out and its text and link
/// read apart; the window's one interaction and the tracker its callbacks
/// reach, with a stand-in drop session (UIKit makes a real one only in a real
/// drag); a field that gives every drag to its own node's `drop`; the agent's
/// drop; and the drop point VoiceOver offers. A simulator runs it:
/// bun host/apple/build.mjs --test --ios --only DragInIOSTests
final class DragInIOSTests: XCTestCase {
    private var window: UIWindow!
    private var heard: [String] = []
    private var folder: URL!

    override func setUpWithError() throws {
        folder = FileManager.default.temporaryDirectory.appendingPathComponent("drop-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }
    override func tearDown() {
        window = nil
        try? FileManager.default.removeItem(at: folder)
    }
    private func fixture(_ name: String, _ bytes: String) throws -> URL {
        let url = folder.appendingPathComponent(name)
        try Data(bytes.utf8).write(to: url)
        return url
    }

    /// A column (1) hearing `column` on the page (9), holding a textarea (2)
    /// hearing `editor` at its top and, below it, a paragraph (4). In a new
    /// window, or in `shared` at `frame`, its events heard with `tag`.
    private func presenter(column: [String], editor: [String] = ["input"], in shared: UIWindow? = nil, frame: CGRect? = nil, tag: String = "") -> Presenter {
        let p = Presenter()
        if shared == nil { window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800)) }
        p.viewport.frame = frame ?? window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.onClipboard = { [unowned self] id, kind, line in heard.append("\(tag)\(id):\(kind)" + (kind == 38 ? ":" + line.split(separator: "\n").last.map(String.init)! : "")) }
        p.apply(wireBatch([
            ["op": "create", "id": 9, "kind": "view"],
            ["op": "create", "id": 1, "kind": "view", "handlers": column],
            ["op": "create", "id": 2, "kind": "textarea", "props": ["id": "editor", "value": "hello"], "handlers": editor],
            ["op": "create", "id": 4, "kind": "text", "props": ["text": "Drop here"]],
            ["op": "children", "id": 9, "ids": [1]],
            ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "roots", "ids": [9]],
            ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 300.0, "h": 80.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 300.0, "w": 200.0, "h": 20.0],
        ]))
        return p
    }
    /// In the column's own box, below the textarea; and over the textarea
    private let below = CGPoint(x: 150, y: 200), overField = CGPoint(x: 50, y: 40)
    private func target(_ p: Presenter) throws -> DropTarget { try XCTUnwrap(p.dragIn as? DropTarget) }
    private func drops(_ p: Presenter) throws -> WindowDrops { try XCTUnwrap(try target(p).drops) }
    /// The interaction a callback names, which the window's delegate does not read
    private let quiet = Quiet()
    private lazy var interaction = UIDropInteraction(delegate: quiet)

    private func provider(_ objects: [NSItemProviderWriting]) -> NSItemProvider {
        let provider = NSItemProvider()
        objects.forEach { provider.registerObject($0, visibility: .all) }
        return provider
    }
    /// A file representation of `url` as `type`, handed over once `gate` (if
    /// any) is signalled, signalling `then` (if any) once it has.
    private func fileProvider(_ url: URL?, type: UTType, inPlace: Bool = false, gate: DispatchSemaphore? = nil, then: DispatchSemaphore? = nil) -> NSItemProvider {
        let provider = NSItemProvider()
        provider.registerFileRepresentation(forTypeIdentifier: type.identifier, fileOptions: inPlace ? [.openInPlace] : [], visibility: .all) { done in
            // A provider in this process loads on the asking thread, so a gated one answers from another
            let answer = { done(url, inPlace, url == nil ? CocoaError(.fileNoSuchFile) : nil); then?.signal() }
            if let gate { DispatchQueue.global().async { gate.wait(); answer() } } else { answer() }
            return nil
        }
        return provider
    }
    /// Waits until `heard` has `count` events, running the main queue.
    private func hear(_ count: Int, timeout: TimeInterval = 10) { until(timeout: timeout) { heard.count >= count } }
    private func until(timeout: TimeInterval = 10, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(timeout)
        while !done(), Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.02)) }
    }
    /// Runs what the main queue holds now, such as an exit's clear.
    private func drain() {
        let ran = expectation(description: "the main queue")
        DispatchQueue.main.async { ran.fulfill() }
        wait(for: [ran], timeout: 5)
    }

    func testAnItemIsWhatItsOwnTypesSay() throws {
        let link = provider([NSURL(string: "https://example.com/fuji")!, "Mount Fuji" as NSString])
        XCTAssertEqual(DropItems.kind(of: link), .words(url: true, text: true), "a link card gives its link and its title")
        XCTAssertEqual(DropItems.kind(of: provider([NSURL(string: "https://example.com/")!])), .words(url: true, text: false))
        XCTAssertEqual(DropItems.kind(of: provider(["words" as NSString])), .words(url: false, text: true))
        let rich = NSAttributedString(string: "Bold", attributes: [.font: UIFont.boldSystemFont(ofSize: 17)])
        XCTAssertEqual(DropItems.kind(of: provider([rich])), .words(url: false, text: true), "rich text gives its plain-text form")
        let image = UIGraphicsImageRenderer(size: CGSize(width: 4, height: 4)).image { c in UIColor.red.setFill(); c.fill(CGRect(x: 0, y: 0, width: 4, height: 4)) }
        guard case .file(let picture) = DropItems.kind(of: provider([image])) else { return XCTFail("a picture is a file") }
        XCTAssertTrue(UTType(picture)?.conforms(to: .image) == true, "by its first declared type: \(picture)")
        let pdf = try fixture("notes.pdf", "%PDF-1.4"), text = try fixture("notes.txt", "notes")
        XCTAssertEqual(DropItems.kind(of: fileProvider(pdf, type: .pdf)), .file(UTType.pdf.identifier))
        let zip = NSItemProvider()
        zip.registerDataRepresentation(forTypeIdentifier: UTType.zip.identifier, visibility: .all) { done in done(Data(), nil); return nil }
        XCTAssertEqual(DropItems.kind(of: zip), .file(UTType.zip.identifier), "a type given as data")
        let odd = try XCTUnwrap(UTType(filenameExtension: "exactodd"))
        XCTAssertTrue(odd.isDynamic)
        XCTAssertEqual(DropItems.kind(of: fileProvider(text, type: odd)), .file(odd.identifier), "a dynamic type, as Exact's file card of an unknown extension registers")
        XCTAssertEqual(DropItems.kind(of: fileProvider(text, type: .plainText, inPlace: true)), .file(UTType.plainText.identifier), "a text file offered in place")
        XCTAssertEqual(DropItems.kind(of: fileProvider(text, type: .plainText)), .words(url: false, text: true), "a text file not offered in place, as a text view reads it")
        let mail = provider([NSURL(string: "mailto:a@example.com")!])
        mail.registerFileRepresentation(forTypeIdentifier: UTType.jpeg.identifier, fileOptions: [], visibility: .all) { done in done(nil, false, nil); return nil }
        XCTAssertEqual(DropItems.kind(of: mail), .words(url: true, text: false), "a non-file link beside a file gives its link")
        XCTAssertEqual(DropItems.kind(of: provider([NSURL(fileURLWithPath: text.path)])), .none, "a bare file URL")
    }

    /// Files copied out in the items' order, whatever order they arrive in, a
    /// failed one and a folder left out, with a link and its title read apart.
    func testTheItemsFilesAreCopiedOutInOrderAndTheirTextAndLinkReadApart() throws {
        let first = try fixture("first.pdf", "%PDF-1.4 first"), second = try fixture("second.txt", "second"), late = DispatchSemaphore(value: 0)
        let link = provider([NSURL(string: "https://example.com/fuji")!, "Mount Fuji" as NSString])
        // The first file is handed over only once the last has been
        let providers = [fileProvider(first, type: .pdf, gate: late), fileProvider(nil, type: .pdf), fileProvider(folder, type: .folder), link,
                         fileProvider(second, type: .plainText, inPlace: true, then: late)]
        let read = expectation(description: "read")
        var said: [String] = []
        DropItems.read(providers, log: { said.append($0) }) { files, text, url, into in
            XCTAssertEqual(files.map(\.lastPathComponent), ["first.pdf", "second.txt"], "in the items' order, the failed and the folder left out")
            XCTAssertEqual(files.map { try? String(contentsOf: $0, encoding: .utf8) }, ["%PDF-1.4 first", "second"])
            XCTAssertTrue(files.allSatisfy { $0.path.hasPrefix(into.path) && $0 != first && $0 != second }, "copies of their own")
            XCTAssertEqual(text, "Mount Fuji")
            XCTAssertEqual(url, "https://example.com/fuji")
            read.fulfill()
        }
        wait(for: [read], timeout: 10)
        drain()
        XCTAssertTrue(said.contains { $0.contains("did not arrive") } && said.contains { $0.contains("is a folder") }, "\(said)")
    }

    /// A drag's callbacks through the window's interaction reach the column's
    /// tracker, and its drop delivers what it carried once every item loads.
    func testADropThroughTheWindowsInteractionReachesTheNode() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drops = try drops(p), point = try XCTUnwrap(p.views[1]).convert(below, to: nil)
        XCTAssertTrue(window.interactions.contains { ($0 as? UIDropInteraction)?.delegate === drops }, "the window's one interaction")
        let session = Session([provider([NSURL(string: "https://example.com/fuji")!, "Mount Fuji" as NSString])], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: session)
        XCTAssertEqual(drops.dropInteraction(interaction, sessionDidUpdate: session).operation, .copy)
        drops.dropInteraction(interaction, performDrop: session)
        drops.dropInteraction(interaction, sessionDidEnd: session)
        hear(2)
        XCTAssertEqual(heard, ["1:55", #"1:38:[[],"Mount Fuji","https:\/\/example.com\/fuji"]"#])
    }

    /// The tracker follows one drag: another session's reports and end are
    /// ignored, and the first still drops; an exit from a view the drag
    /// already left lets nothing go.
    func testAnotherSessionAndALateExitLetNothingGo() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drops = try drops(p), drag = try target(p), editor = try XCTUnwrap(p.views[2]?.textArea), point = try XCTUnwrap(p.views[1]).convert(below, to: nil)
        let session = Session([provider(["words" as NSString])], at: point), other = Session([provider(["other" as NSString])], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: session)
        XCTAssertEqual(drops.dropInteraction(interaction, sessionDidUpdate: other).operation, .forbidden, "one drag at a time")
        drops.dropInteraction(interaction, sessionDidExit: other)
        drops.dropInteraction(interaction, sessionDidEnd: other)
        drain()
        XCTAssertEqual(heard, ["1:55"], "another session's exit and end let nothing go")
        drops.dropInteraction(interaction, performDrop: session)
        drops.dropInteraction(interaction, sessionDidEnd: session)
        hear(2)
        XCTAssertEqual(heard, ["1:55", #"1:38:[[],"words",""]"#], "and the first drag drops")
        heard = []
        // Onto the field, whose text drop reports it, then the window's late exit
        let next = Session([provider(["next" as NSString])], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: next)
        drag.textDroppableView(editor, dropSessionDidEnter: next)
        drops.dropInteraction(interaction, sessionDidExit: next)
        drain()
        XCTAssertEqual(heard, ["1:55"], "a late exit")
        drag.textDroppableView(editor, dropSessionDidExit: next)
        drain()
        XCTAssertEqual(heard, ["1:55", "1:56"], "the exit from the view that reports it")
    }

    /// A drag that moves from one presenter's zone onto another's field in
    /// the same window, the field reporting it before the window's exit, is
    /// followed by the second alone: the first lets it go, and takes the
    /// next drag once it is cancelled.
    func testAFieldTakesTheDragFromAnotherPresenterInItsWindow() throws {
        let a = presenter(column: ["drop", "dragenter", "dragleave"], frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        let b = presenter(column: ["drop", "dragenter", "dragleave"], in: window, frame: CGRect(x: 0, y: 400, width: 400, height: 400), tag: "b")
        let drops = try drops(a), next = try target(b), editor = try XCTUnwrap(b.views[2]?.textArea)
        XCTAssertTrue(try self.drops(b) === drops, "one interaction for the window")
        let session = Session([provider(["words" as NSString])], at: try XCTUnwrap(a.views[1]).convert(below, to: nil))
        drops.dropInteraction(interaction, sessionDidEnter: session)
        session.point = editor.convert(CGPoint(x: 20, y: 20), to: nil)
        next.textDroppableView(editor, dropSessionDidEnter: session)
        drops.dropInteraction(interaction, sessionDidExit: session)
        drain()
        XCTAssertEqual(heard, ["1:55", "1:56", "b1:55"], "the first lets it go as the second's field takes it")
        next.textDroppableView(editor, dropSessionDidEnd: session)
        drops.dropInteraction(interaction, sessionDidEnd: session)
        XCTAssertEqual(heard, ["1:55", "1:56", "b1:55", "b1:56"])
        let after = Session([provider(["next" as NSString])], at: try XCTUnwrap(a.views[1]).convert(below, to: nil))
        XCTAssertEqual(drops.dropInteraction(interaction, sessionDidUpdate: after).operation, .copy, "the first takes the next drag")
        XCTAssertEqual(heard.last, "1:55")
    }

    /// A target its own `dragenter` replaces as the drop lands is the node the
    /// drop was at: the replacement takes nothing, through the window or the agent.
    func testATargetReplacedByItsOwnEnterTakesNothing() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drops = try drops(p), drag = try target(p), point = try XCTUnwrap(p.views[1]).convert(below, to: nil)
        var replacement: UInt32 = 6
        let replace = { [unowned self] in
            p.onClipboard = { [unowned self] id, kind, line in
                heard.append("\(id):\(kind)")
                guard kind == 55, id != replacement else { return }
                p.apply(wireBatch([["op": "destroy", "id": Int(id)],
                                   ["op": "create", "id": Int(replacement), "kind": "view", "handlers": ["drop", "dragenter", "dragleave"]],
                                   ["op": "children", "id": Int(replacement), "ids": [2, 4]], ["op": "children", "id": 9, "ids": [Int(replacement)]],
                                   ["op": "frame", "id": Int(replacement), "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0]]))
            }
        }
        replace()
        drops.dropInteraction(interaction, performDrop: Session([provider(["words" as NSString])], at: point))
        until { drag.waiting.isEmpty }
        XCTAssertEqual(heard, ["1:55"], "the drop's node is gone, and its replacement takes nothing")
        heard = []
        replacement = 7
        let reply = drag.agentDrag(["drop": [String](), "text": "words", "at": [Double(below.x), Double(below.y)]], over: try XCTUnwrap(p.views[6]))
        XCTAssertNotNil(reply["error"], "\(reply)")
        XCTAssertEqual(heard, ["6:55"], "the agent's drop too")
    }

    /// The agent's drop on a target its own enter makes inert is refused, as a person's is.
    func testTheAgentsDropOnATargetItsEnterMakesInertIsRefused() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drag = try target(p)
        p.onClipboard = { [unowned self] id, kind, _ in
            heard.append("\(id):\(kind)")
            if kind == 55 { p.apply(wireBatch([["op": "props", "id": Int(id), "set": ["inert": "true"]]])) }
        }
        let reply = drag.agentDrag(["drop": [String](), "text": "words", "at": [Double(below.x), Double(below.y)]], over: try XCTUnwrap(p.views[1]))
        XCTAssertTrue((reply["error"] as? String)?.contains("no longer takes a drop") == true, "\(reply)")
        XCTAssertEqual(heard, ["1:55", "1:56"], "its leave, and no drop")
    }

    /// A drop on one presenter's zone that another presenter followed: the
    /// first's leave runs after the drop's target is taken, so a leave
    /// handler that replaces that target gives the drop to nothing.
    func testADropAcrossPresentersIsTakenBeforeTheFirstLeaves() throws {
        let a = presenter(column: ["drop", "dragenter", "dragleave"], frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        let b = presenter(column: ["drop", "dragenter", "dragleave"], in: window, frame: CGRect(x: 0, y: 400, width: 400, height: 400), tag: "b")
        let drops = try drops(a), next = try target(b)
        a.onClipboard = { [unowned self] id, kind, _ in
            heard.append("\(id):\(kind)")
            guard kind == 56 else { return }
            b.apply(wireBatch([["op": "destroy", "id": 1],
                               ["op": "create", "id": 6, "kind": "view", "handlers": ["drop", "dragenter", "dragleave"]],
                               ["op": "children", "id": 6, "ids": [2, 4]], ["op": "children", "id": 9, "ids": [6]],
                               ["op": "frame", "id": 6, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0]]))
        }
        let session = Session([provider(["words" as NSString])], at: try XCTUnwrap(a.views[1]).convert(below, to: nil))
        drops.dropInteraction(interaction, sessionDidEnter: session)
        session.point = try XCTUnwrap(b.views[1]).convert(below, to: nil)
        drops.dropInteraction(interaction, performDrop: session)
        until { next.waiting.isEmpty }
        XCTAssertEqual(heard, ["1:55", "b1:55", "1:56"], "the drop's node is gone, and its replacement takes nothing")
    }

    /// A presenter moved to another window lets its drag go, since the old
    /// window's end no longer reaches it, and takes a drag in the new one.
    func testAPresenterMovedToAnotherWindowLetsItsDragGo() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drag = try target(p), point = try XCTUnwrap(p.views[1]).convert(below, to: nil)
        try drops(p).dropInteraction(interaction, sessionDidEnter: Session([provider(["words" as NSString])], at: point))
        let other = UIWindow(frame: window.frame)
        other.addSubview(p.viewport)
        other.makeKeyAndVisible()
        drag.sync() // as the session's view does when it moves to a window
        XCTAssertEqual(heard, ["1:55", "1:56"])
        let moved = try drops(p)
        XCTAssertTrue(other.interactions.contains { ($0 as? UIDropInteraction)?.delegate === moved }, "the new window's interaction")
        XCTAssertEqual(moved.dropInteraction(interaction, sessionDidUpdate: Session([provider(["next" as NSString])], at: point)).operation, .copy)
        XCTAssertEqual(heard, ["1:55", "1:56", "1:55"])
        other.isHidden = true
    }

    /// A row whose view the pool lends to the next row is another node: the
    /// next row hears its own enter, and the drop, and the retired one
    /// nothing more.
    func testARowWhoseViewIsLentIsAnotherNode() throws {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.onClipboard = { [unowned self] id, kind, line in heard.append("\(id):\(kind)" + (kind == 38 ? ":" + line.split(separator: "\n").last.map(String.init)! : "")) }
        let collections = { (row: Int) -> [String: Any] in
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 50, "totalExtent": 2000,
                "rows": [["view": row, "root": row, "epoch": 1]], "correction": NSNull()]]]
        }
        let row = { (base: Int) -> [[String: Any]] in
            [["op": "create", "id": base, "kind": "view", "handlers": ["drop", "dragenter", "dragleave"]],
             ["op": "create", "id": base + 1, "kind": "text", "props": ["text": "Row \(base)"]],
             ["op": "children", "id": base, "ids": [base + 1]],
             ["op": "frame", "id": base, "x": 0.0, "y": 0.0, "w": 300.0, "h": 60.0],
             ["op": "frame", "id": base + 1, "x": 8.0, "y": 8.0, "w": 200.0, "h": 20.0]]
        }
        p.apply(wireBatch([collections(10), ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]], ["op": "create", "id": 9, "kind": "view"]]
            + row(10) + [["op": "children", "id": 1, "ids": [10]], ["op": "children", "id": 9, "ids": [1]], ["op": "roots", "ids": [9]],
                         ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
                         ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 600.0], ["op": "content", "id": 1, "w": 300.0, "h": 2000.0]]))
        let view = try XCTUnwrap(p.views[10]), drops = try drops(p), point = view.convert(CGPoint(x: 250, y: 40), to: nil)
        let session = Session([provider(["words" as NSString])], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: session)
        p.apply(wireBatch([collections(20), ["op": "destroy", "id": 10], ["op": "destroy", "id": 11]] + row(20) + [["op": "children", "id": 1, "ids": [20]]]))
        XCTAssertTrue(p.views[20] === view, "the same view, lent to the next row")
        drops.dropInteraction(interaction, sessionDidUpdate: session)
        drops.dropInteraction(interaction, performDrop: session)
        hear(3)
        XCTAssertEqual(heard, ["10:55", "20:55", #"20:38:[[],"words",""]"#])
    }

    /// A drag that carries nothing usable is forbidden over a drop.
    func testADragOfNothingUsableIsForbidden() throws {
        let p = presenter(column: ["drop"])
        let drops = try drops(p), point = try XCTUnwrap(p.views[1]).convert(below, to: nil)
        let session = Session([provider([NSURL(fileURLWithPath: "/tmp/a")])], at: point)
        XCTAssertEqual(drops.dropInteraction(interaction, sessionDidUpdate: session).operation, .forbidden)
    }

    /// A node a drop still loading its items was over holds a newer drag's
    /// enter until that drop lands, through the real delivery; a node gone
    /// while its items loaded takes nothing.
    func testANewerDragsEnterWaitsForTheLoadingDropToLand() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drops = try drops(p), point = try XCTUnwrap(p.views[1]).convert(below, to: nil), gate = DispatchSemaphore(value: 0)
        let first = Session([fileProvider(try fixture("a.pdf", "%PDF-1.4"), type: .pdf, gate: gate), provider(["with a file" as NSString])], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: first)
        drops.dropInteraction(interaction, performDrop: first)
        drops.dropInteraction(interaction, sessionDidEnd: first)
        let next = Session([provider(["next" as NSString])], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: next)
        XCTAssertEqual(heard, ["1:55"], "the newer drag's enter is held")
        gate.signal()
        hear(3)
        // No session mints the file here, so the drop carries its text
        XCTAssertEqual(heard, ["1:55", #"1:38:[[],"with a file",""]"#, "1:55"], "the drop, then the held enter")
        heard = []
        drops.dropInteraction(interaction, sessionDidEnd: next)
        let gone = Session([fileProvider(try fixture("b.pdf", "%PDF-1.4"), type: .pdf, gate: gate)], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: gone)
        drops.dropInteraction(interaction, performDrop: gone)
        let drag = try target(p)
        XCTAssertFalse(drag.waiting.isEmpty)
        p.apply(wireBatch([["op": "destroy", "id": 1]]))
        gate.signal()
        until { drag.waiting.isEmpty } // the drop landed
        XCTAssertTrue(drag.waiting.isEmpty)
        XCTAssertFalse(heard.contains { $0.hasPrefix("1:38") }, "\(heard)")
    }

    func testAFieldWhoseNodeDeclaresDropGivesUpItsOwnTextDrop() throws {
        let p = presenter(column: ["drop"], editor: ["input", "drop"])
        let editor = try XCTUnwrap(p.views[2]?.textArea)
        XCTAssertFalse(editor.interactions.contains { $0 is UIDropInteraction }, "every drag over it reaches its node")
        XCTAssertTrue(editor.textDropDelegate === (try target(p)))
        let other = presenter(column: ["drop"])
        let kept = try XCTUnwrap(other.views[2]?.textArea), keeps = try XCTUnwrap(kept.textDropInteraction)
        XCTAssertTrue(kept.interactions.contains { $0 === keeps }, "a field under a drop keeps UIKit's text drop")
        XCTAssertTrue(kept.textDropDelegate === (try target(other)), "and reports where the drag is")
    }

    func testTheAgentsDropReachesTheNodeAndRefusesWhatAPersonCouldNotDrop() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drag = try target(p), column = try XCTUnwrap(p.views[1])
        let at = { (q: CGPoint) in [Double(q.x), Double(q.y)] }
        let reply = drag.agentDrag(["drop": [String](), "text": "Mount Fuji", "url": "https://example.com/fuji", "at": at(below)], over: column)
        XCTAssertEqual(reply["tapped"] as? Int, 1, "\(reply)")
        XCTAssertEqual(heard, ["1:55", #"1:38:[[],"Mount Fuji","https:\/\/example.com\/fuji"]"#])
        heard = []
        let photo = try fixture("photo.heic", "heic")
        let field = drag.agentDrag(["drop": [photo.path], "text": "words", "at": at(overField)], over: column)
        XCTAssertTrue((field["error"] as? String)?.contains("inserts the drag's text and links itself") == true, "a field takes a mixed drag's text: \(field)")
        XCTAssertEqual(heard, ["1:55", "1:56"], "the column hears the drag and its end")
        let folderDrop = drag.agentDrag(["drop": [folder.path], "at": at(below)], over: column)
        XCTAssertTrue((folderDrop["error"] as? String)?.contains("not a file") == true, "\(folderDrop)")
        let missing = drag.agentDrag(["drop": [folder.appendingPathComponent("none.pdf").path], "at": at(below)], over: column)
        XCTAssertTrue((missing["error"] as? String)?.contains("not a file") == true, "\(missing)")
        let covered = drag.agentDrag(["drop": [String](), "text": "words", "at": at(below)], over: try XCTUnwrap(p.views[4]))
        XCTAssertTrue((covered["error"] as? String)?.contains("covered") == true, "a point outside the paragraph reaches something else: \(covered)")
    }

    /// VoiceOver offers a zone's drop point on it and on each element it
    /// focuses inside it, a paragraph, a checkbox or a progress bar; none
    /// while the zone is inert; with no drop above, UIKit's own.
    /// A zone whose middle a nested zone takes offers a point where a drop
    /// reaches the zone itself, and a drop there does.
    func testAZonesDropPointReachesTheZoneBesideANestedOne() throws {
        let p = presenter(column: ["drop"])
        p.apply(wireBatch([["op": "create", "id": 30, "kind": "view", "handlers": ["drop"]],
                           ["op": "children", "id": 1, "ids": [2, 4, 30]],
                           ["op": "frame", "id": 30, "x": 100.0, "y": 100.0, "w": 200.0, "h": 200.0]]))
        let zone = try XCTUnwrap(p.views[1]), descriptor = try XCTUnwrap(zone.accessibilityDropPointDescriptors?.first)
        XCTAssertTrue(descriptor.view === zone)
        let at = zone.convert(descriptor.point, to: nil)
        XCTAssertFalse(try XCTUnwrap(p.views[30]).frame.contains(zone.convert(at, from: nil)), "not in the nested zone: \(descriptor.point)")
        try drops(p).dropInteraction(interaction, performDrop: Session([provider(["words" as NSString])], at: at))
        hear(1)
        XCTAssertEqual(heard, [#"1:38:[[],"words",""]"#])
    }

    func testAZoneAndTheElementsInsideItOfferTheZonesDropPoint() throws {
        let p = presenter(column: ["drop"])
        p.apply(wireBatch([["op": "create", "id": 20, "kind": "control", "props": ["type": "checkbox", "accessibilityLabel": "Seen"]],
                           ["op": "create", "id": 21, "kind": "control", "props": ["type": "progress", "accessibilityRole": "progressbar", "value": "0.5"], "handlers": []],
                           ["op": "children", "id": 1, "ids": [2, 4, 20, 21]],
                           ["op": "frame", "id": 20, "x": 8.0, "y": 340.0, "w": 24.0, "h": 24.0],
                           ["op": "frame", "id": 21, "x": 8.0, "y": 370.0, "w": 160.0, "h": 16.0]]))
        let zone = try XCTUnwrap(p.views[1])
        XCTAssertTrue(zone.accessibilityDropPointDescriptors?.first?.view === zone)
        XCTAssertTrue(try XCTUnwrap(p.views[4]).accessibilityDropPointDescriptors?.first?.view === zone, "the paragraph")
        XCTAssertTrue(try XCTUnwrap(p.controls.controls[20]).accessibilityDropPointDescriptors?.first?.view === zone, "the checkbox")
        let bar = try XCTUnwrap(p.controls.bars[21])
        XCTAssertTrue(bar.accessibilityDropPointDescriptors?.first?.view === zone, "the bar")
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["inert": "true"]]]))
        XCTAssertTrue(zone.inert)
        XCTAssertEqual(bar.accessibilityDropPointDescriptors?.count, 0, "none while the zone is inert")
        let none = presenter(column: [])
        XCTAssertNil(try XCTUnwrap(none.views[4]).accessibilityDropPointDescriptors, "no drop above: UIKit's own")
    }
}

private final class Quiet: NSObject, UIDropInteractionDelegate {}

/// A grouped list's custom row is carried into its cell: a drop on it
/// reaches its section's `drop`, by the authored tree, not the cell's.
final class DragInGroupedIOSTests: GroupedListFixture {
    func testADropOnACarriedRowReachesItsSection() throws {
        let p = presenter { self.model(custom: true) }
        defer { window.isHidden = true }
        var heard: [String] = []
        p.onClipboard = { id, kind, _ in heard.append("\(id):\(kind)") }
        // The fixture's sections hear nothing, and handlers come only with a create
        try XCTUnwrap(p.views[3]).handlers = ["drop"]
        let drag = try XCTUnwrap(p.dragIn as? DropTarget)
        drag.sync()
        let row = try XCTUnwrap(p.views[21])
        _ = try cell(p, 21)
        XCTAssertFalse(row.isDescendant(of: try XCTUnwrap(p.views[3])), "carried out of its section's view")
        XCTAssertEqual(p.authoredNodes(from: row).map(\.id), [21, 5, 3, 1], "its authored ancestors")
        let reply = drag.agentDrag(["drop": [String](), "text": "words"], over: row)
        XCTAssertEqual(reply["tapped"] as? Int, 3, "\(reply)")
        XCTAssertEqual(heard, ["3:38"])
    }
}

/// A drop session at a window point, carrying `providers`.
private final class Session: NSObject, UIDropSession {
    let items: [UIDragItem]
    var point: CGPoint
    init(_ providers: [NSItemProvider], at point: CGPoint) {
        items = providers.map(UIDragItem.init(itemProvider:))
        self.point = point
    }
    func location(in view: UIView) -> CGPoint { view.convert(point, from: nil) }
    var allowsMoveOperation: Bool { false }
    var isRestrictedToDraggingApplication: Bool { false }
    func hasItemsConforming(toTypeIdentifiers typeIdentifiers: [String]) -> Bool { false }
    func canLoadObjects(ofClass aClass: any NSItemProviderReading.Type) -> Bool { false }
    var localDragSession: (any UIDragSession)? { nil }
    var progressIndicatorStyle: UIDropSessionProgressIndicatorStyle = .none
    func loadObjects(ofClass aClass: any NSItemProviderReading.Type, completion: @escaping ([any NSItemProviderReading]) -> Void) -> Progress { Progress() }
    let progress = Progress()
}
#endif
