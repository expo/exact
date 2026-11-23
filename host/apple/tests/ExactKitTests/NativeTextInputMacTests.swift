#if os(macOS)
import AppKit
import Carbon.HIToolbox
import ExactCore
import XCTest
@testable import ExactKit

final class NativeTextInputMacTests: XCTestCase {
    private var session: ExactSession!
    private var window: NSWindow!

    override func tearDown() {
        session?.destroy()
        window?.close()
        session = nil
        window = nil
        super.tearDown()
    }

    private func boot() throws {
        _ = NSApplication.shared
        session = ExactApp.shared.makeSession(label: "native-text")
        session.postsTimerAdvances = true
        session.presenter.menus.painted = true
        let size = CGSize(width: 400, height: 440)
        window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = session.presenter.viewport
        XCTAssertNil(session.boot(plan: try fixturePlan("NATIVE_TEXT_INPUT"), size: size).error)
        session.drainFill()
        waitForMainQueue()
        session.drainFill()
    }

    private func node(_ name: String) -> NodeView? {
        session.presenter.views.values.first { $0.props["testId"] == name }
    }

    private func text(_ name: String) -> String { node(name)?.paragraphText ?? "" }

    private func focus(_ name: String) throws -> TextArea {
        let editor = try XCTUnwrap(node(name)?.textArea as? TextArea)
        XCTAssertTrue(window.makeFirstResponder(editor))
        return editor
    }

