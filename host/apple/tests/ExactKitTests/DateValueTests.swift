import Foundation
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif
import XCTest
@testable import ExactKit

final class DateValueTests: XCTestCase {
    func testLocalDateTimesAcceptHTMLSeparatorsAndOptionalSeconds() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "control", "props": ["type": "datetime-local", "value": ""], "handlers": ["input", "change"]],
            ["op": "roots", "ids": [1]],
        ]))
        let node = try XCTUnwrap(p.views[1])
        #if os(macOS)
        let picker = try XCTUnwrap(p.controls.controls[1] as? NSDatePicker)
        #elseif os(iOS)
        let picker = try XCTUnwrap(p.controls.controls[1] as? UIDatePicker)
        #endif
        var heard: [String] = []
        p.onControlValue = { _, value, input, change in
            XCTAssertTrue(input && change)
            heard.append(value)
        }
        for text in ["2026-03-04T10:30", "2026-03-04T10:30:00", "2026-03-04 10:30", "2026-03-04T10:30:45.500"] {
            XCTAssertNotNil(DateValue.parse("datetime-local", text), text)
            #if os(macOS) || os(iOS)
            XCTAssertNil(p.controls.typeDate(picker, node, text)["error"], text)
            XCTAssertEqual(heard.last, text)
            #endif
        }
    }

    /// A date control takes what HTML's value grammar takes, the kernel's
    /// rules, and nothing else: no trailing text, a fraction of one to
    /// three digits (#275).
    func testParseTakesHTMLsValueGrammar() throws {
        let at = try XCTUnwrap(DateValue.parse("datetime-local", "2026-03-04T10:30"))
        for text in ["2026-03-04 10:30", "2026-03-04T10:30:00", "2026-03-04 10:30:00.000"] {
            XCTAssertEqual(DateValue.parse("datetime-local", text), at, text)
        }
        XCTAssertEqual(DateValue.parse("datetime-local", "2026-03-04T10:30:45.5"), at.addingTimeInterval(45.5))
        XCTAssertNotNil(DateValue.parse("date", "2024-02-29"))
        XCTAssertNotNil(DateValue.parse("time", "23:59:59.999"))
        for (kind, text) in [("datetime-local", "2026-03-04T10:30:00junk"), ("datetime-local", "2026-03-04t10:30"),
                             ("datetime-local", "2026-03-04"), ("date", "2025-02-29"), ("date", "26-03-04"),
                             ("date", "0000-01-01"), ("time", "24:00"), ("time", "9:30"), ("time", "10:30:00."),
                             ("time", "10:30:00.1234"), ("time", "10:30:45junk")] {
            XCTAssertNil(DateValue.parse(kind, text), "\(kind) \(text)")
        }
    }

    /// A picker's value is written as HTML's normalized one, seconds and
    /// their fraction kept (#275): it had been minutes only.
    func testFormatWritesHTMLsNormalizedValue() throws {
        for (kind, text, normalized) in [
            ("datetime-local", "2026-03-04T10:30:45.500", "2026-03-04T10:30:45.5"),
            ("datetime-local", "2026-03-04 10:30:00.000", "2026-03-04T10:30"),
            ("datetime-local", "02026-03-04T10:30:07", "2026-03-04T10:30:07"),
            ("datetime-local", "2026-03-04T10:30:00.010", "2026-03-04T10:30:00.01"),
            ("datetime-local", "1969-12-31T23:59:59.999", "1969-12-31T23:59:59.999"),
            ("datetime-local", "0099-01-01T00:00", "0099-01-01T00:00"),
            ("datetime-local", "10000-01-01T00:00", "10000-01-01T00:00"),
            ("time", "10:30:45.120", "10:30:45.12"),
            ("time", "10:30:00", "10:30"),
            ("date", "2026-03-04", "2026-03-04"),
        ] {
            let date = try XCTUnwrap(DateValue.parse(kind, text), text)
            XCTAssertEqual(DateValue.format(kind, date), normalized, text)
            XCTAssertEqual(DateValue.parse(kind, normalized), date, "\(normalized) names the same value")
        }
        // A picker still showing what was typed reads it as typed; one an action moved, as it shows.
        let typed = try XCTUnwrap(DateValue.parse("time", "10:30:45.500"))
        XCTAssertEqual(DateValue.shown("time", typed: "10:30:45.500", typed), "10:30:45.500")
        XCTAssertEqual(DateValue.shown("time", typed: "10:30:45.500", typed.addingTimeInterval(-45.5)), "10:30:45.500")
        XCTAssertEqual(DateValue.shown("time", typed: "10:30:45.500", typed.addingTimeInterval(3600)), "11:30:45.5")
    }

    func testInitialValuesUseLocalFieldsAndTheSameRepresentationAsTheirBounds() throws {
        let instant = try XCTUnwrap(DateValue.parse("datetime-local", "2026-11-10T02:30"))
        let zone = try XCTUnwrap(TimeZone(secondsFromGMT: -8 * 60 * 60))
        for (kind, local, later, earlier) in [
            ("date", "2026-11-09", "2026-11-12", "2026-11-01"),
            ("time", "18:30", "19:00", "09:00"),
            ("datetime-local", "2026-11-09T18:30", "2026-11-12T19:00", "2026-11-01T09:00"),
        ] {
            let expected = try XCTUnwrap(DateValue.parse(kind, local))
            let minimum = try XCTUnwrap(DateValue.parse(kind, later))
            let maximum = try XCTUnwrap(DateValue.parse(kind, earlier))
            XCTAssertEqual(DateValue.initial(kind, now: instant, timeZone: zone), expected)
            XCTAssertEqual(DateValue.initial(kind, now: instant, timeZone: zone, minimum: minimum), minimum)
            XCTAssertEqual(DateValue.initial(kind, now: instant, timeZone: zone, maximum: maximum), maximum)
        }
    }
}
