#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// LLP 1115.000 D7.9: the I-beam shows over text a drag would select, as over
/// an `NSTextView`, and nowhere else: not over a control's label, the
/// chrome's, text the author made unselectable, a link, or anything with a
/// cursor of its own. It comes from the page's one tracking area: a
/// paragraph gets no tracking area and no cursor rect for it.
final class TextCursorMacTests: XCTestCase {
    /// A window that says whether it is key: a test's window is key only if
    /// a person made it so, and the cursor is the key window's.
    private final class Window: NSWindow {
        var key = true
        var invalidatedCursorViews: [ObjectIdentifier] = []
        override var isKeyWindow: Bool { key }
        override func invalidateCursorRects(for aView: NSView) {
            invalidatedCursorViews.append(ObjectIdentifier(aView))
            super.invalidateCursorRects(for: aView)
        }
    }
    private var window: Window!
    private var session: ExactSession!
    override func tearDown() { window?.close(); window = nil; session?.destroy(); session = nil; NSCursor.arrow.set() }

    private func fixture() -> Presenter {
        _ = NSApplication.shared
        session = ExactApp.shared.makeSession(label: "text-cursor")
        let p = session.presenter
        window = Window(contentRect: NSRect(x: 0, y: 0, width: 400, height: 360), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        func text(_ id: Int, _ value: String) -> [String: Any] { ["op": "create", "id": id, "kind": "text", "props": ["text": value], "style": ["font_size": 16.0, "line_height": "20px"]] }
        func frame(_ id: Int, _ y: Int, x: Int = 10, h: Int = 20) -> [String: Any] { ["op": "frame", "id": id, "x": x, "y": y, "w": 300, "h": h] }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            text(2, "Plain content a drag selects"),
            ["op": "create", "id": 3, "kind": "button", "handlers": ["press"]], text(4, "A button's label"),
            ["op": "create", "id": 5, "kind": "view", "handlers": ["press"]], text(6, "A pressable row's words"),
            ["op": "create", "id": 7, "kind": "view", "style": ["cursor": "grab"]], text(8, "Under an authored cursor"),
            ["op": "create", "id": 9, "kind": "view", "props": ["semanticTag": "header"]], text(10, "Window chrome"),
            ["op": "create", "id": 11, "kind": "view", "style": ["user_select": "none"]], text(12, "Opted out"),
            ["op": "create", "id": 13, "kind": "text", "style": ["font_size": 16.0, "line_height": "20px"]],
            ["op": "paragraph", "id": 13, "runs": [
                ["id": 14, "parent": 13, "paint": true, "props": ["text": "Words and then "], "style": ["font_size": 16.0]],
                ["id": 15, "parent": 13, "paint": true, "props": ["text": "a link", "href": "/there"], "style": ["font_size": 16.0]],
            ]],
            ["op": "create", "id": 16, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2, 3, 5, 7, 9, 11, 13, 16]],
            ["op": "children", "id": 3, "ids": [4]], ["op": "children", "id": 5, "ids": [6]], ["op": "children", "id": 7, "ids": [8]],
            ["op": "children", "id": 9, "ids": [10]], ["op": "children", "id": 11, "ids": [12]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 360],
            frame(2, 10), frame(3, 40, h: 30), frame(4, 5, x: 0), frame(5, 80), frame(6, 0, x: 0), frame(7, 110), frame(8, 0, x: 0),
            frame(9, 140), frame(10, 0, x: 0), frame(11, 170), frame(12, 0, x: 0), frame(13, 200), frame(16, 240, h: 100),
        ]))
        p.viewport.layoutSubtreeIfNeeded()
        return p
    }

    /// The pointer moves to a point of a node, as the page's tracking area reports it.
    private func move(_ p: Presenter, to node: NSView, at local: NSPoint? = nil, exits: Bool = false) {
        let point = node.convert(local ?? NSPoint(x: 20, y: node.bounds.midY), to: nil), now = ProcessInfo.processInfo.systemUptime
        let event = exits
            ? NSEvent.enterExitEvent(with: .mouseExited, location: point, modifierFlags: [], timestamp: now, windowNumber: window.windowNumber,
                                     context: nil, eventNumber: 0, trackingNumber: 0, userData: nil)
            : NSEvent.mouseEvent(with: .mouseMoved, location: point, modifierFlags: [], timestamp: now, windowNumber: window.windowNumber,
                                 context: nil, eventNumber: 0, clickCount: 0, pressure: 0)
        p.trackPointer(event!)
    }

    func testTheIBeamShowsOverTextADragSelectsAndNowhereElse() throws {
        let p = fixture()
        let content = try XCTUnwrap(p.views[2])
        XCTAssertFalse(p.pointerOverText)
        move(p, to: content)
        XCTAssertTrue(p.pointerOverText, "content text")
        XCTAssertTrue(NSCursor.current === NSCursor.iBeam, "the I-beam, as over an NSTextView")
        // The blank end of its line is still the text view's.
        move(p, to: content, at: NSPoint(x: 290, y: 10))
        XCTAssertTrue(p.pointerOverText)
        for (id, what) in [(4, "a button's label"), (6, "a pressable row's words"), (10, "the chrome's label"), (12, "text the author made unselectable"), (16, "an empty box")] {
            move(p, to: content)
            XCTAssertTrue(NSCursor.current === NSCursor.iBeam)
            move(p, to: try XCTUnwrap(p.views[UInt32(id)]))
            XCTAssertFalse(p.pointerOverText, what)
            XCTAssertTrue(NSCursor.current === NSCursor.arrow, "the arrow comes back over \(what)")
        }
        // An authored cursor keeps its node's text, and is what comes back.
        move(p, to: content)
        move(p, to: try XCTUnwrap(p.views[8]))
        XCTAssertFalse(p.pointerOverText, "text under an authored cursor")
        XCTAssertTrue(NSCursor.current === NSCursor.openHand, "the author's cursor: \(NSCursor.current)")
        // Leaving the page: the tracking area's exit, outside the viewport.
        move(p, to: content)
        move(p, to: p.viewport, at: NSPoint(x: -20, y: 10), exits: true)
        XCTAssertFalse(p.pointerOverText)
        XCTAssertTrue(NSCursor.current === NSCursor.arrow)
    }

    func testALinkKeepsItsPointingHandAndTheWordsBesideItTheIBeam() throws {
        let p = fixture()
        let paragraph = try XCTUnwrap(p.views[13])
        let link = try XCTUnwrap(paragraph.inlineRects(try XCTUnwrap(p.inlineText(15))).first)
        move(p, to: paragraph, at: NSPoint(x: 8, y: 10))
        XCTAssertTrue(p.pointerOverText, "the paragraph's own words")
        XCTAssertTrue(NSCursor.current === NSCursor.iBeam)
        move(p, to: paragraph, at: NSPoint(x: link.midX, y: link.midY))
        XCTAssertFalse(p.pointerOverText, "its link run is a link")
        XCTAssertTrue(NSCursor.current === NSCursor.pointingHand, "which shows AppKit's pointing hand")
        XCTAssertTrue(paragraph.cursorRect(at: paragraph.convert(NSPoint(x: link.midX, y: link.midY), to: nil)) === NSCursor.pointingHand)
    }

    /// One tracking area, the page's: a paragraph has none of its own, so a
    /// long page's scroll rebuilds nothing for the cursor. And the cursor is
    /// the key window's, as cursor rects are: elsewhere it is left alone.
    func testTheCursorIsThePagesAndTheKeyWindows() throws {
        let p = fixture()
        let content = try XCTUnwrap(p.views[2])
        p.viewport.updateTrackingAreas()
        XCTAssertTrue(content.trackingAreas.isEmpty, "no tracking area on a paragraph")
        XCTAssertFalse(p.viewport.trackingAreas.isEmpty, "the page has its own")
        window.key = false
        NSCursor.arrow.set()
        move(p, to: content)
        XCTAssertTrue(p.pointerOverText, "the answer is kept")
        XCTAssertTrue(NSCursor.current === NSCursor.arrow, "but a window that is not key sets no cursor")
        window.key = true
        move(p, to: content, at: NSPoint(x: 30, y: 10))
        XCTAssertTrue(NSCursor.current === NSCursor.iBeam)
        // A cursor something else set meanwhile is not this presenter's to take away.
        NSCursor.crosshair.set()
        move(p, to: try XCTUnwrap(p.views[16]))
        XCTAssertTrue(NSCursor.current === NSCursor.crosshair)
    }

    func testPaddingContentBoxChangeRefreshesInlineLinkCursorRectWithoutParagraphOp() throws {
        let p = fixture()
        let paragraph = try XCTUnwrap(p.views[13])
        let plain = try XCTUnwrap(p.views[2])
        let run = try XCTUnwrap(p.inlineText(15))
        let before = try XCTUnwrap(paragraph.inlineRects(run).first)
        window.invalidatedCursorViews.removeAll()

        // A padding edit shifts the content box by 40 pt without re-emitting a paragraph op.
        p.apply(wireBatch([
            ["op": "fieldContent", "id": 13, "rect": [40.0, 0.0, 260.0, 20.0], "padding": [0.0, 0.0, 0.0, 40.0]],
        ]))
        XCTAssertEqual(window.invalidatedCursorViews.filter { $0 == ObjectIdentifier(paragraph) }.count, 1,
                       "shifting an inline link's content box invalidates cursor rects once after the batch lands")
        let after = try XCTUnwrap(paragraph.inlineRects(run).first)
        XCTAssertEqual(after.minX - before.minX, 40.0, accuracy: 0.5)
        XCTAssertNil(paragraph.cursorRect(at: paragraph.convert(NSPoint(x: before.minX + 2, y: before.midY), to: nil)),
                     "the old link position no longer takes a pointing hand")
        XCTAssertTrue(paragraph.cursorRect(at: paragraph.convert(NSPoint(x: after.midX, y: after.midY), to: nil)) === NSCursor.pointingHand,
                      "the shifted link position takes the pointing hand")

        // A geometry-only textLeft prop edit shifts inline link geometry without a paragraph op.
        window.invalidatedCursorViews.removeAll()
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["textLeft": "20"]]]))
        XCTAssertEqual(window.invalidatedCursorViews.filter { $0 == ObjectIdentifier(paragraph) }.count, 1)
        let shifted = try XCTUnwrap(paragraph.inlineRects(run).first)
        XCTAssertEqual(shifted.minX - after.minX, 20.0, accuracy: 0.5)

        // Flow and column fragment ops invalidate inline-link cursor rects; a same-width
        // content-box origin shift with an active flow shape recomputes the translated flow layout.
        window.invalidatedCursorViews.removeAll()
        p.apply(wireBatch([["op": "flow", "id": 13, "shapes": [["kind": "rect", "x": 40.0, "y": 0.0, "w": 60.0, "h": 20.0]]]]))
        XCTAssertEqual(window.invalidatedCursorViews.filter { $0 == ObjectIdentifier(paragraph) }.count, 1)
        window.invalidatedCursorViews.removeAll()
        p.apply(wireBatch([["op": "fieldContent", "id": 13, "rect": [60.0, 0.0, 260.0, 20.0], "padding": [0.0, 0.0, 0.0, 60.0]]]))
        XCTAssertEqual(window.invalidatedCursorViews.filter { $0 == ObjectIdentifier(paragraph) }.count, 1)
        window.invalidatedCursorViews.removeAll()
        p.apply(wireBatch([["op": "fragments", "id": 13, "fragments": [[0.0, 0.0, 150.0, 20.0, 0.0, 1.0, 0.0, 0.0]]]]))
        XCTAssertEqual(window.invalidatedCursorViews.filter { $0 == ObjectIdentifier(paragraph) }.count, 1)

        // Moving the view's frame origin alone and editing non-link labels do not churn cursor rects.
        window.invalidatedCursorViews.removeAll()
        p.apply(wireBatch([
            ["op": "frame", "id": 13, "x": 30, "y": 200, "w": 300, "h": 20],
            ["op": "fieldContent", "id": 2, "rect": [20.0, 0.0, 280.0, 20.0], "padding": [0.0, 0.0, 0.0, 20.0]],
        ]))
        XCTAssertFalse(window.invalidatedCursorViews.contains(ObjectIdentifier(paragraph)))
        XCTAssertFalse(window.invalidatedCursorViews.contains(ObjectIdentifier(plain)))

        // Removing the last inline link still resets cursor rects so the stale pointing hand clears.
        window.invalidatedCursorViews.removeAll()
        p.apply(wireBatch([["op": "paragraph", "id": 13, "runs": [
            ["id": 14, "parent": 13, "paint": true, "props": ["text": "Plain words only"], "style": ["font_size": 16.0]],
        ]]]))
        XCTAssertEqual(window.invalidatedCursorViews.filter { $0 == ObjectIdentifier(paragraph) }.count, 1)
    }

    func testSwitchingPositionToAbsoluteUnderUnderlineInvalidatesParagraphDecoration() throws {
        let p = fixture()
        p.apply(wireBatch([
            ["op": "create", "id": 20, "kind": "view", "style": ["text_decoration_line": "underline"]],
            ["op": "create", "id": 21, "kind": "view", "style": ["position_type": "relative"]],
            ["op": "create", "id": 22, "kind": "text", "props": ["text": "Underlined"], "style": ["position_type": "relative", "font_size": 16.0]],
            ["op": "children", "id": 20, "ids": [21]],
            ["op": "children", "id": 21, "ids": [22]],
            ["op": "children", "id": 1, "ids": [2, 3, 5, 7, 9, 11, 13, 16, 20]],
            ["op": "frame", "id": 20, "x": 0, "y": 240, "w": 200, "h": 40],
            ["op": "frame", "id": 21, "x": 0, "y": 0, "w": 200, "h": 40],
            ["op": "frame", "id": 22, "x": 0, "y": 0, "w": 200, "h": 20],
        ]))
        let label = try XCTUnwrap(p.views[22])
        XCTAssertNotNil(label.paragraphSpec().runs.first?.underlineColor)

        // Switching the paragraph to absolute without a paragraph op drops the inherited underline.
        p.apply(wireBatch([["op": "style", "id": 22, "style": ["position_type": "absolute", "font_size": 16.0]]]))
        XCTAssertNil(label.paragraphSpec().runs.first?.underlineColor)

        // Restoring relative brings it back; switching the intermediate container to absolute or popover drops it again.
        p.apply(wireBatch([["op": "style", "id": 22, "style": ["position_type": "relative", "font_size": 16.0]]]))
        XCTAssertNotNil(label.paragraphSpec().runs.first?.underlineColor)
        p.apply(wireBatch([["op": "style", "id": 21, "style": ["position_type": "absolute"]]]))
        XCTAssertNil(label.paragraphSpec().runs.first?.underlineColor)
        p.apply(wireBatch([["op": "style", "id": 21, "style": ["position_type": "relative"]]]))
        XCTAssertNotNil(label.paragraphSpec().runs.first?.underlineColor)
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["popover": "auto"]]]))
        XCTAssertNil(label.paragraphSpec().runs.first?.underlineColor)
        p.apply(wireBatch([["op": "props", "id": 21, "clear": ["popover"]]]))
        XCTAssertNotNil(label.paragraphSpec().runs.first?.underlineColor)
    }
}
#endif
