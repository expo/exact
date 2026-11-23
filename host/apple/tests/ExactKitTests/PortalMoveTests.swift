#if os(macOS) || os(iOS)
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import XCTest
@testable import ExactKit

/// LLP 1013.002 D5 on AppKit and UIKit: a `portal` mover's views move
/// between parents as themselves, its video's player included, in either
/// order the runner's child lists arrive; a hidden mover is made, kept and
/// held outside the window, so nothing sees, focuses or reads it. Events
/// Exact delivers bubble through its declaration parent (D1).
final class PortalMoveTests: XCTestCase {
    #if os(macOS)
    private var window: NSWindow?
    #else
    private var window: UIWindow?
    #endif

    override func tearDown() {
        #if os(macOS)
        window?.close()
        #else
        window?.isHidden = true
        #endif
        window = nil
        super.tearDown()
    }

    private func presenter() -> Presenter {
        let p = Presenter()
        #if os(macOS)
        _ = NSApplication.shared
        p.viewport.frame = NSRect(x: 0, y: 0, width: 400, height: 600)
        let w = NSWindow(contentRect: p.viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.contentView = p.viewport
        #else
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let w = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 600))
        w.frame = CGRect(x: 0, y: 0, width: 400, height: 600)
        p.viewport.frame = w.bounds
        w.addSubview(p.viewport)
        w.makeKeyAndVisible()
        #endif
        window = w
        return p
    }

    /// A root (1) holding a card (2), a detail (3) and the mover's
    /// declaration parent (7), and a mover (4) with a video (5) and a button
    /// (6) that no list places yet: hidden, as the runner makes one whose
    /// destination is missing.
    private func boot(_ p: Presenter) {
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "handlers": ["focusin", "dblclick", "pointerdown"]],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 4, "kind": "view", "props": ["declaredParent": "7"]],
            ["op": "create", "id": 5, "kind": "video", "props": ["muted": "true", "playsinline": "true"]],
            ["op": "create", "id": 6, "kind": "button", "props": ["accessibilityLabel": "Count"]],
            ["op": "create", "id": 7, "kind": "view", "handlers": ["focusin", "pointerdown"]],
            ["op": "children", "id": 1, "ids": [2, 3, 7]],
            ["op": "children", "id": 4, "ids": [5, 6]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 600],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 200, "h": 150],
            ["op": "frame", "id": 3, "x": 0, "y": 200, "w": 400, "h": 300],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 200, "h": 150],
            ["op": "frame", "id": 5, "x": 0, "y": 0, "w": 200, "h": 120],
            ["op": "frame", "id": 6, "x": 0, "y": 120, "w": 80, "h": 30],
        ]))
    }

    private func shown(_ p: Presenter, in parent: UInt32) -> Bool {
        guard let mover = p.views[4], let holder = p.views[parent] else { return false }
        return mover.superview === holder.container && mover.window != nil
    }

    func testAMoverKeepsItsViewsAndPlayerThroughMovesInEitherOrder() throws {
        let p = presenter()
        defer { p.reset() }
        boot(p)
        let mover = try XCTUnwrap(p.views[4], "a hidden mover is made")
        let video = try XCTUnwrap(p.views[5])
        let player = try XCTUnwrap(video.video as AnyObject?)
        let button = try XCTUnwrap(p.views[6])
        XCTAssertNil(mover.superview, "a hidden mover stands in no parent")
        XCTAssertNil(button.window, "nothing in the window reaches a hidden mover")

        // Shown in the card, then moved: the destination's list first, then
        // the source's; then the source's first; then hidden and shown again.
        let steps: [(ops: [[String: Any]], at: UInt32?)] = [
            ([["op": "children", "id": 2, "ids": [4]]], 2),
            ([["op": "children", "id": 3, "ids": [4]], ["op": "children", "id": 2, "ids": []]], 3),
            ([["op": "children", "id": 3, "ids": []], ["op": "children", "id": 2, "ids": [4]]], 2),
            ([["op": "children", "id": 2, "ids": []]], nil),
            ([["op": "children", "id": 3, "ids": [4]]], 3),
        ]
        for (i, step) in steps.enumerated() {
            p.apply(wireBatch(step.ops))
            XCTAssertTrue(p.views[4] === mover && p.views[5] === video && p.views[6] === button, "step \(i): the same views")
            XCTAssertTrue(p.views[5]?.video as AnyObject? === player, "step \(i): the same player")
            XCTAssertTrue(button.superview === mover.container, "step \(i): the mover keeps its children")
            if let at = step.at {
                XCTAssertTrue(shown(p, in: at), "step \(i): shown in \(at)")
                XCTAssertFalse(shown(p, in: at == 2 ? 3 : 2), "step \(i): shown once")
            } else {
                XCTAssertNil(mover.superview, "step \(i): hidden")
                XCTAssertNil(button.window, "step \(i): a hidden mover's button is out of the window")
            }
        }
    }

    func testEventsBubbleThroughTheDeclarationNotTheCard() throws {
        let p = presenter()
        defer { p.reset() }
        boot(p)
        p.apply(wireBatch([["op": "children", "id": 2, "ids": [4]]]))
        XCTAssertTrue(shown(p, in: 2))
        let button = try XCTUnwrap(p.views[6])
        var heard: [String] = []
        p.onFocusEvent = { at, kind, _, _ in heard.append("\(at):\(kind)") }
        p.focus(6)
        XCTAssertEqual(heard, ["7:focusin"], "focusin reaches the declaration parent, not the card")
        #if os(macOS)
        XCTAssertTrue(button.pointerdownHeard, "the declaration parent's pointerdown hears a press")
        XCTAssertNil(button.dblclickListener, "the card's dblclick does not")
        #else
        XCTAssertEqual(PointerRecognizer.bubbling(from: button).map(\.id), [7])
        // A touch falls in the card's views, not the declaration's: the
        // mover's relay gives it to the declaration, and the card's own
        // recognizer leaves it.
        XCTAssertTrue(p.views[4]?.portalRelay?.listener(button) { $0.wantsPointer } === p.views[7])
        let card = try XCTUnwrap(p.views[2]?.gestureRecognizers?.first { $0 is PointerRecognizer } as? PointerRecognizer)
        XCTAssertTrue(card.nearer(button))
        #endif
    }

    /// D5: a focused field moved with its mover keeps the focus, and the
    /// page hears no blur, focus, focusout or focusin for it; hidden, it
    /// loses the focus once.
    func testAFocusedFieldMovedWithItsMoverKeepsTheFocusSilently() throws {
        let p = presenter()
        defer { p.reset() }
        boot(p)
        p.apply(wireBatch([
            ["op": "create", "id": 8, "kind": "input", "props": ["id": "f"], "handlers": ["focus", "blur"]],
            ["op": "frame", "id": 8, "x": 100, "y": 120, "w": 100, "h": 30],
            ["op": "children", "id": 4, "ids": [5, 6, 8]],
            ["op": "children", "id": 2, "ids": [4]],
        ]))
        let field = try XCTUnwrap(p.views[8]?.field)
        #if os(macOS)
        XCTAssertTrue(try XCTUnwrap(window).makeFirstResponder(field))
        let focused = { field.currentEditor() != nil }
        #else
        XCTAssertTrue(field.becomeFirstResponder())
        let focused = { field.isFirstResponder }
        #endif
        waitForMainQueue()
        var heard: [String] = []
        p.onFocus = { heard.append("\($0):focus") }
        p.onBlur = { heard.append("\($0):blur") }
        p.onFocusEvent = { at, kind, _, _ in heard.append("\(at):\(kind)") }
        for (destination, source) in [(3, 2), (2, 3)] as [(UInt32, UInt32)] {
            p.apply(wireBatch([["op": "children", "id": destination, "ids": [4]], ["op": "children", "id": source, "ids": []]]))
            waitForMainQueue()
            XCTAssertTrue(shown(p, in: destination))
            XCTAssertTrue(focused(), "the field keeps the focus in \(destination)")
        }
        XCTAssertEqual(heard, [], "a move delivers no focus events")
        p.apply(wireBatch([["op": "children", "id": 2, "ids": []]]))
        waitForMainQueue()
        XCTAssertFalse(focused(), "a hidden mover's field gives the focus up")
        XCTAssertEqual(heard, ["8:blur"], "hiding it is one loss")
    }
}
#endif
