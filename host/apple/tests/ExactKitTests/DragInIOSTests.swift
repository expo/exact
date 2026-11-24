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
    /// hearing `editor` at its top and, below it, a paragraph (4).
    private func presenter(column: [String], editor: [String] = ["input"]) -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.onClipboard = { [unowned self] id, kind, line in heard.append("\(id):\(kind)" + (kind == 38 ? ":" + line.split(separator: "\n").last.map(String.init)! : "")) }
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
    /// A file representation of `url` as `type`, handed over once `gate` (if any) is signalled.
    private func fileProvider(_ url: URL?, type: UTType, inPlace: Bool = false, gate: DispatchSemaphore? = nil) -> NSItemProvider {
        let provider = NSItemProvider()
        provider.registerFileRepresentation(forTypeIdentifier: type.identifier, fileOptions: inPlace ? [.openInPlace] : [], visibility: .all) { done in
            // A provider in this process loads on the asking thread, so a gated one answers from another
            let answer = { done(url, inPlace, url == nil ? CocoaError(.fileNoSuchFile) : nil) }
            if let gate { DispatchQueue.global().async { gate.wait(); answer() } } else { answer() }
            return nil
        }
        return provider
    }
    /// Waits until `heard` has `count` events, running the main queue.
    private func hear(_ count: Int, timeout: TimeInterval = 10) {
        let deadline = Date().addingTimeInterval(timeout)
        while heard.count < count, Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.02)) }
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
        let providers = [fileProvider(first, type: .pdf, gate: late), fileProvider(nil, type: .pdf), fileProvider(folder, type: .folder), link,
                         fileProvider(second, type: .plainText, inPlace: true)]
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
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) { late.signal() }
        wait(for: [read], timeout: 10)
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
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

    /// The tracker follows one drag: another session's reports are ignored,
    /// and an exit from a view the drag already left lets nothing go.
    func testAnotherSessionAndALateExitLetNothingGo() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drops = try drops(p), drag = try target(p), editor = try XCTUnwrap(p.views[2]?.textArea), point = try XCTUnwrap(p.views[1]).convert(below, to: nil)
        let session = Session([provider(["words" as NSString])], at: point), other = Session([provider(["other" as NSString])], at: point)
        drops.dropInteraction(interaction, sessionDidEnter: session)
        XCTAssertEqual(drops.dropInteraction(interaction, sessionDidUpdate: other).operation, .forbidden, "one drag at a time")
        drops.dropInteraction(interaction, sessionDidEnd: other)
        XCTAssertEqual(heard, ["1:55"], "another session's end lets nothing go")
        // Onto the field, whose text drop reports it, then the window's late exit
        drag.textDroppableView(editor, dropSessionDidEnter: session)
        drops.dropInteraction(interaction, sessionDidExit: session)
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        XCTAssertEqual(heard, ["1:55"], "a late exit")
        drag.textDroppableView(editor, dropSessionDidExit: session)
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        XCTAssertEqual(heard, ["1:55", "1:56"], "the exit from the view that reports it")
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
        p.apply(wireBatch([["op": "destroy", "id": 1]]))
        gate.signal()
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))
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
        let field = drag.agentDrag(["drop": [String](), "text": "words", "at": at(overField)], over: column)
        XCTAssertTrue((field["error"] as? String)?.contains("inserts text and links itself") == true, "\(field)")
        XCTAssertEqual(heard, ["1:55", "1:56"], "the column hears the drag and its end")
        let folderDrop = drag.agentDrag(["drop": [folder.path], "at": at(below)], over: column)
        XCTAssertTrue((folderDrop["error"] as? String)?.contains("not a file") == true, "\(folderDrop)")
        let missing = drag.agentDrag(["drop": [folder.appendingPathComponent("none.pdf").path], "at": at(below)], over: column)
        XCTAssertTrue((missing["error"] as? String)?.contains("not a file") == true, "\(missing)")
        let covered = drag.agentDrag(["drop": [String](), "text": "words", "at": at(below)], over: try XCTUnwrap(p.views[4]))
        XCTAssertTrue((covered["error"] as? String)?.contains("covered") == true, "a point outside the paragraph reaches something else: \(covered)")
    }

    /// VoiceOver offers a zone's drop point on it and on the paragraph it
    /// focuses inside it; with no drop above, UIKit's own.
    func testAZoneAndItsParagraphOfferTheZonesDropPoint() throws {
        let p = presenter(column: ["drop"])
        XCTAssertTrue(try XCTUnwrap(p.views[1]).accessibilityDropPointDescriptors?.first?.view === p.views[1])
        XCTAssertTrue(try XCTUnwrap(p.views[4]).accessibilityDropPointDescriptors?.first?.view === p.views[1], "the paragraph's is its zone's")
        let none = presenter(column: [])
        XCTAssertNil(try XCTUnwrap(none.views[4]).accessibilityDropPointDescriptors, "no drop above: UIKit's own")
    }
}

private final class Quiet: NSObject, UIDropInteractionDelegate {}

/// A drop session at a window point, carrying `providers`.
private final class Session: NSObject, UIDropSession {
    let items: [UIDragItem]
    let point: CGPoint
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
