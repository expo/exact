#if os(iOS)
import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import ExactDragIn
@testable import ExactKit

/// LLP 1094.001 D4 on UIKit: what a dropped item is, read from its own types
/// and the system's type tree; its files copied out and its text and link
/// read apart; a field that gives every drag to its own node's `drop`; and
/// the tracker the agent's drop goes through. UIKit makes a drop session only
/// in a real drag, so a drop still loading its items is stood in for
/// (`waits`). A simulator runs it:
/// bun host/apple/build.mjs --test --ios --only DragInIOSTests
final class DragInIOSTests: XCTestCase {
    private var window: UIWindow!
    private var heard: [String] = []
    private var file: URL!

    override func setUpWithError() throws {
        file = FileManager.default.temporaryDirectory.appendingPathComponent("drop-\(UUID().uuidString).pdf")
        try Data("%PDF-1.4".utf8).write(to: file)
    }
    override func tearDown() {
        window = nil
        try? FileManager.default.removeItem(at: file)
    }

    /// A column (1) hearing `column` on the page (9), holding a textarea (2)
    /// hearing `editor` at its top and, below it, the column's own box.
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
            ["op": "children", "id": 9, "ids": [1]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [9]],
            ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 300.0, "h": 80.0],
        ]))
        return p
    }
    /// Below the textarea, in the column's own box
    private let below: [Double] = [150, 200]
    /// Over the textarea
    private let overField: [Double] = [50, 40]
    private func target(_ p: Presenter) throws -> DropTarget { try XCTUnwrap(p.dragIn as? DropTarget) }

    private func provider(_ objects: [NSItemProviderWriting]) -> NSItemProvider {
        let provider = NSItemProvider()
        objects.forEach { provider.registerObject($0, visibility: .all) }
        return provider
    }
    private func fileProvider(_ url: URL, type: UTType, inPlace: Bool = false) -> NSItemProvider {
        let provider = NSItemProvider()
        provider.registerFileRepresentation(forTypeIdentifier: type.identifier, fileOptions: inPlace ? [.openInPlace] : [], visibility: .all) { done in
            done(url, inPlace, nil)
            return nil
        }
        return provider
    }

    func testAnItemIsWhatItsOwnTypesSay() {
        let link = provider([NSURL(string: "https://example.com/fuji")!, "Mount Fuji" as NSString])
        XCTAssertEqual(DropItems.kind(of: link), .words(url: true, text: true), "a link card gives its link and its title")
        XCTAssertEqual(DropItems.kind(of: provider([NSURL(string: "https://example.com/")!])), .words(url: true, text: false))
        XCTAssertEqual(DropItems.kind(of: provider(["words" as NSString])), .words(url: false, text: true))
        let rich = NSAttributedString(string: "Bold", attributes: [.font: UIFont.boldSystemFont(ofSize: 17)])
        XCTAssertEqual(DropItems.kind(of: provider([rich])), .words(url: false, text: true), "rich text gives its plain-text form")
        let image = UIGraphicsImageRenderer(size: CGSize(width: 4, height: 4)).image { c in UIColor.red.setFill(); c.fill(CGRect(x: 0, y: 0, width: 4, height: 4)) }
        guard case .file(let picture) = DropItems.kind(of: provider([image])) else { return XCTFail("a picture is a file") }
        XCTAssertTrue(UTType(picture)?.conforms(to: .image) == true, "by its first declared type: \(picture)")
        XCTAssertEqual(DropItems.kind(of: fileProvider(file, type: .pdf)), .file(UTType.pdf.identifier))
        let zip = NSItemProvider()
        zip.registerDataRepresentation(forTypeIdentifier: UTType.zip.identifier, visibility: .all) { done in done(Data(), nil); return nil }
        XCTAssertEqual(DropItems.kind(of: zip), .file(UTType.zip.identifier), "a type given as data")
        XCTAssertEqual(DropItems.kind(of: fileProvider(file, type: .plainText, inPlace: true)), .file(UTType.plainText.identifier), "a text file offered in place")
        XCTAssertEqual(DropItems.kind(of: fileProvider(file, type: .plainText)), .words(url: false, text: true), "a text file not offered in place, as a text view reads it")
        let mail = provider([NSURL(string: "mailto:a@example.com")!])
        mail.registerFileRepresentation(forTypeIdentifier: UTType.jpeg.identifier, fileOptions: [], visibility: .all) { done in done(nil, false, nil); return nil }
        XCTAssertEqual(DropItems.kind(of: mail), .words(url: true, text: false), "a non-web link beside a file gives its link")
        XCTAssertEqual(DropItems.kind(of: provider([NSURL(fileURLWithPath: file.path)])), .none, "a bare file URL")
    }

    func testAnItemsFileIsCopiedOutAndALinksTextReadApart() throws {
        let link = provider([NSURL(string: "https://example.com/fuji")!, "Mount Fuji" as NSString])
        let read = expectation(description: "read")
        var minted: [URL] = []
        DropItems.read([fileProvider(file, type: .pdf), link], for: nil, mint: { minted.append($0); return "doc:/1/\($0.lastPathComponent)" }) { files, text, url in
            XCTAssertEqual(files, ["doc:/1/\(self.file.lastPathComponent)"])
            XCTAssertEqual(text, "Mount Fuji")
            XCTAssertEqual(url, "https://example.com/fuji")
            read.fulfill()
        }
        wait(for: [read], timeout: 10)
        let copy = try XCTUnwrap(minted.first)
        XCTAssertNotEqual(copy, file, "a copy of its own")
        XCTAssertEqual(try Data(contentsOf: copy), Data("%PDF-1.4".utf8))
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

    func testTheAgentsDropReachesTheNodeAndAFieldKeepsItsText() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drops = try target(p), column = try XCTUnwrap(p.views[1])
        let reply = drops.agentDrag(["drop": [String](), "text": "Mount Fuji", "url": "https://example.com/fuji", "at": below], over: column)
        XCTAssertEqual(reply["tapped"] as? Int, 1, "\(reply)")
        XCTAssertEqual(heard, ["1:55", #"1:38:[[],"Mount Fuji","https:\/\/example.com\/fuji"]"#])
        heard = []
        let field = drops.agentDrag(["drop": [String](), "text": "words", "at": overField], over: column)
        XCTAssertTrue((field["error"] as? String)?.contains("inserts text and links itself") == true, "\(field)")
        XCTAssertEqual(heard, ["1:55", "1:56"], "the column hears the drag and its end")
        heard = []
        _ = drops.agentDrag(["drop": [file.path], "at": overField], over: column)
        XCTAssertEqual(heard.first, "1:55", "a file over the field is the column's")
    }

    /// A node a drop still loading its items was over holds a newer drag's
    /// enter until that drop lands; the newer drag's leave cancels it.
    func testANewerDragsEnterAtAWaitingDropsNodeWaitsForItToLand() throws {
        let p = presenter(column: ["drop", "dragenter", "dragleave"])
        let drops = try target(p), column = try XCTUnwrap(p.views[1])
        drops.waits([column])
        _ = drops.agentDrag(["dragenter": [String](), "text": "next", "at": below], over: column)
        XCTAssertEqual(heard, [], "held")
        drops.lands([column])
        XCTAssertEqual(heard, ["1:55"])
        _ = drops.agentDrag(["dragleave": true], over: column)
        heard = []
        drops.waits([column])
        _ = drops.agentDrag(["dragenter": [String](), "text": "next", "at": below], over: column)
        _ = drops.agentDrag(["dragleave": true], over: column)
        drops.lands([column])
        XCTAssertEqual(heard, [], "a newer drag that left was neither entered nor left")
    }

    /// A node with `drop` has one drop interaction of the tracker's, and
    /// VoiceOver offers it as a drop point, named by its label.
    func testANodeWithDropIsADropPoint() throws {
        let p = presenter(column: ["drop"])
        let column = try XCTUnwrap(p.views[1]), drops = try target(p)
        let ours = { (v: UIView) in v.interactions.filter { ($0 as? UIDropInteraction)?.delegate === drops }.count }
        XCTAssertEqual(ours(column), 1)
        XCTAssertEqual(ours(try XCTUnwrap(p.views[2])), 0, "a node that hears no drag has none")
        XCTAssertEqual(column.accessibilityDropPointDescriptors?.count, 1)
        XCTAssertNil(try XCTUnwrap(p.views[2]).accessibilityDropPointDescriptors, "a node with no drop is none")
    }
}
#endif
