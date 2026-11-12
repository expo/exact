#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class DatesMacTests: XCTestCase {
    private var window: NSWindow!
    override func tearDown() { window?.close(); window = nil }

    private func fixture(_ kind: String) throws -> (Presenter, DateField) {
        _ = NSApplication.shared
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200),
                          styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "control", "props": ["type": kind, "value": "", "testId": "due"],
             "handlers": ["input", "change"], "style": [:]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 200],
            ["op": "frame", "id": 2, "x": 20, "y": 20, "w": 300, "h": 30],
        ]))
        let field = try XCTUnwrap(p.controls.controls[2] as? DateField)
        field.locale = Locale(identifier: "en_US")
        XCTAssertTrue(window.makeFirstResponder(field))
        return (p, field)
    }

    private func key(_ field: DateField, _ chars: String, _ code: UInt16) throws {
        let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                windowNumber: window.windowNumber, context: nil, characters: chars,
                                                charactersIgnoringModifiers: chars, isARepeat: false, keyCode: code))
        field.keyDown(with: event)
    }

    func testReturnAcceptsAnEmptyFieldsDisplayedValueButFocusAloneDoesNot() throws {
        for kind in ["date", "time", "datetime-local"] {
            let (p, field) = try fixture(kind)
            var values: [String] = []
            p.onControlValue = { _, value, input, change in
                XCTAssertTrue(input && change)
                values.append(value)
            }
            XCTAssertTrue(field.empty)
            try key(field, "\t", 48)
            XCTAssertTrue(values.isEmpty, "Tab does not accept the displayed value")
            XCTAssertTrue(window.makeFirstResponder(nil))
            XCTAssertTrue(values.isEmpty)
            XCTAssertTrue(field.empty, "focus and blur leave \(kind) empty")
            XCTAssertTrue(window.makeFirstResponder(field))
            let held = DateValue.format(kind, field.dateValue)
            try key(field, "\r", 36)
            XCTAssertEqual(values, [held], "Return accepts the displayed \(kind)")
            XCTAssertFalse(field.empty)
            XCTAssertEqual(p.controls.shownDate(field, kind), held)
            try key(field, "\r", 36)
            XCTAssertEqual(values, [held], "Return on a filled field sends no duplicate")
            window.close(); window = nil
        }
    }

    func testTypingTheDisplayedDateAcceptsItWithoutSteppingAwayAndBack() throws {
        let (p, field) = try fixture("date")
        let held = DateValue.format("date", field.dateValue)
        let parts = held.split(separator: "-").map(String.init)
        var values: [String] = []
        p.onControlValue = { _, value, input, change in
            XCTAssertTrue(input && change)
            values.append(value)
        }
        for (i, part) in [parts[1], parts[2], parts[0]].enumerated() {
            if i > 0 { try key(field, "\u{F703}", 124) }
            for digit in part { try key(field, String(digit), 0) }
        }
        XCTAssertTrue(window.makeFirstResponder(nil))
        XCTAssertEqual(values.last, held)
        XCTAssertFalse(field.empty)
        XCTAssertEqual(p.controls.shownDate(field, "date"), held)
    }

    /// The agent's `type` replies with the value the app heard, and a later
    /// change keeps the seconds it gave (#275): the picker had written
    /// minutes only.
    func testATypedLocalDateAndTimeKeepsItsSeconds() throws {
        let (p, field) = try fixture("datetime-local")
        var values: [String] = []
        p.onControlValue = { _, value, _, _ in values.append(value) }
        let node = try XCTUnwrap(p.views[2])
        let reply = p.controls.typeDate(field, node, "2026-03-04T10:30:45.500")
        XCTAssertEqual(values, ["2026-03-04T10:30:45.500"])
        XCTAssertEqual(reply["value"] as? String, "2026-03-04T10:30:45.500")
        XCTAssertEqual(p.controls.shownDate(field, "datetime-local"), "2026-03-04T10:30:45.5")
        try key(field, "\u{F700}", 126)
        XCTAssertEqual(values.count, 2)
        XCTAssertTrue(values[1].hasSuffix(":45.5"), values[1])
        XCTAssertNotNil(p.controls.typeDate(field, node, "2026-03-04T10:30:45junk")["error"])
        XCTAssertEqual(values.count, 2, "a refused value reports nothing")
    }

    func testANativeDateChangeReportsOnce() throws {
        let (p, field) = try fixture("date")
        var values: [String] = []
        p.onControlValue = { _, value, _, _ in values.append(value) }
        try key(field, "\u{F700}", 126)
        XCTAssertEqual(values, [DateValue.format("date", field.dateValue)])
        XCTAssertFalse(field.empty)
    }

    func testAnActionThatClearsTheDateDoesNotCauseASecondReport() throws {
        let (p, field) = try fixture("date")
        var values: [String] = []
        p.onControlValue = { _, value, _, _ in
            values.append(value)
            p.apply(wireBatch([["op": "props", "id": 2, "set": ["value": value]]]))
            p.apply(wireBatch([["op": "props", "id": 2, "set": ["value": ""]]]))
        }
        defer { p.onControlValue = nil }
        try key(field, "\u{F700}", 126)
        XCTAssertEqual(values.count, 1)
        XCTAssertTrue(field.empty)
        try key(field, "\r", 36)
        XCTAssertEqual(values.count, 2, "the next acceptance reports once too")
        XCTAssertTrue(field.empty)
    }
}
#endif
