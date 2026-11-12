// @ref LLP 1116.007 D4: `input type="number"` is a field with a UIStepper at its
// trailing edge, stepping the field's text by HTML's rules and saying so as
// `input` then `change`; `appearance="textfield"`, any other `type` and a
// field too narrow for both leave the bare field.
#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class NumberStepperIOSTests: XCTestCase {
    /// A number input in a session, 300×44 as a bordered field's box, its
    /// text starting at `rect`'s edges as the kernel publishes it.
    private func fixture(_ props: [String: String] = [:], width: CGFloat = 300, rect: [Double]? = nil, handlers: Set<String> = ["input", "change"]) -> (node: NodeView, heard: () -> [String], keep: ExactSession) {
        let session = ExactApp.shared.makeSession(label: "number-stepper")
        let p = session.presenter
        let node = NodeView(id: 1, kind: "input", presenter: p)
        node.bounds = CGRect(x: 0, y: 0, width: width, height: 44)
        node.handlers = handlers
        p.views[node.id] = node
        var heard: [String] = []
        p.onInput = { _, value in heard.append("input \(value)") }
        p.onChange = { _, value in heard.append("change \(value)") }
        node.applyProps(set: ["type": "number", "min": "1", "max": "99", "step": "1", "value": "3", "aria-label": "Quantity", "accessibilityLabel": "Quantity", "testId": "qty"].merging(props) { _, new in new }, clear: [])
        node.applyStyle(["font_size": .number(17)])
        node.applyFieldContent(["rect": rect ?? [6.0, 5.0, Double(width) - 12, 34.0]])
        return (node, { heard }, session)
    }

    func testNumberInsertionFiltersLettersAndKeepsFractionAndExponentCharacters() throws {
        let (node, _, session) = fixture(["value": ""])
        defer { session.destroy() }
        let field = try XCTUnwrap(node.field)
        XCTAssertFalse(node.textField(field, shouldChangeCharactersIn: NSRange(location: 0, length: 0), replacementString: "a"))
        XCTAssertFalse(node.textField(field, shouldChangeCharactersIn: NSRange(location: 0, length: 0), replacementString: "4a4"))
        XCTAssertEqual(field.text, "44")
        XCTAssertTrue(node.textField(field, shouldChangeCharactersIn: NSRange(location: 0, length: 2), replacementString: "-1.5e+2"))
        XCTAssertEqual(field.keyboardType, .numberPad, "a paste is filtered as typing is; the pad is the step's")
    }

    func testNumericSignsFollowChromesEditorIncludingIncompleteValues() throws {
        let (node, _, session) = fixture(["value": ""])
        defer { session.destroy() }
        let field = try XCTUnwrap(node.field)
        for (typed, expected) in [("12-", "12-"), ("--1", "--1"), ("1e2-", "1e2"), ("1e2+", "1e2"), ("1e-+2", "1e-2"), ("-1.5e+2", "-1.5e+2")] {
            field.text = ""
            for char in typed {
                let text = field.text ?? ""
                if node.textField(field, shouldChangeCharactersIn: NSRange(location: text.utf16.count, length: 0), replacementString: String(char)) {
                    field.text = text + String(char)
                }
            }
            XCTAssertEqual(field.text, expected, "typing \(typed)")
            let range = NSRange(location: 0, length: (field.text ?? "").utf16.count)
            if node.textField(field, shouldChangeCharactersIn: range, replacementString: typed) { field.text = typed }
            XCTAssertEqual(field.text, expected, "inserting \(typed)")
        }
        XCTAssertEqual(field.keyboardType, .numberPad)
    }

    /// The plus or the minus: the stepper's value moves a step, as UIKit's
    /// does when a finger lifts on either half, and its `valueChanged` action
    /// runs (a test bundle has no application to send it, so it is called, as
    /// `FieldCompositionIOSTests` calls a field's `fieldChanged`).
    private func press(_ node: NodeView, up: Bool) {
        guard let stepper = node.stepper else { return XCTFail("no stepper") }
        stepper.value += up ? stepper.stepValue : -stepper.stepValue
        node.stepperChanged(stepper)
    }

    func testANumberInputHasAStepperAtItsTrailingEdgeAndTheTextGivesWayToIt() throws {
        let (node, _, session) = fixture()
        defer { session.destroy() }
        let stepper = try XCTUnwrap(node.stepper)
        XCTAssertTrue(stepper.superview === node)
        let field = try XCTUnwrap(node.field)
        XCTAssertEqual(stepper.minimumValue, 1)
        XCTAssertEqual(stepper.maximumValue, 99)
        XCTAssertEqual(stepper.stepValue, 1)
        XCTAssertEqual(stepper.value, 3, "it shows what the field holds")
        let size = stepper.intrinsicContentSize
        XCTAssertEqual(stepper.frame.size, size)
        XCTAssertEqual(stepper.frame.maxX, 294, "its trailing edge is where the text ends")
        XCTAssertEqual(stepper.frame.midY, node.bounds.midY, accuracy: 0.5, "centered in the input's box")
        let text = field.textRect(forBounds: field.bounds)
        XCTAssertEqual(text.minX, 6)
        XCTAssertEqual(text.maxX, stepper.frame.minX - NodeView.stepperGap, accuracy: 0.001, "the text ends a gap short of the stepper")
        XCTAssertEqual(field.editingRect(forBounds: field.bounds), text)
        XCTAssertEqual(field.placeholderRect(forBounds: field.bounds), text)
        XCTAssertEqual(field.keyboardType, .numberPad, "step 1 from min 1: whole numbers, the number pad (LLP 1116.019 D5)")
        XCTAssertEqual(field.text, "3")
        XCTAssertEqual(stepper.accessibilityLabel, "Quantity", "the stepper is named as the field is")
        XCTAssertEqual(field.accessibilityLabel, "Quantity")
        XCTAssertEqual(stepper.accessibilityIdentifier, "qty-stepper")
        XCTAssertFalse(stepper.isHidden)
    }

    func testAStepperClipsInsideAResizingBorderAndRemainsTappable() throws {
        let (node, heard, session) = fixture()
        defer { session.destroy() }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the agent sends the stepper's action through UIApplication")
        }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.rootViewController?.view.addSubview(node)
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        node.applyStyle(["border_width": 6, "border_radius": 20, "overflow_x": "hidden", "overflow_y": "hidden"])
        let field = try XCTUnwrap(node.field), original = try XCTUnwrap(node.stepper)
        let frame = original.frame
        XCTAssertTrue(field.becomeFirstResponder())
        for width in [200.0, 250.0, 350.0] {
            session.presenter.apply(wireBatch([["op": "present", "id": 1, "property": "layout", "w": width - 300]]))
            let box = try XCTUnwrap(node.clipBox), surface = try XCTUnwrap(node.surface)
            XCTAssertTrue(original.superview === box)
            XCTAssertTrue(field.superview === box)
            XCTAssertTrue(box.layer.mask === surface.clip)
            XCTAssertEqual(surface.clip.path?.boundingBoxOfPath.width ?? 0, width - 12, accuracy: 0.001)
            XCTAssertFalse(try XCTUnwrap(surface.clip.path).contains(CGPoint(x: width - 3, y: 22)))
            XCTAssertEqual(original.frame, frame)
            XCTAssertTrue(field.isFirstResponder)
        }
        func tap(_ stepper: FieldStepper, up: Bool) throws {
            let point = stepper.convert(CGPoint(x: stepper.bounds.width * (up ? 0.75 : 0.25), y: stepper.bounds.midY), to: nil)
            let hit = try XCTUnwrap(window.hitTest(window.convert(point, from: nil), with: nil))
            let result = try XCTUnwrap(session.agentInstance.steppedTap(node, hit: hit, at: point, tapped: [Double(point.x), Double(point.y)]))
            XCTAssertEqual(result["stepped"] as? Bool, true)
        }
        field.selectAll(nil)
        field.insertText("5")
        XCTAssertEqual(field.text, "5")
        node.fieldChanged()
        let edits = heard().count
        try tap(original, up: true)
        XCTAssertEqual(field.text, "6")
        XCTAssertEqual(heard().dropFirst(edits), ["input 6", "change 6"])
        let box = try XCTUnwrap(node.clipBox)
        box.bringSubviewToFront(field)
        node.layoutStepper()
        XCTAssertTrue(box.subviews.last === original, "the stepper stays above its field in the clip box")
        node.applyProps(set: ["type": "text", "value": "6"], clear: [])
        node.applyProps(set: ["type": "number"], clear: [])
        let replacement = try XCTUnwrap(node.stepper)
        XCTAssertTrue(replacement.superview === box)
        XCTAssertTrue(box.subviews.last === replacement)
        session.presenter.apply(wireBatch([["op": "present", "id": 1, "property": "layout", "w": 0.0]]))
        XCTAssertNil(node.clipBox)
        XCTAssertTrue(field.superview === node)
        XCTAssertTrue(replacement.superview === node)
        XCTAssertTrue(field.isFirstResponder)
        try tap(replacement, up: false)
        XCTAssertEqual(field.text, "5")
        XCTAssertEqual(heard().suffix(2), ["input 5", "change 5"])
    }

    func testPointerEventsNoneSkipsFieldsAndSteppersDuringAResizeButKeepsAutoDescendants() throws {
        for type in ["number", "text"] {
            let (node, _, session) = fixture(["type": type])
            defer { session.destroy() }
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 100))
            let behind = UIView(frame: window.bounds)
            node.frame.origin = .zero
            window.addSubview(behind)
            window.addSubview(node)
            window.makeKeyAndVisible()
            defer { window.isHidden = true }
            var style: NodeStyle = ["border_width": 6, "border_radius": 20, "overflow_x": "hidden", "overflow_y": "hidden"]
            node.applyStyle(style)
            let field = try XCTUnwrap(node.field)
            let descendant = NodeView(id: 2, kind: "view", presenter: session.presenter)
            descendant.frame = CGRect(x: 20, y: 8, width: 40, height: 28)
            descendant.applyStyle(["pointer_events": "auto"])
            node.container.addSubview(descendant)
            let controls: [UIView] = [field] + (node.stepper.map { [$0] } ?? [])
            func hit(_ view: UIView) -> UIView? {
                let point = CGPoint(x: view is FieldStepper ? view.bounds.width * 0.75 : 100, y: view.bounds.midY)
                return window.hitTest(view.convert(point, to: window), with: nil)
            }
            for offset in [0.0, 50.0, 100.0, 0.0] {
                session.presenter.apply(wireBatch([["op": "present", "id": 1, "property": "layout", "w": offset]]))
                XCTAssertEqual(node.clipBox != nil, offset != 0)
                for events in ["none", "auto"] {
                    style["pointer_events"] = .string(events)
                    node.applyStyle(style)
                    for control in controls {
                        let target = try XCTUnwrap(hit(control), "\(type), resize \(offset), pointer-events \(events)")
                        if events == "none" {
                            XCTAssertTrue(target === behind, "\(type), resize \(offset): \(control) must let the touch through")
                        } else {
                            XCTAssertTrue(target === control || target.isDescendant(of: control))
                        }
                    }
                    let point = descendant.convert(CGPoint(x: 20, y: 14), to: window)
                    XCTAssertTrue(window.hitTest(point, with: nil) === descendant, "an auto descendant stays eligible")
                }
            }
        }
    }

    func testNoStepperWhereTheAuthorOrTheTypeSaysNone() throws {
        let (node, _, session) = fixture()
        defer { session.destroy() }
        let field = try XCTUnwrap(node.field)
        let full = field.textRect(forBounds: field.bounds)
        XCTAssertNotNil(node.stepper)
        // `appearance="textfield"`: the bare field, text to the trailing edge.
        node.applyStyle(["appearance": .string("textfield")])
        XCTAssertNil(node.stepper)
        XCTAssertFalse(node.subviews.contains { $0 is UIStepper })
        XCTAssertEqual(field.textRect(forBounds: field.bounds).maxX, full.maxX + node.stepperRoomForTests, accuracy: 0.001)
        XCTAssertEqual(field.borderStyle, .roundedRect, "still the platform's field")
        node.applyStyle(["font_size": .number(17)])
        XCTAssertNotNil(node.stepper, "back to the default, it has one again")
        // `appearance="none"`: the author's own box.
        node.applyStyle(["appearance": .string("none")])
        XCTAssertNil(node.stepper)
        node.applyStyle(["font_size": .number(17)])
        XCTAssertNotNil(node.stepper)
        // Nothing is inferred from the bounds or the keyboard: another type has none.
        for props in [["type": "text"], ["type": "text", "inputMode": "numeric"], ["type": "text", "inputMode": "decimal"], ["type": "tel"], ["type": "email"], ["type": "password"], ["type": "search"]] {
            node.applyProps(set: props, clear: [])
            XCTAssertNil(node.stepper, "\(props)")
            XCTAssertEqual(field.textRect(forBounds: field.bounds).maxX, full.maxX + node.stepperRoomForTests, accuracy: 0.001, "\(props)")
        }
        node.applyProps(set: ["type": "number"], clear: ["inputMode"])
        XCTAssertNotNil(node.stepper)
        // A plain input has none, whatever its bounds.
        let plain = NodeView(id: 2, kind: "input", presenter: node.presenter!)
        plain.bounds = node.bounds
        plain.applyProps(set: ["min": "1", "max": "99", "step": "1", "value": "3"], clear: [])
        plain.applyFieldContent(["rect": [6.0, 5.0, 288.0, 34.0]])
        XCTAssertNil(plain.stepper)
    }

    func testSteppingSetsTheTextWithinTheBoundsAndSaysInputThenChange() throws {
        let (node, heard, session) = fixture()
        defer { session.destroy() }
        let stepper = try XCTUnwrap(node.stepper)
        let field = try XCTUnwrap(node.field)
        XCTAssertEqual(stepper.actions(forTarget: node, forControlEvent: .valueChanged), ["stepperChanged:"], "UIKit's own action on the stepper")
        press(node, up: true)
        XCTAssertEqual(field.text, "4")
        XCTAssertEqual(heard(), ["input 4", "change 4"])
        XCTAssertEqual(stepper.value, 4)
        press(node, up: false)
        press(node, up: false)
        XCTAssertEqual(field.text, "2")
        XCTAssertEqual(heard().suffix(4), ["input 3", "change 3", "input 2", "change 2"])
        press(node, up: false)
        XCTAssertEqual(field.text, "1")
        XCTAssertEqual(stepper.value, stepper.minimumValue)
        // At `min`, nothing steps below it and nothing is said.
        let count = heard().count
        press(node, up: false)
        XCTAssertEqual(field.text, "1")
        XCTAssertEqual(heard().count, count)
        // Typing carries the stepper to the typed number; the top bounds it.
        field.text = "98"
        node.fieldChanged()
        XCTAssertEqual(stepper.value, 98)
        press(node, up: true)
        XCTAssertEqual(field.text, "99")
        XCTAssertEqual(stepper.value, stepper.maximumValue)
        let top = heard().count
        press(node, up: true)
        XCTAssertEqual(field.text, "99")
        XCTAssertEqual(heard().count, top)
        // A value above `max` steps down to it, and up does nothing.
        field.text = "150"
        node.fieldChanged()
        press(node, up: true)
        XCTAssertEqual(field.text, "150")
        press(node, up: false)
        XCTAssertEqual(field.text, "99")
    }

    func testAnEmptyOrNonNumberFieldStepsFromZeroAndAWholeStepGivesAWholeNumber() throws {
        let (node, heard, session) = fixture(["value": ""])
        defer { session.destroy() }
        let stepper = try XCTUnwrap(node.stepper)
        let field = try XCTUnwrap(node.field)
        press(node, up: false)
        XCTAssertEqual(field.text, "", "from 0, stepping down is below min 1: nothing happens, as the web's")
        XCTAssertEqual(heard(), [])
        press(node, up: true)
        XCTAssertEqual(field.text, "1")
        node.applyProps(set: ["min": "0", "max": "10", "step": "0.5", "value": "1.5"], clear: [])
        XCTAssertEqual(field.text, "1.5")
        XCTAssertEqual(stepper.stepValue, 0.5)
        press(node, up: true)
        XCTAssertEqual(field.text, "2", "a whole step gives a whole number, never 2.0")
        press(node, up: true)
        XCTAssertEqual(field.text, "2.5")
        XCTAssertEqual(heard().suffix(2), ["input 2.5", "change 2.5"])
        // No bounds written: none, as the web's.
        node.applyProps(set: ["value": "-3"], clear: ["min", "max", "step"])
        press(node, up: false)
        XCTAssertEqual(field.text, "-4")
    }

    func testNoHandlersMeansNoEventsButTheTextStillSteps() throws {
        let (node, heard, session) = fixture(handlers: [])
        defer { session.destroy() }
        let stepper = try XCTUnwrap(node.stepper)
        press(node, up: true)
        XCTAssertEqual(node.field?.text, "4")
        XCTAssertEqual(heard(), [])
        node.handlers = ["change"]
        press(node, up: true)
        XCTAssertEqual(heard(), ["change 5"])
    }

    func testTheAppsValueMovesTheStepperAndADisabledOrReadonlyFieldStepsNot() throws {
        let (node, heard, session) = fixture()
        defer { session.destroy() }
        let stepper = try XCTUnwrap(node.stepper)
        node.applyProps(set: ["value": "42"], clear: [])
        XCTAssertEqual(node.field?.text, "42")
        XCTAssertEqual(stepper.value, 42)
        node.applyProps(set: ["disabled": "true"], clear: [])
        XCTAssertFalse(stepper.isEnabled)
        node.applyProps(set: ["editable": "false"], clear: ["disabled"])
        XCTAssertFalse(stepper.isEnabled, "a readonly field's spin buttons do nothing")
        press(node, up: true)
        XCTAssertEqual(node.field?.text, "42")
        XCTAssertEqual(heard(), [])
        node.applyProps(set: [:], clear: ["editable"])
        XCTAssertTrue(stepper.isEnabled)
    }

    /// Typing into the field, as UIKit reports it: the text changes and says so.
    private func type(_ node: NodeView, _ text: String) throws {
        let field = try XCTUnwrap(node.field)
        field.text = text
        node.fieldChanged()
    }

    /// LLP 1116.008 D5 (grocery-list, round 10): a typed 250 against a `max` of
    /// 99 saved 250 while the stepper's plus was dimmed. The edit commits as the
    /// bound it passed, written into the field and told as a step is: `input`
    /// (the app that listens to it holds the same number the field shows), then
    /// the `change`. What was typed while typing is unchanged.
    func testATypedValuePastABoundCommitsAsTheBoundAndTheFieldShowsIt() throws {
        let (node, heard, session) = fixture()
        defer { session.destroy() }
        let field = try XCTUnwrap(node.field), stepper = try XCTUnwrap(node.stepper)
        try type(node, "250")
        XCTAssertEqual(heard(), ["input 250"], "typing is as it was")
        XCTAssertEqual(field.text, "250")
        node.textFieldDidEndEditing(field)
        XCTAssertEqual(heard(), ["input 250", "input 99", "change 99"])
        XCTAssertEqual(field.text, "99")
        XCTAssertEqual(stepper.value, 99, "the stepper shows it")
        // Under the floor the same way, from an edit Return commits.
        try type(node, "0")
        _ = node.textFieldShouldReturn(field)
        XCTAssertEqual(heard().suffix(3), ["input 0", "input 1", "change 1"])
        XCTAssertEqual(field.text, "1")
        XCTAssertEqual(stepper.value, 1)
        // The commit that ends the editing after Return says nothing again.
        let count = heard().count
        node.textFieldDidEndEditing(field)
        XCTAssertEqual(heard().count, count)
        // A fraction past the top comes back to it, and an edit within the bounds is as it was.
        try type(node, "99.5")
        node.textFieldDidEndEditing(field)
        XCTAssertEqual(field.text, "99")
        try type(node, "42")
        node.textFieldDidEndEditing(field)
        XCTAssertEqual(field.text, "42")
        XCTAssertEqual(heard().suffix(2), ["input 42", "change 42"])
    }

    func testTextThatIsNoNumberAndAnEditWithoutBoundsAreLeftAlone() throws {
        let (node, heard, session) = fixture()
        defer { session.destroy() }
        let field = try XCTUnwrap(node.field)
        try type(node, "abc")
        node.textFieldDidEndEditing(field)
        XCTAssertEqual(field.text, "abc")
        XCTAssertEqual(heard(), ["input abc", "change abc"])
        node.applyProps(set: ["value": ""], clear: ["min", "max"])
        try type(node, "250")
        node.textFieldDidEndEditing(field)
        XCTAssertEqual(field.text, "250", "no bounds written: the web's, none")
        XCTAssertEqual(heard().suffix(2), ["input 250", "change 250"])
    }

    func testOnlyAFieldWithAStepperHoldsItsBoundsAndTheHandlersItHasHearIt() throws {
        // `appearance="textfield"`: the bare field, whose bounds are the browser's validation.
        let (bare, bareHeard, bareSession) = fixture()
        defer { bareSession.destroy() }
        bare.applyStyle(["appearance": .string("textfield")])
        XCTAssertNil(bare.stepper)
        try type(bare, "250")
        bare.textFieldDidEndEditing(try XCTUnwrap(bare.field))
        XCTAssertEqual(bare.field?.text, "250")
        XCTAssertEqual(bareHeard(), ["input 250", "change 250"])
        // Another type never has a stepper, whatever its bounds say.
        let (text, textHeard, textSession) = fixture(["type": "text"])
        defer { textSession.destroy() }
        try type(text, "250")
        text.textFieldDidEndEditing(try XCTUnwrap(text.field))
        XCTAssertEqual(text.field?.text, "250")
        XCTAssertEqual(textHeard(), ["input 250", "change 250"])
        // A handler the app has not written is not called.
        let (change, changeHeard, changeSession) = fixture(handlers: ["change"])
        defer { changeSession.destroy() }
        try type(change, "250")
        change.textFieldDidEndEditing(try XCTUnwrap(change.field))
        XCTAssertEqual(changeHeard(), ["change 99"])
        let (input, inputHeard, inputSession) = fixture(handlers: ["input"])
        defer { inputSession.destroy() }
        try type(input, "250")
        input.textFieldDidEndEditing(try XCTUnwrap(input.field))
        XCTAssertEqual(inputHeard(), ["input 250", "input 99"])
        XCTAssertEqual(input.field?.text, "99")
    }

    func testTheFieldKeepsPriorityInABoxTooNarrowForBoth() throws {
        let (node, _, session) = fixture(width: 120)
        defer { session.destroy() }
        let field = try XCTUnwrap(node.field)
        let stepper = try XCTUnwrap(node.stepper)
        XCTAssertTrue(stepper.isHidden, "less than 28 pt for text beside the stepper: no stepper")
        XCTAssertEqual(field.textRect(forBounds: field.bounds).maxX, 114, "the text keeps the field")
        node.bounds = CGRect(x: 0, y: 0, width: 200, height: 44)
        node.applyFieldContent(["rect": [6.0, 5.0, 188.0, 34.0]])
        XCTAssertFalse(stepper.isHidden)
        XCTAssertLessThan(field.textRect(forBounds: field.bounds).maxX, 114)
    }

    func testAStepperInAGroupedRowsBorderlessFieldSitsAtTheCellsEdge() throws {
        let (node, _, session) = fixture(width: 338, rect: [0, 0, 338, 22])
        defer { session.destroy() }
        node.bounds = CGRect(x: 0, y: 0, width: 338, height: 22)
        node.applyFieldContent(["rect": [0.0, 0.0, 338.0, 22.0], "cell": true])
        let stepper = try XCTUnwrap(node.stepper)
        XCTAssertEqual(node.field?.borderStyle, UITextField.BorderStyle.none)
        XCTAssertEqual(stepper.frame.maxX, 338)
        XCTAssertEqual(stepper.frame.midY, 11, accuracy: 0.5, "centered on the line, in a row that is taller")
    }

    func testRightToLeftPutsTheStepperAtTheLeadingEdge() throws {
        let (node, _, session) = fixture()
        defer { session.destroy() }
        node.applyStyle(["direction": .string("rtl")])
        let stepper = try XCTUnwrap(node.stepper)
        let field = try XCTUnwrap(node.field)
        XCTAssertEqual(stepper.frame.minX, 6)
        XCTAssertEqual(field.textRect(forBounds: field.bounds).minX, 6 + stepper.frame.width + NodeView.stepperGap, accuracy: 0.001)
        XCTAssertEqual(field.textRect(forBounds: field.bounds).maxX, 294)
    }
}

private extension NodeView {
    /// What the stepper takes of the text at its end, as the tests reckon
    /// the bare field's rectangle back: the stepper's width and gap, for a
    /// view that still had one a moment ago.
    var stepperRoomForTests: CGFloat { UIStepper().intrinsicContentSize.width + NodeView.stepperGap }
}
#endif
