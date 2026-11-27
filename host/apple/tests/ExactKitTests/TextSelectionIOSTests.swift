#if os(iOS)
import CoreText
import UIKit
import UIKit.UIGestureRecognizerSubclass
import XCTest
@testable import ExactControls
@testable import ExactKit
@testable import ExactMarkdown
@testable import ExactTextSelection

/// LLP 1115.000 D11.6: `user-select="text"` on UIKit. Text its author made
/// selectable takes UIKit's own text interaction when a finger first lands
/// on it and answers it from the paragraphs' own lines; text nobody made
/// selectable takes nothing. UIKit synthesizes no touch for a unit test, so
/// a gesture's result is asked for as the driver asks (`selectWord`).
/// Real long presses, double taps and handles need simulator coordinate
/// touches; the agent's contextmenu and dblclick deliver recognized events.
/// UIKit, so a simulator runs it: bun host/apple/build.mjs --test --ios
final class TextSelectionIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?
    override func tearDown() { session?.destroy(); session = nil; window?.isHidden = true; window = nil; super.tearDown() }

    private func mounted(_ ops: [[String: Any]]) -> Presenter {
        let session = ExactApp.shared.makeSession(label: "text-selection")
        self.session = session
        let p = session.presenter
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        self.window = window
        p.viewport.frame = window.bounds
        window.rootViewController?.view.addSubview(p.viewport)
        p.apply(wireBatch(ops))
        p.viewport.layoutIfNeeded()
        return p
    }
    private func text(_ id: Int, _ value: String, style: [String: Any] = [:], handlers: [String] = []) -> [String: Any] {
        ["op": "create", "id": id, "kind": "text", "props": ["text": value, "testId": "t\(id)"],
         "style": ["font_size": 17.0, "line_height": "22px"].merging(style) { _, new in new }, "handlers": handlers]
    }
    private func box(_ id: Int, style: [String: Any] = [:], props: [String: Any] = [:], handlers: [String] = [], kind: String = "view") -> [String: Any] {
        ["op": "create", "id": id, "kind": kind, "style": style, "props": props, "handlers": handlers]
    }
    private func frame(_ id: Int, _ y: Double, h: Double = 22, x: Double = 0, w: Double = 360) -> [String: Any] {
        ["op": "frame", "id": id, "x": x, "y": y, "w": w, "h": h]
    }
    private func node(_ p: Presenter, _ id: UInt32) throws -> NodeView { try XCTUnwrap(p.views[id], "node \(id)") }
    /// A point inside `word` of a one-line paragraph, in the node's space.
    private func point(in node: NodeView, of word: String) throws -> CGPoint {
        let shaped = SelectableText.shaped(node)
        let range = shaped.range(of: word)
        XCTAssertNotEqual(range.location, NSNotFound, word)
        let held = try XCTUnwrap(node.presenter?.selectionHost.install(node))
        let base = held.document?.parts.first { $0.node === node }?.start ?? 0
        let rect = held.firstRect(for: try XCTUnwrap(held.textRange(from: try XCTUnwrap(held.position(from: held.beginningOfDocument, offset: base + range.location)),
                                                                   to: try XCTUnwrap(held.position(from: held.beginningOfDocument, offset: base + NSMaxRange(range))))))
        return held.textInputView.convert(CGPoint(x: rect.midX, y: rect.midY), to: node)
    }

    func testCaretStaysWithinItsColumnFragment() throws {
        let p = mounted([text(1, "Alpha\nbeta\ngamma\ndelta", style: ["line_height": "20px", "white_space": "pre", "user_select": "text"]),
                         ["op": "roots", "ids": [1]], frame(1, 0, h: 60, w: 220),
                         ["op": "fragments", "id": 1, "fragments": [[0, 0, 100, 60, 0, 2, 0, 10], [120, 0, 100, 40, 2, 4, 120, -40]]]])
        let input = try XCTUnwrap(p.selectionHost.install(try node(p, 1)))
        XCTAssertLessThan(input.index(at: CGPoint(x: 5, y: 55)), 11,
                          "the first fragment owns only Alpha and beta")
    }

    func testCaretAndSelectionFollowThePaintedFractionalBaseline() throws {
        let p = mounted([text(1, "Hxg", style: ["font_size": 13.0, "line_height": "normal", "user_select": "text"], handlers: ["contextmenu"]),
                         ["op": "roots", "ids": [1]], frame(1, 0)])
        let node = try node(p, 1)
        node.traitOverrides.displayScale = 3; node.updateTraitsIfNeeded()
        let input = try XCTUnwrap(p.selectionHost.install(node)), paragraph = try XCTUnwrap(node.paragraphLayout())
        var above: CGFloat = 0
        _ = CTLineGetTypographicBounds(try XCTUnwrap(paragraph.lines.first), &above, nil, nil)
        let baseline = node.paragraphBox().minY + 37.0 / 3
        let caret = input.textInputView.convert(input.caretRect(for: SelectablePosition(0)), to: node)
        XCTAssertEqual(caret.minY + above, baseline, accuracy: 1e-6)
        let rect = try XCTUnwrap(input.selectionRects(for: SelectableRange(NSRange(location: 0, length: 3))).first)
        XCTAssertEqual(input.textInputView.convert(rect.rect, to: node).minY + above, baseline, accuracy: 1e-6)
    }

    /// A page: `prose` holds two paragraphs its author made selectable, a
    /// pressable row holds one it made selectable and one it did not, and a
    /// label, a button's label and an opted-out paragraph stand beside them.
    private func page() -> Presenter {
        mounted([
            box(1),
            text(2, "A label nobody made selectable."),
            box(3, style: ["user_select": "text"]),
            text(4, "The quick brown fox jumps over the lazy dog.", handlers: ["selectionchange"]),
            text(5, "Opted out inside the prose.", style: ["user_select": "none"]),
            box(6, handlers: ["press", "contextmenu"]),
            text(7, "Selectable words in a pressable row.", style: ["user_select": "text"]),
            text(8, "The row's own label."),
            box(9, handlers: ["press"], kind: "button"), text(10, "A button's label"),
            ["op": "children", "id": 1, "ids": [2, 3, 6]], ["op": "children", "id": 3, "ids": [4, 5, 9]],
            ["op": "children", "id": 6, "ids": [7, 8]], ["op": "children", "id": 9, "ids": [10]],
            ["op": "roots", "ids": [1]],
            frame(1, 0, h: 1600, w: 400), frame(2, 80, x: 20), frame(3, 120, h: 110, x: 20), frame(4, 0), frame(5, 30), frame(9, 60, h: 40), frame(10, 9),
            frame(6, 250, h: 60, x: 20), frame(7, 4), frame(8, 32),
        ])
    }

    func testOnlyTextItsAuthorMadeSelectableIs() throws {
        let p = page()
        XCTAssertFalse(try node(p, 2).textSelectable, "nothing said: a UILabel's text selects nothing")
        XCTAssertTrue(try node(p, 4).textSelectable, "the nearest authored user-select is text")
        XCTAssertFalse(try node(p, 5).textSelectable, "the nearest authored user-select is none")
        XCTAssertTrue(try node(p, 7).textSelectable, "said on the text itself, inside a pressable row")
        XCTAssertFalse(try node(p, 8).textSelectable, "the row's own label")
        XCTAssertFalse(try node(p, 10).textSelectable, "a control's label, though the prose above says text")
        // A finger on text that is not selectable adds nothing to it.
        for id in [2, 5, 8, 10] as [UInt32] {
            let unsaid = try node(p, id)
            p.selectionHost.touched(unsaid)
            XCTAssertTrue(unsaid.interactions.isEmpty, "node \(id)")
            XCTAssertTrue((unsaid.gestureRecognizers ?? []).isEmpty, "node \(id)")
            XCTAssertNil(p.selectionHost.selectWord(in: unsaid, at: CGPoint(x: 30, y: 10)), "node \(id)")
        }
        XCTAssertTrue(p.selectionHost.texts.isEmpty)
    }

    /// CSS `user-select: all`: a long press or a double tap in the element
    /// selects all of it, not a word, and a range UIKit asks for that takes
    /// part of it takes all of it. A tap selects nothing, as on a
    /// `UITextView`, so its single taps stay off for the node's press. An inline run's element makes
    /// its paragraph selectable for that run alone, its own text being a
    /// `UILabel`'s; an element around a paragraph is the whole paragraph.
    func testUserSelectAllSelectsTheWholeElement() throws {
        let p = mounted([
            box(1),
            text(2, "/Users/me/project/file.ts", style: ["user_select": "all"], handlers: ["selectionchange"]),
            ["op": "create", "id": 3, "kind": "text", "style": ["font_size": 17.0, "line_height": "22px"], "handlers": ["selectionchange"]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 400, w: 400), frame(2, 80, x: 20), frame(3, 120, x: 20),
            ["op": "paragraph", "id": 3, "runs": [
                ["id": 31, "parent": 3, "paint": true, "props": ["text": "Folder: "]],
                ["id": 32, "parent": 3, "paint": true, "props": ["text": "/tmp/work"], "style": ["user_select": "all"]],
                ["id": 33, "parent": 3, "paint": true, "props": ["text": ". Check it."]],
            ]],
        ])
        let path = try node(p, 2), folder = try node(p, 3)
        var heard: [String] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append("\(id) \(text) \(start) \(end)") }
        XCTAssertTrue(p.selectionHost.admits(path))
        XCTAssertFalse(folder.textSelectable, "the paragraph's own text is a label's")
        XCTAssertTrue(p.selectionHost.admits(folder), "its run is selectable")
        p.selectionHost.touched(path)
        let taps = (path.gestureRecognizers ?? []).compactMap { $0 as? UITapGestureRecognizer }.filter { $0.numberOfTapsRequired == 1 }
        XCTAssertFalse(taps.isEmpty)
        XCTAssertTrue(taps.allSatisfy { !$0.isEnabled }, "nothing selected: a tap is the node's press")
        XCTAssertEqual(p.selectionHost.selectWord(in: path, at: try point(in: path, of: "file"))?.text, "/Users/me/project/file.ts", "a long press")
        XCTAssertEqual(heard.last, "2 /Users/me/project/file.ts 0 25")
        XCTAssertEqual(p.selectionHost.selectWord(in: folder, at: try point(in: folder, of: "work"))?.text, "/tmp/work")
        XCTAssertEqual(heard.last, "3 /tmp/work 8 17")
        XCTAssertNil(p.selectionHost.text(for: path)?.reportable, "one selection on the page")
        p.selectionHost.clear()
        XCTAssertEqual(p.selectionHost.selectWord(in: folder, at: try point(in: folder, of: "tmp"))?.text, "/tmp/work", "a double tap")
        p.selectionHost.clear()
        XCTAssertNil(p.selectionHost.selectWord(in: folder, at: try point(in: folder, of: "Check")), "the label's own words select nothing")
        // A handle UIKit drags into the run: the run comes whole.
        let held = try XCTUnwrap(p.selectionHost.text(for: folder))
        let at = { (offset: Int) in held.position(from: held.beginningOfDocument, offset: offset)! }
        held.selectedTextRange = held.textRange(from: at(10), to: at(12))
        XCTAssertEqual(held.selectedText, "/tmp/work")
    }

    func testUserSelectAllExpandsAcrossItsParagraphsFromEitherEnd() throws {
        for nested in [false, true] {
            let p = mounted([
                box(1, style: ["user_select": nested ? "text" : "all"]),
                box(4, style: ["user_select": "all"]), box(5, style: ["user_select": "text"]),
                text(2, "First paragraph"), text(3, "Second paragraph"), text(6, "Outside paragraph"),
                ["op": "children", "id": 1, "ids": [4, 6]], ["op": "children", "id": 4, "ids": [5, 3]],
                ["op": "children", "id": 5, "ids": [2]], ["op": "roots", "ids": [1]],
                frame(1, 0, h: 200, w: 400), frame(4, 0, h: 130), frame(5, 0, h: 60),
                frame(2, 30, x: 20), frame(3, 80, x: 20), frame(6, 150, x: 20)
            ])
            let expected = nested ? "First paragraph\n\nSecond paragraph" : "First paragraph\n\nSecond paragraph\n\nOutside paragraph"
            for id: UInt32 in [2, 3] {
                let paragraph = try node(p, id)
                p.selectionHost.clear()
                XCTAssertNotNil(p.selectionHost.selectWord(in: paragraph, at: try point(in: paragraph, of: "paragraph")))
                let input = try XCTUnwrap(p.selectionHost.text(for: paragraph))
                XCTAssertEqual(input.selectedText, expected, "starting in paragraph \(id), nested \(nested)")
            }
        }
    }

    func testAWordIsSelectedFromTheParagraphsOwnLinesAndCopied() throws {
        let p = page()
        let prose = try node(p, 4)
        var heard: [String] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append("\(id) \(text) \(start) \(end)") }
        XCTAssertTrue(prose.interactions.isEmpty, "nothing until a finger lands")
        // A finger lands: UIKit's interaction, in its read-only mode, on the paragraph's own view.
        p.selectionHost.touched(prose)
        let held = try XCTUnwrap(p.selectionHost.text(for: prose))
        XCTAssertTrue(prose.interactions.contains { $0 === held.interaction })
        XCTAssertEqual(held.interaction.textInteractionMode, .nonEditable)
        XCTAssertTrue(held.textInputView === prose)
        XCTAssertFalse(held.isEditable, "no keyboard, and a long press selects a word")
        XCTAssertFalse((held as AnyObject) is UIView, "the text input is no view")
        XCTAssertFalse(prose.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil), "nothing selected, nothing to copy")
        // Its single taps are off, so the node's own press hears a tap.
        let taps = (prose.gestureRecognizers ?? []).compactMap { $0 as? UITapGestureRecognizer }.filter { $0.numberOfTapsRequired == 1 }
        XCTAssertFalse(taps.isEmpty)
        XCTAssertTrue(taps.allSatisfy { !$0.isEnabled })
        XCTAssertTrue((prose.gestureRecognizers ?? []).allSatisfy { !$0.delaysTouchesEnded }, "the node's touch lifts when the finger does")

        let found = try XCTUnwrap(p.selectionHost.selectWord(in: prose, at: try point(in: prose, of: "brown")))
        XCTAssertEqual(found.text, "brown")
        XCTAssertEqual(found.range, NSRange(location: 10, length: 5))
        XCTAssertEqual(heard, ["4 brown 10 15"], "`selectionchange`, in the node's own text")
        XCTAssertTrue(held.isFirstResponder)
        XCTAssertTrue(taps.allSatisfy(\.isEnabled), "with a selection up a tap in the text clears it")
        // Where UIKit draws it is where the paragraph's line has the word.
        let range = try XCTUnwrap(held.selectedTextRange)
        let rects = held.selectionRects(for: range)
        XCTAssertEqual(rects.count, 1)
        let paragraph = try XCTUnwrap(prose.paragraphLayout()), boxed = prose.paragraphBox()
        let x0 = boxed.minX + paragraph.origin(0, align: 0, width: boxed.width) + CTLineGetOffsetForStringIndex(paragraph.lines[0], 10, nil)
        let x1 = boxed.minX + paragraph.origin(0, align: 0, width: boxed.width) + CTLineGetOffsetForStringIndex(paragraph.lines[0], 15, nil)
        XCTAssertEqual(rects[0].rect.minX, x0, accuracy: 0.01)
        XCTAssertEqual(rects[0].rect.maxX, x1, accuracy: 0.01)
        XCTAssertTrue(prose.bounds.insetBy(dx: -1, dy: -4).contains(rects[0].rect), "\(rects[0].rect) in \(prose.bounds)")
        XCTAssertTrue(rects[0].containsStart && rects[0].containsEnd)
        XCTAssertEqual(rects[0].writingDirection, .leftToRight)
        XCTAssertEqual(held.text(in: range), "brown")
        XCTAssertEqual(held.caretRect(for: range.start).midX, x0, accuracy: 0.01)

        // The edit menu asks the node for Copy and Select All (while not all selected).
        XCTAssertTrue(prose.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        XCTAssertTrue(prose.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil))
        XCTAssertFalse(prose.canPerformAction(#selector(UIResponderStandardEditActions.cut(_:)), withSender: nil))
        XCTAssertFalse(prose.canPerformAction(#selector(UIResponderStandardEditActions.paste(_:)), withSender: nil))
        // (A unit test's process waits forever on the system pasteboard's contents, so Copy
        // writes to one of the test's own here; `simctl pbpaste` read the system's after a real Copy.)
        let pasteboard = UIPasteboard.withUniqueName()
        defer { UIPasteboard.remove(withName: pasteboard.name) }
        p.selectionHost.pasteboard = pasteboard
        pasteboard.string = "something else"
        prose.copy(nil)
        XCTAssertEqual(pasteboard.string, "brown")

        // UIKit's tap in the text clears it (it sets no range).
        held.selectedTextRange = nil
        XCTAssertNil(p.selectionHost.selection)
        XCTAssertEqual(heard.last, "4  0 0")
        XCTAssertTrue(taps.allSatisfy { !$0.isEnabled })
        XCTAssertFalse(prose.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
    }

    func testTheSelectionSurvivesAScrollAndIsDroppedWhenItsTextChanges() throws {
        let p = page()
        let prose = try node(p, 4)
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: prose, at: try point(in: prose, of: "quick")))
        let held = try XCTUnwrap(p.selectionHost.text(for: prose))
        let before = try XCTUnwrap(held.selectionRects(for: try XCTUnwrap(held.selectedTextRange)).first?.rect)
        let onScreen = prose.convert(before, to: nil)
        // The page scrolls: the selection is the paragraph's, and moves with it.
        p.viewport.contentSize = CGSize(width: 400, height: 1600)
        p.viewport.contentOffset = CGPoint(x: 0, y: 60)
        p.viewport.layoutIfNeeded()
        XCTAssertEqual(p.selectionHost.selection?.text, "quick")
        let after = try XCTUnwrap(held.selectionRects(for: try XCTUnwrap(held.selectedTextRange)).first?.rect)
        XCTAssertEqual(after, before)
        XCTAssertEqual(prose.convert(after, to: nil).minY, onScreen.minY - 60, accuracy: 0.01)
        // A restyle that leaves the words alone leaves the selection.
        p.apply(wireBatch([["op": "style", "id": 4, "style": ["font_size": 17.0, "line_height": "22px", "text_color": [200, 0, 0, 255]]]]))
        XCTAssertEqual(p.selectionHost.selection?.text, "quick")
        // Other text changing leaves it too.
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["text": "Another label."]]]))
        XCTAssertEqual(p.selectionHost.selection?.text, "quick")
        // Its own text changing drops it.
        var heard: [String] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append("\(id) \(text) \(start) \(end)") }
        p.apply(wireBatch([["op": "props", "id": 4, "set": ["text": "Other words stand here now."]]]))
        XCTAssertNil(p.selectionHost.selection)
        XCTAssertNil(held.selectedTextRange)
        XCTAssertEqual(heard, ["4  0 0"])
        // And the new words select.
        XCTAssertEqual(p.selectionHost.selectWord(in: prose, at: try point(in: prose, of: "stand"))?.text, "stand")
        // Text that stops being selectable gives its interaction back.
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["user_select": "none"]]]))
        XCTAssertNil(p.selectionHost.selection)
        XCTAssertTrue(p.selectionHost.texts.isEmpty)
        XCTAssertTrue(prose.interactions.isEmpty)
        XCTAssertTrue((prose.gestureRecognizers ?? []).isEmpty)
    }

    func testOneSelectionAndItGoesWithItsNode() throws {
        let p = page()
        let prose = try node(p, 4), rowText = try node(p, 7)
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: prose, at: try point(in: prose, of: "lazy")))
        XCTAssertEqual(p.selectionHost.selectWord(in: rowText, at: try point(in: rowText, of: "pressable"))?.text, "pressable")
        XCTAssertNil(p.selectionHost.text(for: prose)?.selectedTextRange, "one selection on the page")
        XCTAssertTrue(p.selectionHost.selection?.node === rowText)
        p.apply(wireBatch([["op": "destroy", "id": 7], ["op": "children", "id": 6, "ids": [8]]]))
        XCTAssertNil(p.selectionHost.selection)
        XCTAssertNil(p.selectionHost.text(for: rowText))
    }

    /// A touch whose hit view the test names (UIKit makes none for a test).
    private final class Touch: UITouch {
        let hit: UIView
        var point: CGPoint?
        init(_ hit: UIView) { self.hit = hit; super.init() }
        override var view: UIView? { hit }
        override var window: UIWindow? { hit.window }
        override func location(in view: UIView?) -> CGPoint { hit.convert(point ?? CGPoint(x: hit.bounds.midX, y: hit.bounds.midY), to: view) }
    }

    private final class Hold: UILongPressGestureRecognizer {
        var phase = UIGestureRecognizer.State.possible
        override var state: UIGestureRecognizer.State { get { phase } set { phase = newValue } }
    }

    func testARowsContextMenuWinsALongPressButDoubleTapStillSelectsText() throws {
        let p = page()
        let row = try node(p, 6), selectable = try node(p, 7), label = try node(p, 8)
        let longPress = try XCTUnwrap(row.contextRecognizer, "the row's `contextmenu` is a long press")
        XCTAssertTrue(row.gestureRecognizer(longPress, shouldReceive: Touch(selectable)))
        XCTAssertTrue(row.gestureRecognizer(longPress, shouldReceive: Touch(label)))
        XCTAssertTrue(row.gestureRecognizer(longPress, shouldReceive: Touch(row)))
        let at = selectable.convert(try point(in: selectable, of: "words"), to: nil)
        let held = try XCTUnwrap(p.selectionHost.text(for: selectable))
        XCTAssertFalse(held.interaction.gesturesForFailureRequirements.isEmpty)
        for gesture in held.interaction.gesturesForFailureRequirements {
            XCTAssertTrue(row.gestureRecognizer(longPress, shouldBeRequiredToFailBy: gesture), "the row is tried before selection")
        }
        XCTAssertNil(p.textSelection.agentSelect(selectable, at: at, event: "contextmenu"))
        XCTAssertNil(p.selectionHost.selection)
        var menus: [UInt32] = []
        p.onClipboard = { id, kind, _ in if kind == 10 { menus.append(id) } }
        let hold = Hold()
        for phase in [UIGestureRecognizer.State.began, .changed, .ended] { hold.phase = phase; row.openContext(hold) }
        XCTAssertEqual(menus, [6], "one action for the recognized long press")
        XCTAssertNil(p.selectionHost.selection)
        XCTAssertEqual(p.textSelection.agentSelect(selectable, at: at, event: "dblclick")?["text"] as? String, "words")
        p.selectionHost.clear()
        let prose = try node(p, 4)
        let word = prose.convert(try point(in: prose, of: "brown"), to: nil)
        XCTAssertEqual(p.textSelection.agentSelect(prose, at: word, event: "contextmenu")?["text"] as? String, "brown", "without a row menu a long press selects")
        XCTAssertFalse(row.gestureRecognizer(longPress, shouldReceive: Touch(UITextView())))
        XCTAssertFalse(row.gestureRecognizer(longPress, shouldReceive: Touch(UITextField())))
    }

    func testMenuPrecedenceIgnoresDisabledContainersButHonorsInertAncestry() throws {
        let p = page(), row = try node(p, 6), paragraph = try node(p, 7)
        let at = paragraph.convert(try point(in: paragraph, of: "words"), to: nil)
        let held = try XCTUnwrap(p.selectionHost.text(for: paragraph))
        let selectionGesture = try XCTUnwrap(held.interaction.gesturesForFailureRequirements.first)
        let rowPress = try XCTUnwrap(row.contextRecognizer)
        paragraph.handlers = ["contextmenu"]; paragraph.updateContextGestures()
        let ownPress = try XCTUnwrap(paragraph.contextRecognizer)
        XCTAssertTrue(paragraph.contextMenuSource === paragraph)
        XCTAssertFalse(row.gestureRecognizer(rowPress, shouldReceive: Touch(paragraph)))
        XCTAssertTrue(paragraph.gestureRecognizer(ownPress, shouldBeRequiredToFailBy: selectionGesture))
        XCTAssertFalse(row.gestureRecognizer(rowPress, shouldBeRequiredToFailBy: selectionGesture))
        paragraph.handlers = []; paragraph.updateContextGestures()
        XCTAssertTrue(paragraph.contextMenuSource === row)
        XCTAssertTrue(row.gestureRecognizer(rowPress, shouldBeRequiredToFailBy: selectionGesture))
        row.handlers = ["press"]; row.updateContextGestures()
        XCTAssertNil(paragraph.contextMenuSource)
        XCTAssertEqual(p.textSelection.agentSelect(paragraph, at: at, event: "contextmenu")?["text"] as? String, "words")
        p.selectionHost.clear()
        row.handlers.insert("contextmenu"); row.updateContextGestures()
        let replaced = try XCTUnwrap(row.contextRecognizer)
        XCTAssertTrue(p.selectionHost.text(for: paragraph) === held, "the same text interaction follows the new menu")
        XCTAssertTrue(row.gestureRecognizer(replaced, shouldBeRequiredToFailBy: selectionGesture))
        for id in [6, 1] as [UInt32] {
            let ancestor = try node(p, id)
            ancestor.props["disabled"] = "true"
            XCTAssertTrue(paragraph.contextMenuSource === row)
            XCTAssertTrue(row.gestureRecognizer(replaced, shouldReceive: Touch(paragraph)))
            XCTAssertTrue(row.gestureRecognizer(replaced, shouldBeRequiredToFailBy: selectionGesture))
            ancestor.props["disabled"] = nil
            ancestor.props["inert"] = "true"
            XCTAssertNil(paragraph.contextMenuSource)
            XCTAssertNil(p.textSelection.agentSelect(paragraph, at: at, event: "contextmenu"))
            XCTAssertFalse(row.gestureRecognizer(replaced, shouldReceive: Touch(paragraph)))
            ancestor.props["inert"] = nil
        }
    }

    func testARowsNativeContextPopoverAdmitsSelectableTextAndFiresOnce() throws {
        if ExactEnv.agentMode { throw XCTSkip("under the agent the popover is painted (D4)") }
        let p = page(), row = try node(p, 6), paragraph = try node(p, 7)
        p.apply(wireBatch([
            box(20, props: ["id": "row-menu", "popover": "auto"]),
            box(21, props: ["accessibilityLabel": "Pin"], handlers: ["press"], kind: "button"),
            text(22, "Pin"), ["op": "children", "id": 21, "ids": [22]],
            ["op": "children", "id": 20, "ids": [21]], ["op": "children", "id": 1, "ids": [2, 3, 6, 20]],
            ["op": "props", "id": 6, "set": ["contextPopover": "row-menu"]], frame(20, 0, h: 44, w: 220), frame(21, 0, h: 44, w: 220),
        ]))
        let host = p.menus.context
        let interaction = try XCTUnwrap(row.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        let local = try point(in: paragraph, of: "words"), at = paragraph.convert(local, to: row)
        XCTAssertNil(p.textSelection.agentSelect(paragraph, at: paragraph.convert(local, to: nil), event: "contextmenu"))
        var menus: [UInt32] = []
        p.onClipboard = { id, kind, _ in if kind == 10 { menus.append(id) } }
        let configuration = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: at))
        let owner = try XCTUnwrap(host.open)
        XCTAssertEqual(try XCTUnwrap(host.menu(owner)).children.first?.title, "Pin")
        _ = host.menu(owner); _ = host.lift(owner)
        XCTAssertEqual(menus, [6])
        XCTAssertNil(p.selectionHost.selection)
        host.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
        let readOnly = UITextView(); readOnly.isEditable = false
        for editor in [UITextField(), UITextView(), readOnly] as [UIView] {
            editor.frame = row.bounds; row.container.addSubview(editor)
            XCTAssertTrue(row.hitsNativeEditor(row.hitTest(at, with: nil)))
            XCTAssertNil(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: at), "a native editor keeps its long press")
            editor.removeFromSuperview()
        }
        paragraph.handlers = ["contextmenu"]; paragraph.updateContextGestures()
        XCTAssertNil(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: at), "the paragraph's own menu stays nearest")
        paragraph.handlers = []; paragraph.updateContextGestures()
        row.handlers = ["press"]; row.updateContextGestures(); p.menus.sync()
        XCTAssertTrue(paragraph.contextMenuSource === row, "a popover needs no contextmenu action")
        XCTAssertNil(p.textSelection.agentSelect(paragraph, at: paragraph.convert(local, to: nil), event: "contextmenu"))
        let popoverOnly = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: at))
        _ = host.menu(try XCTUnwrap(host.open))
        XCTAssertEqual(menus, [6], "opening the popover alone invents no action")
        host.contextMenuInteraction(interaction, willEndFor: popoverOnly, animator: nil)
    }

    func testRightToLeftTextAndLargerTypeSelectTheirOwnLines() throws {
        let arabic = "هذا نص عربي قصير للتجربة with English words داخل السطر نفسه حتى نرى التحديد."
        let long = "Dynamic Type makes every line taller and breaks this sentence over more of them than before."
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, arabic, style: ["direction": "rtl"]),
            text(3, long),
            text(4, long, style: ["font_size": 34.0, "line_height": "44px"]),
            ["op": "children", "id": 1, "ids": [2, 3, 4]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 800, w: 400), frame(2, 80, h: 44, x: 20), frame(3, 140, h: 66, x: 20), frame(4, 220, h: 400, x: 20),
        ])
        // Right to left: a word of the Arabic starts at its right edge, an English one at its left.
        let rtl = try node(p, 2)
        XCTAssertEqual(p.selectionHost.selectWord(in: rtl, at: try point(in: rtl, of: "عربي"))?.text, "عربي")
        let held = try XCTUnwrap(p.selectionHost.text(for: rtl))
        var rect = try XCTUnwrap(held.selectionRects(for: try XCTUnwrap(held.selectedTextRange)).first)
        XCTAssertEqual(rect.writingDirection, .rightToLeft)
        XCTAssertGreaterThan(rect.rect.midX, rtl.bounds.midX, "the paragraph starts at the right")
        XCTAssertEqual(p.selectionHost.selectWord(in: rtl, at: try point(in: rtl, of: "English"))?.text, "English")
        rect = try XCTUnwrap(held.selectionRects(for: try XCTUnwrap(held.selectedTextRange)).first)
        XCTAssertEqual(rect.writingDirection, .leftToRight)
        XCTAssertEqual(held.baseWritingDirection(for: held.beginningOfDocument, in: .forward), .rightToLeft)

        // Larger type: all of the paragraph selected is one rect a line, on that line, at that line's height.
        for (id, size) in [(3, 17.0), (4, 34.0)] as [(UInt32, Double)] {
            let node = try node(p, id)
            let input = p.selectionHost.install(node)
            let base = input.document?.parts.first { $0.node === node }?.start ?? 0
            input.selectedTextRange = input.textRange(from: try XCTUnwrap(input.position(from: input.beginningOfDocument, offset: base)),
                                                     to: try XCTUnwrap(input.position(from: input.beginningOfDocument, offset: base + (long as NSString).length)))
            XCTAssertEqual(p.selectionHost.selection?.text, long, "font-size \(size)")
            let rects = input.selectionRects(for: try XCTUnwrap(input.selectedTextRange)).map { input.textInputView.convert($0.rect, to: node) }
            let paragraph = try XCTUnwrap(node.paragraphLayout())
            XCTAssertEqual(rects.count, paragraph.lines.count, "font-size \(size)")
            XCTAssertGreaterThan(rects.count, size > 20 ? 3 : 1, "font-size \(size)")
            for (i, rect) in rects.enumerated() {
                XCTAssertEqual(rect.maxY - rect.height / 2, paragraph.baselines[i] - CGFloat(size) * 0.3, accuracy: CGFloat(size) * 0.5, "line \(i) at \(size)")
                XCTAssertGreaterThan(rect.height, CGFloat(size), "line \(i) at \(size)")
                if i > 0 { XCTAssertGreaterThan(rect.minY, rects[i - 1].minY) }
            }
            // A point on the last line is a caret on the last line.
            let last = rects[rects.count - 1]
            let caret = try XCTUnwrap(input.closestPosition(to: node.convert(CGPoint(x: last.midX, y: last.midY), to: input.textInputView)))
            XCTAssertEqual(input.textInputView.convert(input.caretRect(for: caret), to: node).midY, last.midY, accuracy: 1, "font-size \(size)")
        }
    }

    func testVoiceOverStillReadsSelectableTextAsText() throws {
        let p = page()
        let prose = try node(p, 4), plain = try node(p, 2)
        let label = prose.accessibilityLabel, traits = prose.accessibilityTraits
        XCTAssertTrue(prose.isAccessibilityElement)
        XCTAssertTrue(traits.contains(.staticText))
        XCTAssertEqual(label, "The quick brown fox jumps over the lazy dog.")
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: prose, at: try point(in: prose, of: "fox")))
        XCTAssertTrue(prose.isAccessibilityElement)
        XCTAssertEqual(prose.accessibilityLabel, label)
        XCTAssertEqual(prose.accessibilityTraits, traits)
        XCTAssertNil(prose.accessibilityElements, "the paragraph is the element; the text input adds none")
        XCTAssertEqual(plain.accessibilityTraits, traits, "selectable or not, the same text to VoiceOver")
    }

    /// What it costs (LLP 1115.000 D11.6's as-built row): a page of 400
    /// paragraphs holds no interaction, recognizer or view for selection,
    /// whether or not its author made it selectable, until a finger lands;
    /// then the paragraphs touched hold UIKit's, at most `kept` of them.
    func testAPageOfFourHundredParagraphsPaysOnlyForWhatAFingerTouches() throws {
        func build(selectable: Bool) -> Presenter {
            var ops: [[String: Any]] = [box(1), box(2, style: selectable ? ["user_select": "text"] : [:])]
            for i in 0..<400 { ops.append(text(100 + i, "Paragraph \(i) of the page, which is a line of ordinary words.")) }
            ops += [["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": (0..<400).map { 100 + $0 }], ["op": "roots", "ids": [1]],
                    frame(1, 0, h: 9000, w: 400), frame(2, 0, h: 9000, x: 20)]
            for i in 0..<400 { ops.append(frame(100 + i, Double(i) * 22)) }
            return mounted(ops)
        }
        func count(_ p: Presenter) -> (views: Int, interactions: Int, recognizers: Int) {
            let nodes: [UIView] = (0..<400).compactMap { p.views[UInt32(100 + $0)] } + p.selectionHost.texts.compactMap { $0.document?.surface }
            var views = 0
            func walk(_ view: UIView) { views += 1; view.subviews.forEach(walk) }
            walk(p.root)
            return (views, nodes.reduce(0) { $0 + $1.interactions.count }, nodes.reduce(0) { $0 + ($1.gestureRecognizers?.count ?? 0) })
        }
        // Nothing said.
        var p = build(selectable: false)
        let unsaid = count(p)
        let clock = ContinuousClock()
        var spent = clock.measure { for _ in 0..<5 { for i in 0..<400 { p.views[UInt32(100 + i)]?.touchedText() } } }
        let untouched = count(p)
        XCTAssertEqual(unsaid.interactions, 0); XCTAssertEqual(unsaid.recognizers, 0)
        XCTAssertEqual(untouched.views, unsaid.views); XCTAssertEqual(untouched.interactions, 0); XCTAssertEqual(untouched.recognizers, 0)
        print("text selection cost, 400 paragraphs nobody made selectable: \(unsaid.views) views, \(unsaid.interactions) interactions, \(unsaid.recognizers) recognizers; after a touch on each: \(untouched.views) views, \(untouched.interactions) interactions, \(untouched.recognizers) recognizers; the hit test's question \(spent / 2000) a touch")
        session?.destroy(); window?.isHidden = true

        // Made selectable.
        p = build(selectable: true)
        let said = count(p)
        XCTAssertEqual(said.views, unsaid.views, "saying it adds no view")
        XCTAssertEqual(said.interactions, 0); XCTAssertEqual(said.recognizers, 0)
        spent = clock.measure { p.views[100]?.touchedText() }
        let one = count(p)
        XCTAssertEqual(one.views, said.views + 1, "one shared surface, added on the first touch")
        XCTAssertGreaterThan(one.interactions, 0); XCTAssertGreaterThan(one.recognizers, 0)
        for i in 1..<400 { p.views[UInt32(100 + i)]?.touchedText() }
        let all = count(p)
        XCTAssertEqual(p.selectionHost.texts.count, 1)
        XCTAssertEqual(all.interactions, one.interactions)
        XCTAssertEqual(all.recognizers, one.recognizers)
        XCTAssertEqual(all.views, one.views)
        print("text selection cost, 400 paragraphs made selectable: \(said.views) views, \(said.interactions) interactions, \(said.recognizers) recognizers; after one touch: \(one.interactions) interactions, \(one.recognizers) recognizers (\(spent) to add them); after a touch on each of the 400: \(all.interactions) interactions, \(all.recognizers) recognizers on \(p.selectionHost.texts.count) paragraphs, \(all.views) views")
        // A selection is not taken away to make room.
        let chosen = try node(p, 399)
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: chosen, at: CGPoint(x: 30, y: 11)))
        for i in 0..<20 { p.views[UInt32(100 + i)]?.touchedText() }
        XCTAssertNotNil(p.selectionHost.text(for: chosen)?.selectedTextRange)
        XCTAssertEqual(p.selectionHost.texts.count, 1)
    }
    func testAnAgentTapClearsASelectionWithoutPressingTheRow() throws {
        let p = page()
        let paragraph = try node(p, 7)
        let point = try point(in: paragraph, of: "Selectable")
        XCTAssertNotNil(p.selectionHost.selectWord(in: paragraph, at: point))
        var presses: [UInt32] = []
        p.onPress = { presses.append($0) }
        let reply = Agent(session: try XCTUnwrap(session)).tap(["id": 7])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertNil(p.selectionHost.selection)
        XCTAssertTrue(presses.isEmpty)
    }

    func testMixedDirectionEndpointsSurviveParagraphAndDocumentConversion() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]), text(2, "Before"),
            text(3, "אבג abc דהו", style: ["direction": "rtl"]), text(4, "After"),
            ["op": "children", "id": 1, "ids": [2, 3, 4]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 20, x: 20), frame(3, 70, x: 20), frame(4, 120, x: 20),
        ])
        let owner = try node(p, 1), prose = try node(p, 3)
        let input = SelectableText(node: prose, host: p.selectionHost, attach: false)
        let selection = NSRange(location: 1, length: 5)
        let local = input.selectionRects(for: SelectableRange(selection))
        let start = try XCTUnwrap(local.first { $0.containsStart })
        let end = try XCTUnwrap(local.first { $0.containsEnd })
        XCTAssertEqual(local.filter { $0.containsStart }.count, 1)
        XCTAssertEqual(local.filter { $0.containsEnd }.count, 1)
        XCTAssertEqual(start.writingDirection, .rightToLeft)
        XCTAssertEqual(end.writingDirection, .leftToRight)
        XCTAssertGreaterThan(start.rect.minX, end.rect.maxX)
        // The first rect encloses the selection's first line, its start included.
        let first = input.firstRect(for: SelectableRange(selection))
        XCTAssertEqual(first.maxX, start.rect.maxX, accuracy: 0.001)
        XCTAssertEqual(first.minX, local.map(\.rect.minX).min()!, accuracy: 0.001)

        let document = SelectionDocument(owner: owner, paragraphs: try [node(p, 2), prose, node(p, 4)], host: p.selectionHost)
        let base = try XCTUnwrap(document.parts.first { $0.node === prose }).start
        let global = NSRange(location: base + selection.location, length: selection.length)
        let converted = document.rects(global)
        XCTAssertEqual(converted.count, local.count)
        for (a, b) in zip(local, converted) {
            XCTAssertEqual(b.rect, prose.convert(a.rect, to: document.surface))
            XCTAssertEqual(b.containsStart, a.containsStart)
            XCTAssertEqual(b.containsEnd, a.containsEnd)
            XCTAssertEqual(b.writingDirection, a.writingDirection)
        }
        let across = document.rects(NSRange(location: 1, length: base + NSMaxRange(selection) - 1))
        XCTAssertEqual(across.filter { $0.containsStart }.count, 1)
        XCTAssertEqual(across.filter { $0.containsEnd }.count, 1)
        XCTAssertEqual(across.first { $0.containsEnd }?.rect, prose.convert(end.rect, to: document.surface))
    }

    func testAClampedParagraphKeepsTheEndOfASelectionPastItsLastLine() throws {
        let p = mounted([text(1, "First\nSecond\nThird", style: ["white_space": "pre", "line_clamp": 1, "user_select": "all"]),
                         ["op": "roots", "ids": [1]], frame(1, 0)])
        let input = SelectableText(node: try node(p, 1), host: p.selectionHost, attach: false)
        XCTAssertEqual(input.offset(from: input.beginningOfDocument, to: input.endOfDocument), 18)
        let rects = input.selectionRects(for: SelectableRange(NSRange(location: 0, length: 18)))
        XCTAssertEqual(rects.filter(\.containsStart).count, 1)
        let ends = rects.filter(\.containsEnd)
        XCTAssertEqual(ends.count, 1, "the end handle has a place though no shown line holds the end")
        let caret = input.caretRect(for: SelectablePosition(18))
        XCTAssertEqual(try XCTUnwrap(ends.first).rect, CGRect(x: caret.midX, y: caret.minY, width: 0, height: caret.height))
    }

    func testFirstRectStaysOnTheStartingLineInItsColumn() throws {
        let p = mounted([text(1, "Alpha\nbeta\ngamma\ndelta", style: ["white_space": "pre", "user_select": "text"]),
                         ["op": "roots", "ids": [1]], frame(1, 0, h: 44, w: 300),
                         ["op": "fragments", "id": 1, "fragments": [[0, 0, 120, 44, 0, 2, 0, 0], [160, 0, 120, 44, 2, 4, 160, -44]]]])
        let input = SelectableText(node: try node(p, 1), host: p.selectionHost, attach: false)
        let range = SelectableRange(NSRange(location: 0, length: input.offset(from: input.beginningOfDocument, to: input.endOfDocument)))
        let rects = input.selectionRects(for: range)
        let start = try XCTUnwrap(rects.first { $0.containsStart })
        XCTAssertTrue(rects.contains { $0.rect.minX >= 160 && $0.rect.minY == start.rect.minY }, "gamma shares Alpha's band")
        XCTAssertEqual(input.firstRect(for: range), start.rect)
    }

    func testDocumentSelectionKeepsEndpointsAtParagraphSeparators() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]), text(2, "First"), text(3, "Second"),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 120, w: 400), frame(2, 20, x: 20), frame(3, 70, x: 40),
        ])
        let input = try XCTUnwrap(p.selectionHost.install(try node(p, 2)))
        let document = try XCTUnwrap(input.document)
        XCTAssertEqual(document.text as String, "First\n\nSecond")
        for range in [0..<7, 0..<6, 5..<10, 6..<10, 5..<7, 5..<6, 6..<7, 0..<5, 7..<10, 2..<10, 5..<5] {
            let selection = SelectableRange(NSRange(location: range.lowerBound, length: range.count))
            let rects = input.selectionRects(for: selection)
            if range.isEmpty {
                XCTAssertTrue(rects.isEmpty)
                continue
            }
            let starts = rects.filter { $0.containsStart }, ends = rects.filter { $0.containsEnd }
            XCTAssertEqual(starts.count, 1, "\(range)")
            XCTAssertEqual(ends.count, 1, "\(range)")
            for (offset, endpoint, first) in [(range.lowerBound, starts.first, true), (range.upperBound, ends.first, false)] {
                guard offset == 5 || offset == 6 || offset == 7 else { continue }
                let endpoint = try XCTUnwrap(endpoint, "\(range)")
                let caret = input.caretRect(for: SelectablePosition(offset))
                let x = first ? endpoint.rect.minX : endpoint.rect.maxX
                XCTAssertEqual(x, caret.midX, accuracy: 0.01, "\(range)")
                XCTAssertEqual(endpoint.rect.minY, caret.minY, accuracy: 0.01)
                XCTAssertEqual(endpoint.rect.height, caret.height, accuracy: 0.01)
                XCTAssertEqual(endpoint.writingDirection, .leftToRight)
            }
        }
    }

    func testOneTextInputSelectsAcrossParagraphsAndKeepsTheirSourceRanges() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "First paragraph", handlers: ["selectionchange"]),
            text(3, "Second paragraph", handlers: ["selectionchange"]),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 30, x: 20), frame(3, 80, x: 20)
        ])
        let first = try node(p, 2), second = try node(p, 3)
        p.selectionHost.touched(first)
        let input = try XCTUnwrap(p.selectionHost.text(for: first))
        let range = try XCTUnwrap(input.textRange(from: input.beginningOfDocument, to: input.endOfDocument))
        XCTAssertEqual(input.text(in: range), "First paragraph\n\nSecond paragraph")
        var heard: [String] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append("\(id):\(text):\(start):\(end)") }
        input.selectedTextRange = range
        XCTAssertEqual(input.selectedText, "First paragraph\n\nSecond paragraph")
        XCTAssertEqual(heard, ["2:First paragraph:0:15", "3:Second paragraph:0:16"])
        let rects = input.selectionRects(for: range)
        XCTAssertEqual(rects.count, 2)
        XCTAssertTrue(rects.first?.containsStart == true)
        XCTAssertTrue(rects.last?.containsEnd == true)
        XCTAssertEqual(input.caretRect(for: range.end).midY,
                       second.convert(CGPoint(x: 0, y: second.bounds.midY), to: input.textInputView).y, accuracy: 3)
        let pasteboard = UIPasteboard.withUniqueName()
        defer { UIPasteboard.remove(withName: pasteboard.name) }
        p.selectionHost.pasteboard = pasteboard
        first.copy(nil)
        XCTAssertEqual(pasteboard.string, "First paragraph\n\nSecond paragraph")
        p.apply(wireBatch([["op": "props", "id": 3, "set": ["text": "Changed"]]]))
        XCTAssertNil(p.selectionHost.selection)
    }

    private final class TextTouchEvent: UIEvent {
        override var type: UIEvent.EventType { .touches }
    }

    func testInlineContextMenusKeepTheirParagraphOnTheTextTouchPath() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "First paragraph"), text(3, "Second paragraph"),
            ["op": "paragraph", "id": 2, "runs": [
                ["id": 21, "parent": 2, "paint": true, "props": ["text": "First"], "handlers": ["contextmenu"]],
                ["id": 22, "parent": 2, "paint": true, "props": ["text": " paragraph"]]
            ]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 30, x: 20), frame(3, 80, x: 20)
        ])
        let first = try node(p, 2)
        let at = try point(in: first, of: "First")
        XCTAssertNotNil(first.inlineContextTarget(at: at))
        let recognizer = try XCTUnwrap(first.contextRecognizer)
        let hit = try XCTUnwrap(first.hitTest(at, with: TextTouchEvent()))
        XCTAssertTrue(hit === first, "a menu's paragraph must receive UIKit's text touch")
        XCTAssertTrue(sequence(first: hit, next: { $0.superview }).contains { view in
            view.gestureRecognizers?.contains { $0 === recognizer } == true
        }, "UIKit delivers the long press along this physical path")
        XCTAssertNil(p.selectionHost.text(for: first)?.document, "context-menu prose keeps its paragraph input")
    }

    func testSharedTextTouchesDeliverPointerEventsThroughTheParagraph() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"], handlers: ["pointerdown", "pointerup"]),
            text(2, "First paragraph", handlers: ["pointerdown", "pointermove", "pointerup"]),
            text(3, "Second paragraph"),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 30, x: 20), frame(3, 80, x: 20)
        ])
        let first = try node(p, 2), event = TextTouchEvent()
        let hit = try XCTUnwrap(first.hitTest(CGPoint(x: 30, y: 10), with: event))
        let touch = Touch(hit)
        touch.point = first.convert(CGPoint(x: 30, y: 10), to: hit)
        // UIKit sends recognizer events only up the hit view's physical ancestry.
        let observers = sequence(first: hit, next: { $0.superview }).flatMap { view in
            (view.gestureRecognizers ?? []).compactMap { $0 as? PointerRecognizer }
        }
        var heard: [String] = []
        p.onPointer = { id, kind, _ in heard.append("\(id):\(kind)") }
        observers.forEach { $0.touchesBegan([touch], with: event) }
        observers.forEach { $0.touchesMoved([touch], with: event) }
        observers.forEach { $0.touchesEnded([touch], with: event) }
        XCTAssertEqual(heard, ["2:down", "1:down", "2:move", "2:up", "1:up"])
        first.handlers = []
        first.syncPointerRecognizer()
        heard = []
        observers.forEach { $0.reset() }
        let next = Touch(hit)
        next.point = touch.point
        observers.forEach { $0.touchesBegan([next], with: event) }
        observers.forEach { $0.touchesEnded([next], with: event) }
        XCTAssertEqual(heard, ["1:down", "1:up"], "an ancestor already on the physical touch path hears it once")
        first.handlers = ["pointermove"]
        first.syncPointerRecognizer()
        heard = []
        observers.forEach { $0.reset() }
        let moved = Touch(hit)
        moved.point = touch.point
        observers.forEach { $0.touchesBegan([moved], with: event) }
        observers.forEach { $0.touchesMoved([moved], with: event) }
        observers.forEach { $0.touchesEnded([moved], with: event) }
        XCTAssertEqual(heard, ["1:down", "2:move", "1:up"], "a move-only paragraph still owns the observer")
    }

    /// A held touch that ended without its end reaching the shared input's
    /// observer (a menu took it) is lifted before the next touch, which then
    /// resolves its own paragraph: the second paragraph hears it, not the first.
    func testATouchAfterAnUnseenEndResolvesItsOwnParagraph() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "First paragraph", handlers: ["pointerdown", "pointerup"]),
            text(3, "Second paragraph", handlers: ["pointerdown", "pointerup"]),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 30, x: 20), frame(3, 80, x: 20)
        ])
        let first = try node(p, 2), second = try node(p, 3), event = TextTouchEvent()
        let hit = try XCTUnwrap(first.hitTest(CGPoint(x: 30, y: 10), with: event))
        let observers = sequence(first: hit, next: { $0.superview }).flatMap { view in
            (view.gestureRecognizers ?? []).compactMap { $0 as? PointerRecognizer }
        }
        var heard: [String] = []
        p.onPointer = { id, kind, _ in heard.append("\(id):\(kind)") }
        let opened = EndingTouch(hit)
        opened.point = first.convert(CGPoint(x: 30, y: 10), to: hit)
        observers.forEach { $0.touchesBegan([opened], with: event) }
        XCTAssertEqual(heard, ["2:down"])
        opened.lifted = true
        let next = EndingTouch(hit)
        next.point = second.convert(CGPoint(x: 30, y: 10), to: hit)
        observers.forEach { $0.touchesBegan([next], with: event) }
        observers.forEach { $0.touchesEnded([next], with: event) }
        XCTAssertEqual(heard, ["2:down", "2:up", "3:down", "3:up"])
    }

    func testTheSharedInputTakesTextTouchesAndForwardsAPress() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "First paragraph", style: ["user_select": "text"], handlers: ["press"]),
            text(3, "Second paragraph"),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 30, x: 20), frame(3, 80, x: 20)
        ])
        let first = try node(p, 2), second = try node(p, 3)
        let event = TextTouchEvent()
        let hit = try XCTUnwrap(first.hitTest(CGPoint(x: 30, y: 10), with: event))
        let input = try XCTUnwrap(p.selectionHost.text(for: first))
        XCTAssertTrue(hit === input.textInputView)
        XCTAssertTrue(second.hitTest(CGPoint(x: 30, y: 10), with: event) === hit)
        XCTAssertTrue(hit.interactions.contains { $0 === input.interaction })
        XCTAssertGreaterThan(hit.layer.zPosition, first.layer.zPosition, "the shared surface paints above child backgrounds")
        var presses: [UInt32] = []
        p.onPress = { presses.append($0) }
        let touch = Touch(hit)
        touch.point = first.convert(CGPoint(x: 30, y: 10), to: hit)
        hit.touchesBegan([touch], with: event)
        hit.touchesEnded([touch], with: event)
        XCTAssertEqual(presses, [2])
        for clearsBeforeLift in [false, true] {
            XCTAssertNotNil(p.selectionHost.selectWord(in: first, at: try point(in: first, of: "paragraph")))
            let selectedTouch = Touch(hit)
            selectedTouch.point = touch.point
            hit.touchesBegan([selectedTouch], with: event)
            XCTAssertFalse(first.pressed, "a selection-clearing tap does not arm a paragraph press")
            if clearsBeforeLift { input.clear() }
            hit.touchesEnded([selectedTouch], with: event)
            XCTAssertEqual(presses, [2], "selection clearing wins before or after touchesEnded")
            XCTAssertFalse(first.pressed)
            input.clear()
        }
    }

    func testMultiParagraphMarkdownAtomSelectionClampsToSourceBounds() throws {
        ExactMarkdown.install()
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            ["op": "create", "id": 2, "kind": "text",
             "props": ["text": "Hi [a](x:1)", "markup": "markdown", "testId": "md1",
                       "atoms": #"[{"name":"citation","link":"x:"}]"#,
                       "atomFaces": #"[{"source":"[a](x:1)","label":"Very Long Expanded Atom Label","icon":"","title":""}]"#],
             "style": ["font_size": 17.0, "line_height": "22px"],
             "handlers": ["selectionchange"]],
            text(3, "Second paragraph"),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 30, x: 20), frame(3, 80, x: 20),
        ])
        let first = try node(p, 2)
        let sourceLen = (first.paragraphText as NSString).length
        let shapedLen = SelectableText.shaped(first).length
        XCTAssertGreaterThan(shapedLen, sourceLen, "expanded atom shaped length exceeds source length")
        var heard: [(UInt32, String, Int, Int)] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append((id, text, start, end)) }

        let input = p.selectionHost.install(first)
        XCTAssertNotNil(input.document, "shared SelectionDocument spans both paragraphs")
        // Select purely in the trailing expanded region past sourceLen, then across the whole paragraph.
        let at = { (offset: Int) in input.position(from: input.beginningOfDocument, offset: offset)! }
        input.selectedTextRange = input.textRange(from: at(sourceLen + 2), to: at(shapedLen))
        input.selectedTextRange = input.textRange(from: at(0), to: at(shapedLen))
        XCTAssertTrue(heard.allSatisfy { $0.2 >= 0 && $0.3 <= sourceLen && $0.2 <= $0.3 })
        XCTAssertEqual(heard.last?.1, "Hi [a](x:1)")
        XCTAssertEqual(heard.last?.2, 0)
        XCTAssertEqual(heard.last?.3, sourceLen)

        // selectWord on the expanded atom word past sourceLen must clamp or return nil without trapping.
        input.clear()
        let picked = p.selectionHost.selectWord(in: first, at: try point(in: first, of: "Label"))
        if let picked {
            XCTAssertLessThanOrEqual(NSMaxRange(picked.range), sourceLen)
        }
    }

    func testSharedSurfacePaintsAboveOpaqueBlocksAndPassesTouchesThroughToChildren() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "First paragraph of a comment."),
            box(3, style: ["background_color": [246.0, 248.0, 250.0, 255.0]]),
            text(4, "let answer = 42;", style: ["white_space": "pre"]),
            box(5, handlers: ["press"]),
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 1, "ids": [2, 3, 5]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 240, w: 400), frame(2, 10, x: 20),
            frame(3, 50, h: 60, x: 20), frame(4, 16, h: 24, x: 16, w: 300),
            frame(5, 130, h: 40, x: 20, w: 120),
        ])
        let owner = try node(p, 1), codeBox = try node(p, 3), codeText = try node(p, 4), button = try node(p, 5)
        let event = TextTouchEvent()
        let hit = try XCTUnwrap(codeText.hitTest(CGPoint(x: 20, y: 10), with: event))
        let surface = try XCTUnwrap(p.selectionHost.surface(for: codeText))
        XCTAssertTrue(hit === surface, "touches on a nested paragraph still route to the shared surface")
        XCTAssertGreaterThan(surface.layer.zPosition, codeBox.layer.zPosition, "surface paints above opaque child boxes")
        let surfaceIdx = try XCTUnwrap(owner.container.subviews.firstIndex { $0 === surface })
        let codeIdx = try XCTUnwrap(owner.container.subviews.firstIndex { $0 === codeBox })
        XCTAssertGreaterThan(surfaceIdx, codeIdx)
        // Non-handle points fall through to child boxes and buttons.
        XCTAssertTrue(owner.hitTest(CGPoint(x: 40, y: 150), with: event) === button)
        XCTAssertTrue(owner.hitTest(CGPoint(x: 24, y: 54), with: event) === codeBox)
        // A selection handle subview on the surface wins hits even over an opaque box.
        let handle = UIView(frame: CGRect(x: 30, y: 60, width: 20, height: 20))
        surface.addSubview(handle)
        defer { handle.removeFromSuperview() }
        XCTAssertTrue(owner.hitTest(CGPoint(x: 40, y: 70), with: event) === handle)
    }

    func testSelectAllAndEditMenuWorkForSingleParagraphAndSharedDocument() throws {
        // Single paragraph
        let single = page()
        let prose = try node(single, 4)
        XCTAssertFalse(prose.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil))
        _ = try XCTUnwrap(single.selectionHost.selectWord(in: prose, at: try point(in: prose, of: "brown")))
        XCTAssertTrue(prose.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil))
        prose.selectAll(nil)
        XCTAssertEqual(single.selectionHost.selection?.text, "The quick brown fox jumps over the lazy dog.")
        XCTAssertFalse(prose.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil), "hidden once everything is selected")
        session?.destroy(); session = nil; window?.isHidden = true; window = nil

        // Shared SelectionDocument
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "First paragraph", handlers: ["selectionchange"]),
            text(3, "Second paragraph", handlers: ["selectionchange"]),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 30, x: 20), frame(3, 80, x: 20),
        ])
        let first = try node(p, 2)
        var heard: [String] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append("\(id):\(text):\(start):\(end)") }
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: first, at: try point(in: first, of: "First")))
        let surface = try XCTUnwrap(p.selectionHost.surface(for: first))
        XCTAssertTrue(surface.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        XCTAssertTrue(surface.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil))
        XCTAssertFalse(surface.canPerformAction(#selector(UIResponderStandardEditActions.cut(_:)), withSender: nil))
        XCTAssertFalse(surface.canPerformAction(#selector(UIResponderStandardEditActions.paste(_:)), withSender: nil))
        surface.selectAll(nil)
        XCTAssertEqual(p.selectionHost.text(for: first)?.selectedText, "First paragraph\n\nSecond paragraph")
        XCTAssertFalse(surface.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil))
        XCTAssertTrue(heard.contains("2:First paragraph:0:15"))
        XCTAssertTrue(heard.contains("3:Second paragraph:0:16"))
        let pasteboard = UIPasteboard.withUniqueName()
        defer { UIPasteboard.remove(withName: pasteboard.name) }
        p.selectionHost.pasteboard = pasteboard
        surface.copy(nil)
        XCTAssertEqual(pasteboard.string, "First paragraph\n\nSecond paragraph")
    }

    func testTappingOutsideAndRemoveAllRangesClearSelectionWhileScrollAndHandlesKeepIt() throws {
        let p = mounted([
            box(1),
            box(2, style: ["user_select": "text"]),
            text(3, "First paragraph", handlers: ["selectionchange"]),
            text(4, "Second paragraph", handlers: ["selectionchange"]),
            box(5, handlers: ["press"]),
            ["op": "children", "id": 2, "ids": [3, 4]],
            ["op": "children", "id": 1, "ids": [2, 5]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 400, w: 400), frame(2, 0, h: 140, w: 400),
            frame(3, 20, x: 20), frame(4, 70, x: 20), frame(5, 180, h: 40, x: 20, w: 120),
        ])
        let owner = try node(p, 2), first = try node(p, 3), outside = try node(p, 5)
        var heard: [String] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append("\(id):\(text):\(start):\(end)") }

        // Handle subview on the shared surface is owned by the selection.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: first, at: try point(in: first, of: "First")))
        let surface = try XCTUnwrap(p.selectionHost.surface(for: first))
        let handle = UIView(frame: CGRect(x: 10, y: 10, width: 20, height: 20))
        surface.addSubview(handle)
        XCTAssertTrue(p.textSelection.owns(handle))
        XCTAssertFalse(p.textSelection.owns(owner), "empty container padding is outside the paragraphs and handles")
        XCTAssertFalse(p.textSelection.owns(outside))
        handle.removeFromSuperview()

        // Cancelled touch (scroll pan) keeps the selection.
        let pan = Touch(outside)
        outside.touchesBegan([pan], with: TextTouchEvent())
        outside.touchesCancelled([pan], with: TextTouchEvent())
        XCTAssertTrue(p.textSelection.hasActiveSelection)

        // Tap on empty container ground clears the selection.
        let ground = Touch(owner)
        ground.point = CGPoint(x: 350, y: 120)
        owner.touchesBegan([ground], with: TextTouchEvent())
        owner.touchesEnded([ground], with: TextTouchEvent())
        XCTAssertFalse(p.textSelection.hasActiveSelection)
        XCTAssertEqual(heard.last, "3::0:0")

        // Tap on empty viewport ScrollView clears the selection.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: first, at: try point(in: first, of: "First")))
        let scrollTap = Touch(p.viewport)
        scrollTap.point = CGPoint(x: 200, y: 500)
        p.viewport.touchesEnded([scrollTap], with: TextTouchEvent())
        XCTAssertFalse(p.textSelection.hasActiveSelection)

        // Agent tap on outside button clears selection and still delivers the press.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: first, at: try point(in: first, of: "First")))
        var presses: [UInt32] = []
        p.onPress = { presses.append($0) }
        let reply = Agent(session: try XCTUnwrap(session)).tap(["id": 5])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertFalse(p.textSelection.hasActiveSelection)
        XCTAssertEqual(presses, [5])

        // Contract removeAllRanges() command clears selection on iOS.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: first, at: try point(in: first, of: "First")))
        XCTAssertTrue(p.textSelection.hasActiveSelection)
        session?.apply(wireBatch([["op": "command", "name": "removeAllRanges", "args": [Any]()]]))
        let deadline = Date().addingTimeInterval(1.0)
        while p.textSelection.hasActiveSelection && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.005))
        }
        XCTAssertFalse(p.textSelection.hasActiveSelection)
        XCTAssertEqual(heard.last, "3::0:0")
    }

    func testSelectAllExcludesNonselectableTextInSharedDocument() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Prefix TOKEN Suffix"],
             "style": ["user_select": "none", "font_size": 17.0, "line_height": "22px"],
             "handlers": ["selectionchange"]],
            ["op": "paragraph", "id": 2, "runs": [
                ["id": 21, "parent": 2, "paint": true, "props": ["text": "Prefix "]],
                ["id": 22, "parent": 2, "paint": true, "props": ["text": "TOKEN"], "style": ["user_select": "all"]],
                ["id": 23, "parent": 2, "paint": true, "props": ["text": " Suffix"]],
            ]],
            text(3, "Second paragraph", handlers: ["selectionchange"]),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 180, w: 400), frame(2, 20, x: 20), frame(3, 70, x: 20),
        ])
        let first = try node(p, 2), second = try node(p, 3)
        var heard: [String] = []
        p.onSelectionChange = { id, text, start, end, _ in heard.append("\(id):\(text):\(start):\(end)") }
        let input = p.selectionHost.install(second)
        XCTAssertNotNil(input.document)
        XCTAssertTrue(input.selectAllDocument())
        XCTAssertEqual(input.selectedText, "TOKEN\n\nSecond paragraph", "nonselectable prefix and suffix stay out of Select All copy")
        XCTAssertEqual(heard, ["2:TOKEN:7:12", "3:Second paragraph:0:16"])
        let rects = input.selectionRects(for: try XCTUnwrap(input.selectedTextRange))
        XCTAssertEqual(rects.count, 2)
        // Selecting only inside the nonselectable prefix yields empty selection.
        let prefixRange = try XCTUnwrap(input.textRange(from: input.beginningOfDocument,
                                                        to: try XCTUnwrap(input.position(from: input.beginningOfDocument, offset: 4))))
        input.selectedTextRange = prefixRange
        XCTAssertEqual(input.selected.length, 0)
        XCTAssertTrue(first.isParagraph)
    }

    func testSharedSurfaceClipsRectsToNestedScrollAndSizesToScrollOwner() throws {
        // 1. Nested horizontal scroll container clips wide paragraph selection rects.
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "Above code"),
            box(3, style: ["overflow_x": "auto", "overflow_y": "hidden"]),
            text(4, "let veryLongIdentifier = 1234567890 + 9876543210 + 1111111111;", style: ["white_space": "pre"]),
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 200, w: 400), frame(2, 10, x: 20, w: 360),
            frame(3, 50, h: 40, x: 20, w: 120), frame(4, 0, h: 30, x: 0, w: 600),
            ["op": "content", "id": 3, "w": 600.0, "h": 40.0],
        ])
        let above = try node(p, 2), scroller = try node(p, 3), code = try node(p, 4)
        let input = p.selectionHost.install(above)
        XCTAssertTrue(input.selectAllDocument())
        let surface = try XCTUnwrap(input.document?.surface)
        let clipInSurface = scroller.convert(scroller.bounds, to: surface)
        let codeRects = input.selectionRects(for: try XCTUnwrap(input.selectedTextRange)).filter { $0.rect.minY >= clipInSurface.minY - 1 }
        XCTAssertFalse(codeRects.isEmpty)
        for rect in codeRects {
            XCTAssertLessThanOrEqual(rect.rect.maxX, clipInSurface.maxX + 0.5, "selection must not paint past nested clip")
        }
        // Moving the second paragraph notifies geometry change on settle.
        p.apply(wireBatch([frame(3, 90, h: 40, x: 20, w: 120)]))
        XCTAssertTrue(input.document?.geometryChanged() == false, "settle already recorded the moved geometry")
        XCTAssertTrue(code.isParagraph)
        session?.destroy(); session = nil; window?.isHidden = true; window = nil

        // 2. Scroll-container owner sizes SelectionSurface to its full contentSize, not only the viewport.
        let scrolled = mounted([
            box(1, style: ["user_select": "text", "overflow_y": "auto"]),
            text(2, "Top paragraph"),
            text(3, "Far below paragraph"),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 200, w: 400), frame(2, 10, x: 20), frame(3, 900, x: 20),
            ["op": "content", "id": 1, "w": 400.0, "h": 1200.0],
        ])
        let top = try node(scrolled, 2)
        let scrolledInput = scrolled.selectionHost.install(top)
        let scrolledSurface = try XCTUnwrap(scrolledInput.document?.surface)
        XCTAssertGreaterThanOrEqual(scrolledSurface.frame.height, 1200, "surface covers full scroll content")
    }

    func testNestedScrollLocalRuleAndControlsAndCancelledTouches() throws {
        ExactControls.install()
        let p = mounted([
            box(1),
            box(2, style: ["user_select": "text"]),
            text(3, "Outer paragraph"),
            box(4, style: ["overflow_x": "auto"]),
            text(5, "Inner code paragraph"),
            ["op": "create", "id": 6, "kind": "control", "props": ["type": "checkbox"], "style": [String: Any]()],
            box(7, handlers: ["press"]),
            ["op": "create", "id": 8, "kind": "view", "props": ["disabled": "true"], "style": [String: Any]()],
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "children", "id": 2, "ids": [3, 4]],
            ["op": "children", "id": 1, "ids": [2, 6, 7, 8]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 360, w: 400), frame(2, 0, h: 160, w: 400),
            frame(3, 10, x: 20), frame(4, 60, h: 40, x: 20, w: 200), frame(5, 0, h: 30, x: 0, w: 300),
            ["op": "content", "id": 4, "w": 300.0, "h": 40.0],
            frame(6, 180, h: 30, x: 20, w: 30), frame(7, 220, h: 40, x: 20, w: 100), frame(8, 270, h: 40, x: 20, w: 100),
        ])
        let outer = try node(p, 3), inner = try node(p, 5)
        let enabledBox = try node(p, 7), disabledBox = try node(p, 8)

        // Selecting outside a nested scroller, clearing, and then touching inside stays local to inner.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: outer, at: try point(in: outer, of: "Outer")))
        XCTAssertNotNil(p.selectionHost.text(for: outer)?.document)
        p.textSelection.clear()
        XCTAssertFalse(p.textSelection.routesToSurface(inner), "after clearing, inner paragraph does not reuse outer document")
        p.selectionHost.touched(inner)
        XCTAssertNil(p.selectionHost.text(for: inner)?.document, "starting inside nested scroller stays local")

        // Native checkbox control activation clears an active selection.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: outer, at: try point(in: outer, of: "Outer")))
        XCTAssertTrue(p.textSelection.hasActiveSelection)
        let checkbox = try XCTUnwrap(p.controls.controls[6] as? ExactCheckbox)
        XCTAssertEqual(checkbox.actions(forTarget: p.controls, forControlEvent: .valueChanged), ["changed:"])
        p.controls.changed(checkbox)
        XCTAssertFalse(p.textSelection.hasActiveSelection, "tapping a sibling checkbox clears selection")

        // Logically cancelled press contact on an enabled node does NOT clear selection.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: outer, at: try point(in: outer, of: "Outer")))
        let cancelled = Touch(enabledBox)
        enabledBox.touchesBegan([cancelled], with: TextTouchEvent())
        enabledBox.cancelPressContact()
        enabledBox.touchesEnded([cancelled], with: TextTouchEvent())
        XCTAssertTrue(p.textSelection.hasActiveSelection, "cancelled press contact must not clear selection")

        // Tapping a disabled node DOES clear selection.
        let disabledTap = Touch(disabledBox)
        disabledBox.touchesBegan([disabledTap], with: TextTouchEvent())
        disabledBox.touchesEnded([disabledTap], with: TextTouchEvent())
        XCTAssertFalse(p.textSelection.hasActiveSelection, "tapping a disabled node clears selection")

        // Passive SelectionDismissObserver on documentHost clears on outside tap and ignores drags.
        _ = try XCTUnwrap(p.selectionHost.selectWord(in: outer, at: try point(in: outer, of: "Outer")))
        let observer = try XCTUnwrap(p.documentHost.gestureRecognizers?.compactMap { $0 as? SelectionDismissObserver }.first)
        let checkboxTap = Touch(checkbox)
        observer.touchesBegan([checkboxTap], with: TextTouchEvent())
        observer.touchesEnded([checkboxTap], with: TextTouchEvent())
        XCTAssertFalse(p.textSelection.hasActiveSelection, "root dismiss observer clears on native control tap")
    }

    func testCopiedTextStripsSyntheticSoftHyphensButKeepsAuthoredOnes() throws {
        let p = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "absichtlich", style: ["overflow_wrap": "break-word"]),
            text(3, "ab\u{00AD}sichtlich", style: ["overflow_wrap": "break-word"]),
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 160, w: 400), frame(2, 20, x: 20), frame(3, 70, x: 20),
        ])
        p.documentLanguage = "de"
        let autoNode = try node(p, 2), manualNode = try node(p, 3)
        autoNode.invalidateText(); manualNode.invalidateText()
        XCTAssertFalse(autoNode.paragraphSpec().source.inserted.isEmpty, "auto emergency hyphenation inserts synthetic soft hyphens")
        XCTAssertTrue(manualNode.paragraphSpec().source.inserted.isEmpty, "authored soft hyphen keeps its own break points")

        let input = p.selectionHost.install(autoNode)
        input.selectAllDocument()
        XCTAssertEqual(input.selectedText, "absichtlich\n\nab\u{00AD}sichtlich")
        XCTAssertFalse(autoNode.copiedText(SelectableText.shaped(autoNode) as String, from: 0).contains("\u{00AD}"))
        XCTAssertEqual(manualNode.copiedText(SelectableText.shaped(manualNode) as String, from: 0), "ab\u{00AD}sichtlich")
    }

    func testDisjointAndNestedAllRunsAndClipUpdatesAndControlToggleValue() throws {
        // 1. Nested user-select="all" runs copy once without duplicating inner text,
        // and standalone Select All over two disjoint all runs highlights only the two runs.
        let p = mounted([
            box(1),
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "pre [A B C] mid [D]"],
             "style": ["font_size": 17.0, "line_height": "22px", "user_select": "none"]],
            ["op": "paragraph", "id": 2, "runs": [
                ["id": 20, "parent": 2, "paint": true, "props": ["text": "pre "]],
                ["id": 21, "parent": 2, "paint": true, "props": ["text": "[A "], "style": ["user_select": "all"]],
                ["id": 22, "parent": 21, "paint": true, "props": ["text": "B"], "style": ["user_select": "all"]],
                ["id": 23, "parent": 21, "paint": true, "props": ["text": " C]"], "style": ["user_select": "all"]],
                ["id": 24, "parent": 2, "paint": true, "props": ["text": " mid "]],
                ["id": 25, "parent": 2, "paint": true, "props": ["text": "[D]"], "style": ["user_select": "all"]],
            ]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 120, w: 400), frame(2, 10, x: 10, w: 360),
        ])
        let para = try node(p, 2)
        let standalone = p.selectionHost.install(para)
        XCTAssertTrue(standalone.selectAllDocument())
        XCTAssertEqual(standalone.selectedText, "[A B C][D]", "nested all run does not duplicate 'B' and gap is excluded")
        let rects = standalone.selectionRects(for: try XCTUnwrap(standalone.selectedTextRange))
        XCTAssertEqual(rects.count, 2, "standalone Select All highlights only the two disjoint all runs, not the gap")

        // 2. Selecting inside a nested scroller then selecting an outer spanning document
        // does not leave an idle standalone input shadowing the active spanning document,
        // and SelectionDismissObserver ends every touch sequence in .failed.
        let p2 = mounted([
            box(1),
            box(2, style: ["user_select": "text"]),
            text(3, "Outer prose"),
            ["op": "create", "id": 4, "kind": "scroll", "props": ["scrollsX": "true"], "style": ["overflow_x": "auto"]],
            text(5, "Inner code"),
            ["op": "create", "id": 6, "kind": "control", "props": ["type": "checkbox"], "style": [String: Any]()],
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "children", "id": 2, "ids": [3, 4]],
            ["op": "children", "id": 1, "ids": [2, 6]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 240, w: 400), frame(2, 0, h: 160, w: 400),
            frame(3, 10, x: 20), frame(4, 60, h: 40, x: 20, w: 200), frame(5, 0, h: 30, x: 0, w: 300),
            ["op": "content", "id": 4, "w": 300.0, "h": 40.0],
            frame(6, 180, h: 30, x: 20, w: 30),
        ])
        let outer = try node(p2, 3), inner = try node(p2, 5)
        _ = try XCTUnwrap(p2.selectionHost.selectWord(in: inner, at: try point(in: inner, of: "Inner")))
        XCTAssertNil(p2.selectionHost.text(for: inner)?.document, "started inside nested scroller")
        _ = try XCTUnwrap(p2.selectionHost.selectWord(in: outer, at: try point(in: outer, of: "Outer")))
        XCTAssertNotNil(p2.selectionHost.text(for: inner)?.document, "active spanning document is not shadowed by idle standalone input")

        let observer = try XCTUnwrap(p2.documentHost.gestureRecognizers?.compactMap { $0 as? SelectionDismissObserver }.first)
        let ownedTouch = Touch(outer)
        observer.touchesBegan([ownedTouch], with: TextTouchEvent())
        observer.touchesEnded([ownedTouch], with: TextTouchEvent())
        XCTAssertTrue(p2.textSelection.hasActiveSelection, "touch owned by selection is ignored and does not dismiss")
        let checkbox = try XCTUnwrap(p2.controls.controls[6] as? ExactCheckbox)
        let cancelTouch = Touch(checkbox)
        observer.touchesBegan([cancelTouch], with: TextTouchEvent())
        observer.touchesCancelled([cancelTouch], with: TextTouchEvent())
        observer.touchesEnded([cancelTouch], with: TextTouchEvent())
        XCTAssertTrue(p2.textSelection.hasActiveSelection, "cancelled touch clears tracking and does not dismiss")

        // 3. ControlHost.changed captures the toggled value before clear() runs selectionchange.
        var observedChecked: Bool?
        p2.onChecked = { _, on in observedChecked = on }
        p2.onSelectionChange = { _, _, _, _, _ in
            // Simulate a batch reverting the checkbox during selectionchange.
            checkbox.isOn = false
        }
        outer.handlers.insert("selectionchange")
        checkbox.isOn = true
        p2.controls.changed(checkbox)
        XCTAssertEqual(observedChecked, true, "toggled value is captured before selectionchange runs")
        XCTAssertFalse(p2.textSelection.hasActiveSelection)

        // 4. Nonrectangular clip-path masks the shared surface, clips rects to the shape,
        // and changing the clip path with fixed paragraph frame invalidates geometryChanged().
        let p3 = mounted([
            box(1, style: ["user_select": "text"]),
            text(2, "First line"),
            box(3, style: ["overflow_x": "hidden", "overflow_y": "hidden"]),
            text(4, "Top"),
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 220, w: 300), frame(2, 10, x: 10, w: 280),
            frame(3, 60, h: 100, x: 10, w: 100), frame(4, 0, h: 24, x: 0, w: 100),
        ])
        let clipBox = try node(p3, 3), second = try node(p3, 4)
        let maskLayer = CAShapeLayer()
        maskLayer.frame = clipBox.bounds
        maskLayer.path = CGPath(ellipseIn: CGRect(x: 30, y: 30, width: 40, height: 40), transform: nil)
        clipBox.layer.mask = maskLayer
        let docInput = p3.selectionHost.install(try node(p3, 2))
        let doc = try XCTUnwrap(docInput.document)
        XCTAssertTrue(docInput.selectAllDocument())
        XCTAssertNotNil(doc.surface.layer.mask, "nonrectangular clip masks the shared selection surface")
        let topRects = doc.rects(doc.parts[1].range)
        XCTAssertTrue(topRects.allSatisfy { $0.rect.width == 0 }, "text outside the circle at y=0..25 gets no highlight rects")
        XCTAssertFalse(doc.geometryChanged(), "baseline fingerprint matches")
        maskLayer.path = CGPath(ellipseIn: CGRect(x: 0, y: 0, width: 100, height: 100), transform: nil)
        XCTAssertTrue(doc.geometryChanged(), "changing clip path with fixed paragraph frame invalidates geometry")
        let fullRects = doc.rects(doc.parts[1].range)
        XCTAssertTrue(fullRects.contains { $0.rect.width > 0 }, "expanding circle admits highlight rects")
        _ = second
    }

    /// expo/exact#2001: a paragraph inside a nested horizontal scroll stays on its
    /// own view for hit testing even while an enclosing shared document has an
    /// active selection, so swiping the code block reaches the nested UIScrollView
    /// instead of routing out to the enclosing SelectionSurface (both with and
    /// without a prior touch inside the code block), drags move contentOffset.x
    /// without popping a pushed route, highlighted code consumes taps without
    /// firing press, `user-select="all"` keeps nested scroller touches local,
    /// removing a scroll boundary reconciles idle standalone inputs, and clipped
    /// overflow outside the scrollport does not steal handle targeting.
    func testActiveSelectionDoesNotStealTouchesFromNestedHorizontalScroll() throws {
        let ops: [[String: Any]] = [
            box(1, style: ["user_select": "text"]),
            text(2, "First paragraph of a comment."),
            box(3, handlers: ["press"]),
            box(4, style: ["overflow_x": "auto", "overflow_y": "hidden"]),
            text(5, "let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];", style: ["white_space": "pre", "user_select": "text"], handlers: ["press"]),
            text(6, "Second paragraph."),
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 6]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 300, w: 360),
            frame(2, 10, h: 24, x: 16, w: 328),
            frame(3, 44, h: 44, x: 16, w: 328),
            frame(4, 96, h: 40, x: 16, w: 200),
            frame(5, 0, h: 30, x: 0, w: 600),
            frame(6, 150, h: 24, x: 16, w: 328),
            ["op": "content", "id": 4, "w": 600.0, "h": 40.0],
        ]

        // Case A: pushed route with no prior touch inside the code block.
        let p = mounted(ops)
        let rootVC = UIViewController()
        let detailVC = UIViewController()
        let nav = UINavigationController(rootViewController: rootVC)
        nav.pushViewController(detailVC, animated: false)
        window?.rootViewController = nav
        detailVC.view.addSubview(p.viewport)
        p.viewport.frame = detailVC.view.bounds
        p.viewport.layoutIfNeeded()

        let owner = try node(p, 1), p1 = try node(p, 2), scroller = try node(p, 4), code = try node(p, 5), p2 = try node(p, 6)
        let scrollView = try XCTUnwrap(scroller.scrollView)
        let event = TextTouchEvent()

        _ = try XCTUnwrap(p.selectionHost.selectWord(in: p1, at: try point(in: p1, of: "paragraph")))
        let surface = try XCTUnwrap(p.selectionHost.surface(for: p1))
        XCTAssertTrue(p1.hitTest(CGPoint(x: 20, y: 10), with: event) === surface)
        XCTAssertTrue(p2.hitTest(CGPoint(x: 20, y: 10), with: event) === surface)
        let codeHitWithoutPrior = try XCTUnwrap(owner.hitTest(CGPoint(x: 40, y: 110), with: event))
        XCTAssertTrue(codeHitWithoutPrior === code, "code inside nested scroll must not route hits out to enclosing SelectionSurface")
        XCTAssertTrue(codeHitWithoutPrior.isDescendant(of: scrollView), "nested UIScrollView must remain an ancestor of the hit view")
        XCTAssertNil(p.selectionHost.surface(for: code))
        XCTAssertFalse(p.textSelection.routesToSurface(code))
        XCTAssertFalse(p.textSelection.owns(code))
        XCTAssertFalse(p.textSelection.hasSelection(in: code), "unhighlighted code reports no selection")

        // Simulate horizontal drag left then right on the nested scroller while p1 is selected:
        // contentOffset moves both directions, selection remains intact, and the pushed route stays mounted.
        scroller.scrollViewWillBeginDragging(scrollView)
        scrollView.contentOffset = CGPoint(x: 120, y: 0)
        scroller.scrollViewDidScroll(scrollView)
        XCTAssertEqual(scrollView.contentOffset.x, 120, accuracy: 0.5)
        XCTAssertTrue(p.textSelection.hasActiveSelection, "horizontal scroll inside code keeps p1 selection active")
        XCTAssertEqual(nav.viewControllers.count, 2, "swiping inside code must not pop the pushed route")

        // Handle targeting at nonzero scroll offset must ignore clipped-out code past the scrollport edge.
        let doc = try XCTUnwrap(p.selectionHost.text(for: p1)?.document)
        let outsideScrollport = CGPoint(x: 280, y: 110) // inside code's unclipped width (600) but right of scroller (x=16..216)
        XCTAssertNil(doc.paragraph(at: outsideScrollport), "clipped-out portion of scrolled code must not claim points past scrollport")
        let draggedIndex = doc.index(at: outsideScrollport)
        XCTAssertNotEqual(doc.part(at: draggedIndex)?.node, code, "clipped-out code must not steal handle targeting from visible paragraphs")

        scroller.scrollViewWillBeginDragging(scrollView)
        scrollView.contentOffset = CGPoint(x: 0, y: 0)
        scroller.scrollViewDidScroll(scrollView)
        XCTAssertEqual(scrollView.contentOffset.x, 0, accuracy: 0.5)
        XCTAssertEqual(nav.viewControllers.count, 2, "swiping right back to offset 0 keeps the pushed route")

        // Tapping unhighlighted code clears p1's selection AND delivers code's press handler.
        var pressedIds: [UInt32] = []
        p.onPress = { id in pressedIds.append(id) }
        let unhighlightedTap = EndingTouch(code)
        code.touchesBegan([unhighlightedTap], with: event)
        unhighlightedTap.lifted = true
        code.touchesEnded([unhighlightedTap], with: event)
        XCTAssertFalse(p.textSelection.hasActiveSelection)
        XCTAssertEqual(pressedIds, [5], "tapping unhighlighted code clears selection and delivers press")
        XCTAssertFalse(code.pressed)

        // Select All spans p1, code, and p2; tapping highlighted code clears selection WITHOUT firing press or leaking pressed state,
        // even when SelectionDismissObserver delivers touchesEnded before NodeView.touchesEnded (UIKit's real delivery order).
        pressedIds.removeAll()
        XCTAssertTrue(p.textSelection.selectAll(p1))
        XCTAssertTrue(p.textSelection.hasSelection(in: code))
        XCTAssertTrue(p.selectionHost.text(for: p1)?.selectedText.contains("let numbers") == true)
        let observer = try XCTUnwrap(p.documentHost.gestureRecognizers?.first { $0 is SelectionDismissObserver })
        let highlightedTap = EndingTouch(code)
        observer.touchesBegan([highlightedTap], with: event)
        code.touchesBegan([highlightedTap], with: event)
        highlightedTap.lifted = true
        observer.touchesEnded([highlightedTap], with: event)
        XCTAssertFalse(p.textSelection.hasActiveSelection, "dismiss observer clears selection on lift")
        code.touchesEnded([highlightedTap], with: event)
        XCTAssertTrue(pressedIds.isEmpty, "tapping highlighted code must clear selection without firing press")
        XCTAssertFalse(code.pressed, "consuming tap on highlighted code must reset pressed state")

        // Case B: fresh fixture — prior touch on code installs standalone input, then select p1, then touch code.
        let pPrior = mounted(ops)
        let ownerB = try node(pPrior, 1), p1B = try node(pPrior, 2), scrollerB = try node(pPrior, 4), codeB = try node(pPrior, 5)
        let scrollViewB = try XCTUnwrap(scrollerB.scrollView)
        _ = ownerB.hitTest(CGPoint(x: 40, y: 110), with: event)
        let priorInput = try XCTUnwrap(pPrior.selectionHost.text(for: codeB), "prior touch must install local input on code")
        XCTAssertNil(priorInput.document, "prior touch input on nested scroller must be standalone")
        _ = try XCTUnwrap(pPrior.selectionHost.selectWord(in: p1B, at: try point(in: p1B, of: "paragraph")))
        let codeHitWithPrior = try XCTUnwrap(ownerB.hitTest(CGPoint(x: 40, y: 110), with: event))
        XCTAssertTrue(codeHitWithPrior === codeB, "active document selection must not steal touches after prior scroll")
        XCTAssertTrue(codeHitWithPrior.isDescendant(of: scrollViewB))

        // Removing the scroll boundary reconciles the idle standalone input back to the shared document.
        pPrior.apply(wireBatch([
            ["op": "style", "id": 4, "style": ["overflow_x": "visible", "overflow_y": "visible"]],
        ]))
        XCTAssertNil(scrollerB.scrollView, "removing overflow removes UIScrollView")
        XCTAssertTrue(pPrior.textSelection.routesToSurface(codeB), "after removing scroll boundary, code routes to shared surface")
        XCTAssertTrue(pPrior.textSelection.owns(codeB), "after removing scroll boundary, active document owns code")
        let surfaceB = try XCTUnwrap(pPrior.selectionHost.surface(for: p1B))
        XCTAssertTrue(ownerB.hitTest(CGPoint(x: 40, y: 110), with: event) === surfaceB)

        // Adding a scroll boundary over the initiating paragraph stops `owns` from claiming it.
        pPrior.apply(wireBatch([
            ["op": "style", "id": 1, "style": ["user_select": "text"]],
            ["op": "style", "id": 4, "style": ["overflow_x": "auto", "overflow_y": "hidden"]],
        ]))
        XCTAssertFalse(pPrior.textSelection.owns(codeB), "re-added scroll boundary drops ownership")

        // Case C: enclosing `user-select="all"` preserves atomic range across nested scroller in BOTH directions while keeping scroller hit-testing local.
        let pAll = mounted([
            box(1, style: ["user_select": "all"]),
            text(2, "Intro sentence."),
            box(4, style: ["overflow_x": "auto", "overflow_y": "hidden"]),
            text(5, "const payload = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];", style: ["white_space": "pre"]),
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "children", "id": 1, "ids": [2, 4]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 200, w: 360),
            frame(2, 10, h: 24, x: 16, w: 328),
            frame(4, 50, h: 40, x: 16, w: 200),
            frame(5, 0, h: 30, x: 0, w: 500),
            ["op": "content", "id": 4, "w": 500.0, "h": 40.0],
        ])
        let ownerAll = try node(pAll, 1), p1All = try node(pAll, 2), scrollerAll = try node(pAll, 4), codeAll = try node(pAll, 5)
        let scrollViewAll = try XCTUnwrap(scrollerAll.scrollView)
        _ = try XCTUnwrap(pAll.selectionHost.selectWord(in: p1All, at: try point(in: p1All, of: "Intro")))
        XCTAssertTrue(pAll.selectionHost.text(for: p1All)?.selectedText.contains("const payload") == true, "user-select=all still selects atomically across nested scroller")
        XCTAssertTrue(pAll.textSelection.hasSelection(in: codeAll))
        let hitAll = try XCTUnwrap(ownerAll.hitTest(CGPoint(x: 40, y: 65), with: event))
        XCTAssertTrue(hitAll === codeAll, "user-select=all must not route nested scroller touches out to outer surface")
        XCTAssertTrue(hitAll.isDescendant(of: scrollViewAll))
        XCTAssertFalse(pAll.textSelection.routesToSurface(codeAll))
        XCTAssertFalse(pAll.textSelection.owns(codeAll))
        // Reverse direction: starting selection inside the nested scroller still promotes to the full user-select="all" element.
        pAll.textSelection.clear()
        _ = try XCTUnwrap(pAll.selectionHost.selectWord(in: codeAll, at: try point(in: codeAll, of: "payload")))
        XCTAssertTrue(pAll.selectionHost.text(for: codeAll)?.selectedText.contains("Intro sentence.") == true, "selecting inside nested scroller under user-select=all selects enclosing element")
        XCTAssertTrue(pAll.textSelection.hasSelection(in: p1All))
        XCTAssertTrue(pAll.textSelection.hasSelection(in: codeAll))
        XCTAssertTrue(ownerAll.hitTest(CGPoint(x: 40, y: 65), with: event) === codeAll, "nested scroller stays local after reverse-direction user-select=all")
    }

}
/// A touch on `hit` whose phase a test ends: UIKit's end of it never reached the observers.
private final class EndingTouch: UITouch {
    let hit: UIView
    var point: CGPoint?
    var lifted = false
    init(_ hit: UIView) { self.hit = hit; super.init() }
    override var view: UIView? { hit }
    override var window: UIWindow? { hit.window }
    override var phase: UITouch.Phase { lifted ? .ended : .began }
    override func location(in view: UIView?) -> CGPoint { hit.convert(point ?? CGPoint(x: hit.bounds.midX, y: hit.bounds.midY), to: view) }
}
#endif
