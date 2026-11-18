#if os(macOS) || os(iOS)
import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class SelectSizingTests: XCTestCase {
    func testSelectSizingFollowsTheShownValueOrTheWidestOption() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        var menu = SelectMenu(options: [.init(value: "off", label: "Off", disabled: false),
                                        .init(value: "stop", label: "Until I stop", disabled: false)], chosen: 0)
        p.selectOptions = { _ in menu }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "control", "props": ["type": "select"], "handlers": ["input", "change"]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0],
            ["op": "roots", "ids": [1]],
        ]))
        let node = try XCTUnwrap(p.views[1]), control = try XCTUnwrap(p.controls.controls[1])
        func size() -> CGSize {
            #if os(macOS)
            return p.controls.naturalSize(control)
            #else
            return p.controls.naturalSize(control, node)
            #endif
        }
        func sizing(_ value: String?) {
            let style: [String: Any] = value.map { ["field_sizing": $0, "written": ["field_sizing"]] } ?? [:]
            p.apply(wireBatch([["op": "style", "id": 1, "style": style]]))
        }
        let defaultSize = size()
        sizing("fixed")
        let fixed = size()
        sizing("content")
        let short = size()
        XCTAssertLessThan(short.width, fixed.width)
        XCTAssertEqual(defaultSize, fixed, "a select fits its widest option unless field-sizing is content")
        #if os(macOS)
        let reference = NSPopUpButton(frame: .zero, pullsDown: false)
        reference.addItem(withTitle: "Off")
        XCTAssertEqual(short.width, ceil(reference.cell!.cellSize.width))
        #else
        var configuration = UIButton.Configuration.plain()
        configuration.indicator = .popup
        configuration.contentInsets = .zero
        let reference = UIButton(configuration: configuration)
        reference.showsMenuAsPrimaryAction = true
        reference.changesSelectionAsPrimaryAction = true
        reference.menu = UIMenu(children: menu.options.enumerated().map { index, option in
            UIAction(title: option.label, state: index == 0 ? .on : .off) { _ in }
        })
        XCTAssertEqual(short.width, ceil(reference.intrinsicContentSize.width))
        #endif
        var heard: [String] = []
        p.onControlValue = { _, value, input, change in
            if input { heard.append("input:\(value)") }
            if change { heard.append("change:\(value)") }
            menu.chosen = menu.options.firstIndex { $0.value == value }
        }
        XCTAssertNil(p.controls.type(node, "stop")?["error"])
        XCTAssertGreaterThan(size().width, short.width)
        XCTAssertEqual(heard, ["input:stop", "change:stop"])
        sizing("fixed")
        XCTAssertEqual(size(), fixed)
        XCTAssertNil(p.controls.type(node, "off")?["error"])
        XCTAssertEqual(size(), fixed)
        sizing("content")
        XCTAssertEqual(size(), short)

        // A choice without a bound-value update still changes the shown title.
        p.onControlValue = { _, _, _, _ in }
        p.controls.reportMeasured()
        var reported: CGSize?
        p.onIntrinsic = { sizes in reported = sizes.first { $0.0 == 1 }?.1 }
        XCTAssertNil(p.controls.type(node, "stop")?["error"])
        XCTAssertGreaterThan(size().width, short.width)
        p.controls.reportMeasured()
        XCTAssertEqual(reported, size(), "the new title's size reaches layout even without a bound-value write")
        menu = SelectMenu()
        var batch = wireBatch([])
        batch.controls = true
        p.apply(batch)
        XCTAssertGreaterThan(size().height, 0)
        XCTAssertGreaterThanOrEqual(size().width, 0)
    }
}
#endif
