#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

final class GroupedAccessibilityIOSTests: GroupedListFixture {
    func testAccessibilityOnlyUpdatesRefreshTheVisibleCell() throws {
        let p = presenter { self.model() }
        let milk = try cell(p, 10)
        p.apply(wireBatch([["op": "props", "id": 10, "set": [
            "accessibilityLabel": "Buy milk", "accessibilityRole": "link", "accessibilitySelected": "true"
        ]]]))
        XCTAssertTrue(try cell(p, 10) === milk)
        XCTAssertEqual(milk.accessibilityLabel, "Buy milk")
        // A list cell is a button whatever it opens, as UIKit's is.
        XCTAssertTrue(milk.accessibilityTraits.contains(.button))
        XCTAssertFalse(milk.accessibilityTraits.contains(.link))
        XCTAssertTrue(milk.accessibilityTraits.contains(.selected))
        p.apply(wireBatch([["op": "props", "id": 10, "set": [
            "accessibilityRole": "button", "accessibilitySelected": "false", "accessibilityCurrent": "page"
        ]]]))
        XCTAssertTrue(milk.accessibilityTraits.contains(.button))
        XCTAssertFalse(milk.accessibilityTraits.contains(.link))
        XCTAssertTrue(milk.accessibilityTraits.contains(.selected))
        p.apply(wireBatch([["op": "props", "id": 10, "set": [
            "accessibilityLabel": "Milk again", "accessibilityCurrent": "false"
        ]]]))
        XCTAssertEqual(milk.accessibilityLabel, "Milk again")
        XCTAssertFalse(milk.accessibilityTraits.contains(.selected))
    }

