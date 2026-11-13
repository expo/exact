#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// HTML's empty date field on iOS (bench t7-wizard, 2026-10-08; LLP
/// 1115.000 D13.1): a `UIDatePicker` always holds a date, so while the bound
/// value is "" its content is masked under the platform's pill, which holds
/// the author's `placeholder` and, with none, no words: never today's date,
/// and never a browser's `mm/dd/yyyy`. It still takes the tap that opens it.
/// UIKit, so a simulator runs it: bun host/apple/build.mjs --test --ios
final class DatesIOSTests: XCTestCase {
    /// `sendActions(for:)` reaches the picker's target through the
    /// application, which the bare XCTest agent does not have: there it
    /// sends nothing, so these run hosted.
    static let actions = "UIKit sends a control's actions through the app, which only a hosted run has"

    func testAFocusHandlerDisablingOrMakingThePickerInertLeavesItEmpty() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.actions) }
        for prop in ["disabled", "inert"] {
            let p = Presenter()
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
            defer { window.isHidden = true; p.onFocus = nil; p.onControlValue = nil }
            p.viewport.frame = window.bounds
            window.addSubview(p.viewport)
            window.makeKeyAndVisible()
            p.apply(wireBatch([
                ["op": "create", "id": 1, "kind": "view"],
                ["op": "create", "id": 2, "kind": "control",
                 "props": ["type": "date", "value": "", "min": "2026-11-12", "max": "2026-11-12"],
                 "handlers": ["focus", "input", "change"], "style": [:]],
                ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 400],
                ["op": "frame", "id": 2, "x": 20, "y": 20, "w": 300, "h": 34],
            ]))
            let owner = try XCTUnwrap(p.views[2])
            let picker = try XCTUnwrap(p.controls.controls[2] as? BoundDatePicker)
            let changed = prop == "inert" ? 1 : 2
            var focused: [UInt32] = [], values: [String] = []
            p.onFocus = { id in
                focused.append(id)
                p.apply(wireBatch([["op": "props", "id": changed, "set": [prop: "true"]]]))
            }
            p.onControlValue = { id, value, input, change in
                XCTAssertTrue(input && change)
                values.append(value)
                p.apply(wireBatch([["op": "props", "id": id, "set": ["value": value]]]))
            }
            picker.sendActions(for: .editingDidBegin)
            XCTAssertEqual(focused, [2])
            XCTAssertTrue(p.views[2] === owner && p.controls.controls[2] === picker)
            XCTAssertEqual(picker.isEnabled, prop != "disabled")
            XCTAssertEqual(owner.inert, prop == "inert")
            XCTAssertEqual(values, [], "a focus handler setting \(prop) prevents acceptance")
            XCTAssertTrue(picker.empty)
            XCTAssertEqual(owner.props["value"], "")
            p.onFocus = nil
            p.apply(wireBatch([["op": "props", "id": changed, "clear": [prop]]]))
            picker.sendActions(for: .editingDidBegin)
            XCTAssertEqual(values, ["2026-11-12"], "the enabled picker accepts its value when reopened")
            XCTAssertFalse(picker.empty)
            XCTAssertEqual(owner.props["value"], "2026-11-12")
        }
    }

    func testAFocusHandlerGivingThePickerTheAuthorsLookLeavesItEmpty() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.actions) }
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true; p.onFocus = nil; p.onControlValue = nil }
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "control",
             "props": ["type": "date", "value": "", "min": "2026-11-12", "max": "2026-11-12"],
             "handlers": ["focus", "input", "change"], "style": [:]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 400],
            ["op": "frame", "id": 2, "x": 20, "y": 20, "w": 300, "h": 34],
        ]))
        let picker = try XCTUnwrap(p.controls.controls[2] as? BoundDatePicker)
        XCTAssertNotNil(picker.window)
        var values: [String] = []
        // `appearance: none` takes the picker out of the window and keeps it as the node's control.
        p.onFocus = { _ in p.apply(wireBatch([["op": "style", "id": 2, "style": ["appearance": "none"]]])) }
        p.onControlValue = { _, value, _, _ in values.append(value) }
        picker.sendActions(for: .editingDidBegin)
        XCTAssertTrue(p.controls.controls[2] === picker)
        XCTAssertNil(picker.window)
        XCTAssertEqual(values, [], "a picker the focus handler took off screen accepts nothing")
        XCTAssertTrue(picker.empty)
    }

    func testAFocusHandlerReplacingThePickerDoesNotAcceptTheDetachedPickersValue() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.actions) }
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true; p.onFocus = nil; p.onControlValue = nil }
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "control",
             "props": ["type": "date", "value": "", "min": "2026-11-12", "max": "2026-11-12"],
             "handlers": ["focus", "input", "change"], "style": [:]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 20, "y": 20, "w": 300, "h": 34],
        ]))
        let owner = try XCTUnwrap(p.views[1])
        let picker = try XCTUnwrap(p.controls.controls[1] as? BoundDatePicker)
        var focused: [UInt32] = [], values: [String] = []
        p.onFocus = { id in
            focused.append(id)
            p.apply(wireBatch([["op": "props", "id": id,
                               "set": ["type": "time", "min": "09:30", "max": "09:30"]]]))
        }
        p.onControlValue = { id, value, input, change in
            XCTAssertTrue(input && change)
            values.append(value)
            p.apply(wireBatch([["op": "props", "id": id, "set": ["value": value]]]))
        }
        picker.sendActions(for: .editingDidBegin)
        XCTAssertEqual(focused, [1])
        XCTAssertTrue(p.views[1] === owner)
        let replacement = try XCTUnwrap(p.controls.controls[1] as? BoundDatePicker)
        XCTAssertFalse(replacement === picker)
        XCTAssertNil(picker.superview)
        XCTAssertEqual(values, [], "opening the detached picker cannot report a value for its replacement")
        XCTAssertTrue(replacement.empty)
        XCTAssertEqual(owner.props["value"], "")
        p.onFocus = nil
        replacement.sendActions(for: .editingDidBegin)
        XCTAssertEqual(values, ["09:30"], "the replacement accepts its own bounds when it opens")
        XCTAssertFalse(replacement.empty)
        XCTAssertEqual(owner.props["value"], "09:30")
    }

    func testOpeningAnEmptyPickerAcceptsItsInitialValueAndCanRepeatAfterClearing() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.actions) }
        let p = Presenter()
        // A picker accepts a value only on screen, as a tap requires.
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true; p.onControlValue = nil }
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch([
            ["op": "create", "id": 9, "kind": "view"], ["op": "roots", "ids": [9]],
            ["op": "frame", "id": 9, "x": 0, "y": 0, "w": 400, "h": 400],
        ]))
        for (i, kind) in ["date", "time", "datetime-local"].enumerated() {
            let id = i + 1
            let value = kind == "date" ? "2026-11-12" : kind == "time" ? "14:30" : "2026-11-12T14:30"
            p.apply(wireBatch([
                ["op": "create", "id": id, "kind": "control",
                 "props": ["type": kind, "value": "", "min": value, "max": value], "handlers": ["input", "change"], "style": [:]],
                ["op": "children", "id": 9, "ids": Array(1...id)],
                ["op": "frame", "id": id, "x": 20, "y": 20 + 40 * i, "w": 300, "h": 34],
            ]))
            let picker = try XCTUnwrap(p.controls.controls[UInt32(id)] as? BoundDatePicker)
            XCTAssertNotNil(picker.window)
            var values: [String] = []
            p.onControlValue = { _, value, input, change in
                XCTAssertTrue(input && change)
                values.append(value)
            }
            for _ in 0..<2 {
                XCTAssertTrue(picker.empty)
                picker.sendActions(for: .editingDidBegin)
                XCTAssertEqual(values, [value])
                XCTAssertEqual(DateValue.shown(kind, picker), value)
                XCTAssertFalse(picker.empty)
                picker.sendActions(for: .editingDidBegin)
                XCTAssertEqual(values, [value], "opening a filled picker sends no change")
                p.apply(wireBatch([["op": "props", "id": id, "set": ["value": value]]]))
                p.apply(wireBatch([["op": "props", "id": id, "set": ["value": ""]]]))
                values.removeAll()
            }
            p.apply(wireBatch([["op": "props", "id": id, "clear": ["min", "max"]]]))
            let before = DateValue.format(kind, DateValue.initial(kind))
            picker.sendActions(for: .editingDidBegin)
            let after = DateValue.format(kind, DateValue.initial(kind))
            XCTAssertEqual(values.count, 1)
            XCTAssertTrue([before, after].contains(try XCTUnwrap(values.first)), "a cleared picker starts at now, not its previous value")
            XCTAssertEqual(DateValue.shown(kind, picker), values.first)
        }
    }

    func testEmptyDateValuesReachTheElementsVoiceOverActuallyReads() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's accessibility runtime is not loaded") }
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true }
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view"]]
        for (i, kind) in ["date", "time", "datetime-local"].enumerated() {
            let id = i + 2
            ops += [["op": "create", "id": id, "kind": "control", "props": ["type": kind, "value": "", "placeholder": "Add a date", "testId": kind]],
                ["op": "frame", "id": id, "x": 20, "y": 20 + 60 * i, "w": 300, "h": 34]]
        }
        ops += [["op": "children", "id": 1, "ids": [2, 3, 4]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 400]]
        p.apply(wireBatch(ops))
        window.layoutIfNeeded()
        func elements(_ id: Int) -> [[String: Any]] {
            (p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []).filter { ($0["id"] as? UInt32) == UInt32(id) }
        }
        for id in 2...4 {
            let rows = elements(id)
            XCTAssertFalse(rows.isEmpty, "the native picker is exposed")
            XCTAssertTrue(rows.allSatisfy { $0["value"] as? String == "Add a date" }, "\(rows)")
            // Exact's own elements say it, standing for UIKit's, whose classes
            // are their own: public API, no runtime subclass.
            let picker = try XCTUnwrap(p.controls.controls[UInt32(id)] as? BoundDatePicker)
            let standIns = try XCTUnwrap(picker.accessibilityElements as? [EmptyDateElement])
            XCTAssertEqual(standIns.count, rows.count)
            for element in standIns {
                let native = try XCTUnwrap(element.native)
                XCTAssertFalse(NSStringFromClass(type(of: native)).hasPrefix("ExactEmptyDate_"))
                XCTAssertNotEqual(native.accessibilityValue, "Add a date", "UIKit's element is left as it is")
                XCTAssertEqual(element.accessibilityLabel, native.accessibilityLabel)
                XCTAssertEqual(element.accessibilityTraits, native.accessibilityTraits)
                XCTAssertEqual(element.accessibilityFrame, native.accessibilityFrame)
                XCTAssertEqual(element.accessibilityActivationPoint, native.accessibilityActivationPoint, "VoiceOver's tap lands on UIKit's content")
            }
        }
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["placeholder": "Choose a day"]]]))
        window.layoutIfNeeded()
        XCTAssertEqual(elements(2).compactMap { $0["value"] as? String }, ["Choose a day"])
        p.apply(wireBatch([["op": "props", "id": 2, "clear": ["placeholder"]]]))
        window.layoutIfNeeded()
        XCTAssertEqual(elements(2).count, 1)
        XCTAssertTrue(elements(2).allSatisfy { $0["value"] == nil }, "no placeholder means no date value")
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["value": "2026-01-02"]]]))
        window.layoutIfNeeded()
        let dateRows = elements(2)
        XCTAssertFalse(dateRows.isEmpty)
        XCTAssertTrue(dateRows.allSatisfy { ($0["value"] as? String)?.contains("2026") == true }, "a bound date restores UIKit's value: \(dateRows)")
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["value": "", "placeholder": "Add again"]]]))
        window.layoutIfNeeded()
        XCTAssertEqual(elements(2).compactMap { $0["value"] as? String }, ["Add again"])
        let picker = try XCTUnwrap(p.controls.controls[2] as? UIDatePicker)
        picker.setDate(try XCTUnwrap(DateValue.parse("date", "2026-01-02")), animated: false)
        picker.sendActions(for: .valueChanged)
        window.layoutIfNeeded()
        XCTAssertTrue(elements(2).allSatisfy { ($0["value"] as? String)?.contains("2026") == true }, "a user choice restores UIKit's value")
    }

    func testDateTimeAndDatetimeAlignTheirCompactControlWithinTheAuthoredWidth() throws {
        let p = Presenter(), window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true }
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        for (i, kind) in ["date", "time", "datetime-local"].enumerated() {
            let id = i + 1
            p.apply(wireBatch([
                ["op": "create", "id": id, "kind": "control", "props": ["type": kind], "style": [:]],
                ["op": "roots", "ids": Array(1...id)],
                ["op": "frame", "id": id, "x": 20.0, "y": Double(i * 80), "w": 340.0, "h": 60.0],
            ]))
            let control = try XCTUnwrap(p.controls.controls[UInt32(id)] as? UIDatePicker)
            for (alignment, direction, edge) in [("left", "ltr", "left"), ("right", "ltr", "right"),
                                                 ("start", "rtl", "right"), ("end", "rtl", "left"), ("center", "ltr", "center")] {
                p.apply(wireBatch([["op": "style", "id": id, "style": ["text_align": alignment, "direction": direction, "written": ["text_align"]]]]))
                let owner = try XCTUnwrap(p.views[UInt32(id)]), box = owner.contentBox()
                let x = edge == "left" ? box.minX : edge == "right" ? box.maxX - control.frame.width : box.midX - control.frame.width / 2
                XCTAssertEqual(control.frame.minX, x, accuracy: 0.5, "\(kind) \(alignment) \(direction)")
                XCTAssertEqual(control.frame.midY, box.midY, accuracy: 0.5)
            }
        }
    }

    func testAnEmptyDateShowsItsAuthorsPlaceholderOrNoWordsNeverToday() throws {
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true }
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        // Two empty dates as a batch gives them: one with a placeholder.
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["id": "page"]],
            ["op": "create", "id": 2, "kind": "control", "props": ["type": "date", "value": ""], "handlers": ["change"], "style": [:]],
            ["op": "create", "id": 3, "kind": "control", "props": ["type": "date", "value": "", "placeholder": "Add a date"], "handlers": ["change"], "style": [:]],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 20.0, "y": 20.0, "w": 160.0, "h": 34.0],
            ["op": "frame", "id": 3, "x": 20.0, "y": 80.0, "w": 160.0, "h": 34.0],
        ]))
        let bare = try XCTUnwrap(p.controls.controls[2] as? BoundDatePicker), said = try XCTUnwrap(p.controls.controls[3] as? BoundDatePicker)
        bare.layoutIfNeeded(); said.layoutIfNeeded()
        XCTAssertEqual(try XCTUnwrap(bare.placeholderShown, "an empty picker shows its pill").text, "", "with no placeholder, no words: not a date format")
        // No words at all: across the pill's middle every pixel is the
        // pill's fill, with nothing of today's date under it.
        let pill = try XCTUnwrap(bare.placeholderShown).frame
        let format = UIGraphicsImageRendererFormat(); format.scale = 1; format.opaque = true
        let drawn = UIGraphicsImageRenderer(bounds: bare.bounds, format: format).image { context in
            UIColor.white.setFill(); context.fill(bare.bounds)
            bare.layer.render(in: context.cgContext)
        }
        let cg = try XCTUnwrap(drawn.cgImage), w = cg.width, h = cg.height
        var data = [UInt8](repeating: 0, count: w * h * 4)
        let bitmap = try XCTUnwrap(CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpaceCreateDeviceRGB(),
                                             bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        bitmap.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))
        var seen = Set<[UInt8]>()
        for y in [Int(pill.midY) - 3, Int(pill.midY), Int(pill.midY) + 3] where y >= 0 && y < h {
            for x in Int(pill.minX + 10)..<Int(pill.maxX - 10) where x >= 0 && x < w { seen.insert(Array(data[(y * w + x) * 4..<(y * w + x) * 4 + 4])) }
        }
        XCTAssertEqual(seen.count, 1, "one colour across the pill: \(seen)")
        XCTAssertTrue(seen.allSatisfy { CapturePixels.opaque($0, at: 0) }, "the pill's fill is opaque")
        XCTAssertNotEqual(seen.first, [255, 255, 255, 255], "the pill's fill, not the page")
        XCTAssertEqual(try XCTUnwrap(said.placeholderShown).text, "Add a date", "the author's placeholder, in the platform's pill")
        // The words follow the prop, and go with it.
        p.apply(wireBatch([["op": "props", "id": 3, "set": ["placeholder": "No date"], "clear": []]]))
        said.layoutIfNeeded()
        XCTAssertEqual(said.placeholderShown?.text, "No date")
        p.apply(wireBatch([["op": "props", "id": 3, "set": [:], "clear": ["placeholder"]]]))
        said.layoutIfNeeded()
        XCTAssertEqual(said.placeholderShown?.text, "")

        let picker = try XCTUnwrap(p.controls.makeDate("date") as? BoundDatePicker)
        picker.locale = Locale(identifier: "en_US")
        picker.frame = CGRect(x: 20, y: 140, width: 160, height: 34)
        window.addSubview(picker)
        picker.layoutIfNeeded()
        let fitted = picker.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize)
        picker.empty = true
        picker.layoutIfNeeded()
        let shown = try XCTUnwrap(picker.placeholderShown, "an empty picker shows its pill")
        XCTAssertEqual(shown.text, "", "no words of Exact's own")
        XCTAssertGreaterThan(shown.frame.width, 0, "the pill has a size")
        XCTAssertGreaterThan(shown.frame.height, 0)
        XCTAssertEqual(picker.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize), fitted,
                       "the placeholder takes no part in the picker's measured size")
        XCTAssertEqual(picker.accessibilityValue, picker.placeholder, "VoiceOver hears the placeholder")
        XCTAssertFalse(picker.subviews.isEmpty)
        for view in picker.subviews {
            XCTAssertNotNil(view.layer.mask, "today's date is masked out whole")
            XCTAssertTrue(view.alpha == 1 && !view.isHidden, "and its content still takes the tap that opens it")
        }
        let middle = CGPoint(x: picker.bounds.midX, y: picker.bounds.midY)
        let hit = try XCTUnwrap(picker.hitTest(middle, with: nil), "a tap on the pill lands in the picker")
        XCTAssertTrue(hit !== picker && hit.isDescendant(of: picker), "on UIKit's own content, which opens the calendar")
        XCTAssertEqual(DateValue.shown("date", picker), "", "the value it reports is HTML's empty one")
        picker.empty = false
        picker.layoutIfNeeded()
        XCTAssertNil(picker.placeholderShown, "a value takes the pill away")
        XCTAssertTrue(picker.subviews.allSatisfy { $0.layer.mask == nil }, "and shows the picker's own content")
    }
}
#endif
