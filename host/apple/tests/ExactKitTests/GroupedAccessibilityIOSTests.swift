#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists
import ExactMarkdown

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

    /// A section UIKit draws is hidden by the host, not by its author: a live region that holds the list
    /// still says its rows' words, and a name from content still reads them.
    func testALiveRegionAroundAListSaysTheRowsUIKitDraws() throws {
        let p = presenter { self.model() }
        p.apply(wireBatch([
            ["op": "create", "id": 40, "kind": "view", "props": ["accessibilityLive": "polite"], "handlers": [], "style": [:]],
            ["op": "create", "id": 41, "kind": "text", "props": ["text": "Profile"], "handlers": [], "style": [:]],
            ["op": "create", "id": 42, "kind": "text", "props": ["text": "Notifications"], "handlers": [], "style": [:]],
            ["op": "children", "id": 10, "ids": [41]], ["op": "children", "id": 11, "ids": [42]],
            ["op": "children", "id": 40, "ids": [1]], ["op": "roots", "ids": [40]],
            ["op": "frame", "id": 40, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0],
        ]))
        _ = try cell(p, 10)
        p.syncAccessibility()
        let live = try XCTUnwrap(p.views[40])
        XCTAssertTrue(try XCTUnwrap(p.views[2]).isHidden, "UIKit draws the section")
        XCTAssertEqual(live.liveText, "Profile Notifications")
        XCTAssertEqual(live.accessibleName, "Profile Notifications")
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

    private func markdownRow(accessory: String, target: UInt32?, accessoryView: UInt32?, checked: Bool = false) throws -> Row {
        var row: [String: Any] = ["view": 12, "custom": true, "accessory": accessory, "checked": checked]
        row["target"] = target
        row["accessoryView"] = accessoryView
        let json = try JSONSerialization.data(withJSONObject: ["sections": [["view": 2, "rows": [row]]]])
        return try XCTUnwrap(GroupedListModel(json: json)?.sections.first?.rows.first)
    }

    func testAMarkdownToggleKeepsOneAccessibleAccessoryAndRestoresItsControl() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "the Markdown toggle must be read through UIKit's accessibility runtime") }
        ExactMarkdown.install()
        var row = try markdownRow(accessory: "toggle", target: 13, accessoryView: 13)
        let p = presenter {
            var m = self.model()
            m.sections[0].rows[2] = row
            return m
        }
        defer { window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 14, "kind": "text", "props": ["text": "Read **Receipts**", "markup": "markdown"], "style": [:]],
            ["op": "children", "id": 12, "ids": [14, 13]],
            ["op": "frame", "id": 12, "x": 0.0, "y": 0.0, "w": 354.0, "h": 60.0],
        ]))
        let receipts = try XCTUnwrap(try cell(p, 12) as? GroupedCell)
        let original = try XCTUnwrap(p.views[13])
        let title = try XCTUnwrap(p.views[14])
        let toggle = try XCTUnwrap(try list(p).accessory(12) as? UISwitch)
        XCTAssertTrue(title.isDescendant(of: receipts.contentView))
        XCTAssertNil(receipts.contentConfiguration, "no standard-cell string replaces the carried text")
        XCTAssertTrue(original.isHidden, "only UIKit's accessory switch is visible")
        XCTAssertTrue(receipts.toggle === toggle)
        XCTAssertTrue(receipts.isAccessibilityElement)
        XCTAssertEqual(receipts.accessibilityLabel, "Read Receipts")
        XCTAssertEqual(receipts.accessibilityValue, toggle.accessibilityValue)
        XCTAssertEqual(receipts.inspectionViewId, 13)
        let elements = p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []
        let switches = elements.filter { $0["testId"] as? String == "toggle" }
        XCTAssertEqual(switches.count, 1)
        XCTAssertEqual(switches.first?["name"] as? String, "Read Receipts")
        XCTAssertEqual(switches.first?["role"] as? String, "switch")
        XCTAssertEqual((switches.first?["states"] as? [String: Any])?["checked"] as? Bool, true)
        XCTAssertFalse(elements.contains { ($0["native"] as? [String: Any])?["class"] as? String == "UISwitch" })
        XCTAssertTrue(host(p).draws(13), "the agent targets the accessory, not the hidden control")
        var flips: [Bool] = []
        p.onChecked = { XCTAssertEqual($0, 13); flips.append($1) }
        XCTAssertTrue(receipts.accessibilityActivate())
        XCTAssertEqual(flips, [false])
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["checked": "false"]],
                           ["op": "props", "id": 14, "set": ["text": "Send **Receipts**"]]]))
        XCTAssertFalse(toggle.isOn)
        XCTAssertTrue(original.isHidden)
        XCTAssertEqual(receipts.accessibilityLabel, "Send Receipts", "a carried text can change without changing the row model")
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["disabled": "true"]]]))
        XCTAssertFalse(toggle.isEnabled)
        XCTAssertTrue(receipts.accessibilityActivate())
        XCTAssertEqual(flips, [false])

        row.accessory = "none"
        row.target = nil
        row = try markdownRow(accessory: row.accessory, target: row.target, accessoryView: nil)
        p.apply(wireBatch([["op": "props", "id": 13, "clear": ["disabled"]]]))
        let custom = try XCTUnwrap(try cell(p, 12) as? GroupedCell)
        XCTAssertNil(custom.toggle)
        XCTAssertTrue(custom.accessories.isEmpty)
        XCTAssertFalse(original.isHidden, "an ordinary custom row draws its authored control again")
        XCTAssertFalse(host(p).draws(13))
    }

    func testMarkdownRowsKeepStandardAccessoriesAndCarriedValues() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "read the carried radio through UIKit's accessibility runtime") }
        ExactMarkdown.install()
        for accessory in ["radio", "disclosure", "checkmark", "detail"] {
            let target: UInt32? = ["radio", "detail"].contains(accessory) ? 30 : nil
            var row = try markdownRow(accessory: accessory, target: target, accessoryView: 30, checked: accessory == "radio")
            let p = presenter {
                var m = self.model()
                m.sections[0].rows[2] = row
                return m
            }
            defer { window.isHidden = true }
            let kind = accessory == "radio" ? "control" : accessory == "detail" ? "button" : "image"
            p.apply(wireBatch([
                ["op": "create", "id": 14, "kind": "text", "props": ["text": "**Home**", "markup": "markdown"], "style": [:]],
                ["op": "create", "id": 15, "kind": "text", "props": ["text": "*Private*", "markup": "markdown"], "style": [:]],
                ["op": "create", "id": 30, "kind": kind, "props": ["type": "radio", "name": "network", "value": "home", "checked": "true", "testId": "accessory"], "handlers": accessory == "detail" ? ["press"] : ["input", "change"], "style": [:]],
                ["op": "children", "id": 12, "ids": [14, 15, 30]],
                ["op": "frame", "id": 12, "x": 0.0, "y": 0.0, "w": 354.0, "h": 60.0],
            ]))
            let list = try list(p), original = try XCTUnwrap(p.views[30])
            let native = try cell(p, 12)
            XCTAssertNil(native.contentConfiguration, "no standard-cell string replaces the carried text")
            for id: UInt32 in [14, 15] {
                XCTAssertTrue(try XCTUnwrap(p.views[id]).isDescendant(of: native.contentView), "title and value keep their text views")
            }
            XCTAssertTrue(original.isHidden, accessory)
            if accessory == "detail" {
                XCTAssertEqual(native.accessories.count, 1)
                if let detail = native.accessories.first, case .customView(let view) = detail.accessoryType {
                    XCTAssertTrue(view is UIButton)
                } else { XCTFail("the detail accessory is UIKit's button") }
            } else {
                let expected: UICellAccessory.AccessoryType = accessory == "disclosure" ? .disclosureIndicator : .checkmark
                XCTAssertEqual(native.accessories.map(\.accessoryType), [expected], accessory)
            }
            XCTAssertEqual(native.accessibilityLabel, "Home, Private", "as a standard cell reads its title and value")
            p.views[15]?.isHidden = true
            list.interact(native, 12)
            XCTAssertEqual(native.accessibilityLabel, "Home", "an isHidden text names nothing")
            p.views[15]?.isHidden = false
            list.interact(native, 12)
            if accessory == "radio" {
                XCTAssertTrue(native.isAccessibilityElement)
                XCTAssertTrue(native.accessibilityTraits.contains(.selected))
                let elements = p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []
                XCTAssertFalse(elements.contains { $0["testId"] as? String == "accessory" }, "no second stop on the authored radio")
                row.checked = false
                p.apply(wireBatch([["op": "props", "id": 30, "set": ["checked": "false"]]]))
                XCTAssertTrue(try cell(p, 12).accessories.isEmpty)
                XCTAssertFalse(try cell(p, 12).accessibilityTraits.contains(.selected))
                var checked: [UInt32] = []
                p.onControlValue = { id, _, input, _ in if input { checked.append(id) } }
                XCTAssertTrue(list.select(12))
                XCTAssertEqual(checked, [30])
                XCTAssertNotNil(host(p).activate(original))
                XCTAssertEqual(checked, [30, 30], "the hidden radio's agent target activates the cell")
            } else if accessory == "detail" {
                var pressed: [UInt32] = []
                p.onPress = { pressed.append($0) }
                let button = try XCTUnwrap(list.accessory(12) as? UIButton)
                button.sendActions(for: .touchUpInside)
                XCTAssertEqual(pressed, [30])
                XCTAssertNotNil(host(p).activate(original))
                XCTAssertEqual(pressed, [30, 30])
            }
            row = try markdownRow(accessory: accessory == "radio" ? "radio" : "none",
                                  target: accessory == "radio" ? target : nil, accessoryView: nil)
            p.apply(wireBatch([["op": "props", "id": 12, "set": ["testId": "authored-box"]]]))
            XCTAssertTrue(try cell(p, 12).accessories.isEmpty)
            XCTAssertFalse(original.isHidden, "an authored box restores its own accessory")
            XCTAssertFalse(host(p).draws(30))
        }
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
