// @ref LLP 1115.000 D7 — a key a `keycapture` handler prevented has no
// default on the Mac: the focus stays where it is, as on the web.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactAnimations
@testable import ExactKit

final class KeyCaptureEscapeMacTests: XCTestCase {
    private var fixture: SheetFixture?
    override func tearDown() { fixture?.close(); fixture = nil; super.tearDown() }

    /// A shortcut recorder: an idle field whose focus starts recording, which
    /// mounts an `aria-modal` row around an `autofocus` field that takes every
    /// key first (`keycapture`). Escape ends recording, which changes only the
    /// field's placeholder; the row stays up until the field's `blur`.
    static let page = """
    component Probe
      state recording = false
      state taking = false
      state heard = ""
      action took(key: string)
        taking = key != "blur"
        recording = false
        heard = `${heard}${key};`
      action record
        recording = true
      view
        column testId="page" width="100%" height="100%" padding=20 gap=12
          if recording or taking
            row aria-modal=true testId="modal"
              Recorder(prompt=(recording ? "Press shortcut" : "Unassigned"), capture=took, cancel=took("blur"))
          else
            row focusin=record testId="idle"
              input id="field" testId="field" value="" placeholder="Unassigned" width=200 height=24
          text heard testId="status"

    component Recorder
      props
        prompt: string
        capture: action
        cancel: action
      action keys(k: string, e: KeyboardEvent)
        preventDefault()
        stopPropagation()
        if k == "Escape"
          capture("escape")
        else
          capture(k)
      view
        input id="recorder" testId="recorder" autofocus=true keycapture=keys blur=cancel aria-label="Keybinding" value="" placeholder=prompt width=200 height=24

    """

