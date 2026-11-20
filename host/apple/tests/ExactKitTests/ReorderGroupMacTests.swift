#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactCollections

/// A grouped grip on a Mac (LLP 1094 stage 4): a ghost that follows a drag
/// in any direction into another grouped list, a drop that holds with the
/// ghost kept, the hold's end landing it, and the keys.
final class ReorderGroupMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactCollections.install() } // LLP 1047.001 D10
    private var window: NSWindow?
    override func tearDown() { window?.close(); window = nil }

    /// Two grouped lists side by side: 1 (row wrapper 2, grip 3, text 4)
    /// and 5 (row wrapper 6, grip 7).
    private func fixture(_ calls: ReorderGroupRecorder) -> (Presenter, NodeView) {
        _ = NSApplication.shared
        let p = Presenter()
        p.viewport.frame = NSRect(x: 0, y: 0, width: 400, height: 300)
        p.reorderHost!.groupCalls = calls
        let entry = { (list: Int, wrapper: Int, root: Int) -> [String: Any] in
            ["view": list, "revision": 1, "scrollSequence": 0, "count": 1, "totalExtent": 40,
             "rows": [["view": wrapper, "root": root, "epoch": 7]], "correction": NSNull()]
        }
        p.apply(wireBatch([
            ["op": "collections", "items": [entry(1, 2, 3), entry(5, 6, 7)]],
            ["op": "create", "id": 9, "kind": "view"],
            ["op": "create", "id": 1, "kind": "list", "props": ["id": "col-a", "reorderGroup": "cards"], "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "props": ["reorderFor": "col-a"]],
            ["op": "create", "id": 4, "kind": "text", "props": ["text": "grip"]],
            ["op": "create", "id": 5, "kind": "list", "props": ["id": "col-b", "reorderGroup": "cards"], "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 6, "kind": "view"],
            ["op": "create", "id": 7, "kind": "view", "props": ["reorderFor": "col-b"]],
            ["op": "children", "id": 9, "ids": [1, 5]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 5, "ids": [6]],
            ["op": "children", "id": 6, "ids": [7]],
            ["op": "roots", "ids": [9]],
            ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 400.0, "h": 200.0],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 150.0, "h": 200.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 150.0, "h": 40.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 150.0, "h": 40.0],
            ["op": "frame", "id": 4, "x": 10.0, "y": 10.0, "w": 100.0, "h": 20.0],
            ["op": "frame", "id": 5, "x": 200.0, "y": 0.0, "w": 150.0, "h": 200.0],
            ["op": "frame", "id": 6, "x": 0.0, "y": 0.0, "w": 150.0, "h": 40.0],
            ["op": "frame", "id": 7, "x": 0.0, "y": 0.0, "w": 150.0, "h": 40.0],
            ["op": "content", "id": 1, "w": 150.0, "h": 200.0],
            ["op": "content", "id": 5, "w": 150.0, "h": 200.0]
        ]))
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        self.window = window
        return (p, p.views[4]!)
    }
    private func event(_ type: NSEvent.EventType, _ view: NSView, down: CGFloat, right: CGFloat = 0) -> NSEvent {
        let at = view.convert(NSPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
        return NSEvent.mouseEvent(with: type, location: NSPoint(x: at.x + right, y: at.y - down), modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window!.windowNumber, context: nil,
            eventNumber: 1, clickCount: 1, pressure: 1)!
    }

    func testThePointerChoosesTheDestinationWhenTheGhostStillCoversTheSource() throws {
        let calls = ReorderGroupRecorder()
        let (presenter, _) = fixture(calls)
        let grip = try XCTUnwrap(presenter.views[3])
        let destination = try XCTUnwrap(presenter.views[5])
        let start = grip.convert(NSPoint(x: grip.bounds.maxX - 10, y: grip.bounds.midY), to: nil)
        let point = destination.convert(NSPoint(x: 5, y: destination.bounds.midY), to: nil)
        let hold = try XCTUnwrap(ReorderGroupHold(grip, point: start, ghost: true))
        XCTAssertTrue(hold.move(point))
        let ghost = try XCTUnwrap(hold.ghost)
        XCTAssertLessThan(ghost.frame.midX, destination.convert(destination.bounds, to: nil).minX)
        XCTAssertEqual(calls.log.last, "into 5")
        hold.cancel()
    }

    func testStateReadsOnlyAGroupedOp() {
        let op: [String: Any] = ["op": "reorder", "group": true, "token": "18446744073709551615", "list": 1, "wrapper": 2,
            "phase": "settling", "ending": "timeout", "target": 5, "row": 6, "dispatched": true]
        let state = ReorderGroupState(op)
        XCTAssertEqual(state?.token, UInt64.max)
        XCTAssertEqual(state?.ending, "timeout")
        XCTAssertEqual(state?.row, 6)
        var plain = op; plain["group"] = nil
        XCTAssertNil(ReorderGroupState(plain), "Arrange's own op is not the group's")
        XCTAssertEqual(ReorderGroupStep(key: "ArrowRight"), .nextList)
        XCTAssertEqual(ReorderGroupStep.actions.map(\.0), ["Move earlier", "Move later", "Move to previous list", "Move to next list"])
    }

    func testADragInAnyDirectionLiftsAGhostThatHoldsUntilTheMoveShows() throws {
        let calls = ReorderGroupRecorder()
        let (p, text) = fixture(calls)
        text.mouseDown(with: event(.leftMouseDown, text, down: 0))
        text.mouseDragged(with: event(.leftMouseDragged, text, down: 0, right: 3))
        XCTAssertEqual(calls.log, [], "inside the slop nothing is recognized")
        text.mouseDragged(with: event(.leftMouseDragged, text, down: 0, right: 20))
        XCTAssertEqual(calls.log.first, "begin 3 ghost", "a sideways move lifts a grouped grip")
        let hold = try XCTUnwrap(p.reorderHost!.group)
        let ghost = try XCTUnwrap(hold.ghost)
        XCTAssertTrue(ghost.view.superview != nil, "the ghost is in the window's top layer")
        // The pointer crosses into the second list.
        NSApp.sendEvent(event(.leftMouseDragged, text, down: 10, right: 220))
        XCTAssertEqual(calls.log.last, "into 5")
        NSApp.sendEvent(event(.leftMouseUp, text, down: 10, right: 220))
        XCTAssertEqual(calls.log.suffix(2), ["into 5", "drop"])
        XCTAssertEqual(hold.state.phase, "holding")
        XCTAssertTrue(p.reorderHost!.group === hold, "a hold keeps the ghost and the pin")
        XCTAssertNotNil(hold.ghost?.view.superview)
        // Escape does nothing while holding: its send is out.
        p.reorders.mouse.cancel()
        XCTAssertFalse(calls.log.contains("cancel"))
        // The answer showed the move: the ghost lands on the row, then the session finishes.
        let landed = expectation(description: "the ghost lands, then the session finishes")
        hold.observe(ReorderGroupState(calls.state("settling", target: 5, row: 6, ending: "landed", dispatched: true).ops[0].payload))
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { landed.fulfill() }
        wait(for: [landed], timeout: 2)
        XCTAssertEqual(calls.log.last, "finish")
        XCTAssertNil(p.reorderHost!.group)
        XCTAssertNil(ghost.view.superview, "the ghost is gone")
    }

    func testADropReleasesThePointerWhileTheReorderHolds() throws {
        let calls = ReorderGroupRecorder()
        let (p, text) = fixture(calls)
        defer { p.reorderHost!.group?.abandon() }
        let parent = try XCTUnwrap(p.views[9])
        let target = try XCTUnwrap(p.views[5])
        parent.handlers = ["pointerdown", "pointermove", "pointerup"]
        target.handlers = ["hover"]
        var heard: [String] = [], hovered: [UInt32] = []
        p.onPointer = { id, phase, _ in heard.append("\(phase) \(id)") }
        p.onHover = { id, inside in if inside { hovered.append(id) } }

        text.mouseDown(with: event(.leftMouseDown, text, down: 0))
        text.mouseDragged(with: event(.leftMouseDragged, text, down: 0, right: 20))
        XCTAssertEqual(heard, ["down 9", "move 9"])
        XCTAssertEqual(p.pointerHeld, 9)
        XCTAssertTrue(p.pointerSource === text)
        let hold = try XCTUnwrap(p.reorderHost!.group)
        // The grouped contact's monitor consumes the up before any view sees it.
        NSApp.sendEvent(event(.leftMouseDragged, text, down: 10, right: 220))
        NSApp.sendEvent(event(.leftMouseUp, text, down: 10, right: 220))
        XCTAssertEqual(heard, ["down 9", "move 9", "up 9"])
        XCTAssertNil(p.pointerHeld)
        XCTAssertNil(p.pointerSource)
        XCTAssertTrue(p.pointerPath.isEmpty)
        XCTAssertEqual(hold.state.phase, "holding")
        XCTAssertTrue(p.reorderHost!.group === hold)
        XCTAssertNotNil(hold.ghost?.view.superview)

        heard = []
        p.trackPointer(event(.mouseMoved, target, down: 0))
        p.flushHoverMove()
        XCTAssertEqual(heard, ["move 9"])
        XCTAssertTrue(hovered.contains(5), "hover resumes before another click")

        heard = []
        target.mouseDown(with: event(.leftMouseDown, target, down: 0))
        target.mouseUp(with: event(.leftMouseUp, target, down: 0))
        XCTAssertEqual(heard, ["down 9", "up 9"])
        XCTAssertNil(p.pointerHeld)
    }

    func testTheKeysLiftStepAndDropWithoutAGhost() throws {
        let calls = ReorderGroupRecorder()
        calls.hold = false
        let (p, _) = fixture(calls)
        let grip = try XCTUnwrap(p.views[3])
        XCTAssertTrue(grip.reorderKeys)
        XCTAssertTrue(grip.acceptsFirstResponder, "a grouped grip is in the key view loop")
        XCTAssertTrue(grip.reorderKey(" "))
        XCTAssertNil(p.reorderHost!.group?.ghost)
        XCTAssertTrue(grip.reorderKey("ArrowRight"))
        XCTAssertTrue(grip.reorderKey("Enter"))
        XCTAssertEqual(calls.log, ["begin 3", "step 4", "drop", "finish"], "no ghost: it finishes as the hold ends")
        XCTAssertNil(p.reorderHost!.group)
        // A custom action is the same, in one turn.
        calls.log = []
        let actions = try XCTUnwrap(grip.accessibilityCustomActions())
        XCTAssertEqual(actions.map(\.name), ["Move earlier", "Move later", "Move to previous list", "Move to next list"])
        XCTAssertTrue(grip.reorderAction(.later))
        XCTAssertEqual(calls.log, ["begin 3", "step 2", "drop", "finish"])
    }
}
#endif
