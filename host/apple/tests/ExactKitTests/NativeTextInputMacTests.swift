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
        XCTAssertTrue(text("keys").hasPrefix("capture x;bubble x;up "))
        XCTAssertEqual(text("keys").components(separatedBy: "bubble").count, 2)
        XCTAssertEqual(text("before"), "")

        // An input context can bind a printable key to a selector. Model
        // that callback without changing the machine's key binding files.
        let custom = try event("c", kVK_ANSI_C)
        XCTAssertFalse(session.presenter.routeKey(custom, focused: true, in: window))
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
}
#endif
