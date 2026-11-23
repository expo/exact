#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A text control's `overflow` is its editor's: a textarea's text view and an
/// input's field scroll their own text, as a web `<textarea>` does, so the
/// node makes no scroll view of its own over them, and a click on the field
/// lands in the editor.
final class FieldOverflowMacTests: XCTestCase {
    private var window: NSWindow!
    override func tearDown() { window?.close(); window = nil }

    func testAClickOnAScrollingTextControlLandsInItsEditor() throws {
        _ = NSApplication.shared
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        // CSS computes `overflow-x` to `auto` beside `overflow-y: auto`.
        let scrolls: [String: Any] = ["overflow_x": "auto", "overflow_y": "auto", "written": ["overflow_y"]]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "textarea", "props": ["value": ""], "handlers": ["input"], "style": scrolls],
            ["op": "create", "id": 3, "kind": "input", "props": ["value": ""], "handlers": ["input"], "style": scrolls],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 300.0],
            ["op": "frame", "id": 2, "x": 20.0, "y": 20.0, "w": 240.0, "h": 66.0],
            ["op": "frame", "id": 3, "x": 20.0, "y": 120.0, "w": 240.0, "h": 24.0],
        ]))
        p.viewport.layoutSubtreeIfNeeded()
        let area = try XCTUnwrap(p.views[2]), input = try XCTUnwrap(p.views[3])
        let editors: [(NodeView, NSView?)] = [(area, area.textArea), (input, input.field)]
        for (node, editor) in editors {
            let editor = try XCTUnwrap(editor, node.kind)
            XCTAssertNil(node.scroll, "\(node.kind): the editor scrolls its text; the node makes no scroll view over it")
            let frame = try XCTUnwrap(window.contentView?.superview)
            let middle = frame.convert(NSPoint(x: node.bounds.midX, y: node.bounds.midY), from: node)
            let hit = try XCTUnwrap(frame.hitTest(middle), node.kind)
            XCTAssertTrue(hit === editor || hit.isDescendant(of: editor), "\(node.kind): the click lands on \(hit), outside its editor")
        }
    }
}
#endif