    func testTheDetailButtonSurvivesAccessibilityAndRowUpdates() throws {
        var detail = Row(view: 12, title: "Information", accessory: "detail", target: 13)
        let p = presenter {
            var m = self.model()
            m.sections[0].rows[2] = detail
            return m
        }
        defer { window.isHidden = true }
        let row = try cell(p, 12)
        let list = try list(p)
        let button = try XCTUnwrap(list.accessory(12) as? UIButton)
        var presses: [UInt32] = []
        p.onPress = { presses.append($0) }

        p.apply(wireBatch([["op": "props", "id": 12, "set": [
            "accessibilityLabel": "Read information", "accessibilitySelected": "true"
        ]]]))
        XCTAssertTrue(try cell(p, 12) === row)
        XCTAssertTrue(list.accessory(12) === button)
        XCTAssertEqual(row.accessibilityLabel, "Read information")
        XCTAssertTrue(row.accessibilityTraits.contains(.selected))
        _ = button.accessibilityLabel
        _ = button.accessibilityTraits
        button.isHighlighted = true
        row.updateConfiguration(using: row.configurationState)
        row.setNeedsLayout()
        row.layoutIfNeeded()
        XCTAssertTrue(list.accessory(12) === button)
        XCTAssertNotNil(button.window, "the contact's button remains in the window through the refresh")
        XCTAssertTrue(button.isHighlighted)
        button.isHighlighted = false

        detail.title = "Updated information"
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["accessibilitySelected": "false"],
                           "clear": ["accessibilityLabel"]]]))
        XCTAssertTrue(try cell(p, 12) === row)
        XCTAssertTrue(list.accessory(12) === button)
        XCTAssertEqual(row.accessibilityLabel, "Updated information")
        XCTAssertFalse(row.accessibilityTraits.contains(.selected))
        button.sendActions(for: .touchUpInside)
        XCTAssertEqual(presses, [13])
    }

    /// With UIKit's accessibility runtime, which reads a list cell's traits from its subviews and
    /// took the `checkmark.circle` symbol for a selection: the host's traits are the cell's.
    func testAStandardRowsNameAndSelectionComeFromItsParts() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit reads a cell's subviews into its traits only with its accessibility runtime") }
        var row = Row(view: 10, symbol: "checkmark.circle", title: "Milk", secondary: "Two bottles", pressable: true)
        let p = presenter {
            var m = self.model()
            m.sections[0].rows[0] = row
            return m
        }
        let milk = try cell(p, 10)
        XCTAssertEqual(milk.accessibilityLabel, "Milk, Two bottles")
        XCTAssertTrue(milk.accessibilityTraits.contains(.button))
        XCTAssertFalse(milk.accessibilityTraits.contains(.selected))
        row.accessory = "checkmark"
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["testId": "row10"]]]))
        XCTAssertTrue(try cell(p, 10).accessibilityTraits.contains(.selected))
        row.accessory = "none"
        row.secondary = nil
        row.pressable = false
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["accessibilityLabel": "Buy milk"]]]))
        let renamed = try cell(p, 10)
        XCTAssertEqual(renamed.accessibilityLabel, "Buy milk")
        XCTAssertFalse(renamed.accessibilityTraits.contains(.button))
        XCTAssertFalse(renamed.accessibilityTraits.contains(.selected))
    }

    /// UIKit's accessibility reads a list content view's hidden labels: a
    /// cell whose row drops its value must hold no label with the old one.
    func testACellWhoseRowDropsItsValueKeepsNoLabelOfIt() throws {
        var row = Row(view: 11, title: "Font", secondary: "Georgia", accessory: "disclosure", pressable: true)
        let p = presenter {
            var m = self.model()
            m.sections[0].rows[1] = row
            return m
        }
        defer { window.isHidden = true }
        XCTAssertTrue(labelText(in: try cell(p, 11)).contains("Georgia"))
        row.title = "Privacy Policy"
        row.secondary = nil
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["testId": "privacy"]]]))
        let after = try cell(p, 11)
        after.layoutIfNeeded()
        XCTAssertEqual(labelText(in: after), ["Privacy Policy"])
    }

    /// A toggle row is one element, as a hand-built cell with a switch for its accessory view is: the
    /// row's name with the switch's value and traits, and activating it flips the switch. On iOS 17
    /// the list cell left the switch a second element, which VoiceOver read with no name.
    func testAToggleRowIsOneElementNamedByItsRow() throws {
        var row = Row(view: 12, title: "Read Receipts", accessory: "toggle", target: 13)
        let p = presenter {
            var m = self.model()
            m.sections[0].rows[2] = row
            return m
        }
        defer { window.isHidden = true }
        var flips: [Bool] = []
        p.onChecked = { XCTAssertEqual($0, 13); flips.append($1) }
        let receipts = try cell(p, 12)
        let toggle = try XCTUnwrap(try list(p).accessory(12) as? UISwitch)
        XCTAssertTrue(receipts.isAccessibilityElement, "VoiceOver reads nothing inside an element")
        XCTAssertEqual(receipts.accessibilityLabel, "Read Receipts")
        XCTAssertEqual(receipts.accessibilityValue, toggle.accessibilityValue)
        XCTAssertTrue(receipts.accessibilityTraits.isSuperset(of: toggle.accessibilityTraits))

        // The switch's own name where the row has none, and the row's where it has one.
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["accessibilityLabel": "Send read receipts"]]]))
        XCTAssertEqual(receipts.accessibilityLabel, "Send read receipts")
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["accessibilityLabel": "Receipts"]]]))
        XCTAssertEqual(receipts.accessibilityLabel, "Receipts")

        XCTAssertTrue(receipts.accessibilityActivate())
        XCTAssertEqual(flips, [false])
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["checked": "false"]]]))
        XCTAssertFalse(toggle.isOn)
        XCTAssertEqual(receipts.accessibilityValue, toggle.accessibilityValue)

        // A disabled control dims the row, which then flips nothing.
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["disabled": "true"]]]))
        XCTAssertTrue(receipts.accessibilityTraits.contains(.notEnabled))
        XCTAssertTrue(receipts.accessibilityActivate())
        XCTAssertEqual(flips, [false])
        p.apply(wireBatch([["op": "props", "id": 13, "clear": ["disabled"]]]))
        XCTAssertFalse(receipts.accessibilityTraits.contains(.notEnabled))
        // A disabled row dims it by itself, and keeps it dimmed when its control is disabled too.
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["disabled": "true"]]]))
        XCTAssertTrue(receipts.accessibilityTraits.contains(.notEnabled))
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["disabled": "true"]]]))
        p.apply(wireBatch([["op": "props", "id": 13, "clear": ["disabled"]]]))
        XCTAssertTrue(receipts.accessibilityTraits.contains(.notEnabled))
        p.apply(wireBatch([["op": "props", "id": 12, "clear": ["disabled"]]]))
        XCTAssertFalse(receipts.accessibilityTraits.contains(.notEnabled))

        // A row that is no longer a toggle keeps none of the switch's.
        let switchTraits = toggle.accessibilityTraits
        row.accessory = "none"
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["testId": "receipts"]]]))
        let plain = try cell(p, 12)
        XCTAssertNil(plain.accessibilityValue)
        XCTAssertTrue(plain.accessibilityTraits.intersection(switchTraits).subtracting(.notEnabled).isEmpty)
        XCTAssertEqual(plain.accessibilityLabel, "Receipts")
    }

    /// What VoiceOver reads of a toggle row, which UIKit says only with its accessibility runtime:
    /// one switch, named by the row and checked as its control is; `tree --ax` joins it to the control.
    func testAToggleRowReadsAsOneSwitch() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's switch traits and value need its accessibility runtime") }
        let p = presenter { self.model() }
        defer { window.isHidden = true }
        let receipts = try cell(p, 12)
        let toggle = try XCTUnwrap(try list(p).accessory(12) as? UISwitch)
        XCTAssertEqual(receipts.accessibilityValue, "1")
        XCTAssertTrue(toggle.accessibilityTraits.contains(.toggleButton))
        XCTAssertTrue(receipts.accessibilityTraits.contains(.toggleButton))
        let elements = p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []
        XCTAssertFalse(elements.contains { ($0["native"] as? [String: Any])?["class"] as? String == "UISwitch" }, "no second element")
        let element = try XCTUnwrap(elements.first { $0["testId"] as? String == "toggle" })
        XCTAssertEqual(element["name"] as? String, "Read Receipts")
        XCTAssertEqual(element["role"] as? String, "switch")
        XCTAssertEqual((element["states"] as? [String: Any])?["checked"] as? Bool, true)
    }

    /// Where a grouped list shows its authored nodes, as on tvOS, which reads no model
    /// (`Runtime.groupedList`), a toggle row's switch is the row's element: named by the row's
    /// label, else its own, else the row's words, and saying its state as UIKit's toggle.
    func testAnAuthoredToggleRowsSwitchIsNamedByItsRow() throws {
        let p = presenter { nil }
        defer { window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 14, "kind": "text", "props": ["text": "Read Receipts"], "style": [:]],
            ["op": "children", "id": 12, "ids": [14, 13]],
        ]))
        let toggle = try XCTUnwrap(p.controls.controls[13])
        XCTAssertNil(host(p).lists[1], "no UIKit list: its authored rows show")
        XCTAssertEqual(toggle.accessibilityLabel, "Read Receipts")
        if Presenter.axRuntimeLoaded { XCTAssertEqual(toggle.accessibilityValue, "1") }
        p.apply(wireBatch([["op": "props", "id": 14, "set": ["text": "Receipts sent"]]]))
        XCTAssertEqual(toggle.accessibilityLabel, "Receipts sent", "the row's words as they now are")
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["accessibilityLabel": "Send read receipts"]]]))
        XCTAssertEqual(toggle.accessibilityLabel, "Send read receipts")
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["accessibilityLabel": "Receipts"]]]))
        XCTAssertEqual(toggle.accessibilityLabel, "Receipts")
        // A switch that is not its row's last part is its own.
        p.apply(wireBatch([["op": "props", "id": 13, "clear": ["accessibilityLabel"]], ["op": "children", "id": 12, "ids": [13, 14]]]))
        XCTAssertNil(toggle.accessibilityLabel)
    }

    /// Every label's text in a view, hidden ones too.
    private func labelText(in view: UIView) -> [String] {
        view.subviews.flatMap { ($0 as? UILabel).map { [$0.text ?? ""] } ?? labelText(in: $0) }.filter { !$0.isEmpty }
    }
}
#endif