    private func key(_ code: UInt16, _ characters: String, _ f: SheetFixture) throws {
        let p = f.presenter
        for type in [NSEvent.EventType.keyDown, .keyUp] {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: type, location: .zero, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                       windowNumber: f.window.windowNumber, context: nil, characters: characters,
                                                       charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code))
            // As the session's monitor routes it, then AppKit when it was not taken.
            if type == .keyDown { p.flushKeyViewLoop() }
            if !p.routeKey(event, focused: true, in: f.window) { f.window.sendEvent(event) }
            f.settle()
        }
    }

    /// Escape in the recorder: its handler prevented it and only the
    /// placeholder changed, so the field keeps the focus and hears no blur.
    func testAPreventedEscapeThatChangesAPlaceholderKeepsTheFocus() throws {
        let f = try SheetFixture(page: Self.page)
        fixture = f
        let idle = try XCTUnwrap(try f.node("field").field)
        XCTAssertTrue(f.window.makeFirstResponder(idle))
        f.settle()
        let recorder = try f.node("recorder")
        let field = try XCTUnwrap(recorder.field)
        XCTAssertNotNil(field.currentEditor(), "the recorder mounted and took the focus")
        XCTAssertEqual(field.placeholderAttributedString?.string, "Press shortcut")
        try key(53, "\u{1b}", f)
        XCTAssertEqual(f.status, "escape;", "heard the Escape and no blur")
        XCTAssertTrue(try f.node("recorder") === recorder, "the row stays up")
        XCTAssertEqual(field.placeholderAttributedString?.string, "Unassigned")
        XCTAssertNotNil(field.currentEditor(), "the field keeps the focus")
        XCTAssertTrue(f.window.firstResponder === field.currentEditor())
        // The next key is still the recorder's.
        try key(40, "k", f)
        XCTAssertEqual(f.status, "escape;k;")
        XCTAssertNotNil(field.currentEditor())
    }

    func testHandlersThatOnlyCancelKeepTheEditorsText() throws {
        _ = NSApplication.shared
        let plan = try TableFixture.plan("""
        component CancelEdit
          state caret = -1
          action moved(e: InputEvent)
            caret = e.selectionStart
          action keys(k: string)
            if k == "x"
              preventDefault()
          action editing(e: InputEvent)
            if e.data == some("y") and caret == 1
              preventDefault()
          view
            textarea testId="editor" value="a" keycapture=keys selectionchange=moved beforeinput=editing rows=3
        """)
        let session = ExactApp.shared.makeSession(label: "cancel-edit")
        let presenter = session.presenter
        let rect = NSRect(x: 0, y: 0, width: 300, height: 200)
        let window = NSWindow(contentRect: rect, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { session.destroy(); window.close() }
        // Mount the editor without starting first-pixel work or its layout diagnostics.
        let boot = session.runtime.bootPlan(plan, width: rect.width, height: rect.height)
        XCTAssertNil(boot.error)
        session.apply(boot)
        session.followOffset()
        presenter.viewport.frame = rect
        window.contentView = presenter.viewport
        let node = try XCTUnwrap(presenter.views.values.first { $0.props["testId"] == "editor" })
        let editor = try XCTUnwrap(node.textArea)
        XCTAssertTrue(window.makeFirstResponder(editor))
        editor.setSelectedRange(NSRange(location: 0, length: 0))
        waitForMainQueue()
        let selectionChanged = expectation(description: "the selection change reaches the runner")
        let selections = presenter.fieldSelections
        let onChange = try XCTUnwrap(selections.onChange)
        selections.onChange = { id, value, range in
            let applied = session.appliedBatches
            onChange(id, value, range)
            XCTAssertEqual(session.appliedBatches, applied, "selection already moved in AppKit")
            if range.start == 1 { selectionChanged.fulfill() }
        }
        editor.setSelectedRange(NSRange(location: 1, length: 0))
        wait(for: [selectionChanged], timeout: 5)
        selections.onChange = onChange
        let applied = session.appliedBatches
        let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                   windowNumber: window.windowNumber, context: nil, characters: "x",
                                                   charactersIgnoringModifiers: "x", isARepeat: false, keyCode: 7))
        XCTAssertTrue(presenter.routeKey(event, focused: true, in: window))
        XCTAssertEqual(session.appliedBatches, applied, "the canceled key skips presentation")
        editor.insertText("y", replacementRange: editor.selectedRange())
        XCTAssertEqual(editor.string, "a")
        XCTAssertEqual(session.appliedBatches, applied, "the canceled edit skips presentation")

        session.clock = 0
        let layer = try XCTUnwrap(node.layer)
        presenter.boxAnimations.animations(node.id, ["specs": [["id": "fade", "k": "opacity", "s": 0.0, "d": 1.0,
            "n": 1.0, "fill": 3, "t": [0.0, 1.0], "v": [0.0, 1.0]]]], layer: layer, clock: 0)
        func offset() throws -> Double { try XCTUnwrap(layer.animation(forKey: "fade")).timeOffset }
        XCTAssertEqual(try offset(), 0)
        session.clock = 500
        XCTAssertTrue(presenter.routeKey(event, focused: true, in: window))
        XCTAssertEqual(try offset(), 0.5, accuracy: 0.001, "a canceled key seeks the animation")
        session.clock = 750
        editor.insertText("y", replacementRange: editor.selectedRange())
        XCTAssertEqual(editor.string, "a")
        XCTAssertEqual(try offset(), 0.75, accuracy: 0.001, "a canceled edit seeks the animation")
        editor.insertText("z", replacementRange: editor.selectedRange())
        XCTAssertEqual(editor.string, "az")
    }

    func testAnEmptyKeyStillAppliesWhileAPackagePictureIsRefused() throws {
        let session = ExactApp.shared.makeSession(label: "refused-picture")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("REFUSED_PICTURE"), size: CGSize(width: 300, height: 200)).error)
        settleViewlessSession(session)
        XCTAssertEqual(session.rasters?.hasRefusedPictures, true)
        let editor = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "editor" })
        let applied = session.appliedBatches
        var press = KeyPress("x")
        press.capture = true
        session.presenter.onKey?(editor.id, press)
        XCTAssertTrue(session.presenter.defaultPrevented)
        XCTAssertEqual(session.appliedBatches, applied + 1, "the full pass rechecks the picture")
    }

    func testAnEmptyTextInputStillRunsBeforeTheNextEdit() throws {
        for field in ["input", "textarea"] {
            let f = try SheetFixture(page: """
            component PostEdit
              state body = ""
              state previous = ""
              action before(e: InputEvent)
                if e.data == some("x") and previous == "a"
                  preventDefault()
              action edited(value: string)
                previous = value
                if value == "ac"
                  body = "ABC"
                  setSelectionRange("editor", 1, 2)
                else if value == "AbC"
                  setSelectionRange("editor", 0, 2)
              view
                \(field) id="editor" testId="editor" value=body input=edited beforeinput=before
            """)
            defer { f.close() }
            let node = try f.node("editor")
            let responder = try XCTUnwrap((node.textArea as NSResponder?) ?? node.field)
            XCTAssertTrue(f.window.makeFirstResponder(responder))
            let editor = try XCTUnwrap(node.textArea ?? node.field?.currentEditor() as? NSTextView)
            editor.insertText("a", replacementRange: editor.selectedRange())
            XCTAssertEqual(editor.string, "a")
            editor.insertText("x", replacementRange: editor.selectedRange())
            XCTAssertEqual(editor.string, "a", "beforeinput sees the preceding input's state")
            editor.insertText("c", replacementRange: editor.selectedRange())
            XCTAssertEqual(editor.string, "ABC")
            XCTAssertEqual(editor.selectedRange(), NSRange(location: 1, length: 1))
            editor.insertText("b", replacementRange: editor.selectedRange())
            XCTAssertEqual(editor.string, "AbC")
            XCTAssertEqual(editor.selectedRange(), NSRange(location: 0, length: 2))
            XCTAssertTrue(f.window.firstResponder === editor)
        }
    }

    /// A batch that changes the placeholder of a focused, empty field: AppKit
    /// restarts the field editor's editing to show it (with text in the
    /// field it does not), and the field keeps the focus and its caret, with
    /// no blur, focus or `change` heard.
    func testAPlaceholderChangeKeepsAFocusedFieldsEditing() throws {
        _ = NSApplication.shared
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        addTeardownBlock { window.close() }
        window.contentView = p.viewport
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "view"], ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 200],
                           ["op": "create", "id": 2, "kind": "input", "props": ["value": "", "placeholder": "Press shortcut"], "handlers": ["focus", "blur", "change"]],
                           ["op": "frame", "id": 2, "x": 10, "y": 10, "w": 200, "h": 24],
                           ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]]))
        var heard: [String] = []
        p.onFocus = { _ in heard.append("focus") }
        p.onBlur = { _ in heard.append("blur") }
        p.onChange = { _, _ in heard.append("change") }
        let field = try XCTUnwrap(p.views[2]?.field)
        XCTAssertTrue(window.makeFirstResponder(field))
        heard = []
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["placeholder": "Unassigned"]]]))
        RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
        XCTAssertEqual(field.placeholderAttributedString?.string, "Unassigned")
        XCTAssertTrue(window.firstResponder === field.currentEditor(), "the field keeps the field editor")
        XCTAssertEqual((field.currentEditor() as? NSTextView)?.selectedRange(), NSRange(location: 0, length: 0), "and its caret")
        XCTAssertEqual(heard, [], "no blur, focus or change")
        p.apply(wireBatch([["op": "props", "id": 2, "clear": ["placeholder"]]]))
        XCTAssertTrue(window.firstResponder === field.currentEditor())
        XCTAssertEqual(heard, [])
        // Leaving it is still heard.
        XCTAssertTrue(window.makeFirstResponder(nil))
        RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
        XCTAssertEqual(heard.filter { $0 == "blur" }, ["blur"])
    }
}
#endif
