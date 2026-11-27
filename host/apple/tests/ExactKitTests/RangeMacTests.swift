#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactControls

/// `input type="range"` on macOS is an `NSSlider` whose `trackFillColor`
/// follows CSS `accent-color`, restores the native fill on clear, and survives
/// value edits and appearance changes.
final class RangeMacTests: XCTestCase {
    private var window: NSWindow!

    override func tearDown() {
        window?.close()
        window = nil
        super.tearDown()
    }

    private func channels(_ color: NSColor?) throws -> [Double] {
        let resolved = try XCTUnwrap(color?.usingColorSpace(.sRGB))
        return [resolved.redComponent, resolved.greenComponent, resolved.blueComponent, resolved.alphaComponent]
            .map { Double(min(max($0, 0), 1)) * 255 }
    }

    private func assertChannels(_ got: NSColor?, _ want: [Double], _ message: String, file: StaticString = #filePath, line: UInt = #line) throws {
        let actual = try channels(got)
        for i in 0..<4 {
            XCTAssertEqual(actual[i], want[i], accuracy: 0.51, "\(message): \(actual) is not \(want)", file: file, line: line)
        }
    }

    private func makePresenter(light: Bool = true, ops: [[String: Any]]) -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 120), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: light ? .aqua : .darkAqua)
        window.contentView = p.viewport
        p.apply(wireBatch(ops))
        return p
    }

    func testAccentColorSetsTrackFillColorAndClearsOnAutoOrUnset() throws {
        let magenta: [Any] = [224, 33, 138, 255]
        let teal: [Any] = [48, 176, 199, 255]
        let p = makePresenter(ops: [
            ["op": "create", "id": 1, "kind": "control",
             "props": ["type": "range", "min": "0", "max": "100", "value": "60"],
             "handlers": ["input", "change"],
             "style": ["accent_color": magenta]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 240.0, "h": 24.0],
        ])
        let slider = try XCTUnwrap(p.controls.controls[1] as? NSSlider)
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "initial authored accent-color")

        p.apply(wireBatch([["op": "style", "id": 1, "style": ["accent_color": teal]]]))
        try assertChannels(slider.trackFillColor, [48, 176, 199, 255], "updated accent-color")

        p.apply(wireBatch([["op": "style", "id": 1, "style": ["accent_color": "auto"]]]))
        XCTAssertNil(slider.trackFillColor, "auto restores system accent")

        p.apply(wireBatch([["op": "style", "id": 1, "style": ["accent_color": magenta]]]))
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "re-applied accent-color")

        p.apply(wireBatch([["op": "style", "id": 1, "style": [:]]]))
        XCTAssertNil(slider.trackFillColor, "unset restores system accent")
    }

    func testTrackFillColorFollowsLightDarkAndInheritedTint() throws {
        let pair: [Any] = [[224, 33, 138, 255], [64, 200, 224, 255]]
        let tint: [String: Any] = ["sys": "@tint", "c": [[0, 136, 255, 255], [0, 145, 255, 255]]]
        let orange: [Any] = [255, 149, 0, 255]
        let p = makePresenter(light: true, ops: [
            ["op": "create", "id": 1, "kind": "view", "style": ["accent_color": pair]],
            ["op": "create", "id": 2, "kind": "control",
             "props": ["type": "range", "min": "0", "max": "100", "value": "50"],
             "handlers": ["input", "change"],
             "style": ["accent_color": pair]],
            ["op": "create", "id": 3, "kind": "control",
             "props": ["type": "range", "min": "0", "max": "100", "value": "50"],
             "handlers": ["input", "change"],
             "style": ["accent_color": tint]],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 80.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 240.0, "h": 24.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 32.0, "w": 240.0, "h": 24.0],
        ])
        let direct = try XCTUnwrap(p.controls.controls[2] as? NSSlider)
        let tinted = try XCTUnwrap(p.controls.controls[3] as? NSSlider)
        try assertChannels(direct.trackFillColor, [224, 33, 138, 255], "light scheme")
        try assertChannels(tinted.trackFillColor, [224, 33, 138, 255], "light @tint")

        window.appearance = NSAppearance(named: .darkAqua)
        try assertChannels(direct.trackFillColor, [64, 200, 224, 255], "dark scheme after appearance change")
        try assertChannels(tinted.trackFillColor, [64, 200, 224, 255], "dark @tint after appearance change")

        p.apply(wireBatch([["op": "style", "id": 1, "style": ["accent_color": orange]]]))
        try assertChannels(tinted.trackFillColor, [255, 149, 0, 255], "@tint updated when ancestor accent changes")
    }

    func testTrackFillColorSurvivesRangeChangedAndTypeRange() throws {
        let magenta: [Any] = [224, 33, 138, 255]
        let p = makePresenter(ops: [
            ["op": "create", "id": 1, "kind": "control",
             "props": ["type": "range", "min": "0", "max": "100", "step": "1", "value": "40"],
             "handlers": ["input", "change"],
             "style": ["accent_color": magenta]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 240.0, "h": 24.0],
        ])
        let node = try XCTUnwrap(p.views[1])
        let slider = try XCTUnwrap(p.controls.controls[1] as? NSSlider)
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "before value edits")
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["disabled": "true"]]]))
        XCTAssertFalse(slider.isEnabled)
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "disabled range retains its fill")
        p.apply(wireBatch([["op": "props", "id": 1, "clear": ["disabled"]]]))
        XCTAssertTrue(slider.isEnabled)

        slider.doubleValue = 75
        p.controls.rangeChanged(slider)
        XCTAssertEqual(slider.doubleValue, 75)
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "preserved after rangeChanged")

        _ = p.controls.typeRange(slider, node, "25")
        XCTAssertEqual(slider.doubleValue, 25)
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "preserved after typeRange")
    }

    func testAppearanceRefreshesControlsWithoutWritingTheirValues() throws {
        let pair: [Any] = [[224, 33, 138, 255], [64, 200, 224, 255]]
        let tint: [String: Any] = ["sys": "@tint", "c": pair]
        let controls: [[String: Any]] = ["range", "checkbox", "radio"].enumerated().map { index, kind in
            ["op": "create", "id": index + 1, "kind": "control",
             "props": ["type": kind, "checked": "true"], "style": ["accent_color": tint]]
        }
        let p = makePresenter(ops: controls + [
            ["op": "create", "id": 4, "kind": "view", "style": ["accent_color": pair]],
            ["op": "children", "id": 4, "ids": [1, 2, 3]], ["op": "roots", "ids": [4]],
        ])
        let slider = try XCTUnwrap(p.controls.controls[1] as? NSSlider)
        let toggles = try [2, 3].map { try XCTUnwrap(p.controls.controls[UInt32($0)] as? NSButton) }
        // A hatch changes the native limit. Resolving a colour must leave that limit alone.
        slider.maxValue = 80
        window.appearance = NSAppearance(named: .darkAqua)
        XCTAssertEqual(slider.maxValue, 80)
        try assertChannels(slider.trackFillColor, [64, 200, 224, 255], "dark range")
        for toggle in toggles {
            try assertChannels(toggle.contentTintColor, [64, 200, 224, 255], "dark toggle")
        }
        p.apply(wireBatch([["op": "style", "id": 4, "style": ["accent_color": [255, 149, 0, 255]]]]))
        try assertChannels(slider.trackFillColor, [255, 149, 0, 255], "new parent tint on range")
        for (index, toggle) in toggles.enumerated() {
            try assertChannels(toggle.contentTintColor, [255, 149, 0, 255], "new parent tint")
            toggle.contentTintColor = .systemPurple
            p.apply(wireBatch([["op": "style", "id": index + 2, "style": [:]]]))
            XCTAssertEqual(toggle.contentTintColor, .systemPurple)
        }
        XCTAssertEqual(slider.maxValue, 80)
        let owner = try XCTUnwrap(p.views[1])
        owner.style["accent_color"] = .array([48, 176, 199, 255])
        owner.systemColorsChanged()
        try assertChannels(slider.trackFillColor, [48, 176, 199, 255], "system colour notification")
        XCTAssertEqual(slider.maxValue, 80)
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["max": "100"]]]))
        XCTAssertEqual(slider.maxValue, 100, "the author set the default maximum explicitly")
    }

    func testNativeTrackFillSurvivesAnUnrelatedUpdate() throws {
        let p = makePresenter(ops: [
            ["op": "create", "id": 1, "kind": "control", "props": ["type": "range"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 2, "ids": [1]],
            ["op": "roots", "ids": [2]],
        ])
        let slider = try XCTUnwrap(p.controls.controls[1] as? NSSlider)
        XCTAssertNil(slider.trackFillColor)
        slider.trackFillColor = .systemOrange
        XCTAssertEqual(slider.trackFillColor, .systemOrange)
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["accessibilityLabel": "New label"]]]))
        XCTAssertEqual(slider.trackFillColor, .systemOrange)
        func accent(_ value: [String: Any]) {
            p.apply(wireBatch([["op": "style", "id": 1, "style": value]]))
        }
        let pink: [Any] = [224, 33, 138, 255]
        accent(["accent_color": pink])
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "authored fill")
        accent([:])
        XCTAssertEqual(slider.trackFillColor, .systemOrange)

        accent(["accent_color": pink])
        slider.trackFillColor = .systemPurple
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["value": "42"]]]))
        XCTAssertEqual(slider.trackFillColor, .systemPurple)
        accent(["accent_color": "auto"])
        XCTAssertEqual(slider.trackFillColor, .systemPurple)

        accent(["accent_color": pink])
        slider.trackFillColor = try XCTUnwrap(slider.trackFillColor)
        accent([:])
        try assertChannels(slider.trackFillColor, [224, 33, 138, 255], "the hatch wrote the same pink")
    }
}
#endif
