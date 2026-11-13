#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactMarkdown

/// The editing chords a paragraph and a text area handle themselves follow
/// the layout Command selects. Under Dvorak - QWERTY ⌘ a key types its
/// QWERTY character with Command down (`characters`) and its Dvorak one
/// without (`charactersIgnoringModifiers`); the chord is the QWERTY one.
final class CommandLayoutMacTests: XCTestCase {
    private func key(_ characters: String, ignoring: String, code: UInt16, _ modifiers: NSEvent.ModifierFlags = .command, in window: NSWindow) -> NSEvent {
        NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0, windowNumber: window.windowNumber,
                         context: nil, characters: characters, charactersIgnoringModifiers: ignoring, isARepeat: false, keyCode: code)!
    }
    private func window(_ view: NSView) -> NSWindow {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: view.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        return window
    }

    func testAParagraphCopiesOnTheCommandLayoutsC() {
        let presenter = Presenter()
        let paragraph = NodeView(id: 1, kind: "text", presenter: presenter)
        paragraph.frame = NSRect(x: 0, y: 0, width: 300, height: 20)
        paragraph.props = ["text": "Alpha beta"]
        presenter.viewport.frame = NSRect(x: 0, y: 0, width: 300, height: 40)
        presenter.root.addSubview(paragraph); presenter.views[1] = paragraph
        let window = window(presenter.viewport)
        defer { window.close() }
        XCTAssertTrue(paragraph.isParagraph)
        XCTAssertTrue(window.makeFirstResponder(paragraph))
        // Command-C at ANSI C (Dvorak J) is Copy; Command-I at ANSI I (Dvorak C) is not.
        XCTAssertTrue(paragraph.performKeyEquivalent(with: key("c", ignoring: "j", code: 8, in: window)))
        XCTAssertFalse(paragraph.performKeyEquivalent(with: key("i", ignoring: "c", code: 34, in: window)))
        // A layout Command leaves alone keeps its own letter.
        XCTAssertTrue(paragraph.performKeyEquivalent(with: key("c", ignoring: "c", code: 34, in: window)))
    }

    /// Select All and Copy are ⌘A and ⌘C alone, as the Edit menu has them; a
    /// second modifier leaves the chord to a menu item that owns it.
    func testAParagraphTakesCommandAAndCWithNoOtherModifier() {
        let presenter = Presenter()
        let paragraph = NodeView(id: 1, kind: "text", presenter: presenter)
        paragraph.frame = NSRect(x: 0, y: 0, width: 300, height: 20)
        paragraph.props = ["text": "Alpha beta"]
        presenter.viewport.frame = NSRect(x: 0, y: 0, width: 300, height: 40)
        presenter.root.addSubview(paragraph); presenter.views[1] = paragraph
        let window = window(presenter.viewport)
        defer { window.close() }
        XCTAssertTrue(window.makeFirstResponder(paragraph))
        for letter in ["a", "c"] {
            let code: UInt16 = letter == "a" ? 0 : 8
            for extra: NSEvent.ModifierFlags in [.shift, .option, .control] {
                let event = key(letter, ignoring: letter, code: code, [.command, extra], in: window)
                XCTAssertFalse(paragraph.performKeyEquivalent(with: event), "\(letter) with \(extra.rawValue)")
            }
            XCTAssertTrue(paragraph.performKeyEquivalent(with: key(letter, ignoring: letter, code: code, [.command, .capsLock], in: window)))
        }
    }

    func testATextAreaFormatsAndUndoesOnTheCommandLayoutsKeys() {
        ExactMarkdown.install()
        let presenter = Presenter()
        let node = NodeView(id: 3, kind: "textarea", presenter: presenter)
        node.frame = NSRect(x: 0, y: 0, width: 500, height: 300)
        node.makeTextArea()
        node.props["markup"] = "markdown"
        node.props["value"] = "hello"
        node.applyTextArea()
        node.layoutTextArea()
        let window = window(node)
        defer { window.close() }
        let f = node.textArea as! TextArea
        window.makeFirstResponder(f)
        f.setSelectedRange(NSRange(location: 0, length: 5))
        f.undoManager?.removeAllActions()
        // Command-N at ANSI N (Dvorak B) is not Bold; Command-B at ANSI B (Dvorak X) is.
        XCTAssertFalse(f.performKeyEquivalent(with: key("n", ignoring: "b", code: 45, in: window)))
        XCTAssertEqual(f.string, "hello")
        XCTAssertTrue(f.performKeyEquivalent(with: key("b", ignoring: "x", code: 11, in: window)))
        XCTAssertEqual(f.string, "**hello**")
        // Command-/ at ANSI / (Dvorak Z) is not Undo; Command-Z at ANSI Z (Dvorak ;) is, and with Shift Redo.
        XCTAssertFalse(f.performKeyEquivalent(with: key("/", ignoring: "z", code: 44, in: window)))
        XCTAssertEqual(f.string, "**hello**")
        XCTAssertTrue(f.performKeyEquivalent(with: key("z", ignoring: ";", code: 6, in: window)))
        XCTAssertEqual(f.string, "hello")
        XCTAssertTrue(f.performKeyEquivalent(with: key("Z", ignoring: ":", code: 6, [.command, .shift], in: window)))
        XCTAssertEqual(f.string, "**hello**")
    }
}
#endif