    private func event(_ characters: String, _ code: Int, flags: NSEvent.ModifierFlags = [], type: NSEvent.EventType = .keyDown) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: type, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
                                       windowNumber: window.windowNumber, context: nil, characters: characters,
                                       charactersIgnoringModifiers: characters.lowercased(), isARepeat: false, keyCode: UInt16(code)))
    }

    @discardableResult
    private func press(_ event: NSEvent, editor: TextArea) -> Bool {
        let p = session.presenter
        p.flushKeyViewLoop()
        let cancelled = p.routeKey(event, focused: true, in: window)
        if !cancelled, window.firstResponder === editor { editor.keyDown(with: event) }
        return cancelled
    }

    func testNativeInputPostsBehindAReplyAndCloseSettlesIt() throws {
        try boot()
        let editor = try focus("async")
        waitForMainQueue()
        let previousSelection = text("selection")

        let down = try event("a", kVK_ANSI_A)
        XCTAssertFalse(session.presenter.routeKey(down, focused: true, in: window))
        let entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
        Owner.shared.post {
            entered.signal()
            XCTAssertEqual(release.wait(timeout: .now() + 5), .success)
        }
        XCTAssertEqual(entered.wait(timeout: .now() + 5), .success)
        session.pumpReplies()
        editor.keyDown(with: down)
        XCTAssertEqual(editor.string, "a")
        XCTAssertEqual(text("recorded"), "", "AppKit inserts while the owner is held")
        release.signal()
        XCTAssertFalse(session.beforeUnload())
        XCTAssertEqual(text("settled"), "a")
        XCTAssertEqual(text("first"), "a")
        XCTAssertEqual(text("second"), "a")
        XCTAssertEqual(text("future"), "", "work after the input deadline remains scheduled")
        waitForMainQueue()
        XCTAssertEqual(text("selection"), previousSelection, "the app already heard this caret in input")

        editor.setSelectedRange(NSRange(location: 0, length: 0))
        waitFor("the independent selection change") { self.text("selection") == "0:0" }
    }

    func testCaptureCommandBindingsAndNativeUndoStillHaveTheirDefaults() throws {
        try boot()
        let guarded = try focus("checked")
        XCTAssertTrue(press(try event("x", kVK_ANSI_X), editor: guarded))
        XCTAssertEqual(guarded.string, "")
        XCTAssertEqual(text("keys"), "capture x;bubble x;", "preventDefault does not stop bubbling")

        let editor = try focus("binding")
        XCTAssertFalse(press(try event("A", kVK_ANSI_A, flags: .shift), editor: editor))
        XCTAssertEqual(editor.string, "A")
        session.presenter.keyUp(try event("A", kVK_ANSI_A, flags: .shift, type: .keyUp), in: window)
        XCTAssertEqual(text("before"), "")

        // An input context can bind a printable key to a selector. Model
        // that callback without changing the machine's key binding files.
        let custom = try event("c", kVK_ANSI_C)
        XCTAssertFalse(session.presenter.routeKey(custom, focused: true, in: window))
        XCTAssertTrue(text("keys").hasPrefix("capture x;bubble x;up "), "the release precedes the next key's command")
        XCTAssertEqual(text("keys").components(separatedBy: "bubble").count, 2)
        editor.interpretKey(custom) { editor.doCommand(by: #selector(NSTextView.insertNewline(_:))) }
        XCTAssertEqual(editor.string, "A", "the restored bubble cancels the selector")
        XCTAssertTrue(text("keys").hasSuffix("bubble c;"))

        editor.undo(nil)
        XCTAssertEqual(editor.string, "")
        XCTAssertEqual(text("before"), "historyUndo")
        editor.redo(nil)
        XCTAssertEqual(editor.string, "A")
        XCTAssertEqual(text("before"), "historyRedo")

        let plain = try focus("plain")
        XCTAssertFalse(press(try event("b", kVK_ANSI_B), editor: plain))
        XCTAssertEqual(plain.string, "b")
        XCTAssertEqual(text("before"), "insertText")
        XCTAssertTrue(text("keys").hasSuffix("capture b;bubble b;"))
    }

    func testMarkedTextAndPasteKeepBeforeInput() throws {
        try boot()
        let editor = try focus("binding")
        editor.setMarkedText("に", selectedRange: NSRange(location: 1, length: 0),
                             replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertTrue(editor.hasMarkedText())
        XCTAssertEqual(text("before"), "insertCompositionText")
        editor.unmarkText()

        session.presenter.fieldSelections.withIntent(node("binding"), "insertFromPaste") {
            editor.insertText("PASTE", replacementRange: editor.selectedRange())
        }
        XCTAssertEqual(editor.string, "に")
        XCTAssertEqual(text("before"), "insertFromPaste")
    }

    func testAKeyUpFollowsFocusMovedByThePreviousInput() throws {
        try boot()
        let redirect = try XCTUnwrap(node("redirect"))
        session.apply(session.runtime.press(redirect.id, now: session.now()))
        let editor = try focus("async")
        XCTAssertFalse(press(try event("b", kVK_ANSI_B), editor: editor))
        session.presenter.keyUp(try event("b", kVK_ANSI_B, type: .keyUp), in: window)
        waitFor("the release reaches the new focus") { self.text("keys").hasSuffix("up b;") }
        XCTAssertTrue(window.firstResponder === node("checked")?.textArea)
    }

    func testAnEarlierPublicationRemovesTheEditorBeforeTheNextKey() throws {
        try boot()
        let editor = try focus("async")
        let remove = try XCTUnwrap(node("remove"))
        let ahead = expectation(description: "the owner crosses the barrier before native insertion")
        let runtime = session.runtime
        let topology = runtime.topLayer
        runtime.topLayer = nil // The fixture has no presentation outside its window's tree.
        defer { runtime.topLayer = topology }
        let at = session.now(), id = remove.id
        session.repliesInFlight += 1
        session.replySubmissions.append(0)
        runtime.post(done: { [weak session] batch in session?.publish(.reply, batch) }) { _ in
            Owner.shared.callMain {
                XCTAssertEqual(editor.string, "")
                ahead.fulfill()
            }
            return runtime.press(id, now: at)
        }

        _ = session.presenter.settleNativeKeyDown(in: window)
        wait(for: [ahead], timeout: 5)
        XCTAssertNil(node("async"))
        XCTAssertEqual(text("settled"), "")
    }

    private func armEarlierTimer() throws {
        // Keep this one on the runner clock so it can precede the native
        // input in the same advance pass, rather than use a platform timer.
        session.runtime.useNativeTimers(false)
        let arm = try XCTUnwrap(node("arm"))
        session.advancingDue = true
        session.apply(session.runtime.press(arm.id, now: session.now()))
        session.clockTimer?.invalidate()
        let due = try XCTUnwrap(session.timerDue)
        waitFor("the earlier timer's deadline is before the text input") { self.session.now() > due }
        XCTAssertEqual(text("earlier"), "")
    }

    func testHeldInputSettlesAllYieldsAtItsOwnDeadlineBeforeUnload() throws {
        try boot()
        let editor = try focus("async")
        try armEarlierTimer()
        let down = try event("a", kVK_ANSI_A)
        session.presenter.flushKeyViewLoop()
        XCTAssertFalse(session.presenter.routeKey(down, focused: true, in: window))
        session.advancingDue = false
        editor.keyDown(with: down)
        XCTAssertTrue(session.nativeTextThenWaiting)
        XCTAssertTrue(session.settleNativeText())
        XCTAssertEqual(text("earlier"), "earlier")
        XCTAssertEqual(text("first"), "a")
        XCTAssertEqual(text("second"), "a")
        XCTAssertEqual(text("settled"), "a")
        XCTAssertEqual(text("future"), "")
        XCTAssertFalse(session.beforeUnload())
    }

    func testAnOverBudgetReceiptStopsAndPreventsUnload() throws {
        try boot()
        let loop = try XCTUnwrap(node("loop"))
        session.apply(session.runtime.press(loop.id, now: session.now()))
        let editor = try focus("async")
        XCTAssertFalse(press(try event("a", kVK_ANSI_A), editor: editor))

        XCTAssertFalse(session.beforeUnload())
        XCTAssertTrue(session.nativeText.stalled)
        XCTAssertEqual(text("unload-saw"), "", "an incomplete drain vetoes before dispatch")

        XCTAssertFalse(session.beforeUnload())
        XCTAssertFalse(session.nativeTextThenWaiting, "a later close drains the finite remainder")
        XCTAssertEqual(text("unload-saw"), "a", "the authored callback now sees the last edit")
    }

    func testAnOverBudgetReceiptConsumesReturnBeforeRouting() throws {
        try boot()
        let loop = try XCTUnwrap(node("loop"))
        session.apply(session.runtime.press(loop.id, now: session.now()))
        let editor = try focus("async")
        XCTAssertFalse(press(try event("a", kVK_ANSI_A), editor: editor))

        let enter = try event("\r", kVK_Return)
        XCTAssertTrue(press(enter, editor: editor))
        XCTAssertTrue(session.nativeText.stalled)
        XCTAssertEqual(text("submitted"), "")
        XCTAssertTrue(window.firstResponder === editor)

        let beforeSelection = text("selection")
        editor.setSelectedRange(NSRange(location: 0, length: 0))
        waitForMainQueue()
        XCTAssertEqual(text("selection"), beforeSelection, "selection waits for the receipt too")
        let corrected = try XCTUnwrap(node("checked")?.textArea)
        XCTAssertNotNil(corrected.undoManager)
        session.presenter.fieldSelections.setRangeText(["checked", "fixed"])
        XCTAssertEqual(corrected.string, "fixed", "an authored correction may still change the field")

        XCTAssertTrue(press(enter, editor: editor))
        XCTAssertFalse(session.nativeTextThenWaiting)
        XCTAssertEqual(text("submitted"), "a")
        XCTAssertEqual(text("selection"), "0:0")
        XCTAssertTrue(window.firstResponder === node("checked")?.textArea)
    }

    func testIncompleteSettlementBlocksAPopoverWithNoPressHandler() throws {
        try boot()
        let loop = try XCTUnwrap(node("loop"))
        let open = try XCTUnwrap(node("default-popover"))
        session.presenter.press(loop.id)
        let editor = try focus("async")
        XCTAssertFalse(press(try event("a", kVK_ANSI_A), editor: editor))

        session.presenter.press(open.id)
        XCTAssertTrue(session.nativeText.stalled)
        XCTAssertTrue(session.presenter.openTopLayer.frames.isEmpty)

        session.presenter.press(open.id)
        XCTAssertFalse(session.nativeTextThenWaiting)
        XCTAssertFalse(session.presenter.openTopLayer.frames.isEmpty)
        XCTAssertEqual(text("settled"), "a")
    }

    func testMouseDownDoesNotPassAnIncompleteDrainOrUseAReplacedTarget() throws {
        try boot()
        let loop = try XCTUnwrap(node("loop"))
        session.presenter.press(loop.id)
        let old = try XCTUnwrap(node("old-send"))
        let point = old.convert(NSPoint(x: old.bounds.midX, y: old.bounds.midY), to: nil)
        XCTAssertTrue(old.window === window)
        let target = try XCTUnwrap(Presenter.node(at: point, in: window))
        XCTAssertTrue(target.isDescendant(of: old), "the click hits the old Send button")
        let editor = try focus("async")
        XCTAssertFalse(press(try event("a", kVK_ANSI_A), editor: editor))

        XCTAssertFalse(session.presenter.admitsNativeMouseDown(at: point, in: window))
        XCTAssertTrue(session.nativeText.stalled)
        XCTAssertNotNil(node("old-send"))
        XCTAssertEqual(text("clicked"), "")

        XCTAssertFalse(session.presenter.admitsNativeMouseDown(at: point, in: window), "the second click's old target disappeared during settlement")
        XCTAssertNil(node("old-send"))
        XCTAssertEqual(text("clicked"), "")
        XCTAssertTrue(session.presenter.admitsNativeMouseDown(at: point, in: window))
        session.presenter.press(try XCTUnwrap(node("new-send")).id)
        XCTAssertEqual(text("clicked"), "a")
    }

    func testAStalledSessionDoesNotRetryOnAnotherSessionsTableKey() throws {
        try boot()
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 440))
        window.contentView = content
        let firstView = ExactView(session: session)
        let second = ExactApp.shared.makeSession(label: "native-text-table")
        defer { second.destroy() }
        let secondView = ExactView(session: second)
        firstView.frame = NSRect(x: 0, y: 0, width: 200, height: 440)
        secondView.frame = NSRect(x: 200, y: 0, width: 200, height: 440)
        content.addSubview(firstView)
        content.addSubview(secondView)
        XCTAssertNil(second.boot(plan: try fixturePlan("NATIVE_TEXT_INPUT"), size: secondView.bounds.size).error)
        waitForMainQueue()
        session.drainFill()
        second.drainFill()
        let list = try XCTUnwrap(second.presenter.views.values.first { $0.props["testId"] == "table-keys" })
        let table = try XCTUnwrap(second.presenter.tables.feeds[list.id]?.table)

        let loop = try XCTUnwrap(node("loop"))
        session.apply(session.runtime.press(loop.id, now: session.now()))
        let editor = try focus("async")
        XCTAssertFalse(press(try event("a", kVK_ANSI_A), editor: editor))
        let enter = try event("\r", kVK_Return)
        XCTAssertTrue(press(enter, editor: editor))
        XCTAssertTrue(session.nativeText.stalled)

        XCTAssertTrue(window.makeFirstResponder(table))
        XCTAssertTrue(window.firstResponder === table)
        XCTAssertFalse(firstView.ownsShortcutFocus(in: window))
        XCTAssertTrue(session.presenter.keyTarget(table)?.presenter === second.presenter)
        _ = session.presenter.routeKey(enter, focused: firstView.ownsShortcutFocus(in: window), in: window)
        XCTAssertTrue(session.nativeText.stalled, "the table's key must not retry the other session")
        XCTAssertTrue(window.firstResponder === table)
        second.presenter.flushKeyViewLoop()
        _ = second.presenter.routeKey(enter, focused: secondView.ownsShortcutFocus(in: window), in: window)
        XCTAssertTrue(second.presenter.views.values.contains {
            $0.props["testId"] == "keys" && $0.paragraphText == "bubble Enter;"
        })
    }

    func testAHeldAdvanceDoesNotLetTheWindowClose() throws {
        try boot()
        let editor = try focus("async")
        XCTAssertFalse(press(try event("a", kVK_ANSI_A), editor: editor))
        Owner.shared.sync {}
        session.runtime.held = true
        defer { session.runtime.held = false }

        XCTAssertFalse(session.beforeUnload())
        XCTAssertTrue(session.nativeText.stalled)
    }

    func testImmediateTextDoesNotFireAnElapsedUnrelatedTimer() throws {
        try boot()
        let editor = try focus("async")
        XCTAssertFalse(press(try event("a", kVK_ANSI_A), editor: editor))
        Owner.shared.sync {} // The real input commit arms both kinds of work.
        let elapsed = DispatchSemaphore(value: 0)
        DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(30)) { elapsed.signal() }
        XCTAssertEqual(elapsed.wait(timeout: .now() + 2), .success)

        XCTAssertFalse(session.beforeUnload())
        XCTAssertEqual(text("settled"), "a")
        XCTAssertEqual(text("future"), "", "draining text does not run an elapsed ordinary timer")
    }

    func testPopoverFallbackSettlesTheEarlierChangeAndInputBeforeReturn() throws {
        try boot()
        let open = try XCTUnwrap(node("open"))
        session.apply(session.runtime.press(open.id, now: session.now()))
        session.drainFill()
        XCTAssertFalse(session.presenter.openTopLayer.frames.isEmpty)
        let editor = try focus("popover-editor")
        try armEarlierTimer()
        let down = try event("q", kVK_ANSI_Q)
        session.presenter.flushKeyViewLoop()
        XCTAssertFalse(session.presenter.routeKey(down, focused: true, in: window))
        session.advancingDue = false
        editor.keyDown(with: down)

        XCTAssertEqual(editor.string, "q")
        XCTAssertEqual(text("earlier"), "earlier")
        XCTAssertTrue(session.settleNativeText())
        XCTAssertEqual(text("settled"), "q")
        XCTAssertEqual(text("future"), "")
        XCTAssertTrue(press(try event("\r", kVK_Return), editor: editor))
        XCTAssertEqual(text("submitted"), "q")
        XCTAssertFalse(session.beforeUnload())
    }

}
#endif
